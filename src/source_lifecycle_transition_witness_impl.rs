// Included inside source_contract_replay.rs: reuse its exact journal codecs and
// actual replay engine. No supplied current-state projection is admitted.
pub const SOURCE_LIFECYCLE_TRANSITION_SCHEMA: &str = "tl-literal-source-lifecycle-command-slice-v2";
pub const SOURCE_LIFECYCLE_TRANSITION_BOUNDARY: &str =
    "tl-source-reference-db-rpc-clock-rng-numeric-boundary-v1";
pub const MAX_SOURCE_LIFECYCLE_TRANSITION_BYTES: usize = MAX_CHECKPOINT_BYTES;
const SOURCE_LIFECYCLE_TRANSITION_MAGIC: &[u8; 5] = b"TLSW\x02";
const SOURCE_LIFECYCLE_TRANSITION_OWNER: u8 = 1;
const SOURCE_LIFECYCLE_TRANSITION_PROFILE: u8 = 2;
const SOURCE_LIFECYCLE_TRANSITION_BOUNDARY_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLifecycleTransitionScope {
    RawTransactionSlice,
    SourceMethodSlice,
}
impl SourceLifecycleTransitionScope {
    fn tag(self) -> u8 {
        match self {
            Self::RawTransactionSlice => 1,
            Self::SourceMethodSlice => 2,
        }
    }
    fn from_tag(tag: u8) -> SourceLifecycleTransitionResult<Self> {
        match tag {
            1 => Ok(Self::RawTransactionSlice),
            2 => Ok(Self::SourceMethodSlice),
            _ => Err(SourceLifecycleTransitionError::InvalidEnvelope),
        }
    }
}
fn source_lifecycle_transition_policy(
    policy: SourceContractRuntimePolicy,
) -> SourceLifecycleTransitionResult<()> {
    if !matches!(
        policy,
        SourceContractRuntimePolicy::SourceLifecycleV3
            | SourceContractRuntimePolicy::OptionLifecycleV4
            | SourceContractRuntimePolicy::DistributionLifecycleV5
            | SourceContractRuntimePolicy::ProofLifecycleV6
    ) {
        return Err(SourceLifecycleTransitionError::WrongPolicy);
    }
    Ok(())
}
fn source_lifecycle_transition_raw(policy: SourceContractRuntimePolicy, kind: Option<&V>) -> bool {
    let base = [16., 17., 18., 19., 20., 21., 22., 23., 31.]
        .iter()
        .any(|v| kind == Some(&V::number(*v)));
    let option = matches!(
        policy,
        SourceContractRuntimePolicy::OptionLifecycleV4
            | SourceContractRuntimePolicy::DistributionLifecycleV5
            | SourceContractRuntimePolicy::ProofLifecycleV6
    ) && [24., 25., 27.].iter().any(|v| kind == Some(&V::number(*v)));
    let distribution = matches!(
        policy,
        SourceContractRuntimePolicy::DistributionLifecycleV5
            | SourceContractRuntimePolicy::ProofLifecycleV6
    ) && [26., 28., 29.].iter().any(|v| kind == Some(&V::number(*v)));
    let proof = policy == SourceContractRuntimePolicy::ProofLifecycleV6
        && [30., 32., 33., 34., 35.]
            .iter()
            .any(|v| kind == Some(&V::number(*v)));
    base || option || distribution || proof
}
fn source_lifecycle_transition_method(policy: SourceContractRuntimePolicy, target: &str) -> bool {
    if matches!(
        target,
        "Logic.createContractSeries"
            | "Logic.tradeContractChannel"
            | "Logic.tradeTokensChannel"
            | "Logic.withdrawal"
            | "Logic.transfer"
            | "Logic.settleChannelPNL"
            | "Logic.batchSettlement"
            | "Logic.processKeepAlive"
            | "Logic.processClosePosition"
            | "Logic.processNetSettle"
            | "Logic.recordSettlement"
    ) {
        return true;
    }
    if matches!(
        policy,
        SourceContractRuntimePolicy::OptionLifecycleV4
            | SourceContractRuntimePolicy::DistributionLifecycleV5
            | SourceContractRuntimePolicy::ProofLifecycleV6
    ) && matches!(
        target,
        "Logic.exerciseDerivative"
            | "Logic.deliverContract"
            | "Logic.handleOptionExercise"
            | "Logic.processOptionTrade"
            | "Logic.mintSynthetic"
            | "Logic.redeemSynthetic"
    ) {
        return true;
    }
    let distribution = matches!(
        policy,
        SourceContractRuntimePolicy::DistributionLifecycleV5
            | SourceContractRuntimePolicy::ProofLifecycleV6
    ) && matches!(
        target,
        "Logic.payToTokens" | "Logic.tradeBaiUrbun" | "Logic.tradeMurabaha"
    );
    distribution
        || (policy == SourceContractRuntimePolicy::ProofLifecycleV6
            && matches!(
                target,
                "Logic.processStakeFraudProof"
                    | "Logic.applyProceduralOracleSettlement"
                    | "Logic.queueLiquidityReward"
                    | "Logic.tradeTokenForUTXO"
                    | "Logic.batchMoveZkRollup"
            ))
}

