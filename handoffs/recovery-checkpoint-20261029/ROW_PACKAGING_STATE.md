# Row recovery packaging checkpoint — 2026-10-29

PACKAGING ONLY. No new implementation or build was started for this checkpoint.
Root/isolation and every existing immutable freeze remain unchanged. Parent owns
all repository edits, Core builds, handoff archives and local commits. These
worker stages are outside Git; this worker has not committed or pushed Root.

## Actual implementation status

**The financial Main row/catchup continuation PCs are NOT implemented.** No
Runtime row-machine/controller file was written. Invocation begin/finish
factoring, typed no-index owner loan, superseding intermediate recovery,
verified complete-KV checking before tail repair, terminal WAL suffix and
process-loss financial-prefix reexecution remain designs/next work. Do not
describe the tested metadata components as full financial recovery.

Written implementation/evidence files:

- `C:/Users/patri/Documents/Codex/2026-10-26/go/work/native-main-durable-prefix-v13/source_main_native_intent_store.rs`
  implements the metadata intent/native-prefix/accepted/ack store.
- `C:/Users/patri/Documents/Codex/2026-10-26/go/work/native-main-durable-prefix-v13/store_tests.rs`
  and `namespace_repro.rs` contain store/source-order/crash and namespace tests.
- `C:/Users/patri/Documents/Codex/2026-10-26/go/work/native-main-prepared-validity-v13/source_index_prepared_put_impl.rs`,
  `source_index_prepared_validity_impl.rs`, `source_index_prepared_validity_tests.rs`
  implement/test the actual metadata query/selection/preparation/commit/post-resolution seam.
- `C:/Users/patri/Documents/Codex/2026-10-26/go/work/native-main-durable-prefix-v13/scripts/main_native_row_await_controls.cjs`
  and `fixtures/main_native_row_await.jsonl`, `fixtures/main_native_row_await_locked.jsonl`
  are the genuine original Main/NeDB await oracle and observations.
- `C:/Users/patri/Documents/Codex/2026-10-26/go/work/native-main-durable-prefix-v13/ROW_RECOVERY_CONTRACT.md`,
  `DESIGN.md`, and `WORK_STATE.md` preserve the unfinished ownership/PC/repair design,
  exact source constraints and next implementation details. Pure transforms and
  standalone reproduction scripts are included in the freezes below.

## Preserved immutable handoffs and latest evidence

- `C:/Users/patri/Documents/Codex/2026-10-26/go/work/native-main-durable-prefix-v13/root-ready-intent-store-2/MANIFEST.json`
  SHA256 `da24d343c2f7906189a52bd325733b5c0e77eb7fb2b0dc487ac70fd46b647662`.
  Parent actual Core **16 PASS +1 ignored child entry;8 real child exits**:
  `C:/Users/patri/Documents/Codex/2026-10-26/go/work/native-intent-store-core-first.log`.
  The unpromoted STORE-13-1 namespace defect/source/failure are retained;
  STORE-13-2 binds persistent identity and exact namespace Journal location.
- `C:/Users/patri/Documents/Codex/2026-10-26/go/work/native-main-prepared-validity-v13/root-ready-prepared-validity-2/MANIFEST.json`
  SHA256 `82558c5109839a2873fc6c5f45527583403ffc9de2111bebff8162f28ff043e3`.
  Parent actual Core **7 PASS**:
  `C:/Users/patri/Documents/Codex/2026-10-26/go/work/native-prepared-validity-core-first.log`.
  Tests include1100 complete rows, real intent before update, exact legacy
  source throw/update observations, distinct read/write pins after compaction,
  stale/foreign refusal and healthy exact-post resolution without duplicate append.
- `C:/Users/patri/Documents/Codex/2026-10-26/go/work/native-main-durable-prefix-v13/root-ready-row-await-source-1/MANIFEST.json`
  SHA256 `cb61e1a7e35d6293d3ec02458058df938cd964e366c1c58f7240445214b3b7da`.
  Original unchanged Main/financial bodies +genuine NeDB1.8:3 handled Main0
  scenes/12 observations per genuine numeric profile; all4 generate/check runs
  passed. Corpus SHA256 `721484583b6f79684c4d1c9a6ec2eed17a53f0986ca43d2d513a22ce65be0410`.
  Genuine pending await already has financial effects/processed=true. Resolving
  it returns skips0 with one decode/mark; whole-call retry returns skips1.
  Known rejection runs the real fatal branch:2 marks/1 decode. These recorded
  snapshots are observations ONLY, never financial-owner constructor inputs.
- `C:/Users/patri/Documents/Codex/2026-10-26/go/work/native-main-step-segment-capacity-v13/root-ready-capacity-probe-1/MANIFEST.json`
  SHA256 `cc3fd782d6b2715e1ffa69ab5e0d4afa36e76a9380279c4f02f1fc3093ffb012`.
  Parent actual corrected **18 PASS**:
  `C:/Users/patri/Documents/Codex/2026-10-26/go/work/native-step-binding-capacity-full.log`.
  f5651/independent original failure remain preserved. Proper-prefix publication
  fixes the35-command byte-window stall. Single receipt >64MiB remains an
  explicit retained resource boundary. No Root promotion/full WAL claim follows.

The store/prepared-index actual Core evidence is23 component executions,
separate from the earlier Root release1714 baseline. Standalone/frozen-copy
checks were26 PASS/Clippy for store (19 prior +7 new,8 child exits) and10
PASS/Clippy for prepared validity (7 new +3 reused codec/hash). Parent's actual
Core results supersede the earlier “uncompiled” wording inside those freezes.

## First next task

Implement the actual **sealed Main row continuation** using existing
`source_main_prepare_row_prefix` exactly once and a factored actual invocation
begin/finish. Keep the SAME Replay/Runtime/queue/index; retain actual row/id/kind/
params/height/live/skips/processedAny and Primary/Fatal await PCs outside Clone/Eq
Core. Sync raw command intent before source work and actual prepared native
intent before append. Resume the existing await after verified post/ack;
never rerun decode/logic/markProcessed against alreadyProcessed metadata.
Compare the real financial prefix and return/throw observations against cb61.

Then complete same-lock KV-before-repair and the typed no-index reopen loan,
explicit before/intermediate superseding plans, command/prefix reexecution after
process loss and terminal suffix retention. None of these should be inferred
from a checksum, metadata pin alone, narrow test count or imported current
financial snapshot. Source shutdown remains2 messages then typed exit0, with
no real persistence save, host process exit or generic post-call async drain.
