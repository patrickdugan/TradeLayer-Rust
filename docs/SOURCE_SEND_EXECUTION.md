# Pinned source type 2 execution

The raw-source block runner executes the existing complete `Types.decodePayload`
and `Validity.validateSend` ports against the pending `SourceCatalogOwners` and
the same `EconomicState` Ledger used by every economic operation. A private
`EconomicBlockStage::try_apply_source_send_unverified` then binds the fresh
evaluation to the original body, actor, txid and height. It admits the scalar
source send through the existing sole-owner Joint send operation, recording the
original transaction ID once. A later send in the block sees the earlier send's
actual custody. These APIs remain explicitly unverified for L1 authentication.

## Evidence

`scripts/source_send_vectors.cjs` executes complete, unchanged archived modules
from source commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`:

| Module | Archived source |
| --- | --- |
| Types | `fixtures/legacy_types_source_types.txt` |
| Decoder | `fixtures/legacy_types_source_txDecoder.txt` |
| Validity | `fixtures/legacy_validity_source.txt` |
| Property | `fixtures/reference_fidelity/property.js` |
| ClearList | `fixtures/legacy_clearlist_audit_reference.txt` |
| Tally and Logic | `fixtures/native_genesis/source/{tally,logic}.js` |

Every source has an asserted SHA256 pin. The genuine archived BigNumber 9.3.1
implementation supplies arithmetic. Explicit external services are the in-memory
database interface, activation state, UUID, fixed clock, and an offline Bitcoin
Base58Check address-format RPC projection. Unused imports are inert external
adapters. Neither the decoder nor the source validator is replaced with a test
validity response. The unchanged source's own address validation executes before
its RPC fallback. All source Tally reads and writes run against the database;
Property and ClearList reads execute their actual archived implementations.

`fixtures/source_send_vectors.tsv` records seventeen compositions, including
available balances, vesting markers, source outcomes, and the actual Tally delta
labels. `tests/economic_source_send.rs` consumes these expected results while
running the raw-source economic owner with real catalog and Ledger state. The
direct Logic cases are marked separately in the fixture: they document defects
that the present wire decoder/validator normally prevents reaching.

Regenerate and independently check the source observations with:

```text
node scripts/source_send_vectors.cjs --write
node scripts/source_send_vectors.cjs
cargo test --test economic_source_send
```

## Actual source behavior

| Input or state | Observed source behavior and economic execution |
| --- | --- |
| Scalar whole or fractional send | Actual property and clearlist reads precede the sole Ledger transfer. Source Number amounts must map exactly to atoms. |
| Scalar self-send | The original transaction ID is consumed once; custody remains unchanged after the source debit and credit. Intermediate Number precision is checked. |
| Native properties 2 and 3 | Source selects `vestingSend` by these literal IDs. The existing actual native marker owner moves available coupons and `floor(vesting * (amount / available))` whole marker tokens with the same original txid. Independently pinned native custody and policy are required. |
| Native amount below one whole token | Source validates a positive amount, then Logic throws `Error: Amount must be greater than zero` before the first Tally update. The source receipt preserves that exception. |
| `sendAll` wire body | Decoder omits `propertyIds`; actual Property lookup rejects the missing property. No send-all algorithm is substituted. |
| Four-field comma “multi-send” body | The actual decoder takes its scalar branch and uses `parseInt`: `7,8` becomes property 7 and `2,3` becomes amount 2. The owner sends that scalar result. |
| Other multi-send decoder branch | Decoder returns object entries in `propertyIds` and omits `address` and `amounts`. The actual validator throws while reading the missing address. It does not execute a repaired batch. |
| Direct Logic `sendAll` | The boolean parameter shadows the method name; source throws `TypeError: sendAll is not a function`. This is an oracle observation, not a supported raw-wire path. |
| Direct Logic multi-send | Matching nonempty arrays call unbound `processSend` and throw `ReferenceError`; mismatched arrays throw the literal length error first. These are oracle observations, not a supported raw-wire batch. |
| No balance, insufficient balance, inactive type, restricted procedural property | The actual validator rejects before execution; source evidence is retained and the Ledger does not consume the rejected txid. Outer source replay protection still remembers the candidate. |
| Clearlisted property | The actual source validator reads recipient attestations using missing `params.recipientAddress`, although Types sets `address`. That defect is retained; the owner does not silently substitute the destination. |

Source sends for IDs 2 or 3 without the actual native marker policy are explicitly
unsupported. A policy that remaps coupons to other IDs cannot make source Logic
drag their markers: those scalar sends are likewise explicitly unsupported.

## Representability and persistence

`Tally.updateBalance` performs BigNumber arithmetic and stores each bucket as a
JavaScript Number. The helper checks the pre-debit, intermediate debit and
recipient credit Number boundary before admitting a conserving integer transfer,
then checks the actual owner's post-transfer Tally projection. The fixture
`source-recipient-number-loss` demonstrates a concrete excluded state: crediting
one token to a recipient with `9007199254740992` tokens leaves that source
recipient unchanged while still debiting the sender. Rust records
`UnsupportedMapping` and retains the original custody for this nonconserving
source domain. This is an explicit boundary of the conserving economic owner;
it is not a claim that integer arithmetic reproduces that source value loss.

The actual native owner already checks the source marker formula, whole-token
rounding and Number storage boundaries. Its funded policy supplies no source
genesis or independent mint authority. Marker and coupon changes remain in the
sole Ledger and its existing native event history.

The complete outer raw-source audit, rejected candidates, original txids and
catalog owners survive TLECv10 restart, with native history embedded through the
existing native codec. Tests reopen accepted and rejected cases and repeat the
original IDs to check source replay protection. Tally database delta records and
their defective diagnostic totals are oracle evidence; this change does not
introduce a second database journal or assert database-document trace parity.
