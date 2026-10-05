//! El plugin de Flow Launcher: JSON-RPC 2.0 por stdin/stdout, un mensaje por
//! linea (`Executable_V2`; ver docs/investigacion/2026-10-01-flow-launcher.md
//! §2.3).
//!
//! - `initialize`, `query`, `context_menu`, `reload_data`, `close`, y los
//!   metodos de las acciones (`pedir`, `consulta`, `pedir_y_seguir`, `copiar`, `windows`,
//!   `carpeta`, `pegar_imagen`), cuyos parametros llegan envueltos: `[[p1, p2]]`.
//! - `$/cancelRequest` se ignora: se contesta tan rapido que no hace falta.
//! - El plugin tambien le pide cosas a Flow por el mismo canal
//!   (`ChangeQuery`, `ShowMsg`, `CopyToClipboard`, `OpenDirectory`). Sus
//!   respuestas llegan como lineas sin `method` y se ignoran.
//! - **Nada por stderr**: Flow lo toma como un fallo del plugin. El registro
//!   va a un fichero en la carpeta de ajustes del plugin.

use crate::datos::{self, Datos};
use crate::pedido::{Envio, Mensajero};
use crate::resultados::{self, Contexto};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// El registro, en un fichero (nunca en stderr).
#[derive(Debug, Default, Clone)]
pub struct Registro {
    pub ruta: Option<PathBuf>,
}

impl Registro {
    pub fn escribir(&self, texto: &str) {
        let Some(ruta) = &self.ruta else { return };
        // Que no crezca sin fin: pasado un cuarto de mega se empieza de nuevo.
        if std::fs::metadata(ruta)
            .map(|m| m.len() > 256 * 1024)
            .unwrap_or(false)
        {
            let _ = std::fs::remove_file(ruta);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(ruta)
        {
            let _ = writeln!(f, "{} {}", datos::ahora_ms(), texto);
        }
    }
}

pub struct Plugin<M: Mensajero, W: Write> {
    pub datos: Datos,
    pub mensajero: M,
    salida: W,
    pub registro: Registro,
    /// El icono de cada resultado (ruta absoluta).
    pub icono: String,
    /// La palabra clave de la ultima consulta (`pp`).
    palabra_clave: String,
    siguiente_id: u64,
    /// Cuanto se espera, como mucho, a que una lista nueva aparezca en disco.
    pub espera_lista: Duration,
    pub terminado: bool,
    /// Los pedidos que no corren prisa (los `iconos` que faltan), para
    /// mandarlos despues de contestar.
    avisos: Vec<Value>,
    /// En el modo v1 los manda quien escribe la respuesta (`lib.rs`), cuando
    /// ya la escribio: [`Plugin::mandar_avisos`].
    pub diferir_avisos: bool,
    /// El portapapeles (para pegar una imagen en una tarea). En las pruebas,
    /// uno que nunca tiene imagen.
    pub portapapeles: crate::imagenes::Portapapeles,
}

impl<M: Mensajero, W: Write> Plugin<M, W> {
    pub fn nuevo(datos: Datos, mensajero: M, salida: W, icono: String) -> Self {
        Plugin {
            datos,
            mensajero,
            salida,
            registro: Registro::default(),
            icono,
            palabra_clave: crate::PALABRA_CLAVE.into(),
            avisos: Vec::new(),
            diferir_avisos: false,
            portapapeles: crate::imagenes::SIN_PORTAPAPELES,
            siguiente_id: 1_000_000,
            espera_lista: Duration::from_millis(2500),
            terminado: false,
        }
    }

    pub fn salida(&self) -> &W {
        &self.salida
    }

    fn escribir(&mut self, v: &Value) {
        let mut linea = serde_json::to_string(v).unwrap_or_else(|_| "{}".into());
        linea.push('\n');
        let _ = self.salida.write_all(linea.as_bytes());
        let _ = self.salida.flush();
    }

