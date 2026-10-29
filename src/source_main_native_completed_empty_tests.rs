use super::*;
use crate::source_compact_index_v2::{
    NativePreparedValidityCandidateV1 as Candidate, NativePreparedValidityOperationV1,
    NativePreparedValidityQueryV1,
};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("NATIVE_SUPERSEDING_TMP")
            .map_or_else(std::env::temp_dir, PathBuf::from);
        fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "completed-empty-barrier-{}-{}",
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
fn native_ns() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn control_ns() -> Namespace {
    Namespace::new("LTC", false, "consensus").unwrap()
}
fn index(temp: &Temp) -> SourceCompactIndex {
    let mut index = SourceCompactIndex::create(
        &temp.root,
        native_ns(),
        "actual independent metadata context",
    )
    .unwrap();
    index
        .import_row(
            &index.snapshot().unwrap(),
            obj(&[
                ("_id", text("tx-1-input")),
                ("value", obj(&[("payload", text("caller data"))])),
            ]),
        )
        .unwrap();
    index
}
fn write(index: &SourceCompactIndex) -> NativePreparedValidityWriteV1 {
    let NativePreparedValidityQueryV1::Selected(selected) = index
        .prepare_validity_read(
            &index.snapshot().unwrap(),
            text("input"),
            V::number(1.),
            V::Bool(true),
            V::Undefined,
        )
        .unwrap()
    else {
        panic!()
    };
    let NativePreparedValidityOperationV1::Write(write) = selected
        .prepare_write(index, &index.snapshot().unwrap())
        .unwrap()
    else {
        panic!()
    };
    *write
}
fn make_store(temp: &Temp) -> NativeIntentStoreV1 {
    NativeIntentStoreV1::create(
        &temp.root,
        control_ns(),
        obj(&[("profile", text("metadata-only source controls"))]),
        V::Null,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap()
}
fn pending(
    store: &mut NativeIntentStoreV1,
    write: &NativePreparedValidityWriteV1,
) -> NativeCommandIntentPermitV1 {
    let mut permit = store
        .sync_command_intent(
            &store.pin().unwrap(),
            b"retained original actual command bytes",
            V::Null,
            100000,
        )
        .unwrap();
    let candidates = NativeWriteIntentCandidatesV1::from_prepared_append(
        write.append_plan(),
        obj(&[
            ("actualRead", write.durable_read_context()),
            (
                "retainedControl",
                text("unchanged opaque test control observations"),
            ),
        ]),
    )
    .unwrap();
    store.sync_native_intent(&mut permit, &candidates).unwrap();
    permit
}
fn reopened(
    temp: &Temp,
    original: &NativePreparedValidityWriteV1,
    candidate: Candidate,
) -> (
    SourceCompactIndex,
    NativePreparedValidityRecoveryObservationV1,
    NativePreparedValidityWriteV1,
    NativePreparedValiditySupersessionV1,
) {
    let (index, observation) = SourceCompactIndex::reopen_prepared_validity(
        &temp.root,
        native_ns(),
        original,
        candidate,
        Recovery::DiscardIncompleteTail,
    )
    .unwrap();
    let (replacement, mapping) = index
        .supersede_prepared_validity(original, &observation)
        .unwrap();
    (index, observation, replacement, mapping)
}

struct Ready {
    _native: Temp,
    control: Temp,
    index: SourceCompactIndex,
    write: NativePreparedValidityWriteV1,
    store: NativeIntentStoreV1,
    permit: NativeCommandIntentPermitV1,
}
fn ready(explicit: bool) -> Ready {
    let native = Temp::new();
    let control = Temp::new();
    let mut index = index(&native);
    let mut write = write(&index);
    let mut store = make_store(&control);
    let mut permit = pending(&mut store, &write);
    if explicit {
        drop(index);
        let (fresh, observation, replacement, mapping) =
            reopened(&native, &write, Candidate::Before);
        let operation = store
            .prepare_superseding_native_intent(
                &permit,
                &write,
                &fresh,
                &observation,
                &replacement,
                &mapping,
            )
            .unwrap();
        store
            .sync_prepared_superseding_native_intent(&mut permit, &operation)
            .unwrap();
        index = fresh;
        write = replacement;
    }
    index.commit_prepared_validity(&write).unwrap();
    Ready {
        _native: native,
        control,
        index,
        write,
        store,
        permit,
    }
}
fn files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, path: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for item in fs::read_dir(path).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                visit(root, &path, out)
            } else {
                let raw = match fs::read(&path) {
                    Ok(raw) => raw,
                    Err(e) if matches!(e.raw_os_error(), Some(32 | 33)) => {
                        let m = fs::metadata(&path).unwrap();
                        format!("locked:{}:{:?}", m.len(), m.modified().unwrap()).into_bytes()
                    }
                    Err(e) => panic!("{e}"),
                };
                out.push((path.strip_prefix(root).unwrap().into(), raw));
            }
        }
    }
    let mut out = vec![];
    visit(root, root, &mut out);
    out.sort();
    out
}
const ACCEPTED: &[u8] =
    b"actual accepted control frames; metadata barrier fixture, no financial result";
