use super::*;
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("ACTIVE_HISTORY_META_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "verified-commit-range-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}
static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
impl Drop for Temp {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(self.parent.as_path()));
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn actual_historical_commit_range_checks_both_pinned_ends_and_current_trace() {
    let temp = Temp::new();
    let (mut j, _) = Journal::open(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    let mut pins = Vec::new();
    for n in 0..21 {
        j.commit(vec![Mutation::Put {
            key: "value".into(),
            value: vec![n],
        }])
        .unwrap();
        let pin = j.verified_prefix_pin().unwrap();
        pins.push(
            j.read_verified_current_commit_window(&pin, pin.physical_sequence, None, 1, 1)
                .unwrap()
                .after()
                .clone(),
        );
    }
    let current = j.verified_prefix_pin().unwrap();
    let cursor = j.file.stream_position().unwrap();
    let range = j
        .read_verified_commit_range(
            &current,
            pins.last().unwrap(),
            &pins[4],
            &pins[9],
            (5, 1024),
        )
        .unwrap();
    assert_eq!(
        range
            .commits()
            .iter()
            .map(|c| c.sequence)
            .collect::<Vec<_>>(),
        vec![6, 7, 8, 9, 10]
    );
    assert_eq!(range.before(), &pins[4]);
    assert_eq!(range.after(), &pins[9]);
    assert_eq!(range.current(), &current);
    assert_eq!(j.file.stream_position().unwrap(), cursor);
    j.commit(vec![Mutation::Put {
        key: "later".into(),
        value: vec![22],
    }])
    .unwrap();
    assert_eq!(j.sequence, 22);
}
#[test]
fn bad_historical_trace_and_small_range_budget_refuse_without_poison_or_cursor_change() {
    let temp = Temp::new();
    let (mut j, _) = Journal::open(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    let initial = j.verified_prefix_pin().unwrap();
    let zero = j
        .read_verified_current_commit_window(&initial, 0, None, 1, 1)
        .unwrap()
        .after()
        .clone();
    j.commit(vec![Mutation::Put {
        key: "actual".into(),
        value: vec![1, 2, 3],
    }])
    .unwrap();
    let current = j.verified_prefix_pin().unwrap();
    let trace = j
        .read_verified_current_commit_window(&current, 1, None, 1, 1)
        .unwrap()
        .after()
        .clone();
    let cursor = j.file.stream_position().unwrap();
    let mut bad = trace.clone();
    bad.chain_sha256d[0] ^= 1;
    assert!(j
        .read_verified_commit_range(&current, &trace, &zero, &bad, (1, 1024))
        .is_err());
    assert!(j
        .read_verified_commit_range(&current, &trace, &zero, &trace, (1, 1))
        .is_err());
    assert_eq!(j.file.stream_position().unwrap(), cursor);
    assert_eq!(j.verified_prefix_pin().unwrap(), current);
    assert!(j
        .read_verified_commit_range(&current, &trace, &zero, &trace, (1, 1024))
        .is_ok());
    j.commit(vec![Mutation::Delete {
        key: "actual".into(),
    }])
    .unwrap();
    assert_eq!(j.sequence, 2);
}
