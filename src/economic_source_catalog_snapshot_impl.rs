// Included in economic.rs. TLECv10 retains the complete original source audit,
// both independently pinned bootstraps, and the full actual catalog owners.
// The embedded source-free owner keeps its existing canonical codec, including
// native-v8 history. CRC and canonical structure are not authentication.
const ECONOMIC_SOURCE_CATALOG_MAGIC: &[u8; 5] = b"TLEC\x0a";

impl EconomicState {
    fn source_catalog_snapshot_bytes(&self) -> Result<Vec<u8>> {
        self.verify()?;
        let source_config = self
            .config
            .source_consensus
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let catalog_pin = source_config
            .catalog_bootstrap_commitment
            .ok_or(EconomicError::InvalidConfig)?;
        if self.config.insurance.is_some() {
            return Err(EconomicError::InvalidConfig);
        }
        let catalogs = self
            .source_consensus
            .catalogs
            .as_ref()
            .ok_or(EconomicError::Invariant)?;
        let initial_catalogs = &self.source_consensus.catalog_bootstrap;
        if crate::source_catalogs::bootstrap_commitment(initial_catalogs) != catalog_pin
            || consensus_source::bootstrap_commitment(&self.source_consensus.bootstrap)
                != source_config.bootstrap_commitment
        {
            return Err(EconomicError::Invariant);
        }
        // Decode the complete initial checkpoint, rather than accepting a hash
        // of arbitrary bytes as an initial catalog state. Owner codecs replay
        // their own journals and reject noncanonical or incomplete state.
        let initial =
            crate::source_catalogs::SourceCatalogOwners::from_checkpoint_bytes(initial_catalogs)
                .map_err(|_| econ_snapshot_bad())?;
        if initial
            .checkpoint_bytes()
            .map_err(|_| econ_snapshot_bad())?
            != *initial_catalogs
        {
            return Err(econ_snapshot_bad());
        }
        let current_catalogs = catalogs
            .checkpoint_bytes()
            .map_err(|_| econ_snapshot_bad())?;

        // Only this clone loses the outer source layer. Its current economic
        // books, Ledger, native/contract frames and replay markers stay whole.
        let metadata_ids = self.source_metadata_ids()?;
        let mut base = self.clone();
        base.config.source_consensus = None;
        base.source_consensus = SourceConsensusState::default();
        base.replaying_source_history = false;
        if let Some(oracles) = &catalogs.oracles {
            // Only the complete raw-source replay above authorizes this private
            // structural-encoding context. Native/contract typed frames omit
            // actual oracle document mutations and cannot replay those effects
            // independently. Neither context field is serialized or exposed.
            base.catalog_inner_oracle = Some(oracles.clone());
            base.catalog_inner_source_verified = true;
        }
        if base.config.native_entitlements.is_none() && base.config.contract_binding.is_none() {
            // Generic owners have no typed frame history. These identities
            // remain bound to the outer original source audit and are accepted
            // only through the private catalog-wrapper decode context.
            base.catalog_inner_metadata_ids = metadata_ids;
        }
        let owner_bytes = base.snapshot_bytes()?;
        let mut source = EconomicWriter(Vec::new());
        econ_write_source_state(
            &mut source,
            &self.source_consensus,
            &mut SourceBudget::default(),
        )?;

        let mut out = EconomicWriter(ECONOMIC_SOURCE_CATALOG_MAGIC.to_vec());
        out.put(&source_config.bootstrap_commitment)?;
        match source_config.policy {
            SourceConsensusPolicy::ConservingSequentialV1 => out.byte(1)?,
            SourceConsensusPolicy::ConservingSequentialV2 => out.byte(2)?,
        }
        out.put(&catalog_pin)?;
        out.embedded(&owner_bytes)?;
        out.embedded(&source.0)?;
        out.embedded(initial_catalogs)?;
        out.embedded(&current_catalogs)?;
        with_crc(out.0).map_err(|_| EconomicError::Capacity)
    }

