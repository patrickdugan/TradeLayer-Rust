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
modules, excluding the experimental README. It records the twenty assessed
modules below; the other 29 retain their prior implementation assessment.
The insurance and property rows additionally record external-worker assignments.
It supplements the historical [full-tree inventory](JS_RUST_PORT_AUDIT.md),
whose earlier mappings and assessment commit are preserved.

[SUPPORTING_FUNCTION_PARITY.csv](SUPPORTING_FUNCTION_PARITY.csv) records 449
source entries with their tested domains, Rust tests and remaining criteria.
None of these rows or modules is marked as a complete production port.

| Module | Implemented and verified scope | Remaining completion criteria |
| --- | --- | --- |
| `clearing.js` | Constructor and all 79 active methods assessed; source funding and mark arithmetic/quirks; funded fractional oracle-perpetual epochs, exact settled-basis marks and nominal default offsets in the existing Ledger with complete restart and rollback | Actual liquidation/AMM/ADL and insurance, native VWAP, full contract orchestration, complete admitted-publication history, authenticated marks/actors and historical acceptance |
| `orderbook.js` | All 69 methods, constructor and two comparators assessed; 287 actual-source vectors and nine source defects; contract book, funded type18/6 matching, owner cancellation and complete recovery in the existing joint Ledger | Authenticated orders and contract configuration, funded AMM-first execution, liquidation/ADL, multicontract orchestration, channels, source reporting/rewards and historical acceptance |
| `tally.js` | All 45 methods plus constructor assessed; 248 actual-source vectors and 32 source assertions; actual custody projections, funded match fee/rebate collection and complete FeeBook persistence | Legacy arbitrary-value/NeDB migration, authenticated channel/vesting rules, registered insurance lifecycle, buybacks, remaining shared economic stores and historical acceptance |
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
| `marginMap.js` | Positions, signed margin/PnL, option protection, liquidation/ADL plans and first mint; 246 source vectors plus 10 characterizations; constructor, 2 static and 43 active instance methods assessed | Authenticated funded collateral and clearing integration, broken source dynamic orchestration/redemption, durable position/delta adapters and historical acceptance |
| `vesting.js` | All 22 source members assessed; initialization/failure journals, chain retries, rebates and numerical helpers; 151 actual-source cases | Authorized genesis, supply discrepancy decisions, funded backend and durable partial-initialization recovery; historical acceptance |
| `procedural.js` | All nine methods assessed; local template/contract metadata, source state quirks and context rejection order; 154 actual-source operations | Authenticated proof/relay and settlement verification, funded issuance/redemption, durable metadata migration and historical acceptance |
| `db.js` | Source 32 categories and seven folder/bootstrap cases; bounded exclusively locked disk journal with atomic synced batches, explicit torn-tail recovery and complete funded IOU restart | NeDB query/shape migration, compaction, trusted rollback detection, broader state adapters and deployment power-loss validation including Windows directory entries |
| `persistence.js` | All 17 methods plus constructor assessed; 41 source observations; atomic persisted joint apply/rollback, bounded headers, independently expected configuration/tip, lost-acknowledgement and torn-write recovery | NeDB migration, unified state across all economic modules, trusted branch/tip evidence, compaction and deployment power-loss validation |
| `reOrg.js` | All three source methods, exact tip/RPC/log order and swallowed-failure behavior; 20 actual-source scenarios | Production bounded RPC transport and authenticated active-branch evidence; the source observer performs no economic rollback |
| `init.js` | Both literal source functions and 12 source cases; repaired owned startup plus concrete complete joint/IOU restoration, identity consistency and failure cleanup | Live client/daemon and shutdown, cooperative I/O timeouts, testnet profiles, source environment migration and authenticated readiness |

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

The fourth pass adds [margin positions](MARGIN_MAP.md), [vesting journals](VESTING.md),
[procedural metadata](PROCEDURAL.md) and a [disk journal](STORAGE.md). Independent
cross-review corrected source Number conversions, malformed option strikes and
repaired IOU dust allocation. Storage review corrected allocation preflight and
the new-file locking race; it also added explicit Unix directory syncs. Real
disk restart retains the complete funded ledger, escrow ownership, policy and
replay fences. These changes do not establish complete economic execution,
NeDB migration or power-loss certification on the intended host.

