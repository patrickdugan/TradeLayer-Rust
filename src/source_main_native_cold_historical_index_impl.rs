// Mechanically derived original source owner IO/catchup prefix.
struct NativeMainHistoricalIndexBackend<'a> {
    core: &'a mut ContractCore,
    view: &'a crate::source_compact_index_v2::SourceIndexReadSnapshot,
}
impl SourceBackend for NativeMainHistoricalIndexBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if call.target == "db.getDatabase" {
            if call.arguments != [s("txIndex")] {
                return Err(SourceRuleError::InvalidInput);
            }
            return match self.core.database("txIndex") {
                Ok(_) => Ok(DependencyReturn::Pending(obj(&[(
                    "$database",
                    s("txIndex"),
                )]))),
                Err(Fault::Thrown(e)) => Ok(DependencyReturn::PendingThrow(e)),
                Err(Fault::Local(e)) => Err(e),
            };
        }
        if let Some(method) = call.target.strip_prefix("db.") {
            if call
                .arguments
                .first()
                .is_none_or(|v| v.get("$database") != s("txIndex"))
            {
                return Err(SourceRuleError::InvalidInput);
            }
            let args = &call.arguments[1..];
            let result = match self.core.check_failure("txIndex", method) {
                Ok(()) => {
                    self.view
                        .source_database_read(method, args)
                        .map_err(|error| match error {
                            crate::source_compact_index_v2::CompactIndexError::Source(rule) => rule,
                            crate::source_compact_index_v2::CompactIndexError::Limit => {
                                SourceRuleError::Capacity
                            }
                            _ => SourceRuleError::Backend,
                        })?
                }
                Err(Fault::Thrown(e)) => DependencyReturn::PendingThrow(e),
                Err(Fault::Local(e)) => return Err(e),
            };
            let mut effect = obj(&[
                ("kind", s("db")),
                ("collection", s("txIndex")),
                ("op", s(method.strip_suffix("Async").unwrap_or(method))),
                ("method", s(method)),
                ("args", V::Array(args.to_vec())),
            ]);
            match &result {
                DependencyReturn::Immediate(value) | DependencyReturn::Pending(value) => {
                    set(&mut effect, "returned", value.clone())
                }
                DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error) => set(
                    &mut effect,
                    "thrown",
                    obj(&[("name", s(&error.name)), ("message", s(&error.message))]),
                ),
            }
            .map_err(|_| SourceRuleError::InvalidInput)?;
            self.core.emit(effect);
            return Ok(result);
        }
        MainIndexBackend(self.core).invoke(call)
    }
}

impl ContractCore {
    fn native_main_index_historical_scope(
        &mut self,
        view: &crate::source_compact_index_v2::SourceIndexReadSnapshot,
        instance: Option<u64>,
        method: &str,
        args: Vec<V>,
    ) -> R<V> {
        let mut owner = std::mem::take(
            &mut self
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .index,
        );
        let result = owner.evaluate(
            instance,
            &SourceCall {
                method: method.into(),
                arguments: args,
            },
            &mut NativeMainHistoricalIndexBackend { core: self, view },
        );
        self.main_production
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?
            .index = owner;
        let result = result?;
        for rejection in result.deferred_rejections {
            self.main_height_rejections.push(rejection);
        }
        outcome(result.outcome)
    }
}
