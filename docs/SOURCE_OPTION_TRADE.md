# Literal type 27 option trades

This include implements the reached `Logic.processOptionTrade` and
`MarginMap.applyOptionTrade` graph from
`tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. It uses the existing
sole Core collections, Tally cache, Channels owner and Margin position arena.
It adds no ledger, registry or persistent receiver context.

The complete unchanged source modules are loaded by the differential oracle.
Relevant LF hashes are Logic
`f09d476e581d38cf7278f1424aadba7266f25e0fbc23763ceba4afa79c5b3e1d`,
MarginMap `ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636`,
and Options `02277caaece5339a9ea0dac15437d14ce3fac8dc5ee92a727a0f544efcdbb703`.
Types, decoder and validity retain the hashes checked by the complete contract
execution module inventory.

## Validation and source receivers

Types decodes type 27, attaches block/blockHeight/sender/txid, and runs actual
option validity. Logic normalizes the ticker and height, then validates again.
Both validations load their own genuine MarginMap instance. Execution loads a
third instance after its preceding Tally writes. Direct Logic calls perform one
validation and then the independent execution load.

`SourceOptionValidityBackend` scopes the validator's implicit `mm.margins.get`
receiver to the actual handle just returned by `Margin.getInstance`. It forwards
all other calls to Core and reads that instance's real cached position objects.
It neither reloads positions nor supplies validity results. A failed load that
returns undefined produces the source `.margins` TypeError; factory errors retain
their original awaited exception.

The existing pure/source validity implementation derives premium, margin,
reduce/flip fields and realized PnL from those real positions, contract metadata,
channels and balances. A positive channel balance can make validation pass even
when the later Tally premium debit fails. No balance is invented to repair that
source difference.

## Actual writes and source defects

Logic transfers premium through actual Tally updates, applies the signed seller
margin transition and realized PnL, and optionally credits/debits buyer realized
PnL. It then applies the primary legs, any valid option combo legs, and saves the
hybrid margin map. The real TradeHistory export has no static `recordTrade`, so
the optional history branch is skipped.

Margin application retains Number arithmetic and the original average-price
formula, including reductions, exact closure and flips. Positions remain hybrid
series blobs with nested option tickers. Existing arena objects are mutated in
place, preserving established top-level aliases. New positions attach to the
actual receiver before the delta call. A later source exception therefore
retains the preceding Tally writes, cached positions and completed delta writes.

The call to `_touchExpiryIndex` includes an extra leading `this` argument. Its
`tickerMeta` parameter receives the ticker string, whose `expiryBlock` is
undefined, so the source guard returns. The three constructor-empty index Maps
remain empty on this actual call path. The include preserves that no-op. The
same V4 batch separately implements correctly shaped direct index mutation,
queries and cleanup in the actual Margin owner, including populated Map/Set
state; see [source option indexes](SOURCE_OPTION_INDEXES.md). It does not change
the extra leading argument in the original option-trade call.

The option delta call also preserves its original argument placement: trade
price occupies the delta's `margin` field and creditMargin occupies `avgEntry`.
The source's wrong receiver after a failed execution load produces
`Cannot read properties of undefined (reading 'applyOptionTrade')` after premium
and margin progress. It is not converted into a local rejection that rolls back
those writes. Local capacity and unsupported-value errors retain the runtime's
separate admission rollback policy.

The combo fallback refers to unbound `tradeContractChannel`; the include retains
that `ReferenceError`. Ordinary representable inputs cannot normally reach it:
validity rejects non-option combo tickers, while a valid parsed option always has
a truthy type. A separate unchanged-module Node probe supplied a JavaScript
input accessor that changed the combo ticker after validation. That genuine
source execution threw after seller amount 1001/available 982/margin 19, buyer
amount 999 and two persisted Margin deltas, before saving MarginMaps. Accessor
descriptors are outside SourceValue and are not claimed as a Rust execution
capability. No supplied validation verdict was used to reach the source branch.

## Evidence and bounded scope

The differential corpus has 117 actions, including 81 calls, in four complete
CommonJS module entry orders. It covers direct Logic and raw 27, repeated
validation, opening/reducing/flipping, call/put option packages, wrong and expired
tickers, primitives/null/undefined, missing series/channel, channel-only credit,
malformed cached options, spread unwind restrictions, direct Margin arithmetic,
and actual factory/read/delta/save failures with retries.

It compares outcomes and exact exceptions, raw parameters, every collection,
sole Tally/Channels/Oracle caches and flags, every fresh Margin instance including
its full positions and index Maps, and ordered DB/UUID effects. The factory-fault
adapter returns non-target promises unchanged so it does not add microtasks to
unrelated source calls. A second test restores three retained source prefixes
and continues genuine retries, including an undefined execution receiver, a
second delta failure and final save failure.

Both tests exercise the integrated V4 runtime and its actual Core and source
owners. The 117-action corpus independently matches complete genuine BigNumber
9.3.1 and 9.1.2 executions. The latter dependency is decompressed from the existing
scaling oracle and checked against raw source SHA256
`20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e`.
This establishes tested equality, not whole-version equivalence.

SourceValue does not represent accessors, functions, arbitrary prototype
identity, named Array properties or isolated UTF-16 surrogates. Existing numeric,
array/object and arena bounds still apply. Nonfinite paths rejected by the
existing validity numeric domain remain explicit local limits. The separate
index-owner stage documents its literal mutation and codec scope; the unrelated
full options protocol is not claimed.

## Runtime and replay integration

`OptionLifecycleV4` enables the new routes and raw27 admission, TLCRv4 and a
distinct bootstrap seed. It inherits global clearing and the V3 lifecycle.
V1/V2/V3 retain their method graphs and checkpoint bytes; construction still
defaults to V2. The runtime's diagnostic policy names the selected graph.

Core includes `source_option_trade_impl.rs`. Its V4 dispatch reaches the actual
Logic, Validity and Margin methods. Correctly shaped `_touchExpiryIndex` calls
use the existing Margin owner; the source's extra leading argument still
returns before index mutation. Raw27 Types validation uses a fresh
`SourceOptionValidityBackend` and then runs Logic's second validation.

The version tests restore an independently captured, separately compiled V3
fixture. Integrated validation is recorded in
`fixtures/reference_fidelity/source-option-lifecycle.json`. These command and
prestate commitments establish reproducible conditional execution; external
actor, chain and historical-input authentication remain separate gates.
