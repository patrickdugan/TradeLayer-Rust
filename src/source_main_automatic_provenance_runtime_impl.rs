impl NativeMainServiceContinuationV1 {
    fn automatic_owner_effect(&self, ranges: &[(usize, usize, usize)], at: usize) -> bool {
        self.native_file_profile
            && ranges
                .iter()
                .any(|(_, start, end)| *start <= at && at < *end)
    }
    fn visit_automatic_effect_provenance(
        &self,
        emit: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        if !self.native_file_profile {
            return Ok(());
        }
        emit(
            "automaticFile/ownerEffectProvenance/count",
            &s(self.automatic_owner_effect_provenance.len().to_string()),
        )?;
        for (event, ranges) in self.automatic_owner_effect_provenance.iter().enumerate() {
            emit(
                &format!("automaticFile/ownerEffectProvenance/{event}/count"),
                &s(ranges.len().to_string()),
            )?;
            for (at, (trace, start, end)) in ranges.iter().enumerate() {
                for (field, value) in [("trace", trace), ("start", start), ("end", end)] {
                    emit(
                        &format!("automaticFile/ownerEffectProvenance/{event}/{at}/{field}"),
                        &s(value.to_string()),
                    )?;
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
impl NativeMainServiceContinuationV1 {
    pub(crate) fn automatic_file_test_provenance_valid(&self) -> bool {
        self.native_file_profile
            && self.automatic_owner_effect_provenance.len() == self.events.len()
            && self
                .events
                .iter()
                .zip(&self.automatic_owner_effect_provenance)
                .all(|(event, ranges)| {
                    let mut prior_end = 0;
                    ranges.iter().all(|(trace, start, end)| {
                        let valid = *start >= prior_end
                            && *end >= *start
                            && event
                                .effects
                                .get(*start..*end)
                                .zip(event.trace.get(*trace))
                                .is_some_and(|(effects, trace)| {
                                    effects == trace.effects.as_slice()
                                });
                        prior_end = *end;
                        valid
                    })
                })
    }
}
