//! `nova::CompressedSNARK::verify` (`nova-snark` `nova/mod.rs:915-1037`).
//! See `docs/ZK_NOVA_SPEC.md` sections 6 and 7.

use super::codec::{decode_compressed_snark, CompressedSnark, R1csInstance, RelaxedInstance};
use super::curve::{Affine, BaseOf, Curve, Pallas, ScalarOf, Vesta};
use super::error::NovaVerifyError;
use super::field::{Field, FieldParams, Fp, FpParams, Fq, FqParams};
use super::key::VerifierKey;
use super::poseidon::{PoseidonParams, RandomOracle};
use super::ppsnark;
use super::MAX_PROOF_BYTES;

/// Bits of an RO challenge used by the folds.
const NUM_CHALLENGE_BITS: usize = 128;
/// Bits of an RO output used as a state hash.
const NUM_HASH_BITS: usize = 250;

/// `field_switch`: the canonical integer of `x` reduced into the field `B`.
pub fn field_switch<A: FieldParams, B: FieldParams>(x: &Field<A>) -> Field<B> {
    let mut wide = [0u8; 64];
    wide[..32].copy_from_slice(&x.to_repr());
    Field::<B>::from_uniform_bytes_64(&wide)
}

fn absorb_commitment<C: Curve>(ro: &mut RandomOracle<'_, C::Base>, point: &Affine<C>) {
    if point.is_identity() {
        ro.absorb(BaseOf::<C>::ZERO);
        ro.absorb(BaseOf::<C>::ZERO);
        ro.absorb(BaseOf::<C>::ONE);
    } else {
        ro.absorb(point.x());
        ro.absorb(point.y());
        ro.absorb(BaseOf::<C>::ZERO);
    }
}

fn absorb_limbs<C: Curve>(ro: &mut RandomOracle<'_, C::Base>, x: &ScalarOf<C>) {
    for limb in x.to_limbs() {
        ro.absorb(BaseOf::<C>::from_u64(limb));
    }
}

fn absorb_relaxed<C: Curve>(ro: &mut RandomOracle<'_, C::Base>, u: &RelaxedInstance<C>) {
    absorb_commitment::<C>(ro, &u.comm_w);
    absorb_commitment::<C>(ro, &u.comm_e);
    ro.absorb(field_switch::<C::Scalar, C::Base>(&u.u));
    for x in &u.x {
        absorb_limbs::<C>(ro, x);
    }
}

fn absorb_r1cs<C: Curve>(ro: &mut RandomOracle<'_, C::Base>, u: &R1csInstance<C>) {
    absorb_commitment::<C>(ro, &u.comm_w);
    for x in &u.x {
        ro.absorb(field_switch::<C::Scalar, C::Base>(x));
    }
}

fn challenge<C: Curve>(ro: &RandomOracle<'_, C::Base>) -> ScalarOf<C> {
    field_switch::<C::Base, C::Scalar>(&ro.squeeze(NUM_CHALLENGE_BITS))
}

fn combine<C: Curve>(terms: &[(Affine<C>, ScalarOf<C>)]) -> Affine<C> {
    let mut acc = super::curve::Projective::<C>::identity();
    for (point, scalar) in terms {
        acc = acc + point.mul_scalar(scalar);
    }
    acc.to_affine()
}

/// `NIFS::verify`: folds the fresh instance `u2` into `u1`. Only `u2` and `comm_t` are hashed
/// (`u1` is bound through `u2.X[0]`).
fn nifs_r1cs<C: Curve>(
    params: &PoseidonParams<C::Base>,
    pp_digest: BaseOf<C>,
    u1: &RelaxedInstance<C>,
    u2: &R1csInstance<C>,
    comm_t: &Affine<C>,
) -> RelaxedInstance<C> {
    let mut ro = RandomOracle::new(params);
    ro.absorb(pp_digest);
    absorb_r1cs::<C>(&mut ro, u2);
    absorb_commitment::<C>(&mut ro, comm_t);
    let r = challenge::<C>(&ro);
    RelaxedInstance {
        comm_w: combine(&[(u1.comm_w, ScalarOf::<C>::ONE), (u2.comm_w, r)]),
        comm_e: combine(&[(u1.comm_e, ScalarOf::<C>::ONE), (*comm_t, r)]),
        x: u1.x.iter().zip(&u2.x).map(|(a, b)| *a + r * *b).collect(),
        u: u1.u + r,
    }
}

/// `NIFSRelaxed::verify`: folds the relaxed instance `u2` into `u1`.
fn nifs_relaxed<C: Curve>(
    params: &PoseidonParams<C::Base>,
    pp_digest: BaseOf<C>,
    u1: &RelaxedInstance<C>,
    u2: &RelaxedInstance<C>,
    comm_t: &Affine<C>,
) -> RelaxedInstance<C> {
    let mut ro = RandomOracle::new(params);
    ro.absorb(pp_digest);
    absorb_relaxed::<C>(&mut ro, u1);
    absorb_relaxed::<C>(&mut ro, u2);
    absorb_commitment::<C>(&mut ro, comm_t);
    let r = challenge::<C>(&ro);
    let r_squared = r.square();
    RelaxedInstance {
        comm_w: combine(&[(u1.comm_w, ScalarOf::<C>::ONE), (u2.comm_w, r)]),
        comm_e: combine(&[
            (u1.comm_e, ScalarOf::<C>::ONE),
            (*comm_t, r),
            (u2.comm_e, r_squared),
        ]),
        x: u1.x.iter().zip(&u2.x).map(|(a, b)| *a + r * *b).collect(),
        u: u1.u + r * u2.u,
    }
}

