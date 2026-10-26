// Private sole-ledger custody bridge. Only the outer economic owner can audit
// the exact ~econ: escrows against its contract, margin, IOU and fee metadata.
// This layer provides no sender, chain or economic settlement authentication.

const JOINT_ECONOMIC_MAGIC: &[u8; 5] = b"TLJE\x01";

impl JointProtocolState {
    pub(crate) fn verify_for_economic_owner_unverified(&self) -> Result<(), JointError> {
        self.verify_components_mode_unverified(true)
    }

    fn verify_economic_custody_fence_unverified(&self) -> Result<(), JointError> {
        if !self
            .ledger()
            .has_no_channel_or_settlement_liabilities_unverified()
        {
            return Err(JointError::UnsupportedLegacyLiabilities);
        }
        if self.economic_pending_height.is_some()
            || self.economic_pending_input.is_some()
            || (!self.economic_input_ids.is_empty() && self.finalized_height.is_none())
        {
            return Err(JointError::InconsistentComponents);
        }
        if self.spot.has_clearing_activity_unverified() == self.economic_input_ids.is_empty() {
            return Err(JointError::ReplayConflict);
        }
        if self.economic_input_ids.is_empty()
            && !self
                .ledger()
                .balances_with_account_prefix("~econ:")
                .is_empty()
        {
            return Err(JointError::ReplayConflict);
        }
        if self
            .ledger()
            .balances_with_account_prefix("~")
            .iter()
            .any(|(_, account, _)| {
                !account.as_str().starts_with("~spot:") && !account.as_str().starts_with("~econ:")
            })
        {
            return Err(JointError::UnsupportedLegacyLiabilities);
        }
        Ok(())
    }

    /// Used only on the outer owner's staged block. Economic operations are
    /// recorded separately; ordinary commands retain the exact old dispatch.
    pub(crate) fn apply_one_for_economic_unverified(
        &mut self,
        height: u64,
        transaction: UnverifiedJointTx,
    ) -> Result<(), JointError> {
        self.check_economic_pending_height(height)?;
        self.apply_inner_unverified(height, transaction)?;
        self.economic_pending_height = Some(height);
        Ok(())
    }

    pub(crate) fn finalize_block_for_economic_unverified(
        &mut self,
        height: u64,
    ) -> Result<(), JointError> {
        self.check_economic_pending_height(height)?;
        let mut staged = self.clone();
        staged.economic_pending_height = None;
        staged.economic_pending_input = None;
        staged.finalize_block_mode_unverified(height, true)?;
        *self = staged;
        Ok(())
    }

    /// Exactly the admitted economic event categories in the initial owner:
    /// type18 contract order/trade and type6 owner cancellation. This does not
    /// authorize settlement types23/31, channel transfers, options or insurance.
    pub(crate) fn note_economic_input_unverified(
        &mut self,
        txid: TxId,
        height: u64,
        legacy_type: u8,
    ) -> Result<(), JointError> {
        if !matches!(legacy_type, 18 | 6) {
            return Err(JointError::UnsupportedOperation(legacy_type));
        }
        self.check_economic_pending_height(height)?;
        self.check_new_input_unverified(height, txid)?;
        self.check_activated(legacy_type, height)?;
        self.economic_input_ids.insert(txid);
        self.global_input_ids.insert(txid);
        self.economic_pending_height = Some(height);
        self.economic_pending_input = Some(txid);
        // Even a zero-transfer event must fence old formats permanently.
        self.spot.note_clearing_event_unverified();
        Ok(())
    }

    /// The outer owner derives all addresses and fingerprints and commits its
    /// exact metadata with these movements. External event IDs cannot be reused
    /// as internal ledger-transfer IDs, including registry/spot/economic IDs.
    pub(crate) fn apply_economic_transfer_unverified(
        &mut self,
        internal_id: TxId,
        property: PropertyId,
        from: AccountId,
        to: AccountId,
        amount_atoms: u128,
    ) -> Result<(), JointError> {
        if self.economic_pending_height.is_none() || self.economic_pending_input.is_none() {
            return Err(JointError::InconsistentComponents);
        }
        if self.global_input_ids.contains(&internal_id)
            || self.registry.is_processed(internal_id)
            || self.spot.spot_input_ids_unverified().contains(&internal_id)
            || self.ledger().is_processed(internal_id)
        {
            return Err(JointError::Replay(internal_id));
        }
        self.spot.apply_economic_transfer_unverified(
            internal_id,
            property,
            from,
            to,
            amount_atoms,
        )?;
        Ok(())
    }

