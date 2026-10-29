//! Complete supported-input relation for one exact Bitcoin block transaction.
//! This does not establish parent unspentness, full chain consensus or TL authority.
use crate::bitcoin_standard_input::{
    check_bitcoin_standard_input, BitcoinInputError, BitcoinInputSatisfaction,
    BitcoinInputTrustError, BITCOIN_STANDARD_INPUT_FLAGS,
};
use crate::bitcoin_witness_binding::{
    inspect_bitcoin_witness_binding, BitcoinWitnessRelation, ExpectedWitnessBlock,
    WitnessBindingError,
};
use crate::channel_witness_audit::Cursor;
use crate::network_profile::TradeLayerMainnet;
use crate::read_only::{inspect_transaction, ScanError, MAX_IO_COUNT};
use std::collections::{BTreeMap, BTreeSet};

pub const BITCOIN_SUPPORTED_TRANSACTION_PROFILE: &str =
    "TL-BITCOIN-SUPPORTED-TRANSACTION-RELATION-13-1";
pub const BITCOIN_MAX_MONEY_SATOSHIS: u64 = 21_000_000 * 100_000_000;
pub const MAX_EXTERNAL_FUNDING_BYTES: usize = 16 * 1024 * 1024;
const MAX_STRIPPED_TRANSACTION_BYTES: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BitcoinBasicTransactionError {
    RawTransaction(ScanError),
    EmptyInputs,
    EmptyOutputs,
    StrippedSize,
    NegativeOutput { output: u32 },
    OutputTooLarge { output: u32 },
    OutputTotalTooLarge { output: u32 },
    DuplicateInput { first: u32, second: u32 },
    CoinbaseScriptLength,
    NullPrevout { input: u32 },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BitcoinSupportedTransactionError {
    UnsupportedNetwork,
    UnsupportedFlags,
    WitnessBinding(WitnessBindingError),
    CoinbasePosition,
    TransactionPosition,
    SpendingBasic(BitcoinBasicTransactionError),
    FundingMalformed {
        index: usize,
        source: ScanError,
    },
    DuplicateFunding {
        index: usize,
    },
    UnexpectedExternalFunding {
        index: usize,
    },
    MissingFunding {
        input: u32,
        txid_raw: [u8; 32],
    },
    FundingOutput {
        input: u32,
        vout: u32,
    },
    ForwardOrSelfReference {
        input: u32,
        producer: u32,
    },
    SameBlockCoinbaseSpend {
        input: u32,
    },
    InvalidFundingValue {
        input: u32,
    },
    InputTotalTooLarge {
        input: u32,
    },
    InputsBelowOutputs,
    InputScript {
        input: u32,
        source: BitcoinInputError,
    },
    DuplicateBlockTransactionId {
        first: u32,
        second: u32,
    },
    Capacity,
}
impl core::fmt::Display for BitcoinSupportedTransactionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BitcoinSupportedTransactionError {}
impl core::fmt::Display for BitcoinBasicTransactionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BitcoinBasicTransactionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitcoinFundingOrigin {
    SuppliedExternalRaw { index: usize },
    EarlierBlockTransaction { position: u32 },
}
#[derive(Debug, Clone)]
pub struct CheckedSupportedBitcoinInput<'a> {
    origin: BitcoinFundingOrigin,
    satisfaction: BitcoinInputSatisfaction<'a>,
}
impl<'a> CheckedSupportedBitcoinInput<'a> {
    pub const fn origin(&self) -> BitcoinFundingOrigin {
        self.origin
    }
    pub fn satisfaction(&self) -> &BitcoinInputSatisfaction<'a> {
        &self.satisfaction
    }
}
/// A content relation with complete input coverage, not an admission capability.
#[derive(Debug, Clone)]
pub struct CheckedSupportedBitcoinTransactionRelation<'a> {
    block: BitcoinWitnessRelation<'a>,
    position: u32,
    inputs: Vec<CheckedSupportedBitcoinInput<'a>>,
    input_satoshis: u64,
    output_satoshis: u64,
}
impl<'a> CheckedSupportedBitcoinTransactionRelation<'a> {
    pub const fn profile(&self) -> &'static str {
        BITCOIN_SUPPORTED_TRANSACTION_PROFILE
    }
    pub fn block_relation(&self) -> &BitcoinWitnessRelation<'a> {
        &self.block
    }
    pub const fn transaction_position(&self) -> u32 {
        self.position
    }
    pub fn inputs(&self) -> &[CheckedSupportedBitcoinInput<'a>] {
        &self.inputs
    }
    pub const fn input_satoshis(&self) -> u64 {
        self.input_satoshis
    }
    pub const fn output_satoshis(&self) -> u64 {
        self.output_satoshis
    }
    /// Conditional on the supplied funding bytes; not a validated chain fee.
    pub const fn conditional_fee_satoshis(&self) -> u64 {
        self.input_satoshis - self.output_satoshis
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

struct Input<'a> {
    outpoint: &'a [u8],
    script: &'a [u8],
}
struct Output {
    raw_amount: u64,
}
struct Transaction<'a> {
    inputs: Vec<Input<'a>>,
    outputs: Vec<Output>,
    stripped_bytes: usize,
}
fn transaction(raw: &[u8]) -> Result<Transaction<'_>, ScanError> {
    let observation = inspect_transaction(raw)?;
    let mut cursor = Cursor::new(raw);
    cursor.take(4)?;
    if observation.has_witness {
        cursor.take(2)?;
    }
    let stripped_start = cursor.offset();
    let mut inputs = Vec::new();
    for _ in 0..cursor.count()? {
        let outpoint = cursor.take(36)?;
        let script = cursor.data()?;
        cursor.take(4)?;
        inputs.push(Input { outpoint, script });
    }
    let mut outputs = Vec::new();
    for _ in 0..cursor.count()? {
        let raw_amount = u64::from_le_bytes(cursor.take(8)?.try_into().expect("eight bytes"));
        cursor.data()?;
        outputs.push(Output { raw_amount });
    }
    let stripped_bytes = if observation.has_witness {
        8 + cursor.offset() - stripped_start
    } else {
        raw.len()
    };
    Ok(Transaction {
        inputs,
        outputs,
        stripped_bytes,
    })
}
fn null(outpoint: &[u8]) -> bool {
    outpoint[..32] == [0; 32] && outpoint[32..] == [0xff; 4]
}
fn basic(tx: &Transaction<'_>) -> Result<u64, BitcoinBasicTransactionError> {
    use BitcoinBasicTransactionError as E;
    if tx.inputs.is_empty() {
        return Err(E::EmptyInputs);
    }
    if tx.outputs.is_empty() {
        return Err(E::EmptyOutputs);
    }
    if tx.stripped_bytes > MAX_STRIPPED_TRANSACTION_BYTES {
        return Err(E::StrippedSize);
    }
    let mut total = 0_u64;
    for (output, row) in tx.outputs.iter().enumerate() {
        let output = output as u32;
        if row.raw_amount > i64::MAX as u64 {
            return Err(E::NegativeOutput { output });
        }
        if row.raw_amount > BITCOIN_MAX_MONEY_SATOSHIS {
            return Err(E::OutputTooLarge { output });
        }
        total = total
            .checked_add(row.raw_amount)
            .filter(|sum| *sum <= BITCOIN_MAX_MONEY_SATOSHIS)
            .ok_or(E::OutputTotalTooLarge { output })?;
    }
    let mut seen = BTreeMap::new();
    for (second, input) in tx.inputs.iter().enumerate() {
        if let Some(first) = seen.insert(input.outpoint, second as u32) {
            return Err(E::DuplicateInput {
                first,
                second: second as u32,
            });
        }
    }
    if tx.inputs.len() == 1 && null(tx.inputs[0].outpoint) {
        if !(2..=100).contains(&tx.inputs[0].script.len()) {
            return Err(E::CoinbaseScriptLength);
        }
    } else {
        for (input, row) in tx.inputs.iter().enumerate() {
            if null(row.outpoint) {
                return Err(E::NullPrevout {
                    input: input as u32,
                });
            }
        }
    }
    Ok(total)
}
/// Literal context-free Core rules on the repository's bounded canonical parser
/// subset. The parser may refuse a broader Core-serialized transaction first.
pub fn check_bitcoin_context_free_transaction(
    raw: &[u8],
) -> Result<u64, BitcoinBasicTransactionError> {
    let tx = transaction(raw).map_err(BitcoinBasicTransactionError::RawTransaction)?;
    basic(&tx)
}

