//! Actual source dispatch, owned metadata and the sole funded Ledger together.
//! Raw actors/L1 inputs remain unverified; no fixture validity reply is injected.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::clearlist_source_owner::ClearlistSourceOwner;
use tradelayer_core::consensus_source::{
    SourceBlockMode, SourceBlockReport, SourceConsensusConfig, SourceConsensusPolicy,
    SourceDisposition, SourceTransaction,
};
use tradelayer_core::economic::{EconomicConfig, EconomicError, EconomicState, SettlementPolicy};
use tradelayer_core::joint::JointProtocolState;
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::property_catalog::PropertyCatalog;
use tradelayer_core::source_catalogs::{bootstrap_commitment, SourceCatalogOwners};
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceOutcome, SourceRuleError,
    SourceValue as V,
};

const ATOMS: u128 = 100_000_000;
const ISSUER: &str = "1BoatSLRHtKNngkdXEeobR76b53LETtpyT";
const HOLDER: &str = "1BitcoinEaterAddressDontSendf59kuE";
const BACKUP: &str = "1CounterpartyXXXXXXXXXXXXXXXUWLpVr";
fn a(value: &str) -> AccountId {
    value.try_into().unwrap()
}
fn p(id: u32) -> PropertyId {
    id.try_into().unwrap()
}
fn id(number: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&number.to_be_bytes());
    TxId::from_bytes(bytes)
}
fn block_id(number: u64) -> BlockId {
    let mut bytes = [0x79; 32];
    bytes[24..].copy_from_slice(&number.to_be_bytes());
    BlockId(bytes)
}
fn s(value: &str) -> V {
    V::String(value.into())
}
fn n(value: f64) -> V {
    V::number(value)
}
struct NoTally;
impl SourceBackend for NoTally {
    fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        Err(SourceRuleError::UnknownDependency)
    }
}
fn initial_catalog() -> SourceCatalogOwners {
    let mut properties = PropertyCatalog::new();
    let result = properties
        .execute(
            "addProperty",
            &[
                n(7.0),
                s("BASE"),
                n(100.0),
                s("Fixed"),
                n(0.0),
                s(ISSUER),
                s(BACKUP),
            ],
            &mut NoTally,
        )
        .unwrap();
    assert_eq!(result.outcome, SourceOutcome::Returned(V::Undefined));
    SourceCatalogOwners::new(properties, ClearlistSourceOwner::new())
}
fn fixture() -> EconomicState {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(900),
            Operation::CreateProperty {
                id: p(7),
                issuer: a(ISSUER),
                kind: PropertyKind::Fixed,
                initial_atoms: 100 * ATOMS,
            },
        )
        .unwrap();
    let joint = JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Bitcoin,
        a("admin"),
        ledger,
        BTreeSet::from([p(7)]),
    )
    .unwrap();
    let owners = initial_catalog();
    let config = EconomicConfig {
        network: TradeLayerMainnet::Bitcoin,
        contract: 1.try_into().unwrap(),
        collateral: p(7),
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
        native_entitlements: None,
        insurance: None,
        source_consensus: Some(SourceConsensusConfig {
            bootstrap_commitment: EconomicState::source_bootstrap_commitment(&joint).unwrap(),
            policy: SourceConsensusPolicy::ConservingSequentialV1,
            catalog_bootstrap_commitment: Some(bootstrap_commitment(
                &owners.checkpoint_bytes().unwrap(),
            )),
        }),
    };
    EconomicState::from_joint_with_source_catalogs_unverified(config, joint, owners).unwrap()
}
fn tx(position: u32, number: u64, payload: &str, sender: &str) -> SourceTransaction {
    SourceTransaction {
        position,
        txid: id(number),
        payload: payload.into(),
        marker: s("tl"),
        sender: s(sender),
        reference: V::Null,
        sender_amount: n(1.0),
        reference_amount: n(0.0),
    }
}
fn apply(
    state: &mut EconomicState,
    number: u64,
    transactions: &[SourceTransaction],
) -> SourceBlockReport {
    let previous = state.source_consensus_state().frames.last();
    let header = BlockHeader {
        height: previous.map_or(1, |f| f.header.height + 1),
        id: block_id(number),
        parent: previous.map_or(state.config().network.genesis(), |f| f.header.id),
    };
    state
        .apply_source_block_unverified(header, SourceBlockMode::IndexedReplay, transactions)
        .unwrap()
}
fn activation() -> SourceTransaction {
    tx(
        0,
        1,
        &format!("01;7;8;9;11;12,{}", "11".repeat(32)),
        "admin",
    )
}
fn activate(state: &mut EconomicState) {
    let report = apply(state, 1, &[activation()]);
    assert_eq!(report.accepted, 1);
}
fn issue(position: u32, number: u64, ticker: &str, managed: bool) -> SourceTransaction {
    tx(
        position,
        number,
        &format!("1a,{ticker},,{},{}", u8::from(managed), BACKUP),
        ISSUER,
    )
}
fn base36(mut number: u128) -> String {
    if number == 0 {
        return "0".into();
    }
    let mut chars = Vec::new();
    while number > 0 {
        chars.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(number % 36) as usize] as char);
        number /= 36
    }
    chars.into_iter().rev().collect()
}
fn grant(
    position: u32,
    number: u64,
    amount: u128,
    recipient: &str,
    sender: &str,
) -> SourceTransaction {
    tx(
        position,
        number,
        &format!("b8,{},{}", base36(amount * ATOMS), recipient),
        sender,
    )
}
fn redeem(position: u32, number: u64, amount: u128, sender: &str) -> SourceTransaction {
    tx(
        position,
        number,
        &format!("c8,{}", base36(amount * ATOMS)),
        sender,
    )
}
fn property(state: &EconomicState, id: u32) -> V {
    state
        .source_catalog_owners()
        .unwrap()
        .properties
        .clone()
        .get_property_data(n(id as f64))
        .unwrap()
}
fn property_document(state: &EconomicState) -> Option<V> {
    state
        .source_catalog_owners()
        .unwrap()
        .properties
        .document()
        .cloned()
}
fn roundtrip(state: &EconomicState) -> EconomicState {
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x0a");
    let restored = EconomicState::from_snapshot_bytes(
        &bytes,
        state.config(),
        &a("admin"),
        &BTreeSet::from([p(7)]),
    )
    .unwrap();
    assert_eq!(&restored, state);
    assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
    restored
}

