//! Leer el disco de PixPin, solo leer (docs/protocolo-pedidos.md, «Donde
//! leer»), con una cache en memoria que se invalida por la fecha de los
//! ficheros.
//!
//! Los structs son propios y de solo lectura: solo los campos que hacen falta
//! para buscar y abrir, todos con `#[serde(default)]`, de modo que un campo
//! nuevo de la app o del movil no rompe nada. Nunca se escribe aqui: para
//! cambiar algo se le pide a la app.

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// El fichero de ajustes cuya presencia junto al exe activa el modo portable
/// (`pixpin_store::rutas`).
const NOMBRE_AJUSTES: &str = "pixpinmax.toml";
/// Lo que ensena la app para el proyecto de `guardados: true`
/// (`almacen::NOMBRE_GUARDADOS`), se llame como se llame en el indice.
pub const NOMBRE_GUARDADOS: &str = "Mensajes guardados";
/// Prefijo de las rutas que llegan del movil.
const PORTATIL: &str = "pixpin:files/";

/// Donde esta `pixpinmax.exe` instalado (`herramientas/instalar.ps1`).
pub fn exe_instalado() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(local)
            .join("Programs")
            .join("PixPinMax")
            .join("pixpinmax.exe"),
    )
}

/// La raiz de datos: la carpeta del exe instalado si a su lado hay un
/// `pixpinmax.toml` (portable), si no `%APPDATA%\PixPinMax`. Es la regla de
/// `pixpin_store::rutas::resolver`.
pub fn raiz_de_datos() -> Option<PathBuf> {
    if let Some(dir) = exe_instalado().as_deref().and_then(Path::parent) {
        if dir.join(NOMBRE_AJUSTES).is_file() {
            return Some(dir.to_path_buf());
        }
    }
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("PixPinMax"))
}

// --- Lo que se lee -------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct IndiceCrudo {
    proyectos: Vec<ProyectoCrudo>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct ProyectoCrudo {
    id: String,
    nombre: String,
    tocado: i64,
    hojas: u32,
    guardados: bool,
    papelera: bool,
    #[serde(rename = "enPapelera")]
    en_papelera: bool,
}

/// Un mensaje de `guardados.jsonl`, lo justo para buscar y abrir.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Mensaje {
    pub id: String,
    pub cuando: i64,
    pub clase: String,
    pub texto: Option<String>,
    pub nombre: Option<String>,
    pub ruta: Option<String>,
    pub referencia: Option<String>,
    pub miniapp: Option<String>,
    pub transcripcion: Option<String>,
    /// Lo que dura un audio (el movil lo escribe como numero; `null` vale).
    #[serde(rename = "duracionMs")]
    pub duracion_ms: Option<f64>,
    #[serde(rename = "enBuzon")]
    pub en_buzon: bool,
    /// El id del mensaje al que responde: las fotos y audios de una leccion
    /// responden al suyo (`lec-<id>`).
    #[serde(rename = "respondeA")]
    pub responde_a: Option<String>,
}

impl Mensaje {
    /// Si lleva una leccion aprendida (`LeccionesStore.esLeccion`): un
    /// `ARCHIVO` cuya ruta acaba en `.leccion`.
    pub fn es_leccion(&self) -> bool {
        self.clase == "ARCHIVO"
            && self
                .ruta
                .as_deref()
                .is_some_and(pixpin_lecciones::leccion::es_ruta_de_leccion)
    }
    /// Si es una leccion o una foto o audio suyo: lo que el chat no ensena
    /// (`LeccionesStore.sinLecciones`).
    pub fn es_de_leccion(&self) -> bool {
        self.es_leccion()
            || self
                .responde_a
                .as_deref()
                .is_some_and(|r| r.starts_with(pixpin_lecciones::leccion::PREFIJO))
    }
    pub fn texto(&self) -> &str {
        self.texto.as_deref().unwrap_or("")
    }
    pub fn nombre(&self) -> &str {
        self.nombre.as_deref().unwrap_or("").trim()
    }
    pub fn es_lista_de_tareas(&self) -> bool {
        self.clase == "MINIAPP" && self.miniapp.as_deref() == Some("tareas")
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ProyectoJson {
    hojas: Vec<Hoja>,
}

/// Una hoja de `proyecto.json`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Hoja {
    pub id: String,
    pub nombre: String,
    pub dibujo: Option<String>,
}

/// Una nota `.md` de `notas/`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotaMd {
    pub ruta: PathBuf,
    pub titulo: String,
    pub cuando: i64,
}

/// Lo que se sabe de un proyecto.
#[derive(Debug, Clone, Default)]
pub struct Proyecto {
    pub id: String,
    /// El que se ensena (`Mensajes guardados` para el de guardados).
    pub nombre: String,
    pub tocado: i64,
    pub hojas: u32,
    pub guardados: bool,
    pub carpeta: PathBuf,
    pub mensajes: Vec<Mensaje>,
    pub hojas_json: Vec<Hoja>,
    pub notas_md: Vec<NotaMd>,
    /// Las lecciones aprendidas de su chat, ya leidas de su `.leccion`.
    pub lecciones: Vec<LeccionLeida>,
}

/// Una leccion aprendida tal como esta en el disco: el mensaje que la
/// senala, su fichero `.leccion` y lo que dice (el JSON entero: lo entiende
/// `pixpin_lecciones::Leccion::de_valor`, la misma lectura que la app).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LeccionLeida {
    pub mensaje: String,
    pub ruta: PathBuf,
    pub json: serde_json::Value,
}

/// Donde esta en la carpeta de un chat lo que senala la `ruta` de un mensaje
/// de leccion: como [`Proyecto::ruta_real`] y, ademas, la ruta absoluta del
/// movil de un `.pixpin` viejo, por lo que va detras de `files/` (como
/// `lecciones::almacen::archivo_de` de la app).
pub fn ruta_de_leccion(carpeta: &Path, ruta: &str) -> Option<PathBuf> {
    match ruta.split_once("/files/") {
        Some((_, rel)) if !ruta.starts_with(PORTATIL) => {
            ruta_en(carpeta, &format!("{PORTATIL}{rel}"))
        }
        _ => ruta_en(carpeta, ruta),
    }
}

fn ruta_en(carpeta: &Path, ruta: &str) -> Option<PathBuf> {
    let ruta = ruta.trim();
    if ruta.is_empty() {
        return None;
    }
    let (base, rel) = match ruta.strip_prefix(PORTATIL) {
        // Lo nacido en este PC y enlazado desde un texto (las imagenes
        // de una tarea): `guardados/pc/<chat>/<ruta>`, en su propia
        // carpeta (`pixpin_proyecto::vista`, `Disco::ruta`).
        Some(rel) if rel.starts_with("guardados/pc/") => {
            match rel["guardados/pc/".len()..].split_once('/') {
                Some((_, resto)) => (carpeta.to_path_buf(), resto),
                None => return None,
            }
        }
        Some(rel) => (carpeta.join("android"), rel),
        None => {
            if Path::new(ruta).is_absolute() || ruta.starts_with('/') || ruta.contains(':') {
                return None;
            }
            (carpeta.to_path_buf(), ruta)
        }
    };
    let mut p = base;
    for trozo in rel.split(['/', '\\']) {
        match trozo {
            "" | "." => {}
            ".." => return None,
            t => p.push(t),
        }
    }
    Some(p)
}

impl Proyecto {
    /// El `proyecto` que va en un pedido: `null` es «Mensajes guardados».
    pub fn id_para_pedido(&self) -> serde_json::Value {
        if self.guardados {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(self.id.clone())
        }
    }

