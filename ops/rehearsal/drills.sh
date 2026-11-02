#!/usr/bin/env bash
# Operator drills for tl_observe against the rehearsal's Bitcoin testnet4 node.
# Each drill gets a fresh store under $R/drills/<name>, anchored at the pinned
# testnet4 genesis while the node still holds it. Once the node has pruned its
# earliest blocks, the drills anchor 3,000 blocks below its tip with this
# node's hash: they exercise the runner, and that anchor is not an independent
# declaration.
#
# Early testnet4 blocks are small and one block takes about six steps, so each
# stateful drill uses its own store and step budgets stay well under the
# 64-block spool (drill 9 fills it on purpose).
#
# Prints one line per check: PASS, FAIL or OBSERVED (behaviour recorded with no
# documented expectation). Exit status is the number of FAIL lines.
set -uo pipefail
R=${REHEARSAL_HOME:-$HOME/rehearsal}
HERE=$(cd "$(dirname "$0")" && pwd)
OBS=$HERE/observe.sh
FAILS=0
pass() { echo "PASS     $*"; }
fail() { echo "FAIL     $*"; FAILS=$((FAILS + 1)); }
seen() { echo "OBSERVED $*"; }
field() { python3 -c "import json,sys; print(json.loads(sys.stdin.read().splitlines()[-1])['$1'])"; }
height() { cut -d: -f2 "$1/head.token"; }
fresh() {
  export OBSERVER_STATE=$R/drills/$1
  rm -rf "$OBSERVER_STATE"
  mkdir -p "$OBSERVER_STATE"
  "$OBS" init "${ANCHOR[@]}" > /dev/null
}
wait_exit() { # pid, seconds
  local i
  for ((i = 0; i < $2; i++)); do kill -0 "$1" 2> /dev/null || return 0; sleep 1; done
  return 1
}

cli() { "$R/btc/bitcoin-cli" -datadir="$R/btc-data" "$@"; }
tip=$(cli getblockcount)
[ "$tip" -ge 200 ] || { echo "node tip $tip: wait for at least 200 blocks" >&2; exit 100; }
ANCHOR=()
pruned=$(cli getblockchaininfo \
  | python3 -c 'import json,sys; print(json.load(sys.stdin).get("pruneheight", 0))')
if [ "$pruned" -gt 0 ]; then
  a=$((tip - 3000))
  ANCHOR=("$a" "$(cli getblockhash "$a")")
  echo "NOTE     node pruned below $pruned: drills anchor at $a with this node's hash"
fi

# 1. Initialize and observe from genesis.
fresh observe
out=$("$OBS" run 60)
[ "$(echo "$out" | field mode)" = Observing ] && [ "$(height "$OBSERVER_STATE")" -gt 0 ] \
  && pass "observe: 60 steps reach height $(height "$OBSERVER_STATE"), mode Observing" \
  || fail "observe: $out"

# 2. A status load makes no step and returns the same head.
before=$(cat "$OBSERVER_STATE/head.token")
"$OBS" status > /dev/null
[ "$(cat "$OBSERVER_STATE/head.token")" = "$before" ] \
  && pass "status: head unchanged by a load" || fail "status: head moved"

# 3. A wrong expected head is refused and the store stays loadable.
real=$(cat "$OBSERVER_STATE/head.token")
tampered=${real%?}$([ "${real: -1}" = 0 ] && echo 1 || echo 0)
if EXPECTED_HEAD=$tampered "$OBS" status > /dev/null 2>&1; then
  fail "wrong token: tampered commitment accepted"
else pass "wrong token: tampered commitment refused"; fi
EXPECTED_HEAD=$real "$OBS" status > /dev/null 2>&1 && pass "wrong token: real head still loads" \
  || fail "wrong token: real head no longer loads"

# 4. A running session holds the store lock; the stop file stops it cleanly.
fresh lock
"$OBS" run 12 > /dev/null
"$OBS" run > "$OBSERVER_STATE/bg.out" 2>&1 &
bg=$!
sleep 3
if "$OBS" status > /dev/null 2>&1; then fail "lock: second process opened a running store"
else pass "lock: second process refused while a session runs"; fi
"$OBS" stop
if wait_exit "$bg" 60; then
  wait "$bg"; code=$?
  m=$(field mode < "$OBSERVER_STATE/bg.out")
  [ "$code" -eq 0 ] && [ "$m" = Stopped ] && pass "stop file: session exited 0 in mode Stopped" \
    || fail "stop file: exit $code mode $m"
