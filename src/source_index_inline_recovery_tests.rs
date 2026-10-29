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
        let parent = std::env::var_os("NATIVE_INLINE_RECOVERY_TMP")
            .map_or_else(std::env::temp_dir, PathBuf::from);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-inline-recovery-{}-{}",
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
fn ns() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn make(temp: &Temp) -> SourceCompactIndex {
    let mut index =
        SourceCompactIndex::create(&temp.root, ns(), "actual inline DB source metadata").unwrap();
    index
        .import_row(
            &index.snapshot().unwrap(),
            object(&[("_id", text("MaxHeight")), ("value", V::number(10.))]),
        )
        .unwrap();
    index
}
fn plan(index: &SourceCompactIndex) -> NativePreparedInlineDatabaseV1 {
    index
        .prepare_inline_database(
            &index.snapshot().unwrap(),
            "updateAsync",
            vec![
                object(&[("_id", text("MaxHeight"))]),
                object(&[("$set", object(&[("value", V::number(11.))]))]),
                object(&[("upsert", V::Bool(true))]),
            ],
        )
        .unwrap()
}
fn commit(index: &mut SourceCompactIndex, p: &NativePreparedInlineDatabaseV1, temp: &Temp) {
    let permit = if p.append_plan().unwrap().intermediate().is_some() {
        let point = index.capture_verified_history_pin().unwrap();
        let escrow = temp.root.join("escrow");
        std::fs::create_dir_all(&escrow).unwrap();
        Some(
            index
                .retain_verified_history_generation(
                    &escrow,
                    &[point],
                    crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
                )
                .unwrap(),
        )
    } else {
        None
    };
    index.append_inline_database(p, permit.as_ref()).unwrap();
}
#[test]
fn inline_actual_synced_lost_return_reopens_only_post_without_reappend() {
    let temp = Temp::new();
    let mut index = make(&temp);
    let mut p = plan(&index);
    p.put.as_mut().unwrap().append.test_append_fault("synced");
    assert!(index.append_inline_database(&p, None).is_err());
    drop(index);
    let (mut index, observed) = SourceCompactIndex::reopen_inline_database(
        &temp.root,
        ns(),
        &p,
        NativePreparedValidityCandidateV1::Post,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    assert_eq!(
        observed.candidate(),
        NativePreparedValidityCandidateV1::Post
    );
    assert_eq!(
        index.get("MaxHeight").unwrap().unwrap().get("value"),
        V::number(11.)
    );
    let before = index.capture_verified_history_pin().unwrap();
    assert!(index.append_inline_database(&p, None).is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    assert_eq!(
        index.resolve_inline_database_post(&p).unwrap(),
        DependencyReturn::Pending(V::number(1.))
    );
}
#[test]
fn inline_exact_before_rebinds_only_actual_native_mutations_and_keeps_old_token_unusable() {
    let temp = Temp::new();
    let index = make(&temp);
    let p = plan(&index);
    drop(index);
    let (mut index, observed) = SourceCompactIndex::reopen_inline_database(
        &temp.root,
        ns(),
        &p,
        NativePreparedValidityCandidateV1::Before,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    assert!(index.append_inline_database(&p, None).is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    let (replacement, mapping) = index.supersede_inline_database(&p, &observed).unwrap();
    let proof = index
        .require_inline_native_supersession(&p, &observed, &replacement, &mapping)
        .unwrap();
    assert_eq!(proof.get("profile"), text(INLINE_DATABASE_REOPEN_PROFILE));
    index
        .require_inline_installation(&p, &replacement, &mapping)
        .unwrap();
    assert_eq!(
        replacement.append_plan().unwrap().mutations(),
        p.append_plan().unwrap().mutations()
    );
    commit(&mut index, &replacement, &temp);
    assert_eq!(
        index.resolve_inline_database_post(&replacement).unwrap(),
        DependencyReturn::Pending(V::number(1.))
    );
    assert_eq!(
        index.get("MaxHeight").unwrap().unwrap().get("value"),
        V::number(11.)
    );
}
#[test]
fn inline_actual_compaction_partial_tail_reopens_intermediate_and_preserves_exact_source_operation()
{
    let temp = Temp::new();
    let mut index = make(&temp);
    index.native_main_test_compaction_fixture().unwrap();
    let mut p = plan(&index);
    assert!(p.append_plan().unwrap().intermediate().is_some());
    let point = index.capture_verified_history_pin().unwrap();
    let escrow = temp.root.join("escrow");
    std::fs::create_dir(&escrow).unwrap();
    let permit = index
        .retain_verified_history_generation(
            &escrow,
            &[point],
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    p.put.as_mut().unwrap().append.test_append_fault("partial");
    assert!(index.append_inline_database(&p, Some(&permit)).is_err());
    drop(index);
    let (mut index, observed) = SourceCompactIndex::reopen_inline_database(
        &temp.root,
        ns(),
        &p,
        NativePreparedValidityCandidateV1::Intermediate,
        Recovery::DiscardIncompleteTail,
    )
    .unwrap();
    assert!(observed.recovery_report().discarded_bytes > 0);
    assert_eq!(
        index.get("MaxHeight").unwrap().unwrap().get("value"),
        V::number(10.)
    );
    let (replacement, mapping) = index.supersede_inline_database(&p, &observed).unwrap();
    index
        .require_inline_native_supersession(&p, &observed, &replacement, &mapping)
        .unwrap();
    // A copy pin is not a live-writer permit after reopen; old actual permit is also stale.
    if replacement.append_plan().unwrap().intermediate().is_some() {
        assert!(index
            .append_inline_database(&replacement, Some(&permit))
            .is_err());
    }
    commit(&mut index, &replacement, &temp);
    assert_eq!(
        index.get("MaxHeight").unwrap().unwrap().get("value"),
        V::number(11.)
    );
    assert_eq!(
        index.resolve_inline_database_post(&replacement).unwrap(),
        DependencyReturn::Pending(V::number(1.))
    );
}
#[test]
fn inline_foreign_or_corrupt_candidate_refuses_without_repairing_any_file() {
    let temp = Temp::new();
    let foreign = Temp::new();
    let index = make(&temp);
    let p = plan(&index);
    let other = make(&foreign);
    drop(other);
    drop(index);
    assert!(SourceCompactIndex::reopen_inline_database(
        &foreign.root,
        ns(),
        &p,
        NativePreparedValidityCandidateV1::Before,
        Recovery::RejectIncompleteTail
    )
    .is_err());
    let pin = p
        .verified_reopen_pin(NativePreparedValidityCandidateV1::Before)
        .unwrap();
    let catalog = pin.canonical_catalog;
    let current = std::fs::read(catalog.join("CURRENT")).unwrap();
    let generation = catalog.join(format!("g-{:020}", pin.publication.generation));
    assert!(generation.exists(), "actual canonical generation path");
    // Deliberately change a retained candidate's independent logical fact instead of guessing bytes.
    let mut bad = p
        .verified_reopen_pin(NativePreparedValidityCandidateV1::Before)
        .unwrap();
    bad.content.chain_sha256d[0] ^= 1;
    assert!(Journal::open_existing_verified_prefix(
        &temp.root,
        ns(),
        Recovery::DiscardIncompleteTail,
        &bad
    )
    .is_err());
    assert_eq!(std::fs::read(catalog.join("CURRENT")).unwrap(), current);
}

#[path = "source_index_inline_review_tests.rs"]
mod independent_review_tests;
