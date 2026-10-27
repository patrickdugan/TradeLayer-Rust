use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    SourceContractReplayHead,
};
use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy as P;
use tradelayer_core::validity_source::{DependencyCall, SourceRuleError, SourceValue as V};
#[test]
fn original_compiled_v1_journal_retains_its_method_graph_and_continuation() {
    let commitment = "b637ad06da4ccd4d88dfaaf31a6cb898589fc1cb5c5c78355f8c57f54eba011c";
    let mut head = [0; 32];
    for (i, byte) in head.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&commitment[i * 2..i * 2 + 2], 16).unwrap();
    }
    let bytes = include_bytes!("../fixtures/source_contract_replay_v1.bin");
    let expected = SourceContractReplayHead {
        commands: 5,
        commitment: head,
    };
    let mut replay = SourceContractReplay::from_checkpoint(bytes, expected).unwrap();
    assert_eq!(replay.runtime().policy(), P::OnchainV1);
    assert_eq!(replay.checkpoint().unwrap(), bytes);
    let receipt = replay
        .apply(C::Invoke(DependencyCall {
            target: "Clearing.applyFundingRates".into(),
            arguments: vec![V::number(24.0)],
        }))
        .unwrap();
    assert_eq!(
        receipt.outcome,
        O::LocalError(SourceRuleError::UnknownDependency)
    );
    assert!(receipt.effects.is_empty());
    let next = replay.checkpoint().unwrap();
    assert_eq!(&next[..5], b"TLCR\x01");
    assert_eq!(
        SourceContractReplay::from_checkpoint(&next, replay.head()).unwrap(),
        replay
    );
    // Unknown handle metadata was ignored by the original V1 engine. New
    // object-reference capabilities must not reinterpret those old commands.
    let mut plain =
        tradelayer_core::source_contract_runtime::SourceContractRuntime::with_policy(P::OnchainV1);
    let initialized = plain
        .invoke_dependency(&DependencyCall {
            target: "Margin.getInstance".into(),
            arguments: vec![V::number(1.0)],
        })
        .unwrap();
    let tradelayer_core::validity_source::DependencyReturn::Pending(handle) = initialized else {
        panic!("V1 handle")
    };
    let mut decorated = plain.clone();
    let mut extra = handle.clone();
    extra
        .insert(
            "sourcePositionObjectAliases",
            V::Array(vec![V::Object(vec![
                ("argument".into(), V::number(0.0)),
                ("object".into(), V::number(999.0)),
            ])]),
        )
        .unwrap();
    let call = |h| DependencyCall {
        target: "Margin.calculateMarginRequirement".into(),
        arguments: vec![h, V::number(1.0), V::number(100.0), V::Bool(false)],
    };
    assert_eq!(
        decorated.invoke_dependency(&call(extra)),
        plain.invoke_dependency(&call(handle))
    );
    assert_eq!(
        decorated.state_value().unwrap(),
        plain.state_value().unwrap()
    );
    // A repaired CRC cannot promote an old journal into a different engine.
    let mut substituted = bytes.to_vec();
    substituted[4] = 2;
    substituted.truncate(substituted.len() - 4);
    let mut crc = !0u32;
    for b in &substituted {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    substituted.extend_from_slice(&(!crc).to_be_bytes());
    assert!(SourceContractReplay::from_checkpoint(&substituted, expected).is_err());
    let newer = SourceContractReplay::new().unwrap();
    assert_eq!(newer.runtime().policy(), P::GlobalClearingV2);
    assert_eq!(&newer.checkpoint().unwrap()[..5], b"TLCR\x02");
    assert_ne!(
        newer.head(),
        SourceContractReplay::with_policy(P::OnchainV1)
            .unwrap()
            .head()
    );
}
