//! Canonicalizing constructors: `add`, `mul`, `pow`, plus `expand`.
//! Every expression built through these functions is in canonical form.

use crate::expr::{cmp_expr, Expr};
use crate::matrix;
use crate::num::{factorize, Rat, C};

pub type R = Result<Expr, String>;

pub const ERR_DIV0: &str = "División entre cero";

#[derive(Clone, Copy, Debug)]
enum Coef {
    R(Rat),
    F(f64),
}

impl Coef {
    fn add(self, o: Coef) -> Coef {
        match (self, o) {
            (Coef::R(a), Coef::R(b)) => match a.add(b) {
                Some(r) => Coef::R(r),
                None => Coef::F(a.to_f64() + b.to_f64()),
            },
            _ => Coef::F(self.f() + o.f()),
        }
    }
    fn mul(self, o: Coef) -> Coef {
        match (self, o) {
            (Coef::R(a), Coef::R(b)) => match a.mul(b) {
                Some(r) => Coef::R(r),
                None => Coef::F(a.to_f64() * b.to_f64()),
            },
            _ => Coef::F(self.f() * o.f()),
        }
    }
    fn f(self) -> f64 {
        match self {
            Coef::R(r) => r.to_f64(),
            Coef::F(f) => f,
        }
    }
    fn is_zero(self) -> bool {
        match self {
            Coef::R(r) => r.is_zero(),
            Coef::F(f) => f == 0.0,
        }
    }
    fn is_one(self) -> bool {
        match self {
            Coef::R(r) => r.is_one(),
            Coef::F(f) => f == 1.0,
        }
    }
    fn to_expr(self) -> Expr {
        match self {
            Coef::R(r) => Expr::Num(r),
            Coef::F(f) => Expr::Float(f),
        }
    }
}

fn num_coef(e: &Expr) -> Option<Coef> {
    match e {
        Expr::Num(r) => Some(Coef::R(*r)),
        Expr::Float(f) => Some(Coef::F(*f)),
        _ => None,
    }
}

/// Splits a term into numeric coefficient and the remaining (canonical) part.
fn split_coef(e: Expr) -> (Coef, Expr) {
    match e {
        Expr::Num(r) => (Coef::R(r), Expr::one()),
        Expr::Float(f) => (Coef::F(f), Expr::one()),
        Expr::Mul(mut v) => {
            if let Some(c) = v.first().and_then(num_coef) {
                v.remove(0);
                let rest = if v.len() == 1 { v.pop().unwrap() } else { Expr::Mul(v) };
                (c, rest)
            } else {
                (Coef::R(Rat::ONE), Expr::Mul(v))
            }
        }
        other => (Coef::R(Rat::ONE), other),
    }
}

/// Builds `c * rest` without re-simplifying (rest must be canonical).
fn join_coef(c: Coef, rest: Expr) -> Expr {
    if c.is_zero() {
        return Expr::zero();
    }
    if rest.is_one() {
        return c.to_expr();
    }
    if c.is_one() {
        return rest;
    }
    match rest {
        Expr::Mul(mut v) => {
            v.insert(0, c.to_expr());
            Expr::Mul(v)
        }
        other => Expr::Mul(vec![c.to_expr(), other]),
    }
}

