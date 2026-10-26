# Canonical co-signed channel allocation transcripts

`channel_revision` implements the cryptographic/transcript portion of the new opt-in `SupersedingCoSignedAllocationV1` policy. It canonicalizes and parses exact binary descriptors, full allocations and signed revisions; computes domain-separated commitments; verifies both independently configured public keys with real strict-low-S secp256k1 ECDSA; and checks an exact-successor fixed-pool transition against supplied actual-owner context.

This module does not own a Ledger, accept a private key, sign in production, register a channel, move tokens, maintain the current revision journal, perform an L1 spend, authenticate a funding output/actor or revoke an old L1 transaction. The coordinated funded owner integration is responsible for storing policy/head/history inside the sole Ledger, actual property/participant checks, old-path fences, atomic custody mutations, global replay IDs, complete checkpoints, rollback and durable freshness. A detached signature helper is not promoted as that completed integration.

This is an explicit new policy. Full pinned `channels.js` at `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, SHA256 `b3fa22beab9a9fdb63246daa12582f86591a4217863240321f88fdff0160ec79`, has no canonical co-signed state preimage, monotonic sequence or commitment invalidation protocol. Its first-input pubkey extraction/multisig address reconstruction is not a signature over an allocation. Crowded column bumping can replace participants while preserving/swapping balances. `addCommitment`/`getCommitments` use uninitialized `this.db`. The new format is not a claim of source wire compatibility, historical acceptance, source clearlist compliance or signed Bitcoin transaction verification.

## API and independent policy

`ChannelRevisionPolicy` contains network, exact channel and A/B AccountIds, two canonical compressed public keys, funding epoch and sorted funding outpoints, script commitment, bootstrap Tip, initial Ledger commitment and initial channel-allocation commitment. Policy fields are ordinary public data. The enclosing owner must freeze the expected policy independently of incoming revision bytes or checkpoint contents. A candidate cannot supply its own trusted signing key.

`AllocationRow { property, atoms_a, atoms_b }` uses nonzero `PropertyId` and exact u128 atoms. `RevisionBody` binds policy ID, sequence, previous revision ID, actual-before allocation digest, parent Tip, validity window and complete new allocation. `SignedRevision` appends fixed compact signatures A then B. `RevisionHead` is derived metadata, not a capability. `RevisionContext` must come from the actual enclosing owner.

Public functions:

- `canonical_policy_bytes`, `parse_policy`, `policy_id`.
- `canonical_allocation_bytes`, `parse_allocation`, `bootstrap_allocation_commitment`, `allocation_digest`.
- `bootstrap_ledger_commitment(bytes)`: hashes exact bytes under its domain; this function does not parse, authenticate or validate those bytes as a Ledger. The owner supplies its actual canonical pre-enrollment Ledger bytes.
- `initial_head(policy, initial_allocation)`: checks the independent bootstrap allocation pin and derives sequence-zero head.
- `canonical_revision_body_bytes`, `parse_revision_body`, `revision_id`, `canonical_signed_revision_bytes`, `parse_signed_revision`.
- `verify_revision_signatures(policy, signed)`: checks policy match and both fixed signer slots. This proves a signature relation only; it does not check current sequence, custody, owner context or whether the state was superseded.
- `validate_transition(policy, previous_head, actual_before, context, signed)`: validates the full fixed-pool successor and returns its derived head. The owner must supply its actual head/custody/context and apply the result atomically.

Canonical parsing is structural. `parse_signed_revision` can successfully parse an invalid signature; only explicit cryptographic verification establishes its relation. No parser result grants admission. `ChannelRevisionPolicy::require_authenticated_binding`, `RevisionHead::require_authenticated_admission` and `RevisionHead::require_l1_revocation` always return explicit unavailable errors.

## Exact encoding

All integers are unsigned fixed-width big endian. An Account is `u8 byte_length || ASCII bytes`, length1..90. Reserved `~` accounts are rejected by the new policy. Owner A/B must be distinct. Public keys are fixed33-byte canonical compressed SEC1 encodings and must be distinct finite curve points. Funding outpoints use standard display-order txid bytes and u32 vout, strictly sorted by that pair without duplicates. Genesis/block IDs follow the existing display-order `BlockId` convention. No implicit byte reversal occurs.

Descriptor D:

```text
ASCII "TradeLayer/SignedChannelPolicy/v1/"
u8 policy=1
u8 network=1|2|3
genesis[32]
Account channel, Account participant_A, Account participant_B
compressed_key_A[33], compressed_key_B[33]
u64 positive_funding_epoch
u16 funding_outpoint_count
  repeated: txid_display[32], u32 vout
script_pubkey_sha256[32]
u64 bootstrap_height, bootstrap_block_id[32]
bootstrap_ledger_commitment[32]
bootstrap_allocation_commitment[32]
u8 binding_policy=0  # explicit configured, L1-unverified binding
```

The descriptor includes the actual selected network's genesis. A height-zero bootstrap must have that genesis ID. Heights must not exceed 9,007,199,254,740,991; positive non-genesis anchors remain unverified configuration. The epoch is full positive u64, not a JS Number. Unknown policy/binding tags are rejected.

Allocation L:

```text
u16 property_count
  repeated strictly ascending: u32 property_id, u128 A_atoms, u128 B_atoms
