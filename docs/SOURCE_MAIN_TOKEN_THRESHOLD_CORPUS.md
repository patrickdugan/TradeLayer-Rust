# Original Main/raw20 threshold failure corpus

Thirty frames execute twenty actual Main calls and ten independent constructor/input resets, with zero unavailable-call exclusions. The scenarios cover funded indexed/live baseline, first/second un-awaited activation threshold factory failure and first/second threshold find failure, each followed by the exact original processed retry. The preceding awaited type-activation read remains successful; failure skip counts target the subsequent source threshold calls.

The complete pinned Main, TxIndex, Types, Validity, Activation and other business modules execute unchanged. Explicit factory faults use the same bounded one-shot collection/op/skip/message input already used for find/update faults in the DB test adapter. Real registry fallback initialization and writes remain visible. No activation result, validator result, signature result or financial poststate is supplied.

All cases preserve the source's literal Promise-sufficiency refusal. Full state, actual receivers, returned position identities, data-only RPC inputs, ordered storage effects, fallback writes and processed retries are recorded. Genuine BigNumber 9.3.1 and 9.1.2 regenerate identical fixture SHA-256 `d384ec5fddb47278c81910277163bbc4e160f8c651db908438383251c2c9aec9`.

Rust 1.90 formatting, metadata and Clippy of the new test pass. Fresh runtime execution remains a separate Root gate; this corpus must not be counted as twenty Rust parity passes until that run succeeds. In particular, the fallback write interleavings may expose additional continuation work beyond the ordinary successful threshold-read correction.

Mount the script, vector/inventory fixtures, test and this document under their corresponding repository directories. The Rust test reuses the existing upper-state/effect/replay comparisons and additionally requires an empty V9 pending-threshold diagnostic at every public Main boundary. The corrected 146-frame upper corpus remains separate.
