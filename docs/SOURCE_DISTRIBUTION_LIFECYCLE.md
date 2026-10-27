# Literal distribution and trade lifecycle prefixes

This slice follows pinned JavaScript commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. It adds the actual member-call
behavior of `Logic.payToTokens`, `Logic.tradeBaiUrbun`, and
`Logic.tradeMurabaha`, plus raw transaction admission for types 26, 28 and 29.
The financial operations described later in those handlers are unreachable in
the admitted source graph. This slice does not make those operations work.

`DistributionLifecycleV5` is an explicit policy. It inherits the V4
owners and capabilities, has its own `TLCR\x05` framing and bootstrap domain,
and reports its own runtime policy name. The default stays
`GlobalClearingV2`. V1–V4 keep their prior method refusals, raw admission and
state shapes. No additional balance, position, channel or contract store is
introduced.

## Actual failure boundaries

| Call | First observed source result | Transport |
| --- | --- | --- |
| Direct pay, ordinary module globals | `ReferenceError: tallyMap is not defined` | Rejected Promise |
| Direct pay after existing genuine Tally-export binding | `TypeError: tallyMap.totalTokens is not a function` | Rejected Promise |
| Raw26 with a string payload | `TypeError: tallyMap.hasSufficientBalance is not a function` | Rejected Promise |
| Raw28 with a string payload | `TypeError: Decode.decodeTradeBaiUrbun is not a function` | Rejected Promise |
| Raw29 with a string payload | `TypeError: Decode.decodeTradeMurabaha is not a function` | Rejected Promise |
| Bai with missing/null registry | Nullish receiver dereference of `hasChannel` | Rejected Promise |
| Bai with the genuine Channels export | `TypeError: channelRegistry.hasChannel is not a function` | Rejected Promise |
| Murabaha as a member of genuine Logic | `TypeError: Cannot read properties of undefined (reading 'get')` | Synchronous throw |

Types calls `validatePayToTokens(sender, params, txid)`, while that validator
expects `(params, tallyMap)`. A successful validation verdict is not supplied.
The genuine decoder handles type26 before this wrong receiver fails. For
types28/29, a nullish or non-string payload can fail while evaluating
`encodedPayload.substr(index)`, before the missing decoder call throws. The
existing Types owner retains that argument-evaluation order.

The literal typeSwitch bridge is also retained: type26 supplies its extra
leading `params.tallyMap`; type28 supplies eight arguments to the eleven-argument
Bai handler; type29 supplies the original extra block argument. Normal raw
calls fail before any of these bridges execute.

Channels provides `isValidChannel`, not the `hasChannel` method called by Bai.
Logic imports Channels but has no `this.channelsRegistry` member for Murabaha.
Consequently no token reservation, premium transfer, Margin update or contract
record is fabricated. Later source defects, including Bai's unbound
`downPaymentPercent`/`contractsRegistry` and Murabaha's unbound `marginMap`,
remain behind their genuine earlier failures.

## Genuine Tally-instance probes and scope

The unchanged module was also called with the actual result of
`await Tally.getInstance()` installed as the lowercase global. This is a
separate Node-only receiver-domain probe, not an admitted runtime capability.
The actual constructor owns an empty instance `addresses` Map, while
`loadFromDB` loads the persisted balances into the static `Tally.addresses`.
With two funded persisted actors, the genuine instance still has no entries:
positive distribution amount throws `Insufficient tokens for distribution`;
zero amount throws `No holders for target token`.

The runtime does not copy the static balances into that instance or accept
caller-supplied functions, accessors, replacement module methods, or a fabricated
`totalTokens` result. Reachable payout/debit and rounding writes would need an
explicitly supported genuine instance/global lifecycle before they could be
ported. The source's later unbound `currentBlockHeight`, absent `save` helper,
holder debit sign and un-awaited writes are not repaired here.

## Evidence and replay

The test embeds a whole-module Node driver using the existing hash-checked
source inventory. All three Logic observers return the exact original value
or Promise, preserve `this`, and record original arguments. There is no
replacement validator, matcher, balance sufficiency verdict or financial
result. The corpus contains **146 observations**: four resets, 140 admitted
actions (136 source calls and four genuine export-binding adapters), and two
Node-only instance probes. Thus **144 observations are Rust-comparable**.

The corpus covers all four import entry orders; omitted, nullish and malformed
arguments; real Channels and Tally exports; payload argument-evaluation errors;
and a repeated raw26 failure after the genuine class binding. Handler calls
have no DB or financial effects. Tests compare exact errors and sync/async
transport, original arguments, ordered source/DB traces, all persisted rows,
shared Tally/Channels/Oracle caches, Margin allocation absence and raw parameters.
The second test checks prior-policy refusals and restores the V5 journal after
each command before continuing. Replay binds the complete owner state and
failure receipts; a source throw is not changed into a local rejection or a
successful financial transition.

The archived numeric dependency is genuine BigNumber **9.3.1**. Independently
executing the corpus with the verified genuine **9.1.2** constructor produces
byte-identical output. Its source SHA256 is
`20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e`.
The two work JSONL captures have SHA256
`2a01fa919f8269b1f6042577fafb2f573e44ca1ba1613117c407603dd8247aef`
with CRLF record separators. This is equality of the executed corpus, not a
claim that the two library versions are equivalent generally.

Core includes `source_distribution_lifecycle_impl.rs`; the new routes are gated
by V5. Integrated Rust1.90 validation is recorded in
`fixtures/reference_fidelity/source-index-distribution.json`.

The three focused tests exercise source comparisons, replay and old-policy
compatibility. The independent
V4 fixture is now pinned by the third test: Rust1.90 MSVC compiled the actual
V4 release at `c025384c8353122b3a69c4966a7f63414c3a5987`, before any V5 mount.
Its 14-command, 14,588-byte checkpoint has SHA256
`1af493eac866c75bf73ca57c275c03672e048537d44eb76c539812baff105f18`
and independently retained head
`a294259075ae5cb069be1f549fae755f02e0d6d166321ca85084e6a7454dd5ee`.
The test checks exact old receipts/effect counts, actual Vault/synthetic Maps,
option position, populated expiry indexes, saved margin deltas and future
route refusals; it then restores and continues the old policy. Repaired CRCs
and substituted destination bootstrap commitments cannot promote V4 history
to V5 or downgrade V5 history to V4.
