# Durable observation daemon

`daemon.rs` is a functional polling state machine with an actual synced raw-block spool. It owns one `ObservationRpc` and one `ObservationStore`; it owns no Ledger and cannot call an economic apply/rollback method. The source's runtime responsibilities are separated from transaction admission: polling, retries, pause/stop, observational fork detection, bounded storage, restart and explicit reconciliation work now. Authenticated economic execution remains unavailable through this API.

The configured node's family/mainnet/genesis checks establish identity consistency with the caller's independent configuration. RPC claims, matching header hashes, TXID Merkle roots, checksums and observation commitments are not proofs of PoW, chainwork, active-chain membership, scripts, UTXOs, sender authority, protocol activation or correct economic decoding.

## Construction and composition

`ObserverConfig::new(network, anchor)` takes an explicit mainnet and independent observation anchor. A height-zero anchor must match that network's pinned genesis. A higher anchor is a caller declaration; it is not authenticated by querying the same node. Defaults are a 10-second poll interval, one-second initial retry delay, 60-second maximum delay, three retries, 64 retained blocks and 16 MiB of raw material. The scanner configuration is bounded by the existing worker/window limits. Configuration is persisted and compared exactly on reopen, including scanner settings.

```text
ObservationStore::initialize(root, config)
ObservationStore::load(root, config, independently_expected_head, recovery)
Daemon::new(source, store)
Daemon::step(monotonic_milliseconds, control)
```

The root directory must preexist. Initialization requires an empty sequence-zero journal; it never replaces a populated observer store. Load never creates missing paths and requires the expected sequence, observed tip and complete-state commitment. Callers must obtain the comparison token independently of that load. A checksum or token is still not authentication against a malicious party able to replace both data and expectations.

The concrete RPC adapter, bootstrap restoration adapters, command-line runner and listener are separate composition layers. A service can hold the sole restored `DurableEconomicChain` alongside this observer without copying its Ledger. Observation and economic journals are separate transactions. This module makes no cross-namespace atomicity claim and supplies no route from an observed block to `apply_unverified_block` or a trusted adapter. An attached economic owner must retain its own health checks and independently expected configuration/head on restoration.

## RPC and incremental work

`ObservationRpc` has four methods: `verify_identity`, `reported_tip`, `block_hash` and `raw_block`. `reported_tip` returns a height/hash pair from one node response. Hashes use the existing `BlockId` display-byte convention; the daemon explicitly reverses header digest/previous-hash bytes at the raw boundary.

Identity verification may make up to three controlled RPC calls. Every subsequent step makes at most one RPC call or one bounded TX/Merkle inspection; a successful recheck also performs the bounded synced spool commit. There is no sleep, hidden background task, per-transaction RPC, unbounded retry loop or wallet call in the core. The caller supplies monotonic milliseconds; a backward clock or arithmetic overflow returns an error. The caller must stop on an error rather than silently treat it as successful polling.

The concrete client adapter must use generic `getblock(hash, 0)`. The source-compatible `SourceRpcMethod::GetBlock` intentionally keeps the original wrapper's single argument and would drop verbosity zero. Selected read failures retry through explicit exponential backoff; before retrying a read, the daemon rechecks node identity. A completed caught-up poll or committed block resets the retry counter. The identity phase itself shares the same bounded retry budget. There is no retry of any wallet mutation because no wallet mutation is exposed.

Transient failures clear the identity-ready flag. Exhaustion, unavailable historical data, invalid responses and wrong identity enter distinct durable quarantine modes. Cookie refresh and socket cancellation/deadlines belong to the concrete transport. `BootstrapControl` is checked before each core step and after every RPC result, including error results mapped from controlled cancellation/timeouts and the final canonical-hash recheck before raw/cursor publication; the adapter must also check it within I/O. Cancellation observed inside a call publishes a stopped mode with the old cursor and no new raw record, retry or fork-probe failure receipt. Blocking call latency otherwise remains bounded by the configured RPC timeout, not by an implied ability of this state machine to interrupt an arbitrary synchronous implementation.

