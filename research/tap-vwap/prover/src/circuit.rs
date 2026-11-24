//! Uniform R1CS `StepCircuit` (`TapVwapStepCircuit`) for `TAP-VWAP-03` over
//! `Secq256k1Engine::Scalar` (`halo2curves::secp256k1::Fp`).
//!
//! Enforces on EVERY step (no caller trust/settlement booleans, no prover-selected
//! registry keys, no epoch resets of nullifier state):
//! 1. Canonical 526-byte `Receipt/v1` wire parsing, positive volume checks, non-null
//!    outpoints, distinct state commitments, and opposite-leg offerer bits.
//! 2. Policy, genesis, and Taproot Asset ID equality against public epoch state `z_i`.
//! 3. Tagged SHA-256 `Receipt/v1` conversion digest + 3x BIP-340 Schnorr signature
//!    verifications (`edge`, `btc_peer`, `asset_peer`) with distinct signer keys.
//! 4. Authenticated `registry_root` Poseidon Merkle inclusion for BOTH `asset_outpoint`
//!    and `btc_outpoint`, pinning `is_asset_channel`, `funding_resolved == 1`, `asset_id`,
//!    and exact `party0_rec_key`/`party1_rec_key` mapped via immutable `edge_side` bits.
//! 5. Tagged SHA-256 `AssetHTLC/v1` canonical 77-byte nullifier + persistent cross-epoch
//!    256-bit Indexed Merkle non-membership (`low.val < nul < low.next`) and insertion.
//! 6. Persistent `quote_root` Merkle inclusion + cumulative outgoing msat cap check
//!    (`consumed + delta <= max_out_msat`), reconciling `delta` against `gross_btc_msat`.
//! 7. Tagged SHA-256 `BatchLog/v1` hash-chain update, wrap-free `S_B`/`S_U` accumulation,
//!    and exact half-up rounded `q8` VWAP division (`q_raw * S_B + rem == scale * S_U`).

use ff::{Field, PrimeField};
use halo2curves::secp256k1::Fp;
use nova_snark::{
    frontend::{
        gadgets::boolean::Boolean, num::AllocatedNum, AllocatedBit, ConstraintSystem,
        LinearCombination, SynthesisError,
    },
    traits::circuit::StepCircuit,
};

use crate::manifest::{
    compute_vwap_q8_half_up, EVIDENCE_TIER_1_SIGNED_ASSERTION, RECEIPT_V1_BYTES,
    RECEIPT_VERSION_V1, TAG_ASSET_HTLC_V1, TAG_RECEIPT_V1,
};
use crate::merkle::{
    bytes32_to_scalar, bytes32_to_u128_pair, compute_merkle_root_circuit, enforce_u256_lt_circuit,
    range_check_bits_le, u128_to_scalar, PoseidonHasher, DOMAIN_CHANNEL_REGISTRY_LEAF,
    DOMAIN_INDEXED_NULLIFIER_LEAF, DOMAIN_QUOTE_CONSUMPTION_LEAF, STATE_TREE_DEPTH,
};
use crate::native_verifier::PreparedReceiptStep;
use crate::secp_gadget::{
    pack_256_be_to_limbs_and_fp, verify_bip340_circuit, Bip340CircuitTables,
};
use crate::sha256_gadget::{alloc_bytes_be, tagged_sha256_circuit};

/// Step state vector arity (`z_i` has 18 field elements).
pub const STEP_STATE_ARITY: usize = 18;

/// Domain tag for `BatchLog/v1`.
pub const TAG_BATCH_LOG_V1: &[u8] = b"TradeLayer/TAP-VWAP/BatchLog/v1";

/// Build the initial step vector `z_0` from public epoch parameters and prior persistent roots.
pub fn build_initial_z0(
    policy_hash: &[u8; 32],
    chain_genesis: &[u8; 32],
    asset_id: &[u8; 32],
    asset_decimals: u8,
    epoch_id: u64,
    registry_root: &[u8; 32],
    prev_nullifier_root: &[u8; 32],
    prev_quote_root: &[u8; 32],
) -> Result<Vec<Fp>, String> {
    if asset_decimals > 19 {
        return Err("asset_decimals > 19 unsupported".to_string());
    }
    let scale = 10u128.pow((19 - asset_decimals) as u32);
    let (pol_hi, pol_lo) = bytes32_to_u128_pair(policy_hash);
    let (gen_hi, gen_lo) = bytes32_to_u128_pair(chain_genesis);
    let (ast_hi, ast_lo) = bytes32_to_u128_pair(asset_id);
    let reg_root = bytes32_to_scalar::<Fp>(registry_root)?;
    let nul_root = bytes32_to_scalar::<Fp>(prev_nullifier_root)?;
    let quo_root = bytes32_to_scalar::<Fp>(prev_quote_root)?;

    Ok(vec![
        u128_to_scalar(pol_hi), // 0: policy_hi
        u128_to_scalar(pol_lo), // 1: policy_lo
        u128_to_scalar(gen_hi), // 2: genesis_hi
        u128_to_scalar(gen_lo), // 3: genesis_lo
        u128_to_scalar(ast_hi), // 4: asset_hi
        u128_to_scalar(ast_lo), // 5: asset_lo
        u128_to_scalar(scale),  // 6: scale_pow10 = 10^(19 - d)
        Fp::from(epoch_id),     // 7: epoch_id
        reg_root,               // 8: registry_root
        nul_root,               // 9: nullifier_root
        quo_root,               // 10: quote_root
        Fp::ZERO,               // 11: batch_log_hi
        Fp::ZERO,               // 12: batch_log_lo
        Fp::ZERO,               // 13: sum_btc_msat (S_B)
        Fp::ZERO,               // 14: sum_asset_atoms (S_U)
        Fp::ZERO,               // 15: conversion_count (n)
        Fp::ZERO,               // 16: vwap_price_q8
        Fp::ZERO,               // 17: evidence_tier (0 before first step; 1 after >=1 step)
    ])
}

/// Build the exact expected final state vector `z_n` from a verified `PublicEpochManifest`.
pub fn expected_zn_from_manifest(
    manifest: &crate::manifest::PublicEpochManifest,
) -> Result<Vec<Fp>, String> {
    if manifest.conversion_count == 0 {
        return Err("manifest must have conversion_count >= 1".to_string());
    }
    if manifest.evidence_tier != EVIDENCE_TIER_1_SIGNED_ASSERTION {
        return Err("unexpected evidence_tier in manifest".to_string());
    }
    if manifest.eligible_for_consensus_index {
        return Err("Tier-1 manifest must set eligible_for_consensus_index = false".to_string());
    }
    let scale = 10u128.pow((19 - manifest.asset_decimals) as u32);
    let (pol_hi, pol_lo) = bytes32_to_u128_pair(&manifest.policy_hash);
    let (gen_hi, gen_lo) = bytes32_to_u128_pair(&manifest.chain_genesis);
    let (ast_hi, ast_lo) = bytes32_to_u128_pair(&manifest.asset_id);
    let (bl_hi, bl_lo) = bytes32_to_u128_pair(&manifest.batch_log_commitment);
    let reg_root = bytes32_to_scalar::<Fp>(&manifest.registry_root)?;
    let nul_root = bytes32_to_scalar::<Fp>(&manifest.next_nullifier_root)?;
    let quo_root = bytes32_to_scalar::<Fp>(&manifest.next_quote_root)?;

    Ok(vec![
        u128_to_scalar(pol_hi),
        u128_to_scalar(pol_lo),
        u128_to_scalar(gen_hi),
        u128_to_scalar(gen_lo),
        u128_to_scalar(ast_hi),
        u128_to_scalar(ast_lo),
        u128_to_scalar(scale),
        Fp::from(manifest.epoch_id),
        reg_root,
        nul_root,
        quo_root,
        u128_to_scalar(bl_hi),
        u128_to_scalar(bl_lo),
        u128_to_scalar(manifest.sum_btc_msat),
        u128_to_scalar(manifest.sum_asset_atoms),
        Fp::from(manifest.conversion_count),
        u128_to_scalar(manifest.vwap_price_q8),
        Fp::from(EVIDENCE_TIER_1_SIGNED_ASSERTION as u64),
    ])
}

