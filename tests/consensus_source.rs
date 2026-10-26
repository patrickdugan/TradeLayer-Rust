use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::consensus_source::{
    require_authenticated_admission, SourceBlockMode, SourceConsensusConfig, SourceConsensusPolicy,
    SourceDisposition, SourceTransaction, UnverifiedSourceBlock,
};
use tradelayer_core::economic::{EconomicConfig, EconomicState, SettlementPolicy};
use tradelayer_core::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::joint::JointProtocolState;
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::storage::Recovery;
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::validity_source::SourceValue as V;

fn a(s: &str) -> AccountId {
    s.try_into().unwrap()
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn id(n: u64) -> TxId {
    let mut b = [0; 32];
    b[24..].copy_from_slice(&n.to_be_bytes());
    TxId::from_bytes(b)
}
fn block_id(n: u64) -> BlockId {
    let mut b = [0x41; 32];
    b[24..].copy_from_slice(&n.to_be_bytes());
    BlockId(b)
}
fn fixture(network: TradeLayerMainnet) -> EconomicChain {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(900),
            Operation::CreateProperty {
                id: p(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: (1u128 << 70) + 3,
            },
        )
        .unwrap();
    let joint = JointProtocolState::from_bootstrap_unverified(
        network,
        a("admin"),
        ledger.clone(),
        BTreeSet::from([p()]),
    )
    .unwrap();
    let bootstrap_commitment = EconomicState::source_bootstrap_commitment(&joint).unwrap();
    let economics = EconomicConfig {
        native_entitlements: None,
        insurance: None,
        network,
        contract: 1.try_into().unwrap(),
        collateral: p(),
        inverse: false,
        notional: "1".parse().unwrap(),
        initial_margin_per_contract: "10".parse().unwrap(),
        fee_route: FeeRoute::Native(1),
        policy: SettlementPolicy::ConservingV1,
        funding: None,
        marking: None,
        liquidation: None,
        amm: None,
        contract_binding: None,
        source_consensus: Some(SourceConsensusConfig {
            bootstrap_commitment,
            policy: SourceConsensusPolicy::ConservingSequentialV1,
        }),
    };
    EconomicChain::new_unverified(
        EconomicChainConfig {
            economics,
            admin: a("admin"),
            permitted_spot_properties: BTreeSet::from([p()]),
            rollback_limit: 8,
        },
        ledger,
    )
    .unwrap()
}
fn tx(position: u32, n: u64, payload: &str, sender: &str) -> SourceTransaction {
    SourceTransaction {
        position,
        txid: id(n),
        payload: payload.into(),
        marker: V::String("tl".into()),
        sender: V::String(sender.into()),
        reference: V::Null,
        sender_amount: V::number(1.0),
        reference_amount: V::number(0.0),
    }
}
fn activation(position: u32, n: u64, types: &str) -> SourceTransaction {
    tx(
        position,
        n,
        &format!("0{types},{}", "11".repeat(32)),
        "admin",
    )
}
fn block(
    chain: &EconomicChain,
    n: u64,
    transactions: Vec<SourceTransaction>,
) -> UnverifiedSourceBlock {
    UnverifiedSourceBlock {
        network: chain.config().economics.network,
        header: BlockHeader {
            height: chain.tip().height + 1,
            id: block_id(n),
            parent: chain.tip().id,
        },
        mode: SourceBlockMode::IndexedReplay,
        transactions,
    }
}
fn activate(chain: &mut EconomicChain) {
    let b = block(chain, 1, vec![activation(0, 1, "13;14;18")]);
    assert_eq!(chain.apply_source_unverified_block(&b).unwrap().accepted, 1);
}
fn oracle_lifecycle(chain: &mut EconomicChain) -> UnverifiedSourceBlock {
    // The source create validator accepts the empty ticker; the actual owner
    // rejects it atomically. The next create must still allocate oracle 1.
    let b = block(
        chain,
        2,
        vec![
            tx(0, 2, "d,https://x,backup,,1", "admin"),
            tx(1, 3, "dBTC,https://x,backup,,1", "admin"),
            tx(2, 4, "e1,lfls", "mallory"),
            tx(3, 5, "e1,lfls", "admin"),
        ],
    );
    let report = chain.apply_source_unverified_block(&b).unwrap();
    assert_eq!(report.accepted, 2);
    assert_eq!(report.rejected, 2);
    assert_eq!(
        report.receipts[0].disposition,
        SourceDisposition::EconomicRejected
    );
    assert_eq!(report.receipts[2].disposition, SourceDisposition::Invalid);
    assert_eq!(chain.state().joint().registry().oracle_count(), 1);
    assert_eq!(
        chain
            .state()
            .joint()
            .registry()
            .oracle(1)
            .unwrap()
            .latest
            .as_ref()
            .unwrap()
            .price_1e4,
        1_000_000
    );
    b
}

#[test]
fn sequential_actual_owner_reads_candidate_atomicity_and_once_finalization() {
    let mut chain = fixture(TradeLayerMainnet::Litecoin);
    activate(&mut chain);
    oracle_lifecycle(&mut chain);
    assert_eq!(
        chain.state().ledger().available_atoms(p(), &a("alice")),
        (1u128 << 70) + 3
    );
    assert_eq!(chain.state().finalized_height(), Some(2));
    assert_eq!(chain.state().book().processed_through(), Some(2));
    assert_eq!(&chain.state().snapshot_bytes().unwrap()[..5], b"TLEC\x07");
    assert_eq!(
        chain.state().source_consensus_state().frames[1]
            .transactions
            .len(),
        4
    );
    assert!(require_authenticated_admission().is_err());
}

#[test]
fn prior_activation_repair_and_rejected_replay_evidence_survive_next_block() {
    let mut chain = fixture(TradeLayerMainnet::Bitcoin);
    let b = block(
        &chain,
        1,
        vec![
            activation(0, 1, "13;14"),
            tx(1, 2, "dBTC,https://x,backup,,1", "admin"),
        ],
    );
    let report = chain.apply_source_unverified_block(&b).unwrap();
    assert_eq!(report.accepted, 1);
    assert_eq!(
        report.receipts[1].disposition,
        SourceDisposition::EconomicRejected
    );
    let b = block(
        &chain,
        2,
        vec![
            tx(0, 2, "dBTC,https://x,backup,,1", "admin"),
            tx(1, 3, "dBTC,https://x,backup,,1", "admin"),
        ],
    );
    let report = chain.apply_source_unverified_block(&b).unwrap();
    assert_eq!(report.already_processed, 1);
    assert_eq!(report.accepted, 1);
    assert_eq!(chain.state().joint().registry().oracle_count(), 1);
}

#[test]
fn source_index_and_live_funding_partitions_preserve_original_relative_order() {
    assert!(!SourceBlockMode::Realtime.is_funding_payload("Mbody"));
    assert!(SourceBlockMode::IndexedReplay.is_funding_payload("Kbody"));
    for (mode, expected) in [
        (SourceBlockMode::IndexedReplay, vec![2, 3, 0, 1, 4]),
        (SourceBlockMode::Realtime, vec![1, 3, 0, 2, 4]),
    ] {
        let mut chain = fixture(TradeLayerMainnet::Litecoin);
        let mut b = block(
            &chain,
            1,
            vec![
                tx(0, 10, "?", "admin"),
                tx(1, 11, "m", "admin"),
                tx(2, 12, "k", "admin"),
                tx(3, 13, "4", "admin"),
                tx(4, 14, "z", "admin"),
            ],
        );
        b.mode = mode;
        let report = chain.apply_source_unverified_block(&b).unwrap();
        assert_eq!(
            report
                .receipts
                .iter()
                .map(|r| r.position)
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            chain.state().source_consensus_state().frames[0].transactions[0].position,
            0
        );
    }
}

#[test]
fn empty_due_blocks_run_funding_once_and_are_fully_retained() {
    use tradelayer_core::economic::{FundingConfig, FundingPolicy};
    let pristine = fixture(TradeLayerMainnet::Litecoin);
    let mut cfg = pristine.config().clone();
    cfg.economics.fee_route = FeeRoute::Oracle(1);
    cfg.economics.funding = Some(FundingConfig {
        oracle_id: 1,
        max_staleness_blocks: 32,
        policy: FundingPolicy::ConservingFundingV1,
    });
    let mut chain = EconomicChain::new_unverified(cfg, pristine.state().ledger().clone()).unwrap();
    activate(&mut chain);
    oracle_lifecycle(&mut chain);
    for n in 3..=24 {
        let b = block(&chain, n, Vec::new());
        let report = chain.apply_source_unverified_block(&b).unwrap();
        assert_eq!(report.accepted, 0);
    }
    assert_eq!(chain.state().funding_state().epochs.len(), 1);
    assert_eq!(chain.state().funding_state().epochs[0].height, 24);
    assert_eq!(chain.state().source_consensus_state().frames.len(), 24);
    let bytes = chain.checkpoint_bytes().unwrap();
    assert_eq!(
        EconomicChain::from_checkpoint_bytes(&bytes, chain.config().clone(), chain.tip()).unwrap(),
        chain
    );
}

#[test]
fn canonical_restore_and_reorg_remove_rejected_and_accepted_audit_together_three_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let mut chain = fixture(network);
        activate(&mut chain);
        let before = chain.checkpoint_bytes().unwrap();
        let b = oracle_lifecycle(&mut chain);
        let bytes = chain.checkpoint_bytes().unwrap();
        let cfg = chain.config().clone();
        let tip = chain.tip();
        let mut restored = EconomicChain::from_checkpoint_bytes(&bytes, cfg, tip).unwrap();
        assert_eq!(restored, chain);
        restored.rollback_tip(tip).unwrap();
        assert_eq!(restored.checkpoint_bytes().unwrap(), before);
        restored.apply_source_unverified_block(&b).unwrap();
        assert_eq!(restored.checkpoint_bytes().unwrap(), bytes);
    }
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "tl-source-consensus-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn durable_cas_restart_and_rollback_commit_one_owner_with_all_source_evidence() {
    let temp = Temp::new();
    let mut chain = fixture(TradeLayerMainnet::Dogecoin);
    activate(&mut chain);
    let cfg = chain.config().clone();
    let tip = chain.tip();
    let mut owner = DurableEconomicChain::initialize(&temp.0, chain, tip).unwrap();
    let h1 = owner.head().unwrap();
    let b = block(
        owner.chain().unwrap(),
        2,
        vec![
            tx(0, 3, "dBTC,https://x,backup,,1", "admin"),
            tx(1, 4, "e1,lfls", "admin"),
        ],
    );
    let (h2, report) = owner.apply_source_unverified_block(h1, &b).unwrap();
    assert_eq!(report.accepted, 2);
    assert!(owner.apply_source_unverified_block(h1, &b).is_err());
    assert_eq!(owner.head().unwrap(), h2);
    drop(owner);
    let (mut owner, _) =
        DurableEconomicChain::load(&temp.0, cfg.clone(), h2, Recovery::RejectIncompleteTail)
            .unwrap();
    let h3 = owner.rollback_to(h2, tip).unwrap();
    assert!(h3.sequence > h2.sequence);
    assert_eq!(
        owner
            .chain()
            .unwrap()
            .state()
            .source_consensus_state()
            .frames
            .len(),
        1
    );
    drop(owner);
    assert!(
        DurableEconomicChain::load(&temp.0, cfg.clone(), h2, Recovery::RejectIncompleteTail)
            .is_err()
    );
    let (owner, _) =
        DurableEconomicChain::load(&temp.0, cfg, h3, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(owner.head().unwrap(), h3);
    let expected = tradelayer_core::economic_state_oracle::ExportExpectation {
        config: owner.chain().unwrap().config(),
        head: h3,
        state_commitment: None,
    };
    let export = tradelayer_core::economic_state_oracle::EconomicStateOracleExport::from_durable(
        &owner, &expected,
    )
    .unwrap();
    assert_eq!(export.state_codec_version(), 7);
    let encoded = export.canonical_bytes().unwrap();
    let decoded = tradelayer_core::economic_state_oracle::EconomicStateOracleExport::decode(
        &encoded, &expected,
    )
    .unwrap();
    assert_eq!(decoded.canonical_bytes().unwrap(), encoded);
}

