# Pooled transaction-index scheduler review

Reviewed candidate `d4ba7592de3a93ee59ded6ebb2cd0cec91174368`
(`perf/stage8d4-pooled-weighted-windows`) against baseline `c7a37eb`.
The candidate's LF-normalized `src/parallel_txindex.rs` SHA-256 is
`9c27880fb09a92467f69c254bad1735fb8ffd5e295aeddcc86b6309938b5c4a5`.
This review covers read-only CPU scheduling and the existing dependency-plan
boundary. The candidate checkout remains unchanged. A proposed fix is supplied
as a unified diff and exercised only on a temporary source copy; it has not
been applied or merged into the candidate. This review does not implement the
separate parent-UTXO/script-authentication work.

## Result

**P3: a scan error can suppress a later panic in the same pooled claim.**
The exact candidate and the original windowed scheduler can therefore return
different errors for the same injected faults. This is latent in the current
public raw-block path: successful strict preparation prevents the ordinary
prepared-scanner errors used by this reproduction. It is not a current
authorization bypass, but it contradicts unconditional panic-precedence parity.

Eight fault-injection tests passed against the unmodified candidate, including
an explicit characterization that asserts this divergence. All eight also
passed against a temporary source copy with the proposed fix; in that mode the
new case requires both schedulers to report `WorkerPanicked`. Four separate
baseline-compatible dependency regressions passed. Checks used Rust 1.90.0 on
Windows/MSVC and establish neither production throughput nor consensus
acceptance. The bounded pool remains an opt-in, untrusted preindex candidate.

## Same-claim panic suppression and proposed fix

Using the existing harness's `rows(140)` with two workers and a 512-row window,
set `rows[1][0] = 1` to return an ordinary scan error and `rows[2][0] = 2` to
panic. The weighted policy puts both rows in the first claim, positions `0..13`,
with target work 8,512 bytes. The original windowed scanner executes row 2
despite the error at row 1 and returns `WorkerPanicked`. The candidate's early
`break` at `src/parallel_txindex.rs:545-548` skips row 2 and returns
`RawScan { index: 1, source: Truncated }` instead. The previous mixed-fault test
used positions 3 and 19 in separate claims and did not expose this case.

`fixtures/pooled_panic_parity.patch` removes only that ordinary-error early
exit, allowing the remainder of each dispatched claim to run. Ordered reduction
still selects the earliest ordinary error or duplicate when no panic occurs;
an executed panic retains the existing window-level precedence. The opt-in
driver verifies the original pinned hash, checks that the edit matches exactly
one location and the supplied unified diff, and applies it only to temporary
source. Its compiled source's LF-normalized SHA-256 is
`acf794d8746b0312a899bb96d03420836bf078f02190df11463c4583dad1531c`.
This is tested proposed behavior, not an upstream candidate correction.

## Concurrency, ordering and resource analysis

References below use one-based candidate source lines at the pinned commit.

| Concern | Source | Assessment and exercised evidence |
| --- | --- | --- |
| Dispatch/receive deadlock | `src/parallel_txindex.rs:520-522, 576-591` | The task queue is bounded to `2 * workers`; result sends use the standard unbounded channel. Workers can finish and release task-queue capacity while the coordinator is still dispatching. There is no bounded-result-queue circular wait. Source inclusion tests cover 1,025 rows with 1/7/64/512-row windows and 2/4/16 workers. |
| Receiver mutex | `src/parallel_txindex.rs:534-543` | The mutex covers `recv`, then drops before hashing. One idle receiver may hold it while waiting, but publishing a task does not require that lock. Hashing/classification run outside the mutex. This may affect throughput; it is not a reproduced deadlock. |
| Job panic and shutdown | `src/parallel_txindex.rs:539-563, 635-648` | Scanner execution and local result allocation are within `catch_unwind`. A panic yields one result for that claim, so gather can finish. Closing the task sender precedes all joins. Injected panics in first/middle/last windows, and in every job, returned `WorkerPanicked`; subsequent fresh pools also completed. |
| Ordinary early failure | `src/parallel_txindex.rs:545-548, 612-628` | A claim stops at its first failed row. Reduction encounters that error before the shortened-claim length check. An injected slow failure at position 3 consistently beats a faster failure at position 19. |
| Panic precedence | `src/parallel_txindex.rs:545-548, 599-610` | An executed panic wins over other failures within the same window. A preceding ordinary error in the same claim can skip that panic, producing the P3 mismatch above. Separate-claim comparisons agree; the new same-claim characterization distinguishes unmodified and proposed modes. An earlier window's ordinary error stops before a later window is dispatched. |
| Duplicate ordering | `src/parallel_txindex.rs:612-625` and `append_observation` | Gather restores claim order, then each claim's transaction order. A duplicate at position 700 referencing position 2 beats an error at 900 across windows; an error at 3 beats a duplicate at 100. |
| Window memory bound | `src/parallel_txindex.rs:568-587` | Only one window's claims are dispatched before all its results are gathered and reduced. Claims partition the window, with at most 32 rows each. Local/result buffers collectively cover at most the configured 512 transaction observations, in addition to the accumulated report and metadata. The unbounded result channel does not allow multiple undispatched windows to accumulate. No peak-RSS claim is made. |

