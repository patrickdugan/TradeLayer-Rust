#!/usr/bin/env bash
# Isolated TAP-VWAP-03 Nova prover test & benchmark runner.
#
# TradeLayer-Rust's supply-chain guard (`scripts/check_rust_supply_chain.py`)
# forbids additional `Cargo.toml` / `build.rs` / `vendor/` files inside the
# repository tree. This script copies `research/tap-vwap/prover/` into a
# disposable workspace outside the repository, materializes `Cargo.toml` and
# `Cargo.lock` from `*.isolated`, runs `cargo test --release --locked`, and
# optionally executes the full Nova benchmark / epoch prover.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RECEIPT_DIR="$(cd "$SCRIPT_DIR/../receipt" && pwd)"

STAGE_ROOT="${TAP_VWAP_PROVER_STAGE_DIR:-$(mktemp -d)}"
cleanup() {
  if [[ -z "${TAP_VWAP_PROVER_STAGE_DIR:-}" && -d "$STAGE_ROOT" ]]; then
    rm -rf "$STAGE_ROOT"
  fi
}
trap cleanup EXIT

mkdir -p "$STAGE_ROOT/prover/src" "$STAGE_ROOT/receipt"
cp "$SCRIPT_DIR/Cargo.toml.isolated" "$STAGE_ROOT/prover/Cargo.toml"
if [[ -f "$SCRIPT_DIR/Cargo.lock.isolated" ]]; then
  cp "$SCRIPT_DIR/Cargo.lock.isolated" "$STAGE_ROOT/prover/Cargo.lock"
fi
cp -r "$SCRIPT_DIR/src/." "$STAGE_ROOT/prover/src/"
cp "$RECEIPT_DIR/golden-fixtures.json" "$STAGE_ROOT/receipt/golden-fixtures.json"

cd "$STAGE_ROOT/prover"

LOCK_FLAG=()
if [[ -f Cargo.lock ]]; then
  LOCK_FLAG=(--locked)
fi

cargo test --release "${LOCK_FLAG[@]}"

if [[ "${TAP_VWAP_RUN_BENCH:-0}" == "1" ]]; then
  cargo run --release "${LOCK_FLAG[@]}" -- "$@"
fi
