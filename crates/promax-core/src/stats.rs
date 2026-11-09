//! Statistics, regression, distributions, error analysis.
//!
//! Commands keep results EXACT when the data is exact (rationals, surds,
//! symbols) and the list is not too long; otherwise they fall back to `f64`.
use crate::eval::Cas;
use crate::expr::{Expr, Printer};
use crate::matrix::as_vector;
use crate::num::fmt_f64;
use crate::simplify::{add, div, expand, mul, neg, pow, sub, R};
use std::cmp::Ordering;
use std::f64::consts::PI;

/// Lists longer than this are processed in floating point.
const EXACT_MAX: usize = 2000;

/// One-variable statistics summary.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OneVar {
    pub n: usize,
    pub sum: f64,
    pub sum_sq: f64,
    pub mean: f64,
    /// Sample variance (n - 1).
    pub sample_var: f64,
    /// Population variance (n).
    pub pop_var: f64,
    pub sample_sd: f64,
    pub pop_sd: f64,
    /// Standard error of the mean s/√n.
    pub sem: f64,
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
    pub mode: Vec<f64>,
}

fn median_sorted(v: &[f64]) -> f64 {
    let n = v.len();
    if n == 0 {
        return f64::NAN;
    }
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// (lower half, upper half) excluding the median for odd n (Casio/TI method).
fn halves<T>(v: &[T]) -> (&[T], &[T]) {
    let n = v.len();
    if n < 2 {
        return (v, v);
    }
    let h = n / 2;
    (&v[..h], &v[n - h..])
}

/// Summary statistics of a data set (sample and population variants).
/// Quartiles use the median of the lower/upper halves, excluding the median
/// itself when n is odd (same method as Casio and TI calculators).
pub fn one_var(xs: &[f64]) -> Result<OneVar, String> {
    let n = xs.len();
    if n == 0 {
        return Err("La lista de datos está vacía".into());
    }
    if xs.iter().any(|x| !x.is_finite()) {
        return Err("Los datos deben ser números finitos".into());
    }
    let nf = n as f64;
    let sum: f64 = xs.iter().sum();
    let sum_sq: f64 = xs.iter().map(|x| x * x).sum();
    let mean = sum / nf;
    let ss: f64 = xs.iter().map(|x| (x - mean) * (x - mean)).sum();
    let pop_var = ss / nf;
    let sample_var = if n > 1 { ss / (nf - 1.0) } else { f64::NAN };
    let mut s = xs.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let (lo, hi) = halves(&s);
    // mode: all values with the maximal frequency
    let mut best = 0usize;
    let mut mode = Vec::new();
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j < n && s[j] == s[i] {
            j += 1;
        }
        let c = j - i;
        if c > best {
            best = c;
            mode.clear();
        }
        if c == best {
            mode.push(s[i]);
        }
        i = j;
    }
    Ok(OneVar {
        n,
        sum,
        sum_sq,
        mean,
        sample_var,
        pop_var,
        sample_sd: sample_var.sqrt(),
        pop_sd: pop_var.sqrt(),
        sem: sample_var.sqrt() / nf.sqrt(),
        min: s[0],
        q1: median_sorted(lo),
        median: median_sorted(&s),
        q3: median_sorted(hi),
        max: s[n - 1],
        mode,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegKind {
    Linear,
    Quadratic,
    Cubic,
    Exponential,
    Logarithmic,
    Power,
}

impl RegKind {
    pub const ALL: [RegKind; 6] = [RegKind::Linear, RegKind::Quadratic, RegKind::Cubic, RegKind::Exponential, RegKind::Logarithmic, RegKind::Power];
    pub fn name(&self) -> &'static str {
        match self {
            RegKind::Linear => "Lineal  y = a·x + b",
            RegKind::Quadratic => "Cuadrática  y = a·x² + b·x + c",
            RegKind::Cubic => "Cúbica  y = a·x³ + b·x² + c·x + d",
            RegKind::Exponential => "Exponencial  y = a·e^(b·x)",
            RegKind::Logarithmic => "Logarítmica  y = a + b·ln(x)",
            RegKind::Power => "Potencia  y = a·x^b",
        }
    }
    /// The generic model, e.g. "y = a·x + b".
    pub fn model(&self) -> &'static str {
        let n = self.name();
        n.split_once("  ").map(|(_, m)| m).unwrap_or(n)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Regression {
    pub kind: RegKind,
    /// Coefficients in the order of `RegKind::name` (a, b, c, d).
    pub coeffs: Vec<f64>,
    /// Pearson correlation (linear-type fits), if meaningful.
    pub r: Option<f64>,
    pub r2: f64,
    /// Human readable, e.g. "y = 2x + 1".
    pub formula: String,
}

impl Regression {
    pub fn eval(&self, x: f64) -> f64 {
        let c = &self.coeffs;
        match self.kind {
            RegKind::Linear | RegKind::Quadratic | RegKind::Cubic => c.iter().fold(0.0, |acc, k| acc * x + k),
            RegKind::Exponential => c[0] * (c[1] * x).exp(),
            RegKind::Logarithmic => c[0] + c[1] * x.ln(),
            RegKind::Power => c[0] * x.powf(c[1]),
        }
    }
}

/// Simple least squares line through (xs, ys): (slope, intercept, r).
fn line_fit(xs: &[f64], ys: &[f64]) -> Result<(f64, f64, Option<f64>), String> {
    let n = xs.len() as f64;
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for (x, y) in xs.iter().zip(ys) {
        sxx += (x - mx) * (x - mx);
        sxy += (x - mx) * (y - my);
        syy += (y - my) * (y - my);
    }
    if sxx == 0.0 || sxx <= 1e-300 {
        return Err("Todos los valores de x son iguales: no hay regresión posible".into());
    }
    let a = sxy / sxx;
    let b = my - a * mx;
    let r = if syy > 0.0 { Some((sxy / (sxx * syy).sqrt()).clamp(-1.0, 1.0)) } else { None };
    Ok((a, b, r))
}

/// Solves a small dense system with Gaussian elimination (partial pivoting).
fn gauss_solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let piv = (col..n).max_by(|&i, &j| a[i][col].abs().partial_cmp(&a[j][col].abs()).unwrap_or(Ordering::Equal))?;
        if a[piv][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        for row in col + 1..n {
            let f = a[row][col] / a[col][col];
            for k in col..n {
                a[row][k] -= f * a[col][k];
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|k| a[i][k] * x[k]).sum();
        x[i] = (b[i] - s) / a[i][i];
    }
    Some(x)
}

/// Polynomial least squares of degree `deg`; returns coefficients highest first.
fn poly_fit(xs: &[f64], ys: &[f64], deg: usize) -> Result<Vec<f64>, String> {
    // fit in the standardized variable t = (x - m)/s for conditioning
    let n = xs.len() as f64;
    let m = xs.iter().sum::<f64>() / n;
    let s = (xs.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / n).sqrt();
    if s == 0.0 {
        return Err("Todos los valores de x son iguales: no hay regresión posible".into());
    }
    let k = deg + 1;
    let mut ata = vec![vec![0.0; k]; k];
    let mut aty = vec![0.0; k];
    for (x, y) in xs.iter().zip(ys) {
        let t = (x - m) / s;
        let pw: Vec<f64> = (0..2 * k).map(|i| t.powi(i as i32)).collect();
        for i in 0..k {
            for j in 0..k {
                ata[i][j] += pw[i + j];
            }
            aty[i] += pw[i] * y;
        }
    }
    let ct = gauss_solve(ata, aty).ok_or("Sistema singular: datos insuficientes para el ajuste")?;
    // ct[i] = coefficient of t^i; substitute t = (x - m)/s and expand.
    let mut cx = vec![0.0; k]; // cx[i] = coefficient of x^i
    for (i, c) in ct.iter().enumerate() {
        // c·((x - m)/s)^i = c/s^i · Σ_j C(i,j) x^j (-m)^(i-j)
        let f = c / s.powi(i as i32);
        let mut binom = 1.0;
        for j in 0..=i {
            cx[j] += f * binom * (-m).powi((i - j) as i32);
            binom = binom * (i - j) as f64 / (j + 1) as f64;
        }
    }
    cx.reverse();
    Ok(cx)
}

/// Formats a coefficient times a term (`suffix` empty for a constant), as the
/// first term (`first`) or as a continuation (with " + " / " - ").
fn term(c: f64, suffix: &str, first: bool) -> String {
    let neg = c < 0.0;
    let mag = fmt_f64(c.abs(), 6);
    let body = if suffix.is_empty() {
        mag
    } else if mag == "1" {
        suffix.trim_start_matches('·').to_string()
    } else {
        format!("{mag}{suffix}")
    };
    match (first, neg) {
        (true, true) => format!("-{body}"),
        (true, false) => body,
        (false, true) => format!(" - {body}"),
        (false, false) => format!(" + {body}"),
    }
}

fn poly_formula(c: &[f64]) -> String {
    let deg = c.len() - 1;
    let scale = c.iter().fold(0.0f64, |m, k| m.max(k.abs()));
    let mut s = String::new();
    for (i, &k) in c.iter().enumerate() {
        let p = deg - i;
        if k.abs() <= 1e-10 * scale || fmt_f64(k.abs(), 6) == "0" {
            continue;
        }
        let suf = match p {
            0 => "",
            1 => "x",
            2 => "x²",
            _ => "x³",
        };
        s.push_str(&term(k, suf, s.is_empty()));
    }
    if s.is_empty() {
        s.push('0');
    }
    format!("y = {s}")
}

fn formula(kind: RegKind, c: &[f64]) -> String {
    match kind {
        RegKind::Linear | RegKind::Quadratic | RegKind::Cubic => poly_formula(c),
        RegKind::Exponential => {
            let ex = term(c[1], "x", true);
            format!("y = {}·e^({})", fmt_f64(c[0], 6), ex)
        }
        RegKind::Logarithmic => {
            let a = fmt_f64(c[0], 6);
            if a == "0" {
                format!("y = {}", term(c[1], "·ln(x)", true))
            } else {
                format!("y = {}{}", a, term(c[1], "·ln(x)", false))
            }
        }
        RegKind::Power => format!("y = {}·x^{}", fmt_f64(c[0], 6), fmt_f64(c[1], 6)),
    }
}

/// Least-squares regression. Exponential, logarithmic and power fits are done
/// by linearization (they require positive data where a logarithm is taken).
pub fn regress(kind: RegKind, xs: &[f64], ys: &[f64]) -> Result<Regression, String> {
    if xs.len() != ys.len() {
        return Err("Las listas x e y deben tener la misma longitud".into());
    }
    if xs.iter().chain(ys).any(|v| !v.is_finite()) {
        return Err("Los datos deben ser números finitos".into());
    }
    let need = match kind {
        RegKind::Quadratic => 3,
        RegKind::Cubic => 4,
        _ => 2,
    };
    if xs.len() < need {
        return Err(format!("Se necesitan al menos {need} puntos para esta regresión"));
    }
    let pos = |v: &[f64], what: &str| -> Result<Vec<f64>, String> {
        if v.iter().any(|&t| t <= 0.0) {
            return Err(format!("La regresión {} requiere valores de {what} positivos", kind_adj(kind)));
        }
        Ok(v.iter().map(|t| t.ln()).collect())
    };
    let (coeffs, r) = match kind {
        RegKind::Linear => {
            let (a, b, r) = line_fit(xs, ys)?;
            (vec![a, b], r)
        }
        RegKind::Quadratic => (poly_fit(xs, ys, 2)?, None),
        RegKind::Cubic => (poly_fit(xs, ys, 3)?, None),
        RegKind::Exponential => {
            let ly = pos(ys, "y")?;
            let (b, la, r) = line_fit(xs, &ly)?;
            (vec![la.exp(), b], r)
        }
        RegKind::Logarithmic => {
            let lx = pos(xs, "x")?;
            let (b, a, r) = line_fit(&lx, ys)?;
            (vec![a, b], r)
        }
        RegKind::Power => {
            let lx = pos(xs, "x")?;
            let ly = pos(ys, "y")?;
            let (b, la, r) = line_fit(&lx, &ly)?;
            (vec![la.exp(), b], r)
        }
    };
    let mut reg = Regression { kind, formula: formula(kind, &coeffs), coeffs, r, r2: 0.0 };
    reg.r2 = match kind {
        RegKind::Linear | RegKind::Quadratic | RegKind::Cubic => {
            let my = ys.iter().sum::<f64>() / ys.len() as f64;
            let sst: f64 = ys.iter().map(|y| (y - my) * (y - my)).sum();
            let sse: f64 = xs.iter().zip(ys).map(|(x, y)| (y - reg.eval(*x)).powi(2)).sum();
            if sst == 0.0 || sse <= 1e-24 * sst.max(1e-300) {
                1.0
            } else {
                (1.0 - sse / sst).clamp(0.0, 1.0)
            }
        }
        // linearized fits: r² of the linearized data (as calculators report)
        _ => r.map(|r| r * r).unwrap_or(1.0),
    };
    Ok(reg)
}

fn kind_adj(k: RegKind) -> &'static str {
    match k {
        RegKind::Exponential => "exponencial",
        RegKind::Logarithmic => "logarítmica",
        RegKind::Power => "potencial",
        RegKind::Linear => "lineal",
        RegKind::Quadratic => "cuadrática",
        RegKind::Cubic => "cúbica",
    }
}

// ====================================================================== distributions

/// erfc(x) for x ≥ 0, accurate in the tail.
fn erfc_pos(x: f64) -> f64 {
    if x < 2.5 {
        return 1.0 - crate::num::erf(x);
    }
    if x.is_infinite() {
        return 0.0;
    }
    let mut f = 0.0;
    for k in (1..80).rev() {
        f = (k as f64 / 2.0) / (x + f);
    }
    (-x * x).exp() / PI.sqrt() / (x + f)
}

/// Standard normal CDF Φ(z).
pub fn norm_cdf(z: f64) -> f64 {
    if z.is_nan() {
        return f64::NAN;
    }
    let t = z / std::f64::consts::SQRT_2;
    if z < 0.0 {
        0.5 * erfc_pos(-t)
    } else {
        1.0 - 0.5 * erfc_pos(t)
    }
}

/// Standard normal density φ(z).
pub fn norm_pdf(z: f64) -> f64 {
    (-0.5 * z * z).exp() / (2.0 * PI).sqrt()
}

/// Inverse standard normal CDF (Acklam's rational approximation refined
/// with Halley steps).
pub fn inv_norm(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    const A: [f64; 6] = [-3.969683028665376e1, 2.209460984245205e2, -2.759285104469687e2, 1.383577518672690e2, -3.066479806614716e1, 2.506628277459239];
    const B: [f64; 5] = [-5.447609879822406e1, 1.615858368580409e2, -1.556989798598866e2, 6.680131188771972e1, -1.328068155288572e1];
    const C: [f64; 6] = [-7.784894002430293e-3, -3.223964580411365e-1, -2.400758277161838, -2.549732539343734, 4.374664141464968, 2.938163982698783];
    const D: [f64; 4] = [7.784695709041462e-3, 3.224671290700398e-1, 2.445134137142996, 3.754408661907416];
    let pl = 0.02425;
    let mut x = if p < pl {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5]) / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - pl {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5]) / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    for _ in 0..2 {
        // work on the smaller tail for accuracy
        // e = Φ(x) − p, using Φ(x) = 1 − Q(x) for x ≥ 0
        let e = if x < 0.0 { norm_cdf(x) - p } else { (1.0 - p) - 0.5 * erfc_pos(x / std::f64::consts::SQRT_2) };
        let u = e * (2.0 * PI).sqrt() * (x * x / 2.0).exp();
        if !u.is_finite() {
            break;
        }
        x -= u / (1.0 + x * u / 2.0);
    }
    x
}

