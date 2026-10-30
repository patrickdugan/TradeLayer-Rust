// Real WAL/index files, raw action codec and comparison-only opaque bytes.
// No financial Core or source result is supplied/executed by these controls.
use super::*;
use crate::source_compact_index_v2::SourceCompactIndex;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("MIXED_COLD_METADATA_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&parent).unwrap();
        let root = parent.canonicalize().unwrap().join(format!(
            "mixed-cold-reader-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> (Temp, MixedNativeAckHistoryPinV1, ActiveServiceWalPinV1) {
    let t = Temp::new();
    let index = SourceCompactIndex::create(
        &t.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "raw metadata input",
    )
    .unwrap();
    let native = index
        .capture_verified_history_pin()
        .unwrap()
        .canonical_scalar_bytes()
        .unwrap();
    let archive = b"opaque archive comparison bytes".to_vec();
    let observation = b"opaque original observation bytes".to_vec();
    let refs = [
        ActiveServiceWalBlobV1::of(&archive).unwrap(),
        ActiveServiceWalBlobV1::of(&observation).unwrap(),
        ActiveServiceWalBlobV1::of(&native).unwrap(),
    ];
    let escrow = t.0.join("actual-generation-escrow");
    fs::create_dir(&escrow).unwrap();
    let origin = obj(&[
        ("profile", s(MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE)),
        ("archive", refs[0].value()),
        ("observation", refs[1].value()),
        ("requiredVerifiedHistory", refs[2].value()),
        (
            "generationEscrow",
            obj(&[
                ("profile", s("TL-COMPACT-GENERATION-ESCROW-13-1")),
                ("root", s(escrow.canonicalize().unwrap().to_string_lossy())),
                ("durability", s("FileSyncOnly")),
            ]),
        ),
        (
            "requiredInlineServiceProfile",
            s(MODE4_INLINE_SERVICE_PROFILE),
        ),
        ("requiredNativeFactProfile", s(ACTUAL_NATIVE_FACT_PROFILE)),
        ("actualNativeFactScope", s(MIXED_SOURCE_NATIVE_FACT_SCOPE)),
        (
            "requiredOrdinaryNativeFactProfile",
            s(ACTUAL_VALIDITY_PLAN_FACT_PROFILE),
        ),
        ("ordinaryNativeFactScope", s(ACTUAL_VALIDITY_FACT_SCOPE)),
        (
            "underlyingSourceHistoryProfile",
            s(MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE),
        ),
        ("acknowledgedWindowLimit", s("256")),
    ]);
    let mut wal = ActiveServiceWalV1::create(
        &t.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        origin.clone(),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let created = wal.pin().unwrap();
    let previous = sha256(&bytes(&origin).unwrap());
    let action = crate::source_main_native_session::NativeMainActiveServiceActionV1::Start {
        start: V::number(10.),
        generation: 71,
        documents: V::Object(vec![]),
    }
    .canonical_action_bytes()
    .unwrap();
    let before = b"opaque Before comparison".to_vec();
    let diagnostic = bytes(&obj(&[("apiRejection", V::Null)])).unwrap();
    let mut accepted = b"TLMAP\x01".to_vec();
    accepted.extend_from_slice(&0u64.to_be_bytes());
    accepted.extend_from_slice(&(diagnostic.len() as u64).to_be_bytes());
    accepted.extend_from_slice(&diagnostic);
    let action_pin = ActiveServiceWalBlobV1::of(&action).unwrap();
    let before_pin = ActiveServiceWalBlobV1::of(&before).unwrap();
    let accepted_pin = ActiveServiceWalBlobV1::of(&accepted).unwrap();
    let source = obj(&[("actions", s("1")), ("commitment", s(hex(&[9; 32])))]);
    let mut entry = obj(&[
        ("profile", s(MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE)),
        ("sequence", s("1")),
        ("previousEntry", V::Null),
        ("previousHead", s(hex(&previous))),
        ("action", action_pin.value()),
        ("before", before_pin.value()),
        ("nativeCandidates", V::Null),
        ("nativeMutations", V::Null),
        ("accepted", accepted_pin.value()),
        ("verifiedHistoryBefore", refs[2].value()),
        ("verifiedHistoryPost", refs[2].value()),
        ("sourceHead", source.clone()),
        ("historySegment", V::Null),
    ]);
    for (key, value) in [
        ("requiredNativeFactProfile", s(ACTUAL_NATIVE_FACT_PROFILE)),
        ("actualPendingPlanBefore", V::Null),
        ("actualSelectionBefore", V::Null),
        ("actualPendingPlanAfter", V::Null),
        ("actualSelectionAfter", V::Null),
        ("mixedOperationKind", s("SourceAction")),
        (
            "requiredOrdinaryNativeFactProfile",
            s(ACTUAL_VALIDITY_PLAN_FACT_PROFILE),
        ),
        ("ordinaryPendingPlanBefore", V::Null),
        ("ordinarySelectionBefore", V::Null),
        ("ordinaryPendingPlanAfter", V::Null),
        ("ordinarySelectionAfter", V::Null),
    ] {
        entry.insert(key, value).unwrap();
    }
    let raw_entry = bytes(&entry).unwrap();
    let entry_pin = ActiveServiceWalBlobV1::of(&raw_entry).unwrap();
    let mut chain = b"Native13/ActiveWalPhysicalAck/v1".to_vec();
    chain.extend_from_slice(&previous);
    chain.extend_from_slice(&1u64.to_be_bytes());
    chain.extend_from_slice(&entry_pin.sha256);
    let head = sha256(&chain);
    for stage in ["Intent", "SourcePendingPlanObserved", "Accepted", "Ack"] {
        let mut record = entry.clone();
        if let V::Object(fields) = &mut record {
            fields.retain(|(k, _)| k != "previousEntry");
        }
        record.insert("stage", s(stage)).unwrap();
        record.insert("origin", refs[0].value()).unwrap();
        if matches!(stage, "Intent" | "SourcePendingPlanObserved") {
            if let V::Object(fields) = &mut record {
                fields.retain(|(k, _)| k != "verifiedHistoryPost");
            }
            record.insert("accepted", V::Null).unwrap();
            record.insert("entry", V::Null).unwrap();
            record.insert("head", V::Null).unwrap();
            record.insert("sourceHead", V::Null).unwrap();
        } else {
            record.insert("entry", entry_pin.value()).unwrap();
            record.insert("head", s(hex(&head))).unwrap();
        }
        let blobs = vec![
            archive.clone(),
            observation.clone(),
            native.clone(),
            action.clone(),
            before.clone(),
            accepted.clone(),
            raw_entry.clone(),
        ];
        let references = blobs
            .iter()
            .map(|b| ActiveServiceWalBlobV1::of(b).unwrap())
            .collect();
        let mut plan = wal
            .prepare(&wal.pin().unwrap(), record, blobs, references)
            .unwrap();
        wal.attempt(&mut plan).unwrap();
    }
    let expected = wal
        .capture_mixed_native_ack_history(1, head, &source, &index)
        .unwrap();
    drop(wal);
    drop(index);
    (t, expected, created)
}
#[test]
fn mixed_reader_actual_trace_window_and_metadata_pin_roundtrip_keep_old_reader_closed() {
    let (t, pin, _) = fixture();
    let raw = pin.canonical_expectation_bytes().unwrap();
    let decoded = MixedNativeAckHistoryPinV1::from_canonical_expectation_bytes(
        &Namespace::new("LTC", false, "consensus").unwrap(),
        &Namespace::new("LTC", false, "txIndex").unwrap(),
        &raw,
    )
    .unwrap();
    assert_eq!(pin, decoded);
    let mut reader = VerifiedMixedNativeHistoryV1::open(
        &t.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &decoded,
    )
    .unwrap();
    let window = reader.read_window(0).unwrap();
    assert_eq!(window.entries().len(), 1);
    assert_eq!(window.records.len(), 4);
    assert!(reader
        .window_access(&window)
        .unwrap()
        .ordinary_io_inputs(1)
        .unwrap()
        .is_empty());
    drop(reader);
    assert!(VerifiedSegmentedActiveHistoryV1::open(
        &t.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &decoded.segmented
    )
    .is_err());
}
#[test]
fn mixed_reader_bad_trace_and_foreign_window_refuse_without_publishing() {
    let (t, pin, _) = fixture();
    let mut bad = pin.clone();
    bad.segmented.trace.chain_sha256d[0] ^= 1;
    assert!(VerifiedMixedNativeHistoryV1::open(
        &t.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &bad
    )
    .is_err());
    let mut one = VerifiedMixedNativeHistoryV1::open(
        &t.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &pin,
    )
    .unwrap();
    let window = one.read_window(0).unwrap();
    drop(one);
    let two = VerifiedMixedNativeHistoryV1::open(
        &t.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &pin,
    )
    .unwrap();
    assert!(two.window_access(&window).is_err());
    assert_eq!(two.store.pin().unwrap(), pin.segmented.wal);
}
#[test]
fn mixed_producer_prefix_reads_actual_created_record_bad_expected_kv_keeps_lock_and_cursor() {
    let (t, pin, created) = fixture();
    let mut reader = VerifiedMixedNativeHistoryV1::open(
        &t.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &pin,
    )
    .unwrap();
    let raw = bytes(&obj(&[("actualWal", actual_fact_wal_value(&created))])).unwrap();
    let (actual, record) = reader.mixed_producer_prefix(&raw, 2).unwrap();
    assert_eq!(actual, created);
    assert_eq!(parse(&record).unwrap().get("phase"), s("Created"));
    let mut wrong = created.clone();
    wrong.prefix.content.chain_sha256d[0] ^= 1;
    assert!(reader
        .mixed_producer_prefix(
            &bytes(&obj(&[("actualWal", actual_fact_wal_value(&wrong))])).unwrap(),
            2
        )
        .is_err());
    assert_eq!(reader.read_window(0).unwrap().entries().len(), 1);
    assert!(Journal::open_existing_verified_prefix(
        &t.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        Recovery::RejectIncompleteTail,
        &pin.segmented.wal.prefix
    )
    .is_err());
}
