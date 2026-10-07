//! De lo tecleado a la lista de resultados de Flow.
//!
//! Todo se busca junto: funciones, proyectos, lienzos, notas, archivos,
//! audios y listas de tareas. Cada resultado lleva ya hecho lo que pasa al
//! pulsar Intro ([`Accion`]): un pedido a la app, cambiar lo tecleado en Flow
//! (para entrar en una lista o pedir un nombre) o las dos cosas.

use crate::consulta::{Funcion, Modo, SEPARADOR, analizar};
use crate::datos::{self, Mensaje, Proyecto, Tarea};
use crate::normalizar::{normalizar, puntuar};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Como mucho, cuantos resultados se devuelven.
pub const MAXIMO: usize = 25;
/// Cuantos proyectos recientes salen con `pp` a secas.
const RECIENTES: usize = 6;
/// Puntos de mas para una funcion: «nota» tiene que dar antes la funcion
/// que una nota que se llame «Nota de la reunion».
const EXTRA_FUNCION: u32 = 150;
const EXTRA_PROYECTO: u32 = 20;

/// Iconos de Segoe Fluent Icons.
pub mod glifo {
    pub const CHAT: &str = "\u{E8BD}";
    pub const TAREAS: &str = "\u{E9D5}";
    pub const LIENZO: &str = "\u{E790}";
    pub const NOTA: &str = "\u{E70B}";
    pub const GRABAR: &str = "\u{E720}";
    pub const PIXPIN: &str = "\u{E80F}";
    pub const PROYECTO: &str = "\u{E8B7}";
    pub const ARCHIVO: &str = "\u{E8A5}";
    pub const IMAGEN: &str = "\u{E8B9}";
    pub const AUDIO: &str = "\u{E8D6}";
    pub const PENDIENTE: &str = "\u{E739}";
    pub const HECHA: &str = "\u{E73A}";
    pub const ANADIR: &str = "\u{E710}";
    pub const COPIAR: &str = "\u{E8C8}";
    pub const CARPETA: &str = "\u{E838}";
    pub const AVISO: &str = "\u{E7BA}";
    pub const ABRIR: &str = "\u{E8A7}";
    pub const PIN: &str = "\u{E718}";
    pub const WINDOWS: &str = "\u{E7AC}";
    pub const MINIAPP: &str = "\u{E71D}";
    pub const ADJUNTAR: &str = "\u{E723}";
    /// La bombilla (`Lightbulb`), el 💡 de las lecciones en el movil.
    pub const LECCION: &str = "\u{E82F}";
    /// La camara (`Camera`): capturar.
    pub const CAPTURA: &str = "\u{E722}";
    /// `PhotoCollection`: la galeria de capturas.
    pub const GALERIA: &str = "\u{E9A2}";
    /// `MoveToFolder`: mover una tarea a otra lista.
    pub const MOVER: &str = "\u{E8DE}";
    pub const BORRAR: &str = "\u{E74D}";
    /// `Save`: conservar una captura.
    pub const CONSERVAR: &str = "\u{E74E}";
    /// `Video`.
    pub const VIDEO: &str = "\u{E714}";
    /// `NewWindow`: abrir una ventana de la app.
    pub const VENTANA: &str = "\u{E78B}";
    /// `CellPhone`: mandar al movil.
    pub const MOVIL: &str = "\u{E8EA}";
}

/// El pedido `soltar`: el recuadro flotante donde se sueltan archivos, que
/// van al chat de `p` (`None`: Mensajes guardados).
pub(crate) fn pedido_soltar(p: Option<&Proyecto>) -> Value {
    pedido(
        "soltar",
        json!({ "proyecto": p.map(Proyecto::id_para_pedido).unwrap_or(Value::Null) }),
    )
}

/// «Añadir arrastrando (recuadro flotante)» a ese proyecto (el del chat y
/// el del menu).
pub(crate) fn resultado_arrastrar(p: Option<&Proyecto>) -> Resultado {
    let nombre = p
        .map(|p| p.nombre.as_str())
        .unwrap_or(datos::NOMBRE_GUARDADOS);
    Resultado::nuevo(
        "Añadir arrastrando (recuadro flotante)",
        format!("Suelta archivos en el recuadro y van a «{nombre}»"),
        glifo::ADJUNTAR,
        Accion::Pedido(pedido_soltar(p)),
    )
}

/// Lo que pasa al pulsar Intro.
#[derive(Debug, Clone, PartialEq)]
pub enum Accion {
    /// Mandar el pedido a la app y cerrar Flow.
    Pedido(Value),
    /// Cambiar lo tecleado en Flow (sin cerrarlo).
    Consulta(String),
    /// Mandar el pedido y despues cambiar lo tecleado, sin cerrar Flow (las
    /// tareas: se marcan y la lista se queda a la vista).
    PedirYSeguir { pedido: Value, consulta: String },
    /// Copiar al portapapeles (lo hace Flow).
    Copiar(String),
    /// Abrir la carpeta en el Explorador (lo hace Flow).
    Carpeta { carpeta: String, fichero: String },
    /// Abrir el fichero con su programa de Windows (lo hace el plugin, con
    /// `explorer.exe`).
    Windows(String),
    /// Guardar la imagen del portapapeles como la ficha `[img NN]` y cambiar
    /// lo tecleado a `consulta` (que ya la lleva), sin cerrar Flow
    /// ([`crate::imagenes`]).
    PegarImagen { consulta: String, numero: u32 },
}

impl Accion {
    fn metodo(&self) -> (&'static str, Vec<Value>, bool) {
        match self {
            Accion::Pedido(p) => ("pedir", vec![p.clone()], false),
            Accion::Consulta(c) => ("consulta", vec![json!(c)], true),
            Accion::PedirYSeguir { pedido, consulta } => (
                "pedir_y_seguir",
                vec![pedido.clone(), json!(consulta)],
                true,
            ),
            Accion::Copiar(t) => ("copiar", vec![json!(t)], false),
            Accion::Carpeta { carpeta, fichero } => {
                ("carpeta", vec![json!(carpeta), json!(fichero)], false)
            }
            Accion::Windows(r) => ("windows", vec![json!(r)], false),
            Accion::PegarImagen { consulta, numero } => {
                ("pegar_imagen", vec![json!(consulta), json!(numero)], true)
            }
        }
    }

    /// La accion guardada en un `contextData` (para «Abrir» en el menu).
    pub fn a_valor(&self) -> Value {
        let (metodo, parametros, _) = self.metodo();
        json!({ "metodo": metodo, "parametros": parametros })
    }

    /// La de [`Accion::a_valor`], de vuelta.
    pub fn de_valor(v: &Value) -> Option<Accion> {
        let metodo = v.get("metodo")?.as_str()?;
        let p = v.get("parametros")?.as_array()?;
        let texto = |i: usize| p.get(i).and_then(Value::as_str).map(str::to_string);
        Some(match metodo {
            "pedir" => Accion::Pedido(p.first()?.clone()),
            "consulta" => Accion::Consulta(texto(0)?),
            "pedir_y_seguir" => Accion::PedirYSeguir {
                pedido: p.first()?.clone(),
                consulta: texto(1)?,
            },
            "copiar" => Accion::Copiar(texto(0)?),
            "carpeta" => Accion::Carpeta {
                carpeta: texto(0)?,
                fichero: texto(1).unwrap_or_default(),
            },
            "windows" => Accion::Windows(texto(0)?),
            "pegar_imagen" => Accion::PegarImagen {
                consulta: texto(0)?,
                numero: p.get(1)?.as_u64()? as u32,
            },
            _ => return None,
        })
    }
}

/// Un resultado, antes de pasarlo a JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct Resultado {
    pub titulo: String,
    pub subtitulo: String,
    pub glifo: &'static str,
    pub accion: Accion,
    pub autocompletar: Option<String>,
    pub copiar: Option<String>,
    /// Lo que llega a `context_menu`.
    pub contexto: Option<Value>,
    /// Su propio icono (el de la extension de un archivo, o la foto misma):
    /// entonces no lleva glifo, que Flow pondria por encima.
    pub icono: Option<String>,
    /// Las letras del titulo que coinciden con lo tecleado (`titleHighlightData`:
    /// posiciones en UTF-16, como las cuenta Flow).
    pub resaltado: Vec<usize>,
    /// El texto entero al pasar el raton por el titulo (`titleToolTip`): la
    /// ruta de un fichero. `None`: Flow ensena el titulo.
    pub ayuda_titulo: Option<String>,
    /// Lo mismo para el subtitulo (`subTitleToolTip`): el texto entero.
    pub ayuda_subtitulo: Option<String>,
    /// La clave estable con que Flow aprende lo que se elige (`recordKey`).
    pub clave: Option<String>,
    /// La imagen que se ensena en grande en el panel de vista previa (F1):
    /// `preview.previewImagePath`.
    pub vista_previa: Option<String>,
    /// El fichero de este equipo, para la vista previa de fuera
    /// (`preview.filePath`: QuickLook y parecidos).
    pub fichero: Option<String>,
    /// La barra de progreso (`progressBar`, 0-100): lo hecho de una lista.
    pub progreso: Option<u8>,
}

impl Resultado {
    pub(crate) fn nuevo(
        titulo: impl Into<String>,
        subtitulo: impl Into<String>,
        glifo: &'static str,
        accion: Accion,
    ) -> Self {
        Resultado {
            titulo: titulo.into(),
            subtitulo: subtitulo.into(),
            glifo,
            accion,
            autocompletar: None,
            copiar: None,
            contexto: None,
            icono: None,
            resaltado: Vec::new(),
            ayuda_titulo: None,
            ayuda_subtitulo: None,
            clave: None,
            vista_previa: None,
            fichero: None,
            progreso: None,
        }
    }

    /// Pone la ruta de un fichero de este equipo: el texto de ayuda del
    /// titulo, la vista previa de fuera y, si es una imagen, la grande.
    pub(crate) fn con_fichero(&mut self, ruta: &std::path::Path) {
        let r = ruta.to_string_lossy().to_string();
        if es_imagen(ruta) {
            self.vista_previa = Some(r.clone());
        }
        self.ayuda_titulo = Some(r.clone());
        self.fichero = Some(r);
    }

    /// Pone el `contextData`: `campos` y, en `abrir`, lo que hace Intro.
    pub(crate) fn con_menu(&mut self, campos: Value) {
        self.contexto = Some(extender(json!({ "abrir": self.accion.a_valor() }), campos));
    }

