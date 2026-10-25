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
modules, excluding the experimental README. It records the ten assessed
modules below; the other 39 retain their prior implementation assessment.
The insurance row additionally records its external-worker assignment.
It supplements the historical [full-tree inventory](JS_RUST_PORT_AUDIT.md),
whose earlier mappings and assessment commit are preserved.

[SUPPORTING_FUNCTION_PARITY.csv](SUPPORTING_FUNCTION_PARITY.csv) records 148
source entries with their tested domains, Rust tests and remaining criteria.
None of these rows or modules is marked as a complete production port.

| Module | Implemented and verified scope | Remaining completion criteria |
| --- | --- | --- |
| `base94.js` | Eight helper methods, bounded string inputs, real-source golden vectors and preserved delimiter/fraction quirks | General JS coercions and malformed UTF-16; accepted historical wire transactions and admission integration |
| `base256.js` | Six helper methods, exact custom Unicode alphabet, bounded arbitrary-size arithmetic and source hex quirks | General JS coercions and malformed UTF-16; accepted historical transactions and wire integration |
| `tradeHistoryManager.js` | Seven query predicates, typed token/contract records, stable identities, atomic batches, canonical CRC checkpoint codec and rollback | Historical DB migration/order, positional/PnL/LIFO APIs, durable production persistence and chain-anchored replay |
| `volumeIndex.js` | Exact bounded VWAP, half-up prices, constant-share fee rounding, typed inclusive totals, source reverse-pair/classification characterization and rollback | Legacy DB normalization, valuation and historical prices, logarithmic attenuation, production persistence and reward issuance |
| `iou.js` | Stateful buckets/claims and 67 source transitions; real-loss escrow and funded payouts; explicit atom policy; complete bounded IOU/ledger checkpoints | Authenticated matching/clearing integration and historical atom/dust rules, production DB behavior, broader JS coercion/prototype domain and historical acceptance |
| `options.js` | All nine helper methods, decimal PnL/pricing/exposure/maintenance, ticker and numeric-prefix behavior, source greedy order; 118 actual-source vectors | Surrounding contract creation/exercise/assignment and margin/clearing integration; cross-platform binary64 bridge verification; historical acceptance |
| `vaults.js` | Typed vault create/update/redeem, synthetic links, collateral interfaces and aggregate queries; 9 lifecycle seeds, 13 source queries and 30 characterized failures/quirks | Deliberate typed repairs differ from broken source paths; ledger mint/redeem, hedge lifecycle, durable persistence and funding/rebase specification remain open |
| `contractRegistry.js` | Typed definitions and expiry/catalog queries, native/oracle notional/margin, AMM construction, funding journal and atomic collateral plans; all 40 distinct methods assessed; 219 source cases | Sole-ledger/channel/margin execution, broken oracle pricing/funding dependencies, AMM hydration, durability and historical acceptance |
| `oracle.js` | Metadata/admin lifecycle, price history/circuit/TWAP, stake/fraud metadata and relay replay; 246 source vectors plus 17 quirks | Funded staking/slashing and insurance closure, authenticated prices/clearlist/DLC proofs, durable state and historical acceptance |
| `amm.js` | Capital/share lifecycle and journals, quotes, inventory/curve/token intents, local registry and options adapter; all 25 declarations assessed; 275 source rows | Funded LP issuance/secondary reserves, safe-price execution, AMM-first orderbook/clearing, durability and historical acceptance |

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

The third pass adds the [contract catalog](CONTRACT_REGISTRY.md),
[oracle history](ORACLE.md) and [AMM lifecycle](AMM.md). Actual-source oracles
keep source failures distinct from implemented local behaviors. Independent
review corrected native-inverse invalid-mark metadata Number conversion and
empty AMM percentage positive-zero behavior, with new source vectors.

