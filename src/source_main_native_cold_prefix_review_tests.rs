use super::*;
use crate::source_main_native_owners::{NativeMainExpectation, NativeMainOwners};

fn raw_second_row() -> V {
    let mut second = row();
    let txid = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    second.insert("_id", s(&format!("tx-10-{txid}"))).unwrap();
    let mut value = second.get("value");
    value.insert("txId", s(txid)).unwrap();
    second.insert("value", value).unwrap();
    second
}
fn fresh_guard_input() -> V {
    let mut fresh = raw_second_row();
    let id = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    fresh.insert("_id", s(&format!("tx-10-{id}"))).unwrap();
    let mut value = fresh.get("value");
    value.insert("txId", s(id)).unwrap();
    fresh.insert("value", value).unwrap();
    fresh
}
fn owner_with_two_raw_inputs(temp: &Temp) -> SourceMainNativeSessionV13 {
    // Bootstrap through the real existing raw fixture. Rebuild its original
    // foundation before adding a second raw metadata INPUT and adopting once.
    let initial = owner(temp);
    let foundation = initial.foundation.clone();
    let head = initial.foundation_head;
    let current = initial.index.capture_verified_history_pin().unwrap();
    drop(initial);
    let mut index = open_post(temp, &current);
    index
        .import_row(&current.metadata, raw_second_row())
        .unwrap();
    let financial = SourceContractReplay::from_checkpoint(&foundation, head).unwrap();
    let expected = NativeMainExpectation {
        financial_head: financial.head(),
        namespace: Namespace::new("LTC", false, "txIndex").unwrap(),
        metadata: index.snapshot().unwrap(),
    };
    SourceMainNativeSessionV13::adopt(NativeMainOwners::adopt(financial, index, &expected).unwrap())
        .unwrap()
}
fn later_write(original: &mut SourceMainNativeSessionV13) {
    let points = record_one(
        original,
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![raw_second_row()]),
            height: V::number(10.),
            live: false,
        },
    );
    assert_eq!(
        points.len(),
        2,
        "distinct raw tx must execute its real native validity write"
    );
}
#[test]
fn reviewer_nonempty_completed_financial_prefix_stays_sealed_against_a_later_real_post() {
    let temp = Temp::new();
    let mut original = owner_with_two_raw_inputs(&temp);
    let points = record_one(&mut original, call(false));
    assert_eq!(points.len(), 2);
    let state = original.financial_runtime().state_value().unwrap();
    let receipts = original.history.clone();
    assert!(!receipts[0].effects.is_empty());
    assert_eq!(receipts[0].validity.len(), 1);
    let completed_tip = original.index.capture_verified_history_pin().unwrap();
    let (archive, pin) = original.checkpoint().unwrap();
    later_write(&mut original);
    let actual_post = original.index.capture_verified_history_pin().unwrap();
    assert_ne!(actual_post, completed_tip);
    assert_ne!(original.financial_runtime().state_value().unwrap(), state);
    drop(original);
    let completed = NativeMainHistoricalRowsExpectationV1 {
        archive: pin.clone(),
        current: completed_tip.clone(),
        commands: vec![points],
    };
    let rejected = SourceMainNativeSessionV13::reexecute_historical_rows(
        &archive,
        &completed,
        open_post(&temp, &actual_post),
    )
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
    assert_eq!(cold.financial_runtime().state_value().unwrap(), state);
    assert_eq!(cold.completed_prefix(), receipts);
    assert_eq!(*cold.completed_prefix_head(), pin.head);
    assert_eq!(cold.completed_tip(), &completed_tip);
    assert_eq!(cold.actual_post(), &actual_post);
    assert_eq!(io_counters(cold.index()), io);
    assert!(matches!(
        cold.parts
            .index
            .discard_rows(&actual_post.metadata, &[], "cannot release sealed prefix"),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(
        cold.index().capture_verified_history_pin().unwrap(),
        actual_post
    );
    assert_eq!(io_counters(cold.index()), io);
    assert!(SourceCompactIndex::open_verified_history_owner(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &actual_post
    )
    .is_err());
}

fn repaired_recorded_receipt_mismatch(archive: &[u8]) -> Vec<u8> {
    let payload = crate::codec::verify_crc(archive).unwrap();
    let mut cursor = crate::codec::Cursor::new(payload);
    cursor.take(5).unwrap();
    for _ in 0..2 {
        let len = cursor.u32().unwrap() as usize;
        cursor.take(len).unwrap();
    }
    cursor.take(8 + 32).unwrap();
    for _ in 0..3 {
        let len = cursor.u32().unwrap() as usize;
        cursor.take(len).unwrap();
    }
    assert_eq!(cursor.u32().unwrap(), 1);
    let command = cursor.u32().unwrap() as usize;
    cursor.take(command).unwrap();
    let count = cursor.u32().unwrap() as usize;
    let receipt = cursor.take(count).unwrap();
    assert!(!receipt.is_empty());
    let offset = receipt.as_ptr() as usize - payload.as_ptr() as usize;
    let mut changed = payload.to_vec();
    changed[offset + count - 1] ^= 1;
    crate::codec::with_crc(changed).unwrap()
}
#[test]
fn reviewer_repaired_receipt_mismatch_retains_actual_executed_effects_guard_and_post_lock() {
    let temp = Temp::new();
    let mut original = owner_with_two_raw_inputs(&temp);
    let points = record_one(&mut original, call(false));
    assert_eq!(points.len(), 2);
    let state = original.financial_runtime().state_value().unwrap();
    let receipts = original.history.clone();
    let completed_tip = original.index.capture_verified_history_pin().unwrap();
    let (archive, mut pin) = original.checkpoint().unwrap();
    later_write(&mut original);
    let actual_post = original.index.capture_verified_history_pin().unwrap();
    drop(original);
    let changed = repaired_recorded_receipt_mismatch(&archive);
    // Only the damaged artifact digest/CRC is repaired; the original separately
    // captured logical head/foundation/physical expectations stay unchanged.
    pin.archive_sha256 = crate::hash256::sha256(&changed);
    let expected = NativeMainColdPrefixExpectationV1 {
        completed: NativeMainHistoricalRowsExpectationV1 {
            archive: pin,
            current: completed_tip,
            commands: vec![points],
        },
        actual_post: actual_post.clone(),
    };
    let index = open_post(&temp, &actual_post);
    let io = io_counters(&index);
    let mut rejected = NativeMainColdPrefixV1::reexecute(&changed, &expected, index).unwrap_err();
    assert!(rejected.write_guard_retained());
    assert_eq!(
        rejected.financial_runtime().unwrap().state_value().unwrap(),
        state
    );
    assert_eq!(rejected.actual_completed_prefix(),receipts,"actual source effects must remain in the retained diagnostic receipt after drain/comparison failure");
    assert_eq!(
        rejected.index().capture_verified_history_pin().unwrap(),
        actual_post
    );
    assert_eq!(io_counters(rejected.index()), io);
    assert!(matches!(
        rejected
            .rejected
            .index
            .import_row(&actual_post.metadata, fresh_guard_input()),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(io_counters(rejected.index()), io);
    assert!(SourceCompactIndex::open_verified_history_owner(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &actual_post
    )
    .is_err());
}
