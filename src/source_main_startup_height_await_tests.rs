use super::*;
use std::collections::BTreeMap;

fn runtime() -> SourceContractRuntime {
    SourceContractRuntime::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
}
fn seed(runtime: &mut SourceContractRuntime, key: NativeMainStartupHeightKeyV1, value: V) {
    runtime
        .import_collection(
            "consensus",
            vec![obj(&[("_id", s(key.key())), ("value", value)])],
        )
        .unwrap();
}
fn returned(frame: &mut NativeMainStartupHeightReadV1) -> V {
    match frame.take_result().unwrap() {
        Ok(value) => value,
        Err(error) => panic!("actual height function unexpectedly threw {error:?}"),
    }
}
fn find_count(runtime: &SourceContractRuntime) -> usize {
    runtime
        .core
        .effects
        .iter()
        .filter(|effect| {
            effect.get("kind") == s("db")
                && effect.get("collection") == s("consensus")
                && effect.get("method") == s("findOneAsync")
        })
        .count()
}
fn fields(frame: &NativeMainStartupHeightReadV1) -> BTreeMap<String, V> {
    let mut values = BTreeMap::new();
    frame
        .visit_fields(&mut |path, value| {
            assert!(
                values.insert(path.into(), value.clone()).is_none(),
                "{path}"
            );
            Ok(())
        })
        .unwrap();
    values
}
#[test]
fn actual_factory_then_query_sees_later_write_and_original_query_result_survives_another_write() {
    for key in [
        NativeMainStartupHeightKeyV1::Track,
        NativeMainStartupHeightKeyV1::MaxProcessed,
    ] {
        let mut owner = runtime();
        seed(&mut owner, key, n(1.));
        let mut read = owner.native_main_startup_height_read_begin(key).unwrap();
        assert_eq!(
            read.stop(),
            NativeMainStartupHeightAwaitStopV1::FactoryAwait
        );
        assert_eq!(find_count(&owner), 0);
        assert_eq!(
            read.take_result().err(),
            Some(SourceRuleError::InvalidInput)
        );
        owner
            .core
            .source_main_height_write(key.key(), n(9.))
            .unwrap();
        assert_eq!(
            owner.native_main_startup_height_read_resume(&mut read),
            NativeMainStartupHeightAwaitStopV1::QueryAwait
        );
        assert_eq!(find_count(&owner), 1);
        assert_eq!(fields(&read).get("query/returned/1/value"), Some(&n(9.)));
        owner
            .core
            .source_main_height_write(key.key(), n(10.))
            .unwrap();
        let effects = owner.core.effects.clone();
        assert_eq!(
            owner.native_main_startup_height_read_resume(&mut read),
            NativeMainStartupHeightAwaitStopV1::Complete
        );
        assert_eq!(
            returned(&mut read),
            n(9.),
            "deliver the already executed result, never requery current height10"
        );
        assert_eq!(owner.core.effects, effects, "delivery performs no extra IO");
        assert_eq!(owner.collection("consensus")[0].get("value"), n(10.));
        assert_eq!(
            owner.native_main_startup_height_read_resume(&mut read),
            NativeMainStartupHeightAwaitStopV1::Consumed
        );
        assert_eq!(
            read.take_result().err(),
            Some(SourceRuleError::InvalidInput)
        );
        assert_eq!(find_count(&owner), 1);
    }
}
#[test]
fn actual_factory_rejection_waits_then_propagates_without_query_or_failure_hook_replay() {
    let mut owner = runtime();
    owner
        .fail_storage(StorageFailure {
            collection: "consensus".into(),
            operation: "getDatabase".into(),
            skip: 0,
            message: "actual factory boundary".into(),
        })
        .unwrap();
    let mut read = owner
        .native_main_startup_height_read_begin(NativeMainStartupHeightKeyV1::Track)
        .unwrap();
    assert_eq!(
        read.stop(),
        NativeMainStartupHeightAwaitStopV1::FactoryAwait
    );
    assert_eq!(find_count(&owner), 0);
    assert_eq!(owner.core.failures.len(), 0);
    assert_eq!(
        read.take_result().err(),
        Some(SourceRuleError::InvalidInput)
    );
    let effects = owner.core.effects.clone();
    assert_eq!(
        owner.native_main_startup_height_read_resume(&mut read),
        NativeMainStartupHeightAwaitStopV1::Complete
    );
    assert_eq!(owner.core.effects, effects);
    match read.take_result().unwrap() {
        Err(Fault::Thrown(error)) => {
            assert_eq!(error.name, "Error");
            assert_eq!(error.message, "actual factory boundary");
        }
        other => panic!("original factory throw {other:?}"),
    }
    assert_eq!(find_count(&owner), 0);
}
#[test]
fn query_failure_installed_during_factory_await_is_called_once_then_rethrown_after_real_query_await(
) {
    let mut owner = runtime();
    let mut read = owner
        .native_main_startup_height_read_begin(NativeMainStartupHeightKeyV1::MaxProcessed)
        .unwrap();
    owner
        .fail_storage(StorageFailure {
            collection: "consensus".into(),
            operation: "findOneAsync".into(),
            skip: 0,
            message: "actual later query boundary".into(),
        })
        .unwrap();
    assert_eq!(owner.core.failures.len(), 1);
    assert_eq!(
        owner.native_main_startup_height_read_resume(&mut read),
        NativeMainStartupHeightAwaitStopV1::QueryAwait
    );
    assert_eq!(owner.core.failures.len(), 0);
    assert_eq!(
        read.take_result().err(),
        Some(SourceRuleError::InvalidInput)
    );
    let effects = owner.core.effects.clone();
    assert_eq!(
        owner.native_main_startup_height_read_resume(&mut read),
        NativeMainStartupHeightAwaitStopV1::Complete
    );
    assert_eq!(owner.core.effects, effects);
    match read.take_result().unwrap() {
        Err(Fault::Thrown(error)) => assert_eq!(error.message, "actual later query boundary"),
        other => panic!("original query throw {other:?}"),
    }
    assert_eq!(find_count(&owner), 1);
}
#[test]
fn exact_existing_height_values_and_missing_null_are_not_numeric_normalized() {
    for key in [
        NativeMainStartupHeightKeyV1::Track,
        NativeMainStartupHeightKeyV1::MaxProcessed,
    ] {
        for value in [
            V::Undefined,
            V::Null,
            V::Bool(false),
            s("9"),
            n(0.),
            obj(&[("arbitraryPersistedField", s("retained"))]),
        ] {
            let mut owner = runtime();
            seed(&mut owner, key, value.clone());
            let mut read = owner.native_main_startup_height_read_begin(key).unwrap();
            owner.native_main_startup_height_read_resume(&mut read);
            owner.native_main_startup_height_read_resume(&mut read);
            assert_eq!(returned(&mut read), value);
        }
        let mut owner = runtime();
        let mut read = owner.native_main_startup_height_read_begin(key).unwrap();
        owner.native_main_startup_height_read_resume(&mut read);
        owner.native_main_startup_height_read_resume(&mut read);
        assert_eq!(returned(&mut read), V::Null);
    }
}
#[test]
fn actual_effect_budget_stop_retains_factory_prefix_and_never_becomes_source_rejection() {
    let mut owner = runtime();
    owner.core.effects = vec![V::Null; 65_536];
    let mut read = owner
        .native_main_startup_height_read_begin(NativeMainStartupHeightKeyV1::Track)
        .unwrap();
    assert_eq!(
        read.stop(),
        NativeMainStartupHeightAwaitStopV1::SourceOwnerStopped(SourceRuleError::Capacity)
    );
    assert_eq!(fields(&read).get("factory/present"), Some(&V::Bool(true)));
    let tables = owner.core.tables.clone();
    let effects = owner.core.effects.clone();
    assert_eq!(
        owner.native_main_startup_height_read_resume(&mut read),
        NativeMainStartupHeightAwaitStopV1::SourceOwnerStopped(SourceRuleError::Capacity)
    );
    assert_eq!(
        read.take_result().err(),
        Some(SourceRuleError::InvalidInput)
    );
    assert_eq!(owner.core.tables, tables);
    assert_eq!(owner.core.effects, effects);
    assert_eq!(find_count(&owner), 0);
}
#[test]
fn default_policy_and_all_old_dispatch_namespaces_remain_unadmitted() {
    let mut default = SourceContractRuntime::new();
    let before = default.state_value().unwrap();
    assert_eq!(
        default
            .native_main_startup_height_read_begin(NativeMainStartupHeightKeyV1::Track)
            .err(),
        Some(SourceRuleError::InvalidInput)
    );
    assert_eq!(default.state_value().unwrap(), before);
    for policy in [
        SourceContractRuntimePolicy::GlobalClearingV2,
        SourceContractRuntimePolicy::MainOrchestrationV12,
    ] {
        let mut owner = SourceContractRuntime::with_policy(policy);
        let before = owner.state_value().unwrap();
        assert_eq!(
            owner.invoke_dependency(&DependencyCall {
                target: "SourceMainStartupAwait.height".into(),
                arguments: vec![]
            }),
            Err(SourceRuleError::UnknownDependency)
        );
        assert_eq!(owner.state_value().unwrap(), before);
    }
}
