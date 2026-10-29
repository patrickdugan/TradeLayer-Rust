pub use tradelayer_core::{source_rich_value, validity_source};
#[path = "../src/source_index_metadata_algorithms.rs"]
mod source_index_metadata_algorithms;
#[path = "support/source_native_throw.rs"]
mod source_native_throw;
use source_index_metadata_algorithms::SourceIndexMetadataIo;
use source_native_throw::CreatedThrown;
use validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};
fn s(x: &str) -> V {
    V::String(x.into())
}
fn obj(x: &[(&str, V)]) -> V {
    V::Object(x.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
#[derive(Clone)]
struct Db {
    rows: Vec<V>,
    fail: Option<(&'static str, usize)>,
    seen: Vec<DependencyCall>,
    logs: Vec<(String, Option<CreatedThrown>)>,
}
impl Db {
    fn invoke_raw(&mut self, call: DependencyCall) -> Result<V, CreatedThrown> {
        self.seen.push(call.clone());
        if let Some((method, skip)) = &mut self.fail {
            if *method == call.target {
                if *skip == 0 {
                    self.fail = None;
                    return Err(CreatedThrown::native(
                        source_rich_value::NativeKind::Error,
                        source_rich_value::Realm::HostIo,
                        "recorded failure",
                    ));
                }
                *skip -= 1;
            }
        }
        match call.target.as_str() {
            "db.getDatabase" => Ok(obj(&[("sourceDatabase", s("txIndex"))])),
            "db.findOneAsync" => Ok(self
                .rows
                .iter()
                .find(|row| row.get("_id") == call.arguments[1].get("_id"))
                .cloned()
                .unwrap_or(V::Null)),
            "db.insertAsync" => {
                let row = call.arguments[1].clone();
                self.rows.push(row.clone());
                Ok(row)
            }
            other => panic!("unowned data operation {other}"),
        }
    }
}
impl SourceBackend for Db {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        Ok(match self.invoke_raw(call.clone()) {
            Ok(v) => DependencyReturn::Pending(v),
            Err(e) => DependencyReturn::PendingThrow(SourceException::from(e)),
        })
    }
}
impl SourceIndexMetadataIo for Db {
    type Error = CreatedThrown;
    fn database(&mut self) -> Result<V, Self::Error> {
        self.invoke_raw(DependencyCall {
            target: "db.getDatabase".into(),
            arguments: vec![s("txIndex")],
        })
    }
    fn find_one(&mut self, h: &V, id: &str) -> Result<V, Self::Error> {
        self.invoke_raw(DependencyCall {
            target: "db.findOneAsync".into(),
            arguments: vec![h.clone(), obj(&[("_id", s(id))])],
        })
    }
    fn insert_genesis(&mut self, h: &V, height: V) -> Result<V, Self::Error> {
        self.invoke_raw(DependencyCall {
            target: "db.insertAsync".into(),
            arguments: vec![
                h.clone(),
                obj(&[("_id", s("genesisBlock")), ("value", height)]),
            ],
        })
    }
    fn log_values(&mut self, prefix: &str, _: V) -> Result<(), Self::Error> {
        self.logs.push((prefix.into(), None));
        Ok(())
    }
    fn log_throw(&mut self, prefix: &str, error: &Self::Error) -> Result<(), Self::Error> {
        self.logs.push((prefix.into(), Some(error.clone())));
        Ok(())
    }
}
#[test]
fn shared_metadata_bodies_preserve_actual_legacy_owner_io_outcomes_and_rows() {
    for method in ["findMaxIndexedBlock", "initializeIndex"] {
        for rows in [
            vec![],
            vec![obj(&[("_id", s("MaxHeight")), ("value", V::number(7.))])],
            vec![obj(&[("_id", s("genesisBlock")), ("value", V::number(5.))])],
        ] {
            for fail in [
                None,
                Some(("db.getDatabase", 0)),
                Some(("db.findOneAsync", 0)),
                Some(("db.insertAsync", 0)),
            ] {
                let mut old = Db {
                    rows: rows.clone(),
                    fail,
                    seen: vec![],
                    logs: vec![],
                };
                let mut rich = old.clone();
                let mut owner = tradelayer_core::txindex_source_owner::TxIndexSourceOwner::new();
                let before = owner.clone();
                let actual = owner
                    .evaluate(
                        None,
                        &SourceCall {
                            method: method.into(),
                            arguments: if method == "initializeIndex" {
                                vec![V::number(10.)]
                            } else {
                                vec![]
                            },
                        },
                        &mut old,
                    )
                    .unwrap();
                let choice = if method == "initializeIndex" {
                    source_index_metadata_algorithms::SourceIndexMetadataMethod::Initialize
                } else {
                    source_index_metadata_algorithms::SourceIndexMetadataMethod::FindMax
                };
                let result = match choice {
                    source_index_metadata_algorithms::SourceIndexMetadataMethod::Initialize => {
                        source_index_metadata_algorithms::source_initialize_genesis(
                            &mut rich,
                            V::number(10.),
                        )
                    }
                    source_index_metadata_algorithms::SourceIndexMetadataMethod::FindMax => {
                        source_index_metadata_algorithms::source_find_max(&mut rich)
                    }
                };
                let projected = match &result {
                    Ok(v) => SourceOutcome::Returned(v.clone()),
                    Err(e) => SourceOutcome::Thrown(e.clone().into()),
                };
                assert_eq!(actual.outcome, projected, "{method} {fail:?}");
                assert_eq!(
                    old.seen, rich.seen,
                    "same complete dependency arguments/order"
                );
                assert_eq!(old.rows, rich.rows, "same actual row-before-error prefix");
                assert_eq!(
                    owner, before,
                    "these original metadata methods do not allocate another owner"
                );
                if let Err(e) = result {
                    assert!(e.creation().is_some());
                    assert_eq!(
                        e.creation().unwrap().constructor(),
                        &source_rich_value::NativeKind::Error
                    );
                    assert_eq!(e.realm(), Some(&source_rich_value::Realm::HostIo));
                    if let Some((_, Some(logged))) = rich.logs.last() {
                        assert!(
                            std::sync::Arc::ptr_eq(
                                e.creation().unwrap(),
                                logged.creation().unwrap()
                            ),
                            "log and rethrow preserve the exact creation"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn metadata_catch_boundaries_match_complete_original_module_with_throwing_console() {
    struct BoundaryIo {
        existing: bool,
        rows: usize,
        failures: &'static [&'static str],
        events: Vec<&'static str>,
        first: CreatedThrown,
        replacement: CreatedThrown,
    }
    impl BoundaryIo {
        fn step(&mut self, label: &'static str) -> Result<(), CreatedThrown> {
            self.events.push(label);
            if self.failures.contains(&label) {
                return Err(if label.starts_with("error_") {
                    self.replacement.clone()
                } else {
                    self.first.clone()
                });
            }
            Ok(())
        }
    }
    impl SourceIndexMetadataIo for BoundaryIo {
        type Error = CreatedThrown;
        fn database(&mut self) -> Result<V, Self::Error> {
            self.step("factory")?;
            Ok(V::Undefined)
        }
        fn find_one(&mut self, _: &V, _: &str) -> Result<V, Self::Error> {
            self.step("find")?;
            Ok(if self.existing {
                obj(&[("value", V::number(7.))])
            } else {
                V::Null
            })
        }
        fn insert_genesis(&mut self, _: &V, _: V) -> Result<V, Self::Error> {
            self.step("insert")?;
            self.rows += 1;
            Ok(V::Undefined)
        }
        fn log_values(&mut self, prefix: &str, _: V) -> Result<(), Self::Error> {
            self.step(match prefix {
                "Genesis block is already initialized:" => "log_existing",
                "Genesis block initialized:" => "log_insert",
                _ => panic!("unexpected console message {prefix}"),
            })
        }
        fn log_throw(&mut self, prefix: &str, error: &Self::Error) -> Result<(), Self::Error> {
            assert!(std::sync::Arc::ptr_eq(
                error.creation().unwrap(),
                self.first.creation().unwrap()
            ));
            self.step(match prefix {
                "Error checking for existing genesis block:" => "error_existing",
                "Error initializing genesis block:" => "error_insert",
                "Error finding MaxIndexedBlock:" => "error_max",
                _ => panic!("unexpected console message {prefix}"),
            })
        }
    }
    let oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/txindex_metadata_catch_observer.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let cases: [(&str, bool, &'static [&'static str]); 8] = [
        ("existing_log", true, &["log_existing"]),
        ("insert_log", false, &["log_insert"]),
        ("factory", false, &["factory"]),
        ("find", false, &["find"]),
        ("insert", false, &["insert"]),
        ("catch_replaces", false, &["find", "error_existing"]),
        ("max_find", true, &["find"]),
        ("max_catch_replaces", true, &["find", "error_max"]),
    ];
    let mut actual = Vec::new();
    for (id, existing, failures) in cases {
        let make_error = || {
            CreatedThrown::native(
                source_rich_value::NativeKind::Error,
                source_rich_value::Realm::HostIo,
                "recorded boundary failure",
            )
        };
        let mut io = BoundaryIo {
            existing,
            rows: usize::from(existing),
            failures,
            events: Vec::new(),
            first: make_error(),
            replacement: make_error(),
        };
        let result = if id.starts_with("max_") {
            source_index_metadata_algorithms::source_find_max(&mut io)
        } else {
            source_index_metadata_algorithms::source_initialize_genesis(&mut io, V::number(10.))
        };
        let result = match result {
            Ok(value) => {
                assert_eq!(value, V::Undefined);
                "returned"
            }
            Err(error) => {
                if std::sync::Arc::ptr_eq(error.creation().unwrap(), io.first.creation().unwrap()) {
                    "first"
                } else {
                    assert!(std::sync::Arc::ptr_eq(
                        error.creation().unwrap(),
                        io.replacement.creation().unwrap()
                    ));
                    "replacement"
                }
            }
        };
        actual.push(format!(
            "{id}\t{result}\t{}\t{}",
            io.rows,
            io.events.join(",")
        ));
    }
    let expected = String::from_utf8(oracle.stdout).unwrap();
    assert_eq!(actual.len(), 8);
    assert_eq!(actual.join("\n") + "\n", expected);
}
