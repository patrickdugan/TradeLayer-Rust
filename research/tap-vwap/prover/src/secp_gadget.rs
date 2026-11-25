//! Native-field R1CS BIP-340 Schnorr signature verifier over `secp256k1::Fp`
//! (`nova_snark::provider::Secq256k1Engine::Scalar`).
//!
//! Enforces every normative BIP-340 verification rule without field wrap:
//! 1. `pk_x < p`, `lift_x(pk_x)` on `y^2 = x^3 + 7` with canonical even `pk_y < p`.
//! 2. `r < p` and `s < n` over exact 256-bit integers `(hi_128, lo_128)`.
//! 3. `H = SHA256_tagged("BIP0340/challenge", r || pk_x || msg)` and exact
//!    integer reduction `e + q * n == H` (`q in {0, 1}`, `0 <= e < n`) via carry-checked
//!    128-bit limb addition (zero mod-`p` wrap even when `H >= p`).
//! 4. `R = (r, ry)` on `y^2 = x^3 + 7` with canonical even `ry < p` (`R != infinity`).
//! 5. Shifted affine Weierstrass verification `T_1 + s * G == (T_2 + e * P) + (T_1 - T_2) + R`
//!    eliminating point-at-infinity and incomplete-addition edge cases.

use ff::{Field, PrimeField};
use group::{Curve, Group};
use halo2curves::{
    secp256k1::{Fp, Secp256k1, Secp256k1Affine},
    CurveExt,
};
use nova_snark::frontend::{
    gadgets::boolean::Boolean, num::AllocatedNum, AllocatedBit, ConstraintSystem,
    LinearCombination, SynthesisError,
};

use crate::manifest::TAG_BIP0340_CHALLENGE;
use crate::merkle::{enforce_u256_lt_circuit, u128_to_scalar};
use crate::sha256_gadget::{alloc_bytes_be, booleans_be_to_bytes, tagged_sha256_circuit};

/// `secp256k1` base field modulus `p` upper 128 bits (`0xffffffffffffffffffffffffffffffff`).
pub const SECP_P_HI: u128 = 0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff;
/// `secp256k1` base field modulus `p` lower 128 bits (`0xfffffffffffffffffffffffefffffc2f`).
pub const SECP_P_LO: u128 = 0xffff_ffff_ffff_ffff_ffff_fffe_ffff_fc2f;

/// `secp256k1` scalar group order `n` upper 128 bits (`0xfffffffffffffffffffffffffffffffe`).
pub const SECP_N_HI: u128 = 0xffff_ffff_ffff_ffff_ffff_ffff_ffff_fffe;
/// `secp256k1` scalar group order `n` lower 128 bits (`0xbaaedce6af48a03bbfd25e8cd0364141`).
pub const SECP_N_LO: u128 = 0xbaae_dce6_af48_a03b_bfd2_5e8c_d036_4141;

/// Precomputed constant curve tables for BIP-340 R1CS verification.
#[derive(Clone, Debug)]
pub struct Bip340CircuitTables {
    /// `[2^i * G]` for `i = 0..256`.
    pub g_pows: Vec<(Fp, Fp)>,
    /// Deterministic nothing-up-my-sleeve offset `T_1` for `s * G`.
    pub t1: (Fp, Fp),
    /// Deterministic nothing-up-my-sleeve offset `T_2` for `e * P`.
    pub t2: (Fp, Fp),
    /// Precomputed `T_1 - T_2`.
    pub t1_minus_t2: (Fp, Fp),
}

impl Default for Bip340CircuitTables {
    fn default() -> Self {
        Self::new()
    }
}

