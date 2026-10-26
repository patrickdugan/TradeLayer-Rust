//! Actual source oracle commands compose with funded custody and raw replay.
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
use tradelayer_core::joint::JointProtocolState;
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::oracle_source_owner::{OracleDbFailure, OracleSourceOwner};
use tradelayer_core::property_catalog::PropertyCatalog;
use tradelayer_core::source_catalogs::{bootstrap_commitment, SourceCatalogOwners};
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};

const ATOMS: u128 = 100_000_000;
const ADMIN: &str = "1BoatSLRHtKNngkdXEeobR76b53LETtpyT";
const NEXT: &str = "1BitcoinEaterAddressDontSendf59kuE";
const BACKUP: &str = "1CounterpartyXXXXXXXXXXXXXXXUWLpVr";
fn a(value: &str) -> AccountId {
    value.try_into().unwrap()
}
fn p(value: u32) -> PropertyId {
    value.try_into().unwrap()
}
fn s(value: &str) -> V {
    V::String(value.into())
}
fn n(value: f64) -> V {
    V::number(value)
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect(),
    )
}
fn id(value: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&value.to_be_bytes());
    TxId::from_bytes(bytes)
}
struct NoExternal;
impl SourceBackend for NoExternal {
    fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        Err(SourceRuleError::UnknownDependency)
    }
}
fn fixture(owner: OracleSourceOwner, market: bool) -> EconomicState {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(900),
            Operation::CreateProperty {
                id: p(7),
                issuer: a(ADMIN),
                kind: PropertyKind::Fixed,
                initial_atoms: 1000 * ATOMS,
            },
        )
        .unwrap();
    let mut properties = PropertyCatalog::new();
    assert_eq!(
        properties
            .execute(
                "addProperty",
                &[
                    n(7.0),
                    s("COLLATERAL"),
                    n(1000.0),
                    s("Fixed"),
                    n(0.0),
                    s(ADMIN),
                    s(BACKUP)
                ],
                &mut NoExternal
            )
            .unwrap()
            .outcome,
        SourceOutcome::Returned(V::Undefined)
    );
    let joint = JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Bitcoin,
        a("admin"),
        ledger,
        BTreeSet::from([p(7)]),
    )
    .unwrap();
    let owners =
        SourceCatalogOwners::new(properties, ClearlistSourceOwner::new()).with_oracles(owner);
    let config = EconomicConfig {
        network: TradeLayerMainnet::Bitcoin,
        contract: 1.try_into().unwrap(),
        collateral: p(7),
        inverse: false,
        notional: "1".parse().unwrap(),
        initial_margin_per_contract: "10".parse().unwrap(),
        fee_route: FeeRoute::Oracle(1),
        policy: SettlementPolicy::ConservingV1,
        funding: market.then_some(FundingConfig {
            oracle_id: 1,
            max_staleness_blocks: 192,
            policy: FundingPolicy::ConservingFundingV1,
        }),
        marking: market.then_some(MarkConfig {
            oracle_id: 1,
            policy: MarkPolicy::ConservingMarkBasisV1,
        }),
        liquidation: None,
        amm: None,
        contract_binding: None,
        native_entitlements: None,
        insurance: None,
        source_consensus: Some(SourceConsensusConfig {
            bootstrap_commitment: EconomicState::source_bootstrap_commitment(&joint).unwrap(),
            catalog_bootstrap_commitment: Some(bootstrap_commitment(
                &owners.checkpoint_bytes().unwrap(),
            )),
            policy: SourceConsensusPolicy::ConservingSequentialV1,
        }),
    };
    EconomicState::from_joint_with_source_catalogs_unverified(config, joint, owners).unwrap()
}
fn tx(position: u32, number: u64, payload: String, sender: &str) -> SourceTransaction {
    SourceTransaction {
        position,
        txid: id(number),
        payload,
        marker: s("tl"),
        sender: s(sender),
        reference: V::Null,
        sender_amount: n(1.0),
        reference_amount: n(0.0),
    }
}
fn apply(state: &mut EconomicState, transactions: &[SourceTransaction]) -> SourceBlockReport {
    let previous = state.source_consensus_state().frames.last();
    let height = previous.map_or(1, |frame| frame.header.height + 1);
    let mut bytes = [0x6d; 32];
    bytes[24..].copy_from_slice(&height.to_be_bytes());
    let header = BlockHeader {
        height,
        id: BlockId(bytes),
        parent: previous.map_or(state.config().network.genesis(), |frame| frame.header.id),
    };
    state
        .apply_source_block_unverified(header, SourceBlockMode::IndexedReplay, transactions)
        .unwrap_or_else(|error| {
            panic!("source block {height}: {error:?}; transactions {transactions:?}")
        })
}
fn activate(state: &mut EconomicState) {
    let result = apply(
        state,
        &[tx(
            0,
            1,
            format!("02;8;13;14;15;18,{}", "11".repeat(32)),
            "admin",
        )],
    );
    assert_eq!(result.accepted, 1, "{:?}", result.receipts);
}
fn create(state: &mut EconomicState) {
    let result = apply(
        state,
        &[tx(
            0,
            2,
            format!("dFUND,https://oracle.example,{BACKUP},1;2,0"),
            ADMIN,
        )],
    );
    assert_eq!(result.accepted, 1, "{:?}", result.receipts);
}
fn base36(mut value: u128) -> String {
    let mut text = Vec::new();
    while value != 0 {
        text.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(value % 36) as usize] as char);
        value /= 36;
    }
    text.into_iter().rev().collect()
}
fn publish(
    position: u32,
    number: u64,
    oracle: u32,
    price: u128,
    sender: &str,
) -> SourceTransaction {
    tx(
        position,
        number,
        format!("e{},{}", base36(u128::from(oracle)), base36(price * 10_000)),
        sender,
    )
}
fn read(state: &EconomicState, method: &str, args: Vec<V>) -> V {
    let last = state.source_consensus_state().frames.last();
    let parent = last.map_or(
        Tip {
            height: 0,
            id: state.config().network.genesis(),
        },
        |frame| Tip {
            height: frame.header.height,
            id: frame.header.id,
        },
    );
    let mut backend = EconomicSourceBackend::from_state_unverified(state, parent).unwrap();
    match backend
        .invoke(&DependencyCall {
            target: format!("Oracle.{method}"),
            arguments: args,
        })
        .unwrap()
    {
        DependencyReturn::Pending(value) => value,
        other => panic!("{other:?}"),
    }
}
fn owner(state: &EconomicState) -> &OracleSourceOwner {
    state
        .source_catalog_owners()
        .unwrap()
        .oracles
        .as_ref()
        .unwrap()
}
fn restart(state: &EconomicState) -> EconomicState {
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x0a");
    let result = EconomicState::from_snapshot_bytes(
        &bytes,
        state.config(),
        &a("admin"),
        &BTreeSet::from([p(7)]),
    )
    .unwrap();
    assert_eq!(&result, state);
    assert_eq!(result.snapshot_bytes().unwrap(), bytes);
    result
}
fn mark_ids(state: &EconomicState) -> Vec<TxId> {
    state
        .mark_state()
        .events
        .iter()
        .filter_map(|event| match &event.kind {
            MarkEventKind::Mark { observation, .. } => Some(observation.txid),
            _ => None,
        })
        .collect()
}

