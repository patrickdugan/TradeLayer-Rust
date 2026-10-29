use super::*;
use crate::source_compact_index_v2::SourceCompactIndex;
use crate::storage::Namespace;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
include!("txindex_inline_raw_inputs.rs");
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent =
            std::env::var_os("NATIVE_INLINE_TMP").map_or_else(std::env::temp_dir, PathBuf::from);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-inline-source-pc-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(root) = self.root.canonicalize() {
            assert_eq!(root.parent(), Some(self.parent.as_path()));
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
struct Backend {
    index: SourceCompactIndex,
    documents: V,
    write_at: usize,
    fail_at: Option<usize>,
    calls: Vec<DependencyCall>,
}
impl Backend {
    fn new(temp: &Temp, fail_at: Option<usize>) -> Self {
        let mut index = SourceCompactIndex::create(
            &temp.root,
            Namespace::new("LTC", false, "txIndex").unwrap(),
            "declared inline raw metadata",
        )
        .unwrap();
        let V::Array(rows) = raw_native_rows() else {
            panic!("raw rows")
        };
        for row in rows {
            index.import_row(&index.snapshot().unwrap(), row).unwrap();
        }
        Self {
            index,
            documents: raw_documents(),
            write_at: 0,
            fail_at,
            calls: vec![],
        }
    }
    fn injected(&mut self) -> Option<DependencyReturn> {
        let current = self.write_at;
        self.write_at += 1;
        (self.fail_at == Some(current)).then(|| {
            DependencyReturn::PendingThrow(SourceException {
                name: "Error".into(),
                message: format!("recorded native write {current}"),
            })
        })
    }
    fn snapshot_rows(&self) -> Vec<V> {
        let mut rows = vec![];
        self.index
            .visit_rows(|r| {
                rows.push(r.clone());
                Ok(true)
            })
            .unwrap();
        rows
    }
}
impl SourceBackend for Backend {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        self.calls.push(call.clone());
        if call.target == "ClientWrapper.getInstance" {
            return Ok(DependencyReturn::Pending(obj(&[
                ("$rpcClient", V::number(1.)),
                ("chain", s("LTC")),
                ("test", V::Bool(false)),
            ])));
        }
        if call.target == "db.getDatabase" {
            return Ok(DependencyReturn::Pending(obj(&[(
                "$database",
                s("txIndex"),
            )])));
        }
        if let Some(method) = call.target.strip_prefix("db.") {
            if matches!(method, "insertAsync" | "updateAsync") {
                if let Some(f) = self.injected() {
                    return Ok(f);
                }
            }
            return self
                .index
                .native_database(method, &call.arguments[1..])
                .map_err(|_| SourceRuleError::Backend);
        }
        if call.target == "client.getRawTransaction" {
            let id = arg(&call.arguments, 1);
            let returned = if self.documents.get("id") == id {
                if arg(&call.arguments, 2) == V::Bool(false) {
                    self.documents.get("raw")
                } else {
                    self.documents.get("document")
                }
            } else if self.documents.get("parent").get("txid") == id {
                self.documents.get("parent")
            } else {
                return Err(SourceRuleError::UnknownDependency);
            };
            return Ok(DependencyReturn::Pending(returned));
        }
        if call.target == "client.decoderawtransaction" {
            let raw = arg(&call.arguments, 1);
            if self.documents.get("raw") != raw {
                return Err(SourceRuleError::UnknownDependency);
            }
            return Ok(DependencyReturn::Pending(self.documents.get("document")));
        }
        Err(SourceRuleError::UnknownDependency)
    }
}
fn initialized(b: &mut Backend) -> TxIndexSourceOwner {
    let mut owner = TxIndexSourceOwner::new();
    owner.construct();
    owner.load_module(b).unwrap();
    owner
        .evaluate(
            None,
            &SourceCall {
                method: "init".into(),
                arguments: vec![],
            },
            b,
        )
        .unwrap();
    b.calls.clear();
    owner
}
#[test]
fn copied_actual_ordered_bootstrap_accessors_preserve_real_owner_for_helper_link() {
    let client = obj(&[("$rpcClient", V::number(1.))]);
    let fields = obj(&[("chain", s("LTC")), ("test", V::Bool(false))]);
    let mut owner = TxIndexSourceOwner::new();
    owner.ordered_bootstrap_import_begin().unwrap();
    owner
        .ordered_bootstrap_import_fulfill(client.clone())
        .unwrap();
    owner.ordered_bootstrap_constructor().unwrap();
    assert!(matches!(
        owner
            .ordered_bootstrap_static_fulfill(client.clone(), &fields)
            .unwrap(),
        SourceOutcome::Returned(_)
    ));
    assert_eq!(owner.source_rich_class_client(), &client);
    let mut rejected = TxIndexSourceOwner::new();
    rejected.ordered_bootstrap_import_begin().unwrap();
    rejected
        .ordered_bootstrap_import_reject(SourceException {
            name: "Error".into(),
            message: "recorded bootstrap input".into(),
        })
        .unwrap();
    assert!(rejected.client_rejection.is_some());
}
fn drive(
    owner: &mut TxIndexSourceOwner,
    c: &mut TxIndexInlineContinuationV1,
    b: &mut Backend,
) -> SourceEvaluation {
    for _ in 0..20 {
        match owner.advance_inline_source(101, c, b).unwrap() {
            TxIndexInlineStopV1::Complete => return c.take_evaluation().unwrap(),
            TxIndexInlineStopV1::AwaitDatabase => {
                let call = c.pending_call().unwrap().clone();
                b.calls.push(call.clone());
                let returned = if let Some(f) = b.injected() {
                    f
                } else {
                    let before = b.index.snapshot().unwrap();
                    let plan = b
                        .index
                        .prepare_inline_database(
                            &before,
                            call.target.strip_prefix("db.").unwrap(),
                            call.arguments[1..].to_vec(),
                        )
                        .unwrap();
                    assert_eq!(
                        b.index.snapshot().unwrap(),
                        before,
                        "prepare must not write"
                    );
                    b.index.append_inline_database(&plan, None).unwrap();
                    assert_eq!(
                        owner.advance_inline_source(101, c, b).unwrap(),
                        TxIndexInlineStopV1::AwaitDatabase,
                        "native append cannot resume source"
                    );
                    b.index.resolve_inline_database_post(&plan).unwrap()
                };
                owner.resolve_inline_source(101, c, returned).unwrap();
            }
        }
    }
    panic!("bounded genuine source method did not complete")
}
#[test]
fn inline_process_block_preserves_real_decode_once_catch_and_two_height_publications() {
    for failure in [None, Some(0), Some(1), Some(2)] {
        let old_temp = Temp::new();
        let new_temp = Temp::new();
        let mut old = Backend::new(&old_temp, failure);
        let mut new = Backend::new(&new_temp, failure);
        let mut old_owner = initialized(&mut old);
        let mut new_owner = initialized(&mut new);
        let arguments = vec![raw_block(), V::number(11.)];
        let expected = old_owner
            .evaluate(
                None,
                &SourceCall {
                    method: "processBlockData".into(),
                    arguments: arguments.clone(),
                },
                &mut old,
            )
            .unwrap();
        let mut c = new_owner
            .begin_inline_source(101, TxIndexInlineMethodV1::ProcessBlock, arguments)
            .unwrap();
        let actual = drive(&mut new_owner, &mut c, &mut new);
        assert_eq!(
            actual, expected,
            "complete actual owner evaluation {failure:?}"
        );
        assert_eq!(new.calls, old.calls, "all ordered dependencies {failure:?}");
        assert_eq!(
            new.snapshot_rows(),
            old.snapshot_rows(),
            "all actual native rows {failure:?}"
        );
        assert_eq!(new_owner, old_owner);
        if failure == Some(0) {
            assert_eq!(
                new.index.get("MaxHeight").unwrap().unwrap().get("value"),
                V::number(11.)
            );
            assert_eq!(
                new.index.get("indexExists").unwrap().unwrap().get("value"),
                V::Bool(true)
            );
        }
        if failure == Some(2) {
            assert_eq!(
                new.index.get("MaxHeight").unwrap().unwrap().get("value"),
                V::number(11.)
            );
            assert!(new.index.get("indexExists").unwrap().is_none());
        }
    }
}
#[test]
fn inline_save_and_genesis_match_actual_owner_outcomes_and_failure_prefixes() {
    for (method, kind, instance) in [
        ("saveMaxHeight", TxIndexInlineMethodV1::SaveMaxHeight, None),
        ("initializeIndex", TxIndexInlineMethodV1::Initialize, None),
        (
            "initializeOrLoadDB",
            TxIndexInlineMethodV1::Initialize,
            Some(1),
        ),
    ] {
        for failure in [None, Some(0), Some(1)] {
            let a = Temp::new();
            let b = Temp::new();
            let mut old = Backend::new(&a, failure);
            let mut new = Backend::new(&b, failure);
            let mut old_owner = initialized(&mut old);
            let mut owner = initialized(&mut new);
            let arguments = vec![V::number(if method == "initializeOrLoadDB" {
                2819000.
            } else {
                11.
            })];
            let expected = old_owner
                .evaluate(
                    instance,
                    &SourceCall {
                        method: method.into(),
                        arguments: arguments.clone(),
                    },
                    &mut old,
                )
                .unwrap();
            let mut c = owner.begin_inline_source(101, kind, arguments).unwrap();
            let actual = drive(&mut owner, &mut c, &mut new);
            assert_eq!(
                actual, expected,
                "source outcome/dependencies {method}/{failure:?}"
            );
            assert_eq!(new.calls, old.calls);
            assert_eq!(new.snapshot_rows(), old.snapshot_rows());
        }
    }
}
#[test]
fn inline_pending_write_cannot_redecode_or_change_scope_before_resolution() {
    let temp = Temp::new();
    let mut backend = Backend::new(&temp, None);
    let mut owner = initialized(&mut backend);
    let mut c = owner
        .begin_inline_source(
            101,
            TxIndexInlineMethodV1::ProcessBlock,
            vec![raw_block(), V::number(11.)],
        )
        .unwrap();
    assert_eq!(
        owner
            .advance_inline_source(101, &mut c, &mut backend)
            .unwrap(),
        TxIndexInlineStopV1::AwaitDatabase
    );
    let calls = backend.calls.clone();
    let before = backend.snapshot_rows();
    let pending = c.pending_call().unwrap().clone();
    let mut fields = vec![];
    c.visit_fields(&mut |k, v| fields.push((k.to_string(), v.clone())));
    for _ in 0..3 {
        assert_eq!(
            owner
                .advance_inline_source(101, &mut c, &mut backend)
                .unwrap(),
            TxIndexInlineStopV1::AwaitDatabase
        );
    }
    assert_eq!(backend.calls, calls);
    assert_eq!(backend.snapshot_rows(), before);
    assert_eq!(c.pending_call(), Some(&pending));
    assert!(owner
        .advance_inline_source(102, &mut c, &mut backend)
        .is_err());
    let mut after = vec![];
    c.visit_fields(&mut |k, v| after.push((k.to_string(), v.clone())));
    assert_eq!(after, fields);
    // Apply the actual bytes, but withhold the original source return.
    let plan = backend
        .index
        .prepare_inline_database(
            &backend.index.snapshot().unwrap(),
            "updateAsync",
            pending.arguments[1..].to_vec(),
        )
        .unwrap();
    backend.index.append_inline_database(&plan, None).unwrap();
    assert_eq!(
        backend
            .index
            .get("MaxHeight")
            .unwrap()
            .unwrap()
            .get("value"),
        V::number(10.)
    );
    assert!(backend.index.get("indexExists").unwrap().is_none());
    let mut fields_after_append = vec![];
    c.visit_fields(&mut |k, v| fields_after_append.push((k.to_string(), v.clone())));
    assert_eq!(fields_after_append, fields);
    assert_eq!(
        owner
            .advance_inline_source(101, &mut c, &mut backend)
            .unwrap(),
        TxIndexInlineStopV1::AwaitDatabase
    );
    let returned = backend.index.resolve_inline_database_post(&plan).unwrap();
    owner.resolve_inline_source(101, &mut c, returned).unwrap();
    // The same method resumes at its first height write, without new RPC/decoder work.
    assert_eq!(
        owner
            .advance_inline_source(101, &mut c, &mut backend)
            .unwrap(),
        TxIndexInlineStopV1::AwaitDatabase
    );
    assert_eq!(
        c.pending_call().unwrap().arguments[1].get("_id"),
        s("MaxHeight")
    );
    assert_eq!(
        backend
            .calls
            .iter()
            .filter(|c| c.target == "client.decoderawtransaction")
            .count(),
        1
    );
}
