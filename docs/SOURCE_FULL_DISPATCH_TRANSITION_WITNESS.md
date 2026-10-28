# TLSW4 conditional source command witnesses

TLSW4 adds a separate format for the literal `SpotLifecycleV9`,
`ActivationLifecycleV10`, and `AmmLifecycleV11` runtimes. It reconstructs the
existing sole Core from its complete TLCR command history and reexecutes a
bounded slice on that same Core. It creates no second balance registry.
TLSW1–3, TLCR1–11, their admission/refusals, and the default V2 policy are unchanged.

The caller must retain independent pins for the policy, scope, descriptor,
prehistory head, before state projection and adapter context, ordered commands,
complete receipts, after head, after state projection and after context.
Copying those pins from the same untrusted witness provides no independent evidence.
The verifier checks exact native outcomes (including returned Error objects,
throws and pending throws), ordered effects, sequence numbers and commitments
by actual execution. Source failures retain their actual partial writes. Jobs
and receiver state are reconstructed from history; pending work is bound and
resumed by the existing runtime instead of normalized or silently drained.

## Scopes and trust

`SourceCommandSlice` admits actual source business calls, queue evaluation,
original nine-argument transaction dispatch, height inputs and declared storage
failures. Runtime-local unsupported capabilities reject the witness. All 36
numeric routed kinds have dispatch admission; each still needs successful owned
execution for its concrete arguments and policy. Admission does not assert that
every payload is supported or financially valid.

`MainCallableSlice` admits the existing callable transaction and middle-block
entry points plus the four owned height methods. It requires the actual
source-registry activation profile. It proves only the supplied callable slice.
Imports, entry/profile setters, supplied activation booleans, and environment
answer adapters cannot appear inside either proved slice. Such declared inputs
can be independently bound prehistory, with the resulting trust limits retained.
Any local capability error in that prehistory is refused rather than relabeled
as an original source exception.

Both scopes remain conditional on supplied chain/RPC/client/file/database facts.
No hash authenticates a chain, actor, administrator, transaction signature, RPC
server or canonical prestate. The result always refuses
`require_authenticated_input_and_state`. There is no complete-block scope:
independent canonical carrier completeness, transaction ordering, chain/UTXO
and script evidence are outside TLSW4. UTXORef is a separate consumer and was not
exercised by this work.

## Evidence and current limit

The input projector uses six checked-in genuine original-JavaScript corpora.
It imports only each constructor/reset's initial input tables and explicit
profiles. It never imports a later observed financial poststate. All later state
comes from actual calls on the same Core. Unavailable prefixes are isolated until
a fresh reset. Original modules, dependencies and generators are not changed.

The 390-frame projection executes 240 Main calls and 32 genuine Property seed
calls. The calls preserve 16 source throws; 14 unavailable frames are excluded.
It selects 38 Main witness cases spanning all 36 routed transaction kinds,
including bootstrap/retry, AMM balance changes and deferred spot failure/retry.
Some transaction kinds intentionally return source errors or placeholder results;
this evidence does not turn those into successful settlement implementations.
Six original generators passed both dependency versions (`--check` and
`--lock --check`). The new checks compare genuine source outcomes, then compare
the complete reexecuted native replay owner and receipts. Existing dedicated
source suites establish their separately scoped source-state/effect parity.

The initial worker provider `60ec139e80f8991ab1c065c55f7244f6b38c8f54d849d68ebd5a684e7f9d7c8a`
had a mixed-history split view: the authoritative `orderBooks` collection had
two rows while `state_value().tables` exposed an empty collection. The integrated
V11 shared-owner correction and strict projection guard now pass Root's ten
TLSW4 tests, three guard tests and complete mixed-history comparison on corrected
provider `a19118368cf9ccd4ee6e14b36df0f775b4cc0c6ba345eafa9bfcb5f3ecc55c7a`.
The corrected mixed case produces and verifies a new positive witness, while
the old inconsistent envelope refuses. TLSW4 binds the existing projection and
reconstructs its history; it does not certify complete table enumeration or
unrestricted source-state fidelity. Full release promotion of this batch is pending.

All histories remain subject to TLCR's existing 4,096-command total prefix limit,
4 MiB frames, 32 MiB checkpoint/witness bounds and source runtime resource limits.
The prehistory is fully replayed on restart. This format does not accept the
segmented durable journal; the separate [TLSW5 format](SOURCE_DURABLE_TRANSITION_WITNESS.md)
verifies archive-backed histories. Neither format is a compact snapshot or an
indefinite continuous-node resource solution.

## Integration

The implementation is included once, immediately after TLSW3 inside
`source_contract_replay.rs`. The integration tests, projected fixture/inventory,
projector and independent Node command pins are included in the local checkout.
Exact focused validation is recorded in
`fixtures/reference_fidelity/shared-owner-witness-focused.json`.
