# ZTB-02A: canonical statement and deposit accumulator

Status: executable research fixture only. No actual NEAR contract, chain proof, Nova circuit, Rust custody integration, or sandbox deployment. The mock object capabilities stand in for authenticated runtime receipts and a verified exit. They are deliberately callable by the harness and grant no actual authority.

## Domain and statement

Hash H is SHA-256. Tagged hash = H(ASCII tag || 00 || ordered fields). Account IDs are canonical ASCII (2..64 bytes); named hash uses one byte length then account bytes. Integers are unsigned little-endian, digest fields are 32 raw bytes (no Bitcoin display/internal conversion implied; adapter must freeze that boundary). Domain = tagged TLBRIDGE/DOMAIN/V1 over NEAR genesis, Bitcoin genesis, TL activation, policy, named token hash, named vault hash and u32 property. The token and vault named tags are TLBRIDGE/TOKEN/V1 and TLBRIDGE/VAULT/V1. Changing the asset, chain, deployment or policy changes the domain.

Statement length 392 bytes:
- ASCII TLSTMT01 (8).
- Ten 32-byte digests in order: domain, program, key, previousRoot, nextRoot, parentBlock, tipBlock, nearFinalBlock, depositRoot, exitRoot.
- Eight u64le: epoch, firstHeight, lastHeight, depositCount, exitCount, deposited, minted, burned.

Reject truncation, trailing data, unknown fields, unsigned overflow, reversed intervals, zero epoch, unchanged economic root, and D < M or M < B. The cryptographic receiver independently pins the expected domain, program, key, previous state and finalized roots; the codec alone validates syntax and simple inequalities. Domain commits the policy and asset identities. Full transition relation must separately enforce block continuity, exact counts, complete order and authorization; replay protected receiver advances via compare-and-swap. Root must commit the interval/tip even when no economic events occur. A SHA256 digest is split into two unsigned 128-bit big-endian limbs; no modular reduction. Bitcoin packet = TLBRDG01 || SHA256(statement), exactly 40 bytes. Proof verification must recompute the digest from constrained bytes; never accept a host-supplied hash as evidence.

## Deposits

Deposit identity = tagged TLBRIDGE/DEPOSIT-ID/V1(domain, terminal receipt ID, u32 event index). The mock receipt ID represents a pinned transfer identity whose terminal resolution is separately authenticated. Real NEAR adapter must prove the link between the initiating receipt and terminal outcome. Nonce is not an identity. Leaf = tagged TLBRIDGE/DEPOSIT-LEAF/V1(domain, u64 leaf index, deposit ID, named TLBRIDGE/SENDER/V1 hash, Bitcoin recipient script hash, u64 actual retained atoms). Fixed-depth-16 ordered tree: empty leaf tagged TLBRIDGE/DEPOSIT-EMPTY/V1; parents tagged TLBRIDGE/DEPOSIT-NODE/V1(left,right); root tagged TLBRIDGE/DEPOSIT-ROOT/V1(u64 count, tree root). Index determines direction bits. Capacity 65,536 deposits before a separately versioned rollover protocol; reject overflow. The reference recomputes the tree; production uses an incremental frontier. Membership authenticates the root only; NEARHEAD01/NEARPAY01 must establish the actual finalized reserve record.

On mock transfer, authenticate token predecessor, record pending; after terminal successful resolution append actual retained amount; zero retained creates no leaf; partial refund credits only retained atoms. The real NEP-141 call graph must be audited: ft_on_transfer returns an unused amount and terminal resolution occurs later. The vault cannot treat receiver invocation as final settlement. No permissionless callback or caller-supplied result may authorize minting. A later relay must authenticate the execution or finalized state proof and transfer identity.

## Outgoing payout

Register only a verified exit, reserve its exact amount, pin recipient. Pending -> InFlight(attempt) -> Paid or Pending after authenticated failed transfer. Single mock callback ticket per attempt. If transfer succeeded and callback is unavailable, leave in-flight; never retry on timeout. Reconcile proof of exact terminal external transfer before marking paid. The mock distinguishes physical token holdings and recorded payouts; it conservatively reserves amounts until callback. Contract implementation must additionally handle outgoing paid-but-unrecorded obligations, gas/storage funding, promise result parsing, issuer freeze, token registration, reentrancy and crash recovery. No arbitrary transfer/admin escape.

## Reproduction

Run node test_ztb02.mjs and python3 ztb02_oracle.py in this directory. The JS harness exercises 43 assertions against the codec and mock state machine; the initial author executed the same function bodies with an independent SHA-256 implementation checked against the abc known-answer test. Node and Python commands are committed for independent replay, not claimed executed until evidence exists. The frozen fixture includes domain, wire, digest, packet, limbs and deposit root. Next gate: actual NEAR Rust mock-token sandbox with successful and failed promise receipts and a review of the terminal transfer proof. Keep production_ready=false, can_mint=false, can_redeem=false.
