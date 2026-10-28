# Main transfer Tally continuations

Original Logic.transfer starts two Tally.updateChannelBalance calls without
awaiting them. Awaiting the channel-registry save does not finish those calls.
The original Main caller can mark the transaction processed before the pending
Tally delta and save operations complete. Eagerly draining every update at the
Logic return changes the actual database order, despite matching final balances.

Exact SpotLifecycleV9 Main reuses the existing channel update jobs, mutations,
captured references, delta helpers and source error handling. It reaches the
original Logic return boundary and resumes the remaining jobs at Main's actual
awaited consensus and transaction-index operations. Public Main return drains
remaining jobs. Standalone calls and V1–V8 retain their existing behavior.
The continuation takes no expected balance or source poststate as an input.

The separate original-source corpus contains 102 frames: 34 input resets and
68 Main calls. Its first 30 frames remain byte-identical to the earlier channel
read corpus. Added cases vary unrelated channel records, use indexed and live
Main, reject Tally factories and reads, reject delta reads and inserts, reject
Tally saves, reject later channel writes, and replay processed transactions.
All source business modules remain unchanged. Only constructor state and
one-shot database failure inputs vary.

Both genuine BigNumber constructors regenerate SHA256
`d56e6574921a7917e641af4032b6a67c7d72cbeeda7de1ae20541051efb98f1b`.
The Rust target compares complete original outcomes, actual owners and caches,
financial prefixes, ordered effects and exact checkpoints. Root's final
validation record supplies executed Rust results; generation alone establishes
no Rust parity, authentication or complete block proof.