/// Pack a big-endian bit slice (`len <= 64`) into an `AllocatedNum<Fp>`.
fn pack_be_u64<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    bits_be: &[Boolean],
) -> Result<AllocatedNum<Fp>, SynthesisError> {
    assert!(bits_be.len() <= 64);
    let mut lc = LinearCombination::<Fp>::zero();
    let mut coeff = Fp::ONE;
    let mut val_opt: Option<u64> = Some(0);
    for (i, b) in bits_be.iter().rev().enumerate() {
        lc = lc + &b.lc(CS::one(), coeff);
        coeff = coeff.double();
        val_opt = match (val_opt, b.get_value()) {
            (Some(acc), Some(bit)) => Some(acc | ((bit as u64) << i)),
            _ => None,
        };
    }
    let num = AllocatedNum::alloc(cs.namespace(|| "u64_num"), || {
        let v = val_opt.ok_or(SynthesisError::AssignmentMissing)?;
        Ok(Fp::from(v))
    })?;
    cs.enforce(
        || "constrain_u64_bits",
        |_| lc,
        |lc1| lc1 + CS::one(),
        |lc_n| lc_n + num.get_variable(),
    );
    Ok(num)
}

/// Enforce `x != 0` in `Fp` via witness multiplicative inverse.
fn enforce_nonzero<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    x: &AllocatedNum<Fp>,
) -> Result<(), SynthesisError> {
    let inv = AllocatedNum::alloc(cs.namespace(|| "inv"), || {
        let v = x.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(Option::<Fp>::from(v.invert()).unwrap_or(Fp::ZERO))
    })?;
    cs.enforce(
        || "x * inv == 1",
        |lc| lc + x.get_variable(),
        |lc| lc + inv.get_variable(),
        |lc| lc + CS::one(),
    );
    Ok(())
}

/// Enforce two 256-bit pairs `(a_hi, a_lo)` and `(b_hi, b_lo)` are not identical.
fn enforce_u256_neq<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    a_hi: &AllocatedNum<Fp>,
    a_lo: &AllocatedNum<Fp>,
    b_hi: &AllocatedNum<Fp>,
    b_lo: &AllocatedNum<Fp>,
) -> Result<(), SynthesisError> {
    // Witness selector `use_hi = (a_hi != b_hi)`; enforce selected limb difference != 0.
    let use_hi = Boolean::from(AllocatedBit::alloc(
        cs.namespace(|| "use_hi"),
        a_hi.get_value()
            .zip(b_hi.get_value())
            .map(|(ah, bh)| ah != bh),
    )?);
    let chosen_diff = AllocatedNum::alloc(cs.namespace(|| "chosen_diff"), || {
        let ah = a_hi.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let al = a_lo.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let bh = b_hi.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let bl = b_lo.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let uh = use_hi.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(if uh { ah - bh } else { al - bl })
    })?;
    // chosen_diff - (a_lo - b_lo) == use_hi * ((a_hi - b_hi) - (a_lo - b_lo))
    cs.enforce(
        || "select_neq_limb",
        |lc| lc + &use_hi.lc(CS::one(), Fp::ONE),
        |lc| lc + a_hi.get_variable() - b_hi.get_variable() - a_lo.get_variable() + b_lo.get_variable(),
        |lc| lc + chosen_diff.get_variable() - a_lo.get_variable() + b_lo.get_variable(),
    );
    enforce_nonzero(cs.namespace(|| "chosen_diff_nz"), &chosen_diff)
}

fn select_limb<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    cond: &Boolean,
    if_zero: &AllocatedNum<Fp>,
    if_one: &AllocatedNum<Fp>,
) -> Result<AllocatedNum<Fp>, SynthesisError> {
    let res = AllocatedNum::alloc(cs.namespace(|| "sel"), || {
        let c = cond.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let v0 = if_zero.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let v1 = if_one.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(if c { v1 } else { v0 })
    })?;
    cs.enforce(
        || "cond * (v1 - v0) == res - v0",
        |lc| lc + &cond.lc(CS::one(), Fp::ONE),
        |lc| lc + if_one.get_variable() - if_zero.get_variable(),
        |lc| lc + res.get_variable() - if_zero.get_variable(),
    );
    Ok(res)
}

fn alloc_siblings_array<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    sibs: Option<&[Fp; STATE_TREE_DEPTH]>,
) -> Result<[AllocatedNum<Fp>; STATE_TREE_DEPTH], SynthesisError> {
    let mut vec: Vec<AllocatedNum<Fp>> = Vec::with_capacity(STATE_TREE_DEPTH);
    for i in 0..STATE_TREE_DEPTH {
        vec.push(AllocatedNum::alloc(
            cs.namespace(|| format!("sib_{i}")),
            || Ok(sibs.map(|s| s[i]).unwrap_or(Fp::ZERO)),
        )?);
    }
    vec.try_into()
        .map_err(|_| SynthesisError::Unsatisfiable("siblings len mismatch".to_string()))
}

/// Enforce a single byte bit-slice `[b7..b0]` represents a 1-bit value (`0` or `1`) and return LSB.
fn enforce_bit_byte<CS: ConstraintSystem<Fp>>(
    mut cs: CS,
    byte_bits_be: &[Boolean],
) -> Result<Boolean, SynthesisError> {
    assert_eq!(byte_bits_be.len(), 8);
    for (i, b) in byte_bits_be[0..7].iter().enumerate() {
        cs.enforce(
            || format!("zero_high_bit_{i}"),
            |lc| lc + &b.lc(CS::one(), Fp::ONE),
            |lc| lc + CS::one(),
            |lc| lc,
        );
    }
    Ok(byte_bits_be[7].clone())
}

/// Uniform Nova `StepCircuit<Fp>` for TAP-VWAP-03.
#[derive(Clone)]
pub struct TapVwapStepCircuit {
    /// Cached Poseidon hasher (`U5`).
    pub hasher: PoseidonHasher<Fp>,
    /// Cached BIP-340 curve tables (`[2^i * G]`, `T_1`, `T_2`).
    pub bip340_tables: Bip340CircuitTables,
    /// Optional witness (`None` during `PublicParams::setup` shape synthesis).
    pub step_witness: Option<PreparedReceiptStep<Fp>>,
}

