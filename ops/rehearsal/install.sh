#!/usr/bin/env bash
# Stage the dress rehearsal on an Ubuntu box with passwordless sudo, rustup
# (toolchain 1.90.0) and python3. Usage: install.sh <tradelayer-rust source dir>
#
# Builds tl_observe from the given tree, installs Bitcoin Core 31.1 after
# checking that at least 3 keys from bitcoin-core/guix.sigs builder-keys
# validly sign SHA256SUMS and that the tarball's hash matches it, and installs
# both systemd units. Starts nothing.
set -euo pipefail
SRC=$(cd "$1" && pwd)
KIT=$(cd "$(dirname "$0")" && pwd)
R=$HOME/rehearsal
BTC=31.1
TARBALL=bitcoin-$BTC-x86_64-linux-gnu.tar.gz
MIN_SIGNERS=3
mkdir -p "$R"/{bin,btc,btc-data,kit,observer}

source "$HOME/.cargo/env"
CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=$R/target \
  cargo +1.90.0 build --release --locked --offline --manifest-path "$SRC/Cargo.toml" --bin tl_observe
install -m 0755 "$R/target/release/tl_observe" "$R/bin/tl_observe"

dl=$(mktemp -d)
export GNUPGHOME=$dl/gnupg
mkdir -m 700 "$GNUPGHOME"
base=https://bitcoincore.org/bin/bitcoin-core-$BTC
keys=https://api.github.com/repos/bitcoin-core/guix.sigs/contents/builder-keys
(
  cd "$dl"
  curl -sSfLO "$base/SHA256SUMS"
  curl -sSfLO "$base/SHA256SUMS.asc"
  curl -sSfLO "$base/$TARBALL"
  curl -sSf "$keys" | python3 -c 'import json,sys; [print(k["download_url"]) for k in json.load(sys.stdin)]' \
    | while read -r url; do curl -sSf "$url" | gpg --batch --quiet --import 2> /dev/null || true; done
  # The keyring holds only the builder keys; count distinct valid signers.
  signers=$(gpg --batch --status-fd 1 --verify SHA256SUMS.asc SHA256SUMS 2> /dev/null \
    | awk '$2 == "VALIDSIG" {print $3}' | sort -u | wc -l)
  [ "$signers" -ge "$MIN_SIGNERS" ] || { echo "only $signers valid builder signatures" >&2; exit 1; }
  sha256sum --ignore-missing -c SHA256SUMS 2> /dev/null | grep -qx "$TARBALL: OK"
  tar -xzf "$TARBALL"
  install -m 0755 "bitcoin-$BTC/bin/bitcoind" "bitcoin-$BTC/bin/bitcoin-cli" "$R/btc/"
  echo "Bitcoin Core $BTC: $signers valid builder signatures"
)
rm -rf "$dl"

install -m 0644 "$KIT/bitcoin.conf" "$R/btc-data/bitcoin.conf"
install -m 0755 "$KIT/observe.sh" "$KIT/drills.sh" "$KIT/status.sh" "$KIT/start-long-run.sh" "$R/kit/"
sudo install -m 0644 "$KIT/rehearsal-bitcoind.service" "$KIT/rehearsal-observe.service" \
  /etc/systemd/system/
sudo systemctl daemon-reload
sha256sum "$R/bin/tl_observe" "$R/btc/bitcoind"
