use tradelayer_core::bip32_public::{
    derive_addresses_source_compatible, derive_addresses_with_source_library, AddressNetwork,
    Bip32Error, ExtendedPublicKey, PublicVersion, SourceLibrary,
};
use tradelayer_core::secp256k1::{public_key_tweak_add, SecpError};

fn bytes(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn fixed<const N: usize>(text: &str) -> [u8; N] {
    bytes(text).try_into().unwrap()
}
fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}
fn rows(kind: char) -> Vec<Vec<&'static str>> {
    include_str!("../fixtures/bip32_public_vectors.tsv")
        .lines()
        .filter(|line| line.starts_with(kind) && line.as_bytes().get(1) == Some(&b'\t'))
        .map(|line| line.split('\t').collect())
        .collect()
}

#[test]
fn all_published_bip32_public_keys_parse_and_roundtrip_metadata() {
    let rows = rows('P');
    assert_eq!(rows.len(), 17);
    for row in rows {
        let key = ExtendedPublicKey::parse(row[2]).unwrap();
        assert_eq!(key.to_base58(), row[2], "{}", row[1]);
        assert_eq!(key.version(), PublicVersion::Mainnet);
        assert_eq!(key.depth(), row[3].parse::<u8>().unwrap());
        assert_eq!(hex(&key.parent_fingerprint()), row[4]);
        assert_eq!(key.child_number(), row[5].parse::<u32>().unwrap());
        assert_eq!(hex(&key.chain_code()), row[6]);
        assert_eq!(hex(&key.public_key()), row[7]);
    }
}

#[test]
fn every_published_invalid_vector_is_rejected_by_strict_public_parser() {
    let rows = rows('I');
    assert_eq!(rows.len(), 16);
    for row in &rows {
        assert!(ExtendedPublicKey::parse(row[2]).is_err(), "{}", row[1]);
    }
    for row in rows.iter().filter(|row| row[1].starts_with("public-root-")) {
        assert_eq!(
            ExtendedPublicKey::parse(row[2]),
            Err(Bip32Error::InvalidRootMetadata)
        );
        assert_eq!(
            derive_addresses_source_compatible(row[2]).unwrap().len(),
            40
        );
    }
}

#[test]
fn published_child_edges_and_independent_public_derivations_match_genuine_libraries() {
    let rows = rows('D');
    assert_eq!(rows.len(), 54);
    for row in rows {
        let parent = ExtendedPublicKey::parse(row[2]).unwrap();
        let child = parent.derive_child(row[3].parse().unwrap()).unwrap();
        assert_eq!(child.to_base58(), row[4], "{}", row[1]);
        assert_eq!(child.p2pkh_address(AddressNetwork::BitcoinMainnet), row[5]);
        assert_eq!(child.p2pkh_address(AddressNetwork::LitecoinMainnet), row[6]);
        assert_eq!(child.p2pkh_address(AddressNetwork::Testnet), row[7]);
        assert_eq!(child.parent_fingerprint(), parent.fingerprint());
    }
}

#[test]
fn actual_source_receive_and_change_arrays_match_litecoin_and_bitcoin_fallback() {
    let rows = rows('S');
    assert_eq!(rows.len(), 800);
    for group in rows.chunks_exact(40) {
        let library = if group[0][1] == "ltc" {
            SourceLibrary::Litecoin
        } else {
            SourceLibrary::BitcoinFallback
        };
        let actual = derive_addresses_with_source_library(group[0][3], library).unwrap();
        let expected: Vec<_> = group.iter().map(|row| row[6]).collect();
        assert_eq!(actual, expected, "{} {}", group[0][1], group[0][2]);
        if library == SourceLibrary::Litecoin {
            assert_eq!(
                derive_addresses_source_compatible(group[0][3]).unwrap(),
                actual
            );
        }
        for (i, row) in group.iter().enumerate() {
            assert_eq!(row[4].parse::<usize>().unwrap(), i / 20);
            assert_eq!(row[5].parse::<usize>().unwrap(), i % 20);
        }
    }
    let test = rows.iter().find(|row| row[2] == "tpub").unwrap();
    let key = ExtendedPublicKey::parse(test[3]).unwrap();
    assert_eq!(key.version(), PublicVersion::Testnet);
    assert_eq!(key.to_base58(), test[3]);
    for row in rows
        .iter()
        .filter(|row| row[2].starts_with("field-overflow-"))
    {
        assert_eq!(
            ExtendedPublicKey::parse(row[3]),
            Err(Bip32Error::InvalidPublicKey)
        );
    }
}

