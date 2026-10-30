// No new dispatch case. Only the typed raw-foundation owner may call these.
pub(crate) struct NativeMainStartupIoStateV1 {
    inputs: crate::source_main_raw_startup::NativeStartupInputsV1,
    automatic_requested: bool,
    automatic_settled: bool,
    global_requested: bool,
    errors: Vec<crate::source_main_startup_prefix::StartupThrowV1>,
}
impl NativeMainStartupIoStateV1 {
    pub(crate) fn new(inputs: crate::source_main_raw_startup::NativeStartupInputsV1) -> Self {
        Self {
            inputs,
            automatic_requested: false,
            automatic_settled: false,
            global_requested: false,
            errors: vec![],
        }
    }
    fn error(
        &mut self,
        fact: crate::source_main_raw_startup::NativeStartupIoErrorV1,
    ) -> crate::source_main_startup_prefix::StartupThrowV1 {
        let error = crate::source_main_startup_prefix::StartupThrowV1 {
            identity: self.errors.len() as u64 + 1,
            exception: crate::validity_source::SourceException {
                name: fact.name,
                message: fact.message,
            },
            code: fact.code,
        };
        self.errors.push(error.clone());
        error
    }
    pub(crate) fn visit_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("io/automaticRequested", &V::Bool(self.automatic_requested))?;
        visitor("io/automaticSettled", &V::Bool(self.automatic_settled))?;
        visitor("io/globalRequested", &V::Bool(self.global_requested))?;
        visitor("io/errors/count", &s(self.errors.len().to_string()))?;
        for (i, error) in self.errors.iter().enumerate() {
            visitor(
                &format!("io/errors/{i}/identity"),
                &s(error.identity.to_string()),
            )?;
            visitor(&format!("io/errors/{i}/name"), &s(&error.exception.name))?;
            visitor(
                &format!("io/errors/{i}/message"),
                &s(&error.exception.message),
            )?;
            visitor(&format!("io/errors/{i}/code"), &error.code)?;
        }
        Ok(())
    }
}
struct NativeMainRawStartupLoanV1<'a> {
    core: &'a mut ContractCore,
    index: &'a mut crate::source_compact_index_v2::SourceCompactIndex,
    io: &'a mut NativeMainStartupIoStateV1,
}
impl crate::source_main_startup_prefix::sealed::Sealed for NativeMainRawStartupLoanV1<'_> {}
impl crate::source_main_startup_prefix::StartupOwnerV1 for NativeMainRawStartupLoanV1<'_> {
    fn main_client(&self) -> V {
        self.io.inputs.main_client.clone()
    }
    fn native_error(
        &mut self,
        name: &str,
        message: String,
    ) -> crate::source_main_startup_prefix::StartupThrowV1 {
        self.io
            .error(crate::source_main_raw_startup::NativeStartupIoErrorV1 {
                name: name.into(),
                message,
                code: V::Undefined,
            })
    }
    fn invoke(
        &mut self,
        site: &crate::source_main_startup_prefix::StartupSiteV1,
    ) -> Result<crate::source_main_startup_prefix::StartupCompletionV1, SourceRuleError> {
        use crate::source_main_startup_prefix::{
            StartupCompletionV1 as Completion, StartupSiteV1 as Site,
        };
        let result = match site {
            Site::AutomaticDatabaseClient => {
                if self.io.automatic_requested {
                    return Err(SourceRuleError::InvalidInput);
                }
                self.io.automatic_requested = true;
                self.core
                    .source_main_raw_startup_sdk_request(&self.io.inputs.sdk_client_fields)
                    .map_err(raw_startup_local)?;
                return Ok(Completion::Held(1));
            }
            Site::DatabaseRetryClient => self
                .core
                .source_main_raw_startup_sdk_request(&self.io.inputs.sdk_client_fields),
            Site::MainConstructor => {
                let p = self
                    .core
                    .main_production
                    .as_mut()
                    .ok_or(SourceRuleError::InvalidInput)?;
                p.genesis = n(600000.);
                p.parse_block = n(0.);
                self.core.source_main_raw_startup_sdk_index(
                    self.index,
                    &self.io.inputs.sdk_client_fields,
                )
            }
            Site::GlobalInitializeClient => {
                if self.io.global_requested {
                    return Err(SourceRuleError::InvalidInput);
                }
                self.io.global_requested = true;
                self.core
                    .source_main_raw_startup_sdk_request(&self.io.inputs.sdk_client_fields)
                    .map_err(raw_startup_local)?;
                if let Some(error) = self.io.inputs.global_client_error.clone() {
                    return Ok(Completion::Thrown(self.io.error(error)));
                }
                Ok(obj(&[("$rpcClient", n(1.))]))
            }
            Site::DatabaseChain => self.core.source_main_production_io("Client.getChain", &[]),
            Site::DatabaseTest => self.core.source_main_production_io("Client.getTest", &[]),
            Site::Datastore {
                category,
                directory,
            } => {
                let mut event = obj(&[
                    ("kind", s("datastore-open")),
                    ("category", s(*category)),
                    ("filename", s(format!("{directory}/{category}.db"))),
                    ("autoload", V::Bool(true)),
                ]);
                if let Some(failure) = self
                    .io
                    .inputs
                    .datastore_error
                    .as_ref()
                    .filter(|failure| failure.category == *category)
                {
                    let fact = failure.error.clone();
                    let error = self.io.error(fact);
                    set(
                        &mut event,
                        "thrown",
                        obj(&[
                            ("name", s(&error.exception.name)),
                            ("message", s(&error.exception.message)),
                            ("code", error.code.clone()),
                        ]),
                    )
                    .map_err(raw_startup_local)?;
                    self.core.emit(event);
                    return Ok(Completion::Thrown(error));
                }
                // A handle/slot only: txIndex's authoritative rows stay on the
                // actual native index; orderBooks stays on its existing Queue.
                if *category == "orderBooks" {
                    self.core.source_shared_orderbook_slot()?;
                } else {
                    self.core.table(category).map_err(raw_startup_local)?;
                }
                self.core.emit(event);
                Ok(obj(&[("$database", s(*category))]))
            }
        };
        match result {
            Ok(value) => Ok(Completion::Returned(value)),
            Err(Fault::Thrown(error)) => Ok(Completion::Thrown(self.io.error(
                crate::source_main_raw_startup::NativeStartupIoErrorV1 {
                    name: error.name,
                    message: error.message,
                    code: V::Undefined,
                },
            ))),
            Err(Fault::Local(error)) => Err(error),
        }
    }
    fn settle(
        &mut self,
        token: u64,
    ) -> Result<crate::source_main_startup_prefix::StartupCompletionV1, SourceRuleError> {
        use crate::source_main_startup_prefix::StartupCompletionV1 as Completion;
        if token != 1 || !self.io.automatic_requested || self.io.automatic_settled {
            return Err(SourceRuleError::InvalidInput);
        }
        self.io.automatic_settled = true;
        let value = if let Some(error) = self.io.inputs.automatic_client_error.clone() {
            Completion::Thrown(self.io.error(error))
        } else {
            Completion::Returned(obj(&[("$rpcClient", n(1.))]))
        };
        Ok(value)
    }
}
fn raw_startup_local(fault: Fault) -> SourceRuleError {
    match fault {
        Fault::Local(error) => error,
        Fault::Thrown(_) => SourceRuleError::Backend,
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_raw_startup_preflight(
        &self,
        inputs: &crate::source_main_raw_startup::NativeStartupInputsV1,
    ) -> Result<(), SourceRuleError> {
        if self.policy() != SourceContractRuntimePolicy::MainOrchestrationV12
            || self.core.main_production.is_some()
            || self.core.native_main_binding.is_some()
            || self.core.main_activation_profile != "source-registry"
            || !self.collection("txIndex").is_empty()
        {
            return Err(SourceRuleError::UnknownDependency);
        }
        inputs.main_client.validate()?;
        inputs.sdk_client_fields.validate()?;
        let V::Object(fields) = &inputs.sdk_client_fields else {
            return Err(SourceRuleError::InvalidInput);
        };
        if fields
            .iter()
            .any(|(key, _)| !["chain", "test"].contains(&key.as_str()))
            || (matches!(&inputs.main_client, V::Object(_))
                && inputs.main_client != inputs.sdk_client_fields)
        {
            return Err(SourceRuleError::InvalidInput);
        }

        if let V::Object(fields) = &inputs.main_client {
            if fields
                .iter()
                .any(|(key, _)| !["chain", "test"].contains(&key.as_str()))
            {
                return Err(SourceRuleError::InvalidInput);
            }
        } else if !matches!(inputs.main_client, V::Null | V::Undefined) {
            return Err(SourceRuleError::InvalidInput);
        }
        for error in [
            inputs.automatic_client_error.as_ref(),
            inputs.global_client_error.as_ref(),
            inputs.datastore_error.as_ref().map(|f| &f.error),
        ]
        .into_iter()
        .flatten()
        {
            if error.name.len() > 1024 || error.message.len() > 4096 {
                return Err(SourceRuleError::Capacity);
            }
            error.code.validate()?;
        }
        if inputs.datastore_error.as_ref().is_some_and(|f| {
            !crate::source_main_startup_prefix::CATEGORIES.contains(&f.category.as_str())
        }) {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(())
    }
    pub(crate) fn native_main_raw_startup_begin(
        &mut self,
        index: &mut SourceCompactIndex,
        io: &mut NativeMainStartupIoStateV1,
        generation: u64,
    ) -> Result<crate::source_main_startup_prefix::MainStartupPrefixV1, SourceRuleError> {
        let client = obj(&[
            ("chain", s(&io.inputs.chain)),
            ("test", V::Bool(io.inputs.test)),
        ]);
        self.core
            .source_main_production_configure_mode(
                &[obj(&[
                    ("client", client),
                    ("io", obj(&[])),
                    ("genesisBlock", n(600000.)),
                ])],
                false,
            )
            .map_err(raw_startup_local)?;
        self.core
            .source_main_service_configure(&[obj(&[])])
            .map_err(raw_startup_local)?;
        // The source callable harness supplies already-imported original
        // TxUtils. Execute that genuine prelude on THIS same index owner,
        // before DB import and the later constructor's static init(true).
        self.core
            .source_main_raw_startup_sdk_import(index, &io.inputs.sdk_client_fields)?;
        crate::source_main_startup_prefix::MainStartupPrefixV1::import_database(
            &mut NativeMainRawStartupLoanV1 {
                core: &mut self.core,
                index,
                io,
            },
            generation,
        )
    }
    pub(crate) fn native_main_raw_startup_apply(
        &mut self,
        index: &mut SourceCompactIndex,
        io: &mut NativeMainStartupIoStateV1,
        prefix: &mut crate::source_main_startup_prefix::MainStartupPrefixV1,
        command: crate::source_main_startup_prefix::StartupCommandV1,
    ) -> Result<
        crate::source_main_startup_prefix::StartupStopV1,
        crate::source_main_startup_prefix::StartupErrorV1,
    > {
        let before = prefix.events().len();
        let result = prefix.apply(
            &mut NativeMainRawStartupLoanV1 {
                core: &mut self.core,
                index,
                io,
            },
            command,
        );
        for event in &prefix.events()[before..] {
            self.core.emit(event.clone());
        }
        result
    }
}
