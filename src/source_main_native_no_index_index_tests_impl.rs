#[cfg(test)]
impl SourceCompactIndex {
    pub(crate) fn native_main_test_compaction_fixture(&mut self) -> Result<()> {
        self.configure_compaction(crate::compact_journal::CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 0,
        })?;
        self.import_row(
            &self.snapshot()?,
            V::Object(vec![
                ("_id".into(), V::String("unrelated".into())),
                ("padding".into(), V::String("x".repeat(8000))),
            ]),
        )?;
        for _ in 0..2000 {
            let directory = self
                .journal
                .get(DIRECTORY)?
                .ok_or(CompactIndexError::Corrupt)?
                .to_vec();
            let append = self.journal.prepare_native_append(vec![Mutation::Put {
                key: DIRECTORY.into(),
                value: directory,
            }])?;
            if append.intermediate().is_some() {
                return Ok(());
            }
            self.journal.commit_prepared_native_append(&append)?;
        }
        Err(CompactIndexError::Limit)
    }
    pub(crate) fn native_main_test_compaction_fault(&mut self, point: &str) {
        self.journal.test_fault(point, false);
    }
}
