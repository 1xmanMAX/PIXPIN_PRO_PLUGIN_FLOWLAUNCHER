//! Pruebas sobre una carpeta de datos de mentira (con todas las clases de
//! mensaje y dos listas de tareas) y sobre el dialogo JSON-RPC con Flow.

use crate::datos::Datos;
use crate::pedido::{Envio, Mensajero};
use crate::resultados::{Accion, Contexto, Resultado, resultados};
use crate::rpc::{Plugin, bucle};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

/// Una raiz de datos de PixPin, nueva para cada prueba.
fn raiz(etiqueta: &str) -> PathBuf {
    let r = std::env::temp_dir().join(format!(
        "pixpin-lanzador-pruebas-{etiqueta}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&r);
    let p = r.join("proyectos");
    fs::create_dir_all(&p).unwrap();
    fs::write(
        p.join("indice.json"),
        json!({ "proyectos": [
            { "id": "G1", "nombre": "nombre raro del movil", "tocado": 3000, "hojas": 0, "guardados": true },
            { "id": "P1", "nombre": "Gestión de proyectos", "tocado": 5000, "hojas": 2 },
            { "id": "P2", "nombre": "Thesis", "tocado": 4000, "hojas": 0, "campoNuevo": [1, 2] },
            { "id": "P3", "nombre": "Sin carpeta", "tocado": 9000 },
            { "id": "PX", "nombre": "Tirado", "tocado": 9000, "papelera": true },
            { "id": "P4", "nombre": "Chat de clase", "tocado": 100 }
        ]})
        .to_string(),
    )
    .unwrap();
    for d in [
        "G1/android/guardados",
        "P1/archivos",
        "P1/notas",
        "P2",
        "PX",
        "P4",
    ] {
        fs::create_dir_all(p.join(d)).unwrap();
    }
    fs::write(p.join("G1/android/guardados/foto.jpg"), b"jpg").unwrap();
    fs::write(p.join("G1/android/guardados/voz_1.m4a"), b"m4a").unwrap();
    fs::write(p.join("P1/archivos/Objetivos.md"), b"# Objetivos").unwrap();
    fs::write(
        p.join("P1/notas/suelta.md"),
        "\u{feff}\n# Apuntes sueltos\n\ntexto",
    )
    .unwrap();
    // La nota de verdad del mensaje n2 (por su referencia), una foto y un audio.
    fs::write(p.join("P1/notas/zacta.md"), "# Acta\n\nreunión").unwrap();
    fs::write(p.join("P1/archivos/croquis.png"), b"png").unwrap();
    fs::write(p.join("P1/archivos/v.m4a"), b"m4a").unwrap();
    // El icono de «md» ya lo pinto la app; el de «pdf», no.
    fs::create_dir_all(r.join("cache/iconos-de-extension")).unwrap();
    fs::write(r.join("cache/iconos-de-extension/md.png"), b"png").unwrap();
    fs::write(p.join("P4/guardados.jsonl"), "").unwrap();
    let lineas = |v: &[Value]| {
        v.iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let g1 = lineas(&[
        json!({ "id": "n1", "cuando": 100, "clase": "NOTA", "texto": "# Idea brillante\n\ncomprar pan integral", "nombre": "" }),
        json!({ "id": "i1", "cuando": 101, "clase": "IMAGEN", "ruta": "pixpin:files/guardados/foto.jpg", "nombre": "foto.jpg" }),
        json!({ "id": "v1", "cuando": 102, "clase": "VOZ", "ruta": "pixpin:files/guardados/voz_1.m4a", "nombre": "Clase de cálculo", "transcripcion": "integrales dobles" }),
        json!({ "id": "v2", "cuando": 103, "clase": "VOZ", "ruta": "pixpin:files/guardados/no_esta.m4a", "nombre": "" }),
        json!({ "id": "t1", "cuando": 104, "clase": "MINIAPP", "miniapp": "tareas", "texto": "# Compra\n\n- [ ] pan\n- [x] leche\n- [ ] huevos" }),
        json!({ "id": "ib1", "cuando": 105, "clase": "IMAGEN", "nombre": "buzon.png", "enBuzon": true }),
    ]) + "\n{roto\n";
    fs::write(p.join("G1/guardados.jsonl"), g1).unwrap();
    let p1 = lineas(&[
        json!({ "id": "d1", "cuando": 200, "clase": "DIBUJO", "referencia": "dib-1", "nombre": "Lienzo" }),
        json!({ "id": "pg1", "cuando": 201, "clase": "PAGINA", "referencia": "pag-1", "nombre": "Página uno" }),
        json!({ "id": "a1", "cuando": 202, "clase": "ARCHIVO", "ruta": "archivos/Objetivos.md", "nombre": "Objetivos e Indicadores.md" }),
        json!({ "id": "a2", "cuando": 203, "clase": "ARCHIVO", "ruta": "/storage/emulated/0/x.pdf", "nombre": "Remoto.pdf" }),
        json!({ "id": "pp1", "cuando": 204, "clase": "PROYECTO", "nombre": "otro", "referencia": "P2" }),
        json!({ "id": "t2", "cuando": 205, "clase": "MINIAPP", "miniapp": "tareas", "texto": "# Compra\n- [ ] tornillos" }),
        json!({ "id": "m3", "cuando": 206, "clase": "MINIAPP", "miniapp": "contador", "texto": "# Vasos\nvalor: 3" }),
        json!({ "id": "n2", "cuando": 207, "clase": "NOTA", "texto": "# Acta\n\nreunión", "referencia": "zacta" }),
        json!({ "id": "x1", "cuando": 208, "clase": "NOTA", "texto": "hola equipo, mañana a las 9\nsegunda línea" }),
        json!({ "id": "i2", "cuando": 209, "clase": "IMAGEN", "ruta": "archivos/croquis.png", "nombre": "croquis.png" }),
        json!({ "id": "v3", "cuando": 150, "clase": "VOZ", "ruta": "archivos/v.m4a", "nombre": "",
            "transcripcion": "revisar el presupuesto", "duracionMs": 75000 }),
        json!({ "id": "b9", "cuando": 999, "clase": "NOTA", "texto": "del buzon", "enBuzon": true, "duracionMs": null }),
    ]);
    fs::write(p.join("P1/guardados.jsonl"), p1).unwrap();
    fs::write(
        p.join("P1/proyecto.json"),
        json!({ "id": "P1", "hojas": [
            { "id": "h1", "nombre": "Plano de la casa", "dibujo": "dib-1" },
            { "id": "h2", "nombre": "Hoja suelta", "dibujo": "dib-2" },
            { "id": "h3", "nombre": "", "dibujo": null, "pagina": 0 }
        ]})
        .to_string(),
    )
    .unwrap();
    fs::write(p.join("P2/guardados.jsonl"), "").unwrap();
    r
}

fn ctx() -> Contexto {
    Contexto::con("pp ", 1000)
}

fn buscar(r: &Path, q: &str) -> Vec<Resultado> {
    let mut d = Datos::nuevo(r.to_path_buf());
    resultados(&d.proyectos(), q, &ctx())
}

fn pedido_de(r: &Resultado) -> &Value {
    match &r.accion {
        Accion::Pedido(p) | Accion::PedirYSeguir { pedido: p, .. } => p,
        otra => panic!("no es un pedido: {otra:?}"),
    }
}

// --- Leer y buscar ---------------------------------------------------------

#[test]
fn lee_los_proyectos_sin_papelera_ni_carpetas_que_faltan() {
    let r = raiz("proyectos");
    let mut d = Datos::nuevo(r.clone());
    let ps = d.proyectos();
    let ids: Vec<&str> = ps.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["G1", "P1", "P2", "P4"]);
    assert_eq!(ps[0].nombre, "Mensajes guardados");
    // La linea rota y la del buzon no cuentan.
    assert_eq!(ps[0].mensajes.len(), 5);
    assert_eq!(ps[1].hojas_json.len(), 3);
    assert_eq!(ps[1].notas_md[0].titulo, "Apuntes sueltos");
}

#[test]
fn la_cache_se_pone_al_dia_cuando_cambia_el_fichero() {
    let r = raiz("cache");
    let mut d = Datos::nuevo(r.clone());
    assert_eq!(d.proyectos()[2].mensajes.len(), 0);
    // Se adelanta la cache: dura mientras el fichero no cambie.
    assert!(d.retocar("G1", "t1", |t| crate::datos::con_tarea_anadida(t, "yogur")));
    assert!(
        d.proyectos()[0]
            .mensajes
            .iter()
            .any(|m| m.texto().contains("yogur"))
    );
    std::thread::sleep(std::time::Duration::from_millis(30));
    fs::write(
        r.join("proyectos/P2/guardados.jsonl"),
        json!({ "id": "x", "cuando": 1, "clase": "NOTA", "texto": "nueva" }).to_string(),
    )
    .unwrap();
    let ps = d.proyectos();
    assert_eq!(ps[2].mensajes.len(), 1);
    assert!(
        ps[0].mensajes.iter().any(|m| m.texto().contains("yogur")),
        "G1 no cambio: sigue lo adelantado"
    );
}

#[test]
fn un_archivo_se_abre_por_su_ruta_real() {
    let r = raiz("archivo");
    let v = buscar(&r, "objetivos");
    assert_eq!(v[0].titulo, "Objetivos e Indicadores.md");
    assert!(
        v[0].subtitulo.starts_with("Archivo · Gestión de proyectos"),
        "{}",
        v[0].subtitulo
    );
    let p = pedido_de(&v[0]);
    assert_eq!(p["accion"], "abrir");
    assert_eq!(p["que"]["tipo"], "fichero");
    assert_eq!(
        p["que"]["ruta"],
        r.join("proyectos")
            .join("P1")
            .join("archivos")
            .join("Objetivos.md")
            .to_string_lossy()
            .as_ref()
    );
}

#[test]
fn lo_que_no_esta_en_el_equipo_abre_su_mensaje() {
    let r = raiz("remoto");
    let v = buscar(&r, "remoto");
    assert!(v[0].subtitulo.contains("no está en este equipo"));
    assert_eq!(
        pedido_de(&v[0])["que"],
        json!({ "tipo": "mensaje", "proyecto": "P1", "codigo": "a2" })
    );
}

#[test]
fn lienzos_por_el_nombre_de_su_hoja_y_hojas_sin_mensaje() {
    let r = raiz("lienzos");
    let v = buscar(&r, "plano");
    assert_eq!(v[0].titulo, "Plano de la casa");
    assert_eq!(
        pedido_de(&v[0])["que"],
        json!({ "tipo": "hoja", "proyecto": "P1", "referencia": "dib-1" })
    );
    let v = buscar(&r, "hoja suelta");
    assert_eq!(pedido_de(&v[0])["que"]["referencia"], "dib-2");
    let v = buscar(&r, "pagina uno");
    assert_eq!(v[0].titulo, "Página uno");
    assert!(v[0].subtitulo.starts_with("Hoja"));
}

#[test]
fn notas_del_chat_y_md_sueltas() {
    let r = raiz("notas");
    let v = buscar(&r, "idea");
    assert_eq!(v[0].titulo, "Idea brillante");
    assert_eq!(
        pedido_de(&v[0])["que"],
        json!({ "tipo": "nota", "proyecto": null, "codigo": "n1" })
    );
    // Por el texto de dentro tambien, pero por debajo.
    let v = buscar(&r, "integral");
    assert!(v.iter().any(|x| x.titulo == "Idea brillante"));
    let v = buscar(&r, "apuntes");
    assert_eq!(pedido_de(&v[0])["que"]["tipo"], "fichero");
}

#[test]
fn audios_por_nombre_sin_tildes_y_por_transcripcion() {
    let r = raiz("audios");
    let v = buscar(&r, "calculo");
    assert_eq!(v[0].titulo, "Clase de cálculo");
    assert!(v[0].subtitulo.starts_with("Audio · Mensajes guardados"));
    // Intro lo hace sonar en el reproductor flotante, no lo abre.
    let p = pedido_de(&v[0]);
    assert_eq!(p["accion"], "reproducir");
    assert!(p["ruta"].as_str().unwrap().ends_with("voz_1.m4a"));
    assert_eq!(p["titulo"], "Clase de cálculo");
    let v = buscar(&r, "integrales dobles");
    assert_eq!(v[0].titulo, "Clase de cálculo");
    let v = buscar(&r, "sin nombre");
    assert!(v[0].titulo.starts_with("Nota de voz sin nombre"));
}

#[test]
fn lo_del_buzon_y_los_proyectos_enlazados_no_salen() {
    let r = raiz("buzon");
    assert!(buscar(&r, "buzon").is_empty());
    assert!(!buscar(&r, "otro").iter().any(|x| x.titulo == "otro"));
    assert!(!buscar(&r, "vasos").iter().any(|x| x.titulo == "Vasos"));
}

#[test]
fn vacio_da_las_funciones_y_los_proyectos_recientes() {
    let r = raiz("vacio");
    let v = buscar(&r, "");
    let titulos: Vec<&str> = v.iter().map(|x| x.titulo.as_str()).collect();
    assert_eq!(
        titulos,
        [
            "Tareas",
            "Lienzo nuevo",
            "Nota nueva",
            "Grabar audio",
            "Añadir a proyecto",
            "Nueva lección",
            "Lecciones aprendidas",
            "Repasar hoy",
            "Capturar zona",
            "Capturas",
            "Galería de capturas",
            "Última captura",
            "Enviar al móvil",
            "Abrir PixPin",
            "Gestión de proyectos",
            "Thesis",
            "Mensajes guardados",
            "Chat de clase"
        ]
    );
    assert_eq!(v[0].accion, Accion::Consulta("pp tareas ".into()));
    assert_eq!(v[5].accion, Accion::Consulta("pp a ".into()));
    assert_eq!(v[6].accion, Accion::Consulta("pp lecciones ".into()));
    assert_eq!(v[7].accion, Accion::Consulta("pp repasar ".into()));
    assert_eq!(pedido_de(&v[1])["accion"], "lienzo_nuevo");
    assert!(
        v[3].subtitulo.contains("micrófono flotante"),
        "{}",
        v[3].subtitulo
    );
    // Un proyecto ya no abre la app: entra en su chat.
    assert_eq!(
        v[14].accion,
        Accion::Consulta("pp Gestión de proyectos > ".into())
    );
    assert_eq!(
        v[14].autocompletar.as_deref(),
        Some("pp Gestión de proyectos > ")
    );
    assert_eq!(
        v[16].accion,
        Accion::Consulta("pp Mensajes guardados > ".into())
    );
}

#[test]
fn caso_negativo_el_chat_no_se_ofrece_y_lo_que_no_se_encuentra_no_va_a_guardados() {
    let r = raiz("sin-chat");
    let es_chat = |x: &Resultado| {
        x.titulo == "Chat"
            || x.clave.as_deref() == Some("funcion/chat")
            || matches!(&x.accion, Accion::Pedido(p) if p["accion"] == "chat")
    };
    // Ni con `p` a secas, ni buscando su nombre, ni con algo que no existe.
    for q in [
        "",
        "chat",
        "cha",
        "mensaje",
        "escribir",
        "message",
        "hola que tal",
        "zzzz",
    ] {
        let v = buscar(&r, q);
        assert!(!v.iter().any(es_chat), "«{q}»: {:?}", titulos(&v));
    }
    // «mensaje hola» o «escribir informe» ya no son verbos: se busca.
    assert!(buscar(&r, "escribir informe").is_empty());
    // Escribirlo a proposito sigue valiendo.
    let v = buscar(&r, "chat hola");
    assert_eq!(pedido_de(&v[0])["accion"], "chat");
}

#[test]
fn una_funcion_por_su_nombre_o_un_alias_sale_la_primera() {
    let r = raiz("funciones");
    assert_eq!(buscar(&r, "tar")[0].titulo, "Tareas");
    assert_eq!(buscar(&r, "canv")[0].titulo, "Lienzo nuevo");
    assert_eq!(buscar(&r, "gest")[0].titulo, "Gestión de proyectos");
    assert!(buscar(&r, "x").len() <= crate::resultados::MAXIMO);
}

#[test]
fn verbos_con_texto_y_proyecto() {
    let r = raiz("verbos");
    let v = buscar(&r, "chat hola que tal @gest");
    assert_eq!(v[0].titulo, "Escribir en el chat: hola que tal");
    assert_eq!(
        *pedido_de(&v[0]),
        json!({ "pixpin": 1, "accion": "chat", "texto": "hola que tal", "proyecto": "P1" })
    );
    // `@gest` no es el nombre entero: se sugieren proyectos para completarlo.
    assert!(
        v.iter()
            .any(|x| x.accion
                == Accion::Consulta("pp chat hola que tal @Gestión de proyectos".into()))
    );
    let v = buscar(&r, "chat hola @Gestión de proyectos");
    assert_eq!(v.len(), 1, "con el nombre entero ya no se sugiere nada");

    let v = buscar(&r, "nota idea feliz");
    assert_eq!(
        *pedido_de(&v[0]),
        json!({ "pixpin": 1, "accion": "nota_nueva", "texto": "idea feliz", "proyecto": null })
    );
    let v = buscar(&r, "canvas plano");
    assert_eq!(
        *pedido_de(&v[0]),
        json!({ "pixpin": 1, "accion": "lienzo_nuevo", "nombre": "plano", "proyecto": null })
    );
    assert_eq!(
        v[1].titulo, "Plano de la casa",
        "debajo, los lienzos que ya se llaman asi"
    );
    let v = buscar(&r, "grabar Clase 4 @thesis");
    assert_eq!(
        *pedido_de(&v[0]),
        json!({ "pixpin": 1, "accion": "grabar", "nombre": "Clase 4", "proyecto": "P2" })
    );
    let v = buscar(&r, "grabar");
    assert_eq!(v[0].accion, Accion::Consulta("pp grabar ".into()));
    let v = buscar(&r, "chat @nadaparecido");
    assert!(v[0].subtitulo.contains("no hay ningún proyecto"));
}

#[test]
fn las_listas_de_tareas_y_una_nueva() {
    let r = raiz("listas");
    let v = buscar(&r, "tareas");
    // Sin Inbox: la ventana de Tareas y despues las listas.
    assert_eq!(v.len(), 3);
    assert_eq!(
        *pedido_de(&v[0]),
        json!({ "pixpin": 1, "accion": "ventana", "cual": "tareas" })
    );
    let v = &v[1..];
    // La mas reciente primero; las dos se llaman igual y se distinguen por proyecto.
    assert_eq!(
        v[0].accion,
        Accion::Consulta("pp tareas Compra · Gestión de proyectos > ".into())
    );
    assert_eq!(
        v[1].accion,
        Accion::Consulta("pp tareas Compra · Mensajes guardados > ".into())
    );
    assert_eq!(
        v[1].subtitulo,
        "Lista de tareas · Mensajes guardados · 2 pendientes de 3"
    );
    // Tab completa la lista; la barra dice lo hecho (1 de 3).
    assert_eq!(
        v[1].autocompletar.as_deref(),
        Some("pp tareas Compra · Mensajes guardados > ")
    );
    assert_eq!(v[1].progreso, Some(33));
    let v = buscar(&r, "todo Viaje");
    assert_eq!(v[0].titulo, "Nueva lista: Viaje");
    assert_eq!(
        *pedido_de(&v[0]),
        json!({ "pixpin": 1, "accion": "lista_nueva", "titulo": "Viaje", "proyecto": null })
    );
    let v = buscar(&r, "tareas compra");
    assert!(
        !v.iter().any(|x| x.titulo.starts_with("Nueva lista")),
        "ya existe una que se llama asi"
    );
}

#[test]
fn dentro_de_una_lista_marcar_y_anadir() {
    let r = raiz("lista");
    let v = buscar(&r, "tareas Compra · Mensajes guardados > ");
    let titulos: Vec<&str> = v.iter().map(|x| x.titulo.as_str()).collect();
    assert_eq!(titulos, ["☐ pan", "☐ huevos", "☑ leche"]);
    assert_eq!(
        *pedido_de(&v[1]),
        json!({ "pixpin": 1, "accion": "marcar_tarea", "proyecto": null, "codigo": "t1", "indice": 2, "hecha": true })
    );
    assert_eq!(pedido_de(&v[2])["hecha"], false);
    let Accion::PedirYSeguir { consulta, .. } = &v[0].accion else {
        panic!()
    };
    assert_eq!(consulta, "pp tareas Compra · Mensajes guardados > ");

    let v = buscar(&r, "tareas Compra · Mensajes guardados > yogur");
    assert_eq!(v[0].titulo, "Añadir tarea: yogur");
    assert_eq!(
        v[0].accion,
        Accion::PedirYSeguir {
            pedido: json!({ "pixpin": 1, "accion": "anadir_tarea", "texto": "yogur", "proyecto": null, "codigo": "t1" }),
            consulta: "pp tareas Compra · Mensajes guardados > ".into(),
        }
    );
    // Lo que ya existe se puede anadir otra vez (avisando), y el filtro
    // deja solo esa debajo.
    let v = buscar(&r, "tareas compra · mensajes guardados > Pan");
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].titulo, "Añadir tarea: Pan");
    assert!(v[0].subtitulo.starts_with("Ya hay una igual"));
    assert_eq!(v[1].titulo, "☐ pan");
    // Con el titulo solo vale tambien (la de Gestion, que es la mas nueva).
    let v = buscar(&r, "tareas compra > ");
    assert_eq!(v[0].titulo, "☐ tornillos");
    assert_eq!(
        buscar(&r, "tareas nada de nada > x")[0].accion,
        Accion::Consulta("pp tareas ".into())
    );
}

