use std::collections::BTreeMap;
use tradelayer_core::admission::*;
use tradelayer_core::admission_rpc::*;
use tradelayer_core::bitcoin_inclusion::{
    BitcoinPowProfile, CheckedHeader, InclusionError, MerklePath,
};
use tradelayer_core::bitcoin_standard_input::{
    bitcoin_input_outpoint, BitcoinInputError, BitcoinInputTemplate,
};
use tradelayer_core::bitcoin_witness_binding::{BitcoinWitnessRules, WitnessBindingError};
use tradelayer_core::chain::BlockId;
use tradelayer_core::client::RpcValue;
use tradelayer_core::raw_block_ingest::{merkle_root_unverified, UnverifiedRawBlockView};
use tradelayer_core::read_only::{hex_bytes, inspect_transaction, ScanError};

const FIXTURE: &str = include_str!("../fixtures/admission_vectors.tsv");
const PAYLOAD: &[u8] = b"tl20;alice;5;2";
/// The coinbase witness reserved value; nonzero so it never equals a lone zero wtxid root.
const RESERVED: [u8; 32] = [7; 32];

fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

struct Vector {
    spending: Vec<u8>,
    amount: u64,
    script: Vec<u8>,
}
fn vector(name: &str) -> Vector {
    let row: Vec<&str> = FIXTURE
        .lines()
        .find(|line| line.starts_with(&format!("{name}\t")))
        .unwrap()
        .split('\t')
        .collect();
    Vector {
        spending: unhex(row[5]),
        amount: row[3].parse().unwrap(),
        script: unhex(row[4]),
    }
}

fn varint(n: usize) -> Vec<u8> {
    match n {
        0..=252 => vec![n as u8],
        253..=65535 => [vec![253], (n as u16).to_le_bytes().to_vec()].concat(),
        _ => [vec![254], (n as u32).to_le_bytes().to_vec()].concat(),
    }
}
fn push_script(payload: &[u8]) -> Vec<u8> {
    [vec![0x6a, payload.len() as u8], payload.to_vec()].concat()
}
fn outputs(list: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let mut bytes = varint(list.len());
    for (amount, script) in list {
        bytes.extend(amount.to_le_bytes());
        bytes.extend(varint(script.len()));
        bytes.extend(script);
    }
    bytes
}
/// A legacy transaction with no signature; it only has to parse.
fn unsigned_tx(seed: u8, outs: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let mut bytes = 2u32.to_le_bytes().to_vec();
    bytes.push(1);
    bytes.extend([seed; 32]);
    bytes.extend(0u32.to_le_bytes());
    bytes.push(0);
    bytes.extend(0xffff_fffdu32.to_le_bytes());
    bytes.extend(outputs(outs));
    bytes.extend(0u32.to_le_bytes());
    bytes
}
fn plain_tx(seed: u8) -> Vec<u8> {
    unsigned_tx(seed, &[(1000, vec![0x51])])
}
fn coinbase(extra: &[(u64, Vec<u8>)], commitment: [u8; 32]) -> Vec<u8> {
    let mut outs = vec![(0u64, vec![0x51])];
    outs.extend(extra.iter().cloned());
    outs.push((
        0,
        [
            vec![0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed],
            commitment.to_vec(),
        ]
        .concat(),
    ));
    let mut bytes = 2u32.to_le_bytes().to_vec();
    bytes.extend([0, 1, 1]);
    bytes.extend([0u8; 32]);
    bytes.extend([0xff; 4]);
    bytes.extend([3, 0x01, 0x01, 0x51]);
    bytes.extend([0xff; 4]);
    bytes.extend(outputs(&outs));
    bytes.extend([1, 32]);
    bytes.extend(RESERVED);
    bytes.extend(0u32.to_le_bytes());
    bytes
}
fn txid(raw: &[u8]) -> [u8; 32] {
    inspect_transaction(raw).unwrap().txid
}
fn display(mut hash: [u8; 32]) -> [u8; 32] {
    hash.reverse();
    hash
}

