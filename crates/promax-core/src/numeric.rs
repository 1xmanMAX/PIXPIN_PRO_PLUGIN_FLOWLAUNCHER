//! Fast numeric evaluation: expressions compiled to a compact stack bytecode
//! (no allocation per evaluation), plus numerical algorithms.

use crate::expr::Expr;
use crate::num::{erf, gamma, C};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug)]
enum Op {
    Const(f64),
    Var(u8),
    Add(u16),
    Mul(u16),
    Pow,
    PowI(i32),
    Recip,
    F1(fn(f64) -> f64),
    F2(fn(f64, f64) -> f64),
    Min(u16),
    Max(u16),
}

/// Compiled real-valued function of a few variables.
#[derive(Clone, Debug)]
pub struct Compiled {
    ops: Vec<Op>,
    max_stack: usize,
    pub nvars: usize,
}

fn deg_sin(x: f64) -> f64 {
    (x * PI / 180.0).sin()
}
fn deg_cos(x: f64) -> f64 {
    (x * PI / 180.0).cos()
}
fn deg_tan(x: f64) -> f64 {
    (x * PI / 180.0).tan()
}
fn deg_asin(x: f64) -> f64 {
    x.asin() * 180.0 / PI
}
fn deg_acos(x: f64) -> f64 {
    x.acos() * 180.0 / PI
}
fn deg_atan(x: f64) -> f64 {
    x.atan() * 180.0 / PI
}
fn sec(x: f64) -> f64 {
    1.0 / x.cos()
}
fn csc(x: f64) -> f64 {
    1.0 / x.sin()
}
fn cot(x: f64) -> f64 {
    1.0 / x.tan()
}
fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}
fn fact(x: f64) -> f64 {
    gamma(x + 1.0)
}
fn frac(x: f64) -> f64 {
    x.fract()
}
fn round(x: f64) -> f64 {
    x.round()
}
fn logb(x: f64, b: f64) -> f64 {
    x.ln() / b.ln()
}
fn root(x: f64, n: f64) -> f64 {
    if x < 0.0 && n.fract() == 0.0 && (n as i64) % 2 == 1 {
        -(-x).powf(1.0 / n)
    } else {
        x.powf(1.0 / n)
    }
}
fn modf(a: f64, b: f64) -> f64 {
    a - b * (a / b).floor()
}
fn ncr(n: f64, k: f64) -> f64 {
    (gamma(n + 1.0) / (gamma(k + 1.0) * gamma(n - k + 1.0))).round()
}
fn npr(n: f64, k: f64) -> f64 {
    (gamma(n + 1.0) / gamma(n - k + 1.0)).round()
}
fn atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}
fn heaviside(x: f64) -> f64 {
    if x >= 0.0 {
        1.0
    } else {
        0.0
    }
}
fn asec(x: f64) -> f64 {
    (1.0 / x).acos()
}
fn acsc(x: f64) -> f64 {
    (1.0 / x).asin()
}
fn acot(x: f64) -> f64 {
    (1.0 / x).atan()
}
fn erf_f(x: f64) -> f64 {
    erf(x)
}
fn erfc_f(x: f64) -> f64 {
    1.0 - erf(x)
}
fn normpdf(x: f64) -> f64 {
    (-x * x / 2.0).exp() / (2.0 * PI).sqrt()
}
fn normcdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / 2f64.sqrt()))
}
fn cbrt(x: f64) -> f64 {
    x.cbrt()
}

pub fn func1(name: &str, deg: bool) -> Option<fn(f64) -> f64> {
    Some(match name {
        "sin" => {
            if deg {
                deg_sin
            } else {
                f64::sin
            }
        }
        "cos" => {
            if deg {
                deg_cos
            } else {
                f64::cos
            }
        }
        "tan" => {
            if deg {
                deg_tan
            } else {
                f64::tan
            }
        }
        "asin" => {
            if deg {
                deg_asin
            } else {
                f64::asin
            }
        }
        "acos" => {
            if deg {
                deg_acos
            } else {
                f64::acos
            }
        }
        "atan" => {
            if deg {
                deg_atan
            } else {
                f64::atan
            }
        }
        "sec" => sec,
        "csc" => csc,
        "cot" => cot,
        "asec" => asec,
        "acsc" => acsc,
        "acot" => acot,
        "sinh" => f64::sinh,
        "cosh" => f64::cosh,
        "tanh" => f64::tanh,
        "asinh" => f64::asinh,
        "acosh" => f64::acosh,
        "atanh" => f64::atanh,
        "exp" => f64::exp,
        "ln" => f64::ln,
        "log" => f64::log10,
        "log2" => f64::log2,
        "sqrt" => f64::sqrt,
        "cbrt" => cbrt,
        "abs" => f64::abs,
        "sign" => sign,
        "floor" => f64::floor,
        "ceil" => f64::ceil,
        "round" => round,
        "trunc" | "int" => f64::trunc,
        "frac" => frac,
        "fact" => fact,
        "gamma" => gamma,
        "erf" => erf_f,
        "erfc" => erfc_f,
        "heaviside" | "step" => heaviside,
        "normpdf" => normpdf,
        "normcdf" => normcdf,
        _ => return None,
    })
}

