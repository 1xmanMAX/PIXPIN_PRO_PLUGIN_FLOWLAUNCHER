//! Symbolic evaluator: bottom-up canonicalization + builtin function library.

use crate::expr::Expr;
use crate::matrix::{self, as_vector};
use crate::num::{fmt_f64, gamma, gcd_i128, is_prime_u128, Rat, C};
use crate::numeric::{eval_complex, Compiled};
use crate::simplify::{add, div, expand, from_c, mul, neg, pow, sub, R};

#[derive(Clone, Debug)]
pub struct Settings {
    /// Angles in degrees instead of radians.
    pub deg: bool,
    /// Show complex solutions in `solve`.
    pub complex: bool,
    /// Significant digits for decimal display.
    pub digits: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { deg: false, complex: false, digits: 10 }
    }
}

pub struct Cas<'a> {
    pub s: &'a Settings,
}

/// Every builtin function name (used by the parser to recognize calls).
pub const BUILTINS: &[&str] = &[
    "sin", "cos", "tan", "sec", "csc", "cot", "asin", "acos", "atan", "asec", "acsc", "acot", "atan2", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "exp", "ln", "log", "log2", "sqrt", "cbrt", "root", "abs", "sign", "floor", "ceil", "round", "trunc", "frac", "fact", "gamma", "ncr", "npr", "gcd", "lcm", "mod", "isprime", "factor", "nextprime", "re", "im", "conj", "arg", "polar", "min", "max", "approx", "exact", "simplify", "expand", "diff", "integrate", "limit", "taylor", "sum", "prod", "solve", "nsolve", "tangent", "normal", "slope", "det", "inv", "trn", "rref", "rank", "trace", "identity", "zeros", "ones", "size", "dot", "cross", "norm", "eigvals", "eigvecs", "charpoly", "augment", "index", "mean", "median", "mode", "var", "pvar", "stdev", "pstdev", "sem", "range", "count", "sort", "sortd", "cumsum", "q1", "q3", "iqr", "cov", "corr", "linreg", "quadreg", "cubicreg", "expreg", "logreg", "powreg", "intercept", "seq", "normpdf", "normcdf", "invnorm", "binompdf", "binomcdf", "poissonpdf", "poissoncdf", "abserr", "relerr", "pcterr", "distance", "midpoint", "triangle", "heron", "polyarea", "angle", "todeg", "torad", "bin", "hex", "oct", "numer", "denom", "erf", "erfc", "heaviside", "hypot", "fmin", "fmax", "lhs", "rhs", "coeffs", "degree", "quotient", "remainder", "cancel", "nullspace", "lu", "zscore", "plot",
];

pub fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

fn nargs(name: &str, args: &[Expr], lo: usize, hi: usize) -> Result<(), String> {
    if args.len() < lo || args.len() > hi {
        if lo == hi {
            return Err(format!("{name} requiere {lo} argumento(s)"));
        }
        return Err(format!("{name} requiere entre {lo} y {hi} argumentos"));
    }
    Ok(())
}

/// k such that e = k·π (exact), if recognizable.
fn pi_multiple(e: &Expr) -> Option<Rat> {
    match e {
        Expr::Num(r) if r.is_zero() => Some(Rat::ZERO),
        Expr::Sym(s) if s == "pi" => Some(Rat::ONE),
        Expr::Mul(v) if v.len() == 2 => match (&v[0], &v[1]) {
            (Expr::Num(r), Expr::Sym(s)) if s == "pi" => Some(*r),
            _ => None,
        },
        _ => None,
    }
}

fn sqrt_rat(n: i128, d: i128) -> Expr {
    pow(Expr::rat(n, d), Expr::rat(1, 2)).unwrap_or(Expr::zero())
}

/// sin(kπ) for "nice" k.
pub(crate) fn sin_table(k: Rat) -> Option<Expr> {
    let two = Rat::int(2);
    let mut t = k.sub(two.mul(Rat::int(k.div(two)?.floor()))?)?; // t in [0,2)
    let mut sign = 1;
    if t >= Rat::ONE {
        t = t.sub(Rat::ONE)?;
        sign = -1;
    }
    if t > Rat::new(1, 2)? {
        t = Rat::ONE.sub(t)?;
    }
    let v = if t.is_zero() {
        Expr::zero()
    } else if t == Rat::new(1, 6)? {
        Expr::rat(1, 2)
    } else if t == Rat::new(1, 4)? {
        mul(vec![Expr::rat(1, 2), sqrt_rat(2, 1)]).ok()?
    } else if t == Rat::new(1, 3)? {
        mul(vec![Expr::rat(1, 2), sqrt_rat(3, 1)]).ok()?
    } else if t == Rat::new(1, 2)? {
        Expr::one()
    } else if t == Rat::new(1, 12)? {
        // (√6 - √2)/4
        mul(vec![Expr::rat(1, 4), sub(sqrt_rat(6, 1), sqrt_rat(2, 1)).ok()?]).ok()?
    } else if t == Rat::new(5, 12)? {
        mul(vec![Expr::rat(1, 4), add(vec![sqrt_rat(6, 1), sqrt_rat(2, 1)]).ok()?]).ok()?
    } else if t == Rat::new(1, 10)? {
        // (√5 - 1)/4
        mul(vec![Expr::rat(1, 4), sub(sqrt_rat(5, 1), Expr::one()).ok()?]).ok()?
    } else {
        return None;
    };
    if sign < 0 {
        neg(v).ok()
    } else {
        Some(v)
    }
}