pub fn add(terms: Vec<Expr>) -> R {
    // flatten
    let mut flat = Vec::with_capacity(terms.len());
    for t in terms {
        match t {
            Expr::Add(v) => flat.extend(v),
            other => flat.push(other),
        }
    }
    // structured operands
    if flat.iter().any(|t| matches!(t, Expr::Eq(..))) {
        let mut l = Vec::new();
        let mut r = Vec::new();
        for t in flat {
            match t {
                Expr::Eq(a, b) => {
                    l.push(*a);
                    r.push(*b);
                }
                other => {
                    l.push(other.clone());
                    r.push(other);
                }
            }
        }
        return Ok(Expr::Eq(Box::new(add(l)?), Box::new(add(r)?)));
    }
    if flat.iter().any(|t| t.is_matrix()) {
        let mut acc: Option<Vec<Vec<Expr>>> = None;
        for t in flat {
            match t {
                Expr::Matrix(m) => {
                    acc = Some(match acc {
                        None => m,
                        Some(a) => matrix::add_m(&a, &m)?,
                    })
                }
                Expr::Num(r) if r.is_zero() => {}
                _ => return Err("No se puede sumar una matriz con un escalar".into()),
            }
        }
        return Ok(Expr::Matrix(acc.unwrap()));
    }
    if flat.iter().any(|t| t.is_list()) {
        return broadcast(flat, add);
    }
    if flat.iter().any(|t| matches!(t, Expr::Text(_))) {
        return Err("Operación no válida sobre texto".into());
    }
    {
        let infs: Vec<bool> = flat.iter().filter_map(is_inf_term).collect();
        if !infs.is_empty() {
            if infs.iter().any(|&p| p) && infs.iter().any(|&p| !p) {
                return Err("Indeterminado (∞ - ∞)".into());
            }
            if flat.iter().all(|t| is_inf_term(t).is_some() || !t.has_free_symbols()) {
                return Ok(if infs[0] { Expr::sym("inf") } else { Expr::Mul(vec![Expr::int(-1), Expr::sym("inf")]) });
            }
        }
    }

    let mut coef = Coef::R(Rat::ZERO);
    let mut items: Vec<(Expr, Coef)> = Vec::with_capacity(flat.len());
    for t in flat {
        match t {
            Expr::Num(r) => coef = coef.add(Coef::R(r)),
            Expr::Float(f) => coef = coef.add(Coef::F(f)),
            other => {
                let (c, rest) = split_coef(other);
                items.push((rest, c));
            }
        }
    }
    items.sort_by(|a, b| cmp_expr(&a.0, &b.0));
    let mut out: Vec<Expr> = Vec::with_capacity(items.len() + 1);
    let mut it = items.into_iter().peekable();
    while let Some((key, mut c)) = it.next() {
        while let Some((k2, _)) = it.peek() {
            if *k2 == key {
                c = c.add(it.next().unwrap().1);
            } else {
                break;
            }
        }
        if !c.is_zero() {
            // distribute coefficient into a nested sum? keep as is
            out.push(join_coef(c, key));
        }
    }
    if !coef.is_zero() || out.is_empty() {
        out.insert(0, coef.to_expr());
    }
    if out.len() == 1 {
        return Ok(out.pop().unwrap());
    }
    // Float contamination: fold everything numeric
    Ok(Expr::Add(out))
}

fn broadcast(items: Vec<Expr>, f: fn(Vec<Expr>) -> R) -> R {
    let mut len = None;
    for t in &items {
        if let Expr::List(v) = t {
            match len {
                None => len = Some(v.len()),
                Some(l) if l != v.len() => return Err("Las listas deben tener la misma longitud".into()),
                _ => {}
            }
        }
    }
    let n = len.unwrap_or(0);
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let args: Vec<Expr> = items
            .iter()
            .map(|t| match t {
                Expr::List(v) => v[k].clone(),
                other => other.clone(),
            })
            .collect();
        out.push(f(args)?);
    }
    Ok(Expr::List(out))
}

