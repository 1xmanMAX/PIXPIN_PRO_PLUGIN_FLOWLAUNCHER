//! Equation solving and polynomial algebra.
//!
//! Conventions:
//! * Coefficient vectors returned by [`poly_coeffs`] and used by the `q_*`
//!   (exact rational) helpers are **lowest degree first**: `c[k]` is the
//!   coefficient of `x^k`. The user command `coeffs(p, x)` returns them
//!   highest degree first.
//! * `MPoly` is a sparse multivariate polynomial with rational coefficients
//!   over a list of "atoms" (symbols or non-polynomial sub-expressions such as
//!   `sin(x)` or `e^x`), used by `factor`, `cancel` and polynomial gcd.

use crate::eval::{recognize, Cas};
use crate::expr::{cmp_expr, is_constant, Expr, Printer};
use crate::matrix;
use crate::num::{factorize, gcd_i128, Rat, C};
use crate::numeric::{self, Compiled};
use crate::simplify::{add, div, expand, from_c, mul, neg, pow, sub, subst, R};
use std::cmp::Ordering;

/// Maximum polynomial degree handled symbolically.
const MAX_DEG: usize = 400;
/// Maximum recursion depth of the solver (isolation, substitution).
const MAX_DEPTH: usize = 10;

// =================================================================== helpers

fn num(r: Rat) -> Expr {
    Expr::Num(r)
}

fn terms_of(e: &Expr) -> Vec<Expr> {
    match e {
        Expr::Add(v) => v.clone(),
        other => vec![other.clone()],
    }
}

fn is_sym(e: &Expr, x: &str) -> bool {
    matches!(e, Expr::Sym(s) if s == x)
}

fn add2(a: Expr, b: Expr) -> R {
    add(vec![a, b])
}

fn mul2(a: Expr, b: Expr) -> R {
    mul(vec![a, b])
}

/// Same heuristic the printer uses to order the terms of a sum.
fn display_degree(e: &Expr) -> f64 {
    match e {
        Expr::Sym(s) if !is_constant(s) => 1.0,
        Expr::Pow(b, x) => {
            let d = display_degree(b);
            match x.as_f64() {
                Some(k) => d * k,
                None => d + 1.0,
            }
        }
        Expr::Mul(v) => v.iter().map(display_degree).sum(),
        Expr::Call(_, a) => {
            if a.iter().any(|x| x.has_free_symbols()) {
                0.5
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

/// Free symbols of an expression, in order of appearance.
fn free_symbols(e: &Expr) -> Vec<String> {
    let mut v = Vec::new();
    e.symbols(&mut v);
    v
}

/// Picks the variable: explicit symbol, the only free symbol, or `x`.
fn pick_var(e: &Expr, given: Option<&Expr>) -> Result<String, String> {
    if let Some(g) = given {
        return match g {
            Expr::Sym(s) => Ok(s.clone()),
            Expr::List(v) if v.len() == 1 && v[0].sym_name().is_some() => Ok(v[0].sym_name().unwrap().to_string()),
            _ => Err("Se esperaba una variable, por ejemplo x".into()),
        };
    }
    let syms = free_symbols(e);
    match syms.len() {
        1 => Ok(syms[0].clone()),
        0 => Ok("x".into()),
        _ => {
            if syms.iter().any(|s| s == "x") {
                Ok("x".into())
            } else {
                Err(format!("Indique la variable (hay varias: {})", syms.join(", ")))
            }
        }
    }
}

fn gcd_rat(a: Rat, b: Rat) -> Option<Rat> {
    // gcd(a/b, c/d) = gcd(a, c) / lcm(b, d)
    if a.is_zero() {
        return Some(Rat { n: b.n.abs(), d: b.d });
    }
    if b.is_zero() {
        return Some(Rat { n: a.n.abs(), d: a.d });
    }
    let n = gcd_i128(a.n, b.n);
    let g = gcd_i128(a.d, b.d);
    let l = (a.d / g).checked_mul(b.d)?;
    Rat::new(n, l)
}

fn lcm_i128(a: i128, b: i128) -> Option<i128> {
    if a == 0 || b == 0 {
        return Some(0);
    }
    (a / gcd_i128(a, b)).checked_mul(b).map(|v| v.abs())
}

/// Numeric value (complex) of a closed expression.
fn cval(cas: &Cas, e: &Expr) -> Option<C> {
    if e.has_free_symbols() {
        return None;
    }
    cas.num(e).ok().filter(|z| z.is_finite())
}

fn is_real_val(z: C) -> bool {
    z.im.abs() <= 1e-10 * z.re.abs().max(1.0)
}

/// Sort key for solution values: closed values by (re, im), symbolic last.
fn sort_values(cas: &Cas, vals: &mut Vec<Expr>) {
    let mut keyed: Vec<(Option<C>, usize, Expr)> = vals.drain(..).enumerate().map(|(i, v)| (cval(cas, &v), i, v)).collect();
    keyed.sort_by(|a, b| match (a.0, b.0) {
        (Some(x), Some(y)) => {
            let xr = is_real_val(x);
            let yr = is_real_val(y);
            yr.cmp(&xr).then(x.re.total_cmp(&y.re)).then(x.im.total_cmp(&y.im)).then(a.1.cmp(&b.1))
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.1.cmp(&b.1),
    });
    vals.extend(keyed.into_iter().map(|k| k.2));
}

/// Removes duplicate values (exactly or numerically equal), preferring exact forms.
fn dedup_values(cas: &Cas, vals: Vec<Expr>) -> Vec<Expr> {
    let mut out: Vec<Expr> = Vec::new();
    for v in vals {
        let zv = cval(cas, &v);
        let mut dup = None;
        for (k, o) in out.iter().enumerate() {
            if *o == v {
                dup = Some(k);
                break;
            }
            if let (Some(a), Some(b)) = (zv, cval(cas, o)) {
                if a.sub(b).abs() <= 1e-9 * (1.0 + a.abs()) {
                    dup = Some(k);
                    break;
                }
            }
        }
        match dup {
            Some(k) => {
                if out[k].contains_float() && !v.contains_float() {
                    out[k] = v;
                }
            }
            None => out.push(v),
        }
    }
    out
}

fn clean_float(v: f64) -> f64 {
    if v.abs() < 1e-14 {
        0.0
    } else {
        v
    }
}

/// Exact recognition of a numeric root: small rationals, k·π, √rational.
fn recognize_root(cas: &Cas, v: f64) -> Expr {
    let v = clean_float(v);
    if let Some(r) = Rat::approximate(v, 1000, 1e-12) {
        return Expr::Num(r);
    }
    if !cas.s.deg {
        if let Some(k) = Rat::approximate(v / std::f64::consts::PI, 24, 1e-12) {
            if !k.is_zero() {
                if let Ok(e) = mul2(num(k), Expr::sym("pi")) {
                    return e;
                }
            }
        }
    }
    if let Some(e) = recognize(v) {
        if !matches!(e, Expr::Num(_)) {
            return e;
        }
    }
    Expr::Float(v)
}

// ======================================================= univariate (Expr)

/// Degree in `x` and coefficient of a single (expanded) term.
fn term_split(t: &Expr, x: &str) -> Option<(usize, Expr)> {
    if t.free_of(x) {
        return Some((0, t.clone()));
    }
    match t {
        Expr::Sym(s) if s == x => Some((1, Expr::one())),
        Expr::Pow(b, e) if is_sym(b, x) => {
            let r = e.as_rat()?;
            if !r.is_int() || r.n < 0 || r.n as usize > MAX_DEG {
                return None;
            }
            Some((r.n as usize, Expr::one()))
        }
        Expr::Mul(v) => {
            let mut deg = 0usize;
            let mut rest = Vec::new();
            for f in v {
                if f.free_of(x) {
                    rest.push(f.clone());
                } else {
                    let (d, _) = term_split(f, x)?;
                    if matches!(f, Expr::Mul(_)) {
                        return None;
                    }
                    deg += d;
                }
            }
            if deg > MAX_DEG {
                return None;
            }
            Some((deg, mul(rest).ok()?))
        }
        _ => None,
    }
}

/// Coefficients of `e` as a polynomial in `x`, **lowest degree first**
/// (`c[k]` multiplies `x^k`). Coefficients may contain other symbols.
/// Returns `None` when `e` is not a polynomial in `x`. The zero polynomial
/// yields `[0]`.
pub fn poly_coeffs(e: &Expr, x: &str) -> Option<Vec<Expr>> {
    if matches!(e, Expr::List(_) | Expr::Matrix(_) | Expr::Eq(..) | Expr::Rel(..) | Expr::Text(_)) {
        return None;
    }
    let ex = expand(e).ok()?;
    let mut buckets: Vec<Vec<Expr>> = Vec::new();
    for t in terms_of(&ex) {
        let (d, c) = term_split(&t, x)?;
        if buckets.len() <= d {
            buckets.resize(d + 1, Vec::new());
        }
        buckets[d].push(c);
    }
    let mut out: Vec<Expr> = buckets.into_iter().map(add).collect::<Result<_, _>>().ok()?;
    while out.len() > 1 && out.last().map_or(false, |c| c.is_zero()) {
        out.pop();
    }
    if out.is_empty() {
        out.push(Expr::zero());
    }
    Some(out)
}

/// Degree of `e` in `x` (0 for constants and the zero polynomial).
pub fn poly_degree(e: &Expr, x: &str) -> Option<usize> {
    poly_coeffs(e, x).map(|c| c.len() - 1)
}

/// Builds `Σ c[k]·x^k` from lowest-first coefficients.
pub fn poly_from_coeffs(c: &[Expr], x: &str) -> R {
    let mut terms = Vec::with_capacity(c.len());
    for (k, ck) in c.iter().enumerate() {
        if ck.is_zero() {
            continue;
        }
        terms.push(mul2(ck.clone(), pow(Expr::sym(x), Expr::int(k as i128))?)?);
    }
    add(terms)
}

fn trim_exprs(mut c: Vec<Expr>) -> Vec<Expr> {
    while c.len() > 1 && c.last().map_or(false, |v| v.is_zero()) {
        c.pop();
    }
    c
}

/// Division with remainder of lowest-first coefficient vectors (symbolic
/// coefficients allowed; exact over ℚ when coefficients are rational).
pub fn poly_divmod(a: &[Expr], b: &[Expr]) -> Result<(Vec<Expr>, Vec<Expr>), String> {
    let b = trim_exprs(b.to_vec());
    if b.len() == 1 && b[0].is_zero() {
        return Err("División entre el polinomio cero".into());
    }
    let mut r = trim_exprs(a.to_vec());
    let db = b.len() - 1;
    let lb = b[db].clone();
    if r.len() < b.len() {
        return Ok((vec![Expr::zero()], r));
    }
    let mut q = vec![Expr::zero(); r.len() - db];
    while r.len() >= b.len() {
        let k = r.len() - 1 - db;
        let c = expand(&div(r[r.len() - 1].clone(), lb.clone())?)?;
        q[k] = c.clone();
        for j in 0..db {
            r[k + j] = expand(&sub(r[k + j].clone(), mul2(c.clone(), b[j].clone())?)?)?;
        }
        r.pop();
        r = trim_exprs(r);
        if r.is_empty() || (r.len() == 1 && r[0].is_zero()) {
            r = vec![Expr::zero()];
            break;
        }
    }
    Ok((trim_exprs(q), r))
}

// ======================================================= exact ℚ[x] toolkit

type Q = Vec<Rat>;

fn q_trim(mut p: Q) -> Q {
    while matches!(p.last(), Some(r) if r.is_zero()) {
        p.pop();
    }
    p
}

fn q_get(p: &Q, i: usize) -> Rat {
    p.get(i).copied().unwrap_or(Rat::ZERO)
}

fn q_from_exprs(c: &[Expr]) -> Option<Q> {
    c.iter().map(|e| e.as_rat()).collect::<Option<Vec<_>>>().map(q_trim)
}

fn q_to_exprs(p: &Q) -> Vec<Expr> {
    if p.is_empty() {
        return vec![Expr::zero()];
    }
    p.iter().map(|r| num(*r)).collect()
}

fn q_sub(a: &Q, b: &Q) -> Option<Q> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(q_get(a, i).sub(q_get(b, i))?);
    }
    Some(q_trim(out))
}

fn q_scale(a: &Q, c: Rat) -> Option<Q> {
    a.iter().map(|x| x.mul(c)).collect::<Option<Vec<_>>>().map(q_trim)
}

fn q_divmod(a: &Q, b: &Q) -> Option<(Q, Q)> {
    let b = q_trim(b.clone());
    if b.is_empty() {
        return None;
    }
    let mut r = q_trim(a.clone());
    let db = b.len() - 1;
    let lb = b[db];
    if r.len() < b.len() {
        return Some((vec![], r));
    }
    let mut q = vec![Rat::ZERO; r.len() - db];
    while r.len() >= b.len() {
        let k = r.len() - 1 - db;
        let c = r.last()?.div(lb)?;
        q[k] = c;
        for j in 0..=db {
            r[k + j] = r[k + j].sub(c.mul(b[j])?)?;
        }
        r.pop();
        r = q_trim(r);
    }
    Some((q_trim(q), r))
}

fn q_monic(p: &Q) -> Option<Q> {
    let p = q_trim(p.clone());
    match p.last() {
        None => Some(p),
        Some(l) => q_scale(&p, l.recip()?),
    }
}

/// Monic gcd over ℚ (Euclid with monic normalization).
fn q_gcd(a: &Q, b: &Q) -> Option<Q> {
    let mut a = q_monic(a)?;
    let mut b = q_monic(b)?;
    let mut guard = 0;
    while !b.is_empty() {
        guard += 1;
        if guard > 2 * MAX_DEG {
            return None;
        }
        let (_, r) = q_divmod(&a, &b)?;
        a = b;
        b = q_monic(&r)?;
    }
    q_monic(&a)
}

fn q_deriv(p: &Q) -> Option<Q> {
    (1..p.len()).map(|i| p[i].mul(Rat::int(i as i128))).collect::<Option<Vec<_>>>().map(q_trim)
}

fn q_eval(p: &Q, x: Rat) -> Option<Rat> {
    let mut acc = Rat::ZERO;
    for c in p.iter().rev() {
        acc = acc.mul(x)?.add(*c)?;
    }
    Some(acc)
}

/// `p = content · prim` with `prim` integer, coprime, positive leading coefficient.
fn q_primitive(p: &Q) -> Option<(Rat, Vec<i128>)> {
    let p = q_trim(p.clone());
    if p.is_empty() {
        return Some((Rat::ZERO, vec![]));
    }
    let mut l: i128 = 1;
    for c in &p {
        l = lcm_i128(l, c.d)?;
    }
    let ints: Vec<i128> = p.iter().map(|c| c.n.checked_mul(l / c.d)).collect::<Option<_>>()?;
    let mut g = 0;
    for v in &ints {
        g = gcd_i128(g, *v);
    }
    let sign = if *ints.last()? < 0 { -1 } else { 1 };
    let g = g * sign;
    let prim: Vec<i128> = ints.iter().map(|v| v / g).collect();
    Some((Rat::new(g, l)?, prim))
}

fn q_from_ints(v: &[i128]) -> Q {
    q_trim(v.iter().map(|n| Rat::int(*n)).collect())
}

/// Yun's square-free factorization: monic factors with multiplicities.
fn q_squarefree(p: &Q) -> Option<Vec<(Q, usize)>> {
    let f = q_monic(p)?;
    if f.len() <= 1 {
        return Some(vec![]);
    }
    let fd = q_deriv(&f)?;
    let a0 = q_gcd(&f, &fd)?;
    let mut b = q_divmod(&f, &a0)?.0;
    let c = q_divmod(&fd, &a0)?.0;
    let mut d = q_sub(&c, &q_deriv(&b)?)?;
    let mut out = Vec::new();
    let mut i = 1;
    while b.len() > 1 {
        let a = if d.is_empty() { q_monic(&b)? } else { q_gcd(&b, &d)? };
        if a.len() > 1 {
            out.push((a.clone(), i));
        }
        b = q_divmod(&b, &a)?.0;
        let c = q_divmod(&d, &a)?.0;
        d = q_sub(&c, &q_deriv(&b)?)?;
        i += 1;
        if i > MAX_DEG + 2 {
            return None;
        }
    }
    Some(out)
}

fn divisors(n: i128) -> Option<Vec<i128>> {
    let n = n.abs();
    if n == 0 {
        return Some(vec![]);
    }
    if n > 1_000_000_000_000_000 {
        return None;
    }
    let fs = factorize(n as u128);
    let mut ds: Vec<i128> = vec![1];
    for (p, k) in fs {
        let mut next = Vec::new();
        for d in &ds {
            let mut m = *d;
            next.push(m);
            for _ in 0..k {
                m = m.checked_mul(p as i128)?;
                next.push(m);
            }
        }
        ds = next;
        if ds.len() > 20000 {
            return None;
        }
    }
    ds.sort();
    Some(ds)
}

fn q_to_c_high(p: &Q) -> Vec<C> {
    p.iter().rev().map(|r| C::real(r.to_f64())).collect()
}

/// All distinct rational roots of `p` (rational root theorem, with a
/// numerically guided fallback for huge coefficients).
fn q_rational_roots(p: &Q) -> Option<Vec<Rat>> {
    let (_, mut ints) = q_primitive(p)?;
    let mut roots = Vec::new();
    if ints.is_empty() {
        return Some(roots);
    }
    if ints[0] == 0 {
        roots.push(Rat::ZERO);
        while !ints.is_empty() && ints[0] == 0 {
            ints.remove(0);
        }
    }
    if ints.len() <= 1 {
        return Some(roots);
    }
    let poly = q_from_ints(&ints);
    let a0 = ints[0];
    let an = *ints.last()?;
    let mut found: Vec<Rat> = Vec::new();
    let enumerated = match (divisors(a0), divisors(an)) {
        (Some(dp), Some(dq)) if dp.len() * dq.len() <= 40000 => {
            // numeric prefilter: approximate roots
            let approx: Vec<f64> = numeric::poly_roots(&q_to_c_high(&poly)).into_iter().filter(|z| z.im.abs() < 1e-3 * (1.0 + z.re.abs())).map(|z| z.re).collect();
            let use_filter = ints.len() > 12;
            for q in &dq {
                for p in &dp {
                    for s in [1i128, -1] {
                        let r = match Rat::new(s * p, *q) {
                            Some(r) => r,
                            None => continue,
                        };
                        if use_filter && !approx.iter().any(|a| (a - r.to_f64()).abs() < 1e-3 * (1.0 + a.abs())) {
                            continue;
                        }
                        if !found.contains(&r) && q_eval(&poly, r) == Some(Rat::ZERO) {
                            found.push(r);
                        }
                    }
                }
            }
            true
        }
        _ => false,
    };
    if !enumerated {
        let maxd = an.abs().min(1_000_000_000);
        for z in numeric::poly_roots(&q_to_c_high(&poly)) {
            if z.im.abs() > 1e-6 * (1.0 + z.re.abs()) {
                continue;
            }
            if let Some(r) = Rat::approximate(z.re, maxd, 1e-9) {
                if !found.contains(&r) && q_eval(&poly, r) == Some(Rat::ZERO) {
                    found.push(r);
                }
            }
        }
    }
    roots.extend(found);
    roots.sort();
    roots.dedup();
    Some(roots)
}

/// Splits a monic integer quartic without rational roots into two quadratics.
fn quartic_split(ints: &[i128]) -> Option<(Vec<i128>, Vec<i128>)> {
    if ints.len() != 5 || ints[4] != 1 {
        return None;
    }
    let (d, c, b, a) = (ints[0], ints[1], ints[2], ints[3]);
    let ds = divisors(d)?;
    for q0 in &ds {
        for q in [*q0, -*q0] {
            if d % q != 0 {
                continue;
            }
            let s = d / q;
            let pairs: Vec<(i128, i128)> = if q != s {
                let num = c.checked_sub(a.checked_mul(q)?)?;
                let den = s - q;
                if num % den != 0 {
                    continue;
                }
                let p = num / den;
                vec![(p, a - p)]
            } else {
                if c != q.checked_mul(a)? {
                    continue;
                }
                let disc = a.checked_mul(a)?.checked_sub(4i128.checked_mul(b.checked_sub(2 * q)?)?)?;
                if disc < 0 {
                    continue;
                }
                let r = (disc as f64).sqrt().round() as i128;
                if r * r != disc || (a + r) % 2 != 0 {
                    continue;
                }
                vec![((a + r) / 2, (a - r) / 2)]
            };
            for (p, r) in pairs {
                if q + s + p * r == b && p * s + q * r == c && p + r == a {
                    return Some((vec![q, p, 1], vec![s, r, 1]));
                }
            }
        }
    }
    None
}

/// Further splitting of a square-free factor without rational roots.
fn q_split_irreducible(g: &Q, depth: usize) -> Option<Vec<Q>> {
    let deg = g.len() - 1;
    if deg <= 3 || depth > 3 {
        return Some(vec![g.clone()]);
    }
    let (_, ints) = q_primitive(g)?;
    if deg == 4 {
        if let Some((a, b)) = quartic_split(&ints) {
            return Some(vec![q_from_ints(&a), q_from_ints(&b)]);
        }
    }
    // even polynomial: g(x) = h(x^2)
    if ints.iter().enumerate().all(|(i, c)| i % 2 == 0 || *c == 0) {
        let h: Q = ints.iter().step_by(2).map(|c| Rat::int(*c)).collect();
        // factor h fully, then split each h_i(x^2) further
        let (_, hf) = q_factor(&h)?;
        if hf.len() > 1 || hf.iter().any(|f| f.1 > 1) {
            let mut out = Vec::new();
            for (hi, m) in hf {
                let mut g2 = vec![Rat::ZERO; 2 * hi.len() - 1];
                for (i, c) in hi.iter().enumerate() {
                    g2[2 * i] = *c;
                }
                let parts = q_split_irreducible(&g2, depth + 1)?;
                for _ in 0..m {
                    out.extend(parts.iter().cloned());
                }
            }
            return Some(out);
        }
    }
    Some(vec![g.clone()])
}

/// Factorization over ℚ: `p = c · Π f_i^m_i` with `f_i` primitive integer
/// polynomials (positive leading coefficient).
fn q_factor(p: &Q) -> Option<(Rat, Vec<(Q, usize)>)> {
    let p = q_trim(p.clone());
    if p.len() <= 1 {
        return Some((q_get(&p, 0), vec![]));
    }
    let sqf = q_squarefree(&p)?;
    let mut out: Vec<(Q, usize)> = Vec::new();
    for (f, m) in sqf {
        let roots = q_rational_roots(&f)?;
        let mut g = f.clone();
        for r in roots {
            out.push((vec![Rat::int(-r.n), Rat::int(r.d)], m));
            g = q_divmod(&g, &vec![r.neg()?, Rat::ONE])?.0;
        }
        if g.len() > 1 {
            for h in q_split_irreducible(&g, 0)? {
                let (_, ints) = q_primitive(&h)?;
                out.push((q_from_ints(&ints), m));
            }
        }
    }
    // constant = lc(p) / Π lc(f)^m
    let mut c = *p.last()?;
    for (f, m) in &out {
        c = c.div(f.last()?.powi(*m as i64)?)?;
    }
    out.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then_with(|| cmp_q(&a.0, &b.0)));
    Some((c, out))
}

fn cmp_q(a: &Q, b: &Q) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let c = x.cmp(y);
        if c != Ordering::Equal {
            return c;
        }
    }
    a.len().cmp(&b.len())
}

