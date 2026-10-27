# Main transaction and phase evidence

Node-only evidence for the next literal block driver. Source pin: `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. These five portable files provide no Rust Main integration, build, or new balance owner. The 39 observations do not claim successful full Main import, daemon startup or RPC composition.

The generator produces **39 observations: one full-import boundary, 13 original-constructor resets, and 25 selected Main method invocations**. The initial ten were generated and checked before expanding the corpus. Both archived BigNumber 9.3.1 and the exact source-lock 9.1.2 constructor produce byte-identical observations.

| Artifact | SHA-256 |
| --- | --- |
| `fixtures/main_phase_proof10.jsonl` | `e69ea166fece3dc682e38c4c6f121a86d0d310c6c3a8525b4a93b805ad704107` |
| `fixtures/main_phase_vectors.jsonl` | `0efbb184f59f39f1a387bea0d4870797b5223f5694922759d837009e622abced` |

`fixtures/main_phase_inventory.json` pins the original sources, underlying reference driver, complete extracted classes, and callable method text. The JSONL records keep readable names and summaries; the authoritative `command`, `result`, `state`, and `effects` fields are tagged values retaining Undefined, Number bits/nonfinites/signed zero, BigNumber, Map, Set, RegExp, Promise and field order. A summary is only a convenience projection.

The final observer corrects an initial channel-cache projection to the actual `channelsRegistry` field and additionally records full Property/Contract instance fields and tracked lexical Clearing contexts. The source commands, outcomes, ordered calls and financial effects did not change. These final hashes supersede the preliminary first-ten projection.

## Import and extraction boundary

The unchanged **full Main module** is evaluated first. This deliberately bounded reference loader permits genuine Node `events` and `util`, then refuses `./reOrg.js`. The record is `SourceBoundaryUnavailable: Unavailable full-import dependency: ./reOrg.js`. This is a declared loader boundary, not a claim that the source import inherently throws. Client/DB and Activation startup IIFEs have not run at that point. No successful Reorg/Persistence/client/activation result is supplied to get past it.

For the selected calls, the generator takes the exact complete class text from `class Main {` through its closing brace and likewise the exact `TxIndex` class. It adds a lexical factory around each class and supplies explicit dependencies. It does not rewrite, replace, reorder or extract individual method bodies. It executes the **original Main constructor**, then invokes original methods on that same instance with `Reflect.apply`. Every snapshot checks both its Main prototype and the source singleton identity.

| Extracted text | Bytes | SHA-256 |
| --- | ---: | --- |
| Complete Main class | 39,938 | `cadcab37cf95dcc78d2258d12817c88fc6a01b9220c6cf18a25ddf68b43a7ea2` |
| Complete TxIndex class | 19,533 | `01dcd57e943046f5bb1dc7bc01efe6ea98e456be593ff91ddb5b4799072b257e` |

Selected original callable text hashes:

| Method | SHA-256 |
| --- | --- |
| `Main.processTxSet` | `8e5b8f262824b636cbcef8c304d00304e363a447d309bc09a1f3386117a95e7c` |
| `Main.processTx` | `3b0a52813e06d82cf6f8419abab9e15c91f287deb5a67873d67ca932f627a091` |
| `Main.blockHandlerMid` | `9f72101ef4ddbbaa01c440ae0819b23aeddcb25549c6eddf855706bc696b3a76` |
| `Main.constructConsensusFromIndex` | `49d765706d2959d0d134b13951b4f8b56f5a21f0442c567a5e20add572272d2a` |
| `TxIndex.upsertTxValidityAndReason` | `7f84fa341dbbc091118ad3500cd90c8fb6c74c4b1906a8399d9d8fc9695fbcc9` |

The Main constructor genuinely calls the original `TxIndex.getInstance`, which starts `init` and reaches the unavailable `ClientWrapper.getInstance(true)` capability. The constructor completes with its original rejected Promise field; the harness observes that rejection. It supplies no initialized index/client answer. Class extraction omits module-scope startup/import execution and is expressly a callable-class profile. It is not an assertion that this receiver came from successful production Main initialization.

## Actual dependency graph

The existing `scripts/source_contract_execution_vectors.cjs` source loader is reused with narrow loader/boundary extensions only:

- Load the complete pinned Activation and Vaults modules. Remove the fixture's three activation answer functions. Actual `Activation.isTxTypeActive` now reloads the actual persisted registry through the same shared DB. The constructor's chain/test/admin fields remain absent; `init` is not pretended successful.
- Use the unchanged complete Consensus module, original Types/Validity/Logic, Tally, Orderbook, Margin, ContractRegistry, Clearing, Property, Insurance, Channels, Volume, IOU and other already pinned business modules. No validation, placement, match, reserve, funding, clearing or withdrawal reply is supplied.
- Preserve the original TxIndex metadata methods. Its validity update uses the actual `/^tx/` query and suffix selection. The explicit fixture DB gains genuine JS RegExp matching for that query.
- Refuse all reached ClientWrapper/TxUtils/RPC capabilities. Bind source `COIN` to its original `100000000`. Vesting and Persistence capabilities are explicitly unavailable if reached. Zero-volume vesting branches skip under the actual source condition; there is no supplied vesting success.
- Keep the existing deterministic clock/UUID/RNG and one-shot storage-failure boundaries. Optional balanced prestate imports set property circulation to the actual source Tally totals; this is initial data, not a substituted supply-check result. The original unbalanced prestate remains in the first ten and genuinely fails its supply check.

All financial calls read and mutate the **same shared fixture DB and source Tally singleton/cache**. Complete raw source state, extra Margin instances, channel pending/live caches, activation registry, property/contract cache, constructor future, receiver fields, and ordered DB/source calls are retained. Hidden calls to the reference driver's unowned financial proxies fail a generator assertion. There are no hidden supplied financial replies in the 39 observations.

Margin instances retain encounter order and their current values; tracked Clearing contexts are read through the original pure getter. This value corpus is not a general object-alias checkpoint format. A future Core comparison must also retain the explicit object-origin capabilities of its already-owned Margin/queue graph.

The shared recording DB is an explicit test boundary, not genuine production NeDB or a compact store. Its structured copies retain Undefined and nonfinite values. Existing genuine NeDB copy/serialization evidence remains a separate prerequisite when choosing a production storage profile. Error-valued decoder results and their own-property/copy behavior are not part of this corpus's admitted metadata cases.

## Observed behavior

The first ten establish a concrete transaction/phase path:

1. Full import stops at the declared Reorg import boundary.
2. Original class construction retains the failed original index-startup future.
3. Indexed type 32 has no `valid` field, reaches no Logic, but is still written processed and has its index row updated.
4. Its next indexed attempt returns one skip after only the processed lookup.
5. Missing max height returns Null.
6. Max-height save performs the genuine consensus upsert.
7. The next height read returns 5.
8. Empty live mid-phase calls only actual withdrawals and returns height 10.
9. A genuinely valid type 18 queues and reserves via real source code. Main calls fees, rewards, on-chain queue, channel queue, then Clearing. Clearing throws the genuine Property-1 supply mismatch before withdrawals.
10. A processed-marker write fails after another real queue write. Main's original outer catch writes fatal processed params, updates index validity, retains `didWork`, and attempts the same actual financial phases. It again reaches the supply mismatch, preserving all prior effects.

The additional 29 observations establish:

- Balanced imported supply allows an actual live mid-phase to complete. Its order is fees → rewards → on-chain queue → channel queue → clearing → withdrawals. Repeating the already-processed row instead calls withdrawals alone.
- Inactive-18 validation consults actual Activation storage and marks the row processed without post-phases.
- A late Logic active-pair write failure is caught, marks Logic invalid, yet keeps the valid-before-Logic work flag and still runs real post-phases. A later index-validity write failure similarly rewrites processed params under the fatal catch while the queue prefix remains.
- An unawaited max-height write can reject after the processed marker while `blockHandlerMid` still returns 10 and completes its financial/withdrawal calls. A Node `unhandledRejection` observer records that exact failure. The observer prevents the audit process's default shutdown; the corpus does not claim production host liveness after an unhandled rejection.
- Track-height saves/reads and max-height read failure/retry use original source methods and storage calls.
- Indexed catch-up with no TL rows returns without post-phases. A new invalid type-32 row does run real phases, with **withdrawals before clearing**. Already-processed rows skip those phases. Reached height 10000 forces them despite all IDs already being processed.
- Catch-up starting at empty height 24 and ending at an invalid row at 25 does not call clearing for 24; the actual clearing call is `[25,false]`.
- Original indexed partitioning of database order `[M,K,m,k,4]` yields processed lookups `[K,k,4,M,m]`. Already-processed seed rows isolate partitioning without fabricating validation/financial outcomes.
- Completed finite catch-up reaches the original `syncIfNecessary` and stops at the unavailable `client.getBlockCount`. Its preceding journal and financial effects are retained. Full live begin instead stops at the first unavailable `Persistence.getInstance`, before a claim of indexing/funding execution.

These are completed-drain observations after five `setImmediate` turns. Source calls and DB argument arity/order are retained. This is not generic at-return or endless polling scheduler equivalence. The separately observed constructor rejection and unawaited height rejection must not become successful initialization or a transactional block commit.

## Smallest faithful implementation interface

The evidence supports a new policy-bound internal Main driver over the **existing Core**, with original ordered index/live rows, explicit mode/height, and actual consensus processed rows as inputs. The driver should internally select the source phase list and retain its unfinished continuations. The journal/height fields and raw source params must be part of the same replay-bound authority as Tally, queues, Margin and all collections.

The current public `execute_contract_unverified` result alone is insufficient as the complete Main interface: Main needs the actual decoded parameter object after `.block` assignment and Logic's mutation/throw, then applies its original catches before marker and validity writes. Returned Error objects from Types are also not thrown exceptions; Main can assign `.block` and persist them. That later domain needs a genuine storage/object representation test rather than replacement with `{name,message}` or a predeclared invalid verdict.

Useful minimal hooks are an internal actual Types/Logic call retaining the parameter receiver and throw, real `Consensus.checkIfTxProcessed/markTxAsProcessed`, real TxIndex validity upsert over the same collection, source height read/write continuations, and direct existing phase capabilities. All source-thrown prefixes remain. Local representation/resource refusals need a separate explicit whole-composition rollback rule. No new conserving Ledger, expected financial verdict, atomic marker repair, replay deduplication repair or fake initialization should be introduced.

Full live begin, physical reorg restoration, native bootstrap, positive-volume vesting, paid funding scheduled by Main, arbitrary raw types/lower-owner composition, default production NeDB durability and complete startup remain outside this evidence. Existing individual owners/corpora can be reused, but their successful tests do not fill these missing Main continuations automatically.

## Reproduce

When installed in the repository, the script defaults its reference root to the parent of its `scripts` directory and reads/writes the three evidence files under that same root's `fixtures`. Run from the repository root with Node 24 and an explicit npm prefix containing the genuine exact dependencies already required by `source_contract_execution_vectors.cjs` (`bitcoinjs-lib`, `tiny-secp256k1`, `bip32`). The existing root CI provisioning pins their versions. The BigNumber constructors come from the checked source archive and exact-lock archive, not an ambient npm constructor.

```powershell
$env:TRADELAYER_JS_ROOT = '<explicit provisioned reference npm prefix>'
node scripts/main_phase_vectors.cjs --proof10 --check
node scripts/main_phase_vectors.cjs --proof10 --lock --check
node scripts/main_phase_vectors.cjs --check
node scripts/main_phase_vectors.cjs --lock --check
```

For the staged work-only copy, set `PROOF_REPO` to the actual repository's absolute path and invoke `work/next-main-phase-driver/scripts/main_phase_vectors.cjs` from its containing workspace. This override changes only where the pinned sources/reference loader are read; output fixtures remain next to the staged script's parent directory. No repository output is written by that staged invocation.

`--print` emits the selected observations. `--proof10` checks or regenerates only the first-ten fixture and preserves the expanded fixture/inventory. Ordinary invocations without `--check` regenerate all three evidence files. A changed pinned source, class text or expected observation fails its strict check; the full check additionally compares the reference-driver/body-hash inventory.

No hash here authenticates input parents, actors, chainwork, activation provenance or initial balances. Expected states are assertions, never supplied dependency answers. A future Core comparison must independently execute these original commands and compare the complete graph and ordered effects.
