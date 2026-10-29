//! Literal pinned txIndex.js metadata lifecycle over a single external DB/RPC
//! authority. This owner supplies neither chain acceptance nor financial state.
use crate::codec::{verify_crc, with_crc};
use crate::types_source::{decode_payload_unverified, TypesOutcome};
use crate::validity_source::{
    DependencyCall, DependencyObservation, DependencyReturn, SourceBackend, SourceCall,
    SourceEvaluation, SourceException, SourceOutcome, SourceRuleError, SourceValue as V,
};

use crate::source_index_metadata_algorithms::{SourceIndexMetadataIo, SourceIndexMetadataMethod};

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "095449d90f511df44aff7b316105f57d4c0508ea8fb0b6f9c4a5f358edf6ca51";
pub const MAX_BLOCKS: usize = 1024;
pub const MAX_DEPENDENCIES: usize = 4096;
pub const MAX_TRACE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_STATE_BYTES: usize = 2 * 1024 * 1024;
pub const STATIC_METHODS: [&str; 18] = [
    "init",
    "getInstance",
    "initializeIndex",
    "extractBlockData",
    "saveMaxHeight",
    "fetchChainTip",
    "fetchBlockData",
    "processBlockData",
    "fetchTransactionData",
    "DecodeRawTransaction",
    "isPrintableASCII",
    "processTransaction",
    "upsertTxValidityAndReason",
    "clearTxIndex",
    "resetIndexFlag",
    "findMaxIndexedBlock",
    "getTransactionData",
    "checkForIndex",
];
pub const INSTANCE_METHODS: [&str; 3] = ["saveTransactionData", "loadIndex", "initializeOrLoadDB"];

