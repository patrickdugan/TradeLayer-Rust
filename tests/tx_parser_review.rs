//! Expected hashes come from scripts/audit_tx_parser_review.py's independent
//! constructed serializer and Python hashlib, never from this crate's parser.
use tradelayer_core::parallel_txindex::ParallelIndexConfig;
use tradelayer_core::raw_block_ingest::{RawBlockIngestError, UnverifiedRawBlockView};
use tradelayer_core::read_only::{decode_hex, id_hex, inspect_transaction, ScanError};

struct Vector<'a> {
    label: &'a str,
    status: &'a str,
    inputs: &'a str,
    outputs: &'a str,
    stripped: &'a str,
    txid: &'a str,
    wtxid: &'a str,
    raw: Vec<u8>,
}

fn vectors() -> Vec<Vector<'static>> {
    include_str!("../fixtures/tx_parser_review.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            let [label, status, inputs, outputs, stripped, txid, wtxid, raw] =
                <[&str; 8]>::try_from(fields).expect("8 fixture fields");
            Vector {
                label,
                status,
                inputs,
                outputs,
                stripped,
                txid,
                wtxid,
                raw: decode_hex(raw).unwrap(),
            }
        })
        .collect()
}

fn error_class(error: &ScanError) -> &str {
    match error {
        ScanError::Truncated => "Truncated",
        ScanError::Invalid(_) => "Invalid",
        ScanError::NonCanonicalCompactSize => "NonCanonicalCompactSize",
        ScanError::TrailingBytes => "TrailingBytes",
        ScanError::TooLong => "TooLong",
        ScanError::InvalidHex => "InvalidHex",
    }
}

#[test]
fn empty_item_witness_and_compactsize_hashes_match_independent_hashlib() {
    let all = vectors();
    assert_eq!(all.len(), 104);
    let mut accepted = 0;
    for vector in &all {
        let result = inspect_transaction(&vector.raw);
        if vector.status == "ok" {
            let tx = result.expect(vector.label);
            assert_eq!(id_hex(&tx.txid), vector.txid, "{} TXID", vector.label);
            assert_eq!(id_hex(&tx.wtxid), vector.wtxid, "{} wTXID", vector.label);
            assert_eq!(tx.input_count, vector.inputs.parse::<u32>().unwrap());
            assert_eq!(tx.output_count, vector.outputs.parse::<u32>().unwrap());
            assert_eq!(tx.has_witness, !vector.label.starts_with("legacy-"));
            accepted += 1;
        } else {
            let error = result.expect_err(vector.label);
            assert_eq!(
                error_class(&error),
                vector.status,
                "{}: {error:?}",
                vector.label
            );
        }
    }
    assert_eq!(accepted, 11);
}

#[test]
fn borrowed_block_preparse_and_both_worker_paths_match_external_hashes() {
    let bytes = decode_hex(include_str!("../fixtures/tx_parser_review_block.hex").trim()).unwrap();
    let accepted: Vec<_> = vectors().into_iter().filter(|v| v.status == "ok").collect();
    let view = UnverifiedRawBlockView::parse(&bytes, None).unwrap();
    assert_eq!(view.transaction_count(), accepted.len());
    let stripped = 81
        + accepted
            .iter()
            .map(|v| v.stripped.parse::<usize>().unwrap())
            .sum::<usize>();
    assert_eq!(view.stripped_block_bytes(), stripped);
    assert_eq!(view.block_weight(), (3 * stripped + bytes.len()) as u64);
    let config = ParallelIndexConfig {
        workers: 3,
        window_transactions: 2,
    };
    for report in [
        view.inspect_serial().unwrap(),
        view.inspect_parallel(config).unwrap(),
        view.inspect_parallel_one_team(config).unwrap(),
    ] {
        for (row, expected) in report.ordered_preindex().ordered().iter().zip(&accepted) {
            assert_eq!(
                view.raw_transaction(row.position),
                Some(expected.raw.as_slice())
            );
            assert_eq!(
                id_hex(&row.observation.txid),
                expected.txid,
                "{}",
                expected.label
            );
            assert_eq!(
                id_hex(&row.observation.wtxid),
                expected.wtxid,
                "{}",
                expected.label
            );
        }
        assert_eq!(
            report.ordered_preindex().transaction_count(),
            accepted.len()
        );
    }
}

#[test]
fn unknown_version_parent_and_empty_item_child_remain_syntax_only() {
    let all = vectors();
    let parent = all
        .iter()
        .find(|v| v.label == "legacy-unknown-v2-parent")
        .unwrap();
    let child = all
        .iter()
        .find(|v| v.label == "unknown-v2-parent-spend-empty-item")
        .unwrap();
    assert_eq!(
        &parent.raw[parent.raw.len() - 8..parent.raw.len() - 4],
        [0x52, 2, 0xab, 0xcd]
    );
    let mut parent_hash_raw = decode_hex(parent.txid).unwrap();
    parent_hash_raw.reverse();
    assert_eq!(&child.raw[7..39], parent_hash_raw);
    assert_eq!(&child.raw[child.raw.len() - 6..child.raw.len() - 4], [1, 0]);
    assert!(inspect_transaction(&parent.raw).is_ok());
    assert!(inspect_transaction(&child.raw).is_ok());
    // No script verification or authenticated UTXO result is asserted.
}

#[test]
fn borrowed_prefix_rejects_malformed_flags_counts_and_truncations_independently() {
    for vector in vectors().into_iter().filter(|v| v.status != "ok") {
        let mut block = vec![0; 80];
        block.push(1);
        block.extend_from_slice(&vector.raw);
        // A trailing byte is legal after a prefix transaction and is rejected
        // at the block boundary, as distinct from standalone TX inspection.
        if vector.status == "TrailingBytes" {
            assert!(matches!(
                UnverifiedRawBlockView::parse(&block, None),
                Err(RawBlockIngestError::TrailingRawBlockData)
            ));
        } else {
            match UnverifiedRawBlockView::parse(&block, None) {
                Err(RawBlockIngestError::TransactionBoundary { index, source }) => {
                    assert_eq!(index, 0);
                    assert_eq!(error_class(&source), vector.status, "{}", vector.label);
                }
                result => panic!("{}: unexpected block parse {result:?}", vector.label),
            }
        }
    }
}
