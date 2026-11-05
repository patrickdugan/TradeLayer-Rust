// Included in economic.rs. The input codec of the chain's rollback window: the
// inputs of a retained block in the TLEC input encoding, so that the block can
// be applied again from the state at the rollback floor.

impl EconomicState {
    /// Encodes the inputs of one block and checks that decoding them returns
    /// exactly the inputs, so that a replay applies what the block applied.
    pub(crate) fn encode_window_inputs(inputs: &[EconomicInput]) -> Result<Vec<u8>> {
        let mut out = EconomicWriter(Vec::new());
        let mut budget = ContractRecordBudget::default();
        budget.take(inputs.len())?;
        out.count(inputs.len(), MAX_ECONOMIC_INPUTS)?;
        for input in inputs {
            econ_write_input(&mut out, input, &mut budget)?;
        }
        if Self::decode_window_inputs(&out.0)? != inputs {
            return Err(EconomicError::InvalidInput);
        }
        Ok(out.0)
    }

    pub(crate) fn decode_window_inputs(bytes: &[u8]) -> Result<Vec<EconomicInput>> {
        let mut reader = Cursor::new(bytes);
        let mut budget = ContractRecordBudget::default();
        let count = econ_read_count(&mut reader, MAX_ECONOMIC_INPUTS)?;
        budget.take(count)?;
        let mut inputs = Vec::new();
        for _ in 0..count {
            inputs.push(econ_read_input(&mut reader, &mut budget)?);
        }
        reader.end().map_err(|_| econ_snapshot_bad())?;
        Ok(inputs)
    }
}

impl EconomicState {
    /// The encoding of a state that a block finisher just produced. The plain
    /// owner already audited it under the block's policy, so the encoder skips
    /// the second audit that `snapshot_bytes` runs; every other configuration
    /// takes its own encoder.
    pub(crate) fn snapshot_bytes_of_applied_block(&self) -> Result<Vec<u8>> {
        let plain = self.config.insurance.is_none()
            && self.config.native_entitlements.is_none()
            && self.config.source_consensus.is_none();
        if plain {
            self.snapshot_bytes_after_audit()
        } else {
            self.snapshot_bytes()
        }
    }
}

impl EconomicState {
    /// The snapshot in the layout that stored every internal leg id among the
    /// ledger's replay markers, byte for byte as it was written before legs
    /// were identified by the sequence. A state decoded from that layout
    /// writes it until its first block; this writer produces it for any plain
    /// owner state, for parity fixtures and for readers of the earlier layout.
    pub fn snapshot_bytes_with_stored_legs(&self) -> Result<Vec<u8>> {
        let mut stored = self.clone();
        let domain = stored.leg_domain();
        stored
            .joint
            .remember_stored_legs_unverified(&domain, stored.internal_sequence);
        stored.snapshot_bytes()
    }
}
