//! Contract-bound full input replay, independent expectations and v5/v6 bytes.
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::contract_registry::{ContractAsset, ContractParameters, ContractSeries};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    contract_definition_commitment, ContractBindingConfig, ContractBindingState,
    ContractRiskPolicy, CreateContractInput, EconomicConfig, EconomicInput, EconomicState,
    LiquidationConfig, LiquidationPolicy, MarkConfig, MarkPolicy, SettlementPolicy,
};
use tradelayer_core::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::storage::Recovery;
use tradelayer_core::tally::FeeRoute;
const ATOMS: u128 = 100_000_000;
fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}
fn a(s: &str) -> AccountId {
    s.try_into().unwrap()
}
fn tx(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn config(network: TradeLayerMainnet) -> EconomicChainConfig {
    EconomicChainConfig {
        economics: EconomicConfig {
            native_entitlements: None,
            insurance: None,
            source_consensus: None,
            network,
            contract: 1.try_into().unwrap(),
            collateral: p(),
            inverse: false,
            notional: d("1"),
            initial_margin_per_contract: d("10"),
            fee_route: FeeRoute::Oracle(1),
            policy: SettlementPolicy::ConservingV1,
            contract_binding: None,
            funding: None,
            amm: Some(FundedAmmConfig {
                pool_id: 1,
                spread_bps: 40,
                max_quote_quantity: d("1"),
                max_abs_inventory: d("4"),
                max_collateral_atoms: 500 * ATOMS,
                policy: FundedAmmPolicy::ConservingFundedContractAmmV1,
            }),
            marking: Some(MarkConfig {
                oracle_id: 1,
                policy: MarkPolicy::ConservingMarkBasisV1,
            }),
            liquidation: Some(LiquidationConfig {
                max_price_deviation_bps: 1000,
                max_fills_per_owner: 8,
                policy: LiquidationPolicy::ConservingLiquidationV1,
            }),
        },
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p()]),
        rollback_limit: 16,
    }
}
fn ledger() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            tx(1),
            Operation::CreateProperty {
                id: p(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 2012 * ATOMS,
            },
        )
        .unwrap();
    for (n, owner, amount) in [(2, "bob", 12), (3, "carol", 1000)] {
        ledger
            .apply(
                tx(n),
                Operation::Transfer {
                    id: p(),
                    from: a("alice"),
                    to: a(owner),
                    amount_atoms: amount * ATOMS,
                },
            )
            .unwrap();
    }
    ledger
}
fn joint(n: u8, action: RegistryAction) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: tx(n),
        actor: a("admin"),
        operation: JointOperation::Registry(action),
    })
}
fn publication(n: u8, price: u128) -> EconomicInput {
    joint(
        n,
        RegistryAction::PublishOracle {
            oracle_id: 1,
            price_1e4: price * 10_000,
            high_1e4: None,
            low_1e4: None,
            close_1e4: None,
        },
    )
}
fn order(
    n: u8,
    owner: &str,
    side: Side,
    quantity: &str,
    price: &str,
    height: u64,
) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: tx(n),
        owner: a(owner),
        contract: 1.try_into().unwrap(),
        height,
        side,
        quantity: d(quantity),
        price: d(price),
        post: false,
        is_market: false,
    })
}
fn block(chain: &EconomicChain, inputs: Vec<EconomicInput>) -> UnverifiedEconomicBlock {
    let height = chain.tip().height + 1;
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&height.to_be_bytes());
    UnverifiedEconomicBlock {
        network: chain.config().economics.network,
        header: BlockHeader {
            height,
            id: BlockId(bytes),
            parent: chain.tip().id,
        },
        inputs,
    }
}
fn apply(chain: &mut EconomicChain, inputs: Vec<EconomicInput>) {
    chain.apply_unverified_block(&block(chain, inputs)).unwrap();
}
fn ready(network: TradeLayerMainnet, enabled: bool) -> EconomicChain {
    let mut cfg = config(network);
    if !enabled {
        cfg.economics.amm = None;
    }
    let mut chain = EconomicChain::new_unverified(cfg, ledger()).unwrap();
    apply(
        &mut chain,
        vec![joint(
            10,
            RegistryAction::Activate {
                types: if enabled {
                    vec![2, 6, 10, 13, 14, 15, 18]
                } else {
                    vec![2, 6, 13, 14, 15, 18]
                },
                code_hash: [0x51; 32],
            },
        )],
    );
    let mut inputs = vec![
        joint(
            11,
            RegistryAction::CreateOracle {
                ticker: "TEST".into(),
                url: "https://example.invalid".into(),
                backup: a("backup"),
                lag: 1,
            },
        ),
        publication(12, 100),
    ];
    if enabled {
        inputs.push(EconomicInput::Amm(AmmCapitalInput::Deposit {
            txid: tx(15),
            owner: a("alice"),
            collateral_atoms: 100 * ATOMS,
            min_share_units: 100 * ATOMS,
        }));
    }
    apply(&mut chain, inputs);
    chain
}
fn prepared(network: TradeLayerMainnet, enabled: bool) -> EconomicChain {
    let mut chain = ready(network, enabled);
    apply(
        &mut chain,
        vec![
            order(20, "alice", Side::Buy, "1", "100", 3),
            order(21, "bob", Side::Sell, "1", "100", 3),
        ],
    );
    apply(
        &mut chain,
        vec![
            order(30, "carol", Side::Sell, "1", "1000", 4),
            order(31, "bob", Side::Buy, "1", "80", 4),
        ],
    );
    chain
}

