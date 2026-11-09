//! **Fechas dichas como las dice la gente** («mañana a las 10», «el
//! viernes», «20/10», «el 3 de noviembre»), para las tareas del asistente.
//!
//! El formato de las tareas no cambia: la fecha de una tarea es su marca de
//! recordatorio `⏰ AAAA-MM-DD HH:MM` en hora local, al final de lo que se
//! ve (`tareas::con_hora` de la app, `Tareas.marcaDeHora` del movil). Aqui
//! solo se pasa del español a ese momento, **sin reloj propio**: quien llama
//! da el «ahora» local, y asi las pruebas son deterministas.
//!
//! Estricto a proposito: [`interpretar`] tiene que entender la frase
//! ENTERA. Una fecha que no existe («31 de febrero», «25:00») es un error con
//! su motivo, nunca un dia de al lado; y una que ya paso, tambien (una tarea
//! «para ayer» es casi siempre un malentendido). Lo que no se entiende no se
//! adivina: el camino rapido se lo pasa al modelo.

/// Minusculas y sin tildes (la misma que `pindesk_ia::acciones::normalizar`).
pub fn normalizar(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' => 'a',
            'é' | 'è' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            'ñ' => 'n',
            otro => otro,
        })
        .collect()
}

/// Un momento en hora local: dias desde 1970-01-01 y minuto del dia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Momento {
    pub dia: i64,
    pub minuto: u32,
}

/// La hora por omision cuando solo se dice el dia.
pub const HORA_POR_OMISION: u32 = 9 * 60;

/// Los dias desde 1970-01-01 de una fecha (Howard Hinnant, como
/// `tareas::dias_de` de la app).
pub fn dias_de(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Al reves: (año, mes, dia).
pub fn fecha_de(dias: i64) -> (i64, i64, i64) {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Si la fecha existe (bisiestos incluidos).
pub fn existe(y: i64, m: i64, d: i64) -> bool {
    (1..=12).contains(&m) && d >= 1 && fecha_de(dias_de(y, m, d)) == (y, m, d)
}

/// 0 = lunes ... 6 = domingo (1970-01-01 fue jueves).
pub fn dia_de_la_semana(dia: i64) -> i64 {
    (dia + 3).rem_euclid(7)
}

impl Momento {
    /// El momento de unos milisegundos locales desde 1970.
    pub fn de_ms_locales(ms: i64) -> Momento {
        const DIA: i64 = 86_400_000;
        Momento {
            dia: ms.div_euclid(DIA),
            minuto: (ms.rem_euclid(DIA) / 60_000) as u32,
        }
    }

    /// La marca de las tareas: `⏰ 2026-10-09 10:00`.
    pub fn marca(&self) -> String {
        let (y, m, d) = fecha_de(self.dia);
        format!(
            "⏰ {y:04}-{m:02}-{d:02} {:02}:{:02}",
            self.minuto / 60,
            self.minuto % 60
        )
    }

    /// Lee una marca `⏰ AAAA-MM-DD HH:MM` al final de un texto.
    pub fn de_marca(texto: &str) -> Option<Momento> {
        let i = texto.rfind('⏰')?;
        let r = texto[i + '⏰'.len_utf8()..].trim();
        let n = |a: usize, b: usize| r.get(a..b).and_then(|s| s.parse::<i64>().ok());
        let (y, mo, d, h, mi) = (n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?);
        if !existe(y, mo, d) || !(0..24).contains(&h) || !(0..60).contains(&mi) {
            return None;
        }
        Some(Momento {
            dia: dias_de(y, mo, d),
            minuto: (h * 60 + mi) as u32,
        })
    }

    /// «mañana 10:00», «hoy 17:30», «el lun 20/10 9:00»: para las burbujas.
    pub fn legible(&self, ahora: Momento) -> String {
        let hora = format!("{:02}:{:02}", self.minuto / 60, self.minuto % 60);
        let dia = match self.dia - ahora.dia {
            0 => "hoy".to_string(),
            1 => "mañana".to_string(),
            -1 => "ayer".to_string(),
            _ => {
                let (y, m, d) = fecha_de(self.dia);
                let sem = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"]
                    [dia_de_la_semana(self.dia) as usize];
                let (ya, _, _) = fecha_de(ahora.dia);
                if y == ya {
                    format!("{sem} {d}/{m}")
                } else {
                    format!("{sem} {d}/{m}/{y}")
                }
            }
        };
        format!("{dia} {hora}")
    }
}

/// **Fecha y hora al minuto** para el modelo de la nube (17-oct-2026, el
/// usuario: «que al agente se le pasen fechas de subida con alto detalle
/// (minuto) para que barra rapido lo que le pido»): `vie 3-oct-2026 14:05`
/// en hora local. `ms_utc`: lo guardado (UTC); `desfase_ms`: hora local
/// menos UTC, que da quien llama (aqui no hay reloj). Antes del 2001 no es
/// una fecha de verdad (un mensaje sin hora): `None`.
pub fn fecha_y_hora(ms_utc: i64, desfase_ms: i64) -> Option<String> {
    if ms_utc < 978_307_200_000 {
        return None;
    }
    let m = Momento::de_ms_locales(ms_utc + desfase_ms);
    let (y, mes, d) = fecha_de(m.dia);
    const MESES: [&str; 12] = [
        "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
    ];
    let sem = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"][dia_de_la_semana(m.dia) as usize];
    Some(format!(
        "{sem} {d}-{}-{y} {:02}:{:02}",
        MESES[(mes - 1) as usize],
        m.minuto / 60,
        m.minuto % 60
    ))
}

/// Por que no vale una fecha (el texto va al usuario o al modelo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorFecha {
    /// No parece una fecha.
    NoEntiendo(String),
    /// Parece una fecha, pero no existe (31 de febrero, 25:00).
    NoExiste(String),
    /// Ya paso.
    Pasada(String),
}

impl std::fmt::Display for ErrorFecha {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorFecha::NoEntiendo(s) => write!(
                f,
                "no entiendo la fecha «{s}»; dila como «mañana a las 10», «el viernes» o «20/10/2026»"
            ),
            ErrorFecha::NoExiste(s) => write!(f, "la fecha «{s}» no existe"),
            ErrorFecha::Pasada(s) => write!(f, "la fecha «{s}» ya pasó"),
        }
    }
}

