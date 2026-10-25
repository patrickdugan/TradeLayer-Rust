use tradelayer_core::decimal::Decimal;
use tradelayer_core::iou::{Claim, IouKey, IouStore};
use tradelayer_core::iou_ledger::{
    FundedIouError, FundedIouLedger, IouInput, MatchProfit, PayoutAtomPolicy,
};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;

fn account(name: &str) -> AccountId {
    AccountId::try_from(name).unwrap()
}
fn amount(text: &str) -> Decimal {
    text.parse().unwrap()
}
fn property() -> PropertyId {
    PropertyId::try_from(1).unwrap()
}
fn key() -> IouKey {
    IouKey::new(42, 1)
}
fn id(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn input(n: u8, height: u64) -> IouInput {
    IouInput {
        network: TradeLayerMainnet::Bitcoin,
        txid: id(n),
        height,
    }
}
fn ledger() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(255),
            Operation::CreateProperty {
                id: property(),
                issuer: account("payer"),
                kind: PropertyKind::Fixed,
                initial_atoms: 1_000_000_000,
            },
        )
        .unwrap();
    ledger
}
fn store(claims: &[(&str, &str)]) -> IouStore {
    let mut store = IouStore::new();
    if !claims.is_empty() {
        store
            .save_claim_map(
                key(),
                0,
                claims
                    .iter()
                    .map(|&(name, value)| Claim {
                        address: account(name),
                        amount: amount(value),
                    })
                    .collect(),
            )
            .unwrap();
    }
    store
}
fn book(policy: PayoutAtomPolicy, claims: &[(&str, &str)]) -> FundedIouLedger {
    FundedIouLedger::from_ledger_unverified(
        TradeLayerMainnet::Bitcoin,
        policy,
        ledger(),
        store(claims),
    )
    .unwrap()
}

#[test]
fn full_match_profit_collect_and_redeem_cycle_conserves_the_sole_supply() {
    let mut state = book(PayoutAtomPolicy::Exact, &[]);
    state
        .accrue_match_profit_unverified(
            input(1, 10),
            MatchProfit {
                key: key(),
                buyer: account("buyer"),
                seller: account("seller"),
                buyer_pnl: amount("6"),
                seller_pnl: amount("-2"),
            },
        )
        .unwrap();
    assert_eq!(state.ious().get(key()), amount("-4"));
    assert_eq!(
        state
            .ious()
            .claim_map(key())
            .unwrap()
            .claim(&account("buyer")),
        Some(&amount("4"))
    );
    assert_eq!(
        state
            .ledger()
            .available_atoms(property(), &account("buyer")),
        0
    );
    state
        .collect_loss_unverified(input(2, 11), key(), account("payer"), 400_000_000)
        .unwrap();
    assert_eq!(
        state
            .ledger()
            .available_atoms(property(), &account("payer")),
        600_000_000
    );
    let receipt = state
        .pay_unverified(input(3, 11), key(), 400_000_000)
        .unwrap();
    assert_eq!(receipt.credited_atoms, 400_000_000);
    assert_eq!(
        state
            .ledger()
            .available_atoms(property(), &account("buyer")),
        400_000_000
    );
    assert_eq!(
        state
            .ledger()
            .available_atoms(property(), &state.escrow_for_audit(key()).unwrap()),
        0
    );
    assert!(state.ious().claim_map(key()).unwrap().claims().is_empty());
    assert_eq!(
        state.ious().bucket(key()).unwrap().block_losses,
        amount("4")
    );
    assert_eq!(
        state
            .ledger()
            .property(property())
            .unwrap()
            .total_supply_atoms,
        1_000_000_000
    );
    state.verify().unwrap();
}

#[test]
fn source_pool_reuse_cannot_pay_again_after_real_funding_is_spent() {
    let mut state = book(PayoutAtomPolicy::Exact, &[("buyer", "2")]);
    state
        .collect_loss_unverified(input(1, 1), key(), account("payer"), 100_000_000)
        .unwrap();
    state
        .pay_unverified(input(2, 1), key(), 100_000_000)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        state.pay_unverified(input(3, 1), key(), 100_000_000),
        Err(FundedIouError::InsufficientFunding {
            available: 0,
            requested: 100_000_000
        })
    );
    assert_eq!(state, before);
    // A genuinely new deposit can fund the remaining source claim.
    state
        .collect_loss_unverified(input(3, 1), key(), account("payer"), 100_000_000)
        .unwrap();
    state
        .pay_unverified(input(4, 1), key(), 100_000_000)
        .unwrap();
    assert_eq!(
        state
            .ledger()
            .available_atoms(property(), &account("buyer")),
        200_000_000
    );
}

