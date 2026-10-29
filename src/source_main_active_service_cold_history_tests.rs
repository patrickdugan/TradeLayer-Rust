use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "cold-ack-history-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let path = self.0.canonicalize().unwrap();
        assert_eq!(
            path.parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        let _ = fs::remove_dir_all(path);
    }
}
fn ns() -> Namespace {
    Namespace::new("LTC", false, "consensus").unwrap()
}
fn opaque(before: &[u8]) -> Vec<u8> {
    let mut out = b"TLMAP\x01".to_vec();
    out.extend_from_slice(&(before.len() as u64).to_be_bytes());
    out.extend_from_slice(before);
    let suffix = bytes(&obj(&[("apiRejection", V::Null)])).unwrap();
    out.extend_from_slice(&(suffix.len() as u64).to_be_bytes());
    out.extend_from_slice(&suffix);
    out
}
fn raw_pause() -> Vec<u8> {
    b"TLMASA\x01\0\0\0\x04kind\0\0\0\x07\"Pause\"".to_vec()
}
// Metadata-only administrative WAL fixture. It does not assert that a financial
// controller accepts these opaque observations or a Pause as its first action.
struct Fixture {
    store: ActiveServiceWalV1,
    head: [u8; 32],
    last: V,
    origin: V,
    archive: Vec<u8>,
    origin_history: Vec<u8>,
    observation: Vec<u8>,
}
fn fixture(temp: &Temp) -> Fixture {
    let archive = b"opaque raw foundation/native archive".to_vec();
    let origin_history = b"opaque scalar historical pin".to_vec();
    let observation = b"opaque actual origin observation".to_vec();
    let origin = obj(&[
        ("profile", s(ENHANCED_HISTORY_PROFILE)),
        (
            "archive",
            ActiveServiceWalBlobV1::of(&archive).unwrap().value(),
        ),
        (
            "observation",
            ActiveServiceWalBlobV1::of(&observation).unwrap().value(),
        ),
        (
            "requiredVerifiedHistory",
            ActiveServiceWalBlobV1::of(&origin_history).unwrap().value(),
        ),
    ]);
    let store = ActiveServiceWalV1::create(
        &temp.0,
        ns(),
        origin.clone(),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let head = sha256(&bytes(&origin).unwrap());
    Fixture {
        store,
        head,
        last: V::Null,
        origin,
        archive,
        origin_history,
        observation,
    }
}
fn ack(f: &mut Fixture, sequence: u64, stage: &str) -> ActiveServiceWalPinV1 {
    ack_with_post(f, sequence, stage, None)
}
fn ack_with_post(
    f: &mut Fixture,
    sequence: u64,
    stage: &str,
    post: Option<Vec<u8>>,
) -> ActiveServiceWalPinV1 {
    let action = raw_pause();
    let before = f.observation.clone();
    let accepted = post.unwrap_or_else(|| opaque(&before));
    let history = f.origin_history.clone();
    let entry = obj(&[
        ("profile", s(ENHANCED_HISTORY_PROFILE)),
        ("sequence", s(sequence.to_string())),
        ("previousEntry", f.last.clone()),
        ("previousHead", s(hex(&f.head))),
        (
            "action",
            ActiveServiceWalBlobV1::of(&action).unwrap().value(),
        ),
        (
            "before",
            ActiveServiceWalBlobV1::of(&before).unwrap().value(),
        ),
        ("nativeCandidates", V::Null),
        ("nativeMutations", V::Null),
        (
            "accepted",
            ActiveServiceWalBlobV1::of(&accepted).unwrap().value(),
        ),
        (
            "verifiedHistoryBefore",
            ActiveServiceWalBlobV1::of(&history).unwrap().value(),
        ),
        (
            "verifiedHistoryPost",
            ActiveServiceWalBlobV1::of(&history).unwrap().value(),
        ),
    ]);
    let raw = bytes(&entry).unwrap();
    let entry_pin = ActiveServiceWalBlobV1::of(&raw).unwrap();
    let previous = f.head;
    let mut chain = b"Native13/ActiveWalPhysicalAck/v1".to_vec();
    chain.extend_from_slice(&previous);
    chain.extend_from_slice(&sequence.to_be_bytes());
    chain.extend_from_slice(&entry_pin.sha256);
    f.head = sha256(&chain);
    let record = obj(&[
        ("profile", s(ENHANCED_HISTORY_PROFILE)),
        ("stage", s(stage)),
        ("sequence", s(sequence.to_string())),
        ("previousHead", s(hex(&previous))),
        ("head", s(hex(&f.head))),
        ("action", entry.get("action")),
        ("before", entry.get("before")),
        ("origin", f.origin.get("archive")),
        ("nativeCandidates", V::Null),
        ("nativeMutations", V::Null),
        ("accepted", entry.get("accepted")),
        ("entry", entry_pin.value()),
        ("verifiedHistoryBefore", entry.get("verifiedHistoryBefore")),
        ("verifiedHistoryPost", entry.get("verifiedHistoryPost")),
    ]);
    let blobs = vec![f.archive.clone(), history, action, before, accepted, raw];
    let refs = blobs
        .iter()
        .map(|v| ActiveServiceWalBlobV1::of(v).unwrap())
        .collect();
    let mut plan = f
        .store
        .prepare(&f.store.pin().unwrap(), record, blobs, refs)
        .unwrap();
    let post = f.store.attempt(&mut plan).unwrap();
    f.last = entry_pin.value();
    post
}
#[test]
fn cold_ack_actual_disk_open_checks_full_chain_and_retains_lock_without_writes() {
    let temp = Temp::new();
    let mut f = fixture(&temp);
    ack(&mut f, 1, "Ack");
    let pin = ack(&mut f, 2, "Ack");
    let path = f.store.journal_path();
    let head = f.head;
    drop(f);
    let original = fs::read(&path).unwrap();
    let reader = VerifiedActiveServiceAckHistoryV1::open(&temp.0, ns(), &pin).unwrap();
    assert_eq!(reader.pin().unwrap(), pin);
    assert_eq!(reader.head(), head);
    assert_eq!(reader.entries().len(), 2);
    assert_eq!(reader.entries()[0].raw_action(), raw_pause());
    assert_eq!(
        reader.entries()[0].actual_before(),
        b"opaque actual origin observation"
    );
    assert_eq!(
        reader.origin_archive(),
        b"opaque raw foundation/native archive"
    );
    assert!(
        VerifiedActiveServiceAckHistoryV1::open(&temp.0, ns(), &pin).is_err(),
        "reader keeps actual exclusive writer lock"
    );
    drop(reader);
    assert_eq!(fs::read(&path).unwrap(), original);
}
#[test]
fn cold_ack_pending_accepted_is_not_ack_and_wrong_expected_pin_does_not_create() {
    let temp = Temp::new();
    let mut f = fixture(&temp);
    let pin = ack(&mut f, 1, "Accepted");
    let path = f.store.journal_path();
    drop(f);
    let original = fs::read(&path).unwrap();
    assert!(VerifiedActiveServiceAckHistoryV1::open(&temp.0, ns(), &pin).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    let mut wrong = pin;
    wrong.prefix.content.chain_sha256d[0] ^= 1;
    assert!(VerifiedActiveServiceAckHistoryV1::open(&temp.0, ns(), &wrong).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
}
#[test]
fn cold_ack_missing_or_corrupt_referenced_blob_refuses_exact_pinned_journal() {
    let temp = Temp::new();
    let mut f = fixture(&temp);
    let pin = ack(&mut f, 1, "Ack");
    let blob = f.store.root.join(BLOBS).join(format!(
        "{}.asw1",
        hex(&history_blob(&f.last).unwrap().sha256)
    ));
    let bytes = fs::read(&blob).unwrap();
    drop(f);
    fs::remove_file(&blob).unwrap();
    assert!(VerifiedActiveServiceAckHistoryV1::open(&temp.0, ns(), &pin).is_err());
    fs::write(&blob, b"corrupt").unwrap();
    assert!(VerifiedActiveServiceAckHistoryV1::open(&temp.0, ns(), &pin).is_err());
    fs::write(&blob, &bytes).unwrap();
    assert!(VerifiedActiveServiceAckHistoryV1::open(&temp.0, ns(), &pin).is_ok());
}
#[test]
fn cold_ack_bad_previous_sequence_or_chain_refuses_even_valid_current_receipt() {
    let temp = Temp::new();
    let mut f = fixture(&temp);
    ack(&mut f, 1, "Ack");
    let pin = ack(&mut f, 3, "Ack");
    drop(f);
    assert!(VerifiedActiveServiceAckHistoryV1::open(&temp.0, ns(), &pin).is_err());
}

#[test]
fn cold_ack_malformed_accepted_wire_and_discontinuous_observation_refuse() {
    for variant in 0..2 {
        let temp = Temp::new();
        let mut f = fixture(&temp);
        let mut post = opaque(if variant == 0 {
            &f.observation
        } else {
            b"changed Post observation"
        });
        if variant == 0 {
            post.push(0);
        }
        let mut pin = ack_with_post(&mut f, 1, "Ack", Some(post));
        if variant == 1 {
            pin = ack(&mut f, 2, "Ack");
        }
        drop(f);
        assert!(VerifiedActiveServiceAckHistoryV1::open(&temp.0, ns(), &pin).is_err());
    }
}
#[test]
fn cold_ack_required_origin_history_blob_cannot_be_downgraded_to_absent() {
    let temp = Temp::new();
    let mut f = fixture(&temp);
    let pin = ack(&mut f, 1, "Ack");
    let required = history_blob(&f.origin.get("requiredVerifiedHistory")).unwrap();
    let path = f
        .store
        .root
        .join(BLOBS)
        .join(format!("{}.asw1", hex(&required.sha256)));
    drop(f);
    fs::remove_file(&path).unwrap();
    assert!(VerifiedActiveServiceAckHistoryV1::open(&temp.0, ns(), &pin).is_err());
}

#[test]
fn cold_ack_legacy_mode_and_oversized_total_are_explicit_refusals() {
    let temp = Temp::new();
    let mut f = fixture(&temp);
    let mut budget = MAX_VERIFIED_ACK_HISTORY_BYTES;
    let pin = ActiveServiceWalBlobV1::of(&raw_pause()).unwrap();
    assert!(matches!(
        history_read_blob(&f.store, &pin.value(), &mut budget),
        Err(ActiveServiceWalErrorV1::Capacity)
    ));
    assert_eq!(budget, MAX_VERIFIED_ACK_HISTORY_BYTES);
    assert!(f.store.pin().is_ok());
    let context = f.store.origin_context().unwrap();
    let mut legacy_origin = context.get("origin");
    legacy_origin.insert("profile", s(PROFILE)).unwrap();
    let legacy = Temp::new();
    drop(f.store);
    f.store = ActiveServiceWalV1::create(
        &legacy.0,
        ns(),
        legacy_origin.clone(),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    f.origin = legacy_origin;
    f.head = sha256(&bytes(&f.origin).unwrap());
    let pin = ack(&mut f, 1, "Ack");
    drop(f);
    assert!(VerifiedActiveServiceAckHistoryV1::open(&legacy.0, ns(), &pin).is_err());
}
