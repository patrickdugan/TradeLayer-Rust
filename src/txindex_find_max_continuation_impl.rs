// Literal findMaxIndexedBlock await boundaries for the explicit null-sync
// source profile. A frame holds source locals/observations, never index data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TxIndexFindMaxPcV1 {
    FactoryAwait,
    ReadAwait,
    Finished,
}
pub(crate) struct TxIndexFindMaxReadV1 {
    scope: u64,
    pc: TxIndexFindMaxPcV1,
    frame: Frame,
    database: Option<R<V>>,
    read: Option<R<V>>,
    result: Option<R<V>>,
}
impl TxIndexFindMaxReadV1 {
    pub(crate) fn visit_fields(&self, visitor: &mut dyn FnMut(&str, &V)) {
        visitor("profile", &s("TL-TXINDEX-FIND-MAX-AWAIT-13-1"));
        visitor("scope", &s(self.scope.to_string()));
        visitor("pc", &s(format!("{:?}", self.pc)));
        for (name, result) in [
            ("factory", &self.database),
            ("read", &self.read),
            ("result", &self.result),
        ] {
            visitor(&format!("{name}/present"), &V::Bool(result.is_some()));
            if let Some(result) = result {
                match result {
                    Ok(value) => visitor(&format!("{name}/returned"), value),
                    Err(Fault::Source(e)) => {
                        visitor(&format!("{name}/thrown/name"), &s(&e.name));
                        visitor(&format!("{name}/thrown/message"), &s(&e.message));
                    }
                    Err(Fault::Local(e)) => visitor(&format!("{name}/local"), &s(format!("{e:?}"))),
                }
            }
        }
        visitor(
            "observations/length",
            &s(self.frame.observations.len().to_string()),
        );
        for (at, observation) in self.frame.observations.iter().enumerate() {
            visitor(
                &format!("observations/{at}/target"),
                &s(&observation.call.target),
            );
            visitor(
                &format!("observations/{at}/arguments"),
                &V::Array(observation.call.arguments.clone()),
            );
            inline_visit_return(
                &format!("observations/{at}/returned"),
                &observation.returned,
                visitor,
            );
        }
    }
    pub(crate) fn take_evaluation(&mut self) -> Result<SourceEvaluation, SourceRuleError> {
        if self.pc != TxIndexFindMaxPcV1::Finished {
            return Err(SourceRuleError::InvalidInput);
        }
        TxIndexSourceOwner::evaluation(
            self.result.take().ok_or(SourceRuleError::InvalidInput)?,
            vec![],
            std::mem::take(&mut self.frame),
        )
    }
}
impl TxIndexSourceOwner {
    pub(crate) fn begin_find_max_read<B: SourceBackend>(
        &self,
        scope: u64,
        backend: &mut B,
    ) -> Result<TxIndexFindMaxReadV1, SourceRuleError> {
        if scope == 0 {
            return Err(SourceRuleError::InvalidInput);
        }
        let mut frame = Frame::default();
        let database = frame.database(backend);
        if let Err(Fault::Local(error)) = &database {
            return Err(error.clone());
        }
        Ok(TxIndexFindMaxReadV1 {
            scope,
            pc: TxIndexFindMaxPcV1::FactoryAwait,
            frame,
            database: Some(database),
            read: None,
            result: None,
        })
    }
    // The factory's source Promise settles before this real read is entered.
    // The caller may retain the frame here while its sibling parent proceeds.
    pub(crate) fn enter_find_max_read<B: SourceBackend>(
        &self,
        scope: u64,
        c: &mut TxIndexFindMaxReadV1,
        backend: &mut B,
    ) -> Result<(), SourceRuleError> {
        if c.scope != scope || c.pc != TxIndexFindMaxPcV1::FactoryAwait {
            return Err(SourceRuleError::InvalidInput);
        }
        let database = c.database.take().ok_or(SourceRuleError::InvalidInput)?;
        let result = database.and_then(|h| {
            c.frame.db(
                backend,
                &h,
                "findOneAsync",
                vec![obj(&[("_id", s("MaxHeight"))])],
            )
        });
        if let Err(Fault::Local(error)) = &result {
            return Err(error.clone());
        }
        c.read = Some(result);
        c.pc = TxIndexFindMaxPcV1::ReadAwait;
        Ok(())
    }
    pub(crate) fn settle_find_max_read(
        &self,
        scope: u64,
        c: &mut TxIndexFindMaxReadV1,
    ) -> Result<(), SourceRuleError> {
        if c.scope != scope || c.pc != TxIndexFindMaxPcV1::ReadAwait {
            return Err(SourceRuleError::InvalidInput);
        }
        let result = c.read.take().ok_or(SourceRuleError::InvalidInput)?;
        c.result = Some(result.map(|doc| {
            if truth(&doc) {
                doc.get("value")
            } else {
                V::Null
            }
        }));
        c.pc = TxIndexFindMaxPcV1::Finished;
        Ok(())
    }
    // New-profile only: execute the initializer's real factory/read one await
    // at a time, then hand its SAME existing continuation to the native writer.
    pub(crate) fn null_initializer_factory<B: SourceBackend>(
        &self,
        scope: u64,
        c: &mut TxIndexInlineContinuationV1,
        backend: &mut B,
    ) -> Result<(), SourceRuleError> {
        self.require_inline_owner(scope, c)?;
        if c.method != TxIndexInlineMethodV1::Initialize || c.pc != TxIndexInlinePcV1::Begin {
            return Err(SourceRuleError::InvalidInput);
        }
        match c.frame.database(backend) {
            Ok(handle) => {
                c.database = handle;
                c.pc = TxIndexInlinePcV1::InitializeFind;
            }
            Err(Fault::Source(error)) => {
                c.complete(Err(Fault::Source(error)));
            }
            Err(Fault::Local(error)) => return Err(error),
        }
        Ok(())
    }
    pub(crate) fn null_initializer_read<B: SourceBackend>(
        &self,
        scope: u64,
        c: &mut TxIndexInlineContinuationV1,
        backend: &mut B,
    ) -> Result<bool, SourceRuleError> {
        self.require_inline_owner(scope, c)?;
        if c.pc == TxIndexInlinePcV1::Finished {
            return Ok(false);
        }
        if c.method != TxIndexInlineMethodV1::Initialize
            || c.pc != TxIndexInlinePcV1::InitializeFind
        {
            return Err(SourceRuleError::InvalidInput);
        }
        match c.frame.db(
            backend,
            &c.database,
            "findOneAsync",
            vec![obj(&[("_id", s("genesisBlock"))])],
        ) {
            Ok(doc) => {
                c.block = doc;
                c.pc = TxIndexInlinePcV1::InitializeInsert;
                Ok(true)
            }
            Err(Fault::Source(error)) => {
                c.complete(Err(Fault::Source(error)));
                Ok(false)
            }
            Err(Fault::Local(error)) => Err(error),
        }
    }
    pub(crate) fn null_initializer_settle_read(
        &self,
        scope: u64,
        c: &mut TxIndexInlineContinuationV1,
    ) -> Result<(), SourceRuleError> {
        self.require_inline_owner(scope, c)?;
        if c.pc == TxIndexInlinePcV1::Finished {
            return Ok(());
        }
        if c.method != TxIndexInlineMethodV1::Initialize
            || c.pc != TxIndexInlinePcV1::InitializeInsert
            || c.pending.is_some()
        {
            return Err(SourceRuleError::InvalidInput);
        }
        if truth(&c.block) {
            c.complete(Ok(V::Undefined));
        }
        c.block = V::Undefined;
        Ok(())
    }
}
