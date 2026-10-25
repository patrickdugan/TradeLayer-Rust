use tradelayer_core::volume_index::{
    calculate_vwap, fee_linked_reward_atoms, VolumeError, VolumeHistory, VolumeKind, VolumePair,
    VolumeRecord, VwapObservation, WeightedPrice, MAX_IDENTIFIER_BYTES, MAX_VOLUME_RECORDS,
};

fn pair() -> VolumePair {
    VolumePair {
        first: 1,
        second: 2,
    }
}

fn record(id: &str, height: u64, volume: u64, kind: VolumeKind) -> VolumeRecord {
    VolumeRecord {
        event_id: id.into(),
        source_id: "1-2".into(),
        pair: pair(),
        block_height: height,
        raw_volume_atoms: volume,
        ltc_volume_atoms: volume,
        price_atoms: 200_000_000,
        kind,
    }
}

#[test]
fn pinned_source_vwap_vectors_match_exact_atoms() {
    let mut count = 0;
    for line in include_str!("../fixtures/volume_index_vwap.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 5);
        let data: Vec<_> = fields[2]
            .split(',')
            .filter(|entry| !entry.is_empty())
            .map(|entry| {
                let (a, b) = entry.split_once(':').unwrap();
                let (volume, price) = if fields[1] == "1" { (b, a) } else { (a, b) };
                VwapObservation {
                    volume_atoms: volume.parse().unwrap(),
                    price_atoms: price.parse().unwrap(),
                }
            })
            .collect();
        let actual = calculate_vwap(&data).unwrap();
        if fields[4] == "null" {
            assert_eq!(actual, None, "{}", fields[0]);
            assert_eq!(fields[3], "null");
        } else {
            let actual = actual.unwrap();
            assert_eq!(
                actual.price_atoms_half_up().unwrap(),
                fields[4].parse::<u64>().unwrap(),
                "{}",
                fields[0]
            );
            let js: f64 = fields[3].parse().unwrap();
            let ratio = actual.numerator as f64 / actual.denominator as f64 / 1e8;
            assert!((ratio - js).abs() <= 1e-14, "{}", fields[0]);
        }
        count += 1;
    }
    assert_eq!(count, 8);
}

#[test]
fn constant_share_source_fee_vectors_round_down() {
    let mut count = 0;
    for line in include_str!("../fixtures/volume_index_fee.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let values: Vec<u64> = line.split('\t').map(|v| v.parse().unwrap()).collect();
        let reward = fee_linked_reward_atoms(values[0], values[1], values[2]).unwrap();
        assert_eq!(reward, values[3]);
        assert!(reward <= values[0]);
        count += 1;
    }
    assert_eq!(count, 7);
    assert_eq!(
        fee_linked_reward_atoms(10, 1, 0),
        Err(VolumeError::InvalidShare)
    );
    assert_eq!(
        fee_linked_reward_atoms(10, 2, 1),
        Err(VolumeError::InvalidShare)
    );
    assert_eq!(
        fee_linked_reward_atoms(u64::MAX, u64::MAX, u64::MAX),
        Ok(u64::MAX)
    );
}

