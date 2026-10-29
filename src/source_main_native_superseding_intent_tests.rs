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
            "superseding-{}-{}",
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
#[test]
fn before_supersession_syncs_lineage_before_append_and_only_post_can_ack() {
    let control = Temp::new();
    let native = Temp::new();
    let index = index(&native);
    let original = write(&index);
    let mut store = make_store(&control);
    let mut permit = pending(&mut store, &original);
    drop(index);
    let (mut index, observation, replacement, mapping) =
        reopened(&native, &original, Candidate::Before);
    let before = store.pin().unwrap();
    let native_before = index.snapshot().unwrap();
    let operation = store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping,
        )
        .unwrap();
    assert_eq!(store.pin().unwrap(), before);
    assert_eq!(index.snapshot().unwrap(), native_before);
    assert_eq!(operation.profile(), NATIVE_SUPERSEDING_INTENT_PROFILE);
    assert_eq!(operation.independently_expected_before(), &before);
    assert_eq!(
        operation
            .replacement_candidates()
            .exact_prepared_mutations(),
        permit
            .pending_native
            .as_ref()
            .unwrap()
            .1
            .exact_prepared_mutations()
    );
    assert_eq!(
        operation
            .replacement_candidates()
            .facts
            .get("readContext")
            .get("retainedControl"),
        text("unchanged opaque test control observations")
    );
    store
        .sync_prepared_superseding_native_intent(&mut permit, &operation)
        .unwrap();
    assert_eq!(
        &store.pin().unwrap(),
        operation.independently_expected_after()
    );
    assert_eq!(store.state().unwrap().get("kind"), text(SUPERSEDING_KIND));
    assert_eq!(index.snapshot().unwrap(), native_before);
    assert!(matches!(
        store.sync_native_prefix_ack(
            &mut permit,
            &index.native_observed_append_binding().unwrap()
        ),
        Err(NativeIntentStoreError::WrongPhase)
    ));
    assert!(store
        .sync_superseding_native_prefix_ack(&mut permit, &index, &replacement)
        .is_err());
    index.commit_prepared_validity(&replacement).unwrap();
    store
        .sync_superseding_native_prefix_ack(&mut permit, &index, &replacement)
        .unwrap();
    assert!(permit.pending_native.is_none());
    assert_eq!(store.state().unwrap().get("kind"), text("CommandIntent"));
    assert_eq!(
        store.state().unwrap().get("operationProfile"),
        text(NATIVE_SUPERSEDING_INTENT_PROFILE)
    );
    store
        .sync_accepted(
            &mut permit,
            b"actual bounded accepted control observations",
            V::Null,
        )
        .unwrap();
    store.sync_ack(&mut permit).unwrap();
    let pin = store.pin().unwrap();
    drop(store);
    assert!(matches!(
        NativeIntentStoreV1::open(
            &control.root,
            control_ns(),
            &pin,
            SourceDurableFilesystem::FileSyncOnly
        ),
        Err(NativeIntentStoreError::WrongPhase)
    ));
    let store = NativeIntentStoreV1::open_superseding_profile(
        &control.root,
        control_ns(),
        &pin,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    assert_eq!(store.state().unwrap().get("kind"), text("Idle"));
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
fn preparation_has_no_writes_and_duplicate_sync_retains_exact_pending_prefix() {
    let control = Temp::new();
    let native = Temp::new();
    let index = index(&native);
    let original = write(&index);
    let mut store = make_store(&control);
    let mut permit = pending(&mut store, &original);
    drop(index);
    let (index, observation, replacement, mapping) =
        reopened(&native, &original, Candidate::Before);
    let files_before = files(&control.root);
    let native_pin = index.snapshot().unwrap();
    let control_pin = store.pin().unwrap();
    let operation = store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping,
        )
        .unwrap();
    assert_eq!(files(&control.root), files_before);
    assert_eq!(store.pin().unwrap(), control_pin);
    assert_eq!(index.snapshot().unwrap(), native_pin);
    let duplicate = store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping,
        )
        .unwrap();
    assert_eq!(
        duplicate.independently_expected_after(),
        operation.independently_expected_after()
    );
    store
        .sync_prepared_superseding_native_intent(&mut permit, &operation)
        .unwrap();
    let after = store.pin().unwrap();
    let files_after = files(&control.root);
    assert!(store
        .sync_prepared_superseding_native_intent(&mut permit, &duplicate)
        .is_err());
    assert_eq!(store.pin().unwrap(), after);
    assert_eq!(files(&control.root), files_after);
    assert!(store
        .sync_accepted(&mut permit, b"must stay pending", V::Null)
        .is_err());
    assert!(store.sync_ack(&mut permit).is_err());
    let native_intent = permit.pending_native.as_ref().unwrap().0.clone();
    let body = parse(&store.read_blob(&native_intent).unwrap()).unwrap();
    assert_eq!(
        body.get("supersedes"),
        blob_value(operation.original_native_intent())
    );
    assert_eq!(
        body.get("predecessorControl"),
        superseding_pin_value(&control_pin)
    );
    assert_eq!(body.get("verifiedReopen").get("matched"), text("Before"));
}
#[test]
fn synced_control_lost_ack_requires_explicit_exact_post_open_and_retained_permit() {
    let control = Temp::new();
    let native = Temp::new();
    let index = index(&native);
    let original = write(&index);
    let mut store = make_store(&control);
    let mut permit = pending(&mut store, &original);
    drop(index);
    let (mut index, observation, replacement, mapping) =
        reopened(&native, &original, Candidate::Before);
    let operation = store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping,
        )
        .unwrap();
    let before = permit.pin.clone();
    store.journal.test_fail_after_sync();
    assert!(store
        .sync_prepared_superseding_native_intent(&mut permit, &operation)
        .is_err());
    assert!(matches!(store.pin(), Err(NativeIntentStoreError::Poisoned)));
    assert_eq!(permit.pin, before);
    drop(store);
    let disk = files(&control.root);
    assert!(matches!(
        NativeIntentStoreV1::open(
            &control.root,
            control_ns(),
            operation.independently_expected_after(),
            SourceDurableFilesystem::FileSyncOnly
        ),
        Err(NativeIntentStoreError::WrongPhase)
    ));
    assert_eq!(files(&control.root), disk);
    assert!(NativeIntentStoreV1::open_superseding_profile(
        &control.root,
        control_ns(),
        operation.independently_expected_before(),
        SourceDurableFilesystem::FileSyncOnly
    )
    .is_err());
    assert_eq!(files(&control.root), disk);
    let mut store = NativeIntentStoreV1::open_synced_superseding_intent(
        &control.root,
        control_ns(),
        &operation,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    assert!(
        store
            .sync_prepared_superseding_native_intent(&mut permit, &operation)
            .is_err(),
        "old owner token is never implicitly reused"
    );
    store
        .recover_synced_superseding_native_intent(&operation, &mut permit)
        .unwrap();
    let pin = permit.pin.clone();
    assert!(store
        .recover_synced_superseding_native_intent(&operation, &mut permit)
        .is_err());
    assert_eq!(permit.pin, pin);
    index.commit_prepared_validity(&replacement).unwrap();
    store
        .sync_superseding_native_prefix_ack(&mut permit, &index, &replacement)
        .unwrap();
}
#[test]
fn torn_control_write_never_repairs_or_selects_an_unknown_prefix() {
    let control = Temp::new();
    let native = Temp::new();
    let index = index(&native);
    let original = write(&index);
    let mut store = make_store(&control);
    let mut permit = pending(&mut store, &original);
    drop(index);
    let (index, observation, replacement, mapping) =
        reopened(&native, &original, Candidate::Before);
    let operation = store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping,
        )
        .unwrap();
    store.journal.test_fail_after_partial_write();
    assert!(store
        .sync_prepared_superseding_native_intent(&mut permit, &operation)
        .is_err());
    drop(store);
    let disk = files(&control.root);
    assert!(NativeIntentStoreV1::open(
        &control.root,
        control_ns(),
        operation.independently_expected_before(),
        SourceDurableFilesystem::FileSyncOnly
    )
    .is_err());
    assert_eq!(files(&control.root), disk);
    assert!(NativeIntentStoreV1::open_synced_superseding_intent(
        &control.root,
        control_ns(),
        &operation,
        SourceDurableFilesystem::FileSyncOnly
    )
    .is_err());
    assert_eq!(files(&control.root), disk);
    assert_eq!(index.snapshot().unwrap(), *observation.snapshot());
}
#[test]
fn real_blob_io_failure_retains_original_intent_and_safe_retry_never_appends_native() {
    let control = Temp::new();
    let native = Temp::new();
    let index = index(&native);
    let original = write(&index);
    let mut store = make_store(&control);
    let mut permit = pending(&mut store, &original);
    drop(index);
    let (index, observation, replacement, mapping) =
        reopened(&native, &original, Candidate::Before);
    let operation = store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping,
        )
        .unwrap();
    let before = store.pin().unwrap();
    let blocker = control
        .root
        .join(BLOBS)
        .join(format!("{}.tlc13", hex(&operation.new_intent.sha256)));
    fs::create_dir(&blocker).unwrap();
    assert!(store
        .sync_prepared_superseding_native_intent(&mut permit, &operation)
        .is_err());
    assert_eq!(store.pin().unwrap(), before);
    assert_eq!(permit.pin, before);
    assert_eq!(index.snapshot().unwrap(), *observation.snapshot());
    fs::remove_dir(&blocker).unwrap();
    store
        .sync_prepared_superseding_native_intent(&mut permit, &operation)
        .unwrap();
    assert_eq!(index.snapshot().unwrap(), *observation.snapshot());
}
#[test]
fn foreign_permit_replacement_config_and_stale_owner_are_rejected_before_control_writes() {
    let control = Temp::new();
    let native = Temp::new();
    let index = index(&native);
    let original = write(&index);
    let mut store = make_store(&control);
    let permit = pending(&mut store, &original);
    drop(index);
    let (mut index, observation, replacement, mapping) =
        reopened(&native, &original, Candidate::Before);
    let before = store.pin().unwrap();
    let disk = files(&control.root);
    let foreign = Temp::new();
    let mut other = make_store(&foreign);
    let other_permit = pending(&mut other, &original);
    assert!(store
        .prepare_superseding_native_intent(
            &other_permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping
        )
        .is_err());
    assert_eq!(store.pin().unwrap(), before);
    assert_eq!(files(&control.root), disk);
    index
        .configure_compaction(crate::compact_journal::CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 0,
        })
        .unwrap();
    assert!(store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping
        )
        .is_err());
    assert_eq!(files(&control.root), disk);
    index
        .configure_compaction(replacement.append_plan().configuration().clone())
        .unwrap();
    drop(index);
    let (index, _) = SourceCompactIndex::reopen_prepared_validity(
        &native.root,
        native_ns(),
        &original,
        Candidate::Before,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    assert!(
        store
            .prepare_superseding_native_intent(
                &permit,
                &original,
                &index,
                &observation,
                &replacement,
                &mapping
            )
            .is_err(),
        "reopened owner cannot reuse prior replacement plan token"
    );
    assert_eq!(files(&control.root), disk);
}
#[test]
fn a_second_real_native_partial_failure_can_supersede_without_acknowledging_the_first() {
    let control = Temp::new();
    let native = Temp::new();
    let index = index(&native);
    let original = write(&index);
    let mut store = make_store(&control);
    let mut permit = pending(&mut store, &original);
    drop(index);
    let (mut index, observation, replacement, mapping) =
        reopened(&native, &original, Candidate::Before);
    let first = store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping,
        )
        .unwrap();
    store
        .sync_prepared_superseding_native_intent(&mut permit, &first)
        .unwrap();
    index.test_fail_after_partial_write();
    assert!(index.commit_prepared_validity(&replacement).is_err());
    drop(index);
    let (mut index, observation, next, mapping) =
        reopened(&native, &replacement, Candidate::Before);
    let second = store
        .prepare_superseding_native_intent(
            &permit,
            &replacement,
            &index,
            &observation,
            &next,
            &mapping,
        )
        .unwrap();
    assert_eq!(second.original_native_intent(), &first.new_intent);
    assert_eq!(
        second.replacement_candidates().exact_prepared_mutations(),
        first.replacement_candidates().exact_prepared_mutations()
    );
    store
        .sync_prepared_superseding_native_intent(&mut permit, &second)
        .unwrap();
    index.commit_prepared_validity(&next).unwrap();
    store
        .sync_superseding_native_prefix_ack(&mut permit, &index, &next)
        .unwrap();
    assert_eq!(store.state().unwrap().get("kind"), text("CommandIntent"));
    assert_eq!(
        index.get("tx-1-input").unwrap().unwrap().get("valid"),
        V::Bool(true)
    );
}
#[test]
fn unknown_operation_and_tampered_intent_blob_are_refused_without_state_advancement() {
    let control = Temp::new();
    let native = Temp::new();
    let index = index(&native);
    let original = write(&index);
    let mut store = make_store(&control);
    let mut permit = pending(&mut store, &original);
    drop(index);
    let (index, observation, replacement, mapping) =
        reopened(&native, &original, Candidate::Before);
    let intent = permit.pending_native.as_ref().unwrap().0.clone();
    let path = control
        .root
        .join(BLOBS)
        .join(format!("{}.tlc13", hex(&intent.sha256)));
    let raw = fs::read(&path).unwrap();
    let mut changed = raw.clone();
    changed[0] ^= 1;
    fs::write(&path, changed).unwrap();
    let pin = store.pin().unwrap();
    assert!(store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping
        )
        .is_err());
    assert_eq!(store.pin().unwrap(), pin);
    fs::write(&path, raw).unwrap();
    let mut unknown = store.state().unwrap();
    unknown
        .insert("kind", text("UnknownSupersessionV99"))
        .unwrap();
    unknown
        .insert("operationProfile", text(NATIVE_SUPERSEDING_INTENT_PROFILE))
        .unwrap();
    store.write_state(unknown).unwrap();
    permit.pin = store.pin().unwrap();
    let pin = store.pin().unwrap();
    assert!(store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping
        )
        .is_err());
    drop(store);
    let disk = files(&control.root);
    assert!(NativeIntentStoreV1::open_superseding_profile(
        &control.root,
        control_ns(),
        &pin,
        SourceDurableFilesystem::FileSyncOnly
    )
    .is_err());
    assert_eq!(files(&control.root), disk);
}
fn compaction_write(index: &mut SourceCompactIndex) -> NativePreparedValidityWriteV1 {
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
    for _ in 0..2000 {
        let write = write(index);
        if write.append_plan().intermediate().is_some() {
            return write;
        }
        index.test_supersession_directory_record();
    }
    panic!("actual compaction boundary required")
}
#[test]
fn actual_intermediate_and_orphan_replacement_bind_nondefault_config_and_synced_mapping() {
    for phase in ["after-current-sync", "before-current"] {
        let control = Temp::new();
        let native = Temp::new();
        let mut index = index(&native);
        let original = compaction_write(&mut index);
        let mut store = make_store(&control);
        let mut permit = pending(&mut store, &original);
        index.test_supersession_compaction_fault(phase);
        assert!(index.commit_prepared_validity(&original).is_err());
        drop(index);
        let candidate = if phase == "after-current-sync" {
            Candidate::Intermediate
        } else {
            Candidate::Before
        };
        let (mut index, observation, replacement, mapping) =
            reopened(&native, &original, candidate);
        assert_eq!(
            replacement.append_plan().configuration().target_log_bytes,
            65536
        );
        assert_eq!(
            replacement.append_plan().configuration().retained_ancestors,
            0
        );
        let operation = store
            .prepare_superseding_native_intent(
                &permit,
                &original,
                &index,
                &observation,
                &replacement,
                &mapping,
            )
            .unwrap();
        if phase == "before-current" {
            assert_ne!(
                mapping.original_after().publication.generation,
                mapping.replacement_after().publication.generation
            );
        }
        store
            .sync_prepared_superseding_native_intent(&mut permit, &operation)
            .unwrap();
        index.commit_prepared_validity(&replacement).unwrap();
        store
            .sync_superseding_native_prefix_ack(&mut permit, &index, &replacement)
            .unwrap();
    }
}
#[test]
fn missing_predecessor_link_blocks_post_ack_without_changing_the_pending_control_head() {
    let control = Temp::new();
    let native = Temp::new();
    let index = index(&native);
    let original = write(&index);
    let mut store = make_store(&control);
    let mut permit = pending(&mut store, &original);
    drop(index);
    let (mut index, observation, replacement, mapping) =
        reopened(&native, &original, Candidate::Before);
    let operation = store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping,
        )
        .unwrap();
    store
        .sync_prepared_superseding_native_intent(&mut permit, &operation)
        .unwrap();
    index.commit_prepared_validity(&replacement).unwrap();
    let pin = store.pin().unwrap();
    let predecessor = control
        .root
        .join(BLOBS)
        .join(format!("{}.tlc13", hex(&operation.original_intent.sha256)));
    let raw = fs::read(&predecessor).unwrap();
    fs::remove_file(&predecessor).unwrap();
    assert!(store
        .sync_superseding_native_prefix_ack(&mut permit, &index, &replacement)
        .is_err());
    assert_eq!(store.pin().unwrap(), pin);
    assert!(permit.pending_native.is_some());
    fs::write(predecessor, raw).unwrap();
    store
        .sync_superseding_native_prefix_ack(&mut permit, &index, &replacement)
        .unwrap();
}
#[test]
fn original_or_replacement_post_is_resolve_only_and_never_a_superseding_before_verdict() {
    let control = Temp::new();
    let native = Temp::new();
    let index = index(&native);
    let original = write(&index);
    let mut store = make_store(&control);
    let permit = pending(&mut store, &original);
    drop(index);
    let (mut index, _, replacement, mapping) = reopened(&native, &original, Candidate::Before);
    index.commit_prepared_validity(&replacement).unwrap();
    drop(index);
    let (index, post) = SourceCompactIndex::reopen_prepared_validity(
        &native.root,
        native_ns(),
        &replacement,
        Candidate::Post,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    let pin = store.pin().unwrap();
    let disk = files(&control.root);
    assert!(store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &post,
            &replacement,
            &mapping
        )
        .is_err());
    assert_eq!(store.pin().unwrap(), pin);
    assert_eq!(files(&control.root), disk);
}
#[test]
fn unchanged_legacy_state_and_blob_bytes_match_independent_old_source_executable() {
    const IDLE: &str = include_str!("../fixtures/native_store_13_2_idle.json");
    const COMMAND: &str = include_str!("../fixtures/native_store_13_2_command.json");
    const ACCEPTED: &str = include_str!("../fixtures/native_store_13_2_accepted.json");
    const ACK: &str = include_str!("../fixtures/native_store_13_2_ack.json");
    const BLOBS_TSV: &str = include_str!("../fixtures/native_store_13_2_blobs.tsv");
    let control = Temp::new();
    let mut store = NativeIntentStoreV1::create(
        &control.root,
        control_ns(),
        V::Null,
        V::Null,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    assert_eq!(store.state().unwrap().diagnostic_json().unwrap(), IDLE);
    let mut permit = store
        .sync_command_intent(
            &store.pin().unwrap(),
            b"legacy command bytes",
            V::Bool(false),
            1024,
        )
        .unwrap();
    assert_eq!(store.state().unwrap().diagnostic_json().unwrap(), COMMAND);
    store
        .sync_accepted(&mut permit, b"legacy accepted bytes", V::Bool(true))
        .unwrap();
    assert_eq!(store.state().unwrap().diagnostic_json().unwrap(), ACCEPTED);
    store.sync_ack(&mut permit).unwrap();
    assert_eq!(store.state().unwrap().diagnostic_json().unwrap(), ACK);
    assert_eq!(store.state().unwrap().get("operationProfile"), V::Undefined);
    let pin = store.pin().unwrap();
    drop(store);
    let mut paths = fs::read_dir(control.root.join(BLOBS))
        .unwrap()
        .map(|p| p.unwrap().path())
        .collect::<Vec<_>>();
    paths.sort();
    let mut raw = String::new();
    for path in paths {
        raw.push_str(&format!(
            "{}\t{}\n",
            path.file_name().unwrap().to_str().unwrap(),
            hex(&fs::read(&path).unwrap())
        ));
    }
    assert_eq!(raw, BLOBS_TSV);
    let store = NativeIntentStoreV1::open(
        &control.root,
        control_ns(),
        &pin,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    assert_eq!(store.state().unwrap().diagnostic_json().unwrap(), ACK);
}

#[test]
fn synced_replacement_post_reopen_acknowledges_without_reusing_append_owner_token() {
    let control = Temp::new();
    let native = Temp::new();
    let index = index(&native);
    let original = write(&index);
    let mut store = make_store(&control);
    let mut permit = pending(&mut store, &original);
    drop(index);
    let (mut index, observation, replacement, mapping) =
        reopened(&native, &original, Candidate::Before);
    let operation = store
        .prepare_superseding_native_intent(
            &permit,
            &original,
            &index,
            &observation,
            &replacement,
            &mapping,
        )
        .unwrap();
    store
        .sync_prepared_superseding_native_intent(&mut permit, &operation)
        .unwrap();
    index.test_fail_after_sync();
    assert!(index.commit_prepared_validity(&replacement).is_err());
    drop(index);
    let (mut index, post) = SourceCompactIndex::reopen_prepared_validity(
        &native.root,
        native_ns(),
        &replacement,
        Candidate::Post,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    let before = index.snapshot().unwrap();
    assert!(index.commit_prepared_validity(&replacement).is_err());
    assert_eq!(index.snapshot().unwrap(), before);
    store
        .sync_superseding_native_prefix_ack(&mut permit, &index, &replacement)
        .unwrap();
    assert_eq!(index.snapshot().unwrap(), before);
    assert_eq!(post.candidate(), Candidate::Post);
    assert!(permit.pending_native.is_none());
}
