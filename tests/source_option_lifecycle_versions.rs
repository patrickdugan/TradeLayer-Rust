//! V4 capability, diagnostic, replay-domain and continuation boundaries.
//! The V3 fixture pin comes from the separately compiled pre-V4 release.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    SourceContractReplayHead,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceException, SourceOutcome, SourceRuleError,
    SourceValue as V,
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
fn invoke(target: &str, arguments: Vec<V>) -> C {
    C::Invoke(DependencyCall {
        target: target.into(),
        arguments,
    })
}
fn raw(kind: f64) -> C {
    C::ExecuteContract(vec![
        s("new-policy-admission"),
        n(kind),
        s("tl"),
        s(""),
        s("1BoatSLRHtKNngkdXEeobR76b53LETtpyT"),
        V::Null,
        n(0.),
        n(0.),
        n(10.),
    ])
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
fn earlier_policies_keep_state_shapes_and_refuse_v4_source_routes() {
    let mut heads = vec![];
    for (policy, version, diagnostic) in [
        (P::OnchainV1, 1, V::Undefined),
        (P::GlobalClearingV2, 2, s("GlobalClearingV2")),
        (P::SourceLifecycleV3, 3, s("SourceLifecycleV3")),
    ] {
        let mut replay = SourceContractReplay::with_policy(policy).unwrap();
        let initial = replay.runtime().state_value().unwrap();
        assert_eq!(initial.get("runtimePolicy"), diagnostic);
        assert_eq!(initial.get("syntheticLifecycle"), V::Undefined);
        assert_eq!(
            matches!(initial.get("globalClearing"), V::Object(_)),
            policy != P::OnchainV1
        );
        assert_eq!(
            matches!(initial.get("registryLifecycle"), V::Object(_)),
            policy == P::SourceLifecycleV3
        );
        heads.push(replay.head());
        for target in [
            "Logic.processOptionTrade",
            "Validity.validateOptionTrade",
            "Margin.applyOptionTrade",
            "Margin._touchExpiryIndex",
            "Logic.mintSynthetic",
            "Logic.redeemSynthetic",
            "Synthetic.exists",
            "Vaults.initializeIfNeeded",
            "Vaults.loadFromDatabase",
            "Vaults.createVault",
            "Vaults.exists",
            "Vaults.getVault",
            "Vaults.getVaultId",
            "Vaults.registerSyntheticToken",
            "Vaults.saveVault",
            "Vaults.saveSyntheticToken",
            "Vaults.updateVault",
            "Vaults.updateVaultRedeem",
            "Property.getInstance",
            "Property.addProperty",
            "Property.updateTotalInCirculation",
            "Margin.moveMarginAndContractsForMint",
            "Margin.moveMarginAndContractsForRedeem",
        ] {
            let before = replay.runtime().clone();
            let receipt = replay.apply(invoke(target, vec![])).unwrap();
            assert!(
                matches!(
                    receipt.outcome,
                    O::LocalError(
                        SourceRuleError::UnknownDependency | SourceRuleError::UnknownMethod
                    )
                ),
                "{policy:?} {target}: {:?}",
                receipt.outcome
            );
            assert!(receipt.effects.is_empty(), "{policy:?} {target}");
            assert_eq!(replay.runtime(), &before, "{policy:?} {target}");
        }
        for kind in [24., 25., 27.] {
            let before = replay.runtime().clone();
            let receipt = replay.apply(raw(kind)).unwrap();
            assert_eq!(
                receipt.outcome,
                O::LocalError(SourceRuleError::InvalidInput)
            );
            assert!(receipt.effects.is_empty());
            assert_eq!(replay.runtime(), &before);
        }
        assert_eq!(replay.runtime().state_value().unwrap(), initial);
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(&bytes[..5], &[b'T', b'L', b'C', b'R', version]);
        assert_eq!(
            SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap(),
            replay
        );
    }
    let fourth = SourceContractReplay::with_policy(P::OptionLifecycleV4).unwrap();
    assert_eq!(&fourth.checkpoint().unwrap()[..5], b"TLCR\x04");
    let state = fourth.runtime().state_value().unwrap();
    assert_eq!(state.get("runtimePolicy"), s("OptionLifecycleV4"));
    assert_eq!(
        state.get("syntheticLifecycle"),
        obj(&[("vaults", V::Undefined), ("syntheticTokens", V::Undefined)])
    );
    heads.push(fourth.head());
    for (index, head) in heads.iter().enumerate() {
        assert!(!heads[..index].contains(head));
    }
    assert_eq!(SourceContractRuntime::new().policy(), P::GlobalClearingV2);
    let default = SourceContractReplay::new().unwrap();
    assert_eq!(default.runtime().policy(), P::GlobalClearingV2);
    assert_eq!(&default.checkpoint().unwrap()[..5], b"TLCR\x02");
}

#[test]
fn corrected_crc_and_replaced_bootstrap_cannot_promote_or_downgrade_v3_v4() {
    let mut old = SourceContractReplay::with_policy(P::SourceLifecycleV3).unwrap();
    old.apply(C::SetHeight(n(10.))).unwrap();
    old.apply(invoke("Contract.getNextId", vec![])).unwrap();
    let mut newer = SourceContractReplay::with_policy(P::OptionLifecycleV4).unwrap();
    newer.apply(C::SetHeight(n(10.))).unwrap();
    newer
        .apply(invoke("Vaults.createVault", vec![n(7.), n(1.)]))
        .unwrap();
    let old_bytes = old.checkpoint().unwrap();
    let new_bytes = newer.checkpoint().unwrap();
    for (bytes, source, target, promoted_version, original_head) in [
        (
            old_bytes,
            P::SourceLifecycleV3,
            P::OptionLifecycleV4,
            4,
            old.head(),
        ),
        (
            new_bytes,
            P::OptionLifecycleV4,
            P::SourceLifecycleV3,
            3,
            newer.head(),
        ),
    ] {
        let mut edited = bytes.clone();
        edited[4] = promoted_version;
        repair_crc(&mut edited);
        assert!(SourceContractReplay::from_checkpoint(&edited, original_head).is_err());
        let destination = SourceContractReplay::with_policy(target).unwrap();
        edited[5..37].copy_from_slice(&destination.checkpoint().unwrap()[5..37]);
        repair_crc(&mut edited);
        assert!(SourceContractReplay::from_checkpoint(&edited, original_head).is_err());
        assert!(SourceContractReplay::from_checkpoint(&edited, destination.head()).is_err());
        assert_eq!(
            SourceContractReplay::from_checkpoint(&bytes, original_head)
                .unwrap()
                .runtime()
                .policy(),
            source
        );
    }
}

#[test]
fn v4_actual_vault_and_margin_owners_restore_and_continue_under_distinct_domain() {
    let mut replay = SourceContractReplay::with_policy(P::OptionLifecycleV4).unwrap();
    for command in [
        C::SetHeight(n(10.)),
        invoke("Vaults.createVault", vec![n(7.), n(1.)]),
        invoke("Vaults.registerSyntheticToken", vec![n(9.), n(1.), n(7.)]),
        invoke("Margin.getInstance", vec![n(1.)]),
    ] {
        let receipt = replay.apply(command).unwrap();
        assert!(!matches!(receipt.outcome, O::LocalError(_)));
    }
    let handle = obj(&[("sourceMarginInstance", n(1.))]);
    let apply_option = |quantity| {
        invoke(
            "Margin.applyOptionTrade",
            vec![
                handle.clone(),
                s("alice"),
                s("1-100-C-100"),
                n(quantity),
                n(10.),
                n(10.),
                n(5.),
            ],
        )
    };
    let receipt = replay.apply(apply_option(4.)).unwrap();
    assert!(matches!(
        receipt.outcome,
        O::Dependency(DependencyReturn::Pending(_))
    ));
    replay
        .apply(invoke("Margin.saveMarginMap", vec![handle.clone(), n(10.)]))
        .unwrap();
    let state = replay.runtime().state_value().unwrap();
    let V::Array(vaults) = state.get("syntheticLifecycle").get("vaults") else {
        panic!("actual initialized Vault Map")
    };
    assert_eq!(vaults.len(), 1);
    assert_eq!(
        state.get("syntheticLifecycle").get("syntheticTokens"),
        V::Array(vec![V::Array(vec![
            n(9.),
            obj(&[("contract", n(1.)), ("property", n(7.))]),
        ])])
    );
    assert_eq!(replay.runtime().collection("marginMaps").len(), 1);
    assert_eq!(replay.runtime().collection("marginMapDelta").len(), 1);
    // createVault's omitted save argument is retained in the durable document;
    // the actual in-memory Vault Map still contains its full initialized blob.
    assert_eq!(
        replay.runtime().collection("vaults"),
        &[obj(&[("_id", s("s-7-1")), ("value", V::Undefined)])]
    );
    let bytes = replay.checkpoint().unwrap();
    assert_eq!(&bytes[..5], b"TLCR\x04");
    let mut restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
    assert_eq!(restored, replay);
    assert_eq!(restored.checkpoint().unwrap(), bytes);
    for command in [
        apply_option(-1.),
        invoke("Margin.saveMarginMap", vec![handle, n(11.)]),
        invoke(
            "Vaults.updateVault",
            vec![
                s("s-7-1"),
                obj(&[("contracts", n(2.)), ("margin", n(3.))]),
                n(4.),
                n(5.),
            ],
        ),
    ] {
        let next = replay.apply(command.clone()).unwrap();
        assert_eq!(restored.apply(command).unwrap(), next);
        assert!(!matches!(next.outcome, O::LocalError(_)));
    }
    assert_eq!(restored, replay);
    assert_eq!(restored.checkpoint().unwrap(), replay.checkpoint().unwrap());
    assert_eq!(replay.runtime().collection("marginMapDelta").len(), 2);
}

#[test]
fn separately_compiled_v3_fixture_keeps_exact_receipts_state_and_future_refusals() {
    // Captured by the parent from the actual pre-V4 release library and its
    // independently retained generator output, never read from this envelope.
    // Fixture SHA256: 3cf903cdfea2e3bc03e68558e6a2c13026479d2dd133cbcab2ac55f8be4a582b.
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/source_contract_replay_v3.bin"
    ));
    let hex = "65e94c85b2a693163ad9638cae62f49c1175b8a6596f43df32cc7bd4e856e3fa";
    let mut commitment = [0; 32];
    for (index, byte) in commitment.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * index..2 * index + 2], 16).unwrap();
    }
    let pin = SourceContractReplayHead {
        commands: 10,
        commitment,
    };
    let mut replay = SourceContractReplay::from_checkpoint(bytes, pin).unwrap();
    assert_eq!(replay.runtime().policy(), P::SourceLifecycleV3);
    assert_eq!(replay.checkpoint().unwrap(), bytes);
    let receipts: Vec<_> = replay.receipts().collect();
    assert_eq!(receipts.len(), 10);
    assert_eq!(
        receipts
            .iter()
            .map(|v| v.outcome.clone())
            .collect::<Vec<_>>(),
        vec![
            O::Dependency(DependencyReturn::Immediate(obj(&[(
                "sourceContractRegistry",
                V::Bool(true),
            )]))),
            O::Dependency(DependencyReturn::Pending(V::Array(vec![]))),
            O::Dependency(DependencyReturn::Pending(V::Null)),
            O::Dependency(DependencyReturn::PendingThrow(SourceException {
                name: "TypeError".into(),
                message:
                    "Cannot destructure property 'txidNeutralized1' of 'txParams' as it is null."
                        .into(),
            })),
            O::Dependency(DependencyReturn::Pending(n(0.001))),
            O::LocalError(SourceRuleError::UnknownDependency),
            O::LocalError(SourceRuleError::UnknownDependency),
            O::LocalError(SourceRuleError::UnknownMethod),
            O::LocalError(SourceRuleError::UnknownDependency),
            O::Source(SourceOutcome::Returned(V::Undefined)),
        ]
    );
    assert_eq!(
        receipts.iter().map(|v| v.effects.len()).collect::<Vec<_>>(),
        vec![1, 3, 3, 1, 3, 0, 0, 0, 0, 2]
    );
    let state = replay.runtime().state_value().unwrap();
    assert_eq!(state.get("runtimePolicy"), s("SourceLifecycleV3"));
    assert_eq!(state.get("syntheticLifecycle"), V::Undefined);
    assert_eq!(
        state.get("registryLifecycle"),
        obj(&[
            ("registryInitialized", V::Bool(true)),
            ("seriesInitialized", V::Bool(true)),
        ])
    );
    for target in [
        "Logic.processOptionTrade",
        "Logic.mintSynthetic",
        "Logic.redeemSynthetic",
        "Logic.exerciseDerivative",
        "Logic.deliverContract",
        "Logic.handleOptionExercise",
        "Vaults.createVault",
        "Property.getInstance",
    ] {
        let before = replay.runtime().clone();
        let receipt = replay.apply(invoke(target, vec![])).unwrap();
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::UnknownDependency)
        );
        assert!(receipt.effects.is_empty());
        assert_eq!(replay.runtime(), &before);
    }
    for kind in [24., 25., 27.] {
        let before = replay.runtime().clone();
        let receipt = replay.apply(raw(kind)).unwrap();
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::InvalidInput)
        );
        assert!(receipt.effects.is_empty());
        assert_eq!(replay.runtime(), &before);
    }
    let continued = replay.checkpoint().unwrap();
    assert_eq!(&continued[..5], b"TLCR\x03");
    assert_eq!(
        SourceContractReplay::from_checkpoint(&continued, replay.head()).unwrap(),
        replay
    );
    let mut promoted = bytes.to_vec();
    promoted[4] = 4;
    repair_crc(&mut promoted);
    assert!(SourceContractReplay::from_checkpoint(&promoted, pin).is_err());
    let fourth = SourceContractReplay::with_policy(P::OptionLifecycleV4).unwrap();
    promoted[5..37].copy_from_slice(&fourth.checkpoint().unwrap()[5..37]);
    repair_crc(&mut promoted);
    assert!(SourceContractReplay::from_checkpoint(&promoted, pin).is_err());
}
