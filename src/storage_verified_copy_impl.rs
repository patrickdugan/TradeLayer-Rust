// Copy/digest through the SAME owned file handle. A second handle cannot read
// the locked journal on Windows, even in this process. Cursor is restored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActualJournalFilePinV1 {
    pub bytes: u64,
    pub chunks: u64,
    pub chain_sha256d: [u8; 32],
}
impl Journal {
    pub fn copy_verified_current_file(
        &mut self,
        expected: &VerifiedJournalPrefixV1,
        destination: Option<&Path>,
    ) -> Result<ActualJournalFilePinV1, StorageError> {
        self.check_health()?;
        if self.verified_prefix_pin()? != *expected {
            return Err(StorageError::Corrupt("verified current copy expectation"));
        }
        self.read_verified_history_prefix(expected)?;
        let cursor = self.file.stream_position()?;
        let copied = (|| {
            let length = self.file.metadata()?.len();
            if length != self.end || length > MAX_LOG_BYTES {
                return Err(StorageError::Corrupt("verified current copy length"));
            }
            let mut output = destination
                .map(|path| OpenOptions::new().write(true).create_new(true).open(path))
                .transpose()?;
            self.file.seek(SeekFrom::Start(0))?;
            let mut pin = ActualJournalFilePinV1 {
                bytes: 0,
                chunks: 0,
                chain_sha256d: crate::hash256::double_sha256(b"TL-ESCROW-FILE-CHUNKS-1"),
            };
            let mut buffer = vec![0; 64 * 1024];
            while pin.bytes < length {
                let take = usize::try_from((length - pin.bytes).min(buffer.len() as u64))
                    .map_err(|_| StorageError::LimitExceeded)?;
                self.file.read_exact(&mut buffer[..take])?;
                pin.chain_sha256d = crate::hash256::double_sha256_parts(&[
                    &pin.chain_sha256d,
                    &pin.chunks.to_be_bytes(),
                    &(take as u32).to_be_bytes(),
                    &buffer[..take],
                ]);
                if let Some(file) = &mut output {
                    file.write_all(&buffer[..take])?;
                }
                pin.bytes += take as u64;
                pin.chunks += 1;
            }
            if self.file.metadata()?.len() != length {
                return Err(StorageError::Corrupt("verified copy source changed"));
            }
            if let Some(file) = output {
                file.sync_all()?;
            }
            Ok(pin)
        })();
        if let Err(error) = self.file.seek(SeekFrom::Start(cursor)) {
            self.poisoned = true;
            return Err(error.into());
        }
        copied
    }
}
