//! Guarded book liquidation restart and repaired-checksum semantic corruption.
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::contract_orderbook::{
    BookSnapshot, CancelCriteria, Order, RestingOrder, Side,
};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    EconomicConfig, EconomicInput, EconomicState, LiquidationBookInput, LiquidationConfig,
    LiquidationPolicy, LiquidationState, LiquidationStatus, MarkConfig, MarkEventKind, MarkPolicy,
    SettlementPolicy,
};
use tradelayer_core::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::joint::{JointOperation, UnverifiedJointTx};
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
            contract_binding: None,
            network,
            contract: 1.try_into().unwrap(),
            collateral: p(),
            inverse: false,
            notional: d("1"),
            initial_margin_per_contract: d("10"),
            fee_route: FeeRoute::Oracle(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
            marking: Some(MarkConfig {
                oracle_id: 1,
                policy: MarkPolicy::ConservingMarkBasisV1,
            }),
            liquidation: Some(LiquidationConfig {
                max_price_deviation_bps: 1000,
                max_fills_per_owner: 8,
                policy: LiquidationPolicy::ConservingLiquidationV1,
            }),
            amm: None,
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
fn before_liquidation(network: TradeLayerMainnet) -> EconomicChain {
    let mut chain = EconomicChain::new_unverified(config(network), ledger()).unwrap();
    apply(
        &mut chain,
        vec![joint(
            10,
            RegistryAction::Activate {
                types: vec![2, 6, 13, 14, 15, 18],
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
        ],
    );
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
fn seal(mut raw: Vec<u8>) -> Vec<u8> {
    raw.extend_from_slice(&crc32(&raw).to_be_bytes());
    raw
}
fn count(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u32).to_be_bytes());
}
fn decimal(out: &mut Vec<u8>, value: &Decimal) {
    let s = value.to_string();
    out.push(s.len() as u8);
    out.extend_from_slice(s.as_bytes());
}
fn account(out: &mut Vec<u8>, owner: &AccountId) {
    out.push(owner.as_str().len() as u8);
    out.extend_from_slice(owner.as_str().as_bytes());
}

fn order_bytes(out: &mut Vec<u8>, order: &Order) {
    out.extend_from_slice(&order.txid.as_bytes());
    account(out, &order.owner);
    out.extend_from_slice(&order.contract.get().to_be_bytes());
    out.extend_from_slice(&order.height.to_be_bytes());
    out.push(match order.side {
        Side::Buy => 0,
        Side::Sell => 1,
    });
    decimal(out, &order.quantity);
    decimal(out, &order.price);
    out.push(u8::from(order.post));
    out.push(u8::from(order.is_market));
}
fn resting(out: &mut Vec<u8>, value: &RestingOrder) {
    order_bytes(out, &value.order);
    decimal(out, &value.remaining);
    decimal(out, &value.reserve_quantity);
    out.push(u8::from(value.initial_reduce));
    out.extend_from_slice(&value.sequence.to_be_bytes());
}
fn book_bytes(out: &mut Vec<u8>, book: &BookSnapshot) {
    out.extend_from_slice(&book.contract.get().to_be_bytes());
    for orders in [&book.queued, &book.drained] {
        count(out, orders.len());
        for order in orders {
            order_bytes(out, order);
        }
    }
    count(out, book.resting.len());
    for order in &book.resting {
        resting(out, order);
    }
    count(out, book.seen.len());
    for id in &book.seen {
        out.extend_from_slice(&id.as_bytes());
    }
    out.extend_from_slice(&book.next_sequence.to_be_bytes());
    match book.processed_through {
        None => out.push(0),
        Some(height) => {
            out.push(1);
            out.extend_from_slice(&height.to_be_bytes());
        }
    }
}
// The public metadata tail is deliberately serialized independently of the
// production writer; nested Joint/Ledger bytes and CRCs remain unchanged.
fn liquidation_bytes(state: &LiquidationState) -> Vec<u8> {
    let mut out = Vec::new();
    count(&mut out, state.events.len());
    for e in &state.events {
        out.extend_from_slice(&e.height.to_be_bytes());
        out.extend_from_slice(&e.id.as_bytes());
        out.extend_from_slice(&e.mark_id.as_bytes());
        account(&mut out, &e.owner);
        decimal(&mut out, &e.quantity_before);
        for value in [
            e.margin_before_atoms,
            e.available_before_atoms,
            e.required_before_atoms,
        ] {
            out.extend_from_slice(&value.to_be_bytes());
        }
        decimal(&mut out, &e.guard_price);
        book_bytes(&mut out, &e.book_before);
        count(&mut out, e.cancelled.len());
        for cancellation in &e.cancelled {
            resting(&mut out, &cancellation.order);
            out.extend_from_slice(&cancellation.reserve_atoms.to_be_bytes());
        }
        out.extend_from_slice(&e.top_up_atoms.to_be_bytes());
        count(&mut out, e.fills.len());
        for f in &e.fills {
            resting(&mut out, &f.maker_before);
            for value in [&f.quantity, &f.price, &f.distressed_pnl, &f.maker_pnl] {
                decimal(&mut out, value);
            }
            for value in [
                f.maker_available_before_atoms,
                f.maker_margin_before_atoms,
                f.distressed_available_before_fee_atoms,
                f.distressed_margin_before_fee_atoms,
                f.nominal_fee_atoms,
                f.charged_fee_atoms,
                f.maker_rebate_atoms,
                f.retained_fee_atoms,
                f.collected_loss_atoms,
                f.paid_claim_atoms,
                f.unfunded_loss_atoms,
                f.distressed_margin_after_atoms,
            ] {
                out.extend_from_slice(&value.to_be_bytes());
            }
        }
        match &e.blocked_maker {
            None => out.push(0),
            Some(b) => {
                out.push(1);
                resting(&mut out, &b.order);
                out.extend_from_slice(&b.available_atoms.to_be_bytes());
                out.extend_from_slice(&b.margin_atoms.to_be_bytes());
            }
        }
        out.push(match e.status {
            LiquidationStatus::Cured => 0,
            LiquidationStatus::Closed => 1,
            LiquidationStatus::NoLiquidityWithinGuard => 2,
            LiquidationStatus::UnfundedBestMaker => 3,
            LiquidationStatus::FillLimit => 4,
        });
    }
    count(&mut out, state.book_frames.len());
    for frame in &state.book_frames {
        out.extend_from_slice(&frame.height.to_be_bytes());
        count(&mut out, frame.inputs.len());
        for input in &frame.inputs {
            match input {
                LiquidationBookInput::Order(order) => {
                    out.push(0);
                    order_bytes(&mut out, order);
                }
                LiquidationBookInput::Cancel {
                    txid,
                    owner,
                    criteria,
                } => {
                    out.push(1);
                    out.extend_from_slice(&txid.as_bytes());
                    account(&mut out, owner);
                    match criteria {
                        CancelCriteria::All => out.push(0),
                        CancelCriteria::Txid(id) => {
                            out.push(1);
                            out.extend_from_slice(&id.as_bytes());
                        }
                        CancelCriteria::Price { side, price } => {
                            out.push(2);
                            out.push(match side {
                                Side::Buy => 0,
                                Side::Sell => 1,
                            });
                            decimal(&mut out, price);
                        }
                    }
                }
            }
        }
        count(&mut out, frame.placements.len());
        for placement in &frame.placements {
            order_bytes(&mut out, &placement.order);
            decimal(&mut out, &placement.position);
        }
    }
    out
}
fn replace_liquidation(bytes: &[u8], old: &LiquidationState, new: &LiquidationState) -> Vec<u8> {
    let tail = liquidation_bytes(old);
    let end = bytes.len() - 4;
    assert_eq!(&bytes[end - tail.len()..end], tail);
    let mut out = bytes[..end - tail.len()].to_vec();
    out.extend_from_slice(&liquidation_bytes(new));
    seal(out)
}
fn liquidated(network: TradeLayerMainnet) -> EconomicChain {
    let mut chain = before_liquidation(network);
    apply(&mut chain, vec![publication(13, 1000)]);
    assert_eq!(chain.state().liquidation_state().events.len(), 1);
    assert_eq!(chain.state().liquidation_state().events[0].fills.len(), 1);
    chain
}

#[test]
fn v4_forced_close_restores_full_owner_and_reserve_consumption_on_three_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let chain = liquidated(network);
        let state = chain.state();
        let bytes = state.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..5], b"TLEC\x04");
        assert_eq!(read(&bytes, state).unwrap(), *state);
        assert_eq!(
            read(&bytes, state).unwrap().snapshot_bytes().unwrap(),
            bytes
        );
        assert_eq!(state.positions().get(&a("bob")).unwrap().contracts, d("0"));
        assert_eq!(
            state.positions().get(&a("carol")).unwrap().contracts,
            d("-1")
        );
        assert_eq!(state.reserve_atoms(tx(30)), 0);
        assert_eq!(state.liquidation_state().events[0].cancelled.len(), 1);
        assert!(state.book().resting().is_empty());
        assert!(matches!(
            state.mark_state().events.last().unwrap().kind,
            MarkEventKind::Forced { .. }
        ));
        let checkpoint = chain.checkpoint_bytes().unwrap();
        let mut restored =
            EconomicChain::from_checkpoint_bytes(&checkpoint, chain.config().clone(), chain.tip())
                .unwrap();
        apply(&mut restored, vec![]);
        restored.rollback_tip(restored.tip()).unwrap();
        assert_eq!(restored, chain);
        assert!(state.joint().snapshot_bytes().is_err());
    }
}

