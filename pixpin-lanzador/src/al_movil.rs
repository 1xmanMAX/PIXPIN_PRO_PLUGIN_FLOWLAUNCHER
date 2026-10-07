//! **`p movil` + Ctrl+V: la imagen al lienzo abierto del movil** (6-oct).
//!
//! El usuario: «en el chat del plugin pueda yo pegar la imagen y elegir el
//! dispositivo entre los dispositivos sincronizados dentro del grupo». Sale
//! un resultado por cada aparato del grupo (menos este PC), leidos de
//! `<raiz>\sincro\identidad.json` en solo lectura como todo el plugin; Intro
//! manda el pedido `enviar_al_movil` con las imagenes del borrador
//! (`[img 01]`, ver [`crate::imagenes`]) y la app hace el resto en su hilo
//! (`sincronizar::al_movil`): localizarlo, mandar y avisar.
//!
//! Aqui no se pregunta a la red si el movil esta: el plugin arranca una vez
//! por tecla y un PING de un segundo por aparato se notaria en cada letra.
//! Lo que se sabe sin salir del disco es si alguna vez contesto
//! (`sincro\direcciones.txt`).

use crate::normalizar::{normalizar, puntuar};
use crate::resultados::{Accion, Contexto, Resultado, glifo, pedido, pedido_ventana};
use serde::Deserialize;
use serde_json::json;
use std::path::Path;

/// Un aparato del grupo, como lo necesita el resultado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aparato {
    pub id: String,
    pub nombre: String,
    pub letra: Option<String>,
    /// Si alguna vez contesto a este PC (tiene direccion apuntada).
    pub conocido: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Ficha {
    id: String,
    nombre: String,
    letra: Option<String>,
}

/// Lo justo de `identidad.json` (el formato de `pixpin-proyecto`, que el
/// plugin no enlaza: arrastraria media app para leer tres campos).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Identidad {
    yo: Ficha,
    codigo: Option<String>,
    miembros: Vec<Ficha>,
}

/// Los del grupo menos este equipo. `None`: este PC no esta en un grupo (sin
/// fichero, sin codigo o un fichero que no se entiende).
pub fn del_grupo(raiz: &Path) -> Option<Vec<Aparato>> {
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
    let mut v: Vec<Aparato> = id
        .miembros
        .into_iter()
        .filter(|m| !m.id.is_empty() && m.id != id.yo.id)
        .map(|m| Aparato {
            conocido: conocidos.contains(&m.id.as_str()),
            id: m.id,
            nombre: m.nombre,
            letra: m.letra,
        })
        .collect();
    // Los que ya contestaron alguna vez, primero: son los que de verdad se
    // usan desde este PC.
    v.sort_by(|a, b| {
        b.conocido
            .cmp(&a.conocido)
            .then_with(|| a.nombre.cmp(&b.nombre))
    });
    Some(v)
}

/// El pedido de mandar a `aparato` (las imagenes las pone `con_imagenes`).
pub(crate) fn pedido_al_movil(aparato: &str) -> serde_json::Value {
    pedido("enviar_al_movil", json!({ "aparato": aparato }))
}

/// Lo que sale con `p movil …`. `resto` es lo escrito detras: las fichas
/// `[img NN]` y, si se quiere, parte del nombre del movil.
pub fn resultados(resto: &str, ctx: &Contexto) -> Vec<Resultado> {
    let grupo = ctx.raiz_de_datos().and_then(|r| del_grupo(&r));
    let Some(aparatos) = grupo else {
        return vec![abrir_sincronizar(
            "Este PC no está en un grupo",
            "Sincronizar en PixPin: crea o únete al grupo de tu móvil · Intro la abre",
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
    if !con_fichas {
        return vec![Resultado::nuevo(
            "Pega una imagen con Ctrl+V",
            "Cópiala, pulsa Ctrl+V aquí (sale [img 01]) y elige el móvil: va al lienzo que tenga abierto",
            glifo::MOVIL,
            Accion::Consulta(ctx.consulta(&format!("{} ", resto.trim()))),
        )];
    }
    let q = normalizar(&crate::imagenes::sin_fichas(resto));
    let encajan: Vec<&Aparato> = aparatos
        .iter()
        .filter(|a| q.is_empty() || puntuar(&q, &normalizar(&a.nombre), "") > 0)
        .collect();
    // Lo escrito no es ninguno: mejor todos que ninguno.
    let elegibles = if encajan.is_empty() {
        aparatos.iter().collect()
    } else {
        encajan
    };
    let mut v = Vec::new();
    for a in elegibles {
        let letra = a
            .letra
            .as_deref()
            .map(|l| format!("Letra {l}"))
            .unwrap_or_default();
        let estado = if a.conocido {
            "Intro: al lienzo que tenga abierto (si no hay, a su chat)"
        } else {
            "Aún no se ha conectado con este PC · Intro: lo busca en la Wi-Fi"
        };
        let sub = [letra.as_str(), estado]
            .into_iter()
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let mut r = Resultado::nuevo(
            format!("Enviar a {} · al lienzo abierto", a.nombre),
            sub,
            glifo::MOVIL,
            Accion::Pedido(pedido_al_movil(&a.id)),
        );
        crate::imagenes::con_imagenes(&mut r, resto, ctx);
        r.clave = Some(format!("movil/{}", a.id));
        v.push(r);
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
            Accion::Consulta(ctx.consulta("móvil ")),
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
