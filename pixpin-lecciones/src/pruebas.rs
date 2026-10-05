//! Las pruebas de `LeccionesTest.kt`, una a una, mas las del PC (el JSON con
//! campos desconocidos, el resumen y la fusion).

use crate::buscador::{self, Indice};
use crate::leccion::{self, DIA, Leccion, Repaso, TIPO_ERROR};
use crate::{dictado, etiquetador, fusion, texto};

fn l(id: &str, titulo: &str) -> Leccion {
    Leccion::nueva(id, 1000, titulo)
}

fn con(id: &str, titulo: &str, etiquetas: &[&str], area: &str, repes: usize) -> Leccion {
    Leccion {
        etiquetas: etiquetas.iter().map(|s| s.to_string()).collect(),
        area: area.to_string(),
        repeticiones: (0..repes).map(|i| 2000 + i as i64).collect(),
        ..l(id, titulo)
    }
}

#[test]
fn una_frase_sin_marcas_es_lo_aprendido() {
    let c = dictado::repartir("revisar la escala antes de imprimir");
    assert_eq!(c.titulo, "Revisar la escala antes de imprimir");
    assert_eq!(c.que_paso, "");
}

#[test]
fn el_dictado_de_corrido_se_reparte_en_sus_campos() {
    let c = dictado::repartir(
        "pasó que se vació la losa sin revisar el encofrado porque el maestro tenía prisa, la próxima vez reviso los puntales antes del vaciado",
    );
    assert_eq!(c.que_paso, "Se vació la losa sin revisar el encofrado");
    assert_eq!(c.por_que, "El maestro tenía prisa");
    assert_eq!(c.proxima, "Reviso los puntales antes del vaciado");
    // Sin «aprendí que», lo que se hara distinto hace de titulo.
    assert_eq!(c.titulo, "Reviso los puntales antes del vaciado");
}

#[test]
fn lo_que_va_delante_de_las_marcas_es_el_titulo() {
    let c = dictado::repartir(
        "Pedir todo por escrito. Pasó que el cliente cambió el acuerdo de palabra",
    );
    assert_eq!(c.titulo, "Pedir todo por escrito");
    assert_eq!(c.que_paso, "El cliente cambió el acuerdo de palabra");
}

#[test]
fn las_palabras_se_comparan_sin_acentos_ni_plurales() {
    assert_eq!(
        texto::raiz(&texto::normal("Estructuras")),
        texto::raiz(&texto::normal("estructura"))
    );
    assert_eq!(
        texto::raiz(&texto::normal("hormigón")),
        texto::raiz("hormigon")
    );
    assert_eq!(texto::raiz("construccion"), texto::raiz("construcciones"));
    assert_eq!(texto::normal("Ñandú"), "ñandu");
}

#[test]
fn propone_etiquetas_area_tipo_y_causas() {
    let p = etiquetador::proponer(
        "No revisé el encofrado y la losa se fisuró al vaciar, error por prisa #obra",
        &etiquetador::Aprendido::default(),
        &[],
    );
    assert!(p.etiquetas.contains(&"obra".into()), "{:?}", p.etiquetas);
    assert!(
        p.etiquetas.contains(&"concreto".into()),
        "{:?}",
        p.etiquetas
    );
    assert!(
        p.etiquetas.contains(&"encofrado".into()),
        "{:?}",
        p.etiquetas
    );
    assert_eq!(p.area.as_deref(), Some("Construcción"));
    assert_eq!(p.tipo.as_deref(), Some(TIPO_ERROR));
    assert!(p.causas.contains(&"No revisé".into()), "{:?}", p.causas);
    assert!(p.causas.contains(&"Prisa".into()), "{:?}", p.causas);
}

#[test]
fn mal_no_salta_dentro_de_otra_palabra() {
    let p = etiquetador::proponer(
        "El resultado normal del animal",
        &etiquetador::Aprendido::default(),
        &[],
    );
    assert_eq!(p.tipo, None);
}

