//! Real fractional funding epochs in the complete sole-Ledger economic owner.
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    EconomicConfig, EconomicInput, EconomicState, FundingConfig, FundingPolicy, FundingStatus,
    SettlementPolicy,
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
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-economic-funding-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && self.0.starts_with(std::env::temp_dir()));
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("tl-economic-funding-"));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}
fn a(text: &str) -> AccountId {
    AccountId::try_from(text).unwrap()
}
fn p() -> PropertyId {
    PropertyId::try_from(7).unwrap()
}
fn c() -> ContractId {
    ContractId::try_from(1).unwrap()
}
fn id(n: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&n.to_be_bytes());
    TxId::from_bytes(bytes)
}
fn config(network: TradeLayerMainnet, staleness: u64) -> EconomicConfig {
    EconomicConfig {
        native_entitlements: None,
        insurance: None,
        source_consensus: None,
        contract_binding: None,
        network,
        contract: c(),
        collateral: p(),
        inverse: false,
        notional: d("1"),
        initial_margin_per_contract: d("10"),
        fee_route: FeeRoute::Oracle(1),
        policy: SettlementPolicy::ConservingV1,
        marking: None,
        liquidation: None,
        amm: None,
        funding: Some(FundingConfig {
            oracle_id: 1,
            max_staleness_blocks: staleness,
            policy: FundingPolicy::ConservingFundingV1,
        }),
    }
}
fn ledger() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(1),
            Operation::CreateProperty {
                id: p(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 4000 * ATOMS,
            },
        )
        .unwrap();
    for (n, owner) in [(2, "bob"), (3, "carol"), (4, "dave")] {
        ledger
            .apply(
                id(n),
                Operation::Transfer {
                    id: p(),
                    from: a("alice"),
                    to: a(owner),
                    amount_atoms: 1000 * ATOMS,
                },
            )
            .unwrap();
    }
    ledger
}
fn registry(n: u64, action: RegistryAction) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(n),
        actor: a("publisher"),
        operation: JointOperation::Registry(action),
    })
}
fn activate(n: u64, types: Vec<u8>) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(n),
        actor: a("admin"),
        operation: JointOperation::Registry(RegistryAction::Activate {
            types,
            code_hash: [0x75; 32],
        }),
    })
}
fn create(n: u64) -> EconomicInput {
    registry(
        n,
        RegistryAction::CreateOracle {
            ticker: "FUND".into(),
            url: "https://example.invalid/fund".into(),
            backup: a("backup"),
            lag: 1,
        },
    )
}
fn publish(n: u64, oracle_id: u32, price: u128) -> EconomicInput {
    registry(
        n,
        RegistryAction::PublishOracle {
            oracle_id,
            price_1e4: price * 10_000,
            high_1e4: None,
            low_1e4: None,
            close_1e4: None,
        },
    )
}
fn order(n: u64, owner: &str, side: Side, quantity: &str) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: id(n),
        owner: a(owner),
        contract: c(),
        height: 2,
        side,
        quantity: d(quantity),
        price: d("100"),
        post: false,
        is_market: false,
    })
}
fn opening() -> Vec<EconomicInput> {
    vec![
        order(20, "alice", Side::Buy, "0.5"),
        order(21, "bob", Side::Sell, "0.5"),
    ]
}
fn new(network: TradeLayerMainnet, staleness: u64) -> EconomicState {
    let joint = JointProtocolState::from_bootstrap_unverified(
        network,
        a("admin"),
        ledger(),
        BTreeSet::from([p()]),
    )
    .unwrap();
    let mut state =
        EconomicState::from_joint_unverified(config(network, staleness), joint).unwrap();
    state
        .apply_block_unverified(1, vec![activate(10, vec![2, 6, 13, 14, 15, 18])])
        .unwrap();
    state
}
fn through(state: &mut EconomicState, height: u64) {
    while state.finalized_height().unwrap_or(0) < height {
        state
            .apply_block_unverified(state.finalized_height().unwrap_or(0) + 1, vec![])
            .unwrap();
    }
}
fn before_epoch(network: TradeLayerMainnet, first: u128, last: u128) -> EconomicState {
    let mut state = new(network, 32);
    let mut inputs = vec![create(11), publish(12, 1, first)];
    inputs.extend(opening());
    state.apply_block_unverified(2, inputs).unwrap();
    through(&mut state, 22);
    state
        .apply_block_unverified(23, vec![publish(230, 1, last)])
        .unwrap();
    state
}
fn balance(state: &EconomicState, owner: &str) -> u128 {
    state.ledger().available_atoms(p(), &a(owner))
}
fn assert_conserved(state: &EconomicState) {
    state.verify().unwrap();
    state.ledger().verify_conservation().unwrap();
    let available: u128 = ["alice", "bob", "carol", "dave"]
        .iter()
        .map(|owner| balance(state, owner))
        .sum();
    let margin: u128 = state
        .positions()
        .positions()
        .map(|p| state.margin_atoms(&p.address))
        .sum();
    let reserves: u128 = state
        .book()
        .resting()
        .iter()
        .map(|o| state.reserve_atoms(o.order.txid))
        .sum();
    let fees = state
        .fees()
        .expected_custody_atoms(FeeRoute::Oracle(1).key(p()).unwrap())
        .unwrap();
    assert_eq!(
        available
            + margin
            + reserves
            + fees
            + state.insurance_custody_atoms()
            + state.loss_pool_atoms(),
        4000 * ATOMS
    );
}