The fifth pass adds a [transactional funded IOU owner](DURABLE_IOU.md),
[persistent joint application and rollback](PERSISTENCE.md),
[startup coordination and concrete restore adapters](BOOTSTRAP.md), and the
exact [source reorg observer](REORG.md). Complete candidate state is synced
before publication. Uncertain writes deny cached reads until reopening and
reconciling replay IDs or an independently expected tip. Aggregate payout
receipt bounds prevent repeated dust payments from retaining millions of rows.

The [joint-history floor fix](JOINT_HISTORY_FLOOR.md) preserves all TLJC v1
checks and ordinary bytes. A narrowly selected v2 represents a valid empty
rollback history above the original anchor, with explicit floor equal to its
current tip. This lets the oldest retained state survive restart without
inventing an older ancestor or silently changing the configured anchor.

The sixth pass adds [atomic contract execution](ECONOMIC.md) to the existing
Joint-owned Ledger. The contract book, positions, source PnL, actual order
reserves, margin, collected losses, IOU claims, fees, receipts and replay state
commit together. Partial-fill/self-cross release ordering was corrected during
independent review. Maker rebates require actual fee collection, and IOU
payouts stop at actual remaining pool funds. `ConservingV1` explicitly names
these repairs and its sequential allocation policy; it does not promote them
to historical consensus rules.

TLEC/TLEH checkpoints preserve this complete runtime subset and every retained
before-state. Durable restart requires an independently expected sequence,
tip and full-state commitment. Rollback restores balances and all economic
metadata together, including an empty history at an evicted floor. The decoder
rejects an unreachable positive-height genesis floor even with a repaired CRC.
The standalone funded IOU and ClearingSandbox owners remain separate
experiments; they must not become additional live Ledgers alongside the new
owner. Oracle, AMM, vault, vesting, procedural and full clearing state still
need coherent integration before claiming complete economic recovery.

The external insurance worker reports local commit
`e9995caf83ca40db644211d9fec60b2acfacd12e` and six reserved files. Its actual
archive or patch is not yet present in this checkout. Rust verification,
genuine BigNumber 9.1.2 oracle execution, import and sole-Ledger insurance
lifecycle integration remain pending. Pending insurance fees in the new
runtime are funded escrow custody, not a registered insurance lifecycle.

The external worker also owns the six new property-port paths (`property.rs`,
its tests/oracle/source/vector fixtures and `PROPERTY.md`). Its separate task
covers the seven property kinds, catalog/circulation/admin metadata, source
vectors, complete restoration and plans reconciled to the existing Ledger.
Local funding and recovery work leaves those paths untouched. Assignment is not
an implementation or verification claim.

The seventh pass adds [funded periodic epochs](ECONOMIC_FUNDING.md) without a
second Ledger. Every eligible 24-block epoch completes even on an empty block,
with explicit outcomes for absent, stale or closed oracles and zero payment.
Source mean, rate and collateral-notional arithmetic are separated from the
named conserving policy for fractional payer charges and exact receiver
allocation. All payer debits precede receiver credits from a real transient
escrow; insufficient available funds reject the whole staged block.

Dedicated system-epoch IDs never masquerade as external type18 inputs or
authorize wire23/31. TLJE/TLEC v2 preserve bounded price/closure history,
captured positions, plans and epoch replay together. Representable v1 bytes
remain unchanged. Restored windows are reconciled across retained history,
and an epoch at the tip must retain the current nonflat contracts. The owner
uses the existing Registry's last accepted quote per oracle/height; high-level
OracleList first-update-wins admission remains a separate historical gate.

Fee receipt validation now uses one private replay builder, preserving exact
FeeBook rows, complete journal order/fields and insurance totals. This removes
growing per-receipt journal copies; end-to-end economic-owner throughput and
representative memory/storage measurements remain open.

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
secondary-reserve accounting, funded insurance/ADL, option exercise/assignment, durable
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

