//! Calculus: derivatives, integrals, limits, Taylor series, sums and products.
//!
//! Every result is built through the canonical constructors of `simplify`.
//! Symbolic methods are tried first; numeric methods (adaptive quadrature,
//! Richardson extrapolation, accelerated partial sums) are used as fallback
//! and to cross-check exact results.

use crate::eval::Cas;
use crate::expr::{is_constant, Expr};
use crate::num::Rat;
use crate::numeric;
use crate::simplify::{add, div, expand, mul, neg, num_den, pow, sub, subst, R};
use std::cell::Cell;

/// Largest intermediate expression size accepted (guards against blowups).
const MAX_SIZE: usize = 6000;

// ====================================================================== helpers

fn int(n: i128) -> Expr {
    Expr::int(n)
}
fn inf() -> Expr {
    Expr::sym("inf")
}
fn neg_inf() -> Expr {
    Expr::Mul(vec![int(-1), inf()])
}
fn is_pos_inf(e: &Expr) -> bool {
    matches!(e, Expr::Sym(s) if s == "inf") || matches!(e, Expr::Float(f) if *f == f64::INFINITY)
}
fn is_neg_inf(e: &Expr) -> bool {
    match e {
        Expr::Mul(v) if v.len() == 2 => v[0].as_f64().map_or(false, |c| c < 0.0) && is_pos_inf(&v[1]),
        Expr::Float(f) => *f == f64::NEG_INFINITY,
        _ => false,
    }
}
fn fresh(name: &str) -> String {
    format!("\u{2}{name}")
}
fn half() -> Expr {
    Expr::rat(1, 2)
}
fn sqrt(e: Expr) -> R {
    pow(e, half())
}
fn sq(e: Expr) -> R {
    pow(e, int(2))
}
fn ln(cas: &Cas, e: Expr) -> R {
    cas.apply("ln", vec![e])
}
fn f1(cas: &Cas, name: &str, e: Expr) -> R {
    cas.apply(name, vec![e])
}
fn rat(r: Rat) -> Expr {
    Expr::Num(r)
}

/// Conversion factor of trig arguments: π/180 in degree mode, 1 otherwise.
fn kfac(cas: &Cas) -> Expr {
    if cas.s.deg {
        Expr::Mul(vec![Expr::rat(1, 180), Expr::sym("pi")])
    } else {
        Expr::one()
    }
}

/// Inverse trig value expressed in radians (multiplies by π/180 in degree mode).
fn inv_trig_rad(cas: &Cas, name: &str, z: Expr) -> R {
    mul(vec![kfac(cas), f1(cas, name, z)?])
}

fn auto_var(e: &Expr) -> Result<String, String> {
    let mut v = Vec::new();
    e.symbols(&mut v);
    if v.len() == 1 {
        return Ok(v[0].clone());
    }
    if v.is_empty() || v.iter().any(|s| s == "x") {
        return Ok("x".into());
    }
    Err(format!("Indica la variable: la expresión tiene varias ({})", v.join(", ")))
}

fn var_arg(e: &Expr, cmd: &str) -> Result<String, String> {
    match e {
        Expr::Sym(s) if !is_constant(s) => Ok(s.clone()),
        _ => Err(format!("{cmd}: la variable debe ser un símbolo, p. ej. x")),
    }
}

/// Brings a sum of fractions to a common denominator (numerator expanded).
fn together(e: &Expr) -> Option<Expr> {
    let v = match e {
        Expr::Add(v) if v.len() <= 10 => v,
        _ => return None,
    };
    let nds: Vec<(Expr, Expr)> = v.iter().map(num_den).collect();
    if nds.iter().all(|(_, d)| d.is_one()) {
        return None;
    }
    // common denominator: product of distinct bases with maximal exponent
    let mut fac: Vec<(Expr, Rat)> = Vec::new();
    for (_, d) in &nds {
        let fs = match d {
            Expr::Mul(f) => f.clone(),
            o => vec![o.clone()],
        };
        for f in fs {
            let (b, p) = match &f {
                Expr::Pow(b, p) => ((**b).clone(), p.as_rat()?),
                Expr::Num(_) => (f.clone(), Rat::ONE),
                o => (o.clone(), Rat::ONE),
            };
            match fac.iter_mut().find(|(q, _)| *q == b) {
                Some(slot) => {
                    if p > slot.1 {
                        slot.1 = p
                    }
                }
                None => fac.push((b, p)),
            }
        }
    }
    let den = mul(fac.into_iter().map(|(b, p)| pow(b, Expr::Num(p))).collect::<Result<Vec<_>, _>>().ok()?).ok()?;
    let mut terms = Vec::new();
    for (n, d) in nds {
        terms.push(mul(vec![n, den.clone(), pow(d, int(-1)).ok()?]).ok()?);
    }
    let num = expand(&add(terms).ok()?).ok()?;
    mul(vec![num, pow(den, int(-1)).ok()?]).ok()
}

/// Picks the smallest of `e`, its expansion and its common-denominator form.
fn tidy_frac(e: Expr) -> Expr {
    let e = tidy(e);
    if e.size() < 400 {
        if let Some(t) = together(&e) {
            if t.size() < e.size() {
                return t;
            }
        }
    }
    e
}

/// Picks the smaller of `e` and its expansion.
fn tidy(e: Expr) -> Expr {
    if e.size() < 600 {
        if let Ok(x) = expand(&e) {
            if x.size() < e.size() {
                return x;
            }
        }
    }
    e
}

fn guard(e: Expr) -> R {
    if e.size() > MAX_SIZE {
        return Err("La expresión crece demasiado".into());
    }
    Ok(e)
}

/// Conservative test for "obviously non-negative" expressions (used to drop |·|).
fn positive(e: &Expr) -> bool {
    match e {
        Expr::Num(r) => r.n > 0,
        Expr::Float(f) => *f > 0.0,
        Expr::Sym(s) => s == "pi" || s == "e",
        Expr::Pow(b, x) => {
            if matches!(b.as_ref(), Expr::Sym(s) if s == "e") {
                return true;
            }
            if let Some(k) = x.as_int() {
                if k % 2 == 0 {
                    return true;
                }
            }
            positive(b)
        }
        Expr::Mul(v) | Expr::Add(v) => v.iter().all(positive),
        Expr::Call(n, _) => matches!(n.as_str(), "cosh" | "exp" | "abs"),
        _ => false,
    }
}

fn ln_abs(cas: &Cas, g: Expr) -> R {
    if positive(&g) {
        ln(cas, g)
    } else {
        ln(cas, f1(cas, "abs", g)?)
    }
}

/// Removes `abs` around obviously non-negative arguments (structural; re-evaluate afterwards).
fn strip_abs(e: &Expr) -> Expr {
    match e {
        Expr::Call(n, a) if n == "abs" && a.len() == 1 && positive(&a[0]) => strip_abs(&a[0]),
        Expr::Call(n, a) => Expr::Call(n.clone(), a.iter().map(strip_abs).collect()),
        Expr::Add(v) => Expr::Add(v.iter().map(strip_abs).collect()),
        Expr::Mul(v) => Expr::Mul(v.iter().map(strip_abs).collect()),
        Expr::List(v) => Expr::List(v.iter().map(strip_abs).collect()),
        Expr::Pow(a, b) => Expr::Pow(Box::new(strip_abs(a)), Box::new(strip_abs(b))),
        other => other.clone(),
    }
}

fn is_discontinuous_fn(n: &str) -> bool {
    matches!(n, "sign" | "floor" | "ceil" | "round" | "trunc" | "int" | "frac" | "heaviside" | "mod")
}

fn has_discontinuous(e: &Expr) -> bool {
    match e {
        Expr::Call(n, a) => is_discontinuous_fn(n) || a.iter().any(has_discontinuous),
        Expr::Add(v) | Expr::Mul(v) | Expr::List(v) => v.iter().any(has_discontinuous),
        Expr::Pow(a, b) => has_discontinuous(a) || has_discontinuous(b),
        _ => false,
    }
}

/// True when `v` is a usable finite value (symbolic parameters allowed).
fn finite_val(cas: &Cas, v: &Expr) -> bool {
    if !v.free_of("inf") || matches!(v, Expr::Text(_)) {
        return false;
    }
    if !v.has_free_symbols() {
        return match cas.num(v) {
            Ok(z) => z.is_finite() && !z.is_nan(),
            Err(_) => false,
        };
    }
    true
}

/// Recognizes a float as a "nice" closed form (rationals and simple multiples of
/// π, π², e, √2, ln 2, ...). `tol` is relative.
fn nice(v: f64, tol: f64) -> Option<Expr> {
    if !v.is_finite() {
        return None;
    }
    let sc = v.abs().max(1.0);
    let maxd: i128 = if tol <= 1e-10 { 1000 } else { 60 };
    if let Some(r) = Rat::approximate(v, maxd, tol * sc) {
        return Some(Expr::Num(r));
    }
    let pi = std::f64::consts::PI;
    let e = std::f64::consts::E;
    let consts: Vec<(Expr, f64)> = vec![
        (Expr::sym("pi"), pi),
        (Expr::Pow(Box::new(Expr::sym("pi")), Box::new(int(2))), pi * pi),
        (Expr::sym("e"), e),
        (Expr::call("ln", vec![int(2)]), std::f64::consts::LN_2),
        (Expr::Pow(Box::new(int(2)), Box::new(half())), std::f64::consts::SQRT_2),
        (Expr::Pow(Box::new(int(3)), Box::new(half())), 3f64.sqrt()),
        (Expr::Pow(Box::new(Expr::sym("pi")), Box::new(half())), pi.sqrt()),
        (Expr::Pow(Box::new(Expr::sym("pi")), Box::new(int(4))), pi.powi(4)),
        (Expr::Pow(Box::new(Expr::sym("pi")), Box::new(int(3))), pi.powi(3)),
        (Expr::Pow(Box::new(Expr::sym("e")), Box::new(int(2))), e * e),
        (Expr::Pow(Box::new(Expr::sym("e")), Box::new(int(-1))), 1.0 / e),
        (Expr::Pow(Box::new(Expr::sym("e")), Box::new(half())), e.sqrt()),
    ];
    let cd = maxd.min(360);
    for (c, cv) in consts {
        if let Some(r) = Rat::approximate(v / cv, cd, tol * sc / cv) {
            if !r.is_zero() {
                return mul(vec![Expr::Num(r), c]).ok();
            }
        }
    }
    if let Some(r) = Rat::approximate(v * v, maxd, tol * sc * sc * 2.0) {
        if !r.is_zero() {
            let s = pow(Expr::Num(r), half()).ok()?;
            return if v < 0.0 { neg(s).ok() } else { Some(s) };
        }
    }
    None
}

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * a.abs().max(b.abs()).max(1.0)
}

// ====================================================================== derivatives

/// Symbolic derivative d/dx of `e` (canonical result).
pub fn diff(cas: &Cas, e: &Expr, x: &str) -> R {
    d(cas, e, x, 0)
}

fn d(cas: &Cas, e: &Expr, x: &str, depth: usize) -> R {
    if depth > 200 {
        return Err("Expresión demasiado profunda para derivar".into());
    }
    let r = match e {
        Expr::List(v) => return Ok(Expr::List(v.iter().map(|t| d(cas, t, x, depth + 1)).collect::<Result<_, _>>()?)),
        Expr::Matrix(m) => {
            return Ok(Expr::Matrix(m.iter().map(|r| r.iter().map(|t| d(cas, t, x, depth + 1)).collect()).collect::<Result<Vec<Vec<_>>, _>>()?))
        }
        Expr::Eq(a, b) => return Ok(Expr::Eq(Box::new(d(cas, a, x, depth + 1)?), Box::new(d(cas, b, x, depth + 1)?))),
        Expr::Rel(..) => return Err("No se puede derivar una desigualdad".into()),
        Expr::Text(_) => return Err("No se puede derivar un texto".into()),
        _ if e.free_of(x) => Expr::zero(),
        Expr::Sym(_) => Expr::one(),
        Expr::Add(v) => add(v.iter().map(|t| d(cas, t, x, depth + 1)).collect::<Result<_, _>>()?)?,
        Expr::Mul(v) => {
            // general Leibniz rule over the n-ary product
            let mut terms = Vec::new();
            for i in 0..v.len() {
                if v[i].free_of(x) {
                    continue;
                }
                let di = d(cas, &v[i], x, depth + 1)?;
                if di.is_zero() {
                    continue;
                }
                let mut fs = v.clone();
                fs[i] = di;
                terms.push(mul(fs)?);
            }
            add(terms)?
        }
        Expr::Pow(b, ex) => {
            let (b, ex) = (b.as_ref(), ex.as_ref());
            if ex.free_of(x) {
                mul(vec![ex.clone(), pow(b.clone(), sub(ex.clone(), int(1))?)?, d(cas, b, x, depth + 1)?])?
            } else if b.free_of(x) {
                mul(vec![e.clone(), ln(cas, b.clone())?, d(cas, ex, x, depth + 1)?])?
            } else {
                // d(u^v) = u^v (v' ln u + v u'/u)
                let t1 = mul(vec![d(cas, ex, x, depth + 1)?, ln(cas, b.clone())?])?;
                let t2 = mul(vec![ex.clone(), d(cas, b, x, depth + 1)?, pow(b.clone(), int(-1))?])?;
                mul(vec![e.clone(), add(vec![t1, t2])?])?
            }
        }
        Expr::Call(name, args) => d_call(cas, e, name, args, x, depth)?,
        Expr::Num(_) | Expr::Float(_) => Expr::zero(),
    };
    guard(r)
}

fn unevaluated_diff(e: &Expr, x: &str) -> Expr {
    if let Expr::Call(n, a) = e {
        if n == "diff" && a.len() >= 2 && a[1] == Expr::sym(x) {
            let k = a.get(2).and_then(|k| k.as_int()).unwrap_or(1);
            return Expr::call("diff", vec![a[0].clone(), Expr::sym(x), int(k + 1)]);
        }
    }
    Expr::call("diff", vec![e.clone(), Expr::sym(x)])
}

fn d_call(cas: &Cas, e: &Expr, name: &str, args: &[Expr], x: &str, depth: usize) -> R {
    if name == "log" && args.len() == 2 {
        // log_b(u) = ln(u)/ln(b)
        let r = div(ln(cas, args[0].clone())?, ln(cas, args[1].clone())?)?;
        return d(cas, &r, x, depth + 1);
    }
    if args.len() != 1 {
        return Ok(unevaluated_diff(e, x));
    }
    let u = args[0].clone();
    let du = d(cas, &u, x, depth + 1)?;
    if du.is_zero() {
        return Ok(Expr::zero());
    }
    let k = kfac(cas);
    let ik = pow(k.clone(), int(-1))?;
    let u2 = || sq(u.clone());
    let outer = match name {
        "sin" => mul(vec![k, f1(cas, "cos", u.clone())?])?,
        "cos" => mul(vec![int(-1), k, f1(cas, "sin", u.clone())?])?,
        "tan" => mul(vec![k, sq(f1(cas, "sec", u.clone())?)?])?,
        "sec" => mul(vec![k, f1(cas, "sec", u.clone())?, f1(cas, "tan", u.clone())?])?,
        "csc" => mul(vec![int(-1), k, f1(cas, "csc", u.clone())?, f1(cas, "cot", u.clone())?])?,
        "cot" => mul(vec![int(-1), k, sq(f1(cas, "csc", u.clone())?)?])?,
        "asin" => mul(vec![ik, pow(sub(int(1), u2()?)?, Expr::rat(-1, 2))?])?,
        "acos" => mul(vec![int(-1), ik, pow(sub(int(1), u2()?)?, Expr::rat(-1, 2))?])?,
        "atan" => mul(vec![ik, pow(add(vec![int(1), u2()?])?, int(-1))?])?,
        "acot" => mul(vec![int(-1), ik, pow(add(vec![int(1), u2()?])?, int(-1))?])?,
        "asec" => mul(vec![ik, pow(f1(cas, "abs", u.clone())?, int(-1))?, pow(sub(u2()?, int(1))?, Expr::rat(-1, 2))?])?,
        "acsc" => mul(vec![int(-1), ik, pow(f1(cas, "abs", u.clone())?, int(-1))?, pow(sub(u2()?, int(1))?, Expr::rat(-1, 2))?])?,
        "sinh" => f1(cas, "cosh", u.clone())?,
        "cosh" => f1(cas, "sinh", u.clone())?,
        "tanh" => pow(f1(cas, "cosh", u.clone())?, int(-2))?,
        "asinh" => pow(add(vec![u2()?, int(1)])?, Expr::rat(-1, 2))?,
        "acosh" => pow(sub(u2()?, int(1))?, Expr::rat(-1, 2))?,
        "atanh" => pow(sub(int(1), u2()?)?, int(-1))?,
        "ln" => pow(u.clone(), int(-1))?,
        "log" => pow(mul(vec![u.clone(), ln(cas, int(10))?])?, int(-1))?,
        "log2" => pow(mul(vec![u.clone(), ln(cas, int(2))?])?, int(-1))?,
        "exp" => pow(Expr::sym("e"), u.clone())?,
        "sqrt" => mul(vec![half(), pow(u.clone(), Expr::rat(-1, 2))?])?,
        "cbrt" => mul(vec![Expr::rat(1, 3), pow(u.clone(), Expr::rat(-2, 3))?])?,
        "abs" => f1(cas, "sign", u.clone())?,
        "sign" | "floor" | "ceil" | "round" | "trunc" | "int" | "heaviside" => return Ok(Expr::zero()),
        "frac" => Expr::one(),
        "erf" => mul(vec![int(2), pow(Expr::sym("pi"), Expr::rat(-1, 2))?, pow(Expr::sym("e"), neg(u2()?)?)?])?,
        "erfc" => mul(vec![int(-2), pow(Expr::sym("pi"), Expr::rat(-1, 2))?, pow(Expr::sym("e"), neg(u2()?)?)?])?,
        "normcdf" => f1(cas, "normpdf", u.clone())?,
        "normpdf" => mul(vec![int(-1), u.clone(), f1(cas, "normpdf", u.clone())?])?,
        _ => return Ok(unevaluated_diff(e, x)),
    };
    mul(vec![outer, du])
}

