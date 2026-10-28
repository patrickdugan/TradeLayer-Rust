//! Explicit v1 control boundary around the existing source replay owner.
//!
//! Ordinary commands retain their existing V1--V12 receipts/checkpoint bytes.
//! Main.shutdown is a terminal control, not a DependencyReturn or a ledger write.
//! No host process is exited. The caller receives the typed terminal result.
use crate::codec::{verify_crc, with_crc, Cursor};
use crate::hash256::sha256;
use crate::source_contract_replay::{
    SourceContractCommand, SourceContractReceipt, SourceContractReplay, SourceContractReplayError,
    SourceContractReplayHead, MAX_CHECKPOINT_BYTES as MAX_REPLAY_BYTES, MAX_COMMANDS,
    MAX_FRAME_BYTES,
};
use crate::validity_source::{SourceRuleError, SourceValue as V};

const MAGIC: &[u8; 5] = b"TLMS\x01";
const DOMAIN: &[u8] = b"TradeLayer/SourceMainControlSession/v1/";
pub const MAIN_SOURCE_SHA256: &str =
    "006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009";
pub const MAX_CONTROL_CHECKPOINT_BYTES: usize = MAX_REPLAY_BYTES + MAX_FRAME_BYTES + 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMainControlCommandV1 {
    /// Invokes exactly the existing replay API, including its old refusal of
    /// Invoke(Main.shutdown). No legacy command is silently reinterpreted.
    Replay(SourceContractCommand),
    /// The actual pinned method ignores its receiver and all arguments. The
    /// arguments are retained as bounded control input, never inspected by it.
    MainShutdown(Vec<V>),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceMainProcessExitV1 {
    pub code: u8,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMainControlOutcomeV1 {
    Replay(Box<SourceContractReceipt>),
    Exited(SourceMainProcessExitV1),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceMainControlHeadV1 {
    /// Controls since adoption; the adopted replay retains its earlier history.
    pub controls: u64,
    pub commitment: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMainControlReceiptV1 {
    pub sequence: u64,
    pub previous_head: SourceMainControlHeadV1,
    pub outcome: SourceMainControlOutcomeV1,
    /// Ordinary effects are the original receipt's exact effects. Terminal
    /// effects are the two console messages followed immediately by exit0.
    pub effects: Vec<V>,
    pub head: SourceMainControlHeadV1,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMainControlErrorV1 {
    AlreadyExited(SourceMainProcessExitV1),
    InvalidCheckpoint,
    WrongHead,
    Capacity,
    Input(SourceRuleError),
    Replay(SourceContractReplayError),
}
impl core::fmt::Display for SourceMainControlErrorV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SourceMainControlErrorV1 {}
impl From<SourceContractReplayError> for SourceMainControlErrorV1 {
    fn from(value: SourceContractReplayError) -> Self {
        Self::Replay(value)
    }
}
type Result<T> = core::result::Result<T, SourceMainControlErrorV1>;

#[derive(Debug, PartialEq, Eq)]
struct Terminal {
    arguments: Vec<V>,
    frame: Vec<u8>,
}
/// Owns, rather than clones or replaces, the caller's actual source replay.
/// There is no mutable owner escape after exit. Immutable inspection is allowed.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceMainControlSessionV1 {
    replay: SourceContractReplay,
    adopted: SourceContractReplayHead,
    terminal: Option<Terminal>,
}
impl SourceMainControlSessionV1 {
    pub fn from_replay(replay: SourceContractReplay) -> Self {
        let adopted = replay.head();
        Self {
            replay,
            adopted,
            terminal: None,
        }
    }
    pub fn replay(&self) -> &SourceContractReplay {
        &self.replay
    }
    pub fn adopted_head(&self) -> SourceContractReplayHead {
        self.adopted
    }
    pub fn terminal(&self) -> Option<SourceMainProcessExitV1> {
        self.terminal
            .as_ref()
            .map(|_| SourceMainProcessExitV1 { code: 0 })
    }
    pub fn shutdown_arguments(&self) -> Option<&[V]> {
        self.terminal.as_ref().map(|t| t.arguments.as_slice())
    }
    pub fn ordinary_controls(&self) -> impl Iterator<Item = &SourceContractCommand> {
        self.replay.commands().skip(self.adopted.commands as usize)
    }
    pub fn head(&self) -> SourceMainControlHeadV1 {
        control_head(
            self.adopted,
            self.replay.head(),
            self.terminal.as_ref().map(|t| t.frame.as_slice()),
        )
    }
    pub fn apply(
        &mut self,
        command: SourceMainControlCommandV1,
    ) -> Result<SourceMainControlReceiptV1> {
        if let Some(exit) = self.terminal() {
            return Err(SourceMainControlErrorV1::AlreadyExited(exit));
        }
        let previous_head = self.head();
        let (outcome, effects) = match command {
            SourceMainControlCommandV1::Replay(command) => {
                let receipt = self.replay.apply(command)?;
                let effects = receipt.effects.clone();
                (
                    SourceMainControlOutcomeV1::Replay(Box::new(receipt)),
                    effects,
                )
            }
            SourceMainControlCommandV1::MainShutdown(arguments) => {
                // Validate/encode transport before executing any source prefix.
                let frame = terminal_frame(&arguments)?;
                // The pinned body has no await, this read, persistence call or
                // return before exit. Do not invoke generic Runtime dispatch:
                // it would drain post-call async work and fabricate a return.
                let effects = shutdown_effects();
                self.terminal = Some(Terminal { arguments, frame });
                (
                    SourceMainControlOutcomeV1::Exited(SourceMainProcessExitV1 { code: 0 }),
                    effects,
                )
            }
        };
        Ok(SourceMainControlReceiptV1 {
            sequence: previous_head.controls,
            previous_head,
            outcome,
            effects,
            head: self.head(),
        })
    }
    /// Embeds the original replay's canonical command checkpoint, not a
    /// financial snapshot. The optional terminal is the only control suffix.
    pub fn checkpoint(&self) -> Result<Vec<u8>> {
        let replay = self.replay.checkpoint()?;
        let mut bytes = MAGIC.to_vec();
        put_head(&mut bytes, self.adopted);
        put_head(&mut bytes, self.replay.head());
        put_frame(&mut bytes, &replay)?;
        if let Some(t) = &self.terminal {
            bytes.push(1);
            put_frame(&mut bytes, &t.frame)?;
        } else {
            bytes.push(0);
        }
        if bytes.len().saturating_add(4) > MAX_CONTROL_CHECKPOINT_BYTES {
            return Err(SourceMainControlErrorV1::Capacity);
        }
        with_crc(bytes).map_err(|_| SourceMainControlErrorV1::Capacity)
    }
    /// `expected` must be independently retained, never taken from this input.
    /// Its commitment first pins the embedded replay head and adoption head;
    /// only then is the existing replay admitted by reexecuting every command.
    /// Terminal execution is also repeated, without invoking a host exit/drain.
    pub fn from_checkpoint(bytes: &[u8], expected: SourceMainControlHeadV1) -> Result<Self> {
        if bytes.len() > MAX_CONTROL_CHECKPOINT_BYTES {
            return Err(SourceMainControlErrorV1::Capacity);
        }
        let payload = verify_crc(bytes).map_err(invalid)?;
        let mut c = Cursor::new(payload);
        if c.take(5).map_err(invalid)? != MAGIC {
            return Err(SourceMainControlErrorV1::InvalidCheckpoint);
        }
        let adopted = read_head(&mut c)?;
        let replay_head = read_head(&mut c)?;
        if adopted.commands > replay_head.commands || replay_head.commands > MAX_COMMANDS as u64 {
            return Err(SourceMainControlErrorV1::InvalidCheckpoint);
        }
        let replay_bytes = read_frame(&mut c, MAX_REPLAY_BYTES)?;
        let terminal = match c.u8().map_err(invalid)? {
            0 => None,
            1 => Some(read_frame(&mut c, MAX_FRAME_BYTES)?),
            _ => return Err(SourceMainControlErrorV1::InvalidCheckpoint),
        };
        c.end().map_err(invalid)?;
        if control_head(adopted, replay_head, terminal) != expected {
            return Err(SourceMainControlErrorV1::WrongHead);
        }
        let replay = SourceContractReplay::from_checkpoint(replay_bytes, replay_head)?;
        let actual_origin = if adopted.commands == 0 {
            replay
                .receipts()
                .next()
                .map_or(replay.head().commitment, |r| r.previous_head)
        } else {
            replay
                .receipts()
                .nth(adopted.commands as usize - 1)
                .ok_or(SourceMainControlErrorV1::InvalidCheckpoint)?
                .head
        };
        if actual_origin != adopted.commitment {
            return Err(SourceMainControlErrorV1::InvalidCheckpoint);
        }
        let mut session = Self {
            replay,
            adopted,
            terminal: None,
        };
        if let Some(frame) = terminal {
            let text = core::str::from_utf8(frame).map_err(invalid)?;
            let observed =
                crate::property_catalog::parse_diagnostic_value(text).map_err(invalid)?;
            let V::Array(arguments) = observed.get("arguments") else {
                return Err(SourceMainControlErrorV1::InvalidCheckpoint);
            };
            if terminal_frame(&arguments)? != frame {
                return Err(SourceMainControlErrorV1::InvalidCheckpoint);
            }
            session.apply(SourceMainControlCommandV1::MainShutdown(arguments))?;
        }
        if session.head() != expected || session.checkpoint()? != bytes {
            return Err(SourceMainControlErrorV1::InvalidCheckpoint);
        }
        Ok(session)
    }
}

fn invalid<T>(_: T) -> SourceMainControlErrorV1 {
    SourceMainControlErrorV1::InvalidCheckpoint
}
fn terminal_frame(arguments: &[V]) -> Result<Vec<u8>> {
    let value = V::Object(vec![
        ("arguments".into(), V::Array(arguments.to_vec())),
        ("exitCode".into(), V::number(0.)),
        ("effects".into(), V::Array(shutdown_effects())),
    ]);
    value.validate().map_err(SourceMainControlErrorV1::Input)?;
    let text = value
        .diagnostic_json()
        .map_err(SourceMainControlErrorV1::Input)?;
    if text.len() > MAX_FRAME_BYTES {
        return Err(SourceMainControlErrorV1::Capacity);
    }
    // Every accepted terminal must be reconstructible by the same bounded
    // diagnostic transport used for the ordinary command journal.
    let parsed = crate::property_catalog::parse_diagnostic_value(&text)
        .map_err(|_| SourceMainControlErrorV1::Input(SourceRuleError::InvalidInput))?;
    if parsed != value {
        return Err(SourceMainControlErrorV1::Input(
            SourceRuleError::InvalidInput,
        ));
    }
    Ok(text.into_bytes())
}
fn put_head(bytes: &mut Vec<u8>, head: SourceContractReplayHead) {
    bytes.extend_from_slice(&head.commands.to_be_bytes());
    bytes.extend_from_slice(&head.commitment);
}
fn read_head(c: &mut Cursor<'_>) -> Result<SourceContractReplayHead> {
    Ok(SourceContractReplayHead {
        commands: c.u64().map_err(invalid)?,
        commitment: c.fixed::<32>().map_err(invalid)?,
    })
}
fn put_frame(bytes: &mut Vec<u8>, frame: &[u8]) -> Result<()> {
    let len = u32::try_from(frame.len()).map_err(|_| SourceMainControlErrorV1::Capacity)?;
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(frame);
    Ok(())
}
fn read_frame<'a>(c: &mut Cursor<'a>, bound: usize) -> Result<&'a [u8]> {
    let len = c.u32().map_err(invalid)? as usize;
    if len > bound {
        return Err(SourceMainControlErrorV1::Capacity);
    }
    c.take(len).map_err(invalid)
}
fn control_head(
    adopted: SourceContractReplayHead,
    current: SourceContractReplayHead,
    terminal_frame: Option<&[u8]>,
) -> SourceMainControlHeadV1 {
    let mut bytes = DOMAIN.to_vec();
    bytes.extend_from_slice(MAIN_SOURCE_SHA256.as_bytes());
    put_head(&mut bytes, adopted);
    put_head(&mut bytes, current);
    if let Some(frame) = terminal_frame {
        bytes.push(1);
        bytes.extend_from_slice(&(frame.len() as u32).to_be_bytes());
        bytes.extend_from_slice(frame);
    } else {
        bytes.push(0);
    }
    SourceMainControlHeadV1 {
        controls: current.commands - adopted.commands + u64::from(terminal_frame.is_some()),
        commitment: sha256(&bytes),
    }
}
fn shutdown_effects() -> Vec<V> {
    let s = |v: &str| V::String(v.into());
    let object = |fields: &[(&str, V)]| {
        V::Object(
            fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    };
    let log = |message: &str| {
        object(&[
            ("kind", s("console")),
            ("level", s("log")),
            ("values", V::Array(vec![s(message)])),
        ])
    };
    vec![
        log("Saving state to database..."),
        log("Shutdown completed."),
        object(&[("kind", s("process-exit")), ("code", V::number(0.))]),
    ]
}
