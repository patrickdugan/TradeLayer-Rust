//! Actual source Types 5/6 over funded catalog + Joint + Economic owners.
//! Source defects and the named conserving settlement repair stay distinct.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::clearlist_source_owner::ClearlistSourceOwner;
use tradelayer_core::consensus_source::{
    SourceBlockMode, SourceBlockReport, SourceConsensusConfig, SourceConsensusPolicy,
    SourceDisposition, SourceTransaction,
};
use tradelayer_core::economic::{EconomicConfig, EconomicInput, EconomicState, SettlementPolicy};
use tradelayer_core::economic_validity::{EconomicSourceBackend, EconomicValidityError};
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::property_catalog::PropertyCatalog;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::source_catalogs::{bootstrap_commitment, SourceCatalogOwners};
use tradelayer_core::spot::{CancelSelector, SpotIntent, SpotPair};
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::types_source::{decode_payload_unverified, TypesOutcome};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceOutcome, SourceRuleError,
    SourceValue as V,
};

const ATOMS: u128 = 100_000_000;
const MAKER: &str = "1BoatSLRHtKNngkdXEeobR76b53LETtpyT";
const TAKER: &str = "1BitcoinEaterAddressDontSendf59kuE";
fn a(value: &str) -> AccountId {
    value.try_into().unwrap()
}
fn p(value: u32) -> PropertyId {
    value.try_into().unwrap()
}
fn n(value: f64) -> V {
    V::number(value)
}
fn s(value: &str) -> V {
    V::String(value.into())
}
fn id(value: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&value.to_be_bytes());
    TxId::from_bytes(bytes)
}
fn header(height: u64, parent: BlockId) -> BlockHeader {
    let mut bytes = [0x68; 32];
    bytes[24..].copy_from_slice(&height.to_be_bytes());
    BlockHeader {
        height,
        id: BlockId(bytes),
        parent,
    }
}
fn permitted() -> BTreeSet<PropertyId> {
    BTreeSet::from([p(7), p(8)])
}
struct NoExternal;
impl SourceBackend for NoExternal {
    fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        Err(SourceRuleError::UnknownDependency)
    }
}
fn base() -> (EconomicConfig, JointProtocolState, SourceCatalogOwners) {
    let mut ledger = Ledger::new();
    let mut properties = PropertyCatalog::new();
    for (number, owner, ticker) in [(7, MAKER, "BASE"), (8, TAKER, "QUOTE")] {
        ledger
            .apply(
                id(900 + u64::from(number)),
                Operation::CreateProperty {
                    id: p(number),
                    issuer: a(owner),
                    kind: PropertyKind::Fixed,
                    initial_atoms: 100 * ATOMS,
                },
            )
            .unwrap();
        let result = properties
            .execute(
                "addProperty",
                &[
                    n(f64::from(number)),
                    s(ticker),
                    n(100.0),
                    s("Fixed"),
                    n(0.0),
                    s(owner),
                    s(owner),
                ],
                &mut NoExternal,
            )
            .unwrap();
        assert_eq!(result.outcome, SourceOutcome::Returned(V::Undefined));
    }
    let joint = JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Bitcoin,
        a("admin"),
        ledger,
        permitted(),
    )
    .unwrap();
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
        source_consensus: None,
    };
    (
        config,
        joint,
        SourceCatalogOwners::new(properties, ClearlistSourceOwner::new()),
    )
}
fn fixture() -> EconomicState {
    let (mut config, joint, owners) = base();
    config.source_consensus = Some(SourceConsensusConfig {
        bootstrap_commitment: EconomicState::source_bootstrap_commitment(&joint).unwrap(),
        policy: SourceConsensusPolicy::ConservingSequentialV1,
        catalog_bootstrap_commitment: Some(bootstrap_commitment(
            &owners.checkpoint_bytes().unwrap(),
        )),
    });
    EconomicState::from_joint_with_source_catalogs_unverified(config, joint, owners).unwrap()
}
fn tx(number: u64, payload: String, sender: &str) -> SourceTransaction {
    SourceTransaction {
        position: 0,
        txid: id(number),
        payload,
        marker: s("tl"),
        sender: s(sender),
        reference: V::Null,
        sender_amount: n(1.0),
        reference_amount: n(0.0),
    }
}
fn apply(state: &mut EconomicState, transaction: SourceTransaction) -> SourceBlockReport {
    let previous = state.source_consensus_state().frames.last();
    let next = header(
        previous.map_or(1, |f| f.header.height + 1),
        previous.map_or(state.config().network.genesis(), |f| f.header.id),
    );
    state
        .apply_source_block_unverified(next, SourceBlockMode::IndexedReplay, &[transaction])
        .unwrap()
}
fn activate(state: &mut EconomicState) {
    assert_eq!(
        apply(state, tx(1, format!("05;6,{}", "11".repeat(32)), "admin")).accepted,
        1
    );
}
fn base36(mut number: u128) -> String {
    let mut text = Vec::new();
    while number != 0 {
        text.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(number % 36) as usize] as char);
        number /= 36;
    }
    text.into_iter().rev().collect()
}
fn trade(
    number: u64,
    sender: &str,
    offered: u32,
    desired: u32,
    amount: u128,
    flags: &str,
) -> SourceTransaction {
    let amount = base36(amount * ATOMS);
    tx(
        number,
        format!("5{offered},{desired},{amount},{amount},{flags}"),
        sender,
    )
}
fn parent(state: &EconomicState) -> Tip {
    state.source_consensus_state().frames.last().map_or(
        Tip {
            height: 0,
            id: state.config().network.genesis(),
        },
        |f| Tip {
            height: f.header.height,
            id: f.header.id,
        },
    )
}
fn restored(state: &EconomicState) -> EconomicState {
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x0a");
    let result =
        EconomicState::from_snapshot_bytes(&bytes, state.config(), &a("admin"), &permitted())
            .unwrap();
    assert_eq!(&result, state);
    assert_eq!(result.snapshot_bytes().unwrap(), bytes);
    result
}
fn pending(backend: &mut EconomicSourceBackend<'_>, target: &str, arguments: Vec<V>) -> V {
    match backend
        .invoke(&DependencyCall {
            target: target.into(),
            arguments,
        })
        .unwrap()
    {
        DependencyReturn::Pending(value) => value,
        other => panic!("expected actual async source query: {other:?}"),
    }
}

