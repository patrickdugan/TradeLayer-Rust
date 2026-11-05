//! The node loop: observe blocks, admit and apply them, resolve forks, and
//! serve the listener API between steps. See `docs/NODE.md`.
//!
//! `NodeRunner` owns three things. The observer (`daemon::Daemon`) records raw
//! blocks and detects forks; it never touches the economy. The service
//! (`node_service::NodeHttpService`) owns the durable economic chain. The
//! ingest (`node_ingest::NodeIngest`) is the only path from one to the other:
//! after each observer step the economic chain catches up to the observer's
//! selected chain, and a fork rolls the economic chain back to the observer's
//! common ancestor before the observer rewinds its own selection.

use crate::bootstrap::BootstrapControl;
use crate::chain::{BlockHeader, Tip};
use crate::daemon::{
    Daemon, DaemonError, ForkEvidence, HeadSink, ObservationHead, ObservationRpc, ObservationStore,
    ObserverMode, ObserverPhase, QuarantineReason, RawObservation, StepOutcome,
    MAX_OBSERVED_BLOCKS,
};
use crate::economic_chain::EconomicHead;
use crate::http_service::ServeOutcome;
use crate::listener_config::{head_token, parse_expected_head};
use crate::node_ingest::{BlockReport, IngestError, Locate, NodeIngest, PrevoutProvider};
use crate::node_service::NodeHttpService;
use crate::runtime::{block_id_text, parse_block_id};
use crate::service::ServiceError;
use std::io::Write;
use std::path::Path;

/// Observer records at which the node moves to a fresh observer store.
pub const ROTATE_AT_RECORDS: u32 = MAX_OBSERVED_BLOCKS * 3 / 4;
/// A rotated observer store is anchored this many blocks below the tip, so a
/// shallow fork right after a rotation still finds its common ancestor.
pub const ROTATE_LOOKBACK: u64 = 8;
const MAX_SERVED_PER_CALL: u32 = 16;

#[derive(Debug)]
pub enum NodeError {
    Ingest(IngestError),
    Service(ServiceError),
    Observer(DaemonError),
    /// The observer stopped observing for a reason the node does not clear.
    Quarantined(QuarantineReason),
    NotObserving(ObserverMode),
    HeadLog(std::io::Error),
}
impl core::fmt::Display for NodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Ingest(e) => write!(f, "{e}"),
            Self::Service(e) => write!(f, "Service({e})"),
            Self::Observer(e) => write!(f, "Observer({e})"),
            Self::Quarantined(reason) => write!(f, "ObserverQuarantined({reason:?})"),
            Self::NotObserving(mode) => write!(f, "ObserverNotObserving({mode:?})"),
            Self::HeadLog(e) => write!(f, "HeadLog({e})"),
        }
    }
}
impl std::error::Error for NodeError {}
impl From<IngestError> for NodeError {
    fn from(value: IngestError) -> Self {
        Self::Ingest(value)
    }
}
impl From<ServiceError> for NodeError {
    fn from(value: ServiceError) -> Self {
        Self::Service(value)
    }
}
impl From<DaemonError> for NodeError {
    fn from(value: DaemonError) -> Self {
        Self::Observer(value)
    }
}

type Result<T> = core::result::Result<T, NodeError>;

#[derive(Debug, Clone)]
pub enum NodeEvent {
    Observer(StepOutcome),
    Applied(BlockReport),
    /// The economic chain went back from `from` to `to` (observed tips) and
    /// the observer rewound its selection to `to`.
    Reorganized {
        from: Tip,
        to: Tip,
    },
    /// The observer moved to a fresh store anchored at `anchor`.
    Rotated {
        anchor: Tip,
    },
}

/// Receives each economic head after the economic journal commits it.
pub type EconomicHeadSink = Box<dyn FnMut(EconomicHead) -> std::io::Result<()> + Send>;
/// Creates the observer store that replaces a full one, anchored at the tip.
pub type ObserverRotation =
    Box<dyn FnMut(Tip) -> core::result::Result<ObservationStore, DaemonError> + Send>;

