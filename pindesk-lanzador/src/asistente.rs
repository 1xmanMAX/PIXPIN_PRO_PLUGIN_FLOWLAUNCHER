//! **El asistente desde Flow**: `p asistente <pedido>` (o `p ia`, `p a`,
//! `p ?`).
//!
//! El usuario, 13-oct: «yo le pido algo y lo hace directamente»; y 15-oct:
//! «entro, pongo asistente y le escribo cualquier cosa; el asistente va
//! haciendo las tareas en background y me manda una notificación cuando haya
//! terminado».
//!
//! - Con texto: «Pedir al asistente: …» arriba, con el motor que lo hara y
//!   cuantos tiene delante (con `@proyecto`, sobre ese chat; con Ctrl+V, la
//!   imagen copiada va con el pedido, como en `p s`). Intro lo manda a la
//!   **cola de encargos** de la app (`automatizaciones::cola`) y **Flow se
//!   queda abierto** en `p asistente ` con el encargo ya «en cola» en la
//!   lista: se pueden mandar varios seguidos sin esperar. Al acabar cada
//!   uno, la app avisa con una notificacion de Windows.
//! - Debajo, los encargos en curso y los ultimos acabados (en cola,
//!   haciendo, hecha, falló), leidos de `<raiz>/asistente-encargos.json`;
//!   Intro abre lo que hizo (o el chat «Asistente»).
//! - Sin texto: como pedirle algo, abrir el chat «Asistente», los encargos y
//!   las automatizaciones de la lista, para correrlas ya (pedido `asistente`
//!   con `automatizacion`).
//!
//! Las automatizaciones se leen del disco como todo lo demas del plugin: el
//! JSON que la app guarda en la carpeta del chat «Asistente»
//! (`android/guardados/asistente/automatizaciones.json`).

use crate::datos::Proyecto;
use crate::resultados::{Accion, Contexto, Resultado, glifo, pedido};
use serde_json::{Value, json};
use std::path::Path;

/// Donde guarda la app las automatizaciones, dentro de la carpeta del chat.
pub const FICHERO: &str = "android/guardados/asistente/automatizaciones.json";
/// Donde apunta la app los encargos (en la raiz de los datos).
pub const ENCARGOS: &str = "asistente-encargos.json";
/// Lo que se teclea para volver a la lista tras mandar uno.
pub const VERBO: &str = "asistente ";
/// Cuantos encargos se ensenan, como mucho (los pendientes, todos).
const ENCARGOS_A_LA_VISTA: usize = 6;

/// Lo que el plugin ensena de una automatizacion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Automatizacion {
    pub id: String,
    pub nombre: String,
    pub activa: bool,
    /// `al_capturar`, `a_la_hora`, `al_sincronizar` o `a_mano`.
    pub disparador: String,
    pub hora: Option<String>,
}

/// Las automatizaciones guardadas (las del chat que tenga el fichero). Lo
/// que no se entiende se salta.
pub fn automatizaciones(proyectos: &[Proyecto]) -> Vec<Automatizacion> {
    for p in proyectos {
        let Ok(texto) = std::fs::read_to_string(p.carpeta.join(FICHERO)) else {
            continue;
        };
        return de_json(&texto);
    }
    Vec::new()
}

pub fn de_json(texto: &str) -> Vec<Automatizacion> {
    let Ok(v) = serde_json::from_str::<Value>(texto) else {
        return Vec::new();
    };
    v.get("automatizaciones")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| {
            let id = a.get("id")?.as_str()?.to_string();
            let nombre = a.get("nombre")?.as_str()?.trim().to_string();
            let d = a.get("disparador")?;
            let disparador = d.get("tipo")?.as_str()?.to_string();
            if id.is_empty() || nombre.is_empty() {
                return None;
            }
            Some(Automatizacion {
                id,
                nombre,
                activa: a.get("activa").and_then(Value::as_bool).unwrap_or(true),
                hora: d.get("hora").and_then(Value::as_str).map(str::to_string),
                disparador,
            })
        })
        .collect()
}

