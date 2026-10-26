# Activation source metadata and bounded integration intents

This module implements the complete callable surface of pinned `src/activation.js` as diagnostic metadata over injected dependencies. It does not activate consensus rules in an economic owner, authenticate an administrator, create properties, issue native tokens, or prove a Bitcoin transaction. Every `ActivationObservation::require_authenticated_admission()` returns `UnauthenticatedInputAndState`.

The reference is commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`; the complete LF archive has SHA256 `268f08438eb80765587b8ce24d719f41de573648e108ef64df557faad870ea0e`. The oracle executes that complete unchanged file. It asserts all 18 prototype method names, the constructor/static `getInstance` surface, and all 36 registry names. No extracted or rewritten function body replaces the reference. This source does not use BigNumber.

## API and state ownership

`SourceActivation::new()` creates the source constructor's empty consensus-vector object and inactive 36-row registry. `execute_unverified(ActivationCall { method, arguments }, backend)` executes one method and returns its source return/throw, complete local state after that outcome, ordered dependency calls and answers, swallowed catch-site/exception-name observations, and unawaited rejected dependency Promises. Values use the existing `validity_source::SourceValue`; undefined, absent properties, signed zero, nonfinite binary64, arrays and ordinary objects remain distinct.

`SourceActivationState` exposes `consensus_vector`, `tx_registry`, and optional `chain`, `test`, `admin_address`. `None` means the own property is absent; `Some(Undefined)` means it was assigned undefined. `optional_field_order()` retains actual first-assignment order, including `chain, adminAddress, test` after a partial initialization. `diagnostic_value()` uses that retained order. Named properties on array-valued rows are retained separately by `array_row_properties()`; `array_properties_value()` exposes them for source comparison while the ordinary array diagnostic and JSON serialization retain elements only. There is no mutable state getter. `SourceActivationSingleton::{construct,get_instance}` return the same owner after its first construction. Both constructor chain arguments are ignored exactly as in the source. Neither constructor calls `init`.

Dependencies implement the shared `SourceBackend`. All factories are explicit adapter handles: their recorded value is an opaque marker; the adapter provides the subsequent method capability. The module awaits source-awaited `Immediate`, `Pending`, `Throw`, and `PendingThrow` answers. A backend `Err` is a local unavailable/domain failure, not a source exception, and is never swallowed or converted into a false activation result.

| Dependency target | Ordered arguments | Result used by the source adapter |
| --- | --- | --- |
| `Client.getInstance` | none | Client handle marker |
| `Client.getChain` | none for a falsy init argument; otherwise that argument | New chain value |
| `Client.getTest` | none | New test value, interpreted by truthiness |
| `DB.getDatabase` | `"activations"` | Database handle marker |
| `DB.findAsync` | `{}` | Array of `{_id,value}` documents |
| `DB.updateAsync` | `{_id:"activationsList"}`, `{$set:{value: JSON text}}`, `{upsert:true}` | Awaited result; contents ignored |
| `Vesting.getInstance` | admin, chain, test | Manager handle marker |
| `Vesting.initializeTokens` | supplied block value | Awaited result; no local token issuance |
| `Consensus.pushLatestActivationToConsensusVector` | none | Awaited result; contents ignored |
| `Timer.setTimeout` | supplied milliseconds value | Explicit deterministic callback adapter |

The timer adapter runs a callback without sleeping or reading wall-clock time. The timer call's return value is unused by the source. An injected `PendingThrow` there is therefore a deferred rejection while the callback still completes; an immediate throw rejects `delay`. This characterizes the injected capability, not platform timer clamping, actual elapsed time, cancellation, or a guarantee about Node's real Timeout object.

## Complete method inventory

| Active method | Implemented behavior and source limits |
| --- | --- |
| constructor / static `getInstance` | Singleton reuse; constructor never initializes chain/admin and ignores its chain argument. |
| `init` | Await client factory, chain, then test; synchronously update admin. A test-read exception retains a newly assigned chain and prior test/admin. |
| `updateAdminAddress` | Exact BTC/DOGE/LTC strings and six literal addresses. Test uses truthiness. Unknown chain retains any previous admin. |
| `delay` | Explicit deterministic timer capability; return is undefined after callback completion. |
| `getAdmin` | Current admin, including undefined before initialization. |
| `saveActivationsList` | Await database and upsert the entire registry as JSON text. Source exceptions are caught and swallowed. It persists no chain, admin or local consensus vector. |
| `loadActivationsList` | Await the result, check numeric `length === 0` before requiring forEach/array shape, take the last matching `_id`, and parse its truthy value. Empty/missing list, syntax error or read exception replaces the local registry with defaults and attempts saving them. Save failure is independently swallowed. |
| `activate` | Default-radix `parseInt`, reload first, then set active/block and a truthy codehash. Type 0 calls the external manager/initializer before changing metadata and returns undefined. Other existing truthy rows return their row. Missing type/NaN returns undefined. |
| `areActivationsAboveThreshold` | Reload, count truthy `active` values, and compare `(active/total)*100 > 90`; empty object is false. With defaults, 32/36 is false and 33/36 is true. Imported registries determine their own denominator. |
| `initializeTxRegistry` | Return a fresh inactive registry without assigning it to the owner. |
| `getActivationBlock` | No reload. Own-property test; return block `|| null`, so numeric zero and other falsy values become null. |
| `isTxTypeActive` | Reload; absent/null row is false; compare `active == true`, preserving coercion such as `1`, `"1"`, `"0x1"` and `[1]`. |
| `checkActivationBlock` | Reload; return a present non-nullish block including numeric zero. A missing row throws, unlike `isTxTypeActive`. |
| `isSenderGenesisAdmin` | Strict equality to the single LTC test administrator, independent of initialized network/admin. |
| `isValidJSON` | JSON.parse-style primitive string coercion and syntax check in the explicit supported JSON domain. Duplicate keys use the final value. |
| `isValidJavaScript` | Source placeholder: true for every supported argument. No JavaScript validation is claimed. |
| `minifyJavaScript` | Source placeholder: return the argument unchanged. No minification is claimed. |
| `getNextTxTypeId` | Source placeholder: undefined. It does not allocate an ID. |
| `saveNewTransaction` | Source placeholder: undefined and no dependency calls. It does not save a transaction. |

All 36 names are public as `REGISTRY_NAMES` and compared to the actual constructor output. In particular source IDs 31/32 are `Batch Move Zk Rollup` / `Publish New Tx`; those literals are retained even where other source dispatch tables disagree. Default row field order is `name`, `active`; later `activationBlock` and `codeHash` follow their actual first insertion order.

## Persistence, partial effects and repairs

This source stores only the latest activation block. Repeating activation can move it backward or replace it with zero, a string, an object, or another supported source value. Repeating type 0 invokes `initializeTokens` every time. Whether that dependency's initialization flag prevents issuance belongs to the actual vesting/tally implementation; a successful metadata observation does not establish funded genesis or idempotence. The manager's actual pinned `getInstance(adminAddress,chain)` declaration does not accept the source activation caller's third test argument. This module faithfully records all three supplied arguments and does not invent behavior inside that dependency.

An injected consensus rejection occurs after metadata mutation and the attempted save; it is propagated while local metadata stays changed. The actual pinned consensus implementation also has its own catch-and-swallow behavior, which is separately owned by the consensus port. A rejecting adapter case does not assert that the unchanged consensus callee would ordinarily propagate its internal errors. The activation object's `consensusVector` is never updated by these methods.

The database load adapter also preserves the source plain-object accumulator's derived `_id:"__proto__"` behavior: an object value can provide inherited `activationsList`, while an own matching row shadows it, even when the own value is falsy. Setting that temporary object's prototype to null removes its inherited __proto__ setter; a later __proto__ row creates an own property and cannot restore inherited activationsList. This is distinct from accepting prototype/coercion keys inside imported registry JSON, which remains an explicit unsupported domain.

The oracle additionally executes the complete unchanged archived `db.js` with an explicit no-I/O Datastore adapter and the real Node promisify/path utilities. It confirms that `activations` is an actual database category, that it has `findAsync`/`updateAsync`, that missing `contractAdjustments` remains undefined, and that repeated initialized `init` does not rebuild categories. The source database's eager initialization and external RPC/bootstrap waits remain database-owner behavior. Fault-injection cases also permit the adapter to commit an upsert before throwing synchronously or rejecting asynchronously. The source save catch still swallows the error, and a later load observes the committed value; an error response is not modeled as proof that no write occurred. These are locally generated genuine-source observations over explicit adapter behavior, not an imported external-worker diff or a durability guarantee.

The activation corpus's in-memory database is explicitly an adapter implementing the source's query/upsert interface, not a claim to reproduce the full NeDB engine or durable I/O.

`RepairedActivationIntent::new(types,block,code_hash)` is the separately named useful integration boundary: nonempty unique IDs 0..35, at most 36 IDs, a safe-integer block, and an optional fixed 32-byte hash. It has no actor/authenticated flag and no Ledger. `requires_native_genesis_policy()` reports whether type 0 needs an independent funded policy. Actual Registry integration retains configured-administrator, prior-height, already-active/replay checks and the sole Ledger. The [activation-history view](ACTIVATION_HISTORY.md) derives accepted origins from the existing complete v7 replay; it never imports this diagnostic class state as authority. The existing Registry's monotonic/no-repeat/prior-height rules are intentional repairs; source `activate` does not enforce them. No trusted restore/checkpoint or economic admission is created by importing source JSON.

Source exceptions commit the source's partial local metadata and increment the action count. Local resource/domain/backend errors discard staged local metadata and do not increment it. This staging is a named local safety repair, not rollback of external effects: an adapter may already have received reads or writes before a late local error, and no distributed atomicity is asserted. Durable integration must journal the complete accepted owner action with its actual state under its existing transaction boundary.

## Bounds and retained unsupported observations

Each owner accepts at most 4096 completed source actions; each call has at most 8 arguments and 32 dependency calls. Shared values enforce depth 24, 8192 nodes, 1024 entries per object/array, and 1 MiB of text per value. JSON parsing is separately capped at 65,536 bytes, 4096 nodes and depth 16. Stringification has a 65,536-byte output cap and uses the shared value depth/node bounds; it does not claim the parser's tighter depth/node budget. Thus accepted serialization is not a promise that a later import lies within the parser domain. Dependency exceptions have at most 256 name bytes and 65,536 message bytes. There is no unsafe code or external Rust crate.

The JSON parser preserves numeric negative zero, accepts binary64 overflow as Infinity, and stringification emits null for nonfinite values as JSON.stringify does. Class methods are strict: writes to truthy numeric/string/boolean rows throw the exact create-property TypeError; type 0 also reaches this error for falsy primitive rows, after the external initializer. Ordinary nonzero activation skips falsy rows. Registry arrays with object rows and array-valued rows retain observed behavior. Array rows can receive named active/block/hash properties; immediate getActivationBlock sees them, saving drops them from JSON, and replacing/reloading the registry clears the sidecar. Missing rows retain source TypeErrors. Scalar JSON overflow is metadata only and never an amount eligible for economic admission.

Prototype-changing/coercion keys (`__proto__`, own `toString`/`valueOf`), Promise/class instances, lone UTF-16 surrogates, and prefixed numeric coercion exceeding 128 integer bits are explicit local unsupported domains. Shared `SourceValue` itself remains broad; these restrictions are private to this activation evaluator. Five `Q` observations retain genuine unchanged-source results outside the Rust boundary: three valid JSON texts rejected locally, an own-toString coercion failure, and an oversized hexadecimal parseInt. They are checked as explicit typed rejections and are not counted as exact Rust/source matches. Resource rejection is not reported as a source false/throw result.

## Differential evidence and reproduction

The final corpus contains 259 scenes, 670 exact action observations, and five retained domain observations. Exact observations compare source returns and thrown names/messages, all local fields, ordered dependency arguments/results/modes, swallowed catch-site/exception names, deferred failures, and named array-row properties. Coverage includes activation of every registry ID, all six network admins, uninitialized/unknown network, all source placeholders, zero/falsy codehash behavior, repeated block replacement, empty/default/imported registries, array registries/rows, strict primitive setters and type-0 call ordering, optional field insertion order, zero-length nonarray database responses, derived __proto__ database-key inheritance, null-prototype setter removal and own-key shadowing, duplicate database documents, duplicate JSON keys, parseInt/coercion/nonfinite/-0 boundaries, strict 33-of-36 threshold, synchronous exceptions and asynchronous rejections. Logs, stacks, wall-clock time and JavaScript reference identity are not part of the observation contract.

`S` TSV rows begin a fresh scene with its database snapshot. `A` rows contain the method/arguments/overrides and complete expected observation. `Q` rows contain a genuine source outcome and the declared Rust domain error. Values are shared tagged diagnostic JSON encoded as UTF-8 hex; binary64 values use exact bits, including negative zero. Headers retain the actual method/name inventory, dependency defaults and real-db projection.

From the integrated repository checkout:

```text
node scripts/activation_vectors.cjs --write
node scripts/activation_vectors.cjs --check
cargo test --frozen --test activation
```

The source module and derived history view are integrated in the actual checkout. All eight activation tests and seven actual-owner activation-history tests pass in the complete frozen Cargo run. The unchanged-source Node check passes all 670 exact observations plus five explicit domain cases. The combined batch passes 1,161 top-level Rust tests plus one subprocess lock check across 144 executable targets, Cargo formatting and explicit included-file rustfmt, all-target frozen Clippy with warnings denied, the zero-third-party Rust policy and its 11 mutation tests, and the all-target frozen release build. Shared helper tests are not counted as activation target tests. These local checks do not establish historical or authenticated chain acceptance.
