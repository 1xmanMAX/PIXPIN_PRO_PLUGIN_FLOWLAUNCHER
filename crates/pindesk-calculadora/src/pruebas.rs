use super::*;
use std::path::PathBuf;

fn calc(s: &str) -> Calculo {
    calcular(s, Angulos::Radianes).unwrap_or_else(|f| panic!("{s}: {f}"))
}

#[test]
fn cuentas_exactas_con_su_decimal() {
    let c = calc("1/3 + 1/6");
    assert_eq!(c.ultima().exacto, "1/2");
    assert_eq!(c.ultima().decimal.as_deref(), Some("0.5"));
    assert_eq!(c.resultado(), "= 1/2  ≈ 0.5");
    assert_eq!(calc("sqrt(8)").ultima().exacto, "2√2");
    assert_eq!(
        calc("2^100").ultima().exacto,
        "1267650600228229401496703205376"
    );
    assert_eq!(calc("diff(x^3, x)").ultima().exacto, "3x^2");
    assert_eq!(
        calc("integrate(1/(1+x^2), x, -inf, inf)").ultima().exacto,
        "π"
    );
    assert_eq!(calc("det([[1,2],[3,4]])").ultima().exacto, "-2");
}

#[test]
fn una_ecuacion_sola_se_resuelve_sola() {
    let c = calc("2x+3=7");
    assert_eq!(c.ultima().que, Que::Resuelta(vec!["x".into()]));
    assert_eq!(c.ultima().leido, "2x + 3 = 7");
    assert_eq!(c.ultima().exacto, "{x = 2}");
    assert_eq!(c.resultado(), "x = 2");
    // Dos soluciones.
    assert_eq!(calc("x^2 - 5x + 6 = 0").ultima().exacto, "{x = 2, x = 3}");
    assert_eq!(calc("x^2 - 5x + 6 = 0").resultado(), "x = 2, x = 3");
    // Una desigualdad.
    assert_eq!(calc("x^2 < 4").resultado(), "-2 < x < 2");
    // Un sistema con tantas incognitas como ecuaciones.
    let s = calc("{x + y = 3, x - y = 1}");
    assert_eq!(s.ultima().que, Que::Resuelta(vec!["x".into(), "y".into()]));
    assert_eq!(s.resultado(), "x = 2, y = 1");
    // Caso negativo: dos incognitas en una ecuacion no se resuelven.
    assert_eq!(calc("x + y = 3").ultima().que, Que::Valor);
    // Y lo ya pedido con solve, tal cual.
    assert_eq!(calc("solve(2x=4, x)").ultima().que, Que::Valor);
}

#[test]
fn las_lineas_comparten_variables_y_funciones() {
    let c = calc("a := 2\nf(x) := x^2 + 1\n# un comentario\n\na x = f(3)");
    assert_eq!(c.lineas.len(), 3);
    assert_eq!(c.lineas[0].que, Que::Asignada("a".into()));
    assert_eq!(c.lineas[1].que, Que::Definida("f".into()));
    // `a` ya vale 2: la incognita es solo x.
    assert_eq!(c.ultima().que, Que::Resuelta(vec!["x".into()]));
    assert_eq!(c.resultado(), "x = 5");
    let pasos = c.pasos();
    assert!(pasos.iter().any(|p| p.contains("solve(…, x)")), "{pasos:?}");
}

#[test]
fn en_grados_si_se_pide() {
    assert_eq!(
        calcular("sin(30)", Angulos::Grados)
            .unwrap()
            .ultima()
            .exacto,
        "1/2"
    );
    assert_ne!(calc("sin(30)").ultima().exacto, "1/2");
    assert_eq!(calc("sin(30°)").ultima().exacto, "1/2");
}

#[test]
fn lo_que_falla_dice_por_que() {
    assert_eq!(
        calcular("  \n # nada", Angulos::Radianes),
        Err(Fallo::Vacio)
    );
    match calcular("1/0", Angulos::Radianes) {
        Err(Fallo::Motor { linea: 1, motivo }) => assert!(motivo.contains("cero"), "{motivo}"),
        otro => panic!("{otro:?}"),
    }
    match calcular("a := 1\nx+", Angulos::Radianes) {
        Err(f @ Fallo::Motor { linea: 2, .. }) => assert!(f.to_string().starts_with("linea 2")),
        otro => panic!("{otro:?}"),
    }
    let largo = "1+".repeat(MAX_LETRAS);
    assert_eq!(calcular(&largo, Angulos::Radianes), Err(Fallo::Largo));
    let muchas = "1\n".repeat(MAX_LINEAS + 1);
    assert_eq!(calcular(&muchas, Angulos::Radianes), Err(Fallo::Largo));
}

