# Retained Main caller Start WAL

Bounded prerequisite: raw caller action codec + actual durable Start barrier.
This does NOT complete full caller action history, selected native recovery or
cold caller reexecution. It depends on Root335/af and the new3a consensus route
type, including Root's narrow actual Detached/Initialize readiness guard. That
factory is untouched by this package.

Apply prepare.py pure functions to CURRENT Root source:

1. lib.rs: prepare_lib (one new module).
2. source_main_native_session.rs: prepare_session, prepare_tests (include only).
3. source_main_active_service_wal_tests.rs: prepare_wal_tests (test module only).

Copy five src/ leaves. Existing raw caller fixture and trimmed test transport
are reused without replacement. Actual WAL/storage/defaultMode4 implementation
and bytes are unchanged. prepare_component.py writes review artifacts only
under this stage. Generated whole-file previews must not overwrite newer Root.

The public raw codec TLMACA1 carries only caller Start/control inputs, selected
operation values, and distinct CallerDelay versus ConsensusService timer lanes.
It preserves SourceValue number bits, object order, Undefined/Promise/BigNumber.
It has no source-result/state/plan reader. Unknown tags, oldTLMASA magic, malformed
lengths/depth, bad booleans, truncation and trailing bytes refuse. Future dispatch
must match a selector against exactly ONE actual origin-bound kind/job/scope/
ordinal/role/context; no first-pending fallback is admitted by this codec.

The source controller SourceMainNativeDurableCallerStartV1 consumes ONE actual
NativeSession. Before any source constructor action, it checks actual head/native
physical Before and captures actual native archive, financial commitment, rows,
immutable native origin/foundation, full raw Start and explicit StartOnly scope.
The existing real WAL Store commits a NEW immutable origin profile before source.
Then Intent is synced BEFORE into_retained_main_caller/load_module/header. The
actual constructor is dispatched once; its actual frame/route/generation and
partial/rejected state become Accepted, then command ACK. StartAcknowledged means
only this constructor/barrier completed; it never means Initialize fulfilled.

The executed marker is armed immediately before the actual constructor. On WAL
error the same owner/plan/post/route stay retained. Explicit recover_wal verifies
the exact independently retained Before or Post; Before rearms only the same
metadata write and Post advances only that barrier. Accepted/ACK recovery cannot
run the constructor again. No financial snapshot importer, other Ledger/index,
new financial owner from records, early effects drain or legacy Session/head
release is added. Hashes bind context/routing, not actor/UTXO authority.

This first origin-selected WAL profile admits StartOnly. Later decoded raw action
variants are NOT executable by this component. The same moved caller and actual
route remain private for the next selected-action controller, which must preserve
the competing-publication fence, retain native ambiguity and integrate true native
reopen/supersession before admitting it. No same-process source retry or uncertain
native append retry is implemented here. Existing Mode4 recovery is unchanged.

Executed gates:6 codec tests,3 new real Store controls,18 total actual copied WAL
controls, strict Rust1.90 Clippy for both independent closures. FOUR actual-Core
Start tests are formatted/UNCOMPILED: no constructor before synced Intent, real
Accepted/ACK lost return, healthy Before Accepted retry with exact same owner,
and synced Intent lost return before constructor dispatch.
Parent owns Core/Cargo/commit gates. The generic Store-only tests make no financial
execution/actual Session-origin claim; that claim awaits the four Core tests.

## Actual Root validation

The combined batch passes25new controls:3actual-source Mode4,9metadata/trace,4actual caller Start,6input-codec and3Store checks. Library545/fourignored,44public compatibility checks, all-target Clippy with warnings denied, Cargo fmt,303included file formatting, route inventory and supply policy pass. Counts overlap. Record: `fixtures/reference_fidelity/mode4-caller-start-core-integration.json`.

Root preserved the failed Core evidence and fixed two narrow issues. Two caller diagnostic encodings now borrow Option<String> as &str. The real Mode4 writer committed an unscoped origin observation while its first Before included the escrow-scope frame, leaving the referenced origin blob absent. Mode4 initial observation now uses the identical shared scope encoder. All existing per-action observation bytes, noninline initial observations and the strict reader checks remain unchanged. Existing affected histories remain refused, rather than fabricating a missing observation.

This closes the named metadata/ACK and Start-only boundaries. Complete raw-action Mode4 cold reexecution, selected caller actions/native recovery/cold caller replay, process-loss financial recovery, host durability and authentication remain open. Full release baseline stays2,142Rust/239numeric.
