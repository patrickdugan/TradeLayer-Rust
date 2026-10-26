//! Genuine empty-block funding restart and semantic checkpoint corruption.
use std::collections::BTreeSet;
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::decimal::{Decimal, Rounding};
use tradelayer_core::economic::{
    EconomicConfig, EconomicError, EconomicInput, EconomicState, FundingConfig, FundingObservation,
    FundingPolicy, FundingState, FundingStatus, SettlementPolicy,
};
use tradelayer_core::funding::plan_funding;
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
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
fn config(network: TradeLayerMainnet) -> EconomicConfig {
    EconomicConfig {
        network,
        contract: 1.try_into().unwrap(),
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
            max_staleness_blocks: 96,
            policy: FundingPolicy::ConservingFundingV1,
        }),
    }
}
fn joint(n: u8, operation: JointOperation) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: tx(n),
        actor: a("admin"),
        operation,
    })
}
fn publication(n: u8, price: u128) -> EconomicInput {
    joint(
        n,
        JointOperation::Registry(RegistryAction::PublishOracle {
            oracle_id: 1,
            price_1e4: price * 10_000,
            high_1e4: None,
            low_1e4: None,
            close_1e4: None,
        }),
    )
}
fn pristine(network: TradeLayerMainnet) -> EconomicState {
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
    let joint = JointProtocolState::from_bootstrap_unverified(
        network,
        a("admin"),
        ledger,
        BTreeSet::from([p()]),
    )
    .unwrap();
    EconomicState::from_joint_unverified(config(network), joint).unwrap()
}
fn paid(network: TradeLayerMainnet) -> EconomicState {
    let mut state = pristine(network);
    state
        .apply_block_unverified(
            1,
            vec![joint(
                10,
                JointOperation::Registry(RegistryAction::Activate {
                    types: vec![2, 6, 13, 14, 15, 18],
                    code_hash: [0x51; 32],
                }),
            )],
        )
        .unwrap();
    state
        .apply_block_unverified(
            2,
            vec![
                joint(
                    11,
                    JointOperation::Registry(RegistryAction::CreateOracle {
                        ticker: "TEST".into(),
                        url: "https://example.invalid".into(),
                        backup: a("backup"),
                        lag: 1,
                    }),
                ),
                publication(12, 99),
            ],
        )
        .unwrap();
    state
        .apply_block_unverified(3, vec![publication(13, 100)])
        .unwrap();
    state
        .apply_block_unverified(
            4,
            vec![
                EconomicInput::Order(Order {
                    txid: tx(20),
                    owner: a("alice"),
                    contract: 1.try_into().unwrap(),
                    height: 4,
                    side: Side::Buy,
                    quantity: d("1.25"),
                    price: d("100"),
                    post: false,
                    is_market: false,
                }),
                EconomicInput::Order(Order {
                    txid: tx(21),
                    owner: a("bob"),
                    contract: 1.try_into().unwrap(),
                    height: 4,
                    side: Side::Sell,
                    quantity: d("1.25"),
                    price: d("100"),
                    post: false,
                    is_market: false,
                }),
            ],
        )
        .unwrap();
    for height in 5..=22 {
        state.apply_block_unverified(height, vec![]).unwrap();
    }
    state
        .apply_block_unverified(23, vec![publication(14, 101)])
        .unwrap();
    state.apply_block_unverified(24, vec![]).unwrap();
    assert_eq!(state.funding_state().epochs[0].status, FundingStatus::Paid);
    state
}
fn read(bytes: &[u8], config: &EconomicConfig) -> Result<EconomicState, EconomicError> {
    EconomicState::from_snapshot_bytes(bytes, config, &a("admin"), &BTreeSet::from([p()]))
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
fn raw(bytes: &[u8]) -> Vec<u8> {
    bytes[..bytes.len() - 4].to_vec()
}
fn u32_at(bytes: &[u8], offset: usize) -> usize {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize
}
fn observation_bytes(observation: &FundingObservation) -> Vec<u8> {
    let mut bytes = observation.txid.as_bytes().to_vec();
    bytes.extend_from_slice(&observation.height.to_be_bytes());
    let price = observation.price.to_string();
    bytes.push(price.len() as u8);
    bytes.extend_from_slice(price.as_bytes());
    bytes
}
fn funding_start(bytes: &[u8], state: &EconomicState) -> usize {
    let mut prefix = (state.funding_state().observations.len() as u32)
        .to_be_bytes()
        .to_vec();
    prefix.extend_from_slice(&observation_bytes(&state.funding_state().observations[0]));
    bytes
        .windows(prefix.len())
        .position(|w| w == prefix)
        .unwrap()
}
fn skip_observation(bytes: &[u8], offset: &mut usize) {
    *offset += 40;
    let len = bytes[*offset] as usize;
    *offset += 1 + len;
}
fn epoch_start(bytes: &[u8], state: &EconomicState) -> usize {
    let mut offset = funding_start(bytes, state);
    let count = u32_at(bytes, offset);
    offset += 4;
    for _ in 0..count {
        skip_observation(bytes, &mut offset);
    }
    let closed = bytes[offset];
    offset += 1;
    if closed == 1 {
        offset += 40;
    }
    assert_eq!(u32_at(bytes, offset), state.funding_state().epochs.len());
    offset + 4
}

fn put_decimal(bytes: &mut Vec<u8>, value: &Decimal) {
    let text = value.to_string();
    bytes.push(text.len() as u8);
    bytes.extend_from_slice(text.as_bytes());
}
// Deliberately rebuild only the public funding tail. Nested Joint/Spot/Ledger
// bytes and all of their CRCs remain unchanged; the outer CRC is recomputed.
fn replace_funding(bytes: &[u8], state: &EconomicState, funding: &FundingState) -> Vec<u8> {
    let mut out = bytes[..funding_start(bytes, state)].to_vec();
    out.extend_from_slice(&(funding.observations.len() as u32).to_be_bytes());
    for observation in &funding.observations {
        out.extend_from_slice(&observation_bytes(observation));
    }
    match &funding.closed_at {
        None => out.push(0),
        Some(closure) => {
            out.push(1);
            out.extend_from_slice(&closure.txid.as_bytes());
            out.extend_from_slice(&closure.height.to_be_bytes());
        }
    }
    out.extend_from_slice(&(funding.epochs.len() as u32).to_be_bytes());
    for epoch in &funding.epochs {
        out.extend_from_slice(&epoch.height.to_be_bytes());
        out.extend_from_slice(&epoch.id.as_bytes());
        out.push(match epoch.status {
            FundingStatus::Paid => 0,
            FundingStatus::ZeroRate => 1,
            FundingStatus::NoPositions => 2,
            FundingStatus::MissingPrice => 3,
            FundingStatus::StalePrice => 4,
            FundingStatus::ClosedOracle => 5,
        });
        match &epoch.index {
            None => out.push(0),
            Some(index) => {
                out.push(1);
                out.extend_from_slice(&observation_bytes(index));
            }
        }
        out.extend_from_slice(&(epoch.window.len() as u32).to_be_bytes());
        for observation in &epoch.window {
            out.extend_from_slice(&observation_bytes(observation));
        }
        out.extend_from_slice(&(epoch.positions.len() as u32).to_be_bytes());
        for position in &epoch.positions {
            out.push(position.owner.as_str().len() as u8);
            out.extend_from_slice(position.owner.as_str().as_bytes());
            put_decimal(&mut out, &position.contracts);
            out.extend_from_slice(&position.available_atoms.to_be_bytes());
        }
        match &epoch.plan {
            None => out.push(0),
            Some(plan) => {
                out.push(1);
                put_decimal(&mut out, &plan.rate_bps);
                put_decimal(&mut out, &plan.collateral_notional);
                out.extend_from_slice(&plan.total_atoms.to_be_bytes());
                for payments in [&plan.debits, &plan.credits] {
                    out.extend_from_slice(&(payments.len() as u32).to_be_bytes());
                    for payment in payments {
                        out.push(payment.owner.as_str().len() as u8);
                        out.extend_from_slice(payment.owner.as_str().as_bytes());
                        out.extend_from_slice(&payment.atoms.to_be_bytes());
                    }
                }
            }
        }
    }
    seal(out)
}

#[test]
fn funding_v2_restores_paid_epoch_and_later_oracle_closure_on_all_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let initial = pristine(network);
        let initial_bytes = initial.snapshot_bytes().unwrap();
        assert_eq!(&initial_bytes[..5], b"TLEC\x02");
        assert_eq!(read(&initial_bytes, initial.config()).unwrap(), initial);
        let state = paid(network);
        let bytes = state.snapshot_bytes().unwrap();
        let mut restored = read(&bytes, state.config()).unwrap();
        assert_eq!(restored, state);
        let plan = restored.funding_state().epochs[0].plan.as_ref().unwrap();
        assert_eq!(plan.rate_bps, d("11.875"));
        assert_eq!(plan.collateral_notional, d("101"));
        assert_eq!(plan.total_atoms, 14_992_188);
        assert!(restored.joint().snapshot_bytes().is_err());
        restored
            .apply_block_unverified(
                25,
                vec![joint(
                    90,
                    JointOperation::Registry(RegistryAction::CloseOracle { oracle_id: 1 }),
                )],
            )
            .unwrap();
        restored = read(&restored.snapshot_bytes().unwrap(), state.config()).unwrap();
        for height in 26..=48 {
            restored.apply_block_unverified(height, vec![]).unwrap();
        }
        assert_eq!(
            restored.funding_state().closed_at.as_ref().unwrap().height,
            25
        );
        assert_eq!(
            restored.funding_state().epochs[1].status,
            FundingStatus::ClosedOracle
        );
        assert!(restored.funding_state().epochs[1].plan.is_none());
        assert_eq!(
            read(&restored.snapshot_bytes().unwrap(), state.config()).unwrap(),
            restored
        );
    }
}

