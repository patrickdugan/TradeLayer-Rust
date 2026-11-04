# Economic playground

An off-chain simulation of a BTC derivatives market maker and a stablecoin issuer on the real
TradeLayer Rust economic engine, compiled to WebAssembly and anchored to Bitcoin testnet4 block
heights. Nothing is broadcast and no keys exist: actors, transaction ids and heights are the
engine's unverified caller assertions, and the oracle price is whatever the page publishes.

The engine is `EconomicState` with the `ConservingV1` settlement policy in the legacy fixed-margin
mode. The playground adds sessions and a JSON surface; it changes nothing in the core crate.

## What a session is

| Piece | Choice |
| --- | --- |
| Stablecoin | `tUSD`, property 7, a **managed** token. The issuer account creates it and is the only account that can grant (mint) or redeem (burn) it. 1 tUSD = 100,000,000 atoms. |
| Derivative | Contract 1, an oracle-priced **linear** BTC/tUSD perpetual. One contract is 1 BTC (`notional`), collateral is tUSD, margin is a fixed `margin_per_contract` (default `notional * price / leverage` at the starting price, 10x). |
| Oracle | Account `oracle` creates oracle 1 and publishes its price (USD with four decimals, `price_1e4`). |
| Genesis | Block 1 activates opcodes 1, 2, 6, 11, 12, 13, 14, 15, 18. Block 2 creates the oracle, publishes the first price and applies the initial grants. Blocks you apply start at engine height 3. |

The stablecoin is an issuer-managed token (types 1, 11, 12), not the legacy synthetic mint
(types 24 and 25). The faithful source port reproduces that path's legacy throws, so it cannot
issue.

The economic config needs the collateral property in the bootstrap ledger, so genesis applies the
same ledger operation a type-1 `Issue` applies (`CreateProperty`, managed, issuer account) with an
initial treasury (default 1,000,000 tUSD) held by the issuer. Every later grant, redeem and send
is an ordinary `Joint` input. `pg_block` also accepts `issue` for further properties (ids 9 and
up).

### Why not the accepted-contract mode

The engine's accepted-contract mode (type-16 creation, leverage band, price-valued fees) re-executes
the complete input history inside every block's integrity check. Measured in release, the mean block cost
was 6 ms over the first 5 blocks and 1.3 s over blocks 45 to 49, so it cannot serve a
market-maker session. The fixed-margin
mode keeps the same ledger, order book, positions, fees and custody and checks them without
replaying history.

### Heights

The engine requires contiguous heights starting at 1. `pg_new` takes `anchor_height`, the external
height of the first block you apply (engine height 3). `pg_block` takes the external height and
rejects any value except the next one with `HeightMismatch`. The page sets the anchor to the
testnet4 tip plus one, so each new testnet4 block applies one TradeLayer block at that height.

### Margin, marks, fees and PnL

- An order reserves `qty * margin_per_contract` tUSD when it enters the book; fills move the
  reserve into position margin at the same fixed rate. There is no price band in this mode.
- `marks: false` (default): the oracle price is the reference for quotes and for
  `unrealized_pnl`; PnL is realized on the trade price when a position reduces or closes.
  `marks: true`: every oracle publication settles variation margin through the settled-basis
  policy, paying the loser's move from available balance and then margin to the winner. Marks make
  each block's integrity check replay every earlier mark, so block cost grows faster with history.
- `unrealized_pnl` is `(mark - entry) * contracts * notional` at the last oracle price. With marks
  on it overlaps `realized_pnl` by the part already settled; account `equity_atoms` (available +
  margin + order reserves) is the figure that tracks total PnL.
- Fees are the engine's 5 bps rule, collected in tUSD into fee custody. A post-only quote is the
  maker (it earns the rebate and fills at its own price); the taker pays the fee. The linear fee
  formula divides by `notional`, so `notional` 1 gives 5 bps of the fill price per contract;
  other values scale the fee by `1/notional`.
- No automatic liquidation: `maintenance_calls` (marks on) reports deficits and the engine does not
  act on them.
- Limits (engine constants): 8,192 trade receipts, 65,536 order ids and 65,536 internal transfer
  legs per session, 4,096 inputs per block. Each requoted quote costs two legs (reserve in,
  refund out); at the limit `pg_block` returns `Capacity` and a new session is needed.
