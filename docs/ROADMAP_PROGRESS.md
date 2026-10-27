# Roadmap progress and parallel-worker handoff

Assessment date: 26 October 2026. Reference JavaScript is
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`; this checkout started from Rust
main `c7a37eb90b1ea335051bfc3cb7712116b372cfb7`.

The user assigned authenticated parent-UTXO snapshots and real script
verification to a separate worker. This pass implements independent supporting
modules and reviews transaction-index scheduling and parsing. It does not merge
that worker's branches, publish a branch, or authorize economic transitions.

## Tracking the 49 source modules

The opt-in ProofLifecycleV6 batch passes **1,449 Rust test executions**
(1,448 top-level plus one subprocess) across 208 targets, including
28 new tests. Formatting, all-target Clippy, doctests,
release, zero-third-party-crate policy, eleven policy mutations and ten Core
bridge Python tests pass against 750 frozen inputs. All 84
focused tests across 31 targets pass with genuine locked
BigNumber9.1.2. See `fixtures/reference_fidelity/source-proof-candidate.json`.

Actual type30 settlement, procedural/cache/risk and reward production/settlement
share the source Core Tally, Property cache and collections. Literal write
prefixes, retries, source economics and actual tail32-35 errors/gates remain.
TLSW2 checks238 independently command-pinned source-backed slices; it is
conditional execution evidence, not a complete block or authorization proof.
CandidateEvidenceReader rescans every supplied raw block and rejects omitted,
reordered or relabeled carriers even with rebound transcript hashes. Draft
SQLite/native stores remain external adapters. Default remains V2; independently
compiled populated V5 bytes and all previous policies remain unchanged.

Lower0-15 composition in the same literal Core, real Main/Consensus processed
rows/phase selection, activation/native bootstrap, production storage/RPC and
authenticated inputs/history remain next gates. UTXORef remains a separate
consumer; no new Binohash feature is introduced into TL.

The source-index/distribution batch passes **1,421 Rust test executions**
(1,420 top-level plus one subprocess) across 201 targets, including 14 new
tests. Formatting, all-target Clippy, doctests, release, zero-external-crate
policy and 11 policy mutation tests pass against 730 frozen inputs.
All 63 focused tests across 25 targets also pass with genuine locked BigNumber
9.1.2. The complete txIndex oracle executes 320 observations: 319
successful owner comparisons and one explicit local RegExp-injection refusal.
All 21 source methods and the constructor are covered. Its component checkpoint
is diagnostic owner state; external DB/RPC and original source return-time
scheduling remain separate from this stable-after-drain API.

Opt-in DistributionLifecycleV5 inherits V4 and adds actual payToTokens,
BaiUrbun and Murabaha error prefixes plus raw26/28/29. Its 146-observation
corpus includes 144 Rust-comparable observations and two Node-only genuine
instance probes. Missing source helpers and wrong receivers remain literal;
no financial success is fabricated. An independently compiled, populated V4
fixture restores exact old bytes and future refusals. Default remains V2.
See `fixtures/reference_fidelity/source-index-distribution.json` for evidence.
The actual merged PR142 CoreRPC/raw-block bridge is integrated from upstream
41007c5 and checked on the same tree, including 10 Python tests. Native/SQLite
candidate-store PRs144/145 remain externally owned drafts. Scanner/metadata
composition, broader source domains, authenticated history,
full per-type/block proofs and external PR141 CPU/RSS promotion remain gates.

The explicit OptionLifecycleV4 batch passes **1,407 Rust test executions**
(1,406 top-level plus one subprocess) across 197 executable targets, including 16
new tests. Formatting, all-target Clippy, doc tests, release, zero-external-crate
policy and 11 policy mutation tests pass against 714 frozen inputs.
All 54 focused tests across 23 targets also pass using genuine locked
BigNumber 9.1.2. Five new corpora execute 461 observations: 459 Rust-comparable
observations and 2 Node-only bare-call probes, including reset states. All 135
original on-chain comparisons additionally run under V4. The committed evidence
is `fixtures/reference_fidelity/source-option-lifecycle.json`.

Actual option trades, synthetic mint/redeem, the final derivative exercise
handlers and populated expiry-index Maps/Sets now compose over the same source
owners. Literal missing helpers/variables, wrong argument placement, un-awaited
reads and retained financial prefixes remain observable. V4 option-adjustment
loss coverage preserves actual TypeErrors and earlier loss/IOU writes before
a later actor fails. Original compiled V1/V2/V3 fixtures retain exact bytes and
old capabilities. TLCRv4 is opt-in; default construction remains V2. TLSW1
coverage stays raw18/queue/fees/clearing; broader source methods, historical
authentication, production storage and complete per-type/block fraud proofs
remain open. The txIndex source DB lifecycle is separately queued; its scanner
is implemented and HashMap performance promotion remains externally owned.

The explicit SourceLifecycleV3 batch passes **1,391 Rust test executions**
(1,390 top-level plus one subprocess) across 191 executable targets, including 16
new tests. All-target Clippy, formatting, doc tests, release, zero-external-crate
policy and 11 policy mutation tests pass against 701 frozen inputs. All 38
focused tests across 17 targets also pass with genuine locked BigNumber 9.1.2.
The independent registry/channel corpora contain 456 observations (including
reset states and two explicit unbound-RPC refusals), plus focused coercion tests.
The committed evidence is `fixtures/reference_fidelity/source-lifecycle.json`.

Native/oracle, dated/perpetual and multiple-series definition creation now runs
the actual source lifecycle. Channel contract/token calls, withdrawal, transfer,
keep-alive, close/net/King settlement and queue draining use the same Tally,
live Channels registry and Margin identity arena as on-chain settlement.
Concurrent Tally operations and collateral-return aliases preserve complete
cache/storage/failure prefixes. TLCRv3 is opt-in; the default remains V2, and
compiled original V1/V2 fixtures retain their byte-identical replay behavior.
TLSW1 exports 14 tested conditional command slices from independently pinned
prestates; it does not establish actors, chain context or a complete block
fraud proof. Options/synthetics, remaining methods/domains, production storage
and authenticated history remain gates.

The GlobalClearingV2 batch passes **1,375 Rust test executions** (1,374
top-level plus one subprocess) across186 executable targets, including18 new
tests. All-target Clippy, formatting, doc tests, release, zero-external-crate
policy and11 policy mutation tests pass against684 frozen inputs. The new
funding, Margin, insurance, IOU, liquidation, expiry, mark/supply and transport
corpora comprise908 observations (including reset states), plus8 pure AMM
mutation scenes. All22 focused tests in12 targets also pass under genuine
source-lock BigNumber9.1.2. The committed record is
`fixtures/reference_fidelity/source-global-clearing.json`.

Actual block clearing now composes funding, three mark passes, live book/AMM
liquidation and ADL, source IOU/insurance, option expiry and final supply checks.
Queue DB return/throw effects and un-awaited history/dust jobs retain their
actual source order and failure prefixes. TLCRv2 replay binds full continuation
state; an original compiled TLCRv1 fixture retains byte-identical restoration
and its old unavailable methods. Source/conserving policy choices stay explicit.
Remaining gates include channel transactions/queue, full native/dated definition
lifecycle, production storage, authenticated history and portable witnesses.

The literal on-chain settlement batch passes **1,357 Rust test executions**
(1,356 top-level plus one subprocess check) across176 executable targets,
including18 new tests. Rust1.90 formatting, all-target Clippy, all-target release,
zero-external-crate policy and11 policy-mutation tests pass against660 frozen
inputs. All18 new tests also pass with genuine source-lock BigNumber9.1.2.
The committed record is
`fixtures/reference_fidelity/source-contract-settlement.json`. All32 complete-JS
scenes/135 observations now compare actual Rust queue-to-financial execution,
complete collections/caches and retained failure/retry effects. Focused suites
cover placement, actual reserve reclaim, margin aliases, fees/Insurance, IOUs,
oracle publication and storage/coercion adversaries; full command replay binds
complete continuation state. Earlier queue/read evidence remains in
`fixtures/reference_fidelity/source-contract-execution.json`. Whole-module parity,
authenticated history and exported transition witnesses remain open.

Actual source sends, asynchronous spot reads and an optional complete oracle
owner now compose with the same Ledger. The oracle corpus executes 311 actions
across all19 active methods; 17 complete-source send scenes preserve scalar,
vesting, malformed batch and Number-storage behavior. Types13/14 and oracle
administration mutate the actual documents, cache and watermarks without a
duplicate strict Registry. TLECv10 restore replays original source inputs from
both expected bootstrap pins. Source-thrown metadata prefixes survive, while
funded admission uses the named conserving policy. See
[SOURCE_ORACLE_EXECUTION.md](SOURCE_ORACLE_EXECUTION.md),
[SOURCE_SEND_EXECUTION.md](SOURCE_SEND_EXECUTION.md) and
[SOURCE_SPOT_EXECUTION.md](SOURCE_SPOT_EXECUTION.md). Genuine raw contract-order
validation now has actual selected-contract Margin and initial-margin reads
under explicit `ConservingSequentialV2`; V1 preserves its original unavailable
receipts. Genuine raw matched positions, Paid funding at24, and restore/flip/close
are tested through the sole conserving Ledger. The literal SourceContractRuntime now connects the actual raw18/queue/placement/
financial graph and compares all32 scenes/135 full-source observations, including
complete persisted collections, caches, fees, PNL/IOUs/rewards and retained failure
prefixes. SourceContractReplay restores the actual owner by replaying original
commands and checking outcomes/effects/full continuation state against an
independently expected head. This is an alternative source execution policy;
channel/native lifecycle and authenticated history remain open; global clearing/ADL/expiry now have independent source execution coverage. See
[SOURCE_CONTRACT_READS.md](SOURCE_CONTRACT_READS.md),
[SOURCE_CONTRACT_EXECUTION_REVIEW.md](SOURCE_CONTRACT_EXECUTION_REVIEW.md) and
[CONTRACT_QUEUE_SOURCE_OWNER.md](CONTRACT_QUEUE_SOURCE_OWNER.md).

The actual local property and clearlist catalogs now have owned source state,
103 property scenarios and 200 clearlist observations, with genuine bitcore
public derivation comparisons. Opt-in types1/7/8/9/11/12 reach those owners;
ordinary grants/redemptions use the existing sole Ledger. Complete v10 restore,
raw-candidate replay, durable recovery and rollback compose with native history.
See [SOURCE_CATALOG_EXECUTION.md](SOURCE_CATALOG_EXECUTION.md). Whole-reference
parity remains unestablished: outer rollback/activation policies, unsupported
domains and missing transaction paths are tracked separately.

The completeness workbook now uses **Runtime subset** for partial owner wiring,
with reference fidelity assessed separately. The new per-transaction inventory
in [LOGIC_BY_TX_TYPE.json](LOGIC_BY_TX_TYPE.json) covers the 36 routed source
types plus an unrouted type36 audit row. Its 13 mapper candidates are not 13
complete source transitions. See [REFERENCE_FIDELITY.md](REFERENCE_FIDELITY.md)
for exact source tests, missing owners, deliberate runtime differences and
fraud-proof witness requirements. Independent property and clearlist reviews
expose functional gaps and four repaired clearlist-audit reconstruction errors;
the focused type9 follow-up rejects effects the mapper cannot represent. These
changes do not promote any module to full reference or historical parity.

The current integration adds accepted type-16 oracle-perpetual definitions,
dependency-ordered complete input replay, and source-derived order/fill/average/
mark margin and fee contexts in the sole economic Ledger. See
[ECONOMIC_CONTRACTS.md](ECONOMIC_CONTRACTS.md). The existing AMM-first closeout
and funded LP receipt implementation now shares those contexts. These advances
do not complete native/dated/multicontract orchestration, all validity dependencies,
insurance/ADL or chain authentication; no source module is promoted to complete.

The previous integration added all 43 [source validity exports](VALIDITY_SOURCE.md),
complete [Types dispatch](TYPES_SOURCE.md), actual pending-owner dependency reads,
and [sequential source execution](CONSENSUS_SOURCE.md) through the same Ledger.
TLECv7 retains accepted and rejected raw candidates, reexecutes their real rules
on restore, finalizes economic phases once per block and checks exact rollback
history prefixes. The standalone [activation source lane](ACTIVATION.md) covers
18 methods plus constructor/static singleton; its checked corpus has 670 exact
observations and five explicit domain cases. These are source-conformance and
supported owner subsets; native genesis, complete dependency ownership and
authenticated historical admission remain open. Current full-batch checks are
passed locally; the final combined results and earlier producer/wire baseline are recorded below.

The integrated [TLSEv1 state-oracle producer](ECONOMIC_STATE_ORACLE.md) now exports
the actual locked owner's canonical full economic state with independently
expected configuration/head, exact network/genesis/tip, and separate
state/checkpoint/export commitments. Its binary/file export preserves exact
amounts and complete available replay validation. The state-oracle row is an
**Runtime subset**; authenticated admission, transition-witness export and
external consumer integration remain open. The codecs additionally have
[169 source composition cases](WIRE_ROUNDTRIP_REVIEW.md) and four passing
integrated tests, while their standalone classification remains unchanged.

[PARITY_ROADMAP.csv](PARITY_ROADMAP.csv) contains all 49 JavaScript source
modules, excluding the experimental README. It records forty-one assessed
TL modules below; seven retain their prior assessment and one is an external
UTXORef reference, excluded from the 48-module TL implementation scope.
The insurance and property rows additionally record external-worker assignments.
It supplements the historical [full-tree inventory](JS_RUST_PORT_AUDIT.md),
whose earlier mappings and assessment commit are preserved.

[SUPPORTING_FUNCTION_PARITY.csv](SUPPORTING_FUNCTION_PARITY.csv) records 958
source entries with their tested domains, Rust tests and remaining criteria.
None of these rows or modules is marked as a complete production port.

| Module | Implemented and verified scope | Remaining completion criteria |
| --- | --- | --- |
| `validity.js` | All43 source exports;564 observations comprising562 exact and2 explicit domains; actual pending-owner backend reads for represented facts | Missing property/list/country/xpub/proof owners, broader source domains, complete economic handlers and authenticated historical admission |
| `types.js` | Three callable functions covered across bounded encoding/dispatch/reference helpers;1228 observations including75 genuine Types-to-validity compositions and2 explicit UTF-16 domains; supported owner mapper | Full represented-handler coverage and underlying metadata owners, broader JS domains, authenticated raw inputs and historical acceptance |
| `consensus.js` | All30 active methods plus constructor characterized in56 combined consensus/main observations; bounded source JSON hash helpers and real sequential sole-owner v7 orchestration with raw/rejected replay and exact rollback-prefix checks | Legacy DB/peer methods are not all implemented; full phase owners, high-anchor bootstrap, authenticated admission, historical replay and resource evidence |
| `activation.js` | Standalone source lane:18 methods plus constructor/static singleton,36 registry names,6 admins; 670 exact observations plus5 explicit domains; canonical accepted activation origins derived from actual v7 owner replay | Actual sole-owner activation/native-genesis lifecycle, authoritative administrator/bootstrap, durable integration and historical acceptance |
| `dlcOracleBridge.js` | All7 functions;294 genuine-source observations; real native ECDSA and separate independently trusted payload-bound checks | Authenticated oracle/chain/state configuration, actual DLC economic integration and historical acceptance |
| `experimental/binohash/binohashAdapter.js` | External UTXORef optional feature; user excludes it from TL implementation | No TL Binohash parity gate; TL exports checkable state-oracle data to the separate consumer |
| `interface.js` | All26 active methods plus constructor;90 source scenes and5 stream cases; concrete immutable economic queries, exact custody and actual execution/funding/mark reporting | Historical/index/wallet/proof capabilities, complete external stores, deployment and authenticated inputs |
| `interfaceExpress.js` | All16 active exports; genuine Axios behavior, source errors/absent bodies and actual HTTP client binding | Broader rawURL/JS domains, remote TLS/authentication and production backend completeness |
| `walletInterface.js` | All19 active exports;213 combined client observations and named repaired routing; actual exact-balance HTTP integration | Actual wallet/key control, full database capabilities, deployment and authenticated state |
| `listener.js` | All19 active routes; bounded local HTTP dispatch into the original locked economic owner | Separate node subscriptions, unavailable backend capabilities, launcher/deployment and authentication |
| `walletListener.js` | All43 routes and5 helpers;134 combined source observations and genuine Express routing; explicit unknown sync state and no automatic broadcast | Full wallet/index/issuance/BitVM/external registry stores and authenticated mutation |
| `main.js` | All30 active methods plus constructor characterized in21 observations; durable bounded raw spool, cursor, retries, retained forks, cancellation and concrete Core polling CLI; source phase-order characterization and supported sequential economic owner integration | Authenticated observer-to-economic ingestion and parent UTXO/scripts, unrepresented reward/channel/native phases, Dogecoin AuxPoW, subscriptions, archival and deployed Core interoperability |
| `bitvmBundle.js` | All6 functions,23 full-source observations; source JSON/hash semantics and bounded actual artifact file reads | Independently trusted expected hash, production integration and real proof/signature authority |
| `bitvmCache.js` | All8 methods,52 full-source observations; source lifecycle quirks and atomic finite complete replayed metadata | Actual bond/escrow custody, authenticated actors, adjudication and durable cross-store integration |
| `bitvmRisk.js` | All11 functions,48 full-source observations; cap/window/clamp and repeated-call semantics with atomic metadata replay repair | Accepted event deduplication, actual custody reconciliation, authenticated sweep/escrow and policy promotion |
| `client.js` | All source members mapped and21 wrapper routes;68 observations; bounded HTTP/JSON, cookie rotation, wallet routing, controlled cancellation/deadlines and three-mainnet/genesis startup and daemon wiring | Real Core interoperability, TLS/pooling/testnet/DNS deployment, subscriptions and authenticated chain admission |
| `scaling.js` | All 18 source methods, 167 observations; keepalive/close/net/king metadata, explicit expiry repairs, complete replay checkpoint and real immutable-Ledger custody plans | Signed revisions/root/UTXO rotation, actual sole-Ledger settlement/margin integration, durable migration and historical acceptance |
| `txEncoder.js` | Standalone: all 39 exports, genuine source numeric/string/framing/coercion vectors; 169 encoder/Types/decoder composition rows and 4 passing integrated tests | Broader Number radix36/Unicode domains, historical acceptance and authenticated wire-to-execution integration; source Types identity adapter does not validate transactions |
| `txDecoder.js` | Standalone: all 36 active exports, genuine complete-source numeric/malformed/base vectors and later Type23 shadow behavior; 169 composition rows and 4 passing integrated tests | Target-engine promotion and historical chain corpus, raw-transaction/signature/admission integration; source Types identity adapter observes dispatch only |
| `adjuster.js` | Constructor and all three methods; 641 finite source rows and 19 source-only observations; exact tick/minimum and event order with atomic complete replayed metadata | Applying policy to orders, production persistence, wider JS domains and historical acceptance |
| `issuanceIntent.js` | All three methods; 321 source observations; arbitrary bounded ordered JSON, upsert/query, source ID quirks, nested address arrays and strict full replay | Functional metadata repair differs from the source missing DB category; token issuance authority, production persistence and broader NeDB/historical behavior |
| `stateOracle.js` | Runtime subset: all6 functions and2319 source observations; actual locked full-owner TLSEv1 export supports canonical TLECv1-v7/full-TLEH bytes, independent config/head/state checks, exact network/genesis/tip and new-file-only synced transport;7 baseline integration tests plus poisoned-owner regression | Authenticated admission and producer/oracle provenance, explicit external transition witness, UTXORef consumer integration and historical acceptance; broader legacy locale/Unicode/nonfinite domains remain bounded |
| `clearing.js` | Constructor and all 79 active methods assessed; source funding and mark arithmetic/quirks; funded fractional oracle-perpetual epochs, exact settled-basis marks and nominal default offsets in the existing Ledger with complete restart and rollback | Actual liquidation/AMM/ADL and insurance, native VWAP, full contract orchestration, complete admitted-publication history, authenticated marks/actors and historical acceptance |
| `orderbook.js` | All 69 methods, constructor and two comparators assessed; 287 actual-source vectors and nine source defects; contract book, funded type18/6 matching, owner cancellation and complete recovery in the existing joint Ledger | Authenticated orders and contract configuration, funded AMM-first execution, liquidation/ADL, multicontract orchestration, channels, source reporting/rewards and historical acceptance |
| `tally.js` | All 45 methods plus constructor assessed; 248 actual-source vectors and 32 source assertions; actual custody projections, funded match fee/rebate collection and complete FeeBook persistence | Legacy arbitrary-value/NeDB migration, authenticated channel/vesting rules, registered insurance lifecycle, buybacks, remaining shared economic stores and historical acceptance |
| `base94.js` | Eight helper methods, bounded string inputs, real-source golden vectors and preserved delimiter/fraction quirks | General JS coercions and malformed UTF-16; accepted historical wire transactions and admission integration |
| `base256.js` | Six helper methods, exact custom Unicode alphabet, bounded arbitrary-size arithmetic and source hex quirks | General JS coercions and malformed UTF-16; accepted historical transactions and wire integration |
| `tradeHistoryManager.js` | All18 methods characterized;43 scenes,205 helper cases,24 quirks; exact position/category/episode/LIFO/PnL reports, typed real/forced origins, TLTHv2 replay and immutable projection from the verified sole economic owner | Historical DB migration/order, broader JS domains, durable report service and authenticated fill provenance; unmarked ownerless receipts reject |
| `volumeIndex.js` | All27 static methods plus constructor;49 scenes,312 numeric vectors,26 quirks; finite source metadata, atomic replay/rollback and actual sole-owner directed spot price/VWAP | Native-coin valuation/accepted metadata, reward issuance, production DB migration, transcendental portability and authenticated historical integration |
| `iou.js` | Stateful buckets/claims and 67 source transitions; real-loss escrow and funded payouts; explicit atom policy; complete bounded IOU/ledger checkpoints | Authenticated matching/clearing integration and historical atom/dust rules, production DB behavior, broader JS coercion/prototype domain and historical acceptance |
| `options.js` | All nine helper methods, decimal PnL/pricing/exposure/maintenance, ticker and numeric-prefix behavior, source greedy order; 118 actual-source vectors | Surrounding contract creation/exercise/assignment and margin/clearing integration; cross-platform binary64 bridge verification; historical acceptance |
| `vaults.js` | Typed vault create/update/redeem, synthetic links, collateral interfaces and aggregate queries; 9 lifecycle seeds, 13 source queries and 30 characterized failures/quirks | Deliberate typed repairs differ from broken source paths; ledger mint/redeem, hedge lifecycle, durable persistence and funding/rebase specification remain open |
| `contractRegistry.js` | Typed definitions and expiry/catalog queries, native/oracle notional/margin, AMM construction, funding journal and atomic collateral plans; all 40 distinct methods assessed; 219 source cases | Sole-ledger/channel/margin execution, broken oracle pricing/funding dependencies, AMM hydration, durability and historical acceptance |
| `oracle.js` | Runtime subset: complete actual document/cache/watermark owner, all19 active methods,311 complete-source actions; actual13/14/8 metadata execution, exact persisted funded price views and full raw-source TLECv10 restore; legacy typed API remains separate | Funded staking/slashing and insurance closure, missing Margin/Contract orchestration, broader DB/JS domains, authenticated prices/actors and historical acceptance |
| `amm.js` | Capital/share lifecycle and journals, quotes, inventory/curve/token intents, local registry and options adapter; all 25 declarations assessed; 275 source rows | Funded LP issuance/secondary reserves, safe-price execution, AMM-first orderbook/clearing, durability and historical acceptance |
| `marginMap.js` | Positions, signed margin/PnL, option protection, liquidation/ADL plans and first mint; 246 source vectors plus 10 characterizations; constructor, 2 static and 43 active instance methods assessed | Authenticated funded collateral and clearing integration, broken source dynamic orchestration/redemption, durable position/delta adapters and historical acceptance |
| `vesting.js` | All 22 source members assessed; initialization/failure journals, chain retries, rebates and numerical helpers; 151 actual-source cases | Authorized genesis, supply discrepancy decisions, funded backend and durable partial-initialization recovery; historical acceptance |
| `procedural.js` | All nine methods assessed; local template/contract metadata, source state quirks and context rejection order; 154 actual-source operations | Authenticated proof/relay and settlement verification, funded issuance/redemption, durable metadata migration and historical acceptance |
| `db.js` | Source 32 categories and seven folder/bootstrap cases; bounded exclusively locked disk journal with atomic synced batches, explicit torn-tail recovery and complete funded IOU restart | NeDB query/shape migration, compaction, trusted rollback detection, broader state adapters and deployment power-loss validation including Windows directory entries |
| `persistence.js` | All 17 methods plus constructor assessed; 41 source observations; atomic persisted joint apply/rollback, bounded headers, independently expected configuration/tip, lost-acknowledgement and torn-write recovery | NeDB migration, unified state across all economic modules, trusted branch/tip evidence, compaction and deployment power-loss validation |
| `reOrg.js` | All three source methods, exact tip/RPC/log order and swallowed-failure behavior; 20 actual-source scenarios | Production bounded RPC transport and authenticated active-branch evidence; the source observer performs no economic rollback |
| `init.js` | Both source functions and12 source cases; owned startup, complete joint/IOU/economic restoration, concrete controlled HTTP readiness, exact heads, exclusive ownership and failure cleanup | Testnet profiles, broader source environment/deployment behavior, trusted checkpoint provenance and authenticated readiness |

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

## Guarded funded liquidation and complete book replay

The ninth local economic pass adds [guarded funded closeouts](ECONOMIC_LIQUIDATION.md)
against actual firm resting makers in the same Ledger. Private system IDs do
not create external orders. Owner-order cancellation refunds real reserve atoms;
actual available custody can cure margin; forced reductions release surplus
margin without requiring distressed initial-margin funding. Capped collected
fees alone fund maker rebates and retained fee/insurance-pending custody.

The v4 journal retains every block's ordinary book inputs, cancellation criteria
and source-ordered placements. Replay reconstructs the entire book and positions,
binds computed maker roles and ordinary fee amounts, checks each forced candidate
and cancellation, and follows known margin targets, losses, fee consumption and
top-ups to actual final margin custody. This prevents an empty later block from
hiding a forged historical book and rejects a fabricated zero-fill top-up.
Replay derives the initial deficit-owner set and requires every still-distressed
owner's attempt, including a zero-effect no-liquidity outcome; deleting both its
receipt and private replay ID does not make an incomplete history valid.
Funding positions bind before forced same-height reductions; the exact system
replay union covers funding, marks and liquidations. Full-state rollback retains
all these records together.

The named conserving policy is bounded and differs from source coverage, sizing
and orchestration. It uses the initial captured maintenance-call set and does
not claim a cascade into newly deficient makers. Complete wallet/publication
provenance, authenticated historical acceptance, funded AMM/insurance/ADL, native
marks, multi-contract operation and indefinite-history compaction remain open.
Frames cover every applied block from bootstrap and impose a documented
4096-block bound until a separately verified compaction format exists. Earlier
v1/v2/v3 checkpoint formats retain their disabled-feature bytes.

## Local validation for guarded funded liquidation

Pinned Rust 1.90.0 on Windows/MSVC passed all 872 tests across 112 executables,
formatting including every included implementation, all-target Clippy with
warnings denied, and the frozen all-target release build. The zero-dependency
gate and its 11 adversarial tests passed. Doc tests passed with none defined.
The unchanged-source oracles passed 130 mark vectors and nine characterizations,
117 funding vectors and 19 characterizations, 287 orderbook vectors and nine
characterizations, and 248 tally vectors and 32 behavior assertions. Their
complete clearing, margin-map, orderbook and tally archives match the pinned
JavaScript checkout after LF normalization.

Coverage includes linear and inverse full/partial closeouts, real reserve
cancellation and margin cures, inclusive price guards, price/time priority,
unfunded best makers, per-owner fill bounds, capped funded fees and odd-atom
rebates, funding before forced reductions, later ordinary matching, late-leg
overflow rollback and alternate-fork replay. Repaired-checksum probes reject
forged full book frames, positions, margin top-ups, forced identities, fees,
fill ordering and omitted zero-effect attempts. Complete checkpoint and durable
restart/rollback/replay pass on Bitcoin, Litecoin and Dogecoin profiles.
Genuine pre-liquidation v1 (800 bytes), v2 (822 bytes) and v3 (844 bytes)
checkpoint fingerprints retain their existing disabled-feature bytes.

The explicit policy and bounded-history limit are documented in
[ECONOMIC_LIQUIDATION.md](ECONOMIC_LIQUIDATION.md). These checks establish local
funded consistency, not authenticated inputs, historical liquidation acceptance,
AMM/insurance/ADL integration or indefinite operation. No branch was pushed and
no GitHub CI, Lean or Kani result is claimed.

## Funded contract AMM and LP receipt execution

The combined economic owner now supports an optional private funded AMM for its
configured contract and collateral. LP deposits move actual collateral into the
sole Ledger; nontransferable receipt units use exact quiescent pool NAV for
mint/redemption. Pool signed inventory, settled basis/carry, funding, claims,
defaults and margin stay in the existing structures. Two separately funded
quotes precede real firm-book closeouts under the same guard and fill cap.

Complete per-height frames retain capital inputs/receipts, quote invalidations,
funded quote epochs and every pool cash leg. Independent replay derives the
exact cash stream from capital, quotes, funding, actual margin reconciliation,
fees and IOU/default settlement; an offsetting fictional transfer pair cannot
hide behind unchanged final custody. Public actors and transfer/administration
address endpoints cannot impersonate the private pool. LP activation must
precede the capital frame, including after a rechecksummed restore.

TLEC v5 persists the complete pool and typed private AMM fill origins. Earlier
versions retain their existing bytes, including a genuine active v4 checkpoint
with owned-order cancellation and five complete book frames. The bounded
conserving policy repairs source virtual funds/fees, mixed capital and inventory,
and inverse safe-price direction; it does not establish historical acceptance.
Ordinary AMM market making, live-risk LP operations, transferable LP properties,
token-pair reserves, insurance/ADL, authenticated admission, native/multicontract
execution and indefinite-history compaction remain open.

## Local validation for funded contract AMM execution

Pinned Rust 1.90.0 on Windows/MSVC passed all 896 tests across 114 executables,
formatting including every included implementation, all-target Clippy with
warnings denied, and the frozen all-target release build. The zero-dependency
gate and its 11 adversarial tests passed. Doc tests passed with none defined.
The new complete-source liquidation sweep oracle passed 22 cases; the existing
AMM oracle passed all 275 observations. Mark, funding, orderbook and tally
oracles retained their 130/117/287/248 vectors and behavior characterizations.

Coverage includes actual LP custody, exact NAV shares after funded trading
profit, zero-share rejection, partial/last redemption dust, two independently
reserved quotes, AMM-before-better-book priority, linear/inverse execution,
guard fallback, shared fill/inventory caps, pool insolvency and actual book
closeout, real rebates and funding, later ordinary matches and reorg recovery.
The quote planner rejects maintenance-only funding that cannot cover existing
initial-margin risk. Private-pool balanced external transfers reject atomically;
repaired-checksum history probes reject balanced fictional cash legs, altered
share/quote/cash metadata and LP activation moved into its deposit block.

Full v5 snapshot and durable restart/rollback/replay pass on all three network
profiles. Genuine v1/v2/v3 and active v4 checkpoint fingerprints remain stable
with AMM disabled. The actual price, margin and fee caller contexts still need
accepted contract-catalog binding; this funded policy does not establish source
historical acceptance or authenticated execution. No branch was pushed and no
GitHub CI, Lean or Kani result is claimed.

## Core RPC, complete wire source coverage and L2 lifecycle

The local node client now provides actual bounded HTTP requests rather than
only offline RPC evidence tooling. Source routing/defaults remain separately
characterized; node readiness binds independently expected mainnet/genesis and
never silently changes the configured chain. Loopback tests use mock nodes and
do not execute real wallet operations or prove a deployed Core integration.

The encoder and decoder cover every active source export inside their named
bounded domains. Source framing disagreements, missing dispatch exports,
undefined/nonfinite values and engine-specific rounding remain observable.
These helpers still require the real authenticated raw-transaction admission
pipeline; complete function inventory does not establish historical acceptance.

The scaling owner adds source settlement metadata and explicit safe lifecycle
repairs with complete checkpoint replay. Its King settlement consumes immutable
Ledger custody observations and returns a checked transfer plan. It does not
claim a co-signature, rotate UTXOs or mutate a separate spendable balance store.
All four module completion gates remain open in the 49-file ledger.

## Durable observations, complete history reports and BitVM metadata

[RUNTIME.md](RUNTIME.md) connects controlled Core HTTP with the owned bootstrap,
complete durable economic restoration and a runnable bounded observation CLI.
The [daemon](DAEMON.md) journals raw material and an observation cursor only;
its node reports never authorize economic apply or rollback. Both successful
and failed RPC returns check cancellation before retry or publication. The
concrete wrapper keeps the economic owner immutable and checks journal health
without repeatedly hashing the unchanged checkpoint during polling.

[Economic reporting](ECONOMIC_REPORTING.md) projects exact real execution facts
from the verified complete owner into expanded [trade history](trade-history.md).
Ordinary orders, forced book and private AMM origins remain distinct. Booked
PnL, collected cash, offsets and event-wide payouts retain separate meanings.
Pure marks and funding are borrowed rather than recalculated or settled by a
second store. Source method failures and repaired reporting are characterized
separately; missing historical owner evidence fails explicitly.

The three BitVM modules cover artifact integrity, cache lifecycle metadata and
risk counters. Their123 full-source observations use genuine Node crypto and
explicit database/file/environment boundaries. Atomic metadata writes and
strict full replay repair partial source writes while retaining repeated-call
semantics. These modules grant no proof, signature, custody or payout authority.
External property, insurance, clearlist and authenticated txIndex work remains
reserved and unimported unless actual accessible code is independently checked.

Local Rust1.90/MSVC validation passed1,039 tests across127 executables, frozen
all-target Clippy with warnings denied, formatting including every included
implementation, and the frozen all-target release build. The zero-external-crate
gate and11 adversarial tests passed. All five changed source oracles passed
against both archived and actual pinned source. Local loopback/process tests
exercise cancellation, exact restart tokens and real journal ownership; they do
not establish deployed Core or historical-chain acceptance. No push or GitHub
CI, Lean or Kani result is claimed for this local batch.

## Source clients and the actual local query service

The five previously unported service modules now have full source inventories,
unchanged-source oracles and explicit repaired APIs. The concrete local HTTP
runner in [SERVICE_HTTP.md](SERVICE_HTTP.md) connects both client objects to the
complete route tables and actual immutable economic data. It retains the
original durable lock and Ledger; queries never create replacement state or
grant economic apply/rollback authority.

Known balances preserve available, contract/spot reserves, margin, channels and
pending withdrawals as separate exact atom quantities. Public zero-available
owners with real locked custody remain visible; private escrow accounts are
separately scoped. Catalog queries distinguish unavailable catalog evidence from
an unknown ID. Funding/mark/trade histories preserve real retained provenance.
Unknown parser/index heights stay null and cannot manufacture a synced result.

The full-source evidence includes213 Axios client observations,134 listener
observations and90 facade scenes plus5 exact stream cases. Genuine installed
Axios1.20.0 and Express4.22.3 checks pass. Raw URL/coercion domains and broken
source dependencies remain explicit observations rather than invented successes.
The immutable service reports already-restored readiness without starting the
daemon, loading a wallet or accepting protocol transactions.

The external clearlist worker now reports commit1ad166a with native BIP32 and
TLCLv2. Its actual Git object/patch bytes remain inaccessible locally. Rust and
genuine bitcore checks therefore remain pending; this does not promote local
clearlist or authentication completion. An independent encoder/decoder worker
assignment reserves only a new wire-roundtrip generator, fixture and review doc.

Local frozen Rust1.90/MSVC validation passed1,071 tests across131 executables,
all-target Clippy with warnings denied, formatting of all modules/includes,
and the all-target release build. The zero-third-party-crate gate and11 mutation
tests passed. Loopback tests preserve exact large supplies and the durable head,
reject malformed requests before dispatch and stop a stalled body on control
cancellation. No GitHub push or CI/Lean/Kani result is claimed for this batch.


## Native relay verification and complete finite VolumeIndex coverage

The DLC bridge now uses real dependency-free public secp256k1 ECDSA verification,
with 1,497 independent BigInt/OpenSSL/genuine tiny-secp256k1 reference records.
All seven bridge source functions have 294 complete-source observations. Source
legacy fallback is retained as a compatibility observation; the separately
configured payload-bound policy requires the independent key, event, outcome,
index, state, time and exact raw payload. Neither policy grants economic or
chain admission.

VolumeIndex now inventories all 27 static methods plus constructor: 49 source
scenes, 312 numeric vectors, 26 quirks and the original 15 unchanged rows. Finite
metadata uses explicit atomic batches, prefix rollback and complete replayed
checkpoints. Source DB failures and placeholder paths remain observations.
Signed-zero tie/underflow regressions and the largest safe block's derived
exclusive query bound are covered. Transcendental last-bit platform differences
remain a documented metadata limitation.

Executed spot-price and inclusive directed VWAP queries now borrow actual fills
from the original economic owner, retain exact atom sums and owner tip, and
recompute after complete restoration/rollback. No fallback price or fabricated
native property metadata enters these queries. Native contract admission remains
a separate integration gate. The full-owner state-oracle producer is now
implemented as described below; source query hashes remain a distinct format.

The user clarified that UTXORef is external and TL exports checkable state-oracle
data to it. Binohash is excluded from this crate and tracked as an optional
feature in UTXORef. The inventory therefore contains 48 TL modules and one
external reference. Both encoder and decoder implementations are standalone
ports with complete active-export coverage in documented domains; remaining
legacy-domain, historical corpus and accepted-runtime checks stay explicit.
The earlier external activation assignment was subsequently reassigned to the
local queue after that worker stalled. Its standalone source lane is now
integrated as recorded below; actual owner activation/native issuance remains
separate. The hung external wire-corpus lane was also reassigned locally.

Frozen local Rust 1.90/MSVC validation passed 1,101 tests across 134 executables,
all-target Clippy with warnings denied, formatting of all modules/includes and
the all-target release build (6m21s). The zero-third-party Rust gate and all 11
mutation tests passed. Genuine pinned source/crypto oracles and original Git
blob comparisons passed. No new GitHub CI, Lean/Kani, push or publication is
claimed. Full historical/authenticated production parity remains open.

## Integrated full-owner state-oracle export and wire composition

The producer now implements [TLSEv1](ECONOMIC_STATE_ORACLE.md) over the actual
healthy locked `DurableEconomicChain`. One canonical full TLEH checkpoint
carries its borrowed TLECv1–v6 state payload, network/genesis/exact tip,
rollback floor and distinct domain-separated state/checkpoint/export
commitments. Independently expected configuration and durable head are checked
before canonical owner decoding and available full replay. The binary writer
uses `create_new`, streams and syncs bytes, and the small manifest avoids a
large Base64 service response. Existing legacy query bytes remain unchanged.

Seven passing `tests/economic_state_oracle.rs` tests exercise actual funded
margin/fee/IOU state, exact balances above `2^53`, all three mainnets,
independent expectations, canonical corruption/limits, no-overwrite file I/O,
restart and rollback/ABA fencing. An internal passing regression injects a
synced-but-unacknowledged commit and requires the poisoned original owner to
deny cached-state export. Active TLECv6 input/mark replay is included. Detached
metadata stores, authenticated chain admission, an explicit transition-witness
export and the external UTXORef consumer integration remain separate gates.
The scope remains **48 TL modules plus one external UTXORef feature reference**;
no Binohash implementation or proof engine is added to TL.

The real `tests/wire_roundtrip.rs` target passes all four tests against 169
unchanged-source composition rows. Coverage reaches all 39 encoder exports,
36 decoder exports and 36 numeric Types encoding routes. Missing exports,
double prefixes, malformed outcomes and returned Error objects remain source
observations. The Types identity validity adapter records dispatch/arguments;
it neither executes the source validity module nor establishes acceptance.
Encoder and decoder remain standalone ports in their documented domains.

The last fully verified producer/wire baseline passed **1,113 tests across 136 executable
targets**, plus empty doctest targets; Cargo formatting, explicit included-file
rustfmt, all-target frozen Clippy with warnings denied, the zero-third-party
Rust gate and all 11 mutation tests passed. The Node wire check passed all 169
composition rows. The all-target release build passed in 7m 00s for this
baseline. No GitHub CI, push, proof-system validation or authenticated historical
acceptance is claimed.

## Source validity, Types and sequential owner replay

The complete unchanged-source validity corpus covers all 43 exports in 564
observations: 562 exact comparisons and two explicit unsupported domains.
The Types corpus has 1228 observations: 1151 dispatch/helper cases, 75 genuine
Types-to-full-validity compositions and two explicit unsupported UTF-16 cases.
Source quirks, returned Error objects, deferred rejections, argument mutations
and global-write diagnostics remain observable; successful source evaluation
alone does not grant economic authority.

The actual `EconomicSourceBackend` derives represented facts from the pending
sole owner with independent network/parent/height context. The opt-in
`ConservingSequentialV1` runner recomputes dispatch, rules and the strict mapper
for every raw candidate. Each handler stages atomically; failed, unavailable,
unsupported and duplicate candidates retain evidence without partial custody
changes. The existing book, mark, funding, AMM, liquidation and Joint end phases
run once, and one durable commit publishes their state with the audit. A late
phase failure discards the entire proposed block.

TLECv7 stores the independent bootstrap commitment, canonical bootstrap, full
headers/raw inputs and accepted/rejected receipts. Restore reexecutes them
against real owner state rather than trusting persisted validity decisions or
dependency replies. Before-state checkpoints must be exact prefixes of the
live raw history, closing the rollback-splice case where two valid histories
share asserted header IDs. Source mode denies the old typed bypass; disabled
mode preserves v1-v6 bytes. TLSEv1 can carry v7 without claiming an external
transition witness or authenticated L1 provenance.

The consensus oracle has 56 complete-source observations and characterizes all
30 consensus methods plus constructor. Most legacy database/peer/hash methods
remain characterized behavior, not complete Rust implementations. The real
block driver is the explicitly conserving owner integration: indexed and live
funding partitions retain their distinct source order, while empty blocks and
late failures use the once-per-block durable owner policy. Source-valid oracle
lag zero remains an explicit actual-Registry rejection; positive owner fixtures
use the existing minimum-lag policy rather than weakening it.

Activation has 670 exact source observations and five explicit domain cases.
The [derived activation-history view](ACTIVATION_HISTORY.md) binds accepted
origins to actual v7 replay, exact source positions/txids and Registry records;
restart, rollback, rejected candidates and durable freshness are covered. All 18 methods, constructor/static singleton,
36 names, six administrators, strict 33-of-36 threshold, persistence/failure
order, repeated activation and source placeholders are covered. Its standalone
metadata and named repaired intent do not issue native supply or replace the
actual owner administrator/prior-height/replay gates.

The source batch at `6403716` passed **1,162 Rust test executions: 1,161 top-level
tests plus one subprocess lock check**, across **144 executable targets** and
one empty doctest target. Every top-level test runs without filtering or
ignores; the lock-test child intentionally selects one helper. Cargo formatting,
explicit rustfmt for all included implementation files, all-target frozen Clippy
with warnings denied, the zero-third-party policy and all 11 mutation tests pass.
The all-target frozen release build passes in **5m 44s**. The four changed Node
oracles pass validity564, Types1228, consensus56 and activation670 exact plus5
explicit domain observations. All535 implementation/check inputs stayed unchanged
through the final checks. These counts include real source-vector and durable
owner tests; a vector row is not counted as an independent Rust test. Full property/list/country/
xpub/proof dependencies, all source economic phases, native genesis and high
anchors, historical acceptance and authenticated parent-UTXO/script admission
remain separate completion gates. The inventory stays 48 TL modules plus one
external UTXORef optional-feature reference; Binohash remains outside TL.

## Ordinary Joint restore boundary

A valid standalone channel-bearing Ledger/Spot checkpoint could previously be
embedded in a CRC-repaired ordinary TLJP snapshot, bypassing the Joint
constructor capability fence. Four genuine regressions failed before the fix:
free channel escrow, reserved withdrawal, pending NET claim and swept KING
history were accepted. Restore now repeats the existing channel/settlement
liability fence during ordinary component verification. All five new tests pass
across the three networks, including unchanged normal checkpoint bytes. Economic
combined-mode policy and standalone channel-capable Ledger/Spot behavior remain
unchanged. This closes an import inconsistency; it does not implement signed
channel revisions or authorize channel admission.

## Canonical co-signed channel allocations and source lifecycle

[CHANNEL_REVISIONS.md](CHANNEL_REVISIONS.md) specifies the explicitly new
SupersedingCoSignedAllocationV1 binary policy, canonical allocation/body domains,
strict two-slot low-S signatures and exact successor/custody/context checks. The
independently configured descriptor binds both participants/keys, the funding
epoch/outpoints/script digest and actual bootstrap commitments. Eight transcript
tests consume81 independent reference vectors; twelve additional genuine signed
owner envelopes use actual Ledger bootstrap bytes across all three networks.
These are new-policy vectors, not historical JavaScript transactions.

The [funded owner](SIGNED_CHANNEL_OWNER.md) stores accepted journals beside actual
Ledger escrow, with no second balance owner. Enrollment identity is deterministic
local metadata, not an authenticated L1 txid. The old commit, withdrawal, transfer
(both endpoints), NET and KING entry points are fenced after enrollment. Complete
TLLv4/TLPSv3/TLUCv2 checkpoints require independent policies; disabled legacy
formats keep their existing bytes. New journal/enrollment tails must first appear
at the exact retained transition height and parent. A reproduced valid-state
retroactive splice failed before the fix and now rejects. Whole-tree record,
witness, crypto-count and depth bounds agree between live writes and restore.

The [durable channel wrapper](DURABLE_SIGNED_CHANNELS.md) binds an independent
sequence/tip/checkpoint head, stages the whole actual owner and commits one synced
Journal value. Rollback/reapply changes sequence even when all checkpoint bytes
return to the same value. Real disk tests cover three-network revision/restart,
stale-head rejection before adapter work, atomic failures, independent policy
substitution, OS lock/existing-store fences and a real synced-write lost
acknowledgement. Poisoned cached state/head/Debug access remains fenced.

The complete [channels.js source lane](CHANNELS_SOURCE.md) covers all43 static
methods plus the implicit constructor with the actual unmodified source and
genuine BigNumber. Explicit unsupported JS prototype/reference domains are local
rejections, not source-conformance successes. Source partial effects and errors
remain metadata observations, separate from the funded allocation repair. L1
funding authentication, actual participant identities, script/UTXO validation,
commitment revocation and full historical channel acceptance remain open.

## Verified channel-batch gates

The integrated channel batch passes **1,200 Rust test executions: 1,199
top-level tests plus one subprocess lock check**, across **148 executable
targets** and one empty doctest target. All top-level tests run without filtering
or ignores; the lock-test child intentionally selects its helper. Pinned Rust
1.90 formatting, explicit formatting of every included implementation file,
all-target frozen Clippy with warnings denied, the zero-third-party policy and
all11 dependency mutation tests pass. The all-target frozen release build passes
in **8m11s**. All553 implementation/check inputs remained frozen throughout the
final run.

The unchanged-source channel corpus has445 exact action observations and46
explicit unsupported-domain observations over480 scenes. Independent/genuine
reference checks pass81 transcript vectors and12 actual-funded-bootstrap signed
envelopes. Thirty-eight new Rust tests cover these source/signature cases,
actual sole-owner state, retained-transition/resource regressions and durable
recovery; fixture rows are not counted as separate Rust tests. The ledger keeps
49 module rows (48 TL plus one external feature) and958 unique function entries,
preserving all914 previous entries and adding43 channel methods plus constructor.
No module is promoted to complete historical or authenticated parity.

## Native initialization and entitlement source audit

[NATIVE_GENESIS_SOURCE.md](NATIVE_GENESIS_SOURCE.md) records22 executable source
scenes:18 complete activation/vesting/property/tally compositions with1,647
dependency events, plus four logic/tally/vesting entitlement scenes. Seven exact
source archives are pinned by literal hashes. The ordinary async DB-factory
behavior and the explicitly conditional synchronous repair stay separate.

Declarations, current issued custody, source curve scales and vesting markers
are distinct. Source initialization can persist partial state then report a
complete flag on retry. A reachable fractional coupon transfer creates marker
rounding drift; the repaired synchronous payout can then credit one holder before
failing on another. These observations justify atomic owner transitions but
select no genesis amount, residual beneficiary, cap, mint or default payout
policy. Existing Rust code is unchanged by this audit; its1,200-execution checked
baseline remains70dd911. The two new Node oracles and exact regeneration checks
are the relevant new checks, not additional Rust test counts.

## Funded native and insurance owner integration

The opt-in [native entitlement owner](NATIVE_ENTITLEMENTS.md) now keeps actual
pre-funded target custody, coupled coupon sends, remaining markers and complete
receipt replay in the same Ledger. Independently supplied policy separates
declarations, current supply, optional ceilings, curve scales and actual funding.
It selects an explicit per-call or monotonic-delta driver, exact source quotes
or atomic rejection, coverage and payout order. It supplies no default genesis
amount, residual beneficiary, source volume authority or mint. TLL5, native-aware
Joint formats and TLECv8 retain complete owner state and input history. Two new
unchanged-source Number-boundary scenes bring this audit to24 unique scenes.

The [oracle-perpetual insurance backstop](ECONOMIC_INSURANCE.md) uses real
sponsor deposits and actual insurance-fee custody. Ordered partial coverage
follows the actual AMM/book liquidation phase, pays only outstanding claims and
reduces the matching net unpaid liabilities. Existing funded payouts and new
insured payouts remain distinct; historical gross defaults remain historical.
TLECv9 replays the independently pinned full bootstrap, native policy when
enabled, complete typed inputs, actual custody and insurance receipts.

Every contract-bound retained transition now requires an exact typed-history
prefix and exactly one new frame at its header height, alongside the native
Ledger journal guard. The valid-branch regression rejects a rollback witness
from an independently valid but different earlier funded history. This applies
to existing v6 histories as well as v9 insurance histories.

All29 new integration tests and two private insurance tests pass in the
full repository-wide run. The batch passes **1,231 Rust test executions:
1,230 top-level tests plus one subprocess lock check**, across **153
executable targets** and one empty doctest target. Formatting, every included
implementation file, all-target frozen Clippy with warnings denied, the
zero-third-party policy and all11 mutation tests pass. The all-target frozen
release build passes in **10m40s**. All592 implementation/check
inputs remained frozen through the final gates. The genuine Node checks
verify24 unique native source scenes; fixture rows are not Rust tests. Source sends still need accepted property
and clearlist capabilities; source-mode explicit targets and source+insurance
composition reject. Authenticated genesis, historical admission, positive-height
native migration, native/signed-channel coexistence, the full source insurance
lifecycle and ADL remain open. External wire175-case and insurance49-case reports
are not imported artifacts and do not increase local fixture or test counts.
