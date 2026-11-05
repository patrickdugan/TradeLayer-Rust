//! Strict text config and expected-head tokens for `tradelayer-listenerd`.
//!
//! The config names the `EconomicChainConfig` of one fixed-margin, oracle-priced
//! linear perpetual. It is deployment policy supplied next to the store, never
//! read from it: a store restored under a different config fails to load.
//!
//! Format, one `key = value` pair per line, ASCII, at most 4096 bytes:
//!
//! ```text
//! format = tradelayer-listenerd-config-v1
//! network = bitcoin
//! admin = DemoAdmin
//! contract = 1
//! collateral = 7
//! permitted_spot_properties = 7
//! rollback_limit = 16
//! inverse = false
//! notional = 1
//! initial_margin_per_contract = 10000.00
//! fee_route = oracle:1
//! marking = none
//! ```
//!
//! Blank lines and lines starting with `#` are ignored. Every key above is
//! required exactly once; unknown keys, duplicate keys, missing keys, empty
//! values and malformed values are rejected. There are no defaults.

use crate::chain::{BlockId, Tip};
use crate::contract_registry::ContractId;
use crate::decimal::Decimal;
use crate::economic::{EconomicConfig, MarkConfig, MarkPolicy, SettlementPolicy};
use crate::economic_chain::{EconomicChainConfig, EconomicHead};
use crate::ledger::{AccountId, PropertyId};
use crate::network_profile::TradeLayerMainnet;
use crate::runtime::{block_id_text, parse_block_id};
use crate::tally::FeeRoute;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

pub const CONFIG_FORMAT: &str = "tradelayer-listenerd-config-v1";
pub const MAX_CONFIG_BYTES: usize = 4096;
const KEYS: [&str; 12] = [
    "format",
    "network",
    "admin",
    "contract",
    "collateral",
    "permitted_spot_properties",
    "rollback_limit",
    "inverse",
    "notional",
    "initial_margin_per_contract",
    "fee_route",
    "marking",
];

/// Errors name keys and line numbers, never values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerConfigError {
    TooLarge,
    NotAscii,
    /// A line that is neither blank, a comment nor `key = value`.
    Syntax(usize),
    UnknownKey(String),
    DuplicateKey(String),
    MissingKey(&'static str),
    /// The value of this key is empty or malformed.
    Value(&'static str),
    /// The assembled config failed `EconomicChainConfig::validate`.
    Invalid,
    /// A config outside the fixed-margin oracle-perp shape cannot be written.
    Unsupported(&'static str),
    Head,
}
impl core::fmt::Display for ListenerConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooLarge => write!(f, "config exceeds {MAX_CONFIG_BYTES} bytes"),
            Self::NotAscii => write!(f, "config is not ASCII"),
            Self::Syntax(line) => write!(f, "config line {line} is not `key = value`"),
            Self::UnknownKey(key) => write!(f, "unknown config key `{key}`"),
            Self::DuplicateKey(key) => write!(f, "duplicate config key `{key}`"),
            Self::MissingKey(key) => write!(f, "missing config key `{key}`"),
            Self::Value(key) => write!(f, "invalid value for config key `{key}`"),
            Self::Invalid => write!(f, "config fails economic validation"),
            Self::Unsupported(what) => write!(f, "config is not representable: {what}"),
            Self::Head => write!(f, "expected head must be SEQ:HEIGHT:HASH:COMMITMENT"),
        }
    }
}
impl std::error::Error for ListenerConfigError {}

type Result<T> = core::result::Result<T, ListenerConfigError>;

pub fn parse_config(text: &str) -> Result<EconomicChainConfig> {
    if text.len() > MAX_CONFIG_BYTES {
        return Err(ListenerConfigError::TooLarge);
    }
    if !text.is_ascii() {
        return Err(ListenerConfigError::NotAscii);
    }
    let mut pairs: BTreeMap<&'static str, &str> = BTreeMap::new();
    for (index, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or(ListenerConfigError::Syntax(index + 1))?;
        let key = key.trim();
        let known = KEYS
            .iter()
            .find(|k| **k == key)
            .ok_or_else(|| ListenerConfigError::UnknownKey(key.chars().take(48).collect()))?;
        if pairs.insert(known, value.trim()).is_some() {
            return Err(ListenerConfigError::DuplicateKey((*known).to_owned()));
        }
    }
    let get = |key: &'static str| -> Result<&str> {
        let value = pairs
            .get(key)
            .copied()
            .ok_or(ListenerConfigError::MissingKey(key))?;
        if value.is_empty() {
            return Err(ListenerConfigError::Value(key));
        }
        Ok(value)
    };
    if get("format")? != CONFIG_FORMAT {
        return Err(ListenerConfigError::Value("format"));
    }
    let network = match get("network")? {
        "bitcoin" => TradeLayerMainnet::Bitcoin,
        "litecoin" => TradeLayerMainnet::Litecoin,
        "dogecoin" => TradeLayerMainnet::Dogecoin,
        _ => return Err(ListenerConfigError::Value("network")),
    };
    let admin =
        AccountId::try_from(get("admin")?).map_err(|_| ListenerConfigError::Value("admin"))?;
    let contract = ContractId::try_from(number32("contract", get("contract")?)?)
        .map_err(|_| ListenerConfigError::Value("contract"))?;
    let collateral = property("collateral", get("collateral")?)?;
    let permitted_spot_properties = match get("permitted_spot_properties")? {
        "none" => BTreeSet::new(),
        list => {
            let mut set = BTreeSet::new();
            for part in list.split(',') {
                if !set.insert(property("permitted_spot_properties", part)?) {
                    return Err(ListenerConfigError::Value("permitted_spot_properties"));
                }
            }
            set
        }
    };
    let rollback_limit = usize::try_from(number32("rollback_limit", get("rollback_limit")?)?)
        .map_err(|_| ListenerConfigError::Value("rollback_limit"))?;
    let inverse = match get("inverse")? {
        "true" => true,
        "false" => false,
        _ => return Err(ListenerConfigError::Value("inverse")),
    };
    let notional = decimal("notional", get("notional")?)?;
    let initial_margin_per_contract = decimal(
        "initial_margin_per_contract",
        get("initial_margin_per_contract")?,
    )?;
    let fee_route = FeeRoute::Oracle(oracle("fee_route", get("fee_route")?)?);
    let marking = match get("marking")? {
        "none" => None,
        text => Some(MarkConfig {
            oracle_id: oracle("marking", text)?,
            policy: MarkPolicy::ConservingMarkBasisV1,
        }),
    };
    let config = EconomicChainConfig {
        economics: EconomicConfig {
            native_entitlements: None,
            insurance: None,
            source_consensus: None,
            network,
            contract,
            collateral,
            inverse,
            notional,
            initial_margin_per_contract,
            fee_route,
            policy: SettlementPolicy::ConservingV1,
            funding: None,
            marking,
            liquidation: None,
            amm: None,
            contract_binding: None,
        },
        admin,
        permitted_spot_properties,
        rollback_limit,
    };
    config
        .validate()
        .map_err(|_| ListenerConfigError::Invalid)?;
    Ok(config)
}

