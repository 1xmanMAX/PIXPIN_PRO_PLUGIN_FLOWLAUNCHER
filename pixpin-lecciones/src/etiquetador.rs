//! **Las etiquetas que se ponen solas**, sin conexion y al momento
//! (`Etiquetador.kt`).
//!
//! Tres fuentes, de mas a menos fiable:
//! 1. Lo que uno escribio con `#` o dijo como «etiqueta obra».
//! 2. **Lo que uno suele etiquetar asi** ([`aprender`]): si las lecciones
//!    donde salia «encofrado» llevaban #losas, una nueva con «encofrado»
//!    propone #losas.
//! 3. Un diccionario de conceptos ([`conceptos`]) pensado para quien lo usa:
//!    estudiante de ingenieria, obra, estudio, trabajo y vida diaria. Cada
//!    concepto da una etiqueta y apunta a un area.
//!
//! Tambien propone el tipo (error o acierto) y las causas por las palabras
//! que se usan al contarlo. Nada se impone: sale como propuesta y se quita
//! con un toque, y lo quitado no vuelve ([`crate::Leccion::quitadas`]).
//!
//! El diccionario es **el mismo que el del movil, palabra por palabra**: si
//! cambia alli, se copia aqui.
//!
//! Kotlin usa expresiones regulares; aqui se buscan a mano con las mismas
//! reglas (palabra entera, `#` seguido de 2 a 30 letras…) para no arrastrar
//! un motor de expresiones regulares por cuatro patrones.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::leccion::{Leccion, TIPO_ACIERTO, TIPO_ERROR};
use crate::texto;

pub struct Concepto {
    pub etiqueta: &'static str,
    pub area: &'static str,
    pub raices: Vec<String>,
}

const OBRA: &str = "Construcción";
const ESTUDIO: &str = "Estudio";
const TRABAJO: &str = "Trabajo";
const VIDA: &str = "Vida diaria";

