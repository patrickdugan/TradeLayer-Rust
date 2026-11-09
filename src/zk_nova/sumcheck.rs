//! Sum-check verifier (`nova-snark` `spartan/sumcheck.rs:87-125`).

use super::error::NovaVerifyError;
use super::field::{Field, FieldParams};
use super::polys::{decompress_univariate, eval_univariate};
use super::transcript::Transcript;

/// Verifies a sum-check proof of `claim` over `num_rounds` rounds with round polynomials of degree
/// at most `degree_bound`. Each entry of `compressed` is `[c0, c2, c3, ...]` (no linear term).
///
/// Returns the final evaluation claim and the challenges `r_0 .. r_{num_rounds-1}`.
pub fn verify<P: FieldParams>(
    claim: &Field<P>,
    num_rounds: usize,
    degree_bound: usize,
    compressed: &[Vec<Field<P>>],
    transcript: &mut Transcript<P>,
) -> Result<(Field<P>, Vec<Field<P>>), NovaVerifyError> {
    if compressed.len() != num_rounds {
        return Err(NovaVerifyError::InvalidSumcheckProof);
    }
    let mut e = *claim;
    let mut challenges = Vec::with_capacity(num_rounds);
    for stored in compressed {
        let coefficients =
            decompress_univariate(stored, &e).ok_or(NovaVerifyError::InvalidSumcheckProof)?;
        if coefficients.len() - 1 > degree_bound {
            return Err(NovaVerifyError::InvalidSumcheckProof);
        }
        transcript.absorb_scalars(b"p", stored);
        let r = transcript.squeeze(b"c");
        e = eval_univariate(&coefficients, &r);
        challenges.push(r);
    }
    Ok((e, challenges))
}
