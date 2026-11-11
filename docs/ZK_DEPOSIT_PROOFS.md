# Deposit proofs: M16 attestations

Status: design 2026-11-11. The parser and the consumed-outpoint set are built
(`src/zk_deposit.rs`, `tests/zk_deposit.rs`). No consensus wiring exists.

M16 is an SP1 guest. Its BLAKE3 KoalaBear wrap proof (1,099,173 bytes) commits 1,292 public bytes:

| bytes | content |
|---|---|
| 0..1164 | guest A's chain statement: `"NHC1"`, u32 k, u32 steps, z0 (17 x 32), zn (17 x 32), sha256(compressed Nova key), proofs digest |
| 1164..1292 | `"PAY1"`, u32 segments, u32 height, block hash (32), u32 confirmations, txid (32), u32 vout, u64 value, u32 script length, sha256(script) (32) |

Integers are little-endian; txid and block hash are in Bitcoin internal byte order. The proof shows
that a valid SHA-256d proof-of-work chain runs from the Bitcoin genesis to the tip (`steps` headers),
that the transaction `txid` is in block `height` of that chain, and that its output `vout` has the
stated value and script hash. The real mainnet fixture (`fixtures/zk_m16_public.bin`, SHA-256
`cae6068f...c869`) has tip 975,072, payment block 975,067, 6 confirmations, 940,000 sats, 22-byte script.

## Decision

**The attestation enters at the listener and light-client layer, off consensus (candidate a).**
Consensus admission goes through candidate (c), a payment-inclusion step proved in Nova and checked
by the existing type 36 Nova verifier, as a later step. Candidate (b), verifying the SP1 wrap in
the zero-crate crate, is rejected.

### Why not (b): a type-36 SP1 profile

- Size of the port. `m13-verify` is 786 lines of glue, but it calls the SP1 6.9.0 and Plonky3 verifier.
  The crates on that path, counted from the pinned sources (`~/.cargo/registry`, lines of `src/*.rs`):
  `sp1-hypercube` 14,975; `sp1-recursion-machine` 4,151 (the chip AIRs the verifier evaluates);
  `sp1-verifier` 3,077; `sp1-primitives` 2,019; `sp1-recursion-executor` 2,595; `slop-jagged` 3,270;
  `slop-multilinear` 2,153; `slop-tensor` 1,448; `slop-basefold`, `-sumcheck`, `-merkle-tree`,
  `-stacked`, `-algebra`, `-challenger`, `-koala-bear` about 3,100 together; `p3-koala-bear`
  5,004; `p3-field` 1,998; `blake3` 8,193. That is roughly 50,000 lines to read, of which a
  clean-room port of the reachable subset is 15,000 to 25,000 lines. The in-crate Nova verifier is
  4,078 lines. The port is 4 to 6 times larger, and its only oracle is SP1 itself.
- Wire size. `MAX_ZK_FRAME_BYTES` is 128 KiB and type 36 carries small proofs inline in OP_RETURN
  (`ZK_VERIFIER.md`, Data availability). The wrap proof is 1.1 MB, so it would need the witness
  envelope that the STARK profile uses, which is not built.
- Verification cost. `m13-verify` takes seconds per proof on one thread, against a per-block
  budget in `ZkBlockBudget` sized for Nova (milliseconds to tens of milliseconds).
- Upgrade coupling. The wrap key, recursion key map and transcript version belong to SP1; an SP1
  release is a new verifier version and a re-audit of a 20,000-line port.

### Why not consensus now

