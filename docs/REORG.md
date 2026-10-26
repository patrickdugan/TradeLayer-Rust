# RPC fork observations

`src/reorg.rs` ports all three active members of pinned `src/reOrg.js`: constructor, `getBlock`, and `checkReOrg`. The complete unmodified source is archived in `fixtures/legacy_reorg_source.txt` at JS commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, LF SHA256 `3e4cabb0dc560fd8c601b72e05f0e698ce02adc646eaef241189fd192d906a82`.

This source file is an observer. It never compares a block with a local database, selects the active chain, disconnects a block, replays transactions, changes a balance, or writes a checkpoint. Its comments leave those operations unimplemented. Rust preserves this boundary rather than treating a `valid-fork` or `headers-only` status as reorganization authority.

## Methods and source ordering

| Source | Rust | Behavior |
| --- | --- | --- |
| `constructor(config)` | `ReorgChecker::new(rpc)` | The source constructs `litecoin.Client` with its argument. Rust receives an explicitly configured caller-owned `ReorgRpc`. Neither constructor performs a tip/block RPC. The source file's separate top-level `config` constant is unused by the class. |
| `getBlock(hash)` | `get_block(hash)` | Fetch exactly the supplied hash string. On RPC failure emit `Error fetching block:` and propagate that error through `BlockFetchReport::result`. A successful direct fetch emits no log. |
| `checkReOrg()` | `check_reorg()` | Call `getchaintips` once, select exact statuses `valid-fork` and `headers-only`, then fetch them sequentially in original RPC order. Each successful fetch emits the original potential-reorg message. First error stops traversal; it is logged and swallowed into the report. No automatic retry occurs. |

The source finishes filtering the complete tip list before fetching any block. Tip heights, relative chainwork, status order, duplicate hashes, and a returned block hash that differs from the requested hash do not change its selection. Unknown, uppercase, or whitespace-suffixed statuses are ignored. Duplicate selected tips produce duplicate calls and observations. There is no hidden sorting, deduplication, database lookup, or next-tip fallback after an error.

A block-fetch error is logged by both methods: first the `getBlock` message, then the outer `checkReOrg` message. Prior successful observations/logs remain visible, and later selected tips are not fetched. Failure of `getchaintips` emits only the outer error message. `headers-only` does not imply a retrievable block body, so a missing body can terminate this scan.

The original `checkReOrg` returns JavaScript `undefined` on both success and caught failure. `ReorgReport` is an explicit Rust inspection adapter: it records the successful prefix, source logs, optional failed tip, complete selected count, and swallowed failure without throwing it as a state-changing command. `completed()` reports whether traversal finished; it does not indicate chain validity. Direct `get_block` retains its propagated result separately from its logs. Error messages are retained; JavaScript-specific Error stacks/prototypes are outside this typed interface.

## Transport and typed boundaries

`ReorgRpc` supplies `get_chain_tips` and `get_block`. The caller owns actual HTTP/JSON-RPC transport, credentials/configuration, decoding, response-byte/time limits, execution scheduling and client-construction failures. The implementation makes no network request by itself and imports no external crate. Synchronous trait calls preserve source sequential ordering; concurrent asynchronous JavaScript interleavings are not modeled as consensus behavior.

`ChainTip`, `ReportedBlock`, and `RpcFailure` have private fields and checked constructors. Hash fields are opaque untrusted UTF-8 strings: empty strings and non-hex strings are deliberately accepted, because source does not validate them. Reported heights are nonnegative JS-safe integers. Missing fields, non-array tip responses, non-string hashes/statuses, non-integer/negative heights, custom JavaScript coercions and arbitrary thrown objects/primitives are excluded. A transport adapter must handle malformed RPC data explicitly; the source's property-access/coercion errors for that excluded domain are not claimed as equivalent RPC errors.

Local resource bounds are 1,024 total tips per call, 256 UTF-8 bytes per reported/requested hash, 64 per status, 4,096 per error message, and maximum height 2^53−1. A checked scan holds at most 1,024 observations and 1,025 source logs; it retains no growing cross-call journal. An oversized returned tip list is reported before any block fetch. An oversized direct hash is rejected before RPC and produces no source RPC-error log. These are explicit local bounds, not legacy transaction acceptance rules. RPC response allocation and a transport that never returns remain the transport owner's responsibility.

## Existing rollback and persistence integration

The existing `ChainState`, `UnifiedChainState`, `SpotChainState`, `JointChainState` and `ClearingChainState` wrappers already own bounded reversible states. Their expected-tip rollback guards and complete checkpoint witnesses address different work than this observer. The durable joint-chain owner can persist transitions through its own commit boundary. No observer method calls any of those mutation APIs or translates RPC tip status into authority to call them.

`ObservedMainnetForks` is likewise a separate bounded observed-header audit. Its greatest-observed-work report is not an authenticated live full-node chainstate and cannot promote this observer's metadata. Actual economic reorganization requires independently authenticated branch/block input, a complete economic state owner, retained ancestor witnesses, replay protection, reporting/index coordination, and durable publication of the resulting state. New standalone modules are not automatically captured by older checkpoint formats. The external txindex/UTXO/script-validation lane remains separate.

## Executed source evidence

```text
node scripts/reorg_vectors.cjs
```

The offline oracle SHA-checks and executes the full unmodified JavaScript in Node's VM. Its only dependency adapter is a deterministic `litecoin.Client` with recorded constructor configuration, `cmd('getchaintips')`, and `getBlock` calls. Actual source console calls and returned/thrown behavior generate the fixture; selection/ordering are not replaced by a rewritten oracle. `--source PATH` checks an external exact source copy, and `--write` regenerates the archive-derived TSV intentionally. No npm package or network is required.

There are **20 source scenarios** covering all three methods: no constructor RPC, direct success/failure, empty and ignored tip lists, exact status matching, valid-fork/header-only scanning, RPC order rather than height order, duplicate tips, first-error abort, retained success prefix, swallowed tip-query failure, empty error messages, mismatched returned hashes, Unicode/control metadata and maximum safe height. Rust consumes all cases, comparing exact RPC traces, source log messages, successful prefix and error outcome.

Seven Rust tests also exercise all constructor bounds, UTF-8 byte limits, excessive and maximum-sized tip lists, pre-RPC direct-input rejection, source candidate counts after a partial failure, and repeated independent checks without retry/deduplication. This evidence covers local observer behavior, not historical reorganization acceptance, active-chain discovery, economic rollback, or script/UTXO correctness.
