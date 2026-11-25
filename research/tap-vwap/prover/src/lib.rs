//! Isolated `TAP-VWAP-03` Prover & Native Verifier Crate (`tap-vwap-nova-prover`).
//!
//! Kept strictly outside `tradelayer-core` (`Cargo.toml.isolated`) to preserve the
//! zero-dependency consensus crate and pass `scripts/check_rust_supply_chain.py`.

pub mod circuit;
pub mod manifest;
pub mod merkle;
pub mod native_verifier;
pub mod nova_prover;
pub mod secp_gadget;
pub mod sha256_gadget;

#[cfg(test)]
mod tests {
    use halo2curves::secp256k1::Fp;
    use nova_snark::{
        frontend::{num::AllocatedNum, util_cs::test_cs::TestConstraintSystem, ConstraintSystem},
        traits::circuit::StepCircuit,
    };
    use secp256k1::{Keypair, Secp256k1, SecretKey};

    use crate::circuit::{build_initial_z0, expected_zn_from_manifest, TapVwapStepCircuit};
    use crate::manifest::{
        compute_vwap_q8_half_up, tagged_sha256, CanonicalOutpoint, ReceiptV1, ThreePartySignatures,
        TAG_ASSET_HTLC_V1, TAG_RECEIPT_V1,
    };
    use crate::merkle::{
        ChannelRegistryLeaf, IndexedNullifierTree, PoseidonHasher, QuoteConsumptionTree,
    };
    use crate::native_verifier::{
        verify_and_prepare_epoch_native, verify_bip340_libsecp, AuthenticatedChannelRegistry,
        ReceiptStepWitness,
    };
    use crate::secp_gadget::{verify_bip340_circuit, Bip340CircuitTables};
    use crate::sha256_gadget::{alloc_bytes_be, booleans_be_to_bytes, tagged_sha256_circuit};

    fn deterministic_keypair(seed_u64: u64) -> (Keypair, [u8; 32]) {
        let secp = Secp256k1::new();
        let mut sk_bytes = [0u8; 32];
        sk_bytes[24..32].copy_from_slice(&seed_u64.to_be_bytes());
        let sk = SecretKey::from_slice(&sk_bytes).unwrap();
        let kp = Keypair::from_secret_key(&secp, &sk);
        let (xonly, _parity) = kp.x_only_public_key();
        (kp, xonly.serialize())
    }

    fn sign_digest_deterministic(kp: &Keypair, msg32: &[u8; 32]) -> [u8; 64] {
        let secp = Secp256k1::signing_only();
        *secp.sign_schnorr_no_aux_rand(msg32, kp).as_ref()
    }

    #[test]
    fn test_tagged_sha256_midstate_matches_native() {
        let msg = vec![0x5au8; 526];
        let expected = tagged_sha256(TAG_RECEIPT_V1, &msg);

        let mut cs = TestConstraintSystem::<Fp>::new();
        let bits = alloc_bytes_be(cs.namespace(|| "in"), Some(&msg), 526).unwrap();
        let out_bits = tagged_sha256_circuit(cs.namespace(|| "hash"), TAG_RECEIPT_V1, &bits).unwrap();
        assert!(cs.is_satisfied());
        let got = booleans_be_to_bytes(&out_bits).unwrap();
        assert_eq!(got.as_slice(), expected.as_slice());

        let nul_msg = vec![0xa3u8; 77];
        let expected_nul = tagged_sha256(TAG_ASSET_HTLC_V1, &nul_msg);
        let mut cs2 = TestConstraintSystem::<Fp>::new();
        let nbits = alloc_bytes_be(cs2.namespace(|| "in"), Some(&nul_msg), 77).unwrap();
        let nout =
            tagged_sha256_circuit(cs2.namespace(|| "hash"), TAG_ASSET_HTLC_V1, &nbits).unwrap();
        assert!(cs2.is_satisfied());
        assert_eq!(booleans_be_to_bytes(&nout).unwrap().as_slice(), &expected_nul);
    }

