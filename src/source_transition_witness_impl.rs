// Included inside source_contract_replay.rs: reuse its exact journal codecs and
// actual replay engine. No supplied current-state projection is admitted.
pub const SOURCE_TRANSITION_SCHEMA: &str = "tl-literal-source-command-slice-v1";
pub const SOURCE_TRANSITION_BOUNDARY: &str =
    "tl-source-reference-db-rpc-clock-rng-numeric-boundary-v1";
pub const MAX_SOURCE_TRANSITION_BYTES: usize = MAX_CHECKPOINT_BYTES;
const SOURCE_TRANSITION_MAGIC: &[u8; 5] = b"TLSW\x01";
const SOURCE_TRANSITION_OWNER: u8 = 1;
const SOURCE_TRANSITION_PROFILE: u8 = 1;
const SOURCE_TRANSITION_BOUNDARY_BYTES: usize = 256;

/// This names a declared execution boundary, not an authenticated producer.
/// The source commit alone does not specify the DB/RPC/numeric adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTransitionDescriptor {
    pub source_commit: String,
    pub boundary: String,
}
impl SourceTransitionDescriptor {
    pub fn reference() -> Self {
        Self {
            source_commit: String::from_utf8(SOURCE_PIN.to_vec()).expect("ASCII source pin"),
            boundary: SOURCE_TRANSITION_BOUNDARY.into(),
        }
    }
}

/// Expectations must come from outside the untrusted witness. In particular,
/// reading pre_head from that witness supplies no independent history pin.
pub struct SourceTransitionExpectation<'a> {
    pub policy: SourceContractRuntimePolicy,
    pub descriptor: &'a SourceTransitionDescriptor,
    pub pre_head: SourceContractReplayHead,
    pub commands_commitment: Option<[u8; 32]>,
    pub post_head: Option<SourceContractReplayHead>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceTransitionError {
    Capacity,
    InvalidEnvelope,
    UnsupportedCommand,
    WrongPolicy,
    WrongDescriptor,
    WrongPreHead,
    WrongCommands,
    WrongPostHead,
    WrongPostState,
    Replay(SourceContractReplayError),
}
impl core::fmt::Display for SourceTransitionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SourceTransitionError {}
impl From<SourceContractReplayError> for SourceTransitionError {
    fn from(value: SourceContractReplayError) -> Self {
        Self::Replay(value)
    }
}
type SourceTransitionResult<T> = core::result::Result<T, SourceTransitionError>;

/// A literal command slice, not a complete block, custody authorization, or L1
/// proof. The retained prestate is the complete history-backed TLCR checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTransitionWitness {
    policy: SourceContractRuntimePolicy,
    descriptor: SourceTransitionDescriptor,
    pre_head: SourceContractReplayHead,
    before: Vec<u8>,
    commands: Vec<SourceContractCommand>,
    commands_commitment: [u8; 32],
    post_head: SourceContractReplayHead,
    post_state_commitment: [u8; 32],
}

/// This owns exactly one reconstructed source runtime. It proves execution
/// conditional on the pinned prestate and original commands, without granting
/// authority to the source actor claims, imports, activation, or chain labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalSourceTransition {
    replay: SourceContractReplay,
    pre_head: SourceContractReplayHead,
    pre_state_commitment: [u8; 32],
    commands_commitment: [u8; 32],
    post_state_commitment: [u8; 32],
}
impl ConditionalSourceTransition {
    pub fn scope(&self) -> &'static str {
        "conditional-source-execution-only"
    }
    pub fn replay(&self) -> &SourceContractReplay {
        &self.replay
    }
    pub fn into_replay(self) -> SourceContractReplay {
        self.replay
    }
    pub fn pre_head(&self) -> SourceContractReplayHead {
        self.pre_head
    }
    pub fn post_head(&self) -> SourceContractReplayHead {
        self.replay.head()
    }
    pub fn pre_state_commitment(&self) -> [u8; 32] {
        self.pre_state_commitment
    }
    pub fn post_state_commitment(&self) -> [u8; 32] {
        self.post_state_commitment
    }
    pub fn commands_commitment(&self) -> [u8; 32] {
        self.commands_commitment
    }
    pub fn receipts(&self) -> impl Iterator<Item = &SourceContractReceipt> {
        self.replay.receipts().skip(self.pre_head.commands as usize)
    }
}

