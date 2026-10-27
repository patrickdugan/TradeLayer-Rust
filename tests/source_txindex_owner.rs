use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tradelayer_core::issuance_intent::JsonValue as J;
use tradelayer_core::txindex_source_owner::TxIndexSourceOwner;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};

fn s(v: &str) -> V {
    V::String(v.into())
}
fn has(v: &V, k: &str) -> bool {
    matches!(v,V::Object(f)if f.iter().any(|(name,_)|name==k))
}
fn t(v: &V) -> String {
    match v {
        V::String(t) => t.clone(),
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::Number(b) => f64::from_bits(*b).to_string(),
        _ => "[object Object]".into(),
    }
}
fn o(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn a(v: &V) -> Vec<V> {
    if let V::Array(a) = v {
        a.clone()
    } else {
        panic!("array expected: {v:?}")
    }
}
fn parse(j: &J) -> V {
    let v = V::from_json(j).unwrap();
    fn convert(v: V) -> V {
        match v {
            V::Object(f) => {
                if f.len() == 1 {
                    match f[0].0.as_str() {
                        "$undefined" => return V::Undefined,
                        "$promise" => return V::Promise,
                        "$number" => {
                            let V::String(t) = &f[0].1 else { panic!() };
                            return V::number(match t.as_str() {
                                "NaN" => f64::NAN,
                                "Infinity" => f64::INFINITY,
                                "-Infinity" => f64::NEG_INFINITY,
                                "-0" => -0.0,
                                _ => panic!("bad number"),
                            });
                        }
                        _ => (),
                    }
                }
                V::Object(f.into_iter().map(|(k, v)| (k, convert(v))).collect())
            }
            V::Array(a) => V::Array(a.into_iter().map(convert).collect()),
            v => v,
        }
    }
    convert(v)
}
fn read_records(file: &str) -> Vec<V> {
    fs::read_to_string(file)
        .unwrap()
        .lines()
        .map(|l| parse(&J::parse(l).unwrap()))
        .collect()
}
fn oracle_root() -> PathBuf {
    std::env::var_os("TL_TXINDEX_ORACLE_STAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}
fn fixture(name: &str) -> String {
    oracle_root()
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .into_owned()
}
fn check_unchanged_source(proof: bool) {
    let mut command = Command::new("node");
    command.arg(oracle_root().join("scripts/txindex_source_vectors.cjs"));
    if proof {
        command.arg("--proof10");
    }
    if std::env::var_os("TL_TXINDEX_LOCKED_DEPENDENCY").is_some() {
        command.arg("--lock");
    }
    let output = command
        .arg("--check")
        .output()
        .expect("genuine Node oracle must execute");
    assert!(
        output.status.success(),
        "unchanged-source oracle check failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn exception(e: &V) -> SourceException {
    let V::String(name) = e.get("name") else {
        panic!()
    };
    let V::String(message) = e.get("message") else {
        panic!()
    };
    SourceException { name, message }
}
fn storage(v: V) -> V {
    match v {
        V::Promise => V::object(),
        V::Object(f) if f.len() == 1 && f[0].0 == "$error" => V::object(),
        V::Object(f) => V::Object(f.into_iter().map(|(k, v)| (k, storage(v))).collect()),
        V::Array(a) => V::Array(a.into_iter().map(storage).collect()),
        v => v,
    }
}
fn check_document(v: &V) -> Result<(), SourceException> {
    match v {
        V::Object(fields) => {
            for (k, v) in fields {
                let allowed = (k == "$$date" && matches!(v, V::Number(_)))
                    || (k == "$$deleted" && v == &V::Bool(true))
                    || k == "$$indexCreated"
                    || k == "$$indexRemoved";
                let message = if k.starts_with('$') && !allowed {
                    Some("Field names cannot begin with the $ character")
                } else if k.contains('.') {
                    Some("Field names cannot contain a .")
                } else {
                    None
                };
                if let Some(message) = message {
                    return Err(SourceException {
                        name: "Error".into(),
                        message: message.into(),
                    });
                }
                check_document(v)?;
            }
        }
        V::Array(values) => {
            for v in values {
                check_document(v)?;
            }
        }
        _ => (),
    }
    Ok(())
}
fn project(v: V) -> V {
    match v {
        V::Object(f) if f.iter().any(|(k, _)| k == "sourceDatabase") => {
            o(&[("$database", s("txIndex"))])
        }
        V::Object(f) if f.iter().any(|(k, _)| k == "$rpcClient") => {
            o(&[("$rpcClient", V::number(1.0))])
        }
        V::Object(f) => V::Object(
            f.into_iter()
                .map(|(k, v)| (if k == "$flags" { "flags".into() } else { k }, project(v)))
                .collect(),
        ),
        V::Array(a) => V::Array(a.into_iter().map(project).collect()),
        v => v,
    }
}
#[derive(Clone)]
struct Backend {
    rows: Vec<V>,
    rpc: V,
    failures: Vec<V>,
    effects: Vec<V>,
    client_value: Option<V>,
}
impl Default for Backend {
    fn default() -> Self {
        Self {
            rows: vec![],
            rpc: o(&[
                ("tip", V::number(0.0)),
                ("blocks", V::Array(vec![])),
                ("transactions", V::Array(vec![])),
            ]),
            failures: vec![],
            effects: vec![],
            client_value: None,
        }
    }
}
impl Backend {
    fn configure(&mut self, c: &V) {
        if has(c, "clientValue") {
            self.client_value = Some(c.get("clientValue"));
        }
        if c.get("rpc") != V::Undefined {
            self.rpc = c.get("rpc");
        }
        if let V::Array(f) = c.get("failures") {
            self.failures = f;
        }
    }
    fn effect(
        &mut self,
        target: &str,
        args: Vec<V>,
        returned: Result<V, SourceException>,
    ) -> Result<DependencyReturn, SourceRuleError> {
        let mut row = o(&[("target", s(target)), ("args", project(V::Array(args)))]);
        match returned {
            Ok(v) => {
                row.insert("returned", project(v.clone()))?;
                self.effects.push(row);
                Ok(DependencyReturn::Pending(v))
            }
            Err(e) => {
                row.insert(
                    "thrown",
                    o(&[("name", s(&e.name)), ("message", s(&e.message))]),
                )?;
                self.effects.push(row);
                Ok(DependencyReturn::PendingThrow(e))
            }
        }
    }
    fn fault(&mut self, target: &str) -> Option<SourceException> {
        let at = self
            .failures
            .iter()
            .position(|v| v.get("target") == s(target))?;
        let skip = self.failures[at].get("skip");
        let n = if let V::Number(b) = skip {
            f64::from_bits(b)
        } else {
            0.0
        };
        if n > 0.0 {
            self.failures[at]
                .insert("skip", V::number(n - 1.0))
                .unwrap();
            return None;
        }
        let v = self.failures.remove(at);
        let V::String(message) = v.get("message") else {
            panic!()
        };
        Some(SourceException {
            name: "Error".into(),
            message,
        })
    }
    fn sort(&mut self) {
        self.rows.sort_by_key(|r| {
            if let V::String(id) = r.get("_id") {
                id
            } else {
                panic!("stored _id")
            }
        });
    }
    fn matches(row: &V, query: &V) -> bool {
        let V::Object(q) = query else { panic!() };
        q.iter().all(|(k, v)| {
            let v = v.get("$regex");
            if v != V::Undefined {
                let V::String(pattern) = v.get("$regexp") else {
                    panic!()
                };
                let V::String(id) = row.get(k) else {
                    return false;
                };
                if pattern == "^tx" {
                    id.starts_with("tx")
                } else {
                    let tail = pattern
                        .strip_prefix("^tx-\\d+-")
                        .unwrap()
                        .strip_suffix('$')
                        .unwrap();
                    let Some(rest) = id.strip_prefix("tx-") else {
                        return false;
                    };
                    let Some((height, key)) = rest.split_once('-') else {
                        return false;
                    };
                    !height.is_empty() && height.bytes().all(|b| b.is_ascii_digit()) && key == tail
                }
            } else {
                row.get(k) == query.get(k)
            }
        })
    }
    fn execute(&mut self, target: &str, args: &[V]) -> Result<V, SourceException> {
        if target == "ClientWrapper.getInstance" {
            if let Some(v) = &self.client_value {
                return Ok(v.clone());
            }
            return Ok(o(&[("$rpcClient", V::number(1.0)), ("chain", s("LTC"))]));
        }
        if target == "db.getDatabase" {
            return Ok(o(&[("sourceDatabase", s("txIndex"))]));
        }
        if target.starts_with("client.") {
            let method = target.strip_prefix("client.").unwrap();
            let arg = |i| args.get(i).cloned().unwrap_or(V::Undefined);
            return match method {
                "getBlockCount" => Ok(self.rpc.get("tip")),
                "getBlockHash" => a(&self.rpc.get("blocks"))
                    .iter()
                    .find(|b| b.get("height") == arg(0))
                    .map(|b| b.get("block").get("hash"))
                    .ok_or(SourceException {
                        name: "Error".into(),
                        message: format!("missing block height {}", t(&arg(0))),
                    }),
                "getBlock" => a(&self.rpc.get("blocks"))
                    .iter()
                    .find(|b| b.get("block").get("hash") == arg(0))
                    .map(|b| b.get("block"))
                    .ok_or(SourceException {
                        name: "Error".into(),
                        message: format!("missing block hash {}", t(&arg(0))),
                    }),
                "getRawTransaction" => a(&self.rpc.get("transactions"))
                    .iter()
                    .find(|b| b.get("id") == arg(0))
                    .map(|b| {
                        b.get(if arg(1) == V::Bool(false) {
                            "hex"
                        } else {
                            "decoded"
                        })
                    })
                    .ok_or(SourceException {
                        name: "Error".into(),
                        message: format!("missing raw transaction {}", t(&arg(0))),
                    }),
                "decoderawtransaction" => a(&self.rpc.get("transactions"))
                    .iter()
                    .find(|b| b.get("hex") == arg(0))
                    .map(|b| b.get("decoded"))
                    .ok_or(SourceException {
                        name: "Error".into(),
                        message: format!("missing raw decoder input {}", t(&arg(0))),
                    }),
                _ => panic!("unexpected RPC {method}"),
            };
        }
        let method = target.strip_prefix("db.").unwrap();
        let query = args.first().cloned().unwrap_or(V::Undefined);
        match method {
            "findOneAsync" => Ok(self
                .rows
                .iter()
                .find(|r| Self::matches(r, &query))
                .cloned()
                .unwrap_or(V::Null)),
            "findAsync" => Ok(V::Array(
                self.rows
                    .iter()
                    .filter(|r| Self::matches(r, &query))
                    .cloned()
                    .collect(),
            )),
            "insertAsync" => {
                let row = storage(query);
                check_document(&row)?;
                if self.rows.iter().any(|old| old.get("_id") == row.get("_id")) {
                    return Err(SourceException {
                        name: "Error".into(),
                        message: format!(
                            "Can't insert key {}, it violates the unique constraint",
                            t(&row.get("_id"))
                        ),
                    });
                }
                self.rows.push(row.clone());
                self.sort();
                Ok(row)
            }
            "updateAsync" => {
                let update = args[1].clone();
                let options = args.get(2).cloned().unwrap_or(V::Undefined);
                let found = self.rows.iter().position(|r| Self::matches(r, &query));
                if found.is_none() && options.get("upsert") != V::Bool(true) {
                    return Ok(V::number(0.0));
                }
                let mut row =
                    found.map_or_else(|| o(&[("_id", query.get("_id"))]), |i| self.rows[i].clone());
                if let V::Object(fields) = update.get("$set") {
                    for (k, v) in fields {
                        row.insert(&k, storage(v)).unwrap();
                    }
                } else {
                    row = storage(update);
                }
                check_document(&row)?;
                if let Some(i) = found {
                    self.rows[i] = row;
                } else {
                    self.rows.push(row);
                }
                self.sort();
                Ok(V::number(1.0))
            }
            _ => panic!("unexpected DB {method}"),
        }
    }
}
impl SourceBackend for Backend {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let target = call.target.replacen("RPC.", "client.", 1);
        let args = if call.target.starts_with("db.") && call.target != "db.getDatabase"
            || call.target.starts_with("RPC.")
            || call.target.starts_with("client.")
        {
            call.arguments[1..].to_vec()
        } else {
            call.arguments.clone()
        };
        let result = if let Some(e) = self.fault(&target) {
            Err(e)
        } else {
            self.execute(&target, &args)
        };
        self.effect(&target, args, result)
    }
}
fn owner_state(owner: &TxIndexSourceOwner) -> V {
    let V::Object(mut f) = project(owner.state_value().unwrap()) else {
        panic!()
    };
    f.retain(|(k, _)| k != "moduleLoaded" && k != "clientPromiseRejection");
    V::Object(f)
}
fn compare_state(owner: &TxIndexSourceOwner, b: &Backend, reference: &V, case: &V) {
    let fields = if let V::Object(f) = owner_state(owner) {
        f
    } else {
        panic!()
    };
    for (k, v) in fields {
        assert_eq!(v, reference.get(&k), "state field {k} at {case:?}");
    }
    assert_eq!(
        V::Array(b.rows.clone()),
        reference.get("rows"),
        "rows at {case:?}"
    );
    assert_eq!(
        V::Array(b.failures.clone()),
        reference.get("failures"),
        "failure queue at {case:?}"
    );
}
fn run_corpus(file: &str) {
    let mut owner = TxIndexSourceOwner::new();
    let mut backend = Backend::default();
    owner.load_module(&mut backend).unwrap();
    backend.effects.clear();
    let mut promises = vec![];
    let mut background = vec![];
    for record in read_records(file) {
        let case = record.get("case");
        let command = record.get("command");
        let configure = command.get("configure");
        backend.configure(&configure);
        if has(&configure, "forward") {
            owner.set_forward_index(configure.get("forward")).unwrap();
        }
        backend.effects.clear();
        let arguments = if matches!(command.get("args"), V::Undefined) {
            vec![]
        } else {
            a(&command.get("args"))
        };
        let scope = command.get("scope");
        if scope == s("reset") || scope == s("restart") {
            owner = TxIndexSourceOwner::new();
            promises.clear();
            if scope == s("reset") {
                background.clear();
                backend = Backend::default();
                backend.configure(&command);
                backend.rows = if matches!(command.get("rows"), V::Undefined) {
                    vec![]
                } else {
                    a(&command.get("rows"))
                };
                backend.sort();
                owner.set_forward_index(command.get("forward")).unwrap();
            }
            if command.get("database") == V::Bool(true) {
                backend.rows = backend
                    .rows
                    .iter()
                    .map(|r| {
                        let text = tradelayer_core::property_catalog::json_stringify(r).unwrap();
                        V::from_json(&J::parse(&text).unwrap()).unwrap()
                    })
                    .collect();
            }
            let initialized = owner.load_module(&mut backend).unwrap();
            if let V::Array(errors) = record.get("backgroundErrors") {
                assert_eq!(
                    initialized.deferred_rejections,
                    errors[background.len()..]
                        .iter()
                        .map(exception)
                        .collect::<Vec<_>>(),
                    "initializer rejection {case:?}"
                );
                background = errors;
            }
            if scope == s("reset") {
                backend.effects.clear();
            }
            assert_eq!(
                record.get("outcome"),
                o(&[("transport", s("Immediate")), ("returned", V::Undefined)])
            );
            assert_eq!(
                record.get("effects"),
                V::Array(backend.effects.clone()),
                "reset/restart effects {case:?}"
            );
            compare_state(&owner, &backend, &record.get("state"), &case);
            continue;
        }
        let (outcome, deps, deferred) = if scope == s("constructor") {
            (
                o(&[
                    ("transport", s("Immediate")),
                    ("returned", owner.construct()),
                ]),
                vec![],
                vec![],
            )
        } else {
            let V::String(method) = command.get("method") else {
                panic!()
            };
            let instance = if scope == s("instance") {
                Some(1)
            } else {
                None
            };
            let e = owner.evaluate(
                instance,
                &SourceCall {
                    method: method.clone(),
                    arguments,
                },
                &mut backend,
            );
            if command.get("domain") == s("arbitrary-regexp") {
                assert_eq!(
                    e,
                    Err(SourceRuleError::UnsupportedNumericDomain),
                    "explicit local regexp domain at {case:?}"
                );
                continue;
            }
            let e = e.unwrap_or_else(|e| panic!("local {e:?} at {case:?}"));
            let sync = method == "isPrintableASCII";
            (
                match e.outcome {
                    SourceOutcome::Returned(v) => o(&[
                        ("transport", s(if sync { "Immediate" } else { "Pending" })),
                        ("returned", project(v)),
                    ]),
                    SourceOutcome::Thrown(e) => o(&[
                        ("transport", s(if sync { "Throw" } else { "PendingThrow" })),
                        (
                            "thrown",
                            o(&[("name", s(&e.name)), ("message", s(&e.message))]),
                        ),
                    ]),
                },
                e.dependencies,
                e.deferred_rejections,
            )
        };
        assert_eq!(outcome, record.get("outcome"), "outcome at {case:?}");
        // The decoder really executes; its unused Promise remains a Promise in
        // the source effect, while the dependency receipt retains fulfillment.
        let mut expected_deferred = vec![];
        for dep in &deps {
            if dep.call.target == "Types.decodePayload" {
                let id = V::number((promises.len() + 1) as f64);
                promises.push(match &dep.returned {
                    DependencyReturn::Pending(v) => {
                        o(&[("id", id), ("status", s("fulfilled")), ("value", v.clone())])
                    }
                    DependencyReturn::PendingThrow(e) => o(&[
                        ("id", id),
                        ("status", s("rejected")),
                        (
                            "error",
                            o(&[("name", s(&e.name)), ("message", s(&e.message))]),
                        ),
                    ]),
                    _ => panic!("actual Types is async"),
                });
            }
        }
        if let V::Array(errors) = record.get("backgroundErrors") {
            for e in &errors[background.len()..] {
                expected_deferred.push(exception(e));
            }
            background = errors;
        }
        assert_eq!(
            deferred, expected_deferred,
            "deferred source rejections at {case:?}"
        );
        assert_eq!(
            V::Array(promises.clone()),
            record.get("state").get("promises"),
            "actual decoder future settlements at {case:?}"
        );
        if !deps.is_empty() {
            backend.effects = deps
                .into_iter()
                .map(|dep| {
                    let target = dep.call.target.replacen("RPC.", "client.", 1);
                    let args = if dep.call.target.starts_with("db.")
                        && dep.call.target != "db.getDatabase"
                        || dep.call.target.starts_with("RPC.")
                        || dep.call.target.starts_with("client.")
                    {
                        dep.call.arguments[1..].to_vec()
                    } else {
                        dep.call.arguments
                    };
                    let mut event = o(&[("target", s(&target)), ("args", project(V::Array(args)))]);
                    match dep.returned {
                        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => event
                            .insert(
                                "returned",
                                if target == "Types.decodePayload" {
                                    V::Promise
                                } else {
                                    project(v)
                                },
                            )
                            .unwrap(),
                        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e)
                            if target == "Types.decodePayload" =>
                        {
                            let _ = e;
                            event.insert("returned", V::Promise).unwrap()
                        }
                        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => event
                            .insert(
                                "thrown",
                                o(&[("name", s(&e.name)), ("message", s(&e.message))]),
                            )
                            .unwrap(),
                    }
                    event
                })
                .collect();
        }
        assert_eq!(
            V::Array(backend.effects.clone()),
            record.get("effects"),
            "effects at {case:?}"
        );
        compare_state(&owner, &backend, &record.get("state"), &case);
        let bytes = owner.checkpoint().unwrap();
        assert_eq!(
            owner,
            TxIndexSourceOwner::from_checkpoint(&bytes).unwrap(),
            "checkpoint at {case:?}"
        );
        owner = TxIndexSourceOwner::from_checkpoint(&bytes).unwrap();
    }
}
#[test]
fn actual_pinned_txindex_initial_lifecycle_proof() {
    check_unchanged_source(true);
    run_corpus(&fixture("txindex_source_proof10.jsonl"));
}
#[test]
fn actual_pinned_txindex_complete_lifecycle_and_rpc_graph() {
    check_unchanged_source(false);
    run_corpus(&fixture("txindex_source_vectors.jsonl"));
}

#[test]
fn txindex_checkpoints_reject_corruption_and_inconsistent_live_metadata() {
    let mut owner = TxIndexSourceOwner::new();
    let mut backend = Backend::default();
    owner.load_module(&mut backend).unwrap();
    owner.construct();
    let state = owner.state_value().unwrap();
    for (key, value) in [
        ("instances", V::Array(vec![])),
        ("singleton", V::Null),
        ("moduleLoaded", V::Bool(false)),
        ("extra", V::Null),
        (
            "clientPromiseRejection",
            o(&[("name", s("Error")), ("message", s("invented-rejection"))]),
        ),
        ("classParseBlock", V::Number(0x7ff8_0000_0000_0001)),
    ] {
        let mut bad = state.clone();
        bad.insert(key, value).unwrap();
        assert_eq!(
            TxIndexSourceOwner::from_state_value(&bad),
            Err(SourceRuleError::InvalidInput),
            "invalid field {key}"
        );
    }
    let bytes = owner.checkpoint().unwrap();
    for i in [0, 4, 8, bytes.len() - 1] {
        let mut bad = bytes.clone();
        bad[i] ^= 1;
        assert_eq!(
            TxIndexSourceOwner::from_checkpoint(&bad),
            Err(SourceRuleError::InvalidInput)
        );
    }
    assert_eq!(
        TxIndexSourceOwner::from_checkpoint(&bytes[..bytes.len() - 1]),
        Err(SourceRuleError::InvalidInput)
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        TxIndexSourceOwner::from_checkpoint(&trailing),
        Err(SourceRuleError::InvalidInput)
    );
    let mut oversized = state;
    oversized
        .insert(
            "forward",
            s(&"x".repeat(tradelayer_core::validity_source::MAX_BYTES)),
        )
        .unwrap_err();
    assert_eq!(
        TxIndexSourceOwner::from_state_value(&oversized),
        Err(SourceRuleError::Capacity)
    );
}

#[test]
fn txindex_local_bounds_stop_unbounded_scans_and_preserve_owner_state() {
    struct Broken;
    impl SourceBackend for Broken {
        fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
            Err(SourceRuleError::Backend)
        }
    }
    let mut pending = TxIndexSourceOwner::new();
    let unloaded = pending.clone();
    assert_eq!(
        pending.load_module(&mut Broken),
        Err(SourceRuleError::Backend)
    );
    assert_eq!(pending, unloaded);
    let mut large = Backend {
        client_value: Some(s(&"x".repeat(tradelayer_core::validity_source::MAX_BYTES))),
        ..Backend::default()
    };
    assert_eq!(
        pending.load_module(&mut large),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(pending, unloaded);
    pending.load_module(&mut Backend::default()).unwrap();
    assert_ne!(pending, unloaded);
    struct Infinite {
        calls: usize,
    }
    impl SourceBackend for Infinite {
        fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
            self.calls += 1;
            Ok(DependencyReturn::Pending(match call.target.as_str() {
                "ClientWrapper.getInstance" => {
                    o(&[("$rpcClient", V::number(1.0)), ("chain", s("LTC"))])
                }
                "client.getBlockCount" => V::number(f64::INFINITY),
                "client.getBlockHash" => s("same-block"),
                "client.getBlock" => o(&[("hash", s("same-block")), ("tx", V::Array(vec![]))]),
                "db.getDatabase" => o(&[("sourceDatabase", s("txIndex"))]),
                _ => panic!("unexpected effect {call:?}"),
            }))
        }
    }
    let mut b = Infinite { calls: 0 };
    let mut owner = TxIndexSourceOwner::new();
    owner.load_module(&mut b).unwrap();
    owner
        .evaluate(
            None,
            &SourceCall {
                method: "init".into(),
                arguments: vec![],
            },
            &mut b,
        )
        .unwrap();
    let before = owner.clone();
    assert_eq!(
        owner.evaluate(
            None,
            &SourceCall {
                method: "extractBlockData".into(),
                arguments: vec![V::number(f64::INFINITY)]
            },
            &mut b
        ),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(owner, before);
    assert!(b.calls <= tradelayer_core::txindex_source_owner::MAX_DEPENDENCIES + 2);
    let calls = b.calls;
    assert_eq!(
        owner.evaluate(
            None,
            &SourceCall {
                method: "init".into(),
                arguments: vec![V::Null; 17]
            },
            &mut b
        ),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(
        owner.evaluate(
            Some(2),
            &SourceCall {
                method: "loadIndex".into(),
                arguments: vec![]
            },
            &mut b
        ),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(b.calls, calls);
}

#[test]
fn test_backend_nedb_copy_and_rejection_boundary_matches_real_datastore() {
    let path = fixture("txindex_nedb_boundary.json");
    let reference = parse(&J::parse(&fs::read_to_string(path).unwrap()).unwrap());
    assert_eq!(
        storage(reference.get("original")),
        reference.get("memoryCopy")
    );
    let serialized =
        tradelayer_core::property_catalog::json_stringify(&storage(reference.get("original")))
            .unwrap();
    assert_eq!(s(&serialized), reference.get("serialized"));
    assert_eq!(
        V::from_json(&J::parse(&serialized).unwrap()).unwrap(),
        reference.get("reloaded")
    );
    for rejection in a(&reference.get("rejectedFields")) {
        assert_eq!(
            check_document(&rejection.get("input")),
            Err(exception(&rejection.get("thrown")))
        );
    }
    let mut b = Backend::default();
    let first = o(&[("_id", s("same")), ("value", V::number(1.0))]);
    b.execute("db.insertAsync", &[first]).unwrap();
    let duplicate = o(&[("_id", s("same")), ("value", V::number(2.0))]);
    assert_eq!(
        b.execute("db.insertAsync", &[duplicate]),
        Err(exception(&reference.get("duplicate").get("thrown")))
    );
    assert_eq!(V::Array(b.rows), reference.get("duplicate").get("rows"));
}

#[test]
fn source_return_time_is_separately_recorded_from_stable_checkpoint_time() {
    let records = read_records(&fixture("txindex_source_vectors.jsonl"));
    let source = records
        .iter()
        .find(|r| r.get("case") == s("extract/normal-from-1"))
        .unwrap();
    let returned = source.get("atReturn");
    assert_ne!(
        returned.get("state").get("rows"),
        source.get("state").get("rows")
    );
    assert!(a(&returned.get("effects")).len() < a(&source.get("effects")).len());
    let restart = records
        .iter()
        .find(|r| r.get("case") == s("cache/module-restart-retains-db"))
        .unwrap();
    assert_eq!(
        restart.get("atReturn").get("state").get("txUtilsClient"),
        V::Undefined
    );
    assert_eq!(
        restart.get("state").get("txUtilsClient"),
        o(&[("$rpcClient", V::number(1.0))])
    );
}
