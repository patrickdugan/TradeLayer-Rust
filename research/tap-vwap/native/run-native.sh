#!/usr/bin/env bash
# Run only in a disposable Linux environment. No live wallets or secrets.
set -euo pipefail
umask 077
for cmd in git go make python3; do command -v "$cmd" >/dev/null; done
work=$(mktemp -d)
cleanup() {
  if [[ "${TAP_VWAP_KEEP_PRIVATE:-0}" == 1 ]]; then
    printf 'Private disposable workspace retained at %s\n' "$work" >&2
  else
    rm -rf -- "$work"
  fi
}
trap cleanup EXIT
export GOTOOLCHAIN=go1.26.3
commit=734ce74dd1e816a833a9727bba741718019fef18
src="$work/taproot-assets"
git clone --quiet --depth 1 --branch v0.8.6 https://github.com/lightninglabs/taproot-assets.git "$src"
actual=$(git -C "$src" rev-parse HEAD)
[[ "$actual" == "$commit" ]] || { echo "Unexpected upstream commit" >&2; exit 1; }
python3 - "$src" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]) / "itest/custom_channels/forwarding_history_test.go"
s = p.read_text()
for who, event in (("purchase", "daveFwd"), ("sale", "erinFwd")):
    resp = "daveResp" if who == "purchase" else "erinResp"
    anchor = f"\trequire.Equal(t.t, int64(1), {resp}.TotalCount)"
    if s.count(anchor) != 1:
        raise SystemExit(f"source instrumentation anchor mismatch: {who}")
    marker = f'\n\tt.Logf("TLVWAP02 {who} %d %d %d %d %d", {event}.AssetAmt, {event}.AmtInMsat, {event}.AmtOutMsat, {event}.SettledAt, {event}.FailedAt)'
    s = s.replace(anchor, anchor + marker)
p.write_text(s)
PY
printf 'Pinned tapd: %s\n' "$actual"
# The upstream Makefile builds regtest-only binaries; never point them at a live daemon.
# Avoid publishing raw logs, which can include local credentials or private test state.
if ! (cd "$src" && make itest-cc cccase=forwarding_history) > "$work/native.log" 2>&1; then
  echo "Native build/test failed; no verified smoke report. Inspect private log on a disposable local runner." >&2
  exit 1
fi
python3 - "$work/native.log" "$actual" <<'PY'
import json, pathlib, re, sys
log = pathlib.Path(sys.argv[1]).read_text(errors="replace")
if not re.search(r"(?m)^\s*--- PASS: TestCustomChannels/forwarding_history(?:\s|\()", log):
    raise SystemExit("Targeted forwarding-history test did not report PASS")
rows = {}
for side, asset, incoming, outgoing, settled, failed in re.findall(
    r"(?m)TLVWAP02 (purchase|sale) ([0-9]+) ([0-9]+) ([0-9]+) ([0-9]+) ([0-9]+)\s*$", log
):
    if side in rows:
        raise SystemExit("Duplicate smoke marker: " + side)
    a, i, o, s, f = map(int, (asset, incoming, outgoing, settled, failed))
    if min(a, i, o, s) <= 0 or f != 0:
        raise SystemExit("Invalid smoke quantities or settlement flags")
    if (side == "purchase" and o <= i) or (side == "sale" and i <= o):
        raise SystemExit("Expected BTC leg/asset-anchor relation missing")
    rows[side] = {
        "asset_atoms": str(a),
        "incoming_msat": str(i),
        "outgoing_msat": str(o),
        "candidate_gross_btc_msat": str(o if side == "purchase" else i),
        "local_settled": True,
        "evidence_tier": 0,
    }
if set(rows) != {"purchase", "sale"}:
    raise SystemExit("Missing smoke markers; no report")
print(json.dumps({
    "stage": "TAP-VWAP-02A", "upstream_commit": sys.argv[2],
    "smoke": "PASS", "native_records": rows,
    "authenticated_settlement": False, "principal_vwap_ready": False,
}, indent=2, sort_keys=True))
PY
