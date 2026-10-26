# Funded contract AMM and LP receipt checkpoints

`EconomicConfig.amm` enables `ConservingFundedContractAmmV1` for the combined
owner's configured contract and collateral. It requires marking and liquidation.
The pool uses an exact derived private `~econ:` counterparty and the existing
Joint-owned Ledger. External actors and public book orders cannot impersonate
the pool. Public account validators retain their existing reserved-name fences.

LP units are nontransferable receipt ownership metadata. A deposit transfers
actual collateral to pool cash in the staged owner; it does not mint an LP
property or create a second balance store. Capital operations require a flat
pool, no quotes, no positive IOU and no unpaid loss. At the start of each block,
old funded quotes are cancelled and their actual reserves return to cash.

With cash C and total units S, the first deposit D into an empty pool mints D
units. Later deposits mint `floor(D*S/C)`. A redemption of r units pays
`floor(r*C/S)`; the last burn pays all cash. Zero outputs, minimum-output
failures, foreign units and insufficient supply reject the complete staged
block. Basis carry remains exact metadata even at zero units; it is not cash
and cannot be reset by withdrawing and redepositing. Actual default or unpaid
profit liabilities continue to restrict capital operations.

Fresh admitted marks generate private quote epochs, including empty outcomes.
Quotes use the existing pinned `amm.rs` native-Number/toFixed(8) price bridge.
Quantity, inventory limits and real reserve funding are separate exact checks.
Each side receives half the available cash budget and reserves actual rounded
initial margin. Quote generation pauses when the existing pool position lacks
full initial margin or has unpaid loss, even when maintenance is met, so cash
needed for existing risk cannot fund new quotes. AMM fills precede firm-book liquidation fills and share the
configured per-owner fill cap. Both sides use the named directional guard:
sells at or above their floor, buys at or below their ceiling, including inverse
contracts. This deliberately repairs the pinned source's reversed inverse
predicate. Public virtual `amm` fee exemptions or unfunded quote promises are
not used; the pool receives the actual funded maker rebate.

The source reference remains complete unmodified `amm.js` at
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, LF SHA256
`bfb4f5d94c1d4754ae9a17b646c42c4ffe62e077590d0a184ac8ddc0b38255b1`.
Its nominal capital shares, overloaded inventory field, broken static receiver
paths and virtual-account behavior remain characterized in `AMM.md`. Funded
receipt shares and custody here are an explicit repair policy, with no claim
of historical AMM acceptance parity. Token pairs, options strategies, ordinary
standing AMM orders and transferable LP token issuance remain separate gates.

# Versioned replay and recovery

TLEC version 5 is canonical when AMM configuration is present and accepted-contract
binding is absent. Bound contracts use [version 6](ECONOMIC_CONTRACTS.md), which
adds source price contexts and complete dependency replay. Versions
1–4 retain their bytes for existing configurations. Tests retain genuine
pre-extension fingerprints, including an active version-4 forced close with
owned-order cancellation and five complete book frames, captured from the
compiled `cfdd8ec` owner before AMM fields were added.

Version 5 persists independently expected AMM configuration, LP supply and
owner units, exact pool cash projection, quote generation and active quotes,
every per-height capital input and receipt, released quotes, quote epochs and
cash legs. An epoch records the exact cash-leg prefix boundary used to compute
its pre-quote cash. Each liquidation additionally records private AMM fills and
cancelled pool quotes. `AmmForced` basis events link exact liquidation epoch,
fill index, pool and distressed owner; earlier versions explicitly reject this
tag. Pool positions, margin, basis, carry, claims and defaults remain in the
existing combined economic structures.

Readers bound counts before record loops and enforce canonical maps, flags,
decimals and ratio encodings. Limits are 4096 LP owners, 4096 frames, two quotes
per quote vector, and 65536 composed AMM records across shares, active/released
quotes, inputs, receipts, epochs, epoch quotes and cash legs. Liquidation's
existing composed budget also charges AMM fills and cancelled pool quotes;
combined AMM/book fills are bounded to 64. Existing complete-book frame and
checkpoint byte limits still apply. Hitting a limit rejects the transition;
history is not silently discarded.

Replay recomputes LP outcomes, cash flow, quote reserve/refund legs, pool
positions and margins, AMM-first priority and remaining firm-book execution.
CRC repair alone cannot authorize a changed share supply, price, pool identity,
cash observation, epoch boundary or input frame. CRC and replay do not
authenticate actors, contract configuration, oracle publications or a complete
external wallet history. Independent network, admin, property policy, full
economic configuration and durable expected head remain required inputs.

`EconomicChain` retains the complete AMM owner alongside its rollback history.
`DurableEconomicChain` commits the staged sole-Ledger checkpoint before memory
publication, preserves poisoned-read behavior after uncertain writes, and
checks the independently expected head when reopening. Snapshot and durable
tests cover quote release, rollback and replay on Bitcoin, Litecoin and
Dogecoin. Separate namespace writes provide no cross-owner atomicity.