pub fn mul(factors: Vec<Expr>) -> R {
    let mut flat = Vec::with_capacity(factors.len());
    for f in factors {
        match f {
            Expr::Mul(v) => flat.extend(v),
            other => flat.push(other),
        }
    }
    if flat.iter().any(|t| matches!(t, Expr::Eq(..))) {
        let mut l = Vec::new();
        let mut r = Vec::new();
        for t in flat {
            match t {
                Expr::Eq(a, b) => {
                    l.push(*a);
                    r.push(*b);
                }
                other => {
                    l.push(other.clone());
                    r.push(other);
                }
            }
        }
        return Ok(Expr::Eq(Box::new(mul(l)?), Box::new(mul(r)?)));
    }
    if flat.iter().any(|t| t.is_matrix()) {
        let mut scalars = Vec::new();
        let mut acc: Option<Vec<Vec<Expr>>> = None;
        for t in flat {
            match t {
                Expr::Matrix(m) => {
                    acc = Some(match acc {
                        None => m,
                        Some(a) => matrix::mul_m(&a, &m)?,
                    })
                }
                Expr::List(_) => return Err("No se puede multiplicar matriz por lista".into()),
                other => scalars.push(other),
            }
        }
        let m = acc.unwrap();
        if scalars.is_empty() {
            return Ok(Expr::Matrix(m));
        }
        let s = mul(scalars)?;
        if s.is_one() {
            return Ok(Expr::Matrix(m));
        }
        return Ok(Expr::Matrix(matrix::map_m(&m, |e| mul(vec![s.clone(), e.clone()]))?));
    }
    if flat.iter().any(|t| t.is_list()) {
        return broadcast(flat, mul);
    }
    if flat.iter().any(|t| matches!(t, Expr::Text(_))) {
        return Err("Operación no válida sobre texto".into());
    }

    if flat.iter().any(|t| t.sym_name() == Some("inf")) && flat.iter().all(|t| t.sym_name() == Some("inf") || t.is_number()) {
        let mut sign = 1.0;
        for t in &flat {
            if let Some(v) = t.as_f64() {
                if v == 0.0 {
                    return Err("Indeterminado (0 · ∞)".into());
                }
                sign *= v.signum();
            }
        }
        return Ok(if sign > 0.0 { Expr::sym("inf") } else { Expr::Mul(vec![Expr::int(-1), Expr::sym("inf")]) });
    }
    let mut coef = Coef::R(Rat::ONE);
    let mut groups: Vec<(Expr, Vec<Expr>)> = Vec::new();
    for f in flat {
        match f {
            Expr::Num(r) => coef = coef.mul(Coef::R(r)),
            Expr::Float(x) => coef = coef.mul(Coef::F(x)),
            Expr::Pow(b, e) => groups.push((*b, vec![*e])),
            other => groups.push((other, vec![Expr::one()])),
        }
    }
    if coef.is_zero() {
        return Ok(match coef {
            Coef::F(_) => Expr::Float(0.0),
            _ => Expr::zero(),
        });
    }
    groups.sort_by(|a, b| cmp_expr(&a.0, &b.0));
    let mut merged: Vec<(Expr, Vec<Expr>)> = Vec::with_capacity(groups.len());
    for (b, e) in groups {
        if let Some(last) = merged.last_mut() {
            if last.0 == b {
                last.1.extend(e);
                continue;
            }
        }
        merged.push((b, e));
    }
    let mut out: Vec<Expr> = Vec::with_capacity(merged.len());
    for (b, es) in merged {
        let e = if es.len() == 1 { es.into_iter().next().unwrap() } else { add(es)? };
        let p = if e.is_one() { b } else { pow(b, e)? };
        match p {
            Expr::Num(r) => coef = coef.mul(Coef::R(r)),
            Expr::Float(x) => coef = coef.mul(Coef::F(x)),
            Expr::Mul(v) => {
                for f in v {
                    match f {
                        Expr::Num(r) => coef = coef.mul(Coef::R(r)),
                        Expr::Float(x) => coef = coef.mul(Coef::F(x)),
                        other => out.push(other),
                    }
                }
            }
            Expr::Add(ref terms) if out.is_empty() && false => {
                let _ = terms;
            }
            other => out.push(other),
        }
    }
    if coef.is_zero() {
        return Ok(Expr::zero());
    }
    out.sort_by(cmp_expr);
    // merge again any identical factors produced by expansion (rare)
    let mut dedup: Vec<Expr> = Vec::with_capacity(out.len());
    let mut need_again = false;
    for f in out {
        let base = match &f {
            Expr::Pow(b, _) => (**b).clone(),
            other => other.clone(),
        };
        if let Some(l) = dedup.last() {
            let lb = match l {
                Expr::Pow(b, _) => (**b).clone(),
                other => other.clone(),
            };
            if lb == base && !matches!(base, Expr::Num(_)) {
                need_again = true;
            }
        }
        dedup.push(f);
    }
    if need_again {
        let mut v = vec![coef.to_expr()];
        v.extend(dedup);
        // second pass cannot loop forever: bases are now unique after merge
        return mul_no_regroup(v);
    }
    // distribute a rational coefficient over a single sum: 2(x+1) -> 2x+2
    if dedup.len() == 1 {
        if let (Coef::R(r), Expr::Add(terms)) = (coef, &dedup[0]) {
            if !r.is_one() {
                let t: Result<Vec<Expr>, String> = terms.iter().map(|t| mul(vec![Expr::Num(r), t.clone()])).collect();
                return add(t?);
            }
        }
    }
    Ok(join_coef(coef, if dedup.is_empty() { Expr::one() } else if dedup.len() == 1 { dedup.pop().unwrap() } else { Expr::Mul(dedup) }))
}

