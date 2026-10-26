# Public BIP32 derivation

`src/bip32_public.rs` implements public BIP32 child derivation, strict extended
public key parsing, Base58Check and legacy P2PKH addresses without dependencies.
It reuses the existing SHA256d/HASH160 modules and secp256k1 public-input group
arithmetic. The module contains SHA512/HMAC-SHA512; it adds no seed handling,
private key derivation, signing, unsafe code, or transaction authorization.

## Public APIs

- `ExtendedPublicKey::parse(&str)` accepts standard `xpub` and `tpub` versions,
  exactly 78 payload bytes and a valid four-byte SHA256d checksum. It requires a
  canonical compressed point on secp256k1 and canonical master metadata.
- `derive_child(u32)` implements BIP32 CKDpub. Hardened indices fail. IL greater
  than or equal to n and infinity advance to the next non-hardened index. Zero
  IL is valid and retains the parent public point with the new chain code.
- `derive_child_source_compatible(u32)` reproduces the installed bitcore
  behavior: IL is reduced modulo n rather than rejected, zero IL is accepted,
  infinity advances the index, and the one-byte depth wraps.
- `p2pkh_address(AddressNetwork)` explicitly selects Bitcoin mainnet (0x00),
  Litecoin mainnet (0x30), or shared testnet (0x6f) P2PKH versions. Extended key
  versions do not distinguish Bitcoin from Litecoin.
- `derive_addresses_source_compatible(&str)` returns `Result<Vec<String>,
  Bip32Error>`, containing receive m/0/0 through m/0/19 followed by change m/1/0
  through m/1/19. It reproduces the installed Litecoin source path.
- `derive_addresses_with_source_library(&str, SourceLibrary)` also exposes the
  source's Bitcoin fallback, used only when loading Litecoin fails. An invalid
  key does not trigger fallback. Both source wrappers accept the noncanonical
  public root metadata and reduce noncanonical x coordinates modulo p, as
  bitcore does. Strict SEC1 point parsing elsewhere remains strict.

The source caller owns cache behavior: cache successful arrays by input xpub,
return an empty array for genuine malformed-public-key failures, and do not
cache failures. The source `network` argument is unused. No cache or network
argument is introduced by this crypto module.

## Fidelity and local limits

The frozen source is
`fixtures/legacy_clearlist_audit_reference.txt`, method
`deriveAddressesFromXpub`, lines 200–234. Genuine installed
`bitcore-lib-ltc` 10.10.5 and `bitcore-lib` 8.25.47 were executed. The standard
BIP32 `xpub` version yields Litecoin mainnet addresses when Litecoin loads,
Bitcoin mainnet addresses in the explicit fallback, and `tpub` yields testnet
addresses in either case. Every recorded source array was independently
crosschecked with Node/OpenSSL HMAC-SHA512, a separate Base58 implementation,
and genuine tiny-secp256k1 2.2.4 WASM public point tweaks.

There are explicit unsupported domains and local resource boundaries:

- Genuine bitcore accepts a private `xprv`/`tprv` and internally neuters it. This
  module has no private key operation. It returns `UnsupportedPrivateKey` for
  those versions, and the owner must report that as an unsupported domain,
  rather than claiming the source rejected the input. The genuine source's
  acceptance and successful caching are recorded in fixture `O` rows.
- Nonstandard/custom extended-key versions and custom runtime bitcore networks
  are unsupported. The source's dynamically mutable dependency registry is not
  a native consensus rule.
- Input length is checked before decoding. Base58 decoding uses a fixed
  82-byte array and at most 111 input bytes, with no input-sized allocation.
- Strict depth 255 cannot derive another child. Source mode wraps depth, as
  the genuine library does; source depth 254 and 255 arrays are in the corpus.
- Both modes stop after 128 invalid-child attempts and never cross into
  hardened indices. `RetryLimit` and `ChildIndexExhausted` are explicit local
  failures. The JavaScript source uses recursive retries without this bound.

The secp256k1 `public_key_tweak_add` helper accepts public SEC1 input and a
public 32-byte tweak. It rejects tweaks at least n, accepts zero, and returns
`None` for infinity. Source modulo reduction happens in the BIP32 source
mode. All point arithmetic is variable-time and is intended for public input.

## Reproducible evidence

The primary specification and published vectors are
[BIP32](https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki).
The HMAC unit test also checks
[RFC4231 test case 1](https://www.rfc-editor.org/rfc/rfc4231#section-4.2).
The fixture records package versions, source method SHA256, and SHA256 of the
actual HDPublicKey, point, network, public-key, tiny wrapper and WASM files.

Run against the genuine installed dependencies:

```powershell
node scripts/bip32_public_vectors.cjs --lib-root C:/Users/patri/Documents/Codex/tradelayer.js
```

`--write` regenerates the fixture. Without it, the complete regenerated corpus
must match the stored file byte for byte. Neither mode downloads dependencies
or substitutes cryptography. Litecoin and Bitcoin run in separate child
processes because their libraries mutate a shared cached elliptic point
prototype if loaded together. The explicit Bitcoin fallback only simulates
failure to load Litecoin; all Bitcoin cryptography comes from genuine
bitcore-lib.

The fixture contains:

| Row | Evidence | Count |
| --- | --- | ---: |
| P | All published valid public keys and their metadata | 17 |
| I | Every published BIP32 invalid extended key | 16 |
| D | Published non-hardened edges and independent child derivations | 54 |
| S | Actual source receive/change addresses, including testnet, malformed root metadata and overflowing field coordinates | 800 |
| O | Source failure/private-input and cache observations | 4 |
| C | Genuine bitcore with forced IL=0, n, n+1, max, and infinity | 10 |
| T | Genuine tiny public tweaks, zero/order/infinity boundaries | 96 |
| H | Node/OpenSSL SHA512 padding boundaries | 17 |
| M | Node/OpenSSL HMAC lengths including oversized keys | 68 |

Only the genuine library's HMAC output is injected for `C` rows, allowing
otherwise astronomically unlikely derivation branches to be observed. The
original method, elliptic multiplication/addition, retry logic, metadata, and
serialization still execute, and the original HMAC function is restored in a
`finally` block. Native unit tests replay the same cases and separately test
strict IL rejection, zero acceptance, infinity retry, retry exhaustion and
the hardened index boundary. Integration tests cover invalid points, checksum
failures, invalid Base58, huge-input rejection, all source arrays, and public
tweak differentials. This is evidence for the implemented public derivation
domain, not a claim that an external wallet or transaction is authorized.