fn context() -> V {
    obj(&[(
        "nativePublicationProfile",
        text("actual sealed barrier test context"),
    )])
}
fn command_ready(explicit: bool) -> Ready {
    let mut r = ready(explicit);
    let operation = r
        .store
        .prepare_native_prefix_ack(&r.permit, &r.index, &r.write)
        .unwrap();
    r.store
        .sync_prepared_native_prefix_ack(&mut r.permit, &operation)
        .unwrap();
    r
}
fn no_native_command(explicit: bool) -> (Temp, NativeIntentStoreV1, NativeCommandIntentPermitV1) {
    let (control, mut store) = if explicit {
        // Select explicit mode through a genuine completed native command,
        // then begin a distinct command without any native write or prefix.
        let mut r = command_ready(true);
        r.store
            .sync_accepted(
                &mut r.permit,
                b"actual prior native command control frames for explicit reader bootstrap",
                context(),
            )
            .unwrap();
        r.store.sync_ack(&mut r.permit).unwrap();
        let Ready {
            _native,
            control,
            index,
            write,
            store,
            permit: _,
        } = r;
        drop(write);
        drop(index);
        drop(_native);
        (control, store)
    } else {
        let control = Temp::new();
        let store = make_store(&control);
        (control, store)
    };
    let permit = store
        .sync_command_intent(
            &store.pin().unwrap(),
            b"actual empty-row command with no native writes",
            V::Null,
            100000,
        )
        .unwrap();
    assert!(permit.native_prefix.is_none());
    let V::Object(fields) = store.state().unwrap() else {
        panic!()
    };
    assert!(
        fields.iter().all(|(key, _)| key != "nativePrefix"),
        "actual initial record omits nativePrefix, rather than storing null"
    );
    (control, store, permit)
}
#[test]
fn actual_no_native_command_prepares_without_rewriting_and_matches_original_accepted_ack_bytes() {
    for explicit in [false, true] {
        let (control, mut store, mut permit) = no_native_command(explicit);
        let before = store.pin().unwrap();
        let tree = files(&control.root);
        let accepted = store
            .prepare_completed_accepted(&permit, ACCEPTED, &context())
            .unwrap();
        assert_eq!(store.pin().unwrap(), before);
        assert_eq!(files(&control.root), tree);
        assert!(accepted.native_prefix().is_none());
        store
            .sync_accepted(&mut permit, ACCEPTED, context())
            .unwrap();
        assert_eq!(
            &store.pin().unwrap(),
            accepted.independently_expected_after()
        );
        let ack = store.prepare_completed_command_ack(&permit).unwrap();
        assert!(ack.native_prefix().is_none());
        store.sync_ack(&mut permit).unwrap();
        assert_eq!(&store.pin().unwrap(), ack.independently_expected_after());
        assert_eq!(
            store.read_blob(ack.acknowledgement()).unwrap(),
            ack.acknowledgement_bytes()
        );
    }
}
#[test]
fn actual_no_native_command_recovers_both_synced_barriers_on_same_retained_permit() {
    for explicit in [false, true] {
        let (control, mut store, mut permit) = no_native_command(explicit);
        let accepted = store
            .prepare_completed_accepted(&permit, ACCEPTED, &context())
            .unwrap();
        let before = permit.pin.clone();
        store.journal.test_fail_after_sync();
        assert!(store
            .sync_prepared_completed_accepted(&mut permit, &accepted)
            .is_err());
        assert_eq!(permit.pin, before);
        assert!(permit.native_prefix.is_none());
        drop(store);
        let mut store = NativeIntentStoreV1::open_synced_completed_accepted(
            &control.root,
            control_ns(),
            &accepted,
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap();
        store
            .recover_synced_completed_accepted(&accepted, &mut permit)
            .unwrap();
        assert!(permit.native_prefix.is_none());
        assert!(permit.reservation.capacity() >= permit.receipt_budget);
        let ack = store.prepare_completed_command_ack(&permit).unwrap();
        let before = permit.pin.clone();
        store.journal.test_fail_after_sync();
        assert!(store
            .sync_prepared_completed_command_ack(&mut permit, &ack)
            .is_err());
        assert_eq!(permit.pin, before);
        assert!(permit.reservation.capacity() >= permit.receipt_budget);
        drop(store);
        let store = NativeIntentStoreV1::open_synced_completed_command_ack(
            &control.root,
            control_ns(),
            &ack,
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap();
        store
            .recover_synced_completed_command_ack(&ack, &mut permit)
            .unwrap();
        assert!(permit.native_prefix.is_none());
        assert_eq!(permit.reservation.capacity(), 0);
        assert_eq!(store.state().unwrap().get("kind"), text("Idle"));
    }
}

#[test]
fn actual_no_native_command_healthy_blob_failures_retry_exact_same_operations() {
    for explicit in [false, true] {
        let (control, mut store, mut permit) = no_native_command(explicit);
        let accepted = store
            .prepare_completed_accepted(&permit, ACCEPTED, &context())
            .unwrap();
        let blocker = control
            .root
            .join(BLOBS)
            .join(format!("{}.tlc13", hex(&accepted.accepted_blob().sha256)));
        fs::create_dir(&blocker).unwrap();
        assert!(store
            .sync_prepared_completed_accepted(&mut permit, &accepted)
            .is_err());
        assert_eq!(
            &store.pin().unwrap(),
            accepted.independently_expected_before()
        );
        fs::remove_dir(blocker).unwrap();
        store
            .sync_prepared_completed_accepted(&mut permit, &accepted)
            .unwrap();
        let ack = store.prepare_completed_command_ack(&permit).unwrap();
        let blocker = control
            .root
            .join(BLOBS)
            .join(format!("{}.tlc13", hex(&ack.acknowledgement().sha256)));
        fs::create_dir(&blocker).unwrap();
        assert!(store
            .sync_prepared_completed_command_ack(&mut permit, &ack)
            .is_err());
        assert_eq!(&store.pin().unwrap(), ack.independently_expected_before());
        assert!(permit.reservation.capacity() >= permit.receipt_budget);
        fs::remove_dir(blocker).unwrap();
        store
            .sync_prepared_completed_command_ack(&mut permit, &ack)
            .unwrap();
        assert_eq!(permit.reservation.capacity(), 0);
    }
}