impl Bip340CircuitTables {
    /// Generate deterministic curve tables using `Secp256k1::hash_to_curve`.
    pub fn new() -> Self {
        let mut g_pows = Vec::with_capacity(256);
        let mut cur = Secp256k1::generator();
        for _ in 0..256 {
            let aff: Secp256k1Affine = cur.to_affine();
            g_pows.push((aff.x, aff.y));
            cur = cur.double();
        }
        let h2c = Secp256k1::hash_to_curve("TradeLayer/TAP-VWAP/BIP340Offset");
        let t1_proj = h2c(b"T1_fixed_base_offset_v1");
        let t2_proj = h2c(b"T2_var_base_offset_v1");
        let diff_proj = t1_proj - t2_proj;

        let t1_aff = t1_proj.to_affine();
        let t2_aff = t2_proj.to_affine();
        let diff_aff = diff_proj.to_affine();
        Self {
            g_pows,
            t1: (t1_aff.x, t1_aff.y),
            t2: (t2_aff.x, t2_aff.y),
            t1_minus_t2: (diff_aff.x, diff_aff.y),
        }
    }
}

/// Convert `Fp` canonical bytes (little-endian in `halo2curves`) to big-endian `[u8; 32]`.
pub fn fp_to_be_bytes(x: &Fp) -> [u8; 32] {
    let repr = x.to_repr();
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[31 - i] = repr.as_ref()[i];
    }
    out
}

/// Convert big-endian `[u8; 32]` to `Option<Fp>`.
pub fn fp_from_be_bytes(be: &[u8; 32]) -> Option<Fp> {
    let mut repr = <Fp as PrimeField>::Repr::default();
    for i in 0..32 {
        repr.as_mut()[i] = be[31 - i];
    }
    Option::from(Fp::from_repr(repr))
}

/// Compute `lift_x(x)` with even `y` natively on `secp256k1` (`y^2 = x^3 + 7`).
pub fn lift_x_even_native(x: Fp) -> Option<Fp> {
    let rhs = x * x * x + Fp::from(7u64);
    let y: Fp = Option::from(rhs.sqrt())?;
    let is_odd = (y.to_repr().as_ref()[0] & 1) == 1;
    Some(if is_odd { -y } else { y })
}

/// Allocate a constant `AllocatedNum<Fp>` pinned by a single R1CS constraint.
fn alloc_const<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    name: &'static str,
    val: Fp,
) -> Result<AllocatedNum<Fp>, SynthesisError> {
    let n = AllocatedNum::alloc(cs.namespace(|| name), || Ok(val))?;
    cs.enforce(
        || format!("pin_{name}"),
        |lc| lc + n.get_variable(),
        |lc| lc + CS::one(),
        |lc| lc + (val, CS::one()),
    );
    Ok(n)
}

fn pack_128_be<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    slice_be: &[Boolean],
) -> Result<AllocatedNum<Fp>, SynthesisError> {
    let mut lc = LinearCombination::<Fp>::zero();
    let mut coeff = Fp::ONE;
    let mut val_acc: Option<u128> = Some(0);
    for (i, b) in slice_be.iter().rev().enumerate() {
        lc = lc + &b.lc(CS::one(), coeff);
        coeff = coeff.double();
        val_acc = match (val_acc, b.get_value()) {
            (Some(acc), Some(bit)) => Some(acc | ((bit as u128) << i)),
            _ => None,
        };
    }
    let num = AllocatedNum::alloc(cs.namespace(|| "limb"), || {
        let v = val_acc.ok_or(SynthesisError::AssignmentMissing)?;
        Ok(u128_to_scalar(v))
    })?;
    cs.enforce(
        || "constrain_limb_bits",
        |_| lc,
        |lc1| lc1 + CS::one(),
        |lc_n| lc_n + num.get_variable(),
    );
    Ok(num)
}