fn crc(bytes: &mut [u8]) {
    let n = bytes.len();
    let mut c = !0u32;
    for b in &bytes[..n - 4] {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb8_8320u32.wrapping_mul(c & 1));
        }
    }
    bytes[n - 4..].copy_from_slice(&(!c).to_be_bytes());
}
fn source_start(bytes: &[u8]) -> usize {
    let length = u32::from_be_bytes(bytes[38..42].try_into().unwrap()) as usize;
    42 + length + 4
}
fn read_u32(bytes: &[u8], offset: usize) -> usize {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize
}

#[test]
fn repaired_crc_cannot_forge_rejected_validity_raw_payload_receipt_or_bootstrap() {
    let mut chain = fixture(TradeLayerMainnet::Bitcoin);
    activate(&mut chain);
    oracle_lifecycle(&mut chain);
    let bytes = chain.state().snapshot_bytes().unwrap();
    let cfg = chain.config().economics.clone();
    let permitted = BTreeSet::from([p()]);
    let decode = |b: &[u8]| EconomicState::from_snapshot_bytes(b, &cfg, &a("admin"), &permitted);
    let start = source_start(&bytes);
    let bootstrap = read_u32(&bytes, start);
    let first_frame = start + 4 + bootstrap + 4;
    // Alter a retained accepted receipt, not a checksum, while preserving all
    // original candidate data. Actual source replay must reject the verdict.
    let mut changed = bytes.clone();
    let needle = chain.state().source_consensus_state().frames[1].receipts[0].evaluation_commitment;
    let offset = changed.windows(32).rposition(|w| w == needle).unwrap();
    changed[offset] ^= 1;
    crc(&mut changed);
    assert!(decode(&changed).is_err());
    let mut changed = bytes.clone();
    changed[first_frame + 8] ^= 1;
    crc(&mut changed);
    assert!(decode(&changed).is_err());
    let mut changed = bytes.clone();
    let payload = b"dBTC,https://x,backup,,1";
    let offset = changed
        .windows(payload.len())
        .position(|w| w == payload)
        .unwrap();
    changed[offset + 1] = b'E';
    crc(&mut changed);
    assert!(decode(&changed).is_err());
    let mut changed = bytes.clone();
    changed[5] ^= 1;
    crc(&mut changed);
    assert!(decode(&changed).is_err());
    let mut changed = bytes.clone();
    changed[4] = 6;
    crc(&mut changed);
    assert!(decode(&changed).is_err());
    for n in [0, 4, 37, bytes.len() - 1] {
        assert!(decode(&bytes[..n]).is_err());
    }
    let mut legacy_cfg = cfg.clone();
    legacy_cfg.source_consensus = None;
    assert!(
        EconomicState::from_snapshot_bytes(&bytes, &legacy_cfg, &a("admin"), &permitted).is_err()
    );
}

