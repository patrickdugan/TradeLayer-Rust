use crate::compact_journal::*;
use crate::storage::{Mutation, Namespace, Recovery, StorageError};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "tl-compact-gen-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let base = std::env::temp_dir().canonicalize().unwrap();
        if let Ok(target) = self.0.canonicalize() {
            assert_eq!(target.parent(), Some(base.as_path()));
            assert!(target
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("tl-compact-gen-"));
            let _ = fs::remove_dir_all(target);
        }
    }
}
fn ns() -> Namespace {
    Namespace::new("BTC", true, "txIndex").unwrap()
}
fn put(k: &str, v: &[u8]) -> Vec<Mutation> {
    vec![Mutation::Put {
        key: k.into(),
        value: v.into(),
    }]
}
fn open(t: &Temp) -> CompactingJournal {
    CompactingJournal::open(&t.0, ns(), Recovery::RejectIncompleteTail)
        .unwrap()
        .0
}

#[test]
fn fixed_read_generation_survives_updates_compaction_and_owner_reopen() {
    let t = Temp::new();
    let mut j = open(&t);
    j.configure(CompactionConfig {
        target_log_bytes: 65536,
        retained_ancestors: 0,
    })
    .unwrap();
    j.commit(put("row", b"old")).unwrap();
    let fixed = j.read_snapshot().unwrap();
    j.commit(put("row", b"new")).unwrap();
    let plan = j.prepare_compaction().unwrap();
    let expected = plan.new_publication().clone();
    let report = j.publish_compaction(&plan).unwrap();
    assert_eq!(report.logical_sequence, 2);
    assert_eq!(j.sequence().unwrap(), 2);
    assert_eq!(j.get("row").unwrap(), Some(b"new".as_slice()));
    assert_eq!(fixed.get("row"), Some(b"old".as_slice()));
    drop(plan);
    assert!(j.prune_retention().unwrap().1.contains(&0));
    drop(j);
    let mut j = open(&t);
    j.configure(CompactionConfig {
        target_log_bytes: 65536,
        retained_ancestors: 0,
    })
    .unwrap();
    assert_eq!(j.publication().unwrap(), &expected);
    // This old in-memory snapshot still holds a real shared OS read lease across
    // writer drop/reopen; a new owner's empty Weak map cannot hide it from GC.
    assert!(j.prune_retention().unwrap().1.contains(&0));
    assert_eq!(fixed.get("row"), Some(b"old".as_slice()));
    drop(fixed);
    assert!(j.prune_retention().unwrap().0.contains(&0));
    assert_eq!(j.footprint().unwrap().generation_count, 1);
}

#[test]
fn compaction_keeps_logical_sequence_and_continues_past_old_log_growth() {
    let t = Temp::new();
    let mut j = open(&t);
    j.configure(CompactionConfig {
        target_log_bytes: 65536,
        retained_ancestors: 1,
    })
    .unwrap();
    let mut expected = vec![0; 4096];
    j.commit(put("data", &expected)).unwrap();
    for n in 0..160 {
        expected[0] = n as u8;
        assert_eq!(j.commit(put("data", &expected)).unwrap(), n + 2);
    }
    let f = j.footprint().unwrap();
    assert!(f.active_generation > 2);
    assert!(f.active_log_bytes < 131072);
    assert!(f.retained_disk_bytes < 300000);
    assert!(f.generation_count <= 2);
    assert_eq!(j.get("data").unwrap(), Some(expected.as_slice()));
    let sequence = j.sequence().unwrap();
    drop(j);
    let j = open(&t);
    assert_eq!(j.sequence().unwrap(), sequence);
    assert_eq!(j.get("data").unwrap(), Some(expected.as_slice()));
}

#[test]
fn stale_intent_and_invalid_mutation_do_not_publish() {
    let t = Temp::new();
    let mut j = open(&t);
    j.commit(put("a", b"1")).unwrap();
    let plan = j.prepare_compaction().unwrap();
    j.commit(put("b", b"2")).unwrap();
    let before = j.publication().unwrap().clone();
    assert!(matches!(
        j.publish_compaction(&plan),
        Err(StorageError::Corrupt("stale compaction intent"))
    ));
    assert_eq!(j.publication().unwrap(), &before);
    assert!(matches!(j.commit(vec![]), Err(StorageError::EmptyBatch)));
    assert_eq!(j.publication().unwrap(), &before);
    assert!(matches!(
        j.commit(vec![
            Mutation::Put {
                key: "same".into(),
                value: vec![]
            },
            Mutation::Delete { key: "same".into() }
        ]),
        Err(StorageError::DuplicateKey)
    ));
    assert_eq!(j.sequence().unwrap(), 2);
}

