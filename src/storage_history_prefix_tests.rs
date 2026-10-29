use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let path = parent.join(format!(
            "cold-history-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(path) = self.0.canonicalize() {
            assert_eq!(
                path.parent(),
                Some(std::env::temp_dir().canonicalize().unwrap().as_path())
            );
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
fn ns() -> Namespace {
    Namespace::new("LTC", false, "persistence").unwrap()
}
fn put(j: &mut Journal, value: &[u8]) {
    j.commit(vec![Mutation::Put {
        key: "k".into(),
        value: value.to_vec(),
    }])
    .unwrap();
}
fn file_bytes(j: &mut Journal) -> Vec<u8> {
    let at = j.file.stream_position().unwrap();
    j.file.seek(SeekFrom::Start(0)).unwrap();
    let mut raw = vec![];
    j.file.read_to_end(&mut raw).unwrap();
    j.file.seek(SeekFrom::Start(at)).unwrap();
    raw
}
#[test]
fn cold_open_at_post_derives_actual_old_prefix_without_repair_or_current_mutation() {
    let temp = Temp::new();
    let (mut j, _) = Journal::open(&temp.0, ns(), Recovery::RejectIncompleteTail).unwrap();
    let empty = j.verified_prefix_pin().unwrap();
    put(&mut j, b"old");
    let old = j.verified_prefix_pin().unwrap();
    put(&mut j, b"now");
    let current = j.verified_prefix_pin().unwrap();
    drop(j);
    let (mut j, _) = Journal::open_existing_verified_prefix(
        &temp.0,
        ns(),
        Recovery::RejectIncompleteTail,
        &current,
    )
    .unwrap();
    let raw = file_bytes(&mut j);
    let at = j.file.stream_position().unwrap();
    let history = j.read_verified_history_prefix(&old).unwrap();
    assert_eq!(history.get("k"), Some(b"old".as_slice()));
    assert_eq!(history.complete_kv_receipt().unwrap(), old.content);
    assert_eq!(j.get("k").unwrap(), Some(b"now".as_slice()));
    assert_eq!(j.verified_prefix_pin().unwrap(), current);
    assert_eq!(j.file.stream_position().unwrap(), at);
    assert_eq!(file_bytes(&mut j), raw);
    assert_eq!(
        j.read_verified_history_prefix(&empty)
            .unwrap()
            .entries()
            .count(),
        0
    );
    put(&mut j, b"new");
    assert_eq!(history.get("k"), Some(b"old".as_slice()));
}
#[test]
fn wrong_history_receipt_or_sequence_refuses_but_same_current_owner_stays_healthy() {
    let temp = Temp::new();
    let (mut j, _) = Journal::open(&temp.0, ns(), Recovery::RejectIncompleteTail).unwrap();
    put(&mut j, b"old");
    let old = j.verified_prefix_pin().unwrap();
    put(&mut j, b"now");
    let current = j.verified_prefix_pin().unwrap();
    let raw = file_bytes(&mut j);
    j.file.seek(SeekFrom::Start(3)).unwrap();
    let mut wrong = old.clone();
    wrong.content.chain_sha256d[0] ^= 1;
    assert!(j.read_verified_history_prefix(&wrong).is_err());
    assert_eq!(j.file.stream_position().unwrap(), 3);
    wrong = old.clone();
    wrong.physical_sequence = current.physical_sequence + 1;
    assert!(j.read_verified_history_prefix(&wrong).is_err());
    assert_eq!(j.file.stream_position().unwrap(), 3);
    wrong = old.clone();
    wrong.namespace = Namespace::new("BTC", false, "persistence").unwrap();
    assert!(j.read_verified_history_prefix(&wrong).is_err());
    assert_eq!(j.file.stream_position().unwrap(), 3);
    assert_eq!(j.verified_prefix_pin().unwrap(), current);
    assert_eq!(file_bytes(&mut j), raw);
    put(&mut j, b"new");
    assert_eq!(j.get("k").unwrap(), Some(b"new".as_slice()));
}
fn rechain(raw: &mut [u8], header_len: usize) {
    let mut at = header_len;
    let mut previous = crc32(&raw[..header_len]);
    while at < raw.len() {
        let payload = u32_at(raw, at + 4) as usize;
        raw[at + 16..at + 20].copy_from_slice(&previous.to_be_bytes());
        let header_crc = crc32(&raw[at..at + 20]);
        raw[at + 20..at + 24].copy_from_slice(&header_crc.to_be_bytes());
        previous = crc32(&raw[at..at + 24 + payload]);
        raw[at + 24 + payload..at + 28 + payload].copy_from_slice(&previous.to_be_bytes());
        at += payload + 32;
    }
}
#[test]
fn valid_crc_rewritten_past_with_identical_current_kv_cannot_satisfy_old_history_pin() {
    let temp = Temp::new();
    let (mut j, _) = Journal::open(&temp.0, ns(), Recovery::RejectIncompleteTail).unwrap();
    put(&mut j, b"old");
    let old = j.verified_prefix_pin().unwrap();
    put(&mut j, b"now");
    let current = j.verified_prefix_pin().unwrap();
    let path = j.path().to_owned();
    drop(j);
    let mut raw = std::fs::read(&path).unwrap();
    let at = raw.windows(3).position(|v| v == b"old").unwrap();
    raw[at..at + 3].copy_from_slice(b"BAD");
    rechain(&mut raw, ns().header().len());
    std::fs::write(&path, &raw).unwrap();
    let (mut j, _) = Journal::open_existing_verified_prefix(
        &temp.0,
        ns(),
        Recovery::RejectIncompleteTail,
        &current,
    )
    .unwrap();
    assert!(j.read_verified_history_prefix(&old).is_err());
    assert_eq!(j.get("k").unwrap(), Some(b"now".as_slice()));
    assert_eq!(file_bytes(&mut j), raw);
}
#[test]
fn observed_unknown_disk_suffix_is_never_repaired_and_poison_retains_exclusive_owner() {
    let temp = Temp::new();
    let (mut j, _) = Journal::open(&temp.0, ns(), Recovery::RejectIncompleteTail).unwrap();
    put(&mut j, b"old");
    let old = j.verified_prefix_pin().unwrap();
    j.file.seek(SeekFrom::End(0)).unwrap();
    j.file.write_all(b"short").unwrap();
    j.file.sync_all().unwrap();
    let raw = file_bytes(&mut j);
    assert!(j.read_verified_history_prefix(&old).is_err());
    assert!(matches!(j.sequence(), Err(StorageError::Poisoned)));
    assert_eq!(file_bytes(&mut j), raw);
    assert!(matches!(
        Journal::open_existing(&temp.0, ns(), Recovery::RejectIncompleteTail),
        Err(StorageError::Locked)
    ));
}

#[test]
fn malformed_crc_batch_footer_or_short_frame_poison_without_repair_or_unlock() {
    for kind in 0..4 {
        let temp = Temp::new();
        let (mut j, _) = Journal::open(&temp.0, ns(), Recovery::RejectIncompleteTail).unwrap();
        put(&mut j, b"old");
        let pin = j.verified_prefix_pin().unwrap();
        let mut raw = file_bytes(&mut j);
        let header = ns().header().len();
        let payload = u32_at(&raw, header + 4) as usize;
        if kind == 0 {
            // Payload begins count:u32, then the first mutation tag. Recompute
            // all actual CRCs so the ordinary batch decoder must reject it.
            raw[header + 24 + 4] = 2;
            rechain(&mut raw, header);
        } else if kind == 1 {
            raw[header + 24 + payload] ^= 1;
        } else if kind == 2 {
            raw[header + 28 + payload..header + 32 + payload].copy_from_slice(b"NOPE");
        } else {
            raw[header + 4..header + 8].copy_from_slice(&((payload + 1) as u32).to_be_bytes());
            let checksum = crc32(&raw[header..header + 20]);
            raw[header + 20..header + 24].copy_from_slice(&checksum.to_be_bytes());
        }
        j.file.seek(SeekFrom::Start(0)).unwrap();
        j.file.write_all(&raw).unwrap();
        j.file.sync_all().unwrap();
        let error = j.read_verified_history_prefix(&pin).unwrap_err();
        if kind == 3 {
            assert!(matches!(error, StorageError::IncompleteTail { .. }));
        } else {
            assert!(matches!(error, StorageError::Corrupt(_)));
        }
        assert!(matches!(j.sequence(), Err(StorageError::Poisoned)));
        assert_eq!(file_bytes(&mut j), raw);
        assert!(matches!(
            Journal::open_existing(&temp.0, ns(), Recovery::RejectIncompleteTail),
            Err(StorageError::Locked)
        ));
    }
}
