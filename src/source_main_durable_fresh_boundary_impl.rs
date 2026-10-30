// Independent actual source-prefix producer. No financial/source answers.
pub const DURABLE_FRESH_COORDINATION_PROFILE: &str = "TL-MAIN-DURABLE-FRESH-FACTUAL-INPUT-13-1";
pub const DURABLE_FRESH_BOUNDARY_PROFILE: &str = "TL-MAIN-ACTUAL-FRESH-SOURCE-BOUNDARY-13-1";
pub struct ActualDurableFreshSourceBoundaryV1 {
    raw: Vec<u8>,
    pin: ActiveServiceWalPinV1,
    trace: crate::storage::JournalCommitTracePinV1,
}
impl ActualDurableFreshSourceBoundaryV1 {
    pub fn canonical_boundary_bytes(&self) -> &[u8] {
        &self.raw
    }
    pub fn source_pin(&self) -> &ActiveServiceWalPinV1 {
        &self.pin
    }
    pub fn source_trace(&self) -> &crate::storage::JournalCommitTracePinV1 {
        &self.trace
    }
}
impl ActiveServiceWalV1 {
    pub(crate) fn capture_actual_durable_fresh_source_boundary(
        &mut self,
    ) -> Result<ActualDurableFreshSourceBoundaryV1> {
        let origin = parse(&self.context)?.get("origin");
        actual_source_fact_origin(&origin)?;
        let record = self.record()?;
        if record.get("stage") == s("Ack") {
            actual_source_fact_entry_shape(&record)?;
        } else if record != obj(&[("profile", s(PROFILE)), ("phase", s("Created"))]) {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let pin = self.pin()?;
        let actual = self
            .journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .read_verified_current_commits(
                &pin.prefix,
                65536,
                MAX_VERIFIED_ACK_HISTORY_BYTES as usize,
            )?;
        let trace = actual.trace().clone();
        let raw = bytes(&obj(&[
            ("profile", s(DURABLE_FRESH_BOUNDARY_PROFILE)),
            ("sourceWal", actual_fact_wal_value(&pin)),
            ("originContext", parse(&self.context)?),
            ("actualRecord", record),
            (
                "trace",
                obj(&[
                    ("profile", s(&trace.profile)),
                    ("commits", s(trace.commits.to_string())),
                    ("bytes", s(trace.bytes.to_string())),
                    ("sha256d", s(hex(&trace.chain_sha256d))),
                ]),
            ),
        ]))?;
        ActiveServiceWalBlobV1::of(&raw)?;
        Ok(ActualDurableFreshSourceBoundaryV1 { raw, pin, trace })
    }
}

#[cfg(test)]
#[path = "source_main_durable_fresh_boundary_tests.rs"]
mod durable_fresh_boundary_tests;
