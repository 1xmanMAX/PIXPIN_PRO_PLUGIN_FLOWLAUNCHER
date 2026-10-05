//! **Tareas con imagenes desde Flow**, como en Claude Code.
//!
//! La caja de Flow es texto: Ctrl+V de una imagen no pega nada, y el plugin
//! no ve las teclas. Lo hace la app, que esta siempre abierta: su gancho de
//! teclado (`pixpin_shell::pegar_en_flow`) ve el Ctrl+V con Flow delante y
//! una imagen (sin texto) en el portapapeles, se lo traga, guarda la imagen
//! con [`pegar_en_borrador`] y escribe ` [img 01] ` en la caja.
//!
//! Si la app no esta abierta, al escribir una tarea (`t …`, `tareas …`,
//! dentro de una lista) sale arriba «📎 Pegar la imagen copiada → [img 01]»,
//! que hace lo mismo con Intro (sin cerrar Flow). Las dos guardan la imagen
//! en `<raiz>\cache\lanzador-imagenes\` y apuntan en el borrador
//! (`borrador.json`) que `[img 01]` es ese fichero. Cada tecla es otro
//! proceso: el borrador es lo que les dice que fichero es cada ficha (caduca
//! a la media hora). La app usa este mismo modulo (depende de este crate),
//! asi que el formato es uno solo.
//!
//! **Una imagen, un nombre**: la misma imagen (por su contenido, ver
//! [`huella`]) no se pega dos veces en el mismo borrador.
//!
//! Al mandar la tarea, el pedido `anadir_tarea` lleva `imagenes` (las rutas,
//! en el orden de las fichas renumeradas `[img 01]`, `[img 02]`…); la app
//! cambia cada ficha del texto por el enlace a la imagen.
//!
//! **Ficheros** (4-oct): con ficheros copiados en el Explorador, el Ctrl+V
//! de la app escribe ` [archivo 01] ` por cada uno que no sea una imagen
//! ([`pegar_ficheros_en_borrador`]; una imagen sigue siendo `[img NN]`). El
//! borrador apunta su ruta (`archivos`); un fichero, un nombre (la misma
//! ruta no se repite). Solo un mensaje del chat los lleva (`chat.archivos`,
//! adjuntos como al soltarlos): en una tarea la ficha se queda como texto.

use crate::resultados::{Accion, Contexto, Resultado};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// La carpeta de las imagenes pegadas, dentro de `<raiz>/cache`.
pub const CARPETA: &str = "lanzador-imagenes";
/// Lo que dice que fichero es cada ficha del borrador de ahora.
const BORRADOR: &str = "borrador.json";
/// Un borrador sin tocar en media hora ya no vale.
pub const CADUCA_MS: i64 = 30 * 60 * 1000;
/// Las imagenes de mas de un dia se borran (la app ya las copio a su chat).
const LIMPIAR_MS: i64 = 24 * 60 * 60 * 1000;

/// Las extensiones de un fichero copiado que cuentan como imagen.
pub fn es_imagen_pegable(ruta: &Path) -> bool {
    ruta.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|e| matches!(e.as_str(), "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp"))
}

// --- El portapapeles --------------------------------------------------------

/// Leer el portapapeles, o hacer como que (las pruebas no tocan el de
/// verdad).
#[derive(Clone, Copy)]
pub struct Portapapeles {
    /// Si hay una imagen: el numero de secuencia del portapapeles
    /// (`GetClipboardSequenceNumber`), para saber si ya se pego. Rapido y
    /// sin esperar: si esta ocupado, `None`.
    pub imagen: fn() -> Option<u32>,
    /// Los bytes de la imagen, con la extension que les toca.
    pub leer: fn() -> Option<Copiada>,
    /// Si la app esta abierta (y pega ella con Ctrl+V): entonces sobra el
    /// resultado de pegar.
    pub pega_la_app: fn() -> bool,
}

/// La imagen del portapapeles, leida: el PNG o el BMP del mapa de bits, o
/// el fichero de imagen copiado en el Explorador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copiada {
    /// Sin punto y en minusculas: `png`, `bmp`, `jpg`…
    pub extension: String,
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for Portapapeles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Portapapeles")
    }
}

fn nunca() -> Option<u32> {
    None
}
fn no_leer() -> Option<Copiada> {
    None
}
fn no() -> bool {
    false
}

/// El de las pruebas y el de por defecto: nunca hay imagen.
pub const SIN_PORTAPAPELES: Portapapeles = Portapapeles {
    imagen: nunca,
    leer: no_leer,
    pega_la_app: no,
};

/// El de Windows.
pub const WINDOWS: Portapapeles = Portapapeles {
    imagen: win::imagen,
    leer: win::leer,
    pega_la_app: crate::pedido::app_abierta,
};