fn mul_no_regroup(v: Vec<Expr>) -> R {
    let mut coef = Coef::R(Rat::ONE);
    let mut rest = Vec::new();
    for f in v {
        match num_coef(&f) {
            Some(c) => coef = coef.mul(c),
            None => rest.push(f),
        }
    }
    rest.sort_by(cmp_expr);
    Ok(join_coef(coef, if rest.is_empty() { Expr::one() } else if rest.len() == 1 { rest.pop().unwrap() } else { Expr::Mul(rest) }))
}

pub fn neg(e: Expr) -> R {
    mul(vec![Expr::int(-1), e])
}
pub fn sub(a: Expr, b: Expr) -> R {
    add(vec![a, neg(b)?])
}
pub fn div(a: Expr, b: Expr) -> R {
    if b.is_zero() {
        return Err(ERR_DIV0.into());
    }
    mul(vec![a, pow(b, Expr::int(-1))?])
}
pub fn recip(a: Expr) -> R {
    pow(a, Expr::int(-1))
}

/// Complex number to canonical expression.
pub fn from_c(z: C) -> Expr {
    if z.is_real() || z.im == 0.0 {
        return Expr::Float(z.re);
    }
    let im = Expr::Mul(vec![Expr::Float(z.im), Expr::sym("i")]);
    if z.re.abs() < 1e-15 * z.im.abs() {
        return if z.im == 1.0 { Expr::sym("i") } else { im };
    }
    Expr::Add(vec![Expr::Float(z.re), im])
}

/// `n^(p/q)` for positive integer n: returns (coefficient, remaining base, remaining exponent).
fn int_root(n: i128, p: i128, q: i128) -> Option<(Rat, i128, Rat)> {
    if n <= 1 || n > 1_000_000_000_000_000 {
        return None;
    }
    let fs = factorize(n as u128);
    let mut coef = Rat::ONE;
    let mut rem: Vec<(i128, i128)> = Vec::new();
    for (prime, a) in fs {
        let tot = (a as i128).checked_mul(p)?;
        let whole = tot.div_euclid(q);
        let r = tot.rem_euclid(q);
        coef = coef.mul(Rat::int(prime as i128).powi(whole as i64)?)?;
        if r != 0 {
            rem.push((prime as i128, r));
        }
    }
    if rem.is_empty() {
        return Some((coef, 1, Rat::ONE));
    }
    let mut g = q;
    for (_, r) in &rem {
        g = crate::num::gcd_i128(g, *r);
    }
    let mut base: i128 = 1;
    for (prime, r) in &rem {
        base = base.checked_mul(prime.checked_pow((*r / g) as u32)?)?;
    }
    Some((coef, base, Rat::new(g, q)?))
}

