# Selected-contract source reads

`EconomicSourceReadV2` supplies three healthy owner-backed dependencies used by
actual Types 18 validation. Raw-source owners enable these reads with
`ConservingSequentialV2`; source-free typed owners also support them. Existing
raw-source `ConservingSequentialV1` owners continue returning UnknownDependency
for precisely these three calls, preserving their recorded receipts. The source pin remains
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`: `marginMap.js` methods
`loadMarginMap` (line 24), `getPositionForAddress` (line 2076), and
`contractRegistry.js` `getInitialMargin` (line 504).

The supported domain is the economic owner's selected numeric contract ID.
String IDs are rejected, and other contracts remain unavailable rather than
receiving invented empty positions. Source storage distinguishes numeric and
string series IDs through JSON.stringify, even though getContractInfo uses
parseInt. This bounded projection does not claim every source identifier form.

## Pending instance and position reads

`Margin.loadMarginMap` returns a Pending immutable instance-handle projection
with the actual `seriesId`. Source's native Map and executable methods cannot
be represented as SourceValue, so the projection does not fabricate a Map or
second store. The selected position owner remains `EconomicState.positions()`.
No mutable margin state is created by validation.

Source getPositionForAddress independently reloads its requested contract map;
it does not read positions from its receiver's previously loaded map. The
backend likewise reads the actual selected owner for each getter call. It
first finds an exact stored account, then permits case-insensitive fallback
only when the query starts with lowercase `ltc1`, `tltc1`, `bc1`, or `tb1`.
Stored owner accounts are ASCII, and insertion order remains the existing
MarginMap order.

An existing position exposes its actual address, signed contracts, margin,
and unrealizedPNL fields for this validation slice. Other position methods and
fields are outside this read projection. A missing position returns exactly
the source three-field object `{contracts:0, margin:0, unrealizedPNL:0}` rather
than serializing the richer typed Position default. It does not add an address
or persist a flat row. Non-string addresses retain the asynchronous source
startsWith TypeError.

`Margin.getInstance` and `Margin.margins.get` remain unavailable. These reads
do not silently enable unrelated transaction types or their state mutations.

## Actual contract margin and numeric boundaries

`Contract.getInitialMargin` uses the actual accepted contract catalog; the
static deployment initial-margin parameter is not a substitute. A missing
selected definition produces PendingThrow with the source Error message
`Contract info not found for contract ID: <id>`.

The existing catalog implementation preserves source operation order:

- Linear: notional times price divided by leverage, eight-place half-up,
  then the source Number boundary.
- Inverse: notional divided by price and then leverage, twenty-place half-up
  divisions, eight-place ceiling, then the source Number boundary.

The catalog also retains source's zero-leverage fallback to ten. The healthy
funded owner accepts valid positive contract parameters; the read supports the
positive finite Number prices supplied by raw type 18. Nonfinite, zero, and
string price forms outside this slice fail explicitly.

Position Number projections support both signs. They require the actual
Decimal to survive conversion to source Number text and back unchanged. An
unrepresentable funded value returns UnsupportedNumericDomain instead of
rounding owned contracts, margin, or PnL into a different validation fact.
The calculated initial margin already crosses the source .toNumber boundary
inside the catalog, before being returned as a Pending Number.

## Source behavior and remaining graph

Actual validation still computes reducing/flipping direction from stored
signed contracts. It skips the first opening-margin check for a pure
reduction, charges only excess quantity for a flip, and can subtract the ceil
of a real shortfall divided by initial margin from params.amount. Its final
balance call retains the original total and does not itself reject. These
source quirks are not repaired by the read adapter.

The healthy typed owner has no source NeDB failure document. Source's caught
load/parse failures can resolve to undefined; getDatabase failures outside the
try can reject. Those storage-failure paths are not simulated as successful
empty maps. Native option/synthetic helpers, other margin methods, broader
identifier domains, and full source write orchestration remain separate work.

Source Logic's type-18 flag plumbing also differs from the current conserving
order mapping: isLiq receives decoded reduce, reduce receives post, post
receives stop, and stop receives block. Owner-backed reads alone establish no
claim of complete literal source write or settlement parity.

The explicit V2 policy prevents capability expansion from silently changing
old raw type-18 DependencyUnavailable receipts. Full raw replay still binds
the independently expected policy and recomputes every receipt and owner
transition. New reads do not imply unchanged historical acceptance.

`tests/economic_validity_contracts.rs` exercises actual funded positions,
lookup/default/error semantics, dynamic linear/inverse margin, genuine raw
Types reduction/flip validation, checkpoint reads, immutable custody, and a
real funded position outside the lossless signed Number domain. Full raw matched
execution belongs to the combined economic source integration tests.