const MESES: &[&str] = &[
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
];
const DIAS: &[&str] = &[
    "lunes",
    "martes",
    "miercoles",
    "jueves",
    "viernes",
    "sabado",
    "domingo",
];

fn numero(p: &str) -> Option<i64> {
    if let Ok(n) = p.parse::<i64>() {
        return Some(n);
    }
    const PALABRAS: &[&str] = &[
        "cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve", "diez",
        "once", "doce", "trece", "catorce", "quince",
    ];
    match p {
        "un" | "una" => Some(1),
        "mediodia" => None,
        _ => PALABRAS.iter().position(|x| *x == p).map(|i| i as i64),
    }
}

fn mes(p: &str) -> Option<i64> {
    let p = if p == "setiembre" { "septiembre" } else { p };
    MESES
        .iter()
        .position(|m| *m == p || (p.len() >= 3 && m.starts_with(p) && p.len() <= m.len()))
        .map(|i| i as i64 + 1)
}

fn dia_semana(p: &str) -> Option<i64> {
    DIAS.iter().position(|d| *d == p).map(|i| i as i64)
}

/// Lo que se va leyendo de la frase.
#[derive(Default)]
struct Leido {
    dia: Option<i64>,
    minuto: Option<u32>,
    /// Si el dia salio de una fecha sin año (para pasarla al año que viene
    /// si ya paso).
    sin_anio: bool,
    /// Si el dia salio de un numero solo («el 20»): del mes en curso, o del
    /// siguiente si ya paso.
    sin_mes: bool,
}