#[test]
fn empty_due_epoch_pays_fractional_longs_once_at_24_on_all_three_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let mut state = before_epoch(network, 99, 101);
        assert!(state.funding_state().epochs.is_empty());
        let before_alice = balance(&state, "alice");
        let before_bob = balance(&state, "bob");
        let margin = state.margin_atoms(&a("alice"));
        let trades = state.receipts().to_vec();
        assert_eq!(state.apply_block_unverified(24, vec![]).unwrap(), 0);
        let epoch = &state.funding_state().epochs[0];
        assert_eq!(epoch.height, 24);
        assert_eq!(epoch.status, FundingStatus::Paid);
        let plan = epoch.plan.as_ref().unwrap();
        assert_eq!(plan.rate_bps, d("11.875"));
        assert_eq!(plan.collateral_notional, d("101"));
        assert_eq!(plan.total_atoms, 5_996_875);
        assert_eq!(balance(&state, "alice"), before_alice - 5_996_875);
        assert_eq!(balance(&state, "bob"), before_bob + 5_996_875);
        assert_eq!(state.margin_atoms(&a("alice")), margin);
        assert_eq!(state.receipts(), trades);
        let epoch = epoch.clone();
        state.apply_block_unverified(25, vec![]).unwrap();
        assert_eq!(state.funding_state().epochs, [epoch]);
        assert_eq!(balance(&state, "alice"), before_alice - 5_996_875);
        assert!(state.joint().snapshot_bytes().is_err());
        assert_conserved(&state);
    }
}

#[test]
fn negative_epoch_charges_shorts_and_credits_longs_using_oracle_collateral_notional() {
    let mut state = before_epoch(TradeLayerMainnet::Litecoin, 101, 99);
    let alice = balance(&state, "alice");
    let bob = balance(&state, "bob");
    state.apply_block_unverified(24, vec![]).unwrap();
    let plan = state.funding_state().epochs[0].plan.as_ref().unwrap();
    assert_eq!(plan.rate_bps, d("-11.875"));
    assert_eq!(plan.collateral_notional, d("99"));
    assert_eq!(plan.total_atoms, 5_878_125);
    assert_eq!(plan.debits[0].owner, a("bob"));
    assert_eq!(balance(&state, "bob"), bob - 5_878_125);
    assert_eq!(balance(&state, "alice"), alice + 5_878_125);
    assert_conserved(&state);
}