/// Expectations must come from outside the untrusted witness. In particular,
/// reading pre_head from that witness supplies no independent history pin.
pub struct SourceLifecycleTransitionExpectation<'a> {
    pub policy: SourceContractRuntimePolicy,
    pub scope: SourceLifecycleTransitionScope,
    pub descriptor: &'a SourceTransitionDescriptor,
    pub pre_head: SourceContractReplayHead,
    pub commands_commitment: [u8; 32],
    pub post_head: Option<SourceContractReplayHead>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLifecycleTransitionError {
    Capacity,
    InvalidEnvelope,
    UnsupportedCommand,
    WrongPolicy,
    WrongScope,
    WrongDescriptor,
    WrongPreHead,
    WrongCommands,
    WrongPostHead,
    WrongPostState,
    Replay(SourceContractReplayError),
}
impl core::fmt::Display for SourceLifecycleTransitionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SourceLifecycleTransitionError {}
impl From<SourceContractReplayError> for SourceLifecycleTransitionError {
    fn from(value: SourceContractReplayError) -> Self {
        Self::Replay(value)
    }
}
type SourceLifecycleTransitionResult<T> = core::result::Result<T, SourceLifecycleTransitionError>;

/// A literal command slice, not a complete block, custody authorization, or L1
/// proof. The retained prestate is the complete history-backed TLCR checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLifecycleTransitionWitness {
    policy: SourceContractRuntimePolicy,
    scope: SourceLifecycleTransitionScope,
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
pub struct ConditionalSourceLifecycleTransition {
    replay: SourceContractReplay,
    scope: SourceLifecycleTransitionScope,
    pre_head: SourceContractReplayHead,
    pre_state_commitment: [u8; 32],
    commands_commitment: [u8; 32],
    post_state_commitment: [u8; 32],
}
impl ConditionalSourceLifecycleTransition {
    pub fn scope(&self) -> SourceLifecycleTransitionScope {
        self.scope
    }
    pub fn claim(&self) -> &'static str {
        match self.scope {
            SourceLifecycleTransitionScope::RawTransactionSlice => {
                "conditional-source-raw-command-slice-only"
            }
            SourceLifecycleTransitionScope::SourceMethodSlice => {
                "conditional-source-method-slice-only"
            }
        }
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

fn source_lifecycle_transition_policy_tag(
    policy: SourceContractRuntimePolicy,
) -> SourceLifecycleTransitionResult<u8> {
    // Reuse the replay policy's own codec tag, including future explicitly
    // versioned policies. Never assume that the runtime default is the policy.
    let bytes = SourceContractReplay::with_policy(policy)?.checkpoint()?;
    bytes
        .get(4)
        .copied()
        .ok_or(SourceLifecycleTransitionError::InvalidEnvelope)
}
fn source_lifecycle_transition_descriptor(
    descriptor: &SourceTransitionDescriptor,
) -> SourceLifecycleTransitionResult<()> {
    if descriptor != &SourceTransitionDescriptor::reference() {
        return Err(SourceLifecycleTransitionError::WrongDescriptor);
    }
    Ok(())
}
fn source_lifecycle_transition_pre_policy(
    before: &[u8],
    policy: SourceContractRuntimePolicy,
) -> SourceLifecycleTransitionResult<()> {
    if before.get(..4) != Some(b"TLCR") {
        return Err(SourceLifecycleTransitionError::InvalidEnvelope);
    }
    if before.get(4).copied() != Some(source_lifecycle_transition_policy_tag(policy)?) {
        return Err(SourceLifecycleTransitionError::WrongPolicy);
    }
    Ok(())
}
fn source_lifecycle_transition_command(
    policy: SourceContractRuntimePolicy,
    scope: SourceLifecycleTransitionScope,
    command: &SourceContractCommand,
) -> SourceLifecycleTransitionResult<()> {
    source_lifecycle_transition_policy(policy)?;
    let admitted = match command {
        SourceContractCommand::SetHeight(_) => true,
        SourceContractCommand::ExecuteContract(args) => {
            args.len() == 9 && source_lifecycle_transition_raw(policy, args.get(1))
        }
        SourceContractCommand::EvaluateQueue(call) => {
            matches!(
                call.method.as_str(),
                "processQueuedOnChainOrdersForBlock" | "processQueuedChannelTradesForBlock"
            ) && call.arguments.len() == 1
        }
        SourceContractCommand::Invoke(call) => match call.target.as_str() {
            "Tally.flushQueuedFeeAccruals" | "Channels.processWithdrawals" => {
                call.arguments.len() == 1
            }
            "Clearing.clearingFunction" => matches!(call.arguments.len(), 1 | 2),
            "Logic.settleLiquidityRewards"
                if policy == SourceContractRuntimePolicy::ProofLifecycleV6 =>
            {
                call.arguments.len() == 1
            }
            target if scope == SourceLifecycleTransitionScope::SourceMethodSlice => {
                call.arguments.len() <= 16 && source_lifecycle_transition_method(policy, target)
            }
            _ => false,
        },
        _ => false,
    };
    if !admitted {
        return Err(SourceLifecycleTransitionError::UnsupportedCommand);
    }
    if SourceContractCommand::from_value(&command.value())? != *command {
        return Err(SourceLifecycleTransitionError::InvalidEnvelope);
    }
    diagnostic(&command.value())?;
    Ok(())
}
/// The digest pins the exact ordered commands, including repeated occurrences,
/// original value types, Number bits, source flag positions, and call arity.
pub fn source_lifecycle_transition_commands_commitment(
    policy: SourceContractRuntimePolicy,
    scope: SourceLifecycleTransitionScope,
    commands: &[SourceContractCommand],
) -> SourceLifecycleTransitionResult<[u8; 32]> {
    if commands.len() > MAX_COMMANDS {
        return Err(SourceLifecycleTransitionError::Capacity);
    }
    source_lifecycle_transition_policy(policy)?;
    let mut bytes = b"TradeLayer/SourceLifecycleTransitionCommands/v2/".to_vec();
    bytes.push(source_lifecycle_transition_policy_tag(policy)?);
    bytes.push(scope.tag());
    bytes.extend_from_slice(&(commands.len() as u32).to_be_bytes());
    for command in commands {
        source_lifecycle_transition_command(policy, scope, command)?;
        put_frame(&mut bytes, &diagnostic(&command.value())?)?;
    }
    Ok(sha256(&bytes))
}
fn source_lifecycle_transition_count(
    pre: SourceContractReplayHead,
    count: usize,
) -> SourceLifecycleTransitionResult<u64> {
    let post = pre
        .commands
        .checked_add(count as u64)
        .ok_or(SourceLifecycleTransitionError::Capacity)?;
    if pre.commands > MAX_COMMANDS as u64 || count > MAX_COMMANDS || post > MAX_COMMANDS as u64 {
        return Err(SourceLifecycleTransitionError::Capacity);
    }
    Ok(post)
}
fn source_lifecycle_transition_expected(
    expected: &SourceLifecycleTransitionExpectation<'_>,
    policy: SourceContractRuntimePolicy,
    scope: SourceLifecycleTransitionScope,
    descriptor: &SourceTransitionDescriptor,
    pre_head: SourceContractReplayHead,
    commands: [u8; 32],
    post_head: Option<SourceContractReplayHead>,
) -> SourceLifecycleTransitionResult<()> {
    source_lifecycle_transition_policy(policy)?;
    source_lifecycle_transition_descriptor(expected.descriptor)?;
    if scope != expected.scope {
        return Err(SourceLifecycleTransitionError::WrongScope);
    }
    if policy != expected.policy {
        return Err(SourceLifecycleTransitionError::WrongPolicy);
    }
    if descriptor != expected.descriptor {
        return Err(SourceLifecycleTransitionError::WrongDescriptor);
    }
    if pre_head != expected.pre_head {
        return Err(SourceLifecycleTransitionError::WrongPreHead);
    }
    if expected.commands_commitment != commands {
        return Err(SourceLifecycleTransitionError::WrongCommands);
    }
    if let (Some(want), Some(actual)) = (expected.post_head, post_head) {
        if want != actual {
            return Err(SourceLifecycleTransitionError::WrongPostHead);
        }
    }
    Ok(())
}

impl SourceLifecycleTransitionWitness {
    /// Does not mutate the producer. Temporary reconstruction validates the
    /// exact before checkpoint and derives every suffix receipt/state itself.
    pub fn produce(
        before: &SourceContractReplay,
        commands: &[SourceContractCommand],
        expected: &SourceLifecycleTransitionExpectation<'_>,
    ) -> SourceLifecycleTransitionResult<Self> {
        let scope = expected.scope;
        let policy = before.runtime().policy();
        let commands_commitment =
            source_lifecycle_transition_commands_commitment(policy, scope, commands)?;
        source_lifecycle_transition_count(before.head(), commands.len())?;
        source_lifecycle_transition_expected(
            expected,
            before.runtime().policy(),
            scope,
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
            scope,
            descriptor: expected.descriptor.clone(),
            pre_head: before.head(),
            before: bytes,
            commands: commands.to_vec(),
            commands_commitment,
            post_head: replay.head(),
            post_state_commitment: state_commitment(replay.runtime())?,
        };
        source_lifecycle_transition_expected(
            expected,
            witness.policy,
            witness.scope,
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
    pub fn scope(&self) -> SourceLifecycleTransitionScope {
        self.scope
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
        expected: &SourceLifecycleTransitionExpectation<'_>,
    ) -> SourceLifecycleTransitionResult<ConditionalSourceLifecycleTransition> {
        source_lifecycle_transition_expected(
            expected,
            self.policy,
            self.scope,
            &self.descriptor,
            self.pre_head,
            self.commands_commitment,
            Some(self.post_head),
        )?;
        let post_count = source_lifecycle_transition_count(self.pre_head, self.commands.len())?;
        if self.post_head.commands != post_count {
            return Err(SourceLifecycleTransitionError::WrongPostHead);
        }
        if source_lifecycle_transition_commands_commitment(self.policy, self.scope, &self.commands)?
            != self.commands_commitment
        {
            return Err(SourceLifecycleTransitionError::WrongCommands);
        }
        source_lifecycle_transition_pre_policy(&self.before, expected.policy)?;
        let mut replay = SourceContractReplay::from_checkpoint(&self.before, expected.pre_head)?;
        if replay.runtime().policy() != expected.policy {
            return Err(SourceLifecycleTransitionError::WrongPolicy);
        }
        let pre_state_commitment = state_commitment(replay.runtime())?;
        for command in &self.commands {
            replay.apply(command.clone())?;
        }
        if replay.head() != self.post_head {
            return Err(SourceLifecycleTransitionError::WrongPostHead);
        }
        let actual = state_commitment(replay.runtime())?;
        if actual != self.post_state_commitment {
            return Err(SourceLifecycleTransitionError::WrongPostState);
        }
        Ok(ConditionalSourceLifecycleTransition {
            replay,
            scope: self.scope,
            pre_head: self.pre_head,
            pre_state_commitment,
            commands_commitment: self.commands_commitment,
            post_state_commitment: actual,
        })
    }
    pub fn canonical_bytes(&self) -> SourceLifecycleTransitionResult<Vec<u8>> {
        source_lifecycle_transition_descriptor(&self.descriptor)?;
        source_lifecycle_transition_count(self.pre_head, self.commands.len())?;
        if self.before.len() > MAX_CHECKPOINT_BYTES {
            return Err(SourceLifecycleTransitionError::Capacity);
        }
        let mut bytes = SOURCE_LIFECYCLE_TRANSITION_MAGIC.to_vec();
        bytes.push(SOURCE_LIFECYCLE_TRANSITION_OWNER);
        bytes.push(SOURCE_LIFECYCLE_TRANSITION_PROFILE);
        bytes.push(self.scope.tag());
        bytes.push(source_lifecycle_transition_policy_tag(self.policy)?);
        bytes.extend_from_slice(SOURCE_PIN);
        bytes.extend_from_slice(&(self.descriptor.boundary.len() as u16).to_be_bytes());
        bytes.extend_from_slice(self.descriptor.boundary.as_bytes());
        bytes.extend_from_slice(&self.pre_head.commands.to_be_bytes());
        bytes.extend_from_slice(&self.pre_head.commitment);
        bytes.extend_from_slice(&(self.before.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.before);
        bytes.extend_from_slice(&(self.commands.len() as u32).to_be_bytes());
        for command in &self.commands {
            source_lifecycle_transition_command(self.policy, self.scope, command)?;
            put_frame(&mut bytes, &diagnostic(&command.value())?)?;
        }
        bytes.extend_from_slice(&self.commands_commitment);
        bytes.extend_from_slice(&self.post_head.commands.to_be_bytes());
        bytes.extend_from_slice(&self.post_head.commitment);
        bytes.extend_from_slice(&self.post_state_commitment);
        let mut digest = b"TradeLayer/SourceLifecycleTransitionWitness/v2/".to_vec();
        digest.extend_from_slice(&bytes);
        bytes.extend_from_slice(&sha256(&digest));
        if bytes.len().saturating_add(4) > MAX_SOURCE_LIFECYCLE_TRANSITION_BYTES {
            return Err(SourceLifecycleTransitionError::Capacity);
        }
        with_crc(bytes).map_err(|_| SourceLifecycleTransitionError::Capacity)
    }
    /// Framing and independent expectations are checked before the retained
    /// checkpoint copy or actual replay. No current state or verdict is read.
    pub fn verify_bytes(
        bytes: &[u8],
        expected: &SourceLifecycleTransitionExpectation<'_>,
    ) -> SourceLifecycleTransitionResult<ConditionalSourceLifecycleTransition> {
        source_lifecycle_transition_descriptor(expected.descriptor)?;
        if bytes.len() > MAX_SOURCE_LIFECYCLE_TRANSITION_BYTES {
            return Err(SourceLifecycleTransitionError::Capacity);
        }
        let raw = verify_crc(bytes).map_err(|_| SourceLifecycleTransitionError::InvalidEnvelope)?;
        let mut c = Cursor::new(raw);
        let bad = |_| SourceLifecycleTransitionError::InvalidEnvelope;
        if c.take(5).map_err(bad)? != SOURCE_LIFECYCLE_TRANSITION_MAGIC
            || c.u8().map_err(bad)? != SOURCE_LIFECYCLE_TRANSITION_OWNER
            || c.u8().map_err(bad)? != SOURCE_LIFECYCLE_TRANSITION_PROFILE
        {
            return Err(SourceLifecycleTransitionError::InvalidEnvelope);
        }
        let scope = SourceLifecycleTransitionScope::from_tag(c.u8().map_err(bad)?)?;
        if scope != expected.scope {
            return Err(SourceLifecycleTransitionError::WrongScope);
        }
        source_lifecycle_transition_policy(expected.policy)?;
        if c.u8().map_err(bad)? != source_lifecycle_transition_policy_tag(expected.policy)? {
            return Err(SourceLifecycleTransitionError::WrongPolicy);
        }
        if c.take(SOURCE_PIN.len()).map_err(bad)? != SOURCE_PIN {
            return Err(SourceLifecycleTransitionError::WrongDescriptor);
        }
        let boundary_len = u16::from_be_bytes(c.fixed::<2>().map_err(bad)?) as usize;
        if boundary_len > SOURCE_LIFECYCLE_TRANSITION_BOUNDARY_BYTES {
            return Err(SourceLifecycleTransitionError::Capacity);
        }
        if c.take(boundary_len).map_err(bad)? != expected.descriptor.boundary.as_bytes() {
            return Err(SourceLifecycleTransitionError::WrongDescriptor);
        }
        let pre_head = SourceContractReplayHead {
            commands: c.u64().map_err(bad)?,
            commitment: c.fixed::<32>().map_err(bad)?,
        };
        if pre_head != expected.pre_head {
            return Err(SourceLifecycleTransitionError::WrongPreHead);
        }
        let before_len = c.u32().map_err(bad)? as usize;
        if before_len > MAX_CHECKPOINT_BYTES {
            return Err(SourceLifecycleTransitionError::Capacity);
        }
        let before = c.take(before_len).map_err(bad)?;
        source_lifecycle_transition_pre_policy(before, expected.policy)?;
        let count = c.u32().map_err(bad)? as usize;
        let post_count = source_lifecycle_transition_count(pre_head, count)?;
        let mut commands = Vec::new();
        for _ in 0..count {
            let command = SourceContractCommand::from_value(&read_frame(&mut c)?)?;
            source_lifecycle_transition_command(expected.policy, expected.scope, &command)?;
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
        let mut digest_bytes = b"TradeLayer/SourceLifecycleTransitionWitness/v2/".to_vec();
        digest_bytes.extend_from_slice(
            raw.get(..raw.len().saturating_sub(32))
                .ok_or(SourceLifecycleTransitionError::InvalidEnvelope)?,
        );
        if sha256(&digest_bytes) != digest {
            return Err(SourceLifecycleTransitionError::InvalidEnvelope);
        }
        if post_head.commands != post_count {
            return Err(SourceLifecycleTransitionError::WrongPostHead);
        }
        source_lifecycle_transition_expected(
            expected,
            expected.policy,
            expected.scope,
            expected.descriptor,
            pre_head,
            commands_commitment,
            Some(post_head),
        )?;
        if source_lifecycle_transition_commands_commitment(
            expected.policy,
            expected.scope,
            &commands,
        )? != commands_commitment
        {
            return Err(SourceLifecycleTransitionError::WrongCommands);
        }
        let witness = Self {
            policy: expected.policy,
            scope: expected.scope,
            descriptor: expected.descriptor.clone(),
            pre_head,
            before: before.to_vec(),
            commands,
            commands_commitment,
            post_head,
            post_state_commitment,
        };
        if witness.canonical_bytes()? != bytes {
            return Err(SourceLifecycleTransitionError::InvalidEnvelope);
        }
        witness.verify(expected)
    }
}
