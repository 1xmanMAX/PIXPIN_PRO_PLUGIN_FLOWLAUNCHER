//! **Calculator PROMAX dentro de PinDesk** (8-nov-2026).
//!
//! Calculator PROMAX es la otra app del autor: una calculadora cientifica y
//! algebraica al estilo de la Casio ClassPad, con su propio motor de algebra
//! (`promax-core`, copiado en `crates/promax-core`, ver su PROCEDENCIA.md).
//! Aqui esta lo que PinDesk necesita alrededor de ese motor, una sola vez
//! para los tres sitios donde se calcula:
//!
//! - **el chat**: una nota que empieza por `=` sale con su resultado debajo
//!   ([`nota_de_calculo`]);
//! - **el asistente**: la herramienta `calcular` ([`herramienta`],
//!   [`validar`], [`Calculo::para_el_modelo`]), para que las cuentas exactas
//!   las haga el motor y no el modelo de cabeza;
//! - **Flow Launcher**: `p = 2x+3=7` (el plugin, `apps/pindesk-lanzador`).
//!
//! Y lo comun: una ecuacion sola se resuelve sola (`2x+3=7` es
//! `solve(2x+3=7, x)`), las lineas comparten variables (`a := 5`), y cada
//! calculo corre en su hilo con su tiempo maximo y una pila grande
//! ([`calcular`]): un `factor` enorme no congela la app ni el lanzador.
//!
//! Lo que el motor NO tiene: pasos intermedios (da el resultado exacto y su
//! decimal) ni unidades. Los «pasos» de aqui ([`Calculo::pasos`]) son lo que
//! se hizo: como se leyo cada linea, si se resolvio para que incognita y el
//! resultado.
//!
//! La app completa no recibe nada al abrirla (ni argumentos ni un enlace
//! propio, comprobado en su `main.rs`): [`app`] la encuentra y la abre, y
//! quien la abre copia antes la expresion para pegarla alli.

pub mod app;

use std::sync::mpsc;
use std::time::Duration;

use promax_core::{Engine, Expr, OutKind};
use serde_json::{Map, Value, json};

/// El nombre de la herramienta del asistente.
pub const NOMBRE: &str = "calcular";
/// Lo mas largo que se calcula (letras).
pub const MAX_LETRAS: usize = 2_000;
/// Cuantas lineas como mucho en un calculo.
pub const MAX_LINEAS: usize = 24;
/// Lo que puede tardar un calculo antes de darlo por perdido.
pub const TIEMPO_MAXIMO: Duration = Duration::from_secs(3);
/// La pila del hilo que calcula: el motor es recursivo (el arbol de la
/// expresion), y la del hilo de una ventana es de 1 MB.
const PILA: usize = 64 * 1024 * 1024;

/// En que van los angulos de `sin`, `cos`... (`30°` siempre es en grados).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Angulos {
    #[default]
    Radianes,
    Grados,
}

/// Que fue una linea.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Que {
    /// Una cuenta o una expresion.
    Valor,
    /// Una ecuacion (o un sistema, o una desigualdad) que se resolvio sola
    /// para estas incognitas.
    Resuelta(Vec<String>),
    /// `a := 5`.
    Asignada(String),
    /// `f(x) := x^2`.
    Definida(String),
    /// `plot(...)`: aqui no se dibuja; se dice que funcion era.
    Grafica,
}

/// Una linea calculada.
#[derive(Debug, Clone, PartialEq)]
pub struct Linea {
    /// Lo escrito, tal cual.
    pub escrito: String,
    /// Como lo leyo el motor (`2x + 3 = 7`, `sqrt(8)`).
    pub leido: String,
    /// El resultado exacto (`1/2`, `2√2`, `{x = 2}`).
    pub exacto: String,
    /// Su decimal, si es distinto del exacto.
    pub decimal: Option<String>,
    pub que: Que,
}

impl Linea {
    /// `= 1/2  ≈ 0.5`, o las soluciones sin llaves (`x = 2`) si se resolvio.
    pub fn resultado(&self) -> String {
        let exacto = match &self.que {
            Que::Resuelta(_) => sin_llaves(&self.exacto),
            _ => self.exacto.clone(),
        };
        let mut s = match &self.que {
            Que::Resuelta(_) => exacto,
            Que::Asignada(n) => format!("{n} = {exacto}"),
            Que::Definida(n) => format!("{n} := {exacto}"),
            Que::Grafica | Que::Valor => format!("= {exacto}"),
        };
        if let Some(d) = &self.decimal {
            let d = match &self.que {
                Que::Resuelta(_) => sin_llaves(d),
                _ => d.clone(),
            };
            s.push_str(&format!("  ≈ {d}"));
        }
        s
    }
}