#[test]
fn lo_quitado_no_vuelve() {
    let p = etiquetador::proponer(
        "la losa se fisuró",
        &etiquetador::Aprendido::default(),
        &["concreto".into()],
    );
    assert!(!p.etiquetas.contains(&"concreto".into()));
}

#[test]
fn aprende_las_etiquetas_de_cada_uno() {
    let hechas = vec![
        con("1", "Revisar puntales del encofrado", &["losas"], "", 0),
        con("2", "Encofrado con puntales firmes", &["losas"], "", 0),
    ];
    let p = etiquetador::proponer(
        "Los puntales del encofrado cedieron",
        &etiquetador::aprender(&hechas),
        &[],
    );
    assert!(p.etiquetas.contains(&"losas".into()), "{:?}", p.etiquetas);
}

#[test]
fn las_etiquetas_dichas_se_recogen_y_se_quitan_del_texto() {
    assert_eq!(
        etiquetador::escritas("Mandar el capítulo #Tesis etiqueta asesor"),
        vec!["tesis", "asesor"]
    );
    assert_eq!(
        etiquetador::sin_etiquetas("Mandar el capítulo #Tesis etiqueta asesor"),
        "Mandar el capítulo"
    );
}

fn todas() -> Vec<Indice> {
    vec![
        con(
            "a",
            "Revisar la escala antes de imprimir los planos",
            &["planos"],
            "",
            0,
        ),
        con("b", "Pedir todo por escrito al cliente", &[], "Trabajo", 0),
        con(
            "c",
            "Revisar puntales del encofrado antes del vaciado",
            &[],
            "Construcción",
            0,
        ),
        con("d", "Dormir antes del examen final", &[], "Estudio", 0),
    ]
    .into_iter()
    .map(Indice::nuevo)
    .collect()
}

#[test]
fn busca_con_erratas_y_sin_acentos() {
    let t = todas();
    assert_eq!(buscador::buscar(&t, "escla")[0].leccion.id, "a");
    assert_eq!(buscador::buscar(&t, "ESCRITO")[0].leccion.id, "b");
    assert_eq!(buscador::buscar(&t, "exámenes")[0].leccion.id, "d");
}

#[test]
fn una_palabra_encuentra_por_su_concepto_aunque_no_salga() {
    // «obra» no sale en ninguna; el encofrado es de Construccion.
    assert_eq!(buscador::buscar(&todas(), "obra")[0].leccion.id, "c");
}

#[test]
fn las_que_se_repiten_suben() {
    let dos = vec![
        Indice::nuevo(con("x", "Revisar la escala", &[], "", 0)),
        Indice::nuevo(con("y", "Revisar la escala", &[], "", 3)),
    ];
    assert_eq!(buscador::buscar(&dos, "escala")[0].leccion.id, "y");
}

#[test]
fn avisa_si_ya_hay_una_parecida() {
    let t = todas();
    let p = buscador::parecidas_por_defecto(&t, "otra vez no revisé la escala al imprimir planos");
    assert_eq!(p[0].leccion.id, "a");
    assert!(buscador::parecidas_por_defecto(&t, "comprar pan en la tienda").is_empty());
}

#[test]
fn el_repaso_se_aleja_al_recordar_y_vuelve_al_olvidar_o_repetirse() {
    let mut x = l("r", "algo");
    x = Repaso::recordada(&x, 0);
    assert_eq!(x.caja, 1);
    assert_eq!(x.repasar, 3 * DIA);
    x = Repaso::recordada(&x, 0);
    assert_eq!(x.repasar, 7 * DIA);
    assert_eq!(Repaso::olvidada(&x, 0).caja, 0);
    let rep = Repaso::repetida(&x, 5);
    assert_eq!(rep.caja, 0);
    assert_eq!(rep.veces_que_paso(), 2);
    assert_eq!(rep.gravedad, 2);
    assert_eq!(Repaso::repetida(&rep, 6).gravedad, 3);
}