/// `diff(f)`, `diff(f, x)`, `diff(f, x, n)`.
pub fn cmd_diff(cas: &Cas, args: Vec<Expr>) -> R {
    if args.is_empty() || args.len() > 3 {
        return Err("Uso: diff(f, x) o diff(f, x, n)".into());
    }
    let f = &args[0];
    let (x, n) = match args.len() {
        1 => (auto_var(f)?, 1),
        2 => match &args[1] {
            Expr::Num(r) if r.is_int() => (auto_var(f)?, r.n),
            other => (var_arg(other, "diff")?, 1),
        },
        _ => (var_arg(&args[1], "diff")?, args[2].as_int().ok_or("diff: el orden debe ser un entero")?),
    };
    if !(0..=60).contains(&n) {
        return Err("diff: el orden debe estar entre 0 y 60".into());
    }
    let mut r = f.clone();
    for _ in 0..n {
        r = tidy_frac(diff(cas, &r, &x)?);
        if r.is_zero() {
            break;
        }
    }
    Ok(r)
}

// ====================================================================== polynomials

/// Monomial `c·x^k` (c free of x) → (k, c).
fn mono(t: &Expr, x: &str) -> Option<(usize, Expr)> {
    if t.free_of(x) {
        return Some((0, t.clone()));
    }
    match t {
        Expr::Sym(_) => Some((1, Expr::one())),
        Expr::Pow(b, e) => {
            if matches!(b.as_ref(), Expr::Sym(s) if s == x) {
                let k = e.as_rat()?;
                if k.is_int() && k.n >= 1 && k.n <= 64 {
                    return Some((k.n as usize, Expr::one()));
                }
            }
            None
        }
        Expr::Mul(v) => {
            let mut deg = 0;
            let mut rest = Vec::new();
            for f in v {
                let (k, c) = mono(f, x)?;
                deg += k;
                rest.push(c);
            }
            Some((deg, mul(rest).ok()?))
        }
        _ => None,
    }
}

/// Coefficients (low → high) of `e` as a polynomial in `x` (coefficients free of x).
fn poly_coeffs(e: &Expr, x: &str) -> Option<Vec<Expr>> {
    if e.size() > 1500 {
        return None;
    }
    let ex = expand(e).ok()?;
    let terms = match ex {
        Expr::Add(v) => v,
        o => vec![o],
    };
    let mut c: Vec<Vec<Expr>> = Vec::new();
    for t in &terms {
        let (k, co) = mono(t, x)?;
        if c.len() <= k {
            c.resize(k + 1, Vec::new());
        }
        c[k].push(co);
    }
    let mut out: Vec<Expr> = c.into_iter().map(add).collect::<Result<_, _>>().ok()?;
    while matches!(out.last(), Some(z) if z.is_zero()) {
        out.pop();
    }
    Some(out)
}

/// `e = a·x + b` with `a ≠ 0` free of x.
fn linear(e: &Expr, x: &str) -> Option<(Expr, Expr)> {
    if let Expr::Sym(s) = e {
        if s == x {
            return Some((Expr::one(), Expr::zero()));
        }
    }
    let c = poly_coeffs(e, x)?;
    if c.len() == 2 {
        Some((c[1].clone(), c[0].clone()))
    } else {
        None
    }
}

type P = Vec<Rat>;

fn rat_poly(e: &Expr, x: &str) -> Option<P> {
    poly_coeffs(e, x)?.iter().map(|c| c.as_rat()).collect()
}
fn ptrim(mut p: P) -> P {
    while matches!(p.last(), Some(r) if r.is_zero()) {
        p.pop();
    }
    p
}
fn pscale(a: &[Rat], c: Rat) -> Option<P> {
    Some(ptrim(a.iter().map(|x| x.mul(c)).collect::<Option<Vec<_>>>()?))
}
fn pmul(a: &[Rat], b: &[Rat]) -> Option<P> {
    if a.is_empty() || b.is_empty() {
        return Some(vec![]);
    }
    let mut o = vec![Rat::ZERO; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            o[i + j] = o[i + j].add(x.mul(*y)?)?;
        }
    }
    Some(ptrim(o))
}
fn ppow(a: &[Rat], m: usize) -> Option<P> {
    let mut acc = vec![Rat::ONE];
    for _ in 0..m {
        acc = pmul(&acc, a)?;
    }
    Some(acc)
}
fn pdivmod(a: &[Rat], b: &[Rat]) -> Option<(P, P)> {
    let b = ptrim(b.to_vec());
    let a = ptrim(a.to_vec());
    if b.is_empty() {
        return None;
    }
    if a.len() < b.len() {
        return Some((vec![], a));
    }
    let db = b.len() - 1;
    let lb = b[db];
    let mut r = a.clone();
    let mut q = vec![Rat::ZERO; a.len() - db];
    for i in (0..q.len()).rev() {
        let c = r[i + db].div(lb)?;
        q[i] = c;
        for j in 0..=db {
            r[i + j] = r[i + j].sub(c.mul(b[j])?)?;
        }
    }
    r.truncate(db);
    Some((ptrim(q), ptrim(r)))
}
fn peval(p: &[Rat], x: Rat) -> Option<Rat> {
    let mut acc = Rat::ZERO;
    for c in p.iter().rev() {
        acc = acc.mul(x)?.add(*c)?;
    }
    Some(acc)
}
fn p_expr(p: &[Rat], x: &Expr) -> R {
    let mut t = Vec::new();
    for (k, c) in p.iter().enumerate() {
        if !c.is_zero() {
            t.push(mul(vec![rat(*c), pow(x.clone(), int(k as i128))?])?);
        }
    }
    add(t)
}

fn divisors(n: i128) -> Option<Vec<i128>> {
    let n = n.abs();
    if n == 0 || n > 1_000_000_000_000 {
        return None;
    }
    let mut v = Vec::new();
    let mut i = 1i128;
    while i * i <= n {
        if n % i == 0 {
            v.push(i);
            if i != n / i {
                v.push(n / i);
            }
        }
        i += 1;
    }
    Some(v)
}

/// Rational roots (with multiplicity) and the remaining factor.
fn rat_roots(p: &[Rat]) -> Option<(Vec<(Rat, usize)>, P)> {
    let mut p = ptrim(p.to_vec());
    let mut roots = Vec::new();
    let mut z = 0;
    while p.len() > 1 && p[0].is_zero() {
        p.remove(0);
        z += 1;
    }
    if z > 0 {
        roots.push((Rat::ZERO, z));
    }
    if p.len() <= 1 {
        return Some((roots, p));
    }
    let mut l: i128 = 1;
    for c in &p {
        l = l.checked_mul(c.d / crate::num::gcd_i128(l, c.d))?;
    }
    let ints: Vec<i128> = p.iter().map(|c| c.n.checked_mul(l / c.d)).collect::<Option<_>>()?;
    let dp = divisors(ints[0])?;
    let dq = divisors(*ints.last().unwrap())?;
    let mut cands: Vec<Rat> = Vec::new();
    for a in &dp {
        for b in &dq {
            if let Some(r) = Rat::new(*a, *b) {
                cands.push(r);
                cands.push(r.neg()?);
            }
        }
        if cands.len() > 20000 {
            break;
        }
    }
    cands.sort();
    cands.dedup();
    for r in cands {
        let mut m = 0;
        while p.len() > 1 && peval(&p, r).map_or(false, |v| v.is_zero()) {
            let (q, _) = pdivmod(&p, &[r.neg()?, Rat::ONE])?;
            p = q;
            m += 1;
        }
        if m > 0 {
            roots.push((r, m));
        }
        if p.len() <= 1 {
            break;
        }
    }
    Some((roots, p))
}

/// Factors a monic polynomial without rational roots into monic quadratics.
fn factor_quadratics(p: &[Rat]) -> Option<Vec<(P, usize)>> {
    let n = p.len().checked_sub(1)?;
    if n == 0 {
        return Some(vec![]);
    }
    if n % 2 == 1 {
        return None;
    }
    if n == 2 {
        return Some(vec![(p.to_vec(), 1)]);
    }
    // perfect power of a quadratic
    let m = n / 2;
    let mr = Rat::int(m as i128);
    let pc = p[n - 1].div(mr)?;
    let c2 = Rat::int((m * (m - 1) / 2) as i128);
    let s = p[n - 2].sub(c2.mul(pc.mul(pc)?)?)?.div(mr)?;
    let q = vec![s, pc, Rat::ONE];
    if ppow(&q, m)? == p.to_vec() {
        return Some(vec![(q, m)]);
    }
    if n == 4 && p.iter().all(|c| c.is_int()) {
        // (x² + a x + b)(x² + c x + d)
        let (c0, c1, c2, c3) = (p[0].n, p[1].n, p[2].n, p[3].n);
        for b in divisors(c0)? {
            for b in [b, -b] {
                let dd = c0 / b;
                let mut pairs: Vec<(Rat, Rat)> = Vec::new();
                if dd != b {
                    if let Some(a) = Rat::new(c1 - b * c3, dd - b) {
                        pairs.push((a, Rat::int(c3).sub(a)?));
                    }
                } else if c1 == b * c3 {
                    // a + c = c3, a c = c2 - 2b
                    let disc = c3 * c3 - 4 * (c2 - 2 * b);
                    if disc >= 0 {
                        let r = (disc as f64).sqrt().round() as i128;
                        if r * r == disc {
                            pairs.push((Rat::new(c3 + r, 2)?, Rat::new(c3 - r, 2)?));
                        }
                    }
                }
                for (a, c) in pairs {
                    let q1 = vec![Rat::int(b), a, Rat::ONE];
                    let q2 = vec![Rat::int(dd), c, Rat::ONE];
                    if pmul(&q1, &q2)? == p.to_vec() {
                        if q1 == q2 {
                            return Some(vec![(q1, 2)]);
                        }
                        return Some(vec![(q1, 1), (q2, 1)]);
                    }
                }
            }
        }
    }
    None
}

fn solve_lin(mut m: Vec<Vec<Rat>>, mut rhs: Vec<Rat>) -> Option<Vec<Rat>> {
    let n = rhs.len();
    for col in 0..n {
        let piv = (col..n).find(|&r| !m[r][col].is_zero())?;
        m.swap(col, piv);
        rhs.swap(col, piv);
        let inv = m[col][col].recip()?;
        for j in 0..n {
            m[col][j] = m[col][j].mul(inv)?;
        }
        rhs[col] = rhs[col].mul(inv)?;
        for r in 0..n {
            if r != col && !m[r][col].is_zero() {
                let f = m[r][col];
                for j in 0..n {
                    m[r][j] = m[r][j].sub(f.mul(m[col][j])?)?;
                }
                rhs[r] = rhs[r].sub(f.mul(rhs[col])?)?;
            }
        }
    }
    Some(rhs)
}

/// Factored form `c·Π(q·x − p)^m·rest` of a rational polynomial, if it has rational roots.
fn factor_rat_poly(p: &[Rat], x: &Expr) -> Option<Expr> {
    let (roots, rest) = rat_roots(p)?;
    if roots.is_empty() {
        return None;
    }
    let mut coef = *rest.last()?;
    let rest_monic = pscale(&rest, coef.recip()?)?;
    let mut fs = Vec::new();
    for (r, m) in roots {
        let lin = sub(mul(vec![int(r.d), x.clone()]).ok()?, int(r.n)).ok()?;
        coef = coef.div(Rat::int(r.d).powi(m as i64)?)?;
        fs.push(pow(lin, int(m as i128)).ok()?);
    }
    if rest_monic.len() > 1 {
        fs.push(p_expr(&rest_monic, x).ok()?);
    }
    fs.push(rat(coef));
    mul(fs).ok()
}

// ====================================================================== integration

struct Integ<'c, 's> {
    cas: &'c Cas<'s>,
    budget: Cell<usize>,
}

/// Structural replacement of the sub-expression `g` by `u`, also matching powers
/// of `g` (`x^4` → `u^2` for `g = x^2`, `x` → `u^2` for `g = √x`) and of `g/c`
/// when `g = c·h` with numeric c.
fn replace(e: &Expr, g: &Expr, u: &Expr) -> Expr {
    let mut pats = vec![(g.clone(), u.clone())];
    if let Expr::Mul(v) = g {
        if let Some(Expr::Num(c)) = v.first() {
            let h = if v.len() == 2 { v[1].clone() } else { Expr::Mul(v[1..].to_vec()) };
            if let Some(ic) = c.recip() {
                pats.push((h, Expr::Mul(vec![Expr::Num(ic), u.clone()])));
            }
        }
    }
    replace_pats(e, &pats)
}

fn replace_pats(e: &Expr, pats: &[(Expr, Expr)]) -> Expr {
    for (p, w) in pats {
        if e == p {
            return w.clone();
        }
        if let Expr::Pow(gb, gp) = p {
            if let Expr::Pow(eb, ep) = e {
                if eb == gb {
                    if let Ok(Expr::Num(r)) = div((**ep).clone(), (**gp).clone()) {
                        return Expr::Pow(Box::new(w.clone()), Box::new(Expr::Num(r)));
                    }
                }
            }
            if e == gb.as_ref() && matches!(e, Expr::Sym(_)) {
                if let Ok(Expr::Num(r)) = pow((**gp).clone(), int(-1)) {
                    if r.is_int() {
                        return Expr::Pow(Box::new(w.clone()), Box::new(Expr::Num(r)));
                    }
                }
            }
        }
    }
    match e {
        Expr::Add(v) => Expr::Add(v.iter().map(|t| replace_pats(t, pats)).collect()),
        Expr::Mul(v) => Expr::Mul(v.iter().map(|t| replace_pats(t, pats)).collect()),
        Expr::Pow(a, b) => Expr::Pow(Box::new(replace_pats(a, pats)), Box::new(replace_pats(b, pats))),
        Expr::Call(n, a) => Expr::Call(n.clone(), a.iter().map(|t| replace_pats(t, pats)).collect()),
        other => other.clone(),
    }
}

/// Candidate inner functions for u-substitution.
fn collect_cands(e: &Expr, x: &str, out: &mut Vec<Expr>) {
    let push = |c: &Expr, out: &mut Vec<Expr>| {
        if !c.free_of(x) && !matches!(c, Expr::Sym(_)) && !out.contains(c) && out.len() < 24 {
            out.push(c.clone());
        }
    };
    match e {
        Expr::Call(_, a) => {
            push(e, out);
            for t in a {
                push(t, out);
                collect_cands(t, x, out);
            }
        }
        Expr::Pow(b, p) => {
            push(b, out);
            if !p.free_of(x) {
                push(p, out);
            }
            if p.as_int().is_none() {
                push(e, out);
            }
            collect_cands(b, x, out);
            collect_cands(p, x, out);
        }
        Expr::Add(v) | Expr::Mul(v) => v.iter().for_each(|t| collect_cands(t, x, out)),
        _ => {}
    }
}

/// `a` is `c·b` for a constant c (both polynomials in x).
fn proportional(a: &Expr, b: &Expr, x: &str) -> Option<Expr> {
    let pa = poly_coeffs(a, x)?;
    let pb = poly_coeffs(b, x)?;
    if pa.len() != pb.len() || pa.len() < 2 {
        return None;
    }
    let c = div(pa.last()?.clone(), pb.last()?.clone()).ok()?;
    for (u, v) in pa.iter().zip(&pb) {
        if !sub(u.clone(), mul(vec![c.clone(), v.clone()]).ok()?).ok()?.is_zero() {
            return None;
        }
    }
    Some(c)
}

/// Merges product factors whose bases are proportional sums: (4x+2)·(2x+1)^-1 → 2.
fn merge_prop(e: &Expr, x: &str) -> Option<Expr> {
    let v = match e {
        Expr::Mul(v) => v.clone(),
        _ => return None,
    };
    let be = |f: &Expr| -> (Expr, Expr) {
        match f {
            Expr::Pow(b, p) => ((**b).clone(), (**p).clone()),
            o => (o.clone(), Expr::one()),
        }
    };
    for i in 0..v.len() {
        for j in 0..v.len() {
            if i == j {
                continue;
            }
            let (bi, pi) = be(&v[i]);
            let (bj, _) = be(&v[j]);
            if !matches!(bi, Expr::Add(_)) || !matches!(bj, Expr::Add(_)) || bi == bj {
                continue;
            }
            if let Some(c) = proportional(&bi, &bj, x) {
                let mut nv = v.clone();
                nv[i] = mul(vec![pow(c, pi.clone()).ok()?, pow(bj, pi).ok()?]).ok()?;
                let r = mul(nv).ok()?;
                return Some(merge_prop(&r, x).unwrap_or(r));
            }
        }
    }
    None
}

