//! Explicit RepairedServiceV1 routes for the two pinned Express listeners.
//! Dispatch validates requests; a backend must supply actual owned capabilities.
//! A successful query, prepared payload or observation grants no admission.
use crate::client::RpcValue;
use crate::decimal::Decimal;
use crate::ledger::AccountId;
use crate::service::{
    ServiceBackend, ServiceCall, ServiceError, ServiceMethod, ServiceRequest, ServiceResponse,
};
use std::collections::BTreeMap;
pub const MAX_ROUTE_LIST: usize = 64;
pub const MAX_SAFE_HEIGHT: u64 = 9_007_199_254_740_991;
type Result<T> = core::result::Result<T, ServiceError>;
fn invalid<T>() -> Result<T> {
    Err(ServiceError::InvalidRequest)
}
fn json(text: &str) -> Result<RpcValue> {
    RpcValue::parse(text).map_err(|_| ServiceError::InvalidRequest)
}
fn string(text: &str) -> Result<RpcValue> {
    RpcValue::string(text).map_err(|_| ServiceError::Capacity)
}
fn call<B: ServiceBackend>(
    backend: &mut B,
    target: &'static str,
    arguments: Vec<RpcValue>,
) -> Result<RpcValue> {
    backend.call(ServiceCall { target, arguments })
}
fn response(value: RpcValue) -> Result<ServiceResponse> {
    ServiceResponse::json(200, value)
}
fn unsigned_text(raw: &str, max: u64, zero: bool) -> Result<u64> {
    if raw.is_empty()
        || raw.len() > 20
        || !raw.bytes().all(|b| b.is_ascii_digit())
        || raw.len() > 1 && raw.starts_with('0')
    {
        return invalid();
    }
    let n = raw
        .parse::<u64>()
        .map_err(|_| ServiceError::InvalidRequest)?;
    if n > max || n == 0 && !zero {
        return invalid();
    }
    Ok(n)
}
fn unsigned(value: &RpcValue, max: u64, zero: bool) -> Result<RpcValue> {
    let text = if value.as_json().starts_with('"') {
        value
            .as_string()
            .map_err(|_| ServiceError::InvalidRequest)?
    } else {
        value.as_json().to_owned()
    };
    Ok(RpcValue::unsigned(unsigned_text(&text, max, zero)?))
}
fn address_text(raw: &str) -> Result<RpcValue> {
    if raw.starts_with('~') || AccountId::try_from(raw).is_err() {
        return invalid();
    }
    string(raw)
}
fn address(value: &RpcValue) -> Result<RpcValue> {
    address_text(
        &value
            .as_string()
            .map_err(|_| ServiceError::InvalidRequest)?,
    )
}
fn text(value: &RpcValue, max: usize, empty: bool) -> Result<String> {
    let text = value
        .as_string()
        .map_err(|_| ServiceError::InvalidRequest)?;
    if text.len() > max
        || !empty && text.is_empty()
        || !text.bytes().all(|b| (0x20..=0x7e).contains(&b))
    {
        return invalid();
    }
    Ok(text)
}
fn txid(value: &RpcValue) -> Result<RpcValue> {
    let raw = text(value, 64, false)?;
    if raw.len() != 64 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return invalid();
    }
    string(&raw.to_ascii_lowercase())
}
fn price(raw: &str) -> Result<RpcValue> {
    if raw.len() > 128 {
        return invalid();
    }
    let value: Decimal = raw.parse().map_err(|_| ServiceError::InvalidRequest)?;
    if value <= Decimal::zero() || value.coefficient_digits() > 40 || value.decimal_places() > 20 {
        return invalid();
    }
    string(&value.to_string())
}
fn boolean(value: Option<RpcValue>, default: bool) -> Result<bool> {
    match value {
        None => Ok(default),
        Some(v) if v.as_json() == "true" => Ok(true),
        Some(v) if v.as_json() == "false" => Ok(false),
        _ => invalid(),
    }
}
// Count top-level elements without cloning their JSON. RpcValue has already
// validated syntax, so this pass only enforces the route's allocation budget.
fn array(value: &RpcValue, maximum: usize) -> Result<Vec<RpcValue>> {
    let raw = value.as_json().trim();
    if !raw.starts_with('[') || !raw.ends_with(']') {
        return invalid();
    }
    let inner = &raw[1..raw.len() - 1];
    let mut count = usize::from(!inner.trim().is_empty());
    let (mut depth, mut quoted, mut escape) = (0usize, false, false);
    for b in inner.bytes() {
        if quoted {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                quoted = false;
            }
        } else {
            match b {
                b'"' => quoted = true,
                b'[' | b'{' => depth += 1,
                b']' | b'}' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => count += 1,
                _ => {}
            }
        }
        if count > maximum {
            return Err(ServiceError::Capacity);
        }
    }
    value.as_array().map_err(|_| ServiceError::InvalidRequest)
}
fn field(request: &ServiceRequest, name: &str) -> Result<Option<RpcValue>> {
    if !request.body_present {
        return Ok(None);
    }
    request
        .body
        .field(name)
        .map_err(|_| ServiceError::InvalidRequest)
}
fn required(request: &ServiceRequest, name: &str) -> Result<RpcValue> {
    field(request, name)?.ok_or(ServiceError::InvalidRequest)
}
fn bid(r: &ServiceRequest, name: &str) -> Result<RpcValue> {
    unsigned(&required(r, name)?, u64::from(u32::MAX), false)
}
fn baddress(r: &ServiceRequest, name: &str) -> Result<RpcValue> {
    address(&required(r, name)?)
}
fn balance_address(r: &ServiceRequest) -> Result<RpcValue> {
    match (field(r, "address")?, field(r, "params")?) {
        (Some(a), Some(p)) => {
            let a = address(&a)?;
            if a != address(&p)? {
                return invalid();
            }
            Ok(a)
        }
        (Some(a), None) | (None, Some(a)) => address(&a),
        (None, None) => invalid(),
    }
}
fn query<'a>(r: &'a ServiceRequest, name: &str) -> Result<&'a str> {
    r.query
        .get(name)
        .map(String::as_str)
        .ok_or(ServiceError::InvalidRequest)
}
fn qid(r: &ServiceRequest, name: &str) -> Result<RpcValue> {
    Ok(RpcValue::unsigned(unsigned_text(
        query(r, name)?,
        u64::from(u32::MAX),
        false,
    )?))
}
fn qaddress(r: &ServiceRequest, name: &str) -> Result<RpcValue> {
    address_text(query(r, name)?)
}
fn optional_height(r: &ServiceRequest, name: &str) -> Result<RpcValue> {
    r.query.get(name).map_or(Ok(RpcValue::null()), |v| {
        Ok(RpcValue::unsigned(unsigned_text(v, MAX_SAFE_HEIGHT, true)?))
    })
}
fn funding<B: ServiceBackend>(
    backend: &mut B,
    contract: RpcValue,
    start: RpcValue,
    end: RpcValue,
) -> Result<ServiceResponse> {
    if start.as_json() != "null"
        && end.as_json() != "null"
        && start.as_u64().map_err(|_| ServiceError::InvalidRequest)?
            > end.as_u64().map_err(|_| ServiceError::InvalidRequest)?
    {
        return invalid();
    }
    response(call(
        backend,
        "history.funding",
        vec![contract, start, end],
    )?)
}
fn heights<B: ServiceBackend>(backend: &mut B, path: &str) -> Result<ServiceResponse> {
    let value = call(backend, "runtime.heights", vec![])?;
    let get = |key| -> Result<RpcValue> {
        let v = value
            .field(key)
            .map_err(|_| ServiceError::Backend("runtime.heights"))?
            .unwrap_or_else(RpcValue::null);
        if v.as_json() != "null" {
            v.as_u64()
                .map_err(|_| ServiceError::Backend("runtime.heights"))?;
        }
        Ok(v)
    };
    let p = get("maxProcessedHeight")?;
    let i = get("maxParsedHeight")?;
    let t = get("trackHeight")?;
    let body = match path {
        "tl_getmaxprocessedheight" => format!("{{\"maxProcessedHeight\":{}}}", p.as_json()),
        "tl_getmaxparsedheight" => format!("{{\"maxParsedHeight\":{}}}", i.as_json()),
        "tl_gettrackheight" => format!(
            "{{\"trackHeight\":{},\"source\":\"{}\"}}",
            if t.as_json() == "null" {
                p.as_json()
            } else {
                t.as_json()
            },
            if t.as_json() == "null" {
                "maxProcessed"
            } else {
                "track"
            }
        ),
        _ => {
            let known = [&p, &i, &t].iter().all(|v| v.as_json() != "null");
            let synced = if known {
                if p.as_u64().ok() == i.as_u64().ok() && p.as_u64().ok() == t.as_u64().ok() {
                    "true"
                } else {
                    "false"
                }
            } else {
                "null"
            };
            format!("{{\"available\":{known},\"consensus\":{},\"txIndex\":{},\"trackHeight\":{},\"synced\":{synced}}}",p.as_json(),i.as_json(),t.as_json())
        }
    };
    response(json(&body)?)
}

