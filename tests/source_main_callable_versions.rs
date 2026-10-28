//! Actual V7 release capture made before the first V8 source edit.
use tradelayer_core::source_contract_replay::{
    source_lifecycle_transition_commands_commitment, SourceContractCommand as C,
    SourceContractCommandOutcome as O, SourceContractReplay as R, SourceContractReplayHead,
    SourceLifecycleTransitionError as E, SourceLifecycleTransitionScope as S,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceRuleError, SourceValue as V,
};
const V7: &[u8] = include_bytes!("../fixtures/source_contract_replay_v7.bin");
fn pin() -> SourceContractReplayHead {
    let hex = "e5337955bfaeca0102ed77f8b5b7f8ced805e03df7d02f77e0fc075eefffc66d";
    let mut commitment = [0; 32];
    for (i, b) in commitment.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    SourceContractReplayHead {
        commands: 37,
        commitment,
    }
}
fn queue(amount: f64) -> C {
    C::Invoke(DependencyCall {
        target: "Logic.queueLiquidityReward".into(),
        arguments: vec![
            V::String("alice".into()),
            V::number(amount),
            V::number(10.),
            V::String("continuation".into()),
        ],
    })
}
fn repair_crc(bytes: &mut Vec<u8>) {
    bytes.truncate(bytes.len() - 4);
    let mut crc = !0u32;
    for b in bytes.iter() {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes.extend_from_slice(&(!crc).to_be_bytes());
}
#[test]
fn independent_v7_release_preserves_populated_state_and_future_refusals() {
    assert_eq!(V7.len(), 70_918);
    let mut original = R::from_checkpoint(V7, pin()).unwrap();
    assert_eq!(original.runtime().policy(), P::LowerLifecycleV7);
    assert_eq!(original.head(), pin());
    assert_eq!(original.checkpoint().unwrap(), V7);
    let receipts: Vec<_> = original.receipts().collect();
    assert_eq!(
        receipts[14].outcome,
        O::Dependency(DependencyReturn::Pending(V::String(
            "Token V7FIX (ID: 1) created. Type: Managed".into()
        )))
    );
    for r in &receipts[19..28] {
        assert_eq!(r.outcome, O::LocalError(SourceRuleError::UnknownDependency));
        assert!(r.effects.is_empty());
    }
    for r in &receipts[28..] {
        assert_eq!(r.outcome, O::LocalError(SourceRuleError::InvalidInput));
        assert!(r.effects.is_empty());
    }
    let state = original.runtime().state_value().unwrap();
    assert_eq!(state.get("mainCallable"), V::Undefined);
    assert_eq!(state.get("clearlistLifecycle"), V::Undefined);
    let mut restored = R::from_checkpoint(V7, pin()).unwrap();
    assert_eq!(
        original.apply(queue(0.75)).unwrap(),
        restored.apply(queue(0.75)).unwrap()
    );
    assert_eq!(
        original.checkpoint().unwrap(),
        restored.checkpoint().unwrap()
    );
    assert_eq!(
        original.runtime().state_value().unwrap(),
        restored.runtime().state_value().unwrap()
    );
}
#[test]
fn repaired_checksums_do_not_promote_v7_or_downgrade_v8_or_imply_witness_export() {
    let mut promoted = V7.to_vec();
    promoted[4] = 8;
    repair_crc(&mut promoted);
    assert!(R::from_checkpoint(&promoted, pin()).is_err());
    let mut fresh = R::with_policy(P::MainCallableV8).unwrap();
    fresh.apply(queue(1.)).unwrap();
    let bytes = fresh.checkpoint().unwrap();
    assert_eq!(&bytes[..5], b"TLCR\x08");
    assert_eq!(
        R::from_checkpoint(&bytes, fresh.head())
            .unwrap()
            .checkpoint()
            .unwrap(),
        bytes
    );
    let mut downgraded = bytes;
    downgraded[4] = 7;
    repair_crc(&mut downgraded);
    assert!(R::from_checkpoint(&downgraded, fresh.head()).is_err());
    assert_eq!(SourceContractRuntime::new().policy(), P::GlobalClearingV2);
    assert_eq!(
        source_lifecycle_transition_commands_commitment(
            P::MainCallableV8,
            S::RawTransactionSlice,
            &[C::SetHeight(V::number(10.))]
        ),
        Err(E::WrongPolicy)
    );
}
