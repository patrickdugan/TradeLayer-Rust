use tradelayer_core::decimal::Decimal;
use tradelayer_core::iou::{Claim, IouKey, IouStore};
use tradelayer_core::ledger::AccountId;

fn decimal(value: &str) -> Decimal {
    value.parse().unwrap()
}

fn address(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

#[test]
fn same_address_accrual_crosses_negative_claim_with_source_persistence_rounding() {
    // Independently reproduced with the unmodified pinned iou.js and archived
    // BigNumber 9.3.1. Rounding the two half-shares sequentially gives zero;
    // adding both unrounded shares to -1 before persistence would give 1e-20.
    let key = IouKey::new(1, 2);
    let mut store = IouStore::new();
    store
        .save_claim_map(
            key,
            1,
            [
                ("negative", "-1"),
                ("zero", "0"),
                ("tiny", "0.000000000000000000001"),
            ]
            .into_iter()
            .map(|(name, amount)| Claim {
                address: address(name),
                amount: decimal(amount),
            })
            .collect(),
        )
        .unwrap();
    store
        .add_iou_claims(
            key,
            1,
            &address("negative"),
            &address("negative"),
            &decimal("1"),
            &decimal("1"),
            &decimal("1.000000000000000000005"),
        )
        .unwrap();
    assert_eq!(
        store.claim_map(key).unwrap().claim(&address("negative")),
        Some(&decimal("0"))
    );

    store.add_loss(key, &decimal("1"), 1).unwrap();
    let allocations = store.pay_outstanding_ious(key, &decimal("1"), 1).unwrap();
    assert_eq!(allocations.len(), 1);
    assert_eq!(allocations[0].address, address("tiny"));
    assert_eq!(allocations[0].amount, decimal("0"));

    // A positive subprecision remaining claim is stored as zero, rather than
    // deleted: the source tests remaining <= 0 before calling toString(10).
    let claims = store.claim_map(key).unwrap().claims();
    assert_eq!(claims.len(), 3);
    assert_eq!(claims[0].address, address("negative"));
    assert_eq!(claims[1].address, address("zero"));
    assert_eq!(claims[2].address, address("tiny"));
    assert!(claims.iter().all(|claim| claim.amount.is_zero()));
    assert_eq!(store.claim_count(), 3);
    assert_eq!(store.bucket(key).unwrap().block_losses, decimal("1"));
}
