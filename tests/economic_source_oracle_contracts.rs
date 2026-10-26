//! Genuine raw13/14/16/18 over actual oracle + contract owners. The missing
//! Margin dependency is retained explicitly; no validated params are invented.
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
    EconomicConfig, EconomicState, FundingConfig, FundingPolicy, MarkConfig, MarkEventKind,
    MarkPolicy, SettlementPolicy,
};
use tradelayer_core::economic_validity::EconomicSourceBackend;
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
fn fixture() -> EconomicState {
    let network = TradeLayerMainnet::Bitcoin;
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(900),
            Operation::CreateProperty {
                id: p(),
                issuer: a(A),
                kind: PropertyKind::Fixed,
                initial_atoms: 4000 * U,
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
                amount_atoms: 2000 * U,
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
                    n(4000.0),
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
            policy: SourceConsensusPolicy::ConservingSequentialV1,
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
fn opened_metadata() -> EconomicState {
    let mut state = fixture();
    assert_eq!(
        apply(
            &mut state,
            &[tx(0, 10, &format!("013;14;16;18,{}", "11".repeat(32)), A)]
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
fn raw_leverage_rejection_and_missing_margin_dependency_are_audited_and_restored() {
    let mut state = opened_metadata();
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
    // Matching or paid funding cannot be claimed until this read adapter exists.
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