fn ln_choose(n: f64, k: f64) -> f64 {
    let k = k.min(n - k);
    let mut s = 0.0;
    let mut i = 1.0;
    while i <= k {
        s += ((n - k + i) / i).ln();
        i += 1.0;
    }
    s
}

fn binom_pmf_f(n: f64, p: f64, k: f64) -> f64 {
    if k < 0.0 || k > n || k.fract() != 0.0 {
        return 0.0;
    }
    if p == 0.0 {
        return if k == 0.0 { 1.0 } else { 0.0 };
    }
    if p == 1.0 {
        return if k == n { 1.0 } else { 0.0 };
    }
    (ln_choose(n, k) + k * p.ln() + (n - k) * (1.0 - p).ln()).exp()
}

fn ln_fact(k: f64) -> f64 {
    let mut s = 0.0;
    let mut i = 2.0;
    while i <= k {
        s += (i as f64).ln();
        i += 1.0;
    }
    s
}

fn poisson_pmf_f(l: f64, k: f64) -> f64 {
    if k < 0.0 || k.fract() != 0.0 {
        return 0.0;
    }
    (-l + k * l.ln() - ln_fact(k)).exp()
}

// ====================================================================== command helpers

fn printer(cas: &Cas) -> Printer {
    Printer { sig: cas.s.digits }
}

