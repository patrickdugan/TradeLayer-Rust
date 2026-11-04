#!/usr/bin/env bash
# Builds the release wasm module inside WSL and copies it to an output directory.
#
#   bash wasm/scripts/build-wasm.sh /mnt/c/path/to/out
#
# The output directory must be an absolute POSIX path such as /mnt/c/Users/me/out. A Windows
# path like C:/out is relative inside WSL and would create a directory inside the crate, so it
# is refused. Environment: TL_TARGET_DIR (cargo target dir, default $HOME/work/tl-wasm-build),
# TL_TOOLCHAIN (default 1.90.0).
set -euo pipefail

out="${1:?usage: build-wasm.sh /mnt/c/absolute/out/dir}"
case "$out" in
  /*) ;;
  *) echo "error: out dir must be an absolute POSIX path (/mnt/c/...), got: $out" >&2; exit 2 ;;
esac

export PATH="$HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin:$PATH"
crate="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${TL_TARGET_DIR:-$HOME/work/tl-wasm-build}"

cd "$crate"
cargo "+${TL_TOOLCHAIN:-1.90.0}" build --release --lib --target wasm32-unknown-unknown --locked --offline

mkdir -p "$out"
cp "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/tradelayer_wasm.wasm" "$out/tradelayer_wasm.wasm"
ls -l "$out/tradelayer_wasm.wasm"
