# BitVM bundle artifact integrity

This module ports all six functions in pinned `src/bitvmBundle.js` at `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Full unchanged LF source: `fixtures/legacy_bitvm_bundle_source.txt`, SHA256 `e63a2f2e09e195a03ab2e3acb3fb6f582917c628a4c08774911c4ba6558903fe`.

`compute_bundle_hash` makes the source shallow enumerable copy, removes only root `bundleHash`, serializes with exact JavaScript field ordering/Number formatting, and hashes those bytes. Object insertion order matters; integer index keys enumerate first. Arrays and primitive JSON values retain the source object-spread behavior. Non-BMP primitive string spread is explicitly unsupported because it would create lone UTF16-surrogate values. Nested non-BMP strings in ordinary bundle objects work.

`verify_bundle_bytes` checks missing expected hash, declared hash, self-hash, then expected artifact binding. Hash normalization is ECMAScript whitespace trimming and lowercase. `BundleHashMatch` reports artifact path, matched hash, the source Number projection of selectedBucketPct and arbitrary JSON sourceHashes metadata. selectedBucketPct may be NaN/infinite; it is an observation, not a validated percentage. sourceHashes is not checked against other files. A self-consistent artifact can be manufactured by its author; the caller must independently supply the expected hash.

`verify_bundle_file` performs actual bounded file loading, then the same checks. It resolves an explicit path, opens once and reads at most 65537 bytes, rejecting artifacts above 64KiB. No hidden environment lookup or network occurs. `default_bundle_path` models source environment selection, including its original Windows fallback; callers explicitly supply an environment value. Native filesystem errors and malformed UTF8/JSON are typed diagnostics rather than claims of byte-identical Node operating-system error text.

| Source function | Rust surface |
|---|---|
| sha256Hex | Shared SHA256 artifact helper |
| normalizeHex | normalize_hex |
| defaultBundlePath | default_bundle_path |
| loadBundleFromPath | verify_bundle_file bounded read |
| computeBundleHash | compute_bundle_hash |
| verifyBundleHash | verify_bundle_bytes / verify_bundle_file |

The module also owns crate-private JSON coercion and checkpoint helpers used by the cache/risk modules. It reuses the existing ordered JSON model and exact Number formatter. Generic input documents obey its 64KiB/4096-node/depth16 limits; unsupported custom primitive-coercion fields reject explicitly. Expected hash and path labels are bounded at 4096 bytes. No external crates or unsafe code are added.

The oracle executes the full unchanged source with genuine Node crypto and explicit filesystem/path/environment adapters. Twenty-three source observations cover ordering, root-only hash deletion, integer keys, Number formatting, arrays/primitives, self-check/mismatch errors, metadata coercions and missing-file failure. Rust tests also load real temporary files, detect tampering and reject oversized files. Run `node scripts/bitvm_bundle_vectors.cjs --check`; `--source-root` independently checks the actual pinned checkout.

A matched bundle hash is artifact integrity. This source contains no BitVM proof verification, circuit execution, oracle signature verification, chain inclusion, custody or payout authorization. The adjacent upstream referee circuit has placeholder hashing; it is not invoked or replaced by a successful artifact hash result.

Restore and mutation preflight aggregate JSON nodes before journal copies; metadata budgets reserve room for encoded keys, configuration and framing. Input ordering, binary64 values and source quirks are retained inside these explicit resource limits.
