# Conditional recent lifecycle witness (TLSW3)

`SourceRecentLifecycleTransitionWitness` is a separate opt-in envelope around the actual `SourceContractReplay` for `LowerLifecycleV7` and `MainCallableV8`. It retains the complete before journal, reconstructs that journal with the existing replay codec, and executes the exact original suffix through the existing source runtime. No supplied current-state projection or financial verdict becomes execution input. Existing TLSW1/TLSW2 formats, APIs, admission rules and V8 refusal tests are unchanged. The runtime default stays V2. A future V9 policy is refused by this profile.

The result is conditional source execution evidence. Imported rows, raw RPC documents, actor claims, activation data, environment settings, module order and failure schedules in the before history are supplied facts. Successful verification establishes neither their authenticity nor L1 inclusion, chain acceptance, custody authority or complete block extraction. A hash does not turn any such assumption into authorization.

## Required independent expectations

The consumer must supply every field of `SourceRecentLifecycleTransitionExpectation` independently of the untrusted witness:

- Exact policy and scope.
- Source/dependency/numeric descriptor.
- Before head and state commitment.
- Before module entry and, for V8, activation profile and lock.
- Exact ordered command commitment.
- After head and state commitment.
- After module entry and, for V8, activation profile and lock.

Unlike TLSW2, after-head and both state commitments are mandatory. `source_recent_lifecycle_state_commitment` and `source_recent_lifecycle_entry_profile` read the actual retained replay for a consumer maintaining its own history. Calling them on a replay reconstructed solely from the untrusted witness does not provide independent expectations.

The descriptor fixes source commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, the existing `tl-source-reference-db-rpc-clock-rng-numeric-boundary-v1`, numeric domain `tl-source-number-bits-bounded-decimal-bignumber-v1`, and dependency domain `tl-source-owned-db-data-only-rpc-clock-rng-v1`. These names identify bounded Number-bit/Decimal/BigNumber behavior and the existing owned query/clone/DB/RPC/clock/RNG adapters. They do not promise arbitrary JavaScript numeric inputs, cross-version BigNumber equality or authenticated RPC facts.

## Scopes and admission

`RawTransactionSlice` admits `SetHeight`, exact nine-argument raw commands, and the existing queue, fee, reward, clearing and withdrawal continuations. V7 raw admission is 1, 8, 11–35. V8 additionally admits 2, 3, 4, 7 and 9. Raw 0, 5, 6 and 10 are not admitted. This is a static command gate over the already-owned runtime graph; an admitted command can still be outside the runtime's numeric/dependency domain.

`SourceMethodSlice` additionally admits the existing literal Logic wrappers for those graphs. It inherits the existing V6 method families and adds the V7 Property/Oracle wrappers, and only under V8 the Send/Commit/ClearList wrappers. Direct balance mutation, Margin writes, arbitrary owner methods, imports, activation setters, entry switches, storage-failure injection and adapter configuration are not suffix commands. Necessary input/bootstrap commands must already belong to the independently pinned before history.

`MainCallableSlice` requires V8 and source-registry activation at both endpoints. It admits the callable `processTxSet`, `processTx`, `blockHandlerMid` and the four processed/track-height read/write methods, with exact public call arity, plus `SetHeight`. It does not admit daemon startup, block-begin/index construction, catch-up, RPC polling or reorg services. The selected Main witness evidence exercises kinds 18/32. The broader V8 Main guard admits additional lower kinds, but those paths have not been inferred to have witness coverage from these selected tests.

The standalone V8 `declared-answer` entry remains available only outside Main scopes and is explicitly pinned with its lock transition. Main never accepts that entry. Module entries are limited to the existing `main-subgraph`, `types-first`, `tally-first` and `volume-first` choices. Endpoint profiles are read from the actual reconstructed state and compared with the independent expectations.

TLSW3 uses a drained endpoint boundary. A pending Main height job or Tally read job returns `PendingWork`; the verifier does not silently omit it or claim its continuation ran. Existing completed callbacks, rejected continuations, retained prefixes, receipts, caches, aliases and state fields remain covered by the actual replay state commitment.

