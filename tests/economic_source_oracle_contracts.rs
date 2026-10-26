//! Genuine raw13/14/16/18 over actual oracle + contract owners. V1 retains
//! unavailable reads; V2 reaches the named conserving settlement owner.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::clearlist_source_owner::ClearlistSourceOwner;
use tradelayer_core::consensus_source::{
    SourceBlockMode, SourceBlockReport, SourceConsensusConfig, SourceConsensusPolicy,
    SourceDisposition, SourceTransaction,
};
use tradelayer_core::contract_registry::{ContractAsset, ContractParameters};
use tradelayer_core::economic::{
    contract_definition_commitment, ContractBindingConfig, ContractRiskPolicy, CreateContractInput,
    EconomicConfig, EconomicState, FundingConfig, FundingPolicy, FundingStatus, MarkConfig,
    MarkEventKind, MarkPolicy, SettlementPolicy,
};
use tradelayer_core::economic_validity::EconomicSourceBackend;
use tradelayer_core::issuance_intent::JsonValue;
use tradelayer_core::joint::JointProtocolState;
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::oracle_source_owner::OracleSourceOwner;
use tradelayer_core::property_catalog::PropertyCatalog;
use tradelayer_core::source_catalogs::{bootstrap_commitment, SourceCatalogOwners};
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::types_source::{decode_payload_unverified, TypesError, TypesOutcome};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceOutcome, SourceRuleError,
    SourceValue as V,
};
const U: u128 = 100_000_000;
const A: &str = "1BoatSLRHtKNngkdXEeobR76b53LETtpyT";
const B: &str = "1BitcoinEaterAddressDontSendf59kuE";
const BACKUP: &str = "1CounterpartyXXXXXXXXXXXXXXXUWLpVr";
fn a(s: &str) -> AccountId {
    s.try_into().unwrap()
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn n(v: f64) -> V {
    V::number(v)
}
fn s(v: &str) -> V {
    V::String(v.into())
}
fn id(v: u64) -> TxId {
    let mut b = [0; 32];
    b[24..].copy_from_slice(&v.to_be_bytes());
    TxId::from_bytes(b)
}
fn creation() -> CreateContractInput {
    CreateContractInput {
        txid: id(30),
        issuer: a(A),
        parameters: ContractParameters {
            native: false,
            underlying_oracle_id: Some(1),
            on_chain_data: vec![],
            notional_property_id: ContractAsset::BaseChain,
            notional_value: 1.into(),
            collateral_property_id: ContractAsset::Property(p()),
            leverage: 10.into(),
            expiry_period: 0,
            series: 1,
            inverse: false,
            fee: 0.into(),
            whitelist: None,
            ticker: None,
        },
    }
}
struct NoTally;
impl SourceBackend for NoTally {
    fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        Err(SourceRuleError::UnknownDependency)
    }
}
fn fixture_with_policy(policy: SourceConsensusPolicy) -> EconomicState {
    let network = TradeLayerMainnet::Bitcoin;
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(900),
            Operation::CreateProperty {
                id: p(),
                issuer: a(A),
                kind: PropertyKind::Fixed,
                initial_atoms: 2000 * U,
            },
        )
        .unwrap();
    ledger
        .apply(
            id(901),
            Operation::Transfer {
                id: p(),
                from: a(A),
                to: a(B),
                amount_atoms: 1000 * U,
            },
        )
        .unwrap();
    let joint =
        JointProtocolState::from_bootstrap_unverified(network, a(A), ledger, BTreeSet::from([p()]))
            .unwrap();
    let mut properties = PropertyCatalog::new();
    assert_eq!(
        properties
            .execute(
                "addProperty",
                &[
                    n(7.0),
                    s("COLL"),
                    n(2000.0),
                    s("Fixed"),
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
    let catalogs = SourceCatalogOwners::new(properties, ClearlistSourceOwner::new())
        .with_oracles(OracleSourceOwner::new());
    let config = EconomicConfig {
        network,
        contract: 1.try_into().unwrap(),
        collateral: p(),
        inverse: false,
        notional: 1.into(),
        initial_margin_per_contract: 0.into(),
        fee_route: FeeRoute::Oracle(1),
        policy: SettlementPolicy::ConservingV1,
        funding: Some(FundingConfig {
            oracle_id: 1,
            max_staleness_blocks: 192,
            policy: FundingPolicy::ConservingFundingV1,
        }),
        marking: Some(MarkConfig {
            oracle_id: 1,
            policy: MarkPolicy::ConservingMarkBasisV1,
        }),
        liquidation: None,
        amm: None,
        contract_binding: Some(ContractBindingConfig {
            creation_txid: creation().txid,
            definition_hash: contract_definition_commitment(&creation()).unwrap(),
            bootstrap_commitment: EconomicState::contract_bootstrap_commitment(&joint).unwrap(),
            policy: ContractRiskPolicy::SourceContextsConservingV1,
        }),
        native_entitlements: None,
        insurance: None,
        source_consensus: Some(SourceConsensusConfig {
            bootstrap_commitment: EconomicState::source_bootstrap_commitment(&joint).unwrap(),
            catalog_bootstrap_commitment: Some(bootstrap_commitment(
                &catalogs.checkpoint_bytes().unwrap(),
            )),
            policy,
        }),
    };
    EconomicState::from_joint_with_source_catalogs_unverified(config, joint, catalogs).unwrap()
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
fn apply(state: &mut EconomicState, transactions: &[SourceTransaction]) -> SourceBlockReport {
    let last = state.source_consensus_state().frames.last();
    let height = last.map_or(1, |f| f.header.height + 1);
    let mut bytes = [0x5e; 32];
    bytes[24..].copy_from_slice(&height.to_be_bytes());
    let header = BlockHeader {
        height,
        id: BlockId(bytes),
        parent: last.map_or(state.config().network.genesis(), |f| f.header.id),
    };
    state
        .apply_source_block_unverified(header, SourceBlockMode::IndexedReplay, transactions)
        .unwrap_or_else(|error| panic!("height{height}: {error:?}"))
}
fn base36(mut v: u128) -> String {
    let mut b = vec![];
    while v != 0 {
        b.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(v % 36) as usize]);
        v /= 36;
    }
    b.reverse();
    String::from_utf8(b).unwrap()
}
fn publish(position: u32, number: u64, price: u128) -> SourceTransaction {
    tx(
        position,
        number,
        &format!("e1,{}", base36(price * 10_000)),
        A,
    )
}
fn order(position: u32, number: u64, price: u128) -> SourceTransaction {
    tx(
        position,
        number,
        &format!("i1,{},1,0,0,0,0,0", base36(price * U)),
        A,
    )
}
fn order_for(
    position: u32,
    number: u64,
    actor: &str,
    price: u128,
    quantity: u128,
    sell: bool,
) -> SourceTransaction {
    tx(
        position,
        number,
        &format!(
            "i1,{},{},{},0,0,0,0",
            base36(price * U),
            base36(quantity),
            u8::from(sell)
        ),
        actor,
    )
}
fn available(state: &EconomicState, owner: &str) -> u128 {
    state.ledger().available_atoms(p(), &a(owner))
}
fn position(state: &EconomicState, owner: &str) -> String {
    state
        .positions()
        .get(&a(owner))
        .unwrap()
        .contracts
        .to_string()
}
fn matched_state() -> EconomicState {
    let mut state = opened_metadata();
    // This is genuine raw country self-certification, not supplied validity.
    let report = apply(
        &mut state,
        &[
            tx(0, 80, &format!("90,0,{B},NZ"), B),
            order_for(1, 81, B, 100, 2, true),
            order_for(2, 82, A, 100, 2, false),
        ],
    );
    assert_eq!(report.accepted, 3, "{:?}", report.receipts);
    assert_eq!(position(&state, A), "2");
    assert_eq!(position(&state, B), "-2");
    assert_eq!(state.margin_atoms(&a(A)), 20 * U);
    assert_eq!(state.margin_atoms(&a(B)), 20 * U);
    assert_eq!(state.reserve_atoms(id(81)), 0);
    assert_eq!(state.reserve_atoms(id(82)), 0);
    assert!(state.book().resting().is_empty());
    assert_eq!(state.receipts().len(), 1);
    let receipt = &state.receipts()[0];
    assert_eq!(receipt.buy_txid, id(82));
    assert_eq!(receipt.sell_txid, id(81));
    assert_eq!(receipt.quantity.to_string(), "2");
    assert_eq!(receipt.price.to_string(), "100");
    state.ledger().verify_conservation().unwrap();
    state
}
fn opened_metadata() -> EconomicState {
    opened_metadata_with_policy(SourceConsensusPolicy::ConservingSequentialV2)
}
fn opened_metadata_with_policy(policy: SourceConsensusPolicy) -> EconomicState {
    let mut state = fixture_with_policy(policy);
    assert_eq!(
        apply(
            &mut state,
            &[tx(0, 10, &format!("09;13;14;16;18,{}", "11".repeat(32)), A)]
        )
        .accepted,
        1
    );
    assert_eq!(
        apply(
            &mut state,
            &[tx(
                0,
                20,
                &format!("dINDEX,https://oracle.example,{BACKUP},,1"),
                A
            )]
        )
        .accepted,
        1
    );
    // This raw wire decodes to creation(); that typed value is used only for
    // the independently expected deployment commitment above.
    let report = apply(
        &mut state,
        &[tx(0, 30, "g0,1,,0,1,7,10,0,1,0,0", A), publish(1, 21, 100)],
    );
    assert_eq!(report.accepted, 2, "{:?}", report.receipts);
    state
}
fn restored(state: &EconomicState) -> EconomicState {
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x0a");
    let result =
        EconomicState::from_snapshot_bytes(&bytes, state.config(), &a(A), &BTreeSet::from([p()]))
            .unwrap();
    assert_eq!(&result, state);
    assert_eq!(result.snapshot_bytes().unwrap(), bytes);
    result
}
struct Recording<'a> {
    inner: EconomicSourceBackend<'a>,
    calls: Vec<(DependencyCall, Result<DependencyReturn, SourceRuleError>)>,
}
impl SourceBackend for Recording<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let value = self.inner.invoke(call);
        self.calls.push((call.clone(), value.clone()));
        value
    }
}
fn recording(state: &EconomicState) -> Recording<'_> {
    let last = state.source_consensus_state().frames.last().unwrap();
    Recording {
        inner: EconomicSourceBackend::from_state_unverified(
            state,
            Tip {
                height: last.header.height,
                id: last.header.id,
            },
        )
        .unwrap(),
        calls: vec![],
    }
}