fn is_algebraic(e: &Expr, x: &str) -> bool {
    match e {
        _ if e.free_of(x) => true,
        Expr::Sym(_) => true,
        Expr::Add(v) | Expr::Mul(v) => v.iter().all(|t| is_algebraic(t, x)),
        Expr::Pow(b, p) => p.free_of(x) && is_algebraic(b, x),
        _ => false,
    }
}

impl<'c, 's> Integ<'c, 's> {
    fn int(&self, e: &Expr, x: &str, depth: usize) -> Option<Expr> {
        if depth > 14 || e.size() > 800 {
            return None;
        }
        let b = self.budget.get();
        if b == 0 {
            return None;
        }
        self.budget.set(b - 1);
        let xs = Expr::sym(x);
        if e.free_of(x) {
            return mul(vec![e.clone(), xs]).ok();
        }
        match e {
            Expr::Sym(_) => return mul(vec![half(), sq(xs).ok()?]).ok(),
            Expr::Add(v) => {
                let mut out = Vec::new();
                let mut ok = true;
                for t in v {
                    match self.int(t, x, depth + 1) {
                        Some(r) => out.push(r),
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok {
                    return add(out).ok();
                }
                return self.rational(e, x).or_else(|| self.usub(e, x, depth));
            }
            Expr::Mul(v) => {
                let (c, rest): (Vec<Expr>, Vec<Expr>) = v.iter().cloned().partition(|f| f.free_of(x));
                if !c.is_empty() {
                    let cc = mul(c).ok()?;
                    let r = mul(rest).ok()?;
                    let ir = self.int(&r, x, depth + 1)?;
                    return mul(vec![cc, ir]).ok();
                }
            }
            Expr::List(v) => return Some(Expr::List(v.iter().map(|t| self.int(t, x, depth + 1)).collect::<Option<_>>()?)),
            _ => {}
        }
        self.table(e, x)
            .or_else(|| self.trig_power(e, x, depth))
            .or_else(|| self.quad_forms(e, x))
            .or_else(|| self.rational(e, x))
            .or_else(|| self.usub(e, x, depth))
            .or_else(|| self.exp_trig(e, x))
            .or_else(|| self.sin_cos(e, x, depth))
            .or_else(|| self.trig_product(e, x, depth))
            .or_else(|| self.linear_sub(e, x, depth))
            .or_else(|| self.parts(e, x, depth))
            .or_else(|| self.expand_retry(e, x, depth))
    }

    /// Antiderivative of f(u) w.r.t. u for u = a·x + b; returns (F(u), is_trig).
    fn prim_call(&self, name: &str, u: &Expr) -> Option<(Expr, bool)> {
        let cas = self.cas;
        let u = u.clone();
        let c = |n: &str| f1(cas, n, u.clone()).ok();
        let ik = pow(kfac(cas), int(-1)).ok()?;
        let one_m_u2 = sub(int(1), sq(u.clone()).ok()?).ok()?;
        let one_p_u2 = add(vec![int(1), sq(u.clone()).ok()?]).ok()?;
        Some(match name {
            "sin" => (neg(c("cos")?).ok()?, true),
            "cos" => (c("sin")?, true),
            "tan" => (neg(ln_abs(cas, c("cos")?).ok()?).ok()?, true),
            "cot" => (ln_abs(cas, c("sin")?).ok()?, true),
            "sec" => (ln_abs(cas, add(vec![c("sec")?, c("tan")?]).ok()?).ok()?, true),
            "csc" => (neg(ln_abs(cas, add(vec![c("csc")?, c("cot")?]).ok()?).ok()?).ok()?, true),
            "sinh" => (c("cosh")?, false),
            "cosh" => (c("sinh")?, false),
            "tanh" => (ln(cas, c("cosh")?).ok()?, false),
            "exp" => (pow(Expr::sym("e"), u).ok()?, false),
            "ln" => (sub(mul(vec![u.clone(), c("ln")?]).ok()?, u).ok()?, false),
            "log" => (div(sub(mul(vec![u.clone(), c("ln")?]).ok()?, u).ok()?, ln(cas, int(10)).ok()?).ok()?, false),
            "asin" => (add(vec![mul(vec![u.clone(), c("asin")?]).ok()?, mul(vec![ik, sqrt(one_m_u2).ok()?]).ok()?]).ok()?, false),
            "acos" => (sub(mul(vec![u.clone(), c("acos")?]).ok()?, mul(vec![ik, sqrt(one_m_u2).ok()?]).ok()?).ok()?, false),
            "atan" => (sub(mul(vec![u.clone(), c("atan")?]).ok()?, mul(vec![ik, half(), ln(cas, one_p_u2).ok()?]).ok()?).ok()?, false),
            "acot" => (add(vec![mul(vec![u.clone(), c("acot")?]).ok()?, mul(vec![ik, half(), ln(cas, one_p_u2).ok()?]).ok()?]).ok()?, false),
            "asinh" => (sub(mul(vec![u.clone(), c("asinh")?]).ok()?, sqrt(add(vec![sq(u.clone()).ok()?, int(1)]).ok()?).ok()?).ok()?, false),
            "acosh" => (sub(mul(vec![u.clone(), c("acosh")?]).ok()?, sqrt(sub(sq(u.clone()).ok()?, int(1)).ok()?).ok()?).ok()?, false),
            "atanh" => (add(vec![mul(vec![u.clone(), c("atanh")?]).ok()?, mul(vec![half(), ln_abs(cas, one_m_u2).ok()?]).ok()?]).ok()?, false),
            "erf" => (
                add(vec![
                    mul(vec![u.clone(), c("erf")?]).ok()?,
                    mul(vec![pow(Expr::sym("pi"), Expr::rat(-1, 2)).ok()?, pow(Expr::sym("e"), neg(sq(u.clone()).ok()?).ok()?).ok()?]).ok()?,
                ])
                .ok()?,
                false,
            ),
            "abs" => (mul(vec![half(), u.clone(), c("abs")?]).ok()?, false),
            "sign" => (c("abs")?, false),
            _ => return None,
        })
    }

    /// Direct table: powers / exponentials / elementary functions of a linear argument.
    fn table(&self, e: &Expr, x: &str) -> Option<Expr> {
        let cas = self.cas;
        match e {
            Expr::Pow(b, p) => {
                if p.free_of(x) {
                    let (a, _) = linear(b, x)?;
                    if p.as_int() == Some(-1) {
                        return div(ln_abs(cas, (**b).clone()).ok()?, a).ok();
                    }
                    let p1 = add(vec![(**p).clone(), int(1)]).ok()?;
                    return div(pow((**b).clone(), p1.clone()).ok()?, mul(vec![p1, a]).ok()?).ok();
                }
                if b.free_of(x) {
                    let (a, _) = linear(p, x)?;
                    if matches!(b.as_ref(), Expr::Sym(s) if s == "e") {
                        return div(e.clone(), a).ok();
                    }
                    return div(e.clone(), mul(vec![a, ln(cas, (**b).clone()).ok()?]).ok()?).ok();
                }
                None
            }
            Expr::Call(name, args) => {
                if !(args.len() == 1 || (name == "log" && args.len() == 2 && args[1].free_of(x))) {
                    return None;
                }
                let u = &args[0];
                let (a, _) = linear(u, x)?;
                let (fu, trig) = if name == "log" && args.len() == 2 {
                    let (f, _) = self.prim_call("ln", u)?;
                    (div(f, ln(cas, args[1].clone()).ok()?).ok()?, false)
                } else {
                    self.prim_call(name, u)?
                };
                let mut den = a;
                if trig {
                    den = mul(vec![den, kfac(cas)]).ok()?;
                }
                div(fu, den).ok()
            }
            _ => None,
        }
    }

    /// Powers of trig functions of a linear argument (reduction formulas).
    fn trig_power(&self, e: &Expr, x: &str, depth: usize) -> Option<Expr> {
        let cas = self.cas;
        let (base, p) = match e {
            Expr::Pow(b, p) => (b.as_ref(), p.as_int()?),
            _ => return None,
        };
        let (name, u) = match base {
            Expr::Call(n, a) if a.len() == 1 => (n.as_str(), &a[0]),
            _ => return None,
        };
        if !matches!(name, "sin" | "cos" | "tan" | "cot" | "sec" | "csc") {
            return None;
        }
        let (a, _) = linear(u, x)?;
        if p < 0 {
            let r = match name {
                "sin" => "csc",
                "cos" => "sec",
                "tan" => "cot",
                "cot" => "tan",
                "sec" => "cos",
                _ => "sin",
            };
            let ne = pow(f1(cas, r, u.clone()).ok()?, int(-p)).ok()?;
            return self.int(&ne, x, depth + 1);
        }
        let n = p;
        if !(2..=30).contains(&n) {
            return None;
        }
        let ae = mul(vec![a, kfac(cas)]).ok()?;
        let c = |nm: &str| f1(cas, nm, u.clone()).ok();
        let pw = |nm: &str, k: i128| -> Option<Expr> { pow(c(nm)?, int(k)).ok() };
        let rec = |m: i128| -> Option<Expr> {
            if m == 0 {
                return Some(Expr::sym(x));
            }
            self.int(&pow(c(name)?, int(m)).ok()?, x, depth + 1)
        };
        let (t1, coef, sign) = match name {
            "sin" => (mul(vec![int(-1), pw("sin", n - 1)?, c("cos")?, pow(mul(vec![int(n), ae]).ok()?, int(-1)).ok()?]).ok()?, Rat::new(n - 1, n)?, 1),
            "cos" => (mul(vec![pw("cos", n - 1)?, c("sin")?, pow(mul(vec![int(n), ae]).ok()?, int(-1)).ok()?]).ok()?, Rat::new(n - 1, n)?, 1),
            "tan" => (mul(vec![pw("tan", n - 1)?, pow(mul(vec![int(n - 1), ae]).ok()?, int(-1)).ok()?]).ok()?, Rat::ONE, -1),
            "cot" => (mul(vec![int(-1), pw("cot", n - 1)?, pow(mul(vec![int(n - 1), ae]).ok()?, int(-1)).ok()?]).ok()?, Rat::ONE, -1),
            "sec" => (mul(vec![pw("sec", n - 2)?, c("tan")?, pow(mul(vec![int(n - 1), ae]).ok()?, int(-1)).ok()?]).ok()?, Rat::new(n - 2, n - 1)?, 1),
            _ => (mul(vec![int(-1), pw("csc", n - 2)?, c("cot")?, pow(mul(vec![int(n - 1), ae]).ok()?, int(-1)).ok()?]).ok()?, Rat::new(n - 2, n - 1)?, 1),
        };
        if coef.is_zero() {
            return Some(t1);
        }
        let r = rec(n - 2)?;
        add(vec![t1, mul(vec![rat(coef), int(sign), r]).ok()?]).ok()
    }

    /// Quadratics under square roots, symbolic `1/(c₂x² + c₀)`, Gaussian `e^(quadratic)`.
    fn quad_forms(&self, e: &Expr, x: &str) -> Option<Expr> {
        let cas = self.cas;
        let (b, p) = match e {
            Expr::Pow(b, p) => (b.as_ref(), p.as_ref()),
            _ => return None,
        };
        let xs = Expr::sym(x);
        // Gaussian: e^(c2 x² + c1 x + c0), c2 < 0
        if matches!(b, Expr::Sym(s) if s == "e") {
            let c = rat_poly(p, x)?;
            if c.len() != 3 || !c[2].is_neg() {
                return None;
            }
            let (c0, c1, c2) = (c[0], c[1], c[2]);
            let h = c1.div(c2.mul(Rat::int(2))?)?;
            let cc = c0.sub(c1.mul(c1)?.div(c2.mul(Rat::int(4))?)?)?;
            let s = sqrt(rat(c2.neg()?)).ok()?;
            let t = add(vec![xs, rat(h)]).ok()?;
            let erf = f1(cas, "erf", mul(vec![s.clone(), t]).ok()?).ok()?;
            return mul(vec![sqrt(Expr::sym("pi")).ok()?, half(), pow(s, int(-1)).ok()?, pow(Expr::sym("e"), rat(cc)).ok()?, erf]).ok();
        }
        let pr = p.as_rat()?;
        if pr == Rat::MINUS_ONE {
            // symbolic 1/(c2 x² + c0)
            let c = poly_coeffs(b, x)?;
            if c.len() != 3 || !c[1].is_zero() || (c[0].as_rat().is_some() && c[2].as_rat().is_some()) {
                return None;
            }
            let s0 = sqrt_pos(&c[0])?;
            let s2 = sqrt_pos(&c[2])?;
            let arg = mul(vec![xs, s2.clone(), pow(s0.clone(), int(-1)).ok()?]).ok()?;
            return mul(vec![inv_trig_rad(cas, "atan", arg).ok()?, pow(mul(vec![s0, s2]).ok()?, int(-1)).ok()?]).ok();
        }
        if pr != Rat::new(1, 2)? && pr != Rat::new(-1, 2)? {
            return None;
        }
        let c = rat_poly(b, x)?;
        if c.len() != 3 {
            return None;
        }
        let (c0, c1, c2) = (c[0], c[1], c[2]);
        let h = c1.div(c2.mul(Rat::int(2))?)?;
        let cc = c0.sub(c1.mul(c1)?.div(c2.mul(Rat::int(4))?)?)?;
        let t = add(vec![xs, rat(h)]).ok()?;
        let q = b.clone();
        let sqq = sqrt(q).ok()?;
        let core = if c2.is_neg() {
            if cc.is_neg() || cc.is_zero() {
                return None;
            }
            // asin(t·√(-c2/cc)) / √(-c2)
            let arg = mul(vec![t.clone(), sqrt(rat(c2.neg()?.div(cc)?)).ok()?]).ok()?;
            mul(vec![inv_trig_rad(cas, "asin", arg).ok()?, pow(rat(c2.neg()?), Expr::rat(-1, 2)).ok()?]).ok()?
        } else {
            let s2 = sqrt(rat(c2)).ok()?;
            let inner = add(vec![mul(vec![s2.clone(), t.clone()]).ok()?, sqq.clone()]).ok()?;
            mul(vec![ln_abs(cas, inner).ok()?, pow(s2, int(-1)).ok()?]).ok()?
        };
        if pr.is_neg() {
            return Some(core);
        }
        // ∫√Q = (t/2)√Q + (cc/2)·∫1/√Q
        add(vec![mul(vec![half(), t, sqq]).ok()?, mul(vec![rat(cc), half(), core]).ok()?]).ok()
    }

    /// Rational functions with rational coefficients: division + partial fractions.
    fn rational(&self, e: &Expr, x: &str) -> Option<Expr> {
        let (n, d) = num_den(e);
        if d.is_one() || d.free_of(x) {
            return None;
        }
        let np = rat_poly(&n, x)?;
        let dp = rat_poly(&d, x)?;
        if dp.len() < 2 || dp.len() > 13 || np.len() > 30 {
            return None;
        }
        let xs = Expr::sym(x);
        let (q, r) = pdivmod(&np, &dp)?;
        let lc = *dp.last()?;
        let (roots, rest) = rat_roots(&dp)?;
        let rest_monic = pscale(&rest, rest.last()?.recip()?)?;
        let quads = factor_quadratics(&rest_monic)?;
        let dm = pscale(&dp, lc.recip()?)?;
        // basis of partial fractions
        enum K {
            Lin(Rat, usize),
            QuadX(usize, usize),
            Quad1(usize, usize),
        }
        let mut basis: Vec<(K, P)> = Vec::new();
        for (rt, m) in &roots {
            let lin = vec![rt.neg()?, Rat::ONE];
            for j in 1..=*m {
                let (b, rem) = pdivmod(&dm, &ppow(&lin, j)?)?;
                if !rem.is_empty() {
                    return None;
                }
                basis.push((K::Lin(*rt, j), b));
            }
        }
        for (qi, (qq, m)) in quads.iter().enumerate() {
            for j in 1..=*m {
                let (b, rem) = pdivmod(&dm, &ppow(qq, j)?)?;
                if !rem.is_empty() {
                    return None;
                }
                basis.push((K::QuadX(qi, j), pmul(&b, &[Rat::ZERO, Rat::ONE])?));
                basis.push((K::Quad1(qi, j), b));
            }
        }
        let nn = dm.len() - 1;
        if basis.len() != nn {
            return None;
        }
        let rr = pscale(&r, lc.recip()?)?;
        let mut mat = vec![vec![Rat::ZERO; nn]; nn];
        for (t, (_, bp)) in basis.iter().enumerate() {
            for (i, c) in bp.iter().enumerate() {
                if i < nn {
                    mat[i][t] = *c;
                }
            }
        }
        let rhs: Vec<Rat> = (0..nn).map(|i| *rr.get(i).unwrap_or(&Rat::ZERO)).collect();
        let sol = if rr.is_empty() { vec![Rat::ZERO; nn] } else { solve_lin(mat, rhs)? };
        let mut terms: Vec<Expr> = Vec::new();
        // polynomial part
        for (k, c) in q.iter().enumerate() {
            if !c.is_zero() {
                let k1 = (k + 1) as i128;
                terms.push(mul(vec![rat(c.div(Rat::int(k1))?), pow(xs.clone(), int(k1)).ok()?]).ok()?);
            }
        }
        // collect quadratic coefficients
        let mut qc: Vec<Vec<(Rat, Rat)>> = quads.iter().map(|(_, m)| vec![(Rat::ZERO, Rat::ZERO); *m]).collect();
        for (t, (k, _)) in basis.iter().enumerate() {
            let c = sol[t];
            match k {
                K::Lin(rt, j) => {
                    if c.is_zero() {
                        continue;
                    }
                    let lin = sub(mul(vec![int(rt.d), xs.clone()]).ok()?, int(rt.n)).ok()?;
                    if *j == 1 {
                        terms.push(mul(vec![rat(c), ln_abs(self.cas, lin).ok()?]).ok()?);
                    } else {
                        let j = *j as i128;
                        // c·qd^j ∫(qd x − p)^(-j) = c·qd^(j-1)·(qd x − p)^(1-j)/(1-j)
                        let co = c.mul(Rat::int(rt.d).powi((j - 1) as i64)?)?.div(Rat::int(1 - j))?;
                        terms.push(mul(vec![rat(co), pow(lin, int(1 - j)).ok()?]).ok()?);
                    }
                }
                K::QuadX(qi, j) => qc[*qi][j - 1].0 = c,
                K::Quad1(qi, j) => qc[*qi][j - 1].1 = c,
            }
        }
        for (qi, (qq, _)) in quads.iter().enumerate() {
            for (j, (bx, c1)) in qc[qi].iter().enumerate() {
                if bx.is_zero() && c1.is_zero() {
                    continue;
                }
                terms.push(self.int_quad(*bx, *c1, qq, j + 1, x)?);
            }
        }
        add(terms).ok()
    }

    /// ∫ (B x + C) / q^j for monic irreducible quadratic q.
    fn int_quad(&self, bx: Rat, c1: Rat, q: &[Rat], j: usize, x: &str) -> Option<Expr> {
        let cas = self.cas;
        let xs = Expr::sym(x);
        let (c, b) = (q[0], q[1]);
        let delta = c.mul(Rat::int(4))?.sub(b.mul(b)?)?;
        let qe = p_expr(q, &xs).ok()?;
        let lin = add(vec![mul(vec![int(2), xs.clone()]).ok()?, rat(b)]).ok()?;
        let mut terms = Vec::new();
        if !bx.is_zero() {
            let part = if j == 1 {
                if delta.is_neg() {
                    ln_abs(cas, qe.clone()).ok()?
                } else {
                    ln(cas, qe.clone()).ok()?
                }
            } else {
                let k = 1 - j as i128;
                mul(vec![pow(qe.clone(), int(k)).ok()?, Expr::rat(1, k)]).ok()?
            };
            terms.push(mul(vec![rat(bx.div(Rat::int(2))?), part]).ok()?);
        }
        let cc = c1.sub(bx.mul(b)?.div(Rat::int(2))?)?;
        if !cc.is_zero() {
            // I_1
            let mut ii = if delta.is_neg() {
                let s = sqrt(rat(delta.neg()?)).ok()?;
                let l1 = ln_abs(cas, sub(lin.clone(), s.clone()).ok()?).ok()?;
                let l2 = ln_abs(cas, add(vec![lin.clone(), s.clone()]).ok()?).ok()?;
                mul(vec![pow(s, int(-1)).ok()?, sub(l1, l2).ok()?]).ok()?
            } else {
                let s = sqrt(rat(delta)).ok()?;
                let at = inv_trig_rad(cas, "atan", mul(vec![lin.clone(), pow(s.clone(), int(-1)).ok()?]).ok()?).ok()?;
                mul(vec![int(2), pow(s, int(-1)).ok()?, at]).ok()?
            };
            // I_n = lin/((n-1)Δ q^(n-1)) + (2n-3)·2/((n-1)Δ)·I_{n-1}
            for nn in 2..=j {
                let n1 = Rat::int(nn as i128 - 1);
                let f = n1.mul(delta)?;
                let t1 = mul(vec![lin.clone(), rat(f.recip()?), pow(qe.clone(), int(1 - nn as i128)).ok()?]).ok()?;
                let k = Rat::int(2 * (2 * nn as i128 - 3)).div(f)?;
                ii = add(vec![t1, mul(vec![rat(k), ii]).ok()?]).ok()?;
            }
            terms.push(mul(vec![rat(cc), ii]).ok()?);
        }
        add(terms).ok()
    }

    /// Substitution u = g(x) when f = F(g)·g'.
    fn usub(&self, e: &Expr, x: &str, depth: usize) -> Option<Expr> {
        let cas = self.cas;
        let mut cands = Vec::new();
        collect_cands(e, x, &mut cands);
        let u = fresh("u");
        let us = Expr::sym(&u);
        for g in cands {
            if linear(&g, x).is_some() {
                continue;
            }
            let dg = match diff(cas, &g, x) {
                Ok(v) if !v.is_zero() => v,
                _ => continue,
            };
            let ratio = match pow(dg, int(-1)).and_then(|r| mul(vec![e.clone(), r])) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let mut attempts = vec![ratio.clone()];
            if let Some(m) = merge_prop(&ratio, x) {
                attempts.push(m);
            }
            for r in attempts {
                let rr = match cas.eval(&replace(&r, &g, &us)) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if !rr.free_of(x) {
                    continue;
                }
                if let Some(f) = self.int(&rr, &u, depth + 1) {
                    if let Ok(back) = cas.eval(&subst(&f, &u, &g)) {
                        return Some(back);
                    }
                }
            }
        }
        None
    }

    /// e^(αx+β)·sin/cos(γx+δ).
    fn exp_trig(&self, e: &Expr, x: &str) -> Option<Expr> {
        let cas = self.cas;
        let v = match e {
            Expr::Mul(v) if v.len() == 2 => v,
            _ => return None,
        };
        let (ex, tr) = if matches!(v[0], Expr::Call(..)) { (&v[1], &v[0]) } else { (&v[0], &v[1]) };
        let (b, p) = match ex {
            Expr::Pow(b, p) if b.free_of(x) => (b.as_ref(), p.as_ref()),
            _ => return None,
        };
        let (name, u) = match tr {
            Expr::Call(n, a) if a.len() == 1 && (n == "sin" || n == "cos") => (n.as_str(), &a[0]),
            _ => return None,
        };
        let (al, _) = linear(p, x)?;
        let (ga, _) = linear(u, x)?;
        let al = if matches!(b, Expr::Sym(s) if s == "e") { al } else { mul(vec![al, ln(cas, b.clone()).ok()?]).ok()? };
        let ga = mul(vec![ga, kfac(cas)]).ok()?;
        let den = add(vec![sq(al.clone()).ok()?, sq(ga.clone()).ok()?]).ok()?;
        let s = f1(cas, "sin", u.clone()).ok()?;
        let c = f1(cas, "cos", u.clone()).ok()?;
        let inner = if name == "sin" {
            sub(mul(vec![al, s]).ok()?, mul(vec![ga, c]).ok()?).ok()?
        } else {
            add(vec![mul(vec![al, c]).ok()?, mul(vec![ga, s]).ok()?]).ok()?
        };
        mul(vec![ex.clone(), inner, pow(den, int(-1)).ok()?]).ok()
    }

    /// sinᵐ(u)·cosⁿ(u) (same linear u): odd power → substitution, even powers → reduction.
    fn sin_cos(&self, e: &Expr, x: &str, depth: usize) -> Option<Expr> {
        let cas = self.cas;
        let v = match e {
            Expr::Mul(v) if v.len() == 2 => v,
            _ => return None,
        };
        let get = |t: &Expr| -> Option<(String, Expr, i128)> {
            let (b, k) = match t {
                Expr::Pow(b, p) => (b.as_ref(), p.as_int()?),
                o => (o, 1),
            };
            match b {
                Expr::Call(n, a) if a.len() == 1 && (n == "sin" || n == "cos") => Some((n.clone(), a[0].clone(), k)),
                _ => None,
            }
        };
        let (n1, u1, k1) = get(&v[0])?;
        let (n2, u2, k2) = get(&v[1])?;
        if u1 != u2 || n1 == n2 {
            return None;
        }
        let (m, n) = if n1 == "sin" { (k1, k2) } else { (k2, k1) };
        let u = u1;
        let (a, _) = linear(&u, x)?;
        let ae = mul(vec![a, kfac(cas)]).ok()?;
        let w = fresh("w");
        let ws = Expr::sym(&w);
        let one_m_w2 = sub(int(1), sq(ws.clone()).ok()?).ok()?;
        let (integrand, back) = if m.rem_euclid(2) == 1 && m > 0 {
            // w = cos(u), sin(u) du = −dw/ae
            let t = mul(vec![int(-1), pow(one_m_w2, int((m - 1) / 2)).ok()?, pow(ws.clone(), int(n)).ok()?, pow(ae, int(-1)).ok()?]).ok()?;
            (t, f1(cas, "cos", u.clone()).ok()?)
        } else if n.rem_euclid(2) == 1 && n > 0 {
            let t = mul(vec![pow(one_m_w2, int((n - 1) / 2)).ok()?, pow(ws.clone(), int(m)).ok()?, pow(ae, int(-1)).ok()?]).ok()?;
            (t, f1(cas, "sin", u.clone()).ok()?)
        } else if m >= 0 && n >= 0 {
            // cosⁿ = (1 − sin²)^(n/2): sum of sin powers
            let t = mul(vec![pow(sub(int(1), sq(f1(cas, "sin", u.clone()).ok()?).ok()?).ok()?, int(n / 2)).ok()?, pow(f1(cas, "sin", u.clone()).ok()?, int(m)).ok()?]).ok()?;
            let t = expand(&t).ok()?;
            return self.int(&t, x, depth + 1);
        } else {
            return None;
        };
        let it = expand(&integrand).ok()?;
        let f = self.int(&it, &w, depth + 1)?;
        cas.eval(&subst(&f, &w, &back)).ok()
    }

    /// Product-to-sum for two sin/cos factors (linear arguments) inside a product.
    fn trig_product(&self, e: &Expr, x: &str, depth: usize) -> Option<Expr> {
        let cas = self.cas;
        let v = match e {
            Expr::Mul(v) if v.len() >= 2 => v,
            _ => return None,
        };
        // expand each factor sin/cos(u)^k (k ≥ 1) into a multiset of single factors
        let mut trig: Vec<(String, Expr)> = Vec::new();
        let mut others: Vec<Expr> = Vec::new();
        for t in v {
            let (b, k) = match t {
                Expr::Pow(b, p) => match p.as_int() {
                    Some(k) if (1..=8).contains(&k) => (b.as_ref(), k),
                    _ => {
                        others.push(t.clone());
                        continue;
                    }
                },
                o => (o, 1),
            };
            match b {
                Expr::Call(n, a) if a.len() == 1 && (n == "sin" || n == "cos") && linear(&a[0], x).is_some() => {
                    for _ in 0..k {
                        trig.push((n.clone(), a[0].clone()));
                    }
                }
                _ => others.push(t.clone()),
            }
        }
        if trig.len() < 2 {
            return None;
        }
        let s = |z: Expr| f1(cas, "sin", z).ok();
        let c = |z: Expr| f1(cas, "cos", z).ok();
        let (n1, a) = trig[0].clone();
        let (n2, b) = trig[1].clone();
        let apb = add(vec![a.clone(), b.clone()]).ok()?;
        let amb = sub(a.clone(), b.clone()).ok()?;
        let sum = match (n1.as_str(), n2.as_str()) {
            ("sin", "cos") => add(vec![s(apb)?, s(amb)?]).ok()?,
            ("cos", "sin") => sub(s(apb)?, s(amb)?).ok()?,
            ("sin", "sin") => sub(c(amb)?, c(apb)?).ok()?,
            _ => add(vec![c(amb)?, c(apb)?]).ok()?,
        };
        let mut fs = vec![half(), sum];
        for (n, z) in &trig[2..] {
            fs.push(f1(cas, n, z.clone()).ok()?);
        }
        fs.extend(others);
        let r = expand(&mul(fs).ok()?).ok()?;
        if r.size() > 800 {
            return None;
        }
        self.int(&r, x, depth + 1)
    }

    /// Substitution u = a·x + b for products with powers of a linear expression.
    fn linear_sub(&self, e: &Expr, x: &str, depth: usize) -> Option<Expr> {
        let cas = self.cas;
        let v = match e {
            Expr::Mul(v) if v.len() >= 2 => v,
            _ => return None,
        };
        let g = v.iter().find_map(|t| match t {
            Expr::Pow(b, p) if matches!(b.as_ref(), Expr::Add(_)) && p.as_int().map_or(true, |k| k < 0) && linear(b, x).is_some() => Some((**b).clone()),
            _ => None,
        })?;
        if !v.iter().all(|t| is_algebraic(t, x)) {
            return None;
        }
        let (a, b) = linear(&g, x)?;
        let u = fresh("v");
        let us = Expr::sym(&u);
        let xr = div(sub(us.clone(), b).ok()?, a.clone()).ok()?;
        let rr = cas.eval(&subst(e, x, &xr)).ok()?;
        let rr = expand(&div(rr, a).ok()?).ok()?;
        if rr.size() > 800 {
            return None;
        }
        let f = self.int(&rr, &u, depth + 1)?;
        cas.eval(&subst(&f, &u, &g)).ok()
    }

    /// Integration by parts (polynomial × exp/trig, algebraic × log/inverse-trig).
    fn parts(&self, e: &Expr, x: &str, depth: usize) -> Option<Expr> {
        let cas = self.cas;
        let factors = match e {
            Expr::Mul(v) => v.clone(),
            o => vec![o.clone()],
        };
        let rest_of = |i: usize| -> Option<Expr> {
            let mut r = factors.clone();
            r.remove(i);
            mul(r).ok()
        };
        // polynomial × cyclic factor
        for i in 0..factors.len() {
            let t = &factors[i];
            let cyclic = match t {
                Expr::Pow(b, p) => b.free_of(x) && linear(p, x).is_some(),
                Expr::Call(n, a) => a.len() == 1 && matches!(n.as_str(), "sin" | "cos" | "sinh" | "cosh") && linear(&a[0], x).is_some(),
                _ => false,
            };
            if !cyclic {
                continue;
            }
            let pp = rest_of(i)?;
            let pc = poly_coeffs(&pp, x)?;
            if pc.len() < 2 || pc.len() > 12 {
                continue;
            }
            // tabular integration: Σ (−1)^k P^(k) T_(k+1)
            let mut terms = Vec::new();
            let mut pk = pp.clone();
            let mut tk = t.clone();
            let mut sign = 1;
            while !pk.is_zero() {
                tk = self.int(&tk, x, depth + 1)?;
                terms.push(mul(vec![int(sign), pk.clone(), tk.clone()]).ok()?);
                pk = diff(cas, &pk, x).ok()?;
                sign = -sign;
            }
            return add(terms).ok();
        }
        // algebraic × logarithmic / inverse trig
        for i in 0..factors.len() {
            let t = &factors[i];
            let is_log = |z: &Expr| matches!(z, Expr::Call(n, a) if a.len() == 1 && matches!(n.as_str(), "ln" | "log" | "asin" | "acos" | "atan" | "acot" | "asinh" | "acosh" | "atanh") && !a[0].free_of(x));
            let ok = is_log(t) || matches!(t, Expr::Pow(b, p) if is_log(b) && p.as_int().map_or(false, |k| k >= 2));
            if !ok {
                continue;
            }
            let pp = rest_of(i)?;
            if !is_algebraic(&pp, x) {
                continue;
            }
            let ip = self.int(&pp, x, depth + 1)?;
            let dl = diff(cas, t, x).ok()?;
            let prod = mul(vec![ip.clone(), dl]).ok()?;
            let prod = merge_prop(&prod, x).unwrap_or(prod);
            let r = self.int(&prod, x, depth + 1)?;
            return sub(mul(vec![t.clone(), ip]).ok()?, r).ok();
        }
        None
    }

    fn expand_retry(&self, e: &Expr, x: &str, depth: usize) -> Option<Expr> {
        let expandable = match e {
            Expr::Mul(v) => v.iter().any(|f| matches!(f, Expr::Add(_)) || matches!(f, Expr::Pow(b, p) if matches!(b.as_ref(), Expr::Add(_)) && p.as_int().map_or(false, |k| k > 1))),
            Expr::Pow(b, p) => matches!(b.as_ref(), Expr::Add(_)) && p.as_int().map_or(false, |k| k > 1),
            _ => false,
        };
        if !expandable {
            return None;
        }
        let ex = expand(e).ok()?;
        if ex == *e || ex.size() > 1500 {
            return None;
        }
        self.int(&ex, x, depth + 1)
    }
}

/// √c for "obviously positive" symbolic c (a², 4a², positive numbers, π...).
fn sqrt_pos(c: &Expr) -> Option<Expr> {
    match c {
        Expr::Num(r) if r.n > 0 => sqrt(c.clone()).ok(),
        Expr::Sym(s) if s == "pi" || s == "e" => sqrt(c.clone()).ok(),
        Expr::Pow(b, p) => {
            let k = p.as_int()?;
            if k % 2 == 0 {
                pow((**b).clone(), int(k / 2)).ok()
            } else {
                None
            }
        }
        Expr::Mul(v) => mul(v.iter().map(sqrt_pos).collect::<Option<Vec<_>>>()?).ok(),
        _ => None,
    }
}

/// Numerically checks that dF/dx = f at a few sample points.
fn verify(cas: &Cas, big_f: &Expr, f: &Expr, x: &str) -> bool {
    let df = match diff(cas, big_f, x) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut syms = vec![x.to_string()];
    f.symbols(&mut syms);
    big_f.symbols(&mut syms);
    let vars: Vec<&str> = syms.iter().map(|s| s.as_str()).collect();
    let (fc, dc) = match (cas.compile(f, &vars), cas.compile(&df, &vars)) {
        (Ok(a), Ok(b)) => (a, b),
        _ => return true, // cannot check numerically (complex, unknown functions)
    };
    let pvals = [0.731, 1.37, 0.513, 1.91, 2.29, 0.87];
    let xs = [0.37, 0.81, 1.23, 1.77, 2.41, -0.53, -1.19, 3.1, 0.13, -2.3];
    let mut valid = 0;
    for (k, &xv) in xs.iter().enumerate() {
        let mut v = vec![xv];
        for i in 1..vars.len() {
            v.push(pvals[(i + k) % pvals.len()]);
        }
        let a = fc.eval(&v);
        let b = dc.eval(&v);
        if a.is_finite() && b.is_finite() {
            if (a - b).abs() > 1e-6 * (1.0 + a.abs().max(b.abs())) {
                return false;
            }
            valid += 1;
        }
    }
    let _ = valid;
    true
}

fn antiderivative(cas: &Cas, f: &Expr, x: &str) -> Option<Expr> {
    let it = Integ { cas, budget: Cell::new(3000) };
    let big_f = it.int(f, x, 0)?;
    let big_f = cas.eval(&strip_abs(&big_f)).ok()?;
    let big_f = tidy(big_f);
    if !verify(cas, &big_f, f, x) {
        return None;
    }
    Some(big_f)
}

const ERR_NO_PRIM: &str = "No se encontró una primitiva simbólica; indica límites para integrar numéricamente, p. ej. integrate(f, x, 0, 1)";

/// `integrate(f, x)` (indefinite) and `integrate(f, x, a, b)` (definite).
pub fn cmd_integrate(cas: &Cas, args: Vec<Expr>) -> R {
    match args.len() {
        1 => indefinite(cas, &args[0], &auto_var(&args[0])?),
        2 => indefinite(cas, &args[0], &var_arg(&args[1], "integrate")?),
        3 => {
            let x = auto_var(&args[0])?;
            definite(cas, &args[0], &x, &args[1], &args[2])
        }
        4 => definite(cas, &args[0], &var_arg(&args[1], "integrate")?, &args[2], &args[3]),
        _ => Err("Uso: integrate(f, x) o integrate(f, x, a, b)".into()),
    }
}

fn indefinite(cas: &Cas, f: &Expr, x: &str) -> R {
    match f {
        Expr::List(v) => Ok(Expr::List(v.iter().map(|t| indefinite(cas, t, x)).collect::<Result<_, _>>()?)),
        Expr::Eq(a, b) => Ok(Expr::Eq(Box::new(indefinite(cas, a, x)?), Box::new(indefinite(cas, b, x)?))),
        Expr::Matrix(_) | Expr::Rel(..) | Expr::Text(_) => Err("integrate: argumento no válido".into()),
        _ => antiderivative(cas, f, x).ok_or_else(|| ERR_NO_PRIM.to_string()),
    }
}

enum Bound {
    Fin(Expr),
    Inf,
    Inf2,
}

/// Value of F at a bound (exact substitution or one-sided limit).
fn eval_at(cas: &Cas, big_f: &Expr, x: &str, v: &Expr, side: i32) -> Result<Bound, String> {
    if !is_pos_inf(v) && !is_neg_inf(v) {
        if let Ok(r) = cas.eval(&subst(big_f, x, v)) {
            if finite_val(cas, &r) {
                return Ok(Bound::Fin(r));
            }
        }
    }
    let l = limit_core(cas, big_f, x, v, side)?;
    if is_pos_inf(&l) {
        Ok(Bound::Inf)
    } else if is_neg_inf(&l) {
        Ok(Bound::Inf2)
    } else {
        Ok(Bound::Fin(l))
    }
}

fn exact_definite(cas: &Cas, big_f: &Expr, x: &str, a: &Expr, b: &Expr) -> Result<Expr, String> {
    let fb = eval_at(cas, big_f, x, b, -1)?;
    let fa = eval_at(cas, big_f, x, a, 1)?;
    match (fb, fa) {
        (Bound::Fin(u), Bound::Fin(v)) => Ok(tidy(sub(u, v)?)),
        (Bound::Inf, Bound::Fin(_)) | (Bound::Fin(_), Bound::Inf2) => Ok(inf()),
        (Bound::Inf2, Bound::Fin(_)) | (Bound::Fin(_), Bound::Inf) => Ok(neg_inf()),
        _ => Err("La integral no converge".into()),
    }
}

fn bound_f64(cas: &Cas, e: &Expr) -> Result<f64, String> {
    if is_pos_inf(e) {
        return Ok(f64::INFINITY);
    }
    if is_neg_inf(e) {
        return Ok(f64::NEG_INFINITY);
    }
    cas.num_real(e)
}

/// Detects non-integrable singularities of f in [a, b]: candidate points are roots
/// of the denominator and spikes of |f|; at each one, h·|f(s ± h)| must decay.
/// Returns the singular points found (Err when one is not integrable).
fn singularities(cas: &Cas, f: &Expr, x: &str, a: f64, b: f64, fc: &dyn Fn(f64) -> f64) -> Result<Vec<f64>, ()> {
    let mut pts: Vec<f64> = Vec::new();
    let (_, d) = num_den(f);
    if !d.free_of(x) {
        if let Ok(dc) = cas.func1(&d, x) {
            pts.extend(numeric::find_roots(&mut |t| dc(t), a, b, 400));
        }
    }
    const N: usize = 2000;
    let step = (b - a) / N as f64;
    let ys: Vec<f64> = (0..=N).map(|k| fc(a + step * k as f64).abs()).collect();
    let mut fin: Vec<f64> = ys.iter().cloned().filter(|v| v.is_finite()).collect();
    fin.sort_by(|p, q| p.total_cmp(q));
    let med = fin.get(fin.len() / 2).cloned().unwrap_or(0.0);
    for k in 0..=N {
        let y = ys[k];
        let left = if k > 0 { ys[k - 1] } else { 0.0 };
        let right = if k < N { ys[k + 1] } else { 0.0 };
        if !y.is_finite() || (y > 1e3 * (med + 1e-300) && y >= left && y >= right) {
            let lo = (a + step * (k as f64 - 1.0)).max(a);
            let hi = (a + step * (k as f64 + 1.0)).min(b);
            if let Some(s) = numeric::golden_min(&mut |t| { let v = fc(t).abs(); if v.is_finite() { -v } else { f64::NEG_INFINITY } }, lo, hi) {
                pts.push(s);
            }
        }
        if pts.len() > 50 {
            break;
        }
    }
    let sc = (b - a).abs().max(1e-300);
    for p in pts.iter_mut() {
        if let Some(r) = Rat::approximate(*p, 100, 1e-6 * sc) {
            *p = r.to_f64();
        }
    }
    for &s in &pts {
        for side in [1.0, -1.0] {
            let (h1, h2) = (1e-3 * sc, 1e-9 * sc);
            let (p1, p2) = (s + side * h1, s + side * h2);
            if p2 < a || p2 > b {
                continue;
            }
            let v1 = fc(p1).abs() * h1;
            let v2 = fc(p2).abs() * h2;
            if v2.is_nan() || v1.is_nan() {
                continue;
            }
            if v2.is_infinite() || (v1 > 1e-12 && v2 > 0.6 * v1) {
                return Err(());
            }
        }
    }
    pts.retain(|p| *p > a && *p < b);
    pts.sort_by(|p, q| p.total_cmp(q));
    pts.dedup_by(|p, q| (*p - *q).abs() < 1e-9 * sc);
    Ok(pts)
}

fn definite(cas: &Cas, f: &Expr, x: &str, a: &Expr, b: &Expr) -> R {
    if a == b {
        return Ok(Expr::zero());
    }
    if let Expr::List(v) = f {
        return Ok(Expr::List(v.iter().map(|t| definite(cas, t, x, a, b)).collect::<Result<_, _>>()?));
    }
    let mut syms = Vec::new();
    f.symbols(&mut syms);
    let params = syms.iter().any(|s| s != x) || a.has_free_symbols() || b.has_free_symbols();
    let big_f = antiderivative(cas, f, x);
    if params {
        let big_f = big_f.ok_or("No se encontró una primitiva simbólica (la integral tiene parámetros)")?;
        return exact_definite(cas, &big_f, x, a, b);
    }
    let fa = bound_f64(cas, a)?;
    let fb = bound_f64(cas, b)?;
    let fc = cas.func1(f, x)?;
    let mut cuts = Vec::new();
    if fa.is_finite() && fb.is_finite() {
        match singularities(cas, f, x, fa.min(fb), fa.max(fb), &fc) {
            Ok(p) => cuts = p,
            Err(()) => return Err("La integral diverge (singularidad no integrable en el intervalo)".into()),
        }
    }
    let numv = if cuts.is_empty() {
        numeric::integrate(&mut |t| fc(t), fa, fb, 1e-11)
    } else {
        let mut knots = vec![fa.min(fb)];
        knots.extend(cuts);
        knots.push(fa.max(fb));
        let mut tot = 0.0;
        let mut res = Ok(0.0);
        for w in knots.windows(2) {
            match numeric::integrate(&mut |t| fc(t), w[0], w[1], 1e-11) {
                Ok(v) => tot += v,
                Err(e) => {
                    res = Err(e);
                    break;
                }
            }
        }
        res.map(|_| if fa <= fb { tot } else { -tot })
    };
    if let Some(big_f) = big_f {
        if let Ok(v) = exact_definite(cas, &big_f, x, a, b) {
            if is_pos_inf(&v) || is_neg_inf(&v) {
                return Err("La integral diverge".into());
            }
            if let Ok(z) = cas.num(&v) {
                if z.is_finite() && z.im.abs() <= 1e-12 * (1.0 + z.re.abs()) {
                    match &numv {
                        Ok(nv) if close(z.re, *nv, 1e-6) => return Ok(v),
                        Ok(nv) if nv.abs() > 1e9 => return Err("La integral diverge".into()),
                        _ => {}
                    }
                }
            }
        }
    }
    let nv = numv?;
    Ok(Expr::Float(nv))
}

// ====================================================================== limits

#[derive(Clone, Debug)]
enum Lim {
    Fin(Expr),
    PInf,
    NInf,
    /// Infinite with unknown/ambiguous sign.
    Inf,
}

struct Limiter<'c, 's> {
    cas: &'c Cas<'s>,
    x: String,
    a: Expr,
    dir: i32,
    budget: Cell<usize>,
}

fn is_zero_val(cas: &Cas, e: &Expr) -> bool {
    e.is_zero() || (!e.has_free_symbols() && cas.num(e).map_or(false, |z| z.abs() == 0.0))
}

fn sign_of(cas: &Cas, e: &Expr) -> Option<f64> {
    let z = cas.num(e).ok()?;
    if !z.is_real() || z.re == 0.0 {
        return None;
    }
    Some(z.re.signum())
}

impl<'c, 's> Limiter<'c, 's> {
    fn inf_signed(&self, f: &Expr) -> Lim {
        match self.sign_near(f) {
            Some(s) if s > 0.0 => Lim::PInf,
            Some(_) => Lim::NInf,
            None => Lim::Inf,
        }
    }

    /// Sign of f near a (on the side(s) given by dir).
    fn sign_near(&self, f: &Expr) -> Option<f64> {
        let fc = self.cas.func1(f, &self.x).ok()?;
        let a = self.cas.num_real(&self.a).ok()?;
        let sides: Vec<f64> = if self.dir == 0 { vec![1.0, -1.0] } else { vec![self.dir as f64] };
        let mut found: Option<f64> = None;
        for s in sides {
            let mut sg: Option<f64> = None;
            for k in [3, 5, 7] {
                let h = 10f64.powi(-k) * a.abs().max(1.0);
                let v = fc(a + s * h);
                if v.is_finite() && v != 0.0 {
                    if sg.map_or(false, |g| g != v.signum()) {
                        return None;
                    }
                    sg = Some(v.signum());
                }
            }
            if let Some(g) = sg {
                if found.map_or(false, |f0| f0 != g) {
                    return None;
                }
                found = Some(g);
            }
        }
        found
    }

    fn lim(&self, f: &Expr, depth: usize) -> Option<Lim> {
        let cas = self.cas;
        let x = self.x.as_str();
        if depth > 10 || f.size() > 1500 {
            return None;
        }
        let b = self.budget.get();
        if b == 0 {
            return None;
        }
        self.budget.set(b - 1);
        if f.free_of(x) {
            if is_pos_inf(f) {
                return Some(Lim::PInf);
            }
            if is_neg_inf(f) {
                return Some(Lim::NInf);
            }
            return Some(Lim::Fin(f.clone()));
        }
        if matches!(f, Expr::Sym(_)) {
            return Some(Lim::Fin(self.a.clone()));
        }
        if !has_discontinuous(f) {
            if let Ok(v) = cas.eval(&subst(f, x, &self.a)) {
                if finite_val(cas, &v) {
                    return Some(Lim::Fin(v));
                }
            }
        }
        match f {
            Expr::Add(v) => {
                let mut fin = Vec::new();
                let (mut p, mut n, mut u) = (false, false, false);
                for t in v {
                    match self.lim(t, depth + 1)? {
                        Lim::Fin(e) => fin.push(e),
                        Lim::PInf => p = true,
                        Lim::NInf => n = true,
                        Lim::Inf => u = true,
                    }
                }
                if (p && n) || u {
                    return self.combine(v, depth);
                }
                if p {
                    return Some(Lim::PInf);
                }
                if n {
                    return Some(Lim::NInf);
                }
                Some(Lim::Fin(add(fin).ok()?))
            }
            Expr::Mul(v) => {
                let (nn, dd) = num_den(f);
                if !dd.is_one() && !dd.free_of(x) {
                    return self.quotient(&nn, &dd, f, depth);
                }
                let mut zeros = Vec::new();
                let mut infs = Vec::new();
                let mut fin = Vec::new();
                let mut fin_vals = Vec::new();
                let mut sgn = 1.0;
                let mut unsigned = false;
                let mut bounded = false;
                for t in v {
                    let lt = match self.lim(t, depth + 1) {
                        Some(l) => l,
                        None if is_bounded(t) => {
                            bounded = true;
                            continue;
                        }
                        None => return None,
                    };
                    match lt {
                        Lim::Fin(e) => {
                            if is_zero_val(cas, &e) {
                                zeros.push(t.clone());
                            } else {
                                fin.push(t.clone());
                                fin_vals.push(e);
                            }
                        }
                        Lim::PInf => infs.push(t.clone()),
                        Lim::NInf => {
                            sgn = -sgn;
                            infs.push(t.clone())
                        }
                        Lim::Inf => {
                            unsigned = true;
                            infs.push(t.clone())
                        }
                    }
                }
                if bounded {
                    return if !zeros.is_empty() && infs.is_empty() { Some(Lim::Fin(Expr::zero())) } else { None };
                }
                if !zeros.is_empty() && !infs.is_empty() {
                    let mut num = infs;
                    num.extend(fin);
                    let nume = mul(num).ok()?;
                    let dene = pow(mul(zeros).ok()?, int(-1)).ok()?;
                    return self.quotient(&nume, &dene, f, depth);
                }
                if !infs.is_empty() {
                    if unsigned {
                        return Some(self.inf_signed(f));
                    }
                    let fv = mul(fin_vals).ok()?;
                    let s = if fv.is_one() { 1.0 } else { sign_of(cas, &fv)? };
                    return Some(if s * sgn > 0.0 { Lim::PInf } else { Lim::NInf });
                }
                if !zeros.is_empty() {
                    return Some(Lim::Fin(Expr::zero()));
                }
                Some(Lim::Fin(mul(fin_vals).ok()?))
            }
            Expr::Pow(bb, p) => {
                let (bb, p) = (bb.as_ref(), p.as_ref());
                if p.free_of(x) {
                    if p.as_rat().map_or(false, |r| r.is_neg()) {
                        return self.quotient(&Expr::one(), &pow(bb.clone(), neg(p.clone()).ok()?).ok()?, f, depth);
                    }
                    let pv = cas.num_real(p).ok();
                    return match self.lim(bb, depth + 1)? {
                        Lim::Fin(l) => {
                            if is_zero_val(cas, &l) {
                                if pv.map_or(false, |v| v > 0.0) {
                                    return Some(Lim::Fin(Expr::zero()));
                                }
                                return None;
                            }
                            let r = cas.eval(&pow(l, p.clone()).ok()?).ok()?;
                            if finite_val(cas, &r) {
                                Some(Lim::Fin(r))
                            } else {
                                None
                            }
                        }
                        Lim::PInf => match pv? {
                            v if v > 0.0 => Some(Lim::PInf),
                            v if v < 0.0 => Some(Lim::Fin(Expr::zero())),
                            _ => None,
                        },
                        Lim::NInf => {
                            let k = p.as_int()?;
                            if k < 0 {
                                Some(Lim::Fin(Expr::zero()))
                            } else if k % 2 == 0 {
                                Some(Lim::PInf)
                            } else {
                                Some(Lim::NInf)
                            }
                        }
                        Lim::Inf => {
                            if pv? < 0.0 {
                                Some(Lim::Fin(Expr::zero()))
                            } else {
                                Some(self.inf_signed(f))
                            }
                        }
                    };
                }
                if bb.free_of(x) {
                    let bv = cas.num_real(bb).ok();
                    return match self.lim(p, depth + 1)? {
                        Lim::Fin(g) => {
                            let r = cas.eval(&pow(bb.clone(), g).ok()?).ok()?;
                            if finite_val(cas, &r) {
                                Some(Lim::Fin(r))
                            } else {
                                None
                            }
                        }
                        Lim::PInf => {
                            let bv = bv?;
                            if bv > 1.0 {
                                Some(Lim::PInf)
                            } else if bv >= 0.0 && bv < 1.0 {
                                Some(Lim::Fin(Expr::zero()))
                            } else {
                                None
                            }
                        }
                        Lim::NInf => {
                            let bv = bv?;
                            if bv > 1.0 {
                                Some(Lim::Fin(Expr::zero()))
                            } else if bv > 0.0 && bv < 1.0 {
                                Some(Lim::PInf)
                            } else {
                                None
                            }
                        }
                        Lim::Inf => None,
                    };
                }
                // u^v = e^(v ln u)
                let g = mul(vec![p.clone(), ln(cas, bb.clone()).ok()?]).ok()?;
                match self.lim(&g, depth + 1)? {
                    Lim::Fin(l) => Some(Lim::Fin(cas.eval(&pow(Expr::sym("e"), l).ok()?).ok()?)),
                    Lim::PInf => Some(Lim::PInf),
                    Lim::NInf => Some(Lim::Fin(Expr::zero())),
                    Lim::Inf => None,
                }
            }
            Expr::Call(name, args) => {
                if is_discontinuous_fn(name) {
                    return None;
                }
                if name == "log" && args.len() == 2 {
                    let r = div(ln(cas, args[0].clone()).ok()?, ln(cas, args[1].clone()).ok()?).ok()?;
                    return self.lim(&r, depth + 1);
                }
                if args.len() != 1 {
                    return None;
                }
                let deg = cas.s.deg;
                let half_pi = if deg { int(90) } else { mul(vec![half(), Expr::sym("pi")]).ok()? };
                match self.lim(&args[0], depth + 1)? {
                    Lim::Fin(u) => {
                        if let Ok(v) = cas.apply(name, vec![u.clone()]) {
                            if finite_val(cas, &v) {
                                return Some(Lim::Fin(v));
                            }
                        }
                        if matches!(name.as_str(), "ln" | "log" | "log2") && is_zero_val(cas, &u) {
                            return Some(Lim::NInf);
                        }
                        Some(self.inf_signed(f)).filter(|_| matches!(name.as_str(), "tan" | "cot" | "sec" | "csc" | "atanh"))
                    }
                    Lim::PInf => Some(match name.as_str() {
                        "ln" | "log" | "log2" | "sqrt" | "cbrt" | "sinh" | "cosh" | "asinh" | "acosh" | "exp" | "abs" | "gamma" | "fact" => Lim::PInf,
                        "atan" => Lim::Fin(half_pi),
                        "acot" | "erfc" => Lim::Fin(Expr::zero()),
                        "tanh" | "erf" => Lim::Fin(Expr::one()),
                        _ => return None,
                    }),
                    Lim::NInf => Some(match name.as_str() {
                        "sinh" | "asinh" | "cbrt" => Lim::NInf,
                        "cosh" | "abs" => Lim::PInf,
                        "atan" => Lim::Fin(neg(half_pi).ok()?),
                        "tanh" | "erf" => Lim::Fin(int(-1)),
                        "erfc" => Lim::Fin(int(2)),
                        "exp" => Lim::Fin(Expr::zero()),
                        _ => return None,
                    }),
                    Lim::Inf => match name.as_str() {
                        "abs" | "cosh" => Some(Lim::PInf),
                        _ => None,
                    },
                }
            }
            _ => None,
        }
    }

    /// ∞ − ∞: bring to a common denominator.
    fn combine(&self, v: &[Expr], depth: usize) -> Option<Lim> {
        if v.len() > 4 {
            return None;
        }
        let nds: Vec<(Expr, Expr)> = v.iter().map(num_den).collect();
        if nds.iter().all(|(_, d)| d.is_one()) {
            return None;
        }
        let mut terms = Vec::new();
        for i in 0..nds.len() {
            let mut fs = vec![nds[i].0.clone()];
            for (j, nd) in nds.iter().enumerate() {
                if j != i {
                    fs.push(nd.1.clone());
                }
            }
            terms.push(mul(fs).ok()?);
        }
        let n = tidy(add(terms).ok()?);
        let d = mul(nds.into_iter().map(|p| p.1).collect()).ok()?;
        let whole = div(n.clone(), d.clone()).ok()?;
        self.quotient(&n, &d, &whole, depth + 1)
    }

    fn quotient(&self, n: &Expr, d: &Expr, whole: &Expr, depth: usize) -> Option<Lim> {
        let cas = self.cas;
        let ln_ = self.lim(n, depth + 1)?;
        let ld = self.lim(d, depth + 1)?;
        let infish = |l: &Lim| matches!(l, Lim::PInf | Lim::NInf | Lim::Inf);
        match (&ln_, &ld) {
            (Lim::Fin(nv), Lim::Fin(dv)) => {
                let dz = is_zero_val(cas, dv);
                let nz = is_zero_val(cas, nv);
                if !dz {
                    return Some(Lim::Fin(cas.eval(&div(nv.clone(), dv.clone()).ok()?).ok()?));
                }
                if !nz {
                    return Some(self.inf_signed(whole));
                }
                self.lhopital(n, d, depth)
            }
            (Lim::Fin(_), l) if infish(l) => Some(Lim::Fin(Expr::zero())),
            (l, Lim::Fin(dv)) if infish(l) => {
                if is_zero_val(cas, dv) {
                    return Some(self.inf_signed(whole));
                }
                let sd = sign_of(cas, dv);
                match (l, sd) {
                    (Lim::PInf, Some(s)) => Some(if s > 0.0 { Lim::PInf } else { Lim::NInf }),
                    (Lim::NInf, Some(s)) => Some(if s > 0.0 { Lim::NInf } else { Lim::PInf }),
                    _ => Some(self.inf_signed(whole)),
                }
            }
            _ => self.lhopital(n, d, depth),
        }
    }

    fn lhopital(&self, n: &Expr, d: &Expr, depth: usize) -> Option<Lim> {
        if depth > 8 {
            return None;
        }
        let cas = self.cas;
        let dn = diff(cas, n, &self.x).ok()?;
        let dd = diff(cas, d, &self.x).ok()?;
        if dd.is_zero() {
            return None;
        }
        let g = div(dn, dd).ok()?;
        let g = merge_prop(&g, &self.x).unwrap_or(g);
        let g = tidy(g);
        self.lim(&g, depth + 2)
    }
}

/// Functions bounded for every real argument.
fn is_bounded(e: &Expr) -> bool {
    match e {
        Expr::Call(n, a) => a.len() == 1 && matches!(n.as_str(), "sin" | "cos" | "atan" | "tanh" | "erf" | "sign" | "frac" | "acot"),
        Expr::Pow(b, p) => p.as_int().map_or(false, |k| k > 0) && is_bounded(b),
        _ => false,
    }
}

/// Leading-term analysis of rational functions at ±∞.
fn rational_at_inf(cas: &Cas, f: &Expr, x: &str, s: i32) -> Option<Expr> {
    let (n, d) = num_den(f);
    let np = poly_coeffs(&n, x)?;
    let dp = poly_coeffs(&d, x)?;
    if dp.is_empty() {
        return None;
    }
    if np.is_empty() {
        return Some(Expr::zero());
    }
    let (dn, dd) = (np.len() - 1, dp.len() - 1);
    if dn == 0 && dd == 0 {
        return None;
    }
    let ratio = div(np.last()?.clone(), dp.last()?.clone()).ok()?;
    if dn < dd {
        return Some(Expr::zero());
    }
    if dn == dd {
        return cas.eval(&ratio).ok().map(tidy);
    }
    let sg = sign_of(cas, &ratio)? * if (dn - dd) % 2 == 1 { s as f64 } else { 1.0 };
    Some(if sg > 0.0 { inf() } else { neg_inf() })
}

enum NL {
    Val(f64, f64),
    PInf,
    NInf,
    Undef,
    NoConv,
}

/// Numeric one-sided limit of g at a (side s = ±1) with Richardson extrapolation.
fn num_side(g: &dyn Fn(f64) -> f64, a: f64, s: f64) -> NL {
    let h0 = 0.1 * a.abs().max(1.0);
    const K: usize = 14;
    let all: Vec<f64> = (0..=K).map(|k| g(a + s * h0 / 2f64.powi(k as i32))).collect();
    // longest run of finite values reaching closest to a (overflow → stop)
    let i0 = match all.iter().position(|v| v.is_finite()) {
        Some(i) => i,
        None => {
            return match all[K] {
                v if v == f64::INFINITY => NL::PInf,
                v if v == f64::NEG_INFINITY => NL::NInf,
                _ => NL::Undef,
            }
        }
    };
    let i1 = all[i0..].iter().position(|v| !v.is_finite()).map_or(K + 1, |p| i0 + p);
    let vals = &all[i0..i1];
    let n = vals.len();
    let last = vals[n - 1];
    let ended_inf = i1 <= K && all[i1].is_infinite();
    if n >= 3 {
        let w = &vals[n.saturating_sub(6)..];
        let growing = w.windows(2).all(|p| p[1].abs() > p[0].abs() * 1.3 && p[1].signum() == p[0].signum());
        if growing && (last.abs() > 1e4 || (ended_inf && all[i1].signum() == last.signum())) {
            return if last > 0.0 { NL::PInf } else { NL::NInf };
        }
    }
    if i1 <= K && all[i1..].iter().all(|v| v.is_nan()) && n < 4 {
        return NL::Undef;
    }
    if n < 4 {
        return NL::NoConv;
    }
    // Richardson table; an estimate is accepted when two consecutive differences are small
    let mut best = f64::NAN;
    let mut best_d = f64::INFINITY;
    let mut prev: Vec<f64> = Vec::new();
    let mut prev_d: Vec<f64> = Vec::new();
    for k in 0..n {
        let mut row = vec![vals[k]];
        for m in 1..=k.min(5) {
            let f = 2f64.powi(m as i32) - 1.0;
            let r = row[m - 1] + (row[m - 1] - prev[m - 1]) / f;
            row.push(r);
        }
        let mut dl = vec![f64::INFINITY; row.len()];
        if k > 0 {
            for m in 0..row.len().min(prev.len()) {
                let dlt = (row[m] - prev[m]).abs();
                dl[m] = dlt;
                let e2 = dlt.max(*prev_d.get(m).unwrap_or(&f64::INFINITY));
                if e2.is_finite() && e2 < best_d {
                    best_d = e2;
                    best = row[m];
                }
            }
        }
        prev = row;
        prev_d = dl;
    }
    if best.is_finite() && best_d <= 1e-7 * best.abs().max(1.0) {
        NL::Val(best, best_d)
    } else {
        NL::NoConv
    }
}

const ERR_LIM_SIDES: &str = "El límite no existe (los límites laterales difieren)";

fn numeric_limit(cas: &Cas, g: &Expr, x: &str, a: &Expr, dir: i32) -> R {
    let mut syms = Vec::new();
    g.symbols(&mut syms);
    if syms.iter().any(|s| s != x) {
        return Err("No se pudo calcular el límite simbólicamente".into());
    }
    let fc = cas.func1(g, x)?;
    let af = cas.num_real(a)?;
    let res = |s: f64| num_side(&fc, af, s);
    let r = if dir != 0 {
        res(dir as f64)
    } else {
        match (res(1.0), res(-1.0)) {
            (NL::Undef, o) | (o, NL::Undef) => o,
            (NL::Val(p, e1), NL::Val(q, e2)) => {
                if close(p, q, 1e-6) {
                    NL::Val((p + q) / 2.0, e1.max(e2).max((p - q).abs()))
                } else {
                    return Err(ERR_LIM_SIDES.into());
                }
            }
            (NL::PInf, NL::PInf) => NL::PInf,
            (NL::NInf, NL::NInf) => NL::NInf,
            (NL::NoConv, _) | (_, NL::NoConv) => NL::NoConv,
            _ => return Err(ERR_LIM_SIDES.into()),
        }
    };
    match r {
        NL::Val(v, err) => {
            let tol = (err * 10.0 / v.abs().max(1.0)).max(1e-11);
            Ok(nice(v, tol).unwrap_or(Expr::Float(v)))
        }
        NL::PInf => Ok(inf()),
        NL::NInf => Ok(neg_inf()),
        NL::Undef => Err("La función no está definida cerca de ese punto".into()),
        NL::NoConv => Err("El límite no existe o no se pudo determinar".into()),
    }
}

/// Core limit computation (a may be ±∞; dir ∈ {-1, 0, 1}).
fn limit_core(cas: &Cas, f: &Expr, x: &str, a: &Expr, dir: i32) -> R {
    if let Expr::List(v) = f {
        return Ok(Expr::List(v.iter().map(|t| limit_core(cas, t, x, a, dir)).collect::<Result<_, _>>()?));
    }
    if f.free_of(x) {
        return Ok(f.clone());
    }
    let (g, xv, av, dv) = if is_pos_inf(a) || is_neg_inf(a) {
        let s = if is_pos_inf(a) { 1 } else { -1 };
        if let Some(r) = rational_at_inf(cas, f, x, s) {
            return Ok(r);
        }
        let t = fresh("t");
        let repl = mul(vec![int(s as i128), pow(Expr::sym(&t), int(-1))?])?;
        (cas.eval(&subst(f, x, &repl))?, t, Expr::zero(), 1)
    } else {
        (f.clone(), x.to_string(), a.clone(), dir)
    };
    let lm = Limiter { cas, x: xv.clone(), a: av.clone(), dir: dv, budget: Cell::new(1500) };
    match lm.lim(&g, 0) {
        Some(Lim::Fin(v)) => return Ok(tidy(cas.eval(&v)?)),
        Some(Lim::PInf) => return Ok(inf()),
        Some(Lim::NInf) => return Ok(neg_inf()),
        Some(Lim::Inf) if dv == 0 => {
            let side = |d: i32| Limiter { cas, x: xv.clone(), a: av.clone(), dir: d, budget: Cell::new(0) }.sign_near(&g);
            if let (Some(p), Some(q)) = (side(1), side(-1)) {
                if p != q {
                    return Err(ERR_LIM_SIDES.into());
                }
                return Ok(if p > 0.0 { inf() } else { neg_inf() });
            }
        }
        _ => {}
    }
    numeric_limit(cas, &g, &xv, &av, dv)
}

fn parse_dir(e: &Expr) -> Result<i32, String> {
    match e {
        Expr::Num(_) | Expr::Float(_) => {
            let v = e.as_f64().unwrap();
            Ok(if v > 0.0 {
                1
            } else if v < 0.0 {
                -1
            } else {
                0
            })
        }
        Expr::Text(t) | Expr::Sym(t) => match t.trim() {
            "+" | "right" | "der" | "derecha" => Ok(1),
            "-" | "left" | "izq" | "izquierda" => Ok(-1),
            _ => Err("limit: la dirección debe ser 1 (por la derecha) o -1 (por la izquierda)".into()),
        },
        _ => Err("limit: la dirección debe ser 1 (por la derecha) o -1 (por la izquierda)".into()),
    }
}

/// `limit(f, x, a)` and `limit(f, x, a, dir)`.
pub fn cmd_limit(cas: &Cas, args: Vec<Expr>) -> R {
    match args.len() {
        2 => {
            let x = auto_var(&args[0])?;
            limit_core(cas, &args[0], &x, &args[1], 0)
        }
        3 | 4 => {
            let x = var_arg(&args[1], "limit")?;
            let dir = if args.len() == 4 { parse_dir(&args[3])? } else { 0 };
            limit_core(cas, &args[0], &x, &args[2], dir)
        }
        _ => Err("Uso: limit(f, x, a) o limit(f, x, a, dir)".into()),
    }
}

// ====================================================================== Taylor

/// `taylor(f, x, a, n)`: Σ f⁽ᵏ⁾(a)/k! (x−a)ᵏ (defaults a = 0, n = 5).
pub fn cmd_taylor(cas: &Cas, args: Vec<Expr>) -> R {
    if args.is_empty() || args.len() > 4 {
        return Err("Uso: taylor(f, x, a, n)".into());
    }
    let f = &args[0];
    let x = match args.get(1) {
        Some(v) => var_arg(v, "taylor")?,
        None => auto_var(f)?,
    };
    let a = args.get(2).cloned().unwrap_or(Expr::zero());
    let n = match args.get(3) {
        Some(v) => v.as_int().ok_or("taylor: el orden debe ser un entero")?,
        None => 5,
    };
    if !(0..=40).contains(&n) {
        return Err("taylor: el orden debe estar entre 0 y 40".into());
    }
    let xs = Expr::sym(&x);
    let xa = sub(xs, a.clone())?;
    let mut terms = Vec::new();
    let mut dk = f.clone();
    let mut fact = Rat::ONE;
    for k in 0..=n {
        if k > 0 {
            dk = tidy(diff(cas, &dk, &x)?);
            fact = fact.mul(Rat::int(k)).ok_or("desbordamiento")?;
        }
        let mut ck = cas.eval(&subst(&dk, &x, &a)).ok().filter(|v| finite_val(cas, v));
        if ck.is_none() {
            let l = limit_core(cas, &dk, &x, &a, 0).map_err(|_| "La función no es analítica en ese punto".to_string())?;
            if is_pos_inf(&l) || is_neg_inf(&l) {
                return Err("La función no es analítica en ese punto".into());
            }
            ck = Some(l);
        }
        let ck = ck.unwrap();
        if !ck.is_zero() {
            terms.push(mul(vec![ck, rat(fact.recip().ok_or("desbordamiento")?), pow(xa.clone(), int(k))?])?);
        }
    }
    add(terms)
}

// ====================================================================== sums & products

fn binom(n: i128, k: i128) -> Option<i128> {
    let mut r: i128 = 1;
    for i in 0..k {
        r = r.checked_mul(n - i)? / (i + 1);
    }
    Some(r)
}

/// Bernoulli numbers B₀..Bₙ with B₁ = +1/2.
fn bernoulli(n: usize) -> Option<Vec<Rat>> {
    let mut b = vec![Rat::ONE];
    for m in 1..=n {
        let mut s = Rat::ZERO;
        for (j, bj) in b.iter().enumerate() {
            s = s.add(Rat::int(binom(m as i128 + 1, j as i128)?).mul(*bj)?)?;
        }
        b.push(s.neg()?.div(Rat::int(m as i128 + 1))?);
    }
    if n >= 1 {
        b[1] = Rat::new(1, 2)?;
    }
    Some(b)
}

/// Coefficients of S_p(n) = Σ_{k=1}^{n} k^p (low → high).
fn faulhaber(p: usize) -> Option<P> {
    let b = bernoulli(p)?;
    let mut c = vec![Rat::ZERO; p + 2];
    let inv = Rat::new(1, p as i128 + 1)?;
    for (j, bj) in b.iter().enumerate().take(p + 1) {
        let co = Rat::int(binom(p as i128 + 1, j as i128)?).mul(*bj)?.mul(inv)?;
        c[p + 1 - j] = c[p + 1 - j].add(co)?;
    }
    Some(c)
}

/// Presents a polynomial in a symbol in factored form when that is smaller.
fn present_poly(e: Expr, var: &Expr) -> Expr {
    let e = tidy(e);
    if let Expr::Sym(s) = var {
        if let Some(p) = rat_poly(&e, s) {
            if let Some(fx) = factor_rat_poly(&p, var) {
                if fx.size() < e.size() {
                    return fx;
                }
            }
        }
    }
    e
}

/// Closed forms: constant, polynomial (Faulhaber) and geometric summands.
fn sum_closed(cas: &Cas, f: &Expr, k: &str, a: &Expr, b: &Expr) -> Option<R> {
    let count = add(vec![sub(b.clone(), a.clone()).ok()?, int(1)]).ok()?;
    if f.free_of(k) {
        return Some(mul(vec![f.clone(), count]).map(tidy));
    }
    if let Some(c) = poly_coeffs(f, k) {
        if c.len() <= 22 {
            let am1 = sub(a.clone(), int(1)).ok()?;
            let mut terms = Vec::new();
            for (p, cp) in c.iter().enumerate() {
                if cp.is_zero() {
                    continue;
                }
                let s = faulhaber(p)?;
                let diffp = sub(p_expr(&s, b).ok()?, p_expr(&s, &am1).ok()?).ok()?;
                terms.push(mul(vec![cp.clone(), diffp]).ok()?);
            }
            let r = expand(&add(terms).ok()?).ok()?;
            return Some(Ok(present_poly(r, b)));
        }
    }
    // geometric
    let factors = match f {
        Expr::Mul(v) => v.clone(),
        o => vec![o.clone()],
    };
    let mut q = Expr::one();
    for fac in &factors {
        if fac.free_of(k) {
            continue;
        }
        match fac {
            Expr::Pow(r, g) if r.free_of(k) => {
                let (al, _) = linear(g, k)?;
                q = mul(vec![q, pow((**r).clone(), al).ok()?]).ok()?;
            }
            _ => return None,
        }
    }
    if q.is_one() {
        return None;
    }
    let ta = cas.eval(&subst(f, k, a)).ok()?;
    Some(
        mul(vec![ta, sub(pow(q.clone(), count).ok()?, int(1)).ok()?, pow(sub(q, int(1)).ok()?, int(-1)).ok()?])
            .and_then(|r| cas.eval(&r)),
    )
}

/// Accelerated numeric value of an infinite series (or product).
fn num_series(term: &dyn Fn(f64) -> f64, a: i128, is_prod: bool) -> Result<(f64, f64), String> {
    let mut acc: f64 = if is_prod { 1.0 } else { 0.0 };
    let mut comp = 0.0f64;
    let mut ests: Vec<f64> = Vec::new();
    let mut n_done: usize = 0;
    let mut last_term = 0.0f64;
    let mut tiny = 0usize;
    let div_err = || if is_prod { "El producto no converge".to_string() } else { "La serie diverge".to_string() };
    for j in 0..=14 {
        let target = 16usize << j;
        let mut at_target = 0.0;
        while n_done <= target {
            let t = term((a + n_done as i128) as f64);
            let small = if is_prod { (t - 1.0).abs() } else { t.abs() };
            if tiny >= 30 && (!t.is_finite() || small < 1e-17 * acc.abs()) {
                return Ok((acc, 0.0));
            }
            if !t.is_finite() {
                return Err(div_err());
            }
            if small < 1e-17 * acc.abs() {
                tiny += 1;
            } else {
                tiny = 0;
            }
            last_term = t;
            if is_prod {
                acc *= t;
            } else {
                let y = t - comp;
                let s = acc + y;
                comp = (s - acc) - y;
                acc = s;
            }
            if n_done == target - 1 {
                at_target = acc;
            }
            n_done += 1;
        }
        if !acc.is_finite() {
            return Err(div_err());
        }
        ests.push((at_target + acc) / 2.0);
    }
    let lt = if is_prod { (last_term - 1.0).abs() } else { last_term.abs() };
    let scale = ests.last().unwrap().abs().max(1.0);
    if lt > 1e-4 * scale {
        return Err(div_err());
    }
    let nn = ests.len();
    let d1 = (ests[nn - 1] - ests[nn - 2]).abs();
    let d0 = (ests[nn - 2] - ests[nn - 3]).abs();
    if d1 > 1e-6 * scale && d1 > 0.7 * d0 {
        return Err(div_err());
    }
    // Richardson in 1/N (ratio 2)
    let mut best = *ests.last().unwrap();
    let mut best_d = d1;
    let mut prev: Vec<f64> = Vec::new();
    for (k, v) in ests.iter().enumerate() {
        let mut row = vec![*v];
        for m in 1..=k.min(5) {
            let f = 2f64.powi(m as i32) - 1.0;
            row.push(row[m - 1] + (row[m - 1] - prev[m - 1]) / f);
        }
        if k > 0 {
            for m in 0..row.len().min(prev.len()) {
                let dl = (row[m] - prev[m]).abs();
                if dl.is_finite() && dl < best_d {
                    best_d = dl;
                    best = row[m];
                }
            }
        }
        prev = row;
    }
    if best_d > 1e-5 * scale {
        return Err(if is_prod { "El producto converge demasiado lento".to_string() } else { "La serie converge demasiado lento".to_string() });
    }
    Ok((best, best_d))
}

/// `sum(list)`, `sum(f, k, a, b)`, `prod(list)`, `prod(f, k, a, b)`.
pub fn cmd_sum(cas: &Cas, name: &str, args: Vec<Expr>) -> R {
    let is_prod = name == "prod";
    if args.len() == 1 {
        let v = match &args[0] {
            Expr::List(v) => v.clone(),
            Expr::Matrix(m) => m.iter().flatten().cloned().collect(),
            _ => return Err(format!("{name}: se esperaba una lista o {name}(f, k, a, b)")),
        };
        return if is_prod { mul(v) } else { add(v) };
    }
    if args.len() != 4 {
        return Err(format!("Uso: {name}(f, k, a, b) o {name}(lista)"));
    }
    let f = &args[0];
    let k = var_arg(&args[1], name)?;
    let (a, b) = (&args[2], &args[3]);
    let ai = a.as_int();
    let bi = b.as_int();
    // exact loop
    if let (Some(ai), Some(bi)) = (ai, bi) {
        if bi < ai {
            return Ok(if is_prod { Expr::one() } else { Expr::zero() });
        }
        if bi - ai < 2000 {
            let mut terms = Vec::with_capacity((bi - ai + 1) as usize);
            for i in ai..=bi {
                terms.push(cas.eval(&subst(f, &k, &int(i)))?);
            }
            return if is_prod { mul(terms) } else { add(terms) };
        }
    }
    let binf = is_pos_inf(b);
    if !is_prod && !binf {
        if let Some(r) = sum_closed(cas, f, &k, a, b) {
            return r;
        }
    }
    if is_prod && !binf {
        if f.free_of(&k) {
            return pow(f.clone(), add(vec![sub(b.clone(), a.clone())?, int(1)])?);
        }
        if *f == Expr::sym(&k) {
            if let Some(ai) = ai {
                if ai >= 1 {
                    return div(cas.apply("fact", vec![b.clone()])?, cas.apply("fact", vec![int(ai - 1)])?);
                }
            }
        }
    }
    if binf {
        let ai = ai.ok_or_else(|| format!("{name}: el límite inferior debe ser un entero"))?;
        if !is_prod {
            // exact geometric series
            if let Some(r) = sum_closed_geometric_inf(cas, f, &k, a) {
                return r;
            }
            if poly_coeffs(f, &k).is_some() {
                return Err("La serie diverge".into());
            }
        }
        let mut syms = Vec::new();
        f.symbols(&mut syms);
        if syms.iter().any(|s| *s != k) {
            return Err(format!("{name}: no se encontró una forma cerrada"));
        }
        let fc = cas.func1(f, &k)?;
        let (v, err) = num_series(&fc, ai, is_prod)?;
        let tol = (err * 10.0 / v.abs().max(1.0)).max(1e-11);
        return Ok(nice(v, tol).unwrap_or(Expr::Float(v)));
    }
    // large numeric bounds without closed form
    if let (Some(ai), Some(bi)) = (ai, bi) {
        if bi - ai <= 10_000_000 {
            let fc = cas.func1(f, &k)?;
            let mut acc = if is_prod { 1.0 } else { 0.0 };
            for i in ai..=bi {
                let t = fc(i as f64);
                if is_prod {
                    acc *= t
                } else {
                    acc += t
                }
            }
            return Ok(Expr::Float(acc));
        }
        return Err(format!("{name}: demasiados términos"));
    }
    Err(format!("{name}: no se encontró una forma cerrada"))
}

/// Σ_{k=a}^{∞} of a geometric summand: Some(Ok) value, Some(Err) divergence, None not geometric.
fn sum_closed_geometric_inf(cas: &Cas, f: &Expr, k: &str, a: &Expr) -> Option<R> {
    let factors = match f {
        Expr::Mul(v) => v.clone(),
        o => vec![o.clone()],
    };
    let mut q = Expr::one();
    for fac in &factors {
        if fac.free_of(k) {
            continue;
        }
        match fac {
            Expr::Pow(r, g) if r.free_of(k) => {
                let (al, _) = linear(g, k)?;
                q = mul(vec![q, pow((**r).clone(), al).ok()?]).ok()?;
            }
            _ => return None,
        }
    }
    if q.is_one() {
        return None;
    }
    if !q.has_free_symbols() {
        let z = cas.num(&q).ok()?;
        if z.abs() >= 1.0 {
            return Some(Err("La serie diverge".into()));
        }
    }
    let ta = cas.eval(&subst(f, k, a)).ok()?;
    Some(div(ta, sub(int(1), q).ok()?).and_then(|r| cas.eval(&r)).map(tidy))
}

/// `seq(expr, k, a, b[, step])` → list.
pub fn cmd_seq(cas: &Cas, args: Vec<Expr>) -> R {
    if args.len() < 4 || args.len() > 5 {
        return Err("Uso: seq(expr, k, a, b[, paso])".into());
    }
    let f = &args[0];
    let k = var_arg(&args[1], "seq")?;
    let step = args.get(4).cloned().unwrap_or(Expr::one());
    let (a, b) = (&args[2], &args[3]);
    let af = cas.num_real(a)?;
    let bf = cas.num_real(b)?;
    let sf = cas.num_real(&step)?;
    if sf == 0.0 {
        return Err("seq: el paso no puede ser cero".into());
    }
    let cnt = ((bf - af) / sf + 1e-9).floor();
    if cnt < 0.0 {
        return Ok(Expr::List(vec![]));
    }
    if cnt > 10000.0 {
        return Err("seq: demasiados elementos (máximo 10000)".into());
    }
    let mut out = Vec::new();
    for i in 0..=(cnt as i128) {
        let v = add(vec![a.clone(), mul(vec![int(i), step.clone()])?])?;
        out.push(cas.eval(&subst(f, &k, &v))?);
    }
    Ok(Expr::List(out))
}

// ====================================================================== applications

/// Parses `(f, x, a)` or `(f, a)`; `y = g` uses g.
fn fxa(args: &[Expr], cmd: &str) -> Result<(Expr, String, Expr), String> {
    let f = match &args[0] {
        Expr::Eq(l, r) if l.sym_name() == Some("y") => (**r).clone(),
        o => o.clone(),
    };
    match args.len() {
        2 => Ok((f.clone(), auto_var(&f)?, args[1].clone())),
        3 => Ok((f, var_arg(&args[1], cmd)?, args[2].clone())),
        _ => Err(format!("Uso: {cmd}(f, x, a)")),
    }
}

fn value_at(cas: &Cas, f: &Expr, x: &str, a: &Expr) -> R {
    if let Ok(v) = cas.eval(&subst(f, x, a)) {
        if finite_val(cas, &v) {
            return Ok(v);
        }
    }
    let l = limit_core(cas, f, x, a, 0)?;
    if is_pos_inf(&l) || is_neg_inf(&l) {
        return Err("La función no está definida en ese punto".into());
    }
    Ok(l)
}

/// `tangent(f, x, a)` → y = m(x − a) + f(a); `normal(...)` → normal line.
pub fn cmd_tangent(cas: &Cas, name: &str, args: Vec<Expr>) -> R {
    let (f, x, a) = fxa(&args, name)?;
    let y0 = value_at(cas, &f, &x, &a)?;
    let m = value_at(cas, &diff(cas, &f, &x)?, &x, &a)?;
    let xs = Expr::sym(&x);
    let y = Expr::sym("y");
    let slope = if name == "normal" {
        if is_zero_val(cas, &m) {
            return Ok(Expr::Eq(Box::new(xs), Box::new(a)));
        }
        neg(pow(m, int(-1))?)?
    } else {
        m
    };
    let rhs = expand(&add(vec![mul(vec![slope, sub(xs, a)?])?, y0])?)?;
    Ok(Expr::Eq(Box::new(y), Box::new(rhs)))
}

/// `fmin(f, x, a, b)` / `fmax(...)` → {x = x₀, y = f(x₀)}.
pub fn cmd_fminmax(cas: &Cas, name: &str, args: Vec<Expr>) -> R {
    let is_max = name == "fmax";
    let (f, x, lo, hi) = match args.len() {
        1 => (args[0].clone(), auto_var(&args[0])?, int(-10), int(10)),
        2 => (args[0].clone(), var_arg(&args[1], name)?, int(-10), int(10)),
        3 => (args[0].clone(), auto_var(&args[0])?, args[1].clone(), args[2].clone()),
        4 => (args[0].clone(), var_arg(&args[1], name)?, args[2].clone(), args[3].clone()),
        _ => return Err(format!("Uso: {name}(f, x, a, b)")),
    };
    let (a, b) = (cas.num_real(&lo)?, cas.num_real(&hi)?);
    let (a, b) = (a.min(b), a.max(b));
    let fc = cas.func1(&f, &x)?;
    let sgn = if is_max { -1.0 } else { 1.0 };
    let mut cands: Vec<(f64, f64)> = Vec::new();
    for (xv, yv, _) in numeric::find_extrema(&mut |t| fc(t), a, b, 600) {
        cands.push((xv, yv));
    }
    for xv in [a, b] {
        cands.push((xv, fc(xv)));
    }
    let (mut x0, mut y0) = cands
        .into_iter()
        .filter(|c| c.1.is_finite())
        .min_by(|p, q| (sgn * p.1).total_cmp(&(sgn * q.1)))
        .ok_or_else(|| format!("{name}: la función no es evaluable en el intervalo"))?;
    // refine with Newton on f'
    let df = diff(cas, &f, &x).ok();
    if x0 > a && x0 < b {
        if let Some(dfc) = df.as_ref().and_then(|d| cas.func1(d, &x).ok()) {
            if let Some(r) = numeric::newton(&mut |t| dfc(t), x0) {
                if r >= a && r <= b && (r - x0).abs() < 1e-3 * (1.0 + x0.abs()) {
                    let yr = fc(r);
                    if yr.is_finite() && sgn * yr <= sgn * y0 + 1e-12 * (1.0 + y0.abs()) {
                        x0 = r;
                        y0 = yr;
                    }
                }
            }
        }
    }
    let xs = Expr::sym(&x);
    let mut xe = Expr::Float(x0);
    if let Some(r) = Rat::approximate(x0, 1000, 1e-9 * x0.abs().max(1.0)) {
        let cand = Expr::Num(r);
        let ok = if x0 <= a || x0 >= b {
            true
        } else {
            df.as_ref()
                .and_then(|d| cas.eval(&subst(d, &x, &cand)).ok())
                .and_then(|v| cas.num(&v).ok())
                .map_or(false, |z| z.abs() < 1e-9)
        };
        if ok {
            xe = cand;
        }
    } else if let Some(r) = nice(x0, 1e-9) {
        let ok = df
            .as_ref()
            .and_then(|d| cas.eval(&subst(d, &x, &r)).ok())
            .and_then(|v| cas.num(&v).ok())
            .map_or(false, |z| z.abs() < 1e-9);
        if ok {
            xe = r;
        }
    }
    let ye = if matches!(xe, Expr::Float(_)) {
        Expr::Float(y0)
    } else {
        cas.eval(&subst(&f, &x, &xe)).unwrap_or(Expr::Float(y0))
    };
    Ok(Expr::List(vec![Expr::Eq(Box::new(xs), Box::new(xe)), Expr::Eq(Box::new(Expr::sym("y")), Box::new(ye))]))
}

/// `slope(f, x, a)` = f'(a); `slope(f, a)` with a single variable.
pub fn cmd_slope_at(cas: &Cas, args: Vec<Expr>) -> R {
    if args.len() != 2 && args.len() != 3 {
        return Err("Uso: slope(f, x, a)".into());
    }
    let (f, x, a) = fxa(&args, "slope")?;
    let df = diff(cas, &f, &x)?;
    Ok(tidy(value_at(cas, &df, &x, &a)?))
}

// ====================================================================== tests

#[cfg(test)]
mod tests {
    use crate::tests::{ap, ev};
    use crate::Engine;

    fn num(s: &str) -> f64 {
        s.replace('−', "-").parse::<f64>().unwrap_or_else(|_| panic!("no numérico: {s}"))
    }

    #[test]
    fn derivatives() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "diff(x^3, x)"), "3x^2");
        assert_eq!(ev(&mut e, "diff(sin(x)^2, x)"), "2cos(x)·sin(x)");
        assert_eq!(ev(&mut e, "diff(x^3 + 2x, x)"), "3x^2 + 2");
        assert_eq!(ev(&mut e, "diff(x^4, x, 2)"), "12x^2");
        assert_eq!(ev(&mut e, "diff(e^(2x))"), "2·e^(2x)");
        assert_eq!(ev(&mut e, "diff(ln(x), x)"), "1/x");
        assert_eq!(ev(&mut e, "diff(sin(x), x, 4)"), "sin(x)");
        assert_eq!(ev(&mut e, "diff(x^x, x)"), ev(&mut e, "x^x*(ln(x)+1)"));
        assert_eq!(ev(&mut e, "diff(abs(x), x)"), "sign(x)");
        assert_eq!(ev(&mut e, "diff(atan(x), x)"), "1/(x^2 + 1)");
        assert_eq!(ev(&mut e, "diff(a*x^2, x)"), "2a·x");
        assert_eq!(ev(&mut e, "diff(sqrt(x), x)"), ev(&mut e, "1/(2sqrt(x))"));
        assert_eq!(ev(&mut e, "diff({x^2, x^3}, x)"), "{2x, 3x^2}");
        assert_eq!(ev(&mut e, "diff(2^x, x)"), ev(&mut e, "2^x*ln(2)"));
        assert_eq!(ev(&mut e, "diff(log(x,2), x)"), ev(&mut e, "1/(x*ln(2))"));
    }

    #[test]
    fn derivative_numeric_checks() {
        let mut e = Engine::new();
        // d/dx at a point, compared with central differences
        let cases = ["tan(x)", "sec(x)", "acos(x)", "asinh(x)", "atanh(x/2)", "erf(x)", "x^2*e^(-x)*sin(3x)", "ln(x^2+1)/x", "cot(x)", "acot(x)", "tanh(x)", "cosh(x)^3"];
        for c in cases {
            let d = ap(&mut e, &format!("slope({c}, x, 0.7)"));
            let dv = num(&d);
            let f = e.compile_src(c, &["x"]).unwrap();
            let h = 1e-5;
            let fd = (f.eval1(0.7 + h) - f.eval1(0.7 - h)) / (2.0 * h);
            assert!((dv - fd).abs() < 1e-6 * (1.0 + fd.abs()), "{c}: {dv} vs {fd}");
        }
    }

    #[test]
    fn degree_mode() {
        let mut e = Engine::new();
        e.settings.deg = true;
        assert_eq!(ev(&mut e, "diff(sin(x), x)"), ev(&mut e, "pi/180*cos(x)"));
        assert_eq!(ev(&mut e, "integrate(sin(x), x, 0, 180)"), ev(&mut e, "360/pi"));
        assert_eq!(ev(&mut e, "integrate(1/(1+x^2), x, 0, 1)"), ev(&mut e, "pi/4"));
        assert_eq!(ev(&mut e, "limit(sin(x)/x, x, 0)"), ev(&mut e, "pi/180"));
    }

    #[test]
    fn more_calculus() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "diff((x+1)/(x-1), x)"), "-2/(x - 1)^2");
        assert_eq!(ev(&mut e, "integrate(sin(x)^7*cos(x)^4, x)"), "cos(x)^11/11 - cos(x)^9/3 + 3cos(x)^7/7 - cos(x)^5/5");
        assert_eq!(ev(&mut e, "integrate(x*sqrt(x+1), x)"), "-2·(x + 1)^(3/2)/3 + 2·(x + 1)^(5/2)/5");
        assert!(!ev(&mut e, "integrate(x^10*e^x, x)").starts_with("ERR"));
        assert_eq!(ev(&mut e, "integrate(1/(1+x^3), x, 0, 1)"), ev(&mut e, "ln(2)/3 + pi*sqrt(3)/9"));
        assert!(ev(&mut e, "integrate(1/x^2, x, -1, 1)").contains("diverge"));
        assert!(ev(&mut e, "integrate(tan(x), x, 0, 2)").contains("diverge"));
        assert_eq!(ev(&mut e, "integrate(x^(-1/3), x, -1, 1)"), "0");
        assert_eq!(ev(&mut e, "limit((x^2-a^2)/(x-a), x, a)"), "2a");
        assert_eq!(ev(&mut e, "limit(x*sin(1/x), x, 0)"), "0");
        assert_eq!(ev(&mut e, "limit((3^x+2^x)^(1/x), x, inf)"), "3");
        assert_eq!(ev(&mut e, "limit(e^(1/x), x, 0, -1)"), "0");
        assert_eq!(ev(&mut e, "limit(e^(1/x), x, 0, 1)"), "∞");
        assert_eq!(ev(&mut e, "limit(x/e^x, x, -inf)"), "-∞");
        assert_eq!(ev(&mut e, "sum(1/(4k^2-1), k, 1, inf)"), "1/2");
        assert_eq!(ev(&mut e, "sum(2k+1, k, 0, n-1)"), "n^2");
        assert_eq!(ev(&mut e, "prod(k/(k+1), k, 1, 99)"), "1/100");
        assert_eq!(ev(&mut e, "prod(k, k, 1, n)"), "n!");
        let v = ap(&mut e, "sum(sin(k)/k^2, k, 1, inf)");
        assert!((num(&v) - 1.0139591323607685).abs() < 1e-8, "{v}");
        assert_eq!(ev(&mut e, "tangent(y=x^2, x, 3)"), "y = 6x - 9");
        assert_eq!(ev(&mut e, "normal(x^2, x, 0)"), "x = 0");
        assert_eq!(ev(&mut e, "integrate(k*x, x, 0, 2)"), "2k");
    }

    #[test]
    fn indefinite_integrals() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "integrate(x^2, x)"), "x^3/3");
        assert_eq!(ev(&mut e, "integrate(1/x, x)"), "ln(|x|)");
        assert_eq!(ev(&mut e, "integrate(cos(x), x)"), "sin(x)");
        assert_eq!(ev(&mut e, "integrate(e^(3x), x)"), "e^(3x)/3");
        assert_eq!(ev(&mut e, "integrate(1/(1+x^2), x)"), "atan(x)");
        assert_eq!(ev(&mut e, "integrate(1/sqrt(1-x^2), x)"), "asin(x)");
        assert_eq!(ev(&mut e, "integrate(x*e^x, x)"), ev(&mut e, "x*e^x - e^x"));
        assert_eq!(ev(&mut e, "integrate(ln(x), x)"), ev(&mut e, "x*ln(x) - x"));
        assert_eq!(ev(&mut e, "integrate(2x*cos(x^2), x)"), "sin(x^2)");
        assert_eq!(ev(&mut e, "integrate(x/(x^2+1), x)"), ev(&mut e, "ln(x^2+1)/2"));
        assert_eq!(ev(&mut e, "integrate(sec(x)^2, x)"), "tan(x)");
        let r = ev(&mut e, "integrate(1/(x^2-1), x)");
        assert!(r.contains("ln"), "{r}");
    }

    /// Verifies indefinite integrals by differentiating them numerically.
    #[test]
    fn indefinite_integrals_verified() {
        let mut e = Engine::new();
        let cases = [
            "x^2*sin(x)",
            "x^3*e^(2x)",
            "sin(x)^2",
            "cos(x)^3",
            "tan(x)",
            "x*ln(x)",
            "ln(x)^2",
            "x*atan(x)",
            "e^x*sin(x)",
            "sin(3x)*cos(2x)",
            "1/(x^2+2x+5)",
            "(x+1)/(x^2-5x+6)",
            "1/(x^3+x)",
            "x^3/(x^2+1)",
            "1/(x^2+1)^2",
            "1/(x^4-1)",
            "sqrt(1-x^2)",
            "sqrt(x^2+1)",
            "1/sqrt(x^2+4)",
            "e^x/(1+e^x)",
            "sin(x)^3*cos(x)",
            "(2x+1)^5",
            "x*(x+1)^4",
            "e^(sqrt(x))/sqrt(x)",
            "asin(x)",
            "atan(2x+1)",
            "1/(x*ln(x))",
            "x^2*e^(-x^2)*x",
            "sinh(x)*x",
            "1/(4+x^2)",
            "sec(x)",
            "cos(x)^4",
            "1/(x^2+a^2)",
        ];
        for c in cases {
            let r = ev(&mut e, &format!("integrate({c}, x)"));
            assert!(!r.starts_with("ERR"), "{c}: {r}");
            // check d/dx at a couple of points
            let mut eng = Engine::new();
            eng.eval_line("a := 1.3").unwrap();
            for t in [0.37, 0.61] {
                let lhs = ap(&mut eng, &format!("slope(integrate({c}, x), x, {t})"));
                let f = eng.compile_src(c, &["x"]).unwrap();
                let fv = f.eval1(t);
                let dv = num(&lhs);
                assert!((dv - fv).abs() < 1e-6 * (1.0 + fv.abs()), "{c} at {t}: {dv} vs {fv} (F = {r})");
            }
        }
    }

    #[test]
    fn indefinite_failure() {
        let mut e = Engine::new();
        let r = ev(&mut e, "integrate(e^(x^3), x)");
        assert!(r.starts_with("ERR") && r.contains("límites"), "{r}");
    }

    #[test]
    fn definite_integrals() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "integrate(sin(x), x, 0, pi)"), "2");
        assert_eq!(ev(&mut e, "integrate(x^2, x, 0, 3)"), "9");
        assert_eq!(ev(&mut e, "integrate(1/x, x, 1, e)"), "1");
        assert_eq!(ev(&mut e, "integrate(e^(-x), x, 0, inf)"), "1");
        assert_eq!(ev(&mut e, "integrate(1/(1+x^2), x, -inf, inf)"), "π");
        assert_eq!(ev(&mut e, "integrate(ln(x), x, 0, 1)"), "-1");
        let g = ap(&mut e, "integrate(e^(-x^2), x, -inf, inf)");
        assert!((num(&g) - 1.772453851).abs() < 1e-8, "{g}");
        let g = ap(&mut e, "integrate(e^(x^3), x, 0, 1)");
        assert!((num(&g) - 1.341904418).abs() < 1e-8, "{g}");
        assert_eq!(ev(&mut e, "integrate(x^2, x, 0, t)"), "t^3/3");
        assert!(ev(&mut e, "integrate(1/x, x, 1, inf)").starts_with("ERR"));
    }

    #[test]
    fn limits() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "limit(sin(x)/x, x, 0)"), "1");
        assert_eq!(ev(&mut e, "limit((x^2-1)/(x-1), x, 1)"), "2");
        assert_eq!(ev(&mut e, "limit(1/x, x, 0, 1)"), "∞");
        assert_eq!(ev(&mut e, "limit(1/x, x, 0, -1)"), "-∞");
        assert!(ev(&mut e, "limit(1/x, x, 0)").contains("laterales"));
        assert_eq!(ev(&mut e, "limit((1+1/n)^n, n, inf)"), "e");
        assert_eq!(ev(&mut e, "limit((2x^2+1)/(x^2-3), x, inf)"), "2");
        assert_eq!(ev(&mut e, "limit(x^3/(x+1), x, -inf)"), "∞");
        assert_eq!(ev(&mut e, "limit((1-cos(x))/x^2, x, 0)"), "1/2");
        assert_eq!(ev(&mut e, "limit(x*ln(x), x, 0, 1)"), "0");
        assert_eq!(ev(&mut e, "limit(e^(-x)*x^5, x, inf)"), "0");
        assert_eq!(ev(&mut e, "limit(sqrt(x^2+x)-x, x, inf)"), "1/2");
        assert_eq!(ev(&mut e, "limit(atan(x), x, inf)"), "π/2");
        assert_eq!(ev(&mut e, "limit((e^x-1)/x, x, 0)"), "1");
        assert_eq!(ev(&mut e, "limit(sin(a*x)/x, x, 0)"), "a");
        assert_eq!(ev(&mut e, "limit(x^2+3, x, 2)"), "7");
        assert!(ev(&mut e, "limit(abs(x)/x, x, 0)").contains("laterales"));
        assert_eq!(ev(&mut e, "limit(abs(x)/x, x, 0, 1)"), "1");
        assert!(ev(&mut e, "limit(sin(1/x), x, 0)").starts_with("ERR"));
        assert_eq!(ev(&mut e, "limit(ln(x)/x, x, inf)"), "0");
        assert_eq!(ev(&mut e, "limit(x^(1/x), x, inf)"), "1");
    }

    #[test]
    fn taylor_series() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "taylor(e^x, x, 0, 4)"), ev(&mut e, "1 + x + x^2/2 + x^3/6 + x^4/24"));
        assert_eq!(ev(&mut e, "taylor(sin(x), x)"), ev(&mut e, "x - x^3/6 + x^5/120"));
        assert_eq!(ev(&mut e, "taylor(ln(x), x, 1, 2)"), ev(&mut e, "(x-1) - (x-1)^2/2"));
        assert_eq!(ev(&mut e, "taylor(sin(x)/x, x, 0, 2)"), ev(&mut e, "1 - x^2/6"));
    }

    #[test]
    fn sums_and_products() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "sum(k, k, 1, 100)"), "5050");
        assert_eq!(ev(&mut e, "sum(k^2, k, 1, n)"), ev(&mut e, "n*(n+1)*(2n+1)/6"));
        assert_eq!(ev(&mut e, "sum(k, k, 1, n)"), ev(&mut e, "n*(n+1)/2"));
        assert_eq!(ev(&mut e, "sum(1/2^k, k, 0, inf)"), "2");
        assert_eq!(ev(&mut e, "sum(1/k^2, k, 1, inf)"), "π^2/6");
        assert_eq!(ev(&mut e, "sum(1/fact(k), k, 0, inf)"), "e");
        assert!(ev(&mut e, "sum(1/k, k, 1, inf)").starts_with("ERR"));
        assert_eq!(ev(&mut e, "sum({1,2,3,4})"), "10");
        assert_eq!(ev(&mut e, "prod({1,2,3,4})"), "24");
        assert_eq!(ev(&mut e, "prod(k, k, 1, 5)"), "120");
        assert_eq!(ev(&mut e, "sum(x^k, k, 0, 3)"), ev(&mut e, "1 + x + x^2 + x^3"));
        assert_eq!(ev(&mut e, "sum(k^3, k, 1, 10000)"), "2500500025000000");
        assert_eq!(ev(&mut e, "sum(2^k, k, 0, n)"), ev(&mut e, "2^(n+1) - 1"));
        assert_eq!(ev(&mut e, "sum((-1)^(k+1)/k, k, 1, inf)"), "ln(2)");
    }

    #[test]
    fn seq_tangent_extrema() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "seq(k^2, k, 1, 5)"), "{1, 4, 9, 16, 25}");
        assert_eq!(ev(&mut e, "seq(k, k, 0, 1, 1/4)"), "{0, 1/4, 1/2, 3/4, 1}");
        assert_eq!(ev(&mut e, "tangent(x^2, x, 1)"), "y = 2x - 1");
        assert_eq!(ev(&mut e, "normal(x^2, x, 1)"), ev(&mut e, "y = -x/2 + 3/2"));
        assert_eq!(ev(&mut e, "fmin(x^2-2x+3, x, -5, 5)"), "{x = 1, y = 2}");
        assert_eq!(ev(&mut e, "fmax(-x^2+4x, x, 0, 5)"), "{x = 2, y = 4}");
        assert_eq!(ev(&mut e, "slope(x^3, x, 2)"), "12");
        assert_eq!(ev(&mut e, "slope(x^3, 2)"), "12");
    }
}