impl TapVwapStepCircuit {
    /// Create blank circuit for `PublicParams::setup` R1CS shape synthesis.
    pub fn blank(hasher: PoseidonHasher<Fp>, bip340_tables: Bip340CircuitTables) -> Self {
        Self {
            hasher,
            bip340_tables,
            step_witness: None,
        }
    }

    /// Create witness-populated step circuit for `RecursiveSNARK::prove_step`.
    pub fn with_witness(
        hasher: PoseidonHasher<Fp>,
        bip340_tables: Bip340CircuitTables,
        step_witness: PreparedReceiptStep<Fp>,
    ) -> Self {
        Self {
            hasher,
            bip340_tables,
            step_witness: Some(step_witness),
        }
    }
}

impl StepCircuit<Fp> for TapVwapStepCircuit {
    fn arity(&self) -> usize {
        STEP_STATE_ARITY
    }

    fn synthesize<CS: ConstraintSystem<Fp>>(
        &self,
        cs: &mut CS,
        z: &[AllocatedNum<Fp>],
    ) -> Result<Vec<AllocatedNum<Fp>>, SynthesisError> {
        assert_eq!(z.len(), STEP_STATE_ARITY);

        let z_pol_hi = &z[0];
        let z_pol_lo = &z[1];
        let z_gen_hi = &z[2];
        let z_gen_lo = &z[3];
        let z_ast_hi = &z[4];
        let z_ast_lo = &z[5];
        let z_scale = &z[6];
        let z_epoch = &z[7];
        let z_reg_root = &z[8];
        let z_nul_root = &z[9];
        let z_quo_root = &z[10];
        let z_blog_hi = &z[11];
        let z_blog_lo = &z[12];
        let z_sum_btc = &z[13];
        let z_sum_ast = &z[14];
        let z_count = &z[15];

        let wit = self.step_witness.as_ref();

        // ====================================================================
        // 1. Allocate 526-byte Receipt/v1 wire bits and enforce wire contract
        // ====================================================================
        let raw_bits_be = alloc_bytes_be(
            cs.namespace(|| "receipt_raw_526b"),
            wit.map(|w| &w.raw_receipt[..]),
            RECEIPT_V1_BYTES,
        )?;

        // Version byte == 1
        let ver_num = pack_be_u64(cs.namespace(|| "ver"), &raw_bits_be[0..8])?;
        cs.enforce(
            || "version == 1",
            |lc| lc + ver_num.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + (Fp::from(RECEIPT_VERSION_V1 as u64), CS::one()),
        );

        // Genesis (bytes 1..33), Policy (33..65), Asset (65..97) match public state `z`!
        let (gen_hi, gen_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "gen"), &raw_bits_be[8..264])?;
        let (pol_hi, pol_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "pol"), &raw_bits_be[264..520])?;
        let (ast_hi, ast_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "ast"), &raw_bits_be[520..776])?;

        for (lbl, a, b) in [
            ("gen_hi_eq", &gen_hi, z_gen_hi),
            ("gen_lo_eq", &gen_lo, z_gen_lo),
            ("pol_hi_eq", &pol_hi, z_pol_hi),
            ("pol_lo_eq", &pol_lo, z_pol_lo),
            ("ast_hi_eq", &ast_hi, z_ast_hi),
            ("ast_lo_eq", &ast_lo, z_ast_lo),
        ] {
            cs.enforce(
                || lbl,
                |lc| lc + a.get_variable() - b.get_variable(),
                |lc| lc + CS::one(),
                |lc| lc,
            );
        }

        // Signer x-only pubkeys (97..129, 129..161, 161..193)
        let edge_pk_bits = &raw_bits_be[776..1032];
        let btc_pk_bits = &raw_bits_be[1032..1288];
        let asset_pk_bits = &raw_bits_be[1288..1544];

        let (edge_hi, edge_lo, edge_fp) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "edge_pk"), edge_pk_bits)?;
        let (btc_pk_hi, btc_pk_lo, btc_pk_fp) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "btc_pk"), btc_pk_bits)?;
        let (ast_pk_hi, ast_pk_lo, ast_pk_fp) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "ast_pk"), asset_pk_bits)?;

        // Enforce 3 distinct signer keys (since verify_bip340_circuit enforces each < p, Fp inequality is exact).
        let diff_eb = AllocatedNum::alloc(cs.namespace(|| "diff_eb"), || {
            Ok(edge_fp.get_value().ok_or(SynthesisError::AssignmentMissing)?
                - btc_pk_fp.get_value().ok_or(SynthesisError::AssignmentMissing)?)
        })?;
        cs.enforce(
            || "diff_eb_eq",
            |lc| lc + edge_fp.get_variable() - btc_pk_fp.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + diff_eb.get_variable(),
        );
        enforce_nonzero(cs.namespace(|| "nz_eb"), &diff_eb)?;

        let diff_ea = AllocatedNum::alloc(cs.namespace(|| "diff_ea"), || {
            Ok(edge_fp.get_value().ok_or(SynthesisError::AssignmentMissing)?
                - ast_pk_fp.get_value().ok_or(SynthesisError::AssignmentMissing)?)
        })?;
        cs.enforce(
            || "diff_ea_eq",
            |lc| lc + edge_fp.get_variable() - ast_pk_fp.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + diff_ea.get_variable(),
        );
        enforce_nonzero(cs.namespace(|| "nz_ea"), &diff_ea)?;

        let diff_ba = AllocatedNum::alloc(cs.namespace(|| "diff_ba"), || {
            Ok(btc_pk_fp.get_value().ok_or(SynthesisError::AssignmentMissing)?
                - ast_pk_fp.get_value().ok_or(SynthesisError::AssignmentMissing)?)
        })?;
        cs.enforce(
            || "diff_ba_eq",
            |lc| lc + btc_pk_fp.get_variable() - ast_pk_fp.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + diff_ba.get_variable(),
        );
        enforce_nonzero(cs.namespace(|| "nz_ba"), &diff_ba)?;

        // RFQ ID (193..225) and QuoteTranscript (225..257)
        let (rfq_hi, rfq_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "rfq_id"), &raw_bits_be[1544..1800])?;
        let (qt_hi, qt_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "qt_dig"), &raw_bits_be[1800..2056])?;

        // Pre/post state commitments must differ on both channels (289..321 vs 321..353, 353..385 vs 385..417)
        let (apre_hi, apre_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "apre"), &raw_bits_be[2312..2568])?;
        let (apost_hi, apost_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "apost"), &raw_bits_be[2568..2824])?;
        enforce_u256_neq(
            cs.namespace(|| "asset_state_changed"),
            &apre_hi,
            &apre_lo,
            &apost_hi,
            &apost_lo,
        )?;

        let (bpre_hi, bpre_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "bpre"), &raw_bits_be[2824..3080])?;
        let (bpost_hi, bpost_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "bpost"), &raw_bits_be[3080..3336])?;
        enforce_u256_neq(
            cs.namespace(|| "btc_state_changed"),
            &bpre_hi,
            &bpre_lo,
            &bpost_hi,
            &bpost_lo,
        )?;

        // Direction (byte 417), asset_edge_side (418), btc_edge_side (419)
        let dir_bit = enforce_bit_byte(cs.namespace(|| "dir"), &raw_bits_be[3336..3344])?;
        let asset_edge_bit =
            enforce_bit_byte(cs.namespace(|| "asset_edge"), &raw_bits_be[3344..3352])?;
        let btc_edge_bit =
            enforce_bit_byte(cs.namespace(|| "btc_edge"), &raw_bits_be[3352..3360])?;

        // Asset outpoint (bytes 420..456) & asset_offerer (byte 456)
        let (atx_hi, atx_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "atx"), &raw_bits_be[3360..3616])?;
        let avout = pack_be_u64(cs.namespace(|| "avout"), &raw_bits_be[3616..3648])?;
        let zero_num = AllocatedNum::alloc(cs.namespace(|| "zero"), || Ok(Fp::ZERO))?;
        cs.enforce(
            || "pin_zero",
            |lc| lc + zero_num.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc,
        );
        enforce_u256_neq(
            cs.namespace(|| "atx_nonzero"),
            &atx_hi,
            &atx_lo,
            &zero_num,
            &zero_num,
        )?;
        let avout_not_max = AllocatedNum::alloc(cs.namespace(|| "avout_not_max"), || {
            Ok(avout.get_value().ok_or(SynthesisError::AssignmentMissing)? - Fp::from(0xffff_ffffu64))
        })?;
        cs.enforce(
            || "avout_not_max_eq",
            |lc| lc + avout.get_variable() - (Fp::from(0xffff_ffffu64), CS::one()),
            |lc| lc + CS::one(),
            |lc| lc + avout_not_max.get_variable(),
        );
        enforce_nonzero(cs.namespace(|| "avout_valid"), &avout_not_max)?;

        let asset_offerer_bit =
            enforce_bit_byte(cs.namespace(|| "asset_offerer"), &raw_bits_be[3648..3656])?;

        // BTC outpoint (bytes 465..501) & btc_offerer (byte 501)
        let (btx_hi, btx_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "btx"), &raw_bits_be[3720..3976])?;
        let bvout = pack_be_u64(cs.namespace(|| "bvout"), &raw_bits_be[3976..4008])?;
        enforce_u256_neq(
            cs.namespace(|| "btx_nonzero"),
            &btx_hi,
            &btx_lo,
            &zero_num,
            &zero_num,
        )?;
        let bvout_not_max = AllocatedNum::alloc(cs.namespace(|| "bvout_not_max"), || {
            Ok(bvout.get_value().ok_or(SynthesisError::AssignmentMissing)? - Fp::from(0xffff_ffffu64))
        })?;
        cs.enforce(
            || "bvout_not_max_eq",
            |lc| lc + bvout.get_variable() - (Fp::from(0xffff_ffffu64), CS::one()),
            |lc| lc + CS::one(),
            |lc| lc + bvout_not_max.get_variable(),
        );
        enforce_nonzero(cs.namespace(|| "bvout_valid"), &bvout_not_max)?;

        let btc_offerer_bit =
            enforce_bit_byte(cs.namespace(|| "btc_offerer"), &raw_bits_be[4008..4016])?;

        // Opposite-leg offerer constraints:
        //   2 * dir * asset_edge == asset_offerer + dir + asset_edge - 1
        //   2 * dir * btc_edge == dir + btc_edge - btc_offerer
        cs.enforce(
            || "asset_offerer_vs_direction",
            |lc| lc + &dir_bit.lc(CS::one(), Fp::from(2u64)),
            |lc| lc + &asset_edge_bit.lc(CS::one(), Fp::ONE),
            |lc| {
                lc + &asset_offerer_bit.lc(CS::one(), Fp::ONE)
                    + &dir_bit.lc(CS::one(), Fp::ONE)
                    + &asset_edge_bit.lc(CS::one(), Fp::ONE)
                    - CS::one()
            },
        );
        cs.enforce(
            || "btc_offerer_vs_direction",
            |lc| lc + &dir_bit.lc(CS::one(), Fp::from(2u64)),
            |lc| lc + &btc_edge_bit.lc(CS::one(), Fp::ONE),
            |lc| {
                lc + &dir_bit.lc(CS::one(), Fp::ONE)
                    + &btc_edge_bit.lc(CS::one(), Fp::ONE)
                    - &btc_offerer_bit.lc(CS::one(), Fp::ONE)
            },
        );

        // Positive amounts: asset_atoms (510..518) > 0, gross_btc_msat (518..526) > 0
        let asset_atoms_num = pack_be_u64(cs.namespace(|| "asset_atoms"), &raw_bits_be[4080..4144])?;
        let gross_btc_num = pack_be_u64(cs.namespace(|| "gross_btc"), &raw_bits_be[4144..4208])?;
        enforce_nonzero(cs.namespace(|| "asset_atoms_pos"), &asset_atoms_num)?;
        enforce_nonzero(cs.namespace(|| "gross_btc_pos"), &gross_btc_num)?;

        // ====================================================================
        // 2. Tagged SHA-256 Receipt/v1 Digest + 3x BIP-340 Schnorr Signatures
        // ====================================================================
        let digest_bits_be = tagged_sha256_circuit(
            cs.namespace(|| "receipt_v1_digest"),
            TAG_RECEIPT_V1,
            &raw_bits_be,
        )?;

        verify_bip340_circuit(
            cs.namespace(|| "bip340_edge"),
            &self.bip340_tables,
            edge_pk_bits,
            &digest_bits_be,
            wit.map(|w| &w.signatures.edge),
        )?;
        verify_bip340_circuit(
            cs.namespace(|| "bip340_btc_peer"),
            &self.bip340_tables,
            btc_pk_bits,
            &digest_bits_be,
            wit.map(|w| &w.signatures.btc_peer),
        )?;
        verify_bip340_circuit(
            cs.namespace(|| "bip340_asset_peer"),
            &self.bip340_tables,
            asset_pk_bits,
            &digest_bits_be,
            wit.map(|w| &w.signatures.asset_peer),
        )?;

        // ====================================================================
        // 3. Authenticated ChannelRegistry Merkle Inclusion (Asset & BTC Legs)
        // ====================================================================
        let areg_idx_num = AllocatedNum::alloc(cs.namespace(|| "areg_idx"), || {
            Ok(Fp::from(wit.map(|w| w.asset_reg_idx as u64).unwrap_or(0)))
        })?;
        let breg_idx_num = AllocatedNum::alloc(cs.namespace(|| "breg_idx"), || {
            Ok(Fp::from(wit.map(|w| w.btc_reg_idx as u64).unwrap_or(1)))
        })?;
        // Distinct registry channel slots:
        let reg_idx_diff = AllocatedNum::alloc(cs.namespace(|| "reg_idx_diff"), || {
            Ok(areg_idx_num.get_value().ok_or(SynthesisError::AssignmentMissing)?
                - breg_idx_num.get_value().ok_or(SynthesisError::AssignmentMissing)?)
        })?;
        cs.enforce(
            || "reg_idx_diff_eq",
            |lc| lc + areg_idx_num.get_variable() - breg_idx_num.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + reg_idx_diff.get_variable(),
        );
        enforce_nonzero(cs.namespace(|| "distinct_reg_slots"), &reg_idx_diff)?;

        let areg_idx_bits =
            range_check_bits_le(cs.namespace(|| "areg_idx_bits"), &areg_idx_num, STATE_TREE_DEPTH)?;
        let breg_idx_bits =
            range_check_bits_le(cs.namespace(|| "breg_idx_bits"), &breg_idx_num, STATE_TREE_DEPTH)?;

        let ap0_hi = select_limb(cs.namespace(|| "ap0_hi"), &asset_edge_bit, &edge_hi, &ast_pk_hi)?;
        let ap0_lo = select_limb(cs.namespace(|| "ap0_lo"), &asset_edge_bit, &edge_lo, &ast_pk_lo)?;
        let ap1_hi = select_limb(cs.namespace(|| "ap1_hi"), &asset_edge_bit, &ast_pk_hi, &edge_hi)?;
        let ap1_lo = select_limb(cs.namespace(|| "ap1_lo"), &asset_edge_bit, &ast_pk_lo, &edge_lo)?;

        // Asset channel flags: vout | (1 << 32 [is_asset]) | (1 << 33 [funding_resolved]) | (1 << 34 [active])
        let asset_flags_const = (1u64 << 32) | (1u64 << 33) | (1u64 << 34);
        let aflags = AllocatedNum::alloc(cs.namespace(|| "aflags"), || {
            Ok(avout.get_value().ok_or(SynthesisError::AssignmentMissing)?
                + Fp::from(asset_flags_const))
        })?;
        cs.enforce(
            || "aflags_eq",
            |lc| lc + avout.get_variable() + (Fp::from(asset_flags_const), CS::one()),
            |lc| lc + CS::one(),
            |lc| lc + aflags.get_variable(),
        );

        let areg_leaf_hash = self.hasher.hash_circuit(
            cs.namespace(|| "areg_leaf_hash"),
            DOMAIN_CHANNEL_REGISTRY_LEAF,
            &[
                atx_hi,
                atx_lo,
                ap0_hi,
                ap0_lo,
                ap1_hi,
                ap1_lo,
                ast_hi,
                ast_lo,
                aflags,
            ],
        )?;
        let areg_sibs = alloc_siblings_array(
            cs.namespace(|| "areg_sibs"),
            wit.map(|w| &w.asset_reg_siblings),
        )?;
        let computed_areg_root = compute_merkle_root_circuit(
            cs.namespace(|| "areg_merkle"),
            &self.hasher,
            &areg_leaf_hash,
            &areg_idx_bits,
            &areg_sibs,
        )?;
        cs.enforce(
            || "areg_root == z_reg_root",
            |lc| lc + computed_areg_root.get_variable() - z_reg_root.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc,
        );

        // BTC channel registry verification (`is_asset == 0`, `asset_id == 0`, `funding_resolved == 1`)
        let bp0_hi = select_limb(cs.namespace(|| "bp0_hi"), &btc_edge_bit, &edge_hi, &btc_pk_hi)?;
        let bp0_lo = select_limb(cs.namespace(|| "bp0_lo"), &btc_edge_bit, &edge_lo, &btc_pk_lo)?;
        let bp1_hi = select_limb(cs.namespace(|| "bp1_hi"), &btc_edge_bit, &btc_pk_hi, &edge_hi)?;
        let bp1_lo = select_limb(cs.namespace(|| "bp1_lo"), &btc_edge_bit, &btc_pk_lo, &edge_lo)?;

        let btc_flags_const = (1u64 << 33) | (1u64 << 34); // is_asset = 0, funding_resolved = 1, active = 1
        let bflags = AllocatedNum::alloc(cs.namespace(|| "bflags"), || {
            Ok(bvout.get_value().ok_or(SynthesisError::AssignmentMissing)?
                + Fp::from(btc_flags_const))
        })?;
        cs.enforce(
            || "bflags_eq",
            |lc| lc + bvout.get_variable() + (Fp::from(btc_flags_const), CS::one()),
            |lc| lc + CS::one(),
            |lc| lc + bflags.get_variable(),
        );

        let breg_leaf_hash = self.hasher.hash_circuit(
            cs.namespace(|| "breg_leaf_hash"),
            DOMAIN_CHANNEL_REGISTRY_LEAF,
            &[
                btx_hi,
                btx_lo,
                bp0_hi,
                bp0_lo,
                bp1_hi,
                bp1_lo,
                zero_num.clone(),
                zero_num.clone(),
                bflags,
            ],
        )?;
        let breg_sibs = alloc_siblings_array(
            cs.namespace(|| "breg_sibs"),
            wit.map(|w| &w.btc_reg_siblings),
        )?;
        let computed_breg_root = compute_merkle_root_circuit(
            cs.namespace(|| "breg_merkle"),
            &self.hasher,
            &breg_leaf_hash,
            &breg_idx_bits,
            &breg_sibs,
        )?;
        cs.enforce(
            || "breg_root == z_reg_root",
            |lc| lc + computed_breg_root.get_variable() - z_reg_root.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc,
        );

        // ====================================================================
        // 4. Canonical 77-byte AssetHTLC/v1 Nullifier + Indexed Merkle Update
        // ====================================================================
        let mut nul_payload_bits = Vec::with_capacity(77 * 8);
        nul_payload_bits.extend_from_slice(&raw_bits_be[8..264]); // 32B genesis
        nul_payload_bits.extend_from_slice(&raw_bits_be[3360..3720]); // 36B asset_outpoint + 1B offerer + 8B htlc
        debug_assert_eq!(nul_payload_bits.len(), 77 * 8);

        let nul_bits_be = tagged_sha256_circuit(
            cs.namespace(|| "asset_htlc_nullifier"),
            TAG_ASSET_HTLC_V1,
            &nul_payload_bits,
        )?;
        let (nul_hi, nul_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "pack_nul"), &nul_bits_be)?;

        let one_num = AllocatedNum::alloc(cs.namespace(|| "one_num"), || Ok(Fp::ONE))?;
        cs.enforce(
            || "pin_one",
            |lc| lc + one_num.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + CS::one(),
        );

        // Allocate low-leaf witness & verify `low.val < nul < low.next`
        let low_idx_num = AllocatedNum::alloc(cs.namespace(|| "low_idx"), || {
            Ok(Fp::from(
                wit.map(|w| w.nullifier_witness.low_idx as u64).unwrap_or(0),
            ))
        })?;
        let low_idx_bits =
            range_check_bits_le(cs.namespace(|| "low_idx_bits"), &low_idx_num, STATE_TREE_DEPTH)?;

        let low_v_hi = AllocatedNum::alloc(cs.namespace(|| "low_v_hi"), || {
            Ok(u128_to_scalar(
                wit.map(|w| w.nullifier_witness.low_leaf_before.val_hi)
                    .unwrap_or(0),
            ))
        })?;
        let low_v_lo = AllocatedNum::alloc(cs.namespace(|| "low_v_lo"), || {
            Ok(u128_to_scalar(
                wit.map(|w| w.nullifier_witness.low_leaf_before.val_lo)
                    .unwrap_or(0),
            ))
        })?;
        let low_n_hi = AllocatedNum::alloc(cs.namespace(|| "low_n_hi"), || {
            Ok(u128_to_scalar(
                wit.map(|w| w.nullifier_witness.low_leaf_before.next_hi)
                    .unwrap_or(u128::MAX),
            ))
        })?;
        let low_n_lo = AllocatedNum::alloc(cs.namespace(|| "low_n_lo"), || {
            Ok(u128_to_scalar(
                wit.map(|w| w.nullifier_witness.low_leaf_before.next_lo)
                    .unwrap_or(u128::MAX),
            ))
        })?;

        let _ = range_check_bits_le(cs.namespace(|| "rc_lvh"), &low_v_hi, 128)?;
        let _ = range_check_bits_le(cs.namespace(|| "rc_lvl"), &low_v_lo, 128)?;
        let _ = range_check_bits_le(cs.namespace(|| "rc_lnh"), &low_n_hi, 128)?;
        let _ = range_check_bits_le(cs.namespace(|| "rc_lnl"), &low_n_lo, 128)?;

        enforce_u256_lt_circuit(
            cs.namespace(|| "low_lt_nul"),
            &low_v_hi,
            &low_v_lo,
            &nul_hi,
            &nul_lo,
        )?;
        enforce_u256_lt_circuit(
            cs.namespace(|| "nul_lt_next"),
            &nul_hi,
            &nul_lo,
            &low_n_hi,
            &low_n_lo,
        )?;

        let low_before_hash = self.hasher.hash_circuit(
            cs.namespace(|| "low_before_hash"),
            DOMAIN_INDEXED_NULLIFIER_LEAF,
            &[
                low_v_hi.clone(),
                low_v_lo.clone(),
                low_n_hi.clone(),
                low_n_lo.clone(),
                one_num.clone(),
            ],
        )?;
        let low_sibs = alloc_siblings_array(
            cs.namespace(|| "low_sibs"),
            wit.map(|w| &w.nullifier_witness.low_siblings),
        )?;

        let check_nul_before = compute_merkle_root_circuit(
            cs.namespace(|| "nul_root_before"),
            &self.hasher,
            &low_before_hash,
            &low_idx_bits,
            &low_sibs,
        )?;
        cs.enforce(
            || "nul_root_before == z_nul_root",
            |lc| lc + check_nul_before.get_variable() - z_nul_root.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc,
        );

        // Update low leaf `next := nul` -> `nul_root_mid`
        let low_after_hash = self.hasher.hash_circuit(
            cs.namespace(|| "low_after_hash"),
            DOMAIN_INDEXED_NULLIFIER_LEAF,
            &[
                low_v_hi,
                low_v_lo,
                nul_hi.clone(),
                nul_lo.clone(),
                one_num.clone(),
            ],
        )?;
        let nul_root_mid = compute_merkle_root_circuit(
            cs.namespace(|| "nul_root_mid"),
            &self.hasher,
            &low_after_hash,
            &low_idx_bits,
            &low_sibs,
        )?;

        // Prove `insert_idx` currently holds empty leaf `0` in `nul_root_mid`, then insert new leaf
        let ins_idx_num = AllocatedNum::alloc(cs.namespace(|| "ins_idx"), || {
            Ok(Fp::from(
                wit.map(|w| w.nullifier_witness.insert_idx as u64).unwrap_or(1),
            ))
        })?;
        enforce_nonzero(cs.namespace(|| "ins_idx_nonzero"), &ins_idx_num)?;
        let ins_idx_bits =
            range_check_bits_le(cs.namespace(|| "ins_idx_bits"), &ins_idx_num, STATE_TREE_DEPTH)?;

        let ins_sibs = alloc_siblings_array(
            cs.namespace(|| "ins_sibs"),
            wit.map(|w| &w.nullifier_witness.insert_siblings),
        )?;

        let check_nul_mid = compute_merkle_root_circuit(
            cs.namespace(|| "check_nul_mid_empty"),
            &self.hasher,
            &zero_num,
            &ins_idx_bits,
            &ins_sibs,
        )?;
        cs.enforce(
            || "check_nul_mid == nul_root_mid",
            |lc| lc + check_nul_mid.get_variable() - nul_root_mid.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc,
        );

        let ins_leaf_hash = self.hasher.hash_circuit(
            cs.namespace(|| "ins_leaf_hash"),
            DOMAIN_INDEXED_NULLIFIER_LEAF,
            &[nul_hi, nul_lo, low_n_hi, low_n_lo, one_num.clone()],
        )?;
        let next_nul_root = compute_merkle_root_circuit(
            cs.namespace(|| "next_nul_root"),
            &self.hasher,
            &ins_leaf_hash,
            &ins_idx_bits,
            &ins_sibs,
        )?;

        // ====================================================================
        // 5. Persistent QuoteConsumptionTree Capacity Check & Update
        // ====================================================================
        let quo_idx_num = AllocatedNum::alloc(cs.namespace(|| "quo_idx"), || {
            Ok(Fp::from(wit.map(|w| w.quote_witness.quote_idx as u64).unwrap_or(0)))
        })?;
        let quo_idx_bits =
            range_check_bits_le(cs.namespace(|| "quo_idx_bits"), &quo_idx_num, STATE_TREE_DEPTH)?;

        let quo_max_num = AllocatedNum::alloc(cs.namespace(|| "quo_max"), || {
            Ok(Fp::from(
                wit.map(|w| w.quote_witness.leaf_before.max_out_msat)
                    .unwrap_or(1_000_000_000),
            ))
        })?;
        let quo_prev_used = AllocatedNum::alloc(cs.namespace(|| "quo_prev_used"), || {
            Ok(Fp::from(
                wit.map(|w| w.quote_witness.leaf_before.consumed_out_msat)
                    .unwrap_or(0),
            ))
        })?;
        let quo_delta = AllocatedNum::alloc(cs.namespace(|| "quo_delta"), || {
            Ok(Fp::from(
                wit.map(|w| w.quote_witness.quote_delta_msat)
                    .unwrap_or(100_000),
            ))
        })?;

        let _ = range_check_bits_le(cs.namespace(|| "rc_quo_max"), &quo_max_num, 64)?;
        let _ = range_check_bits_le(cs.namespace(|| "rc_quo_prev"), &quo_prev_used, 64)?;
        let _ = range_check_bits_le(cs.namespace(|| "rc_quo_delta"), &quo_delta, 64)?;
        enforce_nonzero(cs.namespace(|| "quo_delta_pos"), &quo_delta)?;

        // Reconcile `quo_delta` with `gross_btc_num` & `dir_bit`:
        // - Purchase (`dir == 0`): `gross_btc - quo_delta == 0`
        // - Sale (`dir == 1`): `gross_btc - quo_delta >= 0` (`< 2^64`)
        let fee_diff = AllocatedNum::alloc(cs.namespace(|| "fee_diff"), || {
            Ok(gross_btc_num.get_value().ok_or(SynthesisError::AssignmentMissing)?
                - quo_delta.get_value().ok_or(SynthesisError::AssignmentMissing)?)
        })?;
        cs.enforce(
            || "fee_diff_eq",
            |lc| lc + gross_btc_num.get_variable() - quo_delta.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + fee_diff.get_variable(),
        );
        let _ = range_check_bits_le(cs.namespace(|| "rc_fee_diff"), &fee_diff, 64)?;
        // (1 - dir_bit) * fee_diff == 0  => when dir == 0, quo_delta == gross_btc_num!
        cs.enforce(
            || "purchase_exact_quote_delta",
            |lc| lc + CS::one() - &dir_bit.lc(CS::one(), Fp::ONE),
            |lc| lc + fee_diff.get_variable(),
            |lc| lc,
        );

        let quo_new_used = AllocatedNum::alloc(cs.namespace(|| "quo_new_used"), || {
            Ok(quo_prev_used.get_value().ok_or(SynthesisError::AssignmentMissing)?
                + quo_delta.get_value().ok_or(SynthesisError::AssignmentMissing)?)
        })?;
        cs.enforce(
            || "quo_new_used_eq",
            |lc| lc + quo_prev_used.get_variable() + quo_delta.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + quo_new_used.get_variable(),
        );
        let _ = range_check_bits_le(cs.namespace(|| "rc_quo_new"), &quo_new_used, 64)?;

        let quo_headroom = AllocatedNum::alloc(cs.namespace(|| "quo_headroom"), || {
            Ok(quo_max_num.get_value().ok_or(SynthesisError::AssignmentMissing)?
                - quo_new_used.get_value().ok_or(SynthesisError::AssignmentMissing)?)
        })?;
        cs.enforce(
            || "quo_headroom_eq",
            |lc| lc + quo_max_num.get_variable() - quo_new_used.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + quo_headroom.get_variable(),
        );
        // Range-checking headroom < 2^64 proves `quo_new_used <= quo_max_num` without wrap!
        let _ = range_check_bits_le(cs.namespace(|| "rc_quo_headroom"), &quo_headroom, 64)?;

        let quo_before_hash = self.hasher.hash_circuit(
            cs.namespace(|| "quo_before_hash"),
            DOMAIN_QUOTE_CONSUMPTION_LEAF,
            &[
                rfq_hi.clone(),
                rfq_lo.clone(),
                qt_hi.clone(),
                qt_lo.clone(),
                quo_max_num.clone(),
                quo_prev_used,
                one_num.clone(),
            ],
        )?;
        let quo_sibs = alloc_siblings_array(
            cs.namespace(|| "quo_sibs"),
            wit.map(|w| &w.quote_witness.siblings),
        )?;

        let check_quo_before = compute_merkle_root_circuit(
            cs.namespace(|| "check_quo_before"),
            &self.hasher,
            &quo_before_hash,
            &quo_idx_bits,
            &quo_sibs,
        )?;
        cs.enforce(
            || "check_quo_before == z_quo_root",
            |lc| lc + check_quo_before.get_variable() - z_quo_root.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc,
        );

        let quo_after_hash = self.hasher.hash_circuit(
            cs.namespace(|| "quo_after_hash"),
            DOMAIN_QUOTE_CONSUMPTION_LEAF,
            &[
                rfq_hi,
                rfq_lo,
                qt_hi,
                qt_lo,
                quo_max_num,
                quo_new_used,
                one_num.clone(),
            ],
        )?;
        let next_quo_root = compute_merkle_root_circuit(
            cs.namespace(|| "next_quo_root"),
            &self.hasher,
            &quo_after_hash,
            &quo_idx_bits,
            &quo_sibs,
        )?;

        // ====================================================================
        // 6. BatchLog/v1 Hash Chain Update
        // ====================================================================
        let prev_blog_bits = alloc_bytes_be(
            cs.namespace(|| "prev_blog_bits"),
            wit.map(|w| &w.prev_batch_log[..]),
            32,
        )?;
        let (pbl_hi, pbl_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "pack_pbl"), &prev_blog_bits)?;
        cs.enforce(
            || "pbl_hi == z_blog_hi",
            |lc| lc + pbl_hi.get_variable() - z_blog_hi.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc,
        );
        cs.enforce(
            || "pbl_lo == z_blog_lo",
            |lc| lc + pbl_lo.get_variable() - z_blog_lo.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc,
        );

        let mut blog_msg_bits = Vec::with_capacity(512);
        blog_msg_bits.extend_from_slice(&prev_blog_bits);
        blog_msg_bits.extend_from_slice(&digest_bits_be);
        let next_blog_bits = tagged_sha256_circuit(
            cs.namespace(|| "batch_log_hash"),
            TAG_BATCH_LOG_V1,
            &blog_msg_bits,
        )?;
        let (next_blog_hi, next_blog_lo, _) =
            pack_256_be_to_limbs_and_fp(cs.namespace(|| "pack_nbl"), &next_blog_bits)?;

        // ====================================================================
        // 7. Bounded Volume Accumulation & Wrap-Free Exact q8 Half-Up Division
        // ====================================================================
        // S_B' < 2^76 (~750B BTC in msat), S_U' < 2^104 (~2e31 atoms), scale <= 10^19 < 2^64.
        // Product q_raw * S_B' + rem < 2^168 * 2^76 + 2^76 < 2^245 << p (~2^256): ZERO WRAP!
        let next_sum_btc = AllocatedNum::alloc(cs.namespace(|| "next_sum_btc"), || {
            Ok(z_sum_btc.get_value().ok_or(SynthesisError::AssignmentMissing)?
                + gross_btc_num.get_value().ok_or(SynthesisError::AssignmentMissing)?)
        })?;
        cs.enforce(
            || "next_sum_btc_eq",
            |lc| lc + z_sum_btc.get_variable() + gross_btc_num.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + next_sum_btc.get_variable(),
        );
        let _ = range_check_bits_le(cs.namespace(|| "rc_sum_btc_76"), &next_sum_btc, 76)?;
        enforce_nonzero(cs.namespace(|| "sum_btc_pos"), &next_sum_btc)?;

        let next_sum_ast = AllocatedNum::alloc(cs.namespace(|| "next_sum_ast"), || {
            Ok(z_sum_ast.get_value().ok_or(SynthesisError::AssignmentMissing)?
                + asset_atoms_num.get_value().ok_or(SynthesisError::AssignmentMissing)?)
        })?;
        cs.enforce(
            || "next_sum_ast_eq",
            |lc| lc + z_sum_ast.get_variable() + asset_atoms_num.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + next_sum_ast.get_variable(),
        );
        let _ = range_check_bits_le(cs.namespace(|| "rc_sum_ast_104"), &next_sum_ast, 104)?;
        let _ = range_check_bits_le(cs.namespace(|| "rc_scale_64"), z_scale, 64)?;

        let scaled_num = z_scale.mul(cs.namespace(|| "scale_mul_su"), &next_sum_ast)?;

        // Compute exact native Euclidean division `(q_raw, rem, round_up)` when witness is present:
        let div_wit: Option<(u128, u64, u128, bool)> = next_sum_ast
            .get_value()
            .zip(next_sum_btc.get_value())
            .zip(z_scale.get_value())
            .map(|((su_f, sb_f), sc_f)| {
                let fp_to_u128 = |x: Fp| -> u128 {
                    let mut b = [0u8; 16];
                    b.copy_from_slice(&x.to_repr().as_ref()[0..16]);
                    u128::from_le_bytes(b)
                };
                let su = fp_to_u128(su_f);
                let sb = fp_to_u128(sb_f).max(1);
                let sc = fp_to_u128(sc_f);
                // Reconstruct asset_decimals from sc = 10^(19 - d), or use mul_div_rem_u128 directly:
                let (q_raw, rem) = crate::manifest::mul_div_rem_u128(su, sc, sb).unwrap_or((0, 0));
                let round_up = rem.saturating_mul(2) >= sb;
                let q_lo = q_raw;
                let q_hi = 0u64;
                let _ = compute_vwap_q8_half_up; // keep linked
                (q_lo, q_hi, rem, round_up)
            });

        let q_lo = AllocatedNum::alloc(cs.namespace(|| "q_lo"), || {
            Ok(u128_to_scalar(div_wit.map(|w| w.0).unwrap_or(0)))
        })?;
        let q_hi = AllocatedNum::alloc(cs.namespace(|| "q_hi"), || {
            Ok(Fp::from(div_wit.map(|w| w.1).unwrap_or(0)))
        })?;
        let _ = range_check_bits_le(cs.namespace(|| "rc_q_lo_128"), &q_lo, 128)?;
        let _ = range_check_bits_le(cs.namespace(|| "rc_q_hi_40"), &q_hi, 40)?;

        let two_128 = u128_to_scalar::<Fp>(1u128 << 64) * u128_to_scalar::<Fp>(1u128 << 64);
        let q_raw = AllocatedNum::alloc(cs.namespace(|| "q_raw"), || {
            Ok(q_hi.get_value().ok_or(SynthesisError::AssignmentMissing)? * two_128
                + q_lo.get_value().ok_or(SynthesisError::AssignmentMissing)?)
        })?;
        cs.enforce(
            || "q_raw == q_hi*2^128 + q_lo",
            |lc| lc + (two_128, q_hi.get_variable()) + q_lo.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + q_raw.get_variable(),
        );

        let rem_num = AllocatedNum::alloc(cs.namespace(|| "rem_num"), || {
            Ok(u128_to_scalar(div_wit.map(|w| w.2).unwrap_or(0)))
        })?;
        let _ = range_check_bits_le(cs.namespace(|| "rc_rem_76"), &rem_num, 76)?;

        // Enforce `q_raw * next_sum_btc + rem_num == scaled_num` (wrap-free over Z since < 2^245 < p!)
        cs.enforce(
            || "euclidean_division_vwap",
            |lc| lc + q_raw.get_variable(),
            |lc| lc + next_sum_btc.get_variable(),
            |lc| lc + scaled_num.get_variable() - rem_num.get_variable(),
        );

        // Enforce `rem_num < next_sum_btc` via `next_sum_btc - rem_num - 1 < 2^76`
        let rem_lt_diff = AllocatedNum::alloc(cs.namespace(|| "rem_lt_diff"), || {
            Ok(next_sum_btc.get_value().ok_or(SynthesisError::AssignmentMissing)?
                - rem_num.get_value().ok_or(SynthesisError::AssignmentMissing)?
                - Fp::ONE)
        })?;
        cs.enforce(
            || "rem_lt_diff_eq",
            |lc| lc + next_sum_btc.get_variable() - rem_num.get_variable() - CS::one(),
            |lc| lc + CS::one(),
            |lc| lc + rem_lt_diff.get_variable(),
        );
        let _ = range_check_bits_le(cs.namespace(|| "rc_rem_lt_sb"), &rem_lt_diff, 76)?;

        // Half-up rounding: `round_up == [2 * rem_num >= next_sum_btc]`
        let round_up = Boolean::from(AllocatedBit::alloc(
            cs.namespace(|| "round_up"),
            div_wit.map(|w| w.3),
        )?);
        // When round_up == 1: tie_diff = 2*rem - S_B >= 0 (< 2^77)
        // When round_up == 0: tie_diff = S_B - 1 - 2*rem >= 0 (< 2^77)
        // Unified algebraic relation:
        //   round_up * (4*rem - 2*S_B + 1) == tie_diff - (S_B - 1 - 2*rem)
        let tie_diff = AllocatedNum::alloc(cs.namespace(|| "tie_diff"), || {
            let r = rem_num.get_value().ok_or(SynthesisError::AssignmentMissing)?;
            let sb = next_sum_btc.get_value().ok_or(SynthesisError::AssignmentMissing)?;
            let ru = round_up.get_value().ok_or(SynthesisError::AssignmentMissing)?;
            Ok(if ru { r + r - sb } else { sb - Fp::ONE - r - r })
        })?;
        cs.enforce(
            || "half_up_tie_check",
            |lc| lc + &round_up.lc(CS::one(), Fp::ONE),
            |lc| {
                lc + (Fp::from(4u64), rem_num.get_variable())
                    - (Fp::from(2u64), next_sum_btc.get_variable())
                    + CS::one()
            },
            |lc| {
                lc + tie_diff.get_variable() - next_sum_btc.get_variable() + CS::one()
                    + (Fp::from(2u64), rem_num.get_variable())
            },
        );
        let _ = range_check_bits_le(cs.namespace(|| "rc_tie_diff_77"), &tie_diff, 77)?;

        let next_vwap_q8 = AllocatedNum::alloc(cs.namespace(|| "next_vwap_q8"), || {
            let qr = q_raw.get_value().ok_or(SynthesisError::AssignmentMissing)?;
            let ru = round_up.get_value().ok_or(SynthesisError::AssignmentMissing)?;
            Ok(qr + if ru { Fp::ONE } else { Fp::ZERO })
        })?;
        cs.enforce(
            || "next_vwap_q8_eq",
            |lc| lc + q_raw.get_variable() + &round_up.lc(CS::one(), Fp::ONE),
            |lc| lc + CS::one(),
            |lc| lc + next_vwap_q8.get_variable(),
        );

        let next_count = AllocatedNum::alloc(cs.namespace(|| "next_count"), || {
            Ok(z_count.get_value().ok_or(SynthesisError::AssignmentMissing)? + Fp::ONE)
        })?;
        cs.enforce(
            || "next_count_eq",
            |lc| lc + z_count.get_variable() + CS::one(),
            |lc| lc + CS::one(),
            |lc| lc + next_count.get_variable(),
        );

        let next_tier = AllocatedNum::alloc(cs.namespace(|| "next_tier"), || {
            Ok(Fp::from(EVIDENCE_TIER_1_SIGNED_ASSERTION as u64))
        })?;
        cs.enforce(
            || "pin_evidence_tier_1",
            |lc| lc + next_tier.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + (Fp::from(EVIDENCE_TIER_1_SIGNED_ASSERTION as u64), CS::one()),
        );

        Ok(vec![
            z_pol_hi.clone(),
            z_pol_lo.clone(),
            z_gen_hi.clone(),
            z_gen_lo.clone(),
            z_ast_hi.clone(),
            z_ast_lo.clone(),
            z_scale.clone(),
            z_epoch.clone(),
            z_reg_root.clone(),
            next_nul_root,
            next_quo_root,
            next_blog_hi,
            next_blog_lo,
            next_sum_btc,
            next_sum_ast,
            next_count,
            next_vwap_q8,
            next_tier,
        ])
    }
}