/// Exact form, plus " ≈ decimal" when it is not an integer / float.
fn show(cas: &Cas, e: &Expr) -> String {
    let s = printer(cas).print(e);
    let simple = matches!(e, Expr::Float(_)) || matches!(e, Expr::Num(r) if r.is_int());
    if !simple && !e.has_free_symbols() {
        if let Ok(v) = cas.num_real(e) {
            let a = fmt_f64(v, cas.s.digits);
            if a != s {
                return format!("{s} ≈ {a}");
            }
        }
    }
    s
}

fn list_arg(e: &Expr, name: &str) -> Result<Vec<Expr>, String> {
    match e {
        Expr::List(v) => Ok(v.clone()),
        Expr::Matrix(_) => as_vector(e).ok_or_else(|| format!("{name}: se esperaba una lista o vector")),
        _ => Err(format!("{name}: se esperaba una lista, por ejemplo {{1, 2, 3}}")),
    }
}

fn nonempty(v: &[Expr], name: &str) -> Result<(), String> {
    if v.is_empty() {
        return Err(format!("{name}: la lista está vacía"));
    }
    Ok(())
}

fn floats(cas: &Cas, v: &[Expr], name: &str) -> Result<Vec<f64>, String> {
    v.iter()
        .map(|x| {
            if x.has_free_symbols() {
                return Err(format!("{name} requiere datos numéricos"));
            }
            cas.num_real(x).map_err(|_| format!("{name} requiere datos reales"))
        })
        .collect()
}

fn is_exact(v: &[Expr]) -> bool {
    v.len() <= EXACT_MAX
}

fn fl(x: f64) -> Expr {
    Expr::Float(x)
}

/// Expands symbolic results so that e.g. mean({a,b}) = a/2 + b/2.
fn tidy(e: Expr) -> R {
    if e.has_free_symbols() {
        expand(&e)
    } else {
        Ok(e)
    }
}

/// Validates a frequency list and returns it.
fn freq_arg(cas: &Cas, e: &Expr, n: usize, name: &str) -> Result<Vec<Expr>, String> {
    let w = list_arg(e, name)?;
    if w.len() != n {
        return Err(format!("{name}: la lista de frecuencias debe tener la misma longitud que los datos"));
    }
    for x in &w {
        if !x.has_free_symbols() {
            if let Ok(v) = cas.num_real(x) {
                if v < 0.0 {
                    return Err(format!("{name}: las frecuencias no pueden ser negativas"));
                }
            }
        }
    }
    Ok(w)
}

/// Repeats each value according to its (integer) frequency.
fn expand_freq(xs: &[Expr], ws: &[Expr], name: &str) -> Result<Vec<Expr>, String> {
    let mut out = Vec::new();
    for (x, w) in xs.iter().zip(ws) {
        let k = w.as_int().filter(|k| *k >= 0).ok_or_else(|| format!("{name}: las frecuencias deben ser enteros no negativos"))?;
        if out.len() as i128 + k > 1_000_000 {
            return Err(format!("{name}: demasiados datos"));
        }
        for _ in 0..k {
            out.push(x.clone());
        }
    }
    Ok(out)
}

/// Data list plus optional frequency list (ClassPad style `cmd(list, freq)`).
fn data(cas: &Cas, name: &str, args: &[Expr]) -> Result<(Vec<Expr>, Option<Vec<Expr>>), String> {
    if args.is_empty() || args.len() > 2 {
        return Err(format!("{name}(lista) o {name}(lista, frecuencias)"));
    }
    let xs = list_arg(&args[0], name)?;
    nonempty(&xs, name)?;
    let ws = match args.get(1) {
        Some(w) => Some(freq_arg(cas, w, xs.len(), name)?),
        None => None,
    };
    Ok((xs, ws))
}

/// Data with frequencies expanded (for order statistics).
fn data_expanded(cas: &Cas, name: &str, args: &[Expr]) -> Result<Vec<Expr>, String> {
    let (xs, ws) = data(cas, name, args)?;
    let v = match ws {
        Some(w) => expand_freq(&xs, &w, name)?,
        None => xs,
    };
    nonempty(&v, name)?;
    Ok(v)
}

/// Sorts exact values by numeric value.
fn sorted_exact(cas: &Cas, v: &[Expr], name: &str) -> Result<Vec<Expr>, String> {
    let f = floats(cas, v, name)?;
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|&i, &j| f[i].partial_cmp(&f[j]).unwrap_or(Ordering::Equal));
    Ok(idx.into_iter().map(|i| v[i].clone()).collect())
}

fn median_exact(v: &[Expr]) -> R {
    let n = v.len();
    if n % 2 == 1 {
        Ok(v[n / 2].clone())
    } else {
        div(add(vec![v[n / 2 - 1].clone(), v[n / 2].clone()])?, Expr::int(2))
    }
}

/// (N, Σw·x) for a weighted data set.
fn weighted_sum(xs: &[Expr], ws: &Option<Vec<Expr>>) -> Result<(Expr, Expr), String> {
    Ok(match ws {
        None => (Expr::int(xs.len() as i128), add(xs.to_vec())?),
        Some(w) => (add(w.clone())?, add(xs.iter().zip(w).map(|(x, w)| mul(vec![x.clone(), w.clone()])).collect::<Result<_, _>>()?)?),
    })
}

fn mean_e(xs: &[Expr], ws: &Option<Vec<Expr>>) -> R {
    let (n, s) = weighted_sum(xs, ws)?;
    if n.is_zero() {
        return Err("La suma de las frecuencias es cero".into());
    }
    tidy(div(s, n)?)
}

/// Variance (sample if `sample`), exact.
fn var_e(xs: &[Expr], ws: &Option<Vec<Expr>>, sample: bool) -> R {
    let (n, _) = weighted_sum(xs, ws)?;
    let m = mean_e(xs, ws)?;
    let mut terms = Vec::with_capacity(xs.len());
    for (i, x) in xs.iter().enumerate() {
        let d2 = pow(sub(x.clone(), m.clone())?, Expr::int(2))?;
        terms.push(match ws {
            Some(w) => mul(vec![w[i].clone(), d2])?,
            None => d2,
        });
    }
    let ss = tidy(add(terms)?)?;
    let den = if sample { sub(n, Expr::one())? } else { n };
    if let Some(d) = den.as_f64() {
        if d <= 0.0 {
            return Err(if sample { "La varianza muestral requiere al menos 2 datos".into() } else { "La suma de las frecuencias es cero".into() });
        }
    }
    tidy(div(ss, den)?)
}

