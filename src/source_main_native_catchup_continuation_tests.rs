use super::*;
use crate::storage::Namespace;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-catchup-continuation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        if let Ok(root) = self.0.canonicalize() {
            assert_eq!(root.parent(), Some(parent.as_path()));
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
fn invoke(runtime: &mut SourceContractRuntime, target: &str, arguments: Vec<V>) {
    let returned = runtime
        .invoke_dependency(&DependencyCall {
            target: target.into(),
            arguments,
        })
        .unwrap();
    assert!(
        matches!(
            returned,
            DependencyReturn::Immediate(_) | DependencyReturn::Pending(_)
        ),
        "{target}: {returned:?}"
    );
    runtime.take_effects();
}
fn row() -> V {
    // cb61 genuine NeDB await oracle Main0 raw transaction, not params/state.
    obj(&[
        (
            "_id",
            s("tx-10-ac51132ae7087eca33d746db2622442dd53e300de4f38aa9b4bc418555d4a2c8"),
        ),
        (
            "value",
            obj(&[
                (
                    "txId",
                    s("ac51132ae7087eca33d746db2622442dd53e300de4f38aa9b4bc418555d4a2c8"),
                ),
                ("marker", s("tl")),
                ("payload", s("00,production-code")),
                (
                    "sender",
                    obj(&[
                        ("senderAddress", s("MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB")),
                        ("amount", n(0.)),
                    ]),
                ),
                ("reference", V::Array(vec![])),
            ]),
        ),
    ])
}
fn setup_source_rows(
    temp: &Temp,
    rows: Vec<V>,
    source_height: f64,
) -> (SourceContractRuntime, SourceCompactIndex) {
    let mut runtime =
        SourceContractRuntime::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12);
    invoke(
        &mut runtime,
        "SourceActivation.configureClient",
        vec![obj(&[("chain", s("LTC")), ("test", V::Bool(false))])],
    );
    invoke(&mut runtime, "Activation.init", vec![s("LTC")]);
    invoke(
        &mut runtime,
        "SourceMain.configureProduction",
        vec![obj(&[
            (
                "client",
                obj(&[("chain", s("LTC")), ("test", V::Bool(false))]),
            ),
            (
                "io",
                obj(&[
                    ("height", n(source_height)),
                    (
                        "networkInfo",
                        obj(&[("networkactive", V::Bool(true)), ("connections", n(2.))]),
                    ),
                    ("blocks", V::Array(vec![])),
                ]),
            ),
            ("genesisBlock", n(10.)),
        ])],
    );
    runtime.set_height(n(source_height)).unwrap();
    let mut index = SourceCompactIndex::create(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual row raw source commands",
    )
    .unwrap();
    for raw in rows {
        index.import_row(&index.snapshot().unwrap(), raw).unwrap();
    }
    runtime
        .native_main_bind(obj(&[("profile", s("actual-row-test"))]))
        .unwrap();
    NATIVE_MAIN_ACTUAL_ROW_DECODE_COUNT.with(|count| count.set(0));
    (runtime, index)
}
fn setup(temp: &Temp) -> (SourceContractRuntime, SourceCompactIndex) {
    setup_source_rows(temp, vec![row()], 10.)
}
fn marks(runtime: &SourceContractRuntime) -> usize {
    runtime
        .core
        .effects
        .iter()
        .filter(|effect| {
            effect.get("collection") == s("consensus")
                && effect.get("method") == s("updateAsync")
                && arg(&array(&effect.get("args")).unwrap(), 0).get("_id")
                    == row().get("value").get("txId")
        })
        .count()
}
fn decodes(_runtime: &SourceContractRuntime) -> usize {
    NATIVE_MAIN_ACTUAL_ROW_DECODE_COUNT.with(|count| count.get())
}
fn terminal_rpc(runtime: &mut SourceContractRuntime) {
    runtime
        .source_main_step_update_io(obj(&[(
            "failures",
            V::Array(vec![obj(&[
                ("method", s("Client.getBlockCount")),
                ("ordinal", n(1.)),
                ("name", s("SourceBoundaryUnavailable")),
                ("message", s("Native Main control has no live RPC service")),
            ])]),
        )]))
        .unwrap();
}
fn phases(runtime: &SourceContractRuntime, height: f64) -> Vec<V> {
    runtime
        .core
        .effects
        .iter()
        .filter(|effect| {
            effect.get("kind") == s("main-phase")
                && arg(&array(&effect.get("args")).unwrap(), 0) == n(height)
        })
        .map(|effect| effect.get("target"))
        .collect()
}
fn expected_phases() -> Vec<V> {
    [
        "Tally.flushQueuedFeeAccruals",
        "Logic.settleLiquidityRewards",
        "Orderbook.processQueuedOnChainOrdersForBlock",
        "Orderbook.processQueuedChannelTradesForBlock",
        "Channels.processWithdrawals",
        "Clearing.clearingFunction",
    ]
    .map(s)
    .to_vec()
}
fn same(runtime: &SourceContractRuntime, legacy: &SourceContractRuntime) {
    assert_eq!(runtime.core.effects, legacy.core.effects);
    assert_eq!(
        runtime.native_main_state_value().unwrap(),
        legacy.native_main_state_value().unwrap()
    );
}
fn finish_write(
    runtime: &mut SourceContractRuntime,
    index: &mut SourceCompactIndex,
    frame: &mut NativeMainCatchupContinuationV1,
) -> NativeMainRowStopV1 {
    assert_eq!(
        runtime.native_main_catchup_append(index, frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    runtime.native_main_catchup_resolve_post(index, frame)
}
#[test]
fn actual_catchup_resumes_stored_row_then_runs_six_settlement_calls_once() {
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    terminal_rpc(&mut runtime);
    terminal_rpc(&mut legacy);
    let mut frame = runtime.native_main_catchup_begin(n(10.)).unwrap();
    assert_eq!(
        runtime.native_main_catchup_advance(&mut index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(marks(&runtime), 1);
    assert_eq!(decodes(&runtime), 1);
    assert!(runtime.core.queue_context.is_some());
    assert!(phases(&runtime, 10.).is_empty());
    let prefix = runtime.core.effects.clone();
    assert_eq!(
        runtime.native_main_catchup_advance(&mut index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(runtime.core.effects, prefix);
    let mut fields = vec![];
    frame
        .visit_canonical_fields(&mut |path, value| {
            assert!(!matches!(value, V::Object(_) | V::Array(_)));
            fields.push((path.to_owned(), value.clone()));
            Ok(())
        })
        .unwrap();
    assert!(fields.contains(&("pc".into(), s("Trade"))));
    assert_eq!(
        finish_write(&mut runtime, &mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(marks(&runtime), 1);
    assert_eq!(decodes(&runtime), 1);
    assert_eq!(phases(&runtime, 10.), expected_phases());
    assert_eq!(frame.consumed_rows(), 1);
    assert_eq!(frame.validity().len(), 1);
    assert!(frame.grouping().unwrap().complete);
    let expected = legacy.native_main_construct(&mut legacy_index, n(10.));
    assert_eq!(frame.result(), Some(&expected.result));
    same(&runtime, &legacy);
}
#[test]
fn actual_catchup_exact_post_does_not_decode_or_append_again() {
    let temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    terminal_rpc(&mut runtime);
    let mut frame = runtime.native_main_catchup_begin(n(10.)).unwrap();
    assert_eq!(
        runtime.native_main_catchup_advance(&mut index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    index
        .commit_prepared_validity(frame.pending_write().unwrap())
        .unwrap();
    let post = index.snapshot().unwrap();
    assert_eq!(
        runtime.native_main_catchup_resolve_post(&mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(index.snapshot().unwrap(), post);
    assert_eq!(marks(&runtime), 1);
    assert_eq!(decodes(&runtime), 1);
    assert_eq!(phases(&runtime, 10.), expected_phases());
    assert_eq!(
        runtime.native_main_catchup_advance(&mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(phases(&runtime, 10.), expected_phases());
}
#[test]
fn actual_catchup_keeps_funding_then_trade_cursors_and_partial_progress() {
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    terminal_rpc(&mut runtime);
    terminal_rpc(&mut legacy);
    // Original lower04 raw syntax; inactive type4 is source-invalid yet marked
    // and indexed. Its real funding await must precede the Main0 trade prefix.
    let mut funding = row();
    set(&mut funding, "_id", s("tx-10-funding-raw-input")).unwrap();
    let mut value = funding.get("value");
    set(&mut value, "txId", s("funding-raw-input")).unwrap();
    set(
        &mut value,
        "payload",
        s("48,200000000,LKKG9A9a8n7CeHK8Nk7RdNZiCuHZW2U789,1,0,,"),
    )
    .unwrap();
    set(&mut funding, "value", value).unwrap();
    for metadata in [&mut index, &mut legacy_index] {
        metadata
            .import_row(&metadata.snapshot().unwrap(), funding.clone())
            .unwrap();
    }
    let mut frame = runtime.native_main_catchup_begin(n(10.)).unwrap();
    assert_eq!(
        runtime.native_main_catchup_advance(&mut index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        frame.pending_write().unwrap().update_arguments()[0].get("_id"),
        funding.get("_id")
    );
    assert_eq!(frame.consumed_rows(), 1);
    assert_eq!(decodes(&runtime), 1);
    assert!(runtime.collection("propertyList").is_empty());
    assert_eq!(
        finish_write(&mut runtime, &mut index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        frame.pending_write().unwrap().update_arguments()[0].get("_id"),
        row().get("_id")
    );
    assert_eq!(frame.consumed_rows(), 2);
    assert_eq!(decodes(&runtime), 2);
    assert!(!runtime.collection("propertyList").is_empty());
    assert!(runtime.core.queue_context.is_some());
    assert!(phases(&runtime, 10.).is_empty());
    assert_eq!(
        finish_write(&mut runtime, &mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(phases(&runtime, 10.), expected_phases());
    assert_eq!(frame.validity().len(), 2);
    let expected = legacy.native_main_construct(&mut legacy_index, n(10.));
    assert_eq!(frame.result(), Some(&expected.result));
    same(&runtime, &legacy);
}
#[test]
fn actual_catchup_height_zero_forces_exact_six_even_when_all_rows_are_processed() {
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    for (owner, metadata) in [(&mut runtime, &mut index), (&mut legacy, &mut legacy_index)] {
        assert_eq!(
            owner
                .native_main_rows(metadata, V::Array(vec![row()]), n(10.), false)
                .result,
            Ok(DependencyReturn::Pending(n(0.)))
        );
        owner.take_effects();
        terminal_rpc(owner);
    }
    NATIVE_MAIN_ACTUAL_ROW_DECODE_COUNT.with(|count| count.set(0));
    let mut frame = runtime.native_main_catchup_begin(n(0.)).unwrap();
    assert_eq!(
        runtime.native_main_catchup_advance(&mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(decodes(&runtime), 0);
    assert_eq!(marks(&runtime), 0);
    assert_eq!(phases(&runtime, 0.), expected_phases());
    assert!(phases(&runtime, 10.).is_empty());
    assert_eq!(frame.consumed_rows(), 1);
    assert!(frame.validity().is_empty());
    let expected = legacy.native_main_construct(&mut legacy_index, n(0.));
    assert_eq!(frame.result(), Some(&expected.result));
    same(&runtime, &legacy);
}

#[test]
fn actual_whole_query_find_rejection_precedes_grouping_and_financial_rows() {
    // Genuine source gate: Main awaits txIndexDB.findAsync({}) after its
    // consensus read, before grouping or any row/financial processing.
    let temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    runtime
        .fail_storage(StorageFailure {
            collection: "txIndex".into(),
            operation: "findAsync".into(),
            skip: 0,
            message: "recorded-whole-index-query-rejection".into(),
        })
        .unwrap();
    let before = index.snapshot().unwrap();
    let mut frame = runtime.native_main_catchup_begin(n(10.)).unwrap();
    assert_eq!(
        runtime.native_main_catchup_advance(&mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(
        frame.result(),
        Some(&Ok(DependencyReturn::PendingThrow(
            crate::validity_source::SourceException {
                name: "Error".into(),
                message: "recorded-whole-index-query-rejection".into()
            }
        )))
    );
    assert!(frame.grouping().is_none());
    assert!(frame.pending_write().is_none());
    assert_eq!(frame.consumed_rows(), 0);
    assert_eq!(decodes(&runtime), 0);
    assert_eq!(marks(&runtime), 0);
    assert!(frame.validity().is_empty());
    assert!(phases(&runtime, 10.).is_empty());
    for collection in ["propertyList", "tallyMap", "consensus"] {
        assert!(
            runtime.collection(collection).is_empty(),
            "no financial rows in {collection}"
        );
    }
    assert_eq!(index.snapshot().unwrap(), before);
    assert!(
        runtime.core.queue_context.is_none(),
        "source rejection reaches actual enclosing finish"
    );
    assert!(runtime
        .core
        .effects
        .iter()
        .any(|effect| effect.get("collection") == s("consensus")
            && effect.get("method") == s("findOneAsync")
            && arg(&array(&effect.get("args")).unwrap(), 0).get("_id") == s("MaxProcessedHeight")));
    assert!(!runtime
        .core
        .effects
        .iter()
        .any(|effect| effect.get("method") == s("Client.getBlockCount")));
}

#[test]
fn actual_nonzero_volume_reads_then_vests_both_properties_before_withdrawal() {
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    for owner in [&mut runtime, &mut legacy] {
        // Actual source commands create both current block volume and a final
        // cumulative row whose block is below10. No financial table is imported.
        invoke(
            owner,
            "VolumeIndex.saveVolumeDataById",
            vec![
                s("0-99"),
                n(20_000_000_000.),
                n(20_000_000_000.),
                n(1.),
                n(10.),
                s("UTXO"),
            ],
        );
        invoke(
            owner,
            "VolumeIndex.saveVolumeDataById",
            vec![s("0-98"), n(1.), n(1.), n(1.), n(9.), s("UTXO")],
        );
        terminal_rpc(owner);
    }
    let mut frame = runtime.native_main_catchup_begin(n(10.)).unwrap();
    assert_eq!(
        runtime.native_main_catchup_advance(&mut index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let tally_before = runtime.collection("tallyMap").to_vec();
    NATIVE_MAIN_ACTUAL_VESTING_OBSERVATIONS.with(|observed| observed.borrow_mut().clear());
    assert_eq!(
        finish_write(&mut runtime, &mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(
        number(&frame.cumulative.get("ltcPairTotalVolume")).unwrap(),
        20_000_000_001.
    );
    assert_eq!(
        number(&frame.cumulative.get("globalCumulativeVolume")).unwrap(),
        20_000_000_001.
    );
    assert_eq!(
        number(&frame.volume.get("ltcPairs")).unwrap(),
        20_000_000_000.
    );
    assert_eq!(
        number(&frame.volume.get("global")).unwrap(),
        20_000_000_000.
    );
    NATIVE_MAIN_ACTUAL_VESTING_OBSERVATIONS.with(|observed| assert_eq!(*observed.borrow(), vec![(2., 250_000., n(10.)), (3., 45_454.545_456_060_61, n(10.))], "actual computed locals match independently executed original-JS applyVesting arguments"));
    let block_read = runtime
        .core
        .effects
        .iter()
        .position(|effect| {
            effect.get("collection") == s("volumeIndex")
                && effect.get("method") == s("findAsync")
                && arg(&array(&effect.get("args")).unwrap(), 0).get("value.blockHeight") == n(10.)
        })
        .unwrap();
    let withdrawals = runtime
        .core
        .effects
        .iter()
        .position(|effect| {
            effect.get("kind") == s("main-phase")
                && effect.get("target") == s("Channels.processWithdrawals")
        })
        .unwrap();
    assert!(block_read < withdrawals);
    let vesting_effects = &runtime.core.effects[block_read + 1..withdrawals];
    let properties = vesting_effects
        .iter()
        .filter(|effect| {
            effect.get("kind") == s("dependency")
                && effect.get("target") == s("Property.getPropertyData")
        })
        .map(|effect| arg(&array(&effect.get("args")).unwrap(), 0))
        .collect::<Vec<_>>();
    assert_eq!(
        properties,
        vec![n(2.), n(3.), n(2.), n(3.)],
        "source updateVesting reads2/3, then applyVesting2 before3"
    );
    assert_eq!(
        vesting_effects
            .iter()
            .filter(
                |effect| effect.get("kind") == s("database") && effect.get("name") == s("tallyMap")
            )
            .count(),
        2
    );
    assert!(
        !vesting_effects
            .iter()
            .any(|effect| effect.get("collection") == s("tallyMap")
                && effect.get("method") == s("updateAsync")),
        "original unawaited factory TypeError is caught; no invented vesting balance update"
    );
    assert_eq!(runtime.collection("tallyMap"), tally_before);
    assert_eq!(phases(&runtime, 10.), expected_phases());
    let expected = legacy.native_main_construct(&mut legacy_index, n(10.));
    assert_eq!(frame.result(), Some(&expected.result));
    same(&runtime, &legacy);
}

#[test]
fn actual_withdrawal_phase_rejection_excludes_clearing_height_save_and_tail() {
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    terminal_rpc(&mut runtime);
    terminal_rpc(&mut legacy);
    let mut frame = runtime.native_main_catchup_begin(n(10.)).unwrap();
    assert_eq!(
        runtime.native_main_catchup_advance(&mut index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let prefix_withdrawal_reads = runtime
        .core
        .effects
        .iter()
        .filter(|effect| {
            effect.get("collection") == s("withdrawQueue") && effect.get("method") == s("findAsync")
        })
        .count();
    assert_eq!(
        prefix_withdrawal_reads, 1,
        "original source activation state hash performs the first read"
    );
    // Record a future dependency failure at the genuine metadata await. The
    // unchanged legacy run receives the same fact with its observed prefix skip.
    runtime
        .fail_storage(StorageFailure {
            collection: "withdrawQueue".into(),
            operation: "findAsync".into(),
            skip: 0,
            message: "recorded-withdrawal-read-rejection".into(),
        })
        .unwrap();
    legacy
        .fail_storage(StorageFailure {
            collection: "withdrawQueue".into(),
            operation: "findAsync".into(),
            skip: prefix_withdrawal_reads,
            message: "recorded-withdrawal-read-rejection".into(),
        })
        .unwrap();
    assert_eq!(
        finish_write(&mut runtime, &mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(
        frame.result(),
        Some(&Ok(DependencyReturn::PendingThrow(
            crate::validity_source::SourceException {
                name: "Error".into(),
                message: "recorded-withdrawal-read-rejection".into()
            }
        )))
    );
    assert_eq!(phases(&runtime, 10.), expected_phases()[..5]);
    assert!(!runtime
        .collection("consensus")
        .iter()
        .any(|row| row.get("_id") == s("MaxProcessedHeight")));
    assert!(!runtime
        .core
        .effects
        .iter()
        .any(|effect| effect.get("method") == s("Client.getBlockCount")));
    assert!(runtime.core.queue_context.is_none());
    let expected = legacy.native_main_construct(&mut legacy_index, n(10.));
    assert_eq!(frame.result(), Some(&expected.result));
    same(&runtime, &legacy);
}

include!("source_main_native_multiheight_tests.rs");