fn cuando(a: &Automatizacion) -> String {
    match (a.disparador.as_str(), &a.hora) {
        ("al_capturar", _) => "al capturar".into(),
        ("a_la_hora", Some(h)) => format!("cada día a las {h}"),
        ("al_sincronizar", _) => "al sincronizar".into(),
        ("a_mano", _) => "a mano".into(),
        (otro, _) => otro.replace('_', " "),
    }
}

// --- Los encargos ------------------------------------------------------------

/// Un encargo de la cola, tal como lo apunta la app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encargo {
    pub id: String,
    pub texto: String,
    /// `en_cola`, `haciendo`, `hecha` o `fallo`.
    pub estado: String,
    pub creado: i64,
    pub empezado: Option<i64>,
    pub acabado: Option<i64>,
    pub resumen: String,
    pub con_fallos: bool,
    /// `(proyecto, codigo, lienzo)` de lo que hizo.
    pub abrir: Option<(String, String, Option<String>)>,
}

impl Encargo {
    pub fn pendiente(&self) -> bool {
        matches!(self.estado.as_str(), "en_cola" | "haciendo")
    }
}

/// Los encargos del fichero, en su orden (el de llegada). Roto o sin
/// fichero: ninguno.
pub fn encargos_de_json(texto: &str) -> Vec<Encargo> {
    let Ok(v) = serde_json::from_str::<Value>(texto) else {
        return Vec::new();
    };
    let s = |e: &Value, k: &str| {
        e.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    v.get("encargos")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let id = s(e, "id");
            if id.is_empty() {
                return None;
            }
            let abrir = e.get("abrir").and_then(|a| {
                Some((
                    a.get("proyecto")?.as_str()?.to_string(),
                    a.get("codigo")?.as_str()?.to_string(),
                    a.get("lienzo").and_then(Value::as_str).map(str::to_string),
                ))
            });
            Some(Encargo {
                id,
                texto: s(e, "texto"),
                estado: s(e, "estado"),
                creado: e.get("creado").and_then(Value::as_i64).unwrap_or(0),
                empezado: e.get("empezado").and_then(Value::as_i64),
                acabado: e.get("acabado").and_then(Value::as_i64),
                resumen: s(e, "resumen"),
                con_fallos: e
                    .get("con_fallos")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                abrir,
            })
        })
        .collect()
}

/// Los encargos de la raiz de los datos.
pub fn encargos(raiz: &Path) -> Vec<Encargo> {
    std::fs::read_to_string(raiz.join(ENCARGOS))
        .map(|t| encargos_de_json(&t))
        .unwrap_or_default()
}

/// Si el encargo `id` ya esta apuntado (lo espera `pedir_y_seguir`).
pub fn apuntado(raiz: &Path, id: &str) -> bool {
    encargos(raiz).iter().any(|e| e.id == id)
}

/// Un id para el encargo que se va a mandar: la hora y lo pedido. La app
/// no apunta dos veces el mismo id (un Intro repetido).
pub fn id_de(texto: &str, ahora: i64) -> String {
    // FNV-1a: corto y estable.
    let mut h: u32 = 0x811c_9dc5;
    for b in texto.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    format!("f{ahora}-{h:08x}")
}

fn recortar(t: &str, n: usize) -> String {
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= n {
        return t;
    }
    let mut s: String = t.chars().take(n).collect();
    s.push('…');
    s
}

/// El pedido de abrir lo que hizo (el mismo del «Abrir» de su notificacion).
fn abrir_de(e: &Encargo) -> Value {
    match &e.abrir {
        Some((proyecto, _, Some(lienzo))) => pedido(
            "abrir",
            json!({ "que": { "tipo": "hoja", "proyecto": proyecto, "referencia": lienzo } }),
        ),
        Some((proyecto, codigo, None)) => pedido(
            "abrir",
            // La respuesta, en el pin del lector de notas (sin abrir el chat).
            json!({ "que": { "tipo": "nota", "proyecto": proyecto, "codigo": codigo } }),
        ),
        None => pedido("asistente", json!({})),
    }
}