/// «en 3 dias»: «en» seguido de un numero y una unidad.
fn en_unidad(p: &str, sig: Option<&str>, unidad: Option<&str>) -> bool {
    p == "en"
        && sig.and_then(numero).is_some()
        && matches!(
            unidad,
            Some("dia" | "dias" | "semana" | "semanas" | "hora" | "horas")
        )
}

/// Las palabras de relleno que no cambian nada.
fn relleno(p: &str) -> bool {
    matches!(
        p,
        "para"
            | "el"
            | "al"
            | "la"
            | "de"
            | "del"
            | "este"
            | "esta"
            | "dia"
            | "en"
            | "a"
            | "las"
            | "los"
            | "y"
            | "sobre"
            | "hacia"
            | "antes"
    )
}

/// **La fecha de una frase entera.** `ahora`: el momento local de ahora.
pub fn interpretar(frase: &str, ahora: Momento) -> Result<Momento, ErrorFecha> {
    let original = frase
        .trim()
        .trim_end_matches(['.', ',', ';'])
        .trim()
        .to_string();
    let q = normalizar(&original);
    // «20/10/2026», «2026-10-20» y «10:30» se parten aparte.
    // «5pm» y «5:30pm» se leen como «5 pm» y «5:30 pm» (18-oct-2026).
    let palabras: Vec<String> = q
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|p| !p.is_empty())
        .flat_map(
            |p| match p.strip_suffix("pm").or_else(|| p.strip_suffix("am")) {
                Some(n) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == ':') => {
                    vec![n.to_string(), p[n.len()..].to_string()]
                }
                _ => vec![p.to_string()],
            },
        )
        .collect();
    if palabras.is_empty() {
        return Err(ErrorFecha::NoEntiendo(original));
    }
    let no_entiendo = || ErrorFecha::NoEntiendo(original.clone());
    let no_existe = || ErrorFecha::NoExiste(original.clone());
    let mut l = Leido::default();
    let mut i = 0;
    let mut tarde: Option<bool> = None; // Some(true): pm; Some(false): am
    let poner_dia = |l: &mut Leido, d: i64| -> Result<(), ErrorFecha> {
        if l.dia.is_some() {
            return Err(ErrorFecha::NoEntiendo(original.clone()));
        }
        l.dia = Some(d);
        Ok(())
    };
    while i < palabras.len() {
        let p = palabras[i].as_str();
        let sig = palabras.get(i + 1).map(String::as_str);
        // --- dias relativos
        if p == "hoy" {
            poner_dia(&mut l, ahora.dia)?;
        } else if p == "pasado" && sig == Some("manana") {
            poner_dia(&mut l, ahora.dia + 2)?;
            i += 1;
        } else if p == "manana" {
            // «mañana» tras «de la» es la franja («a las 9 de la mañana»).
            let franja =
                i >= 2 && palabras[i - 1] == "la" && palabras[i - 2] == "de" && l.minuto.is_some();
            let por_la = i >= 2 && palabras[i - 1] == "la" && palabras[i - 2] == "por";
            if franja || por_la {
                tarde = Some(false);
                if por_la && l.minuto.is_none() {
                    l.minuto = Some(9 * 60);
                }
            } else {
                poner_dia(&mut l, ahora.dia + 1)?;
            }
        } else if p == "tarde" || p == "noche" {
            let franja_de = i >= 2 && palabras[i - 1] == "la" && palabras[i - 2] == "de";
            if franja_de && l.minuto.is_some() {
                tarde = Some(true);
            } else if l.minuto.is_none() {
                // «esta tarde», «por la noche»: una hora tipica.
                l.minuto = Some(if p == "tarde" { 17 * 60 } else { 20 * 60 });
                tarde = None;
                if i >= 1 && palabras[i - 1] == "esta" && l.dia.is_none() {
                    l.dia = Some(ahora.dia);
                }
            } else {
                return Err(no_entiendo());
            }
        } else if p == "mediodia" {
            if l.minuto.is_some() {
                return Err(no_entiendo());
            }
            l.minuto = Some(12 * 60);
        } else if p == "am" || p == "pm" {
            tarde = Some(p == "pm");
        } else if p == "por" && sig == Some("la") {
            // «por la mañana/tarde/noche»: lo hace la siguiente palabra.
        } else if let Some(ds) = dia_semana(p) {
            // «el viernes», «el proximo viernes», «el viernes que viene».
            let hoy = dia_de_la_semana(ahora.dia);
            let mut salto = (ds - hoy).rem_euclid(7);
            if salto == 0 {
                salto = 7;
            }
            if sig == Some("que") && palabras.get(i + 2).map(String::as_str) == Some("viene") {
                i += 2;
            }
            poner_dia(&mut l, ahora.dia + salto)?;
        } else if p == "proximo" || p == "proxima" || p == "siguiente" {
            // Lo pone la palabra que sigue (dia de la semana o «semana»).
        } else if p == "semana" {
            // «la semana que viene», «la proxima semana»: el lunes que viene.
            let antes = i >= 1 && matches!(palabras[i - 1].as_str(), "proxima" | "siguiente");
            let despues =
                sig == Some("que") && palabras.get(i + 2).map(String::as_str) == Some("viene");
            if !(antes || despues) {
                return Err(no_entiendo());
            }
            if despues {
                i += 2;
            }
            let hoy = dia_de_la_semana(ahora.dia);
            poner_dia(&mut l, ahora.dia + (7 - hoy))?;
        } else if (p == "dentro" && sig == Some("de"))
            || en_unidad(p, sig, palabras.get(i + 2).map(String::as_str))
        {
            let j = if p == "dentro" { i + 2 } else { i + 1 };
            let n = palabras
                .get(j)
                .and_then(|x| numero(x))
                .ok_or_else(no_entiendo)?;
            let unidad = palabras
                .get(j + 1)
                .map(String::as_str)
                .ok_or_else(no_entiendo)?;
            match unidad {
                "dia" | "dias" => poner_dia(&mut l, ahora.dia + n)?,
                "semana" | "semanas" => poner_dia(&mut l, ahora.dia + 7 * n)?,
                "hora" | "horas" => {
                    let total = ahora.dia * 1440 + i64::from(ahora.minuto) + 60 * n;
                    poner_dia(&mut l, total.div_euclid(1440))?;
                    if l.minuto.is_some() {
                        return Err(no_entiendo());
                    }
                    l.minuto = Some(total.rem_euclid(1440) as u32);
                }
                _ => return Err(no_entiendo()),
            }
            i = j + 1;
        } else if let Some((y, m, d)) = fecha_numerica(p, ahora)? {
            // «2026-10-20», «20/10», «20/10/2026», «20-10-26».
            if !existe(y.0, m, d) {
                return Err(no_existe());
            }
            l.sin_anio |= y.1;
            poner_dia(&mut l, dias_de(y.0, m, d))?;
        } else if p.contains(':')
            || p.ends_with('h') && p.len() <= 3 && p[..p.len() - 1].parse::<u32>().is_ok()
        {
            // «10:30», «10h».
            let (h, mi) = match p.split_once(':') {
                Some((h, m)) => (
                    h.parse::<i64>().map_err(|_| no_entiendo())?,
                    m.trim_end_matches('h')
                        .parse::<i64>()
                        .map_err(|_| no_entiendo())?,
                ),
                None => (
                    p[..p.len() - 1].parse::<i64>().map_err(|_| no_entiendo())?,
                    0,
                ),
            };
            if !(0..24).contains(&h) || !(0..60).contains(&mi) {
                return Err(no_existe());
            }
            if l.minuto.is_some() {
                return Err(no_entiendo());
            }
            l.minuto = Some((h * 60 + mi) as u32);
        } else if let Some(n) = numero(p) {
            // Un numero suelto: «a las 10» (hora) o «el 3 de noviembre» (dia).
            let tras_las = i >= 1
                && matches!(palabras[i - 1].as_str(), "las" | "la")
                && i >= 2
                && palabras[i - 2] == "a"
                || i >= 1 && palabras[i - 1] == "las";
            let mes_detras = match (sig, palabras.get(i + 2)) {
                (Some("de"), Some(m)) => mes(m),
                (Some(m), _) => mes(m),
                _ => None,
            };
            if let Some(m) = mes_detras.filter(|_| !tras_las) {
                i += if sig == Some("de") { 2 } else { 1 };
                // ¿Y año? «de 2026», «2026».
                let mut y = None;
                let a = palabras.get(i + 1).map(String::as_str);
                let b = palabras.get(i + 2).map(String::as_str);
                if let Some(x) = a.and_then(|a| a.parse::<i64>().ok()).filter(|x| *x >= 1000) {
                    y = Some(x);
                    i += 1;
                } else if matches!(a, Some("de" | "del"))
                    && let Some(x) = b.and_then(|b| b.parse::<i64>().ok()).filter(|x| *x >= 1000)
                {
                    y = Some(x);
                    i += 2;
                }
                let anio = y.unwrap_or_else(|| fecha_de(ahora.dia).0);
                if !existe(anio, m, n) {
                    return Err(no_existe());
                }
                l.sin_anio |= y.is_none();
                poner_dia(&mut l, dias_de(anio, m, n))?;
            } else if !tras_las
                && l.dia.is_none()
                && i >= 1
                && matches!(palabras[i - 1].as_str(), "el" | "dia")
                && (1..=31).contains(&n)
            {
                // «el 20 (a las 9)»: el dia 20 del mes en curso (18-oct-2026).
                let (y, m, _) = fecha_de(ahora.dia);
                let dia = (0..12)
                    .map(|k| (y + (m - 1 + k) / 12, (m - 1 + k) % 12 + 1))
                    .find(|&(y, m)| existe(y, m, n))
                    .map(|(y, m)| dias_de(y, m, n))
                    .ok_or_else(no_existe)?;
                l.sin_mes = true;
                poner_dia(&mut l, dia)?;
            } else if tras_las
                || matches!(
                    sig,
                    Some("de") | Some("en") | Some("am") | Some("pm") | Some("y")
                )
                || i + 1 == palabras.len()
            {
                // Hora sola: «a las 5 (de la tarde)», «a las 10 y media».
                if !(0..=24).contains(&n) {
                    return Err(no_existe());
                }
                if l.minuto.is_some() {
                    return Err(no_entiendo());
                }
                let mut minuto = (n * 60) as u32;
                if sig == Some("y") {
                    match palabras.get(i + 2).map(String::as_str) {
                        Some("media") => minuto += 30,
                        Some("cuarto") => minuto += 15,
                        _ => return Err(no_entiendo()),
                    }
                    i += 2;
                }
                if minuto >= 24 * 60 {
                    return Err(no_existe());
                }
                l.minuto = Some(minuto);
            } else {
                return Err(no_entiendo());
            }
        } else if let Some(m) =
            mes(p).filter(|_| i >= 1 && palabras[i - 1] == "de" && l.dia.is_none())
        {
            // «el dia 5 de este mes» no; aqui solo llega «de octubre» sin dia.
            let _ = m;
            return Err(no_entiendo());
        } else if relleno(p) || p == "que" || p == "viene" {
            // nada
        } else {
            return Err(no_entiendo());
        }
        i += 1;
    }
    if l.dia.is_none() && l.minuto.is_none() {
        return Err(no_entiendo());
    }
    let mut minuto = l.minuto.unwrap_or(HORA_POR_OMISION);
    if tarde == Some(true) && minuto < 12 * 60 {
        minuto += 12 * 60;
    } else if tarde == Some(false) && (12 * 60..13 * 60).contains(&minuto) {
        minuto -= 12 * 60;
    }
    let mut dia = match l.dia {
        Some(d) => d,
        // Solo la hora: hoy si aun no paso, si no mañana.
        None if minuto > ahora.minuto => ahora.dia,
        None => ahora.dia + 1,
    };
    if l.sin_mes && dia < ahora.dia {
        // «el 3» dicho el 18: el 3 del mes que viene (el primero que lo tenga).
        let (y, m, d) = fecha_de(dia);
        if let Some((y, m)) = (1..=12)
            .map(|k| (y + (m - 1 + k) / 12, (m - 1 + k) % 12 + 1))
            .find(|&(y, m)| existe(y, m, d))
        {
            dia = dias_de(y, m, d);
        }
    }
    if l.sin_anio && dia < ahora.dia {
        // «el 3 de enero» dicho en octubre: el del año que viene.
        let (y, m, d) = fecha_de(dia);
        if existe(y + 1, m, d) {
            dia = dias_de(y + 1, m, d);
        }
    }
    let r = Momento { dia, minuto };
    if r < ahora {
        return Err(ErrorFecha::Pasada(original));
    }
    Ok(r)
}