/// **La huella del contenido** de una imagen (FNV-1a de 64 bits de sus
/// bytes): lo que dice que dos pegadas son la misma imagen aunque se
/// copiara dos veces (el numero de secuencia del portapapeles cambia en
/// cada copia; el contenido no). Fija entre versiones, porque viaja en el
/// borrador de un proceso a otro.
pub fn huella(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

// --- Las fichas `[img NN]` -----------------------------------------------------

/// Una ficha `[img NN]` dentro de un texto (posiciones en bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ficha {
    pub numero: u32,
    pub inicio: usize,
    pub fin: usize,
}

/// `[img 01]`.
pub fn ficha(numero: u32) -> String {
    format!("[img {numero:02}]")
}

/// `[archivo 01]`: un fichero pegado (Ctrl+V de ficheros copiados en el
/// Explorador) que va al chat como adjunto.
pub fn ficha_de_archivo(numero: u32) -> String {
    format!("[archivo {numero:02}]")
}

/// Las fichas del texto, en orden. `[img 1]`, `[IMG 01]` tambien valen; el
/// `0` no.
pub fn fichas(texto: &str) -> Vec<Ficha> {
    fichas_de(texto, b"img")
}

/// Las fichas `[archivo NN]` del texto, con las mismas reglas.
pub fn fichas_de_archivo(texto: &str) -> Vec<Ficha> {
    fichas_de(texto, b"archivo")
}

/// Las fichas `[<palabra> NN]` del texto.
fn fichas_de(texto: &str, palabra: &[u8]) -> Vec<Ficha> {
    let mut v = Vec::new();
    let b = texto.as_bytes();
    let n = palabra.len();
    let mut i = 0;
    while let Some(d) = texto[i..].find('[') {
        let inicio = i + d;
        i = inicio + 1;
        let resto = &b[inicio..];
        if resto.len() < n + 4
            || !resto[1..1 + n].eq_ignore_ascii_case(palabra)
            || resto[1 + n] != b' '
        {
            continue;
        }
        let cifras = 2 + n;
        let digitos = resto[cifras..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        if !(1..=3).contains(&digitos) || resto.get(cifras + digitos) != Some(&b']') {
            continue;
        }
        let numero: u32 = texto[inicio + cifras..inicio + cifras + digitos]
            .parse()
            .unwrap_or(0);
        if numero == 0 {
            continue;
        }
        let fin = inicio + cifras + digitos + 1;
        v.push(Ficha {
            numero,
            inicio,
            fin,
        });
        i = fin;
    }
    v
}

/// El texto sin sus fichas `[img NN]` ni `[archivo NN]`, con los blancos de
/// mas fuera (el mensaje que va al chat delante de sus adjuntos).
pub fn sin_fichas(texto: &str) -> String {
    let mut todas = fichas(texto);
    todas.extend(fichas_de_archivo(texto));
    todas.sort_by_key(|f| f.inicio);
    let mut s = String::new();
    let mut desde = 0;
    for f in todas {
        s.push_str(&texto[desde..f.inicio]);
        s.push(' ');
        desde = f.fin;
    }
    s.push_str(&texto[desde..]);
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// El numero de la siguiente imagen: uno mas que la mayor del texto.
pub fn siguiente(texto: &str) -> u32 {
    fichas(texto).iter().map(|f| f.numero).max().unwrap_or(0) + 1
}

// --- El borrador en disco -------------------------------------------------

/// Que fichero es cada ficha del borrador de ahora.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Borrador {
    /// Cuando se pego la ultima (ms).
    pub tocado: i64,
    /// El numero de secuencia del portapapeles de la ultima pegada.
    #[serde(default)]
    pub secuencia: Option<u32>,
    /// Numero de la ficha → ruta del fichero.
    pub imagenes: BTreeMap<u32, String>,
    /// Numero de la ficha → [`huella`] de su imagen.
    #[serde(default)]
    pub huellas: BTreeMap<u32, u64>,
    /// Los numeros de secuencia del portapapeles cuya imagen ya esta en el
    /// borrador: con uno de estos no se ofrece pegar (sin leer la imagen).
    #[serde(default)]
    pub secuencias: Vec<u32>,
    /// Numero de la ficha `[archivo NN]` → ruta del fichero pegado (el
    /// original: no se copia, la app lo lee al mandar el mensaje).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub archivos: BTreeMap<u32, String>,
}

impl Borrador {
    /// La ficha que ya tiene esa imagen.
    pub fn ficha_de_huella(&self, h: u64) -> Option<u32> {
        self.huellas.iter().find(|(_, x)| **x == h).map(|(n, _)| *n)
    }

    /// El numero de la siguiente imagen segun el borrador (la app no ve lo
    /// tecleado en Flow).
    pub fn siguiente(&self) -> u32 {
        self.imagenes.keys().max().copied().unwrap_or(0) + 1
    }

    /// El numero del siguiente `[archivo NN]` segun el borrador.
    pub fn siguiente_archivo(&self) -> u32 {
        self.archivos.keys().max().copied().unwrap_or(0) + 1
    }

    /// La ficha que ya tiene ese fichero (la misma ruta, sin distinguir
    /// mayusculas ni las dos barras, como Windows).
    pub fn ficha_de_ruta(&self, ruta: &Path) -> Option<u32> {
        let clave = clave_de_ruta(&ruta.to_string_lossy());
        self.archivos
            .iter()
            .find(|(_, r)| clave_de_ruta(r) == clave)
            .map(|(n, _)| *n)
    }

    fn apuntar_secuencia(&mut self, secuencia: Option<u32>) {
        self.secuencia = secuencia;
        if let Some(s) = secuencia.filter(|s| !self.secuencias.contains(s)) {
            self.secuencias.push(s);
        }
    }
}

pub fn carpeta(raiz: &Path) -> PathBuf {
    raiz.join("cache").join(CARPETA)
}

/// El borrador, si no caduco.
pub fn leer_borrador(raiz: &Path, ahora: i64) -> Option<Borrador> {
    let texto = std::fs::read_to_string(carpeta(raiz).join(BORRADOR)).ok()?;
    let b: Borrador = serde_json::from_str(&texto).ok()?;
    (ahora - b.tocado < CADUCA_MS).then_some(b)
}

/// Apunta que la ficha `numero` es `ruta`. La primera (`[img 01]`) empieza
/// un borrador nuevo.
pub fn apuntar(raiz: &Path, numero: u32, ruta: &Path, secuencia: Option<u32>, ahora: i64) -> bool {
    apuntar_con_huella(raiz, numero, ruta, secuencia, None, ahora)
}

fn apuntar_con_huella(
    raiz: &Path,
    numero: u32,
    ruta: &Path,
    secuencia: Option<u32>,
    h: Option<u64>,
    ahora: i64,
) -> bool {
    let previo = leer_borrador(raiz, ahora).unwrap_or_default();
    // La primera imagen empieza un borrador nuevo (los ficheros pegados se
    // quedan: van por su cuenta, y [`podar`] los sigue con lo tecleado).
    let mut b = if numero <= 1 {
        Borrador {
            archivos: previo.archivos,
            ..Borrador::default()
        }
    } else {
        previo
    };
    b.tocado = ahora;
    b.apuntar_secuencia(secuencia);
    b.imagenes
        .insert(numero, ruta.to_string_lossy().to_string());
    if let Some(h) = h {
        b.huellas.insert(numero, h);
    }
    escribir_borrador(raiz, &b)
}

fn escribir_borrador(raiz: &Path, b: &Borrador) -> bool {
    let dir = carpeta(raiz);
    let _ = std::fs::create_dir_all(&dir);
    let Ok(texto) = serde_json::to_string(b) else {
        return false;
    };
    std::fs::write(dir.join(BORRADOR), texto).is_ok()
}

/// Lo que paso al pegar la imagen del portapapeles en el borrador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pegado {
    /// Guardada como la ficha `.0`, en `.1`.
    Nueva(u32, PathBuf),
    /// Esa imagen ya esta en el borrador como la ficha `.0`: no se guarda
    /// otra vez.
    Repetida(u32),
    /// No hay imagen en el portapapeles (o no se pudo guardar).
    Nada,
}

