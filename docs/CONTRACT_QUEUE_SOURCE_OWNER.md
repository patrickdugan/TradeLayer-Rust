# Pinned contract and token queue owner

`ContractQueueSourceOwner` owns the actual `orderBooks` document collection used by the on-chain queues and the separate book caches of constructed source instances. It ports nine methods from `src/orderbook.js` at commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The full archived source is `fixtures/legacy_orderbook_source.txt`, SHA-256 `c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98`.

This owner supplies the real queue and book-metadata write graph. It does not implement `addContractOrder`, `addTokenOrder`, matching, margin settlement, clearing, or a second balance ledger. Placement is an explicit capability. Without that capability, processing a ready order returns `UnknownDependency` at the first actual placement call and retains the queue.

## Source methods and state

| Source method | Owned behavior |
| --- | --- |
| `_getOrderbookDB` | Acquires the `orderBooks` adapter. The returned database object is represented as `{sourceDatabase: "orderBooks"}`. |
| `_addActivePair` | Reads `activePairs`; uses the source array membership test; writes and reloads only when adding a pair. |
| `_updateActivePairs` | Replaces the active-pair document, then reloads the database. |
| `queueOnChainTokenOrder` | Appends the entire token order under `queue-<String(key)>`, preserves original IDs and fields, reloads, then adds the pair. |
| `queueOnChainContractOrder` | Appends the entire contract `params` value, preserving shifted flags and nested original IDs without projecting typed parameters. |
| `processQueuedOnChainOrdersForBlock` | Processes active pairs in document order, selects all non-NaN heights through `Number(height)`, sorts ready entries by the source comparator, invokes placement, loads the book again, then writes the future queue and final active pairs. |
| `getOrderbookInstance` | Constructs a fresh instance on every call and loads its own cache. |
| `loadOrderBook` | Reads and parses the saved `value`; catches find/parse errors at the source boundary. Database acquisition occurs outside that catch. |
| `saveOrderBook` | Replaces `{_id, value: JSON.stringify(data)}` and leaves the instance cache unchanged. |

Rows retain insertion order, all source fields, `undefined`, Number bit patterns including negative zero, nested arrays and objects, and the supported BigNumber/Promise diagnostic values. The queue does not deduplicate transaction IDs. Unknown `kind` entries are ignored during placement and removed when the ready queue is committed. NaN-height entries remain future entries. Token and contract entries can share the same pair queue and comparator.

Each instance retains `tickSize`, the original `orderBookKey`, `orderBooks`, and `block`. A missing or malformed saved JSON value returns an empty book without replacing an existing cache entry. Valid JSON numbers retain the actual `JSON.parse` binary64 result, including negative zero, overflow to either infinity, and underflow to signed zero. The static constructor assigns the returned empty book to its new instance. Saving a new value does not refresh any instance cache. Imported rows affect only the database, so stale caches remain stale until an actual source load updates them.

## Public API and sole-ledger placement boundary

Use `new`, `rows`, `import_rows`, `state_value`, `create_instance`, and `instance_value` for initialization and inspection. `evaluate` accepts a `SourceCall`; `evaluate_instance` supplies the instance receiver required by book load/save. `evaluate_with_backend` forwards placement to a `SourceBackend`. `invoke_source`, `invoke_source_with_backend`, and the `SourceBackend` implementation adapt the same methods to `DependencyReturn::Pending` / `PendingThrow` and the `Orderbook.*` target namespace.

`evaluate_with_capability(call, &mut callback)` gives the callback `(&mut ContractQueueSourceOwner, &DependencyCall)`. This permits a real future placement implementation to load and save the **same** book documents while using the application's sole ledger. The owner never synthesizes book writes from a capability's return value. The source ignores the returned match result; a capability's returned value is likewise ignored. Actual writes must occur through the declared capability and owner operations.

The live JavaScript receiver is represented by a stable `{orderbookInstance: id}` handle:

- `Orderbook.addContractOrder` forwards the source's first twelve arguments unchanged: `contractId`, `price`, `amount`, `sell`, `insurance`, `blockTime`, original entry `txid`, original entry `sender`, `params.isLiq || false`, `reduce`, `post`, `stop`. Argument thirteen is the handle for the existing instance, as in the source.
- `Orderbook.addTokenOrder` forwards the source's three arguments: the full `order`, original `blockHeight`, original `txid`. Its fourth capability argument is the explicit receiver handle; this is adapter context for the source `this` receiver, not an extra argument in the JavaScript call.

A callback can inspect `instance_value(id)` and call `evaluate_instance(id, save/load)` before returning. The test capability exercises genuine source book saves and injected rejections; it makes no claims about order matching or balances. Genuine full-module matching observations are separately produced by `scripts/source_contract_execution_vectors.cjs` and do not authorize the Rust owner to invent missing economic effects.

## Failures and persistence