#[test]
fn raw_oracle_and_contract_creation_bind_actual_ticker_price_and_mark_history() {
    let mut state = opened_metadata();
    let accepted = state.contract(1.try_into().unwrap()).unwrap();
    assert_eq!(accepted.creation_txid, creation().txid);
    assert_eq!(
        accepted.definition_hash,
        contract_definition_commitment(&creation()).unwrap()
    );
    assert_eq!(accepted.definition.ticker, "INDEX-OPERP1-1");
    assert_eq!(state.joint().registry().oracle_count(), 0);
    let report = apply(&mut state, &[publish(0, 40, 102)]);
    assert_eq!(report.accepted, 1);
    assert_eq!(
        state
            .funding_state()
            .observations
            .last()
            .unwrap()
            .price
            .to_string(),
        "102"
    );
    let mark = state.mark_state().events.last().unwrap();
    let MarkEventKind::Mark { observation, .. } = &mark.kind else {
        panic!("source publication must produce mark")
    };
    assert_eq!(observation.txid, id(40));
    assert_eq!(observation.price.to_string(), "102");
    assert!(mark.entries.is_empty());
    let owner = state
        .source_catalog_owners()
        .unwrap()
        .oracles
        .as_ref()
        .unwrap();
    assert_eq!(owner.rows("oracleData").unwrap().len(), 2);
    assert_eq!(state.contract_binding_state().frames.len(), 4);
    state.verify().unwrap();
    let mut replay = restored(&state);
    let command = publish(0, 41, 103);
    assert_eq!(
        apply(&mut state, std::slice::from_ref(&command)),
        apply(&mut replay, &[command])
    );
    assert_eq!(state, replay);
    restored(&replay);
}

