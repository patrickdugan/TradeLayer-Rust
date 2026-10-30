// Actual storage/private admission only; no financial reconstruction claimed.
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
            "paired-created-source-proof-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("source")).unwrap();
        fs::create_dir(root.join("metadata")).unwrap();
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
fn ns(name: &str) -> Namespace {
    Namespace::new("LTC", false, name).unwrap()
}
fn created(
    t: &Temp,
) -> (
    SourceCompactIndex,
    ActualSourceFactColdPinV1,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
) {
    let index = SourceCompactIndex::create(
        &t.root.join("metadata"),
        ns("txIndex"),
        "actual raw administrative fixture",
    )
    .unwrap();
    let history = index
        .capture_verified_history_pin()
        .unwrap()
        .canonical_scalar_bytes()
        .unwrap();
    let archive = b"administrative raw origin; not financial execution".to_vec();
    let observation = b"administrative source observation; not source execution".to_vec();
    let origin = obj(&[
        ("profile", s(ACTUAL_SOURCE_FACT_WAL_PROFILE)),
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
            ActiveServiceWalBlobV1::of(&history).unwrap().value(),
        ),
        (
            "generationEscrow",
            obj(&[
                ("profile", s("TL-COMPACT-GENERATION-ESCROW-13-1")),
                ("root", s(t.root.to_string_lossy())),
                ("durability", s("FileSyncOnly")),
            ]),
        ),
        (
            "requiredInlineServiceProfile",
            s(MODE4_INLINE_SERVICE_PROFILE),
        ),
        ("requiredNativeFactProfile", s(ACTUAL_NATIVE_FACT_PROFILE)),
        ("actualNativeFactScope", s(ACTUAL_SOURCE_FACT_SCOPE)),
    ]);
    let mut source = ActiveServiceWalV1::create(
        &t.root.join("source"),
        ns("consensus"),
        origin,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let actual = source
        .capture_actual_durable_fresh_source_boundary()
        .unwrap();
    let expected = ActualSourceFactColdPinV1 {
        profile: ACTUAL_SOURCE_FACT_COLD_PIN_PROFILE.into(),
        wal: actual.source_pin().clone(),
        metadata_namespace: ns("txIndex"),
        trace: actual.source_trace().clone(),
    };
    drop(source);
    (index, expected, archive, history, observation)
}
#[test]
fn private_created_pair_admission_binds_real_raw_origin_but_old_public_ack_reader_still_refuses() {
    let t = Temp::new();
    let (index, expected, archive, history, observation) = created(&t);
    let before = index.capture_verified_history_pin().unwrap();
    let reader = VerifiedActualSourceFactTraceV1::open_for_paired_created(
        &t.root.join("source"),
        ns("consensus"),
        &expected,
        &archive,
        &history,
        &observation,
    )
    .unwrap();
    assert!(reader.entries().is_empty());
    assert_eq!(reader.checked_metadata_post().unwrap(), before);
    assert_eq!(reader.origin_archive(), archive);
    assert_eq!(reader.original_observation_bytes().unwrap(), observation);
    assert!(
        reader.read_reference(&history_blob(&reader.origin().get("observation")).unwrap()).is_err(),
        "Created does not synthesize the missing child blob; verified bytes stay in the private paired carrier"
    );
    drop(reader);
    assert!(VerifiedActualSourceFactTraceV1::open(
        &t.root.join("source"),
        ns("consensus"),
        &expected
    )
    .is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
}
#[test]
fn private_created_pair_refuses_substituted_raw_origin_and_keeps_actual_source_files() {
    let t = Temp::new();
    let (index, expected, archive, history, observation) = created(&t);
    let before = index.capture_verified_history_pin().unwrap();
    assert!(VerifiedActualSourceFactTraceV1::open_for_paired_created(
        &t.root.join("source"),
        ns("consensus"),
        &expected,
        b"different origin",
        &history,
        &observation
    )
    .is_err());
    assert!(VerifiedActualSourceFactTraceV1::open_for_paired_created(
        &t.root.join("source"),
        ns("consensus"),
        &expected,
        &archive,
        &history,
        b"different observation",
    )
    .is_err());
    let reader = VerifiedActualSourceFactTraceV1::open_for_paired_created(
        &t.root.join("source"),
        ns("consensus"),
        &expected,
        &archive,
        &history,
        &observation,
    )
    .unwrap();
    assert_eq!(reader.checked_metadata_post().unwrap(), before);
}
