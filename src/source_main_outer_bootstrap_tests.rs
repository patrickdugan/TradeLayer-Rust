// Actual-Core proposals. These do not choose a Promise schedule or claim full
// Main startup parity; the scheduler owns actual source jobs and adoption.
use super::*;
use crate::storage::Namespace;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let parent =
            std::env::var_os("OUTER_BOOTSTRAP_TMP").map_or_else(std::env::temp_dir, PathBuf::from);
        std::fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!(
            "actual-core-outer-bootstrap-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn seeded(temp: &Temp) -> (SourceContractRuntime, SourceCompactIndex) {
    let mut runtime =
        SourceContractRuntime::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12);
    runtime
        .import_collection(
            "consensus",
            vec![obj(&[("_id", s("MaxProcessedHeight")), ("value", n(9.))])],
        )
        .unwrap();
    runtime
        .import_collection(
            "tallyMap",
            vec![obj(&[
                ("_id", s("tallyMap")),
                (
                    "data",
                    s("[[\"raw-source-address\",{\"1\":{\"available\":7}}]]"),
                ),
            ])],
        )
        .unwrap();
    let mut index = SourceCompactIndex::create(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "declared original outer-bootstrap raw input",
    )
    .unwrap();
    index
        .import_row(
            &index.snapshot().unwrap(),
            obj(&[("_id", s("MaxHeight")), ("value", n(10.))]),
        )
        .unwrap();
    index
        .import_row(
            &index.snapshot().unwrap(),
            obj(&[
                ("_id", s("tx-10-empty")),
                ("value", obj(&[("payload", s(""))])),
            ]),
        )
        .unwrap();
    (runtime, index)
}
fn raw_index_put(index: &mut SourceCompactIndex, id: &str, value: V) {
    index
        .native_database(
            "updateAsync",
            &[
                obj(&[("_id", s(id))]),
                obj(&[("$set", obj(&[("value", value)]))]),
                obj(&[("upsert", V::Bool(true))]),
            ],
        )
        .unwrap();
}
fn resume(
    runtime: &mut SourceContractRuntime,
    frame: &mut NativeMainOuterBootstrapV1,
    index: &SourceCompactIndex,
    lane: NativeMainOuterBootstrapLaneV1,
) {
    let await_ = frame.pending_site(lane).unwrap();
    runtime
        .native_main_outer_bootstrap_resume_actual(frame, await_, index, &mut |parent, scope| {
            assert_eq!(parent, 41);
            assert_eq!(scope, "TallyMap.getInstance/loadFromDB");
            Ok(42)
        })
        .unwrap();
}
fn query(
    runtime: &mut SourceContractRuntime,
    frame: &mut NativeMainOuterBootstrapV1,
    index: &SourceCompactIndex,
    lane: NativeMainOuterBootstrapLaneV1,
) {
    let await_ = frame.pending_site(lane).unwrap();
    let status = frame.query_status(await_).unwrap();
    assert!(!status.sampled);
    assert!(!status.callback_delivered);
    assert!(await_.is_host_query());
    assert_eq!(await_.lane(), lane);
    assert!(await_.ordinal() > 0);
    if !status.request_rejected {
        runtime
            .native_main_outer_bootstrap_query_sample_actual(frame, await_, index)
            .unwrap();
    }
    runtime
        .native_main_outer_bootstrap_query_deliver_callback(frame, await_, index)
        .unwrap();
    assert!(frame.query_status(await_).unwrap().callback_delivered);
    resume(runtime, frame, index, lane);
}
fn through_two_factories(
    runtime: &mut SourceContractRuntime,
    index: &SourceCompactIndex,
) -> NativeMainOuterBootstrapV1 {
    use NativeMainOuterBootstrapLaneV1 as Lane;
    let mut frame = runtime
        .native_main_outer_bootstrap_new(n(10.), 41, index)
        .unwrap();
    runtime
        .native_main_outer_bootstrap_begin(&mut frame)
        .unwrap();
    resume(runtime, &mut frame, index, Lane::Main); // findMax factory reaction
    query(runtime, &mut frame, index, Lane::Main); // actual query + method reaction
    assert_eq!(
        frame.pending_site(Lane::Main).unwrap().site(),
        NativeMainOuterBootstrapSiteV1::FindMaxResult
    );
    resume(runtime, &mut frame, index, Lane::Main); // Main awaits findMax return
    assert!(frame.tally_job.is_none());
    let effect_start = runtime.core.effects.len();
    resume(runtime, &mut frame, index, Lane::Main); // real Tally child and consensus
    assert_eq!(frame.tally_job, Some(42));
    let factories: Vec<_> = runtime.core.effects[effect_start..]
        .iter()
        .filter(|event| event.get("kind") == s("database"))
        .map(|event| event.get("name"))
        .collect();
    assert_eq!(factories, vec![s("tallyMap"), s("consensus")]);
    assert!(frame.tally_query.is_none());
    assert!(frame.consensus_query.is_none());
    frame
}
fn fields(frame: &NativeMainOuterBootstrapV1) -> BTreeMap<String, V> {
    let mut fields = BTreeMap::new();
    frame
        .visit_canonical_fields(&mut |path, value| {
            assert!(
                fields.insert(path.into(), value.clone()).is_none(),
                "{path}"
            );
            Ok(())
        })
        .unwrap();
    fields
}
#[test]
fn actual_outer_fixed_read_and_unawaited_tally_child_keep_their_own_results() {
    use NativeMainOuterBootstrapLaneV1 as Lane;
    let temp = Temp::new();
    let (mut runtime, mut index) = seeded(&temp);
    let mut frame = through_two_factories(&mut runtime, &index);
    assert!(runtime.core.tally.is_empty());
    resume(&mut runtime, &mut frame, &index, Lane::Main); // consensus factory
    query(&mut runtime, &mut frame, &index, Lane::Main); // MaxProcessed source return
    assert_eq!(
        frame.pending_site(Lane::Main).unwrap().site(),
        NativeMainOuterBootstrapSiteV1::MaxProcessedResult
    );
    resume(&mut runtime, &mut frame, &index, Lane::Main); // actual fixed query request
    raw_index_put(
        &mut index,
        "tx-11-before-executor",
        obj(&[("payload", s(""))]),
    );
    let fixed = frame.pending_site(Lane::Main).unwrap();
    runtime
        .native_main_outer_bootstrap_query_sample_actual(&mut frame, fixed, &index)
        .unwrap();
    let retained = frame
        .actual_query_receipt(fixed)
        .unwrap()
        .unwrap()
        .actual_rows()
        .unwrap()
        .clone();
    raw_index_put(
        &mut index,
        "tx-12-after-executor",
        obj(&[("payload", s(""))]),
    );
    runtime
        .native_main_outer_bootstrap_query_deliver_callback(&mut frame, fixed, &index)
        .unwrap();
    assert!(matches!(
        frame.actual_completion(fixed).unwrap(),
        Some(NativeMainOuterActualCompletionV1::FixedRead(read)) if read.actual_rows().unwrap().complete && read.kind()==crate::source_compact_index_v2::NativeMainStartupIndexReadKindV1::Fixed && read.stopped().is_none()
    ));
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    assert!(frame.prepared_ready());
    assert!(frame.grouping().unwrap().complete);
    assert!(frame.storage_stop().is_none());
    assert!(!frame.tally_child_complete());
    assert!(
        runtime.core.tally.is_empty(),
        "unawaited original Tally reaction has not run"
    );
    assert!(
        matches!(frame.actual_completion(fixed).unwrap(),Some(NativeMainOuterActualCompletionV1::FixedRead(read)) if read.actual_rows()==Some(&retained)),
        "late handlers retain the actual issued query result after source advancement"
    );
    assert!(
        runtime
            .native_main_outer_bootstrap_query_sample_actual(&mut frame, fixed, &index)
            .is_err(),
        "an old payload cannot execute IO again"
    );
    let prepared = frame.take_prepared().unwrap().unwrap().unwrap();
    assert_eq!(prepared.last, 11.);
    assert_eq!(prepared.plan.receipt(), &retained);
    assert!(frame.take_prepared().is_err());
    assert_eq!(frame.pending_site(Lane::Tally).unwrap().source_job(), 42);
    resume(&mut runtime, &mut frame, &index, Lane::Tally); // Tally factory reaction
    let tally = frame.pending_site(Lane::Tally).unwrap();
    runtime
        .native_main_outer_bootstrap_query_sample_actual(&mut frame, tally, &index)
        .unwrap();
    runtime
        .native_main_outer_bootstrap_query_deliver_callback(&mut frame, tally, &index)
        .unwrap();
    assert!(
        runtime.core.tally.is_empty(),
        "callback delivery does not execute source continuation"
    );
    resume(&mut runtime, &mut frame, &index, Lane::Tally);
    assert_eq!(runtime.core.tally.len(), 1);
    assert_eq!(runtime.core.tally[0].0, s("raw-source-address"));
    let load = frame.pending_site(Lane::Tally).unwrap();
    assert!(
        matches!(frame.actual_completion(load).unwrap(),Some(NativeMainOuterActualCompletionV1::TallyMap(entries)) if entries.len()==1)
    );
    resume(&mut runtime, &mut frame, &index, Lane::Tally);
    assert!(frame.tally_child_complete());
    let before = fields(&frame);
    assert_eq!(fields(&frame), before);
}
#[test]
fn actual_outer_findmax_and_consensus_sample_later_but_callback_keeps_original_result() {
    use NativeMainOuterBootstrapLaneV1 as Lane;
    let temp = Temp::new();
    let (mut runtime, mut index) = seeded(&temp);
    let mut frame = runtime
        .native_main_outer_bootstrap_new(n(10.), 41, &index)
        .unwrap();
    runtime
        .native_main_outer_bootstrap_begin(&mut frame)
        .unwrap();
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    raw_index_put(&mut index, "MaxHeight", n(11.));
    let pending = frame.pending_site(Lane::Main).unwrap();
    runtime
        .native_main_outer_bootstrap_query_sample_actual(&mut frame, pending, &index)
        .unwrap();
    raw_index_put(&mut index, "MaxHeight", n(12.));
    runtime
        .native_main_outer_bootstrap_query_deliver_callback(&mut frame, pending, &index)
        .unwrap();
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    assert_eq!(frame.last_indexed_block, Some(n(11.)));
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    let pending = frame.pending_site(Lane::Main).unwrap();
    runtime
        .core
        .source_main_height_write("MaxProcessedHeight", n(13.))
        .unwrap();
    runtime
        .native_main_outer_bootstrap_query_sample_actual(&mut frame, pending, &index)
        .unwrap();
    runtime
        .core
        .source_main_height_write("MaxProcessedHeight", n(14.))
        .unwrap();
    runtime
        .native_main_outer_bootstrap_query_deliver_callback(&mut frame, pending, &index)
        .unwrap();
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    assert_eq!(frame.last_consensus_height, Some(n(13.)));
    assert_eq!(runtime.collection("consensus")[0].get("value"), n(14.));
}
#[test]
fn actual_outer_consensus_factory_rejection_keeps_the_started_tally_child_alive() {
    use NativeMainOuterBootstrapLaneV1 as Lane;
    let temp = Temp::new();
    let (mut runtime, index) = seeded(&temp);
    runtime
        .fail_storage(StorageFailure {
            collection: "consensus".into(),
            operation: "getDatabase".into(),
            skip: 0,
            message: "recorded consensus factory rejection".into(),
        })
        .unwrap();
    let mut frame = through_two_factories(&mut runtime, &index);
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    let rejected = frame.pending_site(Lane::Main).unwrap();
    assert_eq!(
        rejected.site(),
        NativeMainOuterBootstrapSiteV1::MaxProcessedResult
    );
    assert!(
        matches!(frame.actual_completion(rejected).unwrap(), Some(NativeMainOuterActualCompletionV1::Thrown(error)) if error.message=="recorded consensus factory rejection")
    );
    assert!(
        !frame.prepared_ready(),
        "the nested function rejection is not yet the Main await reaction"
    );
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    assert!(
        matches!(frame.take_prepared().unwrap(),Err(Fault::Thrown(error)) if error.message=="recorded consensus factory rejection")
    );
    assert!(frame.fixed_query.is_none());
    assert!(frame.pending_site(Lane::Tally).is_some());
    resume(&mut runtime, &mut frame, &index, Lane::Tally);
    query(&mut runtime, &mut frame, &index, Lane::Tally);
    resume(&mut runtime, &mut frame, &index, Lane::Tally);
    assert!(frame.tally_child_complete());
    assert_eq!(runtime.core.tally.len(), 1);
}
#[test]
fn actual_outer_tally_factory_rejection_is_caught_without_rejecting_main() {
    use NativeMainOuterBootstrapLaneV1 as Lane;
    let temp = Temp::new();
    let (mut runtime, index) = seeded(&temp);
    runtime
        .fail_storage(StorageFailure {
            collection: "tallyMap".into(),
            operation: "getDatabase".into(),
            skip: 0,
            message: "recorded Tally factory rejection".into(),
        })
        .unwrap();
    let mut frame = through_two_factories(&mut runtime, &index);
    resume(&mut runtime, &mut frame, &index, Lane::Tally);
    assert!(matches!(
        frame
            .actual_completion(frame.pending_site(Lane::Tally).unwrap())
            .unwrap(),
        Some(NativeMainOuterActualCompletionV1::Value(V::Undefined))
    ));
    resume(&mut runtime, &mut frame, &index, Lane::Tally);
    assert!(frame.tally_child_complete());
    assert!(runtime.core.tally.is_empty());
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    query(&mut runtime, &mut frame, &index, Lane::Main);
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    query(&mut runtime, &mut frame, &index, Lane::Main);
    assert!(frame.take_prepared().unwrap().unwrap().is_some());
}
#[test]
fn actual_outer_post_query_capacity_keeps_real_result_and_stops_every_ingress() {
    use NativeMainOuterBootstrapLaneV1 as Lane;
    let temp = Temp::new();
    let (mut runtime, index) = seeded(&temp);
    let mut frame = runtime
        .native_main_outer_bootstrap_new(n(10.), 41, &index)
        .unwrap();
    runtime
        .native_main_outer_bootstrap_begin(&mut frame)
        .unwrap();
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    let pending = frame.pending_site(Lane::Main).unwrap();
    runtime
        .native_main_outer_bootstrap_query_sample_actual(&mut frame, pending, &index)
        .unwrap();
    let before = index.snapshot().unwrap();
    runtime.core.effects = vec![V::Null; 65_536];
    assert_eq!(
        runtime.native_main_outer_bootstrap_query_deliver_callback(&mut frame, pending, &index),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(frame.stopped(), Some(&SourceRuleError::Capacity));
    assert!(
        matches!(frame.find_max_query.as_ref().unwrap().actual_returned(),Some(DependencyReturn::Pending(row)) if row.get("value")==n(10.))
    );
    assert!(frame.find_max_query.as_ref().unwrap().callback_delivered());
    assert_eq!(
        runtime.native_main_outer_bootstrap_query_sample_actual(&mut frame, pending, &index),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(
        runtime.native_main_outer_bootstrap_query_deliver_callback(&mut frame, pending, &index),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(
        runtime.native_main_outer_bootstrap_resume_actual(
            &mut frame,
            pending,
            &index,
            &mut |_, _| Ok(42)
        ),
        Err(SourceRuleError::Capacity)
    );
    assert!(frame.take_prepared().is_err());
    assert_eq!(index.snapshot().unwrap(), before);
}

#[test]
fn actual_outer_tally_factory_capacity_retains_its_result_and_never_enters_consensus_factory() {
    use NativeMainOuterBootstrapLaneV1 as Lane;
    let temp = Temp::new();
    let (mut runtime, index) = seeded(&temp);
    let mut frame = runtime
        .native_main_outer_bootstrap_new(n(10.), 41, &index)
        .unwrap();
    runtime
        .native_main_outer_bootstrap_begin(&mut frame)
        .unwrap();
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    query(&mut runtime, &mut frame, &index, Lane::Main);
    resume(&mut runtime, &mut frame, &index, Lane::Main);
    let current = frame.pending_site(Lane::Main).unwrap();
    assert_eq!(
        current.site(),
        NativeMainOuterBootstrapSiteV1::TxIndexFactory
    );
    assert!(frame.tally_factory.is_none());
    assert!(frame.consensus_factory.is_none());
    runtime.core.effects = vec![V::Null; 65_536];
    let before = index.snapshot().unwrap();
    let mut children = vec![];
    assert_eq!(
        runtime.native_main_outer_bootstrap_resume_actual(
            &mut frame,
            current,
            &index,
            &mut |parent, scope| {
                children.push((parent, scope));
                Ok(42)
            },
        ),
        Err(SourceRuleError::Capacity),
    );
    assert_eq!(children, vec![(41, "TallyMap.getInstance/loadFromDB")]);
    assert!(matches!(frame.tally_factory.as_ref(), Some(Ok(_))));
    assert!(
        frame.consensus_factory.is_none(),
        "the next original source call must not run after observation capacity stops this owner"
    );
    assert!(frame.tally_query.is_none());
    assert!(frame.consensus_query.is_none());
    assert_eq!(frame.stopped(), Some(&SourceRuleError::Capacity));
    assert!(runtime.core.source_send_tally_instance_initialized);
    assert!(runtime.core.tally.is_empty());
    assert_eq!(index.snapshot().unwrap(), before);
    assert_eq!(
        runtime.native_main_outer_bootstrap_resume_actual(
            &mut frame,
            current,
            &index,
            &mut |_, _| panic!("no source retry"),
        ),
        Err(SourceRuleError::Capacity),
    );
    let child = frame.pending_site(Lane::Tally).unwrap();
    assert_eq!(
        runtime.native_main_outer_bootstrap_query_sample_actual(&mut frame, child, &index),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(
        runtime.native_main_outer_bootstrap_query_deliver_callback(&mut frame, child, &index),
        Err(SourceRuleError::Capacity)
    );
    assert!(frame.take_prepared().is_err());
}
