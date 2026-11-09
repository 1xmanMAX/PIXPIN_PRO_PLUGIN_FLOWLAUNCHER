//! Geometry helpers: distances, triangles, polygons.
use crate::eval::Cas;
use crate::expr::{Expr, Printer};
use crate::matrix::as_vector;
use crate::num::fmt_f64;
use crate::simplify::{add, div, mul, pow, sub, R};
use std::f64::consts::PI;

/// Known parts of a triangle. Sides a, b, c are opposite to angles alpha, beta, gamma.
/// Angles are in RADIANS.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TriInput {
    pub a: Option<f64>,
    pub b: Option<f64>,
    pub c: Option<f64>,
    pub alpha: Option<f64>,
    pub beta: Option<f64>,
    pub gamma: Option<f64>,
}

/// A fully solved triangle (angles in radians).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Triangle {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
    pub area: f64,
    pub perimeter: f64,
    pub inradius: f64,
    pub circumradius: f64,
}

impl Triangle {
    /// "equilátero" / "isósceles" / "escaleno".
    pub fn side_type(&self) -> &'static str {
        let eq = |x: f64, y: f64| (x - y).abs() <= 1e-9 * x.max(y);
        if eq(self.a, self.b) && eq(self.b, self.c) {
            "equilátero"
        } else if eq(self.a, self.b) || eq(self.b, self.c) || eq(self.a, self.c) {
            "isósceles"
        } else {
            "escaleno"
        }
    }
    /// "acutángulo" / "rectángulo" / "obtusángulo".
    pub fn angle_type(&self) -> &'static str {
        let m = self.alpha.max(self.beta).max(self.gamma);
        if (m - PI / 2.0).abs() < 1e-9 {
            "rectángulo"
        } else if m > PI / 2.0 {
            "obtusángulo"
        } else {
            "acutángulo"
        }
    }
}

const TOL: f64 = 1e-9;

fn close(x: f64, y: f64) -> bool {
    (x - y).abs() <= TOL * (1.0 + x.abs().max(y.abs()))
}

/// Completes a triangle from its three sides and three angles.
fn finish(s: [f64; 3], ang: [f64; 3]) -> Triangle {
    let [a, b, c] = s;
    let p = a + b + c;
    let area = 0.5 * b * c * ang[0].sin();
    Triangle {
        a,
        b,
        c,
        alpha: ang[0],
        beta: ang[1],
        gamma: ang[2],
        area,
        perimeter: p,
        inradius: area / (p / 2.0),
        circumradius: a * b * c / (4.0 * area),
    }
}

/// Angles of a triangle from its sides (law of cosines); validates the triangle inequality.
fn sss(s: [f64; 3]) -> Result<[f64; 3], String> {
    let [a, b, c] = s;
    let m = a.max(b).max(c);
    if m >= (a + b + c - m) * (1.0 - 1e-12) {
        return Err("Los lados no cumplen la desigualdad triangular: no forman un triángulo".into());
    }
    let ang = |x: f64, y: f64, z: f64| ((y * y + z * z - x * x) / (2.0 * y * z)).clamp(-1.0, 1.0).acos();
    let alpha = ang(a, b, c);
    let beta = ang(b, c, a);
    Ok([alpha, beta, PI - alpha - beta])
}

fn check_given(t: &Triangle, sides: &[Option<f64>; 3], angles: &[Option<f64>; 3]) -> bool {
    let s = [t.a, t.b, t.c];
    let g = [t.alpha, t.beta, t.gamma];
    (0..3).all(|i| sides[i].map_or(true, |v| close(v, s[i])) && angles[i].map_or(true, |v| close(v, g[i])))
}

