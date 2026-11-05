# tradelayer-node

`tradelayer-node` is one process that follows a Bitcoin Core node, authenticates the
TradeLayer candidates of each new block, applies them to the durable economic chain,
rolls the chain back when the observer reports a fork, and serves the legacy listener
HTTP API on a loopback address between blocks. It is "admission slice 2": slice 1
(`ADMISSION.md`) authenticates candidates; this slice applies them.

The pieces, each in its own module:

| Module | Role |
| --- | --- |
| `admission`, `admission_rpc` | Slice 1: sender proof and inclusion proof per candidate; parent outputs from `getblock <hash> 3`. |
| `daemon`, `runtime` | The observer: bounded raw-block spool, fork evidence, reconcile. It never touches the economy. |
| `node_ingest` | The only path from an observed block to the economic chain: candidates to `UnverifiedSourceBlock`, apply, rollback, the height map. |
| `node_runner` | The loop: observer step, catch-up, fork resolution, observer rotation, head logs. |
| `node_service` | The loopback listener API over the node's economic chain, with node status. |
| `node_config` | The strict source-consensus config section and the bootstrap. |
| `bin/tradelayer-node` | The CLI. |

## What `authenticated_admission: true` means

A block is admitted when `admission::admit_block` accepted its bytes and returned an
outcome for every TradeLayer candidate. Only an `Authenticated` outcome reaches the source
runner, and the scope of that authentication is exactly this:

- **Input 0 only.** The sender is the address of the parent output that input 0 spends
  (the legacy `getSender` rule). Other inputs are not looked up and their witnesses are
  not verified.
- **P2PKH or P2WPKH parent scripts, SIGHASH_ALL only.** The key behind the parent script
  must have signed the spending transaction, and its address must equal the parent script's
  address. P2SH, wrapped segwit, multisig, P2WSH, taproot, SIGHASH_NONE, SIGHASH_SINGLE,
  ANYONECANPAY and coinbase inputs are refused with a typed reason.
- **The parent output is a claim.** It comes from the node's `getblock <hash> 3` and cannot
  forge a sender, because a wrong claim fails verification. Parent existence, inclusion and
  unspentness are not proven. For P2PKH the amount is not covered by the signature, so
  `sender_amount` is an unauthenticated claim there; for P2WPKH it is signed.
- **Inclusion.** The candidate's txid is proven against the header's Merkle root, and the
  witness bytes are bound to the block through the BIP141 commitment.
- **Block bytes.** Header size, proof of work (`MainnetLike` target limit), claimed hash,
  claimed parent, transaction boundaries, Merkle root and witness commitment are checked.
  Complete Bitcoin block and transaction consensus is not; the active chain is the Core
  node's word, observed through the daemon.
- **Carrier format.** The legacy ASCII `tl` carrier only. A `TLR` binary carrier, and a
  legacy payload that is not ASCII, are counted as `unrepresentable` and not submitted: the
  source runner has no input form for them.

`authenticated_admission` is `true` in the status of a process only after that process
applied a block through admission, and it describes that tip. A store restored from disk
makes no admission claim until the first block this process admits. Every earlier block of
a restored store is exactly as trustworthy as the process that wrote it.

## What the source runner receives

For each authenticated candidate, `node_ingest` builds one `SourceTransaction`; all fields
follow `CONSENSUS_SOURCE.md`:

| Field | Value |
| --- | --- |
| `position` | The transaction's index in the block, coinbase at 0. Ascending, as the runner requires. |
| `txid` | The display-order txid bytes. |
| `payload` | The carrier text after the `tl` marker, type character first (`20;addr;7;a` for a send). |
| `marker` | `"tl"`. |
| `sender` | The authenticated address on the observed network: `bc1q...`/`1...` on mainnet, `tb1q...`/`m...` or `n...` on testnet4. |
| `sender_amount` | The claimed parent output value in coin units (satoshis / 1e8). |
| `reference` | An array with one `{vout, address, satoshis}` object per P2PKH or P2WPKH output other than the carrier, in output order. Other templates are absent. |
| `reference_amount` | The first reference output's value in coin units, 0 when there is none. |