use tradelayer_core::economic::{AmmCapitalInput, FundedAmmConfig, FundedAmmPolicy};

fn creation() -> CreateContractInput {
    CreateContractInput {
        txid: tx(40),
        issuer: a("alice"),
        parameters: ContractParameters {
            native: false,
            underlying_oracle_id: Some(1),
            on_chain_data: vec![],
            notional_property_id: ContractAsset::BaseChain,
            notional_value: d("1"),
            collateral_property_id: ContractAsset::Property(p()),
            leverage: d("10"),
            expiry_period: 0,
            series: 1,
            inverse: false,
            fee: d("0"),
            whitelist: Some(0),
            ticker: None,
        },
    }
}
fn bound_config(network: TradeLayerMainnet) -> EconomicChainConfig {
    let mut cfg = config(network);
    let joint = JointProtocolState::from_bootstrap_unverified(
        network,
        a("admin"),
        ledger(),
        BTreeSet::from([p()]),
    )
    .unwrap();
    cfg.economics.initial_margin_per_contract = d("0");
    cfg.economics.amm = None;
    cfg.economics.liquidation = None;
    cfg.economics.contract_binding = Some(ContractBindingConfig {
        creation_txid: creation().txid,
        definition_hash: contract_definition_commitment(&creation()).unwrap(),
        bootstrap_commitment: EconomicState::contract_bootstrap_commitment(&joint).unwrap(),
        policy: ContractRiskPolicy::SourceContextsConservingV1,
    });
    cfg
}
fn bound_ready(network: TradeLayerMainnet) -> EconomicChain {
    let mut chain = EconomicChain::new_unverified(bound_config(network), ledger()).unwrap();
    apply(
        &mut chain,
        vec![joint(
            10,
            RegistryAction::Activate {
                types: vec![2, 6, 10, 13, 14, 15, 16, 18],
                code_hash: [0x51; 32],
            },
        )],
    );
    apply(
        &mut chain,
        vec![
            joint(
                11,
                RegistryAction::CreateOracle {
                    ticker: "TEST".into(),
                    url: "https://example.invalid".into(),
                    backup: a("backup"),
                    lag: 1,
                },
            ),
            publication(12, 100),
            EconomicInput::CreateContract(creation()),
        ],
    );
    apply(
        &mut chain,
        vec![
            order(20, "alice", Side::Buy, "0.5", "100", 3),
            order(21, "bob", Side::Sell, "0.5", "100", 3),
        ],
    );
    apply(
        &mut chain,
        vec![
            publication(14, 120),
            order(22, "alice", Side::Buy, "0.5", "120", 4),
            order(23, "carol", Side::Sell, "0.5", "120", 4),
        ],
    );
    chain
}
fn bound_active(network: TradeLayerMainnet) -> EconomicChain {
    let mut chain = bound_ready(network);
    apply(&mut chain, vec![publication(13, 110)]);
    apply(
        &mut chain,
        vec![
            publication(15, 150),
            order(60, "carol", Side::Sell, "1", "150", 6),
        ],
    );
    chain
}
fn read(
    bytes: &[u8],
    state: &EconomicState,
) -> Result<EconomicState, tradelayer_core::economic::EconomicError> {
    EconomicState::from_snapshot_bytes(bytes, state.config(), &a("admin"), &BTreeSet::from([p()]))
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
fn seal(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.extend_from_slice(&crc32(&bytes).to_be_bytes());
    bytes
}
fn count(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u32).to_be_bytes());
}
fn dec(out: &mut Vec<u8>, value: &Decimal) {
    let text = value.to_string();
    out.push(text.len() as u8);
    out.extend_from_slice(text.as_bytes());
}
fn account(out: &mut Vec<u8>, owner: &AccountId) {
    out.push(owner.as_str().len() as u8);
    out.extend_from_slice(owner.as_str().as_bytes());
}
fn text(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}
fn opt_u32(out: &mut Vec<u8>, value: Option<u32>) {
    out.push(u8::from(value.is_some()));
    if let Some(value) = value {
        out.extend_from_slice(&value.to_be_bytes());
    }
}
fn parameters(out: &mut Vec<u8>, p: &ContractParameters) {
    out.push(u8::from(p.native));
    opt_u32(out, p.underlying_oracle_id);
    count(out, p.on_chain_data.len());
    for (a, b) in &p.on_chain_data {
        out.extend_from_slice(&a.get().to_be_bytes());
        out.extend_from_slice(&b.get().to_be_bytes());
    }
    out.extend_from_slice(&p.notional_property_id.get().to_be_bytes());
    dec(out, &p.notional_value);
    out.extend_from_slice(&p.collateral_property_id.get().to_be_bytes());
    dec(out, &p.leverage);
    out.extend_from_slice(&p.expiry_period.to_be_bytes());
    out.extend_from_slice(&p.series.to_be_bytes());
    out.push(u8::from(p.inverse));
    dec(out, &p.fee);
    opt_u32(out, p.whitelist);
    out.push(u8::from(p.ticker.is_some()));
    if let Some(value) = &p.ticker {
        text(out, value);
    }
}
fn series(out: &mut Vec<u8>, s: &ContractSeries) {
    out.extend_from_slice(&s.id.get().to_be_bytes());
    account(out, &s.issuer);
    text(out, &s.ticker);
    parameters(out, &s.parameters);
    for values in [&s.expired, &s.unexpired] {
        count(out, values.len());
        for value in values {
            text(out, &value.id);
            out.extend_from_slice(&value.expiration_block.to_be_bytes());
        }
    }
    dec(out, &s.amm.position);
    dec(out, &s.amm.max_position);
    dec(out, &s.amm.max_quote_size);
    out.extend_from_slice(&s.amm.contract_type.get().to_be_bytes());
}
// Independent serializer for the actual fixture's complete catalog tail. Its
// final prefix assertion ensures these mutations do not depend on byte guesses.
fn input(out: &mut Vec<u8>, value: &EconomicInput) {
    match value {
        EconomicInput::CreateContract(value) => {
            out.push(4);
            out.extend_from_slice(&value.txid.as_bytes());
            account(out, &value.issuer);
            parameters(out, &value.parameters);
        }
        EconomicInput::Order(value) => {
            out.push(1);
            out.extend_from_slice(&value.txid.as_bytes());
            account(out, &value.owner);
            out.extend_from_slice(&value.contract.get().to_be_bytes());
            out.extend_from_slice(&value.height.to_be_bytes());
            out.push(if value.side == Side::Buy { 0 } else { 1 });
            dec(out, &value.quantity);
            dec(out, &value.price);
            out.push(u8::from(value.post));
            out.push(u8::from(value.is_market));
        }
        EconomicInput::Joint(value) => {
            out.push(0);
            out.extend_from_slice(&value.txid.as_bytes());
            account(out, &value.actor);
            let JointOperation::Registry(action) = &value.operation else {
                panic!("fixture Joint operation")
            };
            out.push(0);
            match action {
                RegistryAction::Activate { types, code_hash } => {
                    out.push(0);
                    count(out, types.len());
                    out.extend_from_slice(types);
                    out.extend_from_slice(code_hash);
                }
                RegistryAction::CreateOracle {
                    ticker,
                    url,
                    backup,
                    lag,
                } => {
                    out.push(5);
                    text(out, ticker);
                    text(out, url);
                    account(out, backup);
                    out.extend_from_slice(&lag.to_be_bytes());
                }
                RegistryAction::PublishOracle {
                    oracle_id,
                    price_1e4,
                    high_1e4,
                    low_1e4,
                    close_1e4,
                } => {
                    out.push(6);
                    out.extend_from_slice(&oracle_id.to_be_bytes());
                    out.extend_from_slice(&price_1e4.to_be_bytes());
                    for value in [high_1e4, low_1e4, close_1e4] {
                        out.push(u8::from(value.is_some()));
                        if let Some(value) = value {
                            out.extend_from_slice(&value.to_be_bytes());
                        }
                    }
                }
                _ => panic!("fixture Registry operation"),
            }
        }
        _ => panic!("fixture economic input"),
    }
}
fn binding_bytes(state: &ContractBindingState) -> Vec<u8> {
    let mut out = Vec::new();
    count(&mut out, state.bootstrap.len());
    out.extend_from_slice(&state.bootstrap);
    count(&mut out, state.contracts.len());
    for contract in &state.contracts {
        out.extend_from_slice(&contract.creation_txid.as_bytes());
        out.extend_from_slice(&contract.height.to_be_bytes());
        out.extend_from_slice(&contract.definition_hash);
        series(&mut out, &contract.definition);
    }
    count(&mut out, state.frames.len());
    for frame in &state.frames {
        out.extend_from_slice(&frame.height.to_be_bytes());
        count(&mut out, frame.inputs.len());
        for value in &frame.inputs {
            input(&mut out, value);
        }
    }
    out
}
fn replace_binding(
    bytes: &[u8],
    old: &ContractBindingState,
    new: &ContractBindingState,
) -> Vec<u8> {
    let old_tail = binding_bytes(old);
    let end = bytes.len() - 4;
    assert_eq!(&bytes[end - old_tail.len()..end], old_tail);
    let mut out = bytes[..end - old_tail.len()].to_vec();
    out.extend_from_slice(&binding_bytes(new));
    seal(out)
}

