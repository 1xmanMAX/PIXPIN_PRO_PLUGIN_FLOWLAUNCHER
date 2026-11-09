//! Expression tree in canonical n-ary form (SymPy-style):
//! `a - b` is `Add[a, Mul[-1, b]]`, `a / b` is `Mul[a, Pow[b, -1]]`.

use crate::num::{fmt_f64, Rat};
use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Debug)]
pub enum Expr {
    Num(Rat),
    Float(f64),
    Sym(String),
    Add(Vec<Expr>),
    Mul(Vec<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Matrix(Vec<Vec<Expr>>),
    List(Vec<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    /// Relational operator other than `=` (`<`, `>`, `<=`, `>=`, `!=`).
    Rel(String, Box<Expr>, Box<Expr>),
    /// Free text result (stats summaries, factorizations, ...)
    Text(String),
}

impl Expr {
    pub fn int(n: i128) -> Expr {
        Expr::Num(Rat::int(n))
    }
    pub fn rat(n: i128, d: i128) -> Expr {
        Expr::Num(Rat::new(n, d).unwrap_or(Rat::ZERO))
    }
    pub fn sym(s: &str) -> Expr {
        Expr::Sym(s.to_string())
    }
    pub fn call(name: &str, args: Vec<Expr>) -> Expr {
        Expr::Call(name.to_string(), args)
    }
    pub fn zero() -> Expr {
        Expr::int(0)
    }
    pub fn one() -> Expr {
        Expr::int(1)
    }
    pub fn is_zero(&self) -> bool {
        match self {
            Expr::Num(r) => r.is_zero(),
            Expr::Float(f) => *f == 0.0,
            _ => false,
        }
    }
    pub fn is_one(&self) -> bool {
        match self {
            Expr::Num(r) => r.is_one(),
            Expr::Float(f) => *f == 1.0,
            _ => false,
        }
    }
    pub fn is_number(&self) -> bool {
        matches!(self, Expr::Num(_) | Expr::Float(_))
    }
    pub fn as_rat(&self) -> Option<Rat> {
        match self {
            Expr::Num(r) => Some(*r),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i128> {
        match self {
            Expr::Num(r) if r.is_int() => Some(r.n),
            Expr::Float(f) if f.fract() == 0.0 && f.abs() < 1e18 => Some(*f as i128),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Expr::Num(r) => Some(r.to_f64()),
            Expr::Float(f) => Some(*f),
            _ => None,
        }
    }
    pub fn is_matrix(&self) -> bool {
        matches!(self, Expr::Matrix(_))
    }
    pub fn is_list(&self) -> bool {
        matches!(self, Expr::List(_))
    }
    pub fn sym_name(&self) -> Option<&str> {
        match self {
            Expr::Sym(s) => Some(s),
            _ => None,
        }
    }

    /// True if `self` does not contain the symbol `x`.
    pub fn free_of(&self, x: &str) -> bool {
        match self {
            Expr::Sym(s) => s != x,
            Expr::Num(_) | Expr::Float(_) | Expr::Text(_) => true,
            Expr::Add(v) | Expr::Mul(v) | Expr::List(v) | Expr::Call(_, v) => v.iter().all(|e| e.free_of(x)),
            Expr::Pow(a, b) | Expr::Eq(a, b) | Expr::Rel(_, a, b) => a.free_of(x) && b.free_of(x),
            Expr::Matrix(rows) => rows.iter().all(|r| r.iter().all(|e| e.free_of(x))),
        }
    }

    /// Collects free symbols (excluding constants pi, e, i, inf).
    pub fn symbols(&self, out: &mut Vec<String>) {
        match self {
            Expr::Sym(s) => {
                if !is_constant(s) && !out.contains(s) {
                    out.push(s.clone())
                }
            }
            Expr::Num(_) | Expr::Float(_) | Expr::Text(_) => {}
            Expr::Add(v) | Expr::Mul(v) | Expr::List(v) | Expr::Call(_, v) => v.iter().for_each(|e| e.symbols(out)),
            Expr::Pow(a, b) | Expr::Eq(a, b) | Expr::Rel(_, a, b) => {
                a.symbols(out);
                b.symbols(out)
            }
            Expr::Matrix(rows) => rows.iter().flatten().for_each(|e| e.symbols(out)),
        }
    }

    pub fn has_free_symbols(&self) -> bool {
        let mut v = Vec::new();
        self.symbols(&mut v);
        !v.is_empty()
    }

    pub fn contains_float(&self) -> bool {
        match self {
            Expr::Float(_) => true,
            Expr::Num(_) | Expr::Sym(_) | Expr::Text(_) => false,
            Expr::Add(v) | Expr::Mul(v) | Expr::List(v) | Expr::Call(_, v) => v.iter().any(|e| e.contains_float()),
            Expr::Pow(a, b) | Expr::Eq(a, b) | Expr::Rel(_, a, b) => a.contains_float() || b.contains_float(),
            Expr::Matrix(rows) => rows.iter().flatten().any(|e| e.contains_float()),
        }
    }

    /// Size of tree, used to choose the simplest of several equivalent forms.
    pub fn size(&self) -> usize {
        match self {
            Expr::Num(r) => {
                if r.is_int() {
                    1
                } else {
                    2
                }
            }
            Expr::Float(_) | Expr::Sym(_) | Expr::Text(_) => 1,
            Expr::Add(v) | Expr::Mul(v) | Expr::List(v) | Expr::Call(_, v) => 1 + v.iter().map(|e| e.size()).sum::<usize>(),
            Expr::Pow(a, b) | Expr::Eq(a, b) | Expr::Rel(_, a, b) => 1 + a.size() + b.size(),
            Expr::Matrix(rows) => 1 + rows.iter().flatten().map(|e| e.size()).sum::<usize>(),
        }
    }

    fn kind_rank(&self) -> u8 {
        match self {
            Expr::Num(_) | Expr::Float(_) => 0,
            Expr::Sym(_) => 1,
            Expr::Pow(..) => 1,
            Expr::Mul(_) => 2,
            Expr::Call(..) => 3,
            Expr::Add(_) => 4,
            Expr::Matrix(_) => 5,
            Expr::List(_) => 6,
            Expr::Eq(..) => 7,
            Expr::Rel(..) => 8,
            Expr::Text(_) => 9,
        }
    }
}

pub fn is_constant(s: &str) -> bool {
    matches!(s, "pi" | "e" | "i" | "inf")
}

/// Total order used for canonical sorting.
pub fn cmp_expr(a: &Expr, b: &Expr) -> Ordering {
    use Expr::*;
    // Treat x and x^n as comparing by base first so like powers stay close.
    let (ab, ae) = base_exp(a);
    let (bb, be) = base_exp(b);
    if !std::ptr::eq(ab, a) || !std::ptr::eq(bb, b) {
        let c = cmp_expr_raw(ab, bb);
        if c != Ordering::Equal {
            return c;
        }
        return match (ae, be) {
            (Some(x), Some(y)) => cmp_expr(x, y),
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
    }
    let _ = (Num(Rat::ZERO),);
    cmp_expr_raw(a, b)
}

fn base_exp(e: &Expr) -> (&Expr, Option<&Expr>) {
    match e {
        Expr::Pow(b, x) => (b, Some(x)),
        _ => (e, None),
    }
}

fn cmp_expr_raw(a: &Expr, b: &Expr) -> Ordering {
    use Expr::*;
    let ra = a.kind_rank();
    let rb = b.kind_rank();
    if ra != rb {
        return ra.cmp(&rb);
    }
    match (a, b) {
        (Num(x), Num(y)) => x.cmp(y),
        (Float(x), Float(y)) => x.total_cmp(y),
        (Num(x), Float(y)) => x.to_f64().total_cmp(y).then(Ordering::Less),
        (Float(x), Num(y)) => x.total_cmp(&y.to_f64()).then(Ordering::Greater),
        (Sym(x), Sym(y)) => x.cmp(y),
        (Pow(a1, a2), Pow(b1, b2)) => cmp_expr(a1, b1).then_with(|| cmp_expr(a2, b2)),
        (Sym(_), Pow(..)) => Ordering::Less,
        (Pow(..), Sym(_)) => Ordering::Greater,
        (Add(x), Add(y)) | (Mul(x), Mul(y)) | (List(x), List(y)) => cmp_vec(x, y),
        (Call(n1, x), Call(n2, y)) => n1.cmp(n2).then_with(|| cmp_vec(x, y)),
        (Matrix(x), Matrix(y)) => {
            x.len().cmp(&y.len()).then_with(|| {
                for (r1, r2) in x.iter().zip(y) {
                    let c = cmp_vec(r1, r2);
                    if c != Ordering::Equal {
                        return c;
                    }
                }
                Ordering::Equal
            })
        }
        (Eq(a1, a2), Eq(b1, b2)) => cmp_expr(a1, b1).then_with(|| cmp_expr(a2, b2)),
        (Rel(o1, a1, a2), Rel(o2, b1, b2)) => o1.cmp(o2).then_with(|| cmp_expr(a1, b1)).then_with(|| cmp_expr(a2, b2)),
        (Text(x), Text(y)) => x.cmp(y),
        _ => Ordering::Equal,
    }
}

fn cmp_vec(x: &[Expr], y: &[Expr]) -> Ordering {
    for (a, b) in x.iter().zip(y) {
        let c = cmp_expr(a, b);
        if c != Ordering::Equal {
            return c;
        }
    }
    x.len().cmp(&y.len())
}

impl PartialEq for Expr {
    fn eq(&self, o: &Self) -> bool {
        use Expr::*;
        match (self, o) {
            (Num(a), Num(b)) => a == b,
            (Float(a), Float(b)) => a == b,
            (Sym(a), Sym(b)) => a == b,
            (Add(a), Add(b)) | (Mul(a), Mul(b)) | (List(a), List(b)) => a == b,
            (Pow(a, b), Pow(c, d)) | (Eq(a, b), Eq(c, d)) => a == c && b == d,
            (Rel(o1, a, b), Rel(o2, c, d)) => o1 == o2 && a == c && b == d,
            (Call(n, a), Call(m, b)) => n == m && a == b,
            (Matrix(a), Matrix(b)) => a == b,
            (Text(a), Text(b)) => a == b,
            _ => false,
        }
    }
}

// ---------------------------------------------------------------- printing

/// Splits a product into (numeric coefficient, numerator factors, denominator factors).
fn split_fraction(v: &[Expr]) -> (Option<Rat>, Option<f64>, Vec<Expr>, Vec<Expr>) {
    let mut coef: Option<Rat> = None;
    let mut fcoef: Option<f64> = None;
    let mut num = Vec::new();
    let mut den = Vec::new();
    for f in v {
        match f {
            Expr::Num(r) => coef = Some(*r),
            Expr::Float(x) => fcoef = Some(*x),
            Expr::Pow(b, e) => match e.as_ref() {
                Expr::Num(r) if r.is_neg() => {
                    let pe = Rat { n: -r.n, d: r.d };
                    if pe.is_one() {
                        den.push((**b).clone());
                    } else {
                        den.push(Expr::Pow(b.clone(), Box::new(Expr::Num(pe))));
                    }
                }
                _ => num.push(f.clone()),
            },
            _ => num.push(f.clone()),
        }
    }
    (coef, fcoef, num, den)
}

/// Heuristic polynomial degree of a term, for pleasant display ordering.
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

fn prec(e: &Expr) -> u8 {
    match e {
        Expr::Eq(..) | Expr::Rel(..) => 0,
        Expr::Add(_) => 1,
        Expr::Mul(_) => 2,
        Expr::Num(r) if !r.is_int() => 2,
        Expr::Num(r) if r.is_neg() => 1,
        Expr::Float(f) if *f < 0.0 => 1,
        Expr::Pow(_, x) if matches!(x.as_ref(), Expr::Num(r) if r.is_neg()) => 2,
        Expr::Pow(..) => 4,
        _ => 5,
    }
}

pub struct Printer {
    pub sig: usize,
}

impl Printer {
    pub fn print(&self, e: &Expr) -> String {
        let mut s = String::new();
        self.w(e, &mut s);
        s
    }

    fn wp(&self, e: &Expr, min_prec: u8, s: &mut String) {
        if prec(e) < min_prec {
            s.push('(');
            self.w(e, s);
            s.push(')');
        } else {
            self.w(e, s);
        }
    }

    fn w(&self, e: &Expr, s: &mut String) {
        match e {
            Expr::Num(r) => s.push_str(&r.to_string()),
            Expr::Float(f) => s.push_str(&fmt_f64(*f, self.sig)),
            Expr::Sym(n) => s.push_str(match n.as_str() {
                "pi" => "π",
                "inf" => "∞",
                "theta" => "θ",
                other => other,
            }),
            Expr::Text(t) => s.push_str(t),
            Expr::Add(v) => {
                let mut terms: Vec<&Expr> = v.iter().collect();
                terms.sort_by(|a, b| display_degree(b).total_cmp(&display_degree(a)));
                for (i, t) in terms.iter().enumerate() {
                    let (neg, abs) = negate_for_print(t);
                    if i == 0 {
                        if neg {
                            s.push('-');
                        }
                    } else {
                        s.push_str(if neg { " - " } else { " + " });
                    }
                    self.wp(&abs, 2, s);
                }
            }
            Expr::Mul(v) => self.w_mul(v, s),
            Expr::Pow(b, x) => {
                if let Expr::Num(r) = x.as_ref() {
                    if *r == Rat::new(1, 2).unwrap() {
                        s.push('√');
                        self.wroot_arg(b, s);
                        return;
                    }
                    if r.is_neg() {
                        s.push_str("1/");
                        let pe = Rat { n: -r.n, d: r.d };
                        if pe.is_one() {
                            self.wp(b, 3, s);
                        } else {
                            self.w(&Expr::Pow(b.clone(), Box::new(Expr::Num(pe))), s);
                        }
                        return;
                    }
                }
                if let Expr::Sym(n) = b.as_ref() {
                    if n == "e" {
                        s.push('e');
                        s.push('^');
                        self.wexp(x, s);
                        return;
                    }
                }
                self.wp(b, 5, s);
                s.push('^');
                self.wexp(x, s);
            }
            Expr::Call(n, args) => {
                if n == "abs" && args.len() == 1 {
                    s.push('|');
                    self.w(&args[0], s);
                    s.push('|');
                    return;
                }
                if n == "fact" && args.len() == 1 {
                    self.wp(&args[0], 5, s);
                    s.push('!');
                    return;
                }
                s.push_str(n);
                s.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    self.w(a, s);
                }
                s.push(')');
            }
            Expr::Matrix(rows) => {
                s.push('[');
                for (i, r) in rows.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push('[');
                    for (j, c) in r.iter().enumerate() {
                        if j > 0 {
                            s.push_str(", ");
                        }
                        self.w(c, s);
                    }
                    s.push(']');
                }
                s.push(']');
            }
            Expr::List(v) => {
                s.push('{');
                for (i, c) in v.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    self.w(c, s);
                }
                s.push('}');
            }
            Expr::Eq(a, b) => {
                self.w(a, s);
                s.push_str(" = ");
                self.w(b, s);
            }
            Expr::Rel(op, a, b) => {
                self.w(a, s);
                s.push(' ');
                s.push_str(match op.as_str() {
                    "<=" => "≤",
                    ">=" => "≥",
                    "!=" => "≠",
                    o => o,
                });
                s.push(' ');
                self.w(b, s);
            }
        }
    }

    fn wroot_arg(&self, b: &Expr, s: &mut String) {
        match b {
            Expr::Num(r) if r.is_int() && !r.is_neg() => s.push_str(&r.to_string()),
            Expr::Sym(_) => self.w(b, s),
            _ => {
                s.push('(');
                self.w(b, s);
                s.push(')');
            }
        }
    }

    fn wexp(&self, x: &Expr, s: &mut String) {
        match x {
            Expr::Num(r) if r.is_int() && !r.is_neg() => s.push_str(&r.to_string()),
            Expr::Sym(_) => self.w(x, s),
            Expr::Float(f) if *f >= 0.0 => self.w(x, s),
            _ => {
                s.push('(');
                self.w(x, s);
                s.push(')');
            }
        }
    }

    fn w_mul(&self, v: &[Expr], s: &mut String) {
        let (coef, fcoef, num, den) = split_fraction(v);
        let c = coef.unwrap_or(Rat::ONE);
        // numeric prefix
        let mut out = String::new();
        let mut cden: i128 = 1;
        let mut has_prefix = false;
        if let Some(f) = fcoef {
            let mut val = f * c.to_f64();
            if val < 0.0 {
                s.push('-');
                val = -val;
            }
            if val != 1.0 || num.is_empty() {
                out.push_str(&fmt_f64(val, self.sig));
                has_prefix = true;
            }
        } else {
            let (n, d) = (c.n, c.d);
            if n < 0 {
                s.push('-');
            }
            let n = n.abs();
            cden = d;
            if n != 1 || num.is_empty() {
                out.push_str(&n.to_string());
                has_prefix = true;
            }
        }
        let multi = num.len() + has_prefix as usize > 1;
        let mut first = !has_prefix;
        let mut prev_is_coef = has_prefix;
        for f in &num {
            let mut t = String::new();
            self.wp(f, if multi || !den.is_empty() || cden != 1 { 3 } else { 2 }, &mut t);
            if !first {
                // implicit product only right after the plain numeric coefficient: 2x, 3sin(x)
                let next_alpha = t.chars().next().map(|c| !c.is_ascii_digit() && c != '-' && c != '(' && c != 'e' && c != 'E').unwrap_or(false);
                if !(prev_is_coef && next_alpha) {
                    out.push('·');
                }
            }
            out.push_str(&t);
            first = false;
            prev_is_coef = false;
        }
        if out.is_empty() {
            out.push('1');
        }
        s.push_str(&out);
        if cden != 1 || !den.is_empty() {
            s.push('/');
            let mut dparts: Vec<String> = Vec::new();
            if cden != 1 {
                dparts.push(cden.to_string());
            }
            for f in &den {
                let mut t = String::new();
                self.wp(f, 3, &mut t);
                dparts.push(t);
            }
            if dparts.len() > 1 {
                s.push('(');
                s.push_str(&dparts.join("·"));
                s.push(')');
            } else {
                s.push_str(&dparts[0]);
            }
        }
    }
}

/// For printing sums: returns (is_negative, absolute term).
pub fn negate_for_print(t: &Expr) -> (bool, Expr) {
    match t {
        Expr::Num(r) if r.is_neg() => (true, Expr::Num(Rat { n: -r.n, d: r.d })),
        Expr::Float(f) if *f < 0.0 => (true, Expr::Float(-f)),
        Expr::Mul(v) => {
            if let Some(Expr::Num(r)) = v.first() {
                if r.is_neg() {
                    let mut nv = v.clone();
                    let pos = Rat { n: -r.n, d: r.d };
                    if pos.is_one() {
                        nv.remove(0);
                    } else {
                        nv[0] = Expr::Num(pos);
                    }
                    let e = if nv.len() == 1 { nv.pop().unwrap() } else { Expr::Mul(nv) };
                    return (true, e);
                }
            }
            if let Some(Expr::Float(f)) = v.first() {
                if *f < 0.0 {
                    let mut nv = v.clone();
                    nv[0] = Expr::Float(-f);
                    return (true, Expr::Mul(nv));
                }
            }
            (false, t.clone())
        }
        _ => (false, t.clone()),
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&Printer { sig: 10 }.print(self))
    }
}
