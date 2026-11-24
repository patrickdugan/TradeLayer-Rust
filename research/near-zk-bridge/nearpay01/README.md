# NEARPAY01-A: execution outcome binding and conditional receipt graph

2026-11-24. Research only. Source-derived native reference, not a Nova proof, full NEAR light client or custody implementation. All authority gates default false.

## Outcome commitment

Sources: nearcore core/primitives/src/transaction.rs (PartialExecutionOutcome, to_hashes), https://near.github.io/nearcore/architecture/how/proofs.html (published shard-root fixture), https://nomicon.io/ChainSpec/LightClient.html (outdated, must reconcile with pinned nearcore release), https://near.github.io/nearcore/DataStructures/MerkleProof.html.

Narrow success-only profile: Borsh vector of generated receipt IDs (u32 length, 32-byte hashes), gas u64le, burnt tokens u128le, executor account (u32le length, ASCII bytes), status tag 2 + Borsh byte vector for SuccessValue or 3 + 32 bytes for SuccessReceiptId. Let P=SHA256(partial). Outcome leaf=SHA256(u32le(2+logs.length)||id||P||SHA256(log_0)||...). Metadata excluded by nearcore. Reject unknown/failure status in this deposit profile; a separate failure proof may later support recovery. Bound ASCII, receipt/log count and length, direction bits and Merkle path depth. The source guide's published execution fixture reproduces the exact shard root using actual Borsh field values and directional proof; it is a documentation fixture, not a fresh mainnet deposit.

For an RPC light_client_proof: fold outcome leaf via outcome path to shard root, hash the 32-byte shard root and fold shard path to block outcome root, compare with inner lite. Compute lite hash from height, epoch, next epoch, previous state root, outcome root, timestamp, next BP hash and block Merkle root (timestamp exact integer nanoseconds); block hash=H(H(H(lite_bytes)||inner_rest_hash)||prev_block_hash). Verify block path to the independently pinned NEARHEAD01 block Merkle root. This full composition is exercised using a synthetic header fixture only. Resolve current nearcore protocol/version, timestamp encoding, and block hash/commitment-block identity using real RPC vectors before circuit keys are frozen. Never trust the RPC-supplied head root. Archive proofs for all three receipts against one already accepted finality head.

## Two separate authentication problems

An execution outcome commits its executor, status, emitted logs and generated receipt IDs. It does not by itself commit the corresponding action receipt body, its predecessor, argument bytes or input/output data dependencies. A raw RPC receipt JSON object or claimant boolean has zero authority.

The research inspector checks the transfer -> receiver -> resolver graph conditionally: parent status names resolver and both children are generated; executor identities and action IDs agree; transfer method, sender, vault, token, amount and destination match; receiver output data is exactly the resolver input data, with token as data receiver. The authenticated action-receipt layer remains unimplemented. Obtain exact receipt bodies via archival RPC and independently prove membership in the canonical outgoing-receipt commitment (including shard assignment and versioned serialization), then prove creation/data dependency edges; alternatively prove the pinned token/vault code's exact unique promise topology and authenticate complete event payloads. Review sufficiency with nearcore before accepting any approach. No action-authenticated flag until real proof exists.

Current ZTB-02B local Rust candidate logs only candidate sequence. For outcome-only destination binding upgrade it to a canonical complete event: ZTB_DEPOSIT_V1:<domain hex>:<sequence decimal>:<sender>:<script hash hex>:<requested atoms>:<unused atoms>. The fields come from the actual authenticated token predecessor and validated receiver args, not relayer input. Require exact event and receiver SuccessValue; or authenticate full candidate storage separately. Pin and authenticate vault/token deployed code or explicit upgrade/issuer trust policy. The exact NEP-141 transfer event format in this fixture is a deliberately restrictive mock SDK profile; adapt only after pinning each supported token implementation. Refuse extra/malformed events rather than tolerating ambiguous accounting.

## Resolver corner case

Reference https://github.com/near/near-sdk-rs/blob/master/near-contract-standards/src/fungible_token/core_impl.rs. If the sender is unregistered while the promise is in flight, a refund can burn the tokens while the resolver reports the original amount as used. Incomplete refund can also produce a used amount larger than intended keep. Therefore:
  requested = exact initial transfer log amount > 0
  unused = exact receiver result, 0 <= unused <= requested
  kept = requested - unused
  terminal used must equal kept, AND refund log must exactly account for unused.
Strictly reject burns, surplus logs, mismatched values, or unresolved outcomes; full refund consumes candidate but grants no mint. Real reserve solvency is still separate, including all concurrent withdrawals and issuer freezes. A full-retain-only production V1 may simplify the circuit, with partial refunds remaining experimental. Real token code/version must be audited, not assumed from the standard.

Stable identity: ZTB-02A deposit ID = tagged SHA256(domain||terminal resolver ID||u32le(0)); add candidate nullifier tagged TLBRIDGE/CANDIDATE-ID/V1(domain||u64le(sequence)). Both consumed atomically at TL issuance; same vault sequence cannot be claimed using a different receipt. A simulated alternate terminal receipt demonstrates why both are required. Epoch changes and upgrades cannot reset these identities. Future ABI should bind the receiver ID and code policy as well. Reconcile candidate sequence with authenticated vault accumulator before mint.

## Folding plan

Fold typed bounded microsteps using Nova Pallas/Vesta ppsnark IPA. Public state: TL type-36 binding element, bridge domain, accepted NEAR head hash and block Merkle root, expected receipt role/id, running SHA/Merkle state, graph commitment, checked amount and candidate/receipt nullifiers. Split 256-bit digests into two u128 limbs with exact byte reconstruction. Use fixed maximum paths and log lengths or segmented SHA-256 streams with constrained lengths and canonical padding; prove complete processing, no unchecked hash/preimage or successful host verdict. Estimate costs by SHA blocks and distinct shapes before choosing depth; existing type-36 30 KB node-frame and 4 s block budget must be met. Proof-step circuit and key remain unbuilt. No synthetic fixture is acceptable for production minting.

## Next gates

1. Capture live or sandbox archival full RPC outcome proofs and exact receipt bodies for full keep, partial refund, full refund, missing receiver registration and sender deletion/burn. Archive raw JSON with head checkpoint, version, independent source provenance and proof hashes.
2. Verify against an independently checked NEARHEAD01 root, derive current receipt commitment/edge proof and token/vault code identity. Include malicious same-executor action substitutions and detached data receipt.
3. Implement the Nova circuit for this exact relation, generate genuine compressed proof, run TL Rust verifier and negative controls, then review phase-2 custody effect. Keep can_mint=false until all gates pass.

Run node test.mjs. Exact test bodies exercised in an isolated JS runtime with independent SHA256 checked against the abc known answer; Node harness uses node:crypto and compares to frozen evidence. Result: 28 assertions, one published shard-root fixture, synthetic complete membership and conditional graph. No live RPC access in this environment.
