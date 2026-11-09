//! Deterministic, chain-independent TradeLayer registry transition prototype.
//!
//! Registry transitions for tx 0, 7, 8, 9, 13, 14 and 15 are ported here from
//! their JavaScript *intent* with explicit rule corrections. These operations
//! must receive an independently authenticated actor from a future chain adapter.
//! Supplying an AccountId is NOT evidence that its signing key was verified.
//! This module is not integrated with live chain consensus or the Rust ledger.

use crate::ledger::{AccountId, TxId};
use crate::zk_verify::{StatementReceipt, ZkEffect, ZkFeePolicy, ZkProgram, ZkSystem, ZK_TX_TYPE};
use std::collections::{BTreeMap, BTreeSet};

#[path = "registry_zk.rs"]
mod zk;

pub const MAX_REGISTRY_BATCH: usize = 10_000;
pub const MAX_NAME_BYTES: usize = 64;
pub const MAX_URL_BYTES: usize = 256;
pub const MAX_METADATA_BYTES: usize = 512;
pub const MAX_ACTIVATION_TYPE: u8 = 35;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Activation {
    pub height: u64,
    pub code_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Whitelist {
    pub admin: AccountId,
    pub backup: AccountId,
    pub name: String,
    pub url: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
    pub metadata: String,
    pub issued_height: u64,
    pub updated_height: u64,
    pub active: bool,
}

/// Oracle prices are nonzero integers in 1/10,000 quote-unit increments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleQuote {
    pub price_1e4: u128,
    pub high_1e4: Option<u128>,
    pub low_1e4: Option<u128>,
    pub close_1e4: Option<u128>,
    pub height: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Oracle {
    pub admin: AccountId,
    pub backup: AccountId,
    pub ticker: String,
    pub url: String,
    pub lag: u32,
    pub closed: bool,
    pub latest: Option<OracleQuote>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminTarget {
    Whitelist(u32),
    Oracle(u32),
    /// Requires coordinated mutation of ledger issuer ownership; not yet ported.
    Token(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryAction {
    /// The historical tx0 also creates network-native balances. This action
    /// deliberately records activations only; native genesis is unimplemented.
    Activate {
        types: Vec<u8>,
        code_hash: [u8; 32],
    },
    CreateWhitelist {
        name: String,
        url: String,
        description: String,
        backup: AccountId,
    },
    UpdateAdmin {
        target: AdminTarget,
        new_address: AccountId,
        update_backup: bool,
    },
    Attest {
        list_id: u32,
        subject: AccountId,
        metadata: String,
    },
    RevokeAttestation {
        list_id: u32,
        subject: AccountId,
    },
    CreateOracle {
        ticker: String,
        url: String,
        backup: AccountId,
        lag: u32,
    },
    PublishOracle {
        oracle_id: u32,
        price_1e4: u128,
        high_1e4: Option<u128>,
        low_1e4: Option<u128>,
        close_1e4: Option<u128>,
    },
    CloseOracle {
        oracle_id: u32,
    },
    /// `ZkVerifyV1` only. Admin only; entries are immutable and keyed by `program_id`, which is
    /// the SHA-256 of the canonical encoding of every other field (`zk_verify::program_id_of`).
    RegisterZkProgram {
        system: ZkSystem,
        program_id: [u8; 32],
        key_bytes: Vec<u8>,
        public_input_layout: u32,
        effect: ZkEffect,
        verifier_version: u32,
        max_verify_ms: u32,
    },
}

impl RegistryAction {
    pub const fn tx_type(&self) -> u8 {
        match self {
            Self::Activate { .. } => 0,
            Self::CreateWhitelist { .. } => 7,
            Self::UpdateAdmin { .. } => 8,
            Self::Attest { .. } | Self::RevokeAttestation { .. } => 9,
            Self::CreateOracle { .. } => 13,
            Self::PublishOracle { .. } => 14,
            Self::CloseOracle { .. } => 15,
            Self::RegisterZkProgram { .. } => ZK_TX_TYPE,
        }
    }
}

/// Order in the containing block MUST come from authenticated chain data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryTx {
    pub txid: TxId,
    pub actor: AccountId,
    pub height: u64,
    pub action: RegistryAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    EmptyBatch,
    BatchTooLarge,
    InvalidHeight,
    HeightRegression,
    Replay(TxId),
    Unauthorized,
    EmptyActivation,
    DuplicateActivation(u8),
    AlreadyActivated(u8),
    InvalidType(u8),
    ConflictingType34,
    NotActive(u8),
    SameBlockActivation(u8),
    InvalidField(&'static str),
    IdExhausted,
    UnknownWhitelist(u32),
    UnknownOracle(u32),
    UnsupportedTokenAdmin,
    UnsupportedSelfCertification,
    ExistingAttestation,
    MissingAttestation,
    ClosedOracle(u32),
    InvalidPrice,
    ZkVerifyDisabled,
    DuplicateZkProgram,
    ZkSystemReserved,
    ZkProgramIdMismatch,
    ZkRegistryFull,
}

impl core::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RegistryError {}

/// An independently cloneable state machine. Clone-before-commit also makes
/// batch failure and bounded fork tests deterministic and all-or-nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry {
    protocol_admin: AccountId,
    activations: [Option<Activation>; 36],
    whitelists: BTreeMap<u32, Whitelist>,
    attestations: BTreeMap<(u32, AccountId), Attestation>,
    oracles: BTreeMap<u32, Oracle>,
    /// `ZkVerifyV1` state. `zk_fee` is `Some` exactly when the policy is on; with the policy off
    /// all four stay empty and the snapshot is the unchanged v1 encoding.
    zk_fee: Option<ZkFeePolicy>,
    zk_activation: Option<Activation>,
    zk_programs: BTreeMap<[u8; 32], ZkProgram>,
    zk_receipts: Vec<StatementReceipt>,
    next_whitelist_id: u32,
    next_oracle_id: u32,
    processed: BTreeSet<TxId>,
    last_height: Option<u64>,
}

impl Registry {
    /// Caller MUST source the protocol administrator from verified genesis
    /// configuration for its exact chain and network.
    pub fn new(protocol_admin: AccountId) -> Self {
        Self {
            protocol_admin,
            activations: [None; 36],
            whitelists: BTreeMap::new(),
            attestations: BTreeMap::new(),
            oracles: BTreeMap::new(),
            zk_fee: None,
            zk_activation: None,
            zk_programs: BTreeMap::new(),
            zk_receipts: Vec::new(),
            next_whitelist_id: 1,
            next_oracle_id: 1,
            processed: BTreeSet::new(),
            last_height: None,
        }
    }

    pub fn activation(&self, ty: u8) -> Option<Activation> {
        if ty == ZK_TX_TYPE {
            return self.zk_activation;
        }
        self.activations.get(usize::from(ty)).copied().flatten()
    }

    pub(crate) fn protocol_admin_unverified(&self) -> &AccountId {
        &self.protocol_admin
    }

    pub fn whitelist(&self, id: u32) -> Option<&Whitelist> {
        self.whitelists.get(&id)
    }

    pub fn oracle(&self, id: u32) -> Option<&Oracle> {
        self.oracles.get(&id)
    }

    pub(crate) fn oracle_rows(&self) -> impl Iterator<Item = (u32, &Oracle)> {
        self.oracles.iter().map(|(id, oracle)| (*id, oracle))
    }

    pub(crate) fn whitelist_rows(&self) -> impl Iterator<Item = (u32, &Whitelist)> {
        self.whitelists.iter().map(|(id, list)| (*id, list))
    }

    pub(crate) fn attestation_rows(&self) -> impl Iterator<Item = (u32, &AccountId, &Attestation)> {
        self.attestations
            .iter()
            .map(|((id, owner), value)| (*id, owner, value))
    }

    pub fn attestation(&self, list_id: u32, account: &AccountId) -> Option<&Attestation> {
        self.attestations.get(&(list_id, account.clone()))
    }

    pub fn whitelist_count(&self) -> usize {
        self.whitelists.len()
    }

    pub fn oracle_count(&self) -> usize {
        self.oracles.len()
    }

    pub fn processed_count(&self) -> usize {
        self.processed.len()
    }

    /// Read-only replay markers for Stage 22's global input-ID invariant.
    pub fn input_txids_unverified(&self) -> &BTreeSet<TxId> {
        &self.processed
    }

    pub fn last_height(&self) -> Option<u64> {
        self.last_height
    }

    /// A single transaction is still staged and committed atomically.
    pub fn apply(&mut self, tx: RegistryTx) -> Result<(), RegistryError> {
        self.apply_batch([tx])
    }

    /// Executes transactions in the caller-provided order. Never sort them:
    /// order and actor must first be established by an authenticated adapter.
    pub fn apply_batch(
        &mut self,
        transactions: impl IntoIterator<Item = RegistryTx>,
    ) -> Result<(), RegistryError> {
        let mut candidate = self.clone();
        let mut count = 0usize;
        for tx in transactions {
            count += 1;
            if count > MAX_REGISTRY_BATCH {
                return Err(RegistryError::BatchTooLarge);
            }
            if tx.height == 0 {
                return Err(RegistryError::InvalidHeight);
            }
            if candidate.last_height.is_some_and(|prior| tx.height < prior) {
                return Err(RegistryError::HeightRegression);
            }
            if !candidate.processed.insert(tx.txid) {
                return Err(RegistryError::Replay(tx.txid));
            }
            candidate.execute(&tx.actor, tx.height, tx.action)?;
            candidate.last_height = Some(tx.height);
        }
        if count == 0 {
            return Err(RegistryError::EmptyBatch);
        }
        *self = candidate;
        Ok(())
    }

    fn check_active(&self, tx_type: u8, height: u64) -> Result<(), RegistryError> {
        let activation = self
            .activation(tx_type)
            .ok_or(RegistryError::NotActive(tx_type))?;
        if height <= activation.height {
            return Err(RegistryError::SameBlockActivation(tx_type));
        }
        Ok(())
    }

    fn require_nonempty(raw: &str, field: &'static str, max: usize) -> Result<(), RegistryError> {
        if raw.is_empty()
            || raw.len() > max
            || !raw.is_ascii()
            || raw.bytes().any(|b| !(0x20..=0x7e).contains(&b))
        {
            return Err(RegistryError::InvalidField(field));
        }
        Ok(())
    }

    fn require_bounded(raw: &str, field: &'static str, max: usize) -> Result<(), RegistryError> {
        if raw.len() > max || !raw.is_ascii() || raw.bytes().any(|b| !(0x20..=0x7e).contains(&b)) {
            return Err(RegistryError::InvalidField(field));
        }
        Ok(())
    }

    fn execute(
        &mut self,
        actor: &AccountId,
        height: u64,
        action: RegistryAction,
    ) -> Result<(), RegistryError> {
        let ty = action.tx_type();
        if ty == ZK_TX_TYPE && self.zk_fee.is_none() {
            return Err(RegistryError::ZkVerifyDisabled);
        }
        if ty != 0 {
            self.check_active(ty, height)?;
        }
        match action {
            RegistryAction::Activate { types, code_hash } => {
                if actor != &self.protocol_admin {
                    return Err(RegistryError::Unauthorized);
                }
                if types.is_empty() {
                    return Err(RegistryError::EmptyActivation);
                }
                let mut selected = BTreeSet::new();
                for ty in &types {
                    if *ty == ZK_TX_TYPE && self.zk_fee.is_some() {
                        // ZkVerifyV1 opts type 36 into Activate; without the policy it stays
                        // InvalidType below, exactly as before.
                    } else if *ty > MAX_ACTIVATION_TYPE {
                        return Err(RegistryError::InvalidType(*ty));
                    }
                    if *ty == 34 {
                        return Err(RegistryError::ConflictingType34);
                    }
                    if !selected.insert(*ty) {
                        return Err(RegistryError::DuplicateActivation(*ty));
                    }
                    if self.activation(*ty).is_some() {
                        return Err(RegistryError::AlreadyActivated(*ty));
                    }
                }
                for ty in selected {
                    if ty == ZK_TX_TYPE {
                        self.zk_activation = Some(Activation { height, code_hash });
                        continue;
                    }
                    self.activations[usize::from(ty)] = Some(Activation { height, code_hash });
                }
            }
            RegistryAction::CreateWhitelist {
                name,
                url,
                description,
                backup,
            } => {
                Self::require_nonempty(&name, "whitelist name", MAX_NAME_BYTES)?;
                Self::require_bounded(&url, "whitelist url", MAX_URL_BYTES)?;
                Self::require_bounded(&description, "whitelist description", MAX_METADATA_BYTES)?;
                let id = self.next_whitelist_id;
                self.next_whitelist_id = id.checked_add(1).ok_or(RegistryError::IdExhausted)?;
                self.whitelists.insert(
                    id,
                    Whitelist {
                        admin: actor.clone(),
                        backup,
                        name,
                        url,
                        description,
                    },
                );
            }
            RegistryAction::UpdateAdmin {
                target,
                new_address,
                update_backup,
            } => match target {
                AdminTarget::Whitelist(id) => {
                    let list = self
                        .whitelists
                        .get_mut(&id)
                        .ok_or(RegistryError::UnknownWhitelist(id))?;
                    if actor != &list.admin && actor != &list.backup {
                        return Err(RegistryError::Unauthorized);
                    }
                    if update_backup {
                        list.backup = new_address;
                    } else {
                        list.admin = new_address;
                    }
                }
                AdminTarget::Oracle(id) => {
                    let oracle = self
                        .oracles
                        .get_mut(&id)
                        .ok_or(RegistryError::UnknownOracle(id))?;
                    if actor != &oracle.admin && actor != &oracle.backup {
                        return Err(RegistryError::Unauthorized);
                    }
                    if update_backup {
                        oracle.backup = new_address;
                    } else {
                        oracle.admin = new_address;
                    }
                }
                AdminTarget::Token(_) => return Err(RegistryError::UnsupportedTokenAdmin),
            },
            RegistryAction::Attest {
                list_id,
                subject,
                metadata,
            } => {
                if list_id == 0 {
                    return Err(RegistryError::UnsupportedSelfCertification);
                }
                Self::require_bounded(&metadata, "attestation metadata", MAX_METADATA_BYTES)?;
                let list = self
                    .whitelists
                    .get(&list_id)
                    .ok_or(RegistryError::UnknownWhitelist(list_id))?;
                if actor != &list.admin {
                    return Err(RegistryError::Unauthorized);
                }
                let key = (list_id, subject);
                if self.attestations.get(&key).is_some_and(|old| old.active) {
                    return Err(RegistryError::ExistingAttestation);
                }
                self.attestations.insert(
                    key,
                    Attestation {
                        metadata,
                        issued_height: height,
                        updated_height: height,
                        active: true,
                    },
                );
            }
            RegistryAction::RevokeAttestation { list_id, subject } => {
                if list_id == 0 {
                    return Err(RegistryError::UnsupportedSelfCertification);
                }
                let list = self
                    .whitelists
                    .get(&list_id)
                    .ok_or(RegistryError::UnknownWhitelist(list_id))?;
                if actor != &list.admin {
                    return Err(RegistryError::Unauthorized);
                }
                let attestation = self
                    .attestations
                    .get_mut(&(list_id, subject))
                    .ok_or(RegistryError::MissingAttestation)?;
                if !attestation.active {
                    return Err(RegistryError::MissingAttestation);
                }
                attestation.active = false;
                attestation.updated_height = height;
            }
            RegistryAction::CreateOracle {
                ticker,
                url,
                backup,
                lag,
            } => {
                Self::require_nonempty(&ticker, "oracle ticker", MAX_NAME_BYTES)?;
                Self::require_bounded(&url, "oracle url", MAX_URL_BYTES)?;
                if lag == 0 {
                    return Err(RegistryError::InvalidField("oracle lag"));
                }
                let id = self.next_oracle_id;
                self.next_oracle_id = id.checked_add(1).ok_or(RegistryError::IdExhausted)?;
                self.oracles.insert(
                    id,
                    Oracle {
                        admin: actor.clone(),
                        backup,
                        ticker,
                        url,
                        lag,
                        closed: false,
                        latest: None,
                    },
                );
            }
            RegistryAction::PublishOracle {
                oracle_id,
                price_1e4,
                high_1e4,
                low_1e4,
                close_1e4,
            } => {
                let oracle = self
                    .oracles
                    .get_mut(&oracle_id)
                    .ok_or(RegistryError::UnknownOracle(oracle_id))?;
                if actor != &oracle.admin {
                    return Err(RegistryError::Unauthorized);
                }
                if oracle.closed {
                    return Err(RegistryError::ClosedOracle(oracle_id));
                }
                if price_1e4 == 0
                    || [high_1e4, low_1e4, close_1e4]
                        .iter()
                        .flatten()
                        .any(|v| *v == 0)
                {
                    return Err(RegistryError::InvalidPrice);
                }
                if let (Some(high), Some(low)) = (high_1e4, low_1e4) {
                    if high < low {
                        return Err(RegistryError::InvalidPrice);
                    }
                }
                oracle.latest = Some(OracleQuote {
                    price_1e4,
                    high_1e4,
                    low_1e4,
                    close_1e4,
                    height,
                });
            }
            RegistryAction::CloseOracle { oracle_id } => {
                let oracle = self
                    .oracles
                    .get_mut(&oracle_id)
                    .ok_or(RegistryError::UnknownOracle(oracle_id))?;
                if actor != &oracle.admin {
                    return Err(RegistryError::Unauthorized);
                }
                if oracle.closed {
                    return Err(RegistryError::ClosedOracle(oracle_id));
                }
                oracle.closed = true;
            }
            RegistryAction::RegisterZkProgram {
                system,
                program_id,
                key_bytes,
                public_input_layout,
                effect,
                verifier_version,
                max_verify_ms,
            } => {
                if actor != &self.protocol_admin {
                    return Err(RegistryError::Unauthorized);
                }
                self.register_zk_program(ZkProgram {
                    system,
                    program_id,
                    key_bytes,
                    public_input_layout,
                    effect,
                    verifier_version,
                    max_verify_ms,
                })?;
            }
        }
        Ok(())
    }
}

// Checkpoint implementation lives in this module to retain private-field access.
include!("registry_snapshot_impl.rs");