/// Solves SSS, SAS, ASA, AAS and SSA (ambiguous case may return 2 triangles,
/// and an empty vector when no triangle has the given SSA data).
/// Any extra given value is checked for consistency.
pub fn solve_triangle(t: &TriInput) -> Result<Vec<Triangle>, String> {
    let sides = [t.a, t.b, t.c];
    let angles = [t.alpha, t.beta, t.gamma];
    for s in sides.iter().flatten() {
        if !s.is_finite() || *s <= 0.0 {
            return Err("Los lados deben ser números positivos".into());
        }
    }
    for a in angles.iter().flatten() {
        if !a.is_finite() || *a <= 0.0 || *a >= PI {
            return Err("Cada ángulo debe estar entre 0 y 180° (0 y π rad)".into());
        }
    }
    let ks = sides.iter().flatten().count();
    let ka = angles.iter().flatten().count();
    if ks == 0 {
        return Err("Datos insuficientes: se necesita al menos un lado".into());
    }
    if ks + ka < 3 {
        return Err("Datos insuficientes: se necesitan 3 valores con al menos un lado".into());
    }
    let inconsistent = || "Datos inconsistentes: los valores dados no forman un único triángulo".to_string();

    let result: Vec<Triangle> = if ka >= 2 {
        // ASA / AAS: third angle, then law of sines
        let sum: f64 = angles.iter().flatten().sum();
        let mut g = [0.0; 3];
        for i in 0..3 {
            g[i] = match angles[i] {
                Some(v) => v,
                None => PI - sum,
            };
        }
        if ka == 3 && !close(sum, PI) {
            return Err("Los ángulos deben sumar 180° (π rad)".into());
        }
        if g.iter().any(|&v| v <= TOL) {
            return Err("Los ángulos dados suman 180° o más: no hay triángulo".into());
        }
        let i = (0..3).find(|&i| sides[i].is_some()).unwrap();
        let k = sides[i].unwrap() / g[i].sin();
        let s = [k * g[0].sin(), k * g[1].sin(), k * g[2].sin()];
        vec![finish(s, g)]
    } else if ks == 3 {
        let s = [sides[0].unwrap(), sides[1].unwrap(), sides[2].unwrap()];
        vec![finish(s, sss(s)?)]
    } else {
        // two sides, one angle
        let j = (0..3).find(|&i| angles[i].is_some()).unwrap();
        let aj = angles[j].unwrap();
        if sides[j].is_none() {
            // SAS: the angle is between the two known sides
            let (p, q) = ((j + 1) % 3, (j + 2) % 3);
            let (sp, sq) = (sides[p].unwrap(), sides[q].unwrap());
            let mut s = [0.0; 3];
            s[p] = sp;
            s[q] = sq;
            s[j] = (sp * sp + sq * sq - 2.0 * sp * sq * aj.cos()).max(0.0).sqrt();
            let g = sss(s)?;
            vec![finish(s, g)]
        } else {
            // SSA: angle opposite a known side
            let p = (0..3).find(|&i| i != j && sides[i].is_some()).unwrap();
            let q = 3 - j - p;
            let (sj, sp) = (sides[j].unwrap(), sides[p].unwrap());
            let mut sin_p = sp * aj.sin() / sj;
            if (sin_p - 1.0).abs() < 1e-12 {
                sin_p = 1.0; // right angle (asin is ill-conditioned near 1)
            }
            if sin_p > 1.0 + 1e-12 {
                return Ok(vec![]);
            }
            let b1 = sin_p.min(1.0).asin();
            let mut cands = vec![b1];
            if sin_p < 1.0 - 1e-12 {
                cands.push(PI - b1);
            }
            let mut out = Vec::new();
            for bp in cands {
                let bq = PI - aj - bp;
                if bq <= TOL {
                    continue;
                }
                let mut g = [0.0; 3];
                g[j] = aj;
                g[p] = bp;
                g[q] = bq;
                let mut s = [0.0; 3];
                s[j] = sj;
                s[p] = sp;
                s[q] = sj * bq.sin() / aj.sin();
                out.push(finish(s, g));
            }
            out
        }
    };
    if result.iter().any(|tr| !check_given(tr, &sides, &angles)) {
        return Err(inconsistent());
    }
    Ok(result)
}

// ====================================================================== commands

fn printer(cas: &Cas) -> Printer {
    Printer { sig: cas.s.digits }
}

/// Exact form plus " ≈ decimal" when useful.
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

