//! First slice of authenticated chain admission: one observed raw Bitcoin
//! block becomes TradeLayer candidates, each naming a sender proven by a
//! verified input signature and a transaction proven included in the block.
//!
//! Read-only. Nothing here applies a candidate to a ledger, and no result is a
//! `TrustedChainAdapter`. See `docs/ADMISSION.md` for the security argument,
//! the sender rule and what stays outside this slice.
//!
//! Sender rule (the legacy `txUtils.getSender`): input 0 selects the parent
//! output, and the sender is that output's address. Here that address must
//! also belong to a key whose SIGHASH_ALL signature over the whole spending
//! transaction verifies. The parent output (amount and scriptPubKey) comes
//! from an untrusted [`ParentOutputSource`]; a wrong claim fails verification
//! and cannot produce a forged sender.

use crate::bitcoin_inclusion::{
    BitcoinPowProfile, CheckedHeader, IncludedTransaction, InclusionError, MerklePath,
};
use crate::bitcoin_standard_input::{
    bitcoin_input_outpoint, check_bitcoin_input_against_claimed_parent_output, BitcoinInputError,
    BitcoinInputTemplate, BITCOIN_STANDARD_INPUT_FLAGS,
};
use crate::bitcoin_supported_transaction::BITCOIN_MAX_MONEY_SATOSHIS;
use crate::bitcoin_witness_binding::{
    inspect_bitcoin_witness_binding, BitcoinWitnessRules, ExpectedWitnessBlock, WitnessBindingError,
};
use crate::chain::{BlockId, Tip};
use crate::hash160::hash160;
use crate::hash256::double_sha256;
use crate::read_only::{inspect_transaction, Carrier, ScanError};

/// Candidates examined per block. Later candidates are refused, not skipped.
pub const MAX_ADMISSION_CANDIDATES: usize = 20_000;
/// Largest claimed parent scriptPubKey the verifier accepts from a source.
pub const MAX_PARENT_SCRIPT_BYTES: usize = 10_000;
/// The input whose parent output selects the sender (legacy `getSender`).
pub const SENDER_INPUT: u32 = 0;

/// An output being spent, with the transaction id in internal (hashing) order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParentOutpoint {
    pub txid_raw: [u8; 32],
    pub vout: u32,
}

/// A claimed parent output. Nothing about it is trusted by the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentOutput {
    pub amount_satoshis: u64,
    pub script_pubkey: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentSourceError {
    Unavailable,
    InvalidResponse,
    /// The source holds two different claims for one outpoint.
    Inconsistent,
}
impl core::fmt::Display for ParentSourceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ParentSourceError {}

/// Supplies the amount and scriptPubKey of the output an input spends. The
/// answer is a claim: admission verifies the spending signature against it.
pub trait ParentOutputSource {
    /// `Ok(None)` means the source holds no such output.
    fn parent_output(
        &mut self,
        outpoint: &ParentOutpoint,
    ) -> Result<Option<ParentOutput>, ParentSourceError>;
}

/// Address encoding of the observed network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionNetwork {
    Mainnet,
    /// Testnet3, testnet4 and signet.
    Testnet,
    Regtest,
}

/// Expected block identity and rules, held independently of the block bytes.
/// Height, network membership and activation remain caller claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionContext {
    pub height: u64,
    /// Expected header hash, display order.
    pub block: BlockId,
    /// Expected parent header hash, display order.
    pub parent: BlockId,
    pub pow: BitcoinPowProfile,
    pub rules: BitcoinWitnessRules,
    pub network: AdmissionNetwork,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionError {
    Header(InclusionError),
    WrongBlockHash,
    WrongParent,
    Witness(WitnessBindingError),
    Transaction(ScanError),
}
impl core::fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AdmissionError {}

/// Why an input does not authenticate a sender in this slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnauthenticatedReason {
    /// Input 0 is the coinbase's null outpoint.
    CoinbaseInput,
    /// Not P2PKH or P2WPKH: P2SH, wrapped segwit, multisig, P2WSH, taproot and
    /// every other template.
    UnsupportedInputType,
    /// A hash type other than SIGHASH_ALL (0x01).
    UnsupportedSighash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefusalReason {
    /// The transaction carries no TradeLayer carrier.
    NotACandidate,
    /// More than one carrier; the legacy source reads one.
    AmbiguousCarriers {
        count: usize,
    },
    /// The carrier's syntax is invalid.
    MalformedCarrier {
        status: &'static str,
    },
    CandidateLimit,
    MalformedTransaction(ScanError),
    Inclusion(InclusionError),
    InputMalformed(BitcoinInputError),
    Unauthenticated(UnauthenticatedReason),
    ParentSource(ParentSourceError),
    ParentMissing,
    /// Amount above MAX_MONEY or an oversized script.
    ParentOutputInvalid,
    /// The signature relation failed against the claimed parent output.
    Verification(BitcoinInputError),
    /// The verified key's address differs from the claimed parent address.
    SenderMismatch,
}