TradeLayer consensus must decide on state every node can replay. A deposit credit needs the header
chain to be the heaviest chain, which the proof does not show (see "What a light client cannot
know"). Consensus has the real chain through the node's own Bitcoin Core, so the SPV attestation adds
nothing to a full node. It is useful exactly where there is no full node: a wallet or listener that
credits a deposit from a proof. That is the (a) path.

### Why (a) first

- It uses the M16 proof as built, with `m13-verify` as the native verifier in the listener, and
  needs no consensus change.
- The parser and policy are small, dependency-free and fully tested, so a listener embeds them
  without trusting more than SP1 and the pins.
- Credits made this way are the listener's, not the ledger's. A node never replays them.

### Path to consensus: (c)

A deposit step proved directly in Nova: the NHC1 chain state extended with one step that proves
a transaction output is in a header of the chain (txid Merkle path, output decode, script hash,
value). The registered program is one more type 36 Nova program; the existing
`ZkNovaAdapter` verifies it, proofs stay at 12-13 KB inline, and the program key pins the
circuit. This needs a circuit, a key and a prover run, none of which exist, and a Merkle
step over up to 20 siblings per proof. The M16 guest is the specification: the same record fields.
The statement receipt (`ZkEffect::StatementReceipt`) is the effect; a deposit credit is a custody
movement and therefore phase 2 of `ZK_VERIFIER.md`, gated on the state-root commitment.

## Checks the parser makes

`parse_public_values(raw, policy)` returns a `DepositAttestation` or a typed `DepositRefusal`:

- length is exactly 1,292; chain magic `NHC1`; k = 1 (one header per Nova step); steps > 0;
- sha256 of the 544 z0 bytes equals the pinned genesis state
  `b6eeaf96...f045`;
- sha256 of the compressed Nova key equals the pinned mainnet key `6aa50d09...da1d`;
- PAY1 magic; segments > 0; height <= tip; `confirmations == steps - height + 1`;
- `confirmations >= policy.min_confirmations`; script length <= 10,000 (Bitcoin's script limit);
  value > 0.

`check_wrap_statement(statement, raw, policy)` first requires the wrap statement's program key to
be the M16 key `dfc68a5d...cb60` and the public values to hash to its `committed_values_sha256`.
The caller verifies the wrap proof itself (`m13-verify verify`) and passes the statement it
produced. The parser alone proves nothing about the bytes.

The pinned program key is not inside the public values; it comes from the wrap statement. A
listener therefore needs both: the proof check, then this module.

## Replay protection

`ConsumedOutpoints` is a map from `(txid, vout)` to the block hash it was credited under.
`consume` refuses a second credit for the same outpoint whatever block hash the later
attestation names, so a reorg that re-confirms the transaction, or a fresh proof of the same
payment at a later tip, cannot credit it twice. The set belongs to whoever credits (the listener's
store). If the listener also credits through a full node's index, both paths use one set.

A deposit is addressed to a script the credited party controls, so a payment attested for one
user is not creditable to another: the listener compares `script_sha256` against the script
registered for the account (`DepositAttestation::pays_script`) and the value against the
expected amount. An attacker who copies a proof from a mempool or log gains nothing, because the
credit goes to the account whose registered script the output pays, not to the submitter.

## Type 36 binding prefix

Type 36 programs begin their public inputs with a binding element over genesis hash, economic
anchor, authenticated sender, expiry and program id (`binding_element`). M16's statement has no
such element: guest A's z0 is the genesis state, which is the chain commitment, not a per-sender
binding. That is why M16 is not a type 36 program as it stands. Route (c) adds one state element
to z0 for the binding, as `ZK_VERIFIER_NOVA.md` specifies for circuits whose z0 is otherwise fully
meaningful; the deposit step then carries sender and expiry, so a proof is not copyable out of the
mempool. Off consensus, the replay set and the registered script take that role.

## Confirmations and reorg depth

`confirmations` is the proven distance from the payment block to the proven tip, and the parser
checks it equals `steps - height + 1`. The policy threshold is the consumer's: it is the reorg
depth the consumer accepts.
A listener crediting small values can use fewer. A reorg deeper than the threshold after the
credit is not handled by the proof; the consumed-outpoint set records the block hash credited so
the listener can detect that the credited block left the chain and reconcile it.

## What a light client still cannot know

- **The heaviest chain.** The proof is a valid SHA-256d chain from genesis, not the most-work chain.
  An attacker with enough hash power can produce an alternative valid chain, and
  the proof binds the chainwork in `zn`, which the
  consumer must compare to chainwork it learns from independent sources (peers, a block explorer,
  its own header sync). `DepositAttestation::tip_state` exposes `zn` for that comparison.
  The parser does not make that comparison.
- **TradeLayer state.** Whether the deposit was already credited under TradeLayer rules, the
  account's registered script, the property being credited, and any pending transactions are
  ledger facts. The attestation says an output exists; it does not say the output is unspent.
  Spent-ness is not proven; a deposit is final once credited, so the credit logic must not
  assume the output still exists.
- **Output ownership beyond the script hash.** The proof commits the script hash only; the script
  itself is learned from the user and checked with `pays_script`.

## Bitcoin first; Litecoin separate

The Nova chain step hashes SHA-256d, which is Bitcoin's proof of work. Litecoin's is Scrypt (N=1024),
too large to prove per Nova step. A Litecoin deposit proof needs a different construction (an SP1
Scrypt check per header batch, under the Nova chain of SHA-256d links or a separate chain) with its
own pins and its own parser constants; it is not a flag on this module. The parser's pins are
Bitcoin mainnet only, and a testnet4 statement (`NHT4`) is refused by chain magic. TradeLayer's
Litecoin paths keep their LTC parameters and are untouched.

## Next steps

1. Listener integration: call `m13-verify` as a library, then `check_wrap_statement`, then compare
   `tip_state` chainwork and `ConsumedOutpoints::consume`, in `tradelayer-listenerd`.
2. Candidate (c): a Nova payment-inclusion step and its registered program, with the binding element.
3. Testnet4 pins for a regtest or testnet4 listener (`NHT4` key and genesis state).
