# Vault lifecycle and collateral interfaces

`src/vaults.rs` implements a functional, isolated vault store: create, signed
position/collateral/supply updates, redemption, synthetic-token links, collateral
reservation/release, aggregate queries, and empty-vault closure. These operations
consume caller instructions. They do not authenticate mint/redeem requests, debit
the ledger, assess collateral value, verify a contract hedge, or accept on-chain
transactions. No chain, RPC, order book, wallet or storage adapter is connected.

The source reference is `tradelayer.js` commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, `src/vaults.js`, LF-normalized
SHA-256 `3dcf0554923864729bedcfc8244a06e4235d10a3241256eb3ba0a9d3d1534983`.
The fixture generator executes that exact, unmodified source in a Node VM with
an asynchronous in-memory DB and the complete archived, hash-checked BigNumber
9.3.1 source. The latter's MIT licence is preserved in fixtures.
Property/contract services are explicit minimal mocks. The VM does not load the
legacy application or touch its databases. Its 9 lifecycle/seed operations,
13 aggregate queries and 30 source failure/quirk observations are in
`fixtures/vault_vectors.tsv`. Seeds marked `S` install source-shaped string
balances directly; they prove query arithmetic, not source `+=` update behavior.

## Accounting and invariants

`VaultId` is a nonzero collateral `PropertyId` plus a nonzero contract ID, spelled
`s-<property>-<contract>`. `SyntheticTokenId` spells that same pair `<property>-<contract>`
as the source generator does. Parsers reject noncanonical aliases, signs, leading
zeros, empty parts and overflow. These IDs identify a pair, not an authenticated
registry entry. The caller must check real property and contract definitions.

All amounts use the shared finite arbitrary-coefficient decimal, with checked
arithmetic and no float conversion. Hedge contracts may be signed; margin,
available collateral and outstanding synthetic supply remain nonnegative.
Exact updates also deliberately differ from source numeric `+=` through binary64:
for example, typed `0.1 + 0.2` is `0.3`, rather than `0.30000000000000004`.
`update_vault` adds each signed delta and auto-creates missing vaults as the source
does. `redeem` adds a signed contract delta, subtracts positive collateral releases,
and subtracts a positive redeemed amount. The source instead adds a signed
outstanding delta; fixtures translate a source `-4` redemption to typed amount `4`.
No collateral ratio or contract-price formula is inferred from those directions.

Each mutation calculates and validates a replacement vault before inserting it.
Failed updates, redemptions and collateral moves preserve the entire prior state;
a failed auto-create inserts nothing. `update_batch` commits the complete batch
only after every update passes. The store is bounded to 65,536 vaults and links.

The explicit local collateral interface moves available collateral into margin
(`Reserve`) or reverses that movement (`Release`). It conserves their exact sum
and leaves contracts/supply unchanged. It performs no external balance mutation.
Duplicate create is rejected to prevent silently resetting nonempty state.
Synthetic registration requires an existing, matching vault, explicitly stores
the missing source link, and is idempotent. Close requires all four balances zero
and removes its link; these safeguards and local movements are typed additions,
not claims of legacy behavior.

Property collateral queries sum margin plus available, then round the total once
to 8 decimal places with `ROUND_HALF_UP`, matching source BigNumber's default.
Outstanding queries sum by collateral property or return one exact vault's
outstanding; absent queries yield zero. Full stored precision survives queries.
The ordered pair map supports a bounded property range, so per-property queries
visit that property's vaults rather than scanning every collateral namespace.

## Function-level source coverage

