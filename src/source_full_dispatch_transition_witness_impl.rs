// TLSW4 is independent of TLSW1–3. It reuses the actual sole replay owner.
pub const SOURCE_FULL_DISPATCH_TRANSITION_SCHEMA: &str =
    "tl-literal-source-full-dispatch-command-slice-v4";
pub const MAX_SOURCE_FULL_DISPATCH_TRANSITION_BYTES: usize = MAX_CHECKPOINT_BYTES;
const FULL_MAGIC: &[u8; 5] = b"TLSW\x04";
const FULL_COMMAND_DOMAIN: &[u8] = b"TradeLayer/SourceFullDispatchCommands/v4/";
const FULL_RECEIPT_DOMAIN: &[u8] = b"TradeLayer/SourceFullDispatchReceipts/v4/";
const FULL_CONTEXT_DOMAIN: &[u8] = b"TradeLayer/SourceFullDispatchContext/v4/";
const FULL_ENVELOPE_DOMAIN: &[u8] = b"TradeLayer/SourceFullDispatchWitness/v4/";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFullDispatchDescriptor {
    pub source_commit: String,
    pub boundary: String,
    pub numeric_domain: String,
    pub dependency_domain: String,
}
impl SourceFullDispatchDescriptor {
    pub fn reference() -> Self {
        Self {
            source_commit: String::from_utf8(SOURCE_PIN.to_vec()).expect("ASCII"),
            boundary: "tl-source-owned-core-command-main-boundary-v4".into(),
            numeric_domain: "tl-source-number-bits-bounded-decimal-bignumber-v1".into(),
            dependency_domain: "tl-source-declared-adapter-inputs-unverified-v4".into(),
        }
    }
    fn value(&self) -> V {
        obj(&[
            ("schema", s(SOURCE_FULL_DISPATCH_TRANSITION_SCHEMA)),
            ("sourceCommit", s(&self.source_commit)),
            ("boundary", s(&self.boundary)),
            ("numericDomain", s(&self.numeric_domain)),
            ("dependencyDomain", s(&self.dependency_domain)),
        ])
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFullDispatchScope {
    SourceCommandSlice,
    MainCallableSlice,
}
impl SourceFullDispatchScope {
    fn tag(self) -> u8 {
        match self {
            Self::SourceCommandSlice => 1,
            Self::MainCallableSlice => 2,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceFullDispatchError {
    Capacity,
    InvalidEnvelope,
    WrongPolicy,
    WrongScope,
    WrongDescriptor,
    WrongPreHead,
    WrongPostHead,
    WrongPreState,
    WrongPostState,
    WrongPreContext,
    WrongPostContext,
    WrongCommands,
    WrongReceipts,
    UnsupportedCommand,
    UnavailableExecution,
    UnavailableHistory,
    InconsistentStateProjection(String),
    Replay(SourceContractReplayError),
}
impl std::fmt::Display for SourceFullDispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SourceFullDispatchError {}
impl From<SourceContractReplayError> for SourceFullDispatchError {
    fn from(e: SourceContractReplayError) -> Self {
        Self::Replay(e)
    }
}
type FullResult<T> = std::result::Result<T, SourceFullDispatchError>;
use SourceFullDispatchError as FullError;
/// Every endpoint/result pin is mandatory and independently retained. Reading
/// these pins from the same untrusted witness supplies no independent evidence.
pub struct SourceFullDispatchExpectation<'a> {
    pub policy: SourceContractRuntimePolicy,
    pub scope: SourceFullDispatchScope,
    pub descriptor: &'a SourceFullDispatchDescriptor,
    pub pre_head: SourceContractReplayHead,
    pub pre_state_commitment: [u8; 32],
    pub pre_context_commitment: [u8; 32],
    pub commands_commitment: [u8; 32],
    pub receipts_commitment: [u8; 32],
    pub post_head: SourceContractReplayHead,
    pub post_state_commitment: [u8; 32],
    pub post_context_commitment: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFullDispatchTransitionWitness {
    policy: SourceContractRuntimePolicy,
    scope: SourceFullDispatchScope,
    descriptor: SourceFullDispatchDescriptor,
    pre_head: SourceContractReplayHead,
    pre_state: [u8; 32],
    pre_context: V,
    pre_context_commitment: [u8; 32],
    before: Vec<u8>,
    entries: Vec<Entry>,
    commands_commitment: [u8; 32],
    receipts_commitment: [u8; 32],
    post_head: SourceContractReplayHead,
    post_state: [u8; 32],
    post_context: V,
    post_context_commitment: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalSourceFullDispatchTransition {
    replay: SourceContractReplay,
    scope: SourceFullDispatchScope,
    pre_head: SourceContractReplayHead,
    pre_state: [u8; 32],
    pre_context: [u8; 32],
    commands: [u8; 32],
    receipts: [u8; 32],
    post_context: [u8; 32],
}
impl ConditionalSourceFullDispatchTransition {
    pub fn claim(&self) -> &'static str {
        match self.scope {
            SourceFullDispatchScope::SourceCommandSlice => {
                "conditional-source-command-slice-only-v4"
            }
            SourceFullDispatchScope::MainCallableSlice => {
                "conditional-source-main-callable-slice-only-v4"
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
        self.pre_state
    }
    pub fn pre_context_commitment(&self) -> [u8; 32] {
        self.pre_context
    }
    pub fn post_context_commitment(&self) -> [u8; 32] {
        self.post_context
    }
    pub fn commands_commitment(&self) -> [u8; 32] {
        self.commands
    }
    pub fn receipts_commitment(&self) -> [u8; 32] {
        self.receipts
    }
    pub fn receipts(&self) -> impl Iterator<Item = &SourceContractReceipt> {
        self.replay.receipts().skip(self.pre_head.commands as usize)
    }
    pub fn require_authenticated_input_and_state(
        &self,
    ) -> std::result::Result<(), SourceRuleError> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }
}
fn full_policy(p: SourceContractRuntimePolicy) -> FullResult<u8> {
    match p {
        SourceContractRuntimePolicy::SpotLifecycleV9 => Ok(9),
        SourceContractRuntimePolicy::ActivationLifecycleV10 => Ok(10),
        SourceContractRuntimePolicy::AmmLifecycleV11 => Ok(11),
        _ => Err(FullError::WrongPolicy),
    }
}
fn full_descriptor(d: &SourceFullDispatchDescriptor) -> FullResult<()> {
    if d != &SourceFullDispatchDescriptor::reference() {
        return Err(FullError::WrongDescriptor);
    }
    Ok(())
}
fn full_main_call(call: &DependencyCall) -> bool {
    match call.target.as_str() {
        "Main.processTxSet" | "Main.processTx" | "Main.blockHandlerMid" => {
            call.arguments.len() == 2
        }
        "Main.loadMaxProcessedHeight" | "Main.loadTrackHeight" => call.arguments.is_empty(),
        "Main.saveMaxProcessedHeight" | "Main.saveTrackHeight" => call.arguments.len() == 1,
        _ => false,
    }
}
fn full_command(
    p: SourceContractRuntimePolicy,
    scope: SourceFullDispatchScope,
    c: &SourceContractCommand,
) -> FullResult<()> {
    full_policy(p)?;
    let admitted = match c {
        SourceContractCommand::SetHeight(_) => true,
        SourceContractCommand::FailStorage(_) => {
            scope == SourceFullDispatchScope::SourceCommandSlice
        }
        SourceContractCommand::Invoke(call)
            if scope == SourceFullDispatchScope::MainCallableSlice =>
        {
            full_main_call(call)
        }
        SourceContractCommand::Invoke(call) => {
            call.arguments.len() <= 32
                && ([
                    "Logic.",
                    "Tally.",
                    "Property.",
                    "ContractRegistry.",
                    "Contract.",
                    "AMMPool.",
                    "Insurance.",
                    "Vesting.",
                    "Activation.",
                    "Consensus.",
                    "ClearList.",
                    "Clearlist.",
                    "Oracle.",
                    "Channels.",
                    "Clearing.",
                    "MarginMap.",
                    "Orderbook.",
                ]
                .iter()
                .any(|prefix| call.target.starts_with(prefix)))
        }
        SourceContractCommand::EvaluateQueue(call) => {
            scope == SourceFullDispatchScope::SourceCommandSlice && call.arguments.len() <= 16
        }
        SourceContractCommand::ExecuteContract(args) => {
            scope == SourceFullDispatchScope::SourceCommandSlice
                && args.len() == 9
                && (0..36).any(|kind| args.get(1) == Some(&V::number(f64::from(kind))))
        }
        _ => false,
    };
    if !admitted {
        return Err(FullError::UnsupportedCommand);
    }
    if SourceContractCommand::from_value(&c.value())? != *c {
        return Err(FullError::InvalidEnvelope);
    }
    diagnostic(&c.value())?;
    Ok(())
}
/// Actual reconstructed adapter/scheduler profiles, not caller-supplied facts.
/// This value is not an independent pin if obtained from the witness itself.
pub fn source_full_dispatch_context(
    replay: &SourceContractReplay,
    scope: SourceFullDispatchScope,
) -> FullResult<V> {
    full_policy(replay.runtime().policy())?;
    let state = replay
        .runtime()
        .state_value()
        .map_err(SourceContractReplayError::Runtime)?;
    let main = state.get("mainCallable");
    if main.get("profile") != s("TransactionsAndMidV8") {
        return Err(FullError::WrongPreContext);
    }
    if scope == SourceFullDispatchScope::MainCallableSlice
        && main.get("activationProfile") != s("source-registry")
    {
        return Err(FullError::WrongPreContext);
    }
    let entry = state.get("continuation").get("entry");
    if !matches!(&entry,V::String(v)if ["main-subgraph","types-first","tally-first","volume-first"].contains(&v.as_str()))
    {
        return Err(FullError::WrongPreContext);
    }
    let activation = state.get("activationLifecycle");
    let spot = state.get("spotLifecycle");
    Ok(obj(&[
        ("policy", state.get("runtimePolicy")),
        ("moduleEntry", entry),
        ("mainProfile", main.get("profile")),
        ("activationProfile", main.get("activationProfile")),
        ("activationLocked", main.get("activationLocked")),
        ("clientProfile", activation.get("clientProfile")),
        ("clientLocked", activation.get("clientLocked")),
        ("activationAdapter", activation.get("profile")),
        ("ammAdapter", state.get("ammLifecycle").get("profile")),
        ("proofBoundary", state.get("proofLifecycle")),
        (
            "pending",
            obj(&[
                ("mainHeight", main.get("pendingHeight")),
                ("sendRead", state.get("sendCommit").get("pendingRead")),
                ("spot", spot),
                ("activationAwaits", activation.get("awaitState")),
                ("insurance", activation.get("pendingInsurance")),
                ("clearlists", activation.get("pendingClearlists")),
            ]),
        ),
    ]))
}
fn full_context_digest(
    policy: SourceContractRuntimePolicy,
    scope: SourceFullDispatchScope,
    value: &V,
) -> FullResult<[u8; 32]> {
    let mut b = FULL_CONTEXT_DOMAIN.to_vec();
    b.extend([full_policy(policy)?, scope.tag()]);
    put_frame(&mut b, &diagnostic(value)?)?;
    Ok(sha256(&b))
}
pub fn source_full_dispatch_context_commitment(
    replay: &SourceContractReplay,
    scope: SourceFullDispatchScope,
) -> FullResult<[u8; 32]> {
    full_context_digest(
        replay.runtime().policy(),
        scope,
        &source_full_dispatch_context(replay, scope)?,
    )
}
pub fn source_full_dispatch_state_commitment(
    replay: &SourceContractReplay,
) -> FullResult<[u8; 32]> {
    full_policy(replay.runtime().policy())?;
    Ok(state_commitment(replay.runtime())?)
}
pub fn source_full_dispatch_commands_commitment(
    policy: SourceContractRuntimePolicy,
    scope: SourceFullDispatchScope,
    commands: &[SourceContractCommand],
) -> FullResult<[u8; 32]> {
    if commands.is_empty() || commands.len() > MAX_COMMANDS {
        return Err(FullError::Capacity);
    }
    if !commands.iter().any(|c| {
        matches!(
            c,
            SourceContractCommand::Invoke(_)
                | SourceContractCommand::EvaluateQueue(_)
                | SourceContractCommand::ExecuteContract(_)
        )
    }) {
        return Err(FullError::UnsupportedCommand);
    }
    let mut b = FULL_COMMAND_DOMAIN.to_vec();
    b.extend([full_policy(policy)?, scope.tag()]);
    b.extend((commands.len() as u32).to_be_bytes());
    for c in commands {
        full_command(policy, scope, c)?;
        put_frame(&mut b, &diagnostic(&c.value())?)?;
    }
    Ok(sha256(&b))
}
fn full_put_receipt(out: &mut Vec<u8>, r: &SourceContractReceipt) -> FullResult<()> {
    out.extend(r.sequence.to_be_bytes());
    out.extend(r.previous_head);
    put_frame(out, &diagnostic(&r.outcome.value())?)?;
    if r.effects.len() > MAX_EFFECTS {
        return Err(FullError::Capacity);
    }
    out.extend((r.effects.len() as u32).to_be_bytes());
    for v in &r.effects {
        put_frame(out, &diagnostic(v)?)?;
    }
    out.extend(r.state_commitment);
    out.extend(r.effects_commitment);
    out.extend(r.head);
    Ok(())
}
fn full_read_receipt(c: &mut Cursor<'_>) -> FullResult<SourceContractReceipt> {
    let bad = |_| FullError::InvalidEnvelope;
    let sequence = c.u64().map_err(bad)?;
    let previous_head = c.fixed().map_err(bad)?;
    let outcome = SourceContractCommandOutcome::from_value(&read_frame(c)?)?;
    let count = c.u32().map_err(bad)? as usize;
    if count > MAX_EFFECTS {
        return Err(FullError::Capacity);
    }
    let mut effects = Vec::with_capacity(count);
    for _ in 0..count {
        effects.push(read_frame(c)?);
    }
    Ok(SourceContractReceipt {
        sequence,
        previous_head,
        outcome,
        effects,
        state_commitment: c.fixed().map_err(bad)?,
        effects_commitment: c.fixed().map_err(bad)?,
        head: c.fixed().map_err(bad)?,
    })
}
pub fn source_full_dispatch_receipts_commitment(
    policy: SourceContractRuntimePolicy,
    scope: SourceFullDispatchScope,
    receipts: &[SourceContractReceipt],
) -> FullResult<[u8; 32]> {
    if receipts.is_empty() || receipts.len() > MAX_COMMANDS {
        return Err(FullError::Capacity);
    }
    let mut b = FULL_RECEIPT_DOMAIN.to_vec();
    b.extend([full_policy(policy)?, scope.tag()]);
    b.extend((receipts.len() as u32).to_be_bytes());
    for r in receipts {
        full_put_receipt(&mut b, r)?;
    }
    Ok(sha256(&b))
}
fn full_count(pre: SourceContractReplayHead, count: usize) -> FullResult<u64> {
    pre.commands
        .checked_add(count as u64)
        .filter(|n| *n <= MAX_COMMANDS as u64 && count <= MAX_COMMANDS)
        .ok_or(FullError::Capacity)
}
fn full_expected(e: &SourceFullDispatchExpectation<'_>) -> FullResult<()> {
    full_policy(e.policy)?;
    full_descriptor(e.descriptor)?;
    if e.pre_head.commands > MAX_COMMANDS as u64 || e.post_head.commands > MAX_COMMANDS as u64 {
        return Err(FullError::Capacity);
    }
    Ok(())
}

/// Compare each enumerated table with the runtime's authoritative collection.
/// This does not prove that the enumeration contains every collection.
pub fn source_full_dispatch_projection_consistency(
    replay: &SourceContractReplay,
) -> FullResult<()> {
    let state = replay
        .runtime()
        .state_value()
        .map_err(SourceContractReplayError::Runtime)?;
    let V::Array(tables) = state.get("tables") else {
        return Err(FullError::InconsistentStateProjection("<tables>".into()));
    };
    let mut seen = std::collections::BTreeSet::new();
    for table in tables {
        let V::String(name) = table.get("name") else {
            return Err(FullError::InconsistentStateProjection("<name>".into()));
        };
        let rows = table.get("rows");
        if !seen.insert(name.clone())
            || rows != V::Array(replay.runtime().collection(&name).to_vec())
        {
            return Err(FullError::InconsistentStateProjection(name));
        }
    }
    Ok(())
}

fn full_endpoint(
    replay: &SourceContractReplay,
    e: &SourceFullDispatchExpectation<'_>,
    post: bool,
) -> FullResult<V> {
    source_full_dispatch_projection_consistency(replay)?;
    if replay.runtime().policy() != e.policy {
        return Err(FullError::WrongPolicy);
    }
    if replay.head() != if post { e.post_head } else { e.pre_head } {
        return Err(if post {
            FullError::WrongPostHead
        } else {
            FullError::WrongPreHead
        });
    }
    if state_commitment(replay.runtime())?
        != if post {
            e.post_state_commitment
        } else {
            e.pre_state_commitment
        }
    {
        return Err(if post {
            FullError::WrongPostState
        } else {
            FullError::WrongPreState
        });
    }
    let context = source_full_dispatch_context(replay, e.scope).map_err(|error| {
        if post && error == FullError::WrongPreContext {
            FullError::WrongPostContext
        } else {
            error
        }
    })?;
    if full_context_digest(e.policy, e.scope, &context)?
        != if post {
            e.post_context_commitment
        } else {
            e.pre_context_commitment
        }
    {
        return Err(if post {
            FullError::WrongPostContext
        } else {
            FullError::WrongPreContext
        });
    }
    Ok(context)
}
fn full_owned_history(replay: &SourceContractReplay) -> FullResult<()> {
    if replay
        .receipts()
        .any(|r| matches!(r.outcome, SourceContractCommandOutcome::LocalError(_)))
    {
        return Err(FullError::UnavailableHistory);
    }
    Ok(())
}
fn full_apply(
    replay: &mut SourceContractReplay,
    command: &SourceContractCommand,
) -> FullResult<SourceContractReceipt> {
    let r = replay.apply(command.clone())?;
    if matches!(r.outcome, SourceContractCommandOutcome::LocalError(_)) {
        return Err(FullError::UnavailableExecution);
    }
    Ok(r)
}
fn full_put_head(b: &mut Vec<u8>, h: SourceContractReplayHead) {
    b.extend(h.commands.to_be_bytes());
    b.extend(h.commitment);
}
fn full_read_head(c: &mut Cursor<'_>) -> FullResult<SourceContractReplayHead> {
    let bad = |_| FullError::InvalidEnvelope;
    Ok(SourceContractReplayHead {
        commands: c.u64().map_err(bad)?,
        commitment: c.fixed().map_err(bad)?,
    })
}
impl SourceFullDispatchTransitionWitness {
    pub fn produce(
        before: &SourceContractReplay,
        commands: &[SourceContractCommand],
        expected: &SourceFullDispatchExpectation<'_>,
    ) -> FullResult<Self> {
        full_expected(expected)?;
        full_owned_history(before)?;
        let commands_commitment =
            source_full_dispatch_commands_commitment(expected.policy, expected.scope, commands)?;
        if commands_commitment != expected.commands_commitment {
            return Err(FullError::WrongCommands);
        }
        if full_count(expected.pre_head, commands.len())? != expected.post_head.commands {
            return Err(FullError::WrongPostHead);
        }
        let before_bytes = before.checkpoint()?;
        if before_bytes.get(4).copied() != Some(full_policy(expected.policy)?) {
            return Err(FullError::WrongPolicy);
        }
        let mut replay = SourceContractReplay::from_checkpoint(&before_bytes, expected.pre_head)?;
        full_owned_history(&replay)?;
        let pre_context = full_endpoint(&replay, expected, false)?;
        let mut entries = Vec::with_capacity(commands.len());
        for command in commands {
            let receipt = full_apply(&mut replay, command)?;
            entries.push(Entry {
                command: command.clone(),
                receipt,
            });
        }
        let post_context = full_endpoint(&replay, expected, true)?;
        let receipts = entries
            .iter()
            .map(|e| e.receipt.clone())
            .collect::<Vec<_>>();
        let receipts_commitment =
            source_full_dispatch_receipts_commitment(expected.policy, expected.scope, &receipts)?;
        if receipts_commitment != expected.receipts_commitment {
            return Err(FullError::WrongReceipts);
        }
        let witness = Self {
            policy: expected.policy,
            scope: expected.scope,
            descriptor: expected.descriptor.clone(),
            pre_head: expected.pre_head,
            pre_state: expected.pre_state_commitment,
            pre_context,
            pre_context_commitment: expected.pre_context_commitment,
            before: before_bytes,
            entries,
            commands_commitment,
            receipts_commitment,
            post_head: expected.post_head,
            post_state: expected.post_state_commitment,
            post_context,
            post_context_commitment: expected.post_context_commitment,
        };
        witness.canonical_bytes()?;
        Ok(witness)
    }
    pub fn pre_head(&self) -> SourceContractReplayHead {
        self.pre_head
    }
    pub fn post_head(&self) -> SourceContractReplayHead {
        self.post_head
    }
    pub fn commands(&self) -> impl Iterator<Item = &SourceContractCommand> {
        self.entries.iter().map(|e| &e.command)
    }
    pub fn claimed_receipts(&self) -> impl Iterator<Item = &SourceContractReceipt> {
        self.entries.iter().map(|e| &e.receipt)
    }
    pub fn before_checkpoint(&self) -> &[u8] {
        &self.before
    }
    pub fn pre_context(&self) -> &V {
        &self.pre_context
    }
    pub fn post_context(&self) -> &V {
        &self.post_context
    }
    pub fn verify(
        &self,
        e: &SourceFullDispatchExpectation<'_>,
    ) -> FullResult<ConditionalSourceFullDispatchTransition> {
        full_expected(e)?;
        if self.policy != e.policy {
            return Err(FullError::WrongPolicy);
        }
        if self.scope != e.scope {
            return Err(FullError::WrongScope);
        }
        if &self.descriptor != e.descriptor {
            return Err(FullError::WrongDescriptor);
        }
        if self.pre_head != e.pre_head {
            return Err(FullError::WrongPreHead);
        }
        if self.post_head != e.post_head {
            return Err(FullError::WrongPostHead);
        }
        if self.pre_state != e.pre_state_commitment {
            return Err(FullError::WrongPreState);
        }
        if self.post_state != e.post_state_commitment {
            return Err(FullError::WrongPostState);
        }
        if self.pre_context_commitment != e.pre_context_commitment
            || full_context_digest(self.policy, self.scope, &self.pre_context)?
                != e.pre_context_commitment
        {
            return Err(FullError::WrongPreContext);
        }
        if self.post_context_commitment != e.post_context_commitment
            || full_context_digest(self.policy, self.scope, &self.post_context)?
                != e.post_context_commitment
        {
            return Err(FullError::WrongPostContext);
        }
        let commands = self.commands().cloned().collect::<Vec<_>>();
        let receipts = self.claimed_receipts().cloned().collect::<Vec<_>>();
        if self.commands_commitment != e.commands_commitment
            || source_full_dispatch_commands_commitment(self.policy, self.scope, &commands)?
                != e.commands_commitment
        {
            return Err(FullError::WrongCommands);
        }
        if self.receipts_commitment != e.receipts_commitment
            || source_full_dispatch_receipts_commitment(self.policy, self.scope, &receipts)?
                != e.receipts_commitment
        {
            return Err(FullError::WrongReceipts);
        }
        if full_count(self.pre_head, self.entries.len())? != self.post_head.commands {
            return Err(FullError::WrongPostHead);
        }
        if self.before.get(..4) != Some(b"TLCR")
            || self.before.get(4).copied() != Some(full_policy(self.policy)?)
        {
            return Err(FullError::WrongPolicy);
        }
        let mut replay = SourceContractReplay::from_checkpoint(&self.before, e.pre_head)?;
        full_owned_history(&replay)?;
        if full_endpoint(&replay, e, false)? != self.pre_context {
            return Err(FullError::WrongPreContext);
        }
        for entry in &self.entries {
            if full_apply(&mut replay, &entry.command)? != entry.receipt {
                return Err(FullError::WrongReceipts);
            }
        }
        if full_endpoint(&replay, e, true)? != self.post_context {
            return Err(FullError::WrongPostContext);
        }
        Ok(ConditionalSourceFullDispatchTransition {
            replay,
            scope: self.scope,
            pre_head: self.pre_head,
            pre_state: self.pre_state,
            pre_context: self.pre_context_commitment,
            commands: self.commands_commitment,
            receipts: self.receipts_commitment,
            post_context: self.post_context_commitment,
        })
    }
    pub fn canonical_bytes(&self) -> FullResult<Vec<u8>> {
        full_policy(self.policy)?;
        full_descriptor(&self.descriptor)?;
        if self.before.len() > MAX_CHECKPOINT_BYTES
            || self.entries.is_empty()
            || full_count(self.pre_head, self.entries.len())? != self.post_head.commands
        {
            return Err(FullError::Capacity);
        }
        let commands = self.commands().cloned().collect::<Vec<_>>();
        let receipts = self.claimed_receipts().cloned().collect::<Vec<_>>();
        if source_full_dispatch_commands_commitment(self.policy, self.scope, &commands)?
            != self.commands_commitment
        {
            return Err(FullError::WrongCommands);
        }
        if source_full_dispatch_receipts_commitment(self.policy, self.scope, &receipts)?
            != self.receipts_commitment
        {
            return Err(FullError::WrongReceipts);
        }
        if full_context_digest(self.policy, self.scope, &self.pre_context)?
            != self.pre_context_commitment
        {
            return Err(FullError::WrongPreContext);
        }
        if full_context_digest(self.policy, self.scope, &self.post_context)?
            != self.post_context_commitment
        {
            return Err(FullError::WrongPostContext);
        }
        let mut b = FULL_MAGIC.to_vec();
        b.extend([full_policy(self.policy)?, self.scope.tag()]);
        put_frame(&mut b, &diagnostic(&self.descriptor.value())?)?;
        full_put_head(&mut b, self.pre_head);
        b.extend(self.pre_state);
        put_frame(&mut b, &diagnostic(&self.pre_context)?)?;
        b.extend(self.pre_context_commitment);
        b.extend((self.before.len() as u32).to_be_bytes());
        if b.len().saturating_add(self.before.len()).saturating_add(4)
            > MAX_SOURCE_FULL_DISPATCH_TRANSITION_BYTES
        {
            return Err(FullError::Capacity);
        }
        b.extend(&self.before);
        b.extend((self.entries.len() as u32).to_be_bytes());
        for entry in &self.entries {
            put_frame(&mut b, &diagnostic(&entry.command.value())?)?;
            full_put_receipt(&mut b, &entry.receipt)?;
        }
        b.extend(self.commands_commitment);
        b.extend(self.receipts_commitment);
        full_put_head(&mut b, self.post_head);
        b.extend(self.post_state);
        put_frame(&mut b, &diagnostic(&self.post_context)?)?;
        b.extend(self.post_context_commitment);
        let mut digest = FULL_ENVELOPE_DOMAIN.to_vec();
        digest.extend(&b);
        b.extend(sha256(&digest));
        if b.len().saturating_add(4) > MAX_SOURCE_FULL_DISPATCH_TRANSITION_BYTES {
            return Err(FullError::Capacity);
        }
        with_crc(b).map_err(|_| FullError::Capacity)
    }
    pub fn verify_bytes(
        bytes: &[u8],
        expected: &SourceFullDispatchExpectation<'_>,
    ) -> FullResult<ConditionalSourceFullDispatchTransition> {
        full_expected(expected)?;
        if bytes.len() > MAX_SOURCE_FULL_DISPATCH_TRANSITION_BYTES {
            return Err(FullError::Capacity);
        }
        let raw = verify_crc(bytes).map_err(|_| FullError::InvalidEnvelope)?;
        let split = raw
            .len()
            .checked_sub(32)
            .ok_or(FullError::InvalidEnvelope)?;
        let (payload, digest) = raw.split_at(split);
        let mut digest_input = FULL_ENVELOPE_DOMAIN.to_vec();
        digest_input.extend(payload);
        if sha256(&digest_input).as_slice() != digest {
            return Err(FullError::InvalidEnvelope);
        }
        let bad = |_| FullError::InvalidEnvelope;
        let mut c = Cursor::new(payload);
        if c.take(5).map_err(bad)? != FULL_MAGIC {
            return Err(FullError::InvalidEnvelope);
        }
        if c.u8().map_err(bad)? != full_policy(expected.policy)? {
            return Err(FullError::WrongPolicy);
        }
        if c.u8().map_err(bad)? != expected.scope.tag() {
            return Err(FullError::WrongScope);
        }
        if read_frame(&mut c)? != expected.descriptor.value() {
            return Err(FullError::WrongDescriptor);
        }
        let pre_head = full_read_head(&mut c)?;
        if pre_head != expected.pre_head {
            return Err(FullError::WrongPreHead);
        }
        let pre_state = c.fixed().map_err(bad)?;
        if pre_state != expected.pre_state_commitment {
            return Err(FullError::WrongPreState);
        }
        let pre_context = read_frame(&mut c)?;
        let pre_context_commitment = c.fixed().map_err(bad)?;
        if pre_context_commitment != expected.pre_context_commitment
            || full_context_digest(expected.policy, expected.scope, &pre_context)?
                != pre_context_commitment
        {
            return Err(FullError::WrongPreContext);
        }
        let before_len = c.u32().map_err(bad)? as usize;
        if before_len > MAX_CHECKPOINT_BYTES {
            return Err(FullError::Capacity);
        }
        let before = c.take(before_len).map_err(bad)?.to_vec();
        let count = c.u32().map_err(bad)? as usize;
        if count == 0 || full_count(pre_head, count)? != expected.post_head.commands {
            return Err(FullError::WrongPostHead);
        }
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let command = SourceContractCommand::from_value(&read_frame(&mut c)?)?;
            full_command(expected.policy, expected.scope, &command)?;
            let receipt = full_read_receipt(&mut c)?;
            entries.push(Entry { command, receipt });
        }
        let commands_commitment = c.fixed().map_err(bad)?;
        let receipts_commitment = c.fixed().map_err(bad)?;
        let post_head = full_read_head(&mut c)?;
        let post_state = c.fixed().map_err(bad)?;
        let post_context = read_frame(&mut c)?;
        let post_context_commitment = c.fixed().map_err(bad)?;
        c.end().map_err(bad)?;
        let witness = Self {
            policy: expected.policy,
            scope: expected.scope,
            descriptor: expected.descriptor.clone(),
            pre_head,
            pre_state,
            pre_context,
            pre_context_commitment,
            before,
            entries,
            commands_commitment,
            receipts_commitment,
            post_head,
            post_state,
            post_context,
            post_context_commitment,
        };
        if witness.canonical_bytes()?.as_slice() != bytes {
            return Err(FullError::InvalidEnvelope);
        }
        witness.verify(expected)
    }
}
