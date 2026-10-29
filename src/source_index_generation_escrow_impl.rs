// Separate readonly escrow snapshot, retaining original/copy bindings. Never
// substitutes a copied path for an original native owner or financial state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceIndexGenerationEscrowPinV1 {
    pub storage: crate::compact_journal::GenerationEscrowPinV1,
    pub points: Vec<SourceIndexHistoryPinV1>,
}
pub struct SourceIndexGenerationEscrowPermitV1 {
    storage: crate::compact_journal::DurableGenerationEscrowPermitV1,
    pin: SourceIndexGenerationEscrowPinV1,
}
impl SourceIndexGenerationEscrowPermitV1 {
    pub fn independently_retained_pin(&self) -> &SourceIndexGenerationEscrowPinV1 {
        &self.pin
    }
}
pub struct SourceIndexEscrowReadSnapshotV1 {
    expected: SourceIndexHistoryPinV1,
    directory: Directory,
    data: crate::compact_journal::GenerationEscrowSnapshotV1,
}
impl SourceIndexGenerationEscrowPinV1 {
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> std::result::Result<(), SourceRuleError>,
    ) -> std::result::Result<(), SourceRuleError> {
        visitor(
            "profile",
            &V::String("TL-TXINDEX-EXTERNAL-GENERATION-ESCROW-13-1".into()),
        )?;
        let raw = self
            .storage
            .canonical_descriptor_bytes()
            .map_err(|_| SourceRuleError::Backend)?;
        visitor(
            "storageDescriptorHex",
            &V::String(raw.iter().map(|b| format!("{b:02x}")).collect()),
        )?;
        visitor(
            "storageRecordSha256d",
            &V::String(
                self.storage
                    .record_sha256d
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect(),
            ),
        )?;
        visitor("points/count", &V::String(self.points.len().to_string()))?;
        for (i, pin) in self.points.iter().enumerate() {
            pin.visit_canonical_fields(&mut |p, v| visitor(&format!("points/{i}/{p}"), v))?;
        }
        Ok(())
    }
    pub fn open_historical(
        &self,
        expected: &SourceIndexHistoryPinV1,
    ) -> Result<SourceIndexEscrowReadSnapshotV1> {
        if !self.points.contains(expected)
            || expected.profile != VERIFIED_HISTORY_READ_PROFILE
            || expected.metadata.profile != METADATA_PROFILE
            || expected.metadata.publication != expected.storage.publication
            || expected.metadata.sequence != expected.storage.logical_sequence
            || expected.rows.profile != STREAM_PROFILE
            || !expected.rows.complete
            || expected.rows.snapshot != expected.metadata
        {
            return Err(CompactIndexError::WrongContext);
        }
        let data = self.storage.open_historical(&expected.storage)?;
        let directory = Directory::decode(data.get(DIRECTORY).ok_or(CompactIndexError::Corrupt)?)?;
        if directory.context != expected.metadata.context
            || data.entries().count() != directory.pages.len() + 1
        {
            return Err(CompactIndexError::Corrupt);
        }
        let snapshot = SourceIndexEscrowReadSnapshotV1 {
            expected: expected.clone(),
            directory,
            data,
        };
        if snapshot.visit_rows(|_| Ok(true))? != expected.rows {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(snapshot)
    }
}
impl SourceIndexEscrowReadSnapshotV1 {
    pub fn original(&self) -> &SourceIndexHistoryPinV1 {
        &self.expected
    }
    pub fn copied_file(&self) -> &Path {
        &self.data.copied_file
    }
    pub fn visit_rows(
        &self,
        mut visitor: impl FnMut(&V) -> Result<bool>,
    ) -> Result<RowStreamReceipt> {
        if self.raw_entries().count() != self.directory.pages.len() + 1 {
            return Err(CompactIndexError::Corrupt);
        }
        let mut receipt = RowStreamReceipt {
            profile: STREAM_PROFILE.into(),
            snapshot: self.expected.metadata.clone(),
            rows: 0,
            chain_sha256d: crate::hash256::double_sha256(STREAM_PROFILE.as_bytes()),
            complete: false,
        };
        let mut last = None;
        for page in &self.directory.pages {
            let rows = decode_page(
                self.raw_get(&page_key(page.number))
                    .ok_or(CompactIndexError::Corrupt)?,
            )?;
            if row_id(&rows[0])? != page.first {
                return Err(CompactIndexError::Corrupt);
            }
            for row in rows {
                let id = row_id(&row)?;
                if last.as_ref().is_some_and(|old: &String| old >= &id) {
                    return Err(CompactIndexError::Corrupt);
                }
                last = Some(id);
                let raw = row.diagnostic_json()?;
                let mut bytes = receipt.chain_sha256d.to_vec();
                bytes.extend_from_slice(&receipt.rows.to_be_bytes());
                bytes.extend_from_slice(&(raw.len() as u32).to_be_bytes());
                bytes.extend_from_slice(raw.as_bytes());
                receipt.chain_sha256d = crate::hash256::double_sha256(&bytes);
                receipt.rows += 1;
                if !visitor(&row)? {
                    return Ok(receipt);
                }
            }
        }
        if receipt.rows != self.directory.rows {
            return Err(CompactIndexError::Corrupt);
        }
        receipt.complete = true;
        Ok(receipt)
    }
}
impl SourceCompactIndex {
    pub fn retain_verified_history_generation(
        &mut self,
        escrow_root: &Path,
        points: &[SourceIndexHistoryPinV1],
        durability: crate::compact_journal::GenerationEscrowDurabilityV1,
    ) -> Result<SourceIndexGenerationEscrowPermitV1> {
        self.require_live_metadata_write()?;
        if points.is_empty() || points.len() > 128 {
            return Err(CompactIndexError::Limit);
        }
        for point in points {
            self.read_verified_history_generation(point)?;
        }
        let requested = points.iter().map(|p| p.storage.clone()).collect::<Vec<_>>();
        let storage =
            self.journal
                .retain_verified_generation(escrow_root, &requested, durability)?;
        let pin = SourceIndexGenerationEscrowPinV1 {
            storage: storage.independently_retained_pin().clone(),
            points: points.to_vec(),
        };
        for point in points {
            pin.open_historical(point)?;
        }
        Ok(SourceIndexGenerationEscrowPermitV1 { storage, pin })
    }
    /// Explicit guarded native append. The original method/body is untouched.
    pub fn commit_prepared_validity_with_escrow(
        &mut self,
        plan: &NativePreparedValidityWriteV1,
        permit: Option<&SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<StreamedValidityEvaluation> {
        self.require_live_metadata_write()?;
        self.require_snapshot(&plan.put.expected)?;
        if self.native_store_context()? != plan.context
            || self.native_complete_metadata_state()? != plan.before_state
        {
            return Err(CompactIndexError::WrongContext);
        }
        self.journal.commit_prepared_native_append_with_escrow(
            &plan.put.append,
            permit.map(|p| &p.storage),
        )?;
        self.directory = plan.put.directory.clone();
        if self.native_complete_metadata_state()? != plan.after_state {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(prepared_validity_evaluation(
            plan.query.clone(),
            plan.arguments.clone(),
            DependencyReturn::Pending(V::number(1.)),
            self.snapshot()?,
        ))
    }
}

include!("source_index_independent_escrow_impl.rs");
