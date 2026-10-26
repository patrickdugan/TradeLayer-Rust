impl JointChainState {
    pub fn new_with_native_policy_unverified(
        network: TradeLayerMainnet,
        anchor: Tip,
        admin: AccountId,
        ledger: Ledger,
        permitted: BTreeSet<PropertyId>,
        history_limit: usize,
        policy: &crate::native_entitlement::NativeEntitlementPolicy,
    ) -> Result<Self, JointChainError> {
        if !(1..=MAX_REORG_HISTORY).contains(&history_limit) {
            return Err(JointChainError::InvalidHistoryLimit);
        }
        if policy.bootstrap != anchor || policy.network != network {
            return Err(JointChainError::WrongNetwork);
        }
        network
            .check_genesis_if_applicable(anchor)
            .map_err(|_| JointChainError::WrongNetwork)?;
        let state = JointProtocolState::from_native_bootstrap_unverified(
            network, admin, ledger, permitted, policy,
        )?;
        let out = Self {
            network,
            anchor,
            tip: anchor,
            state,
            history: VecDeque::new(),
            history_limit,
        };
        out.checkpoint_bytes()?;
        Ok(out)
    }
    pub fn from_checkpoint_bytes_with_native_policy(
        bytes: &[u8],
        network: TradeLayerMainnet,
        anchor: Tip,
        admin: &AccountId,
        permitted: &BTreeSet<PropertyId>,
        history_limit: usize,
        policy: &crate::native_entitlement::NativeEntitlementPolicy,
    ) -> Result<Self, JointCheckpointError> {
        Self::from_checkpoint_bytes_native_mode(
            bytes,
            network,
            anchor,
            admin,
            permitted,
            history_limit,
            Some(policy),
        )
    }
    fn verify_native_history(&self) -> Result<(), JointCheckpointError> {
        let expected = self.state.native_policy();
        if expected.is_some_and(|p| p.bootstrap != self.anchor || p.network != self.network) {
            return Err(bad("native anchor context"));
        }
        for previous in &self.history {
            if previous.state.native_policy() != expected {
                return Err(bad("native policy changed across rollback history"));
            }
        }
        if expected.is_some() {
            for (index, before) in self.history.iter().enumerate() {
                let (after, height) = self
                    .history
                    .get(index + 1)
                    .map_or((&self.state, self.tip.height), |a| (&a.state, a.tip.height));
                if before.tip.height.checked_add(1) != Some(height)
                    || !before
                        .state
                        .ledger()
                        .native_history_is_prefix_of(after.ledger(), height)
                {
                    return Err(bad("native rollback journal transition"));
                }
            }
        }
        Ok(())
    }
}
