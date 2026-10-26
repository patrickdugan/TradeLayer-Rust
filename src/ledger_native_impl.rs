// Included only in the actual Ledger. Audit records never carry spendable balances.
use crate::native_entitlement::{
    self as native, NativeEntitlementPolicy, NativeError, NativeEventContext, NativeReader,
    NativeWriter, UnverifiedNativeTarget,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativePayout {
    pub owner: AccountId,
    pub coupon_available_atoms: u128,
    pub marker_before_atoms: u128,
    pub quote: native::SourceVestingQuote,
    pub leg_id: Option<TxId>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeEvent {
    CoupledSend {
        txid: TxId,
        context: NativeEventContext,
        coupon: PropertyId,
        from: AccountId,
        to: AccountId,
        amount_atoms: u128,
        drag: native::SourceMarkerDrag,
    },
    Target {
        request: UnverifiedNativeTarget,
        context: NativeEventContext,
        origin: native::NativeTargetOrigin,
        consumed_target_atoms: u128,
        total_payout_atoms: u128,
        payouts: Vec<NativePayout>,
    },
    // Heights are deliberately not invented for this existing height-free API.
    OrdinaryTargetTransfer {
        txid: TxId,
        property: PropertyId,
        from: AccountId,
        to: AccountId,
        atoms: u128,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBindingState {
    pub markers: BTreeMap<AccountId, u128>,
    pub released_atoms: u128,
    pub last_observed_target_atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeState {
    policy: NativeEntitlementPolicy,
    // Exactly one ordinary Ledger witness: native/signed metadata cannot nest.
    bootstrap: Vec<u8>,
    events: Vec<NativeEvent>,
    bindings: BTreeMap<PropertyId, NativeBindingState>,
}
impl NativeState {
    pub fn policy(&self) -> &NativeEntitlementPolicy {
        &self.policy
    }
    pub fn events(&self) -> &[NativeEvent] {
        &self.events
    }
    pub fn binding(&self, coupon: PropertyId) -> Option<&NativeBindingState> {
        self.bindings.get(&coupon)
    }
    pub fn bootstrap_bytes(&self) -> &[u8] {
        &self.bootstrap
    }
}
impl From<NativeError> for LedgerError {
    fn from(e: NativeError) -> Self {
        Self::NativeEntitlement(e)
    }
}
fn native_snapshot_error(e: NativeError) -> SnapshotError {
    SnapshotError::InvalidLedger(e.into())
}
fn native_application_snapshot(e: SnapshotError) -> LedgerError {
    match e {
        SnapshotError::InvalidLedger(e) => e,
        _ => NativeError::Capacity.into(),
    }
}
impl Ledger {
    pub fn native_state(&self) -> Option<&NativeState> {
        self.native.as_ref()
    }
    pub fn native_policy(&self) -> Option<&NativeEntitlementPolicy> {
        self.native.as_ref().map(|n| &n.policy)
    }
    pub fn require_pristine_native_adoption_unverified(&self) -> Result<(), LedgerError> {
        let n = self.native.as_ref().ok_or(NativeError::WrongPolicy)?;
        if !n.events.is_empty() || Self::replay_native_state(n)? != *self {
            return Err(NativeError::WrongBootstrap.into());
        }
        Ok(())
    }
    pub fn native_remaining_atoms(&self, coupon: PropertyId, owner: &AccountId) -> u128 {
        self.native
            .as_ref()
            .and_then(|n| n.bindings.get(&coupon))
            .and_then(|b| b.markers.get(owner))
            .copied()
            .unwrap_or(0)
    }
    /// Adopt existing, exact, independently pinned funding. No property or token is issued.
    pub fn adopt_native_entitlements_unverified(
        &mut self,
        policy: &NativeEntitlementPolicy,
    ) -> Result<TxId, LedgerError> {
        policy.validate()?;
        if self.native.is_some() || self.has_signed_channel_revisions() {
            return Err(NativeError::WrongPolicy.into());
        }
        let bootstrap = self.snapshot_bytes().map_err(native_application_snapshot)?;
        if bootstrap.len() > native::MAX_NATIVE_BYTES {
            return Err(NativeError::Capacity.into());
        }
        if native::bootstrap_commitment(&bootstrap) != policy.bootstrap_ledger_commitment {
            return Err(NativeError::WrongBootstrap.into());
        }
        self.require_native_bootstrap(policy)?;
        let mut next = self.clone();
        next.adopt_native_inner(policy, bootstrap)?;
        next.snapshot_bytes().map_err(native_application_snapshot)?;
        *self = next;
        native::adoption_id(policy).map_err(Into::into)
    }
    fn require_native_bootstrap(&self, p: &NativeEntitlementPolicy) -> Result<(), LedgerError> {
        if self.native.is_some()
            || self.has_signed_channel_revisions()
            || self
                .available
                .keys()
                .any(|(_, a)| a.as_str().starts_with("~native:"))
        {
            return Err(NativeError::WrongBootstrap.into());
        }
        // Reserved native assets must be entirely in actual available custody.
        for a in &p.assets {
            let prop = self.existing_property(a.property)?;
            if prop.issuer != a.issuer
                || prop.kind != a.ledger_kind
                || prop.total_supply_atoms != a.expected_initial_issued_atoms
            {
                return Err(NativeError::WrongBootstrap.into());
            }
            if self.channels.values().any(|c| {
                c.escrow_a.contains_key(&a.property) || c.escrow_b.contains_key(&a.property)
            }) || self
                .pending_withdrawals
                .values()
                .any(|w| w.id == a.property)
                || self
                    .pending_net_settlements
                    .values()
                    .any(|w| w.property == a.property)
                || self
                    .swept_net_settlements
                    .values()
                    .any(|w| w.property == a.property)
                || self
                    .king_settlements
                    .values()
                    .any(|w| w.property == a.property)
            {
                return Err(NativeError::WrongBootstrap.into());
            }
        }
        for b in &p.bindings {
            if self.available_atoms(b.target, &b.funding.funding_owner) < b.funding.target_atoms {
                return Err(NativeError::InsufficientFunding.into());
            }
            for ((id, owner), atoms) in &self.available {
                if *id == b.coupon {
                    native::normal(owner)?;
                    native::source_tokens(*atoms)?;
                }
            }
            for row in &b.initial_markers {
                native::source_tokens(row.atoms)?;
            }
        }
        Ok(())
    }
    fn adopt_native_inner(
        &mut self,
        p: &NativeEntitlementPolicy,
        bootstrap: Vec<u8>,
    ) -> Result<(), LedgerError> {
        let id = native::adoption_id(p)?;
        if !self.processed.insert(id) {
            return Err(LedgerError::AlreadyProcessed(id));
        }
        let policy_id = native::policy_commitment(p)?;
        let mut bindings = BTreeMap::new();
        for (index, b) in p.bindings.iter().enumerate() {
            let to = native::escrow(p, b.coupon)?;
            if b.funding.target_atoms != 0 {
                let leg = native::physical_leg_id(policy_id, id, 0, b.target, &to, index as u32);
                if !self.processed.insert(leg) {
                    return Err(LedgerError::AlreadyProcessed(leg));
                }
                self.native_move(
                    b.target,
                    &b.funding.funding_owner,
                    &to,
                    b.funding.target_atoms,
                )?;
            }
            bindings.insert(
                b.coupon,
                NativeBindingState {
                    markers: b
                        .initial_markers
                        .iter()
                        .map(|r| (r.owner.clone(), r.atoms))
                        .collect(),
                    released_atoms: 0,
                    last_observed_target_atoms: 0,
                },
            );
        }
        self.native = Some(NativeState {
            policy: p.clone(),
            bootstrap,
            events: Vec::new(),
            bindings,
        });
        Ok(())
    }
    fn native_move(
        &mut self,
        property: PropertyId,
        from: &AccountId,
        to: &AccountId,
        atoms: u128,
    ) -> Result<(), LedgerError> {
        self.existing_property(property)?;
        let old = self.available_atoms(property, from);
        if old < atoms {
            return Err(LedgerError::InsufficientAvailable {
                property,
                available: old,
                requested: atoms,
            });
        }
        if from == to || atoms == 0 {
            return Ok(());
        }
        let after = self
            .available_atoms(property, to)
            .checked_add(atoms)
            .ok_or(LedgerError::ArithmeticOverflow)?;
        self.put_balance(property, from.clone(), old - atoms);
        self.put_balance(property, to.clone(), after);
        Ok(())
    }
    fn fence_native_operation(&self, op: &Operation) -> Result<(), LedgerError> {
        let escrow = |a: &AccountId| a.as_str().starts_with("~native:");
        let id = match op {
            Operation::CreateProperty { id, issuer, .. } | Operation::Burn { id, issuer, .. } => {
                if escrow(issuer) {
                    return Err(NativeError::WrongOwner.into());
                }
                *id
            }
            Operation::Mint { id, issuer, to, .. } => {
                if escrow(issuer) || escrow(to) {
                    return Err(NativeError::WrongOwner.into());
                }
                *id
            }
            Operation::Transfer { id, from, to, .. } => {
                if escrow(from) || escrow(to) {
                    return Err(NativeError::WrongOwner.into());
                }
                *id
            }
            Operation::ChannelCommit {
                id, owner, channel, ..
            }
            | Operation::ChannelWithdraw {
                id, owner, channel, ..
            } => {
                if escrow(owner) || escrow(channel) {
                    return Err(NativeError::WrongOwner.into());
                }
                *id
            }
            Operation::ChannelTransfer {
                id,
                from_channel,
                to_channel,
                ..
            } => {
                if escrow(from_channel) || escrow(to_channel) {
                    return Err(NativeError::WrongOwner.into());
                }
                *id
            }
            Operation::ChannelNetClaim { id, channel, .. }
            | Operation::ChannelKingSettle { id, channel, .. } => {
                if escrow(channel) {
                    return Err(NativeError::WrongOwner.into());
                }
                *id
            }
        };
        if let Some(n) = &self.native {
            if n.policy.asset(id).is_some()
                && !matches!(op, Operation::Transfer { .. } if !n.policy.is_coupon(id))
            {
                return Err(NativeError::WrongPolicy.into());
            }
        }
        Ok(())
    }
    fn native_transfer_record(
        &self,
        txid: TxId,
        op: &Operation,
    ) -> Result<Option<NativeEvent>, LedgerError> {
        if let (
            Some(n),
            Operation::Transfer {
                id,
                from,
                to,
                amount_atoms,
            },
        ) = (&self.native, op)
        {
            if n.policy.asset(*id).is_some() {
                Self::native_event_capacity(n, 1)?;
                return Ok(Some(NativeEvent::OrdinaryTargetTransfer {
                    txid,
                    property: *id,
                    from: from.clone(),
                    to: to.clone(),
                    atoms: *amount_atoms,
                }));
            }
        }
        Ok(None)
    }
    fn native_event_capacity(n: &NativeState, additional_rows: usize) -> Result<(), LedgerError> {
        if n.events.len() >= native::MAX_NATIVE_EVENTS {
            return Err(NativeError::Capacity.into());
        }
        let rows = n
            .events
            .iter()
            .try_fold(0usize, |n, e| {
                n.checked_add(match e {
                    NativeEvent::Target { payouts, .. } => payouts.len(),
                    _ => 1,
                })
            })
            .and_then(|n| n.checked_add(additional_rows))
            .ok_or(NativeError::Capacity)?;
        if rows > native::MAX_NATIVE_ROWS {
            return Err(NativeError::Capacity.into());
        }
        Ok(())
    }
    fn require_native_context(
        n: &NativeState,
        context: NativeEventContext,
    ) -> Result<(), LedgerError> {
        let last = n.events.iter().rev().find_map(|e| match e {
            NativeEvent::CoupledSend { context, .. } | NativeEvent::Target { context, .. } => {
                Some(context.height)
            }
            _ => None,
        });
        if context.network != n.policy.network
            || context.height <= n.policy.bootstrap.height
            || context.height > native::MAX_NATIVE_HEIGHT
            || last.is_some_and(|h| context.height < h)
        {
            return Err(NativeError::InvalidInput.into());
        }
        Ok(())
    }
    pub fn send_native_coupon_unverified(
        &mut self,
        txid: TxId,
        context: NativeEventContext,
        coupon: PropertyId,
        from: AccountId,
        to: AccountId,
        amount_atoms: u128,
    ) -> Result<native::SourceMarkerDrag, LedgerError> {
        let mut next = self.clone();
        let drag = next.native_send_inner(txid, context, coupon, from, to, amount_atoms)?;
        next.snapshot_bytes().map_err(native_application_snapshot)?;
        *self = next;
        Ok(drag)
    }
    fn native_send_inner(
        &mut self,
        txid: TxId,
        context: NativeEventContext,
        coupon: PropertyId,
        from: AccountId,
        to: AccountId,
        amount_atoms: u128,
    ) -> Result<native::SourceMarkerDrag, LedgerError> {
        native::normal(&from)?;
        native::normal(&to)?;
        let n = self.native.as_ref().ok_or(NativeError::WrongPolicy)?;
        Self::require_native_context(n, context)?;
        Self::native_event_capacity(n, 1)?;
        if !n.policy.is_coupon(coupon) {
            return Err(NativeError::WrongPolicy.into());
        }
        let old_available = self.available_atoms(coupon, &from);
        let old_marker = self.native_remaining_atoms(coupon, &from);
        let drag = native::source_marker_drag(amount_atoms, old_available, old_marker)?;
        let marker_to = self.native_remaining_atoms(coupon, &to);
        // Exact-or-reject domain includes the source's Number storage boundary.
        native::source_tokens(old_available - amount_atoms)?;
        native::source_tokens(old_marker - drag.atoms)?;
        if from != to {
            native::source_tokens(
                self.available_atoms(coupon, &to)
                    .checked_add(amount_atoms)
                    .ok_or(NativeError::Arithmetic)?,
            )?;
            native::source_tokens(old_marker - drag.atoms)?;
            native::source_tokens(
                marker_to
                    .checked_add(drag.atoms)
                    .ok_or(NativeError::Arithmetic)?,
            )?;
        }
        if !self.processed.insert(txid) {
            return Err(LedgerError::AlreadyProcessed(txid));
        }
        self.native_move(coupon, &from, &to, amount_atoms)?;
        let n = self.native.as_mut().expect("policy exists");
        let b = n.bindings.get_mut(&coupon).expect("binding exists");
        if from != to {
            native_marker_put(&mut b.markers, from.clone(), old_marker - drag.atoms);
            native_marker_put(&mut b.markers, to.clone(), marker_to + drag.atoms);
        }
        n.events.push(NativeEvent::CoupledSend {
            txid,
            context,
            coupon,
            from,
            to,
            amount_atoms,
            drag: drag.clone(),
        });
        Ok(drag)
    }
    pub fn apply_native_target_unverified(
        &mut self,
        request: &UnverifiedNativeTarget,
        context: NativeEventContext,
    ) -> Result<NativeEvent, LedgerError> {
        let mut next = self.clone();
        let event = next.native_target_inner(request, context)?;
        next.snapshot_bytes().map_err(native_application_snapshot)?;
        *self = next;
        Ok(event)
    }
    fn native_target_inner(
        &mut self,
        request: &UnverifiedNativeTarget,
        context: NativeEventContext,
    ) -> Result<NativeEvent, LedgerError> {
        native::normal(&request.claimed_actor)?;
        let n = self.native.as_ref().ok_or(NativeError::WrongPolicy)?;
        Self::require_native_context(n, context)?;
        if request.claimed_actor != n.policy.target_operator {
            return Err(NativeError::WrongOwner.into());
        }
        let spec = n
            .policy
            .binding(request.coupon)
            .ok_or(NativeError::WrongPolicy)?;
        let binding = n.bindings.get(&request.coupon).expect("binding exists");
        let consumed = match n.policy.driver {
            native::NativeDriver::SourcePerCallV1 => request.requested_target_atoms,
            native::NativeDriver::MonotonicDeltaV1 => request
                .requested_target_atoms
                .checked_sub(binding.last_observed_target_atoms)
                .ok_or(NativeError::InvalidInput)?,
        };
        native::source_tokens(consumed)?;
        let owners: BTreeSet<_> = self
            .available
            .iter()
            .filter(|((id, _), _)| *id == request.coupon)
            .map(|((_, a), _)| a.clone())
            .chain(binding.markers.keys().cloned())
            .collect();
        Self::native_event_capacity(n, owners.len())?;
        if owners.len() > native::MAX_NATIVE_MARKERS {
            return Err(NativeError::Capacity.into());
        }
        let source = native::escrow(&n.policy, request.coupon)?;
        let policy_id = native::policy_commitment(&n.policy)?;
        let mut payouts = Vec::with_capacity(owners.len());
        let mut total = 0u128;
        let mut ids = BTreeSet::from([request.txid]);
        for (index, owner) in owners.into_iter().enumerate() {
            native::normal(&owner)?;
            let available = self.available_atoms(request.coupon, &owner);
            let marker = binding.markers.get(&owner).copied().unwrap_or(0);
            let quote = native::source_vesting_quote(consumed, available, spec.curve_scale_atoms)?;
            if quote.atoms > marker {
                return Err(NativeError::InsufficientMarker.into());
            }
            native::source_tokens(marker - quote.atoms)?;
            total = total
                .checked_add(quote.atoms)
                .ok_or(NativeError::Arithmetic)?;
            native::source_tokens(
                self.available_atoms(spec.target, &owner)
                    .checked_add(quote.atoms)
                    .ok_or(NativeError::Arithmetic)?,
            )?;
            let leg_id = (quote.atoms != 0).then(|| {
                native::physical_leg_id(
                    policy_id,
                    request.txid,
                    1,
                    spec.target,
                    &owner,
                    index as u32,
                )
            });
            if let Some(id) = leg_id {
                if !ids.insert(id) {
                    return Err(NativeError::Replay.into());
                }
            }
            payouts.push(NativePayout {
                owner,
                coupon_available_atoms: available,
                marker_before_atoms: marker,
                quote,
                leg_id,
            });
        }
        if total > self.available_atoms(spec.target, &source) {
            return Err(NativeError::InsufficientFunding.into());
        }
        if ids.iter().any(|id| self.processed.contains(id)) {
            return Err(NativeError::Replay.into());
        }
        let released = binding
            .released_atoms
            .checked_add(total)
            .ok_or(NativeError::Arithmetic)?;
        let target = spec.target;
        let event = NativeEvent::Target {
            request: request.clone(),
            context,
            origin: native::NativeTargetOrigin::ExplicitUnverified,
            consumed_target_atoms: consumed,
            total_payout_atoms: total,
            payouts,
        };
        // All quotes, receiver overflows and physical IDs have been checked before the first debit.
        self.processed.extend(ids);
        if let NativeEvent::Target { payouts, .. } = &event {
            for payout in payouts {
                self.native_move(target, &source, &payout.owner, payout.quote.atoms)?;
            }
        }
        let n = self.native.as_mut().expect("policy exists");
        let b = n.bindings.get_mut(&request.coupon).expect("binding exists");
        if let NativeEvent::Target { payouts, .. } = &event {
            for payout in payouts {
                native_marker_put(
                    &mut b.markers,
                    payout.owner.clone(),
                    payout.marker_before_atoms - payout.quote.atoms,
                );
            }
        }
        b.released_atoms = released;
        b.last_observed_target_atoms = request.requested_target_atoms;
        n.events.push(event.clone());
        Ok(event)
    }
    fn verify_native_state(&self) -> Result<(), LedgerError> {
        let Some(n) = &self.native else {
            if self
                .available
                .keys()
                .any(|(_, a)| a.as_str().starts_with("~native:"))
            {
                return Err(NativeError::WrongOwner.into());
            }
            return Ok(());
        };
        if !self.channel_revisions.is_empty() {
            return Err(NativeError::WrongPolicy.into());
        }
        let replay = Self::replay_native_state(n)?;
        if replay.native.as_ref() != Some(n) {
            return Err(NativeError::NonCanonical.into());
        }
        for a in &n.policy.assets {
            if replay.properties.get(&a.property) != self.properties.get(&a.property) {
                return Err(NativeError::WrongPolicy.into());
            }
            let actual: BTreeMap<_, _> = self
                .available
                .iter()
                .filter(|((id, _), _)| *id == a.property)
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            let expected: BTreeMap<_, _> = replay
                .available
                .iter()
                .filter(|((id, _), _)| *id == a.property)
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            if actual != expected {
                return Err(NativeError::Replay.into());
            }
            if self.channels.values().any(|c| {
                c.escrow_a.contains_key(&a.property) || c.escrow_b.contains_key(&a.property)
            }) || self
                .pending_withdrawals
                .values()
                .any(|w| w.id == a.property)
                || self
                    .pending_net_settlements
                    .values()
                    .any(|w| w.property == a.property)
                || self
                    .swept_net_settlements
                    .values()
                    .any(|w| w.property == a.property)
                || self
                    .king_settlements
                    .values()
                    .any(|w| w.property == a.property)
            {
                return Err(NativeError::WrongPolicy.into());
            }
        }
        if !replay.processed.is_subset(&self.processed) {
            return Err(NativeError::Replay.into());
        }
        let allowed: BTreeSet<_> = n
            .policy
            .bindings
            .iter()
            .map(|b| native::escrow(&n.policy, b.coupon).map(|a| (b.target, a)))
            .collect::<Result<_, _>>()?;
        if self.available.keys().any(|(id, a)| {
            a.as_str().starts_with("~native:") && !allowed.contains(&(*id, a.clone()))
        }) {
            return Err(NativeError::WrongOwner.into());
        }
        Ok(())
    }
    fn replay_native_state(n: &NativeState) -> Result<Self, LedgerError> {
        n.policy.validate()?;
        if n.bootstrap.len() > native::MAX_NATIVE_BYTES
            || n.events.len() > native::MAX_NATIVE_EVENTS
            || native::bootstrap_commitment(&n.bootstrap) != n.policy.bootstrap_ledger_commitment
        {
            return Err(NativeError::WrongBootstrap.into());
        }
        // Public ordinary decoder rejects TLL4 and TLL5, preventing policy/budget recursion.
        let mut replay =
            Self::from_snapshot_bytes(&n.bootstrap).map_err(native_application_snapshot)?;
        replay.require_native_bootstrap(&n.policy)?;
        replay.adopt_native_inner(&n.policy, n.bootstrap.clone())?;
        let mut rows = 0usize;
        for event in &n.events {
            rows = rows
                .checked_add(match event {
                    NativeEvent::Target { payouts, .. } => payouts.len(),
                    _ => 1,
                })
                .ok_or(NativeError::Capacity)?;
            if rows > native::MAX_NATIVE_ROWS {
                return Err(NativeError::Capacity.into());
            }
            let actual = match event {
                NativeEvent::CoupledSend {
                    txid,
                    context,
                    coupon,
                    from,
                    to,
                    amount_atoms,
                    ..
                } => {
                    replay.native_send_inner(
                        *txid,
                        *context,
                        *coupon,
                        from.clone(),
                        to.clone(),
                        *amount_atoms,
                    )?;
                    replay
                        .native
                        .as_ref()
                        .expect("adopted")
                        .events
                        .last()
                        .expect("inserted")
                        .clone()
                }
                NativeEvent::Target {
                    request, context, ..
                } => replay.native_target_inner(request, *context)?,
                NativeEvent::OrdinaryTargetTransfer {
                    txid,
                    property,
                    from,
                    to,
                    atoms,
                } => {
                    if !replay.processed.insert(*txid) {
                        return Err(NativeError::Replay.into());
                    }
                    replay.execute(
                        *txid,
                        Operation::Transfer {
                            id: *property,
                            from: from.clone(),
                            to: to.clone(),
                            amount_atoms: *atoms,
                        },
                    )?;
                    replay
                        .native
                        .as_ref()
                        .expect("adopted")
                        .events
                        .last()
                        .ok_or(NativeError::Replay)?
                        .clone()
                }
            };
            if &actual != event {
                return Err(NativeError::Replay.into());
            }
        }
        Ok(replay)
    }
    fn native_snapshot_bytes_verified(&self) -> Result<Vec<u8>, SnapshotError> {
        let n = self
            .native
            .as_ref()
            .ok_or_else(|| native_snapshot_error(NativeError::WrongPolicy))?;
        let mut ordinary = self.clone();
        ordinary.native = None;
        let inner = ordinary.ordinary_snapshot_bytes_verified()?;
        let metadata = native_metadata_bytes(n).map_err(native_snapshot_error)?;
        let mut w = NativeWriter(b"TLL\x05".to_vec());
        w.embedded(&inner, crate::codec::MAX_CHECKPOINT_BYTES)
            .map_err(native_snapshot_error)?;
        w.embedded(&metadata, native::MAX_NATIVE_BYTES)
            .map_err(native_snapshot_error)?;
        crate::codec::with_crc(w.0).map_err(snapshot_codec_error)
    }
    pub fn from_snapshot_bytes_with_native_policy(
        input: &[u8],
        expected: &NativeEntitlementPolicy,
    ) -> Result<Self, SnapshotError> {
        expected.validate().map_err(native_snapshot_error)?;
        if input.len() > native::MAX_NATIVE_BYTES + 4 {
            return Err(native_snapshot_error(NativeError::Capacity));
        }
        let payload = crate::codec::verify_crc(input).map_err(snapshot_codec_error)?;
        let mut r = NativeReader(crate::codec::Cursor::new(payload));
        if r.take(4).map_err(native_snapshot_error)? != b"TLL\x05" {
            return Err(native_snapshot_error(NativeError::WrongPolicy));
        }
        let inner = r
            .embedded(crate::codec::MAX_CHECKPOINT_BYTES)
            .map_err(native_snapshot_error)?;
        let bytes = r
            .embedded(native::MAX_NATIVE_BYTES)
            .map_err(native_snapshot_error)?;
        r.end().map_err(native_snapshot_error)?;
        // The ordinary inner contains ~native custody, so structural decoding is
        // intentionally followed by complete native replay, not a public import.
        let mut state = Self::decode_snapshot_with_channel_policies(inner, None)?;
        let n = parse_native_metadata(bytes).map_err(native_snapshot_error)?;
        if &n.policy != expected {
            return Err(native_snapshot_error(NativeError::WrongPolicy));
        }
        state.native = Some(n);
        state
            .verify_conservation()
            .map_err(SnapshotError::InvalidLedger)?;
        if state.snapshot_bytes_verified()?.as_slice() != input {
            return Err(native_snapshot_error(NativeError::NonCanonical));
        }
        Ok(state)
    }
    /// Strict prefix relation used by enclosing actual-header rollback owners.
    pub(crate) fn native_history_is_prefix_of(&self, after: &Self, height: u64) -> bool {
        if !self.processed.is_subset(&after.processed) {
            return false;
        }
        match (&self.native, &after.native) {
            (None, None) => true,
            (Some(a), Some(b))
                if a.policy == b.policy
                    && a.bootstrap == b.bootstrap
                    && b.events.starts_with(&a.events) =>
            {
                b.events[a.events.len()..].iter().all(|e| match e {
                    NativeEvent::CoupledSend { context, txid, .. } => {
                        context.height == height && !self.processed.contains(txid)
                    }
                    NativeEvent::Target {
                        context,
                        request,
                        payouts,
                        ..
                    } => {
                        context.height == height
                            && !self.processed.contains(&request.txid)
                            && payouts
                                .iter()
                                .filter_map(|p| p.leg_id)
                                .all(|id| !self.processed.contains(&id))
                    }
                    NativeEvent::OrdinaryTargetTransfer { txid, .. } => {
                        !self.processed.contains(txid)
                    }
                })
            }
            _ => false,
        }
    }
}
fn native_marker_put(map: &mut BTreeMap<AccountId, u128>, owner: AccountId, atoms: u128) {
    if atoms == 0 {
        map.remove(&owner);
    } else {
        map.insert(owner, atoms);
    }
}
fn native_context_write(w: &mut NativeWriter, c: NativeEventContext) -> native::NativeResult<()> {
    w.byte(c.network.tag())?;
    w.u64(c.height)
}
fn native_context_read(r: &mut NativeReader<'_>) -> native::NativeResult<NativeEventContext> {
    Ok(NativeEventContext {
        network: crate::network_profile::TradeLayerMainnet::from_tag(r.byte()?)
            .map_err(|_| NativeError::InvalidInput)?,
        height: r.u64()?,
    })
}
fn native_metadata_bytes(n: &NativeState) -> native::NativeResult<Vec<u8>> {
    let mut w = NativeWriter(Vec::new());
    w.embedded(
        &native::canonical_policy_bytes(&n.policy)?,
        native::MAX_NATIVE_POLICY_BYTES,
    )?;
    w.embedded(&n.bootstrap, native::MAX_NATIVE_BYTES)?;
    w.count(n.events.len(), native::MAX_NATIVE_EVENTS)?;
    let mut rows = 0usize;
    for e in &n.events {
        rows = rows
            .checked_add(match e {
                NativeEvent::Target { payouts, .. } => payouts.len(),
                _ => 1,
            })
            .ok_or(NativeError::Capacity)?;
        if rows > native::MAX_NATIVE_ROWS {
            return Err(NativeError::Capacity);
        }
        match e {
            NativeEvent::CoupledSend {
                txid,
                context,
                coupon,
                from,
                to,
                amount_atoms,
                drag,
            } => {
                w.byte(1)?;
                w.put(&txid.as_bytes())?;
                native_context_write(&mut w, *context)?;
                w.u32(coupon.get())?;
                w.account(from)?;
                w.account(to)?;
                w.atoms(*amount_atoms)?;
                w.decimal(&drag.proportion)?;
                w.decimal(&drag.whole_marker_tokens)?;
                w.atoms(drag.atoms)?;
            }
            NativeEvent::Target {
                request,
                context,
                origin: native::NativeTargetOrigin::ExplicitUnverified,
                consumed_target_atoms,
                total_payout_atoms,
                payouts,
            } => {
                w.byte(2)?;
                w.put(&request.txid.as_bytes())?;
                w.account(&request.claimed_actor)?;
                w.u32(request.coupon.get())?;
                w.atoms(request.requested_target_atoms)?;
                native_context_write(&mut w, *context)?;
                w.byte(1)?;
                w.atoms(*consumed_target_atoms)?;
                w.atoms(*total_payout_atoms)?;
                w.count(payouts.len(), native::MAX_NATIVE_MARKERS)?;
                for p in payouts {
                    w.account(&p.owner)?;
                    w.atoms(p.coupon_available_atoms)?;
                    w.atoms(p.marker_before_atoms)?;
                    w.decimal(&p.quote.weight)?;
                    w.decimal(&p.quote.unbridged)?;
                    w.u64(p.quote.source_number_bits)?;
                    w.atoms(p.quote.atoms)?;
                    w.byte(u8::from(p.leg_id.is_some()))?;
                    if let Some(id) = p.leg_id {
                        w.put(&id.as_bytes())?;
                    }
                }
            }
            NativeEvent::OrdinaryTargetTransfer {
                txid,
                property,
                from,
                to,
                atoms,
            } => {
                w.byte(3)?;
                w.put(&txid.as_bytes())?;
                w.u32(property.get())?;
                w.account(from)?;
                w.account(to)?;
                w.atoms(*atoms)?;
            }
        }
    }
    let marker_total = n
        .bindings
        .values()
        .try_fold(0usize, |sum, b| sum.checked_add(b.markers.len()))
        .ok_or(NativeError::Capacity)?;
    if marker_total > native::MAX_NATIVE_MARKERS {
        return Err(NativeError::Capacity);
    }
    w.count(n.bindings.len(), 2)?;
    for (id, b) in &n.bindings {
        w.u32(id.get())?;
        w.atoms(b.released_atoms)?;
        w.atoms(b.last_observed_target_atoms)?;
        w.count(b.markers.len(), native::MAX_NATIVE_MARKERS)?;
        for (owner, atoms) in &b.markers {
            w.account(owner)?;
            w.atoms(*atoms)?;
        }
    }
    Ok(w.0)
}
fn parse_native_metadata(bytes: &[u8]) -> native::NativeResult<NativeState> {
    if bytes.len() > native::MAX_NATIVE_BYTES {
        return Err(NativeError::Capacity);
    }
    let mut r = NativeReader(crate::codec::Cursor::new(bytes));
    let policy = native::parse_policy(r.embedded(native::MAX_NATIVE_POLICY_BYTES)?)?;
    // A witness is one ordinary Ledger, never a native or signed-channel tree.
    let bootstrap = r.embedded(native::MAX_NATIVE_BYTES)?.to_vec();
    let count = r.count(native::MAX_NATIVE_EVENTS)?;
    let mut events = Vec::with_capacity(count);
    let mut rows = 0usize;
    for _ in 0..count {
        let tag = r.byte()?;
        let txid = TxId::from_bytes(r.fixed()?);
        let e = match tag {
            1 => {
                rows = rows.checked_add(1).ok_or(NativeError::Capacity)?;
                let context = native_context_read(&mut r)?;
                let coupon = r.property()?;
                let from = r.account()?;
                let to = r.account()?;
                let amount_atoms = r.atoms()?;
                let drag = native::SourceMarkerDrag {
                    proportion: r.decimal()?,
                    whole_marker_tokens: r.decimal()?,
                    atoms: r.atoms()?,
                };
                NativeEvent::CoupledSend {
                    txid,
                    context,
                    coupon,
                    from,
                    to,
                    amount_atoms,
                    drag,
                }
            }
            2 => {
                let request = UnverifiedNativeTarget {
                    txid,
                    claimed_actor: r.account()?,
                    coupon: r.property()?,
                    requested_target_atoms: r.atoms()?,
                };
                let context = native_context_read(&mut r)?;
                if r.byte()? != 1 {
                    return Err(NativeError::InvalidInput);
                }
                let consumed_target_atoms = r.atoms()?;
                let total_payout_atoms = r.atoms()?;
                let count = r.count(native::MAX_NATIVE_MARKERS)?;
                rows = rows.checked_add(count).ok_or(NativeError::Capacity)?;
                if rows > native::MAX_NATIVE_ROWS {
                    return Err(NativeError::Capacity);
                }
                let mut payouts = Vec::with_capacity(count);
                for _ in 0..count {
                    let owner = r.account()?;
                    let coupon_available_atoms = r.atoms()?;
                    let marker_before_atoms = r.atoms()?;
                    let quote = native::SourceVestingQuote {
                        weight: r.decimal()?,
                        unbridged: r.decimal()?,
                        source_number_bits: r.u64()?,
                        atoms: r.atoms()?,
                    };
                    let leg_id = match r.byte()? {
                        0 => None,
                        1 => Some(TxId::from_bytes(r.fixed()?)),
                        _ => return Err(NativeError::InvalidInput),
                    };
                    payouts.push(NativePayout {
                        owner,
                        coupon_available_atoms,
                        marker_before_atoms,
                        quote,
                        leg_id,
                    });
                }
                NativeEvent::Target {
                    request,
                    context,
                    origin: native::NativeTargetOrigin::ExplicitUnverified,
                    consumed_target_atoms,
                    total_payout_atoms,
                    payouts,
                }
            }
            3 => {
                rows = rows.checked_add(1).ok_or(NativeError::Capacity)?;
                NativeEvent::OrdinaryTargetTransfer {
                    txid,
                    property: r.property()?,
                    from: r.account()?,
                    to: r.account()?,
                    atoms: r.atoms()?,
                }
            }
            _ => return Err(NativeError::InvalidInput),
        };
        if rows > native::MAX_NATIVE_ROWS {
            return Err(NativeError::Capacity);
        }
        events.push(e);
    }
    let count = r.count(2)?;
    let mut bindings = BTreeMap::new();
    let mut previous = None;
    let mut marker_count = 0usize;
    for _ in 0..count {
        let id = r.property()?;
        if previous.is_some_and(|p| p >= id) {
            return Err(NativeError::NonCanonical);
        }
        previous = Some(id);
        let released_atoms = r.atoms()?;
        let last_observed_target_atoms = r.atoms()?;
        let count = r.count(native::MAX_NATIVE_MARKERS)?;
        marker_count = marker_count
            .checked_add(count)
            .ok_or(NativeError::Capacity)?;
        if marker_count > native::MAX_NATIVE_MARKERS {
            return Err(NativeError::Capacity);
        }
        let mut markers = BTreeMap::new();
        let mut prior = None;
        for _ in 0..count {
            let owner = r.account()?;
            let atoms = r.atoms()?;
            if atoms == 0 || prior.as_ref().is_some_and(|p| p >= &owner) {
                return Err(NativeError::NonCanonical);
            }
            prior = Some(owner.clone());
            markers.insert(owner, atoms);
        }
        bindings.insert(
            id,
            NativeBindingState {
                markers,
                released_atoms,
                last_observed_target_atoms,
            },
        );
    }
    r.end()?;
    let n = NativeState {
        policy,
        bootstrap,
        events,
        bindings,
    };
    if native_metadata_bytes(&n)?.as_slice() != bytes {
        return Err(NativeError::NonCanonical);
    }
    Ok(n)
}
