//! Mandarle un pedido a la PixPin que esta abierta (docs/protocolo-pedidos.md):
//! `WM_COPYDATA` con `dwData = 0x5049_5851` y el JSON en UTF-8, a la ventana
//! `PixPinMaxVentanaMensajes`. Si no esta, se arranca la app y se la espera.

use std::path::Path;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, HWND_MESSAGE, PostMessageW, SMTO_ABORTIFHUNG, SMTO_BLOCK,
    SendMessageTimeoutW, WM_COPYDATA, WM_NULL,
};
use windows::core::w;

/// El `dwData` de un pedido.
pub const PEDIDO_JSON: usize = 0x5049_5851;
/// Lo mas grande que se manda.
pub const MAXIMO: usize = 64 * 1024;
/// Cuanto se espera a que aparezca la ventana tras arrancar la app.
pub const ESPERA_ARRANQUE: Duration = Duration::from_secs(8);

/// Lo que puede salir de mandar un pedido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Envio {
    /// La app lo acepto (`1`).
    Aceptado,
    /// `2`: version no soportada.
    VersionNoSoportada,
    /// `3`: JSON roto o accion desconocida.
    NoEntendido,
    /// La app no esta y no se pudo arrancar (o no aparecio a tiempo).
    SinApp(String),
    /// Otra respuesta (`0`, o no contesto a tiempo).
    Otro(isize),
}

impl Envio {
    /// El codigo de salida de `pixpin-lanzador pedido`.
    pub fn codigo_de_salida(&self) -> i32 {
        match self {
            Envio::Aceptado => 0,
            Envio::VersionNoSoportada => 2,
            Envio::NoEntendido => 3,
            Envio::SinApp(_) => 4,
            Envio::Otro(_) => 5,
        }
    }

    /// Lo que se le dice al usuario si fue mal.
    pub fn explicacion(&self) -> String {
        match self {
            Envio::Aceptado => "Hecho".into(),
            Envio::VersionNoSoportada => {
                "Esta PixPin no entiende la version del pedido: actualizala".into()
            }
            Envio::NoEntendido => "Esta PixPin no entiende el pedido: actualizala".into(),
            Envio::SinApp(m) => format!("No se pudo abrir PixPin Max: {m}"),
            Envio::Otro(r) => format!("PixPin no contesto (respuesta {r})"),
        }
    }
}

/// Quien manda los pedidos: la app de verdad, o una de mentira en las pruebas.
pub trait Mensajero {
    fn enviar(&mut self, json: &str) -> Envio;

    /// Un pedido que no corre prisa (los `iconos`): solo si la app ya esta
    /// abierta, sin arrancarla y sin esperar mas que un momento.
    fn avisar(&mut self, json: &str) -> Envio {
        let _ = json;
        Envio::Otro(0)
    }

    /// Abrir un fichero con su programa de Windows. `false` si no se pudo.
    fn abrir_con_windows(&mut self, ruta: &str) -> bool {
        let _ = ruta;
        false
    }
}

/// El de verdad.
pub struct Windows;

impl Mensajero for Windows {
    fn enviar(&mut self, json: &str) -> Envio {
        enviar(json)
    }

    fn avisar(&mut self, json: &str) -> Envio {
        avisar(json)
    }

    fn abrir_con_windows(&mut self, ruta: &str) -> bool {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        if !Path::new(ruta).is_file() {
            return false;
        }
        // El Explorador abre con el programa asociado (o pregunta con cual),
        // y vive fuera del Job de Flow.
        Command::new("explorer.exe")
            .arg(ruta)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(DETACHED_PROCESS)
            .spawn()
            .is_ok()
    }
}

/// Cuanto se espera, como mucho, a que la app conteste un aviso.
pub const ESPERA_AVISO_MS: u32 = 250;

/// Manda el pedido solo si PixPin ya esta abierta (nunca la arranca), con
/// poca espera: es para lo que no corre prisa.
pub fn avisar(json: &str) -> Envio {
    if json.len() > MAXIMO {
        return Envio::NoEntendido;
    }
    let Some(destino) = ventana() else {
        return Envio::SinApp("no esta abierta".into());
    };
    mandar(destino, json, ESPERA_AVISO_MS)
}

