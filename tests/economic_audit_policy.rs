//! The audit cadence never changes a result. Every scenario runs the same block
//! sequence under `EveryBlock` and under three `Periodic` intervals; each block
//! must return the same `Ok`/`Err` and leave the same canonical snapshot bytes.
//! History audits run only on the block's additions between full-audit heights,
//! so these runs are the evidence that the split is not observable.
use std::collections::BTreeSet;
use tradelayer_core::contract_orderbook::{CancelCriteria, Order, Side, MAX_CONTRACT_ORDERS};
use tradelayer_core::contract_registry::{ContractAsset, ContractId, ContractParameters};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    contract_definition_commitment, AuditPolicy, ContractBindingConfig, ContractRiskPolicy,
    CreateContractInput, EconomicConfig, EconomicError, EconomicInput, EconomicState, MarkConfig,
    MarkPolicy, SettlementPolicy, MAX_ECONOMIC_INPUTS, MAX_ECONOMIC_RECEIPTS,
};
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::tally::FeeRoute;

const ATOMS: u128 = 100_000_000;
const POLICIES: [AuditPolicy; 4] = [
    AuditPolicy::EveryBlock,
    AuditPolicy::Periodic(64),
    AuditPolicy::Periodic(1),
    AuditPolicy::Periodic(7),
];

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
fn id(number: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&number.to_be_bytes());
    TxId::from_bytes(bytes)
}

// ---- the harness -----------------------------------------------------------

type Block = (u64, Vec<EconomicInput>);

#[derive(Debug, PartialEq, Eq)]
struct Step {
    result: Result<usize, EconomicError>,
    snapshot_len: usize,
    digest: [u64; 2],
}

/// Two independent FNV-1a lanes over the canonical snapshot bytes.
fn digest(bytes: &[u8]) -> [u64; 2] {
    let mut lanes = [0xcbf2_9ce4_8422_2325_u64, 0x8422_2325_cbf2_9ce4_u64];
    for byte in bytes {
        lanes[0] = (lanes[0] ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        lanes[1] = (lanes[1] ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3 ^ 0x2f);
    }
    lanes
}

fn run(initial: &EconomicState, policy: AuditPolicy, blocks: &[Block]) -> Vec<Step> {
    let mut state = initial.clone();
    state.set_audit_policy(policy);
    assert_eq!(state.audit_policy(), policy);
    blocks
        .iter()
        .map(|(height, inputs)| {
            let result = state.apply_block_unverified(*height, inputs.clone());
            // `snapshot_bytes` audits completely, so a violation the cheaper
            // per-block path missed fails here at the same block.
            let bytes = state.snapshot_bytes().unwrap();
            assert_eq!(state.audit_policy(), policy, "the policy survives blocks");
            Step {
                result,
                snapshot_len: bytes.len(),
                digest: digest(&bytes),
            }
        })
        .collect()
}

/// Runs all policies and asserts every block matches `EveryBlock`. Returns the
/// reference steps for scenario-specific assertions.
fn differential(name: &str, initial: &EconomicState, blocks: &[Block]) -> Vec<Step> {
    let mut runs: Vec<Vec<Step>> = std::thread::scope(|scope| {
        let handles: Vec<_> = POLICIES
            .iter()
            .map(|policy| scope.spawn(|| run(initial, *policy, blocks)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let reference = runs.remove(0);
    assert_eq!(reference.len(), blocks.len());
    for (policy, steps) in POLICIES[1..].iter().zip(&runs) {
        for (index, (got, want)) in steps.iter().zip(&reference).enumerate() {
            assert_eq!(
                got, want,
                "{name}: block {} (index {index}) under {policy:?} differs from EveryBlock",
                blocks[index].0
            );
        }
    }
    reference
}

fn ok_count(steps: &[Step]) -> usize {
    steps.iter().filter(|s| s.result.is_ok()).count()
}

// ---- plain owner: fixed margin, the economic.rs fixtures -------------------

fn plain_config() -> EconomicConfig {
    EconomicConfig {
        native_entitlements: None,
        insurance: None,
        contract_binding: None,
        source_consensus: None,
        network: TradeLayerMainnet::Litecoin,
        contract: c(),
        collateral: p(),
        inverse: false,
        notional: d("1"),
        initial_margin_per_contract: d("10"),
        fee_route: FeeRoute::Native(1),
        policy: SettlementPolicy::ConservingV1,
        funding: None,
        marking: None,
        liquidation: None,
        amm: None,
    }
}

/// `holders`: (owner, whole tokens). Alice issues the supply, funds the holders
/// and keeps `alice_keeps` tokens.
fn plain_bootstrap(holders: &[(&str, u128)], alice_keeps: u128) -> JointProtocolState {
    let total: u128 = holders.iter().map(|(_, t)| t).sum::<u128>() + alice_keeps;
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(1),
            Operation::CreateProperty {
                id: p(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: total * ATOMS,
            },
        )
        .unwrap();
    for (n, (owner, tokens)) in holders.iter().enumerate() {
        ledger
            .apply(
                id(2 + n as u64),
                Operation::Transfer {
                    id: p(),
                    from: a("alice"),
                    to: a(owner),
                    amount_atoms: tokens * ATOMS,
                },
            )
            .unwrap();
    }
    JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Litecoin,
        a("admin"),
        ledger,
        BTreeSet::from([p()]),
    )
    .unwrap()
}

fn plain_state(holders: &[(&str, u128)], alice_keeps: u128) -> EconomicState {
    EconomicState::from_joint_unverified(plain_config(), plain_bootstrap(holders, alice_keeps))
        .unwrap()
}

fn activate_plain() -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(10),
        actor: a("admin"),
        operation: JointOperation::Registry(RegistryAction::Activate {
            types: vec![2, 6, 18],
            code_hash: [0x71; 32],
        }),
    })
}

fn order(
    number: u64,
    owner: &str,
    side: Side,
    quantity: &str,
    price: &str,
    height: u64,
) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: id(number),
        owner: a(owner),
        contract: c(),
        height,
        side,
        quantity: d(quantity),
        price: d(price),
        post: false,
        is_market: false,
    })
}

