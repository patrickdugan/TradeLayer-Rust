# Shared TxIndex metadata algorithms

`TxIndexSourceOwner` now runs the original maximum-height and genesis metadata
bodies through one private IO interface. The existing owner and its legacy DB
adapter retain dependency arguments, order, outcomes and stored rows. The adapter
adds no console effects to legacy evaluations. This factoring prepares the later
rich DB carrier; it does not admit new public rich handlers or replace the
financial owner.

The original `initializeIndex` factory await is outside both source catch blocks.
The first block includes finding the genesis document and logging an existing
genesis. The second includes inserting the genesis and logging success. A
console failure after insertion therefore preserves the insert and enters the
insertion catch. A failure in the catch's error logger replaces the original
thrown value. The helper preserves these boundaries and the same error reference.

Root validation passes 15 Rust executions across four targets: six shared-helper
checks, six original TxIndex owner checks, two rich runtime/replay checks and one
Main partial-write error-prefix check. Helper comparisons cover 24 legacy owner
input/failure combinations. Four helper tests reuse creation-identity invariants;
these counts are not separate full-source parity claims.

`txindex_metadata_catch_observer.cjs` executes the complete, unchanged,
hash-checked pinned `txIndex.js` module for eight recorded DB/console boundary
cases. Rust compares full event order, retained row count and original-versus-
replacement thrown identity. The unchanged earlier helper fails this comparison
for both success-log catch boundaries; the corrected helper passes. The observer
uses genuine `big-json` 3.2.0 and recorded DB/console boundaries. It does not claim
a real NeDB persistence or financial settlement test.

Rust 1.90 formatting, all 117 included implementation formatting checks,
all-target Clippy with warnings denied, supply-chain and per-type inventory
checks pass. Input and log digests are recorded in
`fixtures/reference_fidelity/rich-shared-metadata-core-integration.json`.

The last full release remains `963aae7` with 1,714 Rust executions and 238 numeric
reference checks. This focused integration is not a new full-suite promotion.
Main financial row/catchup crash recovery, rich DB creation provenance,
successful extraction/live and financial/deferred carriers remain unfinished.
Default `GlobalClearingV2` and per-type parity/proof classifications are unchanged.
Hashes and error identity do not authenticate external inputs.
