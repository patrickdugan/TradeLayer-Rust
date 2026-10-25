use std::str::FromStr;
use tradelayer_core::decimal::{Decimal, DecimalError, MAX_DECIMAL_DIGITS};
use tradelayer_core::iou::{
    block_reduction_toward_zero, Claim, IouError, IouKey, IouStore, MAX_IOU_CLAIMS,
    MAX_IOU_RECORDS, MAX_JS_SAFE_BLOCK,
};
use tradelayer_core::ledger::AccountId;

fn d(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}

fn address(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

fn claim(name: &str, amount: &str) -> Claim {
    Claim {
        address: address(name),
        amount: d(amount),
    }
}

fn nonempty(value: String) -> String {
    if value.is_empty() {
        "-".to_string()
    } else {
        value
    }
}

fn bucket_snapshot(store: &IouStore) -> String {
    nonempty(
        store
            .buckets()
            .map(|bucket| {
                format!(
                    "{}:{}/{}/{}/{}/{}/{}/{}",
                    bucket.key.contract_id,
                    bucket.key.property_id,
                    bucket.last_block,
                    bucket.amount,
                    bucket.block_start_amount,
                    bucket.block_delta,
                    bucket.block_losses,
                    bucket.block_profits
                )
            })
            .collect::<Vec<_>>()
            .join(";"),
    )
}

fn claim_snapshot(store: &IouStore) -> String {
    nonempty(
        store
            .claim_maps()
            .map(|map| {
                format!(
                    "{}:{}/{}/{}",
                    map.key.contract_id,
                    map.key.property_id,
                    map.last_block,
                    nonempty(
                        map.claims()
                            .iter()
                            .map(|claim| format!("{}={}", claim.address.as_str(), claim.amount))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                )
            })
            .collect::<Vec<_>>()
            .join(";"),
    )
}

#[test]
fn pinned_source_stateful_vectors() {
    let mut store = IouStore::new();
    let mut scenario = "";
    let mut checked = 0;
    for line in include_str!("../fixtures/iou_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 11, "{line}");
        if scenario != columns[0] {
            scenario = columns[0];
            store = IouStore::new();
        }
        let key = IouKey::new(columns[3].parse().unwrap(), columns[4].parse().unwrap());
        let block = columns[5].parse().unwrap();
        let arg = columns[6];
        let result = match columns[2] {
            "loss" | "profit" | "delta" => {
                let amount = d(arg);
                let changed = match columns[2] {
                    "loss" => store.add_loss(key, &amount, block),
                    "profit" => store.add_profit(key, &amount, block),
                    _ => store.add_delta(key, &amount, block),
                }
                .unwrap();
                if changed.is_some() { "bucket" } else { "none" }.to_string()
            }
            "apply" => {
                store.apply_to_losers(key, &d(arg), block).unwrap();
                "void".to_string()
            }
            "accrue" => {
                let parts: Vec<_> = arg.split(',').collect();
                let changed = store
                    .add_iou_claims(
                        key,
                        block,
                        &address(parts[0]),
                        &address(parts[1]),
                        &d(parts[2]),
                        &d(parts[3]),
                        &d(parts[4]),
                    )
                    .unwrap();
                if changed.is_some() { "claims" } else { "none" }.to_string()
            }
            "pay" => nonempty(
                store
                    .pay_outstanding_ious(key, &d(arg), block)
                    .unwrap()
                    .iter()
                    .map(|allocation| {
                        format!("{}={}", allocation.address.as_str(), allocation.amount)
                    })
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            "save" => {
                let claims = arg
                    .split(',')
                    .map(|entry| {
                        let (name, value) = entry.split_once('=').unwrap();
                        claim(name, value)
                    })
                    .collect();
                store.save_claim_map(key, block, claims).unwrap();
                "void".to_string()
            }
            "delete" => {
                store.delete(key);
                "void".to_string()
            }
            "delete_property" => {
                store.delete_for_property(key.property_id);
                "void".to_string()
            }
            "total" => store
                .get_total_for_property(key.property_id)
                .unwrap()
                .to_string(),
            "reduction" => block_reduction_toward_zero(store.bucket(key))
                .unwrap()
                .to_string(),
            op => panic!("unsupported fixture operation {op}"),
        };
        assert_eq!(result, columns[7], "result {scenario} step {}", columns[1]);
        assert_eq!(
            bucket_snapshot(&store),
            columns[8],
            "buckets {scenario} step {}",
            columns[1]
        );
        assert_eq!(
            claim_snapshot(&store),
            columns[9],
            "claims {scenario} step {}",
            columns[1]
        );
        assert_eq!(
            store
                .get_total_for_property(key.property_id)
                .unwrap()
                .to_string(),
            columns[10],
            "total {scenario} step {}",
            columns[1]
        );
        assert_eq!(
            store.record_count(),
            store.buckets().count() + store.claim_maps().count()
        );
        assert_eq!(
            store.claim_count(),
            store.claim_maps().map(|map| map.claims().len()).sum()
        );
        checked += 1;
    }
    assert_eq!(checked, 67);
}

#[test]
fn failures_do_not_partially_mutate_claims_or_buckets() {
    let key = IouKey::new(1, 2);
    let mut store = IouStore::new();
    store.add_loss(key, &d("10"), 1).unwrap();
    let before = store.clone();
    assert_eq!(
        store.add_loss(key, &d("1"), MAX_JS_SAFE_BLOCK + 1),
        Err(IouError::InvalidBlock)
    );
    assert_eq!(store, before);
    assert_eq!(
        store.add_iou_claims(
            key,
            1,
            &address("alice"),
            &address("__proto__"),
            &d("1"),
            &d("1"),
            &d("2")
        ),
        Err(IouError::ReservedAddress)
    );
    assert_eq!(store, before);
    assert_eq!(
        store.save_claim_map(key, 1, vec![claim("alice", "1"), claim("alice", "2")]),
        Err(IouError::DuplicateClaimant)
    );
    assert_eq!(store, before);
}

#[test]
fn source_noops_do_not_create_records_or_validate_unused_blocks() {
    let key = IouKey::new(1, 2);
    assert_eq!(key.bucket_key(), "1:2");
    assert_eq!(key.claim_key(), "claims:1:2");
    let mut store = IouStore::new();
    assert_eq!(store.add_loss(key, &d("-1"), u64::MAX).unwrap(), None);
    assert_eq!(store.add_profit(key, &d("0"), u64::MAX).unwrap(), None);
    assert_eq!(store.add_delta(key, &d("0"), u64::MAX).unwrap(), None);
    assert!(store
        .pay_outstanding_ious(key, &d("0"), u64::MAX)
        .unwrap()
        .is_empty());
    assert_eq!(store.record_count(), 0);
    assert_eq!(store.get(key), d("0"));
    assert_eq!(block_reduction_toward_zero(None).unwrap(), d("0"));
}

#[test]
fn bounded_record_capacity_and_property_deletion_are_atomic() {
    let mut store = IouStore::new();
    for id in 0..MAX_IOU_RECORDS as u32 {
        store.add_loss(IouKey::new(id, 2), &d("1"), 1).unwrap();
    }
    let before = store.clone();
    let key = IouKey::new(u32::MAX, 2);
    assert_eq!(
        store.add_loss(key, &d("1"), 1),
        Err(IouError::TooManyRecords)
    );
    assert_eq!(store, before);
    assert_eq!(
        store.save_claim_map(key, 1, vec![]),
        Err(IouError::TooManyRecords)
    );
    assert_eq!(store, before);
    assert_eq!(store.delete_for_property(2), MAX_IOU_RECORDS);
    assert_eq!(store.record_count(), 0);
    store.add_loss(key, &d("1"), 1).unwrap();
}

#[test]
fn bounded_global_claim_capacity_and_replacement_are_atomic() {
    let key = IouKey::new(1, 2);
    let mut store = IouStore::new();
    let claims = (0..MAX_IOU_CLAIMS)
        .map(|id| claim(&format!("account-{id}"), "1"))
        .collect();
    store.save_claim_map(key, 1, claims).unwrap();
    let before = store.clone();
    assert_eq!(
        store.save_claim_map(IouKey::new(2, 2), 1, vec![claim("new", "1")]),
        Err(IouError::TooManyClaims)
    );
    assert_eq!(store, before);
    store
        .save_claim_map(key, 2, vec![claim("replacement", "1")])
        .unwrap();
    assert_eq!(store.claim_count(), 1);
    assert_eq!(store.delete_for_property(2), 1);
    assert_eq!(store.claim_count(), 0);
}

#[test]
fn no_implicit_token_funding_or_loss_pool_consumption() {
    let key = IouKey::new(1, 2);
    let mut store = IouStore::new();
    store.add_loss(key, &d("2"), 10).unwrap();
    store
        .save_claim_map(key, 1, vec![claim("alice", "10")])
        .unwrap();
    let bucket = store.bucket(key).unwrap().clone();
    for _ in 0..5 {
        let allocations = store.pay_outstanding_ious(key, &d("2"), 10).unwrap();
        assert_eq!(allocations.len(), 1);
        assert_eq!(allocations[0].amount, d("2"));
        assert_eq!(store.bucket(key).unwrap(), &bucket);
    }
    assert!(store.claim_map(key).unwrap().claims().is_empty());
    assert!(store
        .pay_outstanding_ious(key, &d("2"), 10)
        .unwrap()
        .is_empty());
}

#[test]
fn decimal_limits_fail_atomically_after_local_partial_calculations() {
    let key = IouKey::new(1, 2);
    let mut store = IouStore::new();
    let largest = d(&"9".repeat(MAX_DECIMAL_DIGITS));
    store.add_loss(key, &largest, MAX_JS_SAFE_BLOCK).unwrap();
    let before = store.clone();
    assert_eq!(
        store.add_loss(key, &d("1"), MAX_JS_SAFE_BLOCK),
        Err(IouError::Decimal(DecimalError::LimitExceeded))
    );
    assert_eq!(store, before);

    store
        .save_claim_map(
            key,
            1,
            vec![Claim {
                address: address("seller"),
                amount: largest,
            }],
        )
        .unwrap();
    let before = store.clone();
    // The buyer update succeeds locally before the seller update overflows.
    assert_eq!(
        store.add_iou_claims(
            key,
            1,
            &address("buyer"),
            &address("seller"),
            &d("1"),
            &d("1"),
            &d("2")
        ),
        Err(IouError::Decimal(DecimalError::LimitExceeded))
    );
    assert_eq!(store, before);

    let mut payout_store = IouStore::new();
    payout_store.add_loss(key, &d("2"), 1).unwrap();
    let huge = d(&format!("8{}", "0".repeat(MAX_DECIMAL_DIGITS - 1)));
    payout_store
        .save_claim_map(
            key,
            1,
            vec![
                claim("first", "1"),
                Claim {
                    address: address("second"),
                    amount: huge,
                },
            ],
        )
        .unwrap();
    let before = payout_store.clone();
    // A prior proposed allocation cannot survive a later multiplication error.
    assert_eq!(
        payout_store.pay_outstanding_ious(key, &d("2"), 1),
        Err(IouError::Decimal(DecimalError::LimitExceeded))
    );
    assert_eq!(payout_store, before);
}
