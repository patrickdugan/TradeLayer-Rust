// Retained literal extractBlockData caller. Its final save has a separate
// lifetime: caller outcome can exist while the same child DB await is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxIndexExtractAwaitRoleV1 {
    Awaited,
    Detached,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxIndexExtractPendingV1 {
    parent_scope: u64,
    source_job: u64,
    method_scope: u64,
    operation_ordinal: u64,
    role: TxIndexExtractAwaitRoleV1,
}
impl TxIndexExtractPendingV1 {
    pub fn value(&self) -> V {
        obj(&[
            ("profile", s("TL-TXINDEX-EXTRACT-PENDING-13-1")),
            ("parentScope", s(self.parent_scope.to_string())),
            ("sourceJob", s(self.source_job().to_string())),
            ("methodScope", s(self.method_scope.to_string())),
            ("operationOrdinal", s(self.operation_ordinal().to_string())),
            (
                "role",
                s(match self.role() {
                    TxIndexExtractAwaitRoleV1::Awaited => "Awaited",
                    TxIndexExtractAwaitRoleV1::Detached => "Detached",
                }),
            ),
        ])
    }
    pub(crate) fn method_scope(&self) -> u64 {
        self.method_scope
    }
    pub(crate) fn source_job(&self) -> u64 {
        self.source_job
    }
    pub(crate) fn operation_ordinal(&self) -> u64 {
        self.operation_ordinal
    }
    pub(crate) fn role(&self) -> TxIndexExtractAwaitRoleV1 {
        self.role
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TxIndexExtractPcV1 {
    Begin,
    Height,
    ProcessBlock,
    Detached,
    Done,
}
struct TxIndexExtractChildV1 {
    source_job: u64,
    scope: u64,
    role: TxIndexExtractAwaitRoleV1,
    source: Box<TxIndexInlineContinuationV1>,
}
pub(crate) struct TxIndexExtractContinuationV1 {
    scope: u64,
    owner_state: V,
    arguments: Vec<V>,
    pc: TxIndexExtractPcV1,
    frame: Frame,
    effective: V,
    tip: V,
    height: V,
    blocks: usize,
    next_scope: u64,
    next_job: u64,
    child: Option<TxIndexExtractChildV1>,
    caller: Option<SourceOutcome>,
    caller_announced: bool,
    local_stop: Option<SourceRuleError>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TxIndexExtractStopV1 {
    AwaitDatabase,
    CallerReturned,
    Complete,
}
impl TxIndexInlineContinuationV1 {
    pub(crate) fn pending_write_ordinal(&self) -> u64 {
        self.frame
            .observations
            .iter()
            .filter(|o| matches!(o.call.target.as_str(), "db.insertAsync" | "db.updateAsync"))
            .count() as u64
    }
}
impl TxIndexExtractContinuationV1 {
    pub(crate) fn caller_outcome(&self) -> Option<&SourceOutcome> {
        self.caller.as_ref()
    }
    pub(crate) fn detached_pending(&self) -> bool {
        self.child
            .as_ref()
            .is_some_and(|c| c.role == TxIndexExtractAwaitRoleV1::Detached)
    }
    pub(crate) fn pending_call(&self) -> Option<&DependencyCall> {
        self.child.as_ref()?.source.pending_call()
    }
    pub(crate) fn pending_descriptor(&self) -> Option<TxIndexExtractPendingV1> {
        let child = self.child.as_ref()?;
        child.source.pending_call()?;
        let operation_ordinal = child.source.pending_write_ordinal();
        Some(TxIndexExtractPendingV1 {
            parent_scope: self.scope,
            source_job: child.source_job,
            method_scope: child.scope,
            operation_ordinal,
            role: child.role,
        })
    }
    pub(crate) fn pending_source_mut(&mut self) -> Option<&mut TxIndexInlineContinuationV1> {
        Some(self.child.as_mut()?.source.as_mut())
    }
    pub(crate) fn take_evaluation(&mut self) -> Result<SourceEvaluation, SourceRuleError> {
        if self.pc != TxIndexExtractPcV1::Done || self.child.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(SourceEvaluation {
            outcome: self.caller.take().ok_or(SourceRuleError::InvalidInput)?,
            arguments_after: self.arguments.clone(),
            dependencies: std::mem::take(&mut self.frame.observations),
            deferred_rejections: std::mem::take(&mut self.frame.deferred),
            global_writes: vec![],
        })
    }
    pub(crate) fn visit_fields(&self, visitor: &mut dyn FnMut(&str, &V)) {
        visitor("profile", &s("TL-TXINDEX-RETAINED-EXTRACT-13-1"));
        visitor("scope", &s(self.scope.to_string()));
        visitor("ownerState", &self.owner_state);
        visitor("pc", &s(format!("{:?}", self.pc)));
        for (name, value) in [
            ("effectiveStart", &self.effective),
            ("tip", &self.tip),
            ("height", &self.height),
        ] {
            visitor(name, value);
        }
        visitor("blocks", &s(self.blocks.to_string()));
        visitor("nextScope", &s(self.next_scope.to_string()));
        visitor("nextJob", &s(self.next_job.to_string()));
        visitor("callerAnnounced", &V::Bool(self.caller_announced));
        visitor("traceBytes", &s(self.frame.trace_bytes.to_string()));
        visitor("arguments/length", &s(self.arguments.len().to_string()));
        for (at, value) in self.arguments.iter().enumerate() {
            visitor(&format!("arguments/{at}"), value);
        }
        visitor("caller/present", &V::Bool(self.caller.is_some()));
        if let Some(result) = &self.caller {
            match result {
                SourceOutcome::Returned(value) => visitor("caller/returned", value),
                SourceOutcome::Thrown(error) => {
                    visitor("caller/name", &s(&error.name));
                    visitor("caller/message", &s(&error.message));
                }
            }
        }
        visitor(
            "localStop",
            &self
                .local_stop
                .as_ref()
                .map_or(V::Null, |e| s(format!("{e:?}"))),
        );
        visitor("child/present", &V::Bool(self.child.is_some()));
        if let Some(child) = &self.child {
            visitor("child/sourceJob", &s(child.source_job.to_string()));
            visitor("child/scope", &s(child.scope.to_string()));
            visitor("child/role", &s(format!("{:?}", child.role)));
            child
                .source
                .visit_fields(&mut |p, v| visitor(&format!("child/source/{p}"), v));
        }
        visitor("pending/present", &V::Bool(self.pending_call().is_some()));
        if let Some(descriptor) = self.pending_descriptor() {
            visitor("pending/descriptor", &descriptor.value());
        }
        visitor(
            "observations/length",
            &s(self.frame.observations.len().to_string()),
        );
        for (at, o) in self.frame.observations.iter().enumerate() {
            visitor(&format!("observations/{at}/target"), &s(&o.call.target));
            visitor(
                &format!("observations/{at}/arguments"),
                &V::Array(o.call.arguments.clone()),
            );
            inline_visit_return(&format!("observations/{at}/returned"), &o.returned, visitor);
        }
        visitor("deferred/length", &s(self.frame.deferred.len().to_string()));
        for (at, e) in self.frame.deferred.iter().enumerate() {
            visitor(&format!("deferred/{at}/name"), &s(&e.name));
            visitor(&format!("deferred/{at}/message"), &s(&e.message));
        }
    }
}
impl TxIndexSourceOwner {
    pub(crate) fn begin_retained_extract(
        &self,
        scope: u64,
        start: V,
    ) -> Result<TxIndexExtractContinuationV1, SourceRuleError> {
        start.validate()?;
        let next_scope = scope
            .checked_add(1)
            .filter(|_| scope != 0)
            .ok_or(SourceRuleError::InvalidInput)?;
        Ok(TxIndexExtractContinuationV1 {
            scope,
            owner_state: self.state_value()?,
            arguments: vec![start.clone()],
            pc: TxIndexExtractPcV1::Begin,
            frame: Frame::default(),
            effective: start.clone(),
            tip: V::Undefined,
            height: start,
            blocks: 0,
            next_scope,
            next_job: 0,
            child: None,
            caller: None,
            caller_announced: false,
            local_stop: None,
        })
    }
    fn require_retained_extract(
        &self,
        scope: u64,
        c: &TxIndexExtractContinuationV1,
    ) -> Result<(), SourceRuleError> {
        if scope != c.scope || self.state_value()? != c.owner_state {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(())
    }
    fn retained_extract_child(
        &self,
        c: &mut TxIndexExtractContinuationV1,
        method: TxIndexInlineMethodV1,
        args: Vec<V>,
        role: TxIndexExtractAwaitRoleV1,
    ) -> Result<(), SourceRuleError> {
        if c.child.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        let scope = c.next_scope;
        c.next_scope = scope.checked_add(1).ok_or(SourceRuleError::Capacity)?;
        let job = c.next_job;
        c.next_job = job.checked_add(1).ok_or(SourceRuleError::Capacity)?;
        c.child = Some(TxIndexExtractChildV1 {
            source_job: job,
            scope,
            role,
            source: Box::new(self.begin_inline_source(scope, method, args)?),
        });
        Ok(())
    }
    pub(crate) fn advance_retained_extract<B: SourceBackend>(
        &mut self,
        scope: u64,
        c: &mut TxIndexExtractContinuationV1,
        b: &mut B,
    ) -> Result<TxIndexExtractStopV1, SourceRuleError> {
        self.require_retained_extract(scope, c)?;
        if let Some(error) = &c.local_stop {
            return Err(error.clone());
        }
        let result = self.advance_retained_extract_inner(c, b);
        match result {
            Ok(stop) => Ok(stop),
            Err(Fault::Local(error)) => {
                c.local_stop = Some(error.clone());
                Err(error)
            }
            Err(Fault::Source(error)) => {
                if c.caller.is_some() {
                    c.frame.deferred.push(error);
                } else {
                    c.caller = Some(SourceOutcome::Thrown(error));
                }
                c.pc = TxIndexExtractPcV1::Done;
                c.child = None;
                Ok(TxIndexExtractStopV1::Complete)
            }
        }
    }
    fn advance_retained_extract_inner<B: SourceBackend>(
        &mut self,
        c: &mut TxIndexExtractContinuationV1,
        b: &mut B,
    ) -> R<TxIndexExtractStopV1> {
        loop {
            match c.pc {
                TxIndexExtractPcV1::Begin => {
                    c.tip = self.tip(&mut c.frame, b)?;
                    if self.forward == s("1") || self.forward == s("true") {
                        match self.max_height(&mut c.frame, b) {
                            Ok(V::Undefined | V::Null) => c.effective = c.tip.clone(),
                            Ok(value) => {
                                c.effective = match value {
                                    V::String(text) => s(format!("{text}1")),
                                    V::Object(_) | V::Array(_) => s(format!("{}1", text(&value)?)),
                                    _ => V::number(number(&value)? + 1.),
                                }
                            }
                            Err(Fault::Source(_)) => (),
                            Err(error) => return Err(error),
                        }
                    }
                    if matches!(c.effective, V::Undefined | V::Null) {
                        c.effective = V::number(0.)
                    }
                    c.height = c.effective.clone();
                    if compare(&c.height, &c.tip)? == Some(core::cmp::Ordering::Greater) {
                        c.caller = Some(SourceOutcome::Returned(V::Undefined));
                        c.pc = TxIndexExtractPcV1::Done;
                        return Ok(TxIndexExtractStopV1::Complete);
                    }
                    c.pc = TxIndexExtractPcV1::Height;
                }
                TxIndexExtractPcV1::Height => {
                    if !matches!(
                        compare(&c.height, &c.tip)?,
                        Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
                    ) {
                        let args = vec![c.tip.clone()];
                        self.retained_extract_child(
                            c,
                            TxIndexInlineMethodV1::SaveMaxHeight,
                            args,
                            TxIndexExtractAwaitRoleV1::Detached,
                        )?;
                        // Execute the real detached method's leading DB factory now,
                        // but give the caller its own completion turn before IO.
                        let child = c.child.as_mut().expect("actual final save child");
                        self.advance_inline_source(child.scope, &mut child.source, b)?;
                        c.caller = Some(SourceOutcome::Returned(V::Undefined));
                        c.pc = TxIndexExtractPcV1::Detached;
                        c.caller_announced = true;
                        return Ok(TxIndexExtractStopV1::CallerReturned);
                    }
                    if c.blocks >= MAX_BLOCKS {
                        return Err(SourceRuleError::Capacity.into());
                    }
                    c.blocks += 1;
                    self.class_parse_block = c.height.clone();
                    c.owner_state = self.state_value()?;
                    let block = self.block(c.height.clone(), &mut c.frame, b)?;
                    let args = vec![block, c.height.clone()];
                    self.retained_extract_child(
                        c,
                        TxIndexInlineMethodV1::ProcessBlock,
                        args,
                        TxIndexExtractAwaitRoleV1::Awaited,
                    )?;
                    c.pc = TxIndexExtractPcV1::ProcessBlock;
                }
                TxIndexExtractPcV1::ProcessBlock | TxIndexExtractPcV1::Detached => {
                    let child = c.child.as_mut().expect("retained actual source child");
                    match self.advance_inline_source(child.scope, &mut child.source, b)? {
                        TxIndexInlineStopV1::AwaitDatabase => {
                            return Ok(TxIndexExtractStopV1::AwaitDatabase)
                        }
                        TxIndexInlineStopV1::Complete => {
                            let evaluation = child.source.take_evaluation()?;
                            for observed in evaluation.dependencies {
                                c.frame.observe(observed.call, observed.returned)?;
                            }
                            c.frame.deferred.extend(evaluation.deferred_rejections);
                            let detached = child.role == TxIndexExtractAwaitRoleV1::Detached;
                            c.child = None;
                            match evaluation.outcome {
                                SourceOutcome::Returned(_) => (),
                                SourceOutcome::Thrown(error) if detached => {
                                    c.frame.deferred.push(error)
                                }
                                SourceOutcome::Thrown(error) => return Err(Fault::Source(error)),
                            }
                            if detached {
                                c.pc = TxIndexExtractPcV1::Done;
                                return Ok(TxIndexExtractStopV1::Complete);
                            }
                            c.height = V::number(number(&c.height)? + 1.);
                            c.pc = TxIndexExtractPcV1::Height;
                        }
                    }
                }
                TxIndexExtractPcV1::Done => return Ok(TxIndexExtractStopV1::Complete),
            }
        }
    }
    pub(crate) fn resolve_retained_extract_source(
        &self,
        scope: u64,
        c: &mut TxIndexExtractContinuationV1,
        descriptor: &TxIndexExtractPendingV1,
        returned: DependencyReturn,
    ) -> Result<(), SourceRuleError> {
        self.require_retained_extract(scope, c)?;
        if c.pending_descriptor().as_ref() != Some(descriptor) {
            return Err(SourceRuleError::InvalidInput);
        }
        let child = c.child.as_mut().ok_or(SourceRuleError::InvalidInput)?;
        self.resolve_inline_source(child.scope, &mut child.source, returned)
    }
}