    /// El resultado como lo quiere Flow (camelCase). `puntos` ordena: Flow
    /// ensena de mas a menos.
    pub fn a_json(&self, icono: &str, puntos: i64) -> Value {
        let (metodo, parametros, sin_cerrar) = self.accion.metodo();
        let mut r = json!({
            "title": self.titulo,
            "subTitle": self.subtitulo,
            "icoPath": self.icono.as_deref().unwrap_or(icono),
            "score": puntos,
            "jsonRPCAction": {
                "method": metodo,
                "parameters": parametros,
                "dontHideAfterAction": sin_cerrar,
            },
        });
        if self.icono.is_none() {
            r["glyph"] = json!({ "glyph": self.glifo, "fontFamily": "Segoe Fluent Icons" });
        }
        if let Some(a) = &self.autocompletar {
            r["autoCompleteText"] = json!(a);
        }
        if let Some(c) = &self.copiar {
            r["copyText"] = json!(c);
        }
        if let Some(c) = &self.contexto {
            r["contextData"] = c.clone();
        }
        if !self.resaltado.is_empty() {
            r["titleHighlightData"] = json!(self.resaltado);
        }
        if let Some(t) = self.ayuda_titulo.as_deref().filter(|t| *t != self.titulo) {
            r["titleToolTip"] = json!(t);
        }
        if let Some(t) = self
            .ayuda_subtitulo
            .as_deref()
            .filter(|t| *t != self.subtitulo)
        {
            r["subTitleToolTip"] = json!(t);
        }
        if let Some(k) = &self.clave {
            r["recordKey"] = json!(k);
        }
        if self.vista_previa.is_some() || self.fichero.is_some() {
            let mut v = json!({});
            if let Some(i) = &self.vista_previa {
                v["previewImagePath"] = json!(i);
                v["isMedia"] = json!(true);
            }
            if let Some(f) = &self.fichero {
                v["filePath"] = json!(f);
            }
            r["preview"] = v;
        }
        // `progreso` NO va como `progressBar`: Flow 2.1.4 pinta la barra DETRAS
        // del titulo, con la parte vacia blanca, y el titulo (blanco en el tema
        // oscuro) no se leia (el usuario, 3-oct). Lo hecho ya lo dice el
        // subtitulo («2 pendientes de 3»).
        r
    }
}

/// La lista entera para Flow, con los puntos que mantienen el orden. De
/// cien en cien: Flow suma algo por cada vez que se eligio un resultado, y
/// asi eso no desordena una lista de tareas.
pub fn lista_json(resultados: &[Resultado], icono: &str) -> Vec<Value> {
    let n = resultados.len() as i64;
    resultados
        .iter()
        .enumerate()
        .map(|(i, r)| r.a_json(icono, (n - i as i64) * 100))
        .collect()
}

/// Un pedido del protocolo v1.
pub fn pedido(accion: &str, campos: Value) -> Value {
    let mut p = json!({ "pixpin": 1, "accion": accion });
    if let (Some(p), Value::Object(c)) = (p.as_object_mut(), campos) {
        p.extend(c);
    }
    p
}

/// Si el fichero es un audio que suena en el reproductor flotante: las
/// extensiones que Media Foundation abre en cualquier Windows 10/11.
pub fn es_audio(ruta: &std::path::Path) -> bool {
    ruta.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|e| {
            matches!(
                e.as_str(),
                "m4a" | "mp3" | "wav" | "aac" | "wma" | "flac" | "ogg" | "opus" | "3gp" | "amr"
            )
        })
}

/// Si el fichero es una imagen que Flow sabe ensenar (icono y vista previa).
pub fn es_imagen(ruta: &std::path::Path) -> bool {
    ruta.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|e| {
            matches!(
                e.as_str(),
                "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "ico" | "tif" | "tiff"
            )
        })
}

/// Un texto largo cortado para el texto de ayuda (con «…» si se corto).
pub(crate) fn para_ayuda(texto: &str) -> String {
    const N: usize = 1000;
    let t = texto.trim();
    if t.chars().count() <= N {
        return t.to_string();
    }
    let mut c: String = t.chars().take(N).collect();
    c.push('…');
    c
}

/// `a` con los campos de `b` encima (dos objetos JSON).
pub(crate) fn extender(mut a: Value, b: Value) -> Value {
    if let (Some(a), Value::Object(b)) = (a.as_object_mut(), b) {
        a.extend(b);
    }
    a
}

/// Lo que hace falta para escribir los resultados.
#[derive(Debug, Clone)]
pub struct Contexto {
    /// Lo que va delante en Flow: `"p "`, o vacio si el plugin es global.
    pub prefijo: String,
    pub ahora: i64,
    /// La carpeta de los iconos de extension (`iconos::carpeta`); `None`:
    /// todos con su glifo.
    pub iconos: Option<PathBuf>,
    /// Las extensiones cuyo icono se busco y no estaba (para pedirlo).
    pub faltan: RefCell<BTreeSet<String>>,
    /// La raiz de los datos de PixPin (para las capturas). `None`: se saca
    /// de [`Contexto::iconos`], que vive en `<raiz>/cache/...`.
    pub raiz: Option<PathBuf>,
    /// El portapapeles, para ofrecer pegar una imagen en una tarea (en las
    /// pruebas y por defecto, uno que nunca tiene).
    pub portapapeles: crate::imagenes::Portapapeles,
}

impl Contexto {
    pub fn nuevo(palabra_clave: &str) -> Self {
        let k = palabra_clave.trim();
        let prefijo = if k.is_empty() || k == "*" {
            String::new()
        } else {
            format!("{k} ")
        };
        Contexto::con(prefijo, datos::ahora_ms())
    }

    pub fn con(prefijo: impl Into<String>, ahora: i64) -> Self {
        Contexto {
            prefijo: prefijo.into(),
            ahora,
            iconos: None,
            faltan: RefCell::new(BTreeSet::new()),
            raiz: None,
            portapapeles: crate::imagenes::SIN_PORTAPAPELES,
        }
    }

    /// La raiz de los datos: la puesta, o la de la carpeta de los iconos
    /// (`<raiz>/cache/iconos-de-extension`).
    pub fn raiz_de_datos(&self) -> Option<PathBuf> {
        self.raiz.clone().or_else(|| {
            self.iconos
                .as_deref()
                .and_then(|i| i.parent())
                .and_then(|c| c.parent())
                .map(PathBuf::from)
        })
    }

    pub(crate) fn consulta(&self, texto: &str) -> String {
        format!("{}{}", self.prefijo, texto)
    }

    /// El icono de esa extension si la app ya lo pinto; si no, se apunta.
    pub fn icono_de_extension(&self, ext: &str) -> Option<String> {
        let dir = self.iconos.as_ref()?;
        let png = dir.join(format!("{ext}.png"));
        if crate::datos::existe(&png) {
            return Some(png.to_string_lossy().to_string());
        }
        self.faltan.borrow_mut().insert(ext.to_string());
        None
    }

    /// Las que faltaron, en orden.
    pub fn extensiones_que_faltan(&self) -> Vec<String> {
        self.faltan.borrow().iter().cloned().collect()
    }
}

/// «hoy», «ayer», «hace 3 dias»...
pub fn hace(ms: i64, ahora: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let dias = (ahora - ms).max(0) / 86_400_000;
    match dias {
        0 => "hoy".into(),
        1 => "ayer".into(),
        2..=29 => format!("hace {dias} días"),
        30..=59 => "hace un mes".into(),
        60..=364 => format!("hace {} meses", dias / 30),
        365..=729 => "hace un año".into(),
        _ => format!("hace {} años", dias / 365),
    }
}

fn unir(trozos: &[&str]) -> String {
    trozos
        .iter()
        .filter(|t| !t.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ")
}

// --- Lo que se busca ------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    Funcion,
    Proyecto,
    Lienzo,
    Nota,
    Archivo,
    Audio,
    Lista,
}

/// Una cosa buscable, con su resultado ya hecho.
#[derive(Debug, Clone)]
struct Cosa {
    tipo: Tipo,
    /// Normalizados.
    nombres: Vec<String>,
    dentro: String,
    cuando: i64,
    resultado: Resultado,
}

fn resultado_funcion(f: Funcion, ctx: &Contexto) -> Resultado {
    let (titulo, sub, glifo, accion) = match f {
        Funcion::Chat => (
            "Chat",
            "Escribe y pulsa Intro: va al chat de PixPin",
            glifo::CHAT,
            Accion::Consulta(ctx.consulta("chat ")),
        ),
        Funcion::Tareas => (
            "Tareas",
            "El Inbox y tus listas · «t <tarea>» apunta una en el Inbox",
            glifo::TAREAS,
            Accion::Consulta(ctx.consulta("tareas ")),
        ),
        Funcion::Lienzo => (
            "Lienzo nuevo",
            "Abre un lienzo en blanco · «l <nombre>» para nombrarlo",
            glifo::LIENZO,
            Accion::Pedido(pedido("lienzo_nuevo", json!({}))),
        ),
        Funcion::Nota => (
            "Nota nueva",
            "Abre el editor de notas · «n <texto>» para empezarla",
            glifo::NOTA,
            Accion::Pedido(pedido("nota_nueva", json!({}))),
        ),
        Funcion::Grabar => (
            "Grabar audio",
            "Ponle nombre y pulsa Intro: sale un micrófono flotante que graba",
            glifo::GRABAR,
            Accion::Consulta(ctx.consulta("grabar ")),
        ),
        Funcion::Soltar => (
            "Añadir a proyecto",
            "Un recuadro flotante: suelta archivos en él y van al chat · «añadir <proyecto>»",
            glifo::ADJUNTAR,
            Accion::Consulta(ctx.consulta("añadir ")),
        ),
        Funcion::Leccion => (
            "Nueva lección",
            "Apunta lo que aprendiste · «a <texto>»",
            glifo::LECCION,
            Accion::Consulta(ctx.consulta("a ")),
        ),
        Funcion::Lecciones => (
            "Lecciones aprendidas",
            "Búscalas aquí y abre su ficha · «lecciones <texto>»",
            glifo::LECCION,
            Accion::Consulta(ctx.consulta("lecciones ")),
        ),
        Funcion::Repasar => (
            "Repasar hoy",
            "Las lecciones que tocan repasar hoy, como en la app",
            glifo::LECCION,
            Accion::Consulta(ctx.consulta("repasar ")),
        ),
        Funcion::Capturar => (
            "Capturar zona",
            "Recorta una zona de la pantalla · «c»",
            glifo::CAPTURA,
            Accion::Pedido(pedido_capturar()),
        ),
        Funcion::Capturas => (
            "Capturas",
            "Las últimas, para buscarlas y pinearlas",
            glifo::IMAGEN,
            Accion::Consulta(ctx.consulta("capturas ")),
        ),
        Funcion::Galeria => (
            "Galería de capturas",
            "Abre la galería · «g»",
            glifo::GALERIA,
            Accion::Pedido(pedido_ventana("galeria")),
        ),
        Funcion::Ultima => (
            "Última captura",
            "Pinéala · «u»",
            glifo::PIN,
            Accion::Pedido(pedido("pinear_ultima", json!({}))),
        ),
        Funcion::Movil => (
            "Enviar al móvil",
            "Pega una imagen (Ctrl+V) y elige el móvil: va al lienzo que tenga abierto · «m»",
            glifo::MOVIL,
            Accion::Consulta(ctx.consulta("móvil ")),
        ),
        Funcion::Abrir => (
            "Abrir PixPin",
            "Saca la ventana del chat",
            glifo::PIXPIN,
            Accion::Pedido(pedido("ventana_principal", json!({}))),
        ),
    };
    let mut r = Resultado::nuevo(titulo, sub, glifo, accion);
    r.autocompletar = Some(ctx.consulta(&format!("{} ", f.verbo())));
    r.clave = Some(format!("funcion/{}", f.alias()[0]));
    r
}