pub fn pow(b: Expr, e: Expr) -> R {
    // structured
    if let Expr::Eq(l, r) = b {
        return Ok(Expr::Eq(Box::new(pow(*l, e.clone())?), Box::new(pow(*r, e)?)));
    }
    if b.is_list() || e.is_list() {
        return broadcast(vec![b, e], |v| {
            let mut it = v.into_iter();
            let a = it.next().unwrap();
            let c = it.next().unwrap();
            pow(a, c)
        });
    }
    if let Expr::Matrix(m) = &b {
        let k = e.as_int().ok_or("El exponente de una matriz debe ser entero")?;
        return matrix::pow_m(m, k).map(Expr::Matrix);
    }
    if e.is_matrix() {
        return Err("Exponente matricial no soportado".into());
    }
    if e.is_zero() {
        return Ok(Expr::one());
    }
    if e.is_one() {
        return Ok(b);
    }
    if b.is_one() {
        return Ok(b);
    }
    if b.is_zero() {
        return match e.as_f64() {
            Some(x) if x > 0.0 => Ok(Expr::zero()),
            Some(_) => Err(ERR_DIV0.into()),
            None => Ok(Expr::Pow(Box::new(b), Box::new(e))),
        };
    }
    match (&b, &e) {
        (Expr::Num(rb), Expr::Num(re)) => return pow_rat(*rb, *re),
        (Expr::Float(_), _) | (_, Expr::Float(_)) if b.is_number() && e.is_number() => {
            let z = C::real(b.as_f64().unwrap()).powc(C::real(e.as_f64().unwrap()));
            return Ok(from_c(z));
        }
        _ => {}
    }
    // i^n
    if let (Expr::Sym(s), Some(n)) = (&b, e.as_rat()) {
        if s == "i" && n.is_int() {
            return Ok(match n.n.rem_euclid(4) {
                0 => Expr::one(),
                1 => Expr::sym("i"),
                2 => Expr::int(-1),
                _ => Expr::Mul(vec![Expr::int(-1), Expr::sym("i")]),
            });
        }
    }
    // Euler: e^(k·π·i) exact for "nice" k
    if b.sym_name() == Some("e") {
        if let Some(k) = pi_i_multiple(&e) {
            let half = Rat::new(1, 2).unwrap();
            if let (Some(s), Some(c)) = (crate::eval::sin_table(k), k.add(half).and_then(crate::eval::sin_table)) {
                return add(vec![c, mul(vec![s, Expr::sym("i")])?]);
            }
        }
    }
    // infinity
    if b.sym_name() == Some("inf") {
        if let Some(x) = e.as_f64() {
            return Ok(if x > 0.0 { b } else { Expr::zero() });
        }
    }
    // e^(ln x) = x
    if let (Expr::Sym(s), Expr::Call(f, a)) = (&b, &e) {
        if s == "e" && f == "ln" && a.len() == 1 {
            return Ok(a[0].clone());
        }
    }
    // (a^m)^n
    if let Expr::Pow(a, m) = &b {
        if e.as_rat().map_or(false, |r| r.is_int()) || matches!(m.as_ref(), Expr::Num(r) if r.is_int() && r.n % 2 != 0) && e.as_rat().is_some() {
            let ne = mul(vec![(**m).clone(), e.clone()])?;
            return pow((**a).clone(), ne);
        }
    }
    // (a*b)^n
    if let Expr::Mul(fs) = &b {
        if e.as_rat().map_or(false, |r| r.is_int()) {
            let v: Result<Vec<Expr>, String> = fs.iter().map(|f| pow(f.clone(), e.clone())).collect();
            return mul(v?);
        }
        // pull out positive numeric coefficient: (4x)^(1/2) = 2 x^(1/2)
        if let Some(Expr::Num(c)) = fs.first() {
            if !c.is_neg() && e.as_rat().is_some() {
                let rest = if fs.len() == 2 { fs[1].clone() } else { Expr::Mul(fs[1..].to_vec()) };
                return mul(vec![pow(Expr::Num(*c), e.clone())?, Expr::Pow(Box::new(rest), Box::new(e))]);
            }
        }
    }
    Ok(Expr::Pow(Box::new(b), Box::new(e)))
}

/// k such that e = k·π·i.
fn pi_i_multiple(e: &Expr) -> Option<Rat> {
    let fs: Vec<Expr> = match e {
        Expr::Mul(v) => v.clone(),
        _ => return None,
    };
    let mut k = Rat::ONE;
    let (mut has_pi, mut has_i) = (false, false);
    for f in fs {
        match f {
            Expr::Num(r) => k = k.mul(r)?,
            Expr::Sym(s) if s == "pi" && !has_pi => has_pi = true,
            Expr::Sym(s) if s == "i" && !has_i => has_i = true,
            _ => return None,
        }
    }
    if has_pi && has_i {
        Some(k)
    } else {
        None
    }
}

fn is_inf_term(t: &Expr) -> Option<bool> {
    match t {
        Expr::Sym(s) if s == "inf" => Some(true),
        Expr::Mul(v) if v.len() == 2 && v[1].sym_name() == Some("inf") => v[0].as_f64().map(|c| c > 0.0),
        _ => None,
    }
}