/// Builds and mines a synthetic regtest-difficulty block whose coinbase
/// carries a correct BIP141 witness commitment. Returns the block, its id and
/// its txids in internal order.
fn build_block(
    parent: BlockId,
    extra_coinbase_outputs: &[(u64, Vec<u8>)],
    rest: &[Vec<u8>],
) -> (Vec<u8>, BlockId, Vec<[u8; 32]>) {
    let mut wtxids = vec![[0u8; 32]];
    wtxids.extend(
        rest.iter()
            .map(|raw| inspect_transaction(raw).unwrap().wtxid),
    );
    let witness_root = merkle_root_unverified(&wtxids).unwrap();
    // double-SHA256 of root || nonce, the two-leaf Merkle root of the pair.
    let commitment = merkle_root_unverified(&[witness_root, RESERVED]).unwrap();
    let base = coinbase(extra_coinbase_outputs, commitment);
    let mut transactions = vec![base];
    transactions.extend(rest.iter().cloned());
    let txids: Vec<[u8; 32]> = transactions.iter().map(|raw| txid(raw)).collect();
    let root = merkle_root_unverified(&txids).unwrap();
    let mut previous = parent.0;
    previous.reverse();
    let mut block = Vec::new();
    for nonce in 0u32.. {
        let mut header = 0x2000_0000u32.to_le_bytes().to_vec();
        header.extend(previous);
        header.extend(root);
        header.extend(1_700_000_000u32.to_le_bytes());
        header.extend(0x207f_ffffu32.to_le_bytes());
        header.extend(nonce.to_le_bytes());
        if CheckedHeader::check_header_at_claimed_height(5, &header, BitcoinPowProfile::RegtestLike)
            .is_ok()
        {
            block = header;
            break;
        }
    }
    block.extend(varint(transactions.len()));
    for raw in &transactions {
        block.extend(raw);
    }
    let view = UnverifiedRawBlockView::parse(&block, None).unwrap();
    let id = BlockId(display(view.header_hash_raw()));
    (block, id, txids)
}

const PARENT: BlockId = BlockId([0x44; 32]);
fn context(block: BlockId) -> AdmissionContext {
    AdmissionContext {
        height: 5,
        block,
        parent: PARENT,
        pow: BitcoinPowProfile::RegtestLike,
        rules: BitcoinWitnessRules::SegwitActive,
        network: AdmissionNetwork::Regtest,
    }
}

#[derive(Default)]
struct MapSource {
    outputs: BTreeMap<ParentOutpoint, ParentOutput>,
    failure: Option<ParentSourceError>,
    asked: Vec<ParentOutpoint>,
}
impl MapSource {
    fn with(vectors: &[&Vector]) -> Self {
        let mut source = Self::default();
        for v in vectors {
            source.insert(v, v.amount, v.script.clone());
        }
        source
    }
    fn insert(&mut self, v: &Vector, amount: u64, script: Vec<u8>) {
        let (txid_raw, vout) = bitcoin_input_outpoint(&v.spending, 0).unwrap();
        self.outputs.insert(
            ParentOutpoint { txid_raw, vout },
            ParentOutput {
                amount_satoshis: amount,
                script_pubkey: script,
            },
        );
    }
}
impl ParentOutputSource for MapSource {
    fn parent_output(
        &mut self,
        outpoint: &ParentOutpoint,
    ) -> Result<Option<ParentOutput>, ParentSourceError> {
        self.asked.push(*outpoint);
        if let Some(error) = self.failure {
            return Err(error);
        }
        Ok(self.outputs.get(outpoint).cloned())
    }
}

