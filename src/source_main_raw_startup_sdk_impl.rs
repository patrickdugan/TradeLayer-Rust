// Data-only ClientWrapper boundary over the SAME literal index/Core owners.
struct NativeMainRawStartupSdkBackendV1<'a> {
    core: &'a mut ContractCore,
    index: &'a mut SourceCompactIndex,
    fields: &'a V,
}
impl SourceBackend for NativeMainRawStartupSdkBackendV1<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if call.target == "ClientWrapper.getInstance" {
            self.core.emit(obj(&[
                ("kind", s("rpc-input")),
                ("method", s("Client.getInstance")),
                ("args", V::Array(call.arguments.clone())),
                ("returned", self.fields.clone()),
            ]));
            return Ok(DependencyReturn::Pending(obj(&[("$rpcClient", n(1.))])));
        }
        NativeMainIndexBackend {
            core: self.core,
            index: self.index,
        }
        .invoke(call)
    }
}
impl ContractCore {
    fn source_main_raw_startup_sdk_index(
        &mut self,
        index: &mut SourceCompactIndex,
        fields: &V,
    ) -> R<V> {
        let mut owner = std::mem::take(
            &mut self
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .index,
        );
        let evaluated = owner.evaluate(
            None,
            &SourceCall {
                method: "getInstance".into(),
                arguments: vec![],
            },
            &mut NativeMainRawStartupSdkBackendV1 {
                core: self,
                index,
                fields,
            },
        );
        self.main_production
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?
            .index = owner;
        let evaluated = evaluated?;
        self.main_height_rejections
            .extend(evaluated.deferred_rejections);
        outcome(evaluated.outcome)
    }
    fn source_main_raw_startup_sdk_import(
        &mut self,
        index: &mut SourceCompactIndex,
        fields: &V,
    ) -> Result<(), SourceRuleError> {
        let mut owner = std::mem::take(
            &mut self
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .index,
        );
        let evaluated = owner.load_module(&mut NativeMainRawStartupSdkBackendV1 {
            core: self,
            index,
            fields,
        });
        self.main_production
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?
            .index = owner;
        let evaluated = evaluated?;
        self.main_height_rejections
            .extend(evaluated.deferred_rejections);
        Ok(())
    }
    fn source_main_raw_startup_sdk_request(&mut self, fields: &V) -> R<V> {
        self.emit(obj(&[
            ("kind", s("rpc-input")),
            ("method", s("Client.getInstance")),
            ("args", V::Array(vec![])),
            ("returned", fields.clone()),
        ]));
        Ok(obj(&[("$rpcClient", n(1.))]))
    }
}
