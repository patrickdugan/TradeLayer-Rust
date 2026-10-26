# Canonical economic state-oracle producer

The implemented `economic_state_oracle::EconomicStateOracleExport` produces
canonical, independently checkable data
from the existing locked `DurableEconomicChain`. It does not implement UTXORef,
a fraud-proof engine, signing, transaction admission, or a second Ledger.
An external consumer receives exact state bytes and enough checkpoint witness
data to check their relationship to an independently supplied durable head.

The export covers the current **single economic owner**: the Joint registry,
spot state, sole Ledger, configured contract state, margins, reserves, IOUs,
fees, and enabled funding/mark/liquidation/AMM/catalog components. It does not
silently include detached issuance-intent, procedural, insurance, or other
standalone stores. It does not claim that the entire protocol is ported or
that caller-supplied chain inputs have been authenticated.

## Producer and reader APIs

```rust,ignore
let expected = ExportExpectation {
    config: &independently_configured_chain_policy,
    head: independently_expected_durable_head,
    state_commitment: independently_pinned_state_digest_or_none,
};
let export = EconomicStateOracleExport::from_durable(&owner, &expected)?;
export.write_new(absolute_new_file_path)?;
let decoded = EconomicStateOracleExport::read_file(path, &expected)?;
```

The producer checks the original journal's health and exact expected head,
compares the owner's actual configuration, serializes its canonical checkpoint,
checks that checkpoint's commitment against the expected head, and fences the
head again before publishing the export value. It never calls an economic
mutation method or clones a Ledger into another retained owner. The immutable
borrow excludes concurrent owner mutation. Existing invariant checks may
temporarily reconstruct bounded states while validating replay.

The reader requires the same independently supplied configuration and head.
It validates framing, network/genesis, sequence, tip, lengths and commitments
before allocating a retained checkpoint copy or performing owner replay.
`EconomicChain::from_checkpoint_bytes` then checks canonical TLEH/TLEC encoding,
the supplied economic policy/admin/permitted properties, complete available
owner invariants, and available replay history. A repaired CRC or recomputed
self-hash cannot replace the independently expected head. An optional separate
state pin can reject another otherwise valid head containing different state.

Expectations copied from an untrusted export are not independent. All checks
here establish internal consistency with those expectations; they do not
authenticate their origin or the underlying blockchain transactions.

## One payload and its full-head witness

The existing `EconomicHead.commitment` hashes the **complete TLEH checkpoint**,
including retained rollback frames. A TLEC-only state payload cannot establish
that relationship without additional witness data. The export therefore
retains one canonical TLEHv1 checkpoint. Its canonical TLEC state is exposed as
a borrowed slice inside that checkpoint, rather than duplicating the large
state bytes in the file or retaining another decoded economic owner.

`state_bytes()` returns those exact TLECv1–v7 bytes. `checkpoint_bytes()` returns
the complete TLEH witness. TLSEv1 and TLEHv1 framing remain unchanged. Optional
source consensus selects TLECv7; disabled source consensus preserves v1–v6 bytes.
Older economic versions can be exported as full current state; the producer
never invents absent ordered input history or a transition witness.

## TLSE version 1

All integers in the binary envelope use big-endian encoding. Block hashes and
genesis use the repository's canonical display-order bytes.

| Offset | Bytes | Field |
| --- | --- | --- |
| 0 | 5 | `TLSE` and version byte 1 |
| 5 | 1 | Mainnet tag: Bitcoin 1, Litecoin 2, Dogecoin 3 |
| 6 | 32 | Pinned genesis |
| 38 | 8 | Durable journal sequence |
| 46 | 8 | Exact tip height |
| 54 | 32 | Exact tip block ID |
| 86 | 8 | Rollback floor height |
| 94 | 32 | Rollback floor block ID |
| 126 | 1 | Embedded TLEC version, 1 through 7 |
| 127 | 4 | TLEC state byte length |
| 131 | 4 | TLEH checkpoint byte length |
| 135 | 32 | State commitment |
| 167 | 32 | Full checkpoint commitment |
| 199 | 32 | Export commitment |
| 231 | variable | One complete canonical TLEHv1 checkpoint |

There is no second opaque state root or imported proof object. The TLEC offset
inside TLEHv1 is fixed at 123 bytes and is checked against its actual framing;
unknown versions, trailing data and inconsistent embedded lengths reject.

The three commitments have distinct meanings and domains. `SHA256d` means
SHA-256 applied twice; concatenations below contain the literal ASCII domain
bytes shown, without a trailing NUL.