#[test]
fn enabled_funding_config_is_independently_expected_and_versions_cannot_be_relabelled() {
    let state = paid(TradeLayerMainnet::Bitcoin);
    let bytes = state.snapshot_bytes().unwrap();
    for variant in 0..3 {
        let mut config = state.config().clone();
        match variant {
            0 => config.funding = None,
            1 => config.funding.as_mut().unwrap().oracle_id = 2,
            _ => config.funding.as_mut().unwrap().max_staleness_blocks = 95,
        }
        assert!(read(&bytes, &config).is_err());
    }
    let mut changed = raw(&bytes);
    changed[4] = 1;
    assert!(read(&seal(changed), state.config()).is_err());
    let mut changed = raw(&bytes);
    changed[4] = 3;
    assert!(read(&seal(changed), state.config()).is_err());
    assert!(EconomicState::from_snapshot_bytes(
        &bytes,
        state.config(),
        &a("other-admin"),
        &BTreeSet::from([p()])
    )
    .is_err());
}

#[test]
fn rechecksummed_epoch_identity_status_price_position_and_plan_tampering_rejects() {
    let state = paid(TradeLayerMainnet::Bitcoin);
    let bytes = state.snapshot_bytes().unwrap();
    let epoch = epoch_start(&bytes, &state);
    let mut changed = raw(&bytes);
    changed[epoch + 8..epoch + 40].copy_from_slice(&tx(12).as_bytes());
    assert!(read(&seal(changed), state.config()).is_err());
    let mut changed = raw(&bytes);
    changed[epoch + 40] = 5; // A forged historical closed decision.
    assert!(read(&seal(changed), state.config()).is_err());
    let mut changed = raw(&bytes);
    changed[epoch + 74..epoch + 82].copy_from_slice(&1000u64.to_be_bytes());
    assert!(read(&seal(changed), state.config()).is_err());
    let mut cursor = epoch + 42;
    skip_observation(&bytes, &mut cursor);
    let window = u32_at(&bytes, cursor);
    cursor += 4;
    for _ in 0..window {
        skip_observation(&bytes, &mut cursor);
    }
    let mut changed = raw(&bytes);
    changed[cursor..cursor + 4].copy_from_slice(&4097u32.to_be_bytes());
    assert!(matches!(
        read(&seal(changed), state.config()),
        Err(EconomicError::Capacity)
    ));
    let positions = u32_at(&bytes, cursor);
    cursor += 4;
    for _ in 0..positions {
        let account_len = bytes[cursor] as usize;
        cursor += 1 + account_len;
        let contracts_len = bytes[cursor] as usize;
        cursor += 1 + contracts_len + 16;
    }
    assert_eq!(bytes[cursor], 1);
    cursor += 1;
    for _ in 0..2 {
        let len = bytes[cursor] as usize;
        cursor += 1 + len;
    }
    let mut changed = raw(&bytes);
    changed[cursor + 15] ^= 1;
    assert!(read(&seal(changed), state.config()).is_err());
    let observation = funding_start(&bytes, &state) + 4;
    let mut changed = raw(&bytes);
    changed[observation + 41] = b'0';
    assert!(read(&seal(changed), state.config()).is_err());
}

