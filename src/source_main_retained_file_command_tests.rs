use super::*;
use crate::source_main_step_kernel::MainStepCommandV1 as Command;
use crate::validity_source::SourceValue as V;
#[test]
fn file_domain_refuses_whole_main_native_dispatch_and_factual_update_before_execution() {
    for method in [
        "Main.processBlock",
        "Main.processTx",
        "Main.initialize",
        "Main.syncIndex",
        "Main.processIncomingBlocks",
        "SourceMain.configureService",
    ] {
        assert!(!native_main_retained_file_command_admitted(
            &Command::Start {
                method: method.into(),
                arguments: vec![]
            }
        ));
    }
    assert!(!native_main_retained_file_command_admitted(
        &Command::UpdateRecordedIo(V::Object(vec![]))
    ));
}
#[test]
fn file_domain_admits_source_file_jobs_and_explicit_pause_height_controls() {
    for method in [
        "Persistence.safeCopy",
        "Persistence.safeDelete",
        "Persistence.createSnapshot",
        "Persistence.restoreSnapshot",
        "Main.setPause",
        "Main.saveTrackHeight",
        "Main.loadTrackHeight",
        "Main.loadMaxProcessedHeight",
    ] {
        let command = Command::Start {
            method: method.into(),
            arguments: vec![],
        };
        assert!(native_main_retained_file_command_admitted(&command));
        assert_eq!(Command::from_value(&command.value()).unwrap(), command);
    }
}
#[test]
fn file_domain_preserves_exact_timer_generation_and_shutdown_input_fields() {
    for command in [
        Command::FireTimer {
            id: u64::MAX - 1,
            generation: 91,
        },
        Command::ClearTimer {
            id: 1,
            generation: u64::MAX,
        },
        Command::DrainReady { limit: 1 },
        Command::Shutdown(vec![V::String("literal source input".into())]),
    ] {
        assert!(native_main_retained_file_command_admitted(&command));
        assert_eq!(Command::from_value(&command.value()).unwrap(), command);
    }
}