/// «2026-10-20», «20/10», «20/10/2026», «20-10-26», «20.10»: ((año, sin
/// año), mes, dia). `Ok(None)` si no tiene esa forma.
#[allow(clippy::type_complexity)]
fn fecha_numerica(p: &str, ahora: Momento) -> Result<Option<((i64, bool), i64, i64)>, ErrorFecha> {
    let sep = if p.contains('/') {
        '/'
    } else if p.contains('-') {
        '-'
    } else if p.contains('.') {
        '.'
    } else {
        return Ok(None);
    };
    let trozos: Vec<&str> = p.split(sep).collect();
    let n: Option<Vec<i64>> = trozos.iter().map(|t| t.parse::<i64>().ok()).collect();
    let Some(n) = n else { return Ok(None) };
    let este = fecha_de(ahora.dia).0;
    Ok(match n.as_slice() {
        [y, m, d] if trozos[0].len() == 4 => Some(((*y, false), *m, *d)),
        [d, m, y] => Some(((if *y < 100 { 2000 + y } else { *y }, false), *m, *d)),
        [d, m] => Some(((este, true), *m, *d)),
        _ => None,
    })
}

/// **Separa la fecha del final de una frase**: «llamar al banco mañana a las
/// 10» → («llamar al banco», mañana 10:00). Busca la cola MAS LARGA que sea
/// una fecha entera (con «para», «el»… delante). `Err` si la cola parece una
/// fecha que no existe (para no apuntar «pagar el 31 de febrero» como texto).
pub fn separar(frase: &str, ahora: Momento) -> Result<(String, Option<Momento>), ErrorFecha> {
    let palabras: Vec<&str> = frase.split_whitespace().collect();
    for i in 1..palabras.len() {
        let cola = palabras[i..].join(" ");
        let cabeza = palabras[..i].join(" ");
        let empieza = normalizar(palabras[i]);
        // La cola tiene que empezar como empieza una fecha.
        let parece = matches!(
            empieza.as_str(),
            "para"
                | "el"
                | "al"
                | "hoy"
                | "manana"
                | "pasado"
                | "a"
                | "en"
                | "dentro"
                | "este"
                | "esta"
                | "la"
                | "proximo"
                | "proxima"
                | "antes"
                | "sobre"
        ) || dia_semana(&empieza).is_some()
            || empieza.chars().next().is_some_and(|c| c.is_ascii_digit());
        if !parece {
            continue;
        }
        match interpretar(&cola, ahora) {
            Ok(m) => return Ok((cabeza.trim_end_matches([',', ' ']).to_string(), Some(m))),
            Err(e @ (ErrorFecha::NoExiste(_) | ErrorFecha::Pasada(_))) => return Err(e),
            Err(ErrorFecha::NoEntiendo(_)) => {}
        }
    }
    Ok((frase.trim().to_string(), None))
}

