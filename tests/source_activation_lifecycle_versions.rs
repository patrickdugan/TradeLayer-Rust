//! Actual validated V9 release capture, recorded before V10 source edits.
use tradelayer_core::source_contract_replay::{
    source_lifecycle_transition_commands_commitment as old_commitment,
    source_recent_lifecycle_transition_commands_commitment as recent_commitment,
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay as R,
    SourceContractReplayHead as H, SourceLifecycleTransitionError as E2,
    SourceLifecycleTransitionScope as S2, SourceRecentLifecycleTransitionError as E3,
    SourceRecentLifecycleTransitionScope as S3,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
use tradelayer_core::validity_source::{DependencyCall, SourceRuleError, SourceValue as V};
const V9: &[u8] = include_bytes!("../fixtures/source_contract_replay_v9.bin");
const LENGTH: usize = 213311;
fn pin() -> H {
    let hex = "e24ea3920f53c6ca574ba683c9adf6f0e8c57166dcc23e8a663ce6b66db07966";
    let mut commitment = [0; 32];
    for (i, b) in commitment.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap();
    }
    H {
        commands: 25,
        commitment,
    }
}
fn crc(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    !crc
}
fn repair(bytes: &mut [u8]) {
    let n = bytes.len();
    let check = crc(&bytes[..n - 4]);
    bytes[n - 4..].copy_from_slice(&check.to_le_bytes());
}
fn s(t: &str) -> V {
    V::String(t.into())
}
fn object(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
#[test]
fn actual_v9_release_replay_stays_exact_under_the_next_library() {
    assert_eq!(V9.len(), LENGTH);
    assert_eq!(&V9[..5], b"TLCR\x09");
    let replay = R::from_checkpoint(V9, pin()).unwrap();
    assert_eq!(replay.runtime().policy(), P::SpotLifecycleV9);
    assert_eq!(replay.head(), pin());
    assert_eq!(replay.checkpoint().unwrap(), V9);
    let state = replay.runtime().state_value().unwrap();
    assert_eq!(state.get("activationLifecycle"), V::Undefined);
    assert_eq!(
        state.get("spotLifecycle").get("pendingTokenThresholdReads"),
        V::Array(vec![])
    );
    assert_eq!(
        state.get("spotLifecycle").get("pendingWithdrawalSave"),
        V::Null
    );
    assert_eq!(SourceContractRuntime::new().policy(), P::GlobalClearingV2);
}
#[test]
fn v9_release_cannot_be_relabelled_or_promoted_to_a_prior_witness() {
    let mut wrong = pin();
    wrong.commitment[0] ^= 1;
    assert!(R::from_checkpoint(V9, wrong).is_err());
    for version in [8, 10] {
        let mut bytes = V9.to_vec();
        bytes[4] = version;
        repair(&mut bytes);
        assert!(R::from_checkpoint(&bytes, pin()).is_err());
    }
    let commands = [C::SetHeight(V::number(10.))];
    assert_eq!(
        old_commitment(P::SpotLifecycleV9, S2::RawTransactionSlice, &commands),
        Err(E2::WrongPolicy)
    );
    assert_eq!(
        recent_commitment(P::SpotLifecycleV9, S3::MainCallableSlice, &commands),
        Err(E3::WrongPolicy)
    );
}
#[test]
fn restored_v9_preserves_main0_and_new_bootstrap_capability_refusals_atomically() {
    let mut replay = R::from_checkpoint(V9, pin()).unwrap();
    let row = object(&[
        ("_id", s("tx-10-future-v10-main0")),
        (
            "value",
            object(&[
                ("txId", s("future-v10-main0")),
                ("marker", s("tl")),
                ("payload", s("00,0,0,0,0,0,0,0")),
                (
                    "sender",
                    object(&[
                        ("senderAddress", s("1BoatSLRHtKNngkdXEeobR76b53LETtpyT")),
                        ("amount", V::number(0.)),
                    ]),
                ),
                ("reference", V::Array(vec![])),
            ]),
        ),
    ]);
    let before = replay.runtime().clone();
    let receipt = replay
        .apply(C::Invoke(DependencyCall {
            target: "Main.processTxSet".into(),
            arguments: vec![V::Array(vec![row]), V::number(10.)],
        }))
        .unwrap();
    assert_eq!(
        receipt.outcome,
        O::LocalError(SourceRuleError::InvalidInput)
    );
    assert!(receipt.effects.is_empty());
    assert_eq!(replay.runtime(), &before);
    for target in [
        "SourceActivation.configureClient",
        "Activation.init",
        "Logic.activateTradeLayer",
        "Logic.AMMPool",
    ] {
        let before = replay.runtime().clone();
        let receipt = replay
            .apply(C::Invoke(DependencyCall {
                target: target.into(),
                arguments: vec![V::Null],
            }))
            .unwrap();
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::UnknownDependency),
            "{target}"
        );
        assert!(receipt.effects.is_empty());
        assert_eq!(replay.runtime(), &before);
    }
    let bytes = replay.checkpoint().unwrap();
    assert_eq!(bytes[4], 9);
    assert_eq!(R::from_checkpoint(&bytes, replay.head()).unwrap(), replay);
}
