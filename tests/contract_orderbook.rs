use tradelayer_core::contract_orderbook::*;
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::ledger::{AccountId, TxId};

fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}
fn a(text: &str) -> AccountId {
    AccountId::try_from(text).unwrap()
}
fn id(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn contract() -> ContractId {
    ContractId::try_from(1).unwrap()
}
fn o(n: u8, owner: &str, side: Side, quantity: &str, price: &str, height: u64) -> Order {
    Order {
        txid: id(n),
        owner: a(owner),
        contract: contract(),
        height,
        side,
        quantity: d(quantity),
        price: d(price),
        post: false,
        is_market: false,
    }
}
fn decode(text: &str) -> Vec<Order> {
    text.split(';')
        .map(|item| {
            let f: Vec<_> = item.split(',').collect();
            let mut order = o(
                f[0].parse().unwrap(),
                f[1],
                if f[3] == "B" { Side::Buy } else { Side::Sell },
                f[4],
                f[5],
                f[2].parse().unwrap(),
            );
            order.post = f[6] == "1";
            order.is_market = f[7] == "1";
            order
        })
        .collect()
}
fn imported(orders: Vec<Order>) -> ContractOrderbook {
    let mut seen: Vec<_> = orders.iter().map(|o| o.txid).collect();
    seen.sort();
    let next_sequence = orders.len() as u64;
    ContractOrderbook::from_snapshot(BookSnapshot {
        contract: contract(),
        queued: Vec::new(),
        drained: Vec::new(),
        seen,
        next_sequence,
        resting: orders
            .into_iter()
            .enumerate()
            .map(|(sequence, order)| RestingOrder {
                remaining: order.quantity.clone(),
                reserve_quantity: order.quantity.clone(),
                order,
                initial_reduce: false,
                sequence: sequence as u64,
            })
            .collect(),
        processed_through: None,
    })
    .unwrap()
}
fn shape(book: &ContractOrderbook, result: &BookTransition) -> String {
    let matches = result
        .matches
        .iter()
        .map(|m| {
            format!(
                "{},{},{},{},{},{},{}",
                m.buy_txid.as_bytes()[0],
                m.sell_txid.as_bytes()[0],
                m.quantity,
                m.price,
                u8::from(m.buy_maker),
                u8::from(m.sell_maker),
                m.source_txid.as_bytes()[0]
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    let side = |s| {
        book.side(s)
            .iter()
            .map(|o| format!("{},{}", o.order.txid.as_bytes()[0], o.remaining))
            .collect::<Vec<_>>()
            .join(";")
    };
    format!("{matches}|{}|{}", side(Side::Buy), side(Side::Sell))
}

#[test]
fn all_pinned_source_matching_reserve_queue_and_comparator_vectors() {
    let mut count = 0;
    for line in include_str!("../fixtures/orderbook_vectors.tsv")
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let f: Vec<_> = line.split('\t').collect();
        assert_eq!(f.len(), 5);
        let (kind, name, input, expected) = (f[0], f[1], f[2], f[3]);
        match kind {
            "match" => {
                let mut book = imported(decode(input));
                let result = book.match_resting().unwrap();
                assert_eq!(shape(&book, &result), expected, "{name}");
                assert_eq!(
                    ContractOrderbook::from_snapshot(book.snapshot()).unwrap(),
                    book,
                    "{name} snapshot"
                );
            }
            "reserve" => {
                let values: Vec<_> = input.split(',').collect();
                let order = o(
                    1,
                    "alice",
                    if values[1] == "B" {
                        Side::Buy
                    } else {
                        Side::Sell
                    },
                    values[2],
                    "100",
                    10,
                );
                let result = reserve_quantity(&order, &d(values[0])).unwrap();
                assert_eq!(
                    format!("{},{}", result.quantity, u8::from(result.initial_reduce)),
                    expected,
                    "{name}"
                );
            }
            "compare" => {
                let values: Vec<_> = input.split(',').collect();
                let order = compare_sender_addresses(
                    &a(values[0]),
                    &a(values[1]),
                    id(values[2].parse().unwrap()),
                    id(values[3].parse().unwrap()),
                );
                let actual = match order {
                    core::cmp::Ordering::Less => -1,
                    core::cmp::Ordering::Equal => 0,
                    core::cmp::Ordering::Greater => 1,
                };
                assert_eq!(actual.to_string(), expected, "{name}");
            }
            "queue" => {
                let mut book = ContractOrderbook::new(contract());
                for order in decode(input) {
                    book.queue(order).unwrap();
                }
                book.process_through(10, |_| Ok(d("0"))).unwrap();
                let actual = book
                    .resting()
                    .iter()
                    .map(|o| o.order.txid.as_bytes()[0].to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                assert_eq!(actual, expected, "{name}");
            }
            other => panic!("unhandled oracle kind {other}"),
        }
        count += 1;
    }
    assert_eq!(count, 287);
}

#[test]
fn coordinator_can_fund_and_settle_between_drained_admissions() {
    let mut book = ContractOrderbook::new(contract());
    book.queue(o(1, "alice", Side::Buy, "1.25", "100", 10))
        .unwrap();
    book.queue(o(2, "seller", Side::Sell, "0.5", "99", 10))
        .unwrap();
    book.queue(o(3, "future", Side::Sell, "1", "101", 11))
        .unwrap();
    let ready = book.drain_queued_through(10).unwrap();
    assert_eq!(ready.len(), 2);
    assert!(book.finish_height(10).is_err());
    let restored = ContractOrderbook::from_snapshot(book.snapshot()).unwrap();
    assert_eq!(book, restored);
    let before = book.clone();
    assert_eq!(
        book.place(ready[1].clone(), &d("0")),
        Err(BookError::DuplicateInput)
    );
    assert_eq!(book, before);
    let first = book.place(ready[0].clone(), &d("0")).unwrap();
    assert_eq!(first.admitted[0].quantity, d("1.25"));
    let second = book.place(ready[1].clone(), &d("0")).unwrap();
    assert_eq!(second.matches.len(), 1);
    assert_eq!(second.matches[0].quantity, d("0.5"));
    assert_eq!(second.matches[0].buy_remaining, d("0.75"));
    assert_eq!(second.matches[0].buy_reserve_used, d("0.5"));
    book.finish_height(10).unwrap();
    assert_eq!(book.queued().len(), 1);
    assert_eq!(book.processed_through(), Some(10));
    let before = book.clone();
    assert_eq!(
        book.place(ready[0].clone(), &d("0")),
        Err(BookError::InvalidHeight)
    );
    assert_eq!(book, before);
}

#[test]
fn reducing_leg_consumes_before_flip_reserve_and_cancellation_releases_only_remainder() {
    let mut book = ContractOrderbook::new(contract());
    let long = o(1, "alice", Side::Buy, "4.25", "100", 10);
    let result = book.place(long, &d("-3")).unwrap();
    assert_eq!(result.admitted[0].quantity, d("1.25"));
    assert!(result.admitted[0].initial_reduce);
    let result = book
        .place(o(2, "seller", Side::Sell, "2", "99", 11), &d("0"))
        .unwrap();
    assert_eq!(result.matches[0].buy_reserve_used, d("0"));
    assert_eq!(book.reserve_for_owner(&a("alice")).unwrap(), d("1.25"));
    let result = book
        .place(o(3, "seller", Side::Sell, "1.5", "99", 12), &d("-2"))
        .unwrap();
    assert_eq!(result.matches[0].buy_reserve_used, d("0.5"));
    let removed = book.cancel(&a("alice"), CancelCriteria::All).unwrap();
    assert_eq!(removed.reserve_quantity, d("0.75"));
    assert_eq!(removed.resting[0].remaining, d("0.75"));
    assert!(book.resting().is_empty());
}

#[test]
fn self_cross_before_post_rule_returns_removed_owned_reserve() {
    let mut book = ContractOrderbook::new(contract());
    let mut buy = o(1, "same", Side::Buy, "1", "100", 10);
    buy.post = true;
    book.place(buy, &d("0")).unwrap();
    let result = book
        .place(o(2, "same", Side::Sell, "2", "99", 10), &d("0"))
        .unwrap();
    assert!(result.matches.is_empty());
    assert_eq!(result.self_cross_removed.len(), 1);
    assert_eq!(result.self_cross_removed[0].order.txid, id(1));
    assert_eq!(result.self_cross_removed[0].reserve_quantity, d("1"));
    assert_eq!(book.resting()[0].order.txid, id(2));
}

#[test]
fn repaired_cancellation_enforces_owner_side_and_replay_after_refund() {
    let mut book = ContractOrderbook::new(contract());
    book.place(o(1, "alice", Side::Buy, "1", "98", 10), &d("0"))
        .unwrap();
    book.place(o(2, "other", Side::Buy, "2", "97", 10), &d("0"))
        .unwrap();
    book.place(o(3, "alice", Side::Sell, "3", "103", 10), &d("0"))
        .unwrap();
    assert!(book
        .cancel(&a("alice"), CancelCriteria::Txid(id(2)))
        .unwrap()
        .resting
        .is_empty());
    let removed = book
        .cancel(
            &a("alice"),
            CancelCriteria::Price {
                side: Side::Buy,
                price: d("99"),
            },
        )
        .unwrap();
    assert_eq!(removed.resting.len(), 1);
    assert_eq!(removed.reserve_quantity, d("1"));
    assert_eq!(book.orders_for_owner(&a("alice")).count(), 1);
    assert_eq!(
        book.place(o(1, "alice", Side::Buy, "1", "98", 10), &d("0")),
        Err(BookError::DuplicateInput)
    );
    let removed = book
        .cancel(&a("alice"), CancelCriteria::Txid(id(3)))
        .unwrap();
    assert_eq!(removed.resting[0].order.side, Side::Sell);
    assert_eq!(book.resting()[0].order.owner, a("other"));
}

#[test]
fn failed_callback_or_bad_input_does_not_consume_queue_or_replay_id() {
    let mut book = ContractOrderbook::new(contract());
    book.queue(o(1, "alice", Side::Buy, "1", "100", 10))
        .unwrap();
    book.queue(o(2, "seller", Side::Sell, "1", "99", 10))
        .unwrap();
    let before = book.clone();
    assert_eq!(
        book.process_through(10, |owner| if owner == &a("seller") {
            Err(BookError::Arithmetic)
        } else {
            Ok(d("0"))
        }),
        Err(BookError::Arithmetic)
    );
    assert_eq!(book, before);
    for mut bad in [
        o(3, "alice", Side::Buy, "0", "100", 10),
        o(3, "alice", Side::Buy, "1", "-1", 10),
        o(3, "hasdashx", Side::Buy, "1", "100", MAX_ORDER_HEIGHT + 1),
    ] {
        assert!(book.queue(bad.clone()).is_err());
        assert_eq!(book, before);
        bad.owner = a("has-dash");
        assert!(book.queue(bad).is_err());
        assert_eq!(book, before);
    }
    let ready = book.drain_queued_through(10).unwrap();
    let before = book.clone();
    let mut tampered = ready[0].clone();
    tampered.quantity = d("2");
    assert_eq!(
        book.place(tampered, &d("0")),
        Err(BookError::DuplicateInput)
    );
    assert_eq!(book, before);
}

#[test]
fn snapshot_rejects_structural_tampering_and_preserves_exhausted_replay_history() {
    let mut book = ContractOrderbook::new(contract());
    book.place(o(1, "alice", Side::Buy, "1", "100", 10), &d("0"))
        .unwrap();
    let snapshot = book.snapshot();
    for change in 0..6 {
        let mut bad = snapshot.clone();
        match change {
            0 => bad.seen.clear(),
            1 => bad.seen.push(id(1)),
            2 => bad.resting[0].remaining = d("2"),
            3 => bad.resting[0].reserve_quantity = d("2"),
            4 => bad.next_sequence = 0,
            _ => bad.resting[0].order.contract = ContractId::try_from(2).unwrap(),
        }
        assert!(ContractOrderbook::from_snapshot(bad).is_err());
    }
    book.cancel(&a("alice"), CancelCriteria::All).unwrap();
    let restored = ContractOrderbook::from_snapshot(book.snapshot()).unwrap();
    assert!(restored.resting().is_empty());
    assert!(restored.has_seen(id(1)));
}

#[test]
fn capacity_is_atomic_and_cancellation_frees_live_capacity_only() {
    let mut book = ContractOrderbook::new(contract());
    for n in 0..MAX_CONTRACT_ORDERS {
        let mut order = o(1, "alice", Side::Buy, "0.125", "100", 10);
        let mut bytes = [0; 32];
        bytes[..8].copy_from_slice(&(n as u64).to_le_bytes());
        order.txid = TxId::from_bytes(bytes);
        book.queue(order).unwrap();
    }
    let before = book.clone();
    assert_eq!(
        book.queue(o(255, "other", Side::Buy, "1", "100", 10)),
        Err(BookError::Capacity)
    );
    assert_eq!(book, before);
    let removed = book.cancel(&a("alice"), CancelCriteria::All).unwrap();
    assert_eq!(removed.queued.len(), MAX_CONTRACT_ORDERS);
    assert_eq!(removed.reserve_quantity, d("0"));
    assert_eq!(book.snapshot().seen.len(), MAX_CONTRACT_ORDERS);
    book.queue(o(255, "other", Side::Buy, "1", "100", 10))
        .unwrap();
}

#[test]
fn numeric_and_retained_id_limits_cannot_be_bypassed_by_snapshot_import() {
    let mut book = ContractOrderbook::new(contract());
    let original = book.clone();
    for quantity in [
        "0.000000000000000000001",
        "10000000000000000000000000000000000000000",
    ] {
        assert_eq!(
            book.queue(o(1, "alice", Side::Buy, quantity, "100", 10)),
            Err(BookError::InvalidNumber)
        );
        assert_eq!(book, original);
    }
    let mut snapshot = book.snapshot();
    snapshot.seen = (0..MAX_CONTRACT_ORDER_IDS)
        .map(|n| {
            let mut bytes = [0; 32];
            bytes[..8].copy_from_slice(&(n as u64).to_be_bytes());
            TxId::from_bytes(bytes)
        })
        .collect();
    let mut full = ContractOrderbook::from_snapshot(snapshot.clone()).unwrap();
    let original = full.clone();
    assert_eq!(
        full.queue(o(255, "alice", Side::Buy, "1", "100", 10)),
        Err(BookError::Capacity)
    );
    assert_eq!(full, original);
    snapshot.seen.push(id(255));
    assert!(ContractOrderbook::from_snapshot(snapshot).is_err());
}