#[test]
fn actual_oracle_commands_preserve_source_fields_circuit_breaker_and_publication_ids() {
    let mut state = fixture(OracleSourceOwner::new(), true);
    activate(&mut state);
    create(&mut state);
    assert!(
        state.joint().registry().oracle(1).is_none(),
        "source metadata never invents strict Registry rows"
    );
    let info = read(&state, "getOracleInfo", vec![n(1.0)]);
    assert_eq!(info.get("ticker"), s("FUND"));
    assert_eq!(info.get("lag"), n(0.0));
    assert_eq!(
        info.get("clearlists"),
        V::Array(vec![]),
        "Logic passes absent clearlists despite decoded whitelists"
    );
    assert_eq!(
        read(&state, "getOraclePrice", vec![n(1.0)]),
        n(1.0),
        "source validity getter retains its default"
    );
    assert!(state.funding_state().observations.is_empty());
    let result = apply(
        &mut state,
        &[
            publish(0, 10, 1, 100, ADMIN),
            publish(1, 11, 1, 200, ADMIN),
            tx(2, 12, format!("20;{BACKUP};7;1"), ADMIN),
        ],
    );
    assert_eq!(result.accepted, 3, "{:?}", result.receipts);
    assert_eq!(owner(&state).rows("oracleData").unwrap().len(), 1);
    assert_eq!(read(&state, "getOraclePrice", vec![n(1.0)]), n(100.0));
    assert_eq!(state.funding_state().observations.len(), 1);
    assert_eq!(state.funding_state().observations[0].txid, id(10));
    assert_eq!(
        state.funding_state().observations[0].price,
        "100".parse().unwrap()
    );
    assert_eq!(
        mark_ids(&state),
        vec![id(10)],
        "same-block successful no-op retains first publication ID"
    );
    assert_eq!(state.ledger().available_atoms(p(7), &a(ADMIN)), 999 * ATOMS);
    assert_eq!(state.ledger().available_atoms(p(7), &a(BACKUP)), ATOMS);
    assert!(state.ledger().is_processed(id(12)));
    state = restart(&state);
    let update = tx(0, 13, format!("8{NEXT},0,1,0,1,0"), ADMIN);
    assert_eq!(apply(&mut state, &[update]).accepted, 1);
    assert_eq!(
        read(&state, "isAdmin", vec![s(ADMIN), n(1.0)]),
        V::Bool(false)
    );
    assert_eq!(
        read(&state, "isAdmin", vec![s(NEXT), n(1.0)]),
        V::Bool(true)
    );
    let result = apply(
        &mut state,
        &[publish(0, 14, 1, 500, ADMIN), publish(1, 15, 1, 500, NEXT)],
    );
    assert_eq!(result.receipts[0].disposition, SourceDisposition::Invalid);
    assert_eq!(result.receipts[1].disposition, SourceDisposition::Applied);
    assert_eq!(
        read(&state, "getOraclePrice", vec![n(1.0)]),
        n(105.0),
        "actual source applies its 5% circuit bound"
    );
    assert_eq!(mark_ids(&state), vec![id(10), id(15)]);
    let result = apply(&mut state, &[tx(0, 16, "f1".into(), NEXT)]);
    assert_ne!(
        result.receipts[0].disposition,
        SourceDisposition::Applied,
        "source15 still invokes the wrong validator"
    );
    assert_ne!(
        read(&state, "getOracleInfo", vec![n(1.0)]).get("closed"),
        V::Bool(true)
    );
    while state.finalized_height().unwrap() < 24 {
        apply(&mut state, &[]);
    }
    assert_eq!(state.funding_state().epochs.len(), 1);
    assert_eq!(
        state.funding_state().epochs[0].status,
        FundingStatus::NoPositions
    );
    restart(&state).verify().unwrap();
}

