# Listener routes and bounded dispatch

`service_routes::dispatch` implements every active route in the pinned
`listener.js` and `walletListener.js` under the named **RepairedServiceV1**
policy. It validates each request and calls a capability through
`ServiceBackend`. It owns no Ledger, wallet, index, procedural database, or
initialization singleton. A backend error is returned to the transport; an
unavailable capability is never converted to a successful empty result.

The immutable `EconomicReadBackend` supplies facts from one verified economic
owner. Its public balance views exclude private escrow addresses and include
owned balances in available, reserve, margin, and channel buckets. Its accepted
economic height does not establish a parser or transaction-index height.
Other backends may provide controlled runtime or Core RPC capabilities. Raw
observer blocks do not authorize any route to apply economic changes.

## Complete route surface

Paths use ASCII case-insensitive static segments and one optional trailing
slash, matching the tested default Express behavior. Dynamic address values
retain their case. Query keys and body keys remain case-sensitive. The HTTP
transport decodes each path segment once; dispatch does no second decoding.
An absent POST body is represented separately from an explicit JSON `null`.
GET arguments remain in the query or path even when a GET body is supplied.

The original listener has these 19 active routes:

| Method | Path | Canonical backend target |
| --- | --- | --- |
| POST | `/initMain` | `runtime.initialize` |
| POST | `/getAllBalancesForAddress` | `ledger.balances` |
| POST | `/listProperties` | `ledger.properties` |
| POST | `/getActivations` | `registry.activations` |
| POST | `/getOrderBook` | `spot.book` |
| POST | `/getContractOrderBook` | `contract.book` |
| POST | `/listContractSeries` | `contract.list` |
| POST | `/listOracles` | `registry.oracles` |
| GET | `/addressesWithProperty/:propertyId` | `ledger.addresses_with_property` |
| GET | `/initStatus` | `runtime.status` |
| GET | `/contractTradeHistory/:contractId` | `history.contract` |
| GET | `/tradeHistory/:propertyId1/:propertyId2` | `history.spot` |
| GET | `/fundingHistory/:contractId` | `history.funding` |
| GET | `/oracleHistory/:contractId` | `history.oracle` |
| GET | `/contractPosition/:address/:contractId` | `contract.position` |
| GET | `/clearingHistory/:contractId` | `history.clearing` |
| GET | `/walletPositions/:address` | `wallet.positions` |
| GET | `/walletBalances/:address` | `wallet.balances` |
| GET | `/walletPosition/:address/:contractId` | `wallet.position` |

The wallet listener has these 43 active routes:

| Method | Path | Canonical backend target |
| --- | --- | --- |
| POST | `/tl_initmain` | `runtime.initialize` |
| POST | `/tl_validateaddress` | `node.validate_address` |
| POST | `/tl_getTransaction` | `transactions.get` |
| POST | `/tl_getChannelColumn` | `channel.predict_column` |
| POST | `/tl_getAttestations` | `registry.attestations` |
| POST | `/tl_getIssuanceIntents` | `issuance.intents` |
| POST | `/tl_prepareCommitFromIssuance` | `issuance.prepare_commit` |
| POST | `/tl_loadWallet` | `wallet.load` |
| POST | `/tl_gettransactionsforaddress` | `transactions.address` |
| POST | `/tl_gettransactionforblock` | `transactions.block` |
| POST | `/tl_getMaxProcessedHeight` | `runtime.heights` |
| POST | `/tl_getMaxParsedHeight` | `runtime.heights` |
| POST | `/tl_getTrackHeight` | `runtime.heights` |
| POST | `/tl_checkSync` | `runtime.heights` |
| POST | `/tl_pause` | `runtime.pause` |
| POST | `/tl_getAllBalancesForAddress` | `ledger.balances` |
| POST | `/tl_getChannel` | `channel.get` |
| POST | `/tl_createrawtx_opreturn` | `wallet.add_op_return` |
| POST | `/tl_getProperty` | `ledger.property` |
| POST | `/tl_listProperties` | `ledger.properties` |
| POST | `/tl_listClearlists` | `registry.clear_lists` |
| POST | `/tl_showClearlist` | `registry.clear_list` |
| POST | `/tl_getClearlistById` | `registry.clear_list` |
| POST | `/tl_listFeeCache` | `fees.all` |
| POST | `/tl_propertyFeeCache` | `fees.property` |
| POST | `/tl_bitvmStatus` | `procedural.bitvm_status` |
| POST | `/tl_getActivations` | `registry.activations` |
| POST | `/tl_getOrderbook` | `spot.book` |
| POST | `/tl_getContractOrderbook` | `contract.book` |
| POST | `/tl_listContractSeries` | `contract.info` or `contract.list` |
| POST | `/tl_listOracles` | `registry.oracles` |
| GET | `/tl_contractPosition` | `contract.position` |
| GET | `/tl_channelBalanceForCommiter` | `channel.balance_for_committer` |
| GET | `/tl_getMaxSynth` | `synthetic.max` |
| GET | `/tl_tradeHistory` | `history.spot` |
| GET | `/tl_tokenTradeHistoryForAddress` | `history.spot_for_address` |
| GET | `/tl_getInitMargin` | `contract.initial_margin` |
| GET | `/tl_getContractInfo` | `contract.info` |
| GET | `/tl_contractTradeHistory` | `history.contract` |
| GET | `/tl_contractTradeHistoryForAddress` | `history.contract_for_address` |
| GET | `/tl_totalTradeHistoryForAddress` | `history.address` |
| GET | `/tl_fundingHistory` | `history.funding` |
| GET | `/tl_oracleHistory` | `history.oracle` |

