//! Actual pinned Types.decodePayload orchestration over untrusted source values.
//! Source diagnostics never confer transaction, historical-state or chain authority.
use crate::issuance_intent::JsonValue;
use crate::legacy_txdecoder_audit::{
    source_decode_historical_method_unverified, HistoricalDecoderError, HistoricalValue,
};
use crate::legacy_types_audit::{source_route, LegacyTypesError};
use crate::validity_source::{
    self, DependencyReturn, SourceBackend, SourceCall, SourceEvaluation, SourceException,
    SourceOutcome, SourceRuleError, SourceValue,
};

pub const SOURCE_SHA256: &str = "a49c1b1f846fe3eab91f4434c9f016586ffddf1bb3fb4492516a065df396a0e1";
pub const MAX_ARGUMENTS: usize = 16;
pub const MAX_TRACE_BYTES: usize = 4 * 1024 * 1024;
type V = SourceValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypesError {
    Capacity,
    InvalidInput,
    UnsupportedUtf16Exception,
    Codec(HistoricalDecoderError),
    Validity(SourceRuleError),
    UnauthenticatedInputAndState,
}
impl core::fmt::Display for TypesError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TypesError {}
impl From<SourceRuleError> for TypesError {
    fn from(value: SourceRuleError) -> Self {
        Self::Validity(value)
    }
}
type Result<T> = core::result::Result<T, TypesError>;