## Local validation for positions, vesting, procedural state and disk restart

Pinned Rust 1.90.0 on Windows/MSVC passed 677 tests across 91 executables,
formatting including included implementations, all-target Clippy with warnings
denied, the frozen all-target release build, and the dependency gate with its
11 adversarial policy tests. Doc tests
passed with none defined. Both archived and external pinned-source oracle modes
passed 246 margin vectors plus 10 characterizations, 151 vesting cases, 154
procedural operations, and 32 database categories plus seven layout/bootstrap
scenarios. All four staged JavaScript archives retained their exact SHA256 bytes.

Disk tests cover lock exclusion across processes, every transaction truncation
point, single-byte corruption, crafted valid-CRC malformed records, bounded
rejection, real write-error poisoning and funded IOU replay-safe restart. Unix
directory syncs are implemented but this local run used Windows; deployment
power-loss testing, Windows directory-entry durability and NeDB migration
remain open. No GitHub CI or historical-chain acceptance is claimed here.

## Local validation for transactional recovery and startup

Pinned Rust 1.90.0 on Windows/MSVC passed all 733 tests across 98 executables,
formatting including included implementations, all-target Clippy with warnings
denied, the frozen all-target release build, and the dependency gate plus its
11 adversarial tests. Doc tests passed with none defined. The three new source
oracles passed both archived and external pinned-checkout modes: 12 startup
cases, 20 reorg scenarios and 41 persistence observations, including 26 integer
diagnostic vectors checked by Rust. Staged source archives retained exact hashes.

Coverage includes synced and torn-write failures for both applying and rolling
back blocks, denial of poisoned cached reads, real startup into both durable
owners, exact expected-state checks, full funded payout/replay restoration,
aggregate dust-receipt limits, and canonical oldest-history floor recovery.
All 16 existing joint-chain tests also pass with v1 encoding/validation retained.
These checks do not establish production daemon operation, authenticated inputs,
unified recovery across all economic modules, deployment power-loss guarantees
or historical acceptance. No branch was pushed and no GitHub CI result is claimed.

## Local validation for atomic contract execution and recovery

Pinned Rust 1.90.0 on Windows/MSVC passed all 786 tests across 104 executables,
formatting including every included implementation, all-target Clippy with
warnings denied, and the frozen all-target release build. The zero-dependency
gate and its 11 adversarial tests passed. Doc tests passed with none defined.
The new source oracles passed 287 orderbook and 248 tally vectors, plus nine
orderbook defects and 32 tally behavior assertions. Both complete JavaScript
archives match the clean pinned source checkout byte-for-byte after LF
normalization and retain their declared SHA256 hashes.

Coverage includes fractional position open/increase/reduce/close/flip,
actually funded maker rebates and deferred IOU payouts, owner cancellation,
late-leg failure/retry, shared replay fences, partial-fill/self-cross reserve
release and complete state restart on all three networks. Corruption tests
repair nested CRCs before checking orphan custody, fee-journal route/height
forgery and unreachable history-floor rejection. Durable tests exercise
retention eviction, oldest-floor rollback/rebranching, exact freshness tokens,
explicit torn-tail recovery and a second-process OS lock. Existing clearing
migration error behavior and legacy Joint checkpoint bytes remain compatible.

The explicit ConservingV1 policy, pending insurance custody and remaining
performance/integration gates are documented in [ECONOMIC.md](ECONOMIC.md).
These checks do not establish historical acceptance or authenticated settlement.
No branch was pushed and no GitHub CI, Lean or Kani result is claimed.

## Local validation for funded perpetual funding epochs

Pinned Rust 1.90.0 on Windows/MSVC passed all 820 tests across 107 executables,
formatting including every included implementation, all-target Clippy with
warnings denied, and the frozen all-target release build. The zero-dependency
gate and its 11 adversarial tests passed. Doc tests passed with none defined.
The unchanged-source funding oracle passed 117 average, rate, notional and
payer vectors plus 19 behavior characterizations in both archived-source and
external pinned-checkout modes. The complete clearing archive retains its
declared SHA256 and matches the pinned source after LF normalization.