## Poll, scan and publication

The sequence is identity → reported tip → recheck the current observation cursor → fetch the next height's hash → fetch one raw block → scan → recheck that height's hash → synced publication. A changed hash during fetching quarantines the observation before publication. Empty blocks are still observed as complete raw blocks; the daemon does not manufacture empty economic inputs or run clearing.

For Bitcoin and Litecoin, the existing Bitcoin-style raw parser checks the 80-byte header hash, declared parent, transaction boundaries, resource/weight limits, TXIDs and TXID Merkle root. Its bounded parallel scanner restores transaction order. No funding/trade reordering is performed. The daemon retains the raw block and a compact transaction/candidate-count summary, rather than a second economic state or a parallel index database.

Dogecoin ingestion is deliberately opaque. After bounded header-hash/parent checks and canonical-hash recheck, the raw material is persisted with no structural summary, and the daemon quarantines without advancing the canonical observation cursor. The existing parser does not handle Dogecoin's chain-specific extended AuxPoW block layout. This behavior is not Dogecoin transaction, AuxPoW or consensus validation, even when a particular supplied block happens to resemble the Bitcoin-style fixture format.

The observer uses the existing `consensus` Journal namespace and private key `observer/full-v1`. It does not write `txIndex`, `clearing`, `persistence`, or any protocol state. Each complete state contains the exact independent configuration, raw block graph, canonical-selection indices, mode and bounded fork evidence. One journal frame publishes them together after file sync. The previous in-memory state is replaced only after that commit succeeds.

All state-bearing accessors check Journal health. An uncertain write poisons the journal and denies cached head/config/raw/header/mode/fork reads. A fully committed write with a lost acknowledgement requires reconciliation with the new independently expected head. A torn tail fails closed by default; only explicit `DiscardIncompleteTail` recovery may discard it, preserving the old committed cursor. File paths remain inspectable metadata after poisoning.

## Forks, pause, stop and reconciliation

A changed cursor hash or reported retreat durably quarantines the observer. It then probes one retained canonical height per step, descending to the declared anchor. Evidence distinguishes a matching candidate ancestor, exhausted retained history, and an RPC probe failure. A side-fork tip report by itself is not a command to roll back: this daemon compares its own observed canonical cursor and never treats `ReorgChecker` logs as economic authority.

Fork evidence remains observational. No node-reported ancestor automatically changes either an economic head or the selected observation branch. `reconcile(expected_head, target)` requires an explicit comparison token and a target already on the retained selected path. It truncates only the canonical-selection indices; every old branch's raw record remains stored. New branch observations append to the same bounded material graph. Reconciliation also works after stopping an owner that holds unresolved fork evidence. Ordinary resume cannot bypass that reconciliation requirement.

`pause`, `resume` and `stop` likewise compare an expected observation head. Pause/stop preserve the spool; resume rechecks identity. Quarantine for opaque Dogecoin material or a full spool cannot be silently resumed as if the underlying limitation disappeared. A cancelled/deadline-expired control publishes a stopped mode; repeatedly stepping an already stopped owner does not create additional stop frames. No further RPC occurs while paused, stopped, or terminally quarantined. Restart begins with an identity check for observing owners; persisted quarantine remains stopped for explicit inspection/reconciliation.

The library does not install OS signal handlers, terminate the process, bind an HTTP port or run a thread. Those are runner responsibilities. A portable graceful Ctrl-C guarantee is not supplied by this dependency-free state machine. Explicit stop control, bounded transport I/O, journal sync and normal owner drop provide the usable shutdown boundary.

## Bounds and checkpoint validation

At most 64 raw records, including archived branches, and 16 MiB of raw bytes are retained. Individual blocks obey the existing 4,000,000-byte scanner bound. Smaller independently configured limits are supported. Capacity quarantines ingestion without skipping a block, evicting material or advancing the cursor. A full spool requires an explicit externally reviewed archival/new-store process; no implicit compaction or loss policy exists here. The existing Journal also has its independent 512 MiB log limit, so repeated full-state commits are a bounded prototype storage design, not an indefinite retention or throughput claim.