#[test]
fn source_spot_reserves_partial_fills_cancels_and_restores_full_original_ids() {
    let mut state = fixture();
    activate(&mut state);
    assert_eq!(
        apply(&mut state, trade(2, MAKER, 7, 8, 10, "0,0")).accepted,
        1
    );
    assert_eq!(state.ledger().available_atoms(p(7), &a(MAKER)), 90 * ATOMS);
    let pair = SpotPair::new(p(7), p(8)).unwrap();
    assert_eq!(state.joint().spot().open_orders(pair)[0].txid, id(2));
    state = restored(&state);
    assert_eq!(
        apply(&mut state, trade(3, TAKER, 8, 7, 4, "0,0")).accepted,
        1
    );
    assert_eq!(state.joint().spot().fills().len(), 1);
    let fill = &state.joint().spot().fills()[0];
    assert_eq!(
        (fill.sell_txid, fill.buy_txid, fill.trigger_txid),
        (id(2), id(3), id(3))
    );
    assert_eq!(state.ledger().available_atoms(p(8), &a(MAKER)), 4 * ATOMS);
    assert_eq!(state.ledger().available_atoms(p(7), &a(TAKER)), 4 * ATOMS);
    let before = state.snapshot_bytes().unwrap();
    {
        let mut backend =
            EconomicSourceBackend::from_state_unverified(&state, parent(&state)).unwrap();
        let V::Array(rows) = pending(
            &mut backend,
            "Orderbook.getOrdersForAddress",
            vec![s(MAKER), V::Null, n(7.0), n(8.0)],
        ) else {
            panic!()
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("amountOffered"), n(6.0));
        assert_eq!(rows[0].get("amountExpected"), n(6.0));
        assert_eq!(rows[0].get("price"), n(1.0));
        assert_eq!(
            rows[0].get("txid"),
            s("0000002"),
            "source truncates its display field only"
        );
        for args in [
            vec![V::Undefined, V::Null, n(7.0), n(8.0)],
            vec![s(MAKER), V::Null, n(8.0), n(7.0)],
        ] {
            assert_eq!(
                pending(&mut backend, "Orderbook.getOrdersForAddress", args),
                V::Array(vec![])
            );
        }
        let V::Array(rows) = pending(&mut backend, "Tally.getAddressBalances", vec![s(MAKER)])
        else {
            panic!()
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("propertyId"), s("7"));
        assert_eq!(rows[0].get("ticker"), s("BASE"));
        assert_eq!(rows[0].get("available"), n(90.0));
        assert_eq!(rows[0].get("reserved"), n(6.0));
        assert_eq!(rows[0].get("amount"), n(96.0));
        assert_eq!(rows[1].get("ticker"), s("QUOTE"));
        assert_eq!(
            pending(
                &mut backend,
                "Tally.getAddressBalances",
                vec![s("never-funded")]
            ),
            V::Array(vec![])
        );
    }
    assert_eq!(state.snapshot_bytes().unwrap(), before);
    state = restored(&state);
    // The decoder's cancelAll field also becomes price=.125. The named
    // conserving mapper applies owned/directional All; source price matching
    // and refund defects are documented separately, never asserted as parity.
    let cancel = tx(4, "60,7,8,1,,sell".into(), MAKER);
    let mut backend = EconomicSourceBackend::from_state_unverified(&state, parent(&state)).unwrap();
    let evaluation = decode_payload_unverified(
        &cancel.arguments(4, SourceBlockMode::IndexedReplay),
        &mut backend,
    )
    .unwrap();
    let TypesOutcome::Returned(params) = &evaluation.outcome else {
        panic!("{evaluation:?}")
    };
    assert_eq!(params.get("valid"), V::Bool(true));
    assert_eq!(params.get("fromAddress"), V::Undefined);
    assert_eq!(params.get("contractId"), V::Undefined);
    assert_eq!(params.get("cancelParams").get("price"), n(0.125));
    let trace = &evaluation
        .validity
        .unwrap()
        .settlement
        .evaluation
        .dependencies;
    let query = trace
        .iter()
        .find(|d| d.call.target == "Orderbook.getOrdersForAddress")
        .unwrap();
    assert_eq!(query.call.arguments[0], V::Undefined);
    assert_eq!(query.returned, DependencyReturn::Pending(V::Array(vec![])));
    assert_eq!(apply(&mut state, cancel).accepted, 1);
    assert_eq!(state.joint().spot().open_count(), 0);
    assert_eq!(state.ledger().available_atoms(p(7), &a(MAKER)), 96 * ATOMS);
    assert_eq!(state.ledger().available_atoms(p(8), &a(TAKER)), 96 * ATOMS);
    restored(&state).verify().unwrap();
    let wrong = Tip {
        height: parent(&state).height,
        id: BlockId([0x93; 32]),
    };
    assert!(matches!(
        EconomicSourceBackend::from_state_unverified(&state, wrong),
        Err(EconomicValidityError::WrongHead)
    ));
}