    pub(crate) fn economic_input_ids_unverified(&self) -> &BTreeSet<TxId> {
        &self.economic_input_ids
    }

    fn check_economic_pending_height(&self, height: u64) -> Result<(), JointError> {
        if height == 0
            || self
                .economic_pending_height
                .is_some_and(|pending| pending != height)
        {
            return Err(JointError::InvalidHeight);
        }
        if self.finalized_height.is_some_and(|last| height <= last) {
            return Err(JointError::FinalizedHeight);
        }
        Ok(())
    }

    /// Crate-private TLJE cannot stand alone as an economic checkpoint: only
    /// its outer owner verifies exact escrows, positions, liabilities and fees.
    pub(crate) fn snapshot_for_economic_bytes(&self) -> Result<Vec<u8>, JointSnapshotError> {
        self.verify_for_economic_owner_unverified()
            .map_err(JointSnapshotError::Joint)?;
        let mut out = JOINT_ECONOMIC_MAGIC.to_vec();
        out.push(self.network.tag());
        out.extend_from_slice(&self.network.genesis().0);
        out.push(u8::from(self.spot.has_clearing_activity_unverified()));
        economic_write_embedded(
            &mut out,
            &self
                .registry
                .snapshot_bytes()
                .map_err(JointSnapshotError::Registry)?,
        )?;
        economic_write_embedded(
            &mut out,
            &self
                .spot
                .combined_spot_snapshot_bytes_unverified()
                .map_err(JointSnapshotError::Spot)?,
        )?;
        for ids in [
            &self.global_input_ids,
            &self.ledger_input_ids,
            &self.economic_input_ids,
        ] {
            economic_check_append(&out, 4 + ids.len() * 32)?;
            joint_write_replay_ids(&mut out, ids)?;
        }
        economic_check_append(
            &out,
            if self.finalized_height.is_some() {
                9
            } else {
                1
            },
        )?;
        match self.finalized_height {
            None => out.push(0),
            Some(height) if height != 0 => {
                out.push(1);
                out.extend_from_slice(&height.to_be_bytes());
            }
            Some(_) => return Err(joint_bad("zero economic finalized block")),
        }
        with_crc(out).map_err(|_| joint_bad("economic joint snapshot too large"))
    }

    pub(crate) fn from_economic_snapshot_bytes(
        bytes: &[u8],
        expected_network: TradeLayerMainnet,
        expected_admin: &AccountId,
        expected_permitted: &BTreeSet<PropertyId>,
    ) -> Result<Self, JointSnapshotError> {
        let raw = verify_crc(bytes).map_err(|_| joint_bad("economic joint CRC/size"))?;
        let mut reader = Cursor::new(raw);
        if reader
            .fixed::<5>()
            .map_err(|_| joint_bad("economic joint magic"))?
            != *JOINT_ECONOMIC_MAGIC
        {
            return Err(joint_bad("economic joint version"));
        }
        let network = TradeLayerMainnet::from_tag(
            reader.u8().map_err(|_| joint_bad("economic network tag"))?,
        )
        .map_err(|_| joint_bad("unknown economic network"))?;
        let genesis = reader
            .fixed::<32>()
            .map_err(|_| joint_bad("economic genesis"))?;
        if network != expected_network || genesis != expected_network.genesis().0 {
            return Err(joint_bad("economic network/genesis mismatch"));
        }
        let activity = match reader
            .u8()
            .map_err(|_| joint_bad("economic activity flag"))?
        {
            0 => false,
            1 => true,
            _ => return Err(joint_bad("invalid economic activity flag")),
        };
        let registry =
            Registry::from_snapshot_bytes(joint_read_embedded(&mut reader)?, expected_admin)
                .map_err(JointSnapshotError::Registry)?;
        let spot_bytes = joint_read_embedded(&mut reader)?;
        let global_input_ids = joint_read_replay_ids(&mut reader)?;
        let ledger_input_ids = joint_read_replay_ids(&mut reader)?;
        let economic_input_ids = joint_read_replay_ids(&mut reader)?;
        if activity == economic_input_ids.is_empty() {
            return Err(joint_bad("economic activity/replay mismatch"));
        }
        let finalized_height = match reader
            .u8()
            .map_err(|_| joint_bad("economic finalized flag"))?
        {
            0 => None,
            1 => Some(
                reader
                    .u64()
                    .map_err(|_| joint_bad("economic finalized height"))?,
            ),
            _ => return Err(joint_bad("invalid economic finalized flag")),
        };
        reader
            .end()
            .map_err(|_| joint_bad("trailing economic joint bytes"))?;
        let spot = SpotExchange::from_combined_spot_snapshot_bytes_unverified(
            spot_bytes,
            expected_permitted,
            activity,
        )
        .map_err(JointSnapshotError::Spot)?;
        let result = Self {
            network,
            registry,
            spot,
            global_input_ids,
            ledger_input_ids,
            economic_input_ids,
            finalized_height,
            economic_pending_height: None,
            economic_pending_input: None,
        };
        result
            .verify_for_economic_owner_unverified()
            .map_err(JointSnapshotError::Joint)?;
        if result.snapshot_for_economic_bytes()?.as_slice() != bytes {
            return Err(joint_bad("noncanonical economic joint snapshot"));
        }
        Ok(result)
    }
}

