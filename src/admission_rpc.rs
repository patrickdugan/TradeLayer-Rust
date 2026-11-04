//! `ParentOutputSource` over Bitcoin Core `getblock <hash> 3`.
//!
//! Verbosity 3 reports each input's `prevout` (value and scriptPubKey), so one
//! response covers every parent output a block spends. The data is a claim by
//! the node: admission verifies the spending signature against it and never
//! treats it as authenticated. A response above `MAX_RPC_BODY` (16 MiB) fails
//! with `RpcError::Capacity`, and every candidate of that block is then refused
//! with a typed source error.

use crate::admission::{ParentOutpoint, ParentOutput, ParentOutputSource, ParentSourceError};
use crate::chain::BlockId;
use crate::client::{RpcClient, RpcError, RpcValue};
use crate::read_only::decode_hex;
use crate::runtime::{block_id_text, parse_block_id};
use std::collections::BTreeMap;

/// Distinct parent outputs one response may define.
pub const MAX_PREVOUTS: usize = 200_000;
const MAX_SCRIPT_HEX: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrevoutSourceError {
    Rpc(RpcError),
    InvalidResponse,
    WrongBlock,
    Capacity,
}
impl core::fmt::Display for PrevoutSourceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PrevoutSourceError {}
impl From<RpcError> for PrevoutSourceError {
    fn from(value: RpcError) -> Self {
        Self::Rpc(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Output(ParentOutput),
    Conflict,
}

/// Parent outputs read from one `getblock <hash> 3` response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcPrevoutSource {
    block: BlockId,
    entries: BTreeMap<ParentOutpoint, Entry>,
}

impl RpcPrevoutSource {
    /// One `getblock <hash> 3` call. No retry.
    pub fn fetch(client: &mut RpcClient, block: BlockId) -> Result<Self, PrevoutSourceError> {
        let response = client.call(
            "getblock",
            &[
                RpcValue::string(&block_id_text(block))?,
                RpcValue::unsigned(3),
            ],
            false,
        )?;
        Self::from_block_json(block, &response)
    }

    /// Reads a `getblock` verbosity-3 response. The response's `hash` must be
    /// the requested block. Coinbase inputs and inputs without a `prevout`
    /// define nothing; two different claims for one outpoint mark it
    /// inconsistent.
    pub fn from_block_json(block: BlockId, json: &RpcValue) -> Result<Self, PrevoutSourceError> {
        let hash = json
            .field("hash")?
            .ok_or(PrevoutSourceError::InvalidResponse)?
            .as_string()?;
        if parse_block_id(&hash).map_err(|_| PrevoutSourceError::InvalidResponse)? != block {
            return Err(PrevoutSourceError::WrongBlock);
        }
        let mut entries = BTreeMap::new();
        for transaction in json
            .field("tx")?
            .ok_or(PrevoutSourceError::InvalidResponse)?
            .as_array()?
        {
            let Some(inputs) = transaction.field("vin")? else {
                return Err(PrevoutSourceError::InvalidResponse);
            };
            for input in inputs.as_array()? {
                if input.field("coinbase")?.is_some() {
                    continue;
                }
                let Some(prevout) = input.field("prevout")? else {
                    continue;
                };
                let txid_text = input
                    .field("txid")?
                    .ok_or(PrevoutSourceError::InvalidResponse)?
                    .as_string()?;
                let mut txid_raw: [u8; 32] = decode_hex(&txid_text)
                    .ok()
                    .and_then(|bytes| bytes.try_into().ok())
                    .ok_or(PrevoutSourceError::InvalidResponse)?;
                txid_raw.reverse();
                let vout = u32::try_from(
                    input
                        .field("vout")?
                        .ok_or(PrevoutSourceError::InvalidResponse)?
                        .as_u64()?,
                )
                .map_err(|_| PrevoutSourceError::InvalidResponse)?;
                let amount_satoshis = bitcoin_amount_satoshis(
                    prevout
                        .field("value")?
                        .ok_or(PrevoutSourceError::InvalidResponse)?
                        .as_json(),
                )?;
                let script_hex = prevout
                    .field("scriptPubKey")?
                    .ok_or(PrevoutSourceError::InvalidResponse)?
                    .field("hex")?
                    .ok_or(PrevoutSourceError::InvalidResponse)?
                    .as_string()?;
                if script_hex.len() > MAX_SCRIPT_HEX {
                    return Err(PrevoutSourceError::InvalidResponse);
                }
                let output = ParentOutput {
                    amount_satoshis,
                    script_pubkey: decode_hex(&script_hex)
                        .map_err(|_| PrevoutSourceError::InvalidResponse)?,
                };
                let outpoint = ParentOutpoint { txid_raw, vout };
                match entries.get(&outpoint) {
                    Some(Entry::Output(existing)) if *existing == output => {}
                    Some(_) => {
                        entries.insert(outpoint, Entry::Conflict);
                    }
                    None => {
                        if entries.len() >= MAX_PREVOUTS {
                            return Err(PrevoutSourceError::Capacity);
                        }
                        entries.insert(outpoint, Entry::Output(output));
                    }
                }
            }
        }
        Ok(Self { block, entries })
    }

    pub const fn block(&self) -> BlockId {
        self.block
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl ParentOutputSource for RpcPrevoutSource {
    fn parent_output(
        &mut self,
        outpoint: &ParentOutpoint,
    ) -> Result<Option<ParentOutput>, ParentSourceError> {
        match self.entries.get(outpoint) {
            None => Ok(None),
            Some(Entry::Output(output)) => Ok(Some(output.clone())),
            Some(Entry::Conflict) => Err(ParentSourceError::Inconsistent),
        }
    }
}

/// Core renders an amount as a JSON number with exactly eight decimals. Any
/// other lexeme (exponent, sign, more decimals) is refused rather than rounded.
fn bitcoin_amount_satoshis(lexeme: &str) -> Result<u64, PrevoutSourceError> {
    let invalid = PrevoutSourceError::InvalidResponse;
    let text = lexeme.trim();
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty()
        || whole.len() > 8
        || fraction.len() > 8
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid);
    }
    let whole: u64 = whole
        .parse()
        .map_err(|_| PrevoutSourceError::InvalidResponse)?;
    let mut padded = fraction.to_owned();
    while padded.len() < 8 {
        padded.push('0');
    }
    let fraction: u64 = padded
        .parse()
        .map_err(|_| PrevoutSourceError::InvalidResponse)?;
    whole
        .checked_mul(100_000_000)
        .and_then(|value| value.checked_add(fraction))
        .ok_or(PrevoutSourceError::InvalidResponse)
}