/// El pedido `ventana`: una ventana de la app (`tareas`, `galeria`,
/// `lecciones`).
pub(crate) fn pedido_ventana(cual: &str) -> Value {
    pedido("ventana", json!({ "cual": cual }))
}

/// El pedido de recortar una zona de la pantalla.
pub(crate) fn pedido_capturar() -> Value {
    pedido("capturar", json!({ "modo": "zona" }))
}

/// «Abrir la ventana de …»: una ventana de la app, la misma fila siempre.
pub(crate) fn resultado_ventana(
    cual: &str,
    titulo: &str,
    sub: &str,
    glifo: &'static str,
) -> Resultado {
    let mut r = Resultado::nuevo(titulo, sub, glifo, Accion::Pedido(pedido_ventana(cual)));
    r.clave = Some(format!("ventana/{cual}"));
    r
}

/// Un proyecto: Intro entra en su chat (`pp <proyecto> > `); abrirlo en la
/// app va en el menu y en el chat, arriba del todo.
pub(crate) fn resultado_proyecto(p: &Proyecto, etiqueta: &str, ctx: &Contexto) -> Resultado {
    let hojas = match p.hojas {
        0 => String::new(),
        1 => "1 hoja".into(),
        n => format!("{n} hojas"),
    };
    let tocado = hace(p.tocado, ctx.ahora);
    let tipo = if p.guardados { "Chat" } else { "Proyecto" };
    let entrar = crate::chat::consulta_de(etiqueta, ctx);
    let mut r = Resultado::nuevo(
        &p.nombre,
        unir(&[tipo, &hojas, &tocado]),
        glifo::PROYECTO,
        Accion::Consulta(entrar.clone()),
    );
    r.autocompletar = Some(entrar.clone());
    r.copiar = Some(p.nombre.clone());
    r.clave = Some(format!("proyecto/{}", p.id));
    r.ayuda_subtitulo = Some(p.carpeta.to_string_lossy().to_string());
    r.con_menu(json!({
        "tipo": "proyecto",
        "proyecto": p.id_para_pedido(),
        "carpeta": p.carpeta.to_string_lossy(),
        "consulta": entrar,
    }));
    r
}

pub(crate) fn abrir_mensaje(p: &Proyecto, m: &Mensaje) -> Accion {
    Accion::Pedido(pedido(
        "abrir",
        json!({ "que": { "tipo": "mensaje", "proyecto": p.id_para_pedido(), "codigo": m.id } }),
    ))
}

pub(crate) fn abrir_proyecto(p: &Proyecto) -> Accion {
    Accion::Pedido(pedido(
        "abrir",
        json!({ "que": { "tipo": "proyecto", "proyecto": p.id_para_pedido() } }),
    ))
}

/// El nombre que se ensena de un lienzo.
fn nombre_de_lienzo(p: &Proyecto, m: &Mensaje) -> String {
    let referencia = m.referencia.as_deref().unwrap_or("");
    if let Some(h) = p
        .hojas_json
        .iter()
        .find(|h| h.dibujo.as_deref() == Some(referencia) || h.id == referencia)
    {
        if !h.nombre.trim().is_empty() {
            return h.nombre.trim().to_string();
        }
    }
    let n = m.nombre();
    let n = n.strip_suffix(".excalidraw").unwrap_or(n).trim();
    if n.is_empty() {
        "Lienzo".into()
    } else {
        n.to_string()
    }
}

/// La primera linea con texto, sin `#` delante, cortada a `n` letras (con
/// «…» si se corto).
pub(crate) fn resumen(texto: &str, n: usize) -> Option<String> {
    let l = texto
        .lines()
        .map(|l| {
            l.trim()
                .trim_start_matches('\u{feff}')
                .trim_start_matches('#')
                .trim()
        })
        .find(|l| !l.is_empty())?;
    if l.chars().count() <= n {
        return Some(l.to_string());
    }
    let mut corto: String = l.chars().take(n).collect::<String>().trim_end().to_string();
    corto.push('…');
    Some(corto)
}

/// `0:15`, `12:03`, `1:02:03`.
// `!(ms > 0.0)` es a proposito: tambien atrapa un NaN.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn duracion(ms: f64) -> String {
    if !(ms > 0.0) {
        return String::new();
    }
    let s = (ms / 1000.0).round() as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Donde se ensena: en la busqueda de todo, o dentro del chat de un
/// proyecto (de lo ultimo a lo primero, todas las clases).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Vista {
    Todo,
    Chat,
}

/// Un mensaje hecho resultado, con lo que se busca en el.
pub(crate) struct Pieza {
    pub tipo: Tipo,
    pub nombre: String,
    pub dentro: String,
    pub resultado: Resultado,
}

/// **Un mensaje del chat como resultado.** En [`Vista::Todo`] solo los que
/// se buscan por su nombre (lienzos, notas, archivos, fotos y audios); en
/// [`Vista::Chat`] todos, como el chat.
pub(crate) fn de_mensaje(
    proyectos: &[Proyecto],
    i: usize,
    m: &Mensaje,
    vista: Vista,
    todas: &[Lista],
    ctx: &Contexto,
) -> Option<Pieza> {
    let mut pz = de_mensaje_sin_clave(proyectos, i, m, vista, todas, ctx)?;
    if pz.resultado.clave.is_none() {
        pz.resultado.clave = Some(format!("mensaje/{}/{}", proyectos[i].id, m.id));
    }
    Some(pz)
}

