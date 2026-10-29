use crate::compact_journal::CatalogPublication;
use crate::compact_journal::{CompactingJournal, CompactionConfig, NativeAppendPin};
use crate::storage::{Mutation, Namespace, Recovery};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp {
    path: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("TL_APPEND_TMP")
            .map_or_else(std::env::temp_dir, PathBuf::from)
            .canonicalize()
            .unwrap();
        let path = parent.join(format!(
            "prepared-append-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self { path, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(path) = self.path.canonicalize() {
            assert_eq!(path.parent(), Some(self.parent.as_path()));
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
fn namespace() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn journal(temp: &Temp) -> CompactingJournal {
    CompactingJournal::open(&temp.path, namespace(), Recovery::RejectIncompleteTail)
        .unwrap()
        .0
}
fn put(value: &[u8]) -> Vec<Mutation> {
    vec![Mutation::Put {
        key: "row".into(),
        value: value.into(),
    }]
}
fn pin(journal: &CompactingJournal) -> NativeAppendPin {
    NativeAppendPin {
        publication: journal.publication().unwrap().clone(),
        logical_sequence: journal.sequence().unwrap(),
    }
}
fn tree(path: &Path) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    fn visit(root: &Path, path: &Path, out: &mut Vec<(PathBuf, u64, std::time::SystemTime)>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let metadata = entry.metadata().unwrap();
            if metadata.is_dir() {
                visit(root, &path, out);
            } else {
                out.push((
                    path.strip_prefix(root).unwrap().into(),
                    metadata.len(),
                    metadata.modified().unwrap(),
                ));
            }
        }
    }
    let mut out = vec![];
    visit(path, path, &mut out);
    out.sort();
    out
}
#[test]
fn preparation_has_no_file_writes_and_exact_candidate_commits_once() {
    let temp = Temp::new();
    let mut journal = journal(&temp);
    journal.commit(put(b"before")).unwrap();
    let before = pin(&journal);
    let files = tree(&temp.path);
    let plan = journal.prepare_native_append(put(b"after")).unwrap();
    assert_eq!(plan.before(), &before);
    assert_eq!(plan.after().logical_sequence, before.logical_sequence + 1);
    assert!(plan.intermediate().is_none());
    assert_eq!(tree(&temp.path), files);
    assert_eq!(pin(&journal), before);
    assert_eq!(
        journal.commit_prepared_native_append(&plan).unwrap(),
        *plan.after()
    );
    assert_eq!(journal.get("row").unwrap(), Some(&b"after"[..]));
    let files = tree(&temp.path);
    let after = pin(&journal);
    assert!(journal.commit_prepared_native_append(&plan).is_err());
    assert_eq!(pin(&journal), after);
    assert_eq!(tree(&temp.path), files);
}
#[test]
fn foreign_and_reopened_owner_plans_reject_before_file_mutation() {
    let a = Temp::new();
    let b = Temp::new();
    let mut first = journal(&a);
    let mut second = journal(&b);
    first.commit(put(b"a")).unwrap();
    second.commit(put(b"b")).unwrap();
    let plan = first.prepare_native_append(put(b"changed")).unwrap();
    let second_before = pin(&second);
    let files = tree(&b.path);
    assert!(second.commit_prepared_native_append(&plan).is_err());
    assert_eq!(tree(&b.path), files);
    assert_eq!(pin(&second), second_before);
    assert_eq!(second.get("row").unwrap(), Some(&b"b"[..]));
    let before = pin(&first);
    drop(first);
    let mut reopened = CompactingJournal::open_pinned(
        &a.path,
        namespace(),
        Recovery::RejectIncompleteTail,
        &before.publication,
        before.logical_sequence,
    )
    .unwrap()
    .0;
    let files = tree(&a.path);
    assert!(reopened.commit_prepared_native_append(&plan).is_err());
    assert_eq!(tree(&a.path), files);
    assert_eq!(pin(&reopened), before);
}
#[test]
fn automatic_compaction_pin_is_prepared_before_any_publication_and_pins_old_reads() {
    let temp = Temp::new();
    let mut journal = journal(&temp);
    journal
        .configure(CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 0,
        })
        .unwrap();
    let value = vec![42; 32000];
    journal.commit(put(&value)).unwrap();
    let old_read = journal.read_snapshot().unwrap();
    let changed = vec![43; 32000];
    let plan = loop {
        let files = tree(&temp.path);
        let candidate = journal.prepare_native_append(put(&changed)).unwrap();
        assert_eq!(tree(&temp.path), files);
        if candidate.intermediate().is_some() {
            break candidate;
        }
        journal.commit_prepared_native_append(&candidate).unwrap();
        assert!(journal.sequence().unwrap() < 12);
    };
    let before = pin(&journal);
    let intermediate = plan.intermediate().unwrap();
    assert_eq!(intermediate.logical_sequence, before.logical_sequence);
    assert_eq!(plan.after().publication, intermediate.publication);
    assert!(
        intermediate.publication.physical_creation_sequence
            < intermediate.publication.creation_sequence
    );
    assert_eq!(pin(&journal), before);
    assert_eq!(
        journal.commit_prepared_native_append(&plan).unwrap(),
        *plan.after()
    );
    assert_eq!(old_read.get("row"), Some(value.as_slice()));
    assert_eq!(journal.get("row").unwrap(), Some(changed.as_slice()));
}
#[test]
fn synced_append_error_reopens_only_at_independently_prepared_candidate() {
    let temp = Temp::new();
    let mut journal = journal(&temp);
    journal.commit(put(b"before")).unwrap();
    let plan = journal.prepare_native_append(put(b"candidate")).unwrap();
    let before = plan.before().clone();
    let after = plan.after().clone();
    journal.test_fail_after_sync();
    assert!(journal.commit_prepared_native_append(&plan).is_err());
    assert!(journal.sequence().is_err());
    drop(journal);
    let files = tree(&temp.path);
    assert!(CompactingJournal::open_pinned(
        &temp.path,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &before.publication,
        before.logical_sequence
    )
    .is_err());
    assert_eq!(tree(&temp.path), files);
    let (journal, _) = CompactingJournal::open_pinned(
        &temp.path,
        namespace(),
        Recovery::RejectIncompleteTail,
        &after.publication,
        after.logical_sequence,
    )
    .unwrap();
    assert_eq!(journal.get("row").unwrap(), Some(&b"candidate"[..]));
}
#[test]
fn partial_append_rejects_candidate_without_repair_then_recovers_prepared_before() {
    let temp = Temp::new();
    let mut journal = journal(&temp);
    journal.commit(put(b"before")).unwrap();
    let plan = journal.prepare_native_append(put(b"candidate")).unwrap();
    let before = plan.before().clone();
    let after = plan.after().clone();
    journal.test_fail_after_partial_write();
    assert!(journal.commit_prepared_native_append(&plan).is_err());
    drop(journal);
    let files = tree(&temp.path);
    assert!(CompactingJournal::open_pinned(
        &temp.path,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &after.publication,
        after.logical_sequence
    )
    .is_err());
    assert_eq!(tree(&temp.path), files);
    let (mut journal, report) = CompactingJournal::open_pinned(
        &temp.path,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &before.publication,
        before.logical_sequence,
    )
    .unwrap();
    assert!(report.discarded_bytes > 0);
    assert_eq!(journal.get("row").unwrap(), Some(&b"before"[..]));
    assert!(
        journal.commit_prepared_native_append(&plan).is_err(),
        "reopened owner cannot reuse old live plan"
    );
    let retry = journal.prepare_native_append(put(b"candidate")).unwrap();
    assert_eq!(retry.after(), &after);
    journal.commit_prepared_native_append(&retry).unwrap();
}
fn compaction_plan(journal: &mut CompactingJournal) -> crate::compact_journal::NativeAppendPlan {
    journal
        .configure(CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 0,
        })
        .unwrap();
    let value = vec![71; 32000];
    journal.commit(put(&value)).unwrap();
    loop {
        let plan = journal.prepare_native_append(put(&value)).unwrap();
        if plan.intermediate().is_some() {
            return plan;
        }
        journal.commit_prepared_native_append(&plan).unwrap();
        assert!(journal.sequence().unwrap() < 12);
    }
}
#[test]
fn published_compaction_error_resolves_at_prepared_intermediate_not_after() {
    let temp = Temp::new();
    let mut journal = journal(&temp);
    let plan = compaction_plan(&mut journal);
    let intermediate = plan.intermediate().unwrap();
    let after = plan.after().clone();
    journal.test_fault("after-current-sync", false);
    assert!(journal.commit_prepared_native_append(&plan).is_err());
    assert!(journal.sequence().is_err());
    drop(journal);
    let files = tree(&temp.path);
    assert!(CompactingJournal::open_pinned(
        &temp.path,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &after.publication,
        after.logical_sequence
    )
    .is_err());
    assert_eq!(tree(&temp.path), files);
    let (mut journal, _) = CompactingJournal::open_pinned(
        &temp.path,
        namespace(),
        Recovery::RejectIncompleteTail,
        &intermediate.publication,
        intermediate.logical_sequence,
    )
    .unwrap();
    let retry = journal
        .prepare_native_append(plan.mutations().to_vec())
        .unwrap();
    assert!(retry.intermediate().is_none());
    assert_eq!(retry.after(), &after);
    journal.commit_prepared_native_append(&retry).unwrap();
}
#[test]
fn configuration_drift_rejects_prepared_operation_before_files_change() {
    let temp = Temp::new();
    let mut journal = journal(&temp);
    journal.commit(put(b"before")).unwrap();
    let plan = journal.prepare_native_append(put(b"after")).unwrap();
    let before = pin(&journal);
    let files = tree(&temp.path);
    journal
        .configure(CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 0,
        })
        .unwrap();
    assert!(journal.commit_prepared_native_append(&plan).is_err());
    assert_eq!(pin(&journal), before);
    assert_eq!(tree(&temp.path), files);
}
#[test]
fn abandoned_generation_requires_a_new_prepared_candidate_before_retry() {
    let temp = Temp::new();
    let mut journal = journal(&temp);
    let plan = compaction_plan(&mut journal);
    let before = plan.before().clone();
    journal.test_fault("before-current", false);
    assert!(journal.commit_prepared_native_append(&plan).is_err());
    assert_eq!(pin(&journal), before);
    let retry = journal
        .prepare_native_append(plan.mutations().to_vec())
        .unwrap();
    assert_eq!(retry.before(), &before);
    assert!(
        retry.after().publication.generation > plan.after().publication.generation,
        "an orphan cannot be reused as an assumed candidate"
    );
    journal.commit_prepared_native_append(&retry).unwrap();
}
fn save_pin(path: &Path, pin: &NativeAppendPin) {
    let p = &pin.publication;
    std::fs::write(
        path,
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n",
            p.generation,
            p.parent.map_or("none".into(), |v| v.to_string()),
            p.physical_creation_sequence,
            p.creation_sequence,
            p.manifest_sha256d
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>(),
            pin.logical_sequence
        ),
    )
    .unwrap();
}
fn load_pin(path: &Path) -> NativeAppendPin {
    let raw = std::fs::read_to_string(path).unwrap();
    let lines = raw.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 6);
    let bytes = (0..32)
        .map(|i| u8::from_str_radix(&lines[4][i * 2..i * 2 + 2], 16).unwrap())
        .collect::<Vec<_>>();
    NativeAppendPin {
        publication: CatalogPublication {
            generation: lines[0].parse().unwrap(),
            parent: if lines[1] == "none" {
                None
            } else {
                Some(lines[1].parse().unwrap())
            },
            physical_creation_sequence: lines[2].parse().unwrap(),
            creation_sequence: lines[3].parse().unwrap(),
            manifest_sha256d: bytes.try_into().unwrap(),
        },
        logical_sequence: lines[5].parse().unwrap(),
    }
}
#[test]
#[ignore]
fn prepared_append_child() {
    let root = PathBuf::from(std::env::var("TL_APPEND_CHILD_ROOT").unwrap());
    let before = load_pin(&root.join("expected-before"));
    let after = load_pin(&root.join("expected-after"));
    let phase = std::env::var("TL_APPEND_CHILD_PHASE").unwrap();
    let (mut journal, _) = CompactingJournal::open_pinned(
        &root,
        namespace(),
        Recovery::RejectIncompleteTail,
        &before.publication,
        before.logical_sequence,
    )
    .unwrap();
    journal
        .configure(CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 0,
        })
        .unwrap();
    let mut plan = journal
        .prepare_native_append(put(&vec![72; 32000]))
        .unwrap();
    assert_eq!(plan.after(), &after);
    assert!(plan.intermediate().is_some());
    match phase.as_str() {
        "before-current" | "after-current-sync" => journal.test_fault(&phase, true),
        "partial-append" => plan.test_append_fault("partial"),
        "synced-error" => plan.test_append_fault("synced"),
        "after-append" => (),
        _ => panic!("phase"),
    }
    let result = journal.commit_prepared_native_append(&plan);
    match phase.as_str() {
        "partial-append" | "synced-error" => {
            assert!(result.is_err());
            std::process::exit(75)
        }
        "after-append" => {
            result.unwrap();
            std::process::exit(74)
        }
        _ => panic!("compaction crash hook did not exit"),
    }
}
#[test]
fn real_child_crashes_recover_only_before_intermediate_or_prepared_after() {
    for phase in [
        "before-current",
        "after-current-sync",
        "partial-append",
        "synced-error",
        "after-append",
    ] {
        let temp = Temp::new();
        let mut journal = journal(&temp);
        let _trigger = compaction_plan(&mut journal);
        let plan = journal
            .prepare_native_append(put(&vec![72; 32000]))
            .unwrap();
        let before = plan.before().clone();
        let intermediate = plan.intermediate().unwrap();
        let after = plan.after().clone();
        save_pin(&temp.path.join("expected-before"), &before);
        save_pin(&temp.path.join("expected-after"), &after);
        drop(journal);
        let child_name = format!(
            "{}::prepared_append_child",
            module_path!().split_once("::").unwrap().1
        );
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &child_name, "--ignored", "--nocapture"])
            .env("TL_APPEND_CHILD_ROOT", &temp.path)
            .env("TL_APPEND_CHILD_PHASE", phase)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(if phase == "after-append" {
                74
            } else if phase == "partial-append" || phase == "synced-error" {
                75
            } else {
                73
            }),
            "{phase}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let selected = if phase == "before-current" {
            &before
        } else if phase == "after-current-sync" || phase == "partial-append" {
            &intermediate
        } else {
            &after
        };
        let wrong = if selected == &after {
            &intermediate
        } else {
            &after
        };
        let files = tree(&temp.path);
        assert!(
            CompactingJournal::open_pinned(
                &temp.path,
                namespace(),
                Recovery::DiscardIncompleteTail,
                &wrong.publication,
                wrong.logical_sequence
            )
            .is_err(),
            "{phase} wrong pin"
        );
        assert_eq!(tree(&temp.path), files, "{phase} rejection must not repair");
        let (reopened, report) = CompactingJournal::open_pinned(
            &temp.path,
            namespace(),
            Recovery::DiscardIncompleteTail,
            &selected.publication,
            selected.logical_sequence,
        )
        .unwrap();
        assert_eq!(pin(&reopened), *selected);
        assert_eq!(
            reopened.get("row").unwrap(),
            Some(vec![if selected == &after { 72 } else { 71 }; 32000].as_slice())
        );
        assert_eq!(report.discarded_bytes > 0, phase == "partial-append");
    }
}
