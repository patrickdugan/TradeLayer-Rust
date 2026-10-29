// Metadata only; never contains Runtime, Ledger, Journal or financial snapshots.
const ORDERED_BOOTSTRAP_PROFILE: &str = "TL-MAIN-ORDERED-CALLABLE-BOOTSTRAP-13-1";
const ORDERED_BOOTSTRAP_KEY: &str = "orderedBootstrap";
const ORDERED_BOOTSTRAP_MAX_STEPS: usize = 128;
fn ob_keys(value: &V, allowed: &[&str]) -> Result<(), SourceRuleError> {
    let V::Object(fields) = value else {
        return Err(SourceRuleError::InvalidInput);
    };
    let mut seen = std::collections::BTreeSet::new();
    if fields
        .iter()
        .any(|(key, _)| !allowed.contains(&key.as_str()) || !seen.insert(key.as_str()))
    {
        return Err(SourceRuleError::InvalidInput);
    }
    Ok(())
}
#[derive(Clone)]
struct OrderedBootstrap {
    facts: Vec<V>,
    imported: bool,
    constructed: bool,
    import_status: String,
    index_status: String,
    import_error: Option<usize>,
    index_error: Option<usize>,
    index_handled: bool,
    handled_pending: bool,
    notified: Vec<String>,
    rejected: Vec<String>,
    errors: Vec<V>,
    jobs: Vec<V>,
    events: Vec<V>,
    callers: Vec<V>,
    steps: usize,
    caller_active: bool,
    last_rpc_error: Option<usize>,
}
fn ob_string(v: V) -> Result<String, SourceRuleError> {
    match v {
        V::String(value) => Ok(value),
        _ => Err(SourceRuleError::InvalidInput),
    }
}
fn ob_bool(v: V) -> Result<bool, SourceRuleError> {
    match v {
        V::Bool(value) => Ok(value),
        _ => Err(SourceRuleError::InvalidInput),
    }
}
fn ob_array(v: V) -> Result<Vec<V>, SourceRuleError> {
    match v {
        V::Array(value) => Ok(value),
        _ => Err(SourceRuleError::InvalidInput),
    }
}
fn ob_usize(v: V) -> Result<usize, SourceRuleError> {
    let value = ob_string(v)?
        .parse::<usize>()
        .map_err(|_| SourceRuleError::InvalidInput)?;
    if value > 1024 {
        return Err(SourceRuleError::Capacity);
    }
    Ok(value)
}
fn ob_optional(v: V) -> Result<Option<usize>, SourceRuleError> {
    if v == V::Null {
        Ok(None)
    } else {
        Ok(Some(ob_usize(v)?))
    }
}
fn ob_ordinal(value: usize) -> V {
    s(value.to_string())
}
fn ob_optional_value(value: Option<usize>) -> V {
    value.map_or(V::Null, ob_ordinal)
}
fn ob_error_value(value: &V) -> Result<(), SourceRuleError> {
    ob_keys(value, &["name", "message", "code"])?;
    let V::Object(fields) = value else {
        return Err(SourceRuleError::InvalidInput);
    };
    if fields
        .iter()
        .any(|(key, _)| !matches!(key.as_str(), "name" | "message" | "code"))
        || !matches!(value.get("name"), V::String(_))
        || !matches!(value.get("message"), V::String(_))
    {
        return Err(SourceRuleError::InvalidInput);
    }
    value.validate()?;
    ob_data(&value.get("code"))?;
    Ok(())
}
fn ob_data(value: &V) -> Result<(), SourceRuleError> {
    value.validate()?;
    match value {
        V::Promise | V::BigNumber(_) => Err(SourceRuleError::InvalidInput),
        V::Array(values) => {
            for value in values {
                ob_data(value)?;
            }
            Ok(())
        }
        V::Object(fields) => {
            for (_, value) in fields {
                ob_data(value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
impl OrderedBootstrap {
    fn new(facts: Vec<V>) -> Result<Self, SourceRuleError> {
        if facts.len() != 2 {
            return Err(SourceRuleError::InvalidInput);
        }
        for fact in &facts {
            ob_keys(fact, &["mode", "rejection"])?;
            let V::Object(fields) = fact else {
                return Err(SourceRuleError::InvalidInput);
            };
            if fields
                .iter()
                .any(|(k, _)| !matches!(k.as_str(), "mode" | "rejection"))
            {
                return Err(SourceRuleError::InvalidInput);
            }
            let mode = ob_string(fact.get("mode"))?;
            if !matches!(mode.as_str(), "resolve" | "reject" | "deferred") {
                return Err(SourceRuleError::InvalidInput);
            }
            if matches!(mode.as_str(), "reject" | "deferred") {
                ob_error_value(&fact.get("rejection"))?;
            } else if fact.get("rejection") != V::Undefined {
                return Err(SourceRuleError::InvalidInput);
            }
        }
        Ok(Self {
            facts,
            imported: false,
            constructed: false,
            import_status: "NotStarted".into(),
            index_status: "NotStarted".into(),
            import_error: None,
            index_error: None,
            index_handled: false,
            handled_pending: false,
            notified: vec![],
            rejected: vec![],
            errors: vec![],
            jobs: vec![],
            events: vec![],
            callers: vec![],
            steps: 0,
            caller_active: false,
            last_rpc_error: None,
        })
    }
    fn closed(&self) -> bool {
        self.imported
            && self.constructed
            && self.jobs.is_empty()
            && !self.caller_active
            && !self.handled_pending
            && matches!(self.import_status.as_str(), "Fulfilled" | "Rejected")
            && matches!(self.index_status.as_str(), "Fulfilled" | "Rejected")
            && self
                .rejected
                .iter()
                .all(|role| self.notified.contains(role) || (role == "index" && self.index_handled))
    }
    fn value(&self) -> Result<V, SourceRuleError> {
        let value = obj(&[
            ("profile", s(ORDERED_BOOTSTRAP_PROFILE)),
            ("facts", V::Array(self.facts.clone())),
            ("imported", V::Bool(self.imported)),
            ("constructed", V::Bool(self.constructed)),
            ("importStatus", s(&self.import_status)),
            ("indexStatus", s(&self.index_status)),
            ("importError", ob_optional_value(self.import_error)),
            ("indexError", ob_optional_value(self.index_error)),
            ("indexHandled", V::Bool(self.index_handled)),
            ("handledPending", V::Bool(self.handled_pending)),
            ("notified", V::Array(self.notified.iter().map(s).collect())),
            ("rejected", V::Array(self.rejected.iter().map(s).collect())),
            ("errors", V::Array(self.errors.clone())),
            ("jobs", V::Array(self.jobs.clone())),
            ("events", V::Array(self.events.clone())),
            ("callers", V::Array(self.callers.clone())),
            ("steps", ob_ordinal(self.steps)),
            ("callerActive", V::Bool(self.caller_active)),
            ("lastRpcError", ob_optional_value(self.last_rpc_error)),
            ("metadataQuiescent", V::Bool(self.closed())),
        ]);
        value.validate()?;
        if value.diagnostic_json()?.len() > 1024 * 1024 {
            return Err(SourceRuleError::Capacity);
        }
        Ok(value)
    }
    // Only decode metadata already owned by this Core, never external state.
    fn owned(value: V) -> Result<Self, SourceRuleError> {
        if value.get("profile") != s(ORDERED_BOOTSTRAP_PROFILE) {
            return Err(SourceRuleError::InvalidInput);
        }
        let facts = ob_array(value.get("facts"))?;
        Self::new(facts.clone())?;
        let state = Self {
            facts,
            imported: ob_bool(value.get("imported"))?,
            constructed: ob_bool(value.get("constructed"))?,
            import_status: ob_string(value.get("importStatus"))?,
            index_status: ob_string(value.get("indexStatus"))?,
            import_error: ob_optional(value.get("importError"))?,
            index_error: ob_optional(value.get("indexError"))?,
            index_handled: ob_bool(value.get("indexHandled"))?,
            handled_pending: ob_bool(value.get("handledPending"))?,
            notified: ob_array(value.get("notified"))?
                .into_iter()
                .map(ob_string)
                .collect::<Result<_, _>>()?,
            rejected: ob_array(value.get("rejected"))?
                .into_iter()
                .map(ob_string)
                .collect::<Result<_, _>>()?,
            errors: ob_array(value.get("errors"))?,
            jobs: ob_array(value.get("jobs"))?,
            events: ob_array(value.get("events"))?,
            callers: ob_array(value.get("callers"))?,
            steps: ob_usize(value.get("steps"))?,
            caller_active: ob_bool(value.get("callerActive"))?,
            last_rpc_error: ob_optional(value.get("lastRpcError"))?,
        };
        if state.value()? != value {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(state)
    }
    fn error(&mut self, value: V) -> Result<usize, SourceRuleError> {
        ob_error_value(&value)?;
        if self.errors.len() >= 64 {
            return Err(SourceRuleError::Capacity);
        }
        self.errors.push(value);
        Ok(self.errors.len())
    }
    fn error_value(&self, id: usize) -> Result<&V, SourceRuleError> {
        id.checked_sub(1)
            .and_then(|at| self.errors.get(at))
            .ok_or(SourceRuleError::InvalidInput)
    }
    fn event(&mut self, value: V) -> Result<(), SourceRuleError> {
        if self.events.len() >= 512 {
            return Err(SourceRuleError::Capacity);
        }
        value.validate()?;
        self.events.push(value);
        Ok(())
    }
    fn promise(role: &str) -> V {
        s(if role == "import" {
            "TxUtils.importIIFE#1"
        } else {
            "Main.txIndex#2"
        })
    }
}