#[cfg(test)]
mod pruebas {

    #[test]
    fn fecha_y_hora_al_minuto_en_hora_local() {
        // 3-oct-2026 00:00 UTC = 2-oct 19:00 en Lima (UTC-5).
        let t = 1_790_985_600_000;
        assert_eq!(fecha_y_hora(t, 0).as_deref(), Some("sáb 3-oct-2026 00:00"));
        assert_eq!(
            fecha_y_hora(t, -5 * 3_600_000).as_deref(),
            Some("vie 2-oct-2026 19:00")
        );
        assert_eq!(
            fecha_y_hora(t + 14 * 3_600_000 + 5 * 60_000 + 59_999, 0).as_deref(),
            Some("sáb 3-oct-2026 14:05")
        );
        // Caso negativo: un mensaje sin hora no lleva fecha.
        assert_eq!(fecha_y_hora(0, 0), None);
    }

    use super::*;

    /// Miercoles 14 de octubre de 2026, 10:30.
    fn ahora() -> Momento {
        Momento {
            dia: dias_de(2026, 10, 14),
            minuto: 10 * 60 + 30,
        }
    }

    fn en(frase: &str) -> String {
        let m = interpretar(frase, ahora()).unwrap_or_else(|e| panic!("«{frase}»: {e}"));
        m.marca()
    }

