// Version13 archive reexecutes commands. No current financial state is decoded.
const NATIVE_MAGIC: &[u8; 5] = b"TLCR\x0d";
const MAX_NATIVE_ARCHIVE: usize = 64 * 1024 * 1024;
const MAX_NATIVE_FRAME: usize = 4 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainArchivePin {
    pub head: NativeMainSessionHead,
    pub foundation: SourceContractReplayHead,
    pub origin: NativePhysicalBinding,
    pub current: NativePhysicalBinding,
    /// Damage/substitution detection relative to an independently retained pin;
    /// this is not authorization of L1 data or supplied financial state.
    pub archive_sha256: [u8; 32],
}
pub enum NativeMainReexecutionOwner {
    RawIndex(Box<SourceCompactIndex>),
    Foundation(Box<NativeMainOwners>),
    Session(Box<SourceMainNativeSessionV13>),
}
pub struct RejectedNativeMainReexecution {
    pub error: NativeMainSessionError,
    pub owner: NativeMainReexecutionOwner,
}
impl core::fmt::Debug for RejectedNativeMainReexecution {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainReexecution")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
fn invalid_archive() -> NativeMainSessionError {
    NativeMainSessionError::Backend("invalid native13 archive".into())
}
fn blob(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), NativeMainSessionError> {
    if bytes.len() > MAX_NATIVE_ARCHIVE
        || out.len().saturating_add(4).saturating_add(bytes.len()) > MAX_NATIVE_ARCHIVE
    {
        return Err(NativeMainSessionError::Capacity);
    }
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}
fn read_blob<'a>(
    cursor: &mut crate::codec::Cursor<'a>,
    maximum: usize,
) -> Result<&'a [u8], NativeMainSessionError> {
    let count = cursor.u32().map_err(|_| invalid_archive())? as usize;
    if count > maximum {
        return Err(NativeMainSessionError::Capacity);
    }
    cursor.take(count).map_err(|_| invalid_archive())
}
fn read_value(cursor: &mut crate::codec::Cursor<'_>) -> Result<V, NativeMainSessionError> {
    let bytes = read_blob(cursor, MAX_NATIVE_FRAME)?;
    let text = std::str::from_utf8(bytes).map_err(|_| invalid_archive())?;
    let value =
        crate::property_catalog::parse_diagnostic_value(text).map_err(|_| invalid_archive())?;
    if value
        .diagnostic_json()
        .map_err(|_| invalid_archive())?
        .as_bytes()
        != bytes
    {
        return Err(invalid_archive());
    }
    Ok(value)
}
fn physical_value(binding: &NativePhysicalBinding) -> V {
    let pin = &binding.metadata;
    let publication = &pin.publication;
    obj(&[
        ("identity", s(&binding.persistent_identity)),
        ("catalog", s(&binding.canonical_catalog.to_string_lossy())),
        ("profile", s(&pin.profile)),
        ("context", s(&pin.context)),
        ("sequence", s(&pin.sequence.to_string())),
        ("generation", s(&publication.generation.to_string())),
        (
            "parent",
            publication.parent.map_or(V::Null, |v| s(&v.to_string())),
        ),
        (
            "physicalCreation",
            s(&publication.physical_creation_sequence.to_string()),
        ),
        (
            "logicalCreation",
            s(&publication.creation_sequence.to_string()),
        ),
        ("manifest", s(&hex(&publication.manifest_sha256d))),
    ])
}
fn decode_command(value: V) -> Result<NativeMainSessionCommand, NativeMainSessionError> {
    let command = if value.get("kind") == s("ConstructConsensusFromIndex") {
        NativeMainSessionCommand::ConstructConsensusFromIndex {
            start: value.get("start"),
        }
    } else if value.get("kind") == s("Invoke") {
        let V::String(target) = value.get("target") else {
            return Err(invalid_archive());
        };
        let V::Array(arguments) = value.get("args") else {
            return Err(invalid_archive());
        };
        NativeMainSessionCommand::Invoke(DependencyCall { target, arguments })
    } else if value.get("kind") == s("ProcessRows") {
        let V::Bool(live) = value.get("live") else {
            return Err(invalid_archive());
        };
        NativeMainSessionCommand::ProcessRows {
            rows: value.get("rows"),
            height: value.get("height"),
            live,
        }
    } else {
        return Err(invalid_archive());
    };
    if command.value() != value {
        return Err(invalid_archive());
    }
    Ok(command)
}
struct NativeArchiveInput {
    foundation: Vec<u8>,
    binding: V,
    origin_rows: V,
    entries: Vec<(NativeMainSessionCommand, Vec<u8>)>,
}
fn inspect_archive(
    bytes: &[u8],
    expected: &NativeMainArchivePin,
) -> Result<NativeArchiveInput, NativeMainSessionError> {
    if bytes.len() > MAX_NATIVE_ARCHIVE {
        return Err(NativeMainSessionError::Capacity);
    }
    if sha256(bytes) != expected.archive_sha256 {
        return Err(NativeMainSessionError::WrongHead);
    }
    let payload = crate::codec::verify_crc(bytes).map_err(|_| invalid_archive())?;
    let mut cursor = crate::codec::Cursor::new(payload);
    if cursor.take(5).map_err(|_| invalid_archive())? != NATIVE_MAGIC {
        return Err(invalid_archive());
    }
    if read_value(&mut cursor)? != s(PROFILE) {
        return Err(invalid_archive());
    }
    let foundation = read_blob(
        &mut cursor,
        crate::source_contract_replay::MAX_CHECKPOINT_BYTES,
    )?
    .to_vec();
    let foundation_count = cursor.u64().map_err(|_| invalid_archive())?;
    let foundation_head = cursor.fixed::<32>().map_err(|_| invalid_archive())?;
    if foundation_count != expected.foundation.commands
        || foundation_head != expected.foundation.commitment
    {
        return Err(NativeMainSessionError::WrongHead);
    }
    let binding = read_value(&mut cursor)?;
    let origin_rows = read_value(&mut cursor)?;
    if read_value(&mut cursor)? != physical_value(&expected.origin) {
        return Err(NativeMainSessionError::WrongPhysicalBinding);
    }
    let count = cursor.u32().map_err(|_| invalid_archive())? as usize;
    if count > MAX_COMMANDS || count as u64 != expected.head.commands {
        return Err(NativeMainSessionError::WrongHead);
    }
    let mut entries = Vec::with_capacity(count);
    let mut previous_physical = physical_value(&expected.origin);
    for _ in 0..count {
        let command = decode_command(read_value(&mut cursor)?)?;
        let logical_receipt = read_blob(&mut cursor, MAX_NATIVE_ARCHIVE)?.to_vec();
        // Physical operation records are retained exactly in the archive and
        // bound by its independent archive pin; never used as source answers.
        if read_value(&mut cursor)? != previous_physical {
            return Err(NativeMainSessionError::WrongPhysicalBinding);
        }
        let after = read_value(&mut cursor)?;
        if !matches!(after, V::Object(_)) {
            return Err(NativeMainSessionError::WrongPhysicalBinding);
        }
        previous_physical = after;
        entries.push((command, logical_receipt));
    }
    if cursor.fixed::<32>().map_err(|_| invalid_archive())? != expected.head.commitment {
        return Err(NativeMainSessionError::WrongHead);
    }
    if read_value(&mut cursor)? != physical_value(&expected.current) {
        return Err(NativeMainSessionError::WrongPhysicalBinding);
    }
    if previous_physical != physical_value(&expected.current) {
        return Err(NativeMainSessionError::WrongPhysicalBinding);
    }
    cursor.end().map_err(|_| invalid_archive())?;
    Ok(NativeArchiveInput {
        foundation,
        binding,
        origin_rows,
        entries,
    })
}
impl SourceMainNativeSessionV13 {
    pub fn checkpoint(&self) -> Result<(Vec<u8>, NativeMainArchivePin), NativeMainSessionError> {
        let current = physical(self.index())?;
        if self.history().iter().any(|receipt| {
            matches!(
                receipt.status,
                NativeMainSessionStatus::RecoveryRequired { .. }
            )
        }) {
            return Err(NativeMainSessionError::Halted);
        }
        let mut bytes = NATIVE_MAGIC.to_vec();
        frame(&mut bytes, &s(PROFILE))?;
        blob(&mut bytes, self.foundation_checkpoint())?;
        bytes.extend_from_slice(&self.foundation_head().commands.to_be_bytes());
        bytes.extend_from_slice(&self.foundation_head().commitment);
        frame(&mut bytes, self.logical_binding())?;
        frame(&mut bytes, &logical_read(self.origin_rows()))?;
        frame(&mut bytes, &physical_value(self.origin_physical()))?;
        bytes.extend_from_slice(&(self.history().len() as u32).to_be_bytes());
        for receipt in self.history() {
            frame(&mut bytes, &receipt.command.value())?;
            blob(&mut bytes, &logical_receipt_bytes(receipt)?)?;
            frame(&mut bytes, &physical_value(&receipt.before))?;
            frame(
                &mut bytes,
                &receipt.after.as_ref().map_or(V::Null, physical_value),
            )?;
        }
        bytes.extend_from_slice(&self.head().commitment);
        frame(&mut bytes, &physical_value(&current))?;
        let bytes = crate::codec::with_crc(bytes).map_err(|_| NativeMainSessionError::Capacity)?;
        let pin = NativeMainArchivePin {
            head: self.head(),
            foundation: self.foundation_head(),
            origin: self.origin_physical().clone(),
            current,
            archive_sha256: sha256(&bytes),
        };
        Ok((bytes, pin))
    }
    /// Raw origin metadata is supplied by a factual resolver into a fresh
    /// native owner. Financial state is rebuilt by the actual old source
    /// commands, then each native command; no financial snapshot is imported.
    /// Both recorded and replacement physical bindings are explicit inputs.
    pub fn reexecute(
        bytes: &[u8],
        expected: &NativeMainArchivePin,
        index: SourceCompactIndex,
        expected_raw_origin: &NativePhysicalBinding,
    ) -> Result<Self, Box<RejectedNativeMainReexecution>> {
        let prepared = (|| {
            if physical(&index)? != *expected_raw_origin {
                return Err(NativeMainSessionError::WrongPhysicalBinding);
            }
            if expected_raw_origin.metadata.context != expected.origin.metadata.context
                || expected_raw_origin.metadata.profile != expected.origin.metadata.profile
            {
                return Err(NativeMainSessionError::WrongPhysicalBinding);
            }
            let archive = inspect_archive(bytes, expected)?;
            if logical_read(&complete(&index)?) != archive.origin_rows {
                return Err(NativeMainSessionError::WrongPhysicalBinding);
            }
            let financial =
                SourceContractReplay::from_checkpoint(&archive.foundation, expected.foundation)
                    .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
            Ok((archive, financial))
        })();
        let (archive, financial) = match prepared {
            Ok(value) => value,
            Err(error) => {
                return Err(Box::new(RejectedNativeMainReexecution {
                    error,
                    owner: NativeMainReexecutionOwner::RawIndex(Box::new(index)),
                }))
            }
        };
        let origin = crate::source_main_native_owners::NativeMainExpectation {
            financial_head: expected.foundation,
            namespace: index.native_namespace().clone(),
            metadata: expected_raw_origin.metadata.clone(),
        };
        let owners = match NativeMainOwners::adopt(financial, index, &origin) {
            Ok(owners) => owners,
            Err(rejected) => {
                let error = NativeMainSessionError::Backend(format!("{:?}", rejected.error));
                let (_financial, index) = rejected.into_owners();
                return Err(Box::new(RejectedNativeMainReexecution {
                    error,
                    owner: NativeMainReexecutionOwner::RawIndex(Box::new(index)),
                }));
            }
        };
        let mut session = match Self::adopt(owners) {
            Ok(session) => session,
            Err(rejected) => {
                let error = rejected.error.clone();
                return Err(Box::new(RejectedNativeMainReexecution {
                    error,
                    owner: NativeMainReexecutionOwner::Foundation(Box::new(rejected.into_owners())),
                }));
            }
        };
        let checked = (|| {
            if *session.logical_binding() != archive.binding {
                return Err(NativeMainSessionError::WrongPhysicalBinding);
            }
            for (command, recorded) in archive.entries {
                let next = session.expectation()?;
                let actual = session.apply(&next, command)?;
                if logical_receipt_bytes(actual)? != recorded {
                    return Err(NativeMainSessionError::Backend(
                        "native13 replay mismatch".into(),
                    ));
                }
            }
            if session.head() != expected.head {
                return Err(NativeMainSessionError::WrongHead);
            }
            Ok(())
        })();
        match checked {
            Ok(()) => Ok(session),
            Err(error) => Err(Box::new(RejectedNativeMainReexecution {
                error,
                owner: NativeMainReexecutionOwner::Session(Box::new(session)),
            })),
        }
    }
}
