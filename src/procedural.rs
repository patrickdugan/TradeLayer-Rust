//! Bounded DLC template/contract metadata and the pinned procedural gates.
//!
//! A successful context check only checks stored metadata. It does not verify
//! DLC signatures, prove settlement, authorize a sender or issue/redeem tokens.

use std::collections::BTreeMap;

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e";
pub const MAX_PROCEDURAL_RECORDS: usize = 4096;
pub const MAX_PROCEDURAL_TEXT_BYTES: usize = 4096;
pub const MAX_PROCEDURAL_DOCUMENT_BYTES: usize = 65_536;
pub const MAX_PROCEDURAL_VALUE_NODES: usize = 4096;
pub const MAX_PROCEDURAL_VALUE_DEPTH: usize = 16;
pub const MAX_JS_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// JSON-like, finite metadata domain. Numeric fields are exact JS-safe integers.
/// Floating point, undefined, functions, dates and prototype fields are excluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProceduralValue {
    Null,
    Bool(bool),
    Integer(i64),
    Text(String),
    Array(Vec<ProceduralValue>),
    Object(ProceduralDocument),
}

impl From<&str> for ProceduralValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for ProceduralValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl ProceduralValue {
    pub fn truthy(&self) -> bool {
        match self {
            Self::Null | Self::Bool(false) | Self::Integer(0) => false,
            Self::Text(text) => !text.is_empty(),
            _ => true,
        }
    }

    /// JavaScript String(value) on the documented metadata domain.
    pub fn js_string(&self) -> String {
        match self {
            Self::Null => "null".into(),
            Self::Bool(value) => value.to_string(),
            Self::Integer(value) => value.to_string(),
            Self::Text(value) => value.clone(),
            Self::Object(_) => "[object Object]".into(),
            Self::Array(values) => values
                .iter()
                .map(|value| match value {
                    Self::Null => String::new(),
                    _ => value.js_string(),
                })
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

pub type ProceduralDocument = BTreeMap<String, ProceduralValue>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProceduralError {
    TextLimit,
    DocumentLimit,
    ValueLimit,
    DepthLimit,
    UnsafeInteger,
    UnsupportedFieldName,
    ImmutableRecordId,
    Capacity,
}

impl core::fmt::Display for ProceduralError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ProceduralError {}

/// Known source constants. The source also accepts arbitrary state strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProceduralState {
    Template,
    Funded,
    Open,
    Settled,
    Closed,
    Disputed,
}

impl ProceduralState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Template => "TEMPLATE",
            Self::Funded => "FUNDED",
            Self::Open => "OPEN",
            Self::Settled => "SETTLED",
            Self::Closed => "CLOSED",
            Self::Disputed => "DISPUTED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextRejection {
    MissingIssuanceIds,
    MissingIssuanceHash,
    UnknownTemplate,
    MissingTemplateHash,
    TemplateHashMismatch,
    UnknownIssuanceContract,
    TemplateContractMismatch,
    NotMintable(String),
    IssuanceStateMismatch,
    MissingRedemptionIds,
    UnknownRedemptionContract,
    NotRedeemable(String),
    RedemptionStateMismatch,
    InputLimit,
}

impl core::fmt::Display for ContextRejection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingIssuanceIds => {
                f.write_str("Missing dlcTemplateId/dlcContractId for procedural token")
            }
            Self::MissingIssuanceHash => {
                f.write_str("Missing dlcHash for procedural token issuance")
            }
            Self::UnknownTemplate => f.write_str("Unknown DLC template id for procedural token"),
            Self::MissingTemplateHash => {
                f.write_str("DLC template missing published hash reference")
            }
            Self::TemplateHashMismatch => f.write_str("DLC template hash mismatch"),
            Self::UnknownIssuanceContract => {
                f.write_str("Unknown DLC contract id for procedural token")
            }
            Self::TemplateContractMismatch => f.write_str("DLC template/contract mismatch"),
            Self::NotMintable(state) => write!(f, "DLC contract state {state} not mintable"),
            Self::IssuanceStateMismatch | Self::RedemptionStateMismatch => {
                f.write_str("Provided settlementState does not match DLC contract state")
            }
            Self::MissingRedemptionIds => {
                f.write_str("Missing dlcTemplateId/dlcContractId for procedural redemption")
            }
            Self::UnknownRedemptionContract => {
                f.write_str("Unknown DLC contract id for procedural redemption")
            }
            Self::NotRedeemable(state) => write!(f, "DLC contract state {state} not redeemable"),
            Self::InputLimit => {
                f.write_str("Procedural context input exceeds typed resource limit")
            }
        }
    }
}
impl std::error::Error for ContextRejection {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IssuanceContext<'a> {
    pub contract: &'a ProceduralDocument,
    pub template: &'a ProceduralDocument,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedemptionContext<'a> {
    pub contract: &'a ProceduralDocument,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProceduralRegistry {
    records: BTreeMap<String, ProceduralDocument>,
}

impl ProceduralRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Sorted complete records, suitable for the caller's persistence adapter.
    pub fn records(&self) -> impl Iterator<Item = (&str, &ProceduralDocument)> {
        self.records
            .iter()
            .map(|(key, value)| (key.as_str(), value))
    }

