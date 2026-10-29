use super::*;
static PROBE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn actual_commit_probe(plan: &PreparedActiveServiceWalWriteV1, committed: bool) {
    assert!(plan.attempted());
    assert_ne!(plan.before(), plan.post());
    PROBE.fetch_or(
        if committed { 2 } else { 1 },
        std::sync::atomic::Ordering::SeqCst,
    );
}
#[test]
fn actual_wal_probe_distinguishes_precommit_from_synced_commit() {
    let temp = Temp::new();
    let mut wal = store(&temp);
    let mut prepared = plan(&wal, "Intent");
    PROBE.store(0, std::sync::atomic::Ordering::SeqCst);
    wal.set_test_commit_probe(actual_commit_probe);
    wal.attempt(&mut prepared).unwrap();
    assert_eq!(PROBE.load(std::sync::atomic::Ordering::SeqCst), 3);
    assert_eq!(wal.pin().unwrap(), *prepared.post());
}
#[test]
fn actual_generic_store_pins_new_start_profile_and_exact_raw_input_before_accepted() {
    let temp = Temp::new();
    let action = crate::source_main_retained_caller_action::RetainedMainCallerActionV1::Start {
        caller: crate::source_main_retained_caller_action::RetainedMainCallerStartV1::Initialize,
        scope: 501,
        generation: 71,
        documents: V::Array(vec![]),
    };
    let raw = action.canonical_action_bytes().unwrap();
    let raw_pin = ActiveServiceWalBlobV1::of(&raw).unwrap();
    let origin = obj(&[
        ("profile", s("TL-MAIN-RETAINED-CALLER-START-WAL-13-1")),
        ("admittedActions", s("StartOnly")),
        ("rawStart", raw_pin.value()),
    ]);
    let mut wal = ActiveServiceWalV1::create(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        origin.clone(),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let actual_origin = wal.origin_context().unwrap();
    assert_eq!(actual_origin.get("origin"), origin);
    let record = obj(&[
        ("profile", s("TL-MAIN-RETAINED-CALLER-START-WAL-13-1")),
        ("stage", s("Intent")),
        ("rawAction", raw_pin.value()),
        ("constructorDispatched", V::Bool(false)),
    ]);
    let mut prepared = wal
        .prepare(
            &wal.pin().unwrap(),
            record.clone(),
            vec![raw.clone()],
            vec![raw_pin.clone()],
        )
        .unwrap();
    let before = prepared.before().clone();
    let post = prepared.post().clone();
    wal.set_test_fault(TestFault::AfterCommit);
    assert!(wal.attempt(&mut prepared).is_err());
    assert!(matches!(wal.pin(), Err(ActiveServiceWalErrorV1::Ambiguous)));
    assert_eq!(
        wal.resolve(&prepared).unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(wal.pin().unwrap(), post);
    assert_ne!(post, before);
    assert_eq!(wal.record().unwrap(), record);
    assert_eq!(wal.origin_context().unwrap(), actual_origin);
    assert_eq!(wal.read_blob(&raw_pin).unwrap(), raw);
    assert_eq!(crate::source_main_retained_caller_action::RetainedMainCallerActionV1::from_canonical_action_bytes(&raw).unwrap(),action);
    assert!(matches!(
        wal.rearm_before(&prepared),
        Err(ActiveServiceWalErrorV1::WrongPin)
    ));
}
#[test]
fn real_before_retry_keeps_same_new_profile_raw_start_and_blob_dependencies() {
    let temp = Temp::new();
    let action = crate::source_main_retained_caller_action::RetainedMainCallerActionV1::Start {
        caller: crate::source_main_retained_caller_action::RetainedMainCallerStartV1::InitOrLoad,
        scope: 501,
        generation: 71,
        documents: V::Array(vec![]),
    };
    let raw = action.canonical_action_bytes().unwrap();
    let pin = ActiveServiceWalBlobV1::of(&raw).unwrap();
    let origin = obj(&[
        ("profile", s("TL-MAIN-RETAINED-CALLER-START-WAL-13-1")),
        ("admittedActions", s("StartOnly")),
        ("rawStart", pin.value()),
    ]);
    let mut wal = ActiveServiceWalV1::create(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        origin.clone(),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let mut prepared = wal
        .prepare(
            &wal.pin().unwrap(),
            obj(&[
                ("profile", s("TL-MAIN-RETAINED-CALLER-START-WAL-13-1")),
                ("stage", s("Intent")),
                ("rawAction", pin.value()),
            ]),
            vec![raw.clone()],
            vec![pin.clone()],
        )
        .unwrap();
    wal.set_test_fault(TestFault::BeforeCommit);
    assert!(wal.attempt(&mut prepared).is_err());
    assert_eq!(
        wal.resolve(&prepared).unwrap(),
        ActiveServiceWalResolvedV1::Before
    );
    let record = prepared.record().unwrap();
    let refs = prepared.references().to_vec();
    let mut replacement = wal.rearm_before(&prepared).unwrap();
    assert_eq!(replacement.record().unwrap(), record);
    assert_eq!(replacement.references(), refs);
    wal.attempt(&mut replacement).unwrap();
    assert_eq!(wal.record().unwrap(), record);
    assert_eq!(wal.origin_context().unwrap().get("origin"), origin);
    assert_eq!(wal.read_blob(&pin).unwrap(), raw);
}
