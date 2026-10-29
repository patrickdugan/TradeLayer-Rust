# TradeLayer-Rust live handoff

Updated: 2026-10-29T23:04:22Z. Maintained after every tested integration cycle.

## Goal and boundaries

Finish the pinned JavaScript port and remaining roadmap. The inventory has 48 TL modules plus one external Binohash reference; file/function counts do not prove parity. JavaScript reference commit: `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. TL produces checkable state-oracle data. UTXORef is a separate consumer; optional Binohash work belongs there.

Root owns tracked edits, one whole-Core Cargo process at a time, checks and commits. Subagents work in isolated stages and return immutable manifests. Commit completed tested work on the current branch. Do not push, publish, open a PR or merge without explicit authorization; preserve unrelated changes. AGENTS.md requires the full repository preflight before any authorized Rust push.

Keep one actual financial owner, actual native index and borrowed queue. Never install a dummy index, import expected financial/results state, or recover an uncertain same-process write by repeating a Main/source action. Retry only retained exact native/control operations through their sealed proofs. Integrity hashes are not authentication.

## Verified current state

Full all-target release: implementation `53e6a245d58265aa1a428f30c567aa67e610464c`, tested checkout `306d4186327a8d43d0fdf4c23c89563d3c0bb111`. **2,142 Rust executions** (2,141 top-level plus one successful storage child), **239 locked numeric checks**,282targets and1,822 frozen inputs unchanged before/after. Library491/fourignored. Evidence: `fixtures/reference_fidelity/inline-channel-release-validation.json`. Previous2,120/238 release remains immutable historical evidence.

Current later cycle: same-invocation retained Main consensus,5 new Core/1source-job/3independent caller controls. Library520/fourignored,44 public,7 oldinline/6Mode4, warnings-denied Clippy/fmt291/inventory pass. Actual Initialize fulfills with its final save held and actual financial/native catchup runs without a competing plan. The same owner/header is retained; new route binds actual origin/generation. WAL/recovery/cold/arbitrary concurrent publications remain open. Record: `retained-main-consensus-core-integration.json`. Full baseline stays2,142/239.

The test-only temp-folder fix `567f6b8` respects TL_NATIVE_MAIN_TMP when set and otherwise uses the standard temp directory, preserving canonical-parent/guarded cleanup. Seven actual native release tests pass without the override; no production source changed. Evidence: `fixtures/reference_fidelity/native-main-test-temp-default.json`.

Saved workbook and sidebar last promote the 2,120/238 release at code239c340. They preserve all 37 transaction rows, styles/formulas/filters/panes and false full-parity/proof gates. Evidence: `fixtures/reference_fidelity/native-inline-release-completeness.json`. The later475 scoped description is now synchronized to code146984e in both saved workbook and sidebar; never add scoped executions to the release count. Verification: `fixtures/reference_fidelity/cold-acknowledged-actions-completeness.json`.

## Integrated pieces — do not reapply old packages

- Actual completed-command publication/service, native/control recovery and WAL/history with guarded generation retention. Native recovery covers explicit validity plans; cold metadata/page/history checks retain actual owner locks. Records: `native-main-service-publication-core-integration.json`, `native-main-wal-history-core-integration.json`, `native-recovery-cold-rows-core-integration.json`.
- Historical RowPC prepare → recorded native verification → same source-Promise resolve; same-index cold capability blocks mutations, including discard and escrow paths. Record: `historical-native-await-core-integration.json`.
- Guarded escrow binding sync precedes cross-publication native IO; strict read-only HISTORY-WAL ACK reader and exact existing scalar pin codec. Record: `native-escrow-ack-codec-core-integration.json`. The old reader does not admit Mode4/ESCROW-WAL cold closure.
- Sealed earlier financial prefix with actual later native Post/capability retained, no stale ordinary Session/Replay/checkpoint release; five Core controls include two independent negative cases. Record: `cold-prefix-promises-core-integration.json` and `sealed-cold-prefix-independent-review.json`.
- Ordered callable initialization and Promise v2: 37 Core controls. Both genuine numeric profiles reproduce original14 scenes/99 observations and new12 scenes/143 observations. Core compares98+142 cutoffs; the explicit unavailable full-module boundary observations are not global-IIFE successes. Pending jobs/notifications fence checkpoint/adoption and old witnesses. Arbitrary callbacks/thenables and wider startup remain open.
- Explicit retained inline live entry: actual processBlockData transaction write and separate MaxHeight/indexExists append/return awaits, same Core/index/queue, decoder runs once. Seven live-service Core controls plus owner/database tests pass. Original13 calls/38 records reproduce both numeric versions; this is not13 full Rust Main caller matches. Save/genesis owner PCs exist, but complete startup/catchup callers remain work. Record: `retained-inline-service-core-integration.json`.
- Current cold acknowledged action entry consumes the sealed prefix plus real checked ACK reader, moves the one financial owner, executes strict raw actions with actual historical metadata, recomputes every complete Before/Accepted field stream and original physical receipt, and verifies/resolves the same native await without appending. Both success/failure retain actual index/capability and WAL locks; no hot owner escape. Domain: HISTORY-WAL, one available original publication, ordinary fulfilled validity writes, resolved terminal await. Record: `cold-acknowledged-actions-core-integration.json`.

All record names above are under `fixtures/reference_fidelity/`. Source results/validity/scheduling/financial tables are comparison data, never execution inputs.

- Explicit Mode4 inline WAL and same-process native Before/Intermediate/Post recovery are integrated. The raw intent and any compaction escrow are synced before native IO; replacement mappings sync before native-only retry. The actual financial owner/source PC survive ambiguous writes; no action/decoder retry. Six actual Core and four metadata controls pass. Old readers still refuse Mode4; detached callers/coldMode4/killed-process finance remain open. Record: `inline-native-recovery-wal-core-integration.json`.

## Active workers and next concrete gates

1. `v8_main`: Mode4 and copied-history package45a42cf8 are now integrated/tested. Next is actual historical inline DB preparation against the verified original pages/directory, using a separate opaque historical plan; no fake live index or append permit from pins. Coordinate caller selectors with row. Wider coldMode4 admission remains closed.
2. `row_continuation_impl`: af/3a caller+consensus components are now integrated/tested, including actual Initialize fulfillment and source-job lifetime. NEW Start-only WAL packagebbcd706b is frozen but awaits Root Core gates; raw TLMACA1 codec6/actualStore3new tests pass in helper. Active follow-on wraps selected caller/consensus/detached/native/timer actions with exact origin-route keys and executed-once WAL barriers. No source constructor/action retry after uncertain IO.
3. Both lanes must bind an explicit stable job/scope/operation ordinal plus Awaited/Detached selector. Identical method/args/Before may represent different concurrent saves; AppendPending must not silently choose the first frame. Main owns the WAL/frame selector; row owns caller PCs. Stage-only until Root checks.
4. `v8_clearlist`: channel sort packagea05a13d2 is now integrated/tested, including real64 settlement/65 late-error retained Tally writes, exact comparator schedule and four prior checkpoints. The same sharedV8 helper/MainIO remain unchanged. Mode4 independent review is complete; two additional Root negative controls pass. Worker now audits current admission/UTXO gates and recent external-worker PRs before proposing any disjoint implementation.
5. Broaden cold execution to inline/Mode4, recovery/supersession, cross-publication retained views, unresolved terminal work and actual killed-process financial recovery. Current cold success must not be relabeled as those broader gates or as mutable hot resume.

The e0cee8/0a94 cold-action package is now integrated in this cycle. Older4a/6fa/546690/3b459/8c062/7b839/b2ae/6db packages are already integrated; use new pure transforms against current text rather than old whole-file overlays.

## Non-Main and release requirements

All36 literal routed kinds execute in bounded profiles; only13 are in the separate conserving mapper. Do not mistake that mapper count for missing literal types3/4. Preserve source placeholder failures for17/25/26/28/29/32–35; successful repairs need a named protocol, not invented success.

PR152 fee and PR153 reward standalone accounting are integrated/tested (`pr152-fees-core-integration.json`, `pr153-rewards-core-integration.json`). Literal JS fees/insurance/rewards already run through actual source settlement. New standalone APIs have no implicit matcher/Ledger/payout/consensus wiring: require an explicit opted-in economic policy and real fill/fee/eligibility provenance before wiring.

Remaining real boundaries: parent UTXO/all-input/chain and TradeLayer actor/economic/proof-verifier admission; genuine type23/31 trade/participant/root/claim evidence; complete per-type/block proof composition beyond conditional TLSW4/5; broader source domains; collaborator fuzz results; intended-host/Core interoperability, representative real-block CPU/RSS and directory/power-loss evidence. Do not infer authentication or complete fraud proofs from a checksum, valid flag, claimed actor, green tests or source inventory. Full parity and complete-block proof gates remain false.

Unavailable external insurance/property/clearlist/wire patches are redundant/unverified handoffs, not proof those independently implemented local modules are missing. Verify actual bytes/ownership before importing anything.

## Local validation

Rust1.90 MSVC, no external Rust crates or unsafe. Work scripts set MSVC/SDK paths via `../../work/cargo-check.ps1`. Use named CargoArguments arrays. Debug target: `../../work/main-v12-target`; release target: `../../work/source-settlement-target`. Set TMP/TEMP and native/Main test temp variables to `../../work/cargo-gate-tmp`; genuine JS checkout is `C:/Users/patri/Documents/Codex/tradelayer.js`. Clear source-lock variables for ordinary tests; exact source-lock tests use genuine BigNumber9.1.2. Loopback RPC tests need local test-server access.

Preserve failed logs and immutable packages. Latest e0 scoped runner: `../../work/check-cold-active-actions-second.ps1`; exact log hashes and Root typed-borrow correction are in its integration record. A full release requires a clean committed checkout, frozen tracked-byte manifest, all targets/doc/exact numeric list and before/after verification. Keep source unchanged during that run and distinguish raw execution-manifest versus canonical LF manifest hashes.

## Latest tracker check

Thirteen intentional cells now show code146984e/475 library/44 public scoped checks and the concrete channel64+ gap, preserving all37 transaction rows, formulas, effective styles, filters, panes, geometry and proof gates. The full baseline remains2,120 Rust/238 locked numeric checks. Zero formula errors. Saved workbook and sidebar rows match; source coverage is not promoted beyond the declared domains.

## Latest source-caller check

Twelve unchanged Main/TxIndex calls and35 complete input/observation records reproduce under each genuine numeric profile. Existing13 calls/38 records reproduce byte-identically. New CI checks retain this oracle. No Rust implementation or release count changed; the detached final save still needs actual retained caller/job integration.

## Latest tested integration

Mode4 preserves one actual owner and exact pending native operation across lost-control/lost-native returns, partial writes and real compaction. The new485/44 scoped counts include prior controls and must not be added to the2,120/238 full release. Source caller characterization075aba4 remains source-only evidence until the retained Main wrappers pass actual Core controls. Workbook/sidebar now show the tested inline/channel/review progress through53e6a24; the full release baseline remains2,120/238. Verification: `fixtures/reference_fidelity/inline-channel-completeness.json`.

## Latest channel integration

The reproduced64+ ready-queue refusal is fixed through the existing1,024-row source domain using the original comparator and shared safe V8 sort. Source outcomes, callback/error effects and partial financial writes remain faithful. Source placeholder errors are not repaired into success. No new fee/reward policy, owner, network adapter or authentication is selected. Keep the remaining caller/cold-view and host/proof requirements open.

## Latest independent recovery review

Two added native controls pass inside the actual Root crate, alongside all four prior inline metadata controls. Valid-CRC altered/extra native state is rejected before repair; foreign/stale/duplicate/Post mappings cannot authorize a retry. Production code is unchanged; warnings-denied Clippy/fmt267/inventory pass. The previous full library remains489 until the next all-target run. Record: `fixtures/reference_fidelity/inline-native-recovery-independent-review.json`.

## Latest completeness refresh

Fifteen cells now reflect the tested channel fix, same-process inline recovery and two independent controls, with48 TL modules/one external reference/all37 transaction rows preserved. Full parity/authentication/complete-block proof flags remain false. Saved workbook and sidebar row hashes match; styles/formulas/filters/panes/geometry and unrelated cells are unchanged, with zero formula errors.

## Latest complete release gate

All282 Rust targets, doc tests and the exact87-target genuine9.1.2 list pass on the same frozen checkout. The new channel financial target is included in both full and locked lists. Every1,822 tracked input was checked before/after; raw versus canonical input-manifest digests remain separate. This promotes the tested inline/channel/review code, while retained Main callers, coldMode4/cross-publication/killed-process finance and host/authenticated complete-block proof gates remain open. Saved/sidebar headline promotion is the next metadata step.

## Latest copied-view integration

The exact original metadata facts and actual copied-file identity remain separate. Strict decoding gives expected facts only; real copy/manifest/full-KV/row receipt verification is still required. Five new Root tests plus19combined escrow checks pass. Next work is genuine historical inline preparation and complete source/WAL composition, not a broad reader flag. Full release remains2,142/239; update the workbook with this later scoped work separately.

## Full-release workbook promotion

Saved workbook and existing sidebar now show **2,142 Rust executions /239 locked numeric checks**, implementation53e6a24/tested306d418,282targets/1,822unchanged inputs. Latercc405ad is shown separately as5new/19combined copied-history metadata checks. Sixteen intentional edits preserved all48TL modules/one external reference/37transaction rows, formulas, effective styles, filters, panes, geometry and proof gates. Zero formula errors; saved/sidebar row hashes match. Verification: `fixtures/reference_fidelity/inline-channel-release-completeness.json`. Full source parity, authenticated history and complete-block fraud proofs remain unestablished.

## Latest retained-caller integration

Source helpers and actual Core caller controls now preserve the detached final save rather than forcing a synchronous caller wait. Exact selectors fence wrong/stale job or operation actions. The first component intentionally leaves ConsensusReady unfulfilled; the next same-invocation consensus composition is still active work. Historical-inline990/review049 and Bitcoin-composition41dd are now Root-integrated/tested. Independent caller review is running separately. All current tested source is committed locally; no push/PR is authorized.

## Latest historical/Bitcoin integration

Historical plans compute old-view outcomes and exact native mutation bytes, verify actual Post and resolve once; they cannot create a writer permit or resume a source PC. Bitcoin relation checks every supported input and basic money/duplicate/order rules but keeps authenticated parent/chain/TL gates denied. Root regenerated genuine reference fixtures; neither API widens economic or cold service admission. Caller review679 and same-invocation consensus3a8211 remain queued for Root gates; new Mode4 commit-trace/physical closure and durable caller composition are active worker lanes.

## Latest scoped workbook refresh

Saved/sidebar now showe3a7245/511library and tested historical/Bitcoin/Main-caller scope, while the complete release stays2,142Rust/239numeric. Thirteen intentional edits preserve all48TL modules/one external reference/37transaction rows, formulas/styles/filters/panes/geometry and proof gates, with zero formula errors. Verification:`fixtures/reference_fidelity/historical-bitcoin-completeness.json`. No queued consensus/WAL/trace-reader work is promoted.

## Latest consensus integration

The actual parent invocation is now shared with catchup/service; original normal constructors/visitor bytes remain. Source caller outcome may precede its detached child, but the original header/owner cannot escape before both finish. Existing335 selectors remain receiver-local routing values; new origin-bound profile rejects accidental cross-owner equal-key selection without pretending hashes confer authority. Root fixed the genuine readiness guard and an incorrect test-only absent-field expectation; no reference data was relaxed. Mode4 physical closure3cc and callerStartbbcd remain queued for actual Core gates.
