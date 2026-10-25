# Barrier-pool and parsing-throughput review

Reviewed candidate `9091bcdf03b203f599d67e3c239ebbbe9bd59159` on
26 October 2026, after the earlier [channel-pool review](TXINDEX_SCHEDULER_REVIEW.md).
Its LF-normalized `src/parallel_txindex.rs` SHA-256 is
`7ef5dbba94219f85f3baa5d1bf6e8f47b9ed4b9f56b3b99d8264838af6a3c66c`.
Two independent reviewers examined concurrency and parser/benchmark behavior.
All candidate source line references below refer to that revision. The candidate
checkout remains unchanged; neither pool is merged or selected as the default.

## P2: partial worker startup deadlocks the barrier team

`src/parallel_txindex.rs:696-697` creates barriers for all requested workers plus
the coordinator. Workers immediately enter the first barrier at line 712.
If `scope.spawn` at line 711 fails after some workers start, the coordinator
unwinds before the normal stop handshake at lines 806-807. The scope then waits
for workers that cannot satisfy the fixed barrier's participation count.
Rust 1.90's `Scope::spawn` panics on OS thread-creation failure, while
`Builder::spawn_scoped` returns that failure as an I/O result.
[Pinned standard-library implementation](https://github.com/rust-lang/rust/blob/1.90.0/library/std/src/thread/scoped.rs#L195-L255).

The standalone probe requests four workers and injects a panic immediately
before the selected spawn. This models the documented failure path without
exhausting actual OS resources:

| Already started workers | Observed result |
| --- | --- |
| 0 | Prompt test failure, process exit 101 |
| 1 | Hung; exact owned child killed and waited after four seconds |
| 2 | Hung; exact owned child killed and waited after four seconds |
| 3 | Hung; exact owned child killed and waited after four seconds |

A recovery needs fallible creation and a cancellable startup gate. Started
workers must stay outside the fixed barriers until the complete team exists.
On a creation error, cancel/notify and join the successful starts. Merely using
a fallible spawn and then entering the existing stop barrier still deadlocks.

`scripts/check_barrier_faults.py` verifies the pinned source hash, includes the
unmodified scheduler in a Rust harness, and injects startup faults only into
temporary copies. Its default successful outcome **characterizes the hang**;
it does not assert successful recovery. Timeouts are bounded, and the driver
kills and waits for only its own direct child. An external checkout can be
passed explicitly; otherwise it reads the pinned Git object without fetching:

```text
python scripts/check_barrier_faults.py --source /path/to/candidate/src/parallel_txindex.rs
```

Use Rust 1.90.0 and its normal platform linker/SDK environment. The nine
unmodified-source tests pass: eight scanner-fault cases and normal startup.
They exercise ordering across multiple windows, duplicate/ordinary-error
selection, scanner panics, early-error shutdown and repeated fresh teams.
All four startup probes reproduce the table above.

## Proposed startup recovery, tested on a temporary copy

`fixtures/barrier_startup_cancellation.patch` switches only the barrier pool's
creation to `Builder::spawn_scoped` and places workers behind a mutex/condition
variable startup gate. A guard inside the scope closure cancels that gate on
early return or unwinding, before the scope joins workers. On an explicit spawn
error it cancels, joins the successfully started workers and returns the
existing `WorkerPanicked` error. After all starts succeed, a latched ready
state releases the complete team into the unchanged window barriers.

The exact LF patch SHA-256 is
`b7892c22b9ea848509e56b440fec64a49c90e8bcad0a6820a626c0c11de80a3f`;
the resulting scheduler SHA-256 is
`87985bf7d84af7202936e71f953622ba738fd4c05ac63019747a6fc824f94367`.
`scripts/check_barrier_startup_fix.py` verifies the original hash, strictly
applies the pinned unified patch in memory, verifies the resulting hash and
compiles only temporary files. Formatting and `git apply --check` against the
candidate also passed. Neither candidate nor local-main scheduler is changed.

The proposed copy passed nine normal tests. Four separate probes inject an
I/O error at the fallible spawn-result boundary after 0/1/2/3 successful starts;
all return `WorkerPanicked` promptly and assert zero scanner callbacks. These
model the new API's error result, whereas the original reproduction models
the old API's panic. Four optional startup-panic probes also exit 101 promptly,
demonstrating cancellation during unwinding. The expected panic exits are
successful probe observations, not successful scheduler calls.

```text
python scripts/check_barrier_startup_fix.py --source /path/to/candidate/src/parallel_txindex.rs --startup-unwind-probes
```

An independent reviewer checked cancellation ordering, guard lifetime, the
latched ready state and condition-variable predicate, and found no further
startup defect in this proposal. Actual OS exhaustion, post-start coordinator
panics, performance effects and consensus remain outside this verification.

## Failure parity and normal synchronization

The barrier worker continues after ordinary scan errors at lines 732-741.
Its same-claim error-followed-by-panic test therefore returns `WorkerPanicked`,
matching the original windowed scheduler. The channel pool still has the early
break documented in the prior review; success of the barrier test does not fix
that separate implementation.

For a completely created team, the normal start/completion/stop ordering is
consistent with the tested source-order results. Workers drop their window Arc
before the completion barrier, then the coordinator clears publication before
unwrapping it. Publication-poison handling received static review; these probes
do not inject a poisoned publication mutex. They substitute the prepared scanner
with deterministic delays, errors and panics, so they do not exercise actual
parsing, hashing, witnesses or authenticated admission. OOM abort, panic-abort
builds and arbitrary failures outside these injected sites remain untested.

## Parser and benchmark integration findings

1. **Preserve the parser correction already on local main.** Candidate
   `src/read_only.rs:219-225` still rejects a nonempty witness stack containing
   an empty item. The [parser review](TX_PARSER_REVIEW.md) and independent
   vectors on local main correct this inherited syntax bug. Integration must
   retain that correction.
2. **P2: pool timing still excludes full ingestion.** Candidate benchmark
   lines 157-159 measure the barrier pool on an already parsed view. The full
   helper at `src/raw_block_ingest.rs:503` still selects the original windowed
   scheduler. Candidate fixed ordering, at most five repeats and reference
   equality inside its timer also remain. Port local main's corrected
   measurement method to both pool variants before comparing them.
3. **P2: configured workers ignore window width.** Barrier line 694 and channel
   line 519 cap workers by total transaction count. A one-transaction window
   can therefore wake sixteen barrier workers twice per transaction. Also cap
   the team by the maximum concurrent window width and consider a serial
   fallback when that width is one. This is a source-derived opportunity;
   no speedup is measured here.
4. **Serial work remains inside prepared-view results.** Boundary discovery
   serially walks transaction layouts at `src/raw_block_ingest.rs:111-132`.
   Workers then hash prepared slices; source-order duplicate reduction and
   Merkle construction remain serial. The existing prepared hash already
   hashes disjoint stripped ranges without a stripped-transaction copy.
   Do not attribute all measured time to dispatch or worker hashing.
5. **P3: temporary slots are not total memory.** Each barrier window allocates
   claims, slots and an Arc at lines 759-767. Prepared references, layouts,
   ordered observations, duplicate maps, carrier payloads and Merkle buffers
   remain proportional to the block. The 512-slot limit bounds temporary
   observations; it proves neither total RSS nor zero per-window allocations.
6. **P3: the workload lacks useful skew.** The generator repeats two small,
   carrier-bearing templates. Vary size, witness size, carrier density and
   same-block dependency chains, then compare allocations and full pipelines
   before selecting weighted scheduling or reusable window buffers.

## Independently counted SHA work

`scripts/count_synthetic_ingest_hash_work.py` independently walks the pinned
fixture bytes and counts padded SHA-256 compression calls implied by the
generator's transaction lengths and Merkle tree. Its input-byte SHA-256 is
`3e3113b5051e517ae188901511dd312bddc7c4eea0c3710c7094b8f73bb589f3`.
It imports no Rust cursor/layout code. It counts transaction hashes and Merkle
hashes only, excluding parsing, carrier work, maps, allocation, synchronization
and later validation.

| Transactions / witness percent | Transaction hash calls | Serial Merkle calls |
| --- | ---: | ---: |
| 1,000 / 0 | 3,000 | 3,003 |
| 4,000 / 50 | 17,997 | 12,003 |
| 10,000 / 95 | 58,497 | 30,015 |

For the last case Merkle accounts for 33.9% of these **counted compression
calls**. This is not an elapsed-time fraction or an overall serial-work bound.
Hash input sizes alone cannot establish either quantity.

## Local-main measurement change and smoke results

The corrected main benchmark now rotates nine phases and adds `txid_merkle`.
That phase uses precomputed TXIDs and times the existing serial helper's hashes
and level allocations. TXID materialization and root equality checks are outside
the timer. Merkle work remains inside every prepared/full ingestion phase too;
the separate metric isolates its cost and must not be added to those totals.
It changes instrumentation only, without selecting a scheduler or altering
parser/consensus behavior.

Pinned Rust 1.90.0 MSVC release on the shared Windows host produced the following
smoke observations, with nine repeats and every ordered-equivalence check
passing. The one-transaction/one-worker case also passed. No CPU affinity,
power-policy control or confidence interval was used; no new barrier-pool
throughput measurement was taken.

Formatting, all-target Clippy with warnings denied and the frozen release
benchmark build passed. Output checks verified nine sample fields and exactly
the requested repeat count for each smoke run. The independent work counter
also passed its one-transaction boundary and the table cases above.

| Transactions / workers / witness percent | Full windowed ms | Full one-team ms | Boundary parse ms | Dependency plan ms | Serial Merkle ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1,000 / 4 / 50 | 3.864 | 3.022 | 0.156 | 0.589 | 0.829 |
| 10,000 / 8 / 95 | 56.939 | 32.194 | 2.309 | 13.894 | 11.323 |

These are synthetic local indexing observations, not consensus throughput.
Real RPC, authenticated parent UTXOs, scripts, canonical chain selection and
TradeLayer state application remain outside these benchmarks.