const DICCIONARIO: &[(&str, &str, &[&str])] = &[
    (
        "concreto",
        OBRA,
        &[
            "concreto",
            "hormigón",
            "losa",
            "vaciado",
            "colado",
            "curado",
            "fraguado",
            "slump",
            "mezcla",
            "cemento",
            "fisura",
            "grieta",
            "fisuró",
            "agrietó",
        ],
    ),
    (
        "acero",
        OBRA,
        &[
            "acero",
            "varilla",
            "fierro",
            "armado",
            "estribo",
            "traslape",
            "anclaje",
            "recubrimiento",
            "malla",
        ],
    ),
    (
        "encofrado",
        OBRA,
        &[
            "encofrado",
            "cimbra",
            "puntal",
            "desencofrado",
            "formaleta",
            "molde",
        ],
    ),
    (
        "estructuras",
        OBRA,
        &[
            "estructura",
            "viga",
            "columna",
            "zapata",
            "cimentación",
            "cimiento",
            "muro",
            "carga",
            "momento",
            "cortante",
            "deflexión",
            "sismo",
            "pórtico",
            "placa",
        ],
    ),
    (
        "suelos",
        OBRA,
        &[
            "suelo",
            "excavación",
            "relleno",
            "compactación",
            "talud",
            "terreno",
            "nivel freático",
            "estudio de suelos",
            "geotecnia",
        ],
    ),
    (
        "soldadura",
        OBRA,
        &[
            "soldadura",
            "soldar",
            "electrodo",
            "perno",
            "conexión metálica",
        ],
    ),
    (
        "instalaciones",
        OBRA,
        &[
            "tubería",
            "eléctrica",
            "sanitaria",
            "desagüe",
            "agua",
            "cableado",
            "tablero",
            "instalación",
        ],
    ),
    (
        "seguridad",
        OBRA,
        &[
            "seguridad",
            "epp",
            "casco",
            "arnés",
            "andamio",
            "caída",
            "accidente",
            "riesgo",
            "señalización",
            "lesión",
        ],
    ),
    (
        "planos",
        OBRA,
        &[
            "plano",
            "escala",
            "cota",
            "detalle",
            "corte",
            "elevación",
            "dwg",
            "autocad",
            "revit",
            "lámina",
            "impresión",
            "imprimir",
        ],
    ),
    (
        "metrados",
        OBRA,
        &[
            "metrado",
            "cómputo",
            "cantidad",
            "medición",
            "volumen",
            "área",
        ],
    ),
    (
        "presupuesto",
        OBRA,
        &[
            "presupuesto",
            "costo",
            "precio",
            "cotización",
            "valorización",
            "partida",
            "adicional",
        ],
    ),
    (
        "materiales",
        OBRA,
        &[
            "material",
            "ladrillo",
            "arena",
            "piedra",
            "agregado",
            "pintura",
            "madera",
            "proveedor",
            "pedido",
            "almacén",
        ],
    ),
    (
        "topografía",
        OBRA,
        &[
            "topografía",
            "nivel",
            "replanteo",
            "estación total",
            "trazo",
            "eje",
        ],
    ),
    (
        "supervisión",
        OBRA,
        &[
            "supervisión",
            "supervisor",
            "inspección",
            "residente",
            "maestro de obra",
            "cuaderno de obra",
            "contratista",
            "subcontrata",
            "obrero",
        ],
    ),
    (
        "calidad",
        OBRA,
        &[
            "calidad",
            "ensayo",
            "prueba",
            "probeta",
            "control",
            "norma",
            "especificación",
            "tolerancia",
        ],
    ),
    (
        "cálculo",
        ESTUDIO,
        &[
            "cálculo",
            "fórmula",
            "unidades",
            "ecuación",
            "integral",
            "derivada",
            "resultado",
            "decimal",
            "redondeo",
            "conversión",
        ],
    ),
    (
        "software",
        ESTUDIO,
        &[
            "excel",
            "programa",
            "software",
            "sap2000",
            "etabs",
            "matlab",
            "python",
            "archivo",
            "guardar",
            "respaldo",
            "copia",
            "computadora",
            "laptop",
        ],
    ),
    (
        "exámenes",
        ESTUDIO,
        &[
            "examen",
            "parcial",
            "final",
            "práctica",
            "prueba",
            "nota",
            "calificación",
            "estudiar",
            "repasar",
        ],
    ),
    (
        "tesis",
        ESTUDIO,
        &[
            "tesis",
            "asesor",
            "capítulo",
            "bibliografía",
            "cita",
            "referencia",
            "sustentación",
            "investigación",
        ],
    ),
    (
        "entregas",
        ESTUDIO,
        &[
            "entrega",
            "trabajo",
            "informe",
            "tarea",
            "plazo",
            "fecha límite",
            "profesor",
            "curso",
        ],
    ),
    (
        "reuniones",
        TRABAJO,
        &[
            "reunión",
            "junta",
            "acta",
            "acuerdo",
            "llamada",
            "videollamada",
        ],
    ),
    (
        "comunicación",
        TRABAJO,
        &[
            "correo",
            "mensaje",
            "whatsapp",
            "avisar",
            "aviso",
            "malentendido",
            "explicar",
            "preguntar",
            "confirmar",
            "por escrito",
        ],
    ),
    (
        "cliente",
        TRABAJO,
        &[
            "cliente",
            "propietario",
            "jefe",
            "gerente",
            "compañero",
            "equipo",
            "contrato",
            "pago",
            "factura",
        ],
    ),
    (
        "tiempos",
        TRABAJO,
        &[
            "tiempo",
            "retraso",
            "demora",
            "cronograma",
            "plazo",
            "puntual",
            "tarde",
            "atraso",
            "programación",
        ],
    ),
    (
        "documentos",
        TRABAJO,
        &[
            "documento",
            "permiso",
            "licencia",
            "firma",
            "trámite",
            "expediente",
            "pdf",
            "versión",
        ],
    ),
    (
        "dinero",
        VIDA,
        &[
            "dinero",
            "gasto",
            "ahorro",
            "deuda",
            "banco",
            "compra",
            "pagar",
            "préstamo",
        ],
    ),
    (
        "salud",
        VIDA,
        &[
            "salud",
            "dormir",
            "sueño",
            "comida",
            "ejercicio",
            "médico",
            "cansancio",
            "estrés",
        ],
    ),
    (
        "relaciones",
        VIDA,
        &[
            "familia",
            "amigo",
            "pareja",
            "padres",
            "promesa",
            "discusión",
            "confianza",
        ],
    ),
    (
        "viajes",
        VIDA,
        &["viaje", "transporte", "bus", "vuelo", "maleta", "tráfico"],
    ),
];

