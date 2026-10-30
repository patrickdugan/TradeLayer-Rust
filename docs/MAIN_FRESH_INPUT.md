# Root integration status

This candidate is integrated with 648 library tests, 44 public compatibility checks and all ten new controls passing on Rust 1.90. Strict Clippy, formatting, inventory and supply checks pass. The source corpus reproduces under both genuine numeric profiles. See `fixtures/reference_fidelity/main-fresh-input-core-integration.json`. The original candidate contract follows; its remaining durable replay and fine-grained RPC continuation gates are still open.

# Explicit fresh factual input candidate

The source-only ten-case milestone3ce18858 remains immutable. This candidate adds separate typed runtime admission over the existing actual active inline service or retained-consensus caller. It does not widen the old action enum, visitor, TLMASA1/TLMACA1, Mode4,13-2 fact writer or cold readers.

## Public entry and raw input

`NativeMainFreshRpcInputV1::new(io,documents)` accepts only IO keys `height`, `blocks`, `networkInfo`, `failures` and optional complete replacement of future raw RPC documents. A document envelope has only `id`, optional `raw`, and `document`. Ordinary bounded SourceValue validation plus1024-row/16MiB wire limits apply. None keeps the existing future document set; Some(empty array) explicitly clears it. This accepts external RPC facts, not returned Main/TxIndex outcomes or financial/native row state. Every future dependency lookup still executes the original source owner.

The new canonical wire is `TLMFRI\x01` plus length-framed canonical diagnostic SourceValue, with exact re-encode required. Six standalone controls pass under actual Rust1.90 and strict Clippy against the immutable actual Core provider; old TLMASA1 and the new wire reject one another.

`SourceMainNativeActiveServiceV1::into_fresh_input_service` consumes the whole existing successfully started ordinary INLINE service. It captures the actual source head/physical entry and exposes a distinct `TL-MAIN-NATIVE-FRESH-RPC-INPUT-SERVICE-13-1` wrapper. Failure returns the same unchanged service. Automatic-file/other profiles are not inferred or admitted.

`SourceMainNativeRetainedConsensusV1::into_fresh_input_caller` moves the entire actual caller/service owner and sealed actual route into `TL-MAIN-RETAINED-FRESH-RPC-INPUT-CALLER-13-1`. Existing disjoint source operation descriptors/append/resolve methods forward to that same owner. No new first-pending selection is introduced.

Each wrapper's `feed` validates the whole input, checks the actual healthy receiver/native boundary and reserves history before publication. The Runtime calls the existing strict `source_main_step_update_io`, retaining client, counters, file facts and all finances. Validated future documents replace only the global future RPC lookup data. The retained-caller wrapper updates both its parent extractor documents and its service lookup documents in the same synchronous action. Captured block/row/source locals, actual prepared native plans, invocation generation, route, Kernel, pending timers and borrowed queue remain owned in place.

Feeds are bounded to4096 and record raw input plus the actual pre-feed receiver/native/pending boundary. Full scalar visitors stream all retained feed and source facts. These are in-memory factual admission observations; they are not durable Accepted/ACKs, an independent financial head, actor authority or a replay importer. The same source observation can retry after a capacity failure without reapplying the source action or feed. Post-exit/observation-pending feeds refuse before mutation. No owner/head release exists.

## Mount

Apply `prepare_lib` to CURRENT `lib.rs`, `prepare_runtime` to CURRENT `source_contract_runtime.rs`, and `prepare_session` then `prepare_tests` to CURRENT `source_main_native_session.rs`. Copy all `src/*.rs` leaves and the four new `fixtures/*.jsonl` files. Existing Root fixtures `native_main_service_tail_observer2.jsonl` and `native_main_inline_callers.jsonl` remain raw test-input dependencies. `prepared/` is evidence only; do not whole-file overwrite newer Root bodies.

`prepare_component.py --repository <repo>` writes only this stage, derives/formats those three additive include files and test payloads. The test transport is an exact copy of the existing lossless transport with only two unused test helpers removed; assertions/decoded source values are unchanged. `check_composition.py` verifies old code lines and seven key source/action/visitor leaves are untouched.

The four actual-Core controls pass: new live tip/block across the same10s timer with continuing ordinals; fresh documents during real financial/native validity await without reexecution; invalid/observation-pending/terminal refusal; retained Initialize's detached write/actual route/borrowed invocation surviving feed and append/resolve. Two test-only Runtime readers inspect actual retained metadata; no production fields become public or constructors are fabricated.

## Scope still open

The first source corpus includes held mock RPC Promise boundaries between raw/decoder/sender calls. Existing Rust source-owner calls are still coarse across those RPC boundaries; this candidate exposes updates at its existing retained native/timer/action boundaries, not a newly suspended RPC Promise scheduler. Those finer interleavings remain source evidence for a later explicit RPC continuation.

The new profile has no durable feed-action writer, cold history reader or same-wrapper NoIndex recovery. Native ambiguity retains the whole actual owner and refuses observation/feeds; do not replay source prefixes or reconstruct a service to recover it. All original compaction/competing-publication and native-only recovery fences remain unchanged. Main's13-2 writer is explicitly standalone-single-inline/TLMASA1 and does not admit this feed profile.

Parent owns actual Core checks, integration and commits. No Root edits, whole-Core Cargo, commit or push occurred in this lane. This isolated candidate is not a completed RC6/release claim.
