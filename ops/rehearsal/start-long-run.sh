#!/usr/bin/env bash
# Starts the long observer run once the node leaves IBD. The anchor is the
# block 6 below the node's tip, accepted only when the node and an independent
# explorer report the same hash for that height; on any disagreement or lookup
# failure it starts nothing and exits 1.
#
#   nohup setsid ~/rehearsal/kit/start-long-run.sh < /dev/null > /dev/null 2>&1 &
#
# Progress goes to $R/long-run.log. EXPLORER overrides the explorer base URL
# (an Esplora-style API with /block-height/HEIGHT).
set -uo pipefail
R=${REHEARSAL_HOME:-$HOME/rehearsal}
HERE=$(cd "$(dirname "$0")" && pwd)
EXPLORER=${EXPLORER:-https://mempool.space/testnet4/api}
LOG=$R/long-run.log
cli() { "$R/btc/bitcoin-cli" -datadir="$R/btc-data" "$@"; }
say() { echo "$(date -u +%FT%TZ) $*" >> "$LOG"; }

say "waiting for the node to leave IBD"
until [ "$(cli getblockchaininfo 2> /dev/null \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["initialblockdownload"])' 2> /dev/null)" = False ]; do
  sleep 600
done
tip=$(cli getblockcount) || { say "getblockcount failed"; exit 1; }
height=$((tip - 6))
node=$(cli getblockhash "$height") || { say "getblockhash $height failed"; exit 1; }
other=$(curl -fsS -m 30 "$EXPLORER/block-height/$height") || { say "explorer lookup of $height failed"; exit 1; }
if ! [[ $node =~ ^[0-9a-f]{64}$ ]] || [ "$node" != "$other" ]; then
  say "anchor $height disagrees: node $node, explorer $other"
  exit 1
fi
say "anchor $height $node (node and $EXPLORER agree)"
"$HERE/observe.sh" init "$height" "$node" >> "$LOG" 2>&1 || { say "init failed"; exit 1; }
sudo -n systemctl start rehearsal-observe || { say "systemctl start failed"; exit 1; }
say "rehearsal-observe started"