fn cancel(number: u64, owner: &str, criteria: CancelCriteria) -> EconomicInput {
    EconomicInput::Cancel {
        txid: id(number),
        owner: a(owner),
        criteria,
    }
}

fn send(number: u64, from: &str, to: &str, tokens: u128) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(number),
        actor: a(from),
        operation: JointOperation::Send {
            id: p(),
            recipient: a(to),
            amount_atoms: tokens * ATOMS,
        },
    })
}

fn pair(height: u64, first: u64, buyer: &str, seller: &str, qty: &str, price: &str) -> Block {
    (
        height,
        vec![
            order(first, buyer, Side::Buy, qty, price, height),
            order(first + 1, seller, Side::Sell, qty, price, height),
        ],
    )
}

/// The bootstrap of tests/economic.rs: alice keeps 1000 tokens of 4010.
fn economic_fixture() -> EconomicState {
    plain_state(
        &[("bob", 1000), ("carol", 1000), ("dave", 1000), ("poor", 10)],
        1000,
    )
}

// Ported from tests/economic.rs and tests/economic_review.rs.

#[test]
fn ported_open_increase_reduce_close_and_flip() {
    let initial = economic_fixture();
    let blocks = vec![
        (1, vec![activate_plain()]),
        pair(2, 20, "alice", "bob", "1.25", "100"),
        pair(3, 30, "alice", "bob", "0.75", "120"),
        pair(4, 40, "bob", "alice", "0.5", "130"),
        pair(5, 50, "bob", "alice", "1.5", "110"),
        pair(6, 60, "alice", "bob", "2", "90"),
        pair(7, 70, "bob", "alice", "3", "95"),
        (8, vec![]),
    ];
    let steps = differential("open/flip", &initial, &blocks);
    assert_eq!(ok_count(&steps), blocks.len());
}

