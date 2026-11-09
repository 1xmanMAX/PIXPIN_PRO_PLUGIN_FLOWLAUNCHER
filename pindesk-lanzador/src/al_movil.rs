//! **`p s`: sincronizar con los aparatos del grupo, o mandarles la imagen
//! pegada** (7-oct; antes `p movil`, del 6-oct).
//!
//! El usuario, 7-oct: «dentro del plugin que haya la opcion de sincronizar …
//! me aparezca el listado de los celulares y su estado de conexion y si le
//! doy a uno se sincronice solo con ese, si le doy a todos con todos; pero si
//! entro a sincronizar y le pego la imagen, lo que pase es que se envia al
//! celular que seleccione, para no crear atajos al por mayor».
//!
//! Sale un resultado por cada aparato del grupo (menos este PC), leidos de
//! `<raiz>\sincro\identidad.json` en solo lectura como todo el plugin, y
//! «Todos» delante cuando hay mas de uno. Sin imagen, Intro manda el pedido
//! `sincronizar`; con fichas `[img NN]` (ver [`crate::imagenes`]), el pedido
//! `enviar_al_movil` con las imagenes. La app hace el resto en su hilo y lo
//! dice en el globo.
//!
//! Aqui no se pregunta a la red si el movil esta: el plugin arranca una vez
//! por tecla y un PING de un segundo por aparato se notaria en cada letra.
//! Lo dice la sonda de fondo de la app, que apunta lo que contesto cada uno
//! en `sincro\estado.json`; si no hay, si alguna vez contesto
//! (`sincro\direcciones.txt`).

use crate::normalizar::{normalizar, puntuar};
use crate::resultados::{Accion, Contexto, Resultado, glifo, pedido, pedido_ventana};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

/// Lo que se manda en `aparato` para todos los del grupo.
pub const TODOS: &str = "todos";

/// Lo que dijo la sonda deja de valer pasado esto: con nadie mirando, la
/// app pregunta cada minuto.
const ESTADO_VALE_MS: i64 = 3 * 60_000;

/// Si contesta ahora, segun la ultima sonda de la app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conexion {
    Conectado,
    Desconectado,
    /// Sin una sonda reciente: no se sabe.
    SinSaber,
}

/// Un aparato del grupo, como lo necesita el resultado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aparato {
    pub id: String,
    pub nombre: String,
    pub letra: Option<String>,
    /// Si alguna vez contesto a este PC (tiene direccion apuntada).
    pub conocido: bool,
    pub conexion: Conexion,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Ficha {
    id: String,
    nombre: String,
    letra: Option<String>,
}

/// Lo justo de `identidad.json` (el formato de `pindesk-proyecto`, que el
/// plugin no enlaza: arrastraria media app para leer tres campos).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Identidad {
    yo: Ficha,
    codigo: Option<String>,
    miembros: Vec<Ficha>,
}

/// `sincro\estado.json`, lo que apunta la sonda de la app
/// (`sincronizar::en_fondo::Estado`).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Estado {
    cuando: i64,
    responden: BTreeMap<String, bool>,
}