else
  fail "stop file: session still running after 60 s"; kill "$bg"
fi

# 5. Restart after a graceful stop resumes and advances.
h0=$(height "$OBSERVER_STATE")
out=$("$OBS" run 60)
[ "$(echo "$out" | field mode)" = Observing ] && [ "$(height "$OBSERVER_STATE")" -gt "$h0" ] \
  && pass "restart: resumed from $h0 to $(height "$OBSERVER_STATE")" || fail "restart: $out"

# 6. Pause holds the store; the runner refuses to observe until resume.
fresh pause
"$OBS" run 30 > /dev/null
"$OBS" pause > /dev/null
h0=$(height "$OBSERVER_STATE")
if "$OBS" run 50 > /dev/null 2>&1; then fail "pause: runner observed a paused store"
else pass "pause: runner refuses a paused store"; fi
"$OBS" resume > /dev/null && out=$("$OBS" run 60)
[ "$(height "$OBSERVER_STATE")" -gt "$h0" ] && pass "pause: resume advances from $h0" \
  || fail "pause: resume did not advance"

# 7. Hard kill mid-session: the printed head goes stale, heads.log restarts it.
fresh kill
"$OBS" run 12 > /dev/null
printed=$(cat "$OBSERVER_STATE/head.token")
h0=$(echo "$printed" | cut -d: -f2)
"$OBS" run > "$OBSERVER_STATE/bg.out" 2>&1 &
bg=$!
sleep 10
pkill -9 -f "bin/tl_observe --root $OBSERVER_STATE/store"
wait "$bg" 2> /dev/null
if EXPECTED_HEAD=$printed "$OBS" status > /dev/null 2>&1; then
  fail "hard kill: nothing committed during the killed session; the drill proves nothing"
else pass "hard kill: last printed head (height $h0) is stale"; fi
if "$OBS" status > /dev/null 2>&1 && [ "$(height "$OBSERVER_STATE")" -gt "$h0" ]; then
  pass "hard kill: heads.log restarts the store at height $(height "$OBSERVER_STATE")"
else fail "hard kill: heads.log did not restart the store"; fi
h1=$(height "$OBSERVER_STATE")
out=$("$OBS" run 30)
[ "$(height "$OBSERVER_STATE")" -gt "$h1" ] && pass "hard kill: observation continues past $h1" \
  || fail "hard kill: no progress after the restart: $out"
pgrep -f "bin/tl_observe --root $OBSERVER_STATE/store" > /dev/null \
  && fail "hard kill: tl_observe still running" || pass "hard kill: no orphaned tl_observe"

# 8. RPC outage: a refused port exhausts the retry budget and leaves the loop.
fresh outage
"$OBS" run 30 > /dev/null
RPC_ADDRESS=127.0.0.1:9 "$OBS" run > "$OBSERVER_STATE/bg.out" 2>&1 &
bg=$!
if wait_exit "$bg" 300; then
  m=$(field mode < "$OBSERVER_STATE/bg.out" 2> /dev/null || echo none)
  [ "$m" != Observing ] && pass "outage: left the run loop in mode $m" \
    || fail "outage: exited while Observing"
  if "$OBS" resume > /dev/null 2>&1 && out=$("$OBS" run 30); then
    seen "outage: resume against the live node gives mode $(echo "$out" | field mode)"
  else
    seen "outage: resume refused from mode $m"
  fi
else
  fail "outage: still running after 300 s against a refused port"; "$OBS" stop; wait "$bg"
fi

# 9. Capacity: the 64-record spool quarantines instead of evicting.
fresh capacity
out=$("$OBS" run 2000)
m=$(echo "$out" | field mode)
n=$(echo "$out" | field stored_blocks)
[ "$m" != Observing ] && [ "$n" -le 64 ] \
  && pass "capacity: stopped at $n stored blocks in mode $m" \
  || fail "capacity: mode $m with $n stored blocks"
if "$OBS" run 10 > /dev/null 2>&1; then fail "capacity: runner observed past a full spool"
else pass "capacity: full spool is not resumable by the runner"; fi

echo "FAILS=$FAILS"
exit "$FAILS"
