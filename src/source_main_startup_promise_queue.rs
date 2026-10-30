//! Metadata-only intrinsic-Promise reactions for the explicit startup profile.
//! This object is intended as an owned extension of the existing Main Kernel.
//! It never owns or clones a Runtime, Journal, Ledger or authoritative row set.
use super::MainStepLegacyThrow;
use crate::validity_source::{SourceRuleError, SourceValue as V};
use std::collections::VecDeque;

const MAX_PROMISES: usize = 1024;
const MAX_JOBS: usize = 4096;
const MAX_REACTIONS: usize = 4096;
static NEXT_OWNER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Realm {
    SourceBusiness,
    RecordedHostIo,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PromiseKey {
    owner: u64,
    generation: u64,
    creation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActualRejection {
    owner: u64,
    identity: u64,
    thrown: MainStepLegacyThrow,
}
impl ActualRejection {
    pub(crate) fn identity(&self) -> u64 {
        self.identity
    }
    pub(crate) fn thrown(&self) -> &MainStepLegacyThrow {
        &self.thrown
    }
}
/// Allocated only by the containing actual Kernel before entering source.
/// The serial is the source job identity passed to nonclone backend frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceJobKey {
    owner: u64,
    generation: u64,
    serial: u64,
}
impl SourceJobKey {
    pub(crate) fn serial(self) -> u64 {
        self.serial
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ActualPayloadBinding {
    pub(crate) job: SourceJobKey,
    pub(crate) method_scope: &'static str,
    pub(crate) source_site: &'static str,
    pub(crate) lane: &'static str,
    pub(crate) await_ordinal: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceContinuationKey {
    owner: u64,
    generation: u64,
    serial: u64,
}
impl SourceContinuationKey {
    pub(crate) fn serial(self) -> u64 {
        self.serial
    }
}
/// A reference to an actual retained result on the containing nonclone frame.
/// No aggregate value, native view, financial rows or result constructor is
/// imported through this metadata reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ActualPayloadRef {
    owner: u64,
    generation: u64,
    slot: u64,
}
#[derive(Debug, PartialEq, Eq)]
struct RetainedPayload {
    binding: ActualPayloadBinding,
    ready: bool,
}
#[derive(Debug, PartialEq, Eq)]
struct SourceJob {
    method_scope: &'static str,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ActualSettlement {
    Fulfilled(V),
    Payload(ActualPayloadRef),
    Rejected(ActualRejection),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Notification {
    Unhandled {
        promise: PromiseKey,
        rejection: ActualRejection,
    },
    HandledLater {
        promise: PromiseKey,
        rejection: ActualRejection,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Delivery {
    pub(crate) ticket: DeliveryTicket,
    pub(crate) continuation: u64,
    pub(crate) settlement: ActualSettlement,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DeliveryTicket {
    owner: u64,
    generation: u64,
    serial: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Reaction {
    Continue(u64),
    Forward(PromiseKey),
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Job {
    Assimilate {
        destination: PromiseKey,
        source: PromiseKey,
    },
    Reaction {
        reaction: Reaction,
        settlement: ActualSettlement,
    },
}
#[derive(Debug, PartialEq, Eq)]
struct Promise {
    realm: Realm,
    resolver_locked: bool,
    settlement: Option<ActualSettlement>,
    reactions: Vec<Reaction>,
    handled: bool,
    notified: bool,
}
/// No Clone: the containing same-owner Kernel cannot fork its reaction history.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StartupPromiseQueue {
    owner: u64,
    generation: u64,
    next_delivery: u64,
    next_rejection: u64,
    promises: Vec<Promise>,
    source_jobs: Vec<SourceJob>,
    payloads: Vec<RetainedPayload>,
    continuations: Vec<ActualPayloadBinding>,
    ready: VecDeque<Job>,
    active: Option<Delivery>,
    inflight: Option<Job>,
    reactions: usize,
    handled_later: VecDeque<Notification>,
    pending_rejections: VecDeque<PromiseKey>,
    stopped: Option<SourceRuleError>,
}
impl StartupPromiseQueue {
    pub(crate) fn new(generation: u64) -> Result<Self, SourceRuleError> {
        let owner = NEXT_OWNER
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |id| id.checked_add(1),
            )
            .map_err(|_| SourceRuleError::Capacity)?;
        Ok(Self {
            owner,
            generation,
            next_delivery: 1,
            next_rejection: 1,
            promises: vec![],
            source_jobs: vec![],
            payloads: vec![],
            continuations: vec![],
            ready: VecDeque::new(),
            active: None,
            reactions: 0,
            inflight: None,
            handled_later: VecDeque::new(),
            pending_rejections: VecDeque::new(),
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
    fn position(&self, key: PromiseKey) -> Result<usize, SourceRuleError> {
        if key.owner != self.owner || key.generation != self.generation || key.creation == 0 {
            return Err(SourceRuleError::InvalidInput);
        }
        let at = usize::try_from(key.creation - 1).map_err(|_| SourceRuleError::InvalidInput)?;
        if at >= self.promises.len() {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(at)
    }
    pub(crate) fn create(&mut self, realm: Realm) -> Result<PromiseKey, SourceRuleError> {
        self.live()?;
        if self.promises.len() >= MAX_PROMISES || self.promises.try_reserve(1).is_err() {
            return self.capacity();
        }
        let creation = u64::try_from(self.promises.len())
            .ok()
            .and_then(|id| id.checked_add(1))
            .ok_or(SourceRuleError::Capacity)?;
        self.promises.push(Promise {
            realm,
            resolver_locked: false,
            settlement: None,
            reactions: vec![],
            handled: false,
            notified: false,
        });
        Ok(PromiseKey {
            owner: self.owner,
            generation: self.generation,
            creation,
        })
    }
    fn source_job_position(&self, job: SourceJobKey) -> Result<usize, SourceRuleError> {
        if job.owner != self.owner || job.generation != self.generation || job.serial == 0 {
            return Err(SourceRuleError::InvalidInput);
        }
        let position =
            usize::try_from(job.serial - 1).map_err(|_| SourceRuleError::InvalidInput)?;
        if position >= self.source_jobs.len() {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(position)
    }
    pub(crate) fn source_job_is_same_owner(
        &self,
        job: SourceJobKey,
    ) -> Result<(), SourceRuleError> {
        self.source_job_position(job).map(|_| ())
    }
    pub(crate) fn allocate_source_job(
        &mut self,
        method_scope: &'static str,
    ) -> Result<SourceJobKey, SourceRuleError> {
        self.live()?;
        if method_scope.is_empty() || method_scope.len() > 256 {
            return Err(SourceRuleError::InvalidInput);
        }
        if self.source_jobs.len() >= MAX_PROMISES || self.source_jobs.try_reserve(1).is_err() {
            return self.capacity();
        }
        let serial = u64::try_from(self.source_jobs.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(SourceRuleError::Capacity)?;
        self.source_jobs.push(SourceJob { method_scope });
        Ok(SourceJobKey {
            owner: self.owner,
            generation: self.generation,
            serial,
        })
    }
    fn payload_position(&self, payload: ActualPayloadRef) -> Result<usize, SourceRuleError> {
        if payload.owner != self.owner || payload.generation != self.generation || payload.slot == 0
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let position =
            usize::try_from(payload.slot - 1).map_err(|_| SourceRuleError::InvalidInput)?;
        if position >= self.payloads.len() {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(position)
    }
    /// Reserve this exact source location before entering the actual IO call.
    /// The same containing driver retains the actual frame, query and result.
    pub(crate) fn reserve_actual_payload(
        &mut self,
        binding: ActualPayloadBinding,
    ) -> Result<ActualPayloadRef, SourceRuleError> {
        self.live()?;
        self.source_job_position(binding.job)?;
        if binding.await_ordinal == 0
            || [binding.method_scope, binding.source_site, binding.lane]
                .iter()
                .any(|s| s.is_empty() || s.len() > 256)
            || self
                .payloads
                .iter()
                .any(|payload| payload.binding == binding)
        {
            return Err(SourceRuleError::InvalidInput);
        }
        if self.payloads.len() >= MAX_REACTIONS || self.payloads.try_reserve(1).is_err() {
            return self.capacity();
        }
        let slot = u64::try_from(self.payloads.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(SourceRuleError::Capacity)?;
        self.payloads.push(RetainedPayload {
            binding,
            ready: false,
        });
        Ok(ActualPayloadRef {
            owner: self.owner,
            generation: self.generation,
            slot,
        })
    }
    /// Only after the actual retained frame has captured its true completion.
    /// This method takes no business result or financial state from a caller.
    pub(crate) fn mark_actual_payload_ready(
        &mut self,
        payload: ActualPayloadRef,
    ) -> Result<(), SourceRuleError> {
        self.live()?;
        let at = self.payload_position(payload)?;
        if self.payloads[at].ready {
            return Err(SourceRuleError::InvalidInput);
        }
        self.payloads[at].ready = true;
        Ok(())
    }
    pub(crate) fn actual_payload_binding(
        &self,
        payload: ActualPayloadRef,
    ) -> Result<&ActualPayloadBinding, SourceRuleError> {
        Ok(&self.payloads[self.payload_position(payload)?].binding)
    }
    pub(crate) fn actual_payload_is_ready(
        &self,
        payload: ActualPayloadRef,
    ) -> Result<bool, SourceRuleError> {
        Ok(self.payloads[self.payload_position(payload)?].ready)
    }
    pub(crate) fn reserve_source_continuation(
        &mut self,
        binding: ActualPayloadBinding,
    ) -> Result<SourceContinuationKey, SourceRuleError> {
        self.live()?;
        self.source_job_position(binding.job)?;
        if binding.await_ordinal == 0
            || [binding.method_scope, binding.source_site, binding.lane]
                .iter()
                .any(|s| s.is_empty() || s.len() > 256)
        {
            return Err(SourceRuleError::InvalidInput);
        }
        if self.continuations.len() >= MAX_REACTIONS || self.continuations.try_reserve(1).is_err() {
            return self.capacity();
        }
        let serial = u64::try_from(self.continuations.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(SourceRuleError::Capacity)?;
        self.continuations.push(binding);
        Ok(SourceContinuationKey {
            owner: self.owner,
            generation: self.generation,
            serial,
        })
    }
    pub(crate) fn source_continuation_binding(
        &self,
        key: SourceContinuationKey,
    ) -> Result<&ActualPayloadBinding, SourceRuleError> {
        if key.owner != self.owner || key.generation != self.generation || key.serial == 0 {
            return Err(SourceRuleError::InvalidInput);
        }
        let position =
            usize::try_from(key.serial - 1).map_err(|_| SourceRuleError::InvalidInput)?;
        self.continuations
            .get(position)
            .ok_or(SourceRuleError::InvalidInput)
    }
    pub(crate) fn await_routed(
        &mut self,
        realm: Realm,
        promise: PromiseKey,
        continuation: SourceContinuationKey,
    ) -> Result<PromiseKey, SourceRuleError> {
        self.source_continuation_binding(continuation)?;
        self.await_actual(realm, promise, continuation.serial)
    }
    pub(crate) fn actual_rejection(
        &mut self,
        thrown: &MainStepLegacyThrow,
    ) -> Result<ActualRejection, SourceRuleError> {
        self.live()?;
        thrown.code.validate()?;
        if thrown.exception.name.len() > 4096 || thrown.exception.message.len() > 65536 {
            return Err(SourceRuleError::Capacity);
        }
        let identity = self.next_rejection;
        let Some(next) = identity.checked_add(1) else {
            return self.capacity();
        };
        self.next_rejection = next;
        Ok(ActualRejection {
            owner: self.owner,
            identity,
            thrown: thrown.clone(),
        })
    }
    fn reserve_jobs(&mut self, count: usize) -> Result<(), SourceRuleError> {
        if self
            .ready
            .len()
            .checked_add(count)
            .is_none_or(|n| n > MAX_JOBS)
            || self.ready.try_reserve(count).is_err()
        {
            return self.capacity();
        }
        Ok(())
    }
    fn settle_unlocked(
        &mut self,
        key: PromiseKey,
        value: &ActualSettlement,
    ) -> Result<(), SourceRuleError> {
        let at = self.position(key)?;
        if self.promises[at].settlement.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        self.reserve_jobs(self.promises[at].reactions.len())?;
        // Native Promise rejection notifications are ordered by rejection
        // occurrence, including an adoption's actual forwarding settlement.
        // Reserve before touching the retained resolver/settlement prefix.
        let newly_unhandled =
            !self.promises[at].handled && matches!(value, ActualSettlement::Rejected(_));
        if newly_unhandled && self.pending_rejections.try_reserve(1).is_err() {
            return self.capacity();
        }
        if newly_unhandled {
            self.pending_rejections.push_back(key);
        }
        let reactions = std::mem::take(&mut self.promises[at].reactions);
        self.promises[at].settlement = Some(value.clone());
        for reaction in reactions {
            self.ready.push_back(Job::Reaction {
                reaction,
                settlement: value.clone(),
            });
        }
        Ok(())
    }
    /// Only a captured actual primitive completion may call this private seam.
    /// Native Promise values use adopt(), never a SourceValue::Promise token.
    pub(crate) fn resolve_actual(
        &mut self,
        key: PromiseKey,
        value: &ActualSettlement,
    ) -> Result<bool, SourceRuleError> {
        self.live()?;
        let at = self.position(key)?;
        if self.promises[at].resolver_locked {
            return Ok(false);
        }
        if matches!(value,ActualSettlement::Rejected(rejection) if rejection.owner!=self.owner) {
            return Err(SourceRuleError::InvalidInput);
        }
        if let ActualSettlement::Payload(payload) = value {
            if !self.actual_payload_is_ready(*payload)? {
                return Err(SourceRuleError::InvalidInput);
            }
        }
        if let ActualSettlement::Fulfilled(value) = value {
            value.validate()?;
            if value == &V::Promise {
                return Err(SourceRuleError::InvalidInput);
            }
        }
        self.reserve_jobs(self.promises[at].reactions.len())?;
        self.promises[at].resolver_locked = true;
        self.settle_unlocked(key, value)?;
        Ok(true)
    }
    /// Promise resolve-with-Promise queues the actual thenable-assimilation job.
    /// It locks the resolver immediately even while the adopted source is pending.
    pub(crate) fn adopt(
        &mut self,
        destination: PromiseKey,
        source: PromiseKey,
    ) -> Result<bool, SourceRuleError> {
        self.live()?;
        let at = self.position(destination)?;
        self.position(source)?;
        if self.promises[at].resolver_locked {
            return Ok(false);
        }
        if destination == source {
            let rejection = self.actual_rejection(&MainStepLegacyThrow {
                exception: crate::validity_source::SourceException {
                    name: "TypeError".into(),
                    message: "Chaining cycle detected for promise #<Promise>".into(),
                },
                code: V::Undefined,
            })?;
            return self.resolve_actual(destination, &ActualSettlement::Rejected(rejection));
        }
        self.reserve_jobs(1)?;
        self.promises[at].resolver_locked = true;
        self.ready.push_back(Job::Assimilate {
            destination,
            source,
        });
        Ok(true)
    }
    fn subscribe(&mut self, key: PromiseKey, reaction: Reaction) -> Result<(), SourceRuleError> {
        let at = self.position(key)?;
        if self.reactions >= MAX_REACTIONS {
            return self.capacity();
        }
        let settled = self.promises[at].settlement.clone();
        if settled.is_some() {
            self.reserve_jobs(1)?;
        } else if self.promises[at].reactions.try_reserve(1).is_err() {
            return self.capacity();
        }
        let handled_later = if self.promises[at].notified && !self.promises[at].handled {
            match &settled {
                Some(ActualSettlement::Rejected(rejection)) => Some(Notification::HandledLater {
                    promise: key,
                    rejection: rejection.clone(),
                }),
                _ => None,
            }
        } else {
            None
        };
        if handled_later.is_some() && self.handled_later.try_reserve(1).is_err() {
            return self.capacity();
        }
        self.promises[at].handled = true;
        self.reactions += 1;
        if let Some(settlement) = settled {
            self.ready.push_back(Job::Reaction {
                reaction,
                settlement,
            });
        } else {
            self.promises[at].reactions.push(reaction);
        }
        if let Some(notification) = handled_later {
            self.handled_later.push_back(notification);
        }
        Ok(())
    }
    pub(crate) fn await_actual(
        &mut self,
        realm: Realm,
        promise: PromiseKey,
        continuation: u64,
    ) -> Result<PromiseKey, SourceRuleError> {
        self.live()?;
        let at = self.position(promise)?;
        if continuation == 0 {
            return Err(SourceRuleError::InvalidInput);
        }
        let awaited = if self.promises[at].realm == realm {
            promise
        } else {
            let wrapper = self.create(realm)?;
            self.adopt(wrapper, promise)?;
            wrapper
        };
        self.subscribe(awaited, Reaction::Continue(continuation))?;
        Ok(awaited)
    }
    /// Drains only Promise machinery. Actual source execution is returned as a
    /// retained delivery, then acknowledged only after its SAME owner segment.
    pub(crate) fn next_delivery(&mut self) -> Result<Option<Delivery>, SourceRuleError> {
        self.live()?;
        if self.active.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        while let Some(job) = self.ready.pop_front() {
            self.inflight = Some(job.clone());
            match job {
                Job::Assimilate {
                    destination,
                    source,
                } => {
                    self.subscribe(source, Reaction::Forward(destination))?;
                }
                Job::Reaction {
                    reaction: Reaction::Forward(destination),
                    settlement,
                } => {
                    self.settle_unlocked(destination, &settlement)?;
                }
                Job::Reaction {
                    reaction: Reaction::Continue(continuation),
                    settlement,
                } => {
                    let serial = self.next_delivery;
                    let Some(next) = serial.checked_add(1) else {
                        return self.capacity();
                    };
                    self.next_delivery = next;
                    let delivery = Delivery {
                        ticket: DeliveryTicket {
                            owner: self.owner,
                            generation: self.generation,
                            serial,
                        },
                        continuation,
                        settlement,
                    };
                    self.active = Some(delivery.clone());
                    self.inflight = None;
                    return Ok(Some(delivery));
                }
            }
            self.inflight = None;
        }
        Ok(None)
    }
    pub(crate) fn acknowledge(&mut self, ticket: DeliveryTicket) -> Result<(), SourceRuleError> {
        self.live()?;
        if self
            .active
            .as_ref()
            .is_none_or(|delivery| delivery.ticket != ticket)
        {
            return Err(SourceRuleError::InvalidInput);
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
        fn text(s: impl Into<String>) -> V {
            V::String(s.into())
        }
        fn key(
            path: &str,
            key: PromiseKey,
            visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
        ) -> Result<(), SourceRuleError> {
            visitor(&format!("{path}/owner"), &text(key.owner.to_string()))?;
            visitor(
                &format!("{path}/generation"),
                &text(key.generation.to_string()),
            )?;
            visitor(&format!("{path}/creation"), &text(key.creation.to_string()))
        }
        fn value(
            path: &str,
            value: &V,
            visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
        ) -> Result<(), SourceRuleError> {
            match value {
                V::Array(values) => {
                    visitor(&format!("{path}/kind"), &text("Array"))?;
                    visitor(&format!("{path}/count"), &text(values.len().to_string()))?;
                    for (at, item) in values.iter().enumerate() {
                        value_visit(&format!("{path}/{at}"), item, visitor)?;
                    }
                }
                V::Object(fields) => {
                    visitor(&format!("{path}/kind"), &text("Object"))?;
                    visitor(&format!("{path}/count"), &text(fields.len().to_string()))?;
                    for (at, (name, item)) in fields.iter().enumerate() {
                        visitor(&format!("{path}/{at}/key"), &text(name))?;
                        value_visit(&format!("{path}/{at}/value"), item, visitor)?;
                    }
                }
                scalar => visitor(path, scalar)?,
            }
            Ok(())
        }
        fn value_visit(
            path: &str,
            v: &V,
            visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
        ) -> Result<(), SourceRuleError> {
            value(path, v, visitor)
        }
        fn result(
            path: &str,
            result: &ActualSettlement,
            visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
        ) -> Result<(), SourceRuleError> {
            match result {
                ActualSettlement::Fulfilled(v) => {
                    visitor(&format!("{path}/kind"), &text("Fulfilled"))?;
                    value(&format!("{path}/value"), v, visitor)
                }
                ActualSettlement::Payload(payload) => {
                    visitor(&format!("{path}/kind"), &text("ActualPayload"))?;
                    visitor(&format!("{path}/owner"), &text(payload.owner.to_string()))?;
                    visitor(
                        &format!("{path}/generation"),
                        &text(payload.generation.to_string()),
                    )?;
                    visitor(&format!("{path}/slot"), &text(payload.slot.to_string()))
                }
                ActualSettlement::Rejected(error) => {
                    visitor(&format!("{path}/kind"), &text("Rejected"))?;
                    visitor(
                        &format!("{path}/errorOwner"),
                        &text(error.owner.to_string()),
                    )?;
                    visitor(
                        &format!("{path}/errorIdentity"),
                        &text(error.identity.to_string()),
                    )?;
                    visitor(&format!("{path}/name"), &text(&error.thrown.exception.name))?;
                    visitor(
                        &format!("{path}/message"),
                        &text(&error.thrown.exception.message),
                    )?;
                    value(&format!("{path}/code"), &error.thrown.code, visitor)
                }
            }
        }
        fn reaction(
            path: &str,
            reaction: &Reaction,
            visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
        ) -> Result<(), SourceRuleError> {
            match reaction {
                Reaction::Continue(id) => {
                    visitor(&format!("{path}/kind"), &text("Continue"))?;
                    visitor(&format!("{path}/continuation"), &text(id.to_string()))
                }
                Reaction::Forward(destination) => {
                    visitor(&format!("{path}/kind"), &text("Forward"))?;
                    key(&format!("{path}/destination"), *destination, visitor)
                }
            }
        }
        fn job(
            path: &str,
            job: &Job,
            visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
        ) -> Result<(), SourceRuleError> {
            match job {
                Job::Assimilate {
                    destination,
                    source,
                } => {
                    visitor(&format!("{path}/kind"), &text("Assimilate"))?;
                    key(&format!("{path}/destination"), *destination, visitor)?;
                    key(&format!("{path}/source"), *source, visitor)
                }
                Job::Reaction {
                    reaction: r,
                    settlement,
                } => {
                    visitor(&format!("{path}/kind"), &text("Reaction"))?;
                    reaction(&format!("{path}/reaction"), r, visitor)?;
                    result(&format!("{path}/settlement"), settlement, visitor)
                }
            }
        }
        visitor(
            "profile",
            &text("TL-MAIN-VM-HOST-INTRINSIC-PROMISE-QUEUE-13-1"),
        )?;
        visitor("owner", &text(self.owner.to_string()))?;
        visitor("generation", &text(self.generation.to_string()))?;
        visitor("nextDelivery", &text(self.next_delivery.to_string()))?;
        visitor("nextRejection", &text(self.next_rejection.to_string()))?;
        visitor("totalReactions", &text(self.reactions.to_string()))?;
        visitor(
            "stopped",
            &self
                .stopped
                .as_ref()
                .map_or(V::Null, |error| text(format!("{error:?}"))),
        )?;
        visitor(
            "sourceJobs/count",
            &text(self.source_jobs.len().to_string()),
        )?;
        for (at, job) in self.source_jobs.iter().enumerate() {
            visitor(
                &format!("sourceJobs/{at}/methodScope"),
                &text(job.method_scope),
            )?;
        }
        visitor(
            "continuations/count",
            &text(self.continuations.len().to_string()),
        )?;
        for (at, binding) in self.continuations.iter().enumerate() {
            let path = format!("continuations/{at}");
            visitor(
                &format!("{path}/job"),
                &text(binding.job.serial.to_string()),
            )?;
            visitor(&format!("{path}/methodScope"), &text(binding.method_scope))?;
            visitor(&format!("{path}/sourceSite"), &text(binding.source_site))?;
            visitor(&format!("{path}/lane"), &text(binding.lane))?;
            visitor(
                &format!("{path}/awaitOrdinal"),
                &text(binding.await_ordinal.to_string()),
            )?;
        }
        visitor("payloads/count", &text(self.payloads.len().to_string()))?;
        for (at, payload) in self.payloads.iter().enumerate() {
            let path = format!("payloads/{at}");
            visitor(
                &format!("{path}/job"),
                &text(payload.binding.job.serial.to_string()),
            )?;
            visitor(
                &format!("{path}/methodScope"),
                &text(payload.binding.method_scope),
            )?;
            visitor(
                &format!("{path}/sourceSite"),
                &text(payload.binding.source_site),
            )?;
            visitor(&format!("{path}/lane"), &text(payload.binding.lane))?;
            visitor(
                &format!("{path}/awaitOrdinal"),
                &text(payload.binding.await_ordinal.to_string()),
            )?;
            visitor(&format!("{path}/ready"), &V::Bool(payload.ready))?;
        }
        visitor("promises/count", &text(self.promises.len().to_string()))?;
        for (at, promise) in self.promises.iter().enumerate() {
            let path = format!("promises/{at}");
            visitor(
                &format!("{path}/realm"),
                &text(match promise.realm {
                    Realm::SourceBusiness => "SourceBusiness",
                    Realm::RecordedHostIo => "RecordedHostIo",
                }),
            )?;
            for (name, flag) in [
                ("resolverLocked", promise.resolver_locked),
                ("handled", promise.handled),
                ("notified", promise.notified),
            ] {
                visitor(&format!("{path}/{name}"), &V::Bool(flag))?;
            }
            visitor(
                &format!("{path}/settled"),
                &V::Bool(promise.settlement.is_some()),
            )?;
            if let Some(settlement) = &promise.settlement {
                result(&format!("{path}/settlement"), settlement, visitor)?;
            }
            visitor(
                &format!("{path}/reactions/count"),
                &text(promise.reactions.len().to_string()),
            )?;
            for (at, r) in promise.reactions.iter().enumerate() {
                reaction(&format!("{path}/reactions/{at}"), r, visitor)?;
            }
        }
        visitor("ready/count", &text(self.ready.len().to_string()))?;
        for (at, task) in self.ready.iter().enumerate() {
            job(&format!("ready/{at}"), task, visitor)?;
        }
        visitor("inflight/present", &V::Bool(self.inflight.is_some()))?;
        if let Some(task) = &self.inflight {
            job("inflight", task, visitor)?;
        }
        visitor("active/present", &V::Bool(self.active.is_some()))?;
        if let Some(delivery) = &self.active {
            visitor("active/ticket", &text(delivery.ticket.serial.to_string()))?;
            visitor(
                "active/ticketOwner",
                &text(delivery.ticket.owner.to_string()),
            )?;
            visitor(
                "active/ticketGeneration",
                &text(delivery.ticket.generation.to_string()),
            )?;
            visitor(
                "active/continuation",
                &text(delivery.continuation.to_string()),
            )?;
            result("active/settlement", &delivery.settlement, visitor)?;
        }
        visitor(
            "pendingRejections/count",
            &text(self.pending_rejections.len().to_string()),
        )?;
        for (at, promise) in self.pending_rejections.iter().enumerate() {
            key(&format!("pendingRejections/{at}"), *promise, visitor)?;
        }
        visitor(
            "handledLater/count",
            &text(self.handled_later.len().to_string()),
        )?;
        for (at, notice) in self.handled_later.iter().enumerate() {
            let (promise, error) = match notice {
                Notification::Unhandled { promise, rejection }
                | Notification::HandledLater { promise, rejection } => (*promise, rejection),
            };
            key(&format!("handledLater/{at}/promise"), promise, visitor)?;
            result(
                &format!("handledLater/{at}/error"),
                &ActualSettlement::Rejected(error.clone()),
                visitor,
            )?;
        }
        Ok(())
    }
    pub(crate) fn end_turn(&mut self) -> Result<Vec<Notification>, SourceRuleError> {
        self.live()?;
        if !self.ready.is_empty() || self.active.is_some() || self.inflight.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        let Some(count) = self
            .pending_rejections
            .len()
            .checked_add(self.handled_later.len())
        else {
            return self.capacity();
        };
        let mut out = Vec::new();
        if out.try_reserve(count).is_err() {
            return self.capacity();
        }
        // Node's rejection pass first reports pending rejectionHandled events,
        // then newly unhandled promises in their rejection-occurrence FIFO.
        out.extend(self.handled_later.drain(..));
        while let Some(key) = self.pending_rejections.pop_front() {
            let at = self.position(key)?;
            let promise = &mut self.promises[at];
            if !promise.handled && !promise.notified {
                if let Some(ActualSettlement::Rejected(rejection)) = &promise.settlement {
                    promise.notified = true;
                    out.push(Notification::Unhandled {
                        promise: key,
                        rejection: rejection.clone(),
                    });
                }
            }
        }
        Ok(out)
    }
}

include!("source_main_startup_promise_key_impl.rs");
