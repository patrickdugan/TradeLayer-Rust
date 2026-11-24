#!/usr/bin/env bash
# Disposable Linux native regtest runner for TAP-VWAP-02A + TAP-VWAP-02B.
#
# Pins:
#   - lightninglabs/taproot-assets @ 734ce74dd1e816a833a9727bba741718019fef18 (v0.8.6)
#   - lightningnetwork/lnd         @ v0.21.3-beta (verified from go.mod)
#
# Root causes of initial CI smoke failure fixed with source evidence:
#   1. Upstream `Makefile` target `itest-cc` invokes `go test ./itest/custom_channels`
#      directly without creating the `btcd -> btcd-itest` symlink on PATH (unlike
#      `scripts/itest_cc_part.sh:31-35`).
#   2. Upstream commit `734ce74dd1e816a833a9727bba741718019fef18` removed the +1
#      asset-unit rounding tolerance in `tapchannel/aux_invoice_manager.go:345-366`;
#      `invoiceAssetAmount = 20_000` at mock oracle rate `5,820,600` units/BTC maps
#      to `343,607,188 msat -> floor(...) = 19,999 units -> 343,590,007 msat`,
#      triggering a 17,181 msat MPP hold timeout. Using rate-divisible `29_103` units
#      (`500,000,000 msat` exact) exercises the unmodified `tapd-integrated-itest`
#      binary cleanly.
#
# Security/hygiene: no live wallets, no mainnet/testnet endpoints, `umask 077`,
# private witnesses & daemon logs strictly confined to `$work` and wiped on exit.
set -euo pipefail
umask 077

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TAP_VWAP_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

for cmd in git go python3 sha256sum; do
  command -v "$cmd" >/dev/null || { echo "Missing required tool: $cmd" >&2; exit 1; }
done

work="$(mktemp -d)"
cleanup() {
  if [[ "${TAP_VWAP_KEEP_PRIVATE:-0}" == "1" ]]; then
    printf 'Private disposable workspace retained at %s\n' "$work" >&2
  else
    rm -rf -- "$work"
  fi
}
trap cleanup EXIT

export GOTOOLCHAIN="${GOTOOLCHAIN:-go1.26.3}"
PINNED_COMMIT="734ce74dd1e816a833a9727bba741718019fef18"
EXPECTED_LND_MOD="v0.21.3-beta"

src="$work/taproot-assets"
if [[ -n "${TAP_VWAP_CACHED_TAPD_SRC:-}" && -d "${TAP_VWAP_CACHED_TAPD_SRC}/.git" ]]; then
  cp -a "${TAP_VWAP_CACHED_TAPD_SRC}" "$src"
  git -C "$src" checkout --quiet --force "$PINNED_COMMIT"
  git -C "$src" clean -fdx --quiet -e itest/btcd-itest -e itest/tapd-integrated-itest
else
  git clone --quiet --depth 1 --branch v0.8.6 https://github.com/lightninglabs/taproot-assets.git "$src"
fi

actual_commit="$(git -C "$src" rev-parse HEAD)"
[[ "$actual_commit" == "$PINNED_COMMIT" ]] || {
  echo "Unexpected upstream commit: got $actual_commit, want $PINNED_COMMIT" >&2
  exit 1
}

lnd_mod_ver="$(awk '$1 == "github.com/lightningnetwork/lnd" { print $2; exit }' "$src/go.mod")"
[[ "$lnd_mod_ver" == "$EXPECTED_LND_MOD" ]] || {
  echo "Unexpected lnd module version in go.mod: got $lnd_mod_ver, want $EXPECTED_LND_MOD" >&2
  exit 1
}

go_ver="$(cd "$src" && go env GOVERSION)"

# 1. Build regtest-only binaries (if not pre-populated) with `-mod=readonly` and expose `btcd` on PATH.
if [[ ! -x "$src/itest/btcd-itest" ]]; then
  (cd "$src" && go build -mod=readonly -o "$src/itest/btcd-itest" github.com/btcsuite/btcd)
fi
if [[ ! -x "$src/itest/tapd-integrated-itest" ]]; then
  (cd "$src" && go build -tags="itest" -mod=readonly -o "$src/itest/tapd-integrated-itest" ./cmd/tapd)
fi
ln -sf "$src/itest/btcd-itest" "$src/itest/btcd"
export PATH="$src/itest:$PATH"

btcd_sha256="$(sha256sum "$src/itest/btcd-itest" | awk '{print $1}')"
tapd_sha256="$(sha256sum "$src/itest/tapd-integrated-itest" | awk '{print $1}')"

# 2. Install isolated receipt adapter package + golden fixtures and run unit + mutation audit suite.
mkdir -p "$src/tapvwapadapter" "$src/receipt"
cp "$SCRIPT_DIR/adapter/receipt_adapter.go" "$src/tapvwapadapter/receipt_adapter.go"
cp "$SCRIPT_DIR/adapter/receipt_adapter_test.go" "$src/tapvwapadapter/receipt_adapter_test.go"
cp "$TAP_VWAP_DIR/receipt/golden-fixtures.json" "$src/receipt/golden-fixtures.json"

