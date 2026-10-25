# Throughput benchmark review

Reviewed 2026-10-26 on Windows 10.0.26200 with Rust 1.90.0 MSVC, release
optimization, and 20 available logical processors. Candidate checkout:
`C:\Users\patri\Documents\Codex\2026-10-26\go\work\review-pooled`, commit
`d4ba7592de3a93ee59ded6ebb2cd0cec91174368`, compared with main baseline
`c7a37eb90b1ea335051bfc3cb7712116b372cfb7`. This review concerns measurement
and reporting; it does not grant chain, script, UTXO, or economic validity.

The candidate's bounded pool was faster than the original per-window scheduler
and slower than the whole-block one-team scheduler in all sampled multicore
cases. Its possible benefit is a narrower temporary result-slot bound, which
requires memory measurement alongside throughput before selecting a default.

## Actionable findings

1. **P2: no full-pipeline pooled measurement.** The new pooled samples run on an
   already parsed view at
   [block benchmark:150](C:/Users/patri/Documents/Codex/2026-10-26/go/work/review-pooled/src/bin/tradelayer-block-ingest-bench.rs:150).
   The `bulk_borrowed_parallel_median_ms`, `full_pipeline_speed_ratio`, and
   `bulk_preindex_txs_per_sec` fields still invoke the original windowed path via
   [raw_block_ingest.rs:492](C:/Users/patri/Documents/Codex/2026-10-26/go/work/review-pooled/src/raw_block_ingest.rs:492).
   Add separately named full-pipeline measurements for each scheduler, each
   including the same block clone, header/boundary parse, hashing, ordered
   reduction, and Merkle verification. Keep these distinct from prepared views.

2. **P2: reference-only equality work is inside the timer.**
   [Block benchmark:136](C:/Users/patri/Documents/Codex/2026-10-26/go/work/review-pooled/src/bin/tradelayer-block-ingest-bench.rs:136)
   compares the entire owned report before recording elapsed time; prepared,
   one-team, pooled, and bulk samples record elapsed time before equivalence
   checks. Move elapsed-time capture before the equality checks, and retain
   every check outside all timed regions.

3. **P2: fixed order and limited sampling obscure scheduler effects.**
   [Block benchmark:104](C:/Users/patri/Documents/Codex/2026-10-26/go/work/review-pooled/src/bin/tradelayer-block-ingest-bench.rs:104)
   always executes owned, full-owned, windowed, one-team, pooled, then bulk.
   [Index benchmark:112](C:/Users/patri/Documents/Codex/2026-10-26/go/work/review-pooled/src/bin/tradelayer-index-bench.rs:112)
   always executes serial before parallel. The block benchmark caps repeats at
   five and reports only medians. Rotate/alternate order, warm each actual timed
   pipeline, accept more repeats, and print raw nanoseconds or min/max plus the
   median. Record host interference rather than treating a single ratio as a
   scheduler property.

4. **P2: uniform transactions do not test the weighted policy's motivation.**
   [Block generator:31](C:/Users/patri/Documents/Codex/2026-10-26/go/work/review-pooled/src/bin/tradelayer-block-ingest-bench.rs:31)
   repeats one legacy and one witness template, changing only eight prevout
   bytes. The large case is 826,563 raw bytes across 10,000 transactions.
   Add reproducible skewed transaction sizes, variable carrier density and
   witness sizes, and captured raw-block cases before claiming an advantage
   from weighting. The separate index benchmark varies padding by only six
   bytes and does not invoke pooled or one-team implementations at all.

5. **P3: configured slots are not measured total scratch memory.**
   [Block benchmark:193](C:/Users/patri/Documents/Codex/2026-10-26/go/work/review-pooled/src/bin/tradelayer-block-ingest-bench.rs:193)
   prints a window bound; it does not sample allocations or RSS. All schedules
   retain an output report and duplicate-TXID map, and the raw view wrapper also
   constructs prepared references and a TXID vector at
   [raw_block_ingest.rs:190](C:/Users/patri/Documents/Codex/2026-10-26/go/work/review-pooled/src/raw_block_ingest.rs:190).
   Keep the field labeled as a configured result-slot bound. Add peak RSS or
   allocation instrumentation before claiming an overall memory reduction.

## Scope of measured work

The prepared-view metrics include construction of prepared references, TXID and
wTXID hashing/carrier classification, ordered reduction, duplicate-TXID checks,
and TXID Merkle construction/checking. They exclude the initial structural
boundary parse and block acquisition. Thus they are broader than isolated worker
dispatch, while narrower than block ingestion.

