//! Complete settled-basis checkpoints. Repaired CRCs below are not authority.
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    EconomicConfig, EconomicInput, EconomicState, FundingConfig, FundingObservation, FundingPolicy,
    FundingState, FundingStatus, MarkConfig, MarkEventKind, MarkPolicy, MarkState,
    SettlementPolicy,
};
use tradelayer_core::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::funding::plan_funding;
use tradelayer_core::joint::{JointOperation, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::mark_basis::{BasisAction, ExactRatio};
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
fn config(network: TradeLayerMainnet, inverse: bool, funding: bool) -> EconomicChainConfig {
    EconomicChainConfig {
        economics: EconomicConfig {
            network,
            contract: 1.try_into().unwrap(),
            collateral: p(),
            inverse,
            notional: d(if inverse { "100" } else { "1" }),
            initial_margin_per_contract: d("10"),
            fee_route: FeeRoute::Oracle(1),
            policy: SettlementPolicy::ConservingV1,
            funding: funding.then_some(FundingConfig {
                oracle_id: 1,
                max_staleness_blocks: 96,
                policy: FundingPolicy::ConservingFundingV1,
            }),
            marking: Some(MarkConfig {
                oracle_id: 1,
                policy: MarkPolicy::ConservingMarkBasisV1,
            }),
        },
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p()]),
        rollback_limit: 32,
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
                initial_atoms: 2000 * ATOMS,
            },
        )
        .unwrap();
    ledger
        .apply(
            tx(2),
            Operation::Transfer {
                id: p(),
                from: a("alice"),
                to: a("bob"),
                amount_atoms: 1000 * ATOMS,
            },
        )
        .unwrap();
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
fn pair(height: u64, first: u8, price: &str, closing: bool) -> Vec<EconomicInput> {
    [
        ("alice", if closing { Side::Sell } else { Side::Buy }),
        ("bob", if closing { Side::Buy } else { Side::Sell }),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (owner, side))| {
        EconomicInput::Order(Order {
            txid: tx(first + i as u8),
            owner: a(owner),
            contract: 1.try_into().unwrap(),
            height,
            side,
            quantity: d("1"),
            price: d(price),
            post: false,
            is_market: false,
        })
    })
    .collect()
}
fn block(chain: &EconomicChain, inputs: Vec<EconomicInput>) -> UnverifiedEconomicBlock {
    let height = chain.tip().height + 1;
    let mut id = [0; 32];
    id[24..].copy_from_slice(&height.to_be_bytes());
    UnverifiedEconomicBlock {
        network: chain.config().economics.network,
        header: BlockHeader {
            height,
            id: BlockId(id),
            parent: chain.tip().id,
        },
        inputs,
    }
}
fn apply(chain: &mut EconomicChain, inputs: Vec<EconomicInput>) {
    chain.apply_unverified_block(&block(chain, inputs)).unwrap();
}
fn ready(network: TradeLayerMainnet, inverse: bool, funding: bool) -> EconomicChain {
    let mut chain =
        EconomicChain::new_unverified(config(network, inverse, funding), ledger()).unwrap();
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
    chain
}
fn marked(network: TradeLayerMainnet, inverse: bool, funding: bool) -> EconomicChain {
    let mut chain = ready(network, inverse, funding);
    apply(&mut chain, pair(3, 20, "100", false));
    apply(&mut chain, vec![publication(13, 105)]);
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
fn decimal(out: &mut Vec<u8>, value: &Decimal) {
    let s = value.to_string();
    out.push(s.len() as u8);
    out.extend_from_slice(s.as_bytes());
}
fn account(out: &mut Vec<u8>, owner: &AccountId) {
    out.push(owner.as_str().len() as u8);
    out.extend_from_slice(owner.as_str().as_bytes());
}
fn observation(out: &mut Vec<u8>, value: &FundingObservation) {
    out.extend_from_slice(&value.txid.as_bytes());
    out.extend_from_slice(&value.height.to_be_bytes());
    decimal(out, &value.price);
}
fn ratio(out: &mut Vec<u8>, value: &ExactRatio) {
    for part in [value.numerator(), value.denominator()] {
        let s = part.to_string();
        out.extend_from_slice(&(s.len() as u16).to_be_bytes());
        out.extend_from_slice(s.as_bytes());
    }
}
// Independent serializer of the public mark-tail structures, to mutate semantic
// history without changing nested Joint/Ledger CRCs or economic custody bytes.
fn mark_bytes(marks: &MarkState) -> Vec<u8> {
    let mut out = Vec::new();
    count(&mut out, marks.bases.len());
    for (owner, basis) in &marks.bases {
        account(&mut out, owner);
        decimal(&mut out, &basis.quantity);
        ratio(&mut out, &basis.entry_basis);
        ratio(&mut out, &basis.carry);
    }
    count(&mut out, marks.unpaid_losses.len());
    for (owner, atoms) in &marks.unpaid_losses {
        account(&mut out, owner);
        out.extend_from_slice(&atoms.to_be_bytes());
    }
    count(&mut out, marks.events.len());
    for event in &marks.events {
        out.extend_from_slice(&event.height.to_be_bytes());
        match &event.kind {
            MarkEventKind::Trade {
                buy_txid,
                sell_txid,
            } => {
                out.push(0);
                out.extend_from_slice(&buy_txid.as_bytes());
                out.extend_from_slice(&sell_txid.as_bytes());
            }
            MarkEventKind::Mark {
                id,
                observation: value,
            } => {
                out.push(1);
                out.extend_from_slice(&id.as_bytes());
                observation(&mut out, value);
            }
        }
        count(&mut out, event.entries.len());
        for entry in &event.entries {
            account(&mut out, &entry.owner);
            match &entry.action {
                BasisAction::Trade { delta, price } => {
                    out.push(0);
                    decimal(&mut out, delta);
                    decimal(&mut out, price);
                }
                BasisAction::Mark { price } => {
                    out.push(1);
                    decimal(&mut out, price);
                }
            }
            out.push(u8::from(entry.atoms.negative));
            for n in [
                entry.atoms.atoms,
                entry.available_atoms,
                entry.margin_atoms,
                entry.collected_atoms,
                entry.claim_offset_atoms,
                entry.loss_offset_atoms,
            ] {
                out.extend_from_slice(&n.to_be_bytes());
            }
        }
        for n in [
            event.collected_loss_atoms,
            event.paid_claim_atoms,
            event.unfunded_loss_atoms,
        ] {
            out.extend_from_slice(&n.to_be_bytes());
        }
    }
    count(&mut out, marks.maintenance.len());
    for call in &marks.maintenance {
        account(&mut out, &call.owner);
        decimal(&mut out, &call.contracts);
        out.extend_from_slice(&call.required_atoms.to_be_bytes());
        out.extend_from_slice(&call.margin_atoms.to_be_bytes());
    }
    out
}
fn replace_marks(bytes: &[u8], old: &MarkState, new: &MarkState) -> Vec<u8> {
    let tail = mark_bytes(old);
    let end = bytes.len() - 4;
    assert_eq!(&bytes[end - tail.len()..end], tail);
    let mut out = bytes[..end - tail.len()].to_vec();
    out.extend_from_slice(&mark_bytes(new));
    seal(out)
}
fn funding_bytes(funding: &FundingState) -> Vec<u8> {
    let mut out = Vec::new();
    count(&mut out, funding.observations.len());
    for value in &funding.observations {
        observation(&mut out, value);
    }
    match &funding.closed_at {
        None => out.push(0),
        Some(c) => {
            out.push(1);
            out.extend_from_slice(&c.txid.as_bytes());
            out.extend_from_slice(&c.height.to_be_bytes());
        }
    }
    count(&mut out, funding.epochs.len());
    for e in &funding.epochs {
        out.extend_from_slice(&e.height.to_be_bytes());
        out.extend_from_slice(&e.id.as_bytes());
        out.push(match e.status {
            FundingStatus::Paid => 0,
            FundingStatus::ZeroRate => 1,
            FundingStatus::NoPositions => 2,
            FundingStatus::MissingPrice => 3,
            FundingStatus::StalePrice => 4,
            FundingStatus::ClosedOracle => 5,
        });
        match &e.index {
            None => out.push(0),
            Some(i) => {
                out.push(1);
                observation(&mut out, i);
            }
        }
        count(&mut out, e.window.len());
        for value in &e.window {
            observation(&mut out, value);
        }
        count(&mut out, e.positions.len());
        for position in &e.positions {
            account(&mut out, &position.owner);
            decimal(&mut out, &position.contracts);
            out.extend_from_slice(&position.available_atoms.to_be_bytes());
        }
        match &e.plan {
            None => out.push(0),
            Some(plan) => {
                out.push(1);
                decimal(&mut out, &plan.rate_bps);
                decimal(&mut out, &plan.collateral_notional);
                out.extend_from_slice(&plan.total_atoms.to_be_bytes());
                for payments in [&plan.debits, &plan.credits] {
                    count(&mut out, payments.len());
                    for payment in payments {
                        account(&mut out, &payment.owner);
                        out.extend_from_slice(&payment.atoms.to_be_bytes());
                    }
                }
            }
        }
    }
    out
}
fn replace_funding(bytes: &[u8], state: &EconomicState, new: &FundingState) -> Vec<u8> {
    let mut tail = funding_bytes(state.funding_state());
    tail.extend_from_slice(&mark_bytes(state.mark_state()));
    let end = bytes.len() - 4;
    assert_eq!(&bytes[end - tail.len()..end], tail);
    let mut out = bytes[..end - tail.len()].to_vec();
    out.extend_from_slice(&funding_bytes(new));
    out.extend_from_slice(&mark_bytes(state.mark_state()));
    seal(out)
}

#[test]
fn v3_linear_inverse_basis_and_carry_resume_without_double_paying_on_three_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        for inverse in [false, true] {
            let mut chain = marked(network, inverse, false);
            let bytes = chain.state().snapshot_bytes().unwrap();
            assert_eq!(&bytes[..5], b"TLEC\x03");
            let restored = read(&bytes, chain.state()).unwrap();
            assert_eq!(&restored, chain.state());
            assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
            if inverse {
                assert!(!restored.mark_state().bases[&a("alice")].carry.is_zero());
            }
            let checkpoint = chain.checkpoint_bytes().unwrap();
            let mut restored_chain = EconomicChain::from_checkpoint_bytes(
                &checkpoint,
                chain.config().clone(),
                chain.tip(),
            )
            .unwrap();
            let close = block(&chain, pair(5, 30, "110", true));
            chain.apply_unverified_block(&close).unwrap();
            restored_chain.apply_unverified_block(&close).unwrap();
            assert_eq!(restored_chain, chain);
            assert_eq!(
                chain
                    .state()
                    .positions()
                    .get(&a("alice"))
                    .unwrap()
                    .realized_pnl,
                d(if inverse { "0.09090909" } else { "10" })
            );
            assert!(chain
                .state()
                .positions()
                .get(&a("alice"))
                .unwrap()
                .contracts
                .is_zero());
            assert!(chain.state().joint().snapshot_bytes().is_err());
            let tip = chain.tip();
            chain.rollback_tip(tip).unwrap();
            assert_eq!(chain.checkpoint_bytes().unwrap(), checkpoint);
        }
    }
}