    #[test]
    fn el_calendario_es_el_de_siempre() {
        assert_eq!(dia_de_la_semana(dias_de(2026, 10, 14)), 2, "miercoles");
        assert!(
            existe(2028, 2, 29)
                && !existe(2026, 2, 29)
                && !existe(2026, 2, 31)
                && !existe(2026, 13, 1)
        );
        assert_eq!(fecha_de(dias_de(2026, 12, 31)), (2026, 12, 31));
    }

    #[test]
    fn lo_que_dice_la_gente() {
        assert_eq!(en("mañana"), "⏰ 2026-10-15 09:00");
        assert_eq!(en("mañana a las 10"), "⏰ 2026-10-15 10:00");
        assert_eq!(en("para mañana a las 5 de la tarde"), "⏰ 2026-10-15 17:00");
        assert_eq!(en("pasado mañana"), "⏰ 2026-10-16 09:00");
        assert_eq!(en("hoy a las 18:30"), "⏰ 2026-10-14 18:30");
        assert_eq!(en("esta tarde"), "⏰ 2026-10-14 17:00");
        assert_eq!(en("el viernes"), "⏰ 2026-10-16 09:00");
        assert_eq!(
            en("el miércoles"),
            "⏰ 2026-10-21 09:00",
            "hoy es miercoles: el que viene"
        );
        assert_eq!(en("el lunes por la tarde"), "⏰ 2026-10-19 17:00");
        assert_eq!(en("la semana que viene"), "⏰ 2026-10-19 09:00");
        assert_eq!(en("en 3 días"), "⏰ 2026-10-17 09:00");
        assert_eq!(en("dentro de dos semanas"), "⏰ 2026-10-28 09:00");
        assert_eq!(en("20/10"), "⏰ 2026-10-20 09:00");
        assert_eq!(en("el 20/10/2026 a las 8:15"), "⏰ 2026-10-20 08:15");
        assert_eq!(en("2026-11-03 16:00"), "⏰ 2026-11-03 16:00");
        assert_eq!(en("el 3 de noviembre"), "⏰ 2026-11-03 09:00");
        assert_eq!(
            en("el 5 de enero"),
            "⏰ 2027-01-05 09:00",
            "sin año y ya pasado: el que viene"
        );
        assert_eq!(en("a las 12"), "⏰ 2026-10-14 12:00");
        assert_eq!(en("a las 9"), "⏰ 2026-10-15 09:00", "las 9 ya pasaron hoy");
        assert_eq!(en("mañana a las 10 y media"), "⏰ 2026-10-15 10:30");
    }

