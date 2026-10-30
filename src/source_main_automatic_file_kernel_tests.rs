// Kernel/control metadata only. No financial Runtime, index, executable source
// answer or completed native capability is constructed. A test-only child
// status fixture checks routing; the genuine source controls check execution.
use super::*;
struct RefusingMetadataBoundary;
impl sealed::Sealed for RefusingMetadataBoundary {}
impl MainStepOwner for RefusingMetadataBoundary {
    fn preflight(&self, _: &MainStepPrimitive) -> Result<(), SourceRuleError> {
        Err(SourceRuleError::UnknownDependency)
    }
    fn execute(&mut self, _: &MainStepPrimitive) -> MainStepPrimitiveExecution {
        unreachable!("no source/financial execution is admitted by this metadata boundary")
    }
}
fn incoming(kernel: &mut MainStepKernelV1) -> u64 {
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
        .unwrap()
}
#[test]
fn actual_incoming_branch_creates_one_file_child_before_any_source_operation() {
    let mut kernel = MainStepKernelV1::new(91);
    kernel.enable_automatic_native_files().unwrap();
    let parent = incoming(&mut kernel);
    let eval = kernel
        .apply(
            &mut RefusingMetadataBoundary,
            MainStepCommandV1::DrainReady { limit: 1 },
        )
        .unwrap();
    assert!(eval.trace.is_empty());
    assert!(eval.completed.is_empty());
    let prefix = kernel.service_next_file_prefix().unwrap();
    assert_ne!(prefix.job, parent);
    assert_eq!(
        kernel.jobs[kernel.position(parent)].status,
        MainStepJobStatus::WaitingChild(prefix.job)
    );
    assert!(kernel.service_next_block().is_none());
    assert!(kernel.service_has_ready());
    let blocked = kernel
        .service_drain_one(&mut RefusingMetadataBoundary)
        .unwrap();
    assert_eq!(
        blocked.stop,
        MainStepStop::SourceOwnerStopped(SourceRuleError::UnknownDependency)
    );
    assert!(kernel.service_next_file_prefix().is_none());
    assert!(kernel.service_next_block().is_none());
}
#[test]
fn tail_peek_requires_same_ready_file_child_and_exact_parent_request() {
    let mut kernel = MainStepKernelV1::new(91);
    kernel.enable_automatic_native_files().unwrap();
    let parent = incoming(&mut kernel);
    kernel
        .apply(
            &mut RefusingMetadataBoundary,
            MainStepCommandV1::DrainReady { limit: 1 },
        )
        .unwrap();
    let prefix = kernel.service_next_file_prefix().unwrap();
    let at = kernel.position(prefix.job);
    let Frame::File(file) = &mut kernel.jobs[at].frame else {
        panic!("real FileStepFrame child")
    };
    let FileStepFrame::Block { phase, .. } = file.as_mut() else {
        panic!("real block file child")
    };
    *phase = 4;
    assert!(
        kernel.service_next_block().is_none(),
        "a phase label without its actual child completion is insufficient"
    );
    kernel.jobs[at].child_result = Some(MainStepSourceResult::Fulfilled(V::Undefined));
    assert_eq!(kernel.service_next_block(), Some(prefix.clone()));
    assert!(kernel.service_next_file_prefix().is_none());
    let parent_at = kernel.position(parent);
    let Frame::Incoming { number, .. } = &mut kernel.jobs[parent_at].frame else {
        panic!("real Incoming parent")
    };
    *number = n(1001.);
    assert!(
        kernel.service_next_block().is_none(),
        "wrong parent raw request cannot select this tail"
    );
    let Frame::Incoming { number, .. } = &mut kernel.jobs[parent_at].frame else {
        unreachable!()
    };
    *number = n(1000.);
    kernel.jobs[at].status = MainStepJobStatus::WaitingTimer(7);
    assert!(
        kernel.service_next_block().is_none(),
        "a held file await is not a ready native tail"
    );
}
#[test]
fn thrown_checkpoint_child_must_propagate_before_native_tail_selection() {
    let mut kernel = MainStepKernelV1::new(91);
    kernel.enable_automatic_native_files().unwrap();
    incoming(&mut kernel);
    kernel
        .apply(
            &mut RefusingMetadataBoundary,
            MainStepCommandV1::DrainReady { limit: 1 },
        )
        .unwrap();
    let prefix = kernel.service_next_file_prefix().unwrap();
    let at = kernel.position(prefix.job);
    let Frame::File(file) = &mut kernel.jobs[at].frame else {
        unreachable!()
    };
    let FileStepFrame::Block { phase, .. } = file.as_mut() else {
        unreachable!()
    };
    *phase = 4;
    kernel.jobs[at].child_result = Some(MainStepSourceResult::Thrown(MainStepLegacyThrow {
        exception: SourceException {
            name: "Error".into(),
            message: "scheduler-only rejected-child routing fixture".into(),
        },
        code: V::Undefined,
    }));
    assert!(kernel.service_next_block().is_none());
    // The literal child_value branch throws before requesting a primitive.
    let evaluation = kernel
        .service_drain_one(&mut RefusingMetadataBoundary)
        .unwrap();
    assert!(evaluation.trace.is_empty());
    assert!(matches!(
        kernel.jobs[kernel.position(prefix.job)].frame,
        Frame::Finish(MainStepSourceResult::Thrown(_))
    ));
    let evaluation = kernel
        .service_drain_one(&mut RefusingMetadataBoundary)
        .unwrap();
    assert!(
        evaluation
            .completed
            .iter()
            .any(|(id, result)| *id == prefix.job
                && matches!(result, MainStepSourceResult::Thrown(_)))
    );
    assert!(kernel.service_next_block().is_none());
}
#[test]
fn old_modes_keep_their_original_selection_and_running_kernel_cannot_reprofile() {
    let mut legacy = MainStepKernelV1::new(91);
    let parent = incoming(&mut legacy);
    assert_eq!(legacy.source_profile(), SOURCE_PROFILE);
    assert_eq!(legacy.service_next_block().unwrap().job, parent);
    assert!(legacy.enable_automatic_native_files().is_err());
    let mut files = MainStepKernelV1::with_file_retries(91);
    incoming(&mut files);
    files
        .apply(
            &mut RefusingMetadataBoundary,
            MainStepCommandV1::DrainReady { limit: 1 },
        )
        .unwrap();
    assert_eq!(files.source_profile(), FILE_RETRY_PROFILE);
    assert!(files.service_next_file_prefix().is_none());
    assert!(files.service_next_block().is_none());
    let mut pristine = MainStepKernelV1::new(91);
    pristine
        .service_enqueue(
            &mut RefusingMetadataBoundary,
            MainStepCommandV1::Start {
                method: "Main.setPause".into(),
                arguments: vec![],
            },
        )
        .unwrap();
    assert!(pristine.enable_automatic_native_files().is_err());
}