/// Pack 256 big-endian `Boolean` bits into `(hi_128, lo_128, full_fp)`.
pub fn pack_256_be_to_limbs_and_fp<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    bits_be: &[Boolean],
) -> Result<(AllocatedNum<Fp>, AllocatedNum<Fp>, AllocatedNum<Fp>), SynthesisError> {
    assert_eq!(bits_be.len(), 256);

    let hi = pack_128_be(cs.namespace(|| "hi_128"), &bits_be[0..128])?;
    let lo = pack_128_be(cs.namespace(|| "lo_128"), &bits_be[128..256])?;

    let two_128 = u128_to_scalar::<Fp>(1u128 << 64) * u128_to_scalar::<Fp>(1u128 << 64);
    let full = AllocatedNum::alloc(cs.namespace(|| "full_fp"), || {
        let h = hi.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let l = lo.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(h * two_128 + l)
    })?;
    cs.enforce(
        || "full == hi * 2^128 + lo",
        |lc| lc + (two_128, hi.get_variable()) + lo.get_variable(),
        |lc| lc + CS::one(),
        |lc| lc + full.get_variable(),
    );
    Ok((hi, lo, full))
}

/// Allocate and constrain an even-`y` coordinate `y < p` satisfying `y^2 == x^3 + 7 (mod p)`.
fn alloc_and_constrain_even_curve_y<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    x: &AllocatedNum<Fp>,
    p_hi: &AllocatedNum<Fp>,
    p_lo: &AllocatedNum<Fp>,
) -> Result<AllocatedNum<Fp>, SynthesisError> {
    // Witness canonical even y from x (if x^3+7 is non-residue, fall back to Fp::ZERO when testing
    // invalid witnesses so synthesis runs and R1CS constraint `y^2 == x^3 + 7` catches it!).
    let y_bytes_be_opt: Option<[u8; 32]> = x.get_value().map(|xv| {
        let yv = lift_x_even_native(xv).unwrap_or(Fp::ZERO);
        fp_to_be_bytes(&yv)
    });

    let y_bits_be = alloc_bytes_be(
        cs.namespace(|| "y_bits_be"),
        y_bytes_be_opt.as_ref().map(|b| &b[..]),
        32,
    )?;

    // Enforce evenness: LSB (`y_bits_be[255]`) == 0!
    cs.enforce(
        || "enforce_even_y",
        |lc| lc + &y_bits_be[255].lc(CS::one(), Fp::ONE),
        |lc| lc + CS::one(),
        |lc| lc,
    );

    let (y_hi, y_lo, y) = pack_256_be_to_limbs_and_fp(cs.namespace(|| "pack_y"), &y_bits_be)?;
    enforce_u256_lt_circuit(cs.namespace(|| "y_lt_p"), &y_hi, &y_lo, p_hi, p_lo)?;

    // Enforce y^2 == x^3 + 7 in Fp.
    let x2 = x.square(cs.namespace(|| "x2"))?;
    let x3 = x2.mul(cs.namespace(|| "x3"), x)?;
    cs.enforce(
        || "on_secp256k1_curve",
        |lc| lc + y.get_variable(),
        |lc| lc + y.get_variable(),
        |lc| lc + x3.get_variable() + (Fp::from(7u64), CS::one()),
    );
    Ok(y)
}

