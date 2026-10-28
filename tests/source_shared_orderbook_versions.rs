//! A real UNPROMOTED pre-correction V11 checkpoint is rejected, never relabeled.
use tradelayer_core::source_contract_replay::{
    SourceContractReplay, SourceContractReplayError as E, SourceContractReplayHead as H,
};
const OLD: &[u8] = include_bytes!("../fixtures/source_contract_replay_v11_pre_shared_owner.bin");
fn hash(text: &str) -> [u8; 32] {
    let mut h = [0; 32];
    for (i, b) in h.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap();
    }
    h
}
#[test]
fn stale_unpromoted_v11_shadow_collection_history_is_not_silently_reinterpreted() {
    assert_eq!(OLD.len(), 1081);
    assert_eq!(&OLD[..5], b"TLCR\x0b");
    let old = H {
        commands: 2,
        commitment: hash("580de1a03edf5b96799428f661b496ae557edd5542e41c155eb4103ef5ca0834"),
    };
    assert_eq!(SourceContractReplay::from_checkpoint(OLD,old),Err(E::ReplayMismatch),"old real snapshot/checksum/independent old head are valid, but corrected source state differs; explicit fresh history/head required");
}
