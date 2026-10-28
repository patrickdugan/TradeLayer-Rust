//! Actual ECDSA verification for one bounded native Bitcoin P2WSH 2-of-2
//! SIGHASH_ALL input. Funding bytes are evidence, never proof of unspentness.

use crate::channel_bip143_audit::audit_native_p2wsh_bip143_sighash_all_unverified;
use crate::channel_witness_audit::{
    check_der_all, ClaimedTwoOfTwoPubkeys, Cursor, StructuralChannelError,
};
use crate::network_profile::TradeLayerMainnet;
use crate::read_only::{inspect_transaction, ScanError, MAX_IO_COUNT};
use crate::secp256k1::{verify_ecdsa_digest, SecpError};

/// Exact Core pre-Taproot consensus flag profile. Unsupported flag sets are
/// rejected; accepting this profile does not prove its active-chain context.
pub const BITCOIN_TWO_OF_TWO_FLAGS: u32 =
    (1 << 0) | (1 << 2) | (1 << 4) | (1 << 9) | (1 << 10) | (1 << 11);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BitcoinTwoOfTwoError {
    UnsupportedFlags,
    Structural(StructuralChannelError),
    UnsupportedWitnessScript,
    UnsupportedIdenticalSignatureItems,
    InvalidSignature { slot: usize },
    InvalidPublicKey { slot: usize },
    Cryptography(SecpError),
}
impl From<StructuralChannelError> for BitcoinTwoOfTwoError {
    fn from(value: StructuralChannelError) -> Self {
        Self::Structural(value)
    }
}
impl From<ScanError> for BitcoinTwoOfTwoError {
    fn from(value: ScanError) -> Self {
        Self::Structural(value.into())
    }
}
impl core::fmt::Display for BitcoinTwoOfTwoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BitcoinTwoOfTwoError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitcoinTwoOfTwoTrustError {
    FundingInclusionAndUnspentnessUnauthenticated,
    CompleteTransactionAndChainConsensusUnverified,
    TradeLayerActorAndEconomicAdmissionUnavailable,
}

/// Only constructible after real mathematical verification of both signatures
/// in the actual script's key order. This checks the selected template/input,
/// not other inputs, funding validity, historic inclusion or chain admission.
#[derive(Debug, Clone)]
pub struct CheckedBitcoinTwoOfTwoInput<'a> {
    funding: &'a [u8],
    spending: &'a [u8],
    funding_vout: u32,
    spending_vin: u32,
    funding_satoshis: u64,
    digest: [u8; 32],
    script_keys: [[u8; 33]; 2],
}
impl<'a> CheckedBitcoinTwoOfTwoInput<'a> {
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
        self.funding_satoshis
    }
    pub const fn digest_raw(&self) -> [u8; 32] {
        self.digest
    }
    pub const fn actual_script_keys(&self) -> [[u8; 33]; 2] {
        self.script_keys
    }
    pub const fn require_authenticated_parent_utxo(&self) -> Result<(), BitcoinTwoOfTwoTrustError> {
        Err(BitcoinTwoOfTwoTrustError::FundingInclusionAndUnspentnessUnauthenticated)
    }
    pub const fn require_complete_transaction_consensus(
        &self,
    ) -> Result<(), BitcoinTwoOfTwoTrustError> {
        Err(BitcoinTwoOfTwoTrustError::CompleteTransactionAndChainConsensusUnverified)
    }
    pub const fn require_tradelayer_actor_admission(
        &self,
    ) -> Result<(), BitcoinTwoOfTwoTrustError> {
        Err(BitcoinTwoOfTwoTrustError::TradeLayerActorAndEconomicAdmissionUnavailable)
    }
}

fn selected_witness(raw: &[u8], vin: u32) -> Result<Vec<&[u8]>, BitcoinTwoOfTwoError> {
    if !inspect_transaction(raw)?.has_witness {
        return Err(StructuralChannelError::NonNativeWitness.into());
    }
    let mut cursor = Cursor::new(raw);
    cursor.take(4)?;
    if cursor.take(2)? != [0, 1] {
        return Err(StructuralChannelError::NonNativeWitness.into());
    }
    let inputs = cursor.count()?;
    let selected = vin as usize;
    if selected >= inputs {
        return Err(StructuralChannelError::InvalidInputIndex.into());
    }
    for _ in 0..inputs {
        cursor.take(36)?;
        cursor.data()?;
        cursor.take(4)?;
    }
    let outputs = cursor.count()?;
    for _ in 0..outputs {
        cursor.take(8)?;
        cursor.data()?;
    }
    let mut witness = Vec::new();
    for index in 0..inputs {
        let items = cursor.compact()?;
        if items > MAX_IO_COUNT {
            return Err(ScanError::TooLong.into());
        }
        for _ in 0..items {
            let item = cursor.data()?;
            if index == selected {
                witness.push(item);
            }
        }
    }
    cursor.take(4)?;
    if cursor.offset() != raw.len() {
        return Err(ScanError::TrailingBytes.into());
    }
    Ok(witness)
}

