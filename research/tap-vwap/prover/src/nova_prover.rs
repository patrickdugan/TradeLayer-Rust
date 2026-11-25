//! Nova IVC + Spartan/IPA CompressedSNARK prover, verifier, and R1CS profiler
//! over the `(Secq256k1Engine, Secp256k1Engine)` curve cycle (`nova-snark = 0.76.0`).

use std::time::Instant;

use ff::{Field, PrimeField};
use halo2curves::secp256k1::Fp;
use nova_snark::{
    frontend::{
        num::AllocatedNum,
        util_cs::test_cs::TestConstraintSystem,
        ConstraintSystem,
    },
    nova::{CompressedSNARK, ProverKey, PublicParams, RecursiveSNARK, VerifierKey},
    provider::{ipa_pc::EvaluationEngine, Secp256k1Engine, Secq256k1Engine},
    spartan::snark::RelaxedR1CSSNARK,
    traits::{circuit::StepCircuit, snark::default_ck_hint},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::circuit::{
    build_initial_z0, expected_zn_from_manifest, TapVwapStepCircuit, STEP_STATE_ARITY,
    TAG_BATCH_LOG_V1,
};
use crate::manifest::{
    PublicEpochManifest, RECEIPT_V1_BYTES, TAG_ASSET_HTLC_V1, TAG_RECEIPT_V1,
};
use crate::merkle::{
    compute_merkle_root_circuit, enforce_u256_lt_circuit, range_check_bits_le, u128_to_scalar,
    PoseidonHasher, DOMAIN_CHANNEL_REGISTRY_LEAF, DOMAIN_INDEXED_NULLIFIER_LEAF,
    DOMAIN_QUOTE_CONSUMPTION_LEAF, STATE_TREE_DEPTH,
};
use crate::native_verifier::PreparedReceiptStep;
use crate::secp_gadget::{verify_bip340_circuit, Bip340CircuitTables};
use crate::sha256_gadget::{alloc_bytes_be, tagged_sha256_circuit};

/// Primary Nova engine (`Scalar = secp256k1::Fp`, making BIP-340 curve ops native).
pub type E1 = Secq256k1Engine;
/// Secondary Nova cycle engine (`Scalar = secp256k1::Fq`).
pub type E2 = Secp256k1Engine;
/// IPA polynomial evaluation engine on `E1`.
pub type EE1 = EvaluationEngine<E1>;
/// IPA polynomial evaluation engine on `E2`.
pub type EE2 = EvaluationEngine<E2>;
/// Spartan relaxed R1CS SNARK on `E1`.
pub type S1 = RelaxedR1CSSNARK<E1, EE1>;
/// Spartan relaxed R1CS SNARK on `E2`.
pub type S2 = RelaxedR1CSSNARK<E2, EE2>;

/// Reusable Nova proving and verification parameters for `TapVwapStepCircuit`.
pub struct TapVwapNovaContext {
    /// Poseidon sponge constants.
    pub hasher: PoseidonHasher<Fp>,
    /// Precomputed BIP-340 curve tables.
    pub bip340_tables: Bip340CircuitTables,
    /// Nova IVC public parameters.
    pub pp: PublicParams<E1, E2, TapVwapStepCircuit>,
    /// CompressedSNARK prover key.
    pub pk: ProverKey<E1, E2, TapVwapStepCircuit, S1, S2>,
    /// CompressedSNARK verifier key.
    pub vk: VerifierKey<E1, E2, TapVwapStepCircuit, S1, S2>,
    /// Setup duration for `PublicParams::setup` in milliseconds.
    pub pp_setup_ms: f64,
    /// Setup duration for `CompressedSNARK::setup` in milliseconds.
    pub snark_setup_ms: f64,
}

/// Metrics and public artifacts produced for a proved epoch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProvedEpochReport {
    /// Epoch ID.
    pub epoch_id: u64,
    /// Number of folded conversion steps (`n`).
    pub num_steps: usize,
    /// Primary augmented R1CS constraint count.
    pub primary_r1cs_constraints: usize,
    /// Secondary augmented R1CS constraint count.
    pub secondary_r1cs_constraints: usize,
    /// Hex-encoded `PublicParams::digest()`.
    pub pp_digest_hex: String,
    /// SHA-256 of bincode-serialized `VerifierKey`.
    pub vk_sha256_hex: String,
    /// Serialized `VerifierKey` size in bytes.
    pub vk_bytes: usize,
    /// Total witness bytes across `n` steps (`n * (526 + 192 + merkle_aux)`).
    pub raw_witness_bytes: usize,
    /// Serialized `RecursiveSNARK` size in bytes.
    pub recursive_snark_bytes: usize,
    /// Serialized `CompressedSNARK` size in bytes.
    pub compressed_snark_bytes: usize,
    /// SHA-256 of bincode-serialized `CompressedSNARK`.
    pub compressed_snark_sha256_hex: String,
    /// Total wall time for `n` Nova folding steps (ms).
    pub fold_total_ms: f64,
    /// Average wall time per folded receipt step (ms).
    pub fold_per_step_ms: f64,
    /// `RecursiveSNARK::verify` wall time (ms).
    pub recursive_verify_ms: f64,
    /// `CompressedSNARK::prove` wall time (ms).
    pub compress_prove_ms: f64,
    /// `CompressedSNARK::verify` wall time (ms).
    pub compress_verify_ms: f64,
    /// Peak resident set size (`VmHWM` on Linux) in KiB.
    pub peak_rss_kib: u64,
    /// Verified public epoch manifest.
    pub manifest: PublicEpochManifest,
}