#[test]
fn funding_record_counts_are_bounded_before_untrusted_vector_expansion() {
    let state = paid(TradeLayerMainnet::Bitcoin);
    let bytes = state.snapshot_bytes().unwrap();
    let start = funding_start(&bytes, &state);
    let mut changed = raw(&bytes);
    changed[start..start + 4].copy_from_slice(&4097u32.to_be_bytes());
    assert!(matches!(
        read(&seal(changed), state.config()),
        Err(EconomicError::Capacity)
    ));
    // Every individual window is in bounds. The17th declared4096-record
    // window exceeds the common65536-record budget before its expansion.
    let mut changed = bytes[..start].to_vec();
    changed.extend_from_slice(&0u32.to_be_bytes());
    changed.push(0); // observations, closureNone
    changed.extend_from_slice(&17u32.to_be_bytes());
    let observation = observation_bytes(&state.funding_state().observations[0]);
    for _ in 0..17 {
        changed.extend_from_slice(&24u64.to_be_bytes());
        changed.extend_from_slice(&tx(40).as_bytes());
        changed.extend_from_slice(&[3, 0]); // MissingPrice, indexNone
        changed.extend_from_slice(&4096u32.to_be_bytes());
        for _ in 0..4096 {
            changed.extend_from_slice(&observation);
        }
        changed.extend_from_slice(&0u32.to_be_bytes());
        changed.push(0); // no positions or plan
    }
    assert!(matches!(
        read(&seal(changed), state.config()),
        Err(EconomicError::Capacity)
    ));
}

