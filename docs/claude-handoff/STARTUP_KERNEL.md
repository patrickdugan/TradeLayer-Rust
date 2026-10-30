# Stopped for the user's CONSERVE / Claude handoff

No Root files were changed, no Core Cargo build or child commit was run, and no
tool session is still running in this lane. Parent owns all integration/commits.

## Completed isolated checkpoints

- root-ready-source-kernel-1 / MANIFEST07687ab53d680888194ac96fee638112b503bab1ca663aa440e2ccd2a9d385ed:
  34 Rust1.90 standalone metadata tests + strictClippy. SAME original Kernel owns
  optional intrinsic Promise metadata. Realms, native adoption, actual-result
  opaque payload/job/continuation keys. Default canonical stream compared with
  immutable actual cfbae provider. No production source-controller admission.
- root-ready-source-kernel-nedb-1 / MANIFESTc40f3a1e5ee1a6cd5671930bd35e5a72ac36af9ce5daa0b2292656555e1eb3d0:
  39 standalone metadata tests + strictClippy. SAME Kernel also owns explicit
  NeDB1.8/async0.2.10 read jobs. Two genuinely executed unchanged NeDB scenes
  reproduce 16 sample/callback events. Worker-next is enqueued before callback;
  different DB workers interleave and one DB stays serial. Actual query frame / V
  result remains outside scheduler on the sealed containing source owner.
- ../native-main-startup-key-visitors-v13/root-ready-key-visitors-1 /
  MANIFESTf47629e53fe11276e68c0805eba999b205f8ab1c34eca95a290e1001cbcd76f0:
  41 metadata tests + strictClippy. Read-only validated full Promise/payload/job/
  continuation key visitors and actual_delivery(). No mutable owner exposure.

These freezes are immutable. The live src/snapshot below additionally contains
the narrow notification-order correction described next. Do not confuse those
bytes with earlier frozen manifests.

## Concrete independent notification finding and tested successor

Row's exact0768 review executed genuine Node24 native Promise controls:
end_turn must emit newly unhandled promises in REJECTION order, not creation
order, and pending HandledLater notifications precede new Unhandled events.
Exact two retained adversarial Rust controls fail0768 and pass the new successor.
The live queue now retains pending_rejections FIFO in settle_unlocked (also actual
adoption-forward settlement), reserves before state mutation, canonically streams
FIFO keys, drains HandledLater first, and skips handlers attached before end_turn.

Latest run evidence/kernel-nedb-checks.log: 45 Rust metadata tests PASS and strict
Clippy PASS. run_checks.py's final print still says39; the exact executable test
output is authoritative. No financial-Core check or full source parity is claimed.
Pure correction: ../native-main-startup-notification-order-v13/prepare.py.
Reviewer original evidence: ../review-main-startup-source-kernel-v13/NATIVE.json,
native-notifications.cjs, review_adversarial.rs and adversarial.log.

## Current working files

- src/source_main_startup_promise_queue.rs: current corrected intrinsic queue.
- src/source_main_startup_nedb_jobs.rs: named read-only NeDB executor metadata.
- src/source_main_startup_source_kernel_impl.rs + prepare.py: same Kernel optional
  owned fields, old/default None unchanged, new profile fences old generic starts.
- tests/{kernel_source,component_tests,payload_tests,nedb_jobs_tests,
  key_visitor_tests,notification_order_tests}.rs: current 45-test closure.
- snapshot/: exact copied current Kernel/include closure + new leaves.
  Standalone ONLY omits inaccessible raw-startup private clock include;
  production transform keeps it. No snapshot wholesale Root replacement.
- build_snapshot.py, run_checks.py, INPUTS.json, evidence/: reproduction/pins.
- ../native-main-startup-key-visitors-v13/src/source_main_startup_promise_key_impl.rs.
- proposed_source_main_step_kernel.rs retains the production clock include.

The actual financial source Incoming/return-chain containing controller has NOT
been implemented in this work stage. No placeholder runtime functions or fake
results have been added. The next design sketched in chat is not executable code.

## Required next work / exact ownership

This lane owns Kernel/Promise/Incoming/return chain, real NeDB request/executor /
callback binding and successful-result custody. Main worker owns ONLY outer
bootstrap specific frame/hook/registry binding. Row owns existing row/WAL/cold
native paths. Keep those boundaries; no financial owner clone or extra ledger.

1. Freeze/use the notification successor before any live notification claim.
   Also add requested NedbQueue visit_request_key and actual_delivery observers
   in a separate additive leaf, preserving c40/f476; these are NOT written yet.
2. Bind actual nonclone query frames to opaque source job/site/lane/ordinal and
   the same containing Kernel. Request-time hook/effect != index candidate
   sampling != callback delivery. No arbitrary Runtime/frame transfer API.
3. Main outer frame2557 + corrections dd7/c0e83/dd36 have actual17 Core controls
   PASS (Parent-owned). Use its genuine retained factory/query/return results,
   actual unawaited Tally child, fixed immutable streaming view >1024; never
   synthesize Undefined or rows. Per-factory before/after hooks must create and
   settle true Promises BETWEEN Tally factory and consensus factory, without drain.
4. Implement Sync/checkBlockLag, Incoming, height/factory queries and nested return
   adoption on the SAME Kernel. Start parent await at actual call return, not
   when a later result descriptor appears. VM business / host RPC cross-realm
   adoption must be real intrinsic jobs, not a fixed count of phases/ticks.
5. Preserve all four genuine positive source scenes and original failing queued
   assertions. Require complete RPC/effects/UID plus held-result comparison before
   promotion. Raw UID IO must come from independently captured crypto bytes,
   never expected financial IDs. Old default/replay/witness/WAL/cold stay closed.

## Proven startup ordering bug still OPEN

Actual current cfbae Core captures outer fixed view4rows AFTER first live upsert;
genuine source captures3rows BEFORE first raw transaction RPC/upsert. Thus outer
catchup skips the first block and later writes140/141/142/166 are absent while
first live MaxHeight134 is held. Fixing Start quiescence and caller observation
did NOT fix that. Original logs root-queued-startup-{first,second,third}-owner.log
must remain; do not remove writes_after_selected>0 or compare only parent result.

Genuine source evidence (immutable):
- ../native-main-startup-rpc-order-v13/root-ready-rpc-order-causal-1 (dc7d6dbf...)
- .../root-ready-promise-notifications-1 (1c95c2b7...)
- independent review ../review-queued-rpc-adoption-v13/root-ready-review-1
  (afdf1b3b...). Real host Promise adoption, not extra then/catch instrumentation.

Provider for public diagnostics / metadata DTO linkage only:
../providers/queued-startup-probe-wire-aeebb65/libtradelayer_core.rlib
SHA cfbae645513f4e6fe631ec80d777c24dce4274249ff2a6227c02e9999e9b15d2.
No evidence that this old provider implements the new source scheduler.
