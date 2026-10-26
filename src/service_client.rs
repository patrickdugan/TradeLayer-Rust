//! Both pinned Axios clients, and an explicitly separate repaired routing policy.
//! Requests bind to ServiceTransport; no client manufactures backend results.
use crate::bootstrap::{BootstrapControl, StopReason};
use crate::client::RpcValue;
use crate::issuance_intent::{JsonObject, JsonValue};
use crate::service::{
    ServiceError, ServiceMethod, ServiceRequest, ServiceResponse, ServiceTransport,
};
use std::collections::BTreeMap;

pub const INTERFACE_EXPRESS_SHA256: &str =
    "8cfa990d9aa460d29f5d309beb0987a040e06e2840632fac38e713d94836aab2";
pub const WALLET_INTERFACE_SHA256: &str =
    "d22fbe4ee631ca15fbb4fe4fb2af65c7a424ef4f249eb8686f8aa7b5fd2d8735";
pub const MAX_ARGUMENTS: usize = 32;
pub const MAX_ARGUMENT_BYTES: usize = 1024 * 1024;
pub const MAX_NUMBER_RESPONSE: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceClientKind {
    InterfaceExpress,
    WalletInterface,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientRouting {
    PinnedSource,
    RepairedServiceV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientMethod {
    InitMain,
    ListProperties,
    GetAllBalancesForAddress,
    GetActivations,
    GetOrderBook,
    GetContractOrderBook,
    ListContractSeries,
    ListOracles,
    GetContractPositionForAddressAndContractId,
    GetTradeHistory,
    GetContractTradeHistory,
    GetFundingHistory,
    GetOracleHistory,
    GetClearingHistory,
    GetWalletPositions,
    GetAddressesWithProperty,
    GetContractInfo,
    GetInitialMargin,
    GetColumn,
    GetMaxProcessedHeight,
    GetTrackHeight,
    CheckSync,
}
impl ClientMethod {
    pub const ALL: [Self; 22] = [
        Self::InitMain,
        Self::ListProperties,
        Self::GetAllBalancesForAddress,
        Self::GetActivations,
        Self::GetOrderBook,
        Self::GetContractOrderBook,
        Self::ListContractSeries,
        Self::ListOracles,
        Self::GetContractPositionForAddressAndContractId,
        Self::GetTradeHistory,
        Self::GetContractTradeHistory,
        Self::GetFundingHistory,
        Self::GetOracleHistory,
        Self::GetClearingHistory,
        Self::GetWalletPositions,
        Self::GetAddressesWithProperty,
        Self::GetContractInfo,
        Self::GetInitialMargin,
        Self::GetColumn,
        Self::GetMaxProcessedHeight,
        Self::GetTrackHeight,
        Self::CheckSync,
    ];
    pub fn name(self) -> &'static str {
        use ClientMethod::*;
        match self {
            InitMain => "initMain",
            ListProperties => "listProperties",
            GetAllBalancesForAddress => "getAllBalancesForAddress",
            GetActivations => "getActivations",
            GetOrderBook => "getOrderBook",
            GetContractOrderBook => "getContractOrderBook",
            ListContractSeries => "listContractSeries",
            ListOracles => "listOracles",
            GetContractPositionForAddressAndContractId => {
                "getContractPositionForAddressAndContractId"
            }
            GetTradeHistory => "getTradeHistory",
            GetContractTradeHistory => "getContractTradeHistory",
            GetFundingHistory => "getFundingHistory",
            GetOracleHistory => "getOracleHistory",
            GetClearingHistory => "getClearingHistory",
            GetWalletPositions => "getWalletPositions",
            GetAddressesWithProperty => "getAddressesWithProperty",
            GetContractInfo => "getContractInfo",
            GetInitialMargin => "getInitialMargin",
            GetColumn => "getColumn",
            GetMaxProcessedHeight => "getMaxProcessedHeight",
            GetTrackHeight => "getTrackHeight",
            CheckSync => "checkSync",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|method| method.name() == name)
    }
    pub fn supported_by(self, kind: SourceClientKind) -> bool {
        use ClientMethod::*;
        match kind {
            SourceClientKind::InterfaceExpress => !matches!(
                self,
                GetContractInfo
                    | GetInitialMargin
                    | GetColumn
                    | GetMaxProcessedHeight
                    | GetTrackHeight
                    | CheckSync
            ),
            SourceClientKind::WalletInterface => !matches!(
                self,
                GetClearingHistory | GetWalletPositions | GetAddressesWithProperty
            ),
        }
    }
}

/// Response data after Axios's default JSON-or-text transformation. Raw JSON
/// retains exact lexemes; callers may choose their own bounded typed decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientData {
    Json(RpcValue),
    Text(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientFailure {
    Transport(ServiceError),
    Http { status: u16, data: ClientData },
    SourceThrow { name: &'static str, message: String },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    Failure(ClientFailure),
    UnknownMethod,
    InvalidArguments,
    Capacity,
    UnsupportedSourcePath,
    UnsupportedSourceQuery,
    InvalidResponse,
}
impl core::fmt::Display for ClientError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Avoid exposing response bodies or wallet addresses in error logs.
        match self {
            Self::Failure(ClientFailure::Http { status, .. }) => {
                write!(f, "service HTTP status {status}")
            }
            Self::Failure(_) => f.write_str("service client failure"),
            other => write!(f, "{other:?}"),
        }
    }
}
impl std::error::Error for ClientError {}
type Result<T> = core::result::Result<T, ClientError>;
impl From<ServiceError> for ClientError {
    fn from(value: ServiceError) -> Self {
        Self::Failure(ClientFailure::Transport(value))
    }
}

/// IEEE binary64 result, including NaN, infinities, and negative zero. This is
/// historical Number(response.data), never a validated amount or balance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceNumber(u64);
impl SourceNumber {
    pub fn as_f64(self) -> f64 {
        f64::from_bits(self.0)
    }
    pub fn bits(self) -> u64 {
        self.0
    }
    fn new(value: f64) -> Self {
        Self(if value.is_nan() {
            f64::NAN.to_bits()
        } else {
            value.to_bits()
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientOutcome {
    Undefined {
        swallowed: Option<ClientFailure>,
    },
    Data(ClientData),
    /// Status/type/body projection; Axios config, socket, and headers are not
    /// invented by the generic service transport.
    Response(ServiceResponse),
    Number(SourceNumber),
}
pub struct ServiceClient<T> {
    kind: SourceClientKind,
    routing: ClientRouting,
    transport: T,
}
impl<T: ServiceTransport> ServiceClient<T> {
    pub fn new(kind: SourceClientKind, routing: ClientRouting, transport: T) -> Self {
        Self {
            kind,
            routing,
            transport,
        }
    }
    pub fn into_transport(self) -> T {
        self.transport
    }
    pub fn call_by_name(
        &mut self,
        name: &str,
        args: &[JsonValue],
        control: &BootstrapControl,
    ) -> Result<ClientOutcome> {
        let method = ClientMethod::from_name(name).ok_or(ClientError::UnknownMethod)?;
        self.call(method, args, control)
    }
    pub fn call(
        &mut self,
        method: ClientMethod,
        args: &[JsonValue],
        control: &BootstrapControl,
    ) -> Result<ClientOutcome> {
        check(control)?;
        let request = prepare_request(self.kind, self.routing, method, args)?;
        check(control)?;
        let result = self.transport.send(&request, control);
        check(control)?;
        let response = match result {
            Ok(response) => response,
            Err(error) => return self.failure(method, ClientFailure::Transport(error)),
        };
        // Trait implementations can construct response fields directly.
        if !(100..=599).contains(&response.status) {
            return Err(ClientError::InvalidResponse);
        }
        if response.body.len() > crate::service::MAX_SERVICE_RESPONSE {
            return Err(ClientError::Capacity);
        }
        let data = response_data(&response.body);
        check(control)?;
        let data = data?;
        if !(200..300).contains(&response.status) {
            return self.failure(
                method,
                ClientFailure::Http {
                    status: response.status,
                    data,
                },
            );
        }
        if method == ClientMethod::InitMain {
            return Ok(match self.kind {
                SourceClientKind::InterfaceExpress => ClientOutcome::Undefined { swallowed: None },
                SourceClientKind::WalletInterface => ClientOutcome::Response(response),
            });
        }
        if method == ClientMethod::GetInitialMargin {
            let result = number_response(&data);
            check(control)?;
            return result.map(ClientOutcome::Number);
        }
        Ok(ClientOutcome::Data(data))
    }
    fn failure(&self, method: ClientMethod, failure: ClientFailure) -> Result<ClientOutcome> {
        if self.kind == SourceClientKind::InterfaceExpress && method == ClientMethod::InitMain {
            Ok(ClientOutcome::Undefined {
                swallowed: Some(failure),
            })
        } else {
            Err(ClientError::Failure(failure))
        }
    }
}
fn check(control: &BootstrapControl) -> Result<()> {
    control.check().map_err(|reason| {
        match reason {
            StopReason::Cancelled => ServiceError::Cancelled,
            StopReason::DeadlineExceeded => ServiceError::Timeout,
        }
        .into()
    })
}
fn source_throw(name: &'static str, message: impl Into<String>) -> ClientError {
    ClientError::Failure(ClientFailure::SourceThrow {
        name,
        message: message.into(),
    })
}
fn validate_args(args: &[JsonValue]) -> Result<()> {
    if args.len() > MAX_ARGUMENTS {
        return Err(ClientError::Capacity);
    }
    let mut bytes = 0usize;
    for value in args {
        // JsonValue validates depth/nodes/encoded bytes before allocating its
        // encoding or any request field clones.
        let text = value.to_json().map_err(|_| ClientError::Capacity)?;
        bytes = bytes.checked_add(text.len()).ok_or(ClientError::Capacity)?;
        if bytes > MAX_ARGUMENT_BYTES {
            return Err(ClientError::Capacity);
        }
    }
    Ok(())
}
fn fields(names: &[&str], values: &[Option<&JsonValue>]) -> Result<RpcValue> {
    let mut object = JsonObject::new();
    for (name, value) in names.iter().zip(values) {
        if let Some(value) = value {
            object
                .insert((*name).to_owned(), (*value).clone())
                .map_err(|_| ClientError::Capacity)?;
        }
    }
    RpcValue::parse(&object.to_json().map_err(|_| ClientError::Capacity)?)
        .map_err(|_| ClientError::InvalidArguments)
}
fn parameter<'a>(value: Option<&'a JsonValue>, field: &str) -> Result<Option<&'a JsonValue>> {
    match value {
        None | Some(JsonValue::Null) => Err(source_throw(
            "TypeError",
            format!(
                "Cannot destructure property '{field}' of 'params' as it is {}.",
                if value.is_none() { "undefined" } else { "null" }
            ),
        )),
        Some(JsonValue::Object(object)) => Ok(object.get(field)),
        // ToObject on a JSON primitive/array has none of these named fields.
        Some(_) => Ok(None),
    }
}
fn js_string(value: Option<&JsonValue>) -> Result<String> {
    Ok(match value {
        None => "undefined".into(),
        Some(JsonValue::Null) => "null".into(),
        Some(JsonValue::Bool(value)) => value.to_string(),
        Some(JsonValue::String(value)) => value.clone(),
        Some(JsonValue::Number(value)) => crate::state_oracle::source_number_text(value.as_f64())
            .map_err(|_| ClientError::InvalidArguments)?,
        Some(JsonValue::Array(values)) => {
            let mut output = String::new();
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                if !matches!(value, JsonValue::Null) {
                    output.push_str(&js_string(Some(value))?);
                }
            }
            output
        }
        Some(JsonValue::Object(object)) => {
            if object.get("toString").is_some() {
                return Err(source_throw(
                    "TypeError",
                    "Cannot convert object to primitive value",
                ));
            }
            "[object Object]".into()
        }
    })
}
fn query_fields(names: &[&str], values: &[Option<&JsonValue>]) -> Result<BTreeMap<String, String>> {
    let mut query = BTreeMap::new();
    for (name, value) in names.iter().zip(values) {
        if let Some(value) = value.filter(|value| !matches!(value, JsonValue::Null)) {
            if matches!(value, JsonValue::Array(_) | JsonValue::Object(_)) {
                return Err(ClientError::UnsupportedSourceQuery);
            }
            query.insert((*name).to_owned(), js_string(Some(value))?);
        }
    }
    Ok(query)
}
fn interpolated(prefix: &str, values: &[Option<&JsonValue>]) -> Result<String> {
    let mut path = prefix.to_owned();
    for value in values {
        let part = js_string(*value)?;
        // The shared decoded path cannot preserve raw URL query/fragment,
        // dot-segment normalization, encoded slashes, or ambiguous separators.
        if part == "."
            || part == ".."
            || part
                .chars()
                .any(|c| matches!(c, '/' | '\\' | '?' | '#' | '%') || c.is_control())
        {
            return Err(ClientError::UnsupportedSourcePath);
        }
        path.push('/');
        path.push_str(&part);
    }
    Ok(path)
}

/// All active source exports. Extra arguments are ignored after bounded
/// validation, as JS functions do; missing arguments remain undefined.
pub fn prepare_request(
    kind: SourceClientKind,
    routing: ClientRouting,
    method: ClientMethod,
    args: &[JsonValue],
) -> Result<ServiceRequest> {
    use ClientMethod::*;
    if !method.supported_by(kind) {
        return Err(ClientError::UnknownMethod);
    }
    validate_args(args)?;
    let a = args.first();
    let b = args.get(1);
    let repaired = routing == ClientRouting::RepairedServiceV1;
    let wallet = kind == SourceClientKind::WalletInterface;
    let empty = RpcValue::parse("{}").map_err(|_| ClientError::InvalidArguments)?;
    let mut body = empty.clone();
    let mut query = BTreeMap::new();
    let mut present = true;
    let mut verb = ServiceMethod::Post;
    let path = if !repaired && !wallet {
        match method {
            InitMain => {
                body = fields(&["test"], &[a])?;
                "/initMain".into()
            }
            ListProperties => {
                present = false;
                "/listProperties".into()
            }
            GetAllBalancesForAddress => {
                body = fields(&["address"], &[a])?;
                "/getAllBalancesForAddress".into()
            }
            GetActivations => {
                present = false;
                "/getActivations".into()
            }
            GetOrderBook => {
                body = fields(&["propertyId1", "propertyId2"], &[a, b])?;
                "/getOrderBook".into()
            }
            GetContractOrderBook => {
                body = fields(&["contractId"], &[a])?;
                "/getContractOrderBook".into()
            }
            ListContractSeries => {
                present = false;
                "/listContractSeries".into()
            }
            ListOracles => {
                present = false;
                "/listOracles".into()
            }
            GetContractPositionForAddressAndContractId => {
                verb = ServiceMethod::Get;
                interpolated("/contractPosition", &[a, b])?
            }
            GetTradeHistory => {
                verb = ServiceMethod::Get;
                interpolated("/tradeHistory", &[a, b])?
            }
            GetContractTradeHistory => {
                verb = ServiceMethod::Get;
                interpolated("/contractTradeHistory", &[a])?
            }
            GetFundingHistory => {
                verb = ServiceMethod::Get;
                interpolated("/fundingHistory", &[a])?
            }
            GetOracleHistory => {
                verb = ServiceMethod::Get;
                interpolated("/oracleHistory", &[a])?
            }
            GetClearingHistory => {
                verb = ServiceMethod::Get;
                interpolated("/clearingHistory", &[a])?
            }
            GetWalletPositions => {
                verb = ServiceMethod::Get;
                "/walletPositions".into()
            }
            GetAddressesWithProperty => {
                verb = ServiceMethod::Get;
                interpolated("/addressesWithProperty", &[a])?
            }
            _ => return Err(ClientError::UnknownMethod),
        }
    } else if !repaired {
        match method {
            InitMain => {
                body = RpcValue::parse("{\"test\":true}")
                    .map_err(|_| ClientError::InvalidArguments)?;
                "/tl_initmain".into()
            }
            ListProperties => {
                present = false;
                "/tl_listproperties".into()
            }
            GetContractInfo => {
                verb = ServiceMethod::Get;
                query = query_fields(&["contractId"], &[a])?;
                "/tl_getContractInfo".into()
            }
            GetInitialMargin => {
                verb = ServiceMethod::Get;
                query = query_fields(&["contractId", "price"], &[a, b])?;
                "/tl_getInitMargin".into()
            }
            GetAllBalancesForAddress => {
                body = fields(&["params"], &[a])?;
                "/tl_getAllBalancesForAddress".into()
            }
            GetActivations => {
                present = false;
                "/tl_getactivations".into()
            }
            GetOrderBook => {
                return Err(source_throw("ReferenceError", "propertyId1 is not defined"))
            }
            GetContractOrderBook => {
                return Err(source_throw("ReferenceError", "contractId is not defined"))
            }
            ListContractSeries => {
                present = false;
                "/tl_listcontractseries".into()
            }
            ListOracles => {
                present = false;
                "/tl_listoracles".into()
            }
            GetColumn => {
                verb = ServiceMethod::Get;
                query = query_fields(
                    &["channel", "addressA", "addressB"],
                    &[
                        parameter(a, "channel")?,
                        parameter(a, "addressA")?,
                        parameter(a, "addressB")?,
                    ],
                )?;
                "/tl_getChannelColumn".into()
            }
            GetContractPositionForAddressAndContractId => {
                parameter(a, "address")?;
                verb = ServiceMethod::Get;
                "/tl_contractposition".into()
            }
            GetTradeHistory => {
                parameter(a, "propertyId1")?;
                verb = ServiceMethod::Get;
                "/tl_tradehistory".into()
            }
            GetContractTradeHistory => {
                parameter(a, "contractId")?;
                verb = ServiceMethod::Get;
                "/tl_contracttradehistory".into()
            }
            GetFundingHistory => {
                parameter(a, "contractId")?;
                verb = ServiceMethod::Get;
                "/tl_fundinghistory".into()
            }
            GetOracleHistory => {
                parameter(a, "oracleId")?;
                verb = ServiceMethod::Get;
                "/tl_oraclehistory".into()
            }
            GetMaxProcessedHeight => "/tl_getMaxProcessedHeight".into(),
            GetTrackHeight => "/tl_getTrackHeight".into(),
            CheckSync => "/tl_checkSync".into(),
            _ => return Err(ClientError::UnknownMethod),
        }
    } else {
        // Deliberately named routing repair. Argument signatures stay those of
        // the chosen source client, except explicit-address wallet positions.
        let get = |name: &str| if wallet { parameter(a, name) } else { Ok(a) };
        match method {
            InitMain => {
                body = if wallet {
                    let test = match a {
                        None => false,
                        Some(JsonValue::Bool(value)) => *value,
                        Some(_) => return Err(ClientError::InvalidArguments),
                    };
                    RpcValue::parse(if test {
                        "{\"test\":true}"
                    } else {
                        "{\"test\":false}"
                    })
                    .map_err(|_| ClientError::InvalidArguments)?
                } else {
                    fields(&["test"], &[a])?
                };
                "/tl_initmain".into()
            }
            ListProperties => "/tl_listProperties".into(),
            GetAllBalancesForAddress => {
                body = fields(&["address"], &[a])?;
                "/tl_getAllBalancesForAddress".into()
            }
            GetActivations => "/tl_getActivations".into(),
            GetOrderBook => {
                body = fields(
                    &["propertyId1", "propertyId2"],
                    &[
                        if wallet {
                            parameter(a, "propertyId1")?
                        } else {
                            a
                        },
                        if wallet {
                            parameter(a, "propertyId2")?
                        } else {
                            b
                        },
                    ],
                )?;
                "/tl_getOrderbook".into()
            }
            GetContractOrderBook => {
                body = fields(&["contractId"], &[get("contractId")?])?;
                "/tl_getContractOrderbook".into()
            }
            ListContractSeries => "/tl_listContractSeries".into(),
            ListOracles => "/tl_listOracles".into(),
            GetContractInfo => {
                verb = ServiceMethod::Get;
                query = query_fields(&["contractId"], &[a])?;
                "/tl_getContractInfo".into()
            }
            GetInitialMargin => {
                verb = ServiceMethod::Get;
                query = query_fields(&["contractId", "price"], &[a, b])?;
                "/tl_getInitMargin".into()
            }
            GetColumn => {
                body = fields(
                    &["channelAddress", "newCommitAddress", "cpAddress"],
                    &[
                        parameter(a, "channel")?,
                        parameter(a, "addressA")?,
                        parameter(a, "addressB")?,
                    ],
                )?;
                "/tl_getChannelColumn".into()
            }
            GetContractPositionForAddressAndContractId => {
                verb = ServiceMethod::Get;
                query = query_fields(
                    &["address", "contractId"],
                    &[
                        if wallet { parameter(a, "address")? } else { a },
                        if wallet {
                            parameter(a, "contractId")?
                        } else {
                            b
                        },
                    ],
                )?;
                "/tl_contractPosition".into()
            }
            GetTradeHistory => {
                verb = ServiceMethod::Get;
                query = query_fields(
                    &["propertyId1", "propertyId2"],
                    &[
                        if wallet {
                            parameter(a, "propertyId1")?
                        } else {
                            a
                        },
                        if wallet {
                            parameter(a, "propertyId2")?
                        } else {
                            b
                        },
                    ],
                )?;
                "/tl_tradeHistory".into()
            }
            GetContractTradeHistory => {
                verb = ServiceMethod::Get;
                query = query_fields(&["contractId"], &[get("contractId")?])?;
                "/tl_contractTradeHistory".into()
            }
            GetFundingHistory => {
                verb = ServiceMethod::Get;
                query = query_fields(&["contractId"], &[get("contractId")?])?;
                "/tl_fundingHistory".into()
            }
            GetOracleHistory => {
                verb = ServiceMethod::Get;
                query = query_fields(&["contractId"], &[get("oracleId")?])?;
                "/tl_oracleHistory".into()
            }
            GetClearingHistory => {
                verb = ServiceMethod::Get;
                interpolated("/clearingHistory", &[a])?
            }
            GetWalletPositions => {
                if !matches!(a,Some(JsonValue::String(value))if !value.is_empty()) {
                    return Err(ClientError::InvalidArguments);
                }
                verb = ServiceMethod::Get;
                query = query_fields(&["address"], &[a])?;
                "/walletPositions".into()
            }
            GetAddressesWithProperty => {
                verb = ServiceMethod::Get;
                interpolated("/addressesWithProperty", &[a])?
            }
            GetMaxProcessedHeight => "/tl_getMaxProcessedHeight".into(),
            GetTrackHeight => "/tl_getTrackHeight".into(),
            CheckSync => "/tl_checkSync".into(),
        }
    };
    let path = if !repaired {
        // Node's URL parser trims trailing ASCII spaces from the entire raw
        // interpolated URL. Interior spaces and nonbreaking spaces survive.
        let normalized = path.trim_end_matches(' ');
        if normalized
            .rsplit('/')
            .next()
            .is_some_and(|part| matches!(part, "." | ".."))
        {
            return Err(ClientError::UnsupportedSourcePath);
        }
        normalized.to_owned()
    } else {
        path
    };
    if verb == ServiceMethod::Get {
        present = false;
    }
    let request = ServiceRequest::new(verb, path, query, body)?;
    Ok(if present {
        request
    } else {
        request.without_body()
    })
}
fn response_data(body: &str) -> Result<ClientData> {
    // Axios tries JSON even without a JSON Content-Type and retains malformed
    // text. RpcValue additionally refuses duplicate object keys: use the
    // ordered JSON parser's source last-write behavior within its own bound.
    if let Ok(value) = RpcValue::parse(body) {
        return Ok(ClientData::Json(value));
    }
    if let Ok(value) = JsonValue::parse(body) {
        return Ok(ClientData::Json(
            RpcValue::parse(&value.to_json().map_err(|_| ClientError::Capacity)?)
                .map_err(|_| ClientError::InvalidResponse)?,
        ));
    }
    Ok(ClientData::Text(body.to_owned()))
}
fn number_response(data: &ClientData) -> Result<SourceNumber> {
    let raw = match data {
        ClientData::Json(value) => value.as_json(),
        ClientData::Text(value) => value,
    };
    if raw.len() > MAX_NUMBER_RESPONSE {
        return Err(ClientError::Capacity);
    }
    let number = match data {
        ClientData::Text(value) => number_string(value)?,
        ClientData::Json(value) => number_json(value, 0)?,
    };
    Ok(SourceNumber::new(number))
}
fn number_string(value: &str) -> Result<f64> {
    crate::bitvm_bundle::number(Some(&JsonValue::String(value.to_owned())))
        .map_err(|_| ClientError::Capacity)
}
fn number_json(value: &RpcValue, depth: usize) -> Result<f64> {
    let raw = value.as_json().trim();
    Ok(match raw.as_bytes().first() {
        Some(b'n') => 0.0,
        Some(b't') => 1.0,
        Some(b'f') => 0.0,
        Some(b'"') => number_string(
            &value
                .as_string()
                .map_err(|_| ClientError::InvalidResponse)?,
        )?,
        Some(b'[') => number_string(&array_string(value, depth)?)?,
        Some(b'{') => {
            if value
                .field("toString")
                .map_err(|_| ClientError::InvalidResponse)?
                .is_some()
            {
                return Err(source_throw(
                    "TypeError",
                    "Cannot convert object to primitive value",
                ));
            }
            f64::NAN
        }
        _ => raw.parse().map_err(|_| ClientError::InvalidResponse)?,
    })
}
fn array_string(value: &RpcValue, depth: usize) -> Result<String> {
    if depth > crate::client::MAX_RPC_DEPTH {
        return Err(ClientError::Capacity);
    }
    let values = value.as_array().map_err(|_| ClientError::InvalidResponse)?;
    let mut output = String::new();
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        let raw = value.as_json().trim();
        let part = match raw.as_bytes().first() {
            Some(b'n') => String::new(),
            Some(b'[') => array_string(value, depth + 1)?,
            Some(b'"') => value
                .as_string()
                .map_err(|_| ClientError::InvalidResponse)?,
            Some(b'{') => {
                if value
                    .field("toString")
                    .map_err(|_| ClientError::InvalidResponse)?
                    .is_some()
                {
                    return Err(source_throw(
                        "TypeError",
                        "Cannot convert object to primitive value",
                    ));
                }
                "[object Object]".into()
            }
            Some(b't' | b'f') => raw.to_owned(),
            _ => {
                let value: f64 = raw.parse().map_err(|_| ClientError::InvalidResponse)?;
                if value == 0.0 {
                    "0".into()
                } else if value.is_infinite() {
                    if value.is_sign_negative() {
                        "-Infinity".into()
                    } else {
                        "Infinity".into()
                    }
                } else {
                    crate::state_oracle::source_number_text(value)
                        .map_err(|_| ClientError::InvalidResponse)?
                }
            }
        };
        if output
            .len()
            .checked_add(part.len())
            .ok_or(ClientError::Capacity)?
            > MAX_NUMBER_RESPONSE
        {
            return Err(ClientError::Capacity);
        }
        output.push_str(&part);
    }
    Ok(output)
}
