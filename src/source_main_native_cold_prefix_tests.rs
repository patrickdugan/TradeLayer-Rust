use super::*;
use crate::source_compact_index_v2::CompactIndexError;

fn io_counters(index: &SourceCompactIndex) -> (u64, u64, u64) {
    let io = index.storage_io_counters().unwrap();
    (
        io.confirmed_append_bytes,
        io.copied_checkpoint_bytes,
        io.published_compactions,
    )
}

fn open_post(temp: &Temp, pin: &SourceIndexHistoryPinV1) -> SourceCompactIndex {
    SourceCompactIndex::open_verified_history_owner(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        pin,
    )
    .unwrap()
}

fn fresh_import_row() -> V {
    let mut raw = row();
    raw.insert("_id", s("cold-native-put-must-refuse")).unwrap();
    raw
}

#[test]
fn cold_prefix_reexecutes_original_completed_commands_while_retaining_later_real_native_post() {
    let temp = Temp::new();
    let mut original = owner(&temp);
    let points = record_one(
        &mut original,
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![]),
            height: V::number(10.),
            live: false,
        },
    );
    let prefix_state = original.financial_runtime().state_value().unwrap();
    let prefix_receipts = original.history.clone();
    let completed_tip = original.index.capture_verified_history_pin().unwrap();
    let (archive, archive_pin) = original.checkpoint().unwrap();
    // This later raw source call executes real Main0/mark/validity, advancing
    // both the financial owner and the same metadata file. No output is seeded.
    record_one(&mut original, call(false));
    let actual_post = original.index.capture_verified_history_pin().unwrap();
    assert_ne!(completed_tip, actual_post);
    assert_ne!(
        prefix_state,
        original.financial_runtime().state_value().unwrap()
    );
    let completed = NativeMainHistoricalRowsExpectationV1 {
        archive: archive_pin.clone(),
        current: completed_tip.clone(),
        commands: vec![points],
    };
    drop(original);
    let index = open_post(&temp, &actual_post);
    // The original public API must keep refusing a later physical Post.
    let rejected =
        SourceMainNativeSessionV13::reexecute_historical_rows(&archive, &completed, index)
            .unwrap_err();
    assert!(rejected.financial_runtime().is_none());
    let index = rejected.index;
    let io = io_counters(&index);
    let mut cold = NativeMainColdPrefixV1::reexecute(
        &archive,
        &NativeMainColdPrefixExpectationV1 {
            completed,
            actual_post: actual_post.clone(),
        },
        index,
    )
    .unwrap();
    assert_eq!(
        cold.financial_runtime().state_value().unwrap(),
        prefix_state
    );
    assert_eq!(cold.completed_prefix(), prefix_receipts);
    assert_eq!(cold.completed_prefix_head(), &archive_pin.head);
    assert_eq!(cold.completed_tip(), &completed_tip);
    assert_eq!(cold.actual_post(), &actual_post);
    assert_eq!(
        cold.index().capture_verified_history_pin().unwrap(),
        actual_post
    );
    assert_eq!(io_counters(cold.index()), io);
    assert!(matches!(
        cold.parts.index.import_row(&actual_post.metadata, row()),
        Err(CompactIndexError::WrongContext)
    ));
    assert!(matches!(
        cold.parts
            .index
            .import_row(&actual_post.metadata, fresh_import_row()),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(io_counters(cold.index()), io);
    assert!(SourceCompactIndex::open_verified_history_owner(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &actual_post,
    )
    .is_err());
}

#[test]
fn cold_prefix_with_empty_native_history_uses_historical_origin_not_final_native_rows() {
    let temp = Temp::new();
    let mut original = owner(&temp);
    let prefix_state = original.financial_runtime().state_value().unwrap();
    let completed_tip = original.index.capture_verified_history_pin().unwrap();
    let (archive, archive_pin) = original.checkpoint().unwrap();
    record_one(&mut original, call(false));
    let actual_post = original.index.capture_verified_history_pin().unwrap();
    drop(original);
    let cold = NativeMainColdPrefixV1::reexecute(
        &archive,
        &NativeMainColdPrefixExpectationV1 {
            completed: NativeMainHistoricalRowsExpectationV1 {
                archive: archive_pin,
                current: completed_tip.clone(),
                commands: vec![],
            },
            actual_post: actual_post.clone(),
        },
        open_post(&temp, &actual_post),
    )
    .unwrap();
    assert_eq!(
        cold.financial_runtime().state_value().unwrap(),
        prefix_state
    );
    assert!(cold.completed_prefix().is_empty());
    assert_eq!(cold.completed_tip(), &completed_tip);
    assert_eq!(cold.actual_post(), &actual_post);
    assert!(cold.capability.original_current() != cold.completed_tip());
}

#[test]
fn cold_prefix_refuses_wrong_historical_expectation_and_retains_final_owner_guard_and_lock() {
    let temp = Temp::new();
    let mut original = owner(&temp);
    let mut completed_tip = original.index.capture_verified_history_pin().unwrap();
    let (archive, archive_pin) = original.checkpoint().unwrap();
    record_one(&mut original, call(false));
    let actual_post = original.index.capture_verified_history_pin().unwrap();
    drop(original);
    completed_tip.rows.chain_sha256d[0] ^= 1;
    let index = open_post(&temp, &actual_post);
    let io = io_counters(&index);
    let mut rejected = NativeMainColdPrefixV1::reexecute(
        &archive,
        &NativeMainColdPrefixExpectationV1 {
            completed: NativeMainHistoricalRowsExpectationV1 {
                archive: archive_pin,
                current: completed_tip,
                commands: vec![],
            },
            actual_post: actual_post.clone(),
        },
        index,
    )
    .unwrap_err();
    assert!(rejected.financial_runtime().is_none());
    assert!(rejected.actual_completed_prefix().is_empty());
    assert!(rejected.write_guard_retained());
    assert_eq!(
        rejected.index().capture_verified_history_pin().unwrap(),
        actual_post
    );
    assert_eq!(io_counters(rejected.index()), io);
    assert!(matches!(
        rejected
            .rejected
            .index
            .import_row(&actual_post.metadata, row()),
        Err(CompactIndexError::WrongContext)
    ));
    assert!(matches!(
        rejected
            .rejected
            .index
            .import_row(&actual_post.metadata, fresh_import_row()),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert!(SourceCompactIndex::open_verified_history_owner(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &actual_post,
    )
    .is_err());
}

#[path = "source_main_native_cold_prefix_review_tests.rs"]
mod independent_prefix_review_tests;
