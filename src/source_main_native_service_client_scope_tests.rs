use super::*;
fn configured() -> SourceContractRuntime {
    let mut runtime =
        SourceContractRuntime::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12);
    let client = obj(&[("chain", s("LTC")), ("test", V::Bool(false))]);
    runtime
        .core
        .source_activation_configure(std::slice::from_ref(&client))
        .unwrap();
    runtime
        .core
        .source_main_production_configure(&[obj(&[
            ("client", client),
            (
                "io",
                obj(&[
                    ("height", n(11.)),
                    ("blocks", V::Array(vec![])),
                    ("failures", V::Array(vec![])),
                ]),
            ),
            ("genesisBlock", n(10.)),
        ])])
        .unwrap();
    runtime
        .core
        .source_main_service_configure(&[obj(&[
            ("fileInputs", V::Array(vec![])),
            ("directoryInputs", V::Array(vec![])),
            (
                "fileIOInputs",
                obj(&[("failures", V::Array(vec![])), ("mtimes", V::Array(vec![]))]),
            ),
        ])])
        .unwrap();
    runtime
        .native_main_bind(obj(&[("profile", s("actual-client-scope-test"))]))
        .unwrap();
    runtime.take_effects();
    runtime
}
#[test]
fn ordinary_native_service_flags_do_not_enable_shared_client_without_frame_loan() {
    let mut runtime = configured();
    let returned = runtime
        .core
        .source_activation_client("Client.getInstance", &[])
        .unwrap();
    assert_eq!(returned, obj(&[("sourceActivationClient", V::Bool(true))]));
    assert_eq!(
        runtime.core.effects,
        vec![obj(&[
            ("kind", s("rpc-input")),
            ("target", s("Client.getInstance")),
            ("args", V::Array(vec![]))
        ])]
    );
    assert_eq!(runtime.native_main_service_client_scope_value(), V::Null);
    let before = runtime.core.effects.clone();
    assert_eq!(
        runtime.native_main_service_with_client_scope("foreign-profile", |_| panic!(
            "foreign profile executed"
        )),
        Err::<(), _>(SourceRuleError::InvalidInput)
    );
    assert_eq!(runtime.core.effects, before);
}
#[test]
fn explicit_scope_uses_same_actual_client_counter_and_closes_after_return() {
    let mut runtime = configured();
    runtime
        .native_main_service_with_client_scope(
            NATIVE_MAIN_ACTIVE_SERVICE_CLIENT_BINDING,
            |runtime| {
                assert!(
                    format!("{:?}", runtime.core).contains("native_active_service_client_scope")
                );
                assert_eq!(
                    runtime
                        .core
                        .source_activation_client("Client.getInstance", &[])
                        .unwrap(),
                    obj(&[("sourceActivationClient", V::Bool(true))])
                );
                assert_eq!(
                    runtime
                        .core
                        .source_activation_client("Client.getChain", &[])
                        .unwrap(),
                    s("LTC")
                );
            },
        )
        .unwrap();
    assert_eq!(runtime.native_main_service_client_scope_value(), V::Null);
    assert!(!format!("{:?}", runtime.core).contains("native_active_service_client_scope"));
    runtime
        .core
        .source_main_production_io("Client.getChain", &[])
        .unwrap();
    assert_eq!(runtime.core.effects[1].get("ordinal"), n(1.));
    assert_eq!(runtime.core.effects[2].get("ordinal"), n(2.));
    assert_eq!(
        runtime.core.effects[0].get("returned"),
        obj(&[("chain", s("LTC")), ("test", V::Bool(false))])
    );
}
#[test]
fn rust_unwind_closes_loan_without_rolling_back_actual_io_or_fabricating_source_result() {
    let mut runtime = configured();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = runtime.native_main_service_with_client_scope(
            NATIVE_MAIN_ACTIVE_SERVICE_CLIENT_BINDING,
            |runtime| {
                runtime
                    .core
                    .source_activation_client("Client.getChain", &[])
                    .unwrap();
                panic!("retained actual client prefix");
            },
        );
    }));
    assert!(panic.is_err());
    assert_eq!(runtime.native_main_service_client_scope_value(), V::Null);
    assert_eq!(runtime.core.effects.len(), 1);
    assert_eq!(runtime.core.effects[0].get("ordinal"), n(1.));
}