// --- El dialogo con Flow ---------------------------------------------------

#[derive(Default)]
struct DeMentira {
    pedidos: Vec<Value>,
    /// Los que no arrancan la app (`iconos`).
    avisos: Vec<Value>,
    /// Lo abierto con el programa de Windows.
    lanzados: Vec<String>,
    falla: bool,
}

impl Mensajero for DeMentira {
    fn enviar(&mut self, json: &str) -> Envio {
        self.pedidos.push(serde_json::from_str(json).unwrap());
        if self.falla {
            Envio::SinApp("prueba".into())
        } else {
            Envio::Aceptado
        }
    }
    fn avisar(&mut self, json: &str) -> Envio {
        self.avisos.push(serde_json::from_str(json).unwrap());
        Envio::Aceptado
    }
    fn abrir_con_windows(&mut self, ruta: &str) -> bool {
        self.lanzados.push(ruta.to_string());
        true
    }
}

fn plugin(r: &Path) -> Plugin<DeMentira, Vec<u8>> {
    let mut p = Plugin::nuevo(
        Datos::nuevo(r.to_path_buf()),
        DeMentira::default(),
        Vec::new(),
        "icono.png".into(),
    );
    p.espera_lista = std::time::Duration::ZERO;
    p
}

/// Las lineas que el plugin escribio, como JSON.
fn salida(p: &Plugin<DeMentira, Vec<u8>>) -> Vec<Value> {
    String::from_utf8(p.salida().clone())
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("cada linea es un JSON"))
        .collect()
}

fn consulta(id: u64, search: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": "query",
        "params": [{ "rawQuery": format!("pp {search}"), "search": search, "actionKeyword": "pp", "isReQuery": false }, {}] })
    .to_string()
}

#[test]
fn initialize_y_query_con_la_forma_de_flow() {
    let r = raiz("rpc-query");
    let ajustes = r.join("ajustes");
    let mut p = plugin(&r);
    let entrada = format!(
        "{}\r\n{}\n",
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": [{ "currentPluginMetadata": { "pluginDirectory": r.to_string_lossy(), "pluginSettingsDirectoryPath": ajustes.to_string_lossy() } }] }),
        consulta(2, "objetivos")
    );
    bucle(&mut p, entrada.as_bytes());
    let s = salida(&p);
    assert_eq!(s.len(), 2);
    assert_eq!(
        s[0],
        json!({ "jsonrpc": "2.0", "id": 1, "result": { "hide": false } })
    );
    assert_eq!(s[1]["id"], 2);
    let res = &s[1]["result"];
    assert_eq!(res["settingsChange"], json!({}));
    assert_eq!(res["debugMessage"], "");
    let primero = &res["result"][0];
    assert_eq!(primero["title"], "Objetivos e Indicadores.md");
    // Su icono es el de su extension, que la app ya pinto: sin glifo.
    assert!(
        primero["icoPath"]
            .as_str()
            .unwrap()
            .ends_with(r"iconos-de-extension\md.png"),
        "{}",
        primero["icoPath"]
    );
    assert!(primero.get("glyph").is_none());
    assert_eq!(primero["jsonRPCAction"]["method"], "pedir");
    assert_eq!(primero["jsonRPCAction"]["dontHideAfterAction"], false);
    assert!(
        primero["subTitle"].is_string()
            && primero["score"].is_i64()
            && primero["contextData"].is_object()
    );
    // El registro va a un fichero de la carpeta de ajustes.
    assert!(ajustes.join("pixpin-lanzador.log").is_file());
}

#[test]
fn una_accion_llega_envuelta_y_manda_el_pedido() {
    let r = raiz("rpc-pedir");
    let mut p = plugin(&r);
    let pedido =
        json!({ "pixpin": 1, "accion": "abrir", "que": { "tipo": "proyecto", "proyecto": "P1" } });
    p.atender(
        &json!({ "jsonrpc": "2.0", "id": 7, "method": "pedir", "params": [[pedido]] }).to_string(),
    );
    assert_eq!(p.mensajero.pedidos, vec![pedido]);
    assert_eq!(
        salida(&p),
        vec![json!({ "jsonrpc": "2.0", "id": 7, "result": { "hide": true } })]
    );
}

#[test]
fn si_la_app_no_esta_se_avisa_con_showmsg() {
    let r = raiz("rpc-falla");
    let mut p = plugin(&r);
    p.mensajero.falla = true;
    p.atender(&json!({ "jsonrpc": "2.0", "id": 7, "method": "pedir", "params": [[{ "pixpin": 1, "accion": "ventana_principal" }]] }).to_string());
    let s = salida(&p);
    assert_eq!(s[0]["result"]["hide"], true);
    assert_eq!(s[1]["method"], "ShowMsg");
    assert!(
        s[1]["params"][1]
            .as_str()
            .unwrap()
            .contains("No se pudo abrir PixPin")
    );
}