fn economic_check_append(out: &[u8], count: usize) -> Result<(), JointSnapshotError> {
    if out
        .len()
        .checked_add(count)
        .is_none_or(|size| size > MAX_CHECKPOINT_BYTES - 4)
    {
        Err(joint_bad("economic joint snapshot exceeds bound"))
    } else {
        Ok(())
    }
}
fn economic_write_embedded(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), JointSnapshotError> {
    economic_check_append(out, 4 + bytes.len())?;
    joint_write_embedded(out, bytes)
}

#[cfg(test)]
mod economic_bridge_tests {
    use super::*;

    fn account(name: &str) -> AccountId {
        name.try_into().unwrap()
    }
    fn property(id: u32) -> PropertyId {
        id.try_into().unwrap()
    }
    fn id(marker: u8) -> TxId {
        TxId::from_bytes([marker; 32])
    }
    fn command(marker: u8, actor: &str, operation: JointOperation) -> UnverifiedJointTx {
        UnverifiedJointTx {
            txid: id(marker),
            actor: account(actor),
            operation,
        }
    }
    fn ready() -> JointProtocolState {
        let mut ledger = Ledger::new();
        ledger
            .apply(
                id(1),
                Operation::CreateProperty {
                    id: property(7),
                    issuer: account("alice"),
                    kind: PropertyKind::Fixed,
                    initial_atoms: 1000,
                },
            )
            .unwrap();
        let mut state = JointProtocolState::from_bootstrap_unverified(
            TradeLayerMainnet::Bitcoin,
            account("admin"),
            ledger,
            BTreeSet::from([property(7)]),
        )
        .unwrap();
        state
            .apply_one_for_economic_unverified(
                1,
                command(
                    10,
                    "admin",
                    JointOperation::Registry(RegistryAction::Activate {
                        types: vec![1, 2, 5, 6, 18],
                        code_hash: [0x51; 32],
                    }),
                ),
            )
            .unwrap();
        state.finalize_block_for_economic_unverified(1).unwrap();
        state
    }
    fn decode(bytes: &[u8]) -> Result<JointProtocolState, JointSnapshotError> {
        JointProtocolState::from_economic_snapshot_bytes(
            bytes,
            TradeLayerMainnet::Bitcoin,
            &account("admin"),
            &BTreeSet::from([property(7)]),
        )
    }

