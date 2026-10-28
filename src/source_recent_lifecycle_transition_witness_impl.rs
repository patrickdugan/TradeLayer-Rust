// Included after TLSW1/TLSW2 in source_contract_replay.rs. Their formats,
// admission functions and public APIs are unchanged.
pub const SOURCE_RECENT_LIFECYCLE_TRANSITION_SCHEMA: &str =
    "tl-literal-source-recent-lifecycle-command-slice-v3";
pub const SOURCE_RECENT_LIFECYCLE_NUMERIC_DOMAIN: &str =
    "tl-source-number-bits-bounded-decimal-bignumber-v1";
pub const SOURCE_RECENT_LIFECYCLE_DEPENDENCY_DOMAIN: &str =
    "tl-source-owned-db-data-only-rpc-clock-rng-v1";
pub const MAX_SOURCE_RECENT_LIFECYCLE_TRANSITION_BYTES: usize = MAX_CHECKPOINT_BYTES;
const RECENT_MAGIC: &[u8; 5] = b"TLSW\x03";
const RECENT_COMMAND_DOMAIN: &[u8] = b"TradeLayer/SourceRecentLifecycleTransitionCommands/v3/";
const RECENT_WITNESS_DOMAIN: &[u8] = b"TradeLayer/SourceRecentLifecycleTransitionWitness/v3/";

