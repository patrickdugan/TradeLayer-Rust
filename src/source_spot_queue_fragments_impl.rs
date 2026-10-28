// Explicit V9 callers only. Existing source evaluators retain their old graph.
// No pending state or second arena lives in the queue owner.
impl ContractQueueSourceOwner {
    pub(crate) fn source_book_factory(&mut self, key: V) -> Result<SourceOutcome, SourceRuleError> {
        let before = self.clone();
        let result = (|| -> Eval<V> {
            let key = text(&key)?;
            self.database()?;
            Ok(V::String(key))
        })();
        self.source_fragment_result(before, result)
    }
    pub(crate) fn source_finish_book_load(
        &mut self,
        instance: u64,
        key: V,
    ) -> Result<(SourceOutcome, Option<String>), SourceRuleError> {
        let before = self.clone();
        let mut parsed = false;
        let result = (|| -> Eval<V> {
            let key = text(&key)?;
            self.source_load_book_after_factory(instance, key, &mut parsed)
        })();
        let alias = if parsed && matches!(&result, Ok(V::Object(_) | V::Array(_))) {
            Some(text(&key).map_err(|_| SourceRuleError::InvalidInput)?)
        } else {
            None
        };
        self.source_fragment_result(before, result)
            .map(|outcome| (outcome, alias))
    }
    pub(crate) fn source_finish_book_save(
        &mut self,
        data: V,
        key: V,
    ) -> Result<SourceOutcome, SourceRuleError> {
        let before = self.clone();
        let result = (|| -> Eval<V> {
            let key = text(&key)?;
            let value = json_stringify(&data)?;
            self.update(s(&key), obj(&[("_id", s(&key)), ("value", value)]))?;
            Ok(V::Undefined)
        })();
        self.source_fragment_result(before, result)
    }
    fn source_fragment_result(
        &mut self,
        before: Self,
        result: Eval<V>,
    ) -> Result<SourceOutcome, SourceRuleError> {
        if let Err(Fault::Local(e)) = &result {
            let e = e.clone();
            *self = before;
            return Err(e);
        }
        if let Err(e) = self.checkpoint() {
            *self = before;
            return Err(e);
        }
        match result {
            Ok(v) => Ok(SourceOutcome::Returned(v)),
            Err(Fault::Thrown(e)) => Ok(SourceOutcome::Thrown(e)),
            Err(Fault::Local(e) | Fault::Boundary(e)) => Err(e),
        }
    }
}
