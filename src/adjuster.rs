//! Bounded tick/minimum-size metadata derived from the pinned adjuster source.
//!
//! `AtomicMetadataV1` retains finite binary64 calculations and event order while
//! committing state, actions and events together. It does not persist through
//! the source's broken database call or apply these values to an orderbook.

use crate::codec::{verify_crc, with_crc, Cursor};
use core::fmt;

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_LF_SHA256: &str =
    "b6eea50581a9cc19f2296bae180b528c0116900a73662e5ce9a8ce762c36f77c";
pub const MAX_ADJUSTER_ACTIONS: usize = 8192;
pub const MAX_ADJUSTER_EVENTS: usize = 4096;
pub const MAX_EVENT_TYPE_BYTES: usize = 64;
pub const MAX_EVENT_TIME_MILLIS: u64 = 8_640_000_000_000_000;
pub const MAX_ADJUSTER_CHECKPOINT_BYTES: usize = 2 * 1024 * 1024;
const MAGIC: &[u8; 5] = b"TLAD\x01";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjusterPolicy {
    AtomicMetadataV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjusterError {
    NonFiniteInput,
    ArithmeticOverflow,
    InvalidTime,
    EventTypeTooLong,
    ActionLimit,
    EventLimit,
    InvalidCheckpoint,
}
impl fmt::Display for AdjusterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AdjusterError {}
type Result<T> = core::result::Result<T, AdjusterError>;

#[derive(Debug, Clone, PartialEq)]
pub struct AdjusterState {
    pub tick_size: f64,
    pub min_contract_size: f64,
    pub notional_threshold: f64,
    pub notional_multiplier: f64,
    pub doubling_interval: u32,
    pub consecutive_doublings: u32,
}
impl Default for AdjusterState {
    fn default() -> Self {
        Self {
            tick_size: 0.1,
            min_contract_size: 1.0,
            notional_threshold: 2.0,
            notional_multiplier: 10.0,
            doubling_interval: 3,
            consecutive_doublings: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdjustmentEvent {
    pub sequence: u64,
    pub event_type: String,
    pub value: f64,
    /// Explicit caller data. Sequence determines order even if clocks regress.
    pub time_millis: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AdjustmentAction {
    AdjustTickSize {
        notional_value: f64,
        time_millis: u64,
    },
    IncreaseNotionalSize {
        time_millis: u64,
    },
    /// The source method can append any named finite observation without
    /// changing the calculator. This action does not authorize a tick change.
    SaveEvent {
        event_type: String,
        value: f64,
        time_millis: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TickSizeAdjuster {
    state: AdjusterState,
    actions: Vec<AdjustmentAction>,
    events: Vec<AdjustmentEvent>,
}
impl Default for TickSizeAdjuster {
    fn default() -> Self {
        Self::new()
    }
}
impl TickSizeAdjuster {
    pub fn new() -> Self {
        Self {
            state: AdjusterState::default(),
            actions: Vec::new(),
            events: Vec::new(),
        }
    }
    pub const fn policy(&self) -> AdjusterPolicy {
        AdjusterPolicy::AtomicMetadataV1
    }
    pub fn state(&self) -> &AdjusterState {
        &self.state
    }
    pub fn actions(&self) -> &[AdjustmentAction] {
        &self.actions
    }
    pub fn events(&self) -> &[AdjustmentEvent] {
        &self.events
    }
    pub fn adjust_tick_size(&mut self, notional_value: f64, time_millis: u64) -> Result<f64> {
        self.apply_action(AdjustmentAction::AdjustTickSize {
            notional_value,
            time_millis,
        })?;
        Ok(self.state.tick_size)
    }
    /// Returns the new minimum for convenient typed use; source returns undefined.
    pub fn increase_notional_size(&mut self, time_millis: u64) -> Result<f64> {
        self.apply_action(AdjustmentAction::IncreaseNotionalSize { time_millis })?;
        Ok(self.state.min_contract_size)
    }
    /// A useful local event journal, rather than the source's unawaited NeDB call.
    pub fn save_adjustment_event(
        &mut self,
        event_type: &str,
        value: f64,
        time_millis: u64,
    ) -> Result<AdjustmentEvent> {
        check_event_type(event_type)?;
        finite_input(value)?;
        check_time(time_millis)?;
        self.apply_action(AdjustmentAction::SaveEvent {
            event_type: event_type.to_owned(),
            value,
            time_millis,
        })?;
        Ok(self.events.last().expect("direct event appended").clone())
    }
    fn apply_action(&mut self, action: AdjustmentAction) -> Result<()> {
        validate_action(&action)?;
        if self.actions.len() >= MAX_ADJUSTER_ACTIONS {
            return Err(AdjusterError::ActionLimit);
        }
        let mut state = self.state.clone();
        let mut events = Vec::with_capacity(2);
        let mut emit = |event_type: &str, value: f64, time_millis: u64| -> Result<()> {
            if !value.is_finite() {
                return Err(AdjusterError::ArithmeticOverflow);
            }
            if self.events.len() + events.len() >= MAX_ADJUSTER_EVENTS {
                return Err(AdjusterError::EventLimit);
            }
            events.push(AdjustmentEvent {
                sequence: (self.events.len() + events.len()) as u64,
                event_type: event_type.to_owned(),
                value,
                time_millis,
            });
            Ok(())
        };
        match &action {
            AdjustmentAction::AdjustTickSize {
                notional_value,
                time_millis,
            } => {
                // Multiplication can harmlessly overflow to +Infinity. A finite
                // notional then fails the exact source comparison and resets.
                if *notional_value >= state.notional_threshold * state.min_contract_size {
                    state.consecutive_doublings += 1;
                    if state.consecutive_doublings >= state.doubling_interval {
                        state.tick_size *= 2.0;
                        state.consecutive_doublings = 0;
                        emit("DoubleTickSize", state.tick_size, *time_millis)?;
                    }
                    state.tick_size *= state.notional_multiplier;
                    emit("IncreaseTickSize", state.tick_size, *time_millis)?;
                } else {
                    state.consecutive_doublings = 0;
                }
            }
            AdjustmentAction::IncreaseNotionalSize { time_millis } => {
                state.min_contract_size *= 10.0;
                emit(
                    "IncreaseNotionalSize",
                    state.min_contract_size,
                    *time_millis,
                )?;
            }
            AdjustmentAction::SaveEvent {
                event_type,
                value,
                time_millis,
            } => emit(event_type, *value, *time_millis)?,
        }
        self.state = state;
        self.actions.push(action);
        self.events.extend(events);
        Ok(())
    }
    /// Replay all retained actions from the exact constructor. No stored scalar
    /// or generated event is accepted as evidence of its own transition.
    pub fn verify(&self) -> Result<()> {
        if self.actions.len() > MAX_ADJUSTER_ACTIONS || self.events.len() > MAX_ADJUSTER_EVENTS {
            return Err(AdjusterError::InvalidCheckpoint);
        }
        let mut replay = Self::new();
        for action in &self.actions {
            replay
                .apply_action(action.clone())
                .map_err(|_| AdjusterError::InvalidCheckpoint)?;
        }
        if !same_state(&replay.state, &self.state)
            || replay.events.len() != self.events.len()
            || replay
                .events
                .iter()
                .zip(&self.events)
                .any(|(expected, actual)| !same_event(expected, actual))
        {
            return Err(AdjusterError::InvalidCheckpoint);
        }
        Ok(())
    }
    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>> {
        self.verify()?;
        self.encode_checkpoint()
    }
    fn encode_checkpoint(&self) -> Result<Vec<u8>> {
        let mut bytes = MAGIC.to_vec();
        for value in [
            self.state.tick_size,
            self.state.min_contract_size,
            self.state.notional_threshold,
            self.state.notional_multiplier,
        ] {
            put_f64(&mut bytes, value);
        }
        bytes.extend_from_slice(&self.state.doubling_interval.to_be_bytes());
        bytes.extend_from_slice(&self.state.consecutive_doublings.to_be_bytes());
        bytes.extend_from_slice(&(self.actions.len() as u32).to_be_bytes());
        for action in &self.actions {
            match action {
                AdjustmentAction::AdjustTickSize {
                    notional_value,
                    time_millis,
                } => {
                    bytes.push(0);
                    bytes.extend_from_slice(&time_millis.to_be_bytes());
                    put_f64(&mut bytes, *notional_value);
                }
                AdjustmentAction::IncreaseNotionalSize { time_millis } => {
                    bytes.push(1);
                    bytes.extend_from_slice(&time_millis.to_be_bytes());
                }
                AdjustmentAction::SaveEvent {
                    event_type,
                    value,
                    time_millis,
                } => {
                    bytes.push(2);
                    bytes.extend_from_slice(&time_millis.to_be_bytes());
                    put_type(&mut bytes, event_type);
                    put_f64(&mut bytes, *value);
                }
            }
        }
        bytes.extend_from_slice(&(self.events.len() as u32).to_be_bytes());
        for event in &self.events {
            bytes.extend_from_slice(&event.sequence.to_be_bytes());
            bytes.extend_from_slice(&event.time_millis.to_be_bytes());
            put_type(&mut bytes, &event.event_type);
            put_f64(&mut bytes, event.value);
        }
        if bytes.len().saturating_add(4) > MAX_ADJUSTER_CHECKPOINT_BYTES {
            return Err(AdjusterError::InvalidCheckpoint);
        }
        with_crc(bytes).map_err(|_| AdjusterError::InvalidCheckpoint)
    }
    pub fn from_checkpoint_bytes(bytes: &[u8]) -> Result<Self> {
        let bad = || AdjusterError::InvalidCheckpoint;
        if bytes.len() > MAX_ADJUSTER_CHECKPOINT_BYTES {
            return Err(bad());
        }
        let raw = verify_crc(bytes).map_err(|_| bad())?;
        let mut reader = Cursor::new(raw);
        if reader.fixed::<5>().map_err(|_| bad())? != *MAGIC {
            return Err(bad());
        }
        let state = AdjusterState {
            tick_size: read_f64(&mut reader)?,
            min_contract_size: read_f64(&mut reader)?,
            notional_threshold: read_f64(&mut reader)?,
            notional_multiplier: read_f64(&mut reader)?,
            doubling_interval: reader.u32().map_err(|_| bad())?,
            consecutive_doublings: reader.u32().map_err(|_| bad())?,
        };
        let count = reader.u32().map_err(|_| bad())? as usize;
        if count > MAX_ADJUSTER_ACTIONS {
            return Err(bad());
        }
        let mut actions = Vec::new();
        for _ in 0..count {
            let tag = reader.u8().map_err(|_| bad())?;
            let time_millis = reader.u64().map_err(|_| bad())?;
            let action = match tag {
                0 => AdjustmentAction::AdjustTickSize {
                    notional_value: read_f64(&mut reader)?,
                    time_millis,
                },
                1 => AdjustmentAction::IncreaseNotionalSize { time_millis },
                2 => AdjustmentAction::SaveEvent {
                    event_type: read_type(&mut reader)?,
                    value: read_f64(&mut reader)?,
                    time_millis,
                },
                _ => return Err(bad()),
            };
            validate_action(&action).map_err(|_| bad())?;
            actions.push(action);
        }
        let count = reader.u32().map_err(|_| bad())? as usize;
        if count > MAX_ADJUSTER_EVENTS {
            return Err(bad());
        }
        let mut events = Vec::new();
        for _ in 0..count {
            let event = AdjustmentEvent {
                sequence: reader.u64().map_err(|_| bad())?,
                time_millis: reader.u64().map_err(|_| bad())?,
                event_type: read_type(&mut reader)?,
                value: read_f64(&mut reader)?,
            };
            check_time(event.time_millis).map_err(|_| bad())?;
            events.push(event);
        }
        reader.end().map_err(|_| bad())?;
        let result = Self {
            state,
            actions,
            events,
        };
        result.verify()?;
        if result.encode_checkpoint()?.as_slice() != bytes {
            return Err(bad());
        }
        Ok(result)
    }
}

fn validate_action(action: &AdjustmentAction) -> Result<()> {
    match action {
        AdjustmentAction::AdjustTickSize {
            notional_value,
            time_millis,
        } => {
            finite_input(*notional_value)?;
            check_time(*time_millis)
        }
        AdjustmentAction::IncreaseNotionalSize { time_millis } => check_time(*time_millis),
        AdjustmentAction::SaveEvent {
            event_type,
            value,
            time_millis,
        } => {
            check_event_type(event_type)?;
            finite_input(*value)?;
            check_time(*time_millis)
        }
    }
}
fn finite_input(value: f64) -> Result<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(AdjusterError::NonFiniteInput)
    }
}
fn check_time(time: u64) -> Result<()> {
    if time <= MAX_EVENT_TIME_MILLIS {
        Ok(())
    } else {
        Err(AdjusterError::InvalidTime)
    }
}
fn check_event_type(event_type: &str) -> Result<()> {
    if event_type.len() <= MAX_EVENT_TYPE_BYTES {
        Ok(())
    } else {
        Err(AdjusterError::EventTypeTooLong)
    }
}
fn same_state(a: &AdjusterState, b: &AdjusterState) -> bool {
    a.tick_size.to_bits() == b.tick_size.to_bits()
        && a.min_contract_size.to_bits() == b.min_contract_size.to_bits()
        && a.notional_threshold.to_bits() == b.notional_threshold.to_bits()
        && a.notional_multiplier.to_bits() == b.notional_multiplier.to_bits()
        && a.doubling_interval == b.doubling_interval
        && a.consecutive_doublings == b.consecutive_doublings
}
fn same_event(a: &AdjustmentEvent, b: &AdjustmentEvent) -> bool {
    a.sequence == b.sequence
        && a.event_type == b.event_type
        && a.value.to_bits() == b.value.to_bits()
        && a.time_millis == b.time_millis
}
fn put_f64(bytes: &mut Vec<u8>, value: f64) {
    bytes.extend_from_slice(&value.to_bits().to_be_bytes());
}
fn put_type(bytes: &mut Vec<u8>, value: &str) {
    bytes.push(value.len() as u8);
    bytes.extend_from_slice(value.as_bytes());
}
fn read_f64(reader: &mut Cursor<'_>) -> Result<f64> {
    let value = f64::from_bits(reader.u64().map_err(|_| AdjusterError::InvalidCheckpoint)?);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(AdjusterError::InvalidCheckpoint)
    }
}
fn read_type(reader: &mut Cursor<'_>) -> Result<String> {
    let length = reader.u8().map_err(|_| AdjusterError::InvalidCheckpoint)? as usize;
    if length > MAX_EVENT_TYPE_BYTES {
        return Err(AdjusterError::InvalidCheckpoint);
    }
    let bytes = reader
        .take(length)
        .map_err(|_| AdjusterError::InvalidCheckpoint)?;
    let text = core::str::from_utf8(bytes).map_err(|_| AdjusterError::InvalidCheckpoint)?;
    Ok(text.to_owned())
}
