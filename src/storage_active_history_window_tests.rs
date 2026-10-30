use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf, PathBuf);
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("ACTIVE_HISTORY_META_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "active-history-window-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root.canonicalize().unwrap(), parent)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        assert_eq!(self.0.parent(), Some(self.1.as_path()));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn journal(temp: &Temp) -> Journal {
    Journal::open(
        &temp.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap()
    .0
}
fn put(journal: &mut Journal, value: &str) {
    journal
        .commit(vec![Mutation::Put {
            key: "current".into(),
            value: value.as_bytes().to_vec(),
        }])
        .unwrap();
}
#[test]
fn actual_window_retains_only_suffix_and_matches_whole_trace_without_changing_append_cursor() {
    let t = Temp::new();
    let mut j = journal(&t);
    put(&mut j, "first");
    let before = j.verified_prefix_pin().unwrap();
    let trace = j
        .read_verified_current_commit_window(&before, 0, None, 1, 1024)
        .unwrap()
        .after()
        .clone();
    for at in 0..20 {
        put(&mut j, &at.to_string());
    }
    let pin = j.verified_prefix_pin().unwrap();
    let cursor = j.file.stream_position().unwrap();
    let window = j
        .read_verified_current_commit_window(&pin, 1, Some(&trace), 20, 4096)
        .unwrap();
    assert_eq!(window.commits().len(), 20);
    assert_eq!(window.commits()[0].sequence, 2);
    assert_eq!(window.commits()[19].sequence, 21);
    assert_eq!(
        window.after(),
        j.read_verified_current_commits(&pin, 32, 4096)
            .unwrap()
            .trace()
    );
    assert_eq!(j.file.stream_position().unwrap(), cursor);
    put(&mut j, "after read");
    assert_eq!(j.sequence().unwrap(), 22);
    assert_eq!(j.get("current").unwrap(), Some(b"after read".as_slice()));
}
#[test]
fn bad_retained_trace_and_budget_refuse_without_poison_or_cursor_change() {
    let t = Temp::new();
    let mut j = journal(&t);
    put(&mut j, "first");
    let pin = j.verified_prefix_pin().unwrap();
    let mut trace = j
        .read_verified_current_commit_window(&pin, 1, None, 1, 1024)
        .unwrap()
        .before()
        .clone();
    put(&mut j, "second");
    let pin = j.verified_prefix_pin().unwrap();
    let cursor = j.file.stream_position().unwrap();
    trace.chain_sha256d[0] ^= 1;
    assert!(j
        .read_verified_current_commit_window(&pin, 1, Some(&trace), 1, 1024)
        .is_err());
    assert_eq!(j.file.stream_position().unwrap(), cursor);
    assert_eq!(j.verified_prefix_pin().unwrap(), pin);
    assert!(j
        .read_verified_current_commit_window(&pin, 0, None, 1, 1024)
        .is_err());
    put(&mut j, "still healthy");
    assert_eq!(j.sequence().unwrap(), 3);
}
#[test]
fn sequence_zero_empty_capture_is_actual_and_does_not_create_a_frame() {
    let t = Temp::new();
    let mut j = journal(&t);
    let pin = j.verified_prefix_pin().unwrap();
    let window = j
        .read_verified_current_commit_window(&pin, 0, None, 1, 1)
        .unwrap();
    assert!(window.commits().is_empty());
    assert_eq!(window.before(), window.after());
    assert_eq!(j.sequence().unwrap(), 0);
    put(&mut j, "first real append");
    assert_eq!(j.sequence().unwrap(), 1);
}
#[test]
fn real_current_disk_corruption_refuses_retains_lock_and_poison() {
    let t = Temp::new();
    let mut j = journal(&t);
    put(&mut j, "first");
    let pin = j.verified_prefix_pin().unwrap();
    let end = j.end;
    j.file.seek(SeekFrom::Start(end - 1)).unwrap();
    j.file.write_all(b"X").unwrap();
    j.file.sync_all().unwrap();
    assert!(j
        .read_verified_current_commit_window(&pin, 0, None, 1, 1024)
        .is_err());
    assert!(matches!(j.sequence(), Err(StorageError::Poisoned)));
    assert!(matches!(
        Journal::open_existing(
            &t.0,
            Namespace::new("LTC", false, "consensus").unwrap(),
            Recovery::RejectIncompleteTail
        ),
        Err(StorageError::Locked)
    ));
}
