# Source contract settlement

`contract_settlement_source` ports `Orderbook.processContractMatches`, its loss
sourcing, fee location/calculation, position decomposition and liquidity reward
orchestration from commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`.
The checked full `orderbook.js` archive is
`fixtures/legacy_orderbook_source.txt`, SHA-256
`c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98`.
Inspected source methods start at lines 1677, 1716, 1942, 1975, 2222, 3030 and
3174. This module owns no balances, margin maps, queues, registries or prices.

`process_contract_matches(arguments, backend)` takes the original source
arguments `[matches, currentBlockHeight, channel, last?]` and returns a
`SourceOutcome` or an explicit local `SourceRuleError`. Non-array matches become
an empty list; source self-crosses are skipped. The additional public helpers
`calculate_fee([input])` and `source_funds_for_loss(arguments, backend)` expose
the corresponding original method boundaries without economic admission.

## Backend contract

All business reads and writes use `SourceBackend`; runtime integration must
route them to the application's actual source owners. Test-recorded financial
responses are not a runtime backend or transaction authority.

| Target | Source arguments and required result |
| --- | --- |
| `Margin.loadMarginMap` | `[contractId]` returns `{sourceMarginInstance:id}`. |
| `Margin.getPositionForAddress` | Receiver handle, then `[address, contractId]`; returns the independently loaded source position. |
| `Margin.updateContractBalancesWithMatch` | Receiver, then `[match, channel, buyerClosed, flipLong, sellerClosed, flipShort, block]`; returns `{bp,sp}`. These objects now alias that receiver's cached entries. |
| Other `Margin.*` instance methods | Receiver first, followed by the exact original arguments. Used methods are `setInitialMargin`, `recordMarginMapDelta`, `settlePNL`, `realizePnl`, `reduceMargin`, and `saveMarginMap`. |
| `Contract.*` | `isInverse`, `getCollateralId`, `getInitialMargin`, `getNotionalValue`, `moveCollateralToMargin`, and `getContractInfo`, with original positional arguments. Notional returns `{notionalValue,notionalPerContract}`; margin movement returns the actual position. |
| `Tally.*` | `getTally`, the three `hasSufficient*` methods, `updateBalance` and `updateFeeCache`. Sufficiency results retain the source's `hasSufficient` field. |
| `Clearing.*` | `isPriceUpdatedForBlockHeight` returns its actual `{lastPrice,thisPrice,...}`; `recordTrade` is synchronous and returns a record containing `consumedFromOpened`. |
| `Orderbook.*` | `recordContractTrade`, `cancelExcessOrders`, and `sourceCrossContractReserve`; they must use the same live book/queue owner. Cross-reserve returns a BigNumber `remaining` and its actual `breakdown`. |
| Other owners | `TradeHistory.savePNL`; `VolumeIndex.getContractUnitLTCValue`, `saveVolumeDataById`, `baselineLiquidityReward`, `calculateLiquidityReward`; `ClearList.isAddressInClearlist`; `Channels.getCommitAddresses`; `PnlIou.addIouClaims`, `addProfit`, `addLoss`. |
| Environment | `Clock.nowISOString([])` supplies the source wall-clock ISO string. `SourceGlobal.get(["hasSufficientBalance"])` reads the actual retained undeclared global on a flip shortfall, or throws its actual ReferenceError when absent. |

The complete positional argument lists are visible in the implementation and
the independent JavaScript dependency observations. Original omissions are
intentional: buyer margin movement receives 15 source arguments; seller movement
receives 13, with its position in the flag slot and no txid/position arguments.
`recordContractTrade` receives only the two arguments supplied by this method.
History and reward calls retain their omitted block/txid arguments.

The initial position reads do not establish aliases to the current Margin
instance. Only the returned `bp` and `sp` from WithMatch establish those
identities. Subsequent `realizePnl` argument 6 and `reduceMargin` argument 0
therefore carry explicit adapter metadata on their receiver:
`sourcePositionAliases:[{argument,address}]`, with the index referring to the
original source arguments. The backend strips this metadata and binds the
passed object to that actual cached object, rather than inferring identity from
equal values.

`Margin.inspectPosition([receiver,address])` is a separate read-only adapter
projection. It refreshes only these established references after source
mutations so trade/history arguments and returned trades observe the same live
objects. It must not reload the database or substitute positions from another
instance. It is excluded from source business-effect traces and has no write
authority.

## Order, numeric semantics and failures

The method preserves fee-buffer calls before sufficiency checks; available,
reserve and margin fee priority; flip release/debit operations; original Margin
arguments; same-block Clearing records; opening mark tie-offs; trade history
before closing realization; funded/deferred payouts; volume and reward calls;
final margin save; and IOU writes. It does not repair source defects such as
shifted `isLiq` behavior, missing IDs, the unrelated global used on a flip
shortfall, missing cross-reserve remainder fields, or unguarded seller
liquidation credit. It also does not add positivity or per-unit admission gates.

Source Number arithmetic remains binary64. BigNumber operations use exact
bounded decimal arithmetic, 20-place HALF_UP division and the explicit source
rounding modes. Unary minus and `toNumber` are applied only at the actual source
conversion points. Negative zero, infinities and NaN retain their source
semantics; finite decimals outside binary64 range are not mistaken for infinity
until an actual Number conversion. The pure fee differential includes these
mixed cases and the source's even-satoshi rounding.

Awaited dependency throws end processing at that call and preserve earlier
backend effects. Unawaited `TradeHistory.savePNL` and reward credits preserve
`PendingThrow` as deferred rejections; an immediate `Throw` still propagates.
The backend must retain those asynchronous observations. This stateless module
cannot roll back writes already made by another owner. It never converts a
missing dependency into supplied successful settlement data. Captured CommonJS
partial-export failures are dependency availability outcomes, not an excuse to
replace the source method with a repaired calculation.

Local limits are four process arguments, six loss-helper arguments, one fee
input, shared bounded SourceValue trees, 16,384 dependency calls and 16 MiB of
combined dependency argument/result diagnostics. Decimal inputs longer than 512
characters, unsupported prefixed BigNumber syntax and decimal resource overflow
return an explicit unsupported numeric-domain error. These are implementation
bounds, not protocol validity conditions. SourceValue trees do not represent
arbitrary JS functions, prototypes or caller-owned alias graphs; only the
explicit live Margin aliases described above are supported. Array/Promise/
BigNumber expando mutation is outside that value domain. No local error implies
automatic rollback of an already executed backend prefix.

## Verification and scope

`tests/contract_settlement_source.rs` adds three focused tests. Its independent
Node observer loads the frozen composed driver in memory, executes unchanged
archived Types/decoder/validity/Logic and the actual financial modules, and
records top-level settlement dependency calls using AsyncLocalStorage. All
financial return values come from those genuine modules. Rust then compares
exact calls, arguments, return value and thrown prefix at that declared
dependency boundary. Read-only alias projections are separately marked adapter
observations. The alternate-entry missing VolumeIndex member is represented by
the actual observed TypeError at that attempted dependency lookup.

There are 27 composed settlement invocations across the original scenes and two
additional genuine storage/low-funds scenes. Coverage includes openings,
quantity two, partial fills, closes, flips, inverse and nonunit notionals,
mutable marks, IOU claims/profit/loss, rewards, retained storage failures/retry,
zero-rounded volume and reserve cancellation. A real unawaited history-storage
failure verifies that its rejection does not become an awaited source failure.
A separate 200-case unchanged-source fee differential covers maker flags,
channel fees, exact decimals, rounding and nonfinite/signed-zero Number bridges.
The remaining test checks empty/self-cross/malformed inputs and local limits
without inventing financial calls.

These method comparisons do not themselves establish runtime-owner, global
clearing, liquidation, channel-funded settlement or disk-recovery parity. The
module requires genuine dependency implementations and explicit environment
state; it is separate from the application's ConservingV1 policy. No Cargo or
shared integration files were changed to generate this method-level evidence.
