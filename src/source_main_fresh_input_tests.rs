use super::*;
fn object(fields: Vec<(&str, V)>) -> V {
    V::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
#[test]
fn raw_tip_block_documents_roundtrip_without_collapsing_undefined_or_negative_zero() {
    let io = object(vec![
        ("height", V::number(-0.)),
        (
            "blocks",
            V::Array(vec![object(vec![
                ("height", V::String("12".into())),
                ("hash", V::String("raw12".into())),
                ("block", object(vec![("tx", V::Array(vec![]))])),
            ])]),
        ),
    ]);
    let raw = NativeMainFreshRpcInputV1::new(
        io,
        Some(V::Array(vec![object(vec![
            ("id", V::String("raw-id".into())),
            ("raw", V::Undefined),
            ("document", V::Undefined),
        ])])),
    )
    .unwrap();
    let bytes = raw.canonical_input_bytes().unwrap();
    assert_eq!(
        NativeMainFreshRpcInputV1::from_canonical_input_bytes(&bytes).unwrap(),
        raw
    );
    assert_eq!(raw.io().get("height"), V::number(-0.));
    assert_eq!(raw.documents().unwrap().get("unrelated"), V::Undefined);
}
#[test]
fn feed_cannot_accept_source_financial_or_native_verdict_fields() {
    for field in [
        "processed",
        "valid",
        "params",
        "tables",
        "fileInputs",
        "counts",
        "client",
        "nativeRows",
        "returned",
    ] {
        assert!(
            NativeMainFreshRpcInputV1::new(object(vec![(field, V::Bool(true))]), None).is_err(),
            "{field}"
        );
        assert!(
            NativeMainFreshRpcInputV1::new(
                object(vec![]),
                Some(V::Array(vec![object(vec![
                    ("id", V::String("raw".into())),
                    (field, V::Bool(true))
                ])]))
            )
            .is_err(),
            "document envelope {field}"
        );
    }
}
#[test]
fn absent_documents_and_explicit_empty_replacement_are_distinct() {
    let keep = NativeMainFreshRpcInputV1::new(object(vec![]), None).unwrap();
    let clear = NativeMainFreshRpcInputV1::new(object(vec![]), Some(V::Array(vec![]))).unwrap();
    assert_ne!(
        keep.canonical_input_bytes().unwrap(),
        clear.canonical_input_bytes().unwrap()
    );
    assert!(keep.documents().is_none());
    assert_eq!(clear.documents(), Some(&V::Array(vec![])));
}
#[test]
fn malformed_envelopes_and_unbounded_rows_refuse_before_admission() {
    for raw in [V::Undefined, V::Array(vec![]), V::Bool(true)] {
        assert!(NativeMainFreshRpcInputV1::new(raw, None).is_err());
    }
    assert!(NativeMainFreshRpcInputV1::new(object(vec![("blocks", V::Bool(true))]), None).is_err());
    assert!(NativeMainFreshRpcInputV1::new(
        object(vec![]),
        Some(V::Array(vec![object(vec![("id", V::number(1.))])]))
    )
    .is_err());
    assert!(NativeMainFreshRpcInputV1::new(
        object(vec![]),
        Some(V::Array(vec![object(vec![
            ("id", V::String("x".into())),
            ("raw", V::number(1.))
        ])]))
    )
    .is_err());
    assert!(NativeMainFreshRpcInputV1::new(
        object(vec![(
            "blocks",
            V::Array(vec![V::Undefined; MAX_FRESH_RPC_INPUT_ROWS + 1])
        )]),
        None
    )
    .is_err());
}
#[test]
fn wire_requires_exact_magic_length_fields_and_canonical_bytes() {
    let raw =
        NativeMainFreshRpcInputV1::new(object(vec![("height", V::number(12.))]), None).unwrap();
    let bytes = raw.canonical_input_bytes().unwrap();
    for count in 0..bytes.len() {
        assert!(NativeMainFreshRpcInputV1::from_canonical_input_bytes(&bytes[..count]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(NativeMainFreshRpcInputV1::from_canonical_input_bytes(&trailing).is_err());
    let mut foreign = bytes;
    foreign[0] = b'X';
    assert!(NativeMainFreshRpcInputV1::from_canonical_input_bytes(&foreign).is_err());
}
#[test]
fn old_source_action_wire_cannot_admit_new_factual_feed_or_vice_versa() {
    use crate::source_main_native_session::NativeMainActiveServiceActionV1 as OldAction;
    let raw =
        NativeMainFreshRpcInputV1::new(object(vec![("height", V::number(12.))]), None).unwrap();
    assert!(OldAction::from_canonical_action_bytes(&raw.canonical_input_bytes().unwrap()).is_err());
    let old = OldAction::Advance { limit: 1 }
        .canonical_action_bytes()
        .unwrap();
    assert!(NativeMainFreshRpcInputV1::from_canonical_input_bytes(&old).is_err());
}
