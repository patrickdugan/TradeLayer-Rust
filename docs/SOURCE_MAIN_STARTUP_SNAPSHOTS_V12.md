# Main V12 startup and snapshot composition

The additive V12 paths compose the pinned Main, init, Database and Persistence behavior with the existing Core collections, Activation and TxIndex owners. Default GlobalClearingV2 is unchanged. This phase is validated separately from the preceding full release fa00946. Its focused results do not establish full-reference parity.

The snapshot corpus has 57 observations and 33 actual source calls. Cold startup has 65 observations, five complete unchanged module imports and 55 actual Main calls. Both numeric profiles preserve outcomes, ordered effects, receiver state, collection/cache state and recorded file bytes. The first-ten proofs remain exact. Tests reopen/replay accepted command boundaries. Actual older V12 ordinary checkpoint bytes remain admitted; an earlier isolated capability-error history is refused when its receipts no longer match execution.

Scheduled snapshots preserve Consensus hash calls, pretty metadata, copying/pruning and partial failures. Restoration backs up/copies/removes recorded files and launches the original unawaited height save. The source does not reopen datastore handles or reload Tally/cache state. This behavior is preserved: file restoration is not financial rollback or a durable host-storage barrier.

Cold import runs the original module IIFEs and opens all 32 datastore categories through the same existing row owner. An unawaited Activation initialization rejection retains partial owner state, permits Database/Main initialization to finish and emits the rejection in original order. Once the actual Persistence singleton is assigned, Main.handleReorg retains its native missing-method error. Malformed metadata, iteration and path argument errors retain source property-read order and write prefixes.

Main.delay, immediate setPause, warm/cold genesis lookup and the missing TxUtils.getLatestBlockHeight TypeError are covered in the admitted finite profile. Source IO is a bounded recorded POSIX artifact map. A supplied file, hash or RPC result does not authenticate history or grant actor authority.

Terminal shutdown uses the separate opt-in SourceMainControlSessionV1 described in SOURCE_MAIN_CONTROL_V1.md. It preserves exit0 and console effects without a host exit or generic post-call async drain. Existing V1-V12 Invoke outcomes and wire formats are unchanged. Pause/resume scheduling, startup exit1, remaining Persistence IO branches, rich thrown values, real storage/RPC publication, authenticated history and complete-block proofs remain open.

Focused evidence and exact reviewed inputs: fixtures/reference_fidelity/main-startup-control-integration.json. Remaining coverage: SOURCE_MAIN_PRODUCTION_METHOD_MAP.json and COMPLETENESS_STATUS.json.