#[test]
fn compaction_plan_and_pins_are_bound_to_the_exact_origin_store() {
    let a = Temp::new();
    let b = Temp::new();
    let mut left = open(&a);
    let mut right = open(&b);
    left.commit(put("row", b"left-store")).unwrap();
    right.commit(put("row", b"right-store")).unwrap();
    let plan = left.prepare_compaction().unwrap();
    let left_pin = left.publication().unwrap().clone();
    let right_pin = right.publication().unwrap().clone();
    assert_ne!(
        left_pin, right_pin,
        "persistent catalog identities must distinguish equal counters/namespaces"
    );
    let before = right.footprint().unwrap();
    assert!(matches!(
        right.publish_compaction(&plan),
        Err(StorageError::Corrupt("stale compaction intent"))
    ));
    assert_eq!(right.get("row").unwrap(), Some(b"right-store".as_slice()));
    assert_eq!(
        right.footprint().unwrap().generation_count,
        before.generation_count
    );
    drop(right);
    assert!(CompactingJournal::open_pinned(
        &b.0,
        ns(),
        Recovery::RejectIncompleteTail,
        &left_pin,
        1
    )
    .is_err());
    drop(left);
    let mut left = open(&a);
    assert_eq!(left.publication().unwrap(), &left_pin);
    assert!(
        matches!(
            left.publish_compaction(&plan),
            Err(StorageError::Corrupt("stale compaction intent"))
        ),
        "a sealed plan belongs to its live owner, not a later reopen"
    );
    assert_eq!(left.get("row").unwrap(), Some(b"left-store".as_slice()));
}

