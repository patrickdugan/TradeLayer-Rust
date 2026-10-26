//! Source-shaped activation metadata. Observations never authorize transactions.
use crate::issuance_intent::JsonValue;
use crate::validity_source::{
    DependencyCall, DependencyObservation, DependencyReturn, SourceBackend, SourceException,
    SourceOutcome, SourceRuleError, SourceValue as V,
};

pub const SOURCE_SHA256: &str = "268f08438eb80765587b8ce24d719f41de573648e108ef64df557faad870ea0e";
pub const MAX_ACTIONS: usize = 4096;
pub const MAX_JSON_BYTES: usize = 65_536;
pub const GENESIS_ADMIN: &str = "tltc1qa0kd2d39nmeph3hvcx8ytv65ztcywg5sazhtw8";
pub const METHODS: [&str; 18] = [
    "init",
    "updateAdminAddress",
    "delay",
    "getAdmin",
    "saveActivationsList",
    "loadActivationsList",
    "activate",
    "areActivationsAboveThreshold",
    "initializeTxRegistry",
    "getActivationBlock",
    "isTxTypeActive",
    "checkActivationBlock",
    "isSenderGenesisAdmin",
    "isValidJSON",
    "isValidJavaScript",
    "minifyJavaScript",
    "getNextTxTypeId",
    "saveNewTransaction",
];
pub const REGISTRY_NAMES: [&str; 36] = [
    "Activate TradeLayer",
    "Token Issue",
    "Send",
    "Trade Token for UTXO",
    "Commit Token",
    "On-chain Token for Token",
    "cancelOrder",
    "Create Whitelist",
    "Update Admin",
    "Issue or Revoke Attestation",
    "AMM",
    "Grant Managed Token",
    "Redeem Managed Token",
    "Create Oracle",
    "Publish Oracle Data",
    "Close Oracle",
    "Create Future Contract Series",
    "Exercise Derivative",
    "Trade Contract On-chain",
    "Trade Contract Channel",
    "Trade Tokens Channel",
    "Withdrawal",
    "Transfer",
    "Settle Channel PNL",
    "Mint Synthetic",
    "Redeem Synthetic",
    "Pay to Tokens",
    "Create Option Chain",
    "Trade Bai Urbun",
    "Trade Murabaha",
    "Oracle Stake/Fraud/Relay",
    "Batch Move Zk Rollup",
    "Publish New Tx",
    "Colored Coin",
    "Cross Layer Bridge",
    "Smart Contract Bind",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationCall {
    pub method: String,
    pub arguments: Vec<V>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionalActivationField {
    Chain,
    Test,
    AdminAddress,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceActivationState {
    pub consensus_vector: V,
    pub tx_registry: V,
    /// `None` is an absent own property; `Some(Undefined)` is present.
    pub chain: Option<V>,
    pub test: Option<V>,
    pub admin_address: Option<V>,
    field_order: Vec<OptionalActivationField>,
    array_row_properties: std::collections::BTreeMap<String, V>,
}
impl SourceActivationState {
    pub fn optional_field_order(&self) -> &[OptionalActivationField] {
        &self.field_order
    }
    pub fn array_row_properties(&self) -> &std::collections::BTreeMap<String, V> {
        &self.array_row_properties
    }
    pub fn array_properties_value(&self) -> V {
        V::Object(
            self.array_row_properties
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
    }
    fn assign(&mut self, field: OptionalActivationField, value: V) {
        let slot = match field {
            OptionalActivationField::Chain => &mut self.chain,
            OptionalActivationField::Test => &mut self.test,
            OptionalActivationField::AdminAddress => &mut self.admin_address,
        };
        if slot.is_none() {
            self.field_order.push(field);
        }
        *slot = Some(value);
    }
    fn replace_registry(&mut self, value: V) {
        self.tx_registry = value;
        self.array_row_properties.clear();
    }
    pub fn diagnostic_value(&self) -> V {
        let mut a = vec![
            ("consensusVector".into(), self.consensus_vector.clone()),
            ("txRegistry".into(), self.tx_registry.clone()),
        ];
        for field in &self.field_order {
            let (name, value) = match field {
                OptionalActivationField::Chain => ("chain", &self.chain),
                OptionalActivationField::Test => ("test", &self.test),
                OptionalActivationField::AdminAddress => ("adminAddress", &self.admin_address),
            };
            if let Some(value) = value {
                a.push((name.into(), value.clone()));
            }
        }
        V::Object(a)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationObservation {
    pub outcome: SourceOutcome,
    pub state_after: SourceActivationState,
    pub dependencies: Vec<DependencyObservation>,
    /// Catch site and exception name. Source logs are not a stable error-message API.
    pub swallowed: Vec<(String, String)>,
    pub deferred_rejections: Vec<SourceException>,
}
impl ActivationObservation {
    pub const fn require_authenticated_admission(&self) -> Result<(), SourceRuleError> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceActivation {
    state: SourceActivationState,
    actions: usize,
}
impl Default for SourceActivation {
    fn default() -> Self {
        Self::new()
    }
}
impl SourceActivation {
    pub fn new() -> Self {
        Self {
            state: SourceActivationState {
                consensus_vector: V::object(),
                tx_registry: default_registry(),
                chain: None,
                test: None,
                admin_address: None,
                field_order: Vec::new(),
                array_row_properties: std::collections::BTreeMap::new(),
            },
            actions: 0,
        }
    }
    pub fn state(&self) -> &SourceActivationState {
        &self.state
    }
    pub const fn action_count(&self) -> usize {
        self.actions
    }
    /// Source throws retain partial metadata; local boundary errors do not commit metadata.
    /// External adapters may already have observed calls and must not be treated as transactional.
    pub fn execute_unverified<B: SourceBackend + ?Sized>(
        &mut self,
        call: ActivationCall,
        backend: &mut B,
    ) -> Result<ActivationObservation, SourceRuleError> {
        if self.actions >= MAX_ACTIONS || call.arguments.len() > 8 {
            return Err(SourceRuleError::Capacity);
        }
        if !METHODS.contains(&call.method.as_str()) {
            return Err(SourceRuleError::UnknownMethod);
        }
        for v in &call.arguments {
            v.validate()?;
            domain(v)?;
        }
        let mut next = self.clone();
        let mut ctx = Context {
            backend,
            dependencies: Vec::new(),
            swallowed: Vec::new(),
            deferred_rejections: Vec::new(),
        };
        let outcome = match next.run(&call, &mut ctx) {
            Ok(v) => SourceOutcome::Returned(v),
            Err(Failure::Thrown(e)) => SourceOutcome::Thrown(e),
            Err(Failure::Local(e)) => return Err(e),
        };
        next.state.diagnostic_value().validate()?;
        next.state.array_properties_value().validate()?;
        next.actions += 1;
        let result = ActivationObservation {
            outcome,
            state_after: next.state.clone(),
            dependencies: ctx.dependencies,
            swallowed: ctx.swallowed,
            deferred_rejections: ctx.deferred_rejections,
        };
        *self = next;
        Ok(result)
    }
    fn run<B: SourceBackend + ?Sized>(
        &mut self,
        call: &ActivationCall,
        c: &mut Context<'_, B>,
    ) -> Exec<V> {
        let a = |i| call.arguments.get(i).cloned().unwrap_or(V::Undefined);
        match call.method.as_str() {
            "init" => {
                c.dep("Client.getInstance", vec![])?;
                let chain = c.dep(
                    "Client.getChain",
                    if truth(&a(0)) { vec![a(0)] } else { vec![] },
                )?;
                self.state.assign(OptionalActivationField::Chain, chain);
                let test = c.dep("Client.getTest", vec![])?;
                self.state.assign(OptionalActivationField::Test, test);
                self.update_admin();
                Ok(V::Undefined)
            }
            "updateAdminAddress" => {
                self.update_admin();
                Ok(V::Undefined)
            }
            "delay" => {
                c.dep_mode("Timer.setTimeout", vec![a(0)], false)?;
                Ok(V::Undefined)
            }
            "getAdmin" => Ok(self.state.admin_address.clone().unwrap_or(V::Undefined)),
            "saveActivationsList" => {
                self.save(c)?;
                Ok(V::Undefined)
            }
            "loadActivationsList" => self.load(c),
            "initializeTxRegistry" => Ok(default_registry()),
            "activate" => {
                let key = parse_int(&a(0))?;
                self.load(c)?;
                if key == "0" {
                    c.dep(
                        "Vesting.getInstance",
                        vec![
                            self.state.admin_address.clone().unwrap_or(V::Undefined),
                            self.state.chain.clone().unwrap_or(V::Undefined),
                            self.state.test.clone().unwrap_or(V::Undefined),
                        ],
                    )?;
                    c.dep("Vesting.initializeTokens", vec![a(1)])?;
                    self.activate_row(&key, a(1), a(2))?;
                    self.save(c)?;
                    c.dep("Consensus.pushLatestActivationToConsensusVector", vec![])?;
                    Ok(V::Undefined)
                } else {
                    let row = get(&self.state.tx_registry, &key)?;
                    if !truth(&row) {
                        return Ok(V::Undefined);
                    }
                    self.activate_row(&key, a(1), a(2))?;
                    self.save(c)?;
                    c.dep("Consensus.pushLatestActivationToConsensusVector", vec![])?;
                    get(&self.state.tx_registry, &key)
                }
            }
            "areActivationsAboveThreshold" => {
                self.load(c)?;
                let rows = entries(&self.state.tx_registry)?;
                if rows.is_empty() {
                    return Ok(V::Bool(false));
                }
                let mut active = 0;
                for (_, row) in &rows {
                    if truth(&get(row, "active")?) {
                        active += 1;
                    }
                }
                Ok(V::Bool((active as f64 / rows.len() as f64) * 100.0 > 90.0))
            }
            "getActivationBlock" => {
                let key = text(&a(0))?;
                if matches!(self.state.tx_registry, V::Null | V::Undefined) {
                    return Err(type_error(format!(
                        "Cannot read properties of {} (reading 'hasOwnProperty')",
                        nullish_name(&self.state.tx_registry)
                    )));
                }
                if own(&self.state.tx_registry, "hasOwnProperty") {
                    return Err(type_error(
                        "this.txRegistry.hasOwnProperty is not a function",
                    ));
                }
                if !own(&self.state.tx_registry, &key) {
                    Ok(V::Null)
                } else {
                    let row = get(&self.state.tx_registry, &key)?;
                    let value = if matches!(row, V::Array(_)) {
                        self.state
                            .array_row_properties
                            .get(&key)
                            .map_or(V::Undefined, |v| v.get("activationBlock"))
                    } else {
                        get(&row, "activationBlock")?
                    };
                    Ok(if truth(&value) { value } else { V::Null })
                }
            }
            "isTxTypeActive" => {
                self.load(c)?;
                let row = get(&self.state.tx_registry, &text(&a(0))?)?;
                if matches!(row, V::Null | V::Undefined) {
                    return Ok(V::Bool(false));
                }
                Ok(V::Bool(loose_true(&get(&row, "active")?)?))
            }
            "checkActivationBlock" => {
                self.load(c)?;
                let row = get(&self.state.tx_registry, &text(&a(0))?)?;
                let value = get(&row, "activationBlock")?;
                Ok(if matches!(value, V::Null | V::Undefined) {
                    V::Null
                } else {
                    value
                })
            }
            "isSenderGenesisAdmin" => Ok(V::Bool(a(0) == V::String(GENESIS_ADMIN.into()))),
            "isValidJSON" => {
                let input = text(&a(0))?;
                match parse_json(&input) {
                    Ok(_) => Ok(V::Bool(true)),
                    Err(Failure::Thrown(_)) => Ok(V::Bool(false)),
                    Err(e) => Err(e),
                }
            }
            "isValidJavaScript" => Ok(V::Bool(true)),
            "minifyJavaScript" => Ok(a(0)),
            "getNextTxTypeId" | "saveNewTransaction" => Ok(V::Undefined),
            _ => Err(Failure::Local(SourceRuleError::UnknownMethod)),
        }
    }
    fn update_admin(&mut self) {
        let test = self.state.test.as_ref().is_some_and(truth);
        let main = match self.state.chain.as_ref() {
            Some(V::String(s)) if s == "BTC" => {
                if test {
                    "tb1q8f84erfegxhaylmvpfll9m5rgwymqy4akjnnvq"
                } else {
                    "bc1qktknrnx2jcchjht9anz0uy8ae02xryxq2vxeem"
                }
            }
            Some(V::String(s)) if s == "DOGE" => {
                if test {
                    "nop27JQWbGr95ySHXZMzCg8XXxYzbCBZAW"
                } else {
                    "DLSfu9qvEggkeXAgCAwBBw5BVLvMCtkewz"
                }
            }
            Some(V::String(s)) if s == "LTC" => {
                if test {
                    GENESIS_ADMIN
                } else {
                    "MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB"
                }
            }
            _ => return,
        };
        self.state.assign(
            OptionalActivationField::AdminAddress,
            V::String(main.into()),
        );
    }
    fn activate_row(&mut self, key: &str, block: V, hash: V) -> Exec<()> {
        let mut row = get(&self.state.tx_registry, key)?;
        if matches!(row, V::Array(_)) {
            let mut properties = self
                .state
                .array_row_properties
                .get(key)
                .cloned()
                .unwrap_or_else(V::object);
            properties.insert("active", V::Bool(true))?;
            properties.insert("activationBlock", block)?;
            if truth(&hash) {
                properties.insert("codeHash", hash)?;
            }
            self.state
                .array_row_properties
                .insert(key.into(), properties);
            return Ok(());
        }
        set(&mut row, "active", V::Bool(true))?;
        set(&mut row, "activationBlock", block)?;
        if truth(&hash) {
            set(&mut row, "codeHash", hash)?;
        }
        set(&mut self.state.tx_registry, key, row)
    }
    fn save<B: SourceBackend + ?Sized>(&mut self, c: &mut Context<'_, B>) -> Exec<()> {
        let result = (|| {
            c.dep("DB.getDatabase", vec![V::String("activations".into())])?;
            let json = stringify(&self.state.tx_registry)?;
            let value = json.map_or(V::Undefined, V::String);
            c.dep(
                "DB.updateAsync",
                vec![
                    obj(vec![("_id", V::String("activationsList".into()))]),
                    obj(vec![("$set", obj(vec![("value", value)]))]),
                    obj(vec![("upsert", V::Bool(true))]),
                ],
            )?;
            Ok(())
        })();
        c.catch("save", result).map(|_| ())
    }
    fn load<B: SourceBackend + ?Sized>(&mut self, c: &mut Context<'_, B>) -> Exec<V> {
        let result = (|| {
            c.dep("DB.getDatabase", vec![V::String("activations".into())])?;
            let values = c.dep("DB.findAsync", vec![V::object()])?;
            let length = get(&values, "length")?;
            if matches!(length,V::Number(b) if f64::from_bits(b)==0.0) {
                self.state.replace_registry(default_registry());
                self.save(c)?;
                return Ok(self.state.tx_registry.clone());
            }
            let V::Array(values) = values else {
                return Err(type_error("entries.forEach is not a function"));
            };
            let mut last = None;
            let mut prototype = V::object();
            let mut inherited_prototype_setter = true;
            for value in values {
                let key = text(&get(&value, "_id")?)?;
                if key == "activationsList" {
                    last = Some(get(&value, "value")?);
                } else if key == "__proto__" && inherited_prototype_setter {
                    let value = get(&value, "value")?;
                    if matches!(value, V::Object(_) | V::Array(_) | V::Null) {
                        if matches!(value, V::Null) {
                            inherited_prototype_setter = false;
                        }
                        prototype = value;
                    }
                }
            }
            let last = last.unwrap_or_else(|| prototype.get("activationsList"));
            if truth(&last) {
                self.state.replace_registry(parse_json(&text(&last)?)?);
            } else {
                self.state.replace_registry(default_registry());
                self.save(c)?;
            }
            Ok(self.state.tx_registry.clone())
        })();
        match result {
            Ok(v) => Ok(v),
            Err(Failure::Thrown(e)) => {
                c.swallowed.push(("load".into(), e.name));
                self.state.replace_registry(default_registry());
                self.save(c)?;
                Ok(self.state.tx_registry.clone())
            }
            Err(e) => Err(e),
        }
    }
}

/// Constructor and static getInstance share the same metadata owner; constructor chain is ignored.
#[derive(Debug, Default)]
pub struct SourceActivationSingleton {
    instance: Option<SourceActivation>,
}
impl SourceActivationSingleton {
    pub fn get_instance(&mut self, _chain: V) -> &mut SourceActivation {
        self.instance.get_or_insert_with(SourceActivation::new)
    }
    pub fn construct(&mut self, chain: V) -> &mut SourceActivation {
        self.get_instance(chain)
    }
}

/// Named integration repair: bounded intent only. Parent Registry must enforce actor,
/// prior-height activation, replay and any independently funded type-0 policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairedActivationIntent {
    pub types: Vec<u8>,
    pub block: u64,
    pub code_hash: Option<[u8; 32]>,
}
impl RepairedActivationIntent {
    pub fn new(
        types: Vec<u8>,
        block: u64,
        code_hash: Option<[u8; 32]>,
    ) -> Result<Self, SourceRuleError> {
        if types.is_empty() || types.len() > 36 || block > 9_007_199_254_740_991 {
            return Err(SourceRuleError::InvalidInput);
        }
        let mut seen = [false; 36];
        for ty in &types {
            let Some(slot) = seen.get_mut(*ty as usize) else {
                return Err(SourceRuleError::InvalidInput);
            };
            if *slot {
                return Err(SourceRuleError::InvalidInput);
            }
            *slot = true;
        }
        Ok(Self {
            types,
            block,
            code_hash,
        })
    }
    pub fn requires_native_genesis_policy(&self) -> bool {
        self.types.contains(&0)
    }
}

pub fn default_registry() -> V {
    V::Object(
        REGISTRY_NAMES
            .iter()
            .enumerate()
            .map(|(i, n)| {
                (
                    i.to_string(),
                    obj(vec![
                        ("name", V::String((*n).into())),
                        ("active", V::Bool(false)),
                    ]),
                )
            })
            .collect(),
    )
}
fn obj(fields: Vec<(&str, V)>) -> V {
    V::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
enum Failure {
    Thrown(SourceException),
    Local(SourceRuleError),
}
impl From<SourceRuleError> for Failure {
    fn from(e: SourceRuleError) -> Self {
        Self::Local(e)
    }
}
type Exec<T> = Result<T, Failure>;
fn type_error(s: impl Into<String>) -> Failure {
    Failure::Thrown(SourceException {
        name: "TypeError".into(),
        message: s.into(),
    })
}
fn syntax_error() -> Failure {
    Failure::Thrown(SourceException {
        name: "SyntaxError".into(),
        message: "Invalid JSON".into(),
    })
}
struct Context<'a, B: ?Sized> {
    backend: &'a mut B,
    dependencies: Vec<DependencyObservation>,
    swallowed: Vec<(String, String)>,
    deferred_rejections: Vec<SourceException>,
}
impl<B: SourceBackend + ?Sized> Context<'_, B> {
    fn dep(&mut self, target: &str, arguments: Vec<V>) -> Exec<V> {
        self.dep_mode(target, arguments, true)
    }
    fn dep_mode(&mut self, target: &str, arguments: Vec<V>, awaited: bool) -> Exec<V> {
        if self.dependencies.len() >= 32 {
            return Err(SourceRuleError::Capacity.into());
        }
        let call = DependencyCall {
            target: target.into(),
            arguments,
        };
        let returned = self.backend.invoke(&call)?;
        match &returned {
            DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
                v.validate()?;
                domain(v)?;
            }
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                if e.name.len() > 256 || e.message.len() > MAX_JSON_BYTES {
                    return Err(SourceRuleError::Capacity.into());
                }
            }
        }
        self.dependencies.push(DependencyObservation {
            call,
            returned: returned.clone(),
        });
        match returned {
            DependencyReturn::Immediate(v) => Ok(v),
            DependencyReturn::Pending(v) => Ok(if awaited { v } else { V::Promise }),
            DependencyReturn::PendingThrow(e) if !awaited => {
                self.deferred_rejections.push(e);
                Ok(V::Promise)
            }
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                Err(Failure::Thrown(e))
            }
        }
    }
    fn catch<T>(&mut self, stage: &str, result: Exec<T>) -> Exec<Option<T>> {
        match result {
            Ok(v) => Ok(Some(v)),
            Err(Failure::Thrown(e)) => {
                self.swallowed.push((stage.into(), e.name));
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}
fn truth(v: &V) -> bool {
    match v {
        V::Undefined | V::Null => false,
        V::Bool(b) => *b,
        V::Number(b) => {
            let n = f64::from_bits(*b);
            n != 0.0 && !n.is_nan()
        }
        V::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn nullish_name(v: &V) -> &'static str {
    if matches!(v, V::Null) {
        "null"
    } else {
        "undefined"
    }
}
fn get(v: &V, key: &str) -> Exec<V> {
    match v {
        V::Null | V::Undefined => Err(type_error(format!(
            "Cannot read properties of {} (reading '{key}')",
            nullish_name(v)
        ))),
        V::Array(a) => Ok(if key == "length" {
            V::number(a.len() as f64)
        } else {
            key.parse::<usize>()
                .ok()
                .filter(|i| i.to_string() == key)
                .and_then(|i| a.get(i))
                .cloned()
                .unwrap_or(V::Undefined)
        }),
        V::String(s) if key != "length" => {
            let at = key.parse::<usize>().ok().filter(|i| i.to_string() == key);
            if let Some(unit) = at.and_then(|i| s.encode_utf16().nth(i)) {
                let ch = char::from_u32(u32::from(unit))
                    .ok_or(Failure::Local(SourceRuleError::InvalidInput))?;
                Ok(V::String(ch.to_string()))
            } else {
                Ok(V::Undefined)
            }
        }
        V::Object(fields)
            if !fields.iter().any(|(k, _)| k == key)
                && matches!(
                    key,
                    "constructor"
                        | "toString"
                        | "valueOf"
                        | "hasOwnProperty"
                        | "isPrototypeOf"
                        | "propertyIsEnumerable"
                        | "toLocaleString"
                        | "__defineGetter__"
                        | "__defineSetter__"
                        | "__lookupGetter__"
                        | "__lookupSetter__"
                ) =>
        {
            Ok(V::object())
        }
        _ => Ok(v.get(key)),
    }
}
fn own(v: &V, key: &str) -> bool {
    match v {
        V::Object(a) => a.iter().any(|(k, _)| k == key),
        V::Array(a) => {
            key == "length"
                || key
                    .parse::<usize>()
                    .ok()
                    .is_some_and(|i| i < a.len() && i.to_string() == key)
        }
        V::String(s) => {
            key == "length"
                || key
                    .parse::<usize>()
                    .ok()
                    .is_some_and(|i| i < s.encode_utf16().count() && i.to_string() == key)
        }
        _ => false,
    }
}
fn set(v: &mut V, key: &str, value: V) -> Exec<()> {
    match v {
        V::Null | V::Undefined => Err(type_error(format!(
            "Cannot set properties of {} (setting '{key}')",
            nullish_name(v)
        ))),
        V::Object(_) => {
            v.insert(key, value)?;
            Ok(())
        }
        V::Array(a) => {
            let at = key
                .parse::<usize>()
                .ok()
                .filter(|i| i.to_string() == key)
                .ok_or(Failure::Local(SourceRuleError::InvalidInput))?;
            let slot = a
                .get_mut(at)
                .ok_or(Failure::Local(SourceRuleError::InvalidInput))?;
            *slot = value;
            Ok(())
        }
        V::Number(_) | V::Bool(_) | V::String(_) => {
            let kind = match v {
                V::Number(_) => "number",
                V::Bool(_) => "boolean",
                _ => "string",
            };
            Err(type_error(format!(
                "Cannot create property '{key}' on {kind} '{}'",
                text(v)?
            )))
        }
        _ => Err(SourceRuleError::InvalidInput.into()),
    }
}
fn entries(v: &V) -> Exec<Vec<(String, V)>> {
    match v {
        V::Null | V::Undefined => Err(type_error("Cannot convert undefined or null to object")),
        V::Object(a) => Ok(a.clone()),
        V::Array(a) => Ok(a
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect()),
        V::String(s) => Ok(s
            .chars()
            .enumerate()
            .map(|(i, c)| (i.to_string(), V::String(c.to_string())))
            .collect()),
        _ => Ok(Vec::new()),
    }
}
fn domain(v: &V) -> Result<(), SourceRuleError> {
    match v {
        V::Object(a) => {
            for (k, v) in a {
                if matches!(k.as_str(), "__proto__" | "toString" | "valueOf") {
                    return Err(SourceRuleError::InvalidInput);
                }
                domain(v)?;
            }
        }
        V::Array(a) => {
            for v in a {
                domain(v)?;
            }
        }
        V::BigNumber(_) | V::Promise => return Err(SourceRuleError::InvalidInput),
        _ => {}
    }
    Ok(())
}
fn text(v: &V) -> Exec<String> {
    Ok(match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::Number(b) => {
            let n = f64::from_bits(*b);
            if n.is_nan() {
                "NaN".into()
            } else if n == f64::INFINITY {
                "Infinity".into()
            } else if n == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                crate::state_oracle::source_number_text(n)
                    .map_err(|_| Failure::Local(SourceRuleError::UnsupportedNumericDomain))?
            }
        }
        V::String(s) => s.clone(),
        V::Object(_) => "[object Object]".into(),
        V::Array(a) => a
            .iter()
            .map(|v| {
                if matches!(v, V::Null | V::Undefined) {
                    Ok(String::new())
                } else {
                    text(v)
                }
            })
            .collect::<Exec<Vec<_>>>()?
            .join(","),
        _ => return Err(SourceRuleError::InvalidInput.into()),
    })
}
fn number(v: &V) -> Exec<f64> {
    Ok(match v {
        V::Number(b) => f64::from_bits(*b),
        V::Null => 0.0,
        V::Undefined => f64::NAN,
        V::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        _ => {
            let s = text(v)?;
            let s = s.trim_matches(js_space);
            if s.is_empty() {
                0.0
            } else if s == "Infinity" || s == "+Infinity" {
                f64::INFINITY
            } else if s == "-Infinity" {
                f64::NEG_INFINITY
            } else if s.len() > 1
                && s.as_bytes()[0] == b'0'
                && matches!(s.as_bytes()[1], b'x' | b'X' | b'b' | b'B' | b'o' | b'O')
            {
                let radix = match s.as_bytes()[1] {
                    b'x' | b'X' => 16,
                    b'b' | b'B' => 2,
                    _ => 8,
                };
                if s.len() == 2 || !s[2..].chars().all(|c| c.is_digit(radix)) {
                    f64::NAN
                } else {
                    let n = u128::from_str_radix(&s[2..], radix)
                        .map_err(|_| Failure::Local(SourceRuleError::UnsupportedNumericDomain))?;
                    n as f64
                }
            } else {
                s.parse::<f64>().unwrap_or(f64::NAN)
            }
        }
    })
}
fn loose_true(v: &V) -> Exec<bool> {
    match v {
        V::Null | V::Undefined => Ok(false),
        V::Object(_) => Ok(false),
        _ => Ok(number(v)? == 1.0),
    }
}
fn js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}
fn parse_int(v: &V) -> Exec<String> {
    let input = text(v)?;
    let mut s = input.trim_start_matches(js_space);
    let negative = s.starts_with('-');
    if s.starts_with(['-', '+']) {
        s = &s[1..];
    }
    let radix = if s.starts_with("0x") || s.starts_with("0X") {
        s = &s[2..];
        16
    } else {
        10
    };
    let digits = s
        .chars()
        .take_while(|c| c.is_digit(radix))
        .collect::<String>();
    if digits.is_empty() {
        return Ok("NaN".into());
    }
    let mut n = if radix == 10 {
        digits
            .parse::<f64>()
            .map_err(|_| Failure::Local(SourceRuleError::UnsupportedNumericDomain))?
    } else {
        u128::from_str_radix(&digits, 16)
            .map_err(|_| Failure::Local(SourceRuleError::UnsupportedNumericDomain))? as f64
    };
    if negative {
        n = -n;
    }
    text(&V::number(n))
}
fn quote(s: &str) -> Exec<String> {
    JsonValue::String(s.into())
        .to_json()
        .map_err(|_| SourceRuleError::Capacity.into())
}
fn stringify(v: &V) -> Exec<Option<String>> {
    fn write(v: &V, array: bool) -> Exec<Option<String>> {
        Ok(match v {
            V::Undefined => {
                if array {
                    Some("null".into())
                } else {
                    None
                }
            }
            V::Null => Some("null".into()),
            V::Bool(b) => Some(b.to_string()),
            V::Number(b) => {
                let n = f64::from_bits(*b);
                Some(if n.is_finite() {
                    text(v)?
                } else {
                    "null".into()
                })
            }
            V::String(s) => Some(quote(s)?),
            V::Array(a) => Some(format!(
                "[{}]",
                a.iter()
                    .map(|v| write(v, true).map(|s| s.unwrap_or_else(|| "null".into())))
                    .collect::<Exec<Vec<_>>>()?
                    .join(",")
            )),
            V::Object(a) => {
                let mut a = a.iter().collect::<Vec<_>>();
                a.sort_by_key(|(k, _)| {
                    k.parse::<u32>()
                        .ok()
                        .filter(|n| *n != u32::MAX && n.to_string() == *k)
                        .map_or((1, 0), |n| (0, n))
                });
                let mut f = Vec::new();
                for (k, v) in a {
                    if let Some(v) = write(v, false)? {
                        f.push(format!("{}:{v}", quote(k)?));
                    }
                }
                Some(format!("{{{}}}", f.join(",")))
            }
            _ => return Err(SourceRuleError::InvalidInput.into()),
        })
    }
    let s = write(v, false)?;
    if s.as_ref().is_some_and(|s| s.len() > MAX_JSON_BYTES) {
        return Err(SourceRuleError::Capacity.into());
    }
    Ok(s)
}

/// Bounded JSON.parse counterpart; duplicate keys retain their first slot and last value.
fn parse_json(s: &str) -> Exec<V> {
    if s.len() > MAX_JSON_BYTES {
        return Err(SourceRuleError::Capacity.into());
    }
    struct P<'a> {
        s: &'a str,
        at: usize,
        nodes: usize,
    }
    impl P<'_> {
        fn ws(&mut self) {
            while self
                .s
                .as_bytes()
                .get(self.at)
                .is_some_and(|c| matches!(c, b' ' | b'\n' | b'\r' | b'\t'))
            {
                self.at += 1;
            }
        }
        fn string(&mut self) -> Exec<String> {
            let start = self.at;
            self.at += 1;
            let mut escaped = false;
            while let Some(b) = self.s.as_bytes().get(self.at).copied() {
                self.at += 1;
                if b == b'"' && !escaped {
                    let parsed =
                        JsonValue::parse(&self.s[start..self.at]).map_err(|e| match e {
                            crate::issuance_intent::IntentError::UnsupportedSurrogate => {
                                Failure::Local(SourceRuleError::InvalidInput)
                            }
                            _ => syntax_error(),
                        })?;
                    return if let JsonValue::String(s) = parsed {
                        Ok(s)
                    } else {
                        Err(syntax_error())
                    };
                }
                escaped = b == b'\\' && !escaped;
            }
            Err(syntax_error())
        }
        fn value(&mut self, d: usize) -> Exec<V> {
            self.nodes += 1;
            if d > 16 || self.nodes > 4096 {
                return Err(SourceRuleError::Capacity.into());
            }
            self.ws();
            let Some(b) = self.s.as_bytes().get(self.at).copied() else {
                return Err(syntax_error());
            };
            match b {
                b'"' => Ok(V::String(self.string()?)),
                b'{' => {
                    self.at += 1;
                    let mut fields = Vec::new();
                    self.ws();
                    if self.s.as_bytes().get(self.at) == Some(&b'}') {
                        self.at += 1;
                        return Ok(V::Object(fields));
                    }
                    loop {
                        self.ws();
                        if self.s.as_bytes().get(self.at) != Some(&b'"') {
                            return Err(syntax_error());
                        }
                        let key = self.string()?;
                        self.ws();
                        if self.s.as_bytes().get(self.at) != Some(&b':') {
                            return Err(syntax_error());
                        }
                        self.at += 1;
                        let value = self.value(d + 1)?;
                        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == &key) {
                            *v = value;
                        } else {
                            fields.push((key, value));
                        }
                        self.ws();
                        match self.s.as_bytes().get(self.at) {
                            Some(b'}') => {
                                self.at += 1;
                                break;
                            }
                            Some(b',') => self.at += 1,
                            _ => return Err(syntax_error()),
                        }
                    }
                    fields.sort_by_key(|(k, _)| {
                        k.parse::<u32>()
                            .ok()
                            .filter(|n| *n != u32::MAX && n.to_string() == *k)
                            .map_or((1, 0), |n| (0, n))
                    });
                    Ok(V::Object(fields))
                }
                b'[' => {
                    self.at += 1;
                    let mut a = Vec::new();
                    self.ws();
                    if self.s.as_bytes().get(self.at) == Some(&b']') {
                        self.at += 1;
                        return Ok(V::Array(a));
                    }
                    loop {
                        a.push(self.value(d + 1)?);
                        self.ws();
                        match self.s.as_bytes().get(self.at) {
                            Some(b']') => {
                                self.at += 1;
                                break;
                            }
                            Some(b',') => self.at += 1,
                            _ => return Err(syntax_error()),
                        }
                    }
                    Ok(V::Array(a))
                }
                b't' | b'f' | b'n' => {
                    for (lit, v) in [
                        ("true", V::Bool(true)),
                        ("false", V::Bool(false)),
                        ("null", V::Null),
                    ] {
                        if self.s[self.at..].starts_with(lit) {
                            self.at += lit.len();
                            return Ok(v);
                        }
                    }
                    Err(syntax_error())
                }
                b'-' | b'0'..=b'9' => {
                    let start = self.at;
                    if b == b'-' {
                        self.at += 1;
                    }
                    let first = self
                        .s
                        .as_bytes()
                        .get(self.at)
                        .copied()
                        .ok_or_else(syntax_error)?;
                    if first == b'0' {
                        self.at += 1;
                    } else if (b'1'..=b'9').contains(&first) {
                        while self
                            .s
                            .as_bytes()
                            .get(self.at)
                            .is_some_and(u8::is_ascii_digit)
                        {
                            self.at += 1;
                        }
                    } else {
                        return Err(syntax_error());
                    }
                    if self.s.as_bytes().get(self.at) == Some(&b'.') {
                        self.at += 1;
                        let digit = self.at;
                        while self
                            .s
                            .as_bytes()
                            .get(self.at)
                            .is_some_and(u8::is_ascii_digit)
                        {
                            self.at += 1;
                        }
                        if self.at == digit {
                            return Err(syntax_error());
                        }
                    }
                    if self
                        .s
                        .as_bytes()
                        .get(self.at)
                        .is_some_and(|b| matches!(b, b'e' | b'E'))
                    {
                        self.at += 1;
                        if self
                            .s
                            .as_bytes()
                            .get(self.at)
                            .is_some_and(|b| matches!(b, b'+' | b'-'))
                        {
                            self.at += 1;
                        }
                        let digit = self.at;
                        while self
                            .s
                            .as_bytes()
                            .get(self.at)
                            .is_some_and(u8::is_ascii_digit)
                        {
                            self.at += 1;
                        }
                        if self.at == digit {
                            return Err(syntax_error());
                        }
                    }
                    Ok(V::number(
                        self.s[start..self.at]
                            .parse::<f64>()
                            .map_err(|_| syntax_error())?,
                    ))
                }
                _ => Err(syntax_error()),
            }
        }
    }
    let mut p = P { s, at: 0, nodes: 0 };
    let value = p.value(0)?;
    p.ws();
    if p.at != s.len() {
        return Err(syntax_error());
    }
    value.validate()?;
    domain(&value)?;
    Ok(value)
}