#[test]
fn ported_deferred_profit_claim_and_unfunded_loss() {
    let initial = economic_fixture();
    let blocks = vec![
        (1, vec![activate_plain()]),
        pair(2, 20, "alice", "bob", "1", "100"),
        pair(3, 30, "carol", "alice", "0.5", "110"),
        pair(4, 40, "bob", "dave", "0.5", "110"),
        (5, vec![]),
    ];
    assert_eq!(
        ok_count(&differential("deferred claim", &initial, &blocks)),
        5
    );
    // Prices far enough apart that the loser cannot fund the loss.
    let blocks = vec![
        (1, vec![activate_plain()]),
        pair(2, 20, "alice", "bob", "1", "100"),
        pair(3, 30, "carol", "alice", "0.5", "2100"),
        pair(4, 40, "bob", "dave", "0.5", "4100"),
        (5, vec![]),
    ];
    assert_eq!(
        ok_count(&differential("unfunded loss", &initial, &blocks)),
        5
    );
}

#[test]
fn ported_cancellation_replay_and_failed_blocks() {
    let initial = economic_fixture();
    let blocks = vec![
        (1, vec![activate_plain()]),
        (2, vec![order(20, "alice", Side::Buy, "1.25", "100", 2)]),
        // bob cannot cancel alice's order: accepted as a no-op input.
        (3, vec![cancel(30, "bob", CancelCriteria::Txid(id(20)))]),
        (4, vec![cancel(40, "alice", CancelCriteria::Txid(id(20)))]),
        // Replayed order id: the block fails and changes nothing.
        (5, vec![order(20, "alice", Side::Buy, "1.25", "100", 5)]),
        // Replayed cancel id and a replayed ordinary send id fail the same way.
        (5, vec![cancel(40, "alice", CancelCriteria::All)]),
        (5, vec![send(20, "alice", "bob", 1)]),
        (5, vec![send(50, "alice", "bob", 1)]),
        (6, vec![order(60, "bob", Side::Sell, "1", "100", 6)]),
    ];
    let steps = differential("cancellation", &initial, &blocks);
    assert_eq!(
        steps.iter().filter(|s| s.result.is_err()).count(),
        3,
        "the three replayed ids fail"
    );
}

#[test]
fn ported_last_fee_leg_failure_discards_the_block_then_retries() {
    let initial = economic_fixture();
    let blocks = vec![
        (1, vec![activate_plain()]),
        (2, vec![order(20, "alice", Side::Buy, "1", "100", 2)]),
        // poor can reserve the margin but has nothing left for the taker fee.
        (
            3,
            vec![
                send(30, "bob", "dave", 1),
                order(31, "poor", Side::Sell, "1", "100", 3),
            ],
        ),
        (
            3,
            vec![
                send(30, "bob", "poor", 1),
                order(31, "poor", Side::Sell, "1", "100", 3),
            ],
        ),
        (4, vec![]),
    ];
    let steps = differential("fee leg failure", &initial, &blocks);
    assert!(steps[2].result.is_err());
    assert_eq!(steps[3].result, Ok(1));
}

#[test]
fn ported_partial_fill_then_self_cross() {
    let initial = plain_state(&[("bob", 1000)], 1000);
    let blocks = vec![
        (1, vec![activate_plain()]),
        (
            2,
            vec![
                order(20, "alice", Side::Sell, "1", "100", 2),
                order(21, "bob", Side::Sell, "1", "90", 2),
                order(22, "alice", Side::Buy, "2", "100", 2),
            ],
        ),
        (3, vec![]),
    ];
    let steps = differential("self cross", &initial, &blocks);
    assert_eq!(steps[1].result, Ok(1));
}

// ---- 1,000+ blocks of market-maker and taker flow --------------------------

/// mulberry32, as in the wasm playground benchmark.
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

fn bump(next: &mut u64) -> u64 {
    *next += 1;
    *next
}

fn cents(c: u64) -> String {
    format!("{}.{:02}", c / 100, c % 100)
}

