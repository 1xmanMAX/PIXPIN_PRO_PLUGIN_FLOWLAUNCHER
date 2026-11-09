//! Integration tests of the engine (shared by all modules).
use crate::Engine;

pub fn ev(eng: &mut Engine, s: &str) -> String {
    match eng.eval_line(s) {
        Ok(o) => o.exact_str,
        Err(e) => format!("ERR: {e}"),
    }
}

pub fn ap(eng: &mut Engine, s: &str) -> String {
    match eng.eval_line(s) {
        Ok(o) => o.approx.unwrap_or(o.exact_str),
        Err(e) => format!("ERR: {e}"),
    }
}

#[test]
fn arithmetic() {
    let mut e = Engine::new();
    assert_eq!(ev(&mut e, "1/3 + 1/6"), "1/2");
    assert_eq!(ev(&mut e, "2^10"), "1024");
    assert_eq!(ev(&mut e, "0.1 + 0.2"), "3/10");
    assert_eq!(ev(&mut e, "sqrt(8)"), "2√2");
    assert_eq!(ev(&mut e, "5!"), "120");
    assert_eq!(ev(&mut e, "-2^2"), "-4");
    assert_eq!(ev(&mut e, "2^-1"), "1/2");
    assert_eq!(ev(&mut e, "sin(pi/6)"), "1/2");
    assert_eq!(ev(&mut e, "cos(pi)"), "-1");
    assert_eq!(ev(&mut e, "ln(e^3)"), "3");
    assert_eq!(ap(&mut e, "(1+2i)*(3-i)"), "5 + 5i");
    assert_eq!(ap(&mut e, "sin(1)"), "0.8414709848");
    assert_eq!(ev(&mut e, "ncr(5,2)"), "10");
    assert_eq!(ev(&mut e, "gcd(12, 18)"), "6");
    assert_eq!(ev(&mut e, "factor(360)"), "2^3·3^2·5");
}

#[test]
fn symbolic() {
    let mut e = Engine::new();
    assert_eq!(ev(&mut e, "x + x + 2x"), "4x");
    assert_eq!(ev(&mut e, "expand((x+1)^3)"), "x^3 + 3x^2 + 3x + 1");
    assert_eq!(ev(&mut e, "2(x+1) - 2x"), "2");
}

#[test]
fn variables_and_functions() {
    let mut e = Engine::new();
    ev(&mut e, "a := 5");
    assert_eq!(ev(&mut e, "a^2"), "25");
    ev(&mut e, "f(x) := x^2 + 1");
    assert_eq!(ev(&mut e, "f(3)"), "10");
    assert_eq!(ev(&mut e, "f(a+1)"), "37");
    assert_eq!(ev(&mut e, "ans + 1"), "38");
    ev(&mut e, "x := 2");
    assert_eq!(ev(&mut e, "f(x)"), "5");
}

#[test]
fn matrices() {
    let mut e = Engine::new();
    ev(&mut e, "A := [[1,2],[3,4]]");
    assert_eq!(ev(&mut e, "det(A)"), "-2");
    assert_eq!(ev(&mut e, "inv(A)"), "[[-2, 1], [3/2, -1/2]]");
    assert_eq!(ev(&mut e, "A*inv(A)"), "[[1, 0], [0, 1]]");
    assert_eq!(ev(&mut e, "A^2"), "[[7, 10], [15, 22]]");
    assert_eq!(ev(&mut e, "trn(A)"), "[[1, 3], [2, 4]]");
    assert_eq!(ev(&mut e, "[1,2;3,4] + [1,1;1,1]"), "[[2, 3], [4, 5]]");
    assert_eq!(ev(&mut e, "rref([[1,2,3],[4,5,6]])"), "[[1, 0, -1], [0, 1, 2]]");
}

#[test]
fn lists() {
    let mut e = Engine::new();
    assert_eq!(ev(&mut e, "{1,2,3}*2"), "{2, 4, 6}");
    assert_eq!(ev(&mut e, "{1,4,9}^(1/2)"), "{1, 2, 3}");
}

#[test]
fn plot_stmt() {
    let mut e = Engine::new();
    let o = e.eval_line("plot(sin(x), x^2, -3, 3)").unwrap();
    match o.kind {
        crate::OutKind::Plot(p) => {
            assert_eq!(p.exprs.len(), 2);
            assert_eq!(p.xmin, -3.0);
        }
        _ => panic!(),
    }
    let c = e.compile_src("x^2+1", &["x"]).unwrap();
    assert_eq!(c.eval1(2.0), 5.0);
}