    pub fn get_template(&self, template_id: &str) -> Option<&ProceduralDocument> {
        self.records.get(&format!("template-{template_id}"))
    }

    pub fn get_contract(&self, contract_id: &str) -> Option<&ProceduralDocument> {
        self.records.get(&format!("contract-{contract_id}"))
    }

    /// Returns this write's document; existing fields absent from it remain in
    /// the stored `$set` document, exactly as in the source database operation.
    pub fn upsert_template(
        &mut self,
        template_id: &str,
        data: &ProceduralDocument,
    ) -> Result<ProceduralDocument, ProceduralError> {
        check_text(template_id)?;
        validate_document(data)?;
        let key = format!("template-{template_id}");
        let mut doc = ProceduralDocument::from([
            ("_id".into(), key.clone().into()),
            ("type".into(), "template".into()),
            ("templateId".into(), template_id.into()),
        ]);
        doc.extend(data.clone());
        self.apply_set(&key, doc)
    }

    pub fn upsert_contract(
        &mut self,
        contract_id: &str,
        template_id: &str,
        state: Option<&str>,
        data: &ProceduralDocument,
    ) -> Result<ProceduralDocument, ProceduralError> {
        check_text(contract_id)?;
        check_text(template_id)?;
        validate_document(data)?;
        let state = state.unwrap_or(ProceduralState::Open.as_str());
        check_text(state)?;
        let key = format!("contract-{contract_id}");
        let mut doc = ProceduralDocument::from([
            ("_id".into(), key.clone().into()),
            ("type".into(), "contract".into()),
            ("contractId".into(), contract_id.into()),
            ("templateId".into(), template_id.into()),
            ("state".into(), state.into()),
        ]);
        doc.extend(data.clone());
        self.apply_set(&key, doc)
    }

    /// The source has no transition graph and permits creation of an orphan
    /// contract. The final uppercased next_state overrides metadata.state.
    pub fn transition_contract(
        &mut self,
        contract_id: &str,
        next_state: &str,
        metadata: &ProceduralDocument,
    ) -> Result<ProceduralDocument, ProceduralError> {
        check_text(contract_id)?;
        check_text(next_state)?;
        validate_document(metadata)?;
        let key = format!("contract-{contract_id}");
        let mut doc = self.records.get(&key).cloned().unwrap_or_else(|| {
            ProceduralDocument::from([
                ("_id".into(), key.clone().into()),
                ("type".into(), "contract".into()),
                ("contractId".into(), contract_id.into()),
            ])
        });
        doc.extend(metadata.clone());
        doc.insert("state".into(), next_state.to_uppercase().into());
        self.apply_set(&key, doc)
    }

    pub fn ensure_issuance_context(
        &self,
        template_id: &str,
        contract_id: &str,
        settlement_state: &str,
        dlc_hash: &str,
    ) -> Result<IssuanceContext<'_>, ContextRejection> {
        check_context_inputs(&[template_id, contract_id, settlement_state, dlc_hash])?;
        if template_id.is_empty() || contract_id.is_empty() {
            return Err(ContextRejection::MissingIssuanceIds);
        }
        if dlc_hash.is_empty() {
            return Err(ContextRejection::MissingIssuanceHash);
        }
        let template = self
            .get_template(template_id)
            .ok_or(ContextRejection::UnknownTemplate)?;
        let hash = ["templateHash", "dlcHash", "hash"]
            .into_iter()
            .filter_map(|field| template.get(field))
            .find(|value| value.truthy())
            .map(ProceduralValue::js_string)
            .unwrap_or_default();
        let expected_hash = normalize_hash(&hash);
        if expected_hash.is_empty() {
            return Err(ContextRejection::MissingTemplateHash);
        }
        if normalize_hash(dlc_hash) != expected_hash {
            return Err(ContextRejection::TemplateHashMismatch);
        }
        let contract = self
            .get_contract(contract_id)
            .ok_or(ContextRejection::UnknownIssuanceContract)?;
        check_template(contract, template_id)?;
        let current = state(contract);
        if !matches!(current.as_str(), "FUNDED" | "OPEN") {
            return Err(ContextRejection::NotMintable(current));
        }
        if !settlement_state.is_empty() && settlement_state.to_uppercase() != current {
            return Err(ContextRejection::IssuanceStateMismatch);
        }
        Ok(IssuanceContext { contract, template })
    }

    pub fn ensure_redemption_context(
        &self,
        template_id: &str,
        contract_id: &str,
        settlement_state: &str,
    ) -> Result<RedemptionContext<'_>, ContextRejection> {
        check_context_inputs(&[template_id, contract_id, settlement_state])?;
        if template_id.is_empty() || contract_id.is_empty() {
            return Err(ContextRejection::MissingRedemptionIds);
        }
        // Source redemption checks the contract only, with no template lookup.
        let contract = self
            .get_contract(contract_id)
            .ok_or(ContextRejection::UnknownRedemptionContract)?;
        check_template(contract, template_id)?;
        let current = state(contract);
        if !matches!(current.as_str(), "SETTLED" | "CLOSED") {
            return Err(ContextRejection::NotRedeemable(current));
        }
        if !settlement_state.is_empty() && settlement_state.to_uppercase() != current {
            return Err(ContextRejection::RedemptionStateMismatch);
        }
        Ok(RedemptionContext { contract })
    }

    fn apply_set(
        &mut self,
        key: &str,
        doc: ProceduralDocument,
    ) -> Result<ProceduralDocument, ProceduralError> {
        // NeDB rejects changing `_id`, including modifier-based upserts.
        if doc.get("_id") != Some(&ProceduralValue::Text(key.into())) {
            return Err(ProceduralError::ImmutableRecordId);
        }
        validate_document(&doc)?;
        let mut merged = self.records.get(key).cloned().unwrap_or_default();
        merged.extend(doc.clone());
        validate_document(&merged)?;
        if !self.records.contains_key(key) && self.records.len() >= MAX_PROCEDURAL_RECORDS {
            return Err(ProceduralError::Capacity);
        }
        self.records.insert(key.into(), merged);
        Ok(doc)
    }
}