#[test]
fn source_thrown_publication_retains_poisoned_watermark_without_funded_publication() {
    for operation in ["findAsync", "updateAsync"] {
        let mut oracles = OracleSourceOwner::new();
        oracles
            .fail_database_call(OracleDbFailure {
                collection: "oracleData".into(),
                operation: operation.into(),
                skip: 0,
                exception: SourceException {
                    name: "Error".into(),
                    message: format!("poison-{operation}"),
                },
            })
            .unwrap();
        let mut state = fixture(oracles, true);
        activate(&mut state);
        create(&mut state);
        let result = apply(
            &mut state,
            &[publish(0, 10, 1, 100, ADMIN), publish(1, 11, 1, 200, ADMIN)],
        );
        assert_eq!(
            result.receipts[0].disposition,
            SourceDisposition::Thrown,
            "{:?}",
            result.receipts
        );
        assert!(result.receipts[0]
            .reason
            .contains(&format!("poison-{operation}")));
        assert_eq!(result.receipts[1].disposition, SourceDisposition::Applied);
        assert!(owner(&state).rows("oracleData").unwrap().is_empty());
        assert_eq!(
            owner(&state).state_value().get("lastOracleUpdateBlock"),
            V::Array(vec![V::Array(vec![n(1.0), n(3.0)])])
        );
        if operation == "updateAsync" {
            assert_eq!(
                read(&state, "getOracleInfo", vec![n(1.0)])
                    .get("data")
                    .get("price"),
                n(100.0),
                "failed save retains metadata cache prefix"
            );
        }
        assert_eq!(read(&state, "getOraclePrice", vec![n(1.0)]), n(1.0));
        assert!(state.funding_state().observations.is_empty());
        assert!(mark_ids(&state).is_empty());
        state = restart(&state);
        let result = apply(&mut state, &[publish(0, 12, 1, 100, ADMIN)]);
        assert_eq!(result.accepted, 1, "{:?}", result.receipts);
        assert_eq!(state.funding_state().observations[0].txid, id(12));
        assert_eq!(mark_ids(&state), vec![id(12)]);
        let result = apply(&mut state, &[publish(0, 10, 1, 200, ADMIN)]);
        assert_eq!(
            result.receipts[0].disposition,
            SourceDisposition::AlreadyProcessed,
            "failed raw original ID is also replay protected"
        );
        restart(&state).verify().unwrap();
    }
}

