// Administrative actual-Journal facts only; no financial execution claimed.
use super::*;
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "durable-fresh-source-prefix-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(root) = self.root.canonicalize() {
            assert_eq!(root.parent(), Some(self.parent.as_path()));
            let _ = fs::remove_dir_all(root);
        }
    }
}
fn ns() -> Namespace {
    Namespace::new("LTC", false, "consensus").unwrap()
}
fn origin() -> V {
    let raw = ActiveServiceWalBlobV1::of(b"administrative actual Journal fixture; not finance")
        .unwrap()
        .value();
    obj(&[
        ("profile", s(ACTUAL_SOURCE_FACT_WAL_PROFILE)),
        ("archive", raw.clone()),
        ("observation", raw.clone()),
        ("requiredVerifiedHistory", raw),
        (
            "generationEscrow",
            obj(&[
                ("profile", s("TL-COMPACT-GENERATION-ESCROW-13-1")),
                ("root", s(std::env::temp_dir().to_string_lossy())),
                ("durability", s("FileSyncOnly")),
            ]),
        ),
        (
            "requiredInlineServiceProfile",
            s(MODE4_INLINE_SERVICE_PROFILE),
        ),
        ("requiredNativeFactProfile", s(ACTUAL_NATIVE_FACT_PROFILE)),
        ("actualNativeFactScope", s(ACTUAL_SOURCE_FACT_SCOPE)),
    ])
}
#[test]
fn fresh_source_boundary_retains_exact_current_ordered_trace_not_only_equal_final_kv() {
    let t = Temp::new();
    let mut wal = ActiveServiceWalV1::create(
        &t.root,
        ns(),
        origin(),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let first = wal.capture_actual_durable_fresh_source_boundary().unwrap();
    assert_eq!(first.source_pin(), &wal.pin().unwrap());
    assert_eq!(first.source_trace().commits, 1);
    let mut plan = wal
        .prepare(&wal.pin().unwrap(), wal.record().unwrap(), vec![], vec![])
        .unwrap();
    wal.attempt(&mut plan).unwrap();
    let second = wal.capture_actual_durable_fresh_source_boundary().unwrap();
    assert_eq!(
        first.source_pin().prefix.content,
        second.source_pin().prefix.content
    );
    assert_ne!(first.source_trace(), second.source_trace());
    assert_ne!(
        first.canonical_boundary_bytes(),
        second.canonical_boundary_bytes()
    );
}
#[test]
fn fresh_source_boundary_refuses_unknown_stage_and_outer_profile_without_io_or_repair() {
    let t = Temp::new();
    let mut wal = ActiveServiceWalV1::create(
        &t.root,
        ns(),
        origin(),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let mut plan = wal
        .prepare(
            &wal.pin().unwrap(),
            obj(&[
                ("profile", s(ACTUAL_SOURCE_FACT_WAL_PROFILE)),
                ("stage", s("not-an-ack")),
            ]),
            vec![],
            vec![],
        )
        .unwrap();
    wal.attempt(&mut plan).unwrap();
    let pin = wal.pin().unwrap();
    assert!(wal.capture_actual_durable_fresh_source_boundary().is_err());
    assert_eq!(wal.pin().unwrap(), pin);
    drop(wal);
    let outer = Temp::new();
    let mut wal = ActiveServiceWalV1::create(
        &outer.root,
        ns(),
        obj(&[("profile", s(DURABLE_FRESH_COORDINATION_PROFILE))]),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    assert!(wal.capture_actual_durable_fresh_source_boundary().is_err());
}
#[test]
fn fresh_source_boundary_never_blesses_an_ambiguous_control_write_before_exact_resolution() {
    let t = Temp::new();
    let mut wal = ActiveServiceWalV1::create(
        &t.root,
        ns(),
        origin(),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let mut plan = wal
        .prepare(&wal.pin().unwrap(), wal.record().unwrap(), vec![], vec![])
        .unwrap();
    wal.set_test_fault(TestFault::AfterCommit);
    assert!(wal.attempt(&mut plan).is_err());
    assert!(wal.capture_actual_durable_fresh_source_boundary().is_err());
    assert_eq!(
        wal.resolve(&plan).unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(
        wal.capture_actual_durable_fresh_source_boundary()
            .unwrap()
            .source_pin(),
        plan.post()
    );
}