/// Combined route set. Static names follow Express's default ASCII case-
/// insensitive matching and one optional trailing slash. Argument locations
/// remain explicit; a GET body never substitutes for a missing query value.
pub fn dispatch<B: ServiceBackend>(
    request: &ServiceRequest,
    backend: &mut B,
) -> Result<ServiceResponse> {
    request.validate()?;
    let path = if request.path.len() > 1 {
        request.path.strip_suffix('/').unwrap_or(&request.path)
    } else {
        &request.path
    };
    let parts: Vec<_> = path.strip_prefix('/').unwrap_or(path).split('/').collect();
    if parts.len() > 5 || parts.iter().any(|p| p.is_empty()) {
        return Err(ServiceError::NotFound);
    }
    let first = parts[0].to_ascii_lowercase();
    if request.method == ServiceMethod::Post
        && request.body_present
        && !request.body.as_json().trim_start().starts_with('{')
    {
        return invalid();
    }
    let get = request.method == ServiceMethod::Get;
    let post = request.method == ServiceMethod::Post;
    // Original path-parameter routes. Preserve case in every dynamic segment.
    if get {
        let target = match (first.as_str(), parts.len()) {
            ("addresseswithproperty", 2) => Some((
                "ledger.addresses_with_property",
                vec![RpcValue::unsigned(unsigned_text(
                    parts[1],
                    u64::from(u32::MAX),
                    false,
                )?)],
            )),
            ("contracttradehistory", 2) => Some((
                "history.contract",
                vec![RpcValue::unsigned(unsigned_text(
                    parts[1],
                    u64::from(u32::MAX),
                    false,
                )?)],
            )),
            ("tradehistory", 3) => Some((
                "history.spot",
                vec![
                    RpcValue::unsigned(unsigned_text(parts[1], u64::from(u32::MAX), false)?),
                    RpcValue::unsigned(unsigned_text(parts[2], u64::from(u32::MAX), false)?),
                ],
            )),
            ("spotprice", 3) => Some((
                "spot.last_price",
                vec![
                    RpcValue::unsigned(unsigned_text(parts[1], u64::from(u32::MAX), false)?),
                    RpcValue::unsigned(unsigned_text(parts[2], u64::from(u32::MAX), false)?),
                    optional_height(request, "height")?,
                ],
            )),
            ("spotvwap", 3) => Some((
                "spot.vwap",
                vec![
                    RpcValue::unsigned(unsigned_text(parts[1], u64::from(u32::MAX), false)?),
                    RpcValue::unsigned(unsigned_text(parts[2], u64::from(u32::MAX), false)?),
                    optional_height(request, "startBlock")?,
                    optional_height(request, "endBlock")?,
                ],
            )),
            ("fundinghistory", 2) => {
                return funding(
                    backend,
                    RpcValue::unsigned(unsigned_text(parts[1], u64::from(u32::MAX), false)?),
                    RpcValue::null(),
                    RpcValue::null(),
                )
            }
            ("oraclehistory", 2) => Some((
                "history.oracle",
                vec![RpcValue::unsigned(unsigned_text(
                    parts[1],
                    u64::from(u32::MAX),
                    false,
                )?)],
            )),
            ("clearinghistory", 2) => Some((
                "history.clearing",
                vec![RpcValue::unsigned(unsigned_text(
                    parts[1],
                    u64::from(u32::MAX),
                    false,
                )?)],
            )),
            ("contractposition", 3) => Some((
                "contract.position",
                vec![
                    address_text(parts[1])?,
                    RpcValue::unsigned(unsigned_text(parts[2], u64::from(u32::MAX), false)?),
                ],
            )),
            ("walletpositions", 2) => Some(("wallet.positions", vec![address_text(parts[1])?])),
            ("walletpositions", 1) => {
                Some(("wallet.positions", vec![qaddress(request, "address")?]))
            }
            ("walletbalances", 2) => Some(("wallet.balances", vec![address_text(parts[1])?])),
            ("walletposition", 3) => Some((
                "wallet.position",
                vec![
                    address_text(parts[1])?,
                    RpcValue::unsigned(unsigned_text(parts[2], u64::from(u32::MAX), false)?),
                ],
            )),
            _ => None,
        };
        if let Some((target, args)) = target {
            return response(call(backend, target, args)?);
        }
    }
    if parts.len() != 1 {
        return Err(ServiceError::NotFound);
    }
    let route = first.as_str();
    if post && matches!(route, "initmain" | "tl_initmain") {
        if boolean(field(request, "test")?, false)? {
            return Err(ServiceError::Unavailable("testnet_service"));
        }
        return response(call(backend, "runtime.initialize", vec![])?);
    }
    if get && route == "initstatus" {
        return response(call(backend, "runtime.status", vec![])?);
    }
    if post && route == "tl_pause" {
        return response(call(backend, "runtime.pause", vec![])?);
    }
    if post
        && matches!(
            route,
            "tl_getmaxprocessedheight"
                | "tl_getmaxparsedheight"
                | "tl_gettrackheight"
                | "tl_checksync"
        )
    {
        return heights(backend, route);
    }
    let (target, args) = match (post, get, route) {
        (true, _, "getallbalancesforaddress") => {
            ("ledger.balances", vec![baddress(request, "address")?])
        }
        (true, _, "tl_getallbalancesforaddress") => {
            ("ledger.balances", vec![balance_address(request)?])
        }
        (true, _, "listproperties" | "tl_listproperties") => ("ledger.properties", vec![]),
        (true, _, "getactivations" | "tl_getactivations") => ("registry.activations", vec![]),
        (true, _, "listoracles" | "tl_listoracles") => ("registry.oracles", vec![]),
        (true, _, "getorderbook" | "tl_getorderbook") => (
            "spot.book",
            vec![bid(request, "propertyId1")?, bid(request, "propertyId2")?],
        ),
        (true, _, "getcontractorderbook" | "tl_getcontractorderbook") => {
            ("contract.book", vec![bid(request, "contractId")?])
        }
        (true, _, "listcontractseries") => ("contract.list", vec![]),
        (true, _, "tl_listcontractseries") => {
            if field(request, "contractId")?.is_some() {
                ("contract.info", vec![bid(request, "contractId")?])
            } else {
                ("contract.list", vec![])
            }
        }
        (true, _, "tl_validateaddress") => {
            ("node.validate_address", vec![baddress(request, "address")?])
        }
        (true, _, "tl_gettransaction") => {
            ("transactions.get", vec![txid(&required(request, "txid")?)?])
        }
        (true, _, "tl_gettransactionsforaddress") => {
            ("transactions.address", vec![baddress(request, "address")?])
        }
        (true, _, "tl_gettransactionforblock") => (
            "transactions.block",
            vec![unsigned(
                &required(request, "blockHeight")?,
                MAX_SAFE_HEIGHT,
                true,
            )?],
        ),
        (true, _, "tl_getchannelcolumn") => (
            "channel.predict_column",
            vec![
                baddress(request, "channelAddress")?,
                baddress(request, "newCommitAddress")?,
                match field(request, "cpAddress")? {
                    None => RpcValue::null(),
                    Some(v) if v.as_json() == "null" => RpcValue::null(),
                    Some(v) => address(&v)?,
                },
            ],
        ),
        (true, _, "tl_getattestations") => (
            "registry.attestations",
            vec![
                baddress(request, "address")?,
                unsigned(&required(request, "id")?, u64::from(u32::MAX), true)?,
            ],
        ),
        (true, _, "tl_getissuanceintents") => {
            ("issuance.intents", vec![baddress(request, "address")?])
        }
        (true, _, "tl_preparecommitfromissuance") => {
            if boolean(field(request, "autoSend")?, false)? {
                return Err(ServiceError::Unavailable("authenticated_commit_broadcast"));
            }
            let lists = match field(request, "clearLists")? {
                None => RpcValue::null(),
                Some(v) => {
                    let values = array(&v, MAX_ROUTE_LIST)?;
                    if values.len() > MAX_ROUTE_LIST {
                        return Err(ServiceError::Capacity);
                    }
                    let values = values
                        .iter()
                        .map(|v| unsigned(v, u64::from(u32::MAX), true))
                        .collect::<Result<Vec<_>>>()?;
                    json(&format!(
                        "[{}]",
                        values
                            .iter()
                            .map(RpcValue::as_json)
                            .collect::<Vec<_>>()
                            .join(",")
                    ))?
                }
            };
            (
                "issuance.prepare_commit",
                vec![
                    txid(&required(request, "issuanceTxid")?)?,
                    baddress(request, "fromAddress")?,
                    baddress(request, "channelAddress")?,
                    RpcValue::boolean(boolean(field(request, "payEnabled")?, false)?),
                    lists,
                ],
            )
        }
        (true, _, "tl_loadwallet") => ("wallet.load", vec![]),
        (true, _, "tl_getchannel") => ("channel.get", vec![baddress(request, "params")?]),
        (true, _, "tl_createrawtx_opreturn") => {
            let params = array(&required(request, "params")?, 2)?;
            if params.len() != 2 {
                return invalid();
            }
            let raw = text(&params[0], 128 * 1024, false)?;
            if !raw.len().is_multiple_of(2) || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
                return invalid();
            }
            let payload = text(&params[1], 4096, true)?;
            (
                "wallet.add_op_return",
                vec![string(&raw.to_ascii_lowercase())?, string(&payload)?],
            )
        }
        (true, _, "tl_getproperty") => ("ledger.property", vec![bid(request, "params")?]),
        (true, _, "tl_listclearlists") => ("registry.clear_lists", vec![]),
        (true, _, "tl_showclearlist" | "tl_getclearlistbyid") => (
            "registry.clear_list",
            vec![unsigned(
                &required(request, "id")?,
                u64::from(u32::MAX),
                true,
            )?],
        ),
        (true, _, "tl_listfeecache") => ("fees.all", vec![]),
        (true, _, "tl_propertyfeecache") => (
            "fees.property",
            vec![unsigned(
                &field(request, "propertyId")?
                    .or(field(request, "id")?)
                    .ok_or(ServiceError::InvalidRequest)?,
                u64::from(u32::MAX),
                false,
            )?],
        ),
        (true, _, "tl_bitvmstatus") => {
            let property = field(request, "propertyId")?.map_or(Ok(RpcValue::null()), |v| {
                unsigned(&v, u64::from(u32::MAX), false)
            })?;
            let dlc =
                field(request, "dlcRef")?.map_or(Ok(String::new()), |v| text(&v, 256, true))?;
            (
                "procedural.bitvm_status",
                vec![property, string(dlc.trim())?],
            )
        }
        (_, true, "tl_contractposition") => (
            "contract.position",
            vec![qaddress(request, "address")?, qid(request, "contractId")?],
        ),
        (_, true, "tl_channelbalanceforcommiter") => (
            "channel.balance_for_committer",
            vec![qaddress(request, "address")?, qid(request, "propertyId")?],
        ),
        (_, true, "tl_getmaxsynth") => {
            let key = parse_property_key(query(request, "propId")?)?;
            (
                "synthetic.max",
                vec![qaddress(request, "address")?, string(&key.canonical())?],
            )
        }
        (_, true, "tl_tradehistory") => (
            "history.spot",
            vec![qid(request, "propertyId1")?, qid(request, "propertyId2")?],
        ),
        (_, true, "tl_tokentradehistoryforaddress") => (
            "history.spot_for_address",
            vec![
                qid(request, "propertyId1")?,
                qid(request, "propertyId2")?,
                qaddress(request, "address")?,
            ],
        ),
        (_, true, "tl_getinitmargin") => (
            "contract.initial_margin",
            vec![
                qid(request, "contractId")?,
                price(query(request, "price")?)?,
            ],
        ),
        (_, true, "tl_getcontractinfo") => ("contract.info", vec![qid(request, "contractId")?]),
        (_, true, "tl_contracttradehistory") => {
            ("history.contract", vec![qid(request, "contractId")?])
        }
        (_, true, "tl_contracttradehistoryforaddress") => (
            "history.contract_for_address",
            vec![qid(request, "contractId")?, qaddress(request, "address")?],
        ),
        (_, true, "tl_totaltradehistoryforaddress") => {
            ("history.address", vec![qaddress(request, "address")?])
        }
        (_, true, "tl_fundinghistory") => {
            return funding(
                backend,
                qid(request, "contractId")?,
                optional_height(request, "startBlock")?,
                optional_height(request, "endBlock")?,
            )
        }
        (_, true, "tl_oraclehistory") => ("history.oracle", vec![qid(request, "contractId")?]),
        _ => return Err(ServiceError::NotFound),
    };
    let result = call(backend, target, args)?;
    if route == "tl_getcontractinfo" && matches!(result.as_json(), "null" | "false") {
        return ServiceResponse::json(404, json("{\"error\":\"Contract not found\"}")?);
    }
    if route == "tl_getchannel" && result.as_json() == "null" {
        return response(json(
            "{\"message\":\"No channel found for the given address\",\"channel\":null}",
        )?);
    }
    if route == "tl_getissuanceintents" && result.as_json() == "null" {
        return response(json("[]")?);
    }
    response(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertyKey {
    pub base_property: u32,
    pub contract_id: Option<u32>,
}
impl PropertyKey {
    pub fn canonical(self) -> String {
        self.contract_id.map_or_else(
            || self.base_property.to_string(),
            |c| format!("s{}-{c}", self.base_property),
        )
    }
}
/// Named repair: canonical nonzero u32 property/contract identifiers. The
/// source also accepts negative/fractional mint IDs through Number coercion.
pub fn parse_property_key(raw: &str) -> Result<PropertyKey> {
    if raw.starts_with('s') || raw.starts_with('S') {
        let mut parts = raw[1..].split('-');
        let p = unsigned_text(parts.next().unwrap_or(""), u64::from(u32::MAX), false)?;
        let c = unsigned_text(parts.next().unwrap_or(""), u64::from(u32::MAX), false)?;
        if parts.next().is_some() {
            return invalid();
        }
        Ok(PropertyKey {
            base_property: p as u32,
            contract_id: Some(c as u32),
        })
    } else {
        Ok(PropertyKey {
            base_property: unsigned_text(raw, u64::from(u32::MAX), false)? as u32,
            contract_id: None,
        })
    }
}
pub fn parse_window_index(raw: &str) -> Result<Option<u64>> {
    if raw.len() > 256 {
        return Err(ServiceError::Capacity);
    }
    let Some(rest) = raw.strip_prefix("bitvm-risk-window-") else {
        return Ok(None);
    };
    let Some((property, index)) = rest.split_once('-') else {
        return Ok(None);
    };
    if property.is_empty()
        || index.is_empty()
        || !property
            .bytes()
            .chain(index.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return Ok(None);
    }
    let n = index
        .parse::<u64>()
        .map_err(|_| ServiceError::InvalidRequest)?;
    if n > MAX_SAFE_HEIGHT {
        return invalid();
    }
    Ok(Some(n))
}
/// Explicit environment snapshot, never implicit process-global configuration.
pub fn env_num(env: &BTreeMap<String, String>, name: &str, fallback: f64) -> Result<f64> {
    if !fallback.is_finite() {
        return invalid();
    }
    let Some(raw) = env.get(name).filter(|v| !v.is_empty()) else {
        return Ok(if fallback == 0.0 { 0.0 } else { fallback });
    };
    if raw.len() > 128 || !raw.is_ascii() {
        return invalid();
    }
    let s = raw.trim();
    let value = if s.is_empty() {
        0.0
    } else if let Some((digits, radix)) = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .map(|v| (v, 16))
        .or_else(|| {
            s.strip_prefix("0b")
                .or_else(|| s.strip_prefix("0B"))
                .map(|v| (v, 2))
        })
        .or_else(|| {
            s.strip_prefix("0o")
                .or_else(|| s.strip_prefix("0O"))
                .map(|v| (v, 8))
        })
    {
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii() && c.is_digit(radix)) {
            f64::NAN
        } else {
            let mut number = Decimal::zero();
            for digit in digits.chars() {
                number = number
                    .checked_mul(&Decimal::from(radix))
                    .and_then(|n| {
                        n.checked_add(&Decimal::from(
                            digit.to_digit(radix).expect("validated radix digit"),
                        ))
                    })
                    .map_err(|_| ServiceError::Capacity)?;
            }
            number
                .to_string()
                .parse::<f64>()
                .map_err(|_| ServiceError::InvalidRequest)?
        }
    } else {
        s.parse::<f64>().unwrap_or(f64::NAN)
    };
    Ok(if value.is_finite() {
        value
    } else if fallback == 0.0 {
        0.0
    } else {
        fallback
    })
}
/// Source's value/data/map nullish selection and JSON-string attempt. Resource
/// failures are explicit instead of falling back to an unbounded string.
pub fn parse_value(doc: &RpcValue) -> Result<RpcValue> {
    let mut selected = RpcValue::null();
    for key in ["value", "data", "map"] {
        if let Some(v) = doc.field(key).map_err(|_| ServiceError::InvalidRequest)? {
            if v.as_json() != "null" {
                selected = v;
                break;
            }
        }
    }
    if !selected.as_json().starts_with('"') {
        return Ok(selected);
    }
    let raw = selected
        .as_string()
        .map_err(|_| ServiceError::InvalidRequest)?;
    if raw.len() > 64 * 1024 {
        return Err(ServiceError::Capacity);
    }
    match crate::issuance_intent::JsonValue::parse(&raw) {
        Ok(v) => json(&v.to_json().map_err(|_| ServiceError::Capacity)?),
        Err(
            crate::issuance_intent::IntentError::Capacity
            | crate::issuance_intent::IntentError::DepthLimit,
        ) => Err(ServiceError::Capacity),
        Err(crate::issuance_intent::IntentError::InvalidJson) => Ok(selected),
        Err(_) => Err(ServiceError::InvalidRequest),
    }
}
