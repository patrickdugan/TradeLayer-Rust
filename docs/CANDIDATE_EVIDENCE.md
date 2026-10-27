# Conditional complete candidate evidence

`candidate_evidence` is a read-only verifier and immutable per-height reader for
the whole-block B/E and C/END transcripts introduced by merged
[PR142](https://github.com/patrickdugan/TradeLayer-Rust/pull/142), source
`1f08a32e8bfa5a4ca77f8c64e8f88477e2268f8b`, merge `41007c5`.
It owns no database, writer, ledger, chain selection, RPC client or source
metadata emulation. It uses the existing `raw_block_ingest`, `read_only` and
private SHA256 implementation; there is no second transaction/carrier decoder
or cryptographic implementation.

## Independent expectation and coherent snapshot

The only constructor is:

```rust
CandidateEvidenceReader::verify(
    &independently_expected_descriptor,
    &claimed_store_descriptor,
    complete_block_transcript,
    complete_candidate_transcript,
)
```

The two descriptors must match exactly. Their fields bind the Bitcoin network
label, explicit `PR142_BLOCKS_CANONICAL_CORE_CARRIER_V1` profile, opaque 32-byte
publication token, immediately preceding covered-window anchor, first/last
covered heights, last header hash, and SHA256 of both complete transcript byte
strings. SHA256 includes framing and completion markers. Descriptor hashes are
digest order; block/TX hashes are SHA256d raw order internally and display order
on the wire. A pre-genesis anchor has height -1 and an all-zero hash. Otherwise
the first covered height must be exactly anchor height plus one.

The caller must acquire the expected descriptor from its independently chosen
snapshot/checkpoint contract. Computing expectations from the same untrusted
store claim does not authenticate a snapshot. The fingerprint helper is named
`fingerprint_transcript_unverified` for that reason. Even with all fingerprints
rebound, omitted or forged candidates still fail the structural rescan.

The verifier receives immutable transcript slices; it does not acquire storage
locks or certify how files were read. A SQLite adapter should use one read
transaction and a separately bound snapshot commitment. A native adapter should
capture CURRENT once and read only those referenced, digest-checked segments.
Old pinned snapshots can remain conditional evidence after a reorg; selecting
the current fork or retrying a stale expected token is the caller's job. An
A-B-A return with a different publication token is a different expected
descriptor, even when its block/candidate bytes equal an earlier snapshot.

Both compact stores presently retain candidate bodies rather than every raw
block body. Completeness verification therefore also requires retained B frames
or an external source of the exact raw blocks. This module does not manufacture
missing historical data or prove data availability after pruning.

## Exact frame and completeness checks

The profile accepts canonical ASCII, LF-terminated whole-block frames:

```text
B <height> <display-blockhash> <raw-block-hex>
E BLOCKS <blocks>
C BLOCK <height> <blockhash> <tx-position> <display-txid> <vout> <format> <type-or-dash> <syntax-status> <raw-tx-hex>
END COMPLETE_UNVERIFIED BLOCKS <blocks> <all-transactions> <carriers>
```

Fields are tab-separated. Decimal fields have no signs, whitespace or redundant
leading zeros. Hashes and raw hex use lowercase. Blank, CRLF, NUL, unterminated,
mixed SCRIPT-mode and trailing frames are rejected. This is the canonical
emission/storage profile, not every permissive spelling the PR142 input CLI can
parse. Network labels and anchors remain caller claims checked against the
independent descriptor; they are not inferred as authenticated identities.

For every covered B block, the verifier:

1. Checks the claimed header hash, contiguous height and raw previous-header
   linkage from the independent window anchor.
2. Uses `UnverifiedRawBlockView::parse` and `inspect_serial` to check exact
   transaction boundaries, canonical transaction syntax, block weight and TXID
   Merkle root, and obtain the complete ordered carrier classification. The
   serial verifier uses the same bounded scanner as PR142's worker path.
3. Retains metadata for every block, including blocks with no TL carriers, and
   constructs the entire expected carrier set before accepting progress.
4. Compares every C field against that set: block identity, TX position, TXID,
   vout, format, type, exact syntax label and complete original raw TX bytes.
   All rows must occur once, in the scanner's order; both completion markers and
   all counts must agree. Omitted, extra, duplicate or reordered rows fail.

This closes the membership-versus-completeness boundary demonstrated against
PR144 at `48270d07ec01ad0d8e091f9bedc41c4bd8c3cd5a`: its independently valid
four-TX/two-carrier fixture accepted a C transcript containing only
`END ... 1 4 0`. This verifier rejects that exact omission as `MissingCandidate`,
even after rebinding the transcript hash. It also rejects the probe's arbitrary
type `999` / status `asserted-valid` labels, instead of treating labels as
authority.

## Read API, limits and failure behavior

On success, `blocks()` includes every covered block, `block(height)` returns its
immutable evidence, and `rows()` iterates complete carrier evidence in source
order. Counts include every block transaction, not just candidate transactions.
Raw TX bytes are retained once per candidate transaction and shared by all its
output rows. Transactions without carriers retain block/count metadata only.
The reader copies retained candidate bytes, so later mutation of caller-owned
input buffers does not mutate a verified snapshot.

Each transcript is capped at 64 MiB, each line at 8,000,512 bytes, each covered
window at 256 blocks / one million transactions / 20,000 carriers. Existing raw
block and transaction parser limits also apply, including the four-million-byte
raw block/weight limits. Line bounds are checked before allocation or field
splitting; field splitting itself has fixed column bounds. Candidate raw hex is
compared without allocating another copy per row. No expected full-text C
transcript is built, avoiding repeated multi-output TX-body expansion.

All failures return an error before a reader exists. The function has no external
effects or mutable backend, so there is no partially published progress or
database rollback claim. Revalidation after restart requires the independently
expected descriptor and both original transcripts; there is no self-authorizing
cache import or standalone trusted checkpoint.

`typed-syntax` is the existing scanner's syntax classification. It is not the
literal Types/Validity outcome, activation, source txIndex first-nulldata policy,
or authenticated TradeLayer execution. `require_authenticated_chain`,
`require_authenticated_utxos` and `require_execution_authority` always refuse.
PoW, chainwork, active best-chain membership, BIP141 witness commitment, input
scripts/UTXOs, parent-output addresses/amounts and actor authority remain outside
this module. Network and publication fingerprints do not supply those facts.

## Verification and mounting

Seven tests cover the real 370-byte/four-TX/two-carrier fixture and exact PR142
executable output, independent Python transcript SHA256 golden values, the
PR144 omission probe, every candidate field and both footers, invalid labels,
ordering/duplication, zero-carrier blocks, multiple outputs sharing one raw TX,
independent descriptor tampering, header/Merkle/parent failures, gaps, canonical
framing, bounded input and immutable A-B-A snapshot retries.

The seven tests pass in the actual pinned Rust1.90 full suite, including the
real `tradelayer-core-index-ingest` executable. Formatting, Clippy and release
checks are recorded in `fixtures/reference_fidelity/source-proof-candidate.json`.
No npm package, NeDB runtime, bitcoind, additional Rust crate or storage writer
is introduced. The frozen source txIndex metadata oracle remains a distinct
reference policy.

```text
cargo test --frozen --test candidate_evidence
```
