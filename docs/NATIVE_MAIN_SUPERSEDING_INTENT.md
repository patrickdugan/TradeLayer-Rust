# Verified reopen and durable superseding intent

An actual prepared validity write now captures a private healthy recovery pin
before append. Reopen accepts one explicitly expected Before, compaction
Intermediate or Post candidate from that sealed operation. It checks complete
native contents, identity, location, namespace and publication under the existing
exclusive lock before tail repair, restores the actual compaction configuration,
then verifies Directory and read context. Wrong or forged candidates do not
authorize repair. Immutable original read leases may remain after dropping the
old mutable index.

Before and Intermediate can reprepare only the exact retained actual mutations
on the reopened real owner. The replacement retains original selection, query,
arguments, financial/control context and before/after contents. A fresh native
owner token and actual old-to-new mapping replace the unusable old append token.
The metadata helper performs no source query, mark, decode, append or await
resolution. Original Post is resolve-only.

The new explicit operation is NativeSupersessionIntentV1 under profile
TL-MAIN-NATIVE-SUPERSEDING-INTENT-13-1. Pure preparation retains expected control
Before/Post pins before IO. Sync binds the retained same-command permit, original
pending intent, actual reopened observations, exact mutations, predecessor and
replacement candidates. Fresh append remains a separate action after sync.
Only complete actual replacement Post permits durable prefix acknowledgment.
Repeated native ambiguity can supersede current pending lineage without an
intermediate acknowledgment or Idle state. Synced control lost-ack can reopen
the independently expected exact Post and rebind only the retained permit.
Unknown/torn control bytes remain refused without generic tail repair.

Legacy STORE13-2 context, APIs and old state/blob bytes are preserved. Legacy
readers refuse the new operation marker even after later CommandIntent, Accepted
or Idle states. Explicit profile selection is required. The old-format goldens
were produced twice by an independent executable using unchanged historical
store source and the immutable old support provider. The exact producer/source
and provenance are retained under fixtures/reference_fidelity. Tests compare
those bytes rather than aliasing the new implementation as its own reference.

Root checks pass 128 focused Rust executions: 24 new reopen/supersession tests,
48 existing private regressions, and 56 public compatibility tests across four
targets. The public checks retain segmented 4,353-command replay and eight real
process-exit boundaries. Cargo formatting, all 142 included-file format checks,
legacy producer formatting, all-target Clippy with warnings denied, supply-chain
and per-type inventory checks pass. Initial runner failures from an unavailable
test-target name and missing rustc PATH are preserved separately from the
corrected successful checks. Counts overlap existing controls and do not raise
the last full-release baseline of 1,714 Rust / 238 numeric checks at 963aae7.

These are metadata and control-store prerequisites. The live financial row/
catchup controller still needs the typed no-index and supersession composition,
prepared acknowledgment recovery, service-tail/terminal handling, head/history/
next-command publication and process-loss reexecution. The maximum 1,024-link
lineage boundary is enforced, but that maximum-depth corpus has not run.
No financial snapshot importer, second Ledger, whole-Main retry, default policy
change, authentication or host power-loss claim is introduced. TL supplies
checkable state-oracle data; UTXORef remains a separate consumer.