/// **La fila de un encargo**: su estado, lo pedido y, al acabar, su resumen.
/// `delante`: cuantos tiene antes (los en cola).
pub fn resultado_de_encargo(e: &Encargo, delante: usize, ahora: i64) -> Resultado {
    let que = recortar(&crate::imagenes::sin_fichas(&e.texto), 70);
    let que = if que.is_empty() {
        "la foto".to_string()
    } else {
        que
    };
    let hace = |t: Option<i64>| crate::fecha::hace_con_horas(t.unwrap_or(e.creado), ahora);
    let (titulo, subtitulo) = match e.estado.as_str() {
        "en_cola" => (
            format!("⏳ En cola: {que}"),
            match delante {
                0 => "Es el siguiente · te aviso al acabar".to_string(),
                1 => "Tiene 1 delante · te aviso al acabar".to_string(),
                n => format!("Tiene {n} delante · te aviso al acabar"),
            },
        ),
        "haciendo" => (
            format!("⚙️ Haciendo: {que}"),
            format!(
                "Empezó {} · te aviso al acabar · Intro: abrir el chat «Asistente»",
                hace(e.empezado)
            ),
        ),
        "hecha" => (
            format!(
                "{} {que}",
                if e.con_fallos {
                    "⚠️ A medias:"
                } else {
                    "✅ Hecha:"
                }
            ),
            format!(
                "{} · {} · Intro: abrir",
                recortar(&e.resumen, 90),
                hace(e.acabado)
            ),
        ),
        _ => (
            format!("❌ Falló: {que}"),
            format!("{} · {}", recortar(&e.resumen, 90), hace(e.acabado)),
        ),
    };
    let accion = if e.pendiente() {
        pedido("asistente", json!({}))
    } else {
        abrir_de(e)
    };
    let mut r = Resultado::nuevo(titulo, subtitulo, glifo::ASISTENTE, Accion::Pedido(accion));
    r.clave = Some(format!("encargo/{}", e.id));
    r
}

/// Las filas de los encargos: el que se hace, los que esperan (por orden
/// de llegada) y los ultimos acabados (lo mas nuevo arriba).
pub fn filas_de_encargos(lista: &[Encargo], ahora: i64) -> Vec<Resultado> {
    let mut v = Vec::new();
    let mut delante = 0;
    for e in lista.iter().filter(|e| e.estado == "haciendo") {
        v.push(resultado_de_encargo(e, 0, ahora));
        delante += 1;
    }
    for e in lista.iter().filter(|e| e.estado == "en_cola") {
        v.push(resultado_de_encargo(e, delante, ahora));
        delante += 1;
    }
    let mut acabados: Vec<&Encargo> = lista.iter().filter(|e| !e.pendiente()).collect();
    acabados.sort_by_key(|e| std::cmp::Reverse(e.acabado.unwrap_or(e.creado)));
    v.extend(
        acabados
            .into_iter()
            .map(|e| resultado_de_encargo(e, 0, ahora)),
    );
    v.truncate(ENCARGOS_A_LA_VISTA.max(delante));
    v
}

// --- El motor ---------------------------------------------------------------

/// **El motor que contestara**, leido de `[asistente]` de `pindesk.toml`
/// (sin cargar un lector de TOML: una clave de texto). Vacio o `local` (de
/// cuando habia modelo en este PC, quitado el 27-oct-2026) es la nube que
/// tenga clave, Claude si ninguna (`asistente::nube::eleccion` en la app).
pub fn motor_de_toml(toml: &str) -> String {
    let mut en = false;
    let mut motor = String::new();
    for l in toml.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            en = l == "[asistente]";
            continue;
        }
        if !en {
            continue;
        }
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_matches('"').to_string();
        if k.trim() == "motor" {
            motor = v;
        }
    }
    match motor.as_str() {
        "claude" => "Claude Haiku 5.5".into(),
        "deepseek" => "DeepSeek Flash".into(),
        "" | "local" => "la nube (Claude Haiku 5.5 o DeepSeek)".into(),
        otro => otro.to_string(),
    }
}

