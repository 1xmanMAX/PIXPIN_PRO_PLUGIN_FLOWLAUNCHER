//! Exact/symbolic matrix algebra over canonical expressions.

use crate::expr::Expr;
use crate::simplify::{add, div, expand, mul, neg, sub, R};

pub type M = Vec<Vec<Expr>>;

pub fn dims(m: &M) -> (usize, usize) {
    (m.len(), m.first().map_or(0, |r| r.len()))
}

pub fn identity(n: usize) -> M {
    (0..n).map(|i| (0..n).map(|j| if i == j { Expr::one() } else { Expr::zero() }).collect()).collect()
}

pub fn zeros(r: usize, c: usize) -> M {
    vec![vec![Expr::zero(); c]; r]
}

pub fn map_m(m: &M, mut f: impl FnMut(&Expr) -> R) -> Result<M, String> {
    m.iter().map(|r| r.iter().map(&mut f).collect()).collect()
}

pub fn add_m(a: &M, b: &M) -> Result<M, String> {
    if dims(a) != dims(b) {
        return Err(format!("Dimensiones incompatibles para suma: {}×{} y {}×{}", a.len(), dims(a).1, b.len(), dims(b).1));
    }
    a.iter().zip(b).map(|(ra, rb)| ra.iter().zip(rb).map(|(x, y)| add(vec![x.clone(), y.clone()])).collect()).collect()
}

pub fn mul_m(a: &M, b: &M) -> Result<M, String> {
    let (ar, ac) = dims(a);
    let (br, bc) = dims(b);
    if ac != br {
        return Err(format!("Dimensiones incompatibles para multiplicar: {ar}×{ac} · {br}×{bc}"));
    }
    let mut out = Vec::with_capacity(ar);
    for i in 0..ar {
        let mut row = Vec::with_capacity(bc);
        for j in 0..bc {
            let mut terms = Vec::with_capacity(ac);
            for k in 0..ac {
                terms.push(mul(vec![a[i][k].clone(), b[k][j].clone()])?);
            }
            row.push(add(terms)?);
        }
        out.push(row);
    }
    Ok(out)
}

pub fn transpose(m: &M) -> M {
    let (r, c) = dims(m);
    (0..c).map(|j| (0..r).map(|i| m[i][j].clone()).collect()).collect()
}

pub fn pow_m(m: &M, k: i128) -> Result<M, String> {
    let (r, c) = dims(m);
    if r != c {
        return Err("La potencia requiere una matriz cuadrada".into());
    }
    if k < 0 {
        let inv = inverse(m)?;
        return pow_m(&inv, -k);
    }
    if k > 100_000 {
        return Err("Exponente demasiado grande".into());
    }
    let mut acc = identity(r);
    let mut base = m.clone();
    let mut e = k;
    while e > 0 {
        if e & 1 == 1 {
            acc = mul_m(&acc, &base)?;
        }
        e >>= 1;
        if e > 0 {
            base = mul_m(&base, &base)?;
        }
    }
    Ok(acc)
}

fn is_zero_expr(e: &Expr) -> bool {
    if e.is_zero() {
        return true;
    }
    match e.as_f64() {
        Some(f) => f.abs() < 1e-13,
        None => expand(e).map(|x| x.is_zero()).unwrap_or(false),
    }
}

fn pivot_score(e: &Expr) -> u8 {
    match e {
        Expr::Num(_) => 0,
        Expr::Float(_) => 1,
        _ if !e.has_free_symbols() => 2,
        _ => 3,
    }
}

pub fn det(m: &M) -> R {
    let (r, c) = dims(m);
    if r != c {
        return Err("El determinante requiere una matriz cuadrada".into());
    }
    match r {
        0 => return Ok(Expr::one()),
        1 => return Ok(m[0][0].clone()),
        2 => return sub(mul(vec![m[0][0].clone(), m[1][1].clone()])?, mul(vec![m[0][1].clone(), m[1][0].clone()])?),
        3 if m.iter().flatten().any(|e| e.has_free_symbols()) => {
            // cofactor expansion gives cleaner symbolic results
            let mut terms = Vec::new();
            for j in 0..3 {
                let minor: M = (1..3).map(|i| (0..3).filter(|&k| k != j).map(|k| m[i][k].clone()).collect()).collect();
                let d = det(&minor)?;
                let t = mul(vec![m[0][j].clone(), d])?;
                terms.push(if j % 2 == 1 { neg(t)? } else { t });
            }
            return expand(&add(terms)?);
        }
        _ => {}
    }
    // Gaussian elimination (fraction-producing but exact on rationals)
    let mut a = m.clone();
    let mut sign = false;
    let mut d = Expr::one();
    for col in 0..r {
        let piv = (col..r).filter(|&i| !is_zero_expr(&a[i][col])).min_by_key(|&i| pivot_score(&a[i][col]));
        let p = match piv {
            Some(p) => p,
            None => return Ok(Expr::zero()),
        };
        if p != col {
            a.swap(p, col);
            sign = !sign;
        }
        let pv = a[col][col].clone();
        d = mul(vec![d, pv.clone()])?;
        for i in col + 1..r {
            if is_zero_expr(&a[i][col]) {
                continue;
            }
            let f = div(a[i][col].clone(), pv.clone())?;
            for j in col..r {
                let v = sub(a[i][j].clone(), mul(vec![f.clone(), a[col][j].clone()])?)?;
                a[i][j] = v;
            }
        }
    }
    let d = if d.has_free_symbols() { expand(&d)? } else { d };
    if sign {
        neg(d)
    } else {
        Ok(d)
    }
}

