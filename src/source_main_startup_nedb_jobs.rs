//! Actual NeDB 1.8 executor / async 0.2.10 immediate-job structure.
//! Rows, callbacks, native outcomes and financial state remain on the sealed
//! containing same-owner source frame. This queue chooses no business result.
use super::startup_source_promise_queue::{
    ActualPayloadBinding, ActualPayloadRef, StartupPromiseQueue,
};
use crate::validity_source::{SourceRuleError, SourceValue as V};
use std::collections::VecDeque;
const MAX_REQUESTS: usize = 4096;
const MAX_READY: usize = 8192;
static NEXT_OWNER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Collection {
    Consensus,
    TallyMap,
    TxIndex,
}
impl Collection {
    fn position(self) -> usize {
        match self {
            Self::Consensus => 0,
            Self::TallyMap => 1,
            Self::TxIndex => 2,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Consensus => "consensus",
            Self::TallyMap => "tallyMap",
            Self::TxIndex => "txIndex",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueryKind {
    FindOneStringId,
    FindAll,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RequestKey {
    owner: u64,
    generation: u64,
    serial: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Requested,
    Worker,
    Sample,
    Filter,
    Callback,
    Completed,
}
#[derive(Debug, PartialEq, Eq)]
struct Request {
    binding: ActualPayloadBinding,
    payload: ActualPayloadRef,
    collection: Collection,
    kind: QueryKind,
    phase: Phase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    Process(Collection),
    CandidatesIndex(RequestKey),
    CandidatesFilter(RequestKey),
    Complete(RequestKey),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeliveryKind {
    SampleCandidates,
    DeliverCallback,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Delivery {
    pub(crate) request: RequestKey,
    pub(crate) kind: DeliveryKind,
    pub(crate) payload: ActualPayloadRef,
    ticket: u64,
}
#[derive(Debug, PartialEq, Eq)]
struct DatabaseQueue {
    busy: bool,
    requests: VecDeque<RequestKey>,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StartupNedbQueue {
    owner: u64,
    generation: u64,
    promises_owner_job: Option<ActualPayloadBinding>,
    requests: Vec<Request>,
    databases: [DatabaseQueue; 3],
    ready: VecDeque<Job>,
    active: Option<Delivery>,
    inflight: Option<Job>,
    next_ticket: u64,
    completed_immediates: u64,
    stopped: Option<SourceRuleError>,
}
impl StartupNedbQueue {
    pub(crate) fn new(generation: u64) -> Result<Self, SourceRuleError> {
        let owner = NEXT_OWNER
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .map_err(|_| SourceRuleError::Capacity)?;
        Ok(Self {
            owner,
            generation,
            promises_owner_job: None,
            requests: vec![],
            databases: std::array::from_fn(|_| DatabaseQueue {
                busy: false,
                requests: VecDeque::new(),
            }),
            ready: VecDeque::new(),
            active: None,
            inflight: None,
            next_ticket: 1,
            completed_immediates: 0,
            stopped: None,
        })
    }
    fn live(&self) -> Result<(), SourceRuleError> {
        self.stopped
            .as_ref()
            .map_or(Ok(()), |error| Err(error.clone()))
    }
    fn capacity<T>(&mut self) -> Result<T, SourceRuleError> {
        self.stopped = Some(SourceRuleError::Capacity);
        Err(SourceRuleError::Capacity)
    }
    fn at(&self, key: RequestKey) -> Result<usize, SourceRuleError> {
        if key.owner != self.owner || key.generation != self.generation || key.serial == 0 {
            return Err(SourceRuleError::InvalidInput);
        }
        let at = usize::try_from(key.serial - 1).map_err(|_| SourceRuleError::InvalidInput)?;
        if at >= self.requests.len() {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(at)
    }
    fn reserve_jobs(&mut self, n: usize) -> Result<(), SourceRuleError> {
        if self
            .ready
            .len()
            .checked_add(n)
            .is_none_or(|n| n > MAX_READY)
            || self.ready.try_reserve(n).is_err()
        {
            return self.capacity();
        }
        Ok(())
    }
    /// Called only after the exact raw request is reserved on its same-owner
    /// frame. No query is sampled here. A ready NeDB database queues one real
    /// q.process immediate per push, including pushes while its worker is busy.
    pub(crate) fn enqueue_actual_request(
        &mut self,
        promises: &StartupPromiseQueue,
        payload: ActualPayloadRef,
        collection: Collection,
        kind: QueryKind,
    ) -> Result<RequestKey, SourceRuleError> {
        self.live()?;
        let binding = *promises.actual_payload_binding(payload)?;
        if promises.actual_payload_is_ready(payload)?
            || self
                .requests
                .iter()
                .any(|request| request.payload == payload)
            || (kind == QueryKind::FindAll && collection != Collection::TxIndex)
        {
            return Err(SourceRuleError::InvalidInput);
        }
        // A queue's Promise namespace cannot be substituted after it has
        // accepted a request, even by an equal generation/job/site record.
        if let Some(previous) = self.promises_owner_job {
            promises.source_job_is_same_owner(previous.job)?;
        }
        if self.requests.len() >= MAX_REQUESTS
            || self.requests.try_reserve(1).is_err()
            || self.databases[collection.position()]
                .requests
                .try_reserve(1)
                .is_err()
        {
            return self.capacity();
        }
        self.reserve_jobs(1)?;
        let serial = u64::try_from(self.requests.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(SourceRuleError::Capacity)?;
        let key = RequestKey {
            owner: self.owner,
            generation: self.generation,
            serial,
        };
        self.requests.push(Request {
            binding,
            payload,
            collection,
            kind,
            phase: Phase::Requested,
        });
        self.databases[collection.position()]
            .requests
            .push_back(key);
        self.ready.push_back(Job::Process(collection));
        self.promises_owner_job.get_or_insert(binding);
        Ok(key)
    }
    pub(crate) fn binding(
        &self,
        key: RequestKey,
    ) -> Result<&ActualPayloadBinding, SourceRuleError> {
        Ok(&self.requests[self.at(key)?].binding)
    }
    pub(crate) fn next_delivery(&mut self) -> Result<Option<Delivery>, SourceRuleError> {
        self.live()?;
        if self.active.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        while let Some(job) = self.ready.pop_front() {
            self.inflight = Some(job);
            self.completed_immediates = self
                .completed_immediates
                .checked_add(1)
                .ok_or(SourceRuleError::Capacity)?;
            let issued = match job {
                Job::Process(collection) => {
                    let at = collection.position();
                    if !self.databases[at].busy && !self.databases[at].requests.is_empty() {
                        self.reserve_jobs(1)?;
                        let key = self.databases[at]
                            .requests
                            .pop_front()
                            .expect("checked queue");
                        let row = self.at(key)?;
                        self.databases[at].busy = true;
                        self.requests[row].phase = Phase::Worker;
                        self.ready.push_back(Job::CandidatesIndex(key));
                    }
                    None
                }
                Job::Complete(key) => {
                    let row = self.at(key)?;
                    let collection = self.requests[row].collection;
                    let at = collection.position();
                    if !self.databases[at].busy || self.requests[row].phase != Phase::Completed {
                        return Err(SourceRuleError::InvalidInput);
                    }
                    // async.queue's worker-next calls q.process synchronously;
                    // it does not add a synthetic source or immediate turn.
                    if !self.databases[at].requests.is_empty() {
                        self.reserve_jobs(1)?;
                        let next = self.databases[at]
                            .requests
                            .pop_front()
                            .expect("checked queue");
                        let next_at = self.at(next)?;
                        self.requests[next_at].phase = Phase::Worker;
                        self.ready.push_back(Job::CandidatesIndex(next));
                    } else {
                        self.databases[at].busy = false;
                    }
                    None
                }
                Job::CandidatesIndex(key) => {
                    let at = self.at(key)?;
                    if self.requests[at].phase != Phase::Worker {
                        return Err(SourceRuleError::InvalidInput);
                    }
                    self.requests[at].phase = Phase::Sample;
                    Some((key, DeliveryKind::SampleCandidates))
                }
                Job::CandidatesFilter(key) => {
                    let at = self.at(key)?;
                    if self.requests[at].phase != Phase::Filter {
                        return Err(SourceRuleError::InvalidInput);
                    }
                    self.reserve_jobs(1)?;
                    // Genuine executor wrapper schedules worker-next BEFORE
                    // invoking the actual callback / host Promise resolver.
                    self.ready.push_back(Job::Complete(key));
                    self.requests[at].phase = Phase::Callback;
                    Some((key, DeliveryKind::DeliverCallback))
                }
            };
            self.inflight = None;
            if let Some((request, kind)) = issued {
                let ticket = self.next_ticket;
                self.next_ticket = ticket.checked_add(1).ok_or(SourceRuleError::Capacity)?;
                let delivery = Delivery {
                    request,
                    kind,
                    payload: self.requests[self.at(request)?].payload,
                    ticket,
                };
                self.active = Some(delivery);
                return Ok(Some(delivery));
            }
        }
        Ok(None)
    }
    pub(crate) fn acknowledge_actual(&mut self, delivery: Delivery) -> Result<(), SourceRuleError> {
        self.live()?;
        if self.active != Some(delivery) {
            return Err(SourceRuleError::InvalidInput);
        }
        let at = self.at(delivery.request)?;
        match delivery.kind {
            DeliveryKind::SampleCandidates => {
                if self.requests[at].phase != Phase::Sample {
                    return Err(SourceRuleError::InvalidInput);
                }
                self.reserve_jobs(1)?;
                self.ready
                    .push_back(Job::CandidatesFilter(delivery.request));
                self.requests[at].phase = Phase::Filter;
            }
            DeliveryKind::DeliverCallback => {
                if self.requests[at].phase != Phase::Callback {
                    return Err(SourceRuleError::InvalidInput);
                }
                self.requests[at].phase = Phase::Completed;
            }
        }
        self.active = None;
        Ok(())
    }
    pub(crate) fn stop_after_owner_prefix(&mut self, error: SourceRuleError) {
        self.stopped = Some(error);
    }
    pub(crate) fn visit_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        fn text(s: impl ToString) -> V {
            V::String(s.to_string())
        }
        visitor(
            "profile",
            &text("TL-MAIN-NEDB18-ASYNC0210-STRING-ID-EXECUTOR-13-1"),
        )?;
        visitor("owner", &text(self.owner))?;
        visitor("generation", &text(self.generation))?;
        visitor("nextTicket", &text(self.next_ticket))?;
        visitor("completedImmediates", &text(self.completed_immediates))?;
        visitor(
            "stopped",
            &self
                .stopped
                .as_ref()
                .map_or(V::Null, |e| text(format!("{e:?}"))),
        )?;
        visitor("requests/count", &text(self.requests.len()))?;
        for (at, request) in self.requests.iter().enumerate() {
            let p = format!("requests/{at}");
            visitor(&format!("{p}/collection"), &text(request.collection.name()))?;
            visitor(&format!("{p}/kind"), &text(format!("{:?}", request.kind)))?;
            visitor(&format!("{p}/phase"), &text(format!("{:?}", request.phase)))?;
            visitor(
                &format!("{p}/sourceJob"),
                &text(request.binding.job.serial()),
            )?;
            visitor(
                &format!("{p}/methodScope"),
                &text(request.binding.method_scope),
            )?;
            visitor(
                &format!("{p}/sourceSite"),
                &text(request.binding.source_site),
            )?;
            visitor(&format!("{p}/lane"), &text(request.binding.lane))?;
            visitor(
                &format!("{p}/awaitOrdinal"),
                &text(request.binding.await_ordinal),
            )?;
        }
        for (at, database) in self.databases.iter().enumerate() {
            visitor(&format!("databases/{at}/busy"), &V::Bool(database.busy))?;
            visitor(
                &format!("databases/{at}/queued/count"),
                &text(database.requests.len()),
            )?;
            for (pos, key) in database.requests.iter().enumerate() {
                visitor(&format!("databases/{at}/queued/{pos}"), &text(key.serial))?;
            }
        }
        visitor("ready/count", &text(self.ready.len()))?;
        for (at, job) in self.ready.iter().enumerate() {
            visitor(&format!("ready/{at}"), &text(format!("{job:?}")))?;
        }
        visitor(
            "active",
            &self.active.map_or(V::Null, |d| text(format!("{d:?}"))),
        )?;
        visitor(
            "inflight",
            &self.inflight.map_or(V::Null, |d| text(format!("{d:?}"))),
        )
    }
}
