# Original raw3 UTXO-trade ingress

Work-only stage following the tested Main/Clearlist/Send V8 composition. It has
not been compiled or mounted in the shared checkout. Root owns integration,
Rust validation and the final local commit.

The pinned raw3 transaction is `tradeTokenForUTXO`, not NFT sending. Original
`Logic.typeSwitch` supplies thirteen arguments to the producer already owned by
the V6 liquidity-reward implementation. NFT issuance uses raw1; this work does
not add a fictitious `sendNFT` helper or change raw2 sending.

## Change

`prepare_runtime` admits raw3 only under the existing `MainCallableV8` policy and
adds its literal thirteen-field type-switch bridge in a separate include. It
reuses the original Types decoder, original Validity owner, actual Core Tally,
Channels and existing V6 producer. Older policy admission remains unchanged.
No additional ledger, transaction mapper or financial-success rule is added.

The raw3 source has a real ingress defect: Types calls
`Validity.validateTradeTokenForUTXO(sender, params, txid)` without its fourth
`outputs` argument. With matching references and a positive amount, Validity
eventually reads `outputs.length` and throws. This happens before TxUtils output
lookup. A caller with absent or mismatched references receives parameters with
no `valid` flag; zero, negative and NaN amounts return invalid earlier. These
behaviors remain visible rather than being turned into a local refusal or a
successful transfer. Normal decoded `tagWithdraw` is string/null, so the source
`Number.isInteger(tagWithdraw)` branch cannot be reached from these ASCII bytes.

## Executed source evidence

The first ten genuine calls were executed before the narrow admission fragment
was written. The final corpus executes **34 calls in 42 frames**, including
eight fresh environments. The ten-call proof remains eleven frames. Both
corpora regenerate byte-identically with genuine BigNumber 9.3.1 and 9.1.2.

The final driver executes complete unchanged pinned Types, decoder, Validity,
Logic, Tally, Property, Channels and Activation modules. Activation reads a real
36-entry registry imported as explicit source DB input. It does not receive an
activation, validity or authorization answer from the driver. Original module
source hashes are checked by the reused genuine-source loader. Constructor
`init` and live RPC are not claimed. Local imported state and actor claims remain
unverified inputs; a hash is not authentication.

Cases include strict numeric/string vout differences, missing/duplicate/null
reference rows, reversed references, default payload fields, unknown properties,
noninteger output indexes, string withdrawal tags, inactive registry rows,
channel-funded and absent Tally data, original swallowed database failures and
retries, and the direct producer's missing-channel return. All persisted tables,
live caches, actual Activation receiver, ordered effects and replay checkpoints
are compared by the staged Rust tests. Neither source nor Rust has a newly
invented raw3 successful transfer case: the original omitted argument prevents
that path.

TxUtils remains an explicit unavailable boundary in this ingress corpus. The
positive raw path throws before calling it. No synthetic RPC transaction,
pubkey verdict or supplied authorization is substituted to bypass that defect.
Separate existing V6 tests already cover the actual direct thirteen-argument
producer; those are not relabeled as raw3 ingress successes.

## Verification and integration

The final four Node commands all passed (run from any directory with
`UTXO_TRADE_REPO` set to the checkout and `TRADELAYER_JS_ROOT` set to the genuine
dependency checkout):

```text
node scripts/lower_utxo_trade_vectors.cjs --check
node scripts/lower_utxo_trade_vectors.cjs --lock --check
node scripts/lower_utxo_trade_vectors.cjs --proof10 --check
node scripts/lower_utxo_trade_vectors.cjs --proof10 --lock --check
```

Standalone Rust 1.90 rustfmt passed for the new include and tests. The three Rust
tests are **uncompiled and unrun** pending Root's coordinated build. Mount the
include, tests, script and two fixtures; apply this folder's
`prepare.prepare_runtime` after the current V8 runtime fragments. Mount this
document at `docs/SOURCE_LOWER_UTXO_TRADE.md`. The Main callable owned-kind guard
must separately admit raw3 only when this Types/Validity bridge has been mounted;
the current staged tests exercise raw execution directly, not a fabricated Main
success. No shared file, dependency manifest, push, PR or merge was modified by
this lane.
