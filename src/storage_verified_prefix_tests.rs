use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("TL_VERIFIED_PREFIX_TMP")
            .map_or_else(std::env::temp_dir, PathBuf::from);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "verified-kv-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(path) = self.root.canonicalize() {
            assert_eq!(path.parent(), Some(self.parent.as_path()));
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
fn namespace() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn put(key: &str, value: &[u8]) -> Mutation {
    Mutation::Put {
        key: key.into(),
        value: value.to_vec(),
    }
}
fn journal(temp: &Temp, values: Vec<Mutation>) -> Journal {
    let (mut journal, _) =
        Journal::open(&temp.root, namespace(), Recovery::RejectIncompleteTail).unwrap();
    journal.commit(values).unwrap();
    journal
}
fn tail(path: &Path) {
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(b"TXN1short").unwrap();
    file.sync_all().unwrap();
}
fn unchanged(path: &Path, bytes: &[u8]) {
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn verified_matching_prefix_repairs_but_all_receipt_and_sequence_substitutions_refuse() {
    let temp = Temp::new();
    let journal = journal(
        &temp,
        vec![put("selected", b"known"), put("unrelated", b"retained")],
    );
    let pin = journal.verified_prefix_pin().unwrap();
    let path = journal.path().to_owned();
    drop(journal);
    tail(&path);
    let bytes = std::fs::read(&path).unwrap();
    for mode in 0..8 {
        let mut wrong = pin.clone();
        match mode {
            0 => wrong.physical_sequence = 0,
            1 => wrong.physical_sequence = 2,
            2 => wrong.content.complete = false,
            3 => wrong.content.profile.push('x'),
            4 => wrong.content.keys += 1,
            5 => wrong.content.bytes += 1,
            6 => wrong.content.chain_sha256d[0] ^= 1,
            _ => wrong.profile.push('x'),
        }
        assert!(Journal::open_existing_verified_prefix(
            &temp.root,
            namespace(),
            Recovery::DiscardIncompleteTail,
            &wrong
        )
        .is_err());
        unchanged(&path, &bytes);
    }
    assert!(matches!(
        Journal::open_existing_verified_prefix(
            &temp.root,
            namespace(),
            Recovery::RejectIncompleteTail,
            &pin
        ),
        Err(StorageError::IncompleteTail { .. })
    ));
    unchanged(&path, &bytes);
    let (repaired, report) = Journal::open_existing_verified_prefix(
        &temp.root,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &pin,
    )
    .unwrap();
    assert_eq!(report.discarded_bytes, 9);
    assert_eq!(repaired.complete_kv_receipt().unwrap(), pin.content);
    assert_eq!(
        repaired.get("unrelated").unwrap(),
        Some(b"retained".as_slice())
    );
}

#[test]
fn same_sequence_recomputed_crc_mutations_and_extra_unselected_keys_cannot_authorize_repair() {
    for values in [
        vec![put("selected", b"changed"), put("unrelated", b"retained")],
        vec![
            put("selected", b"known"),
            put("unrelated", b"retained"),
            put("extra", b"unknown"),
        ],
    ] {
        let temp = Temp::new();
        let original = journal(
            &temp,
            vec![put("selected", b"known"), put("unrelated", b"retained")],
        );
        let pin = original.verified_prefix_pin().unwrap();
        let path = original.path().to_owned();
        drop(original);
        let changed = Temp::new();
        let alternative = journal(&changed, values);
        let alternate_path = alternative.path().to_owned();
        drop(alternative);
        std::fs::write(&path, std::fs::read(alternate_path).unwrap()).unwrap();
        tail(&path);
        let bytes = std::fs::read(&path).unwrap();
        assert!(matches!(
            Journal::open_existing_verified_prefix(
                &temp.root,
                namespace(),
                Recovery::DiscardIncompleteTail,
                &pin
            ),
            Err(StorageError::Corrupt(
                "unexpected independently pinned complete KV prefix"
            ))
        ));
        unchanged(&path, &bytes);
    }
}

#[test]
fn foreign_location_or_namespace_refuses_even_with_equal_sequence_and_full_kv() {
    let a = Temp::new();
    let b = Temp::new();
    let left = journal(&a, vec![put("same", b"value")]);
    let right = journal(&b, vec![put("same", b"value")]);
    let pin = left.verified_prefix_pin().unwrap();
    let path = right.path().to_owned();
    drop(left);
    drop(right);
    tail(&path);
    let bytes = std::fs::read(&path).unwrap();
    assert!(Journal::open_existing_verified_prefix(
        &b.root,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &pin
    )
    .is_err());
    unchanged(&path, &bytes);
    let mut wrong = pin;
    wrong.namespace = Namespace::new("LTC", false, "consensus").unwrap();
    assert!(Journal::open_existing_verified_prefix(
        &b.root,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &wrong
    )
    .is_err());
    unchanged(&path, &bytes);
}

#[test]
fn same_exclusive_file_lock_spans_full_kv_check_repair_and_returned_owner() {
    let temp = Temp::new();
    let live = journal(&temp, vec![put("a", b"value")]);
    let pin = live.verified_prefix_pin().unwrap();
    let path = live.path().to_owned();
    drop(live);
    tail(&path);
    let root = temp.root.clone();
    let expected = pin.clone();
    let calls = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = calls.clone();
    VERIFIED_PREFIX_TEST_HOOK.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(Box::new(move |locked| {
            assert_eq!(locked.sequence().unwrap(), expected.physical_sequence);
            assert_eq!(locked.complete_kv_receipt().unwrap(), expected.content);
            assert!(
                locked.file.metadata().unwrap().len() > locked.end,
                "hook precedes actual truncate"
            );
            let contender = std::thread::spawn(move || {
                Journal::open_existing(&root, namespace(), Recovery::RejectIncompleteTail)
                    .map(|_| ())
            });
            assert!(
                matches!(contender.join().unwrap(), Err(StorageError::Locked)),
                "no read/drop/reopen gap before repair"
            );
            observed.store(true, Ordering::SeqCst);
        }));
    });
    let (locked, report) = Journal::open_existing_verified_prefix(
        &temp.root,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &pin,
    )
    .unwrap();
    assert!(calls.load(Ordering::SeqCst));
    assert_eq!(report.discarded_bytes, 9);
    assert!(matches!(
        Journal::open_existing(&temp.root, namespace(), Recovery::RejectIncompleteTail),
        Err(StorageError::Locked)
    ));
    drop(locked);
    let (mut next, _) =
        Journal::open_existing(&temp.root, namespace(), Recovery::RejectIncompleteTail).unwrap();
    next.commit(vec![put("a", b"new")]).unwrap();
    drop(next);
    tail(&path);
    let bytes = std::fs::read(&path).unwrap();
    assert!(Journal::open_existing_verified_prefix(
        &temp.root,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &pin
    )
    .is_err());
    unchanged(&path, &bytes);
}