/// El motor de los ajustes de PixPin (`<raiz>/pindesk.toml`, o el `pixpinmax.toml` de antes).
pub fn motor(raiz: &Path) -> String {
    motor_de_toml(&std::fs::read_to_string(crate::datos::ajustes_en(raiz)).unwrap_or_default())
}

/// **Las filas de `p asistente …`**. `destino_id`/`destino`: el chat de la
/// `@arroba` (o `null` y «en Mensajes guardados»).
pub fn resultados(
    proyectos: &[Proyecto],
    resto: &str,
    destino_id: Value,
    destino: &str,
    ctx: &Contexto,
) -> Vec<Resultado> {
    let mut v = Vec::new();
    let raiz = ctx.raiz_de_datos();
    let lista = raiz.as_deref().map(encargos).unwrap_or_default();
    let motor = raiz
        .as_deref()
        .map(motor)
        .unwrap_or_else(|| motor_de_toml(""));
    let pendientes = lista.iter().filter(|e| e.pendiente()).count();
    let abrir_chat = Resultado::nuevo(
        "Abrir el chat «Asistente»",
        "La conversación con el asistente, en PinDesk",
        glifo::CHAT,
        Accion::Pedido(pedido("asistente", json!({}))),
    );
    if resto.is_empty() {
        let mut r = Resultado::nuevo(
            "Pídele algo al asistente…",
            format!(
                "Escribe detrás de «asistente» y pulsa Intro: lo hace en segundo plano y te avisa al acabar · contesta {motor} · Ctrl+V pega una imagen · @proyecto"
            ),
            glifo::ASISTENTE,
            Accion::Consulta(ctx.consulta(VERBO)),
        );
        r.autocompletar = Some(ctx.consulta(VERBO));
        v.push(r);
        v.push(abrir_chat);
        v.extend(filas_de_encargos(&lista, ctx.ahora));
        for a in automatizaciones(proyectos) {
            let estado = if a.activa { "" } else { " · apagada" };
            let mut r = Resultado::nuevo(
                format!("▶ {}", a.nombre),
                format!(
                    "Automatización ({}){estado} · Intro: correrla ya",
                    cuando(&a)
                ),
                glifo::AUTOMATIZACION,
                Accion::Pedido(pedido("asistente", json!({ "automatizacion": a.id }))),
            );
            r.clave = Some(format!("automatizacion/{}", a.id));
            v.push(r);
        }
        return v;
    }
    let sobre = if destino_id.is_null() {
        String::new()
    } else {
        format!(" · sobre el chat {}", destino.trim_start_matches("en "))
    };
    let turno = match pendientes {
        0 => "lo empieza ya".to_string(),
        1 => "tiene 1 delante".to_string(),
        n => format!("tiene {n} delante"),
    };
    let id = id_de(resto, ctx.ahora);
    let mut r = Resultado::nuevo(
        format!("Pedir al asistente: {resto}"),
        format!(
            "Intro: a la cola ({turno}) · contesta {motor}{sobre} · te aviso con una notificación al acabar"
        ),
        glifo::ASISTENTE,
        Accion::PedirYSeguir {
            pedido: pedido(
                "asistente",
                json!({ "texto": resto, "proyecto": destino_id, "encargo": id }),
            ),
            consulta: ctx.consulta(VERBO),
        },
    );
    r.ayuda_titulo = Some(resto.to_string());
    // Con `[img NN]`, las imagenes van con el pedido (el modelo las ve).
    crate::imagenes::con_imagenes(&mut r, resto, ctx);
    v.push(r);
    v.extend(filas_de_encargos(&lista, ctx.ahora));
    v.push(abrir_chat);
    v
}
