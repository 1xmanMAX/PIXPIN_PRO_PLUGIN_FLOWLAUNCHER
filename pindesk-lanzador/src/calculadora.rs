//! **La calculadora en Flow** (8-nov-2026): `p = 2x+3=7`, `p = sqrt(8)` o
//! `p calc diff(x^3, x)`.
//!
//! La cuenta se hace aqui mismo, en el proceso del plugin y sin la app, con
//! el motor de Calculator PROMAX (`pindesk_calculadora`, el mismo que el chat
//! y el asistente). Sale:
//!
//! 1. el resultado exacto (Intro lo copia), con lo que entendio y lo hecho
//!    en el texto de ayuda del subtitulo (los «pasos»);
//! 2. su decimal, si es otro (Intro lo copia);
//! 3. «Abrir en Calculator PROMAX» si la app esta instalada: la abre y deja
//!    la cuenta en el portapapeles para pegarla alli.
//!
//! Mientras se escribe, una cuenta a medias (`= 2x+`) dice por que no sale,
//! sin cerrar Flow.

use crate::resultados::{Accion, Contexto, Resultado, glifo};

/// Lo que sale en Flow para `= <cuenta>`.
pub fn resultados(cuenta: &str, ctx: &Contexto) -> Vec<Resultado> {
    let cuenta = cuenta.trim();
    if cuenta.is_empty() {
        let mut r = Resultado::nuevo(
            "Calculadora (Calculator PROMAX)",
            "Escribe una cuenta: = 2x+3=7 · = sqrt(8) · = diff(x^3, x) · = det([[1,2],[3,4]])",
            glifo::CALCULADORA,
            Accion::Consulta(ctx.consulta("= ")),
        );
        r.autocompletar = Some(ctx.consulta("= "));
        return vec![r];
    }
    let mut v = Vec::new();
    match pindesk_calculadora::calcular(cuenta, pindesk_calculadora::Angulos::Radianes) {
        Ok(c) => {
            let ultima = c.ultima();
            let resultado = c.resultado();
            let copiar = copiable(&resultado);
            let mut r = Resultado::nuevo(
                resultado.clone(),
                format!("{} · Intro copia el resultado", ultima.leido),
                glifo::CALCULADORA,
                Accion::Copiar(copiar.clone()),
            );
            r.copiar = Some(copiar);
            r.ayuda_titulo = Some(resultado);
            r.ayuda_subtitulo = Some(c.pasos().join("\n"));
            v.push(r);
            if let Some(d) = &ultima.decimal {
                let mut r = Resultado::nuevo(
                    format!("≈ {d}"),
                    "El decimal · Intro lo copia",
                    glifo::CALCULADORA,
                    Accion::Copiar(d.clone()),
                );
                r.copiar = Some(d.clone());
                v.push(r);
            }
        }
        Err(f) => {
            // Sin cerrar Flow: se sigue escribiendo.
            let mut r = Resultado::nuevo(
                format!("No se puede calcular: {f}"),
                "Sigue escribiendo la cuenta (Calculator PROMAX)",
                glifo::AVISO,
                Accion::Consulta(ctx.consulta(&format!("= {cuenta}"))),
            );
            r.ayuda_titulo = Some(f.to_string());
            v.push(r);
        }
    }
    if let Some(exe) = (ctx.promax)() {
        let exe = exe.to_string_lossy().to_string();
        let mut r = Resultado::nuevo(
            "Abrir en Calculator PROMAX",
            "Abre la app con la cuenta copiada: pégala con Ctrl+V",
            glifo::ABRIR,
            Accion::Promax {
                exe: exe.clone(),
                cuenta: cuenta.to_string(),
            },
        );
        r.ayuda_subtitulo = Some(exe);
        v.push(r);
    }
    v
}

/// Lo que se copia de un resultado: sin el `= ` de delante (`= 1/2  ≈ 0.5`
/// copia `1/2`; `x = 2` se copia entero).
fn copiable(resultado: &str) -> String {
    let sin_decimal = resultado.split("  ≈ ").next().unwrap_or(resultado);
    sin_decimal
        .strip_prefix("= ")
        .unwrap_or(sin_decimal)
        .to_string()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn ctx(promax: fn() -> Option<std::path::PathBuf>) -> Contexto {
        let mut c = Contexto::con("p ", 0);
        c.promax = promax;
        c
    }

    #[test]
    fn una_cuenta_sale_con_su_decimal_y_se_copia() {
        let v = resultados("1/3 + 1/6", &ctx(|| None));
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].titulo, "= 1/2  ≈ 0.5");
        assert_eq!(v[0].accion, Accion::Copiar("1/2".into()));
        assert!(v[0].subtitulo.starts_with("1/3 + 1/6"));
        assert_eq!(v[1].titulo, "≈ 0.5");
        assert_eq!(v[1].accion, Accion::Copiar("0.5".into()));
    }

    #[test]
    fn una_ecuacion_se_resuelve_y_dice_que_hizo() {
        let v = resultados("2x+3=7", &ctx(|| None));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].titulo, "x = 2");
        assert_eq!(v[0].accion, Accion::Copiar("x = 2".into()));
        let pasos = v[0].ayuda_subtitulo.as_deref().unwrap();
        assert!(pasos.contains("solve(…, x)"), "{pasos}");
        let json = v[0].a_json("icono.png", 100);
        assert_eq!(json["copyText"], "x = 2");
        assert_eq!(json["jsonRPCAction"]["method"], "copiar");
    }

    #[test]
    fn vacia_o_a_medias_no_cierra_flow() {
        let v = resultados("", &ctx(|| None));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].accion, Accion::Consulta("p = ".into()));
        let v = resultados("2x+", &ctx(|| None));
        assert_eq!(v[0].glifo, glifo::AVISO);
        assert!(v[0].titulo.contains("incompleta"), "{}", v[0].titulo);
        assert_eq!(v[0].accion, Accion::Consulta("p = 2x+".into()));
    }

    #[test]
    fn con_la_app_instalada_se_ofrece_abrirla() {
        fn instalada() -> Option<std::path::PathBuf> {
            Some(std::path::PathBuf::from(r"C:\Calc\calculator-promax.exe"))
        }
        let v = resultados("sqrt(8)", &ctx(instalada));
        let abrir = v.last().unwrap();
        assert_eq!(abrir.titulo, "Abrir en Calculator PROMAX");
        let Accion::Promax { exe, cuenta } = &abrir.accion else {
            panic!("{:?}", abrir.accion)
        };
        assert!(exe.ends_with("calculator-promax.exe"));
        assert_eq!(cuenta, "sqrt(8)");
        // La accion va y vuelve por el menu contextual.
        assert_eq!(
            Accion::de_valor(&abrir.accion.a_valor()),
            Some(abrir.accion.clone())
        );
        // Caso negativo: sin la app, no se ofrece.
        assert!(
            resultados("sqrt(8)", &ctx(|| None))
                .iter()
                .all(|r| !matches!(r.accion, Accion::Promax { .. }))
        );
    }

    #[test]
    fn se_copia_sin_el_igual() {
        assert_eq!(copiable("= 1/2  ≈ 0.5"), "1/2");
        assert_eq!(copiable("= 2√2"), "2√2");
        assert_eq!(copiable("x = 2, y = 1"), "x = 2, y = 1");
    }
}