The bulk measurement includes an in-memory clone in place of network RPC,
header/boundary parsing, the default windowed preindex, and Merkle verification.
Its mock call-count fields are declared structural counts; the benchmark does
not measure per-TX network requests or a real block RPC connection.

Neither executable calls `plan_dependency_waves`, performs script/UTXO
validation, authenticates the active chain, or applies canonical TradeLayer
state. The index executable measures already materialized transaction buffers
and does not construct/check a block Merkle root. Keep all results scoped to
these operations. Add a separately labeled dependency-planning phase rather
than silently incorporating it into one scheduler's timing.

## Measurements

Built the candidate using `cargo build --bins --release --frozen`. Ran block
cases sequentially with five internal repeats each; all reported ordered
equivalence PASS. Values are medians in milliseconds; ratios above one favor
the pooled path. This was a shared host without affinity, power-policy control,
or a confidence interval, so these are local observations.

| Transactions / workers / witness % | Windowed prepared | One-team prepared | Pooled prepared | One-team / pooled |
| --- | ---: | ---: | ---: | ---: |
| 1,000 / 1 / 0 | 4.708 | 4.306 | 4.263 | 1.010 |
| 1,000 / 4 / 50 | 4.440 | 2.123 | 3.683 | 0.577 |
| 4,000 / 4 / 50 | 15.099 | 8.065 | 8.702 | 0.927 |
| 10,000 / 8 / 95 | 49.793 | 18.248 | 25.389 | 0.719 |
| 10,000 / 16 / 95 | 55.508 | 16.252 | 22.711 | 0.716 |

The one-worker variants delegate to the same scanner; their differing medians
are direct evidence of measurement variability. Three immediately following
independent processes for the 10,000 / 8 / 95 case produced:

| Follow-up process | Windowed prepared | One-team prepared | Pooled prepared | One-team / pooled |
| --- | ---: | ---: | ---: | ---: |
| 1 | 22.972 | 11.499 | 15.114 | 0.761 |
| 2 | 23.212 | 11.660 | 15.133 | 0.771 |
| 3 | 23.010 | 11.846 | 15.897 | 0.745 |

The earlier process and these follow-ups differ materially in absolute time.
The observed relative ordering alone supports keeping the bounded pool opt-in;
it does not establish a production default or a fixed expected speedup.

The separate index benchmark, with seven repeats, window 512, and 512 bytes of
base padding, measured 14.398/14.297 ms serial/parallel at 4,000 / 1 / 50;
14.077/9.140 ms at 4,000 / 4 / 50; and 46.108/22.342 ms at 10,000 / 8 / 95.
These validate useful windowed CPU concurrency on these synthetic inputs, not
the pooled candidate or network/consensus throughput.

Raw local measurement output is retained in the task's `work` directory as
`pooled-review-block-matrix.txt`, `pooled-review-repeat-matrix.txt`, and
`pooled-review-index-matrix.txt`. It is scratch evidence, not an exported corpus.

## Follow-up implementation boundary

The parent authorized corrections to the current-main block benchmark only.
The completed correction captures time before every output equivalence check,
rotates eight phases, warms each actual pipeline once, prints raw nanosecond
samples and min/max values, and accepts 1–50 repeats. Existing fields and defaults
remain compatible. Full one-team ingestion includes the same block clone, header
expectation, boundary parse, hashing/reduction, and Merkle calculation as the
windowed bulk path. Separate boundary parse and dependency-plan phases keep their
scopes explicit. No measured phase authenticates external prevouts or scripts.
The separate pooled branch and index benchmark remain unchanged.
This document's source links and measured values refer to the reviewed candidate
commit above, before those corrections.

The later [barrier-pool review](TXINDEX_BARRIER_REVIEW.md) pins candidate
`9091bcd` separately and adds an isolated Merkle phase to local main. The current
benchmark therefore rotates nine phases; the eight-phase observations below
describe the earlier instrumentation.

The corrected release binary completed 1 transaction / 1 worker / 1 repeat;
1,000 / 4 / 50% witness / 8 repeats; and 1 / 1 / 50 repeats. Every case passed
full ordered equivalence, and a shell check verified exactly eight sample fields
with exactly the requested repeat count in each. The 1,000-transaction smoke case
reported full-owned 4.579 ms, full-windowed 4.994 ms, full-one-team 3.113 ms,
boundary parse 0.208 ms, and dependency plan 1.161 ms. These are smoke observations
on the shared host, not a replacement controlled comparison. Its dependency plan
had one wave and zero same-block edges, exposing the current generator's limited
graph workload. Output is retained in `work/benchmark-corrected-smoke.txt`.
