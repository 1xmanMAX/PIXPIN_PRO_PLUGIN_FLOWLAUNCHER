//! **Lo que se reconoce al escribir una tarea** (17 y 18-oct-2026).
//!
//! El usuario: «que reconozca la prioridad y el recordatorio; ponerle tags
//! con el hashtag» y despues «que sea mas inteligente»: un `#palabra` en
//! CUALQUIER sitio es una etiqueta (sale del titulo y se ensena aparte), la
//! urgencia tiene tres niveles (normal, urgente y muy urgente, con palabras
//! que se cambian en Ajustes → Tareas) y la fecha se escribe como se dice
//! («18/10 17:00», «el 20 a las 9», «viernes 15:30», [`crate::fechas`]).
//!
//! **No hay campos nuevos**: todo va DENTRO del texto de la tarea, que es lo
//! unico que viaja al movil (`- [ ] texto`, el Markdown de Android y de
//! Obsidian). Asi el movil, que no sabe nada de esto, lo ensena tal cual:
//!
//! ```text
//! - [ ] llamar al banco ⏫ #casa 💬 no abren hasta el lunes ⏰ 2026-10-17 17:00 ➕ 2026-10-17
//! ```
//!
//! - La **urgencia** usa los emoticonos de prioridad de Obsidian Tasks:
//!   `⏫` muy urgente y `🔼` urgente; normal no lleva nada. El Kotlin de
//!   Android no tiene prioridades en las tareas (`mini/Tareas.kt`, mirado el
//!   18-oct): las ve como dos emoticonos del texto. Lo viejo se sigue
//!   leyendo: el `🔽` de «baja» es normal y se quita.
//! - Las **etiquetas** son `#palabra`. En el fichero van detras del titulo
//!   (las ve el movil y las encuentra cualquier buscador); en el PC se
//!   ensenan como chapitas y el titulo queda limpio.
//! - La **nota** de por que no se esta haciendo va tras `💬`, y la hora
//!   (`⏰`) al final, donde la buscan el vigia y el movil.
//!
//! Sin reloj propio: quien llama da el «ahora» local y las palabras de cada
//! urgencia ([`Palabras`]).

use crate::fechas::{self, Momento, normalizar};

/// Lo que marca la nota de una tarea.
pub const NOTA: char = '💬';
/// Lo que marca la hora de recordar (`tareas::RELOJ` de la app).
pub const RELOJ: char = '⏰';

/// Las palabras de fabrica de «urgente» (Ajustes → Tareas las cambia).
pub const URGENTE: &[&str] = &["urgente", "prioridad alta", "importante", "!"];
/// Las palabras de fabrica de «muy urgente».
pub const MUY_URGENTE: &[&str] = &[
    "muy urgente",
    "urgentísimo",
    "prioridad máxima",
    "!!",
    "!!!",
];

/// Cuanta prisa corre una tarea. Sin decir nada, normal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub enum Urgencia {
    #[default]
    Normal,
    Urgente,
    MuyUrgente,
}

impl Urgencia {
    /// El emoticono que la guarda en el texto (los de Obsidian Tasks).
    pub fn marca(self) -> Option<&'static str> {
        match self {
            Urgencia::MuyUrgente => Some("⏫"),
            Urgencia::Urgente => Some("🔼"),
            Urgencia::Normal => None,
        }
    }

    /// La de un emoticono guardado. `🔽` (la «baja» de antes) es normal.
    pub fn de_marca(c: char) -> Option<Urgencia> {
        match c {
            '⏫' => Some(Urgencia::MuyUrgente),
            '🔼' => Some(Urgencia::Urgente),
            '🔽' => Some(Urgencia::Normal),
            _ => None,
        }
    }

    /// Como se dice (en español; la app usa su catalogo de textos).
    pub fn nombre(self) -> &'static str {
        match self {
            Urgencia::MuyUrgente => "Muy urgente",
            Urgencia::Urgente => "Urgente",
            Urgencia::Normal => "Normal",
        }
    }
}

/// Las palabras que hacen urgente o muy urgente una tarea (Ajustes →
/// Tareas, `[tareas]` del TOML). Cada una puede tener varias palabras
/// («prioridad alta»); se comparan sin mayusculas ni tildes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palabras {
    pub urgente: Vec<String>,
    pub muy_urgente: Vec<String>,
}

