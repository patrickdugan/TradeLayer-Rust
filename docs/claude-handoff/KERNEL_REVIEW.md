# Startup source Kernel review: Claude handoff

Target07687ab53d680888194ac96fee638112b503bab1ca663aa440e2ccd2a9d385ed:26 verified payloads. Successor c40f3a1e5ee1a6cd5671930bd35e5a72ac36af9ce5daa0b2292656555e1eb3d0:38 verified payloads; entire end_turn body is identical after newline normalization and carries the same defects. No author or Root files changed.

## Concrete notification-order defect

verified-copy/src/source_main_startup_promise_queue.rs:910 iterates Promise creation order. Create first/second, reject second before first: actual Node Unhandled order is second,first; unchanged Rust is first,second.

Line925 appends HandledLater after newly Unhandled. After a prior notified rejection, attach its handler and reject a new Promise during the next turn: actual Node order is HandledLater(old),Unhandled(new); unchanged Rust reverses it.

Actual intrinsic Node24.22.1 commands:

    node native-notifications.cjs reverse-rejection
    node native-notifications.cjs handled-before-new

Full exact ordered outputs: NATIVE.json. These are actual process unhandledRejection/rejectionHandled events for intrinsic Promises in the single Node realm, represented as SourceBusiness in Rust. No arbitrary thenable, foreign-realm, Main financial, NeDB or full-event-loop parity is implied. No expected notification is a scheduling input. review_adversarial.rs executes the same schedules through the exact frozen Rust queue; adversarial.log preserves both failing controls and actual order.

Recommended narrow successor: record pending rejection-occurrence FIFO at actual rejected settlement, including adoption forwarding; reserve before settlement and canonically bind it. Emit pending HandledLater first, then newly Unhandled, skipping now-handled promises. Do not substitute Error identity or Promise creation order. Author v8_clearlist confirmed both defects and is implementing a NEW successor. No successor fix has been reviewed/tested here;0768/c40f stay immutable.

## Checks and custody scope

Original34 copied metadata tests PASS. Two added positive controls PASS: exact ordered default canonical-field Vec equals the immutable actual provider, and4096-reaction capacity stop retains inflight adoption/original reactions/locked unsettled destination without fabricated rejection. Total36 positive checks PASS; both new notification checks FAIL as evidence. Rust1.90 compile and warnings-denied Clippy PASS. No private financial Core build or production startup fulfillment claim.

Provider work/providers/queued-startup-probe-wire-aeebb65/libtradelayer_core.rlib SHA cfbae645513f4e6fe631ec80d777c24dce4274249ff2a6227c02e9999e9b15d2. Standalone snapshot excludes only the inaccessible private clock include, as author documents; production pure transform preserves it.

No additional concrete ownership defect found by inspection in this bounded component. Optional non-Clone queue resides in the SAME existing Kernel, absent under old defaults; generic commands are blocked except literal no-drain Shutdown. Keys bind owner/generation; source job/method/site/lane/await ordinal are retained. Exact active ticket controls acknowledgment; wrong/duplicate acknowledgment refuses. Source/resource stops retain active/inflight metadata and payload references.

ActualPayloadRef is an owner-bound ready reference. Its private marker does NOT prove that a containing backend frame owns a real result. Future containing controller must retain/canonically visit the actual non-Clone backend frame/result, derive readiness from actual completion, validate the full continuation binding and acknowledge only after the SAME owner's source segment. This actual containing-owner admission is not established by0768. Valid Promise aliases may share a retained result while consuming separate delivery tickets once; that never authorizes backend IO or source continuation replay.

Bounds remain1024 promises/source jobs and4096 reactions/payloads/routes; no GC or long-run claim. Caller-return/adoption ordering has bounded metadata cases only. Full containing-owner startup, NeDB scheduling/native recovery and broader resource schedules remain separate gates.

Parent reported our separate63f6 actual compaction/copied-prefix Intermediate/sealed-retry/Post financial cold scene PASSED unchanged; this review does not expand that source-scene claim.

No Root edits, Cargo/private Core execution, commits or pushes. Parent expressly reserves integration/commits; this immutable work-stage handoff follows that restriction. Work stopped at the conserve/Claude-handoff instruction.
