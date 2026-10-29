// Additive source-method continuation. No legacy owner field or encoding changes.
// Only the actual DB Promise is suspended; decoder/RPC/source prefixes are not replayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxIndexInlineMethodV1 {
    ProcessBlock,
    SaveMaxHeight,
    Initialize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TxIndexInlinePcV1 {
    Begin,
    NextTransaction,
    SaveHeightFactory,
    MaxHeight,
    IndexExists,
    InitializeFind,
    InitializeInsert,
    Finished,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TxIndexInlineStopV1 {
    AwaitDatabase,
    Complete,
}
pub(crate) struct TxIndexInlineContinuationV1 {
    owner_state: V,
    scope: u64,
    method: TxIndexInlineMethodV1,
    arguments: Vec<V>,
    pc: TxIndexInlinePcV1,
    frame: Frame,
    database: V,
    height_database: V,
    block: V,
    height: V,
    ids: Vec<V>,
    at: usize,
    rows: Vec<V>,
    pending: Option<DependencyCall>,
    result: Option<R<V>>,
}
impl TxIndexInlineContinuationV1 {
    pub(crate) fn pending_call(&self) -> Option<&DependencyCall> {
        self.pending.as_ref()
    }
    pub(crate) fn take_evaluation(&mut self) -> Result<SourceEvaluation, SourceRuleError> {
        if self.pc != TxIndexInlinePcV1::Finished || self.pending.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        TxIndexSourceOwner::evaluation(
            self.result.take().ok_or(SourceRuleError::InvalidInput)?,
            self.arguments.clone(),
            std::mem::take(&mut self.frame),
        )
    }
    // Stream every retained scalar/value tree, including the real pending call.
    // A containing source owner supplies its normal lossless V visitor.
    pub(crate) fn visit_fields(&self, visitor: &mut dyn FnMut(&str, &V)) {
        visitor("profile", &s("TL-TXINDEX-INLINE-SOURCE-PC-13-1"));
        visitor("scope", &s(self.scope.to_string()));
        visitor("method", &s(format!("{:?}", self.method)));
        visitor("pc", &s(format!("{:?}", self.pc)));
        visitor("ownerState", &self.owner_state);
        visitor("database", &self.database);
        visitor("heightDatabase", &self.height_database);
        visitor("block", &self.block);
        visitor("height", &self.height);
        visitor("at", &s(self.at.to_string()));
        visitor("traceBytes", &s(self.frame.trace_bytes.to_string()));
        for (name, values) in [
            ("arguments", &self.arguments),
            ("ids", &self.ids),
            ("rows", &self.rows),
        ] {
            visitor(&format!("{name}/length"), &s(values.len().to_string()));
            for (i, value) in values.iter().enumerate() {
                visitor(&format!("{name}/{i}"), value);
            }
        }
        visitor("pending", &V::Bool(self.pending.is_some()));
        if let Some(call) = &self.pending {
            visitor("pending/target", &s(&call.target));
            visitor("pending/arguments", &V::Array(call.arguments.clone()));
        }
        visitor("result", &V::Bool(self.result.is_some()));
        if let Some(result) = &self.result {
            match result {
                Ok(value) => visitor("result/returned", value),
                Err(Fault::Source(e)) => {
                    visitor("result/source/name", &s(&e.name));
                    visitor("result/source/message", &s(&e.message));
                }
                Err(Fault::Local(e)) => visitor("result/local", &s(format!("{e:?}"))),
            }
        }
        visitor(
            "observations/length",
            &s(self.frame.observations.len().to_string()),
        );
        for (i, observation) in self.frame.observations.iter().enumerate() {
            visitor(
                &format!("observations/{i}/target"),
                &s(&observation.call.target),
            );
            visitor(
                &format!("observations/{i}/arguments"),
                &V::Array(observation.call.arguments.clone()),
            );
            inline_visit_return(
                &format!("observations/{i}/returned"),
                &observation.returned,
                visitor,
            );
        }
        visitor("deferred/length", &s(self.frame.deferred.len().to_string()));
        for (i, error) in self.frame.deferred.iter().enumerate() {
            visitor(&format!("deferred/{i}/name"), &s(&error.name));
            visitor(&format!("deferred/{i}/message"), &s(&error.message));
        }
    }
    fn suspend(&mut self, method: &str, arguments: Vec<V>) -> R<TxIndexInlineStopV1> {
        if self.pending.is_some() || self.frame.observations.len() >= MAX_DEPENDENCIES {
            return Err(SourceRuleError::Capacity.into());
        }
        for argument in &arguments {
            argument.validate()?;
        }
        let database = if matches!(
            self.pc,
            TxIndexInlinePcV1::MaxHeight | TxIndexInlinePcV1::IndexExists
        ) {
            self.height_database.clone()
        } else {
            self.database.clone()
        };
        if matches!(database, V::Null | V::Undefined) {
            return Err(ty(format!(
                "Cannot read properties of {} (reading '{method}')",
                text(&database)?
            )));
        }
        let mut arguments = arguments;
        arguments.insert(0, database);
        self.pending = Some(DependencyCall {
            target: format!("db.{method}"),
            arguments,
        });
        Ok(TxIndexInlineStopV1::AwaitDatabase)
    }
    fn complete(&mut self, result: R<V>) -> TxIndexInlineStopV1 {
        self.result = Some(result);
        self.pc = TxIndexInlinePcV1::Finished;
        TxIndexInlineStopV1::Complete
    }
}
fn inline_visit_return(prefix: &str, value: &DependencyReturn, visitor: &mut dyn FnMut(&str, &V)) {
    let (kind, returned, thrown) = match value {
        DependencyReturn::Immediate(v) => ("Immediate", Some(v), None),
        DependencyReturn::Pending(v) => ("Pending", Some(v), None),
        DependencyReturn::Throw(e) => ("Throw", None, Some(e)),
        DependencyReturn::PendingThrow(e) => ("PendingThrow", None, Some(e)),
    };
    visitor(&format!("{prefix}/kind"), &s(kind));
    if let Some(v) = returned {
        visitor(&format!("{prefix}/value"), v);
    }
    if let Some(e) = thrown {
        visitor(&format!("{prefix}/name"), &s(&e.name));
        visitor(&format!("{prefix}/message"), &s(&e.message));
    }
}
impl TxIndexSourceOwner {
    pub(crate) fn begin_inline_source(
        &self,
        scope: u64,
        method: TxIndexInlineMethodV1,
        arguments: Vec<V>,
    ) -> Result<TxIndexInlineContinuationV1, SourceRuleError> {
        if scope == 0 || arguments.len() > 16 {
            return Err(SourceRuleError::InvalidInput);
        }
        for value in &arguments {
            value.validate()?;
        }
        Ok(TxIndexInlineContinuationV1 {
            owner_state: self.state_value()?,
            scope,
            method,
            height: arg(
                &arguments,
                if method == TxIndexInlineMethodV1::ProcessBlock {
                    1
                } else {
                    0
                },
            ),
            block: if method == TxIndexInlineMethodV1::ProcessBlock {
                arg(&arguments, 0)
            } else {
                V::Undefined
            },
            arguments,
            pc: TxIndexInlinePcV1::Begin,
            frame: Frame::default(),
            database: V::Undefined,
            height_database: V::Undefined,
            ids: vec![],
            at: 0,
            rows: vec![],
            pending: None,
            result: None,
        })
    }
    pub(crate) fn require_inline_owner(
        &self,
        scope: u64,
        continuation: &TxIndexInlineContinuationV1,
    ) -> Result<(), SourceRuleError> {
        if scope != continuation.scope || self.state_value()? != continuation.owner_state {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(())
    }
    pub(crate) fn advance_inline_source<B: SourceBackend>(
        &mut self,
        scope: u64,
        c: &mut TxIndexInlineContinuationV1,
        backend: &mut B,
    ) -> Result<TxIndexInlineStopV1, SourceRuleError> {
        self.require_inline_owner(scope, c)?;
        if c.pending.is_some() {
            return Ok(TxIndexInlineStopV1::AwaitDatabase);
        }
        if c.pc == TxIndexInlinePcV1::Finished {
            return Ok(TxIndexInlineStopV1::Complete);
        }
        let result = self.advance_inline_source_inner(c, backend);
        match result {
            Ok(stop) => Ok(stop),
            Err(Fault::Source(error)) => Ok(c.complete(Err(Fault::Source(error)))),
            Err(Fault::Local(error)) => Err(error),
        }
    }
    fn advance_inline_source_inner<B: SourceBackend>(
        &mut self,
        c: &mut TxIndexInlineContinuationV1,
        b: &mut B,
    ) -> R<TxIndexInlineStopV1> {
        loop {
            match c.pc {
                TxIndexInlinePcV1::Begin => match c.method {
                    TxIndexInlineMethodV1::SaveMaxHeight => {
                        if matches!(c.height, V::Null | V::Undefined) {
                            return Ok(c.complete(Ok(V::Undefined)));
                        }
                        c.pc = TxIndexInlinePcV1::SaveHeightFactory;
                    }
                    TxIndexInlineMethodV1::Initialize => {
                        c.database = c.frame.database(b)?;
                        c.pc = TxIndexInlinePcV1::InitializeFind;
                    }
                    TxIndexInlineMethodV1::ProcessBlock => {
                        c.database = c.frame.database(b)?;
                        c.ids = match prop(&c.block, "tx")? {
                            V::Array(ids) => ids,
                            V::String(text) => text.chars().map(|ch| s(ch.to_string())).collect(),
                            _ => return Err(ty("blockData.tx is not iterable")),
                        };
                        // These are actual source input/capacity boundaries, not truncated rows.
                        if c.ids.len() > 1024 {
                            return Err(SourceRuleError::Capacity.into());
                        }
                        c.pc = TxIndexInlinePcV1::NextTransaction;
                    }
                },
                TxIndexInlinePcV1::NextTransaction => {
                    let Some(id) = c.ids.get(c.at).cloned() else {
                        let rows = std::mem::take(&mut c.rows);
                        return Ok(c.complete(Ok(V::Array(rows))));
                    };
                    let raw = c.frame.rpc(
                        b,
                        &self.class_client,
                        "getRawTransaction",
                        vec![id.clone(), V::Bool(false), prop(&c.block, "hash")?],
                    )?;
                    let decoded = match self.decode(raw, &mut c.frame, b) {
                        Ok(v) => v,
                        Err(Fault::Source(_)) => V::Undefined,
                        Err(e) => return Err(e),
                    };
                    if matches!(decoded, V::Undefined | V::Null)
                        || prop(&decoded, "marker")? != s("tl")
                    {
                        c.at += 1;
                        continue;
                    }
                    let row = self.transaction(
                        prop(&decoded, "payload")?,
                        id.clone(),
                        prop(&decoded, "marker")?,
                        &mut c.frame,
                        b,
                    )?;
                    c.rows.push(row.clone()); // Actual source pushes BEFORE awaiting the upsert.
                    let key = format!("tx-{}-{}", text(&c.height)?, text(&id)?);
                    match c.suspend(
                        "updateAsync",
                        vec![
                            obj(&[("_id", s(&key))]),
                            obj(&[("_id", s(key)), ("value", row)]),
                            obj(&[("upsert", V::Bool(true))]),
                        ],
                    ) {
                        Err(Fault::Source(_)) => {
                            c.pc = TxIndexInlinePcV1::SaveHeightFactory;
                        }
                        other => return other,
                    };
                }
                TxIndexInlinePcV1::SaveHeightFactory => {
                    if matches!(c.height, V::Null | V::Undefined) {
                        if c.method == TxIndexInlineMethodV1::ProcessBlock {
                            c.at += 1;
                            c.pc = TxIndexInlinePcV1::NextTransaction;
                            continue;
                        }
                        return Ok(c.complete(Ok(V::Undefined)));
                    }
                    c.height_database = c.frame.database(b)?;
                    c.pc = TxIndexInlinePcV1::MaxHeight;
                }
                TxIndexInlinePcV1::MaxHeight => {
                    return c.suspend(
                        "updateAsync",
                        vec![
                            obj(&[("_id", s("MaxHeight"))]),
                            obj(&[("$set", obj(&[("value", c.height.clone())]))]),
                            obj(&[("upsert", V::Bool(true))]),
                        ],
                    )
                }
                TxIndexInlinePcV1::IndexExists => {
                    return c.suspend(
                        "updateAsync",
                        vec![
                            obj(&[("_id", s("indexExists"))]),
                            obj(&[("_id", s("indexExists")), ("value", V::Bool(true))]),
                            obj(&[("upsert", V::Bool(true))]),
                        ],
                    )
                }
                TxIndexInlinePcV1::InitializeFind => {
                    let old = c.frame.db(
                        b,
                        &c.database,
                        "findOneAsync",
                        vec![obj(&[("_id", s("genesisBlock"))])],
                    )?;
                    if truth(&old) {
                        return Ok(c.complete(Ok(V::Undefined)));
                    }
                    c.pc = TxIndexInlinePcV1::InitializeInsert;
                }
                TxIndexInlinePcV1::InitializeInsert => {
                    return c.suspend(
                        "insertAsync",
                        vec![obj(&[
                            ("_id", s("genesisBlock")),
                            ("value", c.height.clone()),
                        ])],
                    )
                }
                TxIndexInlinePcV1::Finished => return Ok(TxIndexInlineStopV1::Complete),
            }
        }
    }
    // Only the containing real native DB await adapter calls this after actual
    // completion or an injected source IO failure. An ambiguous native return
    // is NOT a DependencyReturn and must keep this same frame suspended.
    pub(crate) fn resolve_inline_source(
        &self,
        scope: u64,
        c: &mut TxIndexInlineContinuationV1,
        returned: DependencyReturn,
    ) -> Result<(), SourceRuleError> {
        self.require_inline_owner(scope, c)?;
        let call = c.pending.as_ref().ok_or(SourceRuleError::InvalidInput)?;
        match &returned {
            DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => v.validate()?,
            DependencyReturn::Throw(_) | DependencyReturn::PendingThrow(_) => (),
        }
        c.frame.observe(call.clone(), returned.clone())?;
        c.pending = None;
        let source_error = match returned {
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => Some(e),
            _ => None,
        };
        // The only catch here is processBlockData's individual transaction upsert.
        if let Some(error) = source_error {
            if !(c.method == TxIndexInlineMethodV1::ProcessBlock
                && c.pc == TxIndexInlinePcV1::NextTransaction)
            {
                c.complete(Err(Fault::Source(error)));
                return Ok(());
            }
        }
        c.pc = match c.pc {
            TxIndexInlinePcV1::NextTransaction => TxIndexInlinePcV1::SaveHeightFactory,
            TxIndexInlinePcV1::MaxHeight => TxIndexInlinePcV1::IndexExists,
            TxIndexInlinePcV1::IndexExists if c.method == TxIndexInlineMethodV1::ProcessBlock => {
                c.at += 1;
                TxIndexInlinePcV1::NextTransaction
            }
            TxIndexInlinePcV1::IndexExists | TxIndexInlinePcV1::InitializeInsert => {
                c.complete(Ok(V::Undefined));
                return Ok(());
            }
            _ => return Err(SourceRuleError::InvalidInput),
        };
        Ok(())
    }
}
