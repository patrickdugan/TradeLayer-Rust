//! Funded AMM checkpoint lifecycle and repaired-CRC semantic forgeries.
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    EconomicConfig, EconomicInput, EconomicState, LiquidationConfig, LiquidationPolicy, MarkConfig,
    MarkEventKind, MarkPolicy, SettlementPolicy,
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
            native_entitlements: None,
            insurance: None,
            source_consensus: None,
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

use tradelayer_core::economic::{
    AmmCapitalInput, AmmCapitalKind, AmmCashLeg, FundedAmmConfig, FundedAmmPolicy, FundedAmmQuote,
    FundedAmmState,
};

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
fn quotes(out: &mut Vec<u8>, values: &[FundedAmmQuote]) {
    count(out, values.len());
    for q in values {
        out.extend_from_slice(&q.epoch_id.as_bytes());
        out.extend_from_slice(&q.pool_id.to_be_bytes());
        out.extend_from_slice(&q.generation.to_be_bytes());
        out.extend_from_slice(&q.height.to_be_bytes());
        out.extend_from_slice(&q.mark_id.as_bytes());
        out.push(match q.side {
            Side::Buy => 0,
            Side::Sell => 1,
        });
        decimal(out, &q.price);
        decimal(out, &q.remaining);
        out.extend_from_slice(&q.reserve_atoms.to_be_bytes());
    }
}
// Serialize public AMM metadata independently, leaving all nested Joint,
// position, book, IOU, fee and liquidation bytes/checksums unchanged.
fn amm_bytes(amm: &FundedAmmState) -> Vec<u8> {
    let mut out = amm.total_units.to_be_bytes().to_vec();
    count(&mut out, amm.shares.len());
    for (owner, units) in &amm.shares {
        account(&mut out, owner);
        out.extend_from_slice(&units.to_be_bytes());
    }
    out.extend_from_slice(&amm.cash_atoms.to_be_bytes());
    out.extend_from_slice(&amm.generation.to_be_bytes());
    quotes(&mut out, &amm.quotes);
    count(&mut out, amm.frames.len());
    for f in &amm.frames {
        out.extend_from_slice(&f.height.to_be_bytes());
        quotes(&mut out, &f.released_quotes);
        count(&mut out, f.capital_inputs.len());
        for input in &f.capital_inputs {
            let (tag, id, owner, amount, min) = match input {
                AmmCapitalInput::Deposit {
                    txid,
                    owner,
                    collateral_atoms,
                    min_share_units,
                } => (0, txid, owner, collateral_atoms, min_share_units),
                AmmCapitalInput::Redeem {
                    txid,
                    owner,
                    share_units,
                    min_collateral_atoms,
                } => (1, txid, owner, share_units, min_collateral_atoms),
            };
            out.push(tag);
            out.extend_from_slice(&id.as_bytes());
            account(&mut out, owner);
            out.extend_from_slice(&amount.to_be_bytes());
            out.extend_from_slice(&min.to_be_bytes());
        }
        count(&mut out, f.capital_receipts.len());
        for r in &f.capital_receipts {
            out.extend_from_slice(&r.txid.as_bytes());
            account(&mut out, &r.owner);
            out.push(match r.kind {
                AmmCapitalKind::Deposit => 0,
                AmmCapitalKind::Redeem => 1,
            });
            for n in [
                r.collateral_atoms,
                r.share_units,
                r.cash_before_atoms,
                r.total_units_before,
                r.owner_units_before,
            ] {
                out.extend_from_slice(&n.to_be_bytes());
            }
        }
        match &f.quote_epoch {
            None => out.push(0),
            Some(e) => {
                out.push(1);
                out.extend_from_slice(&e.id.as_bytes());
                out.extend_from_slice(&e.height.to_be_bytes());
                out.extend_from_slice(&e.mark_id.as_bytes());
                out.extend_from_slice(&e.generation.to_be_bytes());
                decimal(&mut out, &e.position_before);
                for n in [
                    e.cash_before_atoms,
                    e.margin_before_atoms,
                    e.unpaid_loss_before_atoms,
                ] {
                    out.extend_from_slice(&n.to_be_bytes());
                }
                out.extend_from_slice(&e.cash_leg_count_before.to_be_bytes());
                quotes(&mut out, &e.quotes);
            }
        }
        count(&mut out, f.cash_legs.len());
        for leg in &f.cash_legs {
            out.extend_from_slice(&leg.sequence.to_be_bytes());
            out.push(u8::from(leg.from_pool));
            account(&mut out, &leg.counterparty);
            out.extend_from_slice(&leg.atoms.to_be_bytes());
        }
    }
    out
}
fn replace_amm(bytes: &[u8], old: &FundedAmmState, new: &FundedAmmState) -> Vec<u8> {
    let tail = amm_bytes(old);
    let end = bytes.len() - 4;
    assert_eq!(&bytes[end - tail.len()..end], tail);
    let mut out = bytes[..end - tail.len()].to_vec();
    out.extend_from_slice(&amm_bytes(new));
    seal(out)
}
fn active(network: TradeLayerMainnet) -> EconomicChain {
    let mut chain = prepared(network, true);
    apply(&mut chain, vec![publication(13, 1000)]);
    chain
}

