# Per-block cost of the economic owner

Measured 2026-11-05 on main 9e96eb6 with `wasm/examples/bench_playground.rs`, extended locally with
timers around `EconomicState::verify`, `clone` and `snapshot_bytes`.

Workload: the wasm playground schedule, fixed-margin mode with marks off. Each block carries an
oracle publication, the market maker's cancel-all plus a 3-level ladder per side, and a taker order
with 60% probability. 1,000 blocks, seed 1. Native release build in WSL (Ryzen AI 9 365).

| block | mean ms/block (last 100) | `verify` ms | `snapshot_bytes` ms | `clone` ms | snapshot bytes |
|---:|---:|---:|---:|---:|---:|
| 100 | 2.5 | 1.0 | 2.9 | 0.03 | 145,192 |
| 300 | 9.2 | 2.3 | 7.0 | 0.09 | 393,376 |
| 500 | 18.1 | 4.9 | 13.2 | 0.27 | 666,215 |
| 700 | 25.4 | 6.5 | 20.3 | 0.34 | 926,501 |
| 1,000 | 38.8 | 8.8 | 24.8 | 0.73 | 1,319,020 |

## Where the time goes

`apply_block_unverified` audits the complete state twice per block:

1. `begin_block_stage_unverified` calls `verify()` on the starting state.
2. `finish_internal` calls `snapshot_bytes()`, which calls `verify()` on the new state, verifies the
   joint state again inside `snapshot_for_economic_bytes`, and encodes the complete state to prove
   it serializes.

`verify()` rebuilds a set of every input id ever admitted and compares it with the joint replay
set. It checks every internal leg sequence against the ledger, rebuilds the order book from its
snapshot, and re-derives every custody balance. The state keeps its full history: trade receipts,
replay markers and leg ids. It grows by about 1.3 KB per block in this workload, so each block costs
time proportional to all earlier blocks, and replaying N blocks costs O(N^2).

Accepted-contract mode adds a third term: `verify_contract_binding` re-executes the complete input
history from the committed bootstrap on every block, about 1.3 s per block by block 45.

## What is not an option

Skipping or batching these audits while keeping the same guarantees is not possible:

- A batch that verifies only at its end accepts a state that breaks an invariant at block k and
  repairs it by block n. The per-block path rejects block k.
- Removing the start-of-block `verify()`, which is the only redundant pass when the previous block's
  finish produced the state, saves about 23% and leaves the growth linear.
- Faster encoding is also a constant factor.

## Recommended plan

1. **Incremental invariants with periodic full audits.** Maintain the cross-checked aggregates as
   blocks apply instead of rebuilding them: the economic input-id set against the joint replay set
   (count plus an order-independent hash), per-account custody sums, the leg high-water mark, and
   the reserve totals against the resting book. Per block, check only the aggregates and the
   touched accounts, orders and positions. Run the complete `verify()` plus `snapshot_bytes()`
   every K blocks (K = 64), on every persisted checkpoint, and whenever an incremental check fails.
   Per-block cost becomes O(block); full audits amortize to O(state / K).
2. **Bounded history.** Move trade receipts out of the state into an append-only receipt log whose
   hash is committed in the head. Compact replay markers below the rollback floor into a sorted,
   hashed archive, because a Bitcoin txid buried below the reorg window cannot be re-admitted. This
   changes the TLEC snapshot format and needs a version bump with migration.
3. **Accepted-contract mode.** Carry the binding replay state forward and check each new frame
   against it. Re-execute from the bootstrap only at the full-audit checkpoints of step 1.

Steps 1 and 3 change when a corrupted state is detected: within K blocks instead of within the
block. Every block is still validated, and the consensus result of every valid history is
unchanged. Step 2 changes the persisted format and is not implemented. Steps 1 and 3 are
implemented as described below, with the project owner's approval.

## Audit policy (steps 1 and 3, as built)

`EconomicState` carries a runtime-only `AuditPolicy`: `EveryBlock`, or `Periodic(K)` with the default
`Periodic(64)`. `set_audit_policy` and `with_audit_policy` change it; clones keep it. It is not
serialized and is excluded from `Eq` and `Debug`, so snapshot bytes and state equality do not depend
on it. A state also carries an "audited" marker naming the policy of the block that produced it in
this process; clones keep it, `Eq`, `Debug` and snapshots ignore it, and loaded, decoded or newly
constructed states have none. An interval of 0 or 1 audits completely every block.

`EveryBlock` is the code path that existed before: `verify()` on the starting state, then
`snapshot_bytes()` (which verifies) on the finished state.

Under `Periodic(K)`, on a configuration whose history checks can be split (plain owner, accepted
contracts, funding, marking, liquidation):

1. The start-of-block `verify()` runs only when the starting state does not carry the marker for this
   policy.
2. At finish, the full state is still encoded every block, so every `count(.., MAX_*)` and
   `MAX_CHECKPOINT_BYTES` bound rejects the same block at the same height as before. The encoder
   variant skips the internal audits; `snapshot_bytes()` itself still verifies.