fn de_mensaje_sin_clave(
    proyectos: &[Proyecto],
    i: usize,
    m: &Mensaje,
    vista: Vista,
    todas: &[Lista],
    ctx: &Contexto,
) -> Option<Pieza> {
    let p = &proyectos[i];
    // Las lecciones y sus fotos no son del chat (`LeccionesStore.sinLecciones`):
    // salen como lecciones (`crate::lecciones`).
    if m.es_de_leccion() {
        return None;
    }
    let chat = vista == Vista::Chat;
    let cuando = if chat {
        crate::fecha::momento(m.cuando, ctx.ahora)
    } else {
        hace(m.cuando, ctx.ahora)
    };
    let donde = if chat { "" } else { p.nombre.as_str() };
    let base = json!({
        "tipo": "mensaje",
        "proyecto": p.id_para_pedido(),
        "codigo": m.id,
        "carpeta": p.carpeta.to_string_lossy(),
    });
    let pin_del_mensaje = pedido(
        "pinear",
        json!({ "proyecto": p.id_para_pedido(), "codigo": m.id }),
    );
    let pieza = |tipo, nombre: String, dentro: String, resultado| {
        Some(Pieza {
            tipo,
            nombre,
            dentro,
            resultado,
        })
    };
    match m.clase.as_str() {
        "DIBUJO" | "PAGINA" => {
            let referencia = m.referencia.as_deref().filter(|r| !r.is_empty());
            if referencia.is_none() && !chat {
                return None;
            }
            let nombre = nombre_de_lienzo(p, m);
            let tipo = if m.clase == "PAGINA" {
                "Hoja"
            } else {
                "Lienzo"
            };
            let accion = match referencia {
                Some(r) => Accion::Pedido(pedido(
                    "abrir",
                    json!({ "que": { "tipo": "hoja", "proyecto": p.id_para_pedido(), "referencia": r } }),
                )),
                None => abrir_mensaje(p, m),
            };
            let mut r = Resultado::nuevo(
                &nombre,
                unir(&[tipo, donde, &cuando]),
                glifo::LIENZO,
                accion,
            );
            r.copiar = Some(nombre.clone());
            r.con_menu(extender(base, json!({ "pin": pin_del_mensaje })));
            pieza(Tipo::Lienzo, nombre, String::new(), r)
        }
        "NOTA" => {
            let texto = m.texto();
            // Una nota de verdad tiene su `.md` en `notas/`; lo demas es un
            // mensaje de texto.
            let con_md = p.notas_md.iter().any(|n| {
                let raiz = n
                    .ruta
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                raiz == m.id || m.referencia.as_deref() == Some(raiz.as_str())
            });
            let titulo = if chat {
                resumen(texto, 120).unwrap_or_else(|| {
                    if m.nombre().is_empty() {
                        "Mensaje".into()
                    } else {
                        m.nombre().into()
                    }
                })
            } else if m.nombre().is_empty() {
                datos::primera_linea(texto).unwrap_or_else(|| "Nota".into())
            } else {
                m.nombre().to_string()
            };
            let abrir_nota = Accion::Pedido(pedido(
                "abrir",
                json!({ "que": { "tipo": "nota", "proyecto": p.id_para_pedido(), "codigo": m.id } }),
            ));
            let (etiqueta, accion, glifo) = if chat && !con_md {
                ("Mensaje", abrir_mensaje(p, m), glifo::CHAT)
            } else {
                ("Nota", abrir_nota, glifo::NOTA)
            };
            let mut r = Resultado::nuevo(&titulo, unir(&[etiqueta, donde, &cuando]), glifo, accion);
            r.copiar = Some(texto.to_string());
            r.ayuda_subtitulo = Some(para_ayuda(texto));
            r.con_menu(extender(
                base,
                json!({ "texto": texto, "pin": pin_del_mensaje }),
            ));
            pieza(Tipo::Nota, titulo, texto.to_string(), r)
        }
        "ARCHIVO" | "IMAGEN" | "VOZ" => {
            let ruta = m
                .ruta
                .as_deref()
                .and_then(|r| p.ruta_real(r))
                .filter(|r| crate::datos::existe(r));
            let de_ruta = m
                .ruta
                .as_deref()
                .and_then(|r| r.rsplit(['/', '\\']).next())
                .unwrap_or("")
                .to_string();
            let (tipo, glifo, etiqueta) = match m.clase.as_str() {
                "VOZ" => (Tipo::Audio, glifo::AUDIO, "Audio"),
                "IMAGEN" => (Tipo::Archivo, glifo::IMAGEN, "Foto"),
                _ => (Tipo::Archivo, glifo::ARCHIVO, "Archivo"),
            };
            let etiqueta = if !chat && m.clase == "IMAGEN" {
                "Imagen"
            } else {
                etiqueta
            };
            let transcripcion = m.transcripcion.as_deref().unwrap_or("").trim();
            let titulo = if !m.nombre().is_empty() {
                m.nombre().to_string()
            } else if tipo == Tipo::Audio {
                match resumen(transcripcion, 80).filter(|_| chat) {
                    Some(t) => t,
                    // Sin nombre no hay forma de encontrarlo: se dice que es.
                    None if chat => "Nota de voz".into(),
                    None => {
                        let fecha = if cuando.is_empty() {
                            String::new()
                        } else {
                            format!(" ({cuando})")
                        };
                        format!("Nota de voz sin nombre{fecha}")
                    }
                }
            } else if !de_ruta.is_empty() {
                de_ruta.clone()
            } else {
                etiqueta.to_string()
            };
            let falta = if ruta.is_none() {
                "no está en este equipo"
            } else {
                ""
            };
            let largo = if chat && tipo == Tipo::Audio {
                duracion(m.duracion_ms.unwrap_or(0.0))
            } else {
                String::new()
            };
            // Un audio suena ahi mismo, en el reproductor flotante de la app,
            // sin abrir el chat ni el reproductor de Windows.
            let suena = tipo == Tipo::Audio || ruta.as_deref().is_some_and(es_audio);
            let accion = match &ruta {
                Some(r) if suena => Accion::Pedido(pedido(
                    "reproducir",
                    json!({ "ruta": r.to_string_lossy(), "titulo": titulo }),
                )),
                Some(r) => Accion::Pedido(pedido(
                    "abrir",
                    json!({ "que": { "tipo": "fichero", "ruta": r.to_string_lossy() } }),
                )),
                None => abrir_mensaje(p, m),
            };
            let mut r = Resultado::nuevo(
                &titulo,
                unir(&[etiqueta, donde, &largo, &cuando, falta]),
                glifo,
                accion,
            );
            // El icono: la foto misma (Flow la ensena en pequeno) o el de su
            // extension, como en el chat. Los audios, su glifo.
            r.icono = match m.clase.as_str() {
                "IMAGEN" => ruta.as_ref().map(|r| r.to_string_lossy().to_string()),
                "ARCHIVO" => {
                    let nombre_real = ruta
                        .as_ref()
                        .and_then(|r| r.file_name())
                        .map(|n| n.to_string_lossy().to_string());
                    [
                        nombre_real.as_deref(),
                        Some(m.nombre()),
                        Some(de_ruta.as_str()),
                    ]
                    .into_iter()
                    .flatten()
                    .find_map(crate::iconos::extension)
                    .and_then(|e| ctx.icono_de_extension(&e))
                }
                _ => None,
            };
            let copiar = ruta
                .as_ref()
                .map(|r| r.to_string_lossy().to_string())
                .unwrap_or_else(|| titulo.clone());
            r.copiar = Some(copiar);
            if let Some(rr) = &ruta {
                r.con_fichero(rr);
            }
            if !transcripcion.is_empty() {
                r.ayuda_subtitulo = Some(para_ayuda(transcripcion));
            }
            let mut campos = json!({ "texto": transcripcion });
            if let Some(rr) = &ruta {
                campos["ruta"] = json!(rr.to_string_lossy());
                if tipo != Tipo::Audio {
                    campos["pin"] = pedido("pinear", json!({ "ruta": rr.to_string_lossy() }));
                }
            }
            r.con_menu(extender(base, campos));
            pieza(tipo, titulo, transcripcion.to_string(), r)
        }
        _ if !chat => None,
        "MINIAPP" if m.es_lista_de_tareas() => {
            let l = todas.iter().find(|l| l.proyecto == i && l.codigo == m.id)?;
            let mut r = resultado_lista(l, proyectos, ctx);
            r.subtitulo = unir(&["Lista de tareas", &cuenta_de(l), &cuando]);
            let dentro: Vec<&str> = l.tareas.iter().map(|t| t.texto.as_str()).collect();
            pieza(Tipo::Lista, l.titulo.clone(), dentro.join("\n"), r)
        }
        "MINIAPP" => {
            let mut titulo = datos::titulo_del_documento(m.texto());
            if titulo.is_empty() {
                titulo = m.nombre().to_string();
            }
            if titulo.is_empty() {
                titulo = "Mini-app".into();
            }
            let cual = m.miniapp.as_deref().unwrap_or("");
            let mut r = Resultado::nuevo(
                &titulo,
                unir(&["Mini-app", cual, &cuando]),
                glifo::MINIAPP,
                abrir_mensaje(p, m),
            );
            r.copiar = Some(m.texto().to_string());
            r.ayuda_subtitulo = Some(para_ayuda(m.texto()));
            r.con_menu(extender(base, json!({ "texto": m.texto() })));
            pieza(Tipo::Nota, titulo, m.texto().to_string(), r)
        }
        "PROYECTO" => {
            // El acceso directo a otro proyecto: entra en su chat.
            let destino = m
                .referencia
                .as_deref()
                .and_then(|id| proyectos.iter().position(|q| q.id == id));
            let titulo = match (m.nombre(), destino) {
                ("", Some(j)) => proyectos[j].nombre.clone(),
                ("", None) => "Proyecto".into(),
                (n, _) => n.to_string(),
            };
            let accion = match destino {
                Some(j) => Accion::Consulta(crate::chat::consulta_de(
                    &crate::chat::etiquetas(proyectos)[j],
                    ctx,
                )),
                None => abrir_mensaje(p, m),
            };
            let mut r = Resultado::nuevo(
                &titulo,
                unir(&["Proyecto", &cuando]),
                glifo::PROYECTO,
                accion,
            );
            r.con_menu(base);
            pieza(Tipo::Proyecto, titulo, String::new(), r)
        }
        otra => {
            let titulo = resumen(m.texto(), 120)
                .or_else(|| (!m.nombre().is_empty()).then(|| m.nombre().to_string()))
                .unwrap_or_else(|| otra.to_string());
            let mut r = Resultado::nuevo(
                &titulo,
                unir(&["Mensaje", &cuando]),
                glifo::CHAT,
                abrir_mensaje(p, m),
            );
            r.ayuda_subtitulo = Some(para_ayuda(m.texto()));
            r.con_menu(extender(base, json!({ "texto": m.texto() })));
            pieza(Tipo::Nota, titulo, m.texto().to_string(), r)
        }
    }
}

fn cosas_de_proyecto(proyectos: &[Proyecto], i: usize, ctx: &Contexto, salida: &mut Vec<Cosa>) {
    let p = &proyectos[i];
    let mut cosa = |tipo, nombre: &str, dentro: &str, cuando, resultado| {
        salida.push(Cosa {
            tipo,
            nombres: vec![normalizar(nombre)],
            dentro: normalizar(dentro),
            cuando,
            resultado,
        })
    };
    let mut referencias = Vec::new();
    for m in &p.mensajes {
        if matches!(m.clase.as_str(), "DIBUJO" | "PAGINA" | "NOTA") {
            if let Some(r) = m.referencia.as_deref().filter(|r| !r.is_empty()) {
                referencias.push(r.to_string());
            }
        }
        if let Some(pz) = de_mensaje(proyectos, i, m, Vista::Todo, &[], ctx) {
            cosa(pz.tipo, &pz.nombre, &pz.dentro, m.cuando, pz.resultado);
        }
    }
    // Las hojas con dibujo que no tienen su mensaje en el chat.
    for h in &p.hojas_json {
        let Some(dibujo) = h.dibujo.as_deref().filter(|d| !d.is_empty()) else {
            continue;
        };
        if h.nombre.trim().is_empty() || referencias.iter().any(|r| r == dibujo || r == &h.id) {
            continue;
        }
        let mut r = Resultado::nuevo(
            h.nombre.trim(),
            unir(&["Hoja", &p.nombre]),
            glifo::LIENZO,
            Accion::Pedido(pedido(
                "abrir",
                json!({ "que": { "tipo": "hoja", "proyecto": p.id_para_pedido(), "referencia": dibujo } }),
            )),
        );
        r.copiar = Some(h.nombre.trim().to_string());
        r.clave = Some(format!("hoja/{}/{}", p.id, dibujo));
        r.con_menu(json!({ "tipo": "hoja", "proyecto": p.id_para_pedido(), "carpeta": p.carpeta.to_string_lossy() }));
        cosa(Tipo::Lienzo, h.nombre.trim(), "", p.tocado, r);
    }
    // Las notas .md sueltas (las de un mensaje NOTA ya salieron arriba).
    for n in &p.notas_md {
        let raiz = n
            .ruta
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if referencias.contains(&raiz) || p.mensajes.iter().any(|m| m.id == raiz) {
            continue;
        }
        let ruta = n.ruta.to_string_lossy().to_string();
        let mut r = Resultado::nuevo(
            &n.titulo,
            unir(&["Nota", &p.nombre, &hace(n.cuando, ctx.ahora)]),
            glifo::NOTA,
            Accion::Pedido(pedido(
                "abrir",
                json!({ "que": { "tipo": "fichero", "ruta": ruta } }),
            )),
        );
        r.copiar = Some(ruta.clone());
        r.clave = Some(format!("fichero/{ruta}"));
        r.con_fichero(&n.ruta);
        r.con_menu(json!({ "tipo": "fichero", "ruta": ruta, "pin": pedido("pinear", json!({ "ruta": ruta })) }));
        cosa(Tipo::Nota, &n.titulo, "", n.cuando, r);
    }
}

// --- Las listas de tareas -------------------------------------------------

/// Una lista de tareas (mini-app `tareas`).
#[derive(Debug, Clone)]
pub struct Lista {
    pub proyecto: usize,
    pub codigo: String,
    pub titulo: String,
    /// Lo que se escribe en Flow para entrar: el titulo, o el titulo y el
    /// proyecto si hay dos que se llaman igual. Sin `>`.
    pub etiqueta: String,
    pub tareas: Vec<Tarea>,
    pub cuando: i64,
}

