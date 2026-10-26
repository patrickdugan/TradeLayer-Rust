//! Bounded Core JSON-RPC over HTTP, with explicit credentials and network checks.
//! The transport is independent of protocol admission and never retries a call.
use crate::bootstrap::{BootstrapControl, StopReason};
use crate::network_profile::TradeLayerMainnet;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const MAX_RPC_BODY: usize = 16 * 1024 * 1024;
pub const MAX_RPC_REQUEST: usize = 1024 * 1024;
pub const MAX_RPC_HEADERS: usize = 16 * 1024;
pub const MAX_RPC_NODES: usize = 524_288;
pub const MAX_RPC_DEPTH: usize = 64;
const MAX_CREDENTIAL_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcError {
    InvalidConfig,
    InvalidMethod,
    InvalidJson,
    Capacity,
    Timeout,
    Cancelled,
    Transport(std::io::ErrorKind),
    InvalidHttp,
    HttpStatus(u16),
    InvalidResponse,
    WrongResponseId,
    Remote { code: i64, message: String },
    WrongChain,
    WrongGenesis,
    IdExhausted,
}
impl core::fmt::Display for RpcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Remote messages and transport bodies may contain wallet information.
        match self {
            Self::Remote { code, .. } => write!(f, "Core RPC error {code}"),
            other => write!(f, "{other:?}"),
        }
    }
}
impl std::error::Error for RpcError {}
type Result<T> = core::result::Result<T, RpcError>;
fn io_error(error: std::io::Error) -> RpcError {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => RpcError::Timeout,
        kind => RpcError::Transport(kind),
    }
}

/// Validated raw JSON preserves large block hex and exact integer lexemes.
/// It is not forced through the issuance store's smaller finite-Number domain.
#[derive(Clone, PartialEq, Eq)]
pub struct RpcValue(String);
impl core::fmt::Debug for RpcValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RpcValue")
            .field("bytes", &self.0.len())
            .finish()
    }
}
impl RpcValue {
    pub fn parse(text: &str) -> Result<Self> {
        validate_json(text)?;
        Ok(Self(text.to_owned()))
    }
    pub fn string(text: &str) -> Result<Self> {
        if text.len() > MAX_RPC_BODY / 6 {
            return Err(RpcError::Capacity);
        }
        Self::parse(&quote_json(text))
    }
    pub fn unsigned(value: u64) -> Self {
        Self(value.to_string())
    }
    pub fn boolean(value: bool) -> Self {
        Self(value.to_string())
    }
    pub fn null() -> Self {
        Self("null".to_owned())
    }
    pub fn as_json(&self) -> &str {
        &self.0
    }
    pub fn as_string(&self) -> Result<String> {
        json_string(&self.0)
    }
    pub fn as_u64(&self) -> Result<u64> {
        self.0.trim().parse().map_err(|_| RpcError::InvalidResponse)
    }
    pub fn field(&self, key: &str) -> Result<Option<Self>> {
        let fields = json_fields(&self.0)?;
        Ok(fields
            .into_iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| Self(value.to_owned())))
    }
    pub fn as_array(&self) -> Result<Vec<Self>> {
        json_array(&self.0).map(|items| {
            items
                .into_iter()
                .map(|item| Self(item.to_owned()))
                .collect()
        })
    }
}

