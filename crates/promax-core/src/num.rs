//! Exact rationals (i128) and complex floating point numbers.

use std::cmp::Ordering;
use std::fmt;

pub fn gcd_i128(mut a: i128, mut b: i128) -> i128 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// Exact rational number `n/d` with `d > 0` and `gcd(n, d) == 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rat {
    pub n: i128,
    pub d: i128,
}

impl Rat {
    pub const ZERO: Rat = Rat { n: 0, d: 1 };
    pub const ONE: Rat = Rat { n: 1, d: 1 };
    pub const MINUS_ONE: Rat = Rat { n: -1, d: 1 };

    pub fn int(n: i128) -> Rat {
        Rat { n, d: 1 }
    }

    pub fn new(n: i128, d: i128) -> Option<Rat> {
        if d == 0 {
            return None;
        }
        let g = gcd_i128(n, d);
        let g = if g == 0 { 1 } else { g };
        let (mut n, mut d) = (n / g, d / g);
        if d < 0 {
            n = n.checked_neg()?;
            d = d.checked_neg()?;
        }
        Some(Rat { n, d })
    }

    pub fn is_int(&self) -> bool {
        self.d == 1
    }
    pub fn is_zero(&self) -> bool {
        self.n == 0
    }
    pub fn is_one(&self) -> bool {
        self.n == 1 && self.d == 1
    }
    pub fn is_neg(&self) -> bool {
        self.n < 0
    }
    pub fn to_f64(&self) -> f64 {
        self.n as f64 / self.d as f64
    }

    pub fn add(self, o: Rat) -> Option<Rat> {
        let g = gcd_i128(self.d, o.d);
        let l = (self.d / g).checked_mul(o.d)?;
        let a = self.n.checked_mul(l / self.d)?;
        let b = o.n.checked_mul(l / o.d)?;
        Rat::new(a.checked_add(b)?, l)
    }
    pub fn sub(self, o: Rat) -> Option<Rat> {
        self.add(o.neg()?)
    }
    pub fn neg(self) -> Option<Rat> {
        Some(Rat { n: self.n.checked_neg()?, d: self.d })
    }
    pub fn mul(self, o: Rat) -> Option<Rat> {
        let g1 = gcd_i128(self.n, o.d).max(1);
        let g2 = gcd_i128(o.n, self.d).max(1);
        let n = (self.n / g1).checked_mul(o.n / g2)?;
        let d = (self.d / g2).checked_mul(o.d / g1)?;
        Rat::new(n, d)
    }
    pub fn recip(self) -> Option<Rat> {
        Rat::new(self.d, self.n)
    }
    pub fn div(self, o: Rat) -> Option<Rat> {
        self.mul(o.recip()?)
    }
    pub fn powi(self, e: i64) -> Option<Rat> {
        if e < 0 {
            return self.recip()?.powi(e.checked_neg()?);
        }
        let mut base = self;
        let mut acc = Rat::ONE;
        let mut e = e as u64;
        while e > 0 {
            if e & 1 == 1 {
                acc = acc.mul(base)?;
            }
            e >>= 1;
            if e > 0 {
                base = base.mul(base)?;
            }
        }
        Some(acc)
    }
    pub fn floor(self) -> i128 {
        self.n.div_euclid(self.d)
    }

    /// Best rational approximation of `x` with denominator ≤ `max_den`
    /// (continued fractions). Returns `None` if it is not within `tol`.
    pub fn approximate(x: f64, max_den: i128, tol: f64) -> Option<Rat> {
        if !x.is_finite() || x.abs() > 1e15 {
            return None;
        }
        let neg = x < 0.0;
        let mut v = x.abs();
        let (mut h0, mut h1, mut k0, mut k1) = (0i128, 1i128, 1i128, 0i128);
        for _ in 0..40 {
            let a = v.floor();
            let ai = a as i128;
            let h2 = ai.checked_mul(h1)?.checked_add(h0)?;
            let k2 = ai.checked_mul(k1)?.checked_add(k0)?;
            if k2 > max_den {
                break;
            }
            h0 = h1;
            h1 = h2;
            k0 = k1;
            k1 = k2;
            let approx = h1 as f64 / k1 as f64;
            if (approx - x.abs()).abs() <= tol * x.abs().max(1.0) {
                let n = if neg { -h1 } else { h1 };
                return Rat::new(n, k1);
            }
            let frac = v - a;
            if frac < 1e-15 {
                break;
            }
            v = 1.0 / frac;
        }
        if k1 != 0 {
            let approx = h1 as f64 / k1 as f64;
            if (approx - x.abs()).abs() <= tol * x.abs().max(1.0) {
                let n = if neg { -h1 } else { h1 };
                return Rat::new(n, k1);
            }
        }
        None
    }
}

