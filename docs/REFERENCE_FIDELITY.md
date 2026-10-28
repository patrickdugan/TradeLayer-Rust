Current release ef6dbe9: 1,641 executions (1,640 top-level plus one successful subprocess), 218 exact numeric checks, formatting and Clippy pass. All 1,247 frozen inputs were rehashed. The mixed ownership correction and selected TLSW4/5 and Bitcoin profiles are included. V12 remains staged and uncompiled. Broader historical parity, production integration, authenticated inputs and complete-block proofs remain open. See [release evidence](../fixtures/reference_fidelity/archive-witness-release-validation.json).

Earlier integration accounts retained for provenance:

The full release baseline remains `31ee8f4`: 1,597 executions and 198 exact
numeric-reference checks. The mixed Queue/Core/Consensus correction is committed
at `7f5733e` and passes its focused mixed-history and compatibility gates.

The next batch adds TLSW5 archive-backed conditional witnesses, six Bitcoin
payment/channel input templates and exact block/input byte binding. Its 35 Root
focused executions pass, including 11 prior durable regressions. Clippy and
formatting pass; full release promotion is pending. Independent Core script
comparisons, historical compatibility goldens and scope limits are recorded in
[the focused record](../fixtures/reference_fidelity/archive-witness-bitcoin-focused.json).
Broader source/historical parity, production orchestration, authenticated inputs
and complete-block proofs remain open. The V12 orchestration candidate remains
work-only and uncompiled; it is not part of this batch.

Earlier focused-stage account (preserved for provenance):

# Reference fidelity and runtime coverage

## Current focused integration

`AmmLifecycleV11` now composes all 36 Main kinds in the focused source domain. The latest integrated evidence is [SOURCE_AMM_LIFECYCLE.md](SOURCE_AMM_LIFECYCLE.md); full release, authenticated history and complete block proofs remain separate gates. The last full release baseline is V9 at `b02a045`. Earlier-stage descriptions below retain their original narrower scope.

## Earlier lower-profile evidence

The explicit lower lifecycle profile is now `LowerLifecycleV7`. It runs actual
raw1/8/11-15 and named issuance/grant/redemption/Oracle methods over the same
literal Core, with 137 genuine Property and 212 genuine Oracle observations.
The uncaught engine-specific JSON and composite-actor identity domains are
counted separately as two local refusals. Existing conserving transaction
policies retain their own behavior. Wider raw-type/JavaScript coverage, Main
processing and V7 witness export remain incomplete.

`MAIN_PHASE_DRIVER_EVIDENCE.md` adds 39 reproducible original-class Node
observations. The source has different live/indexed phase order, non-atomic
processed/validity/height journals, and retained failure/retry effects. These
observations guide the next Rust integration; they are not counted as Rust
Main parity, startup success, or completed block proofs.

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

The actual source property and clearlist owners now preserve all seven property
kinds and bounded metadata/cache/database behavior, including source address-only
attestation overwrites. Their opt-in sequential integration is documented in
`SOURCE_CATALOG_EXECUTION.md`. The separate strict Registry still scopes its own
attestations by list; it is not substituted for the source clearlist state. Raw
helper conformance and an integrated conserving transaction remain different
measurements. Unrepresented domains and outer policy differences remain open.

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

`ConservingSequentialV1` uses atomic funded-mutation rollback and once-per-block owner
phases. The source can retain effects before a later failure and has different
live/catch-up phase ordering. Native entitlement, signed-channel allocation,
AMM and insurance policies also have explicit scopes. Their local integration
tests verify those policies, not blanket historical JavaScript equivalence. See
`CONSENSUS_SOURCE.md`, `NATIVE_ENTITLEMENTS.md`, `ECONOMIC_INSURANCE.md` and each
module's policy documentation.

Actual source oracle metadata now has its own complete owner. Its cache,
watermarks, document history and source-thrown partial prefixes are retained;
funded views use real persisted prices without a substitute Registry row.
That distinction, source send failures and the explicitly conserving spot
settlement differences are documented in `SOURCE_ORACLE_EXECUTION.md`,
`SOURCE_SEND_EXECUTION.md` and `SOURCE_SPOT_EXECUTION.md`. Missing contract
Margin dependencies still prevent genuine raw matched-order admission.

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
the later actual source owner. The opt-in catalog path now executes those
effects through the owned clearlist state instead of that generic mapper.
Nine focused generic mapper tests passed; their captured
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
