//! El icono de cada archivo, el del chat de PixPin: una hoja de su color con
//! la extension escrita, que pinta la app (pedido `iconos`,
//! docs/protocolo-pedidos.md) en `<raiz>/cache/iconos-de-extension/<ext>.png`.
//!
//! El plugin solo mira si ya esta: si falta, el resultado lleva el glifo de
//! siempre y se le pide a la app que lo pinte (sin arrancarla), para la
//! consulta siguiente.

use std::path::{Path, PathBuf};

/// Donde pinta la app los iconos.
pub fn carpeta(raiz: &Path) -> PathBuf {
    raiz.join("cache").join("iconos-de-extension")
}

/// La extension de un nombre de fichero, en minusculas: `Informe.PDF` →
/// `pdf`. Solo letras y numeros ASCII y diez como mucho (es un nombre de
/// fichero en la cache); sin extension, o con una rara, `None`.
pub fn extension(nombre: &str) -> Option<String> {
    let nombre = nombre.trim().rsplit(['/', '\\']).next()?;
    let (base, ext) = nombre.rsplit_once('.')?;
    if base.is_empty()
        || ext.is_empty()
        || ext.len() > 10
        || !ext.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// El pedido que pinta los que faltan.
pub fn pedido(extensiones: &[String]) -> serde_json::Value {
    serde_json::json!({ "pixpin": 1, "accion": "iconos", "extensiones": extensiones })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_extension_en_minusculas_y_solo_si_es_razonable() {
        assert_eq!(extension("Informe.PDF").as_deref(), Some("pdf"));
        assert_eq!(extension(r"C:\a.b\plano 2.dwg").as_deref(), Some("dwg"));
        assert_eq!(
            extension("pixpin:files/guardados/x.tar.gz").as_deref(),
            Some("gz")
        );
        assert_eq!(extension("sin_extension"), None);
        assert_eq!(extension(".gitignore"), None);
        assert_eq!(extension("raro.p d f"), None);
        assert_eq!(extension("largo.abcdefghijk"), None);
        assert_eq!(extension("punto."), None);
    }
}
