//! Inner-product-argument evaluation engine verifier (`nova-snark` `provider/ipa_pc.rs:286-389`),
//! in the single multi-scalar form of `docs/ZK_NOVA_SPEC.md` section 9.

use super::codec::IpaProof;
use super::curve::{Affine, Curve, ScalarOf};
use super::error::NovaVerifyError;
use super::field::Field;
use super::msm::msm;
use super::transcript::{commitment_bytes, Transcript};

/// Largest supported number of IPA rounds (`|L| < 32` in nova).
pub const MAX_ROUNDS: usize = 31;

/// Verifies that `comm` opens to `eval` at `point` (the `k` coordinates of the evaluation point,
/// most significant first) under the generators `ck_v` and the scale generator `g_ipa`.
///
/// The check is
/// `sum r_j^2 L_j + sum r_j^-2 R_j + C + gamma (e - a b) G - a sum_i s_i ck_i == identity`.
pub fn verify<C: Curve>(
    ck_v: &[Affine<C>],
    g_ipa: &Affine<C>,
    transcript: &mut Transcript<C::Scalar>,
    comm: &Affine<C>,
    point: &[ScalarOf<C>],
    eval: &ScalarOf<C>,
    proof: &IpaProof<C>,
) -> Result<(), NovaVerifyError> {
    type S<C> = ScalarOf<C>;
    let k = proof.l_vec.len();
    if proof.r_vec.len() != k || k > MAX_ROUNDS || point.len() != k {
        return Err(NovaVerifyError::InvalidInputLength);
    }
    let n = 1usize << k;
    if ck_v.len() < n {
        return Err(NovaVerifyError::InvalidInputLength);
    }

    transcript.dom_sep(b"IPA");
    let mut instance = Vec::with_capacity(97);
    instance.extend_from_slice(&commitment_bytes(comm));
    instance.extend_from_slice(&eval.to_repr());
    transcript.absorb_bytes(b"U", &instance);
    let gamma = transcript.squeeze(b"r");

    let mut challenges: Vec<S<C>> = Vec::with_capacity(k);
    for (l, r) in proof.l_vec.iter().zip(&proof.r_vec) {
        transcript.absorb_bytes(b"L", &commitment_bytes(l));
        transcript.absorb_bytes(b"R", &commitment_bytes(r));
        challenges.push(transcript.squeeze(b"r"));
    }
    let mut inverses: Vec<S<C>> = Vec::with_capacity(k);
    for challenge in &challenges {
        inverses.push(challenge.invert().ok_or(NovaVerifyError::ZeroChallenge)?);
    }

    // b_hat = prod_j ((1 - x_j) r_j^-1 + x_j r_j)
    let mut b_hat = S::<C>::ONE;
    for ((x, r), r_inv) in point.iter().zip(&challenges).zip(&inverses) {
        b_hat *= (S::<C>::ONE - *x) * *r_inv + *x * *r;
    }

    // Variable-base part.
    let mut small_scalars: Vec<S<C>> = Vec::with_capacity(2 * k + 2);
    let mut small_bases: Vec<Affine<C>> = Vec::with_capacity(2 * k + 2);
    for ((l, r), (c, c_inv)) in proof
        .l_vec
        .iter()
        .zip(&proof.r_vec)
        .zip(challenges.iter().zip(&inverses))
    {
        small_scalars.push(c.square());
        small_bases.push(*l);
        small_scalars.push(c_inv.square());
        small_bases.push(*r);
    }
    small_scalars.push(S::<C>::ONE);
    small_bases.push(*comm);
    small_scalars.push(gamma * (*eval - proof.a_hat * b_hat));
    small_bases.push(*g_ipa);
    let small = msm::<C>(&small_scalars, &small_bases)
        .map_err(|_| NovaVerifyError::Internal("msm worker failed"))?;

    // Fixed-base part: -a_hat * sum_i s_i ck_i with s_i built by the doubling recurrence.
    let mut s: Vec<S<C>> = Vec::with_capacity(n);
    let mut first = -proof.a_hat;
    for r_inv in &inverses {
        first *= *r_inv;
    }
    s.push(first);
    let squares: Vec<S<C>> = challenges.iter().map(Field::square).collect();
    for i in 1..n {
        let p = (usize::BITS - 1 - i.leading_zeros()) as usize;
        let previous = s[i - (1usize << p)];
        s.push(previous * squares[k - 1 - p]);
    }
    let big =
        msm::<C>(&s, &ck_v[..n]).map_err(|_| NovaVerifyError::Internal("msm worker failed"))?;

    if (small + big).is_identity() {
        Ok(())
    } else {
        Err(NovaVerifyError::InvalidPcs)
    }
}
