use std::io::Write;
use std::process::{Command, Stdio};
use tradelayer_core::candidate_evidence::*;
use tradelayer_core::raw_block_ingest::{merkle_root_unverified, UnverifiedRawBlockView};
use tradelayer_core::read_only::{decode_hex, hex_bytes, id_hex, inspect_transaction};

const C: &str = concat!(
    "C\tBLOCK\t10\tc0b08e60ac279dba34ed2f59133680a2d76629f6652b4118f9cc1b6e767b6609\t1\tf34a7ca1e079de95a3ea73ae39bcf0b2b5f15cfadbd1712baddd04ef5cb2f349\t0\tlegacy-tl\t2\ttyped-syntax\t020000000101010101010101010101010101010101010101010101010101010101010101010000000000feffffff010000000000000000106a0e746c32303b616c6963653b353b3200000000\n",
    "C\tBLOCK\t10\tc0b08e60ac279dba34ed2f59133680a2d76629f6652b4118f9cc1b6e767b6609\t2\tb10802f071050007fe404f0ba0db34aa560f6f70b2a55aef12b8a1c260d378f8\t0\tlegacy-tl\t2\ttyped-syntax\t0200000000010102020202020202020202020202020202020202020202020202020202020202020000000000feffffff010000000000000000106a0e746c32303b616c6963653b353b32010361626300000000\n",
    "END\tCOMPLETE_UNVERIFIED\tBLOCKS\t1\t4\t2\n"
);
fn fixture() -> Vec<u8> {
    decode_hex(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/raw_block_ingest_synthetic.hex"
        ))
        .trim(),
    )
    .unwrap()
}
fn header_hash(raw: &[u8]) -> [u8; 32] {
    UnverifiedRawBlockView::parse(raw, None)
        .unwrap()
        .header_hash_raw()
}
fn parent(raw: &[u8]) -> [u8; 32] {
    raw[4..36].try_into().unwrap()
}
fn blocks_frame(blocks: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut result = String::new();
    for (height, raw) in blocks {
        result.push_str(&format!(
            "B\t{height}\t{}\t{}\n",
            id_hex(&header_hash(raw)),
            hex_bytes(raw)
        ));
    }
    result.push_str(&format!("E\tBLOCKS\t{}\n", blocks.len()));
    result.into_bytes()
}
fn description(blocks: &[(u32, Vec<u8>)], b: &[u8], c: &[u8]) -> CandidateSnapshotDescriptor {
    CandidateSnapshotDescriptor {
        network: CandidateNetwork::Regtest,
        profile: PROFILE_ID.into(),
        publication_token: [0xa5; 32],
        anchor: CandidateAnchor {
            height: i64::from(blocks[0].0) - 1,
            hash_raw: parent(&blocks[0].1),
        },
        first_height: blocks[0].0,
        last_height: blocks.last().unwrap().0,
        last_hash_raw: header_hash(&blocks.last().unwrap().1),
        block_transcript_sha256: fingerprint_transcript_unverified(b).unwrap(),
        candidate_transcript_sha256: fingerprint_transcript_unverified(c).unwrap(),
    }
}
fn original() -> (Vec<u8>, CandidateSnapshotDescriptor) {
    let blocks = vec![(10, fixture())];
    let b = blocks_frame(&blocks);
    let descriptor = description(&blocks, &b, C.as_bytes());
    (b, descriptor)
}
fn self_bound(b: &[u8], c: &[u8]) -> Result<CandidateEvidenceReader, CandidateEvidenceError> {
    let (_, mut d) = original();
    d.block_transcript_sha256 = fingerprint_transcript_unverified(b).unwrap();
    d.candidate_transcript_sha256 = fingerprint_transcript_unverified(c).unwrap();
    CandidateEvidenceReader::verify(&d, &d, b, c)
}
fn replace_field(transcript: &str, row: usize, field: usize, value: &str) -> String {
    let mut lines = transcript.lines().map(str::to_string).collect::<Vec<_>>();
    let mut fields = lines[row]
        .split('\t')
        .map(str::to_string)
        .collect::<Vec<_>>();
    fields[field] = value.into();
    lines[row] = fields.join("\t");
    lines.join("\n") + "\n"
}
fn scan_cli(b: &[u8]) -> Vec<u8> {
    let mut process = Command::new(env!("CARGO_BIN_EXE_tradelayer-core-index-ingest"))
        .args(["--workers", "1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    process.stdin.take().unwrap().write_all(b).unwrap();
    let out = process.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

#[test]
fn actual_pr142_fixture_matches_independent_expected_fingerprints_and_complete_rows() {
    let (b, d) = original();
    assert_eq!(
        hex_bytes(&d.block_transcript_sha256),
        "11b4e991babd02ef8fed382fc952efd9daad14a035f17fec731a5a6a2cdaf05c"
    );
    assert_eq!(
        hex_bytes(&d.candidate_transcript_sha256),
        "4581f263a37554a9c2261fbe937b4cb2f41970f07346b947851427ccf309bddc"
    );
    assert_eq!(scan_cli(&b), C.as_bytes());
    let reader = CandidateEvidenceReader::verify(&d, &d, &b, C.as_bytes()).unwrap();
    assert_eq!(reader.transaction_count(), 4);
    assert_eq!(reader.carrier_count(), 2);
    assert_eq!(reader.blocks().len(), 1);
    assert!(reader.block(9).is_none());
    assert!(reader.block(11).is_none());
    let block = reader.block(10).unwrap();
    assert_eq!(block.hash_raw(), d.last_hash_raw);
    assert_eq!(block.parent_raw(), d.anchor.hash_raw);
    assert_eq!(block.transaction_count(), 4);
    assert_eq!(
        block
            .candidate_transactions()
            .iter()
            .map(|tx| tx.position())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        reader
            .rows()
            .map(|row| (row.height, row.carrier.vout, row.carrier.status))
            .collect::<Vec<_>>(),
        vec![(10, 0, "typed-syntax"), (10, 0, "typed-syntax")]
    );
    assert_eq!(
        reader.require_authenticated_chain(),
        Err(CandidateEvidenceTrustError::ChainworkAndActiveChainUnverified)
    );
    assert_eq!(
        reader.require_authenticated_utxos(),
        Err(CandidateEvidenceTrustError::InputScriptsAndUtxoUnverified)
    );
    assert_eq!(
        reader.require_execution_authority(),
        Err(CandidateEvidenceTrustError::TradeLayerStateAndActorUnverified)
    );
}

#[test]
fn pr144_omission_and_all_candidate_order_changes_fail_even_with_rebound_hashes() {
    let (b, _) = original();
    // This exact completed transcript committed the PR144 omission probe.
    let missing_all = b"END\tCOMPLETE_UNVERIFIED\tBLOCKS\t1\t4\t0\n";
    assert_eq!(
        self_bound(&b, missing_all),
        Err(CandidateEvidenceError::MissingCandidate)
    );
    let lines = C.lines().collect::<Vec<_>>();
    let missing_last = format!("{}\nEND\tCOMPLETE_UNVERIFIED\tBLOCKS\t1\t4\t1\n", lines[0]);
    assert_eq!(
        self_bound(&b, missing_last.as_bytes()),
        Err(CandidateEvidenceError::MissingCandidate)
    );
    for changed in [
        format!(
            "{}\n{}\nEND\tCOMPLETE_UNVERIFIED\tBLOCKS\t1\t4\t2\n",
            lines[0], lines[0]
        ),
        format!(
            "{}\n{}\nEND\tCOMPLETE_UNVERIFIED\tBLOCKS\t1\t4\t2\n",
            lines[1], lines[0]
        ),
        format!(
            "{}\n{}\n{}\nEND\tCOMPLETE_UNVERIFIED\tBLOCKS\t1\t4\t3\n",
            lines[0], lines[1], lines[1]
        ),
    ] {
        assert!(self_bound(&b, changed.as_bytes()).is_err());
    }
    let changed = replace_field(C, 0, 8, "999");
    assert!(matches!(
        self_bound(&b, changed.as_bytes()),
        Err(CandidateEvidenceError::CandidateMismatch {
            field: CandidateField::TransactionType,
            ..
        })
    ));
    let changed = replace_field(C, 0, 9, "asserted-valid");
    assert!(matches!(
        self_bound(&b, changed.as_bytes()),
        Err(CandidateEvidenceError::CandidateMismatch {
            field: CandidateField::SyntaxStatus,
            ..
        })
    ));
}

#[test]
fn candidate_fields_and_both_completion_markers_are_checked_against_the_rescan() {
    let (b, _) = original();
    for (field, value) in [
        (2, "11"),
        (3, &"00".repeat(32)),
        (4, "0"),
        (5, &"00".repeat(32)),
        (6, "1"),
        (7, "proposed-TLR"),
        (8, "-"),
        (9, "invalid-body"),
        (10, "00"),
    ] {
        let changed = replace_field(C, 0, field, value);
        assert!(self_bound(&b, changed.as_bytes()).is_err(), "field {field}");
    }
    let first_raw = C.lines().next().unwrap().split('\t').nth(10).unwrap();
    let changed_raw = format!("00{}", &first_raw[2..]);
    assert!(matches!(
        self_bound(&b, replace_field(C, 0, 10, &changed_raw).as_bytes()),
        Err(CandidateEvidenceError::CandidateMismatch {
            field: CandidateField::RawTransaction,
            ..
        })
    ));
    for field in 3..=5 {
        let changed = replace_field(C, 2, field, "99");
        assert!(matches!(
            self_bound(&b, changed.as_bytes()),
            Err(CandidateEvidenceError::CandidateCompletionMismatch)
        ));
    }
    for changed in [
        C.lines().take(2).collect::<Vec<_>>().join("\n") + "\n",
        C.replace("BLOCKS\t1\t4\t2", "SCRIPT\t1\t4\t2"),
        format!("{C}E\tBLOCKS\t1\n"),
    ] {
        assert!(self_bound(&b, changed.as_bytes()).is_err());
    }
    let text = std::str::from_utf8(&b).unwrap();
    for changed in [
        text.lines().next().unwrap().to_string() + "\n",
        text.replace("E\tBLOCKS\t1", "E\tBLOCKS\t2"),
        format!("{text}E\tBLOCKS\t1\n"),
        text.replace("E\tBLOCKS", "E\tSCRIPT"),
    ] {
        assert!(self_bound(changed.as_bytes(), C.as_bytes()).is_err());
    }
}

fn build_block(parent_hash: [u8; 32], transactions: &[Vec<u8>], nonce: u32) -> Vec<u8> {
    let mut raw = fixture()[..80].to_vec();
    raw[4..36].copy_from_slice(&parent_hash);
    let txids = transactions
        .iter()
        .map(|tx| inspect_transaction(tx).unwrap().txid)
        .collect::<Vec<_>>();
    raw[36..68].copy_from_slice(&merkle_root_unverified(&txids).unwrap());
    raw[76..80].copy_from_slice(&nonce.to_le_bytes());
    assert!(transactions.len() < 253);
    raw.push(transactions.len() as u8);
    for tx in transactions {
        raw.extend(tx);
    }
    raw
}
fn transaction(scripts: &[&[u8]]) -> Vec<u8> {
    let mut tx = 2u32.to_le_bytes().to_vec();
    tx.push(1);
    tx.extend_from_slice(&[0x73; 32]);
    tx.extend_from_slice(&0u32.to_le_bytes());
    tx.push(0);
    tx.extend_from_slice(&u32::MAX.to_le_bytes());
    tx.push(scripts.len() as u8);
    for script in scripts {
        tx.extend_from_slice(&0u64.to_le_bytes());
        assert!(script.len() < 253);
        tx.push(script.len() as u8);
        tx.extend_from_slice(script);
    }
    tx.extend_from_slice(&0u32.to_le_bytes());
    tx
}
#[test]
fn zero_carrier_blocks_and_multiple_outputs_have_complete_progress_and_one_raw_copy() {
    let raw = fixture();
    let view = UnverifiedRawBlockView::parse(&raw, None).unwrap();
    let plain = view.raw_transaction(0).unwrap().to_vec();
    let empty = build_block(parent(&raw), &[plain], 10);
    let next = build_block(
        header_hash(&empty),
        &[transaction(&[b"\x6a\x0etl20;alice;5;2", b"\x6a\x03TLR"])],
        11,
    );
    let blocks = vec![(10, empty), (11, next)];
    let b = blocks_frame(&blocks);
    let c = scan_cli(&b);
    let d = description(&blocks, &b, &c);
    let reader = CandidateEvidenceReader::verify(&d, &d, &b, &c).unwrap();
    assert_eq!(
        (
            reader.blocks().len(),
            reader.transaction_count(),
            reader.carrier_count()
        ),
        (2, 2, 2)
    );
    assert!(reader
        .block(10)
        .unwrap()
        .candidate_transactions()
        .is_empty());
    let txs = reader.block(11).unwrap().candidate_transactions();
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].carriers().len(), 2);
    let rows = reader.rows().collect::<Vec<_>>();
    assert_eq!(
        rows[0].raw_transaction.as_ptr(),
        rows[1].raw_transaction.as_ptr()
    );
    assert_eq!(rows[1].carrier.status, "invalid-frame");
    assert!(rows[1].carrier.transaction_type.is_none());
    let no_rows = b"END\tCOMPLETE_UNVERIFIED\tBLOCKS\t1\t1\t0\n";
    let first = vec![blocks[0].clone()];
    let b = blocks_frame(&first);
    let d = description(&first, &b, no_rows);
    let verified = CandidateEvidenceReader::verify(&d, &d, &b, no_rows).unwrap();
    assert_eq!(verified.carrier_count(), 0);
    assert_eq!(verified.block(10).unwrap().transaction_count(), 1);
}

#[test]
fn independently_expected_descriptor_prevents_profile_network_snapshot_and_input_splices() {
    let (b, d) = original();
    for field in 0..10 {
        let mut claimed = d.clone();
        match field {
            0 => claimed.network = CandidateNetwork::Main,
            1 => claimed.profile = "future-profile".into(),
            2 => claimed.publication_token[0] ^= 1,
            3 => claimed.anchor.height -= 1,
            4 => claimed.anchor.hash_raw[0] ^= 1,
            5 => claimed.first_height += 1,
            6 => claimed.last_height += 1,
            7 => claimed.last_hash_raw[0] ^= 1,
            8 => claimed.block_transcript_sha256[0] ^= 1,
            _ => claimed.candidate_transcript_sha256[0] ^= 1,
        }
        assert_eq!(
            CandidateEvidenceReader::verify(&d, &claimed, &b, C.as_bytes()),
            Err(CandidateEvidenceError::DescriptorMismatch)
        );
    }
    let altered = replace_field(C, 0, 9, "asserted-valid");
    assert_eq!(
        CandidateEvidenceReader::verify(&d, &d, &b, altered.as_bytes()),
        Err(CandidateEvidenceError::TranscriptDigestMismatch)
    );
    let mut unknown = d.clone();
    unknown.profile = "future-profile".into();
    assert_eq!(
        CandidateEvidenceReader::verify(&unknown, &unknown, &b, C.as_bytes()),
        Err(CandidateEvidenceError::UnsupportedProfile)
    );
    let mut invalid = d.clone();
    invalid.anchor.height = -2;
    assert_eq!(
        CandidateEvidenceReader::verify(&invalid, &invalid, &b, C.as_bytes()),
        Err(CandidateEvidenceError::InvalidAnchorOrRange)
    );
    invalid = d.clone();
    invalid.anchor.height = -1;
    invalid.first_height = 0;
    assert_eq!(
        CandidateEvidenceReader::verify(&invalid, &invalid, &b, C.as_bytes()),
        Err(CandidateEvidenceError::InvalidAnchorOrRange)
    );
    let mut bad_tip = d.clone();
    bad_tip.last_hash_raw[0] ^= 1;
    assert_eq!(
        CandidateEvidenceReader::verify(&bad_tip, &bad_tip, &b, C.as_bytes()),
        Err(CandidateEvidenceError::BlockTipMismatch)
    );
    // Updating both self-reported hashes cannot make incomplete evidence valid.
    assert_eq!(
        self_bound(&b, b"END\tCOMPLETE_UNVERIFIED\tBLOCKS\t1\t4\t0\n"),
        Err(CandidateEvidenceError::MissingCandidate)
    );
}

#[test]
fn raw_block_linkage_merkle_and_canonical_resource_bounds_fail_without_a_reader() {
    let (b, d) = original();
    let text = std::str::from_utf8(&b).unwrap();
    let wrong_header = text.replace(&id_hex(&d.last_hash_raw), &"00".repeat(32));
    assert!(matches!(
        self_bound(wrong_header.as_bytes(), C.as_bytes()),
        Err(CandidateEvidenceError::RawBlock { .. })
    ));
    for changed in [
        text.replace("B\t10\t", "B\t11\t"),
        text.replace("B\t10\t", "B\t010\t"),
        text.replace('\n', "\r\n"),
        format!("{text}\0"),
        text.trim_end().into(),
    ] {
        assert!(self_bound(changed.as_bytes(), C.as_bytes()).is_err());
    }
    let mut raw = fixture();
    raw[4] ^= 1;
    let hash = header_hash(&raw);
    let changed = blocks_frame(&[(10, raw)]);
    let mut anchor = d.clone();
    anchor.last_hash_raw = hash;
    anchor.block_transcript_sha256 = fingerprint_transcript_unverified(&changed).unwrap();
    assert_eq!(
        CandidateEvidenceReader::verify(&anchor, &anchor, &changed, C.as_bytes()),
        Err(CandidateEvidenceError::BlockParentMismatch)
    );
    let mut raw = fixture();
    raw[81] ^= 1;
    let changed = blocks_frame(&[(10, raw)]);
    assert!(matches!(
        self_bound(&changed, C.as_bytes()),
        Err(CandidateEvidenceError::RawBlock { .. })
    ));
    let mut too_many = d.clone();
    too_many.last_height = too_many.first_height + MAX_BLOCKS as u32;
    assert_eq!(
        CandidateEvidenceReader::verify(&too_many, &too_many, &b, C.as_bytes()),
        Err(CandidateEvidenceError::Capacity)
    );
    let mut huge = vec![b'x'; MAX_FRAME_BYTES];
    huge.push(b'\n');
    let mut limit = d.clone();
    limit.block_transcript_sha256 = fingerprint_transcript_unverified(&huge).unwrap();
    assert_eq!(
        CandidateEvidenceReader::verify(&limit, &limit, &huge, C.as_bytes()),
        Err(CandidateEvidenceError::Capacity)
    );
    assert!(self_bound(&b, replace_field(C, 0, 4, "01").as_bytes()).is_err());
    assert!(self_bound(&b, replace_field(C, 0, 5, &"AA".repeat(32)).as_bytes()).is_err());
}

#[test]
fn linked_window_retries_and_aba_forks_never_mix_or_mutate_pinned_readers() {
    let original_raw = fixture();
    let view = UnverifiedRawBlockView::parse(&original_raw, None).unwrap();
    let txs = (0..view.transaction_count())
        .map(|p| view.raw_transaction(p).unwrap().to_vec())
        .collect::<Vec<_>>();
    let a0 = build_block(parent(&original_raw), &txs, 100);
    let a1 = build_block(header_hash(&a0), &txs, 101);
    let blocks_a = vec![(10, a0.clone()), (11, a1)];
    let b_a = blocks_frame(&blocks_a);
    let c_a = scan_cli(&b_a);
    let d_a = description(&blocks_a, &b_a, &c_a);
    let reader_a = CandidateEvidenceReader::verify(&d_a, &d_a, &b_a, &c_a).unwrap();
    assert_eq!(
        (reader_a.transaction_count(), reader_a.carrier_count()),
        (8, 4)
    );
    let b0 = build_block(parent(&original_raw), &[txs[0].clone()], 200);
    let blocks_b = vec![(10, b0)];
    let b_b = blocks_frame(&blocks_b);
    let c_b = scan_cli(&b_b);
    let mut d_b = description(&blocks_b, &b_b, &c_b);
    d_b.publication_token = [0xb6; 32];
    let reader_b = CandidateEvidenceReader::verify(&d_b, &d_b, &b_b, &c_b).unwrap();
    assert!(reader_b.block(11).is_none());
    assert_eq!(reader_b.carrier_count(), 0);
    assert_eq!(
        CandidateEvidenceReader::verify(&d_a, &d_b, &b_b, &c_b),
        Err(CandidateEvidenceError::DescriptorMismatch)
    );
    assert!(CandidateEvidenceReader::verify(&d_b, &d_b, &b_a, &c_b).is_err());
    let mut returned = d_a.clone();
    returned.publication_token = [0xc7; 32];
    assert_eq!(
        CandidateEvidenceReader::verify(&d_a, &returned, &b_a, &c_a),
        Err(CandidateEvidenceError::DescriptorMismatch)
    );
    let restored = CandidateEvidenceReader::verify(&returned, &returned, &b_a, &c_a).unwrap();
    assert_eq!(restored.blocks(), reader_a.blocks());
    assert_eq!(restored.descriptor().publication_token, [0xc7; 32]);
    let mut gap = blocks_a.clone();
    gap[1].0 = 12;
    let b_gap = blocks_frame(&gap);
    let d_gap = description(&gap, &b_gap, &c_a);
    assert_eq!(
        CandidateEvidenceReader::verify(&d_gap, &d_gap, &b_gap, &c_a),
        Err(CandidateEvidenceError::BlockRangeMismatch)
    );
    let mut disconnected = blocks_a.clone();
    disconnected[1].1[4] ^= 1;
    let b_disconnected = blocks_frame(&disconnected);
    let d_disconnected = description(&disconnected, &b_disconnected, &c_a);
    assert_eq!(
        CandidateEvidenceReader::verify(&d_disconnected, &d_disconnected, &b_disconnected, &c_a),
        Err(CandidateEvidenceError::BlockParentMismatch)
    );
    let mut changed = b_a.clone();
    changed.fill(0);
    assert_eq!(reader_a.carrier_count(), 4);
    assert_eq!(
        reader_a.block(11).unwrap().hash_raw(),
        header_hash(&blocks_a[1].1)
    );
}
