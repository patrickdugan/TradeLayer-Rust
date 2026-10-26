# Startup compatibility and owned local readiness

`src/bootstrap.rs` implements both functions in the pinned `init.js` and adds an
explicitly repaired, caller-owned startup coordinator. Concrete controlled RPC
transport and a bounded observation runner are now connected through
[RUNTIME.md](RUNTIME.md). Chain consensus, transaction authentication and
genesis issuance remain separate requirements.

The complete unmodified source is archived in `fixtures/legacy_init_source.txt`
at JS commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, LF SHA256
`55c7dd6da798a58dc57d34ad75be357f9811574db5e6f384620055f966447cc0`.

## Literal source functions

| Source function | Rust entry point | Actual behavior |
| --- | --- | --- |
| local `waitForClientChain` | `legacy_wait_for_client_chain` | Checks captured client truthiness only; never examines chain; ignores timeout. A missing client waits repeatedly forever. |
| exported `initialize` | `legacy_initialize` | Acquire client, perform that wait, delay 500 ms, initialize database with the client's chain, return the same client/database handles. Propagate failures. |

The literal APIs preserve source defects for compatibility tests. Their delay
and dependency futures belong to the caller. Dropping a pending Rust future
stops polling it; these functions do not install background tasks or own a
timer runtime. A custom delay error represents a synchronous `setTimeout`
failure caught by the source Promise executor. A delay that immediately
completes in a missing-client loop can spin indefinitely, just as an immediate
Promise timer adapter can monopolize the JS microtask queue. Production startup
must use the repaired coordinator below.

The source readiness helper's `startTime` and timeout parameter are unused.
Because `client` is a captured argument and never reassigned, a false value
cannot become ready later. Conversely, any object is ready even with undefined
or null `chain`. Those chain values are distinct `LegacyChainValue` variants;
no LTC fallback or identity verification is invented by the literal functions.
Typed clients exclude JS property getters and arbitrary coercion objects.

Direct dependency inspection also found startup behavior outside this file:
requiring `db.js` eagerly launches an unawaited database initialization before
the exported function is called. `client.js` publishes its singleton before
initialization finishes, and its `isInitializing` flag is never cleared.
Database readiness follows NeDB object construction rather than awaited
autoload completion. These dependency defects are not silently reproduced as
successful readiness in the coordinator. The init source oracle uses explicit
dependency adapters; it does not execute the actual RPC clients or NeDB module.

## Repaired coordinator

`BootstrapConfig::new` requires an explicit BTC/LTC/DOGE `TradeLayerMainnet`,
an existing absolute directory, and a category accepted by
`storage::Namespace::for_mainnet`. The configuration owns the namespace and
root. It does not read environment variables, infer a network from a node
subversion, silently select LTC, create a missing root, or map testnet/signet/
regtest onto a mainnet. Additional network profiles require separate work.

`BootstrapCoordinator::start` performs the following sequence:

1. Check cancellation/deadline and acquire a caller-owned client handle.
2. Obtain `ClaimedNodeIdentity`; require configured network, mainnet mode and
   the matching pinned genesis hash.
3. Open the caller's store adapter in the configured namespace.
4. Restore one coherent state owner using independently configured checkpoint
   anchor, expected tip and other policies supplied by the store adapter.
5. Check the restored network and genesis, then perform a final stop check and
   return `ReadyLocalRuntime` owning the client, store and restored state.

`BootstrapClientFactory`/`BootstrapClient` and
`BootstrapStoreFactory`/`BootstrapStore` are generic adapter interfaces.
`RestoredState<S>::state` should contain a complete health-gated durable owner,
such as `PersistentJointChain` or `DurableFundedIou`, rather than a cloned raw
economic state detached from its journal. A pending store adapter can move its
journal into that state owner during restore. The coordinator neither clones
nor serializes state and retains no competing balance cache.

The generic restore contract is a real integration boundary: the store adapter
must validate complete checkpoint invariants, independently expected anchor/tip,
namespace, policy and recovery rules. Matching `RestoredState` metadata alone
cannot establish internal coherence. Likewise, `ClaimedNodeIdentity` is a node
claim. Matching a pinned genesis checks consistency and prevents accidental
network mixing; it does not prove that the node is honest, synchronized, or
that transactions/state are authenticated. No trusted-chain capability is
constructed or returned by this module.

## Ownership, failure and cancellation

Safe Rust requires an exclusive mutable coordinator reference for startup.
No partially constructed runtime is published. Handles remain local until all
steps succeed; error propagation drops every returned partial handle. Adapter
factories are responsible for releasing acquisitions they fail before returning.
Returned handles must implement appropriate `Drop` resource cleanup. Real
`Journal` handles retain their existing exclusive OS locks; those locks, rather
than a process-global bootstrap singleton, provide cross-owner storage exclusion.

An RAII attempt guard marks startup failed even when an adapter unwinds.
Ordinary unwind/error cleanup releases partial state, store and client handles.
Successful runtime destruction drops state first, then the store and client.
Panic abort, process termination and destructors that themselves panic do not
provide normal unwind cleanup guarantees.

Coordinator status is `Idle`, `Starting(stage)`, `Failed(stage)` or `Ready`.
A successful coordinator cannot start again. Failure requires explicit
`reset_after_failure` before retrying, including failure caused by cancellation.
Resetting the coordinator does not undo disk recovery or establish that an
uncertain failed write was absent. The durable owner/store's recovery and replay
policies remain mandatory.