pub struct NodeRunner<R: ObservationRpc, P: PrevoutProvider> {
    daemon: Option<Daemon<R>>,
    service: NodeHttpService,
    prevouts: P,
    ingest: NodeIngest,
    head_sink: Option<EconomicHeadSink>,
    rotation: Option<ObserverRotation>,
    rotate_at: u32,
    rotate_lookback: u64,
}
impl<R: ObservationRpc, P: PrevoutProvider> NodeRunner<R, P> {
    pub fn new(
        daemon: Daemon<R>,
        service: NodeHttpService,
        prevouts: P,
        ingest: NodeIngest,
    ) -> Self {
        Self {
            daemon: Some(daemon),
            service,
            prevouts,
            ingest,
            head_sink: None,
            rotation: None,
            rotate_at: ROTATE_AT_RECORDS,
            rotate_lookback: ROTATE_LOOKBACK,
        }
    }
    pub fn with_head_sink(mut self, sink: EconomicHeadSink) -> Self {
        self.head_sink = Some(sink);
        self
    }
    pub fn with_rotation(mut self, rotation: ObserverRotation) -> Self {
        self.rotation = Some(rotation);
        self
    }
    /// Observer records at which a rotation is considered, and how far below
    /// the tip the new store is anchored; the defaults are `ROTATE_AT_RECORDS`
    /// and `ROTATE_LOOKBACK`. A record count above the observer's limit never
    /// rotates.
    pub fn with_rotation_at(mut self, records: u32, lookback: u64) -> Self {
        self.rotate_at = records;
        self.rotate_lookback = lookback;
        self
    }
    fn daemon(&self) -> &Daemon<R> {
        self.daemon
            .as_ref()
            .expect("daemon is present between steps")
    }
    fn daemon_mut(&mut self) -> &mut Daemon<R> {
        self.daemon
            .as_mut()
            .expect("daemon is present between steps")
    }
    pub fn service(&self) -> &NodeHttpService {
        &self.service
    }
    pub fn observer(&self) -> &Daemon<R> {
        self.daemon()
    }
    pub fn ingest(&self) -> &NodeIngest {
        &self.ingest
    }
    /// Stops the runner and returns the durable chain.
    pub fn into_parts(self) -> (Daemon<R>, NodeHttpService) {
        (
            self.daemon.expect("daemon is present between steps"),
            self.service,
        )
    }

    fn record_head(&mut self, head: EconomicHead) -> Result<()> {
        if let Some(sink) = self.head_sink.as_mut() {
            sink(head).map_err(NodeError::HeadLog)?;
        }
        Ok(())
    }
    fn refresh_observed(&mut self) -> Result<()> {
        let tip = self.daemon().head()?.tip;
        let mut status = self.service.status();
        status.observed_tip = Some(tip);
        self.service.set_status(status);
        Ok(())
    }

    /// Rolls the economic chain back so its tip stands for `target`, when it
    /// stands above it. Returns the observed tip it left.
    fn rewind(&mut self, target: Tip) -> Result<Option<Tip>> {
        let before = self.service.economic_chain()?.tip();
        let from = self.ingest.binding().observed_tip(before);
        let Self {
            service, ingest, ..
        } = self;
        let head = service.with_owner(|owner| ingest.rollback_to(owner, target))??;
        let after = service.economic_chain()?.tip().height;
        if after == before.height {
            return Ok(None);
        }
        let mut status = service.status();
        status.rolled_back_blocks += before.height - after;
        service.set_status(status);
        self.record_head(head)?;
        Ok(Some(from))
    }

    /// Applies every block of the observer's selected chain the economic chain
    /// has not applied, in order. When the economic tip is not on the
    /// observer's chain but the two share a block (a fork the observer could
    /// not report, because its store was rebuilt below the economic tip), the
    /// economic chain goes back to that block first. Refuses when the two
    /// share no block.
    pub fn catch_up(&mut self, events: &mut Vec<NodeEvent>) -> Result<()> {
        enum Next {
            Done,
            Apply(Box<RawObservation>),
            Rewind(Tip),
        }
        loop {
            let next = {
                let store = self.daemon().store();
                let anchor = store.config()?.anchor;
                let headers: Vec<BlockHeader> = store.canonical_headers()?;
                let chain = self.service.economic_chain()?;
                match self.ingest.locate(chain, anchor, &headers)? {
                    Locate::Ahead => Next::Done,
                    Locate::Fork(shared) => Next::Rewind(shared),
                    Locate::Apply(index) => match headers.get(index) {
                        None => Next::Done,
                        Some(next) => Next::Apply(Box::new(
                            store
                                .records()?
                                .iter()
                                .find(|r| r.header() == *next)
                                .cloned()
                                .ok_or(IngestError::Misaligned(
                                    "selected block has no stored record",
                                ))?,
                        )),
                    },
                }
            };
            match next {
                Next::Done => return Ok(()),
                Next::Rewind(shared) => {
                    if let Some(from) = self.rewind(shared)? {
                        events.push(NodeEvent::Reorganized { from, to: shared });
                    }
                }
                Next::Apply(record) => {
                    let Self {
                        service,
                        prevouts,
                        ingest,
                        ..
                    } = self;
                    let (head, report) =
                        service.with_owner(|owner| ingest.apply(owner, &record, prevouts))??;
                    let mut status = service.status();
                    status.authenticated_admission = true;
                    status.admitted_blocks += 1;
                    status.authenticated_candidates += report.authenticated as u64;
                    status.refused_candidates += report.refused_total as u64;
                    status.unrepresentable_candidates += report.unrepresentable as u64;
                    status.applied_candidates += report.source.accepted as u64;
                    service.set_status(status);
                    self.record_head(head)?;
                    events.push(NodeEvent::Applied(report));
                }
            }
        }
    }

