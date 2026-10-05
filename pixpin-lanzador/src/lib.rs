//! **`pixpin-lanzador.exe`**: PixPin Max desde Flow Launcher (`p`).
//!
//! - Con un JSON como argumento es el plugin de Flow tal como lo usa la 2.1.4
//!   (`Executable`, un proceso por tecla: [`rpc::una_vez`]). Para que cada
//!   tecla no relea todos los proyectos hay una cache en disco
//!   (`<raiz>/cache/lanzador-indice.json`, ver [`datos`]).
//! - Sin argumentos es el plugin de Flow que se queda vivo (`Executable_V2`:
//!   JSON-RPC por stdin/stdout, ver [`rpc`]). **Flow 2.1.4 no lo usa**: su
//!   `PluginsLoader.ExecutableV2Plugins` crea un `ExecutablePlugin` (el v1)
//!   aunque `plugin.json` diga `Executable_V2`, y la clase
//!   `ExecutablePluginV2` no se instancia nunca (comprobado en el codigo de la
//!   etiqueta v2.1.4 y en `dev`, 2026-10-03). Se queda listo para cuando lo
//!   arreglen.
//! - `buscar "<texto>"` escribe los resultados, una linea JSON cada uno.
//! - `pedido '<json>'` le manda un pedido a la app (docs/protocolo-pedidos.md).
//!
//! Buscar es leer el disco de PixPin directamente ([`datos`]), aunque la app
//! este cerrada; abrir, crear o marcar es pedirselo a la app ([`pedido`]),
//! que es la unica que escribe sus ficheros.

pub mod capturas;
pub mod chat;
pub mod consulta;
pub mod datos;
pub mod fecha;
pub mod iconos;
pub mod imagenes;
pub mod lecciones;
pub mod menu;
pub mod normalizar;
pub mod pedido;
pub mod resultados;
pub mod rpc;

#[cfg(test)]
mod pruebas;

use std::sync::Mutex;

/// La palabra clave del plugin (la de `plugin/plugin.json`). Cuando Flow
/// manda la que se uso (`actionKeyword`, en el modo v2) vale esa.
pub const PALABRA_CLAVE: &str = "p";

static REGISTRO_DE_PANICOS: Mutex<Option<rpc::Registro>> = Mutex::new(None);

/// Que un panico vaya al registro y no a stderr (Flow tomaria cualquier cosa
/// en stderr por un fallo del plugin).
pub fn poner_registro_de_panicos(registro: rpc::Registro) {
    if let Ok(mut r) = REGISTRO_DE_PANICOS.lock() {
        *r = Some(registro);
    }
}

fn silenciar_panicos() {
    std::panic::set_hook(Box::new(|info| {
        if let Ok(r) = REGISTRO_DE_PANICOS.lock() {
            if let Some(r) = r.as_ref() {
                r.escribir(&format!("PANICO: {info}"));
            }
        }
    }));
}

/// El icono por defecto de los resultados: `Images\pixpin.png` junto al exe
/// (la carpeta del plugin) o, si no, el propio `pixpinmax.exe`.
fn icono_por_defecto() -> String {
    let junto = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("Images").join("pixpin.png")))
        .filter(|p| p.is_file());
    junto
        .or_else(datos::exe_instalado)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// El registro en el modo v1 (no llega `initialize` con la carpeta de
/// ajustes): junto al exe, en la carpeta del plugin.
fn raiz_del_registro() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("pixpin-lanzador.log")))
        .unwrap_or_else(|| std::env::temp_dir().join("pixpin-lanzador.log"))
}

/// Los datos de un proceso que contesta una sola consulta (el modo v1 de
/// Flow, `buscar`): sin vigia, y con la cache de disco
/// (`<raiz>/cache/lanzador-indice.json`), para que cada tecla no relea todos
/// los proyectos.
fn datos_de_un_proceso(raiz: std::path::PathBuf) -> datos::Datos {
    let mut d = datos::Datos::nuevo(raiz).con_cache_en_disco();
    d.vigilar = false;
    d
}

/// Lo que hace `main`. Devuelve el codigo de salida.
pub fn ejecutar(args: &[String]) -> i32 {
    let Some(raiz) = datos::raiz_de_datos() else {
        return 4;
    };
    match args.first().map(String::as_str) {
        None => {
            silenciar_panicos();
            let datos = datos::Datos::nuevo(raiz);
            let stdout = std::io::stdout().lock();
            let mut plugin =
                rpc::Plugin::nuevo(datos, pedido::Windows, stdout, icono_por_defecto());
            plugin.portapapeles = imagenes::WINDOWS;
            rpc::bucle(&mut plugin, std::io::stdin().lock());
            0
        }
        Some("buscar") => {
            let texto = args[1..].join(" ");
            let mut d = datos_de_un_proceso(raiz);
            let proyectos = d.proyectos();
            let mut ctx = resultados::Contexto::nuevo(PALABRA_CLAVE);
            ctx.iconos = Some(iconos::carpeta(d.raiz()));
            ctx.portapapeles = imagenes::WINDOWS;
            let r = resultados::resultados(&proyectos, &texto, &ctx);
            let icono = icono_por_defecto();
            let mut salida = String::new();
            for v in resultados::lista_json(&r, &icono) {
                salida.push_str(&v.to_string());
                salida.push('\n');
            }
            use std::io::Write;
            let _ = std::io::stdout().write_all(salida.as_bytes());
            d.guardar_cache();
            0
        }
        Some("pedido") => {
            let Some(json) = args.get(1) else {
                eprintln!("uso: pixpin-lanzador pedido '<json>'");
                return 3;
            };
            if serde_json::from_str::<serde_json::Value>(json).is_err() {
                eprintln!("el pedido no es JSON");
                return 3;
            }
            let e = pedido::enviar(json);
            if e != pedido::Envio::Aceptado {
                eprintln!("{}", e.explicacion());
            }
            e.codigo_de_salida()
        }
        // El modo v1 de Flow (`Executable`): la peticion en JSON como
        // argumento, la respuesta por stdout (ver `rpc::una_vez`).
        Some(primero) if primero.trim_start().starts_with('{') => {
            silenciar_panicos();
            let Ok(peticion) = serde_json::from_str::<serde_json::Value>(primero) else {
                return 3;
            };
            let datos = datos_de_un_proceso(raiz.clone());
            let mut plugin =
                rpc::Plugin::nuevo(datos, pedido::Windows, Vec::new(), icono_por_defecto());
            plugin.registro.ruta = Some(raiz_del_registro());
            plugin.diferir_avisos = true;
            plugin.portapapeles = imagenes::WINDOWS;
            let respuesta = rpc::una_vez(&mut plugin, &peticion);
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(respuesta.to_string().as_bytes());
            let _ = stdout.flush();
            drop(stdout);
            // Flow no lee la respuesta hasta que el proceso termina: lo que
            // queda tiene que ser corto. La cache para la tecla siguiente, y,
            // sin arrancar la app y con poca espera, que pinte los iconos que
            // faltaron (cada uno, una vez cada 30 s como mucho).
            plugin.datos.guardar_cache();
            plugin.mandar_avisos();
            0
        }
        Some(otro) => {
            eprintln!(
                "orden desconocida: {otro}\nuso: pixpin-lanzador [buscar <texto> | pedido <json>]"
            );
            3
        }
    }
}
