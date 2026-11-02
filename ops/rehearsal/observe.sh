#!/usr/bin/env bash
# One tl_observe operation against the rehearsal's pruned Litecoin node.
#
#   observe.sh init [H HASH]   create the store, anchored at the pinned LTC genesis
#                              or at the declared height H with block hash HASH
#   observe.sh run [STEPS]     observe (forever with no STEPS) until stopped
#   observe.sh stop            ask a running session to stop (stop file)
#   observe.sh pause|resume    durable pause / resume
#   observe.sh status          load the store and print its status (no RPC step)
#
# The head printed by every successful operation is written to head.token. The
# next operation must present it; the store refuses any other head. A session
# killed before it prints leaves head.token behind the store.
set -euo pipefail
R=${REHEARSAL_HOME:-$HOME/rehearsal}
STATE=${OBSERVER_STATE:-$R/observer}
STORE=$STATE/store
TOKEN=$STATE/head.token
STOP=$STATE/stop
mkdir -p "$STORE"
BASE=(
  "$R/bin/tl_observe" --root "$STORE" --network LTC
  --rpc-address "${RPC_ADDRESS:-127.0.0.1:9332}" --cookie-file "$R/ltc-data/.cookie"
)

invoke() { # label, tl_observe arguments...
  local label=$1 out status=0
  shift
  out=$("${BASE[@]}" "$@" 2> >(tee -a "$STATE/errors.log" >&2)) || status=$?
  printf '%s %s exit=%s %s\n' "$(date -u +%FT%TZ)" "$label" "$status" "$out" >> "$STATE/sessions.log"
  [ "$status" -eq 0 ] || return "$status"
  local head
  head=$(printf '%s' "$out" | python3 -c 'import json,sys; print(json.load(sys.stdin)["head"])')
  printf '%s\n' "$head" > "$TOKEN.tmp"
  sync "$TOKEN.tmp"
  mv "$TOKEN.tmp" "$TOKEN"
  sync "$STATE"
  printf '%s\n' "$out"
}
need_token() {
  [ -s "$TOKEN" ] || { echo "no head.token in $STATE; run init first" >&2; exit 2; }
}
mode() { python3 -c 'import json,sys; print(json.loads(sys.stdin.read().splitlines()[-1])["mode"])'; }

case ${1:-} in
  init)
    [ ! -e "$TOKEN" ] || { echo "$STATE already has a head.token" >&2; exit 2; }
    if [ -n "${2:-}" ]; then
      invoke init --initialize --anchor-height "$2" --anchor-hash "$3" --steps 0
    else
      invoke init --initialize --steps 0
    fi
    ;;
  status)
    need_token
    invoke status --expected-head "$(cat "$TOKEN")" --steps 0
    ;;
  pause | resume)
    need_token
    invoke "$1" --expected-head "$(cat "$TOKEN")" "--$1"
    ;;
  stop)
    touch "$STOP"
    ;;
  run)
    need_token
    rm -f "$STOP"
    current=$(invoke status --expected-head "$(cat "$TOKEN")" --steps 0 | mode)
    case $current in
      Observing) ;;
      Stopped) invoke resume --expected-head "$(cat "$TOKEN")" --resume > /dev/null ;;
      *) echo "store is in mode $current; inspect it before running" >&2; exit 3 ;;
    esac
    if [ -n "${2:-}" ]; then
      invoke run --expected-head "$(cat "$TOKEN")" --steps "$2"
    else
      invoke run --expected-head "$(cat "$TOKEN")" --run --stop-file "$STOP"
    fi
    ;;
  *)
    sed -n '2,11p' "$0" >&2
    exit 2
    ;;
esac
