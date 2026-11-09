//! **Dos versiones de la misma leccion, juntas** (mejora del PC, 3-oct-2026).
//!
//! En el movil un archivo `.leccion` que cambio en los dos lados se trata
//! como una foto: gana el tocado mas tarde y lo del otro se pierde. Para una
//! leccion eso duele justo en lo que mas importa: si en el telefono se apunto
//! «me volvio a pasar» y en el PC se repaso, una de las dos cosas
//! desaparece. Aqui se juntan:
//!
//! - **Las repeticiones, todas** (la union, sin repetir). Solo crecen: nadie
//!   quita una repeticion, asi que juntarlas nunca resucita nada borrado. Si
//!   la union trae mas que la version que manda, la gravedad sube con la
//!   regla de `Repaso.repetida`.
//! - **Los textos, del tocado mas tarde** (`tocada`): titulo, que paso, por
//!   que, la proxima vez, tipo, area, causas, referencias…
//! - **El repaso, del que se repaso mas tarde.** Repasar no mueve `tocada`
//!   (asi lo hace el movil), asi que se mira el momento del repaso, que es
//!   `repasar` menos el intervalo de su caja.
//! - **Las etiquetas y las quitadas, la union**, solo en la fusion
//!   [`Modo::Completa`]: cuando lo de aqui es mas nuevo que lo que llega, lo
//!   que llega no vio estos cambios y juntar es lo justo. Cuando lo que llega
//!   es igual de nuevo o mas ([`Modo::Suave`]), lo normal es que ya traiga lo
//!   de aqui (es la siguiente version), y unir haria volver una etiqueta que
//!   alli se quito a proposito; entonces mandan las suyas.
//! - Los fotos y audios (`adjuntos`), la union. Lo desconocido de los dos.

use std::collections::BTreeSet;

use crate::leccion::{DIA, INTERVALOS, Leccion};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    /// Lo de aqui es mas nuevo que lo que llega: se une todo.
    Completa,
    /// Lo que llega manda en lo que se edita; solo se suma lo que crece.
    Suave,
}

/// El momento en que se repaso por ultima vez (o se creo).
fn momento_del_repaso(l: &Leccion) -> i64 {
    let caja = l.caja.clamp(0, INTERVALOS.len() as i64 - 1) as usize;
    l.repasar - INTERVALOS[caja] * DIA
}

fn unir(a: &[String], b: &[String]) -> Vec<String> {
    let mut v: Vec<String> = Vec::with_capacity(a.len() + b.len());
    for x in a.iter().chain(b) {
        if !v.contains(x) {
            v.push(x.clone());
        }
    }
    v
}

/// Junta `aqui` (la version de este equipo) con `llega` (la que trae la
/// sincronizacion).
pub fn fusionar(aqui: &Leccion, llega: &Leccion, modo: Modo) -> Leccion {
    // Los textos, del tocado mas tarde; a igualdad, lo que llega.
    let (base, otra) = if aqui.tocada > llega.tocada {
        (aqui, llega)
    } else {
        (llega, aqui)
    };
    let mut r = base.clone();
    r.tocada = aqui.tocada.max(llega.tocada);
    let repeticiones: BTreeSet<i64> = aqui
        .repeticiones
        .iter()
        .chain(&llega.repeticiones)
        .copied()
        .collect();
    r.repeticiones = repeticiones.into_iter().collect();
    r.adjuntos = unir(&base.adjuntos, &otra.adjuntos);
    let repaso = if momento_del_repaso(otra) > momento_del_repaso(base) {
        otra
    } else {
        base
    };
    r.caja = repaso.caja;
    r.repasar = repaso.repasar;
    if modo == Modo::Completa {
        r.etiquetas = unir(&base.etiquetas, &otra.etiquetas);
        r.quitadas = unir(&base.quitadas, &otra.quitadas);
        r.etiquetas_auto = unir(&base.etiquetas_auto, &otra.etiquetas_auto)
            .into_iter()
            .filter(|e| !r.quitadas.contains(e) && !r.etiquetas.contains(e))
            .collect();
        r.gravedad = aqui.gravedad.max(llega.gravedad);
    }
    if r.repeticiones.len() > base.repeticiones.len() {
        r.gravedad = if r.repeticiones.len() >= 2 {
            r.gravedad.max(3)
        } else {
            r.gravedad.max(2)
        };
    }
    if r.de_mensaje.is_none() {
        r.de_mensaje = otra.de_mensaje.clone();
    }
    for (k, v) in &otra.resto {
        r.resto.entry(k.clone()).or_insert_with(|| v.clone());
    }
    r
}

/// **Lo que la sincronizacion escribe en vez de lo que llega**, o `None` si
/// lo que llega vale tal cual (no hay nada de aqui que se pierda, o uno de
/// los dos no es una leccion legible: entonces no se toca nada).
///
/// El modo sale de las fechas: si lo de aqui se toco despues, lo que llega
/// no lo vio y se une todo ([`Modo::Completa`]); si no, [`Modo::Suave`].
pub fn al_llegar(aqui: &str, llega: &str) -> Option<String> {
    let a = Leccion::leer(aqui)?;
    let b = Leccion::leer(llega)?;
    if a.id != b.id {
        return None;
    }
    let modo = if a.tocada > b.tocada {
        Modo::Completa
    } else {
        Modo::Suave
    };
    let junta = fusionar(&a, &b, modo);
    (junta != b).then(|| junta.escribir())
}

/// Las dos cambiaron y se sabe (la sincronizacion vio el choque): se une
/// todo. `None` si alguna no se lee.
pub fn en_choque(aqui: &str, llega: &str) -> Option<String> {
    let a = Leccion::leer(aqui)?;
    let b = Leccion::leer(llega)?;
    if a.id != b.id {
        return None;
    }
    Some(fusionar(&a, &b, Modo::Completa).escribir())
}