/// **Pega la imagen del portapapeles en el borrador**: la guarda en la
/// carpeta y la apunta como la ficha `numero` (el plugin, que ve lo
/// tecleado) o como la siguiente del borrador (`None`: la app). Una imagen,
/// un nombre: si su contenido ya esta en el borrador no se guarda otra vez
/// y se dice cual es su ficha.
pub fn pegar_en_borrador(
    raiz: &Path,
    pp: &Portapapeles,
    numero: Option<u32>,
    ahora: i64,
) -> Pegado {
    let Some(secuencia) = (pp.imagen)() else {
        return Pegado::Nada;
    };
    let Some(copiada) = (pp.leer)() else {
        return Pegado::Nada;
    };
    pegar_copiada(raiz, &copiada, Some(secuencia), numero, ahora)
}

/// Guarda `copiada` y la apunta (lo comun a pegar del portapapeles y a pegar
/// un fichero de imagen copiado en el Explorador).
fn pegar_copiada(
    raiz: &Path,
    copiada: &Copiada,
    secuencia: Option<u32>,
    numero: Option<u32>,
    ahora: i64,
) -> Pegado {
    let h = huella(&copiada.bytes);
    let previo = leer_borrador(raiz, ahora);
    let numero = numero.unwrap_or_else(|| previo.as_ref().map_or(1, Borrador::siguiente));
    // La ficha 1 empieza un borrador nuevo: lo de antes ya no cuenta.
    if let Some(mut b) = previo.filter(|_| numero > 1) {
        if let Some(n) = b.ficha_de_huella(h) {
            b.apuntar_secuencia(secuencia);
            b.tocado = ahora;
            escribir_borrador(raiz, &b);
            return Pegado::Repetida(n);
        }
    }
    limpiar(raiz);
    let dir = carpeta(raiz);
    let _ = std::fs::create_dir_all(&dir);
    // Con la huella en el nombre: dos imagenes del mismo milisegundo no se pisan.
    let destino = dir.join(format!("img-{ahora:x}-{h:016x}.{}", copiada.extension));
    if std::fs::write(&destino, &copiada.bytes).is_err()
        || !apuntar_con_huella(raiz, numero, &destino, secuencia, Some(h), ahora)
    {
        return Pegado::Nada;
    }
    Pegado::Nueva(numero, destino)
}