/// Adapter declarations bind supplied facts and bounded arithmetic/storage;
/// they do not authenticate those facts, the producer, actors or a chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRecentLifecycleTransitionDescriptor {
    pub source_commit: String,
    pub boundary: String,
    pub numeric_domain: String,
    pub dependency_domain: String,
}
impl SourceRecentLifecycleTransitionDescriptor {
    pub fn reference() -> Self {
        Self {
            source_commit: String::from_utf8(SOURCE_PIN.to_vec()).expect("ASCII source pin"),
            boundary: SOURCE_TRANSITION_BOUNDARY.into(),
            numeric_domain: SOURCE_RECENT_LIFECYCLE_NUMERIC_DOMAIN.into(),
            dependency_domain: SOURCE_RECENT_LIFECYCLE_DEPENDENCY_DOMAIN.into(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Keep the explicit Slice suffix, as in TLSW2: none of these scopes claims a
// complete extracted transaction set or a complete block proof.
#[allow(clippy::enum_variant_names)]
pub enum SourceRecentLifecycleTransitionScope {
    RawTransactionSlice,
    SourceMethodSlice,
    MainCallableSlice,
}
impl SourceRecentLifecycleTransitionScope {
    fn tag(self) -> u8 {
        match self {
            Self::RawTransactionSlice => 1,
            Self::SourceMethodSlice => 2,
            Self::MainCallableSlice => 3,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRecentLifecycleActivationProfile {
    SourceRegistry,
    DeclaredAnswer,
}
/// Both endpoint profiles are independently pinned. V8 activation entry and
/// lock are explicit, including the standalone declared-answer entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRecentLifecycleEntryProfile {
    Lower {
        module_entry: String,
    },
    Main {
        module_entry: String,
        activation_profile: SourceRecentLifecycleActivationProfile,
        activation_locked: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRecentLifecycleTransitionError {
    Capacity,
    InvalidEnvelope,
    WrongPolicy,
    WrongScope,
    WrongDescriptor,
    WrongPreHead,
    WrongPostHead,
    WrongPreState,
    WrongPostState,
    WrongCommands,
    WrongEntryProfile,
    UnsupportedCommand,
    PendingWork,
    UnavailableExecution,
    Replay(SourceContractReplayError),
}
impl core::fmt::Display for SourceRecentLifecycleTransitionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SourceRecentLifecycleTransitionError {}
impl From<SourceContractReplayError> for SourceRecentLifecycleTransitionError {
    fn from(value: SourceContractReplayError) -> Self {
        Self::Replay(value)
    }
}
type RecentResult<T> = core::result::Result<T, SourceRecentLifecycleTransitionError>;
use SourceRecentLifecycleTransitionError as RecentError;

/// All pins are required and must come from outside the untrusted witness.
pub struct SourceRecentLifecycleTransitionExpectation<'a> {
    pub policy: SourceContractRuntimePolicy,
    pub scope: SourceRecentLifecycleTransitionScope,
    pub descriptor: &'a SourceRecentLifecycleTransitionDescriptor,
    pub pre_head: SourceContractReplayHead,
    pub pre_state_commitment: [u8; 32],
    pub pre_entry: &'a SourceRecentLifecycleEntryProfile,
    pub commands_commitment: [u8; 32],
    pub post_head: SourceContractReplayHead,
    pub post_state_commitment: [u8; 32],
    pub post_entry: &'a SourceRecentLifecycleEntryProfile,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRecentLifecycleTransitionWitness {
    policy: SourceContractRuntimePolicy,
    scope: SourceRecentLifecycleTransitionScope,
    descriptor: SourceRecentLifecycleTransitionDescriptor,
    pre_head: SourceContractReplayHead,
    pre_state_commitment: [u8; 32],
    pre_entry: SourceRecentLifecycleEntryProfile,
    before: Vec<u8>,
    commands: Vec<SourceContractCommand>,
    commands_commitment: [u8; 32],
    post_head: SourceContractReplayHead,
    post_state_commitment: [u8; 32],
    post_entry: SourceRecentLifecycleEntryProfile,
}
/// One reconstructed owner, conditional on independently retained history and
/// original commands. This is neither authentication nor a complete block proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalSourceRecentLifecycleTransition {
    replay: SourceContractReplay,
    scope: SourceRecentLifecycleTransitionScope,
    pre_head: SourceContractReplayHead,
    pre_state_commitment: [u8; 32],
    commands_commitment: [u8; 32],
    post_state_commitment: [u8; 32],
}
impl ConditionalSourceRecentLifecycleTransition {
    pub fn scope(&self) -> SourceRecentLifecycleTransitionScope {
        self.scope
    }
    pub fn claim(&self) -> &'static str {
        match self.scope {
            SourceRecentLifecycleTransitionScope::RawTransactionSlice => {
                "conditional-source-raw-command-slice-only"
            }
            SourceRecentLifecycleTransitionScope::SourceMethodSlice => {
                "conditional-source-method-slice-only"
            }
            SourceRecentLifecycleTransitionScope::MainCallableSlice => {
                "conditional-source-main-callable-slice-only"
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
fn recent_policy(policy: SourceContractRuntimePolicy) -> RecentResult<u8> {
    match policy {
        SourceContractRuntimePolicy::LowerLifecycleV7 => Ok(7),
        SourceContractRuntimePolicy::MainCallableV8 => Ok(8),
        _ => Err(RecentError::WrongPolicy),
    }
}
fn recent_descriptor(descriptor: &SourceRecentLifecycleTransitionDescriptor) -> RecentResult<()> {
    if descriptor != &SourceRecentLifecycleTransitionDescriptor::reference() {
        return Err(RecentError::WrongDescriptor);
    }
    Ok(())
}
fn recent_entry_valid(
    policy: SourceContractRuntimePolicy,
    profile: &SourceRecentLifecycleEntryProfile,
) -> RecentResult<()> {
    let entry = match (policy, profile) {
        (
            SourceContractRuntimePolicy::LowerLifecycleV7,
            SourceRecentLifecycleEntryProfile::Lower { module_entry },
        ) => module_entry,
        (
            SourceContractRuntimePolicy::MainCallableV8,
            SourceRecentLifecycleEntryProfile::Main { module_entry, .. },
        ) => module_entry,
        _ => return Err(RecentError::WrongEntryProfile),
    };
    if !matches!(
        entry.as_str(),
        "main-subgraph" | "types-first" | "tally-first" | "volume-first"
    ) {
        return Err(RecentError::WrongEntryProfile);
    }
    Ok(())
}
/// Convenience access to the actual retained owner, not an independent pin if
/// the consumer obtains this value from the untrusted witness itself.
pub fn source_recent_lifecycle_entry_profile(
    replay: &SourceContractReplay,
) -> RecentResult<SourceRecentLifecycleEntryProfile> {
    recent_policy(replay.runtime().policy())?;
    let state = replay
        .runtime()
        .state_value()
        .map_err(SourceContractReplayError::Runtime)?;
    let V::String(module_entry) = state.get("continuation").get("entry") else {
        return Err(RecentError::WrongEntryProfile);
    };
    let profile = if replay.runtime().policy() == SourceContractRuntimePolicy::LowerLifecycleV7 {
        SourceRecentLifecycleEntryProfile::Lower { module_entry }
    } else {
        let main = state.get("mainCallable");
        if main.get("profile") != s("TransactionsAndMidV8") {
            return Err(RecentError::WrongEntryProfile);
        }
        if main.get("pendingHeight") != V::Null
            || state.get("sendCommit").get("pendingRead") != V::Bool(false)
        {
            return Err(RecentError::PendingWork);
        }
        let activation_profile = match main.get("activationProfile") {
            V::String(value) if value == "source-registry" => {
                SourceRecentLifecycleActivationProfile::SourceRegistry
            }
            V::String(value) if value == "declared-answer" => {
                SourceRecentLifecycleActivationProfile::DeclaredAnswer
            }
            _ => return Err(RecentError::WrongEntryProfile),
        };
        let V::Bool(activation_locked) = main.get("activationLocked") else {
            return Err(RecentError::WrongEntryProfile);
        };
        SourceRecentLifecycleEntryProfile::Main {
            module_entry,
            activation_profile,
            activation_locked,
        }
    };
    recent_entry_valid(replay.runtime().policy(), &profile)?;
    Ok(profile)
}
pub fn source_recent_lifecycle_state_commitment(
    replay: &SourceContractReplay,
) -> RecentResult<[u8; 32]> {
    recent_policy(replay.runtime().policy())?;
    Ok(state_commitment(replay.runtime())?)
}
fn recent_raw(policy: SourceContractRuntimePolicy, kind: Option<&V>) -> bool {
    [
        1., 8., 11., 12., 13., 14., 15., 16., 17., 18., 19., 20., 21., 22., 23., 24., 25., 26.,
        27., 28., 29., 30., 31., 32., 33., 34., 35.,
    ]
    .iter()
    .any(|n| kind == Some(&V::number(*n)))
        || (policy == SourceContractRuntimePolicy::MainCallableV8
            && [2., 3., 4., 7., 9.]
                .iter()
                .any(|n| kind == Some(&V::number(*n))))
}
fn recent_method(policy: SourceContractRuntimePolicy, target: &str) -> bool {
    matches!(
        target,
        "Logic.tokenIssue"
            | "Logic.grantManagedToken"
            | "Logic.redeemManagedToken"
            | "Logic.createOracle"
            | "Logic.publishOracleData"
            | "Logic.closeOracle"
            | "Logic.updateAdmin"
            | "Logic.createContractSeries"
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
            | "Logic.exerciseDerivative"
            | "Logic.deliverContract"
            | "Logic.handleOptionExercise"
            | "Logic.processOptionTrade"
            | "Logic.mintSynthetic"
            | "Logic.redeemSynthetic"
            | "Logic.payToTokens"
            | "Logic.tradeBaiUrbun"
            | "Logic.tradeMurabaha"
            | "Logic.processStakeFraudProof"
            | "Logic.applyProceduralOracleSettlement"
            | "Logic.queueLiquidityReward"
            | "Logic.tradeTokenForUTXO"
            | "Logic.batchMoveZkRollup"
    ) || (policy == SourceContractRuntimePolicy::MainCallableV8
        && matches!(
            target,
            "Logic.sendToken"
                | "Logic.sendSingle"
                | "Logic.sendAll"
                | "Logic.processSend"
                | "Logic.vestingSend"
                | "Logic.calculateVestingMovement"
                | "Logic.roundToEightDecimals"
                | "Logic.commitToken"
                | "Logic.createClearList"
                | "Logic.issueOrRevokeAttestation"
                | "Logic.updateBannedCountries"
        ))
}
fn recent_command(
    policy: SourceContractRuntimePolicy,
    scope: SourceRecentLifecycleTransitionScope,
    command: &SourceContractCommand,
) -> RecentResult<()> {
    recent_policy(policy)?;
    use SourceRecentLifecycleTransitionScope as S;
    if scope == S::MainCallableSlice && policy != SourceContractRuntimePolicy::MainCallableV8 {
        return Err(RecentError::WrongScope);
    }
    let admitted = match command {
        SourceContractCommand::SetHeight(_) => true,
        SourceContractCommand::ExecuteContract(args) if scope != S::MainCallableSlice => {
            args.len() == 9 && recent_raw(policy, args.get(1))
        }
        SourceContractCommand::EvaluateQueue(call) if scope != S::MainCallableSlice => {
            matches!(
                call.method.as_str(),
                "processQueuedOnChainOrdersForBlock" | "processQueuedChannelTradesForBlock"
            ) && call.arguments.len() == 1
        }
        SourceContractCommand::Invoke(call) if scope == S::MainCallableSlice => {
            match call.target.as_str() {
                "Main.processTxSet" | "Main.processTx" | "Main.blockHandlerMid" => {
                    call.arguments.len() == 2
                }
                "Main.loadMaxProcessedHeight" | "Main.loadTrackHeight" => call.arguments.is_empty(),
                "Main.saveMaxProcessedHeight" | "Main.saveTrackHeight" => call.arguments.len() == 1,
                _ => false,
            }
        }
        SourceContractCommand::Invoke(call) => match call.target.as_str() {
            "Tally.flushQueuedFeeAccruals"
            | "Channels.processWithdrawals"
            | "Logic.settleLiquidityRewards" => call.arguments.len() == 1,
            "Clearing.clearingFunction" => matches!(call.arguments.len(), 1 | 2),
            target if scope == S::SourceMethodSlice => {
                call.arguments.len() <= 16 && recent_method(policy, target)
            }
            _ => false,
        },
        _ => false,
    };
    if !admitted {
        return Err(RecentError::UnsupportedCommand);
    }
    if SourceContractCommand::from_value(&command.value())? != *command {
        return Err(RecentError::InvalidEnvelope);
    }
    diagnostic(&command.value())?;
    Ok(())
}
pub fn source_recent_lifecycle_transition_commands_commitment(
    policy: SourceContractRuntimePolicy,
    scope: SourceRecentLifecycleTransitionScope,
    commands: &[SourceContractCommand],
) -> RecentResult<[u8; 32]> {
    let tag = recent_policy(policy)?;
    if scope == SourceRecentLifecycleTransitionScope::MainCallableSlice && tag != 8 {
        return Err(RecentError::WrongScope);
    }
    if commands.len() > MAX_COMMANDS {
        return Err(RecentError::Capacity);
    }
    let mut bytes = RECENT_COMMAND_DOMAIN.to_vec();
    bytes.extend_from_slice(&[tag, scope.tag()]);
    for domain in [
        SOURCE_RECENT_LIFECYCLE_NUMERIC_DOMAIN,
        SOURCE_RECENT_LIFECYCLE_DEPENDENCY_DOMAIN,
    ] {
        bytes.extend_from_slice(&(domain.len() as u16).to_be_bytes());
        bytes.extend_from_slice(domain.as_bytes());
    }
    bytes.extend_from_slice(&(commands.len() as u32).to_be_bytes());
    for command in commands {
        recent_command(policy, scope, command)?;
        put_frame(&mut bytes, &diagnostic(&command.value())?)?;
    }
    Ok(sha256(&bytes))
}
fn recent_count(pre: SourceContractReplayHead, count: usize) -> RecentResult<u64> {
    let post = pre
        .commands
        .checked_add(count as u64)
        .ok_or(RecentError::Capacity)?;
    if pre.commands > MAX_COMMANDS as u64 || count > MAX_COMMANDS || post > MAX_COMMANDS as u64 {
        return Err(RecentError::Capacity);
    }
    Ok(post)
}
fn recent_expected(expected: &SourceRecentLifecycleTransitionExpectation<'_>) -> RecentResult<()> {
    recent_policy(expected.policy)?;
    recent_descriptor(expected.descriptor)?;
    recent_entry_valid(expected.policy, expected.pre_entry)?;
    recent_entry_valid(expected.policy, expected.post_entry)?;
    if expected.scope == SourceRecentLifecycleTransitionScope::MainCallableSlice {
        if expected.policy != SourceContractRuntimePolicy::MainCallableV8 {
            return Err(RecentError::WrongScope);
        }
        for profile in [expected.pre_entry, expected.post_entry] {
            if !matches!(
                profile,
                SourceRecentLifecycleEntryProfile::Main {
                    activation_profile: SourceRecentLifecycleActivationProfile::SourceRegistry,
                    ..
                }
            ) {
                return Err(RecentError::WrongEntryProfile);
            }
        }
    }
    Ok(())
}
fn recent_endpoint(
    replay: &SourceContractReplay,
    expected: &SourceRecentLifecycleTransitionExpectation<'_>,
    post: bool,
) -> RecentResult<()> {
    if replay.runtime().policy() != expected.policy {
        return Err(RecentError::WrongPolicy);
    }
    if replay.head()
        != if post {
            expected.post_head
        } else {
            expected.pre_head
        }
    {
        return Err(if post {
            RecentError::WrongPostHead
        } else {
            RecentError::WrongPreHead
        });
    }
    if &source_recent_lifecycle_entry_profile(replay)?
        != if post {
            expected.post_entry
        } else {
            expected.pre_entry
        }
    {
        return Err(RecentError::WrongEntryProfile);
    }
    if state_commitment(replay.runtime())?
        != if post {
            expected.post_state_commitment
        } else {
            expected.pre_state_commitment
        }
    {
        return Err(if post {
            RecentError::WrongPostState
        } else {
            RecentError::WrongPreState
        });
    }
    Ok(())
}
fn recent_apply(
    replay: &mut SourceContractReplay,
    command: &SourceContractCommand,
) -> RecentResult<()> {
    if matches!(
        replay.apply(command.clone())?.outcome,
        SourceContractCommandOutcome::LocalError(_)
    ) {
        return Err(RecentError::UnavailableExecution);
    }
    Ok(())
}
fn recent_put_entry(profile: &SourceRecentLifecycleEntryProfile, out: &mut Vec<u8>) {
    let (tag, entry) = match profile {
        SourceRecentLifecycleEntryProfile::Lower { module_entry } => (7, module_entry),
        SourceRecentLifecycleEntryProfile::Main { module_entry, .. } => (8, module_entry),
    };
    out.extend_from_slice(&[tag, entry.len() as u8]);
    out.extend_from_slice(entry.as_bytes());
    if let SourceRecentLifecycleEntryProfile::Main {
        activation_profile,
        activation_locked,
        ..
    } = profile
    {
        out.push(match activation_profile {
            SourceRecentLifecycleActivationProfile::SourceRegistry => 1,
            SourceRecentLifecycleActivationProfile::DeclaredAnswer => 2,
        });
        out.push(u8::from(*activation_locked));
    }
}
fn recent_read_entry(c: &mut Cursor<'_>) -> RecentResult<SourceRecentLifecycleEntryProfile> {
    let tag = c.u8().map_err(|_| RecentError::InvalidEnvelope)?;
    let len = c.u8().map_err(|_| RecentError::InvalidEnvelope)? as usize;
    if len > 32 {
        return Err(RecentError::InvalidEnvelope);
    }
    let module_entry = core::str::from_utf8(c.take(len).map_err(|_| RecentError::InvalidEnvelope)?)
        .map_err(|_| RecentError::InvalidEnvelope)?
        .to_string();
    match tag {
        7 => Ok(SourceRecentLifecycleEntryProfile::Lower { module_entry }),
        8 => {
            let activation_profile = match c.u8().map_err(|_| RecentError::InvalidEnvelope)? {
                1 => SourceRecentLifecycleActivationProfile::SourceRegistry,
                2 => SourceRecentLifecycleActivationProfile::DeclaredAnswer,
                _ => return Err(RecentError::InvalidEnvelope),
            };
            let activation_locked = match c.u8().map_err(|_| RecentError::InvalidEnvelope)? {
                0 => false,
                1 => true,
                _ => return Err(RecentError::InvalidEnvelope),
            };
            Ok(SourceRecentLifecycleEntryProfile::Main {
                module_entry,
                activation_profile,
                activation_locked,
            })
        }
        _ => Err(RecentError::InvalidEnvelope),
    }
}
impl SourceRecentLifecycleTransitionWitness {
    pub fn produce(
        before: &SourceContractReplay,
        commands: &[SourceContractCommand],
        expected: &SourceRecentLifecycleTransitionExpectation<'_>,
    ) -> RecentResult<Self> {
        recent_expected(expected)?;
        recent_endpoint(before, expected, false)?;
        if recent_count(before.head(), commands.len())? != expected.post_head.commands {
            return Err(RecentError::WrongPostHead);
        }
        let commitment = source_recent_lifecycle_transition_commands_commitment(
            expected.policy,
            expected.scope,
            commands,
        )?;
        if commitment != expected.commands_commitment {
            return Err(RecentError::WrongCommands);
        }
        let before_bytes = before.checkpoint()?;
        let mut replay = SourceContractReplay::from_checkpoint(&before_bytes, expected.pre_head)?;
        for command in commands {
            recent_apply(&mut replay, command)?;
        }
        recent_endpoint(&replay, expected, true)?;
        let witness = Self {
            policy: expected.policy,
            scope: expected.scope,
            descriptor: expected.descriptor.clone(),
            pre_head: expected.pre_head,
            pre_state_commitment: expected.pre_state_commitment,
            pre_entry: expected.pre_entry.clone(),
            before: before_bytes,
            commands: commands.to_vec(),
            commands_commitment: commitment,
            post_head: expected.post_head,
            post_state_commitment: expected.post_state_commitment,
            post_entry: expected.post_entry.clone(),
        };
        witness.canonical_bytes()?;
        Ok(witness)
    }
    pub fn policy(&self) -> SourceContractRuntimePolicy {
        self.policy
    }
    pub fn scope(&self) -> SourceRecentLifecycleTransitionScope {
        self.scope
    }
    pub fn descriptor(&self) -> &SourceRecentLifecycleTransitionDescriptor {
        &self.descriptor
    }
    pub fn pre_head(&self) -> SourceContractReplayHead {
        self.pre_head
    }
    pub fn post_head(&self) -> SourceContractReplayHead {
        self.post_head
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
    pub fn before_checkpoint(&self) -> &[u8] {
        &self.before
    }
    pub fn commands(&self) -> &[SourceContractCommand] {
        &self.commands
    }
    pub fn pre_entry(&self) -> &SourceRecentLifecycleEntryProfile {
        &self.pre_entry
    }
    pub fn post_entry(&self) -> &SourceRecentLifecycleEntryProfile {
        &self.post_entry
    }
    pub fn verify(
        &self,
        expected: &SourceRecentLifecycleTransitionExpectation<'_>,
    ) -> RecentResult<ConditionalSourceRecentLifecycleTransition> {
        recent_expected(expected)?;
        if self.policy != expected.policy {
            return Err(RecentError::WrongPolicy);
        }
        if self.scope != expected.scope {
            return Err(RecentError::WrongScope);
        }
        if &self.descriptor != expected.descriptor {
            return Err(RecentError::WrongDescriptor);
        }
        if self.pre_head != expected.pre_head {
            return Err(RecentError::WrongPreHead);
        }
        if self.pre_state_commitment != expected.pre_state_commitment {
            return Err(RecentError::WrongPreState);
        }
        if self.post_head != expected.post_head {
            return Err(RecentError::WrongPostHead);
        }
        if self.post_state_commitment != expected.post_state_commitment {
            return Err(RecentError::WrongPostState);
        }
        if &self.pre_entry != expected.pre_entry || &self.post_entry != expected.post_entry {
            return Err(RecentError::WrongEntryProfile);
        }
        if self.commands_commitment != expected.commands_commitment
            || source_recent_lifecycle_transition_commands_commitment(
                self.policy,
                self.scope,
                &self.commands,
            )? != self.commands_commitment
        {
            return Err(RecentError::WrongCommands);
        }
        if recent_count(self.pre_head, self.commands.len())? != self.post_head.commands {
            return Err(RecentError::WrongPostHead);
        }
        if self.before.get(..4) != Some(b"TLCR")
            || self.before.get(4).copied() != Some(recent_policy(expected.policy)?)
        {
            return Err(RecentError::WrongPolicy);
        }
        let mut replay = SourceContractReplay::from_checkpoint(&self.before, expected.pre_head)?;
        recent_endpoint(&replay, expected, false)?;
        for command in &self.commands {
            recent_apply(&mut replay, command)?;
        }
        recent_endpoint(&replay, expected, true)?;
        Ok(ConditionalSourceRecentLifecycleTransition {
            replay,
            scope: self.scope,
            pre_head: self.pre_head,
            pre_state_commitment: self.pre_state_commitment,
            commands_commitment: self.commands_commitment,
            post_state_commitment: self.post_state_commitment,
        })
    }
    pub fn canonical_bytes(&self) -> RecentResult<Vec<u8>> {
        let policy = recent_policy(self.policy)?;
        recent_descriptor(&self.descriptor)?;
        recent_entry_valid(self.policy, &self.pre_entry)?;
        recent_entry_valid(self.policy, &self.post_entry)?;
        if self.before.len() > MAX_CHECKPOINT_BYTES {
            return Err(RecentError::Capacity);
        }
        if recent_count(self.pre_head, self.commands.len())? != self.post_head.commands {
            return Err(RecentError::WrongPostHead);
        }
        if source_recent_lifecycle_transition_commands_commitment(
            self.policy,
            self.scope,
            &self.commands,
        )? != self.commands_commitment
        {
            return Err(RecentError::WrongCommands);
        }
        let mut bytes = RECENT_MAGIC.to_vec();
        bytes.extend_from_slice(&[1, 3, self.scope.tag(), policy]);
        bytes.extend_from_slice(SOURCE_PIN);
        for field in [
            &self.descriptor.boundary,
            &self.descriptor.numeric_domain,
            &self.descriptor.dependency_domain,
        ] {
            bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
            bytes.extend_from_slice(field.as_bytes());
        }
        recent_put_entry(&self.pre_entry, &mut bytes);
        recent_put_entry(&self.post_entry, &mut bytes);
        bytes.extend_from_slice(&self.pre_head.commands.to_be_bytes());
        bytes.extend_from_slice(&self.pre_head.commitment);
        bytes.extend_from_slice(&self.pre_state_commitment);
        bytes.extend_from_slice(&(self.before.len() as u32).to_be_bytes());
        if bytes
            .len()
            .saturating_add(self.before.len())
            .saturating_add(4)
            > MAX_SOURCE_RECENT_LIFECYCLE_TRANSITION_BYTES
        {
            return Err(RecentError::Capacity);
        }
        bytes.extend_from_slice(&self.before);
        bytes.extend_from_slice(&(self.commands.len() as u32).to_be_bytes());
        for command in &self.commands {
            put_frame(&mut bytes, &diagnostic(&command.value())?)?;
        }
        bytes.extend_from_slice(&self.commands_commitment);
        bytes.extend_from_slice(&self.post_head.commands.to_be_bytes());
        bytes.extend_from_slice(&self.post_head.commitment);
        bytes.extend_from_slice(&self.post_state_commitment);
        let mut digest = RECENT_WITNESS_DOMAIN.to_vec();
        digest.extend_from_slice(&bytes);
        bytes.extend_from_slice(&sha256(&digest));
        if bytes.len().saturating_add(4) > MAX_SOURCE_RECENT_LIFECYCLE_TRANSITION_BYTES {
            return Err(RecentError::Capacity);
        }
        with_crc(bytes).map_err(|_| RecentError::Capacity)
    }
    pub fn verify_bytes(
        bytes: &[u8],
        expected: &SourceRecentLifecycleTransitionExpectation<'_>,
    ) -> RecentResult<ConditionalSourceRecentLifecycleTransition> {
        recent_expected(expected)?;
        if bytes.len() > MAX_SOURCE_RECENT_LIFECYCLE_TRANSITION_BYTES {
            return Err(RecentError::Capacity);
        }
        let raw = verify_crc(bytes).map_err(|_| RecentError::InvalidEnvelope)?;
        let bad = |_| RecentError::InvalidEnvelope;
        let mut c = Cursor::new(raw);
        if c.take(5).map_err(bad)? != RECENT_MAGIC
            || c.u8().map_err(bad)? != 1
            || c.u8().map_err(bad)? != 3
        {
            return Err(RecentError::InvalidEnvelope);
        }
        if c.u8().map_err(bad)? != expected.scope.tag() {
            return Err(RecentError::WrongScope);
        }
        if c.u8().map_err(bad)? != recent_policy(expected.policy)? {
            return Err(RecentError::WrongPolicy);
        }
        if c.take(SOURCE_PIN.len()).map_err(bad)? != SOURCE_PIN {
            return Err(RecentError::WrongDescriptor);
        }
        for field in [
            &expected.descriptor.boundary,
            &expected.descriptor.numeric_domain,
            &expected.descriptor.dependency_domain,
        ] {
            let len = u16::from_be_bytes(c.fixed::<2>().map_err(bad)?) as usize;
            if len > 256 {
                return Err(RecentError::Capacity);
            }
            if c.take(len).map_err(bad)? != field.as_bytes() {
                return Err(RecentError::WrongDescriptor);
            }
        }
        let pre_entry = recent_read_entry(&mut c)?;
        let post_entry = recent_read_entry(&mut c)?;
        if &pre_entry != expected.pre_entry || &post_entry != expected.post_entry {
            return Err(RecentError::WrongEntryProfile);
        }
        let pre_head = SourceContractReplayHead {
            commands: c.u64().map_err(bad)?,
            commitment: c.fixed::<32>().map_err(bad)?,
        };
        if pre_head != expected.pre_head {
            return Err(RecentError::WrongPreHead);
        }
        let pre_state_commitment = c.fixed::<32>().map_err(bad)?;
        if pre_state_commitment != expected.pre_state_commitment {
            return Err(RecentError::WrongPreState);
        }
        let before_len = c.u32().map_err(bad)? as usize;
        if before_len > MAX_CHECKPOINT_BYTES {
            return Err(RecentError::Capacity);
        }
        let before = c.take(before_len).map_err(bad)?;
        if before.get(..4) != Some(b"TLCR")
            || before.get(4).copied() != Some(recent_policy(expected.policy)?)
        {
            return Err(RecentError::WrongPolicy);
        }
        let count = c.u32().map_err(bad)? as usize;
        let post_count = recent_count(pre_head, count)?;
        let mut commands = Vec::new();
        for _ in 0..count {
            let command = SourceContractCommand::from_value(&read_frame(&mut c)?)?;
            recent_command(expected.policy, expected.scope, &command)?;
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
        let mut digest_bytes = RECENT_WITNESS_DOMAIN.to_vec();
        digest_bytes.extend_from_slice(&raw[..raw.len().saturating_sub(32)]);
        if sha256(&digest_bytes) != digest {
            return Err(RecentError::InvalidEnvelope);
        }
        if post_head.commands != post_count || post_head != expected.post_head {
            return Err(RecentError::WrongPostHead);
        }
        if post_state_commitment != expected.post_state_commitment {
            return Err(RecentError::WrongPostState);
        }
        if commands_commitment != expected.commands_commitment
            || source_recent_lifecycle_transition_commands_commitment(
                expected.policy,
                expected.scope,
                &commands,
            )? != commands_commitment
        {
            return Err(RecentError::WrongCommands);
        }
        let witness = Self {
            policy: expected.policy,
            scope: expected.scope,
            descriptor: expected.descriptor.clone(),
            pre_head,
            pre_state_commitment,
            pre_entry,
            before: before.to_vec(),
            commands,
            commands_commitment,
            post_head,
            post_state_commitment,
            post_entry,
        };
        if witness.canonical_bytes()? != bytes {
            return Err(RecentError::InvalidEnvelope);
        }
        witness.verify(expected)
    }
}