    #[test]
    fn economic_bridge_transfers_and_ordinary_commands_share_one_ledger() {
        let mut state = ready();
        state.note_economic_input_unverified(id(20), 2, 18).unwrap();
        state
            .apply_economic_transfer_unverified(
                id(21),
                property(7),
                account("alice"),
                account("~econ:test"),
                400,
            )
            .unwrap();
        state
            .apply_one_for_economic_unverified(
                2,
                command(
                    22,
                    "alice",
                    JointOperation::Send {
                        id: property(7),
                        recipient: account("bob"),
                        amount_atoms: 550,
                    },
                ),
            )
            .unwrap();
        assert_eq!(
            state
                .ledger()
                .available_atoms(property(7), &account("alice")),
            50
        );
        assert_eq!(
            state.ledger().available_atoms(property(7), &account("bob")),
            550
        );
        assert_eq!(
            state
                .ledger()
                .available_atoms(property(7), &account("~econ:test")),
            400
        );
        assert_eq!(
            state.economic_input_ids_unverified(),
            &BTreeSet::from([id(20)])
        );
        assert!(!state.global_input_ids.contains(&id(21)));
        assert!(state.ledger().is_processed(id(21)));
        state.finalize_block_for_economic_unverified(2).unwrap();
        state.verify_for_economic_owner_unverified().unwrap();
        let bytes = state.snapshot_for_economic_bytes().unwrap();
        assert_eq!(decode(&bytes).unwrap(), state);
        assert!(state.verify_components_unverified().is_err());
        assert!(state.snapshot_bytes().is_err());
        assert!(state.spot().snapshot_bytes().is_err());
        assert!(JointProtocolState::from_snapshot_bytes(
            &bytes,
            state.network(),
            &account("admin"),
            state.spot().permitted_properties()
        )
        .is_err());
    }

    #[test]
    fn economic_bridge_zero_leg_history_fences_old_formats_permanently() {
        let mut state = ready();
        state.note_economic_input_unverified(id(20), 2, 6).unwrap();
        assert!(state.snapshot_bytes().is_err());
        assert!(state.snapshot_for_economic_bytes().is_err());
        state.finalize_block_for_economic_unverified(2).unwrap();
        let restored = decode(&state.snapshot_for_economic_bytes().unwrap()).unwrap();
        assert!(restored
            .ledger()
            .balances_with_account_prefix("~econ:")
            .is_empty());
        assert!(restored.snapshot_bytes().is_err());
        assert!(restored.spot().snapshot_bytes().is_err());
        assert_eq!(
            restored.economic_input_ids_unverified(),
            &BTreeSet::from([id(20)])
        );
    }

    #[test]
    fn economic_bridge_requires_current_block_registration_and_one_height() {
        let mut state = ready();
        let transfer = |state: &mut JointProtocolState| {
            state.apply_economic_transfer_unverified(
                id(30),
                property(7),
                account("alice"),
                account("~econ:test"),
                1,
            )
        };
        assert_eq!(
            transfer(&mut state),
            Err(JointError::InconsistentComponents)
        );
        state.note_economic_input_unverified(id(20), 2, 18).unwrap();
        let pending = state.clone();
        assert_eq!(
            state.finalize_block_for_economic_unverified(3),
            Err(JointError::InvalidHeight)
        );
        assert_eq!(state, pending);
        assert_eq!(
            state.note_economic_input_unverified(id(21), 3, 18),
            Err(JointError::InvalidHeight)
        );
        assert!(state.snapshot_for_economic_bytes().is_err());
        state.finalize_block_for_economic_unverified(2).unwrap();
        state
            .apply_one_for_economic_unverified(
                3,
                command(
                    22,
                    "alice",
                    JointOperation::Send {
                        id: property(7),
                        recipient: account("bob"),
                        amount_atoms: 1,
                    },
                ),
            )
            .unwrap();
        assert_eq!(
            transfer(&mut state),
            Err(JointError::InconsistentComponents)
        );
        assert_eq!(
            state.note_economic_input_unverified(id(23), 2, 18),
            Err(JointError::InvalidHeight)
        );
        assert!(state.snapshot_for_economic_bytes().is_err());
        state.note_economic_input_unverified(id(24), 3, 18).unwrap();
        transfer(&mut state).unwrap();
        state.finalize_block_for_economic_unverified(3).unwrap();
        state.finalize_block_for_economic_unverified(4).unwrap();
        assert_eq!(state.finalized_height(), Some(4));
        assert_eq!(
            decode(&state.snapshot_for_economic_bytes().unwrap()).unwrap(),
            state
        );
    }

