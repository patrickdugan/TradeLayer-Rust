use super::*;
use std::collections::BTreeMap;
fn runtime() -> SourceContractRuntime {
    SourceContractRuntime::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
}
fn query() -> V {
    obj(&[("_id", s("TrackHeight"))])
}
fn seeded() -> SourceContractRuntime {
    let mut runtime = runtime();
    runtime
        .import_collection(
            "consensus",
            vec![obj(&[("_id", s("TrackHeight")), ("value", n(1.))])],
        )
        .unwrap();
    runtime
}
fn fields(frame: &NativeMainStartupDbQueryV1) -> BTreeMap<String, V> {
    let mut out = BTreeMap::new();
    frame
        .visit_fields(&mut |path, value| {
            assert!(out.insert(path.into(), value.clone()).is_none(), "{path}");
            Ok(())
        })
        .unwrap();
    out
}
#[test]
fn request_is_not_sampling_and_sampled_result_is_not_requeried_at_callback() {
    let mut owner = seeded();
    let mut call = owner
        .native_main_startup_db_query_request("consensus", "findOneAsync", vec![query()])
        .unwrap();
    assert!(!call.request_rejected());
    assert!(!call.sampled());
    assert!(!call.callback_delivered());
    assert!(call.actual_result().is_none());
    assert!(
        owner.core.effects.is_empty(),
        "request record exists on actual frame, not a fabricated completed DB event"
    );
    owner
        .core
        .source_main_height_write("TrackHeight", n(9.))
        .unwrap();
    owner
        .native_main_startup_db_query_sample_actual(&mut call)
        .unwrap();
    assert!(call.sampled());
    owner
        .core
        .source_main_height_write("TrackHeight", n(10.))
        .unwrap();
    owner
        .native_main_startup_db_query_deliver_callback(&mut call)
        .unwrap();
    assert!(call.callback_delivered());
    assert!(matches!(call.actual_result(),Some(Ok(row)) if row.get("value")==n(9.)));
    let before = owner.core.effects.clone();
    assert!(matches!(call.take_actual_result(),Ok(Ok(row)) if row.get("value")==n(9.)));
    assert_eq!(owner.collection("consensus")[0].get("value"), n(10.));
    assert_eq!(
        call.take_actual_result().err(),
        Some(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        owner.native_main_startup_db_query_sample_actual(&mut call),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        owner.native_main_startup_db_query_deliver_callback(&mut call),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(owner.core.effects, before);
}
#[test]
fn request_hook_is_consumed_once_before_sample_and_later_queries_still_see_armed_failure() {
    let mut owner = seeded();
    owner
        .fail_storage(StorageFailure {
            collection: "consensus".into(),
            operation: "findOneAsync".into(),
            skip: 1,
            message: "second real request only".into(),
        })
        .unwrap();
    let mut call = owner
        .native_main_startup_db_query_request("consensus", "findOneAsync", vec![query()])
        .unwrap();
    assert_eq!(owner.core.failures[0].skip, 0);
    owner
        .native_main_startup_db_query_sample_actual(&mut call)
        .unwrap();
    owner
        .native_main_startup_db_query_deliver_callback(&mut call)
        .unwrap();
    assert!(matches!(call.take_actual_result(),Ok(Ok(row)) if row.get("value")==n(1.)));
    assert_eq!(
        owner.core.failures.len(),
        1,
        "sampling/callback must not charge the hook again"
    );
    assert!(
        matches!(owner.core.find("consensus",query()),Err(Fault::Thrown(error)) if error.message=="second real request only")
    );
}
#[test]
fn actual_request_rejection_has_no_sampler_or_table_creation_and_is_delivered_once() {
    let mut owner = runtime();
    owner
        .fail_storage(StorageFailure {
            collection: "consensus".into(),
            operation: "findOneAsync".into(),
            skip: 0,
            message: "request-time original failure".into(),
        })
        .unwrap();
    let mut call = owner
        .native_main_startup_db_query_request("consensus", "findOneAsync", vec![query()])
        .unwrap();
    assert!(call.request_rejected());
    assert!(owner.core.tables.is_empty());
    let before = fields(&call);
    assert_eq!(
        owner.native_main_startup_db_query_sample_actual(&mut call),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(fields(&call), before);
    owner
        .native_main_startup_db_query_deliver_callback(&mut call)
        .unwrap();
    assert!(
        matches!(call.take_actual_result(),Ok(Err(Fault::Thrown(error))) if error.message=="request-time original failure")
    );
    assert!(owner.core.tables.is_empty());
    assert!(owner.core.failures.is_empty());
}
#[test]
fn post_sample_recording_capacity_retains_real_result_and_prevents_callback_replay() {
    let mut owner = seeded();
    let mut call = owner
        .native_main_startup_db_query_request("consensus", "findOneAsync", vec![query()])
        .unwrap();
    owner
        .native_main_startup_db_query_sample_actual(&mut call)
        .unwrap();
    owner.core.effects = vec![V::Null; 65_536];
    assert_eq!(
        owner.native_main_startup_db_query_deliver_callback(&mut call),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(call.stopped(), Some(&SourceRuleError::Capacity));
    let retained = fields(&call);
    assert_eq!(retained.get("sampled/returned/1/value"), Some(&n(1.)));
    assert_eq!(retained.get("callback/returned/1/value"), Some(&n(1.)));
    assert!(call.actual_result().is_none());
    assert_eq!(
        call.take_actual_result().err(),
        Some(SourceRuleError::InvalidInput)
    );
    let effects = owner.core.effects.clone();
    assert_eq!(
        owner.native_main_startup_db_query_deliver_callback(&mut call),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(fields(&call), retained);
    assert_eq!(owner.core.effects, effects);
}
#[test]
fn wrong_query_domain_and_default_policy_refuse_before_hook_or_state_changes() {
    let mut owner = seeded();
    let before = owner.state_value().unwrap();
    assert_eq!(
        owner
            .native_main_startup_db_query_request("txIndex", "findAsync", vec![obj(&[])])
            .err(),
        Some(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        owner
            .native_main_startup_db_query_request("consensus", "updateAsync", vec![query()])
            .err(),
        Some(SourceRuleError::InvalidInput)
    );
    assert_eq!(owner.state_value().unwrap(), before);
    let mut owner = SourceContractRuntime::new();
    let before = owner.state_value().unwrap();
    assert_eq!(
        owner
            .native_main_startup_db_query_request("consensus", "findOneAsync", vec![query()])
            .err(),
        Some(SourceRuleError::InvalidInput)
    );
    assert_eq!(owner.state_value().unwrap(), before);
}
fn transported(result: Result<DependencyReturn, SourceRuleError>) -> V {
    match result {
        Ok(DependencyReturn::Immediate(value)) => {
            obj(&[("kind", s("Immediate")), ("value", value)])
        }
        Ok(DependencyReturn::Pending(value)) => obj(&[("kind", s("Pending")), ("value", value)]),
        Ok(DependencyReturn::Throw(error)) => obj(&[
            ("kind", s("Throw")),
            ("name", s(error.name)),
            ("message", s(error.message)),
        ]),
        Ok(DependencyReturn::PendingThrow(error)) => obj(&[
            ("kind", s("PendingThrow")),
            ("name", s(error.name)),
            ("message", s(error.message)),
        ]),
        Err(error) => obj(&[("local", s(format!("{error:?}")))]),
    }
}
#[test]
fn old_direct_db_recording_state_and_errors_match_independently_captured_actual_cfbae_provider() {
    let mut owner = runtime();
    owner
        .import_collection(
            "consensus",
            vec![obj(&[
                ("_id", s("TrackHeight")),
                ("value", n(1.)),
                ("nested", V::Array(vec![s("preserved"), V::Null])),
            ])],
        )
        .unwrap();
    let handle = obj(&[("sourceDatabase", s("consensus"))]);
    let query = query();
    let calls = vec![
        DependencyCall {
            target: "db.getDatabase".into(),
            arguments: vec![s("consensus")],
        },
        DependencyCall {
            target: "db.findOneAsync".into(),
            arguments: vec![handle.clone(), query.clone()],
        },
        DependencyCall {
            target: "db.findAsync".into(),
            arguments: vec![handle.clone(), obj(&[])],
        },
        DependencyCall {
            target: "db.findOneAsync".into(),
            arguments: vec![handle.clone(), query.clone()],
        },
        DependencyCall {
            target: "db.updateAsync".into(),
            arguments: vec![
                handle.clone(),
                query.clone(),
                obj(&[("$set", obj(&[("value", n(2.))]))]),
                obj(&[("upsert", V::Bool(true))]),
            ],
        },
        DependencyCall {
            target: "db.findOneAsync".into(),
            arguments: vec![handle, query],
        },
    ];
    let old = include_str!("../fixtures/startup_await_old_db_complete.jsonl")
        .lines()
        .map(|line| crate::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), old.len());
    for (step, call) in calls.into_iter().enumerate() {
        if step == 3 {
            owner
                .fail_storage(StorageFailure {
                    collection: "consensus".into(),
                    operation: "findOneAsync".into(),
                    skip: 0,
                    message: "independently captured original query failure".into(),
                })
                .unwrap();
        }
        let result = transported(owner.invoke_dependency(&call));
        let effects = owner.take_effects();
        let actual = obj(&[
            ("step", n(step as f64)),
            ("target", s(call.target)),
            ("result", result),
            ("effects", V::Array(effects)),
            ("state", owner.state_value().unwrap()),
        ]);
        assert_eq!(
            actual, old[step],
            "actual old provider step{step}; no self-alias comparison"
        );
    }
}
