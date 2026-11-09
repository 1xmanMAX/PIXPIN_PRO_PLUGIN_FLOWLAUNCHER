//! Tokenizer + Pratt parser with implicit multiplication and Unicode math symbols.

use crate::expr::Expr;
use crate::num::Rat;

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(String),
    Ident(String),
    Op(char), // + - * / ^ ! % °
    LParen,
    RParen,
    LBrack,
    RBrack,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Assign,      // :=
    Eq,          // =
    Rel(String), // < > <= >= !=
    Sqrt,        // √
    Cbrt,        // ∛
    Pipe,        // |
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).map_or(false, |d| d.is_ascii_digit())) {
            let st = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            // exponent: 1e5, 2.5E-3 (only if followed by digit)
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let j = i + 1;
                let k = if j < chars.len() && (chars[j] == '+' || chars[j] == '-') { j + 1 } else { j };
                if k < chars.len() && chars[k].is_ascii_digit() {
                    i = k;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let s: String = chars[st..i].iter().collect();
            if s.matches('.').count() > 1 {
                return Err(format!("Número inválido: {s}"));
            }
            out.push(Tok::Num(s));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            match c {
                'π' => {
                    out.push(Tok::Ident("pi".into()));
                    i += 1;
                    continue;
                }
                'θ' => {
                    out.push(Tok::Ident("theta".into()));
                    i += 1;
                    continue;
                }
                _ => {}
            }
            let st = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') && chars[i] != 'π' {
                i += 1;
            }
            let s: String = chars[st..i].iter().collect();
            out.push(Tok::Ident(s));
            continue;
        }
        i += 1;
        let next = chars.get(i).copied();
        let t = match c {
            '+' => Tok::Op('+'),
            '-' | '−' => Tok::Op('-'),
            '*' | '×' | '·' | '⋅' => Tok::Op('*'),
            '/' | '÷' => Tok::Op('/'),
            '^' => {
                if next == Some('-') {
                    // handled by parser (unary minus in exponent)
                }
                Tok::Op('^')
            }
            '²' => {
                out.push(Tok::Op('^'));
                Tok::Num("2".into())
            }
            '³' => {
                out.push(Tok::Op('^'));
                Tok::Num("3".into())
            }
            '!' => {
                if next == Some('=') {
                    i += 1;
                    Tok::Rel("!=".into())
                } else {
                    Tok::Op('!')
                }
            }
            '%' => Tok::Op('%'),
            '°' => Tok::Op('°'),
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '[' => Tok::LBrack,
            ']' => Tok::RBrack,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            ',' => Tok::Comma,
            ';' => Tok::Semi,
            '|' => Tok::Pipe,
            '√' => Tok::Sqrt,
            '∛' => Tok::Cbrt,
            '∞' => Tok::Ident("inf".into()),
            ':' => {
                if next == Some('=') {
                    i += 1;
                    Tok::Assign
                } else {
                    return Err("Se esperaba ':='".into());
                }
            }
            '=' => {
                if next == Some('=') {
                    i += 1;
                }
                Tok::Eq
            }
            '<' => {
                if next == Some('=') {
                    i += 1;
                    Tok::Rel("<=".into())
                } else {
                    Tok::Rel("<".into())
                }
            }
            '>' => {
                if next == Some('=') {
                    i += 1;
                    Tok::Rel(">=".into())
                } else {
                    Tok::Rel(">".into())
                }
            }
            '≤' => Tok::Rel("<=".into()),
            '≥' => Tok::Rel(">=".into()),
            '≠' => Tok::Rel("!=".into()),
            '→' => Tok::Assign,
            _ => return Err(format!("Carácter no reconocido: '{c}'")),
        };
        out.push(t);
    }
    Ok(out)
}