/// Los conceptos, con sus raices ya cortadas (se cortan una vez).
pub fn conceptos() -> &'static [Concepto] {
    static C: OnceLock<Vec<Concepto>> = OnceLock::new();
    C.get_or_init(|| {
        DICCIONARIO
            .iter()
            .map(|(etiqueta, area, palabras)| {
                let mut raices: Vec<String> = Vec::new();
                for p in *palabras {
                    for r in texto::raices(p) {
                        if !raices.contains(&r) {
                            raices.push(r);
                        }
                    }
                }
                Concepto {
                    etiqueta,
                    area,
                    raices,
                }
            })
            .collect()
    })
}

/// Palabras que delatan el tipo.
const DE_ERROR: &[&str] = &[
    "error",
    "equivoqué",
    "equivocación",
    "olvidé",
    "olvido",
    "falló",
    "falla",
    "mal",
    "problema",
    "no revisé",
    "perdí",
    "rompí",
    "se cayó",
    "rechazaron",
    "multa",
    "reclamo",
    "tuve que rehacer",
    "rehacer",
];
const DE_ACIERTO: &[&str] = &[
    "funcionó",
    "salió bien",
    "acierto",
    "sirvió",
    "ahorré",
    "logré",
    "bien hecho",
    "buena idea",
    "resultó",
];

const DE_CAUSA: &[(&str, &[&str])] = &[
    (
        "Prisa",
        &[
            "prisa",
            "apuré",
            "apurado",
            "rápido",
            "corriendo",
            "última hora",
        ],
    ),
    (
        "No revisé",
        &[
            "no revisé",
            "sin revisar",
            "no verifiqué",
            "no comprobé",
            "no chequeé",
            "no leí",
        ],
    ),
    (
        "Comunicación",
        &[
            "no avisé",
            "no me avisaron",
            "no me dijeron",
            "malentendido",
            "no pregunté",
            "no entendí",
            "no quedó claro",
            "por escrito",
        ],
    ),
    (
        "No sabía",
        &[
            "no sabía",
            "desconocía",
            "no conocía",
            "primera vez",
            "nunca había",
        ],
    ),
    (
        "Supuse algo",
        &[
            "supuse",
            "asumí",
            "creí que",
            "pensé que",
            "di por hecho",
            "daba por hecho",
        ],
    ),
    (
        "Herramienta",
        &[
            "programa",
            "software",
            "se colgó",
            "se trabó",
            "excel",
            "autocad",
            "app",
            "batería",
            "impresora",
        ],
    ),
    (
        "Planificación",
        &[
            "no planifiqué",
            "sin planificar",
            "no calculé el tiempo",
            "a última hora",
            "dejé para",
            "cronograma",
        ],
    ),
    (
        "Cansancio",
        &["cansado", "cansancio", "sueño", "desvelado", "agotado"],
    ),
    (
        "Distracción",
        &[
            "distraído",
            "distracción",
            "celular",
            "me olvidé",
            "no presté atención",
        ],
    ),
];