/// The party proven to have signed the spending transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSender {
    /// Address of the verified key on the observed network.
    pub address: String,
    /// The claimed parent scriptPubKey, equal to the verified key's script.
    pub script_pubkey: Vec<u8>,
    pub template: BitcoinInputTemplate,
    pub public_key: Vec<u8>,
    pub parent: ParentOutpoint,
    /// The claimed parent amount; a verified commitment only when
    /// `parent_amount_is_signed` is true (P2WPKH).
    pub parent_amount_satoshis: u64,
    pub parent_amount_is_signed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateOutcome {
    Authenticated {
        /// Display order. All-zero only when the bytes did not parse.
        txid: [u8; 32],
        position: u32,
        sender: Box<AuthenticatedSender>,
        payload: Carrier,
        inclusion: IncludedTransaction,
    },
    Refused {
        txid: [u8; 32],
        position: u32,
        reason: RefusalReason,
    },
}
impl CandidateOutcome {
    pub fn txid_display(&self) -> [u8; 32] {
        match self {
            Self::Authenticated { txid, .. } | Self::Refused { txid, .. } => *txid,
        }
    }
    pub fn position(&self) -> u32 {
        match self {
            Self::Authenticated { position, .. } | Self::Refused { position, .. } => *position,
        }
    }
    pub const fn is_authenticated(&self) -> bool {
        matches!(self, Self::Authenticated { .. })
    }
}

fn refuse(txid: [u8; 32], position: u32, reason: RefusalReason) -> CandidateOutcome {
    CandidateOutcome::Refused {
        txid,
        position,
        reason,
    }
}

/// Turns one raw block into authenticated candidates, in block order. A
/// header, parent, witness-commitment or transaction-boundary failure is an
/// `AdmissionError` for the whole block; per-candidate failures are
/// `Refused` outcomes. Transactions without a TradeLayer carrier produce no
/// outcome.
pub fn admit_block<S: ParentOutputSource + ?Sized>(
    raw_block: &[u8],
    context: &AdmissionContext,
    source: &mut S,
) -> Result<Vec<CandidateOutcome>, AdmissionError> {
    let header_bytes = raw_block
        .get(..80)
        .ok_or(AdmissionError::Header(InclusionError::HeaderSize))?;
    let header =
        CheckedHeader::check_header_at_claimed_height(context.height, header_bytes, context.pow)
            .map_err(AdmissionError::Header)?;
    if header.tip().id != context.block {
        return Err(AdmissionError::WrongBlockHash);
    }
    if header.parent() != context.parent {
        return Err(AdmissionError::WrongParent);
    }
    // Rebuilds the txid Merkle root against the header and the BIP141 witness
    // commitment, so the witness bytes verified below are the block's own.
    let relation = inspect_bitcoin_witness_binding(
        raw_block,
        ExpectedWitnessBlock {
            tip: Tip {
                height: context.height,
                id: header.tip().id,
            },
            parent: context.parent,
            rules: context.rules,
        },
    )
    .map_err(AdmissionError::Witness)?;
    let count = relation.transaction_count();
    let mut raws = Vec::with_capacity(count);
    let mut observations = Vec::with_capacity(count);
    for position in 0..count {
        let raw = relation
            .raw_transaction(position)
            .ok_or(AdmissionError::Transaction(ScanError::Truncated))?;
        observations.push(inspect_transaction(raw).map_err(AdmissionError::Transaction)?);
        raws.push(raw);
    }
    let mut outcomes = Vec::new();
    if observations.iter().all(|o| o.carriers.is_empty()) {
        return Ok(outcomes);
    }
    let levels = merkle_levels(
        &observations
            .iter()
            .map(|o| o.txid)
            .collect::<Vec<[u8; 32]>>(),
    );
    let mut examined = 0usize;
    for (position, raw) in raws.iter().enumerate() {
        if observations[position].carriers.is_empty() {
            continue;
        }
        examined += 1;
        if examined > MAX_ADMISSION_CANDIDATES {
            let mut txid = observations[position].txid;
            txid.reverse();
            outcomes.push(refuse(txid, position as u32, RefusalReason::CandidateLimit));
            continue;
        }
        let proof = merkle_path(&levels, position);
        outcomes.push(admit_included_transaction(
            &header,
            context.network,
            raw,
            &proof,
            source,
        ));
    }
    Ok(outcomes)
}

