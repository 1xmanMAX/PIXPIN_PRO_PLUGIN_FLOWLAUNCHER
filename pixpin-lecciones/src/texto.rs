//! **Las palabras, tal como las compara el buscador** (`Texto.kt`): sin
//! mayusculas, sin acentos, sin plurales y con las terminaciones mas comunes
//! del espanol recortadas. Asi «Estructuras», «estructura» y «estructural»
//! caen juntas, y un dictado que escribe «hormigon» encuentra «hormigón».
//!
//! Los largos se cuentan en letras y no en bytes: la «ñ» ocupa dos bytes en
//! UTF-8 y una sola unidad en el `String` de Kotlin.

/// Palabras que no dicen nada: no cuentan ni para buscar ni para parecerse.
pub const VACIAS: &[&str] = &[
    "a", "al", "algo", "ante", "antes", "aqui", "asi", "bien", "cada", "como", "con", "cual",
    "cuando", "de", "del", "desde", "donde", "dos", "el", "ella", "en", "entre", "era", "es",
    "esa", "ese", "eso", "esta", "este", "esto", "fue", "ha", "hay", "he", "la", "las", "le",
    "les", "lo", "los", "mas", "me", "mi", "mis", "muy", "nada", "ni", "no", "nos", "o", "otra",
    "otro", "para", "pero", "poco", "por", "porque", "que", "se", "sea", "si", "sin", "sobre",
    "son", "su", "sus", "tambien", "te", "tener", "tengo", "todo", "tu", "un", "una", "uno",
    "unos", "y", "ya", "yo", "vez", "hacer", "hice", "debo", "debe", "siempre", "nunca", "luego",
];

/// La letra sin su acento, como `Normalizer.Form.NFD` mas quitar las marcas.
/// Solo las letras latinas: es lo que se escribe aqui, y una tabla corta
/// evita arrastrar las tablas de Unicode enteras.
fn sin_acento(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'é' | 'è' | 'ë' | 'ê' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'í' | 'ì' | 'ï' | 'î' | 'ĩ' | 'ī' | 'ĭ' | 'į' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ú' | 'ù' | 'ü' | 'û' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ý' | 'ÿ' => 'y',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ź' | 'ż' | 'ž' => 'z',
        'ń' | 'ņ' | 'ň' => 'n',
        'ŕ' | 'ř' => 'r',
        'ĺ' | 'ľ' | 'ļ' => 'l',
        'ť' | 'ţ' => 't',
        'ď' => 'd',
        'ğ' | 'ĝ' | 'ġ' | 'ģ' => 'g',
        // Las marcas sueltas (lo que deja un teclado que compone aparte).
        '\u{0300}'..='\u{036f}' => '\0',
        otra => otra,
    }
}

/// `Texto.normal`: en minusculas y sin acentos, pero con la «ñ».
pub fn normal(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c == 'ñ' { c } else { sin_acento(c) })
        .filter(|c| *c != '\0')
        .collect()
}

fn es_de_palabra(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == 'ñ' || c == '#'
}

/// Las palabras de `s`, normalizadas, sin las vacias.
pub fn palabras(s: &str) -> Vec<String> {
    normal(s)
        .split(|c: char| !es_de_palabra(c))
        .filter(|p| p.chars().count() > 1 && !VACIAS.contains(p))
        .map(str::to_string)
        .collect()
}

const TERMINACIONES: &[&str] = &[
    "aciones", "iciones", "amientos", "imientos", "amiento", "imiento", "mente", "ciones", "acion",
    "icion", "cion", "sion", "ando", "iendo", "adas", "idas", "ados", "idos", "ada", "ida", "ado",
    "ido", "ales", "eles", "ar", "er", "ir", "es", "as", "os", "al", "a", "o", "e", "s",
];

/// La raiz de una palabra, a lo bruto: lo justo para que el singular y el
/// plural, el verbo y su participio, se encuentren. No es un lematizador; no
/// hace falta.
pub fn raiz(p: &str) -> String {
    let largo = p.chars().count();
    if largo <= 4 {
        return p.trim_end_matches('s').to_string();
    }
    for fin in TERMINACIONES {
        if largo >= fin.len() + 4 && p.ends_with(fin) {
            return p[..p.len() - fin.len()].to_string();
        }
    }
    p.to_string()
}

pub fn raices(s: &str) -> Vec<String> {
    palabras(s).iter().map(|p| raiz(p)).collect()
}

/// Distancia de edicion, cortando en cuanto pasa de `tope`: con dictado hay
/// erratas, y una letra de mas o de menos no puede dejar sin resultados.
pub fn distancia(a: &str, b: &str, tope: usize) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > tope {
        return tope + 1;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        let mut min_fila = cur[0];
        for j in 1..=b.len() {
            let c = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + c);
            min_fila = min_fila.min(cur[j]);
        }
        if min_fila > tope {
            return tope + 1;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Cuantas erratas se perdonan segun lo larga que es la palabra buscada.
pub fn erratas(p: &str) -> usize {
    match p.chars().count() {
        n if n >= 8 => 2,
        n if n >= 4 => 1,
        _ => 0,
    }
}
