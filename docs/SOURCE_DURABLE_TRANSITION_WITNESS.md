# TLSW5: archive-backed conditional source transitions

Root integration: nine tests pass on the actual current crate, including the
long financial prefix, three crash boundaries, immutable live-owner verification
and exact old-provider compatibility fixture. Eleven existing durable regressions
also pass. Full release promotion of this batch remains pending. The work-stage
evidence below is retained separately from these Root results.

TLSW5 extends conditional source witnesses to the existing segmented durable
archive. It adds no financial snapshot decoder, second ledger, policy version or
trusted adapter. TLSW1–4, TLCR limits/default V2, durable segment/publication and
journal bytes remain unchanged. A private read-only restore observer is the only
change to the old durable implementation; existing open uses a no-op observer.

The verifier supplies an archive locator and independently retained context,
current publication, prehistory head, scope/descriptor, command/receipt hashes
and expected end history/context. These pins establish consistency relative to
external expectations, not truth or authentication of chain, actors or inputs.
Reading the pins from the same witness provides no independent evidence.

The complete prefix is reexecuted through the existing segment decoder and
actual receipt executor, on the same Core across segment boundaries. The local
proof journal rotates; balances, receivers, aliases, pending jobs and native
failure prefixes do not reset. A validated prefix owner is captured from this
execution when the requested ancestor boundary is reached. It is computed from
commands, never accepted as a supplied current financial state. The current
archive and the requested prehistory must have the exact verified ancestor link.

The new slice is one existing durable segment: 1–256 commands with the original
bounded label. It reuses actual source execution and TLSW4 policy/profile/scope
admission and endpoint projection consistency for V9/V10/V11. The original TLDS1
segment codec derives its resulting history head, including exact source
outcomes, ordered effects, local and absolute sequences, state/receipt heads and
segment digest. All must match independent end pins. Native source throws,
returned Error values and partial writes remain literal. Historical local
capability errors refuse instead of becoming original source execution.

The witness envelope contains the bounded new segment, context/descriptor and
endpoint claims, protected by separate TLSW5 domain hashes and CRC. It does not
contain the full prefix. Verification requires the referenced archive material;
this is not a self-contained compact snapshot or a succinct proof. The current
publication is a separately held verifier-time pin, so an older valid transition
can be checked against a later independently retained publication of which its
prehistory is an actual ancestor. Stale or substituted publication pins refuse.

## Live and offline use

`produce` / `verify` / `verify_bytes` open a closed archive through the existing
namespace/context/publication verification. Journal's exclusive OS lock remains
enforced. For a running owner, `produce_from_archive`, `verify_from_archive` and
`verify_bytes_from_archive` borrow that owner immutably and validate its existing
Journal handle against the same independently retained context/publication. They
then reexecute immutable archive segments; cached financial state is never a
replacement for prefix execution. Tests assert the live publication and complete
runtime remain unchanged. No lock bypass or observer-supplied execution result
is introduced. A poisoned/ambiguous live owner must recover through existing open.

The result exposes the actually reexecuted runtime and bounded actual receipts,
with pre/post history and the publication used for verification. It does not
expose a TLCR checkpoint purporting to contain the complete segmented prefix or
a financial state restoration capability. Both scope claims remain conditional;
`require_authenticated_input_and_state` always refuses. Complete block ordering
and carrier completeness, canonical chain/rule context, authenticated UTXOs and
scripts, actor mapping and economic authority remain separate. No UTXORef mock
exercise was run.

## Verification performed

The immutable actual Core provider is
`a19118368cf9ccd4ee6e14b36df0f775b4cc0c6ba345eafa9bfcb5f3ecc55c7a`.
The work-only Rust 1.90 overlay copies existing private replay/codec helpers for
visibility and uses that provider for actual source business execution. It does
not rebuild Core or modify Root.

Completed checks include:

- A 5,036-command prefix: four actual initial input/load commands, 32 successful
  balance updates and 5,000 genuine fee-owner mutations, followed by a source
  reserved-balance throw across a segment boundary. The new witness reexecutes
  the prefix and exact retained failure. Recovery and retry preserve the actual
  owner, history and failure. The retry reloads the unchanged persisted row and
  retains the same failed memory prefix; it is not repaired into a second debit.
- Live-owner production and byte verification with exact immutability; historical
  verification after the archive advances; stale publication rejection.
- Three actual owned child exits at code 86: torn temporary segment, durable
  segment before publication, and durable publication before acknowledgement.
  Recovery/retry uses the correct independently held publication. Orphaned/torn
  files provide no authority. The failed native financial call remains literal.
- Missing and corrupt archive segments; reordered, duplicated and omitted entry
  histories with repaired checksums/counts/digests/publication pins. Existing
  actual replay rejects stale receipts. Every endpoint/context/command/receipt
  pin is separately exercised. A forged native receipt with all public envelope
  checksums and receipt/post-history pins repaired still fails actual reexecution.
- A work-only exact side-by-side comparison with the unmodified provider durable
  API remains independent evidence. Root's zero-dependency compatibility test
  instead compares its actual execution with deterministic old-provider goldens:
  all ten original commands, ten complete receipts, four publications, four full
  runtime debug diagnostics, four typed state diagnostics, context/reopen output
  and all five exact journal/segment file bytes. The 39-record fixture was
  captured twice directly from immutable a191 public APIs, byte-identically; it
  is used only for comparison, never as an imported financial poststate. Root
  needs no old-provider crate or alias. Provenance is checked in alongside it.
  The old API retains local-error histories, while the new native-source witness
  explicitly refuses them. The observer receives only shared references and
  cannot change execution.
- Main callable scope retains/resumes an actual pending height write across
  durable boundaries. Scope/profile setters, empty/oversized segments, excessive
  labels and older-policy promotion stay refused.

Ten checks passed in one complete optimized run (seven new checks including the
child entry plus three unchanged hash/codec checks), and the two subsequent Main
and bounds checks passed individually. Test and production Clippy with warnings
denied and formatting checks pass. A coherent full Root build/release run remains
the integrator's responsibility.

## Bounds and remaining cost

The existing maximum is 65,536 segments, each at most 256 commands and 32 MiB;
frames, source-state and runtime working-set limits remain those of the existing
codecs/owners. The verifier retains a bounded digest list and at most one decoded
segment at a time, plus actual Core/prefix clones. The TLSW5 envelope is at most
32 MiB including metadata. No aggregate prefix TLCR is created and the old 4,096
TLCR limit is not raised.

Full-prefix reexecution and archive IO remain linear. The offline path first
uses existing open and then performs observed prefix reconstruction; this can
execute the prefix twice. The live borrowed path performs the observed full
execution without reopening the locked journal. No constant restart time,
compact state codec, general daemon memory bound or power-loss guarantee for
Windows FileSyncOnly is claimed. The enumeration guard checks consistency of
enumerated tables, not proof that no state component was omitted.

## Mounting

Add `src/source_durable_transition_witness_impl.rs`,
`src/source_durable_transition_witness_tests.rs`, this document and the
`fixtures/source_durable_transition_compatibility.tsv` / corresponding provenance
JSON. The compatibility revision replaces only the test/document and adds those
fixtures; production witness/observer code is unchanged. Apply the
work-stage `prepare.py::prepare_durable` to fresh current
`src/source_durable_replay_impl.rs`. It wraps old restore with a no-op observer,
adds the private shared-reference callback and the new include, retaining the old
unit-test include. Do not replace a current source with the standalone preview.
No crate-root or Cargo change is required. Root owns its full integration checks,
local commit and any later admission work; nothing is pushed or published here.
