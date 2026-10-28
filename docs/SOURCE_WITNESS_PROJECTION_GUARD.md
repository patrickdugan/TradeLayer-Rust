# Strict TLSW4 endpoint consistency follow-up

This is a separate revision of the frozen initial TLSW4 proposal. It changes no
runtime owner, table, queue, balance, native source exception or previous witness
format. `prepare_witness` adds one error, one read-only consistency check and one
call at TLSW4's reconstructed endpoints.

For **each enumerated** `state_value().tables` entry, the check compares the
original name and rows exactly with `runtime.collection(name)`. Duplicate names
or an unequal authoritative collection refuse with
`InconsistentStateProjection(name)`. It performs no normalization, sorting,
financial poststate substitution or repair. This is consistency of the enumerated
entries; it does **not** prove that no collection or other state component was
omitted from the projection. The raw state commitment helper remains usable for
independent diagnostics; production and verification both refuse an inconsistent
endpoint even when that raw commitment matches.

The genuine mixed regression replays the existing original bootstrap, property,
send and spot sequence from its actual initial reset/profile inputs. The first
three calls retain exact authoritative rows and can be witnessed. On immutable
provider `60ec139e80f8991ab1c065c55f7244f6b38c8f54d849d68ebd5a684e7f9d7c8a`,
the fourth call has two original `orderBooks` rows in the authoritative queue and
an empty `state_value().tables` entry. Strict production refuses that endpoint.

The binary regression fixture is a complete initial TLSW4 envelope that was
actually produced **and verified** by the frozen initial implementation on that
same provider. Its independent command, receipt, context and endpoint pins and
public envelope checksums all match. Strict verification rejects it by comparing
the reexecuted authoritative collection, even though the prior hashes match.
The regression expected rows are original source observations, used only for
comparison and never imported as financial state.

This guard does not itself fix the underlying engine's split view, the Core DB
read path or consensus hashing. Those paths use the separately integrated V11
shared-owner correction. The initial all-36 evidence must not be promoted to
broad source-state oracle fidelity from passing conditional-witness counts.

The selected corpus runs 240 Main and 32 genuine Property seed calls without
weakening the frozen initial tests. Root passed all ten TLSW4 tests and three
strict-guard tests on corrected provider a19118368cf9ccd4ee6e14b36df0f775b4cc0c6ba345eafa9bfcb5f3ecc55c7a.
The mixed check reaches the positive branch: its corrected view exposes the two
exact source rows and produces a new verified witness with independently expected
pins. The original stale envelope refuses. Root's separate complete mixed-history
comparison also passes. These focused results await full release promotion.

On provider 60ec the strict audit accepted all 38 selected isolated witness cases
and refused none of those cases. The separate mixed fourth call was refused as
described above. No claim is made about unselected histories or enumeration
completeness from that count.

All TLCR prefix/resource bounds and the initial TLSW4 trust limits still apply.
There is no segmented durable-history, block-completeness or authentication claim.