// ======================================================= multivariate ℚ[atoms]

type Exps = Vec<u32>;
/// Sparse polynomial: terms sorted by exponent vector (lex, descending).
type MPoly = Vec<(Exps, Rat)>;

/// Splits an expanded term into rational coefficient and atom powers.
fn term_parts(t: &Expr) -> Option<(Rat, Vec<(Expr, u32)>)> {
    let factors: Vec<Expr> = match t {
        Expr::Mul(v) => v.clone(),
        other => vec![other.clone()],
    };
    let mut c = Rat::ONE;
    let mut out = Vec::new();
    for f in factors {
        match &f {
            Expr::Num(r) => c = c.mul(*r)?,
            Expr::Float(_) => return None,
            Expr::List(_) | Expr::Matrix(_) | Expr::Eq(..) | Expr::Rel(..) | Expr::Text(_) => return None,
            Expr::Pow(b, e) if !b.is_number() => {
                if let Some(k) = e.as_rat().filter(|r| r.is_int() && r.n >= 1 && r.n <= MAX_DEG as i128) {
                    out.push(((**b).clone(), k.n as u32));
                    continue;
                }
                // b^(k·u) with integer k > 1  ->  (b^u)^k
                if let Expr::Mul(ev) = e.as_ref() {
                    if let Some(Expr::Num(k)) = ev.first() {
                        if k.is_int() && k.n > 1 && k.n <= 64 {
                            let rest = if ev.len() == 2 { ev[1].clone() } else { Expr::Mul(ev[1..].to_vec()) };
                            out.push((Expr::Pow(b.clone(), Box::new(rest)), k.n as u32));
                            continue;
                        }
                    }
                }
                out.push((f.clone(), 1));
            }
            _ => out.push((f.clone(), 1)),
        }
    }
    Some((c, out))
}

fn collect_atoms(e: &Expr, atoms: &mut Vec<Expr>) -> Option<()> {
    for t in terms_of(e) {
        let (_, parts) = term_parts(&t)?;
        for (a, _) in parts {
            if !atoms.contains(&a) {
                atoms.push(a);
            }
        }
    }
    Some(())
}

/// Sorted atom list shared by several (expanded) expressions.
fn atoms_of(es: &[&Expr]) -> Option<Vec<Expr>> {
    let mut atoms = Vec::new();
    for e in es {
        collect_atoms(e, &mut atoms)?;
    }
    atoms.sort_by(cmp_expr);
    Some(atoms)
}

fn mp_norm(mut p: MPoly) -> Option<MPoly> {
    p.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out: MPoly = Vec::with_capacity(p.len());
    for (e, c) in p {
        if let Some(last) = out.last_mut() {
            if last.0 == e {
                last.1 = last.1.add(c)?;
                continue;
            }
        }
        out.push((e, c));
    }
    out.retain(|t| !t.1.is_zero());
    Some(out)
}

/// Expanded expression -> MPoly over `atoms` (must contain every atom).
fn mp_from(e: &Expr, atoms: &[Expr]) -> Option<MPoly> {
    let mut out = Vec::new();
    for t in terms_of(e) {
        if t.is_zero() {
            continue;
        }
        let (c, parts) = term_parts(&t)?;
        let mut ex = vec![0u32; atoms.len()];
        for (a, k) in parts {
            let i = atoms.iter().position(|x| *x == a)?;
            ex[i] += k;
        }
        out.push((ex, c));
    }
    mp_norm(out)
}

fn mp_to_expr(p: &MPoly, atoms: &[Expr]) -> R {
    let mut terms = Vec::with_capacity(p.len());
    for (ex, c) in p {
        let mut fs = vec![num(*c)];
        for (i, k) in ex.iter().enumerate() {
            if *k > 0 {
                fs.push(pow(atoms[i].clone(), Expr::int(*k as i128))?);
            }
        }
        terms.push(mul(fs)?);
    }
    add(terms)
}

fn mp_const(c: Rat, n: usize) -> MPoly {
    if c.is_zero() {
        vec![]
    } else {
        vec![(vec![0; n], c)]
    }
}

fn mp_sub(a: &MPoly, b: &MPoly) -> Option<MPoly> {
    let mut v = a.clone();
    for (e, c) in b {
        v.push((e.clone(), c.neg()?));
    }
    mp_norm(v)
}

fn mp_mul(a: &MPoly, b: &MPoly) -> Option<MPoly> {
    let mut v = Vec::with_capacity(a.len() * b.len());
    if a.len() * b.len() > 200_000 {
        return None;
    }
    for (e1, c1) in a {
        for (e2, c2) in b {
            let e: Exps = e1.iter().zip(e2).map(|(x, y)| x + y).collect();
            v.push((e, c1.mul(*c2)?));
        }
    }
    mp_norm(v)
}

fn mp_pow(a: &MPoly, k: usize, n: usize) -> Option<MPoly> {
    let mut acc = mp_const(Rat::ONE, n);
    for _ in 0..k {
        acc = mp_mul(&acc, a)?;
    }
    Some(acc)
}

/// Exact division (lex order); `None` if `b` does not divide `a`.
fn mp_div_exact(a: &MPoly, b: &MPoly) -> Option<MPoly> {
    let (be, bc) = b.first()?.clone();
    let mut r = a.clone();
    let mut q: MPoly = Vec::new();
    let mut guard = 0;
    while let Some((le, lc)) = r.first().cloned() {
        guard += 1;
        if guard > 20000 {
            return None;
        }
        if le.iter().zip(&be).any(|(x, y)| x < y) {
            return None;
        }
        let e: Exps = le.iter().zip(&be).map(|(x, y)| x - y).collect();
        let c = lc.div(bc)?;
        let t = vec![(e, c)];
        q.push(t[0].clone());
        r = mp_sub(&r, &mp_mul(&t, b)?)?;
    }
    mp_norm(q)
}

/// Display order of factors of equal degree: (x - 3) before (x - 2) before x + 2.
fn cmp_mpoly(a: &MPoly, b: &MPoly) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let c = y.0.cmp(&x.0).then(x.1.cmp(&y.1));
        if c != Ordering::Equal {
            return c;
        }
    }
    a.len().cmp(&b.len())
}

fn mp_total_deg(p: &MPoly) -> u32 {
    p.iter().map(|(e, _)| e.iter().sum::<u32>()).max().unwrap_or(0)
}

fn mp_active(p: &MPoly, n: usize) -> Vec<usize> {
    (0..n).filter(|i| p.iter().any(|(e, _)| e[*i] > 0)).collect()
}

/// Univariate view in atom `k` (requires all other exponents zero).
fn mp_to_q(p: &MPoly, k: usize) -> Option<Q> {
    let deg = p.iter().map(|(e, _)| e[k]).max().unwrap_or(0) as usize;
    let mut q = vec![Rat::ZERO; deg + 1];
    for (e, c) in p {
        if e.iter().enumerate().any(|(i, v)| i != k && *v > 0) {
            return None;
        }
        q[e[k] as usize] = q[e[k] as usize].add(*c)?;
    }
    Some(q_trim(q))
}

fn q_to_mp(q: &Q, k: usize, n: usize) -> Option<MPoly> {
    let mut v = Vec::new();
    for (i, c) in q.iter().enumerate() {
        let mut e = vec![0; n];
        e[k] = i as u32;
        v.push((e, *c));
    }
    mp_norm(v)
}

/// Rational content with the sign of the leading coefficient.
fn mp_content(p: &MPoly) -> Option<Rat> {
    let mut g = Rat::ZERO;
    for (_, c) in p {
        g = gcd_rat(g, *c)?;
    }
    if g.is_zero() {
        return Some(Rat::ONE);
    }
    if p.first()?.1.is_neg() {
        g = g.neg()?;
    }
    Some(g)
}

