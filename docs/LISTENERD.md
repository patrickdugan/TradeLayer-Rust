# tradelayer-listenerd

`tradelayer-listenerd` serves the library's `EconomicHttpService` (the legacy
`tradelayer.js` listener routes, see `SERVICE_ROUTES.md` and `SERVICE_HTTP.md`)
on a loopback address over one restored economic store. A desktop wallet points
its `tl_listener_url` at it in place of the Node listener on `:3000`.

## Scope: `authenticated_admission:false`

The daemon serves a store exactly as restored. Live ingest from a node does not
exist: nothing applies chain blocks to the economic chain, and authenticated
admission (`ADMISSION.md`) is read-only. Every status reply and the startup line
carry `"authenticated_admission":false`. The daemon never writes to the store,
never applies a block and never derives state from a node. Heights and balances
are those of the store's last committed head; a wallet sees no new activity
until a different store is restored under a new expected head.

The store is bound to an independently supplied config and expected head, the
same discipline as `DurableEconomicChain::load`: a store restored under another
config, or whose head differs from `--expected-head`, is refused.

## Usage

```text
tradelayer-listenerd --root ABS_DIR --bind 127.0.0.1:PORT
    --expected-head SEQ:HEIGHT:HASH:COMMITMENT --config ABS_FILE
    --stop-file ABS_PATH [--timeout-ms MS]
tradelayer-listenerd --demo-seed ABS_DIR [--config ABS_FILE]
```

| Option | Meaning |
| --- | --- |
| `--root` | Absolute path of an existing store directory. |
| `--bind` | Literal loopback `IP:PORT`. Non-loopback addresses (`0.0.0.0`, `[::]`, LAN or public IPs) are refused before the store is opened. Port `0` picks a free port; the startup line reports it. |
| `--expected-head` | `SEQUENCE:HEIGHT:HASH:COMMITMENT`, the token `tl_observe` and `--demo-seed` print. |
| `--config` | Absolute path of the config file below. At most 4096 bytes. |
| `--stop-file` | Absolute path. The daemon serves until this file exists, then exits 0. A stop file that already exists at start is refused, so a stale one cannot stop a new run. |
| `--timeout-ms` | Per-connection I/O timeout, 1 to 60000, default 2000. |

Requests are served one at a time on a single thread. A reset, timed-out or
malformed connection is counted and dropped; the service continues. A store
failure (head changed, journal unhealthy) ends the daemon with exit 5.

Standard output is JSON lines. At start:

```json
{"event":"listening","addr":"127.0.0.1:3000","head":"SEQ:HEIGHT:HASH:COMMITMENT","authenticated_admission":false}
```

At stop: `{"event":"stopped","served":N,"dropped":N,"uptimeMs":N}`. Errors go to
standard error as `tradelayer-listenerd: MESSAGE`. Messages name keys, paths and
line numbers and never print config values or credentials.

| Exit | Meaning |
| --- | --- |
| 0 | Stopped through the stop file, or `--help`. |
| 2 | Usage: unknown, duplicate or missing option, non-loopback bind, relative path, malformed head token, stale stop file. |
| 3 | Config file missing, unreadable or invalid. |
| 4 | Store: wrong expected head (`StaleHead`), config mismatch, locked by another process, or a demo seed refused. |
| 5 | Runtime: cannot bind, or the service failed while running. |

## Config format

A text file of `key = value` lines (ASCII, at most 4096 bytes). Blank lines and
lines starting with `#` are ignored. All twelve keys are required exactly once.
Unknown keys, duplicate keys, missing keys, empty values and malformed values
are rejected. There are no defaults. The config names the `EconomicChainConfig`
of one fixed-margin, oracle-priced linear perpetual.

```text
format = tradelayer-listenerd-config-v1
network = bitcoin
admin = DemoAdmin
contract = 1
collateral = 7
permitted_spot_properties = 7
rollback_limit = 16
inverse = false
notional = 1
initial_margin_per_contract = 10000
fee_route = oracle:1
marking = none
```

