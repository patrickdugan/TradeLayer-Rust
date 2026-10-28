Root integration note: the focused checks recorded in [SOURCE_AMM_LIFECYCLE.md](SOURCE_AMM_LIFECYCLE.md) have passed. The staged handoff below is preserved for provenance; its work-only/uncompiled statements describe the earlier handoff, not the current Root integration. Full release promotion remains pending.

# V11 over the corrected, focused-tested V10

This NEW WORK revision composes the frozen V11 pure functions over the current corrected Root V10 source. It does not rebuild the earlier b02/V10 overlay or replace Root's current runtime with an older whole-file preview. Earlier frozen packages remain untouched. Current Root source hashes and test inputs are listed in the manifest; the base is an uncommitted V10 candidate with 43 actual focused Root tests passing, not a claimed independent V10 release.

## Exact application order

Import `prepare.py` from this directory. It is byte-identical to the frozen `next-v11-compat/prepare.py` and loads the manifest-pinned lower-AMM, quotations and Number-hint pure transforms.

1. Current runtime: `prepare_runtime(prepare_policy(current_runtime))`.
2. Current files independently: `prepare_replay`, `prepare_main`, `prepare_activation`, `prepare_registry`, `prepare_proof`.
3. New AMM include: `prepare_amm(next-lower-amm/root-ready/src/source_amm_lifecycle_impl.rs)`.
4. New quote include: `prepare_quote(next-v11-amm-quotes/root-ready/src/source_amm_quote_impl.rs)`.

The payload contains eight source files (six narrow current-file deltas, two new includes). Complete `database()` and `db()` bodies remain byte-identical to current Root before AND after formatting. Existing lazy materialization and activation/withdrawal/threshold/transfer/spot/send/supply callbacks are preserved. All current includes remain present. Main's union retains every inherited lower/upper/spot path, deliberately inherits raw0 through the existing activation helper, and appends raw10 only for V11. Source-registry/profile/lock authority remains mandatory for Main. No old policy, default V2, witness allowlist, authentication boundary, original function body or financial owner is broadened.

## Fresh tests

Nineteen V11 tests are staged and UNCOMPILED: eight lower/Main/quote/Number-hint tests, eight inherited ordinary/failure suites, and three inherited Main0 tests. The eight inherited suites are freshly generated from current individual Root files; only selected policy, diagnostic labels, checkpoint version and the already-validated first-module unused-helper cleanup change. Main0 uses the current corrected Root test, retaining its optional-binding helper and all old V8/V9 raw0 refusal assertions. Main-AMM gains that same helper for its two optional diagnostic flags: only absent means unbound; present true or unexpected values remain visible. Existing financial state, receivers, outcomes and ordered-effect assertions are retained. Explicit endpoint threshold/withdrawal pending-job assertions are added. No V11 test was executed or linked against an earlier runtime, and no Core/library build ran here.

## NEW Main-AMM observer revision

The frozen Main-AMM driver inherited the same snapshot side effect found in Main0: `state()` copied `tables`, then its diagnostic views called the creating `table()` helper for absent `orderBooks`/`marginMaps`. That changed the next source prestate without a business operation or DB effect. This NEW revision uses nonmutating `(tables.get(name) || [])` only for audit-state views, with an assertion that snapshots preserve the complete table-name sequence. Pinned source/class/method bytes and every business call remain unchanged.

The first ten genuine calls were executed with the new observer and intentionally rejected the old observer proof pin; that bootstrap evidence is preserved in WORK. A separate new proof pin is frozen, without changing the earlier package: `3d860b92ecdadcae535670353dcc64074fc94c2361ce3d9b22f0bc04bd6ecc80`. Full corpus SHA is `6a22e81a4df8774ec6afd825e820fecbf05a679ce988767c95e9491eb1813271`. All four genuine Node checks pass: full and proof10 under BigNumber 9.3.1 and 9.1.2. There remain 36 frames: eight resets, 27 comparable Main calls and one isolated unavailable Main call (28 actual Main calls total). The ten-call proof remains eleven frames (one reset plus ten actual calls).

The exact tagged-transport audit proves all commands, results, summaries, ordered effects and non-table state are unchanged. Every collection's rows are identical. Only empty table inventory/order differs in 28 frames; eight inventories are unchanged. Source/class/method metadata, findings, postphase coverage and dependency descriptors remain identical except the explicit observer note and the two new artifact hashes. No tables are sorted or filtered in Rust assertions and no financial field is omitted. Earlier lower/quote/Number-hint corpora are preserved byte-for-byte.

## Remaining gates

Actual Rust 1.90 formatting, frozen manifest/hash checks, deterministic regeneration, callback/include/guard checks and genuine source-oracle regeneration passed. Root owns fresh V11 Cargo, strict parity, Clippy, all old V1–V10 byte/version gates, full release checks and commit. Hashes and source agreement do not authenticate external inputs or establish complete block validation. Data-only source inputs, finite numeric/storage bounds and explicit unavailable dependencies remain conditional.