/// Reduced row echelon form. Returns (rref, pivot columns).
pub fn rref(m: &M) -> Result<(M, Vec<usize>), String> {
    let (r, c) = dims(m);
    let mut a = m.clone();
    let mut pivots = Vec::new();
    let mut row = 0;
    for col in 0..c {
        if row >= r {
            break;
        }
        let piv = (row..r).filter(|&i| !is_zero_expr(&a[i][col])).min_by_key(|&i| pivot_score(&a[i][col]));
        let p = match piv {
            Some(p) => p,
            None => continue,
        };
        a.swap(p, row);
        let pv = a[row][col].clone();
        for j in 0..c {
            a[row][j] = div(a[row][j].clone(), pv.clone())?;
        }
        for i in 0..r {
            if i != row && !is_zero_expr(&a[i][col]) {
                let f = a[i][col].clone();
                for j in 0..c {
                    let v = sub(a[i][j].clone(), mul(vec![f.clone(), a[row][j].clone()])?)?;
                    a[i][j] = v;
                }
            }
        }
        pivots.push(col);
        row += 1;
    }
    // clean tiny floats
    for rw in a.iter_mut() {
        for e in rw.iter_mut() {
            if let Expr::Float(f) = e {
                if f.abs() < 1e-13 {
                    *e = Expr::zero();
                }
            }
        }
    }
    Ok((a, pivots))
}

pub fn inverse(m: &M) -> Result<M, String> {
    let (r, c) = dims(m);
    if r != c {
        return Err("La inversa requiere una matriz cuadrada".into());
    }
    if r == 2 && m.iter().flatten().any(|e| e.has_free_symbols()) {
        let d = det(m)?;
        if is_zero_expr(&d) {
            return Err("La matriz es singular (det = 0), no tiene inversa".into());
        }
        let adj = vec![vec![m[1][1].clone(), neg(m[0][1].clone())?], vec![neg(m[1][0].clone())?, m[0][0].clone()]];
        return map_m(&adj, |e| div(e.clone(), d.clone()));
    }
    let mut aug: M = m.clone();
    let id = identity(r);
    for (i, row) in aug.iter_mut().enumerate() {
        row.extend(id[i].iter().cloned());
    }
    let (red, piv) = rref(&aug)?;
    if piv.len() < r || piv[r - 1] != r - 1 {
        return Err("La matriz es singular (det = 0), no tiene inversa".into());
    }
    Ok(red.into_iter().map(|row| row[r..].to_vec()).collect())
}

pub fn rank(m: &M) -> Result<usize, String> {
    Ok(rref(m)?.1.len())
}

pub fn trace(m: &M) -> R {
    let (r, c) = dims(m);
    if r != c {
        return Err("La traza requiere una matriz cuadrada".into());
    }
    add((0..r).map(|i| m[i][i].clone()).collect())
}

/// Basis of the null space (as column vectors).
pub fn nullspace(m: &M) -> Result<Vec<Vec<Expr>>, String> {
    let (_, c) = dims(m);
    let (red, piv) = rref(m)?;
    let free: Vec<usize> = (0..c).filter(|j| !piv.contains(j)).collect();
    let mut out = Vec::new();
    for &f in &free {
        let mut v = vec![Expr::zero(); c];
        v[f] = Expr::one();
        for (k, &pc) in piv.iter().enumerate() {
            v[pc] = neg(red[k][f].clone())?;
        }
        out.push(v);
    }
    Ok(out)
}

/// Characteristic polynomial coefficients (Faddeev–LeVerrier), highest degree first:
/// det(λI - A) = λ^n + c1 λ^(n-1) + ... + cn
pub fn charpoly(m: &M) -> Result<Vec<Expr>, String> {
    let (n, c) = dims(m);
    if n != c {
        return Err("Se requiere una matriz cuadrada".into());
    }
    let mut coeffs = vec![Expr::one()];
    let mut mk = zeros(n, n);
    let id = identity(n);
    let mut ck = Expr::one();
    for k in 1..=n {
        // M_k = A M_{k-1} + c_{k-1} I
        let am = mul_m(m, &mk)?;
        let ci = map_m(&id, |e| mul(vec![e.clone(), ck.clone()]))?;
        mk = add_m(&am, &ci)?;
        let amk = mul_m(m, &mk)?;
        let tr = trace(&amk)?;
        ck = div(neg(tr)?, Expr::int(k as i128))?;
        coeffs.push(ck.clone());
    }
    Ok(coeffs)
}

pub fn dot(a: &[Expr], b: &[Expr]) -> R {
    if a.len() != b.len() {
        return Err("Los vectores deben tener la misma dimensión".into());
    }
    let t: Result<Vec<Expr>, String> = a.iter().zip(b).map(|(x, y)| mul(vec![x.clone(), y.clone()])).collect();
    add(t?)
}

pub fn cross(a: &[Expr], b: &[Expr]) -> Result<Vec<Expr>, String> {
    if a.len() != 3 || b.len() != 3 {
        return Err("El producto cruz requiere vectores de 3 componentes".into());
    }
    let c = |i: usize, j: usize| -> R { sub(mul(vec![a[i].clone(), b[j].clone()])?, mul(vec![a[j].clone(), b[i].clone()])?) };
    Ok(vec![c(1, 2)?, c(2, 0)?, c(0, 1)?])
}

/// Flattens a vector-like expression (row/column matrix or list) to its elements.
pub fn as_vector(e: &Expr) -> Option<Vec<Expr>> {
    match e {
        Expr::List(v) => Some(v.clone()),
        Expr::Matrix(m) if m.len() == 1 => Some(m[0].clone()),
        Expr::Matrix(m) if m.iter().all(|r| r.len() == 1) => Some(m.iter().map(|r| r[0].clone()).collect()),
        _ => None,
    }
}