```

At least one and at most256 rows are permitted. Both amounts zero is noncanonical; zero on one side is allowed. Checked A+B must fit u128. The pure module checks IDs, order, amounts and fixed per-property totals; actual property existence and complete actual-custody extraction belong to the owner.

Signed preimage P:

```text
ASCII "TradeLayer/SignedChannelRevision/v1/"
u8 purpose=0  # complete fixed-pool allocation only
policy_id[32]
u64 sequence
previous_revision_id[32]
before_allocation_digest[32]
u64 enclosing_parent_height, enclosing_parent_block_id[32]
u64 valid_from_height, u64 valid_until_height
L
```

The signed envelope is exactly `P || signature_A[64] || signature_B[64]`. Both signatures are compact big-endian r||s over the same SHA256d(P). There is no DER, recovery byte, Bitcoin signed-message prefix, transaction sighash flag, JSON, decimal string, implicit optional field or padding. Structural parsers reject trailing bytes, truncation and noncanonical counts/order.

## Hash domains and initial-state binding

Using SHA256d(x)=SHA256(SHA256(x)):

```text
policy_id = SHA256d(D)
bootstrap_ledger_commitment =
  SHA256d("TradeLayer/ChannelBootstrapLedger/v1/" || exact canonical pre-enrollment Ledger bytes)
bootstrap_allocation_commitment =
  SHA256d("TradeLayer/ChannelBootstrapAllocation/v1/" || initial_L)
allocation_digest =
  SHA256d("TradeLayer/ChannelAllocation/v1/" || policy_id || L)