#[test]
fn missing_stale_closed_zero_and_no_position_epochs_are_explicit_and_fenced() {
    for expected in [
        FundingStatus::MissingPrice,
        FundingStatus::StalePrice,
        FundingStatus::ClosedOracle,
        FundingStatus::ZeroRate,
        FundingStatus::NoPositions,
    ] {
        let mut state = new(
            TradeLayerMainnet::Litecoin,
            if expected == FundingStatus::StalePrice {
                0
            } else {
                32
            },
        );
        let mut inputs = vec![create(11)];
        if expected != FundingStatus::MissingPrice {
            inputs.push(publish(12, 1, 99));
        }
        state.apply_block_unverified(2, inputs).unwrap();
        through(&mut state, 22);
        let inputs = if expected == FundingStatus::ClosedOracle {
            vec![registry(230, RegistryAction::CloseOracle { oracle_id: 1 })]
        } else if expected == FundingStatus::NoPositions {
            vec![publish(230, 1, 101)]
        } else {
            vec![]
        };
        state.apply_block_unverified(23, inputs).unwrap();
        let alice = balance(&state, "alice");
        state.apply_block_unverified(24, vec![]).unwrap();
        let epoch = &state.funding_state().epochs[0];
        assert_eq!(epoch.status, expected);
        assert_eq!(epoch.plan.as_ref().map_or(0, |p| p.total_atoms), 0);
        assert_eq!(balance(&state, "alice"), alice);
        assert!(state.joint().snapshot_bytes().is_err());
        let bytes = state.snapshot_bytes().unwrap();
        assert_eq!(
            EconomicState::from_snapshot_bytes(
                &bytes,
                state.config(),
                &a("admin"),
                &BTreeSet::from([p()])
            )
            .unwrap(),
            state
        );
        assert_conserved(&state);
    }
}

#[test]
fn same_height_accepted_publications_replace_sample_and_ignore_other_oracles() {
    let mut state = new(TradeLayerMainnet::Litecoin, 32);
    let mut inputs = vec![create(11), create(13), publish(12, 1, 99)];
    inputs.extend(opening());
    state.apply_block_unverified(2, inputs).unwrap();
    through(&mut state, 22);
    let before = state.clone();
    state
        .apply_block_unverified(
            23,
            vec![
                publish(231, 1, 101),
                publish(232, 2, 999),
                publish(233, 1, 100),
            ],
        )
        .unwrap();
    state.apply_block_unverified(24, vec![]).unwrap();
    let epoch = &state.funding_state().epochs[0];
    assert_eq!(epoch.status, FundingStatus::Paid);
    assert_eq!(
        epoch.window.iter().map(|o| o.txid).collect::<Vec<_>>(),
        [id(12), id(233)]
    );
    assert_eq!(epoch.index.as_ref().unwrap().txid, id(233));
    assert_eq!(epoch.plan.as_ref().unwrap().rate_bps, d("5.6563"));
    assert_eq!(epoch.plan.as_ref().unwrap().total_atoms, 2_828_150);
    // JointRegistry accepts both external events, while the funding history
    // models one final stored observation per oracle/height. This is distinct
    // from OracleList.publishData's first-attempt same-height admission guard.
    assert!(state.joint().registry().is_processed(id(231)));
    assert!(state.joint().registry().is_processed(id(233)));
    assert_eq!(
        state
            .joint()
            .registry()
            .oracle(2)
            .unwrap()
            .latest
            .as_ref()
            .unwrap()
            .price_1e4,
        999 * 10_000
    );
    let mut reversed = before;
    reversed
        .apply_block_unverified(
            23,
            vec![
                publish(233, 1, 100),
                publish(232, 2, 999),
                publish(231, 1, 101),
            ],
        )
        .unwrap();
    reversed.apply_block_unverified(24, vec![]).unwrap();
    assert_eq!(
        reversed.funding_state().epochs[0].status,
        FundingStatus::Paid
    );
    assert_eq!(
        reversed.funding_state().epochs[0]
            .plan
            .as_ref()
            .unwrap()
            .total_atoms,
        5_996_875
    );
    assert_conserved(&state);
    assert_conserved(&reversed);
}