#[test]
fn complete_kv_receipt_matches_independent_sha256_vectors_and_immutable_snapshots() {
    let temp = Temp::new();
    let (mut live, _) =
        Journal::open(&temp.root, namespace(), Recovery::RejectIncompleteTail).unwrap();
    let to_hex = |raw: [u8; 32]| raw.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        to_hex(live.complete_kv_receipt().unwrap().chain_sha256d),
        "a2f10af3c03188ce479fd367629a7923fb6d6149719af4c48c72b670274452f9"
    );
    live.commit(vec![
        put("é", b"utf8"),
        put("a\0b", &[0, 255]),
        put("a", b""),
    ])
    .unwrap();
    let receipt = live.complete_kv_receipt().unwrap();
    assert_eq!(
        (receipt.keys, receipt.bytes, receipt.complete),
        (3, 12, true)
    );
    assert_eq!(
        to_hex(receipt.chain_sha256d),
        "77fc85c6223148dc208f17c72350513b9415f8370d41b23a0bedb8815f9efb92"
    );
    let captured = live.read_snapshot().unwrap();
    live.commit(vec![put("a", b"changed")]).unwrap();
    assert_eq!(captured.complete_kv_receipt().unwrap(), receipt);
    assert_ne!(live.complete_kv_receipt().unwrap(), receipt);
}

