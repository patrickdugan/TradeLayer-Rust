//! Stage8C: synthetic raw-block preindex benchmark, NO Bitcoin consensus.
//! One complete raw block is scanned using borrowed slices; reference path
//! constructs one owned Vec per transaction and uses the same CPU workers.
use std::env;
use std::hint::black_box;
use std::time::{Duration, Instant};
use tradelayer_core::parallel_txindex::{inspect_unverified_block_parallel, ParallelIndexConfig};
use tradelayer_core::raw_block_ingest::{
    fetch_and_inspect_unverified_block, merkle_root_unverified, UnverifiedDependencyWaves,
    UnverifiedRawBlockIndex, UnverifiedRawBlockView,
};
use tradelayer_core::read_only::{decode_hex, inspect_transaction};

fn compact(buf: &mut Vec<u8>, number: usize) {
    if number < 253 {
        buf.push(number as u8);
    } else if number <= u16::MAX as usize {
        buf.push(0xfd);
        buf.extend_from_slice(&(number as u16).to_le_bytes());
    } else {
        buf.push(0xfe);
        buf.extend_from_slice(&(number as u32).to_le_bytes());
    }
}
fn raw_sample() -> Vec<u8> {
    decode_hex(include_str!("../../fixtures/raw_block_ingest_synthetic.hex").trim()).unwrap()
}
fn make_block(count: usize, witness_percent: usize) -> Vec<u8> {
    let raw = raw_sample();
    let view = UnverifiedRawBlockView::parse(&raw, None).unwrap();
    let coinbase = view.raw_transaction(0).unwrap().to_vec();
    let legacy = view.raw_transaction(1).unwrap();
    let witness = view.raw_transaction(2).unwrap();
    let mut transactions = Vec::with_capacity(count);
    transactions.push(coinbase);
    for i in 1..count {
        let uses_witness = (i * 37) % 100 < witness_percent;
        let mut tx = if uses_witness {
            witness.to_vec()
        } else {
            legacy.to_vec()
        };
        let prevout = if uses_witness { 7 } else { 5 };
        tx[prevout..prevout + 8].copy_from_slice(&(i as u64).to_le_bytes());
        transactions.push(tx);
    }
    let txids = transactions
        .iter()
        .map(|raw| inspect_transaction(raw).unwrap().txid)
        .collect::<Vec<_>>();
    let mut block = raw[..80].to_vec();
    block[36..68].copy_from_slice(&merkle_root_unverified(&txids).unwrap());
    compact(&mut block, count);
    for tx in transactions {
        block.extend_from_slice(&tx);
    }
    block
}
fn median(durations: &[Duration]) -> Duration {
    let mut durations = durations.to_vec();
    durations.sort_unstable();
    durations[durations.len() / 2]
}

#[derive(Clone, Copy)]
enum Phase {
    Owned,
    FullOwned,
    Prepared,
    OneTeamPrepared,
    Bulk,
    OneTeamFull,
    BoundaryParse,
    DependencyPlan,
}

const PHASES: [Phase; 8] = [
    Phase::Owned,
    Phase::FullOwned,
    Phase::Prepared,
    Phase::OneTeamPrepared,
    Phase::Bulk,
    Phase::OneTeamFull,
    Phase::BoundaryParse,
    Phase::DependencyPlan,
];

