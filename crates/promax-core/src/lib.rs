//! # promax-core
//!
//! Zero-dependency computer algebra system and numeric engine for Calculator PROMAX.
//!
//! * Exact rational arithmetic, radicals (`√8 = 2√2`), symbolic constants (π, e, i)
//! * Canonical n-ary expression trees, like-term collection, expansion
//! * Calculus (derivatives, integrals, limits, Taylor series, sums)
//! * Equation solving (exact polynomials, numeric fallback, linear systems)
//! * Exact matrix algebra (det, inverse, rref, eigenvalues...)
//! * Statistics & regression, geometry helpers
//! * Bytecode-compiled numeric evaluation for fast graphing
//!
//! ```
//! let mut eng = promax_core::Engine::new();
//! let out = eng.eval_line("1/3 + 1/6").unwrap();
//! assert_eq!(out.exact_str, "1/2");
//! ```

pub mod calculus;
pub mod eval;
pub mod expr;
pub mod geometry;
pub mod matrix;
pub mod num;
pub mod numeric;
pub mod parser;
pub mod simplify;
pub mod solve;
pub mod stats;

pub use eval::{Cas, Settings};
pub use expr::Expr;
pub use numeric::Compiled;

use parser::{Parser, Stmt};
use simplify::{is_binding_call, subst};
use std::collections::BTreeMap;

/// A curve requested from the worksheet with `plot(f, g, ..., xmin, xmax)`.
#[derive(Clone, Debug)]
pub struct PlotSpec {
    /// Prepared (substituted & simplified) expressions in `var`.
    pub exprs: Vec<Expr>,
    pub labels: Vec<String>,
    pub var: String,
    pub xmin: f64,
    pub xmax: f64,
}

#[derive(Clone, Debug)]
pub enum OutKind {
    Value,
    /// `x := ...` stored a variable.
    Assigned(String),
    /// `f(x) := ...` defined a function.
    Defined(String),
    Plot(PlotSpec),
}

/// Result of evaluating one worksheet line.
#[derive(Clone, Debug)]
pub struct Output {
    /// Pretty form of the parsed input.
    pub input_pretty: String,
    /// Exact / symbolic result.
    pub exact: Expr,
    pub exact_str: String,
    /// Decimal approximation (when it differs from the exact form).
    pub approx: Option<String>,
    pub kind: OutKind,
}

#[derive(Clone, Debug)]
pub struct UserFunc {
    pub params: Vec<String>,
    pub body: Expr,
}

/// Stateful calculator: variables, user functions, settings and `ans`.
#[derive(Clone, Debug, Default)]
pub struct Engine {
    pub settings: Settings,
    vars: BTreeMap<String, Expr>,
    funcs: BTreeMap<String, UserFunc>,
}

