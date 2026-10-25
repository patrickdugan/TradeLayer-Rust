//! Baseline-compatible dependency-scheduler boundary regressions. This file
//! does not authenticate UTXOs, witnesses, scripts or protocol mutations.
use tradelayer_core::parallel_txindex::ParallelIndexConfig;
use tradelayer_core::raw_block_ingest::{
    merkle_root_unverified, DependencyAuditError, DependencyTrustError, UnverifiedRawBlockView,
};
use tradelayer_core::read_only::{decode_hex, inspect_transaction};

fn fixture() -> Vec<u8> {
    decode_hex(include_str!("../fixtures/raw_block_ingest_synthetic.hex").trim()).unwrap()
}

fn block(rows: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = fixture()[..80].to_vec();
    let txids: Vec<_> = rows
        .iter()
        .map(|row| inspect_transaction(row).unwrap().txid)
        .collect();
    bytes[36..68].copy_from_slice(&merkle_root_unverified(&txids).unwrap());
    assert!(rows.len() < 253);
    bytes.push(rows.len() as u8);
    for row in rows {
        bytes.extend_from_slice(row);
    }
    bytes
}

fn transaction(inputs: &[([u8; 32], u32)], outputs: u8, tag: u32) -> Vec<u8> {
    let mut bytes = 2u32.to_le_bytes().to_vec();
    assert!(inputs.len() < 253);
    bytes.push(inputs.len() as u8);
    for (txid, vout) in inputs {
        bytes.extend_from_slice(txid);
        bytes.extend_from_slice(&vout.to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    }
    bytes.push(outputs);
    for _ in 0..outputs {
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&[1, 0x51]);
    }
    bytes.extend_from_slice(&tag.to_le_bytes());
    bytes
}

fn hash(row: &[u8]) -> [u8; 32] {
    inspect_transaction(row).unwrap().txid
}

#[test]
fn dependency_fanout_join_waits_for_both_parents_and_keeps_source_order_within_wave() {
    let coinbase = transaction(&[([0; 32], u32::MAX)], 1, 0);
    let parent = transaction(&[([11; 32], 0)], 2, 1);
    let left = transaction(&[(hash(&parent), 0)], 1, 2);
    let right = transaction(&[(hash(&parent), 1)], 1, 3);
    let join = transaction(&[(hash(&left), 0), (hash(&right), 0)], 1, 4);
    let independent = transaction(&[([22; 32], 0)], 1, 5);
    let bytes = block(&[coinbase, parent, left, right, join, independent]);
    let view = UnverifiedRawBlockView::parse(&bytes, None).unwrap();
    for workers in [1, 2, 4, 16] {
        let scanned = view
            .inspect_parallel(ParallelIndexConfig {
                workers,
                window_transactions: 2,
            })
            .unwrap();
        let plan = view.plan_dependency_waves(&scanned).unwrap();
        assert_eq!(plan.waves(), &[vec![0, 1, 5], vec![2, 3], vec![4]]);
        assert_eq!(plan.wave_by_transaction(), &[0, 0, 1, 1, 2, 0]);
        assert_eq!(plan.same_block_edges(), 4);
        assert_eq!(plan.unverified_external_prevouts(), 2);
    }
}

#[test]
fn duplicate_prevout_inside_one_transaction_is_not_hidden_by_dependency_grouping() {
    let coinbase = transaction(&[([0; 32], u32::MAX)], 1, 0);
    let repeated = transaction(&[([11; 32], 0), ([11; 32], 0)], 1, 1);
    let bytes = block(&[coinbase, repeated]);
    let view = UnverifiedRawBlockView::parse(&bytes, None).unwrap();
    assert_eq!(
        view.plan_dependency_waves(&view.inspect_serial().unwrap()),
        Err(DependencyAuditError::DuplicateInputSpend {
            first: 1,
            second: 1
        })
    );
}

#[test]
fn equal_transaction_count_and_merkle_root_do_not_bind_different_block_headers() {
    let bytes = fixture();
    let original = UnverifiedRawBlockView::parse(&bytes, None).unwrap();
    let scanned = original.inspect_serial().unwrap();
    let mut different = bytes.clone();
    different[76] ^= 1;
    let other = UnverifiedRawBlockView::parse(&different, None).unwrap();
    assert_eq!(original.transaction_count(), other.transaction_count());
    assert_eq!(
        original.header_merkle_root_raw(),
        other.header_merkle_root_raw()
    );
    assert_ne!(original.header_hash_raw(), other.header_hash_raw());
    assert_eq!(
        other.plan_dependency_waves(&scanned),
        Err(DependencyAuditError::MismatchedPreindex)
    );
}

#[test]
fn witness_only_preindex_alias_remains_a_dependency_hint_and_cannot_authorize_scripts() {
    let bytes = fixture();
    let original = UnverifiedRawBlockView::parse(&bytes, None).unwrap();
    let scanned = original.inspect_serial().unwrap();
    let mut rows: Vec<_> = (0..original.transaction_count())
        .map(|i| original.raw_transaction(i).unwrap().to_vec())
        .collect();
    // Fixture TX 2 ends in a one-element witness containing "abc", followed
    // by four locktime bytes. Change only the final witness payload byte.
    let witness_offset = rows[2].len() - 5;
    rows[2][witness_offset] ^= 1;
    let changed_bytes = block(&rows);
    let changed = UnverifiedRawBlockView::parse(&changed_bytes, None).unwrap();
    let changed_index = changed.inspect_serial().unwrap();
    assert_eq!(original.header_hash_raw(), changed.header_hash_raw());
    let before = &scanned.ordered_preindex().ordered()[2].observation;
    let after = &changed_index.ordered_preindex().ordered()[2].observation;
    assert_eq!(before.txid, after.txid);
    assert_ne!(before.wtxid, after.wtxid);
    // The DAG depends on non-witness prevouts. Existing binding deliberately
    // does not authenticate witness bytes or make this a script proof.
    let aliased = changed.plan_dependency_waves(&scanned).unwrap();
    assert_eq!(
        aliased,
        changed.plan_dependency_waves(&changed_index).unwrap()
    );
    assert_eq!(
        aliased.require_parent_utxo_snapshot(),
        Err(DependencyTrustError::ParentUtxoSnapshotUnavailable)
    );
    assert_eq!(
        aliased.require_verified_script_and_coinbase(),
        Err(DependencyTrustError::InputScriptsAndCoinbaseRulesUnverified)
    );
    assert_eq!(
        aliased.require_canonical_block_application(),
        Err(DependencyTrustError::CanonicalBlockStateTransitionUnavailable)
    );
}
