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
            "prepared-prefix-ack-{}-{}",
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
#[test]
fn prepared_ack_is_read_only_and_both_existing_ack_layouts_stay_byte_exact() {
    for explicit in [false, true] {
        let mut r = ready(explicit);
        let before = r.store.pin().unwrap();
        let native = r.index.snapshot().unwrap();
        let tree = files(&r.control.root);
        let prepared = r
            .store
            .prepare_native_prefix_ack(&r.permit, &r.index, &r.write)
            .unwrap();
        assert_eq!(prepared.profile(), NATIVE_PREPARED_PREFIX_ACK_PROFILE);
        assert_eq!(prepared.independently_expected_before(), &before);
        assert_eq!(files(&r.control.root), tree);
        assert_eq!(r.store.pin().unwrap(), before);
        // Execute the distinct, unchanged existing ACK entry as a byte oracle.
        if explicit {
            r.store
                .sync_superseding_native_prefix_ack(&mut r.permit, &r.index, &r.write)
                .unwrap();
        } else {
            r.store
                .sync_native_prefix_ack(
                    &mut r.permit,
                    &r.index.native_observed_append_binding().unwrap(),
                )
                .unwrap();
        }
        assert_eq!(
            &r.store.pin().unwrap(),
            prepared.independently_expected_after()
        );
        assert_eq!(r.store.state().unwrap(), prepared.state);
        assert_eq!(
            r.store.read_blob(prepared.acknowledged_prefix()).unwrap(),
            prepared.raw
        );
        assert_eq!(r.index.snapshot().unwrap(), native);
    }
}
#[test]
fn normal_prepared_ack_sync_updates_only_the_retained_permit_and_duplicates_refuse() {
    for explicit in [false, true] {
        let mut r = ready(explicit);
        let native = r.index.snapshot().unwrap();
        let state = r.index.native_complete_metadata_state().unwrap();
        let prepared = r
            .store
            .prepare_native_prefix_ack(&r.permit, &r.index, &r.write)
            .unwrap();
        let old_intent = r.permit.pending_native.as_ref().unwrap().0.clone();
        assert_eq!(prepared.original_native_intent(), &old_intent);
        r.store
            .sync_prepared_native_prefix_ack(&mut r.permit, &prepared)
            .unwrap();
        assert_eq!(&r.permit.pin, prepared.independently_expected_after());
        assert!(r.permit.pending_native.is_none());
        assert_eq!(r.permit.native_prefix, Some(prepared.prefix.clone()));
        let pin = r.store.pin().unwrap();
        let tree = files(&r.control.root);
        assert!(r
            .store
            .sync_prepared_native_prefix_ack(&mut r.permit, &prepared)
            .is_err());
        assert_eq!(r.store.pin().unwrap(), pin);
        assert_eq!(files(&r.control.root), tree);
        assert_eq!(r.index.snapshot().unwrap(), native);
        assert_eq!(r.index.native_complete_metadata_state().unwrap(), state);
    }
}
#[test]
fn synced_ack_lost_ack_recovers_only_same_retained_permit_with_exact_expected_post() {
    for explicit in [false, true] {
        let Ready {
            _native,
            control,
            index,
            write,
            mut store,
            mut permit,
        } = ready(explicit);
        let prepared = store
            .prepare_native_prefix_ack(&permit, &index, &write)
            .unwrap();
        let native = index.snapshot().unwrap();
        let old_pin = permit.pin.clone();
        store.journal.test_fail_after_sync();
        assert!(store
            .sync_prepared_native_prefix_ack(&mut permit, &prepared)
            .is_err());
        assert!(matches!(store.pin(), Err(NativeIntentStoreError::Poisoned)));
        assert_eq!(permit.pin, old_pin);
        assert!(permit.pending_native.is_some());
        drop(store);
        let tree = files(&control.root);
        assert!(NativeIntentStoreV1::open(
            &control.root,
            control_ns(),
            prepared.independently_expected_before(),
            SourceDurableFilesystem::FileSyncOnly
        )
        .is_err());
        assert_eq!(files(&control.root), tree);
        if explicit {
            assert!(matches!(
                NativeIntentStoreV1::open(
                    &control.root,
                    control_ns(),
                    prepared.independently_expected_after(),
                    SourceDurableFilesystem::FileSyncOnly
                ),
                Err(NativeIntentStoreError::WrongPhase)
            ));
        }
        let mut store = NativeIntentStoreV1::open_synced_native_prefix_ack(
            &control.root,
            control_ns(),
            &prepared,
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap();
        assert!(store
            .sync_accepted(&mut permit, b"old token must refuse", V::Null)
            .is_err());
        store
            .recover_synced_native_prefix_ack(&prepared, &mut permit)
            .unwrap();
        let pin = permit.pin.clone();
        assert!(permit.pending_native.is_none());
        assert!(store
            .recover_synced_native_prefix_ack(&prepared, &mut permit)
            .is_err());
        assert_eq!(permit.pin, pin);
        assert_eq!(index.snapshot().unwrap(), native);
        store
            .sync_accepted(
                &mut permit,
                b"bounded metadata-only accepted observations",
                V::Null,
            )
            .unwrap();
        store.sync_ack(&mut permit).unwrap();
        assert_eq!(store.state().unwrap().get("kind"), text("Idle"));
    }
}
#[test]
fn real_before_state_blob_io_failure_keeps_old_permit_and_allows_exact_retry() {
    for explicit in [false, true] {
        let mut r = ready(explicit);
        let prepared = r
            .store
            .prepare_native_prefix_ack(&r.permit, &r.index, &r.write)
            .unwrap();
        let before = r.store.pin().unwrap();
        let blocker = r
            .control
            .root
            .join(BLOBS)
            .join(format!("{}.tlc13", hex(&prepared.prefix.sha256)));
        fs::create_dir(&blocker).unwrap();
        assert!(r
            .store
            .sync_prepared_native_prefix_ack(&mut r.permit, &prepared)
            .is_err());
        assert_eq!(r.store.pin().unwrap(), before);
        assert_eq!(r.permit.pin, before);
        assert!(r.permit.pending_native.is_some());
        fs::remove_dir(blocker).unwrap();
        r.store
            .sync_prepared_native_prefix_ack(&mut r.permit, &prepared)
            .unwrap();
    }
}
#[test]
fn actual_torn_ack_control_tail_refuses_both_expected_pins_without_any_repair() {
    for explicit in [false, true] {
        let Ready {
            _native,
            control,
            index,
            write,
            mut store,
            mut permit,
        } = ready(explicit);
        let native = index.snapshot().unwrap();
        let prepared = store
            .prepare_native_prefix_ack(&permit, &index, &write)
            .unwrap();
        store.journal.test_fail_after_partial_write();
        assert!(store
            .sync_prepared_native_prefix_ack(&mut permit, &prepared)
            .is_err());
        drop(store);
        let tree = files(&control.root);
        assert!(NativeIntentStoreV1::open_synced_native_prefix_ack(
            &control.root,
            control_ns(),
            &prepared,
            SourceDurableFilesystem::FileSyncOnly
        )
        .is_err());
        assert_eq!(files(&control.root), tree);
        assert!(NativeIntentStoreV1::open(
            &control.root,
            control_ns(),
            prepared.independently_expected_before(),
            SourceDurableFilesystem::FileSyncOnly
        )
        .is_err());
        assert_eq!(files(&control.root), tree);
        assert_eq!(index.snapshot().unwrap(), native);
        assert!(permit.pending_native.is_some());
    }
}

#[test]
fn native_before_and_actual_compaction_intermediate_cannot_prepare_ack() {
    for intermediate in [false, true] {
        let native = Temp::new();
        let control = Temp::new();
        let mut index = index(&native);
        if intermediate {
            index
                .configure_compaction(crate::compact_journal::CompactionConfig {
                    target_log_bytes: 65536,
                    retained_ancestors: 0,
                })
                .unwrap();
            index
                .import_row(
                    &index.snapshot().unwrap(),
                    obj(&[
                        ("_id", text("unrelated")),
                        ("padding", text(&"x".repeat(8000))),
                    ]),
                )
                .unwrap();
        }
        let original = if intermediate {
            let mut actual = None;
            for _ in 0..2000 {
                let candidate = write(&index);
                if candidate.append_plan().intermediate().is_some() {
                    actual = Some(candidate);
                    break;
                }
                index.test_supersession_directory_record();
            }
            actual.expect("real compaction required")
        } else {
            write(&index)
        };
        let mut store = make_store(&control);
        let permit = pending(&mut store, &original);
        if intermediate {
            index.test_supersession_compaction_fault("after-current-sync");
            assert!(index.commit_prepared_validity(&original).is_err());
            drop(index);
            index = SourceCompactIndex::reopen_prepared_validity(
                &native.root,
                native_ns(),
                &original,
                Candidate::Intermediate,
                Recovery::RejectIncompleteTail,
            )
            .unwrap()
            .0;
        }
        let pin = store.pin().unwrap();
        let tree = files(&control.root);
        assert!(store
            .prepare_native_prefix_ack(&permit, &index, &original)
            .is_err());
        assert_eq!(store.pin().unwrap(), pin);
        assert_eq!(files(&control.root), tree);
    }
}
#[test]
fn foreign_write_permit_and_prepared_control_operation_refuse_without_writes() {
    let mut a = ready(false);
    let mut b = ready(false);
    let pin = a.store.pin().unwrap();
    let tree = files(&a.control.root);
    assert!(a
        .store
        .prepare_native_prefix_ack(&a.permit, &b.index, &a.write)
        .is_err());
    assert!(a
        .store
        .prepare_native_prefix_ack(&b.permit, &a.index, &a.write)
        .is_err());
    let prepared = b
        .store
        .prepare_native_prefix_ack(&b.permit, &b.index, &b.write)
        .unwrap();
    assert!(a
        .store
        .sync_prepared_native_prefix_ack(&mut a.permit, &prepared)
        .is_err());
    assert_eq!(a.store.pin().unwrap(), pin);
    assert_eq!(files(&a.control.root), tree);
    b.store.journal.test_fail_after_sync();
    assert!(b
        .store
        .sync_prepared_native_prefix_ack(&mut b.permit, &prepared)
        .is_err());
    drop(b.store);
    let opened = NativeIntentStoreV1::open_synced_native_prefix_ack(
        &b.control.root,
        control_ns(),
        &prepared,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let a_pin = a.permit.pin.clone();
    assert!(opened
        .recover_synced_native_prefix_ack(&prepared, &mut a.permit)
        .is_err());
    assert_eq!(a.permit.pin, a_pin);
    assert!(a.permit.pending_native.is_some());
}
#[test]
fn corrupted_prefix_or_missing_predecessor_prevents_rebind_of_the_retained_permit() {
    for explicit in [false, true] {
        let Ready {
            _native,
            control,
            index,
            write,
            mut store,
            mut permit,
        } = ready(explicit);
        let prepared = store
            .prepare_native_prefix_ack(&permit, &index, &write)
            .unwrap();
        store.journal.test_fail_after_sync();
        assert!(store
            .sync_prepared_native_prefix_ack(&mut permit, &prepared)
            .is_err());
        drop(store);
        let old_permit = permit.pin.clone();
        let target = control
            .root
            .join(BLOBS)
            .join(format!("{}.tlc13", hex(&prepared.prefix.sha256)));
        let raw = fs::read(&target).unwrap();
        let mut corrupt = raw.clone();
        corrupt[0] ^= 1;
        fs::write(&target, &corrupt).unwrap();
        let tree = files(&control.root);
        assert!(NativeIntentStoreV1::open_synced_native_prefix_ack(
            &control.root,
            control_ns(),
            &prepared,
            SourceDurableFilesystem::FileSyncOnly
        )
        .is_err());
        assert_eq!(files(&control.root), tree);
        assert_eq!(permit.pin, old_permit);
        fs::write(&target, raw).unwrap();
        let predecessor = control
            .root
            .join(BLOBS)
            .join(format!("{}.tlc13", hex(&prepared.pending.0.sha256)));
        let original = fs::read(&predecessor).unwrap();
        fs::remove_file(&predecessor).unwrap();
        assert!(NativeIntentStoreV1::open_synced_native_prefix_ack(
            &control.root,
            control_ns(),
            &prepared,
            SourceDurableFilesystem::FileSyncOnly
        )
        .is_err());
        assert_eq!(permit.pin, old_permit);
        fs::write(predecessor, original).unwrap();
        let store = NativeIntentStoreV1::open_synced_native_prefix_ack(
            &control.root,
            control_ns(),
            &prepared,
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap();
        store
            .recover_synced_native_prefix_ack(&prepared, &mut permit)
            .unwrap();
    }
}
#[test]
fn same_sequence_valid_crc_unknown_control_state_is_not_promoted_by_recovery() {
    let Ready {
        _native,
        control,
        index,
        write,
        mut store,
        mut permit,
    } = ready(false);
    let prepared = store
        .prepare_native_prefix_ack(&permit, &index, &write)
        .unwrap();
    let path = store.journal_path().to_owned();
    store.journal.test_fail_after_sync();
    assert!(store
        .sync_prepared_native_prefix_ack(&mut permit, &prepared)
        .is_err());
    drop(store);
    let alternative = Temp::new();
    let (mut raw, _) = Journal::open(
        &alternative.root,
        control_ns(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    let unknown = bytes(&obj(&[("kind", text("unknown repaired control state"))])).unwrap();
    raw.commit(vec![
        Mutation::Put {
            key: CONTEXT.into(),
            value: prepared.after.context.clone(),
        },
        Mutation::Put {
            key: STATE.into(),
            value: unknown.clone(),
        },
    ])
    .unwrap();
    for _ in 1..prepared.after.sequence {
        raw.commit(vec![Mutation::Put {
            key: STATE.into(),
            value: unknown.clone(),
        }])
        .unwrap();
    }
    assert_eq!(raw.sequence().unwrap(), prepared.after.sequence);
    let replacement = raw.path().to_owned();
    drop(raw);
    fs::write(&path, fs::read(replacement).unwrap()).unwrap();
    let tree = files(&control.root);
    assert!(NativeIntentStoreV1::open_synced_native_prefix_ack(
        &control.root,
        control_ns(),
        &prepared,
        SourceDurableFilesystem::FileSyncOnly
    )
    .is_err());
    assert_eq!(files(&control.root), tree);
    assert!(permit.pending_native.is_some());
}

#[test]
fn later_original_native_intent_inside_explicit_store_profile_keeps_its_original_ack_layout() {
    let Ready {
        _native,
        control,
        mut index,
        write,
        mut store,
        mut permit,
    } = ready(true);
    store
        .sync_superseding_native_prefix_ack(&mut permit, &index, &write)
        .unwrap();
    let next = write_next(&index);
    let facts = NativeWriteIntentCandidatesV1::from_prepared_append(
        next.append_plan(),
        next.durable_read_context(),
    )
    .unwrap();
    store.sync_native_intent(&mut permit, &facts).unwrap();
    assert_eq!(store.state().unwrap().get("kind"), text("NativeIntent"));
    assert_eq!(
        store.state().unwrap().get("operationProfile"),
        text(NATIVE_SUPERSEDING_INTENT_PROFILE)
    );
    index.commit_prepared_validity(&next).unwrap();
    let prepared = store
        .prepare_native_prefix_ack(&permit, &index, &next)
        .unwrap();
    assert_eq!(parse(&prepared.raw).unwrap().get("profile"), V::Undefined);
    assert!(prepared.superseding_profile);
    store.journal.test_fail_after_sync();
    assert!(store
        .sync_prepared_native_prefix_ack(&mut permit, &prepared)
        .is_err());
    drop(store);
    assert!(NativeIntentStoreV1::open(
        &control.root,
        control_ns(),
        prepared.independently_expected_after(),
        SourceDurableFilesystem::FileSyncOnly
    )
    .is_err());
    let store = NativeIntentStoreV1::open_synced_native_prefix_ack(
        &control.root,
        control_ns(),
        &prepared,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    store
        .recover_synced_native_prefix_ack(&prepared, &mut permit)
        .unwrap();
}
fn write_next(index: &SourceCompactIndex) -> NativePreparedValidityWriteV1 {
    write(index)
}