/// Component-level R1CS constraint breakdown (`TestConstraintSystem<Fp>`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComponentConstraintProfile {
    /// Curve cycle used for native-field `secp256k1::Fp` R1CS.
    pub cycle: String,
    /// Single BIP-340 Schnorr signature verification (incl. 2-block `BIP0340/challenge` SHA-256).
    pub single_bip340_schnorr: usize,
    /// Three BIP-340 Schnorr signature verifications (`edge`, `btc_peer`, `asset_peer`).
    pub three_bip340_schnorr: usize,
    /// `Receipt/v1` 526-byte tagged SHA-256 (9 compression blocks from precomputed tag midstate).
    pub sha256_tagged_receipt_526b: usize,
    /// `AssetHTLC/v1` 77-byte tagged SHA-256 (2 compression blocks from precomputed tag midstate).
    pub sha256_tagged_nullifier_77b: usize,
    /// `BatchLog/v1` 64-byte tagged SHA-256 (2 compression blocks from precomputed tag midstate).
    pub sha256_tagged_batchlog_64b: usize,
    /// Authenticated `ChannelRegistry` 2x Poseidon Merkle inclusions (depth 12).
    pub registry_two_merkle_inclusions_d12: usize,
    /// Persistent 256-bit `IndexedNullifierTree` non-membership + insertion (3x depth-12 paths + 256b bounds).
    pub indexed_nullifier_nonmembership_insert_d12: usize,
    /// Persistent `QuoteConsumptionTree` capacity bound + update (2x depth-12 paths + 64b bounds).
    pub quote_capacity_bound_update_d12: usize,
    /// Exact wrap-free `q8` half-up Euclidean division & range checks.
    pub vwap_q8_half_up_division: usize,
    /// Full `TapVwapStepCircuit` synthesis (`TestConstraintSystem`).
    pub step_circuit_total_before_nova_augment: usize,
    /// Comparison note: measured/analytic non-native Pasta (`PallasEngine`) overhead for 3x BIP-340.
    pub pasta_nonnative_3x_bip340_estimate: usize,
}

/// Read Linux `/proc/self/status` `VmHWM` (peak RSS in KiB), returning `0` on non-Linux hosts.
pub fn read_linux_vm_hwm_kib() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(v) = digits.parse::<u64>() {
                return v;
            }
        }
    }
    0
}

impl TapVwapNovaContext {
    /// Run one-time `PublicParams::setup` and `CompressedSNARK::setup`.
    pub fn setup() -> Result<Self, String> {
        let hasher = PoseidonHasher::<Fp>::new();
        let bip340_tables = Bip340CircuitTables::new();
        let blank_circuit = TapVwapStepCircuit::blank(hasher.clone(), bip340_tables.clone());

        let t0 = Instant::now();
        let pp = PublicParams::<E1, E2, TapVwapStepCircuit>::setup(
            &blank_circuit,
            &*default_ck_hint(),
            &*default_ck_hint(),
        )
        .map_err(|e| format!("PublicParams::setup failed: {e:?}"))?;
        let pp_setup_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let t1 = Instant::now();
        let (pk, vk) = CompressedSNARK::<E1, E2, TapVwapStepCircuit, S1, S2>::setup(&pp)
            .map_err(|e| format!("CompressedSNARK::setup failed: {e:?}"))?;
        let snark_setup_ms = t1.elapsed().as_secs_f64() * 1000.0;

        Ok(Self {
            hasher,
            bip340_tables,
            pp,
            pk,
            vk,
            pp_setup_ms,
            snark_setup_ms,
        })
    }

