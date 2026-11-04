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
unchanged. Step 2 changes the persisted format. These are consensus-safety decisions for the
project owner and are not implemented.
