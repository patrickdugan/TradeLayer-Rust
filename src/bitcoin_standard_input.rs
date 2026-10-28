//! Selected-input satisfaction for explicit Bitcoin payment/channel templates.
//! This is not arbitrary Script, UTXO authentication or economic admission.
use crate::channel_witness_audit::Cursor;
use crate::hash160::hash160;
use crate::hash256::{double_sha256, sha256};
use crate::read_only::{inspect_transaction, ScanError, MAX_IO_COUNT, MAX_RAW_TRANSACTION};
use crate::secp256k1::{verify_ecdsa_digest, SecpError};

pub const BITCOIN_STANDARD_INPUT_FLAGS: u32 = 0x0e15;
const MAX_PREIMAGE: usize = 2 * MAX_RAW_TRANSACTION;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitcoinInputTemplate {
    P2pkh,
    P2wpkh,
    P2shTwoOfTwo,
    P2wshTwoOfTwo,
    P2shP2wpkh,
    P2shP2wshTwoOfTwo,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BitcoinInputError {
    RawTransaction(ScanError),
    UnsupportedFlags,
    FundingIndex,
    SpendingIndex,
    WrongPrevout,
    UnsupportedFundingScript,
    UnsupportedRedeemScript,
    UnlockingShape,
    NonPushScriptSig,
    NativeScriptSig,
    WrappedScriptSig,
    UnexpectedWitness,
    RedeemHashMismatch,
    WitnessHashMismatch,
    PublicKeyHashMismatch,
    MalformedSignature,
    UnsupportedHashType,
    UnsupportedPublicKeyEncoding,
    InvalidSignature { slot: usize },
    InvalidPublicKey { slot: usize },
    Cryptography(SecpError),
    Capacity,
}
impl From<ScanError> for BitcoinInputError {
    fn from(e: ScanError) -> Self {
        Self::RawTransaction(e)
    }
}
impl core::fmt::Display for BitcoinInputError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BitcoinInputError {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitcoinInputTrustError {
    FundingInclusionAndUnspentnessUnauthenticated,
    CompleteTransactionAndChainConsensusUnverified,
    TradeLayerActorAndEconomicAdmissionUnavailable,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitcoinSignatureSatisfaction {
    pub hash_type: u8,
    pub digest_raw: [u8; 32],
    pub legacy_single_bug: bool,
}
#[derive(Debug, Clone)]
pub struct BitcoinInputSatisfaction<'a> {
    funding: &'a [u8],
    spending: &'a [u8],
    funding_vout: u32,
    spending_vin: u32,
    amount: u64,
    template: BitcoinInputTemplate,
    keys: Vec<Vec<u8>>,
    signatures: Vec<BitcoinSignatureSatisfaction>,
    witness_amount: bool,
}
impl<'a> BitcoinInputSatisfaction<'a> {
    pub const fn raw_funding_transaction(&self) -> &'a [u8] {
        self.funding
    }
    pub const fn raw_spending_transaction(&self) -> &'a [u8] {
        self.spending
    }
    pub const fn funding_vout(&self) -> u32 {
        self.funding_vout
    }
    pub const fn spending_vin(&self) -> u32 {
        self.spending_vin
    }
    pub const fn funding_satoshis(&self) -> u64 {
        self.amount
    }
    pub const fn template(&self) -> BitcoinInputTemplate {
        self.template
    }
    pub fn actual_script_keys(&self) -> &[Vec<u8>] {
        &self.keys
    }
    pub fn signatures(&self) -> &[BitcoinSignatureSatisfaction] {
        &self.signatures
    }
    pub const fn witness_amount_is_signed(&self) -> bool {
        self.witness_amount
    }
    pub const fn require_authenticated_parent_utxo(&self) -> Result<(), BitcoinInputTrustError> {
        Err(BitcoinInputTrustError::FundingInclusionAndUnspentnessUnauthenticated)
    }
    pub const fn require_complete_transaction_consensus(
        &self,
    ) -> Result<(), BitcoinInputTrustError> {
        Err(BitcoinInputTrustError::CompleteTransactionAndChainConsensusUnverified)
    }
    pub const fn require_tradelayer_actor_admission(&self) -> Result<(), BitcoinInputTrustError> {
        Err(BitcoinInputTrustError::TradeLayerActorAndEconomicAdmissionUnavailable)
    }
}
type R<T> = Result<T, BitcoinInputError>;
struct Input<'a> {
    outpoint: &'a [u8],
    script: &'a [u8],
    sequence: &'a [u8],
    witness: Vec<&'a [u8]>,
}
struct Output<'a> {
    amount: u64,
    script: &'a [u8],
    serial: &'a [u8],
}
struct Transaction<'a> {
    version: &'a [u8],
    inputs: Vec<Input<'a>>,
    outputs: Vec<Output<'a>>,
    locktime: &'a [u8],
    txid: [u8; 32],
}
fn transaction(raw: &[u8], selected: Option<usize>) -> R<Transaction<'_>> {
    let observation = inspect_transaction(raw)?;
    let mut c = Cursor::new(raw);
    let version = c.take(4)?;
    if observation.has_witness && c.take(2)? != [0, 1] {
        return Err(BitcoinInputError::UnlockingShape);
    }
    let mut inputs = Vec::new();
    for _ in 0..c.count()? {
        inputs.push(Input {
            outpoint: c.take(36)?,
            script: c.data()?,
            sequence: c.take(4)?,
            witness: vec![],
        });
    }
    let mut outputs = Vec::new();
    for _ in 0..c.count()? {
        let at = c.offset();
        let amount = u64::from_le_bytes(c.take(8)?.try_into().expect("eight bytes"));
        let script = c.data()?;
        outputs.push(Output {
            amount,
            script,
            serial: &raw[at..c.offset()],
        });
    }
    if observation.has_witness {
        for (index, input) in inputs.iter_mut().enumerate() {
            let count = c.compact()?;
            if count > MAX_IO_COUNT {
                return Err(BitcoinInputError::Capacity);
            }
            if selected == Some(index) && count > 4 {
                return Err(BitcoinInputError::UnlockingShape);
            }
            for _ in 0..count {
                let item = c.data()?;
                if selected == Some(index) {
                    input.witness.push(item);
                }
            }
        }
    }
    let locktime = c.take(4)?;
    if c.offset() != raw.len() {
        return Err(ScanError::TrailingBytes.into());
    }
    Ok(Transaction {
        version,
        inputs,
        outputs,
        locktime,
        txid: observation.txid,
    })
}
fn pushes(script: &[u8]) -> R<Vec<Vec<u8>>> {
    let mut c = Cursor::new(script);
    let mut result = vec![];
    while c.offset() < script.len() {
        if result.len() == 4 {
            return Err(BitcoinInputError::UnlockingShape);
        }
        let opcode = c.take(1)?[0];
        let item = match opcode {
            0..=75 => c.take(opcode as usize)?.to_vec(),
            76 => {
                let n = c.take(1)?[0] as usize;
                c.take(n)?.to_vec()
            }
            77 => {
                let n = u16::from_le_bytes(c.take(2)?.try_into().expect("two bytes")) as usize;
                c.take(n)?.to_vec()
            }
            78 => {
                let n = u32::from_le_bytes(c.take(4)?.try_into().expect("four bytes")) as usize;
                c.take(n)?.to_vec()
            }
            79 => vec![0x81],
            81..=96 => vec![opcode - 80],
            _ => return Err(BitcoinInputError::NonPushScriptSig),
        };
        result.push(item);
    }
    Ok(result)
}
fn p2pkh(hash: &[u8]) -> Vec<u8> {
    [b"\x76\xa9\x14".as_slice(), hash, b"\x88\xac".as_slice()].concat()
}
fn multisig_keys(script: &[u8]) -> R<Vec<Vec<u8>>> {
    let mut c = Cursor::new(script);
    if c.take(1)? != [0x52] {
        return Err(BitcoinInputError::UnsupportedRedeemScript);
    }
    let mut keys = vec![];
    for _ in 0..2 {
        let n = c.take(1)?[0];
        if !matches!(n, 33 | 65) {
            return Err(BitcoinInputError::UnsupportedRedeemScript);
        }
        keys.push(c.take(n as usize)?.to_vec());
    }
    if c.take(2)? != [0x52, 0xae] || c.offset() != script.len() {
        return Err(BitcoinInputError::UnsupportedRedeemScript);
    }
    Ok(keys)
}
struct Unlock {
    template: BitcoinInputTemplate,
    script: Vec<u8>,
    keys: Vec<Vec<u8>>,
    signatures: Vec<Vec<u8>>,
    witness: bool,
}
fn witness_unlock(program: &[u8], input: &Input<'_>, wrapped: bool) -> R<Unlock> {
    if program.len() == 22 && program[..2] == [0, 20] {
        if input.witness.len() != 2 {
            return Err(BitcoinInputError::UnlockingShape);
        }
        let key = input.witness[1];
        if hash160(key) != program[2..] {
            return Err(BitcoinInputError::PublicKeyHashMismatch);
        }
        return Ok(Unlock {
            template: if wrapped {
                BitcoinInputTemplate::P2shP2wpkh
            } else {
                BitcoinInputTemplate::P2wpkh
            },
            script: p2pkh(&program[2..]),
            keys: vec![key.to_vec()],
            signatures: vec![input.witness[0].to_vec()],
            witness: true,
        });
    }
    if program.len() == 34 && program[..2] == [0, 32] {
        if input.witness.len() != 4 || !input.witness[0].is_empty() {
            return Err(BitcoinInputError::UnlockingShape);
        }
        let script = input.witness[3];
        if sha256(script) != program[2..] {
            return Err(BitcoinInputError::WitnessHashMismatch);
        }
        return Ok(Unlock {
            template: if wrapped {
                BitcoinInputTemplate::P2shP2wshTwoOfTwo
            } else {
                BitcoinInputTemplate::P2wshTwoOfTwo
            },
            script: script.to_vec(),
            keys: multisig_keys(script)?,
            signatures: vec![input.witness[1].to_vec(), input.witness[2].to_vec()],
            witness: true,
        });
    }
    Err(BitcoinInputError::UnsupportedRedeemScript)
}
fn unlocking(output: &Output<'_>, input: &Input<'_>) -> R<Unlock> {
    let spk = output.script;
    if spk.len() == 25 && spk[..3] == [0x76, 0xa9, 20] && spk[23..] == [0x88, 0xac] {
        if !input.witness.is_empty() {
            return Err(BitcoinInputError::UnexpectedWitness);
        }
        let stack = pushes(input.script)?;
        if stack.len() != 2 {
            return Err(BitcoinInputError::UnlockingShape);
        }
        if hash160(&stack[1]) != spk[3..23] {
            return Err(BitcoinInputError::PublicKeyHashMismatch);
        }
        return Ok(Unlock {
            template: BitcoinInputTemplate::P2pkh,
            script: spk.to_vec(),
            keys: vec![stack[1].clone()],
            signatures: vec![stack[0].clone()],
            witness: false,
        });
    }
    if (spk.len() == 22 && spk[..2] == [0, 20]) || (spk.len() == 34 && spk[..2] == [0, 32]) {
        if !input.script.is_empty() {
            return Err(BitcoinInputError::NativeScriptSig);
        }
        return witness_unlock(spk, input, false);
    }
    if spk.len() == 23 && spk[..2] == [0xa9, 20] && spk[22] == 0x87 {
        let stack = pushes(input.script)?;
        let redeem = stack.last().ok_or(BitcoinInputError::UnlockingShape)?;
        if hash160(redeem) != spk[2..22] {
            return Err(BitcoinInputError::RedeemHashMismatch);
        }
        if (redeem.len() == 22 && redeem[..2] == [0, 20])
            || (redeem.len() == 34 && redeem[..2] == [0, 32])
        {
            let mut canonical = vec![redeem.len() as u8];
            canonical.extend(redeem);
            if stack.len() != 1 || input.script != canonical {
                return Err(BitcoinInputError::WrappedScriptSig);
            }
            return witness_unlock(redeem, input, true);
        }
        if !input.witness.is_empty() {
            return Err(BitcoinInputError::UnexpectedWitness);
        }
        if stack.len() != 4 || !stack[0].is_empty() {
            return Err(BitcoinInputError::UnlockingShape);
        }
        return Ok(Unlock {
            template: BitcoinInputTemplate::P2shTwoOfTwo,
            script: redeem.clone(),
            keys: multisig_keys(redeem)?,
            signatures: vec![stack[1].clone(), stack[2].clone()],
            witness: false,
        });
    }
    Err(BitcoinInputError::UnsupportedFundingScript)
}
fn signature(item: &[u8]) -> R<([u8; 64], u8)> {
    if !(9..=73).contains(&item.len())
        || item[0] != 0x30
        || usize::from(item[1]) != item.len() - 3
        || item[2] != 2
    {
        return Err(BitcoinInputError::MalformedSignature);
    }
    let nr = usize::from(item[3]);
    if nr == 0 || 5 + nr >= item.len() || item[4 + nr] != 2 {
        return Err(BitcoinInputError::MalformedSignature);
    }
    let ns = usize::from(item[5 + nr]);
    if ns == 0 || nr + ns + 7 != item.len() {
        return Err(BitcoinInputError::MalformedSignature);
    }
    let mut compact = [0; 64];
    for (slot, integer) in [&item[4..4 + nr], &item[6 + nr..6 + nr + ns]]
        .into_iter()
        .enumerate()
    {
        if integer[0] & 0x80 != 0
            || (integer.len() > 1 && integer[0] == 0 && integer[1] & 0x80 == 0)
        {
            return Err(BitcoinInputError::MalformedSignature);
        }
        let integer = if integer.len() > 1 && integer[0] == 0 {
            &integer[1..]
        } else {
            integer
        };
        if integer.len() > 32 {
            return Err(BitcoinInputError::MalformedSignature);
        }
        let end = (slot + 1) * 32;
        compact[end - integer.len()..end].copy_from_slice(integer);
    }
    let kind = item[item.len() - 1];
    if !matches!(kind, 1 | 2 | 3 | 0x81 | 0x82 | 0x83) {
        return Err(BitcoinInputError::UnsupportedHashType);
    }
    Ok((compact, kind))
}
fn compact(out: &mut Vec<u8>, n: usize) {
    if n < 253 {
        out.push(n as u8)
    } else if n <= 65535 {
        out.push(253);
        out.extend((n as u16).to_le_bytes())
    } else {
        out.push(254);
        out.extend((n as u32).to_le_bytes())
    }
}
// Legacy CHECKSIG/CHECKMULTISIG removes canonical signature pushes at opcode
// boundaries before hashing. A short signature could equal P2PKH's pushed
// hash bytes; fixed templates alone do not justify omitting this rule.
fn legacy_script_code(script: &[u8], signatures: &[Vec<u8>]) -> R<Vec<u8>> {
    let mut code = script.to_vec();
    for signature in signatures {
        if !(9..=73).contains(&signature.len()) {
            return Err(BitcoinInputError::MalformedSignature);
        }
        let mut needle = vec![signature.len() as u8];
        needle.extend(signature);
        let mut c = Cursor::new(&code);
        let mut next = Vec::with_capacity(code.len());
        while c.offset() < code.len() {
            if code[c.offset()..].starts_with(&needle) {
                c.take(needle.len())?;
                continue;
            }
            let start = c.offset();
            let opcode = c.take(1)?[0];
            let length = match opcode {
                0..=75 => usize::from(opcode),
                76 => usize::from(c.take(1)?[0]),
                77 => u16::from_le_bytes(c.take(2)?.try_into().expect("two bytes")) as usize,
                78 => u32::from_le_bytes(c.take(4)?.try_into().expect("four bytes")) as usize,
                _ => 0,
            };
            c.take(length)?;
            next.extend(&code[start..c.offset()]);
        }
        code = next;
    }
    Ok(code)
}
fn legacy_digest(t: &Transaction<'_>, vin: usize, script: &[u8], kind: u8) -> R<([u8; 32], bool)> {
    let base = kind & 31;
    let alone = kind & 0x80 != 0;
    if base == 3 && vin >= t.outputs.len() {
        let mut one = [0; 32];
        one[0] = 1;
        return Ok((one, true));
    }
    let mut bytes = t.version.to_vec();
    compact(&mut bytes, if alone { 1 } else { t.inputs.len() });
    for (index, input) in t.inputs.iter().enumerate() {
        if alone && index != vin {
            continue;
        }
        bytes.extend(input.outpoint);
        let code = if index == vin { script } else { &[] };
        compact(&mut bytes, code.len());
        bytes.extend(code);
        if index != vin && matches!(base, 2 | 3) {
            bytes.extend([0; 4]);
        } else {
            bytes.extend(input.sequence);
        }
    }
    match base {
        1 => {
            compact(&mut bytes, t.outputs.len());
            for output in &t.outputs {
                bytes.extend(output.serial);
            }
        }
        2 => compact(&mut bytes, 0),
        3 => {
            compact(&mut bytes, vin + 1);
            for _ in 0..vin {
                bytes.extend(u64::MAX.to_le_bytes());
                bytes.push(0);
            }
            bytes.extend(t.outputs[vin].serial);
        }
        _ => return Err(BitcoinInputError::UnsupportedHashType),
    }
    bytes.extend(t.locktime);
    bytes.extend(u32::from(kind).to_le_bytes());
    if bytes.len() > MAX_PREIMAGE {
        return Err(BitcoinInputError::Capacity);
    }
    Ok((double_sha256(&bytes), false))
}
fn witness_digest(
    t: &Transaction<'_>,
    vin: usize,
    script: &[u8],
    amount: u64,
    kind: u8,
) -> R<[u8; 32]> {
    let base = kind & 31;
    let alone = kind & 0x80 != 0;
    let mut prevouts = [0; 32];
    let mut sequences = [0; 32];
    let mut outputs = [0; 32];
    if !alone {
        prevouts = double_sha256(
            &t.inputs
                .iter()
                .flat_map(|i| i.outpoint.iter().copied())
                .collect::<Vec<_>>(),
        );
    }
    if !alone && base == 1 {
        sequences = double_sha256(
            &t.inputs
                .iter()
                .flat_map(|i| i.sequence.iter().copied())
                .collect::<Vec<_>>(),
        );
    }
    if base == 1 {
        outputs = double_sha256(
            &t.outputs
                .iter()
                .flat_map(|o| o.serial.iter().copied())
                .collect::<Vec<_>>(),
        );
    } else if base == 3 && vin < t.outputs.len() {
        outputs = double_sha256(t.outputs[vin].serial);
    }
    let input = &t.inputs[vin];
    let mut bytes = t.version.to_vec();
    bytes.extend(prevouts);
    bytes.extend(sequences);
    bytes.extend(input.outpoint);
    compact(&mut bytes, script.len());
    bytes.extend(script);
    bytes.extend(amount.to_le_bytes());
    bytes.extend(input.sequence);
    bytes.extend(outputs);
    bytes.extend(t.locktime);
    bytes.extend(u32::from(kind).to_le_bytes());
    if bytes.len() > MAX_PREIMAGE {
        return Err(BitcoinInputError::Capacity);
    }
    Ok(double_sha256(&bytes))
}
/// Real mathematical signature satisfaction for the explicit supported input
/// template and flag profile. NONE/SINGLE/ANYONECANPAY retain their actual
/// limited commitments; even a valid legacy SINGLE-bug signature is not an
/// authorization for TradeLayer outputs or admission to any chain.
pub fn check_bitcoin_standard_input<'a>(
    flags: u32,
    funding: &'a [u8],
    vout: u32,
    spending: &'a [u8],
    vin: u32,
) -> R<BitcoinInputSatisfaction<'a>> {
    if flags != BITCOIN_STANDARD_INPUT_FLAGS {
        return Err(BitcoinInputError::UnsupportedFlags);
    }
    let funding_tx = transaction(funding, None)?;
    let spending_tx = transaction(spending, Some(vin as usize))?;
    let output = funding_tx
        .outputs
        .get(vout as usize)
        .ok_or(BitcoinInputError::FundingIndex)?;
    let input = spending_tx
        .inputs
        .get(vin as usize)
        .ok_or(BitcoinInputError::SpendingIndex)?;
    if input.outpoint[..32] != funding_tx.txid || input.outpoint[32..] != vout.to_le_bytes() {
        return Err(BitcoinInputError::WrongPrevout);
    }
    let unlock = unlocking(output, input)?;
    let legacy_code = if unlock.witness {
        Vec::new()
    } else {
        legacy_script_code(&unlock.script, &unlock.signatures)?
    };
    let mut signatures = vec![];
    for (slot, (item, key)) in unlock.signatures.iter().zip(&unlock.keys).enumerate() {
        if !matches!((key.len(), key.first()), (33, Some(2 | 3)) | (65, Some(4))) {
            return Err(BitcoinInputError::UnsupportedPublicKeyEncoding);
        }
        let (compact, kind) = signature(item)?;
        let (digest, bug) = if unlock.witness {
            (
                witness_digest(
                    &spending_tx,
                    vin as usize,
                    &unlock.script,
                    output.amount,
                    kind,
                )?,
                false,
            )
        } else {
            legacy_digest(&spending_tx, vin as usize, &legacy_code, kind)?
        };
        match verify_ecdsa_digest(&digest, key, &compact) {
            Ok(true) => (),
            Ok(false) | Err(SecpError::InvalidSignature) => {
                return Err(BitcoinInputError::InvalidSignature { slot })
            }
            Err(SecpError::InvalidPublicKey) => {
                return Err(BitcoinInputError::InvalidPublicKey { slot })
            }
            Err(e) => return Err(BitcoinInputError::Cryptography(e)),
        }
        signatures.push(BitcoinSignatureSatisfaction {
            hash_type: kind,
            digest_raw: digest,
            legacy_single_bug: bug,
        });
    }
    Ok(BitcoinInputSatisfaction {
        funding,
        spending,
        funding_vout: vout,
        spending_vin: vin,
        amount: output.amount,
        template: unlock.template,
        keys: unlock.keys,
        signatures,
        witness_amount: unlock.witness,
    })
}

