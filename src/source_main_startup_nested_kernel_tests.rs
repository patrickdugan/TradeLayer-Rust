use super::*;
struct RawIncomingIo;
impl sealed::Sealed for RawIncomingIo {}
impl MainStepOwner for RawIncomingIo {
    fn preflight(&self, op: &MainStepPrimitive) -> Result<(), SourceRuleError> {
        if matches!(op, MainStepPrimitive::ProcessBlock { .. }) {
            panic!("financial primitive must remain parked")
        };
        Ok(())
    }
    fn execute(&mut self, op: &MainStepPrimitive) -> MainStepPrimitiveExecution {
        let value = match op {
            MainStepPrimitive::BlockCount => n(2.),
            MainStepPrimitive::HeightRead(_) => n(1.),
            MainStepPrimitive::HeightWrite { .. } => V::Undefined,
            MainStepPrimitive::PauseRead => V::Bool(false),
            MainStepPrimitive::NetworkStatus => obj(&[("status", V::Bool(true))]),
            MainStepPrimitive::Number(v) => v.clone(),
            MainStepPrimitive::Add(a, b) => n(raw_number(a) + raw_number(b)),
            MainStepPrimitive::Greater(a, b) => V::Bool(raw_number(a) > raw_number(b)),
            MainStepPrimitive::LessEqual(a, b) => V::Bool(raw_number(a) <= raw_number(b)),
            MainStepPrimitive::FetchBlock(height) => {
                assert_eq!(height, &n(2.));
                obj(&[
                    ("hash", s("raw-block2")),
                    ("previousblockhash", s("raw-block1")),
                    ("tx", V::Array(vec![])),
                ])
            }
            other => panic!("unrequested source primitive {other:?}"),
        };
        MainStepPrimitiveExecution {
            outcome: MainStepPrimitiveOutcome::Returned(value),
            effects: vec![],
        }
    }
}
fn raw_held_incoming() -> (MainStepKernelV1, MainServiceBlockRequest) {
    let mut kernel = MainStepKernelV1::new(9);
    kernel
        .service_enqueue(
            &mut RawIncomingIo,
            MainStepCommandV1::Start {
                method: "Main.processIncomingBlocks".into(),
                arguments: vec![V::Undefined, n(1.), n(2.)],
            },
        )
        .unwrap();
    for _ in 0..32 {
        if let Some(request) = kernel.service_next_block() {
            return (kernel, request);
        }
        kernel.service_drain_one(&mut RawIncomingIo).unwrap();
    }
    panic!("actual source Kernel Incoming7")
}
#[test]
fn actual_incoming_park_token_retains_original_job_and_rejects_stale_restore_without_mutation() {
    let (mut kernel, request) = raw_held_incoming();
    let token = kernel.startup_park_block(&request).unwrap();
    assert_eq!(kernel.service_next_block(), None);
    assert!(!kernel.service_has_ready());
    assert!(kernel
        .jobs()
        .any(|(id, status)| id == request.job && status == &MainStepJobStatus::Ready));
    let before = kernel.value();
    assert!(matches!(
        kernel.startup_park_block(&request),
        Err(MainStepError::InvalidInput)
    ));
    assert_eq!(kernel.value(), before);
    let wrong = MainStartupParkedBlockV1 {
        generation: 10,
        request: request.clone(),
    };
    assert_eq!(
        kernel.startup_unpark_block(&wrong),
        Err(MainStepError::InvalidInput)
    );
    assert_eq!(kernel.value(), before);
    kernel.startup_unpark_block(&token).unwrap();
    assert_eq!(kernel.service_next_block(), Some(request));
    let restored = kernel.value();
    assert_eq!(
        kernel.startup_unpark_block(&token),
        Err(MainStepError::InvalidInput)
    );
    assert_eq!(kernel.value(), restored);
}
#[test]
fn parked_actual_block_leaves_independent_real_source_delay_schedulable() {
    let (mut kernel, request) = raw_held_incoming();
    let token = kernel.startup_park_block(&request).unwrap();
    let started = kernel
        .service_enqueue(
            &mut RawIncomingIo,
            MainStepCommandV1::Start {
                method: "Main.delay".into(),
                arguments: vec![n(300.)],
            },
        )
        .unwrap()
        .started
        .unwrap();
    assert_ne!(started, request.job);
    kernel.service_drain_one(&mut RawIncomingIo).unwrap();
    assert_eq!(kernel.next_timer(), Some(1));
    assert!(kernel
        .jobs()
        .any(|(id, status)| id == started && matches!(status, MainStepJobStatus::WaitingTimer(1))));
    kernel.startup_unpark_block(&token).unwrap();
    assert_eq!(kernel.service_next_block(), Some(request));
    assert_eq!(kernel.next_timer(), Some(1));
}
