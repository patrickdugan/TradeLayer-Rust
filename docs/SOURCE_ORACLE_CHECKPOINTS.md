# Restoring economic owners with actual source oracles

The existing `TLEC` version 10 catalog wrapper retains four length-delimited
sections: the complete embedded economic owner, original source audit, initial
catalog owners, and current catalog owners. Its header binds the independently
expected Joint bootstrap commitment, source policy and catalog bootstrap
commitment. Actual oracle ownership uses the optional third owner in `TLSC`
version 2. Catalogs without an oracle owner retain canonical `TLSC` version 1
bytes and their existing version 10 restoration behavior.

Actual source oracle operations do not create a parallel strict Registry oracle.
The source audit records their original transactions, while native/contract
typed input frames retain their original IDs as metadata inputs. Those typed
frames cannot independently reconstruct publication/cache changes or the funded
effects that read actual source oracle documents. A bare inner snapshot is
therefore insufficient evidence for restoring this complete owner. Native-v8
contains its bootstrap and typed input frames rather than an independent final
economic-state image: two different actual oracle prices can have identical
native inner bytes when their accepted metadata IDs and funded inputs match.

## Independent restore

When the pinned initial catalog contains an actual oracle owner, decoding:

1. Parses and validates all four sections, the outer CRC and the independently
   supplied deployment pins. Both catalog sections must have valid canonical
   complete-owner encodings. The original source audit must parse completely.
2. Decodes the Joint bootstrap from the source audit using the caller's expected
   network, administrator, permitted properties and native entitlement policy.
   Its independently computed source bootstrap commitment must match the
   deployment pin.
3. Constructs a pristine `EconomicState` with that Joint bootstrap and the
   independently pinned initial property, clearlist and oracle owners.
4. Replays every original source frame through
   `consensus_source::execute_block_unverified`. Real Types decoding and source
   validation read the pending actual owners. Each generated source frame,
   including receipts, must equal the saved original frame.
5. Clears replay/private context flags, compares the complete source audit and
   final catalog owners, and compares the entire canonical version 10 snapshot
   byte-for-byte. This includes embedded funded Joint state, all books, funding
   records, native/contract typed histories, oracle documents, cache and
   watermarks.

Supplied current catalogs and embedded owner bytes are comparison targets. They
do not seed the reconstructed state. A supplied receipt or valid flag never
replaces execution of its original transaction. Raw rejected and duplicate
candidates remain part of the audited history even when they leave balances
unchanged.

## Private inner encoding context

Encoding first verifies the complete outer economic owner through its full
source replay. Only then does it strip the source layer from a clone for the
existing embedded codec. That clone receives a private actual-oracle view and
a private flag stating that full source replay was verified. Structural and
economic checks continue against the full real funded state; only the
insufficient independent native/contract historical replay is skipped in that
specific context. Generic metadata IDs remain bound to the outer original
audit as before.

These context fields are never serialized, accepted as caller inputs, or left
on the returned state. Public legacy decoding starts with no oracle context and
no verified-source flag. The outer restore reconstructs an ordinary complete
economic owner and clears its replay and temporary context fields before
returning it.

## Adversarial coverage

`tests/economic_source_catalog_oracle_snapshot.rs` uses original raw source
transactions with actual source decoding, validity, metadata owners and the
sole funded Ledger. Generic and native scenes include source oracle creation
and publication, real token sends, rejected/duplicate audit records, actual
funding observations and a due funding epoch with no open positions. The native
scene additionally retains source coupon sends and coupled native marker effects. Roundtrips retain
the complete owner and produce the same result for the next source block.

Tests recompute the outer CRC while attempting to splice another valid
economic owner, source audit or current catalogs; inject canonically encoded
oracle metadata or price changes; delete price history or oracle ownership;
strip or modify rejected source transactions, receipts or the source layer;
and replace deployment pins, administrator or permitted-property policy.
Canonical owner encoding, hashing and CRC do not by themselves authorize any
of these substitutions.

The inner section is rejected when decoded with the original source-bound
deployment configuration. Deliberately supplying a different source-free
configuration can let a native-v8 container reconstruct a different legacy
owner: the typed metadata IDs remain, while actual oracle observations and
catalogs do not. Tests distinguish that separate configuration and result from
restoring the original source-bound owner. Splice tests use genuinely different
funded token sends so that their native inner sections also differ.

The source runner and all raw actors remain explicitly unverified at the L1
boundary. These checks establish deterministic restoration from independently
expected bootstraps and complete replay; they do not authenticate transaction
senders or supply absent insurance/proof-verification behavior.
