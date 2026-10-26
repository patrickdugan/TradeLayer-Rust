//! TLJC v2 represents an exhausted retained-history floor without inventing
//! an ancestor witness or changing the independent original deployment anchor.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::joint_chain::{JointChainError, JointChainState, UnverifiedJointBlock};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;

const TIP_OFFSET: usize = 110;
const LIMIT_OFFSET: usize = 150;
const COUNT_OFFSET: usize = 151;
const FLOOR_OFFSET: usize = 152;
const TIP_BYTES: usize = 40;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "tl-joint-floor-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && self.0.starts_with(std::env::temp_dir()));
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("tl-joint-floor-"));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn a(text: &str) -> AccountId {
    text.try_into().unwrap()
}
fn property() -> PropertyId {
    PropertyId::try_from(7).unwrap()
}
fn permitted() -> BTreeSet<PropertyId> {
    BTreeSet::from([property()])
}
fn id(value: u8) -> TxId {
    TxId::from_bytes([value; 32])
}
fn anchor(network: TradeLayerMainnet) -> Tip {
    Tip {
        height: 0,
        id: network.genesis(),
    }
}
fn initial(network: TradeLayerMainnet) -> JointChainState {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(200),
            Operation::CreateProperty {
                id: property(),
                issuer: a("seller"),
                kind: PropertyKind::Fixed,
                initial_atoms: 1000,
            },
        )
        .unwrap();
    JointChainState::new_unverified(network, anchor(network), a("admin"), ledger, permitted(), 2)
        .unwrap()
}
fn block(
    state: &JointChainState,
    hash: u8,
    transactions: Vec<UnverifiedJointTx>,
) -> UnverifiedJointBlock {
    UnverifiedJointBlock {
        network: state.network(),
        header: BlockHeader {
            height: state.tip().height + 1,
            parent: state.tip().id,
            id: BlockId([hash; 32]),
        },
        transactions,
    }
}
fn activate() -> UnverifiedJointTx {
    UnverifiedJointTx {
        txid: id(10),
        actor: a("admin"),
        operation: JointOperation::Registry(RegistryAction::Activate {
            types: vec![2],
            code_hash: [9; 32],
        }),
    }
}
fn send(tx: u8, amount: u128) -> UnverifiedJointTx {
    UnverifiedJointTx {
        txid: id(tx),
        actor: a("seller"),
        operation: JointOperation::Send {
            id: property(),
            recipient: a("alice"),
            amount_atoms: amount,
        },
    }
}
fn floor_state(network: TradeLayerMainnet) -> JointChainState {
    let mut state = initial(network);
    state
        .apply_unverified_block(&block(&state, 21, vec![activate()]))
        .unwrap();
    state
        .apply_unverified_block(&block(&state, 22, vec![send(12, 10)]))
        .unwrap();
    let floor_tip = state.tip();
    let expected_components = state.state().clone();
    state
        .apply_unverified_block(&block(&state, 23, vec![send(13, 20)]))
        .unwrap();
    state
        .apply_unverified_block(&block(&state, 24, vec![send(14, 30)]))
        .unwrap();
    assert_eq!(state.rollback_depth(), 2);
    assert_eq!(state.rollback_floor(), floor_tip);
    assert_eq!(
        state
            .state()
            .ledger()
            .available_atoms(property(), &a("alice")),
        60
    );
    state.rollback_tip(state.tip()).unwrap();
    state.rollback_tip(state.tip()).unwrap();
    assert_eq!(state.tip(), floor_tip);
    assert_eq!(state.state(), &expected_components);
    assert_eq!(state.rollback_depth(), 0);
    state
}
fn decode(
    bytes: &[u8],
    network: TradeLayerMainnet,
) -> Result<JointChainState, tradelayer_core::joint_chain::JointCheckpointError> {
    JointChainState::from_checkpoint_bytes(
        bytes,
        network,
        anchor(network),
        &a("admin"),
        &permitted(),
        2,
    )
}
fn crc(bytes: &[u8]) -> u32 {
    let mut value = !0u32;
    for byte in bytes {
        value ^= u32::from(*byte);
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(value & 1)));
        }
    }
    !value
}
fn checksum(mut bytes: Vec<u8>) -> Vec<u8> {
    let length = bytes.len() - 4;
    let sum = crc(&bytes[..length]);
    bytes[length..].copy_from_slice(&sum.to_be_bytes());
    bytes
}
fn push_tip(out: &mut Vec<u8>, tip: Tip) {
    out.extend_from_slice(&tip.height.to_be_bytes());
    out.extend_from_slice(&tip.id.0);
}
fn push_state(out: &mut Vec<u8>, state: &JointProtocolState) {
    let raw = state.snapshot_bytes().unwrap();
    out.extend_from_slice(&u32::try_from(raw.len()).unwrap().to_be_bytes());
    out.extend_from_slice(&raw);
}
/// Frozen original TLJC-v1 layout, independent of encoder version selection.
fn reference_v1(state: &JointChainState, prior: &[(Tip, JointProtocolState)]) -> Vec<u8> {
    let mut raw = b"TLJC\x01".to_vec();
    raw.push(state.network().tag());
    raw.extend_from_slice(&state.network().genesis().0);
    raw.extend_from_slice(&state.network().chain_id().0);
    push_tip(&mut raw, state.anchor());
    push_tip(&mut raw, state.tip());
    raw.push(2);
    raw.push(u8::try_from(prior.len()).unwrap());
    for (tip, components) in prior {
        push_tip(&mut raw, *tip);
        push_state(&mut raw, components);
    }
    push_state(&mut raw, state.state());
    raw.extend_from_slice(&[0; 4]);
    checksum(raw)
}