    /// Donde esta en este equipo lo que senala la `ruta` de un mensaje:
    /// relativa a la carpeta del proyecto, o `pixpin:files/…` (lo que llego
    /// del movil, que vive en `android/…`). `None` si es de otro aparato o
    /// intenta salirse de la carpeta.
    pub fn ruta_real(&self, ruta: &str) -> Option<PathBuf> {
        ruta_en(&self.carpeta, ruta)
    }
}

// --- Las tareas ----------------------------------------------------------

/// Una casilla de una lista de tareas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tarea {
    /// Su numero entre las casillas, desde 0, en el orden del documento: el
    /// `indice` de `marcar_tarea`.
    pub indice: usize,
    pub texto: String,
    pub hecha: bool,
    /// El dia en que se creo (`➕ AAAA-MM-DD` al final del texto, como
    /// `pixpin_proyecto::mini::partir`), en dias desde 1970.
    pub creada: Option<i64>,
    /// Los enlaces de sus imagenes, en orden; en `texto` va cada una como
    /// `[img 01]` ([`sin_imagenes`]).
    pub imagenes: Vec<String>,
}

/// El texto de una tarea con sus imagenes (`![img 01](pixpin:files/…)`,
/// ver docs/investigacion/2026-10-03-tareas-con-imagenes-android.md) cambiadas
/// por su rotulo entre corchetes (`[img 01]`), y los enlaces en su orden. Lo
/// que no es un enlace bien formado (vacio, con blancos o `)`) se queda como
/// texto, como en `pixpin_proyecto::mini`.
pub fn sin_imagenes(texto: &str) -> (String, Vec<String>) {
    let mut fuera = String::new();
    let mut enlaces = Vec::new();
    let mut resto = texto;
    while let Some(i) = resto.find("![") {
        fuera.push_str(&resto[..i]);
        let tras = &resto[i + 2..];
        let pieza = tras.find("](").and_then(|c| {
            let alt = &tras[..c];
            let cola = &tras[c + 2..];
            let f = cola.find(')')?;
            let enlace = &cola[..f];
            (!alt.contains(']') && !enlace.is_empty() && !enlace.contains(char::is_whitespace))
                .then(|| (alt, enlace, c + 2 + f + 1))
        });
        match pieza {
            Some((alt, enlace, largo)) => {
                let alt = alt.trim();
                fuera.push('[');
                fuera.push_str(if alt.is_empty() { "img" } else { alt });
                fuera.push(']');
                enlaces.push(enlace.to_string());
                resto = &tras[largo..];
            }
            None => {
                fuera.push_str("![");
                resto = tras;
            }
        }
    }
    fuera.push_str(resto);
    (fuera, enlaces)
}

/// El titulo del documento: `^#{1,6}\s+(.*)$` en la primera linea, como
/// `pixpin_proyecto::mini::titulo` (y `MiniApps.kt`).
pub fn titulo_del_documento(documento: &str) -> String {
    let primera = documento.split('\n').next().unwrap_or("").trim();
    let almohadillas = primera.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&almohadillas) {
        return String::new();
    }
    let resto = &primera[almohadillas..];
    if !resto.starts_with([' ', '\t']) {
        return String::new();
    }
    resto.trim().to_string()
}

/// Las casillas de un documento, como `pixpin_proyecto::mini::leer_tareas`.
pub fn leer_tareas(documento: &str) -> Vec<Tarea> {
    documento
        .lines()
        .filter_map(casilla)
        .enumerate()
        .map(|(indice, (texto, hecha))| {
            let (texto, creada) = partir_fecha(&texto);
            let (texto, imagenes) = sin_imagenes(&texto);
            Tarea {
                indice,
                texto,
                hecha,
                creada,
                imagenes,
            }
        })
        .collect()
}

/// El texto de una tarea sin su fecha de creacion, y esa fecha en dias desde
/// 1970 (la regla de `pixpin_proyecto::mini::partir`: `➕ AAAA-MM-DD` al
/// final, separada por un blanco; si no, todo es texto).
pub fn partir_fecha(texto: &str) -> (String, Option<i64>) {
    let t = texto.trim_end();
    let entero = || (texto.trim().to_string(), None);
    if t.len() < 10 || !t.is_char_boundary(t.len() - 10) {
        return entero();
    }
    let (antes, fecha) = t.split_at(t.len() - 10);
    let b = fecha.as_bytes();
    let num = |r: &[u8]| -> Option<i64> {
        r.iter().try_fold(0i64, |a, c| {
            c.is_ascii_digit().then(|| a * 10 + i64::from(c - b'0'))
        })
    };
    if b[4] != b'-' || b[7] != b'-' {
        return entero();
    }
    let (Some(a), Some(m), Some(d)) = (num(&b[0..4]), num(&b[5..7]), num(&b[8..10])) else {
        return entero();
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return entero();
    }
    let Some(resto) = antes.trim_end().strip_suffix('\u{2795}') else {
        return entero();
    };
    if !resto.is_empty() && !resto.ends_with(char::is_whitespace) {
        return entero();
    }
    (resto.trim().to_string(), Some(dias_civiles(a, m, d)))
}

/// Dias desde 1970-01-01 de una fecha del calendario (algoritmo de Hinnant).
pub fn dias_civiles(a: i64, m: i64, d: i64) -> i64 {
    let a = if m <= 2 { a - 1 } else { a };
    let era = a.div_euclid(400);
    let ya = a - era * 400;
    let mp = (m + 9) % 12;
    let dy = (153 * mp + 2) / 5 + d - 1;
    let de = ya * 365 + ya / 4 - ya / 100 + dy;
    era * 146_097 + de - 719_468
}

/// «hoy», «hace 1 dia», «hace N dias» (como el chat, `mini::edad`).
pub fn hace_dias(dias: i64) -> String {
    match dias {
        i64::MIN..=0 => "hoy".into(),
        1 => "hace 1 día".into(),
        n => format!("hace {n} días"),
    }
}

/// `- [ ] texto` / `* [x] texto`: el texto y si esta hecha.
fn casilla(linea: &str) -> Option<(String, bool)> {
    let resto = linea.trim_start().strip_prefix(['-', '*', '+'])?;
    if !resto.starts_with([' ', '\t']) {
        return None;
    }
    let resto = resto.trim_start().strip_prefix('[')?;
    let marca = resto.chars().next()?;
    let resto = resto.get(marca.len_utf8()..)?.strip_prefix(']')?;
    let hecha = match marca {
        'x' | 'X' => true,
        ' ' => false,
        _ => return None,
    };
    Some((resto.trim().to_string(), hecha))
}

/// El documento con la casilla `indice` marcada o desmarcada (para ensenar
/// el cambio sin esperar a que la app lo escriba).
pub fn con_tarea_marcada(documento: &str, indice: usize, hecha: bool) -> String {
    let mut n = 0;
    let mut lineas: Vec<String> = Vec::new();
    for linea in documento.split('\n') {
        if casilla(linea).is_some() {
            if n == indice {
                if let Some(p) = linea.find('[') {
                    let mut l = linea.to_string();
                    l.replace_range(p + 1..p + 2, if hecha { "x" } else { " " });
                    lineas.push(l);
                    n += 1;
                    continue;
                }
            }
            n += 1;
        }
        lineas.push(linea.to_string());
    }
    lineas.join("\n")
}

