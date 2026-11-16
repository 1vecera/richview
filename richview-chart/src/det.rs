//! Math that gives the same bits on every platform. The standard library's `sin`, `cos` and `hypot`
//! call the platform's libm, which differs between the native build and Wasm in the last bit; a
//! layout that iterates on them (force, packing, spirals) can amplify that into a different
//! picture, and its derived record then differs between `check` and the browser (TYP-36). These use
//! only `+ - * /` and `sqrt`, which IEEE 754 fixes exactly.
use std::f64::consts::{PI, TAU};

/// The sine and cosine of `x` radians.
pub fn sin_cos(x: f64) -> (f64, f64) {
    if !x.is_finite() {
        return (f64::NAN, f64::NAN);
    }
    // Reduce to [-pi, pi], then sum the Taylor series, which converges fast there.
    let r = x - TAU * (x / TAU).round();
    let r = if r > PI {
        r - TAU
    } else if r < -PI {
        r + TAU
    } else {
        r
    };
    let r2 = r * r;
    let (mut sin, mut cos) = (r, 1.0);
    let (mut s_term, mut c_term) = (r, 1.0);
    for k in 1..=20 {
        let k2 = f64::from(2 * k);
        c_term *= -r2 / ((k2 - 1.0) * k2);
        s_term *= -r2 / (k2 * (k2 + 1.0));
        cos += c_term;
        sin += s_term;
    }
    (sin, cos)
}

/// The sine of `x` radians.
pub fn sin(x: f64) -> f64 {
    sin_cos(x).0
}

/// The cosine of `x` radians.
pub fn cos(x: f64) -> f64 {
    sin_cos(x).1
}

/// The length of the vector `(x, y)`.
pub fn hypot(x: f64, y: f64) -> f64 {
    (x * x + y * y).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_cos_agree_with_the_standard_library_to_rounding() {
        for i in -400..400 {
            let x = f64::from(i) * 0.37;
            let (s, c) = sin_cos(x);
            assert!(
                (s - x.sin()).abs() < 1e-12 && (c - x.cos()).abs() < 1e-12,
                "{x}: {s} {c}"
            );
        }
        assert_eq!(hypot(3.0, 4.0), 5.0);
    }
}