/// Float (N, mean, Σw(x-m)²).
fn wstats_f(cas: &Cas, xs: &[Expr], ws: &Option<Vec<Expr>>, name: &str) -> Result<(f64, f64, f64), String> {
    let x = floats(cas, xs, name)?;
    let w = match ws {
        Some(w) => floats(cas, w, name)?,
        None => vec![1.0; x.len()],
    };
    let n: f64 = w.iter().sum();
    if n <= 0.0 {
        return Err("La suma de las frecuencias es cero".into());
    }
    let m = x.iter().zip(&w).map(|(a, b)| a * b).sum::<f64>() / n;
    let ss = x.iter().zip(&w).map(|(a, b)| b * (a - m) * (a - m)).sum::<f64>();
    Ok((n, m, ss))
}

fn var_any(cas: &Cas, xs: &[Expr], ws: &Option<Vec<Expr>>, sample: bool, name: &str) -> R {
    if is_exact(xs) {
        return var_e(xs, ws, sample);
    }
    let (n, _, ss) = wstats_f(cas, xs, ws, name)?;
    let d = if sample { n - 1.0 } else { n };
    if d <= 0.0 {
        return Err("La varianza muestral requiere al menos 2 datos".into());
    }
    Ok(fl(ss / d))
}

fn sqrt_e(e: Expr) -> R {
    pow(e, Expr::rat(1, 2))
}

fn two_lists(name: &str, args: &[Expr]) -> Result<(Vec<Expr>, Vec<Expr>), String> {
    if args.len() < 2 {
        return Err(format!("{name}(listaX, listaY)"));
    }
    let xs = list_arg(&args[0], name)?;
    let ys = list_arg(&args[1], name)?;
    if xs.len() != ys.len() {
        return Err(format!("{name}: las listas deben tener la misma longitud"));
    }
    if xs.len() < 2 {
        return Err(format!("{name}: se necesitan al menos 2 puntos"));
    }
    Ok((xs, ys))
}

/// Paired data with an optional frequency list (expanded).
fn paired(name: &str, args: &[Expr]) -> Result<(Vec<Expr>, Vec<Expr>), String> {
    if args.len() > 3 {
        return Err(format!("{name}(listaX, listaY[, frecuencias])"));
    }
    let (xs, ys) = two_lists(name, args)?;
    match args.get(2) {
        None => Ok((xs, ys)),
        Some(w) => {
            let w = list_arg(w, name)?;
            if w.len() != xs.len() {
                return Err(format!("{name}: la lista de frecuencias debe tener la misma longitud que los datos"));
            }
            Ok((expand_freq(&xs, &w, name)?, expand_freq(&ys, &w, name)?))
        }
    }
}

/// Exact sums n, Σx, Σy, Σx², Σy², Σxy.
struct Sums {
    n: Expr,
    sx: Expr,
    sy: Expr,
    sxx: Expr,
    syy: Expr,
    sxy: Expr,
}

fn sums(xs: &[Expr], ys: &[Expr]) -> Result<Sums, String> {
    let sq = |v: &[Expr]| -> R { add(v.iter().map(|x| pow(x.clone(), Expr::int(2))).collect::<Result<_, _>>()?) };
    Ok(Sums {
        n: Expr::int(xs.len() as i128),
        sx: add(xs.to_vec())?,
        sy: add(ys.to_vec())?,
        sxx: sq(xs)?,
        syy: sq(ys)?,
        sxy: add(xs.iter().zip(ys).map(|(x, y)| mul(vec![x.clone(), y.clone()])).collect::<Result<_, _>>()?)?,
    })
}

impl Sums {
    /// n·Σxy − Σx·Σy
    fn nsxy(&self) -> R {
        tidy(sub(mul(vec![self.n.clone(), self.sxy.clone()])?, mul(vec![self.sx.clone(), self.sy.clone()])?)?)
    }
    fn nsxx(&self) -> R {
        tidy(sub(mul(vec![self.n.clone(), self.sxx.clone()])?, pow(self.sx.clone(), Expr::int(2))?)?)
    }
    fn nsyy(&self) -> R {
        tidy(sub(mul(vec![self.n.clone(), self.syy.clone()])?, pow(self.sy.clone(), Expr::int(2))?)?)
    }
    fn slope(&self) -> R {
        let d = self.nsxx()?;
        if d.is_zero() {
            return Err("Todos los valores de x son iguales: pendiente indefinida".into());
        }
        tidy(div(self.nsxy()?, d)?)
    }
    fn intercept(&self) -> R {
        let a = self.slope()?;
        tidy(div(sub(self.sy.clone(), mul(vec![a, self.sx.clone()])?)?, self.n.clone())?)
    }
}

fn reg_kind(name: &str) -> RegKind {
    match name {
        "quadreg" => RegKind::Quadratic,
        "cubicreg" => RegKind::Cubic,
        "expreg" => RegKind::Exponential,
        "logreg" => RegKind::Logarithmic,
        "powreg" => RegKind::Power,
        _ => RegKind::Linear,
    }
}

fn reg_cmd(cas: &Cas, name: &str, args: &[Expr]) -> R {
    let (xs, ys) = paired(name, args)?;
    let kind = reg_kind(name);
    let xf = floats(cas, &xs, name)?;
    let yf = floats(cas, &ys, name)?;
    let reg = regress(kind, &xf, &yf)?;
    let letters = ["a", "b", "c", "d"];
    let mut lines = vec![kind.model().to_string()];
    let exact = kind == RegKind::Linear && is_exact(&xs) && !xs.iter().chain(&ys).any(|e| e.contains_float());
    if exact {
        let s = sums(&xs, &ys)?;
        lines.push(format!("a = {}", show(cas, &s.slope()?)));
        lines.push(format!("b = {}", show(cas, &s.intercept()?)));
    } else {
        let scale = reg.coeffs.iter().fold(0.0f64, |m, c| m.max(c.abs()));
        for (l, c) in letters.iter().zip(&reg.coeffs) {
            let c = if c.abs() < 1e-13 * scale { 0.0 } else { *c };
            lines.push(format!("{l} = {}", fmt_f64(c, cas.s.digits)));
        }
    }
    if let Some(r) = reg.r {
        lines.push(format!("r = {}", fmt_f64(r, 6)));
    }
    lines.push(format!("r² = {}", fmt_f64(reg.r2, 6)));
    lines.push(reg.formula.clone());
    Ok(Expr::Text(lines.join("\n")))
}

fn num_arg(cas: &Cas, e: &Expr, what: &str) -> Result<f64, String> {
    if e.has_free_symbols() {
        return Err(format!("{what} debe ser numérico"));
    }
    cas.num_real(e).map_err(|_| format!("{what} debe ser un número real"))
}

fn mu_sigma(cas: &Cas, args: &[Expr], from: usize) -> Result<(Expr, Expr), String> {
    let mu = args.get(from).cloned().unwrap_or_else(Expr::zero);
    let sigma = args.get(from + 1).cloned().unwrap_or_else(Expr::one);
    if !sigma.has_free_symbols() && num_arg(cas, &sigma, "σ")? <= 0.0 {
        return Err("La desviación estándar σ debe ser positiva".into());
    }
    Ok((mu, sigma))
}

/// Applies `f` elementwise when `x` is a list.
fn map_list(x: &Expr, mut f: impl FnMut(&Expr) -> R) -> R {
    match x {
        Expr::List(v) => Ok(Expr::List(v.iter().map(&mut f).collect::<Result<_, _>>()?)),
        other => f(other),
    }
}

