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
unchanged. Step 2 changes the persisted format. Steps 1 and 3 are implemented as described under
"Audit policy", and step 2 as described under "Bounded history", with the project owner's approval.

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
  `MAX_ECONOMIC_INPUTS` inputs, and trading blocks (release builds settle 10,000 fills, more than
  `MAX_ECONOMIC_RECEIPTS`, with the same result under every policy; debug builds settle 3,000).

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
internal leg makes. Step 2 does not remove them: the encode and the copies are proportional to the
replay markers and legs, which stay exact (see "Bounded history").

Accepted-contract mode, 200 blocks of the test schedule (a price every tenth block, a trade every
fifth), release build, apply time only: `EveryBlock` 101.9 s, `Periodic(64)` 1.6 s.

## Bounded history (step 2, as built)

Step 2 bounds the trade receipts and the fee journal and raises every other history bound. It does
not bound the replay markers or the internal legs: those stay exact, and they are now the whole of
the state's growth.

### What leaves the state

When `marking` is not configured, the state keeps the `RECEIPT_WINDOW` (256) most recent receipts.
At the end of every block, after the block's audits and before the checkpoint encode, every older
receipt moves to the receipt log. Their fee-book journal entries leave the state with them, because
each entry is a function of one receipt. The state commits to what left through a `ReceiptArchive`:

| field | meaning |
|---|---|
| `count`, `last_height` | receipts archived, height of the last one |
| `hash` | running hash: `H[i+1] = sha256d("TradeLayer/EconomicReceiptLog/v1" ‖ H[i] ‖ receipt[i])`, receipt in its TLEC encoding |
| `collected_atoms`, `paid_atoms`, `unfunded_loss_atoms` | totals the loss-pool and bad-debt audits need |
| `insurance_atoms` | insurance requests of the archived accruals |
| `fees` | fee-book rows after the archived accruals, with no journal |

Every reader of `receipts` keeps its result:

| reader | now |
|---|---|
| capacity of `settle_match` and of `verify` | counts the receipts in the state (8,192) |
| complete audit, order and bounds of receipts | the window, plus every archived receipt when the node holds the log |
| loss pool, collected, paid, bad debt | archive totals plus the window |
| fee-book rebuild and insurance expectation | archive rows and insurance plus the window's accruals, journal compared |
| incremental audit | unchanged; the ordering check starts at `last_height` |
| `EconomicHistory` reports | require `marking`, so they never read an archived receipt |
| wasm playground, listener demo | `receipt_count()` and `receipts_since(n)` |

Configurations with `marking` (marks, liquidation, funded AMM, accepted contracts, insurance,
native entitlements, source consensus) pair every receipt with an event in a history that replays
from the first receipt. They archive nothing and keep the 8,192-receipt bound together with the
bounds of those histories (8,192 mark events, 4,096 liquidation events, 4,096 contract frames).

### The receipt log

The log is append-only node data, shared between state clones in chunks of 256 receipts. It is not
in the snapshot, in `Eq` or in `Debug`. A state decoded from a snapshot starts with an empty log
whose base is the archive head; `attach_receipt_log_prefix` joins the earlier receipts after
checking that they chain to that base hash, and `prune_receipt_log` drops the node's copy. A
rollback in `EconomicChain` keeps the log that still chains to the restored head.

The complete audit checks the log against the commitment: the hash chain must reach the committed
hash from the log's base, and every receipt must pass the bounds and ordering checks. A log that
starts at the first receipt also recomputes the committed totals and fee rows. A tampered archived
receipt fails the complete audit with `Invariant`; under `Periodic(K)` that is the next multiple of
`K`. The audit costs one hash per receipt in the log, so a pruned node pays nothing.

### Layout and migration

TLEC v11 (`TLEC\x0b`, then one byte: 0 plain, 1 funding) is v1 or v2 with the archive after the
receipt list. A state that has archived no receipt keeps writing v1 or v2, byte for byte, so every
pinned fixture and every state with at most 256 receipts is unchanged. The reader decodes v1 to v10
as before. A state decoded from v1 or v2 holds every receipt; its first block, or an explicit
`migrate_receipt_history_unverified()`, archives down to the window, after which it writes v11 and
has the same snapshot bytes as a node that never held more. A snapshot taken before the first
archive therefore loads in an `EconomicChain` without re-encoding; the chain's rollback frames keep
their original bytes. `TLSE` state export accepts versions 1 to 7 and 11.

### Replay markers: kept exact

Compaction into a sorted archive does not reduce what the state must hold. Exact replay protection
for an arbitrary 32-byte id needs 32 bytes per id in the snapshot, in an archive as much as in a
set; an archive would add a hash and a binary search and remove nothing. Nothing in the owner binds
a txid to a block height, so an id below the rollback floor cannot be dropped without accepting a
replayed id. Replay protection is therefore unchanged for every id, and the caps rise:

| bound | before | after |
|---|---:|---:|
| order ids retained by the book | 65,536 | 524,288 |
| replay markers of the joint state | 100,000 | 1,048,576 |
| internal legs | 65,536 | 1,048,576 |
| replay markers of a ledger inside an economic checkpoint | 100,000 | 2,097,152 |
| receipts settled over the life of the state | 8,192 | no bound |
| fee-book journal entries over the life of the state | 16,384 | no bound |

The ledger bound applies only to the ledger embedded in an economic or clearing checkpoint; the
standalone ledger checkpoint keeps its 100,000 bound.

### Ceiling

The checkpoint holds 64 MiB (`MAX_CHECKPOINT_BYTES`), and that byte budget now decides before any
count. Measured on the playground schedule below, 8.6 input ids and 10.7 internal legs per block
grow the state by 1,136 bytes per block: 342 bytes in the ledger replay markers (32 bytes per leg)
and 794 bytes in input-id sets and other per-input records (92 bytes per input id). Before step 2
the growth was 1,319 bytes per block.

An `EconomicChain` checkpoint embeds the state and one complete before-state per rollback frame, so
it holds `1 + rollback_limit` copies of the state:

| copies in the checkpoint | blocks until 64 MiB, before | after |
|---|---:|---:|
| 1 (state alone) | 50,878 | 59,074 |
| 2 (`rollback_limit` 1) | 25,439 | 29,537 |
| 7 (`rollback_limit` 6) | 7,268 | 8,439 |
| 65 (`rollback_limit` 64) | 782 | 908 |

The count bounds before step 2 stopped this schedule at about 6,100 blocks (legs), 9,900 blocks
(order ids) and 13,400 blocks (receipts). After it they stop it at about 98,000 (legs), 79,000
(order ids) and 122,000 (markers), so the byte budget decides. A chain with `rollback_limit` 64
halts at about 900 blocks on this schedule, before and after: the frames duplicate the replay
markers 65 times. Removing that limit needs the frames to share the markers (a delta layout for
`TLEH`) or leg ids to stop being stored; neither is part of step 2.

### Tests

`src/economic_history_tests.rs` lowers the receipt window, the receipt capacity and the order-id
bound through a test-only setter (window 4, capacity 16) and runs under `EveryBlock`, `Periodic(1)`,
`Periodic(7)` and the default policy, with and without funding:

- 120 fills settle where the control state with the same capacity stops at fill 17 with `Capacity`;
  the state holds 4 receipts, the log 116, and `verify()` passes;
- the four policies write identical snapshot bytes after each of 59 blocks;
- v1 and v2 bytes before the first archive, v11 after, round trip, rejected corrupt shape and
  config bytes, and a v11 header without an archive;
- a v1 or v2 snapshot with 40 receipts reaches the same snapshot bytes, ledger, positions, fee book
  and archive commitment as a node that ran the new code from block 1, after 39 more blocks, and the
  explicit migration gives the same state at once;
- replayed order and cancel ids from before the first archive are rejected, also after a snapshot
  round trip;
- a changed archived receipt fails `verify()`, fails block 8 under `Periodic(8)` with nothing
  published, and fails the next block under `EveryBlock`; a changed committed total fails without a
  log; a pruned log verifies, agrees with the full node block for block, and takes back exactly its
  own prefix;
- a rollback keeps the log up to the restored head;
- a book restored with 70,000 retained ids admits a new order.

`tests/economic_audit_policy.rs` settles 10,000 fills in 10 blocks (release builds) under every
policy with identical results, beyond the 8,192 receipts a state could hold before.

### Measurements

Playground schedule of the table above, seed 1, marks off, native release build in WSL on the same
machine, mean ms per block over the 100 blocks ending at the listed block. Before is `b75f9cc`.
Median of 3 runs, except before at 2,000 to 5,000 blocks (one run).

| block | before | after |
|---:|---:|---:|
| 100 | 1.47 | 1.41 |
| 300 | 4.90 | 5.00 |
| 500 | 9.08 | 9.15 |
| 700 | 13.05 | 13.56 |
| 1,000 | 18.59 | 18.51 |
| 2,000 | 40.29 | 38.73 |
| 3,000 | 61.14 | 58.93 |
| 4,000 | 83.55 | 80.22 |
| 5,000 | 104.14 | 100.28 |

| after block | state bytes before | state bytes after | fills | input ids | ledger markers |
|---:|---:|---:|---:|---:|---:|
| 1,000 | 1,319,020 | 1,218,503 | 596 | 8,604 | 10,781 |
| 5,000 | 6,594,456 | 5,763,681 | 3,055 | 43,063 | 53,439 |

Both builds produce the same checksum at 1,000 blocks (596 fills, oracle price 972083477) and at
5,000 blocks (3,055 fills, oracle price 990902441). The time per block does not change beyond the
run-to-run spread of about 3%, and still grows by about 0.02 ms per block. Receipts and the journal
were 13% of the state at 5,000 blocks; the rest is replay markers and legs. The finisher still
encodes the whole state in every block (requirement 2 of the audit policy), and the joint audit and
the ledger copies of each leg run over the markers, so the time per block follows the size of the
state. Taking the markers out of that per-block work is not part of step 2.
