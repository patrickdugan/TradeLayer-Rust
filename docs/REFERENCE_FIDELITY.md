# Reference fidelity and runtime coverage

The compatibility reference is `tradelayer.js` commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Runtime integration is a different
measurement from reproducing that reference. The former spreadsheet label
"Integrated subset" meant that some operations reach the actual shared state
owner. It did not establish equivalent reference behavior. The tracker now
calls that stage **Runtime subset** and assesses transaction logic separately.

A module can have complete diagnostic method coverage, partial runtime wiring,
and deliberate runtime differences simultaneously. These facts must be recorded
separately. A successful diagnostic decode or validation is not evidence that
the corresponding `Logic.typeSwitch` state mutation is implemented.

## What counts as faithful

Source-conformance evidence must identify the exact source bytes, dependency
versions, exercised input domain, dependency mocks, observed outputs, exceptions,
and ordered side effects. The source's active definition is authoritative when
JavaScript contains duplicate methods. Characterizing a broken source path is
useful evidence; substituting an intended implementation is a different behavior.

The runtime must not receive a reference-parity label merely because a repair is
safer, conserves balances, compiles, or passes its own tests. A deliberate repair
needs an explicit rule or policy name and its source discrepancy. An accidental
reconstruction error needs a regression against the unchanged source and a fix.
Missing behavior remains missing. Authentication and historical-chain evidence
are additional gates; they must not hide ordinary functional discrepancies.

For example, the current Ledger represents fixed and managed token classes,
while the source property catalog represents seven kinds and additional ticker,
metadata and administration behavior. The separate native-entitlement policy
does not fill that whole catalog. The runtime Registry scopes attestations to a
list, while the JavaScript clearlist store can overwrite another list's record
at the same address. Those are concrete gaps or differences, not full ports.

## Transaction logic inventory

`LOGIC_BY_TX_TYPE.json` records the 36 routed source types (0 through 35), plus
an explicitly unrouted type-36 audit row. It links source dispatch, validation,
effects and defects to actual runtime mappings, dependency gaps, and the next
implementation work. The same evidence feeds **Logic by Tx Type** in the
existing completeness workbook. A mapped action can still be unavailable when
the real pending-owner validation backend cannot supply an accepted property,
clearlist, country, xpub, or other required fact.

The 37 rows are a static inventory, not 37 executed end-to-end conformance
cases. The independent property and clearlist runs, focused Rust tests and
earlier full-suite baseline have separate execution records. A required
post-state witness must state which rules it proves: a conserving runtime
outcome cannot substitute for observing the source's partial-failure effects.

`ConservingSequentialV1` uses atomic candidate rollback and once-per-block owner
phases. The source can retain effects before a later failure and has different
live/catch-up phase ordering. Native entitlement, signed-channel allocation,
AMM and insurance policies also have explicit scopes. Their local integration
tests verify those policies, not blanket historical JavaScript equivalence. See
`CONSENSUS_SOURCE.md`, `NATIVE_ENTITLEMENTS.md`, `ECONOMIC_INSURANCE.md` and each
module's policy documentation.

Independent property and clearlist reviews are linked from the transaction
inventory. Inaccessible web-worker patches are not part of the reviewed code,
regardless of their reported commit IDs, source hashes, or case counts.

The follow-up type9 mapper guard rejects unrepresented issuance roots, exact
lowercase `xpub:` metadata, nonempty/non-string revoke reasons, and list0
country/banlist operations. It preserves ordinary direct attestation and
empty-string-reason revocation, including the source's ignored root on revoke.
The pinned decoder uppercases metadata and emits no `merkleRoot`; the extra
field tests therefore enrich an explicitly diagnostic validity adapter. They
verify the mapper's represented scope, not authenticated wire acceptance or a
new complete clearlist owner. Nine focused mapper tests passed; their captured
checks and final hashes are in
`fixtures/reference_fidelity/type9-mapper-execution.json`. The captured
`committed:false` value describes the delegated check before the parent commit.

## Fraud-proof work is a separate axis

A state or checkpoint commitment binds bytes. It does not independently prove
that an economic transition follows the selected rules. The current TLSE
producer explicitly reports `transitionWitness: "not-exported"`. Local complete
checkpoint replay is useful verification, but is not a compact, exported
disputed-transition proof.

Each transaction row therefore records the required pre-state, post-state,
original transaction and execution order, network/block context and actor facts,
and dependency reads. Remaining work includes an explicit versioned transition
claim, authenticated state-read witnesses, deterministic rule execution, and an
independent verifier that accepts or rejects the disputed claim. Chain inclusion,
sender authorization, and required UTXO/script evidence remain explicit inputs.
No row becomes proof-complete merely because its state hashes agree.

TradeLayer produces checkable state-oracle data. UTXORef is a separate consumer;
optional Binohash work belongs to that repository and is not a TL parity gate.
