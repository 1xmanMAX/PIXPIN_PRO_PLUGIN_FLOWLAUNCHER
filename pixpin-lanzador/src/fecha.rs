//! La hora de un mensaje como la dice el chat: «hoy 14:32», «ayer 09:10»,
//! «12 sep, 18:05», «3 mar 2025». En la hora local de Windows (con su
//! horario de verano del dia que toque).

use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

/// Un instante en la hora local.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Local {
    pub anio: u16,
    pub mes: u16,
    pub dia: u16,
    pub hora: u16,
    pub minuto: u16,
}

impl Local {
    fn mismo_dia(&self, otro: &Local) -> bool {
        (self.anio, self.mes, self.dia) == (otro.anio, otro.mes, otro.dia)
    }
}

/// Milisegundos desde 1970 (UTC) → hora local. `None` si Windows no puede.
pub fn local(ms: i64) -> Option<Local> {
    // FILETIME: centenas de ns desde 1601.
    let t = (ms.checked_add(11_644_473_600_000)?).checked_mul(10_000)?;
    if t < 0 {
        return None;
    }
    let t = t as u64;
    let ft = FILETIME {
        dwLowDateTime: t as u32,
        dwHighDateTime: (t >> 32) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut aqui = SYSTEMTIME::default();
    // SAFETY: punteros a variables locales que viven toda la llamada.
    unsafe {
        FileTimeToSystemTime(&ft, &mut utc).ok()?;
        SystemTimeToTzSpecificLocalTime(None, &utc, &mut aqui).ok()?;
    }
    Some(Local {
        anio: aqui.wYear,
        mes: aqui.wMonth,
        dia: aqui.wDay,
        hora: aqui.wHour,
        minuto: aqui.wMinute,
    })
}

const MESES: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
];

/// Como lo dice el chat, comparando con `ahora` en el calendario local.
pub fn momento(ms: i64, ahora: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let (Some(l), Some(a)) = (local(ms), local(ahora)) else {
        return crate::resultados::hace(ms, ahora);
    };
    formatear(&l, &a, local(ahora - 86_400_000).as_ref())
}

/// «10 oct»: el dia y el mes de `ms` en la hora local (vacio si Windows
/// no puede).
pub fn dia_y_mes(ms: i64) -> String {
    local(ms).map(|l| dia_y_mes_de(&l)).unwrap_or_default()
}

/// Lo de [`dia_y_mes`], sin Windows.
pub fn dia_y_mes_de(l: &Local) -> String {
    let mes = MESES
        .get(usize::from(l.mes.max(1)) - 1)
        .copied()
        .unwrap_or("");
    format!("{} {mes}", l.dia)
}

/// Cuanto hace, con minutos y horas el primer dia: «ahora mismo», «hace 5
/// min», «hace 2 h»; despues, como [`crate::resultados::hace`].
pub fn hace_con_horas(ms: i64, ahora: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let s = (ahora - ms).max(0) / 1000;
    match s {
        0..=59 => "ahora mismo".into(),
        60..=3599 => format!("hace {} min", s / 60),
        3600..=86_399 => format!("hace {} h", s / 3600),
        _ => crate::resultados::hace(ms, ahora),
    }
}

/// Lo de [`momento`], sin Windows (para probarlo).
pub fn formatear(l: &Local, ahora: &Local, ayer: Option<&Local>) -> String {
    let hora = format!("{:02}:{:02}", l.hora, l.minuto);
    if l.mismo_dia(ahora) {
        return format!("hoy {hora}");
    }
    if ayer.is_some_and(|y| l.mismo_dia(y)) {
        return format!("ayer {hora}");
    }
    let mes = MESES
        .get(usize::from(l.mes.max(1)) - 1)
        .copied()
        .unwrap_or("");
    if l.anio == ahora.anio {
        format!("{} {mes}, {hora}", l.dia)
    } else {
        format!("{} {mes} {}", l.dia, l.anio)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn en(anio: u16, mes: u16, dia: u16, hora: u16, minuto: u16) -> Local {
        Local {
            anio,
            mes,
            dia,
            hora,
            minuto,
        }
    }

    #[test]
    fn hoy_ayer_este_anio_y_otro() {
        let ahora = en(2026, 10, 2, 12, 0);
        let ayer = en(2026, 10, 1, 12, 0);
        assert_eq!(
            formatear(&en(2026, 10, 2, 9, 5), &ahora, Some(&ayer)),
            "hoy 09:05"
        );
        assert_eq!(
            formatear(&en(2026, 10, 1, 23, 59), &ahora, Some(&ayer)),
            "ayer 23:59"
        );
        assert_eq!(
            formatear(&en(2026, 9, 12, 18, 5), &ahora, Some(&ayer)),
            "12 sep, 18:05"
        );
        assert_eq!(
            formatear(&en(2025, 3, 3, 1, 0), &ahora, Some(&ayer)),
            "3 mar 2025"
        );
    }

    #[test]
    fn horas_el_primer_dia_y_dias_despues() {
        let h = 3_600_000;
        let ahora = 100 * 86_400_000;
        assert_eq!(hace_con_horas(ahora - 10_000, ahora), "ahora mismo");
        assert_eq!(hace_con_horas(ahora - 5 * 60_000, ahora), "hace 5 min");
        assert_eq!(hace_con_horas(ahora - 2 * h, ahora), "hace 2 h");
        assert_eq!(hace_con_horas(ahora - 3 * 24 * h, ahora), "hace 3 días");
        assert_eq!(dia_y_mes_de(&en(2026, 10, 10, 0, 0)), "10 oct");
    }

    #[test]
    fn caso_negativo_sin_fecha_no_se_dice_nada() {
        assert_eq!(hace_con_horas(0, 1_000_000), "");
        assert_eq!(hace_con_horas(-5, 1_000_000), "");
    }

    #[test]
    fn windows_da_una_hora_local_razonable() {
        let l = local(1_790_000_000_000).expect("hora local");
        assert!(l.anio == 2026 && (1..=12).contains(&l.mes) && l.hora < 24);
        assert_eq!(momento(0, 1), "");
    }
}
