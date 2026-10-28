# Original Main upper transaction corpus

This is source-only evidence for a proposed V9 Main admission. It runs the byte-exact original Main and TxIndex classes and unchanged complete Types, decoder, encoder, Logic, Activation, financial, registry, channel, synthetic, procedural and BitVM metadata modules from commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. No Rust compilation or source-parity result is claimed by this corpus.

The first proof executes one input reset and ten actual `Main.processTxSet` calls before extension. Those ten cover types16,17,19–26 and are available source calls, preserving actual validator failures and caught source exceptions. The complete corpus has 146 frames: 111 Main calls, comprising 109 available source calls and two explicitly unavailable capability experiments, plus 35 constructor/input resets. Frames are not counted as successful Rust transaction matches.

All 18 requested additional kinds are exercised through indexed, indexed processed-retry, live and live mid-phase calls: 16,17,19,20,21,22,23,24,25,26,27,28,29,30,31,33,34,35. Kinds18 and32 retain their earlier evidence and are not added to these counts. Positive/funded scenes additionally exercise original Oracle contract creation, raw19 contract-channel trading, raw21 withdrawal, raw22 transfer, raw23 keepalive, raw24 mint, raw27 option trading, raw30 staking/fraud/relay and raw31 king settlement. A fresh ordinary relay and funded options/king settlement reach all six actual postphases.

## Input and observation boundary

Activation is the actual singleton loaded from explicit persisted `activationsList` rows, with exercised kinds active at block1. Main uses the source-registry profile; no declared activation or validation answer is supplied. Each reset records its constructor options and complete input tables. Channel columns, balances, native positions, contract metadata, scaling history and property supply are input rows, not snapshots copied from a source result.

The explicit RPC document profile executes unchanged original TxUtils `getRawTransaction`, `getBlockHeight`, classifier and public-key extraction methods over supplied raw transaction/block documents. RPC facts remain unverified. Main/TxIndex constructor client startup remains an unavailable boundary; the callable original methods execute on the real original receivers without running the module startup IIFEs or daemon.

Source financial/module methods are unchanged. The shared audit DB adapter preserves the existing bounded query/clone behavior, anchored RegExp matching and the `$push` operation already used by the channel Logic source tests. The first attempt exposed this missing audit-adapter operation; the final corpus supplies that actual bounded storage behavior rather than labeling the adapter exception as a JavaScript defect. The genuine NeDB package as a whole is not claimed equivalent.

The signed relay inputs use deterministic test key1 and genuine `tiny-secp256k1` signing. The unchanged source bridge performs its own signature verification. No `valid`, verification verdict, financial result or success callback is supplied. A signed relay remains input to the source's existing authorization logic; it does not authenticate the Main actor, Bitcoin history or this producer.

The artifact case supplies raw JSON bundle bytes and a claimed hash. The original bundle helper parses and hashes those bytes. Its path dependency uses standard Node `path.posix` with the absolute virtual namespace `/tl-source-artifacts/`; the FS adapter reads only explicitly supplied bytes. It performs no host filesystem access. Hash equality is source artifact integrity, not a BitVM execution proof or financial authorization. No Binohash implementation is added to TL-Rust.

Snapshots retain complete persisted collections, Tally static and distinct instance state, all four ClearList maps, source Activation state, actual Orderbook receivers, Margin instances and position identity references, option indexes, vault/synthetic maps, source environment/artifact inputs, channel/withdrawal caches, clearing contexts and ordered effects. The transparent original `loadOrderBook` observer registers genuine fresh prune receivers before forwarding the unchanged original method. No receiver or source field is removed to obtain equality. Five drained event-loop turns are an explicit observation boundary, not general JavaScript scheduling equivalence.

## Concrete source findings

| Reached path | Original result retained |
| --- | --- |
| Native raw16 body | Decoder produces empty native pairs, so validation refuses it |
| Nested Oracle raw16 | Valid and creates the actual contract series |
| Raw17 | Main catches `derivativeRegistry is not defined` and records invalid processed parameters |
| Funded raw19 | Indexed path is valid; its live path reaches `ContractRegistry.isNativeContract is not a function` at the first fee postphase, retaining valid processed parameters |
| Funded raw20 | Original un-awaited sufficiency result causes the channel-balance refusal |
| Raw21 encoder/body | Decimal amount text and column0 expose the reference's wire mismatch; a base36 amount with column1 reaches a source withdrawal accepted by its column check |
| Funded raw22 | Real source transfer executes through original TxUtils document processing |
| Funded raw23 / raw31 | Actual scaling history and channel writes execute; processed retries skip the original rows |
| Native raw24 / raw25 | Mint succeeds; redemption retains the source `notCovered is not defined` failure |
| Raw26 | Main retains `tallyMap.hasSufficientBalance is not a function` |
| Funded raw27 | Actual option path is valid and its live call reaches all six postphases |
| Raw30 stake/fraud/relay | Actual stake and reward writes occur; after fraud, a later live relay detects the real +0.5 supply mismatch during clearing |
| Signed raw30 and artifact cache | Original signature and bundle-byte checks execute and the accepted metadata/financial source prefixes persist |
| Raw33 | Processed decoded parameters have no `valid` field; no valid flag is invented |
| Raw34 / raw35 | Main records the original absent-decoder method errors |

Storage-failure scenes inject actual consumed writes at Oracle stake recording, processed-marker persistence and index-validity persistence. Each retains the completed source prefix and then executes the exact processed retry in order.

The two unavailable experiments are `TxUtils.validateAddressWrapper` and missing raw artifact bytes. Each has its own reset; no unavailable poststate feeds a later available case. Their full outputs remain in the fixture but they are excluded from source-comparison candidates.

## Reproduction and pending gates

From an installed repository, run `node scripts/main_upper_vectors.cjs --check`, then add `--lock` for genuine BigNumber9.1.2. The default reference is genuine archived BigNumber9.3.1. `--proof10 --check` checks the first ten calls, and `--print --check` executes the original modules again before printing fresh tagged frames. In the work stage, set `PROOF_REPO` to the actual repository and `TRADELAYER_JS_ROOT` to the genuine dependency checkout. No fixture or returned answer is used to drive execution.

Full/proof regeneration checks pass with both numeric constructors. The complete position-return observer retains the actual uncached seller temporary; `SOURCE_MAIN_UPPER_POSITION_OBSERVER.md` records that correction. Fresh Rust differential and older-policy gates pass as described in `SOURCE_MAIN_UPPER_INTEGRATION.md`. Complete-suite and release verification remains separate; generation does not authenticate actors or chain history.
