//! Deterministic, caller-driven vault accounting and synthetic-token links.
//!
//! The arithmetic directions and aggregate queries are derived from pinned
//! `vaults.js`. Source persistence, asynchronous identifier bugs, missing
//! dependencies and incomplete funding/rebase methods are characterized in
//! `docs/VAULTS.md`, not silently interpreted as working settlement. This typed
//! store rejects negative collateral/supply and destructive duplicate creation.
//! No method authenticates a caller, moves ledger balances, values collateral,
//! checks a hedge, or grants transaction/consensus authority.

use crate::decimal::{Decimal, Rounding};
use crate::ledger::PropertyId;
use core::{fmt, str::FromStr};
use std::collections::BTreeMap;

pub const MAX_VAULTS: usize = 65_536;
pub const MAX_SYNTHETIC_TOKENS: usize = 65_536;

/// Canonical source vault spelling: `s-<collateral property>-<hedge contract>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VaultId {
    property_id: PropertyId,
    contract_id: u32,
}

impl VaultId {
    pub fn new(property_id: PropertyId, contract_id: u32) -> Result<Self, VaultError> {
        if contract_id == 0 {
            return Err(VaultError::InvalidIdentifier);
        }
        Ok(Self {
            property_id,
            contract_id,
        })
    }

    pub const fn property_id(self) -> PropertyId {
        self.property_id
    }
    pub const fn contract_id(self) -> u32 {
        self.contract_id
    }
}

impl fmt::Display for VaultId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "s-{}-{}", self.property_id.get(), self.contract_id)
    }
}

fn canonical_u32(text: &str) -> Result<u32, VaultError> {
    if text.is_empty() || text.starts_with('0') || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(VaultError::InvalidIdentifier);
    }
    text.parse().map_err(|_| VaultError::InvalidIdentifier)
}

impl FromStr for VaultId {
    type Err = VaultError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut parts = text.split('-');
        if parts.next() != Some("s") {
            return Err(VaultError::InvalidIdentifier);
        }
        let property = canonical_u32(parts.next().ok_or(VaultError::InvalidIdentifier)?)?;
        let contract = canonical_u32(parts.next().ok_or(VaultError::InvalidIdentifier)?)?;
        if parts.next().is_some() {
            return Err(VaultError::InvalidIdentifier);
        }
        Self::new(
            PropertyId::try_from(property).map_err(|_| VaultError::InvalidIdentifier)?,
            contract,
        )
    }
}

/// Canonical output of source `generateSyntheticTokenId`, whose *two* parts
/// are incompatible with the source's effective three-part async parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SyntheticTokenId(VaultId);

impl SyntheticTokenId {
    pub const fn for_vault(vault_id: VaultId) -> Self {
        Self(vault_id)
    }
    pub const fn vault_id(self) -> VaultId {
        self.0
    }
}

impl fmt::Display for SyntheticTokenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.0.property_id.get(), self.0.contract_id)
    }
}

impl FromStr for SyntheticTokenId {
    type Err = VaultError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut parts = text.split('-');
        let property = canonical_u32(parts.next().ok_or(VaultError::InvalidIdentifier)?)?;
        let contract = canonical_u32(parts.next().ok_or(VaultError::InvalidIdentifier)?)?;
        if parts.next().is_some() {
            return Err(VaultError::InvalidIdentifier);
        }
        Ok(Self(VaultId::new(
            PropertyId::try_from(property).map_err(|_| VaultError::InvalidIdentifier)?,
            contract,
        )?))
    }
}

/// Literal strings returned after awaiting the source's second, overriding
/// `parseSyntheticTokenId`. Empty and nonnumeric parts are accepted there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacySyntheticTokenParts<'a> {
    pub underlying_property_id: &'a str,
    pub hedge_contract_id: &'a str,
    pub vault_id: &'a str,
}

