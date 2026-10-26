//! Opt-in source-quote mechanism over independently pinned pre-funded custody.
//! This supplies no genesis amounts, mint authority, source schedule or L1 proof.
use crate::chain::{BlockId, Tip};
use crate::decimal::{Decimal, Rounding};
use crate::ledger::{AccountId, PropertyId, PropertyKind, TxId};
use crate::network_profile::TradeLayerMainnet;
use core::str::FromStr;
use std::collections::BTreeSet;
pub const ATOMS_PER_TOKEN: u128 = 100_000_000;
pub const MAX_NATIVE_MARKERS: usize = 4096;
pub const MAX_NATIVE_EVENTS: usize = 4096;
pub const MAX_NATIVE_ROWS: usize = 16_384;
pub const MAX_NATIVE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_NATIVE_POLICY_BYTES: usize = 512 * 1024;
pub const MAX_NATIVE_HEIGHT: u64 = 9_007_199_254_740_991;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeError {
    Capacity,
    InvalidPolicy,
    WrongPolicy,
    WrongBootstrap,
    WrongOwner,
    InvalidInput,
    Arithmetic,
    UnsupportedNumericDomain,
    UnrepresentableQuote,
    InsufficientMarker,
    InsufficientFunding,
    Replay,
    NonCanonical,
    Truncated,
    Unauthenticated,
    UnavailableSourceSchedule,
}
impl core::fmt::Display for NativeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for NativeError {}
impl From<crate::decimal::DecimalError> for NativeError {
    fn from(_: crate::decimal::DecimalError) -> Self {
        Self::Arithmetic
    }
}
pub type NativeResult<T> = Result<T, NativeError>;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NativeRole {
    Base,
    VestingBase,
    VestingIncome,
    Income,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceCatalogClass {
    Fixed,
    Vesting,
    Native,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeQuotePolicy {
    SourceQuoteExactOrRejectV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeDriver {
    SourcePerCallV1,
    MonotonicDeltaV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativePayoutOrder {
    AsciiAccountV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeCoverage {
    PerRequestOnlyV1,
    CoverInitialMarkersV1,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeAssetSpec {
    pub role: NativeRole,
    pub property: PropertyId,
    pub issuer: AccountId,
    pub ledger_kind: PropertyKind,
    pub catalog_class: SourceCatalogClass,
    pub declared_figure_atoms: u128,
    pub issuance_ceiling_atoms: Option<u128>,
    pub expected_initial_issued_atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMarkerRow {
    pub owner: AccountId,
    pub atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefundedTargetSpec {
    pub funding_owner: AccountId,
    pub target_atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBindingSpec {
    pub coupon: PropertyId,
    pub target: PropertyId,
    pub curve_scale_atoms: u128,
    pub initial_markers: Vec<NativeMarkerRow>,
    pub funding: PrefundedTargetSpec,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEntitlementPolicy {
    pub network: TradeLayerMainnet,
    pub bootstrap: Tip,
    pub bootstrap_ledger_commitment: [u8; 32],
    pub target_operator: AccountId,
    pub assets: Vec<NativeAssetSpec>,
    pub bindings: Vec<NativeBindingSpec>,
    pub quote_policy: NativeQuotePolicy,
    pub driver: NativeDriver,
    pub payout_order: NativePayoutOrder,
    pub coverage: NativeCoverage,
}
impl NativeEntitlementPolicy {
    pub fn asset(&self, id: PropertyId) -> Option<&NativeAssetSpec> {
        self.assets.iter().find(|a| a.property == id)
    }
    pub fn role(&self, role: NativeRole) -> Option<&NativeAssetSpec> {
        self.assets.iter().find(|a| a.role == role)
    }
    pub fn binding(&self, id: PropertyId) -> Option<&NativeBindingSpec> {
        self.bindings.iter().find(|b| b.coupon == id)
    }
    pub fn is_coupon(&self, id: PropertyId) -> bool {
        self.binding(id).is_some()
    }
    pub const fn require_authenticated(&self) -> NativeResult<()> {
        Err(NativeError::Unauthenticated)
    }
    pub const fn require_source_schedule(&self) -> NativeResult<()> {
        Err(NativeError::UnavailableSourceSchedule)
    }
    pub fn validate(&self) -> NativeResult<()> {
        if self.assets.len() != 4
            || self.bindings.len() != 2
            || self.bootstrap.height > MAX_NATIVE_HEIGHT
        {
            return Err(NativeError::InvalidPolicy);
        }
        self.network
            .check_genesis_if_applicable(self.bootstrap)
            .map_err(|_| NativeError::InvalidPolicy)?;
        normal(&self.target_operator)?;
        let mut properties = BTreeSet::new();
        let roles = [
            NativeRole::Base,
            NativeRole::VestingBase,
            NativeRole::VestingIncome,
            NativeRole::Income,
        ];
        for (a, role) in self.assets.iter().zip(roles) {
            normal(&a.issuer)?;
            if a.role != role
                || !properties.insert(a.property)
                || a.issuance_ceiling_atoms
                    .is_some_and(|v| a.expected_initial_issued_atoms > v)
            {
                return Err(NativeError::InvalidPolicy);
            }
            let expected = match role {
                NativeRole::Base => SourceCatalogClass::Fixed,
                NativeRole::Income => SourceCatalogClass::Native,
                _ => SourceCatalogClass::Vesting,
            };
            if a.catalog_class != expected {
                return Err(NativeError::InvalidPolicy);
            }
        }
        let mut count = 0usize;
        let mut previous = None;
        for b in &self.bindings {
            if previous.is_some_and(|p| p >= b.coupon) || b.curve_scale_atoms == 0 {
                return Err(NativeError::InvalidPolicy);
            }
            previous = Some(b.coupon);
            normal(&b.funding.funding_owner)?;
            let coupon = self.asset(b.coupon).ok_or(NativeError::InvalidPolicy)?;
            let target = self.asset(b.target).ok_or(NativeError::InvalidPolicy)?;
            if !matches!(
                (coupon.role, target.role),
                (NativeRole::VestingBase, NativeRole::Base)
                    | (NativeRole::VestingIncome, NativeRole::Income)
            ) {
                return Err(NativeError::InvalidPolicy);
            }
            source_tokens(b.curve_scale_atoms)?;
            count = count
                .checked_add(b.initial_markers.len())
                .ok_or(NativeError::Capacity)?;
            if count > MAX_NATIVE_MARKERS {
                return Err(NativeError::Capacity);
            }
            let mut prior = None;
            let mut total = 0u128;
            for row in &b.initial_markers {
                normal(&row.owner)?;
                if row.atoms == 0 || prior.is_some_and(|p: &AccountId| p >= &row.owner) {
                    return Err(NativeError::InvalidPolicy);
                }
                prior = Some(&row.owner);
                total = total
                    .checked_add(row.atoms)
                    .ok_or(NativeError::Arithmetic)?;
            }
            if self.coverage == NativeCoverage::CoverInitialMarkersV1
                && b.funding.target_atoms < total
            {
                return Err(NativeError::InsufficientFunding);
            }
        }
        Ok(())
    }
}
pub fn normal(a: &AccountId) -> NativeResult<()> {
    if a.as_str().starts_with('~') {
        Err(NativeError::WrongOwner)
    } else {
        Ok(())
    }
}
pub fn token_decimal(atoms: u128) -> NativeResult<Decimal> {
    Decimal::from(atoms)
        .checked_div(&Decimal::from(ATOMS_PER_TOKEN), 8, Rounding::Down)
        .map_err(Into::into)
}
pub fn exact_atoms(value: &Decimal) -> NativeResult<u128> {
    if value.is_negative() || value.decimal_places() > 8 {
        return Err(NativeError::UnrepresentableQuote);
    }
    value
        .checked_mul(&Decimal::from(ATOMS_PER_TOKEN))?
        .to_string()
        .parse()
        .map_err(|_| NativeError::Arithmetic)
}
pub fn source_number(value: &Decimal) -> NativeResult<(Decimal, u64)> {
    let n = value
        .to_string()
        .parse::<f64>()
        .map_err(|_| NativeError::UnsupportedNumericDomain)?;
    if !n.is_finite() {
        return Err(NativeError::UnsupportedNumericDomain);
    }
    let text = crate::state_oracle::source_number_text(n)
        .map_err(|_| NativeError::UnsupportedNumericDomain)?;
    let out = Decimal::from_str(&text).map_err(|_| NativeError::UnsupportedNumericDomain)?;
    Ok((out, n.to_bits()))
}
pub fn source_tokens(atoms: u128) -> NativeResult<Decimal> {
    let exact = token_decimal(atoms)?;
    let (back, _) = source_number(&exact)?;
    if back != exact {
        return Err(NativeError::UnsupportedNumericDomain);
    }
    Ok(exact)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceVestingQuote {
    pub weight: Decimal,
    pub unbridged: Decimal,
    pub source_number_bits: u64,
    pub atoms: u128,
}
pub fn source_vesting_quote(
    request_atoms: u128,
    coupon_available_atoms: u128,
    curve_scale_atoms: u128,
) -> NativeResult<SourceVestingQuote> {
    if curve_scale_atoms == 0 {
        return Err(NativeError::InvalidPolicy);
    }
    let request = source_tokens(request_atoms)?;
    let available = source_tokens(coupon_available_atoms)?;
    let scale = source_tokens(curve_scale_atoms)?;
    let weight = available.checked_div(&scale, 20, Rounding::HalfUp)?;
    let unbridged = request.checked_mul(&weight)?;
    let (bridged, bits) = source_number(&unbridged)?;
    let atoms = exact_atoms(&bridged)?;
    Ok(SourceVestingQuote {
        weight,
        unbridged,
        source_number_bits: bits,
        atoms,
    })
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMarkerDrag {
    pub proportion: Decimal,
    pub whole_marker_tokens: Decimal,
    pub atoms: u128,
}
pub fn source_marker_drag(
    amount_atoms: u128,
    available_atoms: u128,
    remaining_atoms: u128,
) -> NativeResult<SourceMarkerDrag> {
    let amount = source_tokens(amount_atoms)?;
    let available = source_tokens(available_atoms)?;
    let remaining = source_tokens(remaining_atoms)?;
    if amount.quantize(0, Rounding::Down)?.is_zero()
        || available.is_zero()
        || amount_atoms > available_atoms
    {
        return Err(NativeError::InvalidInput);
    }
    let proportion = amount.checked_div(&available, 20, Rounding::HalfUp)?;
    let whole_marker_tokens = remaining
        .checked_mul(&proportion)?
        .quantize(0, Rounding::Down)?;
    let (bridged, _) = source_number(&whole_marker_tokens)?;
    if bridged != whole_marker_tokens {
        return Err(NativeError::UnsupportedNumericDomain);
    }
    let atoms = exact_atoms(&bridged)?;
    if atoms > remaining_atoms {
        return Err(NativeError::InsufficientMarker);
    }
    Ok(SourceMarkerDrag {
        proportion,
        whole_marker_tokens,
        atoms,
    })
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedNativeTarget {
    pub txid: TxId,
    pub claimed_actor: AccountId,
    pub coupon: PropertyId,
    pub requested_target_atoms: u128,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeTargetOrigin {
    ExplicitUnverified,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeEventContext {
    pub network: TradeLayerMainnet,
    pub height: u64,
}
pub fn bootstrap_commitment(bytes: &[u8]) -> [u8; 32] {
    crate::hash256::double_sha256_parts(&[b"TradeLayer/NativeBootstrapLedger/v1/", bytes])
}
pub fn policy_commitment(p: &NativeEntitlementPolicy) -> NativeResult<[u8; 32]> {
    Ok(crate::hash256::double_sha256_parts(&[
        &canonical_policy_bytes(p)?,
    ]))
}
pub fn adoption_id(p: &NativeEntitlementPolicy) -> NativeResult<TxId> {
    Ok(TxId::from_bytes(crate::hash256::double_sha256_parts(&[
        b"TradeLayer/NativeAdoption/v1/",
        &policy_commitment(p)?,
    ])))
}
pub fn escrow(p: &NativeEntitlementPolicy, coupon: PropertyId) -> NativeResult<AccountId> {
    if p.binding(coupon).is_none() {
        return Err(NativeError::WrongPolicy);
    }
    let hash = policy_commitment(p)?;
    let text = hash.iter().map(|b| format!("{b:02x}")).collect::<String>();
    AccountId::try_from(format!("~native:{text}:{}", coupon.get()).as_str())
        .map_err(|_| NativeError::InvalidPolicy)
}
pub fn physical_leg_id(
    policy: [u8; 32],
    event: TxId,
    tag: u8,
    property: PropertyId,
    owner: &AccountId,
    index: u32,
) -> TxId {
    let mut data = b"TradeLayer/NativePhysicalLeg/v1/".to_vec();
    data.extend_from_slice(&policy);
    data.extend_from_slice(&event.as_bytes());
    data.push(tag);
    data.extend_from_slice(&property.get().to_be_bytes());
    data.push(owner.as_str().len() as u8);
    data.extend_from_slice(owner.as_str().as_bytes());
    data.extend_from_slice(&index.to_be_bytes());
    TxId::from_bytes(crate::hash256::double_sha256_parts(&[&data]))
}

pub(crate) struct NativeWriter(pub Vec<u8>);
impl NativeWriter {
    pub fn put(&mut self, b: &[u8]) -> NativeResult<()> {
        if self
            .0
            .len()
            .checked_add(b.len())
            .is_none_or(|n| n > MAX_NATIVE_BYTES)
        {
            return Err(NativeError::Capacity);
        }
        self.0.extend_from_slice(b);
        Ok(())
    }
    pub fn byte(&mut self, n: u8) -> NativeResult<()> {
        self.put(&[n])
    }
    pub fn u32(&mut self, n: u32) -> NativeResult<()> {
        self.put(&n.to_be_bytes())
    }
    pub fn u64(&mut self, n: u64) -> NativeResult<()> {
        self.put(&n.to_be_bytes())
    }
    pub fn atoms(&mut self, n: u128) -> NativeResult<()> {
        self.put(&n.to_be_bytes())
    }
    pub fn account(&mut self, a: &AccountId) -> NativeResult<()> {
        self.byte(a.as_str().len() as u8)?;
        self.put(a.as_str().as_bytes())
    }
    pub fn count(&mut self, n: usize, max: usize) -> NativeResult<()> {
        if n > max {
            return Err(NativeError::Capacity);
        }
        self.u32(u32::try_from(n).map_err(|_| NativeError::Capacity)?)
    }
    pub fn embedded(&mut self, b: &[u8], max: usize) -> NativeResult<()> {
        self.count(b.len(), max)?;
        self.put(b)
    }
    pub fn decimal(&mut self, d: &Decimal) -> NativeResult<()> {
        let s = d.to_string();
        if s.len() > 256 {
            return Err(NativeError::Capacity);
        }
        self.embedded(s.as_bytes(), 256)
    }
}
pub(crate) struct NativeReader<'a>(pub crate::codec::Cursor<'a>);
impl<'a> NativeReader<'a> {
    pub fn take(&mut self, n: usize) -> NativeResult<&'a [u8]> {
        self.0.take(n).map_err(|_| NativeError::Truncated)
    }
    pub fn byte(&mut self) -> NativeResult<u8> {
        self.0.u8().map_err(|_| NativeError::Truncated)
    }
    pub fn u32(&mut self) -> NativeResult<u32> {
        self.0.u32().map_err(|_| NativeError::Truncated)
    }
    pub fn u64(&mut self) -> NativeResult<u64> {
        self.0.u64().map_err(|_| NativeError::Truncated)
    }
    pub fn atoms(&mut self) -> NativeResult<u128> {
        self.0.u128().map_err(|_| NativeError::Truncated)
    }
    pub fn fixed<const N: usize>(&mut self) -> NativeResult<[u8; N]> {
        self.0.fixed().map_err(|_| NativeError::Truncated)
    }
    pub fn count(&mut self, max: usize) -> NativeResult<usize> {
        let n = self.u32()? as usize;
        if n > max {
            return Err(NativeError::Capacity);
        }
        Ok(n)
    }
    pub fn embedded(&mut self, max: usize) -> NativeResult<&'a [u8]> {
        let n = self.count(max)?;
        self.take(n)
    }
    pub fn account(&mut self) -> NativeResult<AccountId> {
        let n = self.byte()? as usize;
        if n == 0 || n > crate::ledger::MAX_ACCOUNT_BYTES {
            return Err(NativeError::InvalidInput);
        }
        AccountId::try_from(
            core::str::from_utf8(self.take(n)?).map_err(|_| NativeError::InvalidInput)?,
        )
        .map_err(|_| NativeError::InvalidInput)
    }
    pub fn property(&mut self) -> NativeResult<PropertyId> {
        PropertyId::try_from(self.u32()?).map_err(|_| NativeError::InvalidInput)
    }
    pub fn decimal(&mut self) -> NativeResult<Decimal> {
        let b = self.embedded(256)?;
        let s = core::str::from_utf8(b).map_err(|_| NativeError::InvalidInput)?;
        let d = Decimal::from_str(s)?;
        if d.to_string() != s {
            return Err(NativeError::NonCanonical);
        }
        Ok(d)
    }
    pub fn end(self) -> NativeResult<()> {
        self.0.end().map_err(|_| NativeError::NonCanonical)
    }
}
pub fn canonical_policy_bytes(p: &NativeEntitlementPolicy) -> NativeResult<Vec<u8>> {
    p.validate()?;
    let mut w = NativeWriter(b"TradeLayer/NativeEntitlementPolicy/v1/".to_vec());
    w.byte(1)?;
    w.byte(p.network.tag())?;
    w.put(&p.network.genesis().0)?;
    w.u64(p.bootstrap.height)?;
    w.put(&p.bootstrap.id.0)?;
    w.put(&p.bootstrap_ledger_commitment)?;
    w.account(&p.target_operator)?;
    w.byte(1)?;
    w.byte(match p.driver {
        NativeDriver::SourcePerCallV1 => 1,
        NativeDriver::MonotonicDeltaV1 => 2,
    })?;
    w.byte(1)?;
    w.byte(match p.coverage {
        NativeCoverage::PerRequestOnlyV1 => 1,
        NativeCoverage::CoverInitialMarkersV1 => 2,
    })?;
    w.count(p.assets.len(), 4)?;
    for a in &p.assets {
        w.byte(a.role as u8)?;
        w.u32(a.property.get())?;
        w.account(&a.issuer)?;
        w.byte(match a.ledger_kind {
            PropertyKind::Fixed => 1,
            PropertyKind::Managed => 2,
        })?;
        w.byte(match a.catalog_class {
            SourceCatalogClass::Fixed => 1,
            SourceCatalogClass::Vesting => 4,
            SourceCatalogClass::Native => 3,
        })?;
        w.atoms(a.declared_figure_atoms)?;
        w.byte(u8::from(a.issuance_ceiling_atoms.is_some()))?;
        if let Some(n) = a.issuance_ceiling_atoms {
            w.atoms(n)?
        }
        w.atoms(a.expected_initial_issued_atoms)?;
    }
    w.count(p.bindings.len(), 2)?;
    for b in &p.bindings {
        w.u32(b.coupon.get())?;
        w.u32(b.target.get())?;
        w.atoms(b.curve_scale_atoms)?;
        w.account(&b.funding.funding_owner)?;
        w.atoms(b.funding.target_atoms)?;
        w.count(b.initial_markers.len(), MAX_NATIVE_MARKERS)?;
        for row in &b.initial_markers {
            w.account(&row.owner)?;
            w.atoms(row.atoms)?;
        }
    }
    if w.0.len() > MAX_NATIVE_POLICY_BYTES {
        return Err(NativeError::Capacity);
    }
    Ok(w.0)
}
pub fn parse_policy(bytes: &[u8]) -> NativeResult<NativeEntitlementPolicy> {
    if bytes.len() > MAX_NATIVE_POLICY_BYTES {
        return Err(NativeError::Capacity);
    }
    let mut r = NativeReader(crate::codec::Cursor::new(bytes));
    if r.take(b"TradeLayer/NativeEntitlementPolicy/v1/".len())?
        != b"TradeLayer/NativeEntitlementPolicy/v1/"
        || r.byte()? != 1
    {
        return Err(NativeError::InvalidPolicy);
    }
    let network = TradeLayerMainnet::from_tag(r.byte()?).map_err(|_| NativeError::InvalidPolicy)?;
    if r.fixed::<32>()? != network.genesis().0 {
        return Err(NativeError::WrongPolicy);
    }
    let bootstrap = Tip {
        height: r.u64()?,
        id: BlockId(r.fixed()?),
    };
    let bootstrap_ledger_commitment = r.fixed()?;
    let target_operator = r.account()?;
    if r.byte()? != 1 {
        return Err(NativeError::InvalidPolicy);
    }
    let driver = match r.byte()? {
        1 => NativeDriver::SourcePerCallV1,
        2 => NativeDriver::MonotonicDeltaV1,
        _ => return Err(NativeError::InvalidPolicy),
    };
    if r.byte()? != 1 {
        return Err(NativeError::InvalidPolicy);
    }
    let coverage = match r.byte()? {
        1 => NativeCoverage::PerRequestOnlyV1,
        2 => NativeCoverage::CoverInitialMarkersV1,
        _ => return Err(NativeError::InvalidPolicy),
    };
    let n = r.count(4)?;
    let mut assets = Vec::with_capacity(n);
    for _ in 0..n {
        let role = match r.byte()? {
            0 => NativeRole::Base,
            1 => NativeRole::VestingBase,
            2 => NativeRole::VestingIncome,
            3 => NativeRole::Income,
            _ => return Err(NativeError::InvalidPolicy),
        };
        let property = r.property()?;
        let issuer = r.account()?;
        let ledger_kind = match r.byte()? {
            1 => PropertyKind::Fixed,
            2 => PropertyKind::Managed,
            _ => return Err(NativeError::InvalidPolicy),
        };
        let catalog_class = match r.byte()? {
            1 => SourceCatalogClass::Fixed,
            3 => SourceCatalogClass::Native,
            4 => SourceCatalogClass::Vesting,
            _ => return Err(NativeError::InvalidPolicy),
        };
        let declared_figure_atoms = r.atoms()?;
        let issuance_ceiling_atoms = match r.byte()? {
            0 => None,
            1 => Some(r.atoms()?),
            _ => return Err(NativeError::InvalidPolicy),
        };
        let expected_initial_issued_atoms = r.atoms()?;
        assets.push(NativeAssetSpec {
            role,
            property,
            issuer,
            ledger_kind,
            catalog_class,
            declared_figure_atoms,
            issuance_ceiling_atoms,
            expected_initial_issued_atoms,
        });
    }
    let n = r.count(2)?;
    let mut bindings = Vec::with_capacity(n);
    let mut total = 0usize;
    for _ in 0..n {
        let coupon = r.property()?;
        let target = r.property()?;
        let curve_scale_atoms = r.atoms()?;
        let funding = PrefundedTargetSpec {
            funding_owner: r.account()?,
            target_atoms: r.atoms()?,
        };
        let n = r.count(MAX_NATIVE_MARKERS)?;
        total = total.checked_add(n).ok_or(NativeError::Capacity)?;
        if total > MAX_NATIVE_MARKERS {
            return Err(NativeError::Capacity);
        }
        let mut initial_markers = Vec::with_capacity(n);
        for _ in 0..n {
            initial_markers.push(NativeMarkerRow {
                owner: r.account()?,
                atoms: r.atoms()?,
            });
        }
        bindings.push(NativeBindingSpec {
            coupon,
            target,
            curve_scale_atoms,
            initial_markers,
            funding,
        });
    }
    r.end()?;
    let p = NativeEntitlementPolicy {
        network,
        bootstrap,
        bootstrap_ledger_commitment,
        target_operator,
        assets,
        bindings,
        quote_policy: NativeQuotePolicy::SourceQuoteExactOrRejectV1,
        driver,
        payout_order: NativePayoutOrder::AsciiAccountV1,
        coverage,
    };
    if canonical_policy_bytes(&p)?.as_slice() != bytes {
        return Err(NativeError::NonCanonical);
    }
    Ok(p)
}
