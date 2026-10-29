# Rejected-pin recovery ordering correction

This separate revision supersedes only the affected files/transforms in frozen
storage candidate1a0b876a. The old candidate, review snapshots and independent
failure probes remain unchanged. No catalog/TLDB encoding or default admission
policy changes. No Root edit or whole-Core build was performed.

The catalog previously opened its Journal with the requested recovery policy,
then checked publication and logical sequence. An independently rejected pin
could therefore truncate an incomplete tail. Both a foreign publication and a
same-publication stale sequence independently reproduced98→71bytes before error.

The correction checks the catalog publication before opening the generation.
The native Journal's new checked-open path parses the entire valid committed
prefix without repair, checks its expected physical sequence while retaining the
exclusive file lock, and only then permits explicit tail truncation. The catalog
maps logical sequence to the separate physical/logical creation origins before
calling that checked-open API. Existing unpinned APIs keep their original
recovery behavior and byte formats.

The independent probe is retained with assertions unchanged (only module import
paths projected). On corrected code it reports foreign98→98, stale98→98 and
matching98→71 with27bytes explicitly discarded. The new regression additionally
compares all file contents and mtimes, checks both earlier/later stale sequences,
and retains matching authorized recovery. Full standalone debug/release and
Clippy checks include all existing source/stream/crash/packing tests.

Mount after the original47-file candidate: replace compact_journal.rs,
storage_read_snapshot_impl.rs and compact_journal_tests.rs from this revision;
apply only `prepare_native_storage` to fresh current storage.rs. Do not also
apply prepare_catalog/prepare_snapshot_helpers after copying their corrected
files. Those two pure transforms are supplied for independent reproduction.
Root owns actual Core validation and commit. This local correction establishes
pre-repair context/CAS checks, not authentication from a publication hash.