head0 = SHA256d("TradeLayer/ChannelRevisionBootstrap/v1/" || policy_id || initial_allocation_digest)
revision_id = signature_digest = SHA256d(P)
```

The separate bootstrap allocation domain avoids a circular policy hash. A restored arbitrary initial allocation must not become accepted merely because later signatures are valid. `initial_head` matches that allocation against the independently expected descriptor pin. The enrollment owner additionally checks the exact actual Ledger bootstrap commitment. No full Ledger is retained here.

Signatures are excluded from revision identity: another mathematically valid signature encoding/randomness over P cannot create another sequence ID. Strict low-S prevents the usual s↔n−s application-level malleability; high R remains valid. Native invalid/zero/out-of-range scalars produce `InvalidSignature`; a well-formed cryptographic mismatch/high-S returns false from the relation check. Transition validation maps a false relation to `InvalidSignature`. A malformed first slot can fail before evaluating the second; every successful result verifies both fixed slots.

## Transition order and scope

`validate_transition` preflights allocation/body bounds, validates the fixed policy, and requires:

1. Exact policy ID; actual-before digest equals both the private previous head and the signed before digest.
2. For sequence-zero state, previous head equals the independently pinned `initial_head`.
3. Sequence equals checked previous+1, and previous revision ID matches. Overflow is an error.
4. Actual network and parent equal the configured network and signed parent; actual height is parent+1 and strictly after bootstrap height. A height-zero parent is the network genesis.
5. Actual application height is inside the inclusive signed validity window.
6. Complete sorted property coverage matches actual-before rows, and checked A+B totals are exactly unchanged for each property.
7. Both real strict-low-S signatures verify over the exact P digest.

It then returns the new sequence, revision ID and allocation digest. It does not mutate the arguments or owner. A private owner using this function still must verify configured participants, extract **free** current channel escrow, reject pending withdrawals/NET claims, maintain the complete accepted journal and apply head plus A/B custody together. Publicly constructing a `RevisionHead`/`RevisionContext` does not make it genuine.

Fixed-pool V1 supplies no deposit, withdrawal, key rotation, participant bump, fee, PnL, margin or derivative operation. Do not silently reinterpret a co-signed allocation as permission for one of those operations. Do not assign this format a legacy TL transaction number without a separately defined admission/ingestion policy.

## Local supersession and L1 boundary

The owner may expose `RevisionStatus::{Unknown,Current,Superseded}` from its own complete journal/current head. This module defines the enum but owns no journal/status store. After r2 supersedes r1, pure ECDSA verification of r1 can still succeed; current-head transition/consumer checks must reject reuse. Exact-repeat, conflicting same-sequence and skipped-sequence commands do not advance the owner.

Local supersession does not invalidate an old L1 spend. Actual funding-output ownership, spend status, parent UTXOs, script execution, best-chain history, revocation/penalty/timelock/dispute policy, historical clearlist/property eligibility and signed-channel-to-actor binding remain separate gates. Nothing in this module weakens their default denial or creates a verified channel capability.

## Bounds

Public limits:128 channels,64 funding outpoints per descriptor,256 properties per allocation,4096 accepted revisions and8MiB revision metadata. Channel/revision/history limits are constants for the coordinated owner; this pure module owns no accumulated history and therefore cannot enforce that owner's aggregate limit itself. Descriptor input is capped at4096 bytes and signed envelope input at16,384 bytes, before parsing/copies/curve arithmetic. Maximum valid descriptor/body sizes are smaller under the fixed account/count bounds. Parsers check counts and required remaining fixed-row bytes before Vec allocation. Encoders preflight borrowed counts/order/amount bounds before building canonical bytes. No attacker-selected bignum size or exponent enters Rust verification.

## Independent evidence and commands

The new-policy TSV contains81 records:

- **19 V** valid exact D/L/P/envelope/digest/head transitions: first and second revisions on all three networks,12 deterministic pools and a maximum case with256 properties,64 outpoints,90-byte accounts, full-u64 epoch and exact u128 maximum totals.
- **30 S** genuine signatures and independent expected outcomes: correct slots, swapped signers, high-S for each slot, zero/order scalars,16 signature-bit mutations, allocation mutation and policy-ID mutation.
- **32 X** malformed canonical inputs: truncation, trailing bytes, oversize/count bounds, unknown policy/purpose/binding tags, wrong genesis, duplicate keys/parties/outpoints/properties, off-curve key, unsafe heights, reversed window and overflowing/zero allocations.

The script's encoders use Node Buffer and BigInt, independently of Rust. Every valid signature is produced by genuine tiny-secp256k1 2.2.4 and checked by separate affine BigInt/extended-Euclid ECDSA and Node/OpenSSL. OpenSSL verifies ECDSA-SHA256 with SHA256(P) as its input message, producing the exact SHA256d(P) digest. Signature edge cases are also independently checked by the affine implementation. Default `--check` uses the preserved genuine signatures; it does not fabricate signatures or require a downloaded package. Supplying `--tiny-root` additionally checks the genuine pinned package.

The package version and exact JS/WASM SHA256s are pinned as in the existing public secp suite: index.cjs `3864bb04fb2a7a663038301f312010f462142c2555763600ae8eb3a7ecda8b01`, validate.cjs `f0638d147bac3b60764b2b68f79d72cdccad52523fcd0a98599b32e0873f58e2`, wasm `5cbbcb8adc7acc3b78a3fd31c76d573302705ff5fd714d03f5a2602591197cb5`. Node24.19.0/OpenSSL3.5.7 generated this corpus; runtime patch versions are reported without becoming acceptance rules.

```text
node scripts/channel_revision_vectors.cjs --check
node scripts/channel_revision_vectors.cjs --check --tiny-root <pinned-JS-package-owner>
node scripts/channel_revision_vectors.cjs --write --tiny-root <pinned-JS-package-owner>
```

`--sign-request <JSONfile>` and the exported `testOnlySignRequest` are fixture-only tools using fixed public test secrets1 and2. They accept a JSON policy/revision, require its two keys match those fixed test keys and its policy ID match D, and return canonical bytes plus two genuine signatures. All u64/u128 request amounts are decimal strings. No private-key input or signer exists in production Rust; the helper is for the funded owner's real-Ledger fixture generation only.

Eight owned Rust integration tests consume all81 records and independently test every signed body field, policy/context/custody mutations, exact chained heads, sequence overflow, reused mathematically valid old signatures, complete-property conservation, caps, every byte-prefix truncation, current/new allocation digests, unavailable admission/revocation and canonical round trips. The scratch harness additionally includes two existing SHA helper tests; do not count those as new channel tests.

Direct pinned Rust1.90 debug and `-O` harness runs pass all10 tests against the actual baseline core rlib. Direct Clippy with `-D warnings`, pinned rustfmt, default Node check and genuine-package Node check pass. The optimized harness still links that existing core artifact; it is not a full release build or throughput benchmark. Parent centrally runs actual Cargo/integrated owner/full-suite/release checks after integration. This handoff alone makes no claim that those pending owner gates passed.

The five production/test/oracle/doc handoff files are frozen by `crypto-manifest.json`; scratch shims, executables and validation logs are not production files. No TradeLayer checkout files, Cargo configuration, exports, CI or reserved external lanes were edited by this transcript lane, and no commit/push was made. The parent owns the combined reviewed commit.

## Integrated repository validation

The parent integrated this code and ran the actual pinned Rust1.90 repository:
1,199 top-level tests plus one subprocess check pass across148 executable
targets. Formatting (including included implementation files), all-target frozen
Clippy with warnings denied, all11 supply-chain mutation tests and the all-target
frozen release build pass. The final run held553 implementation/check inputs
unchanged. The source corpus has445 exact observations and46 explicit domain
cases; new-policy references cover81 transcript vectors and12 real-bootstrap
signed envelopes. See [ROADMAP_PROGRESS.md](ROADMAP_PROGRESS.md) for scope and
remaining gates. These checks grant no L1 admission or revocation authority.
