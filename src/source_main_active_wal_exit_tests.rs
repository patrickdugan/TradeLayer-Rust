// Actual subprocess controls. They assert STORAGE boundaries only, never cold
// restoration of the lost financial owner or a resumed running service.
use super::*;
use crate::storage::{Journal, JournalCompleteKvReceiptV1, Recovery, VerifiedJournalPrefixV1};
const EXIT_CODE: i32 = 91;
fn text(v: V) -> String {
    let V::String(v) = v else { panic!("text") };
    v
}
fn uint(v: V) -> u64 {
    let raw = text(v);
    let n = raw.parse::<u64>().unwrap();
    assert_eq!(n.to_string(), raw);
    n
}
fn hash(v: V) -> [u8; 32] {
    let raw = text(v);
    assert_eq!(raw.len(), 64);
    assert!(raw.bytes().all(|b| b.is_ascii_hexdigit()));
    let mut out = [0; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&raw[i * 2..i * 2 + 2], 16).unwrap();
    }
    out
}
fn receipt_value(r: &JournalCompleteKvReceiptV1) -> V {
    obj(&[
        ("profile", s(&r.profile)),
        ("keys", s(&r.keys.to_string())),
        ("bytes", s(&r.bytes.to_string())),
        ("chain", s(&hex(&r.chain_sha256d))),
        ("complete", V::Bool(r.complete)),
    ])
}
fn receipt(v: V) -> JournalCompleteKvReceiptV1 {
    JournalCompleteKvReceiptV1 {
        profile: text(v.get("profile")),
        keys: uint(v.get("keys")),
        bytes: uint(v.get("bytes")),
        chain_sha256d: hash(v.get("chain")),
        complete: v.get("complete") == V::Bool(true),
    }
}
fn wal_value(p: &VerifiedJournalPrefixV1) -> V {
    obj(&[
        ("profile", s(&p.profile)),
        ("file", s(&p.canonical_file.to_string_lossy())),
        ("sequence", s(&p.physical_sequence.to_string())),
        ("content", receipt_value(&p.content)),
    ])
}
fn wal_pin(v: V) -> VerifiedJournalPrefixV1 {
    VerifiedJournalPrefixV1 {
        profile: text(v.get("profile")),
        namespace: Namespace::new("LTC", false, "consensus").unwrap(),
        canonical_file: PathBuf::from(text(v.get("file"))),
        physical_sequence: uint(v.get("sequence")),
        content: receipt(v.get("content")),
    }
}
fn write_fact(path: &std::path::Path, value: &V) {
    let raw = value.diagnostic_json().unwrap();
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    std::io::Write::write_all(&mut f, raw.as_bytes()).unwrap();
    f.sync_all().unwrap();
}
fn read_fact(path: &std::path::Path) -> V {
    crate::property_catalog::parse_diagnostic_value(&std::fs::read_to_string(path).unwrap())
        .unwrap()
}
fn child_root() -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os("TL_ACTIVE_WAL_EXIT_ROOT").expect("explicit child directory"),
    );
    let root = path.canonicalize().unwrap();
    assert_eq!(
        root.parent(),
        Some(std::env::temp_dir().canonicalize().unwrap().as_path())
    );
    assert!(root
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("actual-active-wal-exit-"));
    root
}
fn probe(plan: &PreparedActiveServiceWalWriteV1, after: bool) {
    let cut = std::env::var("TL_ACTIVE_WAL_EXIT_CUT").expect("explicit child cut");
    let stage = text(plan.record().unwrap().get("stage"));
    if (cut == "Native" && stage == "Intent") || cut == stage {
        if !after {
            let pins = V::Array(plan.references().iter().map(|p| p.value()).collect());
            write_fact(
                &child_root().join("independent-wal-seal.json"),
                &obj(&[
                    ("before", wal_value(&plan.before().prefix)),
                    ("post", wal_value(&plan.post().prefix)),
                    ("record", plan.record().unwrap()),
                    ("blobs", pins),
                ]),
            );
        } else if cut != "Native" {
            std::process::exit(EXIT_CODE)
        }
    }
}
fn native_value(p: &crate::compact_journal::VerifiedCompactPrefixV1) -> V {
    let q = &p.publication;
    obj(&[
        ("profile", s(&p.profile)),
        ("identity", s(&p.persistent_identity)),
        ("catalog", s(&p.canonical_catalog.to_string_lossy())),
        ("generation", s(&q.generation.to_string())),
        ("parent", q.parent.map_or(V::Null, |n| s(&n.to_string()))),
        (
            "physicalCreation",
            s(&q.physical_creation_sequence.to_string()),
        ),
        ("creation", s(&q.creation_sequence.to_string())),
        ("manifest", s(&hex(&q.manifest_sha256d))),
        ("sequence", s(&p.logical_sequence.to_string())),
        ("content", receipt_value(&p.content)),
    ])
}
fn native_pin(v: V) -> crate::compact_journal::VerifiedCompactPrefixV1 {
    crate::compact_journal::VerifiedCompactPrefixV1 {
        profile: text(v.get("profile")),
        namespace: Namespace::new("LTC", false, "txIndex").unwrap(),
        persistent_identity: text(v.get("identity")),
        canonical_catalog: PathBuf::from(text(v.get("catalog"))),
        publication: crate::compact_journal::CatalogPublication {
            generation: uint(v.get("generation")),
            parent: if v.get("parent") == V::Null {
                None
            } else {
                Some(uint(v.get("parent")))
            },
            physical_creation_sequence: uint(v.get("physicalCreation")),
            creation_sequence: uint(v.get("creation")),
            manifest_sha256d: hash(v.get("manifest")),
        },
        logical_sequence: uint(v.get("sequence")),
        content: receipt(v.get("content")),
    }
}
#[test]
#[ignore = "real child-process entry: called only by active_wal_real_exit_matrix"]
fn active_wal_actual_exit_child() {
    let root = child_root();
    let temp = Temp {
        root: root.clone(),
        parent: root.parent().unwrap().to_owned(),
    };
    let (raw_owner, docs) = setup(&temp, "same-held-live-validity");
    let mut owner = SourceMainNativeDurableActiveServiceV1::create_with_verified_history(
        raw_owner,
        &root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    wal_start(&mut owner, docs);
    drive_to_live(&mut owner);
    let cut = std::env::var("TL_ACTIVE_WAL_EXIT_CUT").unwrap();
    // Retain raw inputs and already-ACKed raw actions before the crash action.
    write_fact(
        &root.join("raw-origin.json"),
        &obj(&[
            ("sourceFixture", s("same-held-live-validity")),
            ("archive", owner.origin_pin.value()),
            (
                "actions",
                V::Array(owner.history().iter().map(|r| r.action.value()).collect()),
            ),
        ]),
    );
    let source = record("same-held-live-validity", "input");
    write_fact(
        &root.join("independent-raw-inputs.json"),
        &obj(&[
            ("rawReset", untag(raw_get(source, "rawReset"))),
            (
                "rawConstructor",
                untag(raw_get(source, "rawHarnessConstructorInput")),
            ),
            ("nativeRows", untag(raw_get(source, "nativeInput"))),
            ("rpcDocuments", untag(raw_get(source, "rawRPCDocuments"))),
        ]),
    );
    let actual_history = owner.index().capture_verified_history_pin().unwrap();
    let mut fields =
        NativeFieldCollector::new("Native13/CrashBoundaryHistory/v1", true, 1024 * 1024);
    actual_history
        .visit_canonical_fields(&mut |p, v| fields.emit(p, v))
        .unwrap();
    let raw = fields.frames.unwrap();
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("independent-history-fields.bin"))
        .unwrap();
    std::io::Write::write_all(&mut f, &raw).unwrap();
    f.sync_all().unwrap();
    drop(f);
    owner.wal.set_test_commit_probe(probe);
    if cut == "Native" {
        let service = owner.active_service().unwrap();
        let write = service.pending_write().unwrap();
        let before = service.index().capture_verified_history_pin().unwrap();
        let mut post = before.storage.clone();
        post.publication = write.append_plan().after().publication.clone();
        post.logical_sequence = write.append_plan().after().logical_sequence;
        let after = write.expected_after_state();
        post.content = JournalCompleteKvReceiptV1 {
            profile: after.profile.clone(),
            keys: after.keys,
            bytes: after.bytes,
            chain_sha256d: after.chain_sha256d,
            complete: after.complete,
        };
        write_fact(
            &root.join("independent-native-seal.json"),
            &obj(&[
                ("before", native_value(&before.storage)),
                ("post", native_value(&post)),
                ("actualPreparedContext", write.durable_read_context()),
            ]),
        );
        let Some(ActiveWalOwner::Active(active)) = &mut owner.owner else {
            panic!("actual owner")
        };
        active.owner.index.test_fail_after_sync();
        owner
            .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
            .unwrap();
        assert!(matches!(
            owner.advance(),
            Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
        ));
        // Real native sync happened; actual original service owner/plan is
        // still held and deliberately lost by process exit, not destructured.
        std::process::exit(EXIT_CODE)
    }
    owner
        .prepare_action(NativeMainActiveServiceActionV1::Pause)
        .unwrap();
    let _ = owner.advance();
    panic!("configured cut did not exit")
}
#[test]
fn active_wal_real_exit_matrix() {
    for cut in ["Intent", "Native", "Accepted", "Ack"] {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-active-wal-exit-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let temp = Temp {
            root: root.clone(),
            parent,
        };
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "active_wal_actual_exit_child", "--nocapture"])
            .env("TL_ACTIVE_WAL_EXIT_ROOT", &root)
            .env("TL_ACTIVE_WAL_EXIT_CUT", cut)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(EXIT_CODE), "real child boundary {cut}");
        let fact = read_fact(&root.join("independent-wal-seal.json"));
        let expected = wal_pin(fact.get("post"));
        let (wal, _) = Journal::open_existing_verified_prefix(
            &root,
            Namespace::new("LTC", false, "consensus").unwrap(),
            Recovery::RejectIncompleteTail,
            &expected,
        )
        .unwrap();
        let actual = crate::property_catalog::parse_diagnostic_value(
            std::str::from_utf8(wal.get("active-service/record/v1").unwrap().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(actual, fact.get("record"));
        assert_eq!(
            actual.get("stage"),
            s(if cut == "Native" { "Intent" } else { cut })
        );
        for raw in arr(fact.get("blobs")) {
            let bytes = std::fs::read(
                root.join("active-service-blobs-v1")
                    .join(format!("{}.asw1", text(raw.get("sha256")))),
            )
            .unwrap();
            assert_eq!(bytes.len() as u64, uint(raw.get("bytes")));
            assert_eq!(sha256(&bytes), hash(raw.get("sha256")));
        }
        if cut == "Native" {
            let native = read_fact(&root.join("independent-native-seal.json"));
            let expected = native_pin(native.get("post"));
            let (index, _) =
                crate::compact_journal::CompactingJournal::open_existing_verified_prefix(
                    &root,
                    Namespace::new("LTC", false, "txIndex").unwrap(),
                    Recovery::RejectIncompleteTail,
                    &expected,
                )
                .unwrap();
            assert_eq!(index.verified_prefix_pin().unwrap(), expected);
        }
        // This only verifies persistent prefixes. No lost financial snapshot or
        // supplied outcome is imported, and no cold service resume is claimed.
        drop(wal);
        drop(temp);
    }
}
