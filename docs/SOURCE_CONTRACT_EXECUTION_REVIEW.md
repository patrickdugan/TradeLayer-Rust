# Composed source contract execution observations

`scripts/source_contract_execution_vectors.cjs` executes 25 complete, unchanged
archived modules from source commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Each archive has a checked SHA-256;
arithmetic uses the genuine archived BigNumber 9.3.1. The fixture contains **32
scenes and 135 observations**, including initial state. These are JavaScript
observations, not Rust parity results.

Run `node scripts/source_contract_execution_vectors.cjs --check`; regenerate
with `--write`. `--summary` prints inspected outcomes and parsed financial
state. Installed cryptographic dependencies are resolved from the pinned JS
checkout, configurable with `--module-root PATH` or `TRADELAYER_JS_ROOT`.

## Execution and observation boundary

Raw actions pass the actual body to `Types.decodePayload` with type 18, marker
`tl`, original sender, original txid and height. Its unchanged decoder and
validator run first. The driver invokes unchanged `Logic.typeSwitch` only when
the returned `params.valid` is exactly true. A separate drain action executes
`Orderbook.processQueuedOnChainOrdersForBlock`. No validation, reserve, match,
position, PnL, reward, fee or insurance result is supplied by the driver.

The same VM realm and a normal CommonJS module cache preserve circular partial
exports. The default entry follows the pinned `main.js` reachable owner import
order: Validity, Tally, MarginMap, Clearing, Property, ContractRegistry,
VolumeIndex, Oracle, Orderbook, encoder, Types, Logic. Each initial observation
records the selected entry and actual module-load sequence. Explicit Types-first,
VolumeIndex-first and Tally-first variants characterize load-order differences.
This does not execute daemon startup, Main transaction indexing or Main's entire
end-of-block driver. Unreached imported services can add startup behavior outside
this declared subgraph; it is not a whole-process startup claim.

The declared adapters are:

- Async `db.getDatabase`, async cloned in-memory document operations, deterministic
  generated document IDs, and one-shot injected storage failures. Every factory
  and operation records its exact argument list, result or throw. The rows are
  adapter state, not a claim about NeDB disk serialization, recovery or durability.
- Activation answers, fixed wall clock, deterministic UUIDs and deterministic
  noncryptographic `Math.random` for actual IOU audit identifiers. Random calls
  and UUID values are recorded. `TxUtils.getBlockCount` is an explicit RPC clock
  boundary returning the current driver height, with arguments/result recorded.
- Real installed bitcoinjs-lib, tiny-secp256k1 and bip32 dependencies; no network
  requests. Bech32, fetch, Litecoin, filesystem use and other TxUtils methods
  throw if invoked. The imported Vesting, TxIndex, Scaling, Vaults, Procedural,
  BitVM and Binohash services likewise throw if invoked. No scene reaches these
  unowned runtime boundaries. Channels, Consensus, AMM and Insurance execute
  their complete pinned sources rather than service substitutes.

Persisted inputs explicitly seed property/contract metadata, balances, country
attestations and usually a historical oracle row. Two named IOU scenes instead
import partial positions with distinct entry bases and omit oracle data; genuine
`getOraclePrice` supplies its source default 1. The reward scene explicitly
imports the extra `issuerAddress` alias and its clearlist attestation. These
inputs do not claim to have been created by a preceding raw issuance/history.

Each TSV row has seven fields: `V`, scene/action, command hex, outcome hex, state
hex, ordered effects hex, compact summary JSON. Hex fields contain diagnostic
JSON: ordinary SourceValue tags retain undefined, field order and binary64 bits;
`["bignumber", text]` and `["map", pairs]` extend that format for actual module
state. They are not all directly importable Rust SourceValue values. State
includes every adapter collection, parsed books/balances/margin maps, actual
Orderbook instances, Clearing maps, oracle watermarks, insurance instances,
Tally static balances and the fee accrual buffer. Ordered effects include actual
call boundaries and exact storage requests; source methods always execute.
Warnings/errors are retained. Four event-loop turns drain real un-awaited source
writes before observing state; no fabricated completions are introduced.

## Concrete ordinary comparison

Actors are A = `1BoatSLRHtKNngkdXEeobR76b53LETtpyT` and B =
`1BitcoinEaterAddressDontSendf59kuE`. Both begin with 1,000 collateral units.
Contract 1 has linear notional 1, leverage 10 and collateral property 7. The price
is 100, height 10; A's original txid ends in `0001`, B's in `0002`.

| Observation | A available / reserved / margin | B available / reserved / margin | Signed contracts A / B | Actual buffered fees |
| --- | --- | --- | --- | --- |
| `ordinary-unmatched/2` | 990 / 10 / 0 | 1000 / 0 / 0 | 0 / 0 | 0 |
| `ordinary-matched/3`, quantity 1 | 989.99975 / 0 / 10 | 989.99975 / 0 / 10 | +1 / -1 | 0.0005 |
| `ordinary-matched-quantity-two/3` | 979.9995 / 0 / 20 | 979.9995 / 0 / 20 | +2 / -2 | 0.001 |

