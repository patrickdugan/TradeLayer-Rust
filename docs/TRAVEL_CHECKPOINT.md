# Travel checkpoint — 2026-10-27

Paused at the user's request. No push, PR or publication.

Verified implementation: `cd9dd14c68192e77ffd230531505c9fe45ba2585`. Rust1.90: **1,461 test executions**, 96 locked-reference checks, Clippy, formatting, doctests and all-target release passed. Default remains GlobalClearingV2. Sidebar and saved completeness workbook retain this verified baseline.

`handoffs/v8-checkpoint-2026-10-27.zip` preserves 46 worker/source/evidence files, including complete Main, Send/commit and ClearList stages, transforms, Node corpora, the independent populated V7 release checkpoint and validation metadata. ZIP integrity and every embedded file hash were checked. SHA-256: `af2370398769fe7cb6b22f449b0dc2dd483140d48b66e88166868cf94841fe99`. Work files also remain under the projectless workspace's `work/` directory.

## Resume

1. Verify the archive SHA and `CHECKPOINT-MANIFEST.json`. If work files are lost, restore the archive into the workspace `work/` folder, preserving its relative directories. Do not overwrite divergent files without inspection.
2. Keep the verified source baseline separate from this **uncompiled V8 handoff**. Main has 39 Node observations (18 proposed admitted method calls, 13 resets, seven explicit unavailable calls, one import boundary). Send has 121 observations; ClearList has 200 per actual library. Rust tests: Main two, Send four, ClearList three — all pending execution.
3. Apply transforms to a fresh copy in this exact order: Main policy/runtime, ClearList runtime, Send runtime. Apply Main replay, ClearList stateless owner bridge and Send accounting hooks. Reverse order is not validated; the Send documentation's either-order statement needs correction.
4. Check the confirmed ClearList country coercion-order finding in `next-main-phase-driver/review/clearlist-country.cjs`: Number(listIdRaw) must execute before checking row status. A revoked malformed row must retain the source's caught error/null behavior. This was not Rust-verified at pause.
5. Mount the independently compiled V7 fixture and add byte/history/version compatibility tests. Run the first combined Rust1.90 focused tests with the existing `work/global-clearing-target`, then full formatting/includes, Clippy, tests, locked references and release. No V8 completeness promotion before actual checks pass.
6. V8 activation profile is explicitly bound: default actual source registry; standalone declared-answer profile refuses Main and locks on first Activation/Main use. All financial mutations use the existing sole Core Tally/collections.
7. Root spot5/6 work contains ten original-source calls plus reset, identical under both BigNumber versions. It reaches actual queue placement/matching and preserves unbound tallyMap, missing cancellation-method and wrong save-arity behavior. It is source evidence only; complete its Rust logic later.

The full roadmap remains active work, not complete. Main startup/RPC/Persistence, remaining lower routes, production compact-index adapters, authenticated history and complete transition proofs remain gates. TL produces state-oracle data; UTXORef remains a separate consumer.

The goal and thirty-minute completeness heartbeat are paused for travel. Resume them when the user returns.