/// Lo que paso con cada fichero copiado al pegarlo (ver
/// [`pegar_ficheros_en_borrador`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PegadoFichero {
    /// Una imagen (por su extension): se guarda y se apunta como `[img NN]`,
    /// como la del portapapeles.
    Imagen(Pegado),
    /// Otro fichero: apuntado como `[archivo NN]`. `nuevo` es `false` si ya
    /// estaba (un fichero, un nombre: no se escribe otra vez).
    Archivo { numero: u32, nuevo: bool },
    /// No es un fichero (una carpeta, o ya no esta): no se pega.
    NoVale(PathBuf),
}

impl PegadoFichero {
    /// La ficha que hay que escribir en Flow, si es nueva.
    pub fn ficha_nueva(&self) -> Option<String> {
        match self {
            PegadoFichero::Imagen(Pegado::Nueva(n, _)) => Some(ficha(*n)),
            PegadoFichero::Archivo {
                numero,
                nuevo: true,
            } => Some(ficha_de_archivo(*numero)),
            _ => None,
        }
    }
}

/// **Pega ficheros copiados (en el Explorador) en el borrador**: cada imagen
/// como `[img NN]` (se guarda una copia, como la del portapapeles), cada otro
/// fichero como `[archivo NN]` (se apunta su ruta). Una carpeta o una ruta
/// que no existe se salta. El mismo fichero dos veces es una sola ficha.
pub fn pegar_ficheros_en_borrador(
    raiz: &Path,
    rutas: &[PathBuf],
    secuencia: Option<u32>,
    ahora: i64,
) -> Vec<PegadoFichero> {
    let mut v = Vec::with_capacity(rutas.len());
    for ruta in rutas {
        if !ruta.is_file() {
            v.push(PegadoFichero::NoVale(ruta.clone()));
            continue;
        }
        if es_imagen_pegable(ruta) {
            let pegado = std::fs::read(ruta)
                .ok()
                .map(|bytes| Copiada {
                    extension: ruta
                        .extension()
                        .map(|e| e.to_string_lossy().to_ascii_lowercase())
                        .unwrap_or_else(|| "png".into()),
                    bytes,
                })
                .map_or(Pegado::Nada, |c| {
                    pegar_copiada(raiz, &c, secuencia, None, ahora)
                });
            v.push(PegadoFichero::Imagen(pegado));
            continue;
        }
        let mut b = leer_borrador(raiz, ahora).unwrap_or_default();
        b.tocado = ahora;
        let (numero, nuevo) = match b.ficha_de_ruta(ruta) {
            Some(n) => (n, false),
            None => {
                let n = b.siguiente_archivo();
                b.archivos.insert(n, ruta.to_string_lossy().to_string());
                (n, true)
            }
        };
        if escribir_borrador(raiz, &b) {
            v.push(PegadoFichero::Archivo { numero, nuevo });
        } else {
            v.push(PegadoFichero::NoVale(ruta.clone()));
        }
    }
    v
}

/// Una ruta para comparar: sin mayusculas y con la barra de Windows.
fn clave_de_ruta(ruta: &str) -> String {
    ruta.replace('/', "\\").to_lowercase()
}

/// Lo que se espera, tras pegar, antes de [`podar`]: mientras la app
/// escribe ` [img 01] ` letra a letra, Flow pregunta con media ficha.
pub const GRACIA_PODAR_MS: i64 = 3_000;

/// **El borrador sigue a lo tecleado**: las fichas que ya no estan en
/// `texto` (la consulta de Flow) se olvidan, y si no queda ninguna se borra.
/// Asi una tarea nueva (Flow vacio) vuelve a empezar en `[img 01]`, y la
/// imagen de una ficha borrada se puede pegar de nuevo. Nada justo despues
/// de pegar (ver [`GRACIA_PODAR_MS`]). Solo escribe si cambia algo.
pub fn podar(raiz: &Path, texto: &str, ahora: i64) {
    let Some(mut b) = leer_borrador(raiz, ahora) else {
        return;
    };
    if ahora - b.tocado < GRACIA_PODAR_MS {
        return;
    }
    let estan: Vec<u32> = fichas(texto).iter().map(|f| f.numero).collect();
    let estan_archivos: Vec<u32> = fichas_de_archivo(texto).iter().map(|f| f.numero).collect();
    let antes = (b.imagenes.len(), b.archivos.len());
    b.imagenes.retain(|n, _| estan.contains(n));
    b.archivos.retain(|n, _| estan_archivos.contains(n));
    if (b.imagenes.len(), b.archivos.len()) == antes {
        return;
    }
    if b.imagenes.len() != antes.0 {
        b.huellas.retain(|n, _| estan.contains(n));
        // No se sabe de que ficha era cada secuencia: que se vuelvan a mirar.
        b.secuencias.clear();
        b.secuencia = None;
    }
    if b.imagenes.is_empty() && b.archivos.is_empty() {
        olvidar_borrador(raiz);
    } else {
        escribir_borrador(raiz, &b);
    }
}