#[test]
fn marcar_una_tarea_la_ensena_marcada_sin_cerrar_flow() {
    let r = raiz("rpc-marcar");
    let mut p = plugin(&r);
    let q = "tareas Compra · Mensajes guardados > ";
    p.atender(&consulta(1, q));
    let s = salida(&p);
    let pan = &s[0]["result"]["result"][0];
    assert_eq!(pan["title"], "☐ pan");
    assert_eq!(pan["jsonRPCAction"]["dontHideAfterAction"], true);
    // Flow manda la accion con sus parametros envueltos.
    let accion = json!({ "jsonrpc": "2.0", "id": 2, "method": pan["jsonRPCAction"]["method"],
        "params": [pan["jsonRPCAction"]["parameters"]] });
    p.atender(&accion.to_string());
    assert_eq!(p.mensajero.pedidos[0]["accion"], "marcar_tarea");
    let s = salida(&p);
    assert_eq!(s[1]["method"], "ChangeQuery");
    assert_eq!(s[1]["params"], json!([format!("pp {q}"), true]));
    assert_eq!(
        s[2],
        json!({ "jsonrpc": "2.0", "id": 2, "result": { "hide": false } })
    );
    // Flow contesta al ChangeQuery (se ignora) y vuelve a consultar.
    p.atender(&json!({ "jsonrpc": "2.0", "id": s[1]["id"], "result": null }).to_string());
    p.atender(&consulta(3, q));
    let s = salida(&p);
    let titulos: Vec<&str> = s[3]["result"]["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap())
        .collect();
    assert_eq!(titulos, ["☐ huevos", "☑ pan", "☑ leche"]);
}

#[test]
fn anadir_una_tarea_la_ensena_en_la_lista() {
    let r = raiz("rpc-anadir");
    let mut p = plugin(&r);
    p.atender(&consulta(1, "tareas Compra · Mensajes guardados > yogur"));
    let anadir = salida(&p)[0]["result"]["result"][0]["jsonRPCAction"].clone();
    p.atender(&json!({ "jsonrpc": "2.0", "id": 2, "method": anadir["method"], "params": [anadir["parameters"]] }).to_string());
    assert_eq!(p.mensajero.pedidos[0]["accion"], "anadir_tarea");
    p.atender(&consulta(3, "tareas Compra · Mensajes guardados > "));
    let s = salida(&p);
    let ultima = s.last().unwrap()["result"]["result"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["title"] == "☐ yogur");
    assert!(ultima);
}

#[test]
fn lo_que_no_es_una_peticion_no_se_contesta() {
    let r = raiz("rpc-ruido");
    let mut p = plugin(&r);
    for l in [
        r#"{"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":3}}"#,
        r#"{"jsonrpc":"2.0","id":1000000,"result":null}"#,
        "esto no es json",
        "",
        r#"{"jsonrpc":"2.0","method":"query","params":[{"search":"x"}]}"#,
    ] {
        p.atender(l);
    }
    assert!(salida(&p).is_empty());
    p.atender(r#"{"jsonrpc":"2.0","id":9,"method":"inventado","params":[]}"#);
    assert_eq!(salida(&p)[0]["error"]["code"], -32601);
}

#[test]
fn menu_contextual_y_cerrar() {
    let r = raiz("rpc-menu");
    let mut p = plugin(&r);
    p.atender(r#"{"jsonrpc":"2.0","id":1,"method":"context_menu","params":[null]}"#);
    assert_eq!(salida(&p)[0]["result"]["result"], json!([]));
    p.atender(&consulta(2, "calculo"));
    let ctx = salida(&p)[1]["result"]["result"][0]["contextData"].clone();
    p.atender(
        &json!({ "jsonrpc": "2.0", "id": 3, "method": "context_menu", "params": [ctx] })
            .to_string(),
    );
    let s = salida(&p);
    let menu = s[2]["result"]["result"].as_array().unwrap();
    let titulos: Vec<&str> = menu.iter().map(|r| r["title"].as_str().unwrap()).collect();
    assert_eq!(
        titulos,
        [
            "Abrir",
            "Ver en el chat",
            "Abrir con el programa de Windows",
            "Mostrar en la carpeta",
            "Copiar texto",
            "Copiar ruta"
        ]
    );
    assert_eq!(
        menu[1]["jsonRPCAction"]["parameters"][0]["que"],
        json!({ "tipo": "mensaje", "proyecto": null, "codigo": "v1" })
    );
    let entrada =
        "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"close\"}\n".to_string() + &consulta(5, "x");
    bucle(&mut p, entrada.as_bytes());
    assert!(p.terminado);
    assert_eq!(salida(&p).len(), 4, "tras close no se atiende nada mas");
}

#[test]
fn una_lista_nueva_espera_y_entra_en_ella() {
    let r = raiz("rpc-nueva");
    let mut p = plugin(&r);
    p.atender(&consulta(1, "tareas Viaje"));
    let a = salida(&p)[0]["result"]["result"][0]["jsonRPCAction"].clone();
    p.atender(
        &json!({ "jsonrpc": "2.0", "id": 2, "method": a["method"], "params": [a["parameters"]] })
            .to_string(),
    );
    assert_eq!(p.mensajero.pedidos[0]["accion"], "lista_nueva");
    let s = salida(&p);
    assert_eq!(s[1]["params"], json!(["pp tareas Viaje > ", true]));
}

// --- El chat de un proyecto, los iconos y el menu ----------------------------

/// Como `buscar`, pero mirando los iconos de la carpeta de la prueba.
fn buscar_con_iconos(r: &Path, q: &str) -> (Vec<Resultado>, Vec<String>) {
    let mut d = Datos::nuevo(r.to_path_buf());
    let mut c = ctx();
    c.iconos = Some(crate::iconos::carpeta(r));
    let v = resultados(&d.proyectos(), q, &c);
    let faltan = c.extensiones_que_faltan();
    (v, faltan)
}

fn titulos(v: &[Resultado]) -> Vec<&str> {
    v.iter().map(|x| x.titulo.as_str()).collect()
}

/// El menu de un resultado.
fn menu_de(r: &Resultado) -> Vec<Resultado> {
    crate::resultados::menu(r.contexto.as_ref().expect("lleva contextData"), &ctx())
}

#[test]
fn el_chat_de_un_proyecto_de_lo_ultimo_a_lo_primero_con_todas_las_clases() {
    let r = raiz("chat");
    let (mut v, faltan) = buscar_con_iconos(&r, "Gestión de proyectos > ");
    // La segunda fila fija: el recuadro para soltar archivos en este proyecto.
    let arrastrar = v.remove(1);
    assert_eq!(arrastrar.titulo, "Añadir arrastrando (recuadro flotante)");
    assert_eq!(
        *pedido_de(&arrastrar),
        json!({ "pixpin": 1, "accion": "soltar", "proyecto": "P1" })
    );
    assert_eq!(
        titulos(&v),
        [
            "Abrir proyecto en la app",
            "croquis.png",
            "hola equipo, mañana a las 9",
            "Acta",
            "Vasos",
            "Compra",
            "otro",
            "Remoto.pdf",
            "Objetivos e Indicadores.md",
            "Página uno",
            "Plano de la casa",
            "revisar el presupuesto",
        ]
    );
    assert_eq!(
        *pedido_de(&v[0]),
        json!({ "pixpin": 1, "accion": "abrir", "que": { "tipo": "proyecto", "proyecto": "P1" } })
    );
    // Foto: su miniatura, sin glifo.
    let foto = r
        .join("proyectos")
        .join("P1")
        .join("archivos")
        .join("croquis.png");
    assert_eq!(v[1].icono.as_deref(), Some(foto.to_string_lossy().as_ref()));
    assert!(v[1].subtitulo.starts_with("Foto · "), "{}", v[1].subtitulo);
    // Un texto: su primera linea, «Mensaje · <hora>», y abre el mensaje.
    assert!(
        v[2].subtitulo.starts_with("Mensaje · "),
        "{}",
        v[2].subtitulo
    );
    assert_eq!(
        pedido_de(&v[2])["que"],
        json!({ "tipo": "mensaje", "proyecto": "P1", "codigo": "x1" })
    );
    // Una nota con su .md: al editor de notas.
    assert_eq!(
        pedido_de(&v[3])["que"],
        json!({ "tipo": "nota", "proyecto": "P1", "codigo": "n2" })
    );
    // Otra mini-app: su mensaje; una lista: entra en ella.
    assert_eq!(pedido_de(&v[4])["que"]["codigo"], "m3");
    assert_eq!(
        v[5].accion,
        Accion::Consulta("pp tareas Compra · Gestión de proyectos > ".into())
    );
    assert!(
        v[5].subtitulo
            .starts_with("Lista de tareas · 1 pendiente de 1"),
        "{}",
        v[5].subtitulo
    );
    // El acceso directo a otro proyecto entra en su chat.
    assert_eq!(v[6].accion, Accion::Consulta("pp Thesis > ".into()));
    // Archivos: el icono de su extension si ya esta; si no, glifo y se pide.
    assert_eq!(v[7].icono, None);
    assert!(
        v[8].icono
            .as_deref()
            .unwrap()
            .ends_with(r"iconos-de-extension\md.png")
    );
    assert_eq!(faltan, ["pdf"]);
    // Lienzos y hojas al lienzo; el audio con lo que dura.
    assert_eq!(
        pedido_de(&v[10])["que"],
        json!({ "tipo": "hoja", "proyecto": "P1", "referencia": "dib-1" })
    );
    assert!(
        v[11].subtitulo.starts_with("Audio · 1:15 · "),
        "{}",
        v[11].subtitulo
    );
    assert!(v[11].icono.is_none(), "los audios, su glifo");
    // Lo del buzon no sale.
    assert!(!titulos(&v).contains(&"del buzon"));
}

#[test]
fn en_el_chat_se_filtra_y_abrir_el_proyecto_baja_al_final() {
    let r = raiz("chat-filtro");
    let v = buscar(&r, "gestion de proyectos > presupuesto");
    assert_eq!(
        titulos(&v),
        [
            "revisar el presupuesto",
            "Escribir en «Gestión de proyectos»: presupuesto",
            "Abrir proyecto en la app",
            "Añadir arrastrando (recuadro flotante)"
        ]
    );
    // Lo que no se encuentra es un mensaje para ESTE chat (no para
    // Mensajes guardados).
    let v = buscar(&r, "Gestión de proyectos > nada de esto");
    assert_eq!(
        titulos(&v),
        [
            "Escribir en «Gestión de proyectos»: nada de esto",
            "Abrir proyecto en la app",
            "Añadir arrastrando (recuadro flotante)"
        ]
    );
    assert_eq!(
        *pedido_de(&v[0]),
        json!({ "pixpin": 1, "accion": "chat", "texto": "nada de esto", "proyecto": "P1" })
    );
    assert_eq!(
        v[0].autocompletar.as_deref(),
        Some("pp Gestión de proyectos > nada de esto")
    );
    // Mensajes guardados por su nombre de siempre, y un trozo del nombre vale.
    let v = buscar(&r, "Mensajes guardados > ");
    assert_eq!(
        v[2].titulo, "Compra",
        "lo mas nuevo de guardados es la lista"
    );
    assert_eq!(
        *pedido_de(&v[1]),
        json!({ "pixpin": 1, "accion": "soltar", "proyecto": null })
    );
    assert_eq!(buscar(&r, "thes > ")[0].titulo, "Abrir proyecto en la app");
    assert!(buscar(&r, "thes > ")[0].subtitulo.starts_with("Thesis"));
    assert!(
        buscar(&r, "zzz > ")[0]
            .titulo
            .starts_with("No encuentro el proyecto")
    );
}

#[test]
fn un_proyecto_que_empieza_por_un_verbo_y_no_choca_con_las_tareas() {
    let r = raiz("chat-verbo");
    let v = buscar(&r, "Chat de clase > ");
    assert_eq!(
        titulos(&v),
        [
            "Abrir proyecto en la app",
            "Añadir arrastrando (recuadro flotante)",
            "El chat está vacío"
        ]
    );
    // «chat hola > adios» sigue siendo escribir en el chat.
    assert_eq!(
        buscar(&r, "chat hola > adios")[0].titulo,
        "Escribir en el chat: hola > adios"
    );
    // y «tareas X > » sigue siendo la lista.
    assert_eq!(buscar(&r, "tareas compra > ")[0].titulo, "☐ tornillos");
}

#[test]
fn el_menu_de_cada_tipo() {
    let r = raiz("menus");
    let (mut v, _) = buscar_con_iconos(&r, "Gestión de proyectos > ");
    // El recuadro lleva el menu del proyecto.
    assert_eq!(menu_de(&v[1])[0].titulo, "Abrir en la app");
    v.remove(1);
    let t = |i: usize| {
        menu_de(&v[i])
            .into_iter()
            .map(|x| x.titulo)
            .collect::<Vec<_>>()
    };
    // El proyecto.
    assert_eq!(
        t(0),
        [
            "Abrir en la app",
            "Ver contenido",
            "Añadir arrastrando (recuadro flotante)",
            "Lienzo nuevo aquí",
            "Nota nueva aquí",
            "Abrir la carpeta"
        ]
    );
    assert_eq!(
        *pedido_de(&menu_de(&v[0])[2]),
        json!({ "pixpin": 1, "accion": "soltar", "proyecto": "P1" })
    );
    // La foto.
    assert_eq!(
        t(1),
        [
            "Abrir",
            "Pinear",
            "Ver en el chat",
            "Abrir con el programa de Windows",
            "Mostrar en la carpeta",
            "Copiar ruta"
        ]
    );
    let m = menu_de(&v[1]);
    let carpeta = r.join("proyectos").join("P1").join("archivos");
    let foto = carpeta.join("croquis.png").to_string_lossy().to_string();
    assert_eq!(m[0].accion, v[1].accion, "Abrir es lo mismo que Intro");
    assert_eq!(
        *pedido_de(&m[1]),
        json!({ "pixpin": 1, "accion": "pinear", "ruta": foto })
    );
    assert_eq!(m[3].accion, Accion::Windows(foto.clone()));
    assert_eq!(
        m[4].accion,
        Accion::Carpeta {
            carpeta: carpeta.to_string_lossy().to_string(),
            fichero: foto.clone()
        }
    );
    assert_eq!(m[5].accion, Accion::Copiar(foto));
    // Un texto: pin por su codigo y copiar el texto entero.
    assert_eq!(
        t(2),
        [
            "Abrir",
            "Pinear",
            "Ver en el chat",
            "Copiar texto",
            "Abrir la carpeta del proyecto"
        ]
    );
    let m = menu_de(&v[2]);
    assert_eq!(
        *pedido_de(&m[1]),
        json!({ "pixpin": 1, "accion": "pinear", "proyecto": "P1", "codigo": "x1" })
    );
    assert_eq!(
        m[3].accion,
        Accion::Copiar("hola equipo, mañana a las 9\nsegunda línea".into())
    );
    // Una lista: Abrir entra en ella.
    assert_eq!(menu_de(&v[5])[0].accion, v[5].accion);
    // Un archivo que no esta en este equipo: nada de Windows ni de pin.
    assert_eq!(
        t(7),
        ["Abrir", "Ver en el chat", "Abrir la carpeta del proyecto"]
    );
    // El lienzo.
    assert_eq!(
        t(10),
        [
            "Abrir",
            "Pinear",
            "Ver en el chat",
            "Abrir la carpeta del proyecto"
        ]
    );
    // El audio: sin pin, con su transcripcion.
    assert_eq!(
        t(11),
        [
            "Abrir",
            "Ver en el chat",
            "Abrir con el programa de Windows",
            "Mostrar en la carpeta",
            "Copiar texto",
            "Copiar ruta"
        ]
    );
}

/// Una peticion del modo v1, en un plugin nuevo (cada una es un proceso):
/// la respuesta, los avisos que dejo pendientes, y lo que mando.
fn v1(r: &Path, peticion: Value) -> (Value, Vec<Value>, DeMentira) {
    let mut p = plugin(r);
    p.diferir_avisos = true;
    let respuesta = crate::rpc::una_vez(&mut p, &peticion);
    let avisos = p.avisos().to_vec();
    p.mandar_avisos();
    (respuesta, avisos, p.mensajero)
}

#[test]
fn modo_v1_consulta_menu_y_acciones() {
    let r = raiz("v1");
    let (q, avisos, m) = v1(
        &r,
        json!({ "method": "query", "parameters": ["Gestión de proyectos > "] }),
    );
    let lista = q["result"].as_array().unwrap();
    assert_eq!(lista[0]["title"], "Abrir proyecto en la app");
    assert_eq!(
        lista[0]["autoCompleteText"], "p Gestión de proyectos > ",
        "en v1, la palabra clave «p»"
    );
    // Los iconos que faltan se piden despues de contestar, y sin arrancar la app.
    assert_eq!(
        avisos,
        vec![json!({ "pixpin": 1, "accion": "iconos", "extensiones": ["pdf"] })]
    );
    assert_eq!(m.avisos.len(), 1);
    assert!(m.pedidos.is_empty());
    // El menu de la foto.
    let (menu, _, _) = v1(
        &r,
        json!({ "method": "context_menu", "parameters": [lista[2]["contextData"]] }),
    );
    let menu = menu["result"].as_array().unwrap().clone();
    assert_eq!(menu[1]["title"], "Pinear");
    // Cada accion del menu, como la manda Flow v1: sus parametros sin envolver.
    let accion = |i: usize| {
        let a = &menu[i]["jsonRPCAction"];
        json!({ "method": a["method"], "parameters": a["parameters"] })
    };
    let (res, _, m) = v1(&r, accion(1));
    assert_eq!(res, json!({}));
    assert_eq!(m.pedidos[0]["accion"], "pinear");
    let (res, _, m) = v1(&r, accion(3));
    assert_eq!(res, json!({}));
    assert!(m.lanzados[0].ends_with("croquis.png"));
    let (carpeta, _, _) = v1(&r, accion(4));
    assert_eq!(carpeta["method"], "Flow.Launcher.OpenDirectory");
    assert!(
        carpeta["parameters"][1]
            .as_str()
            .unwrap()
            .ends_with("croquis.png")
    );
    let (copiar, _, _) = v1(&r, accion(5));
    assert_eq!(copiar["method"], "Flow.Launcher.CopyToClipboard");
    // Intro en un proyecto de la busqueda: entra en su chat.
    let (q, _, _) = v1(&r, json!({ "method": "query", "parameters": ["thesis"] }));
    let a = &q["result"][0]["jsonRPCAction"];
    assert_eq!(a["method"], "consulta");
    let (cambio, _, _) = v1(
        &r,
        json!({ "method": a["method"], "parameters": a["parameters"] }),
    );
    assert_eq!(
        cambio,
        json!({ "method": "Flow.Launcher.ChangeQuery", "parameters": ["p Thesis > ", true] })
    );
}

#[test]
fn en_v2_los_iconos_que_faltan_se_piden_tras_contestar() {
    let r = raiz("v2-iconos");
    let mut p = plugin(&r);
    p.atender(&consulta(1, "remoto"));
    assert_eq!(salida(&p).len(), 1, "la respuesta, y nada mas por stdout");
    assert_eq!(
        p.mensajero.avisos,
        vec![json!({ "pixpin": 1, "accion": "iconos", "extensiones": ["pdf"] })]
    );
    assert!(p.mensajero.pedidos.is_empty());
}

#[test]
fn anadir_a_proyecto_con_el_recuadro_flotante() {
    let r = raiz("soltar");
    let soltar = |p: Value| json!({ "pixpin": 1, "accion": "soltar", "proyecto": p });
    // Por su nombre o un alias, la funcion sale la primera.
    for q in ["solt", "adjun", "agreg", "dro"] {
        assert_eq!(buscar(&r, q)[0].titulo, "Añadir a proyecto", "{q}");
    }
    assert_eq!(
        buscar(&r, "añad")[0].accion,
        Accion::Consulta("pp añadir ".into())
    );
    // Sin nombre: guardados primero y despues los proyectos, del mas tocado.
    let v = buscar(&r, "añadir");
    assert_eq!(
        titulos(&v),
        [
            "Añadir a «Mensajes guardados»",
            "Añadir a «Gestión de proyectos»",
            "Añadir a «Thesis»",
            "Añadir a «Chat de clase»"
        ]
    );
    assert_eq!(*pedido_de(&v[0]), soltar(Value::Null));
    assert_eq!(*pedido_de(&v[1]), soltar(json!("P1")));
    // Con texto, filtra; con alias o tildes, igual.
    let v = buscar(&r, "anadir thes");
    assert_eq!(titulos(&v), ["Añadir a «Thesis»"]);
    assert_eq!(*pedido_de(&v[0]), soltar(json!("P2")));
    assert_eq!(
        titulos(&buscar(&r, "subir gestion")),
        ["Añadir a «Gestión de proyectos»"]
    );
    assert_eq!(
        titulos(&buscar(&r, "agregar mensajes")),
        ["Añadir a «Mensajes guardados»"]
    );
    // Con @ tambien.
    assert_eq!(
        *pedido_de(&buscar(&r, "adjuntar @Thesis")[0]),
        soltar(json!("P2"))
    );
    // Nada que encaje: a guardados, diciendolo.
    let v = buscar(&r, "añadir zzz");
    assert_eq!(v.len(), 1);
    assert!(v[0].titulo.contains("No hay ningún proyecto «zzz»"));
    assert_eq!(*pedido_de(&v[0]), soltar(Value::Null));
}

#[test]
fn leccion_con_texto_abre_la_ficha_rellena_y_sin_texto_la_ofrece() {
    let r = raiz("lecciones");
    let v = buscar(&r, "lección no cargar el móvil de noche");
    assert_eq!(v[0].titulo, "Nueva lección: no cargar el móvil de noche");
    let p = pedido_de(&v[0]);
    assert_eq!(p["accion"], "leccion_nueva");
    assert_eq!(p["texto"], "no cargar el móvil de noche");
    assert!(p.get("imagenes").is_none());
    assert_eq!(pedido_de(v.last().unwrap())["accion"], "lecciones");
    // «a» es lo mismo que «lección».
    assert_eq!(
        buscar(&r, "a no cargar el móvil de noche")[0].titulo,
        "Nueva lección: no cargar el móvil de noche"
    );
    let v = buscar(&r, "aprendi");
    assert_eq!(pedido_de(&v[0])["accion"], "leccion_nueva");
    assert_eq!(v[1].accion, Accion::Consulta("pp lecciones ".into()));
    assert_eq!(
        *pedido_de(&v[2]),
        json!({ "pixpin": 1, "accion": "ventana", "cual": "lecciones" })
    );
}

// --- Las lecciones aprendidas, leidas del disco ----------------------------

/// La raiz de siempre con dos lecciones: un error grave con foto en
/// «Mensajes guardados» que toca repasar, y un acierto en «Gestión de
/// proyectos» que no; y una tercera cuyo fichero aun no llego.
fn raiz_con_lecciones(etiqueta: &str) -> PathBuf {
    let r = raiz(etiqueta);
    let p = r.join("proyectos");
    let escribir = |dir: &str, id: &str, v: Value| {
        let d = p.join(dir).join("android/guardados/lecciones");
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join(format!("{id}.leccion")),
            serde_json::to_string_pretty(&v).unwrap(),
        )
        .unwrap();
    };
    escribir(
        "G1",
        "k1",
        json!({ "id": "k1", "creada": 100, "tocada": 100, "titulo": "Revisar los puntales antes de hormigonar",
            "quePaso": "Se abrió el encofrado", "porQue": "Prisa", "proxima": "Mirar cada puntal",
            "tipo": "error", "area": "Construcción", "gravedad": 3, "etiquetas": ["encofrado"],
            "etiquetasAuto": ["obra"], "repeticiones": [300], "caja": 0, "repasar": 500, "adjuntos": ["f1"], "campoNuevo": 1 }),
    );
    escribir(
        "P1",
        "k2",
        json!({ "id": "k2", "creada": 200, "titulo": "Mandar el acta el mismo día", "tipo": "acierto",
            "area": "Trabajo", "repasar": 5000 }),
    );
    fs::create_dir_all(p.join("G1/archivos")).unwrap();
    fs::write(p.join("G1/archivos/foto-leccion.png"), b"png").unwrap();
    let anadir = |dir: &str, lineas: &[Value]| {
        let f = p.join(dir).join("guardados.jsonl");
        let mut t = fs::read_to_string(&f).unwrap();
        for l in lineas {
            t.push('\n');
            t.push_str(&l.to_string());
        }
        t.push('\n');
        fs::write(&f, t).unwrap();
    };
    anadir(
        "G1",
        &[
            json!({ "id": "lec-k1", "cuando": 100, "clase": "ARCHIVO", "nombre": "💡 Revisar los puntales antes de hormigonar",
                "ruta": "pixpin:files/guardados/lecciones/k1.leccion", "texto": "⚠️ Error que no repetir: Revisar los puntales" }),
            json!({ "id": "f1", "cuando": 101, "clase": "IMAGEN", "nombre": "Foto de la lección",
                "ruta": "archivos/foto-leccion.png", "respondeA": "lec-k1" }),
            json!({ "id": "lec-k3", "cuando": 102, "clase": "ARCHIVO", "nombre": "💡 Aun no llego",
                "ruta": "pixpin:files/guardados/lecciones/k3.leccion" }),
        ],
    );
    anadir(
        "P1",
        &[
            json!({ "id": "lec-k2", "cuando": 200, "clase": "ARCHIVO", "nombre": "💡 Mandar el acta el mismo día",
            "ruta": "pixpin:files/guardados/lecciones/k2.leccion" }),
        ],
    );
    r
}

#[test]
fn lecciones_sin_texto_repasar_y_todas_de_la_mas_tocada() {
    let r = raiz_con_lecciones("lecciones-todas");
    let v = buscar(&r, "lecciones");
    assert_eq!(
        titulos(&v),
        [
            "Repasar hoy (1)",
            "Mandar el acta el mismo día",
            "Revisar los puntales antes de hormigonar",
            "Abrir la ventana de Lecciones"
        ]
    );
    assert_eq!(v[0].accion, Accion::Consulta("pp repasar ".into()));
    let k1 = &v[2];
    assert_eq!(
        k1.subtitulo,
        "📎 1 foto · Error · Construcción · #encofrado #obra · 🔁 2 · hoy"
    );
    assert_eq!(
        v[1].subtitulo,
        "Acierto · Trabajo · Gestión de proyectos · hoy"
    );
    let ruta = r
        .join("proyectos")
        .join("G1")
        .join("android")
        .join("guardados")
        .join("lecciones")
        .join("k1.leccion");
    assert_eq!(
        *pedido_de(k1),
        json!({ "pixpin": 1, "accion": "abrir", "que": { "tipo": "fichero", "ruta": ruta.to_string_lossy() } })
    );
    let foto = r
        .join("proyectos")
        .join("G1")
        .join("archivos")
        .join("foto-leccion.png")
        .to_string_lossy()
        .to_string();
    assert_eq!(k1.vista_previa.as_deref(), Some(foto.as_str()));
    // El texto entero al pasar el raton, como el resumen del movil.
    let ayuda = k1.ayuda_subtitulo.as_deref().unwrap();
    assert!(
        ayuda.starts_with("⚠️ Error que no repetir: Revisar los puntales"),
        "{ayuda}"
    );
    assert!(
        ayuda.contains("La próxima vez: Mirar cada puntal") && ayuda.contains("📎 1 adjunto"),
        "{ayuda}"
    );
    assert_eq!(k1.clave.as_deref(), Some("leccion/k1"));
    let j = k1.a_json("i", 1);
    assert_eq!(j["glyph"]["glyph"], crate::resultados::glifo::LECCION);
    assert_eq!(j["preview"]["previewImagePath"], foto);
    // Con @proyecto, solo las suyas.
    assert_eq!(
        titulos(&buscar(&r, "lecciones @Gestión de proyectos"))[1],
        "Mandar el acta el mismo día"
    );
    assert!(
        !titulos(&buscar(&r, "lecciones @Gestión de proyectos"))
            .contains(&"Revisar los puntales antes de hormigonar")
    );
}

#[test]
fn lecciones_con_texto_busca_como_el_movil() {
    let r = raiz_con_lecciones("lecciones-buscar");
    // Por la etiqueta, por una palabra con plural y con una errata.
    for q in [
        "lecciones encofrado",
        "lecciones puntal",
        "lessons encofrdo",
        "lecciones prisa",
    ] {
        assert_eq!(
            buscar(&r, q)[0].titulo,
            "Revisar los puntales antes de hormigonar",
            "«{q}»"
        );
    }
    let v = buscar(&r, "lecciones acta");
    assert_eq!(v[0].titulo, "Mandar el acta el mismo día");
    assert!(v[0].resaltado.len() >= 4, "lo buscado, resaltado");
    // Al final: buscarlo en la ventana y apuntarlo como nueva.
    let n = v.len();
    assert_eq!(
        *pedido_de(&v[n - 2]),
        json!({ "pixpin": 1, "accion": "lecciones", "consulta": "acta", "proyecto": null })
    );
    assert_eq!(pedido_de(&v[n - 1])["accion"], "leccion_nueva");
}

#[test]
fn caso_negativo_lecciones_sin_coincidencias_ni_ficheros() {
    let r = raiz_con_lecciones("lecciones-no");
    let v = buscar(&r, "lecciones zzzz");
    assert_eq!(v[0].titulo, "Ninguna lección habla de «zzzz»");
    assert_eq!(pedido_de(&v[2])["texto"], "zzzz");
    // La leccion cuyo fichero no llego no sale, ni las lecciones ni sus fotos
    // salen como archivos del chat.
    let todas = buscar(&r, "lecciones");
    assert!(!titulos(&todas).iter().any(|t| t.contains("Aun no llego")));
    for q in ["foto de la leccion", "aun no llego", "💡"] {
        assert!(
            !buscar(&r, q)
                .iter()
                .any(|x| x.titulo.contains("Foto de la lección") || x.titulo.contains("💡")),
            "«{q}»"
        );
    }
    assert!(
        !buscar(&r, "Mensajes guardados > ")
            .iter()
            .any(|x| x.titulo.contains("puntales") || x.titulo.contains("Foto de"))
    );
    // Sin lecciones, se dice como apuntar la primera.
    let vacia = raiz("lecciones-vacia");
    assert_eq!(
        titulos(&buscar(&vacia, "lecciones"))[1],
        "Aún no hay lecciones"
    );
}

#[test]
fn repasar_hoy_con_la_regla_de_la_app() {
    let r = raiz_con_lecciones("repasar");
    let v = buscar(&r, "repasar");
    assert_eq!(
        titulos(&v),
        [
            "Revisar los puntales antes de hormigonar",
            "Abrir la ventana de Lecciones"
        ]
    );
    assert!(
        v[0].subtitulo
            .starts_with("La próxima vez: Mirar cada puntal · "),
        "{}",
        v[0].subtitulo
    );
    assert_eq!(pedido_de(&v[0])["que"]["tipo"], "fichero");
    // Caso negativo: antes de que toque, ninguna, y se dice cuando.
    let mut d = Datos::nuevo(r.clone());
    let v = resultados(&d.proyectos(), "repaso", &Contexto::con("pp ", 0));
    assert_eq!(v[0].titulo, "Hoy no toca repasar ninguna");
    assert_eq!(v[0].subtitulo, "La siguiente toca mañana");
}

#[test]
fn las_lecciones_salen_al_buscar_en_todo_por_debajo_de_lo_exacto() {
    let r = raiz_con_lecciones("lecciones-en-todo");
    let v = buscar(&r, "acta");
    let nota = v.iter().position(|x| x.titulo == "Acta").expect("la nota");
    let leccion = v
        .iter()
        .position(|x| x.titulo == "Mandar el acta el mismo día")
        .expect("la leccion");
    assert!(nota < leccion, "{:?}", titulos(&v));
    // Por significado o por lo que paso tambien.
    assert!(
        titulos(&buscar(&r, "encofrado")).contains(&"Revisar los puntales antes de hormigonar")
    );
    // Caso negativo: una palabra que no dice ninguna, ninguna leccion.
    assert!(!buscar(&r, "integrales").iter().any(|x| {
        x.glifo == crate::resultados::glifo::LECCION
            && x.clave
                .as_deref()
                .is_some_and(|c| c.starts_with("leccion/"))
    }));
}

#[test]
fn el_menu_de_una_leccion() {
    let r = raiz_con_lecciones("lecciones-menu");
    let v = buscar(&r, "lecciones puntal");
    let m = menu_de(&v[0]);
    assert_eq!(
        titulos(&m),
        [
            "Abrir la ficha",
            "Ver en la lista de lecciones",
            "Copiar texto",
            "Pinear su foto",
            "Mostrar en la carpeta"
        ]
    );
    assert_eq!(m[0].accion, v[0].accion);
    assert_eq!(
        *pedido_de(&m[1]),
        json!({ "pixpin": 1, "accion": "lecciones", "consulta": "Revisar los puntales antes de hormigonar", "proyecto": null })
    );
    assert!(matches!(&m[2].accion, Accion::Copiar(t) if t.starts_with("⚠️ Error que no repetir")));
    let carpeta = r
        .join("proyectos")
        .join("G1")
        .join("android")
        .join("guardados")
        .join("lecciones")
        .to_string_lossy()
        .to_string();
    assert!(
        matches!(&m[4].accion, Accion::Carpeta { carpeta: c, fichero } if *c == carpeta && fichero.ends_with("k1.leccion"))
    );
    // Caso negativo: sin foto no hay pin, y la de un proyecto lo lleva.
    let m = menu_de(&buscar(&r, "lecciones acta")[0]);
    assert!(!titulos(&m).contains(&"Pinear su foto"));
    assert_eq!(pedido_de(&m[1])["proyecto"], "P1");
}

#[test]
fn una_leccion_nueva_lleva_sus_imagenes_pegadas() {
    let r = raiz_con_lecciones("leccion-imagenes");
    let img = imagenes::carpeta(&r).join("pegada.png");
    fs::create_dir_all(img.parent().unwrap()).unwrap();
    fs::write(&img, b"png").unwrap();
    let ahora = crate::datos::ahora_ms();
    assert!(imagenes::apuntar(&r, 1, &img, None, ahora));
    let v = buscar_con(&r, "a mirar el plano [img 01]", imagenes::SIN_PORTAPAPELES);
    assert_eq!(v[0].titulo, "Nueva lección: mirar el plano [img 01]");
    let p = pedido_de(&v[0]);
    assert_eq!(p["accion"], "leccion_nueva");
    assert_eq!(p["texto"], "mirar el plano [img 01]");
    assert_eq!(p["imagenes"], json!([img.to_string_lossy()]));
    assert!(
        v[0].subtitulo.starts_with("📎 1 imagen"),
        "{}",
        v[0].subtitulo
    );
    // Buscar las parecidas no lleva la ficha.
    assert!(
        v.iter()
            .any(|x| x.titulo == "Buscar en lecciones: mirar el plano")
    );
    // Con una imagen copiada se ofrece pegarla, como en las tareas.
    let v = buscar_con(&r, "a mirar", CON_IMAGEN);
    assert_eq!(
        v[0].accion,
        Accion::PegarImagen {
            consulta: "pp a mirar [img 01] ".into(),
            numero: 1
        }
    );
    assert!(v[0].subtitulo.contains("la lección"), "{}", v[0].subtitulo);
    // Caso negativo: una ficha que no esta en el borrador se queda como texto.
    let v = buscar_con(&r, "a mirar [img 05]", imagenes::SIN_PORTAPAPELES);
    assert!(pedido_de(&v[0]).get("imagenes").is_none());
    assert!(
        v[0].subtitulo.contains("no se encuentra"),
        "{}",
        v[0].subtitulo
    );
    // Ni se ofrece pegar en buscar lecciones o repasar.
    for q in ["lecciones mirar", "repasar"] {
        assert!(
            !buscar_con(&r, q, CON_IMAGEN)
                .iter()
                .any(|x| matches!(x.accion, Accion::PegarImagen { .. })),
            "«{q}»"
        );
    }
}

// --- El Inbox, los atajos, las capturas y los campos nuevos de Flow --------

/// La raiz de siempre con un Inbox en «Mensajes guardados».
fn raiz_con_inbox(etiqueta: &str) -> PathBuf {
    let r = raiz(etiqueta);
    let g1 = r.join("proyectos/G1/guardados.jsonl");
    let mut t = fs::read_to_string(&g1).unwrap();
    t.push_str(
        &json!({ "id": "ib", "cuando": 50, "clase": "MINIAPP", "miniapp": "tareas",
            "texto": "# Inbox\n\n- [ ] llamar a Ana\n- [x] pagar luz" })
        .to_string(),
    );
    t.push('\n');
    fs::write(&g1, t).unwrap();
    r
}

#[test]
fn tareas_ensena_el_inbox_primero_con_sus_tareas() {
    let r = raiz_con_inbox("inbox");
    let v = buscar(&r, "tareas");
    assert_eq!(
        titulos(&v),
        [
            "Inbox",
            "☐ llamar a Ana",
            "☑ pagar luz",
            "Abrir la ventana de Tareas",
            "Compra",
            "Compra"
        ]
    );
    assert_eq!(v[0].progreso, Some(50));
    assert_eq!(v[0].accion, Accion::Consulta("pp tareas Inbox > ".into()));
    // Intro marca la tarea y vuelve a `p tareas`.
    assert_eq!(
        v[1].accion,
        Accion::PedirYSeguir {
            pedido: json!({ "pixpin": 1, "accion": "marcar_tarea", "proyecto": null, "codigo": "ib", "indice": 0, "hecha": true }),
            consulta: "pp tareas ".into(),
        }
    );
    assert_eq!(v[1].subtitulo, "Tarea · n.º 1 · Inbox · Intro: hecha");
    assert_eq!(
        *pedido_de(&v[3]),
        json!({ "pixpin": 1, "accion": "ventana", "cual": "tareas" })
    );
    // El Inbox no se repite entre las listas.
    assert_eq!(v.iter().filter(|x| x.titulo == "Inbox").count(), 1);
}

#[test]
fn mover_una_tarea_a_otra_lista_desde_el_menu() {
    let r = raiz_con_inbox("mover");
    let v = buscar(&r, "tareas");
    let m = menu_de(&v[1]);
    assert_eq!(
        titulos(&m),
        [
            "Abrir",
            "Ver en el chat",
            "Copiar texto",
            "Mover a «Compra»",
            "Mover a «Compra»",
            "Quitar tarea",
            "Abrir la carpeta del proyecto"
        ]
    );
    assert_eq!(m[0].accion, v[1].accion, "Abrir es marcarla, como Intro");
    assert_eq!(m[3].subtitulo, "Lista de tareas · Gestión de proyectos");
    assert_eq!(
        *pedido_de(&m[3]),
        json!({ "pixpin": 1, "accion": "mover_tarea", "proyecto": null, "codigo": "ib", "indice": 0, "a_proyecto": "P1", "a_codigo": "t2" })
    );
    assert_eq!(pedido_de(&m[4])["a_proyecto"], Value::Null);
    assert_eq!(pedido_de(&m[4])["a_codigo"], "t1");
}

#[test]
fn quitar_una_tarea_y_borrar_una_lista_desde_el_menu() {
    let r = raiz_con_inbox("quitar-menu");
    let v = buscar(&r, "tareas Compra · Mensajes guardados > ");
    assert_eq!(v[1].titulo, "☐ huevos");
    let m = menu_de(&v[1]);
    let quitar = m.iter().find(|x| x.titulo == "Quitar tarea").unwrap();
    // Vuelve a la misma lista, sin cerrar Flow.
    assert_eq!(
        quitar.accion,
        Accion::PedirYSeguir {
            pedido: json!({ "pixpin": 1, "accion": "quitar_tarea", "proyecto": null, "codigo": "t1", "indice": 2 }),
            consulta: "pp tareas Compra · Mensajes guardados > ".into(),
        }
    );
    // La lista, desde todas las listas: «Borrar lista» vuelve a ellas.
    let v = buscar(&r, "tareas");
    let compra = v
        .iter()
        .find(|x| x.titulo == "Compra" && x.subtitulo.contains("Mensajes guardados"))
        .unwrap();
    let m = menu_de(compra);
    let borrar = m.iter().find(|x| x.titulo == "Borrar lista").unwrap();
    assert_eq!(
        borrar.accion,
        Accion::PedirYSeguir {
            pedido: json!({ "pixpin": 1, "accion": "borrar_lista", "proyecto": null, "codigo": "t1" }),
            consulta: "pp tareas ".into(),
        }
    );
    assert!(
        borrar.subtitulo.contains("3 tareas"),
        "{}",
        borrar.subtitulo
    );
    assert_eq!(
        m[m.len() - 2].titulo,
        "Borrar lista",
        "lo que borra, al final (antes de la carpeta)"
    );
    // Caso negativo: lo que no es una tarea ni una lista no ofrece ninguno.
    let v = buscar(&r, "calculo");
    let m = menu_de(&v[0]);
    assert!(
        m.iter()
            .all(|x| x.titulo != "Quitar tarea" && x.titulo != "Borrar lista"),
        "{:?}",
        titulos(&m)
    );
}

#[test]
fn quitar_una_tarea_la_saca_de_la_lista_sin_cerrar_flow() {
    let r = raiz("rpc-quitar");
    let mut p = plugin(&r);
    let q = "tareas Compra · Mensajes guardados > ";
    // Lo que manda Flow al elegir «Quitar tarea» en el menu de «pan».
    let pedido = json!({ "pixpin": 1, "accion": "quitar_tarea", "proyecto": null, "codigo": "t1", "indice": 0 });
    p.atender(
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "pedir_y_seguir",
            "params": [[pedido, format!("pp {q}")]] })
        .to_string(),
    );
    assert_eq!(p.mensajero.pedidos[0]["accion"], "quitar_tarea");
    // La app de mentira no escribe: se adelanta en la cache, sin «pan».
    p.atender(&consulta(2, q));
    let s = salida(&p);
    let titulos: Vec<&str> = s.last().unwrap()["result"]["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap())
        .collect();
    assert_eq!(titulos, ["☐ huevos", "☑ leche"]);
}

#[test]
fn con_tarea_quitada_saca_solo_esa_casilla() {
    let d = "# Compra\n\n- [ ] pan\nnota suelta\n- [x] leche\n- [ ] huevos";
    assert_eq!(
        crate::datos::con_tarea_quitada(d, 1),
        "# Compra\n\n- [ ] pan\nnota suelta\n- [ ] huevos"
    );
    // Caso negativo: una que no existe lo deja igual.
    assert_eq!(crate::datos::con_tarea_quitada(d, 3), d);
}

#[test]
fn caso_negativo_no_se_ofrece_mover_a_su_propia_lista() {
    let r = raiz_con_inbox("mover-no");
    let v = buscar(&r, "tareas Compra · Gestión de proyectos > ");
    assert_eq!(v[0].titulo, "☐ tornillos");
    let destinos: Vec<Value> = menu_de(&v[0])
        .iter()
        .filter(|x| x.titulo.starts_with("Mover a"))
        .map(|x| pedido_de(x)["a_codigo"].clone())
        .collect();
    assert_eq!(destinos, [json!("t1"), json!("ib")]);
}

#[test]
fn t_apunta_en_el_inbox_y_tareas_con_texto_tambien() {
    let r = raiz_con_inbox("apuntar");
    let v = buscar(&r, "t comprar pan");
    assert_eq!(v[0].titulo, "Apuntar tarea: comprar pan");
    // Sin lista: la app la pone en el Inbox.
    assert_eq!(
        v[0].accion,
        Accion::Pedido(json!({ "pixpin": 1, "accion": "anadir_tarea", "texto": "comprar pan" }))
    );
    assert_eq!(
        *pedido_de(&v[1]),
        json!({ "pixpin": 1, "accion": "ventana", "cual": "tareas" })
    );
    // «tareas <texto>»: listas que encajan, lista nueva y, abajo, el Inbox.
    let v = buscar(&r, "tareas leche");
    let ultimo = v.last().unwrap();
    assert_eq!(ultimo.titulo, "Añadir al Inbox: leche");
    assert_eq!(
        ultimo.accion,
        Accion::PedirYSeguir {
            pedido: json!({ "pixpin": 1, "accion": "anadir_tarea", "texto": "leche", "proyecto": null, "codigo": "ib" }),
            consulta: "pp tareas ".into(),
        }
    );
    // Sin Inbox todavia, sin lista: la app lo crea.
    let r = raiz("apuntar-sin-inbox");
    let v = buscar(&r, "tareas leche");
    assert_eq!(
        *pedido_de(v.last().unwrap()),
        json!({ "pixpin": 1, "accion": "anadir_tarea", "texto": "leche" })
    );
}

#[test]
fn caso_negativo_una_letra_pegada_no_apunta_nada() {
    let r = raiz_con_inbox("apuntar-no");
    for q in ["tx", "tornillos", "t"] {
        let v = buscar(&r, q);
        assert!(
            !v.iter()
                .any(|x| matches!(&x.accion, Accion::Pedido(p) if p["accion"] == "anadir_tarea")),
            "«{q}» no es apuntar"
        );
    }
    // «t» sola es la funcion de tareas: el Inbox el primero.
    assert_eq!(buscar(&r, "t")[0].titulo, "Inbox");
}

#[test]
fn los_atajos_de_una_letra() {
    let r = raiz("atajos");
    assert_eq!(
        *pedido_de(&buscar(&r, "n idea")[0]),
        json!({ "pixpin": 1, "accion": "nota_nueva", "texto": "idea", "proyecto": null })
    );
    assert_eq!(
        *pedido_de(&buscar(&r, "l plano")[0]),
        json!({ "pixpin": 1, "accion": "lienzo_nuevo", "nombre": "plano", "proyecto": null })
    );
    assert_eq!(
        *pedido_de(&buscar(&r, "g")[0]),
        json!({ "pixpin": 1, "accion": "ventana", "cual": "galeria" })
    );
    assert_eq!(
        *pedido_de(&buscar(&r, "c")[0]),
        json!({ "pixpin": 1, "accion": "capturar", "modo": "zona" })
    );
    assert_eq!(
        *pedido_de(&buscar(&r, "u")[0]),
        json!({ "pixpin": 1, "accion": "pinear_ultima" })
    );
    // Por su nombre tambien.
    assert_eq!(
        *pedido_de(&buscar(&r, "captura")[0]),
        json!({ "pixpin": 1, "accion": "capturar", "modo": "zona" })
    );
    assert_eq!(
        *pedido_de(&buscar(&r, "última")[0]),
        json!({ "pixpin": 1, "accion": "pinear_ultima" })
    );
    assert_eq!(
        *pedido_de(&buscar(&r, "galeria")[0]),
        json!({ "pixpin": 1, "accion": "ventana", "cual": "galeria" })
    );
    // Las lecciones, con su ventana.
    let ventana = json!({ "pixpin": 1, "accion": "ventana", "cual": "lecciones" });
    assert!(
        buscar(&r, "lecciones")
            .iter()
            .any(|x| matches!(&x.accion, Accion::Pedido(p) if *p == ventana))
    );
}

#[test]
fn caso_negativo_una_letra_sin_atajo_busca_normal() {
    let r = raiz("atajos-no");
    // «x» no es atajo: busca, y no captura nada.
    assert!(
        buscar(&r, "x")
            .iter()
            .all(|x| !matches!(&x.accion, Accion::Pedido(p) if p["accion"] == "capturar"))
    );
    // «ca» busca: salen varias cosas, no solo capturar.
    assert!(buscar(&r, "ca").len() > 1);
}

/// Una carpeta de capturas: la 0002 de hace 2 h, la 0001 (conservada) de hace
/// 3 h, y dos que no cuentan (vacia y de texto).
fn raiz_con_capturas(etiqueta: &str) -> (PathBuf, i64) {
    let r = raiz(etiqueta);
    let c = r.join("capturas");
    fs::create_dir_all(&c).unwrap();
    let ahora = crate::datos::ahora_ms();
    let h = 3_600_000i64;
    for (nombre, hace) in [("captura-0001.png", 3 * h), ("captura-0002.png", 2 * h)] {
        fs::write(c.join(nombre), b"png").unwrap();
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_millis((ahora - hace) as u64);
        fs::File::options()
            .write(true)
            .open(c.join(nombre))
            .unwrap()
            .set_modified(t)
            .unwrap();
    }
    fs::write(c.join("vacia.png"), b"").unwrap();
    fs::write(c.join("notas.txt"), b"x").unwrap();
    fs::write(
        r.join("capturas-caducidad.json"),
        json!({ "desde": 0, "conservadas": ["captura-0001.png"] }).to_string(),
    )
    .unwrap();
    (r, ahora)
}

fn buscar_con_raiz(r: &Path, q: &str, ahora: i64) -> Vec<Resultado> {
    let mut d = Datos::nuevo(r.to_path_buf());
    let mut c = Contexto::con("pp ", ahora);
    c.iconos = Some(crate::iconos::carpeta(r));
    resultados(&d.proyectos(), q, &c)
}

#[test]
fn las_capturas_de_la_mas_nueva_a_la_mas_vieja_con_su_caducidad() {
    let (r, ahora) = raiz_con_capturas("capturas");
    let v = buscar_con_raiz(&r, "capturas", ahora);
    assert_eq!(
        titulos(&v),
        [
            "captura-0002.png",
            "captura-0001.png",
            "Abrir la galería de capturas"
        ]
    );
    let ruta = r
        .join("capturas")
        .join("captura-0002.png")
        .to_string_lossy()
        .to_string();
    // Sin `desde` en el registro, los 7 dias cuentan desde ahora (como la app).
    let se_va = crate::fecha::dia_y_mes(ahora + 7 * 86_400_000);
    assert_eq!(
        v[0].subtitulo,
        format!("Captura · hace 2 h · se borra el {se_va}")
    );
    assert_eq!(v[1].subtitulo, "Captura · hace 3 h · Conservada");
    assert_eq!(
        *pedido_de(&v[0]),
        json!({ "pixpin": 1, "accion": "pinear", "ruta": ruta })
    );
    // Su icono y su vista previa: la imagen misma.
    assert_eq!(v[0].icono.as_deref(), Some(ruta.as_str()));
    let j = v[0].a_json("icono.png", 100);
    assert_eq!(
        j["preview"],
        json!({ "previewImagePath": ruta, "isMedia": true, "filePath": ruta })
    );
    assert_eq!(j["recordKey"], "captura/captura-0002.png");
    assert_eq!(
        *pedido_de(&v[2]),
        json!({ "pixpin": 1, "accion": "ventana", "cual": "galeria" })
    );
    // Filtrar por nombre.
    assert_eq!(
        titulos(&buscar_con_raiz(&r, "capturas 0001", ahora)),
        ["captura-0001.png", "Abrir la galería de capturas"]
    );
    // La galeria: abrirla primero y debajo las capturas.
    assert_eq!(
        titulos(&buscar_con_raiz(&r, "galeria", ahora))[0],
        "Abrir la galería de capturas"
    );
    // La ultima, con su nombre.
    let u = buscar_con_raiz(&r, "u", ahora);
    assert!(
        u[0].subtitulo.starts_with("captura-0002.png · hace 2 h"),
        "{}",
        u[0].subtitulo
    );
}

#[test]
fn el_menu_de_una_captura() {
    let (r, ahora) = raiz_con_capturas("capturas-menu");
    let v = buscar_con_raiz(&r, "capturas", ahora);
    let ruta = r
        .join("capturas")
        .join("captura-0002.png")
        .to_string_lossy()
        .to_string();
    let m = menu_de(&v[0]);
    assert_eq!(
        titulos(&m),
        [
            "Conservar",
            "Copiar imagen",
            "Borrar",
            "Mostrar en la carpeta",
            "Abrir galería"
        ]
    );
    assert_eq!(
        *pedido_de(&m[0]),
        json!({ "pixpin": 1, "accion": "conservar_captura", "ruta": ruta })
    );
    assert_eq!(
        *pedido_de(&m[1]),
        json!({ "pixpin": 1, "accion": "copiar_imagen", "ruta": ruta })
    );
    assert_eq!(
        *pedido_de(&m[2]),
        json!({ "pixpin": 1, "accion": "borrar_captura", "ruta": ruta })
    );
    assert_eq!(
        m[3].accion,
        Accion::Carpeta {
            carpeta: r.join("capturas").to_string_lossy().to_string(),
            fichero: ruta
        }
    );
    assert_eq!(
        *pedido_de(&m[4]),
        json!({ "pixpin": 1, "accion": "ventana", "cual": "galeria" })
    );
    // La conservada no ofrece conservarla otra vez.
    assert_eq!(
        titulos(&menu_de(&v[1])),
        [
            "Copiar imagen",
            "Borrar",
            "Mostrar en la carpeta",
            "Abrir galería"
        ]
    );
}

#[test]
fn caso_negativo_sin_capturas_o_sin_coincidencias() {
    let (r, ahora) = raiz_con_capturas("capturas-no");
    let v = buscar_con_raiz(&r, "capturas zzz", ahora);
    assert_eq!(v[0].titulo, "Ninguna captura con «zzz»");
    assert_eq!(v[0].accion, Accion::Consulta("pp capturas ".into()));
    let r = raiz("capturas-ninguna");
    let v = buscar_con_raiz(&r, "capturas", 1000);
    assert_eq!(v[0].titulo, "No hay capturas todavía");
    assert_eq!(
        *pedido_de(&v[0]),
        json!({ "pixpin": 1, "accion": "capturar", "modo": "zona" })
    );
    // Sin raiz conocida (sin carpeta de iconos), tampoco se rompe nada.
    assert_eq!(buscar(&r, "capturas")[0].titulo, "No hay capturas todavía");
}

#[test]
fn los_campos_nuevos_de_flow_en_el_json() {
    let r = raiz("campos");
    let v = buscar(&r, "croquis");
    let ruta = r
        .join("proyectos")
        .join("P1")
        .join("archivos")
        .join("croquis.png")
        .to_string_lossy()
        .to_string();
    let j = v[0].a_json("icono.png", 100);
    assert_eq!(j["title"], "croquis.png");
    assert_eq!(j["titleHighlightData"], json!([0, 1, 2, 3, 4, 5, 6]));
    assert_eq!(j["titleToolTip"], ruta);
    assert_eq!(j["recordKey"], "mensaje/P1/i2");
    assert_eq!(j["autoCompleteText"], "pp croquis.png");
    assert_eq!(
        j["preview"],
        json!({ "previewImagePath": ruta, "isMedia": true, "filePath": ruta })
    );
    // Un texto: el entero al pasar por el subtitulo.
    let v = buscar(&r, "hola equipo");
    let j = v
        .iter()
        .find(|x| x.titulo.starts_with("hola equipo"))
        .unwrap()
        .a_json("i", 1);
    assert_eq!(
        j["subTitleToolTip"],
        "hola equipo, mañana a las 9\nsegunda línea"
    );
    // Una lista: sin barra de progreso, que en Flow tapaba el titulo.
    let j = buscar(&r, "tareas")[2].a_json("i", 1);
    assert_eq!(j["title"], "Compra");
    assert!(j.get("progressBar").is_none());
    // El filtro de una lista se resalta en la tarea (en UTF-16: «☐ » son 2).
    let v = buscar(&r, "tareas Compra · Mensajes guardados > pan");
    assert_eq!(v[1].a_json("i", 1)["titleHighlightData"], json!([2, 3, 4]));
    // Y el del chat de un proyecto.
    let v = buscar(&r, "Gestión de proyectos > presupuesto");
    assert_eq!(v[0].titulo, "revisar el presupuesto");
    assert_eq!(v[0].resaltado, (11..22).collect::<Vec<_>>());
}

#[test]
fn caso_negativo_sin_consulta_ni_fichero_no_hay_campos_de_mas() {
    let r = raiz("campos-no");
    let v = buscar(&r, "");
    let j = v[0].a_json("icono.png", 100);
    assert!(
        j.get("titleHighlightData").is_none(),
        "sin nada tecleado no se resalta"
    );
    assert!(
        j.get("preview").is_none()
            && j.get("titleToolTip").is_none()
            && j.get("progressBar").is_none()
    );
    assert_eq!(j["recordKey"], "funcion/tareas");
    // Un archivo de otro equipo: ni vista previa ni ruta.
    let v = buscar(&r, "remoto");
    let j = v[0].a_json("i", 1);
    assert!(j.get("preview").is_none() && j.get("titleToolTip").is_none());
}

// --- Tareas con imagenes (las fichas `[img NN]`) ----------------------------

use crate::imagenes::{self, Portapapeles};

fn hay_imagen() -> Option<u32> {
    Some(7)
}
fn hay_otra() -> Option<u32> {
    Some(8)
}
fn otra_copia() -> Option<u32> {
    Some(9)
}

/// «La imagen del portapapeles»: unos bytes de PNG.
fn leer_de_mentira() -> Option<imagenes::Copiada> {
    Some(imagenes::Copiada {
        extension: "png".into(),
        bytes: b"\x89PNG de mentira".to_vec(),
    })
}
fn leer_otra() -> Option<imagenes::Copiada> {
    Some(imagenes::Copiada {
        extension: "png".into(),
        bytes: b"\x89PNG otra distinta".to_vec(),
    })
}
fn sin_leer() -> Option<imagenes::Copiada> {
    None
}
fn app_cerrada() -> bool {
    false
}
fn app_abierta() -> bool {
    true
}

const CON_IMAGEN: Portapapeles = Portapapeles {
    imagen: hay_imagen,
    leer: leer_de_mentira,
    pega_la_app: app_cerrada,
};
/// Otra imagen distinta, copiada despues.
const OTRA_IMAGEN: Portapapeles = Portapapeles {
    imagen: hay_otra,
    leer: leer_otra,
    pega_la_app: app_cerrada,
};
/// La primera imagen copiada otra vez: otro numero de secuencia, los mismos
/// bytes.
const LA_MISMA_OTRA_VEZ: Portapapeles = Portapapeles {
    imagen: otra_copia,
    leer: leer_de_mentira,
    pega_la_app: app_cerrada,
};
/// Dice que hay imagen pero al ir a guardarla ya no esta.
const SE_FUE: Portapapeles = Portapapeles {
    imagen: hay_imagen,
    leer: sin_leer,
    pega_la_app: app_cerrada,
};
/// Con la app abierta (pega ella con Ctrl+V).
const CON_LA_APP: Portapapeles = Portapapeles {
    imagen: hay_imagen,
    leer: leer_de_mentira,
    pega_la_app: app_abierta,
};

fn buscar_con(r: &Path, q: &str, pp: Portapapeles) -> Vec<Resultado> {
    let mut d = Datos::nuevo(r.to_path_buf());
    let mut c = Contexto::con("pp ", crate::datos::ahora_ms());
    c.raiz = Some(r.to_path_buf());
    c.portapapeles = pp;
    resultados(&d.proyectos(), q, &c)
}

/// Pulsa Intro en el resultado `r` (como Flow v2, los parametros envueltos).
fn intro(p: &mut Plugin<DeMentira, Vec<u8>>, id: u64, r: &Value) {
    let a = &r["jsonRPCAction"];
    p.atender(
        &json!({ "jsonrpc": "2.0", "id": id, "method": a["method"], "params": [a["parameters"]] })
            .to_string(),
    );
}

/// Los resultados de la ultima respuesta a `query`.
fn ultima_lista(p: &Plugin<DeMentira, Vec<u8>>) -> Vec<Value> {
    salida(p)
        .iter()
        .rev()
        .find_map(|l| l["result"]["result"].as_array().cloned())
        .unwrap()
}

#[test]
fn las_fichas_se_numeran_y_se_encuentran() {
    let f = imagenes::fichas("a [img 01] b [IMG 3] [img 0] [img x] [img 1234] [img 2");
    assert_eq!(f.iter().map(|f| f.numero).collect::<Vec<_>>(), [1, 3]);
    assert_eq!((f[0].inicio, f[0].fin), (2, 10));
    assert_eq!(imagenes::siguiente("t comprar pan"), 1);
    assert_eq!(imagenes::siguiente("t a [img 01] b [img 03]"), 4);
    assert_eq!(imagenes::ficha(2), "[img 02]");
    assert_eq!(imagenes::ficha(12), "[img 12]");
}

#[test]
fn caso_negativo_lo_que_parece_una_ficha_y_no_lo_es() {
    for t in [
        "[img]",
        "[img 0]",
        "[imagen 01]",
        "[img01]",
        "[img 1234]",
        "img 01]",
        "[img 01",
        "[ img 01]",
    ] {
        assert!(imagenes::fichas(t).is_empty(), "«{t}» no es una ficha");
    }
    assert_eq!(imagenes::siguiente("[img 0] [img x]"), 1);
}

#[test]
fn el_resaltado_de_las_fichas_va_en_utf16() {
    // «😀» son 2 en UTF-16: la ficha empieza en 22 y ocupa 8.
    let t = "Añadir tarea: 😀 foto [img 01] y [img 02]";
    assert_eq!(imagenes::resaltado(t, &[1]), (22..30).collect::<Vec<_>>());
    assert_eq!(
        imagenes::resaltado(t, &[1, 2]),
        (22..30).chain(33..41).collect::<Vec<_>>()
    );
    // Caso negativo: una ficha que no se conoce no se resalta.
    assert!(imagenes::resaltado(t, &[5]).is_empty());
}

#[test]
fn con_una_imagen_copiada_se_ofrece_pegarla_arriba() {
    let r = raiz_con_inbox("pegar");
    let v = buscar_con(&r, "t comprar", CON_IMAGEN);
    assert_eq!(v[0].titulo, "📎 Pegar la imagen copiada → [img 01]");
    assert_eq!(
        v[0].accion,
        Accion::PegarImagen {
            consulta: "pp t comprar [img 01] ".into(),
            numero: 1
        }
    );
    assert_eq!(v[1].titulo, "Apuntar tarea: comprar");
    let j = v[0].a_json("i", 1);
    assert_eq!(
        j["jsonRPCAction"],
        json!({ "method": "pegar_imagen", "parameters": ["pp t comprar [img 01] ", 1], "dontHideAfterAction": true })
    );
    // La ficha del titulo, resaltada («📎» son 2 en UTF-16).
    assert_eq!(j["titleHighlightData"], json!((29..37).collect::<Vec<_>>()));
    // La que toca es la siguiente a las que ya hay.
    let v = buscar_con(&r, "t comprar [img 01] y", CON_IMAGEN);
    assert_eq!(
        v[0].accion,
        Accion::PegarImagen {
            consulta: "pp t comprar [img 01] y [img 02] ".into(),
            numero: 2
        }
    );
    // «t» sola, dentro de una lista y «tareas <texto>» tambien.
    assert_eq!(
        buscar_con(&r, "t", CON_IMAGEN)[0].accion,
        Accion::PegarImagen {
            consulta: "pp t [img 01] ".into(),
            numero: 1
        }
    );
    let v = buscar_con(&r, "tareas Compra · Mensajes guardados > ", CON_IMAGEN);
    assert_eq!(
        v[0].accion,
        Accion::PegarImagen {
            consulta: "pp tareas Compra · Mensajes guardados > [img 01] ".into(),
            numero: 1
        }
    );
    assert!(matches!(
        buscar_con(&r, "tareas leche", CON_IMAGEN)[0].accion,
        Accion::PegarImagen { .. }
    ));
}

#[test]
fn caso_negativo_sin_imagen_o_fuera_de_una_tarea_no_se_ofrece_pegar() {
    let r = raiz_con_inbox("pegar-no");
    let pegar = |v: &[Resultado]| {
        v.iter()
            .any(|x| matches!(x.accion, Accion::PegarImagen { .. }))
    };
    // Sin imagen en el portapapeles, nada.
    for q in [
        "t comprar",
        "t",
        "tareas leche",
        "tareas Compra · Mensajes guardados > ",
    ] {
        assert!(
            !pegar(&buscar_con(&r, q, imagenes::SIN_PORTAPAPELES)),
            "«{q}» sin imagen"
        );
    }
    // Con imagen, pero no es escribir una tarea.
    for q in [
        "",
        "chat hola",
        "n idea",
        "tareas",
        "comprar",
        "tareas leche @thesis",
        "tareas NoExiste > x",
    ] {
        assert!(
            !pegar(&buscar_con(&r, q, CON_IMAGEN)),
            "«{q}» no es una tarea"
        );
    }
}

#[test]
fn pegar_guarda_la_imagen_y_la_tarea_la_lleva_en_el_pedido() {
    let r = raiz_con_inbox("pegar-todo");
    let mut p = plugin(&r);
    p.portapapeles = CON_IMAGEN;
    p.atender(&consulta(1, "t comprar"));
    let pegar = ultima_lista(&p)[0].clone();
    intro(&mut p, 2, &pegar);
    let s = salida(&p);
    let n = s.len();
    assert_eq!(s[n - 2]["method"], "ChangeQuery");
    assert_eq!(s[n - 2]["params"], json!(["pp t comprar [img 01] ", true]));
    assert_eq!(s[n - 1]["result"], json!({ "hide": false }));
    let b = imagenes::leer_borrador(&r, crate::datos::ahora_ms()).expect("el borrador");
    let img1 = b.imagenes[&1].clone();
    assert!(
        img1.contains("lanzador-imagenes") && Path::new(&img1).is_file(),
        "{img1}"
    );
    // Flow vuelve a preguntar con la ficha puesta, y se sigue escribiendo.
    p.atender(&consulta(3, "t comprar [img 01] esto"));
    let v = ultima_lista(&p);
    let anadir = &v[0];
    assert_eq!(anadir["title"], "Apuntar tarea: comprar [img 01] esto");
    assert_eq!(
        anadir["titleHighlightData"],
        json!((23..31).collect::<Vec<_>>())
    );
    assert_eq!(
        anadir["subTitle"],
        "📎 1 imagen · Tarea · Inbox · 1 pendiente de 2 · Intro: apuntarla"
    );
    assert_eq!(anadir["preview"]["previewImagePath"], img1);
    // Una imagen, un nombre: la misma copia ya no se ofrece como [img 02].
    assert!(
        !v.iter()
            .any(|x| x["title"].as_str().unwrap().starts_with("📎 Pegar")),
        "{v:?}"
    );
    // Una segunda imagen, distinta.
    p.portapapeles = OTRA_IMAGEN;
    p.atender(&consulta(4, "t comprar [img 01] esto"));
    let v = ultima_lista(&p);
    assert_eq!(v[0]["title"], "📎 Pegar la imagen copiada → [img 02]");
    intro(&mut p, 4, &v[0]);
    let img2 = imagenes::leer_borrador(&r, crate::datos::ahora_ms())
        .unwrap()
        .imagenes[&2]
        .clone();
    assert_ne!(img1, img2);
    // La primera, copiada otra vez (otra secuencia, mismo contenido): se
    // ofrece, pero al pegarla se avisa y no cambia nada.
    p.portapapeles = LA_MISMA_OTRA_VEZ;
    p.atender(&consulta(5, "t comprar [img 01] esto [img 02]"));
    let pegar = ultima_lista(&p)[0].clone();
    assert_eq!(pegar["title"], "📎 Pegar la imagen copiada → [img 03]");
    intro(&mut p, 5, &pegar);
    let s = salida(&p);
    assert_eq!(s[s.len() - 2]["method"], "ShowMsg");
    assert_eq!(
        s[s.len() - 2]["params"][1],
        "Esa imagen ya está como [img 01]"
    );
    let b = imagenes::leer_borrador(&r, crate::datos::ahora_ms()).unwrap();
    assert_eq!(b.imagenes.len(), 2, "no hay [img 03]");
    // Y ya no se vuelve a ofrecer esa copia.
    p.atender(&consulta(5, "t comprar [img 01] esto [img 02]"));
    assert!(
        !ultima_lista(&p)
            .iter()
            .any(|x| x["title"].as_str().unwrap().starts_with("📎 Pegar"))
    );
    p.atender(&consulta(5, "t comprar [img 01] esto [img 02]"));
    let anadir = ultima_lista(&p)[0].clone();
    assert_eq!(
        anadir["subTitle"].as_str().unwrap().split(" · ").next(),
        Some("📎 2 imágenes")
    );
    intro(&mut p, 6, &anadir);
    assert_eq!(
        p.mensajero.pedidos.last().unwrap(),
        &json!({ "pixpin": 1, "accion": "anadir_tarea", "texto": "comprar [img 01] esto [img 02]", "imagenes": [img1, img2] })
    );
    // Mandada la tarea, el borrador se olvida.
    assert!(imagenes::leer_borrador(&r, crate::datos::ahora_ms()).is_none());
}

#[test]
fn una_ficha_borrada_no_va_y_las_demas_se_renumeran() {
    let r = raiz_con_inbox("pegar-renumerar");
    let ahora = crate::datos::ahora_ms();
    let dir = imagenes::carpeta(&r);
    fs::create_dir_all(&dir).unwrap();
    let (a, b) = (dir.join("a.png"), dir.join("b.png"));
    fs::write(&a, b"a").unwrap();
    fs::write(&b, b"b").unwrap();
    assert!(imagenes::apuntar(&r, 1, &a, Some(1), ahora));
    assert!(imagenes::apuntar(&r, 2, &b, Some(2), ahora));
    // Se borro «[img 01]»: solo va la segunda, que pasa a ser la 01.
    let v = buscar_con(
        &r,
        "tareas Compra · Mensajes guardados > leer [img 02]",
        imagenes::SIN_PORTAPAPELES,
    );
    assert_eq!(v[0].titulo, "Añadir tarea: leer [img 02]");
    assert_eq!(
        *pedido_de(&v[0]),
        json!({ "pixpin": 1, "accion": "anadir_tarea", "texto": "leer [img 01]", "proyecto": null, "codigo": "t1",
            "imagenes": [b.to_string_lossy()] })
    );
    // Y en «tareas <texto>» la ficha hace que sea una tarea del Inbox.
    let v = buscar_con(&r, "tareas leer [img 01]", imagenes::SIN_PORTAPAPELES);
    assert_eq!(
        titulos(&v),
        [
            "Añadir al Inbox: leer [img 01]",
            "Abrir la ventana de Tareas"
        ]
    );
    assert_eq!(pedido_de(&v[0])["imagenes"], json!([a.to_string_lossy()]));
}

#[test]
fn caso_negativo_una_ficha_desconocida_se_queda_como_texto() {
    let r = raiz_con_inbox("pegar-desconocida");
    // Escrita a mano, sin borrador: el pedido de siempre, y se avisa.
    let v = buscar_con(&r, "t foto [img 05]", imagenes::SIN_PORTAPAPELES);
    assert_eq!(
        *pedido_de(&v[0]),
        json!({ "pixpin": 1, "accion": "anadir_tarea", "texto": "foto [img 05]" })
    );
    assert!(
        v[0].subtitulo
            .starts_with("⚠ [img 05] no se encuentra: se queda como texto"),
        "{}",
        v[0].subtitulo
    );
    assert!(v[0].resaltado.is_empty() && v[0].vista_previa.is_none());
    // Una conocida y otra no: va la conocida; la otra, tal cual.
    let ahora = crate::datos::ahora_ms();
    let dir = imagenes::carpeta(&r);
    fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.png");
    fs::write(&a, b"a").unwrap();
    imagenes::apuntar(&r, 1, &a, None, ahora);
    let v = buscar_con(&r, "t foto [img 01] [img 07]", imagenes::SIN_PORTAPAPELES);
    assert_eq!(pedido_de(&v[0])["imagenes"], json!([a.to_string_lossy()]));
    assert_eq!(pedido_de(&v[0])["texto"], "foto [img 01] [img 07]");
    assert!(
        v[0].subtitulo
            .starts_with("📎 1 imagen · ⚠ [img 07] no se encuentra"),
        "{}",
        v[0].subtitulo
    );
    // Un borrador caducado no vale, ni una imagen que ya no esta.
    assert!(imagenes::leer_borrador(&r, ahora + imagenes::CADUCA_MS).is_none());
    fs::remove_file(&a).unwrap();
    let v = buscar_con(&r, "t foto [img 01]", imagenes::SIN_PORTAPAPELES);
    assert!(pedido_de(&v[0]).get("imagenes").is_none());
}

#[test]
fn caso_negativo_si_la_imagen_ya_no_esta_al_pegar_se_avisa() {
    let r = raiz_con_inbox("pegar-se-fue");
    let mut p = plugin(&r);
    p.portapapeles = SE_FUE;
    p.atender(&consulta(1, "t comprar"));
    let pegar = ultima_lista(&p)[0].clone();
    intro(&mut p, 2, &pegar);
    let s = salida(&p);
    let aviso = &s[s.len() - 2];
    assert_eq!(aviso["method"], "ShowMsg");
    assert_eq!(
        aviso["params"][1],
        "No hay ninguna imagen en el portapapeles"
    );
    assert!(!s.iter().any(|l| l["method"] == "ChangeQuery"));
    assert!(imagenes::leer_borrador(&r, crate::datos::ahora_ms()).is_none());
}

#[test]
fn pegar_en_el_modo_v1_cambia_la_consulta() {
    let r = raiz_con_inbox("pegar-v1");
    let mut p = plugin(&r);
    p.portapapeles = CON_IMAGEN;
    let res = crate::rpc::una_vez(
        &mut p,
        &json!({ "method": "pegar_imagen", "parameters": ["p t pan [img 01] ", 1] }),
    );
    assert_eq!(
        res,
        json!({ "method": "Flow.Launcher.ChangeQuery", "parameters": ["p t pan [img 01] ", true] })
    );
    assert!(
        imagenes::leer_borrador(&r, crate::datos::ahora_ms())
            .is_some_and(|b| b.imagenes.contains_key(&1))
    );
}

#[test]
fn con_la_app_abierta_no_sale_pegar_porque_pega_ella_con_ctrl_v() {
    let r = raiz_con_inbox("pegar-con-app");
    let v = buscar_con(&r, "t comprar", CON_LA_APP);
    assert_eq!(v[0].titulo, "Apuntar tarea: comprar");
    assert!(
        !v.iter()
            .any(|x| matches!(x.accion, Accion::PegarImagen { .. }))
    );
    // Caso negativo: con la app cerrada, si.
    assert!(matches!(
        buscar_con(&r, "t comprar", CON_IMAGEN)[0].accion,
        Accion::PegarImagen { .. }
    ));
}

#[test]
fn una_imagen_un_nombre_la_misma_no_se_pega_dos_veces() {
    let r = raiz_con_inbox("pegar-una-vez");
    let ahora = crate::datos::ahora_ms();
    use imagenes::Pegado;
    // Como la app: sin numero, el siguiente del borrador.
    let Pegado::Nueva(1, a) = imagenes::pegar_en_borrador(&r, &CON_IMAGEN, None, ahora) else {
        panic!()
    };
    assert!(a.is_file() && a.extension().unwrap() == "png");
    assert_eq!(
        imagenes::pegar_en_borrador(&r, &CON_IMAGEN, None, ahora),
        Pegado::Repetida(1)
    );
    // Copiada otra vez (otra secuencia): por el contenido, la misma.
    assert_eq!(
        imagenes::pegar_en_borrador(&r, &LA_MISMA_OTRA_VEZ, None, ahora),
        Pegado::Repetida(1)
    );
    let Pegado::Nueva(2, _) = imagenes::pegar_en_borrador(&r, &OTRA_IMAGEN, None, ahora) else {
        panic!()
    };
    let b = imagenes::leer_borrador(&r, ahora).unwrap();
    assert_eq!(b.imagenes.len(), 2);
    assert_eq!(b.secuencias, [7, 9, 8]);
    let ficheros = fs::read_dir(imagenes::carpeta(&r)).unwrap().count();
    assert_eq!(ficheros, 3, "dos imagenes y el borrador");
    // Caso negativo: la ficha 1 empieza un borrador nuevo (una tarea nueva),
    // y ahi la misma imagen si se pega; sin imagen, nada.
    let Pegado::Nueva(1, _) = imagenes::pegar_en_borrador(&r, &CON_IMAGEN, Some(1), ahora) else {
        panic!()
    };
    assert_eq!(
        imagenes::leer_borrador(&r, ahora).unwrap().imagenes.len(),
        1
    );
    assert_eq!(
        imagenes::pegar_en_borrador(&r, &SE_FUE, None, ahora),
        Pegado::Nada
    );
    assert_eq!(
        imagenes::pegar_en_borrador(&r, &imagenes::SIN_PORTAPAPELES, None, ahora),
        Pegado::Nada
    );
    // Un borrador caducado tampoco cuenta.
    let tarde = ahora + imagenes::CADUCA_MS;
    assert!(matches!(
        imagenes::pegar_en_borrador(&r, &CON_IMAGEN, None, tarde),
        Pegado::Nueva(1, _)
    ));
}

#[test]
fn el_borrador_olvida_las_fichas_que_ya_no_estan_en_la_caja() {
    let r = raiz_con_inbox("podar");
    let ahora = crate::datos::ahora_ms();
    imagenes::pegar_en_borrador(&r, &CON_IMAGEN, None, ahora);
    imagenes::pegar_en_borrador(&r, &OTRA_IMAGEN, None, ahora);
    // Justo despues de pegar no se toca (la app aun esta escribiendo).
    imagenes::podar(&r, "t comp", ahora + 100);
    assert_eq!(
        imagenes::leer_borrador(&r, ahora).unwrap().imagenes.len(),
        2
    );
    let luego = ahora + imagenes::GRACIA_PODAR_MS;
    // Con las dos fichas puestas, nada cambia.
    imagenes::podar(&r, "t a [img 01] b [img 02]", luego);
    assert_eq!(
        imagenes::leer_borrador(&r, luego).unwrap().imagenes.len(),
        2
    );
    // Borrada la primera: se olvida, y su imagen se puede volver a pegar.
    imagenes::podar(&r, "t a b [img 02]", luego);
    let b = imagenes::leer_borrador(&r, luego).unwrap();
    assert_eq!(b.imagenes.keys().copied().collect::<Vec<_>>(), [2]);
    assert!(b.secuencias.is_empty());
    assert!(matches!(
        imagenes::pegar_en_borrador(&r, &CON_IMAGEN, None, luego),
        imagenes::Pegado::Nueva(3, _)
    ));
    // Una caja vacia (Flow abierto de nuevo): el borrador entero fuera.
    imagenes::podar(&r, "", luego + imagenes::GRACIA_PODAR_MS);
    assert!(imagenes::leer_borrador(&r, luego).is_none());
    // Caso negativo: sin borrador no pasa nada.
    imagenes::podar(&r, "", luego);
}

#[test]
fn el_formato_del_borrador_es_el_que_leen_la_app_y_el_plugin() {
    // El que escribia el plugin antes (sin huellas) sigue valiendo.
    let viejo = r#"{"tocado":5,"secuencia":7,"imagenes":{"1":"C:\\a.png"}}"#;
    let b: imagenes::Borrador = serde_json::from_str(viejo).unwrap();
    assert_eq!(b.imagenes[&1], "C:\\a.png");
    assert!(b.huellas.is_empty() && b.secuencias.is_empty());
    // Y el de ahora, campo a campo.
    let mut b = imagenes::Borrador {
        tocado: 5,
        secuencia: Some(7),
        ..Default::default()
    };
    b.imagenes.insert(1, "C:\\a.png".into());
    b.huellas.insert(1, imagenes::huella(b"abc"));
    b.secuencias.push(7);
    let j = serde_json::to_value(&b).unwrap();
    assert_eq!(
        j,
        json!({ "tocado": 5, "secuencia": 7, "imagenes": { "1": "C:\\a.png" },
            "huellas": { "1": 0xe71f_a219_0541_574b_u64 }, "secuencias": [7] })
    );
    assert_eq!(serde_json::from_value::<imagenes::Borrador>(j).unwrap(), b);
    // La huella es fija (FNV-1a): la del texto vacio es su base.
    assert_eq!(imagenes::huella(b""), 0xcbf2_9ce4_8422_2325);
    assert_ne!(
        imagenes::huella(b"abc"),
        imagenes::huella(b"abd"),
        "caso negativo"
    );
}

#[test]
fn los_ficheros_copiados_y_el_bmp_de_un_dib() {
    // DROPFILES: pFiles = 20, fWide = 1, y dos rutas en UTF-16.
    let mut b = vec![0u8; 20];
    b[0] = 20;
    b[16] = 1;
    for c in "C:\\a.png\0D:\\b c.jpg\0\0".encode_utf16() {
        b.extend_from_slice(&c.to_le_bytes());
    }
    assert_eq!(
        imagenes::ficheros_de_drop(&b),
        [PathBuf::from("C:\\a.png"), PathBuf::from("D:\\b c.jpg")]
    );
    assert!(
        imagenes::ficheros_de_drop(&[1, 2]).is_empty(),
        "caso negativo: roto"
    );
    assert!(
        imagenes::es_imagen_pegable(Path::new("x.JPEG"))
            && !imagenes::es_imagen_pegable(Path::new("x.pdf"))
    );
    // Un DIB de 1x1 a 24 bits: cabecera de 40 y 4 bytes de pixel.
    let mut dib = vec![0u8; 44];
    dib[0] = 40;
    dib[4] = 1;
    dib[8] = 1;
    dib[12] = 1;
    dib[14] = 24;
    let bmp = imagenes::bmp_de_dib(&dib).unwrap();
    assert_eq!(&bmp[..2], b"BM");
    assert_eq!(u32::from_le_bytes(bmp[2..6].try_into().unwrap()), 58);
    assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 54);
    assert!(
        imagenes::bmp_de_dib(&[0; 8]).is_none(),
        "caso negativo: corto"
    );
}