fn admit(txs: &[Vec<u8>], source: &mut MapSource) -> Vec<CandidateOutcome> {
    let (block, id, _) = build_block(PARENT, &[], txs);
    admit_block(&block, &context(id), source).unwrap()
}
fn single_reason(outcomes: Vec<CandidateOutcome>) -> RefusalReason {
    assert_eq!(outcomes.len(), 1, "{outcomes:?}");
    match outcomes.into_iter().next().unwrap() {
        CandidateOutcome::Refused { reason, .. } => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn p2wpkh_sighash_all_transaction_is_authenticated_with_inclusion() {
    let v = vector("p2wpkh-all");
    let mut source = MapSource::with(&[&v]);
    let (block, id, txids) = build_block(PARENT, &[], std::slice::from_ref(&v.spending));
    let outcomes = admit_block(&block, &context(id), &mut source).unwrap();
    assert_eq!(outcomes.len(), 1);
    let CandidateOutcome::Authenticated {
        txid: found,
        position,
        sender,
        payload,
        inclusion,
    } = &outcomes[0]
    else {
        panic!("not authenticated: {:?}", outcomes[0]);
    };
    assert_eq!(*found, display(txids[1]));
    assert_eq!(*position, 1);
    assert_eq!(sender.template, BitcoinInputTemplate::P2wpkh);
    assert_eq!(sender.script_pubkey, v.script);
    assert!(sender.address.starts_with("bcrt1q"));
    assert_eq!(
        sender.address,
        sender_address(
            AdmissionNetwork::Regtest,
            BitcoinInputTemplate::P2wpkh,
            &v.script[2..].try_into().unwrap()
        )
    );
    assert!(sender.parent_amount_is_signed);
    assert_eq!(sender.parent_amount_satoshis, 10_000);
    assert_eq!(sender.public_key.len(), 33);
    assert_eq!(payload.payload, PAYLOAD);
    assert_eq!(payload.transaction_type, Some(2));
    assert_eq!(payload.status, "typed-syntax");
    assert_eq!(payload.vout, 0);
    assert_eq!(inclusion.block_id(), id);
    assert_eq!(inclusion.height(), 5);
    assert_eq!(inclusion.txid_display(), display(txids[1]));
    assert_eq!((inclusion.tx_index(), inclusion.tx_count()), (1, 2));
    assert_eq!(source.asked.len(), 1);
}

#[test]
fn p2pkh_sighash_all_transaction_is_authenticated_without_a_signed_amount() {
    let v = vector("p2pkh-all");
    let mut source = MapSource::with(&[&v]);
    let outcomes = admit(std::slice::from_ref(&v.spending), &mut source);
    let CandidateOutcome::Authenticated { sender, .. } = &outcomes[0] else {
        panic!("not authenticated: {:?}", outcomes[0]);
    };
    assert_eq!(sender.template, BitcoinInputTemplate::P2pkh);
    assert!(!sender.parent_amount_is_signed);
    assert!(sender.address.starts_with('m') || sender.address.starts_with('n'));
    // The legacy digest does not commit to the amount, so a wrong amount is
    // reported but not detected; the sender is still the signer.
    let mut wrong = MapSource::default();
    wrong.insert(&v, 777, v.script.clone());
    let outcomes = admit(std::slice::from_ref(&v.spending), &mut wrong);
    let CandidateOutcome::Authenticated { sender, .. } = &outcomes[0] else {
        panic!("not authenticated: {:?}", outcomes[0]);
    };
    assert_eq!(sender.parent_amount_satoshis, 777);
    assert!(!sender.parent_amount_is_signed);
}

#[test]
fn address_encodings_match_published_vectors() {
    let hash: [u8; 20] = unhex("751e76e8199196d454941c45d1b3a323f1433bd6")
        .try_into()
        .unwrap();
    assert_eq!(
        sender_address(
            AdmissionNetwork::Mainnet,
            BitcoinInputTemplate::P2wpkh,
            &hash
        ),
        "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"
    );
    assert_eq!(
        sender_address(
            AdmissionNetwork::Testnet,
            BitcoinInputTemplate::P2wpkh,
            &hash
        ),
        "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx"
    );
    assert_eq!(
        sender_address(
            AdmissionNetwork::Mainnet,
            BitcoinInputTemplate::P2pkh,
            &hash
        ),
        "1BgGZ9tcN4rm9KBzDn7KprQz87SZ26SAMH"
    );
    assert_eq!(
        sender_address(
            AdmissionNetwork::Mainnet,
            BitcoinInputTemplate::P2pkh,
            &[0; 20]
        ),
        "1111111111111111111114oLvT2"
    );
}

#[test]
fn candidates_are_found_across_an_odd_transaction_count_and_other_inputs_are_not_asked() {
    let first = vector("p2wpkh-two-inputs");
    let second = vector("p2pkh-all");
    let mut source = MapSource::with(&[&first, &second]);
    let txs = vec![
        plain_tx(1),
        first.spending.clone(),
        plain_tx(2),
        second.spending.clone(),
    ];
    let outcomes = admit(&txs, &mut source);
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[0].position(), 2);
    assert_eq!(outcomes[1].position(), 4);
    assert!(outcomes.iter().all(CandidateOutcome::is_authenticated));
    let CandidateOutcome::Authenticated { inclusion, .. } = &outcomes[1] else {
        unreachable!()
    };
    assert_eq!((inclusion.tx_index(), inclusion.tx_count()), (4, 5));
    // Only input 0 selects the sender; input 1 of the first transaction is
    // never looked up, and its witness is not verified in this slice.
    assert_eq!(source.asked.len(), 2);
}

#[test]
fn wrong_parent_amount_is_refused_for_p2wpkh() {
    let v = vector("p2wpkh-all");
    let mut source = MapSource::default();
    source.insert(&v, v.amount + 1, v.script.clone());
    assert_eq!(
        single_reason(admit(std::slice::from_ref(&v.spending), &mut source)),
        RefusalReason::Verification(BitcoinInputError::InvalidSignature { slot: 0 })
    );
}

#[test]
fn wrong_parent_script_is_refused() {
    let v = vector("p2wpkh-all");
    let other = vector("p2pkh-all");
    // Another key's P2WPKH program.
    let mut foreign = v.script.clone();
    foreign[5] ^= 0xff;
    let mut source = MapSource::default();
    source.insert(&v, v.amount, foreign);
    assert_eq!(
        single_reason(admit(std::slice::from_ref(&v.spending), &mut source)),
        RefusalReason::Verification(BitcoinInputError::PublicKeyHashMismatch)
    );
    // The same key claimed as P2PKH: the witness is unexpected there.
    let mut source = MapSource::default();
    source.insert(&v, v.amount, other.script.clone());
    assert!(matches!(
        single_reason(admit(std::slice::from_ref(&v.spending), &mut source)),
        RefusalReason::Verification(_)
    ));
    // The signer of a P2PKH spend claimed under another key's hash.
    let mut foreign = other.script.clone();
    foreign[5] ^= 0xff;
    let mut source = MapSource::default();
    source.insert(&other, other.amount, foreign);
    assert_eq!(
        single_reason(admit(std::slice::from_ref(&other.spending), &mut source)),
        RefusalReason::Verification(BitcoinInputError::PublicKeyHashMismatch)
    );
}

#[test]
fn sighash_none_and_single_are_unauthenticated() {
    for name in ["p2wpkh-none", "p2wpkh-single", "p2pkh-none"] {
        let v = vector(name);
        let mut source = MapSource::with(&[&v]);
        assert_eq!(
            single_reason(admit(std::slice::from_ref(&v.spending), &mut source)),
            RefusalReason::Unauthenticated(UnauthenticatedReason::UnsupportedSighash),
            "{name}"
        );
    }
}

#[test]
fn unsupported_input_types_are_unauthenticated() {
    // A genuinely signed P2SH-wrapped P2WPKH spend.
    let wrapped = vector("wrapped-p2wpkh-all");
    let mut source = MapSource::with(&[&wrapped]);
    assert_eq!(
        single_reason(admit(std::slice::from_ref(&wrapped.spending), &mut source)),
        RefusalReason::Unauthenticated(UnauthenticatedReason::UnsupportedInputType)
    );
    // Claimed P2WSH and taproot parents for a P2WPKH spend.
    let v = vector("p2wpkh-all");
    for script in [
        [vec![0, 32], vec![7; 32]].concat(),
        [vec![0x51, 32], vec![7; 32]].concat(),
        vec![0x51],
        Vec::new(),
    ] {
        let mut source = MapSource::default();
        source.insert(&v, v.amount, script);
        assert_eq!(
            single_reason(admit(std::slice::from_ref(&v.spending), &mut source)),
            RefusalReason::Unauthenticated(UnauthenticatedReason::UnsupportedInputType)
        );
    }
}

#[test]
fn a_source_cannot_relabel_a_wrapped_spend_as_native_p2wpkh() {
    // The wrapped spend's BIP143 signature covers the same key and script code as a native
    // P2WPKH spend of that key. A lying source that claims the native output must still be
    // refused: the legacy sender is the P2SH address, so accepting would credit another address.
    let wrapped = vector("wrapped-p2wpkh-all");
    let native = vector("p2wpkh-all");
    let mut source = MapSource::default();
    source.insert(&wrapped, wrapped.amount, native.script.clone());
    // `single_reason` panics unless the one outcome is a refusal.
    let reason = single_reason(admit(std::slice::from_ref(&wrapped.spending), &mut source));
    eprintln!("wrapped spend relabelled as native P2WPKH: {reason:?}");
}

#[test]
fn a_tampered_payload_byte_breaks_the_signature() {
    for name in ["p2wpkh-all", "p2pkh-all"] {
        let v = vector(name);
        let at = v
            .spending
            .windows(PAYLOAD.len())
            .position(|window| window == PAYLOAD)
            .unwrap();
        let mut forged = v.spending.clone();
        forged[at + 11] = b'6';
        let mut source = MapSource::with(&[&v]);
        let outcomes = admit(std::slice::from_ref(&forged), &mut source);
        let CandidateOutcome::Refused { reason, txid, .. } = &outcomes[0] else {
            panic!("tampered {name} was admitted");
        };
        assert_eq!(
            *reason,
            RefusalReason::Verification(BitcoinInputError::InvalidSignature { slot: 0 })
        );
        assert_eq!(*txid, display(self::txid(&forged)));
    }
}

#[test]
fn a_transaction_outside_the_block_is_not_included() {
    let v = vector("p2wpkh-all");
    let (block, id, txids) = build_block(PARENT, &[], std::slice::from_ref(&v.spending));
    let header = CheckedHeader::check_header_at_claimed_height(
        5,
        &block[..80],
        BitcoinPowProfile::RegtestLike,
    )
    .unwrap();
    assert_eq!(header.tip().id, id);
    let proof = MerklePath {
        tx_index: 1,
        tx_count: 2,
        siblings_raw: vec![txids[0]],
    };
    let mut source = MapSource::with(&[&v]);
    let outcome = admit_included_transaction(
        &header,
        AdmissionNetwork::Regtest,
        &v.spending,
        &proof,
        &mut source,
    );
    assert!(outcome.is_authenticated());
    // The same proof with a foreign transaction fails against the header root.
    let foreign = vector("p2pkh-all");
    let mut source = MapSource::with(&[&foreign]);
    let outcome = admit_included_transaction(
        &header,
        AdmissionNetwork::Regtest,
        &foreign.spending,
        &proof,
        &mut source,
    );
    let CandidateOutcome::Refused { reason, .. } = outcome else {
        panic!("a foreign transaction was admitted");
    };
    assert_eq!(
        reason,
        RefusalReason::Inclusion(InclusionError::MerkleRootMismatch)
    );
    assert!(source.asked.is_empty(), "no parent lookup before inclusion");
    // A proof with a wrong index or count is invalid.
    for (tx_index, tx_count) in [(0, 2), (5, 2), (1, 0)] {
        let bad = MerklePath {
            tx_index,
            tx_count,
            siblings_raw: vec![txids[0]],
        };
        let outcome = admit_included_transaction(
            &header,
            AdmissionNetwork::Regtest,
            &v.spending,
            &bad,
            &mut source,
        );
        assert!(!outcome.is_authenticated());
    }
}

#[test]
fn hostile_and_truncated_bytes_never_panic_or_admit() {
    let v = vector("p2wpkh-all");
    let (block, id, _) = build_block(PARENT, &[], &[v.spending.clone(), plain_tx(9)]);
    let ctx = context(id);
    let mut source = MapSource::with(&[&v]);
    for length in (0..block.len())
        .step_by(7)
        .chain([0, 1, 79, 80, 81, block.len() - 1])
    {
        assert!(
            admit_block(&block[..length], &ctx, &mut source).is_err(),
            "prefix {length}"
        );
    }
    let mut longer = block.clone();
    longer.push(0);
    assert!(admit_block(&longer, &ctx, &mut source).is_err());
    for at in 0..block.len() {
        for mask in [0x01u8, 0x80, 0xff] {
            let mut hostile = block.clone();
            hostile[at] ^= mask;
            // Any single-byte change breaks the header, a txid, or the
            // witness commitment: nothing may be authenticated.
            assert!(
                admit_block(&hostile, &ctx, &mut source).is_err(),
                "byte {at} mask {mask:#x}"
            );
        }
    }
    // Hostile single transactions through the lower-level entry point.
    let header = CheckedHeader::check_header_at_claimed_height(
        5,
        &block[..80],
        BitcoinPowProfile::RegtestLike,
    )
    .unwrap();
    let proof = MerklePath {
        tx_index: 1,
        tx_count: 2,
        siblings_raw: vec![[0; 32]],
    };
    for raw in [
        &v.spending[..v.spending.len() - 1],
        &v.spending[..10],
        &[][..],
        &[0xffu8; 64][..],
    ] {
        let outcome = admit_included_transaction(
            &header,
            AdmissionNetwork::Regtest,
            raw,
            &proof,
            &mut source,
        );
        assert!(matches!(
            outcome,
            CandidateOutcome::Refused {
                reason: RefusalReason::MalformedTransaction(_),
                txid: [0, ..],
                ..
            }
        ));
    }
}

#[test]
fn block_level_context_failures_are_typed_errors() {
    let v = vector("p2wpkh-all");
    let (block, id, _) = build_block(PARENT, &[], std::slice::from_ref(&v.spending));
    let mut source = MapSource::with(&[&v]);
    let mut ctx = context(id);
    ctx.block = BlockId([1; 32]);
    assert_eq!(
        admit_block(&block, &ctx, &mut source),
        Err(AdmissionError::WrongBlockHash)
    );
    let mut ctx = context(id);
    ctx.parent = BlockId([2; 32]);
    assert_eq!(
        admit_block(&block, &ctx, &mut source),
        Err(AdmissionError::WrongParent)
    );
    let mut ctx = context(id);
    ctx.pow = BitcoinPowProfile::MainnetLike;
    assert_eq!(
        admit_block(&block, &ctx, &mut source),
        Err(AdmissionError::Header(InclusionError::HeaderTooEasy))
    );
    // Witness bytes outside a declared BIP141 context are refused.
    let mut ctx = context(id);
    ctx.rules = BitcoinWitnessRules::NoWitness;
    assert_eq!(
        admit_block(&block, &ctx, &mut source),
        Err(AdmissionError::Witness(
            WitnessBindingError::UnexpectedWitness
        ))
    );
    assert!(source.asked.is_empty());
}

#[test]
fn a_witness_swapped_into_the_block_fails_the_commitment() {
    // Two valid P2WPKH-looking blocks that differ only in witness bytes share
    // a txid, so only the BIP141 commitment tells them apart.
    let v = vector("p2wpkh-all");
    let (block, id, _) = build_block(PARENT, &[], std::slice::from_ref(&v.spending));
    let mut swapped = block.clone();
    // Inside the signature: 4 locktime bytes, a 33-byte key and its length push.
    let at = swapped.len() - 4 - 34 - 5;
    swapped[at] ^= 1;
    let mut source = MapSource::with(&[&v]);
    assert_eq!(
        admit_block(&swapped, &context(id), &mut source),
        Err(AdmissionError::Witness(
            WitnessBindingError::CommitmentMismatch
        ))
    );
}

#[test]
fn carrier_and_parent_failures_are_typed_refusals() {
    let v = vector("p2wpkh-all");
    // No carrier: no outcome at all.
    let mut source = MapSource::with(&[&v]);
    assert!(admit(&[plain_tx(3)], &mut source).is_empty());
    // Two carriers.
    let two = unsigned_tx(
        4,
        &[(0, push_script(PAYLOAD)), (0, push_script(b"tl20;bob;5;2"))],
    );
    assert_eq!(
        single_reason(admit(&[two], &mut source)),
        RefusalReason::AmbiguousCarriers { count: 2 }
    );
    // A carrier with an invalid frame.
    let bad = unsigned_tx(5, &[(0, push_script(b"tl!"))]);
    assert_eq!(
        single_reason(admit(&[bad], &mut source)),
        RefusalReason::MalformedCarrier {
            status: "invalid-frame"
        }
    );
    assert!(source.asked.is_empty());
    // Missing, failing, and out-of-range parent outputs.
    let mut empty = MapSource::default();
    assert_eq!(
        single_reason(admit(std::slice::from_ref(&v.spending), &mut empty)),
        RefusalReason::ParentMissing
    );
    let mut failing = MapSource::with(&[&v]);
    failing.failure = Some(ParentSourceError::Unavailable);
    assert_eq!(
        single_reason(admit(std::slice::from_ref(&v.spending), &mut failing)),
        RefusalReason::ParentSource(ParentSourceError::Unavailable)
    );
    let mut huge = MapSource::default();
    huge.insert(&v, 21_000_000 * 100_000_000 + 1, v.script.clone());
    assert_eq!(
        single_reason(admit(std::slice::from_ref(&v.spending), &mut huge)),
        RefusalReason::ParentOutputInvalid
    );
    let mut long = MapSource::default();
    long.insert(&v, v.amount, vec![0x51; MAX_PARENT_SCRIPT_BYTES + 1]);
    assert_eq!(
        single_reason(admit(std::slice::from_ref(&v.spending), &mut long)),
        RefusalReason::ParentOutputInvalid
    );
}

#[test]
fn a_coinbase_carrier_has_no_sender() {
    let (block, id, _) = build_block(PARENT, &[(0, push_script(PAYLOAD))], &[]);
    let mut source = MapSource::default();
    let outcomes = admit_block(&block, &context(id), &mut source).unwrap();
    assert_eq!(
        single_reason(outcomes),
        RefusalReason::Unauthenticated(UnauthenticatedReason::CoinbaseInput)
    );
    assert!(source.asked.is_empty());
}

fn getblock_json(rows: &[(&str, u32, &str, &str)], hash: &str) -> RpcValue {
    let inputs: Vec<String> = rows
        .iter()
        .map(|(txid, vout, value, script)| {
            format!(
                "{{\"txid\":\"{txid}\",\"vout\":{vout},\"prevout\":{{\"generated\":false,\"height\":1,\"value\":{value},\"scriptPubKey\":{{\"asm\":\"\",\"hex\":\"{script}\",\"type\":\"x\"}}}},\"sequence\":1}}"
            )
        })
        .collect();
    RpcValue::parse(&format!(
        "{{\"hash\":\"{hash}\",\"tx\":[{{\"txid\":\"00\",\"vin\":[{{\"coinbase\":\"0101\",\"sequence\":1}}]}},{{\"txid\":\"11\",\"vin\":[{}]}}]}}",
        inputs.join(",")
    ))
    .unwrap()
}

#[test]
fn rpc_prevout_source_feeds_admission_from_getblock_verbosity_3() {
    let v = vector("p2wpkh-all");
    let (block, id, _) = build_block(PARENT, &[], std::slice::from_ref(&v.spending));
    let (txid_raw, vout) = bitcoin_input_outpoint(&v.spending, 0).unwrap();
    let txid_text = hex_bytes(&display(txid_raw));
    let id_text = hex_bytes(&id.0);
    let script = hex_bytes(&v.script);
    let json = getblock_json(&[(&txid_text, vout, "0.00010000", &script)], &id_text);
    let mut source = RpcPrevoutSource::from_block_json(id, &json).unwrap();
    assert_eq!(source.len(), 1);
    let outcomes = admit_block(&block, &context(id), &mut source).unwrap();
    assert!(outcomes[0].is_authenticated(), "{outcomes:?}");
    // A node that lies about the amount cannot forge or alter the sender.
    let lie = getblock_json(&[(&txid_text, vout, "0.00010001", &script)], &id_text);
    let mut source = RpcPrevoutSource::from_block_json(id, &lie).unwrap();
    assert_eq!(
        single_reason(admit_block(&block, &context(id), &mut source).unwrap()),
        RefusalReason::Verification(BitcoinInputError::InvalidSignature { slot: 0 })
    );
    // Amount lexemes: exact eight-decimal forms parse; others are refused.
    for (value, ok) in [
        ("0.00010000", true),
        ("1", true),
        ("0.1", true),
        ("20999999.99999999", true),
        ("1e-05", false),
        ("0.000100001", false),
        ("-0.1", false),
    ] {
        let json = getblock_json(&[(&txid_text, vout, value, &script)], &id_text);
        assert_eq!(
            RpcPrevoutSource::from_block_json(id, &json).is_ok(),
            ok,
            "{value}"
        );
    }
    // The response must be for the requested block.
    let other = getblock_json(
        &[(&txid_text, vout, "0.0001", &script)],
        &hex_bytes(&[9; 32]),
    );
    assert_eq!(
        RpcPrevoutSource::from_block_json(id, &other),
        Err(PrevoutSourceError::WrongBlock)
    );
    // Two different claims for one outpoint are inconsistent, not silently merged.
    let json = getblock_json(
        &[
            (&txid_text, vout, "0.0001", &script),
            (&txid_text, vout, "0.0002", &script),
        ],
        &id_text,
    );
    let mut source = RpcPrevoutSource::from_block_json(id, &json).unwrap();
    assert_eq!(
        single_reason(admit_block(&block, &context(id), &mut source).unwrap()),
        RefusalReason::ParentSource(ParentSourceError::Inconsistent)
    );
    // An input without a prevout defines nothing.
    let no_prevout = RpcValue::parse(&format!(
        "{{\"hash\":\"{id_text}\",\"tx\":[{{\"txid\":\"11\",\"vin\":[{{\"txid\":\"{txid_text}\",\"vout\":{vout},\"sequence\":1}}]}}]}}"
    ))
    .unwrap();
    let mut source = RpcPrevoutSource::from_block_json(id, &no_prevout).unwrap();
    assert!(source.is_empty());
    assert_eq!(
        single_reason(admit_block(&block, &context(id), &mut source).unwrap()),
        RefusalReason::ParentMissing
    );
}

#[test]
fn truncated_raw_transaction_error_is_reported_with_the_scan_cause() {
    let v = vector("p2pkh-all");
    let (block, _, _) = build_block(PARENT, &[], std::slice::from_ref(&v.spending));
    let header = CheckedHeader::check_header_at_claimed_height(
        5,
        &block[..80],
        BitcoinPowProfile::RegtestLike,
    )
    .unwrap();
    let proof = MerklePath {
        tx_index: 1,
        tx_count: 2,
        siblings_raw: vec![[0; 32]],
    };
    let mut source = MapSource::default();
    let outcome = admit_included_transaction(
        &header,
        AdmissionNetwork::Regtest,
        &v.spending[..v.spending.len() - 3],
        &proof,
        &mut source,
    );
    assert!(matches!(
        outcome,
        CandidateOutcome::Refused {
            reason: RefusalReason::MalformedTransaction(ScanError::Truncated),
            ..
        }
    ));
}
