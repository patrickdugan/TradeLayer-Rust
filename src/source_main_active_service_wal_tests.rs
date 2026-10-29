use super::*;
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("MAIN_ACTIVE_WAL_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "active-wal-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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
fn store(temp: &Temp) -> ActiveServiceWalV1 {
    ActiveServiceWalV1::create(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[("actualOrigin", s("test metadata only"))]),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap()
}
fn plan(store: &ActiveServiceWalV1, phase: &str) -> PreparedActiveServiceWalWriteV1 {
    let raw = b"actual raw action; never a financial answer".to_vec();
    let pin = ActiveServiceWalBlobV1::of(&raw).unwrap();
    store
        .prepare(
            &store.pin().unwrap(),
            obj(&[
                ("profile", s(PROFILE)),
                ("phase", s(phase)),
                ("raw", pin.value()),
            ]),
            vec![raw],
            vec![pin],
        )
        .unwrap()
}
#[test]
fn actual_journal_sync_matches_sealed_full_kv_post_and_blob() {
    let temp = Temp::new();
    let mut store = store(&temp);
    let mut plan = plan(&store, "Intent");
    let post = store.attempt(&mut plan).unwrap();
    assert_eq!(&post, plan.post());
    assert_eq!(
        store.read_blob(&plan.references()[0]).unwrap(),
        b"actual raw action; never a financial answer"
    );
    assert_eq!(store.record().unwrap().get("phase"), s("Intent"));
    assert!(matches!(
        store.attempt(&mut plan),
        Err(ActiveServiceWalErrorV1::AlreadyAttempted)
    ));
}
#[test]
fn lost_after_sync_return_resolves_actual_post_without_repeating_commit() {
    let temp = Temp::new();
    let mut store = store(&temp);
    let mut plan = plan(&store, "Accepted");
    store.set_test_fault(TestFault::AfterCommit);
    assert!(store.attempt(&mut plan).is_err());
    assert!(matches!(
        store.pin(),
        Err(ActiveServiceWalErrorV1::Ambiguous)
    ));
    assert_eq!(
        store.resolve(&plan).unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(store.pin().unwrap(), *plan.post());
    assert!(matches!(
        store.rearm_before(&plan),
        Err(ActiveServiceWalErrorV1::WrongPin)
    ));
}
#[test]
fn exact_before_rearms_only_same_wal_bytes() {
    let temp = Temp::new();
    let mut store = store(&temp);
    let mut plan = plan(&store, "Intent");
    store.set_test_fault(TestFault::BeforeCommit);
    assert!(store.attempt(&mut plan).is_err());
    assert_eq!(
        store.resolve(&plan).unwrap(),
        ActiveServiceWalResolvedV1::Before
    );
    let mut replacement = store.rearm_before(&plan).unwrap();
    assert_eq!(replacement.record, plan.record);
    assert_eq!(replacement.blobs, plan.blobs);
    store.attempt(&mut replacement).unwrap();
    assert_eq!(store.pin().unwrap(), *plan.post());
}
#[test]
fn synced_record_missing_blob_is_not_post_acceptance() {
    let temp = Temp::new();
    let mut store = store(&temp);
    let mut plan = plan(&store, "ACK");
    store.set_test_fault(TestFault::AfterCommit);
    assert!(store.attempt(&mut plan).is_err());
    let path = store
        .root
        .join(BLOBS)
        .join(format!("{}.asw1", hex(&plan.references()[0].sha256)));
    fs::remove_file(&path).unwrap();
    assert!(store.resolve(&plan).is_err());
    assert!(matches!(
        store.pin(),
        Err(ActiveServiceWalErrorV1::Ambiguous)
    ));
    fs::write(&path, &plan.blobs[0]).unwrap();
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .sync_all()
        .unwrap();
    assert_eq!(
        store.resolve(&plan).unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
}
#[test]
fn foreign_store_cannot_use_sealed_transition() {
    let a = Temp::new();
    let b = Temp::new();
    let store_a = store(&a);
    let mut store_b = store(&b);
    let mut plan = plan(&store_a, "Intent");
    let before = store_b.pin().unwrap();
    assert!(matches!(
        store_b.attempt(&mut plan),
        Err(ActiveServiceWalErrorV1::WrongPlan)
    ));
    assert_eq!(store_b.pin().unwrap(), before);
    assert!(!plan.attempted());
}
#[test]
fn actual_partial_tail_repairs_only_independently_expected_before() {
    let temp = Temp::new();
    let mut store = store(&temp);
    let mut plan = plan(&store, "Intent");
    store.set_test_fault(TestFault::BeforeCommit);
    assert!(store.attempt(&mut plan).is_err());
    let path = store.journal_path();
    drop(store.journal.take());
    let original = fs::metadata(&path).unwrap().len();
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(&[1, 2, 3]).unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert_eq!(
        store.resolve(&plan).unwrap(),
        ActiveServiceWalResolvedV1::Before
    );
    assert_eq!(fs::metadata(path).unwrap().len(), original);
    assert_eq!(store.pin().unwrap(), *plan.before());
}
#[test]
fn unrecognized_healthy_suffix_is_not_inferred_as_expected_post() {
    let temp = Temp::new();
    let mut store = store(&temp);
    let mut plan = plan(&store, "Intent");
    store.set_test_fault(TestFault::AfterCommit);
    assert!(store.attempt(&mut plan).is_err());
    store
        .journal
        .as_mut()
        .unwrap()
        .commit(vec![Mutation::Put {
            key: RECORD.into(),
            value: bytes(&obj(&[("unrecognized", V::Bool(true))])).unwrap(),
        }])
        .unwrap();
    assert!(store.resolve(&plan).is_err());
    assert!(store.ambiguous);
}
#[test]
fn already_referenced_corrupt_blob_refuses_preparation() {
    let temp = Temp::new();
    let mut store = store(&temp);
    let mut first = plan(&store, "Intent");
    store.attempt(&mut first).unwrap();
    let pin = first.references()[0].clone();
    let path = store
        .root
        .join(BLOBS)
        .join(format!("{}.asw1", hex(&pin.sha256)));
    fs::write(path, vec![0; pin.bytes as usize]).unwrap();
    assert!(store
        .prepare(
            &store.pin().unwrap(),
            obj(&[("phase", s("Accepted"))]),
            vec![],
            vec![pin]
        )
        .is_err());
}