/// Password/cookie bytes are never included by Debug or Display.
#[derive(Clone)]
pub enum RpcAuth {
    Basic { username: String, password: String },
    Cookie(PathBuf),
}
impl core::fmt::Debug for RpcAuth {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Basic { .. } => "Basic(<redacted>)",
            Self::Cookie(_) => "Cookie(<redacted>)",
        })
    }
}
impl RpcAuth {
    pub fn from_env(env: &BTreeMap<String, String>) -> Self {
        let value = |key: &str, default: &str| {
            env.get(key)
                .filter(|value| !value.is_empty())
                .cloned()
                .unwrap_or_else(|| default.to_owned())
        };
        Self::Basic {
            username: value("RPC_USER", "user"),
            password: value("RPC_PASS", "pass"),
        }
    }
    fn authorization(&self) -> Result<String> {
        let bytes = match self {
            Self::Basic { username, password } => {
                if username.contains(':')
                    || username.len() + password.len() + 1 > MAX_CREDENTIAL_BYTES
                {
                    return Err(RpcError::InvalidConfig);
                }
                format!("{username}:{password}").into_bytes()
            }
            Self::Cookie(path) => {
                let mut file = std::fs::File::open(path).map_err(io_error)?;
                let mut data = Vec::new();
                Read::by_ref(&mut file)
                    .take((MAX_CREDENTIAL_BYTES + 1) as u64)
                    .read_to_end(&mut data)
                    .map_err(io_error)?;
                if data.len() > MAX_CREDENTIAL_BYTES {
                    return Err(RpcError::Capacity);
                }
                while matches!(data.last(), Some(b'\r' | b'\n')) {
                    data.pop();
                }
                if !data.contains(&b':') {
                    return Err(RpcError::InvalidConfig);
                }
                data
            }
        };
        if bytes.is_empty() || bytes.iter().any(|b| *b < 0x20 || *b == 0x7f) {
            return Err(RpcError::InvalidConfig);
        }
        Ok(base64(&bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcBootstrap {
    pub source_chain: String,
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
    pub locked: bool,
    pub wallet: Option<String>,
}
impl RpcBootstrap {
    /// Explicit environment snapshot; never searches for a .env file or logs secrets.
    /// Invalid source Number ports/timeouts are rejected before opening a socket.
    pub fn from_env(env: &BTreeMap<String, String>, default_chain: &str) -> Result<Self> {
        let get = |name: &str| env.get(name).map(String::as_str).unwrap_or("");
        let chain = get("CHAIN").to_uppercase();
        let source_chain = if ["BTC", "LTC", "DOGE"].contains(&chain.as_str()) {
            chain
        } else {
            default_chain.to_owned()
        };
        let default_port = match source_chain.as_str() {
            "BTC" => 8332,
            "BTCTEST" => 18332,
            "LTC" => 9332,
            "LTCTEST" => 19332,
            "DOGE" => 22555,
            "DOGETEST" => 44555,
            _ => return Err(RpcError::InvalidConfig),
        };
        let port = if get("RPC_PORT").is_empty() {
            default_port
        } else {
            parse_config_integer(get("RPC_PORT"))?
        };
        let millis = if get("TIMEOUT_MS").is_empty() {
            60_000
        } else {
            parse_config_integer(get("TIMEOUT_MS"))?
        };
        if port == 0 || millis == 0 || millis > 300_000 {
            return Err(RpcError::InvalidConfig);
        }
        let wallet = ["RPC_WALLET", "LTC_WALLET", "WALLET_NAME"]
            .into_iter()
            .map(get)
            .find(|s| !s.is_empty())
            .map(str::to_owned);
        if wallet.as_ref().is_some_and(|w| w.len() > 1024) {
            return Err(RpcError::Capacity);
        }
        Ok(Self {
            source_chain,
            host: if get("RPC_HOST").is_empty() {
                "127.0.0.1".into()
            } else {
                get("RPC_HOST").into()
            },
            port: u16::try_from(port).map_err(|_| RpcError::InvalidConfig)?,
            timeout: Duration::from_millis(millis),
            locked: get("AUTODETECT") == "0"
                || !get("CHAIN").is_empty()
                || !get("RPC_PORT").is_empty(),
            wallet,
        })
    }
}
fn parse_config_integer(text: &str) -> Result<u64> {
    let text = text.trim();
    let value = if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|_| RpcError::InvalidConfig)?
    } else {
        let number: f64 = text.parse().map_err(|_| RpcError::InvalidConfig)?;
        if !number.is_finite() || number < 0.0 || number.fract() != 0.0 || number > 300_000.0 {
            return Err(RpcError::InvalidConfig);
        }
        number as u64
    };
    Ok(value)
}
pub fn determine_chain_from_subversion(subversion: &str) -> Result<TradeLayerMainnet> {
    let subversion = subversion.to_ascii_lowercase();
    if subversion.contains("litecoin") {
        Ok(TradeLayerMainnet::Litecoin)
    } else if subversion.contains("bitcoin") {
        Ok(TradeLayerMainnet::Bitcoin)
    } else if subversion.contains("dogecoin") {
        Ok(TradeLayerMainnet::Dogecoin)
    } else {
        Err(RpcError::WrongChain)
    }
}

/// Every active pinned wrapper, including the two distinct list-unspent methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRpcMethod {
    GetBlockchainInfo,
    GetRawTransaction,
    GetNewAddress,
    SendToAddress,
    GetNetworkInfo,
    GetTransaction,
    GetBlock,
    GetBlockHash,
    CreateRawTransaction,
    ListUnspent,
    DecodeRawTransaction,
    SignRawTransactionWithWallet,
    DumpPrivKey,
    SendRawTransaction,
    ValidateAddress,
    GetBlockCount,
    LoadWallet,
    ListLabels,
    GetAddressesByLabel,
    GetWalletInfo,
    ListUnspentLower,
}
impl SourceRpcMethod {
    pub fn name(self) -> &'static str {
        use SourceRpcMethod::*;
        match self {
            GetBlockchainInfo => "getblockchaininfo",
            GetRawTransaction => "getrawtransaction",
            GetNewAddress => "getnewaddress",
            SendToAddress => "sendtoaddress",
            GetNetworkInfo => "getnetworkinfo",
            GetTransaction => "gettransaction",
            GetBlock => "getblock",
            GetBlockHash => "getblockhash",
            CreateRawTransaction => "createrawtransaction",
            ListUnspent | ListUnspentLower => "listunspent",
            DecodeRawTransaction => "decoderawtransaction",
            SignRawTransactionWithWallet => "signrawtransactionwithwallet",
            DumpPrivKey => "dumpprivkey",
            SendRawTransaction => "sendrawtransaction",
            ValidateAddress => "validateaddress",
            GetBlockCount => "getblockcount",
            LoadWallet => "loadwallet",
            ListLabels => "listlabels",
            GetAddressesByLabel => "getaddressesbylabel",
            GetWalletInfo => "getwalletinfo",
        }
    }
    pub fn wallet_scoped(self) -> bool {
        use SourceRpcMethod::*;
        matches!(
            self,
            ListUnspent
                | ListUnspentLower
                | SignRawTransactionWithWallet
                | DumpPrivKey
                | ListLabels
                | GetAddressesByLabel
                | GetWalletInfo
        )
    }
    pub fn source_params(self, args: &[RpcValue]) -> Result<Vec<RpcValue>> {
        use SourceRpcMethod::*;
        if args.len() > 4096
            || args.iter().try_fold(0usize, |n, p| {
                n.checked_add(p.0.len()).ok_or(RpcError::Capacity)
            })? > MAX_RPC_REQUEST
        {
            return Err(RpcError::Capacity);
        }
        let count = match self {
            GetBlockchainInfo | GetNewAddress | GetNetworkInfo | GetBlockCount | GetWalletInfo
            | ListUnspentLower => 0,
            GetRawTransaction => {
                let txid = args.first().ok_or(RpcError::InvalidConfig)?.clone();
                return Ok(vec![
                    txid,
                    args.get(1)
                        .cloned()
                        .unwrap_or_else(|| RpcValue::boolean(true)),
                ]);
            }
            SendToAddress => 2,
            GetTransaction | GetBlock | GetBlockHash | GetAddressesByLabel => 1,
            _ => args.len(),
        };
        if args.len() < count {
            return Err(RpcError::InvalidConfig);
        }
        Ok(args[..count].to_vec())
    }
}