fn vec_arg(e: &Expr, name: &str) -> Result<Vec<Expr>, String> {
    as_vector(e).ok_or_else(|| format!("{name}: se esperaba un punto o vector, p. ej. {{1, 2}}"))
}

/// The two points of distance/midpoint: (x1,y1,x2,y2), (x1,y1,z1,x2,y2,z2) or (P, Q).
fn two_points(name: &str, args: &[Expr]) -> Result<(Vec<Expr>, Vec<Expr>), String> {
    let (p, q) = match args.len() {
        2 => (vec_arg(&args[0], name)?, vec_arg(&args[1], name)?),
        4 => (args[..2].to_vec(), args[2..].to_vec()),
        6 => (args[..3].to_vec(), args[3..].to_vec()),
        _ => return Err(format!("Uso: {name}(x1, y1, x2, y2) o {name}(P, Q)")),
    };
    if p.len() != q.len() || p.is_empty() {
        return Err(format!("{name}: los puntos deben tener la misma dimensión"));
    }
    Ok((p, q))
}

fn sqrt_e(e: Expr) -> R {
    pow(e, Expr::rat(1, 2))
}

fn norm2(v: &[Expr]) -> R {
    add(v.iter().map(|x| pow(x.clone(), Expr::int(2))).collect::<Result<_, _>>()?)
}

fn dot(u: &[Expr], v: &[Expr]) -> R {
    add(u.iter().zip(v).map(|(a, b)| mul(vec![a.clone(), b.clone()])).collect::<Result<_, _>>()?)
}

fn real(cas: &Cas, e: &Expr, what: &str) -> Result<f64, String> {
    if e.has_free_symbols() {
        return Err(format!("{what} debe ser numérico"));
    }
    cas.num_real(e).map_err(|_| format!("{what} debe ser un número real"))
}

/// Exact Heron area. Validates the triangle inequality for numeric sides.
fn heron_e(cas: &Cas, a: &Expr, b: &Expr, c: &Expr) -> R {
    if [a, b, c].iter().all(|x| !x.has_free_symbols()) {
        let v = [real(cas, a, "Los lados")?, real(cas, b, "Los lados")?, real(cas, c, "Los lados")?];
        if v.iter().any(|x| *x <= 0.0) {
            return Err("Los lados deben ser positivos".into());
        }
        sss(v)?;
    }
    // A = √((a+b+c)(-a+b+c)(a-b+c)(a+b-c))/4
    let p = add(vec![a.clone(), b.clone(), c.clone()])?;
    let f1 = sub(add(vec![b.clone(), c.clone()])?, a.clone())?;
    let f2 = sub(add(vec![a.clone(), c.clone()])?, b.clone())?;
    let f3 = sub(add(vec![a.clone(), b.clone()])?, c.clone())?;
    let prod = mul(vec![p, f1, f2, f3])?;
    let prod = if prod.has_free_symbols() { crate::simplify::expand(&prod)? } else { prod };
    div(sqrt_e(prod)?, Expr::int(4))
}

fn fmt_angle(cas: &Cas, rad: f64) -> String {
    if cas.s.deg {
        format!("{}°", fmt_f64(rad.to_degrees(), cas.s.digits))
    } else {
        format!("{} rad", fmt_f64(rad, cas.s.digits))
    }
}