fn source_transition_policy_tag(policy: SourceContractRuntimePolicy) -> SourceTransitionResult<u8> {
    // Reuse the replay policy's own codec tag, including future explicitly
    // versioned policies. Never assume that the runtime default is the policy.
    let bytes = SourceContractReplay::with_policy(policy)?.checkpoint()?;
    bytes
        .get(4)
        .copied()
        .ok_or(SourceTransitionError::InvalidEnvelope)
}
fn source_transition_descriptor(
    descriptor: &SourceTransitionDescriptor,
) -> SourceTransitionResult<()> {
    if descriptor != &SourceTransitionDescriptor::reference() {
        return Err(SourceTransitionError::WrongDescriptor);
    }
    Ok(())
}
fn source_transition_pre_policy(
    before: &[u8],
    policy: SourceContractRuntimePolicy,
) -> SourceTransitionResult<()> {
    if before.get(..4) != Some(b"TLCR") {
        return Err(SourceTransitionError::InvalidEnvelope);
    }
    if before.get(4).copied() != Some(source_transition_policy_tag(policy)?) {
        return Err(SourceTransitionError::WrongPolicy);
    }
    Ok(())
}
fn source_transition_command(command: &SourceContractCommand) -> SourceTransitionResult<()> {
    let admitted = match command {
        SourceContractCommand::SetHeight(_) => true,
        SourceContractCommand::ExecuteContract(args) => {
            args.len() == 9 && args.get(1) == Some(&V::number(18.0))
        }
        SourceContractCommand::EvaluateQueue(call) => {
            call.method == "processQueuedOnChainOrdersForBlock" && call.arguments.len() == 1
        }
        SourceContractCommand::Invoke(call) => match call.target.as_str() {
            "Tally.flushQueuedFeeAccruals" => call.arguments.len() == 1,
            // The genuine source's second `realtime` argument is optional and
            // unused. Preserve either original call arity in the command.
            "Clearing.clearingFunction" => matches!(call.arguments.len(), 1 | 2),
            _ => false,
        },
        _ => false,
    };
    if !admitted {
        return Err(SourceTransitionError::UnsupportedCommand);
    }
    if SourceContractCommand::from_value(&command.value())? != *command {
        return Err(SourceTransitionError::InvalidEnvelope);
    }
    diagnostic(&command.value())?;
    Ok(())
}
/// The digest pins the exact ordered commands, including repeated occurrences,
/// original value types, Number bits, source flag positions, and call arity.
pub fn source_transition_commands_commitment(
    commands: &[SourceContractCommand],
) -> SourceTransitionResult<[u8; 32]> {
    if commands.len() > MAX_COMMANDS {
        return Err(SourceTransitionError::Capacity);
    }
    let mut bytes = b"TradeLayer/SourceTransitionCommands/v1/".to_vec();
    bytes.extend_from_slice(&(commands.len() as u32).to_be_bytes());
    for command in commands {
        source_transition_command(command)?;
        put_frame(&mut bytes, &diagnostic(&command.value())?)?;
    }
    Ok(sha256(&bytes))
}
fn source_transition_count(
    pre: SourceContractReplayHead,
    count: usize,
) -> SourceTransitionResult<u64> {
    let post = pre
        .commands
        .checked_add(count as u64)
        .ok_or(SourceTransitionError::Capacity)?;
    if pre.commands > MAX_COMMANDS as u64 || count > MAX_COMMANDS || post > MAX_COMMANDS as u64 {
        return Err(SourceTransitionError::Capacity);
    }
    Ok(post)
}
fn source_transition_expected(
    expected: &SourceTransitionExpectation<'_>,
    policy: SourceContractRuntimePolicy,
    descriptor: &SourceTransitionDescriptor,
    pre_head: SourceContractReplayHead,
    commands: [u8; 32],
    post_head: Option<SourceContractReplayHead>,
) -> SourceTransitionResult<()> {
    source_transition_descriptor(expected.descriptor)?;
    if policy != expected.policy {
        return Err(SourceTransitionError::WrongPolicy);
    }
    if descriptor != expected.descriptor {
        return Err(SourceTransitionError::WrongDescriptor);
    }
    if pre_head != expected.pre_head {
        return Err(SourceTransitionError::WrongPreHead);
    }
    if expected
        .commands_commitment
        .is_some_and(|want| want != commands)
    {
        return Err(SourceTransitionError::WrongCommands);
    }
    if let (Some(want), Some(actual)) = (expected.post_head, post_head) {
        if want != actual {
            return Err(SourceTransitionError::WrongPostHead);
        }
    }
    Ok(())
}

