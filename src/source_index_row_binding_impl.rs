impl SourceCompactIndex {
    pub fn native_observed_append_binding(
        &self,
    ) -> Result<crate::source_main_native_intent_store::NativeObservedAppendBindingV1> {
        crate::source_main_native_intent_store::NativeObservedAppendBindingV1::observe(
            &self.journal,
        )
        .map_err(|_| CompactIndexError::Source(SourceRuleError::Backend))
    }
}
fn native_row_binding_value(
    path: &str,
    value: &V,
    visitor: &mut impl FnMut(&str, &V) -> std::result::Result<(), SourceRuleError>,
) -> std::result::Result<(), SourceRuleError> {
    match value {
        V::Array(values) => {
            visitor(&format!("{path}/kind"), &prepared_text("Array"))?;
            visitor(
                &format!("{path}/count"),
                &prepared_text(values.len().to_string()),
            )?;
            for (at, value) in values.iter().enumerate() {
                native_row_binding_value(&format!("{path}/{at}"), value, visitor)?;
            }
        }
        V::Object(fields) => {
            visitor(&format!("{path}/kind"), &prepared_text("Object"))?;
            visitor(
                &format!("{path}/count"),
                &prepared_text(fields.len().to_string()),
            )?;
            for (at, (key, value)) in fields.iter().enumerate() {
                visitor(&format!("{path}/{at}/key"), &prepared_text(key))?;
                native_row_binding_value(&format!("{path}/{at}/value"), value, visitor)?;
            }
        }
        value => visitor(path, value)?,
    }
    Ok(())
}
impl MainIndexPhaseCursor {
    pub fn visit_native_phase_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> std::result::Result<(), SourceRuleError>,
    ) -> std::result::Result<(), SourceRuleError> {
        native_row_binding_value(
            "snapshot",
            &prepared_snapshot_value(self.snapshot.pin()),
            visitor,
        )?;
        visitor("at", &prepared_text(self.at.to_string()))?;
        visitor(
            "loadedPage",
            &self
                .page
                .map_or(V::Null, |page| prepared_text(page.to_string())),
        )?;
        visitor(
            "locations/count",
            &prepared_text(self.locations.len().to_string()),
        )?;
        for (at, location) in self.locations.iter().enumerate() {
            visitor(
                &format!("locations/{at}/page"),
                &prepared_text(location.page.to_string()),
            )?;
            visitor(
                &format!("locations/{at}/slot"),
                &prepared_text(location.slot.to_string()),
            )?;
        }
        visitor(
            "buffer/count",
            &prepared_text(self.buffer.len().to_string()),
        )?;
        for (at, row) in self.buffer.iter().enumerate() {
            native_row_binding_value(&format!("buffer/{at}"), row, visitor)?;
        }
        Ok(())
    }
}
impl MainIndexStreamPlan {
    pub fn visit_native_plan_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> std::result::Result<(), SourceRuleError>,
    ) -> std::result::Result<(), SourceRuleError> {
        native_row_binding_value(
            "snapshot",
            &prepared_snapshot_value(self.snapshot.pin()),
            visitor,
        )?;
        visitor("receipt/profile", &prepared_text(&self.receipt.profile))?;
        visitor(
            "receipt/rows",
            &prepared_text(self.receipt.rows.to_string()),
        )?;
        visitor(
            "receipt/chain",
            &prepared_text(prepared_hex(&self.receipt.chain_sha256d)),
        )?;
        visitor("receipt/complete", &V::Bool(self.receipt.complete))?;
        visitor(
            "groups/count",
            &prepared_text(self.groups.len().to_string()),
        )?;
        for (at, (height, group)) in self.groups.iter().enumerate() {
            visitor(
                &format!("groups/{at}/height"),
                &match height {
                    HeightKey::Number(height) => prepared_text(height.to_string()),
                    HeightKey::Nan => prepared_text("NaN"),
                },
            )?;
            for (name, locations) in [("funding", &group.funding), ("trade", &group.trade)] {
                visitor(
                    &format!("groups/{at}/{name}/count"),
                    &prepared_text(locations.len().to_string()),
                )?;
                for (slot, location) in locations.iter().enumerate() {
                    visitor(
                        &format!("groups/{at}/{name}/{slot}/page"),
                        &prepared_text(location.page.to_string()),
                    )?;
                    visitor(
                        &format!("groups/{at}/{name}/{slot}/slot"),
                        &prepared_text(location.slot.to_string()),
                    )?;
                }
            }
        }
        Ok(())
    }
}
