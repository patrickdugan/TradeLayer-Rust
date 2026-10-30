#[test]
fn sdk_own_fields_are_distinct_from_actual_chain_rpc_facts_on_one_owner() {
    let raw = FIXTURE.lines().next().unwrap();
    let temp = Temp::new();
    let (financial, index, expected, inputs) = setup(&temp, raw);
    assert_eq!(inputs.sdk_client_fields.get("chain"), V::Undefined);
    assert_eq!(inputs.chain, "LTC");
    let mut owner = SourceMainRawStartupV1::begin(financial, index, &expected, inputs, 9).unwrap();
    owner.apply(StartupCommandV1::ConstructMain).unwrap();
    for event in owner
        .effects()
        .iter()
        .filter(|e| e.get("kind") == s("rpc-input") && e.get("method") == s("Client.getInstance"))
    {
        assert_eq!(
            event.get("returned").get("chain"),
            V::Undefined,
            "actual declared SDK own field"
        );
    }
    owner
        .apply(StartupCommandV1::SettleClient { token: 1 })
        .unwrap();
    drain(&mut owner);
    assert_eq!(
        fields(&owner).get("database/chain"),
        Some(&s("LTC")),
        "actual getChain method data, not own chain field"
    );
    assert_eq!(
        fields(&owner).get("database/path"),
        Some(&s("/tl-data/ltc-main"))
    );
    assert!(owner
        .effects()
        .iter()
        .any(|event| event.get("method") == s("Client.getChain")
            && event.get("returned") == s("LTC")));
    assert_eq!(owner.index.snapshot().unwrap(), expected.metadata);
    assert_eq!(
        owner
            .financial_state()
            .unwrap()
            .get("runtime")
            .get("mainProduction")
            .get("client")
            .get("chain"),
        s("LTC")
    );
}
#[test]
fn conflicting_claimed_main_sdk_alias_refuses_before_configuration() {
    let raw = FIXTURE.lines().next().unwrap();
    let temp = Temp::new();
    let (financial, index, expected, mut inputs) = setup(&temp, raw);
    let before = financial.checkpoint().unwrap();
    inputs.sdk_client_fields = obj(&[
        ("chain", s("different recorded object")),
        ("test", V::Bool(false)),
    ]);
    let rejected = SourceMainRawStartupV1::begin(financial, index, &expected, inputs, 9)
        .err()
        .expect("data-profile alias mismatch");
    assert_eq!(rejected.error, SourceRuleError::InvalidInput);
    let (financial, index) = rejected.into_owners();
    assert_eq!(financial.checkpoint().unwrap(), before);
    assert_eq!(index.snapshot().unwrap(), expected.metadata);
}
