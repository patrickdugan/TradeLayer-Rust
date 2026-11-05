//! Strict source-consensus section of the `tradelayer-node` config.
//!
//! The economic half of the node config is the `tradelayer-listenerd` config
//! (`listener_config`). That format carries no source-consensus policy, so the
//! source runner's settings live in this separate file, with the same
//! discipline: one `key = value` pair per line, ASCII, at most 4096 bytes,
//! every key required exactly once, no defaults.
//!
//! ```text
//! format = tradelayer-node-source-v1
//! policy = conserving-sequential-v2
//! block_mode = realtime
//! bootstrap = collateral-seed
//! collateral_seed_atoms = 100000000
//! bootstrap_commitment = <64 hex>
//! catalog_bootstrap_commitment = <64 hex>
//! ```
//!
//! `bootstrap = collateral-seed` is the only bootstrap the node can start
//! from. The economic state refuses a Ledger without its collateral property
//! (`EconomicConfig::collateral`), and a property needs a nonzero supply, so
//! the bootstrap Ledger holds exactly that property: created by the economic
//! config's administrator with `collateral_seed_atoms` atoms, all held by the
//! administrator. Property and clearlist catalogs start empty. The two
//! commitments are the independent pins `SourceConsensusConfig` requires;
//! `commitments_for_bootstrap` computes the values to pin, and a chain whose
//! pins differ from the bootstrap's is refused.
//! `block_mode` decides which payload types execute first inside a block
//! (`consensus_source::SourceBlockMode`); every node that must agree on state
//! must use the same mode.

use crate::chain::BlockId;
use crate::clearlist_source_owner::ClearlistSourceOwner;
use crate::consensus_source::{SourceBlockMode, SourceConsensusConfig, SourceConsensusPolicy};
use crate::economic::{EconomicError, EconomicState};
use crate::economic_chain::{EconomicChain, EconomicChainConfig};
use crate::hash256::double_sha256;
use crate::joint::JointProtocolState;
use crate::ledger::{Ledger, Operation, PropertyKind, TxId};
use crate::property_catalog::PropertyCatalog;
use crate::runtime::{block_id_text, parse_block_id};
use crate::source_catalogs::{bootstrap_commitment as catalog_commitment, SourceCatalogOwners};
use std::collections::BTreeMap;

pub const SOURCE_CONFIG_FORMAT: &str = "tradelayer-node-source-v1";
pub const MAX_SOURCE_CONFIG_BYTES: usize = 4096;
const KEYS: [&str; 7] = [
    "format",
    "policy",
    "block_mode",
    "bootstrap",
    "collateral_seed_atoms",
    "bootstrap_commitment",
    "catalog_bootstrap_commitment",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeConfigError {
    TooLarge,
    NotAscii,
    Syntax(usize),
    UnknownKey(String),
    DuplicateKey(String),
    MissingKey(&'static str),
    Value(&'static str),
    /// The pinned commitments differ from the bootstrap's.
    CommitmentMismatch,
    /// The economic config already carries a source-consensus section.
    SourceAlreadyConfigured,
    Economic,
}
impl core::fmt::Display for NodeConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooLarge => write!(f, "source config exceeds {MAX_SOURCE_CONFIG_BYTES} bytes"),
            Self::NotAscii => write!(f, "source config is not ASCII"),
            Self::Syntax(line) => write!(f, "source config line {line} is not `key = value`"),
            Self::UnknownKey(key) => write!(f, "unknown source config key `{key}`"),
            Self::DuplicateKey(key) => write!(f, "duplicate source config key `{key}`"),
            Self::MissingKey(key) => write!(f, "missing source config key `{key}`"),
            Self::Value(key) => write!(f, "invalid value for source config key `{key}`"),
            Self::CommitmentMismatch => {
                write!(
                    f,
                    "pinned bootstrap commitments differ from the bootstrap's"
                )
            }
            Self::SourceAlreadyConfigured => {
                write!(
                    f,
                    "the economic config already has a source-consensus section"
                )
            }
            Self::Economic => write!(f, "economic bootstrap failed"),
        }
    }
}
impl std::error::Error for NodeConfigError {}
impl From<EconomicError> for NodeConfigError {
    fn from(_: EconomicError) -> Self {
        Self::Economic
    }
}

