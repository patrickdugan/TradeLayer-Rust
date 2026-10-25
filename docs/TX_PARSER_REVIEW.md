# Transaction parser and hash compatibility review

Reviewed main `c7a37eb90b1ea335051bfc3cb7712116b372cfb7` and pooled-worker candidate `d4ba7592de3a93ee59ded6ebb2cd0cec91174368`. The review found one inherited witness serialization defect. The coordinating task corrected `src/read_only.rs`; independent fixtures now cover the affected case and the prepared/standalone hash boundaries.

## Confirmed defect: empty witness item is still witness

The old `prepare_transaction_prefix` considered witness present only if an item contained at least one byte. Bitcoin Core determines presence from the number of items in each input's stack: `[empty byte array]` is a nonempty stack. [Core v29.0 `CScriptWitness::IsNull`](https://github.com/bitcoin/bitcoin/blob/v29.0/src/script/script.h#L541-L552) and [transaction witness detection](https://github.com/bitcoin/bitcoin/blob/v29.0/src/primitives/transaction.cpp#L67-L84) make this distinction explicit. The [transaction deserializer](https://github.com/bitcoin/bitcoin/blob/v29.0/src/primitives/transaction.h#L200-L237) rejects witness serialization when every input stack has zero items.

Before the fix, the candidate release scanner rejected the independently constructed `one-empty-item` transaction with `Invalid("superfluous SegWit witness marker")`, exit status 1. That transaction has stack serialization `01 00`: one item with zero bytes. The independent Python oracle computes display-order IDs:

| Value | SHA256d display order |
| --- | --- |
| TXID | `730010127fe46ed0ba192cc5d3495bc8b04e2dce8f1415561d1f99271afa4678` |
| wTXID | `8d2a902144912d64222fd9e85eab5ba091803aeeebcef5de1ec8ca7502391550` |

The minimal fix updates the presence flag with `count != 0` after checking the stack-count bound, and still reads every length-prefixed item for syntax validation. A zero-item stack remains valid for an individual input when another input has witness. All-input zero-item stacks remain rejected as superfluous. No witness item content, signature, or previous-output script is interpreted by this change.

This was **not introduced by the pooled-worker optimization**. Both the standalone parser and the transaction-boundary parser before the fused-layout commit `07970a5` used the same incorrect item-content predicate. At the reviewed revisions, standalone `inspect_transaction` and borrowed block parsing both call `prepare_transaction_prefix`; comparing their results against each other cannot independently establish Bitcoin serialization semantics. The new oracle constructs stripped/full bytes separately and uses Python `hashlib`, rather than calling either Rust path.

## Flags, CompactSize, and hash spans

The scanner accepts the current witness marker/flag `00 01` and rejects `00 00`, `00 02`, `00 03`, and `00 ff`. Rejecting unsupported optional-data bits agrees with [Core's flag handling](https://github.com/bitcoin/bitcoin/blob/v29.0/src/primitives/transaction.h#L208-L235). The zero-flag case has a narrower boundary here: Core can deserialize some empty-input/empty-output test objects; this scanner requires nonzero inputs and outputs, and those empty objects are not valid Bitcoin transactions. Input/output count limits and maximum raw size are local scanner restrictions, not a full Core deserialization claim.

Canonical CompactSize checks match the width thresholds and nonminimal rejection conditions in [Core v29.0 `ReadCompactSize`](https://github.com/bitcoin/bitcoin/blob/v29.0/src/serialize.h#L273-L348). The independent matrix replaces each one-byte input count, scriptSig length, output count, output-script length, witness item count, and witness item length with all three wider encodings. All 18 cases reject as `NonCanonicalCompactSize`. Every truncation of the short witness vector, unsupported flags, excessive/zero counts, and a trailing byte are covered separately. A trailing byte is rejected at the standalone transaction boundary; borrowed prefix parsing correctly leaves it for the enclosing block boundary to reject.

The prepared TXID span starts **before the input-count CompactSize**, after any marker/flag, and ends after all outputs. Hashing combines version, that complete vin/vout span, and locktime. The prevout walker starts **after the input count**, using `first_outpoint_start`. The distinction added by existing commit `3935afd` is correct. The independent 253-input/253-output transaction uses a three-byte input count, a three-byte output count, and a 253-byte scriptSig; its expected TXID/wTXID catch accidentally dropping or including count bytes. Legacy 253-input/253-output bytes are covered too. [BIP141's hash definitions](https://github.com/bitcoin/bips/blob/master/bip-0141.mediawiki#transaction-id) and [BIP144's serialization](https://github.com/bitcoin/bips/blob/master/bip-0144.mediawiki#serialization) distinguish the stripped transaction from the full witness serialization.

## Unknown witness version and verification scope

The vectors contain an output `52 02 ab cd`, a version-2 witness program, a distinct synthetic parent with that output, and a child whose first input references the parent's independently hashed output 0 and carries one empty witness item. The test checks that exact reference and serialization and accepts both transactions for syntax inspection. A standalone raw transaction cannot identify the witness version of a spent output without previous-output data. Witness program versions belong to output scripts, not the transaction serialization flag. [BIP141's witness-program definition](https://github.com/bitcoin/bips/blob/master/bip-0141.mediawiki#witness-program) describes that separation.

These are constructed syntax observations, not valid-spend or script-verification fixtures. There is no authenticated UTXO lookup, policy evaluation, witness commitment validation, proof of work, or block consensus validation in this review. The synthetic block header commits to the independent TXID Merkle root solely to exercise hashing and ordering.

## Reproducible checks

`python scripts/audit_tx_parser_review.py` reconstructs 104 vectors, including 11 accepted syntax cases, and verifies `fixtures/tx_parser_review.tsv` and `fixtures/tx_parser_review_block.hex` using only the Python standard library. `--write` explicitly regenerates them. It independently constructs the stripped serialization and full serialization; expected hashes never come from Rust, a shared Rust cursor, or a reparse of Rust's offsets. Accepted vectors use distinct locktimes to keep their TXIDs distinct inside the combined block.

`tests/tx_parser_review.rs` verifies all statuses and hashes, a known synthetic version-2 parent/child reference, all malformed borrowed-prefix outcomes, and the independently constructed block through serial, windowed parallel, and single-team prepared paths. Block byte ranges, stripped size, weight, external Merkle root, TXIDs, and wTXIDs are checked. Four targeted tests passed under pinned Rust 1.90.0 MSVC after the presence fix. The fixture generator also passed. Broader repository checks are recorded by the coordinating task.