fn de_palabra_normal(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == 'ñ'
}

/// Si el texto ya normalizado dice la frase como palabras enteras: «mal» no
/// salta con «normal» (`(?<![a-z0-9ñ])frase(?![a-z0-9ñ])`).
fn dice_frase(normal: &str, frase: &str) -> bool {
    let aguja = texto::normal(frase);
    if aguja.is_empty() {
        return false;
    }
    let mut desde = 0;
    while let Some(i) = normal[desde..].find(&aguja) {
        let inicio = desde + i;
        let fin = inicio + aguja.len();
        let antes = normal[..inicio].chars().next_back();
        let despues = normal[fin..].chars().next();
        if !antes.is_some_and(de_palabra_normal) && !despues.is_some_and(de_palabra_normal) {
            return true;
        }
        desde = inicio + normal[inicio..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

fn dice(normal: &str, frases: &[&str]) -> bool {
    frases.iter().any(|f| dice_frase(normal, f))
}

/// `[\p{L}0-9_]`
fn de_etiqueta(c: char) -> bool {
    c.is_alphabetic() || c.is_ascii_digit() || c == '_'
}

/// Un trozo de texto que es una etiqueta escrita: donde empieza y acaba el
/// trozo entero (para quitarlo) y la etiqueta sin el signo.
struct Hallada {
    inicio: usize,
    fin: usize,
    etiqueta: String,
}

/// Lo que pide una etiqueta: de 2 a 30 de sus signos, empezando en `desde`.
fn tramo_de_etiqueta(s: &str, desde: usize) -> Option<usize> {
    let mut fin = desde;
    let mut n = 0;
    for (i, c) in s[desde..].char_indices() {
        if n == 30 || !de_etiqueta(c) {
            break;
        }
        n += 1;
        fin = desde + i + c.len_utf8();
    }
    (n >= 2).then_some(fin)
}

/// `#([\p{L}0-9_]{2,30})`
fn almohadillas(s: &str) -> Vec<Hallada> {
    let mut v = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i..].chars().next().unwrap_or(' ');
        if c == '#'
            && let Some(fin) = tramo_de_etiqueta(s, i + 1)
        {
            v.push(Hallada {
                inicio: i,
                fin,
                etiqueta: s[i + 1..fin].to_string(),
            });
            i = fin;
            continue;
        }
        i += c.len_utf8();
    }
    v
}

/// `(?i)\b(?:hashtag|etiqueta)\s+([\p{L}0-9_]{2,30})`
fn dichas(s: &str) -> Vec<Hallada> {
    let mut v = Vec::new();
    let bajo = |c: char| c.to_lowercase().next().unwrap_or(c);
    let mut i = 0;
    'fuera: while i < s.len() {
        let c = s[i..].chars().next().unwrap_or(' ');
        let antes = s[..i].chars().next_back();
        if !antes.is_some_and(|a| a.is_alphanumeric() || a == '_') {
            for palabra in ["hashtag", "etiqueta"] {
                let mut j = i;
                let mut ok = true;
                for p in palabra.chars() {
                    match s[j..].chars().next() {
                        Some(x) if bajo(x) == p => j += x.len_utf8(),
                        _ => {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    continue;
                }
                let mut k = j;
                while let Some(x) = s[k..].chars().next().filter(|x| x.is_whitespace()) {
                    k += x.len_utf8();
                }
                if k == j {
                    continue;
                }
                if let Some(fin) = tramo_de_etiqueta(s, k) {
                    v.push(Hallada {
                        inicio: i,
                        fin,
                        etiqueta: s[k..fin].to_string(),
                    });
                    i = fin;
                    continue 'fuera;
                }
            }
        }
        i += c.len_utf8();
    }
    v
}

/// Las etiquetas escritas a mano en el texto (#obra, «etiqueta obra»), sin
/// el signo y en minusculas.
pub fn escritas(texto: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for h in almohadillas(texto).into_iter().chain(dichas(texto)) {
        let e = h.etiqueta.to_lowercase();
        if !v.contains(&e) {
            v.push(e);
        }
    }
    v
}

fn quitar(s: &str, halladas: &[Hallada]) -> String {
    let mut salida = String::with_capacity(s.len());
    let mut desde = 0;
    for h in halladas {
        salida.push_str(&s[desde..h.inicio]);
        desde = h.fin;
    }
    salida.push_str(&s[desde..]);
    salida
}

/// El texto sin las etiquetas escritas, para guardarlo limpio.
pub fn sin_etiquetas(texto: &str) -> String {
    let uno = quitar(texto, &almohadillas(texto));
    let dos = quitar(&uno, &dichas(&uno));
    // `\s{2,}` -> " "
    let mut salida = String::with_capacity(dos.len());
    let mut blancos = String::new();
    for c in dos.chars() {
        if c.is_whitespace() {
            blancos.push(c);
            continue;
        }
        if blancos.chars().count() >= 2 {
            salida.push(' ');
        } else {
            salida.push_str(&blancos);
        }
        blancos.clear();
        salida.push(c);
    }
    if blancos.chars().count() >= 2 {
        salida.push(' ');
    } else {
        salida.push_str(&blancos);
    }
    salida.trim().to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Propuesta {
    pub etiquetas: Vec<String>,
    pub area: Option<String>,
    pub tipo: Option<String>,
    pub causas: Vec<String>,
}

/// **Lo que uno suele etiquetar.** Para cada etiqueta puesta a mano, que
/// raices la acompanan; una leccion nueva que comparte dos o mas raices con
/// las de una etiqueta la propone.
#[derive(Debug, Clone, Default)]
pub struct Aprendido {
    raices_por_etiqueta: BTreeMap<String, BTreeMap<String, i64>>,
}

impl Aprendido {
    pub fn proponer(&self, raices: &[String]) -> Vec<String> {
        let mut v: Vec<(String, usize)> = self
            .raices_por_etiqueta
            .iter()
            .filter_map(|(etiqueta, cuenta)| {
                let comunes = raices
                    .iter()
                    .filter(|r| cuenta.get(*r).copied().unwrap_or(0) > 0)
                    .count();
                (comunes >= 2).then(|| (etiqueta.clone(), comunes))
            })
            .collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.1));
        v.into_iter().map(|(e, _)| e).take(3).collect()
    }
}

pub fn aprender(lecciones: &[Leccion]) -> Aprendido {
    let mut mapa: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for l in lecciones {
        if l.etiquetas.is_empty() {
            continue;
        }
        let mut raices = texto::raices(&format!(
            "{} {} {} {}",
            l.titulo, l.que_paso, l.por_que, l.proxima
        ));
        raices.sort();
        raices.dedup();
        for e in &l.etiquetas {
            let m = mapa.entry(e.clone()).or_default();
            for r in &raices {
                *m.entry(r.clone()).or_insert(0) += 1;
            }
        }
    }
    Aprendido {
        raices_por_etiqueta: mapa,
    }
}

/// Lo que se propone para `texto`. `aprendido` sale de [`aprender`] con las
/// lecciones que ya hay; `quitadas`, lo que uno ya rechazo.
pub fn proponer(texto_entero: &str, aprendido: &Aprendido, quitadas: &[String]) -> Propuesta {
    let normal = texto::normal(texto_entero);
    let mut raices = texto::raices(texto_entero);
    {
        let mut vistas = std::collections::HashSet::new();
        raices.retain(|r| vistas.insert(r.clone()));
    }
    let mut por_concepto: Vec<(&Concepto, usize)> = conceptos()
        .iter()
        .map(|c| (c, raices.iter().filter(|r| c.raices.contains(r)).count()))
        .filter(|(_, n)| *n > 0)
        .collect();
    por_concepto.sort_by_key(|x| std::cmp::Reverse(x.1));
    let del_usuario = aprendido.proponer(&raices);
    let fuera: Vec<String> = quitadas.iter().map(|q| q.to_lowercase()).collect();
    let mut etiquetas: Vec<String> = Vec::new();
    for e in escritas(texto_entero)
        .into_iter()
        .chain(del_usuario)
        .chain(por_concepto.iter().map(|(c, _)| c.etiqueta.to_string()))
    {
        if !etiquetas.contains(&e) && !fuera.contains(&e) {
            etiquetas.push(e);
        }
    }
    etiquetas.truncate(6);
    // El area con mas palabras; a igualdad, la que salio antes.
    let mut areas: Vec<(&str, usize)> = Vec::new();
    for (c, n) in &por_concepto {
        match areas.iter_mut().find(|(a, _)| *a == c.area) {
            Some((_, s)) => *s += n,
            None => areas.push((c.area, *n)),
        }
    }
    let mut area: Option<(&str, usize)> = None;
    for (a, n) in areas {
        if area.is_none_or(|(_, m)| n > m) {
            area = Some((a, n));
        }
    }
    let tipo = if dice(&normal, DE_ACIERTO) {
        Some(TIPO_ACIERTO.to_string())
    } else if dice(&normal, DE_ERROR) {
        Some(TIPO_ERROR.to_string())
    } else {
        None
    };
    let causas = DE_CAUSA
        .iter()
        .filter(|(_, frases)| dice(&normal, frases))
        .map(|(c, _)| c.to_string())
        .collect();
    Propuesta {
        etiquetas,
        area: area.map(|(a, _)| a.to_string()),
        tipo,
        causas,
    }
}

/// Los conceptos a los que apunta una palabra buscada: «obra» → los de
/// Construccion.
pub fn conceptos_de(raiz: &str) -> Vec<&'static Concepto> {
    conceptos()
        .iter()
        .filter(|c| c.raices.iter().any(|r| r == raiz))
        .collect()
}