/// Market maker: cancel everything, then a three-level ladder per side. A taker
/// order follows with probability 0.6. Periodically a block replays an earlier
/// id and fails, and another sends the poor account into a funding failure.
fn market_maker_blocks(blocks: u64) -> Vec<Block> {
    let mut rng = Rng(1);
    let mut price = 100.0_f64;
    let mut next_id = 1000_u64;
    let mut out = vec![(1, vec![activate_plain()])];
    for k in 0..blocks {
        let height = 2 + k;
        price *= 1.0 + (rng.next() - 0.5) * 2.0 * 0.001;
        let mid = (price * 100.0 + 0.5).floor() as u64;
        let mut inputs = vec![cancel(bump(&mut next_id), "mm", CancelCriteria::All)];
        for level in 0..1 {
            let off = mid * (5 + 5 * level) / 10_000;
            inputs.push(order(
                bump(&mut next_id),
                "mm",
                Side::Buy,
                "0.5",
                &cents(mid - off),
                height,
            ));
            inputs.push(order(
                bump(&mut next_id),
                "mm",
                Side::Sell,
                "0.5",
                &cents(mid + off),
                height,
            ));
        }
        if rng.next() < 0.6 {
            let trader = format!("t{}", (rng.next() * 4.0).floor() as u32);
            let buy = rng.next() < 0.5;
            let qty = format!("0.{}", 1 + (rng.next() * 3.0).floor() as u64);
            let side = if buy { Side::Buy } else { Side::Sell };
            let px = if buy {
                mid * 10_020 / 10_000
            } else {
                mid * 9_980 / 10_000
            };
            inputs.push(order(
                bump(&mut next_id),
                &trader,
                side,
                &qty,
                &cents(px),
                height,
            ));
        }
        if k % 97 == 96 {
            // Replays an id from block 2 after an input that applied: the block
            // fails and the same height is then used by the real block.
            out.push((
                height,
                vec![
                    cancel(bump(&mut next_id), "mm", CancelCriteria::All),
                    order(1001, "t0", Side::Buy, "0.1", &cents(mid), height),
                ],
            ));
        }
        if k % 131 == 130 {
            // "poor" cannot fund the margin of the last order of this block.
            out.push((
                height,
                vec![
                    order(
                        bump(&mut next_id),
                        "t1",
                        Side::Buy,
                        "0.1",
                        &cents(mid),
                        height,
                    ),
                    order(
                        bump(&mut next_id),
                        "poor",
                        Side::Buy,
                        "5",
                        &cents(mid),
                        height,
                    ),
                ],
            ));
        }
        out.push((height, inputs));
    }
    out
}

#[test]
fn market_maker_and_taker_flow_over_a_thousand_blocks() {
    let initial = plain_state(
        &[
            ("mm", 5_000_000),
            ("t0", 500_000),
            ("t1", 500_000),
            ("t2", 500_000),
            ("t3", 500_000),
            ("poor", 1),
        ],
        10,
    );
    let n: u64 = 1000;
    let blocks = market_maker_blocks(n);
    let steps = differential("market maker", &initial, &blocks);
    let failed: Vec<_> = steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.result.is_err())
        .map(|(i, _)| blocks[i].0)
        .collect();
    let injected =
        (0..n).filter(|k| k % 97 == 96).count() + (0..n).filter(|k| k % 131 == 130).count();
    assert_eq!(
        failed.len(),
        injected,
        "the injected failing blocks fail: {failed:?}"
    );
    let fills: usize = steps.iter().filter_map(|s| s.result.as_ref().ok()).sum();
    assert!(fills > 400, "the flow trades: {fills}");
}

// ---- accepted-contract mode ------------------------------------------------

fn contract_creation() -> CreateContractInput {
    CreateContractInput {
        txid: id(30),
        issuer: a("issuer"),
        parameters: ContractParameters {
            native: false,
            underlying_oracle_id: Some(1),
            on_chain_data: vec![],
            notional_property_id: ContractAsset::Property(8.try_into().unwrap()),
            notional_value: d("1"),
            collateral_property_id: ContractAsset::Property(p()),
            leverage: d("10"),
            expiry_period: 0,
            series: 1,
            inverse: false,
            fee: Decimal::zero(),
            whitelist: Some(0),
            ticker: None,
        },
    }
}

fn contract_bootstrap() -> JointProtocolState {
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
    for (number, owner) in [(2, "bob"), (3, "carol"), (4, "dave")] {
        ledger
            .apply(
                id(number),
                Operation::Transfer {
                    id: p(),
                    from: a("alice"),
                    to: a(owner),
                    amount_atoms: 1000 * ATOMS,
                },
            )
            .unwrap();
    }
    JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Litecoin,
        a("admin"),
        ledger,
        BTreeSet::from([p()]),
    )
    .unwrap()
}

