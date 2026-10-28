use tradelayer_core::read_only::litecoin_transparent::*;
use tradelayer_core::read_only::{decode_hex, id_hex, inspect_transaction};
const PROFILE: LitecoinTransparentProfile = LitecoinTransparentProfile::Core021EmptyMwebV1;
fn official() -> Vec<u8> {
    decode_hex(include_str!("../fixtures/litecoin_official_post_mweb.hex").trim()).unwrap()
}

#[test]
fn official_complete_post_mweb_block_rederives_declared_ids_and_merkle() {
    let raw = official();
    let block = UnverifiedLitecoinTransparentBlock::parse(&raw, PROFILE, None).unwrap();
    assert_eq!(
        id_hex(&block.block_id_raw()),
        "015e0a3aeca99cc8bd924beebd13ad7e697a1cddf73b54d478d21521f07c41c2"
    );
    assert_eq!(
        id_hex(&block.transparent_merkle_root_raw()),
        "c1d0a808a49f04ac12753bbe990def6943c4a7bd6734f66a3514ab2578540f92"
    );
    assert_eq!(block.transaction_ranges(), &[81..291, 291..676, 676..773]);
    assert_eq!(block.extension_range(), Some(773..944));
    let expected = [
        (
            "47bc54ec3a2c07e5023727c1489e110a95a06ce6027c91db94ebf8f38386233c",
            "4341af3bbe5be8d974fdb4447965717710a57ab4d41049829f9f13b3b280ec2d",
            false,
        ),
        (
            "f695435385ef0d7bd8cc3c1d5c10b9f5f0a2e2d6978a7ed5bf011fd3a4817d03",
            "5c5211aef305551bcdf90e2f4af78fcc834b51041da8512f9fcb14bcff0b07e7",
            false,
        ),
        (
            "efe1699039d3a8fa35dd3fad367e446937ab33bd19f9f0252c46d09b29fe733f",
            "efe1699039d3a8fa35dd3fad367e446937ab33bd19f9f0252c46d09b29fe733f",
            true,
        ),
    ];
    for (tx, (id, wid, hogex)) in block.transactions().iter().zip(expected) {
        let observation = tx.inspect();
        assert_eq!(id_hex(&observation.txid), id);
        assert_eq!(id_hex(&observation.wtxid), wid);
        assert_eq!(tx.is_hogex(), hogex);
        assert!(observation.carriers.is_empty());
        assert_eq!(
            UnverifiedLitecoinTransparentTransaction::parse(tx.raw_bytes(), PROFILE)
                .unwrap()
                .inspect(),
            observation
        );
    }
    let mw = block.mweb_header().unwrap();
    assert_eq!(mw.height, 2_273_263);
    assert_eq!(mw.output_mmr_size, 1614);
    assert_eq!(mw.kernel_mmr_size, 0);
    assert_eq!(
        block.transparent_weight(),
        3 * (81 + 174 + 276 + 94) + (81 + 210 + 385 + 94)
    );
    assert!(block.require_full_litecoin_consensus().is_err());
    assert_eq!(block.profile(), PROFILE);
    assert_eq!(block.raw_bytes(), raw);
}

#[test]
fn genuine_official_pre_mweb_genesis_examples_keep_sha256d_identifiers() {
    for (hex, id) in [
        (
            include_str!("../fixtures/litecoin_main_genesis.hex"),
            "12a765e31ffd4059bada1e25190f6e98c99d9714d334efa41a195a7e7e04bfe2",
        ),
        (
            include_str!("../fixtures/litecoin_test_genesis.hex"),
            "4966625a4b2851d9fdee139e56211a0d88575f59ed816ff5e6a63deb4e3e29a0",
        ),
        (
            include_str!("../fixtures/litecoin_regtest_genesis.hex"),
            "530827f38f93b43ed12af0b3ad25a288dc02ed74d6d7857862df51fc56c416f9",
        ),
    ] {
        let raw = decode_hex(hex.trim()).unwrap();
        let block = UnverifiedLitecoinTransparentBlock::parse(&raw, PROFILE, None).unwrap();
        assert_eq!(id_hex(&block.block_id_raw()), id);
        assert!(block.extension_range().is_none());
        assert!(block.mweb_header().is_none());
        let tx = &block.transactions()[0];
        assert_eq!(tx.inspect(), inspect_transaction(tx.raw_bytes()).unwrap());
    }
}

