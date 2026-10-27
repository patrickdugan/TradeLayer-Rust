# Constructor-owned source option indexes

This V4 slice completes the actual `_touchExpiryIndex`,
`getExpiringTickersUpTo` and `cleanupExpiredTickersUpTo` methods of pinned
`marginMap.js`. The unchanged complete class is
`fixtures/legacy_margin_map_source.txt`, from
`tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, LF SHA256
`ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636`.

Each existing `MarginMapSourceOwner` instance owns its source `expiryIndex`
Map of Sets, `tickerExpiry` Map and `optionOI` Map. Ordered vectors implement
JavaScript insertion order and SameValueZero key equality, including NaN and
normalization of negative-zero keys. There is no additional Margin, financial
balance, Vault or database owner. `margin_option_index_source_impl.rs` is
included inside the existing Margin module.

`_touchExpiryIndex` keeps the source falsey-expiry guard and
`(Number(quantity) || 0)` transitions. Opening increments the ticker's count;
the first opening fixes its expiry until removal. Closing decrements with the
source numeric coercion and removes the ticker only when the count reaches
zero. Reinsertion moves a removed Set member or Map key to the end. Numeric
and string expiry keys remain distinct. Query and cleanup compare
`Number(expiry) <= Number(height)` inside each iteration. Neither coerces the
height when the expiry Map is empty. Cleanup deletes all indexed tickers for
the selected expiry even when their counts exceed one.

The actual `applyOptionTrade` still calls
`this._touchExpiryIndex(this, before, after, fullTicker, meta)`. Its extra first
argument is preserved. With a normal string ticker the expiry guard returns
without indexing; this port does not repair that call. A correctly shaped
direct call uses the existing `{sourceMarginInstance:id}` receiver adapter and
passes the four original arguments unchanged. Its transport is synchronous;
the getter and cleanup are asynchronous.

The new direct mutation is admitted only under `OptionLifecycleV4`. V1 keeps
its existing outer dependency refusal; V2 and V3 keep UnknownMethod. Existing
empty `instance_value` diagnostics retain all three empty arrays. Canonical
owner state omits all three fields when they are empty, preserving prior owner
and replay bytes. Once any index is populated, all three ordered fields are
included in that same owner's checkpoint. Import rejects partial field groups,
duplicate Map keys or Set members, noncanonical negative-zero keys and an
explicit-all-empty alternative encoding. Ordinary `saveMarginMap` writes only
the source positions: loading a new source instance starts with empty indexes
and does not infer them from saved options. An existing live instance retains
its separate constructor indexes.

The enclosing V4 runtime and replay bind policy, checkpoint version and
bootstrap. Standalone owner imports remain diagnostic bootstrap data, not
authenticated execution history. Integrated check results are recorded in
`fixtures/reference_fidelity/source-option-lifecycle.json`.

`tests/source_option_indexes.rs` has two tests. Its complete unchanged class
reference executes **65 observations: 5 resets and 60 calls**, independently
under genuine BigNumber 9.3.1 and 9.1.2. It compares returned values and
sync/async exceptions, every actual table, all live Margin positions and all
three Maps/Sets, pending database faults and ordered database effects. Cases
cover multiple positions, first-expiry retention, removal/reinsertion order,
string and numeric keys, boolean/null/undefined/NaN/Infinity/array coercions,
falsey expiry guards, the real wrong-argument call, position-only persistence,
fresh receivers, and a failed save followed by retry. The three populated
order/restart/failure histories restore and independently replay after each
command. Additional assertions reject ten malformed canonical states and
verify complete rollback of an unsupported identity-key mutation.

The 9.1.2 reference is the archived genuine dependency already used by the
scaling oracle, raw source SHA256
`20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e`.
The comparison establishes equality for this corpus, not all-version parity.
Existing independently compiled V1/V2/V3 checkpoint fixtures are also checked
against the integrated V4 runtime.

Portable index Map keys and Set members use the existing owner's primitive
SourceValue domain. Object-identity keys, functions, accessors and arbitrary
prototype mutation are outside that domain and produce explicit local errors;
they are not converted into source exceptions. Existing instance, collection,
node, text and checkpoint limits still apply. Local domain/capacity errors
restore owner/runtime state and effects. Genuine source exceptions retain their
actual mutation prefix. These tests prove index ownership, persistence and
replay behavior; they do not claim successful expiry financial settlement.
