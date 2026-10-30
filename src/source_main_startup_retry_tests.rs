// Additive actual-Core tests; reuse real raw-startup owner/helpers, never an
// alternate financial adapter. Parent owns first private-Core compilation.
#[test]
fn early_module_rejection_retries_real_db_with_distinct_internal_promise() {
    let raw = include_str!("../fixtures/startup_init_suffix.jsonl")
        .lines()
        .find(|line| {
            json_string(raw_get(line, "kind")) == "input"
                && json_string(raw_get(line, "case"))
                    == "suffix/14-module-db-rejects-before1500-main-retries-actual-db"
        })
        .unwrap();
    let temp = Temp::new();
    let (financial, index, expected, mut inputs) = setup(&temp, raw);
    assert_eq!(
        untag(raw_get(raw, "rawStartupInputs")).get("autoRejectEarly"),
        V::Bool(true)
    );
    inputs.automatic_client_error = Some(NativeStartupIoErrorV1 {
        name: "Error".into(),
        message: "recorded auto DB client rejection".into(),
        code: s("EIO"),
    });
    let mut owner = SourceMainRawStartupV1::begin(financial, index, &expected, inputs, 9).unwrap();
    owner.apply(StartupCommandV1::ConstructMain).unwrap();
    owner.apply(StartupCommandV1::EnterInitialize).unwrap();
    drain(&mut owner);
    owner
        .apply(StartupCommandV1::SettleClient { token: 1 })
        .unwrap();
    drain(&mut owner);
    let original_error = fields(&owner).get("notification/error").unwrap().clone();
    assert_eq!(
        fields(&owner).get("notification/notified"),
        Some(&V::Bool(true))
    );
    for _ in 0..2 {
        let (id, _, _) = owner.next_timer().unwrap();
        owner
            .apply(StartupCommandV1::FireTimer { id, generation: 9 })
            .unwrap();
        drain(&mut owner);
    }
    assert_eq!(owner.stop(), StartupStopV1::Pending);
    assert_eq!(
        owner.next_timer().map(|(_, delay, site)| (delay, site)),
        Some((300, "Main.delay"))
    );
    assert_eq!(fields(&owner).get("database/round"), Some(&s("2")));
    assert_eq!(
        fields(&owner).get("notification/error"),
        Some(&original_error)
    );
    assert_eq!(
        fields(&owner).get("database/initialized"),
        Some(&V::Bool(true))
    );
    assert_eq!(
        owner
            .effects()
            .iter()
            .filter(|e| e.get("kind") == s("unhandled-rejection"))
            .count(),
        1
    );
    assert_eq!(
        owner
            .effects()
            .iter()
            .filter(|e| e.get("kind") == s("datastore-open"))
            .count(),
        32
    );
    assert_eq!(owner.index.snapshot().unwrap(), expected.metadata);
    let (id, _, _) = owner.next_timer().unwrap();
    owner
        .apply(StartupCommandV1::FireTimer { id, generation: 9 })
        .unwrap();
    drain(&mut owner);
    assert_eq!(
        owner.stop(),
        StartupStopV1::InitializeSuffixReady,
        "handoff remains unfulfilled; do not fake the source suffix"
    );
}
#[test]
fn retry_datastore_failure_is_new_awaited_error_not_second_module_notification() {
    let raw = include_str!("../fixtures/startup_init_suffix.jsonl")
        .lines()
        .find(|line| {
            json_string(raw_get(line, "kind")) == "input"
                && json_string(raw_get(line, "case"))
                    == "suffix/15-old-module-rejection-distinct-from-new-db-failure"
        })
        .unwrap();
    let temp = Temp::new();
    let (financial, index, expected, mut inputs) = setup(&temp, raw);
    inputs.automatic_client_error = Some(NativeStartupIoErrorV1 {
        name: "Error".into(),
        message: "recorded auto DB client rejection".into(),
        code: s("EIO"),
    });
    assert_eq!(inputs.datastore_error.as_ref().unwrap().category, "txIndex");
    let mut owner = SourceMainRawStartupV1::begin(financial, index, &expected, inputs, 9).unwrap();
    owner.apply(StartupCommandV1::ConstructMain).unwrap();
    owner.apply(StartupCommandV1::EnterInitialize).unwrap();
    drain(&mut owner);
    owner
        .apply(StartupCommandV1::SettleClient { token: 1 })
        .unwrap();
    drain(&mut owner);
    let old = fields(&owner).get("notification/error").unwrap().clone();
    for _ in 0..2 {
        let (id, _, _) = owner.next_timer().unwrap();
        owner
            .apply(StartupCommandV1::FireTimer { id, generation: 9 })
            .unwrap();
        drain(&mut owner);
    }
    let StartupStopV1::Thrown(current) = owner.stop() else {
        panic!("new actual datastore failure")
    };
    assert_eq!(current.code, s("EACCES"));
    assert_eq!(
        current.exception.message,
        "recorded datastore constructor rejection txIndex"
    );
    assert_ne!(s(current.identity.to_string()), old.get("identity"));
    assert_eq!(fields(&owner).get("notification/error"), Some(&old));
    assert_eq!(
        fields(&owner).get("database/categories/count"),
        Some(&s("2"))
    );
    assert_eq!(
        owner
            .effects()
            .iter()
            .filter(|e| e.get("kind") == s("unhandled-rejection"))
            .count(),
        1
    );
    assert_eq!(owner.index.snapshot().unwrap(), expected.metadata);
    assert_eq!(owner.next_timer(), None);
}
