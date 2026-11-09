//! **«¿Que aprendiste?» en una linea** (lo anade el PC, rediseno v2,
//! 4-oct-2026).
//!
//! La barra de arriba de la lista: se escribe (o se dicta) una frase y todo
//! lo demas se rellena solo, para cambiarlo con un clic si no acierta:
//!
//! - los campos, repartidos como el dictado del movil (`Dictado.kt`): «paso
//!   que…, porque…, la proxima vez…»;
//! - etiquetas, area, tipo y causas, del [`etiquetador`] de siempre;
//! - la gravedad ([`etiquetador::proponer_gravedad`]);
//! - el proyecto: uno cuyo nombre sale en la frase o, si no, el de la
//!   leccion que mas se le parece.
//!
//! El resultado es una [`Leccion`] normal: el formato del archivo no cambia.

use crate::buscador::{self, Indice};
use crate::etiquetador::{self, Aprendido, Propuesta};
use crate::leccion::{Leccion, TIPO_LECCION};
use crate::{dictado, texto};

/// Lo que se propone para una frase, antes de guardarla.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Relleno {
    pub propuesta: Propuesta,
    pub gravedad: i64,
    /// La ficha del proyecto propuesto, si hay uno claro.
    pub proyecto: Option<String>,
}

/// Lo que se propone para `frase`. `proyectos` son `(ficha, nombre)`;
/// `de_quien` dice la ficha de cada leccion por su id (para proponer el
/// proyecto de la que mas se parece).
pub fn rellenar(
    frase: &str,
    aprendido: &Aprendido,
    indices: &[Indice],
    proyectos: &[(String, String)],
    de_quien: &dyn Fn(&str) -> Option<String>,
) -> Relleno {
    if frase.trim().is_empty() {
        return Relleno {
            gravedad: 1,
            ..Relleno::default()
        };
    }
    let propuesta = etiquetador::proponer(frase, aprendido, &[]);
    let gravedad = etiquetador::proponer_gravedad(frase, propuesta.tipo.as_deref());
    let proyecto = proyecto_por_nombre(frase, proyectos).or_else(|| {
        buscador::parecidas(indices, frase, 0.3, 1)
            .first()
            .and_then(|r| de_quien(&r.leccion.id))
    });
    Relleno {
        propuesta,
        gravedad,
        proyecto,
    }
}

/// El proyecto cuyo nombre sale en la frase: todas sus palabras con
/// significado, dos de ellas o una larga estan en ella. Entre varios, el que
/// casa mas.
pub fn proyecto_por_nombre(frase: &str, proyectos: &[(String, String)]) -> Option<String> {
    let mias: Vec<String> = texto::raices(frase);
    let mut mejor: Option<(&str, usize)> = None;
    for (ficha, nombre) in proyectos {
        let suyas: Vec<String> = texto::raices(nombre);
        if suyas.is_empty() {
            continue;
        }
        let casan: Vec<&String> = suyas.iter().filter(|r| mias.contains(r)).collect();
        let comunes = casan.len();
        // Una palabra larga basta (un sitio: «Miraflores»); una corta y
        // comun («obra») sola no.
        let vale =
            comunes == suyas.len() || comunes >= 2 || casan.iter().any(|r| r.chars().count() >= 6);
        if vale && mejor.is_none_or(|(_, n)| comunes > n) {
            mejor = Some((ficha, comunes));
        }
    }
    mejor.map(|(f, _)| f.to_string())
}

/// La leccion que sale de la frase con lo propuesto (y lo que se haya
/// cambiado a mano encima: `area`, `gravedad`).
pub fn leccion(
    frase: &str,
    id: &str,
    ahora: i64,
    propuesta: &Propuesta,
    area: Option<&str>,
    gravedad: i64,
) -> Leccion {
    let escritas = etiquetador::escritas(frase);
    let limpio = etiquetador::sin_etiquetas(frase);
    let c = dictado::repartir(&limpio);
    let auto: Vec<String> = propuesta
        .etiquetas
        .iter()
        .filter(|e| !escritas.contains(e))
        .cloned()
        .collect();
    Leccion {
        titulo: c.titulo.trim().to_string(),
        que_paso: c.que_paso.trim().to_string(),
        por_que: c.por_que.trim().to_string(),
        proxima: if c.proxima != c.titulo {
            c.proxima.trim().to_string()
        } else {
            String::new()
        },
        tipo: propuesta
            .tipo
            .clone()
            .unwrap_or_else(|| TIPO_LECCION.to_string()),
        area: area
            .map(str::to_string)
            .or_else(|| propuesta.area.clone())
            .unwrap_or_default(),
        gravedad: gravedad.clamp(1, 3),
        etiquetas: escritas,
        etiquetas_auto: auto,
        causas: propuesta.causas.clone(),
        ..Leccion::nueva(id, ahora, "")
    }
}
