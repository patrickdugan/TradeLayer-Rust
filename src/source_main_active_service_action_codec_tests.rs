use super::*;
#[test]
fn codec_all_actions_values_integer_boundaries_and_capacity() {
    let values = vec![
        V::Undefined,
        V::Null,
        V::Bool(false),
        V::number(-0.),
        V::number(f64::NAN),
        V::number(f64::INFINITY),
        V::number(f64::NEG_INFINITY),
        V::Promise,
        V::BigNumber("9007199254740993".into()),
        V::Object(vec![
            ("second".into(), V::Array(vec![V::number(-0.)])),
            ("first".into(), V::Undefined),
        ]),
    ];
    let mut actions = vec![
        NativeMainActiveServiceActionV1::Pause,
        NativeMainActiveServiceActionV1::AppendPending,
        NativeMainActiveServiceActionV1::ResolvePost,
        NativeMainActiveServiceActionV1::Advance { limit: 0 },
        NativeMainActiveServiceActionV1::Advance { limit: usize::MAX },
        NativeMainActiveServiceActionV1::FireTimer {
            id: 0,
            generation: u64::MAX,
        },
        NativeMainActiveServiceActionV1::ClearTimer {
            id: u64::MAX,
            generation: 0,
        },
    ];
    for value in values {
        actions.push(NativeMainActiveServiceActionV1::Start {
            start: value.clone(),
            generation: u64::MAX,
            documents: V::Array(vec![value.clone()]),
        });
        actions.push(NativeMainActiveServiceActionV1::Shutdown(vec![value]));
    }
    for action in actions {
        let raw = action.canonical_action_bytes().unwrap();
        assert_eq!(
            NativeMainActiveServiceActionV1::from_canonical_action_bytes(&raw).unwrap(),
            action
        );
        assert_eq!(
            NativeMainActiveServiceActionV1::from_canonical_action_bytes(&raw)
                .unwrap()
                .canonical_action_bytes()
                .unwrap(),
            raw
        );
        let mut bad = raw.clone();
        bad.push(0);
        assert!(NativeMainActiveServiceActionV1::from_canonical_action_bytes(&bad).is_err());
        let mut bad = raw.clone();
        bad[0] = 0;
        assert!(NativeMainActiveServiceActionV1::from_canonical_action_bytes(&bad).is_err());
        assert!(
            NativeMainActiveServiceActionV1::from_canonical_action_bytes(&raw[..raw.len() - 1])
                .is_err()
        );
    }
    let duplicate = V::Object(vec![("x".into(), V::Null), ("x".into(), V::Bool(true))]);
    assert!(NativeMainActiveServiceActionV1::Start {
        start: V::Null,
        generation: 1,
        documents: duplicate
    }
    .canonical_action_bytes()
    .is_err());
    let raw = NativeMainActiveServiceActionV1::Pause
        .canonical_action_bytes()
        .unwrap();
    let mut forged = raw.clone();
    forged.extend_from_slice(&raw[ACTIVE_SERVICE_ACTION_MAGIC.len()..]);
    assert!(NativeMainActiveServiceActionV1::from_canonical_action_bytes(&forged).is_err());
    let oversized = vec![0; MAX_ACTIVE_SERVICE_ACTION_BYTES + 1];
    assert_eq!(
        NativeMainActiveServiceActionV1::from_canonical_action_bytes(&oversized),
        Err(NativeMainActiveServiceErrorV1::Capacity)
    );
}

#[test]
fn decoder_rejects_forged_duplicate_object_fields_and_noncanonical_integer() {
    let invalid = NativeMainActiveServiceActionV1::Start {
        start: V::Null,
        generation: 1,
        documents: V::Object(vec![
            ("same".into(), V::Null),
            ("same".into(), V::Bool(true)),
        ]),
    };
    let mut fields = NativeFieldCollector::new(ACTIVE_SERVICE_ACTION_DOMAIN, true, 1024 * 1024);
    invalid.visit_fields(&mut |p, v| fields.emit(p, v)).unwrap();
    let mut raw = ACTIVE_SERVICE_ACTION_MAGIC.to_vec();
    raw.extend_from_slice(fields.frames.as_ref().unwrap());
    assert!(NativeMainActiveServiceActionV1::from_canonical_action_bytes(&raw).is_err());
    for (kind, path, value) in [
        ("Advance", "limit", "00"),
        ("Advance", "limit", "-1"),
        ("FireTimer", "id", "18446744073709551616"),
    ] {
        let mut fields = NativeFieldCollector::new(ACTIVE_SERVICE_ACTION_DOMAIN, true, 1024);
        fields.emit("kind", &s(kind)).unwrap();
        fields.emit(path, &s(value)).unwrap();
        if kind == "FireTimer" {
            fields.emit("generation", &s("1")).unwrap();
        }
        let mut raw = ACTIVE_SERVICE_ACTION_MAGIC.to_vec();
        raw.extend_from_slice(fields.frames.as_ref().unwrap());
        assert!(NativeMainActiveServiceActionV1::from_canonical_action_bytes(&raw).is_err());
    }
}
