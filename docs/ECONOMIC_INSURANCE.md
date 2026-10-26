# Funded oracle-perpetual backstop in the economic owner

`ConservingOracleBackstopV1` is an opt-in conserving policy over the actual accepted-contract `EconomicState`. It shares that owner's Joint Ledger, positions, exact settled basis, outstanding IOUs, book, AMM, fees, checkpoints and rollback history. It does not import `insurance.js` balances, create an insurance token, mint collateral or prove caller authority. The legacy insurance source port and source-call-path evidence remain separate.

The independently supplied `InsuranceConfig` names a normal sponsor and operator and pins the mainnet, contract, collateral, accepted creation transaction, definition commitment and oracle. These must agree with the independently supplied economic config and the actual accepted non-native perpetual definition. The current slice requires contract binding, marking and liquidation. It rejects insurance plus source-consensus configuration; complete raw/rejected v7 history is never replaced by the typed v9 journal. Native contracts, multiasset insurance and ADL remain unsupported.

`EconomicInput::Insurance` is a local unverified operation with a real external input ID and actor. It has no invented wire type. Prior activation 18 is an explicit market-eligibility requirement of this policy. The same ordinary/economic/global/internal replay collision checks apply to these IDs. The legacy Joint/Spot format fences still reject the resulting activity.

* `Deposit { atoms }` requires the configured sponsor and debits that sponsor's real collateral into the derived insurer escrow.
* `AdoptFees { max_atoms }` requires the operator and transfers at most the actual unadopted insurance-fee custody to that escrow. Historical adopted fees plus current pending fees must equal independently reconstructed fee receipts. The same pending atoms cannot be adopted again.
* `Backstop { max_atoms }` requires the operator. Requests execute in recorded input order after the actual AMM-first and book liquidation phase. They preserve those executions and the remaining market positions.

A backstop first accounts for any existing funded loss-pool payout. It then covers the minimum of its explicit budget, actual reserve custody, remaining positive IOU claims and recorded net unpaid losses. The named policy deliberately permits partial coverage. It records existing-funded payouts separately from insurer-funded payouts, so total payouts need not equal the new insurance contribution. New insured payouts do equal that contribution. Zero eligible coverage is rejected atomically.

The exact canonical IOU claimant order selects payouts. Sorted owner order selects the debt entries extinguished by the same coverage. The full before maps and both allocation lists are retained and recalculated during replay. Historical gross defaults remain unchanged; outstanding unpaid losses decrease. Later positive PnL cannot offset an already extinguished default, and old claims cannot receive the same funding twice. The insurer takes no position and this policy creates no subrogation claim.

All collateral uses exact `u128` atoms. A deposit above 2^53 atoms remains exact. Every reserve, pending-fee, loss-pool and claimant movement passes through the owner's existing internal-transfer sequence and restricted Joint custody bridge. A receipt retains the full local input, fund commitment, height, reserve and pending balances before/after, internal sequence interval and complete backstop proof. AMM claimant payments enter the existing physical AMM cash stream and its semantic replay.

The block candidate is published only after all inputs, matching, marks, funding, real liquidation, backstops, custody checks and checkpoint-size checks succeed. A late empty/invalid request discards earlier transfers and executions in the same candidate. The synced `DurableEconomicChain` journal publishes the entire owner together. Lost acknowledgement poisons the original handle and denies cached reads; reopen requires an independently expected exact head. Rollback restores reserve custody, claims, debts, fees, positions and input replay together. No separate insurance namespace or cross-journal atomicity is asserted.

TLECv9 is a bounded deterministic full-owner replay checkpoint. It retains the exact economic/insurance configuration, independently pinned pristine Joint bootstrap, complete accepted catalog and ordered typed input frames, full insurance state/receipts and internal transfer sequence. Decode reconstructs the actual owner through those same transitions, compares every receipt and canonical bytes, and re-runs owner invariants. It does not strip feature metadata to pass a legacy decoder. The native composition adds independently expected native policy bytes and its checked pristine bootstrap restore; native tag5 and insurance tag6 remain distinct. When insurance is disabled, previous version selection and bytes remain unchanged.

Limits are 4,096 insurance receipts, 4,096 rows per claim/debt/allocation list, 65,536 aggregate insurance records and the existing stricter shared contract-record/64 MiB checkpoint bounds. Counts are checked before decoder loops. No historical records are silently evicted; reaching a limit rejects the candidate until an explicitly designed compaction/replay policy exists.

Retained rollback witnesses also require exact adjacent typed-history prefixes: the same pinned bootstrap, unchanged earlier catalog/frames and exactly one added frame at the actual transition height. Two independently valid histories with identical asserted block IDs cannot be spliced to change the history restored by rollback. This general contract-binding check applies to older v6 owners as well as v9 and remains alongside the native Ledger journal transition check.

The central integration target has11 tests, including AMM-first pool claims,
later reversal, bounded corruption and the two-valid-branch splice regression.
Private tests cover existing-funded-pool replay and a synced payout followed by
lost acknowledgement. The combined native/insurance target uses actual native
target collateral and preserves both complete journals through recovery and
rollback. Repository gate results are recorded in
[roadmap progress](ROADMAP_PROGRESS.md). CRCs, replay and commitments establish
self-consistency under independent configuration. Authenticated inputs,
historical admission, oracle proof authority and L1 finality remain separate
gates.
