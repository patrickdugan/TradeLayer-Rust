# Upper Main V9 differential integration

The upper source corpus now passes fresh Rust 1.90 differential execution: 109 original Main calls, two isolated local capability refusals and 35 input resets. Three tests check the complete runtime graph, older Main admission and canonical virtual artifact paths. Complete-suite and release results are recorded separately. Generation alone is not Rust validation.

The source corpus is copied byte-for-byte from `next-main-upper-corpus` manifest `e75af38bacdf4419c9344ef1edd711e769195267ad55b85d01d214356222523f`: 146 frames, 111 Main calls, 109 available calls, two isolated unavailable-capability experiments and 35 constructor/input resets. The exact Rust observer independently executes that generator's `--print --check` path with both genuine numeric constructors. Its 146 projected frames are byte-identical. No source result or saved poststate drives Rust execution.

## Mount and ownership

Mount the six frozen corpus payloads, `tests/source_main_upper_callable.rs` and this document. Apply the two pure transforms from `prepare.py` to the current parent source:

- `prepare_main` targets `src/source_main_callable_impl.rs` and adds only the exact `SpotLifecycleV9` upper-kind guard arm.
- `prepare_artifact_paths` targets `src/source_proof_settlement_impl.rs` and adds the exact-V9 virtual input-map path predicate at artifact configuration and artifact lookup.

The `preview/` files are readable snapshots of those transforms. Do not blindly replace parent source with them after another worker updates it. The manifest pins their base hashes and the pure transform, so the parent can apply the narrow deltas to its latest owned files. No work-cache binary, helper or copied runtime is a repository payload.

## What the differential test checks

`original_main_upper_types_actual_owners_artifacts_signatures_and_full_replay` imports only recorded constructor/input tables into a fresh V9 replay, applies explicit environment/artifact/RPC bindings, and invokes actual Main calls. It compares exact outcomes and transport, all persisted rows and unexpected nonempty collections, actual caches/singletons, source activation, channels and withdrawals, fee buffers, reward queues, clearing contexts, Orderbook receivers, Margin receivers and position-reference graphs, option indexes, synthetic/vault maps, raw proof environment/artifact maps, and ordered DB/RPC/artifact/postphase effects. Original Number bits are retained; BigNumber formatting normalization preserves negative zero.

The original Main prototype/singleton identity is retained as source-observer evidence and asserted for each frame. Rust uses its one callable Core owner; absent daemon/client startup services are not presented as an equivalent initialized Main daemon. All actual book and Margin receiver counts and fields are compared strictly. The original unfiltered frames remain in the frozen fixture; the portable observer selects the declared external effects and explicit Main/TxUtils call observations, as in the existing Main/spot tests.

Each available call is followed by actual checkpoint reconstruction and exact canonical re-encoding, including receivers, caches, consumed failures, entry profile/lock and continuations. Each unavailable experiment must produce an explicit atomic `UnknownDependency` local receipt, no effects and an unchanged runtime; that receipt also reconstructs from its checkpoint. The next comparable scene requires a constructor reset, so no source-only failure prefix becomes an injected Rust prestate.

`prior_v8_main_guard_still_refuses_every_new_upper_kind_atomically` tests all 18 proposed kinds at the existing V8 Main entry. It requires an atomic `InvalidInput` local refusal before decoding any upper dependency and a valid V8 replay checkpoint. Existing V8 byte fixtures and old witness refusals remain separate parent gates.

## Virtual artifact input boundary

The portable source corpus uses genuine Node `path.posix` and raw bytes under `/tl-source-artifacts/`. On Windows this is not a host absolute path, so the original host-path predicate rejects it. The proposed exact-V9 predicate recognizes only canonical input-map keys in that namespace. It rejects an empty namespace, empty components, leading-separator/UNC aliases, traversal, backslashes, colons, NUL and characters outside the declared component alphabet. Configuration and lookup use the same predicate.

For V1–V8, the original host-path function is unchanged and is selected exactly as before. Existing host-path inputs remain available outside the new V9 namespace. The delta adds no filesystem or network call: bytes still come only from the bounded explicit artifact map. The source signature and bundle helpers verify original data themselves; no Boolean signature/verifier outcome is supplied by the test.

`exact_v9_virtual_artifact_namespace_is_data_only_and_canonical` checks the V9 input namespace, V6–V8 platform-specific legacy behavior, invalid aliases and atomic configuration refusal. The populated Main artifact case separately exercises genuine signature checks, raw bytes, artifact hash validation, source financial/metadata writes and replay. A valid hash or witness is not actor authentication or a complete block/BitVM proof.

## Verification status

The early metadata-only check used immutable runtime SHA256
`542c1f01ff90027fec2d08485dc13980f26b18f4ed86b810aaca34a258a73ea4`.
It established test API/ownership/lints before the upper implementation existed.
Fresh Root library builds have now passed all three differential tests and both
numeric comparisons. The focused evidence also includes raw20 fallback,
raw21 withdrawal and raw22 continuation failures, old-policy checks and
all-target Clippy. Complete release-suite validation and the independently
compiled V9 byte fixture are separate gates recorded by the Root validation
record. No push or PR is authorized.
