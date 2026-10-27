//! Literal pinned contract execution state. This owner is an alternative to the
//! conserving EconomicState, not a shadow spendable balance store beside it.
//! Its single Tally collection/cache retains source Number and failure behavior.
use crate::contract_queue_source_owner::{ContractQueueSourceOwner, QueueDbFailure};
use crate::margin_map_source_owner::{MarginMapSourceOwner, PositionAlias, PositionObjectAlias};
use crate::source_contract_values::*;
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceOutcome, SourceRuleError,
    SourceValue as V,
};

pub const MAX_TABLE_ROWS: usize = 1024;
pub const MAX_TABLES: usize = 32;
/// Replay binds the admitted method graph as well as the original commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceContractRuntimePolicy {
    OnchainV1,
    GlobalClearingV2,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageFailure {
    pub collection: String,
    pub operation: String,
    pub skip: usize,
    pub message: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Table {
    name: String,
    rows: Vec<V>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceContractRuntime {
    pub(crate) queue: ContractQueueSourceOwner,
    pub(crate) core: ContractCore,
    last_parameters: Option<V>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContractCore {
    policy: SourceContractRuntimePolicy,
    queue_context: Option<ContractQueueSourceOwner>,
    clearing_contexts: Vec<(String, Vec<usize>)>,
    deleverage_trades: Vec<(String, Vec<V>)>,
    liquidation_marks: Vec<(String, V)>,
    contract_series: Vec<(V, V)>,
    tally_mod: V,
    tally_static_initialized: bool,
    channels: crate::channels_source::SourceChannels,
    property_cache: Vec<(V, V)>,
    vault_initialized: bool,
    source_unhandled_rejections: Vec<V>,
    source_supply_delta: Option<SourceSupplyDelta>,
    source_history_jobs: Vec<SourceHistoryJob>,
    tables: Vec<Table>,
    margins: MarginMapSourceOwner,
    tally: Vec<(V, V)>,
    fee_buffer: Vec<(V, Vec<(String, V)>)>,
    block_trades: Vec<(String, V)>,
    oracle_marks: Vec<(V, V)>,
    native_marks: Vec<(V, V)>,
    globals: V,
    effects: Vec<V>,
    effects_exhausted: bool,
    failures: Vec<StorageFailure>,
    uuid: u64,
    db_id: u64,
    random: u32,
    height: V,
    time_millis: u64,
    contract_mod: bool,
    contract_mod_raw: V,
    entry: String,
    active: bool,
    insurance: V,
    oracle_source_cache: Vec<(V, V)>,
    oracle_source_watermarks: Vec<(V, V)>,
    oracle_source_initialized: bool,
}
impl Default for SourceContractRuntime {
    fn default() -> Self {
        Self::new()
    }
}
impl SourceContractRuntime {
    pub fn new() -> Self {
        Self::with_policy(SourceContractRuntimePolicy::GlobalClearingV2)
    }
    pub fn with_policy(policy: SourceContractRuntimePolicy) -> Self {
        Self {
            queue: ContractQueueSourceOwner::new(),
            core: ContractCore::new(policy),
            last_parameters: None,
        }
    }
    pub fn policy(&self) -> SourceContractRuntimePolicy {
        self.core.policy
    }
    pub fn last_contract_parameters(&self) -> Option<&V> {
        self.last_parameters.as_ref()
    }
    /// Import complete persisted source collections, not supplied rule outcomes.
    pub fn import_collection(&mut self, name: &str, rows: Vec<V>) -> Result<(), SourceRuleError> {
        if name == "orderBooks" {
            self.queue.import_rows(rows)
        } else {
            self.core.import(name, rows)
        }
    }
    pub fn collection(&self, name: &str) -> &[V] {
        if name == "orderBooks" {
            self.queue.rows()
        } else {
            self.core.rows(name)
        }
    }
    pub fn set_height(&mut self, height: V) -> Result<(), SourceRuleError> {
        height.validate()?;
        self.core.height = height;
        Ok(())
    }
    pub fn set_activation_answer(&mut self, active: bool) {
        self.core.active = active;
    }
    pub fn set_entry(&mut self, entry: &str) -> Result<(), SourceRuleError> {
        if !matches!(
            entry,
            "main-subgraph" | "types-first" | "tally-first" | "volume-first"
        ) {
            return Err(SourceRuleError::InvalidInput);
        };
        self.core.entry = entry.into();
        Ok(())
    }
    pub fn fail_storage(&mut self, failure: StorageFailure) -> Result<(), SourceRuleError> {
        if failure.collection.len() > 128
            || failure.message.len() > 4096
            || failure.skip > 65_536
            || self.core.failures.len() >= MAX_TABLE_ROWS
        {
            return Err(SourceRuleError::Capacity);
        }
        if failure.operation == "removeAsync" && !self.core.global_clearing_enabled() {
            return Err(SourceRuleError::InvalidInput);
        }
        if !matches!(
            failure.operation.as_str(),
            "getDatabase"
                | "findOneAsync"
                | "findAsync"
                | "removeAsync"
                | "updateAsync"
                | "insertAsync"
                | "loadDatabase"
                | "get"
                | "put"
        ) {
            return Err(SourceRuleError::InvalidInput);
        }
        if failure.collection == "orderBooks" {
            self.queue.fail_database_call(QueueDbFailure {
                operation: failure.operation,
                skip: failure.skip,
                exception: crate::validity_source::SourceException {
                    name: "Error".into(),
                    message: failure.message,
                },
            })
        } else {
            self.core.failures.push(failure);
            Ok(())
        }
    }
    pub fn queue(&self) -> &ContractQueueSourceOwner {
        &self.queue
    }
    pub fn margins(&self) -> &MarginMapSourceOwner {
        &self.core.margins
    }
    pub fn tally_cache(&self) -> V {
        V::Array(
            self.core
                .tally
                .iter()
                .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                .collect(),
        )
    }
    pub fn take_effects(&mut self) -> Vec<V> {
        std::mem::take(&mut self.core.effects)
    }
    pub fn state_value(&self) -> Result<V, SourceRuleError> {
        let mut value = obj(&[
            (
                "tables",
                V::Array(
                    self.core
                        .tables
                        .iter()
                        .map(|t| obj(&[("name", s(&t.name)), ("rows", V::Array(t.rows.clone()))]))
                        .collect(),
                ),
            ),
            ("queue", self.queue.state_value()),
            ("margins", self.core.margins.state_value()?),
            ("tally", self.tally_cache()),
            (
                "feeBuffer",
                V::Array(
                    self.core
                        .fee_buffer
                        .iter()
                        .map(|(b, entries)| {
                            V::Array(vec![
                                b.clone(),
                                V::Array(
                                    entries
                                        .iter()
                                        .map(|(k, v)| V::Array(vec![s(k), v.clone()]))
                                        .collect(),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "blockTrades",
                V::Array(
                    self.core
                        .block_trades
                        .iter()
                        .map(|(k, v)| V::Array(vec![s(k), v.clone()]))
                        .collect(),
                ),
            ),
            (
                "oracleMarks",
                V::Array(
                    self.core
                        .oracle_marks
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                ),
            ),
            (
                "nativeMarks",
                V::Array(
                    self.core
                        .native_marks
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                ),
            ),
            ("globals", self.core.globals.clone()),
            ("insurance", self.core.insurance.clone()),
            (
                "oracleSourceCache",
                V::Array(
                    self.core
                        .oracle_source_cache
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                ),
            ),
            (
                "oracleSourceWatermarks",
                V::Array(
                    self.core
                        .oracle_source_watermarks
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                ),
            ),
            (
                "oracleSourceInitialized",
                V::Bool(self.core.oracle_source_initialized),
            ),
            ("sourceOracle", self.core.oracle_source_state()),
            (
                "continuation",
                obj(&[
                    ("uuid", n(self.core.uuid as f64)),
                    ("databaseId", n(self.core.db_id as f64)),
                    ("random", n(f64::from(self.core.random))),
                    ("height", self.core.height.clone()),
                    ("clock", n(self.core.time_millis as f64)),
                    ("contractMod", V::Bool(self.core.contract_mod)),
                    ("entry", s(&self.core.entry)),
                    ("active", V::Bool(self.core.active)),
                    (
                        "lastParameters",
                        self.last_parameters.clone().unwrap_or(V::Undefined),
                    ),
                    (
                        "failures",
                        V::Array(
                            self.core
                                .failures
                                .iter()
                                .map(|f| {
                                    obj(&[
                                        ("collection", s(&f.collection)),
                                        ("operation", s(&f.operation)),
                                        ("skip", n(f.skip as f64)),
                                        ("message", s(&f.message)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    (
                        "queueCheckpoint",
                        s(self
                            .queue
                            .checkpoint()?
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<String>()),
                    ),
                ]),
            ),
        ]);
        if self.core.global_clearing_enabled() {
            set(&mut value, "runtimePolicy", s("GlobalClearingV2"))
                .map_err(|_| SourceRuleError::InvalidInput)?;
            set(
                &mut value,
                "globalClearing",
                self.core.source_clearing_state(),
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        Ok(value)
    }
    pub fn invoke_dependency(
        &mut self,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        for v in &call.arguments {
            v.validate()?
        }
        let before = self.clone();
        self.core.queue_context = Some(std::mem::replace(
            &mut self.queue,
            ContractQueueSourceOwner::new(),
        ));
        let result = self.core.invoke(call);
        let result = if result.is_ok() && self.core.global_clearing_enabled() {
            self.core
                .source_history_drain()
                .map_err(|e| match e {
                    Fault::Local(e) => e,
                    Fault::Thrown(_) => SourceRuleError::Backend,
                })
                .and(result)
        } else {
            result
        };
        self.queue = self
            .core
            .queue_context
            .take()
            .ok_or(SourceRuleError::Backend)?;
        let result = if self.core.effects_exhausted {
            Err(SourceRuleError::Capacity)
        } else {
            result
        };
        if result.is_err() {
            *self = before
        };
        result
    }
    pub fn evaluate_queue(&mut self, call: &SourceCall) -> Result<SourceOutcome, SourceRuleError> {
        let before = self.clone();
        let core = &mut self.core;
        let result = self
            .queue
            .evaluate_with_capability(call, &mut |queue, call| {
                core.harvest_queue(queue);
                if call.target == "Orderbook.addContractOrder" {
                    let mut backend = PlacementBackend { core };
                    match crate::contract_placement_source::add_contract_order(
                        queue,
                        &call.arguments,
                        &mut backend,
                    )? {
                        SourceOutcome::Returned(v) => Ok(DependencyReturn::Pending(v)),
                        SourceOutcome::Thrown(e) => Ok(DependencyReturn::PendingThrow(e)),
                    }
                } else {
                    Err(SourceRuleError::UnknownDependency)
                }
            });
        self.core.harvest_queue(&mut self.queue);
        let result = if result.is_ok() && self.core.global_clearing_enabled() {
            self.core
                .source_history_drain()
                .map_err(|e| match e {
                    Fault::Local(e) => e,
                    Fault::Thrown(_) => SourceRuleError::Backend,
                })
                .and(result)
        } else {
            result
        };
        let result = if self.core.effects_exhausted {
            Err(SourceRuleError::Capacity)
        } else {
            result
        };
        if result.is_err() {
            *self = before
        };
        result
    }
    /// Actual Types/Validity and literal Logic type18 enqueue. Actor claims and
    /// imported source state are explicit unverified inputs, as in the oracle.
    pub fn execute_contract_unverified(
        &mut self,
        arguments: &[V],
    ) -> Result<SourceOutcome, SourceRuleError> {
        for v in arguments {
            v.validate()?
        }
        let before = self.clone();
        let result = self.execute_contract_inner(arguments);
        let result = if self.core.effects_exhausted {
            Err(SourceRuleError::Capacity)
        } else {
            result
        };
        if result.is_err() {
            *self = before
        };
        result
    }
    fn execute_contract_inner(
        &mut self,
        arguments: &[V],
    ) -> Result<SourceOutcome, SourceRuleError> {
        self.last_parameters = None;
        if arguments.get(1) != Some(&n(18.0)) {
            return Err(SourceRuleError::InvalidInput);
        };
        let result = crate::types_source::decode_payload_unverified(arguments, &mut self.core);
        let result = result.map_err(|_| SourceRuleError::Backend)?;
        if let Some(validity) = &result.validity {
            for (key, value) in &validity.settlement.evaluation.global_writes {
                self.core.globals.insert(key, value.clone())?
            }
        }
        match result.outcome {
            crate::types_source::TypesOutcome::Returned(params) => {
                self.last_parameters = Some(params.clone());
                let logic = if params.get("valid") == V::Bool(true) {
                    let queued = obj(&[
                        ("contractId", params.get("contractId")),
                        ("price", params.get("price")),
                        ("amount", params.get("amount")),
                        ("sell", params.get("sell")),
                        ("insurance", params.get("insurance")),
                        ("blockTime", params.get("block")),
                        ("isLiq", params.get("reduce")),
                        ("reduce", params.get("post")),
                        ("post", params.get("stop")),
                        ("stop", params.get("block")),
                    ]);
                    self.evaluate_queue(&SourceCall {
                        method: "queueOnChainContractOrder".into(),
                        arguments: vec![
                            s(text(&params.get("contractId"))
                                .map_err(|_| SourceRuleError::InvalidInput)?),
                            params.get("senderAddress"),
                            queued,
                            params.get("block"),
                            params.get("txid"),
                        ],
                    })?
                } else {
                    SourceOutcome::Returned(V::Undefined)
                };
                match logic {
                    SourceOutcome::Returned(value) => Ok(SourceOutcome::Returned(obj(&[
                        ("params", params),
                        ("returned", value),
                    ]))),
                    SourceOutcome::Thrown(error) => Ok(SourceOutcome::Thrown(error)),
                }
            }
            crate::types_source::TypesOutcome::Rejected(error) => Ok(SourceOutcome::Thrown(error)),
            crate::types_source::TypesOutcome::ReturnedError(error) => {
                Ok(SourceOutcome::Returned(obj(&[
                    ("name", s(error.name)),
                    ("message", s(error.message)),
                ])))
            }
        }
    }
}
impl ContractCore {
    fn new(policy: SourceContractRuntimePolicy) -> Self {
        Self {
            policy,
            queue_context: None,
            clearing_contexts: vec![],
            deleverage_trades: vec![],
            liquidation_marks: vec![],
            contract_series: vec![],
            tally_mod: V::Undefined,
            tally_static_initialized: false,
            channels: crate::channels_source::SourceChannels::new(),
            property_cache: vec![],
            vault_initialized: false,
            source_unhandled_rejections: vec![],
            source_supply_delta: None,
            source_history_jobs: vec![],
            tables: vec![],
            margins: MarginMapSourceOwner::new(),
            tally: vec![],
            fee_buffer: vec![],
            block_trades: vec![],
            oracle_marks: vec![],
            native_marks: vec![],
            globals: obj(&[]),
            effects: vec![],
            effects_exhausted: false,
            failures: vec![],
            uuid: 0,
            db_id: 0,
            random: 1,
            height: n(10.0),
            time_millis: 1_700_000_000_000,
            contract_mod: false,
            contract_mod_raw: V::Undefined,
            entry: "main-subgraph".into(),
            active: true,
            insurance: obj(&[]),
            oracle_source_cache: vec![],
            oracle_source_watermarks: vec![],
            oracle_source_initialized: false,
        }
    }
    fn global_clearing_enabled(&self) -> bool {
        self.policy == SourceContractRuntimePolicy::GlobalClearingV2
    }
    fn rows(&self, name: &str) -> &[V] {
        self.tables
            .iter()
            .find(|t| t.name == name)
            .map_or(&[], |t| t.rows.as_slice())
    }
    fn table(&mut self, name: &str) -> R<&mut Vec<V>> {
        if let Some(i) = self.tables.iter().position(|t| t.name == name) {
            return Ok(&mut self.tables[i].rows);
        };
        if self.tables.len() >= MAX_TABLES {
            return Err(SourceRuleError::Capacity.into());
        };
        self.tables.push(Table {
            name: name.into(),
            rows: vec![],
        });
        Ok(&mut self.tables.last_mut().unwrap().rows)
    }
    fn import(&mut self, name: &str, rows: Vec<V>) -> Result<(), SourceRuleError> {
        if name.len() > 128 || rows.len() > MAX_TABLE_ROWS {
            return Err(SourceRuleError::Capacity);
        };
        for row in &rows {
            row.validate()?;
            if !matches!(row, V::Object(_)) {
                return Err(SourceRuleError::InvalidInput);
            }
        }
        self.table(name)
            .map_err(|_| SourceRuleError::Capacity)?
            .clone_from(&rows);
        Ok(())
    }
    fn emit(&mut self, event: V) {
        if self.effects.len() < 65_536 {
            self.effects.push(event)
        } else {
            self.effects_exhausted = true
        }
    }
    fn check_failure(&mut self, collection: &str, operation: &str) -> R<()> {
        if let Some(i) = self
            .failures
            .iter()
            .position(|f| f.collection == collection && f.operation == operation)
        {
            if self.failures[i].skip > 0 {
                self.failures[i].skip -= 1
            } else {
                return Err(error(self.failures.remove(i).message));
            }
        };
        Ok(())
    }
    fn database(&mut self, name: &str) -> R<V> {
        self.table(name)?;
        let result = self
            .check_failure(name, "getDatabase")
            .map(|()| obj(&[("sourceDatabase", s(name))]));
        let mut event = obj(&[
            ("kind", s("database")),
            ("name", s(name)),
            ("method", s("getDatabase")),
            ("args", V::Array(vec![s(name)])),
        ]);
        match &result {
            Ok(_) => set(&mut event, "returned", obj(&[("database", s(name))]))?,
            Err(Fault::Thrown(e)) => set(
                &mut event,
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )?,
            _ => (),
        };
        self.emit(event);
        if self.global_clearing_enabled() {
            self.source_history_after_database(name)?;
            self.source_supply_after_database(name)?;
        }
        result
    }
    fn matches(&self, row: &V, query: &V) -> R<bool> {
        if self.global_clearing_enabled() && matches!(query, V::Undefined) {
            return Ok(true);
        }
        let V::Object(fields) = query else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        for (k, wanted) in fields {
            if k == "$or" {
                let mut found = false;
                for q in array(wanted)? {
                    if self.matches(row, &q)? {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Ok(false);
                };
                continue;
            };
            let actual = get_path(row, k);
            let matched = if let V::Object(ops) = wanted {
                let mut yes = true;
                for (op, v) in ops {
                    yes &= match op.as_str() {
                        "$ne" => !same(&actual, v),
                        "$exists" => matches!(actual, V::Undefined) != truth(v),
                        "$in" => array(v)?.iter().any(|v| same(&actual, v)),
                        "$gte" => number(&actual)? >= number(v)?,
                        "$lte" => number(&actual)? <= number(v)?,
                        "$gt" => number(&actual)? > number(v)?,
                        "$lt" => number(&actual)? < number(v)?,
                        "$regex" if self.global_clearing_enabled() => {
                            let pattern = text(v)?;
                            let Some(prefix) = pattern.strip_prefix('^') else {
                                return Err(SourceRuleError::UnknownDependency.into());
                            };
                            if prefix.chars().any(|c| "\\.[](){}*+?$|^".contains(c)) {
                                return Err(SourceRuleError::UnknownDependency.into());
                            }
                            matches!(&actual,V::String(value) if value.starts_with(prefix))
                        }
                        _ => return Err(SourceRuleError::UnknownDependency.into()),
                    }
                }
                yes
            } else {
                same(&actual, wanted)
            };
            if !matched {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn storage_clone(&self, v: &V) -> R<V> {
        if self.global_clearing_enabled() {
            source_storage_clone(v)
        } else {
            Ok(v.clone())
        }
    }
    fn db(&mut self, name: &str, op: &str, args: Vec<V>) -> R<V> {
        let result = (|| {
            self.check_failure(name, op)?;
            match op {
                "findOneAsync" => {
                    for row in self.rows(name) {
                        if self.matches(row, &arg(&args, 0))? {
                            return Ok(row.clone());
                        }
                    }
                    Ok(V::Null)
                }
                "findAsync" => {
                    let mut found = vec![];
                    for row in self.rows(name) {
                        if self.matches(row, &arg(&args, 0))? {
                            found.push(row.clone())
                        }
                    }
                    Ok(V::Array(found))
                }
                "get" => Ok(self
                    .rows(name)
                    .iter()
                    .find(|r| same(&r.get("_id"), &arg(&args, 0)))
                    .cloned()
                    .unwrap_or(V::Null)),
                "put" => {
                    let key = arg(&args, 0);
                    let mut doc = obj(&[("_id", key.clone())]);
                    let V::Object(fields) = self.storage_clone(&arg(&args, 1))? else {
                        return Err(SourceRuleError::InvalidInput.into());
                    };
                    for (k, v) in fields {
                        set(&mut doc, &k, v)?
                    }
                    let rows = self.table(name)?;
                    if let Some(i) = rows.iter().position(|r| same(&r.get("_id"), &key)) {
                        rows[i] = doc
                    } else {
                        if rows.len() >= MAX_TABLE_ROWS {
                            return Err(SourceRuleError::Capacity.into());
                        };
                        rows.push(doc)
                    };
                    Ok(V::Undefined)
                }
                "loadDatabase" => Ok(V::Undefined),
                "removeAsync" if self.global_clearing_enabled() => {
                    let query = arg(&args, 0);
                    let multi = truth(&arg(&args, 1).get("multi"));
                    let mut removed = 0;
                    let mut keep = Vec::new();
                    for row in self.rows(name) {
                        if (multi || removed == 0) && self.matches(row, &query)? {
                            removed += 1;
                        } else {
                            keep.push(row.clone());
                        }
                    }
                    *self.table(name)? = keep;
                    Ok(n(f64::from(removed)))
                }
                "insertAsync" => {
                    let mut doc = self.storage_clone(&arg(&args, 0))?;
                    if matches!(doc.get("_id"), V::Undefined) {
                        self.db_id += 1;
                        set(&mut doc, "_id", s(format!("db-{}", self.db_id)))?
                    };
                    let rows = self.table(name)?;
                    if rows.iter().any(|r| same(&r.get("_id"), &doc.get("_id"))) {
                        return Err(error("uniqueViolated"));
                    };
                    if rows.len() >= MAX_TABLE_ROWS {
                        return Err(SourceRuleError::Capacity.into());
                    };
                    rows.push(doc.clone());
                    Ok(doc)
                }
                "updateAsync" => {
                    let query = arg(&args, 0);
                    let update = arg(&args, 1);
                    let options = arg(&args, 2);
                    let mut indexes = vec![];
                    for (i, row) in self.rows(name).iter().enumerate() {
                        if self.matches(row, &query)? {
                            indexes.push(i)
                        }
                    }
                    if !truth(&options.get("multi")) {
                        indexes.truncate(1)
                    };
                    if indexes.is_empty() && truth(&options.get("upsert")) {
                        let mut doc = self.storage_clone(&query)?;
                        if matches!(doc.get("_id"), V::Undefined) {
                            self.db_id += 1;
                            set(&mut doc, "_id", s(format!("db-{}", self.db_id)))?
                        };
                        let rows = self.table(name)?;
                        if rows.len() >= MAX_TABLE_ROWS {
                            return Err(SourceRuleError::Capacity.into());
                        };
                        indexes.push(rows.len());
                        rows.push(doc)
                    };
                    for i in &indexes {
                        let old = self.rows(name)[*i].clone();
                        let V::Object(fields) = &update else {
                            return Err(SourceRuleError::InvalidInput.into());
                        };
                        let mut row = if fields.iter().any(|(k, _)| k.starts_with('$')) {
                            let mut row = old.clone();
                            for (op, values) in fields {
                                let V::Object(values) = values else {
                                    return Err(SourceRuleError::InvalidInput.into());
                                };
                                for (k, v) in values {
                                    let next = match op.as_str() {
                                        "$set" => self.storage_clone(v)?,
                                        "$inc" => n(number(&nullish(get_path(&row, k), n(0.0)))?
                                            + number(v)?),
                                        _ => return Err(SourceRuleError::UnknownDependency.into()),
                                    };
                                    set_path(&mut row, k, next)?
                                }
                            }
                            row
                        } else {
                            self.storage_clone(&update)?
                        };
                        if matches!(row.get("_id"), V::Undefined)
                            && (!self.global_clearing_enabled()
                                || !matches!(old.get("_id"), V::Undefined))
                        {
                            set(&mut row, "_id", old.get("_id"))?
                        };
                        self.table(name)?[*i] = row
                    }
                    Ok(n(indexes.len() as f64))
                }
                _ => Err(SourceRuleError::UnknownDependency.into()),
            }
        })();
        let result = result.and_then(|v| self.storage_clone(&v));
        let short = op.strip_suffix("Async").unwrap_or(op);
        let mut event = obj(&[
            ("kind", s("db")),
            ("collection", s(name)),
            ("op", s(short)),
            ("method", s(op)),
            ("args", self.storage_clone(&V::Array(args))?),
        ]);
        match &result {
            Ok(v) => set(&mut event, "returned", v.clone())?,
            Err(Fault::Thrown(e)) => set(
                &mut event,
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )?,
            _ => (),
        };
        self.emit(event);
        if self.global_clearing_enabled() {
            self.source_supply_after_db(name, op)?;
        }
        result
    }
    fn find(&mut self, name: &str, query: V) -> R<V> {
        self.db(name, "findOneAsync", vec![query])
    }
    fn update(&mut self, name: &str, query: V, update: V, upsert: bool) -> R<V> {
        self.db(
            name,
            "updateAsync",
            vec![query, update, obj(&[("upsert", V::Bool(upsert))])],
        )
    }
    fn call(&mut self, target: &str, args: Vec<V>) -> R<V> {
        awaited(self.invoke(&DependencyCall {
            target: target.into(),
            arguments: args,
        })?)
    }
    fn new_uuid(&mut self) -> String {
        self.uuid += 1;
        let id = format!("source-contract-{}", self.uuid);
        self.emit(obj(&[("kind", s("uuid")), ("value", s(&id))]));
        id
    }
    fn margin_call(&mut self, method: &str, args: &[V]) -> R<V> {
        let handle = arg(args, 0);
        let handle_id = handle.get("sourceMarginInstance");
        let (instance, original) = if matches!(handle_id, V::Number(_)) {
            (Some(number(&handle_id)? as u64), &args[1..])
        } else {
            (None, args)
        };
        let aliases = if let V::Array(entries) = handle.get("sourcePositionAliases") {
            entries
                .iter()
                .map(|v| {
                    Ok(PositionAlias {
                        argument: number(&v.get("argument"))? as usize,
                        instance: instance.ok_or(SourceRuleError::InvalidInput)?,
                        address: v.get("address"),
                    })
                })
                .collect::<R<Vec<_>>>()?
        } else {
            vec![]
        };
        if method == "inspectPosition" {
            return self
                .margins
                .inspect_position(
                    instance.ok_or(SourceRuleError::InvalidInput)?,
                    &arg(original, 0),
                )
                .map_err(Fault::Local);
        };
        let object_aliases = if let V::Array(entries) = if self.global_clearing_enabled() {
            handle.get("sourcePositionObjectAliases")
        } else {
            V::Undefined
        } {
            entries
                .iter()
                .map(|v| {
                    Ok(PositionObjectAlias {
                        argument: number(&v.get("argument"))? as usize,
                        index: if matches!(v.get("index"), V::Undefined) {
                            None
                        } else {
                            Some(number(&v.get("index"))? as usize)
                        },
                        object: number(&v.get("object"))? as usize,
                    })
                })
                .collect::<R<Vec<_>>>()?
        } else {
            vec![]
        };
        let mut owner = std::mem::replace(&mut self.margins, MarginMapSourceOwner::new());
        let result = owner
            .evaluate_detailed_with_object_aliases(
                instance,
                &SourceCall {
                    method: method.into(),
                    arguments: original.to_vec(),
                },
                &aliases,
                &[],
                &object_aliases,
                self,
            )
            .map(|v| v.outcome);
        self.margins = owner;
        outcome(result?)
    }
    fn dispatch(&mut self, call: &DependencyCall) -> R<V> {
        let a = &call.arguments;
        let target = call.target.as_str();
        if let Some(method) = target.strip_prefix("db.") {
            if method == "getDatabase" {
                return self.database(&text(&arg(a, 0))?);
            };
            if a.is_empty() || !matches!(arg(a, 0).get("sourceDatabase"), V::String(_)) {
                return Err(SourceRuleError::InvalidInput.into());
            };
            return self.db(
                &text(&arg(a, 0).get("sourceDatabase"))?,
                method,
                a[1..].to_vec(),
            );
        }
        if let Some(method) = target.strip_prefix("Margin.") {
            if !self.global_clearing_enabled()
                && !crate::margin_map_source_owner::METHODS[..18].contains(&method)
                && method != "inspectPosition"
            {
                return Err(SourceRuleError::UnknownDependency.into());
            }
            return self.margin_call(method, a);
        }
        if let Some(method) = target.strip_prefix("Channels.") {
            if !self.global_clearing_enabled() {
                return Err(SourceRuleError::UnknownDependency.into());
            }
            let mut owner = std::mem::take(&mut self.channels);
            let result = owner.execute_unverified(
                &SourceCall {
                    method: method.into(),
                    arguments: a.clone(),
                },
                &mut SourceChannelsBackend(self),
            );
            self.channels = owner;
            return outcome(result?.outcome);
        }
        if let Some(method) = target.strip_prefix("Property.") {
            if self.global_clearing_enabled() {
                return self.source_clearing_property(method, a);
            }
        }
        if let Some(method) = target.strip_prefix("Vaults.") {
            if self.global_clearing_enabled() {
                return self.source_clearing_vault_query(method, a);
            }
        }
        if let Some(method) = target.strip_prefix("Insurance.") {
            if !self.global_clearing_enabled() {
                return Err(SourceRuleError::UnknownDependency.into());
            }
            return self.source_insurance_call(method, a);
        }
        if target == "Orderbook.estimateLiquidation" && self.global_clearing_enabled() {
            return self.source_liquidation_estimate(a);
        }
        if target == "Orderbook._pruneInstaLiqOrdersFromFreshBook" && self.global_clearing_enabled()
        {
            return self.source_liquidation_prune(a);
        }
        if let Some(method) = target.strip_prefix("Orderbook.") {
            if self.global_clearing_enabled() && method != "recordContractTrade" {
                return self.source_clearing_orderbook_call(method, a);
            }
        }
        if let Some(method) = target.strip_prefix("Tally.") {
            return self.tally_call(method, a);
        }
        if let Some(method) = target.strip_prefix("Contract.") {
            return self.contract_call(method, a);
        }
        if let Some(method) = target.strip_prefix("Clearing.") {
            return self.clearing_call(method, a);
        }
        if let Some(method) = target.strip_prefix("VolumeIndex.") {
            return self.volume_call(method, a);
        }
        if let Some(method) = target.strip_prefix("PnlIou.") {
            return self.iou_call(method, a);
        }
        match target {
            "uuid.v4" => Ok(s(self.new_uuid())),
            "Clock.nowISOString" => Ok(s("2023-11-14T22:13:20.000Z")),
            "Clock.now" => Ok(n(self.time_millis as f64)),
            "TxUtils.getBlockCount" => {
                self.emit(obj(&[
                    ("kind", s("rpc-clock")),
                    ("method", s("TxUtils.getBlockCount")),
                    ("args", V::Array(a.clone())),
                    ("returned", self.height.clone()),
                ]));
                Ok(self.height.clone())
            }
            "SourceGlobal.get" => {
                let key = text(&arg(a, 0))?;
                let value = self.globals.get(&key);
                if matches!(value, V::Undefined) {
                    Err(Fault::Thrown(crate::validity_source::SourceException {
                        name: "ReferenceError".into(),
                        message: format!("{key} is not defined"),
                    }))
                } else {
                    Ok(value)
                }
            }
            "ClearList.isAddressInClearlist" => self.clearlist_contains(a),
            "Orderbook.recordContractTrade" => self.record_contract_trade(a),
            "TradeHistory.savePNL" => self.save_pnl(a),
            "Oracle.load" => self.oracle_source_load(),
            "Oracle.publishData" => self.oracle_publish(a),
            "Oracle.getOracleInfo" => self.oracle_source_info(arg(a, 0)),
            "Oracle.getOraclePrice" => self.oracle_price(arg(a, 0)),
            "Oracle.getTWAP" if self.global_clearing_enabled() => {
                self.source_funding_oracle_twap(a)
            }
            "Activation.isTxTypeActive" => Ok(V::Bool(self.active)),
            "Activation.getAdmin" => Ok(s("admin")),
            "Activation.areActivationsAboveThreshold" => Ok(V::Bool(false)),
            "ClearList.getBanlist" => {
                self.database("banlist")?;
                self.find("banlist", obj(&[]))?;
                Ok(V::Undefined)
            }
            "ClearList.getCountryCodeByAddress" => {
                self.database("attestations")?;
                let rows = array(&self.db(
                    "attestations",
                    "findAsync",
                    vec![obj(&[("data.address", arg(a, 0))])],
                )?)?;
                for row in rows {
                    let row = row.get("data");
                    if number(&nullish(row.get("listId"), row.get("clearlistId")))? == 0.0
                        && nullish(row.get("status"), s("active")) == s("active")
                    {
                        let data = row.get("data");
                        if let V::String(code) = data {
                            return Ok(obj(&[("countryCode", s(code)), ("address", arg(a, 0))]));
                        }
                    }
                }
                Ok(V::Null)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
impl SourceBackend for ContractCore {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let start = self.effects.len();
        let trace = !matches!(
            call.target.as_str(),
            "Margin.inspectPosition"
                | "SourceGlobal.get"
                | "Orderbook.inspectBook"
                | "Orderbook.setCachedBook"
                | "Orderbook.newInstance"
        ) && !call.target.starts_with("db.");
        if trace {
            self.emit(obj(&[
                ("kind", s("dependency")),
                ("target", s(&call.target)),
                ("args", V::Array(call.arguments.clone())),
            ]))
        }
        let result = self.dispatch(call);
        if trace && start < self.effects.len() {
            match &result {
                Ok(v) => {
                    let _ = set(&mut self.effects[start], "returned", v.clone());
                }
                Err(Fault::Thrown(e)) => {
                    let _ = set(
                        &mut self.effects[start],
                        "thrown",
                        obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                    );
                }
                _ => (),
            }
        }
        if matches!(
            call.target.as_str(),
            "uuid.v4"
                | "Activation.getAdmin"
                | "Activation.areActivationsAboveThreshold"
                | "Clock.nowISOString"
                | "Clock.now"
        ) || (self.global_clearing_enabled()
            && matches!(
                call.target.as_str(),
                "Clearing.clampFundingRate"
                    | "Clearing.calculateClearingPNL"
                    | "Clearing.calculateNewContractPNL"
                    | "Clearing.computeLossCoverage"
                    | "Clearing.initPositionCache"
                    | "Clearing.getPositionsFromCache"
                    | "Clearing.flushPositionCache"
                    | "Clearing.addOrUpdatePositionInCache"
                    | "Clearing.recordDeleverageTrade"
                    | "Clearing.recordLiquidation"
                    | "Clearing.recordTrade"
                    | "Clearing.getLiquidation"
                    | "Clearing.getDeleverageTradesThisBlock"
                    | "Clearing.resetBlockTracking"
                    | "Clearing.resetBlockTrades"
                    | "Margin.generateLiquidationOrder"
                    | "Margin.calcPriceDelta"
                    | "Margin.needsLiquidation"
                    | "Margin.calculateLiquidationPrice"
                    | "Margin.calculateMarginRequirement"
                    | "Margin.estimatePNL"
                    | "PnlIou.key"
                    | "PnlIou.claimKey"
                    | "PnlIou.blockReductionTowardZero"
                    | "Clearing.resolveOptionExerciseTarget"
                    | "Clearing.computeExerciseContracts"
                    | "Clearing.applyAssignedUnderlyingPosition"
                    | "Clearing.computeLiquidationPriceFromLoss"
            ))
        {
            match result {
                Ok(v) => Ok(DependencyReturn::Immediate(v)),
                Err(Fault::Thrown(e)) => Ok(DependencyReturn::Throw(e)),
                Err(Fault::Local(e)) => Err(e),
            }
        } else {
            returned(result)
        }
    }
}
struct PlacementBackend<'a> {
    core: &'a mut ContractCore,
}
impl SourceBackend for PlacementBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        self.core.invoke(call)
    }
}
impl crate::contract_placement_source::ContractPlacementBackend for PlacementBackend<'_> {
    fn invoke_in_queue(
        &mut self,
        queue: &mut ContractQueueSourceOwner,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        self.core.harvest_queue(queue);
        if call.target == "Orderbook.processContractMatches" {
            let mut backend = FinancialBackend {
                core: self.core,
                queue,
            };
            match crate::contract_settlement_source::process_contract_matches(
                &call.arguments[1..],
                &mut backend,
            )? {
                SourceOutcome::Returned(v) => Ok(DependencyReturn::Pending(v)),
                SourceOutcome::Thrown(e) => Ok(DependencyReturn::PendingThrow(e)),
            }
        } else {
            self.core.invoke(call)
        }
    }
}
struct FinancialBackend<'a> {
    core: &'a mut ContractCore,
    queue: &'a mut ContractQueueSourceOwner,
}
impl SourceBackend for FinancialBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        self.core.harvest_queue(self.queue);
        if call.target == "TradeHistory.savePNL" && self.core.global_clearing_enabled() {
            return returned(self.core.source_history_begin(&call.arguments));
        }
        if matches!(
            call.target.as_str(),
            "Orderbook.cancelExcessOrders" | "Orderbook.sourceCrossContractReserve"
        ) {
            let mut backend = PlacementBackend { core: self.core };
            let result = if call.target == "Orderbook.cancelExcessOrders" {
                crate::contract_placement_source::cancel_excess_orders(
                    self.queue,
                    &call.arguments,
                    &mut backend,
                )
            } else {
                crate::contract_placement_source::source_cross_contract_reserve(
                    self.queue,
                    &call.arguments,
                    &mut backend,
                )
            }?;
            self.core.harvest_queue(self.queue);
            match result {
                SourceOutcome::Returned(v) => Ok(DependencyReturn::Pending(v)),
                SourceOutcome::Thrown(e) => Ok(DependencyReturn::PendingThrow(e)),
            }
        } else {
            self.core.invoke(call)
        }
    }
}
impl ContractCore {
    fn harvest_queue(&mut self, queue: &mut ContractQueueSourceOwner) {
        let old = queue.take_db_calls();
        let detailed = queue.take_detailed_db_calls();
        if self.global_clearing_enabled() {
            match detailed {
                Ok(events) => {
                    for event in events {
                        self.emit(event);
                    }
                }
                Err(_) => self.effects_exhausted = true,
            }
        } else {
            for call in old {
                self.emit(call.value());
            }
        }
    }
}
include!("source_contract_accounting_impl.rs");
include!("source_contract_aux_impl.rs");

include!("source_contract_fee_impl.rs");

include!("source_contract_oracle_impl.rs");

include!("source_clearing_funding_impl.rs");
include!("source_insurance_lifecycle_impl.rs");
include!("source_clearing_global_impl.rs");

include!("source_insurance_legacy_v1_impl.rs");

include!("source_iou_settlement_impl.rs");
include!("source_clearing_liquidation_impl.rs");
include!("source_clearing_expiry_impl.rs");

include!("source_clearing_mark_settlement_impl.rs");

include!("source_clearing_supply_impl.rs");

include!("source_contract_history_schedule_impl.rs");
