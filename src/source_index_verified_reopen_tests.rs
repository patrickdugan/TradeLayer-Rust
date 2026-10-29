use super::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent =
            std::env::var_os("NATIVE_REOPEN_TMP").map_or_else(std::env::temp_dir, PathBuf::from);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "index-reopen-{}-{}",
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
fn ns() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn s(value: &str) -> V {
    V::String(value.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn make_index(temp: &Temp) -> SourceCompactIndex {
    let mut index =
        SourceCompactIndex::create(&temp.root, ns(), "independent original metadata context")
            .unwrap();
    index
        .import_row(
            &index.snapshot().unwrap(),
            obj(&[
                ("_id", s("tx-0001-input")),
                ("value", obj(&[("payload", s("good"))])),
            ]),
        )
        .unwrap();
    index
}
fn prepare(index: &SourceCompactIndex) -> NativePreparedValidityWriteV1 {
    let NativePreparedValidityQueryV1::Selected(selection) = index
        .prepare_validity_read(
            &index.snapshot().unwrap(),
            s("input"),
            V::number(1.),
            V::Bool(true),
            s("actual validity outcome"),
        )
        .unwrap()
    else {
        panic!()
    };
    let NativePreparedValidityOperationV1::Write(plan) = selection
        .prepare_write(index, &index.snapshot().unwrap())
        .unwrap()
    else {
        panic!()
    };
    *plan
}
fn open(
    temp: &Temp,
    plan: &NativePreparedValidityWriteV1,
    candidate: NativePreparedValidityCandidateV1,
) -> Result<(
    SourceCompactIndex,
    NativePreparedValidityRecoveryObservationV1,
)> {
    SourceCompactIndex::reopen_prepared_validity(
        &temp.root,
        ns(),
        plan,
        candidate,
        Recovery::DiscardIncompleteTail,
    )
}
fn tail(path: &Path) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(b"TXN1short").unwrap();
    f.sync_all().unwrap();
}
fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, path: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for item in std::fs::read_dir(path).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out)
            } else {
                out.push((
                    path.strip_prefix(root).unwrap().into(),
                    std::fs::read(path).unwrap(),
                ))
            }
        }
    }
    let mut out = vec![];
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn before_reopen_requires_dropped_writer_but_preserves_original_read_lease_and_source_query() {
    let temp = Temp::new();
    let index = make_index(&temp);
    let plan = prepare(&index);
    let before = index.snapshot().unwrap();
    let state = index.native_complete_metadata_state().unwrap();
    let retained_read = index.read_generation(&before).unwrap();
    assert!(matches!(
        open(&temp, &plan, NativePreparedValidityCandidateV1::Before),
        Err(CompactIndexError::Storage(StorageError::Locked))
    ));
    drop(index);
    let files = tree(&temp.root);
    let (mut reopened, observation) =
        open(&temp, &plan, NativePreparedValidityCandidateV1::Before).unwrap();
    assert_eq!(
        observation.candidate(),
        NativePreparedValidityCandidateV1::Before
    );
    assert_eq!(observation.profile(), PREPARED_VALIDITY_REOPEN_PROFILE);
    assert_eq!(observation.snapshot(), &before);
    assert_eq!(observation.content(), &state);
    assert_eq!(observation.recovery_report().discarded_bytes, 0);
    assert!(observation.require_authenticated_admission().is_err());
    assert_eq!(reopened.snapshot().unwrap(), before);
    assert_eq!(
        retained_read
            .cursor()
            .next_row()
            .unwrap()
            .unwrap()
            .get("valid"),
        V::Undefined
    );
    assert!(
        reopened.commit_prepared_validity(&plan).is_err(),
        "old live-owner token cannot reappend"
    );
    assert_eq!(plan.query().matched_rows, 1);
    assert_eq!(plan.query().visited_rows, 1);
    drop(reopened);
    assert_eq!(tree(&temp.root), files);
}