fn compact_signature(item: &[u8], signature_slot: usize) -> Result<[u8; 64], BitcoinTwoOfTwoError> {
    check_der_all(item)?;
    let r_len = usize::from(item[3]);
    let s_len = usize::from(item[5 + r_len]);
    let mut compact = [0; 64];
    for (slot, integer) in [&item[4..4 + r_len], &item[6 + r_len..6 + r_len + s_len]]
        .into_iter()
        .enumerate()
    {
        let integer = if integer.len() > 1 && integer[0] == 0 {
            &integer[1..]
        } else {
            integer
        };
        if integer.len() > 32 {
            return Err(BitcoinTwoOfTwoError::InvalidSignature {
                slot: signature_slot,
            });
        }
        let end = (slot + 1) * 32;
        compact[end - integer.len()..end].copy_from_slice(integer);
    }
    Ok(compact)
}

/// Verify exactly the supported native canonical 2-of-2 SIGHASH_ALL input.
/// Keys, DER signatures, amount and outpoint are derived from complete actual
/// witness/funding/spending bytes. No caller-supplied actor or amount exists.
///
/// Consensus ECDSA accepts high S, as Core does without the LOW_S policy flag.
/// Identical signature items and duplicate keys remain explicit unsupported
/// cases of the reused structural profile, not a claim of Core invalidity.
pub fn verify_bitcoin_native_two_of_two_input<'a>(
    flags: u32,
    funding: &'a [u8],
    funding_vout: u32,
    spending: &'a [u8],
    spending_vin: u32,
) -> Result<CheckedBitcoinTwoOfTwoInput<'a>, BitcoinTwoOfTwoError> {
    if flags != BITCOIN_TWO_OF_TWO_FLAGS {
        return Err(BitcoinTwoOfTwoError::UnsupportedFlags);
    }
    let witness = selected_witness(spending, spending_vin)?;
    if witness.len() != 4 || !witness[0].is_empty() {
        return Err(StructuralChannelError::InvalidWitnessShape.into());
    }
    let script = witness[3];
    if script.len() != 71
        || script[..2] != [0x52, 0x21]
        || script[35] != 0x21
        || script[69..] != [0x52, 0xae]
    {
        return Err(BitcoinTwoOfTwoError::UnsupportedWitnessScript);
    }
    if witness[1] == witness[2] {
        return Err(BitcoinTwoOfTwoError::UnsupportedIdenticalSignatureItems);
    }
    let keys: [[u8; 33]; 2] = [
        script[2..35].try_into().expect("first fixed key"),
        script[36..69].try_into().expect("second fixed key"),
    ];
    let digest = audit_native_p2wsh_bip143_sighash_all_unverified(
        TradeLayerMainnet::Bitcoin,
        funding,
        funding_vout,
        spending,
        spending_vin,
        ClaimedTwoOfTwoPubkeys {
            participant_a: keys[0],
            participant_b: keys[1],
        },
    )?;
    for (slot, key) in keys.iter().enumerate() {
        let signature = compact_signature(witness[slot + 1], slot)?;
        match verify_ecdsa_digest(&digest.digest_raw(), key, &signature) {
            Ok(true) => {}
            Ok(false) | Err(SecpError::InvalidSignature) => {
                return Err(BitcoinTwoOfTwoError::InvalidSignature { slot })
            }
            Err(SecpError::InvalidPublicKey) => {
                return Err(BitcoinTwoOfTwoError::InvalidPublicKey { slot })
            }
            Err(error) => return Err(BitcoinTwoOfTwoError::Cryptography(error)),
        }
    }
    Ok(CheckedBitcoinTwoOfTwoInput {
        funding,
        spending,
        funding_vout,
        spending_vin,
        funding_satoshis: digest.structural().funding_satoshis(),
        digest: digest.digest_raw(),
        script_keys: keys,
    })
}
