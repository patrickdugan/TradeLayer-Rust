//! Crafted full-CRC frames distinguish incomplete-tail repair from corruption.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tradelayer_core::storage::{
    Journal, Mutation, Namespace, Recovery, StorageError, CATEGORIES, MAX_BATCH_BYTES,
    MAX_BATCH_OPERATIONS, MAX_KEY_BYTES, MAX_LOG_BYTES, MAX_VALUE_BYTES,
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "tl-storage-adversarial-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        // Remove only this fixture's known file and empty directories.
        let file = namespace().path(&self.0);
        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir(file.parent().unwrap());
        let _ = fs::remove_dir(&self.0);
    }
}

fn namespace() -> Namespace {
    Namespace::new("LTC", true, "persistence").unwrap()
}
fn empty_store(dir: &TestDirectory) -> (PathBuf, Vec<u8>) {
    let (journal, report) =
        Journal::open(dir.path(), namespace(), Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(report.discarded_bytes, 0);
    let path = journal.path().to_path_buf();
    drop(journal);
    let bytes = fs::read(&path).unwrap();
    (path, bytes)
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
fn frame_header(length: u32, sequence: u64, previous_crc: u32) -> Vec<u8> {
    let mut bytes = b"TXN1".to_vec();
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(&sequence.to_be_bytes());
    bytes.extend_from_slice(&previous_crc.to_be_bytes());
    bytes.extend_from_slice(&crc32(&bytes).to_be_bytes());
    bytes
}
fn frame(payload: &[u8], sequence: u64, previous_crc: u32) -> Vec<u8> {
    let mut bytes = frame_header(payload.len() as u32, sequence, previous_crc);
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(&crc32(&bytes).to_be_bytes());
    bytes.extend_from_slice(b"DONE");
    bytes
}
fn put(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&(key.len() as u16).to_be_bytes());
    bytes.extend_from_slice(key);
    bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
    bytes.extend_from_slice(value);
    bytes
}
fn payload(operations: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = (operations.len() as u32).to_be_bytes().to_vec();
    for operation in operations {
        bytes.extend_from_slice(operation);
    }
    bytes
}

#[test]
fn every_incomplete_final_frame_prefix_recovers_previous_batch_atomically() {
    let directory = TestDirectory::new();
    let (path, header) = empty_store(&directory);
    let first = frame(&payload(&[put(b"keep", b"committed")]), 1, crc32(&header));
    let first_crc = u32::from_be_bytes(first[first.len() - 8..first.len() - 4].try_into().unwrap());
    let delete = [vec![0, 0, 4], b"keep".to_vec()].concat();
    let second = frame(&payload(&[put(b"added", b"new"), delete]), 2, first_crc);
    let committed_end = (header.len() + first.len()) as u64;
    for cut in 1..second.len() {
        let original = [header.as_slice(), first.as_slice(), &second[..cut]].concat();
        fs::write(&path, &original).unwrap();
        assert!(
            matches!(Journal::open(directory.path(), namespace(), Recovery::RejectIncompleteTail),
            Err(StorageError::IncompleteTail { offset }) if offset == committed_end),
            "prefix {cut}"
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            original,
            "reject mutated prefix {cut}"
        );
        let (mut journal, report) = Journal::open(
            directory.path(),
            namespace(),
            Recovery::DiscardIncompleteTail,
        )
        .unwrap();
        assert_eq!(report.discarded_bytes, cut as u64);
        assert_eq!(journal.sequence().unwrap(), 1);
        assert_eq!(journal.get("keep").unwrap(), Some(b"committed".as_slice()));
        assert_eq!(journal.get("added").unwrap(), None);
        assert_eq!(fs::metadata(&path).unwrap().len(), committed_end);
        journal
            .commit(vec![Mutation::Put {
                key: "after".into(),
                value: vec![cut as u8],
            }])
            .unwrap();
        drop(journal);
        let (journal, report) = Journal::open(
            directory.path(),
            namespace(),
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        assert_eq!(report.discarded_bytes, 0);
        assert_eq!(journal.sequence().unwrap(), 2);
        assert_eq!(journal.get("after").unwrap(), Some([cut as u8].as_slice()));
    }
}

#[test]
fn every_complete_frame_single_bit_corruption_rejects_without_repair() {
    let directory = TestDirectory::new();
    let (path, header) = empty_store(&directory);
    let batch = frame(&payload(&[put(b"key", b"value")]), 1, crc32(&header));
    for at in 0..batch.len() {
        for bit in 0..8 {
            let mut damaged = batch.clone();
            damaged[at] ^= 1 << bit;
            let bytes = [header.as_slice(), damaged.as_slice()].concat();
            fs::write(&path, &bytes).unwrap();
            let result = Journal::open(
                directory.path(),
                namespace(),
                Recovery::DiscardIncompleteTail,
            );
            assert!(result.is_err(), "accepted byte {at} bit {bit}");
            assert!(
                !matches!(result, Err(StorageError::IncompleteTail { .. })),
                "misclassified corruption"
            );
            assert_eq!(
                fs::read(&path).unwrap(),
                bytes,
                "repaired byte {at} bit {bit}"
            );
        }
    }
}

#[test]
fn complete_valid_crc_malformed_payloads_never_become_recoverable_tails() {
    let directory = TestDirectory::new();
    let (path, header) = empty_store(&directory);
    let mut invalid_kind = put(b"k", b"v");
    invalid_kind[0] = 2;
    let zero_key = put(b"", b"v");
    let oversized_key = put(&vec![b'k'; MAX_KEY_BYTES + 1], b"v");
    let mut oversized_value = vec![1, 0, 1, b'k'];
    oversized_value.extend_from_slice(&((MAX_VALUE_BYTES + 1) as u32).to_be_bytes());
    let mut truncated_value = put(b"k", b"v");
    truncated_value.pop();
    let mut trailing = payload(&[put(b"k", b"v")]);
    trailing.push(0);
    let cases = [
        ("empty batch", 0_u32.to_be_bytes().to_vec()),
        (
            "oversized count",
            ((MAX_BATCH_OPERATIONS + 1) as u32).to_be_bytes().to_vec(),
        ),
        ("missing operation", 1_u32.to_be_bytes().to_vec()),
        ("unknown kind", payload(&[invalid_kind])),
        ("empty key", payload(&[zero_key])),
        ("oversized key", payload(&[oversized_key])),
        ("invalid UTF8", payload(&[put(&[0xff], b"v")])),
        (
            "duplicate keys",
            payload(&[put(b"k", b"v"), put(b"k", b"x")]),
        ),
        (
            "unordered keys",
            payload(&[put(b"z", b"v"), put(b"a", b"v")]),
        ),
        ("oversized value", payload(&[oversized_value])),
        ("truncated value", payload(&[truncated_value])),
        ("trailing payload", trailing),
    ];
    for (name, payload) in cases {
        let encoded = frame(&payload, 1, crc32(&header));
        let bytes = [header.as_slice(), encoded.as_slice()].concat();
        fs::write(&path, &bytes).unwrap();
        let result = Journal::open(
            directory.path(),
            namespace(),
            Recovery::DiscardIncompleteTail,
        );
        assert!(result.is_err(), "accepted {name}");
        assert!(
            !matches!(result, Err(StorageError::IncompleteTail { .. })),
            "misclassified {name}"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes, "modified {name}");
    }
}

#[test]
fn valid_crc_header_bounds_and_chain_checks_precede_tail_discard() {
    let directory = TestDirectory::new();
    let (path, header) = empty_store(&directory);
    let prior = crc32(&header);
    for (name, encoded) in [
        (
            "oversized length",
            frame_header((MAX_BATCH_BYTES + 1) as u32, 1, prior),
        ),
        ("undersized length", frame_header(3, 1, prior)),
        ("zero sequence", frame_header(4, 0, prior)),
        ("skipped sequence", frame_header(4, 2, prior)),
        ("max sequence", frame_header(4, u64::MAX, prior)),
        ("wrong previous CRC", frame_header(4, 1, prior ^ 1)),
    ] {
        let bytes = [header.as_slice(), encoded.as_slice()].concat();
        fs::write(&path, &bytes).unwrap();
        assert!(
            Journal::open(
                directory.path(),
                namespace(),
                Recovery::DiscardIncompleteTail
            )
            .is_err(),
            "accepted {name}"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes, "discarded {name}");
    }
}

#[test]
fn second_writer_cannot_recover_or_modify_a_locked_journal() {
    let directory = TestDirectory::new();
    let (mut first, _) = Journal::open(
        directory.path(),
        namespace(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    first
        .commit(vec![Mutation::Put {
            key: "key".into(),
            value: b"first".to_vec(),
        }])
        .unwrap();
    let before = fs::metadata(first.path()).unwrap().len();
    assert!(matches!(
        Journal::open(
            directory.path(),
            namespace(),
            Recovery::DiscardIncompleteTail
        ),
        Err(StorageError::Locked)
    ));
    assert_eq!(fs::metadata(first.path()).unwrap().len(), before);
    first
        .commit(vec![Mutation::Put {
            key: "key".into(),
            value: b"second".to_vec(),
        }])
        .unwrap();
    drop(first);
    let (second, _) = Journal::open(
        directory.path(),
        namespace(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    assert_eq!(second.sequence().unwrap(), 2);
    assert_eq!(second.get("key").unwrap(), Some(b"second".as_slice()));
}

#[test]
fn oversized_log_rejects_from_metadata_without_recovery_or_reading_payload() {
    let directory = TestDirectory::new();
    let (path, _) = empty_store(&directory);
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(MAX_LOG_BYTES + 1).unwrap();
    drop(file);
    assert!(matches!(
        Journal::open(
            directory.path(),
            namespace(),
            Recovery::DiscardIncompleteTail
        ),
        Err(StorageError::LimitExceeded)
    ));
    assert_eq!(fs::metadata(&path).unwrap().len(), MAX_LOG_BYTES + 1);
}

#[test]
fn category_names_and_chain_folders_match_complete_unmodified_db_source() {
    let mut categories = Vec::new();
    let mut layouts = 0;
    for line in include_str!("../fixtures/storage_layout_vectors.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        match fields[0] {
            "category" => {
                assert_eq!(fields.len(), 4);
                assert_eq!(fields[1].parse::<usize>().unwrap(), categories.len());
                assert_eq!(fields[3], format!("{}.db", fields[2]));
                categories.push(fields[2]);
            }
            "layout" => {
                assert_eq!(fields.len(), 7);
                for category in CATEGORIES {
                    let namespace = Namespace::new(fields[2], fields[3] == "1", category).unwrap();
                    let path = namespace.path(Path::new("fixture-root"));
                    assert_eq!(
                        path.parent()
                            .unwrap()
                            .file_name()
                            .unwrap()
                            .to_str()
                            .unwrap(),
                        fields[4]
                    );
                    // Rust journals use their own format/extension. The oracle
                    // establishes categories and folders, not NeDB bytes.
                    assert_eq!(
                        path.file_name().unwrap().to_str().unwrap(),
                        format!("{category}.tldb")
                    );
                }
                layouts += 1;
            }
            other => panic!("unknown db layout row {other}"),
        }
    }
    assert_eq!(categories.as_slice(), CATEGORIES);
    assert_eq!(layouts, 7);
}