#[test]
fn partial_budgets_and_source_proportions_pay_exact_atom_allocations() {
    let mut state = book(PayoutAtomPolicy::Exact, &[("alice", "3"), ("bob", "1")]);
    state
        .collect_loss_unverified(input(1, 1), key(), account("payer"), 200_000_000)
        .unwrap();
    let receipt = state
        .pay_unverified(input(2, 1), key(), 100_000_000)
        .unwrap();
    assert_eq!(
        receipt
            .allocations
            .iter()
            .map(|a| a.credited_atoms)
            .collect::<Vec<_>>(),
        vec![75_000_000, 25_000_000]
    );
    assert!(receipt
        .allocations
        .iter()
        .all(|a| a.atom_rounding_delta.is_zero()));
    assert_eq!(
        state
            .ious()
            .claim_map(key())
            .unwrap()
            .claim(&account("alice")),
        Some(&amount("2.25"))
    );
}

#[test]
fn exact_atom_policy_rejects_fractional_shares_and_restores_claims_and_balances() {
    let mut state = book(
        PayoutAtomPolicy::Exact,
        &[("alice", "1"), ("bob", "1"), ("carol", "1")],
    );
    state
        .collect_loss_unverified(input(1, 1), key(), account("payer"), 100_000_000)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        state.pay_unverified(input(2, 1), key(), 100_000_000),
        Err(FundedIouError::FractionalAtom)
    );
    assert_eq!(state, before);
}

#[test]
fn down_policy_keeps_real_dust_in_escrow_and_reports_decimal_differences() {
    let mut state = book(
        PayoutAtomPolicy::Down,
        &[("alice", "1"), ("bob", "1"), ("carol", "1")],
    );
    state
        .collect_loss_unverified(input(1, 1), key(), account("payer"), 100_000_000)
        .unwrap();
    let receipt = state
        .pay_unverified(input(2, 1), key(), 100_000_000)
        .unwrap();
    assert_eq!(receipt.credited_atoms, 99_999_999);
    assert_eq!(
        receipt.allocations[0].source_amount,
        amount("0.33333333333333333333")
    );
    assert_eq!(
        receipt.allocations[0].atom_rounding_delta,
        amount("-0.00000000333333333333")
    );
    assert_eq!(
        state
            .ledger()
            .available_atoms(property(), &state.escrow_for_audit(key()).unwrap()),
        1
    );
    state.verify().unwrap();
}

#[test]
fn half_up_overallocation_is_rejected_even_when_the_escrow_has_extra_funds() {
    let mut state = book(PayoutAtomPolicy::HalfUp, &[("alice", "1"), ("bob", "1")]);
    state
        .collect_loss_unverified(input(1, 1), key(), account("payer"), 10)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        state.pay_unverified(input(2, 1), key(), 1),
        Err(FundedIouError::RoundedPayoutExceedsBudget)
    );
    assert_eq!(state, before);
}

#[test]
fn half_up_cannot_expand_a_new_block_loss_pool_using_old_escrow_funds() {
    let mut state = book(PayoutAtomPolicy::HalfUp, &[("alice", "1"), ("bob", "1")]);
    state
        .collect_loss_unverified(input(1, 1), key(), account("payer"), 10)
        .unwrap();
    state
        .collect_loss_unverified(input(2, 2), key(), account("payer"), 1)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        state.pay_unverified(input(3, 2), key(), 10),
        Err(FundedIouError::RoundedPayoutExceedsSourcePool)
    );
    assert_eq!(state, before);
}

#[test]
fn source_block_gate_noop_is_replay_fenced_and_does_not_spend_older_pool() {
    let mut state = book(PayoutAtomPolicy::Exact, &[("alice", "1")]);
    state
        .collect_loss_unverified(input(1, 1), key(), account("payer"), 100_000_000)
        .unwrap();
    let receipt = state
        .pay_unverified(input(2, 2), key(), 100_000_000)
        .unwrap();
    assert!(receipt.allocations.is_empty());
    let before = state.clone();
    assert_eq!(
        state.pay_unverified(input(2, 2), key(), 100_000_000),
        Err(FundedIouError::Replay(id(2)))
    );
    assert_eq!(state, before);
    assert_eq!(
        state
            .ledger()
            .available_atoms(property(), &state.escrow_for_audit(key()).unwrap()),
        100_000_000
    );
}

