use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "history-pin-codec-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let path = self.0.canonicalize().unwrap();
        assert_eq!(
            path.parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        let _ = std::fs::remove_dir_all(path);
    }
}
fn ns() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn raw_row() -> V {
    V::Object(vec![
        ("_id".into(), V::String("tx-1-raw".into())),
        (
            "value".into(),
            V::Object(vec![("payload".into(), V::String("0,raw".into()))]),
        ),
    ])
}
#[test]
fn history_pin_codec_roundtrips_actual_empty_row_and_compacted_physical_logical_fields() {
    let temp = Temp::new();
    let mut index =
        SourceCompactIndex::create(&temp.0, ns(), "raw origin with Unicode Ω and quote\"").unwrap();
    for ordinal in 0..3 {
        if ordinal == 1 {
            index
                .import_row(&index.snapshot().unwrap(), raw_row())
                .unwrap();
        }
        if ordinal == 2 {
            index.compact_metadata(&index.snapshot().unwrap()).unwrap();
        }
        let pin = index.capture_verified_history_pin().unwrap();
        let wire = pin.canonical_scalar_bytes().unwrap();
        let decoded = SourceIndexHistoryPinV1::from_canonical_scalar_bytes(&ns(), &wire).unwrap();
        assert_eq!(decoded, pin);
        assert_eq!(decoded.canonical_scalar_bytes().unwrap(), wire);
        let actual = index.read_verified_history_generation(&decoded).unwrap();
        assert_eq!(actual.pin(), &pin.metadata);
        if ordinal == 2 {
            assert!(pin.metadata.publication.parent.is_some());
            assert_ne!(
                pin.metadata.publication.physical_creation_sequence,
                pin.metadata.publication.creation_sequence
            );
        }
    }
}
#[test]
fn history_pin_codec_decoding_receipt_is_not_disk_or_source_admission() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(&temp.0, ns(), "actual raw origin").unwrap();
    index
        .import_row(&index.snapshot().unwrap(), raw_row())
        .unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let mut wrong = before.clone();
    wrong.storage.content.chain_sha256d[0] ^= 1;
    let decoded = SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
        &ns(),
        &wrong.canonical_scalar_bytes().unwrap(),
    )
    .unwrap();
    assert!(index.read_verified_history_generation(&decoded).is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    wrong = before.clone();
    wrong.rows.chain_sha256d[0] ^= 1;
    let decoded = SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
        &ns(),
        &wrong.canonical_scalar_bytes().unwrap(),
    )
    .unwrap();
    assert!(index.read_verified_history_generation(&decoded).is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
}
#[test]
fn history_pin_codec_unknown_profiles_foreign_namespace_incomplete_or_misaligned_receipts_refuse() {
    let temp = Temp::new();
    let index = SourceCompactIndex::create(&temp.0, ns(), "actual raw origin").unwrap();
    let pin = index.capture_verified_history_pin().unwrap();
    let wire = pin.canonical_scalar_bytes().unwrap();
    assert!(SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
        &Namespace::new("BTC", false, "txIndex").unwrap(),
        &wire
    )
    .is_err());
    for case in 0..5 {
        let mut wrong = pin.clone();
        match case {
            0 => wrong.profile.push('x'),
            1 => wrong.storage.profile.push('x'),
            2 => wrong.storage.content.complete = false,
            3 => wrong.rows.complete = false,
            _ => wrong.rows.snapshot.sequence += 1,
        };
        assert!(SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
            &ns(),
            &wrong.canonical_scalar_bytes().unwrap()
        )
        .is_err());
    }
}
#[test]
fn history_pin_codec_wrong_fields_truncation_noncanonical_integers_and_budget_refuse() {
    let temp = Temp::new();
    let index = SourceCompactIndex::create(&temp.0, ns(), "actual raw origin").unwrap();
    let wire = index
        .capture_verified_history_pin()
        .unwrap()
        .canonical_scalar_bytes()
        .unwrap();
    assert!(
        SourceIndexHistoryPinV1::from_canonical_scalar_bytes(&ns(), &wire[..wire.len() - 1])
            .is_err()
    );
    let mut extra = wire.clone();
    extra.extend_from_slice(&[0]);
    assert!(SourceIndexHistoryPinV1::from_canonical_scalar_bytes(&ns(), &extra).is_err());
    let mut fields = vec![];
    let pin = index.capture_verified_history_pin().unwrap();
    pin.visit_canonical_fields(&mut |path, value| {
        fields.push((path.to_owned(), value.clone()));
        Ok(())
    })
    .unwrap();
    for case in 0..3 {
        let mut fields = fields.clone();
        match case {
            0 => fields.swap(0, 1),
            1 => fields.push(("params/valid".into(), V::Bool(true))),
            _ => {
                fields
                    .iter_mut()
                    .find(|(path, _)| path == "storage/logicalSequence")
                    .unwrap()
                    .1 = V::String("01".into())
            }
        }
        let mut raw = vec![];
        for (path, value) in fields {
            let body = value.diagnostic_json().unwrap();
            raw.extend_from_slice(&(path.len() as u32).to_be_bytes());
            raw.extend_from_slice(path.as_bytes());
            raw.extend_from_slice(&(body.len() as u32).to_be_bytes());
            raw.extend_from_slice(body.as_bytes());
        }
        assert!(SourceIndexHistoryPinV1::from_canonical_scalar_bytes(&ns(), &raw).is_err());
    }
    assert!(matches!(
        SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
            &ns(),
            &vec![0; MAX_HISTORY_PIN_BYTES + 1]
        ),
        Err(CompactIndexError::Limit)
    ));
}
