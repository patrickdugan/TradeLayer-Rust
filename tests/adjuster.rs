// Complete-source finite vectors plus atomic repaired metadata/checkpoint tests.
use tradelayer_core::adjuster::{
    AdjusterError, AdjusterPolicy, AdjustmentAction, TickSizeAdjuster, MAX_ADJUSTER_ACTIONS,
    MAX_ADJUSTER_CHECKPOINT_BYTES, MAX_ADJUSTER_EVENTS, MAX_EVENT_TIME_MILLIS,
    MAX_EVENT_TYPE_BYTES,
};

fn number(bits: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(bits, 16).unwrap())
}
fn text(hex: &str) -> String {
    if hex == "-" {
        return String::new();
    }
    let bytes: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    String::from_utf8(bytes).unwrap()
}
fn round_trip(owner: &TickSizeAdjuster) {
    owner.verify().unwrap();
    let bytes = owner.checkpoint_bytes().unwrap();
    let restored = TickSizeAdjuster::from_checkpoint_bytes(&bytes).unwrap();
    assert_eq!(&restored, owner);
    assert_eq!(restored.checkpoint_bytes().unwrap(), bytes);
}

#[test]
fn all_finite_rows_match_complete_unmodified_source() {
    let mut owner = TickSizeAdjuster::new();
    let mut name = "";
    let mut rows = 0;
    let mut observations = Vec::new();
    for line in include_str!("../fixtures/adjuster_vectors.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let cells: Vec<_> = line.split('\t').collect();
        if cells[0] == "Q" {
            observations.push(cells[1]);
            continue;
        }
        assert_eq!(cells.len(), 14, "{line}");
        if name != cells[1] {
            round_trip(&owner);
            owner = TickSizeAdjuster::new();
            name = cells[1];
        }
        let before_events = owner.events().len();
        let time: u64 = cells[5].parse().unwrap();
        match cells[3] {
            "C" => {
                assert_eq!(cells[2], "0");
                assert_eq!(cells[6], "constructed");
                assert!(owner.actions().is_empty());
            }
            "A" => {
                let returned = owner.adjust_tick_size(number(cells[4]), time).unwrap();
                assert_eq!(returned.to_bits(), number(cells[6]).to_bits(), "{name}");
            }
            "N" => {
                let returned = owner.increase_notional_size(time).unwrap();
                assert_eq!(cells[6], "undefined", "source has no return value");
                assert_eq!(
                    returned.to_bits(),
                    owner.state().min_contract_size.to_bits()
                );
            }
            "S" => {
                let (kind, value) = cells[4].split_once(':').unwrap();
                owner
                    .save_adjustment_event(&text(kind), number(value), time)
                    .unwrap();
                assert_eq!(number(cells[6]), 1.0, "recording DB returned one update");
            }
            _ => panic!("unknown source vector method"),
        }
        let state = owner.state();
        for (actual, expected) in [
            (state.tick_size, cells[7]),
            (state.min_contract_size, cells[8]),
            (state.notional_threshold, cells[9]),
            (state.notional_multiplier, cells[10]),
        ] {
            assert_eq!(actual.to_bits(), number(expected).to_bits(), "{name}");
        }
        assert_eq!(state.doubling_interval, cells[11].parse::<u32>().unwrap());
        assert_eq!(
            state.consecutive_doublings,
            cells[12].parse::<u32>().unwrap()
        );
        let expected: Vec<_> = if cells[13] == "-" {
            Vec::new()
        } else {
            cells[13].split(';').collect()
        };
        let actual = &owner.events()[before_events..];
        assert_eq!(actual.len(), expected.len(), "{name}");
        for (index, (event, specification)) in actual.iter().zip(expected).enumerate() {
            let parts: Vec<_> = specification.split(':').collect();
            assert_eq!(event.sequence, (before_events + index) as u64);
            assert_eq!(event.event_type, text(parts[0]));
            assert_eq!(event.value.to_bits(), number(parts[1]).to_bits());
            assert_eq!(event.time_millis, parts[2].parse::<u64>().unwrap());
        }
        rows += 1;
    }
    round_trip(&owner);
    assert_eq!(rows, 641);
    assert_eq!(observations.len(), 19);
    for required in [
        "complete-source-example",
        "coercion-symbol",
        "tick-state-overflow",
        "minimum-state-overflow",
        "untyped-direct-event",
        "real-db-async-and-missing-category",
    ] {
        assert!(observations.contains(&required));
    }
}