`/tl_decodeTx` is commented out in the source and is not registered. The
repaired client convenience route `/walletPositions?address=...` delegates to
the same explicit-address capability as `/walletPositions/:address`.

## Request and capability contracts

Identifiers are canonical unsigned decimal integers bounded by `u32`.
Property and contract IDs must be nonzero; clearlist IDs may be zero. Strings
such as `"1junk"`, `"01"`, `"0x10"`, fractions, negative IDs, exponent-form
JSON numbers, and nonfinite coercions are rejected before a backend call.
Addresses must satisfy the bounded ASCII `AccountId` domain and cannot start
with the private custody prefix `~`. Transaction IDs are exactly 64 hex
characters and are normalized to lowercase.

The two balance bodies differ in the source: `/getAllBalancesForAddress`
uses `address`, while `/tl_getAllBalancesForAddress` uses `params`. The latter
also accepts repaired-client `address`; if both fields are supplied they must
be valid and equal. `/tl_getChannel` still uses string `params` and
`/tl_getProperty` uses scalar ID `params`.

`channel.predict_column` receives `[channelAddress,newCommitAddress,cpAddress]`,
where only the last value may be null. `history.funding` receives
`[contractId,startBlock,endBlock]`; omitted endpoints are null. Provided heights
are bounded by the exact JavaScript integer range `2^53-1`, and a reversed
range is invalid. `contract.initial_margin` receives an ID and a positive
canonical decimal string with at most 40 coefficient digits and scale 20.

`issuance.prepare_commit` receives
`[issuanceTxid,fromAddress,channelAddress,payEnabled,clearLists]`. Booleans are
literal JSON booleans, not JavaScript truthiness. An optional clearlist array
contains at most 64 validated IDs. `autoSend:true` returns typed
`Unavailable("authenticated_commit_broadcast")` without calling a backend.
Prepared data is not a verified issuance, signature, or broadcast. Testnet
initialization is likewise an explicit unavailable capability in this
mainnet service policy.

`wallet.add_op_return` receives lowercase raw transaction hex and printable
ASCII payload. Raw hex is bounded by 128 KiB and payload by 4,096 bytes.
Collection counts are checked on borrowed JSON before cloning array elements.
The shared request envelope also bounds path, query, and JSON body size.

`procedural.bitvm_status` receives `[propertyIdOrNull,trimmedDlcRef]`.
`synthetic.max` receives `[address,canonicalPropertyKey]`, where a key is a
nonzero property ID or `s<property>-<contract>`. These capabilities require
their own actual metadata owner; the economic read backend returns typed
unavailability when it lacks one. Dispatch does not fabricate a BitVM risk
database or infer funded synthetic issuance from a short position.

The height backend returns `maxProcessedHeight`, `maxParsedHeight`, and
`trackHeight` as unsigned integers or null. Unknown parser or tracking facts
remain null. `tl_checkSync` returns `available:false,synced:null` unless all
three are known. `tl_getTrackHeight` may expose the accepted economic height
as a fallback only with explicit `source:"maxProcessed"`; this never makes
the sync check successful. The route status delegates to the runtime instead
of inventing a local initialization flag.

## Source behavior and named repairs

The source oracle preserves these concrete behaviors rather than presenting
them as repaired runtime guarantees:

- The original listener launches `initialize()` without awaiting it and its
  initialization flag remains false even after completion. The wallet
  listener sets its initializing flag after awaits, allowing two concurrent
  initialization calls. It also runs a hardcoded history query before its
  error handler, and wallet loading does not await `TxUtils.load()`.
- Wallet initialization throttles `req.wallet`, not `req.body.wallet`.
  Its initial timestamp rejects a flagged call within the first 30 seconds.
  `waitForClientWrapper` checks only a truthy `chain`, catches readiness
  errors, and sleeps after its last failed attempt too. Rust startup and
  controlled readiness belong to the concrete runtime backend.
- `ContractsRegistry`, `Oracles`, `clearingHistory`, and `WalletCache` are
  undefined on several original routes; the wallet oracle-history route also
  references undefined `Oracles`. Rust routes delegate to explicit owned
  history or wallet capabilities instead.
- `/tl_propertyFeeCache` reads `req.id`, which Express does not populate from
  the body. The repaired route reads `body.propertyId`, falling back to
  `body.id`. `/tl_listContractSeries` preserves a supplied contract lookup
  and lists actual contracts when no contract ID is supplied.
- Source `parseInt`, `Number`, and `isNaN` checks allow values outside valid
  identifier domains. The strict finite integer policy above is intentional.
  Original wallet `params` bodies are valid source shapes, not defects.

The BitVM source route is executed with an explicit procedural database
boundary. Its pending/challenged counters ignore status case; its expiring
count includes overdue positive deadlines; deadline-sorted targets are capped
at 50. Global risk and window risk ignore the DLC filter, while per-DLC pending
uses a maximum rather than summing rows. Window length uses a floored
environment value bounded below by one. Feature flags require exact string
`"1"`, the commitment scheme is normalized separately, and last watchtower
tick is hardcoded zero. Those are source observations, not claims about the
availability or correctness of a Rust procedural owner.

The max-synth source path selects inverse-native contracts by notional
property, considers only negative positions, and falls back from a positive
last mark to a token quote. It uses derived `.notionalValue` and computes
`short * notional` without proving margin or issuance funding. Its permissive
property-key coercions are characterized separately. Its `parseValue` helper
is defined but unused by that route.

Public helper ports cover strict `parse_property_key`, safe-range
`parse_window_index`, finite ASCII `env_num` with an explicit environment
snapshot, and bounded `parse_value` nullish selection. `parse_value` attempts
source-compatible JSON parsing of strings, preserves syntactically invalid
strings, and returns explicit resource or unsupported-number errors. Generic
JSON number rendering reuses the differential-tested shared source formatter.
The asynchronous readiness helper remains runtime capability behavior and is
fully characterized in the source oracle, rather than adding an uncontrolled
sleep loop to dispatch.

## Exact-source evidence

Archives are complete Git blobs from
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, with original LF bytes:

| Archive | SHA-256 |
| --- | --- |
| `legacy_listener_source.txt` | `25bcbe7521e58ad9ae52d624fe9e937f079abca5e8c2e9ff9e9f73667ef2e146` |
| `legacy_wallet_listener_source.txt` | `d97a2997d70d2df8708d3fa9024c4f7b87dadf79921d876771181cd88837d3b8` |

`node scripts/service_routes_vectors.cjs --check` executes both complete,
unchanged files inside Node VM contexts and compares 134 observations,
including all 62 active handlers and all five helper functions. Express
registration, clock, database, singleton, and other dependencies are explicit
recording mocks. The fixture records source responses, exceptions, and actual
dependency argument shapes; mocked results do not prove economic behavior.

An optional genuine Express probe uses the same complete files, actual
Express routing/JSON middleware, and Node `IncomingMessage`/`ServerResponse`
over in-memory streams, without network or wallet I/O:

```text
node scripts/service_routes_vectors.cjs --check --express <installed-express-directory>
```

Express 4.22.3 was checked for static case folding, optional trailing slash,
source path coercion, case-sensitive query keys, and wallet balance `params`.
This is routing evidence for that installed version, not a claim that all
Express configurations or future versions behave identically.

The integration suite exercises every baseline route against its canonical
backend target, invalid inputs before backend calls, absent bodies, capability
errors, commit restrictions, bounded lists, explicit wallet address repair,
unknown heights, and source helper vectors. The parent build runs the Rust
test and lint gates together with the real HTTP/runtime integration.
