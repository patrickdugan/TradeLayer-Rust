// Explicit dependency-return provenance for V3 contract composition. These
// transient slots never introduce another position or balance authority.
fn process_contract_matches_actual(
    arguments: &[V],
    backend: &mut FinancialBackend<'_>,
) -> Result<SourceOutcome, SourceRuleError> {
    if backend.core.lifecycle_enabled() {
        crate::contract_settlement_source::process_contract_matches_with_position_identity(
            arguments, backend,
        )
    } else {
        crate::contract_settlement_source::process_contract_matches(arguments, backend)
    }
}
impl ContractCore {
    fn source_margin_field_aliases(&self, handle: &V) -> R<Vec<PositionFieldObjectAlias>> {
        let V::Array(entries) = handle.get("sourcePositionFieldObjectAliases") else {
            return Ok(vec![]);
        };
        entries
            .iter()
            .map(|v| {
                Ok(PositionFieldObjectAlias {
                    argument: number(&v.get("argument"))? as usize,
                    field: text(&v.get("field"))?,
                    object: number(&v.get("object"))? as usize,
                })
            })
            .collect()
    }
}
impl crate::contract_settlement_source::ContractSettlementIdentityBackend for FinancialBackend<'_> {
    fn invoke_with_position_identity(
        &mut self,
        call: &DependencyCall,
        roots: &[PositionObjectAlias],
        fields: &[PositionFieldObjectAlias],
    ) -> Result<(DependencyReturn, Option<usize>), SourceRuleError> {
        if !self.core.lifecycle_enabled() {
            return self.invoke(call).map(|v| (v, None));
        }
        // These values descend from the exact return IDs retained by Engine.
        // Copying its source-local property mutations back is not an equality
        // search and does not bind any position solely by address.
        for alias in roots {
            if alias.index.is_some() {
                return Err(SourceRuleError::InvalidInput);
            }
            let value = call
                .arguments
                .get(alias.argument)
                .ok_or(SourceRuleError::InvalidInput)?;
            self.core
                .margins
                .replace_position_object(alias.object, value.clone())?;
        }
        for alias in fields {
            let argument = call
                .arguments
                .get(alias.argument)
                .ok_or(SourceRuleError::InvalidInput)?;
            let value = argument.get(&alias.field);
            self.core
                .margins
                .replace_position_object(alias.object, value)?;
        }
        let mut actual = call.clone();
        if call.target.starts_with("Margin.") && (!roots.is_empty() || !fields.is_empty()) {
            let Some(handle) = actual.arguments.first_mut() else {
                return Err(SourceRuleError::InvalidInput);
            };
            if !roots.is_empty() {
                let aliases = roots
                    .iter()
                    .map(|a| {
                        Ok(obj(&[
                            (
                                "argument",
                                n(a.argument
                                    .checked_sub(1)
                                    .ok_or(SourceRuleError::InvalidInput)?
                                    as f64),
                            ),
                            ("object", n(a.object as f64)),
                        ]))
                    })
                    .collect::<Result<Vec<_>, SourceRuleError>>()?;
                set(handle, "sourcePositionObjectAliases", V::Array(aliases))
                    .map_err(|_| SourceRuleError::InvalidInput)?;
            }
            if !fields.is_empty() {
                let aliases = fields
                    .iter()
                    .map(|a| {
                        Ok(obj(&[
                            (
                                "argument",
                                n(a.argument
                                    .checked_sub(1)
                                    .ok_or(SourceRuleError::InvalidInput)?
                                    as f64),
                            ),
                            ("field", s(&a.field)),
                            ("object", n(a.object as f64)),
                        ]))
                    })
                    .collect::<Result<Vec<_>, SourceRuleError>>()?;
                set(
                    handle,
                    "sourcePositionFieldObjectAliases",
                    V::Array(aliases),
                )
                .map_err(|_| SourceRuleError::InvalidInput)?;
            }
        }
        // Only actual position argument 14 flows to setInitialMargin's fifth
        // argument. In particular, preserve the seller's omitted position and
        // its mistaken argument12; do not turn it into the buyer signature.
        let collateral = if call.target == "Contract.moveCollateralToMargin" {
            roots
                .iter()
                .find(|a| a.argument == 14 && a.index.is_none())
                .map(|a| a.object)
        } else {
            None
        };
        let previous = std::mem::replace(&mut self.core.collateral_position_origin, collateral);
        let active = std::mem::replace(&mut self.core.position_identity_active, true);
        self.core.returned_position_origin = None;
        let result = self.invoke(&actual);
        let origin = self.core.returned_position_origin.take();
        self.core.collateral_position_origin = previous;
        self.core.position_identity_active = active;
        result.map(|v| (v, origin))
    }
}