/// These are settlements of the source's outer async function. Returning an
/// Error object is a fulfilled Promise, distinguishable from a rejection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypesOutcome {
    Returned(V),
    ReturnedError(SourceException),
    Rejected(SourceException),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecoderObservation {
    pub method: String,
    pub body: String,
    pub returned: Option<V>,
    pub thrown: Option<SourceException>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValiditySettlement {
    pub evaluation: SourceEvaluation,
    /// Source validators are async; dispatch-only adapters can return directly.
    /// Types awaits either case and adopts the eventual value or rejection.
    pub pending: bool,
}
pub trait TypesValidity {
    fn validate(
        &mut self,
        call: &SourceCall,
    ) -> core::result::Result<ValiditySettlement, SourceRuleError>;
}
pub struct SourceValidityAdapter<'a, B: SourceBackend> {
    pub backend: &'a mut B,
}
impl<B: SourceBackend> TypesValidity for SourceValidityAdapter<'_, B> {
    fn validate(
        &mut self,
        call: &SourceCall,
    ) -> core::result::Result<ValiditySettlement, SourceRuleError> {
        Ok(ValiditySettlement {
            evaluation: validity_source::evaluate_unverified(call, self.backend)?,
            pending: true,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidityObservation {
    pub call: SourceCall,
    pub settlement: ValiditySettlement,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypesEvaluation {
    pub outcome: TypesOutcome,
    pub arguments_after: Vec<V>,
    pub decoder: Option<DecoderObservation>,
    pub validity: Option<ValidityObservation>,
}
impl TypesEvaluation {
    pub const fn require_authenticated_admission(&self) -> core::result::Result<(), TypesError> {
        Err(TypesError::UnauthenticatedInputAndState)
    }
    pub const fn require_trusted_execution(&self) -> core::result::Result<(), LegacyTypesError> {
        Err(LegacyTypesError::TrustedTransactionExecutionForbidden)
    }
    /// Stable tagged diagnostic; no JSON.stringify loss of undefined/NaN/-0.
    pub fn diagnostic_json(&self) -> Result<String> {
        preflight(&self.arguments_after)?;
        if let Some(call) = &self.validity {
            validate_settlement(&call.settlement)?;
        }
        let (status, value) = match &self.outcome {
            TypesOutcome::Returned(value) => ("returned", value.diagnostic_json()?),
            TypesOutcome::ReturnedError(error) => ("returned-error", exception_json(error)?),
            TypesOutcome::Rejected(error) => ("rejected", exception_json(error)?),
        };
        let decoder = if let Some(call) = &self.decoder {
            format!(
                "{{\"method\":{},\"body\":{},\"returned\":{},\"thrown\":{}}}",
                quote(&call.method)?,
                quote(&call.body)?,
                call.returned
                    .as_ref()
                    .map(V::diagnostic_json)
                    .transpose()?
                    .unwrap_or_else(|| "null".into()),
                call.thrown
                    .as_ref()
                    .map(exception_json)
                    .transpose()?
                    .unwrap_or_else(|| "null".into())
            )
        } else {
            "null".into()
        };
        let validity = if let Some(call) = &self.validity {
            let (settlement, value) = match &call.settlement.evaluation.outcome {
                SourceOutcome::Returned(v) => ("returned", v.diagnostic_json()?),
                SourceOutcome::Thrown(e) => ("thrown", exception_json(e)?),
            };
            let evaluation = &call.settlement.evaluation;
            let mut dependencies = Vec::new();
            for observation in &evaluation.dependencies {
                let (mode, returned) = match &observation.returned {
                    DependencyReturn::Immediate(v) => ("immediate", v.diagnostic_json()?),
                    DependencyReturn::Pending(v) => ("pending", v.diagnostic_json()?),
                    DependencyReturn::Throw(e) => ("throw", exception_json(e)?),
                    DependencyReturn::PendingThrow(e) => ("pending-throw", exception_json(e)?),
                };
                dependencies.push(format!(
                    "[{},{},{},{}]",
                    quote(&observation.call.target)?,
                    V::Array(observation.call.arguments.clone()).diagnostic_json()?,
                    quote(mode)?,
                    returned
                ));
            }
            let deferred = evaluation
                .deferred_rejections
                .iter()
                .map(exception_json)
                .collect::<Result<Vec<_>>>()?
                .join(",");
            let globals = evaluation
                .global_writes
                .iter()
                .map(|(name, value)| Ok(format!("[{},{}]", quote(name)?, value.diagnostic_json()?)))
                .collect::<Result<Vec<_>>>()?
                .join(",");
            format!("{{\"method\":{},\"arguments\":{},\"pending\":{},\"outcome\":{},\"value\":{},\"argumentsAfter\":{},\"dependencies\":[{}],\"deferredRejections\":[{}],\"globalWrites\":[{}]}}",
                quote(&call.call.method)?, V::Array(call.call.arguments.clone()).diagnostic_json()?, call.settlement.pending,
                quote(settlement)?, value, V::Array(evaluation.arguments_after.clone()).diagnostic_json()?, dependencies.join(","), deferred, globals)
        } else {
            "null".into()
        };
        let output = format!("{{\"async\":true,\"status\":{},\"value\":{},\"argumentsAfter\":{},\"decoder\":{},\"validity\":{}}}",
            quote(status)?, value, V::Array(self.arguments_after.clone()).diagnostic_json()?, decoder, validity);
        if output.len() > MAX_TRACE_BYTES {
            return Err(TypesError::Capacity);
        }
        Ok(output)
    }
}
fn quote(value: &str) -> Result<String> {
    if value.len() > crate::issuance_intent::MAX_JSON_BYTES {
        return Err(TypesError::Capacity);
    }
    JsonValue::String(value.to_owned())
        .to_json()
        .map_err(|_| TypesError::Capacity)
}
fn exception_json(error: &SourceException) -> Result<String> {
    if error.name.len().saturating_add(error.message.len()) > 4096 {
        return Err(TypesError::Capacity);
    }
    Ok(format!(
        "[{},{}]",
        quote(&error.name)?,
        quote(&error.message)?
    ))
}
fn exception(name: &str, message: impl Into<String>) -> SourceException {
    SourceException {
        name: name.into(),
        message: message.into(),
    }
}
fn validate_settlement(settlement: &ValiditySettlement) -> Result<()> {
    let evaluation = &settlement.evaluation;
    if evaluation.dependencies.len() > validity_source::MAX_DEPENDENCIES
        || evaluation.deferred_rejections.len() > validity_source::MAX_DEPENDENCIES
        || evaluation.global_writes.len() > validity_source::MAX_DEPENDENCIES
    {
        return Err(TypesError::Capacity);
    }
    let mut charged = 0usize;
    let mut value = |v: &V| -> Result<()> {
        v.validate()?;
        charged = charged
            .checked_add(v.diagnostic_json()?.len())
            .ok_or(TypesError::Capacity)?;
        if charged > MAX_TRACE_BYTES {
            return Err(TypesError::Capacity);
        }
        Ok(())
    };
    for v in &evaluation.arguments_after {
        value(v)?;
    }
    if let SourceOutcome::Returned(v) = &evaluation.outcome {
        value(v)?;
    }
    for observation in &evaluation.dependencies {
        if observation.call.target.len() > 128 {
            return Err(TypesError::Capacity);
        }
        preflight(&observation.call.arguments)?;
        for v in &observation.call.arguments {
            value(v)?;
        }
        if let DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) = &observation.returned
        {
            value(v)?;
        }
    }
    for (name, v) in &evaluation.global_writes {
        if name.len() > 128 {
            return Err(TypesError::Capacity);
        }
        value(v)?;
    }
    for error in evaluation
        .deferred_rejections
        .iter()
        .chain(match &evaluation.outcome {
            SourceOutcome::Thrown(e) => Some(e),
            _ => None,
        })
        .chain(
            evaluation
                .dependencies
                .iter()
                .filter_map(|d| match &d.returned {
                    DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => Some(e),
                    _ => None,
                }),
        )
    {
        if error.name.len().saturating_add(error.message.len()) > 4096 {
            return Err(TypesError::Capacity);
        }
    }
    Ok(())
}
#[derive(Debug)]
enum Fault {
    Source(SourceException),
    Local(TypesError),
}
impl From<TypesError> for Fault {
    fn from(value: TypesError) -> Self {
        Self::Local(value)
    }
}
impl From<SourceRuleError> for Fault {
    fn from(value: SourceRuleError) -> Self {
        Self::Local(value.into())
    }
}
type Eval<T> = core::result::Result<T, Fault>;
fn source_throw(name: &str, message: impl Into<String>) -> Fault {
    Fault::Source(exception(name, message))
}
fn nullish(value: &V) -> bool {
    matches!(value, V::Undefined | V::Null)
}
fn truth(value: &V) -> bool {
    match value {
        V::Undefined | V::Null | V::Bool(false) => false,
        V::Number(bits) => {
            let n = f64::from_bits(*bits);
            n != 0.0 && !n.is_nan()
        }
        V::String(value) => !value.is_empty(),
        _ => true,
    }
}
fn primitive_text(value: &V) -> Eval<String> {
    Ok(match value {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(v) => v.to_string(),
        V::String(v) | V::BigNumber(v) => v.clone(),
        V::Number(bits) => {
            let n = f64::from_bits(*bits);
            if n.is_nan() {
                "NaN".into()
            } else if n == f64::INFINITY {
                "Infinity".into()
            } else if n == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                crate::state_oracle::source_number_text(n).map_err(|_| TypesError::InvalidInput)?
            }
        }
        V::Array(values) => {
            let mut out = String::new();
            for (i, value) in values.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if !nullish(value) {
                    out.push_str(&primitive_text(value)?);
                }
                if out.len() > validity_source::MAX_BYTES {
                    return Err(TypesError::Capacity.into());
                }
            }
            out
        }
        V::Object(fields) => {
            if fields.iter().any(|(key, _)| key == "toString") {
                return Err(source_throw(
                    "TypeError",
                    "Cannot convert object to primitive value",
                ));
            }
            "[object Object]".into()
        }
        V::Promise => "[object Promise]".into(),
    })
}
fn strict(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Undefined, V::Undefined) | (V::Null, V::Null) => true,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::String(a), V::String(b)) => a == b,
        (V::Number(a), V::Number(b)) => f64::from_bits(*a) == f64::from_bits(*b),
        _ => false,
    }
}
fn property(value: &V, key: &str) -> Eval<V> {
    if nullish(value) {
        return Err(source_throw(
            "TypeError",
            format!(
                "Cannot read properties of {} (reading '{key}')",
                primitive_text(value)?
            ),
        ));
    }
    Ok(value.get(key))
}
fn assign(value: &mut V, key: &str, new: V) -> Eval<()> {
    if nullish(value) {
        return Err(source_throw(
            "TypeError",
            format!(
                "Cannot set properties of {} (setting '{key}')",
                primitive_text(value)?
            ),
        ));
    }
    if matches!(value, V::Object(_)) {
        value.insert(key, new)?;
    }
    // types.js is non-strict: writes to primitive wrappers are silently lost.
    Ok(())
}
fn from_historical(value: &HistoricalValue) -> V {
    match value {
        HistoricalValue::Undefined => V::Undefined,
        HistoricalValue::Null => V::Null,
        HistoricalValue::Bool(v) => V::Bool(*v),
        HistoricalValue::Number(v) => V::number(v.as_f64()),
        HistoricalValue::String(v) => V::String(v.clone()),
        HistoricalValue::Array(v) => V::Array(v.iter().map(from_historical).collect()),
        HistoricalValue::Object(v) => V::Object(
            v.iter()
                .map(|(k, v)| (k.clone(), from_historical(v)))
                .collect(),
        ),
    }
}
fn preflight(values: &[V]) -> Result<()> {
    fn walk(v: &V, depth: usize, nodes: &mut usize, bytes: &mut usize) -> Result<()> {
        *nodes = nodes.checked_add(1).ok_or(TypesError::Capacity)?;
        if *nodes > validity_source::MAX_NODES || depth > validity_source::MAX_DEPTH {
            return Err(TypesError::Capacity);
        }
        match v {
            V::String(s) | V::BigNumber(s) => {
                *bytes = bytes.checked_add(s.len()).ok_or(TypesError::Capacity)?
            }
            V::Array(values) => {
                if values.len() > 1024 {
                    return Err(TypesError::Capacity);
                }
                for v in values {
                    walk(v, depth + 1, nodes, bytes)?;
                }
            }
            V::Object(fields) => {
                if fields.len() > 1024 {
                    return Err(TypesError::Capacity);
                }
                let mut names = std::collections::BTreeSet::new();
                for (key, value) in fields {
                    if !names.insert(key) {
                        return Err(TypesError::InvalidInput);
                    }
                    *bytes = bytes.checked_add(key.len()).ok_or(TypesError::Capacity)?;
                    walk(value, depth + 1, nodes, bytes)?;
                }
            }
            _ => (),
        }
        if *bytes > validity_source::MAX_BYTES {
            return Err(TypesError::Capacity);
        }
        Ok(())
    }
    if values.len() > MAX_ARGUMENTS {
        return Err(TypesError::Capacity);
    }
    let (mut nodes, mut bytes) = (0, 0);
    for value in values {
        walk(value, 0, &mut nodes, &mut bytes)?;
    }
    Ok(())
}
pub fn resolve_reference_address_unverified(reference: &V) -> Result<V> {
    preflight(core::slice::from_ref(reference))?;
    let found = match reference {
        V::String(v) => v.clone(),
        V::Array(values) => values
            .iter()
            .find_map(|v| match v.get("address") {
                V::String(s) if !s.is_empty() => Some(s),
                _ => None,
            })
            .unwrap_or_default(),
        V::Object(_) => match reference.get("address") {
            V::String(s) if !s.is_empty() => s,
            _ => match reference.get("senderAddress") {
                V::String(s) if !s.is_empty() => s,
                _ => String::new(),
            },
        },
        _ => String::new(),
    };
    Ok(V::String(found))
}