| Source methods | Verified behavior and Rust treatment |
| --- | --- |
| `createVault` | `s-property-contract`, zero balances. Source duplicate creation resets state, and calls `saveVault` without its `vault` argument. Rust retains the ID/zero shape, rejects destructive duplicates and has no claimed DB parity. |
| `updateVault` | Adds contracts, margin, available (`grossRequired`), and outstanding (`amount`), auto-creating missing IDs. Numeric bounded vectors match. Source JavaScript `+=` on a stored string concatenates (`"10" + -2` gives `"10-2"`), and source updates accept negative collateral/supply. Rust requires exact typed decimals and rejects those negative results. |
| `updateVaultRedeem` | Adds contracts, subtracts margin/available, adds signed amount to outstanding. Source accepts overdrawn negative balances; missing source vault logs and returns `undefined`. Rust returns `NotFound`, rejects overdrafts and takes nonnegative release/redemption amounts. |
| `getVault` | Typed immutable lookup implements local retrieval. |
| `getTotalBalanceForProperty`, `getTotalOutstandingForProperty` | Source `parseInt` ID selection and BigNumber aggregate rounding covered on canonical IDs. Numeric-string property query `"1"` differs from numeric `1`, yielding zero through strict comparison. Rust uses typed property/exact vault queries; arbitrary malformed IDs, JS type coercion and NaN are not accepted. |
| `registerSyntheticToken`, `getVaultId`, `findSyntheticTokenIdByVaultId` | Source registration saves `{contract, property}` without `vaultId`; therefore its getters yield `undefined`/`null` on normal registered tokens. Rust stores a real pair-checked vault link as a deliberate functional repair. |
| `generateSyntheticTokenId`, both `parseSyntheticTokenId` definitions | Source generator returns two parts; the later async parser overrides the earlier method and requires exactly three parts, returning unchanged strings. The separate `parse_legacy_synthetic_token_id` reproduces that three-string result, including empty/nonnumeric parts. The typed ID parser accepts the canonical generated pair. |
| `findVaultIdByCompoundIdentifier` | Checks `underlyingPropertyId`/`hedgeContractId`, although created vaults carry `propertyId`/`contractId`; normal created vaults return `null`. Typed pair lookup uses `VaultId`. |
| `exists` | Missing `await` means a Promise is compared with `null`; a missing DB token reports true under the asynchronous DB API. Rust synthetic lookup checks the actual map. |
| `initializeIfNeeded`, `loadFromDatabase`, `saveVault`, `saveSyntheticToken` | Source save writes JSON in `value`, load reads `data`; vault load asks `findOneAsync` but expects an array, and synthetic load does not await its query. Actual async singleton rows fail to restore. Source create/funding save can write `value: undefined`. Reload-loss observations are tested. No production persistence or legacy DB migration is claimed. |
| `isValidPropertyId`, `isValidContractId`, `isSyntheticProperty` | The individual helpers call mocked dependencies. `isSyntheticProperty` does not await the first boolean Promise, so invalid property `999` with valid contract `2` reports true. No such source helper is used to authorize a typed vault. Real registry validation remains external. |
| `reuseVaultNumber` | No normal created vault has `isEmpty`/`isExpired`. Its fallback calls nonexistent `generateVaultId` and throws. Typed close/recreate uses the same stable pair, with an empty-state guard. |
| `sendSyntheticCurrency`, `tradeSyntheticCurrency`, `postMargin` | Direct send references undefined `TallyMap`; trade references undefined `Orderbook`; channel variants call missing `getChannel`. `postMargin` destructures an unawaited parser Promise and calls missing `isValidSyntheticTokenId`. Actual error paths are recorded; no successful transfer, trade, margin settlement or channel behavior is invented. |
| `applyPerpetualSwapFunding` | Created/updated `contracts` is a scalar. Indexing it by contract ID gives no balance, so normal funding is a no-op with no save. A manually injected object-shaped balance updates 10 to 11 at rate 0.1, then calls `saveVault` without a vault value. This contradictory shape does not define safe funding semantics, and no typed funding application is exported. |
| `rebaseSyntheticCurrency` | Does not await `findSyntheticTokenIdByVaultId`; it looks up a Promise in the synthetic map and throws `Synthetic token not found` on registered source tokens. Source tokens also have no initialized `amount`. No rebase formula is promoted into functional settlement. |

## Verification

Run from the repository root (Node must be installed):

```text
node scripts/vault_vectors.cjs --check
node scripts/vault_vectors.cjs --check --source /path/to/tradelayer.js/src/vaults.js
cargo test --frozen --test vaults
```

The Rust integration tests replay every lifecycle/seed and aggregate-query
fixture, exercise create/issue/reserve/release/redeem/close with explicit links,
and check failed-mutation atomicity, batch rollback, nonnegative balances,
identifier rejection, preserved precision, aggregate rounding and very large
integer coefficients. Checked arithmetic limit failures also preserve state,
including rollback of an earlier successful batch element. Source-error fixture rows are checked by the actual VM
oracle, not represented as successful Rust operations. Arithmetic correctness,
mocked source behavior and authenticated economic validity are separate claims.

The default oracle reads the archived pinned module and needs no separate
checkout or npm installation. The source flag verifies another checkout against
the same pin. See [decimal arithmetic](DECIMAL.md) for the shared reference loader.

Remaining integration work includes authenticated property/contract and ledger
admission, mint/redeem collateral economics, hedge establishment/closure, durable
storage/replay, real funding/rebase specification, and historical chain vectors.
Those requirements remain open; the source module is not marked fully ported.
