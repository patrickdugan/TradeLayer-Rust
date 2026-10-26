# Bounded Core observation runner

`runtime.rs` connects the concrete Core HTTP client to owned bootstrap and the
durable observation state machine described in [DAEMON.md](DAEMON.md).
`tl_observe` is a runnable observation service. It records raw blocks from a
configured node and reports its independently scoped observation head.

The runner requires an existing absolute root, an explicit mainnet, a literal
IP/socket address, and either deliberate initialization or a complete expected
head token. Authentication uses an explicit cookie file or `RPC_USER`/`RPC_PASS`
environment values. Passwords are not command-line arguments. Source default
credentials still apply if no environment values are supplied. RPCs issued by
the observer are read-only; the client library's separate wallet wrappers are
not invoked by this runner.

For example, with cookie authentication configured on the operator's machine:

```text
tl_observe --root C:\TL\observations --network BTC --rpc-address 127.0.0.1:8332 --cookie-file C:\Bitcoin\.cookie --initialize --steps 1
```

Each finite step performs one bounded state-machine phase. Identity checking
uses three controlled RPC calls; later RPC phases use at most one. `--steps 1`
is therefore a startup observation, not one complete block. Finite mode does
not wait through polling/retry intervals. Continuous operation uses `--run
--stop-file ABSOLUTE_PATH`; creating that file requests cooperative shutdown.
The runner checks it between phases and sleeps at most 100ms between checks.
There is no portable Ctrl-C signal-handler guarantee. A blocked socket may take
up to the configured per-call timeout to return; cancellation checks do not
interrupt an OS read in progress.

The final JSON includes `head` in `SEQUENCE:HEIGHT:HASH:COMMITMENT` form, mode,
phase, retained counts, identity status and `authenticated_admission:false`.
Restart supplies that complete independently retained token via
`--expected-head`. The original anchor and scanner settings must also match.
There is no implicit latest-head discovery, checkpoint replacement, genesis
fallback or automatic torn-tail repair. Library callers can explicitly select
the journal's documented recovery policy; the CLI rejects incomplete tails.

`--pause`, `--resume`, `--stop` and `--reconcile HEIGHT:HASH` compare the supplied
head before changing durable state. Controls perform no RPC. Malformed option
values reject before opening a store. Semantic reconciliation still requires a
retained target and the daemon's permitted fork state. A quarantine causes the
CLI to exit; a just-discovered fork may still have `ForkSearch` phase with no
candidate ancestor. Library callers may continue bounded fork-search steps for
observational evidence. No node report grants economic rollback authority.

Retention is capped at 64 blocks including old branches, 16MiB raw material and
the independent journal byte limit. Capacity stops ingestion without eviction.
This full-state journal design requires a separately reviewed archival or
compaction policy for indefinite operation. It makes no production throughput
claim. Bitcoin/Litecoin structural and TXID-Merkle checks remain observations;
Dogecoin extended AuxPoW layout stays opaque and quarantined.

## Complete economic-owner bootstrap

`RpcClientFactory` moves one configured client into `BootstrapCoordinator`;
identity readiness checks family, mainnet and genesis through actual controlled
HTTP calls. `EconomicStoreFactory` restores the complete `DurableEconomicChain`
from independently supplied configuration, expected full head and recovery
policy. Scope, cancellation, missing stores and failed restoration are checked
without creating an economic replacement. The returned owner retains its
journal OS lock and poison checks.

`ObservedEconomicRuntime` can own that restored economic journal together with
the observer. It exposes immutable economic queries and observation steps only.
It captures the complete economic head once, checks journal health and tip on
every access, and avoids rehashing an unchanged large economic checkpoint each
poll. There is no mutable economic reference or raw-RPC-to-economic apply path.
`into_parts` consumes the wrapper and returns the original two owners.

The CLI itself opens only the observation journal. Authenticated parent UTXOs,
script verification, protocol admission, accepted block execution and a trusted
reorg policy remain required before raw observations can advance economic state.
Tests use real local TCP mock servers and actual journals/processes; they cover
all-mainnet bootstrap, fixed economic state during raw block-1 observation,
in-flight cancellation, exact CLI restart tokens, control/shutdown durability,
stale heads and pre-I/O argument rejection. Real installed Core interoperability
and production deployment remain unverified.