#[test]
fn una_tarea_con_imagenes_ensena_img_01_y_no_el_enlace() {
    let (t, v) = crate::datos::sin_imagenes(
        "yeso ![img 01](pixpin:files/guardados/pc/general/archivos/tarea-1-01.png) ya",
    );
    assert_eq!(t, "yeso [img 01] ya");
    assert_eq!(
        v,
        ["pixpin:files/guardados/pc/general/archivos/tarea-1-01.png"]
    );
    let tareas =
        crate::datos::leer_tareas("- [ ] yeso ![img 01](pixpin:files/a.png) ➕ 2026-10-03");
    assert_eq!(tareas[0].texto, "yeso [img 01]");
    assert_eq!(tareas[0].imagenes.len(), 1);
    assert!(tareas[0].creada.is_some());
    // Caso negativo: un enlace mal formado (con blancos) se queda como texto.
    let (t, v) = crate::datos::sin_imagenes("mira ![x](con blanco.png)");
    assert_eq!(t, "mira ![x](con blanco.png)");
    assert!(v.is_empty());
}

/// **Lo que cuesta una consulta, por partes**, con una copia de los datos
/// de verdad (`PIXPIN_MEDIR_RAIZ`, una carpeta con `proyectos` y `cache`):
/// `cargo test -p pixpin-lanzador medir_por_partes -- --ignored --nocapture`.
/// Cada vuelta es como un proceso nuevo del modo v1 (leer la cache, mirar el
/// disco, buscar, escribir el JSON, guardar la cache).
#[test]
#[ignore]
fn medir_por_partes() {
    use std::time::{Duration, Instant};
    let Some(r) = std::env::var_os("PIXPIN_MEDIR_RAIZ").map(PathBuf::from) else {
        return;
    };
    let mediana = |mut v: Vec<Duration>| {
        v.sort();
        v[v.len() / 2]
    };
    for q in ["", "tarea", "lec", "notas pdf", "zzzz"] {
        let mut partes: Vec<Vec<Duration>> = vec![Vec::new(); 6];
        for _ in 0..22 {
            let t0 = Instant::now();
            let mut d = Datos::nuevo(r.clone()).con_cache_en_disco();
            d.vigilar = false;
            let t1 = Instant::now();
            crate::imagenes::podar(&r, q, crate::datos::ahora_ms());
            let ps = d.proyectos();
            let t2 = Instant::now();
            let mut c = Contexto::nuevo("p");
            c.iconos = Some(crate::iconos::carpeta(&r));
            c.raiz = Some(r.clone());
            let v = resultados(&ps, q, &c);
            let t3 = Instant::now();
            let json = Value::Array(crate::resultados::lista_json(&v, "x.png")).to_string();
            let t4 = Instant::now();
            let rel = d.relecturas;
            d.guardar_cache();
            let t5 = Instant::now();
            if std::env::var_os("PIXPIN_MEDIR_DETALLE").is_some() {
                println!(
                    "  relecturas {rel} guardar {:?} cache {:?}",
                    t5 - t4,
                    t1 - t0
                );
            }
            std::hint::black_box(json);
            for (i, t) in [t1 - t0, t2 - t1, t3 - t2, t4 - t3, t5 - t4, t5 - t0]
                .into_iter()
                .enumerate()
            {
                partes[i].push(t);
            }
        }
        let m: Vec<String> = partes
            .into_iter()
            .map(|p| format!("{:.2}", mediana(p).as_secs_f64() * 1000.0))
            .collect();
        println!(
            "{:<12} cache {} | disco {} | buscar {} | json {} | guardar {} | total {} ms",
            format!("'{q}'"),
            m[0],
            m[1],
            m[2],
            m[3],
            m[4],
            m[5]
        );
    }
}