/// Incomplete affine Weierstrass point addition `(x1, y1) + (x2, y2)` (`x1 != x2`).
fn point_add_incomplete<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    x1: &AllocatedNum<Fp>,
    y1: &AllocatedNum<Fp>,
    x2: &AllocatedNum<Fp>,
    y2: &AllocatedNum<Fp>,
) -> Result<(AllocatedNum<Fp>, AllocatedNum<Fp>), SynthesisError> {
    let lambda = AllocatedNum::alloc(cs.namespace(|| "lambda"), || {
        let xv1 = x1.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let yv1 = y1.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let xv2 = x2.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let yv2 = y2.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let dx_inv = Option::<Fp>::from((xv2 - xv1).invert()).unwrap_or(Fp::ZERO);
        Ok((yv2 - yv1) * dx_inv)
    })?;
    // Enforce lambda * (x2 - x1) == y2 - y1, AND prove x2 - x1 != 0 via inverse witness!
    let dx_inv = AllocatedNum::alloc(cs.namespace(|| "dx_inv"), || {
        let xv1 = x1.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let xv2 = x2.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(Option::<Fp>::from((xv2 - xv1).invert()).unwrap_or(Fp::ZERO))
    })?;
    cs.enforce(
        || "dx_nonzero",
        |lc| lc + x2.get_variable() - x1.get_variable(),
        |lc| lc + dx_inv.get_variable(),
        |lc| lc + CS::one(),
    );
    cs.enforce(
        || "slope_add",
        |lc| lc + lambda.get_variable(),
        |lc| lc + x2.get_variable() - x1.get_variable(),
        |lc| lc + y2.get_variable() - y1.get_variable(),
    );

    let x3 = AllocatedNum::alloc(cs.namespace(|| "x3"), || {
        let l = lambda.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let xv1 = x1.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let xv2 = x2.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(l * l - xv1 - xv2)
    })?;
    cs.enforce(
        || "x3 == lambda^2 - x1 - x2",
        |lc| lc + lambda.get_variable(),
        |lc| lc + lambda.get_variable(),
        |lc| lc + x3.get_variable() + x1.get_variable() + x2.get_variable(),
    );

    let y3 = AllocatedNum::alloc(cs.namespace(|| "y3"), || {
        let l = lambda.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let xv1 = x1.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let yv1 = y1.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let xv3 = x3.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(l * (xv1 - xv3) - yv1)
    })?;
    cs.enforce(
        || "y3 == lambda*(x1 - x3) - y1",
        |lc| lc + lambda.get_variable(),
        |lc| lc + x1.get_variable() - x3.get_variable(),
        |lc| lc + y3.get_variable() + y1.get_variable(),
    );
    Ok((x3, y3))
}

/// Affine point doubling `2 * (x, y)` on `y^2 = x^3 + 7` (`a = 0`, `y != 0`).
fn point_double<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    x: &AllocatedNum<Fp>,
    y: &AllocatedNum<Fp>,
) -> Result<(AllocatedNum<Fp>, AllocatedNum<Fp>), SynthesisError> {
    let x_sq = x.square(cs.namespace(|| "x_sq"))?;
    let y_inv = AllocatedNum::alloc(cs.namespace(|| "y_inv"), || {
        let yv = y.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(Option::<Fp>::from(yv.invert()).unwrap_or(Fp::ZERO))
    })?;
    cs.enforce(
        || "y_nonzero",
        |lc| lc + y.get_variable(),
        |lc| lc + y_inv.get_variable(),
        |lc| lc + CS::one(),
    );

    let lambda = AllocatedNum::alloc(cs.namespace(|| "lambda_dbl"), || {
        let xs = x_sq.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let yv = y.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let two_y_inv = Option::<Fp>::from((yv + yv).invert()).unwrap_or(Fp::ZERO);
        Ok((xs + xs + xs) * two_y_inv)
    })?;
    cs.enforce(
        || "2*y*lambda == 3*x^2",
        |lc| lc + lambda.get_variable(),
        |lc| lc + y.get_variable() + y.get_variable(),
        |lc| lc + x_sq.get_variable() + x_sq.get_variable() + x_sq.get_variable(),
    );

    let x3 = AllocatedNum::alloc(cs.namespace(|| "x3_dbl"), || {
        let l = lambda.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let xv = x.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(l * l - xv - xv)
    })?;
    cs.enforce(
        || "x3 == lambda^2 - 2*x",
        |lc| lc + lambda.get_variable(),
        |lc| lc + lambda.get_variable(),
        |lc| lc + x3.get_variable() + x.get_variable() + x.get_variable(),
    );

    let y3 = AllocatedNum::alloc(cs.namespace(|| "y3_dbl"), || {
        let l = lambda.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let xv = x.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let yv = y.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let xv3 = x3.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(l * (xv - xv3) - yv)
    })?;
    cs.enforce(
        || "y3 == lambda*(x - x3) - y",
        |lc| lc + lambda.get_variable(),
        |lc| lc + x.get_variable() - x3.get_variable(),
        |lc| lc + y3.get_variable() + y.get_variable(),
    );
    Ok((x3, y3))
}