#[test]
fn repaired_crc_does_not_authorize_changed_liquidation_identity_fills_fees_book_or_status() {
    let chain = liquidated(TradeLayerMainnet::Bitcoin);
    let state = chain.state();
    let bytes = state.snapshot_bytes().unwrap();
    let old = state.liquidation_state();
    let mut variants = Vec::new();
    let mut value = old.clone();
    value.events[0].id = tx(99);
    variants.push(value);
    let mut value = old.clone();
    value.events[0].mark_id = tx(10);
    variants.push(value);
    let mut value = old.clone();
    value.events[0].quantity_before = d("-2");
    variants.push(value);
    let mut value = old.clone();
    value.events[0].guard_price = d("1");
    variants.push(value);
    let mut value = old.clone();
    value.events[0].fills[0].quantity = d("0.5");
    variants.push(value);
    let mut value = old.clone();
    value.events[0].fills[0].charged_fee_atoms += 1;
    variants.push(value);
    let mut value = old.clone();
    value.events[0].fills[0].retained_fee_atoms += 1;
    variants.push(value);
    let mut value = old.clone();
    value.events[0].fills[0].maker_before.order.txid = tx(99);
    variants.push(value);
    let mut value = old.clone();
    value.events[0].book_before.resting.clear();
    variants.push(value);
    let mut value = old.clone();
    value.events[0].cancelled.clear();
    variants.push(value);
    let mut value = old.clone();
    value.events[0].cancelled[0].reserve_atoms += 1;
    variants.push(value);
    let mut value = old.clone();
    value.events[0].top_up_atoms += 1;
    variants.push(value);
    let mut value = old.clone();
    value.events[0].status = LiquidationStatus::NoLiquidityWithinGuard;
    variants.push(value);
    let mut value = old.clone();
    value.events.clear();
    variants.push(value);
    let mut value = old.clone();
    value.book_frames[2].placements[0].position = d("1");
    variants.push(value);
    let mut value = old.clone();
    value.book_frames.remove(3);
    variants.push(value);
    let mut value = old.clone();
    let LiquidationBookInput::Order(order) = &mut value.book_frames[3].inputs[0] else {
        panic!()
    };
    order.price = d("1001");
    variants.push(value);
    for value in variants {
        assert!(read(&replace_liquidation(&bytes, old, &value), state).is_err());
    }
    let mut config = state.config().clone();
    config.liquidation.as_mut().unwrap().max_price_deviation_bps = 1;
    assert!(EconomicState::from_snapshot_bytes(
        &bytes,
        &config,
        &a("admin"),
        &BTreeSet::from([p()])
    )
    .is_err());
    let tail = liquidation_bytes(old);
    let start = bytes.len() - 4 - tail.len();
    let mut value = bytes[..bytes.len() - 4].to_vec();
    value[start..start + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(read(&seal(value), state).is_err());

    let event = &old.events[0];
    let mut forced = event.height.to_be_bytes().to_vec();
    forced.push(2);
    forced.extend_from_slice(&event.id.as_bytes());
    forced.extend_from_slice(&0u32.to_be_bytes());
    forced.extend_from_slice(&event.fills[0].maker_before.order.txid.as_bytes());
    account(&mut forced, &event.owner);
    let position = bytes
        .windows(forced.len())
        .position(|part| part == forced)
        .unwrap();
    let mut value = bytes[..bytes.len() - 4].to_vec();
    value[position + 41..position + 45].copy_from_slice(&1u32.to_be_bytes());
    assert!(read(&seal(value), state).is_err());
}

#[test]
fn v4_external_book_cancellation_criteria_restore_without_losing_the_input_journal() {
    for criteria in [
        CancelCriteria::All,
        CancelCriteria::Txid(tx(31)),
        CancelCriteria::Price {
            side: Side::Buy,
            price: d("80"),
        },
    ] {
        let mut chain = before_liquidation(TradeLayerMainnet::Bitcoin);
        apply(
            &mut chain,
            vec![EconomicInput::Cancel {
                txid: tx(40),
                owner: a("bob"),
                criteria: criteria.clone(),
            }],
        );
        let state = chain.state();
        assert!(
            matches!(&state.liquidation_state().book_frames.last().unwrap().inputs[0],
            LiquidationBookInput::Cancel{criteria:restored,..} if *restored == criteria)
        );
        let bytes = state.snapshot_bytes().unwrap();
        assert_eq!(read(&bytes, state).unwrap(), *state);
        assert_eq!(
            read(&bytes, state).unwrap().snapshot_bytes().unwrap(),
            bytes
        );
    }
}

#[test]
fn zero_fill_attempt_cannot_invent_top_up_or_disappear_with_its_epoch_marker() {
    let mut chain = before_liquidation(TradeLayerMainnet::Bitcoin);
    apply(
        &mut chain,
        vec![
            EconomicInput::Cancel {
                txid: tx(40),
                owner: a("carol"),
                criteria: CancelCriteria::Txid(tx(30)),
            },
            EconomicInput::Cancel {
                txid: tx(41),
                owner: a("bob"),
                criteria: CancelCriteria::All,
            },
            publication(13, 1000),
        ],
    );
    let state = chain.state();
    let old = state.liquidation_state();
    assert_eq!(old.events.len(), 1);
    assert!(old.events[0].fills.is_empty());
    assert_eq!(
        old.events[0].status,
        LiquidationStatus::NoLiquidityWithinGuard
    );
    assert_eq!(old.events[0].available_before_atoms, 0);
    assert_eq!(old.events[0].top_up_atoms, 0);
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(read(&bytes, state).unwrap(), *state);
    let mut forged = old.clone();
    forged.events[0].available_before_atoms = 1;
    forged.events[0].top_up_atoms = 1;
    assert!(read(&replace_liquidation(&bytes, old, &forged), state).is_err());

    // A missing no-op attempt must not become valid merely by also removing
    // its private system ID and repairing every affected length/checksum.
    assert!(old.events[0].cancelled.is_empty());
    let epoch = old.events[0].id;
    let mut ids: Vec<_> = state
        .mark_state()
        .events
        .iter()
        .filter_map(|event| {
            if let MarkEventKind::Mark { id, .. } = event.kind {
                Some(id)
            } else {
                None
            }
        })
        .collect();
    ids.push(epoch);
    ids.sort();
    let mut pattern = (ids.len() as u32).to_be_bytes().to_vec();
    for id in &ids {
        pattern.extend_from_slice(&id.as_bytes());
    }
    let joint_start = bytes
        .windows(5)
        .position(|part| part == b"TLJE\x02")
        .unwrap();
    let length_start = joint_start - 4;
    let length = u32::from_be_bytes(bytes[length_start..joint_start].try_into().unwrap()) as usize;
    let joint_end = joint_start + length;
    let mut joint = bytes[joint_start..joint_end - 4].to_vec();
    let category = joint
        .windows(pattern.len())
        .position(|part| part == pattern)
        .unwrap();
    let mut replacement = ((ids.len() - 1) as u32).to_be_bytes().to_vec();
    for id in ids.into_iter().filter(|id| *id != epoch) {
        replacement.extend_from_slice(&id.as_bytes());
    }
    joint.splice(category..category + pattern.len(), replacement);
    let joint = seal(joint);
    let mut out = bytes[..length_start].to_vec();
    out.extend_from_slice(&(joint.len() as u32).to_be_bytes());
    out.extend_from_slice(&joint);
    out.extend_from_slice(&bytes[joint_end..bytes.len() - 4]);
    let mut omitted = old.clone();
    omitted.events.clear();
    assert!(read(&replace_liquidation(&seal(out), old, &omitted), state).is_err());
}

#[test]
fn v4_durable_commit_reopen_rollback_and_replay_preserve_forced_history_on_three_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let directory = Directory::new();
        let chain = before_liquidation(network);
        let config = chain.config().clone();
        let before_tip = chain.tip();
        let mut durable =
            DurableEconomicChain::initialize(&directory.0, chain, before_tip).unwrap();
        let event = block(durable.chain().unwrap(), vec![publication(13, 1000)]);
        let head = durable.head().unwrap();
        let after = durable.apply_unverified_block(head, &event).unwrap();
        let expected = durable.chain().unwrap().clone();
        drop(durable);
        let (mut durable, _) = DurableEconomicChain::load(
            &directory.0,
            config.clone(),
            after,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        assert_eq!(durable.chain().unwrap(), &expected);
        let rollback = durable.rollback_to(after, before_tip).unwrap();
        drop(durable);
        let (mut durable, _) = DurableEconomicChain::load(
            &directory.0,
            config,
            rollback,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        durable.apply_unverified_block(rollback, &event).unwrap();
        assert_eq!(durable.chain().unwrap(), &expected);
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-liquidation-snapshot-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        assert!(
            self.0.is_absolute()
                && self.0.starts_with(std::env::temp_dir())
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("tl-liquidation-snapshot-")
        );
        let _ = fs::remove_dir_all(&self.0);
    }
}