    /// Resolves an observer fork: the economic chain goes back to the common
    /// ancestor, then the observer rewinds to it.
    fn resolve_fork(
        &mut self,
        now_ms: u64,
        control: &BootstrapControl,
        events: &mut Vec<NodeEvent>,
    ) -> Result<()> {
        // The daemon finds the common ancestor one getblockhash at a time.
        let mut budget = u64::from(self.daemon().status()?.stored_blocks) + 2;
        while budget > 0 && self.daemon().status()?.phase == ObserverPhase::ForkSearch {
            self.daemon_mut().step(now_ms, control)?;
            budget -= 1;
        }
        let head: ObservationHead = self.daemon().head()?;
        let evidence: Option<ForkEvidence> = self.daemon().store().fork_evidence()?;
        let target = match evidence {
            Some(found) => found.common_ancestor.ok_or(IngestError::ForkUnresolved(
                "no common ancestor within the observer's history",
            ))?,
            // A hash recheck failed with no search: the selected tip stands.
            None => head.tip,
        };
        let from = self
            .ingest
            .binding()
            .observed_tip(self.service.economic_chain()?.tip());
        self.rewind(target)?;
        self.daemon_mut().reconcile(head, target)?;
        events.push(NodeEvent::Reorganized { from, to: target });
        Ok(())
    }

    fn maybe_rotate(&mut self, events: &mut Vec<NodeEvent>) -> Result<()> {
        if self.rotation.is_none() {
            return Ok(());
        }
        let status = self.daemon().status()?;
        if status.stored_blocks < self.rotate_at || status.mode != ObserverMode::Observing {
            return Ok(());
        }
        let store = self.daemon().store();
        if store.fork_evidence()?.is_some() {
            return Ok(());
        }
        let headers = store.canonical_headers()?;
        let anchor = store.config()?.anchor;
        let economic = self.service.economic_chain()?.tip();
        let observed = self.ingest.binding().observed_tip(economic);
        if observed != status.head.tip {
            return Ok(());
        }
        let wanted = observed.height.saturating_sub(self.rotate_lookback);
        let new_anchor = headers
            .iter()
            .find(|h| h.height == wanted)
            .map_or(anchor, |h| Tip {
                height: h.height,
                id: h.id,
            });
        if new_anchor == anchor {
            return Ok(());
        }
        let fresh = (self.rotation.as_mut().expect("checked above"))(new_anchor)?;
        let (rpc, old) = self
            .daemon
            .take()
            .expect("daemon is present between steps")
            .into_parts();
        drop(old);
        self.daemon = Some(Daemon::new(rpc, fresh));
        events.push(NodeEvent::Rotated { anchor: new_anchor });
        Ok(())
    }

    /// One observer step followed by the economic work it calls for.
    pub fn step(&mut self, now_ms: u64, control: &BootstrapControl) -> Result<Vec<NodeEvent>> {
        let mut events = Vec::new();
        self.catch_up(&mut events)?;
        let outcome = self.daemon_mut().step(now_ms, control)?;
        events.push(NodeEvent::Observer(outcome));
        match outcome {
            StepOutcome::Recorded { .. } => {
                self.catch_up(&mut events)?;
                self.maybe_rotate(&mut events)?;
            }
            StepOutcome::Quarantined {
                reason: QuarantineReason::ForkDetected | QuarantineReason::CursorAboveTip,
            } => {
                self.resolve_fork(now_ms, control, &mut events)?;
                self.catch_up(&mut events)?;
            }
            StepOutcome::Quarantined { reason } => return Err(NodeError::Quarantined(reason)),
            StepOutcome::Paused => return Err(NodeError::NotObserving(ObserverMode::Paused)),
            StepOutcome::Stopped | StepOutcome::Progress | StepOutcome::Waiting { .. } => {}
        }
        self.refresh_observed()?;
        Ok(events)
    }