impl PartialOrd for Rat {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Rat {
    fn cmp(&self, o: &Self) -> Ordering {
        match (self.n.checked_mul(o.d), o.n.checked_mul(self.d)) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => self.to_f64().total_cmp(&o.to_f64()),
        }
    }
}

impl fmt::Display for Rat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.d == 1 {
            write!(f, "{}", self.n)
        } else {
            write!(f, "{}/{}", self.n, self.d)
        }
    }
}

/// Complex number in double precision.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct C {
    pub re: f64,
    pub im: f64,
}

impl C {
    pub const I: C = C { re: 0.0, im: 1.0 };
    pub const ZERO: C = C { re: 0.0, im: 0.0 };
    pub const ONE: C = C { re: 1.0, im: 0.0 };
    pub fn new(re: f64, im: f64) -> C {
        C { re, im }
    }
    pub fn real(re: f64) -> C {
        C { re, im: 0.0 }
    }
    pub fn is_real(&self) -> bool {
        self.im.abs() <= 1e-12 * self.re.abs().max(1.0)
    }
    pub fn is_finite(&self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
    pub fn is_nan(&self) -> bool {
        self.re.is_nan() || self.im.is_nan()
    }
    pub fn abs(&self) -> f64 {
        self.re.hypot(self.im)
    }
    pub fn arg(&self) -> f64 {
        self.im.atan2(self.re)
    }
    pub fn conj(&self) -> C {
        C::new(self.re, -self.im)
    }
    pub fn add(self, o: C) -> C {
        C::new(self.re + o.re, self.im + o.im)
    }
    pub fn sub(self, o: C) -> C {
        C::new(self.re - o.re, self.im - o.im)
    }
    pub fn mul(self, o: C) -> C {
        if self.im == 0.0 && o.im == 0.0 {
            return C::real(self.re * o.re);
        }
        C::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
    pub fn scale(self, k: f64) -> C {
        C::new(self.re * k, self.im * k)
    }
    pub fn neg(self) -> C {
        C::new(-self.re, -self.im)
    }
    pub fn div(self, o: C) -> C {
        if self.im == 0.0 && o.im == 0.0 {
            return C::real(self.re / o.re);
        }
        let d = o.re * o.re + o.im * o.im;
        C::new((self.re * o.re + self.im * o.im) / d, (self.im * o.re - self.re * o.im) / d)
    }
    pub fn exp(self) -> C {
        let r = self.re.exp();
        if self.im == 0.0 {
            return C::real(r);
        }
        C::new(r * self.im.cos(), r * self.im.sin())
    }
    pub fn ln(self) -> C {
        if self.im == 0.0 && self.re >= 0.0 {
            return C::real(self.re.ln());
        }
        C::new(self.abs().ln(), self.arg())
    }
    pub fn sqrt(self) -> C {
        if self.im == 0.0 {
            if self.re >= 0.0 {
                return C::real(self.re.sqrt());
            }
            return C::new(0.0, (-self.re).sqrt());
        }
        let r = self.abs();
        let re = ((r + self.re) / 2.0).sqrt();
        let im = ((r - self.re) / 2.0).sqrt().copysign(self.im);
        C::new(re, im)
    }
    pub fn powc(self, e: C) -> C {
        if e.im == 0.0 {
            let n = e.re;
            if self.im == 0.0 {
                if self.re >= 0.0 || n.fract() == 0.0 {
                    return C::real(self.re.powf(n));
                }
            }
            if n.fract() == 0.0 && n.abs() <= 64.0 {
                let mut acc = C::ONE;
                let mut b = if n < 0.0 { C::ONE.div(self) } else { self };
                let mut k = n.abs() as u32;
                while k > 0 {
                    if k & 1 == 1 {
                        acc = acc.mul(b);
                    }
                    b = b.mul(b);
                    k >>= 1;
                }
                return acc;
            }
        }
        if self.re == 0.0 && self.im == 0.0 {
            return if e.re > 0.0 { C::ZERO } else { C::new(f64::INFINITY, 0.0) };
        }
        self.ln().mul(e).exp()
    }
    pub fn sin(self) -> C {
        if self.im == 0.0 {
            return C::real(self.re.sin());
        }
        C::new(self.re.sin() * self.im.cosh(), self.re.cos() * self.im.sinh())
    }
    pub fn cos(self) -> C {
        if self.im == 0.0 {
            return C::real(self.re.cos());
        }
        C::new(self.re.cos() * self.im.cosh(), -self.re.sin() * self.im.sinh())
    }
    pub fn tan(self) -> C {
        if self.im == 0.0 {
            return C::real(self.re.tan());
        }
        self.sin().div(self.cos())
    }
    pub fn sinh(self) -> C {
        if self.im == 0.0 {
            return C::real(self.re.sinh());
        }
        self.exp().sub(self.neg().exp()).scale(0.5)
    }
    pub fn cosh(self) -> C {
        if self.im == 0.0 {
            return C::real(self.re.cosh());
        }
        self.exp().add(self.neg().exp()).scale(0.5)
    }
    pub fn tanh(self) -> C {
        if self.im == 0.0 {
            return C::real(self.re.tanh());
        }
        self.sinh().div(self.cosh())
    }
    // asin z = -i ln(iz + sqrt(1 - z^2))
    pub fn asin(self) -> C {
        if self.im == 0.0 && self.re.abs() <= 1.0 {
            return C::real(self.re.asin());
        }
        let iz = C::I.mul(self);
        let s = C::ONE.sub(self.mul(self)).sqrt();
        C::I.neg().mul(iz.add(s).ln())
    }
    pub fn acos(self) -> C {
        if self.im == 0.0 && self.re.abs() <= 1.0 {
            return C::real(self.re.acos());
        }
        C::real(std::f64::consts::FRAC_PI_2).sub(self.asin())
    }
    // atan z = i/2 ln((i+z)/(i-z))
    pub fn atan(self) -> C {
        if self.im == 0.0 {
            return C::real(self.re.atan());
        }
        let a = C::I.add(self).div(C::I.sub(self)).ln();
        C::new(0.0, 0.5).mul(a)
    }
    pub fn asinh(self) -> C {
        if self.im == 0.0 {
            return C::real(self.re.asinh());
        }
        self.add(self.mul(self).add(C::ONE).sqrt()).ln()
    }
    pub fn acosh(self) -> C {
        if self.im == 0.0 && self.re >= 1.0 {
            return C::real(self.re.acosh());
        }
        self.add(self.add(C::ONE).sqrt().mul(self.sub(C::ONE).sqrt())).ln()
    }
    pub fn atanh(self) -> C {
        if self.im == 0.0 && self.re.abs() < 1.0 {
            return C::real(self.re.atanh());
        }
        C::ONE.add(self).div(C::ONE.sub(self)).ln().scale(0.5)
    }
}

/// Lanczos approximation of the gamma function (real argument).
pub fn gamma(x: f64) -> f64 {
    if x.fract() == 0.0 && x <= 0.0 {
        return f64::NAN;
    }
    if x < 0.5 {
        return std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma(1.0 - x));
    }
    if x.fract() == 0.0 && x <= 171.0 {
        let mut acc = 1.0;
        let mut k = 2.0;
        while k < x {
            acc *= k;
            k += 1.0;
        }
        return acc;
    }
    const G: f64 = 7.0;
    const COEF: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.5203681218851,
        -1259.1392167224028,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507343278686905,
        -0.13857109526572012,
        9.984_369_578_019_572e-6,
        1.5056327351493116e-7,
    ];
    let x = x - 1.0;
    let mut a = COEF[0];
    let t = x + G + 0.5;
    for (i, c) in COEF.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    (2.0 * std::f64::consts::PI).sqrt() * t.powf(x + 0.5) * (-t).exp() * a
}