// --- Ficheros pegados (las fichas `[archivo NN]`) ----------------------------

#[test]
fn las_fichas_de_archivo_se_leen_y_se_quitan_del_texto() {
    let t = "mira [archivo 01] y [ARCHIVO 3] [img 02] [archivo 0] [archivo x]";
    let f = imagenes::fichas_de_archivo(t);
    assert_eq!(f.iter().map(|f| f.numero).collect::<Vec<_>>(), [1, 3]);
    assert_eq!(&t[f[0].inicio..f[0].fin], "[archivo 01]");
    assert_eq!(imagenes::ficha_de_archivo(4), "[archivo 04]");
    // Las de imagen siguen siendo solo las de imagen.
    assert_eq!(imagenes::fichas(t).len(), 1);
    assert_eq!(
        imagenes::sin_fichas(t),
        "mira y [archivo 0] [archivo x]",
        "las que no son fichas se quedan"
    );
}

/// Unos ficheros «copiados en el Explorador», fuera de la raiz.
fn ficheros_copiados(r: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let fuera = r.join("escritorio");
    fs::create_dir_all(fuera.join("carpeta")).unwrap();
    let pdf = fuera.join("informe.pdf");
    let xlsx = fuera.join("datos.xlsx");
    let png = fuera.join("foto.png");
    fs::write(&pdf, b"%PDF").unwrap();
    fs::write(&xlsx, b"PK").unwrap();
    fs::write(&png, b"\x89PNG copiada en el explorador").unwrap();
    (pdf, xlsx, png, fuera.join("carpeta"))
}

