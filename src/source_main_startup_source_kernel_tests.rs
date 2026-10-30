use crate::validity_source;
#[path = "source_main_startup_promise_component_tests.rs"]
mod component_tests;
#[path = "source_main_startup_promise_key_tests.rs"]
mod key_visitor_tests;
#[path = "source_main_startup_nedb_jobs_tests.rs"]
mod nedb_jobs_tests;
#[path = "source_main_startup_notification_order_tests.rs"]
mod notification_order_tests;
#[path = "source_main_startup_promise_payload_tests.rs"]
mod payload_tests;
use crate::source_main_step_kernel;
use source_main_step_kernel::startup_source_promise_queue::{ActualSettlement, Realm};
use source_main_step_kernel::*;
use std::collections::BTreeMap;
use validity_source::{SourceRuleError, SourceValue as V};
struct NeverOwner;
impl sealed::Sealed for NeverOwner {}
impl MainStepOwner for NeverOwner {
    fn preflight(&self, _: &MainStepPrimitive) -> Result<(), SourceRuleError> {
        panic!("no financial/source owner may run in this metadata control")
    }
    fn execute(&mut self, _: &MainStepPrimitive) -> MainStepPrimitiveExecution {
        panic!("no financial/source owner may run in this metadata control")
    }
}
fn fields(kernel: &MainStepKernelV1) -> BTreeMap<String, V> {
    let mut out = BTreeMap::new();
    kernel
        .visit_canonical_fields(&mut |path, value| {
            assert!(out.insert(path.into(), value.clone()).is_none(), "{path}");
            Ok(())
        })
        .unwrap();
    out
}
fn default_kernel_observation(kernel: &MainStepKernelV1) -> String {
    let mut out = format!("value\t{:?}\n", kernel.value());
    kernel
        .visit_canonical_fields(&mut |path, value| {
            out.push_str(&format!("field\t{path}\t{value:?}\n"));
            Ok(())
        })
        .unwrap();
    out
}
#[test]
fn unchanged_default_value_and_complete_field_stream_match_actual_old_provider() {
    // Captured from the actual pre-mount committed Kernel rlib.
    let kernel = MainStepKernelV1::new(9);
    assert_eq!(
        default_kernel_observation(&kernel),
        include_str!("../fixtures/startup_kernel_default_v1.txt")
    );
}
#[test]
fn actual_same_kernel_owns_source_promises_and_old_command_cannot_drain_or_start_them() {
    let mut kernel = MainStepKernelV1::new(9);
    kernel.startup_install_source_promises().unwrap();
    assert_eq!(kernel.source_profile(), "TL-MAIN-VM-BUSINESS-HOST-IO-13-1");
    let queue = kernel.startup_source_promises().unwrap();
    let host = queue.create(Realm::RecordedHostIo).unwrap();
    queue
        .resolve_actual(host, &ActualSettlement::Fulfilled(V::number(4.)))
        .unwrap();
    queue.await_actual(Realm::SourceBusiness, host, 1).unwrap();
    let before = fields(&kernel);
    for command in [
        MainStepCommandV1::Start {
            method: "Main.syncIfNecessary".into(),
            arguments: vec![],
        },
        MainStepCommandV1::DrainReady { limit: 1 },
        MainStepCommandV1::UpdateRecordedIo(V::Object(vec![])),
    ] {
        assert_eq!(
            kernel.apply(&mut NeverOwner, command),
            Err(MainStepError::InvalidInput)
        );
        assert_eq!(fields(&kernel), before);
    }
    let actual = kernel
        .startup_source_promises()
        .unwrap()
        .next_delivery()
        .unwrap()
        .unwrap();
    assert_eq!(actual.continuation, 1);
    assert_eq!(
        actual.settlement,
        ActualSettlement::Fulfilled(V::number(4.))
    );
    kernel
        .startup_source_promises()
        .unwrap()
        .acknowledge(actual.ticket)
        .unwrap();
    assert!(kernel
        .startup_source_promises()
        .unwrap()
        .next_delivery()
        .unwrap()
        .is_none());
}
#[test]
fn source_profile_install_cannot_replace_pending_old_source_job_or_reinitialize_queue() {
    let mut kernel = MainStepKernelV1::new(9);
    kernel
        .service_enqueue(
            &mut NeverOwner,
            MainStepCommandV1::Start {
                method: "Main.syncIfNecessary".into(),
                arguments: vec![],
            },
        )
        .unwrap();
    let before = fields(&kernel);
    assert_eq!(
        kernel.startup_install_source_promises(),
        Err(MainStepError::InvalidInput)
    );
    assert_eq!(fields(&kernel), before);
    let mut source = MainStepKernelV1::new(9);
    source.startup_install_source_promises().unwrap();
    let before = fields(&source);
    assert_eq!(
        source.startup_install_source_promises(),
        Err(MainStepError::InvalidInput)
    );
    assert_eq!(fields(&source), before);
}
#[test]
fn literal_shutdown_does_not_drain_source_promises_and_leaves_their_actual_custody_visible() {
    let mut kernel = MainStepKernelV1::new(9);
    kernel.startup_install_source_promises().unwrap();
    let promise = kernel
        .startup_source_promises()
        .unwrap()
        .create(Realm::SourceBusiness)
        .unwrap();
    kernel
        .startup_source_promises()
        .unwrap()
        .resolve_actual(promise, &ActualSettlement::Fulfilled(V::number(4.)))
        .unwrap();
    kernel
        .startup_source_promises()
        .unwrap()
        .await_actual(Realm::SourceBusiness, promise, 1)
        .unwrap();
    let before = fields(&kernel);
    let done = kernel
        .apply(&mut NeverOwner, MainStepCommandV1::Shutdown(vec![]))
        .unwrap();
    assert_eq!(done.stop, MainStepStop::ProcessExited(0));
    let after = fields(&kernel);
    for (key, value) in before
        .iter()
        .filter(|(key, _)| key.starts_with("startupPromises/"))
    {
        assert_eq!(after.get(key), Some(value));
    }
    assert_eq!(
        kernel.startup_source_promises().err(),
        Some(MainStepError::AlreadyExited)
    );
}
#[test]
fn canonical_source_promise_visitor_refusal_never_mutates_retained_reactions() {
    let mut kernel = MainStepKernelV1::new(9);
    kernel.startup_install_source_promises().unwrap();
    kernel
        .startup_source_promises()
        .unwrap()
        .create(Realm::SourceBusiness)
        .unwrap();
    let before = fields(&kernel);
    assert_eq!(
        kernel.visit_canonical_fields(&mut |path, _| if path.starts_with("startupPromises/") {
            Err(SourceRuleError::Capacity)
        } else {
            Ok(())
        }),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(fields(&kernel), before);
}
struct RecordedCountFailure(bool);
impl sealed::Sealed for RecordedCountFailure {}
impl MainStepOwner for RecordedCountFailure {
    fn preflight(&self, operation: &MainStepPrimitive) -> Result<(), SourceRuleError> {
        assert!(matches!(operation, MainStepPrimitive::BlockCount));
        Ok(())
    }
    fn execute(&mut self, operation: &MainStepPrimitive) -> MainStepPrimitiveExecution {
        assert!(matches!(operation, MainStepPrimitive::BlockCount));
        MainStepPrimitiveExecution {
            outcome: if self.0 {
                MainStepPrimitiveOutcome::Stopped(SourceRuleError::Capacity)
            } else {
                MainStepPrimitiveOutcome::Thrown(MainStepLegacyThrow {
                    exception: validity_source::SourceException {
                        name: "Error".into(),
                        message: "recorded raw RPC rejection".into(),
                    },
                    code: V::String("EIO".into()),
                })
            },
            effects: vec![],
        }
    }
}
#[test]
fn legacy_metadata_count_rejection_and_local_stop_remain_distinct() {
    for local_stop in [false, true] {
        let mut kernel = MainStepKernelV1::new(9);
        let evaluation = kernel
            .apply(
                &mut RecordedCountFailure(local_stop),
                MainStepCommandV1::Start {
                    method: "Main.processIncomingBlocks".into(),
                    arguments: vec![V::Undefined, V::number(1.), V::number(2.)],
                },
            )
            .unwrap();
        if local_stop {
            assert_eq!(
                evaluation.stop,
                MainStepStop::SourceOwnerStopped(SourceRuleError::Capacity)
            );
            assert!(evaluation.completed.is_empty());
        } else {
            assert!(
                matches!(&evaluation.completed[..],[(_,MainStepSourceResult::Thrown(error))] if error.exception.message=="recorded raw RPC rejection"&&error.code==V::String("EIO".into()))
            );
        }
    }
}
