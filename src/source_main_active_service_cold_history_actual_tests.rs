// Child of actual durable controller tests; reuse their literal source raw
// reset/native/RPC inputs, never expected financial state.
use super::*;
use crate::source_main_active_service_wal::VerifiedActiveServiceAckHistoryV1;
fn history_temp() -> Temp {
    let parent = std::env::temp_dir().canonicalize().unwrap();
    let root = parent.join(format!(
        "actual-cold-active-ack-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    Temp { root, parent }
}
#[test]
fn cold_actual_source_start_pause_ack_chain_opens_without_live_financial_controller() {
    let temp = history_temp();
    let (owner, docs) = setup(&temp, "detached-live-rpc-failure");
    let mut owner = SourceMainNativeDurableActiveServiceV1::create_with_verified_history(
        owner,
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    wal_start(&mut owner, docs);
    run_action(&mut owner, NativeMainActiveServiceActionV1::Pause);
    let receipts = owner.history().to_vec();
    let head = owner.head();
    let pin = owner.wal_pin().unwrap();
    drop(owner);
    let reader = VerifiedActiveServiceAckHistoryV1::open(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &pin,
    )
    .unwrap();
    assert_eq!(reader.head(), head);
    assert_eq!(reader.entries().len(), 2);
    for (actual, expected) in reader.entries().iter().zip(&receipts) {
        assert_eq!(actual.sequence, expected.sequence);
        assert_eq!(actual.entry, expected.entry);
        assert_eq!(actual.head, expected.head);
        assert_eq!(
            crate::source_main_active_service_wal::ActiveServiceWalBlobV1::of(
                actual.actual_before()
            )
            .unwrap(),
            expected.before
        );
        assert_eq!(
            crate::source_main_active_service_wal::ActiveServiceWalBlobV1::of(
                actual.actual_accepted()
            )
            .unwrap(),
            expected.accepted
        );
        assert_eq!(
            NativeMainActiveServiceActionV1::from_canonical_action_bytes(actual.raw_action())
                .unwrap()
                .canonical_action_bytes()
                .unwrap(),
            actual.raw_action()
        );
    }
    assert_eq!(reader.pin().unwrap(), pin);
}
#[test]
fn cold_actual_native_append_and_resolve_history_retains_every_raw_evidence_blob() {
    let temp = history_temp();
    let (owner, docs) = setup(&temp, "same-held-live-validity");
    let mut owner = SourceMainNativeDurableActiveServiceV1::create_with_verified_history(
        owner,
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    wal_start(&mut owner, docs);
    drive_to_live(&mut owner);
    run_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    run_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    let receipts = owner.history().to_vec();
    let pin = owner.wal_pin().unwrap();
    let head = owner.head();
    drop(owner);
    let reader = VerifiedActiveServiceAckHistoryV1::open(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &pin,
    )
    .unwrap();
    assert_eq!(reader.head(), head);
    assert_eq!(reader.entries().len(), receipts.len());
    let mut appends = 0;
    for (actual, expected) in reader.entries().iter().zip(&receipts) {
        assert_eq!(actual.entry, expected.entry);
        assert_eq!(actual.head, expected.head);
        if let Some(mutations) = actual.native_mutations() {
            assert_eq!(
                crate::source_main_active_service_wal::ActiveServiceWalBlobV1::of(mutations)
                    .unwrap(),
                expected.native_mutations.clone().unwrap()
            );
            assert_eq!(
                crate::source_main_active_service_wal::ActiveServiceWalBlobV1::of(
                    actual.native_candidates().unwrap()
                )
                .unwrap(),
                expected.native_candidates.clone().unwrap()
            );
            appends += 1;
        }
        let _raw =
            NativeMainActiveServiceActionV1::from_canonical_action_bytes(actual.raw_action())
                .unwrap();
        assert!(!actual.verified_history_before().is_empty());
        assert!(!actual.verified_history_post().is_empty());
    }
    assert!(
        appends >= 2,
        "actual catchup and live native writes were recorded"
    );
    assert_eq!(reader.pin().unwrap(), pin);
}
