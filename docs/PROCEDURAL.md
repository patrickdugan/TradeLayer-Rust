# Procedural DLC metadata registry

`src/procedural.rs` ports the concrete template/contract metadata behavior in
`src/procedural.js` at source commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Its complete unmodified LF source is
archived as `fixtures/legacy_procedural_source.txt`, SHA256
`0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e`.

The registry stores metadata only. An accepted issuance or redemption context
means that these metadata fields pass the source conditions. It provides no
authenticated sender, DLC/relay signature verification, settlement proof, token
issuance, redemption, reserve movement or ledger effect. The JS caller performs
other property/admin/balance operations in `validity.js` and `logic.js`; those
operations are not implemented by this module. The existing `oracle.rs` relay
metadata and `dlcOracleBridge.js` call path were reviewed for this boundary.
There is no additional cryptographic verifier hidden behind a successful gate.

## Concrete behavior

| Source operation | Rust operation | Behavior |
| --- | --- | --- |
| `_base` | Owned `ProceduralRegistry` / `records()` | Replaces the shared NeDB database with bounded in-memory metadata; caller owns durability. |
| `upsertTemplate` | `upsert_template` | Writes `_id=template-ID`, type and template ID before spreading caller data. |
| `upsertContract` | `upsert_contract` | Writes contract/template IDs and state before caller data. `None` state means `OPEN`; an explicit empty state remains empty. |
| `getContract` / `getTemplate` | `get_contract` / `get_template` | Queries the prefixed record key, without validating the stored type or internal ID. |
| `_normalizeHash` | `normalize_hash` | ECMAScript whitespace trimming and lowercasing; no hex syntax or cryptographic hash validation. |
| `ensureIssuanceContext` | `ensure_issuance_context` | Checks IDs, provided hash, existing template, published hash, matching hash, existing contract, matching template ID, FUNDED/OPEN state and optional settlement state, in source order. |
| `ensureRedemptionContext` | `ensure_redemption_context` | Checks IDs, existing contract, matching template ID, SETTLED/CLOSED state and optional settlement state. It does not look up the template. |
| `transitionContract` | `transition_contract` | Merges previous data and metadata, then forces uppercased next state. Creates an orphan record when absent and enforces no state-transition graph. |
| `PROCEDURAL_STATES` | `ProceduralState` | Six exact known labels: TEMPLATE, FUNDED, OPEN, SETTLED, CLOSED and DISPUTED. Raw state strings remain supported. |

Upserts use `$set` semantics. Omitted older fields survive in storage, while the
returned write document contains only that call's defaults and data. A subsequent
query can therefore contain fields absent from the returned upsert result.
Returned owned write documents cannot mutate the registry.

Caller metadata may override type, template ID, contract ID, state and other
fields. Transition's final state assignment overrides metadata.state. `_id`
changes reject atomically, including creation, matching NeDB's `model.modify`
check in modifier-based upserts. Metadata is not silently sanitized into a
different contract. In particular, an orphan with no template ID compares equal
to the literal caller ID `undefined`, as JavaScript `String(undefined)` does.
The source permits successful redemption without a published template; this
behavior is preserved and is not a proof of any valid DLC settlement.

Published hash selection uses JS truthiness in `templateHash || dlcHash || hash`
before normalization. A whitespace-only first value blocks fallback and yields
the missing-hash rejection. Empty arrays and objects are truthy, and their string
coercion follows the documented value domain. State checks uppercase without
trimming: `open` is mintable while ` open ` is not. All source rejection reasons
and their ordering are preserved within the accepted domain.

## Typed and resource boundaries

Wire IDs and supplied states/hashes are strings, matching `txDecoder.js`'s
procedural fields. Additional metadata uses `ProceduralValue`: null, booleans,
strings, exact signed JS-safe integers, arrays and objects. Floating point,
undefined metadata values, functions, symbols, dates, custom prototypes and
custom primitive-conversion methods are excluded. In particular, field names
containing `.`, starting with `$`, or equal to `__proto__`, `prototype`,
`constructor`, `toString` or `valueOf` reject as an explicit typed boundary.
The source's dotted `$set` and prototype/coercion behavior is not claimed for
that excluded domain.

The registry accepts at most 4,096 total template/contract records. Individual
texts and field names are at most 4,096 UTF-8 bytes; the generated prefixed `_id`
must fit too, so IDs accepted for writes are at most 4,087 bytes. Documents have
at most 4,096 value nodes, nesting depth 16, and 65,536 aggregate key/value bytes
(integer values count as eight bytes, null/bool as one). These are resource
bounds, not source consensus rules. Both input patches and the complete merged
record are validated before a mutation. Failed additions or updates preserve
all stored records. Context arguments exceeding the text bound return an
explicit `InputLimit` before source checks.

Unicode lowercase/uppercase uses Rust's Unicode tables. Pinned vectors include
ECMAScript whitespace, Greek final sigma and Turkish dotted I. General Unicode
table equivalence across arbitrary future JS/Rust runtimes is not proved;
ordinary ASCII hash/state/ID inputs do not have that table-version dependency.

## Executed source evidence

`scripts/procedural_vectors.cjs` checks the source SHA and executes its complete,
unmodified class inside Node's VM. A minimal adapter provides detached
`findOneAsync` and NeDB-style `$set`/upsert behavior, including the immutable
record-ID check. Expected state, queried documents, successful gate documents
and rejection reasons all come from that source execution. The adapter does
not replace procedural conditions or normalize values for them.

The 154 fixture operations span five isolated registries: all rejection stages,
all six constants, unknown/empty/Unicode states, default and explicit state,
hash precedence/truthiness/case/whitespace, metadata overrides, retained `$set`
fields, nested values, orphan transitions, rolling metadata and immutable-ID
errors. Six Rust tests replay those operations and separately exercise resource
bounds, merged-document rejection, capacity and atomic failures.

Run the offline source check with:

```text
node scripts/procedural_vectors.cjs
node scripts/procedural_vectors.cjs --source-root /path/to/tradelayer.js
cargo test --test procedural --frozen
```

Use `--write` only to regenerate the fixture intentionally. No npm package or
network access is needed for the default archived-source check. These synthetic
source traces are not historical chain transactions. Production promotion still
requires authenticated admission, the surrounding property/token lifecycle,
ledger integration, durable/reorg-safe state and historical acceptance evidence.