fn select_coord<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    cond: &Boolean,
    t: &AllocatedNum<Fp>,
    f: &AllocatedNum<Fp>,
) -> Result<AllocatedNum<Fp>, SynthesisError> {
    let out = AllocatedNum::alloc(cs.namespace(|| "sel"), || {
        let c = cond.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let tv = t.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let fv = f.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(if c { tv } else { fv })
    })?;
    cs.enforce(
        || "cond * (t - f) == out - f",
        |lc| lc + &cond.lc(CS::one(), Fp::ONE),
        |lc| lc + t.get_variable() - f.get_variable(),
        |lc| lc + out.get_variable() - f.get_variable(),
    );
    Ok(out)
}

/// Conditional selection between two affine points: returns `p_true` if `cond == 1`, else `p_false`.
fn select_point<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    cond: &Boolean,
    p_true: &(AllocatedNum<Fp>, AllocatedNum<Fp>),
    p_false: &(AllocatedNum<Fp>, AllocatedNum<Fp>),
) -> Result<(AllocatedNum<Fp>, AllocatedNum<Fp>), SynthesisError> {
    let rx = select_coord(cs.namespace(|| "sel_x"), cond, &p_true.0, &p_false.0)?;
    let ry = select_coord(cs.namespace(|| "sel_y"), cond, &p_true.1, &p_false.1)?;
    Ok((rx, ry))
}