#[test]
fn repaired_crc_cannot_change_basis_carry_actions_offsets_or_maintenance() {
    let chain = marked(TradeLayerMainnet::Bitcoin, true, false);
    let state = chain.state();
    let bytes = state.snapshot_bytes().unwrap();
    assert!(read(&bytes, state).is_ok());
    let mut changes = Vec::new();
    let mut marks = state.mark_state().clone();
    marks.bases.get_mut(&a("alice")).unwrap().carry =
        ExactRatio::from_parts(d("1"), d("7")).unwrap();
    changes.push(marks);
    let mut marks = state.mark_state().clone();
    marks.events[1].entries[0].action = BasisAction::Trade {
        delta: d("2"),
        price: d("100"),
    };
    changes.push(marks);
    let mut marks = state.mark_state().clone();
    marks.events.last_mut().unwrap().entries[0].claim_offset_atoms = 1;
    changes.push(marks);
    let mut marks = state.mark_state().clone();
    marks.events.last_mut().unwrap().entries[0].loss_offset_atoms = 1;
    changes.push(marks);
    let mut marks = state.mark_state().clone();
    marks.unpaid_losses.insert(a("alice"), 1);
    changes.push(marks);
    let mut marks = state.mark_state().clone();
    marks.events.last_mut().unwrap().entries.pop();
    changes.push(marks);
    let mut marks = state.mark_state().clone();
    marks.events[1].entries[0].atoms.negative = true; // zero may have only one sign
    changes.push(marks);
    for marks in changes {
        assert!(read(&replace_marks(&bytes, state.mark_state(), &marks), state).is_err());
    }
    let mut wrong = state.config().clone();
    wrong.marking.as_mut().unwrap().oracle_id = 2;
    assert!(EconomicState::from_snapshot_bytes(
        &bytes,
        &wrong,
        &a("admin"),
        &BTreeSet::from([p()])
    )
    .is_err());
    // Finalized rational integer encoding rejects alternate fractions before replay.
    let tail = mark_bytes(state.mark_state());
    let start = bytes.len() - 4 - tail.len();
    let numerator = start + 4 + 1 + "alice".len() + 2; // count, owner, quantity "1"
    let mut forged = bytes[..bytes.len() - 4].to_vec();
    forged[numerator..numerator + 2].copy_from_slice(&514u16.to_be_bytes());
    assert!(read(&seal(forged), state).is_err());
    let mut forged = bytes[..bytes.len() - 4].to_vec();
    forged[start..start + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(read(&seal(forged), state).is_err());

    // A mathematically equivalent unreduced fraction is noncanonical too.
    let numerator_len =
        u16::from_be_bytes(bytes[numerator..numerator + 2].try_into().unwrap()) as usize;
    let denominator = numerator + 2 + numerator_len;
    let denominator_len =
        u16::from_be_bytes(bytes[denominator..denominator + 2].try_into().unwrap()) as usize;
    let basis = &state.mark_state().bases[&a("alice")].entry_basis;
    let mut replacement = Vec::new();
    for part in [basis.numerator(), basis.denominator()] {
        let text = part.checked_mul(&d("2")).unwrap().to_string();
        replacement.extend_from_slice(&(text.len() as u16).to_be_bytes());
        replacement.extend_from_slice(text.as_bytes());
    }
    let mut forged = bytes[..bytes.len() - 4].to_vec();
    forged.splice(numerator..denominator + 2 + denominator_len, replacement);
    assert!(read(&seal(forged), state).is_err());
}

#[test]
fn default_liability_and_maintenance_restore_then_reversal_extinguishes_nominal_claims() {
    let mut chain = ready(TradeLayerMainnet::Bitcoin, false, false);
    apply(&mut chain, pair(3, 20, "100", false));
    apply(&mut chain, vec![publication(13, 2100)]);
    let state = chain.state();
    assert!(!state.mark_state().unpaid_losses.is_empty());
    assert!(!state.mark_state().maintenance.is_empty());
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(read(&bytes, state).unwrap(), *state);
    let mut forged = state.mark_state().clone();
    forged.maintenance.clear();
    assert!(read(&replace_marks(&bytes, state.mark_state(), &forged), state).is_err());
    let mut restored = EconomicChain::from_checkpoint_bytes(
        &chain.checkpoint_bytes().unwrap(),
        chain.config().clone(),
        chain.tip(),
    )
    .unwrap();
    apply(&mut restored, vec![publication(14, 100)]);
    apply(&mut restored, pair(6, 30, "100", true));
    assert!(restored.state().mark_state().unpaid_losses.is_empty());
    assert!(restored
        .state()
        .ious()
        .claim_maps()
        .all(|map| map.claims().is_empty()));
    assert!(restored.state().unfunded_loss_atoms() > 0);
    assert_eq!(
        read(
            &restored.state().snapshot_bytes().unwrap(),
            restored.state()
        )
        .unwrap(),
        *restored.state()
    );
}

#[test]
fn historical_funding_positions_must_match_mark_journal_even_after_tip_moves_on() {
    let mut chain = marked(TradeLayerMainnet::Bitcoin, false, true);
    for _ in 5..=22 {
        apply(&mut chain, vec![]);
    }
    apply(&mut chain, vec![publication(14, 106)]);
    apply(&mut chain, vec![]);
    apply(&mut chain, vec![]);
    let state = chain.state();
    let bytes = state.snapshot_bytes().unwrap();
    let mut funding = state.funding_state().clone();
    let epoch = &mut funding.epochs[0];
    assert_eq!(epoch.height, 24);
    assert_eq!(state.finalized_height(), Some(25));
    for position in &mut epoch.positions {
        position.contracts = position.contracts.checked_mul(&d("0.5")).unwrap();
    }
    let old = epoch.plan.as_ref().unwrap();
    epoch.plan =
        Some(plan_funding(&epoch.positions, &old.collateral_notional, &old.rate_bps).unwrap());
    assert!(read(&replace_funding(&bytes, state, &funding), state).is_err());
}

#[test]
fn historical_empty_owner_mark_cannot_reuse_other_input_or_disagree_with_known_funding_quote() {
    let mut chain = ready(TradeLayerMainnet::Bitcoin, false, true);
    apply(&mut chain, vec![]);
    let state = chain.state();
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(state.finalized_height(), Some(3));
    for change_id in [false, true] {
        let mut marks = state.mark_state().clone();
        assert!(marks.events[0].entries.is_empty());
        let MarkEventKind::Mark { observation, .. } = &mut marks.events[0].kind else {
            panic!()
        };
        if change_id {
            observation.txid = tx(10);
        } else {
            observation.price = d("101");
        }
        assert!(read(&replace_marks(&bytes, state.mark_state(), &marks), state).is_err());
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-mark-snapshot-{}-{}-{}",
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
                    .starts_with("tl-mark-snapshot-")
        );
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn funded_mark_checkpoint_durable_rollback_and_replay_use_same_ledger_on_three_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let directory = Directory::new();
        let chain = marked(network, false, true);
        let config = chain.config().clone();
        let marked_tip = chain.tip();
        let mut durable =
            DurableEconomicChain::initialize(&directory.0, chain, marked_tip).unwrap();
        let close = block(durable.chain().unwrap(), pair(5, 30, "110", true));
        let before = durable.head().unwrap();
        let closed = durable.apply_unverified_block(before, &close).unwrap();
        let closed_chain = durable.chain().unwrap().clone();
        drop(durable);
        let (mut durable, _) = DurableEconomicChain::load(
            &directory.0,
            config.clone(),
            closed,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        assert_eq!(durable.chain().unwrap(), &closed_chain);
        let rollback = durable.rollback_to(closed, marked_tip).unwrap();
        drop(durable);
        let (mut durable, _) = DurableEconomicChain::load(
            &directory.0,
            config,
            rollback,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        durable.apply_unverified_block(rollback, &close).unwrap();
        assert_eq!(durable.chain().unwrap(), &closed_chain);
    }
}