#[test]
fn bounded_bad_input_and_typed_bypass_leave_actual_chain_unchanged() {
    let mut chain = fixture(TradeLayerMainnet::Litecoin);
    let before = chain.checkpoint_bytes().unwrap();
    let mut b = block(
        &chain,
        1,
        vec![activation(1, 1, "13"), activation(0, 2, "14")],
    );
    assert!(chain.apply_source_unverified_block(&b).is_err());
    b.transactions = vec![tx(0, 1, &"0".repeat(65_537), "admin")];
    assert!(chain.apply_source_unverified_block(&b).is_err());
    b.transactions = vec![activation(0, 1, "13")];
    b.transactions[0].sender_amount = V::number(f64::INFINITY);
    assert!(chain.apply_source_unverified_block(&b).is_err());
    let typed = UnverifiedEconomicBlock {
        network: b.network,
        header: b.header,
        inputs: Vec::new(),
    };
    assert!(chain.apply_unverified_block(&typed).is_err());
    assert_eq!(chain.checkpoint_bytes().unwrap(), before);
}

#[test]
fn genuine_consensus_oracle_hashes_remain_separate_from_economic_roots() {
    use tradelayer_core::consensus_source::{source_state_consensus_hash, source_tx_index_hash};
    use tradelayer_core::issuance_intent::JsonValue;
    fn unhex(s: &str) -> Vec<u8> {
        s.as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }
    let fixture = include_str!("../fixtures/consensus_vectors.tsv");
    let observation = |name: &str| {
        let line = fixture
            .lines()
            .find(|l| l.starts_with(&format!("V\t{name}\t")))
            .unwrap();
        let encoded = line.split('\t').nth(2).unwrap();
        JsonValue::parse(std::str::from_utf8(&unhex(encoded)).unwrap()).unwrap()
    };
    let JsonValue::Object(expected) = observation("state-exact-json-hash") else {
        panic!()
    };
    let JsonValue::String(expected_json) = expected.get("state").unwrap() else {
        panic!()
    };
    let JsonValue::String(expected_hash) = expected.get("hash").unwrap() else {
        panic!()
    };
    let mut rows = Vec::new();
    for name in [
        "tallyMap",
        "activations",
        "channels",
        "clearlists",
        "contractList",
        "feeCache",
        "insurance",
        "marginMaps",
        "oracleData",
        "oracleList",
        "orderBooks",
        "propertyList",
        "syntheticTokens",
        "vaults",
        "volumeIndex",
        "withdrawQueue",
    ] {
        let values = match name {
            "tallyMap" => vec![
                JsonValue::parse(r#"{"_id":"z","value":1}"#).unwrap(),
                JsonValue::parse(r#"{"_id":"a","value":2}"#).unwrap(),
            ],
            "activations" => vec![JsonValue::parse(r#"{"_id":"a","value":1}"#).unwrap()],
            _ => Vec::new(),
        };
        rows.push((name.into(), values));
    }
    let (json, hash) = source_state_consensus_hash(&rows).unwrap();
    assert_eq!(&json, expected_json);
    assert_eq!(hash.as_slice(), unhex(expected_hash));
    let values=JsonValue::parse(r#"[{"_id":"meta"},{"_id":"tx-b","value":2},{"_id":"tx-a","value":1},{"_id":"txNotDash","value":3}]"#).unwrap();
    let JsonValue::Array(values) = values else {
        panic!()
    };
    let JsonValue::String(expected) = observation("tx-index-db-order") else {
        panic!()
    };
    assert_eq!(
        source_tx_index_hash(&values).unwrap().as_slice(),
        unhex(&expected)
    );
    assert!(fixture.lines().any(|l| l.starts_with("M\t30\t")));
}

#[test]
fn actual_prechange_v1_through_v5_snapshot_bytes_stay_exact_when_disabled() {
    use tradelayer_core::economic::{
        FundedAmmConfig, FundedAmmPolicy, FundingConfig, FundingPolicy, LiquidationConfig,
        LiquidationPolicy, MarkConfig, MarkPolicy,
    };
    let original = fixture(TradeLayerMainnet::Bitcoin);
    let joint = JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Bitcoin,
        a("admin"),
        original.state().ledger().clone(),
        BTreeSet::from([p()]),
    )
    .unwrap();
    for row in include_str!("../fixtures/consensus_legacy_snapshot_vectors.tsv").lines() {
        let mut fields = row.split('\t');
        let version: u8 = fields.next().unwrap().parse().unwrap();
        let hex = fields.next().unwrap();
        let bytes: Vec<u8> = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect();
        let mut cfg = original.config().economics.clone();
        cfg.source_consensus = None;
        cfg.fee_route = if version == 1 {
            FeeRoute::Native(1)
        } else {
            FeeRoute::Oracle(1)
        };
        cfg.funding = if version == 2 {
            Some(FundingConfig {
                oracle_id: 1,
                max_staleness_blocks: 32,
                policy: FundingPolicy::ConservingFundingV1,
            })
        } else {
            None
        };
        cfg.marking = if version >= 3 {
            Some(MarkConfig {
                oracle_id: 1,
                policy: MarkPolicy::ConservingMarkBasisV1,
            })
        } else {
            None
        };
        cfg.liquidation = if version >= 4 {
            Some(LiquidationConfig {
                max_price_deviation_bps: 100,
                max_fills_per_owner: 64,
                policy: LiquidationPolicy::ConservingLiquidationV1,
            })
        } else {
            None
        };
        cfg.amm = if version >= 5 {
            Some(FundedAmmConfig {
                pool_id: 1,
                spread_bps: 25,
                max_quote_quantity: 1u32.into(),
                max_abs_inventory: 10u32.into(),
                max_collateral_atoms: 1u128 << 80,
                policy: FundedAmmPolicy::ConservingFundedContractAmmV1,
            })
        } else {
            None
        };
        let state = EconomicState::from_joint_unverified(cfg.clone(), joint.clone()).unwrap();
        assert_eq!(state.snapshot_bytes().unwrap(), bytes, "legacy v{version}");
        assert_eq!(
            EconomicState::from_snapshot_bytes(&bytes, &cfg, &a("admin"), &BTreeSet::from([p()]))
                .unwrap(),
            state
        );
    }
}

// Append to tests/consensus_source.rs; reuses its existing fixture/id/block/crc helpers.
// This is a scratch-only independent regression for the source-history prefix fence.
#[test]
fn rollback_snapshot_cannot_splice_valid_alternate_source_history_at_the_same_tip() {
    let mut branch_a = fixture(TradeLayerMainnet::Bitcoin);
    let a1 = block(&branch_a, 1, vec![activation(0, 1, "13;14")]);
    branch_a.apply_source_unverified_block(&a1).unwrap();
    let alternate_before = branch_a.state().snapshot_bytes().unwrap();

    let mut branch_b = fixture(TradeLayerMainnet::Bitcoin);
    let b1 = block(&branch_b, 1, vec![activation(0, 1, "13")]);
    branch_b.apply_source_unverified_block(&b1).unwrap();
    assert_eq!(branch_a.tip(), branch_b.tip());
    assert_ne!(
        branch_a.state().source_consensus_state().frames,
        branch_b.state().source_consensus_state().frames
    );
    // Both prior owners are genuine independently replayable v7 snapshots.
    assert_eq!(
        EconomicState::from_snapshot_bytes(
            &alternate_before,
            &branch_b.config().economics,
            &a("admin"),
            &BTreeSet::from([p()]),
        )
        .unwrap(),
        *branch_a.state()
    );
    let b2 = block(&branch_b, 2, Vec::new());
    branch_b.apply_source_unverified_block(&b2).unwrap();
    let bytes = branch_b.checkpoint_bytes().unwrap();
    assert_eq!(
        EconomicChain::from_checkpoint_bytes(&bytes, branch_b.config().clone(), branch_b.tip(),)
            .unwrap(),
        branch_b
    );

    // TLEHv1: magic/network/genesis/rollback-limit, floor+tip, owner, then frames.
    // Replace only height 2's `before` owner, allowing a different byte length;
    // retain the current owner, every header and the independently expected tip.
    let mut cursor = 5 + 1 + 32 + 1 + 40 + 40;
    cursor += 4 + read_u32(&bytes, cursor);
    let frame_count = bytes[cursor] as usize;
    cursor += 1;
    assert_eq!(frame_count, 2);
    let mut last_before = None;
    for _ in 0..frame_count {
        cursor += 8 + 32 + 32;
        let length_offset = cursor;
        let length = read_u32(&bytes, cursor);
        cursor += 4;
        last_before = Some((length_offset, cursor + length));
        cursor += length;
    }
    assert_eq!(cursor + 4, bytes.len());
    let (length_offset, after_before) = last_before.unwrap();
    let mut spliced = bytes[..length_offset].to_vec();
    spliced.extend_from_slice(&(alternate_before.len() as u32).to_be_bytes());
    spliced.extend_from_slice(&alternate_before);
    spliced.extend_from_slice(&bytes[after_before..]);
    crc(&mut spliced);

    assert!(EconomicChain::from_checkpoint_bytes(
        &spliced,
        branch_b.config().clone(),
        branch_b.tip(),
    )
    .is_err());
}

#[test]
fn source_valid_zero_lag_is_atomically_rejected_without_consuming_oracle_id() {
    let mut chain = fixture(TradeLayerMainnet::Litecoin);
    activate(&mut chain);
    let b = block(
        &chain,
        2,
        vec![
            tx(0, 70, "dBTC,https://x,backup,,0", "admin"),
            tx(1, 71, "dBTC,https://x,backup,,1", "admin"),
        ],
    );
    let report = chain.apply_source_unverified_block(&b).unwrap();
    assert_eq!(report.accepted, 1);
    assert_eq!(report.rejected, 1);
    assert_eq!(
        report.receipts[0].disposition,
        SourceDisposition::EconomicRejected
    );
    assert!(report.receipts[0].reason.contains("oracle lag"));
    assert_eq!(chain.state().joint().registry().oracle_count(), 1);
    assert!(chain.state().joint().registry().oracle(1).is_some());
    assert!(chain.state().joint().registry().oracle(2).is_none());
}
