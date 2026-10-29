# Full bounded V8 ordering for channel queues

The channel queue used to refuse more than63 ready rows. The unchanged source calls V8 Array.sort on the complete ready list. `channel_queue_sort` now invokes the existing safe `source_v8_stable_sort` with the unchanged `channel_queue_compare`, retaining the shared1,024-entry bound. This is a source-domain extension, not a new matching/fee policy, repaired comparator, duplicate-ID filter or balance owner.

Only `contract_channel_queue_source_impl.rs` changes in production. It includes the same helper already used by Main IO in its own module. The helper file is byte-identical SHA256 `af2a5787df75e48b0fee6f689ddf1fac5c3ae44e0aa1ac81570216ca40df77ef`; Main IO callers, formats, state fields and readers are untouched. Prior compiled queue checkpoint bytes remain exact for four previously admitted inputs and their source continuations.

The helper's existing attribution/licenses remain required: Node v24.22.1 / V8 `13.6.233.17-node.58`, `array-sort.tq` Git blob `703b74789b4d7e8f223e2c3da44f9fa0eb8983cc`, `licenses/V8-BSD.txt` and `licenses/V8-PYTHON-TIMSORT.txt`. This is a pinned V8 comparison schedule, not a promise about every ECMAScript engine or future version.

## Evidence

The complete unchanged pinned Orderbook module executes in the existing declared queue DB/placement harness. A transparent realm-level sort wrapper forwards the real comparator and records every compared pair, native exception and final array. It does not supply comparator answers. Both genuine BigNumber9.3.1 and9.1.2 reproduce **16 cases /32 records** byte-for-byte:0/1/2/63/64/65/128/1,024 entries; out-of-order runs; ties; number/string/array heights with nontransitive equality; Infinity/string-Infinity subtraction producing NaN; malformed sender/txid errors; mixed known token/contract callback calls; earlier book writes before a later callback error; and a first pair persisted before the second pair's DB failure.

Four new queue tests compare the entire comparator schedule, exact native error and partial-array behavior; complete queue documents/instance caches/DB and callback traces; four actual8bcc checkpoint goldens; and unchanged refusal before mutation above1,024 entries. The callback tests are explicitly metadata/source-interface tests. Their declared placement boundary records arguments and may invoke original book-save operations; it is not a supplied successful financial settlement.

A separate complete original financial-module corpus covers a64-entry queue that performs real token settlement, and a65-entry queue whose later missing payload throws after real Tally changes. Both genuine numeric profiles produce the same six records. The generator asserts that original persisted Tally data actually changed, that the first queue clears, and that the late-error queue remains with65 rows. No financial method is replaced in those scenes. The proposed private/public Core test reuses the existing complete source comparison helpers, initial reference-fixture inputs, same SourceRuntime owners and all collection/cache/effect assertions. Expected later financial state is assertion-only.

The exact standalone queue closure passes **15 tests**: four new queue controls, eight unchanged public queue controls and three support checks. Those include the existing117-action whole-module corpus and57 raw18 enqueue projections. The new financial test compiles against the immutable actual prior Core provider and fails specifically with Capacity at the real64-entry financial call; that reproduction is retained. It has **not passed against a newly built corrected Core**; Parent owns that gate.

Clippy lints are denied in the extracted helper and produce no Clippy errors. Three existing dead-code warnings from private bridges used only by the surrounding Core are retained; no production lint allowance was added. Root's all-target Clippy with all warnings denied remains Parent's validation. Rust1.90 formatting and exact pure-transform derivation pass.

## Scope

The comparator retains source Number/String distinctions, sender ranking, ASCII locale fallback, txid errors and equality/NaN behavior. No arbitrary Unicode collation, executable/accessor/proxy realm, unbounded JS values or authenticated financial input is added. The old local state rollback/source thrown-prefix behavior is unchanged. Array work is copied back only on successful sort, preserving the observed V8 error boundary. The source's enqueue-versus-drain payload defect and partial settlement/retry behavior remain intact.

This closes the demonstrated channel-sort capacity gap only. It does not generalize to every orderbook sort or remove external script/UTXO/proof/economic authority gates. No Root edits, whole-Core Cargo, push, PR or integration commit were performed by this worker.

## Root integration

The four new queue controls and the actual-Core financial test now pass. The financial test executes both full source scenes, including real token settlement and a later source exception retaining the earlier Tally writes. The combined library passes489 tests (four child entries ignored);16 existing channel/queue/Main-sort public checks and five locked numeric financial checks pass. All-target Clippy denies warnings; Cargo fmt,266 included files, route inventory and supply-chain checks pass. Counts overlap. Root reproduced16sort cases/32records and2financial scenes/6records under both genuine numeric profiles, byte-identically. No Root source correction or expected result change was needed.

The unchanged8bcc checkpoint goldens remain assertion-only. Source/reset fixture rows and explicit raw queue seeds are initial inputs; no post-action balance, acceptance or result is imported. The prior Capacity failure is retained as the reproducer, not counted as a successful corrected-Core execution. Verification: `fixtures/reference_fidelity/channel-queue-v8-sort-core-integration.json`. The full all-target release baseline remains2,120Rust/238numeric until a new frozen release completes.
