//! **La puerta de PinDesk en Flow** (9-nov-2026, el id `flow` del catalogo,
//! docs/licencias.md): lo que decide la app y el plugin obedece.
//!
//! El plugin corre en el proceso de Flow y no comprueba firmas ni lee la
//! cuenta: la app (`apps/pindesk/src/puerta_de_flow.rs`) escribe
//! `<raiz>/cache/flow-puerta.json` cuando PinDesk en Flow **no esta
//! disponible** (oculto en el catalogo de la edicion restringida) o **falta
//! entrar con Google**, con el titulo y el subtitulo ya traducidos al idioma
//! de la app. Mientras el fichero esta, cada consulta contesta **un solo
//! resultado** con ese texto, y el menu de un resultado sale vacio. Sin el
//! fichero (la edicion completa, o con `flow` abierto) todo es como siempre.
//!
//! Tocar el fichero solo cambia lo que se ensena al buscar: lo que el plugin
//! le pide a la app lo vuelve a mirar la app (`origen: "flow"` en cada
//! pedido, [`crate::pedido`]).

use std::path::Path;

use serde::Deserialize;
use serde_json::json;

use crate::resultados::{self, Accion, Resultado, glifo};

/// El fichero, en `<raiz>/cache`.
pub const FICHERO: &str = "flow-puerta.json";

/// Lo que dice el fichero.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Puerta {
    /// `"oculta"` (no esta en esta PinDesk) o `"entrar"` (falta entrar con
    /// Google).
    pub puerta: String,
    pub titulo: String,
    #[serde(default)]
    pub subtitulo: String,
}

impl Puerta {
    /// Si lo que falta es entrar con la cuenta.
    pub fn falta_entrar(&self) -> bool {
        self.puerta == "entrar"
    }

    /// **El unico resultado**: con la cuenta, abrir PinDesk (que pide
    /// entrar); oculta, se queda lo tecleado tal cual.
    pub fn resultado(&self, palabra_clave: &str, busqueda: &str) -> Resultado {
        let (g, accion) = if self.falta_entrar() {
            (
                glifo::PINDESK,
                Accion::Pedido(resultados::pedido("ventana_principal", json!({}))),
            )
        } else {
            (
                glifo::AVISO,
                Accion::Consulta(format!("{palabra_clave} {busqueda}")),
            )
        };
        Resultado::nuevo(self.titulo.clone(), self.subtitulo.clone(), g, accion)
    }
}

/// La puerta de ahora, si la app dejo el fichero (y se entiende).
pub fn leer(raiz: &Path) -> Option<Puerta> {
    let b = std::fs::read(raiz.join("cache").join(FICHERO)).ok()?;
    serde_json::from_slice::<Puerta>(&b)
        .ok()
        .filter(|p| !p.titulo.trim().is_empty() && matches!(p.puerta.as_str(), "oculta" | "entrar"))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn escribir(raiz: &Path, texto: &str) {
        std::fs::create_dir_all(raiz.join("cache")).unwrap();
        std::fs::write(raiz.join("cache").join(FICHERO), texto).unwrap();
    }

    #[test]
    fn sin_fichero_o_roto_no_hay_puerta() {
        let d = std::env::temp_dir().join(format!("pindesk-puerta-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(leer(&d), None);
        escribir(&d, "{roto");
        assert_eq!(leer(&d), None);
        escribir(&d, r#"{"puerta": "abierta", "titulo": "x"}"#);
        assert_eq!(leer(&d), None);
        escribir(&d, r#"{"puerta": "oculta", "titulo": ""}"#);
        assert_eq!(leer(&d), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn oculta_un_resultado_que_no_hace_nada_y_entrar_abre_pindesk() {
        let d = std::env::temp_dir().join(format!("pindesk-puerta2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        escribir(
            &d,
            r#"{"version": 1, "puerta": "oculta", "titulo": "No disponible", "subtitulo": "En tu versión no"}"#,
        );
        let p = leer(&d).unwrap();
        let r = p.resultado("p", "t comprar pan");
        assert_eq!(r.titulo, "No disponible");
        assert_eq!(r.subtitulo, "En tu versión no");
        assert!(matches!(&r.accion, Accion::Consulta(c) if c == "p t comprar pan"));

        escribir(&d, r#"{"puerta": "entrar", "titulo": "Entra con Google"}"#);
        let p = leer(&d).unwrap();
        assert!(p.falta_entrar());
        let r = p.resultado("p", "");
        assert!(matches!(&r.accion, Accion::Pedido(v) if v["accion"] == "ventana_principal"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
