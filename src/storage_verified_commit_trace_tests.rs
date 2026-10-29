use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("NATIVE_WAL_TRACE_TMP")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-mode4-commit-trace-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let expected = self.0.parent().unwrap();
        if let Ok(root) = self.0.canonicalize() {
            assert_eq!(root.parent(), Some(expected));
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
fn ns() -> Namespace {
    Namespace::new("LTC", false, "consensus").unwrap()
}
fn put(j: &mut Journal, value: &[u8]) {
    j.commit(vec![Mutation::Put {
        key: "record".into(),
        value: value.to_vec(),
    }])
    .unwrap();
}
fn file_bytes(j: &mut Journal) -> Vec<u8> {
    let cursor = j.file.stream_position().unwrap();
    j.file.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes = vec![];
    j.file.read_to_end(&mut bytes).unwrap();
    j.file.seek(SeekFrom::Start(cursor)).unwrap();
    bytes
}
#[test]
fn actual_commit_trace_preserves_complete_ordered_mutations_and_file_cursor() {
    let temp = Temp::new();
    let (mut j, _) = Journal::open(&temp.0, ns(), Recovery::RejectIncompleteTail).unwrap();
    let first = vec![
        Mutation::Put {
            key: "context".into(),
            value: b"actual origin".to_vec(),
        },
        Mutation::Put {
            key: "record".into(),
            value: b"Created".to_vec(),
        },
    ];
    j.commit(first.clone()).unwrap();
    put(&mut j, b"Intent");
    put(&mut j, b"Ack");
    let pin = j.verified_prefix_pin().unwrap();
    j.file.seek(SeekFrom::Start(7)).unwrap();
    let history = j.read_verified_current_commits(&pin, 16, 4096).unwrap();
    assert_eq!(history.prefix(), &pin);
    assert_eq!(history.trace().bytes, j.end);
    assert_eq!(history.trace().commits, 3);
    assert_eq!(history.commits()[0].mutations, first);
    assert_eq!(history.commits()[1].sequence, 2);
    assert_eq!(
        history.commits()[1].mutations,
        vec![Mutation::Put {
            key: "record".into(),
            value: b"Intent".to_vec()
        }]
    );
    assert_eq!(
        history.commits()[2].mutations,
        vec![Mutation::Put {
            key: "record".into(),
            value: b"Ack".to_vec()
        }]
    );
    assert_eq!(j.file.stream_position().unwrap(), 7);
    assert_eq!(j.verified_prefix_pin().unwrap(), pin);
    assert_eq!(
        j.read_verified_current_commits(&pin, 16, 4096)
            .unwrap()
            .trace(),
        history.trace()
    );
}
fn rechain(raw: &mut [u8]) {
    let mut at = ns().header().len();
    let mut previous = crc32(&raw[..at]);
    while at < raw.len() {
        let payload = u32_at(raw, at + 4) as usize;
        raw[at + 16..at + 20].copy_from_slice(&previous.to_be_bytes());
        let header = crc32(&raw[at..at + 20]);
        raw[at + 20..at + 24].copy_from_slice(&header.to_be_bytes());
        previous = crc32(&raw[at..at + 24 + payload]);
        raw[at + 24 + payload..at + 28 + payload].copy_from_slice(&previous.to_be_bytes());
        at += payload + 32;
    }
}
#[test]
fn independently_retained_trace_detects_valid_crc_past_rewrite_with_same_final_kv() {
    let temp = Temp::new();
    let (mut j, _) = Journal::open(&temp.0, ns(), Recovery::RejectIncompleteTail).unwrap();
    put(&mut j, b"Intent");
    put(&mut j, b"Accepted");
    put(&mut j, b"Ack");
    let pin = j.verified_prefix_pin().unwrap();
    let actual = j.read_verified_current_commits(&pin, 16, 4096).unwrap();
    let path = j.path().to_owned();
    drop(j);
    let mut raw = std::fs::read(&path).unwrap();
    let at = raw.windows(6).position(|v| v == b"Intent").unwrap();
    raw[at..at + 6].copy_from_slice(b"Forged");
    rechain(&mut raw);
    std::fs::write(&path, &raw).unwrap();
    let (mut reopened, _) =
        Journal::open_existing_verified_prefix(&temp.0, ns(), Recovery::RejectIncompleteTail, &pin)
            .unwrap();
    let rewritten = reopened
        .read_verified_current_commits(&pin, 16, 4096)
        .unwrap();
    assert_eq!(rewritten.prefix(), actual.prefix());
    assert_ne!(rewritten.trace(), actual.trace());
    assert_eq!(
        rewritten.commits()[0].mutations,
        vec![Mutation::Put {
            key: "record".into(),
            value: b"Forged".to_vec()
        }]
    );
    assert_eq!(reopened.get("record").unwrap(), Some(b"Ack".as_slice()));
    assert_eq!(file_bytes(&mut reopened), raw);
}
#[test]
fn trace_capacity_is_atomic_and_actual_corrupt_suffix_poison_keeps_same_lock() {
    let temp = Temp::new();
    let (mut j, _) = Journal::open(&temp.0, ns(), Recovery::RejectIncompleteTail).unwrap();
    put(&mut j, b"Intent");
    put(&mut j, b"Ack");
    let pin = j.verified_prefix_pin().unwrap();
    assert!(matches!(
        j.read_verified_current_commits(&pin, 1, 4096),
        Err(StorageError::LimitExceeded)
    ));
    assert!(matches!(
        j.read_verified_current_commits(&pin, 16, 1),
        Err(StorageError::LimitExceeded)
    ));
    assert_eq!(j.verified_prefix_pin().unwrap(), pin);
    j.file.seek(SeekFrom::End(0)).unwrap();
    j.file.write_all(b"short").unwrap();
    j.file.sync_all().unwrap();
    let actual = file_bytes(&mut j);
    assert!(j.read_verified_current_commits(&pin, 16, 4096).is_err());
    assert!(matches!(j.sequence(), Err(StorageError::Poisoned)));
    assert!(matches!(
        Journal::open_existing(&temp.0, ns(), Recovery::RejectIncompleteTail),
        Err(StorageError::Locked)
    ));
    assert_eq!(file_bytes(&mut j), actual);
}