#[test]
fn actual_partial_write_accepts_only_prepared_before_without_reexecution() {
    let temp = Temp::new();
    let mut index = make_index(&temp);
    let plan = prepare(&index);
    let before = index.snapshot().unwrap();
    let path = index.journal.path().to_owned();
    index.test_fail_after_partial_write();
    assert!(index.commit_prepared_validity(&plan).is_err());
    drop(index);
    let damaged = tree(&temp.root);
    assert!(open(&temp, &plan, NativePreparedValidityCandidateV1::Post).is_err());
    assert_eq!(tree(&temp.root), damaged);
    assert!(open(
        &temp,
        &plan,
        NativePreparedValidityCandidateV1::Intermediate
    )
    .is_err());
    assert_eq!(tree(&temp.root), damaged);
    let damaged_len = std::fs::metadata(&path).unwrap().len();
    let (reopened, observation) =
        open(&temp, &plan, NativePreparedValidityCandidateV1::Before).unwrap();
    assert!(observation.recovery_report().discarded_bytes > 0);
    assert!(std::fs::metadata(&path).unwrap().len() < damaged_len);
    assert_eq!(reopened.snapshot().unwrap(), before);
    assert_eq!(
        reopened.get("tx-0001-input").unwrap().unwrap().get("valid"),
        V::Undefined
    );
}

#[test]
fn actual_synced_post_reopens_and_resolves_old_await_without_duplicate_append() {
    let temp = Temp::new();
    let mut index = make_index(&temp);
    let plan = prepare(&index);
    index.test_fail_after_sync();
    assert!(index.commit_prepared_validity(&plan).is_err());
    drop(index);
    let files = tree(&temp.root);
    assert!(open(&temp, &plan, NativePreparedValidityCandidateV1::Before).is_err());
    assert_eq!(tree(&temp.root), files);
    let (mut reopened, observation) =
        open(&temp, &plan, NativePreparedValidityCandidateV1::Post).unwrap();
    assert_eq!(
        observation.candidate(),
        NativePreparedValidityCandidateV1::Post
    );
    assert_eq!(observation.content(), plan.expected_after_state());
    let outcome = reopened.resolve_prepared_validity_post(&plan).unwrap();
    assert_eq!(
        outcome.update.unwrap().returned,
        DependencyReturn::Pending(V::number(1.))
    );
    assert_eq!(
        reopened.snapshot().unwrap().sequence,
        plan.append_plan().after().logical_sequence
    );
    assert!(reopened.commit_prepared_validity(&plan).is_err());
    drop(reopened);
    assert_eq!(tree(&temp.root), files);
}

fn needs_compaction(index: &mut SourceCompactIndex) -> NativePreparedValidityWriteV1 {
    index
        .configure_compaction(crate::compact_journal::CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 0,
        })
        .unwrap();
    let row = obj(&[("_id", s("unrelated")), ("padding", s(&"x".repeat(8000)))]);
    index.import_row(&index.snapshot().unwrap(), row).unwrap();
    for _ in 0..2000 {
        let plan = prepare(index);
        if plan.append_plan().intermediate().is_some() {
            return plan;
        }
        let directory = index.journal.get(DIRECTORY).unwrap().unwrap().to_vec();
        index
            .journal
            .commit(vec![Mutation::Put {
                key: DIRECTORY.into(),
                value: directory,
            }])
            .unwrap();
    }
    panic!("actual native append must require compaction")
}

#[test]
fn actual_published_compaction_midpoint_reopens_with_original_logical_physical_mapping() {
    let temp = Temp::new();
    let mut index = make_index(&temp);
    let plan = needs_compaction(&mut index);
    let pin = plan.append_plan().intermediate().unwrap();
    let before = index.native_complete_metadata_state().unwrap();
    let retained = index.read_generation(&index.snapshot().unwrap()).unwrap();
    index.journal.test_fault("after-current-sync", false);
    assert!(index.commit_prepared_validity(&plan).is_err());
    drop(index);
    let files = tree(&temp.root);
    for candidate in [
        NativePreparedValidityCandidateV1::Before,
        NativePreparedValidityCandidateV1::Post,
    ] {
        assert!(open(&temp, &plan, candidate).is_err());
        assert_eq!(tree(&temp.root), files);
    }
    let (reopened, observation) = open(
        &temp,
        &plan,
        NativePreparedValidityCandidateV1::Intermediate,
    )
    .unwrap();
    assert_eq!(
        observation.candidate(),
        NativePreparedValidityCandidateV1::Intermediate
    );
    assert_eq!(observation.snapshot().publication, pin.publication);
    assert_eq!(observation.snapshot().sequence, pin.logical_sequence);
    assert!(pin.publication.physical_creation_sequence < pin.publication.creation_sequence);
    assert_eq!(observation.content(), &before);
    assert_eq!(
        reopened.get("tx-0001-input").unwrap().unwrap().get("valid"),
        V::Undefined
    );
    assert!(retained.cursor().next_row().unwrap().is_some());
    drop(reopened);
    assert_eq!(tree(&temp.root), files);
}