/// Authenticates one transaction against a locally checked header and a txid
/// Merkle branch. A txid branch does not bind witness bytes: [`admit_block`]
/// does that through the BIP141 commitment, and a caller of this function must
/// bring the same assurance for the witness it passes. When `raw_tx` does not
/// parse, the outcome carries an all-zero txid.
pub fn admit_included_transaction<S: ParentOutputSource + ?Sized>(
    header: &CheckedHeader,
    network: AdmissionNetwork,
    raw_tx: &[u8],
    proof: &MerklePath,
    source: &mut S,
) -> CandidateOutcome {
    let position = proof.tx_index;
    let observation = match inspect_transaction(raw_tx) {
        Ok(observation) => observation,
        Err(error) => {
            return refuse(
                [0; 32],
                position,
                RefusalReason::MalformedTransaction(error),
            )
        }
    };
    let mut txid = observation.txid;
    txid.reverse();
    let carrier = match observation.carriers.as_slice() {
        [] => return refuse(txid, position, RefusalReason::NotACandidate),
        [carrier] => carrier,
        many => {
            return refuse(
                txid,
                position,
                RefusalReason::AmbiguousCarriers { count: many.len() },
            )
        }
    };
    if !matches!(carrier.status, "typed-syntax" | "opaque-type") {
        return refuse(
            txid,
            position,
            RefusalReason::MalformedCarrier {
                status: carrier.status,
            },
        );
    }
    let inclusion = match header.check_transaction_inclusion(raw_tx, proof) {
        Ok(inclusion) => inclusion,
        Err(error) => return refuse(txid, position, RefusalReason::Inclusion(error)),
    };
    match authenticate_sender(network, raw_tx, source) {
        Ok(sender) => CandidateOutcome::Authenticated {
            txid,
            position,
            sender: Box::new(sender),
            payload: carrier.clone(),
            inclusion,
        },
        Err(reason) => refuse(txid, position, reason),
    }
}

#[derive(Clone, Copy)]
enum Spendable {
    P2pkh([u8; 20]),
    P2wpkh([u8; 20]),
}
fn classify_parent_script(script: &[u8]) -> Option<Spendable> {
    if script.len() == 25 && script[..3] == [0x76, 0xa9, 20] && script[23..] == [0x88, 0xac] {
        return Some(Spendable::P2pkh(script[3..23].try_into().ok()?));
    }
    if script.len() == 22 && script[..2] == [0, 20] {
        return Some(Spendable::P2wpkh(script[2..].try_into().ok()?));
    }
    None
}

fn authenticate_sender<S: ParentOutputSource + ?Sized>(
    network: AdmissionNetwork,
    raw_tx: &[u8],
    source: &mut S,
) -> Result<AuthenticatedSender, RefusalReason> {
    let (txid_raw, vout) =
        bitcoin_input_outpoint(raw_tx, SENDER_INPUT).map_err(RefusalReason::InputMalformed)?;
    if txid_raw == [0; 32] && vout == u32::MAX {
        return Err(RefusalReason::Unauthenticated(
            UnauthenticatedReason::CoinbaseInput,
        ));
    }
    let parent = ParentOutpoint { txid_raw, vout };
    let output = source
        .parent_output(&parent)
        .map_err(RefusalReason::ParentSource)?
        .ok_or(RefusalReason::ParentMissing)?;
    if output.amount_satoshis > BITCOIN_MAX_MONEY_SATOSHIS
        || output.script_pubkey.len() > MAX_PARENT_SCRIPT_BYTES
    {
        return Err(RefusalReason::ParentOutputInvalid);
    }
    let claimed = classify_parent_script(&output.script_pubkey).ok_or(
        RefusalReason::Unauthenticated(UnauthenticatedReason::UnsupportedInputType),
    )?;
    let satisfaction = check_bitcoin_input_against_claimed_parent_output(
        BITCOIN_STANDARD_INPUT_FLAGS,
        raw_tx,
        SENDER_INPUT,
        output.amount_satoshis,
        &output.script_pubkey,
    )
    .map_err(|error| match error {
        BitcoinInputError::UnsupportedHashType => {
            RefusalReason::Unauthenticated(UnauthenticatedReason::UnsupportedSighash)
        }
        BitcoinInputError::UnsupportedFundingScript => {
            RefusalReason::Unauthenticated(UnauthenticatedReason::UnsupportedInputType)
        }
        other => RefusalReason::Verification(other),
    })?;
    let key_hash = hash160(satisfaction.public_key());
    let (template, claimed_hash) = match claimed {
        Spendable::P2pkh(hash) => (BitcoinInputTemplate::P2pkh, hash),
        Spendable::P2wpkh(hash) => (BitcoinInputTemplate::P2wpkh, hash),
    };
    if satisfaction.template() != template || key_hash != claimed_hash {
        return Err(RefusalReason::SenderMismatch);
    }
    Ok(AuthenticatedSender {
        address: sender_address(network, template, &key_hash),
        script_pubkey: output.script_pubkey,
        template,
        public_key: satisfaction.public_key().to_vec(),
        parent,
        parent_amount_satoshis: output.amount_satoshis,
        parent_amount_is_signed: satisfaction.amount_is_signed(),
    })
}

