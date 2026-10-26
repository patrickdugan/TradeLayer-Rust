//! Restore actual oracle catalogs by replaying original source, including sole
//! Ledger sends, native coupling and real funded epoch observations.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::clearlist_source_owner::ClearlistSourceOwner;
use tradelayer_core::consensus_source::{
    SourceBlockMode, SourceConsensusConfig, SourceConsensusPolicy, SourceDisposition,
    SourceTransaction,
};
use tradelayer_core::economic::{
    EconomicConfig, EconomicState, FundingConfig, FundingPolicy, SettlementPolicy,
};
use tradelayer_core::joint::JointProtocolState;
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::native_entitlement::*;
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::oracle_source_owner::OracleSourceOwner;
use tradelayer_core::property_catalog::PropertyCatalog;
use tradelayer_core::source_catalogs::{bootstrap_commitment, SourceCatalogOwners};
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceOutcome, SourceRuleError,
    SourceValue as V,
};
const U: u128 = 100_000_000;
const A: &str = "1BoatSLRHtKNngkdXEeobR76b53LETtpyT";
const B: &str = "1BitcoinEaterAddressDontSendf59kuE";
const BACKUP: &str = "1CounterpartyXXXXXXXXXXXXXXXUWLpVr";
fn a(s: &str) -> AccountId {
    s.try_into().unwrap()
}
fn p(n: u32) -> PropertyId {
    n.try_into().unwrap()
}
fn s(v: &str) -> V {
    V::String(v.into())
}
fn n(v: f64) -> V {
    V::number(v)
}
fn id(n: u64) -> TxId {
    let mut b = [0; 32];
    b[24..].copy_from_slice(&n.to_be_bytes());
    TxId::from_bytes(b)
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
struct NoTally;
impl SourceBackend for NoTally {
    fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        Err(SourceRuleError::UnknownDependency)
    }
}
fn fixture(native: bool) -> (EconomicState, EconomicConfig, BTreeSet<PropertyId>) {
    let network = TradeLayerMainnet::Bitcoin;
    let mut ledger = Ledger::new();
    for property in [1, 2, 3, 4, 7] {
        ledger
            .apply(
                id(900 + property as u64),
                Operation::CreateProperty {
                    id: p(property),
                    issuer: a(A),
                    kind: if property == 7 {
                        PropertyKind::Fixed
                    } else {
                        PropertyKind::Managed
                    },
                    initial_atoms: if property == 7 { 100 * U } else { 20 * U },
                },
            )
            .unwrap();
    }
    for property in [2, 3] {
        ledger
            .apply(
                id(920 + property as u64),
                Operation::Transfer {
                    id: p(property),
                    from: a(A),
                    to: a(B),
                    amount_atoms: 10 * U,
                },
            )
            .unwrap();
    }
    let policy = native.then(|| NativeEntitlementPolicy {
        network,
        bootstrap: Tip {
            height: 0,
            id: network.genesis(),
        },
        bootstrap_ledger_commitment: tradelayer_core::native_entitlement::bootstrap_commitment(
            &ledger.snapshot_bytes().unwrap(),
        ),
        target_operator: a("operator"),
        assets: [
            NativeRole::Base,
            NativeRole::VestingBase,
            NativeRole::VestingIncome,
            NativeRole::Income,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, role)| NativeAssetSpec {
            role,
            property: p(i as u32 + 1),
            issuer: a(A),
            ledger_kind: PropertyKind::Managed,
            catalog_class: match role {
                NativeRole::Base => SourceCatalogClass::Fixed,
                NativeRole::Income => SourceCatalogClass::Native,
                _ => SourceCatalogClass::Vesting,
            },
            declared_figure_atoms: 20 * U,
            issuance_ceiling_atoms: None,
            expected_initial_issued_atoms: 20 * U,
        })
        .collect(),
        bindings: [(2, 1), (3, 4)]
            .into_iter()
            .map(|(coupon, target)| NativeBindingSpec {
                coupon: p(coupon),
                target: p(target),
                curve_scale_atoms: 20 * U,
                initial_markers: [B, A]
                    .into_iter()
                    .map(|owner| NativeMarkerRow {
                        owner: a(owner),
                        atoms: 10 * U,
                    })
                    .collect(),
                funding: PrefundedTargetSpec {
                    funding_owner: a(A),
                    target_atoms: 20 * U,
                },
            })
            .collect(),
        quote_policy: NativeQuotePolicy::SourceQuoteExactOrRejectV1,
        driver: NativeDriver::SourcePerCallV1,
        payout_order: NativePayoutOrder::AsciiAccountV1,
        coverage: NativeCoverage::CoverInitialMarkersV1,
    });
    let permitted = BTreeSet::from([p(7)]);
    let joint = match &policy {
        Some(policy) => {
            ledger.adopt_native_entitlements_unverified(policy).unwrap();
            JointProtocolState::from_native_bootstrap_unverified(
                network,
                a("admin"),
                ledger,
                permitted.clone(),
                policy,
            )
            .unwrap()
        }
        None => JointProtocolState::from_bootstrap_unverified(
            network,
            a("admin"),
            ledger,
            permitted.clone(),
        )
        .unwrap(),
    };
    let mut properties = PropertyCatalog::new();
    for property in [1, 2, 3, 4, 7] {
        assert_eq!(
            properties
                .execute(
                    "addProperty",
                    &[
                        n(property as f64),
                        s(&format!("P{property}")),
                        n(100.0),
                        s(if matches!(property, 2 | 3) {
                            "Vesting"
                        } else {
                            "Fixed"
                        }),
                        n(0.0),
                        s(A),
                        s(BACKUP)
                    ],
                    &mut NoTally
                )
                .unwrap()
                .outcome,
            SourceOutcome::Returned(V::Undefined)
        );
    }
    let mut oracles = OracleSourceOwner::new();
    oracles
        .import_rows(
            "oracleList",
            vec![obj(&[
                ("_id", s("oracle-0")),
                ("id", n(0.0)),
                (
                    "name",
                    obj(&[
                        ("type", s("bootstrap legacy nested")),
                        ("adminAddress", s(A)),
                        ("backupAddress", s(BACKUP)),
                    ]),
                ),
                ("whitelists", V::Array(vec![n(17.0)])),
                ("unknown", s("keep pinned full row")),
            ])],
        )
        .unwrap();
    oracles
        .evaluate(&SourceCall {
            method: "load".into(),
            arguments: vec![],
        })
        .unwrap();
    let owners =
        SourceCatalogOwners::new(properties, ClearlistSourceOwner::new()).with_oracles(oracles);
    let config = EconomicConfig {
        network,
        contract: 1.try_into().unwrap(),
        collateral: p(7),
        inverse: false,
        notional: "1".parse().unwrap(),
        initial_margin_per_contract: "1".parse().unwrap(),
        fee_route: FeeRoute::Oracle(1),
        policy: SettlementPolicy::ConservingV1,
        funding: Some(FundingConfig {
            oracle_id: 1,
            max_staleness_blocks: 32,
            policy: FundingPolicy::ConservingFundingV1,
        }),
        marking: None,
        liquidation: None,
        amm: None,
        contract_binding: None,
        native_entitlements: policy,
        insurance: None,
        source_consensus: Some(SourceConsensusConfig {
            bootstrap_commitment: EconomicState::source_bootstrap_commitment(&joint).unwrap(),
            catalog_bootstrap_commitment: Some(bootstrap_commitment(
                &owners.checkpoint_bytes().unwrap(),
            )),
            policy: SourceConsensusPolicy::ConservingSequentialV1,
        }),
    };
    (
        EconomicState::from_joint_with_source_catalogs_unverified(config.clone(), joint, owners)
            .unwrap(),
        config,
        permitted,
    )
}
fn tx(position: u32, number: u64, payload: &str, actor: &str) -> SourceTransaction {
    SourceTransaction {
        position,
        txid: id(number),
        payload: payload.into(),
        marker: s("tl"),
        sender: s(actor),
        reference: V::Null,
        sender_amount: n(1.0),
        reference_amount: n(0.0),
    }
}
fn apply(state: &mut EconomicState, transactions: &[SourceTransaction]) {
    let last = state.source_consensus_state().frames.last();
    let height = last.map_or(1, |f| f.header.height + 1);
    let mut block = [0x6a; 32];
    block[24..].copy_from_slice(&height.to_be_bytes());
    let header = BlockHeader {
        height,
        id: BlockId(block),
        parent: last.map_or(state.config().network.genesis(), |f| f.header.id),
    };
    state
        .apply_source_block_unverified(header, SourceBlockMode::IndexedReplay, transactions)
        .unwrap_or_else(|e| panic!("source height {height}: {e:?}"));
}
fn base36(mut value: u128) -> String {
    let mut b = vec![];
    while value > 0 {
        b.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(value % 36) as usize]);
        value /= 36;
    }
    b.reverse();
    String::from_utf8(b).unwrap()
}
fn history(state: &mut EconomicState, native: bool, last_price: u128) {
    history_with_send(state, native, last_price, 10);
}
fn history_with_send(state: &mut EconomicState, native: bool, last_price: u128, send_tokens: u128) {
    apply(
        state,
        &[tx(
            0,
            1,
            &format!("02;13;14;18,{}", "11".repeat(32)),
            "admin",
        )],
    );
    let mut commands = vec![
        tx(0, 50, &format!("dBTC,https://prices,{BACKUP},,1"), A),
        tx(1, 51, "e1,lfls", A),
        tx(2, 52, &format!("20;{B};7;{}", base36(send_tokens)), A),
    ];
    if native {
        commands.push(tx(3, 53, &format!("20;{B};2;1thojk~"), A));
    }
    apply(state, &commands);
    assert!(
        state
            .source_consensus_state()
            .frames
            .last()
            .unwrap()
            .receipts
            .iter()
            .all(|r| r.disposition == SourceDisposition::Applied),
        "{:?}",
        state.source_consensus_state().frames.last()
    );
    apply(
        state,
        &[
            tx(0, 60, "?source-only-rejected-evidence", A),
            tx(1, 50, "?source-duplicate-original-id", A),
        ],
    );
    while state.finalized_height().unwrap() < 22 {
        apply(state, &[]);
    }
    apply(
        state,
        &[tx(0, 61, &format!("e1,{}", base36(last_price * 10_000)), A)],
    );
    apply(state, &[]);
    assert_eq!(state.finalized_height(), Some(24));
    assert_eq!(
        state.ledger().available_atoms(p(7), &a(A)),
        (100 - send_tokens) * U
    );
    assert_eq!(state.ledger().available_atoms(p(7), &a(B)), send_tokens * U);
    assert_eq!(state.joint().registry().oracle_count(), 0);
    assert_eq!(state.funding_state().observations.len(), 2);
    assert_eq!(state.funding_state().epochs.len(), 1);
    assert_eq!(
        state
            .funding_state()
            .observations
            .last()
            .unwrap()
            .price
            .to_string(),
        last_price.to_string()
    );
    if native {
        assert_eq!(state.native_entitlement_state().frames().len(), 24);
        assert_eq!(state.ledger().native_remaining_atoms(p(2), &a(A)), 9 * U);
    }
}
#[derive(Clone)]
struct Parts {
    pins: Vec<u8>,
    sections: Vec<Vec<u8>>,
}
fn unpack(bytes: &[u8]) -> Parts {
    assert_eq!(&bytes[..5], b"TLEC\x0a");
    let mut at = 70;
    let mut sections = vec![];
    for _ in 0..4 {
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        at += 4;
        sections.push(bytes[at..at + len].to_vec());
        at += len;
    }
    assert_eq!(at + 4, bytes.len());
    Parts {
        pins: bytes[..70].to_vec(),
        sections,
    }
}
fn crc(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in bytes {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}
fn pack(p: &Parts) -> Vec<u8> {
    let mut b = p.pins.clone();
    for s in &p.sections {
        b.extend_from_slice(&(s.len() as u32).to_be_bytes());
        b.extend_from_slice(s);
    }
    b.extend_from_slice(&crc(&b).to_be_bytes());
    b
}
#[track_caller]
fn rejects(bytes: &[u8], config: &EconomicConfig, permitted: &BTreeSet<PropertyId>) {
    assert!(EconomicState::from_snapshot_bytes(bytes, config, &a("admin"), permitted).is_err());
}

#[test]
fn full_raw_source_replay_restores_oracle_funded_generic_and_native_owners() {
    for native in [false, true] {
        let (mut state, config, permitted) = fixture(native);
        history(&mut state, native, 102);
        let bytes = state.snapshot_bytes().unwrap();
        let parts = unpack(&bytes);
        assert_eq!(&parts.sections[2][..5], b"TLSC\x02");
        assert_eq!(
            &parts.sections[0][..5],
            if native { b"TLEC\x08" } else { b"TLEC\x02" }
        );
        let mut restored =
            EconomicState::from_snapshot_bytes(&bytes, &config, &a("admin"), &permitted).unwrap();
        assert!(
            restored == state,
            "complete restored owner must equal original"
        );
        assert!(
            restored.snapshot_bytes().unwrap() == bytes,
            "complete canonical bytes must match"
        );
        restored.verify().unwrap();
        // Original deployment pins always reject a stripped source wrapper.
        rejects(&parts.sections[0], &config, &permitted);
        let mut bare = config.clone();
        bare.source_consensus = None;
        if native {
            // Native-v8 is a bootstrap + typed-frame replay container. Under a
            // deliberately different source-free configuration it can restore
            // a distinct owner with no actual oracle publications. That is no
            // provenance for this source-bound owner or its funded observations.
            let projected = EconomicState::from_snapshot_bytes(
                &parts.sections[0],
                &bare,
                &a("admin"),
                &permitted,
            )
            .unwrap();
            assert!(
                projected != state,
                "source-free native replay must not masquerade as original owner"
            );
            assert_eq!(projected.finalized_height(), state.finalized_height());
            assert!(
                projected.native_entitlement_state().frames()
                    == state.native_entitlement_state().frames()
            );
            assert!(projected.funding_state().observations.is_empty());
            assert!(projected.source_catalog_owners().is_none());
            assert!(projected.config().source_consensus.is_none());
        } else {
            rejects(&parts.sections[0], &bare, &permitted);
        }
        apply(&mut state, &[tx(0, 70, "e1,lv1c", A)]);
        apply(&mut restored, &[tx(0, 70, "e1,lv1c", A)]);
        assert!(
            restored == state,
            "next raw block must reproduce the same owner"
        );
        assert!(
            restored.snapshot_bytes().unwrap() == state.snapshot_bytes().unwrap(),
            "next canonical bytes must match"
        );
    }
}

#[test]
fn valid_other_owner_audit_or_catalog_sections_cannot_be_spliced_with_fresh_crc() {
    for native in [false, true] {
        let (mut state, config, permitted) = fixture(native);
        history(&mut state, native, 102);
        let good = unpack(&state.snapshot_bytes().unwrap());
        let (mut other, other_config, _) = fixture(native);
        assert_eq!(other_config, config);
        // A different token send changes real funded state and the native-v8
        // typed frames. Oracle price alone changes only outer raw/catalog bytes
        // when both histories have identical metadata input IDs.
        history_with_send(&mut other, native, 101, 9);
        let other = unpack(&other.snapshot_bytes().unwrap());
        for section in [0, 1, 3] {
            assert!(
                good.sections[section] != other.sections[section],
                "alternate section {section} must actually differ (native={native})"
            );
            let mut mixed = good.clone();
            mixed.sections[section] = other.sections[section].clone();
            rejects(&pack(&mixed), &config, &permitted);
        }
        let mut mixed = good.clone();
        mixed.sections[0] = other.sections[0].clone();
        mixed.sections[3] = other.sections[3].clone();
        rejects(&pack(&mixed), &config, &permitted);
    }
}

#[test]
fn canonical_current_oracle_metadata_price_history_and_owner_presence_are_compared() {
    for native in [false, true] {
        let (mut state, config, permitted) = fixture(native);
        history(&mut state, native, 102);
        let good = unpack(&state.snapshot_bytes().unwrap());
        for kind in 0..4 {
            let mut owners = SourceCatalogOwners::from_checkpoint_bytes(&good.sections[3]).unwrap();
            let oracle = owners.oracles.as_mut().unwrap();
            match kind {
                0 => {
                    let mut rows = oracle.rows("oracleList").unwrap().to_vec();
                    rows[1]
                        .insert("invented", s("CRC does not authorize metadata"))
                        .unwrap();
                    oracle.import_rows("oracleList", rows).unwrap();
                }
                1 => {
                    let mut rows = oracle.rows("oracleData").unwrap().to_vec();
                    rows.last_mut()
                        .unwrap()
                        .insert("data", obj(&[("price", n(999.0))]))
                        .unwrap();
                    oracle.import_rows("oracleData", rows).unwrap();
                }
                2 => oracle.import_rows("oracleData", vec![]).unwrap(),
                _ => owners.oracles = None,
            }
            let mut mixed = good.clone();
            mixed.sections[3] = owners.checkpoint_bytes().unwrap();
            assert!(
                mixed.sections[3] != good.sections[3],
                "poisoned current catalogs must actually differ"
            );
            rejects(&pack(&mixed), &config, &permitted);
        }
    }
}

#[test]
fn rejected_audit_receipts_bootstraps_and_source_layers_cannot_be_stripped() {
    for native in [false, true] {
        let (mut state, config, permitted) = fixture(native);
        history(&mut state, native, 102);
        let bytes = state.snapshot_bytes().unwrap();
        let good = unpack(&bytes);
        let mut mixed = good.clone();
        let needle = b"?source-only-rejected-evidence";
        let at = mixed.sections[1]
            .windows(needle.len())
            .position(|w| w == needle)
            .unwrap();
        mixed.sections[1][at] = b'0';
        rejects(&pack(&mixed), &config, &permitted);
        let mut mixed = good.clone();
        let bootstrap = u32::from_be_bytes(mixed.sections[1][..4].try_into().unwrap()) as usize;
        let at = 4 + bootstrap;
        mixed.sections[1][at..at + 4].copy_from_slice(&0u32.to_be_bytes());
        mixed.sections[1].truncate(at + 4);
        rejects(&pack(&mixed), &config, &permitted);
        let mut mixed = good.clone();
        let last = mixed.sections[1].len() - 1;
        mixed.sections[1][last] ^= 1;
        rejects(&pack(&mixed), &config, &permitted);
        for section in [2, 3] {
            let mut mixed = good.clone();
            mixed.sections[section].clear();
            rejects(&pack(&mixed), &config, &permitted);
        }
        let mut wrong = config.clone();
        wrong
            .source_consensus
            .as_mut()
            .unwrap()
            .catalog_bootstrap_commitment
            .as_mut()
            .unwrap()[0] ^= 1;
        rejects(&bytes, &wrong, &permitted);
        assert!(
            EconomicState::from_snapshot_bytes(&bytes, &config, &a("mallory"), &permitted).is_err()
        );
        rejects(&bytes, &config, &BTreeSet::new());
        rejects(&good.sections[0], &config, &permitted);
    }
}