- Block cost grows with session length: each block's integrity check hashes every earlier internal
  transfer leg. See the benchmark.

## Operations

All requests are `{"op": ...}` JSON through `tl_call`; responses are `{"ok":true,...}` or
`{"ok":false,"code":...,"message":...}`. Atom amounts are JSON integers up to 2^53; decimals
(quantities, prices) are strings.

### `pg_new`

`{"op":"pg_new","id"?:"s1","anchor_height"?:3,"leverage"?:10,"notional"?:"1","margin_per_contract"?:"10000","price_1e4"?:1000000000,"marks"?:false,"issuer_atoms"?:1e14,"issuer"?:"issuer","oracle"?:"oracle","admin"?:"admin","accounts"?:[{"name":"mm","tusd_atoms":N},"alice"]}`

Creates a session, applies the genesis blocks and grants each listed account its `tusd_atoms`.
Returns `session`, `next_height`, `price_1e4`, `leverage`, `margin_per_contract`, `marks`,
`notional`, `tusd_property`.

### `pg_block`

`{"op":"pg_block","session":"s1","height":H,"ops":[...]}` applies one block. Ops run in the given
order; orders are queued and matched at the end of the block in the engine's source order.

| `t` | Fields | Meaning |
| --- | --- | --- |
| `oracle` | `price_1e4` | the oracle account publishes a price |
| `grant` | `to`, `atoms`, `property`? `by`? | the property issuer mints to `to` |
| `redeem` | `atoms`, `from`? `property`? `by`? | `from` sends to the issuer (when given), the issuer burns |
| `send` | `from`, `to`, `atoms`, `property`? | token transfer |
| `issue` | `property` (9+), `atoms`, `kind`? (`managed` or `fixed`), `by`? | a new token |
| `order` | `account`, `side`, `qty`, `price`, `post`? `market`? | contract order; a market order without `price` uses the last price of an `oracle` op in the block or the last published price |
| `cancel` | `account`, `order_id` | cancel one resting order (ids come from the `ops` array of the response; an unknown id is a no-op) |
| `cancel_all` | `account` | cancel all of the account's resting orders |

Returns `height`, `engine_height`, `blocks`, `price_1e4`, `fill_count`, `fills` (price, qty,
buyer, seller, ids, fee atoms) and `ops` (one object per input, `id` for orders).

A rejected block changes nothing: the engine applies each block on a private clone and publishes
it only on success. The session keeps its height, ids and accounts, so the same block can be
retried.

### `pg_state`

`{"op":"pg_state","session":"s1","book_depth"?:200,"fills"?:20,"verify"?:false}`. Returns
`oracle`, `contract`, `accounts` (available, margin, order reserves, equity), `book` (bids and
asks with ids), `positions` (contracts, entry, margin, unrealized and realized PnL),
`net_contracts` (always 0), `open_interest`, `maintenance_calls`, `custody` (supply, available,
margin, reserves, fees, loss pool, insurance, `balanced`), `properties` and recent `fills`.
`verify: true` also runs the engine verifier and ledger conservation check.

### `pg_drop`

`{"op":"pg_drop","session":"s1"}` removes the session.

### Error codes

`BadRequest`, `UnknownOp`, `UnknownSession`, `SessionExists`, `HeightMismatch`, `InsufficientFunds`,
`NotIssuer`, `Unauthorized`, `OrderRejected` (a bad order field or a duplicate id), `InvalidInput`,
`ZeroAmount`, `AmountTooLarge`, `Replay`, `InvalidHeight`, `NotActivated`, `UnknownProperty`,
`PropertyExists`, `FixedSupply`, `OracleClosed`, `InvalidPrice`, `Arithmetic`, `Capacity`. Any
other engine error carries its variant name.

## The page

`playground/index.html` plus `app.mjs` and `engine.mjs`: plain ES modules, no build step, no CDN.

Panels: chain clock and oracle, issuer (grant and redeem tUSD), trader (deposit, market and limit
buy and sell, close position, cancel), market-maker controls, order book, positions and PnL,
balances and custody, a `pg_block` timing readout and an event log.

