# Verified native recovery open

The explicit Journal recovery-open API checks an independently retained physical
sequence, canonical file location and complete key/value receipt while retaining
the same exclusive file lock used for replay and optional tail truncation. Wrong,
stale, foreign or changed contents reject without truncating, including a changed
frame with recomputed CRC and extra keys outside the selected transaction query.

The compactor first checks persistent identity, canonical catalog, namespace and
exact current publication under its writer lease. It maps the expected logical
sequence to the physical generation sequence, then invokes the same-lock Journal
check. Existing open APIs and serialized bytes are unchanged.

Complete KV receipts match the prepared-validity metadata receipt profile:
double SHA-256 over the domain, then every sorted key/value with ordinal and length
framing. Every key contributes to the count and byte total. Hashing streams chunks
without cloning the entire map. All receipt fields must come from independently
retained healthy observations/prepared mutations; unknown/incomplete receipts
are rejected. A matching sequence alone cannot authorize repair.

Root checks pass 102 Rust executions: 101 parent tests and one storage subprocess.
They include 12 new integrity/lock/race tests, existing prepared append/update,
compactor and index tests, storage/adversarial tests and native owner/intent-store
regressions. Three ignored entries are child-test runners. Counts reuse existing
controls and do not increase the last full-release total.

The standalone byte-comparison test used a separately linked old library, which
is unavailable as an external crate within a Core unit test. Root now compares
exact before/damaged/recovered bytes and ordered recovery/KV records captured
twice by an executable linked to immutable provider8bccb174. Capture source,
fixtures and provenance are checked in. No self-comparison replaces that check.
The original compile failure and missing-temp-setting run remain in the evidence.

Cargo formatting, 133 included implementation formatting checks plus the capture
producer, all-target Clippy with warnings denied, supply-chain and unchanged
37-row per-type inventory checks pass. Digests and scope are recorded in
`fixtures/reference_fidelity/verified-native-prefix-core-integration.json`.

This storage prerequisite does not complete financial Main crash recovery.
Row/catchup continuations, typed no-index ownership, superseding intermediate
intents, terminal suffixes and process-loss reexecution remain separate work.
Tests cover cooperating file handles/writer leases, not host power loss or hostile
replacement of the surrounding filesystem. Content integrity does not authenticate
external chain, actor or transaction facts. Default GlobalClearingV2 is unchanged.
