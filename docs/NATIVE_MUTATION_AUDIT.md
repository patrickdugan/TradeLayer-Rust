# Native entitlement owner mutation audit

Reviewed base: `70dd91190892d4523ab283abdd6da0b78c0b42b2`. The owner is integrated
in the combined native/insurance batch; see [verified gate results](ROADMAP_PROGRESS.md).
This audit does not establish historical acceptance.

The named mechanism is `SourceQuoteExactOrRejectV1`. Its mandatory independent policy selects four distinct property roles, actual issuer/kind/current-issued totals, separate source catalog classes/declarations, two coupon/target pairs, curve scales, initial non-owned markers, real funding owners/amounts, operator, driver, coverage policy and ASCII payout order. No deployed amount, issuer, operator or default is provided. A coupon marker is accounting metadata, never another spendable balance.

| Actual mutation boundary | Required behavior |
|---|---|
| Ledger `execute`: Create/Mint/Burn | Reject all four reserved role IDs after adoption, regardless of Fixed/Managed kind. Ordinary supply of other properties remains unchanged. |
| Ledger `execute`: Transfer | Coupon transfers reject. Target transfers append an exact ordinary-target journal record. Every transfer into/out of `~native:` rejects even for unrelated property IDs or self transfers. |
| Coupled coupon send | Only the private owner bridge invokes the atomic Ledger handler. Transfer original coupon amount and whole-unit source marker drag together; source Number conversion loss rejects. Original input ID is processed once. |
| Explicit target | Operator, network, actual height, complete source quotes, markers, eight-decimal representability, receiver overflow/Number boundary, funding and all deterministic physical IDs are preflighted before debit. Targets transfer actual escrow tokens, never mint or infer a residual. |
| Channel commit/withdraw/transfer/net claim/king settlement | Reserved native property IDs reject in the common private operation fence. Transfer checks both endpoints. Adoption rejects existing native channel, pending withdrawal, claim or settlement liabilities. |
| Channel withdrawal maturity | Existing native liabilities cannot be adopted; later creation is fenced. Restore rechecks this disjointness, so maturity cannot become a hidden target/coupon mutation. |
| Co-signed channel enrollment/revision | Native coexistence rejects before witness/signature work. Native bootstrap witnesses are ordinary TLL1–3 only; signed witnesses cannot recursively import a native owner. Shared scoped budgets and independent combined policy are required before enabling coexistence. |
| Spot reserve/fill/cancel | All actual available movements pass through Ledger Transfer. Coupons are excluded from permitted spot assets. Target movements are fully journaled; actual spot reserves remain reconciled to the same Ledger. |
| Clearing/margin/fees/AMM/IOU legs | Existing economic transfers use the common Ledger path. Configured coupon collateral is forbidden; configured target collateral can move only through the actual existing funded owner and every target transfer is journaled. Native payout never reuses an economic escrow address or fake external type. |
| Unsupported embedding owners | Legacy Protocol, Chain, FundedIou, isolated SpotChain and legacy ClearingSandbox must reject native metadata. Legacy Ledger, TLSP/TLJP/TLJE readers reject native versions; extracting an ordinary nested Ledger cannot remove native custody fences. |
| Native restore | Independent expected policy; exact full ordinary bootstrap hash; no nested native/signed metadata; replay every native event; compare all actual balances of all four roles and exact `(target_property, escrow_account)` pairs; preserve bootstrap and native replay IDs. |
| Joint/global categories | Coupon send/explicit target originals belong to the actual Ledger/global partition. Physical IDs are receipt-bound and checked against Registry, Spot, global, economic and system IDs. No fake type18 or system epoch. |
| Header history | Actual outer owner checks parent/tip. Native events contain only actual available network/height, never an invented parent hash. Retained before→after must preserve exact policy/bootstrap/event prefix and reject newly introduced IDs already processed before that transition. |

`SourcePerCallV1` consumes the complete supplied target each call, including repeats. `MonotonicDeltaV1` retains the absolute observed target and consumes its nonnegative increment. These are different opt-in policies; neither authorizes the target. `ExplicitUnverified` targets are local typed inputs. Source execution mode must reject them until an actual captured owner-derived volume origin exists.

The source's whole-token marker floor can diverge from later fractional payout weights: A/B coupons and markers10/10, send1.1, gives coupon8.9/11.1 and markers9/11. A full target20 with scale20 exceeds Bob's marker; the new mechanism rejects the whole transition. It does not clip, redistribute or mint. Four frozen complete-source observations and two additional Number-boundary observations retain the actual source no-op/partial-failure behavior separately.

V1 Joint/Economic construction is pristine genesis only; positive-height migration, funded type0/genesis issuance, an automatic volume schedule, authenticated actor/UTXO authority and native+signed-channel coexistence remain separate gates.
