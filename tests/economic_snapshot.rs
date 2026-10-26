//! Full-state checkpoint tests use genuine funded contract execution. CRC
//! repairs deliberately reach semantic validation rather than checksum denial.
use std::collections::BTreeSet;
use tradelayer_core::contract_orderbook::{CancelCriteria, Order, Side};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    EconomicConfig, EconomicError, EconomicInput, EconomicState, SettlementPolicy,
};
use tradelayer_core::iou::IouKey;
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::tally::{FeeJournalEntry, FeeRoute};

const ATOMS: u128 = 100_000_000;
fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}
fn a(value: &str) -> AccountId {
    value.try_into().unwrap()
}
fn id(marker: u8) -> TxId {
    TxId::from_bytes([marker; 32])
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn c() -> ContractId {
    1.try_into().unwrap()
}
fn config(network: TradeLayerMainnet) -> EconomicConfig {
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
        fee_route: FeeRoute::Native(1),
        policy: SettlementPolicy::ConservingV1,
        funding: None,
        marking: None,
        liquidation: None,
        amm: None,
    }
}
fn pristine(network: TradeLayerMainnet) -> EconomicState {
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
    for (marker, owner) in [(2, "bob"), (3, "carol"), (4, "dave")] {
        ledger
            .apply(
                id(marker),
                Operation::Transfer {
                    id: p(),
                    from: a("alice"),
                    to: a(owner),
                    amount_atoms: 1000 * ATOMS,
                },
            )
            .unwrap();
    }
    let joint = JointProtocolState::from_bootstrap_unverified(
        network,
        a("admin"),
        ledger,
        BTreeSet::from([p()]),
    )
    .unwrap();
    EconomicState::from_joint_unverified(config(network), joint).unwrap()
}
fn activated(network: TradeLayerMainnet) -> EconomicState {
    let mut state = pristine(network);
    state
        .apply_block_unverified(
            1,
            vec![EconomicInput::Joint(UnverifiedJointTx {
                txid: id(10),
                actor: a("admin"),
                operation: JointOperation::Registry(RegistryAction::Activate {
                    types: vec![2, 6, 18],
                    code_hash: [0x71; 32],
                }),
            })],
        )
        .unwrap();
    state
}
fn order(
    marker: u8,
    owner: &str,
    side: Side,
    quantity: &str,
    price: &str,
    height: u64,
) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: id(marker),
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
fn with_claim(network: TradeLayerMainnet) -> EconomicState {
    let mut state = activated(network);
    state
        .apply_block_unverified(
            2,
            vec![
                order(20, "alice", Side::Buy, "1", "100", 2),
                order(21, "bob", Side::Sell, "1", "100", 2),
            ],
        )
        .unwrap();
    state
        .apply_block_unverified(
            3,
            vec![
                order(30, "carol", Side::Buy, "0.5", "110", 3),
                order(31, "alice", Side::Sell, "0.5", "110", 3),
            ],
        )
        .unwrap();
    assert_eq!(
        state
            .ious()
            .claim_map(IouKey::new(1, 7))
            .unwrap()
            .claim(&a("alice")),
        Some(&d("5"))
    );
    state
}
fn read(bytes: &[u8], expected: &EconomicConfig) -> Result<EconomicState, EconomicError> {
    EconomicState::from_snapshot_bytes(bytes, expected, &a("admin"), &BTreeSet::from([p()]))
}

#[test]
fn checkpoint_restores_real_collateral_positions_iou_fees_and_replay_on_all_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let state = with_claim(network);
        let bytes = state.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..5], b"TLEC\x01");
        let mut restored = read(&bytes, state.config()).unwrap();
        assert_eq!(restored, state);
        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
        assert!(!restored.fees().journal().is_empty());
        assert!(restored.joint().snapshot_bytes().is_err());
        assert!(restored.joint().spot().snapshot_bytes().is_err());
        let before = restored.clone();
        assert!(restored
            .apply_block_unverified(4, vec![order(20, "dave", Side::Buy, "1", "100", 4)])
            .is_err());
        assert_eq!(restored, before);
        let alice_before = restored.ledger().available_atoms(p(), &a("alice"));
        restored
            .apply_block_unverified(
                4,
                vec![
                    order(40, "bob", Side::Buy, "0.5", "110", 4),
                    order(41, "dave", Side::Sell, "0.5", "110", 4),
                ],
            )
            .unwrap();
        assert_eq!(restored.receipts()[2].collected_loss_atoms, 5 * ATOMS);
        assert_eq!(restored.receipts()[2].paid_claim_atoms, 5 * ATOMS);
        assert_eq!(
            restored.ledger().available_atoms(p(), &a("alice")),
            alice_before + 5 * ATOMS
        );
        assert_eq!(
            restored.positions().get(&a("alice")).unwrap().iou_claim,
            Decimal::zero()
        );
        assert_eq!(
            read(&restored.snapshot_bytes().unwrap(), state.config()).unwrap(),
            restored
        );
    }
}