impl Default for Palabras {
    fn default() -> Self {
        Palabras::preestablecidas()
    }
}

impl Palabras {
    /// Las de fabrica.
    pub fn preestablecidas() -> Palabras {
        Palabras {
            urgente: URGENTE.iter().map(|s| s.to_string()).collect(),
            muy_urgente: MUY_URGENTE.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Las de la tabla `[tareas]` de un `pindesk.toml` (lo que lee el
    /// plugin de Flow, que no carga los ajustes de la app). Lo que no este
    /// escrito, de fabrica.
    pub fn de_toml(texto: &str) -> Palabras {
        let mut p = Palabras::preestablecidas();
        let Some(tabla) = tabla_de(texto, "tareas") else {
            return p;
        };
        if let Some(v) = lista_de(tabla, "urgente") {
            p.urgente = v;
        }
        if let Some(v) = lista_de(tabla, "muy_urgente") {
            p.muy_urgente = v;
        }
        p
    }

    /// Cada frase partida en palabras llanas, con su nivel; las vacias fuera.
    fn partidas(&self) -> Vec<(Vec<String>, Urgencia)> {
        let mut v = Vec::new();
        for (lista, nivel) in [
            (&self.muy_urgente, Urgencia::MuyUrgente),
            (&self.urgente, Urgencia::Urgente),
        ] {
            for f in lista {
                let w: Vec<String> = f
                    .split_whitespace()
                    .map(llana)
                    .filter(|w| !w.is_empty())
                    .collect();
                if !w.is_empty() {
                    v.push((w, nivel));
                }
            }
        }
        v
    }
}

/// El texto de la tabla `[nombre]` de un TOML (hasta la siguiente cabecera).
fn tabla_de<'a>(texto: &'a str, nombre: &str) -> Option<&'a str> {
    let cabecera = format!("[{nombre}]");
    let mut inicio = None;
    let mut pos = 0;
    for linea in texto.split_inclusive('\n') {
        let l = linea.trim();
        if let Some(i) = inicio {
            if l.starts_with('[') && !l.starts_with("[[") && l.ends_with(']') && !l.contains('=') {
                return Some(&texto[i..pos]);
            }
        } else if l == cabecera {
            inicio = Some(pos + linea.len());
        }
        pos += linea.len();
    }
    inicio.map(|i| &texto[i..])
}

/// Una lista de textos `clave = ["a", 'b', …]` (en uno o varios renglones).
fn lista_de(tabla: &str, clave: &str) -> Option<Vec<String>> {
    let mut resto = tabla;
    loop {
        let i = resto.find(clave)?;
        let antes_ok = resto[..i]
            .chars()
            .next_back()
            .is_none_or(|c| c == '\n' || c.is_whitespace());
        let tras = resto[i + clave.len()..].trim_start_matches([' ', '\t']);
        if antes_ok && let Some(valor) = tras.strip_prefix('=') {
            let valor = valor.trim_start();
            let cuerpo = valor.strip_prefix('[')?;
            let mut v = Vec::new();
            let mut cs = cuerpo.chars();
            while let Some(c) = cs.next() {
                match c {
                    ']' => return Some(v),
                    '"' | '\'' => {
                        let mut s = String::new();
                        while let Some(d) = cs.next() {
                            match d {
                                '\\' if c == '"' => {
                                    if let Some(e) = cs.next() {
                                        s.push(match e {
                                            'n' => '\n',
                                            't' => '\t',
                                            otro => otro,
                                        });
                                    }
                                }
                                d if d == c => break,
                                d => s.push(d),
                            }
                        }
                        v.push(s);
                    }
                    '#' => {
                        // Un comentario hasta el final del renglon.
                        for d in cs.by_ref() {
                            if d == '\n' {
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
            return None;
        }
        resto = &resto[i + clave.len()..];
    }
}

/// Una tarea escrita, ya entendida.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Frase {
    /// Lo que hay que hacer, sin lo reconocido.
    pub titulo: String,
    pub urgencia: Urgencia,
    /// Sin la almohadilla, en el orden escrito y sin repetir.
    pub etiquetas: Vec<String>,
    /// Cuando recordarla, en hora local.
    pub hora: Option<Momento>,
    /// La nota de por que no se esta haciendo (tras `💬`), tal cual.
    pub nota: Option<String>,
}

impl Frase {
    /// **El texto que se guarda**: el titulo, la urgencia, las etiquetas,
    /// la nota y la hora, en ese orden (el que leen el movil y el vigia).
    pub fn texto(&self) -> String {
        let mut s = self.titulo.trim().to_string();
        let mut pon = |t: &str| {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str(t);
        };
        if let Some(m) = self.urgencia.marca() {
            pon(m);
        }
        for e in &self.etiquetas {
            pon(&format!("#{e}"));
        }
        if let Some(n) = self.nota.as_deref().filter(|n| !n.trim().is_empty()) {
            pon(&format!("{NOTA} {}", n.trim()));
        }
        if let Some(h) = self.hora {
            pon(&h.marca());
        }
        s
    }

    /// **Lo que se entendio, en una linea** («Muy urgente · #casa · ⏰
    /// mañana 17:00»): el subtitulo de Flow antes de pulsar. Vacio si no se
    /// reconocio nada.
    pub fn resumen(&self, ahora: Momento) -> String {
        let mut partes: Vec<String> = Vec::new();
        if self.urgencia != Urgencia::Normal {
            partes.push(self.urgencia.nombre().to_string());
        }
        partes.extend(self.etiquetas.iter().map(|e| format!("#{e}")));
        if let Some(h) = self.hora {
            partes.push(format!("{RELOJ} {}", h.legible(ahora)));
        }
        partes.join(" · ")
    }
}

/// Una palabra para compararla: minusculas, sin tildes ni la puntuacion de
/// los bordes (los `!` se quedan: son palabras de urgencia).
fn llana(p: &str) -> String {
    normalizar(p.trim_matches(|c: char| !c.is_alphanumeric() && c != '!'))
}

/// Si la palabra llana `t` es `w` («urgente!» tambien es «urgente»).
fn igual(t: &str, w: &str) -> bool {
    t == w || (t.chars().any(char::is_alphanumeric) && t.trim_end_matches('!') == w)
}

/// La etiqueta de una palabra `#algo`, sin la almohadilla ni la puntuacion
/// de detras. `None` si no lo es (`#` suelto, `#1` sigue siendo un numero).
pub fn etiqueta_de(p: &str) -> Option<String> {
    let resto = p.strip_prefix('#')?;
    let e: String = resto
        .trim_end_matches(|c: char| !c.is_alphanumeric())
        .to_string();
    let valida = !e.is_empty()
        && e.chars().any(char::is_alphabetic)
        && e.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    valida.then_some(e)
}

/// La marca `⏰ AAAA-MM-DD HH:MM` de un texto: donde empieza y su momento.
pub fn marca_de_hora(texto: &str) -> Option<(usize, Momento)> {
    let i = texto.rfind(RELOJ)?;
    let resto = texto[i + RELOJ.len_utf8()..].trim_start().as_bytes();
    if resto.get(4) != Some(&b'-') || resto.get(7) != Some(&b'-') || resto.get(13) != Some(&b':') {
        return None;
    }
    Momento::de_marca(&texto[i..]).map(|m| (i, m))
}

/// Lo que dice que lo que sigue es cuando recordarla.
fn es_aviso(p: &str) -> bool {
    matches!(
        p,
        "recordar"
            | "recuerdame"
            | "recordarme"
            | "recordatorio"
            | "recuerda"
            | "avisar"
            | "avisame"
            | "avisarme"
            | "aviso"
            | "alarma"
    )
}

/// Por donde puede empezar una fecha escrita al principio de la tarea.
fn empieza_fecha(p: &str) -> bool {
    matches!(
        p,
        "hoy"
            | "manana"
            | "pasado"
            | "el"
            | "este"
            | "esta"
            | "proximo"
            | "proxima"
            | "dentro"
            | "en"
    ) || matches!(
        p,
        "lunes" | "martes" | "miercoles" | "jueves" | "viernes" | "sabado" | "domingo"
    ) || p.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// **Entiende una tarea escrita.** `ahora`: el momento local; `palabras`:
/// las de cada urgencia.
///
/// Lo que no se entiende se queda en el titulo, nunca se adivina: una fecha
/// que no existe o que ya paso («recordar el 31 de febrero») no pone hora y
/// se lee tal cual. Vale tambien para un texto ya guardado (lo que sale al
/// corregir una tarea): sus marcas `⏫ #casa 💬 … ⏰ …` se vuelven a leer y
/// el resultado es el mismo texto.
pub fn leer(texto: &str, ahora: Momento, palabras: &Palabras) -> Frase {
    // 0. La hora ya marcada y la nota, que van detras de todo.
    let mut hora_guardada = None;
    let mut cuerpo = texto.to_string();
    if let Some((i, m)) = marca_de_hora(&cuerpo) {
        hora_guardada = Some(m);
        cuerpo.truncate(i);
    }
    let mut nota = None;
    if let Some(i) = cuerpo.find(NOTA) {
        let n = cuerpo[i + NOTA.len_utf8()..]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !n.is_empty() {
            nota = Some(n);
        }
        cuerpo.truncate(i);
    }
    // 1. Las etiquetas y la urgencia, esten donde esten.
    let crudas: Vec<&str> = cuerpo.split_whitespace().collect();
    let (urgencia, etiquetas, quedan) = urgencia_y_etiquetas(&crudas, palabras);
    // 2. La hora: tras «recordar» / «avisame»…, o una fecha al final o al
    //    principio.
    let mut palabras_t: Vec<String> = quedan.iter().map(|&k| crudas[k].to_string()).collect();
    let mut hora = None;
    if let Some((r, m)) = hallar_hora(&palabras_t, ahora) {
        hora = m;
        palabras_t.drain(r);
    }
    let titulo = palabras_t
        .join(" ")
        .trim_end_matches([',', ';', ':', ' ', '-'])
        .trim()
        .to_string();
    Frase {
        titulo,
        urgencia,
        etiquetas,
        hora: hora.or(hora_guardada),
        nota,
    }
}

/// La urgencia, las etiquetas y que palabras quedan (sus posiciones en
/// `crudas`) cuando se quitan.
fn urgencia_y_etiquetas(
    crudas: &[&str],
    palabras: &Palabras,
) -> (Urgencia, Vec<String>, Vec<usize>) {
    let frases = palabras.partidas();
    let llanas: Vec<String> = crudas.iter().map(|p| llana(p)).collect();
    let mut urgencia = Urgencia::Normal;
    let mut etiquetas: Vec<String> = Vec::new();
    let mut quedan: Vec<usize> = Vec::with_capacity(crudas.len());
    let mut i = 0;
    while i < crudas.len() {
        let p = crudas[i];
        if let Some(e) = etiqueta_de(p) {
            if !etiquetas
                .iter()
                .any(|x| x.to_lowercase() == e.to_lowercase())
            {
                etiquetas.push(e);
            }
            i += 1;
            continue;
        }
        let mut cs = p.chars();
        if let (Some(c), None) = (cs.next(), cs.next())
            && let Some(u) = Urgencia::de_marca(c)
        {
            urgencia = urgencia.max(u);
            i += 1;
            continue;
        }
        // La frase de urgencia mas larga que empiece aqui (a igual largo,
        // la de «muy urgente»).
        let hallada = frases
            .iter()
            .filter(|(w, _)| {
                i + w.len() <= llanas.len() && w.iter().zip(&llanas[i..]).all(|(w, t)| igual(t, w))
            })
            .max_by_key(|(w, u)| (w.len(), *u));
        if let Some((w, u)) = hallada {
            urgencia = urgencia.max(*u);
            // «con prioridad alta»: el «con» tambien sobra.
            if quedan.last().is_some_and(|&q| llanas[q] == "con") {
                quedan.pop();
            }
            i += w.len();
            continue;
        }
        quedan.push(i);
        i += 1;
    }
    (urgencia, etiquetas, quedan)
}

/// **Donde dice la hora** una lista de palabras: las que hay que quitar y
/// el momento (`None` con un «recordar» suelto al final, que se quita sin
/// decir cuando). Tras «recordar» / «avisame»…, o una fecha al final o al
/// principio.
fn hallar_hora(p: &[String], ahora: Momento) -> Option<(std::ops::Range<usize>, Option<Momento>)> {
    if let Some(k) = p.iter().position(|x| es_aviso(&llana(x))) {
        // La cola mas larga tras la palabra que sea una fecha entera; lo que
        // sobre detras vuelve al titulo («recordar mañana comprar pan»). Una
        // fecha que no existe o ya paso para ahi: «el 31 de febrero» no es
        // «el 31» con «de febrero» de titulo.
        for j in (k + 2..=p.len()).rev() {
            let cola = p[k + 1..j].join(" ");
            match fechas::interpretar(&cola, ahora) {
                Ok(m) => return Some((k..j, Some(por_la_tarde(&cola, ahora).unwrap_or(m)))),
                Err(fechas::ErrorFecha::NoEntiendo(_)) => {}
                Err(_) => break,
            }
        }
        if k > 0 && k + 1 == p.len() {
            // «recordar» suelto al final: no dice cuando; se quita igual.
            return Some((k..k + 1, None));
        }
    }
    let frase = p.join(" ");
    if let Ok((cabeza, Some(m))) = fechas::separar(&frase, ahora)
        // Un numero suelto al final («leer el capitulo 5») no es una hora:
        // sin «recordar» delante, la hora tiene que decirse como hora.
        && frase[cabeza.len()..].trim().parse::<u64>().is_err()
    {
        let m = por_la_tarde(&frase[cabeza.len()..], ahora).unwrap_or(m);
        return Some((cabeza.split_whitespace().count()..p.len(), Some(m)));
    }
    if p.len() >= 2 && empieza_fecha(&llana(&p[0])) {
        // «18/10 17:00 llamar al banco», «mañana 7 gimnasio»: lo mas largo
        // del principio que sea una fecha entera, dejando algo de titulo. Un
        // numero solo («5 manzanas») no es una hora.
        for j in (1..p.len()).rev() {
            let cabeza = p[..j].join(" ");
            if j == 1 && cabeza.parse::<u64>().is_ok() {
                continue;
            }
            match fechas::interpretar(&cabeza, ahora) {
                Ok(m) => return Some((0..j, Some(por_la_tarde(&cabeza, ahora).unwrap_or(m)))),
                Err(fechas::ErrorFecha::NoEntiendo(_)) => {}
                Err(_) => break,
            }
        }
    }
    None
}

/// **Lo escrito sin la fecha dicha con palabras** («pan mañana a las 10
/// urgente» → «pan urgente»), con lo demas en su sitio y sin reconocer: lo
/// que hace el selector de dia y hora antes de poner la marca elegida.
pub fn sin_fecha_dicha(texto: &str, ahora: Momento, palabras: &Palabras) -> String {
    let crudas: Vec<&str> = texto.split_whitespace().collect();
    let (_, _, quedan) = urgencia_y_etiquetas(&crudas, palabras);
    let otras: Vec<String> = quedan.iter().map(|&k| crudas[k].to_string()).collect();
    let Some((r, Some(_))) = hallar_hora(&otras, ahora) else {
        return texto.trim().to_string();
    };
    let fuera: Vec<usize> = quedan[r].to_vec();
    crudas
        .iter()
        .enumerate()
        .filter(|(k, _)| !fuera.contains(k))
        .map(|(_, p)| *p)
        .collect::<Vec<_>>()
        .join(" ")
}

/// **«A las 5» es por la tarde.** Nadie apunta una tarea para las cinco de
/// la madrugada sin decirlo: una hora suelta de la 1 a las 7 tras «las», sin
/// «de la mañana», «am» ni minutos, se lee como de la tarde (las 17:00).
/// Devuelve `None` si la frase no es de esas o ya dice la franja.
fn por_la_tarde(frase: &str, ahora: Momento) -> Option<Momento> {
    let q = normalizar(frase);
    let palabras: Vec<&str> = q.split_whitespace().collect();
    let franja = [
        "de la manana",
        "por la manana",
        "madrugada",
        "tarde",
        "noche",
        "mediodia",
    ]
    .iter()
    .any(|f| q.contains(f))
        || palabras
            .iter()
            .any(|p| p.ends_with("am") || p.ends_with("pm"));
    if franja {
        return None;
    }
    let suelta = palabras.windows(2).any(|w| {
        matches!(w[0], "las" | "la") && w[1].parse::<u32>().is_ok_and(|n| (1..=7).contains(&n))
    });
    if !suelta {
        return None;
    }
    fechas::interpretar(&format!("{frase} de la tarde"), ahora).ok()
}

// ----------------------------------------------- leer lo guardado

/// Lo que lleva una tarea ya guardada, para ensenarla. `texto`: lo que se
/// ve (sin la fecha de creacion `➕` ni las imagenes, que quita la app).
pub fn marcas(texto: &str) -> Frase {
    let mut base = texto.to_string();
    let mut hora = None;
    if let Some((i, m)) = marca_de_hora(&base) {
        hora = Some(m);
        base.truncate(i);
    }
    let mut nota = None;
    if let Some(i) = base.find(NOTA) {
        let n = base[i + NOTA.len_utf8()..].trim().to_string();
        if !n.is_empty() {
            nota = Some(n);
        }
        base.truncate(i);
    }
    let mut urgencia = Urgencia::Normal;
    let mut etiquetas: Vec<String> = Vec::new();
    let mut quedan = Vec::new();
    for p in base.split_whitespace() {
        let mut cs = p.chars();
        if let (Some(c), None) = (cs.next(), cs.next())
            && let Some(u) = Urgencia::de_marca(c)
        {
            urgencia = urgencia.max(u);
            continue;
        }
        if let Some(e) = etiqueta_de(p) {
            if !etiquetas.contains(&e) {
                etiquetas.push(e);
            }
            continue;
        }
        quedan.push(p);
    }
    Frase {
        titulo: quedan.join(" "),
        urgencia,
        etiquetas,
        hora,
        nota,
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Miercoles 14 de octubre de 2026, 10:30 (el de las pruebas de fechas).
    fn ahora() -> Momento {
        Momento {
            dia: fechas::dias_de(2026, 10, 14),
            minuto: 10 * 60 + 30,
        }
    }

    fn a(mes: i64, dia: i64, h: u32, m: u32) -> Option<Momento> {
        Some(Momento {
            dia: fechas::dias_de(2026, mes, dia),
            minuto: h * 60 + m,
        })
    }

    fn leer_(t: &str) -> Frase {
        leer(t, ahora(), &Palabras::preestablecidas())
    }

    #[test]
    fn la_frase_del_usuario_se_entiende_entera() {
        let f = leer_("llamar al banco prioridad alta recordar a las 5 #casa");
        assert_eq!(f.titulo, "llamar al banco");
        assert_eq!(f.urgencia, Urgencia::Urgente);
        assert_eq!(f.etiquetas, ["casa"]);
        assert_eq!(f.hora, a(10, 14, 17, 0), "las 5 son las 17:00 de hoy");
        assert_eq!(f.texto(), "llamar al banco 🔼 #casa ⏰ 2026-10-14 17:00");
        assert_eq!(f.resumen(ahora()), "Urgente · #casa · ⏰ hoy 17:00");
    }

    #[test]
    fn sin_palabras_la_urgencia_es_normal_y_no_lleva_marca() {
        let f = leer_("regar las plantas");
        assert_eq!(f.urgencia, Urgencia::Normal);
        assert_eq!(f.texto(), "regar las plantas");
        assert_eq!(f.resumen(ahora()), "");
        assert_eq!(Urgencia::default(), Urgencia::Normal);
    }

    #[test]
    fn urgente_y_muy_urgente_con_las_palabras_de_fabrica() {
        let u = |t: &str| leer_(t).urgencia;
        use Urgencia::*;
        assert_eq!(u("pagar luz urgente"), Urgente);
        assert_eq!(u("pagar luz URGENTE!"), Urgente);
        assert_eq!(u("importante revisar planos"), Urgente);
        assert_eq!(u("revisar planos con prioridad alta"), Urgente);
        assert_eq!(u("revisar planos !"), Urgente);
        assert_eq!(u("pagar luz muy urgente"), MuyUrgente);
        assert_eq!(u("pagar luz urgentísimo"), MuyUrgente);
        assert_eq!(u("pagar luz prioridad maxima"), MuyUrgente);
        assert_eq!(u("pagar luz !!"), MuyUrgente);
        assert_eq!(
            u("urgente y muy urgente a la vez"),
            MuyUrgente,
            "gana la mayor"
        );
        assert_eq!(
            leer_("pagar luz muy urgente").titulo,
            "pagar luz",
            "«muy» no se queda"
        );
        assert_eq!(
            leer_("revisar planos con prioridad alta").titulo,
            "revisar planos"
        );
        // Lo guardado se vuelve a leer igual (corregir una tarea).
        assert_eq!(u("pagar luz ⏫"), MuyUrgente);
        assert_eq!(u("pagar luz 🔼"), Urgente);
        let viejo = leer_("pagar luz 🔽");
        assert_eq!(
            (viejo.urgencia, viejo.titulo.as_str()),
            (Normal, "pagar luz"),
            "la «baja» de antes"
        );
        // Negativos: ni «baja» ni palabras parecidas.
        for t in [
            "comprar pan prioridad baja",
            "ir a urgencias",
            "subir a la parte alta",
            "muy bien hecho",
            "¡hola",
        ] {
            assert_eq!(u(t), Normal, "{t}");
        }
        assert_eq!(leer_("ir a urgencias").titulo, "ir a urgencias");
    }

    #[test]
    fn las_palabras_son_las_de_ajustes() {
        let p = Palabras {
            urgente: vec!["pronto".into()],
            muy_urgente: vec!["ya mismo".into()],
        };
        let f = leer("llamar ya mismo", ahora(), &p);
        assert_eq!(
            (f.titulo.as_str(), f.urgencia),
            ("llamar", Urgencia::MuyUrgente)
        );
        assert_eq!(
            leer("llamar pronto", ahora(), &p).urgencia,
            Urgencia::Urgente
        );
        // Las de fabrica ya no valen si se quitaron.
        assert_eq!(
            leer("llamar urgente", ahora(), &p).urgencia,
            Urgencia::Normal
        );
        // Sin ninguna, todo normal.
        let nada = Palabras {
            urgente: vec![],
            muy_urgente: vec![" ".into()],
        };
        assert_eq!(
            leer("llamar urgente !!", ahora(), &nada).urgencia,
            Urgencia::Normal
        );
    }

    #[test]
    fn las_etiquetas_en_cualquier_sitio_salen_del_titulo() {
        let f = leer_("#obra pedir yeso #Obra para el #muro-norte, #1 hoy no");
        assert_eq!(
            f.titulo, "pedir yeso para el #1 hoy no",
            "#1 es un numero, no una etiqueta"
        );
        assert_eq!(f.etiquetas, ["obra", "muro-norte"]);
        let medio = leer_("llamar a #casa Ana");
        assert_eq!(
            (medio.titulo.as_str(), medio.etiquetas.as_slice()),
            ("llamar a Ana", &["casa".to_string()][..])
        );
        // En el fichero van detras del titulo: el movil las ve.
        assert_eq!(medio.texto(), "llamar a Ana #casa");
        // Negativos: «#» suelto o pegado a nada.
        assert!(leer_("precio # 5").etiquetas.is_empty());
        assert!(leer_("canal C#").etiquetas.is_empty());
    }

    #[test]
    fn la_hora_como_se_escribe() {
        let h = |t: &str| leer_(t).hora;
        assert_eq!(h("pagar alquiler 18/10 17:00"), a(10, 18, 17, 0));
        assert_eq!(h("pagar alquiler 18-10 5pm"), a(10, 18, 17, 0));
        assert_eq!(h("pagar alquiler 18 oct 17:00"), a(10, 18, 17, 0));
        assert_eq!(h("reunión el 20 a las 9"), a(10, 20, 9, 0));
        assert_eq!(h("cita 20 de octubre 8:30"), a(10, 20, 8, 30));
        assert_eq!(h("gimnasio mañana 7"), a(10, 15, 7, 0));
        assert_eq!(h("entregar informe viernes 15:30"), a(10, 16, 15, 30));
        assert_eq!(
            h("llamar a Ana recuérdame mañana a las 10"),
            a(10, 15, 10, 0)
        );
        assert_eq!(h("cita médico avisame a las 18:30"), a(10, 14, 18, 30));
        // Al principio tambien.
        let f = leer_("18/10 17:00 llamar al banco");
        assert_eq!(
            (f.titulo.as_str(), f.hora),
            ("llamar al banco", a(10, 18, 17, 0))
        );
        let f = leer_("mañana 7 gimnasio");
        assert_eq!((f.titulo.as_str(), f.hora), ("gimnasio", a(10, 15, 7, 0)));
        // Ya pasada este año: la del que viene.
        assert_eq!(
            h("renovar seguro 2/10"),
            Some(Momento {
                dia: fechas::dias_de(2027, 10, 2),
                minuto: 9 * 60
            })
        );
        // Lo que sigue a la fecha tras «recordar» vuelve al titulo.
        let f = leer_("recordar mañana comprar pan");
        assert_eq!(
            (f.titulo.as_str(), f.hora),
            ("comprar pan", a(10, 15, 9, 0))
        );
    }

    #[test]
    fn caso_negativo_lo_que_no_es_fecha_no_pone_hora() {
        for t in [
            "leer el capítulo 5",
            "recordar el 31 de febrero",
            "comprar 3 kilos",
            "recordar",
            "5 manzanas",
            "2 kilos de pan",
            "llamar al 911",
            "pagar 18/13",
            "hoy limpiar",
            "el 31 de febrero comprar",
        ] {
            assert_eq!(leer_(t).hora, None, "{t}");
        }
        assert_eq!(
            leer_("recordar el 31 de febrero").titulo,
            "recordar el 31 de febrero"
        );
        assert_eq!(leer_("5 manzanas").titulo, "5 manzanas");
        assert_eq!(leer_("  regar las plantas ").texto(), "regar las plantas");
    }

    #[test]
    fn lo_guardado_se_vuelve_a_leer_igual() {
        let crudo = "llamar al banco ⏫ #casa 💬 cierran a las 3 ⏰ 2026-10-14 17:00";
        let f = leer_(crudo);
        assert_eq!(f.texto(), crudo);
        assert_eq!(
            f.nota.as_deref(),
            Some("cierran a las 3"),
            "la nota no se lee como hora"
        );
        // Una hora ya pasada se respeta (es la que tenia).
        let viejo = "pan ⏰ 2020-01-01 09:00";
        assert_eq!(leer_(viejo).texto(), viejo);
        // Corregida con otra fecha escrita: manda la nueva.
        let f = leer_("pan mañana 7 ⏰ 2026-10-14 17:00");
        assert_eq!(f.texto(), "pan ⏰ 2026-10-15 07:00");
        // Y lo que se ve.
        let m = marcas(crudo);
        assert_eq!(m.titulo, "llamar al banco");
        assert_eq!(m.urgencia, Urgencia::MuyUrgente);
        assert_eq!(m.etiquetas, ["casa"]);
        assert_eq!(m.nota.as_deref(), Some("cierran a las 3"));
        assert_eq!(m.hora, a(10, 14, 17, 0));
    }

    #[test]
    fn sin_la_fecha_dicha_lo_demas_queda_en_su_sitio() {
        let p = Palabras::preestablecidas();
        let s = |t: &str| sin_fecha_dicha(t, ahora(), &p);
        assert_eq!(s("pan mañana a las 10 urgente #casa"), "pan urgente #casa");
        assert_eq!(s("18/10 17:00 llamar #casa"), "llamar #casa");
        assert_eq!(s("llamar recuérdame el viernes"), "llamar");
        // Sin fecha, tal cual.
        assert_eq!(
            s("leer el capítulo 5 urgente"),
            "leer el capítulo 5 urgente"
        );
    }

    #[test]
    fn las_palabras_se_leen_del_toml_de_ajustes() {
        let toml = "idioma = \"es\"\n\n[tareas]\n# las mias\nurgente = [\"pronto\", 'ya']\nmuy_urgente = [\n  \"ahora \\\"mismo\\\"\", # comentario\n  \"!!\",\n]\n\n[voz]\nurgente = [\"no\"]\n";
        let p = Palabras::de_toml(toml);
        assert_eq!(p.urgente, ["pronto", "ya"]);
        assert_eq!(p.muy_urgente, ["ahora \"mismo\"", "!!"]);
        // Sin tabla o sin clave: las de fabrica.
        assert_eq!(
            Palabras::de_toml("idioma = \"es\"\n"),
            Palabras::preestablecidas()
        );
        let solo = Palabras::de_toml("[tareas]\nurgente = []\n");
        assert!(solo.urgente.is_empty());
        assert_eq!(solo.muy_urgente, Palabras::preestablecidas().muy_urgente);
    }
}
