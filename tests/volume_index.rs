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

use tradelayer_core::volume_index::{
    source_attenuated_liquidity_reward, source_calculate_vwap, source_contract_ltc_volume,
    source_contract_unit_ltc, source_liquidity_reward_share, source_normalize_contract_info,
    source_token_ltc_volume, source_vesting_projection, source_volatility, SourceBlockVolumeResult,
    SourceContractFields, SourceContractInfo, SourcePriceOrigin, SourceRewardContext,
    SourceRewardOptions, SourceValuationRequest, SourceValuationVolume, SourceVolumeAction,
    SourceVolumeDocument, SourceVolumeError, SourceVolumeSave, SourceVolumeStore,
    SourceVolumeValue, MAX_SOURCE_ACTIONS, MAX_SOURCE_HEIGHT, SOURCE_METHODS,
};
fn float(text: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(text, 16).unwrap())
}
fn options(f: &[&str]) -> SourceRewardOptions {
    SourceRewardOptions {
        max_share: float(f[0]),
        min_share: float(f[1]),
        pivot_ltc: float(f[2]),
        slope: float(f[3]),
    }
}
fn numeric_result(
    result: Result<f64, SourceVolumeError>,
    expected: &str,
    name: &str,
    transcendental: bool,
) {
    match expected {
        "nonfinite" => assert_eq!(result, Err(SourceVolumeError::NonFinite), "{name}"),
        "throw" => assert!(result.is_err(), "{name}"),
        _ => {
            let expected = float(expected);
            let actual = result.unwrap_or_else(|e| panic!("{name}: {e:?}"));
            if transcendental && actual != 0.0 {
                assert!(
                    (actual - expected).abs() <= 16.0 * f64::EPSILON * expected.abs().max(1.0),
                    "{name}: {actual} != {expected}"
                );
            } else {
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "{name}: {actual} != {expected}"
                );
            }
        }
    }
}