/// Verify every input of the exact selected block transaction, with exact
/// external funding coverage. Earlier in-block funding is selected only from
/// the actual block, and must not be supplied again as external evidence.
pub fn check_bitcoin_supported_transaction<'a>(
    network: TradeLayerMainnet,
    raw_block: &'a [u8],
    expected: ExpectedWitnessBlock,
    flags: u32,
    position: u32,
    raw_external_funding: &[&'a [u8]],
) -> Result<CheckedSupportedBitcoinTransactionRelation<'a>, BitcoinSupportedTransactionError> {
    use BitcoinSupportedTransactionError as E;
    if network != TradeLayerMainnet::Bitcoin {
        return Err(E::UnsupportedNetwork);
    }
    if flags != BITCOIN_STANDARD_INPUT_FLAGS {
        return Err(E::UnsupportedFlags);
    }
    let block = inspect_bitcoin_witness_binding(raw_block, expected).map_err(E::WitnessBinding)?;
    if position == 0 {
        return Err(E::CoinbasePosition);
    }
    let spending = block
        .raw_transaction(position as usize)
        .ok_or(E::TransactionPosition)?;
    let tx = transaction(spending)
        .map_err(|source| E::SpendingBasic(BitcoinBasicTransactionError::RawTransaction(source)))?;
    let output_satoshis = basic(&tx).map_err(E::SpendingBasic)?;
    if raw_external_funding.len() as u64 > MAX_IO_COUNT {
        return Err(E::Capacity);
    }
    let mut external = BTreeMap::new();
    let mut evidence_bytes = 0_usize;
    for (index, raw) in raw_external_funding.iter().enumerate() {
        evidence_bytes = evidence_bytes
            .checked_add(raw.len())
            .filter(|bytes| *bytes <= MAX_EXTERNAL_FUNDING_BYTES)
            .ok_or(E::Capacity)?;
        let observation =
            inspect_transaction(raw).map_err(|source| E::FundingMalformed { index, source })?;
        if external.insert(observation.txid, (index, *raw)).is_some() {
            return Err(E::DuplicateFunding { index });
        }
    }
    let mut block_transactions = BTreeMap::new();
    for index in 0..block.transaction_count() {
        let raw = block
            .raw_transaction(index)
            .expect("complete block relation");
        let observation =
            inspect_transaction(raw).expect("block relation structurally checked all bytes");
        if let Some((first, _)) = block_transactions.insert(observation.txid, (index as u32, raw)) {
            return Err(E::DuplicateBlockTransactionId {
                first,
                second: index as u32,
            });
        }
    }
    let mut used = BTreeSet::new();
    let mut resolved = Vec::new();
    let mut input_satoshis = 0_u64;
    for (input, row) in tx.inputs.iter().enumerate() {
        let input = input as u32;
        let txid: [u8; 32] = row.outpoint[..32]
            .try_into()
            .expect("32-byte outpoint hash");
        let vout = u32::from_le_bytes(
            row.outpoint[32..]
                .try_into()
                .expect("four-byte output index"),
        );
        let (funding, origin) = if let Some((producer, raw)) = block_transactions.get(&txid) {
            if *producer >= position {
                return Err(E::ForwardOrSelfReference {
                    input,
                    producer: *producer,
                });
            }
            if *producer == 0 {
                return Err(E::SameBlockCoinbaseSpend { input });
            }
            (
                *raw,
                BitcoinFundingOrigin::EarlierBlockTransaction {
                    position: *producer,
                },
            )
        } else {
            let (index, raw) = external.get(&txid).ok_or(E::MissingFunding {
                input,
                txid_raw: txid,
            })?;
            used.insert(*index);
            (
                *raw,
                BitcoinFundingOrigin::SuppliedExternalRaw { index: *index },
            )
        };
        let fund =
            transaction(funding).expect("same immutable funding bytes structurally checked above");
        let value = fund
            .outputs
            .get(vout as usize)
            .ok_or(E::FundingOutput { input, vout })?
            .raw_amount;
        if value > BITCOIN_MAX_MONEY_SATOSHIS {
            return Err(E::InvalidFundingValue { input });
        }
        input_satoshis = input_satoshis
            .checked_add(value)
            .filter(|sum| *sum <= BITCOIN_MAX_MONEY_SATOSHIS)
            .ok_or(E::InputTotalTooLarge { input })?;
        resolved.push((funding, origin, vout));
    }
    if let Some(index) = (0..raw_external_funding.len()).find(|index| !used.contains(index)) {
        return Err(E::UnexpectedExternalFunding { index });
    }
    if input_satoshis < output_satoshis {
        return Err(E::InputsBelowOutputs);
    }
    let mut inputs = Vec::new();
    for (input, (funding, origin, vout)) in resolved.into_iter().enumerate() {
        let satisfaction =
            check_bitcoin_standard_input(flags, funding, vout, spending, input as u32).map_err(
                |source| E::InputScript {
                    input: input as u32,
                    source,
                },
            )?;
        inputs.push(CheckedSupportedBitcoinInput {
            origin,
            satisfaction,
        });
    }
    Ok(CheckedSupportedBitcoinTransactionRelation {
        block,
        position,
        inputs,
        input_satoshis,
        output_satoshis,
    })
}
