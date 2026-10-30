use super::*;
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
static SERIAL: AtomicU64 = AtomicU64::new(0);
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("ACTIVE_HISTORY_META_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "wal-pin-reuse-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(self.parent.as_path()));
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn store(t: &Temp) -> ActiveServiceWalV1 {
    ActiveServiceWalV1::create(
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[("profile", s("metadata-only hash reuse control"))]),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap()
}
fn same(a: &PreparedActiveServiceWalWriteV1, b: &PreparedActiveServiceWalWriteV1) {
    assert_eq!(a.owner, b.owner);
    assert_eq!(a.before, b.before);
    assert_eq!(a.post, b.post);
    assert_eq!(a.record, b.record);
    assert_eq!(a.blobs, b.blobs);
    assert_eq!(a.references, b.references);
    assert_eq!(a.attempted, b.attempted);
}
#[test]
fn prepared_plan_bytes_and_sync_post_equal_exact_old_body_on_same_actual_writer() {
    let t = Temp::new();
    let mut store = store(&t);
    let existing = b"existing actual checked blob".to_vec();
    store.write_blob(&existing).unwrap();
    let payloads = vec![
        vec![b'a'; 256 * 1024],
        vec![b'b'; 128 * 1024],
        b"same".to_vec(),
        b"same".to_vec(),
    ];
    let mut refs = payloads
        .iter()
        .rev()
        .map(|v| ActiveServiceWalBlobV1::of(v).unwrap())
        .collect::<Vec<_>>();
    refs.push(ActiveServiceWalBlobV1::of(&existing).unwrap());
    let record = obj(&[
        ("profile", s("metadata comparison only")),
        ("stage", s("Ack")),
    ]);
    let pin = store.pin().unwrap();
    let reference = store
        .prepare_reference_uncached(&pin, record.clone(), payloads.clone(), refs.clone())
        .unwrap();
    let mut reused = store.prepare(&pin, record, payloads, refs).unwrap();
    same(&reference, &reused);
    assert_eq!(store.pin().unwrap(), pin);
    let post = store.attempt(&mut reused).unwrap();
    assert_eq!(&post, reference.post());
    for pin in reference.references() {
        store.read_blob(pin).unwrap();
    }
}
#[test]
fn missing_existing_dependency_and_invalid_new_blob_refuse_with_same_error_and_no_mutation() {
    let t = Temp::new();
    let store = store(&t);
    let pin = store.pin().unwrap();
    for (blobs, refs) in [
        (vec![vec![]], vec![]),
        (
            vec![b"new".to_vec()],
            vec![ActiveServiceWalBlobV1::of(b"missing existing dependency").unwrap()],
        ),
    ] {
        let a = store
            .prepare_reference_uncached(&pin, obj(&[]), blobs.clone(), refs.clone())
            .err()
            .expect("reference refusal");
        let b = store
            .prepare(&pin, obj(&[]), blobs, refs)
            .err()
            .expect("optimized refusal");
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
        assert_eq!(store.pin().unwrap(), pin);
    }
}

thread_local! {static READ_ORDER:std::cell::RefCell<Option<Vec<ActiveServiceWalBlobV1>>>=const{std::cell::RefCell::new(None)};}
pub(super) fn observe_existing_blob_read(pin: &ActiveServiceWalBlobV1) {
    READ_ORDER.with(|log| {
        if let Some(log) = log.borrow_mut().as_mut() {
            log.push(pin.clone());
        }
    });
}
fn reads<T>(call: impl FnOnce() -> T) -> (T, Vec<ActiveServiceWalBlobV1>) {
    READ_ORDER.with(|v| *v.borrow_mut() = Some(vec![]));
    let result = call();
    let order = READ_ORDER.with(|v| v.borrow_mut().take().unwrap());
    (result, order)
}
#[test]
fn repeated_existing_reference_reads_keep_exact_order_and_error_precedence() {
    let t = Temp::new();
    let store = store(&t);
    let a = store.write_blob(b"existing A").unwrap();
    let b = store.write_blob(b"existing B").unwrap();
    let new = b"new payload".to_vec();
    let new_pin = ActiveServiceWalBlobV1::of(&new).unwrap();
    let pin = store.pin().unwrap();
    let refs = vec![b.clone(), a.clone(), b.clone(), new_pin];
    let (old, order_old) = reads(|| {
        store
            .prepare_reference_uncached(&pin, obj(&[]), vec![new.clone()], refs.clone())
            .unwrap()
    });
    let (fresh, order_fresh) = reads(|| {
        store
            .prepare(&pin, obj(&[]), vec![new.clone()], refs.clone())
            .unwrap()
    });
    same(&old, &fresh);
    assert_eq!(order_old, vec![b.clone(), a.clone(), b.clone()]);
    assert_eq!(order_fresh, order_old);
    fs::write(
        store
            .root
            .join(BLOBS)
            .join(format!("{}.asw1", hex(&a.sha256))),
        b"truncated",
    )
    .unwrap();
    let missing = ActiveServiceWalBlobV1::of(b"missing reference").unwrap();
    for refs in [
        vec![a.clone(), missing.clone()],
        vec![missing.clone(), a.clone()],
    ] {
        let (old, order_old) = reads(|| {
            store
                .prepare_reference_uncached(&pin, obj(&[]), vec![new.clone()], refs.clone())
                .err()
                .unwrap()
        });
        let (fresh, order_fresh) = reads(|| {
            store
                .prepare(&pin, obj(&[]), vec![new.clone()], refs.clone())
                .err()
                .unwrap()
        });
        assert_eq!(format!("{old:?}"), format!("{fresh:?}"));
        assert_eq!(order_old, vec![refs[0].clone()]);
        assert_eq!(order_fresh, order_old);
    }
    let (old, order_old) = reads(|| {
        store
            .prepare_reference_uncached(&pin, obj(&[]), vec![vec![]], vec![missing.clone()])
            .err()
            .unwrap()
    });
    let (fresh, order_fresh) = reads(|| {
        store
            .prepare(&pin, obj(&[]), vec![vec![]], vec![missing])
            .err()
            .unwrap()
    });
    assert!(matches!(old, ActiveServiceWalErrorV1::Capacity));
    assert!(matches!(fresh, ActiveServiceWalErrorV1::Capacity));
    assert!(order_old.is_empty());
    assert!(order_fresh.is_empty());
    assert_eq!(store.pin().unwrap(), pin);
}