fn rat_sqrt(r: Rat) -> Option<Rat> {
    if r.is_neg() {
        return None;
    }
    let sn = (r.n as f64).sqrt().round() as i128;
    let sd = (r.d as f64).sqrt().round() as i128;
    if sn.checked_mul(sn)? == r.n && sd.checked_mul(sd)? == r.d {
        Rat::new(sn, sd)
    } else {
        None
    }
}

/// Factors `p` into (constant, [(primitive factor, multiplicity)]).
fn mp_factor(p: &MPoly, n: usize, depth: usize) -> Option<(Rat, Vec<(MPoly, usize)>)> {
    if p.is_empty() {
        return Some((Rat::ZERO, vec![]));
    }
    let c = mp_content(p)?;
    let mut prim: MPoly = p.iter().map(|(e, x)| Some((e.clone(), x.div(c)?))).collect::<Option<_>>()?;
    let mut coef = c;
    let mut out: Vec<(MPoly, usize)> = Vec::new();
    // monomial content
    let mut mins = vec![u32::MAX; n];
    for (e, _) in &prim {
        for i in 0..n {
            mins[i] = mins[i].min(e[i]);
        }
    }
    for i in 0..n {
        if mins[i] > 0 && mins[i] != u32::MAX {
            let mut e = vec![0; n];
            e[i] = 1;
            out.push((vec![(e, Rat::ONE)], mins[i] as usize));
            for t in prim.iter_mut() {
                t.0[i] -= mins[i];
            }
        }
    }
    let active = mp_active(&prim, n);
    let push_all = |out: &mut Vec<(MPoly, usize)>, coef: &mut Rat, sub: (Rat, Vec<(MPoly, usize)>), mult: usize| -> Option<()> {
        *coef = coef.mul(sub.0.powi(mult as i64)?)?;
        for (f, m) in sub.1 {
            out.push((f, m * mult));
        }
        Some(())
    };
    if active.is_empty() {
        // constant 1
    } else if active.len() == 1 {
        let k = active[0];
        let q = mp_to_q(&prim, k)?;
        let (c2, fs) = q_factor(&q)?;
        coef = coef.mul(c2)?;
        for (f, m) in fs {
            out.push((q_to_mp(&f, k, n)?, m));
        }
    } else if active.len() == 2 && {
        let d0 = prim[0].0.iter().sum::<u32>();
        prim.iter().all(|(e, _)| e.iter().sum::<u32>() == d0)
    } {
        // homogeneous in two atoms: dehomogenize (b = 1), factor, homogenize
        let (a, b) = (active[0], active[1]);
        let d = prim[0].0.iter().sum::<u32>();
        let mut q = vec![Rat::ZERO; d as usize + 1];
        for (e, x) in &prim {
            q[e[a] as usize] = q[e[a] as usize].add(*x)?;
        }
        let (c2, fs) = q_factor(&q_trim(q))?;
        coef = coef.mul(c2)?;
        for (f, m) in fs {
            let k = f.len() - 1;
            let mut v = Vec::new();
            for (j, x) in f.iter().enumerate() {
                let mut e = vec![0; n];
                e[a] = j as u32;
                e[b] = (k - j) as u32;
                v.push((e, *x));
            }
            out.push((mp_norm(v)?, m));
        }
    } else if prim.len() == 2 && depth < 6 {
        // difference of squares A^2 - B^2 = (A - B)(A + B)
        let (e1, c1) = prim[0].clone();
        let (e2, c2) = prim[1].clone();
        let even = e1.iter().chain(&e2).all(|k| k % 2 == 0);
        let done = if even && !c1.is_neg() && c2.is_neg() {
            match (rat_sqrt(c1), rat_sqrt(c2.neg()?)) {
                (Some(s1), Some(s2)) => {
                    let a: MPoly = vec![(e1.iter().map(|k| k / 2).collect(), s1)];
                    let b: MPoly = vec![(e2.iter().map(|k| k / 2).collect(), s2)];
                    let f1 = mp_sub(&a, &b)?;
                    let f2 = mp_norm(a.iter().chain(&b).cloned().collect())?;
                    let r1 = mp_factor(&f1, n, depth + 1)?;
                    let r2 = mp_factor(&f2, n, depth + 1)?;
                    push_all(&mut out, &mut coef, r1, 1)?;
                    push_all(&mut out, &mut coef, r2, 1)?;
                    true
                }
                _ => false,
            }
        } else {
            false
        };
        if !done {
            out.push((prim.clone(), 1));
        }
    } else {
        // try perfect powers via repeated exact square roots is costly; keep as is
        out.push((std::mem::take(&mut prim), 1));
    }
    // merge equal factors
    let mut merged: Vec<(MPoly, usize)> = Vec::new();
    for (f, m) in out {
        if f.len() == 1 && f[0].0.iter().all(|k| *k == 0) {
            coef = coef.mul(f[0].1.powi(m as i64)?)?;
            continue;
        }
        if let Some(x) = merged.iter_mut().find(|x| x.0 == f) {
            x.1 += m;
        } else {
            merged.push((f, m));
        }
    }
    Some((coef, merged))
}

/// Factor list of a polynomial expression in its atoms.
fn factor_list(e: &Expr, atoms: &[Expr]) -> Option<(Rat, Vec<(MPoly, usize)>)> {
    let mp = mp_from(e, atoms)?;
    mp_factor(&mp, atoms.len(), 0)
}

/// Polynomial gcd of two MPolys (exact for univariate, factor-based otherwise).
fn mp_gcd(a: &MPoly, b: &MPoly, n: usize) -> Option<MPoly> {
    if a.is_empty() {
        return Some(b.clone());
    }
    if b.is_empty() {
        return Some(a.clone());
    }
    let mut act = mp_active(a, n);
    for i in mp_active(b, n) {
        if !act.contains(&i) {
            act.push(i);
        }
    }
    if act.is_empty() {
        return Some(mp_const(Rat::ONE, n));
    }
    if act.len() == 1 {
        let k = act[0];
        let g = q_gcd(&mp_to_q(a, k)?, &mp_to_q(b, k)?)?;
        return q_to_mp(&g, k, n);
    }
    let (_, fa) = mp_factor(a, n, 0)?;
    let (_, fb) = mp_factor(b, n, 0)?;
    let mut g = mp_const(Rat::ONE, n);
    for (f, m) in &fa {
        if let Some((_, m2)) = fb.iter().find(|x| x.0 == *f) {
            g = mp_mul(&g, &mp_pow(f, (*m).min(*m2), n)?)?;
        }
    }
    Some(g)
}

// ======================================================= public polynomial API

/// Builds a display product `coef · Π f^m` without re-canonicalizing
/// (so the factored form is not expanded back).
fn build_product(coef: Rat, mut factors: Vec<(Expr, i128, u32)>) -> Expr {
    // stable: callers pass factors already ordered within each degree
    factors.sort_by(|a, b| (a.1 < 0).cmp(&(b.1 < 0)).then(matches!(a.0, Expr::Add(_)).cmp(&matches!(b.0, Expr::Add(_)))).then(a.2.cmp(&b.2)));
    let mut items: Vec<Expr> = factors
        .into_iter()
        .map(|(f, m, _)| if m == 1 { f } else { Expr::Pow(Box::new(f), Box::new(Expr::int(m))) })
        .collect();
    if items.is_empty() {
        return num(coef);
    }
    if coef.is_one() && items.len() == 1 {
        return items.pop().unwrap();
    }
    let mut v = Vec::with_capacity(items.len() + 1);
    if !coef.is_one() {
        v.push(num(coef));
    }
    v.extend(items);
    Expr::Mul(v)
}

/// Monic gcd of two polynomials (in all their symbols).
pub fn poly_gcd_expr(cas: &Cas, a: &Expr, b: &Expr) -> R {
    let ea = expand(a)?;
    let eb = expand(b)?;
    let atoms = atoms_of(&[&ea, &eb]).ok_or("gcd: se esperaban polinomios con coeficientes racionales")?;
    let n = atoms.len();
    let pa = mp_from(&ea, &atoms).ok_or("gcd: se esperaban polinomios")?;
    let pb = mp_from(&eb, &atoms).ok_or("gcd: se esperaban polinomios")?;
    let g = mp_gcd(&pa, &pb, n).ok_or("gcd: cálculo demasiado grande")?;
    if g.is_empty() {
        return Ok(Expr::zero());
    }
    // monic (leading coefficient 1)
    let lc = g[0].1;
    let g: MPoly = g.into_iter().map(|(e, c)| (e, c.div(lc).unwrap_or(c))).collect();
    let r = mp_to_expr(&g, &atoms)?;
    cas.eval(&r)
}

/// Factors polynomials and rational functions over ℚ.
pub fn factor(cas: &Cas, e: &Expr, var: Option<&Expr>) -> R {
    match e {
        Expr::List(v) => return Ok(Expr::List(v.iter().map(|x| factor(cas, x, var)).collect::<Result<_, _>>()?)),
        Expr::Eq(a, b) => return Ok(Expr::Eq(Box::new(factor(cas, a, var)?), Box::new(factor(cas, b, var)?))),
        Expr::Matrix(m) => return Ok(Expr::Matrix(matrix::map_m(m, |x| factor(cas, x, var))?)),
        Expr::Text(_) | Expr::Rel(..) => return Ok(e.clone()),
        _ => {}
    }
    if e.contains_float() || !e.has_free_symbols() && !matches!(e, Expr::Add(_) | Expr::Mul(_)) {
        return Ok(e.clone());
    }
    Ok(factor_inner(cas, e).unwrap_or_else(|| e.clone()))
}

fn factor_inner(cas: &Cas, e: &Expr) -> Option<Expr> {
    let (n, d) = together(cas, e).ok()?;
    let n = expand(&n).ok()?;
    let d = expand(&d).ok()?;
    let atoms = atoms_of(&[&n, &d])?;
    if atoms.is_empty() {
        return None;
    }
    let na = atoms.len();
    let (cn, fnum) = factor_list(&n, &atoms)?;
    let (cd, fden) = factor_list(&d, &atoms)?;
    if cd.is_zero() {
        return None;
    }
    let coef = cn.div(cd)?;
    let mut fs: Vec<(MPoly, i128)> = fnum.into_iter().map(|(f, m)| (f, m as i128)).collect();
    for (f, m) in fden {
        if let Some(x) = fs.iter_mut().find(|x| x.0 == f) {
            x.1 -= m as i128;
        } else {
            fs.push((f, -(m as i128)));
        }
    }
    fs.sort_by(|a, b| cmp_mpoly(&a.0, &b.0));
    let mut items = Vec::new();
    for (f, m) in fs {
        if m == 0 {
            continue;
        }
        let deg = mp_total_deg(&f);
        let fe = mp_to_expr(&f, &atoms).ok()?;
        items.push((fe, m, deg));
    }
    let _ = na;
    // trivial factorization: keep the original form
    if items.len() == 1 && items[0].1 == 1 && (coef.is_one() || coef == Rat::MINUS_ONE) {
        return cas.eval(&mul2(num(coef), items[0].0.clone()).ok()?).ok();
    }
    Some(build_product(coef, items))
}

/// Splits `e` into (numerator, denominator), combining sums over a common
/// denominator. Non-rational sub-expressions are kept as atoms.
fn together(cas: &Cas, e: &Expr) -> Result<(Expr, Expr), String> {
    together_d(cas, e, 0)
}

fn together_d(cas: &Cas, e: &Expr, depth: usize) -> Result<(Expr, Expr), String> {
    if depth > 40 {
        return Ok((e.clone(), Expr::one()));
    }
    Ok(match e {
        Expr::Num(r) if !r.is_int() => (Expr::int(r.n), Expr::int(r.d)),
        Expr::Add(v) => {
            let mut n = Expr::zero();
            let mut d = Expr::one();
            for t in v {
                let (tn, td) = together_d(cas, t, depth + 1)?;
                if td == d {
                    n = add2(n, tn)?;
                } else if td.is_one() {
                    n = add2(n, mul2(tn, d.clone())?)?;
                } else if d.is_one() {
                    n = add2(mul2(n, td.clone())?, tn)?;
                    d = td;
                } else {
                    // reuse common factors of the denominators
                    let (dd, tdd) = split_common(&d, &td);
                    n = add2(mul2(n, tdd.clone())?, mul2(tn, dd)?)?;
                    d = mul2(d, tdd)?;
                }
            }
            (n, d)
        }
        Expr::Mul(v) => {
            let mut ns = Vec::new();
            let mut ds = Vec::new();
            for f in v {
                let (a, b) = together_d(cas, f, depth + 1)?;
                ns.push(a);
                ds.push(b);
            }
            (mul(ns)?, mul(ds)?)
        }
        Expr::Pow(b, k) => match k.as_rat() {
            Some(r) if r.is_int() && r.n != 0 && r.n.abs() <= 64 => {
                let (bn, bd) = together_d(cas, b, depth + 1)?;
                let m = Expr::int(r.n.abs());
                if r.n > 0 {
                    (pow(bn, m.clone())?, pow(bd, m)?)
                } else {
                    (pow(bd, m.clone())?, pow(bn, m)?)
                }
            }
            Some(r) if r.is_neg() => (Expr::one(), pow((**b).clone(), num(r.neg().ok_or("desbordamiento")?))?),
            _ => (e.clone(), Expr::one()),
        },
        _ => (e.clone(), Expr::one()),
    })
}

/// For two denominators (products), returns (d1 / common, d2 / common).
fn split_common(d1: &Expr, d2: &Expr) -> (Expr, Expr) {
    let f1: Vec<Expr> = match d1 {
        Expr::Mul(v) => v.clone(),
        o => vec![o.clone()],
    };
    let mut f2: Vec<Expr> = match d2 {
        Expr::Mul(v) => v.clone(),
        o => vec![o.clone()],
    };
    let mut rest1 = Vec::new();
    for f in f1 {
        if let Some(i) = f2.iter().position(|g| *g == f) {
            f2.remove(i);
        } else {
            rest1.push(f);
        }
    }
    let mk = |v: Vec<Expr>| mul(v).unwrap_or(Expr::one());
    (mk(rest1), mk(f2))
}

/// Rational function normalization: single fraction, common factors removed.
pub fn cancel(cas: &Cas, e: &Expr) -> R {
    match e {
        Expr::List(v) => return Ok(Expr::List(v.iter().map(|x| cancel(cas, x)).collect::<Result<_, _>>()?)),
        Expr::Eq(a, b) => return Ok(Expr::Eq(Box::new(cancel(cas, a)?), Box::new(cancel(cas, b)?))),
        Expr::Matrix(m) => return Ok(Expr::Matrix(matrix::map_m(m, |x| cancel(cas, x))?)),
        Expr::Text(_) | Expr::Rel(..) => return Ok(e.clone()),
        _ => {}
    }
    match cancel_inner(cas, e) {
        Ok(Some(r)) => Ok(r),
        _ => Ok(e.clone()),
    }
}

