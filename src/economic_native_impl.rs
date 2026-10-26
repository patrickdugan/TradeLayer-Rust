// Opt-in native history is part of the actual Economic owner. No copied Ledger.
const ECONOMIC_NATIVE_MAGIC: &[u8; 5] = b"TLEC\x08";
const MAX_NATIVE_FRAMES: usize = 4096;
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeEconomicState {
    bootstrap: Vec<u8>,
    frames: Vec<ContractInputFrame>,
}
impl NativeEconomicState {
    pub fn bootstrap_bytes(&self) -> &[u8] {
        &self.bootstrap
    }
    pub fn frames(&self) -> &[ContractInputFrame] {
        &self.frames
    }
}
impl EconomicState {
    pub fn native_entitlement_state(&self) -> &NativeEconomicState {
        &self.native_entitlements
    }
    fn decode_joint_bootstrap(
        &self,
        bytes: &[u8],
    ) -> core::result::Result<JointProtocolState, crate::joint::JointSnapshotError> {
        match &self.config.native_entitlements {
            Some(p) => JointProtocolState::from_economic_snapshot_bytes_with_native_policy(
                bytes,
                self.config.network,
                self.joint.registry().protocol_admin_unverified(),
                self.joint.spot().permitted_properties(),
                p,
            ),
            None => JointProtocolState::from_economic_snapshot_bytes(
                bytes,
                self.config.network,
                self.joint.registry().protocol_admin_unverified(),
                self.joint.spot().permitted_properties(),
            ),
        }
    }
    fn native_config_bytes(config: &EconomicConfig) -> Result<Vec<u8>> {
        let mut out = EconomicWriter(Vec::new());
        // Shape flags make all optional native-v8 configurations unambiguous.
        out.byte(
            u8::from(config.funding.is_some())
                | (u8::from(config.marking.is_some()) << 1)
                | (u8::from(config.liquidation.is_some()) << 2)
                | (u8::from(config.amm.is_some()) << 3)
                | (u8::from(config.contract_binding.is_some()) << 4),
        )?;
        econ_write_config(&mut out, config)?;
        match &config.source_consensus {
            None => out.byte(0)?,
            Some(s) => {
                out.byte(1)?;
                out.put(&s.bootstrap_commitment)?;
                match s.policy {
                    SourceConsensusPolicy::ConservingSequentialV1 => out.byte(1)?,
                }
            }
        }
        out.embedded(
            &crate::native_entitlement::canonical_policy_bytes(
                config
                    .native_entitlements
                    .as_ref()
                    .ok_or(EconomicError::InvalidConfig)?,
            )
            .map_err(|_| EconomicError::InvalidConfig)?,
        )?;
        Ok(out.0)
    }
    fn write_native_frames(
        out: &mut EconomicWriter,
        frames: &[ContractInputFrame],
        budget: &mut ContractRecordBudget,
    ) -> Result<()> {
        budget.take(frames.len())?;
        out.count(frames.len(), MAX_NATIVE_FRAMES)?;
        for f in frames {
            out.u64(f.height)?;
            budget.take(f.inputs.len())?;
            out.count(f.inputs.len(), MAX_ECONOMIC_INPUTS)?;
            for input in &f.inputs {
                econ_write_input(out, input, budget)?;
            }
        }
        Ok(())
    }
    fn preflight_native_input_iter<'a>(
        &self,
        height: u64,
        count: usize,
        inputs: impl Iterator<Item = &'a EconomicInput>,
    ) -> Result<()> {
        if self.config.native_entitlements.is_none() {
            return Ok(());
        }
        if self.native_entitlements.frames.len() >= MAX_NATIVE_FRAMES || count > MAX_ECONOMIC_INPUTS
        {
            return Err(EconomicError::Capacity);
        }
        let mut out = EconomicWriter(Vec::new());
        let mut budget = ContractRecordBudget::default();
        Self::write_native_frames(&mut out, &self.native_entitlements.frames, &mut budget)?;
        budget.take(1)?;
        budget.take(count)?;
        out.u64(height)?;
        out.count(count, MAX_ECONOMIC_INPUTS)?;
        for input in inputs {
            if self.config.source_consensus.is_some()
                && matches!(input, EconomicInput::NativeTarget(_))
            {
                return Err(EconomicError::InvalidInput);
            }
            econ_write_input(&mut out, input, &mut budget)?;
        }
        Ok(())
    }
    fn preflight_native_inputs(&self, height: u64, inputs: &[EconomicInput]) -> Result<()> {
        self.preflight_native_input_iter(height, inputs.len(), inputs.iter())
    }
    fn preflight_native_candidate(
        &self,
        height: u64,
        preceding: &[EconomicInput],
        candidate: &EconomicInput,
    ) -> Result<()> {
        self.preflight_native_input_iter(
            height,
            preceding
                .len()
                .checked_add(1)
                .ok_or(EconomicError::Capacity)?,
            preceding.iter().chain(core::iter::once(candidate)),
        )
    }
    fn verify_native_entitlements(&self) -> Result<()> {
        let Some(policy) = &self.config.native_entitlements else {
            return if self.native_entitlements == NativeEconomicState::default()
                && !self.replaying_native_history
                && self.ledger().native_policy().is_none()
            {
                Ok(())
            } else {
                Err(EconomicError::Invariant)
            };
        };
        if self.ledger().native_policy() != Some(policy)
            || self.native_entitlements.frames.len() as u64 != self.finalized_height().unwrap_or(0)
        {
            return Err(EconomicError::Invariant);
        }
        for (index, f) in self.native_entitlements.frames.iter().enumerate() {
            if f.height != index as u64 + 1
                || f.inputs.len() > MAX_ECONOMIC_INPUTS
                || (self.config.source_consensus.is_some()
                    && f.inputs
                        .iter()
                        .any(|i| matches!(i, EconomicInput::NativeTarget(_))))
            {
                return Err(EconomicError::Invariant);
            }
        }
        if self.config.contract_binding.is_some()
            && (self.contract_binding.bootstrap != self.native_entitlements.bootstrap
                || self.contract_binding.frames != self.native_entitlements.frames)
        {
            return Err(EconomicError::Invariant);
        }
        if self.config.source_consensus.is_some()
            && self.source_consensus.bootstrap != self.native_entitlements.bootstrap
        {
            return Err(EconomicError::Invariant);
        }
        let mut out = EconomicWriter(Vec::new());
        Self::write_native_frames(
            &mut out,
            &self.native_entitlements.frames,
            &mut ContractRecordBudget::default(),
        )?;
        // Other full-history drivers use the same private transition functions;
        // their final whole-owner equality still includes these native frames.
        if self.replaying_native_history
            || self.replaying_contract_history
            || self.replaying_source_history
            || self.config.source_consensus.is_some()
        {
            return Ok(());
        }
        let bootstrap = self
            .decode_joint_bootstrap(&self.native_entitlements.bootstrap)
            .map_err(|_| EconomicError::InvalidSnapshot)?;
        let mut replay =
            Self::from_joint_native_mode_unverified(self.config.clone(), bootstrap, true)?;
        replay.replaying_contract_history = self.config.contract_binding.is_some();
        for f in &self.native_entitlements.frames {
            replay.apply_block_unverified(f.height, f.inputs.clone())?;
        }
        replay.replaying_contract_history = false;
        replay.replaying_native_history = false;
        if replay != *self {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }
    fn native_snapshot_bytes(&self) -> Result<Vec<u8>> {
        self.verify()?;
        let mut out = EconomicWriter(ECONOMIC_NATIVE_MAGIC.to_vec());
        out.embedded(&Self::native_config_bytes(&self.config)?)?;
        out.embedded(&self.native_entitlements.bootstrap)?;
        Self::write_native_frames(
            &mut out,
            &self.native_entitlements.frames,
            &mut ContractRecordBudget::default(),
        )?;
        match &self.config.source_consensus {
            None => out.byte(0)?,
            Some(_) => {
                out.byte(1)?;
                let mut s = EconomicWriter(Vec::new());
                econ_write_source_state(
                    &mut s,
                    &self.source_consensus,
                    &mut SourceBudget::default(),
                )?;
                out.embedded(&s.0)?;
            }
        }
        with_crc(out.0).map_err(|_| EconomicError::Capacity)
    }
    fn from_native_snapshot_bytes(
        bytes: &[u8],
        expected: &EconomicConfig,
        admin: &AccountId,
        permitted: &BTreeSet<PropertyId>,
    ) -> Result<Self> {
        expected.validate()?;
        let policy = expected
            .native_entitlements
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let raw = verify_crc(bytes).map_err(|_| EconomicError::InvalidSnapshot)?;
        let mut r = Cursor::new(raw);
        if r.fixed::<5>().map_err(|_| econ_snapshot_bad())? != *ECONOMIC_NATIVE_MAGIC {
            return Err(econ_snapshot_bad());
        }
        if econ_read_embedded(&mut r)? != Self::native_config_bytes(expected)? {
            return Err(EconomicError::InvalidConfig);
        }
        let bootstrap_bytes = econ_read_embedded(&mut r)?;
        let bootstrap = JointProtocolState::from_economic_snapshot_bytes_with_native_policy(
            bootstrap_bytes,
            expected.network,
            admin,
            permitted,
            policy,
        )
        .map_err(|_| EconomicError::InvalidSnapshot)?;
        let count = econ_read_count(&mut r, MAX_NATIVE_FRAMES)?;
        let mut budget = ContractRecordBudget::default();
        budget.take(count)?;
        let mut frames = Vec::with_capacity(count);
        for _ in 0..count {
            let height = econ_read_u64(&mut r)?;
            let count = econ_read_count(&mut r, MAX_ECONOMIC_INPUTS)?;
            budget.take(count)?;
            let mut inputs = Vec::with_capacity(count);
            for _ in 0..count {
                inputs.push(econ_read_input(&mut r, &mut budget)?);
            }
            frames.push(ContractInputFrame { height, inputs });
        }
        let source = match r.u8().map_err(|_| econ_snapshot_bad())? {
            0 if expected.source_consensus.is_none() => None,
            1 if expected.source_consensus.is_some() => {
                let source_bytes = econ_read_embedded(&mut r)?;
                if source_bytes.len() > MAX_SOURCE_HISTORY_BYTES {
                    return Err(EconomicError::Capacity);
                }
                let mut reader = Cursor::new(source_bytes);
                let s = econ_read_source_state(&mut reader, &mut SourceBudget::default())?;
                reader.end().map_err(|_| econ_snapshot_bad())?;
                Some(s)
            }
            _ => return Err(econ_snapshot_bad()),
        };
        r.end().map_err(|_| econ_snapshot_bad())?;
        // No current custody, validity answer or receipt is trusted from this
        // container: actual owner transitions reconstruct every resulting field.
        let mut replay =
            Self::from_joint_native_mode_unverified(expected.clone(), bootstrap, true)?;
        replay.replaying_contract_history = expected.contract_binding.is_some();
        if let Some(source) = source {
            if source.bootstrap != bootstrap_bytes {
                return Err(econ_snapshot_bad());
            }
            replay.replaying_source_history = true;
            for f in &source.frames {
                let (next, _) = consensus_source::execute_block_unverified(
                    &replay,
                    f.header,
                    f.mode,
                    &f.transactions,
                )
                .map_err(|_| econ_snapshot_bad())?;
                replay = next;
                if replay.source_consensus.frames.last() != Some(f) {
                    return Err(econ_snapshot_bad());
                }
            }
            if replay.source_consensus != source {
                return Err(econ_snapshot_bad());
            }
            replay.replaying_source_history = false;
        } else {
            for f in &frames {
                replay.apply_block_unverified(f.height, f.inputs.clone())?;
            }
        }
        replay.replaying_contract_history = false;
        replay.replaying_native_history = false;
        if replay.native_entitlements.frames != frames
            || replay.snapshot_bytes()?.as_slice() != bytes
        {
            return Err(econ_snapshot_bad());
        }
        Ok(replay)
    }
    pub(crate) fn native_history_transition(&self, after: &Self, height: u64) -> Result<()> {
        if self.config.native_entitlements != after.config.native_entitlements {
            return Err(EconomicError::Invariant);
        }
        if self.config.native_entitlements.is_none() {
            return Ok(());
        }
        let before = &self.native_entitlements;
        let next = &after.native_entitlements;
        if before.bootstrap != next.bootstrap
            || !next.frames.starts_with(&before.frames)
            || next.frames.len() != before.frames.len() + 1
            || next.frames.last().is_none_or(|f| f.height != height)
            || !self
                .ledger()
                .native_history_is_prefix_of(after.ledger(), height)
        {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }
}
