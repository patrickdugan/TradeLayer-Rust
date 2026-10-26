# Explicit BigNumber lock reference

`node --require ./scripts/bignumber_lock_reference.cjs <generator> --check`
opts a reference check into the genuine BigNumber **9.1.2** package selected by
the pinned TradeLayer source lock. Set `TL_BIGNUMBER_LOCK_ROOT` to an npm prefix
containing that package. The preload uses Node `createRequire` from that prefix
to resolve `bignumber.js/package.json` and its CommonJS entry, checks the package
name/version and the **raw source bytes** before evaluation, and reports the
override briefly on stderr. Missing configuration, another version, or modified
source fails before a constructor is evaluated.

Only the existing exported `legacy_numeric_oracle.loadBigNumber` function is
replaced. Every call evaluates the verified source in a fresh VM and asserts its
20-place, half-up defaults. Generator entry points and `require.main` behavior
stay intact. Archived `BIG_NUMBER_VERSION = "9.3.1"`, its source hash, fixture
headers, source pins, and checked-in expected outputs remain their original
provenance. This mode does not relabel those fixtures as having been generated
with 9.1.2, nor does it replace other modules' dependencies.

## Package and source evidence

The TradeLayer commit is `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`; its
`package-lock.json` SHA-256 is
`cb7f2b040efe779e43d4aeddd56f406079c10c6083f21c990ee76c92e7110a8d`.
Its locked `bignumber.js` version is `9.1.2` and the resolved official archive is
[the npm 9.1.2 tarball](https://registry.npmjs.org/bignumber.js/-/bignumber.js-9.1.2.tgz).

| Item | Verified value |
| --- | --- |
| Lock archive SRI | `sha512-2/mKyZH9K85bzOEfhXDBFZTGd1CTs+5IHpeFQo9luiBG7hghdC851Pj2WAhb6E3R6b9tZj/XKhbg4fum+Kepug==` |
| Official tarball SHA-256 | `819bd8cc7ee880c2bb5014f45c0c4593300810de7e1fe348ef3e6bed35be149c` |
| Official tarball size | 79,226 bytes |
| Raw `bignumber.js` SHA-256 | `20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e` |
| Raw `bignumber.js` size | 89,661 bytes |

The official package source has CRLF line endings. The preload deliberately
hashes its Buffer without newline normalization. A normalized or edited copy
is not the raw package reference and is rejected. npm verifies its downloaded
archive integrity; the preload additionally checks the exact source it will
evaluate.

## Reproduce from the repository root

PowerShell, using a scratch prefix outside the checkout:

```powershell
$lockPrefix = [IO.Path]::GetFullPath("../../work/bignumber-lock-reference")
npm --prefix $lockPrefix install --save-exact bignumber.js@9.1.2
$env:TL_BIGNUMBER_LOCK_ROOT = $lockPrefix
node --require ./scripts/bignumber_lock_reference.cjs ./scripts/source_contract_execution_vectors.cjs --check
node --require ./scripts/bignumber_lock_reference.cjs ./scripts/source_send_vectors.cjs --check
node --require ./scripts/bignumber_lock_reference.cjs ./scripts/oracle_source_owner_vectors.cjs --check
```

POSIX shells can use the same `npm --prefix ../../work/bignumber-lock-reference
install --save-exact bignumber.js@9.1.2` command and export
`TL_BIGNUMBER_LOCK_ROOT=../../work/bignumber-lock-reference` before the same Node
commands. The preload is independent of this repository's absolute location.
The composed generator still needs its separately documented genuine source
module dependencies; `TL_BIGNUMBER_LOCK_ROOT` selects only this numeric loader.
Do not use fixture generation/update options for a reference comparison.

## Checked scope

The earlier controlled comparison used a work preload with this same
exported-function-only substitution on Node 24.19.0. All **31** generator/audit
`--check` invocations passed against unchanged expected fixtures or their
existing direct source assertions:

```text
source_contract_execution_vectors.cjs   source_send_vectors.cjs
oracle_source_owner_vectors.cjs         margin_map_vectors.cjs
orderbook_vectors.cjs                   contract_registry_vectors.cjs
decimal_vectors.cjs                     clearing_vectors.cjs
mark_basis_vectors.cjs                  oracle_vectors.cjs
validity_vectors.cjs                    types_vectors.cjs
amm_liquidation_vectors.cjs             audit_legacy_txdecoder_source.cjs
audit_legacy_txencoder_source.cjs        channels_vectors.cjs
contract_binding_vectors.cjs            iou_vectors.cjs
native_entitlement_vectors.cjs          native_entitlement_owner_vectors.cjs
native_genesis_vectors.cjs              property_catalog_vectors.cjs
tally_vectors.cjs                       txencoder_vectors.cjs
trade_history_vectors.cjs               vault_vectors.cjs
vesting_vectors.cjs                     wire_roundtrip_vectors.cjs
options_vectors.cjs                     volume_index_vectors.cjs
audit_property_reference.cjs
```

The checked-in portable preload was separately exercised with the three
commands above against the actual downloaded, verified 9.1.2 package. These
results establish equality only for the tested reference corpus and assertions.
They do not establish blanket equivalence between versions, alter fixture
provenance, or establish source settlement, authentication, or production
admission parity.