fn cancel_inner(cas: &Cas, e: &Expr) -> Result<Option<Expr>, String> {
    let (n, d) = together(cas, e)?;
    let n = expand(&n)?;
    let d = expand(&d)?;
    if d.is_zero() {
        return Err(crate::simplify::ERR_DIV0.into());
    }
    if d.is_number() {
        return Ok(Some(expand(&div(n, d)?)?));
    }
    let atoms = match atoms_of(&[&n, &d]) {
        Some(a) => a,
        None => return Ok(Some(div(n, d)?)),
    };
    let na = atoms.len();
    let (pn, pd) = match (mp_from(&n, &atoms), mp_from(&d, &atoms)) {
        (Some(a), Some(b)) => (a, b),
        _ => return Ok(Some(div(n, d)?)),
    };
    let g = mp_gcd(&pn, &pd, na).unwrap_or_else(|| mp_const(Rat::ONE, na));
    let (pn, pd) = match (mp_div_exact(&pn, &g), mp_div_exact(&pd, &g)) {
        (Some(a), Some(b)) if !g.is_empty() => (a, b),
        _ => (pn, pd),
    };
    // normalize numeric contents; denominator's displayed leading term positive
    let cn = mp_content(&pn).unwrap_or(Rat::ONE);
    let mut cd = mp_content(&pd).unwrap_or(Rat::ONE);
    let n2: MPoly = pn.iter().map(|(e, c)| (e.clone(), c.div(cn).unwrap_or(*c))).collect();
    let mut d2: MPoly = pd.iter().map(|(e, c)| (e.clone(), c.div(cd).unwrap_or(*c))).collect();
    let mut de = mp_to_expr(&d2, &atoms)?;
    let lead = terms_of(&de).into_iter().fold((f64::NEG_INFINITY, Expr::one()), |acc, t| {
        let dd = display_degree(&t);
        if dd > acc.0 {
            (dd, t)
        } else {
            acc
        }
    });
    if crate::expr::negate_for_print(&lead.1).0 {
        cd = cd.neg().unwrap_or(cd);
        d2 = d2.into_iter().map(|(e, c)| (e, c.neg().unwrap_or(c))).collect();
        de = mp_to_expr(&d2, &atoms)?;
    }
    let ne = mp_to_expr(&n2, &atoms)?;
    let coef = cn.div(cd).ok_or("desbordamiento")?;
    if de.is_one() {
        return Ok(Some(mul2(num(coef), ne)?));
    }
    Ok(Some(mul(vec![num(coef), ne, pow(de, Expr::int(-1))?])?))
}

/// Exact square root of a polynomial expression, if it is a perfect square.
fn poly_sqrt(e: &Expr, depth: usize) -> Option<Expr> {
    let e = expand(e).ok()?;
    if e.is_zero() {
        return Some(Expr::zero());
    }
    if let Expr::Num(r) = e {
        return rat_sqrt(r).map(num);
    }
    if depth > 6 {
        return None;
    }
    let syms = free_symbols(&e);
    let v = syms.first()?;
    let c = poly_coeffs(&e, v)?;
    let deg = c.len() - 1;
    if deg % 2 != 0 || deg > 40 {
        return None;
    }
    let m = deg / 2;
    let top = poly_sqrt(&c[deg], depth + 1)?;
    if top.is_zero() {
        return None;
    }
    let mut r = vec![Expr::zero(); m + 1];
    r[m] = top.clone();
    let two_top = mul2(Expr::int(2), top).ok()?;
    for d in (m..2 * m).rev() {
        let k = d - m;
        let mut s = Vec::new();
        for i in (k + 1)..m {
            let j = d - i;
            if j > k && j < m {
                s.push(mul2(r[i].clone(), r[j].clone()).ok()?);
            }
        }
        let rest = sub(c[d].clone(), add(s).ok()?).ok()?;
        r[k] = expand(&div(rest, two_top.clone()).ok()?).ok()?;
    }
    let root = poly_from_coeffs(&r, v).ok()?;
    let chk = expand(&sub(pow(root.clone(), Expr::int(2)).ok()?, e.clone()).ok()?).ok()?;
    if chk.is_zero() {
        Some(root)
    } else {
        None
    }
}

/// `coeffs`, `degree`, `quotient`, `remainder`.
pub fn cmd_poly(cas: &Cas, name: &str, args: Vec<Expr>) -> R {
    let need = if matches!(name, "quotient" | "remainder") { 2 } else { 1 };
    if args.len() < need || args.len() > need + 1 {
        return Err(format!("{name} requiere {need} o {} argumentos", need + 1));
    }
    let mut all = args[0].clone();
    if need == 2 {
        all = Expr::List(vec![args[0].clone(), args[1].clone()]);
    }
    let x = pick_var(&all, args.get(need))?;
    let not_poly = |e: &Expr| format!("{name}: {} no es un polinomio en {x}", e);
    let ca = poly_coeffs(&args[0], &x).ok_or_else(|| not_poly(&args[0]))?;
    match name {
        "coeffs" => {
            let v: Vec<Expr> = ca.iter().rev().map(|c| cas.eval(c)).collect::<Result<_, _>>()?;
            Ok(Expr::List(v))
        }
        "degree" => {
            if ca.len() == 1 && ca[0].is_zero() {
                return Ok(Expr::Text("-∞".into()));
            }
            Ok(Expr::int(ca.len() as i128 - 1))
        }
        _ => {
            let cb = poly_coeffs(&args[1], &x).ok_or_else(|| not_poly(&args[1]))?;
            let (q, r) = match (q_from_exprs(&ca), q_from_exprs(&cb)) {
                (Some(a), Some(b)) => {
                    let (q, r) = q_divmod(&a, &b).ok_or("División entre el polinomio cero o desbordamiento")?;
                    (q_to_exprs(&q), q_to_exprs(&r))
                }
                _ => poly_divmod(&ca, &cb)?,
            };
            let res = if name == "quotient" { q } else { r };
            let p = poly_from_coeffs(&res, &x)?;
            cas.eval(&p)
        }
    }
}

// ======================================================= simplify

/// Tries several rewrites (expand, cancel, factor, trig and log identities)
/// and returns the smallest equivalent form (ties keep the original).
pub fn simplify_full(cas: &Cas, e: &Expr) -> R {
    match e {
        Expr::List(v) => return Ok(Expr::List(v.iter().map(|x| simplify_full(cas, x)).collect::<Result<_, _>>()?)),
        Expr::Matrix(m) => return Ok(Expr::Matrix(matrix::map_m(m, |x| simplify_full(cas, x))?)),
        Expr::Eq(a, b) => return Ok(Expr::Eq(Box::new(simplify_full(cas, a)?), Box::new(simplify_full(cas, b)?))),
        Expr::Rel(o, a, b) => return Ok(Expr::Rel(o.clone(), Box::new(simplify_full(cas, a)?), Box::new(simplify_full(cas, b)?))),
        Expr::Text(_) | Expr::Num(_) | Expr::Float(_) | Expr::Sym(_) => return Ok(e.clone()),
        _ => {}
    }
    let mut best = e.clone();
    let mut consider = |c: Option<Expr>| {
        if let Some(c) = c {
            if !matches!(c, Expr::Text(_)) && (c.size(), exp_weight(&c)) < (best.size(), exp_weight(&best)) {
                best = c;
            }
        }
    };
    let expanded = expand(e).ok().and_then(|x| cas.eval(&x).ok());
    consider(expanded.clone());
    let trig = trig_pass(cas, e, 0).ok();
    consider(trig.clone());
    if let Some(ex) = &expanded {
        consider(trig_pass(cas, ex, 0).ok());
    }
    consider(cancel(cas, e).ok());
    if let Some(t) = &trig {
        consider(cancel(cas, t).ok());
    }
    consider(log_pass(cas, e, 0).ok());
    let factored = factor(cas, e, None).ok();
    consider(factored.clone());
    if let Some(f) = &factored {
        // identities hidden inside factors: sin^4 - cos^4 = (sin^2 + cos^2)(...)
        if let Ok(t) = trig_pass(cas, f, 0) {
            consider(expand(&t).ok().and_then(|x| cas.eval(&x).ok()));
            consider(Some(t));
        }
    }
    if let Some(ex) = &expanded {
        if let Some(t) = trig_pass(cas, ex, 0).ok() {
            consider(cancel(cas, &t).ok());
        }
    }
    Ok(best)
}

/// Secondary simplicity measure: total magnitude of integer exponents.
fn exp_weight(e: &Expr) -> i128 {
    match e {
        Expr::Pow(b, k) => exp_weight(b) + k.as_int().map_or(1, |n| n.abs().min(1000)) + exp_weight(k),
        Expr::Add(v) | Expr::Mul(v) | Expr::List(v) | Expr::Call(_, v) => v.iter().map(exp_weight).sum(),
        _ => 0,
    }
}

/// Returns (argument, rest) if `t = rest · fname(arg)^2`.
fn extract_sq(t: &Expr, fname: &str) -> Option<(Expr, Expr)> {
    let is_sq = |f: &Expr| -> Option<Expr> {
        if let Expr::Pow(b, k) = f {
            if k.as_int() == Some(2) {
                if let Expr::Call(n, a) = b.as_ref() {
                    if n == fname && a.len() == 1 {
                        return Some(a[0].clone());
                    }
                }
            }
        }
        None
    };
    match t {
        Expr::Mul(v) => {
            for (i, f) in v.iter().enumerate() {
                if let Some(u) = is_sq(f) {
                    let mut rest = v.clone();
                    rest.remove(i);
                    return Some((u, mul(rest).ok()?));
                }
            }
            None
        }
        other => is_sq(other).map(|u| (u, Expr::one())),
    }
}

fn sq_call(f: &str, u: &Expr) -> R {
    pow(Expr::call(f, vec![u.clone()]), Expr::int(2))
}