#[test]
fn v1_raw_leverage_rejection_and_unavailable_margin_replay_are_preserved() {
    let mut state = opened_metadata_with_policy(SourceConsensusPolicy::ConservingSequentialV1);
    let over_band = order(0, 70, 150);
    let mut backend = recording(&state);
    let evaluation = decode_payload_unverified(
        &over_band.arguments(4, SourceBlockMode::IndexedReplay),
        &mut backend,
    )
    .unwrap();
    let TypesOutcome::Returned(params) = evaluation.outcome else {
        panic!()
    };
    assert_eq!(params.get("valid"), V::Bool(false));
    assert!(params
        .get("reason")
        .diagnostic_json()
        .unwrap()
        .contains("Trade price deviates"));
    assert!(backend
        .calls
        .iter()
        .any(|(call, result)| call.target == "Oracle.getOraclePrice"
            && *result == Ok(DependencyReturn::Pending(n(100.0)))));
    assert!(!backend
        .calls
        .iter()
        .any(|(call, _)| call.target.starts_with("Margin.")));
    let within_band = order(1, 71, 100);
    let mut backend = recording(&state);
    assert_eq!(
        decode_payload_unverified(
            &within_band.arguments(4, SourceBlockMode::IndexedReplay),
            &mut backend
        ),
        Err(TypesError::Validity(SourceRuleError::UnknownDependency))
    );
    let (missing, result) = backend.calls.last().unwrap();
    assert_eq!(missing.target, "Margin.loadMarginMap");
    assert_eq!(missing.arguments, vec![n(1.0)]);
    assert_eq!(*result, Err(SourceRuleError::UnknownDependency));
    // The actual dependency is forwarded, including its unavailable result.
    // V1 deliberately preserves its original unavailable-dependency result.
    drop(backend);
    let before = state.ledger().snapshot_bytes().unwrap();
    let report = apply(&mut state, &[over_band, within_band]);
    assert_eq!(report.receipts[0].disposition, SourceDisposition::Invalid);
    assert_eq!(
        report.receipts[1].disposition,
        SourceDisposition::DependencyUnavailable
    );
    assert!(report.receipts[1].reason.contains("UnknownDependency"));
    assert_eq!(state.ledger().snapshot_bytes().unwrap(), before);
    assert!(state.positions().positions().next().is_none());
    assert!(state.book().resting().is_empty());
    assert_eq!(state.margin_atoms(&a(A)), 0);
    let mut replay = restored(&state);
    assert_eq!(
        replay
            .source_consensus_state()
            .frames
            .last()
            .unwrap()
            .receipts,
        report.receipts
    );
    let command = publish(0, 72, 101);
    assert_eq!(
        apply(&mut state, std::slice::from_ref(&command)),
        apply(&mut replay, &[command])
    );
    assert_eq!(state, replay);
    restored(&replay);
}