#[test]
fn rejected_foreign_and_stale_pins_cannot_repair_or_truncate_a_partial_tail() {
    fn files(root: &std::path::Path) -> Vec<(PathBuf, Vec<u8>, std::time::SystemTime)> {
        fn visit(
            root: &std::path::Path,
            path: &std::path::Path,
            out: &mut Vec<(PathBuf, Vec<u8>, std::time::SystemTime)>,
        ) {
            for e in fs::read_dir(path).unwrap() {
                let e = e.unwrap();
                let p = e.path();
                if e.file_type().unwrap().is_dir() {
                    visit(root, &p, out);
                } else {
                    out.push((
                        p.strip_prefix(root).unwrap().to_path_buf(),
                        fs::read(&p).unwrap(),
                        fs::metadata(&p).unwrap().modified().unwrap(),
                    ));
                }
            }
        }
        let mut out = vec![];
        visit(root, root, &mut out);
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
    let a = Temp::new();
    let b = Temp::new();
    let mut left = open(&a);
    left.commit(put("row", b"A")).unwrap();
    let foreign = left.publication().unwrap().clone();
    drop(left);
    let mut right = open(&b);
    right.commit(put("row", b"B")).unwrap();
    let pin = right.publication().unwrap().clone();
    let data = right.path().to_path_buf();
    right.test_fail_after_partial_write();
    assert!(right.commit(put("second", b"incomplete")).is_err());
    drop(right);
    let before = files(&b.0);
    let length = fs::metadata(&data).unwrap().len();
    assert!(CompactingJournal::open_pinned(
        &b.0,
        ns(),
        Recovery::DiscardIncompleteTail,
        &foreign,
        1
    )
    .is_err());
    assert_eq!(
        files(&b.0),
        before,
        "foreign publication must be rejected before recovery mutation"
    );
    for stale in [0, 2] {
        assert!(CompactingJournal::open_pinned(
            &b.0,
            ns(),
            Recovery::DiscardIncompleteTail,
            &pin,
            stale
        )
        .is_err());
        assert_eq!(
            files(&b.0),
            before,
            "logical pin {stale} must be checked against the valid committed prefix before repair"
        );
    }
    let (repaired, report) =
        CompactingJournal::open_pinned(&b.0, ns(), Recovery::DiscardIncompleteTail, &pin, 1)
            .unwrap();
    assert!(report.discarded_bytes > 0);
    assert_eq!(
        fs::metadata(&data).unwrap().len() + report.discarded_bytes,
        length
    );
    assert_eq!(repaired.get("row").unwrap(), Some(b"B".as_slice()));
    assert!(repaired.get("second").unwrap().is_none());
}

#[test]
fn sorted_copy_packing_has_independent_physical_and_logical_counters() {
    let t = Temp::new();
    let mut j = open(&t);
    let mib = 1024 * 1024;
    j.commit(vec![
        Mutation::Put {
            key: "a".into(),
            value: vec![1; 40 * mib],
        },
        Mutation::Put {
            key: "c".into(),
            value: vec![3; 22 * mib],
        },
    ])
    .unwrap();
    j.commit(vec![
        Mutation::Put {
            key: "b".into(),
            value: vec![2; 40 * mib],
        },
        Mutation::Put {
            key: "d".into(),
            value: vec![4; 22 * mib],
        },
    ])
    .unwrap();
    assert_eq!(j.sequence().unwrap(), 2);
    let plan = j.prepare_compaction().unwrap();
    assert_eq!(plan.new_publication().creation_sequence, 2);
    assert_eq!(plan.new_publication().physical_creation_sequence, 3);
    let publication = plan.new_publication().clone();
    j.publish_compaction(&plan).unwrap();
    drop(plan);
    assert_eq!(j.sequence().unwrap(), 2);
    assert_eq!(j.get("a").unwrap().unwrap().len(), 40 * mib);
    assert_eq!(j.get("d").unwrap().unwrap()[0], 4);
    j.commit_if_sequence(2, put("e", b"after-three-physical-copy-batches"))
        .unwrap();
    drop(j);
    let (j, _) =
        CompactingJournal::open_pinned(&t.0, ns(), Recovery::RejectIncompleteTail, &publication, 3)
            .unwrap();
    assert_eq!(
        j.get("e").unwrap(),
        Some(b"after-three-physical-copy-batches".as_slice())
    );
    assert_eq!(j.sequence().unwrap(), 3);
}

#[test]
fn generation_partial_failure_retries_without_treating_orphan_as_active() {
    let t = Temp::new();
    let mut j = open(&t);
    j.commit(put("a", b"original")).unwrap();
    let old = j.publication().unwrap().clone();
    let plan = j.prepare_compaction().unwrap();
    j.test_fault("after-generation-sync", false);
    assert!(j.publish_compaction(&plan).is_err());
    assert_eq!(j.publication().unwrap(), &old);
    assert_eq!(j.get("a").unwrap(), Some(b"original".as_slice()));
    drop(plan);
    let plan = j.prepare_compaction().unwrap();
    assert!(plan.new_publication().generation > 1);
    j.publish_compaction(&plan).unwrap();
    drop(plan);
    j.prune_retention().unwrap();
    assert_eq!(j.footprint().unwrap().generation_count, 2);
}

#[test]
fn post_current_failure_poison_and_reopen_exact_candidate() {
    let t = Temp::new();
    let mut j = open(&t);
    j.commit(put("a", b"one")).unwrap();
    let plan = j.prepare_compaction().unwrap();
    let candidate = plan.new_publication().clone();
    j.test_fault("after-current", false);
    assert!(j.publish_compaction(&plan).is_err());
    assert!(matches!(j.get("a"), Err(StorageError::Poisoned)));
    assert_eq!(j.pending_publication(), Some(&candidate));
    drop(j);
    let mut j = open(&t);
    assert_eq!(j.publication().unwrap(), &candidate);
    assert_eq!(j.sequence().unwrap(), 1);
    assert_eq!(j.get("a").unwrap(), Some(b"one".as_slice()));
    assert_eq!(j.commit(put("b", b"two")).unwrap(), 2);
}

#[test]
fn native_partial_write_and_lost_ack_reconcile_with_exact_pin_and_cas() {
    let t = Temp::new();
    let mut j = open(&t);
    j.commit(put("old", b"retained")).unwrap();
    let pin = j.publication().unwrap().clone();
    j.test_fail_after_partial_write();
    assert!(j.commit_if_sequence(1, put("new", b"partial")).is_err());
    assert!(matches!(j.get("old"), Err(StorageError::Poisoned)));
    drop(j);
    assert!(matches!(
        CompactingJournal::open_pinned(&t.0, ns(), Recovery::RejectIncompleteTail, &pin, 1),
        Err(StorageError::IncompleteTail { .. })
    ));
    let (mut j, repair) =
        CompactingJournal::open_pinned(&t.0, ns(), Recovery::DiscardIncompleteTail, &pin, 1)
            .unwrap();
    assert!(repair.discarded_bytes > 0);
    assert!(j.get("new").unwrap().is_none());
    j.test_fail_after_sync();
    assert!(j
        .commit_if_sequence(1, put("new", b"actually-synced"))
        .is_err());
    assert!(j.read_snapshot().is_err());
    drop(j);
    assert!(
        CompactingJournal::open_pinned(&t.0, ns(), Recovery::RejectIncompleteTail, &pin, 1)
            .is_err()
    );
    let (mut j, _) =
        CompactingJournal::open_pinned(&t.0, ns(), Recovery::RejectIncompleteTail, &pin, 2)
            .unwrap();
    assert_eq!(j.get("new").unwrap(), Some(b"actually-synced".as_slice()));
    assert!(matches!(
        j.commit_if_sequence(1, put("new", b"duplicate-retry")),
        Err(StorageError::Corrupt("stale logical mutation"))
    ));
    assert_eq!(j.sequence().unwrap(), 2);
}

#[test]
#[ignore]
fn compaction_process_exit_child() {
    let Some(root) = std::env::var_os("TL_COMPACT_CRASH_ROOT") else {
        return;
    };
    let point = std::env::var("TL_COMPACT_CRASH_POINT").unwrap();
    let (mut j, _) = CompactingJournal::open_existing(
        &PathBuf::from(root),
        ns(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    let plan = j.prepare_compaction().unwrap();
    j.test_fault(&point, true);
    j.publish_compaction(&plan).unwrap();
    panic!("configured crash was not reached");
}

#[test]
fn actual_process_exit_matrix_generation_current_and_retention() {
    for point in [
        "before-generation",
        "during-generation",
        "after-generation-sync",
        "before-current",
        "after-current",
        "after-current-sync",
        "before-retention",
        "during-retention",
        "after-retention",
    ] {
        let t = Temp::new();
        let mut j = open(&t);
        for range in [0..1024, 1024..1100] {
            j.commit(
                range
                    .map(|n| Mutation::Put {
                        key: format!("k{n:04}"),
                        value: format!("v{n:04}").into_bytes(),
                    })
                    .collect(),
            )
            .unwrap();
        }
        let plan = j.prepare_compaction().unwrap();
        j.publish_compaction(&plan).unwrap();
        drop(plan);
        let old = j.publication().unwrap().clone();
        let plan = j.prepare_compaction().unwrap();
        let candidate = plan.new_publication().clone();
        drop(plan);
        drop(j);
        let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            "compact_journal_tests::compaction_process_exit_child",
            "--ignored",
            "--test-threads=1",
        ]);
        cmd.env("TL_COMPACT_CRASH_ROOT", &t.0)
            .env("TL_COMPACT_CRASH_POINT", point);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let output = cmd.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(73),
            "{point}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let published = matches!(
            point,
            "after-current"
                | "after-current-sync"
                | "before-retention"
                | "during-retention"
                | "after-retention"
        );
        let expected = if published { &candidate } else { &old };
        let (mut j, _) =
            CompactingJournal::open_pinned(&t.0, ns(), Recovery::RejectIncompleteTail, expected, 2)
                .unwrap();
        assert_eq!(j.get("k0000").unwrap(), Some(b"v0000".as_slice()));
        assert_eq!(j.get("k1099").unwrap(), Some(b"v1099".as_slice()));
        j.commit_if_sequence(2, put("after-recovery", b"exactly-once"))
            .unwrap();
        let retry = j.prepare_compaction().unwrap();
        j.publish_compaction(&retry).unwrap();
        drop(retry);
        j.prune_retention().unwrap();
        assert_eq!(j.sequence().unwrap(), 3);
        assert!(j.footprint().unwrap().generation_count <= 2, "{point}");
    }
}