fn mandar(destino: HWND, json: &str, espera_ms: u32) -> Envio {
    let datos = json.as_bytes();
    let paquete = COPYDATASTRUCT {
        dwData: PEDIDO_JSON,
        cbData: datos.len() as u32,
        lpData: datos.as_ptr() as *mut _,
    };
    let mut respuesta: usize = 0;
    // SAFETY: SendMessageTimeoutW es sincrono: `datos` y `paquete` viven
    // durante toda la llamada, que es lo que exige WM_COPYDATA.
    let r = unsafe {
        SendMessageTimeoutW(
            destino,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(&paquete as *const _ as isize),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            espera_ms,
            Some(&mut respuesta),
        )
    };
    if r.0 == 0 {
        return Envio::Otro(0);
    }
    // Un toque a la cola para despertar su bucle (WM_COPYDATA no pasa por ella).
    // SAFETY: mensaje sin punteros.
    let _ = unsafe { PostMessageW(Some(destino), WM_NULL, WPARAM(0), LPARAM(0)) };
    match respuesta as isize {
        1 => Envio::Aceptado,
        2 => Envio::VersionNoSoportada,
        3 => Envio::NoEntendido,
        otro => Envio::Otro(otro),
    }
}

fn ventana() -> Option<HWND> {
    // SAFETY: la clase es un literal estatico terminado en cero; si no hay
    // ninguna ventana devuelve error o nula, y se descarta.
    let h = unsafe { FindWindowW(w!("PixPinMaxVentanaMensajes"), None) };
    if let Ok(h) = h {
        if !h.0.is_null() {
            return Some(h);
        }
    }
    // Por si es de solo mensajes (`HWND_MESSAGE`), que FindWindowW no ve.
    // SAFETY: igual que arriba.
    let h = unsafe {
        FindWindowExW(
            Some(HWND_MESSAGE),
            None,
            w!("PixPinMaxVentanaMensajes"),
            None,
        )
    };
    match h {
        Ok(h) if !h.0.is_null() => Some(h),
        _ => None,
    }
}

/// Arranca `pixpinmax.exe` fuera del Job de Flow (si no, Flow se la llevaria
/// por delante al cerrarse) y sin heredar sus tuberias.
fn arrancar(exe: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    if !exe.is_file() {
        return Err(format!("no esta {}", exe.display()));
    }
    let preparar = |c: &mut Command| {
        c.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(d) = exe.parent() {
            c.current_dir(d);
        }
    };
    let mut c = Command::new(exe);
    preparar(&mut c);
    c.creation_flags(CREATE_BREAKAWAY_FROM_JOB | DETACHED_PROCESS);
    if c.spawn().is_ok() {
        return Ok(());
    }
    // El Job no deja salirse: que la arranque el Explorador, que no esta en el.
    let mut c = Command::new("explorer.exe");
    c.arg(exe);
    preparar(&mut c);
    c.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Manda el pedido, arrancando PixPin si hace falta.
pub fn enviar(json: &str) -> Envio {
    if json.len() > MAXIMO {
        return Envio::NoEntendido;
    }
    // Que lo que abra PixPin pueda pasar al frente: Flow (que arranco este
    // proceso) tiene el foco ahora, y sin este permiso Windows deja la
    // ventana nueva detras (o minimizada) y solo hace parpadear su boton.
    // SAFETY: sin punteros; falla sin efecto si no se puede dar.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(u32::MAX);
    }
    let destino = match ventana() {
        Some(h) => h,
        None => {
            let Some(exe) = crate::datos::exe_instalado() else {
                return Envio::SinApp("no hay LOCALAPPDATA".into());
            };
            if let Err(e) = arrancar(&exe) {
                return Envio::SinApp(e);
            }
            let inicio = Instant::now();
            loop {
                std::thread::sleep(Duration::from_millis(100));
                if let Some(h) = ventana() {
                    // Que la app acabe de arrancar su bucle antes del pedido.
                    std::thread::sleep(Duration::from_millis(300));
                    break h;
                }
                if inicio.elapsed() > ESPERA_ARRANQUE {
                    return Envio::SinApp("no aparecio en 8 s".into());
                }
            }
        }
    };
    mandar(destino, json, 5000)
}

/// Si PixPin esta abierta (su ventana de mensajes existe). Rapido: no le
/// manda nada.
pub fn app_abierta() -> bool {
    ventana().is_some()
}