```text
stateCommitment = SHA256d(
  "TradeLayer/EconomicStateOracle/State/v1/" || u32(TLEC.len) || TLEC
)

checkpointCommitment = SHA256d(
  "TradeLayer/DurableEconomicChain/v1/" || TLEH
)

exportCommitment = SHA256d(
  "TradeLayer/EconomicStateOracle/Export/v1/" || TLSE[0..199] || TLEH
)
```

The checkpoint commitment is exactly the existing durable-head algorithm.
The export commitment also binds sequence, exact chain context, declared
versions/lengths, and the separate state digest. A rollback and deterministic
reapplication may reproduce the same TLEC and TLEH bytes, while the changed
journal sequence produces a different export commitment and rejects a stale
expected head.

These are data commitments, not proofs of authenticated transition validity.
They must not be equated with older TLPS/TLPT or separate zkTL root namespaces.

## Manifest and transport

`manifest_json()` returns a small fixed-order JSON document with schema, scope,
`authenticatedAdmission:false`, network/genesis, exact tip and rollback floor,
durable sequence, codec versions/lengths, the three commitments, and
`transitionWitness:"not-exported"`. Heights and sequence are decimal strings
so JSON consumers cannot round a `u64` through binary64. State atom amounts are
left in their exact canonical binary encoding, including values above `2^53`.

The full payload is never expanded to Base64 inside the 16 MiB service JSON
response. Initial transport is a new binary file plus the small manifest. A
future HTTP chunk endpoint must bind every requested chunk to this export
commitment and must not mix different owner heads.

## File and allocation behavior

The checkpoint limit is the existing 64 MiB. The export limit is that size plus
the fixed 231-byte header. Declared lengths are checked before retained-copy
allocation and owner decoding. Hashing uses the existing parts API so adding
a domain does not create another full-size concatenation buffer. Writing
streams header and checkpoint instead of building a second complete envelope.

`write_new` requires an absolute path in an existing directory and uses atomic
`create_new`; an existing path is never overwritten. It writes all bytes and
syncs the file. Unix also syncs the parent directory; this implementation does
not promise Windows directory-entry durability across power loss. A failed
write or sync returns an operation-labelled error and can leave the new file
partial; it is not silently deleted or presented as complete.

`read_file` checks file metadata before allocating, reads exactly the bounded
reported size, detects an extra byte or truncation, and runs the complete
decoder. File integrity and independently expected identities are still
required even when a path was previously written by this producer.

## Remaining producer/consumer boundary

The legacy `state_oracle` balance query format remains unchanged. It uses
source binary64 quantization/bucketing, includes selected available balances,
and omits full economic custody/replay state and chain context. Its hash is not
substituted for this exact owner-state commitment.

UTXORef remains external. It must independently establish the relevant chain,
trusted configuration/bootstrap, accepted inputs, oracle identity and event
authorization. The source-shaped DLC bridge remains a distinct relay check;
its legacy fallback is not used by this producer. Signing, fraud adjudication,
L1 inclusion, and proof verification are not performed here.

A later transition-data producer can export actual before/after states,
ordered typed inputs, and linked block context where real history exists.
TLECv6 retains complete typed input frames and a committed pristine bootstrap.
Optional TLECv7 additionally retains actual raw source candidates and accepted,
rejected and duplicate receipts. Restore reexecutes Types, validity, actual-owner
reads and mapped commands, and requires each rollback snapshot's source history
to be an exact prefix of the current history.

That embedded audit is replay material, not authenticated L1 evidence or a
separate fraud-proof witness bundle. Actor/reference amounts remain caller
claims, and missing dependency state cannot authorize a command. The manifest's
`transitionWitness:"not-exported"` retains this narrow meaning. Older snapshots
do not acquire invented source inputs or missing transition evidence.

## Verification

The seven passing tests in `tests/economic_state_oracle.rs` build a real funded
sole-owner lifecycle with actual margin, fee custody and an unpaid profit claim.
They check exact balances above
`2^53`, all three mainnets, untouched original owner/head, canonical decode,
configuration/network mismatch, independently pinned state and head, a second
genuinely valid owner with recomputed CRCs/hashes, malformed framing and limits,
new-file/no-overwrite behavior, durable restart, rollback and ABA sequence.
An active v6 fixture also restores and reexecutes four complete bound input
frames with actual contract creation, positions and a funded mark. The internal
unit regression also passes: it injects a synced-but-unacknowledged journal
commit and requires
the poisoned original owner to deny export of its cached state. The fully verified
producer/wire baseline passed 1,113 tests across 136 executables, formatting,
all-target Clippy, the zero-third-party-crate gate and all-target release.

The current source-consensus extension has targeted coverage for actual v7 TLSE
export, full source replay, restart, rollback, independently bound expectations
and alternate-history prefix rejection. Full current-batch checks remain pending.
These results are local implementation evidence, not authenticated historical
acceptance or external UTXORef integration.