/// Error function (Abramowitz-Stegun 7.1.26 refined with series for small x).
pub fn erf(x: f64) -> f64 {
    if x.abs() < 2.5 {
        // Maclaurin series, converges quickly here
        let mut sum = x;
        let mut term = x;
        let x2 = x * x;
        let mut n = 0.0;
        loop {
            n += 1.0;
            term *= -x2 / n;
            let add = term / (2.0 * n + 1.0);
            sum += add;
            if add.abs() < 1e-17 * sum.abs() {
                break;
            }
            if n > 200.0 {
                break;
            }
        }
        return sum * 2.0 / std::f64::consts::PI.sqrt();
    }
    // continued fraction for erfc
    let ax = x.abs();
    let mut f = 0.0;
    for k in (1..60).rev() {
        f = (k as f64 / 2.0) / (ax + f);
    }
    let erfc = (-ax * ax).exp() / std::f64::consts::PI.sqrt() / (ax + f);
    (1.0 - erfc).copysign(x)
}

pub fn is_prime_u128(n: u128) -> bool {
    if n < 2 {
        return false;
    }
    for p in [2u128, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        if n % p == 0 {
            return n == p;
        }
    }
    if n < 1 << 64 {
        // Deterministic Miller-Rabin for 64-bit
        let n64 = n as u64;
        let mut d = n64 - 1;
        let mut s = 0;
        while d % 2 == 0 {
            d /= 2;
            s += 1;
        }
        let mulmod = |a: u64, b: u64| ((a as u128 * b as u128) % n64 as u128) as u64;
        let powmod = |mut b: u64, mut e: u64| {
            let mut r = 1u64;
            b %= n64;
            while e > 0 {
                if e & 1 == 1 {
                    r = mulmod(r, b);
                }
                b = mulmod(b, b);
                e >>= 1;
            }
            r
        };
        'outer: for a in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
            let mut x = powmod(a, d);
            if x == 1 || x == n64 - 1 {
                continue;
            }
            for _ in 0..s - 1 {
                x = mulmod(x, x);
                if x == n64 - 1 {
                    continue 'outer;
                }
            }
            return false;
        }
        return true;
    }
    let mut i = 41u128;
    while i * i <= n {
        if n % i == 0 {
            return false;
        }
        i += 2;
        if i > 10_000_000 {
            break;
        }
    }
    true
}