pub fn cmd(cas: &Cas, name: &str, args: Vec<Expr>) -> R {
    match name {
        "distance" => {
            let (p, q) = two_points(name, &args)?;
            let d: Vec<Expr> = p.iter().zip(&q).map(|(a, b)| sub(b.clone(), a.clone())).collect::<Result<_, _>>()?;
            sqrt_e(norm2(&d)?)
        }
        "midpoint" => {
            let (p, q) = two_points(name, &args)?;
            Ok(Expr::List(p.iter().zip(&q).map(|(a, b)| div(add(vec![a.clone(), b.clone()])?, Expr::int(2))).collect::<Result<_, _>>()?))
        }
        "heron" => {
            if args.len() != 3 {
                return Err("Uso: heron(a, b, c)".into());
            }
            heron_e(cas, &args[0], &args[1], &args[2])
        }
        "triangle" => {
            if args.len() != 3 {
                return Err("Uso: triangle(a, b, c) con los tres lados".into());
            }
            let s = [real(cas, &args[0], "Los lados")?, real(cas, &args[1], "Los lados")?, real(cas, &args[2], "Los lados")?];
            let tr = solve_triangle(&TriInput { a: Some(s[0]), b: Some(s[1]), c: Some(s[2]), ..Default::default() })?;
            let t = tr[0];
            let area = heron_e(cas, &args[0], &args[1], &args[2])?;
            let per = add(args.clone())?;
            let semi = div(per.clone(), Expr::int(2))?;
            let inr = div(area.clone(), semi)?;
            let circ = div(mul(args.clone())?, mul(vec![Expr::int(4), area.clone()])?)?;
            let p = printer(cas);
            let lines = [
                format!("Lados: a = {}, b = {}, c = {}", p.print(&args[0]), p.print(&args[1]), p.print(&args[2])),
                format!("Ángulos: α = {}, β = {}, γ = {}", fmt_angle(cas, t.alpha), fmt_angle(cas, t.beta), fmt_angle(cas, t.gamma)),
                format!("Área = {}", show(cas, &area)),
                format!("Perímetro = {}", show(cas, &per)),
                format!("Inradio r = {}", show(cas, &inr)),
                format!("Circunradio R = {}", show(cas, &circ)),
                format!("Tipo: {}, {}", t.side_type(), t.angle_type()),
            ];
            Ok(Expr::Text(lines.join("\n")))
        }
        "polyarea" => {
            let (xs, ys) = match args.len() {
                2 => (vec_arg(&args[0], name)?, vec_arg(&args[1], name)?),
                1 => {
                    // list of points {{x1,y1},...} or n×2 matrix
                    let rows: Vec<Vec<Expr>> = match &args[0] {
                        Expr::Matrix(m) => m.clone(),
                        Expr::List(v) => v.iter().map(|p| vec_arg(p, name)).collect::<Result<_, _>>()?,
                        _ => return Err("Uso: polyarea({x1, x2, ...}, {y1, y2, ...})".into()),
                    };
                    if rows.iter().any(|r| r.len() != 2) {
                        return Err("polyarea: cada vértice debe tener 2 coordenadas".into());
                    }
                    (rows.iter().map(|r| r[0].clone()).collect(), rows.iter().map(|r| r[1].clone()).collect())
                }
                _ => return Err("Uso: polyarea({x1, x2, ...}, {y1, y2, ...})".into()),
            };
            if xs.len() != ys.len() {
                return Err("polyarea: las listas deben tener la misma longitud".into());
            }
            if xs.len() < 3 {
                return Err("polyarea: se necesitan al menos 3 vértices".into());
            }
            let n = xs.len();
            let mut terms = Vec::with_capacity(n);
            for i in 0..n {
                let j = (i + 1) % n;
                terms.push(sub(mul(vec![xs[i].clone(), ys[j].clone()])?, mul(vec![xs[j].clone(), ys[i].clone()])?)?);
            }
            let s = add(terms)?;
            let s = if s.has_free_symbols() { crate::simplify::expand(&s)? } else { s };
            div(cas.apply("abs", vec![s])?, Expr::int(2))
        }
        "angle" => {
            let (u, v) = match args.len() {
                2 => (vec_arg(&args[0], name)?, vec_arg(&args[1], name)?),
                _ => return Err("Uso: angle(u, v) con dos vectores".into()),
            };
            if u.len() != v.len() {
                return Err("angle: los vectores deben tener la misma dimensión".into());
            }
            let nu = norm2(&u)?;
            let nv = norm2(&v)?;
            if nu.is_zero() || nv.is_zero() {
                return Err("angle: el vector nulo no tiene dirección".into());
            }
            let c = div(dot(&u, &v)?, sqrt_e(mul(vec![nu, nv])?)?)?;
            let c = match c {
                Expr::Float(f) => Expr::Float(f.clamp(-1.0, 1.0)),
                other => other,
            };
            cas.apply("acos", vec![c])
        }
        _ => Err(format!("{name}: no implementado")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{ap, ev};
    use crate::Engine;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * (1.0 + b.abs())
    }

    #[test]
    fn sss_345() {
        let t = solve_triangle(&TriInput { a: Some(3.0), b: Some(4.0), c: Some(5.0), ..Default::default() }).unwrap();
        assert_eq!(t.len(), 1);
        let t = t[0];
        assert!(near(t.gamma, PI / 2.0));
        assert!(near(t.area, 6.0) && near(t.perimeter, 12.0));
        assert!(near(t.inradius, 1.0) && near(t.circumradius, 2.5));
        assert_eq!(t.side_type(), "escaleno");
        assert_eq!(t.angle_type(), "rectángulo");
        assert!(solve_triangle(&TriInput { a: Some(1.0), b: Some(2.0), c: Some(3.0), ..Default::default() }).is_err());
        assert!(solve_triangle(&TriInput { a: Some(1.0), b: Some(2.0), c: Some(5.0), ..Default::default() }).is_err());
    }

    #[test]
    fn sas_asa_aas() {
        // SAS: b=4, c=3, α=90° -> a=5
        let t = solve_triangle(&TriInput { b: Some(4.0), c: Some(3.0), alpha: Some(PI / 2.0), ..Default::default() }).unwrap()[0];
        assert!(near(t.a, 5.0) && near(t.area, 6.0));
        // right angle + two legs (gamma = 90°)
        let t = solve_triangle(&TriInput { a: Some(3.0), b: Some(4.0), gamma: Some(PI / 2.0), ..Default::default() }).unwrap()[0];
        assert!(near(t.c, 5.0));
        // hypotenuse + leg + right angle (SSA, unique)
        let r = solve_triangle(&TriInput { a: Some(3.0), c: Some(5.0), gamma: Some(PI / 2.0), ..Default::default() }).unwrap();
        assert_eq!(r.len(), 1);
        assert!(near(r[0].b, 4.0));
        // ASA: equilateral
        let t = solve_triangle(&TriInput { c: Some(2.0), alpha: Some(PI / 3.0), beta: Some(PI / 3.0), ..Default::default() }).unwrap()[0];
        assert!(near(t.a, 2.0) && near(t.b, 2.0) && near(t.gamma, PI / 3.0));
        assert_eq!(t.side_type(), "equilátero");
        assert_eq!(t.angle_type(), "acutángulo");
        // AAS
        let t = solve_triangle(&TriInput { a: Some(1.0), alpha: Some(PI / 6.0), beta: Some(PI / 2.0), ..Default::default() }).unwrap()[0];
        assert!(near(t.b, 2.0) && near(t.c, 3f64.sqrt()));
        // angles too big
        assert!(solve_triangle(&TriInput { a: Some(1.0), alpha: Some(2.0), beta: Some(1.5), ..Default::default() }).is_err());
    }

    #[test]
    fn ssa_ambiguous() {
        // a=6, b=8, α=30°: sinβ = 2/3 -> two triangles
        let r = solve_triangle(&TriInput { a: Some(6.0), b: Some(8.0), alpha: Some(PI / 6.0), ..Default::default() }).unwrap();
        assert_eq!(r.len(), 2);
        assert!(near(r[0].beta, (2.0f64 / 3.0).asin()));
        assert!(near(r[1].beta, PI - (2.0f64 / 3.0).asin()));
        for t in &r {
            assert!(near(t.alpha + t.beta + t.gamma, PI));
            assert!(near(t.a / t.alpha.sin(), t.c / t.gamma.sin()));
        }
        // no solution: a=2, b=8, α=30°
        let r = solve_triangle(&TriInput { a: Some(2.0), b: Some(8.0), alpha: Some(PI / 6.0), ..Default::default() }).unwrap();
        assert!(r.is_empty());
        // exactly one (right triangle): a=4, b=8, α=30°
        let r = solve_triangle(&TriInput { a: Some(4.0), b: Some(8.0), alpha: Some(PI / 6.0), ..Default::default() }).unwrap();
        assert_eq!(r.len(), 1);
        assert!(near(r[0].beta, PI / 2.0));
        // one: a > b
        let r = solve_triangle(&TriInput { a: Some(10.0), b: Some(8.0), alpha: Some(PI / 6.0), ..Default::default() }).unwrap();
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn insufficient_inconsistent() {
        assert!(solve_triangle(&TriInput { a: Some(1.0), b: Some(1.0), ..Default::default() }).is_err());
        assert!(solve_triangle(&TriInput { alpha: Some(1.0), beta: Some(1.0), gamma: Some(PI - 2.0), ..Default::default() }).is_err());
        assert!(solve_triangle(&TriInput { a: Some(3.0), b: Some(4.0), c: Some(5.0), gamma: Some(1.0), ..Default::default() }).is_err());
        assert!(solve_triangle(&TriInput { a: Some(3.0), b: Some(4.0), c: Some(5.0), gamma: Some(PI / 2.0), ..Default::default() }).is_ok());
        assert!(solve_triangle(&TriInput { a: Some(-3.0), b: Some(4.0), c: Some(5.0), ..Default::default() }).is_err());
    }

    #[test]
    fn commands() {
        let mut e = Engine::new();
        assert_eq!(ev(&mut e, "distance(0, 0, 3, 4)"), "5");
        assert_eq!(ev(&mut e, "distance(1, 1, 2, 2)"), "√2");
        assert_eq!(ev(&mut e, "distance({1,2,3}, {2,4,5})"), "3");
        assert_eq!(ev(&mut e, "distance([1,2], [3,3])"), "√5");
        assert_eq!(ev(&mut e, "midpoint(0, 0, 3, 4)"), "{3/2, 2}");
        assert_eq!(ev(&mut e, "midpoint({1,2,3}, {3,4,5})"), "{2, 3, 4}");
        assert_eq!(ev(&mut e, "heron(3, 4, 5)"), "6");
        assert_eq!(ev(&mut e, "heron(1, 1, 1)"), "√3/4");
        assert!(ev(&mut e, "heron(1, 2, 3)").starts_with("ERR"));
        assert_eq!(ev(&mut e, "polyarea({0,4,4,0},{0,0,3,3})"), "12");
        assert_eq!(ev(&mut e, "polyarea({0,1,0},{0,0,1})"), "1/2");
        assert_eq!(ev(&mut e, "polyarea({{0,0},{0,1},{1,0}})"), "1/2");
        assert_eq!(ev(&mut e, "angle({1,0},{0,1})"), "π/2");
        assert_eq!(ev(&mut e, "angle({1,0},{1,1})"), "π/4");
        assert_eq!(ap(&mut e, "angle({1,2},{3,1})"), "0.7853981634");
        assert_eq!(ap(&mut e, "angle({1,0,0},{1,2,2})"), "1.230959417");
        e.settings.deg = true;
        assert_eq!(ev(&mut e, "angle({1,0},{-1,1})"), "135");
        let t = ev(&mut e, "triangle(3, 4, 5)");
        assert_eq!(
            t,
            "Lados: a = 3, b = 4, c = 5\nÁngulos: α = 36.86989765°, β = 53.13010235°, γ = 90°\nÁrea = 6\nPerímetro = 12\nInradio r = 1\nCircunradio R = 5/2 ≈ 2.5\nTipo: escaleno, rectángulo"
        );
        e.settings.deg = false;
        let t = ev(&mut e, "triangle(2, 2, 2)");
        assert!(t.contains("α = 1.047197551 rad"), "{t}");
        assert!(t.contains("Área = √3"), "{t}");
        assert!(t.contains("Tipo: equilátero, acutángulo"), "{t}");
        assert!(ev(&mut e, "triangle(2, 3, 6)").starts_with("ERR"));
        assert!(ev(&mut e, "triangle(2, 2, 3)").contains("isósceles, obtusángulo"));
    }
}
