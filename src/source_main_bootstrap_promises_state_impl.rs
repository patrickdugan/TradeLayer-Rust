const ORDERED_BOOTSTRAP_PROMISES_V2_PROFILE: &str = "TL-MAIN-ORDERED-CALLABLE-PROMISES-13-2";
#[derive(Clone)]
struct OrderedBootstrapHandlerV2 {
    label: String,
    scheduled: bool,
    done: bool,
    ran: bool,
}
#[derive(Clone)]
struct OrderedBootstrapPromisesV2 {
    // 0: not requested, 1: unresolved host client, 2: fulfilled, 3: rejected.
    clients: [usize; 2],
    registered: [bool; 2],
    reasons: [Option<usize>; 2],
    aliases: [Option<String>; 2],
    objects: Vec<(String, V)>,
    references: Vec<(String, usize)>,
    handlers: Vec<OrderedBootstrapHandlerV2>,
    epoch: usize,
}
fn ob_v2_pair(value: V) -> Result<[V; 2], SourceRuleError> {
    ob_array(value)?
        .try_into()
        .map_err(|_| SourceRuleError::InvalidInput)
}
fn ob_v2_role(role: &str) -> Result<usize, SourceRuleError> {
    match role {
        "import" => Ok(0),
        "index" => Ok(1),
        _ => Err(SourceRuleError::InvalidInput),
    }
}
fn ob_v2_optional_text(value: V) -> Result<Option<String>, SourceRuleError> {
    if value == V::Null {
        Ok(None)
    } else {
        Ok(Some(ob_string(value)?))
    }
}
impl OrderedBootstrapHandlerV2 {
    fn value(&self) -> V {
        obj(&[
            ("label", s(&self.label)),
            ("scheduled", V::Bool(self.scheduled)),
            ("done", V::Bool(self.done)),
            ("ran", V::Bool(self.ran)),
        ])
    }
    fn owned(value: V) -> Result<Self, SourceRuleError> {
        ob_keys(&value, &["label", "scheduled", "done", "ran"])?;
        let label = ob_string(value.get("label"))?;
        if label.is_empty() || label.len() > 128 {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(Self {
            label,
            scheduled: ob_bool(value.get("scheduled"))?,
            done: ob_bool(value.get("done"))?,
            ran: ob_bool(value.get("ran"))?,
        })
    }
}
impl OrderedBootstrapPromisesV2 {
    fn new(raw_facts: Vec<V>, raw_objects: V) -> Result<(Vec<V>, Self), SourceRuleError> {
        if raw_facts.len() != 2 {
            return Err(SourceRuleError::InvalidInput);
        }
        let mut objects = vec![];
        for object in match raw_objects {
            V::Undefined => vec![],
            v => ob_array(v)?,
        } {
            ob_keys(&object, &["id", "error"])?;
            let id = ob_string(object.get("id"))?;
            if id.is_empty()
                || id.len() > 128
                || objects.iter().any(|(prior, _)| prior == &id)
                || objects.len() >= 16
            {
                return Err(SourceRuleError::InvalidInput);
            }
            let error = object.get("error");
            ob_error_value(&error)?;
            objects.push((id, error));
        }
        let mut aliases: [Option<String>; 2] = [None, None];
        let mut facts = vec![];
        for (role, fact) in raw_facts.into_iter().enumerate() {
            ob_keys(&fact, &["mode", "rejection", "rejectionRef"])?;
            let mode = ob_string(fact.get("mode"))?;
            if !["resolve", "reject", "deferred"].contains(&mode.as_str()) {
                return Err(SourceRuleError::InvalidInput);
            }
            let error = if fact.get("rejectionRef") != V::Undefined {
                if mode == "resolve" || fact.get("rejection") != V::Undefined {
                    return Err(SourceRuleError::InvalidInput);
                }
                let id = ob_string(fact.get("rejectionRef"))?;
                let error = objects
                    .iter()
                    .find(|(key, _)| key == &id)
                    .ok_or(SourceRuleError::InvalidInput)?
                    .1
                    .clone();
                aliases[role] = Some(id);
                error
            } else {
                fact.get("rejection")
            };
            let mut normalized = vec![("mode", s(mode))];
            if error != V::Undefined {
                normalized.push(("rejection", error));
            }
            facts.push(obj(&normalized));
        }
        OrderedBootstrap::new(facts.clone())?;
        Ok((
            facts,
            Self {
                clients: [0, 0],
                registered: [false, false],
                reasons: [None, None],
                aliases,
                objects,
                references: vec![],
                handlers: vec![],
                epoch: 0,
            },
        ))
    }
    fn value(&self) -> Result<V, SourceRuleError> {
        let value = obj(&[
            ("profile", s(ORDERED_BOOTSTRAP_PROMISES_V2_PROFILE)),
            (
                "clients",
                V::Array(self.clients.iter().copied().map(ob_ordinal).collect()),
            ),
            (
                "registered",
                V::Array(self.registered.iter().copied().map(V::Bool).collect()),
            ),
            (
                "reasons",
                V::Array(
                    self.reasons
                        .iter()
                        .copied()
                        .map(ob_optional_value)
                        .collect(),
                ),
            ),
            (
                "aliases",
                V::Array(
                    self.aliases
                        .iter()
                        .map(|id| id.as_ref().map_or(V::Null, s))
                        .collect(),
                ),
            ),
            (
                "objects",
                V::Array(
                    self.objects
                        .iter()
                        .map(|(id, error)| obj(&[("id", s(id)), ("error", error.clone())]))
                        .collect(),
                ),
            ),
            (
                "references",
                V::Array(
                    self.references
                        .iter()
                        .map(|(id, reference)| {
                            obj(&[("id", s(id)), ("reference", ob_ordinal(*reference))])
                        })
                        .collect(),
                ),
            ),
            (
                "handlers",
                V::Array(
                    self.handlers
                        .iter()
                        .map(OrderedBootstrapHandlerV2::value)
                        .collect(),
                ),
            ),
            ("epoch", ob_ordinal(self.epoch)),
        ]);
        value.validate()?;
        Ok(value)
    }
    fn owned(value: V) -> Result<Self, SourceRuleError> {
        ob_keys(
            &value,
            &[
                "profile",
                "clients",
                "registered",
                "reasons",
                "aliases",
                "objects",
                "references",
                "handlers",
                "epoch",
            ],
        )?;
        if value.get("profile") != s(ORDERED_BOOTSTRAP_PROMISES_V2_PROFILE) {
            return Err(SourceRuleError::InvalidInput);
        }
        let [a, b] = ob_v2_pair(value.get("clients"))?;
        let clients = [ob_usize(a)?, ob_usize(b)?];
        if clients.iter().any(|n| *n > 3) {
            return Err(SourceRuleError::InvalidInput);
        }
        let [a, b] = ob_v2_pair(value.get("registered"))?;
        let registered = [ob_bool(a)?, ob_bool(b)?];
        let [a, b] = ob_v2_pair(value.get("reasons"))?;
        let reasons = [ob_optional(a)?, ob_optional(b)?];
        let [a, b] = ob_v2_pair(value.get("aliases"))?;
        let aliases = [ob_v2_optional_text(a)?, ob_v2_optional_text(b)?];
        let mut objects = vec![];
        for object in ob_array(value.get("objects"))? {
            ob_keys(&object, &["id", "error"])?;
            let id = ob_string(object.get("id"))?;
            let error = object.get("error");
            ob_error_value(&error)?;
            if id.is_empty()
                || id.len() > 128
                || objects.iter().any(|(prior, _)| prior == &id)
                || objects.len() >= 16
            {
                return Err(SourceRuleError::InvalidInput);
            }
            objects.push((id, error));
        }
        let mut references = vec![];
        for reference in ob_array(value.get("references"))? {
            ob_keys(&reference, &["id", "reference"])?;
            let id = ob_string(reference.get("id"))?;
            if !objects.iter().any(|(prior, _)| prior == &id)
                || references.iter().any(|(prior, _)| prior == &id)
            {
                return Err(SourceRuleError::InvalidInput);
            }
            references.push((id, ob_usize(reference.get("reference"))?));
        }
        let handlers = ob_array(value.get("handlers"))?
            .into_iter()
            .map(OrderedBootstrapHandlerV2::owned)
            .collect::<Result<Vec<_>, _>>()?;
        if handlers.len() > 16
            || handlers
                .iter()
                .enumerate()
                .any(|(i, h)| handlers[..i].iter().any(|old| old.label == h.label))
        {
            return Err(SourceRuleError::Capacity);
        }
        let result = Self {
            clients,
            registered,
            reasons,
            aliases,
            objects,
            references,
            handlers,
            epoch: ob_usize(value.get("epoch"))?,
        };
        if result.value() != Ok(value) {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(result)
    }
}
impl OrderedBootstrap {
    fn profile(&self) -> &'static str {
        if self.promise_v2.is_some() {
            ORDERED_BOOTSTRAP_PROMISES_V2_PROFILE
        } else {
            ORDERED_BOOTSTRAP_PROFILE
        }
    }
    fn promises(&self) -> Result<&OrderedBootstrapPromisesV2, SourceRuleError> {
        self.promise_v2
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)
    }
    fn promises_mut(&mut self) -> Result<&mut OrderedBootstrapPromisesV2, SourceRuleError> {
        self.promise_v2
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)
    }
    fn v2_rejection(&mut self, role: usize) -> Result<usize, SourceRuleError> {
        let alias = self.promises()?.aliases[role].clone();
        if let Some(alias) = &alias {
            if let Some((_, reference)) = self
                .promises()?
                .references
                .iter()
                .find(|(id, _)| id == alias)
            {
                return Ok(*reference);
            }
        }
        let reference = self.error(self.facts[role].get("rejection"))?;
        if let Some(alias) = alias {
            self.promises_mut()?.references.push((alias, reference));
        }
        Ok(reference)
    }
    fn v2_job(&mut self, job: V) -> Result<(), SourceRuleError> {
        if self.jobs.len() >= 64 {
            return Err(SourceRuleError::Capacity);
        }
        job.validate()?;
        self.jobs.push(job);
        Ok(())
    }
    fn v2_status(&mut self, role: usize, status: &str) {
        if role == 0 {
            self.import_status = status.into();
        } else {
            self.index_status = status.into();
        }
    }
}
