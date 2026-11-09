//! Multilinear and univariate polynomial helpers used by the Spartan verifier
//! (`nova-snark` `spartan/polys/*`).
//!
//! Point coordinates are indexed from the most significant variable: `a[0]` pairs with the top
//! bit of a table index.

use super::field::{Field, FieldParams};

/// `[1, s, s^2, ..., s^(n-1)]`.
pub fn powers<P: FieldParams>(s: &Field<P>, n: usize) -> Vec<Field<P>> {
    let mut out = Vec::with_capacity(n);
    let mut current = Field::<P>::ONE;
    for _ in 0..n {
        out.push(current);
        current *= *s;
    }
    out
}

/// `eq(a, b) = prod_i (a_i b_i + (1 - a_i)(1 - b_i))` over the shorter length.
pub fn eq_eval<P: FieldParams>(a: &[Field<P>], b: &[Field<P>]) -> Field<P> {
    let mut acc = Field::<P>::ONE;
    for (x, y) in a.iter().zip(b) {
        acc *= eq_term(x, y);
    }
    acc
}

fn eq_term<P: FieldParams>(x: &Field<P>, y: &Field<P>) -> Field<P> {
    *x * *y + (Field::<P>::ONE - *x) * (Field::<P>::ONE - *y)
}

/// The table of `eq(a, bits(i))` for `i` in `0..2^len(a)`, index MSB first.
///
/// The caller bounds `a.len()`; the table has `2^len` entries.
pub fn eq_table<P: FieldParams>(a: &[Field<P>]) -> Vec<Field<P>> {
    let mut table = vec![Field::<P>::ONE];
    for coordinate in a {
        let one_minus = Field::<P>::ONE - *coordinate;
        let mut next = Vec::with_capacity(table.len() * 2);
        for entry in &table {
            next.push(*entry * one_minus);
            next.push(*entry * *coordinate);
        }
        table = next;
    }
    table
}

/// `Identity_k(x) = sum_i 2^(k-1-i) x_i`: the integer whose bits are `x`.
pub fn identity_eval<P: FieldParams>(x: &[Field<P>]) -> Field<P> {
    let mut acc = Field::<P>::ZERO;
    for coordinate in x {
        acc = acc.double() + *coordinate;
    }
    acc
}

/// `MaskedEq(a, m)(x)`: the equality polynomial whose first `2^m` table entries are zero
/// (`masked_eq.rs:34-52`).
///
/// With `s = len - m`: `(prod_{j<s} eqterm_j - prod_{j<s} (1-a_j)(1-x_j)) * prod_{j>=s} eqterm_j`.
/// Returns `None` when `m > len` (nova underflows there).
pub fn masked_eq_eval<P: FieldParams>(
    a: &[Field<P>],
    m: usize,
    x: &[Field<P>],
) -> Option<Field<P>> {
    let len = a.len().min(x.len());
    if m > len {
        return None;
    }
    let split = len - m;
    let mut full_low = Field::<P>::ONE;
    let mut zero_low = Field::<P>::ONE;
    for j in 0..split {
        full_low *= eq_term(&a[j], &x[j]);
        zero_low *= (Field::<P>::ONE - a[j]) * (Field::<P>::ONE - x[j]);
    }
    let mut high = Field::<P>::ONE;
    for j in split..len {
        high *= eq_term(&a[j], &x[j]);
    }
    Some((full_low - zero_low) * high)
}

/// Evaluation of the multilinear extension of the vector `[z0, z1, z2, 0, ...]` of length
/// `2^len(y)` at `y` (`multilinear.rs:195-226`). Returns `None` when `len(y) < 3`.
pub fn sparse_eval<P: FieldParams>(z: &[Field<P>; 3], y: &[Field<P>]) -> Option<Field<P>> {
    let m = y.len();
    if m < 3 {
        return None;
    }
    let chi = eq_table(&y[m - 3..]);
    let mut prefix = Field::<P>::ONE;
    for coordinate in &y[..m - 3] {
        prefix *= Field::<P>::ONE - *coordinate;
    }
    Some(prefix * (z[0] * chi[0] + z[1] * chi[1] + z[2] * chi[2]))
}

/// The multilinear extension of the power polynomial at `x`:
/// `prod_i (1 + (t^(2^i) - 1) x_i)` (`power.rs`).
pub fn power_eval<P: FieldParams>(t: &Field<P>, x: &[Field<P>]) -> Field<P> {
    let mut t_pow = *t;
    let mut acc = Field::<P>::ONE;
    for coordinate in x {
        acc *= Field::<P>::ONE + (t_pow - Field::<P>::ONE) * *coordinate;
        t_pow = t_pow.square();
    }
    acc
}

/// Recovers the full coefficient list `[c0, c1, c2, ...]` from the stored list
/// `[c0, c2, c3, ...]` using `p(0) + p(1) = hint`. `None` when the stored list is empty.
pub fn decompress_univariate<P: FieldParams>(
    stored: &[Field<P>],
    hint: &Field<P>,
) -> Option<Vec<Field<P>>> {
    let first = *stored.first()?;
    let mut linear = *hint - first - first;
    for coefficient in &stored[1..] {
        linear -= *coefficient;
    }
    let mut coefficients = Vec::with_capacity(stored.len() + 1);
    coefficients.push(first);
    coefficients.push(linear);
    coefficients.extend_from_slice(&stored[1..]);
    Some(coefficients)
}

/// Horner evaluation of `coefficients` (lowest degree first) at `r`.
pub fn eval_univariate<P: FieldParams>(coefficients: &[Field<P>], r: &Field<P>) -> Field<P> {
    let mut acc = Field::<P>::ZERO;
    for coefficient in coefficients.iter().rev() {
        acc = acc * *r + *coefficient;
    }
    acc
}
