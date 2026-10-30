/// Borrowed bounded window of an already independently checked mixed trace.
/// It carries metadata/typed environmental facts, never a source outcome.
pub struct VerifiedMixedNativeWindowAccessV1<'a> {
    reader: &'a VerifiedMixedNativeHistoryV1,
    window: &'a VerifiedMixedNativeWindowV1,
}
impl VerifiedMixedNativeHistoryV1 {
    pub fn window_access<'a>(
        &'a self,
        window: &'a VerifiedMixedNativeWindowV1,
    ) -> Result<VerifiedMixedNativeWindowAccessV1<'a>> {
        if window.reader_owner != self.store.owner {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        // Bind the actual entry blobs and root chain to THIS retained reader.
        // A window is not constructible publicly, and these comparisons also
        // refuse accidental cross-reader windows with different evidence.
        for entry in &window.entries {
            let value = parse(&self.read_reference(&entry.entry)?)?;
            if history_count(&value.get("sequence"))? != entry.sequence
                || value.get("profile") != s(MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE)
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
        }
        Ok(VerifiedMixedNativeWindowAccessV1 {
            reader: self,
            window,
        })
    }
}
impl VerifiedMixedNativeWindowAccessV1<'_> {
    pub fn entries(&self) -> &[VerifiedActiveServiceAckEntryV1] {
        &self.window.entries
    }
    pub fn read_reference(&self, pin: &ActiveServiceWalBlobV1) -> Result<Vec<u8>> {
        self.reader.read_reference(pin)
    }
    pub fn origin(&self) -> &V {
        self.reader.origin()
    }
    pub fn origin_history(&self) -> &[u8] {
        self.reader.origin_history()
    }
    fn entry(&self, sequence: u64) -> Result<&VerifiedActiveServiceAckEntryV1> {
        self.entries()
            .iter()
            .find(|e| e.sequence == sequence)
            .ok_or(ActiveServiceWalErrorV1::WrongPin)
    }
    pub fn entry_record(&self, sequence: u64) -> Result<V> {
        parse(&self.read_reference(&self.entry(sequence)?.entry)?)
    }
    pub fn source_administrative_refs(
        &self,
        sequence: u64,
    ) -> Result<(
        Option<ActiveServiceWalBlobV1>,
        Option<ActiveServiceWalBlobV1>,
    )> {
        let entry = self.entry_record(sequence)?;
        let optional = |name: &str| match entry.get(name) {
            V::Undefined | V::Null => Ok(None),
            v => history_blob(&v).map(Some),
        };
        Ok((optional("nativeRecovery")?, optional("generationEscrow")?))
    }
    pub fn source_pending_input(
        &self,
        sequence: u64,
        after: bool,
    ) -> Result<Option<ActualSourcePendingFactInputV1>> {
        let value = self.entry_record(sequence)?;
        let suffix = if after { "After" } else { "Before" };
        let reference = value.get(&format!("actualPendingPlan{suffix}"));
        if reference == V::Null {
            return Ok(None);
        }
        let metadata = ActualNativePlanFactExpectationV1::from_canonical_fact_bytes(
            &self.reader.mixed_expected.metadata_namespace,
            &self.read_reference(&history_blob(&reference)?)?,
        )?;
        let selection = value.get(&format!("actualSelection{suffix}"));
        if metadata.native_read_context() != selection.get("pending").get("context") {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        Ok(Some(ActualSourcePendingFactInputV1 {
            metadata,
            selection,
        }))
    }
    pub fn source_io_inputs(&self, sequence: u64) -> Result<Vec<ActualSourceIoFactInputV1>> {
        let mut inputs = vec![];
        for (physical, value) in self.window.records.iter().filter(|(_, r)| {
            r.get("sequence") == s(sequence.to_string()) && r.get("stage") == s("NativeIoObserved")
        }) {
            if MixedNativeHistoryOperationV1::from_record(value)?
                != MixedNativeHistoryOperationV1::Inline
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let pending =
                self.read_reference(&history_blob(&value.get("actualNativeIoPendingFact"))?)?;
            let metadata = ActualNativePlanFactExpectationV1::from_canonical_fact_bytes(
                &self.reader.mixed_expected.metadata_namespace,
                &pending,
            )?;
            let io = ActualNativeIoFactExpectationV1::from_canonical_fact_bytes(
                &self.reader.mixed_expected.metadata_namespace,
                &pending,
                &self.read_reference(&history_blob(&value.get("actualNativeIoFact"))?)?,
            )?;
            let selection = value.get("actualIoSelection");
            if metadata.native_read_context() != selection.get("pending").get("context") {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            inputs.push(ActualSourceIoFactInputV1 {
                physical: *physical,
                metadata,
                io,
                selection,
            });
        }
        Ok(inputs)
    }
    pub fn ordinary_pending_input(
        &self,
        sequence: u64,
        after: bool,
    ) -> Result<Option<(ActualValidityPlanFactExpectationV1, V)>> {
        let value = self.entry_record(sequence)?;
        let suffix = if after { "After" } else { "Before" };
        let reference = value.get(&format!("ordinaryPendingPlan{suffix}"));
        if reference == V::Null {
            return Ok(None);
        }
        let metadata = ActualValidityPlanFactExpectationV1::from_canonical_fact_bytes(
            &self.reader.mixed_expected.metadata_namespace,
            &self.read_reference(&history_blob(&reference)?)?,
        )?;
        let selection = value.get(&format!("ordinarySelection{suffix}"));
        if metadata.native_read_context()
            != selection.get("actualSource").get("pending").get("context")
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        Ok(Some((metadata, selection)))
    }
    pub fn ordinary_io_inputs(&self, sequence: u64) -> Result<Vec<MixedOrdinaryColdIoV1>> {
        let mut out = vec![];
        for (physical, value) in self.window.records.iter().filter(|(_, r)| {
            r.get("sequence") == s(sequence.to_string()) && r.get("stage") == s("NativeIoObserved")
        }) {
            if MixedNativeHistoryOperationV1::from_record(value)?
                != MixedNativeHistoryOperationV1::Validity
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let pending =
                self.read_reference(&history_blob(&value.get("ordinaryNativeIoPendingFact"))?)?;
            let metadata = ActualValidityPlanFactExpectationV1::from_canonical_fact_bytes(
                &self.reader.mixed_expected.metadata_namespace,
                &pending,
            )?;
            let io = ActualValidityIoFactExpectationV1::from_canonical_fact_bytes(
                &self.reader.mixed_expected.metadata_namespace,
                &pending,
                &self.read_reference(&history_blob(&value.get("ordinaryNativeIoFact"))?)?,
            )?;
            let selection = value.get("ordinaryIoSelection");
            if metadata.native_read_context()
                != selection.get("actualSource").get("pending").get("context")
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            out.push(MixedOrdinaryColdIoV1 {
                physical: *physical,
                metadata,
                io,
                selection,
            });
        }
        Ok(out)
    }
    pub(crate) fn verify_ordinary_success_plan(
        &self,
        sequence: u64,
        source: &crate::source_compact_index_v2::NativeHistoricalValidityAwaitV1,
    ) -> Result<()> {
        let inputs = self.ordinary_io_inputs(sequence)?;
        if inputs.len() != 1 || inputs[0].io.native_io().is_some() {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let input = &inputs[0];
        input.metadata.verify_recomputed_initial_plan(source)?;
        let candidates = Mode4Candidates::parse(
            input.metadata.native_candidates(),
            input.metadata.original(),
        )?;
        if candidates.read_context != source.durable_read_context()
            || candidates.before != Mode4AppendPoint::of(input.metadata.original())
            || candidates.post != Mode4AppendPoint::of(source.original_post())
        {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        let raw = self
            .entry(sequence)?
            .native_candidates()
            .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
        if parse(raw)? != input.metadata.native_candidates() {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        Ok(())
    }
}
pub struct MixedOrdinaryColdIoV1 {
    physical: u64,
    metadata: ActualValidityPlanFactExpectationV1,
    io: ActualValidityIoFactExpectationV1,
    selection: V,
}
impl MixedOrdinaryColdIoV1 {
    pub fn physical(&self) -> u64 {
        self.physical
    }
    pub fn metadata(&self) -> &ActualValidityPlanFactExpectationV1 {
        &self.metadata
    }
    pub fn io(&self) -> &ActualValidityIoFactExpectationV1 {
        &self.io
    }
    pub fn selection(&self) -> &V {
        &self.selection
    }
}