- **Heights.** *Live testnet4* polls `GET https://mempool.space/testnet4/api/blocks/tip/height`
  every 30 s and applies one block per new testnet4 block; your actions queue for the next one.
  *Local* heights step on a timer or fast-forward up to 5,000 blocks at once and apply every
  action immediately. Changing the mode starts a new session. If testnet4 is unreachable the page
  falls back to local heights.
- **Oracle.** A seeded random walk (seed and per-block volatility are inputs), or the USD price
  from `https://mempool.space/api/v1/prices`, polled with the tip.
- **Market maker.** JavaScript in the page. Each block it takes the new oracle price, cancels its
  stale orders and posts a symmetric post-only ladder (`levels` bids and asks, first offset
  `spread` bps, `step` bps per level, `size` contracts). Levels shrink to what its tUSD can
  reserve. It is funded by an issuer grant.
- **Failed blocks.** If the engine rejects a block the page retries without your queued inputs,
  then with the oracle price alone, and logs the engine's reason.

Run it with the built binary (no copy needed):

```
node playground/serve.mjs <path>/tradelayer_wasm.wasm 8080
```

Open `http://127.0.0.1:8080/`. `?mode=local` starts in local heights; `?offline=1` disables every
network call (local heights, random-walk oracle).

## Build, tests and checks

All commands run from `wasm/`. Use WSL on Windows: the mingw linker fails on this cdylib.

```
cargo build --locked --offline --release --target wasm32-unknown-unknown
cargo fmt --check
cargo clippy --locked --offline --all-targets -- -D warnings
cargo clippy --locked --offline --lib --target wasm32-unknown-unknown -- -D warnings
cargo test --locked --offline --test playground
```

`tests/playground.rs` covers supply conservation across issue, grant and redeem; market-maker
quotes with taker fills giving opposite positions that sum to zero; an oracle move changing
unrealized PnL and equity; atomic rejection of a bad block; typed errors for unknown sessions and
malformed input.

Browser check (headless Chrome over the DevTools protocol; local heights, no network):

```
node playground/check_browser.mjs <chrome> <path>/tradelayer_wasm.wasm [screenshot.png]
```

It fast-forwards 200 blocks, grants and redeems tUSD, opens and closes a trader position and
asserts a non-empty book, net positions of zero, balanced custody and no console errors.

## Benchmark

`examples/bench_playground.rs` (native) and `playground/bench.mjs` (Node, through the wasm) run
the same seeded schedule: per block one oracle publication (random walk, at most 0.1% per block),
an MM cancel-all and a 3-level post-only ladder on each side (6 quotes of 0.5 contract), and with
probability 0.6 one taker order (market or crossing limit, 0.1 to 0.3 contract) from one of four
traders. Each call is one `pg_block` including JSON encode and decode. The summaries carry a
`checksum` (total fills, market-maker inventory, net contracts, last price) that is identical
across the two runtimes. Add the argument `marks` to settle marks on every publication.

```
cargo run --release --example bench_playground -- 1000 1
node playground/bench.mjs <path>/tradelayer_wasm.wasm 1000 1
```

Results, 1,000 blocks, seed 1, marks off, Windows host (WSL for native, Node 24.19 for wasm),
three runs each, release builds:

| Runtime | Median ms per block | Blocks per second (median) | Wall for 1,000 blocks | First 100 blocks / last 100 (mean ms) |
| --- | --- | --- | --- | --- |
| Native (`bench_playground`) | 20.9 to 21.6 (typical 21.0) | 46 to 48 | about 21 s | 2.7 / 39 |
| Wasm in Node (`bench.mjs`) | 25.6 to 38.9 (typical 37.6) | 26 to 39 | 28 to 38 s | 3.3 / 75 |

Both runtimes produce identical checksums (596 fills, market-maker inventory 1.7, net contracts
0, last price 972083477), so the wasm runs the same engine path. The spread in the wasm runs
comes from other builds sharing the machine.

Block cost grows with session length because the engine's per-block integrity check hashes every
earlier internal transfer leg: a fresh session costs about 3 ms per block and block 1,000 about
40 ms (native). Short sessions are fast; a fast-forward of 200 blocks in the browser takes about 2 s.

With `marks` on (200 blocks, same schedule) the median is 42.8 ms native and 101.8 ms in wasm, and
the cost climbs faster because each block's check replays every earlier mark.