#[test]
fn disabled_amm_preserves_genuine_active_v4_bytes_with_forced_fill_and_full_frames() {
    let mut chain = prepared(TradeLayerMainnet::Bitcoin, false);
    apply(&mut chain, vec![publication(13, 1000)]);
    let bytes = chain.state().snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x04");
    assert_eq!(bytes.len(), 5681);
    // Captured from cfdd8ec's compiled owner, before AMM fields existed.
    // SHA256d:0b2f3cc9f8f4a52801a7a003fe43ea0f4763c01ff7cf0fb8e8b57f43e2d578b2.
    let fingerprint = bytes.iter().fold(14_695_981_039_346_656_037u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(1_099_511_628_211)
    });
    assert_eq!(fingerprint, 5_299_499_982_685_114_171);
    assert_eq!(read(&bytes, chain.state()).unwrap(), *chain.state());
}

#[test]
fn v5_amm_first_fill_and_pool_custody_restore_on_three_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let chain = active(network);
        let state = chain.state();
        let bytes = state.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..5], b"TLEC\x05");
        let restored = read(&bytes, state).unwrap();
        assert_eq!(restored, *state);
        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
        let event = &state.liquidation_state().events[0];
        assert_eq!(event.amm_fills.len(), 1);
        assert!(event.fills.is_empty());
        assert_eq!(event.amm_fills[0].price, d("1004"));
        assert_eq!(state.reserve_atoms(tx(30)), 10 * ATOMS);
        assert_eq!(state.positions().get(&a("bob")).unwrap().contracts, d("0"));
        let owner = state.amm_owner().unwrap();
        assert_eq!(state.positions().get(&owner).unwrap().contracts, d("-1"));
        assert_eq!(
            state.ledger().available_atoms(p(), &owner),
            state.amm_state().cash_atoms
        );
        assert_eq!(state.amm_state().total_units, 100 * ATOMS);
        assert_eq!(state.amm_state().quotes.len(), 1);
        assert!(matches!(
            state.mark_state().events.last().unwrap().kind,
            MarkEventKind::AmmForced { .. }
        ));
        let restored_chain = EconomicChain::from_checkpoint_bytes(
            &chain.checkpoint_bytes().unwrap(),
            chain.config().clone(),
            chain.tip(),
        )
        .unwrap();
        assert_eq!(restored_chain, chain);
        assert!(state.joint().snapshot_bytes().is_err());
    }
}

