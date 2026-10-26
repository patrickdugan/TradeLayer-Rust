# Source spot execution and the conserving owner

The pinned source is `tradelayer.js` commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, specifically `src/types.js`,
`src/txDecoder.js`, `src/validity.js`, `src/logic.js`, and `src/orderbook.js`.
The Rust runner evaluates actual Types and validity against owned property and
clearlist catalogs and the sole funded Joint/Economic Ledger. Raw sender,
transaction, and block inputs remain unverified.

`EconomicSourceReadV1` now implements both actual orderbook dependencies:

- `Orderbook.getOrderbookInstance(key)` resolves asynchronously to an instance
  projection containing the literal key and its actual settled buy/sell rows.
- `Orderbook.getOrdersForAddress(address, contractId, offeredId, desiredId)`
  resolves asynchronously to actual matching sender rows, with buys before
  sells. A truthy contract ID selects its literal string key. Otherwise the
  key is the literal `offeredId-desiredId`; this query does not normalize a
  reversed pair. Queued orders are absent, matching source database reads.

Unknown keys and absent addresses yield real empty books/results. Queries do
not create orders or balances. The token result's display `txid` uses the
source's first-three/last-four abbreviation; all reserve, fill, replay, and
checkpoint identities retain the original 32 bytes. Token `amountExpected`
after a partial fill is derived from actual quote legs because the conserving
book only needs the seller's remaining offered amount. A residual expected
amount outside the backend's nonnegative exact numeric domain fails closed.

`Tally.getAddressBalances` iterates properties with actual owner custody,
returns string property IDs and the actual catalog ticker, and exposes the
source amount/available/reserved/margin/vesting/channel row shape. It uses the
catalog's genuine string-key lookup/cache behavior. An unknown account returns
`[]`. If a funded property is absent from the catalog, the source's uncaught
`info.ticker` dereference remains an asynchronous TypeError. Source zero rows
that have no representation in the conserving Ledger are not manufactured.

The direct backend factory checks a source-enabled owner's stored parent
height and block ID. A source-free EconomicState retains its finalized height
but has no standalone block ID; callers needing that binding use the existing
EconomicChain or durable-chain constructor.

## Source defects retained in validation

Types adds `senderAddress` for type 6. It does not add `fromAddress` or
`contractId`, although cancellation validity uses those fields. Consequently
the actual order query receives `undefined`; the backend preserves it.
Moreover, `getOrdersForAddress` is async and validity does not await it. Its
`Promise.length` is undefined, so an empty resolved result does not trigger
the source's `length === 0` rejection. Returning an immediate mock array would
change this source behavior.

The token cancellation decoder derives `cancelParams.price` from the
`cancelAll` field divided by eight, rather than the transmitted price field.
Thus `cancelAll=1` also produces price `0.125`. Missing side defaults to the
string `"0"`, which source validity rejects. A transmitted transaction-ID
selector is rejected as deprecated. These decoder/validator outputs remain
unchanged.

## Named conserving settlement differences

`ConservingSequentialV1` maps admitted raw forms onto the existing
`ConservingV1` owner. This is an explicit repaired settlement policy, not a
claim of literal JavaScript matcher parity:

- Type 5 retains complete original IDs, reserves the actual offered asset, and
  uses the existing integer quote/base rational matcher and exact Ledger legs.
  The source calculates offered/expected on both sides despite opposite units,
  truncates token order IDs, and has additional fee/volume/reward behavior.
- Raw type-6 `cancelAll=1` maps to owned, directional cancellation in the
  conserving book. It does not also execute the decoder's accidental price
  branch. The source's `shouldCancelOrder` address criterion can match both
  directions; its price criterion does not enforce sender ownership, and its
  refund loop always uses the requested offered property. Those source
  mutation defects are not adopted by the sole conserving Ledger.
- The owner rejects cancellation with no actual matching owned order even
  when the source's unawaited validator query accepts it. Stop/post orders
  fail with the actual owner's `UnsupportedFlag`. Price-only cancellation
  remains `UnsupportedMapping`; transaction-ID raw cancellation remains
  source-invalid. No fabricated dependency result bypasses these limits.

`tests/economic_source_spot.rs` exercises real funded catalogs and raw Types
5/6: reserve, partial fill, full original fill IDs, cancellation/refund,
literal-key and missing-address reads, async quirks, decoder failures,
unsupported forms, typed owner checkpoints, and complete raw-source/catalog
checkpoint replay. `tests/economic_validity.rs` also checks actual funded
contract-order instance/address reads and immutable owner state.