#[cfg(test)]
mod legacy_script_tests {
    use super::*;
    #[test]
    fn signature_removal_respects_opcode_boundaries_and_all_multisig_items() {
        // A 20-byte positive DER+hash-type item; this is a normalization KAT,
        // not a fabricated valid public-key/hash preimage or spend receipt.
        let item = vec![
            0x30, 17, 2, 7, 1, 2, 3, 4, 5, 6, 7, 2, 6, 1, 2, 3, 4, 5, 6, 1,
        ];
        assert_eq!(item.len(), 20);
        signature(&item).unwrap();
        assert_eq!(
            legacy_script_code(&p2pkh(&item), std::slice::from_ref(&item)).unwrap(),
            vec![0x76, 0xa9, 0x88, 0xac]
        );
        let mut pushed = vec![33, 2];
        pushed.extend([20]);
        pushed.extend(&item);
        pushed.resize(34, 0xab);
        assert_eq!(
            legacy_script_code(&pushed, std::slice::from_ref(&item)).unwrap(),
            pushed
        );
        let mut other = item.clone();
        other[6] ^= 1;
        let mut code = vec![20];
        code.extend(&item);
        code.extend([0x52, 20]);
        code.extend(&other);
        code.push(0xae);
        assert_eq!(
            legacy_script_code(&code, &[item, other]).unwrap(),
            vec![0x52, 0xae]
        );
    }
}