#[test]
fn el_archivo_se_lee_aunque_traiga_campos_de_una_version_nueva() {
    let txt = l("j", "Algo")
        .escribir()
        .replace("\"titulo\"", "\"campoNuevo\": 1, \"titulo\"");
    let leida = Leccion::leer(&txt).expect("se lee");
    assert_eq!(leida.titulo, "Algo");
    // Y al reescribirla, lo desconocido sigue ahi.
    assert!(leida.escribir().contains("\"campoNuevo\""));
}

// ------------------------------------------------------------ las del PC

#[test]
fn un_archivo_del_movil_se_lee_con_sus_valores_por_defecto() {
    // Lo minimo que escribiria una version vieja: sin tocada ni repasar.
    let l = Leccion::leer(r#"{"id":"k1","creada":5000,"titulo":"Hola"}"#).unwrap();
    assert_eq!(l.tocada, 5000);
    assert_eq!(l.repasar, 5000 + DIA);
    assert_eq!(l.tipo, leccion::TIPO_LECCION);
    assert!(l.en_lista);
    // Casos negativos: sin titulo, o con un tipo cambiado, no es una leccion.
    assert!(Leccion::leer(r#"{"id":"k1","creada":5000}"#).is_none());
    assert!(Leccion::leer(r#"{"id":"k1","creada":5000,"titulo":"x","gravedad":"alta"}"#).is_none());
    assert!(Leccion::leer("no es json").is_none());
}

#[test]
fn el_json_usa_los_nombres_de_kotlin() {
    let t = l("j", "Algo").escribir();
    for campo in [
        "\"quePaso\"",
        "\"porQue\"",
        "\"etiquetasAuto\"",
        "\"deMensaje\": null",
        "\"enLista\": true",
    ] {
        assert!(t.contains(campo), "falta {campo} en {t}");
    }
    assert!(!t.contains("que_paso"));
}

#[test]
fn el_resumen_es_el_del_movil() {
    let x = Leccion {
        tipo: TIPO_ERROR.into(),
        que_paso: "Se vació".into(),
        proxima: "Revisar".into(),
        etiquetas: vec!["obra".into()],
        etiquetas_auto: vec!["concreto".into(), "obra".into()],
        repeticiones: vec![1, 2],
        ..l("r", "No vaciar sin revisar")
    };
    assert_eq!(
        x.resumen(),
        "⚠️ Error que no repetir: No vaciar sin revisar\nQué pasó: Se vació\nLa próxima vez: Revisar\n🔁 Pasó 3 veces\n#obra #concreto"
    );
    assert_eq!(l("q", "Corta").resumen(), "💡 Lección: Corta");
    assert_eq!(x.nombre(), "💡 No vaciar sin revisar");
}

#[test]
fn el_id_nuevo_lleva_la_hora_en_base_36() {
    assert_eq!(leccion::base36(35), "z");
    assert_eq!(leccion::base36(36), "10");
    let id = leccion::nuevo_id(1_790_000_000_000);
    assert!(id.starts_with(&leccion::base36(1_790_000_000_000)));
    assert_eq!(id.len(), leccion::base36(1_790_000_000_000).len() + 3);
    assert_ne!(
        leccion::nuevo_id(1),
        leccion::nuevo_id(1),
        "dos del mismo instante no chocan"
    );
}

#[test]
fn las_de_hoy_van_primero_las_graves() {
    let a = Leccion {
        gravedad: 1,
        ..l("a", "a")
    };
    let b = Leccion {
        gravedad: 3,
        ..l("b", "b")
    };
    let lejos = Leccion {
        repasar: 10 * DIA,
        gravedad: 3,
        ..l("c", "c")
    };
    let hoy = Repaso::de_hoy(&[a, b, lejos], 2 * DIA, 5);
    let ids: Vec<_> = hoy.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(ids, ["b", "a"]);
}

#[test]
fn la_fusion_junta_las_repeticiones_y_el_repaso_mas_reciente() {
    let base = l("f", "Revisar puntales");
    // En el movil: me volvio a pasar (toca `tocada`).
    let movil = Repaso::repetida(&base, 10 * DIA);
    // En el PC: se repaso y se recordo mas tarde (no toca `tocada`), y otra
    // repeticion propia.
    let pc = Repaso::recordada(&Repaso::repetida(&base, 9 * DIA), 12 * DIA);
    let junta = fusion::fusionar(&pc, &movil, fusion::Modo::Suave);
    assert_eq!(
        junta.repeticiones,
        vec![9 * DIA, 10 * DIA],
        "ninguna se pierde"
    );
    assert_eq!(junta.gravedad, 3, "dos veces: grave");
    assert_eq!(
        junta.repasar, pc.repasar,
        "el repaso del PC es el mas reciente"
    );
    assert_eq!(junta.tocada, 10 * DIA);
}

#[test]
fn la_fusion_completa_une_etiquetas_y_los_textos_son_del_mas_nuevo() {
    let base = l("f", "Viejo");
    let aqui = Leccion {
        tocada: 50,
        titulo: "Nuevo de aqui".into(),
        etiquetas: vec!["obra".into()],
        quitadas: vec!["planos".into()],
        ..base.clone()
    };
    let mut llega = Leccion {
        tocada: 40,
        titulo: "De alli".into(),
        etiquetas: vec!["tesis".into()],
        etiquetas_auto: vec!["planos".into()],
        ..base
    };
    llega
        .resto
        .insert("campoNuevo".into(), serde_json::json!(7));
    let txt = fusion::al_llegar(&aqui.escribir(), &llega.escribir()).expect("hay que juntar");
    let j = Leccion::leer(&txt).unwrap();
    assert_eq!(j.titulo, "Nuevo de aqui");
    assert_eq!(j.etiquetas, vec!["obra", "tesis"]);
    assert!(j.etiquetas_auto.is_empty(), "la quitada aqui no vuelve");
    assert_eq!(
        j.resto.get("campoNuevo"),
        Some(&serde_json::json!(7)),
        "lo desconocido de alli se queda"
    );
}

#[test]
fn lo_que_llega_y_ya_lo_trae_todo_no_se_toca() {
    let base = l("f", "Uno");
    let llega = Leccion {
        tocada: 5000,
        titulo: "Dos".into(),
        etiquetas: vec![],
        ..base.clone()
    };
    let aqui = Leccion {
        etiquetas: vec!["vieja".into()],
        ..base
    };
    // Lo que llega es la version siguiente: manda, y la etiqueta que alli se
    // quito no resucita.
    assert_eq!(fusion::al_llegar(&aqui.escribir(), &llega.escribir()), None);
    // Caso negativo: lo que no es una leccion no se fusiona.
    assert_eq!(fusion::al_llegar("roto", &llega.escribir()), None);
    let otra = l("otra", "x");
    assert_eq!(fusion::al_llegar(&otra.escribir(), &llega.escribir()), None);
}

// ---------------------------------------------------------- v2 (lo del PC)

#[test]
fn el_repaso_de_tres_botones_dice_cuando_vuelve() {
    use crate::leccion::Nota;
    // Caja 3 (14 dias): recordar la lleva a 30, a medias a 7, olvidar a 1.
    let x = Leccion {
        caja: 3,
        ..l("a", "Sellar grietas")
    };
    assert_eq!(Repaso::dias_hasta(&x, Nota::Recordaba), 30);
    assert_eq!(Repaso::dias_hasta(&x, Nota::AMedias), 7);
    assert_eq!(Repaso::dias_hasta(&x, Nota::Olvide), 1);
    let r = Repaso::calificar(&x, Nota::AMedias, 10 * DIA);
    assert_eq!((r.caja, r.repasar), (2, 17 * DIA));
    // Solo cambian caja y repasar: el JSON es el mismo que escribe el movil.
    let antes = x.a_valor();
    let despues = r.a_valor();
    let distintos: Vec<&String> = antes
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| antes[k.as_str()] != despues[k.as_str()])
        .collect();
    assert_eq!(distintos, vec!["caja", "repasar"]);
    // Caso negativo: a medias en la primera caja no baja de 0 ni de un dia.
    let nueva = l("b", "x");
    assert_eq!(Repaso::a_medias(&nueva, 0).caja, 0);
    assert_eq!(Repaso::dias_hasta(&nueva, Nota::AMedias), 1);
}

#[test]
fn la_gravedad_se_propone_por_lo_que_cuenta() {
    use crate::etiquetador::proponer_gravedad;
    assert_eq!(
        proponer_gravedad("La escalera del sótano resbala con el polvo de yeso", None),
        2
    );
    assert_eq!(
        proponer_gravedad("Casi hay un accidente con la amoladora", None),
        3
    );
    assert_eq!(proponer_gravedad("Algo cualquiera", Some(TIPO_ERROR)), 2);
    // Caso negativo: una frase neutra se queda en leve.
    assert_eq!(
        proponer_gravedad("Revisar la escala antes de imprimir", None),
        1
    );
    // Ni «malo» por «mal» ni «gravedad» por «grave»: palabras enteras.
    assert_eq!(proponer_gravedad("La gravedad del asunto es baja", None), 1);
}

#[test]
fn la_barra_rapida_rellena_area_gravedad_y_proyecto() {
    use crate::rapida;
    let proyectos = vec![
        ("f1".to_string(), "Obra Miraflores".to_string()),
        ("f2".to_string(), "Tesis".to_string()),
    ];
    let previa = con(
        "p",
        "Sellar grietas del muro antes de pintar",
        &[],
        "Construcción",
        0,
    );
    let indices = vec![Indice::nuevo(previa)];
    let de_quien = |id: &str| (id == "p").then(|| "f1".to_string());
    let r = rapida::rellenar(
        "La escalera del sótano en Miraflores resbala con el polvo de yeso; barrer antes de bajar material",
        &etiquetador::Aprendido::default(),
        &indices,
        &proyectos,
        &de_quien,
    );
    assert_eq!(r.gravedad, 2);
    assert_eq!(
        r.proyecto.as_deref(),
        Some("f1"),
        "el nombre sale en la frase"
    );
    // Sin nombre, el de la que mas se parece.
    let r2 = rapida::rellenar(
        "Pintar el muro con grietas sin sellar",
        &Default::default(),
        &indices,
        &proyectos,
        &de_quien,
    );
    assert_eq!(r2.proyecto.as_deref(), Some("f1"));
    // Caso negativo: nada parecido ni nombrado, sin proyecto.
    let r3 = rapida::rellenar(
        "Dormir antes del examen",
        &Default::default(),
        &indices,
        &proyectos,
        &de_quien,
    );
    assert_eq!(r3.proyecto, None);
    assert_eq!(
        rapida::rellenar("  ", &Default::default(), &indices, &proyectos, &de_quien).gravedad,
        1
    );
}

#[test]
fn la_barra_rapida_reparte_la_frase_en_sus_campos() {
    use crate::rapida;
    let frase = "pasó que pintamos sin sellar porque había prisa, la próxima vez compro el sellador con la pintura #obra";
    let p = etiquetador::proponer(frase, &Default::default(), &[]);
    let x = rapida::leccion(frase, "id1", 500, &p, Some("Trabajo"), 2);
    assert_eq!(x.que_paso, "Pintamos sin sellar");
    assert_eq!(x.por_que, "Había prisa");
    assert_eq!(x.titulo, "Compro el sellador con la pintura");
    assert_eq!(x.proxima, "", "si hace de titulo no se repite");
    assert_eq!(x.area, "Trabajo", "lo cambiado a mano gana a lo propuesto");
    assert_eq!(x.gravedad, 2);
    assert_eq!(x.etiquetas, vec!["obra"]);
    assert!(!x.etiquetas_auto.contains(&"obra".to_string()));
    assert_eq!((x.id.as_str(), x.creada), ("id1", 500));
    // Caso negativo: una gravedad fuera de 1..=3 no se guarda tal cual.
    assert_eq!(rapida::leccion("x y", "i", 1, &p, None, 9).gravedad, 3);
}
