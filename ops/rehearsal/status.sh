#!/usr/bin/env bash
# Node sync progress, disk, and the last recorded observer sessions.
set -uo pipefail
R=${REHEARSAL_HOME:-$HOME/rehearsal}
STATE=${OBSERVER_STATE:-$R/observer}
"$R/btc/bitcoin-cli" -datadir="$R/btc-data" getblockchaininfo 2> /dev/null | python3 -c '
import json, sys
i = json.load(sys.stdin)
print("node  chain=%s blocks=%s headers=%s progress=%.4f ibd=%s pruneheight=%s size=%.2fGB" % (
    i["chain"], i["blocks"], i["headers"], i["verificationprogress"],
    i["initialblockdownload"], i.get("pruneheight"), i["size_on_disk"] / 1e9))
' || echo "node  RPC unavailable"
df -h / | awk 'NR == 2 {print "disk  used=" $3 " free=" $4 " (" $5 ")"}'
free -m | awk '/^Mem/ {print "mem   used=" $3 "M available=" $7 "M"}'
systemctl is-active rehearsal-bitcoind.service rehearsal-observe.service 2> /dev/null \
  | paste -sd' ' | sed 's/^/units bitcoind,observe = /'
[ -s "$STATE/head.token" ] && echo "token $(cat "$STATE/head.token")"
[ -s "$STATE/heads.log" ] && echo "heads.log $(tail -n 1 "$STATE/heads.log")"
if [ -f "$STATE/sessions.log" ]; then tail -3 "$STATE/sessions.log"; fi