const MAX_SUBST_DEPTH: usize = 64;

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    fn is_func_pred(&self) -> impl Fn(&str) -> bool + '_ {
        move |n: &str| eval::is_builtin(n) || self.funcs.contains_key(n)
    }

    /// Parses an expression (no evaluation).
    pub fn parse(&self, src: &str) -> Result<Expr, String> {
        let p = self.is_func_pred();
        Parser::parse_expr(src, &p)
    }

    pub fn printer(&self) -> expr::Printer {
        expr::Printer { sig: self.settings.digits }
    }

    pub fn print(&self, e: &Expr) -> String {
        self.printer().print(e)
    }

    // ------------------------------------------------------------ variables

    pub fn vars(&self) -> impl Iterator<Item = (&String, &Expr)> {
        self.vars.iter()
    }
    pub fn funcs(&self) -> impl Iterator<Item = (&String, &UserFunc)> {
        self.funcs.iter()
    }
    pub fn get_var(&self, name: &str) -> Option<&Expr> {
        self.vars.get(name)
    }
    pub fn set_var(&mut self, name: &str, value: Expr) {
        self.funcs.remove(name);
        self.vars.insert(name.to_string(), value);
    }
    pub fn define_func(&mut self, name: &str, params: Vec<String>, body: Expr) {
        self.vars.remove(name);
        self.funcs.insert(name.to_string(), UserFunc { params, body });
    }
    pub fn remove(&mut self, name: &str) {
        self.vars.remove(name);
        self.funcs.remove(name);
    }
    pub fn clear_all(&mut self) {
        self.vars.clear();
        self.funcs.clear();
    }

    /// Serializes variables and functions as re-evaluable lines.
    pub fn export_state(&self) -> String {
        let p = expr::Printer { sig: 17 };
        let mut s = String::new();
        for (n, f) in &self.funcs {
            s.push_str(&format!("{}({}) := {}\n", n, f.params.join(", "), p.print(&f.body)));
        }
        for (n, v) in &self.vars {
            if n != "ans" {
                s.push_str(&format!("{} := {}\n", n, p.print(v)));
            }
        }
        s
    }

    pub fn import_state(&mut self, s: &str) {
        for line in s.lines() {
            if !line.trim().is_empty() {
                let _ = self.eval_line(line);
            }
        }
    }

    // ------------------------------------------------------------ substitution

    /// Replaces defined variables and inlines user functions, respecting
    /// variables bound by calculus commands (diff(f, x) keeps x free).
    pub fn prepare(&self, e: &Expr) -> Result<Expr, String> {
        let mut shadow = Vec::new();
        self.prep(e, &mut shadow, 0)
    }

    fn prep(&self, e: &Expr, shadow: &mut Vec<String>, depth: usize) -> Result<Expr, String> {
        if depth > MAX_SUBST_DEPTH {
            return Err("Definición recursiva demasiado profunda".into());
        }
        Ok(match e {
            Expr::Sym(s) => {
                if shadow.contains(s) {
                    return Ok(e.clone());
                }
                match self.vars.get(s) {
                    Some(v) => self.prep(v, shadow, depth + 1)?,
                    None => e.clone(),
                }
            }
            Expr::Add(v) => Expr::Add(v.iter().map(|x| self.prep(x, shadow, depth)).collect::<Result<_, _>>()?),
            Expr::Mul(v) => Expr::Mul(v.iter().map(|x| self.prep(x, shadow, depth)).collect::<Result<_, _>>()?),
            Expr::List(v) => Expr::List(v.iter().map(|x| self.prep(x, shadow, depth)).collect::<Result<_, _>>()?),
            Expr::Pow(a, b) => Expr::Pow(Box::new(self.prep(a, shadow, depth)?), Box::new(self.prep(b, shadow, depth)?)),
            Expr::Eq(a, b) => Expr::Eq(Box::new(self.prep(a, shadow, depth)?), Box::new(self.prep(b, shadow, depth)?)),
            Expr::Rel(o, a, b) => Expr::Rel(o.clone(), Box::new(self.prep(a, shadow, depth)?), Box::new(self.prep(b, shadow, depth)?)),
            Expr::Matrix(m) => Expr::Matrix(m.iter().map(|r| r.iter().map(|x| self.prep(x, shadow, depth)).collect()).collect::<Result<Vec<Vec<_>>, _>>()?),
            Expr::Call(name, args) => {
                // bound variables of calculus commands
                let mut bound: Vec<String> = Vec::new();
                if is_binding_call(name) {
                    if let Some(b) = args.get(1) {
                        match b {
                            Expr::Sym(s) => bound.push(s.clone()),
                            Expr::List(v) => bound.extend(v.iter().filter_map(|x| x.sym_name().map(String::from))),
                            _ => {}
                        }
                    }
                }
                let n0 = shadow.len();
                let mut out_args = Vec::with_capacity(args.len());
                for (k, a) in args.iter().enumerate() {
                    if k <= 1 && !bound.is_empty() {
                        shadow.extend(bound.iter().cloned());
                        let r = self.prep(a, shadow, depth);
                        shadow.truncate(n0);
                        out_args.push(r?);
                    } else {
                        out_args.push(self.prep(a, shadow, depth)?);
                    }
                }
                if let Some(f) = self.funcs.get(name) {
                    if f.params.len() != out_args.len() {
                        return Err(format!("{} requiere {} argumento(s)", name, f.params.len()));
                    }
                    // simultaneous substitution via temporary names (avoids capture)
                    let mut body = f.body.clone();
                    for (i, p) in f.params.iter().enumerate() {
                        body = subst(&body, p, &Expr::Sym(format!("\u{1}{i}")));
                    }
                    for (i, a) in out_args.iter().enumerate() {
                        body = subst(&body, &format!("\u{1}{i}"), a);
                    }
                    // params are not globals inside the body
                    let mut sh = shadow.clone();
                    sh.extend(f.params.iter().cloned());
                    return self.prep(&body, &mut sh, depth + 1);
                }
                Expr::Call(name.clone(), out_args)
            }
            other => other.clone(),
        })
    }

    /// Prepares and symbolically evaluates an expression.
    pub fn evaluate(&self, e: &Expr) -> Result<Expr, String> {
        let p = self.prepare(e)?;
        Cas::new(&self.settings).eval(&p)
    }

    /// Compiles an expression source to fast bytecode in the given variables.
    pub fn compile_src(&self, src: &str, vars: &[&str]) -> Result<Compiled, String> {
        let e = self.parse(src)?;
        self.compile_expr(&e, vars)
    }

    pub fn compile_expr(&self, e: &Expr, vars: &[&str]) -> Result<Compiled, String> {
        // shadow plotting variables so a global `x := 3` does not freeze the curve
        let mut shadow: Vec<String> = vars.iter().map(|s| s.to_string()).collect();
        let p = self.prep(e, &mut shadow, 0)?;
        let cas = Cas::new(&self.settings);
        let ev = cas.eval(&p).unwrap_or(p);
        // y = f(x) form: graph rhs
        let ev = match ev {
            Expr::Eq(l, r) if l.sym_name() == Some("y") => *r,
            other => other,
        };
        Compiled::new(&ev, vars, self.settings.deg)
    }

    // ------------------------------------------------------------ main entry

    /// Evaluates one worksheet line: expression, `x := v`, `f(x) := body`,
    /// or `plot(f, g, xmin, xmax)`.
    pub fn eval_line(&mut self, src: &str) -> Result<Output, String> {
        let src = src.trim();
        if src.is_empty() {
            return Err("Entrada vacía".into());
        }
        let stmt = {
            let p = self.is_func_pred();
            Parser::parse_stmt(src, &p)?
        };
        let printer = self.printer();
        match stmt {
            Stmt::DefFunc(name, params, body) => {
                if eval::is_builtin(&name) {
                    return Err(format!("'{name}' es una función integrada y no se puede redefinir"));
                }
                let pretty = format!("{}({}) := {}", name, params.join(", "), printer.print(&body));
                self.define_func(&name, params, body.clone());
                Ok(Output { input_pretty: pretty.clone(), exact: body.clone(), exact_str: printer.print(&body), approx: None, kind: OutKind::Defined(name) })
            }
            Stmt::AssignVar(name, value) => {
                if expr::is_constant(&name) || eval::is_builtin(&name) {
                    return Err(format!("'{name}' es un nombre reservado"));
                }
                let v = self.evaluate(&value)?;
                let mut out = self.make_output(format!("{} := {}", name, printer.print(&value)), v.clone())?;
                out.kind = OutKind::Assigned(name.clone());
                self.set_var(&name, v.clone());
                self.vars.insert("ans".into(), v);
                Ok(out)
            }
            Stmt::Expr(e) => {
                if let Expr::Call(n, args) = &e {
                    if n == "plot" || n == "graph" || n == "graficar" {
                        return self.plot_cmd(args);
                    }
                }
                let v = self.evaluate(&e)?;
                let out = self.make_output(printer.print(&e), v.clone())?;
                self.vars.insert("ans".into(), v);
                Ok(out)
            }
        }
    }

    fn make_output(&self, input_pretty: String, v: Expr) -> Result<Output, String> {
        let printer = self.printer();
        let exact_str = printer.print(&v);
        let mut approx = None;
        // `{x = a, x = b}`: judge closedness on the solution values only
        let probe = strip_solution_lhs(&v);
        let needs_approx = !matches!(v, Expr::Text(_)) && !probe.has_free_symbols() && {
            match &probe {
                Expr::Num(r) => !r.is_int(),
                Expr::Float(_) => false,
                Expr::List(l) => l.iter().any(|x| !matches!(x, Expr::Float(_)) && !matches!(x, Expr::Num(r) if r.is_int())),
                Expr::Matrix(m) => m.iter().flatten().any(|x| !matches!(x, Expr::Float(_)) && !matches!(x, Expr::Num(r) if r.is_int())),
                _ => true,
            }
        };
        let cas = Cas::new(&self.settings);
        if needs_approx {
            if let Ok(a) = cas.approx(&v) {
                let s = printer.print(&a);
                if s != exact_str {
                    approx = Some(s);
                }
            }
        } else if v.has_free_symbols() && !matches!(v, Expr::Text(_)) {
            // symbolic result with inexact constants: show decimal coefficients
            if !v.contains_float() && (exact_str.contains('π') || exact_str.contains('√')) {
                if let Ok(a) = cas.approx(&v) {
                    let s = printer.print(&a);
                    if s != exact_str {
                        approx = Some(s);
                    }
                }
            }
        }
        Ok(Output { input_pretty, exact: v, exact_str, approx, kind: OutKind::Value })
    }

    fn plot_cmd(&mut self, args: &[Expr]) -> Result<Output, String> {
        let mut exprs = Vec::new();
        let mut nums = Vec::new();
        for a in args {
            let prepared = {
                let mut sh = vec!["x".to_string()];
                self.prep(a, &mut sh, 0)?
            };
            let ev = Cas::new(&self.settings).eval(&prepared)?;
            if !ev.has_free_symbols() && ev.is_number() {
                nums.push(ev.as_f64().unwrap());
            } else if let Expr::List(v) = ev {
                exprs.extend(v);
            } else {
                exprs.push(ev);
            }
        }
        if exprs.is_empty() {
            return Err("plot requiere al menos una función, p. ej. plot(sin(x), -5, 5)".into());
        }
        let (xmin, xmax) = match nums.as_slice() {
            [a, b] => (a.min(*b), a.max(*b)),
            [] => (-10.0, 10.0),
            _ => return Err("plot(f, ..., xmin, xmax)".into()),
        };
        let mut syms = Vec::new();
        for e in &exprs {
            e.symbols(&mut syms);
        }
        let var = if syms.len() == 1 { syms[0].clone() } else { "x".into() };
        let labels: Vec<String> = exprs.iter().map(|e| self.print(e)).collect();
        let pretty = format!("plot({})", args.iter().map(|a| self.print(a)).collect::<Vec<_>>().join(", "));
        Ok(Output {
            input_pretty: pretty,
            exact: Expr::List(exprs.clone()),
            exact_str: labels.join(", "),
            approx: None,
            kind: OutKind::Plot(PlotSpec { exprs, labels, var, xmin, xmax }),
        })
    }
}

/// Replaces `x = value` by `value` (recursively in lists) for display decisions.
fn strip_solution_lhs(e: &Expr) -> Expr {
    match e {
        Expr::Eq(l, r) if matches!(l.as_ref(), Expr::Sym(_)) => (**r).clone(),
        Expr::List(v) => Expr::List(v.iter().map(strip_solution_lhs).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests;