fn func2(name: &str) -> Option<fn(f64, f64) -> f64> {
    Some(match name {
        "log" => logb,
        "root" => root,
        "mod" => modf,
        "ncr" => ncr,
        "npr" => npr,
        "atan2" => atan2,
        "pow" => f64::powf,
        "hypot" => f64::hypot,
        _ => return None,
    })
}

impl Compiled {
    pub fn new(e: &Expr, vars: &[&str], deg: bool) -> Result<Compiled, String> {
        let mut ops = Vec::new();
        compile(e, vars, deg, &mut ops)?;
        // compute stack depth
        let mut d: i64 = 0;
        let mut maxd: i64 = 0;
        for op in &ops {
            d += match op {
                Op::Const(_) | Op::Var(_) => 1,
                Op::Add(n) | Op::Mul(n) | Op::Min(n) | Op::Max(n) => 1 - *n as i64,
                Op::Pow | Op::F2(_) => -1,
                Op::PowI(_) | Op::Recip | Op::F1(_) => 0,
            };
            maxd = maxd.max(d);
        }
        Ok(Compiled { ops, max_stack: maxd as usize + 1, nvars: vars.len() })
    }

    /// Evaluates with a reusable stack buffer.
    #[inline]
    pub fn eval_with(&self, vars: &[f64], st: &mut Vec<f64>) -> f64 {
        st.clear();
        if st.capacity() < self.max_stack {
            st.reserve(self.max_stack);
        }
        for op in &self.ops {
            match *op {
                Op::Const(c) => st.push(c),
                Op::Var(i) => st.push(vars[i as usize]),
                Op::Add(n) => {
                    let n = n as usize;
                    let s = st.len() - n;
                    let v: f64 = st[s..].iter().sum();
                    st.truncate(s);
                    st.push(v);
                }
                Op::Mul(n) => {
                    let n = n as usize;
                    let s = st.len() - n;
                    let v: f64 = st[s..].iter().product();
                    st.truncate(s);
                    st.push(v);
                }
                Op::Min(n) => {
                    let n = n as usize;
                    let s = st.len() - n;
                    let v = st[s..].iter().cloned().fold(f64::INFINITY, f64::min);
                    st.truncate(s);
                    st.push(v);
                }
                Op::Max(n) => {
                    let n = n as usize;
                    let s = st.len() - n;
                    let v = st[s..].iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    st.truncate(s);
                    st.push(v);
                }
                Op::Pow => {
                    let e = st.pop().unwrap();
                    let b = st.pop().unwrap();
                    st.push(real_pow(b, e));
                }
                Op::PowI(k) => {
                    let b = st.pop().unwrap();
                    st.push(b.powi(k));
                }
                Op::Recip => {
                    let b = st.pop().unwrap();
                    st.push(1.0 / b);
                }
                Op::F1(f) => {
                    let a = st.pop().unwrap();
                    st.push(f(a));
                }
                Op::F2(f) => {
                    let b = st.pop().unwrap();
                    let a = st.pop().unwrap();
                    st.push(f(a, b));
                }
            }
        }
        st.pop().unwrap_or(f64::NAN)
    }

    pub fn eval(&self, vars: &[f64]) -> f64 {
        let mut st = Vec::with_capacity(self.max_stack);
        self.eval_with(vars, &mut st)
    }

    pub fn eval1(&self, x: f64) -> f64 {
        self.eval(&[x])
    }
}

