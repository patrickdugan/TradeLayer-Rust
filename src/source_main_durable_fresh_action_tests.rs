use super::*;
use crate::validity_source::SourceValue as V;
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
#[test]
fn exact_nested_feed_and_source_action_codecs_roundtrip_every_variant_and_special_values() {
    let feed = NativeMainFreshRpcInputV1::new(
        obj(&[
            ("height", V::number(f64::NAN)),
            ("blocks", V::Array(vec![])),
            (
                "networkInfo",
                obj(&[
                    ("same", V::Undefined),
                    ("sameSecond", V::number(-0.)),
                    ("infinity", V::number(f64::INFINITY)),
                ]),
            ),
        ]),
        Some(V::Array(vec![obj(&[
            ("id", V::String("raw".into())),
            (
                "document",
                obj(&[("ordered", V::Bool(false)), ("orderedSecond", V::Null)]),
            ),
        ])])),
    )
    .unwrap();
    let source = [
        NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: u64::MAX,
            documents: V::Array(vec![]),
        },
        NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        NativeMainActiveServiceActionV1::AppendPending,
        NativeMainActiveServiceActionV1::ResolvePost,
        NativeMainActiveServiceActionV1::Pause,
        NativeMainActiveServiceActionV1::Shutdown(vec![
            V::Undefined,
            V::number(-0.),
            V::number(f64::NEG_INFINITY),
        ]),
        NativeMainActiveServiceActionV1::FireTimer {
            id: u64::MAX,
            generation: u64::MAX,
        },
        NativeMainActiveServiceActionV1::ClearTimer {
            id: u64::MAX,
            generation: 0,
        },
    ];
    for value in std::iter::once(NativeMainDurableFreshActionV1::Feed(feed)).chain(
        source
            .into_iter()
            .map(NativeMainDurableFreshActionV1::SourceAction),
    ) {
        let raw = value.canonical_action_bytes().unwrap();
        assert_eq!(
            NativeMainDurableFreshActionV1::from_canonical_action_bytes(&raw).unwrap(),
            value
        );
        assert!(crate::source_main_native_session::NativeMainActiveServiceActionV1::from_canonical_action_bytes(&raw).is_err());
        assert!(
            crate::source_main_fresh_input::NativeMainFreshRpcInputV1::from_canonical_input_bytes(
                &raw
            )
            .is_err()
        );
    }
}
#[test]
fn exact_outer_length_tags_and_trailing_data_are_rejected() {
    let good = NativeMainDurableFreshActionV1::SourceAction(NativeMainActiveServiceActionV1::Pause)
        .canonical_action_bytes()
        .unwrap();
    for length in 0..good.len() {
        assert!(
            NativeMainDurableFreshActionV1::from_canonical_action_bytes(&good[..length]).is_err()
        );
    }
    let mut bad = good.clone();
    bad[MAGIC.len()] = 2;
    assert!(NativeMainDurableFreshActionV1::from_canonical_action_bytes(&bad).is_err());
    let mut bad = good.clone();
    bad.push(0);
    assert!(NativeMainDurableFreshActionV1::from_canonical_action_bytes(&bad).is_err());
    let mut bad = good.clone();
    bad[MAGIC.len() + 1..MAGIC.len() + 9].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(NativeMainDurableFreshActionV1::from_canonical_action_bytes(&bad).is_err());
    let mut bad = good.clone();
    bad[0] ^= 1;
    assert!(NativeMainDurableFreshActionV1::from_canonical_action_bytes(&bad).is_err());
}
#[test]
fn oversized_outer_input_rejects_before_shared_decode_and_preserves_original_input() {
    let raw = vec![0; MAX_DURABLE_FRESH_ACTION_BYTES + 1];
    let before = raw.clone();
    assert_eq!(
        NativeMainDurableFreshActionV1::from_canonical_action_bytes(&raw),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(raw, before);
}

#[test]
fn duplicate_object_keys_remain_explicitly_outside_existing_nested_value_admission() {
    let duplicate = obj(&[("duplicate", V::number(1.)), ("duplicate", V::number(2.))]);
    assert_eq!(
        NativeMainFreshRpcInputV1::new(obj(&[("networkInfo", duplicate.clone())]), None),
        Err(SourceRuleError::InvalidInput)
    );
    let source = NativeMainDurableFreshActionV1::SourceAction(
        NativeMainActiveServiceActionV1::Shutdown(vec![duplicate]),
    );
    assert_eq!(
        source.canonical_action_bytes(),
        Err(SourceRuleError::InvalidInput)
    );
}