fn parse_number(s: &str) -> Result<Expr, String> {
    // Exact decimal -> rational when possible (0.1 = 1/10, like ClassPad standard mode).
    let lower = s.to_ascii_lowercase();
    let (mant, exp) = match lower.split_once('e') {
        Some((m, e)) => (m.to_string(), e.parse::<i32>().map_err(|_| format!("Número inválido: {s}"))?),
        None => (lower.clone(), 0),
    };
    let (ip, fp) = match mant.split_once('.') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (mant.clone(), String::new()),
    };
    let digits = format!("{}{}", ip, fp);
    let digits = digits.trim_start_matches('0');
    let scale = fp.len() as i32 - exp;
    if digits.len() <= 30 && scale.abs() <= 30 {
        let n: i128 = if digits.is_empty() { 0 } else { digits.parse().map_err(|_| format!("Número inválido: {s}"))? };
        let r = if scale >= 0 {
            10i128.checked_pow(scale as u32).and_then(|d| Rat::new(n, d))
        } else {
            10i128.checked_pow((-scale) as u32).and_then(|m| n.checked_mul(m)).map(Rat::int)
        };
        if let Some(r) = r {
            return Ok(Expr::Num(r));
        }
    }
    s.parse::<f64>().map(Expr::Float).map_err(|_| format!("Número inválido: {s}"))
}

/// A parsed top-level statement.
#[derive(Clone, Debug)]
pub enum Stmt {
    Expr(Expr),
    AssignVar(String, Expr),
    DefFunc(String, Vec<String>, Expr),
}

pub struct Parser<'a> {
    toks: Vec<Tok>,
    pos: usize,
    is_func: &'a dyn Fn(&str) -> bool,
    extra_funcs: Vec<String>,
    depth: usize,
}

const MAX_DEPTH: usize = 256;

impl<'a> Parser<'a> {
    pub fn parse_stmt(src: &str, is_func: &'a dyn Fn(&str) -> bool) -> Result<Stmt, String> {
        let toks = lex(src)?;
        if toks.is_empty() {
            return Err("Entrada vacía".into());
        }
        // f(x, y) := body
        if let (Some(Tok::Ident(name)), Some(Tok::LParen)) = (toks.first(), toks.get(1)) {
            let mut params = Vec::new();
            let mut k = 2;
            let mut ok = false;
            loop {
                match toks.get(k) {
                    Some(Tok::Ident(p)) => {
                        params.push(p.clone());
                        k += 1;
                        match toks.get(k) {
                            Some(Tok::Comma) => k += 1,
                            Some(Tok::RParen) => {
                                ok = true;
                                break;
                            }
                            _ => break,
                        }
                    }
                    _ => break,
                }
            }
            if ok && toks.get(k + 1) == Some(&Tok::Assign) {
                let body_toks = toks[k + 2..].to_vec();
                let mut p = Parser { toks: body_toks, pos: 0, is_func, extra_funcs: vec![name.clone()], depth: 0 };
                let body = p.parse_full()?;
                return Ok(Stmt::DefFunc(name.clone(), params, body));
            }
        }
        // x := value
        if let (Some(Tok::Ident(name)), Some(Tok::Assign)) = (toks.first(), toks.get(1)) {
            let mut p = Parser { toks: toks[2..].to_vec(), pos: 0, is_func, extra_funcs: vec![], depth: 0 };
            return Ok(Stmt::AssignVar(name.clone(), p.parse_full()?));
        }
        // value → x   (ClassPad style store)
        if let Some(ai) = toks.iter().position(|t| *t == Tok::Assign) {
            if ai + 2 == toks.len() {
                if let Tok::Ident(name) = &toks[ai + 1] {
                    let mut p = Parser { toks: toks[..ai].to_vec(), pos: 0, is_func, extra_funcs: vec![], depth: 0 };
                    return Ok(Stmt::AssignVar(name.clone(), p.parse_full()?));
                }
            }
        }
        let mut p = Parser { toks, pos: 0, is_func, extra_funcs: vec![], depth: 0 };
        Ok(Stmt::Expr(p.parse_full()?))
    }

    pub fn parse_expr(src: &str, is_func: &'a dyn Fn(&str) -> bool) -> Result<Expr, String> {
        let toks = lex(src)?;
        let mut p = Parser { toks, pos: 0, is_func, extra_funcs: vec![], depth: 0 };
        p.parse_full()
    }