/// `{x = 2}` → `x = 2`, `{x = 2, y = 1}` → `x = 2, y = 1`. Una lista dentro
/// de otra se queda como esta.
fn sin_llaves(s: &str) -> String {
    match s.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        Some(dentro) if !dentro.contains('{') => dentro.to_string(),
        _ => s.to_string(),
    }
}

/// Un calculo entero (una o varias lineas, en orden).
#[derive(Debug, Clone, PartialEq)]
pub struct Calculo {
    pub lineas: Vec<Linea>,
}

impl Calculo {
    /// La ultima linea: la que se ensena como resultado.
    pub fn ultima(&self) -> &Linea {
        // `calcular` nunca devuelve un calculo sin lineas.
        &self.lineas[self.lineas.len() - 1]
    }

    /// El resultado en una linea (el de la ultima).
    pub fn resultado(&self) -> String {
        self.ultima().resultado()
    }

    /// Lo que se hizo, linea a linea: como se leyo, si se resolvio y el
    /// resultado. El motor no da pasos intermedios.
    pub fn pasos(&self) -> Vec<String> {
        let mut v = Vec::new();
        for l in &self.lineas {
            match &l.que {
                Que::Resuelta(inc) => {
                    v.push(format!("{}  → solve(…, {})", l.leido, inc.join(", ")))
                }
                _ => v.push(l.leido.clone()),
            }
            v.push(format!("  {}", l.resultado()));
        }
        v
    }

    /// **La respuesta para el modelo**: JSON corto con lo leido, el exacto y
    /// el decimal de cada linea. Lo leido va para que el modelo vea si se
    /// entendio otra cosa (`sin(30)` en radianes).
    pub fn para_el_modelo(&self) -> String {
        let lineas: Vec<Value> = self
            .lineas
            .iter()
            .map(|l| {
                let mut o = json!({ "leido": l.leido, "exacto": l.exacto });
                if let Some(d) = &l.decimal {
                    o["decimal"] = json!(d);
                }
                match &l.que {
                    Que::Resuelta(inc) => o["resuelto_para"] = json!(inc),
                    Que::Asignada(n) => o["guardado_en"] = json!(n),
                    Que::Definida(n) => o["funcion"] = json!(n),
                    Que::Grafica => o["nota"] = json!("aqui no se dibuja: usa graficar_funcion"),
                    Que::Valor => {}
                }
                o
            })
            .collect();
        let mut r = json!({ "resultado": self.ultima().exacto });
        if let Some(d) = &self.ultima().decimal {
            r["decimal"] = json!(d);
        }
        if lineas.len() > 1 || !matches!(self.ultima().que, Que::Valor) {
            r["lineas"] = json!(lineas);
        } else {
            r["leido"] = json!(self.ultima().leido);
        }
        r["motor"] = json!("Calculator PROMAX (exacto)");
        r.to_string()
    }
}

/// Lo que no salio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fallo {
    Vacio,
    /// Mas de [`MAX_LETRAS`] letras o de [`MAX_LINEAS`] lineas.
    Largo,
    /// El motor no lo entendio o no lo pudo calcular (`linea` desde 1).
    Motor {
        linea: usize,
        motivo: String,
    },
    /// Tardo mas de [`TIEMPO_MAXIMO`].
    Tarda,
}

impl std::fmt::Display for Fallo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fallo::Vacio => write!(f, "no hay nada que calcular"),
            Fallo::Largo => write!(
                f,
                "demasiado largo (hasta {MAX_LETRAS} letras y {MAX_LINEAS} lineas)"
            ),
            Fallo::Motor { linea: 1, motivo } => write!(f, "{motivo}"),
            Fallo::Motor { linea, motivo } => write!(f, "linea {linea}: {motivo}"),
            Fallo::Tarda => write!(f, "tarda demasiado (mas de {} s)", TIEMPO_MAXIMO.as_secs()),
        }
    }
}

/// **Calcula `texto`** (una expresion o varias lineas) en su propio hilo,
/// con [`TIEMPO_MAXIMO`]. Si se pasa, se devuelve [`Fallo::Tarda`] y el hilo
/// se deja acabar solo (el motor no se puede interrumpir).
pub fn calcular(texto: &str, angulos: Angulos) -> Result<Calculo, Fallo> {
    calcular_con_tiempo(texto, angulos, TIEMPO_MAXIMO)
}

