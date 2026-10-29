# Ordered bootstrap Promise continuation v2

Explicit namespace TL-MAIN-ORDERED-CALLABLE-PROMISES-13-2 extends the genuine
twelve-scene 8a9d20 source corpus. It uses existing policy12; no policy or wire
magic is allocated. The old profile, its fourteen comparisons, old dispatch,
default bytes and proof fences remain intact. V1 metadata has no additional
serialized field unless this new profile is selected.

The same Core owns the production index, client binding and real Main method.
Only metadata job/handler/error capability state is added. There is no Runtime,
Ledger, Journal, financial snapshot or per-job financial clone in that state.
Configuration still calls the existing production validator and setup with
eager static initialization disabled. Actual import/constructor and client
assignment use the same private TxIndex owner continuation methods.

## Actual source continuation

The original VM async initialization awaits a host-realm native client Promise.
The new FIFO models its thenable-assimilation job, host-client bridge reaction,
actual init resumption, and outer init-IIFE/getInstance await resumption. It is
an algorithm over those source jobs, not a case-ID schedule or supplied result
backend. A deferred client may settle before or after assimilation installs its
reaction; that distinction changes which job is enqueued, rather than changing
an observed source field directly.

HostAwaitCheckpoint appends a sentinel to the actual ready FIFO and executes
only preceding jobs. Jobs they enqueue remain after that sentinel. On the
executed deferred-success scene, the first checkpoint still has unassigned
clients, the second assigns both clients but retains both outer awaits, and
the third resumes the outer promises. Full DrainMicrotasks and EndTurn drain
the finite ready queue. EndTurn then emits genuine pending unhandled and
rejectionHandled notifications once, retaining the established global history.

AttachIndexHandler registers a unique bounded catch registration label. Each
actual source catch reaction runs once; multiple catches are retained. Attaching
before notification suppresses the original unhandled event. The first late
registration schedules one rejectionHandled notification; later registrations
still run their callbacks without scheduling that notification again. A catch
on an already fulfilled Promise retains its pass-through reaction as a pending
job even though its rejection callback is not invoked.

Configuration accepts data-only rejectionObjects with unique IDs and explicit
rejectionRef client capabilities. The same declared object is interned once
when first rejected and remains one error reference through both outer promises,
catches and notifications. Separate identical error fields retain different
references. There is no error-text matching. These IDs identify externally
declared Error capabilities in this bounded epoch, never financial state or
executable arbitrary callbacks. Unknown/duplicate IDs and answer-like fields
refuse before setup. Source steps contain only caller controls/settlements.

The original real Main height/RPC path executes; the v2 raw-control glue is
derived from the captured v1 implementation and calls the same methods. No
validity, actor, balance, accepted flag or business poststate drives execution.

## Ownership, replay and compatibility

While an inner/outer/catch job or rejection notification remains, checkpoint
and NativeMainOwners adoption continue to refuse. Client assignments alone
cannot close an outer Promise await. A failed adoption returns the same moved
financial/index owners; remaining actual raw controls finish on those owners.
Closed raw commands reexecute from the original foundation, rebuilding shared
reference relationships from raw declared capabilities rather than importing
metadata or financial snapshots. The existing legacy-witness refusal remains
present for both profiles. No node/economic/authentication admission changes.

The v1 metadata helper is independently compiled alongside the new helper;
absent-v2 serialization is byte-identical in the actual standalone tests.
The Parent's property_catalog test-reader correction is preserved. Existing
old dispatch goldens and unknown-configure refusal test are not replaced.

## Evidence and remaining gates

The immutable original corpus executes unchanged full TxUtils/TxIndex and the
whole original Main class with genuine numeric/dependency profiles. Both
numeric checks pass twelve scenes/143 observations, plus byte-exact original
fourteen-scene/99-observation checks. The v2 fixture derives 142 source
projections; the honest full-Main reOrg refusal remains a separate boundary.
Raw controls/configuration alone drive Core; expected projections are assertions.
Visible traces compare request/caller/catch/notification ordering and original
shared error identities, in addition to the existing initializer field checks.

Six actual Rust1.90 metadata tests pass, including alias prevalidation, separate
equal errors, roundtrip identity, pending outer closure, job bounds and v1 byte
compatibility. They do not execute private Core. Fifteen new actual-Core tests
are proposed: all twelve complete source scenes plus pending-owner recovery,
profile/input refusal and legacy witness fencing. Core execution and full
Clippy remain PENDING Parent; parsing/formatting is not reported as a Core pass.

The explicitly observed Node24.22.1 host/VM native-Promise profile does not
claim arbitrary thenables/callback bodies, general mutable thrown graphs or
full Rich14 transport. Full Main/reOrg/global IIFEs, original client.js network
setup, startup process-death/WAL replay and full initialize/timer execution
remain separate. This proposal completes these twelve finite source scenes
without treating their genuine behaviors as unsupported to obtain green tests.

## Parent Core integration

All 37 startup Core controls pass (22 retained controls plus 15 new), including every one of the twelve new source scenes and all 142 compared cutoffs. Both genuine numeric profiles execute 12 scenes/143 observations; the same observer also reproduces the original 14 scenes/99 observations in each profile without changed bytes. Each corpus includes its explicit unavailable full-Main boundary; those are not successful global-IIFE observations.

The combined suite passes 454 library tests and 44 public compatibility checks; Clippy warnings denied, Cargo fmt, 240 include-format files, inventory and supply-chain checks pass. V1/default serialization, dispatch and proof/adoption fences remain. Root changed a newly written cross-profile test to expect the existing outer UnknownDependency refusal, leaving production behavior and unchanged-state assertions intact. Projection code has explicit UTF-8 IO and a unique inventory filename; source-derived projection logic and all expected source cutoffs are unchanged.

Arbitrary callbacks/thenables, wider full-module startup/import domains and process-loss startup recovery remain unvalidated. These bounded class/Promise cases do not widen existing conditional-witness or actor/chain authentication scope.