    fn responder(&mut self, id: &Value, result: Value) {
        self.escribir(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    /// Pedirle algo a Flow (`ChangeQuery`, `ShowMsg`...). La respuesta se
    /// ignora cuando llegue.
    fn llamar_a_flow(&mut self, metodo: &str, params: Value) {
        let id = self.siguiente_id;
        self.siguiente_id += 1;
        self.escribir(&json!({ "jsonrpc": "2.0", "id": id, "method": metodo, "params": params }));
    }

    fn contexto(&self) -> Contexto {
        let mut c = Contexto::nuevo(&self.palabra_clave);
        c.iconos = Some(crate::iconos::carpeta(self.datos.raiz()));
        c.raiz = Some(self.datos.raiz().to_path_buf());
        c.portapapeles = self.portapapeles;
        c
    }

    /// Manda los avisos pendientes (sin arrancar la app).
    pub fn mandar_avisos(&mut self) {
        for a in std::mem::take(&mut self.avisos) {
            let texto = a.to_string();
            let e = self.mensajero.avisar(&texto);
            self.registro.escribir(&format!("aviso {texto} -> {e:?}"));
        }
    }

    /// Los avisos que quedan por mandar.
    pub fn avisos(&self) -> &[Value] {
        &self.avisos
    }

    /// Atiende una linea. Nunca entra en panico por lo que llegue.
    pub fn atender(&mut self, linea: &str) {
        let linea = linea.trim();
        if linea.is_empty() {
            return;
        }
        let msg: Value = match serde_json::from_str(linea) {
            Ok(v) => v,
            Err(e) => {
                self.registro
                    .escribir(&format!("linea rota ({e}): {linea}"));
                return;
            }
        };
        let Some(metodo) = msg.get("method").and_then(Value::as_str) else {
            // La respuesta de Flow a algo que le pedimos.
            if let Some(e) = msg.get("error") {
                self.registro
                    .escribir(&format!("Flow contesto con error: {e}"));
            }
            return;
        };
        let id = msg.get("id").cloned().filter(|i| !i.is_null());
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let resultado = match metodo {
            "$/cancelRequest" => return,
            "initialize" => {
                self.inicializar(&params);
                json!({ "hide": false })
            }
            "query" => {
                // Primero la respuesta; despues, sin prisa, los iconos.
                let r = self.consulta(&params);
                if let Some(id) = &id {
                    self.responder(id, r);
                }
                if !self.diferir_avisos {
                    self.mandar_avisos();
                }
                return;
            }
            "context_menu" => self.menu(&params),
            "reload_data" => {
                self.datos.olvidar();
                Value::Null
            }
            "close" => {
                self.terminado = true;
                Value::Null
            }
            "pedir" => {
                // Primero se contesta, para que Flow se cierre ya aunque haya
                // que esperar a que arranque la app.
                if let Some(id) = &id {
                    self.responder(id, json!({ "hide": true }));
                }
                let args = argumentos(&params);
                self.pedir(args.first());
                return;
            }
            "consulta" => {
                let args = argumentos(&params);
                if let Some(t) = args.first().and_then(Value::as_str) {
                    self.llamar_a_flow("ChangeQuery", json!([t, true]));
                }
                json!({ "hide": false })
            }
            "pedir_y_seguir" => {
                let args = argumentos(&params);
                self.pedir_y_seguir(args.first(), args.get(1).and_then(Value::as_str));
                json!({ "hide": false })
            }
            "pegar_imagen" => {
                let args = argumentos(&params);
                let numero = args.get(1).and_then(Value::as_u64).unwrap_or(1) as u32;
                self.pegar_imagen(args.first().and_then(Value::as_str), numero);
                json!({ "hide": false })
            }
            "copiar" => {
                let args = argumentos(&params);
                if let Some(t) = args.first().and_then(Value::as_str) {
                    self.llamar_a_flow("CopyToClipboard", json!([t, false, true]));
                }
                json!({ "hide": true })
            }
            "carpeta" => {
                let args = argumentos(&params);
                let carpeta = args.first().and_then(Value::as_str).unwrap_or("");
                let fichero = args
                    .get(1)
                    .and_then(Value::as_str)
                    .filter(|f| !f.is_empty());
                self.llamar_a_flow("OpenDirectory", json!([carpeta, fichero]));
                json!({ "hide": true })
            }
            "windows" => {
                let args = argumentos(&params);
                if let Some(r) = args.first().and_then(Value::as_str) {
                    let hecho = self.mensajero.abrir_con_windows(r);
                    self.registro
                        .escribir(&format!("abrir con Windows {r} -> {hecho}"));
                    if !hecho {
                        let icono = self.icono.clone();
                        self.llamar_a_flow(
                            "ShowMsg",
                            json!(["PixPin Max", format!("No se pudo abrir {r}"), icono]),
                        );
                    }
                }
                json!({ "hide": true })
            }
            otro => {
                self.registro
                    .escribir(&format!("metodo desconocido: {otro}"));
                if let Some(id) = &id {
                    self.escribir(&json!({ "jsonrpc": "2.0", "id": id,
                        "error": { "code": -32601, "message": format!("metodo desconocido: {otro}") } }));
                }
                return;
            }
        };
        if let Some(id) = &id {
            self.responder(id, resultado);
        }
    }

    fn inicializar(&mut self, params: &Value) {
        let meta = primero(params)
            .get("currentPluginMetadata")
            .cloned()
            .unwrap_or(Value::Null);
        if let Some(dir) = meta
            .get("pluginSettingsDirectoryPath")
            .and_then(Value::as_str)
        {
            let dir = PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            self.registro.ruta = Some(dir.join("pixpin-lanzador.log"));
            crate::poner_registro_de_panicos(self.registro.clone());
        }
        if let Some(dir) = meta.get("pluginDirectory").and_then(Value::as_str) {
            let png = PathBuf::from(dir).join("Images").join("pixpin.png");
            if png.is_file() {
                self.icono = png.to_string_lossy().to_string();
            }
        }
        self.registro.escribir(&format!(
            "iniciado; datos en {}",
            self.datos.raiz().display()
        ));
    }

    fn consulta(&mut self, params: &Value) -> Value {
        let q = primero(params);
        let k = q
            .get("actionKeyword")
            .and_then(Value::as_str)
            .unwrap_or(crate::PALABRA_CLAVE);
        self.palabra_clave = k.to_string();
        let busqueda = q.get("search").and_then(Value::as_str).unwrap_or("");
        let inicio = Instant::now();
        // Las imagenes pegadas cuya ficha ya no esta en la caja, olvidadas:
        // una tarea nueva vuelve a empezar en `[img 01]`.
        crate::imagenes::podar(self.datos.raiz(), busqueda, datos::ahora_ms());
        let proyectos = self.datos.proyectos();
        let ctx = self.contexto();
        let r = resultados::resultados(&proyectos, busqueda, &ctx);
        // Cada aviso puede esperar a la app hasta un cuarto de segundo: el
        // mismo icono no se vuelve a pedir en cada tecla.
        let faltan = self.datos.iconos_que_pedir(ctx.extensiones_que_faltan());
        if !faltan.is_empty() {
            self.avisos.push(crate::iconos::pedido(&faltan));
        }
        let t = inicio.elapsed();
        if t > Duration::from_millis(50) {
            self.registro
                .escribir(&format!("consulta «{busqueda}» tardo {t:?}"));
        }
        json!({ "result": resultados::lista_json(&r, &self.icono), "settingsChange": {}, "debugMessage": "" })
    }

    fn menu(&mut self, params: &Value) -> Value {
        // En versiones de Flow con el fallo #4686 llega `[null]`: lista vacia.
        let c = primero(params);
        let r = if c.is_object() {
            resultados::menu(c, &self.contexto())
        } else {
            Vec::new()
        };
        json!({ "result": resultados::lista_json(&r, &self.icono), "settingsChange": {}, "debugMessage": "" })
    }

    fn mandar(&mut self, pedido: &Value) -> Envio {
        let texto = pedido.to_string();
        let e = self.mensajero.enviar(&texto);
        self.registro.escribir(&format!("pedido {texto} -> {e:?}"));
        if e != Envio::Aceptado {
            let icono = self.icono.clone();
            self.llamar_a_flow("ShowMsg", json!(["PixPin Max", e.explicacion(), icono]));
        } else if pedido.get("imagenes").is_some() || pedido.get("archivos").is_some() {
            // Esa tarea (o ese mensaje) ya se llevo sus imagenes y ficheros:
            // lo siguiente empieza de cero.
            crate::imagenes::olvidar_borrador(self.datos.raiz());
        }
        e
    }

    /// «📎 Pegar la imagen copiada»: la guarda como la ficha `numero` del
    /// borrador y pone `consulta` (lo tecleado con la ficha) en Flow.
    fn pegar_imagen(&mut self, consulta: Option<&str>, numero: u32) {
        let Some(consulta) = consulta else { return };
        use crate::imagenes::Pegado;
        let raiz = self.datos.raiz().to_path_buf();
        let pegado = crate::imagenes::pegar_en_borrador(
            &raiz,
            &self.portapapeles,
            Some(numero),
            datos::ahora_ms(),
        );
        self.registro
            .escribir(&format!("pegar imagen {numero} -> {pegado:?}"));
        let icono = self.icono.clone();
        match pegado {
            Pegado::Nueva(..) => self.llamar_a_flow("ChangeQuery", json!([consulta, true])),
            // Una imagen, un nombre: la misma no se pega otra vez.
            Pegado::Repetida(n) => {
                let aviso = format!("Esa imagen ya está como {}", crate::imagenes::ficha(n));
                self.llamar_a_flow("ShowMsg", json!(["PixPin Max", aviso, icono]));
            }
            Pegado::Nada => {
                self.llamar_a_flow(
                    "ShowMsg",
                    json!([
                        "PixPin Max",
                        "No hay ninguna imagen en el portapapeles",
                        icono
                    ]),
                );
            }
        }
    }

    fn pedir(&mut self, pedido: Option<&Value>) {
        if let Some(p) = pedido.filter(|p| p.is_object()) {
            self.mandar(p);
        }
    }

    /// Las tareas: mandar el pedido, poner al dia la cache con lo que la app
    /// va a escribir, y volver a pedir la lista sin cerrar Flow.
    fn pedir_y_seguir(&mut self, pedido: Option<&Value>, consulta: Option<&str>) {
        let Some(p) = pedido.filter(|p| p.is_object()) else {
            return;
        };
        // Antes de mandarla: cuantas tareas tiene ya su lista, para saber
        // cuando la app la ha escrito (una mas al anadir, una menos al
        // quitar; y al borrar la lista, que ya no este).
        let espera = match p.get("accion").and_then(Value::as_str) {
            Some("anadir_tarea") => self.tareas_en_destino(p).map(Espera::Mas),
            Some("quitar_tarea") => self.tareas_en_destino(p).map(Espera::Menos),
            Some("borrar_lista") => Some(Espera::SinLista),
            _ => None,
        };
        let envio = self.mandar(p);
        if envio == Envio::Aceptado {
            // Una tarea nueva se espera en el DISCO, como una lista nueva:
            // adelantarla solo en memoria no bastaba. Si el vigia releia el
            // disco antes de que la app escribiera, la tarea se perdia de la
            // lista, y nada la volvia a pintar (el usuario: «agregue uno nuevo
            // y la lista no se actualiza»). Si la app tarda mas de la cuenta,
            // se adelanta en memoria como antes. Quitar y borrar, igual: si
            // no, la lista volveria a salir con lo que se acaba de quitar.
            let escrita = espera.is_some_and(|e| {
                let inicio = Instant::now();
                loop {
                    let hecho = match e {
                        Espera::Mas(n) => self.tareas_en_destino(p).is_some_and(|m| m > n),
                        Espera::Menos(n) => self.tareas_en_destino(p).is_some_and(|m| m < n),
                        Espera::SinLista => !self.lista_sigue(p),
                    };
                    if hecho {
                        break true;
                    }
                    if inicio.elapsed() >= self.espera_lista {
                        break false;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            });
            if !escrita {
                self.adelantar(p);
            }
        }
        if let Some(c) = consulta {
            self.llamar_a_flow("ChangeQuery", json!([c, true]));
        }
    }

    /// Cuantas tareas hay ahora en la lista de un pedido `anadir_tarea`: la
    /// de su `codigo`, o, sin el (el Inbox, que la app crea si falta), todas
    /// las del proyecto. `None` si no se sabe de que proyecto es.
    fn tareas_en_destino(&mut self, p: &Value) -> Option<usize> {
        let proyecto = match p.get("proyecto").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => self.datos.id_de_guardados()?,
        };
        let codigo = p
            .get("codigo")
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty());
        let ps = self.datos.proyectos();
        Some(
            resultados::listas(&ps)
                .iter()
                .filter(|l| ps[l.proyecto].id == proyecto && codigo.is_none_or(|c| l.codigo == c))
                .map(|l| l.tareas.len())
                .sum(),
        )
    }

    /// Si la lista `codigo` de un pedido (`borrar_lista`) sigue en el disco.
    fn lista_sigue(&mut self, p: &Value) -> bool {
        let proyecto = match p.get("proyecto").and_then(Value::as_str) {
            Some(id) => Some(id.to_string()),
            None => self.datos.id_de_guardados(),
        };
        let (Some(proyecto), Some(codigo)) = (proyecto, p.get("codigo").and_then(Value::as_str))
        else {
            return false;
        };
        let ps = self.datos.proyectos();
        resultados::listas(&ps)
            .iter()
            .any(|l| ps[l.proyecto].id == proyecto && l.codigo == codigo)
    }

    /// Lo que se espera que la app escriba, ya en la cache.
    fn adelantar(&mut self, p: &Value) {
        let proyecto = match p.get("proyecto").and_then(Value::as_str) {
            Some(id) => Some(id.to_string()),
            None => self.datos.id_de_guardados(),
        };
        let Some(proyecto) = proyecto else { return };
        let codigo = p.get("codigo").and_then(Value::as_str).unwrap_or("");
        match p.get("accion").and_then(Value::as_str) {
            Some("marcar_tarea") => {
                let indice = p.get("indice").and_then(Value::as_u64).unwrap_or(u64::MAX) as usize;
                let hecha = p.get("hecha").and_then(Value::as_bool).unwrap_or(true);
                let _ = self.datos.proyectos();
                self.datos.retocar(&proyecto, codigo, |d| {
                    datos::con_tarea_marcada(d, indice, hecha)
                });
            }
            Some("quitar_tarea") => {
                let indice = p.get("indice").and_then(Value::as_u64).unwrap_or(u64::MAX) as usize;
                let _ = self.datos.proyectos();
                self.datos
                    .retocar(&proyecto, codigo, |d| datos::con_tarea_quitada(d, indice));
            }
            Some("anadir_tarea") => {
                let texto = p
                    .get("texto")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let _ = self.datos.proyectos();
                self.datos
                    .retocar(&proyecto, codigo, |d| datos::con_tarea_anadida(d, &texto));
            }
            Some("lista_nueva") => {
                // Sin su codigo no se puede adelantar: se espera a que la app
                // la escriba (suele tardar unas decenas de ms).
                let titulo = crate::normalizar::normalizar(
                    p.get("titulo").and_then(Value::as_str).unwrap_or(""),
                );
                let inicio = Instant::now();
                while inicio.elapsed() < self.espera_lista {
                    let ps = self.datos.proyectos();
                    if resultados::listas(&ps).iter().any(|l| {
                        crate::normalizar::normalizar(&l.titulo) == titulo
                            && ps[l.proyecto].id == proyecto
                    }) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(80));
                }
            }
            _ => {}
        }
    }
}

/// Lo que se espera ver en el disco tras un pedido de tareas: cuantas tenia
/// su lista antes (para ver una mas o una menos), o que la lista ya no este.
#[derive(Debug, Clone, Copy)]
enum Espera {
    Mas(usize),
    Menos(usize),
    SinLista,
}

/// `[x, ...]` → `x`; lo demas, tal cual.
fn primero(params: &Value) -> &Value {
    match params {
        Value::Array(a) => a.first().unwrap_or(&Value::Null),
        otro => otro,
    }
}

/// Los parametros de una accion: Flow manda `[[p1, p2]]`; se acepta tambien
/// `[p1, p2]` por si alguna version no los envuelve.
fn argumentos(params: &Value) -> Vec<Value> {
    match params {
        Value::Array(a) if a.len() == 1 && a[0].is_array() => {
            a[0].as_array().cloned().unwrap_or_default()
        }
        Value::Array(a) => a.clone(),
        Value::Null => Vec::new(),
        otro => vec![otro.clone()],
    }
}

/// **El modo de un proceso por consulta** (`Executable`, el v1 de Flow): la
/// peticion llega como argumento (`{"method":"query","parameters":["texto"]}`)
/// y la respuesta sale por stdout, una sola. El usuario, 2-oct: Flow 2.1.4 lo
/// lanzaba asi y el plugin contestaba «orden desconocida». Se traduce al
/// mismo plugin de siempre: la peticion pasa a v2, y de lo que escribe se saca
/// el resultado (`{"result":[...]}`) o la llamada a Flow que pidio
/// (`{"method":"Flow.Launcher.ChangeQuery","parameters":[...]}`).
pub fn una_vez<M: Mensajero>(plugin: &mut Plugin<M, Vec<u8>>, peticion: &Value) -> Value {
    let metodo = peticion.get("method").and_then(Value::as_str).unwrap_or("");
    let parametros = peticion
        .get("parameters")
        .cloned()
        .unwrap_or(Value::Array(Vec::new()));
    let params = match metodo {
        "query" => {
            let texto = primero(&parametros).as_str().unwrap_or("").to_string();
            json!([{ "search": texto, "rawQuery": format!("{} {texto}", crate::PALABRA_CLAVE), "actionKeyword": crate::PALABRA_CLAVE }, {}])
        }
        "context_menu" => json!([primero(&parametros).clone()]),
        // Las acciones llegan sin envolver: `[p1, p2]`.
        _ => json!([parametros]),
    };
    plugin.atender(
        &json!({ "jsonrpc": "2.0", "id": 1, "method": metodo, "params": params }).to_string(),
    );
    // Tras tocar una tarea, la consulta de despues es otro proceso que lee el
    // disco: un respiro para que la app lo haya escrito.
    if metodo == "pedir_y_seguir" {
        std::thread::sleep(Duration::from_millis(350));
    }
    let salida = String::from_utf8_lossy(plugin.salida()).to_string();
    let mut respuesta = Value::Null;
    let mut llamada = Value::Null;
    for linea in salida.lines() {
        let Ok(v) = serde_json::from_str::<Value>(linea) else {
            continue;
        };
        match v.get("method").and_then(Value::as_str) {
            // La primera llamada a Flow es la que vale (solo cabe una).
            Some(m) if llamada.is_null() => {
                llamada = json!({ "method": format!("Flow.Launcher.{m}"), "parameters": v.get("params").cloned().unwrap_or(Value::Null) });
            }
            Some(_) => {}
            None if v.get("id") == Some(&json!(1)) => {
                respuesta = v.get("result").cloned().unwrap_or(Value::Null)
            }
            None => {}
        }
    }
    match metodo {
        "query" | "context_menu" => respuesta,
        _ if !llamada.is_null() => llamada,
        _ => json!({}),
    }
}

/// El bucle: lee lineas hasta que Flow cierre la entrada o mande `close`.
pub fn bucle<M: Mensajero, W: Write>(plugin: &mut Plugin<M, W>, mut entrada: impl BufRead) {
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        match entrada.read_until(b'\n', &mut bytes) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        // Un byte que no sea UTF-8 no puede tirar el plugin.
        plugin.atender(&String::from_utf8_lossy(&bytes));
        if plugin.terminado {
            break;
        }
    }
}