#[test]
fn old_journal_api_keeps_immutable_provider_bytes_and_recovery_behavior() {
    // Captured by capture_old_journal.rs linked to immutable 8bccb174...,
    // independently of this crate; these are not self-comparison goldens.
    const BEFORE: &[u8] = include_bytes!("../fixtures/verified_prefix_old_journal_v1.before.bin");
    const DAMAGED: &[u8] = include_bytes!("../fixtures/verified_prefix_old_journal_v1.damaged.bin");
    const AFTER: &[u8] = include_bytes!("../fixtures/verified_prefix_old_journal_v1.after.bin");
    const OBSERVATIONS: &str =
        include_str!("../fixtures/verified_prefix_old_journal_v1.observations.tsv");
    let a = Temp::new();
    let (mut ours, initial_report) =
        Journal::open(&a.root, namespace(), Recovery::RejectIncompleteTail).unwrap();
    ours.commit(vec![put("b", b"second"), put("a", b"first")])
        .unwrap();
    ours.commit(vec![
        Mutation::Delete { key: "a".into() },
        put("c", b"third"),
    ])
    .unwrap();
    let ours_path = ours.path().to_owned();
    let before_sequence = ours.sequence().unwrap();
    let before_rows: Vec<_> = ours
        .entries()
        .unwrap()
        .map(|(k, v)| (k.to_owned(), v.to_vec()))
        .collect();
    drop(ours);
    assert_eq!(std::fs::read(&ours_path).unwrap(), BEFORE);
    tail(&ours_path);
    assert_eq!(std::fs::read(&ours_path).unwrap(), DAMAGED);
    let (restored, report) =
        Journal::open_existing(&a.root, namespace(), Recovery::DiscardIncompleteTail).unwrap();
    let mut observations = format!("initial_discarded_bytes\t{}\nbefore_sequence\t{}\ndiscarded_bytes\t{}\nafter_sequence\t{}\n", initial_report.discarded_bytes, before_sequence, report.discarded_bytes, restored.sequence().unwrap());
    let hex = |raw: &[u8]| raw.iter().map(|b| format!("{b:02x}")).collect::<String>();
    for (key, value) in before_rows {
        observations.push_str(&format!(
            "before_row\t{}\t{}\n",
            hex(key.as_bytes()),
            hex(&value)
        ));
    }
    for (key, value) in restored.entries().unwrap() {
        observations.push_str(&format!(
            "after_row\t{}\t{}\n",
            hex(key.as_bytes()),
            hex(value)
        ));
    }
    assert_eq!(observations, OBSERVATIONS);
    drop(restored);
    assert_eq!(std::fs::read(ours_path).unwrap(), AFTER);
}

#[test]
fn verified_open_never_repairs_a_corrupt_complete_frame() {
    let temp = Temp::new();
    let live = journal(&temp, vec![put("a", b"known")]);
    let pin = live.verified_prefix_pin().unwrap();
    let path = live.path().to_owned();
    drop(live);
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    bytes.extend_from_slice(b"short");
    std::fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        Journal::open_existing_verified_prefix(
            &temp.root,
            namespace(),
            Recovery::DiscardIncompleteTail,
            &pin
        ),
        Err(StorageError::Corrupt(_))
    ));
    unchanged(&path, &bytes);
}

#[test]
fn compact_verified_open_checks_context_publication_and_logical_physical_mapping_before_repair() {
    use crate::compact_journal::CompactingJournal;
    let temp = Temp::new();
    let (mut live, _) =
        CompactingJournal::open(&temp.root, namespace(), Recovery::RejectIncompleteTail).unwrap();
    live.commit(vec![put("a", b"known")]).unwrap();
    live.commit(vec![put("b", b"also known")]).unwrap();
    let old = live.verified_prefix_pin().unwrap();
    let plan = live.prepare_compaction().unwrap();
    live.publish_compaction(&plan).unwrap();
    let pin = live.verified_prefix_pin().unwrap();
    let path = live.path().to_owned();
    assert_eq!(pin.logical_sequence, 2);
    assert_eq!(pin.publication.creation_sequence, 2);
    assert_eq!(pin.publication.physical_creation_sequence, 1);
    assert_eq!(pin.content, old.content);
    drop(live);
    tail(&path);
    let bytes = std::fs::read(&path).unwrap();
    for mode in 0..6 {
        let mut wrong = pin.clone();
        match mode {
            0 => wrong = old.clone(),
            1 => wrong.logical_sequence = 3,
            2 => wrong.logical_sequence = 0,
            3 => wrong.persistent_identity.push('x'),
            4 => wrong.canonical_catalog.push("foreign"),
            _ => wrong.content.chain_sha256d[0] ^= 1,
        };
        assert!(CompactingJournal::open_existing_verified_prefix(
            &temp.root,
            namespace(),
            Recovery::DiscardIncompleteTail,
            &wrong
        )
        .is_err());
        unchanged(&path, &bytes);
    }
    let (repaired, report) = CompactingJournal::open_existing_verified_prefix(
        &temp.root,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &pin,
    )
    .unwrap();
    assert_eq!(report.discarded_bytes, 9);
    assert_eq!(repaired.sequence().unwrap(), 2);
    assert_eq!(repaired.verified_prefix_pin().unwrap(), pin);
}

#[test]
fn compact_same_publication_and_sequence_but_forged_valid_crc_content_refuses_without_repair() {
    use crate::compact_journal::CompactingJournal;
    let temp = Temp::new();
    let (mut live, _) =
        CompactingJournal::open(&temp.root, namespace(), Recovery::RejectIncompleteTail).unwrap();
    live.commit(vec![put("a", b"known")]).unwrap();
    let pin = live.verified_prefix_pin().unwrap();
    let path = live.path().to_owned();
    drop(live);
    let other = Temp::new();
    let attacker = journal(
        &other,
        vec![put("a", b"wrong"), put("outside-selected-row", b"extra")],
    );
    let other_path = attacker.path().to_owned();
    drop(attacker);
    std::fs::write(&path, std::fs::read(other_path).unwrap()).unwrap();
    tail(&path);
    let bytes = std::fs::read(&path).unwrap();
    assert!(matches!(
        CompactingJournal::open_existing_verified_prefix(
            &temp.root,
            namespace(),
            Recovery::DiscardIncompleteTail,
            &pin
        ),
        Err(StorageError::Corrupt(
            "unexpected independently pinned complete KV prefix"
        ))
    ));
    unchanged(&path, &bytes);
}