#[test]
fn bitcoin_default_admission_and_original_transaction_bytes_stay_unchanged() {
    let raw = official();
    assert!(tradelayer_core::raw_block_ingest::UnverifiedRawBlockView::parse(&raw, None).is_err());
    assert!(inspect_transaction(&raw[676..773]).is_err());
    for range in [81..291, 291..676] {
        let bytes = &raw[range];
        let before = bytes.to_vec();
        let old = inspect_transaction(bytes).unwrap();
        assert_eq!(
            UnverifiedLitecoinTransparentTransaction::parse(bytes, PROFILE)
                .unwrap()
                .inspect(),
            old
        );
        assert_eq!(bytes, before);
    }
}

#[test]
fn every_official_block_truncation_trailing_byte_and_hash_substitution_fails() {
    let raw = official();
    for end in 0..raw.len() {
        assert!(
            UnverifiedLitecoinTransparentBlock::parse(&raw[..end], PROFILE, None).is_err(),
            "truncation {end}"
        );
    }
    let mut trailing = raw.clone();
    trailing.push(0);
    assert!(matches!(
        UnverifiedLitecoinTransparentBlock::parse(&trailing, PROFILE, None),
        Err(LitecoinTransparentError::TrailingBytes)
    ));
    assert!(matches!(
        UnverifiedLitecoinTransparentBlock::parse(&raw, PROFILE, Some([7; 32])),
        Err(LitecoinTransparentError::RequestedBlockIdMismatch)
    ));
    let mut changed = raw.clone();
    changed[721] ^= 1;
    assert!(matches!(
        UnverifiedLitecoinTransparentBlock::parse(&changed, PROFILE, None),
        Err(LitecoinTransparentError::TransparentMerkleRootMismatch)
    ));
}

#[test]
fn unimplemented_mweb_bodies_and_noncanonical_optional_bytes_refuse_explicitly() {
    let raw = official();
    for index in 941..944 {
        let mut changed = raw.clone();
        changed[index] = 1;
        assert!(matches!(
            UnverifiedLitecoinTransparentBlock::parse(&changed, PROFILE, None),
            Err(LitecoinTransparentError::UnsupportedNonemptyMwebBody)
        ));
    }
    let mut unknown = raw.clone();
    unknown[773] = 2;
    assert!(matches!(
        UnverifiedLitecoinTransparentBlock::parse(&unknown, PROFILE, None),
        Err(LitecoinTransparentError::NonCanonicalOptionalPresence(2))
    ));
    let mut null = raw[..773].to_vec();
    null.push(0);
    let view = UnverifiedLitecoinTransparentBlock::parse(&null, PROFILE, None).unwrap();
    assert!(view.mweb_header().is_none());
    assert_eq!(view.extension_range(), Some(773..774));
    let mut tx = raw[676..773].to_vec();
    let pointer = tx.len() - 5;
    tx[pointer] = 1;
    assert!(matches!(
        UnverifiedLitecoinTransparentTransaction::parse(&tx, PROFILE),
        Err(LitecoinTransparentError::UnsupportedNonNullMwebTransaction)
    ));
    tx[pointer] = 2;
    assert!(matches!(
        UnverifiedLitecoinTransparentTransaction::parse(&tx, PROFILE),
        Err(LitecoinTransparentError::NonCanonicalOptionalPresence(2))
    ));
    for flag in [0, 2, 4, 10, 255] {
        let mut tx = raw[676..773].to_vec();
        tx[5] = flag;
        assert!(
            matches!(UnverifiedLitecoinTransparentTransaction::parse(&tx,PROFILE),Err(LitecoinTransparentError::UnsupportedOptionalFlags(v))if v==flag)
        );
    }
    let mut only = raw[676..773].to_vec();
    only[6] = 0;
    assert!(matches!(
        UnverifiedLitecoinTransparentTransaction::parse(&only, PROFILE),
        Err(LitecoinTransparentError::UnsupportedMwebOnlyTransaction)
    ));
}