/// Prime factorization by trial division (with limit).
pub fn factorize(mut n: u128) -> Vec<(u128, u32)> {
    let mut out = Vec::new();
    let mut p = 2u128;
    while p * p <= n {
        if n % p == 0 {
            let mut k = 0;
            while n % p == 0 {
                n /= p;
                k += 1;
            }
            out.push((p, k));
        }
        p += if p == 2 { 1 } else { 2 };
        if p > 50_000_000 {
            break;
        }
    }
    if n > 1 {
        out.push((n, 1));
    }
    out
}

/// Formats a float with `sig` significant digits, trimming zeros.
pub fn fmt_f64(x: f64, sig: usize) -> String {
    if x.is_nan() {
        return "indefinido".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "∞".into() } else { "-∞".into() };
    }
    if x == 0.0 {
        return "0".into();
    }
    let ax = x.abs();
    let exp = ax.log10().floor() as i32;
    if !(-5..sig as i32).contains(&exp) {
        let s = format!("{:.*e}", sig.saturating_sub(1), x);
        // trim mantissa zeros: 1.2300e5 -> 1.23E5
        if let Some((m, e)) = s.split_once('e') {
            let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
            return format!("{}E{}", m, e);
        }
        return s;
    }
    let decimals = (sig as i32 - 1 - exp).max(0) as usize;
    let s = format!("{:.*}", decimals, x);
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

pub fn fmt_c(z: C, sig: usize) -> String {
    let re = if z.re.abs() < 1e-14 * z.im.abs() { 0.0 } else { z.re };
    let im = if z.im.abs() < 1e-14 * z.re.abs().max(1e-300) || z.im.abs() < 1e-15 { 0.0 } else { z.im };
    if im == 0.0 {
        return fmt_f64(re, sig);
    }
    let ims = if (im.abs() - 1.0).abs() < 1e-15 { String::new() } else { fmt_f64(im.abs(), sig) };
    if re == 0.0 {
        return format!("{}{}i", if im < 0.0 { "-" } else { "" }, ims);
    }
    format!("{} {} {}i", fmt_f64(re, sig), if im < 0.0 { "-" } else { "+" }, ims)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rat_ops() {
        let a = Rat::new(1, 3).unwrap();
        let b = Rat::new(1, 6).unwrap();
        assert_eq!(a.add(b).unwrap(), Rat::new(1, 2).unwrap());
        assert_eq!(Rat::approximate(0.75, 1000, 1e-12).unwrap(), Rat::new(3, 4).unwrap());
    }
    #[test]
    fn gamma_fact() {
        assert!((gamma(5.0) - 24.0).abs() < 1e-9);
        assert!((gamma(0.5) - std::f64::consts::PI.sqrt()).abs() < 1e-12);
        assert!((erf(1.0) - 0.8427007929497149).abs() < 1e-12);
    }
    #[test]
    fn primes() {
        assert!(is_prime_u128(1_000_000_007));
        assert!(!is_prime_u128(1_000_000_007 * 3));
        assert_eq!(factorize(360), vec![(2, 3), (3, 2), (5, 1)]);
    }
}
