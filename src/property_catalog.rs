//! Bounded, stateful port of the pinned property.js metadata owner.
//! Declared circulation is metadata; actual funded balances belong to the Ledger.
use crate::codec::{verify_crc, with_crc, Cursor};
use crate::issuance_intent::JsonValue;
use crate::validity_source::{
    DependencyCall, DependencyObservation, DependencyReturn, SourceBackend, SourceException,
    SourceOutcome, SourceRuleError, SourceValue,
};

#[path = "property_catalog_numeric.rs"]
mod numeric;
#[path = "property_catalog_codec.rs"]
mod value_codec;
use numeric::{js_number, js_text, quantize, supply_change};

pub const SOURCE_SHA256: &str = "ef0ee7ba5c362227a8bfd44192e351c107871a93a3dc93b04ce6d1a5ebfc70b1";
pub const MAX_ENTRIES: usize = 1024;
pub const MAX_HISTORY: usize = 1024;
pub const MAX_CHECKPOINT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_BACKEND_REPLY_BYTES: usize = 4096;
pub const METHODS: [&str; 18] = [
    "constructor",
    "getInstance",
    "load",
    "getNextPropertyId",
    "createToken",
    "addProperty",
    "inspectPropertyIndex",
    "getAMM",
    "isManagedAndAdmin",
    "updateTotalInCirculation",
    "save",
    "getPropertyIndex",
    "doesTickerExist",
    "isSyntheticToken",
    "grantTokens",
    "redeemTokens",
    "getPropertyData",
    "updateAdmin",
];
type V = SourceValue;
type Result<T> = core::result::Result<T, CatalogError>;

pub fn parse_diagnostic_value(text: &str) -> Result<V> {
    value_codec::decode(text)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    Capacity,
    InvalidInput,
    UnknownMethod,
    /// Functions, proxies, aliased reference graphs and unsupported numeric forms.
    UnsupportedDomain,
    Backend(SourceRuleError),
    InvalidCheckpoint,
}
impl core::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CatalogError {}
impl From<SourceRuleError> for CatalogError {
    fn from(e: SourceRuleError) -> Self {
        match e {
            SourceRuleError::Capacity => Self::Capacity,
            _ => Self::Backend(e),
        }
    }
}