/// Verify a single BIP-340 Schnorr signature in R1CS over `Fp`:
/// - `pk_bits_be`: 256 big-endian `Boolean` bits of the x-only public key.
/// - `msg_bits_be`: 256 big-endian `Boolean` bits of the 32-byte message digest.
/// - `sig_bytes`: witness 64-byte signature `(r || s)`.
pub fn verify_bip340_circuit<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    tables: &Bip340CircuitTables,
    pk_bits_be: &[Boolean],
    msg_bits_be: &[Boolean],
    sig_bytes: Option<&[u8; 64]>,
) -> Result<(), SynthesisError> {
    assert_eq!(pk_bits_be.len(), 256);
    assert_eq!(msg_bits_be.len(), 256);

    let p_hi = alloc_const(cs.namespace(|| "p_hi"), "p_hi", u128_to_scalar(SECP_P_HI))?;
    let p_lo = alloc_const(cs.namespace(|| "p_lo"), "p_lo", u128_to_scalar(SECP_P_LO))?;
    let n_hi = alloc_const(cs.namespace(|| "n_hi"), "n_hi", u128_to_scalar(SECP_N_HI))?;
    let n_lo = alloc_const(cs.namespace(|| "n_lo"), "n_lo", u128_to_scalar(SECP_N_LO))?;

    // 1. Canonical pk_x < p and even lift_x(pk_x) = (px, py).
    let (pk_hi, pk_lo, px) = pack_256_be_to_limbs_and_fp(cs.namespace(|| "pack_pk"), pk_bits_be)?;
    enforce_u256_lt_circuit(cs.namespace(|| "pk_lt_p"), &pk_hi, &pk_lo, &p_hi, &p_lo)?;
    let py = alloc_and_constrain_even_curve_y(cs.namespace(|| "lift_pk"), &px, &p_hi, &p_lo)?;

    // 2. Allocate signature (r_bits_be, s_bits_be), enforce r < p and s < n.
    let r_bits_be = alloc_bytes_be(
        cs.namespace(|| "r_bits"),
        sig_bytes.map(|s| &s[0..32]),
        32,
    )?;
    let s_bits_be = alloc_bytes_be(
        cs.namespace(|| "s_bits"),
        sig_bytes.map(|s| &s[32..64]),
        32,
    )?;

    let (r_hi, r_lo, rx) = pack_256_be_to_limbs_and_fp(cs.namespace(|| "pack_r"), &r_bits_be)?;
    enforce_u256_lt_circuit(cs.namespace(|| "r_lt_p"), &r_hi, &r_lo, &p_hi, &p_lo)?;

    let (s_hi, s_lo, _s_fp) = pack_256_be_to_limbs_and_fp(cs.namespace(|| "pack_s"), &s_bits_be)?;
    enforce_u256_lt_circuit(cs.namespace(|| "s_lt_n"), &s_hi, &s_lo, &n_hi, &n_lo)?;

    // 3. Challenge H = SHA256_tagged("BIP0340/challenge", r || pk || msg) and exact mod n reduction.
    let mut chall_bits_be = Vec::with_capacity(768);
    chall_bits_be.extend_from_slice(&r_bits_be);
    chall_bits_be.extend_from_slice(pk_bits_be);
    chall_bits_be.extend_from_slice(msg_bits_be);

    let h_bits_be = tagged_sha256_circuit(
        cs.namespace(|| "bip340_challenge_hash"),
        TAG_BIP0340_CHALLENGE,
        &chall_bits_be,
    )?;
    let (h_hi, h_lo, _) = pack_256_be_to_limbs_and_fp(cs.namespace(|| "pack_h"), &h_bits_be)?;

    // Compute native (q_bit, c_lo, e_bytes_be) from h_bits_be:
    let mod_n_wit: Option<(bool, bool, [u8; 32])> = booleans_be_to_bytes(&h_bits_be).map(|hb| {
        let mut h_hi_b = [0u8; 16];
        let mut h_lo_b = [0u8; 16];
        h_hi_b.copy_from_slice(&hb[0..16]);
        h_lo_b.copy_from_slice(&hb[16..32]);
        let h_hi_u = u128::from_be_bytes(h_hi_b);
        let h_lo_u = u128::from_be_bytes(h_lo_b);

        let ge_n = (h_hi_u, h_lo_u) >= (SECP_N_HI, SECP_N_LO);
        let (e_hi_u, e_lo_u, c_lo) = if ge_n {
            let (lo_diff, borrow) = h_lo_u.overflowing_sub(SECP_N_LO);
            let hi_diff = h_hi_u.wrapping_sub(SECP_N_HI).wrapping_sub(borrow as u128);
            // Note: lo_diff + SECP_N_LO overflows u128 iff borrow == true!
            (hi_diff, lo_diff, borrow)
        } else {
            (h_hi_u, h_lo_u, false)
        };
        let mut eb = [0u8; 32];
        eb[0..16].copy_from_slice(&e_hi_u.to_be_bytes());
        eb[16..32].copy_from_slice(&e_lo_u.to_be_bytes());
        (ge_n, c_lo, eb)
    });

    let q_bit = Boolean::from(AllocatedBit::alloc(
        cs.namespace(|| "q_bit"),
        mod_n_wit.as_ref().map(|w| w.0),
    )?);
    let carry_lo = Boolean::from(AllocatedBit::alloc(
        cs.namespace(|| "carry_lo"),
        mod_n_wit.as_ref().map(|w| w.1),
    )?);
    let e_bits_be = alloc_bytes_be(
        cs.namespace(|| "e_bits"),
        mod_n_wit.as_ref().map(|w| &w.2[..]),
        32,
    )?;
    let (e_hi, e_lo, _) = pack_256_be_to_limbs_and_fp(cs.namespace(|| "pack_e"), &e_bits_be)?;
    enforce_u256_lt_circuit(cs.namespace(|| "e_lt_n"), &e_hi, &e_lo, &n_hi, &n_lo)?;

    let two_128 = u128_to_scalar::<Fp>(1u128 << 64) * u128_to_scalar::<Fp>(1u128 << 64);
    let n_lo_fp = u128_to_scalar::<Fp>(SECP_N_LO);
    let n_hi_fp = u128_to_scalar::<Fp>(SECP_N_HI);

    // Exact integer limb equations (all terms < 2^129 << p):
    //   e_lo + q_bit * N_LO == h_lo + carry_lo * 2^128
    //   e_hi + q_bit * N_HI + carry_lo == h_hi
    cs.enforce(
        || "mod_n_lo_limb",
        |lc| {
            lc + e_lo.get_variable()
                + &q_bit.lc(CS::one(), n_lo_fp)
                - h_lo.get_variable()
                - &carry_lo.lc(CS::one(), two_128)
        },
        |lc| lc + CS::one(),
        |lc| lc,
    );
    cs.enforce(
        || "mod_n_hi_limb",
        |lc| {
            lc + e_hi.get_variable()
                + &q_bit.lc(CS::one(), n_hi_fp)
                + &carry_lo.lc(CS::one(), Fp::ONE)
                - h_hi.get_variable()
        },
        |lc| lc + CS::one(),
        |lc| lc,
    );

    // 4. Witness and constrain R = (rx, ry) on curve with even ry < p (`R != infinity`).
    let ry = alloc_and_constrain_even_curve_y(cs.namespace(|| "lift_R"), &rx, &p_hi, &p_lo)?;

    // 5A. Fixed-base scalar mul: `acc_s = T_1 + s * G` (LSB first: s_bits_be[255 - i]).
    let mut acc_s = (
        alloc_const(cs.namespace(|| "t1_x"), "t1_x", tables.t1.0)?,
        alloc_const(cs.namespace(|| "t1_y"), "t1_y", tables.t1.1)?,
    );
    for i in 0..256 {
        let s_bit = &s_bits_be[255 - i];
        let gx = alloc_const(cs.namespace(|| format!("gx_{i}")), "gx", tables.g_pows[i].0)?;
        let gy = alloc_const(cs.namespace(|| format!("gy_{i}")), "gy", tables.g_pows[i].1)?;
        let added = point_add_incomplete(
            cs.namespace(|| format!("s_add_{i}")),
            &acc_s.0,
            &acc_s.1,
            &gx,
            &gy,
        )?;
        acc_s = select_point(cs.namespace(|| format!("s_sel_{i}")), s_bit, &added, &acc_s)?;
    }

    // 5B. Variable-base scalar mul: `acc_e = T_2 + e * P` (LSB first: e_bits_be[255 - i]).
    let mut acc_e = (
        alloc_const(cs.namespace(|| "t2_x"), "t2_x", tables.t2.0)?,
        alloc_const(cs.namespace(|| "t2_y"), "t2_y", tables.t2.1)?,
    );
    let mut cur_p = (px, py);
    for i in 0..256 {
        let e_bit = &e_bits_be[255 - i];
        let added = point_add_incomplete(
            cs.namespace(|| format!("e_add_{i}")),
            &acc_e.0,
            &acc_e.1,
            &cur_p.0,
            &cur_p.1,
        )?;
        acc_e = select_point(cs.namespace(|| format!("e_sel_{i}")), e_bit, &added, &acc_e)?;
        if i + 1 < 256 {
            cur_p = point_double(cs.namespace(|| format!("p_dbl_{i}")), &cur_p.0, &cur_p.1)?;
        }
    }

    // 5C. Enforce `acc_s == (acc_e + (T_1 - T_2)) + R`.
    let diff_x = alloc_const(cs.namespace(|| "dt_x"), "dt_x", tables.t1_minus_t2.0)?;
    let diff_y = alloc_const(cs.namespace(|| "dt_y"), "dt_y", tables.t1_minus_t2.1)?;
    let shifted_ep = point_add_incomplete(
        cs.namespace(|| "shift_ep"),
        &acc_e.0,
        &acc_e.1,
        &diff_x,
        &diff_y,
    )?;
    let rhs = point_add_incomplete(
        cs.namespace(|| "add_R"),
        &shifted_ep.0,
        &shifted_ep.1,
        &rx,
        &ry,
    )?;

    cs.enforce(
        || "bip340_eq_x",
        |lc| lc + acc_s.0.get_variable() - rhs.0.get_variable(),
        |lc| lc + CS::one(),
        |lc| lc,
    );
    cs.enforce(
        || "bip340_eq_y",
        |lc| lc + acc_s.1.get_variable() - rhs.1.get_variable(),
        |lc| lc + CS::one(),
        |lc| lc,
    );
    Ok(())
}