#[test]
fn v5_lp_receipt_deposit_redeem_and_new_owner_resume_through_zero_supply() {
    let mut chain = ready(TradeLayerMainnet::Bitcoin, true);
    apply(
        &mut chain,
        vec![EconomicInput::Amm(AmmCapitalInput::Redeem {
            txid: tx(16),
            owner: a("alice"),
            share_units: 40 * ATOMS,
            min_collateral_atoms: 40 * ATOMS,
        })],
    );
    apply(
        &mut chain,
        vec![EconomicInput::Amm(AmmCapitalInput::Deposit {
            txid: tx(17),
            owner: a("carol"),
            collateral_atoms: 30 * ATOMS,
            min_share_units: 30 * ATOMS,
        })],
    );
    let bytes = chain.state().snapshot_bytes().unwrap();
    assert_eq!(read(&bytes, chain.state()).unwrap(), *chain.state());
    chain = EconomicChain::from_checkpoint_bytes(
        &chain.checkpoint_bytes().unwrap(),
        chain.config().clone(),
        chain.tip(),
    )
    .unwrap();
    assert_eq!(chain.state().amm_state().shares[&a("alice")], 60 * ATOMS);
    assert_eq!(chain.state().amm_state().shares[&a("carol")], 30 * ATOMS);
    for (n, owner, units) in [(18, "alice", 60), (19, "carol", 30)] {
        apply(
            &mut chain,
            vec![EconomicInput::Amm(AmmCapitalInput::Redeem {
                txid: tx(n),
                owner: a(owner),
                share_units: units * ATOMS,
                min_collateral_atoms: units * ATOMS,
            })],
        );
    }
    assert_eq!(chain.state().amm_state().total_units, 0);
    assert_eq!(chain.state().amm_state().cash_atoms, 0);
    assert!(chain.state().amm_state().shares.is_empty());
    assert_eq!(
        read(&chain.state().snapshot_bytes().unwrap(), chain.state()).unwrap(),
        *chain.state()
    );
}