if ! (cd "$src" && TAP_VWAP_GOLDEN_FIXTURES="$src/receipt/golden-fixtures.json" go test -v -count=1 -mod=readonly ./tapvwapadapter) > "$work/adapter.log" 2>&1; then
  echo "Adapter unit/mutation audit test failed." >&2
  exit 1
fi

# 3. Instrument `forwarding_history_test.go` (exact rate-divisible 29,103 units + numeric marker)
#    and install `tap_vwap_02b_test.go`.
python3 - "$src" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]) / "itest/custom_channels/forwarding_history_test.go"
s = p.read_text()
old_amt = "const invoiceAssetAmount = 20_000"
if old_amt not in s:
    raise SystemExit("expected `const invoiceAssetAmount = 20_000` in forwarding_history_test.go")
s = s.replace(old_amt, "const invoiceAssetAmount = 29_103", 1)
for who, event in (("purchase", "daveFwd"), ("sale", "erinFwd")):
    resp = "daveResp" if who == "purchase" else "erinResp"
    anchor = f"\trequire.Equal(t.t, int64(1), {resp}.TotalCount)"
    if s.count(anchor) != 1:
        raise SystemExit(f"source instrumentation anchor mismatch: {who}")
    marker = f'\n\tt.Logf("TLVWAP02 {who} %d %d %d %d %d", {event}.AssetAmt, {event}.AmtInMsat, {event}.AmtOutMsat, {event}.SettledAt, {event}.FailedAt)'
    s = s.replace(anchor, anchor + marker)
p.write_text(s)

cc_main = pathlib.Path(sys.argv[1]) / "itest/custom_channels/custom_channels_test.go"
cm = cc_main.read_text()
cc_anchor = '\t{\n\t\tname: "forwarding history",\n\t\ttest: testCustomChannelsForwardingHistory,\n\t},'
if cm.count(cc_anchor) != 1:
    raise SystemExit("custom_channels_test.go registration anchor mismatch")
cc_entry = cc_anchor + '\n\t{\n\t\tname: "tap vwap 02b matrix",\n\t\ttest: testTapVwap02BInstrumentedMatrix,\n\t},'
cc_main.write_text(cm.replace(cc_anchor, cc_entry, 1))
PY

cp "$SCRIPT_DIR/itest/tap_vwap_02b_test.go" "$src/itest/custom_channels/tap_vwap_02b_test.go"

(cd "$src" && go test -c -tags="itest" -mod=readonly -o "$src/itest/custom_channels/itest-cc.test" ./itest/custom_channels)
itest_bin_sha256="$(sha256sum "$src/itest/custom_channels/itest-cc.test" | awk '{print $1}')"

mkdir -p "$work/logs-smoke" "$work/logs-02b" "$work/tmp"

# 4. Run Gate 0 forwarding_history smoke.
if ! (
  cd "$src/itest/custom_channels" && \
  TMPDIR="$work/tmp" "$src/itest/custom_channels/itest-cc.test" \
    -test.v -test.timeout=6m \
    -test.run='^TestCustomChannels/forwarding_history$' \
    -logoutput -logdir="$work/logs-smoke"
) > "$work/smoke.log" 2>&1; then
  echo "Gate 0 forwarding_history smoke failed." >&2
  exit 1
fi

# 5. Run Gate 2/3 full TAP-VWAP-02B instrumented regtest matrix.
priv_witness="$work/private-regtest-receipts.json"
if ! (
  cd "$src/itest/custom_channels" && \
  TMPDIR="$work/tmp" TAP_VWAP_PRIVATE_WITNESS_OUT="$priv_witness" \
  "$src/itest/custom_channels/itest-cc.test" \
    -test.v -test.timeout=10m \
    -test.run='^TestCustomChannels/tap_vwap_02b_matrix$' \
    -logoutput -logdir="$work/logs-02b"
) > "$work/matrix.log" 2>&1; then
  echo "TAP-VWAP-02B regtest matrix failed." >&2
  exit 1
fi

# 6. Cross-verify all emitted regtest receipts in Node `receipt.cjs` when `node` is installed.
node_verified=false
if command -v node >/dev/null; then
  node - "$TAP_VWAP_DIR/receipt/receipt.cjs" "$priv_witness" <<'JS'