/// Equivalence checks and output destruction are outside every timer. Full
/// pipelines include their temporary raw buffers' allocation and destruction.
fn measure(
    phase: Phase,
    raw: &[u8],
    view: &UnverifiedRawBlockView<'_>,
    config: ParallelIndexConfig,
    expected: &UnverifiedRawBlockIndex,
    dependency: &UnverifiedDependencyWaves,
) -> Duration {
    let start = Instant::now();
    match phase {
        Phase::Owned => {
            let result = {
                let owned: Vec<_> = view
                    .transaction_ranges()
                    .iter()
                    .map(|range| raw[range.clone()].to_vec())
                    .collect();
                inspect_unverified_block_parallel(&owned, config).unwrap()
            };
            let elapsed = start.elapsed();
            assert_eq!(&result, expected.ordered_preindex());
            black_box(result);
            elapsed
        }
        Phase::FullOwned => {
            // Same acquisition clone, structural parse, header expectation,
            // hashing, ordered reduction and Merkle construction as full
            // borrowed paths, plus the owned per-TX copy and reparse.
            let (result, root) = {
                let full = raw.to_vec();
                let full_view =
                    UnverifiedRawBlockView::parse(&full, Some(view.header_hash_raw())).unwrap();
                let owned: Vec<_> = full_view
                    .transaction_ranges()
                    .iter()
                    .map(|range| full[range.clone()].to_vec())
                    .collect();
                let result = inspect_unverified_block_parallel(&owned, config).unwrap();
                let root = merkle_root_unverified(
                    &result
                        .ordered()
                        .iter()
                        .map(|row| row.observation.txid)
                        .collect::<Vec<_>>(),
                )
                .unwrap();
                (result, root)
            };
            let elapsed = start.elapsed();
            assert_eq!(root, view.header_merkle_root_raw());
            assert_eq!(&result, expected.ordered_preindex());
            black_box(result);
            elapsed
        }
        Phase::Prepared | Phase::OneTeamPrepared => {
            let result = match phase {
                Phase::Prepared => view.inspect_parallel(config),
                _ => view.inspect_parallel_one_team(config),
            }
            .unwrap();
            let elapsed = start.elapsed();
            assert_eq!(&result, expected);
            black_box(result);
            elapsed
        }
        Phase::Bulk | Phase::OneTeamFull => {
            let result = match phase {
                Phase::Bulk => {
                    fetch_and_inspect_unverified_block(view.header_hash_raw(), config, |_| {
                        Ok(raw.to_vec())
                    })
                    .unwrap()
                }
                _ => {
                    let full = raw.to_vec();
                    let full_view =
                        UnverifiedRawBlockView::parse(&full, Some(view.header_hash_raw())).unwrap();
                    full_view.inspect_parallel_one_team(config).unwrap()
                }
            };
            let elapsed = start.elapsed();
            assert_eq!(&result, expected);
            black_box(result);
            elapsed
        }
        Phase::BoundaryParse => {
            let parsed = UnverifiedRawBlockView::parse(raw, Some(view.header_hash_raw())).unwrap();
            let elapsed = start.elapsed();
            assert_eq!(parsed.transaction_ranges(), view.transaction_ranges());
            assert_eq!(parsed.block_weight(), view.block_weight());
            assert_eq!(parsed.header_hash_raw(), view.header_hash_raw());
            black_box(parsed);
            elapsed
        }
        Phase::DependencyPlan => {
            let planned = view.plan_dependency_waves(expected).unwrap();
            let elapsed = start.elapsed();
            assert_eq!(&planned, dependency);
            black_box(planned);
            elapsed
        }
    }
}

fn print_samples(label: &str, samples: &[Duration]) {
    let raw = samples
        .iter()
        .map(|sample| sample.as_nanos().to_string())
        .collect::<Vec<_>>()
        .join(",");
    println!("{label}_samples_ns={raw}");
    println!(
        "{label}_min_ns={}",
        samples.iter().min().unwrap().as_nanos()
    );
    println!(
        "{label}_max_ns={}",
        samples.iter().max().unwrap().as_nanos()
    );
}

