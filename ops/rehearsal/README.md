# tl_observe dress rehearsal

Runs the RC-1 observation service (`tl_observe`, built from this tree) as an
operator would: against a real Bitcoin testnet4 node, under systemd, with
scripted operator drills. It exercises what RC-1 ships, the durable
observation daemon. RC-1 has no authenticated economic admission (see
`docs/RC.md`, Scope), so nothing here applies blocks to a Ledger, and nothing
touches a wallet. Testnet4 observation is observation only
(`--network BTC-TESTNET4`, `docs/RUNTIME.md`).

## Layout on the box

| Path | Contents |
|---|---|
| `~/rehearsal/bin/tl_observe` | release build of this tree |
| `~/rehearsal/btc/` | Bitcoin Core 31.1 `bitcoind`, `bitcoin-cli` |
| `~/rehearsal/btc-data/` | pruned testnet4 datadir, `bitcoin.conf`, RPC cookie in `testnet4/.cookie` |
| `~/rehearsal/kit/` | `observe.sh`, `drills.sh`, `status.sh`, `start-long-run.sh` |
| `~/rehearsal/observer/` | the long-running store, `anchor`, `heads.log`, `head.token`, `sessions.log`, `errors.log` |
| `~/rehearsal/drills/<name>/` | one throwaway store per drill |

The node is testnet4, pruned to 2 GB of block files, `blocksonly`, no inbound
listener, RPC on 127.0.0.1:48332 with cookie auth. `tl_observe` reads the
cookie file, so no RPC password exists anywhere.

## Staging

```bash
bash ops/rehearsal/install.sh /path/to/TradeLayer-Rust
sudo systemctl start rehearsal-bitcoind
~/rehearsal/kit/status.sh
```

`install.sh` accepts the Bitcoin Core tarball only when at least 3 keys from
`bitcoin-core/guix.sigs` `builder-keys` validly sign `SHA256SUMS` and the
tarball's hash matches it. Build `tl_observe` before starting `bitcoind`: on a
2 GB box a release build alongside a syncing node runs out of memory.

Every operation passes `--head-log ~/rehearsal/observer/heads.log`: tl_observe
appends each head it commits (`prepared` before the store write, `committed`
after it) and restarts from that log, so a SIGKILL, OOM kill or power loss
mid-session leaves the store loadable. `head.token` holds the head printed by
the last operation that exited normally.

## Drills

`drills.sh` anchors each drill store at the pinned testnet4 genesis while the
node still holds its first block file. On a pruned node it anchors 3,000
blocks below the tip using that node's hash, which suits a runner drill but is
not an independent anchor declaration.

```bash
~/rehearsal/kit/drills.sh
```

## Long run

After the node finishes IBD, initialize the long-running store and start the
service. An anchor above genesis is a caller declaration that the observer does
not authenticate; take the anchor hash from a source independent of this node.

```bash
~/rehearsal/kit/observe.sh init HEIGHT HASH
sudo systemctl start rehearsal-observe
sudo systemctl stop rehearsal-observe    # graceful: stop file, head recorded
```

`start-long-run.sh` does both unattended: it waits for the node to leave IBD,
anchors 6 blocks below the tip only when the node and mempool.space's
testnet4 API report the same hash, and logs to `~/rehearsal/long-run.log`.

```bash
nohup setsid ~/rehearsal/kit/start-long-run.sh < /dev/null > /dev/null 2>&1 &
```

The spool holds at most 64 raw blocks and then quarantines instead of evicting
(`docs/DAEMON.md`, Bounds): about ten hours of testnet4 at its 10-minute
target, less when blocks come faster. Orphaned blocks stay in the spool and
count against the cap.

When the node reorganizes below the observer's cursor, the session ends in
`Quarantined(ForkDetected)`. Before exiting, `tl_observe` finishes its fork
search and prints the evidence; `fork.common_ancestor` is the highest stored
block still on the node's active chain. The observer never picks a branch
itself. `observe.sh reconcile` with no target reconciles to that ancestor;
`reconcile HEIGHT:HASH` names one explicitly, and `tl_observe` refuses
(`InvalidTransition`, no state change) a height whose stored block differs.