#[test]
fn inclusive_queries_and_type_totals_are_order_independent() {
    let mut history = VolumeHistory::new(8).unwrap();
    history
        .insert_batch(&[
            record("later", 12, 30, VolumeKind::Contract),
            record("second", 11, 20, VolumeKind::Token),
            record("first", 11, 10, VolumeKind::Utxo),
        ])
        .unwrap();
    assert!(history.sample_range(0, 10).unwrap().is_empty());
    assert_eq!(
        history
            .sample_range(11, 11)
            .unwrap()
            .iter()
            .map(|r| r.event_id.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );
    let totals = history.totals_through(11).unwrap();
    assert_eq!(
        (
            totals.global_atoms,
            totals.utxo_atoms,
            totals.contract_atoms
        ),
        (30, 10, 0)
    );
    assert_eq!(history.totals_through(12).unwrap().contract_atoms, 30);
    assert_eq!(history.sample_range(12, 11), Err(VolumeError::InvalidRange));
}

#[test]
fn source_character_zero_subtotal_is_distinct_from_type() {
    let mut history = VolumeHistory::new(3).unwrap();
    let mut misleading = record("token", 7, 17, VolumeKind::Token);
    misleading.source_id = "10-21".into();
    history
        .insert_batch(&[misleading, record("utxo", 7, 9, VolumeKind::Utxo)])
        .unwrap();
    let source = history.source_block_volumes(7).unwrap();
    assert_eq!((source.global_atoms, source.ltc_pairs_atoms), (26, 17));
    assert_eq!(history.totals_through(7).unwrap().utxo_atoms, 9);
    assert_eq!(history.source_block_volumes(8).unwrap().global_atoms, 0);
}

#[test]
fn directional_vwap_and_source_reverse_selection_are_explicit() {
    let mut history = VolumeHistory::new(4).unwrap();
    let direct = record("direct", 10, 100, VolumeKind::Token);
    let mut reverse = record("reverse", 11, 100, VolumeKind::Token);
    reverse.pair = pair().reversed();
    reverse.price_atoms = 800_000_000;
    history.insert_batch(&[direct, reverse]).unwrap();
    assert_eq!(
        history
            .vwap(pair(), 10, 11)
            .unwrap()
            .unwrap()
            .price_atoms_half_up()
            .unwrap(),
        200_000_000
    );
    assert_eq!(
        history
            .source_vwap(pair(), 10, 11)
            .unwrap()
            .unwrap()
            .price_atoms_half_up()
            .unwrap(),
        500_000_000
    );
    assert_eq!(
        history
            .source_vwap(pair(), 11, 11)
            .unwrap()
            .unwrap()
            .price_atoms_half_up()
            .unwrap(),
        800_000_000
    );
    assert_eq!(history.vwap(pair(), 11, 11).unwrap(), None);
}

#[test]
fn rollback_retains_boundary_and_replacement_suffix_is_replayable() {
    let mut history = VolumeHistory::new(3).unwrap();
    history
        .insert_batch(&[
            record("a", 1, 10, VolumeKind::Token),
            record("b", 2, 20, VolumeKind::Token),
        ])
        .unwrap();
    let retained = history.sample_range(1, 1).unwrap()[0].clone();
    assert_eq!(history.rollback_after(1), 1);
    assert_eq!(history.sample_range(1, 1).unwrap()[0], &retained);
    history
        .insert(record("b", 2, 99, VolumeKind::Token))
        .unwrap();
    assert_eq!(history.totals_through(2).unwrap().global_atoms, 109);
    assert_eq!(history.rollback_after(u64::MAX), 0);
    assert_eq!(history.rollback_after(0), 2);
    assert!(history.is_empty());
}

#[test]
fn duplicate_invalid_and_capacity_batches_are_atomic() {
    let mut history = VolumeHistory::new(2).unwrap();
    let original = record("a", 1, 10, VolumeKind::Token);
    history.insert(original.clone()).unwrap();
    let before = history.clone();
    assert_eq!(history.insert(original), Err(VolumeError::DuplicateEvent));
    let mut invalid = record("invalid", 2, 20, VolumeKind::Token);
    invalid.source_id = "x".repeat(MAX_IDENTIFIER_BYTES + 1);
    assert_eq!(
        history.insert_batch(&[invalid]),
        Err(VolumeError::InvalidIdentifier)
    );
    assert_eq!(
        history.insert_batch(&[
            record("b", 2, 20, VolumeKind::Token),
            record("c", 2, 20, VolumeKind::Token)
        ]),
        Err(VolumeError::CapacityExceeded)
    );
    assert_eq!(history, before);
    let mut empty = VolumeHistory::new(2).unwrap();
    let repeated = record("same", 1, 1, VolumeKind::Token);
    assert_eq!(
        empty.insert_batch(&[repeated.clone(), repeated]),
        Err(VolumeError::DuplicateEvent)
    );
    assert!(empty.is_empty());
    assert_eq!(VolumeHistory::new(0), Err(VolumeError::InvalidCapacity));
    assert_eq!(
        VolumeHistory::new(MAX_VOLUME_RECORDS + 1),
        Err(VolumeError::InvalidCapacity)
    );
}

#[test]
fn arithmetic_boundaries_fail_without_wrapping() {
    let big = VwapObservation {
        volume_atoms: u64::MAX,
        price_atoms: u64::MAX,
    };
    assert_eq!(
        calculate_vwap(&[big])
            .unwrap()
            .unwrap()
            .price_atoms_half_up(),
        Ok(u64::MAX)
    );
    assert_eq!(
        calculate_vwap(&[big, big]),
        Err(VolumeError::ArithmeticOverflow)
    );
    assert_eq!(
        calculate_vwap(&vec![big; MAX_VOLUME_RECORDS + 1]),
        Err(VolumeError::CapacityExceeded)
    );
    assert_eq!(
        WeightedPrice {
            numerator: u128::MAX,
            denominator: 1
        }
        .price_atoms_half_up(),
        Err(VolumeError::ArithmeticOverflow)
    );
    assert_eq!(
        WeightedPrice {
            numerator: 1,
            denominator: 0
        }
        .price_atoms_half_up(),
        Err(VolumeError::InvalidRange)
    );
    assert_eq!(
        WeightedPrice {
            numerator: u128::MAX - 1,
            denominator: u128::MAX
        }
        .price_atoms_half_up(),
        Ok(1)
    );
}