| Key | Value |
| --- | --- |
| `format` | Exactly `tradelayer-listenerd-config-v1`. |
| `network` | `bitcoin`, `litecoin` or `dogecoin`. |
| `admin` | Registry admin account (printable ASCII, no spaces, not starting with `~`). |
| `contract` | Perpetual contract id, nonzero `u32`. |
| `collateral` | Collateral property id, nonzero `u32`. |
| `permitted_spot_properties` | `none`, or comma-separated property ids without spaces. |
| `rollback_limit` | Retained rollback frames, 1 up to the library's reorg history bound. |
| `inverse` | `true` or `false`. |
| `notional` | Positive decimal (digits and `.` only). |
| `initial_margin_per_contract` | Positive decimal (digits and `.` only): the fixed margin. |
| `fee_route` | `oracle:N`, with `N` equal to `contract`. |
| `marking` | `none`, or `oracle:N` for mark-basis marking. |

The parser is `src/listener_config.rs` (`parse_config`, `render_config`,
`parse_expected_head`, `head_token`). Funding, liquidation, AMM, accepted-contract
binding, insurance and native-entitlement modules are not expressible in this
format.

## Demo

`--demo-seed ABS_DIR` builds a new store of **synthetic demo data** through the
public economic API (`src/listener_demo.rs`, modelled on `wasm/src/playground.rs`).
Nothing in it is derived from, signed for or broadcast to any chain. The command
refuses a non-empty existing directory, so a store is never extended or
overwritten, and refuses an existing config file. It creates `ABS_DIR` when it
does not exist, writes the matching config (to `--config`, default
`ABS_DIR/listenerd.conf`, headed by a `# DEMO DATA` comment) and prints one JSON
line with `"demo":true`, the label, the paths and the `expectedHead`.

Contents (network `bitcoin`, block ids `64656d6f` ("demo") plus the height):

- Bootstrap: `tUSD` (property 7), a managed token issued by `DemoIssuer` with a
  10,000,000 unit supply, created through the same ledger operation a type-1
  `Issue` applies, because the collateral must exist before the economic config.
- Block 1: opcode activation by `DemoAdmin`.
- Block 2: oracle 1 (`BTCUSD`, run by `DemoOracle`) and its price 100000.0000;
  `Issue` of a second managed token (property 9, 1,000 units) by `DemoIssuer`;
  `Grant` of 500,000 tUSD to `DemoMM`, 100,000 tUSD to `DemoAlice` and 100 units
  of property 9 to `DemoAlice`.
- Contract 1: BTC/tUSD linear perpetual, notional 1, fixed margin 10,000 tUSD per
  contract (10x at the oracle price), fee route oracle 1, no marks, no funding.
- Block 3: `DemoMM` rests a post-only ladder: bids 99900, 99800 and 99700, asks
  100100, 100200 and 100300, one contract each.
- Block 4: `DemoAlice` buys 2 at a 100200 limit, sweeping the 100100 and 100200
  asks: two fills, one open trader position (`DemoAlice` long 2, average price
  100150, margin 20,000 tUSD) against `DemoMM` short 2. The book keeps the three
  bids and the 100300 ask.

Final head after the seed: sequence 5, height 4. Run it:

```text
tradelayer-listenerd --demo-seed C:\tl-demo
tradelayer-listenerd --root C:\tl-demo --bind 127.0.0.1:3000 ^
  --expected-head <expectedHead from the seed output> ^
  --config C:\tl-demo\listenerd.conf --stop-file C:\tl-demo-stop
curl -X POST -H "Content-Type: application/json" -d "{\"address\":\"DemoAlice\"}" ^
  http://127.0.0.1:3000/tl_getAllBalancesForAddress
type nul > C:\tl-demo-stop
```

## Endpoints

All 19 + 43 routes of the pinned `listener.js` and `walletListener.js` dispatch
(`SERVICE_ROUTES.md`). Which of them answer with data depends on what the
restored store retains. A fixed-margin store answers:

- Balances: `/getAllBalancesForAddress`, `/tl_getAllBalancesForAddress`,
  `/walletBalances/:address`.