type R<T> = Result<T, Fault>;
#[derive(Debug)]
enum Fault {
    Source(SourceException),
    Local(SourceRuleError),
}
impl From<SourceRuleError> for Fault {
    fn from(e: SourceRuleError) -> Self {
        Self::Local(e)
    }
}
fn thrown(name: &str, message: impl Into<String>) -> Fault {
    Fault::Source(SourceException {
        name: name.into(),
        message: message.into(),
    })
}
fn ty(message: impl Into<String>) -> Fault {
    thrown("TypeError", message)
}
fn s(v: impl Into<String>) -> V {
    V::String(v.into())
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn arg(a: &[V], i: usize) -> V {
    a.get(i).cloned().unwrap_or(V::Undefined)
}
fn truth(v: &V) -> bool {
    match v {
        V::Undefined | V::Null | V::Bool(false) => false,
        V::Number(b) => f64::from_bits(*b) != 0.0 && !f64::from_bits(*b).is_nan(),
        V::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn prop(v: &V, key: &str) -> R<V> {
    if matches!(v, V::Null | V::Undefined) {
        return Err(ty(format!(
            "Cannot read properties of {} (reading '{key}')",
            text(v)?
        )));
    }
    if let Ok(i) = key.parse::<usize>() {
        if i.to_string() == key {
            match v {
                V::Array(a) => return Ok(a.get(i).cloned().unwrap_or(V::Undefined)),
                V::String(t) => {
                    return t.encode_utf16().nth(i).map_or(Ok(V::Undefined), |unit| {
                        String::from_utf16(&[unit])
                            .map(s)
                            .map_err(|_| SourceRuleError::UnsupportedNumericDomain.into())
                    })
                }
                _ => (),
            }
        }
    }
    Ok(v.get(key))
}
fn text(v: &V) -> R<String> {
    Ok(match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::String(s) => s.clone(),
        V::Promise => "[object Promise]".into(),
        V::BigNumber(_) => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
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
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
            }
        }
        V::Array(a) => a
            .iter()
            .map(|v| {
                if matches!(v, V::Undefined | V::Null) {
                    Ok(String::new())
                } else {
                    text(v)
                }
            })
            .collect::<R<Vec<_>>>()?
            .join(","),
        V::Object(fields) => {
            if fields.iter().any(|(k, _)| k == "toString") {
                return Err(ty("Cannot convert object to primitive value"));
            }
            "[object Object]".into()
        }
    })
}
fn number(v: &V) -> R<f64> {
    Ok(match v {
        V::Undefined | V::Promise => f64::NAN,
        V::Null => 0.0,
        V::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        V::Number(b) => f64::from_bits(*b),
        V::String(t) => {
            let t = t.trim_matches(js_space);
            if t.is_empty() {
                0.0
            } else if t == "Infinity" || t == "+Infinity" {
                f64::INFINITY
            } else if t == "-Infinity" {
                f64::NEG_INFINITY
            } else if t.starts_with("0x") || t.starts_with("0X") {
                radix(&t[2..], 16)?
            } else if t.starts_with("0b") || t.starts_with("0B") {
                radix(&t[2..], 2)?
            } else if t.starts_with("0o") || t.starts_with("0O") {
                radix(&t[2..], 8)?
            } else if t
                .bytes()
                .all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b))
            {
                t.parse().unwrap_or(f64::NAN)
            } else {
                f64::NAN
            }
        }
        V::Array(_) | V::Object(_) => number(&s(text(v)?))?,
        V::BigNumber(_) => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
    })
}
fn js_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn radix(t: &str, base: u32) -> R<f64> {
    if t.is_empty() || !t.chars().all(|c| c.is_digit(base)) {
        return Ok(f64::NAN);
    }
    let mut n = crate::decimal::Decimal::zero();
    for c in t.trim_start_matches('0').chars() {
        n = n
            .checked_mul(&crate::decimal::Decimal::from(base))
            .and_then(|v| {
                v.checked_add(&crate::decimal::Decimal::from(
                    c.to_digit(base).unwrap_or(0),
                ))
            })
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    }
    Ok(n.to_string().parse().unwrap_or(f64::INFINITY))
}
fn compare(a: &V, b: &V) -> R<Option<core::cmp::Ordering>> {
    let a = match a {
        V::Array(_) | V::Object(_) => s(text(a)?),
        _ => a.clone(),
    };
    let b = match b {
        V::Array(_) | V::Object(_) => s(text(b)?),
        _ => b.clone(),
    };
    if let (V::String(a), V::String(b)) = (&a, &b) {
        return Ok(Some(a.encode_utf16().cmp(b.encode_utf16())));
    }
    Ok(number(&a)?.partial_cmp(&number(&b)?))
}
fn exception_value(e: SourceException) -> V {
    obj(&[(
        "$error",
        obj(&[("name", s(e.name)), ("message", s(e.message))]),
    )])
}
fn returned_error(message: impl Into<String>) -> V {
    exception_value(SourceException {
        name: "Error".into(),
        message: message.into(),
    })
}
fn handle() -> V {
    obj(&[("$txIndexInstance", V::number(1.0))])
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxIndexSourceOwner {
    singleton: bool,
    class_client: V,
    class_parse_block: V,
    txutils_client: V,
    forward: V,
    module_loaded: bool,
    client_rejection: Option<SourceException>,
}
impl Default for TxIndexSourceOwner {
    fn default() -> Self {
        Self {
            singleton: false,
            class_client: V::Undefined,
            class_parse_block: V::Undefined,
            txutils_client: V::Undefined,
            forward: V::Undefined,
            module_loaded: false,
            client_rejection: None,
        }
    }
}
#[derive(Default)]
struct Frame {
    observations: Vec<DependencyObservation>,
    deferred: Vec<SourceException>,
    trace_bytes: usize,
}
struct TypesBackend<'a, B> {
    frame: &'a mut Frame,
    backend: &'a mut B,
}
impl<B: SourceBackend> SourceBackend for TypesBackend<'_, B> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let returned = self.backend.invoke(call)?;
        self.frame.observe(call.clone(), returned.clone())?;
        Ok(returned)
    }
}
fn value_bytes(v: &V) -> usize {
    match v {
        V::String(t) | V::BigNumber(t) => t.len() + 1,
        V::Array(a) => 1 + a.iter().map(value_bytes).sum::<usize>(),
        V::Object(f) => {
            1 + f
                .iter()
                .map(|(k, v)| k.len() + value_bytes(v) + 1)
                .sum::<usize>()
        }
        _ => 9,
    }
}
fn return_bytes(v: &DependencyReturn) -> usize {
    match v {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => value_bytes(v),
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
            e.name.len() + e.message.len() + 2
        }
    }
}
impl Frame {
    fn observe(
        &mut self,
        call: DependencyCall,
        returned: DependencyReturn,
    ) -> Result<(), SourceRuleError> {
        let size = call.target.len()
            + call.arguments.iter().map(value_bytes).sum::<usize>()
            + return_bytes(&returned);
        let bytes = self
            .trace_bytes
            .checked_add(size)
            .ok_or(SourceRuleError::Capacity)?;
        if self.observations.len() >= MAX_DEPENDENCIES || bytes > MAX_TRACE_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        self.trace_bytes = bytes;
        self.observations
            .push(DependencyObservation { call, returned });
        Ok(())
    }
    fn call<B: SourceBackend>(
        &mut self,
        b: &mut B,
        target: &str,
        args: Vec<V>,
        awaited: bool,
    ) -> R<V> {
        if self.observations.len() >= MAX_DEPENDENCIES {
            return Err(SourceRuleError::Capacity.into());
        }
        for a in &args {
            a.validate()?;
        }
        let call = DependencyCall {
            target: target.into(),
            arguments: args,
        };
        let result = b.invoke(&call)?;
        self.observe(call, result.clone())?;
        match result {
            DependencyReturn::Immediate(v) => {
                v.validate()?;
                Ok(v)
            }
            DependencyReturn::Pending(v) => {
                v.validate()?;
                Ok(if awaited { v } else { V::Promise })
            }
            DependencyReturn::Throw(e) => Err(Fault::Source(e)),
            DependencyReturn::PendingThrow(e) => {
                if awaited {
                    Err(Fault::Source(e))
                } else {
                    self.deferred.push(e);
                    Ok(V::Promise)
                }
            }
        }
    }
    fn database<B: SourceBackend>(&mut self, b: &mut B) -> R<V> {
        self.call(b, "db.getDatabase", vec![s("txIndex")], true)
    }
    fn db<B: SourceBackend>(&mut self, b: &mut B, h: &V, method: &str, mut a: Vec<V>) -> R<V> {
        if matches!(h, V::Null | V::Undefined) {
            return Err(ty(format!(
                "Cannot read properties of {} (reading '{method}')",
                text(h)?
            )));
        }
        a.insert(0, h.clone());
        self.call(b, &format!("db.{method}"), a, true)
    }
    fn rpc<B: SourceBackend>(
        &mut self,
        b: &mut B,
        client: &V,
        method: &str,
        mut a: Vec<V>,
    ) -> R<V> {
        if matches!(client, V::Null | V::Undefined) {
            return Err(ty(format!(
                "Cannot read properties of {} (reading '{method}')",
                text(client)?
            )));
        }
        if !matches!(client.get("$rpcClient"),V::Number(bits)if {let id=f64::from_bits(bits);id.is_finite()&&id>=1.0&&id.fract()==0.0})
        {
            return Err(ty(format!("this.client.{method} is not a function")));
        }
        a.insert(0, client.clone());
        self.call(b, &format!("client.{method}"), a, true)
    }
}
struct FrameMetadata<'f, 'b, B>(&'f mut Frame, &'b mut B);
impl<B: SourceBackend> SourceIndexMetadataIo for FrameMetadata<'_, '_, B> {
    type Error = Fault;
    fn database(&mut self) -> R<V> {
        self.0.database(self.1)
    }
    fn find_one(&mut self, h: &V, id: &str) -> R<V> {
        self.0
            .db(self.1, h, "findOneAsync", vec![obj(&[("_id", s(id))])])
    }
    fn insert_genesis(&mut self, h: &V, height: V) -> R<V> {
        self.0.db(
            self.1,
            h,
            "insertAsync",
            vec![obj(&[("_id", s("genesisBlock")), ("value", height)])],
        )
    }
    // Old SourceEvaluation does not expose console calls. No new dependency,
    // trace bytes, outcome or stored row is introduced under the old APIs.
    fn log_values(&mut self, _: &str, _: V) -> R<()> {
        Ok(())
    }
    fn log_throw(&mut self, _: &str, _: &Fault) -> R<()> {
        Ok(())
    }
}
impl TxIndexSourceOwner {
    pub fn new() -> Self {
        Self::default()
    }
    /// Import-time TxUtils.init is independent of static TxIndex.init. Invoke
    /// once and drain before stable commands, exactly as the reference driver.
    pub fn load_module<B: SourceBackend>(
        &mut self,
        b: &mut B,
    ) -> Result<SourceEvaluation, SourceRuleError> {
        let mut f = Frame::default();
        let old = self.clone();
        if !self.module_loaded {
            self.module_loaded = true;
            match f.call(b, "ClientWrapper.getInstance", vec![], true) {
                Ok(v) => self.txutils_client = v,
                Err(Fault::Source(e)) => {
                    self.client_rejection = Some(e.clone());
                    f.deferred.push(e)
                }
                Err(Fault::Local(e)) => {
                    *self = old;
                    return Err(e);
                }
            }
        }
        if let Err(e) = self.state_value() {
            *self = old;
            return Err(e);
        }
        Self::evaluation(Ok(V::Undefined), vec![], f)
    }
    /// Explicit original environment input, not a validator or DB result.
    pub fn set_forward_index(&mut self, value: V) -> Result<(), SourceRuleError> {
        value.validate()?;
        self.forward = value;
        Ok(())
    }
    pub fn construct(&mut self) -> V {
        self.singleton = true;
        handle()
    }
    pub fn evaluate<B: SourceBackend>(
        &mut self,
        instance: Option<u64>,
        call: &SourceCall,
        b: &mut B,
    ) -> Result<SourceEvaluation, SourceRuleError> {
        if call.arguments.len() > 16 {
            return Err(SourceRuleError::Capacity);
        }
        for a in &call.arguments {
            a.validate()?;
        }
        if instance.is_some_and(|id| id != 1 || !self.singleton) {
            return Err(SourceRuleError::InvalidInput);
        }
        if !(if instance.is_some() {
            INSTANCE_METHODS.contains(&call.method.as_str())
        } else {
            STATIC_METHODS.contains(&call.method.as_str())
        }) {
            return Err(SourceRuleError::UnknownMethod);
        }
        let old = self.clone();
        let mut f = Frame::default();
        let result = self
            .run(instance, &call.method, &call.arguments, &mut f, b)
            .and_then(|v| {
                v.validate()?;
                Ok(v)
            });
        if matches!(result, Err(Fault::Local(_))) {
            *self = old;
            return Self::evaluation(result, call.arguments.clone(), f);
        }
        if let Err(e) = self.state_value() {
            *self = old;
            return Err(e);
        }
        Self::evaluation(result, call.arguments.clone(), f)
    }
    fn evaluation(
        result: R<V>,
        args: Vec<V>,
        f: Frame,
    ) -> Result<SourceEvaluation, SourceRuleError> {
        Ok(SourceEvaluation {
            outcome: match result {
                Ok(v) => {
                    v.validate()?;
                    SourceOutcome::Returned(v)
                }
                Err(Fault::Source(e)) => SourceOutcome::Thrown(e),
                Err(Fault::Local(e)) => return Err(e),
            },
            arguments_after: args,
            dependencies: f.observations,
            deferred_rejections: f.deferred,
            global_writes: vec![],
        })
    }
    pub fn invoke_source<B: SourceBackend>(
        &mut self,
        instance: Option<u64>,
        method: &str,
        a: &[V],
        b: &mut B,
    ) -> Result<DependencyReturn, SourceRuleError> {
        let sync = method == "isPrintableASCII";
        Ok(
            match self
                .evaluate(
                    instance,
                    &SourceCall {
                        method: method.into(),
                        arguments: a.to_vec(),
                    },
                    b,
                )?
                .outcome
            {
                SourceOutcome::Returned(v) if sync => DependencyReturn::Immediate(v),
                SourceOutcome::Returned(v) => DependencyReturn::Pending(v),
                SourceOutcome::Thrown(e) if sync => DependencyReturn::Throw(e),
                SourceOutcome::Thrown(e) => DependencyReturn::PendingThrow(e),
            },
        )
    }
    fn run<B: SourceBackend>(
        &mut self,
        _instance: Option<u64>,
        method: &str,
        a: &[V],
        f: &mut Frame,
        b: &mut B,
    ) -> R<V> {
        match method {
            "init" => self.init(f, b),
            "getInstance" => {
                self.construct();
                self.init(f, b)?;
                Ok(handle())
            }
            "initializeIndex" | "initializeOrLoadDB" => self.metadata_with(
                SourceIndexMetadataMethod::Initialize,
                arg(a, 0),
                &mut FrameMetadata(f, b),
            ),
            "saveMaxHeight" => self.save_height(arg(a, 0), f, b),
            "fetchChainTip" => self.tip(f, b),
            "fetchBlockData" => self.block(arg(a, 0), f, b),
            "fetchTransactionData" => f.rpc(
                b,
                &self.class_client,
                "getRawTransaction",
                vec![arg(a, 0), arg(a, 1), arg(a, 2)],
            ),
            "DecodeRawTransaction" => match self.decode(arg(a, 0), f, b) {
                Ok(v) => Ok(v),
                Err(Fault::Source(_)) => Ok(V::Undefined),
                Err(e) => Err(e),
            },
            "isPrintableASCII" => Ok(V::Bool(printable(&arg(a, 0))?)),
            "processTransaction" => self.transaction(arg(a, 0), arg(a, 1), arg(a, 2), f, b),
            "processBlockData" => self.process_block(arg(a, 0), arg(a, 1), f, b),
            "extractBlockData" => self.extract(arg(a, 0), f, b),
            "saveTransactionData" => {
                let key = format!("tx-{}-{}", text(&arg(a, 3))?, text(&arg(a, 0))?);
                let query = obj(&[("_id", s(&key))]);
                let doc = obj(&[("_id", s(&key)), ("txData", arg(a, 4))]);
                let r = (|| {
                    let h = f.database(b)?;
                    let old = f.db(b, &h, "findOneAsync", vec![query.clone()])?;
                    if truth(&old) {
                        f.db(
                            b,
                            &h,
                            "updateAsync",
                            vec![
                                query,
                                obj(&[(
                                    "$set",
                                    obj(&[("txData", arg(a, 1)), ("payload", arg(a, 2))]),
                                )]),
                            ],
                        )?;
                    } else {
                        f.db(b, &h, "insertAsync", vec![doc])?;
                    }
                    Ok(V::Undefined)
                })();
                match r {
                    Err(Fault::Source(_)) => Ok(V::Undefined),
                    other => other,
                }
            }
            "loadIndex" => {
                let h = f.database(b)?;
                let rows = f.db(b, &h, "findAsync", vec![V::object()])?;
                let V::Array(rows) = rows else {
                    return Err(ty("entries.forEach is not a function"));
                };
                let mut data = V::object();
                for row in rows {
                    let key = text(&prop(&row, "_id")?)?;
                    if key == "__proto__" {
                        return Err(SourceRuleError::UnsupportedNumericDomain.into());
                    }
                    data.insert(&key, prop(&row, "value")?)?;
                }
                integer_order(&mut data);
                Ok(data)
            }
            "upsertTxValidityAndReason" => {
                let h = f.database(b)?;
                let rows = f.db(b, &h, "findAsync", vec![regex_query("^tx")])?;
                let V::Array(rows) = rows else {
                    return Err(ty("allTxData.find is not a function"));
                };
                let suffix = format!("-{}", text(&arg(a, 0))?);
                for row in rows {
                    let id = prop(&row, "_id")?;
                    let V::String(id) = id else {
                        return Err(if matches!(id, V::Null | V::Undefined) {
                            ty(format!(
                                "Cannot read properties of {} (reading 'endsWith')",
                                text(&id)?
                            ))
                        } else {
                            ty("txData._id.endsWith is not a function")
                        });
                    };
                    if id.ends_with(&suffix) {
                        f.db(
                            b,
                            &h,
                            "updateAsync",
                            vec![
                                obj(&[("_id", s(id))]),
                                obj(&[(
                                    "$set",
                                    obj(&[
                                        ("type", arg(a, 1)),
                                        ("valid", arg(a, 2)),
                                        ("reason", arg(a, 3)),
                                    ]),
                                )]),
                                obj(&[("upsert", V::Bool(true))]),
                            ],
                        )?;
                        break;
                    }
                }
                Ok(V::Undefined)
            }
            "clearTxIndex" => Err(thrown("ReferenceError", "Datastore is not defined")),
            "resetIndexFlag" => Err(thrown("ReferenceError", "txIndexDB is not defined")),
            "findMaxIndexedBlock" => self.max_height(f, b),
            "getTransactionData" => {
                let h = f.database(b)?;
                let pattern = format!("^tx-\\d+-{}$", text(&arg(a, 0))?);
                validate_regex(&pattern)?;
                let doc = f.db(b, &h, "findOneAsync", vec![regex_query(&pattern)])?;
                if truth(&doc) {
                    prop(&doc, "value")
                } else {
                    Ok(V::Null)
                }
            }
            "checkForIndex" => match (|| {
                let h = f.database(b)?;
                let doc = f.db(
                    b,
                    &h,
                    "findOneAsync",
                    vec![obj(&[("_id", s("indexExists"))])],
                )?;
                Ok(V::Bool(truth(&doc)))
            })() {
                Err(Fault::Source(_)) => Ok(V::Undefined),
                r => r,
            },
            _ => Err(SourceRuleError::UnknownMethod.into()),
        }
    }
    fn init<B: SourceBackend>(&mut self, f: &mut Frame, b: &mut B) -> R<V> {
        self.class_client = f.call(b, "ClientWrapper.getInstance", vec![V::Bool(true)], true)?;
        // The source console expression dereferences chain after assignment.
        let _ = text(&prop(&self.class_client, "chain")?)?;
        Ok(obj(&[("$txIndexClass", V::Bool(true))]))
    }
    fn tip<B: SourceBackend>(&mut self, f: &mut Frame, b: &mut B) -> R<V> {
        match f.rpc(b, &self.class_client, "getBlockCount", vec![]) {
            Err(Fault::Source(e)) => Err(thrown(
                "Error",
                format!("Error fetching chain tip: {}: {}", e.name, e.message),
            )),
            r => r,
        }
    }
    fn block<B: SourceBackend>(&mut self, height: V, f: &mut Frame, b: &mut B) -> R<V> {
        let hash = f.rpc(b, &self.class_client, "getBlockHash", vec![height])?;
        f.rpc(b, &self.class_client, "getBlock", vec![hash])
    }
    fn save_height<B: SourceBackend>(&mut self, height: V, f: &mut Frame, b: &mut B) -> R<V> {
        if matches!(height, V::Null | V::Undefined) {
            return Ok(V::Undefined);
        }
        let h = f.database(b)?;
        f.db(
            b,
            &h,
            "updateAsync",
            vec![
                obj(&[("_id", s("MaxHeight"))]),
                obj(&[("$set", obj(&[("value", height)]))]),
                obj(&[("upsert", V::Bool(true))]),
            ],
        )?;
        f.db(
            b,
            &h,
            "updateAsync",
            vec![
                obj(&[("_id", s("indexExists"))]),
                obj(&[("_id", s("indexExists")), ("value", V::Bool(true))]),
                obj(&[("upsert", V::Bool(true))]),
            ],
        )?;
        Ok(V::Undefined)
    }
    fn max_height<B: SourceBackend>(&mut self, f: &mut Frame, b: &mut B) -> R<V> {
        self.metadata_with(
            SourceIndexMetadataMethod::FindMax,
            V::Undefined,
            &mut FrameMetadata(f, b),
        )
    }
    pub(crate) fn metadata_with<I: SourceIndexMetadataIo>(
        &mut self,
        method: SourceIndexMetadataMethod,
        height: V,
        io: &mut I,
    ) -> Result<V, I::Error> {
        match method {
            SourceIndexMetadataMethod::FindMax => {
                crate::source_index_metadata_algorithms::source_find_max(io)
            }
            SourceIndexMetadataMethod::Initialize => {
                crate::source_index_metadata_algorithms::source_initialize_genesis(io, height)
            }
        }
    }
    fn decode<B: SourceBackend>(&mut self, raw: V, f: &mut Frame, b: &mut B) -> R<V> {
        let decoded = f.rpc(b, &self.class_client, "decoderawtransaction", vec![raw])?;
        let outputs = prop(&decoded, "vout")?;
        let V::Array(outputs) = outputs else {
            return Err(if matches!(outputs, V::Undefined | V::Null) {
                ty(format!(
                    "Cannot read properties of {} (reading 'find')",
                    text(&outputs)?
                ))
            } else {
                ty("decodedTx.vout.find is not a function")
            });
        };
        let mut found = None;
        for output in outputs {
            if prop(&prop(&output, "scriptPubKey")?, "type")? == s("nulldata") {
                found = Some(output);
                break;
            }
        }
        let Some(output) = found else {
            return Ok(V::Null);
        };
        let hex = prop(&prop(&output, "scriptPubKey")?, "hex")?;
        let V::String(hex) = hex else {
            return Err(if matches!(hex, V::Null | V::Undefined) {
                ty(format!(
                    "Cannot read properties of {} (reading 'indexOf')",
                    text(&hex)?
                ))
            } else {
                ty("opReturnData.indexOf is not a function")
            });
        };
        let units = hex.encode_utf16().collect::<Vec<_>>();
        let marker = [b'7' as u16, b'4' as u16, b'6' as u16, b'c' as u16];
        let Some(offset) = units.windows(4).position(|w| w == marker) else {
            return Ok(V::Null);
        };
        if offset > 6 {
            return Ok(V::Null);
        }
        let start = if units.get(4..8) == Some(&marker) {
            8
        } else if units.get(5..9) == Some(&marker) {
            9
        } else if units.get(6..10) == Some(&marker) {
            10
        } else {
            offset + 4
        };
        let bytes = loose_units(&units[start..]);
        if !bytes.iter().all(|v| (0x20..=0x7e).contains(v)) {
            return Ok(V::Null);
        }
        Ok(obj(&[
            ("marker", s("tl")),
            (
                "payload",
                s(String::from_utf8(bytes).map_err(|_| SourceRuleError::InvalidInput)?),
            ),
            ("decodedTx", decoded),
        ]))
    }
    fn transaction<B: SourceBackend>(
        &mut self,
        payload: V,
        id: V,
        marker: V,
        f: &mut Frame,
        b: &mut B,
    ) -> R<V> {
        let sender = self.sender(id.clone(), f, b)?;
        let outputs = self.outputs(id.clone(), f, b)?;
        let V::Array(outputs) = outputs else {
            return Err(if matches!(outputs, V::Null | V::Undefined) {
                ty(format!(
                    "Cannot read properties of {} (reading 'filter')",
                    text(&outputs)?
                ))
            } else {
                ty("outputs.filter is not a function")
            });
        };
        let mut reference = vec![];
        for o in outputs {
            let v = prop(&o, "vout")?;
            if matches!(v,V::Number(bits)if f64::from_bits(bits)==0.0||f64::from_bits(bits)==1.0) {
                reference.push(obj(&[
                    ("address", prop(&o, "address")?),
                    ("satoshis", prop(&o, "satoshis")?),
                ]));
            }
        }
        // Deliberately THREE original arguments. The source does not await this
        // async decoder. Execute its real port, retain its Promise settlement,
        // and hand the Promise itself to the actual DB boundary.
        let args = vec![id.clone(), marker.clone(), payload.clone()];
        let slot = f.observations.len();
        f.observe(
            DependencyCall {
                target: "Types.decodePayload".into(),
                arguments: args.clone(),
            },
            DependencyReturn::Pending(V::Undefined),
        )?;
        let decoded = decode_payload_unverified(
            &args,
            &mut TypesBackend {
                frame: f,
                backend: b,
            },
        )
        .map_err(|e| match e {
            crate::types_source::TypesError::Capacity => SourceRuleError::Capacity,
            _ => SourceRuleError::InvalidInput,
        })?;
        let returned = match decoded.outcome {
            TypesOutcome::Returned(v) => DependencyReturn::Pending(v),
            TypesOutcome::ReturnedError(e) => DependencyReturn::Pending(exception_value(e)),
            TypesOutcome::Rejected(e) => {
                f.deferred.push(e.clone());
                DependencyReturn::PendingThrow(e)
            }
        };
        f.trace_bytes = f
            .trace_bytes
            .checked_sub(return_bytes(&f.observations[slot].returned))
            .and_then(|v| v.checked_add(return_bytes(&returned)))
            .ok_or(SourceRuleError::Capacity)?;
        if f.trace_bytes > MAX_TRACE_BYTES {
            return Err(SourceRuleError::Capacity.into());
        }
        f.observations[slot].returned = returned;
        Ok(obj(&[
            ("sender", sender),
            ("reference", V::Array(reference)),
            ("payload", payload),
            ("decodedParams", V::Promise),
            ("marker", marker),
            ("txId", id),
        ]))
    }
    fn sender<B: SourceBackend>(&mut self, id: V, f: &mut Frame, b: &mut B) -> R<V> {
        let tx = match f.rpc(
            b,
            &self.txutils_client,
            "getRawTransaction",
            vec![id.clone()],
        ) {
            Ok(v) => v,
            Err(Fault::Source(_)) => V::Undefined,
            Err(e) => return Err(e),
        };
        if !truth(&tx) {
            return Ok(returned_error(format!(
                "Invalid transaction data for {}",
                text(&id)?
            )));
        }
        let vin = prop(&tx, "vin")?;
        if !truth(&vin) || prop(&vin, "length")? == V::number(0.0) {
            return Ok(returned_error(format!(
                "Invalid transaction data for {}",
                text(&id)?
            )));
        }
        let input = prop(&vin, "0")?;
        let parent_id = prop(&input, "txid")?;
        if !truth(&parent_id) {
            return Ok(returned_error(format!(
                "No previous transaction reference in input for {}",
                text(&parent_id)?
            )));
        }
        let parent = f.rpc(
            b,
            &self.txutils_client,
            "getRawTransaction",
            vec![parent_id.clone()],
        )?;
        if !truth(&parent) {
            return Ok(returned_error(format!(
                "Invalid parent transaction data for {}",
                text(&parent_id)?
            )));
        }
        let outputs = prop(&parent, "vout")?;
        let index = prop(&input, "vout")?;
        if !truth(&outputs)
            || matches!(
                compare(&prop(&outputs, "length")?, &index)?,
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            )
        {
            return Ok(returned_error(format!(
                "Invalid parent transaction data for {}",
                text(&parent_id)?
            )));
        }
        let output = prop(&outputs, &text(&index)?)?;
        let script = if truth(&output) {
            prop(&output, "scriptPubKey")?
        } else {
            V::Undefined
        };
        let addresses = if truth(&script) {
            prop(&script, "addresses")?
        } else {
            V::Undefined
        };
        if !truth(&output) || !truth(&script) || !truth(&addresses) {
            return Ok(returned_error(format!(
                "No output found for vin {} in transaction {}",
                text(&index)?,
                text(&parent_id)?
            )));
        }
        Ok(obj(&[
            ("senderAddress", prop(&addresses, "0")?),
            ("amount", prop(&output, "value")?),
        ]))
    }
    fn outputs<B: SourceBackend>(&mut self, id: V, f: &mut Frame, b: &mut B) -> R<V> {
        if !truth(&self.txutils_client) {
            if let Some(e) = &self.client_rejection {
                return Err(Fault::Source(e.clone()));
            }
        }
        let r = (|| {
            let tx = f.rpc(b, &self.txutils_client, "getRawTransaction", vec![id])?;
            let outputs = prop(&tx, "vout")?;
            let V::Array(outputs) = outputs else {
                return Err(if matches!(outputs, V::Null | V::Undefined) {
                    ty(format!(
                        "Cannot read properties of {} (reading 'map')",
                        text(&outputs)?
                    ))
                } else {
                    ty("tx.vout.map is not a function")
                });
            };
            let mut result = vec![];
            for output in outputs {
                let script = prop(&output, "scriptPubKey")?;
                let addresses = prop(&script, "addresses")?;
                let address = if truth(&addresses) {
                    prop(&addresses, "0")?
                } else {
                    V::Null
                };
                let amount = number(&prop(&output, "value")?)? * 100_000_000.0;
                let satoshis = math_round(amount);
                let row = obj(&[
                    ("address", address.clone()),
                    ("satoshis", V::number(satoshis)),
                    ("vout", prop(&output, "n")?),
                ]);
                if truth(&address) {
                    result.push(row);
                }
            }
            Ok(V::Array(result))
        })();
        match r {
            Err(Fault::Source(_)) => Ok(V::Undefined),
            r => r,
        }
    }
    fn process_block<B: SourceBackend>(
        &mut self,
        block: V,
        height: V,
        f: &mut Frame,
        b: &mut B,
    ) -> R<V> {
        let h = f.database(b)?;
        let transactions = prop(&block, "tx")?;
        let ids = match transactions {
            V::Array(a) => a,
            V::String(t) => t.chars().map(|c| s(c.to_string())).collect(),
            _ => return Err(ty("blockData.tx is not iterable")),
        };
        let mut result = vec![];
        for id in ids {
            let raw = f.rpc(
                b,
                &self.class_client,
                "getRawTransaction",
                vec![id.clone(), V::Bool(false), prop(&block, "hash")?],
            )?;
            let decoded = match self.decode(raw, f, b) {
                Ok(v) => v,
                Err(Fault::Source(_)) => V::Undefined,
                Err(e) => return Err(e),
            };
            if !matches!(decoded, V::Undefined | V::Null) && prop(&decoded, "marker")? == s("tl") {
                let row = self.transaction(
                    prop(&decoded, "payload")?,
                    id.clone(),
                    prop(&decoded, "marker")?,
                    f,
                    b,
                )?;
                result.push(row.clone());
                let key = format!("tx-{}-{}", text(&height)?, text(&id)?);
                match f.db(
                    b,
                    &h,
                    "updateAsync",
                    vec![
                        obj(&[("_id", s(&key))]),
                        obj(&[("_id", s(key)), ("value", row)]),
                        obj(&[("upsert", V::Bool(true))]),
                    ],
                ) {
                    Ok(_) | Err(Fault::Source(_)) => (),
                    Err(e) => return Err(e),
                };
                self.save_height(height.clone(), f, b)?;
            }
        }
        Ok(V::Array(result))
    }
    fn extract<B: SourceBackend>(&mut self, start: V, f: &mut Frame, b: &mut B) -> R<V> {
        let tip = self.tip(f, b)?;
        let mut height = start;
        if self.forward == s("1") || self.forward == s("true") {
            match self.max_height(f, b) {
                Ok(V::Undefined | V::Null) => height = tip.clone(),
                Ok(v) => {
                    height = match v {
                        V::String(t) => s(format!("{t}1")),
                        V::Object(_) | V::Array(_) => s(format!("{}1", text(&v)?)),
                        _ => V::number(number(&v)? + 1.0),
                    }
                }
                Err(Fault::Source(_)) => (),
                Err(e) => return Err(e),
            }
        }
        if matches!(height, V::Undefined | V::Null) {
            height = V::number(0.0);
        }
        if compare(&height, &tip)? == Some(core::cmp::Ordering::Greater) {
            return Ok(V::Undefined);
        }
        let mut count = 0;
        while matches!(
            compare(&height, &tip)?,
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        ) {
            if count >= MAX_BLOCKS {
                return Err(SourceRuleError::Capacity.into());
            }
            count += 1;
            self.class_parse_block = height.clone();
            let block = self.block(height.clone(), f, b)?;
            self.process_block(block, height.clone(), f, b)?;
            height = V::number(number(&height)? + 1.0);
        }
        // This source Promise is intentionally unawaited. Stable boundaries
        // drain it; its rejection is diagnostic, not extractBlockData failure.
        match self.save_height(tip, f, b) {
            Err(Fault::Source(e)) => f.deferred.push(e),
            Err(e) => return Err(e),
            Ok(_) => (),
        };
        Ok(V::Undefined)
    }
    pub fn state_value(&self) -> Result<V, SourceRuleError> {
        let state = obj(&[
            ("singleton", if self.singleton { handle() } else { V::Null }),
            ("classClient", self.class_client.clone()),
            ("classParseBlock", self.class_parse_block.clone()),
            (
                "instances",
                V::Array(if self.singleton {
                    vec![obj(&[
                        ("id", V::number(1.0)),
                        ("client", V::Null),
                        ("parseBlock", V::number(0.0)),
                    ])]
                } else {
                    vec![]
                }),
            ),
            ("txUtilsClient", self.txutils_client.clone()),
            ("forward", self.forward.clone()),
            ("moduleLoaded", V::Bool(self.module_loaded)),
            (
                "clientPromiseRejection",
                self.client_rejection.clone().map_or(V::Undefined, |e| {
                    obj(&[("name", s(e.name)), ("message", s(e.message))])
                }),
            ),
        ]);
        state.validate()?;
        Ok(state)
    }
    pub fn from_state_value(state: &V) -> Result<Self, SourceRuleError> {
        state.validate()?;
        let singleton = state.get("singleton");
        let present = if singleton == V::Null {
            false
        } else if singleton == handle() {
            true
        } else {
            return Err(SourceRuleError::InvalidInput);
        };
        let V::Bool(module_loaded) = state.get("moduleLoaded") else {
            return Err(SourceRuleError::InvalidInput);
        };
        let owner = Self {
            singleton: present,
            class_client: state.get("classClient"),
            class_parse_block: state.get("classParseBlock"),
            txutils_client: state.get("txUtilsClient"),
            forward: state.get("forward"),
            module_loaded,
            client_rejection: match state.get("clientPromiseRejection") {
                V::Undefined => None,
                V::Object(fields) if fields.len() == 2 => {
                    let v = state.get("clientPromiseRejection");
                    let (V::String(name), V::String(message)) = (v.get("name"), v.get("message"))
                    else {
                        return Err(SourceRuleError::InvalidInput);
                    };
                    Some(SourceException { name, message })
                }
                _ => return Err(SourceRuleError::InvalidInput),
            },
        };
        if !module_loaded && owner.txutils_client != V::Undefined {
            return Err(SourceRuleError::InvalidInput);
        }
        if owner.client_rejection.is_some()
            && (!module_loaded || owner.txutils_client != V::Undefined)
        {
            return Err(SourceRuleError::InvalidInput);
        }
        if owner.state_value()? != *state {
            return Err(SourceRuleError::InvalidInput);
        }
        if V::from_diagnostic_json(&state.diagnostic_json()?)? != *state {
            return Err(SourceRuleError::InvalidInput);
        }
        owner.checkpoint()?;
        Ok(owner)
    }
    pub fn checkpoint(&self) -> Result<Vec<u8>, SourceRuleError> {
        let json = self.state_value()?.diagnostic_json()?;
        if json.len() > MAX_STATE_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        let mut bytes = b"TLTI\x01".to_vec();
        bytes.extend_from_slice(&(json.len() as u32).to_be_bytes());
        bytes.extend_from_slice(json.as_bytes());
        with_crc(bytes).map_err(|_| SourceRuleError::Capacity)
    }
    pub fn from_checkpoint(bytes: &[u8]) -> Result<Self, SourceRuleError> {
        if bytes.len() > MAX_STATE_BYTES + 13 {
            return Err(SourceRuleError::Capacity);
        }
        let raw = verify_crc(bytes).map_err(|_| SourceRuleError::InvalidInput)?;
        if raw.len() < 9 || &raw[..5] != b"TLTI\x01" {
            return Err(SourceRuleError::InvalidInput);
        }
        let len = u32::from_be_bytes(
            raw[5..9]
                .try_into()
                .map_err(|_| SourceRuleError::InvalidInput)?,
        ) as usize;
        if raw.len() != len + 9 {
            return Err(SourceRuleError::InvalidInput);
        }
        let state = V::from_diagnostic_json(
            core::str::from_utf8(&raw[9..]).map_err(|_| SourceRuleError::InvalidInput)?,
        )?;
        let owner = Self::from_state_value(&state)?;
        if owner.checkpoint()? != bytes {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(owner)
    }
}
fn loose_units(t: &[u16]) -> Vec<u8> {
    fn digit(c: u16) -> Option<u8> {
        match c {
            48..=57 => Some((c - 48) as u8),
            97..=102 => Some((c - 97 + 10) as u8),
            65..=70 => Some((c - 65 + 10) as u8),
            _ => None,
        }
    }
    let mut out = vec![];
    for p in t.chunks_exact(2) {
        let (Some(a), Some(b)) = (digit(p[0]), digit(p[1])) else {
            break;
        };
        out.push(a * 16 + b);
    }
    out
}
fn printable(v: &V) -> R<bool> {
    if matches!(v, V::Number(_)) {
        return Err(thrown(
            "RangeError",
            "toString() radix argument must be between 2 and 36",
        ));
    }
    if let V::Object(fields) = v {
        if fields.iter().any(|(k, _)| k == "toString") {
            return Err(ty("buf.toString is not a function"));
        }
    }
    if matches!(v, V::Undefined | V::Null) {
        return Err(ty(format!(
            "Cannot read properties of {} (reading 'toString')",
            text(v)?
        )));
    }
    Ok(text(v)?.chars().all(|c| (' '..='~').contains(&c)))
}
fn math_round(v: f64) -> f64 {
    if !v.is_finite() || v == 0.0 {
        return v;
    }
    if (-0.5..0.0).contains(&v) {
        return -0.0;
    }
    let f = v.floor();
    if v - f < 0.5 {
        f
    } else {
        f + 1.0
    }
}
fn integer_order(v: &mut V) {
    if let V::Object(fields) = v {
        fields.sort_by_key(|(k, _)| {
            let n = k
                .parse::<u32>()
                .ok()
                .filter(|n| *n != u32::MAX && n.to_string() == *k);
            (n.is_none(), n.unwrap_or(0))
        });
    }
}
fn regex_query(pattern: &str) -> V {
    obj(&[(
        "_id",
        obj(&[("$regex", obj(&[("$regexp", s(pattern)), ("$flags", s(""))]))]),
    )])
}
fn validate_regex(pattern: &str) -> R<()> {
    let id = pattern
        .strip_prefix("^tx-\\d+-")
        .and_then(|s| s.strip_suffix('$'))
        .ok_or(SourceRuleError::InvalidInput)?;
    if id
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b':'))
    {
        return Ok(());
    }
    // Only independently verified constructor errors are supported; all other
    // injected regexp programs are outside this literal-ID bounded adapter.
    let reason = match id {
        "[" => Some("Unterminated character class"),
        "(" => Some("Unterminated group"),
        ")" => Some("Unmatched ')'"),
        _ => None,
    };
    if let Some(reason) = reason {
        Err(thrown(
            "SyntaxError",
            format!("Invalid regular expression: /{pattern}/: {reason}"),
        ))
    } else {
        Err(SourceRuleError::UnsupportedNumericDomain.into())
    }
}

impl TxIndexSourceOwner {
    pub(crate) fn source_rich_class_client(&self) -> &V {
        &self.class_client
    }
}
