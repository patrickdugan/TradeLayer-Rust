use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "paired-fresh-actual-trace-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
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
    Namespace::new("LTC", false, "consensus").unwrap()
}
fn put(j: &mut Journal, value: &[u8]) {
    j.commit(vec![Mutation::Put {
        key: "actual-raw-metadata".into(),
        value: value.to_vec(),
    }])
    .unwrap();
}
#[test]
fn actual_past_point_trace_matches_capture_and_current_cursor_without_writes() {
    let t = Temp::new();
    let (mut j, _) = Journal::open(&t.root, ns(), Recovery::RejectIncompleteTail).unwrap();
    put(&mut j, b"first");
    let past = j.verified_prefix_pin().unwrap();
    let old = j
        .read_verified_current_commits(&past, 8, 4096)
        .unwrap()
        .trace()
        .clone();
    put(&mut j, b"second");
    let current = j.verified_prefix_pin().unwrap();
    assert_eq!(
        j.read_verified_fresh_trace_point(&current, &past).unwrap(),
        old
    );
    assert_eq!(j.verified_prefix_pin().unwrap(), current);
    put(&mut j, b"third");
    assert_eq!(j.sequence().unwrap(), 3);
}
#[test]
fn past_wrong_kv_and_foreign_file_refuse_with_actual_current_owner_still_healthy() {
    let t = Temp::new();
    let (mut j, _) = Journal::open(&t.root, ns(), Recovery::RejectIncompleteTail).unwrap();
    put(&mut j, b"same");
    let past = j.verified_prefix_pin().unwrap();
    put(&mut j, b"same");
    let current = j.verified_prefix_pin().unwrap();
    let mut wrong = past.clone();
    wrong.content.chain_sha256d[0] ^= 1;
    assert!(j.read_verified_fresh_trace_point(&current, &wrong).is_err());
    assert_eq!(j.verified_prefix_pin().unwrap(), current);
    let other = Temp::new();
    let (mut foreign, _) =
        Journal::open(&other.root, ns(), Recovery::RejectIncompleteTail).unwrap();
    put(&mut foreign, b"same");
    assert!(j
        .read_verified_fresh_trace_point(&current, &foreign.verified_prefix_pin().unwrap())
        .is_err());
    assert_eq!(j.verified_prefix_pin().unwrap(), current);
}
#[test]
fn equal_current_kv_does_not_turn_different_past_commit_order_into_same_trace() {
    let a = Temp::new();
    let b = Temp::new();
    let (mut left, _) = Journal::open(&a.root, ns(), Recovery::RejectIncompleteTail).unwrap();
    let (mut right, _) = Journal::open(&b.root, ns(), Recovery::RejectIncompleteTail).unwrap();
    put(&mut left, b"first");
    let lp = left.verified_prefix_pin().unwrap();
    put(&mut right, b"different");
    let rp = right.verified_prefix_pin().unwrap();
    put(&mut left, b"final");
    put(&mut right, b"final");
    let l = left.verified_prefix_pin().unwrap();
    let r = right.verified_prefix_pin().unwrap();
    assert_eq!(l.content, r.content);
    assert_ne!(
        left.read_verified_fresh_trace_point(&l, &lp).unwrap(),
        right.read_verified_fresh_trace_point(&r, &rp).unwrap()
    );
}