fn real_pow(b: f64, e: f64) -> f64 {
    if b < 0.0 && e.fract() != 0.0 {
        // real odd roots: (-8)^(1/3) = -2
        let inv = 1.0 / e;
        if (inv - inv.round()).abs() < 1e-9 && (inv.round() as i64) % 2 != 0 {
            return -(-b).powf(e);
        }
        // e = p/q with odd q
        for q in [3.0, 5.0, 7.0, 9.0] {
            let p = e * q;
            if (p - p.round()).abs() < 1e-9 {
                let r = (-b).powf(e);
                return if (p.round() as i64) % 2 == 0 { r } else { -r };
            }
        }
        return f64::NAN;
    }
    b.powf(e)
}

fn compile(e: &Expr, vars: &[&str], deg: bool, ops: &mut Vec<Op>) -> Result<(), String> {
    match e {
        Expr::Num(r) => ops.push(Op::Const(r.to_f64())),
        Expr::Float(f) => ops.push(Op::Const(*f)),
        Expr::Sym(s) => {
            if let Some(i) = vars.iter().position(|v| v == s) {
                ops.push(Op::Var(i as u8));
            } else {
                ops.push(Op::Const(match s.as_str() {
                    "pi" => PI,
                    "e" => std::f64::consts::E,
                    "inf" => f64::INFINITY,
                    "i" => return Err("Número complejo en evaluación real".into()),
                    _ => return Err(format!("Variable no definida: {s}")),
                }));
            }
        }
        Expr::Add(v) => {
            for t in v {
                compile(t, vars, deg, ops)?;
            }
            ops.push(Op::Add(v.len() as u16));
        }
        Expr::Mul(v) => {
            for t in v {
                compile(t, vars, deg, ops)?;
            }
            ops.push(Op::Mul(v.len() as u16));
        }
        Expr::Pow(b, x) => {
            if let Expr::Sym(s) = b.as_ref() {
                if s == "e" {
                    compile(x, vars, deg, ops)?;
                    ops.push(Op::F1(f64::exp));
                    return Ok(());
                }
            }
            compile(b, vars, deg, ops)?;
            match x.as_int() {
                Some(-1) => ops.push(Op::Recip),
                Some(k) if k.abs() <= 64 => ops.push(Op::PowI(k as i32)),
                _ => {
                    if x.as_rat().map_or(false, |r| r.n == 1 && r.d == 2) {
                        ops.push(Op::F1(f64::sqrt));
                    } else {
                        compile(x, vars, deg, ops)?;
                        ops.push(Op::Pow);
                    }
                }
            }
        }
        Expr::Call(name, args) => {
            match (name.as_str(), args.len()) {
                ("min", n) if n > 0 => {
                    for a in args {
                        compile(a, vars, deg, ops)?;
                    }
                    ops.push(Op::Min(n as u16));
                    return Ok(());
                }
                ("max", n) if n > 0 => {
                    for a in args {
                        compile(a, vars, deg, ops)?;
                    }
                    ops.push(Op::Max(n as u16));
                    return Ok(());
                }
                (_, 1) => {
                    if let Some(f) = func1(name, deg) {
                        compile(&args[0], vars, deg, ops)?;
                        ops.push(Op::F1(f));
                        return Ok(());
                    }
                }
                (_, 2) => {
                    if let Some(f) = func2(name) {
                        compile(&args[0], vars, deg, ops)?;
                        compile(&args[1], vars, deg, ops)?;
                        ops.push(Op::F2(f));
                        return Ok(());
                    }
                }
                _ => {}
            }
            return Err(format!("Función no evaluable numéricamente: {name}"));
        }
        Expr::Matrix(_) | Expr::List(_) => return Err("Se esperaba un valor escalar".into()),
        Expr::Eq(a, b) => {
            // f = g  ->  f - g
            compile(a, vars, deg, ops)?;
            compile(b, vars, deg, ops)?;
            ops.push(Op::Const(-1.0));
            ops.push(Op::Mul(2));
            ops.push(Op::Add(2));
        }
        Expr::Rel(..) | Expr::Text(_) => return Err("Expresión no numérica".into()),
    }
    Ok(())
}

// ------------------------------------------------------------ complex eval