The canonical `TLOB` version-1 envelope uses fixed-width bounded fields and a CRC. Decoder counts and aggregate raw byte lengths are checked before nested allocation. Restore checks network/configuration, unique raw IDs, parent/height graph linkage, selected-path continuity, summary bounds, bounded fork evidence and exact reencoding. It recomputes raw header/hash/parent and supported TXID Merkle/summary results over all retained records. It never trusts an opaque imported economic state or a caller-set replay bypass flag. Reopening may inspect the whole bounded spool; the one-step work rule concerns active polling.

Tests cover actual Journal writes/reopen on Bitcoin and Litecoin, exact retained raw bytes, fork/retreat and archived-branch reconciliation, failed ancestor probes, retries and identity recheck, independent comparison tokens, pause/stop/cancellation, opaque Dogecoin material, invalid hash/parent/Merkle data, changed hash during fetching, capacity and missing/locked stores. Private tests repair envelope CRCs while forging raw or summaries, forge count/length fields, and use the real lost-ack/partial-write fault hooks to check poisoning and recovery.

## Complete pinned main.js characterization

`fixtures/legacy_main_source.txt` preserves all 975 pinned source lines from `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, LF SHA256 `006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009`. `scripts/daemon_vectors.cjs` executes that complete unchanged source with explicit recording client/DB/index/protocol dependencies, fixed scheduling and inert process exit. No source method body or numeric expression is rewritten. Caller-level method boundaries are isolated only where necessary to terminate the source's unbounded polling.

The 21 committed observations execute all 30 active methods plus the constructor. They characterize the import-time singleton/client race, undefined-DB dereference, ignored restoration path, source genesis selection, catch-up type4/20 prioritization, live pre-apply checkpoint, partially applied logic marked processed, undefined realtime `activationInstance`, independent height markers, fire-and-forget polling, network/recovery behavior, four attempts for `retries=3`, and logging-only shutdown. These are source observations and named repairs, not a successful execution claim for its database/wallet dependencies.

| Active source methods | Runtime treatment |
| --- | --- |
| constructor, `getInstance`, `initialize` | Explicit owned bootstrap and store restoration; no published partial singleton or import-time service startup |
| `delay`, `fetchWithRetry`, `checkNetworkStatus`, `enterRecoveryMode` | Monotonic caller-driven scheduling, selected-read retry budget, explicit identity/error states |
| `getCurrentBlockHeight`, `getGenesisBlock`, `getLastBlock`, `checkBlockLag`, `checkSync` | Claimed node tip and independently configured anchor, separately exposed from durable observation progress |
| `initOrLoadTxIndex`, `syncIndex`, `constructOrLoadConsensus`, `constructConsensusFromIndex` | Bounded raw spool/restart only; no full-index read, unauthenticated consensus reconstruction, or txIndex mutation |
| `syncIfNecessary`, `processIncomingBlocks`, `setPause`, `shutdown` | One incremental owner, explicit scheduling/pause/stop, synced publication and normal drop |
| `processBlock`, `blockHandlerBegin`, `blockHandlerMid`, `blockHandlerEnd` | Ordered fetch/scan/recheck/commit; transaction execution and economic finalization remain behind the existing absent authenticated-admission boundary |
| `processTxSet`, `processTx` | Source behavior characterized; no diagnostic decoder output promoted into accepted operations |
| `handleReorg` | Durable evidence and explicit observation reconciliation; no automatic economic rollback |
| `saveMaxProcessedHeight`, `saveTrackHeight`, `loadMaxProcessedHeight`, `loadTrackHeight` | One atomically published observation cursor, explicitly distinct from an economic processed height |

```text
node scripts/daemon_vectors.cjs --check
node scripts/daemon_vectors.cjs --check --source /path/to/pinned/src/main.js
```

Complete source method coverage does not complete authenticated daemon deployment. Full header/UTXO/script/transaction admission, accepted protocol decoding, coordinated economic commits, real-node interoperability evidence, production retention/compaction and listener query coverage retain their separate gates.