#[test]
fn pegar_ficheros_los_apunta_y_el_chat_los_lleva_como_adjuntos() {
    use imagenes::{Pegado, PegadoFichero};
    let r = raiz("pegar-ficheros");
    let (pdf, xlsx, png, _) = ficheros_copiados(&r);
    let ahora = crate::datos::ahora_ms();
    let v = imagenes::pegar_ficheros_en_borrador(&r, &[pdf.clone(), png.clone()], Some(9), ahora);
    assert_eq!(
        v[0],
        PegadoFichero::Archivo {
            numero: 1,
            nuevo: true
        }
    );
    // Una imagen copiada como fichero sigue siendo `[img NN]`, con su copia.
    let PegadoFichero::Imagen(Pegado::Nueva(1, copia)) = &v[1] else {
        panic!("{v:?}")
    };
    assert!(copia.is_file() && copia.starts_with(imagenes::carpeta(&r)));
    let fichas: Vec<String> = v.iter().filter_map(PegadoFichero::ficha_nueva).collect();
    assert_eq!(fichas, ["[archivo 01]", "[img 01]"]);
    // Otro fichero despues: el siguiente numero.
    let v = imagenes::pegar_ficheros_en_borrador(&r, std::slice::from_ref(&xlsx), None, ahora);
    assert_eq!(
        v,
        [PegadoFichero::Archivo {
            numero: 2,
            nuevo: true
        }]
    );

    // En el chat de un proyecto, el mensaje lleva los dos y la imagen.
    let v = buscar_con(
        &r,
        "Gestión de proyectos > mira [archivo 02] [img 01] y [archivo 01]",
        imagenes::SIN_PORTAPAPELES,
    );
    let escribir = &v[0];
    assert!(
        escribir
            .titulo
            .starts_with("Escribir en «Gestión de proyectos»"),
        "{}",
        escribir.titulo
    );
    let p = pedido_de(escribir);
    assert_eq!(p["accion"], "chat");
    assert_eq!(p["proyecto"], "P1");
    assert_eq!(p["texto"], "mira [archivo 02] [img 01] y [archivo 01]");
    assert_eq!(
        p["archivos"],
        json!([pdf.to_string_lossy(), xlsx.to_string_lossy()])
    );
    assert_eq!(p["imagenes"], json!([copia.to_string_lossy()]));
    // En el subtitulo, el nombre de cada fichero; F1, la imagen.
    assert!(
        escribir.subtitulo.contains("informe.pdf, datos.xlsx"),
        "{}",
        escribir.subtitulo
    );
    assert_eq!(
        escribir.vista_previa.as_deref(),
        Some(&*copia.to_string_lossy())
    );
    // Las tres fichas, resaltadas.
    assert_eq!(
        imagenes::fichas(&escribir.titulo).len()
            + imagenes::fichas_de_archivo(&escribir.titulo).len(),
        3
    );
    let marcas = escribir.resaltado.len();
    assert_eq!(marcas, "[archivo 02]".len() * 2 + "[img 01]".len());
}

