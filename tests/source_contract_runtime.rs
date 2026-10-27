//! Full-source fixtures provide expected states, never dependency responses.
use std::collections::BTreeMap;
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceCall, SourceOutcome, SourceValue as V,
};
fn s(x: &str) -> V {
    V::String(x.into())
}
fn n(x: f64) -> V {
    V::number(x)
}
fn text(v: &V) -> String {
    if let V::String(x) = v {
        x.clone()
    } else {
        panic!("string expected: {v:?}")
    }
}
fn number(v: &V) -> f64 {
    if let V::Number(x) = v {
        f64::from_bits(*x)
    } else {
        panic!("number expected: {v:?}")
    }
}
fn decode(hex: &str) -> V {
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    let t = std::str::from_utf8(&bytes).unwrap();
    tradelayer_core::property_catalog::parse_diagnostic_value(t)
        .unwrap_or_else(|e| panic!("decode {e:?}: {}", &t[..t.len().min(350)]))
}
fn fields(v: &V) -> Vec<(String, V)> {
    if let V::Object(x) = v {
        x.clone()
    } else {
        panic!("object expected")
    }
}
fn assert_value(actual: &V, expected: &V, label: &str) {
    fn canonical(v: &V) -> V {
        match v {
            V::BigNumber(t) => V::BigNumber(
                t.parse::<tradelayer_core::decimal::Decimal>()
                    .map_or_else(|_| t.clone(), |d| d.to_string()),
            ),
            V::Object(f) => V::Object(f.iter().map(|(k, v)| (k.clone(), canonical(v))).collect()),
            V::Array(a) => V::Array(a.iter().map(canonical).collect()),
            _ => v.clone(),
        }
    }
    let actual = canonical(actual);
    let expected = canonical(expected);
    let actual = &actual;
    let expected = &expected;
    if actual == expected {
        return;
    };
    fn diff(a: &V, b: &V, path: String) -> String {
        match (a, b) {
            (V::Object(a), V::Object(b)) => {
                for (i, ((ak, av), (bk, bv))) in a.iter().zip(b).enumerate() {
                    if ak != bk {
                        return format!("{path} field{i}: {ak} != {bk}");
                    };
                    if av != bv {
                        return diff(av, bv, format!("{path}.{ak}"));
                    }
                }
                format!("{path} field lengths {} != {}", a.len(), b.len())
            }
            (V::Array(a), V::Array(b)) => {
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    if a != b {
                        return diff(a, b, format!("{path}[{i}]"));
                    }
                }
                format!("{path} lengths {} != {}", a.len(), b.len())
            }
            (V::String(a), V::String(b)) => {
                let at = a
                    .bytes()
                    .zip(b.bytes())
                    .position(|(a, b)| a != b)
                    .unwrap_or(a.len().min(b.len()));
                let start = at.saturating_sub(50);
                format!(
                    "{path} string byte{at}: {:?} != {:?}",
                    a.get(start..(at + 100).min(a.len())),
                    b.get(start..(at + 100).min(b.len()))
                )
            }
            _ => format!("{path}: {a:?} != {b:?}"),
        }
    }
    panic!("{label}: {}", diff(actual, expected, "$".into()))
}
fn outcome(v: SourceOutcome) -> V {
    match v {
        SourceOutcome::Returned(v) => V::Object(vec![("returned".into(), v)]),
        SourceOutcome::Thrown(e) => V::Object(vec![
            ("params".into(), V::Undefined),
            (
                "thrown".into(),
                V::Object(vec![
                    ("name".into(), s(&e.name)),
                    ("message".into(), s(&e.message)),
                ]),
            ),
        ]),
    }
}
const PROJECT: &str = r#"
const fs=require('node:fs');
const rows=fs.readFileSync('fixtures/source_contract_execution_vectors.tsv','utf8').trimEnd().split('\n').filter(x=>x.startsWith('V\t'));
const norm=x=>x[0]==='map'?['array',x[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:x[0]==='object'?['object',x[1].map(([k,v])=>[k,norm(v)])]:x[0]==='array'?['array',x[1].map(norm)]:x;
const get=(v,k)=>v[1].find(([key])=>key===k)?.[1]??['undefined'];
const hex=x=>Buffer.from(JSON.stringify(norm(x))).toString('hex');
for(const row of rows){const f=row.split('\t'),cmd=JSON.parse(Buffer.from(f[2],'hex')),out=JSON.parse(Buffer.from(f[3],'hex')),state=JSON.parse(Buffer.from(f[4],'hex'));
 const clean=['object',out[1].filter(([k])=>k!=='sourceLogs')];console.log(['V',f[1],hex(cmd),hex(clean)].join('\t'));
 const tables=get(state,'tables');for(const table of tables[1]){const name=get(table,'name')[1];console.log(['C',name].join('\t'));for(const doc of get(table,'rows')[1])console.log(['R',name,hex(doc)].join('\t'));}
 for(const key of ['blockTrades','latestOracleMarkById','latestNativeMarkById'])console.log(['S',key,hex(get(get(state,'clearing'),key))].join('\t'));
 console.log(['S','instances',hex(get(state,'instances'))].join('\t'));
 console.log(['S','tallyCache',hex(get(get(state,'tally'),'addresses'))].join('\t'));
 console.log(['S','sourceOracle',hex(get(state,'oracle'))].join('\t'));
 console.log(['S','insurance',hex(get(state,'insurance'))].join('\t'));
 console.log(['S','feeAccrualBuffer',hex(get(get(state,'tally'),'feeAccrualBuffer'))].join('\t'));
}
"#;
#[derive(Default)]
struct Case {
    name: String,
    command: Option<V>,
    expected: Option<V>,
    tables: BTreeMap<String, Vec<V>>,
    state: BTreeMap<String, V>,
}
fn apply_case(runtime: &mut SourceContractRuntime, c: &Case) {
    let command = c.command.as_ref().unwrap();
    if command.get("op") == s("reset") {
        *runtime = SourceContractRuntime::with_policy(runtime.policy());
        let scene = command.get("scene");
        runtime.set_activation_answer(scene.get("inactive") != V::Bool(true));
        let entry = if scene.get("preload") == s("tally") {
            "tally-first"
        } else if scene.get("preload") == s("volumeIndex") {
            "volume-first"
        } else if scene.get("entry") == s("types-first") {
            "types-first"
        } else {
            "main-subgraph"
        };
        runtime.set_entry(entry).unwrap();
        for (name, rows) in &c.tables {
            runtime.import_collection(name, rows.clone()).unwrap()
        }
        return;
    }
    let height = [
        command.get("block"),
        command.get("drain"),
        command.get("publish").get("height"),
        command.get("markProbe").get("height"),
        command.get("flushFees"),
    ]
    .into_iter()
    .find(|v| !matches!(v, V::Undefined))
    .unwrap_or(n(10.0));
    runtime.set_height(height).unwrap();
    let actual = if let V::String(raw) = command.get("raw") {
        let result = runtime.execute_contract_unverified(&[
            command.get("txid"),
            n(18.0),
            s("tl"),
            s(&raw),
            command.get("sender"),
            V::Null,
            n(0.0),
            n(0.0),
            if matches!(command.get("block"), V::Undefined) {
                n(10.0)
            } else {
                command.get("block")
            },
        ]);
        match result {
            Ok(SourceOutcome::Returned(v)) => v,
            Ok(v) => {
                let mut value = outcome(v);
                value
                    .insert(
                        "params",
                        runtime
                            .last_contract_parameters()
                            .cloned()
                            .unwrap_or(V::Undefined),
                    )
                    .unwrap();
                value
            }
            Err(e) => panic!(
                "{} source execution unavailable: {e:?}; effects {:?}",
                c.name,
                runtime.take_effects().last()
            ),
        }
    } else if !matches!(command.get("drain"), V::Undefined) {
        outcome(
            runtime
                .evaluate_queue(&SourceCall {
                    method: "processQueuedOnChainOrdersForBlock".into(),
                    arguments: vec![command.get("drain")],
                })
                .unwrap_or_else(|e| {
                    panic!(
                        "{} drain {e:?}; last effects {:?}",
                        c.name,
                        runtime
                            .take_effects()
                            .iter()
                            .rev()
                            .take(8)
                            .collect::<Vec<_>>()
                    )
                }),
        )
    } else if !matches!(command.get("failure"), V::Undefined) {
        let f = command.get("failure");
        let op = text(&f.get("op"));
        runtime
            .fail_storage(StorageFailure {
                collection: text(&f.get("collection")),
                operation: format!("{op}Async"),
                skip: if matches!(f.get("skip"), V::Undefined) {
                    0
                } else {
                    number(&f.get("skip")) as usize
                },
                message: text(&f.get("message")),
            })
            .unwrap();
        outcome(SourceOutcome::Returned(V::Undefined))
    } else if !matches!(command.get("markProbe"), V::Undefined) {
        let p = command.get("markProbe");
        let r = runtime
            .invoke_dependency(&DependencyCall {
                target: "Clearing.isPriceUpdatedForBlockHeight".into(),
                arguments: vec![p.get("contractId"), p.get("height")],
            })
            .unwrap();
        outcome(match r {
            DependencyReturn::Pending(v) | DependencyReturn::Immediate(v) => {
                SourceOutcome::Returned(v)
            }
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                SourceOutcome::Thrown(e)
            }
        })
    } else if !matches!(command.get("flushFees"), V::Undefined) {
        let r = runtime
            .invoke_dependency(&DependencyCall {
                target: "Tally.flushQueuedFeeAccruals".into(),
                arguments: vec![command.get("flushFees")],
            })
            .unwrap();
        outcome(match r {
            DependencyReturn::Pending(v) | DependencyReturn::Immediate(v) => {
                SourceOutcome::Returned(v)
            }
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                SourceOutcome::Thrown(e)
            }
        })
    } else if !matches!(command.get("publish"), V::Undefined) {
        let p = command.get("publish");
        let r = runtime
            .invoke_dependency(&DependencyCall {
                target: "Oracle.publishData".into(),
                arguments: vec![
                    n(1.0),
                    p.get("price"),
                    p.get("price"),
                    p.get("price"),
                    p.get("price"),
                    p.get("height"),
                ],
            })
            .unwrap();
        outcome(match r {
            DependencyReturn::Pending(v) | DependencyReturn::Immediate(v) => {
                SourceOutcome::Returned(v)
            }
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                SourceOutcome::Thrown(e)
            }
        })
    } else {
        panic!("unknown action {}", c.name)
    };
    assert_value(
        &actual,
        c.expected.as_ref().unwrap(),
        &format!("{} complete source outcome", c.name),
    );
    for (name, expected) in &c.tables {
        assert_value(
            &V::Array(runtime.collection(name).to_vec()),
            &V::Array(expected.clone()),
            &format!("{} complete persisted collection {name}", c.name),
        );
    }
    let state = runtime.state_value().unwrap();
    for (source, actual) in [
        ("blockTrades", "blockTrades"),
        ("latestOracleMarkById", "oracleMarks"),
        ("latestNativeMarkById", "nativeMarks"),
    ] {
        assert_value(
            &state.get(actual),
            &c.state[source],
            &format!("{} {source}", c.name),
        )
    }
    if !matches!(c.state["feeAccrualBuffer"], V::Undefined) {
        assert_value(
            &state.get("feeBuffer"),
            &c.state["feeAccrualBuffer"],
            &format!("{} complete fee cache", c.name),
        )
    }
    if !matches!(c.state["tallyCache"], V::Undefined) {
        assert_value(
            &runtime.tally_cache(),
            &c.state["tallyCache"],
            &format!("{} actual Tally cache", c.name),
        )
    }
    assert_value(
        &state.get("sourceOracle"),
        &c.state["sourceOracle"],
        &format!("{} actual Oracle cache/watermarks", c.name),
    );
    let actual_insurance = V::Array(
        fields(&state.get("insurance"))
            .into_iter()
            .map(|(k, v)| V::Array(vec![s(&k), v]))
            .collect(),
    );
    assert_value(
        &actual_insurance,
        &c.state["insurance"],
        &format!("{} actual Insurance cache", c.name),
    );
    if let V::Array(tables) = state.get("tables") {
        for table in tables {
            let name = text(&table.get("name"));
            if !c.tables.contains_key(&name) {
                assert_value(
                    &table.get("rows"),
                    &V::Array(vec![]),
                    &format!("{} unexpected collection {name}", c.name),
                )
            }
        }
    }
    // Queue instance fields retain source receiver identity and live cache.
    let V::Array(expected_instances) = &c.state["instances"] else {
        panic!()
    };
    for instance in expected_instances {
        let id = number(&instance.get("id")) as u64;
        let expected = V::Object(
            fields(instance)
                .into_iter()
                .filter(|(k, _)| k != "id")
                .collect(),
        );
        assert_value(
            &runtime.queue().instance_value(id).unwrap(),
            &expected,
            &format!("{} queue cache", c.name),
        )
    }
}
fn run_source_composition(policy: P) {
    let result = std::process::Command::new("node")
        .args(["-e", PROJECT])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut runtime = SourceContractRuntime::with_policy(policy);
    let mut current = Case::default();
    let mut count = 0;
    for line in std::str::from_utf8(&result.stdout).unwrap().lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        match f[0] {
            "V" => {
                if current.command.is_some() {
                    apply_case(&mut runtime, &current);
                    count += 1
                };
                current = Case {
                    name: f[1].into(),
                    command: Some(decode(f[2])),
                    expected: Some(decode(f[3])),
                    ..Default::default()
                };
            }
            "C" => {
                current.tables.insert(f[1].into(), vec![]);
            }
            "R" => current.tables.get_mut(f[1]).unwrap().push(decode(f[2])),
            "S" => {
                current.state.insert(f[1].into(), decode(f[2]));
            }
            _ => panic!(),
        }
    }
    apply_case(&mut runtime, &current);
    count += 1;
    assert_eq!(count, 135);
}

#[test]
fn genuine_source_composition_drives_real_mutable_contract_owners() {
    run_source_composition(P::GlobalClearingV2);
}

#[test]
fn v4_retains_all_original_genuine_onchain_settlement_comparisons() {
    run_source_composition(P::OptionLifecycleV4);
}

#[test]
fn v6_retains_all_original_genuine_onchain_settlement_comparisons() {
    run_source_composition(P::ProofLifecycleV6);
}
