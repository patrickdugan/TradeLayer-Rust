# Actual source oracle execution

The source pin is `tradelayer.js`
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The raw runner now executes source
Types 13 (create), 14 (publish), and oracle-targeted type 8 (update admin)
against the optional actual `OracleSourceOwner` inside `SourceCatalogOwners`.
The owner retains complete oracleList/oracleData documents, typed cache keys,
publication watermarks, and actual source exceptions. It owns no Ledger.

Successful commands record their full original transaction ID through
`EconomicInput::SourceMetadata`; their metadata effects live exclusively in
the actual source owner. The runner never invents or mirrors strict Registry
oracle rows. Existing source-free configurations continue to use the legacy
typed Registry. Adding the actual owner independently changes the pinned
catalog bootstrap (TLSC2); configurations without it retain TLSC1 bytes.

## Actual Logic and validity plumbing

- Type 13 passes exactly the object constructed by Logic.createOracle. Types
  decodes `whitelists`, but Logic passes `params.clearlists`; the absent field
  reaches the actual owner and becomes an empty clearlist array. Source lag
  zero, source allocation, metadata defaults, cache writes, and document
  fields remain source behavior.
- Type 14 calls the actual publishData method with decoded price/high/low/
  close and block fields. It uses the source's default-price and circuit
  breaker rules, preserves metadata, and persists the actual source record.
  A second same-height publication can return successfully without writing
  anything. That transaction retains its own metadata identity but does not
  replace the first publication's funding or mark identity.
- Oracle-targeted type 8 uses the actual updateAdmin method, including the
  source's backup/primary selector and metadata/cache changes. Admission
  reads use the actual pending owner, so later candidates see the preceding
  admitted update.
- Type 15 still follows Types' incorrect publish-validator route. The port
  does not turn the malformed close path into a functional close command.

Validity's `Oracle.*` getter calls execute genuine read methods on the
pending actual owner, retaining source cache effects. `getOraclePrice` keeps
the source fallback of `1` when no finite price row exists. No canned oracle
record, admin result, or price dependency is supplied.

## Exception prefixes and funded custody

An actual JavaScript Throw/PendingThrow can occur after metadata, cache, or
watermark changes. The oracle stage retains that exact actual-owner prefix,
returns a Thrown receipt, and records the original candidate in raw history.
It does not add a successful SourceMetadata marker or perform a funding or
mark hook. For example, a failed first publication can poison the same-block
watermark; the next source publication then skips and succeeds while no
durable price exists. A failed save can additionally leave the latest price
in the metadata cache. Complete raw-source restoration reconstructs both.

Economic and admission failures retain the candidate rollback of both
`ConservingSequentialV1` and `ConservingSequentialV2`. This differs from
retaining a source exception's metadata prefix:
no failed economic operation, funded Ledger movement, or publication hook is
committed. Capacity/unsupported-domain/unknown-dependency failures remain
explicit local errors rather than fabricated source exceptions.

## Actual prices for funded markets

The private market view gives precedence to the actual source owner, then
the private inner-codec owner, and otherwise the existing Registry.
A configured actual owner with a missing oracle never falls through to a
second Registry. Metadata comes from the actual cache/getOracleInfo behavior;
quotes come from persisted oracleData, not the metadata's possibly unpersisted
latest-data cache.

Price-row selection preserves numeric/string oracle-ID distinctions, applies
the source `Number.isFinite(Number(data.price))` filter, and chooses the latest
row with the source's `blockHeight > previous.blockHeight` comparison. The
source getter does not filter the row's `type`; neither does this view. It
returns no quote when none exists instead of manufacturing the getter's
default `1`. Finite Number prices become exact source decimal text. Decimal
String prices retain all their precision for BigNumber callers, rather than
being rounded through Number or truncated to four decimal places.

Funded height/price representations that the native exact Decimal view cannot
represent fail explicitly. Unreferenced source metadata does not undergo
those funded-market checks: a legacy nested-admin document without a ticker
can still publish normally when no configured funding/mark references it.

Actual funding and mark hooks run only for a changed source publication that
has a genuine persisted current-height quote and matches the configured
market. Tests cover missing durable data, poisoned watermarks, duplicate
publication identities, circuit limits, admin changes, real Ledger sends,
funding/mark history, and complete raw-source checkpoint replay.

`tests/economic_source_oracle.rs` exercises these full owner paths. The
`actual_market_oracle_tests` unit tests additionally check typed-ID filtering,
the source's string-height comparison, full decimal String precision, pure
reads, and the distinction between an absent funded quote and source default 1.

V2 now supplies actual selected-contract `Margin.loadMarginMap`,
`Margin.getPositionForAddress` and `Contract.getInitialMargin` reads. V1 keeps
their previous UnknownDependency results so its recorded unavailable-dependency
receipts replay unchanged. The Pending instance projection, source missing-row
shape, address lookup, accepted definition and signed Number bounds are described
in `SOURCE_CONTRACT_READS.md`; no second position store or supplied validity
verdict is introduced.

`tests/economic_source_oracle_contracts.rs` combines genuine raw13/14/16/18 and
the actual non-admin country attestation with these reads. Opposing orders reach
real funded +2/-2 positions, then actual persisted source prices drive a Paid
funding epoch at height 24 with a genuine debit/credit between the actors.
TLECv10 restoration reexecutes the complete raw history and continues through
a flip and close, preserving the paid epoch and sole-Ledger conservation.
This demonstrates the named conserving funding/settlement owner; it does not
claim literal source funding or complete-state financial parity.

The conserving contract mapping still differs from the pinned Logic dispatcher,
whose argument order assigns
`isLiq = params.reduce`, `reduce = params.post`, `post = params.stop`, and
`stop = params.block`. Owner-backed validation does not make that typed flag
policy literal source behavior.

The source also writes its own queue/active-pair documents, reserve and margin
deltas, fee cache, trade/PNL history, mutable Clearing mark caches, volume and
liquidity rewards, and closeout IOU accounting. The frozen genuine composed
source harness now records 32 scenes / 135 observations, while the standalone
queue owner has nine-method / 109-action differential coverage plus 57 genuine
enqueue boundaries. Those document/instance and source-observation results do
not integrate the entire financial write graph into the economic runner. See
`SOURCE_CONTRACT_EXECUTION_REVIEW.md`, `CONTRACT_QUEUE_SOURCE_OWNER.md` and
`CONTRACT_EXECUTION_FIDELITY.md` for the observed matches and remaining gaps.
