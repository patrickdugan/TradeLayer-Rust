//! Explicit factual dependency input only. This is neither a source outcome
//! nor a financial/native-state importer, execution permit or WAL admission.
use crate::validity_source::{SourceRuleError, SourceValue as V};
const MAGIC: &[u8] = b"TLMFRI\x01";
pub const MAX_FRESH_RPC_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FRESH_RPC_INPUT_ROWS: usize = 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainFreshRpcInputV1 {
    io: V,
    documents: Option<V>,
}
impl NativeMainFreshRpcInputV1 {
    pub fn new(io: V, documents: Option<V>) -> Result<Self, SourceRuleError> {
        io.validate()?;
        let V::Object(fields) = &io else {
            return Err(SourceRuleError::InvalidInput);
        };
        if fields.iter().any(|(name, _)| {
            !["height", "blocks", "networkInfo", "failures"].contains(&name.as_str())
        }) {
            return Err(SourceRuleError::InvalidInput);
        }
        for name in ["blocks", "failures"] {
            match io.get(name) {
                V::Undefined => (),
                V::Array(rows) if rows.len() <= MAX_FRESH_RPC_INPUT_ROWS => (),
                _ => return Err(SourceRuleError::InvalidInput),
            }
        }
        if let Some(documents) = &documents {
            documents.validate()?;
            let V::Array(rows) = documents else {
                return Err(SourceRuleError::InvalidInput);
            };
            if rows.len() > MAX_FRESH_RPC_INPUT_ROWS {
                return Err(SourceRuleError::Capacity);
            }
            for row in rows {
                let V::Object(fields) = row else {
                    return Err(SourceRuleError::InvalidInput);
                };
                if fields
                    .iter()
                    .any(|(name, _)| !["id", "raw", "document"].contains(&name.as_str()))
                    || !matches!(row.get("id"), V::String(_))
                    || !matches!(row.get("raw"), V::Undefined | V::String(_))
                {
                    return Err(SourceRuleError::InvalidInput);
                }
            }
        }
        let value = Self { io, documents };
        value.canonical_input_bytes()?;
        Ok(value)
    }
    pub fn io(&self) -> &V {
        &self.io
    }
    pub fn documents(&self) -> Option<&V> {
        self.documents.as_ref()
    }
    pub fn value(&self) -> V {
        V::Object(vec![
            ("io".into(), self.io.clone()),
            ("documentsPresent".into(), V::Bool(self.documents.is_some())),
            (
                "documents".into(),
                self.documents.clone().unwrap_or(V::Undefined),
            ),
        ])
    }
    pub fn canonical_input_bytes(&self) -> Result<Vec<u8>, SourceRuleError> {
        let body = self.value().diagnostic_json()?;
        let length = MAGIC
            .len()
            .checked_add(8)
            .and_then(|n| n.checked_add(body.len()))
            .ok_or(SourceRuleError::Capacity)?;
        if length > MAX_FRESH_RPC_INPUT_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve(length)
            .map_err(|_| SourceRuleError::Capacity)?;
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&(body.len() as u64).to_be_bytes());
        bytes.extend_from_slice(body.as_bytes());
        Ok(bytes)
    }
    pub fn from_canonical_input_bytes(bytes: &[u8]) -> Result<Self, SourceRuleError> {
        if bytes.len() > MAX_FRESH_RPC_INPUT_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        let raw = bytes
            .strip_prefix(MAGIC)
            .ok_or(SourceRuleError::InvalidInput)?;
        let (length, body) = raw
            .split_at_checked(8)
            .ok_or(SourceRuleError::InvalidInput)?;
        let expected = usize::try_from(u64::from_be_bytes(
            length
                .try_into()
                .map_err(|_| SourceRuleError::InvalidInput)?,
        ))
        .map_err(|_| SourceRuleError::Capacity)?;
        if expected != body.len() {
            return Err(SourceRuleError::InvalidInput);
        }
        let value = V::from_diagnostic_json(
            std::str::from_utf8(body).map_err(|_| SourceRuleError::InvalidInput)?,
        )?;
        let documents = match value.get("documentsPresent") {
            V::Bool(false) if value.get("documents") == V::Undefined => None,
            V::Bool(true) => Some(value.get("documents")),
            _ => return Err(SourceRuleError::InvalidInput),
        };
        let input = Self::new(value.get("io"), documents)?;
        if input.value() != value || input.canonical_input_bytes()?.as_slice() != bytes {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(input)
    }
}

#[cfg(test)]
#[path = "source_main_fresh_input_tests.rs"]
mod tests;
