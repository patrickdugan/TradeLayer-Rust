//! Explicit paired-WAL factual inputs; existing nested codecs remain exact.
use crate::source_main_fresh_input::NativeMainFreshRpcInputV1;
use crate::source_main_native_session::{
    NativeMainActiveServiceActionV1, NativeMainActiveServiceErrorV1,
};
fn source_codec_error(error: NativeMainActiveServiceErrorV1) -> SourceRuleError {
    match error {
        NativeMainActiveServiceErrorV1::Capacity => SourceRuleError::Capacity,
        _ => SourceRuleError::InvalidInput,
    }
}
use crate::validity_source::SourceRuleError;
const MAGIC: &[u8] = b"TLMDFI\x01";
pub const MAX_DURABLE_FRESH_ACTION_BYTES: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainDurableFreshActionV1 {
    Feed(NativeMainFreshRpcInputV1),
    SourceAction(NativeMainActiveServiceActionV1),
}
impl NativeMainDurableFreshActionV1 {
    pub fn canonical_action_bytes(&self) -> Result<Vec<u8>, SourceRuleError> {
        let (tag, body) = match self {
            Self::Feed(input) => (0, input.canonical_input_bytes()?),
            Self::SourceAction(action) => (
                1,
                action
                    .canonical_action_bytes()
                    .map_err(source_codec_error)?,
            ),
        };
        let length = MAGIC
            .len()
            .checked_add(9)
            .and_then(|n| n.checked_add(body.len()))
            .ok_or(SourceRuleError::Capacity)?;
        if length > MAX_DURABLE_FRESH_ACTION_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        let mut raw = Vec::new();
        raw.try_reserve(length)
            .map_err(|_| SourceRuleError::Capacity)?;
        raw.extend_from_slice(MAGIC);
        raw.push(tag);
        raw.extend_from_slice(&(body.len() as u64).to_be_bytes());
        raw.extend_from_slice(&body);
        Ok(raw)
    }
    pub fn from_canonical_action_bytes(raw: &[u8]) -> Result<Self, SourceRuleError> {
        if raw.len() > MAX_DURABLE_FRESH_ACTION_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        let body = raw
            .strip_prefix(MAGIC)
            .ok_or(SourceRuleError::InvalidInput)?;
        let (&tag, body) = body.split_first().ok_or(SourceRuleError::InvalidInput)?;
        let (length, body) = body
            .split_at_checked(8)
            .ok_or(SourceRuleError::InvalidInput)?;
        let length = usize::try_from(u64::from_be_bytes(
            length
                .try_into()
                .map_err(|_| SourceRuleError::InvalidInput)?,
        ))
        .map_err(|_| SourceRuleError::Capacity)?;
        if length != body.len() {
            return Err(SourceRuleError::InvalidInput);
        }
        let action = match tag {
            0 => Self::Feed(NativeMainFreshRpcInputV1::from_canonical_input_bytes(body)?),
            1 => Self::SourceAction(
                NativeMainActiveServiceActionV1::from_canonical_action_bytes(body)
                    .map_err(source_codec_error)?,
            ),
            _ => return Err(SourceRuleError::InvalidInput),
        };
        if action.canonical_action_bytes()?.as_slice() != raw {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(action)
    }
}
#[cfg(test)]
#[path = "source_main_durable_fresh_action_tests.rs"]
mod tests;
