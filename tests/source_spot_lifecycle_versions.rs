//! Actual populated V8 release capture made before the first V9 edit.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay as R,
    SourceContractReplayHead,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
use tradelayer_core::validity_source::{DependencyCall, SourceRuleError, SourceValue as V};
const V8: &[u8] = include_bytes!("../fixtures/source_contract_replay_v8.bin");
fn pin() -> SourceContractReplayHead {
    let hex = "4a6ea6d8e313a985fadba718039d4d0ef417af801da2a6bd917952c374d5c21c";
    let mut commitment = [0; 32];
    for (i, byte) in commitment.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    SourceContractReplayHead {
        commands: 27,
        commitment,
    }
}
#[test]
fn actual_v8_release_stays_exact_with_v9_and_new_witness_code() {
    assert_eq!(V8.len(), 424_544);
    let replay = R::from_checkpoint(V8, pin()).unwrap();
    assert_eq!(replay.runtime().policy(), P::MainCallableV8);
    assert_eq!(replay.head(), pin());
    assert_eq!(replay.checkpoint().unwrap(), V8);
    assert_eq!(SourceContractRuntime::new().policy(), P::GlobalClearingV2);
    let wrong = SourceContractReplayHead {
        commands: 27,
        commitment: [0; 32],
    };
    assert!(R::from_checkpoint(V8, wrong).is_err());
}
#[test]
fn restored_v8_keeps_spot_and_future_activation_amm_refusals() {
    let mut replay = R::from_checkpoint(V8, pin()).unwrap();
    for kind in [0., 5., 6., 10.] {
        let receipt = replay
            .apply(C::ExecuteContract(vec![
                V::String("future-v8".into()),
                V::number(kind),
                V::String("tl".into()),
                V::String(String::new()),
                V::String("1BoatSLRHtKNngkdXEeobR76b53LETtpyT".into()),
                V::Null,
                V::number(0.),
                V::number(0.),
                V::number(10.),
            ]))
            .unwrap();
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::InvalidInput)
        );
    }
    for target in [
        "Logic.onChainTokenToToken",
        "Logic.cancelOrder",
        "Logic.AMMPool",
    ] {
        let receipt = replay
            .apply(C::Invoke(DependencyCall {
                target: target.into(),
                arguments: vec![],
            }))
            .unwrap();
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::UnknownDependency)
        );
    }
    let bytes = replay.checkpoint().unwrap();
    assert_eq!(
        R::from_checkpoint(&bytes, replay.head())
            .unwrap()
            .checkpoint()
            .unwrap(),
        bytes
    );
}