/// Recognizes a float as a "nice" exact number: rational or √rational.
pub fn recognize(v: f64) -> Option<Expr> {
    if !v.is_finite() {
        return None;
    }
    if let Some(r) = Rat::approximate(v, 10_000, 1e-13) {
        return Some(Expr::Num(r));
    }
    if v > 0.0 {
        if let Some(r) = Rat::approximate(v * v, 10_000, 1e-12) {
            return pow(Expr::Num(r), Expr::rat(1, 2)).ok();
        }
    } else if let Some(r) = Rat::approximate(v * v, 10_000, 1e-12) {
        return neg(pow(Expr::Num(r), Expr::rat(1, 2)).ok()?).ok();
    }
    None
}

fn to_list(e: &Expr) -> Result<Vec<Expr>, String> {
    match e {
        Expr::List(v) => Ok(v.clone()),
        Expr::Matrix(_) => as_vector(e).ok_or_else(|| "Se esperaba una lista o vector".to_string()),
        _ => Err("Se esperaba una lista, por ejemplo {1, 2, 3}".into()),
    }
}

fn int_arg(e: &Expr, what: &str) -> Result<i128, String> {
    e.as_int().ok_or_else(|| format!("{what} debe ser un número entero"))
}

impl<'a> Cas<'a> {
    pub fn new(s: &'a Settings) -> Self {
        Cas { s }
    }

    pub fn eval(&self, e: &Expr) -> R {
        Ok(match e {
            Expr::Num(_) | Expr::Float(_) | Expr::Sym(_) | Expr::Text(_) => e.clone(),
            Expr::Add(v) => add(v.iter().map(|x| self.eval(x)).collect::<Result<_, _>>()?)?,
            Expr::Mul(v) => mul(v.iter().map(|x| self.eval(x)).collect::<Result<_, _>>()?)?,
            Expr::Pow(b, x) => pow(self.eval(b)?, self.eval(x)?)?,
            Expr::Call(n, a) => {
                let args: Vec<Expr> = a.iter().map(|x| self.eval(x)).collect::<Result<_, _>>()?;
                self.apply(n, args)?
            }
            Expr::Matrix(m) => Expr::Matrix(m.iter().map(|r| r.iter().map(|x| self.eval(x)).collect()).collect::<Result<Vec<Vec<_>>, _>>()?),
            Expr::List(v) => Expr::List(v.iter().map(|x| self.eval(x)).collect::<Result<_, _>>()?),
            Expr::Eq(a, b) => Expr::Eq(Box::new(self.eval(a)?), Box::new(self.eval(b)?)),
            Expr::Rel(o, a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                // decide closed numeric comparisons
                if !a.has_free_symbols() && !b.has_free_symbols() {
                    if let (Ok(x), Ok(y)) = (self.num(&a), self.num(&b)) {
                        if x.is_real() && y.is_real() {
                            let t = match o.as_str() {
                                "<" => x.re < y.re,
                                ">" => x.re > y.re,
                                "<=" => x.re <= y.re,
                                ">=" => x.re >= y.re,
                                _ => (x.re - y.re).abs() > 1e-12,
                            };
                            return Ok(Expr::Text(if t { "verdadero".into() } else { "falso".into() }));
                        }
                    }
                }
                Expr::Rel(o.clone(), Box::new(a), Box::new(b))
            }
        })
    }

    /// Complex numeric value of a closed expression.
    pub fn num(&self, e: &Expr) -> Result<C, String> {
        eval_complex(e, self.s.deg)
    }

    pub fn num_real(&self, e: &Expr) -> Result<f64, String> {
        let z = self.num(e)?;
        if !z.is_real() {
            return Err("Se esperaba un número real".into());
        }
        Ok(z.re)
    }

    /// Numeric approximation preserving structure (lists, matrices).
    pub fn approx(&self, e: &Expr) -> R {
        Ok(match e {
            Expr::List(v) => Expr::List(v.iter().map(|x| self.approx(x)).collect::<Result<_, _>>()?),
            Expr::Matrix(m) => Expr::Matrix(matrix::map_m(m, |x| self.approx(x))?),
            Expr::Eq(a, b) => Expr::Eq(Box::new(self.approx(a)?), Box::new(self.approx(b)?)),
            Expr::Text(_) => e.clone(),
            _ if e.has_free_symbols() => self.approx_partial(e)?,
            _ => from_c(self.num(e)?),
        })
    }

    /// Replaces closed numeric sub-expressions with floats.
    fn approx_partial(&self, e: &Expr) -> R {
        if !e.has_free_symbols() {
            if let Ok(z) = self.num(e) {
                return Ok(from_c(z));
            }
            return Ok(e.clone());
        }
        Ok(match e {
            Expr::Add(v) => add(v.iter().map(|x| self.approx_partial(x)).collect::<Result<_, _>>()?)?,
            Expr::Mul(v) => mul(v.iter().map(|x| self.approx_partial(x)).collect::<Result<_, _>>()?)?,
            Expr::Pow(b, x) => {
                let b2 = if b.sym_name() == Some("e") { (**b).clone() } else { self.approx_partial(b)? };
                // keep integer exponents exact
                let x2 = if x.as_rat().is_some() { (**x).clone() } else { self.approx_partial(x)? };
                pow(b2, x2)?
            }
            Expr::Call(n, a) => Expr::Call(n.clone(), a.iter().map(|x| self.approx_partial(x)).collect::<Result<_, _>>()?),
            other => other.clone(),
        })
    }

    pub fn compile(&self, e: &Expr, vars: &[&str]) -> Result<Compiled, String> {
        Compiled::new(e, vars, self.s.deg)
    }

    /// Symbolic → numeric closure in one variable.
    pub fn func1(&self, e: &Expr, x: &str) -> Result<impl Fn(f64) -> f64, String> {
        let c = self.compile(e, &[x])?;
        Ok(move |t: f64| c.eval1(t))
    }

