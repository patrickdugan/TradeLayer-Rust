// Scheduler metadata only. The fulfilled child below is a routing fixture,
// never an executable financial result or native completion capability.
use super::*;
struct Refusing;
impl sealed::Sealed for Refusing {}
impl MainStepOwner for Refusing {
    fn preflight(&self, _: &MainStepPrimitive) -> Result<(), SourceRuleError> {
        Err(SourceRuleError::UnknownDependency)
    }
    fn execute(&mut self, _: &MainStepPrimitive) -> MainStepPrimitiveExecution {
        unreachable!("metadata-only boundary")
    }
}
fn tail() -> (MainStepKernelV1, MainServiceBlockRequest) {
    let mut kernel = MainStepKernelV1::new(91);
    kernel.enable_automatic_native_files().unwrap();
    kernel
        .spawn(
            Frame::Incoming {
                latest: n(999.),
                tip: n(1000.),
                block: obj(&[("hash", s("raw1000")), ("tx", V::Array(vec![]))]),
                number: n(1000.),
                phase: 7,
            },
            None,
            false,
        )
        .unwrap();
    kernel.service_drain_one(&mut Refusing).unwrap();
    let request = kernel.service_next_file_prefix().unwrap();
    let at = kernel.position(request.job);
    let Frame::File(file) = &mut kernel.jobs[at].frame else {
        unreachable!()
    };
    let FileStepFrame::Block { phase, .. } = file.as_mut() else {
        unreachable!()
    };
    *phase = 4;
    kernel.jobs[at].child_result = Some(MainStepSourceResult::Fulfilled(V::Undefined));
    (kernel, request)
}
fn canonical(kernel: &MainStepKernelV1) -> Vec<(String, V)> {
    let mut result = vec![];
    kernel
        .visit_canonical_fields(&mut |p, v| {
            result.push((p.to_owned(), v.clone()));
            Ok(())
        })
        .unwrap();
    result
}
#[test]
fn exact_parked_native_tail_leaves_independent_ready_job_schedulable() {
    let (mut kernel, request) = tail();
    let extra = kernel
        .spawn(
            Frame::Atomic(DependencyCall {
                target: "Main.setPause".into(),
                arguments: vec![],
            }),
            None,
            true,
        )
        .unwrap();
    kernel.service_park_automatic_file_tail(&request).unwrap();
    assert_eq!(kernel.ready.front(), Some(&extra));
    assert!(kernel.service_next_block().is_none());
    assert_eq!(
        kernel.jobs[kernel.position(request.job)].status,
        MainStepJobStatus::Ready
    );
    assert!(kernel.service_has_ready());
    assert_eq!(
        kernel.service_enqueue(
            &mut Refusing,
            MainStepCommandV1::Start {
                method: "Main.setPause".into(),
                arguments: vec![]
            }
        ),
        Err(MainStepError::NotQuiescent),
        "actually ready independent work keeps the original quiescence fence"
    );
    kernel.service_unpark_automatic_file_tail(&request).unwrap();
    assert_eq!(kernel.ready.front(), Some(&request.job));
    assert_eq!(kernel.service_next_block(), Some(request));
    assert!(kernel.ready.contains(&extra));
}
#[test]
fn foreign_or_duplicate_park_unpark_cannot_displace_actual_native_tail() {
    let (mut kernel, request) = tail();
    let mut foreign = request.clone();
    foreign.height = n(1001.);
    let initial = canonical(&kernel);
    assert!(kernel.service_park_automatic_file_tail(&foreign).is_err());
    assert_eq!(canonical(&kernel), initial);
    kernel.service_park_automatic_file_tail(&request).unwrap();
    let held = canonical(&kernel);
    assert!(kernel.service_park_automatic_file_tail(&request).is_err());
    assert!(kernel.service_unpark_automatic_file_tail(&foreign).is_err());
    assert_eq!(canonical(&kernel), held);
    kernel.service_unpark_automatic_file_tail(&request).unwrap();
    assert!(kernel.service_unpark_automatic_file_tail(&request).is_err());
}
#[test]
fn parked_request_is_fully_bound_even_while_absent_from_ready_queue() {
    let (mut kernel, request) = tail();
    kernel.service_park_automatic_file_tail(&request).unwrap();
    let mut fields: Vec<(String, V)> = vec![];
    kernel
        .visit_canonical_fields(&mut |p, v| {
            fields.push((p.into(), v.clone()));
            Ok(())
        })
        .unwrap();
    assert!(fields.contains(&(
        "automaticFile/nativeTailParked/present".into(),
        V::Bool(true)
    )));
    assert!(
        fields
            .iter()
            .any(|(p, v)| p == "automaticFile/nativeTailParked/block/hash" && v == &s("raw1000"))
            || fields.iter().any(
                |(p, v)| p.starts_with("automaticFile/nativeTailParked/block/")
                    && v == &s("raw1000")
            )
    );
    assert!(fields
        .iter()
        .any(|(p, v)| p == "automaticFile/nativeTailParked/height/atom" && v == &n(1000.)));
}
#[test]
fn legacy_profile_cannot_enter_or_expose_automatic_park_state() {
    let mut legacy = MainStepKernelV1::new(91);
    let (_, request) = tail();
    let before = canonical(&legacy);
    assert!(legacy.service_park_automatic_file_tail(&request).is_err());
    assert!(legacy.service_unpark_automatic_file_tail(&request).is_err());
    assert_eq!(canonical(&legacy), before);
    let mut fields: Vec<(String, V)> = vec![];
    legacy
        .visit_canonical_fields(&mut |p, v| {
            fields.push((p.to_owned(), v.clone()));
            Ok(())
        })
        .unwrap();
    assert!(fields.iter().all(|(p, _)| !p.starts_with("automaticFile/")));
}
#[test]
fn literal_exit_keeps_parked_tail_unresolved_and_forbids_unpark() {
    let (mut kernel, request) = tail();
    kernel.service_park_automatic_file_tail(&request).unwrap();
    let evaluation = kernel
        .service_enqueue(&mut Refusing, MainStepCommandV1::Shutdown(vec![]))
        .unwrap();
    assert_eq!(evaluation.stop, MainStepStop::ProcessExited(0));
    let exited = canonical(&kernel);
    assert!(kernel.service_unpark_automatic_file_tail(&request).is_err());
    assert_eq!(canonical(&kernel), exited);
    assert!(kernel.native_file_tail_parked.is_some());
}
