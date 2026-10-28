//! Native reader conformance against actual unchanged PR145 serializers.
use tradelayer_core::compact_candidate_snapshot::*;
use tradelayer_core::{
    raw_block_ingest::UnverifiedRawBlockView,
    read_only::{decode_hex, id_hex, inspect_transaction},
};
#[path = "../src/hash256.rs"]
mod fixture_hash;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
const WRITER_MANIFEST: &str = r###"TLCAND-MANIFEST-1
NETWORK	regtest
ANCHOR	9	6666666666666666666666666666666666666666666666666666666666666666
GEN	1
B	10	c0b08e60ac279dba34ed2f59133680a2d76629f6652b4118f9cc1b6e767b6609	6666666666666666666666666666666666666666666666666666666666666666	4	seg-0000000010-3292ae62926ccaf5d2e65a13c4f35ddea5582a483c49acfdccc5a8e0b3d1e90e.dat	3292ae62926ccaf5d2e65a13c4f35ddea5582a483c49acfdccc5a8e0b3d1e90e
END	1
"###;
const WRITER_SEGMENT: &str = r###"TLCAND-SEGMENT-1	10	c0b08e60ac279dba34ed2f59133680a2d76629f6652b4118f9cc1b6e767b6609	6666666666666666666666666666666666666666666666666666666666666666	4
C	BLOCK	10	c0b08e60ac279dba34ed2f59133680a2d76629f6652b4118f9cc1b6e767b6609	1	f34a7ca1e079de95a3ea73ae39bcf0b2b5f15cfadbd1712baddd04ef5cb2f349	0	legacy-tl	2	typed-syntax	020000000101010101010101010101010101010101010101010101010101010101010101010000000000feffffff010000000000000000106a0e746c32303b616c6963653b353b3200000000
C	BLOCK	10	c0b08e60ac279dba34ed2f59133680a2d76629f6652b4118f9cc1b6e767b6609	2	b10802f071050007fe404f0ba0db34aa560f6f70b2a55aef12b8a1c260d378f8	0	legacy-tl	2	typed-syntax	0200000000010102020202020202020202020202020202020202020202020202020202020202020000000000feffffff010000000000000000106a0e746c32303b616c6963653b353b32010361626300000000
END	2
"###;