#[test]
fn lo_que_tarda_no_bloquea() {
    // Un tiempo de cero: aun la cuenta mas corta llega tarde.
    let r = calcular_con_tiempo("factor(2^61 - 1)", Angulos::Radianes, Duration::ZERO);
    assert!(matches!(r, Err(Fallo::Tarda) | Ok(_)), "{r:?}");
}

/// Lo raro que se puede teclear: el motor contesta (bien o con su error)
/// y no entra en panico. Con `panic = "abort"` del release, un panico
/// cerraria PinDesk entera.
#[test]
fn lo_raro_no_rompe_el_motor() {
    let raros: Vec<String> = [
        "(((((((((((((((((((((((((((((((((((((((((((((((((((((((((1",
        "((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((",
        "------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------1",
        "1e999999",
        "10^10^10",
        "0^0",
        "0/0",
        "sqrt(-1)^2",
        "[[1,2],[3]]",
        "inv([[1,2],[2,4]])",
        "det([[1,2,3]])",
        "{}",
        "[]",
        "()",
        "|",
        "x!!!!",
        "ln(0)",
        "log(-1)",
        "solve(x=x, x)",
        "solve(0=1, x)",
        "limit(1/x, x, 0)",
        "sum(k, k, 1, inf)",
        "integrate(e^(x^2), x)",
        "999999999999999999999999999999999999999! ",
        "factor(123456789012345678901234567890)",
        "A[0]",
        "f(x) := f(x)\nf(1)",
        "a := a + 1",
        "°°°",
        "√√√",
        "=",
        ":=",
        "1,2,3",
        "plot()",
        "%",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // En este hilo (con su pila grande) y no con `calcular`: alli un panico
    // se veria como un fallo mas y la prueba no lo notaria.
    let hilo = std::thread::Builder::new().stack_size(PILA).spawn(move || {
        for r in raros {
            if let Ok(lineas) = lineas_de(&r) {
                let _ = calcular_aqui(&lineas, Angulos::Radianes);
                let _ = calcular_aqui(&lineas, Angulos::Grados);
            }
        }
    });
    hilo.unwrap().join().expect("el motor entro en panico");
}

#[test]
fn lo_escrito_por_una_persona_tiene_que_parecer_una_cuenta() {
    for si in [
        "2x+3=7",
        "1/3 + 1/6",
        "sqrt(8)",
        "sin(30°)",
        "diff(x^3 sin(x), x)",
        "a := 2\n3a",
        "f(t) := t^2\nf(3)",
        "theta + 1",
        "x1 + x_2",
        "{x+y=3, x-y=1}",
        "15% * 80",
        "det([[1,2],[3,4]])",
        // Un nombre largo vale si lo define el propio calculo.
        "costo := 5\ncosto * 3",
    ] {
        assert!(parece_una_cuenta(si), "{si}");
    }
    for no in [
        "hola que tal",
        "precio total",
        "ok",
        "revisar el plano",
        "costo * 3",
        "x+",
        "SUMA(A1:A3)",
    ] {
        assert!(!parece_una_cuenta(no), "{no}");
    }
}

#[test]
fn en_el_chat_solo_lo_que_empieza_por_igual() {
    assert_eq!(pedido_del_chat("= 2x+3=7"), Some("2x+3=7"));
    assert_eq!(pedido_del_chat("  =1/3+1/6 "), Some("1/3+1/6"));
    // Casos negativos: sin `=`, `==`, solo `=`, una frase, una formula de Excel.
    assert_eq!(pedido_del_chat("2x+3=7"), None);
    assert_eq!(pedido_del_chat("== titulo =="), None);
    assert_eq!(pedido_del_chat("="), None);
    assert_eq!(pedido_del_chat("=)"), None);
    assert_eq!(pedido_del_chat("= hola que tal"), None);
    assert_eq!(pedido_del_chat("=SUMA(A1:A3)"), None);

    let nota = nota_de_calculo("= 2x+3=7").unwrap();
    assert_eq!(nota, "= 2x+3=7\n→ x = 2");
    assert!(es_nota_de_calculo(&nota));
    assert_eq!(cuenta_de_la_nota(&nota).as_deref(), Some("2x+3=7"));
    // Una nota que ya trae su resultado no se recalcula.
    assert_eq!(nota_de_calculo(&nota), None);
    // Lo que no se puede calcular sale como se escribio.
    assert_eq!(nota_de_calculo("= 1/0"), None);
    assert_eq!(nota_de_calculo("= x+"), None);
    // Varias lineas.
    let n = nota_de_calculo("= a := 3\n2a").unwrap();
    assert_eq!(n, "= a := 3\n2a\n→ = 6");
    assert_eq!(cuenta_de_la_nota(&n).as_deref(), Some("a := 3\n2a"));
    assert!(!es_nota_de_calculo("= algo\nsin flecha"));
    assert_eq!(cuenta_de_la_nota("hola"), None);
}

#[test]
fn la_herramienta_valida_y_contesta_en_json() {
    let (nombre, descripcion, esquema) = herramienta();
    assert_eq!(nombre, NOMBRE);
    assert!(descripcion.contains("de cabeza"));
    assert_eq!(esquema["required"][0], "expresion");

    let args = |v: Value| v.as_object().unwrap().clone();
    let p = validar(&args(json!({"expresion": " 2x+3=7 "}))).unwrap();
    assert_eq!(p.expresion, "2x+3=7");
    assert_eq!(p.angulos, Angulos::Radianes);
    let r: Value = serde_json::from_str(&hacer(&p).unwrap()).unwrap();
    assert_eq!(r["resultado"], "{x = 2}");
    assert_eq!(r["lineas"][0]["resuelto_para"][0], "x");

    // Otro nombre de campo y en grados.
    let p = validar(&args(json!({"expr": "sin(30)", "angulos": "grados"}))).unwrap();
    let r: Value = serde_json::from_str(&hacer(&p).unwrap()).unwrap();
    assert_eq!(r["resultado"], "1/2");
    assert_eq!(r["leido"], "sin(30)");
    let r: Value = serde_json::from_str(
        &hacer(&validar(&args(json!({"expresion": "1/3"}))).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(r["decimal"], "0.3333333333");

    // Casos negativos.
    assert!(validar(&args(json!({}))).unwrap_err().contains("expresion"));
    assert!(validar(&args(json!({"expresion": "  "}))).is_err());
    assert!(
        validar(&args(json!({"expresion": "1", "angulos": "gradianes"})))
            .unwrap_err()
            .contains("angulos")
    );
    let fallo = hacer(&validar(&args(json!({"expresion": "1/0"}))).unwrap()).unwrap_err();
    assert!(
        fallo.starts_with("calcular: ") && fallo.contains("cero"),
        "{fallo}"
    );
}

#[test]
fn encuentra_la_app_instalada() {
    let local = PathBuf::from("C:/Users/max/AppData/Local");
    let en_programs = local
        .join("Programs")
        .join("Calculator PROMAX")
        .join(app::EXE);
    let var = |v: &str| (v == "LOCALAPPDATA").then(|| local.clone().into_os_string());
    let r = app::buscar(&var, &|p| p == en_programs);
    assert_eq!(r.as_deref(), Some(en_programs.as_path()));

    // En el PATH.
    let carpeta = PathBuf::from("/opt/promax");
    let path = std::env::join_paths([PathBuf::from("/usr/bin"), carpeta.clone()]).unwrap();
    let en_path = carpeta.join(app::EXE);
    let var = |v: &str| (v == "PATH").then(|| path.clone());
    assert_eq!(app::buscar(&var, &|p| p == en_path), Some(en_path.clone()));

    // Caso negativo: en ningun sitio.
    assert_eq!(app::buscar(&var, &|_| false), None);
    assert_eq!(app::buscar(&|_| None, &|_| true), None);
}

/// La copia de promax-core sigue sin dependencias (lo dice su PROCEDENCIA.md).
#[test]
fn promax_core_sigue_sin_dependencias() {
    let toml = include_str!("../../promax-core/Cargo.toml");
    let deps = toml
        .split("[dependencies]")
        .nth(1)
        .unwrap_or_default()
        .lines()
        .take_while(|l| !l.starts_with('['))
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .count();
    assert_eq!(deps, 0, "promax-core tiene que seguir sin dependencias");
}