    fn parse_full(&mut self) -> Result<Expr, String> {
        if self.toks.is_empty() {
            return Err("Expresión vacía".into());
        }
        let e = self.parse_relation()?;
        if self.pos < self.toks.len() {
            return Err(format!("Símbolo inesperado: {}", tok_str(&self.toks[self.pos])));
        }
        Ok(e)
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }
    fn expect(&mut self, t: Tok) -> Result<(), String> {
        match self.next() {
            Some(ref x) if *x == t => Ok(()),
            Some(x) => Err(format!("Se esperaba '{}' pero se encontró '{}'", tok_str(&t), tok_str(&x))),
            None => Err(format!("Falta '{}'", tok_str(&t))),
        }
    }

    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err("Expresión demasiado anidada".into());
        }
        Ok(())
    }

    fn parse_relation(&mut self) -> Result<Expr, String> {
        let lhs = self.parse_additive()?;
        match self.peek() {
            Some(Tok::Eq) => {
                self.next();
                let rhs = self.parse_additive()?;
                Ok(Expr::Eq(Box::new(lhs), Box::new(rhs)))
            }
            Some(Tok::Rel(op)) => {
                let op = op.clone();
                self.next();
                let rhs = self.parse_additive()?;
                Ok(Expr::Rel(op, Box::new(lhs), Box::new(rhs)))
            }
            _ => Ok(lhs),
        }
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let mut terms = vec![self.parse_term()?];
        loop {
            match self.peek() {
                Some(Tok::Op('+')) => {
                    self.next();
                    terms.push(self.parse_term()?);
                }
                Some(Tok::Op('-')) => {
                    self.next();
                    let t = self.parse_term()?;
                    terms.push(neg(t));
                }
                _ => break,
            }
        }
        self.depth -= 1;
        Ok(if terms.len() == 1 { terms.pop().unwrap() } else { Expr::Add(terms) })
    }

    fn starts_primary(&self) -> bool {
        matches!(
            self.peek(),
            Some(Tok::Num(_)) | Some(Tok::Ident(_)) | Some(Tok::LParen) | Some(Tok::LBrack) | Some(Tok::LBrace) | Some(Tok::Sqrt) | Some(Tok::Cbrt)
        )
    }

    fn parse_term(&mut self) -> Result<Expr, String> {
        let mut factors = vec![self.parse_unary()?];
        loop {
            match self.peek() {
                Some(Tok::Op('*')) => {
                    self.next();
                    factors.push(self.parse_unary()?);
                }
                Some(Tok::Op('/')) => {
                    self.next();
                    let d = self.parse_unary()?;
                    factors.push(Expr::Pow(Box::new(d), Box::new(Expr::int(-1))));
                }
                Some(Tok::Op('%')) => {
                    // postfix percent: x% = x/100
                    self.next();
                    factors.push(Expr::rat(1, 100));
                }
                _ if self.starts_primary() => {
                    // implicit multiplication: 2x, 3(x+1), x sin x
                    factors.push(self.parse_power()?);
                }
                _ => break,
            }
        }
        Ok(if factors.len() == 1 { factors.pop().unwrap() } else { Expr::Mul(factors) })
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.next();
                self.enter()?;
                let e = self.parse_unary()?;
                self.depth -= 1;
                Ok(neg(e))
            }
            Some(Tok::Op('+')) => {
                self.next();
                self.parse_unary()
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> Result<Expr, String> {
        let base = self.parse_postfix()?;
        if let Some(Tok::Op('^')) = self.peek() {
            self.next();
            self.enter()?;
            let exp = self.parse_unary_power()?;
            self.depth -= 1;
            return Ok(Expr::Pow(Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    // exponent: allows -x and right associativity: 2^-3, 2^3^2 = 2^(3^2)
    fn parse_unary_power(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.next();
                let e = self.parse_unary_power()?;
                Ok(neg(e))
            }
            _ => self.parse_power(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_primary()?;
        loop {
            match self.peek() {
                Some(Tok::Op('!')) => {
                    self.next();
                    e = Expr::call("fact", vec![e]);
                }
                Some(Tok::Op('°')) => {
                    self.next();
                    e = Expr::Mul(vec![e, Expr::sym("pi"), Expr::Pow(Box::new(Expr::int(180)), Box::new(Expr::int(-1)))]);
                }
                Some(Tok::LBrack) if matches!(e, Expr::Sym(_) | Expr::Matrix(_) | Expr::List(_)) => {
                    // indexing A[i, j] or L[k]
                    self.next();
                    let mut idx = vec![self.parse_additive()?];
                    while let Some(Tok::Comma) = self.peek() {
                        self.next();
                        idx.push(self.parse_additive()?);
                    }
                    self.expect(Tok::RBrack)?;
                    let mut args = vec![e];
                    args.extend(idx);
                    e = Expr::call("index", args);
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn is_function(&self, name: &str) -> bool {
        (self.is_func)(name) || self.extra_funcs.iter().any(|f| f == name)
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, String> {
        self.expect(Tok::LParen)?;
        let mut args = Vec::new();
        if let Some(Tok::RParen) = self.peek() {
            self.next();
            return Ok(args);
        }
        loop {
            args.push(self.parse_relation()?);
            match self.next() {
                Some(Tok::Comma) => continue,
                Some(Tok::RParen) => break,
                Some(t) => return Err(format!("Se esperaba ',' o ')' pero se encontró '{}'", tok_str(&t))),
                None => return Err("Falta ')'".into()),
            }
        }
        Ok(args)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let r = self.parse_primary_inner();
        self.depth -= 1;
        r
    }

    fn parse_primary_inner(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Tok::Num(s)) => parse_number(&s),
            Some(Tok::Ident(name)) => {
                let name = canonical_name(&name);
                if self.is_function(&name) {
                    if let Some(Tok::LParen) = self.peek() {
                        let args = self.parse_args()?;
                        return Ok(Expr::Call(name, args));
                    }
                    // function without parentheses: sin x, ln 2x
                    if self.starts_primary() || matches!(self.peek(), Some(Tok::Op('-'))) {
                        let neg_arg = if let Some(Tok::Op('-')) = self.peek() {
                            self.next();
                            true
                        } else {
                            false
                        };
                        let mut fs = vec![self.parse_power()?];
                        // absorb implicit product terms that are not functions: sin 2x
                        while self.starts_primary() && !self.next_is_function() {
                            fs.push(self.parse_power()?);
                        }
                        let mut arg = if fs.len() == 1 { fs.pop().unwrap() } else { Expr::Mul(fs) };
                        if neg_arg {
                            arg = neg(arg);
                        }
                        return Ok(Expr::Call(name, vec![arg]));
                    }
                    return Ok(Expr::Sym(name));
                }
                Ok(Expr::Sym(name))
            }
            Some(Tok::LParen) => {
                let e = self.parse_relation()?;
                if let Some(Tok::Comma) = self.peek() {
                    // tuple (a, b) -> list
                    let mut v = vec![e];
                    while let Some(Tok::Comma) = self.peek() {
                        self.next();
                        v.push(self.parse_relation()?);
                    }
                    self.expect(Tok::RParen)?;
                    return Ok(Expr::List(v));
                }
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Some(Tok::Pipe) => {
                let e = self.parse_additive()?;
                self.expect(Tok::Pipe)?;
                Ok(Expr::call("abs", vec![e]))
            }
            Some(Tok::Sqrt) => {
                let e = self.parse_power()?;
                Ok(Expr::call("sqrt", vec![e]))
            }
            Some(Tok::Cbrt) => {
                let e = self.parse_power()?;
                Ok(Expr::call("cbrt", vec![e]))
            }
            Some(Tok::LBrace) => {
                let mut v = Vec::new();
                if let Some(Tok::RBrace) = self.peek() {
                    self.next();
                    return Ok(Expr::List(v));
                }
                loop {
                    v.push(self.parse_relation()?);
                    match self.next() {
                        Some(Tok::Comma) => continue,
                        Some(Tok::RBrace) => break,
                        _ => return Err("Falta '}' en la lista".into()),
                    }
                }
                Ok(Expr::List(v))
            }
            Some(Tok::LBrack) => self.parse_matrix(),
            Some(t) => Err(format!("Símbolo inesperado: '{}'", tok_str(&t))),
            None => Err("Expresión incompleta".into()),
        }
    }

    fn next_is_function(&self) -> bool {
        match self.peek() {
            Some(Tok::Ident(n)) => self.is_function(&canonical_name(n)),
            _ => false,
        }
    }

    // [[1,2],[3,4]]  |  [1,2;3,4]  |  [1,2,3]
    fn parse_matrix(&mut self) -> Result<Expr, String> {
        let mut rows: Vec<Vec<Expr>> = vec![Vec::new()];
        if let Some(Tok::RBrack) = self.peek() {
            self.next();
            return Ok(Expr::Matrix(vec![]));
        }
        loop {
            let e = self.parse_relation()?;
            rows.last_mut().unwrap().push(e);
            match self.next() {
                Some(Tok::Comma) => {}
                Some(Tok::Semi) => rows.push(Vec::new()),
                Some(Tok::RBrack) => break,
                _ => return Err("Falta ']' en la matriz".into()),
            }
        }
        // [[..],[..]] form: a single row whose elements are all row matrices
        if rows.len() == 1 && rows[0].iter().all(|e| matches!(e, Expr::Matrix(r) if r.len() == 1)) {
            let inner: Vec<Vec<Expr>> = rows[0]
                .drain(..)
                .map(|e| match e {
                    Expr::Matrix(mut r) => r.remove(0),
                    _ => unreachable!(),
                })
                .collect();
            rows = inner;
        }
        let w = rows[0].len();
        if rows.iter().any(|r| r.len() != w) {
            return Err("Las filas de la matriz deben tener la misma longitud".into());
        }
        Ok(Expr::Matrix(rows))
    }
}

/// Maps aliases (including Spanish names) to canonical function names.
pub fn canonical_name(name: &str) -> String {
    let n = match name {
        "sen" => "sin",
        "tg" => "tan",
        "arcsin" | "asen" | "arcsen" => "asin",
        "arccos" => "acos",
        "arctan" | "atg" => "atan",
        "senh" => "sinh",
        "raiz" | "raíz" => "sqrt",
        "log10" => "log",
        "derivada" | "derivar" => "diff",
        "integral" | "integrar" | "int" | "∫" => "integrate",
        "limite" | "límite" | "lim" => "limit",
        "resolver" => "solve",
        "factorizar" => "factor",
        "expandir" => "expand",
        "simplificar" => "simplify",
        "media" | "promedio" => "mean",
        "mediana" => "median",
        "moda" => "mode",
        "varianza" => "var",
        "desviacion" | "desviación" | "std" | "sd" => "stdev",
        "pendiente" => "slope",
        "transpose" | "trans" | "transpuesta" => "trn",
        "inverse" | "inversa" => "inv",
        "ident" | "eye" => "identity",
        "suma" | "sigma" | "Σ" => "sum",
        "producto" => "prod",
        "nCr" | "comb" | "binomial" => "ncr",
        "nPr" | "perm" => "npr",
        "factorial" => "fact",
        "mcd" => "gcd",
        "mcm" => "lcm",
        "esprimo" => "isprime",
        "aprox" | "approximate" | "decimal" => "approx",
        "dim" => "size",
        "inf" | "infinity" | "oo" => "inf",
        "PI" | "Pi" => "pi",
        "taylor" | "series" => "taylor",
        "len" | "length" => "count",
        other => other,
    };
    n.to_string()
}

fn neg(e: Expr) -> Expr {
    match e {
        Expr::Num(r) => Expr::Num(Rat { n: -r.n, d: r.d }),
        Expr::Float(f) => Expr::Float(-f),
        other => Expr::Mul(vec![Expr::int(-1), other]),
    }
}

fn tok_str(t: &Tok) -> String {
    match t {
        Tok::Num(s) | Tok::Ident(s) | Tok::Rel(s) => s.clone(),
        Tok::Op(c) => c.to_string(),
        Tok::LParen => "(".into(),
        Tok::RParen => ")".into(),
        Tok::LBrack => "[".into(),
        Tok::RBrack => "]".into(),
        Tok::LBrace => "{".into(),
        Tok::RBrace => "}".into(),
        Tok::Comma => ",".into(),
        Tok::Semi => ";".into(),
        Tok::Assign => ":=".into(),
        Tok::Eq => "=".into(),
        Tok::Sqrt => "√".into(),
        Tok::Cbrt => "∛".into(),
        Tok::Pipe => "|".into(),
    }
}
