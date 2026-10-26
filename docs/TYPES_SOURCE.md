# Complete source Types decode orchestration

This integrated source port executes the pinned `Types.decodePayload` orchestration over the
existing complete historical decoder and the new complete `validity_source`
diagnostic engine. It replaces no strict wire parser or economic owner. The old
`legacy_types_audit` route inventory remains useful metadata; it never performed
this decode/mutate/validate work and is reused rather than duplicated.

Source: `tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, complete
`src/types.js`, LF SHA256
`a49c1b1f846fe3eab91f4434c9f016586ffddf1bb3fb4492516a065df396a0e1`.
The existing unchanged archive `fixtures/legacy_types_source_types.txt` is reused.
No second archive or substitute source excerpt is introduced.

## What was missing

The existing APIs already implement active historical codec bodies, later duplicate
export shadows, malformed numeric behavior and missing exports. Existing Types
inspection only classified what a route *would* do. It did not call a decoder,
mutate its returned object, resolve outputs, construct nested parameters, invoke
validity with its actual arguments or adopt the validator's eventual result.
The existing wire composition corpus used identity validity and compared a coarse
Types classification, rather than executing this Rust orchestration.

`types_source` now performs those operations. `encodePayload` remains covered by
the existing complete encoder API and generic concatenation characterization;
this increment does not introduce a duplicate encoder. The reference-address
helper is also exposed for direct bounded diagnostic use.

## APIs and async semantics

```rust,ignore
decode_payload_unverified(arguments: &[SourceValue], backend: &mut impl SourceBackend)
    -> Result<TypesEvaluation, TypesError>

decode_with_validity_unverified(arguments: &[SourceValue], replacement: &mut impl TypesValidity)
    -> Result<TypesEvaluation, TypesError>

resolve_reference_address_unverified(reference: &SourceValue)
    -> Result<SourceValue, TypesError>