fn pow_rat(b: Rat, e: Rat) -> R {
    if e.is_int() {
        if e.n.abs() <= 4096 {
            if let Some(r) = b.powi(e.n as i64) {
                return Ok(Expr::Num(r));
            }
        }
        return Ok(Expr::Float(b.to_f64().powf(e.to_f64())));
    }
    // negative base
    if b.is_neg() {
        let pb = b.neg().unwrap();
        if e.d % 2 == 1 {
            // real odd root
            let r = pow_rat(pb, e)?;
            return if e.n % 2 == 0 { Ok(r) } else { neg(r) };
        }
        if e.d == 2 {
            // (-r)^(p/2) = i^p r^(p/2)
            let ip = pow(Expr::sym("i"), Expr::int(e.n))?;
            return mul(vec![ip, pow_rat(pb, e)?]);
        }
        let z = C::real(b.to_f64()).powc(C::real(e.to_f64()));
        return Ok(from_c(z));
    }
    // split e = k + f, 0 < f < 1 ; rationalizes denominators
    let k = e.floor();
    let f = e.sub(Rat::int(k)).unwrap();
    let whole = match b.powi(k as i64) {
        Some(w) => w,
        None => return Ok(Expr::Float(b.to_f64().powf(e.to_f64()))),
    };
    let mut coef = whole;
    let mut out: Vec<Expr> = Vec::new();
    for (part, is_den) in [(b.n, false), (b.d, true)] {
        if part == 1 {
            continue;
        }
        // numerator: n^f ; denominator: d^(-f) = d^-1 * d^(1-f)
        let (pf, extra) = if is_den { (Rat::ONE.sub(f).unwrap(), Rat::new(1, part).unwrap()) } else { (f, Rat::ONE) };
        coef = coef.mul(extra).ok_or("desbordamiento")?;
        match int_root(part, pf.n, pf.d) {
            Some((c, base, ex)) => {
                coef = coef.mul(c).ok_or("desbordamiento")?;
                if base != 1 {
                    out.push(Expr::Pow(Box::new(Expr::int(base)), Box::new(Expr::Num(ex))));
                }
            }
            None => out.push(Expr::Pow(Box::new(Expr::int(part)), Box::new(Expr::Num(pf)))),
        }
    }
    if out.is_empty() {
        return Ok(Expr::Num(coef));
    }
    // combine bases with identical exponent: 2^(1/2)*3^(1/2) -> 6^(1/2)
    if out.len() == 2 {
        if let (Expr::Pow(b1, e1), Expr::Pow(b2, e2)) = (&out[0], &out[1]) {
            if e1 == e2 {
                if let (Some(x), Some(y)) = (b1.as_int(), b2.as_int()) {
                    if let Some(p) = x.checked_mul(y) {
                        out = vec![Expr::Pow(Box::new(Expr::int(p)), e1.clone())];
                    }
                }
            }
        }
    }
    out.sort_by(cmp_expr);
    Ok(join_coef(Coef::R(coef), if out.len() == 1 { out.pop().unwrap() } else { Expr::Mul(out) }))
}

/// Full expansion of products and integer powers of sums.
pub fn expand(e: &Expr) -> R {
    match e {
        Expr::Add(v) => {
            let t: Result<Vec<Expr>, String> = v.iter().map(expand).collect();
            add(t?)
        }
        Expr::Mul(v) => {
            let mut acc = vec![Expr::one()];
            for f in v {
                let f = expand(f)?;
                let terms = match f {
                    Expr::Add(t) => t,
                    other => vec![other],
                };
                let mut next = Vec::with_capacity(acc.len() * terms.len());
                for a in &acc {
                    for t in &terms {
                        next.push(mul(vec![a.clone(), t.clone()])?);
                    }
                }
                if next.len() > 20000 {
                    return Err("Expansión demasiado grande".into());
                }
                acc = next;
            }
            add(acc)
        }
        Expr::Pow(b, x) => {
            let b = expand(b)?;
            let x = expand(x)?;
            if let (Expr::Add(_), Some(n)) = (&b, x.as_int()) {
                if (2..=64).contains(&n) {
                    let mut acc = b.clone();
                    for _ in 1..n {
                        acc = expand(&Expr::Mul(vec![acc, b.clone()]))?;
                    }
                    return Ok(acc);
                }
                if (-64..=-2).contains(&n) {
                    let mut acc = b.clone();
                    for _ in 1..(-n) {
                        acc = expand(&Expr::Mul(vec![acc, b.clone()]))?;
                    }
                    return pow(acc, Expr::int(-1));
                }
            }
            pow(b, x)
        }
        Expr::List(v) => Ok(Expr::List(v.iter().map(expand).collect::<Result<_, _>>()?)),
        Expr::Matrix(m) => Ok(Expr::Matrix(matrix::map_m(m, expand)?)),
        Expr::Eq(a, b) => Ok(Expr::Eq(Box::new(expand(a)?), Box::new(expand(b)?))),
        Expr::Call(n, a) => Ok(Expr::Call(n.clone(), a.iter().map(expand).collect::<Result<_, _>>()?)),
        other => Ok(other.clone()),
    }
}