Both sides pay 0.00025 at quantity 1 and 0.0005 at quantity 2. Fees remain in
Tally's real accrual buffer after placement; no feeCache row has yet been
written. Column seven exposes these buckets, actors, original IDs, decoded and
queued flags without decoding the full state. It reports buffered, persisted
value and persisted stash separately; insurance is in the primary state.

## Observed write graph and source defects

- Actual queue writes happen before reserves. Ordinary flags become
  `isLiq=false, reduce=false, post=false, stop=10`. Raw reduce shifts to isLiq,
  raw post to reduce, raw stop to post. The insurance argument is absent from
  the stored order. At unchanged price 100, the raw-reduce scene transfers 100
  between the actors and leaves A's reserve 10 with position margin zero. The
  raw-stop scene gives A a 0.00025 rebate and debits B 0.0005.
- Zero quantity really validates and queues, then placement skips it. Negative
  quantity -1 validates, reserves 10 and rests with amount -1. Two individually
  valid orders can oversubscribe one balance; the second placement is skipped
  after the first reserves it. Validation can also shrink a requested quantity.
- Matching saves the emptied/partially filled book before financial processing.
  Actual fees, reserve-to-margin transfers, signed positions, margin deltas,
  Clearing pools, PnL histories and volume/cumulative volume then execute. The
  observed seller `moveCollateralToMargin` call has 13 arguments: its position
  occupies the `flag` slot; txid and position arguments are missing. This is
  preserved, including source delta fields and missing IDs, without repair.
- Partial fills retain `initMargin` as the string `"20.00000000"`. Self-cross
  removes the buy but retains 20 reserved against one remaining sell. Real
  close and flip scenes execute; a flip leaves Tally margin 20.2 and reserve
  10.2 while the corresponding position margin is 10. These observations must
  not be normalized into a conserving balance model.
- The same-block mark scene primes Clearing at 100, actually publishes 102,
  then matches twice. The two match visits pass exits 100 and 102 respectively
  into source settlement. Clearing's mutable cache is financially observable.
- Injected history and volume write failures preserve prior book, fee, position
  and Clearing effects and leave the queue intact. Retrying the failed volume
  drain genuinely produces positions +2/-2 from the retained original orders.
  A positive notional of 1e-8 also reaches an un-injected source throw when its
  contract-unit LTC value rounds to zero, after positions/history already exist.
- Main-subgraph matching succeeds, but a subsequent fee flush throws
  `ContractRegistry.isNativeContract is not a function` from Tally's captured
  partial export; buffered fees survive. Explicit Tally-first loading executes
  the actual flush, Insurance deposit/event/snapshot, RPC clock read and stash
  write: 0.00025 goes to Insurance and 0.00025 to stash. Explicit VolumeIndex-first
  loading instead throws `VolumeIndex.getContractUnitLTCValue is not a function`
  after positions/history. The oracle does not replace either captured export.
- Imported partial-close scenes reach real IOU claims and audit writes: the
  positive case creates 0.1 claims for each actor and a -0.2 profit bucket; the
  negative case funds a +0.2 loss bucket. The imported issuerAddress scene
  executes the real un-awaited reward calls and credits property 3 by 0.00000875
  to each actor. Ordinary source-created `issuer` metadata does not satisfy that
  alias check; enhanced rewards are not demonstrated here.

Verified method references: Types `decodePayload`; decoder
`decodeTradeContractOnchain`; Validity `validateTradeContractOnchain` and
`hasReferencePrice`; Logic `typeSwitch` and `tradeContractOnchain`; Orderbook
`queueOnChainContractOrder`, `processQueuedOnChainOrdersForBlock`,
`addContractOrder`, `matchContractOrders`, `processContractMatches`,
`sourceFundsForLoss`, `calculateFee` and reward predicates; ContractRegistry
`moveCollateralToReserve`/`moveCollateralToMargin`; MarginMap `setInitialMargin`,
`updateContractBalancesWithMatch`, `settlePNL`, `realizePnl` and `reduceMargin`;
Clearing `recordTrade`/`isPriceUpdatedForBlockHeight`; Tally fee queue/flush;
VolumeIndex volume/reward methods; PnlIou claims/profit/loss; Insurance
deposit/event/snapshot. Their exact archived paths and hashes are in the script.

## Remaining coverage

The fixture does not establish global `Clearing.clearingFunction` parity,
liquidation/deleveraging/insurance payout, native-index contracts, channel queues,
cross-contract reserve cancellation, enhanced rewards, all rounding domains or
disk restart semantics. It also does not reproduce all historical transactions
behind imported inputs. Remaining unowned imports are explicit throwing
boundaries, not supplied successful business results.

An atomic, positive-quantity, reserve-refunding or replay-fenced ConservingV1
implementation has different policy from several observations above. Raw
admission into that implementation is useful coverage but cannot establish the
archived financial write graph. No Rust/shared files or Cargo checks were used
to generate these results.