#[test]
fn failed_debit_and_failed_claim_accrual_do_not_install_metadata_or_input_ids() {
    let mut state = book(PayoutAtomPolicy::Exact, &[]);
    let before = state.clone();
    assert!(matches!(
        state.collect_loss_unverified(input(1, 1), key(), account("payer"), 1_000_000_001),
        Err(FundedIouError::Ledger(_))
    ));
    assert_eq!(state, before);
    assert!(state
        .accrue_match_profit_unverified(
            input(1, 1),
            MatchProfit {
                key: key(),
                buyer: account("constructor"),
                seller: account("seller"),
                buyer_pnl: amount("1"),
                seller_pnl: amount("0")
            }
        )
        .is_err());
    assert_eq!(state, before);
    state
        .collect_loss_unverified(input(1, 1), key(), account("payer"), 1)
        .unwrap();
}

#[test]
fn late_invalid_claimant_restores_an_earlier_valid_allocation() {
    let mut state = book(
        PayoutAtomPolicy::Exact,
        &[("alice", "1"), ("~internal", "1")],
    );
    state
        .collect_loss_unverified(input(1, 1), key(), account("payer"), 200_000_000)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        state.pay_unverified(input(2, 1), key(), 200_000_000),
        Err(FundedIouError::ReservedAccount)
    );
    assert_eq!(state, before);
}

#[test]
fn input_order_network_key_and_reserved_account_guards_preserve_state() {
    let mut state = book(PayoutAtomPolicy::Exact, &[]);
    state
        .collect_loss_unverified(input(1, 10), key(), account("payer"), 1)
        .unwrap();
    let before = state.clone();
    let mut wrong = input(2, 10);
    wrong.network = TradeLayerMainnet::Litecoin;
    assert_eq!(
        state.collect_loss_unverified(wrong, key(), account("payer"), 1),
        Err(FundedIouError::WrongNetwork)
    );
    assert_eq!(
        state.collect_loss_unverified(input(2, 9), key(), account("payer"), 1),
        Err(FundedIouError::HeightRegression)
    );
    assert_eq!(
        state.collect_loss_unverified(input(2, 10), IouKey::new(0, 1), account("payer"), 1),
        Err(FundedIouError::InvalidContract)
    );
    assert_eq!(
        state.collect_loss_unverified(input(2, 10), key(), account("~iou:spoof"), 1),
        Err(FundedIouError::ReservedAccount)
    );
    assert_eq!(
        state.collect_loss_unverified(input(2, 10), key(), account("payer"), 0),
        Err(FundedIouError::ZeroAmount)
    );
    assert_eq!(
        state.collect_loss_unverified(input(255, 10), key(), account("payer"), 1),
        Err(FundedIouError::Replay(id(255)))
    );
    assert_eq!(state, before);
}

#[test]
fn escrow_and_internal_journal_identities_are_network_and_contract_separated() {
    let btc = book(PayoutAtomPolicy::Exact, &[]);
    let ltc = FundedIouLedger::from_ledger_unverified(
        TradeLayerMainnet::Litecoin,
        PayoutAtomPolicy::Exact,
        ledger(),
        IouStore::new(),
    )
    .unwrap();
    assert_ne!(
        btc.escrow_for_audit(key()).unwrap(),
        ltc.escrow_for_audit(key()).unwrap()
    );
    assert_ne!(
        btc.escrow_for_audit(key()).unwrap(),
        btc.escrow_for_audit(IouKey::new(43, 1)).unwrap()
    );
    let mut btc = btc;
    let mut ltc = ltc;
    btc.collect_loss_unverified(input(1, 1), key(), account("payer"), 1)
        .unwrap();
    let mut other = input(1, 1);
    other.network = TradeLayerMainnet::Litecoin;
    ltc.collect_loss_unverified(other, key(), account("payer"), 1)
        .unwrap();
    assert_ne!(
        btc.ledger().input_txids_unverified(),
        ltc.ledger().input_txids_unverified()
    );
}

#[test]
fn existing_internal_balance_cannot_be_adopted_without_ownership_metadata() {
    let mut ledger = ledger();
    ledger
        .apply(
            id(1),
            Operation::Transfer {
                id: property(),
                from: account("payer"),
                to: account("~iou:foreign"),
                amount_atoms: 1,
            },
        )
        .unwrap();
    assert_eq!(
        FundedIouLedger::from_ledger_unverified(
            TradeLayerMainnet::Bitcoin,
            PayoutAtomPolicy::Exact,
            ledger,
            IouStore::new()
        ),
        Err(FundedIouError::EscrowCollision)
    );
}