/// Complex numeric evaluation of a closed expression.
pub fn eval_complex(e: &Expr, deg: bool) -> Result<C, String> {
    Ok(match e {
        Expr::Num(r) => C::real(r.to_f64()),
        Expr::Float(f) => C::real(*f),
        Expr::Sym(s) => match s.as_str() {
            "pi" => C::real(PI),
            "e" => C::real(std::f64::consts::E),
            "i" => C::I,
            "inf" => C::real(f64::INFINITY),
            _ => return Err(format!("Variable no definida: {s}")),
        },
        Expr::Add(v) => {
            let mut acc = C::ZERO;
            for t in v {
                acc = acc.add(eval_complex(t, deg)?);
            }
            acc
        }
        Expr::Mul(v) => {
            let mut acc = C::ONE;
            for t in v {
                acc = acc.mul(eval_complex(t, deg)?);
            }
            acc
        }
        Expr::Pow(b, x) => {
            let bb = eval_complex(b, deg)?;
            let xx = eval_complex(x, deg)?;
            if bb.is_real() && xx.is_real() && bb.re < 0.0 {
                let r = real_pow(bb.re, xx.re);
                if !r.is_nan() {
                    return Ok(C::real(r));
                }
            }
            if bb.re == 0.0 && bb.im == 0.0 && xx.re < 0.0 {
                return Err("División entre cero".into());
            }
            bb.powc(xx)
        }
        Expr::Call(name, args) => {
            let a: Result<Vec<C>, String> = args.iter().map(|x| eval_complex(x, deg)).collect();
            let a = a?;
            let all_real = a.iter().all(|z| z.is_real());
            if all_real {
                let r: Vec<f64> = a.iter().map(|z| z.re).collect();
                if r.len() == 1 {
                    if let Some(f) = func1(name, deg) {
                        let v = f(r[0]);
                        if !v.is_nan() || r[0].is_nan() {
                            return Ok(C::real(v));
                        }
                    }
                } else if r.len() == 2 {
                    if let Some(f) = func2(name) {
                        return Ok(C::real(f(r[0], r[1])));
                    }
                }
                match name.as_str() {
                    "min" if !r.is_empty() => return Ok(C::real(r.iter().cloned().fold(f64::INFINITY, f64::min))),
                    "max" if !r.is_empty() => return Ok(C::real(r.iter().cloned().fold(f64::NEG_INFINITY, f64::max))),
                    _ => {}
                }
            }
            if a.len() == 1 {
                let z = a[0];
                let k = if deg { PI / 180.0 } else { 1.0 };
                return Ok(match name.as_str() {
                    "sin" => z.scale(k).sin(),
                    "cos" => z.scale(k).cos(),
                    "tan" => z.scale(k).tan(),
                    "asin" => z.asin().scale(1.0 / k),
                    "acos" => z.acos().scale(1.0 / k),
                    "atan" => z.atan().scale(1.0 / k),
                    "sinh" => z.sinh(),
                    "cosh" => z.cosh(),
                    "tanh" => z.tanh(),
                    "asinh" => z.asinh(),
                    "acosh" => z.acosh(),
                    "atanh" => z.atanh(),
                    "exp" => z.exp(),
                    "ln" => z.ln(),
                    "log" => z.ln().scale(1.0 / std::f64::consts::LN_10),
                    "log2" => z.ln().scale(1.0 / std::f64::consts::LN_2),
                    "sqrt" => z.sqrt(),
                    "abs" => C::real(z.abs()),
                    "re" => C::real(z.re),
                    "im" => C::real(z.im),
                    "conj" => z.conj(),
                    "arg" => C::real(z.arg() / k),
                    "sec" => C::ONE.div(z.scale(k).cos()),
                    "csc" => C::ONE.div(z.scale(k).sin()),
                    "cot" => C::ONE.div(z.scale(k).tan()),
                    _ => return Err(format!("Función no evaluable numéricamente: {name}")),
                });
            }
            return Err(format!("Función no evaluable numéricamente: {name}"));
        }
        Expr::Eq(..) | Expr::Rel(..) => return Err("Una ecuación no tiene valor numérico".into()),
        Expr::Matrix(_) | Expr::List(_) | Expr::Text(_) => return Err("Se esperaba un escalar".into()),
    })
}

// ------------------------------------------------------------ integration

