# Authenticated chain admission, first slice

`src/admission.rs` turns one observed raw Bitcoin block into TradeLayer candidates. Each authenticated candidate names a sender proven by a verified input signature, and its transaction is proven included in the block by a Merkle branch checked against the block header. The slice is read-only: nothing is applied to a ledger, and no result implements `TrustedChainAdapter`.

## API

```rust
pub fn admit_block<S: ParentOutputSource + ?Sized>(
    raw_block: &[u8],
    context: &AdmissionContext,
    source: &mut S,
) -> Result<Vec<CandidateOutcome>, AdmissionError>;

pub trait ParentOutputSource {
    fn parent_output(&mut self, outpoint: &ParentOutpoint)
        -> Result<Option<ParentOutput>, ParentSourceError>;
}

pub enum CandidateOutcome {
    Authenticated { txid, position, sender: AuthenticatedSender, payload: Carrier, inclusion: IncludedTransaction },
    Refused { txid, position, reason: RefusalReason },
}
```

- `AdmissionContext` carries the expected header hash, the expected parent hash, the height, the proof-of-work profile, the BIP141 rule context and the address network. These are held independently of the block bytes. Height, network membership and activation remain caller claims.
- `admit_block` returns `Err(AdmissionError)` when the whole block cannot be trusted as bytes: header size or proof-of-work failure, wrong block hash, wrong parent, a transaction boundary or txid Merkle failure, a BIP141 witness-commitment failure, or an unexpected witness. Per-candidate failures are `Refused` outcomes. Transactions without a TradeLayer carrier produce no outcome.
- `admit_included_transaction` authenticates one transaction against a checked header and a Merkle branch. A txid branch does not bind witness bytes; `admit_block` binds them through the BIP141 commitment, and a direct caller of the lower-level function supplies that assurance itself.
- `txid` is in display order (as RPC prints it). `Refused` carries an all-zero txid only when the transaction bytes do not parse.
- `sender_address(network, template, key_hash)` encodes the P2WPKH (bech32 v0: `bc`, `tb`, `bcrt`) or P2PKH (Base58Check: version `0x00` mainnet, `0x6f` testnet and regtest) address.

Bounds: the raw block is at most 4,000,000 bytes (`MAX_UNVERIFIED_RAW_BLOCK_BYTES`); at most `MAX_ADMISSION_CANDIDATES` (20,000) candidates are examined per block and later ones are refused with `CandidateLimit`; a claimed parent script is at most `MAX_PARENT_SCRIPT_BYTES` (10,000); one parent lookup happens per candidate that passes the carrier and inclusion checks. All parsing returns errors on hostile bytes. The crate forbids `unsafe`.

## Candidates

A candidate is a transaction in the block with exactly one `OP_RETURN <canonical push>` carrier that starts with `tl` or `TLR` and whose syntax status is `typed-syntax` or `opaque-type` (`src/read_only.rs`, the detection `candidate_evidence.rs` and the txindex code use). More than one carrier is `AmbiguousCarriers`; an invalid frame or body is `MalformedCarrier`. The legacy decoder's non-canonical `746c` layouts are outside this slice.

## Sender rule

The legacy source (`tradelayer.js@dca0888b`, `src/txUtils.js::getSender`, ported in `src/txindex_source_owner.rs::sender` and described in `docs/LITECOIN_SOURCE_TRACE.md`) selects input 0, follows its outpoint to the parent output, and takes that output's `scriptPubKey` address as the sender, with the output value as the amount. Admission applies the same rule: input 0 selects the parent, and the sender is the address of the parent scriptPubKey. It adds the requirement that the key behind that address signed the spending transaction.

Input 0 only selects the sender, as in the source. The other inputs are not looked up and their witnesses are not verified in this slice.

## Security argument

For a SIGHASH_ALL signature on a P2WPKH input (BIP143) or a P2PKH input:

1. The signature commits to every output of the spending transaction, including the TradeLayer payload output, and to every outpoint.
2. For P2WPKH it also commits to the spent amount, and the script code derives from the witness public key.
3. The parent output (amount and scriptPubKey) can therefore come from an untrusted source. In this slice that source is the node's `getblock <hash> 3` prevout data. A wrong claimed script changes the script code or the key-hash check, and a wrong P2WPKH amount changes the digest; verification fails and the candidate is refused. A claim cannot make a key that did not sign the transaction verify, so it cannot produce a forged sender.
4. The sender is the address of the verified key. It must equal the address of the claimed parent scriptPubKey (`SenderMismatch` otherwise).
5. A legacy P2PKH digest does not commit to the amount. The outcome reports `parent_amount_is_signed: false` for P2PKH, and the amount stays an unauthenticated claim there.
6. Any other input type, any hash type other than SIGHASH_ALL (0x01), and a coinbase input are `Unauthenticated` with a typed reason (`UnsupportedInputType`, `UnsupportedSighash`, `CoinbaseInput`). SIGHASH_NONE, SIGHASH_SINGLE and ANYONECANPAY variants are among them, because they do not commit to all outputs.

Witness bytes are bound to the block. `admit_block` recomputes the txid Merkle root against the header and the BIP141 witness commitment (`inspect_bitcoin_witness_binding`), so the signature checked is the block's own witness and not a substituted one.

Inclusion: the candidate's txid Merkle branch is built from the block's txids and verified against the observed header's Merkle root with `CheckedHeader::check_transaction_inclusion` (`src/bitcoin_inclusion.rs`). The result is an `IncludedTransaction` with height, block id, position and transaction count. A transaction outside the block fails with `Inclusion(MerkleRootMismatch)` before any parent lookup.

The signature relation is `check_bitcoin_input_against_claimed_parent_output` in `src/bitcoin_standard_input.rs`. It reuses the same BIP143 and legacy digest code and the same ECDSA verifier as `check_bitcoin_standard_input`, restricted to P2PKH and P2WPKH under SIGHASH_ALL, and needs no parent transaction bytes.

## What the slice does not establish

- Parent existence, inclusion and unspentness. The parent output is a claim; a node that reports a different real output than the block spent cannot forge a sender, but the parent transaction is not proven in a block.
- The other inputs of the transaction, other templates (P2SH, wrapped segwit, multisig, P2WSH, taproot), complete transaction and block consensus, and the active chain.
- TradeLayer activation, sender authorization for a transaction type, and any ledger or economic effect.

## Parent source over RPC

`src/admission_rpc.rs` implements `ParentOutputSource` over `getblock <hash> 3`. `RpcPrevoutSource::fetch` makes one call through `RpcClient`; `from_block_json` reads a response. Each non-coinbase input's `prevout.value` (exactly eight decimals, anything else refused) and `prevout.scriptPubKey.hex` define its parent outpoint. The response `hash` must equal the requested block. Two different claims for one outpoint make that outpoint `Inconsistent`. A response above `MAX_RPC_BODY` (16 MiB) fails with `RpcError::Capacity`, which refuses the block's candidates; very large mainnet blocks reach that limit.

## CLI

`tl_admit --network BTC|BTC-TESTNET4 --rpc-address IP:PORT [--cookie-file PATH] (--height H | --block-hash HASH)` fetches the header, the raw block (`getblock <hash> 0`) and the prevouts, runs `admit_block`, and prints one JSON object with every outcome and `"applied_to_ledger":false`. It verifies the node's network first and writes nothing.

## Tests

`tests/admission.rs` builds mined regtest-difficulty blocks with a correct witness commitment around signed transactions from `fixtures/admission_vectors.tsv`. `scripts/generate_admission_vectors.py` regenerates that file from public throwaway scalars (`--check` compares bytes). The tests cover: authenticated P2WPKH and P2PKH senders with inclusion; address vectors; odd transaction counts; a wrong amount, wrong script, SIGHASH_NONE and SIGHASH_SINGLE, unsupported input types, a tampered payload byte, a non-included transaction, and hostile truncated and bit-flipped bytes; block-level context failures; carrier and parent failures; and the RPC prevout parser with a mock response.

## Live run, 2026-11-05

`tl_admit --network BTC-TESTNET4` (release build of this slice) ran against the witness box's
pruned Bitcoin Core 31.1 testnet4 node for heights 159,028 to 159,047: 20 blocks of 1 to 52
transactions, exit 0 for each, 16 to 33 ms per block including the `getblock` calls. Every block
passed the header, proof-of-work, Merkle and witness-commitment checks and reported 0 candidates,
since testnet4 carries no TradeLayer transactions. `applied_to_ledger` is false in every report.