    #[test]
    fn economic_bridge_activation_and_unsupported_authority_remain_closed() {
        let mut state = JointProtocolState::from_bootstrap_unverified(
            TradeLayerMainnet::Bitcoin,
            account("admin"),
            Ledger::new(),
            BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(
            state.note_economic_input_unverified(id(20), 1, 18),
            Err(JointError::NoActivation(18))
        );
        state
            .apply_one_for_economic_unverified(
                1,
                command(
                    10,
                    "admin",
                    JointOperation::Registry(RegistryAction::Activate {
                        types: vec![6, 18],
                        code_hash: [0; 32],
                    }),
                ),
            )
            .unwrap();
        assert_eq!(
            state.note_economic_input_unverified(id(20), 1, 18),
            Err(JointError::ActivatedInSameBlock(18))
        );
        state.finalize_block_for_economic_unverified(1).unwrap();
        for ty in [0, 19, 23, 31, 35, 255] {
            assert_eq!(
                state.note_economic_input_unverified(id(20), 2, ty),
                Err(JointError::UnsupportedOperation(ty))
            );
        }
        for ty in [23, 31] {
            assert_eq!(
                state.apply_one_for_economic_unverified(
                    2,
                    command(20, "alice", JointOperation::EconomicSettlement { ty })
                ),
                Err(JointError::SettlementAuthorizationRequired)
            );
        }
        assert_eq!(
            state.apply_one_for_economic_unverified(
                2,
                command(20, "alice", JointOperation::UtxoTradeType3)
            ),
            Err(JointError::UtxoAuthenticationRequired)
        );
        assert_eq!(
            state.note_economic_input_unverified(id(20), 1, 18),
            Err(JointError::FinalizedHeight)
        );
        assert_eq!(
            state.note_economic_input_unverified(id(20), 0, 18),
            Err(JointError::InvalidHeight)
        );
    }

    #[test]
    fn economic_bridge_replay_and_reserved_custody_fail_without_mutation() {
        let mut state = ready();
        for collision in [1, 10] {
            assert_eq!(
                state.note_economic_input_unverified(id(collision), 2, 18),
                Err(JointError::Replay(id(collision)))
            );
        }
        state.note_economic_input_unverified(id(20), 2, 18).unwrap();
        for collision in [1, 10, 20] {
            let before = state.clone();
            assert_eq!(
                state.apply_economic_transfer_unverified(
                    id(collision),
                    property(7),
                    account("alice"),
                    account("~econ:test"),
                    1
                ),
                Err(JointError::Replay(id(collision)))
            );
            assert_eq!(state, before);
        }
        for address in ["~spot:x", "~clrm:x", "~ins:x", "~other:x", "bob"] {
            let before = state.clone();
            assert!(state
                .apply_economic_transfer_unverified(
                    id(21),
                    property(7),
                    account("alice"),
                    account(address),
                    1
                )
                .is_err());
            assert_eq!(state, before);
        }
        state
            .apply_economic_transfer_unverified(
                id(21),
                property(7),
                account("alice"),
                account("~econ:test"),
                1,
            )
            .unwrap();
        assert_eq!(
            state.note_economic_input_unverified(id(21), 2, 18),
            Err(JointError::Replay(id(21)))
        );
        assert_eq!(
            state.apply_one_for_economic_unverified(
                2,
                command(
                    21,
                    "alice",
                    JointOperation::Send {
                        id: property(7),
                        recipient: account("bob"),
                        amount_atoms: 1,
                    }
                )
            ),
            Err(JointError::Replay(id(21)))
        );
        assert_eq!(
            state.note_economic_input_unverified(id(20), 2, 6),
            Err(JointError::Replay(id(20)))
        );
    }

    // Exercise the real combined Spot decoder with a freshly checksummed
    // ledger, rather than trusting CRC failure to reject structural liabilities.
    fn with_replaced_ledger(state: &JointProtocolState, ledger: &Ledger) -> JointProtocolState {
        let bytes = state
            .spot
            .combined_spot_snapshot_bytes_unverified()
            .unwrap();
        let raw = verify_crc(&bytes).unwrap();
        let policy_count = u32::from_be_bytes(raw[5..9].try_into().unwrap()) as usize;
        let size_offset = 9 + policy_count * 4;
        let previous_size =
            u32::from_be_bytes(raw[size_offset..size_offset + 4].try_into().unwrap()) as usize;
        let new_ledger = ledger.snapshot_bytes().unwrap();
        let mut replacement = raw[..size_offset].to_vec();
        replacement.extend_from_slice(&(new_ledger.len() as u32).to_be_bytes());
        replacement.extend_from_slice(&new_ledger);
        replacement.extend_from_slice(&raw[size_offset + 4 + previous_size..]);
        let mut result = state.clone();
        result.spot = SpotExchange::from_combined_spot_snapshot_bytes_unverified(
            &with_crc(replacement).unwrap(),
            state.spot.permitted_properties(),
            state.spot.has_clearing_activity_unverified(),
        )
        .unwrap();
        result
    }

    #[test]
    fn economic_bridge_checks_reserved_families_and_unowned_channel_liabilities() {
        let state = ready();
        for reserved in [
            "~econ:orphan",
            "~evil:orphan",
            "~clrm:orphan",
            "~ins:orphan",
        ] {
            let mut ledger = state.ledger().clone();
            ledger
                .apply(
                    id(30),
                    Operation::Transfer {
                        id: property(7),
                        from: account("alice"),
                        to: account(reserved),
                        amount_atoms: 1,
                    },
                )
                .unwrap();
            let bad = with_replaced_ledger(&state, &ledger);
            assert!(bad.verify_for_economic_owner_unverified().is_err());
            assert!(bad.snapshot_for_economic_bytes().is_err());
        }
        let mut ledger = state.ledger().clone();
        ledger
            .apply(
                id(30),
                Operation::ChannelCommit {
                    id: property(7),
                    owner: account("alice"),
                    channel: account("channel"),
                    side: crate::ledger::ChannelSide::A,
                    amount_atoms: 1,
                },
            )
            .unwrap();
        let bad = with_replaced_ledger(&state, &ledger);
        assert_eq!(
            bad.verify_for_economic_owner_unverified(),
            Err(JointError::UnsupportedLegacyLiabilities)
        );
    }

    #[test]
    fn economic_bridge_rejects_rechecksummed_noncanonical_replay_and_policy() {
        let mut state = ready();
        state.note_economic_input_unverified(id(20), 2, 18).unwrap();
        state.note_economic_input_unverified(id(21), 2, 6).unwrap();
        state.finalize_block_for_economic_unverified(2).unwrap();
        let bytes = state.snapshot_for_economic_bytes().unwrap();
        for offset in [0, 4, 5, 6, 38] {
            let mut raw = verify_crc(&bytes).unwrap().to_vec();
            raw[offset] ^= 0x80;
            assert!(decode(&with_crc(raw).unwrap()).is_err());
        }
        for length in 0..bytes.len() {
            assert!(decode(&bytes[..length]).is_err());
        }
        let mut cursor = Cursor::new(verify_crc(&bytes).unwrap());
        cursor.take(39).unwrap();
        let registry_size = joint_read_embedded(&mut cursor).unwrap().len();
        let spot_size = joint_read_embedded(&mut cursor).unwrap().len();
        let first_count = 39 + 4 + registry_size + 4 + spot_size;
        let global_count =
            u32::from_be_bytes(bytes[first_count..first_count + 4].try_into().unwrap()) as usize;
        let econ_count = first_count + 4 + global_count * 32 + 4;
        assert_eq!(
            u32::from_be_bytes(bytes[econ_count..econ_count + 4].try_into().unwrap()),
            2
        );
        let mut raw = verify_crc(&bytes).unwrap().to_vec();
        let first = raw[econ_count + 4..econ_count + 36].to_vec();
        raw[econ_count + 36..econ_count + 68].copy_from_slice(&first);
        assert!(decode(&with_crc(raw).unwrap()).is_err());
        let mut raw = verify_crc(&bytes).unwrap().to_vec();
        raw[first_count..first_count + 4].copy_from_slice(&100_001u32.to_be_bytes());
        assert!(decode(&with_crc(raw).unwrap()).is_err());
        assert!(JointProtocolState::from_economic_snapshot_bytes(
            &bytes,
            TradeLayerMainnet::Litecoin,
            &account("admin"),
            &BTreeSet::from([property(7)])
        )
        .is_err());
        assert!(JointProtocolState::from_economic_snapshot_bytes(
            &bytes,
            state.network(),
            &account("other-admin"),
            &BTreeSet::from([property(7)])
        )
        .is_err());
        assert!(JointProtocolState::from_economic_snapshot_bytes(
            &bytes,
            state.network(),
            &account("admin"),
            &BTreeSet::new()
        )
        .is_err());
        state.economic_input_ids.insert(id(1));
        state.global_input_ids.insert(id(1));
        assert_eq!(
            state.verify_for_economic_owner_unverified(),
            Err(JointError::ReplayConflict)
        );
    }
}