#[test]
fn pristine_and_funded_order_then_cancellation_restart_without_losing_categories() {
    let pristine = pristine(TradeLayerMainnet::Bitcoin);
    assert_eq!(
        read(&pristine.snapshot_bytes().unwrap(), pristine.config()).unwrap(),
        pristine
    );
    let mut state = activated(TradeLayerMainnet::Bitcoin);
    state
        .apply_block_unverified(2, vec![order(20, "alice", Side::Buy, "2", "90", 2)])
        .unwrap();
    assert_eq!(state.reserve_atoms(id(20)), 20 * ATOMS);
    let mut restored = read(&state.snapshot_bytes().unwrap(), state.config()).unwrap();
    assert_eq!(restored, state);
    restored
        .apply_block_unverified(
            3,
            vec![EconomicInput::Cancel {
                txid: id(30),
                owner: a("alice"),
                criteria: CancelCriteria::All,
            }],
        )
        .unwrap();
    assert_eq!(restored.reserve_atoms(id(20)), 0);
    let mut restored = read(&restored.snapshot_bytes().unwrap(), state.config()).unwrap();
    let before = restored.clone();
    assert!(restored
        .apply_block_unverified(
            4,
            vec![EconomicInput::Cancel {
                txid: id(30),
                owner: a("alice"),
                criteria: CancelCriteria::All
            }]
        )
        .is_err());
    assert_eq!(restored, before);
}