fn main() {
    let args: Vec<_> = env::args().collect();
    if args.len() > 5 {
        panic!("usage: tradelayer-block-ingest-bench [tx_count:1..10000] [workers:1..16] [witness_percent:0..100] [repeats:1..50]");
    }
    let parse = |i: usize, default: usize| -> usize {
        args.get(i).map_or(default, |text| {
            text.parse().expect("valid integer argument")
        })
    };
    let count = parse(1, 4000);
    let workers = parse(2, 4);
    let witness_percent = parse(3, 50);
    let repeats = parse(4, 3);
    assert!((1..=10000).contains(&count));
    assert!((1..=16).contains(&workers));
    assert!(witness_percent <= 100);
    assert!((1..=50).contains(&repeats));
    let config = ParallelIndexConfig {
        workers,
        window_transactions: 512,
    };
    let raw = make_block(count, witness_percent);
    let view = UnverifiedRawBlockView::parse(&raw, None).expect("bounded raw block");
    let old_vec = view
        .transaction_ranges()
        .iter()
        .map(|span| raw[span.clone()].to_vec())
        .collect::<Vec<_>>();
    let old = inspect_unverified_block_parallel(&old_vec, config).unwrap();
    let new = view.inspect_parallel(config).unwrap();
    assert_eq!(old, *new.ordered_preindex());
    let dependency = view.plan_dependency_waves(&new).unwrap();
    // Warm every measured phase, including full clones/parses and planning.
    for phase in PHASES {
        black_box(measure(phase, &raw, &view, config, &new, &dependency));
    }
    let mut samples: [Vec<Duration>; PHASES.len()] = std::array::from_fn(|_| Vec::new());
    for repeat in 0..repeats {
        // Deterministic rotation gives every phase each slot over eight
        // repeats. Raw per-phase samples retain their chronological order.
        for offset in 0..PHASES.len() {
            let slot = (repeat + offset) % PHASES.len();
            samples[slot].push(measure(
                PHASES[slot],
                &raw,
                &view,
                config,
                &new,
                &dependency,
            ));
        }
    }
    let baseline_ms = median(&samples[0]).as_secs_f64() * 1000.0;
    let full_owned_ms = median(&samples[1]).as_secs_f64() * 1000.0;
    let prepared_ms = median(&samples[2]).as_secs_f64() * 1000.0;
    let one_team_ms = median(&samples[3]).as_secs_f64() * 1000.0;
    let bulk_ms = median(&samples[4]).as_secs_f64() * 1000.0;
    let one_team_full_ms = median(&samples[5]).as_secs_f64() * 1000.0;
    let boundary_parse_ms = median(&samples[6]).as_secs_f64() * 1000.0;
    let dependency_plan_ms = median(&samples[7]).as_secs_f64() * 1000.0;
    let bulk_seconds = (bulk_ms / 1000.0).max(1e-9);
    println!("benchmark=UNVERIFIED_SYNTHETIC_BULK_BLOCK_INGEST");
    println!("transaction_count={count}");
    println!("witness_percent={witness_percent}");
    println!("workers={workers}");
    println!("repeats={repeats}");
    println!("measurement_order=ROTATE_EIGHT_PHASES_BY_REPEAT");
    println!("warmup_runs_per_phase=1");
    println!("equivalence_checks=OUTSIDE_TIMERS");
    println!("prepared_phase_scope=PREPARSED_HASH_CARRIER_REDUCTION_TXID_MERKLE");
    println!("full_phase_scope=MOCK_CLONE_HEADER_BOUNDARIES_HASH_CARRIER_REDUCTION_TXID_MERKLE");
    println!("boundary_parse_scope=HEADER_HASH_BOUNDARIES_LAYOUTS_WEIGHT_NO_CLONE");
    println!("dependency_plan_scope=PREPARSED_PREINDEXED_UNVERIFIED_PREVOUT_GRAPH");
    println!("legacy_measured_ratio_scope=OWNED_TX_COPY_SCAN_OVER_FULL_BLOCK_WINDOWED");
    println!("raw_block_bytes={}", raw.len());
    println!("declared_weight={}", view.block_weight());
    println!("reference_owned_tx_vectors={count}");
    println!("new_raw_tx_byte_copies=0");
    println!("bulk_mock_rpc_calls_per_iteration=1");
    println!("legacy_per_tx_mock_rpc_calls_if_fetched_individually={count}");
    println!("owned_vector_parallel_median_ms={baseline_ms:.3}");
    println!("full_owned_block_pipeline_median_ms={full_owned_ms:.3}");
    println!("fused_prepared_only_median_ms={prepared_ms:.3}");
    println!("one_team_prepared_only_median_ms={one_team_ms:.3}");
    println!(
        "one_team_vs_windowed_ratio={:.3}",
        prepared_ms / one_team_ms.max(1e-9)
    );
    println!(
        "one_team_extra_result_slots={}",
        if workers > 1 { count } else { 0 }
    );
    println!("bulk_borrowed_parallel_median_ms={bulk_ms:.3}");
    println!("one_team_full_block_pipeline_median_ms={one_team_full_ms:.3}");
    println!("boundary_parse_median_ms={boundary_parse_ms:.3}");
    println!("dependency_plan_median_ms={dependency_plan_ms:.3}");
    println!("dependency_wave_count={}", dependency.waves().len());
    println!(
        "dependency_same_block_edges={}",
        dependency.same_block_edges()
    );
    println!(
        "dependency_unverified_external_prevouts={}",
        dependency.unverified_external_prevouts()
    );
    println!(
        "one_team_full_vs_windowed_ratio={:.3}",
        bulk_ms / one_team_full_ms.max(1e-9)
    );
    println!(
        "one_team_full_vs_owned_ratio={:.3}",
        full_owned_ms / one_team_full_ms.max(1e-9)
    );
    println!(
        "full_pipeline_speed_ratio={:.3}",
        full_owned_ms / bulk_ms.max(1e-9)
    );
    println!("measured_ratio={:.3}", baseline_ms / bulk_ms.max(1e-9));
    println!(
        "bulk_preindex_txs_per_sec={:.0}",
        count as f64 / bulk_seconds
    );
    println!("ordered_equivalence=PASS");
    for (label, values) in [
        "owned_vector_parallel",
        "full_owned_block_pipeline",
        "fused_prepared_only",
        "one_team_prepared_only",
        "bulk_borrowed_parallel",
        "one_team_full_block_pipeline",
        "boundary_parse",
        "dependency_plan",
    ]
    .into_iter()
    .zip(&samples)
    {
        print_samples(label, values);
    }
    println!("txid_merkle=LOCAL_CHECK_ONLY");
    println!("dependency_plan=STRUCTURAL_ONLY_EXTERNAL_UTXOS_AND_SCRIPTS_UNVERIFIED");
    println!("full_block_consensus=UNVERIFIED");
    println!("tradelayer_state=UNVERIFIED");
}