const GK_X: [f64; 8] = [
    0.991455371120812639206854697526329,
    0.949107912342758524526189684047851,
    0.864864423359769072789712788640926,
    0.741531185599394439863864773280788,
    0.586087235467691130294144845693013,
    0.405845151377397166906606412076961,
    0.207784955007898467600689403773245,
    0.000000000000000000000000000000000,
];
const GK_WK: [f64; 8] = [
    0.022935322010529224963732008058970,
    0.063092092629978553290700663189204,
    0.104790010322250183839876322541518,
    0.140653259715525918745189590510238,
    0.169004726639267902826583426598550,
    0.190350578064785409913256402421014,
    0.204432940075298892414161999234649,
    0.209482141084727828012999174891714,
];
const GK_WG: [f64; 4] = [
    0.129484966168869693270611432679082,
    0.279705391489276667901467771423780,
    0.381830050505118944950369775488975,
    0.417959183673469387755102040816327,
];

fn gk15(f: &mut dyn FnMut(f64) -> f64, a: f64, b: f64) -> (f64, f64) {
    let c = 0.5 * (a + b);
    let h = 0.5 * (b - a);
    let fc = f(c);
    let mut k = fc * GK_WK[7];
    let mut g = fc * GK_WG[3];
    for i in 0..7 {
        let dx = h * GK_X[i];
        let f1 = f(c - dx);
        let f2 = f(c + dx);
        k += GK_WK[i] * (f1 + f2);
        if i % 2 == 1 {
            g += GK_WG[i / 2] * (f1 + f2);
        }
    }
    (k * h, ((k - g) * h).abs())
}

/// Adaptive Gauss–Kronrod (7-15) integration. Supports infinite limits.
pub fn integrate(f: &mut dyn FnMut(f64) -> f64, a: f64, b: f64, tol: f64) -> Result<f64, String> {
    if a == b {
        return Ok(0.0);
    }
    if a > b {
        return integrate(f, b, a, tol).map(|v| -v);
    }
    if a.is_infinite() || b.is_infinite() {
        // x = t / (1 - t^2), t in (-1, 1)
        let (ta, tb) = (
            if a.is_infinite() { -1.0 } else { inv_map(a) },
            if b.is_infinite() { 1.0 } else { inv_map(b) },
        );
        let mut g = |t: f64| {
            let d = 1.0 - t * t;
            if d <= 0.0 {
                return 0.0;
            }
            let x = t / d;
            let v = f(x) * (1.0 + t * t) / (d * d);
            if v.is_finite() {
                v
            } else {
                0.0
            }
        };
        return integrate(&mut g, ta, tb, tol);
    }
    let mut stack = vec![(a, b, 0u32)];
    let mut total = 0.0;
    let mut err_total = 0.0;
    let mut evals = 0usize;
    let (whole, _) = gk15(f, a, b);
    let scale = whole.abs().max(1e-12);
    while let Some((x0, x1, depth)) = stack.pop() {
        let (v, err) = gk15(f, x0, x1);
        evals += 15;
        if !v.is_finite() {
            if depth > 40 {
                return Err("La integral diverge o no es numéricamente estable".into());
            }
            let m = 0.5 * (x0 + x1);
            stack.push((x0, m, depth + 1));
            stack.push((m, x1, depth + 1));
            continue;
        }
        if err <= (tol * scale).max(1e-15) * (x1 - x0) / (b - a) || depth > 50 || evals > 2_000_000 {
            total += v;
            err_total += err;
        } else {
            let m = 0.5 * (x0 + x1);
            stack.push((x0, m, depth + 1));
            stack.push((m, x1, depth + 1));
        }
    }
    let _ = err_total;
    if !total.is_finite() {
        return Err("La integral diverge".into());
    }
    Ok(total)
}

fn inv_map(x: f64) -> f64 {
    // inverse of x = t/(1-t^2)
    if x == 0.0 {
        0.0
    } else {
        (-1.0 + (1.0 + 4.0 * x * x).sqrt()) / (2.0 * x)
    }
}

// ------------------------------------------------------------ roots

/// Brent/Illinois refinement of a bracketed root.
pub fn bisect(f: &mut dyn FnMut(f64) -> f64, mut a: f64, mut b: f64) -> Option<f64> {
    let mut fa = f(a);
    let mut fb = f(b);
    if fa == 0.0 {
        return Some(a);
    }
    if fb == 0.0 {
        return Some(b);
    }
    if fa.signum() == fb.signum() {
        return None;
    }
    let mut side = 0;
    for _ in 0..200 {
        // regula falsi (Illinois)
        let c = (a * fb - b * fa) / (fb - fa);
        let c = if c.is_finite() && c > a.min(b) && c < a.max(b) { c } else { 0.5 * (a + b) };
        let fc = f(c);
        if fc == 0.0 || (b - a).abs() < 1e-15 * c.abs().max(1e-300) {
            return Some(c);
        }
        if fc.signum() == fb.signum() {
            b = c;
            fb = fc;
            if side == -1 {
                fa /= 2.0;
            }
            side = -1;
        } else {
            a = c;
            fa = fc;
            if side == 1 {
                fb /= 2.0;
            }
            side = 1;
        }
        if (b - a).abs() < 1e-14 * (1.0 + a.abs()) {
            return Some(0.5 * (a + b));
        }
    }
    Some(0.5 * (a + b))
}