#[test]
fn checkpoint_requires_independent_exact_config_administrator_and_spot_policy() {
    let state = with_claim(TradeLayerMainnet::Litecoin);
    let bytes = state.snapshot_bytes().unwrap();
    let mut variants = Vec::new();
    let mut changed = state.config().clone();
    changed.network = TradeLayerMainnet::Bitcoin;
    variants.push(changed);
    let mut changed = state.config().clone();
    changed.inverse = true;
    variants.push(changed);
    let mut changed = state.config().clone();
    changed.notional = d("2");
    variants.push(changed);
    let mut changed = state.config().clone();
    changed.initial_margin_per_contract = d("11");
    variants.push(changed);
    let mut changed = state.config().clone();
    changed.collateral = 8.try_into().unwrap();
    variants.push(changed);
    let mut changed = state.config().clone();
    changed.fee_route = FeeRoute::Oracle(1);
    variants.push(changed);
    let mut changed = state.config().clone();
    changed.contract = 2.try_into().unwrap();
    changed.fee_route = FeeRoute::Native(2);
    variants.push(changed);
    for expected in variants {
        assert!(read(&bytes, &expected).is_err());
    }
    assert!(EconomicState::from_snapshot_bytes(
        &bytes,
        state.config(),
        &a("other-admin"),
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
fn size(raw: &[u8], offset: usize) -> usize {
    u32::from_be_bytes(raw[offset..offset + 4].try_into().unwrap()) as usize
}
fn replace_embedded(raw: &mut Vec<u8>, offset: usize, replacement: &[u8]) {
    let previous = size(raw, offset);
    let mut field = (replacement.len() as u32).to_be_bytes().to_vec();
    field.extend_from_slice(replacement);
    raw.splice(offset..offset + 4 + previous, field);
}

#[test]
fn truncation_checksum_and_rechecksummed_version_count_decimal_sequence_fail_closed() {
    let state = activated(TradeLayerMainnet::Bitcoin);
    let bytes = state.snapshot_bytes().unwrap();
    for prefix in 0..bytes.len() {
        assert!(read(&bytes[..prefix], state.config()).is_err());
    }
    for offset in [0, 4, 5, 6, 47] {
        let mut damaged = bytes.clone();
        damaged[offset] ^= 0x80;
        assert!(read(&damaged, state.config()).is_err());
        damaged.truncate(damaged.len() - 4);
        assert!(read(&seal(damaged), state.config()).is_err());
    }
    let mut raw = bytes[..bytes.len() - 4].to_vec();
    let sequence = raw.len() - 8;
    raw[sequence..].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(read(&seal(raw), state.config()).is_err());
    let mut raw = bytes[..bytes.len() - 4].to_vec();
    raw.splice(47..49, [2, b'0', b'1']); // canonical decimal "1" -> prohibited "01"
    assert!(read(&seal(raw), state.config()).is_err());
    let joint = bytes.windows(5).position(|w| w == b"TLJE\x01").unwrap();
    let mut raw = bytes[..bytes.len() - 4].to_vec();
    raw[joint - 4..joint].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(read(&seal(raw), state.config()).is_err());
    let mut raw = bytes[..bytes.len() - 4].to_vec();
    raw.push(0);
    assert!(read(&seal(raw), state.config()).is_err());
}

#[test]
fn valid_nested_crcs_cannot_smuggle_an_unowned_economic_escrow() {
    let state = with_claim(TradeLayerMainnet::Bitcoin);
    let bytes = state.snapshot_bytes().unwrap();
    let joint_start = bytes.windows(5).position(|w| w == b"TLJE\x01").unwrap();
    let joint_size = size(&bytes, joint_start - 4);
    let mut joint_raw = bytes[joint_start..joint_start + joint_size - 4].to_vec();
    let spot_length_offset = 39 + 4 + size(&joint_raw, 39);
    let spot_start = spot_length_offset + 4;
    let spot_size = size(&joint_raw, spot_length_offset);
    let mut spot_raw = joint_raw[spot_start..spot_start + spot_size - 4].to_vec();
    let ledger_length_offset = 9 + 4 * size(&spot_raw, 5);
    let mut ledger = state.ledger().clone();
    ledger
        .apply(
            id(90),
            Operation::Transfer {
                id: p(),
                from: a("alice"),
                to: a("~econ:orphan"),
                amount_atoms: 1,
            },
        )
        .unwrap();
    ledger.verify_conservation().unwrap();
    replace_embedded(
        &mut spot_raw,
        ledger_length_offset,
        &ledger.snapshot_bytes().unwrap(),
    );
    replace_embedded(&mut joint_raw, spot_length_offset, &seal(spot_raw));
    let mut outer_raw = bytes[..bytes.len() - 4].to_vec();
    replace_embedded(&mut outer_raw, joint_start - 4, &seal(joint_raw));
    let malicious = seal(outer_raw);
    assert!(matches!(
        read(&malicious, state.config()),
        Err(EconomicError::CustodyMismatch)
    ));
}

#[test]
fn rechecksummed_well_formed_fee_journal_cannot_change_route_or_claim_future_events() {
    let state = with_claim(TradeLayerMainnet::Bitcoin);
    let bytes = state.snapshot_bytes().unwrap();
    let FeeJournalEntry::Accrued(plan) = state.fees().journal()[0] else {
        panic!("expected live accrual");
    };
    let mut record = vec![0];
    record.extend_from_slice(&plan.block.to_be_bytes());
    record.extend_from_slice(&plan.key.property.get().to_be_bytes());
    record.extend_from_slice(&plan.key.contract.to_be_bytes());
    record.push(1); // Native route, as in the independent config.
    record.extend_from_slice(&plan.route.contract().to_be_bytes());
    for atoms in [
        plan.amount_atoms,
        plan.collection_atoms,
        plan.value_added_atoms,
        plan.stash_added_atoms,
        plan.insurance_atoms,
        plan.custody_before_atoms,
        plan.custody_after_atoms,
    ] {
        record.extend_from_slice(&atoms.to_be_bytes());
    }
    let offset = bytes
        .windows(record.len())
        .position(|w| w == record)
        .unwrap();
    let mut raw = bytes[..bytes.len() - 4].to_vec();
    raw[offset + 17] = 2; // An individually valid Oracle(1) record.
    assert!(read(&seal(raw), state.config()).is_err());
    let mut raw = bytes[..bytes.len() - 4].to_vec();
    raw[offset + 1..offset + 9].copy_from_slice(&1000u64.to_be_bytes());
    assert!(read(&seal(raw), state.config()).is_err());
}