/// El documento sin la casilla `indice` (para ensenar la tarea quitada sin
/// esperar a que la app lo escriba). Una que no existe lo deja igual.
pub fn con_tarea_quitada(documento: &str, indice: usize) -> String {
    let mut n = 0;
    documento
        .split('\n')
        .filter(|linea| {
            if casilla(linea).is_none() {
                return true;
            }
            n += 1;
            n - 1 != indice
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// El documento con una tarea pendiente mas al final.
pub fn con_tarea_anadida(documento: &str, texto: &str) -> String {
    let mut d = documento.trim_end_matches('\n').to_string();
    if !d.is_empty() {
        d.push('\n');
    }
    d.push_str("- [ ] ");
    d.push_str(texto.trim());
    d
}

// --- La cache ------------------------------------------------------------

fn fecha(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Cada cuanto se miran las fechas aunque el vigia no haya avisado: la red
/// por si Windows se pierde un aviso (en una unidad de red, por ejemplo).
pub const REVISION_FORZOSA: Duration = Duration::from_secs(2);

/// Un aviso de Windows (`FindFirstChangeNotificationW`) sobre toda la carpeta
/// `proyectos`: mientras no salte, nada cambio y no hace falta mirar ninguna
/// fecha. Preguntarle cuesta una llamada sin espera; mirar las fechas de todos
/// los proyectos, unos milisegundos (cada `metadata` pasa por el antivirus).
#[derive(Debug)]
struct Vigia(windows::Win32::Foundation::HANDLE);

// SAFETY: el HANDLE es de este proceso y solo se usa con las funciones de
// avisos de cambios, que valen desde cualquier hilo.
unsafe impl Send for Vigia {}

impl Vigia {
    fn nuevo(carpeta: &Path) -> Option<Vigia> {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Storage::FileSystem::{
            FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
            FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FindFirstChangeNotificationW,
        };
        let ancho: Vec<u16> = carpeta
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let filtro = FILE_NOTIFY_CHANGE_FILE_NAME
            | FILE_NOTIFY_CHANGE_DIR_NAME
            | FILE_NOTIFY_CHANGE_SIZE
            | FILE_NOTIFY_CHANGE_LAST_WRITE;
        // SAFETY: `ancho` acaba en cero y vive durante la llamada.
        let h = unsafe {
            FindFirstChangeNotificationW(windows::core::PCWSTR(ancho.as_ptr()), true, filtro)
        }
        .ok()?;
        if h.is_invalid() {
            return None;
        }
        Some(Vigia(h))
    }

    /// Si algo cambio desde la ultima vez. Lo rearma antes de contestar, de
    /// modo que un cambio que llegue mientras se relee salta la proxima vez.
    fn salto(&self) -> bool {
        use windows::Win32::Foundation::WAIT_OBJECT_0;
        use windows::Win32::Storage::FileSystem::FindNextChangeNotification;
        use windows::Win32::System::Threading::WaitForSingleObject;
        // SAFETY: el HANDLE es valido mientras viva el vigia.
        unsafe {
            if WaitForSingleObject(self.0, 0) != WAIT_OBJECT_0 {
                return false;
            }
            // Se vacian todos los avisos que esperan (rearmar con cambios ya
            // apuntados vuelve a saltar en el acto): una rafaga de cambios
            // es una sola revision.
            for _ in 0..32 {
                let _ = FindNextChangeNotification(self.0);
                if WaitForSingleObject(self.0, 0) != WAIT_OBJECT_0 {
                    break;
                }
            }
        }
        true
    }
}

impl Drop for Vigia {
    fn drop(&mut self) {
        // SAFETY: se cierra una sola vez, aqui.
        unsafe {
            let _ = windows::Win32::Storage::FileSystem::FindCloseChangeNotification(self.0);
        }
    }
}

/// Cuantas veces se miraron las fechas del disco (de cualquier `Datos`): la
/// cache de [`existe`] vale mientras no cambie.
static REVISIONES: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static EXISTENTES: RefCell<(u64, HashMap<PathBuf, bool>)> = RefCell::new((0, HashMap::new()));
}

/// `ruta.is_file()`, recordado hasta que [`Datos::proyectos`] vuelva a mirar
/// el disco (porque el vigia aviso de un cambio, o pasados
/// [`REVISION_FORZOSA`]). Para lo que se pregunta en cada consulta de cada
/// resultado (si el fichero de un mensaje sigue ahi, si ya esta su icono).
pub fn existe(ruta: &Path) -> bool {
    let generacion = REVISIONES.load(Ordering::Relaxed);
    EXISTENTES.with(|c| {
        let mut c = c.borrow_mut();
        if c.0 != generacion {
            c.0 = generacion;
            c.1.clear();
        }
        if let Some(&e) = c.1.get(ruta) {
            return e;
        }
        let e = ruta.is_file();
        c.1.insert(ruta.to_path_buf(), e);
        e
    })
}

fn ms(t: SystemTime) -> i64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Las fechas de los ficheros de los que sale un proyecto: si alguna cambia,
/// se relee.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
struct Fechas {
    guardados: Option<(SystemTime, u64)>,
    proyecto: Option<(SystemTime, u64)>,
    notas: Option<(SystemTime, u64)>,
    /// La carpeta de las lecciones (`android/guardados/lecciones`): la app
    /// y el movil reescriben el `.leccion` aparte y lo renombran, asi que
    /// repasar o editar una cambia esto aunque el chat no cambie.
    #[serde(default)]
    lecciones: Option<(SystemTime, u64)>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Entrada {
    fechas: Fechas,
    mensajes: Vec<Mensaje>,
    hojas: Vec<Hoja>,
    notas_md: Vec<NotaMd>,
    #[serde(default)]
    lecciones: Vec<LeccionLeida>,
}

/// Cuanto valen las respuestas de [`existe`] guardadas en la cache de disco
/// (un fichero borrado a mano no cambia ninguna fecha de las que se miran).
pub const VIDA_EXISTENTES_MS: i64 = 30_000;
/// Cuanto se espera antes de volver a pedir el mismo icono.
pub const REPETIR_ICONO_MS: i64 = 30_000;
/// La version del formato de la cache de disco: otra, y se tira.
const VERSION_CACHE: u32 = 2;

/// El nombre de la cache de disco, en `<raiz>/cache/`.
pub const NOMBRE_CACHE: &str = "lanzador-indice.json";

/// Lo que se guarda en disco entre un proceso y el siguiente (el modo de un
/// proceso por tecla, `Executable`): lo leido de cada proyecto con las fechas
/// y tamanos de sus ficheros. Cada tecla lee este fichero, lista las carpetas
/// y solo relee lo que cambio.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct CacheDisco {
    version: u32,
    raiz: PathBuf,
    fecha_indice: Option<(SystemTime, u64)>,
    indice: Vec<ProyectoCrudo>,
    cache: HashMap<String, Entrada>,
    /// Las respuestas de [`existe`], y desde cuando valen (ms).
    existentes: HashMap<PathBuf, bool>,
    existentes_desde: i64,
    /// Los iconos ya pedidos a la app, y cuando (ms).
    iconos_pedidos: HashMap<String, i64>,
}

/// Lo leido del disco, que se pone al dia solo con lo que cambio.
#[derive(Debug)]
pub struct Datos {
    raiz: PathBuf,
    fecha_indice: Option<(SystemTime, u64)>,
    indice: Vec<ProyectoCrudo>,
    cache: HashMap<String, Entrada>,
    /// Lo que devolvio la ultima vez [`Datos::proyectos`], mientras el vigia
    /// no avise de un cambio.
    ultima: Option<Vec<Proyecto>>,
    /// Cuando se miraron las fechas por ultima vez.
    revisado: Option<Instant>,
    vigia: Option<Vigia>,
    /// Si se pone un vigia sobre `proyectos` (en un proceso que vive una
    /// sola consulta no sirve de nada).
    pub vigilar: bool,
    /// Cada cuanto se miran las fechas aunque el vigia calle
    /// ([`REVISION_FORZOSA`]).
    pub revision_forzosa: Duration,
    /// Cuantas veces se miraron las fechas (para las pruebas).
    pub revisiones: u64,
    /// Cuantos proyectos se releyeron de verdad (para las pruebas).
    pub relecturas: u64,
    /// Donde se guarda la cache de disco ([`Datos::con_cache_en_disco`]).
    ruta_cache: Option<PathBuf>,
    /// Que hay algo nuevo que guardar en ella.
    sucio: bool,
    /// Que se adelanto algo que la app aun no escribio: eso no se guarda.
    retocado: bool,
    /// Las respuestas de [`existe`] que trajo la cache de disco, a la espera
    /// de saber si nada cambio.
    existentes_heredados: Option<(HashMap<PathBuf, bool>, i64)>,
    existentes_desde: i64,
    /// Cuantas respuestas de [`existe`] de la cache de disco siguen valiendo
    /// (ya estan en el fichero: no hace falta reescribirlo por ellas).
    existentes_en_disco: usize,
    iconos_pedidos: HashMap<String, i64>,
}

impl Datos {
    pub fn nuevo(raiz: PathBuf) -> Self {
        Datos {
            raiz,
            fecha_indice: None,
            indice: Vec::new(),
            cache: HashMap::new(),
            ultima: None,
            revisado: None,
            vigia: None,
            vigilar: true,
            revision_forzosa: REVISION_FORZOSA,
            revisiones: 0,
            relecturas: 0,
            ruta_cache: None,
            sucio: false,
            retocado: false,
            existentes_heredados: None,
            existentes_desde: 0,
            existentes_en_disco: 0,
            iconos_pedidos: HashMap::new(),
        }
    }

    /// Con la cache de disco de `<raiz>/cache/lanzador-indice.json`: se carga
    /// ahora y se guarda con [`Datos::guardar_cache`].
    pub fn con_cache_en_disco(self) -> Self {
        let ruta = self.raiz.join("cache").join(NOMBRE_CACHE);
        self.con_cache_en(ruta)
    }

    /// Lo mismo, con la cache en otra ruta.
    pub fn con_cache_en(mut self, ruta: PathBuf) -> Self {
        let leida = std::fs::read(&ruta)
            .ok()
            .and_then(|b| serde_json::from_slice::<CacheDisco>(&b).ok())
            .filter(|c| c.version == VERSION_CACHE && c.raiz == self.raiz);
        if let Some(c) = leida {
            self.fecha_indice = c.fecha_indice;
            self.indice = c.indice;
            self.cache = c.cache;
            self.iconos_pedidos = c.iconos_pedidos;
            if ahora_ms() - c.existentes_desde < VIDA_EXISTENTES_MS {
                self.existentes_heredados = Some((c.existentes, c.existentes_desde));
            }
        } else {
            self.sucio = true;
        }
        self.ruta_cache = Some(ruta);
        self
    }

    /// Guarda la cache de disco si hay algo nuevo (y nada adelantado que la
    /// app aun no escribio). De un tiron: se escribe aparte y se renombra.
    pub fn guardar_cache(&mut self) {
        let Some(ruta) = self.ruta_cache.clone() else {
            return;
        };
        if self.retocado {
            return;
        }
        let existentes = EXISTENTES.with(|c| {
            let c = c.borrow();
            if c.0 == REVISIONES.load(Ordering::Relaxed) {
                c.1.clone()
            } else {
                HashMap::new()
            }
        });
        // Lo que ya esta en el fichero no obliga a reescribirlo. (Antes se
        // comparaba con lo heredado DESPUES de que `proyectos` se lo llevara:
        // casi cada tecla reescribia la cache, y el proceso siguiente pagaba
        // ademas que el antivirus mirara el fichero recien escrito.)
        let heredados = self.existentes_en_disco.max(
            self.existentes_heredados
                .as_ref()
                .map(|h| h.0.len())
                .unwrap_or(0),
        );
        if !self.sucio && existentes.len() <= heredados {
            return;
        }
        let desde = if self.existentes_desde > 0 {
            self.existentes_desde
        } else {
            ahora_ms()
        };
        let cuantos = existentes.len();
        let c = CacheDisco {
            version: VERSION_CACHE,
            raiz: self.raiz.clone(),
            fecha_indice: self.fecha_indice,
            indice: self.indice.clone(),
            cache: std::mem::take(&mut self.cache),
            existentes,
            existentes_desde: desde,
            iconos_pedidos: self.iconos_pedidos.clone(),
        };
        let bytes = serde_json::to_vec(&c);
        self.cache = c.cache;
        let Ok(bytes) = bytes else { return };
        if let Some(dir) = ruta.parent() {
            let _ = std::fs::create_dir_all(dir);
            limpiar_temporales(dir);
        }
        let aparte = ruta.with_extension(format!("{}.tmp", std::process::id()));
        if std::fs::write(&aparte, bytes).is_ok() && std::fs::rename(&aparte, &ruta).is_err() {
            let _ = std::fs::remove_file(&aparte);
        }
        self.existentes_en_disco = cuantos;
        self.sucio = false;
    }

    /// De las extensiones cuyo icono falta, las que hay que pedir ya (las que
    /// no se pidieron hace menos de [`REPETIR_ICONO_MS`]); quedan apuntadas.
    pub fn iconos_que_pedir(&mut self, faltan: Vec<String>) -> Vec<String> {
        let ahora = ahora_ms();
        let pedir: Vec<String> = faltan
            .into_iter()
            .filter(|e| {
                self.iconos_pedidos
                    .get(e)
                    .is_none_or(|t| ahora - t >= REPETIR_ICONO_MS)
            })
            .collect();
        for e in &pedir {
            self.iconos_pedidos.insert(e.clone(), ahora);
            self.sucio = true;
        }
        self.iconos_pedidos
            .retain(|_, t| ahora - *t < REPETIR_ICONO_MS);
        pedir
    }

    pub fn raiz(&self) -> &Path {
        &self.raiz
    }

    /// Olvidar todo (el `reload_data` de Flow).
    pub fn olvidar(&mut self) {
        self.fecha_indice = None;
        self.indice.clear();
        self.cache.clear();
        self.ultima = None;
        self.existentes_heredados = None;
        self.existentes_en_disco = 0;
        self.sucio = true;
    }

    fn carpeta_proyectos(&self) -> PathBuf {
        self.raiz.join("proyectos")
    }

    /// Si lo de la ultima vez sigue valiendo sin mirar el disco: hay vigia,
    /// no aviso de nada y no toca la revision forzosa.
    fn sigue_valiendo(&mut self) -> bool {
        if self.ultima.is_none() {
            return false;
        }
        let Some(vigia) = &self.vigia else {
            return false;
        };
        if vigia.salto() {
            return false;
        }
        self.revisado
            .is_some_and(|t| t.elapsed() < self.revision_forzosa)
    }

    /// Los proyectos, al dia con el disco. En un proceso que se queda vivo lo
    /// normal es que no haya que mirar nada: el vigia dice si algo cambio en
    /// `proyectos`, y si nada cambio se devuelve lo de la ultima vez. Si
    /// cambio (o no hay vigia), se listan las carpetas, que dan la fecha y el
    /// tamano de cada fichero sin abrirlo, y solo se relee lo que cambio.
    pub fn proyectos(&mut self) -> Vec<Proyecto> {
        if self.sigue_valiendo() {
            if let Some(u) = &self.ultima {
                return u.clone();
            }
        }
        if self.vigia.is_none() && self.vigilar {
            // Antes de mirar nada: lo que cambie a partir de ahora, salta.
            self.vigia = Vigia::nuevo(&self.carpeta_proyectos());
        }
        self.revisado = Some(Instant::now());
        self.revisiones += 1;
        let generacion = REVISIONES.fetch_add(1, Ordering::Relaxed) + 1;
        let antes = self.relecturas;
        let salida = self.mirar_el_disco();
        // Lo que la cache de disco sabia de `existe` vale si nada cambio.
        match self.existentes_heredados.take() {
            Some((mapa, desde)) if self.relecturas == antes && !self.sucio => {
                self.existentes_en_disco = mapa.len();
                EXISTENTES.with(|c| *c.borrow_mut() = (generacion, mapa));
                self.existentes_desde = desde;
            }
            _ => {
                self.existentes_en_disco = 0;
                self.existentes_desde = ahora_ms();
            }
        }
        self.ultima = Some(salida.clone());
        salida
    }

    fn mirar_el_disco(&mut self) -> Vec<Proyecto> {
        let base = self.carpeta_proyectos();
        // Una sola lista de `proyectos`: la huella del indice y que carpetas hay.
        let hijos = listar(&base);
        let ruta_indice = base.join("indice.json");
        let f = hijos.get("indice.json").and_then(|h| h.huella);
        if f.is_none() || f != self.fecha_indice {
            self.indice = std::fs::read_to_string(&ruta_indice)
                .ok()
                .and_then(|t| {
                    serde_json::from_str::<IndiceCrudo>(t.trim_start_matches('\u{feff}')).ok()
                })
                .map(|i| i.proyectos)
                .unwrap_or_default();
            self.fecha_indice = f;
            self.sucio = true;
        }
        // Los proyectos que valen, y la huella de cada uno: listar sus
        // carpetas es lo que cuesta (una llamada al disco por carpeta, que
        // ademas mira el antivirus), asi que se hace a la vez en varios hilos.
        let validos: Vec<usize> = self
            .indice
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                !(p.id.is_empty() || p.papelera || p.en_papelera || p.id.contains(['/', '\\', '.']))
                    && hijos.get(p.id.as_str()).is_some_and(|h| h.carpeta)
            })
            .map(|(i, _)| i)
            .collect();
        let carpetas: Vec<PathBuf> = validos
            .iter()
            .map(|i| base.join(&self.indice[*i].id))
            .collect();
        let huellas = fechas_en_paralelo(&carpetas);
        let mut vistos = Vec::new();
        let mut salida = Vec::new();
        for ((i, carpeta), fechas) in validos.into_iter().zip(carpetas).zip(huellas) {
            let p = &self.indice[i];
            vistos.push(p.id.clone());
            let nueva = !self.cache.contains_key(&p.id);
            let entrada = self.cache.entry(p.id.clone()).or_default();
            if nueva || entrada.fechas != fechas {
                *entrada = leer_proyecto(&carpeta);
                entrada.fechas = fechas;
                self.relecturas += 1;
                self.sucio = true;
            }
            salida.push(Proyecto {
                id: p.id.clone(),
                nombre: if p.guardados {
                    NOMBRE_GUARDADOS.to_string()
                } else {
                    p.nombre.trim().to_string()
                },
                tocado: p.tocado,
                hojas: p.hojas,
                guardados: p.guardados,
                carpeta,
                mensajes: entrada.mensajes.clone(),
                hojas_json: entrada.hojas.clone(),
                notas_md: entrada.notas_md.clone(),
                lecciones: entrada.lecciones.clone(),
            });
        }
        let n = self.cache.len();
        self.cache.retain(|id, _| vistos.contains(id));
        if self.cache.len() != n {
            self.sucio = true;
        }
        salida
    }

    /// Cambia en la cache el texto de un mensaje (lo que se espera que la
    /// app escriba). Se pierde en cuanto el fichero cambie de fecha: entonces
    /// manda lo que haya en el disco. Nunca se guarda en la cache de disco.
    pub fn retocar(
        &mut self,
        proyecto: &str,
        codigo: &str,
        cambio: impl FnOnce(&str) -> String,
    ) -> bool {
        let Some(e) = self.cache.get_mut(proyecto) else {
            return false;
        };
        let Some(m) = e.mensajes.iter_mut().find(|m| m.id == codigo) else {
            return false;
        };
        let nuevo = cambio(m.texto());
        m.texto = Some(nuevo.clone());
        self.retocado = true;
        // Y en lo que se devuelve sin mirar el disco.
        if let Some(m) = self
            .ultima
            .iter_mut()
            .flatten()
            .filter(|p| p.id == proyecto)
            .flat_map(|p| p.mensajes.iter_mut())
            .find(|m| m.id == codigo)
        {
            m.texto = Some(nuevo);
        }
        true
    }

    /// El id del proyecto de «Mensajes guardados», si lo hay.
    pub fn id_de_guardados(&mut self) -> Option<String> {
        self.proyectos()
            .into_iter()
            .find(|p| p.guardados)
            .map(|p| p.id)
    }
}

/// Cuantas carpetas de proyecto mira cada hilo (con menos, no compensa
/// arrancar otro).
const CARPETAS_POR_HILO: usize = 3;

/// [`fechas_de`] de cada carpeta, en su orden, repartidas en hilos.
fn fechas_en_paralelo(carpetas: &[PathBuf]) -> Vec<Fechas> {
    let todas = |t: &[PathBuf]| t.iter().map(|c| fechas_de(c)).collect::<Vec<_>>();
    if carpetas.len() <= CARPETAS_POR_HILO {
        return todas(carpetas);
    }
    let mut trozos = carpetas.chunks(CARPETAS_POR_HILO);
    let primero = trozos.next().unwrap_or(&[]);
    std::thread::scope(|s| {
        let hilos: Vec<_> = trozos.map(|t| (t, s.spawn(move || todas(t)))).collect();
        let mut v = todas(primero);
        for (t, h) in hilos {
            // Si un hilo cayera, sus carpetas se miran aqui.
            v.extend(h.join().unwrap_or_else(|_| todas(t)));
        }
        v
    })
}

/// Las fechas y tamanos de los ficheros de los que sale un proyecto (ver
/// [`Fechas`]): su carpeta, y la de notas y la de lecciones si las tiene.
fn fechas_de(carpeta: &Path) -> Fechas {
    let dentro = listar(carpeta);
    let h = |n: &str| dentro.get(n).and_then(|h| h.huella);
    // Las notas: la mas nueva de sus `.md` y cuantas y cuanto ocupan
    // (el titulo sale de dentro, y editarla no cambia la carpeta).
    let notas = dentro.get("notas").filter(|n| n.carpeta).and_then(|n| {
        let lista = listar(&carpeta.join("notas"));
        let tamano = lista
            .values()
            .filter_map(|h| h.huella)
            .fold((lista.len() as u64) << 40, |a, (_, t)| a.wrapping_add(t));
        let mas_nueva = lista
            .values()
            .filter_map(|h| h.huella)
            .map(|(f, _)| f)
            .chain(n.huella.map(|(f, _)| f))
            .max();
        mas_nueva.map(|f| (f, tamano))
    });
    // Las lecciones, igual: cuantas, cuanto ocupan y la mas nueva.
    let lecciones = dentro.get("android").filter(|a| a.carpeta).and_then(|_| {
        let lista = listar(&carpeta.join("android").join("guardados").join("lecciones"));
        let tamano = lista
            .values()
            .filter_map(|h| h.huella)
            .fold((lista.len() as u64) << 40, |a, (_, t)| a.wrapping_add(t));
        lista
            .values()
            .filter_map(|h| h.huella)
            .map(|(f, _)| f)
            .max()
            .map(|f| (f, tamano))
    });
    Fechas {
        guardados: h("guardados.jsonl"),
        proyecto: h("proyecto.json"),
        notas,
        lecciones,
    }
}

/// Borra los `lanzador-indice.<pid>.tmp` de mas de un minuto: los deja un
/// proceso al que Flow mato a medio escribir (Flow mata el de la tecla
/// anterior en cuanto llega la siguiente).
fn limpiar_temporales(dir: &Path) {
    let Ok(lista) = std::fs::read_dir(dir) else {
        return;
    };
    let prefijo = NOMBRE_CACHE.trim_end_matches(".json");
    for e in lista.flatten() {
        let nombre = e.file_name();
        let nombre = nombre.to_string_lossy();
        if !(nombre.starts_with(prefijo) && nombre.ends_with(".tmp")) {
            continue;
        }
        let viejo = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|t| t > Duration::from_secs(60));
        if viejo {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Lo que dice de un hijo la lista de su carpeta.
#[derive(Debug, Clone, Copy)]
struct Hijo {
    huella: Option<(SystemTime, u64)>,
    carpeta: bool,
}

/// Los hijos de una carpeta con su fecha y tamano. En Windows salen de la
/// propia lista (`FindNextFileW`), sin abrir cada fichero: una llamada por
/// carpeta en vez de una apertura por fichero (que el antivirus mira).
fn listar(carpeta: &Path) -> HashMap<String, Hijo> {
    let mut hijos = HashMap::new();
    let Ok(dir) = std::fs::read_dir(carpeta) else {
        return hijos;
    };
    for e in dir.flatten() {
        let Ok(m) = e.metadata() else { continue };
        let huella = m
            .modified()
            .ok()
            .map(|t| (t, if m.is_dir() { 0 } else { m.len() }));
        hijos.insert(
            e.file_name().to_string_lossy().to_string(),
            Hijo {
                huella,
                carpeta: m.is_dir(),
            },
        );
    }
    hijos
}

fn leer_proyecto(carpeta: &Path) -> Entrada {
    let mensajes: Vec<Mensaje> = std::fs::read_to_string(carpeta.join("guardados.jsonl"))
        .map(|t| {
            t.lines()
                .map(|l| l.trim().trim_start_matches('\u{feff}'))
                .filter(|l| !l.is_empty())
                // Una linea rota se salta, como en la app.
                .filter_map(|l| serde_json::from_str::<Mensaje>(l).ok())
                .filter(|m| !m.id.is_empty() && !m.en_buzon)
                .collect()
        })
        .unwrap_or_default();
    let hojas = std::fs::read_to_string(carpeta.join("proyecto.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<ProyectoJson>(t.trim_start_matches('\u{feff}')).ok())
        .map(|p| p.hojas)
        .unwrap_or_default();
    let mut notas_md = Vec::new();
    if let Ok(dir) = std::fs::read_dir(carpeta.join("notas")) {
        for e in dir.flatten() {
            let ruta = e.path();
            if ruta
                .extension()
                .and_then(|x| x.to_str())
                .is_none_or(|x| !x.eq_ignore_ascii_case("md"))
            {
                continue;
            }
            let titulo = titulo_de_md(&ruta);
            let cuando = fecha(&ruta).map(ms).unwrap_or(0);
            notas_md.push(NotaMd {
                ruta,
                titulo,
                cuando,
            });
        }
    }
    let lecciones = leer_lecciones(carpeta, &mensajes);
    Entrada {
        fechas: Fechas::default(),
        mensajes,
        hojas,
        notas_md,
        lecciones,
    }
}

/// Las lecciones del chat: el `.leccion` de cada mensaje que lleva una. Una
/// cuyo fichero aun no llego (el mensaje viajo y el fichero no) o no es una
/// leccion, no sale (como `lecciones::almacen::listar` de la app).
fn leer_lecciones(carpeta: &Path, mensajes: &[Mensaje]) -> Vec<LeccionLeida> {
    mensajes
        .iter()
        .filter(|m| m.es_leccion())
        .filter_map(|m| {
            let ruta = ruta_de_leccion(carpeta, m.ruta.as_deref()?)?;
            let texto = std::fs::read_to_string(&ruta).ok()?;
            let json: serde_json::Value =
                serde_json::from_str(texto.trim_start_matches('\u{feff}')).ok()?;
            pixpin_lecciones::Leccion::de_valor(&json)?;
            Some(LeccionLeida {
                mensaje: m.id.clone(),
                ruta,
                json,
            })
        })
        .collect()
}

/// La primera linea con texto de una nota `.md` (sin las almohadillas), o el
/// nombre del fichero.
fn titulo_de_md(ruta: &Path) -> String {
    use std::io::Read;
    let mut trozo = Vec::new();
    if let Ok(f) = std::fs::File::open(ruta) {
        let _ = f.take(4096).read_to_end(&mut trozo);
    }
    let texto = String::from_utf8_lossy(&trozo);
    primera_linea(&texto).unwrap_or_else(|| {
        ruta.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    })
}

/// La primera linea con texto, sin `#` delante ni enlaces de imagen.
pub fn primera_linea(texto: &str) -> Option<String> {
    texto
        .lines()
        .map(|l| {
            l.trim()
                .trim_start_matches('\u{feff}')
                .trim_start_matches('#')
                .trim()
        })
        .find(|l| !l.is_empty() && !l.starts_with("!["))
        .map(|l| {
            let l: String = l.chars().take(90).collect();
            l
        })
}

/// Milisegundos de ahora.
pub fn ahora_ms() -> i64 {
    ms(SystemTime::now())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_casillas_se_numeran_en_el_orden_del_documento() {
        let d = "# Compra\n\nlo que sea\n- [ ] pan\n* [x] vino\n+ [X] sal\n-[ ] no\n- [?] tampoco";
        assert_eq!(titulo_del_documento(d), "Compra");
        let t = leer_tareas(d);
        assert_eq!(t.len(), 3);
        assert_eq!(
            t[0],
            Tarea {
                indice: 0,
                texto: "pan".into(),
                hecha: false,
                creada: None,
                imagenes: vec![]
            }
        );
        assert_eq!(
            t[2],
            Tarea {
                indice: 2,
                texto: "sal".into(),
                hecha: true,
                creada: None,
                imagenes: vec![]
            }
        );
    }

    #[test]
    fn titulo_solo_con_blanco_detras_de_las_almohadillas() {
        assert_eq!(titulo_del_documento("#pegado\n- [ ] a"), "");
        assert_eq!(titulo_del_documento("####### siete"), "");
        assert_eq!(titulo_del_documento("## Dos"), "Dos");
    }

    #[test]
    fn marcar_y_anadir_en_la_cache() {
        let d = "# L\n\n- [x] a\n- [ ] b";
        assert_eq!(con_tarea_marcada(d, 1, true), "# L\n\n- [x] a\n- [x] b");
        assert_eq!(con_tarea_marcada(d, 0, false), "# L\n\n- [ ] a\n- [ ] b");
        assert_eq!(
            con_tarea_anadida(d, " c "),
            "# L\n\n- [x] a\n- [ ] b\n- [ ] c"
        );
        assert_eq!(con_tarea_anadida("", "c"), "- [ ] c");
    }

    #[test]
    fn rutas_del_movil_y_relativas() {
        let p = Proyecto {
            carpeta: PathBuf::from(r"C:\d\proyectos\X"),
            ..Default::default()
        };
        assert_eq!(
            p.ruta_real("pixpin:files/guardados/voz_1.m4a"),
            Some(PathBuf::from(
                r"C:\d\proyectos\X\android\guardados\voz_1.m4a"
            ))
        );
        assert_eq!(
            p.ruta_real("archivos/a b.pdf"),
            Some(PathBuf::from(r"C:\d\proyectos\X\archivos\a b.pdf"))
        );
        assert_eq!(p.ruta_real("/storage/emulated/0/x.jpg"), None);
        assert_eq!(p.ruta_real(r"C:\x.jpg"), None);
        assert_eq!(p.ruta_real("archivos/../../fuera"), None);
        assert_eq!(p.ruta_real(""), None);
        // Una leccion: la portatil, y la absoluta de un movil viejo por lo de
        // detras de `files/`.
        let x = Path::new(r"C:\d\proyectos\X");
        let esperada = Some(PathBuf::from(
            r"C:\d\proyectos\X\android\guardados\lecciones\k.leccion",
        ));
        assert_eq!(
            ruta_de_leccion(x, "pixpin:files/guardados/lecciones/k.leccion"),
            esperada
        );
        assert_eq!(
            ruta_de_leccion(
                x,
                "/data/user/0/com.forge.pixpin/files/guardados/lecciones/k.leccion"
            ),
            esperada
        );
        // Caso negativo: no se sale de la carpeta.
        assert_eq!(ruta_de_leccion(x, "/data/files/../../k.leccion"), None);
    }

    #[test]
    fn la_primera_linea_salta_titulos_vacios_e_imagenes() {
        assert_eq!(
            primera_linea("\n# \n![a](b)\n## Hola\nmas").as_deref(),
            Some("Hola")
        );
        assert_eq!(primera_linea("  \n"), None);
    }
}

#[cfg(test)]
mod pruebas_de_fecha {
    use super::*;

    #[test]
    fn la_fecha_de_creada_se_separa_del_texto_y_se_cuenta_en_dias() {
        let (t, c) = partir_fecha("pan ➕ 2026-10-02");
        assert_eq!(t, "pan");
        assert_eq!(c, Some(dias_civiles(2026, 10, 2)));
        assert_eq!(dias_civiles(1970, 1, 1), 0);
        assert_eq!(dias_civiles(2026, 10, 2) - dias_civiles(2026, 9, 30), 2);
        // Sin la marca, o pegada a una palabra, todo es texto.
        assert_eq!(
            partir_fecha("pan 2026-10-02"),
            ("pan 2026-10-02".into(), None)
        );
        assert_eq!(partir_fecha("pan➕ 2026-10-02").1, None);
        assert_eq!(leer_tareas("- [ ] pan ➕ 2026-10-02")[0].texto, "pan");
        assert_eq!(
            (hace_dias(0), hace_dias(1), hace_dias(5)),
            ("hoy".into(), "hace 1 día".into(), "hace 5 días".into())
        );
    }
}

#[cfg(test)]
mod pruebas_de_cache {
    use super::*;
    use std::fs;

    /// Dos proyectos con un mensaje cada uno y una nota en el primero.
    fn raiz(etiqueta: &str) -> PathBuf {
        let r = std::env::temp_dir().join(format!(
            "pixpin-lanzador-cache-{etiqueta}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&r);
        let p = r.join("proyectos");
        fs::create_dir_all(p.join("A/notas")).unwrap();
        fs::create_dir_all(p.join("B")).unwrap();
        fs::write(
            p.join("indice.json"),
            r#"{"proyectos":[{"id":"A","nombre":"Uno","tocado":2},{"id":"B","nombre":"Dos","tocado":1}]}"#,
        )
        .unwrap();
        fs::write(
            p.join("A/guardados.jsonl"),
            "{\"id\":\"a1\",\"cuando\":1,\"clase\":\"NOTA\",\"texto\":\"hola\"}\n",
        )
        .unwrap();
        fs::write(
            p.join("B/guardados.jsonl"),
            "{\"id\":\"b1\",\"cuando\":1,\"clase\":\"NOTA\",\"texto\":\"adios\"}\n",
        )
        .unwrap();
        fs::write(p.join("A/notas/n.md"), "# Titulo viejo\n").unwrap();
        r
    }

    fn anadir(r: &Path, proyecto: &str, linea: &str) {
        use std::io::Write;
        let ruta = r.join("proyectos").join(proyecto).join("guardados.jsonl");
        let mut f = fs::OpenOptions::new().append(true).open(ruta).unwrap();
        writeln!(f, "{linea}").unwrap();
    }

    fn sin_vigia(r: &Path, cache: &Path) -> Datos {
        let mut d = Datos::nuevo(r.to_path_buf()).con_cache_en(cache.to_path_buf());
        d.vigilar = false;
        d
    }

    #[test]
    fn el_vigia_ahorra_mirar_el_disco_mientras_nada_cambia() {
        let r = raiz("vigia");
        let mut d = Datos::nuevo(r.clone());
        assert_eq!(d.proyectos().len(), 2);
        assert_eq!((d.revisiones, d.relecturas), (1, 2));
        // Los ficheros recien creados aun pueden dar algun aviso tardio (el
        // sistema de ficheros pone al dia sus entradas, el antivirus los
        // mira): que se calme. Esos avisos hacen mirar, no releer.
        std::thread::sleep(Duration::from_millis(300));
        d.proyectos();
        let revisiones = d.revisiones;
        // Caso negativo: sin cambios no se mira nada ni se relee nada.
        for _ in 0..5 {
            assert_eq!(d.proyectos()[1].mensajes.len(), 1);
        }
        assert_eq!((d.revisiones, d.relecturas), (revisiones, 2));
        // Un mensaje nuevo en B: el vigia salta y solo se relee B.
        anadir(
            &r,
            "B",
            r#"{"id":"b2","cuando":2,"clase":"NOTA","texto":"otra"}"#,
        );
        let mut ps = d.proyectos();
        // El aviso de Windows llega casi siempre antes de volver de escribir;
        // por si tarda, se le da un margen.
        for _ in 0..50 {
            if ps[1].mensajes.len() == 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
            ps = d.proyectos();
        }
        assert_eq!(ps[1].mensajes.len(), 2);
        assert_eq!(d.relecturas, 3, "A no cambio: no se relee");
    }

    #[test]
    fn sin_vigia_se_mira_el_disco_cada_vez() {
        let r = raiz("sinvigia");
        let mut d = Datos::nuevo(r.clone());
        d.vigilar = false;
        d.proyectos();
        d.proyectos();
        assert_eq!(
            (d.revisiones, d.relecturas),
            (2, 2),
            "se mira cada vez, pero no se relee lo que no cambio"
        );
        anadir(
            &r,
            "A",
            r#"{"id":"a2","cuando":2,"clase":"NOTA","texto":"x"}"#,
        );
        assert_eq!(d.proyectos()[0].mensajes.len(), 2);
        assert_eq!(d.relecturas, 3);
    }

    #[test]
    fn la_cache_de_disco_sirve_al_proceso_siguiente_y_se_invalida_por_fecha_y_tamano() {
        let r = raiz("disco");
        let ruta = r.join("cache").join(NOMBRE_CACHE);
        let mut d = sin_vigia(&r, &ruta);
        assert_eq!(d.proyectos().len(), 2);
        assert_eq!(d.relecturas, 2);
        d.guardar_cache();
        assert!(ruta.is_file());

        // El proceso siguiente no relee nada.
        let mut d = sin_vigia(&r, &ruta);
        let ps = d.proyectos();
        assert_eq!(d.relecturas, 0);
        assert_eq!(ps[0].mensajes[0].texto(), "hola");
        assert_eq!(ps[0].notas_md[0].titulo, "Titulo viejo");

        // Cambia B: el siguiente relee solo B.
        anadir(
            &r,
            "B",
            r#"{"id":"b2","cuando":2,"clase":"NOTA","texto":"otra"}"#,
        );
        let mut d = sin_vigia(&r, &ruta);
        let ps = d.proyectos();
        assert_eq!(d.relecturas, 1);
        assert_eq!(ps[1].mensajes.len(), 2);
        d.guardar_cache();

        // Editar una nota cambia su titulo aunque la carpeta no cambie.
        std::thread::sleep(Duration::from_millis(20));
        fs::write(
            r.join("proyectos/A/notas/n.md"),
            "# Titulo nuevo y mas largo\n",
        )
        .unwrap();
        let mut d = sin_vigia(&r, &ruta);
        assert_eq!(
            d.proyectos()[0].notas_md[0].titulo,
            "Titulo nuevo y mas largo"
        );
        assert_eq!(d.relecturas, 1);
    }

    #[test]
    fn caso_negativo_una_cache_de_otra_raiz_o_rota_no_vale() {
        let r = raiz("ajena");
        let ruta = r.join("cache").join(NOMBRE_CACHE);
        let mut d = sin_vigia(&r, &ruta);
        d.proyectos();
        d.guardar_cache();
        // Otra raiz con la misma cache: se relee todo.
        let otra = raiz("ajena-otra");
        let mut d = sin_vigia(&otra, &ruta);
        d.proyectos();
        assert_eq!(d.relecturas, 2);
        // Rota: igual.
        fs::write(&ruta, "{ no es json").unwrap();
        let mut d = sin_vigia(&r, &ruta);
        assert_eq!(d.proyectos()[1].mensajes[0].texto(), "adios");
        assert_eq!(d.relecturas, 2);
    }

    #[test]
    fn caso_negativo_lo_adelantado_no_se_guarda_en_disco() {
        let r = raiz("retocado");
        let ruta = r.join("cache").join(NOMBRE_CACHE);
        let mut d = sin_vigia(&r, &ruta);
        d.proyectos();
        d.guardar_cache();
        let mut d = sin_vigia(&r, &ruta);
        d.proyectos();
        assert!(d.retocar("A", "a1", |_| "adelantado".into()));
        assert_eq!(d.proyectos()[0].mensajes[0].texto(), "adelantado");
        d.guardar_cache();
        let mut d = sin_vigia(&r, &ruta);
        assert_eq!(
            d.proyectos()[0].mensajes[0].texto(),
            "hola",
            "manda el disco, no lo adelantado"
        );
    }

    #[test]
    fn existe_se_recuerda_y_pasa_al_proceso_siguiente_si_nada_cambio() {
        let r = raiz("existe");
        let f = r.join("proyectos/A/archivo.pdf");
        fs::write(&f, b"x").unwrap();
        let ruta = r.join("cache").join(NOMBRE_CACHE);
        let mut d = sin_vigia(&r, &ruta);
        d.proyectos();
        assert!(existe(&f));
        d.guardar_cache();
        // Se borra a mano (ninguna fecha de las que se miran cambia): el
        // proceso siguiente se fia de la cache, que dura poco.
        fs::remove_file(&f).unwrap();
        let mut d = sin_vigia(&r, &ruta);
        d.proyectos();
        assert!(existe(&f), "heredado de la cache de disco");
        // Caso negativo: si algo cambio, lo heredado no vale.
        anadir(
            &r,
            "B",
            r#"{"id":"b2","cuando":2,"clase":"NOTA","texto":"otra"}"#,
        );
        let mut d = sin_vigia(&r, &ruta);
        d.proyectos();
        assert!(!existe(&f));
    }

    #[test]
    fn la_tecla_siguiente_no_reescribe_la_cache_si_no_aprendio_nada() {
        let r = raiz("sin-reescribir");
        let f = r.join("proyectos/A/archivo.pdf");
        let g = r.join("proyectos/A/otro.pdf");
        fs::write(&f, b"x").unwrap();
        let ruta = r.join("cache").join(NOMBRE_CACHE);
        let mut d = sin_vigia(&r, &ruta);
        d.proyectos();
        assert!(existe(&f));
        d.guardar_cache();
        // El proceso siguiente pregunta lo mismo: el fichero no se toca (se
        // borra tras leerlo para ver si se vuelve a escribir).
        let mut d = sin_vigia(&r, &ruta);
        fs::remove_file(&ruta).unwrap();
        d.proyectos();
        assert!(existe(&f));
        d.guardar_cache();
        assert!(!ruta.exists(), "nada nuevo: no se reescribe");
        // Caso negativo: con una respuesta nueva de `existe`, si.
        assert!(!existe(&g));
        d.guardar_cache();
        assert!(ruta.is_file(), "algo nuevo: se guarda");
    }

    #[test]
    fn caso_negativo_los_temporales_viejos_se_borran_y_los_recientes_no() {
        let r = raiz("temporales");
        let dir = r.join("cache");
        fs::create_dir_all(&dir).unwrap();
        let viejo = dir.join("lanzador-indice.111.tmp");
        let reciente = dir.join("lanzador-indice.222.tmp");
        let ajeno = dir.join("otra-cosa.tmp");
        for f in [&viejo, &reciente, &ajeno] {
            fs::write(f, b"").unwrap();
        }
        let hace_rato = std::time::SystemTime::now() - Duration::from_secs(600);
        for f in [&viejo, &ajeno] {
            fs::File::options()
                .write(true)
                .open(f)
                .unwrap()
                .set_modified(hace_rato)
                .unwrap();
        }
        limpiar_temporales(&dir);
        assert!(!viejo.exists());
        assert!(reciente.exists(), "puede ser de un proceso vivo");
        assert!(ajeno.exists(), "no es de la cache");
    }

    #[test]
    fn un_icono_se_pide_una_vez_aunque_cambie_el_proceso() {
        let r = raiz("iconos");
        let ruta = r.join("cache").join(NOMBRE_CACHE);
        let mut d = sin_vigia(&r, &ruta);
        d.proyectos();
        assert_eq!(
            d.iconos_que_pedir(vec!["pdf".into(), "dwg".into()]),
            ["pdf", "dwg"]
        );
        // Caso negativo: en seguida, no se vuelve a pedir.
        assert!(d.iconos_que_pedir(vec!["pdf".into()]).is_empty());
        d.guardar_cache();
        let mut d = sin_vigia(&r, &ruta);
        d.proyectos();
        assert_eq!(
            d.iconos_que_pedir(vec!["pdf".into(), "zip".into()]),
            ["zip"]
        );
    }

    #[test]
    fn las_huellas_en_paralelo_son_las_mismas_y_en_el_mismo_orden() {
        let r = raiz("paralelo");
        let base = r.join("proyectos");
        let mut carpetas: Vec<PathBuf> = (0..8)
            .map(|i| {
                let c = base.join(format!("Q{i}"));
                fs::create_dir_all(c.join("notas")).unwrap();
                fs::write(c.join("guardados.jsonl"), "x".repeat(i + 1)).unwrap();
                fs::write(c.join("notas").join("n.md"), "y".repeat(i * 3)).unwrap();
                c
            })
            .collect();
        // Caso negativo: una que no existe no tumba a las demas.
        carpetas.push(base.join("no-existe"));
        let una_a_una: Vec<Fechas> = carpetas.iter().map(|c| fechas_de(c)).collect();
        assert_eq!(fechas_en_paralelo(&carpetas), una_a_una);
        assert_eq!(una_a_una[8], Fechas::default());
        assert_eq!(una_a_una[3].guardados.map(|(_, t)| t), Some(4));
    }

    /// Lo que ahorra mirar las carpetas en hilos, con los datos de verdad
    /// (`PIXPIN_MEDIR_RAIZ`): `cargo test -p pixpin-lanzador medir_huellas
    /// -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn medir_huellas_en_paralelo() {
        let Some(r) = std::env::var_os("PIXPIN_MEDIR_RAIZ").map(PathBuf::from) else {
            return;
        };
        let base = r.join("proyectos");
        let carpetas: Vec<PathBuf> = fs::read_dir(&base)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();
        let (mut uno, mut varios) = (Vec::new(), Vec::new());
        for _ in 0..40 {
            let t = Instant::now();
            std::hint::black_box(carpetas.iter().map(|c| fechas_de(c)).collect::<Vec<_>>());
            uno.push(t.elapsed());
            let t = Instant::now();
            std::hint::black_box(fechas_en_paralelo(&carpetas));
            varios.push(t.elapsed());
        }
        uno.sort();
        varios.sort();
        println!(
            "{} carpetas: una a una {:?}, en hilos {:?} (medianas)",
            carpetas.len(),
            uno[20],
            varios[20]
        );
    }
}