The fault harness substitutes only the prepared scanner with deterministic
delays, scan errors and panics. By default it includes the candidate's complete
scheduling source unmodified; the explicit patch mode includes the temporary
copy described above. This directly exercises error/shutdown branches that are
normally unreachable through the public raw-block API after successful strict
preparation. It does not test the real parser, hashing or carrier classification.
Abort-on-OOM, OS thread creation failure, panic-abort builds and arbitrary
future panics outside the job catch boundary are not characterized by the
injected tests. Real scanner equivalence remains covered by the candidate's
existing integration tests and the repository's full checks.

## Dependency binding: observed limitation, not a current authorization bypass

`src/raw_block_ingest.rs:333-335` binds a supplied preindex by header hash,
TXID Merkle root and transaction count. A different header with the same
transactions is rejected. The four new integration regressions verify that
check, a fanout/join dependency graph, and a duplicated prevout within one
transaction.

A witness-only mutation leaves the TXID Merkle root and block header unchanged.
The regression `witness_only_preindex_alias_remains_a_dependency_hint_and_cannot_authorize_scripts`
changes the final payload byte of synthetic transaction 2's witness. Its TXID
and header remain equal, its wTXID changes, and the altered view accepts the
original preindex for dependency planning. Its DAG equals the freshly indexed
altered block's DAG because witness bytes do not change prevouts. All parent
UTXO, script/coinbase and block-application trust gates still return errors.
The behavior is therefore an explicit boundary of the existing scheduling
hint, not a security regression introduced by the worker pool.

Before any future API reuses this object as a script-validation result or
authenticated execution plan, bind it to the exact source transaction/witness
snapshot and verified witness commitment, plus the authenticated parent UTXO
snapshot and fork context. A header/TXID-only comparison must not stand in for
that binding. This is an integration requirement for the separately owned
authentication work, rather than a request to relax the current denied gates.

## Reproduction

Baseline-compatible regressions, run in the main Rust checkout:

```text
cargo test --frozen --test txindex_scheduler_review
```

Exact-source fault tests, with no Python/Rust package dependencies:

```text
python scripts/check_scheduler_faults.py --git-ref d4ba7592de3a93ee59ded6ebb2cd0cec91174368
python scripts/check_scheduler_faults.py --source /path/to/candidate/src/parallel_txindex.rs
python scripts/check_scheduler_faults.py --git-ref d4ba7592de3a93ee59ded6ebb2cd0cec91174368 --apply-panic-parity-fix
```

The driver checks the normalized source hash before compiling a standalone
Rust test crate from `fixtures/txindex_scheduler_fault_harness.rs.txt`. It
executes all eight tests with a 45-second whole-process timeout, so a deadlock
fails the check. It writes temporary files under `target/scheduler-review`
by default and verifies the resolved cleanup target stays below the chosen
scratch parent. `--scratch-root`, `--rustc` and `--timeout` support restricted
environments. Windows requires an initialized MSVC linker/SDK environment.
Source-file mode against the unchanged isolated candidate completed all eight
tests. Source-ref mode against the locally available pinned commit with
`--apply-panic-parity-fix` completed all eight tests using Rust's
`--cfg panic_parity_fixed`. The eighth test intentionally asserts different
outcomes in the two modes; a default PASS does not mean panic equivalence.
The driver emits its mode, the original source hash, the compiled source hash,
and `candidate_checkout=UNCHANGED`. The default compiled hash equals the
original pinned hash; only the proposed mode emits the patched hash above.
Local run output is retained in the task's scratch `work` directory as
`scheduler-eight-original.txt` and `scheduler-eight-patched.txt`.
Source-ref mode requires that commit to be available in the local repository.
No remote is fetched.

The candidate's source hash intentionally excludes line-ending differences.
Changing the scheduler requires a new review; supplying a different
`--expected-sha256` only changes what bytes the harness compiles and is not
evidence that those bytes have received this review.
The proposed patch mode requires the original pinned hash even if a different
`--expected-sha256` is supplied.