#[test]
fn v2_genuine_raw_orders_read_pending_country_and_settle_actual_sole_ledger_positions() {
    let mut state = opened_metadata();
    let before = state.ledger().snapshot_bytes().unwrap();
    let denied = order_for(0, 70, B, 100, 2, true);
    let report = apply(&mut state, std::slice::from_ref(&denied));
    assert_eq!(report.receipts[0].disposition, SourceDisposition::Invalid);
    assert!(report.receipts[0].reason.contains("country"));
    assert_eq!(state.ledger().snapshot_bytes().unwrap(), before);
    let report = apply(
        &mut state,
        &[
            tx(0, 80, &format!("90,0,{B},NZ"), B),
            order_for(1, 81, B, 100, 2, true),
            order_for(2, 82, A, 100, 2, false),
        ],
    );
    assert_eq!(report.accepted, 3, "{:?}", report.receipts);
    assert_eq!(position(&state, A), "2");
    assert_eq!(position(&state, B), "-2");
    assert_eq!(state.margin_atoms(&a(A)), 20 * U);
    assert_eq!(state.margin_atoms(&a(B)), 20 * U);
    let mut replay = restored(&state);
    assert_eq!(
        apply(&mut state, std::slice::from_ref(&denied)).already_processed,
        1
    );
    assert_eq!(apply(&mut replay, &[denied]).already_processed, 1);
    assert_eq!(state, replay);
    restored(&state);
}

#[test]
fn v2_ordinary_match_cash_and_positions_equal_independently_executed_full_source() {
    // Read the frozen genuine JS composition at runtime: its full-state trace
    // is large and is not a Rust snapshot or supplied dependency response.
    let fixture = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/source_contract_execution_vectors.tsv"
    ))
    .unwrap();
    let summary = fixture
        .lines()
        .find_map(|row| {
            let columns: Vec<_> = row.split('\t').collect();
            (columns.len() == 7 && columns[1] == "ordinary-matched-quantity-two/3")
                .then(|| columns[6])
        })
        .expect("independently executed ordinary quantity-two match");
    let source = V::from_json(&JsonValue::parse(summary).unwrap()).unwrap();
    let V::Array(actors) = source.get("actors") else {
        panic!("source actors")
    };
    assert_eq!(actors.len(), 2);
    let state = matched_state();
    for actor in actors {
        let V::String(address) = actor.get("address") else {
            panic!("source address")
        };
        assert!([A, B].contains(&address.as_str()));
        let owner = a(&address);
        let position = state.positions().get(&owner).unwrap();
        let margin = state.margin_atoms(&owner);
        assert_eq!(
            actor.get("available"),
            n(available(&state, &address) as f64 / U as f64)
        );
        assert_eq!(actor.get("reserved"), n(0.0));
        assert_eq!(actor.get("margin"), n(margin as f64 / U as f64));
        assert_eq!(actor.get("positionMargin"), n(margin as f64 / U as f64));
        assert_eq!(
            actor.get("contracts"),
            n(position.contracts.to_string().parse().unwrap())
        );
        assert_eq!(
            actor.get("amount"),
            n((available(&state, &address) + margin) as f64 / U as f64)
        );
    }
    assert_eq!(source.get("fees").get("bufferedPlusPersisted"), s("0.001"));
    assert_eq!(state.receipts()[0].retained_fee_atoms, 100_000);
    // This comparison establishes the ordinary cash/position intersection.
    // JS buffers these fees and has different queue flags, IDs, persistence
    // and failure prefixes; this is not a complete-state parity assertion.
    restored(&state);
}