#[test]
fn last_payer_failure_reverts_late_ordinary_sends_epoch_id_and_all_custody_then_retry_works() {
    let mut state = new(TradeLayerMainnet::Litecoin, 32);
    state
        .apply_block_unverified(
            2,
            vec![
                create(11),
                publish(12, 1, 99),
                order(20, "alice", Side::Buy, "0.25"),
                order(21, "carol", Side::Buy, "0.25"),
                order(22, "bob", Side::Sell, "0.5"),
            ],
        )
        .unwrap();
    through(&mut state, 22);
    state
        .apply_block_unverified(23, vec![publish(230, 1, 101)])
        .unwrap();
    let before = state.clone();
    let bytes = state.snapshot_bytes().unwrap();
    let carol = balance(&state, "carol");
    let send = |n, from: &str, amount_atoms| {
        EconomicInput::Joint(UnverifiedJointTx {
            txid: id(n),
            actor: a(from),
            operation: JointOperation::Send {
                id: p(),
                recipient: a("dave"),
                amount_atoms,
            },
        })
    };
    assert!(state
        .apply_block_unverified(
            24,
            vec![send(240, "alice", ATOMS), send(241, "carol", carol)]
        )
        .is_err());
    assert_eq!(state, before);
    assert_eq!(state.snapshot_bytes().unwrap(), bytes);
    assert!(!state.ledger().is_processed(id(240)));
    assert!(!state.ledger().is_processed(id(241)));
    assert!(state.funding_state().epochs.is_empty());
    state
        .apply_block_unverified(
            24,
            vec![
                send(240, "alice", ATOMS),
                send(241, "carol", carol - 2_998_438),
            ],
        )
        .unwrap();
    assert_eq!(balance(&state, "carol"), 0);
    let plan = state.funding_state().epochs[0].plan.as_ref().unwrap();
    assert_eq!(plan.total_atoms, 5_996_876);
    assert_eq!(plan.debits.len(), 2);
    assert_conserved(&state);
}

#[test]
fn epoch_requires_prior_activation_and_inclusive_192_block_window_is_preserved() {
    let network = TradeLayerMainnet::Litecoin;
    let joint = JointProtocolState::from_bootstrap_unverified(
        network,
        a("admin"),
        ledger(),
        BTreeSet::from([p()]),
    )
    .unwrap();
    let mut state = EconomicState::from_joint_unverified(config(network, 192), joint).unwrap();
    state
        .apply_block_unverified(1, vec![activate(10, vec![2, 6, 13, 14, 15])])
        .unwrap();
    state.apply_block_unverified(2, vec![create(11)]).unwrap();
    through(&mut state, 22);
    state
        .apply_block_unverified(23, vec![publish(230, 1, 99)])
        .unwrap();
    state
        .apply_block_unverified(24, vec![activate(240, vec![18]), publish(241, 1, 101)])
        .unwrap();
    assert!(state.funding_state().epochs.is_empty());
    through(&mut state, 214);
    state
        .apply_block_unverified(215, vec![publish(2150, 1, 103)])
        .unwrap();
    state.apply_block_unverified(216, vec![]).unwrap();
    assert_eq!(state.funding_state().epochs[0].height, 48);
    let epoch = state.funding_state().epochs.last().unwrap();
    assert_eq!(epoch.height, 216);
    assert_eq!(
        epoch.window.iter().map(|o| o.height).collect::<Vec<_>>(),
        [24, 215]
    );
    assert_eq!(epoch.plan.as_ref().unwrap().rate_bps, d("11.63"));
    assert_conserved(&state);
}