/// Los del grupo menos este equipo, los conectados primero. `None`: este PC
/// no esta en un grupo (sin fichero, sin codigo o un fichero que no se
/// entiende).
pub fn del_grupo(raiz: &Path, ahora: i64) -> Option<Vec<Aparato>> {
    let sincro = raiz.join("sincro");
    let texto = std::fs::read_to_string(sincro.join("identidad.json")).ok()?;
    let id: Identidad = serde_json::from_str(&texto).ok()?;
    if id.codigo.as_deref().is_none_or(|c| c.trim().is_empty()) {
        return None;
    }
    // `id \t host \t puerto`, el formato del movil.
    let direcciones = std::fs::read_to_string(sincro.join("direcciones.txt")).unwrap_or_default();
    let conocidos: Vec<&str> = direcciones
        .lines()
        .filter_map(|l| l.split('\t').next())
        .collect();
    let estado: Estado = std::fs::read_to_string(sincro.join("estado.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let reciente = estado.cuando > 0 && ahora - estado.cuando <= ESTADO_VALE_MS;
    let mut v: Vec<Aparato> = id
        .miembros
        .into_iter()
        .filter(|m| !m.id.is_empty() && m.id != id.yo.id)
        .map(|m| Aparato {
            conocido: conocidos.contains(&m.id.as_str()),
            conexion: match estado.responden.get(&m.id) {
                Some(true) if reciente => Conexion::Conectado,
                Some(false) if reciente => Conexion::Desconectado,
                _ => Conexion::SinSaber,
            },
            id: m.id,
            nombre: m.nombre,
            letra: m.letra,
        })
        .collect();
    // Los conectados primero; luego los que ya contestaron alguna vez, que
    // son los que de verdad se usan desde este PC.
    v.sort_by(|a, b| {
        (b.conexion == Conexion::Conectado)
            .cmp(&(a.conexion == Conexion::Conectado))
            .then_with(|| b.conocido.cmp(&a.conocido))
            .then_with(|| a.nombre.cmp(&b.nombre))
    });
    Some(v)
}

/// El pedido de mandar a `aparato` (las imagenes las pone `con_imagenes`).
pub(crate) fn pedido_al_movil(aparato: &str) -> serde_json::Value {
    pedido("enviar_al_movil", json!({ "aparato": aparato }))
}

fn pedido_sincronizar(aparato: &str) -> serde_json::Value {
    pedido("sincronizar", json!({ "aparato": aparato }))
}

/// Como esta, en palabras.
fn estado(a: &Aparato) -> &'static str {
    match a.conexion {
        Conexion::Conectado => "🟢 Conectado",
        Conexion::Desconectado => "⚪ Sin conexión",
        Conexion::SinSaber if a.conocido => "⚪ Sin comprobar ahora",
        Conexion::SinSaber => "⚪ Aún no se ha conectado con este PC",
    }
}

/// Lo que sale con `p s …`. `resto` es lo escrito detras: las fichas
/// `[img NN]` y, si se quiere, parte del nombre del aparato (o «todos»).
pub fn resultados(resto: &str, ctx: &Contexto) -> Vec<Resultado> {
    let grupo = ctx.raiz_de_datos().and_then(|r| del_grupo(&r, ctx.ahora));
    let Some(aparatos) = grupo else {
        return vec![abrir_sincronizar(
            "Este PC no está en un grupo",
            "Sincronizar en PinDesk: crea o únete al grupo de tu móvil · Intro la abre",
        )];
    };
    if aparatos.is_empty() {
        return vec![abrir_sincronizar(
            "Todavía no hay otro aparato en el grupo",
            "Une tu móvil al grupo en Sincronizar · Intro la abre",
        )];
    }
    let con_fichas = !crate::imagenes::fichas(resto).is_empty()
        || !crate::imagenes::fichas_de_archivo(resto).is_empty();
    let q = normalizar(&crate::imagenes::sin_fichas(resto));
    let pide_todos = !q.is_empty() && TODOS.starts_with(q.as_str());
    let encajan: Vec<&Aparato> = aparatos
        .iter()
        .filter(|a| q.is_empty() || puntuar(&q, &normalizar(&a.nombre), "") > 0)
        .collect();
    // Lo escrito no es ninguno (ni «todos»): mejor todos que ninguno.
    let elegibles: Vec<&Aparato> = if encajan.is_empty() && !pide_todos {
        aparatos.iter().collect()
    } else {
        encajan
    };
    let mut v = Vec::new();
    // «Todos», delante, si hay mas de uno y no se esta eligiendo uno.
    if aparatos.len() > 1 && (pide_todos || elegibles.len() == aparatos.len()) {
        let conectados = aparatos
            .iter()
            .filter(|a| a.conexion == Conexion::Conectado)
            .count();
        let cuantos = format!("{conectados} de {} conectados", aparatos.len());
        let mut r = if con_fichas {
            Resultado::nuevo(
                format!(
                    "Enviar a todos ({}) · a lo que tengan abierto",
                    aparatos.len()
                ),
                format!("{cuantos} · Intro: a cada uno, a su chat o lienzo abierto"),
                glifo::MOVIL,
                Accion::Pedido(pedido_al_movil(TODOS)),
            )
        } else {
            Resultado::nuevo(
                format!("Sincronizar con todos ({})", aparatos.len()),
                format!("{cuantos} · Intro: uno detrás de otro, en segundo plano"),
                glifo::SINCRONIZAR,
                Accion::Pedido(pedido_sincronizar(TODOS)),
            )
        };
        if con_fichas {
            crate::imagenes::con_imagenes(&mut r, resto, ctx);
        }
        r.clave = Some(format!("sincronizar/{TODOS}"));
        v.push(r);
    }
    for a in elegibles {
        let letra = a
            .letra
            .as_deref()
            .map(|l| format!("Letra {l}"))
            .unwrap_or_default();
        let que = if con_fichas {
            "Intro: a su chat abierto (un lienzo abierto solo acepta fotos)"
        } else {
            "Intro: sincroniza solo con este"
        };
        let sub = [estado(a), letra.as_str(), que]
            .into_iter()
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let mut r = if con_fichas {
            Resultado::nuevo(
                format!("Enviar a {} · a lo que tenga abierto", a.nombre),
                sub,
                glifo::MOVIL,
                Accion::Pedido(pedido_al_movil(&a.id)),
            )
        } else {
            Resultado::nuevo(
                format!("Sincronizar con {}", a.nombre),
                sub,
                glifo::MOVIL,
                Accion::Pedido(pedido_sincronizar(&a.id)),
            )
        };
        if con_fichas {
            crate::imagenes::con_imagenes(&mut r, resto, ctx);
        }
        r.clave = Some(format!("sincronizar/{}", a.id));
        v.push(r);
    }
    if !con_fichas {
        return v;
    }
    // Las fichas que quedan son de un borrador caducado (o de otro): sin una
    // sola imagen que mandar, Intro no mandaria nada.
    let lleva_algo = v.first().is_some_and(|r| match &r.accion {
        Accion::Pedido(p) => p.get("imagenes").is_some() || p.get("archivos").is_some(),
        _ => false,
    });
    if !lleva_algo {
        return vec![Resultado::nuevo(
            "Esa imagen ya no está: vuelve a pegarla con Ctrl+V",
            "Las imágenes pegadas caducan a la media hora",
            glifo::AVISO,
            Accion::Consulta(ctx.consulta("sincronizar ")),
        )];
    }
    v
}

fn abrir_sincronizar(titulo: &str, sub: &str) -> Resultado {
    let mut r = Resultado::nuevo(
        titulo,
        sub,
        glifo::AVISO,
        Accion::Pedido(pedido_ventana("sincronizar")),
    );
    r.clave = Some("ventana/sincronizar".into());
    r
}