/// Writes the config text that `parse_config` accepts. Fails for configs that
/// use features the daemon's config format does not carry.
pub fn render_config(config: &EconomicChainConfig) -> Result<String> {
    let e = &config.economics;
    if e.native_entitlements.is_some()
        || e.insurance.is_some()
        || e.source_consensus.is_some()
        || e.funding.is_some()
        || e.liquidation.is_some()
        || e.amm.is_some()
        || e.contract_binding.is_some()
    {
        return Err(ListenerConfigError::Unsupported(
            "optional economic modules",
        ));
    }
    let FeeRoute::Oracle(fee_oracle) = e.fee_route else {
        return Err(ListenerConfigError::Unsupported("fee route"));
    };
    let network = match e.network {
        TradeLayerMainnet::Bitcoin => "bitcoin",
        TradeLayerMainnet::Litecoin => "litecoin",
        TradeLayerMainnet::Dogecoin => "dogecoin",
    };
    let permitted = if config.permitted_spot_properties.is_empty() {
        "none".to_owned()
    } else {
        config
            .permitted_spot_properties
            .iter()
            .map(|p| p.get().to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    let marking = e
        .marking
        .as_ref()
        .map_or_else(|| "none".to_owned(), |m| format!("oracle:{}", m.oracle_id));
    let text = format!(
        "format = {CONFIG_FORMAT}\nnetwork = {network}\nadmin = {}\ncontract = {}\ncollateral = {}\npermitted_spot_properties = {permitted}\nrollback_limit = {}\ninverse = {}\nnotional = {}\ninitial_margin_per_contract = {}\nfee_route = oracle:{fee_oracle}\nmarking = {marking}\n",
        config.admin.as_str(),
        e.contract.get(),
        e.collateral.get(),
        config.rollback_limit,
        e.inverse,
        e.notional,
        e.initial_margin_per_contract,
    );
    if parse_config(&text)? != *config {
        return Err(ListenerConfigError::Unsupported("round trip"));
    }
    Ok(text)
}

fn number32(key: &'static str, raw: &str) -> Result<u32> {
    if raw.is_empty()
        || raw.len() > 10
        || !raw.bytes().all(|b| b.is_ascii_digit())
        || raw.len() > 1 && raw.starts_with('0')
    {
        return Err(ListenerConfigError::Value(key));
    }
    raw.parse().map_err(|_| ListenerConfigError::Value(key))
}
fn property(key: &'static str, raw: &str) -> Result<PropertyId> {
    PropertyId::try_from(number32(key, raw)?).map_err(|_| ListenerConfigError::Value(key))
}
fn oracle(key: &'static str, raw: &str) -> Result<u32> {
    let id = number32(
        key,
        raw.strip_prefix("oracle:")
            .ok_or(ListenerConfigError::Value(key))?,
    )?;
    if id == 0 {
        return Err(ListenerConfigError::Value(key));
    }
    Ok(id)
}
fn decimal(key: &'static str, raw: &str) -> Result<Decimal> {
    if raw.len() > 64 || !raw.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return Err(ListenerConfigError::Value(key));
    }
    Decimal::from_str(raw).map_err(|_| ListenerConfigError::Value(key))
}

/// `SEQUENCE:HEIGHT:HASH:COMMITMENT`, the token `tl_observe` prints for its head.
pub fn parse_expected_head(text: &str) -> Result<EconomicHead> {
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() != 4 {
        return Err(ListenerConfigError::Head);
    }
    let digits = |s: &str| {
        if s.is_empty() || s.len() > 20 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ListenerConfigError::Head);
        }
        s.parse::<u64>().map_err(|_| ListenerConfigError::Head)
    };
    Ok(EconomicHead {
        sequence: digits(parts[0])?,
        tip: Tip {
            height: digits(parts[1])?,
            id: parse_block_id(parts[2]).map_err(|_| ListenerConfigError::Head)?,
        },
        commitment: parse_block_id(parts[3])
            .map_err(|_| ListenerConfigError::Head)?
            .0,
    })
}

pub fn head_token(head: EconomicHead) -> String {
    format!(
        "{}:{}:{}:{}",
        head.sequence,
        head.tip.height,
        block_id_text(head.tip.id),
        block_id_text(BlockId(head.commitment))
    )
}