#[test]
fn v2_actual_source_prices_fund_positions_at_24_and_restore_flip_and_close() {
    let mut state = matched_state();
    while state.finalized_height().unwrap() < 22 {
        apply(&mut state, &[]);
    }
    let report = apply(&mut state, &[publish(0, 90, 104)]);
    assert_eq!(report.accepted, 1);
    let before_a = available(&state, A);
    let before_b = available(&state, B);
    let margin_a = state.margin_atoms(&a(A));
    let margin_b = state.margin_atoms(&a(B));
    apply(&mut state, &[]);
    let epoch = &state.funding_state().epochs[0];
    assert_eq!(epoch.height, 24);
    assert_eq!(epoch.status, FundingStatus::Paid);
    assert_eq!(epoch.positions.len(), 2);
    let plan = epoch.plan.as_ref().unwrap();
    assert!(plan.total_atoms > 0);
    assert_eq!(plan.debits.len(), 1);
    assert_eq!(plan.credits.len(), 1);
    assert_eq!(plan.debits[0].owner, a(A));
    assert_eq!(plan.credits[0].owner, a(B));
    assert_eq!(available(&state, A), before_a - plan.total_atoms);
    assert_eq!(available(&state, B), before_b + plan.total_atoms);
    assert_eq!(state.margin_atoms(&a(A)), margin_a);
    assert_eq!(state.margin_atoms(&a(B)), margin_b);
    let paid = epoch.clone();
    let mut replay = restored(&state);
    let flip = [
        order_for(0, 91, A, 104, 5, true),
        order_for(1, 92, B, 104, 5, false),
    ];
    assert_eq!(apply(&mut state, &flip), apply(&mut replay, &flip));
    assert_eq!(position(&state, A), "-3");
    assert_eq!(position(&state, B), "3");
    assert_eq!(state.funding_state().epochs, [paid]);
    assert_eq!(state, replay);
    let close = [
        order_for(0, 93, A, 104, 3, false),
        order_for(1, 94, B, 104, 3, true),
    ];
    assert_eq!(apply(&mut state, &close), apply(&mut replay, &close));
    assert_eq!(position(&state, A), "0");
    assert_eq!(position(&state, B), "0");
    assert_eq!(state.margin_atoms(&a(A)), 0);
    assert_eq!(state.margin_atoms(&a(B)), 0);
    assert_eq!(state, replay);
    state.ledger().verify_conservation().unwrap();
    restored(&state);
}

#[test]
fn source_policy_version_is_independently_expected_and_v1_history_remains_replayable() {
    let v1 = opened_metadata_with_policy(SourceConsensusPolicy::ConservingSequentialV1);
    let v2 = opened_metadata();
    let v1_bytes = v1.snapshot_bytes().unwrap();
    let v2_bytes = v2.snapshot_bytes().unwrap();
    assert_eq!(v1_bytes[37], 1);
    assert_eq!(v2_bytes[37], 2);
    assert_ne!(v1_bytes, v2_bytes);
    assert!(EconomicState::from_snapshot_bytes(
        &v1_bytes,
        v2.config(),
        &a(A),
        &BTreeSet::from([p()])
    )
    .is_err());
    assert!(EconomicState::from_snapshot_bytes(
        &v2_bytes,
        v1.config(),
        &a(A),
        &BTreeSet::from([p()])
    )
    .is_err());
    restored(&v1);
    restored(&v2);
}
