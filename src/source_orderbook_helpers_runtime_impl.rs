// Explicit source-only helper entry. Legacy Invoke/Types/replay admission stays exact.
impl SourceContractRuntime {
    pub fn invoke_orderbook_lifecycle_helper(
        &mut self,
        instance: u64,
        entry: crate::contract_queue_source_owner::OrderbookHelperEntryV1,
        call: &SourceCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        if self.policy() != SourceContractRuntimePolicy::MainOrchestrationV12 {
            return Err(SourceRuleError::UnknownDependency);
        }
        if self.core.queue_context.is_some() || self.core.ordered_bootstrap_present() {
            return Err(SourceRuleError::InvalidInput);
        }
        let before = self.clone();
        let mut queue = std::mem::replace(&mut self.queue, ContractQueueSourceOwner::new());
        let core = &mut self.core;
        let result = queue.evaluate_orderbook_helper_instance(
            instance,
            entry,
            call,
            &mut |queue, dependency| {
                // Only a reached real registry dependency borrows the same Core.
                // No supplied financial result or second balance/registry owner.
                core.harvest_queue(queue);
                core.invoke(dependency)
            },
        );
        core.harvest_queue(&mut queue);
        self.queue = queue;
        let result = if self.core.effects_exhausted {
            Err(SourceRuleError::Capacity)
        } else {
            result
        };
        match result {
            Ok(SourceOutcome::Returned(value)) => Ok(DependencyReturn::Pending(value)),
            Ok(SourceOutcome::Thrown(error)) => Ok(DependencyReturn::PendingThrow(error)),
            Err(error) => {
                *self = before;
                Err(error)
            }
        }
    }
}
