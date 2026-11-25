//! CLI benchmark & epoch prover for `TAP-VWAP-03` (`tap-vwap-prover`).
//!
//! Usage:
//!   tap-vwap-prover [--regtest-witness <private-regtest-receipts.json>] [--out <benchmarks.json>]

use std::collections::HashMap;
use std::env;

use ff::Field;
use halo2curves::secp256k1::Fp;
use secp256k1::{Keypair, Secp256k1, SecretKey};
use serde::Serialize;

use tap_vwap_nova_prover::{
    circuit::build_initial_z0,
    manifest::{CanonicalOutpoint, ReceiptV1, ThreePartySignatures},
    merkle::{ChannelRegistryLeaf, IndexedNullifierTree, PoseidonHasher, QuoteConsumptionTree},
    native_verifier::{
        verify_and_prepare_epoch_native, AuthenticatedChannelRegistry, ReceiptStepWitness,
    },
    nova_prover::{
        profile_component_constraints, read_linux_vm_hwm_kib, ComponentConstraintProfile,
        ProvedEpochReport, TapVwapNovaContext,
    },
};

#[derive(Serialize)]
struct FullBenchmarkOutput {
    stage: &'static str,
    prover_crate: &'static str,
    nova_snark_version: &'static str,
    secp256k1_version: &'static str,
    rust_profile: &'static str,
    evidence_tier: u8,
    eligible_for_consensus_index: bool,
    pp_setup_ms: f64,
    compressed_snark_setup_ms: f64,
    peak_rss_kib: u64,
    component_constraints: ComponentConstraintProfile,
    synthetic_cross_epoch_proofs: Vec<ProvedEpochReport>,
    live_regtest_epoch_proof: Option<ProvedEpochReport>,
}

fn det_keypair(seed_u64: u64) -> (Keypair, [u8; 32]) {
    let secp = Secp256k1::new();
    let mut sk_bytes = [0u8; 32];
    sk_bytes[24..32].copy_from_slice(&seed_u64.to_be_bytes());
    let sk = SecretKey::from_slice(&sk_bytes).expect("valid sk");
    let kp = Keypair::from_secret_key(&secp, &sk);
    let (xonly, _) = kp.x_only_public_key();
    (kp, xonly.serialize())
}

fn det_sign(kp: &Keypair, msg32: &[u8; 32]) -> [u8; 64] {
    let secp = Secp256k1::signing_only();
    *secp.sign_schnorr_no_aux_rand(msg32, kp).as_ref()
}

fn parse_hex32(s: &str) -> Result<[u8; 32], String> {
    let b = hex::decode(s).map_err(|e| format!("hex32: {e}"))?;
    b.try_into().map_err(|_| "expected 32 bytes".to_string())
}

fn parse_hex64(s: &str) -> Result<[u8; 64], String> {
    let b = hex::decode(s).map_err(|e| format!("hex64: {e}"))?;
    b.try_into().map_err(|_| "expected 64 bytes".to_string())
}