#[test]
fn complete_oldest_retained_state_round_trips_for_all_three_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let floor = floor_state(network);
        let bytes = floor.checkpoint_bytes().unwrap();
        assert_eq!(&bytes[..5], b"TLJC\x02");
        assert_eq!(bytes[LIMIT_OFFSET], 2);
        assert_eq!(bytes[COUNT_OFFSET], 0);
        assert_eq!(
            &bytes[FLOOR_OFFSET..FLOOR_OFFSET + TIP_BYTES],
            &bytes[TIP_OFFSET..TIP_OFFSET + TIP_BYTES]
        );
        let mut restored = decode(&bytes, network).unwrap();
        assert_eq!(restored, floor);
        assert_eq!(restored.checkpoint_bytes().unwrap(), bytes);
        assert_eq!(restored.anchor(), anchor(network));
        assert_eq!(restored.rollback_floor(), restored.tip());
        assert_eq!(restored.state().finalized_height(), Some(2));
        assert_eq!(restored.state().registry().activation(2).unwrap().height, 1);
        assert_eq!(restored.state().processed_count(), 2);
        assert!(restored.state().registry().is_processed(id(10)));
        assert!(restored.state().ledger().is_processed(id(12)));
        assert!(!restored.state().ledger().is_processed(id(13)));
        assert!(!restored.state().ledger().is_processed(id(14)));
        assert_eq!(
            restored
                .state()
                .ledger()
                .available_atoms(property(), &a("alice")),
            10
        );
        assert_eq!(
            restored
                .state()
                .ledger()
                .available_atoms(property(), &a("seller")),
            990
        );
        restored.state().verify_components_unverified().unwrap();
        assert!(matches!(
            restored.rollback_tip(restored.tip()),
            Err(JointChainError::EmptyRollbackHistory)
        ));
        assert_eq!(restored, floor);
        let wrong = Tip {
            height: restored.tip().height,
            id: BlockId([254; 32]),
        };
        assert!(matches!(
            restored.rollback_tip(wrong),
            Err(JointChainError::WrongRollbackTip)
        ));
        assert_eq!(restored, floor);
    }
}

#[test]
fn ordinary_v1_bytes_and_its_empty_history_rejection_remain_unchanged() {
    let network = TradeLayerMainnet::Bitcoin;
    let mut state = initial(network);
    assert_eq!(state.checkpoint_bytes().unwrap(), reference_v1(&state, &[]));
    let prior = (state.tip(), state.state().clone());
    state
        .apply_unverified_block(&block(&state, 21, vec![activate()]))
        .unwrap();
    assert_eq!(
        state.checkpoint_bytes().unwrap(),
        reference_v1(&state, &[prior])
    );
    assert_eq!(&state.checkpoint_bytes().unwrap()[..5], b"TLJC\x01");
    assert_eq!(
        decode(&state.checkpoint_bytes().unwrap(), network).unwrap(),
        state
    );
    let floor = floor_state(network);
    let legacy_gap = reference_v1(&floor, &[]);
    assert!(
        decode(&legacy_gap, network).is_err(),
        "v1 must still reject above-anchor states lacking prior witnesses"
    );
    let mut downgraded = floor.checkpoint_bytes().unwrap();
    downgraded[4] = 1;
    downgraded.drain(FLOOR_OFFSET..FLOOR_OFFSET + TIP_BYTES);
    assert_eq!(checksum(downgraded), legacy_gap);
}

#[test]
fn restored_floor_can_extend_and_return_to_identical_canonical_v2() {
    let network = TradeLayerMainnet::Litecoin;
    let floor = floor_state(network);
    let canonical = floor.checkpoint_bytes().unwrap();
    let mut resumed = decode(&canonical, network).unwrap();
    resumed
        .apply_unverified_block(&block(&resumed, 43, vec![send(13, 15)]))
        .unwrap();
    assert_eq!(resumed.rollback_depth(), 1);
    assert_eq!(resumed.rollback_floor(), floor.tip());
    assert_eq!(
        resumed
            .state()
            .ledger()
            .available_atoms(property(), &a("alice")),
        25
    );
    let ordinary = resumed.checkpoint_bytes().unwrap();
    assert_eq!(&ordinary[..5], b"TLJC\x01");
    let mut resumed = decode(&ordinary, network).unwrap();
    resumed.rollback_tip(resumed.tip()).unwrap();
    assert_eq!(resumed, floor);
    assert_eq!(resumed.checkpoint_bytes().unwrap(), canonical);
}

