# Roadmap progress and parallel-worker handoff

Assessment date: 26 October 2026. Reference JavaScript is
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`; this checkout started from Rust
main `c7a37eb90b1ea335051bfc3cb7712116b372cfb7`.

The user assigned authenticated parent-UTXO snapshots and real script
verification to a separate worker. This pass implements independent supporting
modules and reviews transaction-index scheduling and parsing. It does not merge
that worker's branches, publish a branch, or authorize economic transitions.

## Tracking the 49 source modules

[PARITY_ROADMAP.csv](PARITY_ROADMAP.csv) contains all 49 JavaScript source
modules, excluding the experimental README. It records the seven assessed
modules below and explicitly leaves the other 42 at their prior audit status.
It supplements the historical [full-tree inventory](JS_RUST_PORT_AUDIT.md),
whose earlier mappings and assessment commit are preserved.

[SUPPORTING_FUNCTION_PARITY.csv](SUPPORTING_FUNCTION_PARITY.csv) records 63
source functions with their tested domains, Rust tests and remaining criteria.
None of these rows or modules is marked as a complete production port.

| Module | Implemented and verified scope | Remaining completion criteria |
| --- | --- | --- |
| `base94.js` | Eight helper methods, bounded string inputs, real-source golden vectors and preserved delimiter/fraction quirks | General JS coercions and malformed UTF-16; accepted historical wire transactions and admission integration |
| `base256.js` | Six helper methods, exact custom Unicode alphabet, bounded arbitrary-size arithmetic and source hex quirks | General JS coercions and malformed UTF-16; accepted historical transactions and wire integration |
| `tradeHistoryManager.js` | Seven query predicates, typed token/contract records, stable identities, atomic batches, canonical CRC checkpoint codec and rollback | Historical DB migration/order, positional/PnL/LIFO APIs, durable production persistence and chain-anchored replay |
| `volumeIndex.js` | Exact bounded VWAP, half-up prices, constant-share fee rounding, typed inclusive totals, source reverse-pair/classification characterization and rollback | Legacy DB normalization, valuation and historical prices, logarithmic attenuation, production persistence and reward issuance |
| `iou.js` | Stateful buckets and claims, source 20-place division/persistence rounding, claimant order, payout gates/reduction, totals and deletion; 67 actual-source transitions | Funded redemption and ledger/replay integration, production DB behavior, broader JS coercion/prototype domain and historical acceptance |
| `options.js` | All nine helper methods, decimal PnL/pricing/exposure/maintenance, ticker and numeric-prefix behavior, source greedy order; 118 actual-source vectors | Surrounding contract creation/exercise/assignment and margin/clearing integration; cross-platform binary64 bridge verification; historical acceptance |
| `vaults.js` | Typed vault create/update/redeem, synthetic links, collateral interfaces and aggregate queries; 9 lifecycle seeds, 13 source queries and 30 characterized failures/quirks | Deliberate typed repairs differ from broken source paths; ledger mint/redeem, hedge lifecycle, durable persistence and funding/rebase specification remain open |

The combined [offline spot reporting adapter](OFFLINE_SPOT_REPORTING.md)
inserts existing spot fills into both indexes atomically, rejects network
mixing and duplicate observations, and rolls both back together. Addresses and
LTC valuation are explicit caller metadata. It retains exact traded amounts
and labels the volume price's eight-decimal rounding.

The second implementation pass adds shared [finite decimal arithmetic](DECIMAL.md)
with checked arbitrary coefficients and explicit rounding, used by the new
[IOU](IOU.md), [options](OPTIONS.md) and [vault](VAULTS.md) modules. It removes
machine-integer precision barriers for normal finite economic calculations.
The shared engine is compared against 1,942 real BigNumber vectors and separate
resource-limit/sign/ordering adversaries. Economic source oracles now run from
complete archived, hash-pinned JavaScript and BigNumber code without an external
legacy checkout or npm installation. They are included in CI.

## Transaction throughput review

- [Parser review](TX_PARSER_REVIEW.md): found and fixed an inherited witness
  presence bug. A nonempty witness stack containing an empty item must be
  accepted for syntax inspection. Independent Python serialization/hashlib
  vectors exercise both standalone and fused/prepared paths, including 253
  inputs/outputs and nonminimal CompactSize encodings.
- [Scheduler review](TXINDEX_SCHEDULER_REVIEW.md): examines the newer pooled
  candidate `d4ba7592de3a93ee59ded6ebb2cd0cec91174368` in an isolated checkout.
  Reusable fault injection checks error ordering, panic shutdown, duplicates,
  and multiple windows. It also records the witness-only preindex binding
  boundary and the same-claim error/panic difference found in final review.
- [Benchmark review](THROUGHPUT_BENCHMARK_REVIEW.md): identifies unequal timer
  boundaries, missing full-pipeline scheduler comparisons, fixed measurement
  order, limited sampling and uniform synthetic inputs. The current-main
  executable now rotates eight phases, warms each one, prints raw timing
  samples, and reports full one-team ingestion, structural parsing and
  dependency planning separately. Pooled candidate measurements remain scoped
  to their reviewed revision. No default scheduler is changed.

Synthetic timings measure local parsing/hashing/indexing work. They exclude
real RPC, authenticated parent UTXOs, script verification, canonical chain
selection and serial TradeLayer economic application. The candidate pool's
temporary 512-observation bound is not a measurement of total resident memory.

## Next independent work

Connect the standalone IOU/vault state and options calculations to complete
contract, clearing and ledger lifecycles, with explicit funded-transfer and
replay rules. Source payout-pool reuse and contradictory vault persistence
cannot silently become corrected economic consensus. Complete the remaining
contract-registry, oracle, AMM/insurance/ADL and option-exercise/assignment paths
against the pinned source and historical evidence.
Keep the separate worker's UTXO/script work and the Stage 25P/Q/R admission and
signed-channel gates visible as outstanding dependencies. Subsequent source
functions should receive individual test and historical-vector criteria before
their module status can advance.

## Local validation for the first supporting-module pass

Pinned Rust 1.90.0 on Windows/MSVC passed formatting (including separately
included implementation files), Clippy with warnings denied, all 546 Rust
tests across 74 test executables, doc tests, and the frozen release build of
all targets. The supply-chain verifier and its 11 adversarial tests passed.
Independent checks passed 1,221 Base94/Base256 golden rows, 24 source-executed
history queries plus the additional typed-query vector, 15 source-executed
volume/fee vectors, and 104 independently constructed parser vectors.

The isolated pooled candidate passed its 22 selected Rust integration tests.
The scheduler fault harness passed eight characterization tests against the
original source and eight against the explicitly proposed temporary patch.
The original mode includes the known same-claim panic-precedence divergence;
its passing result must not be read as evidence of complete error parity.
The corrected benchmark passed small, eight-repeat and 50-repeat smoke runs
with ordered equivalence and the requested timing sample counts. These are
local results; GitHub CI and separate Lean/Kani verifiers were not run here.

## Local validation for the decimal and economic-module pass

Pinned Rust 1.90.0 on Windows/MSVC passed all 582 tests across 79 executables,
plus the independent same-claimant IOU rounding regression (583 tests across
80 executables in total). Formatting, Clippy with warnings denied, doc tests
(none defined), and the frozen all-target release build passed. The first test
attempt encountered two linker file-access failures; the complete rerun with
two build jobs and a workspace temporary directory passed without source changes.

The archived-source oracles passed 1,942 decimal vectors, 67 IOU transitions,
118 options vectors, 9 vault lifecycle/seed operations, 13 vault aggregate
queries, 30 vault source quirks, and the existing 15 volume/fee vectors.
The three new module oracles also passed against the actual pinned JS checkout.
The decimal adversarial review found no arithmetic defect within the documented
bounds; the economic review found no parity or atomicity defect within the
documented domains. Functional repairs to broken vault source paths remain
explicit divergences, not historical acceptance claims.