#[test]
fn oracle_closure_receipt_cannot_disappear_or_move_before_paid_epoch() {
    let mut state = paid(TradeLayerMainnet::Bitcoin);
    state
        .apply_block_unverified(
            25,
            vec![joint(
                90,
                JointOperation::Registry(RegistryAction::CloseOracle { oracle_id: 1 }),
            )],
        )
        .unwrap();
    let bytes = state.snapshot_bytes().unwrap();
    let mut cursor = funding_start(&bytes, &state);
    let count = u32_at(&bytes, cursor);
    cursor += 4;
    for _ in 0..count {
        skip_observation(&bytes, &mut cursor);
    }
    assert_eq!(bytes[cursor], 1);
    let mut changed = raw(&bytes);
    changed[cursor + 33..cursor + 41].copy_from_slice(&23u64.to_be_bytes());
    assert!(read(&seal(changed), state.config()).is_err());
    let mut changed = raw(&bytes);
    changed.splice(cursor..cursor + 41, [0]);
    assert!(read(&seal(changed), state.config()).is_err());
}

#[test]
fn known_early_samples_cannot_be_omitted_even_with_a_recomputed_zero_rate_plan() {
    let state = paid(TradeLayerMainnet::Bitcoin);
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(
        replace_funding(&bytes, &state, state.funding_state()),
        bytes
    );
    let mut funding = state.funding_state().clone();
    let epoch = &mut funding.epochs[0];
    assert_eq!(epoch.window.len(), 3);
    epoch.window.retain(|observation| observation.height == 23);
    let notional = epoch.plan.as_ref().unwrap().collateral_notional.clone();
    epoch.plan = Some(plan_funding(&epoch.positions, &notional, &Decimal::zero()).unwrap());
    epoch.status = FundingStatus::ZeroRate;
    assert_eq!(epoch.plan.as_ref().unwrap().total_atoms, 0);
    let outcome = read(&replace_funding(&bytes, &state, &funding), state.config());
    assert!(
        matches!(outcome, Err(EconomicError::Invariant)),
        "expected structural denial; observed error {:?}",
        outcome.as_ref().err()
    );
}