#[test]
fn complete_source_numeric_vectors_cover_prices_valuation_rewards_and_volatility() {
    let (mut count, mut inventory, mut scenes) = (0, 0, 0);
    let mut zero_grid = [0, 0];
    for line in include_str!("../fixtures/volume_index_methods.tsv")
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let f: Vec<_> = line.split('\t').collect();
        match f[0] {
            "I" => {
                let raw = json_hex(f[1]);
                let names = raw.as_array().unwrap();
                assert_eq!(names.len(), 27);
                for name in SOURCE_METHODS {
                    assert!(names.iter().any(|v| v.as_string().unwrap() == name));
                }
                inventory += 1;
            }
            "M" => {
                assert_eq!(f.len(), 10);
                for value in &f[3..5] {
                    json_hex(value);
                }
                for value in &f[6..10] {
                    json_hex(value);
                }
                if f[1] == "default-constructor" {
                    assert_eq!(f[5], "throw");
                }
                scenes += 1;
            }
            "U" => {
                numeric_result(
                    source_contract_unit_ltc(
                        SourceContractInfo {
                            notional_property_id: f[2].parse().unwrap(),
                            notional_value: float(f[3]),
                            leverage: float(f[4]),
                            ..Default::default()
                        },
                        float(f[5]),
                        f[6] == "1",
                    ),
                    f[7],
                    f[1],
                    false,
                );
                count += 1;
            }
            "T" => {
                numeric_result(
                    source_token_ltc_volume([float(f[2]), float(f[3])], [float(f[4]), float(f[5])]),
                    f[6],
                    f[1],
                    false,
                );
                count += 1;
            }
            "C" => {
                numeric_result(
                    source_contract_ltc_volume(
                        float(f[2]),
                        float(f[3]),
                        float(f[4]),
                        f[5] == "1",
                        float(f[6]),
                    ),
                    f[7],
                    f[1],
                    false,
                );
                count += 1;
            }
            "R" => {
                if f[1].starts_with("zero-grid-") {
                    zero_grid[0] += 1;
                }
                numeric_result(
                    source_liquidity_reward_share(float(f[2]), &options(&f[3..7])),
                    f[7],
                    f[1],
                    true,
                );
                count += 1;
            }
            "F" => {
                if f[1].starts_with("zero-grid-") {
                    zero_grid[1] += 1;
                }
                numeric_result(
                    source_attenuated_liquidity_reward(
                        float(f[2]),
                        float(f[3]),
                        &options(&f[4..8]),
                    ),
                    f[8],
                    f[1],
                    false,
                );
                count += 1;
            }
            "V" => {
                let prices = if f[2] == "-" {
                    vec![]
                } else {
                    f[2].split(',').map(float).collect()
                };
                numeric_result(source_volatility(&prices), f[3], f[1], true);
                count += 1;
            }
            "W" => {
                let entries = f[2]
                    .split(',')
                    .map(|e| {
                        let (a, b) = e.split_once(':').unwrap();
                        (float(a), float(b))
                    })
                    .collect::<Vec<_>>();
                numeric_result(
                    source_calculate_vwap(&entries)
                        .unwrap()
                        .ok_or(SourceVolumeError::Arithmetic),
                    f[3],
                    f[1],
                    false,
                );
                count += 1;
            }
            _ => panic!("unknown fixture row"),
        }
    }
    assert_eq!((inventory, scenes, count), (1, 49, 312));
    assert_eq!(zero_grid, [32, 32]);
}
fn source_doc(id: &str, height: u64, price: f64, volume: Option<f64>) -> SourceVolumeDocument {
    SourceVolumeDocument {
        id: id.into(),
        value: SourceVolumeValue {
            block_height: Some(height),
            price: Some(price),
            volume,
            ..Default::default()
        },
        ..Default::default()
    }
}
fn source_save(id: &str, height: u64, volume: f64, kind: &str) -> SourceVolumeAction {
    SourceVolumeAction::SaveVolume(SourceVolumeSave {
        id: id.into(),
        block_height: height,
        raw_volume: volume,
        ltc_volume: volume,
        price: 2.0,
        kind: kind.into(),
    })
}
#[test]
fn mutable_source_metadata_retains_repeat_count_overwrites_strict_prefix_and_schemas() {
    let mut s = SourceVolumeStore::new(10).unwrap();
    s.apply(source_save("1-2", 10, 3.0, "UTXO")).unwrap();
    s.apply(source_save("1-2", 10, 3.0, "UTXO")).unwrap();
    assert_eq!(s.len(), 3);
    assert_eq!(s.actions().len(), 2);
    assert_eq!(
        s.get_cumulative_volumes(10)
            .unwrap()
            .global_cumulative_volume,
        0.0
    );
    assert_eq!(
        s.get_cumulative_volumes(11)
            .unwrap()
            .global_cumulative_volume,
        6.0
    );
    assert_eq!(
        s.get_cumulative_volumes(11).unwrap().ltc_pair_total_volume,
        6.0
    );
    assert_eq!(
        s.get_cumulative_volumes(11)
            .unwrap()
            .contract_cumulative_volume,
        0.0
    );
    assert!(s.sample_volumes_by_block(10).unwrap().is_empty());
    assert_eq!(
        s.get_vwap(pair(), 10, 10),
        Err(SourceVolumeError::NonFinite)
    );
    assert_eq!(
        s.get_block_volumes(10).unwrap(),
        SourceBlockVolumeResult::Totals {
            ltc_pairs: 0.0,
            global: 3.0
        }
    );
    assert_eq!(
        s.get_block_volumes(9).unwrap(),
        SourceBlockVolumeResult::EmptyZero
    );
    s.apply(source_save("10-21", 11, 4.0, "token")).unwrap();
    assert_eq!(
        s.get_block_volumes(11).unwrap(),
        SourceBlockVolumeResult::Totals {
            ltc_pairs: 4.0,
            global: 4.0
        }
    );
    assert_eq!(
        s.get_cumulative_volumes(11)
            .unwrap()
            .global_cumulative_volume,
        0.0
    );
    assert_eq!(
        s.get_cumulative_volumes(12)
            .unwrap()
            .global_cumulative_volume,
        10.0
    );
    assert_eq!(
        s.get_cumulative_volumes(12).unwrap().ltc_pair_total_volume,
        6.0
    );
    let mut legacy = source_doc("1-2-x", 8, 5.0, Some(2.0));
    legacy.top_block_height = Some(8);
    legacy.top_volume = Some(2.0);
    s.apply(SourceVolumeAction::Put(legacy)).unwrap();
    assert_eq!(s.sample_volumes_by_block_range(8, 9).unwrap().len(), 1);
    assert_eq!(s.calculate_cumulative_volume(1, 2).unwrap(), 2.0);
    assert_eq!(s.audit_vwap(8).unwrap(), Some(0.001));
    assert_eq!(s.repaired_recent_vwap(1, 2, 1).unwrap(), Some(5.0));
    s.apply(SourceVolumeAction::Put(source_doc(
        "2-1",
        8,
        8.0,
        Some(2.0),
    )))
    .unwrap();
    let mut direct = source_doc("1-2", 9, 4.0, Some(2.0));
    direct.value.ltc_volume = Some(3.0);
    s.apply(SourceVolumeAction::Put(direct)).unwrap();
    assert_eq!(s.get_vwap(pair(), 10, 2).unwrap(), Some(6.0));
}
#[test]
fn source_latest_prices_fallback_and_named_asof_repair_cannot_be_confused() {
    let mut s = SourceVolumeStore::new(8).unwrap();
    let fallback = s.get_token_price_in_ltc("0").unwrap();
    assert_eq!(fallback.price, 0.001);
    assert_eq!(fallback.origin, SourcePriceOrigin::Fallback);
    s.apply(SourceVolumeAction::Put(source_doc("1-2", 100, 8.0, None)))
        .unwrap();
    s.apply(SourceVolumeAction::Put(source_doc("2-1", 9, 4.0, None)))
        .unwrap();
    assert_eq!(s.get_last_price("1-2", 1).unwrap().unwrap().price, 8.0);
    let at9 = s.retained_price_as_of("1-2", 9).unwrap().unwrap();
    assert_eq!(at9.price, 0.25);
    assert_eq!(at9.origin, SourcePriceOrigin::Inverse);
    assert_eq!(at9.block_height, Some(9));
    assert_eq!(s.retained_price_as_of("1-2", 8).unwrap(), None);
    assert_eq!(
        s.get_last_price("1-2-extra", 1).unwrap().unwrap().price,
        0.25
    );
    assert_eq!(s.get_last_price("no-pair-found", 1).unwrap(), None);
    s.apply(SourceVolumeAction::Put(source_doc("0-7", 10, 0.25, None)))
        .unwrap();
    assert_eq!(
        s.get_token_price_in_ltc("7").unwrap().origin,
        SourcePriceOrigin::Direct
    );
    let request = SourceValuationRequest {
        volume: SourceValuationVolume::Token([2.0, 3.0]),
        id: "0-7".into(),
        kind: "token".into(),
        collateral_id: None,
        notional_value: None,
        inverse: None,
        trade_price: None,
    };
    let valuation = s.get_utxo_equivalent_volume(&request).unwrap().unwrap();
    assert_eq!(valuation.value, 0.752);
    assert_eq!(valuation.prices[0].origin, SourcePriceOrigin::Fallback);
    let mut contract = SourceValuationRequest {
        volume: SourceValuationVolume::Scalar(2.0),
        id: "1".into(),
        kind: "contract".into(),
        collateral_id: Some(7),
        notional_value: Some(10.0),
        inverse: Some(false),
        trade_price: Some(100.0),
    };
    assert_eq!(
        s.get_utxo_equivalent_volume(&contract)
            .unwrap()
            .unwrap()
            .value,
        500.0
    );
    contract.collateral_id = Some(0);
    assert_eq!(s.get_utxo_equivalent_volume(&contract).unwrap(), None);
    assert_eq!(
        s.get_tl_price_in_ltc().unwrap().origin,
        SourcePriceOrigin::Fallback
    );
}
#[test]
fn reward_context_and_normalized_contract_metadata_are_nonspending_projections() {
    let mut max_context_store = SourceVolumeStore::new(4).unwrap();
    let max_context = SourceRewardContext {
        fee_paid: Some(1.0),
        block: Some(MAX_SOURCE_HEIGHT),
        ..Default::default()
    };
    assert_eq!(
        max_context_store
            .liquidity_reward(0.0, &max_context)
            .unwrap(),
        0.35
    );
    assert_eq!(
        max_context_store.get_cumulative_volumes(MAX_SOURCE_HEIGHT + 1),
        Err(SourceVolumeError::InvalidRange)
    );
    max_context_store
        .apply(SourceVolumeAction::UpdateCumulative {
            volume: 1000.0,
            kind: "UTXO".into(),
            block: MAX_SOURCE_HEIGHT,
        })
        .unwrap();
    assert_eq!(
        max_context_store
            .get_cumulative_volumes(MAX_SOURCE_HEIGHT)
            .unwrap()
            .global_cumulative_volume,
        0.0
    );
    assert_eq!(
        max_context_store
            .liquidity_reward(0.0, &max_context)
            .unwrap(),
        source_attenuated_liquidity_reward(1.0, 1000.0, &max_context.options).unwrap()
    );
    let metadata = source_normalize_contract_info(
        9,
        Some(2),
        SourceContractFields {
            id: Some(1),
            notional_value: Some(2.5),
            leverage: Some(0.0),
            ticker: Some("X".into()),
            native: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(metadata.contract_id, 1);
    assert_eq!(metadata.info.leverage, 1.0);
    assert_eq!(metadata.info.notional_value, 2.5);
    assert_eq!(
        source_normalize_contract_info(9, Some(2), SourceContractFields::default())
            .unwrap()
            .contract_id,
        2
    );
    let mut s = SourceVolumeStore::new(4).unwrap();
    s.apply(SourceVolumeAction::UpdateCumulative {
        volume: 1000.0,
        kind: "UTXO".into(),
        block: 9,
    })
    .unwrap();
    let ctx = SourceRewardContext {
        fee_rate: Some(0.001),
        block: Some(9),
        ..Default::default()
    };
    let before = s.checkpoint_bytes().unwrap();
    assert_eq!(
        s.liquidity_reward(100.0, &ctx).unwrap(),
        source_attenuated_liquidity_reward(0.1, 1000.0, &ctx.options).unwrap()
    );
    assert_eq!(
        s.baseline_liquidity_reward(100.0, 0.2, &ctx).unwrap(),
        source_attenuated_liquidity_reward(0.2, 1000.0, &ctx.options).unwrap()
    );
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
    let negative_zero_options = SourceRewardOptions {
        max_share: -0.0,
        min_share: -0.0,
        pivot_ltc: 1000.0,
        slope: 0.0,
    };
    assert_eq!(
        source_liquidity_reward_share(0.0, &negative_zero_options)
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        source_attenuated_liquidity_reward(1.0, 0.0, &negative_zero_options)
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        source_attenuated_liquidity_reward(0.0, 0.0, &negative_zero_options)
            .unwrap()
            .to_bits(),
        0.0_f64.to_bits()
    );
    let underflow = SourceContractInfo {
        notional_value: 1e-300,
        leverage: -1e308,
        ..Default::default()
    };
    assert_eq!(
        source_contract_unit_ltc(underflow, 1.0, false)
            .unwrap()
            .to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(
        source_contract_unit_ltc(underflow, 1.0, true),
        Err(SourceVolumeError::ZeroContractValue)
    );
    assert_eq!(
        source_contract_unit_ltc(
            SourceContractInfo {
                leverage: -1.0,
                ..underflow
            },
            1.0,
            false
        )
        .unwrap()
        .to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(source_vesting_projection(None).unwrap(), (0.0, 0.0));
    assert!(matches!(
        source_vesting_projection(Some(1e10)),
        Err(SourceVolumeError::UnsupportedSourcePath(_))
    ));
    assert_eq!(
        source_contract_unit_ltc(
            SourceContractInfo {
                notional_value: 1e308,
                notional_property_id: 7,
                ..Default::default()
            },
            1e308,
            false
        )
        .unwrap(),
        0.0
    );
}
#[test]
fn source_metadata_failures_capacity_and_nonfinite_mutations_are_atomic() {
    let mut tiny = SourceVolumeStore::new(2).unwrap();
    let before = tiny.clone();
    assert_eq!(
        tiny.apply(source_save("1-2", 10, 3.0, "UTXO")),
        Err(SourceVolumeError::Capacity)
    );
    assert_eq!(tiny, before);
    let mut s = SourceVolumeStore::new(3).unwrap();
    s.apply(source_save("1-2", 10, 3.0, "UTXO")).unwrap();
    let before = s.checkpoint_bytes().unwrap();
    assert_eq!(
        s.apply(source_save("2-3", 11, 1.0, "token")),
        Err(SourceVolumeError::Capacity)
    );
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
    assert_eq!(
        s.apply(SourceVolumeAction::UpdateCumulative {
            volume: f64::NAN,
            kind: "UTXO".into(),
            block: 11
        }),
        Err(SourceVolumeError::NonFinite)
    );
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
    assert_eq!(
        s.apply(SourceVolumeAction::SaveVolatility {
            lookback: 14400,
            now_ms: 0
        }),
        Err(SourceVolumeError::Capacity)
    );
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
    assert_eq!(
        s.apply(SourceVolumeAction::UpdateCumulative {
            volume: 1.0,
            kind: "UTXO".into(),
            block: MAX_SOURCE_HEIGHT + 1
        }),
        Err(SourceVolumeError::InvalidRange)
    );
    s.apply(source_save("1-2", 11, 1.0, "token")).unwrap();
    assert_eq!(
        s.get_cumulative_volumes(12)
            .unwrap()
            .global_cumulative_volume,
        4.0
    );
    let mut bad_math = SourceVolumeStore::new(4).unwrap();
    bad_math
        .apply_batch(&[
            SourceVolumeAction::Put(source_doc("x", 1, 100.0, None)),
            SourceVolumeAction::Put(source_doc("y", 2, 101.0, None)),
        ])
        .unwrap();
    let before = bad_math.checkpoint_bytes().unwrap();
    assert_eq!(
        bad_math.apply(SourceVolumeAction::SaveVolatility {
            lookback: 2,
            now_ms: 0
        }),
        Err(SourceVolumeError::NonFinite)
    );
    assert_eq!(bad_math.checkpoint_bytes().unwrap(), before);
}
#[test]
fn batched_metadata_and_action_prefix_rollback_rebuild_exact_retained_versions() {
    let mut s = SourceVolumeStore::new(4).unwrap();
    let actions = [
        source_save("1-2", 10, 3.0, "UTXO"),
        source_save("1-2", 11, 4.0, "token"),
    ];
    s.apply_batch(&actions).unwrap();
    let original = s.clone();
    assert_eq!(s.rollback_to_action_count(1).unwrap(), 1);
    assert_eq!(s.document("1-2").unwrap().value.block_height, Some(10));
    assert_eq!(
        s.get_cumulative_volumes(11)
            .unwrap()
            .global_cumulative_volume,
        3.0
    );
    s.apply(actions[1].clone()).unwrap();
    assert_eq!(
        s.checkpoint_bytes().unwrap(),
        original.checkpoint_bytes().unwrap()
    );
    let before = s.clone();
    assert_eq!(
        s.apply_batch(&[
            source_save("2-3", 12, 1.0, "token"),
            source_save("3-4", 12, 1.0, "token")
        ]),
        Err(SourceVolumeError::Capacity)
    );
    assert_eq!(s, before);
    assert_eq!(
        s.rollback_to_action_count(3),
        Err(SourceVolumeError::InvalidRange)
    );
    assert_eq!(s.rollback_to_action_count(0).unwrap(), 2);
    assert!(s.is_empty());
    assert!(s.actions().is_empty());
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
fn repair_crc(bytes: &mut [u8]) {
    let split = bytes.len() - 4;
    let crc = crc32(&bytes[..split]);
    bytes[split..].copy_from_slice(&crc.to_be_bytes());
}
#[test]
fn complete_metadata_checkpoint_replays_actions_binds_cache_and_preserves_negative_zero() {
    let mut s = SourceVolumeStore::new(10).unwrap();
    for (i, p) in [100.0, 101.0, 103.0].into_iter().enumerate() {
        s.apply(SourceVolumeAction::Put(source_doc(
            &format!("x{i}"),
            10 - i as u64,
            p,
            Some(1.0),
        )))
        .unwrap();
    }
    s.apply(SourceVolumeAction::SaveVolatility {
        lookback: 14400,
        now_ms: 0,
    })
    .unwrap();
    assert!(s.document("volIndex").unwrap().value.volatility.unwrap() > 0.0);
    assert_eq!(s.document("volIndex").unwrap().value.updated_at_ms, Some(0));
    s.apply(SourceVolumeAction::UpdateCumulative {
        volume: -0.000000001,
        kind: "UTXO".into(),
        block: 11,
    })
    .unwrap();
    assert_eq!(
        s.document("globalCumulativeVolume")
            .unwrap()
            .value
            .global_cumulative_volume
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        s.get_cumulative_volumes(12)
            .unwrap()
            .global_cumulative_volume
            .to_bits(),
        0.0_f64.to_bits()
    );
    let bytes = s.checkpoint_bytes().unwrap();
    let restored = SourceVolumeStore::from_checkpoint_bytes(&bytes).unwrap();
    assert_eq!(restored.checkpoint_bytes().unwrap(), bytes);
    assert_eq!(restored, s);
    for cut in 0..bytes.len() {
        assert!(
            SourceVolumeStore::from_checkpoint_bytes(&bytes[..cut]).is_err(),
            "prefix{cut}"
        );
    }
    let mut tampered = bytes.clone();
    let end = tampered.len() - 5;
    tampered[end] ^= 1;
    repair_crc(&mut tampered);
    assert!(SourceVolumeStore::from_checkpoint_bytes(&tampered).is_err());
    let mut wrong = bytes.clone();
    wrong[4] = 2;
    repair_crc(&mut wrong);
    assert!(SourceVolumeStore::from_checkpoint_bytes(&wrong).is_err());
    let mut extra = bytes[..bytes.len() - 4].to_vec();
    extra.push(0);
    extra.extend_from_slice(&[0; 4]);
    repair_crc(&mut extra);
    assert!(SourceVolumeStore::from_checkpoint_bytes(&extra).is_err());
    let mut oversized = b"TLVI\x01".to_vec();
    oversized.extend_from_slice(&10_u32.to_be_bytes());
    oversized.extend_from_slice(&((MAX_SOURCE_ACTIONS + 1) as u32).to_be_bytes());
    oversized.extend_from_slice(&[0; 4]);
    repair_crc(&mut oversized);
    assert_eq!(
        SourceVolumeStore::from_checkpoint_bytes(&oversized),
        Err(SourceVolumeError::Capacity)
    );
}
fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn json_hex(text: &str) -> tradelayer_core::client::RpcValue {
    tradelayer_core::client::RpcValue::parse(std::str::from_utf8(&unhex(text)).unwrap()).unwrap()
}
#[test]
fn actual_source_repeated_save_documents_match_typed_atomic_metadata_replay() {
    let line = include_str!("../fixtures/volume_index_methods.tsv")
        .lines()
        .find(|s| s.starts_with("M\trepeat-save-overwrite-double-count\t"))
        .unwrap();
    let f: Vec<_> = line.split('\t').collect();
    let program = json_hex(f[3]).as_array().unwrap();
    let mut s = SourceVolumeStore::new(10).unwrap();
    for step in program {
        let step = step.as_array().unwrap();
        let method = step[0].as_string().unwrap();
        let args = step[1].as_array().unwrap();
        if method == "saveVolumeDataById" {
            let number = |n: usize| args[n].as_json().parse().unwrap();
            s.apply(SourceVolumeAction::SaveVolume(SourceVolumeSave {
                id: args[0].as_string().unwrap(),
                raw_volume: number(1),
                ltc_volume: number(2),
                price: number(3),
                block_height: args[4].as_u64().unwrap(),
                kind: args[5].as_string().unwrap(),
            }))
            .unwrap();
        }
    }
    let expected = json_hex(f[7]).as_array().unwrap();
    assert_eq!(s.len(), expected.len());
    for (actual, expected) in s.documents().zip(expected) {
        let id = expected.field("_id").unwrap().unwrap().as_string().unwrap();
        assert_eq!(actual.id, id);
        let value = expected.field("value").unwrap().unwrap();
        for (key, actual) in [
            ("rawVolume", actual.value.raw_volume),
            ("ltcVolume", actual.value.ltc_volume),
            ("price", actual.value.price),
            (
                "globalCumulativeVolume",
                actual.value.global_cumulative_volume,
            ),
            ("ltcPairTotalVolume", actual.value.ltc_pair_total_volume),
        ] {
            let expected = value.field(key).unwrap();
            assert_eq!(actual.is_some(), expected.is_some(), "{id}:{key}");
            if let (Some(actual), Some(expected)) = (actual, expected) {
                assert_eq!(
                    actual.to_bits(),
                    expected.as_json().parse::<f64>().unwrap().to_bits(),
                    "{id}:{key}"
                );
            }
        }
    }
    assert_eq!(
        SourceVolumeStore::from_checkpoint_bytes(&s.checkpoint_bytes().unwrap()).unwrap(),
        s
    );
}
#[test]
fn actual_source_failures_and_partial_db_effects_are_not_fabricated_successes() {
    let mut count = 0;
    for line in include_str!("../fixtures/volume_index_quirks.tsv")
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let f: Vec<_> = line.split('\t').collect();
        assert_eq!(f.len(), 11);
        assert_eq!(f[0], "Q");
        let result = json_hex(f[6]);
        let docs = json_hex(f[7]).as_array().unwrap();
        match f[1] {
            "partial-save-second-write-failure" => {
                assert_eq!(f[5], "throw");
                assert_eq!(docs.len(), 1);
            }
            "partial-save-third-write-failure" => {
                assert_eq!(f[5], "throw");
                assert_eq!(docs.len(), 2);
            }
            "source-ignored-height" => assert_eq!(result.as_json(), "8"),
            "source-zero-string-truthy" => assert_eq!(result.as_string().unwrap(), "0"),
            "source-vesting-threshold-failure" => assert_eq!(f[5], "throw"),
            _ => {}
        }
        count += 1;
    }
    assert_eq!(count, 26);
}
