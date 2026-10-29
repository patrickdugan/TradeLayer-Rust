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
            "actual-live-service-block-{}-{}",
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
fn setup(temp: &Temp) -> (SourceContractRuntime, SourceCompactIndex) {
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
                    ("height", n(10.)),
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
    runtime.set_height(n(10.)).unwrap();
    let mut index = SourceCompactIndex::create(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual row raw source commands",
    )
    .unwrap();
    index.import_row(&index.snapshot().unwrap(), row()).unwrap();
    runtime
        .native_main_bind(obj(&[("profile", s("actual-row-test"))]))
        .unwrap();
    NATIVE_MAIN_ACTUAL_ROW_DECODE_COUNT.with(|count| count.set(0));
    (runtime, index)
}
fn service_setup(temp: &Temp) -> (SourceContractRuntime, SourceCompactIndex) {
    let (mut runtime, index) = setup(temp);
    invoke(
        &mut runtime,
        "SourceMain.configureService",
        vec![obj(&[
            ("fileInputs", V::Array(vec![])),
            ("directoryInputs", V::Array(vec![])),
            (
                "fileIOInputs",
                obj(&[("failures", V::Array(vec![])), ("mtimes", V::Array(vec![]))]),
            ),
        ])],
    );
    (runtime, index)
}
fn request(height: V) -> crate::source_main_step_kernel::MainServiceBlockRequest {
    crate::source_main_step_kernel::MainServiceBlockRequest {
        job: 17,
        block: obj(&[
            ("hash", s("same-live-empty-10")),
            ("previousblockhash", s("same-live-empty-9")),
            ("tx", V::Array(vec![])),
        ]),
        height,
    }
}
fn original_block(
    runtime: &mut SourceContractRuntime,
    index: &mut SourceCompactIndex,
    request: &crate::source_main_step_kernel::MainServiceBlockRequest,
) -> Result<DependencyReturn, SourceRuleError> {
    let call = DependencyCall {
        target: "Main.processBlock".into(),
        arguments: vec![request.block.clone(), request.height.clone()],
    };
    let mut validity = vec![];
    let mut consumed = 0;
    let mut storage_stop = None;
    runtime.source_main_invoke_with(&call, false, &mut |runtime, _| {
        runtime.source_main_step_block(
            index,
            request.block.clone(),
            request.height.clone(),
            &mut validity,
            &mut consumed,
            &mut storage_stop,
        )
    })
}
fn frame_returned(
    frame: &NativeMainServiceBlockFrame,
) -> Result<DependencyReturn, SourceRuleError> {
    match frame.result.as_ref().expect("actual finished block") {
        Ok(value) => Ok(DependencyReturn::Pending(value.clone())),
        Err(Fault::Thrown(error)) => Ok(DependencyReturn::PendingThrow(error.clone())),
        Err(Fault::Local(error)) => Err(error.clone()),
    }
}
#[test]
fn actual_empty_live_block_matches_unchanged_native_begin_mid_end() {
    let temp = Temp::new();
    let reference_temp = Temp::new();
    let (mut runtime, mut index) = service_setup(&temp);
    let (mut reference, mut reference_index) = service_setup(&reference_temp);
    let request = request(n(11.));
    let expected = original_block(&mut reference, &mut reference_index, &request);
    assert_eq!(
        expected,
        Ok(DependencyReturn::Pending(obj(&[
            ("reOrg", V::Bool(false)),
            ("restore", n(11.))
        ])))
    );
    let mut frame = runtime.native_main_service_block_begin(request).unwrap();
    let effects_before = runtime.core.effects.clone();
    let mut fields = vec![];
    frame
        .visit_canonical_fields(&mut |path, value| {
            assert!(!matches!(value, V::Object(_) | V::Array(_)));
            fields.push((path.to_owned(), value.clone()));
            Ok(())
        })
        .unwrap();
    assert!(fields.contains(&("invocation/present".into(), V::Bool(true))));
    assert!(fields.contains(&("request/job".into(), s("17"))));
    assert!(fields.contains(&("result/present".into(), V::Bool(false))));
    assert_eq!(
        runtime.core.effects, effects_before,
        "scalar observation cannot drain or append effects"
    );
    assert_eq!(
        runtime.native_main_service_block_advance(&mut index, &mut frame, &V::Array(vec![])),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(frame_returned(&frame), expected);
    assert_eq!(runtime.core.effects, reference.core.effects);
    assert_eq!(
        runtime.native_main_state_value().unwrap(),
        reference.native_main_state_value().unwrap()
    );
    assert!(frame.invocation.is_none());
    assert!(frame.row.is_none());
    assert!(frame.validity.is_empty());
}

#[test]
fn actual_withdrawal_rejection_matches_unchanged_live_block_and_excludes_end() {
    let temp = Temp::new();
    let reference_temp = Temp::new();
    let (mut runtime, mut index) = service_setup(&temp);
    let (mut reference, mut reference_index) = service_setup(&reference_temp);
    for owner in [&mut runtime, &mut reference] {
        owner
            .fail_storage(StorageFailure {
                collection: "withdrawQueue".into(),
                operation: "findAsync".into(),
                skip: 0,
                message: "actual-live-withdrawal-read-rejection".into(),
            })
            .unwrap();
    }
    let request = request(n(11.));
    let expected = original_block(&mut reference, &mut reference_index, &request);
    assert!(
        matches!(&expected, Ok(DependencyReturn::PendingThrow(error)) if error.message == "actual-live-withdrawal-read-rejection")
    );
    let mut frame = runtime.native_main_service_block_begin(request).unwrap();
    assert_eq!(
        runtime.native_main_service_block_advance(&mut index, &mut frame, &V::Array(vec![])),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(frame_returned(&frame), expected);
    assert_eq!(runtime.core.effects, reference.core.effects);
    assert_eq!(
        runtime.native_main_state_value().unwrap(),
        reference.native_main_state_value().unwrap()
    );
    assert!(runtime
        .core
        .effects
        .iter()
        .any(|effect| effect.get("kind") == s("main-phase")
            && effect.get("target") == s("Channels.processWithdrawals")));
    assert!(
        !runtime
            .core
            .effects
            .iter()
            .any(|effect| effect.get("collection") == s("volumeIndex")),
        "rejected withdrawal must not enter blockHandlerEnd volume reads"
    );
    let mut fields = vec![];
    frame
        .visit_canonical_fields(&mut |path, value| {
            fields.push((path.to_owned(), value.clone()));
            Ok(())
        })
        .unwrap();
    assert!(fields.contains(&("result/kind".into(), s("Thrown"))));
    assert!(fields.contains(&(
        "result/message".into(),
        s("actual-live-withdrawal-read-rejection")
    )));
}

#[test]
fn actual_mid_height_guard_preserves_same_begin_prefix_and_borrowed_invocation() {
    let temp = Temp::new();
    let reference_temp = Temp::new();
    let (mut runtime, mut index) = service_setup(&temp);
    let (mut reference, mut reference_index) = service_setup(&reference_temp);
    let request = request(n(-1.));
    let expected = original_block(&mut reference, &mut reference_index, &request);
    assert_eq!(expected, Err(SourceRuleError::InvalidInput));
    let mut frame = runtime.native_main_service_block_begin(request).unwrap();
    assert_eq!(
        runtime.native_main_service_block_advance(&mut index, &mut frame, &V::Array(vec![])),
        NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput)
    );
    assert!(frame.invocation.is_some());
    assert!(frame.result.is_none());
    assert_eq!(runtime.core.effects, reference.core.effects);
    assert_eq!(
        runtime.native_main_state_value().unwrap(),
        reference.native_main_state_value().unwrap()
    );
    assert!(!runtime
        .core
        .effects
        .iter()
        .any(|effect| effect.get("kind") == s("main-phase")));
}