`QueueDbFailure {operation, skip, exception}` schedules a declared adapter rejection. Accepted operations are `getDatabase`, `findOneAsync`, `updateAsync`, and `loadDatabase`. The first matching schedule decrements `skip` for each matching operation and throws at zero. `take_db_calls` and `take_placement_calls` drain bounded diagnostic traces; those traces are observations, not replay authority.

Source exceptions and an external dependency failure retain their actual prefix:

- A queue update can persist before the reload or active-pair update fails, leaving an orphaned queue.
- A later placement can throw after earlier placement or book writes. The queue remains uncleared, so retry invokes those original entries again.
- The post-placement database acquisition can fail before the queue write. A find/parse rejection during the load is caught by the original source boundary.
- A previous pair's queue may already be cleared when a later pair fails. The final active-pair update has not run, so the persisted active list can remain old.
- The final active-pair update can fail after all ready queues have cleared.

A local resource or unsupported-domain rejection is a separate Rust boundary. It restores the owner's rows, caches, failure schedule, and diagnostic state to their pre-call values. It cannot undo writes already performed by an external capability. An integrating application needing atomic local-capacity behavior across owners must stage its sole ledger and all affected owners together before committing. Source exceptions must not be silently converted into that local rollback.

## Checkpoints and bounded domain

`checkpoint` / `from_checkpoint` (aliases `encode` / `decode`) encode complete current rows, all instance fields and IDs, and pending adapter-failure schedules. `durable_checkpoint` / `restore_durable` encode only persisted rows: restart discards process instances and injected fault schedules. The two formats have distinct mode bytes and cannot be interchanged. Diagnostic traces are drained or discarded on restore and are not serialized.

The canonical format is `TLQS`, version byte `1`, a complete/durable mode byte, ordered length-delimited sections, and CRC32. Decoding checks all section counts, source value domains, unique primitive row IDs, sequential instance IDs, valid fault operations, canonical Number/NaN encodings, exact end of input, and byte-for-byte canonical re-encoding. CRC32 detects damage; it does not authenticate a checkpoint or establish that its state came from valid transactions.

Limits are 1,024 collection rows, instances, scheduled failures, and entries per source array/object; 8,192 nodes, 1 MiB aggregate text, and depth 24 per `SourceValue`; and 16 MiB of encoded owner state before CRC. `SourceValue` owns trees rather than aliased object graphs; the live instance reference is represented by its explicit handle, and reference-valued active-pair entries retain their distinct original-entry identities during slice/filter processing. The checkpoint parser charges node and text budgets before allocating a child or string. Book JSON uses a private Number-preserving parser with the existing 64 KiB input, 4,096 node and depth-16 bounds; it reuses the existing string decoder. Rendering retains its existing 64 KiB bound. Duplicate JSON keys keep their first position and last value; integer fields enumerate in source order. Standalone surrogate strings and JSON capacity limits produce a local rejection with rollback, not a successful empty book. Source operations outside these explicit bounds return a local error, without substituting another economic result.

Strings are UTF-8 scalar strings. Source keys whose cache behavior would require JavaScript prototype mutation (`__proto__`), standalone UTF-16 surrogates, and unsupported numeric/JSON representations are outside this domain. The comparator reproduces the source's final-three-character ranking, then the observed ordinary Node English-locale ASCII fallback and transaction-ID tie break. Non-ASCII locale collation is explicitly unsupported. This does not introduce a portable Unicode collation claim that the original locale-dependent source itself does not supply.

## Verification

`tests/contract_queue_source_owner.rs` executes the entire hash-pinned `orderbook.js` in Node with the genuine archived BigNumber 9.3.1 dependency. The nine queue/book methods remain unchanged. Only the declared database and out-of-scope placement capability are supplied. Its 117 actions compare returned/thrown outcomes, complete documents and every instance cache, ordered database calls, exact placement arguments, and earlier external capability effects. Every action is followed by complete checkpoint restore and continued execution. Genuine source parse cases include root/nested negative zero, positive/negative overflow, signed underflow, duplicate keys and numeric field ordering; their Number bits also survive checkpoint restore.

An independent test also projects 57 successful enqueue calls from the frozen genuine raw18 composition fixture. Starting from each actual preceding database state, it reproduces the queued parameters, returned value, all resulting `orderBooks` rows, and ordered queue storage calls. This includes the original logic argument shifts; it does not replay recorded matching responses as Rust authority.

The corpus covers full parameters and IDs, repeated IDs, token intermix, future and NaN heights, case/punctuation/address/transaction-ID ordering, malformed data, fresh versus stale instances, durable restarts, and early and late database or capability failures. Additional tests cover absent capabilities, distinct complete/durable modes, every-byte corruption and truncation, valid-CRC count/ID/trailing-byte splices, admissible nested count boundaries, incremental aggregate-node/depth/text rejection, and local capacity rollback including scheduled failures.