#[test]
fn genuine_private_key_source_acceptance_is_an_explicit_native_unsupported_domain() {
    let rows = rows('O');
    assert_eq!(rows.len(), 4);
    for row in rows {
        if row[2] == "private" {
            assert_eq!(row[4], "40");
            assert_eq!(row[5], "true");
            assert_eq!(
                derive_addresses_source_compatible(row[3]),
                Err(Bip32Error::UnsupportedPrivateKey)
            );
        } else {
            assert_eq!(row[4], "0");
            assert_eq!(row[5], "false");
            assert!(derive_addresses_source_compatible(row[3]).is_err());
        }
    }
}

#[test]
fn tiny_secp256k1_differential_public_tweaks_cover_zero_order_and_infinity() {
    let rows = rows('T');
    assert_eq!(rows.len(), 96);
    for row in rows {
        let actual = public_key_tweak_add(&bytes(row[2]), &fixed(row[3]));
        match row[4] {
            "arithmetic" => assert_eq!(actual, Err(SecpError::Arithmetic), "{}", row[1]),
            "infinity" => assert_eq!(actual, Ok(None), "{}", row[1]),
            point => assert_eq!(actual.unwrap().unwrap(), fixed::<33>(point), "{}", row[1]),
        }
    }
    for row in include_str!("../fixtures/secp256k1_vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("P\t"))
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(|row| row[3] == "key")
    {
        let key = if row[2] == "-" {
            Vec::new()
        } else {
            bytes(row[2])
        };
        assert_eq!(
            public_key_tweak_add(&key, &[0; 32]),
            Err(SecpError::InvalidPublicKey)
        );
    }
}

#[test]
fn bounded_decode_and_explicit_derivation_failures() {
    for input in [
        String::new(),
        "1".repeat(110),
        "1".repeat(112),
        "x".repeat(1024 * 1024),
    ] {
        assert_eq!(
            ExtendedPublicKey::parse(&input),
            Err(Bip32Error::InvalidLength)
        );
    }
    let fixture = rows('P')[0][2];
    for invalid in [b'0', b'O', b'I', b'l', b' ', b'\n', 0x80] {
        let mut text = fixture.as_bytes().to_vec();
        text[50] = invalid;
        let result = ExtendedPublicKey::parse(&String::from_utf8_lossy(&text));
        assert!(matches!(
            result,
            Err(Bip32Error::InvalidBase58 | Bip32Error::InvalidLength)
        ));
    }
    let mut changed = fixture.as_bytes().to_vec();
    changed[110] = if changed[110] == b'1' { b'2' } else { b'1' };
    assert_eq!(
        ExtendedPublicKey::parse(core::str::from_utf8(&changed).unwrap()),
        Err(Bip32Error::InvalidChecksum)
    );
    let root = ExtendedPublicKey::parse(fixture).unwrap();
    assert_eq!(root.derive_child(1 << 31), Err(Bip32Error::HardenedChild));
    assert_eq!(root.derive_child(u32::MAX), Err(Bip32Error::HardenedChild));
    let deepest = rows('S').iter().find(|row| row[2] == "depth255").unwrap()[3];
    assert_eq!(
        ExtendedPublicKey::parse(deepest).unwrap().derive_child(0),
        Err(Bip32Error::DepthExhausted)
    );
}