fn chain_config() -> EconomicChainConfig {
    EconomicChainConfig {
        economics: config(TradeLayerMainnet::Litecoin, 32),
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p()]),
        rollback_limit: 4,
    }
}
fn block(
    chain: &EconomicChain,
    marker: u64,
    inputs: Vec<EconomicInput>,
) -> UnverifiedEconomicBlock {
    UnverifiedEconomicBlock {
        network: chain.config().economics.network,
        header: BlockHeader {
            height: chain.tip().height + 1,
            id: BlockId(id(marker).as_bytes()),
            parent: chain.tip().id,
        },
        inputs,
    }
}

#[test]
fn paid_epoch_full_checkpoint_disk_restart_rollback_and_changed_mark_rebranch_are_exact() {
    let dir = Directory::new();
    let cfg = chain_config();
    let mut chain = EconomicChain::new_unverified(cfg.clone(), ledger()).unwrap();
    chain
        .apply_unverified_block(&block(
            &chain,
            1001,
            vec![activate(10, vec![2, 6, 13, 14, 15, 18])],
        ))
        .unwrap();
    let mut inputs = vec![create(11), publish(12, 1, 99)];
    inputs.extend(opening());
    chain
        .apply_unverified_block(&block(&chain, 1002, inputs))
        .unwrap();
    while chain.tip().height < 22 {
        chain
            .apply_unverified_block(&block(&chain, 1000 + chain.tip().height + 1, vec![]))
            .unwrap();
    }
    chain
        .apply_unverified_block(&block(&chain, 1023, vec![publish(230, 1, 101)]))
        .unwrap();
    let tip23 = chain.tip();
    let before = chain.state().clone();
    let mut owner = DurableEconomicChain::initialize(&dir.0, chain, tip23).unwrap();
    let due = block(owner.chain().unwrap(), 1024, vec![]);
    let head = owner
        .apply_unverified_block(owner.head().unwrap(), &due)
        .unwrap();
    let paid = owner.chain().unwrap().clone();
    let bytes = paid.checkpoint_bytes().unwrap();
    assert_eq!(
        EconomicChain::from_checkpoint_bytes(&bytes, cfg.clone(), paid.tip()).unwrap(),
        paid
    );
    assert_eq!(
        paid.state().funding_state().epochs[0]
            .plan
            .as_ref()
            .unwrap()
            .total_atoms,
        5_996_875
    );
    drop(owner);
    let (mut owner, report) =
        DurableEconomicChain::load(&dir.0, cfg.clone(), head, Recovery::RejectIncompleteTail)
            .unwrap();
    assert_eq!(report.discarded_bytes, 0);
    assert_eq!(owner.chain().unwrap(), &paid);
    let head = owner.rollback_to(head, tip23).unwrap();
    assert_eq!(owner.chain().unwrap().state(), &before);
    assert!(owner
        .chain()
        .unwrap()
        .state()
        .funding_state()
        .epochs
        .is_empty());
    drop(owner);
    let (mut owner, _) =
        DurableEconomicChain::load(&dir.0, cfg, head, Recovery::RejectIncompleteTail).unwrap();
    let branch = block(owner.chain().unwrap(), 2024, vec![publish(2400, 1, 103)]);
    owner.apply_unverified_block(head, &branch).unwrap();
    let state = owner.chain().unwrap().state();
    assert_eq!(state.funding_state().epochs.len(), 1);
    assert_eq!(
        state.funding_state().epochs[0]
            .plan
            .as_ref()
            .unwrap()
            .total_atoms,
        6_437_500
    );
    assert_eq!(
        balance(state, "alice"),
        balance(&before, "alice") - 6_437_500
    );
    assert_eq!(balance(state, "bob"), balance(&before, "bob") + 6_437_500);
    assert_conserved(state);
}
