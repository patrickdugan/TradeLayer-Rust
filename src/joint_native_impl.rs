// Same Joint owner, independent optional policy, no economic/registry wire alias.
impl JointProtocolState {
    pub fn native_policy(&self) -> Option<&crate::native_entitlement::NativeEntitlementPolicy> {
        self.spot.native_policy()
    }
    pub fn from_native_bootstrap_unverified(
        network: TradeLayerMainnet,
        admin: AccountId,
        ledger: Ledger,
        permitted: BTreeSet<PropertyId>,
        policy: &crate::native_entitlement::NativeEntitlementPolicy,
    ) -> Result<Self, JointError> {
        if policy.network != network
            || policy.bootstrap
                != (crate::chain::Tip {
                    height: 0,
                    id: network.genesis(),
                })
            || !ledger.has_no_channel_or_settlement_liabilities_unverified()
        {
            return Err(JointError::UnsupportedLegacyLiabilities);
        }
        ledger
            .require_pristine_native_adoption_unverified()
            .map_err(SpotError::Ledger)?;
        let spot = SpotExchange::from_native_ledger_unverified(ledger, permitted, policy)?;
        let state = Self {
            network,
            registry: Registry::new(admin),
            spot,
            global_input_ids: BTreeSet::new(),
            ledger_input_ids: BTreeSet::new(),
            economic_input_ids: BTreeSet::new(),
            system_epoch_ids: BTreeSet::new(),
            economic_pending_height: None,
            economic_pending_input: None,
            finalized_height: None,
        };
        state.verify_components_unverified()?;
        Ok(state)
    }
    pub fn from_snapshot_bytes_with_native_policy(
        bytes: &[u8],
        network: TradeLayerMainnet,
        admin: &AccountId,
        permitted: &BTreeSet<PropertyId>,
        policy: &crate::native_entitlement::NativeEntitlementPolicy,
    ) -> Result<Self, JointSnapshotError> {
        if policy.network != network {
            return Err(joint_bad("native policy network"));
        }
        Self::from_snapshot_bytes_native_mode(bytes, network, admin, permitted, Some(policy))
    }
    pub(crate) fn from_economic_snapshot_bytes_with_native_policy(
        bytes: &[u8],
        network: TradeLayerMainnet,
        admin: &AccountId,
        permitted: &BTreeSet<PropertyId>,
        policy: &crate::native_entitlement::NativeEntitlementPolicy,
    ) -> Result<Self, JointSnapshotError> {
        if policy.network != network {
            return Err(joint_bad("native policy network"));
        }
        Self::from_economic_snapshot_bytes_native_mode(
            bytes,
            network,
            admin,
            permitted,
            Some(policy),
        )
    }
    pub(crate) fn apply_native_target_for_economic_unverified(
        &mut self,
        height: u64,
        request: &crate::native_entitlement::UnverifiedNativeTarget,
    ) -> Result<crate::ledger::NativeEvent, JointError> {
        self.check_economic_pending_height(height)?;
        self.check_new_input_unverified(height, request.txid)?;
        let mut next = self.clone();
        let event = next.spot.apply_native_target_unverified(
            request,
            crate::native_entitlement::NativeEventContext {
                network: self.network,
                height,
            },
        )?;
        // The actual new physical legs may not alias any non-Ledger input bucket.
        for id in next
            .ledger()
            .input_txids_unverified()
            .difference(self.ledger().input_txids_unverified())
        {
            if *id != request.txid
                && (self.global_input_ids.contains(id)
                    || self.system_epoch_ids.contains(id)
                    || self.registry.is_processed(*id)
                    || self.spot.spot_input_ids_unverified().contains(id))
            {
                return Err(JointError::Replay(*id));
            }
        }
        next.global_input_ids.insert(request.txid);
        next.ledger_input_ids.insert(request.txid);
        next.economic_pending_height = Some(height);
        // This is a local ordinary funded operation, not a fake type18/system epoch.
        *self = next;
        Ok(event)
    }
}