#[test]
fn caso_negativo_el_mismo_fichero_dos_veces_una_carpeta_o_uno_que_no_esta() {
    use imagenes::PegadoFichero;
    let r = raiz("pegar-ficheros-no");
    let (pdf, _, _, carpeta) = ficheros_copiados(&r);
    let no_esta = r.join("escritorio").join("borrado.docx");
    let ahora = crate::datos::ahora_ms();
    // La misma ruta escrita de otra forma es el mismo fichero.
    let otra_forma = PathBuf::from(pdf.to_string_lossy().to_uppercase().replace('\\', "/"));
    let v = imagenes::pegar_ficheros_en_borrador(
        &r,
        &[pdf.clone(), carpeta.clone(), no_esta.clone(), otra_forma],
        None,
        ahora,
    );
    assert_eq!(
        v,
        [
            PegadoFichero::Archivo {
                numero: 1,
                nuevo: true
            },
            PegadoFichero::NoVale(carpeta),
            PegadoFichero::NoVale(no_esta),
            PegadoFichero::Archivo {
                numero: 1,
                nuevo: false
            },
        ]
    );
    assert!(v.iter().skip(1).all(|x| x.ficha_nueva().is_none()));
    // Y pegarlo de nuevo, despues, tampoco escribe nada.
    let v = imagenes::pegar_ficheros_en_borrador(&r, std::slice::from_ref(&pdf), None, ahora);
    assert_eq!(v[0].ficha_nueva(), None);
    let b = imagenes::leer_borrador(&r, ahora).unwrap();
    assert_eq!(b.archivos.len(), 1);

    // Un fichero que se borro despues de pegarlo no va: su ficha se queda
    // como texto y se avisa.
    fs::remove_file(&pdf).unwrap();
    let v = buscar_con(
        &r,
        "Thesis > ahi va [archivo 01]",
        imagenes::SIN_PORTAPAPELES,
    );
    let p = pedido_de(&v[0]);
    assert!(p.get("archivos").is_none(), "{p}");
    assert!(
        v[0].subtitulo.contains("[archivo 01] no se encuentra"),
        "{}",
        v[0].subtitulo
    );
}

