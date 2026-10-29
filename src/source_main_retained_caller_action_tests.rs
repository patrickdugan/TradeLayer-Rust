use super::*;
fn selector() -> V {
    V::Object(vec![
        ("kind".into(), V::String("CallerInline".into())),
        ("role".into(), V::String("Detached".into())),
        ("sourceJob".into(), V::String("3".into())),
        ("context".into(), V::Undefined),
    ])
}
#[test]
fn raw_actions_round_trip_all_explicit_source_and_native_selections() {
    let mut actions = vec![
        RetainedMainCallerActionV1::AdvanceCaller,
        RetainedMainCallerActionV1::EnterConsensus,
        RetainedMainCallerActionV1::AdvanceConsensus { limit: 4096 },
        RetainedMainCallerActionV1::AdvanceDetached {
            selector: selector(),
        },
        RetainedMainCallerActionV1::AppendSelected {
            selector: selector(),
        },
        RetainedMainCallerActionV1::ResolveSelectedPost {
            selector: selector(),
        },
        RetainedMainCallerActionV1::Pause,
    ];
    for lane in [
        RetainedMainCallerTimerLaneV1::CallerDelay,
        RetainedMainCallerTimerLaneV1::ConsensusService,
    ] {
        actions.push(RetainedMainCallerActionV1::FireTimer {
            lane,
            id: 1,
            generation: u64::MAX,
        });
    }
    for caller in [
        RetainedMainCallerStartV1::Initialize,
        RetainedMainCallerStartV1::InitOrLoad,
        RetainedMainCallerStartV1::SyncIndex {
            max: V::number(10.),
        },
    ] {
        actions.push(RetainedMainCallerActionV1::Start {
            caller,
            scope: 501,
            generation: 71,
            documents: V::Array(vec![]),
        });
    }
    for action in actions {
        let bytes = action.canonical_action_bytes().unwrap();
        assert_eq!(
            RetainedMainCallerActionV1::from_canonical_action_bytes(&bytes).unwrap(),
            action
        );
        assert_eq!(
            RetainedMainCallerActionV1::from_canonical_action_bytes(&bytes)
                .unwrap()
                .canonical_action_bytes()
                .unwrap(),
            bytes
        );
    }
}
#[test]
fn raw_value_encoding_preserves_bits_undefined_and_original_object_order() {
    let input = V::Object(vec![
        ("z".into(), V::Number(0x8000000000000000)),
        ("a".into(), V::Number(0x7ff8000000000042)),
        (
            "values".into(),
            V::Array(vec![
                V::Undefined,
                V::Null,
                V::Bool(false),
                V::Bool(true),
                V::Number(0x7ff0000000000000),
                V::String("actual 🚀 source input".into()),
                V::Promise,
                V::BigNumber("1.2300".into()),
            ]),
        ),
    ]);
    let action = RetainedMainCallerActionV1::AdvanceDetached { selector: input };
    let raw = action.canonical_action_bytes().unwrap();
    assert_eq!(
        RetainedMainCallerActionV1::from_canonical_action_bytes(&raw).unwrap(),
        action
    );
}
#[test]
fn every_truncated_input_and_trailing_byte_refuses_without_partial_action() {
    let action = RetainedMainCallerActionV1::Start {
        caller: RetainedMainCallerStartV1::SyncIndex {
            max: V::number(10.),
        },
        scope: 501,
        generation: 71,
        documents: V::Array(vec![selector()]),
    };
    let raw = action.canonical_action_bytes().unwrap();
    for n in 0..raw.len() {
        assert!(
            RetainedMainCallerActionV1::from_canonical_action_bytes(&raw[..n]).is_err(),
            "prefix {n}"
        );
    }
    let mut extra = raw;
    extra.push(0);
    assert_eq!(
        RetainedMainCallerActionV1::from_canonical_action_bytes(&extra),
        Err(RetainedMainCallerActionErrorV1::InvalidEncoding)
    );
}
#[test]
fn old_action_magic_unknown_kind_timer_lane_and_noncanonical_boolean_refuse() {
    assert!(RetainedMainCallerActionV1::from_canonical_action_bytes(b"TLMASA\x01\x01").is_err());
    for body in [
        vec![255],
        vec![7, 255],
        vec![4, 2, 2],
        vec![4, 255],
        vec![0, 255],
    ] {
        let mut raw = MAGIC.to_vec();
        raw.extend_from_slice(&body);
        assert_eq!(
            RetainedMainCallerActionV1::from_canonical_action_bytes(&raw),
            Err(RetainedMainCallerActionErrorV1::InvalidEncoding)
        );
    }
}
#[test]
fn declared_counts_depth_and_size_are_bounded_before_allocating_claimed_payloads() {
    let mut raw = MAGIC.to_vec();
    raw.extend_from_slice(&[4, 5]);
    raw.extend_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        RetainedMainCallerActionV1::from_canonical_action_bytes(&raw),
        Err(RetainedMainCallerActionErrorV1::Capacity)
    );
    let raw = vec![0; MAX_BYTES + 1];
    assert_eq!(
        RetainedMainCallerActionV1::from_canonical_action_bytes(&raw),
        Err(RetainedMainCallerActionErrorV1::Capacity)
    );
    let mut raw = MAGIC.to_vec();
    raw.push(4);
    for _ in 0..66 {
        raw.push(5);
        raw.extend_from_slice(&1u32.to_be_bytes());
    }
    raw.push(0);
    assert_eq!(
        RetainedMainCallerActionV1::from_canonical_action_bytes(&raw),
        Err(RetainedMainCallerActionErrorV1::Capacity)
    );
}
#[test]
fn foreground_detached_append_resolve_and_timer_lanes_have_distinct_raw_bytes() {
    let actions = [
        RetainedMainCallerActionV1::AdvanceDetached {
            selector: selector(),
        },
        RetainedMainCallerActionV1::AppendSelected {
            selector: selector(),
        },
        RetainedMainCallerActionV1::ResolveSelectedPost {
            selector: selector(),
        },
        RetainedMainCallerActionV1::FireTimer {
            lane: RetainedMainCallerTimerLaneV1::CallerDelay,
            id: 1,
            generation: 71,
        },
        RetainedMainCallerActionV1::FireTimer {
            lane: RetainedMainCallerTimerLaneV1::ConsensusService,
            id: 1,
            generation: 71,
        },
    ];
    let bytes = actions
        .iter()
        .map(|a| a.canonical_action_bytes().unwrap())
        .collect::<Vec<_>>();
    for (a, left) in bytes.iter().enumerate() {
        for right in bytes.iter().skip(a + 1) {
            assert_ne!(left, right);
        }
    }
}