impl SourceTransitionWitness {
    /// Does not mutate the producer. Temporary reconstruction validates the
    /// exact before checkpoint and derives every suffix receipt/state itself.
    pub fn produce(
        before: &SourceContractReplay,
        commands: &[SourceContractCommand],
        expected: &SourceTransitionExpectation<'_>,
    ) -> SourceTransitionResult<Self> {
        let commands_commitment = source_transition_commands_commitment(commands)?;
        source_transition_count(before.head(), commands.len())?;
        source_transition_expected(
            expected,
            before.runtime().policy(),
            expected.descriptor,
            before.head(),
            commands_commitment,
            None,
        )?;
        let bytes = before.checkpoint()?;
        let mut replay = SourceContractReplay::from_checkpoint(&bytes, expected.pre_head)?;
        for command in commands {
            replay.apply(command.clone())?;
        }
        let witness = Self {
            policy: before.runtime().policy(),
            descriptor: expected.descriptor.clone(),
            pre_head: before.head(),
            before: bytes,
            commands: commands.to_vec(),
            commands_commitment,
            post_head: replay.head(),
            post_state_commitment: state_commitment(replay.runtime())?,
        };
        source_transition_expected(
            expected,
            witness.policy,
            &witness.descriptor,
            witness.pre_head,
            witness.commands_commitment,
            Some(witness.post_head),
        )?;
        witness.canonical_bytes()?;
        Ok(witness)
    }
    pub fn policy(&self) -> SourceContractRuntimePolicy {
        self.policy
    }
    pub fn descriptor(&self) -> &SourceTransitionDescriptor {
        &self.descriptor
    }
    pub fn pre_head(&self) -> SourceContractReplayHead {
        self.pre_head
    }
    pub fn post_head(&self) -> SourceContractReplayHead {
        self.post_head
    }
    pub fn before_checkpoint(&self) -> &[u8] {
        &self.before
    }
    pub fn commands(&self) -> &[SourceContractCommand] {
        &self.commands
    }
    pub fn commands_commitment(&self) -> [u8; 32] {
        self.commands_commitment
    }
    pub fn post_state_commitment(&self) -> [u8; 32] {
        self.post_state_commitment
    }

