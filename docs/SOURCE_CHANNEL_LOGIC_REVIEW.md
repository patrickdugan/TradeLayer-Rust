# Literal channel Logic integration review

`tests/source_channel_logic.rs` executes the complete unchanged pinned Logic,
Channels, Scaling, Types, Validity and supporting CommonJS modules in the same
realm. Logic is the archived source at commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`; its SHA256 is
`f09d476e581d38cf7278f1424aadba7266f25e0fbc23763ceba4afa79c5b3e1d`.
The reference loader adds the unchanged Scaling module with source SHA256
`ea2103b70dd051d39e44c9201a1eed2107b8110ac9803a0b825b92942799ca61`.

The test extends the existing recording DB boundary with the actual `$push`
operator reached by `ScalingL2.recordSettlement`. It supplies initial persisted
documents and deterministic clock/RPC boundaries. It changes no source method,
validator, channel balance, match, financial result or returned verdict.
JavaScript runs only as the independent test reference.

The runtime under test explicitly selects `SourceLifecycleV3`. Existing V1 and
V2 execution graphs and checkpoints remain separate versioned policies.

## Compared observations

The main test has 54 genuine observations: 17 independent setup states,
35 supported calls and two explicit unavailable-RPC observations. Every
supported call compares the exact return/throw and synchronous/Promise transport,
all persisted collections, the complete live Channels registry, pending
withdrawals, source Tally cache, every live Margin instance, Orderbook instances,
fee/clearing/mark caches, unhandled rejection diagnostics, and ordered
database/UUID/RNG/RPC effects. Unexpected nonempty collections fail the test.
Ordinary strings, Number bits, object/row order and serialized JSON remain exact.

Covered paths include:

- Keep-alive updates only the first matching duplicate trade; missing trades
  still record the actual keep-alive metadata.
- Close and net-settle metadata retain first-match neutralization, signed
  amounts, source directions and no-op missing documents.
- A close whose original trade includes `contractId` composes the actual
  conditional `tradeContractChannel` offset and financial settlement. Its
  source Tally collateral is supplied as persisted prestate. A separate case
  fails at the offset's Margin database factory after the close metadata has
  persisted; retry executes the real offset. No match or financial result is
  supplied. Live Margin comparisons retain the earlier collateral receiver's
  position object as the later match changes it. A further genuine failure in
  the match's first position-delta insert checks the shared live mutation
  prefix before the source exception, then retries from that actual state.
- Active `Logic.settleChannelPNL` routes each numeric enum and records through
  the actual imported `ScalingL2`. A string enum follows the source unknown-type
  return path. Null parameters throw before channel reads.
- King settlement sweeps the actual five source arrays, moves channel balances,
  persists through the live Channels owner and appends the audit record. Direct
  calls retain source negative balances, string concatenation on the receiving
  balance, and repeated payments on retry.
- A missing property throws after the sweep write. Channel-save failure and a
  late Scaling write failure preserve different real cache/storage prefixes.
  Retrying executes the source payment again. Failure during imported Scaling
  settlement recording preserves the earlier keep-alive write.
- Seven raw 23/31 calls execute actual decoder, validity and Logic dispatch.
  All four raw 23 variants and two raw 31 payments are genuinely valid; the
  third raw 31 call is genuinely rejected for insufficient payer balance.
- Synchronous `Logic.withdrawal` starts both un-awaited operations. The cold
  path preserves channel-factory, withdrawal-factory, channel-read and
  withdrawal-write ordering. A queue-write rejection leaves the pending object;
  a channel-load rejection does not suppress the independent enqueue.

The second test rebuilds five failure scenes through original replay commands,
restores after each failure and retry, and compares the complete reconstructed
continuation owner. It supplies no current owner or poststate on restore.

## Findings and capability boundary

The genuine observations identified these integration fixes owned by the root:

1. `Logic.withdrawal` must return synchronously and retain both async operation
   prefixes/interleaving; sequentially awaiting the two dependencies changes
   observable ordering and rejection behavior.
2. Valid raw 31 reaches validity's explicit uppercase `DB.getDatabase` and
   `DB.findOneAsync` capability protocol. Those calls must read the same actual
   Scaling collection used by Logic and the imported Scaling module.
3. The source null-destructuring exception names the variable `txParams` in its
   message, rather than formatting the value `null` as the variable name.
4. The collateral receiver's position and the later match position share a
   source object. Allocating a fresh nested `buyerPosition`/`sellerPosition`
   while processing the match leaves earlier live Margin receiver caches
   stale even when the final database rows match. Their explicit object
   provenance must survive the composition. The V3 adapter obtains object IDs
   from the actual Margin returns and passes explicit nested-field aliases;
   it does not infer identity from matching values or an address. Later PNL
   and save operations continue using the same arena objects. V1/V2 retain
   the earlier entry and checkpoint behavior. All transient origin slots are
   cleared at the public runtime boundaries.

The ordinary direct-transfer reference reaches unavailable
`TxUtils.getRawTransaction` inside `Channels.recordCommitToChannel`. The two
unbound transfer observations therefore assert an explicit local
`UnknownDependency` and complete Rust admission rollback. They are not counted
as supported source financial comparisons. No successful transaction-RPC answer
or participant identity is fabricated. The separately configured RPC-bound
continuation profile in `tests/source_channel_financial.rs` executes the later
live-object/Tally interleaving and compares it with the unchanged JavaScript,
without changing this default capability boundary. Its 189-observation corpus
includes raw transaction inputs, source classification/extraction, both
un-awaited Tally operations, channel persistence, failure prefixes and retries.

This review does not establish actor authorization, signed channel revisions, UTXO ownership,
complete block scheduling or historical chain acceptance.