/// Olvida el borrador (la tarea ya se mando). Las imagenes se quedan: la app
/// las copia cuando le llega el pedido.
pub fn olvidar_borrador(raiz: &Path) {
    let _ = std::fs::remove_file(carpeta(raiz).join(BORRADOR));
}

/// Borra las imagenes de mas de un dia.
pub fn limpiar(raiz: &Path) {
    let Ok(dir) = std::fs::read_dir(carpeta(raiz)) else {
        return;
    };
    let viejo = std::time::Duration::from_millis(LIMPIAR_MS as u64);
    for e in dir.flatten() {
        if e.file_name() == BORRADOR {
            continue;
        }
        let edad = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.elapsed().ok());
        if edad.is_some_and(|t| t > viejo) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

// --- De las fichas al pedido ------------------------------------------------

/// Las fichas de un texto, resueltas contra el borrador.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Adjuntas {
    /// El texto para la app: las fichas conocidas renumeradas `[img 01]`,
    /// `[img 02]`… (y `[archivo 01]`…) en el orden de su numero; las
    /// desconocidas, tal cual.
    pub texto: String,
    /// Las rutas de las imagenes, en el orden de las fichas renumeradas.
    pub rutas: Vec<String>,
    /// Los numeros conocidos (los de lo tecleado, sin renumerar).
    pub conocidas: Vec<u32>,
    /// Las rutas de los ficheros `[archivo NN]`, en su orden renumerado.
    pub archivos: Vec<String>,
    /// Los numeros de `[archivo NN]` conocidos (sin renumerar).
    pub archivos_conocidos: Vec<u32>,
    /// Las fichas que no se sabe que fichero son (escritas a mano, o de un
    /// borrador caducado): se quedan como texto.
    pub desconocidas: Vec<String>,
}

/// Los numeros de `fs` que tienen fichero, en orden y sin repetir.
fn conocidos(fs: &[Ficha], ruta: impl Fn(u32) -> bool) -> Vec<u32> {
    let mut v: Vec<u32> = fs.iter().map(|f| f.numero).filter(|n| ruta(*n)).collect();
    v.sort_unstable();
    v.dedup();
    v
}

pub fn resolver(texto: &str, borrador: Option<&Borrador>) -> Adjuntas {
    let existe = |r: &&String| Path::new(r.as_str()).is_file();
    let ruta_de = |n: u32| borrador.and_then(|b| b.imagenes.get(&n)).filter(existe);
    let archivo_de = |n: u32| borrador.and_then(|b| b.archivos.get(&n)).filter(existe);
    let de_imagen = fichas(texto);
    let de_archivo = fichas_de_archivo(texto);
    let conocidas = conocidos(&de_imagen, |n| ruta_de(n).is_some());
    let archivos_conocidos = conocidos(&de_archivo, |n| archivo_de(n).is_some());
    // Las dos clases de ficha, en el orden del texto.
    let mut todas: Vec<(Ficha, bool)> = de_imagen
        .into_iter()
        .map(|f| (f, false))
        .chain(de_archivo.into_iter().map(|f| (f, true)))
        .collect();
    todas.sort_by_key(|(f, _)| f.inicio);
    let mut a = Adjuntas::default();
    let mut desde = 0;
    for (f, es_archivo) in &todas {
        a.texto.push_str(&texto[desde..f.inicio]);
        let lista = if *es_archivo {
            &archivos_conocidos
        } else {
            &conocidas
        };
        match lista.iter().position(|n| *n == f.numero) {
            Some(i) if *es_archivo => a.texto.push_str(&ficha_de_archivo(i as u32 + 1)),
            Some(i) => a.texto.push_str(&ficha(i as u32 + 1)),
            None => {
                let t = &texto[f.inicio..f.fin];
                a.texto.push_str(t);
                if !a.desconocidas.iter().any(|d| d == t) {
                    a.desconocidas.push(t.to_string());
                }
            }
        }
        desde = f.fin;
    }
    a.texto.push_str(&texto[desde..]);
    a.rutas = conocidas
        .iter()
        .filter_map(|n| ruta_de(*n).cloned())
        .collect();
    a.archivos = archivos_conocidos
        .iter()
        .filter_map(|n| archivo_de(*n).cloned())
        .collect();
    a.conocidas = conocidas;
    a.archivos_conocidos = archivos_conocidos;
    a
}