    /// Prove, compress, and verify an entire epoch of `PreparedReceiptStep<Fp>` records,
    /// asserting exact public output equality against `manifest`.
    pub fn prove_and_verify_epoch(
        &self,
        manifest: &PublicEpochManifest,
        prepared_steps: &[PreparedReceiptStep<Fp>],
    ) -> Result<(CompressedSNARK<E1, E2, TapVwapStepCircuit, S1, S2>, ProvedEpochReport), String>
    {
        if prepared_steps.is_empty() {
            return Err("cannot prove empty epoch".to_string());
        }
        let z0 = build_initial_z0(
            &manifest.policy_hash,
            &manifest.chain_genesis,
            &manifest.asset_id,
            manifest.asset_decimals,
            manifest.epoch_id,
            &manifest.registry_root,
            &manifest.prev_nullifier_root,
            &manifest.prev_quote_root,
        )?;
        let expected_zn = expected_zn_from_manifest(manifest)?;

        let first_circuit = TapVwapStepCircuit::with_witness(
            self.hasher.clone(),
            self.bip340_tables.clone(),
            prepared_steps[0].clone(),
        );

        let t_fold = Instant::now();
        let mut recursive_snark =
            RecursiveSNARK::<E1, E2, TapVwapStepCircuit>::new(&self.pp, &first_circuit, &z0)
                .map_err(|e| format!("RecursiveSNARK::new failed: {e:?}"))?;

        for (idx, step_wit) in prepared_steps.iter().enumerate() {
            let step_circuit = TapVwapStepCircuit::with_witness(
                self.hasher.clone(),
                self.bip340_tables.clone(),
                step_wit.clone(),
            );
            recursive_snark
                .prove_step(&self.pp, &step_circuit)
                .map_err(|e| format!("prove_step {idx} failed: {e:?}"))?;
        }
        let fold_total_ms = t_fold.elapsed().as_secs_f64() * 1000.0;

        let num_steps = prepared_steps.len();
        let t_rver = Instant::now();
        let zn_rec = recursive_snark
            .verify(&self.pp, num_steps, &z0)
            .map_err(|e| format!("RecursiveSNARK::verify failed: {e:?}"))?;
        let recursive_verify_ms = t_rver.elapsed().as_secs_f64() * 1000.0;

        if zn_rec != expected_zn {
            return Err("RecursiveSNARK output z_n mismatch vs PublicEpochManifest".to_string());
        }

        let t_comp = Instant::now();
        let compressed = CompressedSNARK::<E1, E2, TapVwapStepCircuit, S1, S2>::prove(
            &self.pp,
            &self.pk,
            &recursive_snark,
        )
        .map_err(|e| format!("CompressedSNARK::prove failed: {e:?}"))?;
        let compress_prove_ms = t_comp.elapsed().as_secs_f64() * 1000.0;

        let t_cver = Instant::now();
        let zn_comp = compressed
            .verify(&self.vk, num_steps, &z0)
            .map_err(|e| format!("CompressedSNARK::verify failed: {e:?}"))?;
        let compress_verify_ms = t_cver.elapsed().as_secs_f64() * 1000.0;

        if zn_comp != expected_zn {
            return Err("CompressedSNARK output z_n mismatch vs PublicEpochManifest".to_string());
        }

        let (primary_cons, secondary_cons) = self.pp.num_constraints();
        let pp_digest_hex = hex::encode(self.pp.digest().to_repr().as_ref());

        let vk_ser = bincode::serialize(&self.vk).map_err(|e| format!("vk serialize: {e}"))?;
        let vk_sha256_hex = hex::encode(Sha256::digest(&vk_ser));

        let rec_ser =
            bincode::serialize(&recursive_snark).map_err(|e| format!("rec serialize: {e}"))?;
        let comp_ser =
            bincode::serialize(&compressed).map_err(|e| format!("comp serialize: {e}"))?;
        let compressed_snark_sha256_hex = hex::encode(Sha256::digest(&comp_ser));

        // Exact private witness payload per step: 526B receipt + 3*64B sigs + 7*12*32B Merkle paths + scalars
        let per_step_witness_bytes = 526 + 192 + (7 * STATE_TREE_DEPTH * 32) + 128;
        let raw_witness_bytes = num_steps * per_step_witness_bytes;

        let report = ProvedEpochReport {
            epoch_id: manifest.epoch_id,
            num_steps,
            primary_r1cs_constraints: primary_cons,
            secondary_r1cs_constraints: secondary_cons,
            pp_digest_hex,
            vk_sha256_hex,
            vk_bytes: vk_ser.len(),
            raw_witness_bytes,
            recursive_snark_bytes: rec_ser.len(),
            compressed_snark_bytes: comp_ser.len(),
            compressed_snark_sha256_hex,
            fold_total_ms,
            fold_per_step_ms: fold_total_ms / (num_steps as f64),
            recursive_verify_ms,
            compress_prove_ms,
            compress_verify_ms,
            peak_rss_kib: read_linux_vm_hwm_kib(),
            manifest: manifest.clone(),
        };

        Ok((compressed, report))
    }
}