    /// El 18-oct-2026: dia, mes y hora sin año son del año en curso; si ya
    /// paso, del siguiente.
    #[test]
    fn dia_mes_y_hora_como_se_apuntan_en_una_tarea() {
        assert_eq!(en("18/10 17:00"), "⏰ 2026-10-18 17:00");
        assert_eq!(en("18-10 5pm"), "⏰ 2026-10-18 17:00");
        assert_eq!(en("18-10 5:30pm"), "⏰ 2026-10-18 17:30");
        assert_eq!(en("18 oct 17:00"), "⏰ 2026-10-18 17:00");
        assert_eq!(en("el 20 a las 9"), "⏰ 2026-10-20 09:00");
        assert_eq!(
            en("el 20"),
            "⏰ 2026-10-20 09:00",
            "«el 20» es un dia, no las 20:00"
        );
        assert_eq!(en("20 de octubre 8:30"), "⏰ 2026-10-20 08:30");
        assert_eq!(en("mañana 7"), "⏰ 2026-10-15 07:00");
        assert_eq!(en("viernes 15:30"), "⏰ 2026-10-16 15:30");
        assert_eq!(en("a las 8am"), "⏰ 2026-10-15 08:00");
        // Ya pasado este año: el que viene; «el 3» ya pasado: el mes que viene.
        assert_eq!(en("2/10 17:00"), "⏰ 2027-10-02 17:00");
        assert_eq!(en("13 oct"), "⏰ 2027-10-13 09:00");
        assert_eq!(en("el 3 a las 10"), "⏰ 2026-11-03 10:00");
        // «el 31» en un mes de 30 dias no existe: el siguiente que lo tenga.
        let fin_de_nov = Momento {
            dia: dias_de(2026, 11, 2),
            minuto: 600,
        };
        assert_eq!(
            interpretar("el 31", fin_de_nov).unwrap().marca(),
            "⏰ 2026-12-31 09:00"
        );
        // Negativos: sigue sin adivinar.
        for f in ["el 32", "18/13 17:00", "5xm"] {
            assert!(interpretar(f, ahora()).is_err(), "{f}");
        }
        assert!(matches!(
            interpretar("el 32", ahora()),
            Err(ErrorFecha::NoExiste(_))
        ));
    }