fn binom_args(cas: &Cas, name: &str, args: &[Expr]) -> Result<(i128, Expr), String> {
    let n = args[0].as_int().filter(|n| *n >= 0).ok_or_else(|| format!("{name}: n debe ser un entero no negativo"))?;
    let p = args[1].clone();
    if !p.has_free_symbols() {
        let pv = num_arg(cas, &p, "p")?;
        if !(0.0..=1.0).contains(&pv) {
            return Err(format!("{name}: la probabilidad p debe estar entre 0 y 1"));
        }
    }
    Ok((n, p))
}

fn binom_pmf_e(cas: &Cas, n: i128, p: &Expr, k: i128) -> R {
    if k < 0 || k > n {
        return Ok(Expr::zero());
    }
    if n > 200 || p.contains_float() {
        let pv = num_arg(cas, p, "p")?;
        return Ok(fl(binom_pmf_f(n as f64, pv, k as f64)));
    }
    let c = cas.apply("ncr", vec![Expr::int(n), Expr::int(k)])?;
    mul(vec![c, pow(p.clone(), Expr::int(k))?, pow(sub(Expr::one(), p.clone())?, Expr::int(n - k))?])
}

fn binom_cdf_e(cas: &Cas, n: i128, p: &Expr, lo: i128, hi: i128) -> R {
    let lo = lo.max(0);
    let hi = hi.min(n);
    if hi < lo {
        return Ok(Expr::zero());
    }
    if n > 200 || p.contains_float() {
        let pv = num_arg(cas, p, "p")?;
        let s: f64 = (lo..=hi).map(|k| binom_pmf_f(n as f64, pv, k as f64)).sum();
        return Ok(fl(s.min(1.0)));
    }
    let terms = (lo..=hi).map(|k| binom_pmf_e(cas, n, p, k)).collect::<Result<Vec<_>, _>>()?;
    tidy(add(terms)?)
}

fn kint(cas: &Cas, e: &Expr, name: &str, floor: bool) -> Result<Option<i128>, String> {
    if let Some(k) = e.as_int() {
        return Ok(Some(k));
    }
    let v = num_arg(cas, e, &format!("{name}: k"))?;
    if floor {
        if v.is_infinite() {
            return Ok(Some(if v > 0.0 { i64::MAX as i128 } else { -1 }));
        }
        Ok(Some(v.floor() as i128))
    } else {
        Ok(None)
    }
}

fn poisson_lambda(cas: &Cas, name: &str, l: &Expr) -> Result<(), String> {
    if !l.has_free_symbols() && num_arg(cas, l, "λ")? <= 0.0 {
        return Err(format!("{name}: λ debe ser positivo"));
    }
    Ok(())
}

fn poisson_exact_ok(l: &Expr, k: i128) -> bool {
    !l.contains_float() && (0..=30).contains(&k)
}

fn poisson_cdf(cas: &Cas, l: &Expr, k: i128) -> R {
    if k < 0 {
        return Ok(Expr::zero());
    }
    if poisson_exact_ok(l, k) {
        // e^(-λ)·Σ λ^i/i!
        let mut terms = Vec::new();
        let mut fact: i128 = 1;
        for i in 0..=k {
            if i > 1 {
                fact *= i;
            }
            terms.push(div(pow(l.clone(), Expr::int(i))?, Expr::int(fact))?);
        }
        return mul(vec![pow(Expr::sym("e"), neg(l.clone())?)?, add(terms)?]);
    }
    let lv = num_arg(cas, l, "λ")?;
    let mut s = 0.0;
    let mut lt = -lv; // ln term
    for i in 0..=k.min(10_000_000) {
        if i > 0 {
            lt += lv.ln() - (i as f64).ln();
        }
        s += lt.exp();
        if i as f64 > lv && lt < -745.0 {
            break;
        }
    }
    Ok(fl(s.min(1.0)))
}

fn err_pair(name: &str, args: &[Expr], mut f: impl FnMut(&Expr, &Expr) -> R) -> R {
    if args.len() != 2 {
        return Err(format!("{name}(aproximado, exacto)"));
    }
    match (&args[0], &args[1]) {
        (Expr::List(a), Expr::List(b)) => {
            if a.len() != b.len() {
                return Err(format!("{name}: las listas deben tener la misma longitud"));
            }
            Ok(Expr::List(a.iter().zip(b).map(|(x, y)| f(x, y)).collect::<Result<_, _>>()?))
        }
        (Expr::List(a), e) => Ok(Expr::List(a.iter().map(|x| f(x, e)).collect::<Result<_, _>>()?)),
        (a, Expr::List(b)) => Ok(Expr::List(b.iter().map(|y| f(a, y)).collect::<Result<_, _>>()?)),
        (a, b) => f(a, b),
    }
}

fn arity(name: &str, args: &[Expr], lo: usize, hi: usize, usage: &str) -> Result<(), String> {
    if args.len() < lo || args.len() > hi {
        return Err(format!("Uso: {name}{usage}"));
    }
    Ok(())
}

