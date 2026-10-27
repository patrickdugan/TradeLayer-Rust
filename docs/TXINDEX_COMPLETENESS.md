# txIndex completeness

The parallel scanner is implemented. The remaining source-port work is the
complete pinned `txIndex.js` database lifecycle, which should be tracked
separately from performance promotion and authenticated chain admission.

| Area | Verified scope | Remaining work |
| --- | --- | --- |
| Read-only scanner | Bounded parallel fetch, fused parsing, TXID/wTXID hashing, TL carrier extraction, deterministic source ordering, Merkle comparison and optional same-block dependency waves | Broader real-chain interoperability measurements |
| Literal JavaScript behavior | Pinned marker/first-nulldata behavior, reference projection and three-versus-nine-argument decoder mismatch are characterized independently | Whole initialization, MaxHeight and recovery flags, transaction rows/lookups, validity updates and clear/reset owner, including source failures and restart behavior |
| HashMap performance candidate | External PR141 has source-order equivalence checks, synthetic CPU measurements and a real mainnet block-1 smoke test | Representative multi-transaction real-block CPU and peak-RSS measurements on the intended host before promotion |
| Authenticated admission | Existing scanner reports remain explicitly unverified | Parent UTXO/script and authoritative chain/context integration in its separate worker lane |

The local source baseline is `dd79498`. This assessment inspected
`src/parallel_txindex.rs`, `src/raw_block_ingest.rs`,
`src/legacy_txindex_audit.rs`, their tests and existing source/throughput reviews.
Generic journal and durable observer functionality does not establish a
complete literal TxIndex owner.

PR141 was read through GitHub on 27 October 2026 at
`8cb31416adf36dc7eefa53a396a40659eb3a6668`: open, draft and unmerged. Its body
explicitly leaves representative real-block CPU/RSS evidence pending.
[PR141](https://github.com/patrickdugan/TradeLayer-Rust/pull/141) remains owned by
the external worker; this assessment changes no scheduler or candidate branch.

Full Bitcoin validation is not a prerequisite for describing read-only scanning
as implemented. It is a separate gate before treating observations as accepted
state-changing inputs. The source DB lifecycle remains an independent porting
task; neither that lifecycle nor a faster reducer supplies authentication.