/// Como se llama el Inbox: la lista de «Mensajes guardados» adonde va todo
/// lo que se apunta sin decir donde (`tareas::INBOX` de la app).
pub const INBOX: &str = "Inbox";

impl Lista {
    /// Si es el Inbox: la de «Mensajes guardados» que se llama asi.
    pub fn es_inbox(&self, proyectos: &[Proyecto]) -> bool {
        proyectos.get(self.proyecto).is_some_and(|p| p.guardados)
            && self.titulo.trim().eq_ignore_ascii_case(INBOX)
    }

    /// Lo hecho, de 0 a 100 (`None` si esta vacia).
    pub fn progreso(&self) -> Option<u8> {
        let n = self.tareas.len();
        (n > 0).then(|| (self.tareas.iter().filter(|t| t.hecha).count() * 100 / n) as u8)
    }

    /// La consulta que entra en ella: `p tareas <etiqueta> > `.
    pub fn consulta(&self, ctx: &Contexto) -> String {
        ctx.consulta(&format!("tareas {} {SEPARADOR} ", self.etiqueta))
    }
}

pub fn listas(proyectos: &[Proyecto]) -> Vec<Lista> {
    let mut v = Vec::new();
    for (i, p) in proyectos.iter().enumerate() {
        for m in p.mensajes.iter().filter(|m| m.es_lista_de_tareas()) {
            let mut titulo = datos::titulo_del_documento(m.texto());
            if titulo.is_empty() {
                titulo = m.nombre().to_string();
            }
            if titulo.is_empty() {
                titulo = "Tareas".into();
            }
            let titulo = titulo.replace(SEPARADOR, "›");
            v.push(Lista {
                proyecto: i,
                codigo: m.id.clone(),
                etiqueta: titulo.clone(),
                titulo,
                tareas: datos::leer_tareas(m.texto()),
                cuando: m.cuando,
            });
        }
    }
    // Dos con el mismo titulo: se distinguen por el proyecto (y por un
    // numero si tambien coincide).
    let normal: Vec<String> = v.iter().map(|l| normalizar(&l.titulo)).collect();
    for i in 0..v.len() {
        if normal.iter().filter(|n| **n == normal[i]).count() > 1 {
            let nombre = proyectos[v[i].proyecto].nombre.replace(SEPARADOR, "›");
            v[i].etiqueta = format!("{} · {}", v[i].titulo, nombre);
        }
    }
    let etiquetas: Vec<String> = v.iter().map(|l| normalizar(&l.etiqueta)).collect();
    for i in 0..v.len() {
        let antes = etiquetas[..i]
            .iter()
            .filter(|e| **e == etiquetas[i])
            .count();
        if antes > 0 {
            v[i].etiqueta = format!("{} · {}", v[i].etiqueta, antes + 1);
        }
    }
    v.sort_by_key(|l| std::cmp::Reverse(l.cuando));
    v
}

/// «2 pendientes de 3», «vacía»...
fn cuenta_de(l: &Lista) -> String {
    let pendientes = l.tareas.iter().filter(|t| !t.hecha).count();
    match (pendientes, l.tareas.len()) {
        (_, 0) => "vacía".to_string(),
        (0, n) => format!("todas hechas ({n})"),
        (1, n) => format!("1 pendiente de {n}"),
        (k, n) => format!("{k} pendientes de {n}"),
    }
}

fn resultado_lista(l: &Lista, proyectos: &[Proyecto], ctx: &Contexto) -> Resultado {
    let p = &proyectos[l.proyecto];
    let cuenta = cuenta_de(l);
    let mut r = Resultado::nuevo(
        &l.titulo,
        unir(&["Lista de tareas", &p.nombre, &cuenta]),
        glifo::TAREAS,
        Accion::Consulta(l.consulta(ctx)),
    );
    r.autocompletar = Some(l.consulta(ctx));
    r.progreso = l.progreso();
    r.clave = Some(format!("lista/{}/{}", p.id, l.codigo));
    let texto: String = l
        .tareas
        .iter()
        .map(|t| format!("- [{}] {}\n", if t.hecha { "x" } else { " " }, t.texto))
        .collect();
    r.copiar = Some(texto.clone());
    if !texto.is_empty() {
        r.ayuda_subtitulo = Some(para_ayuda(&texto));
    }
    // «Borrar lista»: se vuelve a todas las listas, ya sin ella.
    r.con_menu(json!({ "tipo": "mensaje", "proyecto": p.id_para_pedido(), "codigo": l.codigo, "texto": texto,
        "carpeta": p.carpeta.to_string_lossy(),
        "borrar_lista": borrar_lista_del_menu(p, l, ctx) }));
    r
}

// --- Elegir --------------------------------------------------------------

/// El proyecto que mejor encaja con lo escrito tras `@` (y si encaja del
/// todo). `None` si no se escribio nada o no encaja ninguno.
fn elegir_proyecto<'a>(proyectos: &'a [Proyecto], texto: &str) -> Option<(&'a Proyecto, bool)> {
    let q = normalizar(texto.trim());
    if q.is_empty() {
        return None;
    }
    proyectos
        .iter()
        .map(|p| (p, puntuar(&q, &normalizar(&p.nombre), "")))
        .filter(|(_, s)| *s > 0)
        .max_by(|a, b| a.1.cmp(&b.1).then(a.0.tocado.cmp(&b.0.tocado)))
        .map(|(p, s)| (p, s == crate::normalizar::IGUAL))
}

/// Las sugerencias de proyecto mientras se escribe tras `@`.
fn sugerir_proyectos(
    proyectos: &[Proyecto],
    texto: &str,
    delante: &str,
    ctx: &Contexto,
) -> Vec<Resultado> {
    let q = normalizar(texto.trim());
    let mut v: Vec<(&Proyecto, u32)> = proyectos
        .iter()
        .map(|p| {
            (
                p,
                if q.is_empty() {
                    1
                } else {
                    puntuar(&q, &normalizar(&p.nombre), "")
                },
            )
        })
        .filter(|(_, s)| *s > 0)
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.tocado.cmp(&a.0.tocado)));
    v.into_iter()
        .take(5)
        .map(|(p, _)| {
            let c = ctx.consulta(&format!("{} @{}", delante.trim_end(), p.nombre));
            let mut r = Resultado::nuevo(
                format!("En «{}»", p.nombre),
                "Intro: elegir este proyecto",
                glifo::PROYECTO,
                Accion::Consulta(c.clone()),
            );
            r.autocompletar = Some(c);
            r
        })
        .collect()
}

/// Buscar en todo (o solo en unos tipos, o en un proyecto).
fn buscar(
    proyectos: &[Proyecto],
    texto: &str,
    en_proyecto: Option<&str>,
    tipos: Option<&[Tipo]>,
    ctx: &Contexto,
) -> Vec<Resultado> {
    let q = normalizar(texto.trim());
    let mut cosas = Vec::new();
    if en_proyecto.is_none() {
        for f in Funcion::OFRECIDAS {
            let r = resultado_funcion(f, ctx);
            let mut nombres: Vec<String> = f.alias().iter().map(|a| a.to_string()).collect();
            nombres.push(normalizar(&r.titulo));
            cosas.push(Cosa {
                tipo: Tipo::Funcion,
                nombres,
                dentro: String::new(),
                cuando: i64::MAX,
                resultado: r,
            });
        }
    }
    let etiquetas = crate::chat::etiquetas(proyectos);
    for (i, p) in proyectos.iter().enumerate() {
        if en_proyecto.is_some_and(|id| id != p.id) {
            continue;
        }
        if en_proyecto.is_none() {
            cosas.push(Cosa {
                tipo: Tipo::Proyecto,
                nombres: vec![normalizar(&p.nombre)],
                dentro: String::new(),
                cuando: p.tocado,
                resultado: resultado_proyecto(p, &etiquetas[i], ctx),
            });
        }
        cosas_de_proyecto(proyectos, i, ctx, &mut cosas);
    }
    for l in listas(proyectos) {
        let p = &proyectos[l.proyecto];
        if en_proyecto.is_some_and(|id| id != p.id) {
            continue;
        }
        let dentro: Vec<&str> = l.tareas.iter().map(|t| t.texto.as_str()).collect();
        cosas.push(Cosa {
            tipo: Tipo::Lista,
            nombres: vec![normalizar(&l.titulo)],
            dentro: normalizar(&dentro.join("\n")),
            cuando: l.cuando,
            resultado: resultado_lista(&l, proyectos, ctx),
        });
    }
    let mut puntuadas: Vec<(u32, i64, Resultado)> = cosas
        .into_iter()
        .filter(|c| tipos.is_none_or(|t| t.contains(&c.tipo)))
        .filter_map(|c| {
            let base = c
                .nombres
                .iter()
                .map(|n| puntuar(&q, n, &c.dentro))
                .max()
                .unwrap_or(0);
            if base == 0 && !q.is_empty() {
                return None;
            }
            let extra = match c.tipo {
                Tipo::Funcion => EXTRA_FUNCION,
                Tipo::Proyecto => EXTRA_PROYECTO,
                _ => 0,
            };
            Some((base + extra, c.cuando, c.resultado))
        })
        .collect();
    // Las lecciones aprendidas, buscadas como en la app y por debajo de lo
    // que se llama exactamente asi (`lecciones::para_buscar_en_todo`).
    if tipos.is_none() && !q.is_empty() {
        puntuadas.extend(crate::lecciones::para_buscar_en_todo(
            proyectos,
            texto,
            en_proyecto,
            ctx,
        ));
    }
    puntuadas.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    puntuadas
        .into_iter()
        .take(MAXIMO)
        .map(|(_, _, mut r)| {
            // Tab escribe su nombre: la busqueda se queda en el.
            if r.autocompletar.is_none() {
                r.autocompletar = Some(ctx.consulta(&r.titulo));
            }
            r
        })
        .collect()
}