fn prove_regtest_witness_file(
    ctx: &TapVwapNovaContext,
    path: &str,
) -> Result<ProvedEpochReport, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read regtest witness: {e}"))?;
    let val: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse regtest witness: {e}"))?;

    let genesis = parse_hex32(val["genesis"].as_str().ok_or("missing genesis")?)?;
    let policy = parse_hex32(val["policy"].as_str().ok_or("missing policy")?)?;
    let asset_id = parse_hex32(val["asset_id"].as_str().ok_or("missing asset_id")?)?;
    let items = val["receipts"].as_array().ok_or("missing receipts array")?;
    if items.is_empty() {
        return Err("empty regtest receipts".to_string());
    }

    let mut registry = AuthenticatedChannelRegistry::<Fp>::new(&ctx.hasher);
    let mut chan_map: HashMap<([u8; 32], u32), usize> = HashMap::new();
    let mut next_chan_idx = 0usize;

    let mut quotes = QuoteConsumptionTree::<Fp>::new(&ctx.hasher);
    let mut quote_map: HashMap<[u8; 32], usize> = HashMap::new();
    let mut next_quote_idx = 0usize;

    let mut step_witnesses = Vec::with_capacity(items.len());

    for (idx, item) in items.iter().enumerate() {
        let raw_hex = item["encoded_hex"]
            .as_str()
            .ok_or_else(|| format!("item {idx} missing encoded_hex"))?;
        let raw_bytes = hex::decode(raw_hex).map_err(|e| format!("{e}"))?;
        let rec = ReceiptV1::decode(&raw_bytes)?;

        let sigs_obj = &item["signatures"];
        let signatures = ThreePartySignatures {
            edge: parse_hex64(sigs_obj["edge"].as_str().ok_or("missing edge sig")?)?,
            btc_peer: parse_hex64(sigs_obj["btcPeer"].as_str().ok_or("missing btcPeer sig")?)?,
            asset_peer: parse_hex64(
                sigs_obj["assetPeer"]
                    .as_str()
                    .ok_or("missing assetPeer sig")?,
            )?,
        };

        let quote_max = item["quote_max_out_msat"]
            .as_u64()
            .ok_or("missing quote_max_out_msat")?;
        let quote_used = item["quote_used_msat"]
            .as_u64()
            .ok_or("missing quote_used_msat")?;

        // Register asset channel if not already registered
        let a_key = (rec.asset_outpoint.txid_display, rec.asset_outpoint.vout);
        let asset_chan_idx = if let Some(&cidx) = chan_map.get(&a_key) {
            cidx
        } else {
            let cidx = next_chan_idx;
            next_chan_idx += 1;
            let (p0, p1) = if rec.asset_edge_side == 0 {
                (rec.edge_key, rec.asset_peer_key)
            } else {
                (rec.asset_peer_key, rec.edge_key)
            };
            registry.register_channel(
                &ctx.hasher,
                cidx,
                ChannelRegistryLeaf {
                    txid_display: rec.asset_outpoint.txid_display,
                    vout: rec.asset_outpoint.vout,
                    party0_rec_key: p0,
                    party1_rec_key: p1,
                    is_asset_channel: true,
                    asset_id,
                    funding_resolved: true,
                },
            )?;
            chan_map.insert(a_key, cidx);
            cidx
        };

        // Register BTC channel if not already registered
        let b_key = (rec.btc_outpoint.txid_display, rec.btc_outpoint.vout);
        let btc_chan_idx = if let Some(&cidx) = chan_map.get(&b_key) {
            cidx
        } else {
            let cidx = next_chan_idx;
            next_chan_idx += 1;
            let (p0, p1) = if rec.btc_edge_side == 0 {
                (rec.edge_key, rec.btc_peer_key)
            } else {
                (rec.btc_peer_key, rec.edge_key)
            };
            registry.register_channel(
                &ctx.hasher,
                cidx,
                ChannelRegistryLeaf {
                    txid_display: rec.btc_outpoint.txid_display,
                    vout: rec.btc_outpoint.vout,
                    party0_rec_key: p0,
                    party1_rec_key: p1,
                    is_asset_channel: false,
                    asset_id: [0u8; 32],
                    funding_resolved: true,
                },
            )?;
            chan_map.insert(b_key, cidx);
            cidx
        };

        // Register quote slot if first time seeing this rfq_id
        let quote_idx = if let Some(&qidx) = quote_map.get(&rec.rfq_id) {
            qidx
        } else {
            let qidx = next_quote_idx;
            next_quote_idx += 1;
            quotes.register_quote(
                &ctx.hasher,
                qidx,
                rec.rfq_id,
                rec.quote_transcript,
                quote_max,
            )?;
            quote_map.insert(rec.rfq_id, qidx);
            qidx
        };

        step_witnesses.push(ReceiptStepWitness {
            receipt: rec,
            signatures,
            asset_chan_idx,
            btc_chan_idx,
            quote_idx,
            quote_delta_msat: quote_used,
        });
    }

    let mut nullifiers = IndexedNullifierTree::<Fp>::new(&ctx.hasher);
    let (manifest, prepared) = verify_and_prepare_epoch_native(
        &ctx.hasher,
        policy,
        genesis,
        asset_id,
        6, // d = 6 decimals (q8 scale = 10^13)
        1,
        &registry,
        &mut nullifiers,
        &mut quotes,
        &step_witnesses,
    )?;

    let (_comp, report) = ctx.prove_and_verify_epoch(&manifest, &prepared)?;
    Ok(report)
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let mut regtest_path: Option<String> = None;
    let mut out_path: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--regtest-witness" => {
                i += 1;
                regtest_path = Some(args.get(i).ok_or("missing path")?.clone());
            }
            "--out" => {
                i += 1;
                out_path = Some(args.get(i).ok_or("missing out path")?.clone());
            }
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }

    // Build deterministic synthetic multi-epoch fixtures (Charlie <-> Dave <-> Erin <-> Fabia)
    let hasher = PoseidonHasher::<Fp>::new();
    let genesis = [0x06u8; 32];
    let policy = [0x17u8; 32];
    let asset_id = [0x28u8; 32];

    let (charlie_kp, charlie_x) = det_keypair(201);
    let (dave_kp, dave_x) = det_keypair(202);
    let (erin_kp, erin_x) = det_keypair(203);
    let (fabia_kp, fabia_x) = det_keypair(204);

    let op_cd = CanonicalOutpoint {
        txid_display: [0xc1u8; 32],
        vout: 0,
    };
    let op_de = CanonicalOutpoint {
        txid_display: [0xd2u8; 32],
        vout: 1,
    };
    let op_ef = CanonicalOutpoint {
        txid_display: [0xe3u8; 32],
        vout: 0,
    };

    let mut registry = AuthenticatedChannelRegistry::<Fp>::new(&hasher);
    registry.register_channel(
        &hasher,
        0,
        ChannelRegistryLeaf {
            txid_display: op_cd.txid_display,
            vout: op_cd.vout,
            party0_rec_key: charlie_x,
            party1_rec_key: dave_x,
            is_asset_channel: true,
            asset_id,
            funding_resolved: true,
        },
    )?;
    registry.register_channel(
        &hasher,
        1,
        ChannelRegistryLeaf {
            txid_display: op_de.txid_display,
            vout: op_de.vout,
            party0_rec_key: dave_x,
            party1_rec_key: erin_x,
            is_asset_channel: false,
            asset_id: [0u8; 32],
            funding_resolved: true,
        },
    )?;
    registry.register_channel(
        &hasher,
        2,
        ChannelRegistryLeaf {
            txid_display: op_ef.txid_display,
            vout: op_ef.vout,
            party0_rec_key: erin_x,
            party1_rec_key: fabia_x,
            is_asset_channel: true,
            asset_id,
            funding_resolved: true,
        },
    )?;

    let mut nullifiers = IndexedNullifierTree::<Fp>::new(&hasher);
    let mut quotes = QuoteConsumptionTree::<Fp>::new(&hasher);

    let rfq_dave_shared = [0x91u8; 32];
    let qt_dave_shared = [0xa1u8; 32];
    let rfq_erin_1 = [0x92u8; 32];
    let qt_erin_1 = [0xa2u8; 32];
    let rfq_erin_2 = [0x93u8; 32];
    let qt_erin_2 = [0xa3u8; 32];

    quotes.register_quote(&hasher, 0, rfq_dave_shared, qt_dave_shared, 900_000_000)?;
    quotes.register_quote(&hasher, 1, rfq_erin_1, qt_erin_1, 1_000_000_000)?;
    quotes.register_quote(&hasher, 2, rfq_erin_2, qt_erin_2, 1_000_000_000)?;

    // Epoch 1: Dave purchase (500M msat, 29,103,000 atoms) + Erin sale (500,001,500 msat gross, 500M quote out)
    let rec_e1_s1 = ReceiptV1 {
        genesis,
        policy,
        asset: asset_id,
        edge_key: dave_x,
        btc_peer_key: erin_x,
        asset_peer_key: charlie_x,
        rfq_id: rfq_dave_shared,
        quote_transcript: qt_dave_shared,
        payment_hash: [0x51u8; 32],
        asset_pre: [0x10u8; 32],
        asset_post: [0x11u8; 32],
        btc_pre: [0x20u8; 32],
        btc_post: [0x21u8; 32],
        direction: 0,
        asset_edge_side: 1,
        btc_edge_side: 0,
        asset_outpoint: op_cd,
        asset_offerer: 0,
        asset_htlc: 1,
        btc_outpoint: op_de,
        btc_offerer: 0,
        btc_htlc: 1,
        asset_atoms: 29_103_000,
        gross_btc_msat: 500_000_000,
    };
    let d1 = rec_e1_s1.digest()?;
    let w_e1_s1 = ReceiptStepWitness {
        receipt: rec_e1_s1,
        signatures: ThreePartySignatures {
            edge: det_sign(&dave_kp, &d1),
            btc_peer: det_sign(&erin_kp, &d1),
            asset_peer: det_sign(&charlie_kp, &d1),
        },
        asset_chan_idx: 0,
        btc_chan_idx: 1,
        quote_idx: 0,
        quote_delta_msat: 500_000_000,
    };

    let rec_e1_s2 = ReceiptV1 {
        genesis,
        policy,
        asset: asset_id,
        edge_key: erin_x,
        btc_peer_key: dave_x,
        asset_peer_key: fabia_x,
        rfq_id: rfq_erin_1,
        quote_transcript: qt_erin_1,
        payment_hash: [0x51u8; 32],
        asset_pre: [0x30u8; 32],
        asset_post: [0x31u8; 32],
        btc_pre: [0x21u8; 32],
        btc_post: [0x22u8; 32],
        direction: 1,
        asset_edge_side: 0,
        btc_edge_side: 1,
        asset_outpoint: op_ef,
        asset_offerer: 0,
        asset_htlc: 1,
        btc_outpoint: op_de,
        btc_offerer: 0,
        btc_htlc: 1,
        asset_atoms: 29_103_000,
        gross_btc_msat: 500_001_500,
    };
    let d2 = rec_e1_s2.digest()?;
    let w_e1_s2 = ReceiptStepWitness {
        receipt: rec_e1_s2,
        signatures: ThreePartySignatures {
            edge: det_sign(&erin_kp, &d2),
            btc_peer: det_sign(&dave_kp, &d2),
            asset_peer: det_sign(&fabia_kp, &d2),
        },
        asset_chan_idx: 2,
        btc_chan_idx: 1,
        quote_idx: 1,
        quote_delta_msat: 500_000_000,
    };

    let (man_e1, prep_e1) = verify_and_prepare_epoch_native(
        &hasher,
        policy,
        genesis,
        asset_id,
        6,
        1,
        &registry,
        &mut nullifiers,
        &mut quotes,
        &[w_e1_s1, w_e1_s2],
    )?;

    let z0_e1 = build_initial_z0(
        &man_e1.policy_hash,
        &man_e1.chain_genesis,
        &man_e1.asset_id,
        man_e1.asset_decimals,
        man_e1.epoch_id,
        &man_e1.registry_root,
        &man_e1.prev_nullifier_root,
        &man_e1.prev_quote_root,
    )?;
    let component_constraints = profile_component_constraints(&prep_e1[0], &z0_e1)?;

    // Epoch 2 (carries forward persistent nullifier & quote roots; uses remaining 350M of 900M on quote 0!)
    let rec_e2_s1 = ReceiptV1 {
        genesis,
        policy,
        asset: asset_id,
        edge_key: dave_x,
        btc_peer_key: erin_x,
        asset_peer_key: charlie_x,
        rfq_id: rfq_dave_shared,
        quote_transcript: qt_dave_shared,
        payment_hash: [0x52u8; 32],
        asset_pre: [0x11u8; 32],
        asset_post: [0x12u8; 32],
        btc_pre: [0x22u8; 32],
        btc_post: [0x23u8; 32],
        direction: 0,
        asset_edge_side: 1,
        btc_edge_side: 0,
        asset_outpoint: op_cd,
        asset_offerer: 0,
        asset_htlc: 2,
        btc_outpoint: op_de,
        btc_offerer: 0,
        btc_htlc: 2,
        asset_atoms: 20_372_100,
        gross_btc_msat: 350_000_000,
    };
    let d3 = rec_e2_s1.digest()?;
    let w_e2_s1 = ReceiptStepWitness {
        receipt: rec_e2_s1,
        signatures: ThreePartySignatures {
            edge: det_sign(&dave_kp, &d3),
            btc_peer: det_sign(&erin_kp, &d3),
            asset_peer: det_sign(&charlie_kp, &d3),
        },
        asset_chan_idx: 0,
        btc_chan_idx: 1,
        quote_idx: 0,
        quote_delta_msat: 350_000_000,
    };

    let rec_e2_s2 = ReceiptV1 {
        genesis,
        policy,
        asset: asset_id,
        edge_key: erin_x,
        btc_peer_key: dave_x,
        asset_peer_key: fabia_x,
        rfq_id: rfq_erin_2,
        quote_transcript: qt_erin_2,
        payment_hash: [0x53u8; 32],
        asset_pre: [0x31u8; 32],
        asset_post: [0x32u8; 32],
        btc_pre: [0x23u8; 32],
        btc_post: [0x24u8; 32],
        direction: 0,
        asset_edge_side: 0,
        btc_edge_side: 1,
        asset_outpoint: op_ef,
        asset_offerer: 1,
        asset_htlc: 2,
        btc_outpoint: op_de,
        btc_offerer: 1,
        btc_htlc: 3,
        asset_atoms: 14_551_500,
        gross_btc_msat: 250_000_000,
    };
    let d4 = rec_e2_s2.digest()?;
    let w_e2_s2 = ReceiptStepWitness {
        receipt: rec_e2_s2,
        signatures: ThreePartySignatures {
            edge: det_sign(&erin_kp, &d4),
            btc_peer: det_sign(&dave_kp, &d4),
            asset_peer: det_sign(&fabia_kp, &d4),
        },
        asset_chan_idx: 2,
        btc_chan_idx: 1,
        quote_idx: 2,
        quote_delta_msat: 250_000_000,
    };

    let (man_e2, prep_e2) = verify_and_prepare_epoch_native(
        &hasher,
        policy,
        genesis,
        asset_id,
        6,
        2,
        &registry,
        &mut nullifiers,
        &mut quotes,
        &[w_e2_s1, w_e2_s2],
    )?;

    // Setup Nova public parameters + Spartan/IPA CompressedSNARK keys
    let ctx = TapVwapNovaContext::setup()?;
    let (comp_e1, rep_e1) = ctx.prove_and_verify_epoch(&man_e1, &prep_e1)?;
    let (_comp_e2, rep_e2) = ctx.prove_and_verify_epoch(&man_e2, &prep_e2)?;

    // Soundness sanity check on CompressedSNARK: tampered z0 MUST fail verify!
    let mut bad_z0 = z0_e1.clone();
    bad_z0[0] += Fp::ONE;
    if comp_e1.verify(&ctx.vk, prep_e1.len(), &bad_z0).is_ok() {
        return Err("CompressedSNARK accepted tampered z0!".to_string());
    }

    let live_regtest_epoch_proof = if let Some(ref p) = regtest_path {
        Some(prove_regtest_witness_file(&ctx, p)?)
    } else {
        None
    };

    let output = FullBenchmarkOutput {
        stage: "TAP-VWAP-03",
        prover_crate: "tap-vwap-nova-prover v0.1.0 (isolated)",
        nova_snark_version: "0.76.0",
        secp256k1_version: "0.30.0",
        rust_profile: "release (opt-level = 3)",
        evidence_tier: 1,
        eligible_for_consensus_index: false,
        pp_setup_ms: ctx.pp_setup_ms,
        compressed_snark_setup_ms: ctx.snark_setup_ms,
        peak_rss_kib: read_linux_vm_hwm_kib(),
        component_constraints,
        synthetic_cross_epoch_proofs: vec![rep_e1, rep_e2],
        live_regtest_epoch_proof,
    };

    let json = serde_json::to_string_pretty(&output).map_err(|e| format!("{e}"))?;
    if let Some(p) = out_path {
        std::fs::write(&p, format!("{json}\n")).map_err(|e| format!("write {p}: {e}"))?;
    } else {
        println!("{json}");
    }
    Ok(())
}
