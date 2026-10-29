// Only the explicit ordered-bootstrap controller calls these private source
// continuation points. The SAME metadata owner survives each real await.
impl TxIndexSourceOwner {
    pub(crate) fn ordered_bootstrap_import_begin(&mut self) -> Result<(), SourceRuleError> {
        if self.module_loaded
            || self.txutils_client != V::Undefined
            || self.client_rejection.is_some()
        {
            return Err(SourceRuleError::InvalidInput);
        }
        self.module_loaded = true;
        Ok(())
    }
    pub(crate) fn ordered_bootstrap_import_fulfill(
        &mut self,
        client: V,
    ) -> Result<(), SourceRuleError> {
        if !self.module_loaded
            || self.txutils_client != V::Undefined
            || self.client_rejection.is_some()
        {
            return Err(SourceRuleError::InvalidInput);
        }
        client.validate()?;
        // Actual TxUtils.init assignment after its clientPromise await.
        self.txutils_client = client;
        Ok(())
    }
    pub(crate) fn ordered_bootstrap_import_reject(
        &mut self,
        rejection: SourceException,
    ) -> Result<(), SourceRuleError> {
        if !self.module_loaded
            || self.txutils_client != V::Undefined
            || self.client_rejection.is_some()
        {
            return Err(SourceRuleError::InvalidInput);
        }
        // No client assignment occurs on a rejected await. The controller
        // retains the actual Error reference and outer IIFE promise identity.
        self.client_rejection = Some(rejection);
        Ok(())
    }
    pub(crate) fn ordered_bootstrap_constructor(&mut self) -> Result<(), SourceRuleError> {
        if !self.module_loaded || self.singleton || self.class_client != V::Undefined {
            return Err(SourceRuleError::InvalidInput);
        }
        // Literal TxIndex constructor stores the singleton before static init.
        self.construct();
        Ok(())
    }
    pub(crate) fn ordered_bootstrap_static_fulfill(
        &mut self,
        client: V,
        bound_client_fields: &V,
    ) -> Result<SourceOutcome, SourceRuleError> {
        if !self.singleton || self.class_client != V::Undefined {
            return Err(SourceRuleError::InvalidInput);
        }
        client.validate()?;
        self.class_client = client;
        // Preserve assignment-before-console-dereference partial failure.
        // RPC handles retain identity; property reads use that handle's SAME
        // declared raw client descriptor rather than inventing undefined fields.
        let fields = if self.class_client == obj(&[("$rpcClient", V::number(1.))]) {
            bound_client_fields
        } else {
            &self.class_client
        };
        match prop(fields, "chain").and_then(|value| text(&value)) {
            Ok(_) => Ok(SourceOutcome::Returned(obj(&[(
                "$txIndexClass",
                V::Bool(true),
            )]))),
            Err(Fault::Source(error)) => Ok(SourceOutcome::Thrown(error)),
            Err(Fault::Local(error)) => Err(error),
        }
    }
}