fn derandomize<C: Curve>(
    folded: &RelaxedInstance<C>,
    h: &Affine<C>,
    witness_blind: &ScalarOf<C>,
    error_blind: &ScalarOf<C>,
) -> RelaxedInstance<C> {
    RelaxedInstance {
        comm_w: (folded.comm_w.to_projective() - h.mul_scalar(witness_blind)).to_affine(),
        comm_e: (folded.comm_e.to_projective() - h.mul_scalar(error_blind)).to_affine(),
        x: folded.x.clone(),
        u: folded.u,
    }
}

/// Cheap structural checks that need no expanded key: step count, arity, instance widths.
pub fn precheck(
    f_arity: usize,
    num_steps: u64,
    z0: &[Fq],
    snark: &CompressedSnark,
) -> Result<(), NovaVerifyError> {
    if num_steps == 0 {
        return Err(NovaVerifyError::ZeroSteps);
    }
    if z0.len() != f_arity || snark.zn.len() != f_arity {
        return Err(NovaVerifyError::ArityMismatch);
    }
    let two = |len: usize| len == 2;
    if !(two(snark.l_u_secondary.x.len())
        && two(snark.r_u_primary.x.len())
        && two(snark.r_u_secondary.x.len())
        && two(snark.l_ur_primary.x.len())
        && two(snark.l_ur_secondary.x.len()))
    {
        return Err(NovaVerifyError::InstanceLength);
    }
    Ok(())
}

/// Decodes a proof, enforcing [`MAX_PROOF_BYTES`].
pub fn decode(proof: &[u8]) -> Result<CompressedSnark, NovaVerifyError> {
    if proof.len() > MAX_PROOF_BYTES {
        return Err(NovaVerifyError::ProofTooLarge {
            len: proof.len(),
            max: MAX_PROOF_BYTES,
        });
    }
    Ok(decode_compressed_snark(proof)?)
}

/// Verifies a `CompressedSNARK` and returns its final state `zn`.
pub fn verify(
    vk: &VerifierKey,
    num_steps: u64,
    z0: &[Fq],
    proof: &[u8],
) -> Result<Vec<Fq>, NovaVerifyError> {
    let snark = decode(proof)?;
    verify_decoded(vk, num_steps, z0, snark)
}

/// Verifies an already decoded proof.
pub fn verify_decoded(
    vk: &VerifierKey,
    num_steps: u64,
    z0: &[Fq],
    snark: CompressedSnark,
) -> Result<Vec<Fq>, NovaVerifyError> {
    precheck(vk.f_arity, num_steps, z0, &snark)?;

    // Hash binding of the running instances.
    let mut h1 = RandomOracle::new(&vk.ro_secondary);
    h1.absorb(vk.pp_digest);
    h1.absorb(Fq::from_u64(num_steps));
    for z in z0 {
        h1.absorb(*z);
    }
    for z in &snark.zn {
        h1.absorb(*z);
    }
    absorb_relaxed::<Vesta>(&mut h1, &snark.r_u_secondary);
    h1.absorb(snark.ri_primary);
    let hash_primary = h1.squeeze(NUM_HASH_BITS);

    let pp_digest_fp = field_switch::<FqParams, FpParams>(&vk.pp_digest);
    let mut h2 = RandomOracle::new(&vk.ro_primary);
    h2.absorb(pp_digest_fp);
    h2.absorb(Fp::from_u64(num_steps));
    h2.absorb(Fp::ZERO);
    h2.absorb(Fp::ZERO);
    absorb_relaxed::<Pallas>(&mut h2, &snark.r_u_primary);
    h2.absorb(snark.ri_secondary);
    let hash_secondary = h2.squeeze(NUM_HASH_BITS);

    let x0 = field_switch::<FpParams, FqParams>(&snark.l_u_secondary.x[0]);
    if hash_primary != x0 || hash_secondary != snark.l_u_secondary.x[1] {
        return Err(NovaVerifyError::HashBindingMismatch);
    }

    // Fold the secondary side, then the primary side.
    let uf = nifs_r1cs::<Vesta>(
        &vk.ro_secondary,
        vk.pp_digest,
        &snark.r_u_secondary,
        &snark.l_u_secondary,
        &snark.nifs_uf_secondary,
    );
    let un_secondary = nifs_relaxed::<Vesta>(
        &vk.ro_secondary,
        vk.pp_digest,
        &uf,
        &snark.l_ur_secondary,
        &snark.nifs_un_secondary,
    );
    let un_primary = nifs_relaxed::<Pallas>(
        &vk.ro_primary,
        pp_digest_fp,
        &snark.r_u_primary,
        &snark.l_ur_primary,
        &snark.nifs_un_primary,
    );

    let u_primary = derandomize::<Pallas>(
        &un_primary,
        &vk.dk_primary,
        &snark.wit_blind_r_wn_primary,
        &snark.err_blind_r_wn_primary,
    );
    let u_secondary = derandomize::<Vesta>(
        &un_secondary,
        &vk.dk_secondary,
        &snark.wit_blind_r_wn_secondary,
        &snark.err_blind_r_wn_secondary,
    );

    ppsnark::verify::<Pallas>(&vk.primary, &u_primary, &snark.snark_primary)?;
    ppsnark::verify::<Vesta>(&vk.secondary, &u_secondary, &snark.snark_secondary)?;
    Ok(snark.zn)
}