    fn from_source_catalog_snapshot_bytes(
        bytes: &[u8],
        expected: &EconomicConfig,
        admin: &AccountId,
        permitted: &BTreeSet<PropertyId>,
    ) -> Result<Self> {
        expected.validate()?;
        if expected.insurance.is_some() {
            return Err(EconomicError::InvalidConfig);
        }
        let expected_source = expected
            .source_consensus
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let expected_catalog_pin = expected_source
            .catalog_bootstrap_commitment
            .ok_or(EconomicError::InvalidConfig)?;
        let raw = verify_crc(bytes).map_err(|_| econ_snapshot_bad())?;
        let mut reader = Cursor::new(raw);
        if reader.fixed::<5>().map_err(|_| econ_snapshot_bad())? != *ECONOMIC_SOURCE_CATALOG_MAGIC {
            return Err(econ_snapshot_bad());
        }
        let joint_pin: [u8; 32] = reader.fixed().map_err(|_| econ_snapshot_bad())?;
        let policy = match reader.u8().map_err(|_| econ_snapshot_bad())? {
            1 => SourceConsensusPolicy::ConservingSequentialV1,
            2 => SourceConsensusPolicy::ConservingSequentialV2,
            _ => return Err(econ_snapshot_bad()),
        };
        let catalog_pin: [u8; 32] = reader.fixed().map_err(|_| econ_snapshot_bad())?;
        if joint_pin != expected_source.bootstrap_commitment
            || policy != expected_source.policy
            || catalog_pin != expected_catalog_pin
        {
            return Err(EconomicError::InvalidConfig);
        }

        let owner_bytes = econ_read_embedded(&mut reader)?;
        let source_bytes = econ_read_embedded(&mut reader)?;
        if source_bytes.len() > MAX_SOURCE_HISTORY_BYTES {
            return Err(EconomicError::Capacity);
        }
        let initial_catalog_bytes = econ_read_embedded(&mut reader)?;
        let current_catalog_bytes = econ_read_embedded(&mut reader)?;
        reader.end().map_err(|_| econ_snapshot_bad())?;

        // Both pins originate in expected deployment configuration. Recomputed
        // CRCs or attacker-chosen catalog/bootstrap bytes cannot replace them.
        if crate::source_catalogs::bootstrap_commitment(initial_catalog_bytes)
            != expected_catalog_pin
        {
            return Err(econ_snapshot_bad());
        }
        let initial = crate::source_catalogs::SourceCatalogOwners::from_checkpoint_bytes(
            initial_catalog_bytes,
        )
        .map_err(|_| econ_snapshot_bad())?;
        let current = crate::source_catalogs::SourceCatalogOwners::from_checkpoint_bytes(
            current_catalog_bytes,
        )
        .map_err(|_| econ_snapshot_bad())?;
        if initial
            .checkpoint_bytes()
            .map_err(|_| econ_snapshot_bad())?
            != initial_catalog_bytes
            || current
                .checkpoint_bytes()
                .map_err(|_| econ_snapshot_bad())?
                != current_catalog_bytes
        {
            return Err(econ_snapshot_bad());
        }

        let mut source_reader = Cursor::new(source_bytes);
        let mut source = econ_read_source_state(&mut source_reader, &mut SourceBudget::default())?;
        source_reader.end().map_err(|_| econ_snapshot_bad())?;
        if consensus_source::bootstrap_commitment(&source.bootstrap) != joint_pin {
            return Err(econ_snapshot_bad());
        }
        source.catalog_bootstrap = initial_catalog_bytes.to_vec();
        source.catalogs = Some(current);

        if initial.oracles.is_some() {
            // The supplied current catalogs and embedded owner are comparison
            // targets only. Build the entire owner independently from BOTH
            // expected bootstrap pins and replay every original source frame.
            let bootstrap = match &expected.native_entitlements {
                Some(policy) => {
                    JointProtocolState::from_economic_snapshot_bytes_with_native_policy(
                        &source.bootstrap,
                        expected.network,
                        admin,
                        permitted,
                        policy,
                    )
                }
                None => JointProtocolState::from_economic_snapshot_bytes(
                    &source.bootstrap,
                    expected.network,
                    admin,
                    permitted,
                ),
            }
            .map_err(|_| econ_snapshot_bad())?;
            if Self::source_bootstrap_commitment(&bootstrap)? != joint_pin {
                return Err(econ_snapshot_bad());
            }
            let mut replay = Self::from_joint_with_source_catalogs_unverified(
                expected.clone(),
                bootstrap,
                initial,
            )?;
            replay.replaying_source_history = true;
            for frame in &source.frames {
                let (next, _) = consensus_source::execute_block_unverified(
                    &replay,
                    frame.header,
                    frame.mode,
                    &frame.transactions,
                )
                .map_err(|_| econ_snapshot_bad())?;
                replay = next;
                if replay.source_consensus.frames.last() != Some(frame) {
                    return Err(econ_snapshot_bad());
                }
            }
            replay.replaying_source_history = false;
            replay.catalog_inner_metadata_ids.clear();
            replay.catalog_inner_oracle = None;
            replay.catalog_inner_source_verified = false;
            if replay.source_consensus != source {
                return Err(econ_snapshot_bad());
            }
            // This re-encodes the actual funded Joint, books, all typed native
            // and contract frames, and current document owner. No supplied
            // verdict, current state, or independently decodable legacy owner
            // can substitute for the raw-frame replay that produced it.
            if replay.source_catalog_snapshot_bytes()?.as_slice() != bytes {
                return Err(econ_snapshot_bad());
            }
            return Ok(replay);
        }

        let mut base_config = expected.clone();
        base_config.source_consensus = None;
        let mut state = if base_config.native_entitlements.is_none()
            && base_config.contract_binding.is_none()
        {
            let metadata_ids = Self::metadata_ids_from_source(&source)?;
            Self::from_snapshot_bytes_mode(
                owner_bytes,
                &base_config,
                admin,
                permitted,
                metadata_ids,
            )?
        } else {
            Self::from_snapshot_bytes(owner_bytes, &base_config, admin, permitted)?
        };
        // Reject nested/noncanonical encodings and retain the complete native
        // owner history. No reconstruction from a reduced Ledger projection.
        if state.snapshot_bytes()?.as_slice() != owner_bytes {
            return Err(econ_snapshot_bad());
        }
        state.catalog_inner_metadata_ids.clear();
        state.config = expected.clone();
        state.source_consensus = source;
        state.replaying_source_history = false;
        // Canonical encoding first invokes full verification: replay every
        // original candidate against the real joint and catalog bootstraps and
        // compare the entire resulting owner, including catalogs, metadata and
        // history, receipts, and native frames.
        if state.source_catalog_snapshot_bytes()?.as_slice() != bytes {
            return Err(econ_snapshot_bad());
        }
        Ok(state)
    }
}
