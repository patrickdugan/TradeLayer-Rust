// These callbacks may reserve identities and settle the SAME Kernel's real
// primitive Promise from retained evidence. They never supply a business value.
pub(crate) trait NativeMainOuterFactoryHooksV1 {
    fn register_child(
        &mut self,
        parent: u64,
        method_scope: &'static str,
    ) -> Result<u64, SourceRuleError>;
    fn before_factory(
        &mut self,
        await_: NativeMainOuterBootstrapAwaitV1,
    ) -> Result<(), SourceRuleError>;
    fn after_factory_actual(
        &mut self,
        frame: &NativeMainOuterBootstrapV1,
        await_: NativeMainOuterBootstrapAwaitV1,
    ) -> Result<(), SourceRuleError>;
}
struct NativeMainOuterLegacyFactoryHooksV1<'a, F> {
    register_child: &'a mut F,
}
impl<F: FnMut(u64, &'static str) -> Result<u64, SourceRuleError>> NativeMainOuterFactoryHooksV1
    for NativeMainOuterLegacyFactoryHooksV1<'_, F>
{
    fn register_child(&mut self, parent: u64, scope: &'static str) -> Result<u64, SourceRuleError> {
        (self.register_child)(parent, scope)
    }
    fn before_factory(
        &mut self,
        _: NativeMainOuterBootstrapAwaitV1,
    ) -> Result<(), SourceRuleError> {
        Ok(())
    }
    fn after_factory_actual(
        &mut self,
        _: &NativeMainOuterBootstrapV1,
        _: NativeMainOuterBootstrapAwaitV1,
    ) -> Result<(), SourceRuleError> {
        Ok(())
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_outer_bootstrap_begin_with_hooks(
        &mut self,
        frame: &mut NativeMainOuterBootstrapV1,
        hooks: &mut impl NativeMainOuterFactoryHooksV1,
    ) -> Result<(), SourceRuleError> {
        if frame.started || frame.stopped.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        frame.install(
            NativeMainOuterBootstrapLaneV1::Main,
            NativeMainOuterBootstrapSiteV1::FindMaxFactory,
        )?;
        frame.started = true;
        self.native_main_outer_bootstrap_capture_factory_with_hooks(
            frame,
            "txIndex",
            NativeMainOuterBootstrapSiteV1::FindMaxFactory,
            hooks,
        )
    }
    fn native_main_outer_bootstrap_capture_factory_with_hooks(
        &mut self,
        frame: &mut NativeMainOuterBootstrapV1,
        name: &'static str,
        site: NativeMainOuterBootstrapSiteV1,
        hooks: &mut impl NativeMainOuterFactoryHooksV1,
    ) -> Result<(), SourceRuleError> {
        let lane = if site == NativeMainOuterBootstrapSiteV1::TallyFactory {
            NativeMainOuterBootstrapLaneV1::Tally
        } else {
            NativeMainOuterBootstrapLaneV1::Main
        };
        let await_ = frame
            .pending_site(lane)
            .ok_or(SourceRuleError::InvalidInput)?;
        if await_.site() != site {
            return Err(SourceRuleError::InvalidInput);
        }
        frame.require(await_)?;
        hooks
            .before_factory(await_)
            .map_err(|error| frame.stop(error))?;
        // The result is produced only here, by this actual Core factory.
        let actual = self.core.database(name);
        frame.retain_factory(actual, site)?;
        if self.core.effects_exhausted {
            return Err(frame.stop(SourceRuleError::Capacity));
        }
        hooks
            .after_factory_actual(frame, await_)
            .map_err(|error| frame.stop(error))
    }
}