    #[test]
    fn test_official_bip340_vectors_in_r1cs_and_golden_fixtures() {
        let raw_json = std::fs::read_to_string("../receipt/golden-fixtures.json")
            .expect("read ../receipt/golden-fixtures.json");
        let parsed: serde_json::Value = serde_json::from_str(&raw_json).unwrap();
        let tables = Bip340CircuitTables::new();

        let vectors = parsed["bip340_official_32b_vectors"].as_array().unwrap();
        assert_eq!(vectors.len(), 15, "must test all 15 official 32B BIP-340 vectors");

        for v in vectors {
            let idx = v["index"].as_u64().unwrap();
            let pk_bytes: [u8; 32] = hex::decode(v["pubkey"].as_str().unwrap())
                .unwrap()
                .try_into()
                .unwrap();
            let msg_bytes: [u8; 32] = hex::decode(v["message"].as_str().unwrap())
                .unwrap()
                .try_into()
                .unwrap();
            let sig_bytes: [u8; 64] = hex::decode(v["signature"].as_str().unwrap())
                .unwrap()
                .try_into()
                .unwrap();
            let exp_valid = v["valid"].as_bool().unwrap();

            // 1. Native libsecp256k1 check
            let native_ok = verify_bip340_libsecp(&pk_bytes, &msg_bytes, &sig_bytes).is_ok();
            assert_eq!(native_ok, exp_valid, "libsecp mismatch on BIP-340 vector {idx}");

            // 2. R1CS TestConstraintSystem check
            let mut cs = TestConstraintSystem::<Fp>::new();
            let pk_bits = alloc_bytes_be(cs.namespace(|| "pk"), Some(&pk_bytes), 32).unwrap();
            let msg_bits = alloc_bytes_be(cs.namespace(|| "msg"), Some(&msg_bytes), 32).unwrap();
            let syn_res = verify_bip340_circuit(
                cs.namespace(|| "bip340"),
                &tables,
                &pk_bits,
                &msg_bits,
                Some(&sig_bytes),
            );
            let r1cs_ok = syn_res.is_ok() && cs.is_satisfied();
            assert_eq!(
                r1cs_ok, exp_valid,
                "R1CS BIP-340 mismatch on vector {idx}: expected {exp_valid}, got {r1cs_ok}"
            );
        }

        // Cross-check frozen 526-byte golden fixtures byte-for-byte
        let fixtures = parsed["receipt_fixtures"].as_array().unwrap();
        assert_eq!(fixtures.len(), 2);
        for fx in fixtures {
            let raw_hex = fx["encoded_hex"].as_str().unwrap();
            let dig_hex = fx["digest_hex"].as_str().unwrap();
            let nul_hex = fx["nullifier_hex"].as_str().unwrap();

            let raw_bytes = hex::decode(raw_hex).unwrap();
            let rec = ReceiptV1::decode(&raw_bytes).expect("decode golden receipt");
            assert_eq!(hex::encode(rec.encode().unwrap()), raw_hex);
            assert_eq!(hex::encode(rec.digest().unwrap()), dig_hex);
            assert_eq!(hex::encode(rec.nullifier().unwrap()), nul_hex);
        }
    }