/// Default composition executes the actual bounded source validity port, using
/// caller-provided dependency observations. It never invokes economic mutation.
pub fn decode_payload_unverified<B: SourceBackend>(
    arguments: &[V],
    backend: &mut B,
) -> Result<TypesEvaluation> {
    decode_with_validity_unverified(arguments, &mut SourceValidityAdapter { backend })
}
/// An explicit replacement validity boundary, useful for dispatch experiments.
/// An identity adapter is dispatch-only evidence and is not source validation.
pub fn decode_with_validity_unverified<T: TypesValidity>(
    arguments: &[V],
    validity: &mut T,
) -> Result<TypesEvaluation> {
    preflight(arguments)?;
    let mut args = arguments.to_vec();
    let arg = |index| arguments.get(index).cloned().unwrap_or(V::Undefined);
    let mut decoder = None;
    let mut observed = None;
    let run = (|| -> Eval<TypesOutcome> {
        let marker = arg(2);
        let marker_matches = match &marker {
            V::String(s) => s == "tl",
            V::Array(_) | V::Object(_) | V::Promise | V::BigNumber(_) => {
                primitive_text(&marker)? == "tl"
            }
            _ => false,
        };
        if !marker_matches {
            return Ok(TypesOutcome::ReturnedError(exception(
                "Error",
                "Invalid payload",
            )));
        }
        // String concatenation in the source console.log is evaluated even
        // when the logger is silent and before switch dispatch.
        primitive_text(&arg(1))?;
        primitive_text(&arg(8))?;
        let type_id = match arg(1) {
            V::Number(bits) => {
                let n = f64::from_bits(bits);
                if n.is_finite() && n.fract() == 0.0 && (0.0..=35.0).contains(&n) {
                    n as u8
                } else {
                    return Ok(TypesOutcome::ReturnedError(exception(
                        "Error",
                        "Unknown transaction type",
                    )));
                }
            }
            _ => {
                return Ok(TypesOutcome::ReturnedError(exception(
                    "Error",
                    "Unknown transaction type",
                )))
            }
        };
        if matches!(type_id, 0 | 3 | 4) {
            primitive_text(&arg(3))?;
        }
        let body = match arg(3) {
            V::String(body) => body,
            value if nullish(&value) => {
                return Err(source_throw(
                    "TypeError",
                    format!(
                        "Cannot read properties of {} (reading 'substr')",
                        primitive_text(&value)?
                    ),
                ))
            }
            _ => {
                return Err(source_throw(
                    "TypeError",
                    "encodedPayload.substr is not a function",
                ))
            }
        };
        let route = source_route(type_id).ok_or(TypesError::InvalidInput)?;
        let mut call = DecoderObservation {
            method: route.decoder.into(),
            body: body.clone(),
            returned: None,
            thrown: None,
        };
        let decoded = if !route.decoder_exported {
            Err(exception(
                "TypeError",
                format!("Decode.{} is not a function", route.decoder),
            ))
        } else {
            match source_decode_historical_method_unverified(route.decoder, &body) {
                Ok(value) => Ok(from_historical(value.value())),
                Err(HistoricalDecoderError::SourceThrow { name, message }) => {
                    // Existing decoder errors expose UTF-8 String. Preserve the
                    // genuine UTF-16-only error observations as an explicit
                    // unsupported domain, never a replacement-character match.
                    if message == "Digit is not in alphabet: \u{fffd}"
                        && body.chars().any(|c| c as u32 > 0xffff)
                    {
                        return Err(TypesError::UnsupportedUtf16Exception.into());
                    }
                    Err(exception(name, message))
                }
                Err(error) => return Err(TypesError::Codec(error).into()),
            }
        };
        let mut params = match decoded {
            Ok(value) => {
                value.validate()?;
                call.returned = Some(value.clone());
                decoder = Some(call);
                value
            }
            Err(error) => {
                call.thrown = Some(error.clone());
                decoder = Some(call);
                return Err(Fault::Source(error));
            }
        };
        let sender = arg(4);
        let reference = arg(5);
        let txid = arg(0);
        let block = arg(8);
        // Every assignment remains in exact source order, including overwrite slots.
        let fields: &[&str] = match type_id {
            0 => &["block"],
            1 => &["senderAddress", "block"],
            2..=14 => &["senderAddress", "txid", "block"],
            15 => &[],
            16..=17 => &["senderAddress", "txid", "block"],
            18..=22 | 24..=26 | 28..=35 => &["block", "senderAddress", "txid"],
            23 => &["block", "senderAddress", "channelAddress", "txid"],
            27 => &["block", "blockHeight", "senderAddress", "txid"],
            _ => return Err(TypesError::InvalidInput.into()),
        };
        if type_id == 11 {
            let recipient = resolve_reference_address_unverified(&reference)?;
            assign(&mut params, "referenceAddress", recipient.clone())?;
            if !truth(&property(&params, "addressToGrantTo")?) && truth(&recipient) {
                assign(&mut params, "addressToGrantTo", recipient)?;
            }
        }
        for field in fields {
            let key = if type_id == 9 && *field == "senderAddress" {
                "sender"
            } else {
                field
            };
            let value = match *field {
                "block" | "blockHeight" => block.clone(),
                "txid" => txid.clone(),
                _ => sender.clone(),
            };
            assign(&mut params, key, value)?;
        }
        if type_id == 3 {
            let references = match &reference {
                V::Array(values) => values,
                value if nullish(value) => {
                    return Err(source_throw(
                        "TypeError",
                        format!(
                            "Cannot read properties of {} (reading 'find')",
                            primitive_text(value)?
                        ),
                    ))
                }
                _ => {
                    return Err(source_throw(
                        "TypeError",
                        "reference.find is not a function",
                    ))
                }
            };
            let find = |key: &str| -> Eval<Option<V>> {
                let desired = property(&params, key)?;
                for value in references {
                    if strict(&property(value, "vout")?, &desired) {
                        return Ok(Some(value.clone()));
                    }
                }
                Ok(None)
            };
            let payment = find("payToAddress")?;
            let delivery = find("tokenOutput")?;
            // The actual decoder returns a string/null tag, so this broken
            // integer-only branch is unreachable with genuine codec output.
            let tag = property(&params, "tagWithdraw")?;
            if let V::Number(bits) = tag {
                let n = f64::from_bits(bits);
                if n.is_finite() && n.fract() == 0.0 {
                    if let Some(cold) = find("tagWithdraw")? {
                        assign(&mut params, "tagWithdraw", property(&cold, "address")?)?;
                        return Err(source_throw(
                            "ReferenceError",
                            "coldWithdrawAddress is not defined",
                        ));
                    }
                }
            }
            if let (Some(payment), Some(delivery)) = (payment, delivery) {
                assign(
                    &mut params,
                    "satsPaymentAddress",
                    property(&payment, "address")?,
                )?;
                assign(
                    &mut params,
                    "tokenDeliveryAddress",
                    property(&delivery, "address")?,
                )?;
                assign(&mut params, "satsReceived", arg(7))?;
            } else {
                return Ok(TypesOutcome::Returned(params));
            }
        }
        if type_id == 23 {
            let names = [
                "txidNeutralized1",
                "txidNeutralized2",
                "markPrice",
                "settleType",
                "columnAIsSeller",
                "columnAIsMaker",
                "netAmount",
                "expiryBlock",
                "blockStart",
                "blockEnd",
                "propertyId",
                "aPaysBDirection",
                "channelRoot",
                "totalContracts",
                "neutralizedCount",
            ];
            let mut nested = Vec::new();
            for name in names {
                nested.push((name.into(), property(&params, name)?));
            }
            assign(&mut params, "txParams", V::Object(nested))?;
        }
        if !route.validator_invoked_by_source {
            return Ok(TypesOutcome::Returned(params));
        }
        let mut arguments = vec![sender, params, txid];
        if matches!(type_id, 4 | 11) {
            arguments.push(reference);
        }
        if type_id == 4 {
            arguments.push(arg(7));
        }
        preflight(&arguments)?;
        let call = SourceCall {
            method: route.validator.ok_or(TypesError::InvalidInput)?.into(),
            arguments,
        };
        let settlement = validity.validate(&call)?;
        validate_settlement(&settlement)?;
        if settlement.evaluation.dependencies.len() > validity_source::MAX_DEPENDENCIES
            || settlement.evaluation.deferred_rejections.len() > validity_source::MAX_DEPENDENCIES
        {
            return Err(TypesError::Capacity.into());
        }
        for e in &settlement.evaluation.deferred_rejections {
            if e.name.len().saturating_add(e.message.len()) > 4096 {
                return Err(TypesError::Capacity.into());
            }
        }
        preflight(&settlement.evaluation.arguments_after)?;
        match &settlement.evaluation.outcome {
            SourceOutcome::Returned(v) => {
                v.validate()?;
            }
            SourceOutcome::Thrown(e) if e.name.len().saturating_add(e.message.len()) > 4096 => {
                return Err(TypesError::Capacity.into())
            }
            SourceOutcome::Thrown(_) => (),
        }
        // These references were passed through to the validator. Preserve
        // observed deep mutations in the original nine-argument diagnostics.
        for (validator_index, original_index) in [(0, 4), (2, 0), (3, 5), (4, 7)] {
            if let (Some(original), Some(after)) = (
                args.get_mut(original_index),
                settlement.evaluation.arguments_after.get(validator_index),
            ) {
                if matches!(original, V::Object(_) | V::Array(_)) {
                    *original = after.clone();
                }
            }
        }
        let returned = settlement.evaluation.outcome.clone();
        observed = Some(ValidityObservation { call, settlement });
        let result = match returned {
            SourceOutcome::Returned(v) => v,
            SourceOutcome::Thrown(error) => return Err(Fault::Source(error)),
        };
        if matches!(type_id, 2 | 3 | 4 | 16 | 23 | 31) {
            primitive_text(&property(&result, "valid")?)?;
            primitive_text(&property(&result, "reason")?)?;
        }
        Ok(TypesOutcome::Returned(result))
    })();
    let outcome = match run {
        Ok(value) => value,
        Err(Fault::Source(error)) => TypesOutcome::Rejected(error),
        Err(Fault::Local(error)) => return Err(error),
    };
    preflight(&args)?;
    Ok(TypesEvaluation {
        outcome,
        arguments_after: args,
        decoder,
        validity: observed,
    })
}
