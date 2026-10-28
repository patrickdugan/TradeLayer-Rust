use tradelayer_core::source_contract_replay::{SourceContractReplay, SourceContractReplayHead};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
use tradelayer_core::validity_source::{DependencyCall, SourceValue as V};
const FROZEN: &[u8] =
    include_bytes!("../fixtures/source_contract_replay_v11_before_production.bin");
#[test]
fn v11_before_production_retains_exact_bytes_and_all_new_route_refusals() {
    let hex = "ad83bba96f54b925a51127e092842bf9b0f85d9474fe88352ffab4f11422a24b";
    let mut commitment = [0; 32];
    for (i, v) in commitment.iter_mut().enumerate() {
        *v = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    assert_eq!(FROZEN.len(), 2271);
    assert_eq!(&FROZEN[..5], b"TLCR\x0b");
    let replay = SourceContractReplay::from_checkpoint(
        FROZEN,
        SourceContractReplayHead {
            commands: 3,
            commitment,
        },
    )
    .unwrap();
    assert_eq!(replay.runtime().policy(), P::AmmLifecycleV11);
    assert_eq!(replay.checkpoint().unwrap(), FROZEN);
    assert_eq!(replay.runtime().collection("orderBooks").len(), 1);
    assert_eq!(
        replay.runtime().collection("consensus")[0].get("value"),
        V::number(10.)
    );
    let mut runtime = SourceContractRuntime::with_policy(P::AmmLifecycleV11);
    for target in [
        "Main.blockHandlerEnd",
        "Main.getCurrentBlockHeight",
        "Main.checkBlockLag",
        "Main.initOrLoadTxIndex",
        "Main.syncIndex",
        "Main.checkSync",
        "Main.constructOrLoadConsensus",
        "Main.constructConsensusFromIndex",
        "SourceMain.configureProduction",
        "SourceMain.configureService",
        "Main.initialize",
        "Main.checkNetworkStatus",
        "Main.fetchWithRetry",
        "Main.blockHandlerBegin",
        "Main.processBlock",
        "Main.enterRecoveryMode",
        "Main.processIncomingBlocks",
        "Main.syncIfNecessary",
        "Main.handleReorg",
    ] {
        let before = runtime.clone();
        assert!(
            runtime
                .invoke_dependency(&DependencyCall {
                    target: target.into(),
                    arguments: vec![]
                })
                .is_err(),
            "{target} stays outside V11"
        );
        assert_eq!(runtime, before);
    }
    assert_eq!(SourceContractRuntime::new().policy(), P::GlobalClearingV2);
}

#[test]
fn v12_service_admission_does_not_expand_tlsw4_or_tlsw5_witness_domains() {
    use tradelayer_core::source_contract_replay::source_durable::{
        source_durable_transition_commands_commitment, SourceDurableContext,
        SourceDurableFilesystem, SourceDurableHistoryHead, SourceDurableTransitionError,
    };
    use tradelayer_core::source_contract_replay::{
        source_full_dispatch_commands_commitment, source_full_dispatch_context_commitment,
        SourceContractCommand, SourceFullDispatchError, SourceFullDispatchScope,
    };
    let replay = SourceContractReplay::with_policy(P::MainOrchestrationV12).unwrap();
    let commands = [SourceContractCommand::Invoke(DependencyCall {
        target: "Main.loadTrackHeight".into(),
        arguments: vec![],
    })];
    let context = SourceDurableContext {
        chain: "V12-WITNESS-BOUNDARY".into(),
        test: true,
        policy: P::MainOrchestrationV12,
        filesystem: SourceDurableFilesystem::FileSyncOnly,
    };
    // These unrelated endpoint bytes cannot confer admission to a new policy.
    let untrusted_head = SourceDurableHistoryHead {
        commands: 0,
        segments: 0,
        source_receipt_head: [0; 32],
        source_state: [0; 32],
        last_segment: [0; 32],
        commitment: [0; 32],
    };
    for scope in [
        SourceFullDispatchScope::SourceCommandSlice,
        SourceFullDispatchScope::MainCallableSlice,
    ] {
        assert_eq!(
            source_full_dispatch_context_commitment(&replay, scope),
            Err(SourceFullDispatchError::WrongPolicy)
        );
        assert_eq!(
            source_full_dispatch_commands_commitment(P::MainOrchestrationV12, scope, &commands),
            Err(SourceFullDispatchError::WrongPolicy)
        );
        assert!(matches!(
            source_durable_transition_commands_commitment(
                &context,
                scope,
                untrusted_head,
                "v12",
                &commands
            ),
            Err(SourceDurableTransitionError::Source(
                SourceFullDispatchError::WrongPolicy
            ))
        ));
    }
}