/// Pythagorean identities, applied bottom-up.
fn trig_pass(cas: &Cas, e: &Expr, depth: usize) -> R {
    if depth > 60 {
        return Ok(e.clone());
    }
    let e = match e {
        Expr::Add(v) => add(v.iter().map(|x| trig_pass(cas, x, depth + 1)).collect::<Result<_, _>>()?)?,
        Expr::Mul(v) => mul(v.iter().map(|x| trig_pass(cas, x, depth + 1)).collect::<Result<_, _>>()?)?,
        Expr::Pow(b, k) => pow(trig_pass(cas, b, depth + 1)?, trig_pass(cas, k, depth + 1)?)?,
        Expr::Call(n, a) => cas.apply(n, a.iter().map(|x| trig_pass(cas, x, depth + 1)).collect::<Result<_, _>>()?)?,
        other => other.clone(),
    };
    let mut terms = match &e {
        Expr::Add(v) => v.clone(),
        _ => return Ok(e),
    };
    let mut changed = true;
    let mut guard = 0;
    while changed && guard < 50 {
        changed = false;
        guard += 1;
        'outer: for i in 0..terms.len() {
            // a·sin²u + a·cos²u -> a
            if let Some((u, rest)) = extract_sq(&terms[i], "sin") {
                for j in 0..terms.len() {
                    if j == i {
                        continue;
                    }
                    if let Some((u2, rest2)) = extract_sq(&terms[j], "cos") {
                        if u2 == u && rest2 == rest {
                            let (a, b) = (i.max(j), i.min(j));
                            terms.remove(a);
                            terms.remove(b);
                            terms.push(rest);
                            changed = true;
                            break 'outer;
                        }
                    }
                }
            }
            // k - k·sin²u -> k·cos²u ; k - k·cos²u -> k·sin²u ; k + k·tan²u -> k·sec²u
            for (f, g, sign) in [("sin", "cos", -1i128), ("cos", "sin", -1), ("tan", "sec", 1)] {
                if let Some((u, rest)) = extract_sq(&terms[i], f) {
                    let target = mul2(Expr::int(sign), rest.clone())?;
                    if let Some(j) = terms.iter().position(|t| *t == target && t.is_number()) {
                        if j != i {
                            let newt = mul2(target.clone(), sq_call(g, &u)?)?;
                            let (a, b) = (i.max(j), i.min(j));
                            terms.remove(a);
                            terms.remove(b);
                            terms.push(newt);
                            changed = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
    add(terms)
}

/// ln(a) + ln(b) -> ln(a·b) (integer multiples become powers).
fn log_pass(cas: &Cas, e: &Expr, depth: usize) -> R {
    if depth > 60 {
        return Ok(e.clone());
    }
    let e = match e {
        Expr::Add(v) => add(v.iter().map(|x| log_pass(cas, x, depth + 1)).collect::<Result<_, _>>()?)?,
        Expr::Mul(v) => mul(v.iter().map(|x| log_pass(cas, x, depth + 1)).collect::<Result<_, _>>()?)?,
        Expr::Call(n, a) => cas.apply(n, a.iter().map(|x| log_pass(cas, x, depth + 1)).collect::<Result<_, _>>()?)?,
        other => other.clone(),
    };
    let terms = match &e {
        Expr::Add(v) => v.clone(),
        _ => return Ok(e),
    };
    let mut args = Vec::new();
    let mut rest = Vec::new();
    for t in terms {
        let (k, inner) = match &t {
            Expr::Call(n, a) if n == "ln" && a.len() == 1 => (1, a[0].clone()),
            Expr::Mul(v) if v.len() == 2 => match (&v[0], &v[1]) {
                (Expr::Num(r), Expr::Call(n, a)) if n == "ln" && a.len() == 1 && r.is_int() && r.n.abs() <= 8 => (r.n, a[0].clone()),
                _ => (0, Expr::zero()),
            },
            _ => (0, Expr::zero()),
        };
        if k != 0 {
            args.push(pow(inner, Expr::int(k))?);
        } else {
            rest.push(t);
        }
    }
    if args.len() < 2 {
        return Ok(e);
    }
    rest.push(Expr::call("ln", vec![mul(args)?]));
    add(rest)
}

// ======================================================= polynomial roots

/// Exact quadratic roots for rational coefficients (`a x^2 + b x + c`).
fn quad_exact(a: Rat, b: Rat, c: Rat, with_complex: bool) -> Option<Vec<Expr>> {
    let two_a = a.mul(Rat::int(2))?;
    let h = b.neg()?.div(two_a)?;
    let disc = b.mul(b)?.sub(Rat::int(4).mul(a)?.mul(c)?)?;
    let d4 = disc.div(two_a.mul(two_a)?)?;
    if d4.is_zero() {
        return Some(vec![num(h)]);
    }
    if d4.is_neg() && !with_complex {
        return Some(vec![]);
    }
    let s = pow(num(d4), Expr::rat(1, 2)).ok()?;
    Some(vec![sub(num(h), s.clone()).ok()?, add2(num(h), s).ok()?])
}

fn numeric_roots_c(coeffs_high: &[C], with_complex: bool) -> Vec<Expr> {
    let mut out = Vec::new();
    for z in numeric::poly_roots(coeffs_high) {
        if is_real_val(z) || z.im.abs() < 1e-9 * (1.0 + z.abs()) {
            out.push(Expr::Float(clean_float(z.re)));
        } else if with_complex {
            out.push(from_c(C::new(clean_float(z.re), z.im)));
        }
    }
    out
}

/// Exact roots of a square-free rational polynomial without rational roots.
fn q_solve_rest(cas: &Cas, g: &Q, with_complex: bool, depth: usize) -> Vec<Expr> {
    let deg = g.len().saturating_sub(1);
    match deg {
        0 => return vec![],
        1 => return g[0].neg().and_then(|x| x.div(g[1])).map(|r| vec![num(r)]).unwrap_or_default(),
        2 => {
            if let Some(v) = quad_exact(g[2], g[1], g[0], with_complex) {
                return v;
            }
        }
        _ => {}
    }
    if depth < 3 {
        // factor into lower degree pieces (quartic split, even polynomials)
        if let Some(parts) = q_split_irreducible(g, 0) {
            if parts.len() > 1 {
                return parts.iter().flat_map(|p| q_solve_rest(cas, p, with_complex, depth + 1)).collect();
            }
        }
        // binomial a x^n + b
        if g[1..deg].iter().all(|c| c.is_zero()) {
            if let Some(r) = g[0].neg().and_then(|x| x.div(g[deg])) {
                let mut out = Vec::new();
                let root = pow(num(r), Expr::rat(1, deg as i128));
                if let Ok(root) = root {
                    if deg % 2 == 1 {
                        out.push(root);
                    } else if !r.is_neg() {
                        if let Ok(nr) = neg(root.clone()) {
                            out.push(nr);
                        }
                        out.push(root);
                    }
                }
                if with_complex {
                    for z in numeric_roots_c(&q_to_c_high(g), true) {
                        if !matches!(z, Expr::Float(_)) {
                            out.push(z);
                        }
                    }
                }
                return out;
            }
        }
        // even: g(x) = h(x^2) with deg h <= 2
        if deg == 4 && g[1].is_zero() && g[3].is_zero() {
            let h: Q = vec![g[0], g[2], g[4]];
            if let Some(hr) = quad_exact(h[2], h[1], h[0], true) {
                let mut out = Vec::new();
                for r in hr {
                    match cval(cas, &r) {
                        Some(z) if is_real_val(z) && z.re >= 0.0 => {
                            if let Ok(s) = pow(r.clone(), Expr::rat(1, 2)) {
                                if let Ok(ns) = neg(s.clone()) {
                                    out.push(ns);
                                }
                                out.push(s);
                            }
                        }
                        Some(z) if is_real_val(z) => {
                            if with_complex {
                                if let Ok(s) = mul2(Expr::sym("i"), pow(neg(r.clone()).unwrap_or(r.clone()), Expr::rat(1, 2)).unwrap_or(Expr::zero())) {
                                    if let Ok(ns) = neg(s.clone()) {
                                        out.push(ns);
                                    }
                                    out.push(s);
                                }
                            }
                        }
                        Some(z) => {
                            if with_complex {
                                let w = z.sqrt();
                                out.push(from_c(w.neg()));
                                out.push(from_c(w));
                            }
                        }
                        None => {}
                    }
                }
                return out;
            }
        }
    }
    numeric_roots_c(&q_to_c_high(g), with_complex)
}

/// Roots of a rational polynomial with multiplicities.
fn q_roots(cas: &Cas, p: &Q, with_complex: bool) -> Option<Vec<(Expr, usize)>> {
    let sqf = q_squarefree(p)?;
    let mut out = Vec::new();
    for (f, m) in sqf {
        let roots = q_rational_roots(&f)?;
        let mut g = f.clone();
        for r in roots {
            out.push((num(r), m));
            g = q_divmod(&g, &vec![r.neg()?, Rat::ONE])?.0;
        }
        for v in q_solve_rest(cas, &g, with_complex, 0) {
            out.push((v, m));
        }
    }
    Some(out)
}

/// Roots of a polynomial given by lowest-first coefficients. With
/// `multiplicity`, repeated roots are listed repeatedly.
fn solve_poly(cas: &Cas, c: &[Expr], with_complex: bool, multiplicity: bool) -> Result<Vec<Expr>, String> {
    let c = trim_exprs(c.to_vec());
    let deg = c.len() - 1;
    if deg == 0 {
        return Ok(vec![]);
    }
    // exact rational
    if let Some(q) = q_from_exprs(&c) {
        if let Some(rs) = q_roots(cas, &q, with_complex) {
            let mut out = Vec::new();
            for (v, m) in rs {
                for _ in 0..(if multiplicity { m } else { 1 }) {
                    out.push(v.clone());
                }
            }
            sort_values(cas, &mut out);
            return Ok(out);
        }
    }
    let closed = c.iter().all(|x| !x.has_free_symbols());
    if closed && (c.iter().any(|x| x.contains_float()) || deg > 2) {
        let cs: Vec<C> = c.iter().rev().map(|x| cas.num(x)).collect::<Result<_, _>>()?;
        let mut out = numeric_roots_c(&cs, with_complex);
        if !multiplicity {
            out = dedup_values(cas, out);
        }
        sort_values(cas, &mut out);
        return Ok(out);
    }
    // symbolic (or irrational exact) coefficients
    let mut out = Vec::new();
    let mut c = c;
    let mut zeros = 0;
    while c.len() > 1 && c[0].is_zero() {
        c.remove(0);
        zeros += 1;
    }
    for _ in 0..(if multiplicity { zeros } else { zeros.min(1) }) {
        out.push(Expr::zero());
    }
    let deg = c.len() - 1;
    match deg {
        0 => {}
        1 => out.push(simp(cas, &neg(div(c[0].clone(), c[1].clone())?)?)),
        2 => out.extend(quad_symbolic(cas, &c[2], &c[1], &c[0], with_complex, multiplicity)?),
        _ => {
            if c[1..deg].iter().all(|x| x.is_zero()) {
                // binomial
                let r = simp(cas, &neg(div(c[0].clone(), c[deg].clone())?)?);
                let root = pow(r, Expr::rat(1, deg as i128))?;
                if deg % 2 == 0 {
                    out.push(neg(root.clone())?);
                }
                out.push(root);
            } else if deg == 4 && c[1].is_zero() && c[3].is_zero() {
                for r in quad_symbolic(cas, &c[4], &c[2], &c[0], true, multiplicity)? {
                    let s = pow(r, Expr::rat(1, 2))?;
                    out.push(neg(s.clone())?);
                    out.push(s);
                }
            } else {
                return Err("No se encontró una solución simbólica (grado mayor que 2)".into());
            }
        }
    }
    if !with_complex {
        out.retain(|v| cval(cas, v).map_or(true, is_real_val));
    }
    sort_values(cas, &mut out);
    Ok(out)
}

fn simp(cas: &Cas, e: &Expr) -> Expr {
    let e = cas.eval(e).unwrap_or_else(|_| e.clone());
    if e.has_free_symbols() {
        let c = cancel(cas, &e).unwrap_or_else(|_| e.clone());
        if c.size() <= e.size() {
            return c;
        }
    }
    e
}

/// Square root with the rational content pulled out: √(4 - 4y) = 2√(1 - y).
fn sqrt_with_content(e: &Expr) -> R {
    let mut g = Rat::ZERO;
    let mut ok = true;
    for t in terms_of(e) {
        let c = match &t {
            Expr::Num(r) => *r,
            Expr::Mul(v) => match v.first() {
                Some(Expr::Num(r)) => *r,
                _ => Rat::ONE,
            },
            Expr::Float(_) => {
                ok = false;
                Rat::ONE
            }
            _ => Rat::ONE,
        };
        match gcd_rat(g, c) {
            Some(x) => g = x,
            None => ok = false,
        }
    }
    if !ok || g.is_zero() || g.is_one() {
        return pow(e.clone(), Expr::rat(1, 2));
    }
    let inner = expand(&div(e.clone(), num(g))?)?;
    mul2(pow(num(g), Expr::rat(1, 2))?, pow(inner, Expr::rat(1, 2))?)
}

fn quad_symbolic(cas: &Cas, a: &Expr, b: &Expr, c: &Expr, with_complex: bool, multiplicity: bool) -> Result<Vec<Expr>, String> {
    let disc = expand(&sub(pow(b.clone(), Expr::int(2))?, mul(vec![Expr::int(4), a.clone(), c.clone()])?)?)?;
    let disc = cas.eval(&disc)?;
    let two_a = mul2(Expr::int(2), a.clone())?;
    if disc.is_zero() {
        let r = simp(cas, &div(neg(b.clone())?, two_a)?);
        return Ok(if multiplicity { vec![r.clone(), r] } else { vec![r] });
    }
    if let Some(z) = cval(cas, &disc) {
        if is_real_val(z) && z.re < 0.0 && !with_complex {
            return Ok(vec![]);
        }
    }
    let sq = match poly_sqrt(&disc, 0) {
        Some(s) => s,
        None => sqrt_with_content(&disc)?,
    };
    let r1 = simp(cas, &div(sub(neg(b.clone())?, sq.clone())?, two_a.clone())?);
    let r2 = simp(cas, &div(add2(neg(b.clone())?, sq)?, two_a)?);
    Ok(vec![r1, r2])
}

/// Roots of polynomial `p` in `var` as values (exact when possible), with
/// multiplicity. `with_complex` includes complex roots.
pub fn solve_poly_values(cas: &Cas, p: &Expr, var: &str, with_complex: bool) -> Result<Vec<Expr>, String> {
    let c = poly_coeffs(p, var).ok_or_else(|| format!("La expresión no es un polinomio en {var}"))?;
    solve_poly(cas, &c, with_complex, true)
}

// ======================================================= single equations

enum Sol {
    All,
    Vals(Vec<Expr>),
}

/// Converts `l = r` (or `f`) to `l - r`.
fn to_zero_form(cas: &Cas, e: &Expr) -> R {
    match e {
        Expr::Eq(l, r) => cas.eval(&sub((**l).clone(), (**r).clone())?),
        other => cas.eval(other),
    }
}

fn num_check(cas: &Cas, f: &Expr, x: &str, v: &Expr) -> bool {
    let fv = subst(f, x, v);
    match cas.num(&fv) {
        Ok(z) => {
            if !z.is_finite() {
                return false;
            }
            let scale = cval(cas, v).map_or(1.0, |w| 1.0 + w.abs());
            z.abs() <= 1e-7 * scale
        }
        Err(_) => false,
    }
}

fn denominator_zero(cas: &Cas, d: &Expr, x: &str, v: &Expr) -> bool {
    if d.free_of(x) {
        return false;
    }
    let dv = subst(d, x, v);
    if let Ok(e) = cas.eval(&dv) {
        if e.is_zero() {
            return true;
        }
    }
    match cas.num(&dv) {
        Ok(z) => z.abs() < 1e-12,
        Err(_) => false,
    }
}

/// Values of `x` with `f(x) = 0`.
fn solve_values(cas: &Cas, f: &Expr, x: &str, range: Option<(f64, f64)>, depth: usize) -> Result<Sol, String> {
    if depth > MAX_DEPTH {
        return Err("Ecuación demasiado compleja".into());
    }
    let f = cas.eval(f)?;
    if f.free_of(x) {
        if f.is_zero() {
            return Ok(Sol::All);
        }
        if let Some(z) = cval(cas, &f) {
            if z.abs() < 1e-14 {
                return Ok(Sol::All);
            }
        }
        return Ok(Sol::Vals(vec![]));
    }
    let complex = cas.s.complex;
    let (n, d) = together(cas, &f)?;
    let n = expand(&n).unwrap_or(n);
    let d = expand(&d).unwrap_or(d);
    let mut vals: Vec<Expr> = Vec::new();
    let mut solved = false;
    if n.free_of(x) {
        return Ok(Sol::Vals(vec![]));
    }
    if let Some(c) = poly_coeffs(&n, x) {
        match solve_poly(cas, &c, complex, false) {
            Ok(v) => {
                vals = v;
                solved = true;
            }
            Err(e) => {
                if c.iter().any(|t| t.has_free_symbols()) {
                    return Err(e);
                }
            }
        }
    }
    if !solved && range.is_none() {
        if let Some(v) = isolate(cas, &n, &Expr::zero(), x, depth) {
            vals = v.into_iter().filter(|v| num_check(cas, &f, x, v) || cval(cas, v).is_none()).collect();
            solved = true;
        } else if let Some(v) = kernel_solve(cas, &n, x, depth) {
            vals = v.into_iter().filter(|v| num_check(cas, &f, x, v) || cval(cas, v).is_none()).collect();
            solved = true;
        }
    }
    if !solved {
        if f.free_of_all_but(x) {
            vals = numeric_scan(cas, &f, x, range)?.into_iter().map(|r| recognize_root(cas, r)).collect();
        } else {
            return Err("No se pudo resolver la ecuación simbólicamente".into());
        }
    }
    vals.retain(|v| !denominator_zero(cas, &d, x, v));
    if !complex {
        vals.retain(|v| cval(cas, v).map_or(true, is_real_val));
    }
    if let Some((a, b)) = range {
        vals.retain(|v| match cval(cas, v) {
            Some(z) => is_real_val(z) && z.re >= a - 1e-12 && z.re <= b + 1e-12,
            None => true,
        });
    }
    let mut vals = dedup_values(cas, vals);
    sort_values(cas, &mut vals);
    Ok(Sol::Vals(vals))
}

trait FreeOfAllBut {
    fn free_of_all_but(&self, x: &str) -> bool;
}
impl FreeOfAllBut for Expr {
    fn free_of_all_but(&self, x: &str) -> bool {
        free_symbols(self).iter().all(|s| s == x)
    }
}

/// Union of two candidate sets; `None` (could not isolate) is contagious.
fn concat(a: Option<Vec<Expr>>, b: Option<Vec<Expr>>) -> Option<Vec<Expr>> {
    let mut x = a?;
    x.extend(b?);
    Some(x)
}

fn half_turn(cas: &Cas) -> Expr {
    if cas.s.deg {
        Expr::int(180)
    } else {
        Expr::sym("pi")
    }
}

fn real_of(cas: &Cas, e: &Expr) -> Option<f64> {
    cval(cas, e).filter(|z| is_real_val(*z)).map(|z| z.re)
}

/// Solves `lhs = rhs` for `x` by inverting the outermost operations.
fn isolate(cas: &Cas, lhs: &Expr, rhs: &Expr, x: &str, depth: usize) -> Option<Vec<Expr>> {
    if depth > MAX_DEPTH {
        return None;
    }
    let rhs = cas.eval(rhs).ok()?;
    if is_sym(lhs, x) && rhs.free_of(x) {
        return Some(vec![rhs]);
    }
    if lhs.free_of(x) {
        return None;
    }
    // polynomial after the inversions so far
    if !rhs.free_of(x) || !matches!(lhs, Expr::Add(_) | Expr::Mul(_)) {
        let diff = cas.eval(&sub(lhs.clone(), rhs.clone()).ok()?).ok()?;
        if !diff.free_of(x) {
            let (n, _) = together(cas, &diff).ok()?;
            if let Some(c) = poly_coeffs(&n, x) {
                return solve_poly(cas, &c, cas.s.complex, false).ok();
            }
        }
    }
    let ev = |e: R| -> Option<Expr> { e.ok().and_then(|v| cas.eval(&v).ok()) };
    match lhs {
        Expr::Add(v) => {
            let (dep, indep): (Vec<Expr>, Vec<Expr>) = v.iter().cloned().partition(|t| !t.free_of(x));
            let nrhs = ev(sub(rhs.clone(), add(indep).ok()?))?;
            if dep.len() == 1 {
                return isolate(cas, &dep[0], &nrhs, x, depth + 1);
            }
            let nonpoly: Vec<usize> = (0..dep.len()).filter(|i| poly_coeffs(&dep[*i], x).is_none()).collect();
            if nonpoly.len() == 1 {
                let k = nonpoly[0];
                let others: Vec<Expr> = dep.iter().enumerate().filter(|(i, _)| *i != k).map(|(_, t)| t.clone()).collect();
                let r2 = ev(sub(nrhs, add(others).ok()?))?;
                return isolate(cas, &dep[k], &r2, x, depth + 1);
            }
            None
        }
        Expr::Mul(v) => {
            let (dep, indep): (Vec<Expr>, Vec<Expr>) = v.iter().cloned().partition(|t| !t.free_of(x));
            if !indep.is_empty() {
                let nrhs = ev(div(rhs.clone(), mul(indep).ok()?))?;
                let l = mul(dep).ok()?;
                return isolate(cas, &l, &nrhs, x, depth + 1);
            }
            if rhs.is_zero() {
                let mut acc = Some(vec![]);
                for fct in &dep {
                    match solve_values(cas, fct, x, None, depth + 1) {
                        Ok(Sol::Vals(vs)) => acc = concat(acc, Some(vs)),
                        Ok(Sol::All) => return None,
                        Err(_) => return None,
                    }
                }
                return acc;
            }
            None
        }
        Expr::Pow(b, k) if !b.free_of(x) && k.free_of(x) => {
            let r = match k.as_rat() {
                Some(r) => r,
                None => return isolate(cas, b, &ev(pow(rhs.clone(), pow((**k).clone(), Expr::int(-1)).ok()?))?, x, depth + 1),
            };
            if r.is_neg() {
                if rhs.is_zero() {
                    return Some(vec![]);
                }
                let l = pow((**b).clone(), num(r.neg()?)).ok()?;
                return isolate(cas, &l, &ev(pow(rhs.clone(), Expr::int(-1)))?, x, depth + 1);
            }
            let (p, q) = (r.n, r.d);
            let rv = real_of(cas, &rhs);
            if q % 2 == 0 && rv.map_or(false, |v| v < 0.0) {
                return Some(vec![]);
            }
            if p == 1 {
                return isolate(cas, b, &ev(pow(rhs.clone(), Expr::int(q)))?, x, depth + 1);
            }
            if p % 2 == 0 {
                if rv.map_or(false, |v| v < 0.0) && !cas.s.complex {
                    return Some(vec![]);
                }
                let root = ev(pow(rhs.clone(), Expr::rat(1, p)))?;
                let mut acc = Some(vec![]);
                for s in [neg(root.clone()).ok()?, root] {
                    let t = ev(pow(s, Expr::int(q)))?;
                    acc = concat(acc, isolate(cas, b, &t, x, depth + 1));
                }
                return acc;
            }
            isolate(cas, b, &ev(pow(rhs.clone(), num(Rat::new(q, p)?)))?, x, depth + 1)
        }
        Expr::Pow(base, g) if base.free_of(x) && !g.free_of(x) => {
            if let Some(v) = real_of(cas, &rhs) {
                if v <= 0.0 {
                    return Some(vec![]);
                }
            }
            let l = cas.apply("ln", vec![rhs.clone()]).ok()?;
            let t = if is_sym(base, "e") { l } else { ev(div(l, cas.apply("ln", vec![(**base).clone()]).ok()?))? };
            isolate(cas, g, &t, x, depth + 1)
        }
        Expr::Call(name, args) if args.len() == 1 || (name == "log" && args.len() == 2 && args[1].free_of(x)) => {
            let g = &args[0];
            let rv = real_of(cas, &rhs);
            let ap = |f: &str, a: Expr| cas.apply(f, vec![a]).ok();
            let ht = half_turn(cas);
            let quarter = if cas.s.deg { 90.0 } else { std::f64::consts::FRAC_PI_2 };
            match name.as_str() {
                "ln" => isolate(cas, g, &ev(pow(Expr::sym("e"), rhs.clone()))?, x, depth + 1),
                "log" => {
                    let base = args.get(1).cloned().unwrap_or(Expr::int(10));
                    isolate(cas, g, &ev(pow(base, rhs.clone()))?, x, depth + 1)
                }
                "sin" | "cos" => {
                    if rv.map_or(false, |v| v.abs() > 1.0 + 1e-12) {
                        return Some(vec![]);
                    }
                    let (v1, v2) = if name == "sin" {
                        let a = ap("asin", rhs.clone())?;
                        (a.clone(), ev(sub(ht, a))?)
                    } else {
                        let a = ap("acos", rhs.clone())?;
                        (a.clone(), ev(neg(a))?)
                    };
                    let first = isolate(cas, g, &v1, x, depth + 1);
                    if v1 == v2 {
                        return first;
                    }
                    concat(first, isolate(cas, g, &v2, x, depth + 1))
                }
                "tan" => isolate(cas, g, &ap("atan", rhs.clone())?, x, depth + 1),
                "sec" | "csc" | "cot" => {
                    if rhs.is_zero() && name != "cot" {
                        return Some(vec![]);
                    }
                    let f = match name.as_str() {
                        "sec" => "cos",
                        "csc" => "sin",
                        _ => "tan",
                    };
                    let inv = ev(pow(rhs.clone(), Expr::int(-1)))?;
                    isolate(cas, &Expr::call(f, vec![g.clone()]), &inv, x, depth + 1)
                }
                "asin" | "atan" => {
                    if rv.map_or(false, |v| v.abs() > quarter + 1e-12 || (name == "atan" && v.abs() >= quarter)) {
                        return Some(vec![]);
                    }
                    let f = if name == "asin" { "sin" } else { "tan" };
                    isolate(cas, g, &ap(f, rhs.clone())?, x, depth + 1)
                }
                "acos" => {
                    if rv.map_or(false, |v| v < -1e-12 || v > 2.0 * quarter + 1e-12) {
                        return Some(vec![]);
                    }
                    isolate(cas, g, &ap("cos", rhs.clone())?, x, depth + 1)
                }
                "sinh" => isolate(cas, g, &ap("asinh", rhs.clone())?, x, depth + 1),
                "tanh" => {
                    if rv.map_or(false, |v| v.abs() >= 1.0) {
                        return Some(vec![]);
                    }
                    isolate(cas, g, &ap("atanh", rhs.clone())?, x, depth + 1)
                }
                "cosh" => {
                    if rv.map_or(false, |v| v < 1.0) {
                        return Some(vec![]);
                    }
                    let a = ap("acosh", rhs.clone())?;
                    concat(isolate(cas, g, &ev(neg(a.clone()))?, x, depth + 1), isolate(cas, g, &a, x, depth + 1))
                }
                "asinh" => isolate(cas, g, &ap("sinh", rhs.clone())?, x, depth + 1),
                "abs" => {
                    if rv.map_or(false, |v| v < 0.0) {
                        return Some(vec![]);
                    }
                    let first = isolate(cas, g, &ev(neg(rhs.clone()))?, x, depth + 1);
                    if rhs.is_zero() {
                        return first;
                    }
                    concat(first, isolate(cas, g, &rhs, x, depth + 1))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Substitution u = K(x) when `n` is a polynomial in a single kernel K.
fn kernel_solve(cas: &Cas, n: &Expr, x: &str, depth: usize) -> Option<Vec<Expr>> {
    let atoms = atoms_of(&[n])?;
    let dep: Vec<usize> = (0..atoms.len()).filter(|i| !atoms[*i].free_of(x)).collect();
    if dep.len() != 1 || is_sym(&atoms[dep[0]], x) {
        return None;
    }
    let k = dep[0];
    let kernel = atoms[k].clone();
    let t = "\u{2}t";
    let mp = mp_from(n, &atoms)?;
    // rebuild with the kernel replaced by t
    let mut atoms2 = atoms.clone();
    atoms2[k] = Expr::sym(t);
    let e2 = mp_to_expr(&mp, &atoms2).ok()?;
    let c = poly_coeffs(&e2, t)?;
    if c.len() <= 2 && c.len() < 2 {
        return None;
    }
    let ts = solve_poly(cas, &c, true, false).ok()?;
    let mut acc = Some(vec![]);
    for tv in ts {
        if let Some(z) = cval(cas, &tv) {
            if !is_real_val(z) && !cas.s.complex {
                continue;
            }
        }
        acc = concat(acc, isolate(cas, &kernel, &tv, x, depth + 1));
    }
    acc
}

/// Numeric real roots of f(x) = 0 by sampling, bracketing and Newton.
fn numeric_scan(cas: &Cas, f: &Expr, x: &str, range: Option<(f64, f64)>) -> Result<Vec<f64>, String> {
    let fc = cas.func1(f, x)?;
    let mut g = |t: f64| fc(t);
    let mut roots: Vec<f64> = Vec::new();
    let (a, b) = range.unwrap_or((-100.0, 100.0));
    roots.extend(numeric::find_roots(&mut g, a, b, if range.is_some() { 4000 } else { 20000 }));
    if range.is_none() {
        // log-spaced wider search
        let mut pts: Vec<f64> = Vec::new();
        for k in 0..=500 {
            let v = 10f64.powf(2.0 + 5.0 * k as f64 / 500.0);
            pts.push(v);
        }
        for sign in [1.0, -1.0] {
            let mut prev: Option<(f64, f64)> = None;
            for &p in &pts {
                let t = sign * p;
                let y = g(t);
                if let Some((t0, y0)) = prev {
                    if y.is_finite() && y0.is_finite() && y0.signum() != y.signum() {
                        if let Some(r) = numeric::bisect(&mut g, t0.min(t), t0.max(t)) {
                            let fr = g(r);
                            if fr.is_finite() && fr.abs() < 1e-6 * (1.0 + y0.abs().min(y.abs())) {
                                roots.push(r);
                            }
                        }
                    }
                }
                prev = Some((t, y));
            }
        }
    }
    let starts: Vec<f64> = match range {
        Some((a, b)) => (0..=20).map(|k| a + (b - a) * k as f64 / 20.0).collect(),
        None => vec![0.0, 0.5, -0.5, 1.0, -1.0, 2.0, -2.0, 5.0, -5.0, 10.0, -10.0, 50.0, -50.0],
    };
    for s in starts {
        if let Some(r) = numeric::newton(&mut g, s) {
            let fr = g(r);
            if r.is_finite() && fr.is_finite() && fr.abs() < 1e-10 * (1.0 + r.abs()) {
                if range.map_or(true, |(a, b)| r >= a - 1e-9 && r <= b + 1e-9) {
                    roots.push(r);
                }
            }
        }
    }
    roots.sort_by(|a, b| a.total_cmp(b));
    roots.dedup_by(|a, b| (*a - *b).abs() < 1e-7 * (1.0 + a.abs()));
    // polish
    let mut out: Vec<f64> = roots.into_iter().map(|r| numeric::newton(&mut g, r).filter(|p| (p - r).abs() < 1e-6 * (1.0 + r.abs())).unwrap_or(r)).map(clean_float).collect();
    out.dedup_by(|a, b| (*a - *b).abs() < 1e-9 * (1.0 + a.abs()));
    if out.len() > 30 {
        out.sort_by(|a, b| a.abs().total_cmp(&b.abs()));
        out.truncate(30);
        out.sort_by(|a, b| a.total_cmp(b));
    }
    Ok(out)
}

// ======================================================= inequalities

fn fmt_val(cas: &Cas, e: &Expr) -> String {
    Printer { sig: cas.s.digits }.print(e)
}

fn solve_inequality(cas: &Cas, op: &str, l: &Expr, r: &Expr, x: &str, range: Option<(f64, f64)>) -> R {
    let f = cas.eval(&sub(l.clone(), r.clone())?)?;
    if op == "!=" {
        return match solve_values(cas, &f, x, range, 0)? {
            Sol::All => Ok(Expr::Text("Sin solución".into())),
            Sol::Vals(v) if v.is_empty() => Ok(Expr::Text("Todos los números reales".into())),
            Sol::Vals(v) => Ok(Expr::Text(v.iter().map(|a| format!("{x} ≠ {}", fmt_val(cas, a))).collect::<Vec<_>>().join(" ∧ "))),
        };
    }
    if !f.free_of_all_but(x) {
        return Err("La desigualdad debe tener una sola variable".into());
    }
    let (n, d) = together(cas, &f)?;
    let n = expand(&n)?;
    let d = expand(&d)?;
    // critical points: zeros of numerator and denominator
    let mut crit: Vec<(Expr, f64, bool)> = Vec::new(); // (value, numeric, is_pole)
    for (p, pole) in [(&n, false), (&d, true)] {
        if p.free_of(x) {
            continue;
        }
        let vals = match solve_values(cas, p, x, range, 0) {
            Ok(Sol::Vals(v)) => v,
            Ok(Sol::All) => vec![],
            Err(_) => numeric_scan(cas, p, x, range)?.into_iter().map(|r| recognize_root(cas, r)).collect(),
        };
        for v in vals {
            if let Some(z) = cval(cas, &v) {
                if is_real_val(z) {
                    crit.push((v, z.re, pole));
                }
            }
        }
    }
    crit.sort_by(|a, b| a.1.total_cmp(&b.1).then(b.2.cmp(&a.2)));
    crit.dedup_by(|a, b| (a.1 - b.1).abs() < 1e-10 * (1.0 + a.1.abs()));
    let fc = cas.func1(&f, x)?;
    let holds = |v: f64| -> bool {
        let y = fc(v);
        if !y.is_finite() {
            return false;
        }
        match op {
            "<" => y < 0.0,
            ">" => y > 0.0,
            "<=" => y <= 1e-12,
            ">=" => y >= -1e-12,
            _ => false,
        }
    };
    let strict = matches!(op, "<" | ">");
    // pieces: interval_0, point_0, interval_1, point_1, ..., interval_k
    let (lo, hi) = range.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
    let crit: Vec<(Expr, f64, bool)> = crit.into_iter().filter(|c| c.1 >= lo && c.1 <= hi).collect();
    let k = crit.len();
    let sample = |i: usize| -> f64 {
        // test point of interval i (between crit[i-1] and crit[i])
        let left = if i == 0 { None } else { Some(crit[i - 1].1) };
        let right = if i == k { None } else { Some(crit[i].1) };
        match (left, right) {
            (None, None) => {
                if lo.is_finite() && hi.is_finite() {
                    0.5 * (lo + hi)
                } else if lo.is_finite() {
                    lo + 1.0
                } else if hi.is_finite() {
                    hi - 1.0
                } else {
                    0.0
                }
            }
            (None, Some(r)) => {
                if lo.is_finite() {
                    0.5 * (lo + r)
                } else {
                    r - 1.0 - r.abs()
                }
            }
            (Some(l), None) => {
                if hi.is_finite() {
                    0.5 * (l + hi)
                } else {
                    l + 1.0 + l.abs()
                }
            }
            (Some(l), Some(r)) => 0.5 * (l + r),
        }
    };
    let iv_ok: Vec<bool> = (0..=k).map(|i| holds(sample(i))).collect();
    let pt_ok: Vec<bool> = crit.iter().map(|c| !c.2 && !strict).collect();
    // segments
    #[derive(Clone)]
    struct Seg {
        a: Option<(Expr, bool)>, // endpoint, inclusive
        b: Option<(Expr, bool)>,
    }
    let mut segs: Vec<Seg> = Vec::new();
    let mut cur: Option<Seg> = None;
    let lo_e = if lo.is_finite() { Some((Expr::Float(lo), true)) } else { None };
    for i in 0..=k {
        // interval i
        if iv_ok[i] {
            if cur.is_none() {
                let a = if i == 0 { lo_e.clone() } else { Some((crit[i - 1].0.clone(), false)) };
                cur = Some(Seg { a, b: None });
            }
        } else if let Some(mut s) = cur.take() {
            s.b = Some((crit[i - 1].0.clone(), pt_ok[i - 1]));
            segs.push(s);
        }
        if i < k {
            // point i
            if pt_ok[i] {
                if cur.is_none() {
                    cur = Some(Seg { a: Some((crit[i].0.clone(), true)), b: None });
                }
            } else if let Some(mut s) = cur.take() {
                s.b = Some((crit[i].0.clone(), false));
                segs.push(s);
            }
            // point included but next interval excluded: close at point
            if pt_ok[i] && !iv_ok[i + 1] {
                if let Some(mut s) = cur.take() {
                    s.b = Some((crit[i].0.clone(), true));
                    segs.push(s);
                }
            }
        }
    }
    if let Some(mut s) = cur.take() {
        s.b = if hi.is_finite() { Some((Expr::Float(hi), true)) } else { None };
        segs.push(s);
    }
    if segs.is_empty() {
        return Ok(Expr::Text("Sin solución".into()));
    }
    let lt = |inc: bool| if inc { "≤" } else { "<" };
    let parts: Vec<String> = segs
        .iter()
        .map(|s| match (&s.a, &s.b) {
            (None, None) => "Todos los números reales".to_string(),
            (None, Some((b, inc))) => format!("{x} {} {}", lt(*inc), fmt_val(cas, b)),
            (Some((a, inc)), None) => format!("{x} {} {}", if *inc { "≥" } else { ">" }, fmt_val(cas, a)),
            (Some((a, ia)), Some((b, ib))) => {
                if a == b {
                    format!("{x} = {}", fmt_val(cas, a))
                } else {
                    format!("{} {} {x} {} {}", fmt_val(cas, a), lt(*ia), lt(*ib), fmt_val(cas, b))
                }
            }
        })
        .collect();
    Ok(Expr::Text(parts.join(" ∨ ")))
}

// ======================================================= systems

fn gauss_f64(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let p = (col..n).max_by(|i, j| a[*i][col].abs().total_cmp(&a[*j][col].abs()))?;
        if a[p][col].abs() < 1e-300 {
            return None;
        }
        a.swap(p, col);
        b.swap(p, col);
        for i in col + 1..n {
            let f = a[i][col] / a[col][col];
            if f != 0.0 {
                for j in col..n {
                    a[i][j] -= f * a[col][j];
                }
                b[i] -= f * b[col];
            }
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = b[i];
        for j in i + 1..n {
            s -= a[i][j] * x[j];
        }
        x[i] = s / a[i][i];
    }
    if x.iter().all(|v| v.is_finite()) {
        Some(x)
    } else {
        None
    }
}

/// Damped Newton / Gauss-Newton with numeric Jacobian.
fn newton_nd(fs: &[Compiled], x0: &[f64]) -> Option<Vec<f64>> {
    let n = x0.len();
    let m = fs.len();
    let eval = |x: &[f64]| -> Vec<f64> { fs.iter().map(|f| f.eval(x)).collect() };
    let norm = |v: &[f64]| v.iter().map(|a| a * a).sum::<f64>().sqrt();
    let mut x = x0.to_vec();
    let mut fx = eval(&x);
    if !fx.iter().all(|v| v.is_finite()) {
        return None;
    }
    for _ in 0..100 {
        let nf = norm(&fx);
        if nf < 1e-14 {
            break;
        }
        let mut j = vec![vec![0.0; n]; m];
        for k in 0..n {
            let h = 1e-7 * x[k].abs().max(1.0);
            let mut xp = x.clone();
            xp[k] += h;
            let mut xm = x.clone();
            xm[k] -= h;
            let fp = eval(&xp);
            let fm = eval(&xm);
            for i in 0..m {
                j[i][k] = (fp[i] - fm[i]) / (2.0 * h);
            }
        }
        let dx = if m == n {
            gauss_f64(j, fx.iter().map(|v| -v).collect())?
        } else {
            let mut jtj = vec![vec![0.0; n]; n];
            let mut jtf = vec![0.0; n];
            for a in 0..n {
                for b in 0..n {
                    jtj[a][b] = (0..m).map(|i| j[i][a] * j[i][b]).sum();
                }
                jtf[a] = -(0..m).map(|i| j[i][a] * fx[i]).sum::<f64>();
            }
            gauss_f64(jtj, jtf)?
        };
        let mut t = 1.0;
        let (mut xn, mut fnew);
        loop {
            xn = x.iter().zip(&dx).map(|(a, d)| a + t * d).collect::<Vec<f64>>();
            fnew = eval(&xn);
            if (fnew.iter().all(|v| v.is_finite()) && norm(&fnew) < nf) || t < 1e-4 {
                break;
            }
            t *= 0.5;
        }
        if !fnew.iter().all(|v| v.is_finite()) {
            return None;
        }
        let step = norm(&dx) * t;
        x = xn;
        fx = fnew;
        if step < 1e-15 * (1.0 + norm(&x)) {
            break;
        }
    }
    if norm(&fx) < 1e-9 && x.iter().all(|v| v.is_finite()) {
        Some(x)
    } else {
        None
    }
}

fn numeric_system(cas: &Cas, fs: &[Expr], vars: &[String]) -> Result<Vec<Vec<Expr>>, String> {
    let vr: Vec<&str> = vars.iter().map(|s| s.as_str()).collect();
    let comp: Vec<Compiled> = fs.iter().map(|f| cas.compile(f, &vr)).collect::<Result<_, _>>()?;
    let n = vars.len();
    let base = [0.5, -0.5, 1.5, -1.5, 3.0, -3.0, 7.0, -7.0];
    let mut starts: Vec<Vec<f64>> = Vec::new();
    let total = 8usize.saturating_pow(n as u32).min(300);
    for idx in 0..total {
        let mut v = Vec::with_capacity(n);
        let mut k = idx;
        for _ in 0..n {
            v.push(base[k % 8]);
            k /= 8;
        }
        starts.push(v);
    }
    let mut seed: u64 = 0x9E3779B97F4A7C15;
    for _ in 0..100 {
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            v.push(((seed >> 11) as f64 / (1u64 << 53) as f64) * 40.0 - 20.0);
        }
        starts.push(v);
    }
    let mut sols: Vec<Vec<f64>> = Vec::new();
    for s in starts {
        if let Some(r) = newton_nd(&comp, &s) {
            if !sols.iter().any(|o| o.iter().zip(&r).all(|(a, b)| (a - b).abs() < 1e-6 * (1.0 + a.abs()))) {
                sols.push(r);
            }
        }
        if sols.len() > 50 {
            break;
        }
    }
    sols.sort_by(|a, b| {
        for (x, y) in a.iter().zip(b) {
            let c = x.total_cmp(y);
            if c != Ordering::Equal {
                return c;
            }
        }
        Ordering::Equal
    });
    Ok(sols.into_iter().map(|s| s.into_iter().map(|v| recognize_root(cas, v)).collect()).collect())
}

/// Linear system via exact rref. `None` if not linear.
fn linear_system(cas: &Cas, fs: &[Expr], vars: &[String]) -> Option<Result<Expr, String>> {
    let nv = vars.len();
    let mut rows: Vec<Vec<Expr>> = Vec::new();
    for f in fs {
        let mut row = Vec::with_capacity(nv + 1);
        for v in vars {
            let c = poly_coeffs(f, v)?;
            if c.len() > 2 {
                return None;
            }
            let a = c.get(1).cloned().unwrap_or(Expr::zero());
            if vars.iter().any(|w| !a.free_of(w)) {
                return None;
            }
            row.push(a);
        }
        let mut k = f.clone();
        for v in vars {
            k = subst(&k, v, &Expr::zero());
        }
        let k = cas.eval(&k).ok()?;
        row.push(neg(k).ok()?);
        rows.push(row);
    }
    Some((|| {
        let (red, piv) = matrix::rref(&rows)?;
        if piv.contains(&nv) {
            return Ok(Expr::Text("Sin solución".into()));
        }
        if piv.is_empty() {
            return Ok(Expr::Text("Infinitas soluciones (todas las variables son libres)".into()));
        }
        let mut out = Vec::new();
        for (r, &p) in piv.iter().enumerate() {
            let mut val = red[r][nv].clone();
            for j in 0..nv {
                if j != p && !piv.contains(&j) && !red[r][j].is_zero() {
                    val = sub(val, mul2(red[r][j].clone(), Expr::sym(&vars[j]))?)?;
                }
            }
            out.push(Expr::Eq(Box::new(Expr::sym(&vars[p])), Box::new(simp(cas, &val))));
        }
        Ok(Expr::List(out))
    })())
}

/// Exact elimination: solve one equation for one variable and substitute.
fn subst_system(cas: &Cas, fs: Vec<Expr>, vars: Vec<String>, depth: usize) -> Result<Vec<Vec<(String, Expr)>>, String> {
    if depth > 8 {
        return Err("Sistema demasiado complejo".into());
    }
    let mut eqs = Vec::new();
    for f in fs {
        let f = cas.eval(&f)?;
        if f.is_zero() {
            continue;
        }
        if vars.iter().all(|v| f.free_of(v)) {
            if cval(cas, &f).map_or(false, |z| z.abs() > 1e-12) {
                return Ok(vec![]);
            }
            continue;
        }
        eqs.push(f);
    }
    if vars.is_empty() {
        return Ok(vec![vec![]]);
    }
    if eqs.is_empty() {
        return Err("El sistema tiene infinitas soluciones".into());
    }
    // 1) an equation linear in some variable
    let mut best: Option<(usize, usize, u8)> = None;
    for (i, f) in eqs.iter().enumerate() {
        for (j, v) in vars.iter().enumerate() {
            if let Some(c) = poly_coeffs(f, v) {
                if c.len() == 2 {
                    let a = &c[1];
                    let score = if a.is_number() {
                        0
                    } else if vars.iter().all(|w| a.free_of(w)) {
                        1
                    } else {
                        2
                    };
                    if best.map_or(true, |b| score < b.2) {
                        best = Some((i, j, score));
                    }
                }
            }
        }
    }
    let mut branch: Option<(usize, usize, Vec<Expr>)> = None;
    if let Some((i, j, _)) = best {
        let c = poly_coeffs(&eqs[i], &vars[j]).unwrap();
        let val = simp(cas, &neg(div(c[0].clone(), c[1].clone())?)?);
        branch = Some((i, j, vec![val]));
    } else {
        // 2) an equation in a single variable
        for (i, f) in eqs.iter().enumerate() {
            let inv: Vec<usize> = (0..vars.len()).filter(|j| !f.free_of(&vars[*j])).collect();
            if inv.len() == 1 {
                if let Sol::Vals(v) = solve_values(cas, f, &vars[inv[0]], None, depth + 1)? {
                    branch = Some((i, inv[0], v));
                    break;
                }
            }
        }
        // 3) a quadratic in some variable
        if branch.is_none() {
            'q: for (i, f) in eqs.iter().enumerate() {
                for (j, v) in vars.iter().enumerate() {
                    if let Some(c) = poly_coeffs(f, v) {
                        if c.len() == 3 && c[2].is_number() {
                            if let Ok(vs) = quad_symbolic(cas, &c[2], &c[1], &c[0], true, false) {
                                branch = Some((i, j, vs));
                                break 'q;
                            }
                        }
                    }
                }
            }
        }
    }
    let (i, j, vals) = branch.ok_or("No se pudo resolver el sistema por sustitución")?;
    let v = vars[j].clone();
    let rest_vars: Vec<String> = vars.iter().filter(|w| **w != v).cloned().collect();
    let mut out = Vec::new();
    for val in vals {
        let others: Vec<Expr> = eqs.iter().enumerate().filter(|(k, _)| *k != i).map(|(_, f)| subst(f, &v, &val)).collect();
        // keep the equation itself when the value involves remaining variables (denominators)
        let subs = subst_system(cas, others, rest_vars.clone(), depth + 1)?;
        for s in subs {
            let mut full = val.clone();
            for (w, wv) in &s {
                full = subst(&full, w, wv);
            }
            let full = simp(cas, &full);
            let mut sol = s.clone();
            sol.push((v.clone(), full));
            out.push(sol);
            if out.len() > 100 {
                return Err("Demasiadas soluciones".into());
            }
        }
    }
    Ok(out)
}

fn solve_system(cas: &Cas, eqs: &[Expr], vars_arg: Option<&Expr>) -> R {
    let fs: Vec<Expr> = eqs.iter().map(|e| to_zero_form(cas, e)).collect::<Result<_, _>>()?;
    let vars: Vec<String> = match vars_arg {
        Some(Expr::List(v)) => v.iter().map(|s| s.sym_name().map(String::from).ok_or_else(|| "Las incógnitas deben ser variables".to_string())).collect::<Result<_, _>>()?,
        Some(Expr::Sym(s)) => vec![s.clone()],
        Some(_) => return Err("Las incógnitas deben ser una lista de variables, p. ej. {x, y}".into()),
        None => {
            let mut v = Vec::new();
            for f in &fs {
                f.symbols(&mut v);
            }
            v.sort();
            v
        }
    };
    if vars.is_empty() {
        return Err("No hay incógnitas".into());
    }
    if vars.len() == 1 && fs.len() == 1 {
        return solve_single(cas, &fs[0], &vars[0], None);
    }
    if let Some(r) = linear_system(cas, &fs, &vars) {
        return r;
    }
    let finish = |sols: Vec<Vec<Expr>>| -> R {
        let complex = cas.s.complex;
        let mut good: Vec<Vec<Expr>> = Vec::new();
        for s in sols {
            // verify numerically
            let mut ok = true;
            let mut closed = true;
            for f in &fs {
                let mut g = f.clone();
                for (k, v) in vars.iter().enumerate() {
                    g = subst(&g, v, &s[k]);
                }
                if g.has_free_symbols() {
                    closed = false;
                    continue;
                }
                match cas.num(&g) {
                    Ok(z) if z.is_finite() && z.abs() < 1e-7 * (1.0 + s.iter().filter_map(|e| cval(cas, e)).map(|z| z.abs()).fold(0.0, f64::max)) => {}
                    _ => ok = false,
                }
            }
            if !complex && s.iter().any(|e| cval(cas, e).map_or(false, |z| !is_real_val(z))) {
                ok = false;
            }
            let _ = closed;
            if ok && !good.contains(&s) {
                good.push(s);
            }
        }
        if good.is_empty() {
            return Ok(Expr::Text("Sin solución".into()));
        }
        let mk = |s: &Vec<Expr>| Expr::List(vars.iter().zip(s).map(|(v, e)| Expr::Eq(Box::new(Expr::sym(v)), Box::new(e.clone()))).collect());
        if good.len() == 1 {
            return Ok(mk(&good[0]));
        }
        Ok(Expr::List(good.iter().map(mk).collect()))
    };
    match subst_system(cas, fs.clone(), vars.clone(), 0) {
        Ok(sols) => {
            let ordered: Vec<Vec<Expr>> = sols
                .into_iter()
                .filter_map(|s| vars.iter().map(|v| s.iter().find(|(w, _)| w == v).map(|(_, e)| e.clone())).collect::<Option<Vec<_>>>())
                .collect();
            let mut ordered = ordered;
            ordered.sort_by(|a, b| {
                for (x, y) in a.iter().zip(b) {
                    match (cval(cas, x), cval(cas, y)) {
                        (Some(p), Some(q)) => {
                            let c = p.re.total_cmp(&q.re).then(p.im.total_cmp(&q.im));
                            if c != Ordering::Equal {
                                return c;
                            }
                        }
                        _ => return Ordering::Equal,
                    }
                }
                Ordering::Equal
            });
            finish(ordered)
        }
        Err(_) => {
            if fs.iter().all(|f| free_symbols(f).iter().all(|s| vars.contains(s))) {
                finish(numeric_system(cas, &fs, &vars)?)
            } else {
                Err("No se pudo resolver el sistema".into())
            }
        }
    }
}

// ======================================================= commands

fn solve_single(cas: &Cas, f: &Expr, x: &str, range: Option<(f64, f64)>) -> R {
    match solve_values(cas, f, x, range, 0)? {
        Sol::All => Ok(Expr::Text("Todos los números reales".into())),
        Sol::Vals(v) if v.is_empty() => Ok(Expr::Text("Sin solución".into())),
        Sol::Vals(v) => Ok(Expr::List(v.into_iter().map(|e| Expr::Eq(Box::new(Expr::sym(x)), Box::new(e))).collect())),
    }
}

/// `solve(eq, x [, a, b])`, `solve({eqs}, {vars})`, `nsolve(eq, x, guess)`.
pub fn cmd_solve(cas: &Cas, name: &str, args: Vec<Expr>) -> R {
    if args.is_empty() || args.len() > 4 {
        return Err(format!("{name} requiere entre 1 y 4 argumentos"));
    }
    if name == "nsolve" {
        return cmd_nsolve(cas, args);
    }
    let eq = &args[0];
    if let Expr::List(eqs) = eq {
        let multi_var = matches!(args.get(1), Some(Expr::List(v)) if v.len() > 1);
        if eqs.len() == 1 && !multi_var {
            let mut a = args.clone();
            a[0] = eqs[0].clone();
            return cmd_solve(cas, name, a);
        }
        return solve_system(cas, eqs, args.get(1));
    }
    // variable and optional range
    let (var_arg, rest): (Option<&Expr>, &[Expr]) = match args.get(1) {
        Some(Expr::Sym(_)) | Some(Expr::List(_)) => (args.get(1), &args[2..]),
        Some(_) => (None, &args[1..]),
        None => (None, &[]),
    };
    let x = pick_var(eq, var_arg)?;
    let range = match rest {
        [] => None,
        [a, b] => {
            let a = cas.num_real(a)?;
            let b = cas.num_real(b)?;
            Some((a.min(b), a.max(b)))
        }
        _ => return Err("solve(ecuación, x, a, b): se esperaban dos límites".into()),
    };
    match eq {
        Expr::Rel(op, l, r) => solve_inequality(cas, op, l, r, &x, range),
        Expr::Text(t) if t == "verdadero" => Ok(Expr::Text("Todos los números reales".into())),
        Expr::Text(t) if t == "falso" => Ok(Expr::Text("Sin solución".into())),
        Expr::Matrix(_) | Expr::Text(_) => Err("solve requiere una ecuación".into()),
        _ => {
            let f = to_zero_form(cas, eq)?;
            solve_single(cas, &f, &x, range)
        }
    }
}

fn cmd_nsolve(cas: &Cas, args: Vec<Expr>) -> R {
    if let Expr::List(eqs) = &args[0] {
        let fs: Vec<Expr> = eqs.iter().map(|e| to_zero_form(cas, e)).collect::<Result<_, _>>()?;
        let vars: Vec<String> = match args.get(1) {
            Some(Expr::List(v)) => v.iter().map(|s| s.sym_name().map(String::from).ok_or_else(|| "Las incógnitas deben ser variables".to_string())).collect::<Result<_, _>>()?,
            _ => return Err("nsolve({ecuaciones}, {variables}, {valores iniciales})".into()),
        };
        let vr: Vec<&str> = vars.iter().map(|s| s.as_str()).collect();
        let comp: Vec<Compiled> = fs.iter().map(|f| cas.compile(f, &vr)).collect::<Result<_, _>>()?;
        let guess: Vec<f64> = match args.get(2) {
            Some(Expr::List(g)) => g.iter().map(|e| cas.num_real(e)).collect::<Result<_, _>>()?,
            Some(g) => vec![cas.num_real(g)?; vars.len()],
            None => vec![1.0; vars.len()],
        };
        if guess.len() != vars.len() {
            return Err("El número de valores iniciales no coincide con el de incógnitas".into());
        }
        let r = newton_nd(&comp, &guess).or_else(|| numeric_system(cas, &fs, &vars).ok().and_then(|s| s.first().map(|v| v.iter().map(|e| cas.num_real(e).unwrap_or(f64::NAN)).collect())));
        let r = r.ok_or("No se encontró una solución numérica")?;
        return Ok(Expr::List(vars.iter().zip(r).map(|(v, x)| Expr::Eq(Box::new(Expr::sym(v)), Box::new(Expr::Float(clean_float(x))))).collect()));
    }
    let (var_arg, rest): (Option<&Expr>, &[Expr]) = match args.get(1) {
        Some(Expr::Sym(_)) => (args.get(1), &args[2..]),
        Some(_) => (None, &args[1..]),
        None => (None, &[]),
    };
    let x = pick_var(&args[0], var_arg)?;
    let f = to_zero_form(cas, &args[0])?;
    let g0 = match rest.first() {
        Some(g) => cas.num_real(g)?,
        None => 0.0,
    };
    let fc = cas.func1(&f, &x)?;
    let mut g = |t: f64| fc(t);
    let ok = |r: f64, g: &mut dyn FnMut(f64) -> f64| r.is_finite() && g(r).abs() < 1e-9 * (1.0 + r.abs());
    let mut root = numeric::newton(&mut g, g0).filter(|r| ok(*r, &mut g));
    if root.is_none() {
        let mut rad = 1.0;
        while rad <= 1e6 && root.is_none() {
            let rs = numeric::find_roots(&mut g, g0 - rad, g0 + rad, 2000);
            root = rs.into_iter().min_by(|a, b| (a - g0).abs().total_cmp(&(b - g0).abs()));
            rad *= 10.0;
        }
    }
    let r = root.ok_or("No se encontró una raíz numérica")?;
    Ok(Expr::List(vec![Expr::Eq(Box::new(Expr::sym(&x)), Box::new(Expr::Float(clean_float(r))))]))
}

// ======================================================= tests

#[cfg(test)]
mod tests {
    use crate::tests::{ap, ev};
    use crate::Engine;

    #[test]
    fn poly_commands() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "coeffs(3x^2 - 2x + 5, x)"), "{3, -2, 5}");
        assert_eq!(ev(&mut e, "coeffs(a*x^2 + b, x)"), "{a, 0, b}");
        assert_eq!(ev(&mut e, "degree(x^5 + x, x)"), "5");
        assert_eq!(ev(&mut e, "degree((x+1)^3)"), "3");
        assert_eq!(ev(&mut e, "quotient(x^3 - 1, x - 1, x)"), "x^2 + x + 1");
        assert_eq!(ev(&mut e, "remainder(x^3 + 2, x - 1, x)"), "3");
        assert_eq!(ev(&mut e, "quotient(x^2 + 3x + 2, 2x + 2)"), "x/2 + 1");
        assert_eq!(ev(&mut e, "gcd(x^2 - 1, x^2 + 2x + 1)"), "x + 1");
        assert_eq!(ev(&mut e, "gcd(x^2 - y^2, x + y)"), "x + y");
    }

    #[test]
    fn factoring() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "factor(x^2 - 5x + 6)"), "(x - 3)·(x - 2)");
        assert_eq!(ev(&mut e, "factor(x^3 - x)"), "x·(x - 1)·(x + 1)");
        assert_eq!(ev(&mut e, "factor(2x^2 - 2)"), "2·(x - 1)·(x + 1)");
        assert_eq!(ev(&mut e, "factor(x^4 - 1)"), "(x - 1)·(x + 1)·(x^2 + 1)");
        assert_eq!(ev(&mut e, "factor(x^2 + 2x + 1)"), "(x + 1)^2");
        assert_eq!(ev(&mut e, "factor(6x^2 - x - 1)"), "(2x - 1)·(3x + 1)");
        assert_eq!(ev(&mut e, "factor(x^4 + 4)"), "(x^2 - 2x + 2)·(x^2 + 2x + 2)");
        assert_eq!(ev(&mut e, "factor(x^2 - y^2)"), "(x - y)·(x + y)");
        assert_eq!(ev(&mut e, "factor(x^2*y + x*y^2)"), "x·y·(x + y)");
        assert_eq!(ev(&mut e, "factor(x^2 + 1)"), "x^2 + 1");
        assert_eq!(ev(&mut e, "factor((x^2 - 1)/(x^2 + 2x + 1))"), "(x - 1)/(x + 1)");
        assert_eq!(ev(&mut e, "factor(sin(x)^2 - 1)"), "(sin(x) - 1)·(sin(x) + 1)");
    }

    #[test]
    fn cancel_and_simplify() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "cancel((x^2 - 1)/(x - 1))"), "x + 1");
        assert_eq!(ev(&mut e, "cancel(1/(x-1) - 1/(x+1))"), "2/(x^2 - 1)");
        assert_eq!(ev(&mut e, "cancel((x^2 - y^2)/(x - y))"), "x + y");
        assert_eq!(ev(&mut e, "simplify(sin(x)^2 + cos(x)^2)"), "1");
        assert_eq!(ev(&mut e, "simplify(3sin(x)^2 + 3cos(x)^2)"), "3");
        assert_eq!(ev(&mut e, "simplify(1 - sin(x)^2)"), "cos(x)^2");
        assert_eq!(ev(&mut e, "simplify((x^2 + 2x + 1)/(x + 1))"), "x + 1");
        assert_eq!(ev(&mut e, "simplify(x + 1)"), "x + 1");
        assert_eq!(ev(&mut e, "simplify(sin(x)^4 - cos(x)^4)"), "-cos(x)^2 + sin(x)^2");
        assert_eq!(ev(&mut e, "simplify(tan(x)^2 + 1)"), "sec(x)^2");
        assert_eq!(ev(&mut e, "simplify(ln(2) + ln(x))"), "ln(2x)");
    }

    #[test]
    fn polynomial_equations() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "solve(x^2 = 4, x)"), "{x = -2, x = 2}");
        assert_eq!(ev(&mut e, "solve(x^2 - 5x + 6 = 0)"), "{x = 2, x = 3}");
        assert_eq!(ev(&mut e, "solve(x^2 + x - 1 = 0, x)"), "{x = -1/2 - √5/2, x = -1/2 + √5/2}");
        assert_eq!(ev(&mut e, "solve(2x + 3 = 7)"), "{x = 2}");
        assert_eq!(ev(&mut e, "solve(x^3 - 6x^2 + 11x - 6 = 0)"), "{x = 1, x = 2, x = 3}");
        assert_eq!(ev(&mut e, "solve(x^2 + 1 = 0, x)"), "Sin solución");
        assert_eq!(ev(&mut e, "solve(x + 1 = x + 1, x)"), "Todos los números reales");
        assert_eq!(ev(&mut e, "solve(x + 1 = x, x)"), "Sin solución");
        assert_eq!(ev(&mut e, "solve(x^4 - 5x^2 + 6 = 0)"), "{x = -√3, x = -√2, x = √2, x = √3}");
        assert_eq!(ev(&mut e, "solve(x^3 = 2)"), "{x = 2^(1/3)}");
        assert_eq!(ev(&mut e, "solve(1/x + 1 = 0)"), "{x = -1}");
        assert_eq!(ev(&mut e, "solve((x^2-1)/(x-1) = 0)"), "{x = -1}");
        assert_eq!(ap(&mut e, "solve(x^5 - x - 1 = 0)"), "{x = 1.167303978}");
    }

    #[test]
    fn symbolic_equations() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "solve(a*x + b = 0, x)"), "{x = -b/a}");
        let s = ev(&mut e, "solve(a*x^2 + b*x + c = 0, x)");
        assert!(s.contains("√(b^2 - 4a·c)") || s.contains("√(-4a·c + b^2)"), "{s}");
        assert_eq!(ev(&mut e, "solve(x^2 - a^2 = 0, x)"), "{x = -a, x = a}");
    }

    #[test]
    fn complex_mode() {
        let mut e = Engine::new();
        e.settings.complex = true;
        assert_eq!(ev(&mut e, "solve(x^2 + 1 = 0, x)"), "{x = -i, x = i}");
        assert_eq!(ev(&mut e, "solve(x^2 + 2x + 5 = 0, x)"), "{x = -1 - 2i, x = -1 + 2i}");
        assert_eq!(ev(&mut e, "solve(x^4 = 16, x)"), "{x = -2, x = 2, x = -2i, x = 2i}");
    }

    #[test]
    fn transcendental_equations() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "solve(e^x = 5, x)"), "{x = ln(5)}");
        assert_eq!(ev(&mut e, "solve(sin(x) = 1/2, x, 0, 3)"), "{x = π/6, x = 5π/6}");
        assert_eq!(ev(&mut e, "solve(sin(x) = 1/2, x)"), "{x = π/6, x = 5π/6}");
        assert_eq!(ev(&mut e, "solve(ln(x) = 2, x)"), "{x = e^2}");
        assert_eq!(ev(&mut e, "solve(sqrt(x) = x - 2, x)"), "{x = 4}");
        assert_eq!(ev(&mut e, "solve(2^x = 8, x)"), "{x = 3}");
        assert_eq!(ev(&mut e, "solve(e^(2x) - 3e^x + 2 = 0, x)"), "{x = 0, x = ln(2)}");
        assert_eq!(ap(&mut e, "solve(cos(x) = x, x)"), "{x = 0.7390851332}");
        assert_eq!(ev(&mut e, "solve(e^x = -1, x)"), "Sin solución");
        assert_eq!(ev(&mut e, "solve(x^x = 4, x)"), "{x = 2}");
        assert_eq!(ev(&mut e, "solve(abs(x - 1) = 3, x)"), "{x = -2, x = 4}");
        assert_eq!(ev(&mut e, "solve(sin(x) = 0, x, -7, 7)"), "{x = -2π, x = -π, x = 0, x = π, x = 2π}");
        assert_eq!(ev(&mut e, "solve(2^x = 3, x)"), "{x = ln(3)/ln(2)}");
        assert_eq!(ap(&mut e, "nsolve(x^3 - 2x - 5 = 0, x, 2)"), "{x = 2.094551482}");
        assert_eq!(ap(&mut e, "nsolve(cos(x) - x, 1)"), "{x = 0.7390851332}");
    }

    #[test]
    fn systems() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "solve({x + y = 3, x - y = 1}, {x, y})"), "{x = 2, y = 1}");
        assert_eq!(ev(&mut e, "solve({x^2 + y^2 = 25, x - y = 1}, {x, y})"), "{{x = -3, y = -4}, {x = 4, y = 3}}");
        assert_eq!(ev(&mut e, "solve({x + y = 1, 2x + 2y = 2}, {x, y})"), "{x = -y + 1}");
        assert_eq!(ev(&mut e, "solve({x + y = 1, x + y = 2}, {x, y})"), "Sin solución");
        assert_eq!(ev(&mut e, "solve({x + y + z = 6, x - y = 0, z = 2x}, {x, y, z})"), "{x = 3/2, y = 3/2, z = 3}");
        assert_eq!(ev(&mut e, "solve({x*y = 6, x + y = 5}, {x, y})"), "{{x = 2, y = 3}, {x = 3, y = 2}}");
        assert_eq!(ev(&mut e, "solve({x + y = 3}, {x, y})"), "{x = -y + 3}");
        let s = ap(&mut e, "solve({e^x + y = 3, x - y^2 = 0}, {x, y})");
        assert!(s.starts_with("{"), "{s}");
    }

    #[test]
    fn inequalities() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "solve(x^2 - x - 6 > 0, x)"), "x < -2 ∨ x > 3");
        assert_eq!(ev(&mut e, "solve(x^2 <= 4, x)"), "-2 ≤ x ≤ 2");
        assert_eq!(ev(&mut e, "solve(2x + 1 < 5, x)"), "x < 2");
        assert_eq!(ev(&mut e, "solve(x^2 + 1 > 0, x)"), "Todos los números reales");
        assert_eq!(ev(&mut e, "solve(x^2 < 0, x)"), "Sin solución");
        assert_eq!(ev(&mut e, "solve(1/x > 0, x)"), "x > 0");
    }

    #[test]
    fn eigenvalues() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "eigvals([[2,0],[0,3]])"), "{2, 3}");
        assert_eq!(ev(&mut e, "eigvals([[1,2],[2,1]])"), "{-1, 3}");
        assert_eq!(ev(&mut e, "eigvals([[2,1],[0,2]])"), "{2, 2}");
        let s = ap(&mut e, "eigvals([[1.5, 0], [0, 2.5]])");
        assert_eq!(s, "{1.5, 2.5}");
    }
}