/// Palabras que delatan una leccion grave o importante. **Lo anade el PC**
/// (v2, 4-oct-2026): el movil no propone la gravedad y la deja en «Leve»;
/// la barra rapida de la lista la rellena sola y se cambia con un clic.
const DE_GRAVE: &[&str] = &[
    "grave",
    "peligro",
    "peligroso",
    "accidente",
    "herido",
    "herida",
    "lesión",
    "me lastimé",
    "incendio",
    "inundación",
    "perdí todo",
    "lo perdí todo",
    "se perdió todo",
    "despido",
    "despidieron",
    "demanda",
    "denuncia",
    "multa",
    "hospital",
    "urgencias",
    "electrocutado",
    "electrocutó",
    "se derrumbó",
    "colapsó",
];
const DE_IMPORTANTE: &[&str] = &[
    "importante",
    "rehacer",
    "tuve que rehacer",
    "repintar",
    "retraso",
    "se retrasó",
    "atraso",
    "costó",
    "caro",
    "dinero",
    "rechazaron",
    "reclamo",
    "resbala",
    "resbaló",
    "resbaladizo",
    "se cayó",
    "caída",
    "otra vez",
    "de nuevo",
    "volvió a pasar",
    "perdí",
    "se rompió",
    "rompí",
    "nota baja",
    "desaprobé",
];

/// **La gravedad que se propone** para un texto: 3 si habla de algo grave
/// (un accidente, perderlo todo, una multa), 2 si de algo que costo
/// (rehacer, un retraso, un resbalon) o es un error, 1 si no.
pub fn proponer_gravedad(texto_entero: &str, tipo: Option<&str>) -> i64 {
    let normal = texto::normal(texto_entero);
    if dice(&normal, DE_GRAVE) {
        3
    } else if dice(&normal, DE_IMPORTANTE) || tipo == Some(TIPO_ERROR) {
        2
    } else {
        1
    }
}