#[test]
fn defaults_and_direct_events_do_not_claim_tick_application() {
    let mut owner = TickSizeAdjuster::default();
    assert_eq!(owner.policy(), AdjusterPolicy::AtomicMetadataV1);
    assert_eq!(owner.state().tick_size.to_bits(), 0.1_f64.to_bits());
    assert_eq!(owner.state().min_contract_size, 1.0);
    assert_eq!(owner.state().notional_threshold, 2.0);
    assert_eq!(owner.state().notional_multiplier, 10.0);
    assert_eq!(owner.state().doubling_interval, 3);
    assert_eq!(owner.state().consecutive_doublings, 0);
    let original = owner.state().clone();
    let event = owner
        .save_adjustment_event("DoubleTickSize", -0.0, 0)
        .unwrap();
    assert_eq!(event.value.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(event.sequence, 0);
    assert_eq!(*owner.state(), original);
    assert!(matches!(
        owner.actions()[0],
        AdjustmentAction::SaveEvent { .. }
    ));
    owner
        .save_adjustment_event("", -2.0, MAX_EVENT_TIME_MILLIS)
        .unwrap();
    owner.save_adjustment_event("λ🙂", 0.0, 1).unwrap();
    round_trip(&owner);
}

#[test]
fn invalid_inputs_and_event_metadata_fail_atomically() {
    let mut owner = TickSizeAdjuster::new();
    owner.adjust_tick_size(2.0, 9).unwrap();
    let before = owner.clone();
    for input in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            owner.adjust_tick_size(input, 0),
            Err(AdjusterError::NonFiniteInput)
        );
        assert_eq!(owner, before);
        assert_eq!(
            owner.save_adjustment_event("x", input, 0),
            Err(AdjusterError::NonFiniteInput)
        );
        assert_eq!(owner, before);
    }
    assert_eq!(
        owner.increase_notional_size(MAX_EVENT_TIME_MILLIS + 1),
        Err(AdjusterError::InvalidTime)
    );
    assert_eq!(owner, before);
    assert_eq!(
        owner.save_adjustment_event(&"x".repeat(MAX_EVENT_TYPE_BYTES + 1), 1.0, 0),
        Err(AdjusterError::EventTypeTooLong)
    );
    assert_eq!(owner, before);
    owner
        .save_adjustment_event(&"x".repeat(MAX_EVENT_TYPE_BYTES), 1.0, 0)
        .unwrap();
    round_trip(&owner);
}

#[test]
fn harmless_infinite_threshold_is_distinct_from_state_overflow() {
    let mut owner = TickSizeAdjuster::new();
    owner.adjust_tick_size(2.0, 0).unwrap();
    owner.adjust_tick_size(2.0, 0).unwrap();
    for _ in 0..308 {
        owner.increase_notional_size(0).unwrap();
    }
    assert!(owner.state().min_contract_size.is_finite());
    assert!((owner.state().notional_threshold * owner.state().min_contract_size).is_infinite());
    let tick = owner.state().tick_size;
    let events = owner.events().len();
    assert_eq!(owner.adjust_tick_size(f64::MAX, 0).unwrap(), tick);
    assert_eq!(owner.state().consecutive_doublings, 0);
    assert_eq!(owner.events().len(), events);
    let before = owner.clone();
    assert_eq!(
        owner.increase_notional_size(0),
        Err(AdjusterError::ArithmeticOverflow)
    );
    assert_eq!(owner, before);
    round_trip(&owner);
}

#[test]
fn source_tick_overflow_rejects_entire_double_and_increase_transition() {
    let mut owner = TickSizeAdjuster::new();
    for _ in 0..281 {
        owner.adjust_tick_size(2.0, 0).unwrap();
    }
    assert_eq!(owner.state().consecutive_doublings, 2);
    let before = owner.clone();
    assert_eq!(
        owner.adjust_tick_size(2.0, 0),
        Err(AdjusterError::ArithmeticOverflow)
    );
    assert_eq!(owner, before);
    owner.adjust_tick_size(1.0, 0).unwrap();
    assert_eq!(owner.state().consecutive_doublings, 0);
    round_trip(&owner);
}

#[test]
fn third_call_event_capacity_failure_publishes_neither_event_then_retry_succeeds() {
    let mut owner = TickSizeAdjuster::new();
    owner.adjust_tick_size(2.0, 0).unwrap();
    owner.adjust_tick_size(2.0, 0).unwrap();
    for _ in 0..MAX_ADJUSTER_EVENTS - 3 {
        owner.save_adjustment_event("observation", 0.0, 0).unwrap();
    }
    assert_eq!(owner.events().len(), MAX_ADJUSTER_EVENTS - 1);
    let before = owner.clone();
    assert_eq!(
        owner.adjust_tick_size(2.0, 0),
        Err(AdjusterError::EventLimit)
    );
    assert_eq!(owner, before);
    owner.adjust_tick_size(1.0, 0).unwrap();
    owner.adjust_tick_size(2.0, 0).unwrap();
    assert_eq!(owner.events().len(), MAX_ADJUSTER_EVENTS);
    let full = owner.clone();
    assert_eq!(
        owner.save_adjustment_event("last", 0.0, 0),
        Err(AdjusterError::EventLimit)
    );
    assert_eq!(owner, full);
    round_trip(&owner);
}

#[test]
fn no_event_actions_still_have_a_bounded_replay_history() {
    let mut owner = TickSizeAdjuster::new();
    for _ in 0..MAX_ADJUSTER_ACTIONS {
        owner.adjust_tick_size(1.0, 0).unwrap();
    }
    assert!(owner.events().is_empty());
    let before = owner.clone();
    assert_eq!(
        owner.increase_notional_size(0),
        Err(AdjusterError::ActionLimit)
    );
    assert_eq!(owner, before);
    round_trip(&owner);
}