#[test]
fn at_tip_position_receipts_cannot_omit_or_resize_balanced_actual_positions() {
    let state = paid(TradeLayerMainnet::Bitcoin);
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(
        replace_funding(&bytes, &state, state.funding_state()),
        bytes
    );
    for omit in [true, false] {
        let mut funding = state.funding_state().clone();
        let epoch = &mut funding.epochs[0];
        let rate = epoch.plan.as_ref().unwrap().rate_bps.clone();
        let notional = epoch.plan.as_ref().unwrap().collateral_notional.clone();
        if omit {
            epoch.positions.clear();
            epoch.status = FundingStatus::NoPositions;
        } else {
            for position in &mut epoch.positions {
                position.contracts = position
                    .contracts
                    .checked_div(&d("2"), 20, Rounding::HalfUp)
                    .unwrap();
            }
            epoch.status = FundingStatus::Paid;
        }
        epoch.plan = Some(plan_funding(&epoch.positions, &notional, &rate).unwrap());
        assert!(
            epoch.plan.as_ref().unwrap().total_atoms
                < state.funding_state().epochs[0]
                    .plan
                    .as_ref()
                    .unwrap()
                    .total_atoms
        );
        let outcome = read(&replace_funding(&bytes, &state, &funding), state.config());
        assert!(
            matches!(outcome, Err(EconomicError::Invariant)),
            "expected structural denial; observed error {:?}",
            outcome.as_ref().err()
        );
    }
}

#[test]
fn empty_window_stale_epoch_must_retain_the_latest_known_out_of_window_index() {
    let mut state = paid(TradeLayerMainnet::Bitcoin);
    for height in 25..=216 {
        state.apply_block_unverified(height, vec![]).unwrap();
    }
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(
        replace_funding(&bytes, &state, state.funding_state()),
        bytes
    );
    let genuine = state.funding_state().epochs.last().unwrap();
    assert_eq!(genuine.height, 216);
    assert_eq!(genuine.status, FundingStatus::StalePrice);
    assert!(genuine.window.is_empty()); // floor24, latest quote is from23.
    assert_eq!(genuine.index.as_ref().unwrap().height, 23);
    let older = state.funding_state().epochs[0].window[0].clone();
    assert_eq!(older.height, 2); // Still known from the earlier complete epoch.
    for remove in [true, false] {
        let mut funding = state.funding_state().clone();
        let epoch = funding.epochs.last_mut().unwrap();
        epoch.index = if remove { None } else { Some(older.clone()) };
        epoch.status = if remove {
            FundingStatus::MissingPrice
        } else {
            FundingStatus::StalePrice
        };
        epoch.plan = None;
        let outcome = read(&replace_funding(&bytes, &state, &funding), state.config());
        assert!(
            matches!(outcome, Err(EconomicError::Invariant)),
            "expected latest-index denial; observed error {:?}",
            outcome.as_ref().err()
        );
    }
}