#[test]
fn actual_catalog_allocates_after_bootstrap_and_managed_helpers_mutate_only_the_sole_ledger() {
    let mut state = fixture();
    activate(&mut state);
    let report = apply(&mut state, 2, &[issue(0, 2, "OWNED", true)]);
    assert_eq!(
        report.receipts[0].disposition,
        SourceDisposition::Applied,
        "{:?}",
        report.receipts
    );
    assert_eq!(
        state.ledger().property(p(8)).unwrap().kind,
        PropertyKind::Managed
    );
    assert_eq!(state.ledger().available_atoms(p(8), &a(ISSUER)), 10 * ATOMS);
    assert_eq!(property(&state, 8).get("ticker"), s("OWNED"));
    assert_eq!(property(&state, 8).get("issuer"), s(ISSUER));
    assert_eq!(property(&state, 8).get("backupAddress"), s(BACKUP));
    assert_eq!(property(&state, 8).get("type"), n(2.0));
    assert_eq!(property(&state, 8).get("totalInCirculation"), n(10.0));
    assert_eq!(
        state
            .source_catalog_owners()
            .unwrap()
            .properties
            .cache_entries()
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>(),
        vec![n(7.0), n(8.0)]
    );

    let report = apply(
        &mut state,
        3,
        &[grant(0, 3, 2, HOLDER, ISSUER), redeem(1, 4, 3, ISSUER)],
    );
    assert_eq!(report.accepted, 2, "{:?}", report.receipts);
    assert_eq!(
        state.ledger().property(p(8)).unwrap().total_supply_atoms,
        9 * ATOMS
    );
    assert_eq!(state.ledger().available_atoms(p(8), &a(ISSUER)), 7 * ATOMS);
    assert_eq!(state.ledger().available_atoms(p(8), &a(HOLDER)), 2 * ATOMS);
    assert_eq!(property(&state, 8).get("totalInCirculation"), n(9.0));
    assert_eq!(
        state.ledger().available_atoms(p(7), &a(ISSUER)),
        100 * ATOMS
    );
    assert!(state.ledger().is_processed(id(2)));
    assert!(state.ledger().is_processed(id(3)));
    assert!(state.ledger().is_processed(id(4)));
    state.verify().unwrap();
    let mut restored = roundtrip(&state);
    let continuation = [grant(0, 5, 1, HOLDER, ISSUER)];
    assert_eq!(
        apply(&mut state, 4, &continuation),
        apply(&mut restored, 4, &continuation)
    );
    assert_eq!(state, restored);
    assert_eq!(property(&state, 8).get("totalInCirculation"), n(10.0));
}

