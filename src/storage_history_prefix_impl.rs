// A cold, read-only history view derived from actual frames under the SAME
// current owner's exclusive file lock. No archive-supplied KV values enter it.
impl Journal {
    pub fn read_verified_history_prefix(
        &mut self,
        expected: &VerifiedJournalPrefixV1,
    ) -> Result<JournalReadSnapshot, StorageError> {
        self.check_health()?;
        if expected.profile != VERIFIED_PREFIX_OPEN_PROFILE
            || expected.namespace != self.namespace
            || expected.canonical_file != self.path.canonicalize()?
            || expected.physical_sequence > self.sequence
            || expected.content.profile != COMPLETE_KV_RECEIPT_PROFILE
            || !expected.content.complete
        {
            return Err(StorageError::Corrupt(
                "invalid independently pinned history prefix",
            ));
        }
        // &mut self serializes cursor use with all readers/writers; restore it
        // before returning either a snapshot or a refusal. Never truncate/sync.
        let cursor = self.file.stream_position()?;
        let result = self.read_history_frames(expected);
        if let Err(error) = self.file.seek(SeekFrom::Start(cursor)) {
            self.poisoned = true;
            return Err(error.into());
        }
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                // A disk/current-owner inconsistency was actually observed.
                // Retain the lock but refuse subsequent stale cached access.
                self.poisoned = true;
                return Err(error);
            }
        };
        if snapshot.complete_kv_receipt()? != expected.content {
            return Err(StorageError::Corrupt(
                "unexpected independently pinned historical KV",
            ));
        }
        Ok(snapshot)
    }
    fn read_history_frames(
        &mut self,
        expected: &VerifiedJournalPrefixV1,
    ) -> Result<JournalReadSnapshot, StorageError> {
        self.read_history_frames_with_visit(expected, |_, _, _| {})
    }
    fn read_history_frames_with_visit(
        &mut self,
        expected: &VerifiedJournalPrefixV1,
        mut visit: impl FnMut(u64, &[u8], &[Mutation]),
    ) -> Result<JournalReadSnapshot, StorageError> {
        let length = self.file.metadata()?.len();
        if length != self.end || length > MAX_LOG_BYTES {
            return Err(StorageError::Corrupt(
                "history file differs from healthy current prefix",
            ));
        }
        self.file.seek(SeekFrom::Start(0))?;
        let expected_header = self.namespace.header();
        let mut header = vec![0; expected_header.len()];
        self.file.read_exact(&mut header)?;
        if header != expected_header {
            return Err(StorageError::WrongNamespace);
        }
        let mut values = BTreeMap::new();
        let mut sequence = 0u64;
        let mut previous_crc = crc32(&header);
        let mut end = header.len() as u64;
        let mut selected = (expected.physical_sequence == 0).then(|| values.clone());
        // Check the entire healthy current log, including the suffix after the
        // selected historical point, without installing it as a mutable owner.
        while end < length {
            let remaining = length - end;
            if remaining < 24 {
                return Err(StorageError::IncompleteTail { offset: end });
            }
            let mut header = [0u8; 24];
            self.file.read_exact(&mut header)?;
            if &header[..4] != b"TXN1" || crc32(&header[..20]) != u32_at(&header, 20) {
                return Err(StorageError::Corrupt("history batch header"));
            }
            let payload_len = u32_at(&header, 4) as usize;
            if !(4..=MAX_BATCH_BYTES).contains(&payload_len) {
                return Err(StorageError::LimitExceeded);
            }
            let next_sequence = u64::from_be_bytes(header[8..16].try_into().unwrap());
            if sequence.checked_add(1) != Some(next_sequence) || u32_at(&header, 16) != previous_crc
            {
                return Err(StorageError::Corrupt("history batch sequence or CRC chain"));
            }
            let frame_len = payload_len as u64 + 32;
            if remaining < frame_len {
                return Err(StorageError::IncompleteTail { offset: end });
            }
            let mut frame = header.to_vec();
            frame.resize(24 + payload_len + 8, 0);
            self.file.read_exact(&mut frame[24..])?;
            let checksum = u32_at(&frame, 24 + payload_len);
            if &frame[28 + payload_len..] != b"DONE"
                || crc32(&frame[..24 + payload_len]) != checksum
            {
                return Err(StorageError::Corrupt(
                    "history batch checksum or commit footer",
                ));
            }
            let mutations = decode(&frame[24..24 + payload_len])?;
            values = apply(&values, &mutations)?;
            visit(next_sequence, &frame, &mutations);
            sequence = next_sequence;
            previous_crc = checksum;
            end += frame_len;
            if sequence == expected.physical_sequence {
                selected = Some(values.clone());
            }
        }
        if sequence != self.sequence
            || previous_crc != self.previous_crc
            || end != self.end
            || values != self.values
        {
            return Err(StorageError::Corrupt(
                "history current suffix differs from actual owner",
            ));
        }
        let snapshot = JournalReadSnapshot {
            namespace: self.namespace.clone(),
            sequence: expected.physical_sequence,
            values: selected.ok_or(StorageError::Corrupt("history point absent"))?,
        };
        Ok(snapshot)
    }
}