    /// Serves queued requests, at most `MAX_SERVED_PER_CALL`, and returns how
    /// many were answered.
    pub fn serve(&self, control: &BootstrapControl) -> Result<u32> {
        let mut served = 0;
        while served < MAX_SERVED_PER_CALL {
            match self.service.serve_one(control) {
                Ok(ServeOutcome::Idle | ServeOutcome::Stopped) => break,
                Ok(ServeOutcome::Responded { .. }) => served += 1,
                // One bad connection does not stop the node.
                Err(
                    ServiceError::Transport(_)
                    | ServiceError::Timeout
                    | ServiceError::InvalidHttp
                    | ServiceError::Cancelled,
                ) => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(served)
    }
}

/// An append-only head record, synced after every line. The observer writes
/// `prepared` and `committed` lines; the economic log writes `committed`.
pub fn head_log_sink(path: &Path, create: bool) -> std::io::Result<EconomicHeadSink> {
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create_new(create)
        .open(path)?;
    #[cfg(unix)]
    if create {
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent).and_then(|dir| dir.sync_all())?;
        }
    }
    Ok(Box::new(move |head| {
        file.write_all(format!("committed {}\n", head_token(head)).as_bytes())?;
        file.sync_data()
    }))
}

/// The economic head to restore: the last `committed` line of the log. A final
/// line without a newline was cut off before its sync and is ignored.
pub fn last_economic_head(path: &Path) -> std::result::Result<EconomicHead, String> {
    let error = |e: std::io::Error| format!("head log {}: {e}", path.display());
    if std::fs::metadata(path).map_err(error)?.len() > 16 * 1024 * 1024 {
        return Err("economic head log exceeds 16 MiB".into());
    }
    let text = std::fs::read_to_string(path).map_err(error)?;
    let complete = text.rfind('\n').map_or("", |end| &text[..end]);
    let mut last = None;
    for (number, line) in complete.lines().enumerate() {
        let token = line
            .strip_prefix("committed ")
            .ok_or_else(|| format!("invalid economic head log line {}", number + 1))?;
        last = Some(
            parse_expected_head(token)
                .map_err(|_| format!("invalid economic head log line {}", number + 1))?,
        );
    }
    last.ok_or_else(|| "economic head log records no head".into())
}

/// `SEQUENCE:HEIGHT:HASH:COMMITMENT` for an observer head, the token
/// `tl_observe` prints.
pub fn observation_head_token(head: ObservationHead) -> String {
    format!(
        "{}:{}:{}:{}",
        head.sequence,
        head.tip.height,
        block_id_text(head.tip.id),
        block_id_text(crate::chain::BlockId(head.commitment))
    )
}

pub fn parse_observation_head(text: &str) -> std::result::Result<ObservationHead, String> {
    let parts = text.split(':').collect::<Vec<_>>();
    if parts.len() != 4 {
        return Err("invalid observer head".into());
    }
    Ok(ObservationHead {
        sequence: parts[0].parse().map_err(|_| "invalid sequence")?,
        tip: Tip {
            height: parts[1].parse().map_err(|_| "invalid tip height")?,
            id: parse_block_id(parts[2]).map_err(|_| "invalid tip hash")?,
        },
        commitment: parse_block_id(parts[3])
            .map_err(|_| "invalid head commitment")?
            .0,
    })
}

/// Restart candidates for the observer from its head log: the last committed
/// head, then a prepared head recorded after it.
pub fn observer_head_candidates(path: &Path) -> std::result::Result<Vec<ObservationHead>, String> {
    let error = |e: std::io::Error| format!("head log {}: {e}", path.display());
    if std::fs::metadata(path).map_err(error)?.len() > 16 * 1024 * 1024 {
        return Err("head log exceeds 16 MiB".into());
    }
    let text = std::fs::read_to_string(path).map_err(error)?;
    let complete = text.rfind('\n').map_or("", |end| &text[..end]);
    let (mut committed, mut prepared) = (None, None);
    for (number, line) in complete.lines().enumerate() {
        let invalid = || format!("invalid head log line {}", number + 1);
        match line.split_once(' ').ok_or_else(invalid)? {
            ("prepared", head) => {
                prepared = Some(parse_observation_head(head).map_err(|_| invalid())?)
            }
            ("committed", head) => {
                committed = Some(parse_observation_head(head).map_err(|_| invalid())?);
                prepared = None;
            }
            _ => return Err(invalid()),
        }
    }
    let heads: Vec<_> = committed.into_iter().chain(prepared).collect();
    if heads.is_empty() {
        return Err("head log records no head".into());
    }
    Ok(heads)
}

/// The observer's head sink writing `prepared` and `committed` lines.
pub fn observer_head_sink(path: &Path, create: bool) -> std::io::Result<HeadSink> {
    use crate::daemon::HeadRecord;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create_new(create)
        .open(path)?;
    #[cfg(unix)]
    if create {
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent).and_then(|dir| dir.sync_all())?;
        }
    }
    Ok(Box::new(move |record| {
        let line = match record {
            HeadRecord::Prepared(head) => format!("prepared {}\n", observation_head_token(head)),
            HeadRecord::Committed(head) => format!("committed {}\n", observation_head_token(head)),
        };
        file.write_all(line.as_bytes())?;
        file.sync_data()
    }))
}
