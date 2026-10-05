//! **Una frase dicha de corrido, repartida en sus campos** (`Dictado.kt`).
//!
//! Quien dicta no rellena un formulario: dice «paso que se vacio la losa sin
//! revisar el encofrado porque el maestro tenia prisa; la proxima vez reviso
//! los puntales antes del vaciado». Aqui se corta por las palabras que todo
//! el mundo usa al contarlo y cada trozo va a su sitio. Sin ninguna de ellas,
//! todo es lo aprendido.

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Campos {
    pub titulo: String,
    pub que_paso: String,
    pub por_que: String,
    pub proxima: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Campo {
    Titulo,
    Paso,
    PorQue,
    Proxima,
}

const MARCAS: &[(&str, Campo)] = &[
    ("aprendí que", Campo::Titulo),
    ("aprendi que", Campo::Titulo),
    ("lección:", Campo::Titulo),
    ("leccion:", Campo::Titulo),
    ("lo que pasó fue que", Campo::Paso),
    ("lo que pasó es que", Campo::Paso),
    ("pasó que", Campo::Paso),
    ("paso que", Campo::Paso),
    ("resulta que", Campo::Paso),
    ("qué pasó:", Campo::Paso),
    ("que paso:", Campo::Paso),
    ("porque", Campo::PorQue),
    ("debido a que", Campo::PorQue),
    ("debido a", Campo::PorQue),
    ("la causa fue", Campo::PorQue),
    ("por qué:", Campo::PorQue),
    ("ya que", Campo::PorQue),
    ("la próxima vez", Campo::Proxima),
    ("la proxima vez", Campo::Proxima),
    ("de ahora en adelante", Campo::Proxima),
    ("a partir de ahora", Campo::Proxima),
    ("en adelante", Campo::Proxima),
    ("para la próxima", Campo::Proxima),
    ("para la proxima", Campo::Proxima),
    ("haré distinto:", Campo::Proxima),
];

fn bajo(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Donde sale `frase` en `texto` sin distinguir mayusculas y como palabras
/// enteras (`(?i)(?<![\p{L}])frase(?![\p{L}])`), sin solaparse: el
/// `findAll` de Kotlin.
fn buscar(texto: &str, frase: &str) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    let mut i = 0;
    while i < texto.len() {
        let c0 = texto[i..].chars().next().unwrap_or(' ');
        let mut j = i;
        let mut ok = true;
        for p in frase.chars() {
            match texto[j..].chars().next() {
                Some(x) if bajo(x) == p => j += x.len_utf8(),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if ok
            && !texto[..i]
                .chars()
                .next_back()
                .is_some_and(char::is_alphabetic)
            && !texto[j..].chars().next().is_some_and(char::is_alphabetic)
        {
            v.push((i, j));
            i = j.max(i + c0.len_utf8());
            continue;
        }
        i += c0.len_utf8();
    }
    v
}

pub fn repartir(dicho: &str) -> Campos {
    let texto = dicho.trim();
    let mut todas: Vec<(usize, usize, Campo)> = MARCAS
        .iter()
        .flat_map(|(f, c)| buscar(texto, f).into_iter().map(move |(a, b)| (a, b, *c)))
        .collect();
    // Estable: a igual inicio, en el orden de la lista.
    todas.sort_by_key(|t| t.0);
    let mut cortes: Vec<(usize, usize, Campo)> = Vec::new();
    let mut vistos: Vec<Campo> = Vec::new();
    for t in todas {
        if vistos.contains(&t.2) {
            continue;
        }
        // Solapada con otra («debido a que» y «debido a»).
        if cortes.iter().any(|c| t.0 < c.1) {
            continue;
        }
        vistos.push(t.2);
        cortes.push(t);
    }
    if cortes.is_empty() {
        return Campos {
            titulo: mayuscula(&limpio(texto)),
            ..Default::default()
        };
    }
    let mut trozos: std::collections::HashMap<Campo, String> = std::collections::HashMap::new();
    let delante = limpio(&texto[..cortes[0].0]);
    for (i, (_, fin, campo)) in cortes.iter().enumerate() {
        let hasta = cortes.get(i + 1).map_or(texto.len(), |c| c.0);
        trozos.insert(*campo, limpio(&texto[*fin..hasta]));
    }
    // Lo de delante es lo aprendido, si no se dijo aparte; si no, el suceso.
    let mut titulo = trozos.get(&Campo::Titulo).cloned().unwrap_or_default();
    let mut paso = trozos.get(&Campo::Paso).cloned().unwrap_or_default();
    if !delante.is_empty() {
        if titulo.is_empty() {
            titulo = delante;
        } else {
            paso = [delante, paso]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(". ");
        }
    }
    let proxima = trozos.get(&Campo::Proxima).cloned().unwrap_or_default();
    // Sin frase de lo aprendido, lo que se hara distinto lo resume mejor.
    if titulo.is_empty() {
        titulo = if proxima.is_empty() {
            paso.clone()
        } else {
            proxima.clone()
        };
    }
    Campos {
        titulo: mayuscula(&titulo),
        que_paso: mayuscula(&paso),
        por_que: mayuscula(&trozos.get(&Campo::PorQue).cloned().unwrap_or_default()),
        proxima: mayuscula(&proxima),
    }
}

fn limpio(s: &str) -> String {
    s.trim()
        .trim_matches(|c| matches!(c, ',' | ';' | ':' | '.' | '-' | '—'))
        .trim()
        .to_string()
}

fn mayuscula(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(p) => p.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}
