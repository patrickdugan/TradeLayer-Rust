//! Whole pinned modules, actual mutable financial owners, declared storage and
//! environment boundaries. No mocked validation, margin or settlement verdicts.
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceOutcome, SourceRuleError,
    SourceValue as V,
};

const ORACLE: &str = include_str!("../scripts/channel_queue_large_financial.cjs");

fn string(v: V) -> String {
    let V::String(t) = v else {
        panic!("string {v:?}")
    };
    t
}
fn array(v: V) -> Vec<V> {
    let V::Array(t) = v else {
        panic!("array {v:?}")
    };
    t
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
}
fn observed(result: DependencyReturn) -> V {
    match result {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => obj(&[("returned", v)]),
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => obj(&[(
            "thrown",
            obj(&[
                ("name", V::String(e.name)),
                ("message", V::String(e.message)),
            ]),
        )]),
    }
}
fn source_observed(result: SourceOutcome) -> V {
    match result {
        SourceOutcome::Returned(v) => obj(&[("returned", v)]),
        SourceOutcome::Thrown(e) => obj(&[(
            "thrown",
            obj(&[
                ("name", V::String(e.name)),
                ("message", V::String(e.message)),
            ]),
        )]),
    }
}
fn cases() -> Vec<V> {
    let result = std::process::Command::new("node")
        .args(["-e", ORACLE, env!("CARGO_MANIFEST_DIR")])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let cases = std::str::from_utf8(&result.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            tradelayer_core::property_catalog::parse_diagnostic_value(line)
                .unwrap_or_else(|e| panic!("{e:?}: {}", &line[..line.len().min(200)]))
        })
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 6, "unchanged source observation inventory");
    cases
}

struct ActualBackend<'a>(&'a mut SourceContractRuntime);
impl SourceBackend for ActualBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        self.0.invoke_dependency(call)
    }
}
fn compare_graph(detailed_tokens: bool) {
    let mut runtime =
        SourceContractRuntime::with_policy(SourceContractRuntimePolicy::SourceLifecycleV3);
    let mut calls = 0;
    let mut raw_calls = 0;
    let mut effect_errors = vec![];
    for case in cases() {
        let name = string(case.get("name"));
        let c = case.get("command");
        if !matches!(c.get("reset"), V::Undefined) {
            runtime =
                SourceContractRuntime::with_policy(SourceContractRuntimePolicy::SourceLifecycleV3);
            if let V::String(entry) = c.get("reset").get("entry") {
                runtime.set_entry(&entry).unwrap();
            }
            for table in array(case.get("state").get("tables")) {
                runtime
                    .import_collection(&string(table.get("name")), array(table.get("rows")))
                    .unwrap();
            }
            if c.get("reset").get("bindTally") == V::Bool(true) {
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: "SourceGlobal.bindTally".into(),
                        arguments: vec![],
                    })
                    .unwrap();
            }
            if c.get("reset").get("rpc") == V::Bool(true) {
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: "SourceGlobal.bindRpcTransactions".into(),
                        arguments: vec![],
                    })
                    .unwrap();
            }
        } else if !matches!(c.get("seed"), V::Undefined) {
            let seed = c.get("seed");
            runtime
                .import_collection(&string(seed.get("collection")), array(seed.get("rows")))
                .unwrap();
        } else {
            if !matches!(c.get("failure"), V::Undefined) {
                let f = c.get("failure");
                runtime
                    .fail_storage(StorageFailure {
                        collection: string(f.get("collection")),
                        operation: {
                            let op = string(f.get("op"));
                            if op == "loadDatabase" || op == "getDatabase" {
                                op
                            } else {
                                format!("{op}Async")
                            }
                        },
                        skip: if let V::Number(bits) = f.get("skip") {
                            f64::from_bits(bits) as usize
                        } else {
                            0
                        },
                        message: string(f.get("message")),
                    })
                    .unwrap();
            }
            let result = if !matches!(c.get("raw"), V::Undefined) {
                raw_calls += 1;
                source_observed(
                    runtime
                        .execute_contract_unverified(&array(c.get("raw")))
                        .unwrap_or_else(|e| panic!("{name}: {e:?}")),
                )
            } else if detailed_tokens
                && matches!(c.get("target"),V::String(t) if t=="Orderbook.processTokenMatches" || t=="Orderbook.processTokenChannelTrades")
            {
                calls += 1;
                let args = array(c.get("args"));
                let result = tradelayer_core::token_settlement_source::evaluate_token_settlement(
                    &args[1..],
                    c.get("target") == V::String("Orderbook.processTokenChannelTrades".into()),
                    &mut ActualBackend(&mut runtime),
                )
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
                assert_eq!(
                    result.arguments_after,
                    array(c.get("argumentsAfter"))[1..],
                    "{name} source match mutation"
                );
                source_observed(result.outcome)
            } else {
                calls += 1;
                observed(
                    runtime
                        .invoke_dependency(&DependencyCall {
                            target: string(c.get("target")),
                            arguments: array(c.get("args")),
                        })
                        .unwrap_or_else(|e| panic!("{name}: {e:?}")),
                )
            };
            assert_eq!(result, case.get("outcome"), "{name} actual outcome");
        }
        for table in array(case.get("state").get("tables")) {
            let collection = string(table.get("name"));
            assert_eq!(
                V::Array(runtime.collection(&collection).to_vec()),
                table.get("rows"),
                "{name} persisted {collection}"
            );
        }
        assert_eq!(
            runtime.tally_cache(),
            case.get("state").get("tally"),
            "{name} actual Tally cache"
        );
        let state = runtime.state_value().unwrap();
        let channels = state.get("globalClearing").get("channels");
        assert_eq!(
            channels.get("registry"),
            case.get("state").get("channels"),
            "{name} actual channel cache"
        );
        assert_eq!(
            channels.get("pendingWithdrawals"),
            case.get("state").get("withdrawals"),
            "{name} withdrawals"
        );
        assert_eq!(
            state.get("feeBuffer"),
            case.get("state").get("feeBuffer"),
            "{name} fee buffer"
        );
        assert_eq!(
            state.get("blockTrades"),
            case.get("state").get("blockTrades"),
            "{name} clearing trades"
        );
        assert_eq!(
            state.get("globalClearing").get("unhandledRejections"),
            case.get("state").get("unhandledRejections"),
            "{name} unawaited source rejections"
        );
        for (i, book) in array(case.get("state").get("books")).iter().enumerate() {
            assert_eq!(
                runtime.queue().instance_value(i as u64 + 1).unwrap(),
                *book,
                "{name} instance{}",
                i + 1
            );
        }
        let effects=runtime.take_effects().into_iter().filter(|v|v.get("collection")==V::String("orderBooks".into())||matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock","rpc"].contains(&k.as_str()))).collect::<Vec<_>>();
        let expected = array(case.get("effects"));
        for (i, (actual, wanted)) in effects.iter().zip(&expected).enumerate() {
            if actual != wanted {
                effect_errors.push(format!("{name} effect{i}: {actual:?} != {wanted:?}"));
                break;
            }
        }
        if effects.len() != expected.len() {
            effect_errors.push(format!(
                "{name} effect lengths {} != {}",
                effects.len(),
                expected.len()
            ));
        }
    }
    assert_eq!(calls, 2, "actual full-source invocation inventory");
    assert_eq!(raw_calls, 0);
    assert!(effect_errors.is_empty(), "{}", effect_errors.join("\n"));
}
#[test]
fn large_channel_queue_runs_actual_token_settlement_and_retains_late_source_failure() {
    compare_graph(false);
}