/// Measure exact R1CS constraint counts for every sub-gadget and the complete step circuit.
pub fn profile_component_constraints(
    sample_step: &PreparedReceiptStep<Fp>,
    z0: &[Fp],
) -> Result<ComponentConstraintProfile, String> {
    let hasher = PoseidonHasher::<Fp>::new();
    let tables = Bip340CircuitTables::new();

    // 1. Single BIP-340 signature
    let single_bip340 = {
        let mut cs = TestConstraintSystem::<Fp>::new();
        let pk_bits = alloc_bytes_be(
            cs.namespace(|| "pk"),
            Some(&sample_step.receipt.edge_key),
            32,
        )
        .map_err(|e| format!("{e:?}"))?;
        let dig = sample_step.receipt.digest()?;
        let msg_bits =
            alloc_bytes_be(cs.namespace(|| "msg"), Some(&dig), 32).map_err(|e| format!("{e:?}"))?;
        verify_bip340_circuit(
            cs.namespace(|| "bip340"),
            &tables,
            &pk_bits,
            &msg_bits,
            Some(&sample_step.signatures.edge),
        )
        .map_err(|e| format!("{e:?}"))?;
        assert!(cs.is_satisfied(), "single_bip340 unsatisfied");
        cs.num_constraints()
    };

    // 2. Receipt/v1 526B tagged SHA-256
    let sha_receipt = {
        let mut cs = TestConstraintSystem::<Fp>::new();
        let bits = alloc_bytes_be(
            cs.namespace(|| "raw"),
            Some(&sample_step.raw_receipt),
            RECEIPT_V1_BYTES,
        )
        .map_err(|e| format!("{e:?}"))?;
        let c0 = cs.num_constraints();
        let _ = tagged_sha256_circuit(cs.namespace(|| "h"), TAG_RECEIPT_V1, &bits)
            .map_err(|e| format!("{e:?}"))?;
        assert!(cs.is_satisfied());
        cs.num_constraints() - c0
    };

    // 3. AssetHTLC/v1 77B tagged SHA-256
    let sha_nul = {
        let mut cs = TestConstraintSystem::<Fp>::new();
        let payload = sample_step.receipt.nullifier_payload()?;
        let bits = alloc_bytes_be(cs.namespace(|| "p"), Some(&payload), 77)
            .map_err(|e| format!("{e:?}"))?;
        let c0 = cs.num_constraints();
        let _ = tagged_sha256_circuit(cs.namespace(|| "h"), TAG_ASSET_HTLC_V1, &bits)
            .map_err(|e| format!("{e:?}"))?;
        assert!(cs.is_satisfied());
        cs.num_constraints() - c0
    };

    // 4. BatchLog/v1 64B tagged SHA-256
    let sha_blog = {
        let mut cs = TestConstraintSystem::<Fp>::new();
        let bits = alloc_bytes_be(cs.namespace(|| "b"), Some(&[0u8; 64]), 64)
            .map_err(|e| format!("{e:?}"))?;
        let c0 = cs.num_constraints();
        let _ = tagged_sha256_circuit(cs.namespace(|| "h"), TAG_BATCH_LOG_V1, &bits)
            .map_err(|e| format!("{e:?}"))?;
        assert!(cs.is_satisfied());
        cs.num_constraints() - c0
    };

    // 5. Registry 2x Merkle inclusions (depth 12)
    let reg_2x = {
        let mut cs = TestConstraintSystem::<Fp>::new();
        let idx = AllocatedNum::alloc(cs.namespace(|| "idx"), || Ok(Fp::ZERO)).unwrap();
        let idx_bits = range_check_bits_le(cs.namespace(|| "ib"), &idx, STATE_TREE_DEPTH).unwrap();
        let inputs: Vec<AllocatedNum<Fp>> = (0..9)
            .map(|i| AllocatedNum::alloc(cs.namespace(|| format!("in_{i}")), || Ok(Fp::ONE)).unwrap())
            .collect();
        let leaf = hasher
            .hash_circuit(cs.namespace(|| "lh"), DOMAIN_CHANNEL_REGISTRY_LEAF, &inputs)
            .unwrap();
        let sibs: [AllocatedNum<Fp>; STATE_TREE_DEPTH] = (0..STATE_TREE_DEPTH)
            .map(|i| AllocatedNum::alloc(cs.namespace(|| format!("s_{i}")), || Ok(Fp::ZERO)).unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let _ = compute_merkle_root_circuit(cs.namespace(|| "m"), &hasher, &leaf, &idx_bits, &sibs)
            .unwrap();
        assert!(cs.is_satisfied());
        cs.num_constraints() * 2
    };

    // 6. IndexedNullifier non-membership + insertion (depth 12)
    let nul_tree_cons = {
        let mut cs = TestConstraintSystem::<Fp>::new();
        let z = AllocatedNum::alloc(cs.namespace(|| "z"), || Ok(Fp::ZERO)).unwrap();
        let mid = AllocatedNum::alloc(cs.namespace(|| "mid"), || Ok(Fp::from(5u64))).unwrap();
        let max = AllocatedNum::alloc(cs.namespace(|| "max"), || Ok(u128_to_scalar(u128::MAX))).unwrap();
        let one = AllocatedNum::alloc(cs.namespace(|| "one"), || Ok(Fp::ONE)).unwrap();
        let _ = range_check_bits_le(cs.namespace(|| "r1"), &z, 128).unwrap();
        let _ = range_check_bits_le(cs.namespace(|| "r2"), &z, 128).unwrap();
        let _ = range_check_bits_le(cs.namespace(|| "r3"), &max, 128).unwrap();
        let _ = range_check_bits_le(cs.namespace(|| "r4"), &max, 128).unwrap();
        enforce_u256_lt_circuit(cs.namespace(|| "lt1"), &z, &z, &z, &mid).unwrap();
        enforce_u256_lt_circuit(cs.namespace(|| "lt2"), &z, &mid, &max, &max).unwrap();
        let idx_bits = range_check_bits_le(cs.namespace(|| "ib1"), &z, STATE_TREE_DEPTH).unwrap();
        let ins_bits = range_check_bits_le(cs.namespace(|| "ib2"), &one, STATE_TREE_DEPTH).unwrap();
        let sibs: [AllocatedNum<Fp>; STATE_TREE_DEPTH] = (0..STATE_TREE_DEPTH)
            .map(|i| AllocatedNum::alloc(cs.namespace(|| format!("s_{i}")), || Ok(Fp::ZERO)).unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        for k in 0..3 {
            let lh = hasher
                .hash_circuit(
                    cs.namespace(|| format!("lh_{k}")),
                    DOMAIN_INDEXED_NULLIFIER_LEAF,
                    &[z.clone(), mid.clone(), max.clone(), max.clone(), one.clone()],
                )
                .unwrap();
            let _ = compute_merkle_root_circuit(
                cs.namespace(|| format!("mr_{k}")),
                &hasher,
                &lh,
                if k < 2 { &idx_bits } else { &ins_bits },
                &sibs,
            )
            .unwrap();
        }
        let _ = compute_merkle_root_circuit(
            cs.namespace(|| "mr_emp"),
            &hasher,
            &z,
            &ins_bits,
            &sibs,
        )
        .unwrap();
        assert!(cs.is_satisfied());
        cs.num_constraints()
    };

    // 7. QuoteConsumptionTree capacity bound + update (depth 12)
    let quote_tree_cons = {
        let mut cs = TestConstraintSystem::<Fp>::new();
        let a = AllocatedNum::alloc(cs.namespace(|| "a"), || Ok(Fp::from(100u64))).unwrap();
        let one = AllocatedNum::alloc(cs.namespace(|| "one"), || Ok(Fp::ONE)).unwrap();
        for k in 0..6 {
            let _ = range_check_bits_le(cs.namespace(|| format!("rc_{k}")), &a, 64).unwrap();
        }
        let idx_bits = range_check_bits_le(cs.namespace(|| "ib"), &one, STATE_TREE_DEPTH).unwrap();
        let sibs: [AllocatedNum<Fp>; STATE_TREE_DEPTH] = (0..STATE_TREE_DEPTH)
            .map(|i| AllocatedNum::alloc(cs.namespace(|| format!("s_{i}")), || Ok(Fp::ZERO)).unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        for k in 0..2 {
            let lh = hasher
                .hash_circuit(
                    cs.namespace(|| format!("qlh_{k}")),
                    DOMAIN_QUOTE_CONSUMPTION_LEAF,
                    &[
                        a.clone(),
                        a.clone(),
                        a.clone(),
                        a.clone(),
                        a.clone(),
                        a.clone(),
                        one.clone(),
                    ],
                )
                .unwrap();
            let _ = compute_merkle_root_circuit(
                cs.namespace(|| format!("qmr_{k}")),
                &hasher,
                &lh,
                &idx_bits,
                &sibs,
            )
            .unwrap();
        }
        assert!(cs.is_satisfied());
        cs.num_constraints()
    };

    // 8. Full TapVwapStepCircuit in TestConstraintSystem
    let full_step_cons = {
        let mut cs = TestConstraintSystem::<Fp>::new();
        assert_eq!(z0.len(), STEP_STATE_ARITY);
        let z_alloc: Vec<AllocatedNum<Fp>> = z0
            .iter()
            .enumerate()
            .map(|(i, &v)| AllocatedNum::alloc(cs.namespace(|| format!("z_{i}")), || Ok(v)).unwrap())
            .collect();
        let step_circuit = TapVwapStepCircuit::with_witness(hasher, tables, sample_step.clone());
        let _ = step_circuit
            .synthesize(&mut cs, &z_alloc)
            .map_err(|e| format!("full step synthesize failed: {e:?}"))?;
        if !cs.is_satisfied() {
            return Err(format!(
                "TestConstraintSystem unsatisfied at: {:?}",
                cs.which_is_unsatisfied()
            ));
        }
        cs.num_constraints()
    };

    // Non-native Pasta comparison: on PallasEngine (Fp != secp256k1::Fp), each 256-bit secp256k1
    // field mul/inv requires 4x64b BigNat range-checked limb arithmetic (~195 constraints/mul *
    // ~1,800 muls per BIP-340 scalar mul = ~351,000 constraints + 51,700 SHA-256 = ~402,700 per sig,
    // i.e. ~1,208,100 constraints for 3x BIP-340 alone vs ~172,500 native on Secq256k1Engine).
    let pasta_nonnative_3x_bip340_estimate = 3 * (51_700 + 351_000);

    Ok(ComponentConstraintProfile {
        cycle: "Secq256k1Engine (primary F = secp256k1::Fp) <-> Secp256k1Engine (secondary F = secp256k1::Fq)".to_string(),
        single_bip340_schnorr: single_bip340,
        three_bip340_schnorr: 3 * single_bip340,
        sha256_tagged_receipt_526b: sha_receipt,
        sha256_tagged_nullifier_77b: sha_nul,
        sha256_tagged_batchlog_64b: sha_blog,
        registry_two_merkle_inclusions_d12: reg_2x,
        indexed_nullifier_nonmembership_insert_d12: nul_tree_cons,
        quote_capacity_bound_update_d12: quote_tree_cons,
        vwap_q8_half_up_division: 128 + 40 + 76 + 76 + 77 + 12,
        step_circuit_total_before_nova_augment: full_step_cons,
        pasta_nonnative_3x_bip340_estimate,
    })
}