```bash
~/rehearsal/kit/observe.sh reconcile
sudo systemctl start rehearsal-observe
```

The unit sets `AUTO_RECONCILE_DEPTH=6`: `observe.sh run` reconciles a fork
whose ancestor is at most 6 blocks below the old tip, records it in
`sessions.log` and the journal, and runs on. A deeper fork, a fork without an
ancestor (the search hit the anchor, an RPC probe failed, or the hash changed
during the final recheck) or any other quarantine stops the unit for an
operator.

## Results, 2026-11-02 (testnet4)

Box: 1 vCPU, 2 GB RAM + 1 GB swap, 19 GB disk. `tl_observe` from `a8e5ac8`
(release, 7 min 35 s; sha256 `49b519dc…a06888a08`). Bitcoin Core 31.1, 11
valid builder signatures. Drills anchored at the testnet4 genesis while the
node was syncing (tip above 10,800 of 158,625 headers): 17 PASS, 0 FAIL,
1 OBSERVED.

| Drill | Result |
|---|---|
| observe from genesis | 60 steps reach height 9, mode `Observing` |
| status load | head unchanged |
| tampered expected head | refused; the real head still loads |
| second process during a session | refused (store lock) |
| stop file | exit 0, mode `Stopped`, head recorded |
| restart after stop | resumes and advances |
| pause | runner refuses a paused store; resume advances |
| hard kill (SIGKILL after 10 s) | the printed head (height 1) is stale; `heads.log` restarts the store at 17 and observation continues; no orphaned process |
| RPC outage (refused port) | `Quarantined(RetryExhausted)`; OBSERVED: resume against the live node returns to `Observing` |
| capacity | stops at 64 blocks in `Quarantined(SpoolFull)`; not resumable |

Long run: the node left IBD at 158,627 after about 30 minutes;
`start-long-run.sh` anchored at 158,621 (node and mempool.space agreed) and
started the unit at 15:52 UTC. At 16:11 testnet4 replaced 158,626-158,627
(the node then showed nine competing tips above 158,625) and the session
ended `Quarantined(ForkDetected)` in phase `ForkSearch` with the cursor at
158,627. `reconcile` refused 158,626 and accepted 158,625; the restarted unit
reached the tip, 158,630, within a minute.

Earlier runs the same day used a pruned Litecoin mainnet node, the wrong
network for this rehearsal; `tl_observe` then accepted only mainnets. They
produced findings 1 and 3, which carry over.

Findings:

1. **Hard-kill recovery (fixed in `8689b5a`).** `tl_observe` printed the head
   only when a session ended, so after SIGKILL, OOM kill or power loss the
   store refused the last recorded head (`StaleHead`). `--head-log` records
   every head around each commit, and the hard-kill drill restarts from it.
   A kit bug surfaced alongside it: operations after `init HEIGHT HASH`
   omitted the anchor and the store refused the changed configuration
   (`InvalidSnapshot`); `observe.sh` keeps the anchor in `anchor`.
2. **Spool lifetime.** The 64-block cap ends a store after about ten hours of
   testnet4. Continuous observation needs the externally reviewed
   archival/new-store process `docs/DAEMON.md` leaves open.
3. **Memory on a 2 GB box.** A release build of `tl_observe` is OOM-killed
   while a syncing node holds most of the RAM. Build first, or stop the node
   unit for the build (about 8 minutes).
4. **Forks stopped the service (fixed in this kit and `tl_observe`).** On
   testnet4, short reorganizations are routine, and each one below the
   cursor ended the session until an operator found the common block by
   asking the node: the CLI exited on the quarantine before the daemon's fork
   search ran. `tl_observe` now finishes that search before exiting and
   prints the evidence, and the kit reconciles forks up to 6 blocks deep.
