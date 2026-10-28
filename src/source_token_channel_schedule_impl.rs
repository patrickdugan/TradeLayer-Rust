// Exact V9 Main/raw20. Calls inspect the Promise, while the actual registry
// method and its fallback persistence continue at their source await points.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceTokenThresholdRead {
    arguments: Vec<V>,
    replies: Vec<(DependencyCall, DependencyReturn)>,
    next: Option<DependencyCall>,
    after_read: bool,
    trace: usize,
}
// Execute the actual owner only with replies from already executed real DB
// operations. At the first not-yet-executed dependency, stop without committing
// its clone. No default registry, Boolean or storage success is supplied here.
struct SourceTokenThresholdPrefix<'a> {
    replies: &'a [(DependencyCall, DependencyReturn)],
    cursor: usize,
    next: Option<DependencyCall>,
}
impl SourceBackend for SourceTokenThresholdPrefix<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if let Some((expected, reply)) = self.replies.get(self.cursor) {
            if call != expected {
                return Err(SourceRuleError::InvalidInput);
            }
            self.cursor += 1;
            return Ok(reply.clone());
        }
        self.next = Some(call.clone());
        Err(SourceRuleError::Backend)
    }
}
impl ContractCore {
    fn source_token_threshold_enabled(&self) -> bool {
        self.spot_lifecycle_enabled()
            && self.source_spot_main_active
            && self.channel_token_validation
            && self.main_activation_profile == "source-registry"
    }
    fn source_token_threshold_probe(&mut self, job: &mut SourceTokenThresholdRead) -> R<bool> {
        let mut owner = self.main_activation.clone();
        let mut backend = SourceTokenThresholdPrefix {
            replies: &job.replies,
            cursor: 0,
            next: None,
        };
        let result = owner.execute_unverified(
            crate::activation::ActivationCall {
                method: "areActivationsAboveThreshold".into(),
                arguments: job.arguments.clone(),
            },
            &mut backend,
        );
        match result {
            Err(SourceRuleError::Backend)
                if backend.next.is_some() && backend.cursor == job.replies.len() =>
            {
                job.next = backend.next;
                Ok(false)
            }
            Err(error) => Err(error.into()),
            Ok(observation) => {
                if backend.cursor != job.replies.len() || backend.next.is_some() {
                    return Err(SourceRuleError::Backend.into());
                }
                self.main_activation = owner;
                match observation.outcome {
                    SourceOutcome::Returned(value) => {
                        if job.trace < self.effects.len() {
                            set(&mut self.effects[job.trace], "returned", value)?;
                        }
                    }
                    SourceOutcome::Thrown(error) => {
                        if job.trace < self.effects.len() {
                            set(
                                &mut self.effects[job.trace],
                                "thrown",
                                obj(&[("name", s(&error.name)), ("message", s(&error.message))]),
                            )?;
                        }
                        self.source_supply_rejection(error)?;
                    }
                }
                job.next = None;
                Ok(true)
            }
        }
    }
    fn source_token_threshold_step(&mut self, job: &mut SourceTokenThresholdRead) -> R<bool> {
        if job.replies.len() >= 8 {
            return Err(SourceRuleError::Capacity.into());
        }
        let call = job.next.take().ok_or(SourceRuleError::Backend)?;
        let reply = match call.target.as_str() {
            "DB.getDatabase" if call.arguments == [s("activations")] => {
                returned(self.database("activations"))?
            }
            "DB.findAsync" => {
                returned(self.db("activations", "findAsync", call.arguments.clone()))?
            }
            "DB.updateAsync" => {
                returned(self.db("activations", "updateAsync", call.arguments.clone()))?
            }
            _ => return Err(SourceRuleError::UnknownDependency.into()),
        };
        job.replies.push((call, reply));
        self.source_token_threshold_probe(job)
    }
    fn source_token_threshold_begin(&mut self, arguments: &[V]) -> R<V> {
        if self.source_token_threshold_reads.len() >= 2 {
            return Err(SourceRuleError::Capacity.into());
        }
        let trace = self
            .effects
            .len()
            .checked_sub(1)
            .ok_or(SourceRuleError::Backend)?;
        let factory = returned(self.database("activations"))?;
        let mut job = SourceTokenThresholdRead {
            arguments: arguments.to_vec(),
            replies: vec![(
                DependencyCall {
                    target: "DB.getDatabase".into(),
                    arguments: vec![s("activations")],
                },
                factory,
            )],
            next: None,
            after_read: false,
            trace,
        };
        if !self.source_token_threshold_probe(&mut job)? {
            self.source_token_threshold_reads.push(job);
        }
        Ok(V::Promise)
    }
    fn source_token_threshold_at_boundary(&mut self, after_read: bool) -> R<()> {
        let jobs = std::mem::take(&mut self.source_token_threshold_reads);
        let mut remaining = Vec::new();
        for mut job in jobs {
            if job.after_read != after_read {
                remaining.push(job);
                continue;
            }
            if !self.source_token_threshold_step(&mut job)? {
                // Each real factory/read/write suspends the original async
                // method again. Do not complete a fallback save eagerly.
                job.after_read = !after_read;
                remaining.push(job);
            }
        }
        if !self.source_token_threshold_reads.is_empty() {
            return Err(SourceRuleError::Backend.into());
        }
        self.source_token_threshold_reads = remaining;
        Ok(())
    }
    fn source_token_threshold_after_database(&mut self, name: &str) -> R<()> {
        if self.source_token_threshold_enabled() && matches!(name, "attestations" | "propertyList")
        {
            self.source_token_threshold_at_boundary(false)?;
        }
        Ok(())
    }
    fn source_token_threshold_after_read(&mut self, name: &str) -> R<()> {
        if self.source_token_threshold_enabled() && matches!(name, "attestations" | "propertyList")
        {
            self.source_token_threshold_at_boundary(true)?;
        }
        Ok(())
    }
    fn source_token_threshold_drain(&mut self) -> R<()> {
        let mut steps = 0;
        for mut job in std::mem::take(&mut self.source_token_threshold_reads) {
            while !self.source_token_threshold_step(&mut job)? {
                steps += 1;
                if steps > 16 {
                    return Err(SourceRuleError::Capacity.into());
                }
            }
        }
        Ok(())
    }
    fn source_token_threshold_pending(&self) -> V {
        V::Array(
            self.source_token_threshold_reads
                .iter()
                .map(|job| {
                    obj(&[
                        ("arguments", V::Array(job.arguments.clone())),
                        ("trace", n(job.trace as f64)),
                        ("afterRead", V::Bool(job.after_read)),
                        (
                            "next",
                            job.next.as_ref().map_or(V::Null, |call| {
                                obj(&[
                                    ("target", s(&call.target)),
                                    ("arguments", V::Array(call.arguments.clone())),
                                ])
                            }),
                        ),
                        (
                            "replies",
                            V::Array(
                                job.replies
                                    .iter()
                                    .map(|(call, reply)| {
                                        obj(&[
                                            ("target", s(&call.target)),
                                            ("arguments", V::Array(call.arguments.clone())),
                                            (
                                                "reply",
                                                match reply {
                                                    DependencyReturn::Immediate(v)
                                                    | DependencyReturn::Pending(v) => {
                                                        obj(&[("returned", v.clone())])
                                                    }
                                                    DependencyReturn::Throw(e)
                                                    | DependencyReturn::PendingThrow(e) => {
                                                        obj(&[(
                                                            "thrown",
                                                            obj(&[
                                                                ("name", s(&e.name)),
                                                                ("message", s(&e.message)),
                                                            ]),
                                                        )])
                                                    }
                                                },
                                            ),
                                        ])
                                    })
                                    .collect(),
                            ),
                        ),
                    ])
                })
                .collect(),
        )
    }
}
