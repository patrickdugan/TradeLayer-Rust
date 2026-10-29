// Child of actual enhanced WAL controller tests; raw fixture setup only.
use super::*;
#[test]
fn actual_enhanced_wal_history_writer_bytes_match_strict_metadata_pin_codec() {
    let temp = Temp::new();
    let (mut owner, _docs) = setup(&temp, "detached-live-rpc-failure");
    let actual = owner.index().capture_verified_history_pin().unwrap();
    let writer = SourceMainNativeDurableActiveServiceV1::history_pin_bytes(owner.index()).unwrap();
    assert_eq!(actual.canonical_scalar_bytes().unwrap(), writer);
    let decoded =
        crate::source_compact_index_v2::SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
            owner.index().native_namespace(),
            &writer,
        )
        .unwrap();
    assert_eq!(decoded, actual);
    assert_eq!(
        owner
            .index
            .read_verified_history_generation(&decoded)
            .unwrap()
            .pin(),
        &actual.metadata
    );
}