`BootstrapControl` shares a cancellation flag across clones and optionally
holds a monotonic `Instant` deadline. The coordinator checks at phase boundaries
and after adapter calls. If an adapter returns an error while control is
stopped, the result reports that stop reason and phase. Blocking adapters must
cooperate by checking the supplied control inside their own I/O/retry loops;
this synchronous generic coordinator cannot interrupt a blocked system call.
It is not a scheduler or forced preemptive timeout mechanism.

The final stop check is the readiness publication linearization point. A
cancellation arriving afterward does not revoke an already returned runtime.
Shutdown of a running service remains the service owner's responsibility.

## Evidence and open integration

`node scripts/bootstrap_vectors.cjs` executes the complete unchanged init source
in a VM against deterministic client/database adapters and virtual timers.
`--source PATH` verifies an external exact source copy; `--write` deliberately
regenerates the TSV. Both modes verify the archived source hash.

The 12 source cases cover six chain values (including undefined/null),
client/database/timer rejection, missing-client pending after 5,850 ms despite a
5,000 ms timeout, present-client readiness without chain, and a custom helper
timeout still pending at 35 ms despite a 20 ms timeout. Handle identity, call
order and exact virtual elapsed time are checked. Pending timers are virtual;
the runner creates no real delayed/background startup work.

Rust tests consume all 12 vectors through the literal async functions using a
safe manual poller. Repaired-coordinator tests cover successful ownership,
all three mainnets, every adapter failure and cancellation stage, expired and
in-acquisition deadlines, error/stop precedence, identity mismatch, unwind
cleanup, explicit retry, and one-time publication. A concrete disk test moves
an actual `Journal` into its restored owner, checks retained lock exclusion,
then demonstrates invalid-restore cleanup and successful reopening. Its small
test checkpoint is an ownership fixture, not economic-state acceptance proof.

Real Core deployment/interoperability, broader source environment semantics,
testnet profiles, historical checkpoint authentication and daemon subscriptions
remain open. The concrete polling runner supplies bounded cooperative shutdown.
The readiness coordinator is functional local orchestration; these independent
requirements are not inferred from its successful tests.

## Concrete durable-owner restoration

`src/bootstrap_storage.rs` supplies restoration-only implementations of the
store interfaces for three complete durable owners:

* `FundedIouStoreFactory` uses an independent `FundedIouRestoreConfig` containing
  expected mainnet, payout atom policy, a **mandatory exact `DurableIouHead`**
  and recovery policy. It calls `DurableFundedIou::open_existing`, including
  complete funded ledger/IOU/replay snapshot validation.
* `JointStoreFactory` uses `JointRestoreConfig`, containing the independent
  `PersistenceConfig`, exact expected tip and recovery policy. It calls
  `PersistentJointChain::load`, including configuration, complete chain snapshot
  and retained-header validation.
* `EconomicStoreFactory` uses `EconomicRestoreConfig` with the complete economic
  chain configuration, mandatory exact `EconomicHead` and recovery policy. It
  restores `DurableEconomicChain` in the `clearing` namespace, preserving the
  sole funded Ledger, complete economic replay, journal lock and poison checks.

These factories validate the bootstrap category/namespace and network before
any opening. Their `open` step prepares a root-bound, one-shot restore adapter;
actual file opening and full restoration happen in `restore`. A subsequent
restore request cannot switch root or namespace, and an attempted restore cannot
be repeated on the same prepared adapter. Pre-cancelled requests that never
attempted I/O do not consume it. Factories do not create missing state, files or
directories and never fall back to genesis or an older checkpoint.

`restore` checks cooperative control before and after the owner's load, derives
network/genesis from its health-checked validated state, then moves the entire
owner into `RestoredState`. No ledger or chain-state clone is published.
The restored owner keeps journal locks, replay protection and poison checks after
startup; normal mutations continue through that owner's existing synced APIs.
The prepared store retains only configuration and its `RecoveryReport`, available
through `recovery_report()` after successful restoration. It is not another
economic state authority.

Recovery policy is explicit. `RejectIncompleteTail` leaves torn tails unchanged;
`DiscardIncompleteTail` permits the underlying journal's documented truncation
and returns discarded-byte evidence. Cancellation or a later identity check
cannot undo recovery that already changed disk. Adapters never treat a failed
or cancelled load as proof that disk was unchanged. Selecting or authenticating
expected heads/tips remains the operator's independent responsibility.

End-to-end adapter tests restore actual owners through `BootstrapCoordinator`,
perform a funded IOU claim/loss/payout batch, restart and reject replay. They
also mutate joint activation/send state, restart, reject replay, roll back and
persist an alternate branch. Additional cases cover pre-open scope/root
rejection, independently mismatched head/tip/policy, missing state, malformed
complete checkpoints, one-shot semantics, cancellation after both actual loads,
lock cleanup/reopening, and explicit torn-tail policy/report propagation.
The original injected client supplies node identity only. `tests/runtime.rs`
also uses the actual controlled HTTP client against local mock servers for all
three mainnet identities and complete economic-owner restoration. These tests
establish local durable orchestration, not authenticated historical acceptance.
