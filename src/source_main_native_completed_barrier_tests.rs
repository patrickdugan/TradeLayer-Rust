use super::*;
use crate::source_compact_index_v2::{
    NativePreparedValidityCandidateV1 as Candidate, NativePreparedValidityOperationV1,
    NativePreparedValidityQueryV1,
};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("NATIVE_SUPERSEDING_TMP")
            .map_or_else(std::env::temp_dir, PathBuf::from);
        fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "completed-barrier-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(root) = self.root.canonicalize() {
            assert_eq!(root.parent(), Some(self.parent.as_path()));
            let _ = fs::remove_dir_all(root);
        }
    }
}
fn native_ns() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn control_ns() -> Namespace {
    Namespace::new("LTC", false, "consensus").unwrap()
}
fn index(temp: &Temp) -> SourceCompactIndex {
    let mut index = SourceCompactIndex::create(
        &temp.root,
        native_ns(),
        "actual independent metadata context",
    )
    .unwrap();
    index
        .import_row(
            &index.snapshot().unwrap(),
            obj(&[
                ("_id", text("tx-1-input")),
                ("value", obj(&[("payload", text("caller data"))])),
            ]),
        )
        .unwrap();
    index
}
fn write(index: &SourceCompactIndex) -> NativePreparedValidityWriteV1 {
    let NativePreparedValidityQueryV1::Selected(selected) = index
        .prepare_validity_read(
            &index.snapshot().unwrap(),
            text("input"),
            V::number(1.),
            V::Bool(true),
            V::Undefined,
        )
        .unwrap()
    else {
        panic!()
    };
    let NativePreparedValidityOperationV1::Write(write) = selected
        .prepare_write(index, &index.snapshot().unwrap())
        .unwrap()
    else {
        panic!()
    };
    *write
}
fn make_store(temp: &Temp) -> NativeIntentStoreV1 {
    NativeIntentStoreV1::create(
        &temp.root,
        control_ns(),
        obj(&[("profile", text("metadata-only source controls"))]),
        V::Null,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap()
}
fn pending(
    store: &mut NativeIntentStoreV1,
    write: &NativePreparedValidityWriteV1,
) -> NativeCommandIntentPermitV1 {
    let mut permit = store
        .sync_command_intent(
            &store.pin().unwrap(),
            b"retained original actual command bytes",
            V::Null,
            100000,
        )
        .unwrap();
    let candidates = NativeWriteIntentCandidatesV1::from_prepared_append(
        write.append_plan(),
        obj(&[
            ("actualRead", write.durable_read_context()),
            (
                "retainedControl",
                text("unchanged opaque test control observations"),
            ),
        ]),
    )
    .unwrap();
    store.sync_native_intent(&mut permit, &candidates).unwrap();
    permit
}
fn reopened(
    temp: &Temp,
    original: &NativePreparedValidityWriteV1,
    candidate: Candidate,
) -> (
    SourceCompactIndex,
    NativePreparedValidityRecoveryObservationV1,
    NativePreparedValidityWriteV1,
    NativePreparedValiditySupersessionV1,
) {
    let (index, observation) = SourceCompactIndex::reopen_prepared_validity(
        &temp.root,
        native_ns(),
        original,
        candidate,
        Recovery::DiscardIncompleteTail,
    )
    .unwrap();
    let (replacement, mapping) = index
        .supersede_prepared_validity(original, &observation)
        .unwrap();
    (index, observation, replacement, mapping)
}

struct Ready {
    _native: Temp,
    control: Temp,
    index: SourceCompactIndex,
    write: NativePreparedValidityWriteV1,
    store: NativeIntentStoreV1,
    permit: NativeCommandIntentPermitV1,
}
fn ready(explicit: bool) -> Ready {
    let native = Temp::new();
    let control = Temp::new();
    let mut index = index(&native);
    let mut write = write(&index);
    let mut store = make_store(&control);
    let mut permit = pending(&mut store, &write);
    if explicit {
        drop(index);
        let (fresh, observation, replacement, mapping) =
            reopened(&native, &write, Candidate::Before);
        let operation = store
            .prepare_superseding_native_intent(
                &permit,
                &write,
                &fresh,
                &observation,
                &replacement,
                &mapping,
            )
            .unwrap();
        store
            .sync_prepared_superseding_native_intent(&mut permit, &operation)
            .unwrap();
        index = fresh;
        write = replacement;
    }
    index.commit_prepared_validity(&write).unwrap();
    Ready {
        _native: native,
        control,
        index,
        write,
        store,
        permit,
    }
}
fn files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, path: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for item in fs::read_dir(path).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                visit(root, &path, out)
            } else {
                let raw = match fs::read(&path) {
                    Ok(raw) => raw,
                    Err(e) if matches!(e.raw_os_error(), Some(32 | 33)) => {
                        let m = fs::metadata(&path).unwrap();
                        format!("locked:{}:{:?}", m.len(), m.modified().unwrap()).into_bytes()
                    }
                    Err(e) => panic!("{e}"),
                };
                out.push((path.strip_prefix(root).unwrap().into(), raw));
            }
        }
    }
    let mut out = vec![];
    visit(root, root, &mut out);
    out.sort();
    out
}
const ACCEPTED: &[u8] =
    b"actual accepted control frames; metadata barrier fixture, no financial result";