#[test]
fn caso_negativo_en_una_tarea_los_ficheros_no_van_y_se_avisa() {
    let r = raiz_con_inbox("pegar-ficheros-tarea");
    let (pdf, _, _, _) = ficheros_copiados(&r);
    let ahora = crate::datos::ahora_ms();
    imagenes::pegar_ficheros_en_borrador(&r, &[pdf], None, ahora);
    let v = buscar_con(&r, "t revisar [archivo 01]", imagenes::SIN_PORTAPAPELES);
    let p = pedido_de(&v[0]);
    assert_eq!(p["accion"], "anadir_tarea");
    assert!(p.get("archivos").is_none(), "{p}");
    assert_eq!(p["texto"], "revisar [archivo 01]");
    assert!(
        v[0].subtitulo.contains("solo van en un mensaje del chat"),
        "{}",
        v[0].subtitulo
    );
}

#[test]
fn el_borrador_olvida_los_ficheros_que_ya_no_estan_en_la_caja() {
    let r = raiz("podar-ficheros");
    let (pdf, xlsx, _, _) = ficheros_copiados(&r);
    let ahora = crate::datos::ahora_ms();
    imagenes::pegar_ficheros_en_borrador(&r, &[pdf, xlsx], None, ahora);
    let despues = ahora + imagenes::GRACIA_PODAR_MS + 1;
    imagenes::podar(&r, "Thesis > hola [archivo 02]", despues);
    let b = imagenes::leer_borrador(&r, despues).unwrap();
    assert_eq!(b.archivos.keys().copied().collect::<Vec<_>>(), [2]);
    // Sin ninguna ficha, el borrador se va (y el siguiente empieza en 01).
    imagenes::podar(&r, "Thesis > hola", despues);
    assert!(imagenes::leer_borrador(&r, despues).is_none());
}

// --- Enviar al movil (6-oct) -------------------------------------------------

/// Este PC (`pc`) en un grupo con dos moviles; solo `tel` ha contestado
/// alguna vez (tiene direccion apuntada).
fn raiz_en_grupo(etiqueta: &str, codigo: Option<&str>) -> PathBuf {
    let r = raiz(etiqueta);
    let s = r.join("sincro");
    fs::create_dir_all(&s).unwrap();
    fs::write(
        s.join("identidad.json"),
        json!({
            "yo": { "id": "pc", "nombre": "Portatil", "letra": "A" },
            "codigo": codigo,
            "miembros": [
                { "id": "pc", "nombre": "Portatil", "letra": "A" },
                { "id": "tab", "nombre": "Tableta", "letra": "C", "desde": 5 },
                { "id": "tel", "nombre": "Pixel de Max", "letra": "B", "otroCampo": 1 }
            ]
        })
        .to_string(),
    )
    .unwrap();
    fs::write(s.join("direcciones.txt"), "tel\t192.168.1.20\t47474\n").unwrap();
    r
}

/// Una imagen pegada como `[img 01]` en el borrador de ahora.
fn pegar_una(r: &Path) -> PathBuf {
    let img = imagenes::carpeta(r).join("pegada.png");
    fs::create_dir_all(img.parent().unwrap()).unwrap();
    fs::write(&img, b"png").unwrap();
    assert!(imagenes::apuntar(
        r,
        1,
        &img,
        None,
        crate::datos::ahora_ms()
    ));
    img
}

#[test]
fn con_una_imagen_pegada_sale_un_movil_por_aparato_del_grupo() {
    let r = raiz_en_grupo("movil-aparatos", Some("ABCD2345"));
    let img = pegar_una(&r);
    let v = buscar_con(&r, "movil [img 01]", imagenes::SIN_PORTAPAPELES);
    let titulos: Vec<&str> = v.iter().map(|x| x.titulo.as_str()).collect();
    // Este PC no sale; el que ya contesto alguna vez, primero.
    assert_eq!(
        titulos,
        [
            "Enviar a Pixel de Max · al lienzo abierto",
            "Enviar a Tableta · al lienzo abierto"
        ]
    );
    let p = pedido_de(&v[0]);
    assert_eq!(p["accion"], "enviar_al_movil");
    assert_eq!(p["aparato"], "tel");
    assert_eq!(p["imagenes"], json!([img.to_string_lossy()]));
    assert!(
        v[0].subtitulo.starts_with("📎 1 imagen · Letra B"),
        "{}",
        v[0].subtitulo
    );
    assert!(
        v[1].subtitulo.contains("Aún no se ha conectado"),
        "{}",
        v[1].subtitulo
    );
    assert_eq!(v[0].glifo, crate::resultados::glifo::MOVIL);
    // La «m» y lo escrito detras eligen el movil por su nombre.
    let v = buscar_con(&r, "m [img 01] tabl", imagenes::SIN_PORTAPAPELES);
    assert_eq!(v.len(), 1);
    assert_eq!(pedido_de(&v[0])["aparato"], "tab");
}

#[test]
fn caso_negativo_sin_imagen_pegada_se_explica_como_pegarla() {
    let r = raiz_en_grupo("movil-sin-imagen", Some("ABCD2345"));
    let v = buscar_con(&r, "móvil", imagenes::SIN_PORTAPAPELES);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].titulo, "Pega una imagen con Ctrl+V");
    assert!(matches!(v[0].accion, Accion::Consulta(_)));
    // Con una imagen copiada y la app cerrada se ofrece pegarla arriba.
    let v = buscar_con(&r, "móvil", CON_IMAGEN);
    assert!(matches!(v[0].accion, Accion::PegarImagen { numero: 1, .. }));
    assert!(v[0].subtitulo.contains("móvil"), "{}", v[0].subtitulo);
    // Una ficha que ya no esta en el borrador no manda nada.
    let v = buscar_con(&r, "movil [img 03]", imagenes::SIN_PORTAPAPELES);
    assert_eq!(v.len(), 1);
    assert!(
        v[0].titulo.starts_with("Esa imagen ya no está"),
        "{}",
        v[0].titulo
    );
}

#[test]
fn caso_negativo_sin_grupo_se_ofrece_abrir_sincronizar() {
    let r = raiz_en_grupo("movil-sin-grupo", None);
    pegar_una(&r);
    let v = buscar_con(&r, "celular [img 01]", imagenes::SIN_PORTAPAPELES);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].titulo, "Este PC no está en un grupo");
    let p = pedido_de(&v[0]);
    assert_eq!(p["accion"], "ventana");
    assert_eq!(p["cual"], "sincronizar");
    // Y sin carpeta `sincro` tampoco hay grupo.
    let r = raiz("movil-sin-sincro");
    assert!(crate::al_movil::del_grupo(&r).is_none());
}

#[test]
fn enviar_al_movil_sale_en_la_lista_de_funciones() {
    let r = raiz("movil-funciones");
    let v = buscar_con(&r, "", imagenes::SIN_PORTAPAPELES);
    let f = v
        .iter()
        .find(|x| x.titulo == "Enviar al móvil")
        .expect("la funcion se ofrece");
    assert_eq!(f.accion, Accion::Consulta("pp móvil ".into()));
}