static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "tl-candidate-reader-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        fs::create_dir(p.join("segments")).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn raw() -> Vec<u8> {
    decode_hex(include_str!("../fixtures/raw_block_ingest_synthetic.hex").trim()).unwrap()
}
fn hx(v: &[u8]) -> String {
    v.iter().map(|b| format!("{b:02x}")).collect()
}
fn pin(manifest: &[u8], generation: u64) -> CandidateSnapshotExpectation {
    CandidateSnapshotExpectation {
        profile: CompactCandidateProfile::NativeV1BitcoinCanonicalAllOutputs,
        network: CandidateNetwork::Regtest,
        anchor_height: 9,
        anchor_hash_display: [0x66; 32],
        generation,
        manifest_sha256d: fixture_hash::double_sha256(manifest),
    }
}
fn install(t: &Temp, manifest: &str, segment: &str) -> CandidateSnapshotExpectation {
    let line = manifest.lines().find(|l| l.starts_with("B\t")).unwrap();
    let name = line.split('\t').nth(5).unwrap();
    fs::write(t.0.join("segments").join(name), segment.as_bytes()).unwrap();
    fs::write(t.0.join("CURRENT"), manifest.as_bytes()).unwrap();
    pin(manifest.as_bytes(), 1)
}
fn native(t: &Temp) -> CandidateSnapshotExpectation {
    install(t, WRITER_MANIFEST, WRITER_SEGMENT)
}
fn original_rows() -> Vec<String> {
    WRITER_SEGMENT
        .lines()
        .filter(|l| l.starts_with("C\t"))
        .map(str::to_string)
        .collect()
}
fn render(t: &Temp, raw: &[u8], rows: &[String], generation: u64) -> CandidateSnapshotExpectation {
    let view = UnverifiedRawBlockView::parse(raw, None).unwrap();
    let header = id_hex(&view.header_hash_raw());
    let mut parent: [u8; 32] = raw[4..36].try_into().unwrap();
    parent.reverse();
    let segment = format!(
        "TLCAND-SEGMENT-1\t10\t{header}\t{}\t{}\n{}END\t{}\n",
        hx(&parent),
        view.transaction_count(),
        rows.iter().map(|r| r.clone() + "\n").collect::<String>(),
        rows.len()
    );
    let digest = hx(&fixture_hash::double_sha256(segment.as_bytes()));
    let name = format!("seg-0000000010-{digest}.dat");
    let manifest = format!(
        "TLCAND-MANIFEST-1\nNETWORK\tregtest\nANCHOR\t9\t{}\nGEN\t{generation}\nB\t10\t{header}\t{}\t{}\t{name}\t{digest}\nEND\t1\n",
        hx(&parent),
        hx(&parent),
        view.transaction_count()
    );
    let mut expected = install(t, &manifest, &segment);
    expected.generation = generation;
    expected
}
fn changed(rows: &[String], row: usize, field: usize, value: &str) -> Vec<String> {
    let mut r = rows.to_vec();
    let mut f = r[row].split('\t').map(str::to_string).collect::<Vec<_>>();
    f[field] = value.into();
    r[row] = f.join("\t");
    r
}
#[test]
fn unchanged_original_writer_fixture_roundtrips_without_authority() {
    let t = Temp::new();
    let expected = native(&t);
    let before = fs::read(t.0.join("CURRENT")).unwrap();
    let reader = CompactCandidateSnapshot::open(&t.0, expected).unwrap();
    assert_eq!(reader.block_identities().count(), 1);
    assert_eq!(reader.token().observed_tip_height, 10);
    let block = reader.read_block(10, &raw()).unwrap();
    assert_eq!(block.outputs().len(), 2);
    assert_eq!(block.identity().transaction_count, 4);
    assert_eq!(block.snapshot(), reader.token());
    assert_eq!(
        block.raw_block_sha256d(),
        fixture_hash::double_sha256(&raw())
    );
    let view_raw = raw();
    let view = UnverifiedRawBlockView::parse(&view_raw, None).unwrap();
    for out in block.outputs() {
        assert_eq!(
            block.raw_transaction(out.transaction_position),
            view.raw_transaction(out.transaction_position as usize)
        );
    }
    assert_eq!(
        block.require_authenticated_chain(),
        Err(CandidateTrustError::ActiveChainAndConsensusUnverified)
    );
    assert_eq!(
        block.require_authenticated_actor(),
        Err(CandidateTrustError::InputScriptsAndActorEvidenceRequired)
    );
    assert_eq!(
        block.require_tradelayer_transition(),
        Err(CandidateTrustError::TradeLayerStateUnverified)
    );
    assert_eq!(fs::read(t.0.join("CURRENT")).unwrap(), before);
    assert_eq!(
        reader.read_block(11, &raw()),
        Err(CandidateSnapshotError::MissingHeight)
    );
}
#[test]
fn omitted_all_and_one_are_rejected_even_with_repaired_end_digests_and_pin() {
    for rows in [vec![], original_rows()[..1].to_vec()] {
        let t = Temp::new();
        let p = render(&t, &raw(), &rows, 1);
        let reader = CompactCandidateSnapshot::open(&t.0, p).unwrap();
        assert_eq!(
            reader.read_block(10, &raw()),
            Err(CandidateSnapshotError::CandidateSetMismatch)
        );
    }
}
#[test]
fn duplicate_extra_and_reordered_candidates_are_rejected() {
    let rows = original_rows();
    let mut duplicate = rows.clone();
    duplicate.insert(1, rows[0].clone());
    let mut extra = rows.clone();
    extra.push(rows[1].clone());
    let mut reversed = rows.clone();
    reversed.reverse();
    for rows in [duplicate, extra, reversed] {
        let t = Temp::new();
        let p = render(&t, &raw(), &rows, 1);
        assert_eq!(
            CompactCandidateSnapshot::open(&t.0, p)
                .unwrap()
                .read_block(10, &raw()),
            Err(CandidateSnapshotError::CandidateSetMismatch)
        );
    }
}
#[test]
fn opaque_type_syntax_and_format_claims_are_independently_rederived() {
    for (field, value) in [(7, "proposed-TLR"), (8, "999"), (9, "asserted-valid")] {
        let t = Temp::new();
        let rows = changed(&original_rows(), 0, field, value);
        let p = render(&t, &raw(), &rows, 1);
        assert_eq!(
            CompactCandidateSnapshot::open(&t.0, p)
                .unwrap()
                .read_block(10, &raw()),
            Err(CandidateSnapshotError::ClassificationMismatch)
        );
    }
}
#[test]
fn forged_position_txid_vout_and_raw_body_fail_after_checksum_repair() {
    let rows = original_rows();
    for (field, value) in [
        (4, "0".to_string()),
        (5, "0".repeat(64)),
        (6, "999".to_string()),
    ] {
        let t = Temp::new();
        let changed = changed(&rows, 0, field, &value);
        let p = render(&t, &raw(), &changed, 1);
        assert_eq!(
            CompactCandidateSnapshot::open(&t.0, p)
                .unwrap()
                .read_block(10, &raw()),
            Err(CandidateSnapshotError::CandidateSetMismatch)
        );
    }
    let mut body = rows[0].split('\t').nth(10).unwrap().to_string();
    body.replace_range(0..2, "ff");
    let changed = changed(&rows, 0, 10, &body);
    let t = Temp::new();
    let p = render(&t, &raw(), &changed, 1);
    assert_eq!(
        CompactCandidateSnapshot::open(&t.0, p)
            .unwrap()
            .read_block(10, &raw()),
        Err(CandidateSnapshotError::RawTransactionMismatch)
    );
}
#[test]
fn source_raw_block_hash_and_full_merkle_are_required() {
    let t = Temp::new();
    let p = native(&t);
    let reader = CompactCandidateSnapshot::open(&t.0, p).unwrap();
    for index in [0, 369] {
        let mut bytes = raw();
        bytes[index] ^= 1;
        assert_eq!(
            reader.read_block(10, &bytes),
            Err(CandidateSnapshotError::RawBlockMismatch)
        );
    }
}
#[test]
fn wrong_network_anchor_generation_and_publication_pin_fail() {
    let t = Temp::new();
    let p = native(&t);
    let mut bad = p.clone();
    bad.network = CandidateNetwork::Main;
    assert!(matches!(
        CompactCandidateSnapshot::open(&t.0, bad),
        Err(CandidateSnapshotError::WrongContext)
    ));
    let mut bad = p.clone();
    bad.anchor_hash_display = [0; 32];
    assert!(matches!(
        CompactCandidateSnapshot::open(&t.0, bad),
        Err(CandidateSnapshotError::WrongContext)
    ));
    let mut bad = p.clone();
    bad.generation = 2;
    assert!(matches!(
        CompactCandidateSnapshot::open(&t.0, bad),
        Err(CandidateSnapshotError::WrongContext)
    ));
    let mut bad = p;
    bad.manifest_sha256d = [0; 32];
    assert!(matches!(
        CompactCandidateSnapshot::open(&t.0, bad),
        Err(CandidateSnapshotError::StaleSnapshot)
    ));
}
#[test]
fn changed_generation_and_a_b_a_same_tip_never_reuse_old_token() {
    let t = Temp::new();
    let p = native(&t);
    let reader = CompactCandidateSnapshot::open(&t.0, p).unwrap();
    let initial = raw();
    let view = UnverifiedRawBlockView::parse(&initial, None).unwrap();
    let alternate = single_block(view.raw_transaction(0).unwrap());
    let p2 = render(&t, &alternate, &[], 2);
    assert_eq!(
        reader.ensure_current(),
        Err(CandidateSnapshotError::StaleSnapshot)
    );
    assert_eq!(
        reader.read_block(10, &raw()),
        Err(CandidateSnapshotError::StaleSnapshot)
    );
    let second = CompactCandidateSnapshot::open(&t.0, p2).unwrap();
    assert_ne!(
        second
            .read_block(10, &alternate)
            .unwrap()
            .identity()
            .hash_display,
        reader.token().observed_tip_hash_display
    );
    let p3 = render(&t, &raw(), &original_rows(), 3);
    assert_eq!(
        second.ensure_current(),
        Err(CandidateSnapshotError::StaleSnapshot)
    );
    let third = CompactCandidateSnapshot::open(&t.0, p3).unwrap();
    assert_eq!(
        third
            .read_block(10, &raw())
            .unwrap()
            .identity()
            .hash_display,
        reader.token().observed_tip_hash_display
    );
}
#[test]
fn corruption_missing_terminators_and_trailing_rows_fail() {
    let t = Temp::new();
    let p = native(&t);
    let path = t.0.join("segments").join(
        WRITER_MANIFEST
            .lines()
            .find(|l| l.starts_with("B\t"))
            .unwrap()
            .split('\t')
            .nth(5)
            .unwrap(),
    );
    fs::write(&path, b"corrupt").unwrap();
    assert_eq!(
        CompactCandidateSnapshot::open(&t.0, p)
            .unwrap()
            .read_block(10, &raw()),
        Err(CandidateSnapshotError::SegmentDigestMismatch)
    );
    for manifest in [
        WRITER_MANIFEST.replace("END\t1\n", ""),
        WRITER_MANIFEST.to_string() + "END\t1\n",
        WRITER_MANIFEST.replace("\n", "\r\n"),
    ] {
        let t = Temp::new();
        let p = install(&t, &manifest, WRITER_SEGMENT);
        assert!(matches!(
            CompactCandidateSnapshot::open(&t.0, p),
            Err(CandidateSnapshotError::InvalidManifest)
        ));
    }
}
#[test]
fn path_traversal_and_nonregular_current_are_rejected() {
    let t = Temp::new();
    let name = WRITER_MANIFEST
        .lines()
        .find(|l| l.starts_with("B\t"))
        .unwrap()
        .split('\t')
        .nth(5)
        .unwrap();
    let bad = WRITER_MANIFEST.replace(name, "../outside.dat");
    fs::write(t.0.join("CURRENT"), bad.as_bytes()).unwrap();
    assert!(matches!(
        CompactCandidateSnapshot::open(&t.0, pin(bad.as_bytes(), 1)),
        Err(CandidateSnapshotError::InvalidManifest)
    ));
    fs::remove_file(t.0.join("CURRENT")).unwrap();
    fs::create_dir(t.0.join("CURRENT")).unwrap();
    assert!(matches!(
        CompactCandidateSnapshot::open(&t.0, pin(WRITER_MANIFEST.as_bytes(), 1)),
        Err(CandidateSnapshotError::UnsafePath)
    ));
}
#[test]
fn oversized_files_are_rejected_before_unbounded_allocation() {
    let t = Temp::new();
    let p = native(&t);
    let f = fs::OpenOptions::new()
        .write(true)
        .open(t.0.join("CURRENT"))
        .unwrap();
    f.set_len(MAX_MANIFEST_BYTES as u64 + 1).unwrap();
    assert!(matches!(
        CompactCandidateSnapshot::open(&t.0, p),
        Err(CandidateSnapshotError::Limit)
    ));
    let t = Temp::new();
    let p = native(&t);
    let reader = CompactCandidateSnapshot::open(&t.0, p).unwrap();
    let name = WRITER_MANIFEST
        .lines()
        .find(|l| l.starts_with("B\t"))
        .unwrap()
        .split('\t')
        .nth(5)
        .unwrap();
    let f = fs::OpenOptions::new()
        .write(true)
        .open(t.0.join("segments").join(name))
        .unwrap();
    f.set_len(MAX_SEGMENT_BYTES as u64 + 1).unwrap();
    assert_eq!(
        reader.read_block(10, &raw()),
        Err(CandidateSnapshotError::Limit)
    );
}
fn single_block(tx: &[u8]) -> Vec<u8> {
    let mut header = raw()[..80].to_vec();
    header[36..68].copy_from_slice(&inspect_transaction(tx).unwrap().txid);
    header.push(1);
    header.extend_from_slice(tx);
    header
}
fn discover_rows(bytes: &[u8]) -> Vec<String> {
    let view = UnverifiedRawBlockView::parse(bytes, None).unwrap();
    let report = view.inspect_serial().unwrap();
    let mut rows = vec![];
    for tx in report.ordered_preindex().candidates_in_source_order() {
        for c in &tx.observation.carriers {
            rows.push(format!(
                "C\tBLOCK\t10\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                id_hex(&view.header_hash_raw()),
                tx.position,
                id_hex(&tx.observation.txid),
                c.vout,
                c.format.label(),
                c.transaction_type
                    .map_or_else(|| "-".into(), |v| v.to_string()),
                c.status,
                hx(view.raw_transaction(tx.position).unwrap())
            ));
        }
    }
    rows
}
#[test]
fn actual_zero_carrier_block_is_complete_and_raw_bytes_remain_mandatory() {
    let bytes = raw();
    let view = UnverifiedRawBlockView::parse(&bytes, None).unwrap();
    let zero = single_block(view.raw_transaction(0).unwrap());
    assert!(discover_rows(&zero).is_empty());
    let t = Temp::new();
    let p = render(&t, &zero, &[], 1);
    let reader = CompactCandidateSnapshot::open(&t.0, p).unwrap();
    assert!(reader.read_block(10, &zero).unwrap().outputs().is_empty());
    assert_eq!(
        reader.read_block(10, &[]),
        Err(CandidateSnapshotError::RawBlockMismatch)
    );
}
#[test]
fn multiple_carriers_in_one_transaction_require_every_output() {
    let mut tx = vec![1, 0, 0, 0, 1];
    tx.extend_from_slice(&[0x11; 32]);
    tx.extend_from_slice(&[0, 0, 0, 0, 0, 255, 255, 255, 255, 2]);
    for payload in [b"tl2", b"tl7"] {
        tx.extend_from_slice(&[0; 8]);
        tx.push(5);
        tx.extend_from_slice(&[0x6a, 3]);
        tx.extend_from_slice(payload);
    }
    tx.extend_from_slice(&[0; 4]);
    let bytes = single_block(&tx);
    let rows = discover_rows(&bytes);
    assert_eq!(rows.len(), 2);
    let t = Temp::new();
    let p = render(&t, &bytes, &rows, 1);
    let result = CompactCandidateSnapshot::open(&t.0, p)
        .unwrap()
        .read_block(10, &bytes)
        .unwrap();
    assert_eq!(
        result.outputs().iter().map(|c| c.vout).collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(result.raw_transaction(0), Some(tx.as_slice()));
    let p = render(&t, &bytes, &rows[..1], 2);
    assert_eq!(
        CompactCandidateSnapshot::open(&t.0, p)
            .unwrap()
            .read_block(10, &bytes),
        Err(CandidateSnapshotError::CandidateSetMismatch)
    );
}
#[cfg(unix)]
#[test]
fn symlinked_current_is_rejected() {
    use std::os::unix::fs::symlink;
    let t = Temp::new();
    let p = native(&t);
    let outside = t.0.join("outside");
    fs::write(&outside, WRITER_MANIFEST).unwrap();
    fs::remove_file(t.0.join("CURRENT")).unwrap();
    symlink(outside, t.0.join("CURRENT")).unwrap();
    assert!(matches!(
        CompactCandidateSnapshot::open(&t.0, p),
        Err(CandidateSnapshotError::UnsafePath)
    ));
}