/// Las posiciones UTF-16 (`titleHighlightData`) de las fichas de `titulo`
/// cuyo numero esta en `numeros`.
pub fn resaltado(titulo: &str, numeros: &[u32]) -> Vec<usize> {
    resaltado_de(titulo, fichas(titulo), numeros)
}

fn resaltado_de(titulo: &str, fs: Vec<Ficha>, numeros: &[u32]) -> Vec<usize> {
    let mut v = Vec::new();
    for f in fs.into_iter().filter(|f| numeros.contains(&f.numero)) {
        let inicio = titulo[..f.inicio].encode_utf16().count();
        let largo = titulo[f.inicio..f.fin].encode_utf16().count();
        v.extend(inicio..inicio + largo);
    }
    v
}

/// Cuantos nombres de fichero se dicen en el subtitulo (los demas, «y N mas»).
const NOMBRES_EN_SUBTITULO: usize = 3;

/// **El «Añadir …» de una tarea con fichas** (o el mensaje de un chat): el
/// pedido lleva `imagenes` (y, solo en un `chat`, `archivos`) y el texto
/// renumerado; el titulo, las fichas resaltadas; el subtitulo, cuantas
/// imagenes, el nombre de cada fichero (y que fichas no se encuentran); F1,
/// la primera. Sin fichas no toca nada.
pub fn con_imagenes(r: &mut Resultado, texto: &str, ctx: &Contexto) {
    if fichas(texto).is_empty() && fichas_de_archivo(texto).is_empty() {
        return;
    }
    let es_chat = matches!(&r.accion,
        Accion::Pedido(p) | Accion::PedirYSeguir { pedido: p, .. } if p["accion"] == "chat");
    let mut borrador = ctx
        .raiz_de_datos()
        .and_then(|raiz| leer_borrador(&raiz, ctx.ahora));
    // Los ficheros solo van como adjuntos de un mensaje del chat: en una
    // tarea, una nota o una leccion la ficha se queda como texto.
    let archivos_sueltos: Vec<String> = if es_chat {
        Vec::new()
    } else {
        if let Some(b) = borrador.as_mut() {
            b.archivos.clear();
        }
        fichas_de_archivo(texto)
            .iter()
            .map(|f| texto[f.inicio..f.fin].to_string())
            .collect()
    };
    let mut a = resolver(texto.trim(), borrador.as_ref());
    a.desconocidas.retain(|d| !archivos_sueltos.contains(d));
    if let Accion::Pedido(p) | Accion::PedirYSeguir { pedido: p, .. } = &mut r.accion {
        if !a.rutas.is_empty() || !a.archivos.is_empty() {
            p["texto"] = json!(a.texto);
        }
        if !a.rutas.is_empty() {
            p["imagenes"] = json!(a.rutas);
        }
        if !a.archivos.is_empty() {
            p["archivos"] = json!(a.archivos);
        }
    }
    let cuenta = match a.rutas.len() {
        0 => String::new(),
        1 => "📎 1 imagen".to_string(),
        n => format!("📎 {n} imágenes"),
    };
    let nombres: Vec<String> = a
        .archivos
        .iter()
        .take(NOMBRES_EN_SUBTITULO)
        .map(|r| {
            Path::new(r)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| r.clone())
        })
        .collect();
    let ficheros = match a.archivos.len() {
        0 => String::new(),
        n if n > NOMBRES_EN_SUBTITULO => format!(
            "📄 {} y {} más",
            nombres.join(", "),
            n - NOMBRES_EN_SUBTITULO
        ),
        _ => format!("📄 {}", nombres.join(", ")),
    };
    let aviso = match a.desconocidas.as_slice() {
        [] => String::new(),
        [una] => format!("⚠ {una} no se encuentra: se queda como texto"),
        varias => format!(
            "⚠ {} no se encuentran: se quedan como texto",
            varias.join(" ")
        ),
    };
    let sueltos = if archivos_sueltos.is_empty() {
        String::new()
    } else {
        format!(
            "⚠ los archivos solo van en un mensaje del chat: {} se queda como texto",
            archivos_sueltos.join(" ")
        )
    };
    let previo = std::mem::take(&mut r.subtitulo);
    r.subtitulo = [
        cuenta.as_str(),
        ficheros.as_str(),
        aviso.as_str(),
        sueltos.as_str(),
        previo.as_str(),
    ]
    .into_iter()
    .filter(|t| !t.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    let mut marcas = resaltado(&r.titulo, &a.conocidas);
    marcas.extend(resaltado_de(
        &r.titulo,
        fichas_de_archivo(&r.titulo),
        &a.archivos_conocidos,
    ));
    marcas.sort_unstable();
    r.resaltado = marcas;
    if let Some(primera) = a.rutas.first().or(a.archivos.first()) {
        r.vista_previa = Some(primera.clone());
        r.fichero = Some(primera.clone());
    }
}