Coverage includes positive and negative fractional funding, funded conservation,
empty-block cadence, same-height final-accepted observations, stale/closed/missing
prices, zero-leg replay fences, failed-block rollback and restart on all three
networks. Rechecksummed corruption tests reject inconsistent epoch windows,
outdated as-of indices, closure metadata, position captures, plans and aggregate
allocation bounds. Existing v1 checkpoint fingerprints remain unchanged when
funding is disabled. Fee journal reconstruction preserves exact complete state
while avoiding repeated full-journal clones; helper measurements are exploratory
and do not establish end-to-end throughput or peak-memory improvements.

Funding follows the explicit conserving policy in [ECONOMIC_FUNDING.md](ECONOMIC_FUNDING.md).
Registry duplicate-publication admission differs from the source OracleList's
first-attempt guard and is documented rather than claimed as parity. Authenticated
execution, full publication provenance, settled mark basis, liquidation,
insurance lifecycle and historical acceptance remain open. No branch was pushed
and no GitHub CI, Lean or Kani result is claimed.

## Exact settled basis and funded marks

The eighth local economic pass adds [exact rational basis](MARK_BASIS.md) and
[funded oracle mark settlement](ECONOMIC_MARKS.md) to the same Ledger owner.
Marks advance settlement basis without replacing historical trade averages;
closes pay only the remaining change. Inverse additions use reciprocal-price
exposure and signed subatom carry survives partial closes, flips and flat/reopen.

Fresh final-accepted same-height oracle publications trigger one system epoch
once type18 was active in a prior block. All matching precedes the mark; funding
follows it. Negative PnL first cancels that owner's unpaid positive claims;
positive PnL first offsets that owner's unpaid losses. Only remaining obligations
can collect or create cash claims, and payouts remain limited by actual custody.
A price round trip after default therefore cannot retain reversed nominal claims.
Current below-half-initial-margin calls project actual margin, without executing
liquidation or treating a wallet top-up as margin custody.

The bounded action journal independently recomputes every basis/carry transition,
trade average, realized amount, complete IOU state, current default liability,
maintenance call and system replay union. Historical funding quantities are also
bound to reconstructed positions, and overlapping same-oracle funding/mark price
captures must agree. These consistency checks do not authenticate inputs or prove
complete publication history. TLEC v3 persists all mark fields and owner offsets;
marking disabled retains the prior v1/v2 formats. Full liquidation, funded AMM,
insurance lifecycle, chain authorization and historical acceptance remain open.

## Local validation for settled basis and funded marks

Pinned Rust 1.90.0 on Windows/MSVC passed all 845 tests across 110 executables,
formatting including every included implementation, all-target Clippy with
warnings denied, and the frozen all-target release build. The zero-dependency
gate and its 11 adversarial tests passed. Doc tests passed with none defined.
The unchanged-source mark oracle passed 130 vectors and nine characterized
quirks; the funding oracle retained its 117 vectors and 19 characterizations.
Both complete clearing and margin-map archives match the pinned JavaScript
checkout byte-for-byte after LF normalization and retain their declared hashes.

Coverage includes linear/inverse adds, partial closes, flips and close after a
paid mark, signed rational carry through flat/reopen, loss/claim offsets after
default and price reversal, final same-height publications, maintenance deficits,
late funding failure and full rollback. Repaired-checksum probes reject forged
basis/carry/actions/offsets, noncanonical ratios and negative zero, excessive
counts, historical funding quantities inconsistent with trade replay and mark
observations inconsistent with known funding history. Full checkpoint and disk
restart/rollback/replay pass on all three network profiles. Genuine pre-extension
v1 (800 bytes) and v2 (822 bytes) checkpoint fingerprints remain unchanged when
marking is disabled; v3 restores every basis, nominal obligation and event.

The conserving basis and default-offset policy is explicit in
[ECONOMIC_MARKS.md](ECONOMIC_MARKS.md). Untyped historical mark publication IDs,
full publication provenance, contract configuration, native marks, funded
liquidation/AMM/insurance and authenticated historical acceptance remain open.
No branch was pushed and no GitHub CI, Lean or Kani result is claimed.