#[test]
fn foreign_root_namespace_and_stale_sequence_refuse_before_any_tail_repair() {
    let a = Temp::new();
    let b = Temp::new();
    let index_a = make_index(&a);
    let index_b = make_index(&b);
    let plan = prepare(&index_a);
    let before_b = index_b.snapshot().unwrap();
    assert_eq!(before_b.sequence, index_a.snapshot().unwrap().sequence);
    assert_eq!(
        index_b.native_complete_metadata_state().unwrap(),
        index_a.native_complete_metadata_state().unwrap()
    );
    let path_b = index_b.journal.path().to_owned();
    drop(index_a);
    drop(index_b);
    tail(&path_b);
    let files = tree(&b.root);
    assert!(open(&b, &plan, NativePreparedValidityCandidateV1::Before).is_err());
    assert_eq!(tree(&b.root), files);
    assert!(SourceCompactIndex::reopen_prepared_validity(
        &b.root,
        Namespace::new("BTC", false, "txIndex").unwrap(),
        &plan,
        NativePreparedValidityCandidateV1::Before,
        Recovery::DiscardIncompleteTail
    )
    .is_err());
    assert_eq!(tree(&b.root), files);
    let (mut current, _) = open(&a, &plan, NativePreparedValidityCandidateV1::Before).unwrap();
    current
        .import_row(&current.snapshot().unwrap(), obj(&[("_id", s("later"))]))
        .unwrap();
    let path = current.journal.path().to_owned();
    drop(current);
    tail(&path);
    let files = tree(&a.root);
    assert!(open(&a, &plan, NativePreparedValidityCandidateV1::Before).is_err());
    assert_eq!(tree(&a.root), files);
    assert!(open(&a, &plan, NativePreparedValidityCandidateV1::Post).is_err());
    assert_eq!(tree(&a.root), files);
}

#[test]
fn same_sequence_valid_crc_forged_page_and_extra_key_refuse_before_truncation() {
    for extra in [false, true] {
        let temp = Temp::new();
        let index = make_index(&temp);
        let plan = prepare(&index);
        let path = index.journal.path().to_owned();
        let dir = index.journal.get(DIRECTORY).unwrap().unwrap().to_vec();
        let mut rows = index.page(&index.directory.pages[0]).unwrap();
        if !extra {
            rows[0]
                .insert("value", obj(&[("payload", s("evil"))]))
                .unwrap();
        }
        let mut changes = vec![
            Mutation::Put {
                key: DIRECTORY.into(),
                value: dir.clone(),
            },
            Mutation::Put {
                key: page_key(0),
                value: encode_page(&rows).unwrap(),
            },
        ];
        if extra {
            changes.push(Mutation::Put {
                key: "unselected-extra".into(),
                value: b"not in independently prepared state".to_vec(),
            });
        }
        drop(index);
        let alternate = Temp::new();
        let (mut raw, _) =
            crate::storage::Journal::open(&alternate.root, ns(), Recovery::RejectIncompleteTail)
                .unwrap();
        raw.commit(vec![Mutation::Put {
            key: DIRECTORY.into(),
            value: dir,
        }])
        .unwrap();
        raw.commit(changes).unwrap();
        assert_eq!(
            raw.sequence().unwrap(),
            plan.append_plan().before().logical_sequence
        );
        let alternate_path = raw.path().to_owned();
        drop(raw);
        std::fs::write(&path, std::fs::read(alternate_path).unwrap()).unwrap();
        tail(&path);
        let files = tree(&temp.root);
        assert!(matches!(
            open(&temp, &plan, NativePreparedValidityCandidateV1::Before),
            Err(CompactIndexError::Storage(StorageError::Corrupt(
                "unexpected independently pinned complete KV prefix"
            )))
        ));
        assert_eq!(tree(&temp.root), files);
    }
}