    #[test]
    fn lo_imposible_y_lo_pasado_no_se_adivinan() {
        for f in [
            "el 31 de febrero",
            "30/02/2027",
            "mañana a las 25:00",
            "el 32 de octubre",
        ] {
            assert!(
                matches!(interpretar(f, ahora()), Err(ErrorFecha::NoExiste(_))),
                "{f}"
            );
        }
        assert!(matches!(
            interpretar("el 1/1/2020", ahora()),
            Err(ErrorFecha::Pasada(_))
        ));
        for f in [
            "cuando pueda",
            "algun dia",
            "el jueves o el viernes",
            "",
            "mañana y pasado mañana",
        ] {
            assert!(
                matches!(interpretar(f, ahora()), Err(ErrorFecha::NoEntiendo(_))),
                "{f}"
            );
        }
    }

    #[test]
    fn la_fecha_se_separa_del_final() {
        let (t, m) = separar("llamar al banco mañana a las 10", ahora()).unwrap();
        assert_eq!(t, "llamar al banco");
        assert_eq!(m.unwrap().marca(), "⏰ 2026-10-15 10:00");
        let (t, m) = separar("revisar planos para el viernes", ahora()).unwrap();
        assert_eq!(
            (t.as_str(), m.unwrap().marca()),
            ("revisar planos", "⏰ 2026-10-16 09:00".to_string())
        );
        // Sin fecha, el texto tal cual (y «el» en medio no es una fecha).
        assert_eq!(
            separar("pedir el cemento", ahora()).unwrap(),
            ("pedir el cemento".into(), None)
        );
        assert_eq!(separar("comprar 3 sacos", ahora()).unwrap().1, None);
        assert!(separar("pagar el alquiler el 31 de febrero", ahora()).is_err());
    }

    #[test]
    fn la_marca_va_y_vuelve() {
        let m = interpretar("mañana a las 10", ahora()).unwrap();
        assert_eq!(Momento::de_marca(&format!("llamar {}", m.marca())), Some(m));
        assert_eq!(Momento::de_marca("llamar ⏰ 2026-02-31 10:00"), None);
        assert_eq!(m.legible(ahora()), "mañana 10:00");
    }
}
