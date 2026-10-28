# Separate V10 Insurance predicate correction

## Root integration status

Fresh Rust1.90 Root builds passed46 focused executions across11 targets,
including both numeric references, actual Main0, all16 mixed native subsets,
the inherited Main routes, and independent V8/V9/V10 byte/refusal gates.
All-target Clippy, formatting and14 genuine source-regeneration checks pass.
Main0 audit snapshots are nonmutating, as documented in
`SOURCE_MAIN_V10_OBSERVER_STORAGE.md`; actual JS bodies and financial outcomes
are unchanged. A populated pre-V11 snapshot has explicit focused-debug
provenance and makes no release claim. The complete release suite is a separate
gate; the last verified full baseline remains V9 b02a045. Source inputs and
state hashes do not establish chain or actor authority.

## Historical work-stage handoff

The immutable mixed-native V10 manifest is
8ccbd4bbd29a4baa6623485ea12fa882a14c20ac9f70b6d3f6083fbbcde73fcc.
This WORK-only correction changes the extracted shared credit helper under
activation_lifecycle_enabled() only. Earlier policies retain their exact former
predicate; no original frozen stage, root checkout or AMM snapshot is edited.

In that stage's source_contract_fee_impl.rs line516, a balance's propertyId is
read with SourceValue.get and conversion failures are swallowed by is_ok_and.
Consequently a preceding null balance can be skipped and an insurance reserve
created. Original insurance.js line57 uses an ordered native Array.find callback
that reads b.propertyId and calls String on it. A preceding null or invalid
String conversion rejects the unawaited deposit before credit/event/snapshot.
Conversely, a match before a later null succeeds and never reads the later row.

prepare_fee replaces that predicate only for V10 with an ordered, short-circuit
property access and source String conversion. It propagates the genuine source
TypeError into the existing unawaited receiver's rejection history. It does not
make the deposit atomic, introduce a ledger, alter other financial prefixes or
change scheduling/metadata owners. No blanket array prevalidation is used.

The new genuine-source corpus has eight frames: four explicit input resets and
four full original Logic.activateTradeLayer(0,10,...) calls. It uses unchanged
Activation/Vesting/Property/Contract/AMM/Tally/Insurance/Consensus and actual
source file hashing. Initial insurance rows are explicit input data. No reserve,
success bit, validation result or expected poststate is supplied.

The cases are null first; a real match before null; a null after an earlier
non-match; and a property ID whose String conversion throws. Both genuine
BigNumber9.3.1 and9.1.2 regenerate identically and pass --check. Fixture SHA-256:

d5fa690daab8de97877c997e1573f930cbb8443f9593eb45e4b67ae7ff58f196.

Two Rust tests are staged and UNCOMPILED, one per genuine numeric dependency.
They reuse the complete pending V10 owner comparator: actual activation/manager,
all collections and order, Tally static versus singleton caches, Property,
ClearList, consensus vector, unregistered insurance behavior, exact unhandled
errors, ordered DB/RPC/file effects and checkpoint restoration. Rustfmt1.90
syntax passes. Root must run actual focused tests and the old-policy regressions
before integration. A passing source fixture is not Rust parity evidence.

The original independent two-case review probe is preserved under
work/review-v10-clearlist/insurance-default.json, SHA-256
c1fdd48960b650daacae0a62555a705ca4fa9c3d0e7ac10f7ae85eee210d10c3.
Its matching locked-version observations
are byte-equivalent after the explicit version label. The expanded corpus is the
machine-readable conformance artifact for this correction.