#[test]
fn complete_profile_flag_and_receipt_fields_are_never_normalized() {
    for field in 0..5 {
        let temp = Temp::new();
        let index = make_index(&temp);
        let mut plan = prepare(&index);
        let path = index.journal.path().to_owned();
        drop(index);
        tail(&path);
        let files = tree(&temp.root);
        match field {
            0 => plan.before_state.complete = false,
            1 => plan.before_state.profile.push('x'),
            2 => plan.before_state.keys += 1,
            3 => plan.before_state.bytes += 1,
            _ => plan.before_state.chain_sha256d[0] ^= 1,
        }
        assert!(open(&temp, &plan, NativePreparedValidityCandidateV1::Before).is_err());
        assert_eq!(tree(&temp.root), files);
    }
}

fn assert_preserved(
    original: &NativePreparedValidityWriteV1,
    replacement: &NativePreparedValidityWriteV1,
) {
    assert_eq!(replacement.query(), original.query());
    assert_eq!(replacement.update_arguments(), original.update_arguments());
    assert_eq!(replacement.pinned_read(), original.pinned_read());
    assert_eq!(
        replacement.expected_before_state(),
        original.expected_before_state()
    );
    assert_eq!(
        replacement.expected_after_state(),
        original.expected_after_state()
    );
    assert_eq!(
        replacement.append_plan().mutations(),
        original.append_plan().mutations()
    );
    assert_eq!(
        replacement.read.cursor().next_row().unwrap(),
        original.read.cursor().next_row().unwrap()
    );
}

#[test]
fn partial_before_supersession_reprepares_actual_mutations_without_source_reselection() {
    let temp = Temp::new();
    let mut index = make_index(&temp);
    let plan = prepare(&index);
    index.test_fail_after_partial_write();
    assert!(index.commit_prepared_validity(&plan).is_err());
    drop(index);
    let (mut reopened, observation) =
        open(&temp, &plan, NativePreparedValidityCandidateV1::Before).unwrap();
    let before = reopened.snapshot().unwrap();
    let footprint = reopened.footprint().unwrap();
    let (replacement, mapping) = reopened
        .supersede_prepared_validity(&plan, &observation)
        .unwrap();
    assert_eq!(reopened.snapshot().unwrap(), before);
    assert_eq!(reopened.footprint().unwrap(), footprint);
    assert_eq!(mapping.original_before(), plan.append_plan().before());
    assert_eq!(mapping.original_after(), plan.append_plan().after());
    assert_eq!(mapping.reopened(), &before);
    assert_eq!(
        mapping.replacement_before(),
        replacement.append_plan().before()
    );
    assert_eq!(
        mapping.replacement_after(),
        replacement.append_plan().after()
    );
    assert_preserved(&plan, &replacement);
    let result = reopened.commit_prepared_validity(&replacement).unwrap();
    assert_eq!(result.query, plan.query);
    assert_eq!(
        reopened.native_complete_metadata_state().unwrap(),
        *plan.expected_after_state()
    );
    let seq = reopened.snapshot().unwrap().sequence;
    assert!(reopened.commit_prepared_validity(&replacement).is_err());
    assert_eq!(reopened.snapshot().unwrap().sequence, seq);
}

