use tradelayer_core::bitvm_bundle::*;
use tradelayer_core::issuance_intent::{JsonNumber, JsonObject, JsonValue};
fn unhex(raw: &str) -> String {
    if raw == "-" {
        return String::new();
    }
    String::from_utf8(
        raw.as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(core::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}
fn string<'a>(o: &'a JsonObject, k: &str) -> &'a str {
    match o.get(k).unwrap() {
        JsonValue::String(s) => s,
        _ => panic!("string"),
    }
}
fn object(v: &JsonValue) -> &JsonObject {
    match v {
        JsonValue::Object(o) => o,
        _ => panic!("object"),
    }
}
#[test]
fn complete_source_hash_and_artifact_vectors() {
    let mut count = 0;
    for line in include_str!("../fixtures/bitvm_bundle_vectors.tsv")
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let f: Vec<_> = line.split('\t').collect();
        count += 1;
        let input = JsonValue::parse(&unhex(f[2])).unwrap();
        let expected = JsonValue::parse(&unhex(f[5])).unwrap();
        match f[0] {
            "hash" => assert_eq!(
                JsonValue::String(compute_bundle_hash(&input).unwrap()),
                expected,
                "{}",
                f[1]
            ),
            "verify" => {
                let o = object(&input);
                let result = verify_bundle_bytes(
                    string(o, "expected"),
                    string(o, "label"),
                    string(o, "raw").as_bytes(),
                );
                let e = object(&expected);
                if e.get("valid") == Some(&JsonValue::Bool(false)) {
                    assert_eq!(
                        result.unwrap_err(),
                        BitvmError::Source(string(e, "reason").into()),
                        "{}",
                        f[1]
                    );
                } else {
                    let result = result.unwrap();
                    assert_eq!(result.bundle_hash, string(e, "bundleHash"));
                    assert_eq!(result.bundle_path, string(e, "bundlePath"));
                    assert_eq!(&result.source_hashes, e.get("sourceHashes").unwrap());
                    if f[1] == "selected-nan" {
                        assert!(result.selected_bucket_pct.is_nan());
                    } else {
                        let JsonValue::Number(n) = e.get("selectedBucketPct").unwrap() else {
                            panic!("number")
                        };
                        assert_eq!(result.selected_bucket_pct.to_bits(), n.as_f64().to_bits());
                    }
                }
            }
            "trace" => {
                assert_eq!(
                    object(&expected).get("valid"),
                    Some(&JsonValue::Bool(false))
                );
            }
            _ => panic!("op"),
        }
    }
    assert_eq!(count, 23);
}
#[test]
fn actual_file_loading_and_hash_tampering() {
    let mut doc =
        JsonObject::parse("{\"sourceHashes\":{\"test\":\"abc\"},\"selectedBucketPct\":2.5}")
            .unwrap();
    let hash = compute_bundle_hash(&JsonValue::Object(doc.clone())).unwrap();
    doc.insert("bundleHash".into(), hash.as_str().into())
        .unwrap();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("tl-bitvm-bundle-{}-{unique}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("bundle.json");
    std::fs::write(&path, doc.to_json().unwrap()).unwrap();
    let verified = verify_bundle_file(&hash, &path).unwrap();
    assert_eq!(verified.bundle_hash, hash);
    assert!(std::path::Path::new(&verified.bundle_path).is_absolute());
    doc.insert(
        "selectedBucketPct".into(),
        JsonValue::Number(JsonNumber::from_f64(3.0).unwrap()),
    )
    .unwrap();
    std::fs::write(&path, doc.to_json().unwrap()).unwrap();
    assert!(matches!(
        verify_bundle_file(&hash, &path),
        Err(BitvmError::Source(_))
    ));
    std::fs::write(&path, vec![b'x'; MAX_BUNDLE_BYTES + 1]).unwrap();
    assert_eq!(verify_bundle_file(&hash, &path), Err(BitvmError::Capacity));
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(
        verify_bundle_file(&hash, &path),
        Err(BitvmError::Io(_))
    ));
    std::fs::remove_dir(&directory).unwrap();
}
#[test]
fn source_order_defaults_and_explicit_bounds() {
    assert_ne!(
        compute_bundle_hash(&JsonValue::parse("{\"a\":1,\"b\":2}").unwrap()).unwrap(),
        compute_bundle_hash(&JsonValue::parse("{\"b\":2,\"a\":1}").unwrap()).unwrap()
    );
    assert_eq!(default_bundle_path(Some("")), DEFAULT_BUNDLE_PATH);
    assert_eq!(default_bundle_path(Some("local.json")), "local.json");
    assert_eq!(normalize_hex("\u{feff} ABC\u{a0}"), "abc");
    assert_eq!(
        verify_bundle_bytes("", "label", b"{}"),
        Err(BitvmError::Source("Missing bundleHash".into()))
    );
    assert_eq!(
        verify_bundle_bytes("a", "label", b"{"),
        Err(BitvmError::InvalidJson)
    );
    assert_eq!(
        compute_bundle_hash(&JsonValue::String("\u{1f600}".into())),
        Err(BitvmError::UnsupportedSourceDomain)
    );
    assert_eq!(
        verify_bundle_bytes(&"a".repeat(4097), "x", b"{}"),
        Err(BitvmError::Capacity)
    );
}
