use super::*;
#[test]
fn actual_descriptor_roundtrips_without_files_and_never_promotes_a_permit() {
    let temp = Temp::new();
    let mut journal = store(&temp);
    put(&mut journal, "a", b"one");
    let point = journal.verified_prefix_pin().unwrap();
    let permit = journal
        .retain_verified_generation(
            &temp.escrow,
            std::slice::from_ref(&point),
            GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let pin = permit.independently_retained_pin().clone();
    let raw = pin.canonical_descriptor_bytes().unwrap();
    let decoded =
        GenerationEscrowPinV1::from_canonical_descriptor_bytes(&ns(), &raw, pin.record_sha256d)
            .unwrap();
    assert_eq!(decoded, pin);
    drop(permit);
    drop(journal);
    let moved = pin.copied_generation.with_extension("temporarily-moved");
    fs::rename(&pin.copied_generation, &moved).unwrap();
    assert_eq!(
        GenerationEscrowPinV1::from_canonical_descriptor_bytes(&ns(), &raw, pin.record_sha256d)
            .unwrap(),
        pin,
        "decoding an expectation performs no filesystem lookup"
    );
    assert!(
        decoded.open_historical(&point).is_err(),
        "expected bytes are not actual verified copy evidence or a writer permit"
    );
    fs::rename(&moved, &pin.copied_generation).unwrap();
    assert_eq!(
        decoded.open_historical(&point).unwrap().get("a"),
        Some(b"one".as_slice())
    );
}
#[test]
fn descriptor_rejects_trailing_truncated_oversized_namespace_and_lexical_aliases() {
    let temp = Temp::new();
    let mut journal = store(&temp);
    put(&mut journal, "a", b"one");
    let point = journal.verified_prefix_pin().unwrap();
    let permit = journal
        .retain_verified_generation(
            &temp.escrow,
            &[point],
            GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let pin = permit.independently_retained_pin();
    let raw = pin.canonical_descriptor_bytes().unwrap();
    for n in 0..raw.len() {
        let broken = &raw[..n];
        assert!(GenerationEscrowPinV1::from_canonical_descriptor_bytes(
            &ns(),
            broken,
            double_sha256(broken)
        )
        .is_err());
    }
    let mut extra = raw.clone();
    extra.push(0);
    assert!(GenerationEscrowPinV1::from_canonical_descriptor_bytes(
        &ns(),
        &extra,
        double_sha256(&extra)
    )
    .is_err());
    let oversized = vec![0; MAX_EXPECTED_ESCROW_DESCRIPTOR_BYTES + 1];
    assert!(
        GenerationEscrowPinV1::from_canonical_descriptor_bytes(&ns(), &oversized, [0; 32]).is_err()
    );
    assert!(GenerationEscrowPinV1::from_canonical_descriptor_bytes(
        &Namespace::new("BTC", false, "txIndex").unwrap(),
        &raw,
        pin.record_sha256d
    )
    .is_err());
    let mut alias = pin.clone();
    let mut path = alias.copied_root.as_os_str().to_owned();
    path.push(std::path::MAIN_SEPARATOR_STR);
    path.push(".");
    alias.copied_root = PathBuf::from(path);
    let alias_raw = escrow_record(&alias).unwrap();
    assert!(GenerationEscrowPinV1::from_canonical_descriptor_bytes(
        &ns(),
        &alias_raw,
        double_sha256(&alias_raw)
    )
    .is_err());
}