/// **Lo que sale en Flow para lo tecleado.**
pub fn resultados(proyectos: &[Proyecto], busqueda: &str, ctx: &Contexto) -> Vec<Resultado> {
    // `<nombre entero de un proyecto> > ...` es su chat, aunque el nombre
    // empiece por un verbo («Chat de clase > »). El chat tiene su propio
    // maximo (`chat::MAXIMO_CHAT`).
    if let Some((delante, filtro)) = busqueda.split_once(SEPARADOR) {
        if let Some(i) = crate::chat::exacto(proyectos, delante) {
            let mut v = crate::chat::en_proyecto(proyectos, i, filtro, ctx);
            resaltar(&mut v, filtro);
            return v;
        }
    }
    let modo = analizar(busqueda);
    // Escribiendo una tarea se puede pegar la imagen copiada (`t` sola
    // tambien: «t [img 01]» es una tarea). Con `@proyecto` no, que la ficha
    // quedaria dentro del nombre.
    let de_tarea = match &modo {
        Modo::Apuntar { .. } | Modo::Lista { .. } => true,
        Modo::Verbo {
            funcion: Funcion::Tareas,
            resto,
            proyecto: None,
        } => !resto.trim().is_empty() || busqueda.trim().chars().count() == 1,
        // Y escribiendo una leccion nueva («a …», «lección …»): son sus fotos.
        Modo::Verbo {
            funcion: Funcion::Leccion,
            resto,
            proyecto: None,
        } => !resto.trim().is_empty() || busqueda.trim().chars().count() == 1,
        // Y lo que va al movil: sin imagen no hay nada que mandar.
        Modo::Verbo {
            funcion: Funcion::Movil,
            proyecto: None,
            ..
        } => true,
        _ => false,
    };
    // Lo que se resalta en los titulos: lo que se busca o filtra.
    let resaltable = match &modo {
        Modo::Vacio | Modo::Apuntar { .. } => String::new(),
        Modo::Buscar { texto, .. } => texto.clone(),
        Modo::Verbo { resto, .. } => resto.clone(),
        Modo::Lista { filtro, .. } | Modo::Proyecto { filtro, .. } => filtro.clone(),
    };
    let mut v = match modo {
        Modo::Vacio => {
            let mut v: Vec<Resultado> = Funcion::OFRECIDAS
                .iter()
                .map(|f| resultado_funcion(*f, ctx))
                .collect();
            let etiquetas = crate::chat::etiquetas(proyectos);
            let mut recientes: Vec<usize> = (0..proyectos.len()).collect();
            recientes.sort_by_key(|i| std::cmp::Reverse(proyectos[*i].tocado));
            v.extend(
                recientes
                    .into_iter()
                    .take(RECIENTES)
                    .map(|i| resultado_proyecto(&proyectos[i], &etiquetas[i], ctx)),
            );
            v
        }
        Modo::Buscar { texto, proyecto } => match proyecto {
            None => buscar(proyectos, &texto, None, None, ctx),
            Some(pt) => {
                let elegido = elegir_proyecto(proyectos, &pt);
                let mut v = Vec::new();
                if !elegido.is_some_and(|(_, exacto)| exacto) {
                    v.extend(sugerir_proyectos(proyectos, &pt, &texto, ctx));
                }
                if let Some((p, _)) = elegido {
                    v.extend(buscar(proyectos, &texto, Some(&p.id), None, ctx));
                }
                v
            }
        },
        Modo::Verbo {
            funcion,
            resto,
            proyecto,
        } => verbo(proyectos, funcion, &resto, proyecto.as_deref(), ctx),
        Modo::Lista { lista, filtro } => en_lista(proyectos, &lista, &filtro, ctx),
        Modo::Proyecto { proyecto, filtro } => {
            let mut v = crate::chat::buscar_y_entrar(proyectos, &proyecto, &filtro, ctx);
            resaltar(&mut v, &filtro);
            return v;
        }
        Modo::Apuntar { texto } => apuntar(proyectos, &texto, ctx),
    };
    // Arriba del todo (solo con la app cerrada: abierta, pega ella con
    // Ctrl+V). En una lista que no existe, no.
    if de_tarea && v.first().is_some_and(|r| r.glifo != glifo::AVISO) {
        if let Some(r) = crate::imagenes::resultado_pegar(busqueda, ctx) {
            v.insert(0, r);
        }
    }
    v.truncate(MAXIMO);
    resaltar(&mut v, &resaltable);
    v
}

/// Pone en cada titulo (que aun no lo tenga) las letras que coinciden con
/// `consulta`.
pub(crate) fn resaltar(v: &mut [Resultado], consulta: &str) {
    if consulta.trim().is_empty() {
        return;
    }
    for r in v.iter_mut().filter(|r| r.resaltado.is_empty()) {
        r.resaltado = crate::normalizar::resaltado(&r.titulo, consulta);
    }
}

/// `t <texto>`: apuntar una tarea en el Inbox (pedido `anadir_tarea` sin
/// lista: la app la pone en el Inbox, y lo crea si no esta).
fn apuntar(proyectos: &[Proyecto], texto: &str, ctx: &Contexto) -> Vec<Resultado> {
    let texto = texto.trim();
    let todas = listas(proyectos);
    let inbox = todas.iter().find(|l| l.es_inbox(proyectos));
    let cuenta = inbox.map(cuenta_de).unwrap_or_default();
    let mut r = Resultado::nuevo(
        format!("Apuntar tarea: {texto}"),
        unir(&["Tarea", INBOX, &cuenta, "Intro: apuntarla"]),
        glifo::ANADIR,
        Accion::Pedido(pedido("anadir_tarea", json!({ "texto": texto }))),
    );
    r.ayuda_titulo = Some(texto.to_string());
    crate::imagenes::con_imagenes(&mut r, texto, ctx);
    let mut v = vec![r];
    v.push(resultado_ventana_tareas());
    if let Some(l) = inbox {
        let mut r = resultado_lista(l, proyectos, ctx);
        r.subtitulo = unir(&["Ver el Inbox", &cuenta]);
        v.push(r);
    }
    v
}

fn resultado_ventana_tareas() -> Resultado {
    resultado_ventana(
        "tareas",
        "Abrir la ventana de Tareas",
        "El Inbox y todas tus listas, en PixPin",
        glifo::VENTANA,
    )
}

