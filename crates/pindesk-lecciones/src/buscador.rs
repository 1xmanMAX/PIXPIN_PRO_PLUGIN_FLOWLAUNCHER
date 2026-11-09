//! **El buscador de las lecciones** (`Buscador.kt`): sin conexion y al
//! instante con miles.
//!
//! - Sin acentos, sin plurales y con erratas perdonadas ([`crate::texto`]):
//!   se dicta, y el dictado se equivoca.
//! - Busca en todo: lo aprendido pesa mas que el suceso, y las etiquetas y
//!   las **palabras de referencia** casi tanto como el titulo.
//! - **Por significado cercano**: una palabra que pertenece a un concepto del
//!   etiquetador busca tambien la etiqueta de ese concepto y su area.
//!   «Construcción» encuentra la leccion del encofrado aunque no lo diga.
//! - Las que se repiten y las graves suben.
//!
//! Cada leccion se prepara una vez ([`Indice`]) y se reutiliza mientras no
//! cambie: buscar es solo comparar palabras ya cortadas.

use std::collections::HashSet;

use crate::etiquetador;
use crate::leccion::Leccion;
use crate::texto;

const TITULO: f64 = 3.0;
const ETIQUETA: f64 = 2.5;
const REFERENCIA: f64 = 2.2;
const PROXIMA: f64 = 1.6;
const PORQUE: f64 = 1.3;
const AREA: f64 = 1.2;
const PASO: f64 = 1.0;

/// Una leccion ya cortada en palabras, campo por campo, con su peso.
#[derive(Debug, Clone)]
pub struct Indice {
    pub leccion: Leccion,
    pub campos: Vec<(HashSet<String>, f64)>,
    pub todas: HashSet<String>,
}