The [funded IOU adapter](FUNDED_IOU.md) owns one ledger and commits collected
losses, source claims, actual escrow payouts and replay IDs atomically. Its
atom policy is explicit and is not a historical tally rule. A review regression
also prevents per-claim rounding from expanding the source current loss pool.
[Combined checkpoints](IOU_CHECKPOINTS.md) preserve the full ledger, IOU
counters/claim order, escrows and replay state across local restart/fork restore.

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
  executable now rotates nine phases, warms each one, prints raw timing
  samples, and reports full one-team ingestion, structural parsing and
  dependency planning separately. Pooled candidate measurements remain scoped
  to their reviewed revision. No default scheduler is changed.
- [Barrier-pool follow-up](TXINDEX_BARRIER_REVIEW.md): independently reviews
  candidate `9091bcd`, reproduces a partial-startup barrier deadlock, confirms
  its same-claim error/panic parity, and records remaining benchmark issues.
  A separate local-main Merkle phase exposes serial work inside ingestion.

Synthetic timings measure local parsing/hashing/indexing work. They exclude
real RPC, authenticated parent UTXOs, script verification, canonical chain
selection and serial TradeLayer economic application. The candidate pool's
temporary 512-observation bound is not a measurement of total resident memory.

The external ChatGPT worker owns subsequent tx-index/throughput work.
[PR #141](https://github.com/patrickdugan/TradeLayer-Rust/pull/141), read at
`8cb31416adf36dc7eefa53a396a40659eb3a6668`, is draft and unmerged. It succeeds
the exploratory #139 and reports a narrower HashMap-only candidate with a
process-separated real-block CPU/RSS harness and a mainnet block-1 smoke test.
Representative Bitcoin Core block CPU runs and target-host peak-memory evidence
remain promotion gates; block 1 has only one transaction.
Those measurements and any promotion belong to that worker's lane. This local
economic-module pass does not merge or select the performance candidates.

## Next independent work

The external ChatGPT worker also owns the `insurance.js` port to
`src/insurance.rs`, with `tests/insurance.rs`, its source oracle/fixtures and
`docs/INSURANCE.md`. The local agents must leave those files to that worker.
No insurance implementation or validation is claimed in this local batch.

Connect contract collateral plans, oracle marks, AMM journals, vaults and options
to complete matching/clearing and sole-ledger lifecycles. The funded IOU lane
now has real transfers and local restart recovery; authenticated source matches,
historical atom/dust rules and full clearing integration remain open. Source
payout-pool reuse, contradictory vault persistence and broken contract funding
cannot silently become corrected economic consensus. Complete funded LP and
secondary-reserve accounting, insurance/ADL, option exercise/assignment, durable
production storage and historical replay against the pinned source.
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

## Local validation for catalogs, oracle, AMM and funded IOUs

Pinned Rust 1.90.0 on Windows/MSVC passed all 633 tests across 86 executables,
formatting including the new included IOU codecs, all-target Clippy with
warnings denied, the frozen all-target release build and doc tests (none
defined). The dependency verifier and all 11 adversarial policy tests passed.
These are local checks; no branch was pushed and no CI run, Lean/Kani proof
or historical acceptance result is claimed for this local head.

The actual-source oracles passed 219 contract-registry cases, 246 oracle
lifecycle vectors plus 17 oracle quirks, and 275 AMM rows. Both archived and
external pinned-source modes passed. The existing decimal, IOU, options,
vault and volume source checks also passed unchanged.

Independent review corrected two source-parity errors and added source-derived
regressions. The funded IOU suite includes 13 real-ledger integration tests,
nine restart/canonical-codec tests, and two additional capacity regressions.
The latter exercise a valid standalone claim map exceeding the 64 MiB encoding
cap and a real 100,000-ID ledger journal boundary. Funded construction and
mutations now require successful complete serialization, rejecting an
uncheckpointable import or rolling back a transition that crosses a bound.
The standalone arithmetic store still has a separate serialization limit.
This closes a local recovery gap; production durability and authenticated
economic execution remain open.