#[test]
fn repaired_crc_cannot_change_amm_lp_cash_quote_epoch_or_frame_provenance() {
    let chain = active(TradeLayerMainnet::Bitcoin);
    let state = chain.state();
    let bytes = state.snapshot_bytes().unwrap();
    let old = state.amm_state();
    let mut variants = Vec::new();
    let mut value = old.clone();
    value.total_units += 1;
    variants.push(value);
    let mut value = old.clone();
    *value.shares.get_mut(&a("alice")).unwrap() += 1;
    variants.push(value);
    let mut value = old.clone();
    value.cash_atoms += 1;
    variants.push(value);
    let mut value = old.clone();
    value.frames[1].capital_receipts[0].share_units += 1;
    variants.push(value);
    let mut value = old.clone();
    let AmmCapitalInput::Deposit {
        min_share_units, ..
    } = &mut value.frames[1].capital_inputs[0]
    else {
        panic!()
    };
    *min_share_units = 101 * ATOMS;
    variants.push(value);
    let mut value = old.clone();
    value.frames[1].cash_legs[0].atoms += 1;
    variants.push(value);
    let mut value = old.clone();
    value.frames[1].cash_legs[0].counterparty = a("bob");
    variants.push(value);
    let mut value = old.clone();
    value.quotes[0].reserve_atoms += 1;
    variants.push(value);
    let mut value = old.clone();
    value
        .frames
        .last_mut()
        .unwrap()
        .quote_epoch
        .as_mut()
        .unwrap()
        .cash_leg_count_before = u32::MAX;
    variants.push(value);
    let mut value = old.clone();
    value
        .frames
        .last_mut()
        .unwrap()
        .quote_epoch
        .as_mut()
        .unwrap()
        .quotes[0]
        .price = d("1");
    variants.push(value);
    let mut value = old.clone();
    value.frames.remove(2);
    variants.push(value);
    // Existing nonpool internal transfer IDs are not permission to invent
    // balanced pool cash traffic with an unchanged final cash/Ledger state.
    let cash_sequences: BTreeSet<_> = old
        .frames
        .iter()
        .flat_map(|frame| frame.cash_legs.iter().map(|leg| leg.sequence))
        .collect();
    let previous = old.frames[2].cash_legs.last().unwrap().sequence;
    let next = old.frames[3..]
        .iter()
        .flat_map(|frame| &frame.cash_legs)
        .next()
        .unwrap()
        .sequence;
    let nonpool: Vec<_> = ((previous + 1)..next)
        .filter(|sequence| !cash_sequences.contains(sequence))
        .take(2)
        .collect();
    assert_eq!(nonpool.len(), 2);
    let mut value = old.clone();
    value.frames[2].cash_legs.extend([
        AmmCashLeg {
            sequence: nonpool[0],
            from_pool: false,
            counterparty: a("alice"),
            atoms: 1,
        },
        AmmCashLeg {
            sequence: nonpool[1],
            from_pool: true,
            counterparty: a("bob"),
            atoms: 1,
        },
    ]);
    variants.push(value);
    for value in variants {
        assert!(read(&replace_amm(&bytes, old, &value), state).is_err());
    }
    let mut cfg = state.config().clone();
    cfg.amm.as_mut().unwrap().pool_id = 2;
    assert!(
        EconomicState::from_snapshot_bytes(&bytes, &cfg, &a("admin"), &BTreeSet::from([p()]))
            .is_err()
    );
    let tail = amm_bytes(old);
    let start = bytes.len() - 4 - tail.len();
    let mut raw = bytes[..bytes.len() - 4].to_vec();
    raw[start + 16..start + 20].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(read(&seal(raw), state).is_err());

    // Move ONLY known type10 activation to the deposit's own height. Repair
    // Registry, Joint and economic CRCs; all LP/cash/book records stay intact.
    let joint_start = bytes
        .windows(5)
        .position(|part| part == b"TLJE\x02")
        .unwrap();
    let joint_len =
        u32::from_be_bytes(bytes[joint_start - 4..joint_start].try_into().unwrap()) as usize;
    let joint_end = joint_start + joint_len;
    let mut joint = bytes[joint_start..joint_end - 4].to_vec();
    let registry_start = joint
        .windows(5)
        .position(|part| part == b"TLRG\x01")
        .unwrap();
    let registry_len = u32::from_be_bytes(
        joint[registry_start - 4..registry_start]
            .try_into()
            .unwrap(),
    ) as usize;
    let registry_end = registry_start + registry_len;
    let mut registry = joint[registry_start..registry_end - 4].to_vec();
    let admin_len = u16::from_be_bytes(registry[5..7].try_into().unwrap()) as usize;
    let mut cursor = 7 + admin_len;
    for _ in 0..10 {
        let present = registry[cursor];
        cursor += 1 + if present == 1 { 40 } else { 0 };
    }
    assert_eq!(registry[cursor], 1);
    registry[cursor + 1..cursor + 9].copy_from_slice(&2u64.to_be_bytes());
    joint[registry_start..registry_end].copy_from_slice(&seal(registry));
    let mut forged = bytes[..bytes.len() - 4].to_vec();
    forged[joint_start..joint_end].copy_from_slice(&seal(joint));
    assert!(read(&seal(forged), state).is_err());
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-amm-snapshot-{}-{}-{}",
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
                    .starts_with("tl-amm-snapshot-")
        );
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn v5_durable_restore_quote_release_rollback_and_replay_keep_one_funded_owner() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let directory = Directory::new();
        let chain = prepared(network, true);
        let config = chain.config().clone();
        let before = chain.tip();
        let mut durable = DurableEconomicChain::initialize(&directory.0, chain, before).unwrap();
        let mark = block(durable.chain().unwrap(), vec![publication(13, 1000)]);
        let head = durable.head().unwrap();
        let marked = durable.apply_unverified_block(head, &mark).unwrap();
        let expected = durable.chain().unwrap().clone();
        let empty = block(durable.chain().unwrap(), vec![]);
        let after = durable.apply_unverified_block(marked, &empty).unwrap();
        assert!(durable
            .chain()
            .unwrap()
            .state()
            .amm_state()
            .quotes
            .is_empty());
        drop(durable);
        let (mut durable, _) = DurableEconomicChain::load(
            &directory.0,
            config.clone(),
            after,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        let rollback = durable.rollback_to(after, marked.tip).unwrap();
        assert_eq!(durable.chain().unwrap(), &expected);
        drop(durable);
        let (mut durable, _) = DurableEconomicChain::load(
            &directory.0,
            config,
            rollback,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        let replay = durable.rollback_to(rollback, before).unwrap();
        durable.apply_unverified_block(replay, &mark).unwrap();
        assert_eq!(durable.chain().unwrap(), &expected);
    }
}