fn repair_crc(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.truncate(bytes.len() - 4);
    let mut crc = !0_u32;
    for &byte in &bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    bytes.extend_from_slice(&(!crc).to_be_bytes());
    bytes
}
fn event_positions(bytes: &[u8]) -> Vec<(usize, usize, usize)> {
    let count = u32::from_be_bytes(bytes[45..49].try_into().unwrap()) as usize;
    let mut cursor = 49;
    for _ in 0..count {
        let tag = bytes[cursor];
        cursor += 9;
        match tag {
            0 => cursor += 8,
            1 => {}
            2 => cursor += 1 + bytes[cursor] as usize + 8,
            _ => unreachable!(),
        }
    }
    let events = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
    cursor += 4;
    let mut offsets = Vec::new();
    for _ in 0..events {
        let start = cursor;
        cursor += 16;
        let len = bytes[cursor] as usize;
        let kind = cursor + 1;
        let value = kind + len;
        offsets.push((start, kind, value));
        cursor = value + 8;
    }
    offsets
}

#[test]
fn valid_crc_cannot_hide_changed_state_action_event_or_signed_zero() {
    let mut owner = TickSizeAdjuster::new();
    owner.adjust_tick_size(2.0, 0).unwrap();
    owner.adjust_tick_size(2.0, 3).unwrap();
    owner.adjust_tick_size(2.0, 2).unwrap();
    owner.save_adjustment_event("x", -0.0, 0).unwrap();
    owner.increase_notional_size(MAX_EVENT_TIME_MILLIS).unwrap();
    let original = owner.checkpoint_bytes().unwrap();
    let events = event_positions(&original);
    let mut mutations = Vec::new();
    for offset in [5, 13, 21, 29, 37, 41] {
        let mut bytes = original.clone();
        bytes[offset] ^= 1;
        mutations.push(bytes);
    }
    let mut bytes = original.clone();
    bytes[4] = 2;
    mutations.push(bytes);
    let mut bytes = original.clone();
    bytes[45..49].copy_from_slice(&((MAX_ADJUSTER_ACTIONS + 1) as u32).to_be_bytes());
    mutations.push(bytes);
    let mut bytes = original.clone();
    bytes[49] = 99;
    mutations.push(bytes);
    let mut bytes = original.clone();
    bytes[50..58].copy_from_slice(&(MAX_EVENT_TIME_MILLIS + 1).to_be_bytes());
    mutations.push(bytes);
    let mut bytes = original.clone();
    bytes[58..66].copy_from_slice(&f64::NAN.to_bits().to_be_bytes());
    mutations.push(bytes);
    let mut bytes = original.clone();
    bytes[58..66].copy_from_slice(&0.0_f64.to_bits().to_be_bytes());
    mutations.push(bytes);
    let (first, kind, value) = events[0];
    let mut bytes = original.clone();
    bytes[first + 7] = 1;
    mutations.push(bytes);
    let mut bytes = original.clone();
    bytes[first + 15] = 1;
    mutations.push(bytes);
    let mut bytes = original.clone();
    bytes[kind] = b'X';
    mutations.push(bytes);
    let mut bytes = original.clone();
    bytes[kind] = 0xff;
    mutations.push(bytes);
    let mut bytes = original.clone();
    bytes[value..value + 8].copy_from_slice(&f64::INFINITY.to_bits().to_be_bytes());
    mutations.push(bytes);
    // The direct event is fourth after two increases and a double+increase.
    let signed_zero = events[4].2;
    assert_eq!(
        &original[signed_zero..signed_zero + 8],
        &(-0.0_f64).to_bits().to_be_bytes()
    );
    let mut bytes = original.clone();
    bytes[signed_zero..signed_zero + 8].copy_from_slice(&0.0_f64.to_bits().to_be_bytes());
    mutations.push(bytes);
    for bytes in mutations {
        assert_eq!(
            TickSizeAdjuster::from_checkpoint_bytes(&repair_crc(bytes)),
            Err(AdjusterError::InvalidCheckpoint)
        );
    }
    round_trip(&owner);
}

#[test]
fn strict_checkpoint_rejects_prefixes_trailing_checksum_and_resource_abuse() {
    let mut owner = TickSizeAdjuster::new();
    owner.adjust_tick_size(2.0, 0).unwrap();
    let bytes = owner.checkpoint_bytes().unwrap();
    for end in 0..bytes.len() {
        assert!(TickSizeAdjuster::from_checkpoint_bytes(&bytes[..end]).is_err());
    }
    let mut damaged = bytes.clone();
    damaged[5] ^= 1;
    assert!(TickSizeAdjuster::from_checkpoint_bytes(&damaged).is_err());
    let mut trailing = bytes.clone();
    trailing.insert(trailing.len() - 4, 0);
    assert!(TickSizeAdjuster::from_checkpoint_bytes(&repair_crc(trailing)).is_err());
    assert!(
        TickSizeAdjuster::from_checkpoint_bytes(&vec![0; MAX_ADJUSTER_CHECKPOINT_BYTES + 1])
            .is_err()
    );
}
