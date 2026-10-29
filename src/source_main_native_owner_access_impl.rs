// Read-only owner identity access for the explicit native Main loan boundary.
impl SourceCompactIndex {
    pub fn native_namespace(&self) -> &Namespace {
        self.journal.namespace()
    }
    pub(crate) fn native_database(
        &mut self,
        method: &str,
        arguments: &[V],
    ) -> Result<DependencyReturn> {
        self.database(method, arguments)
    }
    pub fn native_store_context(&self) -> Result<(String, std::path::PathBuf)> {
        Ok(self.journal.native_main_store_context()?)
    }
}
impl MainIndexPhaseCursor {
    pub fn remaining_rows(&self) -> u64 {
        (self.locations.len() - self.at) as u64
    }
}
