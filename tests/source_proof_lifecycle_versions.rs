//! Frozen pre-V6 source ownership and independent V5 replay compatibility.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    SourceContractReplayHead,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
};
fn s(value: &str) -> V {
    V::String(value.into())
}
fn n(value: f64) -> V {
    V::number(value)
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn array(value: V) -> Vec<V> {
    let V::Array(value) = value else {
        panic!("array")
    };
    value
}
#[test]
fn earlier_policies_refuse_new_type30_graph_without_reinterpreting_old_receipts() {
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        let before = runtime.clone();
        assert_eq!(
            runtime.state_value().unwrap().get("proofLifecycle"),
            V::Undefined
        );
        for target in [
            "SourceEnvironment.configure",
            "SourceArtifacts.configure",
            "Logic.processStakeFraudProof",
            "Logic.applyProceduralOracleSettlement",
            "Oracle.recordStake",
            "Oracle.getStake",
            "Oracle.applyFraudProof",
            "Oracle.relayTradeLayerState",
            "Dlc.parseRelayBlob",
            "Dlc.validateRelayBundle",
        ] {
            assert_eq!(
                runtime.invoke_dependency(&DependencyCall {
                    target: target.into(),
                    arguments: vec![]
                }),
                Err(SourceRuleError::UnknownDependency)
            );
            assert_eq!(runtime, before);
        }
        assert_eq!(
            runtime.execute_contract_unverified(&[
                s("legacy"),
                n(30.),
                s("tl"),
                s("0,1,7,1"),
                s("A"),
                V::Null,
                n(0.),
                n(0.),
                n(10.)
            ]),
            Err(SourceRuleError::InvalidInput)
        );
        assert_eq!(runtime, before);
    }
    assert_eq!(SourceContractRuntime::new().policy(), P::GlobalClearingV2);
}
fn repair_crc(bytes: &mut Vec<u8>) {
    bytes.truncate(bytes.len() - 4);
    let mut crc = !0u32;
    for byte in bytes.iter() {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes.extend_from_slice(&(!crc).to_be_bytes());
}
#[test]
fn independently_compiled_v5_fixture_preserves_real_state_and_future_refusals() {
    // Actual Rust1.90 MSVC V5 release capture, independent of this V6 build.
    let bytes = include_bytes!("../fixtures/source_contract_replay_v5.bin");
    assert_eq!(bytes.len(), 18_336);
    let hex = "3ca16cb2adb98a43b79500982923415e50137cf86b6ec7d9168c4998b9c607ce";
    let mut commitment = [0; 32];
    for (i, b) in commitment.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    let pin = SourceContractReplayHead {
        commands: 19,
        commitment,
    };
    let mut old = SourceContractReplay::from_checkpoint(bytes, pin).unwrap();
    assert_eq!(old.checkpoint().unwrap(), bytes);
    assert_eq!(old.runtime().policy(), P::DistributionLifecycleV5);
    let error = |name: &str, message: &str| tradelayer_core::validity_source::SourceException {
        name: name.into(),
        message: message.into(),
    };
    let handle = obj(&[("sourceMarginInstance", n(1.))]);
    let receipts: Vec<_> = old.receipts().collect();
    assert_eq!(
        receipts
            .iter()
            .map(|v| v.outcome.clone())
            .collect::<Vec<_>>(),
        vec![
            O::Void,
            O::Dependency(DependencyReturn::Pending(s("s-7-1"))),
            O::Dependency(DependencyReturn::Pending(V::Undefined)),
            O::Dependency(DependencyReturn::Pending(handle.clone())),
            O::Dependency(DependencyReturn::Pending(obj(&[
                ("contracts", n(4.)),
                ("avgPrice", n(10.)),
                ("margin", n(5.)),
                ("closedQty", n(0.)),
                ("flipQty", n(0.))
            ]))),
            O::Dependency(DependencyReturn::Immediate(V::Undefined)),
            O::Dependency(DependencyReturn::Pending(V::Undefined)),
            O::Dependency(DependencyReturn::Pending(V::Array(vec![s("1-100-C-100")]))),
            O::Dependency(DependencyReturn::PendingThrow(error(
                "ReferenceError",
                "tallyMap is not defined"
            ))),
            O::Dependency(DependencyReturn::PendingThrow(error(
                "TypeError",
                "Cannot read properties of undefined (reading 'hasChannel')"
            ))),
            O::Dependency(DependencyReturn::Throw(error(
                "TypeError",
                "Cannot read properties of undefined (reading 'get')"
            ))),
            O::Source(SourceOutcome::Thrown(error(
                "TypeError",
                "tallyMap.hasSufficientBalance is not a function"
            ))),
            O::Source(SourceOutcome::Thrown(error(
                "TypeError",
                "Decode.decodeTradeBaiUrbun is not a function"
            ))),
            O::Source(SourceOutcome::Thrown(error(
                "TypeError",
                "Decode.decodeTradeMurabaha is not a function"
            ))),
            O::LocalError(SourceRuleError::UnknownDependency),
            O::LocalError(SourceRuleError::UnknownDependency),
            O::LocalError(SourceRuleError::InvalidInput),
            O::LocalError(SourceRuleError::InvalidInput),
            O::LocalError(SourceRuleError::InvalidInput),
        ]
    );
    assert_eq!(
        receipts.iter().map(|v| v.effects.len()).collect::<Vec<_>>(),
        vec![0, 7, 3, 3, 8, 1, 3, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    let state = old.runtime().state_value().unwrap();
    assert_eq!(state.get("runtimePolicy"), s("DistributionLifecycleV5"));
    assert_eq!(
        array(state.get("syntheticLifecycle").get("vaults")).len(),
        1
    );
    assert_eq!(
        state.get("syntheticLifecycle").get("syntheticTokens"),
        V::Array(vec![V::Array(vec![
            n(9.),
            obj(&[("contract", n(1.)), ("property", n(7.))])
        ])])
    );
    let position = old
        .runtime()
        .margins()
        .inspect_position(1, &s("alice"))
        .unwrap();
    assert_eq!(
        position.get("options").get("1-100-C-100").get("contracts"),
        n(4.)
    );
    assert_eq!(
        position.get("options").get("1-100-C-100").get("margin"),
        n(5.)
    );
    let instance = old.runtime().margins().instance_value(1).unwrap();
    assert_eq!(
        instance.get("expiryIndex"),
        V::Array(vec![V::Array(vec![
            n(100.),
            V::Array(vec![s("1-100-C-100")])
        ])])
    );
    assert_eq!(
        instance.get("optionOI"),
        V::Array(vec![V::Array(vec![s("1-100-C-100"), n(1.)])])
    );
    assert_eq!(old.runtime().collection("marginMaps").len(), 1);
    assert_eq!(old.runtime().collection("marginMapDelta").len(), 1);
    for target in ["Logic.processStakeFraudProof", "Logic.batchMoveZkRollup"] {
        let before = old.runtime().clone();
        let receipt = old
            .apply(C::Invoke(DependencyCall {
                target: target.into(),
                arguments: vec![],
            }))
            .unwrap();
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::UnknownDependency)
        );
        assert!(receipt.effects.is_empty());
        assert_eq!(old.runtime(), &before);
    }
    for kind in [30., 32., 33.] {
        let before = old.runtime().clone();
        let receipt = old
            .apply(C::ExecuteContract(vec![
                s("continue-old"),
                n(kind),
                s("tl"),
                s(""),
                s("A"),
                V::Null,
                n(0.),
                n(0.),
                n(10.),
            ]))
            .unwrap();
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::InvalidInput)
        );
        assert!(receipt.effects.is_empty());
        assert_eq!(old.runtime(), &before);
    }
    let mut resumed =
        SourceContractReplay::from_checkpoint(&old.checkpoint().unwrap(), old.head()).unwrap();
    let continuation = C::Invoke(DependencyCall {
        target: "Margin.getExpiringTickersUpTo".into(),
        arguments: vec![handle, n(100.)],
    });
    assert_eq!(
        old.apply(continuation.clone()).unwrap(),
        resumed.apply(continuation).unwrap()
    );
    assert_eq!(old, resumed);
    let mut sixth = SourceContractReplay::with_policy(P::ProofLifecycleV6).unwrap();
    sixth
        .apply(C::Invoke(DependencyCall {
            target: "Oracle.getStake".into(),
            arguments: vec![n(1.), s("missing")],
        }))
        .unwrap();
    assert_eq!(
        sixth.runtime().state_value().unwrap().get("runtimePolicy"),
        s("ProofLifecycleV6")
    );
    for (original, source, target, version, head) in [
        (
            bytes.to_vec(),
            P::DistributionLifecycleV5,
            P::ProofLifecycleV6,
            6,
            pin,
        ),
        (
            sixth.checkpoint().unwrap(),
            P::ProofLifecycleV6,
            P::DistributionLifecycleV5,
            5,
            sixth.head(),
        ),
    ] {
        let mut edited = original.clone();
        edited[4] = version;
        repair_crc(&mut edited);
        assert!(SourceContractReplay::from_checkpoint(&edited, head).is_err());
        let destination = SourceContractReplay::with_policy(target).unwrap();
        edited[5..37].copy_from_slice(&destination.checkpoint().unwrap()[5..37]);
        repair_crc(&mut edited);
        assert!(SourceContractReplay::from_checkpoint(&edited, head).is_err());
        assert!(SourceContractReplay::from_checkpoint(&edited, destination.head()).is_err());
        assert_eq!(
            SourceContractReplay::from_checkpoint(&original, head)
                .unwrap()
                .runtime()
                .policy(),
            source
        );
    }
}