/// **«📎 Pegar la imagen copiada → [img NN]»**, si el portapapeles tiene
/// una y la app no esta abierta (si lo esta, Ctrl+V ya la pega: ver el
/// principio del modulo). `busqueda` es lo tecleado tras la palabra clave.
/// Una imagen ya pegada en este borrador (la misma copia del portapapeles)
/// no se vuelve a ofrecer: una imagen, un nombre.
pub fn resultado_pegar(busqueda: &str, ctx: &Contexto) -> Option<Resultado> {
    let secuencia = (ctx.portapapeles.imagen)()?;
    if (ctx.portapapeles.pega_la_app)() {
        return None;
    }
    let numero = siguiente(busqueda);
    let ya_pegada = numero > 1
        && ctx
            .raiz_de_datos()
            .and_then(|raiz| leer_borrador(&raiz, ctx.ahora))
            .is_some_and(|b| b.secuencias.contains(&secuencia) || b.secuencia == Some(secuencia));
    if ya_pegada {
        return None;
    }
    let f = ficha(numero);
    let consulta = ctx.consulta(&format!("{} {f} ", busqueda.trim_end()));
    use crate::consulta::{Funcion, Modo, analizar};
    let en = match analizar(busqueda) {
        Modo::Verbo {
            funcion: Funcion::Leccion,
            ..
        } => "la lección, como foto",
        _ => "la tarea",
    };
    let sub = format!("Intro: la imagen del portapapeles va en {en}; sigue escribiendo detrás");
    let titulo = format!("📎 Pegar la imagen copiada → {f}");
    let mut r = Resultado::nuevo(
        &titulo,
        sub,
        crate::resultados::glifo::ADJUNTAR,
        Accion::PegarImagen { consulta, numero },
    );
    r.resaltado = resaltado(&titulo, &[numero]);
    Some(r)
}

// --- Windows ----------------------------------------------------------------

mod win {
    //! Leer el portapapeles sin esperar: `IsClipboardFormatAvailable` no lo
    //! abre; solo un fichero copiado (`CF_HDROP`) obliga a abrirlo, un
    //! momento, para ver si es una imagen.

