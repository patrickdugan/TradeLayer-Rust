# tl_observe dress rehearsal

Runs the RC-1 observation service (`tl_observe`, built from this tree) as an
operator would: against a real mainnet node, under systemd, with scripted
operator drills. It exercises what RC-1 ships, the durable observation
daemon. RC-1 has no authenticated economic admission (see `docs/RC.md`, Scope),
so nothing here applies blocks to a Ledger, and nothing touches a wallet.

## Layout on the box

| Path | Contents |
|---|---|
| `~/rehearsal/bin/tl_observe` | release build of this tree |
| `~/rehearsal/ltc/` | Litecoin Core 0.21.5.8 `litecoind`, `litecoin-cli` |
| `~/rehearsal/ltc-data/` | pruned mainnet datadir, `litecoin.conf`, RPC `.cookie` |
| `~/rehearsal/kit/` | `observe.sh`, `drills.sh`, `status.sh` |
| `~/rehearsal/observer/` | the long-running store, `head.token`, `sessions.log`, `errors.log` |
| `~/rehearsal/drills/<name>/` | one throwaway store per drill |

The node is mainnet, pruned to 2 GB of block files, `blocksonly`, no inbound
listener, RPC on 127.0.0.1:9332 with cookie auth. `tl_observe` reads the cookie
file, so no RPC password exists anywhere.

## Staging

```bash
bash ops/rehearsal/install.sh /path/to/TradeLayer-Rust
sudo systemctl start rehearsal-litecoind
~/rehearsal/kit/status.sh
```

`install.sh` accepts the Litecoin tarball only when `SHA256SUMS.asc` carries a
valid signature from a key in the Litecoin repository's `contrib/gitian-keys`
at the release tag, and the tarball's hash matches it.

## Drills

`drills.sh` anchors each drill store at the pinned Litecoin genesis, so it needs
the node's earliest blocks: run it in the first part of the pruned IBD, before
the node prunes `blk00000.dat`.

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

The spool holds at most 64 raw blocks and then quarantines instead of evicting
(`docs/DAEMON.md`, Bounds). On Litecoin's 2.5-minute blocks that is under three
hours of observation per store.

## Results, 2026-11-02

Box: 1 vCPU, 2 GB RAM + 1 GB swap, 19 GB disk. `tl_observe` built from
`1a077c1` (release, 7 min 39 s; sha256 `151b7f3d…a965f539ae`). Litecoin Core
0.21.5.8, signature valid from the repository's `davidburkett38` key. Drills ran
at node heights 11,645 to 33,218 of IBD: 15 PASS, 0 FAIL, 2 OBSERVED.

| Drill | Result |
|---|---|
| observe from genesis | 60 steps reach height 9, mode `Observing` |
| status load | head unchanged |
| tampered expected head | refused; the real head still loads |
| second process during a session | refused (store lock) |
| stop file | exit 0, mode `Stopped`, head recorded |
| restart after stop | resumes and advances |
| pause | runner refuses a paused store; resume advances |
| hard kill (SIGKILL after 10 s) | OBSERVED: the last recorded head is refused with `StaleHead` |
| RPC outage (refused port) | `Quarantined(RetryExhausted)`; OBSERVED: resume against the live node returns to `Observing` |
| capacity | stops at 64 blocks in `Quarantined(SpoolFull)`; not resumable |

Findings:

1. **Hard-kill recovery.** `tl_observe` prints the observation head only when a
   session ends. After SIGKILL, OOM kill or power loss, the store has committed
   frames past the last recorded head, the store refuses that head
   (`StaleHead`), and the CLI has no operation that reveals the current head
   without one. Recovery needs an independent record of each committed head,
   for example a per-commit head line on stdout that the supervisor persists.
2. **Spool lifetime.** The 64-block cap ends a store in under three hours on
   Litecoin mainnet. Continuous observation needs the externally reviewed
   archival/new-store process `docs/DAEMON.md` leaves open.
3. **Node memory.** `litecoind` was OOM-killed at a 1,300 MB cgroup cap during
   header sync (about 3.2 million headers). With `dbcache=100` and a 1,700 MB
   cap it has run without restarts. The observer met that node crash
   unattended: bounded retries, then `Quarantined(RetryExhausted)`, resumable.
