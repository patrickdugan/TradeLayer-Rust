use tradelayer_core::source_contract_replay::{
    SourceContractReplay, SourceContractReplayError, SourceContractReplayHead,
};
fn control() -> SourceContractReplayHead {
    SourceContractReplayHead {
        commands: 4,
        commitment: [
            9, 189, 75, 121, 110, 199, 242, 90, 150, 197, 65, 117, 242, 19, 28, 69, 125, 244, 82,
            130, 13, 136, 187, 2, 17, 84, 161, 235, 78, 123, 88, 108,
        ],
    }
}
fn refusal() -> SourceContractReplayHead {
    SourceContractReplayHead {
        commands: 5,
        commitment: [
            99, 195, 244, 44, 224, 249, 227, 78, 17, 38, 253, 90, 86, 121, 66, 99, 108, 64, 162,
            201, 237, 208, 179, 174, 239, 131, 152, 77, 216, 37, 2, 211,
        ],
    }
}
#[test]
fn ordinary_actual_pre_snapshot_v12_bytes_remain_admitted_and_canonical() {
    let bytes = include_bytes!("../fixtures/main_v12_before_snapshot_control.bin");
    let replay = SourceContractReplay::from_checkpoint(bytes, control())
        .expect("actual prior-provider ordinary V12 history");
    assert_eq!(replay.checkpoint().unwrap(), bytes);
}
#[test]
fn earlier_unpromoted_v12_snapshot_capability_refusal_is_not_relabelled() {
    let bytes = include_bytes!("../fixtures/main_v12_old_snapshot_refusal.bin");
    assert_eq!(
        SourceContractReplay::from_checkpoint(bytes, refusal()),
        Err(SourceContractReplayError::ReplayMismatch),
        "the actual earlier capability-error history must fail reexecution"
    );
}