fn contract_config(input: &CreateContractInput) -> EconomicConfig {
    EconomicConfig {
        native_entitlements: None,
        insurance: None,
        source_consensus: None,
        network: TradeLayerMainnet::Litecoin,
        contract: c(),
        collateral: p(),
        inverse: input.parameters.inverse,
        notional: input.parameters.notional_value.clone(),
        initial_margin_per_contract: Decimal::zero(),
        fee_route: FeeRoute::Oracle(1),
        policy: SettlementPolicy::ConservingV1,
        funding: None,
        marking: Some(MarkConfig {
            oracle_id: 1,
            policy: MarkPolicy::ConservingMarkBasisV1,
        }),
        liquidation: None,
        amm: None,
        contract_binding: Some(ContractBindingConfig {
            creation_txid: input.txid,
            definition_hash: contract_definition_commitment(input).unwrap(),
            bootstrap_commitment: EconomicState::contract_bootstrap_commitment(
                &contract_bootstrap(),
            )
            .unwrap(),
            policy: ContractRiskPolicy::SourceContextsConservingV1,
        }),
    }
}

fn registry(number: u64, actor: &str, action: RegistryAction) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(number),
        actor: a(actor),
        operation: JointOperation::Registry(action),
    })
}

fn publish(number: u64, price: u128) -> EconomicInput {
    registry(
        number,
        "publisher",
        RegistryAction::PublishOracle {
            oracle_id: 1,
            price_1e4: price * 10_000,
            high_1e4: None,
            low_1e4: None,
            close_1e4: None,
        },
    )
}

fn contract_blocks(blocks: u64) -> Vec<Block> {
    let mut out = vec![
        (
            1,
            vec![registry(
                10,
                "admin",
                RegistryAction::Activate {
                    types: vec![2, 6, 10, 13, 14, 15, 16, 18],
                    code_hash: [0x79; 32],
                },
            )],
        ),
        (
            2,
            vec![
                registry(
                    20,
                    "publisher",
                    RegistryAction::CreateOracle {
                        ticker: "INDEX".into(),
                        url: "https://example.invalid/index".into(),
                        backup: a("backup"),
                        lag: 1,
                    },
                ),
                EconomicInput::CreateContract(contract_creation()),
                publish(21, 100),
            ],
        ),
    ];
    let owners = ["alice", "bob", "carol", "dave"];
    // A mark epoch runs for each published price, so prices are published every
    // tenth block and trades happen every fifth block at the latest mark.
    let mut mark = 100_u128;
    for height in 3..3 + blocks {
        let mut inputs = Vec::new();
        if height.is_multiple_of(10) {
            mark = 100 + u128::from(height / 10 % 5);
            inputs.push(publish(height * 100 + 90, mark));
        }
        if height.is_multiple_of(5) {
            let buyer = owners[height as usize / 5 % 4];
            let seller = owners[(height as usize / 5 + 1) % 4];
            let at = mark.to_string();
            inputs.push(order(height * 100, buyer, Side::Buy, "0.5", &at, height));
            inputs.push(order(
                height * 100 + 1,
                seller,
                Side::Sell,
                "0.5",
                &at,
                height,
            ));
        }
        if height == 3 + blocks / 2 {
            // A failing block in the middle: replays the oracle publication id.
            out.push((height, vec![publish(21, 100)]));
        }
        out.push((height, inputs));
    }
    out
}

#[test]
fn accepted_contract_mode_over_a_long_history() {
    let input = contract_creation();
    let initial =
        EconomicState::from_joint_unverified(contract_config(&input), contract_bootstrap())
            .unwrap();
    // EveryBlock re-executes every frame, with a complete audit per replayed
    // block, three times per block: the reference run costs O(blocks^3). Debug
    // builds run 70 blocks, which still cross the full-audit heights 7, 14, ..,
    // 63 and 64 and a mid-run failure; release builds run the 200-block history.
    let n: u64 = if cfg!(debug_assertions) { 70 } else { 200 };
    let blocks = contract_blocks(n);
    let steps = differential("accepted contract", &initial, &blocks);
    assert_eq!(steps.iter().filter(|s| s.result.is_err()).count(), 1);
    let fills: usize = steps.iter().filter_map(|s| s.result.as_ref().ok()).sum();
    assert!(fills as u64 >= n / 10, "the flow trades: {fills}");
}

