use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp {
    parent: PathBuf,
    root: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent =
            std::env::var_os("NATIVE_INLINE_TMP").map_or_else(std::env::temp_dir, PathBuf::from);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-inline-native-plan-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self { parent, root }
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
fn text(v: &str) -> V {
    V::String(v.into())
}
fn object(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn new_index(temp: &Temp) -> SourceCompactIndex {
    SourceCompactIndex::create(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual inline raw metadata",
    )
    .unwrap()
}
fn update(id: &str, value: V) -> Vec<V> {
    vec![
        object(&[("_id", text(id))]),
        object(&[("$set", object(&[("value", value)]))]),
        object(&[("upsert", V::Bool(true))]),
    ]
}
#[test]
fn inline_native_preparation_has_no_io_and_applied_return_remains_a_distinct_boundary() {
    let temp = Temp::new();
    let mut index = new_index(&temp);
    let before = index.capture_verified_history_pin().unwrap();
    let plan = index
        .prepare_inline_database(
            &before.metadata,
            "updateAsync",
            update("MaxHeight", V::number(11.)),
        )
        .unwrap();
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    assert!(index.resolve_inline_database_post(&plan).is_err());
    let mut fields = vec![];
    plan.visit_canonical_fields(&mut |k, v| fields.push((k.to_string(), v.clone())))
        .unwrap();
    assert!(fields.iter().any(|(k, _)| k == "mutations/0/value"));
    index.append_inline_database(&plan, None).unwrap();
    assert_eq!(
        index.get("MaxHeight").unwrap().unwrap().get("value"),
        V::number(11.)
    );
    assert_eq!(
        index.resolve_inline_database_post(&plan).unwrap(),
        DependencyReturn::Pending(V::number(1.))
    );
    let after = index.capture_verified_history_pin().unwrap();
    assert!(index.append_inline_database(&plan, None).is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), after);
    let mut same_fields = vec![];
    plan.visit_canonical_fields(&mut |k, v| same_fields.push((k.to_string(), v.clone())))
        .unwrap();
    assert_eq!(same_fields, fields);
}
#[test]
fn inline_insert_preserves_memory_copy_and_duplicate_rejection_without_native_write() {
    let temp = Temp::new();
    let mut index = new_index(&temp);
    let raw = object(&[("_id", text("genesisBlock")), ("value", V::Promise)]);
    let plan = index
        .prepare_inline_database(&index.snapshot().unwrap(), "insertAsync", vec![raw])
        .unwrap();
    index.append_inline_database(&plan, None).unwrap();
    let expected = object(&[("_id", text("genesisBlock")), ("value", V::object())]);
    assert_eq!(
        index.resolve_inline_database_post(&plan).unwrap(),
        DependencyReturn::Pending(expected.clone())
    );
    assert_eq!(index.get("genesisBlock").unwrap(), Some(expected.clone()));
    let before = index.capture_verified_history_pin().unwrap();
    let duplicate = index
        .prepare_inline_database(&before.metadata, "insertAsync", vec![expected])
        .unwrap();
    assert!(duplicate.append_plan().is_none());
    index.append_inline_database(&duplicate, None).unwrap();
    assert!(matches!(
        index.resolve_inline_database_post(&duplicate).unwrap(),
        DependencyReturn::PendingThrow(_)
    ));
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
}
#[test]
fn inline_cross_publication_requires_actual_synced_escrow_and_cold_scope_refuses_before_io() {
    let temp = Temp::new();
    let mut index = new_index(&temp);
    index
        .import_row(
            &index.snapshot().unwrap(),
            object(&[("_id", text("MaxHeight")), ("value", V::number(10.))]),
        )
        .unwrap();
    index.native_main_test_compaction_fixture().unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let plan = index
        .prepare_inline_database(
            &before.metadata,
            "updateAsync",
            update("MaxHeight", V::number(11.)),
        )
        .unwrap();
    assert!(plan.append_plan().unwrap().intermediate().is_some());
    assert!(index.append_inline_database(&plan, None).is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    let escrow = temp.parent.join(format!(
        "actual-inline-native-escrow-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&escrow).unwrap();
    let permit = index
        .retain_verified_history_generation(
            &escrow,
            std::slice::from_ref(&before),
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    index.append_inline_database(&plan, Some(&permit)).unwrap();
    assert_eq!(
        index.resolve_inline_database_post(&plan).unwrap(),
        DependencyReturn::Pending(V::number(1.))
    );
    let after = index.capture_verified_history_pin().unwrap();
    assert_ne!(after.metadata.publication, before.metadata.publication);
    let cold = index.begin_verified_cold_execution(&after).unwrap();
    let next = index
        .prepare_inline_database(
            &after.metadata,
            "updateAsync",
            update("indexExists", V::Bool(true)),
        )
        .unwrap();
    assert!(index.append_inline_database(&next, None).is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), after);
    index.finish_verified_cold_execution(&cold).unwrap();
    // Held read pins/permit protect their actual files until explicitly dropped.
    drop(permit);
    drop(index);
    let escrow = escrow.canonicalize().unwrap();
    assert_eq!(escrow.parent(), Some(temp.parent.as_path()));
    let _ = std::fs::remove_dir_all(escrow);
}