```

Argument order is exactly `(txId, type, marker, encodedPayload, sender, reference,
senderAmount, referenceAmount, block)`. Missing arguments become `Undefined`;
up to sixteen supplied arguments are retained, with ignored extras preserved.
The default entry actually calls `validity_source::evaluate_unverified` through
`SourceValidityAdapter`, using the supplied dependency backend. It never silently
substitutes an identity validator. A replacement validity adapter is explicitly
named and its observations are dispatch-only evidence.

The source function always returns a Promise. Rust records its eventual settlement:
`TypesOutcome::Returned(value)`, `ReturnedError(exception)` for a fulfilled Error
object, or `Rejected(exception)`. These must not be collapsed into the same error
channel. Local capacity/representation/backend failures remain `TypesError`.
`TypesValidity` settlements record immediate versus pending results; the source
awaits both. Source rejections propagate, while unawaited rejected dependencies
remain in the validity engine's `deferred_rejections` metadata.

`TypesEvaluation` retains ordered original argument diagnostics, the actual decoder
method/body/result or exception, the exact validity call, its mutated arguments,
ordered dependency observations, deferred rejections and source global writes.
`diagnostic_json()` includes all of those rule effects, including the actual
Type18 `hasSufficientBalance` implicit global write. Global writes are diagnostic
data; they never change a Rust process global or economic owner.

Both authority methods permanently deny. A source `valid:true` value, pending
Promise, decoded payload or dependency snapshot is no actor/chain capability.

## All 36 routes and preserved defects

The existing exact route inventory supplies all 36 decoder names and validators.
Active historical decoder APIs preserve later duplicate shadows; no earlier
SettleChannelPNL implementation is substituted for the active Base94 method.

* Marker checking uses the source loose comparison; array coercion such as
  `['tl']` can pass. The switch uses strict numeric selectors, so `'7'` remains
  unknown and negative zero selects zero. Observable string conversion in console
  expressions occurs before dispatch, even with a silent logger.
* Decoder results receive sender/txid/block writes in exact source order, preserving
  existing object slots. Type9 writes `sender`, Type27 also writes `blockHeight`.
* Type3 searches reference outputs using strict `vout` equality, preserves null-entry
  and malformed-reference throws, and only invokes validity when both outputs are
  found. Its cold-withdraw branch references an undefined variable, but the genuine
  decoder returns a string/null tag, so that integer-only branch is unreachable
  from genuine wire decoding.
* Type11 resolves string/array/object reference precedence, inserts `referenceAddress`
  and fills an empty recipient before passing the original reference as argument4.
  Type4 additionally passes reference and referenceAmount as arguments4 and5.
* Type15 discards the earlier block-bearing object, decodes a primitive number and
  calls `validatePublishOracleData`, preserving the wrong validator and missing block.
* Type23 adds `channelAddress=sender` and constructs all fifteen ordered `txParams`
  members, retaining undefined values. It does not synthesize missing channel proof.
* Types26/28/29 retain their wrong sender-first validator signatures. Missing
  decoder exports prevent Types28/29 from reaching their validators in genuine
  execution. No argument-order repair is silently applied.
* Types28/29/34/35 retain missing named decoder errors. Argument evaluation still
  occurs first, so a null payload can fail at `.substr` before the missing-call error.
* Types32/33 return decorated decoded data without invoking validity. Commented-out
  validator calls for Types32–35 are not promoted to active checks.
* Validators can replace the returned object, return undefined/null, mutate parameters
  or reject. Post-validation source logs on Types2/3/4/16/23/31 can themselves reject
  when reading nullish `valid`/`reason` or coercing malformed property values.
* The real txIndex three-argument call preserves its shifted marker/type behavior
  and returns Error objects rather than being corrected into a nine-argument call.

## Exact supported domain

Input values use the validity engine's ordered bounded data representation:
undefined/null, boolean, binary64 bits, UTF-8 strings, arrays, ordered objects,
explicit Promise diagnostics and BigNumber text. Cyclic/aliased host objects,
callable properties, proxies, symbols and arbitrary JavaScript execution are outside
this typed domain. Codec bodies retain their existing 4096-byte bound. Borrowed
preflight checks aggregate input nodes/depth/bytes before cloning; replacement
settlement metadata and diagnostic output also have bounded counts/bytes.

Two genuine Base94 error observations contain a **lone UTF-16 surrogate inside the
exception message**. The shared Rust exception type stores UTF-8 strings and the
existing decoder replaces that code unit. This port returns the explicit local
`UnsupportedUtf16Exception` for that narrow case. Both original source observations
remain in the fixture with their exact escaped source error and unsupported-domain
label. They are not counted as exact matches or silently normalized.

## Typed projection into existing economic handlers

`types_economic::map_economic_input_unverified(original_arguments, evaluation,
height, configured_contract)` is a pure typed projection. The configured contract
must come from the actual staged owner's configuration. In particular, the current
`EconomicInput::Cancel` variant has no contract field, so source contract-cancel
targets are checked before constructing it; a cancel for contract2 cannot clear
the sole owner's contract1 book.

The mapper binds original actor/txid/type/height to the actual decoder and validator
observations and requires a returned source result with validity present. It uses
the already evaluated result, never a separate caller `valid` input. Deferred
rejections quarantine the projection. Because the evaluation is a public diagnostic
struct, a caller can fabricate one; only the private consensus stage may treat its
own independently recomputed evaluation as evidence for an unverified candidate.
The mapper itself supplies no authority or custody mutation.

Supported projections are activation; scalar normal send; spot placement; supported
spot/contract cancellation; whitelist/admin/attestation registry actions; plain
managed grants/redeems; representable oracle creation/publication; oracle-based
contract creation; and positive contract limit orders. Amount conversion uses
source Number decimal text and requires exact atom precision, never rounding an
unrepresentable remainder into funds. Existing owner invariants still run.

Explicit unsupported/unrepresentable results cover allocated issuance IDs absent
from source parameters, sendAll needing actual balance projection, multi-send,
colored outputs, UTXO/channel/synthetic/option/settlement handlers, AMM capital/share
semantics, DLC/procedural grant context, unsupported oracle whitelist metadata,
native contract creation, missing expiry/series values, unsupported order flags,
and source cancellation price/side forms without a specified bridge. Type15 is not
repaired into a close-oracle operation. No property/catalog/native balance or
missing authenticated data is manufactured.

## Evidence and integration status

`scripts/types_vectors.cjs --check` executes full unchanged Types, encoder and decoder
archives with genuine archived BigNumber9.3.1/Base94/Base256/converter code. It reuses
the complete validity source harness for source-rule composition. The 1228 observations
comprise 1151 exact dispatch-only cases, two explicit UTF-16 error-domain restrictions,
and 75 full Types-to-validity cases with explicit snapshot dependencies. They cover
all36 routes, mutation/replacement/error/await modes, the existing decoder corpus,
missing arguments, strict references, malformed values and source global/deferred
effects. Snapshot dependencies are not real chain state.

Rust integration tests compare exact tagged values, decoder/validator arguments,
settlements, mutations and complete dependency/deferred/global metadata. A separate
test proves that the default entry actually executes the full validity engine.
Mapper tests cover exact one-atom transfer, original identity binding, creation/order
fields, cross-contract cancellation quarantine and unsupported metadata. The modules
are now exported from TL and connected to the private sequential economic stage.
The [actual-owner backend](ECONOMIC_VALIDITY.md) derives represented reads from the
pending sole owner. The [source consensus runner](CONSENSUS_SOURCE.md) recomputes
Types and validity for each candidate, applies supported commands atomically and
retains rejected evidence. TLECv7 restore repeats those decisions from raw input
history and checks rollback snapshots against the current history prefix.

The default source APIs and mapper remain diagnostics rather than admission
capabilities. Missing property/list/country/xpub/proof state is unavailable;
unsupported commands cannot become economic successes. The source-valid lag-zero
oracle case, for example, maps faithfully and is retained as an actual Registry
rejection. No service or raw RPC observer automatically authorizes this runner.

The full frozen Cargo run passes 1,161 top-level tests plus one subprocess lock
check across 144 executable targets, including all source/mapper/backend tests
and 12 sequential-consensus tests. Formatting, explicit included-file rustfmt,
all-target frozen Clippy with warnings denied, the zero-third-party Rust policy
and 11 mutation tests, and all-target release validation pass. These are local
conformance and consistency checks; historical/authenticated acceptance remains open.