pub fn cmd(cas: &Cas, name: &str, args: Vec<Expr>) -> R {
    match name {
        "mean" => {
            let (xs, ws) = data(cas, name, &args)?;
            if is_exact(&xs) {
                return mean_e(&xs, &ws);
            }
            Ok(fl(wstats_f(cas, &xs, &ws, name)?.1))
        }
        "var" | "pvar" => {
            let (xs, ws) = data(cas, name, &args)?;
            var_any(cas, &xs, &ws, name == "var", name)
        }
        "stdev" | "pstdev" => {
            let (xs, ws) = data(cas, name, &args)?;
            sqrt_e(var_any(cas, &xs, &ws, name == "stdev", name)?)
        }
        "sem" => {
            let (xs, ws) = data(cas, name, &args)?;
            let v = var_any(cas, &xs, &ws, true, name)?;
            let (n, _) = weighted_sum(&xs, &ws)?;
            sqrt_e(div(v, n)?)
        }
        "count" => {
            let (xs, ws) = data(cas, name, &args)?;
            Ok(weighted_sum(&xs, &ws)?.0)
        }
        "median" | "q1" | "q3" | "iqr" | "range" => {
            let v = data_expanded(cas, name, &args)?;
            let s = sorted_exact(cas, &v, name)?;
            if !is_exact(&s) || s.iter().any(|e| e.contains_float()) {
                let f = floats(cas, &s, name)?;
                let (lo, hi) = halves(&f);
                return Ok(fl(match name {
                    "median" => median_sorted(&f),
                    "q1" => median_sorted(lo),
                    "q3" => median_sorted(hi),
                    "iqr" => median_sorted(hi) - median_sorted(lo),
                    _ => f[f.len() - 1] - f[0],
                }));
            }
            let (lo, hi) = halves(&s);
            match name {
                "median" => median_exact(&s),
                "q1" => median_exact(lo),
                "q3" => median_exact(hi),
                "iqr" => sub(median_exact(hi)?, median_exact(lo)?),
                _ => sub(s[s.len() - 1].clone(), s[0].clone()),
            }
        }
        "mode" => {
            let v = data_expanded(cas, name, &args)?;
            let s = sorted_exact(cas, &v, name)?;
            let f = floats(cas, &s, name)?;
            let mut best = 0;
            let mut modes: Vec<Expr> = Vec::new();
            let mut i = 0;
            while i < s.len() {
                let mut j = i;
                while j < s.len() && (s[j] == s[i] || f[j] == f[i]) {
                    j += 1;
                }
                if j - i > best {
                    best = j - i;
                    modes.clear();
                }
                if j - i == best {
                    modes.push(s[i].clone());
                }
                i = j;
            }
            Ok(if modes.len() == 1 { modes.pop().unwrap() } else { Expr::List(modes) })
        }
        "sort" | "sortd" => {
            arity(name, &args, 1, 1, "(lista)")?;
            let v = list_arg(&args[0], name)?;
            let mut s = sorted_exact(cas, &v, name)?;
            if name == "sortd" {
                s.reverse();
            }
            Ok(Expr::List(s))
        }
        "cumsum" => {
            arity(name, &args, 1, 1, "(lista)")?;
            let v = list_arg(&args[0], name)?;
            let mut acc = Expr::zero();
            let mut out = Vec::with_capacity(v.len());
            for x in v {
                acc = add(vec![acc, x])?;
                out.push(acc.clone());
            }
            Ok(Expr::List(out))
        }
        "cov" | "corr" | "slope" | "intercept" => {
            let (xs, ys) = paired(name, &args)?;
            if !is_exact(&xs) {
                let xf = floats(cas, &xs, name)?;
                let yf = floats(cas, &ys, name)?;
                let n = xf.len() as f64;
                let mx = xf.iter().sum::<f64>() / n;
                let my = yf.iter().sum::<f64>() / n;
                let sxy: f64 = xf.iter().zip(&yf).map(|(x, y)| (x - mx) * (y - my)).sum();
                return match name {
                    "cov" => Ok(fl(sxy / (n - 1.0))),
                    "corr" => {
                        let (_, _, r) = line_fit(&xf, &yf)?;
                        r.map(fl).ok_or_else(|| "corr: los valores de y son constantes".into())
                    }
                    _ => {
                        let (a, b, _) = line_fit(&xf, &yf)?;
                        Ok(fl(if name == "slope" { a } else { b }))
                    }
                };
            }
            let s = sums(&xs, &ys)?;
            match name {
                "cov" => {
                    // (nΣxy − ΣxΣy) / (n(n−1))
                    let n = xs.len() as i128;
                    tidy(div(s.nsxy()?, Expr::int(n * (n - 1)))?)
                }
                "corr" => {
                    let d = mul(vec![s.nsxx()?, s.nsyy()?])?;
                    if d.is_zero() {
                        return Err("corr: alguna de las variables es constante".into());
                    }
                    div(s.nsxy()?, sqrt_e(d)?)
                }
                "slope" => s.slope(),
                _ => s.intercept(),
            }
        }
        "linreg" | "quadreg" | "cubicreg" | "expreg" | "logreg" | "powreg" => reg_cmd(cas, name, &args),
        "normpdf" => {
            arity(name, &args, 1, 3, "(x[, μ, σ])")?;
            let (mu, sigma) = mu_sigma(cas, &args, 1)?;
            map_list(&args[0], |x| {
                if x.has_free_symbols() || mu.has_free_symbols() || sigma.has_free_symbols() {
                    // symbolic density (plottable): e^(-(x-μ)²/(2σ²)) / (σ√(2π))
                    let z = div(sub(x.clone(), mu.clone())?, sigma.clone())?;
                    let ex = mul(vec![Expr::rat(-1, 2), pow(z, Expr::int(2))?])?;
                    return mul(vec![pow(sigma.clone(), Expr::int(-1))?, pow(mul(vec![Expr::int(2), Expr::sym("pi")])?, Expr::rat(-1, 2))?, pow(Expr::sym("e"), ex)?]);
                }
                let (xv, m, s) = (num_arg(cas, x, "x")?, num_arg(cas, &mu, "μ")?, num_arg(cas, &sigma, "σ")?);
                Ok(fl(norm_pdf((xv - m) / s) / s))
            })
        }
        "normcdf" => {
            arity(name, &args, 1, 4, "(x) = P(X ≤ x), o normcdf(a, b[, μ, σ]) = P(a ≤ X ≤ b)")?;
            // 1 arg: Φ(x); 2 args: P(a≤Z≤b); 3 args: P(X≤x) with (μ, σ); 4 args: P(a≤X≤b)
            let (lo, hi, from) = match args.len() {
                1 => (None, &args[0], 1),
                2 => (Some(&args[0]), &args[1], 2),
                3 => (None, &args[0], 1),
                _ => (Some(&args[0]), &args[1], 2),
            };
            let (mu, sigma) = mu_sigma(cas, &args, from)?;
            let symbolic = args.iter().any(|a| a.has_free_symbols());
            if symbolic {
                // Φ(t) = (1 + erf((t-μ)/(σ√2)))/2
                let phi = |t: &Expr| -> R {
                    let z = div(sub(t.clone(), mu.clone())?, mul(vec![sigma.clone(), sqrt_e(Expr::int(2))?])?)?;
                    mul(vec![Expr::rat(1, 2), add(vec![Expr::one(), Expr::call("erf", vec![z])])?])
                };
                return match lo {
                    None => phi(hi),
                    Some(a) => sub(phi(hi)?, phi(a)?),
                };
            }
            let (m, s) = (num_arg(cas, &mu, "μ")?, num_arg(cas, &sigma, "σ")?);
            let cdf = |t: &Expr| -> Result<f64, String> { Ok(norm_cdf((num_arg(cas, t, "x")? - m) / s)) };
            Ok(fl(match lo {
                None => cdf(hi)?,
                Some(a) => {
                    let (av, bv) = (num_arg(cas, a, "a")?, num_arg(cas, hi, "b")?);
                    if av > bv {
                        return Err("normcdf: el límite inferior debe ser menor que el superior".into());
                    }
                    // use the smaller tail for accuracy
                    let (za, zb) = ((av - m) / s, (bv - m) / s);
                    if za > 0.0 {
                        norm_cdf(-za) - norm_cdf(-zb)
                    } else {
                        norm_cdf(zb) - norm_cdf(za)
                    }
                }
            }))
        }
        "invnorm" => {
            arity(name, &args, 1, 3, "(p[, μ, σ])")?;
            let (mu, sigma) = mu_sigma(cas, &args, 1)?;
            let (m, s) = (num_arg(cas, &mu, "μ")?, num_arg(cas, &sigma, "σ")?);
            map_list(&args[0], |p| {
                let pv = num_arg(cas, p, "p")?;
                if !(pv > 0.0 && pv < 1.0) {
                    return Err("invnorm: la probabilidad debe estar entre 0 y 1 (sin incluirlos)".into());
                }
                Ok(fl(m + s * inv_norm(pv)))
            })
        }
        "binompdf" => {
            arity(name, &args, 2, 3, "(n, p, k)")?;
            let (n, p) = binom_args(cas, name, &args)?;
            if args.len() == 2 {
                return Ok(Expr::List((0..=n.min(10_000)).map(|k| binom_pmf_e(cas, n, &p, k)).collect::<Result<_, _>>()?));
            }
            map_list(&args[2], |k| match kint(cas, k, name, false)? {
                Some(k) => binom_pmf_e(cas, n, &p, k),
                None => Ok(Expr::zero()),
            })
        }
        "binomcdf" => {
            arity(name, &args, 3, 4, "(n, p, k) = P(X ≤ k), o binomcdf(n, p, a, b) = P(a ≤ X ≤ b)")?;
            let (n, p) = binom_args(cas, name, &args)?;
            if args.len() == 4 {
                let a = kint(cas, &args[2], name, true)?.unwrap();
                let b = kint(cas, &args[3], name, true)?.unwrap();
                // P(a ≤ X ≤ b) with a rounded up
                let a = if args[2].as_int().is_some() { a } else { a + 1 };
                return binom_cdf_e(cas, n, &p, a, b);
            }
            map_list(&args[2], |k| binom_cdf_e(cas, n, &p, 0, kint(cas, k, name, true)?.unwrap()))
        }
        "poissonpdf" => {
            arity(name, &args, 2, 2, "(λ, k)")?;
            let l = args[0].clone();
            poisson_lambda(cas, name, &l)?;
            map_list(&args[1], |k| {
                let k = match kint(cas, k, name, false)? {
                    Some(k) if k >= 0 => k,
                    _ => return Ok(Expr::zero()),
                };
                if poisson_exact_ok(&l, k) {
                    let f = cas.apply("fact", vec![Expr::int(k)])?;
                    return mul(vec![pow(Expr::sym("e"), neg(l.clone())?)?, pow(l.clone(), Expr::int(k))?, pow(f, Expr::int(-1))?]);
                }
                Ok(fl(poisson_pmf_f(num_arg(cas, &l, "λ")?, k as f64)))
            })
        }
        "poissoncdf" => {
            arity(name, &args, 2, 2, "(λ, k)")?;
            let l = args[0].clone();
            poisson_lambda(cas, name, &l)?;
            map_list(&args[1], |k| poisson_cdf(cas, &l, kint(cas, k, name, true)?.unwrap()))
        }
        "abserr" => err_pair(name, &args, |a, e| cas.apply("abs", vec![sub(a.clone(), e.clone())?])),
        "relerr" | "pcterr" => err_pair(name, &args, |a, e| {
            if e.is_zero() {
                return Err(format!("{name}: el valor exacto no puede ser cero"));
            }
            let r = div(cas.apply("abs", vec![sub(a.clone(), e.clone())?])?, cas.apply("abs", vec![e.clone()])?)?;
            if name == "pcterr" {
                mul(vec![Expr::int(100), r])
            } else {
                Ok(r)
            }
        }),
        "zscore" => {
            match args.len() {
                1 => {
                    // standardize a list with its mean and sample standard deviation
                    let xs = list_arg(&args[0], name)?;
                    nonempty(&xs, name)?;
                    let (m, s) = if is_exact(&xs) {
                        (mean_e(&xs, &None)?, sqrt_e(var_e(&xs, &None, true)?)?)
                    } else {
                        let (n, m, ss) = wstats_f(cas, &xs, &None, name)?;
                        (fl(m), fl((ss / (n - 1.0)).sqrt()))
                    };
                    if s.is_zero() {
                        return Err("zscore: la desviación estándar es cero".into());
                    }
                    Ok(Expr::List(xs.iter().map(|x| tidy(div(sub(x.clone(), m.clone())?, s.clone())?)).collect::<Result<_, _>>()?))
                }
                3 => {
                    let (mu, sigma) = mu_sigma(cas, &args, 1)?;
                    map_list(&args[0], |x| tidy(div(sub(x.clone(), mu.clone())?, sigma.clone())?))
                }
                _ => Err("Uso: zscore(x, μ, σ) o zscore(lista)".into()),
            }
        }
        _ => Err(format!("{name}: no implementado")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{ap, ev};
    use crate::{Engine, Settings};

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol * (1.0 + b.abs())
    }

    #[test]
    fn one_var_basic() {
        let o = one_var(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        assert_eq!(o.n, 4);
        assert_eq!(o.mean, 2.5);
        assert!(close(o.sample_var, 5.0 / 3.0, 1e-15));
        assert_eq!(o.pop_var, 1.25);
        assert_eq!((o.q1, o.median, o.q3), (1.5, 2.5, 3.5));
        assert_eq!((o.min, o.max), (1.0, 4.0));
        let o = one_var(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]).unwrap();
        assert_eq!((o.q1, o.median, o.q3), (2.0, 4.0, 6.0));
        let o = one_var(&[2.0, 2.0, 3.0, 5.0, 5.0, 9.0]).unwrap();
        assert_eq!(o.mode, vec![2.0, 5.0]);
        assert!(close(o.sem, o.sample_sd / 6f64.sqrt(), 1e-15));
        assert!(one_var(&[]).is_err());
        let o = one_var(&[7.0]).unwrap();
        assert!(o.sample_var.is_nan());
        assert_eq!(o.q1, 7.0);
    }

    #[test]
    fn regression_linear() {
        let xs = [1.0, 2.0, 3.0, 4.0];
        let ys = [3.0, 5.0, 7.0, 9.0];
        let r = regress(RegKind::Linear, &xs, &ys).unwrap();
        assert!(close(r.coeffs[0], 2.0, 1e-12) && close(r.coeffs[1], 1.0, 1e-12));
        assert!(close(r.r.unwrap(), 1.0, 1e-12));
        assert!(close(r.r2, 1.0, 1e-12));
        assert_eq!(r.formula, "y = 2x + 1");
        assert!(close(r.eval(10.0), 21.0, 1e-12));
        let r = regress(RegKind::Linear, &[0.0, 1.0, 2.0], &[1.0, -1.0, -3.0]).unwrap();
        assert_eq!(r.formula, "y = -2x + 1");
        assert!(close(r.r.unwrap(), -1.0, 1e-12));
        // known dataset
        let r = regress(RegKind::Linear, &[1.0, 2.0, 3.0, 4.0, 5.0], &[2.0, 4.0, 5.0, 4.0, 5.0]).unwrap();
        assert!(close(r.coeffs[0], 0.6, 1e-12) && close(r.coeffs[1], 2.2, 1e-12));
        assert!(close(r.r2, 0.6, 1e-12));
        assert_eq!(r.formula, "y = 0.6x + 2.2");
        assert!(regress(RegKind::Linear, &[1.0, 1.0], &[1.0, 2.0]).is_err());
    }

    #[test]
    fn regression_nonlinear() {
        let xs: Vec<f64> = (0..6).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 2.0 * x * x - 3.0 * x + 1.0).collect();
        let r = regress(RegKind::Quadratic, &xs, &ys).unwrap();
        assert!(close(r.coeffs[0], 2.0, 1e-9) && close(r.coeffs[1], -3.0, 1e-9) && close(r.coeffs[2], 1.0, 1e-9));
        assert_eq!(r.formula, "y = 2x² - 3x + 1");
        assert!(close(r.r2, 1.0, 1e-12));
        let ys: Vec<f64> = xs.iter().map(|x| x * x * x - x).collect();
        let r = regress(RegKind::Cubic, &xs, &ys).unwrap();
        assert_eq!(r.formula, "y = x³ - x");
        let xs = [1.0, 2.0, 3.0, 4.0];
        let ys: Vec<f64> = xs.iter().map(|x: &f64| 3.0 * (0.5 * x).exp()).collect();
        let r = regress(RegKind::Exponential, &xs, &ys).unwrap();
        assert!(close(r.coeffs[0], 3.0, 1e-9) && close(r.coeffs[1], 0.5, 1e-9));
        assert_eq!(r.formula, "y = 3·e^(0.5x)");
        let ys: Vec<f64> = xs.iter().map(|x: &f64| 2.0 * x.powf(1.5)).collect();
        let r = regress(RegKind::Power, &xs, &ys).unwrap();
        assert_eq!(r.formula, "y = 2·x^1.5");
        let ys: Vec<f64> = xs.iter().map(|x: &f64| 1.0 - 2.0 * x.ln()).collect();
        let r = regress(RegKind::Logarithmic, &xs, &ys).unwrap();
        assert_eq!(r.formula, "y = 1 - 2·ln(x)");
        assert!(regress(RegKind::Exponential, &[1.0, 2.0], &[1.0, -1.0]).is_err());
        assert!(regress(RegKind::Cubic, &[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0]).is_err());
    }

    #[test]
    fn exact_descriptive() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "mean({1,2,3,4})"), "5/2");
        assert_eq!(ev(&mut e, "var({1,2,3,4})"), "5/3");
        assert_eq!(ev(&mut e, "pvar({1,2,3,4})"), "5/4");
        assert_eq!(ev(&mut e, "stdev({1,2,3,4})"), "√15/3");
        assert_eq!(ev(&mut e, "pstdev({1,2,3,4})"), "√5/2");
        assert_eq!(ev(&mut e, "sem({1,2,3,4})"), "√15/6");
        assert_eq!(ev(&mut e, "median({5,1,3,2})"), "5/2");
        assert_eq!(ev(&mut e, "median({5,1,3})"), "3");
        assert_eq!(ev(&mut e, "q1({1,2,3,4,5,6,7})"), "2");
        assert_eq!(ev(&mut e, "q3({1,2,3,4,5,6,7})"), "6");
        assert_eq!(ev(&mut e, "iqr({1,2,3,4,5,6,7,8})"), "4");
        assert_eq!(ev(&mut e, "range({3,-1,7})"), "8");
        assert_eq!(ev(&mut e, "count({3,-1,7})"), "3");
        assert_eq!(ev(&mut e, "mode({1,2,2,3})"), "2");
        assert_eq!(ev(&mut e, "mode({1,1,2,2,3})"), "{1, 2}");
        assert_eq!(ev(&mut e, "sort({3,1/2,2})"), "{1/2, 2, 3}");
        assert_eq!(ev(&mut e, "sortd({3,1/2,2})"), "{3, 2, 1/2}");
        assert_eq!(ev(&mut e, "cumsum({1,2,3})"), "{1, 3, 6}");
        assert_eq!(ev(&mut e, "mean({a,b})"), "a/2 + b/2");
        assert_eq!(ap(&mut e, "mean({1.5, 2.5})"), "2");
        assert!(ev(&mut e, "mean({})").starts_with("ERR"));
        assert!(ev(&mut e, "var({1})").starts_with("ERR"));
        assert!(ev(&mut e, "median({a,b})").starts_with("ERR"));
    }

    #[test]
    fn weighted() {
        let mut e = Engine::new();
        // {1,1,2,3,3,3}
        assert_eq!(ev(&mut e, "mean({1,2,3},{2,1,3})"), "13/6");
        assert_eq!(ev(&mut e, "var({1,2,3},{2,1,3})"), "29/30");
        assert_eq!(ev(&mut e, "var({1,1,2,3,3,3})"), "29/30");
        assert_eq!(ev(&mut e, "median({1,2,3},{2,1,3})"), "5/2");
        assert_eq!(ev(&mut e, "count({1,2,3},{2,1,3})"), "6");
        assert_eq!(ev(&mut e, "mode({1,2,3},{2,1,3})"), "3");
        assert!(ev(&mut e, "mean({1,2},{1})").starts_with("ERR"));
    }

    #[test]
    fn big_list_float() {
        let e = Engine::new();
        let s = Settings::default();
        let cas = Cas::new(&s);
        let big = Expr::List((1..=3000).map(Expr::int).collect());
        assert_eq!(e.print(&cmd(&cas, "mean", vec![big.clone()]).unwrap()), "1500.5");
        assert_eq!(e.print(&cmd(&cas, "median", vec![big.clone()]).unwrap()), "1500.5");
        assert_eq!(e.print(&cmd(&cas, "pvar", vec![big]).unwrap()), "749999.9167");
    }

    #[test]
    fn bivariate() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "slope({1,2,3},{3,5,7})"), "2");
        assert_eq!(ev(&mut e, "intercept({1,2,3},{3,5,7})"), "1");
        assert_eq!(ev(&mut e, "slope({1,2,3,4,5},{2,4,5,4,5})"), "3/5");
        assert_eq!(ev(&mut e, "intercept({1,2,3,4,5},{2,4,5,4,5})"), "11/5");
        assert_eq!(ev(&mut e, "cov({1,2,3},{3,5,7})"), "2");
        assert_eq!(ev(&mut e, "corr({1,2,3},{3,5,7})"), "1");
        assert_eq!(ap(&mut e, "corr({1,2,3,4,5},{2,4,5,4,5})"), "0.7745966692");
        let t = ev(&mut e, "linreg({1,2,3,4},{3,5,7,9})");
        assert_eq!(t, "y = a·x + b\na = 2\nb = 1\nr = 1\nr² = 1\ny = 2x + 1");
        let t = ev(&mut e, "linreg({1,2,3,4,5},{2,4,5,4,5})");
        assert_eq!(t, "y = a·x + b\na = 3/5 ≈ 0.6\nb = 11/5 ≈ 2.2\nr = 0.774597\nr² = 0.6\ny = 0.6x + 2.2");
        let t = ev(&mut e, "quadreg({0,1,2,3},{1,0,3,10})");
        assert_eq!(t, "y = a·x² + b·x + c\na = 2\nb = -3\nc = 1\nr² = 1\ny = 2x² - 3x + 1");
        assert!(ev(&mut e, "expreg({1,2},{1,-1})").starts_with("ERR"));
        assert!(ev(&mut e, "powreg({1,2,3},{2,4})").starts_with("ERR"));
    }

    #[test]
    fn distributions() {
        let mut e = Engine::new();
        assert_eq!(ap(&mut e, "normpdf(0)"), "0.3989422804");
        assert_eq!(ap(&mut e, "normcdf(1.96)"), "0.9750021049");
        assert_eq!(ap(&mut e, "normcdf(-1, 1)"), "0.6826894921");
        assert_eq!(ap(&mut e, "normcdf(90, 110, 100, 10)"), "0.6826894921");
        assert_eq!(ap(&mut e, "normcdf(0)"), "0.5");
        assert_eq!(ap(&mut e, "invnorm(0.975)"), "1.959963985");
        assert_eq!(ap(&mut e, "invnorm(0.5, 100, 15)"), "100");
        assert!(close(inv_norm(1e-10), -6.361340902404056, 1e-12));
        for &p in &[1e-12, 0.001, 0.02, 0.3, 0.5, 0.77, 0.99, 0.999999] {
            assert!(close(norm_cdf(inv_norm(p)), p, 1e-12), "p={p}");
        }
        assert_eq!(ev(&mut e, "binompdf(4, 1/2, 2)"), "3/8");
        assert_eq!(ev(&mut e, "binomcdf(4, 1/2, 2)"), "11/16");
        assert_eq!(ev(&mut e, "binomcdf(4, 1/2, 1, 3)"), "7/8");
        assert_eq!(ev(&mut e, "binompdf(2, 1/2)"), "{1/4, 1/2, 1/4}");
        assert_eq!(ap(&mut e, "binompdf(10, 0.3, 3)"), "0.266827932");
        assert_eq!(ev(&mut e, "binompdf(4, 1/2, 5)"), "0");
        assert_eq!(ap(&mut e, "poissonpdf(2, 3)"), "0.1804470443");
        assert_eq!(ap(&mut e, "poissoncdf(2, 3)"), "0.8571234605");
        assert_eq!(ap(&mut e, "poissonpdf(2.5, 0)"), "0.08208499862");
        assert!(ev(&mut e, "invnorm(1)").starts_with("ERR"));
        assert!(ev(&mut e, "binompdf(4, 2, 1)").starts_with("ERR"));
        assert!(ev(&mut e, "normpdf(0, 0, -1)").starts_with("ERR"));
        // symbolic density can be plotted
        let c = e.compile_src("normpdf(x)", &["x"]).unwrap();
        assert!(close(c.eval1(1.0), norm_pdf(1.0), 1e-12));
    }

    #[test]
    fn errors_and_zscores() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "abserr(3.14, pi)").len() > 0, true);
        assert_eq!(ev(&mut e, "abserr(22/7, 3)"), "1/7");
        assert_eq!(ev(&mut e, "relerr(11/10, 1)"), "1/10");
        assert_eq!(ev(&mut e, "pcterr(11/10, 1)"), "10");
        assert_eq!(ev(&mut e, "abserr({9, 11, 10}, 10)"), "{1, 1, 0}");
        assert_eq!(ev(&mut e, "relerr({9, 11}, {10, 10})"), "{1/10, 1/10}");
        assert_eq!(ev(&mut e, "var(abserr({9, 11, 10}, 10))"), "1/3");
        assert!(ev(&mut e, "relerr(1, 0)").starts_with("ERR"));
        assert_eq!(ap(&mut e, "abserr(3.14, pi)"), "0.00159265359");
        assert_eq!(ev(&mut e, "zscore(130, 100, 15)"), "2");
        assert_eq!(ev(&mut e, "zscore({1,2,3})"), "{-1, 0, 1}");
        assert_eq!(ev(&mut e, "zscore({85, 100}, 100, 15)"), "{-1, 0}");
    }
}