#[test]
fn source_cancellation_keeps_unawaited_empty_query_and_real_decoder_failures() {
    let mut state = fixture();
    activate(&mut state);
    // A nonexistent sender order does not invalidate source: Promise.length
    // is undefined. The actual conserving owner still rejects NoMatchingOrder.
    let report = apply(&mut state, tx(2, "60,7,8,1,,sell".into(), MAKER));
    assert_eq!(
        report.receipts[0].disposition,
        SourceDisposition::EconomicRejected,
        "{:?}",
        report.receipts
    );
    assert!(report.receipts[0].reason.contains("NoMatchingOrder"));
    let report = apply(&mut state, tx(3, "60,7,8,1".into(), MAKER));
    assert_eq!(report.receipts[0].disposition, SourceDisposition::Invalid);
    assert!(report.receipts[0].reason.contains("Invalid side parameter"));
    let report = apply(
        &mut state,
        tx(4, format!("60,7,8,0,{}", "ab".repeat(32)), MAKER),
    );
    assert_eq!(report.receipts[0].disposition, SourceDisposition::Invalid);
    assert!(report.receipts[0]
        .reason
        .contains("TxId parameter deprecated"));
    let report = apply(&mut state, tx(5, "60,7,8,0,,sell".into(), MAKER));
    assert_eq!(
        report.receipts[0].disposition,
        SourceDisposition::UnsupportedMapping
    );
    assert_eq!(state.joint().spot().open_count(), 0);
    restored(&state);
}

