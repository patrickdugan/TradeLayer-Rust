# Owned clearlist.js state

`ClearlistSourceOwner` owns the actual list and attestation documents used by its source methods. `SourceBackend` returns queries against these documents; it does not accept supplied clearlist dependency answers. The owner contains no economic balances. Economic transactions use the enclosing economic owner's one Ledger.

The implementation follows `src/clearlist.js` at `tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, SHA256 `fbdb702e5e89bac113226223f42bf762af13fff40dee4e0c4ab6cd4a50d451e1`. The immutable source is `fixtures/legacy_clearlist_audit_reference.txt`. This module is separate from the corrected Registry and from the earlier typed observational audit.

## Method coverage

All 25 source methods execute against owned state:

| Methods | Preserved behavior |
| --- | --- |
| `createClearlist`, `createclearlist`, `getNextId` | Falsy-ID allocation, JavaScript `parseInt` prefixes, typed IDs, explicit-ID upsert, object/positional alias, discarded URL, original `admin`/`backup` fields. Creation writes the collection without populating the singleton Map. |
| `loadClearlists`, `getClearlistById`, `getList` | Ordered cloned-document loads retain stale Map entries. Numeric and string keys differ. `getList` reads the attestations collection. A failed load is swallowed and may leave old Map entries available. |
| `verifyAdmin`, `updateAdmin` | Primary-admin test, original and newer field aliases, required loaded Map entry, in-memory mutation before the database write, string-key shadow row beside the numeric original. A later load can restore the numeric row's old admin. |
| `addAttestation`, `addAttestationWithXpub` | One address-only key replaces earlier list membership and metadata. Inline empty `xpub:` remains an empty string. Explicit falsy xpub becomes null without inferring an inline prefix. |
| `isAddressInClearlist`, `getAttestations`, `getAttestationHistory`, `revokeAttestation` | Exact legacy/current field queries; root rows count as direct membership when their address matches. History selects current rows and stably sorts descending timestamps. Canonical revoke changes every matching active row, including root rows. Only an empty canonical result enables the unsafe prefix/address fallback, including another list or an already revoked row. |
| `getCountryCodeByAddress` | List 0 numeric coercion; missing/falsy status defaults; string metadata and four object aliases; first truthy alias before trimming; Unicode uppercase; `blockHeight || timestamp || null`; query order; graceful database-error null. Complete original metadata survives each operation. |
| `setBanlist`, `setBanList`, `getBanlist` | Persistent `globalBanlist` document and volatile cached value. The getter reads the array's missing `data` property and returns undefined. Banlist rows also enter the loaded lists Map. A process restart resets the cached default, including `SY`. |
| `deriveAddressesFromXpub`, `isAddressInClearlistOrDerived` | Native public BIP32, receive `m/0/0..19` then change `m/1/0..19`; ignored network argument; xpub-only cache; direct-membership short circuit; inline and registered-root lookup. Invalid keys produce an uncached empty result under the source catch. |
| `storeMerkleRoot`, `registerXpubForRoot`, `verifyMerkleProof`, `buildMerkleTree`, `generateMerkleProof` | Genuine SHA256 of UTF-8 leaf strings and concatenated hex text; initial odd-leaf duplication; omitted siblings at odd upper levels; ordered proof objects; duplicate registration suppression; root-global registrations shared across lists. Loads do not reconstruct registrations. |

The default derivation environment matches installed `bitcore-lib-ltc` 10.10.5: standard xpubs produce Litecoin-main P2PKH addresses, and tpubs produce testnet addresses. `with_source_library(SourceLibrary::BitcoinFallback)` models the source choosing `bitcore-lib` when loading Litecoin fails. Parsing an invalid xpub never switches the selected library. Both modes use the same native public derivation implementation; no derived-address answers are injected.

## Integration API

```rust
let mut clearlist = ClearlistSourceOwner::new();
let outcome = clearlist.evaluate(&SourceCall {
    method: "addAttestation".into(),
    arguments: vec![list_id, address, metadata, block],
})?;
let complete_snapshot = clearlist.checkpoint()?;
let restored = ClearlistSourceOwner::from_checkpoint(&complete_snapshot)?;
```

`evaluate` preserves a source return or source exception in `SourceOutcome`. A local capacity/representation error remains a `SourceRuleError`, so an unsupported computation cannot silently become a negative membership answer. Async source methods return `DependencyReturn::Pending` or `PendingThrow` through `invoke_source`; synchronous crypto methods return `Immediate` or `Throw`. The `SourceBackend` implementation recognizes `ClearList.method` and `clearlistManager.method`.

Source reads can mutate singleton state. `getClearlistById` loads its Map, and derivation fills a cache. A validator that discards a cloned owner also discards those effects; the transaction owner must choose and apply its replay policy deliberately. An enclosing atomic transaction can clone the metadata owner and commit that clone together with the sole economic Ledger. This module itself preserves partial source effects: for example, a rejected second revoke write leaves the first row revoked, and an admin write rejection leaves its earlier Map mutation.

`rows(collection)` exposes immutable full documents. `import_rows` replaces a collection for adapter recovery while retaining singleton state, matching an external database change before the next load. Unknown top-level fields and arbitrary supported nested metadata remain intact. `state_value` exports both collections plus the four source singleton-state components in their observable insertion order. `take_db_calls` drains actual adapter operations including queries, updates, options, and collection acquisition. The trace is bounded and is diagnostic, so protocol operation does not require retaining an unbounded log.

`fail_database_call` injects a named adapter rejection after a chosen number of matching operations. It exercises actual source catch/rethrow boundaries and partial writes. Failure injections and the diagnostic trace are external execution instrumentation: they are excluded from checkpoint bytes and protocol-state equality.

## Persistence and replay

There are two intentionally different recovery contracts:

* `checkpoint` / `from_checkpoint`: the complete replay owner, including both document collections, loaded lists Map, cached banlist, xpub cache, root registrations and derivation-library selection. Use this for deterministic economic checkpoints, rollback and continued replay.
* `durable_checkpoint` / `restore_durable`: only the two document collections that source JS writes to its database. Restoring starts a new process singleton: empty Maps/cache/registrations and the default cached banlist. `restore_durable_with_source_library` selects an explicit dependency environment at restart. No subsequent list load reconstructs Merkle registrations.

Both formats use versioned `TLCO` envelopes, bounded typed binary values and CRC32. Restore rejects corrupt bytes, the wrong recovery mode, duplicate document/map keys, invalid shapes, overlarge inputs and noncanonical reserialization. CRC32 detects accidental damage and supplies no authentication. Collection order, object field order, undefined/null, numeric/string IDs, floating-point bits, and unknown row fields survive checkpoints. `replay` applies a bounded sequence of actual source calls; thrown source operations retain their source side effects.

The source-value domain is dense, UTF-8 data represented by `SourceValue`, with at most 1,024 rows or Map entries per collection, the existing per-value limits, and a 16 MiB owner checkpoint limit. Object-reference Map keys, custom prototypes/callable coercions, named or sparse arrays, standalone UTF-16 surrogate values and binary `Buffer` crypto arguments require capabilities outside that value representation and are not silently converted to supported inputs. Full documents can still contain supported generic objects and arrays. Valid private extended keys are accepted by JavaScript's HDPublicKey constructor but are deliberately outside the native public-only BIP32 capability; they return an explicit local unsupported-domain error. Native arithmetic or retry limits also propagate local errors instead of certifying absence. Local capability errors roll back their attempted call, preserving a valid checkpointable owner.

History containing malformed timestamps can make the source comparator inconsistent: NaN compares equal to every timestamp, including unequal numeric values. For fewer than 64 rows the implementation preserves the observed Node/V8 natural-run and binary-insertion order. Numeric/coercible timestamps support the full row limit, as do histories whose comparator always reports equality. Larger histories mixing NaN timestamps with unequal numeric values return an explicit local unsupported-domain error because their source order depends on the runtime's merge implementation. The source runtime uses [V8's stable adaptive sort](https://raw.githubusercontent.com/v8/v8/13.6.233/third_party/v8/builtins/array-sort.tq); stable sorting alone does not define an inconsistent comparator's result.

## Differential evidence

`scripts/clearlist_source_owner_vectors.cjs` executes the entire unchanged source, verifies its SHA256 and uses the genuine installed `bitcore-lib-ltc` and `bitcore-lib`. The fallback oracle runs in a separate Node process because loading both installed libraries into one process mutates their shared elliptic implementation. The typed-ID database adapter is an explicit clone-isolating model of the collection API; this evidence does not assert byte-level NeDB file compatibility.

The checked-in corpus contains **200 sequential observations covering all 25 methods**. Every step compares the exact return/exception, all document fields, all singleton/cache/root state, and every adapter call. Cases include typed IDs, list0/admin shadowing, cross-list replacement/revoke, root/direct coexistence, stable equal-time and coercing history, country aliases/coercions, stale loads, restart, real derivation/cache membership in both dependency environments, invalid keys, source Merkle proof defects, swallowed reads, rejected writes, and partial multirow revocation.

The focused Rust test also performs canonical complete checkpoint roundtrips after every observation. Separate tests cover dependency integration, complete-versus-durable continuation, corruption/wrong-mode rejection, arbitrary metadata/replay, explicit unsupported domains and fallback configuration recovery.

```powershell
node scripts/clearlist_source_owner_vectors.cjs --module-root C:/PATH/TO/tradelayer.js
& ../../work/cargo-check.ps1 test --offline --test clearlist_source_owner
& ../../work/cargo-check.ps1 -CargoArguments @('clippy','--offline','--test','clearlist_source_owner','--','-D','warnings')
```

The generator ran with Node v24.19.0, actual `bitcore-lib-ltc` 10.10.5 and actual fallback `bitcore-lib` 8.25.47. All six focused tests and the 200 source observations passed in the isolated native test run. Repository integration, enclosing transaction tests and the required local commit belong to the parent task.
