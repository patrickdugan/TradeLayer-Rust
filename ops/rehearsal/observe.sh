#!/usr/bin/env bash
# One tl_observe operation against the rehearsal's pruned Bitcoin testnet4 node.
#
#   observe.sh init [H HASH]   create the store, anchored at the pinned testnet4 genesis
#                              or at the declared height H with block hash HASH
#   observe.sh run [STEPS]     observe (forever with no STEPS) until stopped
#   observe.sh stop            ask a running session to stop (stop file)
#   observe.sh pause|resume    durable pause / resume
#   observe.sh status          load the store and print its status (no RPC step)
#   observe.sh reconcile H:HASH  after a fork, keep the stored selected path up to
#                              block H (HASH must be the stored block there)
#
# tl_observe appends every head it commits to heads.log (prepared before the
# store write, committed after it) and restarts from that log, so a session
# killed at any point stays loadable. EXPECTED_HEAD=SEQ:HEIGHT:HASH:COMMITMENT
# presents an explicit head instead. The head printed by each successful
# operation is also copied to head.token for reading. A declared anchor is
# kept in the file anchor and passed to every later operation: the store
# refuses (InvalidSnapshot) a configuration other than the one it was created
# with.
set -euo pipefail
R=${REHEARSAL_HOME:-$HOME/rehearsal}
STATE=${OBSERVER_STATE:-$R/observer}
STORE=$STATE/store
TOKEN=$STATE/head.token
HEADS=$STATE/heads.log
STOP=$STATE/stop
ANCHOR_FILE=$STATE/anchor
mkdir -p "$STORE"
if [ "${1:-}" = init ] && [ -n "${2:-}" ]; then
  [ ! -e "$HEADS" ] || { echo "$STATE already has a heads.log" >&2; exit 2; }
  printf '%s %s\n' "$2" "$3" > "$ANCHOR_FILE"
  sync "$ANCHOR_FILE"
elif [ "${1:-}" = init ]; then
  rm -f "$ANCHOR_FILE"
fi
ANCHOR=()
if [ -s "$ANCHOR_FILE" ]; then
  read -r anchor_height anchor_hash < "$ANCHOR_FILE"
  ANCHOR=(--anchor-height "$anchor_height" --anchor-hash "$anchor_hash")
fi
BASE=(
  "$R/bin/tl_observe" --root "$STORE" --network BTC-TESTNET4
  --rpc-address "${RPC_ADDRESS:-127.0.0.1:48332}" --cookie-file "$R/btc-data/testnet4/.cookie"
  --head-log "$HEADS" "${ANCHOR[@]}"
)
EXPECT=()
[ -z "${EXPECTED_HEAD:-}" ] || EXPECT=(--expected-head "$EXPECTED_HEAD")

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
need_log() {
  [ -s "$HEADS" ] || { echo "no heads.log in $STATE; run init first" >&2; exit 2; }
}
mode() { python3 -c 'import json,sys; print(json.loads(sys.stdin.read().splitlines()[-1])["mode"])'; }

case ${1:-} in
  init)
    [ ! -e "$HEADS" ] || { echo "$STATE already has a heads.log" >&2; exit 2; }
    invoke init --initialize --steps 0
    ;;
  status)
    need_log
    invoke status "${EXPECT[@]}" --steps 0
    ;;
  pause | resume)
    need_log
    invoke "$1" "${EXPECT[@]}" "--$1"
    ;;
  reconcile)
    need_log
    [ -n "${2:-}" ] || { echo "usage: observe.sh reconcile HEIGHT:HASH" >&2; exit 2; }
    invoke reconcile "${EXPECT[@]}" --reconcile "$2"
    ;;
  stop)
    touch "$STOP"
    ;;
  run)
    need_log
    rm -f "$STOP"
    current=$(invoke status "${EXPECT[@]}" --steps 0 | mode)
    case $current in
      Observing) ;;
      Stopped) invoke resume "${EXPECT[@]}" --resume > /dev/null ;;
      *) echo "store is in mode $current; inspect it before running" >&2; exit 3 ;;
    esac
    EXPECT=() # the explicit head is spent; the session continues from heads.log
    if [ -n "${2:-}" ]; then
      invoke run "${EXPECT[@]}" --steps "$2"
    else
      invoke run "${EXPECT[@]}" --run --stop-file "$STOP"
    fi
    ;;
  *)
    sed -n '2,11p' "$0" >&2
    exit 2
    ;;
esac
