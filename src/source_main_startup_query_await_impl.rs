// Raw request, actual executor sampling, then original callback recording.
// No Runtime in this frame, no public completion constructor or result input.
enum NativeMainStartupDbQueryPcV1 {
    Requested,
    Sampled,
    Callback,
    Consumed,
    Stopped(SourceRuleError),
}
pub(crate) struct NativeMainStartupDbQueryV1 {
    collection: &'static str,
    method: &'static str,
    arguments: Vec<V>,
    pc: NativeMainStartupDbQueryPcV1,
    request_failure: Option<Fault>,
    sampled_result: Option<R<V>>,
    callback_result: Option<R<V>>,
    observer_fault: Option<Fault>,
}
impl NativeMainStartupDbQueryV1 {
    pub(crate) fn stopped(&self) -> Option<&SourceRuleError> {
        match &self.pc {
            NativeMainStartupDbQueryPcV1::Stopped(error) => Some(error),
            _ => None,
        }
    }
    pub(crate) fn actual_result(&self) -> Option<&R<V>> {
        if matches!(self.pc, NativeMainStartupDbQueryPcV1::Callback) {
            self.callback_result.as_ref()
        } else {
            None
        }
    }
    pub(crate) fn request_rejected(&self) -> bool {
        self.request_failure.is_some()
    }
    pub(crate) fn sampled(&self) -> bool {
        self.sampled_result.is_some()
    }
    pub(crate) fn callback_delivered(&self) -> bool {
        matches!(
            self.pc,
            NativeMainStartupDbQueryPcV1::Callback | NativeMainStartupDbQueryPcV1::Consumed
        )
    }
    pub(crate) fn take_actual_result(&mut self) -> Result<R<V>, SourceRuleError> {
        if !matches!(self.pc, NativeMainStartupDbQueryPcV1::Callback) {
            return Err(SourceRuleError::InvalidInput);
        }
        let result = self
            .callback_result
            .take()
            .ok_or(SourceRuleError::InvalidInput)?;
        self.pc = NativeMainStartupDbQueryPcV1::Consumed;
        Ok(result)
    }
    pub(crate) fn visit_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor(
            "profile",
            &s("TL-MAIN-SOURCE-STRING-ID-QUERY-EXECUTOR-CALLBACK-13-1"),
        )?;
        visitor("collection", &s(self.collection))?;
        visitor("method", &s(self.method))?;
        native_main_row_visit_value("arguments", &V::Array(self.arguments.clone()), visitor)?;
        visitor(
            "phase",
            &s(match self.pc {
                NativeMainStartupDbQueryPcV1::Requested => "Requested",
                NativeMainStartupDbQueryPcV1::Sampled => "Sampled",
                NativeMainStartupDbQueryPcV1::Callback => "Callback",
                NativeMainStartupDbQueryPcV1::Consumed => "Consumed",
                NativeMainStartupDbQueryPcV1::Stopped(_) => "Stopped",
            }),
        )?;
        for (name, result) in [
            ("sampled", self.sampled_result.as_ref()),
            ("callback", self.callback_result.as_ref()),
        ] {
            visitor(&format!("{name}/present"), &V::Bool(result.is_some()))?;
            if let Some(result) = result {
                match result {
                    Ok(value) => {
                        native_main_row_visit_value(&format!("{name}/returned"), value, visitor)?
                    }
                    Err(error) => startup_query_visit_fault(name, error, visitor)?,
                }
            }
        }
        for (name, error) in [
            ("requestFailure", self.request_failure.as_ref()),
            ("observerFault", self.observer_fault.as_ref()),
        ] {
            visitor(&format!("{name}/present"), &V::Bool(error.is_some()))?;
            if let Some(error) = error {
                startup_query_visit_fault(name, error, visitor)?;
            }
        }
        if let NativeMainStartupDbQueryPcV1::Stopped(error) = &self.pc {
            visitor("ownerStop", &s(format!("{error:?}")))?;
        }
        Ok(())
    }
}
fn startup_query_clone_result(result: &R<V>) -> R<V> {
    match result {
        Ok(value) => Ok(value.clone()),
        Err(Fault::Thrown(error)) => Err(Fault::Thrown(error.clone())),
        Err(Fault::Local(error)) => Err(Fault::Local(error.clone())),
    }
}
fn startup_query_visit_fault(
    path: &str,
    error: &Fault,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    match error {
        Fault::Thrown(error) => {
            visitor(&format!("{path}/thrown/name"), &s(&error.name))?;
            visitor(&format!("{path}/thrown/message"), &s(&error.message))?;
        }
        Fault::Local(error) => visitor(&format!("{path}/local"), &s(format!("{error:?}")))?,
    }
    Ok(())
}
fn startup_query_observer_stop(
    query: &mut NativeMainStartupDbQueryV1,
    error: Fault,
) -> SourceRuleError {
    let stop = match &error {
        Fault::Local(error) => error.clone(),
        Fault::Thrown(_) => SourceRuleError::Backend,
    };
    query.observer_fault = Some(error);
    query.pc = NativeMainStartupDbQueryPcV1::Stopped(stop.clone());
    stop
}
impl SourceContractRuntime {
    pub(crate) fn native_main_startup_db_query_request(
        &mut self,
        collection: &'static str,
        method: &'static str,
        args: Vec<V>,
    ) -> Result<NativeMainStartupDbQueryV1, SourceRuleError> {
        if self.policy() != SourceContractRuntimePolicy::MainOrchestrationV12
            || !["consensus", "tallyMap"].contains(&collection)
            || method != "findOneAsync"
            || args.len() != 1
        {
            return Err(SourceRuleError::InvalidInput);
        }
        for value in &args {
            value.validate()?;
        }
        // The reached Main height/processed-read and Tally bootstrap call
        // sites request one literal _id. This new private seam does not admit
        // arbitrary NeDB find/ordering/query plans; old generic DB is exact.
        if !matches!(&args[0],V::Object(fields) if fields.len()==1 && fields[0].0=="_id" && matches!(&fields[0].1,V::String(_)))
        {
            return Err(SourceRuleError::InvalidInput);
        }
        // This is the request-time failure boundary. No rows are sampled or
        // table materialized here. The typed containing frame records this
        // actual raw call now, independently of its eventual result.
        let request_failure = self.core.check_failure(collection, method).err();
        let pc = match &request_failure {
            Some(Fault::Local(error)) => NativeMainStartupDbQueryPcV1::Stopped(error.clone()),
            _ => NativeMainStartupDbQueryPcV1::Requested,
        };
        Ok(NativeMainStartupDbQueryV1 {
            collection,
            method,
            arguments: args,
            pc,
            request_failure,
            sampled_result: None,
            callback_result: None,
            observer_fault: None,
        })
    }
    pub(crate) fn native_main_startup_db_query_sample_actual(
        &mut self,
        query: &mut NativeMainStartupDbQueryV1,
    ) -> Result<(), SourceRuleError> {
        if !matches!(query.pc, NativeMainStartupDbQueryPcV1::Requested) || query.request_rejected()
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let result = (|| {
            if self.core.activation_lifecycle_enabled() {
                self.core.table(query.collection)?;
            }
            self.core
                .source_db_read_rows(query.collection, query.method, &query.arguments)
        })();
        let stop = match &result {
            Err(Fault::Local(error)) => Some(error.clone()),
            _ => None,
        };
        query.sampled_result = Some(result);
        query.pc = stop
            .as_ref()
            .map_or(NativeMainStartupDbQueryPcV1::Sampled, |error| {
                NativeMainStartupDbQueryPcV1::Stopped(error.clone())
            });
        stop.map_or(Ok(()), Err)
    }
    pub(crate) fn native_main_startup_db_query_deliver_callback(
        &mut self,
        query: &mut NativeMainStartupDbQueryV1,
    ) -> Result<(), SourceRuleError> {
        let result = match &query.pc {
            NativeMainStartupDbQueryPcV1::Requested if query.request_rejected() => {
                match query
                    .request_failure
                    .as_ref()
                    .ok_or(SourceRuleError::InvalidInput)?
                {
                    Fault::Thrown(error) => Err(Fault::Thrown(error.clone())),
                    Fault::Local(error) => return Err(error.clone()),
                }
            }
            NativeMainStartupDbQueryPcV1::Sampled => startup_query_clone_result(
                query
                    .sampled_result
                    .as_ref()
                    .ok_or(SourceRuleError::InvalidInput)?,
            ),
            NativeMainStartupDbQueryPcV1::Stopped(error) => return Err(error.clone()),
            _ => return Err(SourceRuleError::InvalidInput),
        };
        let recorded = match self.core.source_db_record_result(
            query.collection,
            query.method,
            query.arguments.clone(),
            result,
        ) {
            Ok(result) => result,
            Err(error) => return Err(startup_query_observer_stop(query, error)),
        };
        // The real successful/failed native result is retained BEFORE any
        // post-call observer/deferred-child hook can fail. Such a failure is
        // not relabeled as a rejection of the unrelated original query.
        query.callback_result = Some(recorded);
        if self.core.effects_exhausted {
            return Err(startup_query_observer_stop(
                query,
                Fault::Local(SourceRuleError::Capacity),
            ));
        }
        if let Err(error) = self.core.source_db_after_recorded_result(
            query.collection,
            query.method,
            query
                .callback_result
                .as_ref()
                .expect("same stored actual callback"),
        ) {
            return Err(startup_query_observer_stop(query, error));
        }
        query.pc = NativeMainStartupDbQueryPcV1::Callback;
        Ok(())
    }
}
