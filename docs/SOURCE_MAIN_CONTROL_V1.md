# Main control session V1

`SourceMainControlSessionV1` is an explicit control boundary that takes ownership of an existing `SourceContractReplay`. Ordinary controls call that actual owner's existing `apply`; there is no replacement runtime, financial snapshot import or second ledger. The new API is opt-in and leaves all V1–V12 command semantics, receipts and TLCR checkpoint encodings unchanged. In particular, old `Invoke(Main.shutdown)` keeps its old result; it is not silently reinterpreted.

Use `SourceMainControlCommandV1::MainShutdown(arguments)` to execute the terminal control. The pinned Main method has no parameter use, receiver read, await, real persistence write or normal return before `process.exit(0)`. The terminal result is `SourceMainControlOutcomeV1::Exited(SourceMainProcessExitV1 { code: 0 })`, with exactly these ordered effects:

1. Console log: `Saving state to database...`
2. Console log: `Shutdown completed.`
3. Process exit code 0.

The first message describes the source text; no state-saving operation exists in the body. The wrapper records effects as data and never exits the Rust/host process. It also never calls generic runtime dispatch or post-call async draining for this control. It preserves every preceding accepted replay mutation, its head, queued storage failures and canonical replay bytes. The terminal prefix is committed in the separate control history and is not rolled back on later refused calls.

The successful pinned body is unconditional. It works with no configured Main instance and does not depend on a V12 policy; tests cover all twelve existing policies and original JavaScript invocation with a hostile receiver, ignored values and an unbound receiver. Terminal arguments are retained using the existing bounded diagnostic transport. Invalid/unencodable input is refused before any prefix; ordinary replay capacity does not block shutdown.

After exit, every `apply` returns `AlreadyExited` without mutation, including another shutdown. Only immutable access to the owned replay is exposed. This is a session lifecycle contract: it does not terminate unrelated sessions or prevent an external caller deliberately constructing a different replay.

The control head has its own version/domain and commits the adopted replay head, current replay head, source pin and terminal input/outcome/effects. Its count is controls since adoption, including the terminal suffix. The underlying replay count excludes shutdown. Ordinary controls remain stored only in the actual replay journal; the wrapper does not duplicate that journal.

The new `TLMS\x01` checkpoint envelope contains the actual canonical TLCR command checkpoint, adoption/current replay heads and optional terminal frame. It contains no financial snapshot. Admission requires an independently retained `SourceMainControlHeadV1`; parsing a head from the same input is not an independent expectation. Admission first binds the source replay head through that expectation, then uses the original `SourceContractReplay::from_checkpoint` to reexecute every ordinary command, verifies the adoption prefix, reexecutes the terminal control and demands canonical byte equality. CRC/hash pins detect inconsistency, not authentication.

Original proof `main_shutdown_vectors.cjs` executes the real child exit under both archived numeric libraries. The added `main_shutdown_control_vectors.cjs` evaluates the unchanged complete pinned Main class and invokes its real shutdown method in three child processes. It captures both console effects and performs real process exit, proving no ordinary return and no queued microtask continuation. The Rust tests compare these exact effects while checking populated replay state and pending storage-failure preservation.

This feature does not add paused/waiting service jobs, real timer handles, cancellation, startup exit1, generic process control, durable flush/publication or financial restoration. Those require separate source execution and lifecycle design. It makes no complete-daemon/full-parity claim.

Root Core integration now passes all eight official control tests. The phase also passes all-target Clippy with warnings denied and 165 focused Root executions (including 134 library tests). The standalone eleven-test count includes three reused hash/CRC tests; it is not eleven new control tests. Full-suite promotion is recorded separately.
