// Private custody bridge; the existing SpotExchange retains the only Ledger.
impl SpotExchange {
    pub fn native_policy(&self) -> Option<&crate::native_entitlement::NativeEntitlementPolicy> {
        self.native_policy.as_ref()
    }
    pub fn from_snapshot_bytes_with_native_policy(
        bytes: &[u8],
        permitted: &BTreeSet<PropertyId>,
        policy: &crate::native_entitlement::NativeEntitlementPolicy,
    ) -> Result<Self, SpotSnapshotError> {
        Self::from_snapshot_bytes_mode(bytes, permitted, false, false, Some(policy))
    }
    pub(crate) fn from_combined_spot_snapshot_bytes_with_native_policy_unverified(
        bytes: &[u8],
        permitted: &BTreeSet<PropertyId>,
        clearing: bool,
        policy: &crate::native_entitlement::NativeEntitlementPolicy,
    ) -> Result<Self, SpotSnapshotError> {
        Self::from_snapshot_bytes_mode(bytes, permitted, true, clearing, Some(policy))
    }
    pub(crate) fn send_native_coupon_unverified(
        &mut self,
        txid: TxId,
        context: crate::native_entitlement::NativeEventContext,
        coupon: PropertyId,
        from: AccountId,
        to: AccountId,
        atoms: u128,
    ) -> Result<(), SpotError> {
        if self.native_policy.is_none() {
            return Err(SpotError::NativePolicyRequired);
        }
        if self.seen.contains(&txid) || self.ledger.is_processed(txid) {
            return Err(SpotError::Replay(txid));
        }
        let mut next = self.clone();
        next.ledger
            .send_native_coupon_unverified(txid, context, coupon, from, to, atoms)?;
        next.verify_reserves()?;
        *self = next;
        Ok(())
    }
    pub(crate) fn apply_native_target_unverified(
        &mut self,
        request: &crate::native_entitlement::UnverifiedNativeTarget,
        context: crate::native_entitlement::NativeEventContext,
    ) -> Result<crate::ledger::NativeEvent, SpotError> {
        if self.native_policy.is_none() {
            return Err(SpotError::NativePolicyRequired);
        }
        let mut next = self.clone();
        let event = next
            .ledger
            .apply_native_target_unverified(request, context)?;
        next.verify_reserves()?;
        *self = next;
        Ok(event)
    }
}
