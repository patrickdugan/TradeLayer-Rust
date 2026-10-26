use tradelayer_core::secp256k1::{
    canonical_compressed_public_key, verify_ecdsa_digest, verify_ecdsa_digest_strict, SecpError,
};

fn bytes(text: &str) -> Vec<u8> {
    if text == "-" {
        return Vec::new();
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn fixed<const N: usize>(text: &str) -> [u8; N] {
    bytes(text).try_into().unwrap()
}
fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    include_str!("../fixtures/secp256k1_vectors.tsv")
        .lines()
        .filter(|line| line.starts_with(kind) && line.as_bytes().get(kind.len()) == Some(&b'\t'))
        .map(|line| line.split('\t').collect())
        .collect()
}
fn expected(value: &str) -> Result<bool, SecpError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        "key" => Err(SecpError::InvalidPublicKey),
        "signature" => Err(SecpError::InvalidSignature),
        other => panic!("unknown {other}"),
    }
}

#[test]
fn genuine_points_canonicalize_and_malformed_encodings_reject() {
    let rows = rows("P");
    assert_eq!(rows.len(), 143);
    let mut accepted = 0;
    for row in rows {
        let result = canonical_compressed_public_key(&bytes(row[2]));
        if row[3] == "key" {
            assert_eq!(result, Err(SecpError::InvalidPublicKey), "{}", row[1]);
        } else {
            let point = result.unwrap();
            assert_eq!(point, fixed::<33>(row[3]), "{}", row[1]);
            assert_eq!(canonical_compressed_public_key(&point).unwrap(), point);
            accepted += 1;
        }
    }
    assert_eq!(accepted, 129);
}

#[test]
fn genuine_tiny_signatures_and_mutations_match_both_policies() {
    let rows = rows("V");
    assert_eq!(rows.len(), 252);
    for row in rows {
        let digest = fixed(row[2]);
        let key = bytes(row[3]);
        let sig = fixed(row[4]);
        assert_eq!(
            verify_ecdsa_digest(&digest, &key, &sig),
            expected(row[5]),
            "{} default",
            row[1]
        );
        assert_eq!(
            verify_ecdsa_digest_strict(&digest, &key, &sig),
            expected(row[6]),
            "{} strict",
            row[1]
        );
    }
}

#[test]
fn independent_openssl_signatures_verify_in_native_arithmetic() {
    let rows = rows("O");
    assert_eq!(rows.len(), 12);
    for row in rows {
        let digest = fixed(row[3]);
        let key = bytes(row[4]);
        let sig = fixed(row[5]);
        assert_eq!(
            verify_ecdsa_digest(&digest, &key, &sig),
            Ok(true),
            "{}",
            row[1]
        );
        assert_eq!(
            verify_ecdsa_digest_strict(&digest, &key, &sig),
            expected(row[7]),
            "{}",
            row[1]
        );
        let mut wrong_digest = digest;
        wrong_digest[0] ^= 0x80;
        assert_eq!(verify_ecdsa_digest(&wrong_digest, &key, &sig), Ok(false));
    }
}

#[test]
fn rare_x_mod_order_and_infinity_paths_are_exercised() {
    let rows = rows("V");
    for (name, answer) in [
        ("affine-x-greater-than-order", true),
        ("infinity-sum-g", false),
        ("infinity-sum-negative-g", false),
    ] {
        let row = rows.iter().find(|row| row[1] == name).unwrap();
        assert_eq!(
            verify_ecdsa_digest(&fixed(row[2]), &bytes(row[3]), &fixed(row[4])),
            Ok(answer),
            "{name}"
        );
    }
    // The rare witness point itself has x>=n; the test would fail a verifier
    // comparing affine x directly to r instead of x mod n.
    let points = rows_for_rare_point();
    let n = bytes("fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141");
    assert!(points[1..] >= n[..]);
}
fn rows_for_rare_point() -> Vec<u8> {
    let row = rows("P")
        .into_iter()
        .find(|row| row[1] == "rare-high-x")
        .unwrap();
    bytes(row[2])
}

#[test]
fn scalar_zero_errors_and_high_s_policy_are_explicit() {
    let all = rows("V");
    let low = all.iter().find(|row| row[1] == "tiny-low-0").unwrap();
    let high = all.iter().find(|row| row[1] == "tiny-high-0").unwrap();
    let digest = fixed(low[2]);
    let key = bytes(low[3]);
    let sig = fixed::<64>(low[4]);
    let half = bytes("7fffffffffffffffffffffffffffffff5d576e7357a4501ddfe92f46681b20a0");
    assert!(sig[..32] > half[..]); // genuine strict verify accepts high R.
    assert_eq!(verify_ecdsa_digest_strict(&digest, &key, &sig), Ok(true));
    assert_eq!(
        verify_ecdsa_digest(&digest, &key, &fixed(high[4])),
        Ok(true)
    );
    assert_eq!(
        verify_ecdsa_digest_strict(&digest, &key, &fixed(high[4])),
        Ok(false)
    );
    for name in ["zero-r", "zero-s", "zero-both"] {
        let row = all.iter().find(|row| row[1] == name).unwrap();
        assert_eq!(row[7], "false"); // tiny returns false; native API is typed.
        assert_eq!(
            verify_ecdsa_digest(&fixed(row[2]), &bytes(row[3]), &fixed(row[4])),
            Err(SecpError::InvalidSignature)
        );
    }
    for key in [Vec::new(), vec![2; 32], vec![4; 66], vec![2; 1_048_576]] {
        assert_eq!(
            canonical_compressed_public_key(&key),
            Err(SecpError::InvalidPublicKey)
        );
        assert_eq!(
            verify_ecdsa_digest(&digest, &key, &sig),
            Err(SecpError::InvalidPublicKey)
        );
    }
}