/// An injected rejection at the zero-based ordered database operation position.
/// The underlying document remains the one actually owned by this catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseFailure {
    pub operation: usize,
    pub exception: SourceException,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogEffect {
    Database {
        operation: String,
        value: V,
        failure: Option<SourceException>,
    },
    Tally(DependencyObservation),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEvaluation {
    pub outcome: SourceOutcome,
    /// Constructor/getInstance return the same owned instance. SourceValue has no
    /// reference-valued Map graph, so its value is undefined and this flag is set.
    pub singleton_return: bool,
    pub effects: Vec<CatalogEffect>,
}
impl CatalogEvaluation {
    /// Stable diagnostic projection used by the unchanged-source corpus.
    pub fn diagnostic_value(&self) -> V {
        evaluation_value(self)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    document: Option<V>,
    cache: Vec<(V, V)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    ReplaceDocument(Option<V>),
    Call {
        method: String,
        args: Vec<V>,
        failures: Vec<DatabaseFailure>,
        evaluation: CatalogEvaluation,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyCatalog {
    initial_document: Option<V>,
    state: State,
    history: Vec<Event>,
}
impl Default for PropertyCatalog {
    fn default() -> Self {
        Self::new()
    }
}
impl PropertyCatalog {
    pub fn new() -> Self {
        Self::from_document(None).expect("empty catalog")
    }
    pub fn from_document(document: Option<V>) -> Result<Self> {
        validate_document(&document)?;
        let catalog = Self {
            initial_document: document.clone(),
            state: State {
                document,
                cache: Vec::new(),
            },
            history: Vec::new(),
        };
        catalog.checkpoint_bytes()?;
        Ok(catalog)
    }
    /// The singleton is scoped to this owned catalog, rather than a process global.
    pub fn get_instance(&mut self) -> &mut Self {
        self
    }
    pub fn document(&self) -> Option<&V> {
        self.state.document.as_ref()
    }
    pub fn cache_entries(&self) -> &[(V, V)] {
        &self.state.cache
    }
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
    /// Explicitly model an external database writer without refreshing the cache.
    pub fn replace_document(&mut self, document: Option<V>) -> Result<()> {
        validate_document(&document)?;
        self.reserve(estimate(&document)?)?;
        let mut candidate = self.clone();
        candidate.state.document = document.clone();
        candidate.history.push(Event::ReplaceDocument(document));
        candidate.checkpoint_bytes()?;
        *self = candidate;
        Ok(())
    }
    pub fn get_property_data(&mut self, id: V) -> Result<V> {
        self.query("getPropertyData", &[id])
    }
    pub fn is_managed_and_admin(&mut self, id: V, address: V) -> Result<V> {
        self.query("isManagedAndAdmin", &[id, address])
    }
    pub fn does_ticker_exist(&mut self, ticker: V) -> Result<V> {
        self.query("doesTickerExist", &[ticker])
    }
    fn query(&mut self, method: &str, args: &[V]) -> Result<V> {
        match self.execute(method, args, &mut NoTally)?.outcome {
            SourceOutcome::Returned(v) => Ok(v),
            SourceOutcome::Thrown(_) => Err(CatalogError::InvalidInput),
        }
    }
    pub fn execute<B: SourceBackend>(
        &mut self,
        method: &str,
        args: &[V],
        backend: &mut B,
    ) -> Result<CatalogEvaluation> {
        self.execute_with_faults(method, args, &[], backend)
    }
    pub fn execute_with_faults<B: SourceBackend>(
        &mut self,
        method: &str,
        args: &[V],
        failures: &[DatabaseFailure],
        backend: &mut B,
    ) -> Result<CatalogEvaluation> {
        self.execute_inner(method, args, failures, backend, false)
    }
    fn execute_inner<B: SourceBackend>(
        &mut self,
        method: &str,
        args: &[V],
        failures: &[DatabaseFailure],
        backend: &mut B,
        forecast: bool,
    ) -> Result<CatalogEvaluation> {
        if !METHODS.contains(&method) && !["quantize8", "getTallyMap"].contains(&method) {
            return Err(CatalogError::UnknownMethod);
        }
        if args.len() > 8 || failures.len() > 32 {
            return Err(CatalogError::Capacity);
        }
        for v in args {
            validate_value(v)?;
        }
        if failures
            .windows(2)
            .any(|p| p[0].operation >= p[1].operation)
        {
            return Err(CatalogError::InvalidInput);
        }
        for f in failures {
            validate_value(&V::String(f.exception.name.clone()))?;
            validate_value(&V::String(f.exception.message.clone()))?;
        }
        self.reserve(
            V::Array(args.to_vec())
                .diagnostic_json()?
                .len()
                .saturating_mul(8)
                .saturating_add(8192),
        )?;
        // Check deterministic metadata/serialization limits before launching a
        // real tally capability. The forecast performs no external calls.
        if !forecast && matches!(method, "grantTokens" | "redeemTokens") {
            let mut preview = self.clone();
            preview.execute_inner(method, args, failures, &mut ForecastTally, true)?;
            if preview
                .checkpoint_bytes()?
                .len()
                .saturating_add(MAX_BACKEND_REPLY_BYTES * 2)
                > MAX_CHECKPOINT_BYTES
            {
                return Err(CatalogError::Capacity);
            }
        }
        let mut next = self.state.clone();
        let mut engine = Engine {
            state: &mut next,
            backend,
            failures,
            db_operation: 0,
            effects: Vec::new(),
        };
        let result = engine.call(method, args);
        let effects = engine.effects;
        let outcome = match result {
            Ok(v) => SourceOutcome::Returned(v),
            Err(Fault::JsonSyntax) => return Err(CatalogError::UnsupportedDomain),
            Err(Fault::Source(e)) => SourceOutcome::Thrown(e),
            Err(Fault::Local(e)) => return Err(e),
        };
        validate_document(&next.document)?;
        if next.cache.len() > MAX_ENTRIES {
            return Err(CatalogError::Capacity);
        }
        let evaluation = CatalogEvaluation {
            outcome,
            singleton_return: matches!(method, "constructor" | "getInstance"),
            effects,
        };
        let mut candidate = self.clone();
        candidate.state = next;
        candidate.history.push(Event::Call {
            method: method.into(),
            args: args.to_vec(),
            failures: failures.to_vec(),
            evaluation: evaluation.clone(),
        });
        candidate.checkpoint_bytes()?;
        *self = candidate;
        Ok(evaluation)
    }
    fn reserve(&self, extra: usize) -> Result<()> {
        if self.history.len() >= MAX_HISTORY
            || self
                .checkpoint_bytes()?
                .len()
                .saturating_add(extra)
                .saturating_add(estimate(&self.state.document)?.saturating_mul(4))
                > MAX_CHECKPOINT_BYTES
        {
            return Err(CatalogError::Capacity);
        }
        Ok(())
    }
    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>> {
        let mut out = b"TLPC\x01".to_vec();
        put_value(&mut out, &option_value(&self.initial_document))?;
        put_count(&mut out, self.history.len())?;
        for event in &self.history {
            put_value(&mut out, &event_value(event))?;
        }
        put_value(&mut out, &state_value(&self.state))?;
        if out.len() + 4 > MAX_CHECKPOINT_BYTES {
            return Err(CatalogError::Capacity);
        }
        with_crc(out).map_err(|_| CatalogError::InvalidCheckpoint)
    }
    /// Re-executes the complete journal, including source exceptions and backend
    /// rejections, and verifies every ordered effect and the final document/cache.
    pub fn from_checkpoint_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(CatalogError::Capacity);
        }
        let payload = verify_crc(bytes).map_err(|_| CatalogError::InvalidCheckpoint)?;
        let mut reader = Cursor::new(payload);
        if reader
            .take(5)
            .map_err(|_| CatalogError::InvalidCheckpoint)?
            != b"TLPC\x01"
        {
            return Err(CatalogError::InvalidCheckpoint);
        }
        let initial = parse_option(read_value(&mut reader)?)?;
        let mut catalog = Self::from_document(initial)?;
        let count = reader.u32().map_err(|_| CatalogError::InvalidCheckpoint)? as usize;
        if count > MAX_HISTORY {
            return Err(CatalogError::InvalidCheckpoint);
        }
        for _ in 0..count {
            let expected = read_value(&mut reader)?;
            let parts = array(&expected)?;
            match parts.first() {
                Some(V::String(tag)) if tag == "document" && parts.len() == 2 => {
                    catalog.replace_document(parse_option(parts[1].clone())?)?
                }
                Some(V::String(tag)) if tag == "call" && parts.len() == 5 => {
                    let method = string(&parts[1])?;
                    let args = array(&parts[2])?;
                    let failures = parse_failures(&parts[3])?;
                    let evaluation = parse_evaluation(&parts[4])?;
                    let mut backend = ReplayBackend {
                        returns: evaluation
                            .effects
                            .iter()
                            .filter_map(|e| {
                                if let CatalogEffect::Tally(o) = e {
                                    Some(o.clone())
                                } else {
                                    None
                                }
                            })
                            .collect(),
                        at: 0,
                    };
                    let got = catalog.execute_with_faults(method, args, &failures, &mut backend)?;
                    if got != evaluation || backend.at != backend.returns.len() {
                        return Err(CatalogError::InvalidCheckpoint);
                    }
                }
                _ => return Err(CatalogError::InvalidCheckpoint),
            }
            if event_value(
                catalog
                    .history
                    .last()
                    .ok_or(CatalogError::InvalidCheckpoint)?,
            ) != expected
            {
                return Err(CatalogError::InvalidCheckpoint);
            }
        }
        if read_value(&mut reader)? != state_value(&catalog.state) {
            return Err(CatalogError::InvalidCheckpoint);
        }
        reader.end().map_err(|_| CatalogError::InvalidCheckpoint)?;
        if catalog.checkpoint_bytes()? != bytes {
            return Err(CatalogError::InvalidCheckpoint);
        }
        Ok(catalog)
    }
}
struct NoTally;
struct ForecastTally;
impl SourceBackend for ForecastTally {
    fn invoke(
        &mut self,
        _: &DependencyCall,
    ) -> core::result::Result<DependencyReturn, SourceRuleError> {
        Ok(DependencyReturn::Pending(V::Undefined))
    }
}
impl SourceBackend for NoTally {
    fn invoke(
        &mut self,
        _: &DependencyCall,
    ) -> core::result::Result<DependencyReturn, SourceRuleError> {
        Err(SourceRuleError::UnknownDependency)
    }
}
struct ReplayBackend {
    returns: Vec<DependencyObservation>,
    at: usize,
}
impl SourceBackend for ReplayBackend {
    fn invoke(
        &mut self,
        call: &DependencyCall,
    ) -> core::result::Result<DependencyReturn, SourceRuleError> {
        let value = self.returns.get(self.at).ok_or(SourceRuleError::Backend)?;
        if &value.call != call {
            return Err(SourceRuleError::Backend);
        }
        self.at += 1;
        Ok(value.returned.clone())
    }
}
#[derive(Debug)]
enum Fault {
    Source(SourceException),
    JsonSyntax,
    Local(CatalogError),
}
impl From<CatalogError> for Fault {
    fn from(e: CatalogError) -> Self {
        Self::Local(e)
    }
}
type Eval<T> = core::result::Result<T, Fault>;
fn exception(name: &str, message: impl Into<String>) -> SourceException {
    SourceException {
        name: name.into(),
        message: message.into(),
    }
}
fn throw(name: &str, message: impl Into<String>) -> Fault {
    Fault::Source(exception(name, message))
}
fn truth(v: &V) -> bool {
    match v {
        V::Undefined | V::Null | V::Bool(false) => false,
        V::Number(bits) => {
            let n = f64::from_bits(*bits);
            n != 0.0 && !n.is_nan()
        }
        V::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn strict(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Number(a), V::Number(b)) => f64::from_bits(*a) == f64::from_bits(*b),
        (V::String(a), V::String(b)) => a == b,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::Null, V::Null) | (V::Undefined, V::Undefined) => true,
        _ => false,
    }
}
fn same_key(a: &V, b: &V) -> bool {
    strict(a, b)
        || matches!((a,b),(V::Number(a),V::Number(b)) if f64::from_bits(*a).is_nan()&&f64::from_bits(*b).is_nan())
}
fn set_map(cache: &mut Vec<(V, V)>, mut key: V, value: V) {
    if let V::Number(bits) = &key {
        if f64::from_bits(*bits) == 0.0 {
            key = V::number(0.0)
        }
    }
    if let Some((_, old)) = cache.iter_mut().find(|(k, _)| same_key(k, &key)) {
        *old = value
    } else {
        cache.push((key, value))
    }
}
fn get(v: &V, key: &str) -> Eval<V> {
    if matches!(v, V::Null | V::Undefined) {
        return Err(throw(
            "TypeError",
            format!(
                "Cannot read properties of {} (reading '{key}')",
                js_text(v)?
            ),
        ));
    }
    Ok(v.get(key))
}
fn assign(v: &mut V, key: &str, value: V) -> Eval<()> {
    match v {
        V::Object(_) => v.insert(key, value).map_err(|e| Fault::Local(e.into())),
        V::Array(_) => Err(CatalogError::UnsupportedDomain.into()),
        V::Null | V::Undefined => Err(throw(
            "TypeError",
            format!("Cannot set properties of {} (setting '{key}')", js_text(v)?),
        )),
        _ => Err(throw(
            "TypeError",
            format!(
                "Cannot create property '{key}' on {} '{}'",
                match v {
                    V::Bool(_) => "boolean",
                    V::Number(_) => "number",
                    _ => "string",
                },
                js_text(v)?
            ),
        )),
    }
}
fn field_object(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn argument(args: &[V], i: usize) -> V {
    args.get(i).cloned().unwrap_or(V::Undefined)
}
struct Engine<'a, B: SourceBackend> {
    state: &'a mut State,
    backend: &'a mut B,
    failures: &'a [DatabaseFailure],
    db_operation: usize,
    effects: Vec<CatalogEffect>,
}
impl<B: SourceBackend> Engine<'_, B> {
    fn db(&mut self, operation: &str, value: V) -> Eval<()> {
        let failure = self
            .failures
            .iter()
            .find(|f| f.operation == self.db_operation)
            .map(|f| f.exception.clone());
        self.db_operation += 1;
        self.effects.push(CatalogEffect::Database {
            operation: operation.into(),
            value,
            failure: failure.clone(),
        });
        if let Some(e) = failure {
            Err(Fault::Source(e))
        } else {
            Ok(())
        }
    }
    fn read(&mut self) -> Eval<Option<V>> {
        self.db("getDatabase", V::String("propertyList".into()))?;
        self.db(
            "findOneAsync",
            field_object(&[("_id", V::String("propertyIndex".into()))]),
        )?;
        Ok(self.state.document.clone())
    }
    fn load(&mut self) -> Eval<V> {
        let read = self.read();
        let result = (|| {
            let value = read?
                .as_ref()
                .map(|d| d.get("value"))
                .unwrap_or(V::Undefined);
            if !truth(&value) {
                self.state.cache.clear();
                return Ok(V::Undefined);
            }
            let data = if let V::String(text) = value {
                parse_json(&text)?
            } else {
                value
            };
            let valid = matches!(&data,V::Array(a) if a.iter().all(|v|matches!(v,V::Array(p) if p.len()==2)));
            if valid {
                let V::Array(rows) = data else { unreachable!() };
                let mut cache = Vec::new();
                for row in rows {
                    let V::Array(p) = row else { unreachable!() };
                    set_map(&mut cache, p[0].clone(), p[1].clone());
                }
                self.state.cache = cache;
            } else {
                self.state.cache.clear()
            }
            Ok(V::Undefined)
        })();
        match result {
            Err(Fault::Source(_) | Fault::JsonSyntax) => Ok(V::Undefined),
            other => other,
        }
    }
    fn parsed_document(&mut self) -> Eval<Option<V>> {
        let doc = self.read()?;
        let value = doc.as_ref().map(|d| d.get("value")).unwrap_or(V::Undefined);
        if !truth(&value) {
            return Ok(None);
        }
        Ok(Some(json_parse_value(&value)?))
    }
    fn get_data_inner(&mut self, id: &V) -> Eval<V> {
        let Some(data) = self.parsed_document()? else {
            return Ok(V::Null);
        };
        let rows = find_rows(&data, "find")?;
        if matches!(id,V::String(s) if s.starts_with('s')) {
            for row in rows {
                if strict(&index(row, 0)?, id) {
                    return index(row, 1);
                }
            }
        }
        let target = js_number(id)?;
        for row in rows {
            if js_number(&index(row, 0)?)? == target {
                return index(row, 1);
            }
        }
        Ok(V::Null)
    }
    fn get_data(&mut self, id: &V) -> Eval<V> {
        match self.get_data_inner(id) {
            Err(Fault::Source(_) | Fault::JsonSyntax) => Ok(V::Null),
            other => other,
        }
    }
    fn save(&mut self) -> Eval<V> {
        let rows = V::Array(
            self.state
                .cache
                .iter()
                .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                .collect(),
        );
        let text = json_stringify(&rows)?;
        let doc = field_object(&[
            ("_id", V::String("propertyIndex".into())),
            ("value", V::String(text)),
        ]);
        self.db("getDatabase", V::String("propertyList".into()))?;
        self.db(
            "updateAsync",
            V::Array(vec![
                field_object(&[("_id", V::String("propertyIndex".into()))]),
                doc.clone(),
                field_object(&[("upsert", V::Bool(true))]),
            ]),
        )?;
        self.state.document = Some(doc);
        Ok(V::Undefined)
    }
    fn next_id(&mut self) -> Eval<V> {
        self.load()?;
        let mut max = 0.0f64;
        for (key, _) in &self.state.cache {
            let number = js_number(key)?;
            max = if max.is_nan() || number.is_nan() {
                f64::NAN
            } else {
                max.max(number)
            }
        }
        Ok(V::number(max + 1.0))
    }
    fn add(&mut self, args: &[V]) -> Eval<V> {
        let id = argument(args, 0);
        let ticker = argument(args, 1);
        let supply = argument(args, 2);
        let kind = argument(args, 3);
        let type_name = js_text(&kind)?;
        let type_code = match type_name.as_str() {
            "Fixed" => 1,
            "Managed" => 2,
            "Native" => 3,
            "Vesting" => 4,
            "Synthetic" => 5,
            "Non-Fungible" => 6,
            "Procedural" => 7,
            "constructor"
            | "toString"
            | "toLocaleString"
            | "valueOf"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "__defineGetter__"
            | "__defineSetter__"
            | "__lookupGetter__"
            | "__lookupSetter__"
            | "__proto__" => return Err(CatalogError::UnsupportedDomain.into()),
            _ => return Err(throw("Error", "Invalid property type.")),
        };
        let meta = match args.get(7) {
            Some(V::Undefined) | None => V::object(),
            Some(v) => v.clone(),
        };
        let existing = self
            .state
            .cache
            .iter()
            .position(|(k, v)| same_key(k, &id) && truth(v));
        let mut property = existing.map_or_else(V::object, |i| self.state.cache[i].1.clone());
        let update = (|| {
            if existing.is_some() {
                let total = supply_change(&get(&property, "totalInCirculation")?, &supply, false)?;
                assign(&mut property, "totalInCirculation", V::number(total))?;
                let old = get(&property, "ticker")?;
                assign(
                    &mut property,
                    "ticker",
                    if truth(&ticker) { ticker.clone() } else { old },
                )?;
                assign(&mut property, "type", V::number(type_code as f64))?;
                for (field, i) in [("whitelistId", 4), ("issuer", 5), ("backupAddress", 6)] {
                    let value = argument(args, i);
                    let old = get(&property, field)?;
                    assign(
                        &mut property,
                        field,
                        if truth(&value) { value } else { old },
                    )?;
                }
                let procedural = get(&meta, "proceduralType")?;
                if !matches!(procedural, V::Undefined | V::Null) {
                    assign(
                        &mut property,
                        "proceduralType",
                        V::number(js_number(&procedural)?),
                    )?
                }
            } else {
                // Literal evaluation quantizes before reading meta.proceduralType.
                let supply = V::number(quantize(&supply)?);
                let procedural = get(&meta, "proceduralType")?;
                let procedural = if matches!(procedural, V::Undefined | V::Null) {
                    V::Undefined
                } else {
                    V::number(js_number(&procedural)?)
                };
                property = field_object(&[
                    ("ticker", ticker.clone()),
                    ("totalInCirculation", supply),
                    ("type", V::number(type_code as f64)),
                    ("whitelistId", argument(args, 4)),
                    ("issuer", argument(args, 5)),
                    ("backupAddress", argument(args, 6)),
                    ("proceduralType", procedural),
                ]);
            }
            Ok(())
        })();
        if let Err(error) = update {
            // An existing object is a reference into the singleton Map. Earlier
            // assignments survive a later source exception (for example null meta).
            if let (Some(i), Fault::Source(_)) = (existing, &error) {
                self.state.cache[i].1 = property
            }
            return Err(error);
        }
        // Source evaluates a discarded blob too, including its second quantize.
        quantize(&supply)?;
        if self.state.cache.len() >= MAX_ENTRIES
            && !self.state.cache.iter().any(|(k, _)| same_key(k, &id))
        {
            return Err(CatalogError::Capacity.into());
        }
        set_map(&mut self.state.cache, id, property);
        self.save()
    }
    fn call(&mut self, method: &str, args: &[V]) -> Eval<V> {
        let id = argument(args, 0);
        match method {
            "constructor" | "getInstance" => Ok(V::Undefined),
            "load" => self.load(),
            "getNextPropertyId" => self.next_id(),
            "save" => self.save(),
            "quantize8" => Ok(V::number(quantize(&id)?)),
            "getTallyMap" => Err(CatalogError::UnsupportedDomain.into()),
            "getAMM" => Err(throw("ReferenceError", "PropertyRegistry is not defined")),
            "isSyntheticToken" => {
                self.load()?;
                Err(throw(
                    "TypeError",
                    "Cannot read properties of undefined (reading 'get')",
                ))
            }
            "inspectPropertyIndex" => {
                self.load()?;
                Ok(V::Undefined)
            }
            "getPropertyData" => self.get_data(&id),
            "getPropertyIndex" => {
                if self.state.cache.is_empty() {
                    self.load()?;
                }
                self.state
                    .cache
                    .iter()
                    .map(|(id, p)| {
                        Ok(field_object(&[
                            ("id", id.clone()),
                            ("ticker", get(p, "ticker")?),
                            ("totalInCirculation", get(p, "totalInCirculation")?),
                            ("type", get(p, "type")?),
                        ]))
                    })
                    .collect::<Eval<Vec<_>>>()
                    .map(V::Array)
            }
            "addProperty" => self.add(args),
            "createToken" => {
                if self.state.cache.iter().any(|(k, _)| same_key(k, &id)) {
                    return Err(throw("TypeError", "ticker is not a constructor"));
                }
                for (_, p) in &self.state.cache {
                    if strict(&get(p, "ticker")?, &id) {
                        return Ok(field_object(&[
                            ("name", V::String("Error".into())),
                            (
                                "message",
                                V::String(format!("Ticker \"{}\" already exists.", js_text(&id)?)),
                            ),
                        ]));
                    }
                }
                let next = self.next_id()?;
                let mut add = vec![next.clone()];
                add.extend_from_slice(args);
                self.add(&add)?;
                Ok(next)
            }
            "doesTickerExist" => {
                let Some(doc) = self.read()? else {
                    return Ok(V::Bool(false));
                };
                if !truth(&doc) {
                    return Ok(V::Bool(false));
                }
                let parsed = json_parse_value(&doc.get("value"))?;
                let V::Array(rows) = parsed else {
                    return Err(throw("TypeError", "parsedIndex is not iterable"));
                };
                for row in rows {
                    let V::Array(p) = row else {
                        return Err(CatalogError::UnsupportedDomain.into());
                    };
                    if strict(&get(p.get(1).unwrap_or(&V::Undefined), "ticker")?, &id) {
                        return Ok(V::Bool(true));
                    }
                }
                Ok(V::Bool(false))
            }
            "isManagedAndAdmin" => {
                let result = (|| {
                    let Some(parsed) = self.parsed_document()? else {
                        return Ok(V::Bool(false));
                    };
                    for row in find_rows(&parsed, "find")? {
                        if strict(&index(row, 0)?, &id) {
                            let p = index(row, 1)?;
                            let kind = get(&p, "type")?;
                            return Ok(V::Bool(
                                (strict(&kind, &V::number(2.0)) || strict(&kind, &V::number(7.0)))
                                    && strict(&get(&p, "issuer")?, &argument(args, 1)),
                            ));
                        }
                    }
                    Ok(V::Bool(false))
                })();
                match result {
                    Err(Fault::Source(_) | Fault::JsonSyntax) => Ok(V::Bool(false)),
                    other => other,
                }
            }
            "updateTotalInCirculation" => {
                let mut property = self.get_data(&id)?;
                if !truth(&property) {
                    return Err(throw("Error", "Property not found"));
                }
                let supply = supply_change(
                    &get(&property, "totalInCirculation")?,
                    &argument(args, 1),
                    false,
                )?;
                assign(&mut property, "totalInCirculation", V::number(supply))?;
                let doc = self.read()?.unwrap_or(V::Null);
                let parsed = json_parse_value(&get(&doc, "value")?)?;
                let mut found = false;
                for row in find_rows(&parsed, "find")? {
                    if strict(&index(row, 0)?, &id) {
                        found = true;
                        break;
                    }
                }
                if found {
                    // An injected rejection from this deliberately unawaited
                    // async factory would also create an unhandled rejection.
                    // That process-level scheduling domain is not repaired here.
                    if self
                        .failures
                        .iter()
                        .any(|f| f.operation == self.db_operation)
                    {
                        return Err(CatalogError::UnsupportedDomain.into());
                    }
                    self.db("getDatabase", V::String("propertyList".into()))?;
                    Err(throw("TypeError", "base.updateAsync is not a function"))
                } else {
                    Err(throw(
                        "Error",
                        "Failed to update totalInCirculation, property not found in index",
                    ))
                }
            }
            "updateAdmin" => {
                let Some(mut parsed) = self.parsed_document()? else {
                    return Err(throw("Error", "Property index not found."));
                };
                let rows = find_rows(&parsed, "findIndex")?;
                let target = js_number(&id)?;
                let mut found = None;
                for (i, row) in rows.iter().enumerate() {
                    if js_number(&index(row, 0)?)? == target {
                        found = Some(i);
                        break;
                    }
                }
                let Some(found) = found else {
                    return Err(throw(
                        "Error",
                        format!("Property with ID {} does not exist.", js_text(&id)?),
                    ));
                };
                let V::Array(rows) = &mut parsed else {
                    unreachable!()
                };
                let V::Array(pair) = &mut rows[found] else {
                    return Err(CatalogError::UnsupportedDomain.into());
                };
                let field = if truth(&argument(args, 2)) {
                    "backupAddress"
                } else {
                    "issuer"
                };
                if pair.len() < 2 {
                    return Err(CatalogError::UnsupportedDomain.into());
                }
                assign(&mut pair[1], field, argument(args, 1))?;
                let text = json_stringify(&parsed)?;
                let update =
                    field_object(&[("$set", field_object(&[("value", V::String(text.clone()))]))]);
                self.db(
                    "updateAsync",
                    V::Array(vec![
                        field_object(&[("_id", V::String("propertyIndex".into()))]),
                        update,
                        field_object(&[("upsert", V::Bool(true))]),
                    ]),
                )?;
                let doc = self.state.document.get_or_insert_with(V::object);
                assign(doc, "value", V::String(text))?;
                Ok(V::Undefined)
            }
            "grantTokens" | "redeemTokens" => {
                let mut property = self.get_data(&id)?;
                if !truth(&property) {
                    return Err(throw(
                        "Error",
                        format!("Property with ID {} not found.", js_text(&id)?),
                    ));
                }
                let amount = argument(args, 2);
                let redeem = method == "redeemTokens";
                if redeem && numeric::less(&get(&property, "totalInCirculation")?, &amount)? {
                    return Err(throw(
                        "Error",
                        format!(
                            "Insufficient managed tokens for redemption for property {}.",
                            js_text(&id)?
                        ),
                    ));
                }
                let total = supply_change(&get(&property, "totalInCirculation")?, &amount, redeem)?;
                assign(&mut property, "totalInCirculation", V::number(total))?;
                self.load()?;
                set_map(&mut self.state.cache, V::number(js_number(&id)?), property);
                let call = DependencyCall {
                    target: "Tally.updateBalance".into(),
                    arguments: vec![
                        argument(args, 1),
                        id,
                        if redeem {
                            V::number(-js_number(&amount)?)
                        } else {
                            amount
                        },
                        V::number(0.0),
                        V::number(0.0),
                        V::number(0.0),
                        V::String(if redeem { "redeemToken" } else { "grantToken" }.into()),
                        argument(args, 3),
                    ],
                };
                let returned = self
                    .backend
                    .invoke(&call)
                    .map_err(|e| Fault::Local(CatalogError::Backend(e)))?;
                match &returned {
                    DependencyReturn::Immediate(value) | DependencyReturn::Pending(value) => {
                        validate_value(value)?
                    }
                    DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error) => {
                        validate_value(&V::String(error.name.clone()))?;
                        validate_value(&V::String(error.message.clone()))?;
                    }
                }
                if return_value(&returned)
                    .diagnostic_json()
                    .map_err(CatalogError::from)?
                    .len()
                    > MAX_BACKEND_REPLY_BYTES
                {
                    return Err(CatalogError::Capacity.into());
                }
                self.effects
                    .push(CatalogEffect::Tally(DependencyObservation {
                        call,
                        returned: returned.clone(),
                    }));
                match returned {
                    DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                        return Err(Fault::Source(e))
                    }
                    _ => {}
                }
                self.save()
            }
            _ => Err(CatalogError::UnknownMethod.into()),
        }
    }
}
fn validate_value(value: &V) -> Result<()> {
    value.validate()?;
    fn walk(v: &V) -> Result<()> {
        match v {
            V::Promise | V::BigNumber(_) => Err(CatalogError::UnsupportedDomain),
            V::Number(bits) if f64::from_bits(*bits).is_nan() && *bits != f64::NAN.to_bits() => {
                Err(CatalogError::UnsupportedDomain)
            }
            V::Object(fields) => {
                if fields.iter().any(|(k, value)| {
                    ["toJSON", "toString", "valueOf"].contains(&k.as_str())
                        || (k == "_isBigNumber" && value == &V::Bool(true))
                }) {
                    return Err(CatalogError::UnsupportedDomain);
                }
                for (_, v) in fields {
                    walk(v)?
                }
                Ok(())
            }
            V::Array(values) => {
                for v in values {
                    walk(v)?
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    walk(value)
}
fn validate_document(document: &Option<V>) -> Result<()> {
    if let Some(v) = document {
        validate_value(v)?;
    }
    Ok(())
}
fn estimate(document: &Option<V>) -> Result<usize> {
    Ok(match document {
        Some(v) => v.diagnostic_json()?.len(),
        None => 0,
    })
}
fn parse_json(text: &str) -> Eval<V> {
    match JsonValue::parse(text) {
        Ok(value) => {
            let value = V::from_json(&value).map_err(|e| Fault::Local(e.into()))?;
            validate_value(&value)?;
            Ok(value)
        }
        // Engine-specific SyntaxError text changes between JS runtimes. Catching
        // source methods swallow this marker; an uncaught marker is unsupported.
        Err(
            crate::issuance_intent::IntentError::Capacity
            | crate::issuance_intent::IntentError::DepthLimit,
        ) => Err(CatalogError::Capacity.into()),
        Err(
            crate::issuance_intent::IntentError::UnsupportedSurrogate
            | crate::issuance_intent::IntentError::InvalidNumber,
        ) => Err(CatalogError::UnsupportedDomain.into()),
        Err(_) => Err(Fault::JsonSyntax),
    }
}
fn json_parse_value(value: &V) -> Eval<V> {
    parse_json(&js_text(value)?)
}
fn find_rows<'a>(data: &'a V, method: &str) -> Eval<&'a [V]> {
    match data {
        V::Array(rows) => Ok(rows),
        V::Null | V::Undefined => Err(throw(
            "TypeError",
            format!(
                "Cannot read properties of {} (reading '{method}')",
                js_text(data)?
            ),
        )),
        _ => Err(throw(
            "TypeError",
            format!("parsedData.{method} is not a function"),
        )),
    }
}
fn index(value: &V, index: usize) -> Eval<V> {
    match value {
        V::Array(values) => Ok(values.get(index).cloned().unwrap_or(V::Undefined)),
        V::String(text) if text.chars().any(|c| c as u32 > 0xffff) => {
            Err(CatalogError::UnsupportedDomain.into())
        }
        V::String(text) => Ok(text
            .chars()
            .nth(index)
            .map(|c| V::String(c.to_string()))
            .unwrap_or(V::Undefined)),
        V::Null | V::Undefined => Err(throw(
            "TypeError",
            format!(
                "Cannot read properties of {} (reading '{index}')",
                js_text(value)?
            ),
        )),
        V::Object(_) => Ok(value.get(&index.to_string())),
        _ => Ok(V::Undefined),
    }
}
/// JSON.stringify over the bounded SourceValue domain. Undefined object fields
/// disappear, array holes/nonfinite numbers become null, and integer keys sort.
pub fn json_stringify(value: &V) -> Result<String> {
    fn quote(s: &str) -> Result<String> {
        JsonValue::String(s.into())
            .to_json()
            .map_err(|_| CatalogError::Capacity)
    }
    fn emit(v: &V, out: &mut String) -> Result<()> {
        match v {
            V::Undefined | V::Null => out.push_str("null"),
            V::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            V::Number(bits) => {
                let n = f64::from_bits(*bits);
                if n.is_finite() {
                    out.push_str(
                        &crate::state_oracle::source_number_text(n)
                            .map_err(|_| CatalogError::UnsupportedDomain)?,
                    )
                } else {
                    out.push_str("null")
                }
            }
            V::String(s) => out.push_str(&quote(s)?),
            V::Array(values) => {
                out.push('[');
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        out.push(',')
                    }
                    emit(v, out)?
                }
                out.push(']')
            }
            V::Object(fields) => {
                let mut sorted: Vec<_> = fields
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, v))| !matches!(v, V::Undefined))
                    .collect();
                let integer = |key: &str| {
                    key.parse::<u32>()
                        .ok()
                        .filter(|n| *n != u32::MAX && n.to_string() == key)
                };
                sorted.sort_by_key(|(i, (key, _))| match integer(key) {
                    Some(n) => (0, n as usize),
                    None => (1, *i),
                });
                out.push('{');
                for (i, (_, (key, v))) in sorted.into_iter().enumerate() {
                    if i > 0 {
                        out.push(',')
                    }
                    out.push_str(&quote(key)?);
                    out.push(':');
                    emit(v, out)?
                }
                out.push('}')
            }
            _ => return Err(CatalogError::UnsupportedDomain),
        }
        if out.len() > crate::issuance_intent::MAX_JSON_BYTES {
            return Err(CatalogError::Capacity);
        }
        Ok(())
    }
    let mut out = String::new();
    emit(value, &mut out)?;
    Ok(out)
}
fn option_value(value: &Option<V>) -> V {
    V::Array(value.iter().cloned().collect())
}
fn parse_option(value: V) -> Result<Option<V>> {
    match value {
        V::Array(mut a) if a.len() <= 1 => Ok(a.pop()),
        _ => Err(CatalogError::InvalidCheckpoint),
    }
}
fn state_value(state: &State) -> V {
    V::Array(vec![
        option_value(&state.document),
        V::Array(
            state
                .cache
                .iter()
                .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                .collect(),
        ),
    ])
}
fn exception_value(e: &SourceException) -> V {
    V::Array(vec![
        V::String(e.name.clone()),
        V::String(e.message.clone()),
    ])
}
fn parse_exception(v: &V) -> Result<SourceException> {
    let a = array(v)?;
    if a.len() != 2 {
        return Err(CatalogError::InvalidCheckpoint);
    }
    Ok(exception(string(&a[0])?, string(&a[1])?))
}
fn return_value(v: &DependencyReturn) -> V {
    let (tag, value) = match v {
        DependencyReturn::Immediate(v) => ("immediate", v.clone()),
        DependencyReturn::Pending(v) => ("pending", v.clone()),
        DependencyReturn::Throw(e) => ("throw", exception_value(e)),
        DependencyReturn::PendingThrow(e) => ("pendingThrow", exception_value(e)),
    };
    V::Array(vec![V::String(tag.into()), value])
}
fn parse_return(v: &V) -> Result<DependencyReturn> {
    let a = array(v)?;
    if a.len() != 2 {
        return Err(CatalogError::InvalidCheckpoint);
    }
    Ok(match string(&a[0])? {
        "immediate" => DependencyReturn::Immediate(a[1].clone()),
        "pending" => DependencyReturn::Pending(a[1].clone()),
        "throw" => DependencyReturn::Throw(parse_exception(&a[1])?),
        "pendingThrow" => DependencyReturn::PendingThrow(parse_exception(&a[1])?),
        _ => return Err(CatalogError::InvalidCheckpoint),
    })
}
fn evaluation_value(e: &CatalogEvaluation) -> V {
    let outcome = match &e.outcome {
        SourceOutcome::Returned(v) => V::Array(vec![V::String("return".into()), v.clone()]),
        SourceOutcome::Thrown(x) => V::Array(vec![V::String("throw".into()), exception_value(x)]),
    };
    let effects = e
        .effects
        .iter()
        .map(|effect| match effect {
            CatalogEffect::Database {
                operation,
                value,
                failure,
            } => V::Array(vec![
                V::String("db".into()),
                V::String(operation.clone()),
                value.clone(),
                V::Array(failure.iter().map(exception_value).collect()),
            ]),
            CatalogEffect::Tally(o) => V::Array(vec![
                V::String("tally".into()),
                V::String(o.call.target.clone()),
                V::Array(o.call.arguments.clone()),
                return_value(&o.returned),
            ]),
        })
        .collect();
    V::Array(vec![
        outcome,
        V::Bool(e.singleton_return),
        V::Array(effects),
    ])
}
fn parse_evaluation(value: &V) -> Result<CatalogEvaluation> {
    let a = array(value)?;
    if a.len() != 3 {
        return Err(CatalogError::InvalidCheckpoint);
    }
    let o = array(&a[0])?;
    if o.len() != 2 {
        return Err(CatalogError::InvalidCheckpoint);
    }
    let outcome = match string(&o[0])? {
        "return" => SourceOutcome::Returned(o[1].clone()),
        "throw" => SourceOutcome::Thrown(parse_exception(&o[1])?),
        _ => return Err(CatalogError::InvalidCheckpoint),
    };
    let V::Bool(singleton_return) = a[1] else {
        return Err(CatalogError::InvalidCheckpoint);
    };
    let mut effects = Vec::new();
    for value in array(&a[2])? {
        let p = array(value)?;
        if p.len() != 4 {
            return Err(CatalogError::InvalidCheckpoint);
        }
        effects.push(match string(&p[0])? {
            "db" => {
                let f = array(&p[3])?;
                if f.len() > 1 {
                    return Err(CatalogError::InvalidCheckpoint);
                }
                CatalogEffect::Database {
                    operation: string(&p[1])?.into(),
                    value: p[2].clone(),
                    failure: f.first().map(parse_exception).transpose()?,
                }
            }
            "tally" => CatalogEffect::Tally(DependencyObservation {
                call: DependencyCall {
                    target: string(&p[1])?.into(),
                    arguments: array(&p[2])?.to_vec(),
                },
                returned: parse_return(&p[3])?,
            }),
            _ => return Err(CatalogError::InvalidCheckpoint),
        });
    }
    Ok(CatalogEvaluation {
        outcome,
        singleton_return,
        effects,
    })
}
fn event_value(event: &Event) -> V {
    match event {
        Event::ReplaceDocument(doc) => {
            V::Array(vec![V::String("document".into()), option_value(doc)])
        }
        Event::Call {
            method,
            args,
            failures,
            evaluation,
        } => V::Array(vec![
            V::String("call".into()),
            V::String(method.clone()),
            V::Array(args.clone()),
            V::Array(
                failures
                    .iter()
                    .map(|f| {
                        V::Array(vec![
                            V::String(f.operation.to_string()),
                            exception_value(&f.exception),
                        ])
                    })
                    .collect(),
            ),
            evaluation_value(evaluation),
        ]),
    }
}
fn parse_failures(value: &V) -> Result<Vec<DatabaseFailure>> {
    array(value)?
        .iter()
        .map(|v| {
            let a = array(v)?;
            if a.len() != 2 {
                return Err(CatalogError::InvalidCheckpoint);
            }
            Ok(DatabaseFailure {
                operation: string(&a[0])?
                    .parse()
                    .map_err(|_| CatalogError::InvalidCheckpoint)?,
                exception: parse_exception(&a[1])?,
            })
        })
        .collect()
}
fn array(value: &V) -> Result<&[V]> {
    if let V::Array(a) = value {
        Ok(a)
    } else {
        Err(CatalogError::InvalidCheckpoint)
    }
}
fn string(value: &V) -> Result<&str> {
    if let V::String(a) = value {
        Ok(a)
    } else {
        Err(CatalogError::InvalidCheckpoint)
    }
}
fn put_count(bytes: &mut Vec<u8>, count: usize) -> Result<()> {
    bytes.extend_from_slice(
        &u32::try_from(count)
            .map_err(|_| CatalogError::Capacity)?
            .to_be_bytes(),
    );
    Ok(())
}
fn put_value(bytes: &mut Vec<u8>, value: &V) -> Result<()> {
    let text = value.diagnostic_json()?;
    put_count(bytes, text.len())?;
    bytes.extend_from_slice(text.as_bytes());
    if bytes.len() > MAX_CHECKPOINT_BYTES {
        return Err(CatalogError::Capacity);
    }
    Ok(())
}
fn read_value(reader: &mut Cursor<'_>) -> Result<V> {
    let n = reader.u32().map_err(|_| CatalogError::InvalidCheckpoint)? as usize;
    if n > MAX_CHECKPOINT_BYTES {
        return Err(CatalogError::InvalidCheckpoint);
    }
    let s = core::str::from_utf8(
        reader
            .take(n)
            .map_err(|_| CatalogError::InvalidCheckpoint)?,
    )
    .map_err(|_| CatalogError::InvalidCheckpoint)?;
    parse_diagnostic_value(s).map_err(|_| CatalogError::InvalidCheckpoint)
}
