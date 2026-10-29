use crate::compact_candidate_snapshot::*;
use crate::issuance_intent::JsonValue as J;
use crate::source_compact_index_v2::*;
use crate::storage::Namespace;
use crate::txindex_source_owner::TxIndexSourceOwner;
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceEvaluation, SourceException,
    SourceOutcome, SourceRuleError, SourceValue as V,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
fn s(t: impl Into<String>) -> V {
    V::String(t.into())
}
fn o(f: &[(&str, V)]) -> V {
    V::Object(f.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn array(v: &V) -> Vec<V> {
    if let V::Array(a) = v {
        a.clone()
    } else {
        panic!("array: {v:?}")
    }
}
fn text(v: &V) -> String {
    match v {
        V::String(t) => t.clone(),
        _ => panic!("text: {v:?}"),
    }
}
fn field(v: &V, k: &str) -> bool {
    matches!(v,V::Object(f)if f.iter().any(|(n,_)|n==k))
}
fn parse(t: &str) -> V {
    fn convert(v: V) -> V {
        match v {
            V::Object(f) => {
                if f.len() == 1 {
                    match f[0].0.as_str() {
                        "$undefined" => return V::Undefined,
                        "$promise" => return V::Promise,
                        "$number" => {
                            return V::number(match text(&f[0].1).as_str() {
                                "NaN" => f64::NAN,
                                "Infinity" => f64::INFINITY,
                                "-Infinity" => f64::NEG_INFINITY,
                                "-0" => -0.0,
                                _ => panic!(),
                            })
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
    convert(V::from_json(&J::parse(t).unwrap()).unwrap())
}
fn project(v: V) -> V {
    match v {
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
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "tl-compact-source-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let base = std::env::temp_dir().canonicalize().unwrap();
        if let Ok(target) = self.0.canonicalize() {
            assert_eq!(target.parent(), Some(base.as_path()));
            assert!(target
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("tl-compact-source-"));
            let _ = fs::remove_dir_all(target);
        }
    }
}
fn namespace() -> Namespace {
    Namespace::new("BTC", true, "txIndex").unwrap()
}
fn store(t: &Temp) -> SourceCompactIndex {
    SourceCompactIndex::create(
        &t.0,
        namespace(),
        "regtest/anchor9/candidate-profile-native-v1/source-profile-v1",
    )
    .unwrap()
}
struct Inputs {
    rpc: V,
}
impl Default for Inputs {
    fn default() -> Self {
        Self { rpc: V::Undefined }
    }
}
impl SourceBackend for Inputs {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let a = &call.arguments;
        let id = || a.get(1).cloned().unwrap_or(V::Undefined);
        let missing = |message: String| {
            Ok(DependencyReturn::PendingThrow(SourceException {
                name: "Error".into(),
                message,
            }))
        };
        if call.target == "ClientWrapper.getInstance" {
            return Ok(DependencyReturn::Pending(o(&[
                ("$rpcClient", V::number(1.0)),
                ("chain", s("LTC")),
            ])));
        }
        let blocks = if let V::Array(v) = self.rpc.get("blocks") {
            v
        } else {
            vec![]
        };
        let txs = if let V::Array(v) = self.rpc.get("transactions") {
            v
        } else {
            vec![]
        };
        let v = match call.target.as_str() {
            "client.getBlockCount" => self.rpc.get("tip"),
            "client.getBlockHash" => {
                if let Some(b) = blocks.iter().find(|b| b.get("height") == id()) {
                    b.get("block").get("hash")
                } else {
                    return missing(format!("missing block height {}", text(&id())));
                }
            }
            "client.getBlock" => {
                if let Some(b) = blocks.iter().find(|b| b.get("block").get("hash") == id()) {
                    b.get("block")
                } else {
                    return missing(format!("missing block hash {}", text(&id())));
                }
            }
            "client.getRawTransaction" => {
                if let Some(t) = txs.iter().find(|t| t.get("id") == id()) {
                    t.get(if a.get(2) == Some(&V::Bool(false)) {
                        "hex"
                    } else {
                        "decoded"
                    })
                } else {
                    return missing(format!("missing raw transaction {}", text(&id())));
                }
            }
            "client.decoderawtransaction" => {
                if let Some(t) = txs.iter().find(|t| t.get("hex") == id()) {
                    t.get("decoded")
                } else {
                    return missing(format!("missing raw decoder input {}", text(&id())));
                }
            }
            _ => return Err(SourceRuleError::UnknownDependency),
        };
        Ok(DependencyReturn::Pending(v))
    }
}
struct Backend<'a> {
    store: &'a mut SourceCompactIndex,
    inputs: &'a mut Inputs,
    failures: &'a mut Vec<V>,
}
impl SourceBackend for Backend<'_> {
    fn invoke(&mut self, c: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if let Some(i) = self
            .failures
            .iter()
            .position(|f| f.get("target") == s(&c.target))
        {
            if let V::Number(b) = self.failures[i].get("skip") {
                let n = f64::from_bits(b);
                if n > 0.0 {
                    self.failures[i].insert("skip", V::number(n - 1.0))?;
                } else {
                    let f = self.failures.remove(i);
                    return Ok(DependencyReturn::PendingThrow(SourceException {
                        name: "Error".into(),
                        message: text(&f.get("message")),
                    }));
                }
            }
        }
        CompactTxIndexBackend {
            metadata: self.store,
            inputs: self.inputs,
        }
        .invoke(c)
    }
}
fn events(e: &SourceEvaluation) -> V {
    V::Array(
        e.dependencies
            .iter()
            .map(|d| {
                let target = &d.call.target;
                let args = if target.starts_with("client.")
                    || (target.starts_with("db.") && target != "db.getDatabase")
                {
                    d.call.arguments[1..].to_vec()
                } else {
                    d.call.arguments.clone()
                };
                let mut event = o(&[("target", s(target)), ("args", project(V::Array(args)))]);
                if target == "Types.decodePayload" {
                    event.insert("returned", V::Promise).unwrap();
                } else {
                    match &d.returned {
                        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
                            event.insert("returned", project(v.clone())).unwrap()
                        }
                        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => event
                            .insert(
                                "thrown",
                                o(&[("name", s(&e.name)), ("message", s(&e.message))]),
                            )
                            .unwrap(),
                    }
                }
                event
            })
            .collect(),
    )
}
fn rows(store: &SourceCompactIndex) -> Vec<V> {
    let mut rows = vec![];
    store
        .visit_rows(|row| {
            rows.push(row.clone());
            Ok(true)
        })
        .unwrap();
    rows
}
fn corpus(file: &str) {
    let mut temp = Temp::new();
    let mut store = store(&temp);
    let mut owner = TxIndexSourceOwner::new();
    let mut inputs = Inputs::default();
    let mut failures = vec![];
    owner
        .load_module(&mut Backend {
            store: &mut store,
            inputs: &mut inputs,
            failures: &mut failures,
        })
        .unwrap();
    for line in fs::read_to_string(file).unwrap().lines() {
        let r = parse(line);
        let case = text(&r.get("case"));
        let c = r.get("command");
        let scope = c.get("scope");
        if scope == s("reset") {
            drop(store);
            temp = Temp::new();
            store = SourceCompactIndex::create(
                &temp.0,
                namespace(),
                "regtest/anchor9/candidate-profile-native-v1/source-profile-v1",
            )
            .unwrap();
            owner = TxIndexSourceOwner::new();
            inputs.rpc = c.get("configure").get("rpc");
            failures.clear();
            owner
                .load_module(&mut Backend {
                    store: &mut store,
                    inputs: &mut inputs,
                    failures: &mut failures,
                })
                .unwrap();
            continue;
        }
        let cfg = c.get("configure");
        if field(&cfg, "rpc") {
            inputs.rpc = cfg.get("rpc");
        }
        if field(&cfg, "failures") {
            failures = array(&cfg.get("failures"));
        }
        if scope == s("constructor") {
            assert_eq!(
                owner.construct(),
                r.get("outcome").get("returned"),
                "{case}"
            );
        } else {
            let method = text(&c.get("method"));
            let before = store.snapshot().unwrap();
            let old_owner = owner.clone();
            let e = owner.evaluate(
                if scope == s("instance") {
                    Some(1)
                } else {
                    None
                },
                &SourceCall {
                    method: method.clone(),
                    arguments: array(&c.get("args")),
                },
                &mut Backend {
                    store: &mut store,
                    inputs: &mut inputs,
                    failures: &mut failures,
                },
            );
            if c.get("domain") == s("logical-error-storage-unsupported") {
                assert_eq!(e, Err(SourceRuleError::UnsupportedNumericDomain), "{case}");
                assert_eq!(store.snapshot().unwrap(), before);
                assert_eq!(owner, old_owner);
                let expected = array(&r.get("state").get("rows"));
                assert!(
                    expected
                        .iter()
                        .any(|row| row.get("value").get("sender") == V::object()),
                    "actual NeDB copied the genuine returned Error to an empty object"
                );
                continue;
            }
            let e = e.unwrap_or_else(|e| panic!("{case}: {e:?}"));
            let outcome = match &e.outcome {
                SourceOutcome::Returned(v) => o(&[
                    (
                        "transport",
                        s(if method == "isPrintableASCII" {
                            "Immediate"
                        } else {
                            "Pending"
                        }),
                    ),
                    ("returned", project(v.clone())),
                ]),
                SourceOutcome::Thrown(e) => o(&[
                    (
                        "transport",
                        s(if method == "isPrintableASCII" {
                            "Throw"
                        } else {
                            "PendingThrow"
                        }),
                    ),
                    (
                        "thrown",
                        o(&[("name", s(&e.name)), ("message", s(&e.message))]),
                    ),
                ]),
            };
            assert_eq!(outcome, r.get("outcome"), "outcome {case}");
            assert_eq!(
                events(&e),
                r.get("effects"),
                "ordered dependency effects {case}"
            );
        }
        assert_eq!(
            V::Array(rows(&store)),
            r.get("state").get("rows"),
            "metadata authority {case}"
        );
        let V::Object(owner_state) = project(owner.state_value().unwrap()) else {
            panic!("owner state");
        };
        for (key, value) in owner_state {
            if key != "moduleLoaded" && key != "clientPromiseRejection" {
                assert_eq!(
                    value,
                    r.get("state").get(&key),
                    "actual source owner field {key} at {case}"
                );
            }
        }
        assert_eq!(
            V::Array(failures.clone()),
            r.get("state").get("failures"),
            "failure prefix {case}"
        );
        // Every frame is reopened from actual durable bytes. Same owner checkpoint,
        // independently retained metadata sequence/context: no supplied poststate.
        let pin = store.snapshot().unwrap();
        let cache = owner.checkpoint().unwrap();
        drop(store);
        store = SourceCompactIndex::open(&temp.0, namespace(), &pin).unwrap();
        owner = TxIndexSourceOwner::from_checkpoint(&cache).unwrap();
    }
}
fn stage() -> PathBuf {
    std::env::var_os("TL_COMPACT_SOURCE_STAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(option_env!("CARGO_MANIFEST_DIR").unwrap_or(".")))
}
#[test]
fn unchanged_first_ten_over_real_native_pages() {
    corpus(
        stage()
            .join("fixtures/txindex_source_proof10.jsonl")
            .to_str()
            .unwrap(),
    );
}
#[test]
fn genuine_literal_queries_updates_source_failures_and_rpc_selection() {
    corpus(
        stage()
            .join("fixtures/compact_adapter_vectors.jsonl")
            .to_str()
            .unwrap(),
    );
}
#[test]
fn paged_migration_exceeds_core_and_per_key_row_limits_and_retains_order() {
    let t = Temp::new();
    let mut store = store(&t);
    for n in (0..4200).rev() {
        let row = o(&[
            ("_id", s(format!("tx-{n:06}-id{n:06}"))),
            ("value", V::number(n as f64)),
        ]);
        let pin = store.snapshot().unwrap();
        store.import_row(&pin, row).unwrap();
    }
    assert_eq!(store.len().unwrap(), 4200);
    let mut count = 0;
    store
        .visit_rows(|r| {
            assert_eq!(r.get("value"), V::number(count as f64));
            count += 1;
            Ok(true)
        })
        .unwrap();
    assert_eq!(count, 4200);
    let mut b = CompactTxIndexBackend {
        metadata: &mut store,
        inputs: &mut Inputs::default(),
    };
    let call = DependencyCall {
        target: "db.findAsync".into(),
        arguments: vec![o(&[("$database", s("txIndex"))]), V::object()],
    };
    assert_eq!(b.invoke(&call), Err(SourceRuleError::Capacity));
    let pin = store.snapshot().unwrap();
    drop(store);
    let reopened = SourceCompactIndex::open(&t.0, namespace(), &pin).unwrap();
    assert_eq!(
        reopened
            .get("tx-004199-id004199")
            .unwrap()
            .unwrap()
            .get("value"),
        V::number(4199.0)
    );
}
#[test]
fn migration_conflict_snapshot_cas_and_explicit_metadata_reorg() {
    let t = Temp::new();
    let mut store = store(&t);
    let original = store.snapshot().unwrap();
    let row = o(&[("_id", s("tx-10-a")), ("value", V::number(1.0))]);
    let first = store.import_row(&original, row.clone()).unwrap();
    assert_eq!(store.import_row(&first, row).unwrap(), first);
    assert!(matches!(
        store.import_row(&original, o(&[("_id", s("tx-11-b"))])),
        Err(CompactIndexError::StaleSnapshot)
    ));
    assert!(matches!(
        store.import_row(
            &first,
            o(&[("_id", s("tx-10-a")), ("value", V::number(2.0))])
        ),
        Err(CompactIndexError::WrongContext)
    ));
    let second = store
        .import_row(
            &first,
            o(&[("_id", s("tx-11-b")), ("value", V::number(3.0))]),
        )
        .unwrap();
    let after = store
        .discard_rows(&second, &["tx-11-b".into()], "regtest/explicit-fork-B")
        .unwrap();
    assert_eq!(store.len().unwrap(), 1);
    assert!(store.get("tx-11-b").unwrap().is_none());
    assert!(store.get("tx-10-a").unwrap().is_some());
    assert!(matches!(
        store.require_snapshot(&second),
        Err(CompactIndexError::WrongContext)
    ));
    drop(store);
    let reopened = SourceCompactIndex::open(&t.0, namespace(), &after).unwrap();
    assert_eq!(reopened.len().unwrap(), 1);
}
#[test]
fn actual_native_writer_blocks_expose_every_raw_transaction_without_candidate_filter() {
    for name in [
        "non-tl-hides-later-tl",
        "first-tl-wins",
        "empty-nulldata-hides-later",
        "nonminimal-pushdata1",
        "nonminimal-pushdata2",
        "two-pushes",
        "printable-boundary",
        "nonprintable",
        "no-tl",
        "tlr-is-literal-ascii",
    ] {
        let directory = stage().join("fixtures/source_compact_index").join(name);
        let manifest = fs::read(directory.join("writer-CURRENT.txt")).unwrap();
        let text = std::str::from_utf8(&manifest).unwrap();
        let segment = text
            .lines()
            .find(|l| l.starts_with("B\t"))
            .unwrap()
            .split('\t')
            .nth(5)
            .unwrap();
        let t = Temp::new();
        fs::create_dir(t.0.join("segments")).unwrap();
        fs::copy(
            directory.join("writer-segment.dat"),
            t.0.join("segments").join(segment),
        )
        .unwrap();
        fs::write(t.0.join("CURRENT"), &manifest).unwrap();
        let pin = CandidateSnapshotExpectation {
            profile: CompactCandidateProfile::NativeV1BitcoinCanonicalAllOutputs,
            network: CandidateNetwork::Regtest,
            anchor_height: 9,
            anchor_hash_display: [0x66; 32],
            generation: 1,
            manifest_sha256d: crate::hash256::double_sha256(&manifest),
        };
        let snapshot = CompactCandidateSnapshot::open(&t.0, pin).unwrap();
        let raw = crate::read_only::decode_hex(
            fs::read_to_string(directory.join("block.hex"))
                .unwrap()
                .trim(),
        )
        .unwrap();
        let source = SourceCompactBlock::read(&snapshot, 10, &raw).unwrap();
        assert_eq!(source.profile(), DISCOVERY_PROFILE);
        assert_eq!(source.raw_transactions().count(), 1, "{name}");
        let (_, tx) = source.raw_transactions().next().unwrap();
        assert_eq!(tx, &raw[81..], "actual same block bytes {name}");
        if name == "nonminimal-pushdata1" {
            assert!(source.canonical_evidence().outputs().is_empty());
        }
        if name == "first-tl-wins" {
            assert_eq!(source.canonical_evidence().outputs().len(), 2);
        }
        assert!(source
            .canonical_evidence()
            .require_authenticated_actor()
            .is_err());
        assert!(source
            .canonical_evidence()
            .require_authenticated_chain()
            .is_err());
        assert!(source
            .canonical_evidence()
            .require_tradelayer_transition()
            .is_err());
    }
}

#[test]
fn native_uncertain_publication_poison_and_explicit_tail_recovery() {
    use crate::compact_journal::CompactingJournal as Journal;
    use crate::storage::{Recovery, StorageError};
    let t = Temp::new();
    let mut metadata = store(&t);
    let before = metadata.snapshot().unwrap();
    let row = o(&[("_id", s("tx-10-a")), ("value", V::number(1.0))]);
    metadata.test_fail_after_sync();
    assert!(matches!(
        metadata.import_row(&before, row.clone()),
        Err(CompactIndexError::Storage(StorageError::Io(_)))
    ));
    assert!(matches!(
        metadata.snapshot(),
        Err(CompactIndexError::Storage(StorageError::Poisoned))
    ));
    assert!(metadata.get("tx-10-a").is_err());
    assert!(metadata.get("absent").is_err());
    assert!(metadata.len().is_err());
    assert!(metadata
        .visit_rows(|_| panic!("poisoned row must not escape"))
        .is_err());
    drop(metadata);
    assert!(matches!(
        SourceCompactIndex::open(&t.0, namespace(), &before),
        Err(CompactIndexError::StaleSnapshot)
    ));
    // This injected failpoint ran after real sync. The test independently
    // expects the issued publication and its exact original document.
    let mut published = before.clone();
    published.sequence += 1;
    let mut metadata = SourceCompactIndex::open(&t.0, namespace(), &published).unwrap();
    assert_eq!(metadata.get("tx-10-a").unwrap(), Some(row.clone()));
    assert_eq!(metadata.import_row(&published, row).unwrap(), published);
    metadata.test_fail_after_partial_write();
    assert!(metadata
        .import_row(
            &published,
            o(&[("_id", s("tx-11-b")), ("value", V::number(2.0))])
        )
        .is_err());
    assert!(metadata.snapshot().is_err());
    drop(metadata);
    assert!(matches!(
        SourceCompactIndex::open(&t.0, namespace(), &published),
        Err(CompactIndexError::Storage(
            StorageError::IncompleteTail { .. }
        ))
    ));
    // Repair is an explicit existing storage operation, never automatic open.
    let (journal, recovery) =
        Journal::open_existing(&t.0, namespace(), Recovery::DiscardIncompleteTail).unwrap();
    assert!(recovery.discarded_bytes > 0);
    assert_eq!(journal.sequence().unwrap(), published.sequence);
    drop(journal);
    let metadata = SourceCompactIndex::open(&t.0, namespace(), &published).unwrap();
    assert!(metadata.get("tx-10-a").unwrap().is_some());
    assert!(metadata.get("tx-11-b").unwrap().is_none());
}

#[test]
fn metadata_profile_refuses_ambiguous_logical_objects_and_wrong_collection() {
    let t = Temp::new();
    assert!(matches!(
        SourceCompactIndex::create(
            &t.0,
            Namespace::new("BTC", true, "tallyMap").unwrap(),
            "context"
        ),
        Err(CompactIndexError::WrongContext)
    ));
    let mut metadata = store(&t);
    for value in [
        V::BigNumber("1".into()),
        o(&[("$error", s("ambiguous"))]),
        o(&[("__proto__", V::Null)]),
        o(&[("$$date", V::number(1.0))]),
    ] {
        let pin = metadata.snapshot().unwrap();
        assert!(matches!(
            metadata.import_row(&pin, o(&[("_id", s("unsupported")), ("value", value)])),
            Err(CompactIndexError::Source(
                SourceRuleError::UnsupportedNumericDomain
            ))
        ));
        assert_eq!(metadata.snapshot().unwrap(), pin);
    }
}

#[test]
fn complete_stream_exceeds_aggregate_limit_and_is_fixed_during_same_owner_writes() {
    let t = Temp::new();
    let mut metadata = store(&t);
    for n in 0..1025 {
        let pin = metadata.snapshot().unwrap();
        metadata
            .import_row(
                &pin,
                o(&[
                    ("_id", s(format!("tx-10-{n:064x}"))),
                    ("value", V::number(n as f64)),
                ]),
            )
            .unwrap();
    }
    let pin = metadata.snapshot().unwrap();
    let fixed = metadata.read_generation(&pin).unwrap();
    let mut cursor = fixed.cursor();
    let mut inputs = Inputs::default();
    let mut backend = CompactTxIndexBackend {
        metadata: &mut metadata,
        inputs: &mut inputs,
    };
    let update = DependencyCall {
        target: "db.updateAsync".into(),
        arguments: vec![
            o(&[("$database", s("txIndex"))]),
            o(&[("_id", s(format!("tx-10-{:064x}", 0)))]),
            o(&[("$set", o(&[("valid", V::Bool(true))]))]),
            o(&[("upsert", V::Bool(true))]),
        ],
    };
    assert_eq!(
        backend.invoke(&update),
        Ok(DependencyReturn::Pending(V::number(1.0)))
    );
    let all = DependencyCall {
        target: "db.findAsync".into(),
        arguments: vec![o(&[("$database", s("txIndex"))]), V::object()],
    };
    assert_eq!(backend.invoke(&all), Err(SourceRuleError::Capacity));
    assert!(matches!(
        metadata.read_generation(&pin),
        Err(CompactIndexError::StaleSnapshot)
    ));
    for _ in 0..2 {
        let expected = metadata.snapshot().unwrap();
        metadata.compact_metadata(&expected).unwrap();
    }
    assert!(
        metadata.storage_footprint().unwrap().generation_count >= 3,
        "the live original read generation must remain retained beyond the ancestor target"
    );
    for n in 0..1025 {
        let row = cursor.next_row().unwrap().unwrap();
        assert_eq!(row.get("value"), V::number(n as f64));
        assert_eq!(row.get("valid"), V::Undefined);
    }
    assert!(!cursor.receipt().complete);
    assert!(cursor.next_row().unwrap().is_none());
    let receipt = cursor.receipt();
    assert!(receipt.complete);
    assert_eq!(receipt.rows, 1025);
    assert_eq!(receipt.snapshot, pin);
    assert_eq!(receipt.profile, STREAM_PROFILE);
    assert_eq!(
        metadata
            .get(&format!("tx-10-{:064x}", 0))
            .unwrap()
            .unwrap()
            .get("valid"),
        V::Bool(true)
    );
    drop(cursor);
    drop(fixed);
    let expected = metadata.snapshot().unwrap();
    metadata.compact_metadata(&expected).unwrap();
    assert!(metadata.storage_footprint().unwrap().generation_count <= 2);
}

#[test]
fn unchanged_main_grouping_controls_match_complete_snapshot_phase_streams() {
    fn get<'a>(j: &'a J, k: &str) -> &'a J {
        let J::Object(o) = j else {
            panic!("object");
        };
        o.get(k).unwrap()
    }
    fn num(j: &J) -> u64 {
        let J::Number(n) = j else {
            panic!("number");
        };
        n.as_f64() as u64
    }
    fn text_j(j: &J) -> String {
        let J::String(t) = j else {
            panic!("string");
        };
        t.clone()
    }
    fn phase_digest(rows: &[V]) -> String {
        let mut chain = crate::hash256::double_sha256(STREAM_PROFILE.as_bytes());
        for (i, row) in rows.iter().enumerate() {
            let text = row.diagnostic_json().unwrap();
            let mut bytes = chain.to_vec();
            bytes.extend_from_slice(&(i as u64).to_be_bytes());
            bytes.extend_from_slice(&(text.len() as u32).to_be_bytes());
            bytes.extend_from_slice(text.as_bytes());
            chain = crate::hash256::double_sha256(&bytes);
        }
        chain.iter().map(|b| format!("{b:02x}")).collect()
    }
    let mut cases: Vec<(J, Vec<V>)> = vec![];
    for line in fs::read_to_string(stage().join("fixtures/main_stream_controls.jsonl"))
        .unwrap()
        .lines()
    {
        let j = J::parse(line).unwrap();
        if text_j(get(&j, "kind")) == "case" {
            cases.push((j, vec![]));
        } else {
            let row = V::from_diagnostic_json(&get(&j, "row").to_json().unwrap()).unwrap();
            cases.last_mut().unwrap().1.push(row);
        }
    }
    assert_eq!(cases.len(), 7);
    for (header, inputs) in cases {
        let name = text_j(get(&header, "name"));
        let t = Temp::new();
        let mut metadata = store(&t);
        for row in inputs {
            let pin = metadata.snapshot().unwrap();
            metadata.import_row(&pin, row).unwrap();
        }
        let pin = metadata.snapshot().unwrap();
        let fixed = metadata.read_generation(&pin).unwrap();
        let planned = fixed.plan_main_groups().unwrap();
        let J::Array(expected_phases) = get(&header, "phases") else {
            panic!()
        };
        match planned {
            MainIndexPlanningOutcome::Thrown { exception, receipt } => {
                assert!(receipt.complete);
                assert_eq!(receipt.rows, num(get(&header, "initialRows")));
                assert_eq!(
                    exception.name,
                    text_j(get(get(get(&header, "result"), "thrown"), "name"))
                );
                assert_eq!(
                    exception.message,
                    text_j(get(get(get(&header, "result"), "thrown"), "message"))
                );
                assert!(expected_phases.is_empty(), "{name}");
            }
            MainIndexPlanningOutcome::Ready(plan) => {
                assert!(plan.receipt().complete);
                assert_eq!(plan.receipt().rows, num(get(&header, "initialRows")));
                let actual = plan
                    .receipt()
                    .chain_sha256d
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                assert_eq!(
                    actual,
                    text_j(get(&header, "initialDigest")),
                    "full original query {name}"
                );
                assert!(plan.require_authenticated_admission().is_err());
                let max = plan.max_index_block();
                let active = matches!(max,V::Number(b)if f64::from_bits(b)>0.0&&f64::from_bits(b).is_finite());
                let mut index = 0;
                if active {
                    let V::Number(b) = max else { unreachable!() };
                    let max = f64::from_bits(b) as u64;
                    assert!(max <= 100);
                    for height in 10..=max {
                        if plan.heights().any(|n| n == height) {
                            for funding in [true, false] {
                                let mut cursor = plan.phase(height, funding);
                                let mut rows = vec![];
                                while let Some(row) = cursor.next_row().unwrap() {
                                    rows.push(row);
                                }
                                let expected = &expected_phases[index];
                                assert_eq!(
                                    V::from_diagnostic_json(
                                        &get(expected, "height").to_json().unwrap()
                                    )
                                    .unwrap(),
                                    V::number(height as f64),
                                    "height {name}"
                                );
                                assert_eq!(
                                    rows.len() as u64,
                                    num(get(expected, "rows")),
                                    "phase length {name}"
                                );
                                assert_eq!(
                                    phase_digest(&rows),
                                    text_j(get(expected, "rowStreamDigest")),
                                    "actual original phase byte/order {name}"
                                );
                                index += 1;
                            }
                        }
                    }
                }
                assert_eq!(
                    index,
                    expected_phases.len(),
                    "same actual source processTxSet calls {name}"
                );
            }
        }
    }
}

#[test]
fn streamed_validity_uses_complete_genuine_query_and_first_suffix_without_array_proxy() {
    fn get<'a>(j: &'a J, k: &str) -> &'a J {
        let J::Object(o) = j else { panic!() };
        o.get(k).unwrap()
    }
    fn text_j(j: &J) -> String {
        let J::String(t) = j else { panic!() };
        t.clone()
    }
    fn num(j: &J) -> u64 {
        let J::Number(n) = j else { panic!() };
        n.as_f64() as u64
    }
    fn tagged(j: &J) -> V {
        V::from_diagnostic_json(&j.to_json().unwrap()).unwrap()
    }
    let mut cases: Vec<(J, Vec<V>, Vec<V>)> = vec![];
    for line in fs::read_to_string(stage().join("fixtures/validity_stream_controls.jsonl"))
        .unwrap()
        .lines()
    {
        let j = J::parse(line).unwrap();
        match text_j(get(&j, "kind")).as_str() {
            "case" => cases.push((j, vec![], vec![])),
            "input-row" => cases.last_mut().unwrap().1.push(tagged(get(&j, "row"))),
            "output-row" => cases.last_mut().unwrap().2.push(tagged(get(&j, "row"))),
            _ => panic!(),
        }
    }
    assert_eq!(cases.len(), 7);
    for (header, input, expected_rows) in cases {
        let name = text_j(get(&header, "name"));
        let t = Temp::new();
        let mut metadata = store(&t);
        for row in input {
            let pin = metadata.snapshot().unwrap();
            metadata.import_row(&pin, row).unwrap();
        }
        let args = array(&tagged(get(&header, "args")));
        let pin = metadata.snapshot().unwrap();
        let result = metadata
            .stream_upsert_validity(
                &pin,
                args[0].clone(),
                args[1].clone(),
                args[2].clone(),
                args[3].clone(),
            )
            .unwrap();
        assert!(result.query.complete);
        assert_eq!(result.query.visited_rows, num(get(&header, "initialRows")));
        assert_eq!(result.query.matched_rows, num(get(&header, "matchedRows")));
        assert_eq!(
            result
                .query
                .chain_sha256d
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            text_j(get(&header, "queryDigest")),
            "complete original NeDB query/order {name}"
        );
        let outcome = match result.outcome.clone() {
            SourceOutcome::Returned(v) => o(&[("transport", s("Pending")), ("returned", v)]),
            SourceOutcome::Thrown(e) => o(&[
                ("transport", s("PendingThrow")),
                (
                    "thrown",
                    o(&[("name", s(e.name)), ("message", s(e.message))]),
                ),
            ]),
        };
        assert_eq!(
            outcome,
            parse(&get(&header, "outcome").to_json().unwrap()),
            "actual original upsert outcome {name}"
        );
        assert_eq!(
            rows(&metadata),
            expected_rows,
            "actual original persisted metadata {name}"
        );
        assert_eq!(result.profile, VALIDITY_STREAM_PROFILE);
        assert!(result.require_authenticated_admission().is_err());
        let after = metadata.snapshot().unwrap();
        drop(metadata);
        let restored = SourceCompactIndex::open(&t.0, namespace(), &after).unwrap();
        assert_eq!(rows(&restored), expected_rows);
    }
}