fn verbo(
    proyectos: &[Proyecto],
    f: Funcion,
    resto: &str,
    arroba: Option<&str>,
    ctx: &Contexto,
) -> Vec<Resultado> {
    let elegido = arroba.and_then(|a| elegir_proyecto(proyectos, a));
    let destino_id = elegido
        .map(|(p, _)| p.id_para_pedido())
        .unwrap_or(Value::Null);
    let destino = match (arroba, elegido) {
        (_, Some((p, _))) => format!("en «{}»", p.nombre),
        (Some(a), None) if !a.trim().is_empty() => format!(
            "en {} (no hay ningún proyecto «{}»)",
            datos::NOMBRE_GUARDADOS,
            a.trim()
        ),
        _ => format!("en {}", datos::NOMBRE_GUARDADOS),
    };
    let resto = resto.trim();
    let mut v = Vec::new();
    match f {
        Funcion::Chat => {
            if resto.is_empty() {
                let accion = match elegido {
                    Some((p, _)) => Accion::Pedido(pedido(
                        "abrir",
                        json!({ "que": { "tipo": "proyecto", "proyecto": p.id_para_pedido() } }),
                    )),
                    None => Accion::Pedido(pedido("ventana_principal", json!({}))),
                };
                v.push(Resultado::nuevo(
                    "Escribir en el chat…",
                    format!("Escribe el mensaje detrás de «chat» · Intro abre el chat {destino}"),
                    glifo::CHAT,
                    accion,
                ));
            } else {
                let mut r = Resultado::nuevo(
                    format!("Escribir en el chat: {resto}"),
                    format!("Intro: mandarlo {destino}"),
                    glifo::CHAT,
                    Accion::Pedido(pedido(
                        "chat",
                        json!({ "texto": resto, "proyecto": destino_id }),
                    )),
                );
                // Con `[img NN]`, las imagenes van con el mensaje como fotos.
                crate::imagenes::con_imagenes(&mut r, resto, ctx);
                v.push(r);
            }
        }
        Funcion::Nota => {
            let (titulo, campos) = if resto.is_empty() {
                ("Nota nueva".to_string(), json!({ "proyecto": destino_id }))
            } else {
                (
                    format!("Nota nueva: {resto}"),
                    json!({ "texto": resto, "proyecto": destino_id }),
                )
            };
            let mut r = Resultado::nuevo(
                titulo,
                format!("Intro: abrirla en el editor de notas, {destino}"),
                glifo::NOTA,
                Accion::Pedido(pedido("nota_nueva", campos)),
            );
            // Y en una nota, cada `[img NN]` queda como su imagen dentro.
            crate::imagenes::con_imagenes(&mut r, resto, ctx);
            v.push(r);
        }
        Funcion::Lienzo => {
            let (titulo, campos) = if resto.is_empty() {
                (
                    "Lienzo nuevo".to_string(),
                    json!({ "proyecto": destino_id }),
                )
            } else {
                (
                    format!("Lienzo nuevo: {resto}"),
                    json!({ "nombre": resto, "proyecto": destino_id }),
                )
            };
            v.push(Resultado::nuevo(
                titulo,
                format!("Intro: abrirlo, {destino}"),
                glifo::LIENZO,
                Accion::Pedido(pedido("lienzo_nuevo", campos)),
            ));
        }
        Funcion::Grabar => {
            if resto.is_empty() {
                let mut r = Resultado::nuevo(
                    "Grabar audio: escribe su nombre",
                    "El audio necesita un nombre para encontrarlo luego",
                    glifo::GRABAR,
                    Accion::Consulta(ctx.consulta("grabar ")),
                );
                r.autocompletar = Some(ctx.consulta("grabar "));
                v.push(r);
            } else {
                v.push(Resultado::nuevo(
                    format!("Grabar audio: {resto}"),
                    format!("Intro: sale un micrófono flotante que graba; un clic lo para y lo guarda {destino}"),
                    glifo::GRABAR,
                    Accion::Pedido(pedido("grabar", json!({ "nombre": resto, "proyecto": destino_id }))),
                ));
            }
        }
        // Como el movil: solo «que aprendiste» hace falta; lo demas (etiquetas,
        // area, parecidas) lo propone la ficha que se abre.
        Funcion::Leccion => {
            if resto.is_empty() {
                let mut r = Resultado::nuevo(
                    "Nueva lección: escribe lo que aprendiste",
                    format!(
                        "Con el texto, Intro abre la ficha ya rellena · Ctrl+V pega una foto · {destino}"
                    ),
                    glifo::LECCION,
                    Accion::Pedido(pedido("leccion_nueva", json!({ "proyecto": destino_id }))),
                );
                r.autocompletar = Some(ctx.consulta("lección "));
                v.push(r);
                v.push(Resultado::nuevo(
                    "Lecciones aprendidas",
                    "Búscalas aquí · «lecciones <texto>»",
                    glifo::LECCION,
                    Accion::Consulta(ctx.consulta("lecciones ")),
                ));
                v.push(crate::lecciones::resultado_ventana_lecciones());
            } else {
                let mut r = Resultado::nuevo(
                    format!("Nueva lección: {resto}"),
                    format!("Intro: abre la ficha con esto, {destino}"),
                    glifo::LECCION,
                    Accion::Pedido(pedido(
                        "leccion_nueva",
                        json!({ "texto": resto, "proyecto": destino_id }),
                    )),
                );
                // Con `[img NN]`, las imagenes son las fotos de la leccion.
                crate::imagenes::con_imagenes(&mut r, resto, ctx);
                v.push(r);
                // ¿Ya la tenias? Las que hablan de lo mismo, debajo.
                let sin_fichas = quitar_fichas(resto);
                let todas = crate::lecciones::todas(proyectos);
                let mut parecidas = crate::lecciones::buscar(&todas, &sin_fichas);
                parecidas.sort_by_key(|x| std::cmp::Reverse(x.0));
                v.extend(
                    parecidas
                        .iter()
                        .take(crate::lecciones::EN_TODO)
                        .map(|(_, l)| {
                            let mut r = crate::lecciones::resultado(l, proyectos, ctx);
                            r.subtitulo = format!("¿Ya la tienes? · {}", r.subtitulo);
                            r
                        }),
                );
                if !sin_fichas.is_empty() {
                    v.push(Resultado::nuevo(
                        format!("Buscar en lecciones: {sin_fichas}"),
                        "Intro: la lista de lecciones de la app con esta búsqueda",
                        glifo::VENTANA,
                        Accion::Pedido(pedido(
                            "lecciones",
                            json!({ "consulta": sin_fichas, "proyecto": destino_id }),
                        )),
                    ));
                }
            }
        }
        Funcion::Lecciones => v.extend(crate::lecciones::lista(
            proyectos,
            resto,
            elegido.map(|(p, _)| p),
            ctx,
        )),
        Funcion::Repasar => v.extend(crate::lecciones::repasar(proyectos, ctx)),
        Funcion::Soltar => {
            // Sin proyecto elegido, el primero va a Mensajes guardados;
            // debajo, cada proyecto (los que encajan con lo escrito).
            let q = normalizar(resto);
            let titulo = |p: &Proyecto| format!("Añadir a «{}»", p.nombre);
            let sub = "Intro: un recuadro flotante; suelta archivos en él y van a su chat";
            match elegido {
                Some((p, _)) => v.push(Resultado::nuevo(
                    titulo(p),
                    sub,
                    glifo::ADJUNTAR,
                    Accion::Pedido(pedido_soltar(Some(p))),
                )),
                None if q.is_empty() => {
                    let guardados = proyectos.iter().find(|p| p.guardados);
                    v.push(Resultado::nuevo(
                        format!("Añadir a «{}»", datos::NOMBRE_GUARDADOS),
                        sub,
                        glifo::ADJUNTAR,
                        Accion::Pedido(pedido_soltar(guardados)),
                    ));
                }
                None => {}
            }
            if elegido.is_none() {
                let mut encajan: Vec<(u32, &Proyecto)> = proyectos
                    .iter()
                    .filter(|p| !(q.is_empty() && p.guardados))
                    .map(|p| {
                        (
                            if q.is_empty() {
                                1
                            } else {
                                puntuar(&q, &normalizar(&p.nombre), "")
                            },
                            p,
                        )
                    })
                    .filter(|(s, _)| *s > 0)
                    .collect();
                encajan.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.tocado.cmp(&a.1.tocado)));
                v.extend(encajan.into_iter().map(|(_, p)| {
                    Resultado::nuevo(
                        titulo(p),
                        sub,
                        glifo::ADJUNTAR,
                        Accion::Pedido(pedido_soltar(Some(p))),
                    )
                }));
                if v.is_empty() {
                    v.push(Resultado::nuevo(
                        format!("No hay ningún proyecto «{resto}»"),
                        format!("Intro: añadir a «{}»", datos::NOMBRE_GUARDADOS),
                        glifo::AVISO,
                        Accion::Pedido(pedido_soltar(proyectos.iter().find(|p| p.guardados))),
                    ));
                }
            }
        }
        Funcion::Abrir => {
            let accion = match elegido {
                Some((p, _)) => Accion::Pedido(pedido(
                    "abrir",
                    json!({ "que": { "tipo": "proyecto", "proyecto": p.id_para_pedido() } }),
                )),
                None => Accion::Pedido(pedido("ventana_principal", json!({}))),
            };
            v.push(Resultado::nuevo(
                "Abrir PixPin",
                "Saca la ventana del chat",
                glifo::PIXPIN,
                accion,
            ));
        }
        Funcion::Tareas => {
            let todas = listas(proyectos);
            let en: Option<&str> = elegido.map(|(p, _)| p.id.as_str());
            let q = normalizar(resto);
            let inbox = todas.iter().find(|l| l.es_inbox(proyectos));
            let mut encajan: Vec<(u32, &Lista)> = todas
                .iter()
                .filter(|l| en.is_none_or(|id| proyectos[l.proyecto].id == id))
                .map(|l| {
                    (
                        if q.is_empty() {
                            1
                        } else {
                            puntuar(&q, &normalizar(&l.titulo), "")
                        },
                        l,
                    )
                })
                .filter(|(s, _)| *s > 0)
                .collect();
            encajan.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cuando.cmp(&a.1.cuando)));
            let exacta = encajan.iter().any(|(_, l)| normalizar(&l.titulo) == q);
            let nueva = (!q.is_empty() && !exacta).then(|| {
                let titulo = resto.replace(SEPARADOR, "›");
                Resultado::nuevo(
                    format!("Nueva lista: {titulo}"),
                    format!("Intro: crear la lista de tareas {destino}"),
                    glifo::ANADIR,
                    Accion::PedirYSeguir {
                        pedido: pedido(
                            "lista_nueva",
                            json!({ "titulo": titulo, "proyecto": destino_id }),
                        ),
                        consulta: ctx.consulta(&format!("tareas {titulo} {SEPARADOR} ")),
                    },
                )
            });
            let aqui = ctx.consulta("tareas ");
            if !crate::imagenes::fichas(resto).is_empty() {
                // Con una imagen pegada es una tarea, no el nombre de una lista.
                v.push(anadir_al_inbox(resto, inbox, proyectos, &aqui, ctx));
                v.push(resultado_ventana_tareas());
            } else if q.is_empty() {
                // Sin texto: el Inbox el primero, con lo suyo a la vista
                // (Intro marca; el menu lo reparte con «Mover a…»).
                let inbox_aqui =
                    inbox.filter(|l| en.is_none_or(|id| proyectos[l.proyecto].id == id));
                if let Some(l) = inbox_aqui {
                    v.push(resultado_lista(l, proyectos, ctx));
                    let mut tareas: Vec<&Tarea> = l.tareas.iter().collect();
                    tareas.sort_by_key(|t| (t.hecha, t.indice));
                    v.extend(
                        tareas
                            .into_iter()
                            .take(TAREAS_DEL_INBOX)
                            .map(|t| resultado_tarea(l, t, proyectos, &todas, &aqui, ctx)),
                    );
                }
                v.push(resultado_ventana_tareas());
                if encajan.is_empty() {
                    v.push(Resultado::nuevo(
                        "No hay listas de tareas",
                        "Escribe el título de una lista nueva y pulsa Intro · «t <tarea>» apunta en el Inbox",
                        glifo::TAREAS,
                        Accion::Consulta(aqui),
                    ));
                } else {
                    v.extend(
                        encajan
                            .iter()
                            .filter(|(_, l)| !inbox_aqui.is_some_and(|i| i.codigo == l.codigo))
                            .map(|(_, l)| resultado_lista(l, proyectos, ctx)),
                    );
                }
            } else {
                v.extend(
                    encajan
                        .iter()
                        .map(|(_, l)| resultado_lista(l, proyectos, ctx)),
                );
                v.extend(nueva);
                // Lo escrito tambien puede ser una tarea para el Inbox.
                v.push(anadir_al_inbox(resto, inbox, proyectos, &aqui, ctx));
            }
        }
        Funcion::Capturar => {
            let mut r = Resultado::nuevo(
                "Capturar zona",
                "Recorta una zona de la pantalla",
                glifo::CAPTURA,
                Accion::Pedido(pedido_capturar()),
            );
            r.clave = Some("funcion/captura".into());
            v.push(r);
            v.push(resultado_ultima(ctx));
        }
        Funcion::Ultima => v.push(resultado_ultima(ctx)),
        Funcion::Movil => v.extend(crate::al_movil::resultados(resto, ctx)),
        Funcion::Galeria => {
            v.push(crate::capturas::resultado_galeria());
            v.extend(crate::capturas::lista(ctx, resto));
        }
        Funcion::Capturas => {
            v.extend(crate::capturas::lista(ctx, resto));
            v.push(crate::capturas::resultado_galeria());
        }
    }
    if let Some(a) = arroba {
        if !elegido.is_some_and(|(_, exacto)| exacto) {
            let delante = format!("{} {}", f.verbo(), resto);
            v.extend(sugerir_proyectos(proyectos, a, &delante, ctx));
        }
    }
    // Debajo, lo que ya existe de ese tipo y se llama asi.
    let tipos: &[Tipo] = match f {
        Funcion::Lienzo => &[Tipo::Lienzo],
        Funcion::Nota => &[Tipo::Nota],
        Funcion::Grabar => &[Tipo::Audio],
        Funcion::Abrir => &[Tipo::Proyecto],
        Funcion::Chat
        | Funcion::Tareas
        | Funcion::Soltar
        | Funcion::Leccion
        | Funcion::Lecciones
        | Funcion::Repasar
        | Funcion::Capturar
        | Funcion::Capturas
        | Funcion::Galeria
        | Funcion::Ultima
        | Funcion::Movil => &[],
    };
    // Sin texto, los mas recientes de ese tipo («pp voz»: los audios).
    if !tipos.is_empty() {
        let en = elegido.map(|(p, _)| p.id.clone());
        v.extend(buscar(proyectos, resto, en.as_deref(), Some(tipos), ctx));
    }
    v
}