#[test]
fn explicit_floor_and_deployment_configuration_fail_closed_after_crc_repair() {
    let network = TradeLayerMainnet::Dogecoin;
    let bytes = floor_state(network).checkpoint_bytes().unwrap();
    let mut cases = Vec::new();
    let mut changed = bytes.clone();
    changed[FLOOR_OFFSET + 7] ^= 1;
    cases.push(changed);
    let mut changed = bytes.clone();
    changed[FLOOR_OFFSET + 8] ^= 1;
    cases.push(changed);
    let mut changed = bytes.clone();
    changed[COUNT_OFFSET] = 1;
    cases.push(changed);
    let mut changed = bytes.clone();
    changed[4] = 3;
    cases.push(changed);
    let mut changed = bytes.clone();
    changed[6] ^= 1;
    cases.push(changed);
    let mut changed = bytes.clone();
    changed[38] ^= 1;
    cases.push(changed);
    let mut changed = bytes.clone();
    changed[78] ^= 1;
    cases.push(changed);
    // Keep floor==tip, but claim a different finalized height. Nested state
    // verification must reject this even with a recomputed valid outer CRC.
    let mut changed = bytes.clone();
    changed[TIP_OFFSET..TIP_OFFSET + 8].copy_from_slice(&3u64.to_be_bytes());
    changed[FLOOR_OFFSET..FLOOR_OFFSET + 8].copy_from_slice(&3u64.to_be_bytes());
    cases.push(changed);
    let mut changed = bytes.clone();
    changed[FLOOR_OFFSET + TIP_BYTES..FLOOR_OFFSET + TIP_BYTES + 4]
        .copy_from_slice(&u32::MAX.to_be_bytes());
    cases.push(changed);
    let mut changed = bytes.clone();
    changed.drain(FLOOR_OFFSET..FLOOR_OFFSET + TIP_BYTES);
    cases.push(changed);
    let mut changed = bytes.clone();
    changed.insert(changed.len() - 4, 0);
    cases.push(changed);
    for changed in cases {
        assert!(decode(&checksum(changed), network).is_err());
    }
    assert!(JointChainState::from_checkpoint_bytes(
        &bytes,
        network,
        anchor(network),
        &a("wrong-admin"),
        &permitted(),
        2
    )
    .is_err());
    assert!(JointChainState::from_checkpoint_bytes(
        &bytes,
        network,
        anchor(network),
        &a("admin"),
        &BTreeSet::new(),
        2
    )
    .is_err());
    assert!(JointChainState::from_checkpoint_bytes(
        &bytes,
        network,
        anchor(network),
        &a("admin"),
        &permitted(),
        3
    )
    .is_err());
    let wrong_anchor = Tip {
        height: 1,
        id: BlockId([1; 32]),
    };
    assert!(JointChainState::from_checkpoint_bytes(
        &bytes,
        network,
        wrong_anchor,
        &a("admin"),
        &permitted(),
        2
    )
    .is_err());
    assert!(decode(&bytes, TradeLayerMainnet::Bitcoin).is_err());
}

#[test]
fn v2_is_rejected_for_v1_representable_states_and_all_truncated_prefixes() {
    let network = TradeLayerMainnet::Bitcoin;
    let mut state = initial(network);
    // Test the bootstrap version first, then an ordinary one-frame state.
    let mut upgrade = state.checkpoint_bytes().unwrap();
    upgrade[4] = 2;
    let tip = upgrade[TIP_OFFSET..TIP_OFFSET + TIP_BYTES].to_vec();
    upgrade.splice(FLOOR_OFFSET..FLOOR_OFFSET, tip);
    assert!(decode(&checksum(upgrade), network).is_err());
    state
        .apply_unverified_block(&block(&state, 21, vec![activate()]))
        .unwrap();
    let mut upgrade = state.checkpoint_bytes().unwrap();
    upgrade[4] = 2;
    let tip = upgrade[TIP_OFFSET..TIP_OFFSET + TIP_BYTES].to_vec();
    upgrade.splice(FLOOR_OFFSET..FLOOR_OFFSET, tip);
    assert!(decode(&checksum(upgrade), network).is_err());
    let bytes = floor_state(network).checkpoint_bytes().unwrap();
    for length in 0..bytes.len() {
        assert!(
            decode(&bytes[..length], network).is_err(),
            "prefix {length}"
        );
    }
}

#[test]
fn immutable_checkpoint_file_can_recover_the_floor_without_changing_anchor() {
    let directory = Directory::new();
    let network = TradeLayerMainnet::Litecoin;
    let floor = floor_state(network);
    let path = directory.0.join("exhausted-floor.tljc");
    floor.write_new_checkpoint(&path).unwrap();
    assert_eq!(
        JointChainState::load_checkpoint(
            &path,
            network,
            anchor(network),
            &a("admin"),
            &permitted(),
            2
        )
        .unwrap(),
        floor
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        floor.checkpoint_bytes().unwrap()
    );
    assert!(floor.write_new_checkpoint(&path).is_err());
}