/// [`calcular`] con otro tiempo maximo.
pub fn calcular_con_tiempo(
    texto: &str,
    angulos: Angulos,
    tiempo: Duration,
) -> Result<Calculo, Fallo> {
    let lineas = lineas_de(texto)?;
    let (tx, rx) = mpsc::channel();
    let hilo = std::thread::Builder::new()
        .name("calculadora".into())
        .stack_size(PILA)
        .spawn(move || {
            let _ = tx.send(calcular_aqui(&lineas, angulos));
        });
    if hilo.is_err() {
        // Sin hilo (memoria agotada), aqui mismo: es lo que se haria igual.
        return calcular_aqui(&lineas_de(texto)?, angulos);
    }
    match rx.recv_timeout(tiempo) {
        Ok(r) => r,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(Fallo::Tarda),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(Fallo::Motor {
            linea: 1,
            motivo: "el motor se detuvo".into(),
        }),
    }
}

/// Las lineas que se calculan: sin las vacias ni los comentarios (`#`).
fn lineas_de(texto: &str) -> Result<Vec<String>, Fallo> {
    if texto.chars().count() > MAX_LETRAS {
        return Err(Fallo::Largo);
    }
    let v: Vec<String> = texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();
    if v.is_empty() {
        return Err(Fallo::Vacio);
    }
    if v.len() > MAX_LINEAS {
        return Err(Fallo::Largo);
    }
    Ok(v)
}

fn calcular_aqui(lineas: &[String], angulos: Angulos) -> Result<Calculo, Fallo> {
    let mut motor = Engine::new();
    motor.settings.deg = angulos == Angulos::Grados;
    let mut hechas = Vec::with_capacity(lineas.len());
    for (k, escrito) in lineas.iter().enumerate() {
        let mal = |motivo: String| Fallo::Motor {
            linea: k + 1,
            motivo,
        };
        let (fuente, incognitas) = match incognitas_a_resolver(&motor, escrito) {
            Some(inc) => (resolver(escrito, &inc), Some(inc)),
            None => (escrito.clone(), None),
        };
        let salida = motor.eval_line(&fuente).map_err(mal)?;
        let que = match (&salida.kind, incognitas) {
            (_, Some(inc)) => Que::Resuelta(inc),
            (OutKind::Value, None) => Que::Valor,
            (OutKind::Assigned(n), None) => Que::Asignada(n.clone()),
            (OutKind::Defined(n), None) => Que::Definida(n.clone()),
            (OutKind::Plot(_), None) => Que::Grafica,
        };
        // Lo leido es lo escrito por el usuario, no el `solve(...)` de aqui.
        let leido = match &que {
            Que::Resuelta(_) => motor
                .parse(escrito)
                .map(|e| motor.print(&e))
                .unwrap_or_else(|_| escrito.clone()),
            _ => salida.input_pretty.clone(),
        };
        hechas.push(Linea {
            escrito: escrito.clone(),
            leido,
            exacto: salida.exact_str.clone(),
            decimal: salida.approx.clone(),
            que,
        });
    }
    Ok(Calculo { lineas: hechas })
}

/// `solve(<escrito>, x)` o `solve(<escrito>, {x, y})`.
fn resolver(escrito: &str, incognitas: &[String]) -> String {
    match incognitas {
        [x] => format!("solve({escrito}, {x})"),
        v => format!("solve({escrito}, {{{}}})", v.join(", ")),
    }
}