fn merkle_levels(txids: &[[u8; 32]]) -> Vec<Vec<[u8; 32]>> {
    let mut levels = vec![txids.to_vec()];
    while levels.last().is_some_and(|level| level.len() > 1) {
        let level = levels.last().expect("nonempty levels");
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let mut concat = [0u8; 64];
            concat[..32].copy_from_slice(&pair[0]);
            concat[32..].copy_from_slice(pair.get(1).unwrap_or(&pair[0]));
            next.push(double_sha256(&concat));
        }
        levels.push(next);
    }
    levels
}

fn merkle_path(levels: &[Vec<[u8; 32]>], position: usize) -> MerklePath {
    let mut siblings_raw = Vec::new();
    let mut index = position;
    for level in &levels[..levels.len() - 1] {
        siblings_raw.push(level.get(index ^ 1).copied().unwrap_or(level[index]));
        index /= 2;
    }
    MerklePath {
        tx_index: position as u32,
        tx_count: levels[0].len() as u32,
        siblings_raw,
    }
}

/// The address a P2PKH or P2WPKH key hash has on `network`.
pub fn sender_address(
    network: AdmissionNetwork,
    template: BitcoinInputTemplate,
    key_hash: &[u8; 20],
) -> String {
    match template {
        BitcoinInputTemplate::P2wpkh => bech32_v0_address(
            match network {
                AdmissionNetwork::Mainnet => "bc",
                AdmissionNetwork::Testnet => "tb",
                AdmissionNetwork::Regtest => "bcrt",
            },
            key_hash,
        ),
        _ => base58check(
            match network {
                AdmissionNetwork::Mainnet => 0x00,
                AdmissionNetwork::Testnet | AdmissionNetwork::Regtest => 0x6f,
            },
            key_hash,
        ),
    }
}

const BECH32_CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
fn bech32_polymod(values: &[u8]) -> u32 {
    const GENERATOR: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
    let mut checksum = 1u32;
    for value in values {
        let top = checksum >> 25;
        checksum = ((checksum & 0x1ff_ffff) << 5) ^ u32::from(*value);
        for (bit, generator) in GENERATOR.iter().enumerate() {
            if (top >> bit) & 1 == 1 {
                checksum ^= generator;
            }
        }
    }
    checksum
}
fn bech32_v0_address(hrp: &str, program: &[u8; 20]) -> String {
    let mut data = vec![0u8];
    let (mut accumulator, mut bits) = (0u32, 0u32);
    for byte in program {
        accumulator = (accumulator << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            data.push(((accumulator >> bits) & 31) as u8);
        }
    }
    if bits > 0 {
        data.push(((accumulator << (5 - bits)) & 31) as u8);
    }
    let mut values: Vec<u8> = hrp.bytes().map(|b| b >> 5).collect();
    values.push(0);
    values.extend(hrp.bytes().map(|b| b & 31));
    values.extend(&data);
    values.extend([0; 6]);
    let polymod = bech32_polymod(&values) ^ 1;
    let mut text = format!("{hrp}1");
    for value in &data {
        text.push(BECH32_CHARSET[usize::from(*value)] as char);
    }
    for index in 0..6 {
        text.push(BECH32_CHARSET[((polymod >> (5 * (5 - index))) & 31) as usize] as char);
    }
    text
}

fn base58check(version: u8, hash: &[u8; 20]) -> String {
    const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut payload = vec![version];
    payload.extend(hash);
    let checksum = double_sha256(&payload);
    payload.extend(&checksum[..4]);
    let leading = payload.iter().take_while(|byte| **byte == 0).count();
    let mut digits: Vec<u8> = Vec::new();
    for byte in &payload {
        let mut carry = u32::from(*byte);
        for digit in digits.iter_mut() {
            carry += u32::from(*digit) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut text = String::new();
    text.extend(std::iter::repeat_n('1', leading));
    text.extend(
        digits
            .iter()
            .rev()
            .map(|digit| ALPHABET[usize::from(*digit)] as char),
    );
    text
}
