#!/usr/bin/env bash
# Stage the dress rehearsal on an Ubuntu box with passwordless sudo, rustup
# (toolchain 1.90.0) and python3. Usage: install.sh <tradelayer-rust source dir>
#
# Builds tl_observe from the given tree, installs Litecoin Core 0.21.5.8 after
# checking SHA256SUMS.asc against the release signers committed in the Litecoin
# repository at that tag, and installs both systemd units. Starts nothing.
set -euo pipefail
SRC=$(cd "$1" && pwd)
KIT=$(cd "$(dirname "$0")" && pwd)
R=$HOME/rehearsal
LTC=0.21.5.8
TARBALL=litecoin-$LTC-x86_64-linux-gnu.tar.gz
mkdir -p "$R"/{bin,ltc,ltc-data,kit,observer}

source "$HOME/.cargo/env"
CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=$R/target \
  cargo +1.90.0 build --release --locked --offline --manifest-path "$SRC/Cargo.toml" --bin tl_observe
install -m 0755 "$R/target/release/tl_observe" "$R/bin/tl_observe"

dl=$(mktemp -d)
export GNUPGHOME=$dl/gnupg
mkdir -m 700 "$GNUPGHOME"
base=https://github.com/litecoin-project/litecoin/releases/download/v$LTC
keys=https://raw.githubusercontent.com/litecoin-project/litecoin/v$LTC/contrib/gitian-keys
(
  cd "$dl"
  curl -sSfLO "$base/SHA256SUMS.asc"
  curl -sSfLO "$base/$TARBALL"
  for k in coblee-key davidburkett38-key losh11-key oharareidryan-key rromanchuk-key \
    theholyroger-key thrasher-key ultragtx-key void-main xinxi-wang-key zhiyan-key; do
    curl -sSf "$keys/$k.pgp" | gpg --batch --quiet --import
  done
  # The keyring holds only the repository's release keys. VALIDSIG, not GOODSIG:
  # the committed copy of the 0.21.5.8 signer's key carries a 2022 expiry.
  gpg --batch --status-fd 1 --verify SHA256SUMS.asc 2> /dev/null | grep -q '^\[GNUPG:\] VALIDSIG '
  sha256sum --ignore-missing -c SHA256SUMS.asc 2> /dev/null | grep -qx "$TARBALL: OK"
  tar -xzf "$TARBALL"
  install -m 0755 "litecoin-$LTC/bin/litecoind" "litecoin-$LTC/bin/litecoin-cli" "$R/ltc/"
)
rm -rf "$dl"

install -m 0644 "$KIT/litecoin.conf" "$R/ltc-data/litecoin.conf"
install -m 0755 "$KIT/observe.sh" "$KIT/drills.sh" "$KIT/status.sh" "$R/kit/"
sudo install -m 0644 "$KIT/rehearsal-litecoind.service" "$KIT/rehearsal-observe.service" \
  /etc/systemd/system/
sudo systemctl daemon-reload
sha256sum "$R/bin/tl_observe" "$R/ltc/litecoind"