type Result<T> = core::result::Result<T, NodeConfigError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeSourceConfig {
    pub policy: SourceConsensusPolicy,
    pub block_mode: SourceBlockMode,
    /// Atoms of the collateral property the bootstrap Ledger holds.
    pub collateral_seed_atoms: u128,
    pub bootstrap_commitment: [u8; 32],
    pub catalog_bootstrap_commitment: [u8; 32],
}

pub fn parse_source_config(text: &str) -> Result<NodeSourceConfig> {
    if text.len() > MAX_SOURCE_CONFIG_BYTES {
        return Err(NodeConfigError::TooLarge);
    }
    if !text.is_ascii() {
        return Err(NodeConfigError::NotAscii);
    }
    let mut pairs: BTreeMap<&'static str, &str> = BTreeMap::new();
    for (index, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or(NodeConfigError::Syntax(index + 1))?;
        let key = key.trim();
        let known = KEYS
            .iter()
            .find(|k| **k == key)
            .ok_or_else(|| NodeConfigError::UnknownKey(key.chars().take(48).collect()))?;
        if pairs.insert(known, value.trim()).is_some() {
            return Err(NodeConfigError::DuplicateKey((*known).to_owned()));
        }
    }
    let get = |key: &'static str| -> Result<&str> {
        let value = pairs
            .get(key)
            .copied()
            .ok_or(NodeConfigError::MissingKey(key))?;
        if value.is_empty() {
            return Err(NodeConfigError::Value(key));
        }
        Ok(value)
    };
    if get("format")? != SOURCE_CONFIG_FORMAT {
        return Err(NodeConfigError::Value("format"));
    }
    let policy = match get("policy")? {
        "conserving-sequential-v1" => SourceConsensusPolicy::ConservingSequentialV1,
        "conserving-sequential-v2" => SourceConsensusPolicy::ConservingSequentialV2,
        _ => return Err(NodeConfigError::Value("policy")),
    };
    let block_mode = match get("block_mode")? {
        "realtime" => SourceBlockMode::Realtime,
        "indexed-replay" => SourceBlockMode::IndexedReplay,
        _ => return Err(NodeConfigError::Value("block_mode")),
    };
    if get("bootstrap")? != "collateral-seed" {
        return Err(NodeConfigError::Value("bootstrap"));
    }
    let seed = get("collateral_seed_atoms")?;
    if seed.len() > 30 || !seed.bytes().all(|b| b.is_ascii_digit()) || seed.starts_with('0') {
        return Err(NodeConfigError::Value("collateral_seed_atoms"));
    }
    let collateral_seed_atoms = seed
        .parse::<u128>()
        .map_err(|_| NodeConfigError::Value("collateral_seed_atoms"))?;
    let commitment = |key: &'static str| -> Result<[u8; 32]> {
        let text = get(key)?;
        if text.len() != 64 || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(NodeConfigError::Value(key));
        }
        parse_block_id(text)
            .map(|id| id.0)
            .map_err(|_| NodeConfigError::Value(key))
    };
    Ok(NodeSourceConfig {
        policy,
        block_mode,
        collateral_seed_atoms,
        bootstrap_commitment: commitment("bootstrap_commitment")?,
        catalog_bootstrap_commitment: commitment("catalog_bootstrap_commitment")?,
    })
}

