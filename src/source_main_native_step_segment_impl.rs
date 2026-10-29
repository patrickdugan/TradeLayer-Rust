// Included beside the actual step controller; only proof records are rotated.
pub const NATIVE_STEP_SEGMENT_PROFILE: &str = "TL-MAIN-NATIVE-STEP-SEGMENT-13-1";
pub const MAX_NATIVE_STEP_SEGMENT_COMMANDS: usize = 256;
pub const MAX_NATIVE_STEP_SEGMENT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_NATIVE_STEP_SEGMENTS: u64 = 65_536;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeStepHistoryAdmissionV1 {
    Ready,
    PublishRetained {
        retained_commands: usize,
        encoded_bytes: u64,
    },
    SingleRecordTooLarge {
        sequence: u64,
        encoded_bytes: u64,
        maximum_bytes: u64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeStepHistoryHeadV1 {
    pub segments: u64,
    pub source: NativeStepHeadV1,
    pub last_segment_sha256: [u8; 32],
    pub commitment: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeStepHistoryPublicationV1 {
    pub head: NativeStepHistoryHeadV1,
    pub physical: NativePhysicalBinding,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeStepHistoryExpectationV1 {
    /// Last complete published segment, explicitly separate from current owner.
    pub sealed: NativeStepHistoryPublicationV1,
    pub current: NativeStepExpectationV1,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeStepHistoryBootstrapV1 {
    pub context: String,
    pub profile: NativeStepProfileV1,
    pub generation: u64,
    pub archive: Vec<u8>,
    pub archive_pin: NativeStepCheckpointPinV1,
    pub publication: NativeStepHistoryPublicationV1,
}
pub struct NativeStepPreparedSegmentV1 {
    bytes: Vec<u8>,
    publication: NativeStepHistoryPublicationV1,
    commands: usize,
}
impl NativeStepPreparedSegmentV1 {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn publication(&self) -> &NativeStepHistoryPublicationV1 {
        &self.publication
    }
    pub fn commands(&self) -> usize {
        self.commands
    }
}
pub enum NativeStepHistoryRejectionOwnerV1 {
    Origin(Box<SourceCompactIndex>),
    Bootstrap(Box<RejectedNativeStepReexecutionV1>),
    Steps(Box<SourceMainNativeStepSessionV1>),
}
pub struct RejectedNativeStepHistoryV1 {
    pub error: NativeStepSessionError,
    pub owner: NativeStepHistoryRejectionOwnerV1,
}
impl core::fmt::Debug for RejectedNativeStepHistoryV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeStepHistoryV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
/// No resource is cloneable and the mutable financial/index owners never escape.
pub struct NativeStepHistoryWriterV1 {
    session: SourceMainNativeStepSessionV1,
    bootstrap: NativeStepHistoryBootstrapV1,
    sealed: NativeStepHistoryPublicationV1,
}
impl core::fmt::Debug for NativeStepHistoryWriterV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NativeStepHistoryWriterV1")
            .field("sealed", &self.sealed)
            .field("actual_head", &self.session.head())
            .finish_non_exhaustive()
    }
}
pub struct ReexecutedNativeStepHistoryV1 {
    session: SourceMainNativeStepSessionV1,
    verified: NativeStepHistoryPublicationV1,
}
impl core::fmt::Debug for ReexecutedNativeStepHistoryV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ReexecutedNativeStepHistoryV1")
            .field("verified", &self.verified)
            .finish_non_exhaustive()
    }
}
impl ReexecutedNativeStepHistoryV1 {
    pub fn session(&self) -> &SourceMainNativeStepSessionV1 {
        &self.session
    }
    pub fn verified_publication(&self) -> &NativeStepHistoryPublicationV1 {
        &self.verified
    }
    /// The returned owner's actual physical binding can differ on fresh replay.
    /// It cannot publish a partial ordinary checkpoint after proof rotation.
    pub fn into_session(self) -> SourceMainNativeStepSessionV1 {
        self.session
    }
}
fn native_history_error() -> NativeStepSessionError {
    NativeStepSessionError::Backend("invalid Native13 segmented history".into())
}
fn native_history_head_bytes(out: &mut Vec<u8>, head: &NativeStepHistoryHeadV1) {
    out.extend_from_slice(&head.segments.to_be_bytes());
    out.extend_from_slice(&head.source.commands.to_be_bytes());
    out.extend_from_slice(&head.source.commitment);
    out.extend_from_slice(&head.last_segment_sha256);
    out.extend_from_slice(&head.commitment);
}
fn native_history_read_head(
    cursor: &mut crate::codec::Cursor<'_>,
) -> Result<NativeStepHistoryHeadV1, NativeStepSessionError> {
    Ok(NativeStepHistoryHeadV1 {
        segments: cursor.u64().map_err(|_| native_history_error())?,
        source: NativeStepHeadV1 {
            commands: cursor.u64().map_err(|_| native_history_error())?,
            commitment: cursor.fixed().map_err(|_| native_history_error())?,
        },
        last_segment_sha256: cursor.fixed().map_err(|_| native_history_error())?,
        commitment: cursor.fixed().map_err(|_| native_history_error())?,
    })
}
fn native_history_initial(
    context: &str,
    profile: NativeStepProfileV1,
    generation: u64,
    source: NativeStepHeadV1,
) -> Result<NativeStepHistoryHeadV1, NativeStepSessionError> {
    if context.len() > 256 {
        return Err(NativeStepSessionError::Capacity);
    }
    let mut bytes = NATIVE_STEP_SEGMENT_PROFILE.as_bytes().to_vec();
    for value in [s(context), s(profile.identifier())] {
        frame(&mut bytes, &value).map_err(|_| native_history_error())?;
    }
    bytes.extend_from_slice(&generation.to_be_bytes());
    bytes.extend_from_slice(&source.commands.to_be_bytes());
    bytes.extend_from_slice(&source.commitment);
    Ok(NativeStepHistoryHeadV1 {
        segments: 0,
        source,
        last_segment_sha256: [0; 32],
        commitment: sha256(&bytes),
    })
}
fn native_history_after(
    previous: &NativeStepHistoryHeadV1,
    source: NativeStepHeadV1,
    digest: [u8; 32],
) -> Result<NativeStepHistoryHeadV1, NativeStepSessionError> {
    let segments = previous
        .segments
        .checked_add(1)
        .ok_or(NativeStepSessionError::Capacity)?;
    if segments > MAX_NATIVE_STEP_SEGMENTS || source.commands <= previous.source.commands {
        return Err(NativeStepSessionError::Capacity);
    }
    let mut bytes = NATIVE_STEP_SEGMENT_PROFILE.as_bytes().to_vec();
    bytes.extend_from_slice(&previous.commitment);
    bytes.extend_from_slice(&segments.to_be_bytes());
    bytes.extend_from_slice(&source.commands.to_be_bytes());
    bytes.extend_from_slice(&source.commitment);
    Ok(NativeStepHistoryHeadV1 {
        segments,
        source,
        last_segment_sha256: digest,
        commitment: sha256(&bytes),
    })
}
// Called only after a successful sink or an exact actual replay comparison.
// It clears proof records only, never the owner, kernel, global head or flags.
fn native_history_release(session: &mut SourceMainNativeStepSessionV1) {
    session.native_history_release_proof();
}
fn native_history_record_size(
    session: &SourceMainNativeStepSessionV1,
    receipt: &NativeStepReceiptV1,
) -> Result<usize, NativeStepSessionError> {
    let sequence =
        usize::try_from(receipt.sequence).map_err(|_| NativeStepSessionError::Capacity)?;
    let command = session
        .command_field_frames(sequence)
        .ok_or_else(native_history_error)?;
    let mut size = 64usize; // actual financial commitment and source head
    for raw in [
        command,
        receipt.result_frames.as_slice(),
        receipt.kernel_frames.as_slice(),
        receipt.native_frames.as_slice(),
    ] {
        size = size
            .checked_add(4)
            .and_then(|v| v.checked_add(raw.len()))
            .ok_or(NativeStepSessionError::Capacity)?;
    }
    let after = receipt
        .after
        .as_ref()
        .ok_or(NativeStepSessionError::RecoveryRequired)?;
    let physical = physical_value(after)
        .diagnostic_json()
        .map_err(NativeStepSessionError::Input)?;
    size.checked_add(4)
        .and_then(|v| v.checked_add(physical.len()))
        .ok_or(NativeStepSessionError::Capacity)
}
impl NativeStepHistoryWriterV1 {
    pub fn adopt(
        mut session: SourceMainNativeStepSessionV1,
        expected: &NativeStepExpectationV1,
        context: &str,
        mut sink: impl FnMut(&NativeStepHistoryBootstrapV1) -> Result<(), String>,
    ) -> Result<Self, Box<RejectedNativeStepHistoryV1>> {
        let prepared = (|| {
            if session.expectation()? != *expected {
                return Err(NativeStepSessionError::WrongHead);
            }
            let (archive, archive_pin) = session.checkpoint()?;
            let head = native_history_initial(
                context,
                session.profile(),
                session.initial_generation(),
                session.head()?,
            )?;
            let publication = NativeStepHistoryPublicationV1 {
                head,
                physical: expected.physical.clone(),
            };
            Ok(NativeStepHistoryBootstrapV1 {
                context: context.into(),
                profile: session.profile(),
                generation: session.initial_generation(),
                archive,
                archive_pin,
                publication,
            })
        })();
        let bootstrap = match prepared {
            Ok(v) => v,
            Err(error) => {
                return Err(Box::new(RejectedNativeStepHistoryV1 {
                    error,
                    owner: NativeStepHistoryRejectionOwnerV1::Steps(Box::new(session)),
                }))
            }
        };
        if let Err(error) = sink(&bootstrap) {
            return Err(Box::new(RejectedNativeStepHistoryV1 {
                error: NativeStepSessionError::PublicationBlocked(error),
                owner: NativeStepHistoryRejectionOwnerV1::Steps(Box::new(session)),
            }));
        }
        native_history_release(&mut session);
        Ok(Self {
            session,
            sealed: bootstrap.publication.clone(),
            bootstrap,
        })
    }
    pub fn bootstrap(&self) -> &NativeStepHistoryBootstrapV1 {
        &self.bootstrap
    }
    pub fn session(&self) -> &SourceMainNativeStepSessionV1 {
        &self.session
    }
    pub fn expectation(&self) -> Result<NativeStepHistoryExpectationV1, NativeStepSessionError> {
        Ok(NativeStepHistoryExpectationV1 {
            sealed: self.sealed.clone(),
            current: self.session.expectation()?,
        })
    }
    pub fn apply_step(
        &mut self,
        expected: &NativeStepHistoryExpectationV1,
        command: MainStepCommandV1,
    ) -> Result<&NativeStepReceiptV1, NativeStepSessionError> {
        if self.session.native_history_is_exited() {
            return Err(NativeStepSessionError::Exited);
        }
        if self.expectation()? != *expected {
            return Err(NativeStepSessionError::WrongHead);
        }
        if !matches!(&command, MainStepCommandV1::Shutdown(_))
            && !matches!(self.admission()?, NativeStepHistoryAdmissionV1::Ready)
        {
            return Err(NativeStepSessionError::Capacity);
        }
        self.session.apply_step(&expected.current, command)
    }
    pub fn publish_pending(&mut self) -> Result<&NativeStepReceiptV1, NativeStepSessionError> {
        self.session.publish_pending()
    }
    pub fn raise_publication_limit(&mut self, bytes: usize) -> Result<(), NativeStepSessionError> {
        self.session.raise_publication_limit(bytes)
    }
    pub fn publish_segment(
        &mut self,
        expected: &NativeStepHistoryExpectationV1,
        mut sink: impl FnMut(&NativeStepPreparedSegmentV1) -> Result<(), String>,
    ) -> Result<NativeStepHistoryPublicationV1, NativeStepSessionError> {
        if self.expectation()? != *expected {
            return Err(NativeStepSessionError::WrongHead);
        }
        let segment = self.prepare_segment()?;
        sink(&segment).map_err(NativeStepSessionError::PublicationBlocked)?;
        self.session
            .native_history_release_prefix_proof(segment.commands)?;
        self.sealed = segment.publication;
        Ok(self.sealed.clone())
    }
    fn segment_header(&self) -> Result<(Vec<u8>, usize), NativeStepSessionError> {
        let mut bytes = b"TLCR\x0d".to_vec();
        let framed =
            |out: &mut Vec<u8>, value: &V| frame(out, value).map_err(|_| native_history_error());
        framed(&mut bytes, &s(NATIVE_STEP_SEGMENT_PROFILE))?;
        framed(&mut bytes, &s(&self.bootstrap.context))?;
        framed(&mut bytes, &s(self.bootstrap.profile.identifier()))?;
        bytes.extend_from_slice(&self.bootstrap.generation.to_be_bytes());
        native_history_head_bytes(&mut bytes, &self.sealed.head);
        framed(&mut bytes, &physical_value(&self.sealed.physical))?;
        let count_at = bytes.len();
        bytes.extend_from_slice(&0u32.to_be_bytes());
        Ok((bytes, count_at))
    }
    pub fn admission(&self) -> Result<NativeStepHistoryAdmissionV1, NativeStepSessionError> {
        self.session.head()?;
        let (header, _) = self.segment_header()?;
        let mut total = header
            .len()
            .checked_add(4)
            .ok_or(NativeStepSessionError::Capacity)?;
        let mut first = None;
        let mut count = 0usize;
        for receipt in self.session.receipts() {
            let size = native_history_record_size(&self.session, receipt)?;
            if first.is_none() {
                first = Some((
                    receipt.sequence,
                    total
                        .checked_add(size)
                        .ok_or(NativeStepSessionError::Capacity)?,
                ));
            }
            total = total
                .checked_add(size)
                .ok_or(NativeStepSessionError::Capacity)?;
            count = count
                .checked_add(1)
                .ok_or(NativeStepSessionError::Capacity)?;
        }
        if let Some((sequence, bytes)) = first {
            if bytes > MAX_NATIVE_STEP_SEGMENT_BYTES {
                return Ok(NativeStepHistoryAdmissionV1::SingleRecordTooLarge {
                    sequence,
                    encoded_bytes: bytes as u64,
                    maximum_bytes: MAX_NATIVE_STEP_SEGMENT_BYTES as u64,
                });
            }
        }
        if count >= MAX_NATIVE_STEP_SEGMENT_COMMANDS || total > MAX_NATIVE_STEP_SEGMENT_BYTES {
            return Ok(NativeStepHistoryAdmissionV1::PublishRetained {
                retained_commands: count,
                encoded_bytes: total as u64,
            });
        }
        Ok(NativeStepHistoryAdmissionV1::Ready)
    }
    fn prepare_segment(&self) -> Result<NativeStepPreparedSegmentV1, NativeStepSessionError> {
        if self.session.requires_recovery() {
            return Err(NativeStepSessionError::RecoveryRequired);
        }
        let source = self.session.head()?;
        let count = self.session.receipts().count();
        if count == 0
            || count > MAX_NATIVE_STEP_SEGMENT_COMMANDS + 1
            || Some(source.commands) != self.sealed.head.source.commands.checked_add(count as u64)
        {
            return Err(NativeStepSessionError::Capacity);
        }
        let (mut bytes, count_at) = self.segment_header()?;
        let framed =
            |out: &mut Vec<u8>, value: &V| frame(out, value).map_err(|_| native_history_error());
        let mut selected = 0usize;
        let mut previous = self.sealed.head.source.clone();
        let mut before = self.sealed.physical.clone();
        for receipt in self.session.receipts() {
            if receipt.sequence != previous.commands
                || receipt.previous_head != previous.commitment
                || receipt.before != before
            {
                return Err(NativeStepSessionError::WrongHead);
            }
            let entry_size = native_history_record_size(&self.session, receipt)?;
            if bytes
                .len()
                .checked_add(entry_size)
                .and_then(|n| n.checked_add(4))
                .ok_or(NativeStepSessionError::Capacity)?
                > MAX_NATIVE_STEP_SEGMENT_BYTES
            {
                if selected == 0 {
                    return Err(NativeStepSessionError::Capacity);
                }
                break;
            }
            let sequence =
                usize::try_from(receipt.sequence).map_err(|_| NativeStepSessionError::Capacity)?;
            native_step_blob(
                &mut bytes,
                self.session
                    .command_field_frames(sequence)
                    .ok_or_else(native_history_error)?,
            )?;
            for frames in [
                &receipt.result_frames,
                &receipt.kernel_frames,
                &receipt.native_frames,
            ] {
                native_step_blob(&mut bytes, frames)?;
            }
            bytes.extend_from_slice(&receipt.financial_commitment);
            bytes.extend_from_slice(&receipt.head);
            let after = receipt
                .after
                .as_ref()
                .ok_or(NativeStepSessionError::RecoveryRequired)?;
            framed(&mut bytes, &physical_value(after))?;
            before = after.clone();
            previous = NativeStepHeadV1 {
                commands: previous
                    .commands
                    .checked_add(1)
                    .ok_or(NativeStepSessionError::Capacity)?,
                commitment: receipt.head,
            };
            selected += 1;
        }
        if selected == count
            && (previous != source || before != self.session.expectation()?.physical)
        {
            return Err(NativeStepSessionError::WrongHead);
        }
        bytes[count_at..count_at + 4].copy_from_slice(&(selected as u32).to_be_bytes());
        let bytes = crate::codec::with_crc(bytes).map_err(|_| NativeStepSessionError::Capacity)?;
        let head = native_history_after(&self.sealed.head, previous, sha256(&bytes))?;
        Ok(NativeStepPreparedSegmentV1 {
            bytes,
            publication: NativeStepHistoryPublicationV1 {
                head,
                physical: before,
            },
            commands: selected,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeRecordedHistoryPublicationV1 {
    head: NativeStepHistoryHeadV1,
    physical: V,
}
impl From<&NativeStepHistoryPublicationV1> for NativeRecordedHistoryPublicationV1 {
    fn from(value: &NativeStepHistoryPublicationV1) -> Self {
        Self {
            head: value.head.clone(),
            physical: physical_value(&value.physical),
        }
    }
}
struct NativeHistoryRecordV1 {
    command: MainStepCommandV1,
    command_frames: Vec<u8>,
    result: Vec<u8>,
    kernel: Vec<u8>,
    native: Vec<u8>,
    financial: [u8; 32],
    head: [u8; 32],
}
struct NativeDecodedHistorySegmentV1 {
    previous: NativeRecordedHistoryPublicationV1,
    after: NativeRecordedHistoryPublicationV1,
    records: Vec<NativeHistoryRecordV1>,
}
fn native_history_decode(
    bytes: &[u8],
    digest: [u8; 32],
    bootstrap: &NativeStepHistoryBootstrapV1,
) -> Result<NativeDecodedHistorySegmentV1, NativeStepSessionError> {
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(NativeStepSessionError::Capacity);
    }
    if sha256(bytes) != digest {
        return Err(NativeStepSessionError::WrongHead);
    }
    let payload = crate::codec::verify_crc(bytes).map_err(|_| native_history_error())?;
    let mut cursor = crate::codec::Cursor::new(payload);
    if cursor.take(5).map_err(|_| native_history_error())? != b"TLCR\x0d" {
        return Err(native_history_error());
    }
    for value in [
        s(NATIVE_STEP_SEGMENT_PROFILE),
        s(&bootstrap.context),
        s(bootstrap.profile.identifier()),
    ] {
        if read_value(&mut cursor).map_err(|_| native_history_error())? != value {
            return Err(native_history_error());
        }
    }
    if cursor.u64().map_err(|_| native_history_error())? != bootstrap.generation {
        return Err(native_history_error());
    }
    let previous = NativeRecordedHistoryPublicationV1 {
        head: native_history_read_head(&mut cursor)?,
        physical: read_value(&mut cursor).map_err(|_| native_history_error())?,
    };
    if !matches!(previous.physical, V::Object(_)) {
        return Err(native_history_error());
    }
    let count = cursor.u32().map_err(|_| native_history_error())? as usize;
    if count == 0 || count > MAX_NATIVE_STEP_SEGMENT_COMMANDS + 1 {
        return Err(NativeStepSessionError::Capacity);
    }
    let mut records = Vec::with_capacity(count);
    let mut source = previous.head.source.clone();
    let mut physical = previous.physical.clone();
    for at in 0..count {
        let command_frames = native_step_read_blob(&mut cursor)?.to_vec();
        let command = native_step_decode_command(&command_frames)?;
        if count > MAX_NATIVE_STEP_SEGMENT_COMMANDS
            && at == count - 1
            && !matches!(&command, MainStepCommandV1::Shutdown(_))
        {
            return Err(NativeStepSessionError::Capacity);
        }
        let result = native_step_read_blob(&mut cursor)?.to_vec();
        let kernel = native_step_read_blob(&mut cursor)?.to_vec();
        let native = native_step_read_blob(&mut cursor)?.to_vec();
        let summaries = [
            native_step_parse_fields(&command_frames, "Native13/StepCommand/v1")?.1,
            native_step_parse_fields(&result, "Native13/StepResult/v1")?.1,
            native_step_parse_fields(&kernel, "Native13/StepKernel/v1")?.1,
            native_step_parse_fields(&native, "Native13/StepNative/v1")?.1,
        ];
        let financial = cursor.fixed::<32>().map_err(|_| native_history_error())?;
        let head = cursor.fixed::<32>().map_err(|_| native_history_error())?;
        let mut chain = bootstrap.profile.identifier().as_bytes().to_vec();
        chain.extend_from_slice(&source.commitment);
        chain.extend_from_slice(&source.commands.to_be_bytes());
        for summary in &summaries {
            native_step_field_summary(&mut chain, summary);
        }
        chain.extend_from_slice(&financial);
        if sha256(&chain) != head {
            return Err(NativeStepSessionError::WrongHead);
        }
        physical = read_value(&mut cursor).map_err(|_| native_history_error())?;
        if !matches!(physical, V::Object(_)) {
            return Err(native_history_error());
        }
        source = NativeStepHeadV1 {
            commands: source
                .commands
                .checked_add(1)
                .ok_or(NativeStepSessionError::Capacity)?,
            commitment: head,
        };
        records.push(NativeHistoryRecordV1 {
            command,
            command_frames,
            result,
            kernel,
            native,
            financial,
            head,
        });
    }
    cursor.end().map_err(|_| native_history_error())?;
    let after = NativeRecordedHistoryPublicationV1 {
        head: native_history_after(&previous.head, source, digest)?,
        physical,
    };
    Ok(NativeDecodedHistorySegmentV1 {
        previous,
        after,
        records,
    })
}
fn native_history_verify_chain(
    bootstrap: &NativeStepHistoryBootstrapV1,
    expected: &NativeStepHistoryPublicationV1,
    reader: &mut impl FnMut([u8; 32]) -> Result<Vec<u8>, String>,
) -> Result<Vec<[u8; 32]>, NativeStepSessionError> {
    let archive = native_step_inspect_archive(
        &bootstrap.archive,
        &bootstrap.archive_pin,
        bootstrap.profile,
    )?;
    if archive.generation != bootstrap.generation
        || bootstrap.publication.head
            != native_history_initial(
                &bootstrap.context,
                bootstrap.profile,
                bootstrap.generation,
                bootstrap.archive_pin.head.clone(),
            )?
        || bootstrap.publication.physical != bootstrap.archive_pin.current
    {
        return Err(NativeStepSessionError::WrongHead);
    }
    if expected.head.segments > MAX_NATIVE_STEP_SEGMENTS {
        return Err(NativeStepSessionError::Capacity);
    }
    let mut reverse = Vec::with_capacity(expected.head.segments as usize);
    let mut cursor = NativeRecordedHistoryPublicationV1::from(expected);
    while cursor.head.segments > 0 {
        let digest = cursor.head.last_segment_sha256;
        let bytes = reader(digest).map_err(NativeStepSessionError::Backend)?;
        let segment = native_history_decode(&bytes, digest, bootstrap)?;
        if segment.after != cursor
            || segment.previous.head.segments.checked_add(1) != Some(cursor.head.segments)
        {
            return Err(NativeStepSessionError::WrongHead);
        }
        reverse.push(digest);
        cursor = segment.previous;
    }
    if cursor != NativeRecordedHistoryPublicationV1::from(&bootstrap.publication) {
        return Err(NativeStepSessionError::WrongHead);
    }
    reverse.reverse();
    Ok(reverse)
}
impl NativeStepHistoryWriterV1 {
    /// Reader failures or changed bytes after preflight retain the real prefix.
    /// Original physical history is checked; fresh physical pins are separate.
    pub fn reexecute(
        bootstrap: &NativeStepHistoryBootstrapV1,
        expected: &NativeStepHistoryPublicationV1,
        mut reader: impl FnMut([u8; 32]) -> Result<Vec<u8>, String>,
        index: SourceCompactIndex,
        expected_raw_origin: &NativePhysicalBinding,
        selected_profile: NativeStepProfileV1,
    ) -> Result<ReexecutedNativeStepHistoryV1, Box<RejectedNativeStepHistoryV1>> {
        if bootstrap.profile != selected_profile {
            return Err(Box::new(RejectedNativeStepHistoryV1 {
                error: native_history_error(),
                owner: NativeStepHistoryRejectionOwnerV1::Origin(Box::new(index)),
            }));
        }
        let digests = match native_history_verify_chain(bootstrap, expected, &mut reader) {
            Ok(v) => v,
            Err(error) => {
                return Err(Box::new(RejectedNativeStepHistoryV1 {
                    error,
                    owner: NativeStepHistoryRejectionOwnerV1::Origin(Box::new(index)),
                }))
            }
        };
        let restored = match bootstrap.profile {
            NativeStepProfileV1::Immediate => SourceMainNativeStepSessionV1::reexecute(
                &bootstrap.archive,
                &bootstrap.archive_pin,
                index,
                expected_raw_origin,
            ),
            NativeStepProfileV1::FileRetries => {
                SourceMainNativeStepSessionV1::reexecute_with_file_retries(
                    &bootstrap.archive,
                    &bootstrap.archive_pin,
                    index,
                    expected_raw_origin,
                )
            }
        };
        let mut session = match restored {
            Ok(v) => v,
            Err(rejected) => {
                return Err(Box::new(RejectedNativeStepHistoryV1 {
                    error: rejected.error.clone(),
                    owner: NativeStepHistoryRejectionOwnerV1::Bootstrap(rejected),
                }))
            }
        };
        native_history_release(&mut session);
        let checked = (|| {
            let mut publication = NativeRecordedHistoryPublicationV1::from(&bootstrap.publication);
            for digest in digests {
                let bytes = reader(digest).map_err(NativeStepSessionError::Backend)?;
                let segment = native_history_decode(&bytes, digest, bootstrap)?;
                if segment.previous != publication
                    || session.head()? != segment.previous.head.source
                {
                    return Err(NativeStepSessionError::WrongHead);
                }
                for recorded in segment.records {
                    let expected = session.expectation()?;
                    let actual = session.apply_step(&expected, recorded.command)?;
                    if actual.result_frames != recorded.result
                        || actual.kernel_frames != recorded.kernel
                        || actual.native_frames != recorded.native
                        || actual.financial_commitment != recorded.financial
                        || actual.head != recorded.head
                    {
                        return Err(NativeStepSessionError::Backend(
                            "actual segmented step reexecution mismatch".into(),
                        ));
                    }
                    let sequence = usize::try_from(actual.sequence)
                        .map_err(|_| NativeStepSessionError::Capacity)?;
                    if session.command_field_frames(sequence)
                        != Some(recorded.command_frames.as_slice())
                    {
                        return Err(NativeStepSessionError::WrongHead);
                    }
                }
                if session.head()? != segment.after.head.source {
                    return Err(NativeStepSessionError::WrongHead);
                }
                native_history_release(&mut session);
                publication = segment.after;
            }
            if session.head()? != expected.head.source {
                return Err(NativeStepSessionError::WrongHead);
            }
            Ok(())
        })();
        match checked {
            Ok(()) => Ok(ReexecutedNativeStepHistoryV1 {
                session,
                verified: expected.clone(),
            }),
            Err(error) => Err(Box::new(RejectedNativeStepHistoryV1 {
                error,
                owner: NativeStepHistoryRejectionOwnerV1::Steps(Box::new(session)),
            })),
        }
    }
}