    pub fn verify(
        &self,
        expected: &SourceTransitionExpectation<'_>,
    ) -> SourceTransitionResult<ConditionalSourceTransition> {
        source_transition_expected(
            expected,
            self.policy,
            &self.descriptor,
            self.pre_head,
            self.commands_commitment,
            Some(self.post_head),
        )?;
        let post_count = source_transition_count(self.pre_head, self.commands.len())?;
        if self.post_head.commands != post_count {
            return Err(SourceTransitionError::WrongPostHead);
        }
        if source_transition_commands_commitment(&self.commands)? != self.commands_commitment {
            return Err(SourceTransitionError::WrongCommands);
        }
        source_transition_pre_policy(&self.before, expected.policy)?;
        let mut replay = SourceContractReplay::from_checkpoint(&self.before, expected.pre_head)?;
        if replay.runtime().policy() != expected.policy {
            return Err(SourceTransitionError::WrongPolicy);
        }
        let pre_state_commitment = state_commitment(replay.runtime())?;
        for command in &self.commands {
            replay.apply(command.clone())?;
        }
        if replay.head() != self.post_head {
            return Err(SourceTransitionError::WrongPostHead);
        }
        let actual = state_commitment(replay.runtime())?;
        if actual != self.post_state_commitment {
            return Err(SourceTransitionError::WrongPostState);
        }
        Ok(ConditionalSourceTransition {
            replay,
            pre_head: self.pre_head,
            pre_state_commitment,
            commands_commitment: self.commands_commitment,
            post_state_commitment: actual,
        })
    }
    pub fn canonical_bytes(&self) -> SourceTransitionResult<Vec<u8>> {
        source_transition_descriptor(&self.descriptor)?;
        source_transition_count(self.pre_head, self.commands.len())?;
        if self.before.len() > MAX_CHECKPOINT_BYTES {
            return Err(SourceTransitionError::Capacity);
        }
        let mut bytes = SOURCE_TRANSITION_MAGIC.to_vec();
        bytes.push(SOURCE_TRANSITION_OWNER);
        bytes.push(SOURCE_TRANSITION_PROFILE);
        bytes.push(source_transition_policy_tag(self.policy)?);
        bytes.extend_from_slice(SOURCE_PIN);
        bytes.extend_from_slice(&(self.descriptor.boundary.len() as u16).to_be_bytes());
        bytes.extend_from_slice(self.descriptor.boundary.as_bytes());
        bytes.extend_from_slice(&self.pre_head.commands.to_be_bytes());
        bytes.extend_from_slice(&self.pre_head.commitment);
        bytes.extend_from_slice(&(self.before.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.before);
        bytes.extend_from_slice(&(self.commands.len() as u32).to_be_bytes());
        for command in &self.commands {
            source_transition_command(command)?;
            put_frame(&mut bytes, &diagnostic(&command.value())?)?;
        }
        bytes.extend_from_slice(&self.commands_commitment);
        bytes.extend_from_slice(&self.post_head.commands.to_be_bytes());
        bytes.extend_from_slice(&self.post_head.commitment);
        bytes.extend_from_slice(&self.post_state_commitment);
        let mut digest = b"TradeLayer/SourceTransitionWitness/v1/".to_vec();
        digest.extend_from_slice(&bytes);
        bytes.extend_from_slice(&sha256(&digest));
        if bytes.len().saturating_add(4) > MAX_SOURCE_TRANSITION_BYTES {
            return Err(SourceTransitionError::Capacity);
        }
        with_crc(bytes).map_err(|_| SourceTransitionError::Capacity)
    }
    /// Framing and independent expectations are checked before the retained
    /// checkpoint copy or actual replay. No current state or verdict is read.
    pub fn verify_bytes(
        bytes: &[u8],
        expected: &SourceTransitionExpectation<'_>,
    ) -> SourceTransitionResult<ConditionalSourceTransition> {
        source_transition_descriptor(expected.descriptor)?;
        if bytes.len() > MAX_SOURCE_TRANSITION_BYTES {
            return Err(SourceTransitionError::Capacity);
        }
        let raw = verify_crc(bytes).map_err(|_| SourceTransitionError::InvalidEnvelope)?;
        let mut c = Cursor::new(raw);
        let bad = |_| SourceTransitionError::InvalidEnvelope;
        if c.take(5).map_err(bad)? != SOURCE_TRANSITION_MAGIC
            || c.u8().map_err(bad)? != SOURCE_TRANSITION_OWNER
            || c.u8().map_err(bad)? != SOURCE_TRANSITION_PROFILE
        {
            return Err(SourceTransitionError::InvalidEnvelope);
        }
        if c.u8().map_err(bad)? != source_transition_policy_tag(expected.policy)? {
            return Err(SourceTransitionError::WrongPolicy);
        }
        if c.take(SOURCE_PIN.len()).map_err(bad)? != SOURCE_PIN {
            return Err(SourceTransitionError::WrongDescriptor);
        }
        let boundary_len = u16::from_be_bytes(c.fixed::<2>().map_err(bad)?) as usize;
        if boundary_len > SOURCE_TRANSITION_BOUNDARY_BYTES {
            return Err(SourceTransitionError::Capacity);
        }
        if c.take(boundary_len).map_err(bad)? != expected.descriptor.boundary.as_bytes() {
            return Err(SourceTransitionError::WrongDescriptor);
        }
        let pre_head = SourceContractReplayHead {
            commands: c.u64().map_err(bad)?,
            commitment: c.fixed::<32>().map_err(bad)?,
        };
        if pre_head != expected.pre_head {
            return Err(SourceTransitionError::WrongPreHead);
        }
        let before_len = c.u32().map_err(bad)? as usize;
        if before_len > MAX_CHECKPOINT_BYTES {
            return Err(SourceTransitionError::Capacity);
        }
        let before = c.take(before_len).map_err(bad)?;
        source_transition_pre_policy(before, expected.policy)?;
        let count = c.u32().map_err(bad)? as usize;
        let post_count = source_transition_count(pre_head, count)?;
        let mut commands = Vec::new();
        for _ in 0..count {
            let command = SourceContractCommand::from_value(&read_frame(&mut c)?)?;
            source_transition_command(&command)?;
            commands.push(command);
        }
        let commands_commitment = c.fixed::<32>().map_err(bad)?;
        let post_head = SourceContractReplayHead {
            commands: c.u64().map_err(bad)?,
            commitment: c.fixed::<32>().map_err(bad)?,
        };
        let post_state_commitment = c.fixed::<32>().map_err(bad)?;
        let digest = c.fixed::<32>().map_err(bad)?;
        c.end().map_err(bad)?;
        let mut digest_bytes = b"TradeLayer/SourceTransitionWitness/v1/".to_vec();
        digest_bytes.extend_from_slice(
            raw.get(..raw.len().saturating_sub(32))
                .ok_or(SourceTransitionError::InvalidEnvelope)?,
        );
        if sha256(&digest_bytes) != digest {
            return Err(SourceTransitionError::InvalidEnvelope);
        }
        if post_head.commands != post_count {
            return Err(SourceTransitionError::WrongPostHead);
        }
        source_transition_expected(
            expected,
            expected.policy,
            expected.descriptor,
            pre_head,
            commands_commitment,
            Some(post_head),
        )?;
        if source_transition_commands_commitment(&commands)? != commands_commitment {
            return Err(SourceTransitionError::WrongCommands);
        }
        let witness = Self {
            policy: expected.policy,
            descriptor: expected.descriptor.clone(),
            pre_head,
            before: before.to_vec(),
            commands,
            commands_commitment,
            post_head,
            post_state_commitment,
        };
        if witness.canonical_bytes()? != bytes {
            return Err(SourceTransitionError::InvalidEnvelope);
        }
        witness.verify(expected)
    }
}
