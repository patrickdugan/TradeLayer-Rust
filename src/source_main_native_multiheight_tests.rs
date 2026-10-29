// Raw inputs and expected boundaries come from unchanged original Main/NeDB.
fn multiheight_row(height: u64, id: &str, payload: &str) -> V {
    let mut value = row().get("value");
    let V::Object(fields) = &mut value else {
        unreachable!()
    };
    for (name, field) in fields {
        if name == "txId" {
            *field = s(id);
        }
        if name == "payload" {
            *field = s(payload);
        }
    }
    obj(&[("_id", s(format!("tx-{height}-{id}"))), ("value", value)])
}
fn multiheight_setup(
    temp: &Temp,
    all: bool,
    extra_trade: bool,
) -> (SourceContractRuntime, SourceCompactIndex) {
    let previous = multiheight_row(9, "multiheight-previous9", "00,production-code");
    let funding = multiheight_row(
        10,
        "multiheight-funding10",
        "48,200000000,LKKG9A9a8n7CeHK8Nk7RdNZiCuHZW2U789,1,0,,",
    );
    let trade = multiheight_row(11, "multiheight-fresh11", "00,production-code");
    let mut rows = vec![previous.clone(), funding.clone(), trade.clone()];
    if extra_trade {
        rows.push(multiheight_row(
            10,
            "aaa-before-funding-in-native-id-order",
            "00,production-code",
        ));
    }
    rows.push(obj(&[("_id", s("MaxHeight")), ("value", n(11.))]));
    let (mut runtime, mut index) = setup_source_rows(temp, rows, 11.);
    for (height, raw) in if all {
        vec![(9., previous), (10., funding), (11., trade)]
    } else {
        vec![(9., previous)]
    } {
        let outcome = runtime.native_main_rows(&mut index, V::Array(vec![raw]), n(height), false);
        assert_eq!(outcome.result, Ok(DependencyReturn::Pending(n(0.))));
    }
    runtime.take_effects();
    NATIVE_MAIN_ACTUAL_ROW_DECODE_COUNT.with(|count| count.set(0));
    terminal_rpc(&mut runtime);
    (runtime, index)
}
fn multiheight_finish(
    runtime: &mut SourceContractRuntime,
    index: &mut SourceCompactIndex,
) -> NativeMainCatchupContinuationV1 {
    let mut frame = runtime.native_main_catchup_begin(n(8.)).unwrap();
    let mut stop = runtime.native_main_catchup_advance(index, &mut frame);
    for _ in 0..8 {
        match stop {
            NativeMainRowStopV1::Complete => return frame,
            NativeMainRowStopV1::AwaitNativeWrite => {
                stop = finish_write(runtime, index, &mut frame)
            }
            _ => panic!("unexpected actual catchup boundary: {stop:?}"),
        }
    }
    panic!("bounded raw corpus must reach its actual source return");
}
fn multiheight_marks(runtime: &SourceContractRuntime) -> Vec<(V, V)> {
    runtime
        .core
        .effects
        .iter()
        .filter_map(|effect| {
            if effect.get("collection") != s("consensus")
                || effect.get("method") != s("updateAsync")
            {
                return None;
            }
            let args = array(&effect.get("args")).unwrap();
            let set = arg(&args, 1).get("$set");
            (set.get("processed") == V::Bool(true))
                .then(|| (arg(&args, 0).get("_id"), set.get("params")))
        })
        .collect()
}
fn multiheight_assert_return(frame: &NativeMainCatchupContinuationV1, name: &str, message: &str) {
    assert_eq!(
        frame.result(),
        Some(&Ok(DependencyReturn::PendingThrow(
            crate::validity_source::SourceException {
                name: name.into(),
                message: message.into()
            }
        )))
    );
}
fn multiheight_assert_no_tail(runtime: &SourceContractRuntime) {
    assert!(!runtime
        .collection("consensus")
        .iter()
        .any(|r| r.get("_id") == s("MaxProcessedHeight")));
    assert!(!runtime
        .core
        .effects
        .iter()
        .any(|e| e.get("method") == s("Client.getBlockCount")));
}
#[test]
fn original_multiheight_skips_previous_then_settles_funding_before_later_trade() {
    let temp = Temp::new();
    let (mut runtime, mut index) = multiheight_setup(&temp, false, false);
    let frame = multiheight_finish(&mut runtime, &mut index);
    multiheight_assert_return(
        &frame,
        "SourceBoundaryUnavailable",
        "Native Main control has no live RPC service",
    );
    assert_eq!(decodes(&runtime), 2);
    assert_eq!(
        multiheight_marks(&runtime)
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>(),
        vec![s("multiheight-funding10"), s("multiheight-fresh11")]
    );
    assert!(phases(&runtime, 8.).is_empty());
    assert!(phases(&runtime, 9.).is_empty());
    assert_eq!(phases(&runtime, 10.), expected_phases());
    assert_eq!(phases(&runtime, 11.), expected_phases());
    assert_eq!(
        index
            .get("tx-10-multiheight-funding10")
            .unwrap()
            .unwrap()
            .get("valid"),
        V::Bool(false)
    );
    assert_eq!(
        runtime.core.main_production.as_ref().unwrap().parse_block,
        n(11.)
    );
    assert!(runtime
        .collection("consensus")
        .iter()
        .any(|r| r.get("_id") == s("MaxProcessedHeight") && r.get("value") == n(11.)));
}
#[test]
fn original_multiheight_double_funding_validity_failure_escapes_before_both_trade_phases() {
    let temp = Temp::new();
    let (mut runtime, mut index) = multiheight_setup(&temp, false, true);
    let retained_tally = runtime.collection("tallyMap").to_vec();
    for message in [
        "recorded-funding-primary-validity-rejection",
        "recorded-funding-fatal-validity-rejection",
    ] {
        runtime
            .fail_storage(StorageFailure {
                collection: "txIndex".into(),
                operation: "updateAsync".into(),
                skip: 0,
                message: message.into(),
            })
            .unwrap();
    }
    let frame = multiheight_finish(&mut runtime, &mut index);
    multiheight_assert_return(&frame, "Error", "recorded-funding-fatal-validity-rejection");
    assert_eq!(decodes(&runtime), 1);
    let marks = multiheight_marks(&runtime);
    assert_eq!(marks.len(), 2);
    assert!(marks
        .iter()
        .all(|(id, _)| *id == s("multiheight-funding10")));
    assert_eq!(
        marks[1].1.get("reason"),
        s("Fatal tx processing error: recorded-funding-primary-validity-rejection")
    );
    assert!(phases(&runtime, 10.).is_empty());
    assert!(phases(&runtime, 11.).is_empty());
    assert!(!runtime.collection("consensus").iter().any(|r| [
        s("multiheight-fresh11"),
        s("aaa-before-funding-in-native-id-order")
    ]
    .contains(&r.get("_id"))));
    assert_eq!(runtime.collection("tallyMap"), retained_tally);
    multiheight_assert_no_tail(&runtime);
}
#[test]
fn original_multiheight_clearing_factory_rejection_retains_earlier_settlement_and_stops_later_height(
) {
    let temp = Temp::new();
    let (mut runtime, mut index) = multiheight_setup(&temp, false, false);
    runtime
        .fail_storage(StorageFailure {
            collection: "oracleData".into(),
            operation: "getDatabase".into(),
            skip: 0,
            message: "recorded-clearing-oracleData-factory-rejection".into(),
        })
        .unwrap();
    let frame = multiheight_finish(&mut runtime, &mut index);
    multiheight_assert_return(
        &frame,
        "Error",
        "recorded-clearing-oracleData-factory-rejection",
    );
    assert_eq!(decodes(&runtime), 1);
    assert_eq!(multiheight_marks(&runtime).len(), 1);
    assert_eq!(phases(&runtime, 10.), expected_phases());
    assert!(phases(&runtime, 11.).is_empty());
    assert_eq!(
        runtime.core.main_production.as_ref().unwrap().parse_block,
        n(10.)
    );
    assert_eq!(
        index
            .get("tx-10-multiheight-funding10")
            .unwrap()
            .unwrap()
            .get("valid"),
        V::Bool(false)
    );
    assert!(!runtime
        .collection("consensus")
        .iter()
        .any(|r| r.get("_id") == s("multiheight-fresh11")));
    multiheight_assert_no_tail(&runtime);
}
#[test]
fn original_multiheight_processed_invalid_funding_skips_all_settlement_and_saves_last_height() {
    let temp = Temp::new();
    let (mut runtime, mut index) = multiheight_setup(&temp, true, false);
    let frame = multiheight_finish(&mut runtime, &mut index);
    multiheight_assert_return(
        &frame,
        "SourceBoundaryUnavailable",
        "Native Main control has no live RPC service",
    );
    assert_eq!(decodes(&runtime), 0);
    assert!(multiheight_marks(&runtime).is_empty());
    for height in [8., 9., 10., 11.] {
        assert!(phases(&runtime, height).is_empty());
    }
    assert!(runtime
        .collection("consensus")
        .iter()
        .any(|r| r.get("_id") == s("MaxProcessedHeight") && r.get("value") == n(11.)));
    assert_eq!(
        index
            .get("tx-10-multiheight-funding10")
            .unwrap()
            .unwrap()
            .get("valid"),
        V::Bool(false)
    );
}
