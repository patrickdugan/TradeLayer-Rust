//! Funded, atomic integration of IOU calculations with the sole token ledger.
//!
//! This is an offline caller-driven adapter. It does not authenticate matches,
//! losses, actors or chain order. Legacy source allocations are retained, while
//! an explicit atom policy bridges their decimals to the existing token ledger.

use crate::decimal::{Decimal, DecimalError, Rounding};
use crate::hash256::double_sha256;
use crate::iou::{IouError, IouKey, IouStore, MAX_JS_SAFE_BLOCK};
use crate::ledger::{AccountId, Ledger, LedgerError, Operation, PropertyId, TxId};
use crate::network_profile::TradeLayerMainnet;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_FUNDED_IOU_INPUTS: usize = 10_000;
const ESCROW_PREFIX: &str = "~iou:";
const ATOMS_PER_UNIT: u128 = 100_000_000;

/// A caller-selected local accounting policy, not a historical consensus rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayoutAtomPolicy {
    Exact,
    Down,
    HalfUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IouInput {
    pub network: TradeLayerMainnet,
    pub txid: TxId,
    pub height: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchProfit {
    pub key: IouKey,
    pub buyer: AccountId,
    pub seller: AccountId,
    pub buyer_pnl: Decimal,
    pub seller_pnl: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundedAllocation {
    pub address: AccountId,
    pub source_amount: Decimal,
    pub credited_atoms: u128,
    /// Actual atom credit minus the source allocation, retaining its sign.
    pub atom_rounding_delta: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IouPayoutReceipt {
    pub key: IouKey,
    pub input: IouInput,
    pub budget_atoms: u128,
    pub credited_atoms: u128,
    pub allocations: Vec<FundedAllocation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FundedIouError {
    WrongNetwork,
    InvalidHeight,
    HeightRegression,
    InvalidContract,
    ReservedAccount,
    EscrowCollision,
    ZeroAmount,
    NonpositiveMatchProfit,
    FractionalAtom,
    AmountOverflow,
    RoundedPayoutExceedsBudget,
    RoundedPayoutExceedsSourcePool,
    TooManyInputs,
    UncheckpointableState,
    Replay(TxId),
    InsufficientFunding { available: u128, requested: u128 },
    Decimal(DecimalError),
    Iou(IouError),
    Ledger(LedgerError),
}

impl core::fmt::Display for FundedIouError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FundedIouError {}
impl From<DecimalError> for FundedIouError {
    fn from(value: DecimalError) -> Self {
        Self::Decimal(value)
    }
}
impl From<IouError> for FundedIouError {
    fn from(value: IouError) -> Self {
        Self::Iou(value)
    }
}
impl From<LedgerError> for FundedIouError {
    fn from(value: LedgerError) -> Self {
        Self::Ledger(value)
    }
}

/// Owns one ledger. IOU claims/buckets are liabilities and accounting metadata,
/// not additional token balances. Every operation stages and verifies both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundedIouLedger {
    network: TradeLayerMainnet,
    atom_policy: PayoutAtomPolicy,
    ledger: Ledger,
    ious: IouStore,
    escrows: BTreeMap<IouKey, AccountId>,
    inputs: BTreeSet<TxId>,
    last_height: Option<u64>,
}

impl FundedIouLedger {
    /// Network and imported claims are caller assertions. Existing internal
    /// escrows are rejected rather than adopted without their ownership state.
    pub fn from_ledger_unverified(
        network: TradeLayerMainnet,
        atom_policy: PayoutAtomPolicy,
        ledger: Ledger,
        ious: IouStore,
    ) -> Result<Self, FundedIouError> {
        if !ledger.balances_with_account_prefix("~").is_empty() {
            return Err(FundedIouError::EscrowCollision);
        }
        let last_height = ious
            .buckets()
            .map(|row| row.last_block)
            .chain(ious.claim_maps().map(|row| row.last_block))
            .max();
        let state = Self {
            network,
            atom_policy,
            ledger,
            ious,
            escrows: BTreeMap::new(),
            inputs: BTreeSet::new(),
            last_height,
        };
        state.verify()?;
        state.ensure_checkpointable()?;
        Ok(state)
    }

    pub const fn network(&self) -> TradeLayerMainnet {
        self.network
    }
    pub const fn atom_policy(&self) -> PayoutAtomPolicy {
        self.atom_policy
    }
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }
    pub fn ious(&self) -> &IouStore {
        &self.ious
    }
    pub fn processed_count(&self) -> usize {
        self.inputs.len()
    }
    pub fn last_height(&self) -> Option<u64> {
        self.last_height
    }

    pub fn escrow_for_audit(&self, key: IouKey) -> Result<AccountId, FundedIouError> {
        self.property(key)?;
        let mut domain = b"TradeLayer/funded-iou/escrow/v1".to_vec();
        domain.extend_from_slice(&self.network.genesis().0);
        domain.extend_from_slice(&key.contract_id.to_be_bytes());
        domain.extend_from_slice(&key.property_id.to_be_bytes());
        Ok(AccountId::try_from(
            format!(
                "{ESCROW_PREFIX}{}",
                crate::read_only::hex_bytes(&double_sha256(&domain))
            )
            .as_str(),
        )?)
    }

    /// Collect an actual loss into the contract/property escrow and record the
    /// same quantity in the source bucket, or change neither state.
    pub fn collect_loss_unverified(
        &mut self,
        input: IouInput,
        key: IouKey,
        payer: AccountId,
        amount_atoms: u128,
    ) -> Result<(), FundedIouError> {
        self.transaction(input, |staged| {
            check_owner(&payer)?;
            if amount_atoms == 0 {
                return Err(FundedIouError::ZeroAmount);
            }
            let property = staged.property(key)?;
            let escrow = staged.escrow_for_audit(key)?;
            staged.ledger.apply(
                staged.internal_txid(input.txid, key, b"collect", 0)?,
                Operation::Transfer {
                    id: property,
                    from: payer,
                    to: escrow.clone(),
                    amount_atoms,
                },
            )?;
            staged
                .ious
                .add_loss(key, &units(amount_atoms)?, input.height)?;
            staged.escrows.insert(key, escrow);
            Ok(())
        })
    }

    /// Source orderbook's positive net-PnL path: accrue shares first, then
    /// subtract the net profit from the bucket. No token credit occurs here.
    /// Caller must establish that the match qualifies for this source path.
    pub fn accrue_match_profit_unverified(
        &mut self,
        input: IouInput,
        profit: MatchProfit,
    ) -> Result<(), FundedIouError> {
        self.transaction(input, |staged| {
            staged.property(profit.key)?;
            check_owner(&profit.buyer)?;
            check_owner(&profit.seller)?;
            let delta = profit.buyer_pnl.checked_add(&profit.seller_pnl)?;
            if delta <= Decimal::zero() {
                return Err(FundedIouError::NonpositiveMatchProfit);
            }
            staged.ious.add_iou_claims(
                profit.key,
                input.height,
                &profit.buyer,
                &profit.seller,
                &profit.buyer_pnl,
                &profit.seller_pnl,
                &delta,
            )?;
            staged.ious.add_profit(profit.key, &delta, input.height)?;
            Ok(())
        })
    }

    /// Use source payout gates, proportions and claim reductions, then pay
    /// from actual escrow. Source's unchanged block-loss pool is preserved;
    /// it cannot create extra funds, because each credit has a matching debit.
    pub fn pay_unverified(
        &mut self,
        input: IouInput,
        key: IouKey,
        budget_atoms: u128,
    ) -> Result<IouPayoutReceipt, FundedIouError> {
        self.transaction(input, |staged| {
            let property = staged.property(key)?;
            if budget_atoms == 0 {
                return Err(FundedIouError::ZeroAmount);
            }
            let source_pool = staged
                .ious
                .bucket(key)
                .filter(|bucket| bucket.last_block == input.height)
                .map_or_else(Decimal::zero, |bucket| bucket.block_losses.clone());
            let source =
                staged
                    .ious
                    .pay_outstanding_ious(key, &units(budget_atoms)?, input.height)?;
            let mut allocations = Vec::with_capacity(source.len());
            let mut credited_atoms = 0_u128;
            for allocation in source {
                check_owner(&allocation.address)?;
                let atoms = atoms(&allocation.amount, staged.atom_policy)?;
                credited_atoms = credited_atoms
                    .checked_add(atoms)
                    .ok_or(FundedIouError::AmountOverflow)?;
                allocations.push(FundedAllocation {
                    address: allocation.address,
                    atom_rounding_delta: units(atoms)?.checked_sub(&allocation.amount)?,
                    source_amount: allocation.amount,
                    credited_atoms: atoms,
                });
            }
            if credited_atoms > budget_atoms {
                return Err(FundedIouError::RoundedPayoutExceedsBudget);
            }
            if units(credited_atoms)? > source_pool {
                return Err(FundedIouError::RoundedPayoutExceedsSourcePool);
            }
            let escrow = staged.escrow_for_audit(key)?;
            let available = staged.ledger.available_atoms(property, &escrow);
            if credited_atoms > available {
                return Err(FundedIouError::InsufficientFunding {
                    available,
                    requested: credited_atoms,
                });
            }
            for (index, allocation) in allocations.iter().enumerate() {
                if allocation.credited_atoms != 0 {
                    staged.ledger.apply(
                        staged.internal_txid(input.txid, key, b"pay", index)?,
                        Operation::Transfer {
                            id: property,
                            from: escrow.clone(),
                            to: allocation.address.clone(),
                            amount_atoms: allocation.credited_atoms,
                        },
                    )?;
                }
            }
            Ok(IouPayoutReceipt {
                key,
                input,
                budget_atoms,
                credited_atoms,
                allocations,
            })
        })
    }

    fn property(&self, key: IouKey) -> Result<PropertyId, FundedIouError> {
        if key.contract_id == 0 {
            return Err(FundedIouError::InvalidContract);
        }
        let property = PropertyId::try_from(key.property_id)?;
        if self.ledger.property(property).is_none() {
            return Err(LedgerError::UnknownProperty(property).into());
        }
        Ok(property)
    }

    fn internal_txid(
        &self,
        outer: TxId,
        key: IouKey,
        purpose: &[u8],
        leg: usize,
    ) -> Result<TxId, FundedIouError> {
        let mut domain = b"TradeLayer/funded-iou/leg/v1".to_vec();
        domain.extend_from_slice(&self.network.genesis().0);
        domain.extend_from_slice(&outer.as_bytes());
        domain.extend_from_slice(&key.contract_id.to_be_bytes());
        domain.extend_from_slice(&key.property_id.to_be_bytes());
        domain.extend_from_slice(&(purpose.len() as u32).to_be_bytes());
        domain.extend_from_slice(purpose);
        domain.extend_from_slice(
            &u32::try_from(leg)
                .map_err(|_| FundedIouError::AmountOverflow)?
                .to_be_bytes(),
        );
        let id = TxId::from_bytes(double_sha256(&domain));
        if id == outer || self.inputs.contains(&id) {
            return Err(FundedIouError::Replay(id));
        }
        Ok(id)
    }

    fn transaction<T>(
        &mut self,
        input: IouInput,
        execute: impl FnOnce(&mut Self) -> Result<T, FundedIouError>,
    ) -> Result<T, FundedIouError> {
        if input.network != self.network {
            return Err(FundedIouError::WrongNetwork);
        }
        if input.height > MAX_JS_SAFE_BLOCK {
            return Err(FundedIouError::InvalidHeight);
        }
        if self.last_height.is_some_and(|height| input.height < height) {
            return Err(FundedIouError::HeightRegression);
        }
        if self.inputs.contains(&input.txid) || self.ledger.is_processed(input.txid) {
            return Err(FundedIouError::Replay(input.txid));
        }
        if self.inputs.len() >= MAX_FUNDED_IOU_INPUTS {
            return Err(FundedIouError::TooManyInputs);
        }
        let mut staged = self.clone();
        let result = execute(&mut staged)?;
        staged.inputs.insert(input.txid);
        staged.last_height = Some(input.height);
        staged.verify()?;
        staged.ensure_checkpointable()?;
        *self = staged;
        Ok(result)
    }

    fn ensure_checkpointable(&self) -> Result<(), FundedIouError> {
        // Source record-count limits alone do not bound encoded bytes, and
        // the embedded ledger has its own journal limits. An accepted funded
        // operation must remain restorable, including its replay markers.
        self.snapshot_bytes()
            .map(|_| ())
            .map_err(|_| FundedIouError::UncheckpointableState)
    }

    pub fn verify(&self) -> Result<(), FundedIouError> {
        self.ledger.verify_conservation()?;
        let mut owned = BTreeSet::new();
        for (&key, escrow) in &self.escrows {
            let property = self.property(key)?;
            if *escrow != self.escrow_for_audit(key)? || !owned.insert((property, escrow.clone())) {
                return Err(FundedIouError::EscrowCollision);
            }
        }
        for (property, account, _) in self.ledger.balances_with_account_prefix("~") {
            if !owned.contains(&(property, account)) {
                return Err(FundedIouError::EscrowCollision);
            }
        }
        for (key, height) in self
            .ious
            .buckets()
            .map(|row| (row.key, row.last_block))
            .chain(self.ious.claim_maps().map(|row| (row.key, row.last_block)))
        {
            self.property(key)?;
            if height > MAX_JS_SAFE_BLOCK || self.last_height.is_none_or(|last| height > last) {
                return Err(FundedIouError::InvalidHeight);
            }
        }
        Ok(())
    }
}

fn check_owner(owner: &AccountId) -> Result<(), FundedIouError> {
    if owner.as_str().starts_with('~') {
        Err(FundedIouError::ReservedAccount)
    } else {
        Ok(())
    }
}

fn units(atoms: u128) -> Result<Decimal, DecimalError> {
    Decimal::from(atoms).checked_div(&Decimal::from(ATOMS_PER_UNIT), 8, Rounding::Down)
}

fn atoms(value: &Decimal, policy: PayoutAtomPolicy) -> Result<u128, FundedIouError> {
    let scaled = value.checked_mul(&Decimal::from(ATOMS_PER_UNIT))?;
    let whole = match policy {
        PayoutAtomPolicy::Exact => {
            if scaled.decimal_places() != 0 {
                return Err(FundedIouError::FractionalAtom);
            }
            scaled
        }
        PayoutAtomPolicy::Down => scaled.quantize(0, Rounding::Down)?,
        PayoutAtomPolicy::HalfUp => scaled.quantize(0, Rounding::HalfUp)?,
    };
    whole
        .to_string()
        .parse()
        .map_err(|_| FundedIouError::AmountOverflow)
}

include!("iou_ledger_snapshot_impl.rs");