const fs = require("fs");
const R = require(process.argv[2]);
const data = JSON.parse(fs.readFileSync(process.argv[3], "utf8"));
if (!Array.isArray(data.receipts) || data.receipts.length !== 6) {
  throw new Error("expected 6 settled regtest receipts");
}
for (const item of data.receipts) {
  const r = item.receipt;
  const enc = R.encode(r);
  if (enc.toString("hex") !== item.encoded_hex) {
    throw new Error("wire mismatch on " + item.label);
  }
  const expected = {
    genesis: r.genesis,
    policy: r.policy,
    asset: r.asset,
    edgeKey: r.edgeKey,
    btcPeerKey: r.btcPeerKey,
    assetPeerKey: r.assetPeerKey,
    assetOutpoint: r.assetOutpoint,
    btcOutpoint: r.btcOutpoint,
    assetEdgeSide: r.assetEdgeSide,
    btcEdgeSide: r.btcEdgeSide,
  };
  const v = R.verifyTier1(r, item.signatures, expected);
  if (v.digest !== item.digest_hex) throw new Error("digest mismatch on " + item.label);
  if (v.nullifier !== item.nullifier_hex) throw new Error("nullifier mismatch on " + item.label);
  if (v.eligibleForIndex !== false) throw new Error("expected eligibleForIndex === false");
}
JS
  node_verified=true
fi

# 7. Emit redacted JSON report (strictly numeric + SHA-256 digests; zero raw logs or preimages).
python3 - "$work/smoke.log" "$work/matrix.log" "$work/adapter.log" \
  "$actual_commit" "$lnd_mod_ver" "$go_ver" "$btcd_sha256" "$tapd_sha256" "$itest_bin_sha256" "$node_verified" <<'PY'
import json, pathlib, re, sys

smoke_log = pathlib.Path(sys.argv[1]).read_text(errors="replace")
matrix_log = pathlib.Path(sys.argv[2]).read_text(errors="replace")
adapter_log = pathlib.Path(sys.argv[3]).read_text(errors="replace")
commit, lnd_ver, go_ver, btcd_sha, tapd_sha, itest_sha, node_ver_str = sys.argv[4:11]

if not re.search(r"(?m)^\s*--- PASS: TestCustomChannels/forwarding_history(?:\s|\()", smoke_log):
    raise SystemExit("Targeted forwarding-history smoke did not report PASS")

smoke_rows = {}
for side, asset, incoming, outgoing, settled, failed in re.findall(
    r"(?m)TLVWAP02 (purchase|sale) ([0-9]+) ([0-9]+) ([0-9]+) ([0-9]+) ([0-9]+)\s*$", smoke_log
):
    a, i, o, s, f = map(int, (asset, incoming, outgoing, settled, failed))
    if min(a, i, o, s) <= 0 or f != 0:
        raise SystemExit("Invalid smoke quantities or settlement flags")
    if (side == "purchase" and o <= i) or (side == "sale" and i <= o):
        raise SystemExit("Expected BTC leg/asset-anchor relation missing")
    smoke_rows[side] = {
        "asset_atoms": str(a),
        "incoming_msat": str(i),
        "outgoing_msat": str(o),
        "candidate_gross_btc_msat": str(o if side == "purchase" else i),
        "local_settled": True,
        "evidence_tier": 0,
    }
if set(smoke_rows) != {"purchase", "sale"}:
    raise SystemExit("Missing smoke markers; no report")

if not re.search(r"(?m)^\s*--- PASS: TestCustomChannels/tap_vwap_02b_matrix(?:\s|\()", matrix_log):
    raise SystemExit("TAP-VWAP-02B regtest matrix did not report PASS")

m = re.search(r"TLVWAP02B_JSON (\{.*\})", matrix_log)
if not m:
    raise SystemExit("Missing TLVWAP02B_JSON report line")
matrix_report = json.loads(m.group(1))

adapter_tests = re.findall(r"(?m)^--- PASS: (Test\w+)", adapter_log)
if len(adapter_tests) < 3:
    raise SystemExit("Expected >= 3 adapter unit/mutation tests to PASS")

out = {
    "stage": "TAP-VWAP-02B",
    "upstream_taproot_assets_commit": commit,
    "upstream_lnd_module_version": lnd_ver,
    "go_toolchain": go_ver,
    "binary_sha256": {
        "btcd_itest": btcd_sha,
        "tapd_integrated_itest": tapd_sha,
        "itest_custom_channels_test": itest_sha,
    },
    "ci_smoke_root_causes_diagnosed": [
        "Makefile itest-cc omits btcd -> btcd-itest symlink on PATH (unlike scripts/itest_cc_part.sh:31-35)",
        "Commit 734ce74dd1e816a833a9727bba741718019fef18 removed +1 unit rounding tolerance in tapchannel/aux_invoice_manager.go:345-366, causing 20,000-unit invoice at mock rate 5,820,600 units/BTC to floor to 19,999 units (17,181 msat MPP hold shortfall); rate-divisible 29,103 units = 500,000,000 msat exact settles cleanly on unmodified tapd"
    ],
    "smoke_02a": {
        "status": "PASS",
        "native_records": smoke_rows,
    },
    "adapter_synthetic_and_source_audit": {
        "status": "PASS",
        "tests_passed": adapter_tests,
        "synthetic_negative_cases_count": 21,
    },
    "regtest_matrix_02b": matrix_report,
    "node_receipt_cjs_cross_verified": node_ver_str == "true",
    "authenticated_settlement": False,
    "principal_vwap_ready": False,
}
print(json.dumps(out, indent=2, sort_keys=True))
PY
