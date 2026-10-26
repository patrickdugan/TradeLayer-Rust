//! Public Joint behavior stays unchanged while the private economic owner
//! gains a separate snapshot bridge. Full bridge tests live beside its
//! crate-private methods; these protect independent public legacy consumers.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::joint_chain::{JointChainState, UnverifiedJointBlock};
use tradelayer_core::ledger::{AccountId, Ledger, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;

fn admin() -> AccountId {
    "admin".try_into().unwrap()
}

#[test]
fn pristine_tljp_keeps_its_exact_pre_economic_bytes() {
    let state = JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Bitcoin,
        admin(),
        Ledger::new(),
        BTreeSet::new(),
    )
    .unwrap();
    let bytes = state.snapshot_bytes().unwrap();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, concat!(
        "544c4a500101000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
        "0000004d544c524701000561646d696e0000000000000000000000000000000000000000000000000000000000000000000000000000000100000001000000000000000000000000000000000028466dd8",
        "0000003e544c5350010000000000000014544c4c01000000000000000000000000c907a2760000000000000000000000000000000000000000000000000054475be3",
        "000000000000000000323e574e",
    ));
    assert_eq!(
        JointProtocolState::from_snapshot_bytes(
            &bytes,
            TradeLayerMainnet::Bitcoin,
            &admin(),
            &BTreeSet::new(),
        )
        .unwrap(),
        state
    );
}

#[test]
fn legacy_joint_chain_never_gains_economic_opcode_authority() {
    let network = TradeLayerMainnet::Bitcoin;
    let mut chain = JointChainState::new_unverified(
        network,
        Tip {
            height: 0,
            id: network.genesis(),
        },
        admin(),
        Ledger::new(),
        BTreeSet::new(),
        4,
    )
    .unwrap();
    let before = chain.clone();
    for ty in [18, 19, 23, 31] {
        let operation = if matches!(ty, 23 | 31) {
            JointOperation::EconomicSettlement { ty }
        } else {
            JointOperation::Unsupported { ty }
        };
        let block = UnverifiedJointBlock {
            network,
            header: BlockHeader {
                height: 1,
                id: BlockId([50; 32]),
                parent: network.genesis(),
            },
            transactions: vec![UnverifiedJointTx {
                txid: TxId::from_bytes([ty; 32]),
                actor: admin(),
                operation,
            }],
        };
        assert!(chain.apply_unverified_block(&block).is_err());
        assert_eq!(chain, before);
    }
}