#[derive(Debug, Clone)]
pub struct RpcClient {
    endpoint: SocketAddr,
    auth: RpcAuth,
    wallet: Option<String>,
    timeout: Duration,
    expected_network: TradeLayerMainnet,
    next_id: u64,
}
impl RpcClient {
    /// Concrete local-node configuration without unbounded DNS resolution.
    /// Literal IPv4/IPv6 and localhost are accepted; other hosts can be resolved
    /// by the application's bounded resolver and supplied directly to `new`.
    pub fn from_env(
        env: &BTreeMap<String, String>,
        default_chain: &str,
        expected_network: TradeLayerMainnet,
    ) -> Result<Self> {
        let bootstrap = RpcBootstrap::from_env(env, default_chain)?;
        let ip = if bootstrap.host.eq_ignore_ascii_case("localhost") {
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        } else {
            bootstrap
                .host
                .parse()
                .map_err(|_| RpcError::InvalidConfig)?
        };
        Self::new(
            SocketAddr::new(ip, bootstrap.port),
            RpcAuth::from_env(env),
            bootstrap.wallet,
            bootstrap.timeout,
            expected_network,
        )
    }
    pub fn new(
        endpoint: SocketAddr,
        auth: RpcAuth,
        wallet: Option<String>,
        timeout: Duration,
        expected_network: TradeLayerMainnet,
    ) -> Result<Self> {
        if endpoint.port() == 0
            || timeout.is_zero()
            || timeout > Duration::from_secs(300)
            || wallet.as_ref().is_some_and(|s| s.len() > 1024)
        {
            return Err(RpcError::InvalidConfig);
        }
        auth.authorization()?;
        Ok(Self {
            endpoint,
            auth,
            wallet,
            timeout,
            expected_network,
            next_id: 1,
        })
    }
    pub fn call_source(&mut self, method: SourceRpcMethod, args: &[RpcValue]) -> Result<RpcValue> {
        self.call(
            method.name(),
            &method.source_params(args)?,
            method.wallet_scoped(),
        )
    }
    pub fn call(
        &mut self,
        method: &str,
        params: &[RpcValue],
        wallet_scoped: bool,
    ) -> Result<RpcValue> {
        self.call_inner(method, params, wallet_scoped, None)
    }
    pub fn expected_network(&self) -> TradeLayerMainnet {
        self.expected_network
    }
    /// Cooperatively checks cancellation between socket operations. A blocked
    /// operation is bounded by the smaller request/control deadline.
    pub fn call_controlled(
        &mut self,
        method: &str,
        params: &[RpcValue],
        wallet_scoped: bool,
        control: &BootstrapControl,
    ) -> Result<RpcValue> {
        let result = self.call_inner(method, params, wallet_scoped, Some(control));
        rpc_check_control(Some(control))?;
        result
    }
    fn call_inner(
        &mut self,
        method: &str,
        params: &[RpcValue],
        wallet_scoped: bool,
        control: Option<&BootstrapControl>,
    ) -> Result<RpcValue> {
        rpc_check_control(control)?;
        if method.is_empty()
            || method.len() > 96
            || !method
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(RpcError::InvalidMethod);
        }
        let param_bytes = params.iter().try_fold(2usize, |n, p| {
            n.checked_add(p.0.len() + 1).ok_or(RpcError::Capacity)
        })?;
        if param_bytes > MAX_RPC_REQUEST || params.len() > 4096 {
            return Err(RpcError::Capacity);
        }
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or(RpcError::IdExhausted)?;
        let body = format!(
            "{{\"jsonrpc\":\"1.0\",\"id\":{id},\"method\":{},\"params\":[{}]}}",
            quote_json(method),
            params
                .iter()
                .map(|p| p.0.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
        if body.len() > MAX_RPC_REQUEST {
            return Err(RpcError::Capacity);
        }
        let path = if wallet_scoped {
            self.wallet
                .as_ref()
                .map(|w| format!("/wallet/{}", percent_encode(w)))
                .unwrap_or_else(|| "/".into())
        } else {
            "/".into()
        };
        let auth = self.auth.authorization()?;
        let request = format!("POST {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Basic {auth}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", self.endpoint, body.len());
        let started = Instant::now();
        rpc_check_control(control)?;
        let timeout = match control.and_then(BootstrapControl::deadline) {
            Some(deadline) => self.timeout.min(
                deadline
                    .checked_duration_since(started)
                    .filter(|d| !d.is_zero())
                    .ok_or(RpcError::Timeout)?,
            ),
            None => self.timeout,
        };
        let mut stream = TcpStream::connect_timeout(&self.endpoint, timeout).map_err(io_error)?;
        let mut pending = request.as_bytes();
        while !pending.is_empty() {
            rpc_check_control(control)?;
            stream
                .set_write_timeout(Some(remaining(started, timeout)?))
                .map_err(io_error)?;
            let written = stream.write(pending).map_err(io_error)?;
            if written == 0 {
                return Err(RpcError::Transport(std::io::ErrorKind::WriteZero));
            }
            pending = &pending[written..];
        }
        let (status, body) = read_http(&mut stream, started, timeout, control)?;
        let fields = json_fields(&body).map_err(|_| {
            if status == 200 {
                RpcError::InvalidResponse
            } else {
                RpcError::HttpStatus(status)
            }
        })?;
        rpc_check_control(control)?;
        remaining(started, timeout)?;
        let lookup = |key: &str| {
            fields
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| *value)
        };
        if lookup("id").and_then(|s| s.trim().parse::<u64>().ok()) != Some(id) {
            return Err(RpcError::WrongResponseId);
        }
        if let Some(error) = lookup("error").filter(|s| s.trim() != "null") {
            let error = RpcValue::parse(error)?;
            let code = error
                .field("code")?
                .ok_or(RpcError::InvalidResponse)?
                .0
                .parse::<i64>()
                .map_err(|_| RpcError::InvalidResponse)?;
            let message = error
                .field("message")?
                .ok_or(RpcError::InvalidResponse)?
                .as_string()?;
            rpc_check_control(control)?;
            remaining(started, timeout)?;
            return Err(RpcError::Remote { code, message });
        }
        if status != 200 {
            return Err(RpcError::HttpStatus(status));
        }
        let result = RpcValue::parse(lookup("result").ok_or(RpcError::InvalidResponse)?)?;
        rpc_check_control(control)?;
        remaining(started, timeout)?;
        Ok(result)
    }
    /// Readiness is independently bound to chain family, mainnet and genesis.
    /// A mismatch does not rewrite the expected network or redirect the endpoint.
    pub fn verify_network(&mut self) -> Result<()> {
        self.verify_network_inner(None)
    }
    pub fn verify_network_controlled(&mut self, control: &BootstrapControl) -> Result<()> {
        let result = self.verify_network_inner(Some(control));
        rpc_check_control(Some(control))?;
        result
    }
    fn verify_network_inner(&mut self, control: Option<&BootstrapControl>) -> Result<()> {
        let network = self.call_inner("getnetworkinfo", &[], false, control)?;
        let version = network
            .field("subversion")?
            .ok_or(RpcError::InvalidResponse)?
            .as_string()?;
        // Keep the source helper's literal-name behavior separately. Real Core
        // defaults also advertise Satoshi/Shibetoshi; readiness supports those.
        let product = version.to_ascii_lowercase();
        let runtime_chain = if product.starts_with("/satoshi:") {
            TradeLayerMainnet::Bitcoin
        } else if product.starts_with("/shibetoshi:") {
            TradeLayerMainnet::Dogecoin
        } else if product.starts_with("/litecoincore:") {
            TradeLayerMainnet::Litecoin
        } else {
            determine_chain_from_subversion(&version)?
        };
        if runtime_chain != self.expected_network {
            return Err(RpcError::WrongChain);
        }
        let blockchain = self.call_inner("getblockchaininfo", &[], false, control)?;
        if blockchain
            .field("chain")?
            .ok_or(RpcError::InvalidResponse)?
            .as_string()?
            != "main"
        {
            return Err(RpcError::WrongChain);
        }
        let genesis = self
            .call_inner("getblockhash", &[RpcValue::unsigned(0)], false, control)?
            .as_string()?;
        let expected = self
            .expected_network
            .genesis()
            .0
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if genesis != expected {
            return Err(RpcError::WrongGenesis);
        }
        rpc_check_control(control)?;
        Ok(())
    }
}
fn rpc_check_control(control: Option<&BootstrapControl>) -> Result<()> {
    if let Some(control) = control {
        control.check().map_err(|stop| match stop {
            StopReason::Cancelled => RpcError::Cancelled,
            StopReason::DeadlineExceeded => RpcError::Timeout,
        })?;
    }
    Ok(())
}
fn remaining(start: Instant, timeout: Duration) -> Result<Duration> {
    timeout
        .checked_sub(start.elapsed())
        .filter(|left| !left.is_zero())
        .ok_or(RpcError::Timeout)
}
fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as usize;
        let b = chunk.get(1).copied().unwrap_or(0) as usize;
        let c = chunk.get(2).copied().unwrap_or(0) as usize;
        out.push(TABLE[a >> 2] as char);
        out.push(TABLE[((a & 3) << 4) | (b >> 4)] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((b & 15) << 2) | (c >> 6)] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[c & 63] as char
        } else {
            '='
        });
    }
    out
}

include!("rpc_json_impl.rs");
include!("rpc_http_impl.rs");