    use super::{Copiada, es_imagen_pegable};
    use ::windows::Win32::Foundation::{HANDLE, HGLOBAL};
    use ::windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
        OpenClipboard, RegisterClipboardFormatW,
    };
    use ::windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use ::windows::core::w;
    use std::path::PathBuf;

    const CF_DIB: u32 = 8;
    const CF_HDROP: u32 = 15;
    const CF_DIBV5: u32 = 17;

    /// Los formatos PNG con nombre (Chrome, Edge, Recortes: «PNG»; algunos,
    /// «image/png»).
    fn formatos_png() -> [u32; 2] {
        unsafe {
            [
                RegisterClipboardFormatW(w!("PNG")),
                RegisterClipboardFormatW(w!("image/png")),
            ]
        }
    }

    fn hay(formato: u32) -> bool {
        formato != 0 && unsafe { IsClipboardFormatAvailable(formato).is_ok() }
    }

    pub fn imagen() -> Option<u32> {
        let mapa = formatos_png()
            .into_iter()
            .chain([CF_DIBV5, CF_DIB])
            .any(hay);
        if mapa || (hay(CF_HDROP) && fichero_copiado().is_some()) {
            return Some(unsafe { GetClipboardSequenceNumber() });
        }
        None
    }

    /// El portapapeles abierto mientras vive (y cerrado al soltarlo).
    struct Abierto;
    impl Abierto {
        fn abrir() -> Option<Abierto> {
            unsafe { OpenClipboard(None).ok().map(|_| Abierto) }
        }
        /// Los bytes de ese formato, copiados.
        fn bytes(&self, formato: u32) -> Option<Vec<u8>> {
            unsafe {
                let h: HANDLE = GetClipboardData(formato).ok()?;
                let g = HGLOBAL(h.0);
                let n = GlobalSize(g);
                let p = GlobalLock(g) as *const u8;
                if p.is_null() {
                    return None;
                }
                let v = std::slice::from_raw_parts(p, n).to_vec();
                let _ = GlobalUnlock(g);
                (!v.is_empty()).then_some(v)
            }
        }
    }
    impl Drop for Abierto {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    /// Los ficheros copiados (en el Explorador), en su orden: `CF_HDROP`.
    /// Sin ficheros, o con el portapapeles ocupado, ninguno.
    pub fn ficheros() -> Vec<PathBuf> {
        if !hay(CF_HDROP) {
            return Vec::new();
        }
        Abierto::abrir()
            .and_then(|a| a.bytes(CF_HDROP))
            .map(|b| ficheros_de_drop(&b))
            .unwrap_or_default()
    }

    /// La primera imagen de los ficheros copiados (en el Explorador).
    fn fichero_copiado() -> Option<PathBuf> {
        let bytes = Abierto::abrir()?.bytes(CF_HDROP)?;
        ficheros_de_drop(&bytes)
            .into_iter()
            .find(|f| es_imagen_pegable(f) && f.is_file())
    }

    /// Las rutas de un `DROPFILES` (`pFiles` en el byte 0, `fWide` en el 16).
    pub(crate) fn ficheros_de_drop(b: &[u8]) -> Vec<PathBuf> {
        let leer_u32 = |i: usize| {
            b.get(i..i + 4)
                .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        };
        let (Some(desde), Some(ancho)) = (leer_u32(0), leer_u32(16)) else {
            return Vec::new();
        };
        let resto = b.get(desde as usize..).unwrap_or(&[]);
        let mut v = Vec::new();
        if ancho != 0 {
            let u: Vec<u16> = resto
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            for trozo in u.split(|c| *c == 0) {
                if trozo.is_empty() {
                    break;
                }
                v.push(PathBuf::from(String::from_utf16_lossy(trozo)));
            }
        } else {
            for trozo in resto.split(|c| *c == 0) {
                if trozo.is_empty() {
                    break;
                }
                v.push(PathBuf::from(String::from_utf8_lossy(trozo).to_string()));
            }
        }
        v
    }

    /// Un `.bmp` a partir de un `CF_DIB`: su cabecera de fichero delante.
    pub(crate) fn bmp_de_dib(dib: &[u8]) -> Option<Vec<u8>> {
        let u32_en = |i: usize| {
            dib.get(i..i + 4)
                .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        };
        let tam_cabecera = u32_en(0)? as usize;
        let bits = u16::from_le_bytes([*dib.get(14)?, *dib.get(15)?]) as u32;
        let compresion = u32_en(16)?;
        let usados = u32_en(32).unwrap_or(0);
        let paleta = if usados > 0 {
            usados
        } else if bits <= 8 {
            1 << bits
        } else {
            0
        } as usize;
        // BI_BITFIELDS (3) y BI_ALPHABITFIELDS (6) con la cabecera corta
        // llevan las mascaras detras.
        let mascaras = match (tam_cabecera, compresion) {
            (40, 3) => 12,
            (40, 6) => 16,
            _ => 0,
        };
        let desplazamiento = 14 + tam_cabecera + paleta * 4 + mascaras;
        if tam_cabecera < 12 || desplazamiento > 14 + dib.len() {
            return None;
        }
        let total = 14 + dib.len();
        let mut f = Vec::with_capacity(total);
        f.extend_from_slice(b"BM");
        f.extend_from_slice(&(total as u32).to_le_bytes());
        f.extend_from_slice(&[0; 4]);
        f.extend_from_slice(&(desplazamiento as u32).to_le_bytes());
        f.extend_from_slice(dib);
        Some(f)
    }

    pub fn leer() -> Option<Copiada> {
        if let Some(f) = fichero_copiado() {
            let extension = f.extension()?.to_string_lossy().to_ascii_lowercase();
            return Some(Copiada {
                extension,
                bytes: std::fs::read(&f).ok()?,
            });
        }
        // Copiado mientras esta abierto; lo demas, ya cerrado.
        let dib = {
            let a = Abierto::abrir()?;
            if let Some(png) = formatos_png()
                .into_iter()
                .filter(|f| hay(*f))
                .find_map(|f| a.bytes(f))
            {
                return Some(Copiada {
                    extension: "png".into(),
                    bytes: png,
                });
            }
            a.bytes(CF_DIB)?
        };
        Some(Copiada {
            extension: "bmp".into(),
            bytes: bmp_de_dib(&dib)?,
        })
    }
}

/// Los ficheros copiados en el portapapeles (`CF_HDROP`, lo que deja
/// «Copiar» en el Explorador). Lo usa la app al tragarse un Ctrl+V en Flow.
pub fn ficheros_copiados() -> Vec<PathBuf> {
    win::ficheros()
}

#[cfg(test)]
pub(crate) use win::{bmp_de_dib, ficheros_de_drop};

/// Lo que cuesta mirar el portapapeles de verdad (en cada tecla de una
/// tarea): `cargo test -p pixpin-lanzador cuesta_mirar -- --ignored --nocapture`.
#[cfg(test)]
#[test]
#[ignore]
fn cuesta_mirar_el_portapapeles() {
    let n = 200;
    let inicio = std::time::Instant::now();
    let mut hay = None;
    for _ in 0..n {
        hay = (WINDOWS.imagen)();
    }
    let t = inicio.elapsed();
    println!("portapapeles: {:?} por consulta (imagen: {hay:?})", t / n);
    assert!(t / n < std::time::Duration::from_millis(5));
}