impl Indice {
    pub fn nuevo(leccion: Leccion) -> Indice {
        let de = |s: &str| texto::raices(s).into_iter().collect::<HashSet<_>>();
        let de_lista = |v: &[String]| {
            v.iter()
                .flat_map(|s| texto::raices(s))
                .collect::<HashSet<_>>()
        };
        let mut porque = de(&leccion.por_que);
        porque.extend(de_lista(&leccion.causas));
        let campos = vec![
            (de(&leccion.titulo), TITULO),
            (de_lista(&leccion.todas_las_etiquetas()), ETIQUETA),
            (de_lista(&leccion.referencias), REFERENCIA),
            (de(&leccion.proxima), PROXIMA),
            (porque, PORQUE),
            (de(&leccion.area), AREA),
            (de(&leccion.que_paso), PASO),
        ];
        let todas = campos.iter().flat_map(|(c, _)| c.iter().cloned()).collect();
        Indice {
            leccion,
            campos,
            todas,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Resultado {
    pub leccion: Leccion,
    pub puntos: f64,
}

/// Que tan bien casa la palabra buscada `q` con la palabra `p` de la
/// leccion: 1 exacta, menos con prefijo o errata.
fn casa(q: &str, p: &str) -> f64 {
    if q == p {
        return 1.0;
    }
    if q.chars().count() >= 3 && p.starts_with(q) {
        return 0.8;
    }
    let tope = texto::erratas(q);
    if tope > 0 && texto::distancia(q, p, tope) <= tope {
        return 0.6;
    }
    0.0
}

/// Lo buscado: cada raiz con su peso, mas las etiquetas y areas de los
/// conceptos a los que apunta.
struct Consulta {
    terminos: Vec<(String, f64)>,
    cuantas: usize,
}

fn sin_repetir(v: Vec<String>) -> Vec<String> {
    let mut vistas = HashSet::new();
    v.into_iter().filter(|x| vistas.insert(x.clone())).collect()
}

fn consulta(texto_buscado: &str) -> Consulta {
    let raices = sin_repetir(texto::raices(texto_buscado));
    let extra: Vec<String> = sin_repetir(
        raices
            .iter()
            .flat_map(|r| {
                etiquetador::conceptos_de(r).into_iter().flat_map(|c| {
                    texto::raices(c.etiqueta)
                        .into_iter()
                        .chain(texto::raices(c.area))
                })
            })
            .collect(),
    )
    .into_iter()
    .filter(|e| !raices.contains(e))
    .collect();
    let cuantas = raices.len();
    let terminos = raices
        .into_iter()
        .map(|r| (r, 1.0))
        .chain(extra.into_iter().map(|e| (e, 0.5)))
        .collect();
    Consulta { terminos, cuantas }
}

fn ordenar(v: &mut [Resultado]) {
    v.sort_by(|a, b| {
        b.puntos
            .partial_cmp(&a.puntos)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

pub fn buscar(indices: &[Indice], texto_buscado: &str) -> Vec<Resultado> {
    let q = consulta(texto_buscado);
    if q.terminos.is_empty() {
        return Vec::new();
    }
    let mut salida = Vec::new();
    for ix in indices {
        let mut puntos = 0.0;
        let mut halladas = 0usize;
        for (i, (palabra, peso_de_la_palabra)) in q.terminos.iter().enumerate() {
            let mut mejor: f64 = 0.0;
            for (palabras, peso_del_campo) in &ix.campos {
                if palabras.is_empty() {
                    continue;
                }
                if palabras.contains(palabra) {
                    mejor = mejor.max(*peso_del_campo);
                    continue;
                }
                for p in palabras {
                    let c = casa(palabra, p);
                    if c > 0.0 {
                        mejor = mejor.max(c * peso_del_campo);
                    }
                }
            }
            if mejor > 0.0 && i < q.cuantas {
                halladas += 1;
            }
            puntos += mejor * peso_de_la_palabra;
        }
        if puntos <= 0.0 {
            continue;
        }
        // Las palabras buscadas que no salen restan.
        let cubre = if q.cuantas == 0 {
            1.0
        } else {
            halladas as f64 / q.cuantas as f64
        };
        if q.cuantas > 0 && halladas == 0 {
            // Solo caso por concepto: vale, pero va detras.
            puntos *= 0.5;
        } else {
            puntos *= 0.4 + 0.6 * cubre;
        }
        let l = &ix.leccion;
        puntos *= 1.0
            + 0.15 * l.repeticiones.len() as f64
            + 0.1 * (l.gravedad - 1) as f64
            + if l.es_error() { 0.1 } else { 0.0 };
        salida.push(Resultado {
            leccion: l.clone(),
            puntos,
        });
    }
    ordenar(&mut salida);
    salida
}

/// **¿Ya tienes una leccion asi?** Mientras se escribe una nueva, las que se
/// le parecen. Si se parece mucho, lo que toca es apuntar que **volvio a
/// pasar**. Parecido = proporcion de raices del texto nuevo que salen en la
/// leccion (y al reves).
pub fn parecidas(
    indices: &[Indice],
    texto_nuevo: &str,
    minimo: f64,
    cuantas: usize,
) -> Vec<Resultado> {
    let nuevas: HashSet<String> = texto::raices(texto_nuevo).into_iter().collect();
    if nuevas.len() < 2 {
        return Vec::new();
    }
    let mut v: Vec<Resultado> = indices
        .iter()
        .filter_map(|ix| {
            let suyas = &ix.todas;
            if suyas.is_empty() {
                return None;
            }
            let comunes = nuevas
                .iter()
                .filter(|n| suyas.contains(*n) || suyas.iter().any(|s| casa(n, s) >= 0.8))
                .count();
            let a = comunes as f64 / nuevas.len() as f64;
            let b = comunes as f64 / suyas.len().clamp(1, 12) as f64;
            let s = (2.0 * a * b) / (a + b).max(1e-9);
            (s >= minimo).then(|| Resultado {
                leccion: ix.leccion.clone(),
                puntos: s,
            })
        })
        .collect();
    ordenar(&mut v);
    v.truncate(cuantas);
    v
}

/// Con los valores por defecto del movil (0,45 y tres).
pub fn parecidas_por_defecto(indices: &[Indice], texto_nuevo: &str) -> Vec<Resultado> {
    parecidas(indices, texto_nuevo, 0.45, 3)
}

/// **Relacionadas**: las que comparten etiquetas o palabras con esta.
pub fn relacionadas(indices: &[Indice], l: &Leccion, cuantas: usize) -> Vec<Leccion> {
    let propia;
    let mia = match indices.iter().find(|ix| ix.leccion.id == l.id) {
        Some(ix) => &ix.todas,
        None => {
            propia = Indice::nuevo(l.clone());
            &propia.todas
        }
    };
    let etiquetas: HashSet<String> = l.todas_las_etiquetas().into_iter().collect();
    let mut v: Vec<(Leccion, usize)> = indices
        .iter()
        .filter(|ix| ix.leccion.id != l.id)
        .map(|ix| {
            let comunes = ix.todas.iter().filter(|x| mia.contains(*x)).count();
            let mismas = ix
                .leccion
                .todas_las_etiquetas()
                .iter()
                .filter(|e| etiquetas.contains(*e))
                .count();
            (ix.leccion.clone(), comunes + 2 * mismas)
        })
        .filter(|(_, n)| *n >= 3)
        .collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    v.into_iter().take(cuantas).map(|(l, _)| l).collect()
}

/// **Las de este sitio**: para un proyecto, las lecciones que hablan de lo
/// mismo que su nombre. Es el aviso en el momento justo.
pub fn para_el_contexto(indices: &[Indice], contexto: &str, cuantas: usize) -> Vec<Leccion> {
    if contexto.trim().is_empty() {
        return Vec::new();
    }
    buscar(indices, contexto)
        .into_iter()
        .filter(|r| r.puntos >= 2.0)
        .map(|r| r.leccion)
        .take(cuantas)
        .collect()
}