#[test]
fn source_post_and_stop_forms_reach_real_owner_and_fail_closed() {
    let mut state = fixture();
    activate(&mut state);
    for (number, flags) in [(2, "1,0"), (3, "0,1"), (4, "1,1")] {
        let report = apply(&mut state, trade(number, MAKER, 7, 8, 10, flags));
        assert_eq!(
            report.receipts[0].disposition,
            SourceDisposition::EconomicRejected,
            "{:?}",
            report.receipts
        );
        assert!(report.receipts[0].reason.contains("UnsupportedFlag"));
    }
    assert_eq!(state.ledger().available_atoms(p(7), &a(MAKER)), 100 * ATOMS);
    assert_eq!(state.joint().spot().open_count(), 0);
    restored(&state);
}

#[test]
fn typed_spot_owner_checkpoint_also_retains_reserves_and_cancellation() {
    let (config, joint, _) = base();
    let mut state = EconomicState::from_joint_unverified(config, joint).unwrap();
    let joint_tx = |number, actor, operation| {
        EconomicInput::Joint(UnverifiedJointTx {
            txid: id(number),
            actor: a(actor),
            operation,
        })
    };
    state
        .apply_block_unverified(
            1,
            vec![joint_tx(
                1,
                "admin",
                JointOperation::Registry(RegistryAction::Activate {
                    types: vec![5, 6],
                    code_hash: [0x11; 32],
                }),
            )],
        )
        .unwrap();
    state
        .apply_block_unverified(
            2,
            vec![joint_tx(
                2,
                MAKER,
                JointOperation::SpotPlace(SpotIntent {
                    offered: p(7),
                    desired: p(8),
                    offered_atoms: 10 * ATOMS as u64,
                    expected_atoms: 10 * ATOMS as u64,
                    post: false,
                    stop: false,
                }),
            )],
        )
        .unwrap();
    let bytes = state.snapshot_bytes().unwrap();
    let mut next =
        EconomicState::from_snapshot_bytes(&bytes, state.config(), &a("admin"), &permitted())
            .unwrap();
    assert_eq!(next, state);
    assert_eq!(
        next.joint()
            .spot()
            .open_orders(SpotPair::new(p(7), p(8)).unwrap())[0]
            .txid,
        id(2)
    );
    next.apply_block_unverified(
        3,
        vec![joint_tx(
            3,
            MAKER,
            JointOperation::SpotCancel {
                offered: p(7),
                desired: p(8),
                is_contract: false,
                selector: CancelSelector::All,
            },
        )],
    )
    .unwrap();
    assert_eq!(next.ledger().available_atoms(p(7), &a(MAKER)), 100 * ATOMS);
    assert_eq!(next.joint().spot().open_count(), 0);
    next.verify().unwrap();
}
