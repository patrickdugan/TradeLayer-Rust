//! Bind selected-input verification to exact bytes from a complete supplied
//! block. This relation grants neither parent-UTXO nor TradeLayer authority.
use crate::bitcoin_standard_input::{
    check_bitcoin_standard_input, BitcoinInputError, BitcoinInputSatisfaction,
    BitcoinInputTrustError,
};
use crate::bitcoin_witness_binding::{
    inspect_bitcoin_witness_binding, BitcoinWitnessRelation, ExpectedWitnessBlock,
    WitnessBindingError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitcoinBlockInputPosition {
    pub transaction: u32,
    pub input: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BitcoinBlockInputError {
    WitnessBinding(WitnessBindingError),
    CoinbasePosition,
    TransactionPosition,
    Input(BitcoinInputError),
}
impl core::fmt::Display for BitcoinBlockInputError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BitcoinBlockInputError {}

/// Both facts refer to the same borrowed block bytes. A detached spending
/// transaction, signature verdict or script key list cannot be substituted.
/// Only the selected input is checked, under the explicit supported flag set.
#[derive(Debug, Clone)]
pub struct CheckedBitcoinBlockInput<'a> {
    block: BitcoinWitnessRelation<'a>,
    position: BitcoinBlockInputPosition,
    input: BitcoinInputSatisfaction<'a>,
}
impl<'a> CheckedBitcoinBlockInput<'a> {
    pub fn block_relation(&self) -> &BitcoinWitnessRelation<'a> {
        &self.block
    }
    pub const fn position(&self) -> BitcoinBlockInputPosition {
        self.position
    }
    pub fn input_satisfaction(&self) -> &BitcoinInputSatisfaction<'a> {
        &self.input
    }
    pub fn require_authenticated_parent_utxo(&self) -> Result<(), BitcoinInputTrustError> {
        self.input.require_authenticated_parent_utxo()
    }
    pub fn require_complete_transaction_consensus(&self) -> Result<(), BitcoinInputTrustError> {
        self.input.require_complete_transaction_consensus()
    }
    pub fn require_tradelayer_actor_admission(&self) -> Result<(), BitcoinInputTrustError> {
        self.input.require_tradelayer_actor_admission()
    }
}

/// Recheck TXID/witness commitments in a complete raw block, select the actual
/// spending bytes by position, then verify the supported input against the
/// supplied funding transaction/output. Expected block/rule context and flags
/// are independent caller expectations. Funding inclusion, spentness, other
/// inputs and all remaining transaction/chain rules remain unproven.
pub fn check_bitcoin_block_input<'a>(
    raw_block: &'a [u8],
    expected: ExpectedWitnessBlock,
    flags: u32,
    position: BitcoinBlockInputPosition,
    raw_funding: &'a [u8],
    funding_vout: u32,
) -> Result<CheckedBitcoinBlockInput<'a>, BitcoinBlockInputError> {
    let block = inspect_bitcoin_witness_binding(raw_block, expected)
        .map_err(BitcoinBlockInputError::WitnessBinding)?;
    if position.transaction == 0 {
        return Err(BitcoinBlockInputError::CoinbasePosition);
    }
    let spending = block
        .raw_transaction(position.transaction as usize)
        .ok_or(BitcoinBlockInputError::TransactionPosition)?;
    let input =
        check_bitcoin_standard_input(flags, raw_funding, funding_vout, spending, position.input)
            .map_err(BitcoinBlockInputError::Input)?;
    Ok(CheckedBitcoinBlockInput {
        block,
        position,
        input,
    })
}

#[cfg(test)]
#[path = "bitcoin_block_input_tests.rs"]
mod tests;