// Synthetic byte construction tests framing, hashing and carrier extraction
// only; these are not mined transactions or supplied source-validity answers.
fn framing(flags: u8) -> Vec<u8> {
    let mut tx = 2u32.to_le_bytes().to_vec();
    if flags != 0 {
        tx.extend([0, flags]);
    }
    tx.push(1);
    tx.extend([0; 36]);
    tx.push(0);
    tx.extend([255; 4]);
    tx.push(1);
    tx.extend([0; 8]);
    tx.extend([5, 0x6a, 3, b't', b'l', b'0']);
    if flags & 1 != 0 {
        tx.extend([1, 0]);
    }
    if flags & 8 != 0 {
        tx.push(0);
    }
    tx.extend([0; 4]);
    tx
}
#[test]
fn synthetic_flag9_retains_witness_id_and_uses_existing_carrier_classifier() {
    let legacy = framing(0);
    let witness = framing(1);
    let hogex = framing(8);
    let combined = framing(9);
    let a = UnverifiedLitecoinTransparentTransaction::parse(&legacy, PROFILE)
        .unwrap()
        .inspect();
    let b = UnverifiedLitecoinTransparentTransaction::parse(&witness, PROFILE)
        .unwrap()
        .inspect();
    let c = UnverifiedLitecoinTransparentTransaction::parse(&hogex, PROFILE)
        .unwrap()
        .inspect();
    let d = UnverifiedLitecoinTransparentTransaction::parse(&combined, PROFILE)
        .unwrap()
        .inspect();
    assert_eq!(a.txid, b.txid);
    assert_eq!(a.txid, c.txid);
    assert_eq!(a.txid, d.txid);
    assert_eq!(b.wtxid, d.wtxid);
    assert_eq!(c.wtxid, c.txid);
    assert_eq!(a.carriers, b.carriers);
    assert_eq!(a.carriers, d.carriers);
    assert_eq!(d.carriers.len(), 1);
    assert_eq!(d.carriers[0].payload, b"tl0");
    let mut empty = combined.clone();
    let at = empty.len() - 7;
    empty.remove(at + 1);
    empty[at] = 0;
    assert!(UnverifiedLitecoinTransparentTransaction::parse(&empty, PROFILE).is_err());
    let mut extra = hogex;
    extra.push(0);
    assert!(matches!(
        UnverifiedLitecoinTransparentTransaction::parse(&extra, PROFILE),
        Err(LitecoinTransparentError::TrailingBytes)
    ));
}

#[test]
fn canonical_counts_resource_caps_and_bounded_mweb_varints_fail_closed() {
    let raw = official();
    let mut noncanonical = raw[..80].to_vec();
    noncanonical.extend([253, 3, 0]);
    noncanonical.extend(&raw[81..]);
    assert!(matches!(
        UnverifiedLitecoinTransparentBlock::parse(&noncanonical, PROFILE, None),
        Err(LitecoinTransparentError::Syntax(
            tradelayer_core::read_only::ScanError::NonCanonicalCompactSize
        ))
    ));
    let mut zero = raw[..80].to_vec();
    zero.push(0);
    assert!(matches!(
        UnverifiedLitecoinTransparentBlock::parse(&zero, PROFILE, None),
        Err(LitecoinTransparentError::InvalidTransactionCount)
    ));
    let over = vec![0; MAX_LITECOIN_TRANSPARENT_BLOCK_BYTES + 1];
    assert!(matches!(
        UnverifiedLitecoinTransparentBlock::parse(&over, PROFILE, None),
        Err(LitecoinTransparentError::OversizedBlock)
    ));
    let mut height = raw[..774].to_vec();
    height.extend([255; 20]);
    assert!(matches!(
        UnverifiedLitecoinTransparentBlock::parse(&height, PROFILE, None),
        Err(LitecoinTransparentError::InvalidMwebHeaderVarint)
    ));
    let mut bad_mmr = raw[..938].to_vec();
    bad_mmr.extend([255; 20]);
    assert!(UnverifiedLitecoinTransparentBlock::parse(&bad_mmr, PROFILE, None).is_err());
    let too_long = vec![0; tradelayer_core::read_only::MAX_RAW_TRANSACTION + 1];
    assert!(matches!(
        UnverifiedLitecoinTransparentTransaction::parse(&too_long, PROFILE),
        Err(LitecoinTransparentError::Syntax(
            tradelayer_core::read_only::ScanError::TooLong
        ))
    ));
}

#[test]
fn hogex_position_and_duplicate_transparent_ids_are_not_silently_accepted() {
    let raw = official();
    let mut misplaced = raw[..291].to_vec();
    misplaced.extend(&raw[676..773]);
    misplaced.extend(&raw[291..676]);
    misplaced.extend(&raw[773..]);
    assert!(matches!(
        UnverifiedLitecoinTransparentBlock::parse(&misplaced, PROFILE, None),
        Err(LitecoinTransparentError::HogExNotFinal)
    ));
    let mut duplicate = raw[..80].to_vec();
    duplicate.push(4);
    duplicate.extend(&raw[81..676]);
    duplicate.extend(&raw[291..676]);
    duplicate.extend(&raw[676..]);
    assert!(matches!(
        UnverifiedLitecoinTransparentBlock::parse(&duplicate, PROFILE, None),
        Err(LitecoinTransparentError::InvalidTransparentMerkleTree)
    ));
}