#[test]
fn unpublished_orphan_generation_gets_fresh_superseding_native_candidate_not_old_plan_retry() {
    let temp = Temp::new();
    let mut index = make_index(&temp);
    let plan = needs_compaction(&mut index);
    let old_after = plan.append_plan().after().clone();
    index.journal.test_fault("before-current", false);
    assert!(index.commit_prepared_validity(&plan).is_err());
    drop(index);
    let files = tree(&temp.root);
    assert!(open(
        &temp,
        &plan,
        NativePreparedValidityCandidateV1::Intermediate
    )
    .is_err());
    assert_eq!(tree(&temp.root), files);
    let (mut reopened, observation) =
        open(&temp, &plan, NativePreparedValidityCandidateV1::Before).unwrap();
    let (replacement, mapping) = reopened
        .supersede_prepared_validity(&plan, &observation)
        .unwrap();
    assert_preserved(&plan, &replacement);
    assert_ne!(
        replacement.append_plan().after().publication.generation,
        old_after.publication.generation,
        "orphan generation cannot be reused silently"
    );
    assert_eq!(mapping.original_after(), &old_after);
    assert_eq!(
        mapping.replacement_after(),
        replacement.append_plan().after()
    );
    assert!(reopened.commit_prepared_validity(&plan).is_err());
    reopened.commit_prepared_validity(&replacement).unwrap();
    assert_eq!(
        reopened.native_complete_metadata_state().unwrap(),
        *plan.expected_after_state()
    );
}

#[test]
fn published_intermediate_supersession_preserves_original_fixed_read_and_update_arguments() {
    let temp = Temp::new();
    let mut index = make_index(&temp);
    let plan = needs_compaction(&mut index);
    index.journal.test_fault("after-current-sync", false);
    assert!(index.commit_prepared_validity(&plan).is_err());
    drop(index);
    let (mut reopened, observation) = open(
        &temp,
        &plan,
        NativePreparedValidityCandidateV1::Intermediate,
    )
    .unwrap();
    let (replacement, mapping) = reopened
        .supersede_prepared_validity(&plan, &observation)
        .unwrap();
    assert_preserved(&plan, &replacement);
    assert_eq!(replacement.write_expected(), observation.snapshot());
    assert_ne!(
        replacement.write_expected().publication,
        plan.write_expected().publication
    );
    assert_eq!(
        mapping.replacement_before().publication,
        plan.append_plan().intermediate().unwrap().publication
    );
    reopened.commit_prepared_validity(&replacement).unwrap();
    assert_eq!(
        reopened.native_complete_metadata_state().unwrap(),
        *plan.expected_after_state()
    );
}

#[test]
fn supersession_refuses_post_stale_or_a_different_actual_plan_without_appending() {
    let temp = Temp::new();
    let mut index = make_index(&temp);
    let plan = prepare(&index);
    // An independent genuine preparation has different actual source update args.
    let NativePreparedValidityQueryV1::Selected(other) = index
        .prepare_validity_read(
            &index.snapshot().unwrap(),
            s("input"),
            V::number(1.),
            V::Bool(false),
            s("different actual outcome"),
        )
        .unwrap()
    else {
        panic!()
    };
    let NativePreparedValidityOperationV1::Write(other) = other
        .prepare_write(&index, &index.snapshot().unwrap())
        .unwrap()
    else {
        panic!()
    };
    drop(index);
    let (mut reopened, observation) =
        open(&temp, &plan, NativePreparedValidityCandidateV1::Before).unwrap();
    let state = reopened.native_complete_metadata_state().unwrap();
    let before = reopened.snapshot().unwrap();
    assert!(reopened
        .supersede_prepared_validity(&other, &observation)
        .is_err());
    assert_eq!(reopened.snapshot().unwrap(), before);
    assert_eq!(reopened.native_complete_metadata_state().unwrap(), state);
    reopened
        .import_row(&reopened.snapshot().unwrap(), obj(&[("_id", s("later"))]))
        .unwrap();
    assert!(reopened
        .supersede_prepared_validity(&plan, &observation)
        .is_err());
    drop(reopened);
    let post = Temp::new();
    index = make_index(&post);
    let plan = prepare(&index);
    index.commit_prepared_validity(&plan).unwrap();
    drop(index);
    let (reopened, observation) =
        open(&post, &plan, NativePreparedValidityCandidateV1::Post).unwrap();
    let before = reopened.snapshot().unwrap();
    assert!(reopened
        .supersede_prepared_validity(&plan, &observation)
        .is_err());
    assert_eq!(reopened.snapshot().unwrap(), before);
}