pub fn parse_legacy_synthetic_token_id(
    text: &str,
) -> Result<LegacySyntheticTokenParts<'_>, VaultError> {
    let mut parts = text.split('-');
    let underlying_property_id = parts.next().ok_or(VaultError::InvalidIdentifier)?;
    let hedge_contract_id = parts.next().ok_or(VaultError::InvalidIdentifier)?;
    let vault_id = parts.next().ok_or(VaultError::InvalidIdentifier)?;
    if parts.next().is_some() {
        return Err(VaultError::InvalidIdentifier);
    }
    Ok(LegacySyntheticTokenParts {
        underlying_property_id,
        hedge_contract_id,
        vault_id,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vault {
    pub id: VaultId,
    /// Signed hedge position. A short is negative; collateral and supply below
    /// are nonnegative. This module supplies no contract valuation model.
    pub contracts: Decimal,
    pub margin: Decimal,
    pub available: Decimal,
    pub outstanding: Decimal,
}

impl Vault {
    fn empty(id: VaultId) -> Self {
        Self {
            id,
            contracts: Decimal::from(0_i128),
            margin: Decimal::from(0_i128),
            available: Decimal::from(0_i128),
            outstanding: Decimal::from(0_i128),
        }
    }

    fn validate(&self) -> Result<(), VaultError> {
        if self.margin.is_negative() {
            return Err(VaultError::NegativeMargin);
        }
        if self.available.is_negative() {
            return Err(VaultError::NegativeAvailable);
        }
        if self.outstanding.is_negative() {
            return Err(VaultError::NegativeOutstanding);
        }
        Ok(())
    }

    pub fn collateral(&self) -> Result<CollateralPosition, VaultError> {
        Ok(CollateralPosition {
            property_id: self.id.property_id,
            margin: self.margin.clone(),
            available: self.available.clone(),
            total: add(&self.margin, &self.available)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollateralPosition {
    pub property_id: PropertyId,
    pub margin: Decimal,
    pub available: Decimal,
    pub total: Decimal,
}

/// Signed additions, corresponding to source `updateVault`. Missing vaults
/// are created. Resulting collateral/supply must be nonnegative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultUpdate {
    pub contracts_delta: Decimal,
    pub margin_delta: Decimal,
    pub available_delta: Decimal,
    pub outstanding_delta: Decimal,
}

/// Source redemption adds contracts, subtracts margin/available, and adds a
/// signed outstanding delta. This interface takes a nonnegative redeemed
/// amount and subtracts it, making that caller convention explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultRedemption {
    pub contracts_delta: Decimal,
    pub margin_released: Decimal,
    pub available_released: Decimal,
    pub amount_redeemed: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollateralMovement {
    Reserve,
    Release,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    InvalidIdentifier,
    AlreadyExists(VaultId),
    NotFound(VaultId),
    SyntheticNotFound(SyntheticTokenId),
    SyntheticVaultMismatch,
    CapacityExceeded,
    Arithmetic,
    NegativeAmount,
    NegativeMargin,
    NegativeAvailable,
    NegativeOutstanding,
    NotEmpty(VaultId),
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for VaultError {}

fn add(left: &Decimal, right: &Decimal) -> Result<Decimal, VaultError> {
    left.checked_add(right).map_err(|_| VaultError::Arithmetic)
}
fn sub(left: &Decimal, right: &Decimal) -> Result<Decimal, VaultError> {
    left.checked_sub(right).map_err(|_| VaultError::Arithmetic)
}
fn nonnegative(amount: &Decimal) -> Result<(), VaultError> {
    if amount.is_negative() {
        Err(VaultError::NegativeAmount)
    } else {
        Ok(())
    }
}

/// Isolated vault state. All mutations build and validate replacement values
/// before writing; batch updates clone once and commit only if all succeed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VaultRegistry {
    vaults: BTreeMap<VaultId, Vault>,
    synthetic_tokens: BTreeMap<SyntheticTokenId, VaultId>,
}

impl VaultRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn vault(&self, id: VaultId) -> Option<&Vault> {
        self.vaults.get(&id)
    }
    pub fn vault_count(&self) -> usize {
        self.vaults.len()
    }
    pub fn synthetic_count(&self) -> usize {
        self.synthetic_tokens.len()
    }
    pub fn vaults(&self) -> impl Iterator<Item = &Vault> {
        self.vaults.values()
    }
    pub fn vaults_for_property(&self, property: PropertyId) -> impl Iterator<Item = &Vault> {
        let first = VaultId {
            property_id: property,
            contract_id: 1,
        };
        let last = VaultId {
            property_id: property,
            contract_id: u32::MAX,
        };
        self.vaults.range(first..=last).map(|(_, vault)| vault)
    }

    pub fn create_vault(&mut self, id: VaultId) -> Result<(), VaultError> {
        if self.vaults.contains_key(&id) {
            return Err(VaultError::AlreadyExists(id));
        }
        if self.vaults.len() >= MAX_VAULTS {
            return Err(VaultError::CapacityExceeded);
        }
        self.vaults.insert(id, Vault::empty(id));
        Ok(())
    }

    pub fn update_vault(&mut self, id: VaultId, update: &VaultUpdate) -> Result<(), VaultError> {
        let existing = self.vaults.get(&id);
        if existing.is_none() && self.vaults.len() >= MAX_VAULTS {
            return Err(VaultError::CapacityExceeded);
        }
        let mut next = existing.cloned().unwrap_or_else(|| Vault::empty(id));
        next.contracts = add(&next.contracts, &update.contracts_delta)?;
        next.margin = add(&next.margin, &update.margin_delta)?;
        next.available = add(&next.available, &update.available_delta)?;
        next.outstanding = add(&next.outstanding, &update.outstanding_delta)?;
        next.validate()?;
        self.vaults.insert(id, next);
        Ok(())
    }

    pub fn update_batch(&mut self, updates: &[(VaultId, VaultUpdate)]) -> Result<(), VaultError> {
        if updates.is_empty() {
            return Ok(());
        }
        let mut next = self.clone();
        for (id, update) in updates {
            next.update_vault(*id, update)?;
        }
        *self = next;
        Ok(())
    }

    pub fn redeem(&mut self, id: VaultId, redemption: &VaultRedemption) -> Result<(), VaultError> {
        nonnegative(&redemption.margin_released)?;
        nonnegative(&redemption.available_released)?;
        nonnegative(&redemption.amount_redeemed)?;
        let mut next = self
            .vaults
            .get(&id)
            .cloned()
            .ok_or(VaultError::NotFound(id))?;
        next.contracts = add(&next.contracts, &redemption.contracts_delta)?;
        next.margin = sub(&next.margin, &redemption.margin_released)?;
        next.available = sub(&next.available, &redemption.available_released)?;
        next.outstanding = sub(&next.outstanding, &redemption.amount_redeemed)?;
        next.validate()?;
        self.vaults.insert(id, next);
        Ok(())
    }

    /// Explicit local collateral interface, with exact conservation between
    /// available and margin. It does not debit or credit any external ledger.
    pub fn move_collateral(
        &mut self,
        id: VaultId,
        movement: CollateralMovement,
        amount: &Decimal,
    ) -> Result<(), VaultError> {
        nonnegative(amount)?;
        let mut next = self
            .vaults
            .get(&id)
            .cloned()
            .ok_or(VaultError::NotFound(id))?;
        match movement {
            CollateralMovement::Reserve => {
                next.available = sub(&next.available, amount)?;
                next.margin = add(&next.margin, amount)?;
            }
            CollateralMovement::Release => {
                next.margin = sub(&next.margin, amount)?;
                next.available = add(&next.available, amount)?;
            }
        }
        next.validate()?;
        self.vaults.insert(id, next);
        Ok(())
    }

    /// Explicitly link the canonical synthetic identifier to its vault. The
    /// source stores property/contract but omits the link its getters require.
    /// Re-registering the identical link is idempotent.
    pub fn register_synthetic_token(
        &mut self,
        token: SyntheticTokenId,
        vault_id: VaultId,
    ) -> Result<(), VaultError> {
        if token.vault_id() != vault_id {
            return Err(VaultError::SyntheticVaultMismatch);
        }
        if !self.vaults.contains_key(&vault_id) {
            return Err(VaultError::NotFound(vault_id));
        }
        if !self.synthetic_tokens.contains_key(&token)
            && self.synthetic_tokens.len() >= MAX_SYNTHETIC_TOKENS
        {
            return Err(VaultError::CapacityExceeded);
        }
        self.synthetic_tokens.insert(token, vault_id);
        Ok(())
    }

    pub fn synthetic_vault(&self, token: SyntheticTokenId) -> Result<&Vault, VaultError> {
        let id = self
            .synthetic_tokens
            .get(&token)
            .ok_or(VaultError::SyntheticNotFound(token))?;
        self.vaults.get(id).ok_or(VaultError::NotFound(*id))
    }

    /// Empty vaults can be closed without erasing collateral or issued supply.
    /// This safe typed lifecycle operation has no source equivalent.
    pub fn close_vault(&mut self, id: VaultId) -> Result<(), VaultError> {
        let vault = self.vaults.get(&id).ok_or(VaultError::NotFound(id))?;
        if !vault.contracts.is_zero()
            || !vault.margin.is_zero()
            || !vault.available.is_zero()
            || !vault.outstanding.is_zero()
        {
            return Err(VaultError::NotEmpty(id));
        }
        self.vaults.remove(&id);
        self.synthetic_tokens.retain(|_, vault_id| *vault_id != id);
        Ok(())
    }

    /// Sum unrounded positions first, then apply source BigNumber's default
    /// decimalPlaces(8), ROUND_HALF_UP, once to the aggregate.
    pub fn total_collateral_for_property(
        &self,
        property: PropertyId,
    ) -> Result<Decimal, VaultError> {
        let mut total = Decimal::from(0_i128);
        for vault in self.vaults_for_property(property) {
            total = add(&total, &add(&vault.margin, &vault.available)?)?;
        }
        total
            .quantize(8, Rounding::HalfUp)
            .map_err(|_| VaultError::Arithmetic)
    }

    pub fn total_outstanding_for_property(
        &self,
        property: PropertyId,
    ) -> Result<Decimal, VaultError> {
        let mut total = Decimal::from(0_i128);
        for vault in self.vaults_for_property(property) {
            total = add(&total, &vault.outstanding)?;
        }
        total
            .quantize(8, Rounding::HalfUp)
            .map_err(|_| VaultError::Arithmetic)
    }

    /// The source's direct `s-...` outstanding query yields zero if absent.
    pub fn outstanding_for_vault(&self, id: VaultId) -> Result<Decimal, VaultError> {
        self.vaults.get(&id).map_or_else(
            || Ok(Decimal::from(0_i128)),
            |v| {
                v.outstanding
                    .quantize(8, Rounding::HalfUp)
                    .map_err(|_| VaultError::Arithmetic)
            },
        )
    }
}