Source throws and their completed financial/storage prefixes are valid source outcomes and remain visible in receipts. A suffix `LocalError` is refused as `UnavailableExecution`: TLSW3 does not promote a journaled local capability or numeric refusal into executed JavaScript evidence. Before-history local refusals remain part of the pinned history.

## Encoding and limits

All framing integers are big endian:

1. `TLSW\x03`, literal owner 1, explicit profile 3, scope 1/2/3, exact replay policy 7/8.
2. Forty source-commit bytes and three 16-bit-length descriptor strings: boundary, numeric domain and dependency domain.
3. Before and after entry profiles. Each has tag 7/8 and a bounded one-byte-length module entry. V8 additionally contains activation tag 1/2 and an exact Boolean lock byte.
4. Before-head count/hash, before-state hash, and the length-prefixed complete TLCR checkpoint.
5. Suffix count and the existing canonical diagnostic command frames.
6. Exact command hash, after-head count/hash and after-state hash.
7. Domain-separated SHA256 and CRC32.

Command hashing uses `TradeLayer/SourceRecentLifecycleTransitionCommands/v3/`, policy, scope, both fixed numeric/dependency domains, count and each original length-prefixed frame. Envelope hashing uses `TradeLayer/SourceRecentLifecycleTransitionWitness/v3/`. Original Number bits, negative zero, argument omissions, value types, repeated occurrences, source flags and ordering are retained. Repaired self hashes cannot replace the consumer's independent pins.

Limits reuse the real replay codecs: 4096 total before-plus-suffix commands, 4 MiB per diagnostic frame and 32 MiB aggregate checkpoint/envelope bounds, with the existing source-value and runtime budgets. The verifier checks framing, descriptor, policy, profile and independent pins before replay; it does not deserialize a supplied current state. This is bounded full-history replay evidence, not a succinct proof or a resource benchmark.

## Verification scope

The staged test reuses the complete existing original-source Property, Oracle, Main, ClearList, Send/Commit and raw3 corpus environments. Its replay wrapper exports and verifies an admitted command before applying the original outcome/state/effect assertions. Other metadata/bootstrap calls remain history. The verified baseline contains 314 source-observed witness slices. The source outcome, persisted tables, live caches, activation owner, Margin/Orderbook identities, ordered RPC/DB effects and all six reached Main postphases remain checked by the corresponding existing observers. The two unsupported Main startup methods are excluded exactly as in the original corpus; no source poststate is imported to force later equality.

| Corpus and profile | Source calls compared | TLSW3 slices | Input resets | Local refusals compared |
| --- | ---: | ---: | ---: | ---: |
| Property, V7 | 109 | 82 | 26 | 1 JSON-domain refusal, excluded from source matches |
| Oracle, V7 selected populated scenes | 58 | 44 | 24 | 0; composite actor scene excluded |
| Main, V8 source-registry | 18 | 18 | 13 | 0; seven unavailable source calls separately excluded |
| ClearList, V8 declared-answer, both address libraries | 206 | 60 | 40 | 0; object-key scene excluded |
| Send/Commit, V8 declared-answer | 82 | 76 | 33 | 1 unbound-RPC refusal, excluded from source matches |
| Raw3, V8 source-registry | 34 | 34 | 8 | 0 |

Source calls and witness exports are different counts. The ClearList number combines two real-library runs, each with 103 compared invocations and 30 exports. Oracle also executes 11 explicit input frames; ClearList executes six per library; Property has one seed; Send/Commit has four seeds and one explicit binding frame. These are not counted as source invocations or witness slices. Main's 39 observations contain 18 compared calls, 13 resets, seven unavailable calls and one import boundary; this does not become 39 Rust source matches. The independent `scripts/recent_lifecycle_witness_command_pins.cjs` assembles five digest pins directly with Node Buffer framing and crypto, without using witness bytes or the Rust encoder.

Validation is performed against a copied, hash-pinned actual Rust 1.90 runtime library. Only the new test/replay helper is compiled in the work lane. This does not establish that a freshly integrated whole-library Cargo build, Clippy or the complete Rust suite has passed; the parent integration owns those gates. Final counts and hashes belong in the accompanying manifest and validation record.