#[test]
fn unreferenced_legacy_metadata_publication_does_not_require_a_funded_market_shape() {
    let mut oracles = OracleSourceOwner::new();
    oracles
        .import_rows(
            "oracleList",
            vec![obj(&[
                ("_id", s("oracle-9")),
                ("id", n(9.0)),
                (
                    "name",
                    obj(&[("adminAddress", s(ADMIN)), ("backupAddress", s(BACKUP))]),
                ),
            ])],
        )
        .unwrap();
    let mut state = fixture(oracles, false);
    activate(&mut state);
    let result = apply(&mut state, &[publish(0, 10, 9, 100, ADMIN)]);
    assert_eq!(result.accepted, 1, "{:?}", result.receipts);
    assert_eq!(read(&state, "getOraclePrice", vec![n(9.0)]), n(100.0));
    assert_eq!(
        read(&state, "getOracleInfo", vec![n(9.0)]).get("ticker"),
        V::Undefined
    );
    assert!(state.joint().registry().oracle(9).is_none());
    restart(&state).verify().unwrap();
}

#[test]
fn publication_before_selected_contract_exists_restores_then_marks_after_raw_creation() {
    // The typed value pins the independently expected contract definition;
    // every actual command below is decoded and validated from its raw wire.
    let creation = CreateContractInput {
        txid: id(30),
        issuer: a(ADMIN),
        parameters: ContractParameters {
            native: false,
            underlying_oracle_id: Some(1),
            on_chain_data: vec![],
            notional_property_id: ContractAsset::BaseChain,
            notional_value: 1.into(),
            collateral_property_id: ContractAsset::Property(p(7)),
            leverage: 10.into(),
            expiry_period: 0,
            series: 1,
            inverse: false,
            fee: 0.into(),
            whitelist: None,
            ticker: None,
        },
    };
    let pristine = fixture(OracleSourceOwner::new(), true);
    let mut config = pristine.config().clone();
    config.initial_margin_per_contract = 0.into();
    config.contract_binding = Some(ContractBindingConfig {
        creation_txid: creation.txid,
        definition_hash: contract_definition_commitment(&creation).unwrap(),
        bootstrap_commitment: EconomicState::contract_bootstrap_commitment(pristine.joint())
            .unwrap(),
        policy: ContractRiskPolicy::SourceContextsConservingV1,
    });
    let mut state = EconomicState::from_joint_with_source_catalogs_unverified(
        config,
        pristine.joint().clone(),
        pristine.source_catalog_owners().unwrap().clone(),
    )
    .unwrap();
    assert_eq!(
        apply(
            &mut state,
            &[tx(
                0,
                1,
                format!("02;13;14;16;18,{}", "11".repeat(32)),
                "admin"
            )]
        )
        .accepted,
        1
    );
    create(&mut state);
    let report = apply(&mut state, &[publish(0, 10, 1, 100, ADMIN)]);
    assert_eq!(report.accepted, 1, "{:?}", report.receipts);
    assert!(state.contract(1.try_into().unwrap()).is_none());
    assert_eq!(state.funding_state().observations[0].txid, id(10));
    assert!(
        mark_ids(&state).is_empty(),
        "no mark epoch exists before the selected contract"
    );
    state = restart(&state);

    let report = apply(
        &mut state,
        &[tx(0, 30, "g0,1,,0,1,7,10,0,1,0,0".into(), ADMIN)],
    );
    assert_eq!(report.accepted, 1, "{:?}", report.receipts);
    let accepted = state.contract(1.try_into().unwrap()).unwrap();
    assert_eq!(accepted.creation_txid, creation.txid);
    assert_eq!(
        accepted.definition_hash,
        contract_definition_commitment(&creation).unwrap()
    );
    assert_eq!(accepted.definition.ticker, "FUND-OPERP1-1");
    assert!(
        mark_ids(&state).is_empty(),
        "creation does not invent an earlier publication epoch"
    );
    state = restart(&state);

    let report = apply(
        &mut state,
        &[
            publish(0, 40, 1, 102, ADMIN),
            tx(1, 41, format!("20;{BACKUP};7;1"), ADMIN),
        ],
    );
    assert_eq!(report.accepted, 2, "{:?}", report.receipts);
    assert_eq!(mark_ids(&state), vec![id(40)]);
    assert_eq!(
        state
            .funding_state()
            .observations
            .iter()
            .map(|row| row.txid)
            .collect::<Vec<_>>(),
        vec![id(10), id(40)]
    );
    assert_eq!(state.ledger().available_atoms(p(7), &a(ADMIN)), 999 * ATOMS);
    assert_eq!(state.ledger().available_atoms(p(7), &a(BACKUP)), ATOMS);
    assert!(state.ledger().is_processed(id(41)));
    assert!(state.joint().registry().oracle(1).is_none());
    restart(&state).verify().unwrap();
}
