# Retained Main consensus composition

This candidate composes af213b04 (Root335) with actual consensus/catchup and the
existing running-Service Kernel. It is an explicit source component, separate
from Mode4/WAL. The original receiver-local caller descriptor bytes/API stay
unchanged. Source12/35 and original Source13/38 fixtures stay byte-exact.

Apply prepare.py functions to CURRENT source text, preserving Root's removal of
the two unused test-transport helpers. No full-file replacement is required:

1. txindex_extract_continuation_impl.rs: prepare_extract_owner.
2. txindex_extract_continuation_tests.rs: prepare_extract_tests.
3. source_main_native_catchup_continuation_impl.rs: prepare_catchup.
4. source_main_native_service_block_impl.rs: prepare_block.
5. source_main_native_service_runtime_impl.rs: prepare_service.
6. source_main_native_service_fields_impl.rs: prepare_service_fields.
7. source_contract_runtime.rs: prepare_runtime.
8. source_main_native_session.rs: prepare_session, prepare_session_tests.

Copy the five src/ leaves. The existing source_main_retained_caller_test_transport
is reused without replacement. prepare_component.py writes review outputs ONLY
under this stage. It verifies exact old catchup/block/extractor execution bodies
and derives the borrowed Service constructor from the actual current constructor.
Normal constructors still create real invocations. Their Some-invocation finish
path remains the original actual finish; only the new None-invocation borrower
maps the real computed result without leaving somebody else's queue. The old
Service visitor emits no additional field when borrowed_invocation is false.

Public composition:

    SourceMainNativeRetainedCallerV1::into_retained_consensus()

The new component consumes the already-moved caller and retains its SAME actual
Session, Replay, index, invocation, queue, detached child and all effects. The
new Service frame is continuation metadata; no second financial runtime, ledger,
index, prepared-plan constructor, state importer or expected-result input exists.
It drives the actual shared catchup/financial rows/settlement and original Kernel
one turn at a time, exposing the genuine consensus caller fulfillment before
further discarded incoming work. The original Main header remains held while a
detached child exists. Once the actual caller AND that child finish, the ONE
original header finishes; the component still retains the whole financial owner
and all effects. Later live blocks use their existing real headers on that same
owner. There is no completed Session/head/checkpoint export.

advance_consensus and advance_detached are distinct selections. The latter must
name the actual detached source job, including the ready interval after one DB
callback resolves and before the next DB call executes. Append and resolve name
an exact actual operation; application never supplies a source return. A retained
attempt is armed before IO, including an operation already attempted in the
consumed caller. Separate verified Post resolve resumes only that actual child.
Attempt/recovery ambiguity has no retry or new-owner escape in this component.

New pending operations bind kind, callerScope/sourceJob/methodScope/
operationOrdinal/Awaited-or-Detached, actual prepared read context when present,
and a sealed actual-origin route. That route is constructed ONCE from the actual
retained entry Session/head/physical Before, immutable native origin, actual
foundation bytes/head, declared generation and the retained invocation's actual
Kernel generation/target. It is not reconstructed from current Post. These hashes
bind routing/context, not actor or UTXO authority. Correct future no-index recovery
must move the same route unchanged. No public route constructor/decoder exists.
Old af/335 logical selectors remain receiver-local; two equal old selectors from
different actual stores are deliberately refused by the new route profile.

Bounded competing-write admission: an unresolved detached native plan may coexist
with the actual empty-tx-index consensus path, so genuine before-apply/applied-
return-held final-save cases can fulfill Initialize correctly. No second native
plan/live dispatch is admitted while that competing write is unresolved. Financial
catchup with successful native validity writes is admitted when there is no other
pending native plan. The Core tests cover it with a raw Main0 row, real financial
execution and same actual held Post. Arbitrary concurrent native publications,
native reopen/supersession/escrow acquisition, caller WAL, cold caller replay and
broader startup/import recursion remain explicit follow-ons. Existing Mode4
continues to select only its original direct live method.

Checks completed here: one new actual SourceOwner/Journal ready-job control, all
124 copied actual source/metadata controls pass (one ignored), strict Rust1.90
Clippy, pure transform/derivation equality and formatting. FIVE actual-Core tests
are formatted but UNCOMPILED here: before-apply and applied-held Initialize
fulfillment, detached rejection, nonempty actual financial/native catchup and
two-owner equal-logical-selector refusal. Parent owns all Core/Cargo/commit gates.

## Actual Root gate

Five new actual consensus controls, one real source-job control and three independent caller controls pass. Library520/fourignored,44 public checks and the seven old inline/six Mode4 controls pass. All-target Clippy denies warnings; Cargo fmt,291 included files and inventory pass. Counts overlap. The first Core run exposed the constructor rejecting an Initialize frame whose genuine detached child had moved it from ConsensusReady into Detached. Root extended only this new readiness guard, retaining the real invocation/completed Extract promise and refusing local/native stops; no PC/source action was repeated. A second test incorrectly expected an absent field from the unchanged old catchup visitor. The test now checks the actual borrowed marker and absence of all child-invocation fields, retaining the parent-invocation check. Original source/reference data and old visitor bytes stay unchanged; failures and exact deltas are retained.

The component executes real financial/native catchup and exposes actual Initialize fulfillment while the final save remains held, within the documented competing-write fence. It retains the same owner and finishes the one header only when caller and child finish. New origin-route keys bind context, not authority. Durable caller WAL/recovery, cold caller replay, arbitrary competing publications, full startup and authenticated proof composition remain open. Full all-target baseline remains2,142Rust/239numeric. Verification: `fixtures/reference_fidelity/retained-main-consensus-core-integration.json`.