#[test]
fn duplicate_ticker_replay_wrong_actor_and_insufficient_holder_balance_preserve_business_state() {
    let mut state = fixture();
    activate(&mut state);
    apply(&mut state, 2, &[issue(0, 2, "OWNED", true)]);
    apply(&mut state, 3, &[grant(0, 3, 2, HOLDER, ISSUER)]);
    let ledger = state.ledger().clone();
    let document = property_document(&state);
    let report = apply(
        &mut state,
        4,
        &[
            issue(0, 10, "OWNED", false),
            grant(1, 11, 1, HOLDER, "mallory"),
            redeem(2, 12, 1, HOLDER),
            redeem(3, 13, 11, ISSUER),
            tx(4, 15, &format!("8{HOLDER},0,0,1,8,0"), ISSUER),
            grant(5, 3, 9, HOLDER, ISSUER),
        ],
    );
    assert_eq!(report.accepted, 0, "{:?}", report.receipts);
    assert_eq!(report.rejected, 5);
    assert_eq!(report.already_processed, 1);
    assert_eq!(
        report
            .receipts
            .iter()
            .map(|r| r.disposition)
            .collect::<Vec<_>>(),
        vec![
            SourceDisposition::Invalid,
            SourceDisposition::Invalid,
            SourceDisposition::Invalid,
            SourceDisposition::Invalid,
            SourceDisposition::Thrown,
            SourceDisposition::AlreadyProcessed
        ]
    );
    assert_eq!(state.ledger(), &ledger);
    assert_eq!(property_document(&state), document);
    assert_eq!(property(&state, 8).get("totalInCirculation"), n(12.0));
    // Rejected candidates remain in the source journal and are replay-blocked.
    let report = apply(
        &mut state,
        5,
        &[issue(0, 10, "CHANGED", false), issue(1, 14, "NEXT", false)],
    );
    assert_eq!(report.already_processed, 1);
    assert_eq!(report.accepted, 1, "{:?}", report.receipts);
    assert_eq!(property(&state, 9).get("ticker"), s("NEXT"));
    assert!(state.ledger().property(p(10)).is_none());
    roundtrip(&state);
}

#[test]
fn prior_activation_candidate_rollback_does_not_consume_the_catalog_id() {
    let mut state = fixture();
    let before = property_document(&state);
    let report = apply(&mut state, 1, &[activation(), issue(1, 2, "EARLY", true)]);
    assert_eq!(report.accepted, 1, "{:?}", report.receipts);
    assert_eq!(
        report.receipts[1].disposition,
        SourceDisposition::EconomicRejected
    );
    assert_eq!(property_document(&state), before);
    assert!(state.ledger().property(p(8)).is_none());
    let report = apply(
        &mut state,
        2,
        &[issue(0, 2, "EARLY", true), issue(1, 3, "LATER", true)],
    );
    assert_eq!(report.already_processed, 1);
    assert_eq!(report.accepted, 1, "{:?}", report.receipts);
    assert_eq!(property(&state, 8).get("ticker"), s("LATER"));
    roundtrip(&state);
}

#[test]
fn complete_catalog_bootstrap_pin_and_checkpoint_expectations_are_required() {
    let state = fixture();
    let mut other = initial_catalog();
    other.properties.replace_document(None).unwrap();
    assert!(matches!(
        EconomicState::from_joint_with_source_catalogs_unverified(
            state.config().clone(),
            state.joint().clone(),
            other
        ),
        Err(EconomicError::InvalidConfig)
    ));
    let bytes = state.snapshot_bytes().unwrap();
    let mut changed = state.config().clone();
    changed
        .source_consensus
        .as_mut()
        .unwrap()
        .catalog_bootstrap_commitment
        .as_mut()
        .unwrap()[0] ^= 1;
    assert!(EconomicState::from_snapshot_bytes(
        &bytes,
        &changed,
        &a("admin"),
        &BTreeSet::from([p(7)])
    )
    .is_err());
    let mut corrupted = bytes;
    let index = corrupted.len() / 2;
    corrupted[index] ^= 1;
    assert!(EconomicState::from_snapshot_bytes(
        &corrupted,
        state.config(),
        &a("admin"),
        &BTreeSet::from([p(7)])
    )
    .is_err());
    roundtrip(&state);
}

#[test]
fn type7_and_type9_mutate_the_actual_clearlist_owner_without_fabricating_registry_rows() {
    let mut state = fixture();
    activate(&mut state);
    let report = apply(
        &mut state,
        2,
        &[
            tx(
                0,
                20,
                &format!("7{BACKUP},Actual,https://example,owned source metadata"),
                ISSUER,
            ),
            tx(1, 21, &format!("90,1,{HOLDER},approved"), ISSUER),
            tx(2, 22, &format!("90,0,{HOLDER},ca"), HOLDER),
        ],
    );
    assert_eq!(report.accepted, 3, "{:?}", report.receipts);
    let owners = state.source_catalog_owners().unwrap();
    assert_eq!(owners.clearlists.rows("clearlists").unwrap().len(), 1);
    let rows = owners.clearlists.rows("attestations").unwrap();
    // Source uses only address as the row key: list0 self-cert overwrites the
    // previous list1 attestation at the same address.
    assert_eq!(rows.len(), 1);
    assert_eq!(state.joint().registry().whitelist_count(), 0);
    assert_eq!(rows[0].get("data").get("address"), s(HOLDER));
    assert_eq!(rows[0].get("data").get("listId"), n(0.0));
    assert_eq!(rows[0].get("data").get("data"), s("CA"));
    assert_eq!(
        state.ledger().available_atoms(p(7), &a(ISSUER)),
        100 * ATOMS
    );
    roundtrip(&state);
}