// ---- capacity pressure -----------------------------------------------------

#[test]
fn capacity_rejects_the_same_block_under_every_policy() {
    let holders = [
        ("bob", 10_000_000),
        ("carol", 10_000_000),
        ("dave", 10_000_000),
    ];
    let initial = plain_state(&holders, 10_000_000);
    let owners = ["alice", "bob", "carol", "dave"];
    let mut next_id = 1000_u64;
    let mut blocks: Vec<Block> = vec![(1, vec![activate_plain()])];
    let mut height = 2;
    // Fill the book: buys far below any sell, 1,024 per block, until full.
    for _ in 0..MAX_CONTRACT_ORDERS / 1024 {
        let mut inputs = Vec::new();
        for i in 0..1024 {
            next_id += 1;
            inputs.push(order(
                next_id,
                owners[i % 4],
                Side::Buy,
                "0.01",
                "50",
                height,
            ));
        }
        blocks.push((height, inputs));
        height += 1;
    }
    let full_height = height - 1;
    // One more resting order is rejected by the book capacity.
    next_id += 1;
    blocks.push((
        height,
        vec![order(next_id, "alice", Side::Buy, "0.01", "50", height)],
    ));
    let book_reject = blocks.len() - 1;
    // Release the book, then trade pairs until a capacity bound rejects a block.
    let mut releases = Vec::new();
    for owner in owners {
        next_id += 1;
        releases.push(cancel(next_id, owner, CancelCriteria::All));
    }
    blocks.push((height, releases));
    height += 1;
    // More inputs than a block may carry: rejected before any input applies.
    let oversized: Vec<EconomicInput> = (0..=MAX_ECONOMIC_INPUTS as u64)
        .map(|n| order(5_000_000 + n, "alice", Side::Buy, "0.01", "50", height))
        .collect();
    blocks.push((height, oversized));
    let oversized_reject = blocks.len() - 1;
    let pairs_start = blocks.len();
    // Pairs of crossing orders. Each fill adds a receipt and about eight legs,
    // and the staged ledger copies its replay set for every leg, so applying a
    // block costs more as history grows. Debug builds stop well before the old
    // receipt bound; release builds pass it: receipts leave the state for the
    // receipt log after every block, so the lifetime fill count is not bounded.
    let trade_blocks = if cfg!(debug_assertions) { 3 } else { 10 };
    for _ in 0..trade_blocks {
        let mut inputs = Vec::new();
        for i in 0..1000_u32 {
            let (buyer, seller) = if i.is_multiple_of(2) {
                ("alice", "bob")
            } else {
                ("bob", "alice")
            };
            next_id += 1;
            inputs.push(order(next_id, buyer, Side::Buy, "1", "100", height));
            next_id += 1;
            inputs.push(order(next_id, seller, Side::Sell, "1", "100", height));
        }
        blocks.push((height, inputs));
        height += 1;
    }
    let steps = differential("capacity", &initial, &blocks);
    assert!(full_height >= 5);
    assert!(
        matches!(steps[book_reject].result, Err(EconomicError::Book(_))),
        "{:?}",
        steps[book_reject].result
    );
    assert_eq!(steps[oversized_reject].result, Err(EconomicError::Capacity));
    let trading = &steps[pairs_start..];
    let rejected: Vec<_> = trading
        .iter()
        .enumerate()
        .filter(|(_, s)| s.result.is_err())
        .map(|(i, s)| (i, s.result.clone()))
        .collect();
    let fills_before_reject: usize = trading
        .iter()
        .take_while(|s| s.result.is_ok())
        .filter_map(|s| s.result.as_ref().ok())
        .sum();
    assert!(rejected.is_empty(), "{rejected:?}");
    if cfg!(debug_assertions) {
        assert!(fills_before_reject > MAX_ECONOMIC_RECEIPTS / 4);
    } else {
        // More fills than the state could ever hold: 10 blocks of 1,000 fills.
        assert!(fills_before_reject > MAX_ECONOMIC_RECEIPTS);
    }
}