    fn angle_to_pi(&self, a: &Expr) -> Option<Rat> {
        if self.s.deg {
            a.as_rat().and_then(|r| r.div(Rat::int(180)))
        } else {
            pi_multiple(a)
        }
    }

    fn pi_to_angle(&self, k: Rat) -> R {
        if self.s.deg {
            Ok(Expr::Num(k.mul(Rat::int(180)).ok_or("desbordamiento")?))
        } else {
            mul(vec![Expr::Num(k), Expr::sym("pi")])
        }
    }

    fn numeric_fallback(&self, name: &str, args: &[Expr]) -> Option<R> {
        // Any float argument -> numeric evaluation
        if args.iter().all(|a| a.is_number() || !a.has_free_symbols()) && args.iter().any(|a| a.contains_float()) {
            let call = Expr::Call(name.to_string(), args.to_vec());
            return Some(self.num(&call).map(from_c));
        }
        None
    }

    pub fn apply(&self, name: &str, args: Vec<Expr>) -> R {
        // map elementwise over lists for unary math functions
        if args.len() == 1 && args[0].is_list() && crate::numeric::func1(name, false).is_some() {
            if let Expr::List(v) = &args[0] {
                return Ok(Expr::List(v.iter().map(|x| self.apply(name, vec![x.clone()])).collect::<Result<_, _>>()?));
            }
        }
        match name {
            "sin" | "cos" | "tan" | "sec" | "csc" | "cot" => {
                nargs(name, &args, 1, 1)?;
                let a = &args[0];
                if let Some(r) = self.numeric_fallback(name, &args) {
                    return r;
                }
                // odd/even symmetry
                if let Expr::Mul(v) = a {
                    if let Some(Expr::Num(c)) = v.first() {
                        if c.is_neg() {
                            let pos = neg(a.clone())?;
                            let inner = self.apply(name, vec![pos])?;
                            return if matches!(name, "cos" | "sec") { Ok(inner) } else { neg(inner) };
                        }
                    }
                }
                if let Some(k) = self.angle_to_pi(a) {
                    let s = sin_table(k);
                    let c = Rat::new(1, 2).and_then(|h| k.add(h)).and_then(sin_table);
                    if let (Some(s), Some(c)) = (s, c) {
                        return match name {
                            "sin" => Ok(s),
                            "cos" => Ok(c),
                            "tan" => div(s, c).map_err(|_| "tan indefinida en ese ángulo".into()),
                            "cot" => div(c, s).map_err(|_| "cot indefinida en ese ángulo".into()),
                            "sec" => div(Expr::one(), c).map_err(|_| "sec indefinida en ese ángulo".into()),
                            _ => div(Expr::one(), s).map_err(|_| "csc indefinida en ese ángulo".into()),
                        };
                    }
                }
                // inverse composition: sin(asin(x)) = x
                if let Expr::Call(f, inner) = a {
                    if inner.len() == 1 && ((name == "sin" && f == "asin") || (name == "cos" && f == "acos") || (name == "tan" && f == "atan")) {
                        return Ok(inner[0].clone());
                    }
                }
                Ok(Expr::Call(name.into(), args))
            }
            "asin" | "acos" | "atan" => {
                nargs(name, &args, 1, 1)?;
                if let Some(r) = self.numeric_fallback(name, &args) {
                    return r;
                }
                let a = &args[0];
                if !a.has_free_symbols() {
                    if let Ok(z) = self.num(a) {
                        if z.is_real() {
                            let v = match name {
                                "asin" => z.re.asin(),
                                "acos" => z.re.acos(),
                                _ => z.re.atan(),
                            };
                            if v.is_finite() {
                                if let Some(k) = Rat::approximate(v / std::f64::consts::PI, 24, 1e-13) {
                                    if let Some(t) = sin_table(if name == "acos" { Rat::new(1, 2).unwrap().sub(k).unwrap_or(k) } else { k }) {
                                        let _ = t;
                                    }
                                    return self.pi_to_angle(k);
                                }
                            }
                        }
                    }
                }
                Ok(Expr::Call(name.into(), args))
            }
            "asec" | "acsc" | "acot" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh" | "atanh" | "erf" | "erfc" | "gamma" | "log2" | "heaviside" => {
                nargs(name, &args, 1, 1)?;
                if let Some(r) = self.numeric_fallback(name, &args) {
                    return r;
                }
                let a = &args[0];
                if a.is_zero() && matches!(name, "sinh" | "tanh" | "asinh" | "atanh" | "erf") {
                    return Ok(Expr::zero());
                }
                if a.is_zero() && name == "cosh" {
                    return Ok(Expr::one());
                }
                if name == "gamma" {
                    if let Some(n) = a.as_int() {
                        if n >= 1 {
                            return self.apply("fact", vec![Expr::int(n - 1)]);
                        }
                    }
                }
                if name == "log2" {
                    return self.apply("log", vec![a.clone(), Expr::int(2)]);
                }
                Ok(Expr::Call(name.into(), args))
            }
            "atan2" => {
                nargs(name, &args, 2, 2)?;
                let y = self.num_real(&args[0])?;
                let x = self.num_real(&args[1])?;
                let v = y.atan2(x);
                if let Some(k) = Rat::approximate(v / std::f64::consts::PI, 24, 1e-13) {
                    return self.pi_to_angle(k);
                }
                Ok(Expr::Float(if self.s.deg { v.to_degrees() } else { v }))
            }
            "exp" => {
                nargs(name, &args, 1, 1)?;
                pow(Expr::sym("e"), args.into_iter().next().unwrap())
            }
            "sqrt" => {
                nargs(name, &args, 1, 1)?;
                pow(args.into_iter().next().unwrap(), Expr::rat(1, 2))
            }
            "cbrt" => {
                nargs(name, &args, 1, 1)?;
                pow(args.into_iter().next().unwrap(), Expr::rat(1, 3))
            }
            "root" => {
                nargs(name, &args, 2, 2)?;
                let mut it = args.into_iter();
                let x = it.next().unwrap();
                let n = it.next().unwrap();
                pow(x, pow(n, Expr::int(-1))?)
            }
            "ln" => {
                nargs(name, &args, 1, 1)?;
                if let Some(r) = self.numeric_fallback(name, &args) {
                    return r;
                }
                let a = &args[0];
                if a.is_one() {
                    return Ok(Expr::zero());
                }
                if a.is_zero() {
                    return Err("ln(0) no está definido".into());
                }
                if let Expr::Sym(s) = a {
                    if s == "e" {
                        return Ok(Expr::one());
                    }
                }
                if let Expr::Pow(b, x) = a {
                    if let Expr::Sym(s) = b.as_ref() {
                        if s == "e" {
                            return Ok((**x).clone());
                        }
                    }
                    // ln(a^n) = n ln(a) for positive numeric base
                    if b.as_rat().map_or(false, |r| !r.is_neg()) {
                        return mul(vec![(**x).clone(), self.apply("ln", vec![(**b).clone()])?]);
                    }
                }
                // ln of perfect integer powers: ln(8) = 3 ln(2)
                if let Some(n) = a.as_int() {
                    if n > 1 {
                        let fs = crate::num::factorize(n as u128);
                        if fs.len() == 1 && fs[0].1 > 1 {
                            return mul(vec![Expr::int(fs[0].1 as i128), Expr::call("ln", vec![Expr::int(fs[0].0 as i128)])]);
                        }
                    }
                }
                Ok(Expr::Call(name.into(), args))
            }
            "log" => {
                nargs(name, &args, 1, 2)?;
                let base = args.get(1).cloned().unwrap_or(Expr::int(10));
                if let Some(r) = self.numeric_fallback(name, &args) {
                    return r;
                }
                let x = &args[0];
                if x.is_one() {
                    return Ok(Expr::zero());
                }
                if *x == base {
                    return Ok(Expr::one());
                }
                if let (Some(xr), Some(br)) = (x.as_rat(), base.as_rat()) {
                    if !xr.is_neg() && br.to_f64() > 0.0 && br != Rat::ONE {
                        let k = (xr.to_f64().ln() / br.to_f64().ln()).round() as i64;
                        if k.abs() < 200 && br.powi(k) == Some(xr) {
                            return Ok(Expr::int(k as i128));
                        }
                        // rational exponent, e.g. log(√10)=1/2
                    }
                }
                if let Expr::Pow(b, e) = x {
                    if **b == base {
                        return Ok((**e).clone());
                    }
                }
                if args.len() == 2 && base == Expr::sym("e") {
                    return self.apply("ln", vec![x.clone()]);
                }
                Ok(Expr::Call(name.into(), args))
            }
            "abs" => {
                nargs(name, &args, 1, 1)?;
                let a = &args[0];
                match a {
                    Expr::Num(r) => return Ok(Expr::Num(Rat { n: r.n.abs(), d: r.d })),
                    Expr::Float(f) => return Ok(Expr::Float(f.abs())),
                    Expr::Matrix(_) => return self.apply("det", args),
                    _ => {}
                }
                if !a.has_free_symbols() {
                    let z = self.num(a)?;
                    let v = z.abs();
                    if a.contains_float() {
                        return Ok(Expr::Float(v));
                    }
                    if z.is_real() {
                        return if z.re >= 0.0 { Ok(a.clone()) } else { neg(a.clone()) };
                    }
                    return Ok(recognize(v).unwrap_or(Expr::Float(v)));
                }
                if let Expr::Mul(v) = a {
                    if let Some(Expr::Num(c)) = v.first() {
                        let rest = div(a.clone(), Expr::Num(*c))?;
                        return mul(vec![Expr::Num(Rat { n: c.n.abs(), d: c.d }), self.apply("abs", vec![rest])?]);
                    }
                }
                if let Expr::Pow(_, x) = a {
                    if x.as_int().map_or(false, |k| k % 2 == 0) {
                        return Ok(a.clone());
                    }
                }
                Ok(Expr::Call(name.into(), args))
            }
            "sign" | "floor" | "ceil" | "round" | "trunc" | "int" | "frac" => {
                nargs(name, &args, 1, 2)?;
                let a = &args[0];
                if name == "round" && args.len() == 2 {
                    let d = int_arg(&args[1], "Los decimales")?;
                    let v = self.num_real(a)?;
                    let p = 10f64.powi(d as i32);
                    return Ok(Expr::Float((v * p).round() / p));
                }
                if let Some(r) = a.as_rat() {
                    let fl = r.floor();
                    return Ok(match name {
                        "sign" => Expr::int(r.n.signum()),
                        "floor" => Expr::int(fl),
                        "ceil" => Expr::int(if r.is_int() { r.n } else { fl + 1 }),
                        "round" => Expr::int(r.add(Rat::new(1, 2).unwrap()).map(|x| x.floor()).unwrap_or(fl)),
                        "trunc" | "int" => Expr::int(if r.is_neg() && !r.is_int() { fl + 1 } else { fl }),
                        _ => {
                            let t = if r.is_neg() && !r.is_int() { fl + 1 } else { fl };
                            Expr::Num(r.sub(Rat::int(t)).unwrap())
                        }
                    });
                }
                if !a.has_free_symbols() {
                    let v = self.num_real(a)?;
                    let f = crate::numeric::func1(if name == "int" { "trunc" } else { name }, false).unwrap();
                    let res = f(v);
                    if name == "frac" {
                        return Ok(Expr::Float(res));
                    }
                    return Ok(Expr::int(res as i128));
                }
                Ok(Expr::Call(name.into(), args))
            }
            "fact" => {
                nargs(name, &args, 1, 1)?;
                let a = &args[0];
                if let Some(n) = a.as_rat().filter(|r| r.is_int()).map(|r| r.n) {
                    if n < 0 {
                        return Err("El factorial no está definido para enteros negativos".into());
                    }
                    if n <= 33 {
                        let mut acc: i128 = 1;
                        for k in 2..=n {
                            acc *= k;
                        }
                        return Ok(Expr::int(acc));
                    }
                    if n <= 170 {
                        return Ok(Expr::Float(gamma(n as f64 + 1.0)));
                    }
                    return Err("Factorial demasiado grande (máx. 170!)".into());
                }
                if a.is_number() {
                    return Ok(Expr::Float(gamma(a.as_f64().unwrap() + 1.0)));
                }
                Ok(Expr::Call(name.into(), args))
            }
            "ncr" | "npr" => {
                nargs(name, &args, 2, 2)?;
                let (n, k) = match (args[0].as_int(), args[1].as_int()) {
                    (Some(n), Some(k)) => (n, k),
                    _ => {
                        if let Some(r) = self.numeric_fallback(name, &args) {
                            return r;
                        }
                        return Ok(Expr::Call(name.into(), args));
                    }
                };
                if k < 0 || n < 0 || k > n {
                    return Ok(Expr::zero());
                }
                let mut acc: i128 = 1;
                if name == "npr" {
                    for t in (n - k + 1)..=n {
                        acc = acc.checked_mul(t).ok_or("Resultado demasiado grande")?;
                    }
                } else {
                    let k = k.min(n - k);
                    for t in 1..=k {
                        acc = acc.checked_mul(n - k + t).ok_or("Resultado demasiado grande")? / t;
                    }
                }
                Ok(Expr::int(acc))
            }
            "gcd" | "lcm" => {
                let vals: Vec<Expr> = if args.len() == 1 { to_list(&args[0])? } else { args.clone() };
                if vals.is_empty() {
                    return Err(format!("{name} requiere argumentos"));
                }
                if vals.iter().any(|v| v.has_free_symbols()) {
                    if name == "gcd" && vals.len() == 2 {
                        return crate::solve::poly_gcd_expr(self, &vals[0], &vals[1]);
                    }
                    return Ok(Expr::Call(name.into(), args));
                }
                let mut acc = int_arg(&vals[0], "El argumento")?;
                for v in &vals[1..] {
                    let b = int_arg(v, "El argumento")?;
                    acc = if name == "gcd" {
                        gcd_i128(acc, b)
                    } else {
                        let g = gcd_i128(acc, b);
                        if g == 0 {
                            0
                        } else {
                            (acc / g).checked_mul(b).ok_or("Resultado demasiado grande")?.abs()
                        }
                    };
                }
                Ok(Expr::int(acc))
            }
            "mod" => {
                nargs(name, &args, 2, 2)?;
                if let (Some(a), Some(b)) = (args[0].as_rat(), args[1].as_rat()) {
                    if b.is_zero() {
                        return Err("mod entre cero".into());
                    }
                    let q = a.div(b).ok_or("desbordamiento")?.floor();
                    return Ok(Expr::Num(a.sub(b.mul(Rat::int(q)).ok_or("desbordamiento")?).ok_or("desbordamiento")?));
                }
                if let Some(r) = self.numeric_fallback(name, &args) {
                    return r;
                }
                if args.iter().any(|a| a.has_free_symbols()) {
                    return Ok(Expr::Call(name.into(), args));
                }
                Ok(Expr::Float(crate::numeric::Compiled::new(&Expr::Call("mod".into(), args), &[], self.s.deg)?.eval(&[])))
            }
            "isprime" => {
                nargs(name, &args, 1, 1)?;
                let n = int_arg(&args[0], "El argumento")?;
                Ok(Expr::Text(if n > 1 && is_prime_u128(n as u128) { "verdadero".into() } else { "falso".into() }))
            }
            "nextprime" => {
                nargs(name, &args, 1, 1)?;
                let mut n = int_arg(&args[0], "El argumento")?.max(1) + 1;
                while !is_prime_u128(n as u128) {
                    n += 1;
                }
                Ok(Expr::int(n))
            }
            "factor" => {
                nargs(name, &args, 1, 2)?;
                if let Some(n) = args[0].as_rat().filter(|r| r.is_int()).map(|r| r.n) {
                    if n.abs() < 2 {
                        return Ok(Expr::int(n));
                    }
                    let fs = crate::num::factorize(n.unsigned_abs());
                    let parts: Vec<String> = fs.iter().map(|(p, k)| if *k == 1 { p.to_string() } else { format!("{p}^{k}") }).collect();
                    return Ok(Expr::Text(format!("{}{}", if n < 0 { "-" } else { "" }, parts.join("·"))));
                }
                if let Some(r) = args[0].as_rat() {
                    let a = self.apply("factor", vec![Expr::int(r.n)])?;
                    let b = self.apply("factor", vec![Expr::int(r.d)])?;
                    return Ok(Expr::Text(format!("({}) / ({})", a, b)));
                }
                crate::solve::factor(self, &args[0], args.get(1))
            }
            "re" | "im" | "conj" => {
                nargs(name, &args, 1, 1)?;
                let a = &args[0];
                if a.is_list() {
                    if let Expr::List(v) = a {
                        return Ok(Expr::List(v.iter().map(|x| self.apply(name, vec![x.clone()])).collect::<Result<_, _>>()?));
                    }
                }
                let (re, im) = self.split_complex(a)?;
                match name {
                    "re" => Ok(re),
                    "im" => Ok(im),
                    _ => sub(re, mul(vec![im, Expr::sym("i")])?),
                }
            }
            "arg" => {
                nargs(name, &args, 1, 1)?;
                let z = self.num(&args[0])?;
                let v = z.arg();
                if !args[0].contains_float() {
                    if let Some(k) = Rat::approximate(v / std::f64::consts::PI, 24, 1e-13) {
                        return self.pi_to_angle(k);
                    }
                }
                Ok(Expr::Float(if self.s.deg { v.to_degrees() } else { v }))
            }
            "polar" => {
                nargs(name, &args, 1, 1)?;
                let z = self.num(&args[0])?;
                let r = recognize(z.abs()).unwrap_or(Expr::Float(z.abs()));
                let th = self.apply("arg", args.clone())?;
                Ok(Expr::Text(format!("{} ∠ {}", r, th)))
            }
            "min" | "max" => {
                let vals: Vec<Expr> = if args.len() == 1 { to_list(&args[0])? } else { args.clone() };
                if vals.is_empty() {
                    return Err(format!("{name} requiere argumentos"));
                }
                if vals.iter().any(|v| v.has_free_symbols()) {
                    return Ok(Expr::Call(name.into(), args));
                }
                let mut best = vals[0].clone();
                let mut bv = self.num_real(&best)?;
                for v in &vals[1..] {
                    let x = self.num_real(v)?;
                    if (name == "min" && x < bv) || (name == "max" && x > bv) {
                        bv = x;
                        best = v.clone();
                    }
                }
                Ok(best)
            }
            "approx" => {
                nargs(name, &args, 1, 1)?;
                self.approx(&args[0])
            }
            "exact" => {
                nargs(name, &args, 1, 1)?;
                let a = &args[0];
                match a {
                    Expr::Float(f) => Ok(recognize(*f).or_else(|| Rat::approximate(*f, 1_000_000_000, 1e-15).map(Expr::Num)).unwrap_or(a.clone())),
                    Expr::List(v) => Ok(Expr::List(v.iter().map(|x| self.apply("exact", vec![x.clone()])).collect::<Result<_, _>>()?)),
                    _ => Ok(a.clone()),
                }
            }
            "simplify" => {
                nargs(name, &args, 1, 1)?;
                crate::solve::simplify_full(self, &args[0])
            }
            "expand" => {
                nargs(name, &args, 1, 1)?;
                let e = expand(&args[0])?;
                self.eval(&e)
            }
            "cancel" => {
                nargs(name, &args, 1, 1)?;
                crate::solve::cancel(self, &args[0])
            }
            "numer" | "denom" => {
                nargs(name, &args, 1, 1)?;
                let (n, d) = crate::simplify::num_den(&args[0]);
                Ok(if name == "numer" { n } else { d })
            }
            "lhs" | "rhs" => {
                nargs(name, &args, 1, 1)?;
                match &args[0] {
                    Expr::Eq(a, b) | Expr::Rel(_, a, b) => Ok(if name == "lhs" { (**a).clone() } else { (**b).clone() }),
                    _ => Err("Se esperaba una ecuación".into()),
                }
            }
            "diff" => crate::calculus::cmd_diff(self, args),
            "integrate" => crate::calculus::cmd_integrate(self, args),
            "limit" => crate::calculus::cmd_limit(self, args),
            "taylor" => crate::calculus::cmd_taylor(self, args),
            "sum" | "prod" => crate::calculus::cmd_sum(self, name, args),
            "seq" => crate::calculus::cmd_seq(self, args),
            "tangent" | "normal" => crate::calculus::cmd_tangent(self, name, args),
            "fmin" | "fmax" => crate::calculus::cmd_fminmax(self, name, args),
            "slope" => {
                if args.len() == 2 && args[0].is_list() {
                    return crate::stats::cmd(self, "slope", args);
                }
                if args.len() == 4 && args.iter().all(|a| !a.has_free_symbols()) {
                    return div(sub(args[3].clone(), args[1].clone())?, sub(args[2].clone(), args[0].clone())?).map_err(|_| "Pendiente indefinida (recta vertical)".into());
                }
                if args.len() == 2 && args[0].is_list() == false && as_vector(&args[0]).is_some() {
                    let p = as_vector(&args[0]).unwrap();
                    let q = as_vector(&args[1]).ok_or("Se esperaban dos puntos")?;
                    if p.len() == 2 && q.len() == 2 {
                        return div(sub(q[1].clone(), p[1].clone())?, sub(q[0].clone(), p[0].clone())?);
                    }
                }
                crate::calculus::cmd_slope_at(self, args)
            }
            "solve" | "nsolve" => crate::solve::cmd_solve(self, name, args),
            "coeffs" | "degree" | "quotient" | "remainder" => crate::solve::cmd_poly(self, name, args),
            "det" | "inv" | "trn" | "rref" | "rank" | "trace" | "identity" | "zeros" | "ones" | "size" | "dot" | "cross" | "norm" | "eigvals" | "eigvecs" | "charpoly" | "augment" | "index" | "nullspace" | "lu" => self.matrix_cmd(name, args),
            "mean" | "median" | "mode" | "var" | "pvar" | "stdev" | "pstdev" | "sem" | "range" | "count" | "sort" | "sortd" | "cumsum" | "q1" | "q3" | "iqr" | "cov" | "corr" | "linreg" | "quadreg" | "cubicreg" | "expreg" | "logreg" | "powreg" | "intercept" | "normpdf" | "normcdf" | "invnorm" | "binompdf" | "binomcdf" | "poissonpdf" | "poissoncdf" | "abserr" | "relerr" | "pcterr" | "zscore" => crate::stats::cmd(self, name, args),
            "distance" | "midpoint" | "triangle" | "heron" | "polyarea" | "angle" => crate::geometry::cmd(self, name, args),
            "hypot" => {
                nargs(name, &args, 2, 2)?;
                pow(add(vec![pow(args[0].clone(), Expr::int(2))?, pow(args[1].clone(), Expr::int(2))?])?, Expr::rat(1, 2))
            }
            "todeg" => {
                nargs(name, &args, 1, 1)?;
                mul(vec![args[0].clone(), Expr::int(180), pow(Expr::sym("pi"), Expr::int(-1))?])
            }
            "torad" => {
                nargs(name, &args, 1, 1)?;
                mul(vec![args[0].clone(), Expr::sym("pi"), Expr::rat(1, 180)])
            }
            "bin" | "hex" | "oct" => {
                nargs(name, &args, 1, 1)?;
                let n = int_arg(&args[0], "El argumento")?;
                let (s, a) = (if n < 0 { "-" } else { "" }, n.unsigned_abs());
                Ok(Expr::Text(match name {
                    "bin" => format!("{s}0b{a:b}"),
                    "hex" => format!("{s}0x{a:X}"),
                    _ => format!("{s}0o{a:o}"),
                }))
            }
            "plot" => Err("plot(...) solo puede usarse como instrucción principal".into()),
            _ => Ok(Expr::Call(name.into(), args)),
        }
    }

    /// Splits a closed or symbolic expression into real and imaginary parts
    /// (treating all free symbols as real).
    pub fn split_complex(&self, e: &Expr) -> Result<(Expr, Expr), String> {
        let ex = expand(e)?;
        let terms = match ex {
            Expr::Add(t) => t,
            other => vec![other],
        };
        let i = Expr::sym("i");
        let mut re = Vec::new();
        let mut im = Vec::new();
        for t in terms {
            if t.free_of("i") {
                re.push(t);
            } else {
                let c = div(t.clone(), i.clone())?;
                if c.free_of("i") {
                    im.push(c);
                } else {
                    // non-polynomial in i: numeric
                    let z = self.num(&t)?;
                    re.push(Expr::Float(z.re));
                    im.push(Expr::Float(z.im));
                }
            }
        }
        Ok((add(re)?, add(im)?))
    }

    fn matrix_cmd(&self, name: &str, args: Vec<Expr>) -> R {
        let need_m = |i: usize| -> Result<&Vec<Vec<Expr>>, String> {
            match args.get(i) {
                Some(Expr::Matrix(m)) => Ok(m),
                _ => Err(format!("{name} requiere una matriz, por ejemplo [[1,2],[3,4]]")),
            }
        };
        match name {
            "det" => matrix::det(need_m(0)?),
            "inv" => {
                if let Some(Expr::Matrix(m)) = args.first() {
                    return Ok(Expr::Matrix(matrix::inverse(m)?));
                }
                nargs(name, &args, 1, 1)?;
                pow(args[0].clone(), Expr::int(-1))
            }
            "trn" => Ok(Expr::Matrix(match &args[0] {
                Expr::Matrix(m) => matrix::transpose(m),
                Expr::List(v) => v.iter().map(|x| vec![x.clone()]).collect(),
                _ => return Err("trn requiere una matriz".into()),
            })),
            "rref" => Ok(Expr::Matrix(matrix::rref(need_m(0)?)?.0)),
            "rank" => Ok(Expr::int(matrix::rank(need_m(0)?)? as i128)),
            "trace" => matrix::trace(need_m(0)?),
            "nullspace" => {
                let ns = matrix::nullspace(need_m(0)?)?;
                Ok(Expr::List(ns.into_iter().map(|v| Expr::Matrix(v.into_iter().map(|x| vec![x]).collect())).collect()))
            }
            "identity" => {
                let n = int_arg(&args[0], "La dimensión")?;
                if !(1..=200).contains(&n) {
                    return Err("Dimensión fuera de rango".into());
                }
                Ok(Expr::Matrix(matrix::identity(n as usize)))
            }
            "zeros" | "ones" => {
                nargs(name, &args, 1, 2)?;
                let r = int_arg(&args[0], "Las filas")?;
                let c = args.get(1).map(|a| int_arg(a, "Las columnas")).transpose()?.unwrap_or(r);
                if !(1..=500).contains(&r) || !(1..=500).contains(&c) {
                    return Err("Dimensión fuera de rango".into());
                }
                let v = if name == "zeros" { Expr::zero() } else { Expr::one() };
                Ok(Expr::Matrix(vec![vec![v; c as usize]; r as usize]))
            }
            "size" => match &args[0] {
                Expr::Matrix(m) => {
                    let (r, c) = matrix::dims(m);
                    Ok(Expr::List(vec![Expr::int(r as i128), Expr::int(c as i128)]))
                }
                Expr::List(v) => Ok(Expr::int(v.len() as i128)),
                _ => Ok(Expr::int(1)),
            },
            "dot" => {
                nargs(name, &args, 2, 2)?;
                let a = as_vector(&args[0]).ok_or("dot requiere vectores")?;
                let b = as_vector(&args[1]).ok_or("dot requiere vectores")?;
                matrix::dot(&a, &b)
            }
            "cross" => {
                nargs(name, &args, 2, 2)?;
                let a = as_vector(&args[0]).ok_or("cross requiere vectores")?;
                let b = as_vector(&args[1]).ok_or("cross requiere vectores")?;
                let c = matrix::cross(&a, &b)?;
                Ok(if args[0].is_list() { Expr::List(c) } else { Expr::Matrix(vec![c]) })
            }
            "norm" => {
                nargs(name, &args, 1, 1)?;
                if let Some(v) = as_vector(&args[0]) {
                    let sq: Vec<Expr> = v.iter().map(|x| pow(x.clone(), Expr::int(2))).collect::<Result<_, _>>()?;
                    return pow(add(sq)?, Expr::rat(1, 2));
                }
                // Frobenius norm
                let m = need_m(0)?;
                let sq: Vec<Expr> = m.iter().flatten().map(|x| pow(x.clone(), Expr::int(2))).collect::<Result<_, _>>()?;
                pow(add(sq)?, Expr::rat(1, 2))
            }
            "charpoly" => {
                let m = need_m(0)?;
                let var = args.get(1).and_then(|v| v.sym_name()).unwrap_or("x").to_string();
                let cs = matrix::charpoly(m)?;
                let n = cs.len() - 1;
                let terms: Vec<Expr> = cs.into_iter().enumerate().map(|(k, c)| mul(vec![c, pow(Expr::sym(&var), Expr::int((n - k) as i128))?])).collect::<Result<_, _>>()?;
                add(terms)
            }
            "eigvals" => {
                let m = need_m(0)?;
                let cs = matrix::charpoly(m)?;
                let var = "λ";
                let n = cs.len() - 1;
                let terms: Vec<Expr> = cs.into_iter().enumerate().map(|(k, c)| mul(vec![c, pow(Expr::sym(var), Expr::int((n - k) as i128))?])).collect::<Result<_, _>>()?;
                let p = add(terms)?;
                let sols = crate::solve::solve_poly_values(self, &p, var, true)?;
                Ok(Expr::List(sols))
            }
            "eigvecs" => {
                let m = need_m(0)?;
                let vals = match self.matrix_cmd("eigvals", args.clone())? {
                    Expr::List(v) => v,
                    _ => vec![],
                };
                let n = m.len();
                let mut out = Vec::new();
                let mut seen: Vec<Expr> = Vec::new();
                for l in vals {
                    if seen.contains(&l) {
                        continue;
                    }
                    seen.push(l.clone());
                    let li = matrix::map_m(&matrix::identity(n), |e| mul(vec![e.clone(), l.clone()]))?;
                    let neg_li = matrix::map_m(&li, |e| neg(e.clone()))?;
                    let a = matrix::add_m(m, &neg_li)?;
                    let a = if l.contains_float() { matrix::map_m(&a, |e| self.approx(e))? } else { a };
                    let ns = matrix::nullspace(&a)?;
                    let ns = if ns.is_empty() && l.contains_float() {
                        // numerical tolerance: drop last row of rref
                        let mut red = matrix::rref(&a)?.0;
                        red.pop();
                        matrix::nullspace(&red)?
                    } else {
                        ns
                    };
                    for v in ns {
                        out.push(Expr::List(vec![l.clone(), Expr::Matrix(v.into_iter().map(|x| vec![x]).collect())]));
                    }
                }
                Ok(Expr::List(out))
            }
            "augment" => {
                nargs(name, &args, 2, 2)?;
                let a = need_m(0)?;
                let b = need_m(1)?;
                if a.len() != b.len() {
                    return Err("augment requiere el mismo número de filas".into());
                }
                Ok(Expr::Matrix(a.iter().zip(b).map(|(x, y)| x.iter().chain(y).cloned().collect()).collect()))
            }
            "index" => {
                let idx: Vec<i128> = args[1..].iter().map(|a| int_arg(a, "El índice")).collect::<Result<_, _>>()?;
                match (&args[0], idx.as_slice()) {
                    (Expr::List(v), [k]) => v.get((*k - 1) as usize).cloned().ok_or_else(|| "Índice fuera de rango".into()),
                    (Expr::Matrix(m), [i, j]) => m.get((*i - 1) as usize).and_then(|r| r.get((*j - 1) as usize)).cloned().ok_or_else(|| "Índice fuera de rango".into()),
                    (Expr::Matrix(m), [k]) => {
                        if m.len() == 1 {
                            m[0].get((*k - 1) as usize).cloned().ok_or_else(|| "Índice fuera de rango".into())
                        } else if m.iter().all(|r| r.len() == 1) {
                            m.get((*k - 1) as usize).map(|r| r[0].clone()).ok_or_else(|| "Índice fuera de rango".into())
                        } else {
                            m.get((*k - 1) as usize).map(|r| Expr::Matrix(vec![r.clone()])).ok_or_else(|| "Índice fuera de rango".into())
                        }
                    }
                    (Expr::Sym(_), _) => Ok(Expr::Call("index".into(), args.clone())),
                    _ => Err("Índice no válido".into()),
                }
            }
            "lu" => {
                let m = need_m(0)?;
                let n = m.len();
                if n == 0 || m[0].len() != n {
                    return Err("lu requiere una matriz cuadrada".into());
                }
                let mut l = matrix::identity(n);
                let mut u = m.clone();
                for k in 0..n {
                    if u[k][k].is_zero() {
                        return Err("LU sin pivoteo no existe (pivote cero)".into());
                    }
                    for i in k + 1..n {
                        let f = div(u[i][k].clone(), u[k][k].clone())?;
                        l[i][k] = f.clone();
                        for j in k..n {
                            u[i][j] = sub(u[i][j].clone(), mul(vec![f.clone(), u[k][j].clone()])?)?;
                        }
                    }
                }
                Ok(Expr::List(vec![Expr::Matrix(l), Expr::Matrix(u)]))
            }
            _ => Err(format!("Comando de matriz desconocido: {name}")),
        }
    }
}

/// Formats an approximate value for display.
pub fn fmt_number(e: &Expr, digits: usize) -> String {
    match e {
        Expr::Float(f) => fmt_f64(*f, digits),
        _ => crate::expr::Printer { sig: digits }.print(e),
    }
}