#[test]
fn disabled_binding_preserves_genuine_active_v5_with_amm_forced_fill() {
    let mut chain = prepared(TradeLayerMainnet::Bitcoin, true);
    apply(&mut chain, vec![publication(13, 1000)]);
    let bytes = chain.state().snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x05");
    assert_eq!(bytes.len(), 8840);
    // Captured from committed69a72c7's actual rlib before binding fields existed.
    let fingerprint = bytes.iter().fold(14_695_981_039_346_656_037u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(1_099_511_628_211)
    });
    assert_eq!(fingerprint, 12_997_489_052_212_977_811);
    assert_eq!(read(&bytes, chain.state()).unwrap(), *chain.state());
}
#[test]
fn v6_full_context_and_price_risk_restore_on_three_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let chain = bound_active(network);
        let state = chain.state();
        let bytes = state.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..5], b"TLEC\x06");
        let restored = read(&bytes, state).unwrap();
        assert_eq!(restored, *state);
        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
        assert_eq!(state.contract_binding_state().frames.len(), 6);
        assert_eq!(state.contract_binding_state().contracts.len(), 1);
        assert_eq!(
            state.contract_binding_state().contracts[0].creation_txid,
            creation().txid
        );
        assert_eq!(
            state.contract_binding_state().contracts[0]
                .definition
                .parameters
                .leverage,
            d("10")
        );
        assert_eq!(state.reserve_atoms(tx(60)), 15 * ATOMS);
        assert!(state.joint().snapshot_bytes().is_err());
        let restored = EconomicChain::from_checkpoint_bytes(
            &chain.checkpoint_bytes().unwrap(),
            chain.config().clone(),
            chain.tip(),
        )
        .unwrap();
        assert_eq!(restored, chain);
    }
}
#[test]
fn repaired_crc_cannot_change_catalog_dependency_order_inputs_or_risk() {
    let chain = bound_active(TradeLayerMainnet::Bitcoin);
    let state = chain.state();
    let bytes = state.snapshot_bytes().unwrap();
    let old = state.contract_binding_state();
    let mut variants = Vec::new();
    for (field, value) in [(0, d("2")), (1, d("20"))] {
        let mut v = old.clone();
        if field == 0 {
            v.contracts[0].definition.parameters.notional_value = value;
        } else {
            v.contracts[0].definition.parameters.leverage = value;
        }
        variants.push(v);
    }
    let mut v = old.clone();
    v.contracts[0].definition.parameters.inverse = true;
    variants.push(v);
    let mut v = old.clone();
    v.contracts[0].definition.parameters.underlying_oracle_id = Some(2);
    variants.push(v);
    let mut v = old.clone();
    v.contracts[0].definition.parameters.collateral_property_id =
        ContractAsset::Property(8.try_into().unwrap());
    variants.push(v);
    let mut v = old.clone();
    v.contracts[0].definition.issuer = a("bob");
    variants.push(v);
    let mut v = old.clone();
    v.contracts[0].height = 1;
    variants.push(v);
    let mut v = old.clone();
    v.contracts[0].definition.amm.max_position = d("2");
    variants.push(v);
    let mut v = old.clone();
    v.contracts[0].definition.unexpired[0].expiration_block += 1;
    variants.push(v);
    let mut v = old.clone();
    v.frames[1].inputs.swap(0, 2);
    variants.push(v);
    let mut v = old.clone();
    v.frames[1].inputs.remove(2);
    variants.push(v);
    let mut v = old.clone();
    v.frames.remove(3);
    variants.push(v);
    let mut v = old.clone();
    let EconomicInput::CreateContract(input) = &mut v.frames[1].inputs[2] else {
        panic!()
    };
    input.parameters.leverage = d("20");
    variants.push(v);
    let mut v = old.clone();
    let EconomicInput::Order(input) = &mut v.frames[3].inputs[1] else {
        panic!()
    };
    input.price = d("121");
    variants.push(v);
    let mut v = old.clone();
    let EconomicInput::Joint(input) = &mut v.frames[4].inputs[0] else {
        panic!()
    };
    input.txid = tx(10);
    variants.push(v);
    let mut v = old.clone();
    v.bootstrap[6] ^= 1;
    v.bootstrap.truncate(v.bootstrap.len() - 4);
    v.bootstrap = seal(v.bootstrap);
    variants.push(v);
    for (i, v) in variants.iter().enumerate() {
        assert!(
            read(&replace_binding(&bytes, old, v), state).is_err(),
            "forged catalog/context {i}"
        );
    }
    // Preserve complete inputs and nested component CRCs, but alter the
    // admission-price margin basis held beside the actual funded reserve.
    let mut row = tx(60).as_bytes().to_vec();
    account(&mut row, &a("carol"));
    dec(&mut row, &d("1"));
    row.extend_from_slice(&(15 * ATOMS).to_be_bytes());
    row.push(1);
    dec(&mut row, &d("15"));
    let positions: Vec<_> = bytes
        .windows(row.len())
        .enumerate()
        .filter_map(|(i, w)| (w == row).then_some(i))
        .collect();
    assert_eq!(positions.len(), 1);
    let mut forged = bytes[..bytes.len() - 4].to_vec();
    let last = positions[0] + row.len() - 1;
    forged[last] = b'6';
    assert!(read(&seal(forged), state).is_err());
}
#[test]
fn v6_reopen_requires_independent_definition_bootstrap_admin_and_properties() {
    let chain = bound_active(TradeLayerMainnet::Bitcoin);
    let state = chain.state();
    let bytes = state.snapshot_bytes().unwrap();
    for field in 0..4 {
        let mut cfg = state.config().clone();
        let binding = cfg.contract_binding.as_mut().unwrap();
        match field {
            0 => binding.creation_txid = tx(41),
            1 => binding.definition_hash[0] ^= 1,
            2 => binding.bootstrap_commitment[0] ^= 1,
            _ => cfg.notional = d("2"),
        };
        assert!(EconomicState::from_snapshot_bytes(
            &bytes,
            &cfg,
            &a("admin"),
            &BTreeSet::from([p()])
        )
        .is_err());
    }
    assert!(EconomicState::from_snapshot_bytes(
        &bytes,
        state.config(),
        &a("wrong"),
        &BTreeSet::from([p()])
    )
    .is_err());
    assert!(EconomicState::from_snapshot_bytes(
        &bytes,
        state.config(),
        &a("admin"),
        &BTreeSet::new()
    )
    .is_err());
    let mut forged = bytes[..bytes.len() - 4].to_vec();
    assert_eq!(forged[57], 0);
    forged[57] = 128;
    assert!(read(&seal(forged), state).is_err());
    let mut forged = bytes[..bytes.len() - 4].to_vec();
    forged.push(0);
    assert!(read(&seal(forged), state).is_err());
}
fn temp_root() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "tl-contract-snapshot-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
#[test]
fn v6_durable_restart_rollback_before_creation_and_replay_on_three_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let chain = bound_ready(network);
        let cfg = chain.config().clone();
        let root = temp_root();
        let mut durable =
            DurableEconomicChain::initialize(&root, chain.clone(), chain.tip()).unwrap();
        let mark = block(&chain, vec![publication(13, 110)]);
        let marked = durable
            .apply_unverified_block(durable.head().unwrap(), &mark)
            .unwrap();
        let expected = durable.chain().unwrap().clone();
        drop(durable);
        let (mut durable, _) =
            DurableEconomicChain::load(&root, cfg.clone(), marked, Recovery::RejectIncompleteTail)
                .unwrap();
        assert_eq!(durable.chain().unwrap(), &expected);
        let target = expected.config().anchor();
        let rolled = durable
            .rollback_to(durable.head().unwrap(), target)
            .unwrap();
        assert!(durable
            .chain()
            .unwrap()
            .state()
            .contract_binding_state()
            .contracts
            .is_empty());
        drop(durable);
        let (mut durable, _) =
            DurableEconomicChain::load(&root, cfg, rolled, Recovery::RejectIncompleteTail).unwrap();
        let replay = bound_active(network);
        for frame in &replay.state().contract_binding_state().frames {
            let block = block(durable.chain().unwrap(), frame.inputs.clone());
            durable
                .apply_unverified_block(durable.head().unwrap(), &block)
                .unwrap();
        }
        assert_eq!(durable.chain().unwrap(), &replay);
        drop(durable);
        fs::remove_dir_all(root).unwrap();
    }
}