3. A height that is a multiple of K runs the complete `verify()` on the finished state. Every other
   height runs the live checks and the delta checks below, and the finished state gets the marker.
4. A failed audit fails the finish. The caller's state is untouched, the block is rejected, and the
   error is the one `verify()` returns. A corruption in history that a block did not touch is
   therefore detected at most K-1 blocks late, at the next multiple of K.

| check | cost | per block under `Periodic(K)` | at multiples of K |
|---|---|---|---|
| config, book structure and reserves, positions, margins, custody balances, IOU balances, fee custody and fee-book structure, caps | live | complete | complete |
| book `from_snapshot` round trip | live plus seen set | `verify_structure_unverified`: the same checks run in place, no copy of the seen set | complete |
| input ids: seen, cancel and contract-creation ids against the joint replay set | history | category sizes sum to the joint set's size; this block's order and cancel ids and new contracts are members | complete rebuild |
| leg ids | history | this block's legs are processed | every leg |
| receipts: order, bounds, seen ids | history | this block's receipts | every receipt |
| loss pool, collected and paid totals, bad debt | history | increments of this block's receipts and mark events against the audited start | complete sums |
| fee book against receipts | history | the start fee book replayed over this block's receipts, journal suffix compared | complete rebuild |
| fee-book journal entries | history | entries added by this block | every entry |
| accepted contracts, input frames | history | contracts and the frame added by this block | every contract and frame |
| accepted-contract re-execution | history | the replay carried from the previous block executes this block's frame and must equal the state | re-execution from the bootstrap; the result is carried forward |

Kept in the per-block path because no sound split exists without a journal in the joint state or a
different encoding:

- The joint audit (ledger conservation and reserves, custody fence, and the union of ledger,
  registry, spot and economic id sets against the global set). It runs once per block inside
  `finalize_block_for_economic_unverified` instead of four times.
- `verify_funding`, `verify_marks`, `verify_liquidations` and `verify_system_epochs`: they replay
  event histories and only run when the matching feature is configured.
- The fee derivation of a liquidation configuration, which interleaves forced and AMM fills with
  receipts, keeps its complete rebuild.
- The full-state encode of requirement 2.

Configurations with native entitlements, source consensus, the insurance fund or the funded AMM, and
states that are replaying history, run the complete audit on every block under either policy.

Accepted-contract mode keeps the replay state of the previous block. A block executes its frame on a
copy of that replay and compares the result with the new state. The replay itself runs under
`EveryBlock`, as the complete re-execution does, so its cost per block is one audited block. A
state without a carried replay re-executes from the bootstrap.

### Tests

`tests/economic_audit_policy.rs` runs each scenario under `EveryBlock`, `Periodic(64)`,
`Periodic(1)` and `Periodic(7)`, and asserts the same `Ok` or `Err` and the same snapshot bytes
(length and 128-bit digest) after every block, including failed blocks:

- six histories in five tests, ported from `tests/economic.rs` and `tests/economic_review.rs` (open,
  reduce, flip; deferred claims; unfunded loss; cancellation and replayed ids; last-leg failure with
  retry; partial fill and self-cross);
- 1,000 blocks of market-maker and taker flow with injected failing blocks;
- accepted-contract mode with a failing block in the history (200 blocks in release builds, 70 in
  debug builds, because the `EveryBlock` reference re-executes every frame three times per block);
- capacity: the book filled to `MAX_CONTRACT_ORDERS` and one more order, a block with more than
  `MAX_ECONOMIC_INPUTS` inputs, and trading blocks (release builds run into
  `MAX_ECONOMIC_RECEIPTS` and every policy rejects the same block; debug builds stop at about 2,500
  receipts).

`src/economic_audit_tests.rs` injects a violation with a `cfg(test)` hook: a changed fee on the
oldest receipt, which no later block touches. `Periodic(8)` accepts blocks 3 to 7 and rejects block 8
with `Invariant`, publishing nothing; `EveryBlock` rejects block 3. A state without the marker
audits completely before its first block.

### Measurements

Playground schedule, 1,000 blocks, seed 1, marks off, native release build in WSL on the machine
above. Mean ms per block over the last 100 blocks ending at the listed block; median of 5 runs
before and 6 runs after. `wasm/src/playground.rs` uses the default policy.

| block | before | after |
|---:|---:|---:|
| 100 | 2.5 | 1.6 |
| 300 | 8.9 | 5.3 |
| 500 | 16.7 | 9.9 |
| 700 | 23.8 | 13.8 |
| 1,000 | 38.3 | 23.4 |

Both builds produce the same checksum (596 fills, final oracle price 972083477).

The cost still grows with history, at about 0.021 ms per block instead of 0.034. What remains is
mostly requirement 2 and work outside the audit: the full-state encode (about 11 ms at block 1,000 on the market-maker test schedule),
the joint audit at finalize, the state and joint clones, and the staged ledger copy that every
internal leg makes. Step 2 (bounded history) is what removes the encode and the copies.

Accepted-contract mode, 200 blocks of the test schedule (a price every tenth block, a trade every
fifth), release build, apply time only: `EveryBlock` 101.9 s, `Periodic(64)` 1.6 s.