#[test]
fn v6_db_ranges_use_source_string_comparison_and_keep_earlier_numeric_policy() {
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
        P::ProofLifecycleV6,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        runtime
            .import_collection(
                "ranges",
                vec![
                    obj(&[("_id", s("string9")), ("block", s("9"))]),
                    obj(&[("_id", s("number7")), ("block", n(7.))]),
                    obj(&[("_id", s("number12")), ("block", n(12.))]),
                    obj(&[("_id", s("string10")), ("block", s("10"))]),
                    obj(&[("_id", s("array2")), ("block", V::Array(vec![s("2")]))]),
                    obj(&[("_id", s("object")), ("block", obj(&[]))]),
                ],
            )
            .unwrap();
        let handle = match runtime
            .invoke_dependency(&DependencyCall {
                target: "db.getDatabase".into(),
                arguments: vec![s("ranges")],
            })
            .unwrap()
        {
            DependencyReturn::Pending(handle) | DependencyReturn::Immediate(handle) => handle,
            other => panic!("factory {other:?}"),
        };
        for (op, old, v6) in [
            ("$lt", vec!["string9", "number7", "array2"], vec!["number7"]),
            (
                "$lte",
                vec!["string9", "number7", "string10", "array2"],
                vec!["number7", "string10"],
            ),
            (
                "$gt",
                vec!["number12"],
                vec!["string9", "number12", "array2", "object"],
            ),
            (
                "$gte",
                vec!["number12", "string10"],
                vec!["string9", "number12", "string10", "array2", "object"],
            ),
        ] {
            for bound in [
                s("10"),
                V::BigNumber("10".into()),
                V::BigNumber("10.0".into()),
                V::Array(vec![V::BigNumber("10.0".into())]),
                n(f64::NAN),
            ] {
                let result = runtime
                    .invoke_dependency(&DependencyCall {
                        target: "db.findAsync".into(),
                        arguments: vec![
                            handle.clone(),
                            obj(&[("block", obj(&[(op, bound.clone())]))]),
                        ],
                    })
                    .unwrap();
                let rows = match result {
                    DependencyReturn::Pending(V::Array(rows))
                    | DependencyReturn::Immediate(V::Array(rows)) => rows,
                    other => panic!("query {other:?}"),
                };
                let ids: Vec<_> = rows.into_iter().map(|row| row.get("_id")).collect();
                let wanted = if bound == n(f64::NAN) {
                    vec![]
                } else if policy == P::ProofLifecycleV6 {
                    v6.iter().map(|x| s(x)).collect()
                } else {
                    old.iter().map(|x| s(x)).collect()
                };
                assert_eq!(ids, wanted, "{policy:?} {op} {bound:?}");
            }
        }
    }
}
