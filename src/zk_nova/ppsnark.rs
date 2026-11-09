//! Spartan preprocessing-SNARK verifier (`nova-snark` `spartan/ppsnark.rs:1388-1655`), with the
//! IPA evaluation engine. See `docs/ZK_NOVA_SPEC.md` section 8.

use super::codec::{PpsnarkProof, RelaxedInstance};
use super::curve::{Affine, Curve, ScalarOf};
use super::error::NovaVerifyError;
use super::ipa;
use super::key::CurveVk;
use super::msm::msm;
use super::polys::{eq_eval, identity_eval, masked_eq_eval, powers, sparse_eval};
use super::sumcheck;
use super::transcript::{commitment_bytes, Transcript};

fn log2_exact(value: usize) -> Option<usize> {
    if value != 0 && value.is_power_of_two() {
        Some(value.trailing_zeros() as usize)
    } else {
        None
    }
}

/// Verifies `proof` for the (derandomized) relaxed instance `u` under `vk`.
pub fn verify<C: Curve>(
    vk: &CurveVk<C>,
    u: &RelaxedInstance<C>,
    proof: &PpsnarkProof<C>,
) -> Result<(), NovaVerifyError> {
    type S<C> = ScalarOf<C>;
    let one = S::<C>::ONE;

    if u.x.len() != 2 {
        return Err(NovaVerifyError::InstanceLength);
    }
    let shape = (
        log2_exact(vk.num_cons),
        log2_exact(vk.num_vars),
        log2_exact(vk.n),
    );
    let (r_out, m, k) = match shape {
        (Some(r_out), Some(m), Some(k)) => (r_out, m, k),
        _ => return Err(NovaVerifyError::InvalidKey("shape is not a power of two")),
    };
    if m < 3 || k < m + 1 || k < r_out || vk.ck_v().len() < vk.n {
        return Err(NovaVerifyError::InvalidKey(
            "shape outside the supported range",
        ));
    }

    let mut transcript = Transcript::<C::Scalar>::new(b"RelaxedR1CSSNARK");
    transcript.absorb_scalar(b"vk", &vk.digest);
    let mut instance = Vec::with_capacity(65 + 65 + 96);
    instance.extend_from_slice(&commitment_bytes(&u.comm_w));
    instance.extend_from_slice(&commitment_bytes(&u.comm_e));
    instance.extend_from_slice(&u.u.to_repr());
    instance.extend_from_slice(&u.x[0].to_repr());
    instance.extend_from_slice(&u.x[1].to_repr());
    transcript.absorb_bytes(b"U", &instance);

    let tau: Vec<S<C>> = (0..r_out).map(|_| transcript.squeeze(b"t")).collect();

    // Outer sum-check.
    let zero = S::<C>::ZERO;
    let (claim_outer, r_outer) =
        sumcheck::verify(&zero, r_out, 3, &proof.sc_outer, &mut transcript)?;
    let az = proof.eval_az_at_r_outer;
    let bz = proof.eval_bz_at_r_outer;
    let cz = proof.eval_cz_at_r_outer;
    let e_outer = proof.eval_e_at_r_outer;
    if eq_eval(&tau, &r_outer) * (az * bz - u.u * cz - e_outer) != claim_outer {
        return Err(NovaVerifyError::InvalidSumcheckProof);
    }
    transcript.absorb_scalars(b"e", &[az, bz, cz, e_outer]);

    let mut r_outer_full: Vec<S<C>> = (0..k - r_out).map(|_| transcript.squeeze(b"p")).collect();
    let factor = r_outer_full.iter().fold(one, |acc, r| acc * (one - *r));
    r_outer_full.extend_from_slice(&r_outer);

    transcript.absorb_commitments(b"e", &[proof.comm_l_row, proof.comm_l_col]);
    let c = transcript.squeeze(b"c");
    let gamma = transcript.squeeze(b"g");
    let r = transcript.squeeze(b"r");
    transcript.absorb_commitments(
        b"l",
        &[
            proof.comm_t_plus_r_inv_row,
            proof.comm_w_plus_r_inv_row,
            proof.comm_t_plus_r_inv_col,
            proof.comm_w_plus_r_inv_col,
        ],
    );
    let rho: Vec<S<C>> = (0..k).map(|_| transcript.squeeze(b"r")).collect();
    let s = transcript.squeeze(b"r");
    let coeffs = powers(&s, 9);

    let c2_pow = c.square();
    let abc_at_outer = az + c * bz + c2_pow * cz;
    let claim0 = coeffs[6] * factor * abc_at_outer + coeffs[7] * factor * e_outer;
    let (claim_inner, r_in) =
        sumcheck::verify(&claim0, k, 3, &proof.sc_inner_batched, &mut transcript)?;

    // Expected final value of the batched inner sum-check.
    let rho_eq = eq_eval(&rho, &r_in);
    let eq_ro = eq_eval(&r_outer_full, &r_in);
    let masked =
        masked_eq_eval(&r_outer_full, m, &r_in).ok_or(NovaVerifyError::InvalidSumcheckProof)?;
    let id = identity_eval(&r_in);

    let t_row = id + gamma * eq_ro + r;
    let w_row = proof.eval_row + gamma * proof.eval_l_row + r;

    let l = k - (m + 1);
    let fc = r_in[..l].iter().fold(one, |acc, x| acc * (one - *x));
    let ru = &r_in[l..];
    let eval_x = sparse_eval(&[u.u, u.x[0], u.x[1]], &ru[1..])
        .ok_or(NovaVerifyError::InvalidSumcheckProof)?;
    let z_col = proof.eval_w + fc * ru[0] * eval_x;
    let t_col = id + gamma * z_col + r;
    let w_col = proof.eval_col + gamma * proof.eval_l_col + r;

    let mem = coeffs[0] * (proof.eval_t_plus_r_inv_row - proof.eval_w_plus_r_inv_row)
        + coeffs[1] * (proof.eval_t_plus_r_inv_col - proof.eval_w_plus_r_inv_col)
        + coeffs[2] * rho_eq * (proof.eval_t_plus_r_inv_row * t_row - proof.eval_ts_row)
        + coeffs[3] * rho_eq * (proof.eval_w_plus_r_inv_row * w_row - one)
        + coeffs[4] * rho_eq * (proof.eval_t_plus_r_inv_col * t_col - proof.eval_ts_col)
        + coeffs[5] * rho_eq * (proof.eval_w_plus_r_inv_col * w_col - one);
    let abc = coeffs[6]
        * proof.eval_l_row
        * proof.eval_l_col
        * (proof.eval_val_a + c * proof.eval_val_b + c2_pow * proof.eval_val_c);
    let ee = coeffs[7] * eq_ro * proof.eval_e;
    let wb = coeffs[8] * masked * proof.eval_w;
    if mem + abc + ee + wb != claim_inner {
        return Err(NovaVerifyError::InvalidSumcheckProof);
    }

    // Batch the fifteen openings and hand them to the IPA.
    let comm_vec: [Affine<C>; 15] = [
        u.comm_w,
        u.comm_e,
        proof.comm_l_row,
        proof.comm_l_col,
        vk.spark.comm_val_a,
        vk.spark.comm_val_b,
        vk.spark.comm_val_c,
        proof.comm_t_plus_r_inv_row,
        vk.spark.comm_row,
        proof.comm_w_plus_r_inv_row,
        vk.spark.comm_ts_row,
        proof.comm_t_plus_r_inv_col,
        vk.spark.comm_col,
        proof.comm_w_plus_r_inv_col,
        vk.spark.comm_ts_col,
    ];
    let eval_vec: [S<C>; 15] = [
        proof.eval_w,
        proof.eval_e,
        proof.eval_l_row,
        proof.eval_l_col,
        proof.eval_val_a,
        proof.eval_val_b,
        proof.eval_val_c,
        proof.eval_t_plus_r_inv_row,
        proof.eval_row,
        proof.eval_w_plus_r_inv_row,
        proof.eval_ts_row,
        proof.eval_t_plus_r_inv_col,
        proof.eval_col,
        proof.eval_w_plus_r_inv_col,
        proof.eval_ts_col,
    ];
    transcript.absorb_scalars(b"e", &eval_vec);
    let c_batch = transcript.squeeze(b"c");
    let weights = powers(&c_batch, 15);
    let comm_batched = msm::<C>(&weights, &comm_vec)
        .map_err(|_| NovaVerifyError::Internal("msm worker failed"))?
        .to_affine();
    let eval_batched = weights
        .iter()
        .zip(&eval_vec)
        .fold(S::<C>::ZERO, |acc, (w, e)| acc + *w * *e);

    ipa::verify::<C>(
        vk.ck_v(),
        &vk.ck_s_generator(),
        &mut transcript,
        &comm_batched,
        &r_in,
        &eval_batched,
        &proof.eval_arg,
    )
}