/// El texto sin sus fichas `[img NN]`, con los blancos de mas fuera.
fn quitar_fichas(texto: &str) -> String {
    let mut s = String::new();
    let mut desde = 0;
    for f in crate::imagenes::fichas(texto) {
        s.push_str(&texto[desde..f.inicio]);
        s.push(' ');
        desde = f.fin;
    }
    s.push_str(&texto[desde..]);
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Cuantas tareas del Inbox salen con `p tareas` a secas.
const TAREAS_DEL_INBOX: usize = 10;
/// Cuantas listas, como mucho, ofrece «Mover a…».
const DESTINOS: usize = 15;

/// **Una tarea como resultado**: Intro la marca o desmarca y vuelve a
/// `vuelta` sin cerrar Flow; el menu ofrece «Mover a…» cada otra lista.
pub(crate) fn resultado_tarea(
    l: &Lista,
    t: &Tarea,
    proyectos: &[Proyecto],
    todas: &[Lista],
    vuelta: &str,
    ctx: &Contexto,
) -> Resultado {
    let p = &proyectos[l.proyecto];
    let (marca, glifo, que) = if t.hecha {
        ("☑", glifo::HECHA, "Intro: desmarcarla")
    } else {
        ("☐", glifo::PENDIENTE, "Intro: hecha")
    };
    // Cuanto lleva creada (docs/investigacion/2026-10-02-tareas-con-fecha.md).
    let edad = t
        .creada
        .zip(crate::fecha::local(ctx.ahora))
        .map(|(c, hoy)| {
            datos::hace_dias(
                datos::dias_civiles(i64::from(hoy.anio), i64::from(hoy.mes), i64::from(hoy.dia))
                    - c,
            )
        })
        .unwrap_or_default();
    let donde = if p.guardados {
        l.titulo.clone()
    } else {
        format!("{} · {}", l.titulo, p.nombre)
    };
    let mut r = Resultado::nuevo(
        format!("{marca} {}", t.texto),
        // Con su numero: dos tareas iguales (se puede apuntar la misma dos
        // veces) darian dos filas identicas, y Flow junta las identicas.
        unir(&[
            "Tarea",
            &format!("n.º {}", t.indice + 1),
            &donde,
            &edad,
            que,
        ]),
        glifo,
        Accion::PedirYSeguir {
            pedido: pedido(
                "marcar_tarea",
                json!({ "proyecto": p.id_para_pedido(), "codigo": l.codigo, "indice": t.indice, "hecha": !t.hecha }),
            ),
            consulta: vuelta.to_string(),
        },
    );
    r.copiar = Some(t.texto.clone());
    r.ayuda_titulo = Some(t.texto.clone());
    r.autocompletar = Some(format!("{}{}", l.consulta(ctx), t.texto));
    r.clave = Some(format!("tarea/{}/{}/{}", p.id, l.codigo, t.texto));
    // Sus imagenes: `[img 01]` resaltado en el titulo, cuantas en el
    // subtitulo y la primera en la vista previa (F1).
    if !t.imagenes.is_empty() {
        let titulo = r.titulo.clone();
        let numeros: Vec<u32> = crate::imagenes::fichas(&titulo)
            .iter()
            .map(|f| f.numero)
            .collect();
        r.resaltado = crate::imagenes::resaltado(&titulo, &numeros);
        let n = t.imagenes.len();
        r.subtitulo = format!(
            "📎 {n} {} · {}",
            if n == 1 { "imagen" } else { "imágenes" },
            r.subtitulo
        );
        if let Some(ruta) = p
            .ruta_real(&t.imagenes[0])
            .filter(|r| crate::datos::existe(r))
        {
            r.con_fichero(&ruta);
        }
    }
    let destinos: Vec<Value> = todas
        .iter()
        .filter(|d| !(d.proyecto == l.proyecto && d.codigo == l.codigo))
        .take(DESTINOS)
        .map(|d| {
            let dp = &proyectos[d.proyecto];
            json!({ "titulo": d.titulo, "donde": dp.nombre, "a_proyecto": dp.id_para_pedido(), "a_codigo": d.codigo })
        })
        .collect();
    r.con_menu(json!({
        "tipo": "tarea",
        "proyecto": p.id_para_pedido(),
        "codigo": l.codigo,
        "texto": t.texto,
        "carpeta": p.carpeta.to_string_lossy(),
        "mover": { "proyecto": p.id_para_pedido(), "codigo": l.codigo, "indice": t.indice, "destinos": destinos },
        // «Quitar tarea»: se vuelve a la misma lista, ya sin ella.
        "quitar": {
            "pedido": pedido("quitar_tarea", json!({ "proyecto": p.id_para_pedido(), "codigo": l.codigo, "indice": t.indice })),
            "vuelta": vuelta,
        },
    }));
    r
}

/// «Añadir al Inbox: <texto>»: la tarea va al Inbox y se vuelve a `vuelta`.
/// Si el Inbox ya esta, se dice cual (`codigo`), para ensenarla en el acto;
/// si no, la app lo crea.
fn anadir_al_inbox(
    texto: &str,
    inbox: Option<&Lista>,
    proyectos: &[Proyecto],
    vuelta: &str,
    ctx: &Contexto,
) -> Resultado {
    let texto = texto.trim();
    let campos = match inbox {
        Some(l) => {
            json!({ "texto": texto, "proyecto": proyectos[l.proyecto].id_para_pedido(), "codigo": l.codigo })
        }
        None => json!({ "texto": texto }),
    };
    let mut r = Resultado::nuevo(
        format!("Añadir al Inbox: {texto}"),
        unir(&["Tarea", INBOX, "Intro: apuntarla"]),
        glifo::ANADIR,
        Accion::PedirYSeguir {
            pedido: pedido("anadir_tarea", campos),
            consulta: vuelta.to_string(),
        },
    );
    crate::imagenes::con_imagenes(&mut r, texto, ctx);
    r
}

/// «Última captura»: Intro la saca como pin. Si se sabe cual es, se ensena.
fn resultado_ultima(ctx: &Contexto) -> Resultado {
    let mut r = Resultado::nuevo(
        "Última captura",
        "Pinéala",
        glifo::PIN,
        Accion::Pedido(pedido("pinear_ultima", json!({}))),
    );
    r.clave = Some("funcion/ultima".into());
    if let Some(c) = ctx
        .raiz_de_datos()
        .and_then(|raiz| crate::capturas::leer(&raiz, ctx.ahora).into_iter().next())
    {
        r.subtitulo = unir(&[
            &c.nombre,
            &crate::fecha::hace_con_horas(c.cuando, ctx.ahora),
            "Intro: pinearla",
        ]);
        if !c.es_video() {
            r.icono = Some(c.ruta.to_string_lossy().to_string());
        }
        r.con_fichero(&c.ruta);
    }
    r
}

/// La lista que encaja con lo escrito antes de `>`.
fn buscar_lista<'a>(todas: &'a [Lista], texto: &str) -> Option<&'a Lista> {
    let q = normalizar(texto.trim());
    if let Some(l) = todas.iter().find(|l| normalizar(&l.etiqueta) == q) {
        return Some(l);
    }
    if let Some(l) = todas.iter().find(|l| normalizar(&l.titulo) == q) {
        return Some(l);
    }
    todas
        .iter()
        .map(|l| (puntuar(&q, &normalizar(&l.etiqueta), ""), l))
        .filter(|(s, _)| *s > 0)
        .max_by(|a, b| a.0.cmp(&b.0).then(a.1.cuando.cmp(&b.1.cuando)))
        .map(|(_, l)| l)
}

fn en_lista(proyectos: &[Proyecto], lista: &str, filtro: &str, ctx: &Contexto) -> Vec<Resultado> {
    let todas = listas(proyectos);
    let Some(l) = buscar_lista(&todas, lista) else {
        return vec![Resultado::nuevo(
            format!("No encuentro la lista «{lista}»"),
            "Intro: ver todas las listas",
            glifo::AVISO,
            Accion::Consulta(ctx.consulta("tareas ")),
        )];
    };
    let p = &proyectos[l.proyecto];
    let aqui = ctx.consulta(&format!("tareas {} {SEPARADOR} ", l.etiqueta));
    let filtro = filtro.trim();
    let ahora_mismo = format!("{aqui}{filtro}");
    let q = normalizar(filtro);
    let mut v = Vec::new();
    // Tambien con una igual ya en la lista: antes se escondia, y apuntar la
    // misma tarea dos veces no hacia nada (el usuario: «ingrese una tarea dos
    // veces y solo aparece una vez»). Se avisa, pero se deja.
    let exacta = l.tareas.iter().any(|t| normalizar(&t.texto) == q);
    if !q.is_empty() {
        let mut r = Resultado::nuevo(
            format!("Añadir tarea: {filtro}"),
            if exacta {
                format!(
                    "Ya hay una igual · Intro: añadir otra a «{}» · {}",
                    l.titulo, p.nombre
                )
            } else {
                format!("Intro: añadirla a «{}» · {}", l.titulo, p.nombre)
            },
            glifo::ANADIR,
            Accion::PedirYSeguir {
                pedido: pedido(
                    "anadir_tarea",
                    json!({ "texto": filtro, "proyecto": p.id_para_pedido(), "codigo": l.codigo }),
                ),
                consulta: aqui.clone(),
            },
        );
        crate::imagenes::con_imagenes(&mut r, filtro, ctx);
        v.push(r);
    }
    let mut tareas: Vec<&Tarea> = l
        .tareas
        .iter()
        .filter(|t| q.is_empty() || puntuar(&q, &normalizar(&t.texto), "") > 0)
        .collect();
    // Pendientes primero, cada grupo en el orden del documento.
    tareas.sort_by_key(|t| (t.hecha, t.indice));
    for t in tareas {
        v.push(resultado_tarea(l, t, proyectos, &todas, &ahora_mismo, ctx));
    }
    if l.tareas.is_empty() && q.is_empty() {
        let mut r = Resultado::nuevo(
            format!("«{}» está vacía", l.titulo),
            "Escribe una tarea y pulsa Intro para añadirla",
            glifo::TAREAS,
            Accion::Consulta(aqui),
        );
        // Una lista vacia es la primera que se quiere borrar.
        r.con_menu(
            json!({ "tipo": "mensaje", "proyecto": p.id_para_pedido(), "codigo": l.codigo,
            "borrar_lista": borrar_lista_del_menu(p, l, ctx) }),
        );
        v.push(r);
    }
    v
}

/// Lo que el menu necesita para «Borrar lista»: el pedido y adonde volver
/// (todas las listas, ya sin ella).
fn borrar_lista_del_menu(p: &Proyecto, l: &Lista, ctx: &Contexto) -> Value {
    json!({
        "pedido": pedido("borrar_lista", json!({ "proyecto": p.id_para_pedido(), "codigo": l.codigo })),
        "vuelta": ctx.consulta("tareas "),
        "cuantas": l.tareas.len(),
    })
}

/// El menu contextual (flecha derecha o Mayus+Intro): ver [`crate::menu`].
pub use crate::menu::menu;
