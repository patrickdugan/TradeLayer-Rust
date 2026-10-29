use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent =
            std::env::var_os("NATIVE_PREPARED_TMP").map_or_else(std::env::temp_dir, PathBuf::from);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "prepared-validity-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(root) = self.root.canonicalize() {
            assert_eq!(root.parent(), Some(self.parent.as_path()));
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
fn s(v: &str) -> V {
    V::String(v.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn make_index(temp: &Temp, count: usize) -> SourceCompactIndex {
    let mut index = SourceCompactIndex::create(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual source input metadata",
    )
    .unwrap();
    for at in 0..count {
        let row = obj(&[
            ("_id", s(&format!("tx-{at:04}-id-{at}"))),
            (
                "value",
                obj(&[
                    ("payload", s("1literal caller metadata")),
                    ("txId", s(&format!("id-{at}"))),
                ]),
            ),
        ]);
        index.import_row(&index.snapshot().unwrap(), row).unwrap();
    }
    index
}
fn selected(index: &SourceCompactIndex, txid: &str) -> NativePreparedValiditySelectionV1 {
    let NativePreparedValidityQueryV1::Selected(selected) = index
        .prepare_validity_read(
            &index.snapshot().unwrap(),
            s(txid),
            V::number(1.),
            V::Bool(true),
            V::Undefined,
        )
        .unwrap()
    else {
        panic!("actual suffix must be selected")
    };
    *selected
}
fn prepared(index: &SourceCompactIndex, txid: &str) -> NativePreparedValidityWriteV1 {
    let NativePreparedValidityOperationV1::Write(plan) = selected(index, txid)
        .prepare_write(index, &index.snapshot().unwrap())
        .unwrap()
    else {
        panic!("valid metadata update must prepare")
    };
    *plan
}
#[test]
fn complete_1100_row_query_prepares_exact_native_intent_before_any_update() {
    let temp = Temp::new();
    let control = Temp::new();
    let mut index = make_index(&temp, 1100);
    let before = index.snapshot().unwrap();
    let footprint = index.footprint().unwrap();
    let state = index.native_complete_metadata_state().unwrap();
    let plan = prepared(&index, "id-1099");
    assert!(plan.query().complete);
    assert_eq!(plan.query().visited_rows, 1100);
    assert_eq!(plan.query().matched_rows, 1100);
    assert_eq!(plan.query().snapshot, before);
    assert_eq!(plan.write_expected(), &before);
    assert_eq!(index.snapshot().unwrap(), before);
    assert_eq!(index.footprint().unwrap(), footprint);
    assert_eq!(plan.expected_before_state(), &state);
    assert_ne!(plan.expected_after_state(), &state);
    assert!(plan.expected_after_state().complete);
    use crate::source_contract_replay::source_durable::SourceDurableFilesystem;
    use crate::source_main_native_intent_store::{
        NativeIntentStoreV1, NativeObservedAppendBindingV1, NativeWriteIntentCandidatesV1,
    };
    let mut store = NativeIntentStoreV1::create(
        &control.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[("profile", s(PREPARED_VALIDITY_PROFILE))]),
        V::Null,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let mut permit = store
        .sync_command_intent(
            &store.pin().unwrap(),
            b"explicit metadata-operation input; no financial result",
            V::Null,
            1024 * 1024,
        )
        .unwrap();
    let read_context = plan.durable_read_context();
    assert_eq!(read_context.get("logical").get("complete"), V::Bool(true));
    let facts =
        NativeWriteIntentCandidatesV1::from_prepared_append(plan.append_plan(), read_context)
            .unwrap();
    store.sync_native_intent(&mut permit, &facts).unwrap();
    assert_eq!(
        index.snapshot().unwrap(),
        before,
        "native intent is synced before append"
    );
    let evaluation = index.commit_prepared_validity(&plan).unwrap();
    assert_eq!(
        evaluation.outcome,
        crate::validity_source::SourceOutcome::Returned(V::Undefined)
    );
    assert_eq!(
        evaluation.update.unwrap().returned,
        DependencyReturn::Pending(V::number(1.))
    );
    assert_eq!(
        index.native_complete_metadata_state().unwrap(),
        *plan.expected_after_state()
    );
    store
        .sync_native_prefix_ack(
            &mut permit,
            &NativeObservedAppendBindingV1::observe(&index.journal).unwrap(),
        )
        .unwrap();
    assert_eq!(
        index.get("tx-1099-id-1099").unwrap().unwrap().get("valid"),
        V::Bool(true)
    );
    // Existing bounded DB API stays bounded; no shortened source array appears.
    assert!(index.native_database("findAsync", &[V::object()]).is_err());
}
#[test]
fn prepared_update_matches_actual_legacy_streamed_dependency_observations() {
    let a = Temp::new();
    let b = Temp::new();
    let mut legacy = make_index(&a, 3);
    let mut next = make_index(&b, 3);
    let expected = legacy
        .native_stream_upsert_validity(
            &legacy.snapshot().unwrap(),
            s("id-1"),
            V::number(1.),
            V::Bool(true),
            V::Undefined,
            &mut |_| Ok(None),
        )
        .unwrap();
    let plan = prepared(&next, "id-1");
    let actual = next.commit_prepared_validity(&plan).unwrap();
    assert_eq!(actual.query.chain_sha256d, expected.query.chain_sha256d);
    assert_eq!(actual.query.visited_rows, expected.query.visited_rows);
    assert_eq!(actual.query.matched_rows, expected.query.matched_rows);
    assert!(actual.query.complete && expected.query.complete);
    assert_eq!(actual.outcome, expected.outcome);
    assert_eq!(actual.update, expected.update);
    assert_eq!(
        next.native_complete_metadata_state().unwrap(),
        legacy.native_complete_metadata_state().unwrap()
    );
}
#[test]
fn source_rejection_and_memory_copy_throw_do_not_mutate_metadata() {
    let temp = Temp::new();
    let mut index = make_index(&temp, 1);
    let before = index.snapshot().unwrap();
    let state = index.native_complete_metadata_state().unwrap();
    let error = SourceException {
        name: "Error".into(),
        message: "recorded source update failure".into(),
    };
    let rejected = selected(&index, "id-0")
        .source_rejected(&index, error.clone())
        .unwrap();
    assert_eq!(
        rejected.outcome,
        crate::validity_source::SourceOutcome::Thrown(error)
    );
    assert_eq!(rejected.post_state, before);
    assert_eq!(index.native_complete_metadata_state().unwrap(), state);
    let before = index.snapshot().unwrap();
    let state = index.native_complete_metadata_state().unwrap();
    let reason = obj(&[("$bad", V::Bool(true))]);
    let NativePreparedValidityQueryV1::Selected(selection) = index
        .prepare_validity_read(
            &before,
            s("id-0"),
            V::number(1.),
            V::Bool(true),
            reason.clone(),
        )
        .unwrap()
    else {
        panic!()
    };
    let NativePreparedValidityOperationV1::Complete(actual) =
        (*selection).prepare_write(&index, &before).unwrap()
    else {
        panic!("actual NeDB memory-copy throw must remain a source throw")
    };
    let legacy = index
        .native_stream_upsert_validity(
            &before,
            s("id-0"),
            V::number(1.),
            V::Bool(true),
            reason,
            &mut |_| Ok(None),
        )
        .unwrap();
    assert_eq!(*actual, legacy);
    assert!(matches!(
        actual.outcome,
        crate::validity_source::SourceOutcome::Thrown(_)
    ));
    assert_eq!(index.snapshot().unwrap(), before);
    assert_eq!(index.native_complete_metadata_state().unwrap(), state);
}
#[test]
fn read_pin_is_distinct_from_current_write_pin_after_pure_compaction() {
    let temp = Temp::new();
    let mut index = make_index(&temp, 3);
    let old = index.snapshot().unwrap();
    let selection = selected(&index, "id-1");
    let compact = index.journal.prepare_compaction().unwrap();
    index.journal.publish_compaction(&compact).unwrap();
    let current = index.snapshot().unwrap();
    assert_eq!(old.sequence, current.sequence);
    assert_ne!(old.publication, current.publication);
    let NativePreparedValidityOperationV1::Write(plan) =
        selection.prepare_write(&index, &current).unwrap()
    else {
        panic!()
    };
    assert_eq!(plan.query().snapshot, old);
    assert_eq!(plan.write_expected(), &current);
    index.commit_prepared_validity(&plan).unwrap();
    assert_eq!(
        index.native_complete_metadata_state().unwrap(),
        *plan.expected_after_state()
    );
}
#[test]
fn stale_or_foreign_selection_rejects_before_native_mutation() {
    let temp = Temp::new();
    let foreign = Temp::new();
    let mut index = make_index(&temp, 2);
    let other = make_index(&foreign, 2);
    let selection = selected(&index, "id-0");
    let before = other.footprint().unwrap();
    assert!(matches!(
        selection.prepare_write(&other, &other.snapshot().unwrap()),
        Err(CompactIndexError::WrongContext)
    ));
    assert_eq!(other.footprint().unwrap(), before);
    let selection = selected(&index, "id-0");
    index
        .import_row(
            &index.snapshot().unwrap(),
            obj(&[("_id", s("another source row"))]),
        )
        .unwrap();
    let before = index.footprint().unwrap();
    assert!(matches!(
        selection.prepare_write(&index, &index.snapshot().unwrap()),
        Err(CompactIndexError::WrongContext)
    ));
    assert_eq!(index.footprint().unwrap(), before);
}
#[test]
fn exact_native_post_resolves_old_await_after_reopen_without_duplicate_commit() {
    let temp = Temp::new();
    let mut index = make_index(&temp, 3);
    let plan = prepared(&index, "id-1");
    let expected = index.commit_prepared_validity(&plan).unwrap();
    let post = index.snapshot().unwrap();
    let state = index.native_complete_metadata_state().unwrap();
    drop(index);
    let mut reopened = SourceCompactIndex::open(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &post,
    )
    .unwrap();
    assert!(
        reopened.commit_prepared_validity(&plan).is_err(),
        "stale write CAS cannot append twice"
    );
    assert_eq!(reopened.snapshot().unwrap(), post);
    let resolved = reopened.resolve_prepared_validity_post(&plan).unwrap();
    assert_eq!(resolved, expected);
    assert_eq!(reopened.snapshot().unwrap(), post);
    assert_eq!(reopened.native_complete_metadata_state().unwrap(), state);
    assert_eq!(
        plan.read.cursor().next_row().unwrap().unwrap().get("valid"),
        V::Undefined,
        "pinned original read is preserved across commit/reopen"
    );
}
#[test]
fn no_suffix_is_complete_and_does_not_construct_a_write() {
    let temp = Temp::new();
    let index = make_index(&temp, 3);
    let before = index.snapshot().unwrap();
    let NativePreparedValidityQueryV1::Complete(result) = index
        .prepare_validity_read(
            &before,
            s("not present"),
            V::number(1.),
            V::Bool(false),
            s("literal input"),
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(result.query.visited_rows, 3);
    assert!(result.query.complete);
    assert!(result.update.is_none());
    assert_eq!(result.post_state, before);
    assert_eq!(index.snapshot().unwrap(), before);
}