#[test]
fn checked_compact_partial_append_requires_actual_complete_before_prefix() {
    use crate::compact_journal::CompactingJournal;
    let temp = Temp::new();
    let (mut live, _) =
        CompactingJournal::open(&temp.root, namespace(), Recovery::RejectIncompleteTail).unwrap();
    live.commit(vec![put("a", b"before")]).unwrap();
    let pin = live.verified_prefix_pin().unwrap();
    let path = live.path().to_owned();
    live.test_fail_after_partial_write();
    assert!(live.commit(vec![put("b", b"uncommitted")]).is_err());
    assert!(matches!(
        live.complete_kv_receipt(),
        Err(StorageError::Poisoned)
    ));
    drop(live);
    let bytes = std::fs::read(&path).unwrap();
    let mut wrong = pin.clone();
    wrong.content.chain_sha256d[0] ^= 1;
    assert!(CompactingJournal::open_existing_verified_prefix(
        &temp.root,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &wrong
    )
    .is_err());
    unchanged(&path, &bytes);
    let (restored, report) = CompactingJournal::open_existing_verified_prefix(
        &temp.root,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &pin,
    )
    .unwrap();
    assert!(report.discarded_bytes > 0);
    assert_eq!(restored.sequence().unwrap(), 1);
    assert_eq!(restored.get("b").unwrap(), None);
}

#[test]
fn synced_complete_append_is_not_rolled_back_to_a_stale_before_pin() {
    use crate::compact_journal::CompactingJournal;
    let temp = Temp::new();
    let (mut live, _) =
        CompactingJournal::open(&temp.root, namespace(), Recovery::RejectIncompleteTail).unwrap();
    live.commit(vec![put("a", b"before")]).unwrap();
    let before = live.verified_prefix_pin().unwrap();
    let path = live.path().to_owned();
    // Expected post is independently computed by actual metadata mutations,
    // before opening the uncertain original; no financial state is supplied.
    let oracle = Temp::new();
    let mut expected = journal(&oracle, vec![put("a", b"before")]);
    expected.commit(vec![put("b", b"post")]).unwrap();
    let post_content = expected.complete_kv_receipt().unwrap();
    drop(expected);
    live.test_fail_after_sync();
    assert!(live.commit(vec![put("b", b"post")]).is_err());
    drop(live);
    tail(&path);
    let bytes = std::fs::read(&path).unwrap();
    assert!(CompactingJournal::open_existing_verified_prefix(
        &temp.root,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &before
    )
    .is_err());
    unchanged(&path, &bytes);
    let mut post = before;
    post.logical_sequence = 2;
    post.content = post_content;
    let (restored, report) = CompactingJournal::open_existing_verified_prefix(
        &temp.root,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &post,
    )
    .unwrap();
    assert_eq!(report.discarded_bytes, 9);
    assert_eq!(restored.sequence().unwrap(), 2);
    assert_eq!(restored.get("b").unwrap(), Some(b"post".as_slice()));
}

#[test]
fn unpublished_compaction_generation_never_replaces_the_independently_pinned_current() {
    use crate::compact_journal::CompactingJournal;
    let temp = Temp::new();
    let (mut live, _) =
        CompactingJournal::open(&temp.root, namespace(), Recovery::RejectIncompleteTail).unwrap();
    live.commit(vec![put("a", b"before")]).unwrap();
    let before = live.verified_prefix_pin().unwrap();
    let path = live.path().to_owned();
    let plan = live.prepare_compaction().unwrap();
    let mut unpublished = before.clone();
    unpublished.publication = plan.new_publication().clone();
    live.test_fault("before-current", false);
    assert!(live.publish_compaction(&plan).is_err());
    drop(live);
    tail(&path);
    let bytes = std::fs::read(&path).unwrap();
    assert!(CompactingJournal::open_existing_verified_prefix(
        &temp.root,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &unpublished
    )
    .is_err());
    unchanged(&path, &bytes);
    let (restored, report) = CompactingJournal::open_existing_verified_prefix(
        &temp.root,
        namespace(),
        Recovery::DiscardIncompleteTail,
        &before,
    )
    .unwrap();
    assert_eq!(report.discarded_bytes, 9);
    assert_eq!(restored.verified_prefix_pin().unwrap(), before);
}