The block header handed to the runner is the economic header (next section) and the block
mode is the node's configured `block_mode`. Candidates are submitted in block order; the
runner applies its own funding-first partition. A refused candidate is omitted: the runner
records the candidates it is given, and a candidate with no proven sender has no sender to
give it. Refusals are counted and listed (first 32 per block) in the block event.

An authenticated candidate that the source rules reject is recorded by the runner as a
rejected candidate with its receipt and changes no balance; that is the runner's own
behavior and appears in `rejected` of the block event.

If the parent-output source fails (RPC error, response above the 16 MiB RPC body limit),
the block is not applied and the node exits with code 7. Treating the outage as a refusal
would make the state depend on it. The source is asked only when the block holds a
candidate.

## Heights and the economic chain

The economic chain starts at height 0 on the network genesis tip and extends one block at a
time; its parent id and height are checked on every apply. `NodeBinding` maps it onto the
observed chain: **economic height 0 is the observed base tip** (`--anchor-height`,
`--anchor-hash`, default the network genesis), and economic height `e` is observed height
`base.height + e`. The first economic block's parent is the economic genesis id, which
stands for the base tip. With the default base the map is the identity.

Invariant, checked before every apply and after every restart: the economic tip, seen
through the map, is the observer's anchor or a block of the observer's selected chain, and
the block applied is that tip's child on that chain. Anything else is a refusal with
exit code 7 and nothing is applied: a different chain, a gap, a replayed block, a base that
does not match the store. The one tolerated state is an economic tip above the observer's
tip, which exists only while a rotated observer re-fetches history below the tip; nothing is
applied until the observer passes the economic tip, and then the check applies.

A nonzero base shifts the heights the source rules see by `base.height`. That is correct for
a network that starts at its base (the testnet4 smoke run) and wrong for rules that name
absolute Bitcoin heights. Launch on mainnet from height 0 (a full replay of the observed
chain) or add an authenticated historical bootstrap; this slice does not do either.

## Reorgs

The observer detects a fork (`ForkDetected` or `CursorAboveTip`), searches for the common
ancestor one `getblockhash` at a time, and records `ForkEvidence`. The runner then:

1. finishes the ancestor search (bounded by the observer's stored blocks);
2. rolls the economic chain back to the ancestor, mapped to economic height, with
   `DurableEconomicChain::rollback_to` (one synced commit; bounded by `rollback_limit` of the
   economic config);
3. rewinds the observer's selection with `Daemon::reconcile` to the same ancestor;
4. lets the observer fetch the winning branch, which catch-up applies block by block.

A fork with no recorded ancestor (`history_exhausted`, `probe_failed`) or deeper than the
economic rollback window stops the node with code 7 or 8. A hash recheck failure with no
search evidence (`ForkDetected` from the scan step) rewinds to the observer's current tip.
The result equals applying the winning branch from scratch, including the retained rollback
history; `tests/node_ingest.rs` compares the two checkpoints byte for byte.

The observer's spool holds 64 records and never prunes. When it reaches 48 records and the
chains are level, the node starts a fresh observer store, anchored 8 blocks below the tip so
a shallow fork right after the switch still finds its ancestor, in a new directory
`observer-<height>-<hash>` under `--root`. A restart loads the newest generation that its
head log still names.

## Bootstrap and config

Two files; the economic half is the `tradelayer-listenerd` config
(`LISTENERD.md`, `listener_config`), unchanged and without `source_consensus`. The
source-consensus half is `node_config`:

```text
format = tradelayer-node-source-v1
policy = conserving-sequential-v2
block_mode = realtime
bootstrap = collateral-seed
collateral_seed_atoms = 100000000
bootstrap_commitment = <64 hex>
catalog_bootstrap_commitment = <64 hex>
```

One pair per line, ASCII, at most 4096 bytes, every key required once, unknown, duplicate,
empty and malformed values refused, no defaults. `block_mode` (`realtime` or
`indexed-replay`) fixes the funding-first order inside a block, so it must be the same on
every node that has to agree. The economic state refuses a Ledger without its collateral
property and a property needs a nonzero supply, so the one bootstrap the node can start from
is the config's collateral property, created by the administrator with
`collateral_seed_atoms` atoms held by the administrator, and empty property and clearlist
catalogs. `tradelayer-node --print-bootstrap-commitments --config F --collateral-seed-atoms N`
prints the two commitments to pin. `--initialize` refuses a source config whose pins differ
from the bootstrap's.

## Usage

```text
tradelayer-node --root ABS_DIR --network BTC|BTC-TESTNET4 --rpc-address IP:PORT
    [--cookie-file PATH] --config ABS_FILE --source-config ABS_FILE
    --bind 127.0.0.1:PORT --stop-file ABS_PATH --head-log ABS_PATH
    (--initialize
     | [--economic-head-log ABS_PATH | --economic-expected-head SEQ:HEIGHT:HASH:COMMITMENT]
       [--resume])
    [--anchor-height H --anchor-hash HASH] [--max-blocks N]
    [--rpc-timeout-ms MS] [--timeout-ms MS]
tradelayer-node --print-bootstrap-commitments --config ABS_FILE --collateral-seed-atoms N
```

| Option | Meaning |
| --- | --- |
| `--root` | Existing directory. Holds `economic/`, the `observer-*` generations and `node-base`. |
| `--network` | `BTC` (mainnet addresses) or `BTC-TESTNET4` (testnet addresses; the economy is still the Bitcoin economy). |
| `--rpc-address`, `--cookie-file` | Core RPC, literal `IP:PORT`; cookie auth, or `RPC_USER` and `RPC_PASS` from the environment. The node's identity is verified at startup. |
| `--config`, `--source-config` | The two config files above. The config's `network` must be `bitcoin`. |
| `--bind` | Literal loopback `IP:PORT` (port 0 picks one). Non-loopback is refused before any store opens. |
| `--stop-file` | The node exits 0 when this file appears; one that exists at start is refused. |
| `--head-log` | The observer's append-only head record (`prepared` and `committed` lines, synced), as `tl_observe --head-log`. |
| `--economic-head-log` | The economic head record, `committed` lines; default `<head-log>.economic`. |
| `--economic-expected-head` | Overrides the economic log: the head to restore. |
| `--initialize` | Creates the economic store from the bootstrap and the first observer store; refuses an existing store. |
| `--anchor-height`, `--anchor-hash` | The base tip; default the network genesis. Recorded in `<root>/node-base` by `--initialize`; a restart reads the record and refuses different values. |
| `--resume` | Clears a paused or quarantined observer (other than a fork, which the node resolves) at start. A store the node stopped itself resumes without it. |
| `--max-blocks` | Exit 0 after this many blocks admitted in this run. |

Standard output is JSON lines: `listening` at start; `block` per admitted block
(`observed`, `economic_height`, `candidates`, `authenticated`, `refused`, `unrepresentable`,
`submitted`, `accepted`, `rejected`, `already_processed`, `refusals`, `economic_head`);
`reorganized`; `observer_rotated`; `stopped`. Errors go to standard error as
`tradelayer-node: MESSAGE`.

| Exit | Meaning |
| --- | --- |
| 0 | Stopped through the stop file or `--max-blocks`; `--help`. |
| 2 | Usage: unknown, duplicate or missing option, non-loopback bind, relative path, stale stop file. |
| 3 | Config: unreadable or invalid file, pins that differ from the bootstrap, a non-Bitcoin config. |
| 4 | Store: cannot initialize or restore, stale head, no observer generation loads. |
| 5 | Service: cannot bind, or serving failed. |
| 6 | Observer: quarantined, paused, or its store failed. |
| 7 | Refused: alignment mismatch, block bytes that fail admission, parent-output outage, unresolvable fork. |
| 8 | Economic apply or rollback failed. |
| 9 | A head log could not be written. |

## Status

`GET /initstatus` (the listener's `tl_initmain` status route) on the bind address returns the
listener's economic state plus the node's:

```json
{"initialized":true,"scope":"node","daemonStarted":true,
 "authenticated_admission":true,"admission_scope":"input0-p2pkh-p2wpkh-sighash-all",
 "admitted_blocks":20,"authenticated_candidates":0,"refused_candidates":0,
 "unrepresentable_candidates":0,"applied_candidates":0,"rolled_back_blocks":0,
 "height_base":"159028:<hash>","observed_tip":"159048:<hash>",
 "economicHead":"SEQ:HEIGHT:HASH:COMMITMENT","state":{...}}
```

`authenticated_admission` is `false` until this process applies an admitted block. The
other listener routes serve the economic chain as `EconomicHttpService` does. Requests are
answered between observer steps, one at a time; an observer RPC call in flight delays them
by at most `--rpc-timeout-ms`.

`node_service::NodeHttpService` replaces `EconomicHttpService` here because the listener
service reports `authenticated_admission:false` unconditionally and its owner can only be
moved out and re-bound; the node needs the status above and keeps the listening socket open
while it lends the durable chain to one block apply.

## Crash and restart

Each economic commit is one synced journal write. The economic head is appended to its log
after the commit, so a crash between the two leaves the store one commit ahead of the log;
restart then reports `StaleHead` (exit 4) and `--economic-expected-head` supplies the head.
The observer writes `prepared` before and `committed` after each commit, so it recovers
without help. Catch-up on restart applies every observer block the economic chain lacks.

## Verification

`tests/node_ingest.rs` builds mined regtest-difficulty blocks around the signed
transactions in `fixtures/node_vectors.tsv` (`scripts/generate_node_vectors.py` regenerates
them; `--check` compares bytes) and drives the real `Daemon`, `NodeRunner` and durable
stores over a mock node:

- an authenticated P2WPKH send and a P2PKH send move balances as the source runner defines;
- a refused candidate (wrong claimed parent amount) changes nothing and is not submitted;
- an authenticated candidate the source rejects is recorded and changes nothing;
- the source row carries the fields above;
- a reorg replaces the applied block, and the checkpoint equals the winning branch applied
  from scratch;
- a restart from both durable stores resumes and reaches the same state;
- a height or chain mismatch is refused with nothing applied;
- a parent-output outage stops the block, and the block applies after the outage;
- status over loopback is `false` before and `true` after an admitted block;
- observer rotation keeps the chains aligned and a fork after it resolves;
- the config parser is strict and the pins are checked against the bootstrap.

## Live run, 2026-11-05

Linux release build (Rust 1.90.0, `--locked --offline`) on the witness box against its pruned
Bitcoin Core testnet4 node. Base 159,118 (tip 159,158 at the start), bootstrap
`collateral_seed_atoms = 100000000`, `conserving-sequential-v2`, `realtime`.

- `--initialize --max-blocks 20`: blocks 159,119 to 159,138 admitted and applied in 1.27 s;
  every block `candidates 0`, `authenticated 0`; exit 0. `GET /initstatus` on the bind
  address returned `authenticated_admission:true`, `admitted_blocks:20`, economic height 20,
  `economicHead` `21:20:...138:7c4897e1...`. A second run from a clean root produced the same
  economic head byte for byte.
- Restart without `--initialize` and without anchor flags: restored the economic head from
  its log and the observer from its head log, applied 21 further blocks (to 159,159, a block
  that arrived during the run), answered `/initstatus` over loopback (`admitted_blocks:21`),
  and exited 0 on the stop file.
- A restart with anchor flags that differ from `node-base` exited 2; a non-loopback `--bind`
  exited 2.

Testnet4 carries no TradeLayer transactions, so every candidate count is 0.

## Cost

Each commit verifies the checkpoint by replaying the complete retained source history, so
the cost of applying a block grows with the number of blocks the chain holds. Twenty blocks
take 1.3 s in a release build; `tests/node_ingest.rs` keeps its chains short for the debug
build. Bounding the retained history belongs to the economic snapshot format, not this slice.

## Not in this slice

- Other inputs of a transaction and their parents; parent inclusion and unspentness; other
  input templates and the legacy source's sender conventions for them.
- Complete Bitcoin block and transaction consensus; the active chain is Core's word.
- An authenticated historical bootstrap, so a node can start at a launch height instead of
  at its base; absolute-height rules under a nonzero base.
- The `TLR` binary carrier and the legacy non-canonical `746c` layouts.
- A prepared line for the economic head log, which would remove the one-commit crash window.
- Parent-output responses above the RPC body limit (very large mainnet blocks).
