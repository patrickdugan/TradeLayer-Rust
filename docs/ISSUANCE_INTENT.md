# Issuance-intent metadata

`src/issuance_intent.rs` provides a bounded metadata store for the three methods
in pinned `src/issuanceIntent.js`. It stores arbitrary ordered JSON objects;
there is no schema of guessed issuance fields. Recording a document does not
issue a token, validate a DLC, authenticate its author, reserve collateral, or
authorize a later operation.

The complete unchanged LF source is archived in
`fixtures/legacy_issuance_intent_source.txt`, from
`tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Its SHA256 is
`dafabd4d8a51e805cac299b33e815a2367ffb1b934f943c4519454a671aec932`.
The oracle also executes complete `fixtures/legacy_db_source.txt`, SHA256
`b92a05fee6652494b72bf6893d9633850c697be773e88b24d62798a8bc8f5e38`.

## Original database failure

Actual pinned `db.js` initializes 32 categories. `issuanceIntent` is absent.
`getDatabase('issuanceIntent')` therefore returns `undefined` after successful
initialization. A valid `recordIntent` fails when calling `updateAsync`;
`getIntent` and `getIntentsByAddress` fail on `findOneAsync` and `findAsync`.
An empty recording selector throws `txid is required` before opening the
database. The oracle executes these failures through unchanged `db.js` with
inert datastore handles and skipped RPC bootstrap. Those dependency handles
avoid filesystem effects; they do not manufacture a successful database route.

Successful functionality is an explicit metadata repair. A separate labelled
adapter supplies the missing category to unchanged `issuanceIntent.js` and
records its actual spread, replacement, timestamp and query behavior. This
adapter is not a claim of successful original NeDB persistence.

| Source method | Functional Rust operation |
| --- | --- |
| `recordIntent(txid, intent)` | `record_intent(selector, &JsonObject, supplied_millis)` |
| `getIntent(txid)` | `get_intent(selector)` |
| `getIntentsByAddress(address)` | `get_intents_by_address(address)` |

## Arbitrary JSON and source field order

`JsonValue` supports null, booleans, finite binary64 numbers, UTF8 strings,
arrays and ordered objects. `JsonObject::parse`, `get`, `iter`, `insert` and
`to_json` allow custom nested fields without exposing the backing vector.
Object keys use JavaScript own-key enumeration: canonical integer indices
`0..4294967294` sort first, while other strings retain insertion order.
Overwriting a key retains its position. Duplicate keys in parsed JSON take the
last value and retain the first slot, matching the source JSON domain.

Recording follows the actual source expression:

```javascript
{ _id: txid, ...intent, updatedAt: Date.now() }
```

An intent `_id` overrides the selector. An intent `updatedAt` is always replaced
by the supplied clock value, retaining the field's existing slot when present.
Updates replace the entire selected document; old optional fields disappear.
The successful write journal keeps the complete original object, including
overwritten values, selector and supplied milliseconds.

All string keys are metadata, including `constructor`, `__proto__`, dotted
names and `$` prefixes. There are no JavaScript prototype effects or query
operators in this store. Actual NeDB rejects many dotted/`$` field names; this
generic metadata repair does not claim backend compatibility for those names.
Top-level primitive/null/array object-spread behavior is characterized by the
source oracle but is outside the functional `JsonObject` recording API.

Number serialization uses the shared exact source-number formatter in
`state_oracle.rs`, including JavaScript's exponent thresholds and shortest
round-trip selection. Negative zero serializes as `0`; unsafe integer text
rounds through binary64. The 270 numeric source cases include exponent
boundaries, shortest/tie cases, subnormals, maximum finite values and fixed-seed
binary64 observations. NaN and Infinity are rejected rather than serialized as
JSON null. Overflowed JSON-number text is likewise rejected. Underflow to zero
is supported. Valid escaped UTF16 surrogate pairs become UTF8; lone surrogates
return `UnsupportedSurrogate`, since a Rust UTF8 string cannot hold them.

## ID and address behavior

An overridden ID does not create a selector alias. Recording selector
`original` with `_id: "alias"` makes `get_intent("original")` return none and
`get_intent("alias")` return the document. Repeating the first upsert fails
`DuplicateId`. Selecting an existing row and changing its ID fails
`ImmutableId`. Every failure leaves documents, journal and indexes unchanged.

Nonstring IDs remain arbitrary JSON metadata, visible through `documents()`.
Their uniqueness uses ordered structural JSON equality. String lookups only
select string IDs; an empty stored ID can be queried although the recording
selector itself must be nonempty. NeDB's multikey object/array ID index quirks
are deliberately not reproduced as storage machinery. In particular its empty
array ID can vanish from ordinary queries; the repaired store retains the row.

The address index matches a string `recipientAddress`, or a string member of
its recursively nested arrays, once per document. It does not descend into
object values. JSON depth/node limits bound that recursion. Updating or replacing a document removes
its former memberships. Results follow stable first-insertion row order;
updating a row retains its slot. Source methods specify no explicit sort.
Genuine NeDB may return its ID-index order instead; that order is observed
separately and is not asserted to equal the repair's stable row order.

## Bounds, atomicity and checkpoints

The supplied clock is a nonnegative JS-safe integer millisecond value. The
recording selector is nonempty UTF8 and at most 1,024 bytes. Limits are 1,024
documents, 4,096 successful writes, 65,536 JSON bytes and 4,096 value nodes per
object, depth 16, and aggregate retained metadata of 8 MiB/65,536 nodes.
Conservative JSON accounting includes a comma per entry and checkpoint framing
headroom. These local resource limits are not historical consensus rules.

Input and resulting spread projection are measured by borrowing their values.
Existing documents plus original journal inputs share the aggregate budget,
so repeatedly replacing one row cannot evade it. Those checks precede object
clones. ID and capacity errors publish no partial document, index or journal.
The address projection is rebuilt from owned rows after a successful write;
there is no second source of balances or issuance state.

`checkpoint_bytes` writes canonical `TLII` version 1 bytes containing all
successful inputs and resulting documents, plus CRC32. Loading bounds lengths
before allocation, rejects invalid UTF8/noncanonical JSON/unknown framing or
trailing bytes, reexecutes every write with its original clock value, compares
all resulting documents, reconstructs indexes, and requires identical canonical
bytes. Rechecksummed inconsistent projections are rejected. CRC and replay
prove internal consistency, not the authenticity of the recorded claims.
No filesystem journal, live database adapter or chain integration is implied.

## Verification

```text
node scripts/issuance_intent_vectors.cjs --check
cargo test --test issuance_intent --frozen
```

The fixture contains 321 source observations: 19 writes, 5 ID queries, 12 address
queries, 3 complete row projections, 270 numeric cases, 9 original-failure or
non-object spread observations, and 3 adapter reset boundaries. Rust tests also
exercise arbitrary nested metadata, ordering, replacement/index updates,
identity failures, bounds, aggregate journal limits, invalid JSON/UTF8,
canonical checkpoints and rechecksummed semantic corruption.

An optional local check executes unchanged issuance-intent source against the
installed genuine NeDB in memory:

```text
node scripts/issuance_intent_vectors.cjs --check --nedb-root PATH/TO/tradelayer.js
```

That check passed ID mismatch/duplicate/immutability, nested-array address
membership/replacement with object values excluded, index order, dotted-key
rejection and the empty-array-ID anomaly. Modern Node removed utility predicates
used by this older NeDB; the optional runner explicitly supplies `util.isDate`
and `util.isRegExp` compatibility predicates. It does not replace NeDB's update,
query or index algorithms, and it does not write its database to disk.
