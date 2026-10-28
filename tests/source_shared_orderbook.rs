//! Exact-V11 single-collection authority and genuine Consensus hash regression.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{DependencyCall, DependencyReturn as D, SourceValue as V};
fn s(v: impl Into<String>) -> V {
    V::String(v.into())
}
fn n(v: f64) -> V {
    V::number(v)
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn arr(v: V) -> Vec<V> {
    let V::Array(v) = v else {
        panic!("array {v:?}")
    };
    v
}
fn text(v: V) -> String {
    let V::String(v) = v else {
        panic!("string {v:?}")
    };
    v
}
fn call(target: &str, arguments: Vec<V>) -> DependencyCall {
    DependencyCall {
        target: target.into(),
        arguments,
    }
}
fn invoke(rt: &mut SourceContractRuntime, target: &str, args: Vec<V>) -> D {
    rt.invoke_dependency(&call(target, args)).unwrap()
}
fn handle() -> V {
    obj(&[("sourceDatabase", s("orderBooks"))])
}
fn row(id: &str, book: &str) -> V {
    obj(&[("_id", s(id)), ("value", s(book))])
}
fn effects(rt: &mut SourceContractRuntime) -> V {
    V::Array(
        rt.take_effects()
            .into_iter()
            .filter(
                |v| matches!(v.get("kind"),V::String(k)if ["database","db"].contains(&k.as_str())),
            )
            .collect(),
    )
}
fn cases(lock: bool) -> Vec<V> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let stage = std::env::var("TL_SHARED_ORDERBOOK_EVIDENCE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| root.to_path_buf());
    let output = std::process::Command::new("node")
        .env("PROOF_REPO", root)
        .arg(stage.join("scripts/shared_orderbook_vectors.cjs"))
        .args([
            "--check",
            "--print",
            if lock { "--lock" } else { "--baseline" },
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect()
}
fn imported(case: &V) -> SourceContractRuntime {
    let mut rt = SourceContractRuntime::with_policy(P::AmmLifecycleV11);
    for table in arr(case.get("inputs")) {
        rt.import_collection(&text(table.get("name")), arr(table.get("rows")))
            .unwrap();
    }
    rt
}
#[test]
fn genuine_source_shared_reads_consensus_hashes_failures_and_exact_tables() {
    let data = cases(false);
    assert_eq!(data, cases(true));
    assert_eq!(data.len(), 3);
    assert_ne!(data[0].get("hash"), data[1].get("hash"));
    for case in data {
        let name = text(case.get("name"));
        let mut rt = imported(&case);
        if case.get("failFind") == V::Bool(true) {
            rt.fail_storage(StorageFailure {
                collection: "orderBooks".into(),
                operation: "findAsync".into(),
                skip: 0,
                message: "shared-book-find".into(),
            })
            .unwrap();
        } else {
            assert_eq!(
                invoke(&mut rt, "db.getDatabase", vec![s("orderBooks")]),
                D::Pending(handle())
            );
            let authoritative = V::Array(rt.collection("orderBooks").to_vec());
            assert_eq!(authoritative, case.get("read"));
            assert_eq!(
                invoke(&mut rt, "db.findAsync", vec![handle(), obj(&[])]),
                D::Pending(authoritative),
                "{name} generic DB read must use actual Queue rows"
            );
            assert_eq!(
                effects(&mut rt),
                case.get("readEffects"),
                "{name} exact source lookup effects"
            );
        }
        assert_eq!(
            invoke(&mut rt, "Consensus.stateConsensusHash", vec![]),
            D::Pending(case.get("hash")),
            "{name} exact genuine source Consensus hash"
        );
        assert_eq!(
            effects(&mut rt),
            case.get("effects"),
            "{name} exact source hash read/failure order"
        );
        assert_eq!(
            rt.state_value().unwrap().get("tables"),
            case.get("tables"),
            "{name} complete collection inventory/order/rows"
        );
    }
}
#[test]
fn consensus_hash_itself_observes_persisted_queue_rows() {
    for case in cases(false)
        .into_iter()
        .filter(|case| case.get("failFind") != V::Bool(true))
    {
        let mut rt = imported(&case);
        assert_eq!(
            invoke(&mut rt, "Consensus.stateConsensusHash", vec![]),
            D::Pending(case.get("hash")),
            "{} independent genuine source hash",
            text(case.get("name"))
        );
        assert_eq!(effects(&mut rt), case.get("effects"));
    }
}
#[test]
fn generic_writes_queue_loads_and_failures_share_one_row_and_failure_owner() {
    let mut rt = SourceContractRuntime::with_policy(P::AmmLifecycleV11);
    let first = row("shared-a", r#"{"buy":[{"price":2}],"sell":[]}"#);
    rt.import_collection("orderBooks", vec![first.clone()])
        .unwrap();
    let second = row("shared-b", r#"{"buy":[],"sell":[]}"#);
    assert_eq!(
        invoke(
            &mut rt,
            "db.updateAsync",
            vec![
                handle(),
                obj(&[("_id", s("shared-b"))]),
                second.clone(),
                obj(&[("upsert", V::Bool(true))])
            ]
        ),
        D::Pending(n(1.))
    );
    assert_eq!(
        rt.collection("orderBooks"),
        &[first.clone(), second.clone()]
    );
    assert_eq!(rt.queue().rows(), rt.collection("orderBooks"));
    let expected_tables = V::Array(vec![obj(&[
        ("name", s("orderBooks")),
        ("rows", V::Array(vec![first.clone(), second])),
    ])]);
    assert_eq!(rt.state_value().unwrap().get("tables"), expected_tables);
    rt.fail_storage(StorageFailure {
        collection: "orderBooks".into(),
        operation: "findOneAsync".into(),
        skip: 1,
        message: "shared-find".into(),
    })
    .unwrap();
    assert_eq!(
        invoke(
            &mut rt,
            "db.findOneAsync",
            vec![handle(), obj(&[("_id", s("shared-a"))])]
        ),
        D::Pending(first)
    );
    let receiver = match invoke(
        &mut rt,
        "Orderbook.getOrderbookInstance",
        vec![s("shared-a")],
    ) {
        D::Pending(v) => v,
        other => panic!("source loader catches find failure: {other:?}"),
    };
    let V::Number(bits) = receiver.get("orderbookInstance") else {
        panic!("real receiver")
    };
    assert_eq!(
        rt.queue()
            .instance_value(f64::from_bits(bits) as u64)
            .unwrap()
            .get("orderBooks")
            .get("shared-a"),
        obj(&[("buy", V::Array(vec![])), ("sell", V::Array(vec![]))])
    );
    assert!(
        arr(effects(&mut rt))
            .iter()
            .any(|e| e.get("thrown").get("message") == s("shared-find")),
        "same injected read failure consumed across generic and Queue paths"
    );
    let mut rt = SourceContractRuntime::with_policy(P::AmmLifecycleV11);
    rt.fail_storage(StorageFailure {
        collection: "orderBooks".into(),
        operation: "getDatabase".into(),
        skip: 0,
        message: "factory-first".into(),
    })
    .unwrap();
    assert!(matches!(
        invoke(&mut rt, "db.getDatabase", vec![s("orderBooks")]),
        D::PendingThrow(_)
    ));
    assert_eq!(
        rt.state_value().unwrap().get("tables"),
        V::Array(vec![]),
        "failed handle acquisition never materializes collection"
    );
    invoke(&mut rt, "db.getDatabase", vec![s("orderBooks")]);
    assert_eq!(rt.state_value().unwrap().get("tables"), V::Array(vec![]));
    invoke(&mut rt, "db.findAsync", vec![handle(), obj(&[])]);
    assert_eq!(
        arr(rt.state_value().unwrap().get("tables")).len(),
        1,
        "successful first operation materializes exactly one slot"
    );
}
#[test]
fn authoritative_book_rows_remain_replay_bound_after_generic_and_queue_access() {
    let mut replay = SourceContractReplay::with_policy(P::AmmLifecycleV11).unwrap();
    let rows = vec![row("shared-a", r#"{"buy":[],"sell":[]}"#)];
    replay
        .apply(C::Import {
            collection: "orderBooks".into(),
            rows: rows.clone(),
        })
        .unwrap();
    let receipt = replay
        .apply(C::Invoke(call("db.findAsync", vec![handle(), obj(&[])])))
        .unwrap();
    assert_eq!(
        receipt.outcome,
        O::Dependency(D::Pending(V::Array(rows.clone())))
    );
    let bytes = replay.checkpoint().unwrap();
    let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
    assert_eq!(restored, replay);
    assert_eq!(restored.checkpoint().unwrap(), bytes);
    assert_eq!(restored.runtime().collection("orderBooks"), rows);
}

#[test]
fn native_partial_throws_keep_valid_prefixes_but_cannot_commit_invalid_queue_rows() {
    let rows = vec![
        obj(&[("_id", s("a")), ("x", V::Array(vec![]))]),
        obj(&[("_id", s("b")), ("x", n(0.))]),
    ];
    let mut rt = SourceContractRuntime::with_policy(P::AmmLifecycleV11);
    rt.import_collection("orderBooks", rows.clone()).unwrap();
    let result = invoke(
        &mut rt,
        "db.updateAsync",
        vec![
            handle(),
            obj(&[]),
            obj(&[("$push", obj(&[("x", n(1.))]))]),
            obj(&[("multi", V::Bool(true))]),
        ],
    );
    assert!(
        matches!(result, D::PendingThrow(_)),
        "native non-array push keeps its valid first-row prefix"
    );
    assert_eq!(
        rt.collection("orderBooks")[0].get("x"),
        V::Array(vec![n(1.)])
    );
    assert_eq!(rt.collection("orderBooks")[1], rows[1]);
    rt.queue().checkpoint().unwrap();
    let mut rt = SourceContractRuntime::with_policy(P::AmmLifecycleV11);
    rt.import_collection("orderBooks", rows).unwrap();
    let before = rt.clone();
    let invalid = obj(&[
        ("$set", obj(&[("_id", s("b"))])),
        ("$push", obj(&[("x", n(1.))])),
    ]);
    assert!(
        rt.invoke_dependency(&call(
            "db.updateAsync",
            vec![
                handle(),
                obj(&[]),
                invalid,
                obj(&[("multi", V::Bool(true))])
            ]
        ))
        .is_err(),
        "a native partial prefix outside the existing unique-row domain is refused locally"
    );
    assert_eq!(
        rt, before,
        "invalid owner state never escapes the public call"
    );
    rt.queue().checkpoint().unwrap();
}