pub fn newton(f: &mut dyn FnMut(f64) -> f64, mut x: f64) -> Option<f64> {
    for _ in 0..100 {
        let fx = f(x);
        if !fx.is_finite() {
            return None;
        }
        if fx == 0.0 {
            return Some(x);
        }
        let h = 1e-7 * x.abs().max(1e-3);
        let d = (f(x + h) - f(x - h)) / (2.0 * h);
        if d == 0.0 || !d.is_finite() {
            return None;
        }
        let nx = x - fx / d;
        if (nx - x).abs() <= 1e-14 * nx.abs().max(1.0) {
            return Some(nx);
        }
        x = nx;
    }
    if f(x).abs() < 1e-10 {
        Some(x)
    } else {
        None
    }
}

/// Finds real roots of f in [a, b] by dense sampling + refinement.
pub fn find_roots(f: &mut dyn FnMut(f64) -> f64, a: f64, b: f64, n: usize) -> Vec<f64> {
    let mut roots: Vec<f64> = Vec::new();
    let step = (b - a) / n as f64;
    let mut xs = Vec::with_capacity(n + 1);
    let mut ys = Vec::with_capacity(n + 1);
    for k in 0..=n {
        let x = a + step * k as f64;
        xs.push(x);
        ys.push(f(x));
    }
    for k in 0..n {
        let (y0, y1) = (ys[k], ys[k + 1]);
        if !y0.is_finite() || !y1.is_finite() {
            continue;
        }
        if y0 == 0.0 {
            roots.push(xs[k]);
        } else if y0.signum() != y1.signum() && y1 != 0.0 {
            if let Some(r) = bisect(f, xs[k], xs[k + 1]) {
                // reject poles (sign change through infinity)
                let fr = f(r);
                if fr.is_finite() && fr.abs() < 1e-6 * (1.0 + y0.abs().min(y1.abs())) {
                    roots.push(r);
                }
            }
        }
        // tangential roots: local minimum of |f| close to zero
        if k > 0 {
            let (ya, yb, yc) = (ys[k - 1].abs(), ys[k].abs(), ys[k + 1].abs());
            if yb < ya && yb <= yc && yb < 1e-3 && ys[k - 1].signum() == ys[k + 1].signum() {
                let g = |x: f64| x;
                let _ = g;
                if let Some(r) = golden_min(&mut |x| f(x).abs(), xs[k - 1], xs[k + 1]) {
                    if f(r).abs() < 1e-9 {
                        roots.push(r);
                    }
                }
            }
        }
    }
    if ys[n] == 0.0 {
        roots.push(xs[n]);
    }
    roots.sort_by(|a, b| a.total_cmp(b));
    roots.dedup_by(|a, b| (*a - *b).abs() < 1e-7 * (1.0 + a.abs()));
    roots
}

/// Golden-section minimization on [a, b].
pub fn golden_min(f: &mut dyn FnMut(f64) -> f64, mut a: f64, mut b: f64) -> Option<f64> {
    let gr = (5f64.sqrt() - 1.0) / 2.0;
    let mut c = b - gr * (b - a);
    let mut d = a + gr * (b - a);
    let mut fc = f(c);
    let mut fd = f(d);
    for _ in 0..200 {
        if (b - a).abs() < 1e-13 * (1.0 + a.abs()) {
            break;
        }
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - gr * (b - a);
            fc = f(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + gr * (b - a);
            fd = f(d);
        }
    }
    let x = 0.5 * (a + b);
    if x.is_finite() {
        Some(x)
    } else {
        None
    }
}

