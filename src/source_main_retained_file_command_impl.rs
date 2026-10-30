// Explicit domain admission, before any kernel or source mutation. Automatic
// Main.processBlock and factual updates need their own retained composition.
pub(crate) fn native_main_retained_file_command_admitted(
    command: &crate::source_main_step_kernel::MainStepCommandV1,
) -> bool {
    use crate::source_main_step_kernel::MainStepCommandV1 as Command;
    match command {
        Command::Start { method, .. } => [
            "Persistence.safeCopy",
            "Persistence.safeDelete",
            "Persistence.createSnapshot",
            "Persistence.restoreSnapshot",
            "Main.setPause",
            "Main.saveTrackHeight",
            "Main.loadTrackHeight",
            "Main.loadMaxProcessedHeight",
        ]
        .contains(&method.as_str()),
        Command::DrainReady { .. }
        | Command::FireTimer { .. }
        | Command::ClearTimer { .. }
        | Command::Shutdown(_) => true,
        Command::UpdateRecordedIo(_) => false,
    }
}