/// Structural substitution of symbol `x` by `v` (no re-simplification).
pub fn subst(e: &Expr, x: &str, v: &Expr) -> Expr {
    match e {
        Expr::Sym(s) if s == x => v.clone(),
        Expr::Add(t) => Expr::Add(t.iter().map(|a| subst(a, x, v)).collect()),
        Expr::Mul(t) => Expr::Mul(t.iter().map(|a| subst(a, x, v)).collect()),
        Expr::List(t) => Expr::List(t.iter().map(|a| subst(a, x, v)).collect()),
        Expr::Call(n, t) => {
            // respect bound variables of calculus commands
            if is_binding_call(n) && t.get(1).and_then(|s| s.sym_name()) == Some(x) {
                return e.clone();
            }
            Expr::Call(n.clone(), t.iter().map(|a| subst(a, x, v)).collect())
        }
        Expr::Pow(a, b) => Expr::Pow(Box::new(subst(a, x, v)), Box::new(subst(b, x, v))),
        Expr::Eq(a, b) => Expr::Eq(Box::new(subst(a, x, v)), Box::new(subst(b, x, v))),
        Expr::Rel(o, a, b) => Expr::Rel(o.clone(), Box::new(subst(a, x, v)), Box::new(subst(b, x, v))),
        Expr::Matrix(m) => Expr::Matrix(m.iter().map(|r| r.iter().map(|a| subst(a, x, v)).collect()).collect()),
        other => other.clone(),
    }
}

pub fn is_binding_call(n: &str) -> bool {
    matches!(n, "diff" | "integrate" | "limit" | "sum" | "prod" | "taylor" | "solve" | "nsolve" | "seq" | "tangent" | "fmin" | "fmax")
}

/// Numerator/denominator split of a canonical expression.
pub fn num_den(e: &Expr) -> (Expr, Expr) {
    match e {
        Expr::Pow(b, x) => {
            if let Some(r) = x.as_rat() {
                if r.is_neg() {
                    let p = Rat { n: -r.n, d: r.d };
                    return (Expr::one(), if p.is_one() { (**b).clone() } else { Expr::Pow(b.clone(), Box::new(Expr::Num(p))) });
                }
            }
            (e.clone(), Expr::one())
        }
        Expr::Mul(v) => {
            let mut n = Vec::new();
            let mut d = Vec::new();
            for f in v {
                match f {
                    Expr::Num(r) if !r.is_int() => {
                        n.push(Expr::int(r.n));
                        d.push(Expr::int(r.d));
                    }
                    _ => {
                        let (a, b) = num_den(f);
                        if !a.is_one() {
                            n.push(a);
                        }
                        if !b.is_one() {
                            d.push(b);
                        }
                    }
                }
            }
            let mk = |mut v: Vec<Expr>| if v.is_empty() { Expr::one() } else if v.len() == 1 { v.pop().unwrap() } else { Expr::Mul(v) };
            (mk(n), mk(d))
        }
        Expr::Num(r) if !r.is_int() => (Expr::int(r.n), Expr::int(r.d)),
        _ => (e.clone(), Expr::one()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn x() -> Expr {
        Expr::sym("x")
    }
    #[test]
    fn like_terms() {
        let e = add(vec![x(), x(), Expr::int(3)]).unwrap();
        assert_eq!(e.to_string(), "2x + 3");
        let z = sub(add(vec![x(), Expr::one()]).unwrap(), add(vec![Expr::one(), x()]).unwrap()).unwrap();
        assert!(z.is_zero());
    }
    #[test]
    fn powers() {
        let e = mul(vec![x(), pow(x(), Expr::int(2)).unwrap()]).unwrap();
        assert_eq!(e.to_string(), "x^3");
        assert_eq!(pow(Expr::int(8), Expr::rat(1, 2)).unwrap().to_string(), "2√2");
        assert_eq!(pow(Expr::int(2), Expr::rat(-1, 2)).unwrap().to_string(), "√2/2");
        assert_eq!(pow(Expr::int(-4), Expr::rat(1, 2)).unwrap().to_string(), "2i");
        assert_eq!(pow(Expr::int(-8), Expr::rat(1, 3)).unwrap().to_string(), "-2");
        let s2 = pow(Expr::int(2), Expr::rat(1, 2)).unwrap();
        assert_eq!(mul(vec![s2.clone(), s2]).unwrap().to_string(), "2");
    }
    #[test]
    fn expansion() {
        let e = pow(add(vec![x(), Expr::one()]).unwrap(), Expr::int(2)).unwrap();
        assert_eq!(expand(&e).unwrap().to_string(), "x^2 + 2x + 1");
    }
}
