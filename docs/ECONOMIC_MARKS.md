# Funded settled-basis marks

`EconomicState` can enable the caller-selected `ConservingMarkBasisV1` policy
with `EconomicConfig.marking`. Contract configuration, raw notional and oracle
authority remain independently supplied assertions. Accepted actors and blocks
still use the existing unverified boundary.

The combined owner uses the Joint/Spot-owned Ledger for contract reserves,
margin, fees, collected losses and claim payments. Its exact settlement basis
is metadata; arithmetic average entry remains reporting metadata. For signed
quantity Q, signed basis C and coordinate U(P), a mark recognizes
`N * (Q * U(mark) - C)` then resets C to `Q * U(mark)`. U(P) is P for linear
contracts and the negative 20-place half-up reciprocal for inverse contracts.
Opening or adding contributes signed quantity times U(entry). Reducing removes
the exact proportional basis; a flip opens its excess at the trade price.
Consequently opening at 100, marking at 105 and closing at 110 realizes 5 + 5
for one linear contract with notional 1.

Each owner retains a canonical exact rational carry. The policy adds new PnL
to that carry, emits signed atoms by truncating toward zero at 1e8 atoms per
token, and retains the exact subatom remainder, including through flat/reopened
positions. Carry is never spendable collateral. Ratios have reduced integer
numerator and positive denominator, with at most 512 digits per integer;
zero is represented only as 0/1. Arithmetic or capacity failure rejects the
staged block.

Negative PnL first cancels the owner's unpaid positive claim. Remaining loss
collects actual available and margin custody, recording only its uncollectible
remainder as an unpaid loss. Positive PnL first offsets that owner's earlier
unpaid loss, then creates a claim for the remainder. Actual collected funds pay
claims. Gross historical default counters remain historical when liabilities
are offset. This makes a price reversal extinguish nominal default claims
without creating collateral.

An admitted bound-oracle publication creates one mark epoch using the final
accepted quote at that height, after earlier type-18 activation and after the
block's queued trades. Epoch IDs are private system IDs, separate from external
transaction IDs. Marks can record zero owners. Maintenance calls are current
deficit projections against half the configured initial-margin requirement,
rounded up; they do not execute liquidation, AMM routing or ADL.

The pinned JavaScript active mark PnL uses raw contract notional (the
`notionalPerContract` argument), 20-place reciprocal division and raw PnL. Its
separate helper rounds final PnL to eight places. The exact basis/carry policy
here is an explicit conserving repair, not historical source acceptance parity.
The source orchestration's losses/liquidations/profits stages do not make
automatic liquidation available in this owner.

# Checkpoint and replay

TLEC version 3 is canonical exactly when marking is enabled. It embeds the
combined Joint checkpoint, independently expected configuration, optional
funding state, final basis and unpaid-loss maps, the complete trade/mark action
journal, per-entry signed atoms and claim/loss offsets, and current maintenance
calls. Version 1 (no funding/marking) and version 2 (funding only) retain their
original bytes; regression fingerprints were captured from pre-extension
compiled owners. Readers validate bounds before record loops, canonical ratio
text, component structure, complete economic replay and canonical re-encoding.
Limits are 8192 events, 4096 mark epochs and 65536 aggregate basis, unpaid-loss,
maintenance and event-entry records; no pruning invents earlier basis history.

Replay recomputes basis, carry, average entry, last mark, realized atoms, IOUs,
pool and unpaid losses, binds trade events to retained trade receipts, and
reconciles exact system epochs. Known same-oracle funding observations and mark
observations must agree; captured historical funding positions must agree with
trade/mark replay even after the tip advances. Independent expected network,
admin, property set and complete economic configuration remain mandatory.

CRC and structural replay do not authenticate history. Mark-only historical
publication IDs currently resolve through an untyped processed-ID registry;
cross-checking known funding captures does not prove complete publication
provenance. A verified accepted-publication history and independently bound
contract/oracle configuration remain required gates. The sole economic owner
does not automatically integrate standalone AMM, liquidation or insurance
experiments, and separate journal namespaces have no atomic commit guarantee.

`EconomicChain` checkpoints retain complete rollback history, including exact
ratios and offsets. `DurableEconomicChain` publishes staged state only after a
synced journal commit, denies cached reads after uncertain writes, and requires
an independently expected journal head when reopening. Tests cover linear and
inverse close after restart, repaired-CRC semantic corruption, default reversal,
and durable rollback/replay on Bitcoin, Litecoin and Dogecoin profiles.
