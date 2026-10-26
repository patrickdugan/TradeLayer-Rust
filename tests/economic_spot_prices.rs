use std::collections::{BTreeMap, BTreeSet};
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::client::RpcValue;
use tradelayer_core::decimal::{Decimal, Rounding};
use tradelayer_core::economic::{EconomicConfig, EconomicInput, SettlementPolicy};
use tradelayer_core::economic_chain::{
    EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::economic_spot_prices::*;
use tradelayer_core::joint::{JointOperation, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::service::{ServiceError, ServiceMethod, ServiceRequest};
use tradelayer_core::service_routes::dispatch;
use tradelayer_core::service_view::EconomicReadBackend;
use tradelayer_core::spot::SpotIntent;
use tradelayer_core::tally::FeeRoute;

fn p(id: u32) -> PropertyId {
    id.try_into().unwrap()
}
fn a(text: &str) -> AccountId {
    text.try_into().unwrap()
}
fn tx(id: u8) -> TxId {
    TxId::from_bytes([id; 32])
}
fn joint(id: u8, actor: &str, operation: JointOperation) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: tx(id),
        actor: a(actor),
        operation,
    })
}
fn new(network: TradeLayerMainnet) -> EconomicChain {
    let mut ledger = Ledger::new();
    for (id, owner) in [(5, "alice"), (6, "bob"), (7, "alice")] {
        ledger
            .apply(
                tx(id as u8),
                Operation::CreateProperty {
                    id: p(id),
                    issuer: a(owner),
                    kind: PropertyKind::Fixed,
                    initial_atoms: u128::from(u64::MAX) * 10,
                },
            )
            .unwrap();
    }
    let mut chain = EconomicChain::new_unverified(
        EconomicChainConfig {
            economics: EconomicConfig {
                network,
                contract: 1.try_into().unwrap(),
                collateral: p(7),
                inverse: false,
                notional: Decimal::from(1_u64),
                initial_margin_per_contract: Decimal::from(10_u64),
                fee_route: FeeRoute::Native(1),
                policy: SettlementPolicy::ConservingV1,
                funding: None,
                marking: None,
                liquidation: None,
                amm: None,
                contract_binding: None,
            },
            admin: a("admin"),
            permitted_spot_properties: BTreeSet::from([p(5), p(6), p(7)]),
            rollback_limit: 8,
        },
        ledger,
    )
    .unwrap();
    apply(
        &mut chain,
        10,
        vec![joint(
            10,
            "admin",
            JointOperation::Registry(RegistryAction::Activate {
                types: vec![5, 6],
                code_hash: [0x71; 32],
            }),
        )],
    );
    chain
}
fn apply(chain: &mut EconomicChain, marker: u8, inputs: Vec<EconomicInput>) {
    chain
        .apply_unverified_block(&UnverifiedEconomicBlock {
            network: chain.config().economics.network,
            header: BlockHeader {
                height: chain.tip().height + 1,
                parent: chain.tip().id,
                id: BlockId([marker; 32]),
            },
            inputs,
        })
        .unwrap();
}
fn placement(
    id: u8,
    actor: &str,
    offered: u32,
    desired: u32,
    amount: u64,
    expect: u64,
) -> EconomicInput {
    joint(
        id,
        actor,
        JointOperation::SpotPlace(SpotIntent {
            offered: p(offered),
            desired: p(desired),
            offered_atoms: amount,
            expected_atoms: expect,
            post: false,
            stop: false,
        }),
    )
}
fn trade(chain: &mut EconomicChain, marker: u8, base: u64, quote: u64) {
    apply(
        chain,
        marker,
        vec![
            placement(marker, "alice", 5, 6, base, quote),
            placement(marker + 1, "bob", 6, 5, quote, base),
        ],
    );
}
fn value(row: &ExecutedSpotPrice) -> String {
    row.price(20, Rounding::HalfUp).unwrap().to_string()
}
#[test]
fn exact_directed_prices_vwap_and_as_of_queries_use_actual_funded_fills() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let mut chain = new(network);
        trade(&mut chain, 20, 3, 2);
        trade(&mut chain, 30, 1, 3);
        let before = chain.checkpoint_bytes().unwrap();
        let prices = EconomicSpotPrices::from_state(chain.state()).unwrap();
        assert_eq!(prices.network(), network);
        assert!(prices.last_price(p(5), p(6), 1).unwrap().is_none());
        let as_of = prices.last_price(p(5), p(6), 2).unwrap().unwrap();
        assert_eq!(value(&as_of), "0.66666666666666666667");
        assert_eq!((as_of.base_atoms, as_of.quote_atoms), (3, 2));
        let last = prices.last_price(p(5), p(6), 3).unwrap().unwrap();
        assert_eq!(value(&last), "3");
        let direct = prices.vwap(p(5), p(6), 2, 3).unwrap().unwrap();
        let reverse = prices.vwap(p(6), p(5), 2, 3).unwrap().unwrap();
        assert_eq!(
            (direct.base_atoms, direct.quote_atoms, direct.sample_count),
            (4, 5, 2)
        );
        assert_eq!((reverse.base_atoms, reverse.quote_atoms), (5, 4));
        assert_eq!(value(&direct), "1.25");
        assert_eq!(value(&reverse), "0.8");
        assert_eq!((direct.first_fill_index, direct.last_fill_index), (0, 1));
        let samples = prices.samples(p(6), p(5), 2, 3).unwrap();
        assert_eq!((samples[0].base_atoms, samples[0].quote_atoms), (2, 3));
        assert_eq!(samples[0].sell_txid, tx(20));
        assert_eq!(samples[0].buy_txid, tx(21));
        assert_eq!(samples[1].fill_index, 1);
        assert_eq!(chain.checkpoint_bytes().unwrap(), before);
        chain.state().ledger().verify_conservation().unwrap();
    }
}
#[test]
fn restored_owner_rollback_and_replacement_recompute_prices_together() {
    let mut chain = new(TradeLayerMainnet::Litecoin);
    trade(&mut chain, 20, 3, 2);
    let first_tip = chain.tip();
    trade(&mut chain, 30, 1, 3);
    let snapshot = chain.checkpoint_bytes().unwrap();
    let mut restored =
        EconomicChain::from_checkpoint_bytes(&snapshot, chain.config().clone(), chain.tip())
            .unwrap();
    let current = EconomicSpotPrices::from_state(restored.state())
        .unwrap()
        .vwap(p(5), p(6), 2, 3)
        .unwrap()
        .unwrap();
    assert_eq!(value(&current), "1.25");
    restored.rollback_tip(restored.tip()).unwrap();
    assert_eq!(restored.tip(), first_tip);
    let old = EconomicSpotPrices::from_state(restored.state())
        .unwrap()
        .last_price(p(5), p(6), 2)
        .unwrap()
        .unwrap();
    assert_eq!(value(&old), "0.66666666666666666667");
    trade(&mut restored, 40, 2, 8);
    let replacement = EconomicSpotPrices::from_state(restored.state())
        .unwrap()
        .vwap(p(5), p(6), 2, 3)
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            replacement.base_atoms,
            replacement.quote_atoms,
            replacement.sample_count
        ),
        (5, 10, 2)
    );
    assert_eq!(value(&replacement), "2");
    assert_eq!(
        value(
            &EconomicSpotPrices::from_state(chain.state())
                .unwrap()
                .last_price(p(5), p(6), 3)
                .unwrap()
                .unwrap()
        ),
        "3"
    );
}
#[test]
fn amount_domain_exceeds_u64_price_atoms_and_never_uses_float() {
    let mut chain = new(TradeLayerMainnet::Bitcoin);
    trade(&mut chain, 20, 1, u64::MAX);
    trade(&mut chain, 30, 1, u64::MAX);
    let prices = EconomicSpotPrices::from_state(chain.state()).unwrap();
    let row = prices.vwap(p(5), p(6), 2, 3).unwrap().unwrap();
    assert_eq!(row.quote_atoms, u128::from(u64::MAX) * 2);
    assert_eq!(value(&row), u64::MAX.to_string());
    let inverse = prices.vwap(p(6), p(5), 2, 3).unwrap().unwrap();
    assert_eq!(inverse.price(8, Rounding::HalfUp).unwrap(), Decimal::zero());
    assert_eq!(
        inverse.price(30, Rounding::HalfUp).unwrap().to_string(),
        "0.000000000000000000054210108624"
    );
    assert_eq!(
        (inverse.base_atoms, inverse.quote_atoms),
        (u128::from(u64::MAX) * 2, 2)
    );
}
#[test]
fn absent_execution_and_invalid_ranges_remain_explicit() {
    let chain = new(TradeLayerMainnet::Dogecoin);
    let view = EconomicSpotPrices::from_state(chain.state()).unwrap();
    assert_eq!(view.last_price(p(5), p(6), 1).unwrap(), None);
    assert_eq!(view.vwap(p(5), p(7), 0, 1).unwrap(), None);
    assert_eq!(
        view.vwap(p(5), p(6), 1, 0),
        Err(SpotPriceError::InvalidRange)
    );
    assert_eq!(
        view.last_price(p(5), p(6), 2),
        Err(SpotPriceError::BeyondFinalizedHeight)
    );
    assert_eq!(
        view.last_price(p(5), p(5), 1),
        Err(SpotPriceError::SameAsset)
    );
    assert_eq!(
        view.last_price(p(5), p(8), 1),
        Err(SpotPriceError::UnknownProperty)
    );
}
#[test]
fn service_queries_preserve_direction_exact_amounts_owner_tip_and_errors() {
    let mut chain = new(TradeLayerMainnet::Litecoin);
    trade(&mut chain, 20, 3, 2);
    trade(&mut chain, 30, 1, 3);
    let before = chain.checkpoint_bytes().unwrap();
    let mut backend = EconomicReadBackend::from_chain(&chain).unwrap();
    let request = |path: &str, pairs: &[(&str, &str)]| {
        ServiceRequest::new(
            ServiceMethod::Get,
            path,
            pairs
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect::<BTreeMap<_, _>>(),
            RpcValue::null(),
        )
        .unwrap()
        .without_body()
    };
    let output = dispatch(
        &request("/spotVwap/6/5", &[("startBlock", "2"), ("endBlock", "3")]),
        &mut backend,
    )
    .unwrap();
    let output = RpcValue::parse(&output.body).unwrap();
    assert_eq!(
        output
            .field("policy")
            .unwrap()
            .unwrap()
            .as_string()
            .unwrap(),
        PRICE_POLICY
    );
    assert_eq!(
        output
            .field("authenticatedAdmission")
            .unwrap()
            .unwrap()
            .as_json(),
        "false"
    );
    assert_eq!(
        output
            .field("network")
            .unwrap()
            .unwrap()
            .as_string()
            .unwrap(),
        "litecoin"
    );
    let price = output.field("price").unwrap().unwrap();
    assert_eq!(
        price
            .field("quotePerBase")
            .unwrap()
            .unwrap()
            .as_string()
            .unwrap(),
        "0.8"
    );
    assert_eq!(
        price
            .field("baseAtoms")
            .unwrap()
            .unwrap()
            .as_string()
            .unwrap(),
        "5"
    );
    let output = dispatch(&request("/spotPrice/5/6", &[("height", "2")]), &mut backend).unwrap();
    assert!(output.body.contains("0.66666666666666666667"));
    let output = dispatch(&request("/spotPrice/5/7", &[]), &mut backend).unwrap();
    assert_eq!(
        RpcValue::parse(&output.body)
            .unwrap()
            .field("price")
            .unwrap()
            .unwrap()
            .as_json(),
        "null"
    );
    for (path, query, error) in [
        (
            "/spotVwap/5/6",
            vec![("startBlock", "3"), ("endBlock", "2")],
            ServiceError::InvalidRequest,
        ),
        (
            "/spotPrice/5/6",
            vec![("height", "4")],
            ServiceError::InvalidRequest,
        ),
        (
            "/spotPrice/5/6",
            vec![("height", "02")],
            ServiceError::InvalidRequest,
        ),
        ("/spotPrice/5/5", vec![], ServiceError::InvalidRequest),
        ("/spotPrice/5/8", vec![], ServiceError::NotFound),
    ] {
        assert_eq!(dispatch(&request(path, &query), &mut backend), Err(error));
    }
    assert_eq!(chain.checkpoint_bytes().unwrap(), before);
}