/// Local extrema of f on [a, b]: (x, y, is_max).
pub fn find_extrema(f: &mut dyn FnMut(f64) -> f64, a: f64, b: f64, n: usize) -> Vec<(f64, f64, bool)> {
    let step = (b - a) / n as f64;
    let ys: Vec<f64> = (0..=n).map(|k| f(a + step * k as f64)).collect();
    let mut out = Vec::new();
    for k in 1..n {
        let (y0, y1, y2) = (ys[k - 1], ys[k], ys[k + 1]);
        if !(y0.is_finite() && y1.is_finite() && y2.is_finite()) {
            continue;
        }
        let x0 = a + step * (k - 1) as f64;
        let x2 = a + step * (k + 1) as f64;
        if y1 <= y0 && y1 < y2 || y1 < y0 && y1 <= y2 {
            if let Some(x) = golden_min(f, x0, x2) {
                out.push((x, f(x), false));
            }
        } else if y1 >= y0 && y1 > y2 || y1 > y0 && y1 >= y2 {
            if let Some(x) = golden_min(&mut |t| -f(t), x0, x2) {
                out.push((x, f(x), true));
            }
        }
    }
    out
}

/// All complex roots of a polynomial (coefficients highest degree first), Aberth–Ehrlich.
pub fn poly_roots(coeffs: &[C]) -> Vec<C> {
    let mut c: Vec<C> = coeffs.to_vec();
    while c.len() > 1 && c[0].abs() == 0.0 {
        c.remove(0);
    }
    let n = c.len().saturating_sub(1);
    if n == 0 {
        return vec![];
    }
    let lead = c[0];
    let c: Vec<C> = c.iter().map(|z| z.div(lead)).collect();
    // Cauchy bound for initial circle
    let bound = 1.0 + c[1..].iter().map(|z| z.abs()).fold(0.0, f64::max);
    let mut z: Vec<C> = (0..n)
        .map(|k| {
            let ang = 2.0 * PI * k as f64 / n as f64 + 0.4;
            C::new(bound * 0.5 * ang.cos(), bound * 0.5 * ang.sin())
        })
        .collect();
    let eval = |x: C| -> (C, C) {
        let mut p = C::ZERO;
        let mut dp = C::ZERO;
        for coef in &c {
            dp = dp.mul(x).add(p);
            p = p.mul(x).add(*coef);
        }
        (p, dp)
    };
    for _ in 0..500 {
        let mut maxd: f64 = 0.0;
        for i in 0..n {
            let (p, dp) = eval(z[i]);
            if p.abs() == 0.0 {
                continue;
            }
            let ratio = p.div(dp);
            let mut s = C::ZERO;
            for j in 0..n {
                if j != i {
                    s = s.add(C::ONE.div(z[i].sub(z[j])));
                }
            }
            let w = ratio.div(C::ONE.sub(ratio.mul(s)));
            if w.is_finite() {
                z[i] = z[i].sub(w);
                maxd = maxd.max(w.abs() / z[i].abs().max(1.0));
            }
        }
        if maxd < 1e-15 {
            break;
        }
    }
    // polish with Newton and clean tiny imaginary parts
    for r in z.iter_mut() {
        for _ in 0..3 {
            let (p, dp) = eval(*r);
            if dp.abs() > 0.0 {
                let nr = r.sub(p.div(dp));
                if nr.is_finite() {
                    *r = nr;
                }
            }
        }
        if r.im.abs() < 1e-10 * r.re.abs().max(1e-6) {
            r.im = 0.0;
        }
        if r.re.abs() < 1e-14 * r.im.abs() {
            r.re = 0.0;
        }
    }
    z.sort_by(|a, b| a.re.total_cmp(&b.re).then(a.im.total_cmp(&b.im)));
    z
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gk() {
        let v = integrate(&mut |x: f64| x.sin(), 0.0, PI, 1e-12).unwrap();
        assert!((v - 2.0).abs() < 1e-12);
        let g = integrate(&mut |x: f64| (-x * x).exp(), f64::NEG_INFINITY, f64::INFINITY, 1e-10).unwrap();
        assert!((g - PI.sqrt()).abs() < 1e-8, "{g}");
    }
    #[test]
    fn roots() {
        let r = find_roots(&mut |x: f64| x * x - 2.0, -10.0, 10.0, 1000);
        assert_eq!(r.len(), 2);
        assert!((r[1] - 2f64.sqrt()).abs() < 1e-12);
        let p = poly_roots(&[C::real(1.0), C::real(0.0), C::real(1.0)]);
        assert!((p[0].im.abs() - 1.0).abs() < 1e-12);
    }
}
