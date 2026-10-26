# Economic owner reads for source validity

`EconomicSourceBackend` implements the source validator dependency interface
against the actual immutable economic owner under `EconomicSourceReadV1`.
The checked constructors bind the exact expected tip or healthy durable head;
the private block-stage constructor binds the candidate network, parent and
next height. An immutable borrow prevents state mutation during evaluation.
No Ledger, balance map or independent catalog is retained.

The ten implemented reads cover protocol admin, active flags and the strict
33-of-36 activation threshold; actual free/reserved/margin/channel custody;
oracle admin/backup, known oracle metadata and an actual published price; and
contract definitions admitted by the original owner. Async source reads return
`Pending`, while `Activation.getAdmin` returns immediately. Source missing-await
bugs therefore remain visible to the validator rather than becoming a fabricated
synchronous authorization check.

Custody units cross the source Number boundary only when the existing canonical
ECMAScript Number text round-trips to the exact owner Decimal amount. A lost
atomic unit is an explicit `UnsupportedNumericDomain` error. The full owner and
its canonical export preserve larger exact amounts independently. Available
sufficiency excludes actual order reserves and contract margin; channel custody
is keyed by channel address, not credited to each participant's free balance.
Required amounts supplied as String or BigNumber must survive the same Number
boundary: the source's first comparison uses JavaScript `<` before BigNumber
shortfall arithmetic. An extra decimal remainder that Number would erase is
explicitly unsupported rather than compared under a different exact rule.

This is a named owner projection, not a reconstructed legacy NeDB database.
Zero owned custody maps to missing tally zero; legacy retained zero-valued rows
are not recreated. The current owner has no funded vesting state, so the
vesting entitlement projection is zero. Pending withdrawals have distinct local
custody semantics and currently make the tally projection unavailable. No
legacy fallback oracle price of 1 is returned without a real publication.

Unrepresented property catalogs, tickers, country/xpub attestations, raw chain
outputs, proof verifiers, guest engines and native-market evidence return
`UnknownDependency` or `Backend`. The base Ledger's issuer/kind fields do not
become fabricated source property type/whitelist/procedural metadata. Registry
oracle and accepted-contract projections expose only recorded owner fields;
they are not a claim of complete external store migration.

The source validity result, including a returned `valid: true` or Promise, is
diagnostic. Both the backend and evaluation permanently deny authenticated
admission. The executor still enforces its independently configured named
economic policy and actual custody invariants. For example, source backup-admin
recognition does not change the existing Registry's stricter publisher rule.
Chain inputs, sender identity and historical acceptance remain separate gates.

The integration tests exercise actual three-network owner reserves and margin,
exact-amount rejection, source BigNumber shortfalls, real oracle publications,
the complete source validator's Promise behavior, immutable state, explicit
missing dependencies and durable restart/rollback/ABA freshness.