    #[test]
    fn test_circuit_and_native_adversarial_negative_matrix() {
        let hasher = PoseidonHasher::<Fp>::new();
        let tables = Bip340CircuitTables::new();

        let genesis = [0x11u8; 32];
        let policy = [0x22u8; 32];
        let asset_id = [0x33u8; 32];

        let (edge_kp, edge_x) = deterministic_keypair(101);
        let (btc_kp, btc_x) = deterministic_keypair(102);
        let (asset_kp, asset_x) = deterministic_keypair(103);
        let (rogue_kp, rogue_x) = deterministic_keypair(999);

        let asset_op = CanonicalOutpoint {
            txid_display: [0xa1u8; 32],
            vout: 0,
        };
        let btc_op = CanonicalOutpoint {
            txid_display: [0xb2u8; 32],
            vout: 1,
        };

        let mut registry = AuthenticatedChannelRegistry::<Fp>::new(&hasher);
        registry
            .register_channel(
                &hasher,
                0,
                ChannelRegistryLeaf {
                    txid_display: asset_op.txid_display,
                    vout: asset_op.vout,
                    party0_rec_key: asset_x,
                    party1_rec_key: edge_x,
                    is_asset_channel: true,
                    asset_id,
                    funding_resolved: true,
                },
            )
            .unwrap();
        registry
            .register_channel(
                &hasher,
                1,
                ChannelRegistryLeaf {
                    txid_display: btc_op.txid_display,
                    vout: btc_op.vout,
                    party0_rec_key: edge_x,
                    party1_rec_key: btc_x,
                    is_asset_channel: false,
                    asset_id: [0u8; 32],
                    funding_resolved: true,
                },
            )
            .unwrap();

        let rfq_id = [0x44u8; 32];
        let quote_transcript = [0x55u8; 32];

        let mut nullifiers = IndexedNullifierTree::<Fp>::new(&hasher);
        let mut quotes = QuoteConsumptionTree::<Fp>::new(&hasher);
        quotes
            .register_quote(&hasher, 0, rfq_id, quote_transcript, 600_000_000)
            .unwrap();

        let make_signed_step = |htlc_id: u64, atoms: u64, msat: u64, qdelta: u64| -> ReceiptStepWitness {
            let mut apre = [0x60u8; 32];
            let mut apost = [0x61u8; 32];
            apre[0] = htlc_id as u8;
            apost[0] = (htlc_id as u8).wrapping_add(1);
            let rec = ReceiptV1 {
                genesis,
                policy,
                asset: asset_id,
                edge_key: edge_x,
                btc_peer_key: btc_x,
                asset_peer_key: asset_x,
                rfq_id,
                quote_transcript,
                payment_hash: [0x77u8; 32],
                asset_pre: apre,
                asset_post: apost,
                btc_pre: [0x80u8; 32],
                btc_post: [0x81u8; 32],
                direction: 0, // purchase
                asset_edge_side: 1,
                btc_edge_side: 0,
                asset_outpoint: asset_op,
                asset_offerer: 0,
                asset_htlc: htlc_id,
                btc_outpoint: btc_op,
                btc_offerer: 0,
                btc_htlc: htlc_id + 10,
                asset_atoms: atoms,
                gross_btc_msat: msat,
            };
            let dig = rec.digest().unwrap();
            let sigs = ThreePartySignatures {
                edge: sign_digest_deterministic(&edge_kp, &dig),
                btc_peer: sign_digest_deterministic(&btc_kp, &dig),
                asset_peer: sign_digest_deterministic(&asset_kp, &dig),
            };
            ReceiptStepWitness {
                receipt: rec,
                signatures: sigs,
                asset_chan_idx: 0,
                btc_chan_idx: 1,
                quote_idx: 0,
                quote_delta_msat: qdelta,
            }
        };

        let step1 = make_signed_step(1, 35_000_000, 350_000_000, 350_000_000);

        // Negative 1: Forged edge signature rejected
        let mut forged = step1.clone();
        forged.signatures.edge[63] ^= 1;
        assert!(verify_and_prepare_epoch_native(
            &hasher,
            policy,
            genesis,
            asset_id,
            6,
            1,
            &registry,
            &mut nullifiers.clone(),
            &mut quotes.clone(),
            &[forged],
        )
        .is_err());

        // Negative 2: Rogue peer key not bound in ChannelRegistry rejected even with valid rogue sig!
        let mut rogue_step = step1.clone();
        rogue_step.receipt.asset_peer_key = rogue_x;
        let rogue_dig = rogue_step.receipt.digest().unwrap();
        rogue_step.signatures = ThreePartySignatures {
            edge: sign_digest_deterministic(&edge_kp, &rogue_dig),
            btc_peer: sign_digest_deterministic(&btc_kp, &rogue_dig),
            asset_peer: sign_digest_deterministic(&rogue_kp, &rogue_dig),
        };
        assert!(verify_and_prepare_epoch_native(
            &hasher,
            policy,
            genesis,
            asset_id,
            6,
            1,
            &registry,
            &mut nullifiers.clone(),
            &mut quotes.clone(),
            &[rogue_step],
        )
        .is_err());

        // Negative 3: Unregistered channel slot rejected
        let mut unreg = step1.clone();
        unreg.asset_chan_idx = 5;
        assert!(verify_and_prepare_epoch_native(
            &hasher,
            policy,
            genesis,
            asset_id,
            6,
            1,
            &registry,
            &mut nullifiers.clone(),
            &mut quotes.clone(),
            &[unreg],
        )
        .is_err());

        // Negative 4: Duplicate nullifier in same epoch rejected
        assert!(verify_and_prepare_epoch_native(
            &hasher,
            policy,
            genesis,
            asset_id,
            6,
            1,
            &registry,
            &mut nullifiers.clone(),
            &mut quotes.clone(),
            &[step1.clone(), step1.clone()],
        )
        .is_err());

        // Valid Epoch 1 (`350M` of `600M` quote cap) -> SUCCEEDS in both native AND R1CS!
        let (manifest_e1, prep_e1) = verify_and_prepare_epoch_native(
            &hasher,
            policy,
            genesis,
            asset_id,
            6,
            1,
            &registry,
            &mut nullifiers,
            &mut quotes,
            std::slice::from_ref(&step1),
        )
        .expect("valid epoch 1");
        assert!(!manifest_e1.eligible_for_consensus_index);
        assert_eq!(manifest_e1.evidence_tier, 1);

        // Check R1CS satisfaction on Epoch 1 AND R1CS rejection when tampering output or sig!
        let z0_e1 = build_initial_z0(
            &manifest_e1.policy_hash,
            &manifest_e1.chain_genesis,
            &manifest_e1.asset_id,
            manifest_e1.asset_decimals,
            manifest_e1.epoch_id,
            &manifest_e1.registry_root,
            &manifest_e1.prev_nullifier_root,
            &manifest_e1.prev_quote_root,
        )
        .unwrap();
        let exp_zn_e1 = expected_zn_from_manifest(&manifest_e1).unwrap();

        let mut cs_ok = TestConstraintSystem::<Fp>::new();
        let z_in: Vec<AllocatedNum<Fp>> = z0_e1
            .iter()
            .enumerate()
            .map(|(i, &v)| AllocatedNum::alloc(cs_ok.namespace(|| format!("z_{i}")), || Ok(v)).unwrap())
            .collect();
        let circ_ok = TapVwapStepCircuit::with_witness(hasher.clone(), tables.clone(), prep_e1[0].clone());
        let z_out = circ_ok.synthesize(&mut cs_ok, &z_in).unwrap();
        assert!(cs_ok.is_satisfied(), "unsatisfied: {:?}", cs_ok.which_is_unsatisfied());
        let z_out_vals: Vec<Fp> = z_out.iter().map(|a| a.get_value().unwrap()).collect();
        assert_eq!(z_out_vals, exp_zn_e1);

        // Tamper signature in R1CS witness directly -> TestConstraintSystem MUST reject!
        let mut bad_prep = prep_e1[0].clone();
        bad_prep.signatures.btc_peer[10] ^= 0x80;
        let mut cs_bad = TestConstraintSystem::<Fp>::new();
        let z_in_b: Vec<AllocatedNum<Fp>> = z0_e1
            .iter()
            .enumerate()
            .map(|(i, &v)| AllocatedNum::alloc(cs_bad.namespace(|| format!("z_{i}")), || Ok(v)).unwrap())
            .collect();
        let circ_bad = TapVwapStepCircuit::with_witness(hasher.clone(), tables.clone(), bad_prep);
        let _ = circ_bad.synthesize(&mut cs_bad, &z_in_b);
        assert!(!cs_bad.is_satisfied(), "tampered signature must fail R1CS");

        // Negative 5: Cross-epoch replay of `step1` in Epoch 2 MUST fail (persistent nullifier root!)
        assert!(
            verify_and_prepare_epoch_native(
                &hasher,
                policy,
                genesis,
                asset_id,
                6,
                2,
                &registry,
                &mut nullifiers,
                &mut quotes,
                std::slice::from_ref(&step1),
            )
            .is_err(),
            "cross-epoch nullifier replay must fail"
        );

        // Negative 6: Cross-epoch quote overfill (`350M + 300M = 650M > 600M`) MUST fail!
        let step_over = make_signed_step(2, 30_000_000, 300_000_000, 300_000_000);
        assert!(
            verify_and_prepare_epoch_native(
                &hasher,
                policy,
                genesis,
                asset_id,
                6,
                2,
                &registry,
                &mut nullifiers,
                &mut quotes,
                &[step_over],
            )
            .is_err(),
            "cross-epoch quote overfill must fail"
        );

        // Valid Epoch 2: remaining quote headroom (`350M + 200M = 550M <= 600M`) SUCCEEDS!
        let step2 = make_signed_step(3, 20_000_000, 200_000_000, 200_000_000);
        let (manifest_e2, _prep_e2) = verify_and_prepare_epoch_native(
            &hasher,
            policy,
            genesis,
            asset_id,
            6,
            2,
            &registry,
            &mut nullifiers,
            &mut quotes,
            &[step2],
        )
        .expect("valid epoch 2 remaining quote fill");
        assert_eq!(manifest_e2.prev_nullifier_root, manifest_e1.next_nullifier_root);
        assert_eq!(manifest_e2.prev_quote_root, manifest_e1.next_quote_root);

        // Exact half-up rounding tie boundary check:
        // scale for d=6 is 10^13. Choose S_U = 1, S_B = 20_000_000_000_000 (2 * 10^13):
        // ratio = 10^13 / (2 * 10^13) = 0.5 -> half-up MUST round to 1!
        let (p_tie, q_raw, rem, ru) = compute_vwap_q8_half_up(1, 20_000_000_000_000, 6).unwrap();
        assert_eq!((p_tie, q_raw, rem, ru), (1, 0, 10_000_000_000_000, true));
        // Just below half (`2*10^13 + 1`): 2 * 10^13 < 2*10^13 + 1 -> MUST round to 0!
        let (p_below, _, _, ru_b) = compute_vwap_q8_half_up(1, 20_000_000_000_001, 6).unwrap();
        assert_eq!((p_below, ru_b), (0, false));
    }
}
