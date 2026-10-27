//! A populated V6 release capture made before the first V7 source edit.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    SourceContractReplayHead,
};
use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy as P;
use tradelayer_core::validity_source::{DependencyCall, SourceRuleError, SourceValue as V};
const V6: &[u8] = include_bytes!("../fixtures/source_contract_replay_v6.bin");
fn pin() -> SourceContractReplayHead {
    let hex = "e8b616851f86a69ba178b363ef2d2d3d81b98b96303769ecc88a2a890a928b0c";
    let mut commitment = [0; 32];
    for (i, byte) in commitment.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    SourceContractReplayHead {
        commands: 27,
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
    for byte in bytes.iter() {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes.extend_from_slice(&(!crc).to_be_bytes());
}
#[test]
fn actual_v6_release_bytes_state_and_future_refusals_survive_v7_build() {
    assert_eq!(V6.len(), 34_379);
    let mut replay = SourceContractReplay::from_checkpoint(V6, pin()).unwrap();
    assert_eq!(replay.runtime().policy(), P::ProofLifecycleV6);
    assert_eq!(replay.head(), pin());
    assert_eq!(replay.checkpoint().unwrap(), V6);
    let state = replay.runtime().state_value().unwrap();
    assert_eq!(
        state.get("proofLifecycle").get("propertyInitialized"),
        V::Bool(true)
    );
    assert_eq!(
        state
            .get("proofLifecycle")
            .get("environment")
            .get("TL_BITVM_CHALLENGE_BLOCKS"),
        V::String("6".into())
    );
    assert_eq!(
        state.get("proofLifecycle").get("artifacts"),
        V::Object(vec![])
    );
    assert!(matches!(state.get("liquidityRewards").get("queue"), V::Array(rows) if rows.len()==1));
    let receipts: Vec<_> = replay.receipts().collect();
    assert_eq!(
        receipts[2].outcome,
        O::LocalError(SourceRuleError::InvalidInput)
    );
    for receipt in &receipts[14..20] {
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::UnknownDependency)
        );
        assert!(receipt.effects.is_empty());
    }
    for receipt in &receipts[20..] {
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::InvalidInput)
        );
        assert!(receipt.effects.is_empty());
    }
    let mut restored = SourceContractReplay::from_checkpoint(V6, pin()).unwrap();
    assert_eq!(
        replay.apply(queue(0.75)).unwrap(),
        restored.apply(queue(0.75)).unwrap()
    );
    assert_eq!(replay.head(), restored.head());
    assert_eq!(replay.checkpoint().unwrap(), restored.checkpoint().unwrap());
    assert_eq!(
        replay.runtime().state_value().unwrap(),
        restored.runtime().state_value().unwrap()
    );
}
#[test]
fn repaired_checksums_cannot_promote_v6_or_downgrade_v7_receipts() {
    let mut promoted = V6.to_vec();
    promoted[4] = 7;
    repair_crc(&mut promoted);
    assert!(SourceContractReplay::from_checkpoint(&promoted, pin()).is_err());
    let mut v7 = SourceContractReplay::with_policy(P::LowerLifecycleV7).unwrap();
    v7.apply(queue(1.)).unwrap();
    let bytes = v7.checkpoint().unwrap();
    assert_eq!(&bytes[..5], b"TLCR\x07");
    assert_eq!(
        SourceContractReplay::from_checkpoint(&bytes, v7.head())
            .unwrap()
            .checkpoint()
            .unwrap(),
        bytes
    );
    let mut downgraded = bytes;
    downgraded[4] = 6;
    repair_crc(&mut downgraded);
    assert!(SourceContractReplay::from_checkpoint(&downgraded, v7.head()).is_err());
    assert_eq!(
        tradelayer_core::source_contract_runtime::SourceContractRuntime::new().policy(),
        P::GlobalClearingV2
    );
}
