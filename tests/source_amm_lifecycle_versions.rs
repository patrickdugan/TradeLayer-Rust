//! Actual populated V10 focused-debug capture, saved before V11 existed.
use tradelayer_core::source_contract_replay::{
    source_lifecycle_transition_commands_commitment as old_commitment,
    source_recent_lifecycle_transition_commands_commitment as recent_commitment,
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay as R,
    SourceContractReplayHead as H, SourceLifecycleTransitionError as E2,
    SourceLifecycleTransitionScope as S2, SourceRecentLifecycleTransitionError as E3,
    SourceRecentLifecycleTransitionScope as S3,
};
use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy as P;
use tradelayer_core::validity_source::{DependencyCall, SourceRuleError, SourceValue as V};
const V10: &[u8] = include_bytes!("../fixtures/source_contract_replay_v10_debug.bin");
fn hash(text: &str) -> [u8; 32] {
    let mut value = [0; 32];
    for (i, byte) in value.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap();
    }
    value
}
fn pin() -> H {
    H {
        commands: 15,
        commitment: hash("38bbe0a230b3cd29e37d6617f0ca7d7ebae99e21ef2004a4237713b9a419c83a"),
    }
}
fn s(text: &str) -> V {
    V::String(text.into())
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
fn actual_populated_v10_debug_history_keeps_exact_bytes_and_owners() {
    assert_eq!(V10.len(), 94665);
    assert_eq!(&V10[..5], b"TLCR\x0a");
    let replay = R::from_checkpoint(V10, pin()).unwrap();
    assert_eq!(replay.runtime().policy(), P::ActivationLifecycleV10);
    assert!(matches!(replay.runtime().tally_cache(), V::Array(ref rows) if !rows.is_empty()));
    let state = replay.runtime().state_value().unwrap();
    assert_ne!(state.get("activationLifecycle"), V::Undefined);
    assert_eq!(state.get("ammLifecycle"), V::Undefined);
    assert_eq!(replay.checkpoint().unwrap(), V10);
}
#[test]
fn repaired_checksums_cannot_relabel_v10_or_supply_a_prior_witness() {
    let mut wrong = pin();
    wrong.commitment[0] ^= 1;
    assert!(R::from_checkpoint(V10, wrong).is_err());
    for version in [9, 11] {
        let mut bytes = V10.to_vec();
        bytes[4] = version;
        let n = bytes.len();
        let mut crc = 0xffff_ffff_u32;
        for byte in &bytes[..n - 4] {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
            }
        }
        bytes[n - 4..].copy_from_slice(&(!crc).to_le_bytes());
        assert!(R::from_checkpoint(&bytes, pin()).is_err());
    }
    let commands = [C::SetHeight(V::number(10.))];
    assert_eq!(
        old_commitment(
            P::ActivationLifecycleV10,
            S2::RawTransactionSlice,
            &commands
        ),
        Err(E2::WrongPolicy)
    );
    assert_eq!(
        recent_commitment(P::ActivationLifecycleV10, S3::MainCallableSlice, &commands),
        Err(E3::WrongPolicy)
    );
}
#[test]
fn restored_v10_still_refuses_new_amm_entry_points_atomically() {
    let mut replay = R::from_checkpoint(V10, pin()).unwrap();
    let row = object(&[
        ("_id", s("tx-10-future-amm")),
        (
            "value",
            object(&[
                ("txId", s("future-amm")),
                ("marker", s("tl")),
                ("payload", s("a")),
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
    let commands = [
        (
            C::ExecuteContract(vec![
                s("future-amm"),
                V::number(10.),
                s("tl"),
                s(""),
                s("1BoatSLRHtKNngkdXEeobR76b53LETtpyT"),
                V::Null,
                V::number(0.),
                V::number(0.),
                V::number(10.),
            ]),
            SourceRuleError::InvalidInput,
        ),
        (
            C::Invoke(DependencyCall {
                target: "Main.processTxSet".into(),
                arguments: vec![V::Array(vec![row]), V::number(10.)],
            }),
            SourceRuleError::InvalidInput,
        ),
        (
            C::Invoke(DependencyCall {
                target: "Logic.AMMPool".into(),
                arguments: vec![],
            }),
            SourceRuleError::UnknownDependency,
        ),
    ];
    for (command, error) in commands {
        let before = replay.runtime().clone();
        let receipt = replay.apply(command).unwrap();
        assert_eq!(receipt.outcome, O::LocalError(error));
        assert!(receipt.effects.is_empty());
        assert_eq!(replay.runtime(), &before);
    }
    let bytes = replay.checkpoint().unwrap();
    assert_eq!(bytes[4], 10);
    assert_eq!(R::from_checkpoint(&bytes, replay.head()).unwrap(), replay);
}
