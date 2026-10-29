# Explicit ordered callable bootstrap

`SourceMainBootstrap.configure` selects
`TL-MAIN-ORDERED-CALLABLE-BOOTSTRAP-13-1` before any old production setup or
locked Main/activation execution. Existing `SourceMain.configureProduction`,
the default V2 policy, earlier formats and eager initialization stay unchanged.
That earlier setup path retains its previous initialization-order boundary;
this new entry is required for the source import/constructor ordering below.

The new profile reuses the production validator and the same production index,
client binding, IO facts and Core. Its bounded jobs/error-reference metadata
lives under the existing service object and is absent by default. It holds no
Runtime, Ledger, Journal or financial snapshot. The actual TxIndex owner starts
module import before its independent singleton/static initializer. Its private
continuation helpers update that same owner's existing fields at settlement.

## Controls and observable promises

Both configure and every step require the exact profile namespace. Configure
takes declared client/IO/service facts and two client-call facts with modes
`resolve`, `reject` or `deferred`. Rejections contain ordinary string name/message
Error fields and bounded data-only code values. No source result, actor,
validation, accepted flag, balance or expected poststate is accepted as input.

`SourceMainBootstrap.step` supports import, construct, explicit client settlement,
finite microtask drain, end-of-turn notifications, a fixed late index handler,
actual Main height invocation and the literal missing-client raw-call guard.
Step acceptance is a control result, distinct from source promise completion.
Deferred clients remain pending until a supplied client-capability settlement.

Import requests `getInstance()` with no arguments before construction requests
`getInstance(true)`. Static assignment and TxUtils assignment remain distinct;
the original index instance's own client remains null. Raw client descriptors
resolve property reads on opaque RPC handles. Static assignment precedes its
potential native chain-field exception. Client rejections retain their original
error-reference slot through promise settlement and notification. Equal error
fields do not merge distinct rejected objects.

Promise/error slots are scoped to the owning startup epoch. They are diagnostic
references, never caller-supplied executable capabilities or global identities.
The index handler selects the actual retained Main.txIndex slot. Notification
order follows actual queued settlements; handler attachment before notification
suppresses that notification, and attachment afterwards produces a subsequent
rejectionHandled event. Notifications enter the established global history at
the actual explicit end-turn boundary, avoiding the former invocation-capture
window that could omit a bootstrap rejection.

The existing real Main height method executes on the same owner and IO. A
direct RPC failure carrier binds its original error reference; display strings
are not used to identify it. The missing-client TxUtils raw guard preserves
the source's new ReferenceError before its try/catch. Successful direct raw IO
is not admitted by this startup control profile without the native document
lease; it is not replaced with a supplied business answer.

## Replay, ownership and proof boundaries

Every raw configure/control input is recorded as an actual Invoke command in
the real source Replay. No additional per-job financial clone or second state
owner is introduced. The existing Replay's command rollback behavior remains.
Unresolved or notification-pending startup cannot expose a legacy checkpoint
or be adopted by NativeMainOwners. Adoption also checks that the quiescent
metadata agrees with the actual same index's module/singleton/client fields.
Failures return the complete actual owners without rewinding previous setup.

Closed raw commands reexecute from the genuine foundation. They are not client,
cache or financial snapshots. A closed owner can enter the existing active
service; its already imported same index causes no duplicate import request.
Startup steps are refused after native binding. Existing service/WAL and cold
replay implementations are not replaced by this proposal.

Old interpreters refuse the new target namespace. Actual immutable-provider
goldens retain default/configured old V12 checkpoint bytes and the old refusal
of an unfamiliar bootstrap target. The current reader cannot relabel that old
recorded refusal as successful startup. TLSW2–5 already exclude this V12 domain.
TLSW1 receives an explicit produce/verify fence because its old descriptor can
otherwise admit arbitrary replay-policy prestates. No existing witness domain
silently gains this new initialization capability.

## Evidence and remaining domains

The immutable ecc021 source corpus executed unchanged full TxUtils/TxIndex
modules and the whole unchanged Main class: 14 scenes, 99 observations, both
genuine numeric profiles. This stage derives 98 initializer/caller/notification
projections exclusively from those observations. Only their separate raw input
fields drive execution; projected expected state is assertion-only. Complete
financial tables, receiver prototypes, logs and general engine state are not
claimed equivalent by this projection. Undefined source code fields remain
distinct from explicit null. The 99th genuine full-Main import refusal at
reOrg is retained separately rather than fabricated as a successful IIFE.

Actual local Rust1.90 primitive checks cover four same-owner continuation cases
and three metadata cases, plus three unchanged codec/SHA support tests. They
do not execute the new private Core adapter. Twenty-one actual-Core tests are
provided for Parent: fourteen named source scenes, namespace/default fences,
pending adoption/checkpoint behavior, closed reexecution, same-index service
handoff, old byte goldens and the legacy witness fence. Core execution/Clippy
remain pending Parent. No Root edits or Core Cargo runs were performed here.

Global Main/reOrg/init/Activation module IIFEs remain an unavailable separate
domain. This profile assumes the explicit prebound client facade used by the
source corpus; it does not implement original client.js network selection or
connection setup. General promise callbacks/thenables, arbitrary thrown graphs
and Rich14 transport, partially initialized process-death recovery, and full
Main.initialize network/genesis/timer behavior remain separate gates. The
recorded finite-turn profile is Node24.22.1, not a claim about every Node runtime.
Integrity and relative head agreement establish no chain or actor authority.

## Parent integration

All22 actual-Core controls now pass, including14 original-source scene comparisons and ownership/replay/default/proof fences. The combined library passes392 tests (four ignored),44 public compatibility checks pass, and all-target Clippy, Cargo fmt,206 included-file format checks, inventory and supply-chain checks pass. Both genuine source numeric profiles reproduce14 scenes/99 observations with the unchanged088be935 fixture;98 initializer/caller/notification projections are compared in Core. The99th observation is an honest unavailable full-module reOrg boundary, not successful global Main/IIFE initialization.

The frozen old source loader has a separate exact-byte filename; current shared service loader is unchanged. The test fixture reader uses the existing deeper diagnostic parser; no production JSON bounds or expected observations were changed. Projection generation has in-repository paths, explicit UTF-8/LF and read-only checking. Seven actual older-provider step-refusal head/checkpoint/archive cases preserve wrong-policy/unconfigured behavior. Older unknown configure histories refuse mismatching replay rather than being relabeled successful initialization.

The separate twelve-scene Promise corpus and v2 continuation are not included in this validation. Rich callbacks/thenables, full global Main/IIFEs, interrupted bootstrap process-loss and broader domains remain open. The new profile is refused by old conditional witness boundaries; no hash authenticates a client or chain.
