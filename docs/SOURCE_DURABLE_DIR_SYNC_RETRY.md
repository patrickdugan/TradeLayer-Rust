Root integration note: the focused checks recorded in [SOURCE_AMM_LIFECYCLE.md](SOURCE_AMM_LIFECYCLE.md) have passed. The staged handoff below is preserved for provenance; its work-only/uncompiled statements describe the earlier handoff, not the current Root integration. Full release promotion remains pending.

# Durable segment retry directory barrier

This separate revision preserves the frozen durable adapter manifest `2ecd3453c116e271daeb31543afc8548550906cf3abf3df1aabe51534734f203`. It changes no Root file, live source owner, replay format, command input, receipt, financial result or expected-head contract.

The original existing-segment fast path verifies a matching immutable file and returns success. A prior attempt can have synced the file and renamed it, then failed the segment-directory barrier. Reopening at the retained old publication and retrying finds that valid orphan. Returning immediately would permit publication without completing the failed barrier under `FileAndDirectorySync`.

The production fix repeats `durable_sync_directory` after validating an existing segment and before returning to publication. `FileSyncOnly` remains a no-op at that barrier. The normal production support check remains Unix-only for the stronger profile, and normal Windows admission remains `UnsupportedFilesystem`. No directory-power-loss certification is inferred.

## Meaningful fault/reopen/retry test

A module-private, test-only thread-local barrier probe lets the stronger profile's ordering be tested on Windows without enabling it in production. The probe intercepts only the filesystem barrier, never Core calls, financial data, source outcomes or receipts. Other test threads and child processes retain the original filesystem behavior; a guard removes the probe even on failure.

The new test initializes the actual strong-profile context and executes two real fee commands on the unchanged source owner. The first injected segment-directory failure occurs after the segment file has been synced and renamed. The test requires `PublicationFailed`, a verified existing segment, a poisoned handle and successful reopening of the retained old publication with its exact old owner.

The second attempt reuses that same verified orphan and injects another barrier failure. Reopening must still find the old publication. The third attempt succeeds only after its barrier, publishes one segment/two commands and matches an independent actual runtime executing those commands once. A stale-CAS retry is refused before another filesystem call. A final reopen reconstructs the same owner.

A mutation control removes only the production existing-segment barrier from a separate harness copy. The new test must then fail on attempt two because the old fast path publishes instead of observing the injected failure. This distinguishes a functioning retry barrier from a test that merely counts successful calls after publication.

## Honest Root test coverage

The Root payload removes the old `old` alias/self-comparison test. Its replacement asserts exact V1–V9 policy tags, fresh and short current canonical checkpoint roundtrips, fixed limits and complete head/runtime/checkpoint preservation on command4097 rejection. It does not call this an older-library byte comparison or replace Root's independent populated V1–V10 fixture gates.

The actual comparison with the immutable earlier library remains **only in the standalone harness**, under its explicit legacy-comparison name. The Root test payload has no external-crate unit-test import. Current V10/V11 populated durability evidence is outside this small revision and is not implied by tests linked to the earlier library.

## Checks and minimal integration

The standalone harness uses an immutable copied owner library, SHA256 `90e478a58b0bbe79a35dbaa13c8a3349f357d2ff3ef27933abfe86c7e986a64a`, exact copied replay/hash/codec helpers and the new adapter. It compiles only those helpers/tests against the existing owners. Tests use an explicit work-directory temporary root. Physical Unix directory synchronization remains unexecuted on this Windows host; the injected test checks the ordering contract, not power-loss behavior.

Root has already mounted the frozen adapter. Apply `prepare.py:prepare_impl` to its **current** `src/source_durable_replay_impl.rs`, and `prepare_tests` to its current `src/source_durable_replay_tests.rs`; the latter accepts either the original external alias or Root's already corrected crate-local alias by replacing that old test body. Copy this document if desired. Keep the existing replay include, input fixtures and independent older binary-version tests unchanged. The pure transforms fail on unfamiliar or already applied anchors. Do not replace the current parent replay/runtime with historical previews.

Root owns actual integration checks and commit. No whole-Core build, Root edit, remote write, snapshot shortcut, chain authentication, actor authorization or broader exactly-once claim was added here. The bounded archive/context/CAS and history-linear recovery limitations of the original adapter still apply.