fn context() -> V {
    obj(&[(
        "nativePublicationProfile",
        text("actual sealed barrier test context"),
    )])
}
fn command_ready(explicit: bool) -> Ready {
    let mut r = ready(explicit);
    let operation = r
        .store
        .prepare_native_prefix_ack(&r.permit, &r.index, &r.write)
        .unwrap();
    r.store
        .sync_prepared_native_prefix_ack(&mut r.permit, &operation)
        .unwrap();
    r
}
fn blob_path(r: &Ready, blob: &NativeIntentBlobV1) -> PathBuf {
    r.control
        .root
        .join(BLOBS)
        .join(format!("{}.tlc13", hex(&blob.sha256)))
}
#[test]
fn both_barriers_prepare_read_only_and_unchanged_entries_are_exact_byte_oracles() {
    for explicit in [false, true] {
        let mut r = command_ready(explicit);
        let before = r.store.pin().unwrap();
        let tree = files(&r.control.root);
        let accepted = r
            .store
            .prepare_completed_accepted(&r.permit, ACCEPTED, &context())
            .unwrap();
        assert_eq!(files(&r.control.root), tree);
        assert_eq!(r.store.pin().unwrap(), before);
        assert_eq!(accepted.independently_expected_before(), &before);
        r.store
            .sync_accepted(&mut r.permit, ACCEPTED, context())
            .unwrap();
        assert_eq!(
            &r.store.pin().unwrap(),
            accepted.independently_expected_after()
        );
        assert_eq!(
            r.store.read_blob(accepted.accepted_blob()).unwrap(),
            accepted.accepted_bytes()
        );
        let before = r.store.pin().unwrap();
        let tree = files(&r.control.root);
        let ack = r.store.prepare_completed_command_ack(&r.permit).unwrap();
        assert_eq!(files(&r.control.root), tree);
        assert_eq!(r.store.pin().unwrap(), before);
        r.store.sync_ack(&mut r.permit).unwrap();
        assert_eq!(&r.store.pin().unwrap(), ack.independently_expected_after());
        assert_eq!(
            r.store.read_blob(ack.acknowledgement()).unwrap(),
            ack.acknowledgement_bytes()
        );
        assert_eq!(
            r.store.state().unwrap().get("ack"),
            blob_value(ack.acknowledgement())
        );
    }
}
#[test]
fn normal_prepared_final_barriers_preserve_reservation_until_ack_and_refuse_duplicates() {
    for explicit in [false, true] {
        let mut r = command_ready(explicit);
        let native = r.index.snapshot().unwrap();
        let accepted = r
            .store
            .prepare_completed_accepted(&r.permit, ACCEPTED, &context())
            .unwrap();
        r.store
            .sync_prepared_completed_accepted(&mut r.permit, &accepted)
            .unwrap();
        assert!(r.permit.reservation.capacity() >= r.permit.receipt_budget);
        let tree = files(&r.control.root);
        assert!(r
            .store
            .sync_prepared_completed_accepted(&mut r.permit, &accepted)
            .is_err());
        assert_eq!(files(&r.control.root), tree);
        let ack = r.store.prepare_completed_command_ack(&r.permit).unwrap();
        r.store
            .sync_prepared_completed_command_ack(&mut r.permit, &ack)
            .unwrap();
        assert_eq!(r.permit.reservation.capacity(), 0);
        let tree = files(&r.control.root);
        assert!(r
            .store
            .sync_prepared_completed_command_ack(&mut r.permit, &ack)
            .is_err());
        assert_eq!(files(&r.control.root), tree);
        assert_eq!(r.index.snapshot().unwrap(), native);
        let pin = r.store.pin().unwrap();
        drop(r.store);
        if explicit {
            assert!(NativeIntentStoreV1::open(
                &r.control.root,
                control_ns(),
                &pin,
                SourceDurableFilesystem::FileSyncOnly
            )
            .is_err());
            NativeIntentStoreV1::open_superseding_profile(
                &r.control.root,
                control_ns(),
                &pin,
                SourceDurableFilesystem::FileSyncOnly,
            )
            .unwrap();
        } else {
            NativeIntentStoreV1::open(
                &r.control.root,
                control_ns(),
                &pin,
                SourceDurableFilesystem::FileSyncOnly,
            )
            .unwrap();
        }
    }
}
#[test]
fn actual_synced_accepted_lost_ack_rebinds_only_retained_permit_and_keeps_reservation() {
    for explicit in [false, true] {
        let mut r = command_ready(explicit);
        let accepted = r
            .store
            .prepare_completed_accepted(&r.permit, ACCEPTED, &context())
            .unwrap();
        let old = r.permit.pin.clone();
        let native = r.index.snapshot().unwrap();
        r.store.journal.test_fail_after_sync();
        assert!(r
            .store
            .sync_prepared_completed_accepted(&mut r.permit, &accepted)
            .is_err());
        assert_eq!(r.permit.pin, old);
        assert!(r.permit.reservation.capacity() >= r.permit.receipt_budget);
        drop(r.store);
        let mut opened = NativeIntentStoreV1::open_synced_completed_accepted(
            &r.control.root,
            control_ns(),
            &accepted,
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap();
        let mut foreign = command_ready(explicit);
        let foreign_pin = foreign.permit.pin.clone();
        assert!(opened
            .recover_synced_completed_accepted(&accepted, &mut foreign.permit)
            .is_err());
        assert_eq!(foreign.permit.pin, foreign_pin);
        opened
            .recover_synced_completed_accepted(&accepted, &mut r.permit)
            .unwrap();
        assert!(r.permit.reservation.capacity() >= r.permit.receipt_budget);
        assert!(opened
            .recover_synced_completed_accepted(&accepted, &mut r.permit)
            .is_err());
        let ack = opened.prepare_completed_command_ack(&r.permit).unwrap();
        opened
            .sync_prepared_completed_command_ack(&mut r.permit, &ack)
            .unwrap();
        assert_eq!(r.permit.reservation.capacity(), 0);
        assert_eq!(r.index.snapshot().unwrap(), native);
    }
}

#[test]
fn actual_synced_command_ack_lost_ack_clears_reservation_only_after_exact_post_rebind() {
    for explicit in [false, true] {
        let mut r = command_ready(explicit);
        r.store
            .sync_accepted(&mut r.permit, ACCEPTED, context())
            .unwrap();
        let ack = r.store.prepare_completed_command_ack(&r.permit).unwrap();
        let old = r.permit.pin.clone();
        r.store.journal.test_fail_after_sync();
        assert!(r
            .store
            .sync_prepared_completed_command_ack(&mut r.permit, &ack)
            .is_err());
        assert_eq!(r.permit.pin, old);
        assert!(r.permit.reservation.capacity() >= r.permit.receipt_budget);
        drop(r.store);
        let opened = NativeIntentStoreV1::open_synced_completed_command_ack(
            &r.control.root,
            control_ns(),
            &ack,
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap();
        let mut foreign = command_ready(explicit);
        let foreign_pin = foreign.permit.pin.clone();
        assert!(opened
            .recover_synced_completed_command_ack(&ack, &mut foreign.permit)
            .is_err());
        assert_eq!(foreign.permit.pin, foreign_pin);
        assert!(foreign.permit.reservation.capacity() >= foreign.permit.receipt_budget);
        opened
            .recover_synced_completed_command_ack(&ack, &mut r.permit)
            .unwrap();
        assert_eq!(r.permit.reservation.capacity(), 0);
        assert_eq!(&r.permit.pin, ack.independently_expected_after());
        assert!(opened
            .recover_synced_completed_command_ack(&ack, &mut r.permit)
            .is_err());
    }
}

#[test]
fn missing_accepted_or_ack_blob_blocks_post_rebind_and_preserves_original_permit() {
    for explicit in [false, true] {
        for at_ack in [false, true] {
            let mut r = command_ready(explicit);
            let accepted = r
                .store
                .prepare_completed_accepted(&r.permit, ACCEPTED, &context())
                .unwrap();
            if at_ack {
                r.store
                    .sync_prepared_completed_accepted(&mut r.permit, &accepted)
                    .unwrap();
            }
            let ack = if at_ack {
                Some(r.store.prepare_completed_command_ack(&r.permit).unwrap())
            } else {
                None
            };
            let old = r.permit.pin.clone();
            r.store.journal.test_fail_after_sync();
            if let Some(ack) = &ack {
                assert!(r
                    .store
                    .sync_prepared_completed_command_ack(&mut r.permit, ack)
                    .is_err());
            } else {
                assert!(r
                    .store
                    .sync_prepared_completed_accepted(&mut r.permit, &accepted)
                    .is_err());
            }
            let target = blob_path(
                &r,
                if let Some(ack) = &ack {
                    ack.acknowledgement()
                } else {
                    accepted.accepted_blob()
                },
            );
            drop(r.store);
            let raw = fs::read(&target).unwrap();
            fs::remove_file(&target).unwrap();
            let tree = files(&r.control.root);
            let opened = if let Some(ack) = &ack {
                NativeIntentStoreV1::open_synced_completed_command_ack(
                    &r.control.root,
                    control_ns(),
                    ack,
                    SourceDurableFilesystem::FileSyncOnly,
                )
            } else {
                NativeIntentStoreV1::open_synced_completed_accepted(
                    &r.control.root,
                    control_ns(),
                    &accepted,
                    SourceDurableFilesystem::FileSyncOnly,
                )
            };
            assert!(opened.is_err());
            assert_eq!(r.permit.pin, old);
            assert_eq!(files(&r.control.root), tree);
            assert!(r.permit.reservation.capacity() >= r.permit.receipt_budget);
            fs::write(target, raw).unwrap();
            if let Some(ack) = &ack {
                let opened = NativeIntentStoreV1::open_synced_completed_command_ack(
                    &r.control.root,
                    control_ns(),
                    ack,
                    SourceDurableFilesystem::FileSyncOnly,
                )
                .unwrap();
                opened
                    .recover_synced_completed_command_ack(ack, &mut r.permit)
                    .unwrap();
            } else {
                let opened = NativeIntentStoreV1::open_synced_completed_accepted(
                    &r.control.root,
                    control_ns(),
                    &accepted,
                    SourceDurableFilesystem::FileSyncOnly,
                )
                .unwrap();
                opened
                    .recover_synced_completed_accepted(&accepted, &mut r.permit)
                    .unwrap();
            }
        }
    }
}

#[test]
fn same_sequence_valid_journal_unknown_state_and_wrong_namespace_are_never_promoted() {
    for explicit in [false, true] {
        let mut r = command_ready(explicit);
        let accepted = r
            .store
            .prepare_completed_accepted(&r.permit, ACCEPTED, &context())
            .unwrap();
        r.store
            .write_state(obj(&[
                ("kind", text("UnknownFinishedCommand")),
                ("accepted", V::Null),
            ]))
            .unwrap();
        assert_eq!(
            r.store.pin().unwrap().sequence,
            accepted.independently_expected_after().sequence
        );
        let old = r.permit.pin.clone();
        drop(r.store);
        let tree = files(&r.control.root);
        assert!(NativeIntentStoreV1::open_synced_completed_accepted(
            &r.control.root,
            control_ns(),
            &accepted,
            SourceDurableFilesystem::FileSyncOnly
        )
        .is_err());
        assert!(NativeIntentStoreV1::open_synced_completed_accepted(
            &r.control.root,
            native_ns(),
            &accepted,
            SourceDurableFilesystem::FileSyncOnly
        )
        .is_err());
        assert_eq!(r.permit.pin, old);
        assert_eq!(files(&r.control.root), tree);
    }
}
#[test]
fn real_healthy_blob_failures_retry_exact_prepared_barriers_on_same_store() {
    for explicit in [false, true] {
        let mut r = command_ready(explicit);
        let accepted = r
            .store
            .prepare_completed_accepted(&r.permit, ACCEPTED, &context())
            .unwrap();
        let blocker = blob_path(&r, accepted.accepted_blob());
        fs::create_dir(&blocker).unwrap();
        assert!(r
            .store
            .sync_prepared_completed_accepted(&mut r.permit, &accepted)
            .is_err());
        assert_eq!(
            &r.store.pin().unwrap(),
            accepted.independently_expected_before()
        );
        assert_eq!(&r.permit.pin, accepted.independently_expected_before());
        fs::remove_dir(blocker).unwrap();
        r.store
            .sync_prepared_completed_accepted(&mut r.permit, &accepted)
            .unwrap();
        let ack = r.store.prepare_completed_command_ack(&r.permit).unwrap();
        let blocker = blob_path(&r, ack.acknowledgement());
        fs::create_dir(&blocker).unwrap();
        assert!(r
            .store
            .sync_prepared_completed_command_ack(&mut r.permit, &ack)
            .is_err());
        assert_eq!(&r.store.pin().unwrap(), ack.independently_expected_before());
        assert_eq!(&r.permit.pin, ack.independently_expected_before());
        assert!(r.permit.reservation.capacity() >= r.permit.receipt_budget);
        fs::remove_dir(blocker).unwrap();
        r.store
            .sync_prepared_completed_command_ack(&mut r.permit, &ack)
            .unwrap();
        assert_eq!(r.permit.reservation.capacity(), 0);
    }
}
#[test]
fn torn_final_barrier_tails_refuse_expected_post_without_repair_or_permit_mutation() {
    for explicit in [false, true] {
        for at_ack in [false, true] {
            let mut r = command_ready(explicit);
            let accepted = r
                .store
                .prepare_completed_accepted(&r.permit, ACCEPTED, &context())
                .unwrap();
            if at_ack {
                r.store
                    .sync_prepared_completed_accepted(&mut r.permit, &accepted)
                    .unwrap();
            }
            let ack = if at_ack {
                Some(r.store.prepare_completed_command_ack(&r.permit).unwrap())
            } else {
                None
            };
            let before = r.permit.pin.clone();
            r.store.journal.test_fail_after_partial_write();
            let stopped = if let Some(ack) = &ack {
                r.store
                    .sync_prepared_completed_command_ack(&mut r.permit, ack)
            } else {
                r.store
                    .sync_prepared_completed_accepted(&mut r.permit, &accepted)
            };
            assert!(stopped.is_err());
            assert_eq!(r.permit.pin, before);
            drop(r.store);
            let tree = files(&r.control.root);
            let opened = if let Some(ack) = &ack {
                NativeIntentStoreV1::open_synced_completed_command_ack(
                    &r.control.root,
                    control_ns(),
                    ack,
                    SourceDurableFilesystem::FileSyncOnly,
                )
            } else {
                NativeIntentStoreV1::open_synced_completed_accepted(
                    &r.control.root,
                    control_ns(),
                    &accepted,
                    SourceDurableFilesystem::FileSyncOnly,
                )
            };
            assert!(opened.is_err());
            assert_eq!(files(&r.control.root), tree);
            assert_eq!(r.permit.pin, before);
            assert!(r.permit.reservation.capacity() >= r.permit.receipt_budget);
        }
    }
}
#[test]
fn foreign_permits_wrong_phase_and_budget_refuse_without_writes() {
    for explicit in [false, true] {
        let mut r = command_ready(explicit);
        let mut foreign = command_ready(explicit);
        assert!(r.store.prepare_completed_command_ack(&r.permit).is_err());
        let excessive = vec![0; r.permit.receipt_budget + 1];
        assert!(r
            .store
            .prepare_completed_accepted(&r.permit, &excessive, &context())
            .is_err());
        let accepted = r
            .store
            .prepare_completed_accepted(&r.permit, ACCEPTED, &context())
            .unwrap();
        let tree = files(&foreign.control.root);
        let pin = foreign.permit.pin.clone();
        assert!(foreign
            .store
            .sync_prepared_completed_accepted(&mut foreign.permit, &accepted)
            .is_err());
        assert_eq!(files(&foreign.control.root), tree);
        assert_eq!(foreign.permit.pin, pin);
        r.store
            .sync_prepared_completed_accepted(&mut r.permit, &accepted)
            .unwrap();
        let ack = r.store.prepare_completed_command_ack(&r.permit).unwrap();
        assert!(foreign
            .store
            .sync_prepared_completed_command_ack(&mut foreign.permit, &ack)
            .is_err());
        assert_eq!(files(&foreign.control.root), tree);
        assert_eq!(foreign.permit.pin, pin);
    }
}
