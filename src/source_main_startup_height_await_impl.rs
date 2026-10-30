// Actual Main height-read await segments for the explicitly typed startup
// continuation. No Invoke dispatch, legacy Frame/receipt change or Core clone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeMainStartupHeightKeyV1 {
    Track,
    MaxProcessed,
}
impl NativeMainStartupHeightKeyV1 {
    fn key(self) -> &'static str {
        match self {
            Self::Track => "TrackHeight",
            Self::MaxProcessed => "MaxProcessedHeight",
        }
    }
    fn method(self) -> &'static str {
        match self {
            Self::Track => "Main.loadTrackHeight",
            Self::MaxProcessed => "Main.loadMaxProcessedHeight",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeMainStartupHeightAwaitStopV1 {
    FactoryAwait,
    QueryAwait,
    Complete,
    SourceOwnerStopped(SourceRuleError),
    Consumed,
}
enum NativeMainStartupHeightReadPcV1 {
    Factory,
    Query,
    Complete,
    Stopped(SourceRuleError),
    Consumed,
}
/// This frame contains actual source locals/results only. The containing
/// typed startup controller keeps the SAME financial owner; this frame cannot
/// construct, clone, release, checkpoint or reconstruct that owner.
pub(crate) struct NativeMainStartupHeightReadV1 {
    key: NativeMainStartupHeightKeyV1,
    pc: NativeMainStartupHeightReadPcV1,
    factory: Option<R<V>>,
    query: Option<R<V>>,
    result: Option<R<V>>,
}
impl NativeMainStartupHeightReadV1 {
    pub(crate) fn stop(&self) -> NativeMainStartupHeightAwaitStopV1 {
        match &self.pc {
            NativeMainStartupHeightReadPcV1::Factory => {
                NativeMainStartupHeightAwaitStopV1::FactoryAwait
            }
            NativeMainStartupHeightReadPcV1::Query => {
                NativeMainStartupHeightAwaitStopV1::QueryAwait
            }
            NativeMainStartupHeightReadPcV1::Complete => {
                NativeMainStartupHeightAwaitStopV1::Complete
            }
            NativeMainStartupHeightReadPcV1::Stopped(error) => {
                NativeMainStartupHeightAwaitStopV1::SourceOwnerStopped(error.clone())
            }
            NativeMainStartupHeightReadPcV1::Consumed => {
                NativeMainStartupHeightAwaitStopV1::Consumed
            }
        }
    }
    pub(crate) fn take_result(&mut self) -> Result<R<V>, SourceRuleError> {
        if !matches!(self.pc, NativeMainStartupHeightReadPcV1::Complete) {
            return Err(SourceRuleError::InvalidInput);
        }
        let result = self.result.take().ok_or(SourceRuleError::InvalidInput)?;
        self.pc = NativeMainStartupHeightReadPcV1::Consumed;
        Ok(result)
    }
    fn stopped(&mut self, error: SourceRuleError) {
        // The actual prior factory/query result remains in its original slot.
        // A local resource stop is not a source Promise rejection.
        self.pc = NativeMainStartupHeightReadPcV1::Stopped(error);
    }
    fn finish(&mut self, result: R<V>) {
        self.result = Some(result);
        self.pc = NativeMainStartupHeightReadPcV1::Complete;
    }
    pub(crate) fn visit_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s("TL-MAIN-SOURCE-HEIGHT-AWAIT-13-1"))?;
        visitor("method", &s(self.key.method()))?;
        visitor("key", &s(self.key.key()))?;
        // This leaf is reserved for the named VM-business/host-IO startup
        // profile. Realm comes from that pinned adapter contract, never from
        // a returned value, observed scheduler trace or claimed caller field.
        visitor("realmProfile", &s("TL-MAIN-VM-BUSINESS-HOST-IO-13-1"))?;
        visitor(
            "awaitPromiseRealm",
            &s(match self.pc {
                NativeMainStartupHeightReadPcV1::Factory => "SourceBusiness",
                NativeMainStartupHeightReadPcV1::Query => "HostIo",
                _ => "SourceBusinessFunctionResult",
            }),
        )?;
        visitor(
            "site",
            &s(match self.pc {
                NativeMainStartupHeightReadPcV1::Factory => "await db.getDatabase('consensus')",
                NativeMainStartupHeightReadPcV1::Query => {
                    "await consensusDB.findOneAsync({_id:key})"
                }
                NativeMainStartupHeightReadPcV1::Complete => {
                    "return height/null or rethrow original error"
                }
                NativeMainStartupHeightReadPcV1::Stopped(_) => "retained local owner stop",
                NativeMainStartupHeightReadPcV1::Consumed => {
                    "actual function result delivered once"
                }
            }),
        )?;
        for (name, result) in [
            ("factory", self.factory.as_ref()),
            ("query", self.query.as_ref()),
            ("result", self.result.as_ref()),
        ] {
            visitor(&format!("{name}/present"), &V::Bool(result.is_some()))?;
            if let Some(result) = result {
                match result {
                    Ok(value) => {
                        native_main_row_visit_value(&format!("{name}/returned"), value, visitor)?
                    }
                    Err(Fault::Thrown(error)) => {
                        visitor(&format!("{name}/thrown/name"), &s(&error.name))?;
                        visitor(&format!("{name}/thrown/message"), &s(&error.message))?;
                    }
                    Err(Fault::Local(error)) => {
                        visitor(&format!("{name}/local"), &s(format!("{error:?}")))?
                    }
                }
            }
        }
        if let NativeMainStartupHeightReadPcV1::Stopped(error) = &self.pc {
            visitor("ownerStopped", &s(format!("{error:?}")))?;
        }
        Ok(())
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_startup_height_read_begin(
        &mut self,
        key: NativeMainStartupHeightKeyV1,
    ) -> Result<NativeMainStartupHeightReadV1, SourceRuleError> {
        if self.policy() != SourceContractRuntimePolicy::MainOrchestrationV12 {
            return Err(SourceRuleError::InvalidInput);
        }
        let factory = self.core.database("consensus");
        let stop = if self.core.effects_exhausted {
            Some(SourceRuleError::Capacity)
        } else if let Err(Fault::Local(error)) = &factory {
            Some(error.clone())
        } else {
            None
        };
        Ok(NativeMainStartupHeightReadV1 {
            key,
            pc: stop.map_or(
                NativeMainStartupHeightReadPcV1::Factory,
                NativeMainStartupHeightReadPcV1::Stopped,
            ),
            factory: Some(factory),
            query: None,
            result: None,
        })
    }
    /// Called only when this actual source await reaction is selected by the
    /// same startup Kernel. It never drains unrelated jobs or repeats IO.
    pub(crate) fn native_main_startup_height_read_resume(
        &mut self,
        frame: &mut NativeMainStartupHeightReadV1,
    ) -> NativeMainStartupHeightAwaitStopV1 {
        match frame.pc {
            NativeMainStartupHeightReadPcV1::Factory => {
                let Some(factory) = frame.factory.take() else {
                    frame.stopped(SourceRuleError::InvalidInput);
                    return frame.stop();
                };
                match factory {
                    Err(Fault::Local(error)) => {
                        frame.factory = Some(Err(Fault::Local(error.clone())));
                        frame.stopped(error);
                    }
                    Err(Fault::Thrown(error)) => frame.finish(Err(Fault::Thrown(error))),
                    Ok(database) => {
                        frame.factory = Some(Ok(database));
                        let query = self
                            .core
                            .find("consensus", obj(&[("_id", s(frame.key.key()))]));
                        let stop = if self.core.effects_exhausted {
                            Some(SourceRuleError::Capacity)
                        } else if let Err(Fault::Local(error)) = &query {
                            Some(error.clone())
                        } else {
                            None
                        };
                        frame.query = Some(query);
                        frame.pc = stop.map_or(
                            NativeMainStartupHeightReadPcV1::Query,
                            NativeMainStartupHeightReadPcV1::Stopped,
                        );
                    }
                }
            }
            NativeMainStartupHeightReadPcV1::Query => {
                let Some(query) = frame.query.take() else {
                    frame.stopped(SourceRuleError::InvalidInput);
                    return frame.stop();
                };
                match query {
                    Err(Fault::Local(error)) => {
                        frame.query = Some(Err(Fault::Local(error.clone())));
                        frame.stopped(error);
                    }
                    Err(Fault::Thrown(error)) => frame.finish(Err(Fault::Thrown(error))),
                    Ok(row) => {
                        let value = if truth(&row) {
                            row.get("value")
                        } else {
                            V::Null
                        };
                        frame.query = Some(Ok(row));
                        frame.finish(Ok(value));
                    }
                }
            }
            NativeMainStartupHeightReadPcV1::Complete
            | NativeMainStartupHeightReadPcV1::Stopped(_)
            | NativeMainStartupHeightReadPcV1::Consumed => (),
        }
        frame.stop()
    }
}