pub fn render_source_config(config: &NodeSourceConfig) -> String {
    let policy = match config.policy {
        SourceConsensusPolicy::ConservingSequentialV1 => "conserving-sequential-v1",
        SourceConsensusPolicy::ConservingSequentialV2 => "conserving-sequential-v2",
    };
    let mode = match config.block_mode {
        SourceBlockMode::Realtime => "realtime",
        SourceBlockMode::IndexedReplay => "indexed-replay",
    };
    format!(
        "format = {SOURCE_CONFIG_FORMAT}\npolicy = {policy}\nblock_mode = {mode}\nbootstrap = collateral-seed\ncollateral_seed_atoms = {}\nbootstrap_commitment = {}\ncatalog_bootstrap_commitment = {}\n",
        config.collateral_seed_atoms,
        block_id_text(BlockId(config.bootstrap_commitment)),
        block_id_text(BlockId(config.catalog_bootstrap_commitment)),
    )
}

fn empty_owners() -> SourceCatalogOwners {
    SourceCatalogOwners::new(PropertyCatalog::new(), ClearlistSourceOwner::new())
}

/// The bootstrap Ledger: the collateral property, created by the
/// administrator, all of it held by the administrator.
pub fn bootstrap_ledger(config: &EconomicChainConfig, seed_atoms: u128) -> Result<Ledger> {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            TxId::from_bytes(double_sha256(
                b"TradeLayer/NodeBootstrap/collateral-seed/v1",
            )),
            Operation::CreateProperty {
                id: config.economics.collateral,
                issuer: config.admin.clone(),
                kind: PropertyKind::Fixed,
                initial_atoms: seed_atoms,
            },
        )
        .map_err(|_| NodeConfigError::Value("collateral_seed_atoms"))?;
    Ok(ledger)
}

/// The `(bootstrap, catalog bootstrap)` commitments of the bootstrap under
/// `config`'s network, administrator, collateral and permitted spot
/// properties.
pub fn commitments_for_bootstrap(
    config: &EconomicChainConfig,
    seed_atoms: u128,
) -> Result<([u8; 32], [u8; 32])> {
    let joint = JointProtocolState::from_bootstrap_unverified(
        config.economics.network,
        config.admin.clone(),
        bootstrap_ledger(config, seed_atoms)?,
        config.permitted_spot_properties.clone(),
    )
    .map_err(|_| NodeConfigError::Economic)?;
    let ledger = EconomicState::source_bootstrap_commitment(&joint)?;
    let catalogs = catalog_commitment(
        &empty_owners()
            .checkpoint_bytes()
            .map_err(|_| NodeConfigError::Economic)?,
    );
    Ok((ledger, catalogs))
}

/// Sets the economic config's source-consensus section from `source`. The
/// listener config format carries none, so a present section is refused.
pub fn apply_source_config(
    config: &mut EconomicChainConfig,
    source: &NodeSourceConfig,
) -> Result<()> {
    if config.economics.source_consensus.is_some() {
        return Err(NodeConfigError::SourceAlreadyConfigured);
    }
    config.economics.source_consensus = Some(SourceConsensusConfig {
        bootstrap_commitment: source.bootstrap_commitment,
        policy: source.policy,
        catalog_bootstrap_commitment: Some(source.catalog_bootstrap_commitment),
    });
    config.validate().map_err(|_| NodeConfigError::Economic)
}

/// A pristine economic chain over the bootstrap, after checking that the
/// config's pinned commitments are the bootstrap's.
pub fn fresh_chain(config: EconomicChainConfig, seed_atoms: u128) -> Result<EconomicChain> {
    let pinned = config
        .economics
        .source_consensus
        .as_ref()
        .ok_or(NodeConfigError::Economic)?;
    let (ledger_commitment, catalog_commitment) = commitments_for_bootstrap(&config, seed_atoms)?;
    if pinned.bootstrap_commitment != ledger_commitment
        || pinned.catalog_bootstrap_commitment != Some(catalog_commitment)
    {
        return Err(NodeConfigError::CommitmentMismatch);
    }
    let ledger = bootstrap_ledger(&config, seed_atoms)?;
    Ok(EconomicChain::new_with_source_catalogs_unverified(
        config,
        ledger,
        empty_owners(),
    )?)
}