- Properties, activations, oracles, clearlists, fees: `/listProperties`,
  `/tl_getProperty`, `/getActivations`, `/tl_getActivations`, `/listOracles`,
  `/tl_listOracles`, `/tl_listClearlists`, `/tl_getClearlistById`,
  `/tl_showClearlist`, `/tl_getAttestations`, `/tl_listFeeCache`,
  `/tl_propertyFeeCache`, `/addressesWithProperty/:propertyId`.
- Orderbooks: `/getContractOrderBook`, `/tl_getContractOrderbook`,
  `/getOrderBook`, `/tl_getOrderbook` (spot).
- Positions: `/contractPosition/:address/:contractId`,
  `/tl_contractPosition`, `/walletPositions/:address`,
  `/walletPositions?address=`, `/walletPosition/:address/:contractId`,
  `/tl_getInitMargin`.
- Channels: `/tl_getChannel`, `/tl_channelBalanceForCommiter`.
- History: `/tradeHistory/:propertyId1/:propertyId2`, `/tl_tradeHistory`.
- Status and heights: `/initStatus`, `/initMain`, `/tl_initmain`,
  `/tl_getMaxProcessedHeight`, `/tl_getMaxParsedHeight`, `/tl_getTrackHeight`,
  `/tl_checkSync`. `initMain` reports the restored head and starts nothing
  (`"daemonStarted":false`); parser and track heights are null, so
  `/tl_checkSync` reports `available:false,synced:null`.

Routes whose capability the store does not hold answer
`503 {"error":"unavailable","capability":"NAME"}`, never an empty success:

| Capability | Routes |
| --- | --- |
| `accepted_contract_catalog`, `accepted_contract_definition` | `/listContractSeries`, `/tl_listContractSeries`, `/tl_getContractInfo` (fixed-margin mode has no accepted-contract catalog) |
| `contract_owner_history`, `spot_fill_owner_history` | `/contractTradeHistory/:contractId`, `/tl_contractTradeHistory`, `/tl_contractTradeHistoryForAddress`, `/tl_tokenTradeHistoryForAddress`, `/tl_totalTradeHistoryForAddress` |
| `funding_history`, `mark_history`, `retained_oracle_observations` | `/fundingHistory/:contractId`, `/tl_fundingHistory`, `/clearingHistory/:contractId`, `/oracleHistory/:contractId`, `/tl_oracleHistory` |
| `authenticated_transaction_index` | `/tl_getTransaction`, `/tl_gettransactionsforaddress`, `/tl_gettransactionforblock` |
| `wallet_rpc` | `/tl_loadWallet`, `/tl_validateaddress`, `/tl_createrawtx_opreturn` |
| `runtime_control`, `testnet_service` | `/tl_pause`, `/tl_initmain` with `"test":true` |
| `channel_lifecycle_view`, `issuance_intent_store`, `bitvm_capability`, `synthetic_vault_store`, `authenticated_commit_broadcast` | `/tl_getChannelColumn`, `/tl_getIssuanceIntents`, `/tl_prepareCommitFromIssuance`, `/tl_bitvmStatus`, `/tl_getMaxSynth` |

### `tl_check_kyc` and `tl_check_commits`

The pinned `listener.js` and `walletListener.js` register neither name, and no
pinned source defines them. They are JSON-RPC methods of the TradeLayer-patched
Litecoin Core node, called by `tradelayer-wallet` through its node RPC
(`tl_check_kyc [address]`, `tl_check_commits [address]`); their semantics live in
that node's C++ and are not part of this repository. The current Rust state does
not back them: it records attestations by list id and per-channel balance
aggregates, but not the node's KYC verdict (which list the wallet's `enabled(kyc_0)`
reply is derived from) and not per-commit records with block and sender. Inventing
either shape would hand the wallet a made-up answer.

`POST /tl_check_kyc` and `POST /tl_check_commits` therefore parse their address
(`address` or `params`, the same rules as `/tl_getAllBalancesForAddress`) and
answer `503 {"error":"unavailable","capability":"node_kyc_attestation_rpc"}` and
`"node_channel_commit_rpc"`. Callers keep these two methods on a node RPC
connection.

## Build

```text
cargo +1.90.0 build --release --bin tradelayer-listenerd --locked
```

No external crates are used. Tests: `cargo test --test listenerd` spawns the
binary against a freshly seeded demo store.
