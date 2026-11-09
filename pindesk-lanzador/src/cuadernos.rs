//! **`p carpeta`: las carpetas de paginas del chat** (26-oct-2026).
//!
//! El usuario: «añade acceso a las nuevas funciones … la de crear y añadir
//! paginas a las carpetas» en el plugin. Una carpeta es un mensaje
//! `MINIAPP` con la palabra `carpeta` y sus paginas en `texto`
//! (`crates/pindesk-proyecto/src/carpeta_de_paginas.rs`); aqui solo se lee.
//!
//! Sale cada carpeta (Intro: su ventana, con las paginas en orden) y debajo
//! «Nueva página en …» (la pagina siguiente, que se abre en el lienzo). Con
//! texto que no es el nombre de ninguna, «Nueva carpeta «texto»». Lo hace la
//! app con el pedido `carpeta_paginas`.

use crate::datos::{Mensaje, Proyecto};
use crate::normalizar::normalizar;
use crate::resultados::{Accion, Contexto, Resultado, glifo, pedido};
use serde_json::{Value, json};

/// La palabra de `Mensaje.miniapp` de una carpeta.
pub const MINIAPP: &str = "carpeta";

/// Cuantas carpetas se ensenan, con su «Nueva página» cada una.
const TOPE: usize = 6;

pub fn es_carpeta(m: &Mensaje) -> bool {
    m.clase == "MINIAPP" && m.miniapp.as_deref() == Some(MINIAPP)
}

fn documento(m: &Mensaje) -> Option<Value> {
    serde_json::from_str::<Value>(m.texto()).ok()
}

/// Cuantas paginas lleva (las borradas del chat aun cuentan: la app las
/// salta al abrirla).
pub fn paginas(m: &Mensaje) -> usize {
    documento(m)
        .and_then(|v| v.get("paginas")?.as_array().map(Vec::len))
        .unwrap_or(0)
}

/// Si es la carpeta de una clase o reunion grabada (`tipo: "grabacion"`,
/// `carpeta_de_paginas::TIPO_GRABACION`): se abre, pero no se le anaden
/// paginas.
pub fn es_grabacion(m: &Mensaje) -> bool {
    documento(m).is_some_and(|v| v.get("tipo").and_then(Value::as_str) == Some("grabacion"))
}

pub fn nombre(m: &Mensaje) -> &str {
    match m.nombre() {
        "" => "Cuaderno",
        n => n,
    }
}

fn cuantas(n: usize) -> String {
    match n {
        0 => "Vacía".into(),
        1 => "1 página".into(),
        n => format!("{n} páginas"),
    }
}

fn pedido_carpeta(hacer: &str, p: &Proyecto, m: &Mensaje) -> Value {
    pedido(
        "carpeta_paginas",
        json!({ "hacer": hacer, "proyecto": p.id_para_pedido(), "carpeta": m.id }),
    )
}

/// La fila de una carpeta: Intro la abre.
pub fn resultado(p: &Proyecto, m: &Mensaje) -> Resultado {
    let mut r = Resultado::nuevo(
        nombre(m),
        if es_grabacion(m) {
            format!("Clase o reunión grabada · {} · Intro: abrirla", p.nombre)
        } else {
            format!(
                "Carpeta de páginas · {} · {} · Intro: abrirla",
                cuantas(paginas(m)),
                p.nombre
            )
        },
        glifo::CARPETA_PAGINAS,
        Accion::Pedido(pedido_carpeta("abrir", p, m)),
    );
    r.clave = Some(format!("carpeta/{}/{}", p.id, m.id));
    r
}

fn resultado_pagina(p: &Proyecto, m: &Mensaje) -> Resultado {
    let mut r = Resultado::nuevo(
        format!("Nueva página en «{}»", nombre(m)),
        "La plantilla de la última página, vacía, y se abre en el lienzo",
        glifo::ANADIR,
        Accion::Pedido(pedido_carpeta("pagina", p, m)),
    );
    r.clave = Some(format!("carpeta-pagina/{}/{}", p.id, m.id));
    r
}

/// `p carpeta [texto]`: las carpetas que se llaman asi (todas sin texto),
/// las mas recientes primero, y crear una. `en`: el proyecto del `@`, si se
/// eligio uno; `destino_id` y `destino`, donde va la nueva.
pub fn resultados(
    proyectos: &[Proyecto],
    texto: &str,
    en: Option<&str>,
    destino_id: Value,
    destino: &str,
    ctx: &Contexto,
) -> Vec<Resultado> {
    let buscado = normalizar(texto.trim());
    let mut todas: Vec<(&Proyecto, &Mensaje)> = proyectos
        .iter()
        .filter(|p| en.is_none_or(|id| p.id == id))
        .flat_map(|p| {
            p.mensajes
                .iter()
                .filter(|m| es_carpeta(m))
                .map(move |m| (p, m))
        })
        .filter(|(_, m)| buscado.is_empty() || normalizar(nombre(m)).contains(&buscado))
        .collect();
    todas.sort_by_key(|(_, m)| std::cmp::Reverse(m.cuando));
    let exacta = todas.iter().any(|(_, m)| normalizar(nombre(m)) == buscado);
    let mut v = Vec::new();
    for (p, m) in todas.iter().take(TOPE) {
        v.push(resultado(p, m));
        if !es_grabacion(m) {
            v.push(resultado_pagina(p, m));
        }
    }
    if !exacta {
        let (titulo, sub) = if buscado.is_empty() {
            (
                "Nueva carpeta de páginas".to_string(),
                format!("Se llamará «Cuaderno» {destino} · «carpeta <nombre>» para nombrarla"),
            )
        } else {
            (
                format!("Nueva carpeta «{}»", texto.trim()),
                format!("Intro: crearla {destino} y abrirla"),
            )
        };
        let mut r = Resultado::nuevo(
            titulo,
            sub,
            glifo::ANADIR,
            Accion::Pedido(pedido(
                "carpeta_paginas",
                json!({ "hacer": "nueva", "proyecto": destino_id, "nombre": texto.trim() }),
            )),
        );
        if buscado.is_empty() {
            r.autocompletar = Some(ctx.consulta("carpeta "));
        }
        // Despues de las carpetas: lo de todos los dias es abrir una.
        v.push(r);
    }
    v
}