/// **Si la linea es una ecuacion que resolver**, y para que: una ecuacion o
/// desigualdad con una sola incognita, o un sistema `{…, …}` de ecuaciones
/// con tantas incognitas como ecuaciones. Las letras que ya tienen valor
/// (`a := 2`) no cuentan. `x = 2` tambien (sale `x = 2`): asi no hay casos
/// raros.
fn incognitas_a_resolver(motor: &Engine, escrito: &str) -> Option<Vec<String>> {
    if escrito.contains(":=") {
        return None;
    }
    let e = motor.parse(escrito).ok()?;
    let libres = |e: &Expr| {
        let mut v = Vec::new();
        e.symbols(&mut v);
        v.retain(|s| motor.get_var(s).is_none());
        v.sort();
        v
    };
    match &e {
        Expr::Eq(..) | Expr::Rel(..) => {
            let v = libres(&e);
            (v.len() == 1).then_some(v)
        }
        Expr::List(ecs) if ecs.len() > 1 && ecs.iter().all(|x| matches!(x, Expr::Eq(..))) => {
            let v = libres(&e);
            (v.len() == ecs.len()).then_some(v)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Lo escrito por una persona (el chat y Flow): solo lo que parece una cuenta

/// **Si lo escrito por una persona parece una cuenta** y no una frase: el
/// motor lee `hola que tal` como el producto `hola·que·tal`, y eso no puede
/// salir en el chat ni en Flow. Vale si todas sus letras libres son de una
/// sola letra (`x`, `y1`, `t_0`), una letra griega escrita (`theta`) o las
/// que define el propio calculo (`a := …`, `f(x) := …`).
pub fn parece_una_cuenta(texto: &str) -> bool {
    let Ok(lineas) = lineas_de(texto) else {
        return false;
    };
    let mut motor = Engine::new();
    let mut propias: Vec<String> = Vec::new();
    for l in &lineas {
        // `nombre := …` y `f(x, y) := …`: el nombre es suyo.
        let (delante, detras) = match l.split_once(":=") {
            Some((a, b)) => (Some(a.trim()), b),
            None => (None, l.as_str()),
        };
        let mut parametros = Vec::new();
        if let Some(d) = delante {
            let (nombre, params) = match d.split_once('(') {
                Some((n, p)) => (n.trim(), p.trim_end_matches(')')),
                None => (d, ""),
            };
            if !es_nombre(nombre) {
                return false;
            }
            propias.push(nombre.to_string());
            parametros.extend(params.split(',').map(|p| p.trim().to_string()));
        }
        // Lo que el motor no lee no es una cuenta (`=SUMA(A1:A3)` de Excel).
        let Ok(e) = motor.parse(detras) else {
            return false;
        };
        let mut libres = Vec::new();
        e.symbols(&mut libres);
        let ajena = libres
            .iter()
            .any(|s| !propias.contains(s) && !parametros.contains(s) && !es_letra_de_cuenta(s));
        if ajena {
            return false;
        }
        // Las de `a := 5` cuentan para las lineas de abajo.
        let _ = motor.eval_line(l);
    }
    true
}

fn es_nombre(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(char::is_alphabetic) && c.all(|c| c.is_alphanumeric() || c == '_')
}

/// `x`, `y1`, `t_0`, `θ` o una letra griega escrita.
fn es_letra_de_cuenta(s: &str) -> bool {
    const GRIEGAS: &[&str] = &[
        "alpha", "beta", "gamma", "delta", "epsilon", "theta", "lambda", "mu", "sigma", "omega",
        "phi", "rho", "tau",
    ];
    let mut c = s.chars();
    let primera = c.next().is_some_and(char::is_alphabetic);
    let resto: String = c.collect();
    (primera && resto.chars().all(|c| c.is_ascii_digit() || c == '_')) || GRIEGAS.contains(&s)
}

// ---------------------------------------------------------------------------
// El chat: `= …`

/// La flecha que separa la cuenta de su resultado en una nota.
pub const FLECHA: &str = "→ ";

/// **Si una nota es una cuenta para calcular**: empieza por `=` y detras
/// hay algo que parece una cuenta (`= 2x+3=7`, `=1/3+1/6`). No: `==`,
/// `=)`, `= hola`, ni una nota que ya trae su resultado. Devuelve la cuenta,
/// sin el `=`.
pub fn pedido_del_chat(texto: &str) -> Option<&str> {
    let t = texto.trim();
    let cuenta = t.strip_prefix('=')?.trim();
    if cuenta.is_empty() || cuenta.starts_with('=') || es_nota_de_calculo(t) {
        return None;
    }
    parece_una_cuenta(cuenta).then_some(cuenta)
}

/// **La nota de una cuenta del chat**, con su resultado: lo escrito tal cual
/// y debajo `→ x = 2` (o `→ = 1/2  ≈ 0.5`). `None` si no es una cuenta o no
/// se pudo calcular: entonces la nota sale como se escribio.
pub fn nota_de_calculo(texto: &str) -> Option<String> {
    let cuenta = pedido_del_chat(texto)?;
    let c = calcular(cuenta, Angulos::Radianes).ok()?;
    Some(format!("{}\n{FLECHA}{}", texto.trim(), c.resultado()))
}

/// Si la nota es una de [`nota_de_calculo`] (empieza por `=` y lleva su
/// linea `→ …`).
pub fn es_nota_de_calculo(texto: &str) -> bool {
    texto.trim_start().starts_with('=') && texto.lines().skip(1).any(|l| l.starts_with(FLECHA))
}

/// La cuenta de una nota de [`nota_de_calculo`] (sin el `=` ni el
/// resultado), para llevarla a Calculator PROMAX.
pub fn cuenta_de_la_nota(texto: &str) -> Option<String> {
    if !es_nota_de_calculo(texto) {
        return None;
    }
    let antes: Vec<&str> = texto
        .trim_start()
        .lines()
        .take_while(|l| !l.starts_with(FLECHA))
        .collect();
    let cuenta = antes.join("\n");
    let cuenta = cuenta.trim().strip_prefix('=')?.trim();
    (!cuenta.is_empty()).then(|| cuenta.to_string())
}

// ---------------------------------------------------------------------------
// La herramienta del asistente

/// La herramienta tal como se le describe al modelo: nombre, descripcion y
/// JSON Schema de los argumentos (`pindesk-ia` la pone en su catalogo).
pub fn herramienta() -> (&'static str, &'static str, Value) {
    (
        NOMBRE,
        concat!(
            "Calcula EXACTO con el motor de algebra de Calculator PROMAX (fracciones, raices y pi sin redondear, y su decimal). ",
            "Usala para CUALQUIER cuenta en vez de hacerla de cabeza, y repite el resultado tal cual. ",
            "Sabe: aritmetica (1/3+1/6, 2^100, 10!, 15% * 80), simplify, expand, factor (numeros y polinomios), ",
            "ecuaciones y sistemas (una ecuacion sola se resuelve: 2x+3=7; {x+y=3, x-y=1}; x^2<4), nsolve(f=g, x, x0), ",
            "diff(f, x), integrate(f, x) e integrate(f, x, a, b), limit(f, x, a), taylor(f, x, a, n), sum(f, k, a, b), ",
            "matrices [[1,2],[3,4]] con det, inv, rref, eigvals; media, mediana, varianza, desviacion; ncr, npr, gcd, lcm, isprime; ",
            "complejos (i); grados con 30° o «angulos»:«grados». Varias lineas: a := 5 y f(x) := x^2 valen para las siguientes. ",
            "Sin unidades: convierte tu las unidades antes."
        ),
        json!({
            "type": "object",
            "required": ["expresion"],
            "properties": {
                "expresion": {"type": "string", "description": "la cuenta, con la sintaxis de una calculadora (* / ^, sqrt, sin, ln, pi, e); varias lineas separadas por \\n"},
                "angulos": {"type": "string", "enum": ["radianes", "grados"], "description": "omitir: radianes"}
            }
        }),
    )
}

/// Una llamada ya validada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pedido {
    pub expresion: String,
    pub angulos: Angulos,
}

/// **Valida los argumentos de `calcular`.** Acepta tambien la cuenta en
/// otro campo de texto (los modelos a veces la llaman `expr` o `cuenta`).
pub fn validar(args: &Map<String, Value>) -> Result<Pedido, String> {
    let expresion = ["expresion", "expr", "cuenta", "expression"]
        .iter()
        .find_map(|k| args.get(*k).and_then(Value::as_str))
        .or_else(|| {
            args.iter()
                .find(|(k, _)| *k != "angulos")
                .and_then(|(_, v)| v.as_str())
        })
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("falta «expresion»: la cuenta que calcular")?;
    if expresion.chars().count() > MAX_LETRAS {
        return Err(format!(
            "«expresion» es demasiado larga (hasta {MAX_LETRAS} letras)"
        ));
    }
    let angulos = match args.get("angulos").and_then(Value::as_str).map(str::trim) {
        None | Some("") | Some("radianes") | Some("rad") => Angulos::Radianes,
        Some("grados") | Some("deg") | Some("°") => Angulos::Grados,
        Some(otro) => {
            return Err(format!("«angulos» = «{otro}» no vale: radianes o grados"));
        }
    };
    Ok(Pedido {
        expresion: expresion.to_string(),
        angulos,
    })
}

/// **Hace la llamada**: el JSON para el modelo o el motivo del fallo (que
/// el modelo lee para corregir la sintaxis).
pub fn hacer(p: &Pedido) -> Result<String, String> {
    calcular(&p.expresion, p.angulos)
        .map(|c| c.para_el_modelo())
        .map_err(|f| format!("{NOMBRE}: {f}"))
}

#[cfg(test)]
mod pruebas;