pub fn normalize_hash(hash: &str) -> String {
    hash.trim_matches(js_whitespace).to_lowercase()
}

fn check_template(
    contract: &ProceduralDocument,
    template_id: &str,
) -> Result<(), ContextRejection> {
    let actual = contract
        .get("templateId")
        .map(ProceduralValue::js_string)
        .unwrap_or_else(|| "undefined".into());
    if actual == template_id {
        Ok(())
    } else {
        Err(ContextRejection::TemplateContractMismatch)
    }
}

fn state(contract: &ProceduralDocument) -> String {
    contract
        .get("state")
        .filter(|value| value.truthy())
        .map(ProceduralValue::js_string)
        .unwrap_or_default()
        .to_uppercase()
}

fn check_text(text: &str) -> Result<(), ProceduralError> {
    if text.len() > MAX_PROCEDURAL_TEXT_BYTES {
        Err(ProceduralError::TextLimit)
    } else {
        Ok(())
    }
}

fn check_context_inputs(inputs: &[&str]) -> Result<(), ContextRejection> {
    if inputs.iter().any(|value| check_text(value).is_err()) {
        Err(ContextRejection::InputLimit)
    } else {
        Ok(())
    }
}

fn validate_document(doc: &ProceduralDocument) -> Result<(), ProceduralError> {
    fn walk(
        doc: &ProceduralDocument,
        depth: usize,
        nodes: &mut usize,
        bytes: &mut usize,
    ) -> Result<(), ProceduralError> {
        for (key, item) in doc {
            if key.contains('.')
                || key.starts_with('$')
                || matches!(
                    key.as_str(),
                    "__proto__" | "prototype" | "constructor" | "toString" | "valueOf"
                )
            {
                return Err(ProceduralError::UnsupportedFieldName);
            }
            check_text(key)?;
            *bytes += key.len();
            value(item, depth, nodes, bytes)?;
        }
        Ok(())
    }
    fn value(
        item: &ProceduralValue,
        depth: usize,
        nodes: &mut usize,
        bytes: &mut usize,
    ) -> Result<(), ProceduralError> {
        if depth > MAX_PROCEDURAL_VALUE_DEPTH {
            return Err(ProceduralError::DepthLimit);
        }
        *nodes += 1;
        if *nodes > MAX_PROCEDURAL_VALUE_NODES {
            return Err(ProceduralError::ValueLimit);
        }
        match item {
            ProceduralValue::Text(text) => {
                check_text(text)?;
                *bytes += text.len();
            }
            ProceduralValue::Integer(number)
                if !(-MAX_JS_SAFE_INTEGER..=MAX_JS_SAFE_INTEGER).contains(number) =>
            {
                return Err(ProceduralError::UnsafeInteger)
            }
            ProceduralValue::Integer(_) => *bytes += 8,
            ProceduralValue::Null | ProceduralValue::Bool(_) => *bytes += 1,
            ProceduralValue::Array(items) => {
                for item in items {
                    value(item, depth + 1, nodes, bytes)?;
                }
            }
            ProceduralValue::Object(doc) => walk(doc, depth + 1, nodes, bytes)?,
        }
        if *bytes > MAX_PROCEDURAL_DOCUMENT_BYTES {
            return Err(ProceduralError::DocumentLimit);
        }
        Ok(())
    }
    walk(doc, 0, &mut 0, &mut 0)
}

fn js_whitespace(character: char) -> bool {
    matches!(character,
        '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}' | '\u{feff}')
}
