use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent =
            std::env::var_os("NATIVE_QUEUED_TMP").map_or_else(std::env::temp_dir, PathBuf::from);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "native-queued-source-db-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
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
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
}
fn s(value: impl Into<String>) -> V {
    V::String(value.into())
}
fn index(temp: &Temp) -> SourceCompactIndex {
    let mut index = SourceCompactIndex::create(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "independently declared queued-source metadata",
    )
    .unwrap();
    for row in [
        obj(&[("_id", s("MaxHeight")), ("value", V::number(10.))]),
        obj(&[("_id", s("tx-10-raw")), ("value", s("raw source row"))]),
    ] {
        index.import_row(&index.snapshot().unwrap(), row).unwrap();
    }
    index
}
fn update(id: &str, key: &str, value: V, upsert: bool) -> Vec<V> {
    vec![
        obj(&[("_id", s(id))]),
        obj(&[("$set", obj(&[(key, value)]))]),
        obj(&[("upsert", V::Bool(upsert))]),
    ]
}
#[test]
fn queued_max_write_evaluates_actual_later_page_and_preserves_other_source_update() {
    let temp = Temp::new();
    let mut index = index(&temp);
    let old = index
        .prepare_inline_database(
            &index.snapshot().unwrap(),
            "updateAsync",
            update("MaxHeight", "value", V::number(11.), true),
        )
        .unwrap();
    let mut first = index
        .queue_source_inline_database(
            "updateAsync",
            update("MaxHeight", "value", V::number(11.), true),
        )
        .unwrap();
    let mut other = index
        .queue_source_inline_database(
            "updateAsync",
            update("tx-10-raw", "valid", V::Bool(true), true),
        )
        .unwrap();
    let applied_other = index
        .execute_queued_source_inline_database(&mut other, None)
        .unwrap();
    let before = index.snapshot().unwrap();
    assert!(
        index.append_inline_database(&old, None).is_err(),
        "old early prepared plan stays stale; its rules were NOT widened"
    );
    assert_eq!(index.snapshot().unwrap(), before);
    let applied_first = index
        .execute_queued_source_inline_database(&mut first, None)
        .unwrap();
    assert_eq!(
        index.get("MaxHeight").unwrap().unwrap().get("value"),
        V::number(11.)
    );
    assert_eq!(
        index.get("tx-10-raw").unwrap().unwrap().get("valid"),
        V::Bool(true)
    );
    assert!(
        index
            .resolve_inline_database_post(other.attempted_plan().unwrap())
            .is_err(),
        "legacy strict Post remains closed after a later commit"
    );
    assert_eq!(
        index
            .resolve_applied_queued_source_inline_database(&mut first, &applied_first)
            .unwrap(),
        DependencyReturn::Pending(V::number(1.))
    );
    assert_eq!(
        index
            .resolve_applied_queued_source_inline_database(&mut other, &applied_other)
            .unwrap(),
        DependencyReturn::Pending(V::number(1.))
    );
    let after = index.snapshot().unwrap();
    assert!(index
        .resolve_applied_queued_source_inline_database(&mut other, &applied_other)
        .is_err());
    assert!(index
        .execute_queued_source_inline_database(&mut first, None)
        .is_err());
    assert_eq!(index.snapshot().unwrap(), after);
}
#[test]
fn queued_query_is_not_evaluated_until_real_execution_and_result_is_exact_request_bound() {
    let temp = Temp::new();
    let mut index = index(&temp);
    let mut first = index
        .queue_source_inline_database(
            "updateAsync",
            update("MaxHeight", "value", V::number(11.), false),
        )
        .unwrap();
    let context = index.snapshot().unwrap().context;
    index
        .discard_rows(&index.snapshot().unwrap(), &["MaxHeight".into()], &context)
        .unwrap();
    let mut second = index
        .queue_source_inline_database(
            "updateAsync",
            update("MaxHeight", "value", V::number(11.), false),
        )
        .unwrap();
    let applied_first = index
        .execute_queued_source_inline_database(&mut first, None)
        .unwrap();
    let applied_second = index
        .execute_queued_source_inline_database(&mut second, None)
        .unwrap();
    assert!(
        index
            .resolve_applied_queued_source_inline_database(&mut second, &applied_first)
            .is_err(),
        "even equal no-op return/Post cannot substitute another actual request"
    );
    index
        .import_row(
            &index.snapshot().unwrap(),
            obj(&[("_id", s("later-raw-row")), ("value", V::number(9.))]),
        )
        .unwrap();
    assert_eq!(
        index
            .resolve_applied_queued_source_inline_database(&mut first, &applied_first)
            .unwrap(),
        DependencyReturn::Pending(V::number(0.))
    );
    assert_eq!(
        index
            .resolve_applied_queued_source_inline_database(&mut second, &applied_second)
            .unwrap(),
        DependencyReturn::Pending(V::number(0.))
    );
    assert_eq!(index.get("MaxHeight").unwrap(), None);
}
#[test]
fn queued_request_and_success_receipt_refuse_foreign_and_reopened_live_writer() {
    let ta = Temp::new();
    let tb = Temp::new();
    let mut a = index(&ta);
    let mut b = index(&tb);
    let mut queued = a
        .queue_source_inline_database(
            "updateAsync",
            update("MaxHeight", "value", V::number(11.), true),
        )
        .unwrap();
    let b_before = b.snapshot().unwrap();
    assert!(b
        .execute_queued_source_inline_database(&mut queued, None)
        .is_err());
    assert!(!queued.attempted());
    assert_eq!(b.snapshot().unwrap(), b_before);
    let applied = a
        .execute_queued_source_inline_database(&mut queued, None)
        .unwrap();
    assert!(b
        .resolve_applied_queued_source_inline_database(&mut queued, &applied)
        .is_err());
    assert!(!queued.resolved);
    let pin = a.snapshot().unwrap();
    drop(a);
    let reopened = SourceCompactIndex::open(
        &ta.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &pin,
    )
    .unwrap();
    assert!(
        reopened
            .resolve_applied_queued_source_inline_database(&mut queued, &applied)
            .is_err(),
        "no receipt/owner rebind is inferred from equal disk bytes"
    );
    assert!(!queued.resolved);
    assert_eq!(reopened.snapshot().unwrap(), pin);
}
#[test]
fn queued_context_change_refuses_before_store_mutation() {
    let temp = Temp::new();
    let mut index = index(&temp);
    let mut queued = index
        .queue_source_inline_database(
            "updateAsync",
            update("MaxHeight", "value", V::number(11.), true),
        )
        .unwrap();
    index
        .discard_rows(
            &index.snapshot().unwrap(),
            &[],
            "different independently selected context",
        )
        .unwrap();
    let before = index.snapshot().unwrap();
    assert!(index
        .execute_queued_source_inline_database(&mut queued, None)
        .is_err());
    assert!(!queued.attempted());
    assert_eq!(index.snapshot().unwrap(), before);
}
#[test]
fn queued_cold_write_entries_refuse_before_request_or_append_and_keep_capability() {
    let temp = Temp::new();
    let mut index = index(&temp);
    let mut queued = index
        .queue_source_inline_database(
            "updateAsync",
            update("MaxHeight", "value", V::number(11.), true),
        )
        .unwrap();
    let pin = index.capture_verified_history_pin().unwrap();
    let capability = index.begin_verified_cold_execution(&pin).unwrap();
    let before = index.snapshot().unwrap();
    assert!(matches!(
        index.queue_source_inline_database(
            "updateAsync",
            update("MaxHeight", "value", V::number(12.), true)
        ),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert!(matches!(
        index.execute_queued_source_inline_database(&mut queued, None),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert!(!queued.attempted());
    assert_eq!(index.snapshot().unwrap(), before);
    assert_eq!(
        index.get("MaxHeight").unwrap().unwrap().get("value"),
        V::number(10.)
    );
    index.finish_verified_cold_execution(&capability).unwrap();
    let actual = index
        .execute_queued_source_inline_database(&mut queued, None)
        .unwrap();
    assert_eq!(
        index
            .resolve_applied_queued_source_inline_database(&mut queued, &actual)
            .unwrap(),
        DependencyReturn::Pending(V::number(1.))
    );
}
#[test]
fn queued_partial_or_synced_lost_ack_retains_plan_without_applied_return_or_reexecution() {
    for fault in ["partial", "synced"] {
        let temp = Temp::new();
        let mut index = index(&temp);
        let mut queued = index
            .queue_source_inline_database(
                "updateAsync",
                update("MaxHeight", "value", V::number(11.), true),
            )
            .unwrap();
        queued.append_fault = Some(fault);
        assert!(index
            .execute_queued_source_inline_database(&mut queued, None)
            .is_err());
        assert!(queued.attempted());
        assert!(!queued.successful);
        assert!(!queued.resolved);
        let actual = queued.attempted_plan().unwrap();
        assert_eq!(actual.method(), "updateAsync");
        assert_eq!(
            actual.arguments(),
            update("MaxHeight", "value", V::number(11.), true)
        );
        assert!(
            index
                .execute_queued_source_inline_database(&mut queued, None)
                .is_err(),
            "unknown native result cannot be retried as a new source operation"
        );
        assert!(!queued.successful);
        assert!(!queued.resolved);
    }
}
#[test]
fn applied_source_receipt_keeps_original_read_lease_across_real_ordered_compactions() {
    let temp = Temp::new();
    let mut index = index(&temp);
    index
        .configure_compaction(crate::compact_journal::CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 1,
        })
        .unwrap();
    let mut queued = index
        .queue_source_inline_database(
            "updateAsync",
            update("MaxHeight", "value", V::number(11.), true),
        )
        .unwrap();
    let applied = index
        .execute_queued_source_inline_database(&mut queued, None)
        .unwrap();
    let original_generation = applied.post.pin().publication.generation;
    for round in 0..32 {
        let mut later = index
            .queue_source_inline_database(
                "updateAsync",
                update(
                    "tx-10-raw",
                    "value",
                    s(format!("{round}:{}", "x".repeat(16000))),
                    true,
                ),
            )
            .unwrap();
        index
            .prepare_queued_source_inline_database(&mut later)
            .unwrap();
        let permit = if later
            .attempted_plan()
            .unwrap()
            .append_plan()
            .is_some_and(|plan| plan.intermediate().is_some())
        {
            let before = index.capture_verified_history_pin().unwrap();
            Some(
                index
                    .retain_verified_history_generation(
                        &{
                            let escrow_root = temp.root.join(format!("escrow-{round}"));
                            std::fs::create_dir_all(&escrow_root).unwrap();
                            escrow_root
                        },
                        &[before],
                        crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
                    )
                    .unwrap(),
            )
        } else {
            None
        };
        let actual = index
            .append_prepared_queued_source_inline_database(&mut later, permit.as_ref())
            .unwrap();
        index
            .resolve_applied_queued_source_inline_database(&mut later, &actual)
            .unwrap();
    }
    assert!(
        index.snapshot().unwrap().publication.generation > original_generation,
        "real compaction must occur, not merely ordinary appends"
    );
    let mut cursor = applied.post.cursor();
    let mut old_rows = vec![];
    while let Some(row) = cursor.next_row().unwrap() {
        old_rows.push(row);
    }
    assert_eq!(old_rows.len(), 2);
    assert_eq!(
        old_rows
            .iter()
            .find(|r| r.get("_id") == s("tx-10-raw"))
            .unwrap()
            .get("value"),
        s("raw source row")
    );
    assert_eq!(
        index
            .resolve_applied_queued_source_inline_database(&mut queued, &applied)
            .unwrap(),
        DependencyReturn::Pending(V::number(1.))
    );
    assert_ne!(
        index.get("tx-10-raw").unwrap().unwrap().get("value"),
        s("raw source row")
    );
}

#[cfg(test)]
mod independent_queued_source_review {
    use super::*;

    #[test]
    fn late_preparation_is_not_a_success_and_stale_append_never_requeries() {
        let temp = Temp::new();
        let mut index = index(&temp);
        let context = index.snapshot().unwrap().context;
        index
            .discard_rows(&index.snapshot().unwrap(), &["MaxHeight".into()], &context)
            .unwrap();
        let arguments = update("MaxHeight", "value", V::number(11.), false);
        let mut queued = index
            .queue_source_inline_database("updateAsync", arguments.clone())
            .unwrap();
        index
            .prepare_queued_source_inline_database(&mut queued)
            .unwrap();
        assert!(queued.attempted());
        assert!(!queued.successful);
        assert!(!queued.append_attempted);
        let actual_plan = queued.attempted_plan().unwrap() as *const _;
        assert_eq!(
            queued.attempted_plan().unwrap().returned,
            DependencyReturn::Pending(V::number(0.))
        );
        index
            .import_row(
                &index.snapshot().unwrap(),
                obj(&[("_id", s("MaxHeight")), ("value", V::number(20.))]),
            )
            .unwrap();
        let after_competing_insert = index.snapshot().unwrap();
        let append_bytes = index.journal.io_counters().unwrap().confirmed_append_bytes;
        assert!(index
            .append_prepared_queued_source_inline_database(&mut queued, None)
            .is_err());
        assert_eq!(queued.attempted_plan().unwrap() as *const _, actual_plan);
        assert!(!queued.successful);
        assert!(!queued.append_attempted);
        assert_eq!(index.snapshot().unwrap(), after_competing_insert);
        assert_eq!(
            index.journal.io_counters().unwrap().confirmed_append_bytes,
            append_bytes
        );
        assert_eq!(
            index.get("MaxHeight").unwrap().unwrap().get("value"),
            V::number(20.)
        );
        assert!(index
            .execute_queued_source_inline_database(&mut queued, None)
            .is_err());
        let mut distinct = index
            .queue_source_inline_database("updateAsync", arguments)
            .unwrap();
        let applied = index
            .execute_queued_source_inline_database(&mut distinct, None)
            .unwrap();
        assert_eq!(
            index
                .resolve_applied_queued_source_inline_database(&mut distinct, &applied)
                .unwrap(),
            DependencyReturn::Pending(V::number(1.))
        );
    }

    #[test]
    fn insert_unique_error_is_evaluated_late_and_held_result_survives_later_row_removal() {
        let temp = Temp::new();
        let mut index = index(&temp);
        let row = obj(&[("_id", s("late-insert")), ("value", s("actual raw row"))]);
        let mut queued = index
            .queue_source_inline_database("insertAsync", vec![row.clone()])
            .unwrap();
        index.import_row(&index.snapshot().unwrap(), row).unwrap();
        let before = index.snapshot().unwrap();
        let actual = index
            .execute_queued_source_inline_database(&mut queued, None)
            .unwrap();
        assert_eq!(index.snapshot().unwrap(), before);
        assert!(queued.successful);
        assert!(queued.attempted_plan().unwrap().append_plan().is_none());
        let original_result = queued.attempted_plan().unwrap().returned.clone();
        match &original_result {
            DependencyReturn::PendingThrow(error) => {
                assert_eq!(error.name, "Error");
                assert_eq!(
                    error.message,
                    "Can't insert key late-insert, it violates the unique constraint"
                );
            }
            other => panic!("actual late unique-key source error required, got {other:?}"),
        }
        let context = index.snapshot().unwrap().context;
        index
            .discard_rows(
                &index.snapshot().unwrap(),
                &["late-insert".into()],
                &context,
            )
            .unwrap();
        assert_eq!(index.get("late-insert").unwrap(), None);
        assert_eq!(
            index
                .resolve_applied_queued_source_inline_database(&mut queued, &actual)
                .unwrap(),
            original_result
        );
        assert!(index
            .resolve_applied_queued_source_inline_database(&mut queued, &actual)
            .is_err());
    }
}
