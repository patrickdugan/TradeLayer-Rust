extern crate tradelayer_core as native_owner;
#[path = "support/native_fixture.rs"]
mod support;
use native_owner::chain::{BlockHeader, BlockId, Tip};
use native_owner::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use native_owner::joint_chain::{JointChainState, UnverifiedJointBlock};
use native_owner::native_entitlement::NativeDriver;
use native_owner::network_profile::TradeLayerMainnet;
use native_owner::registry::RegistryAction;
use std::collections::BTreeSet;
use support::*;
fn block(
    parent: Tip,
    id: u8,
    inputs: Vec<UnverifiedJointTx>,
    network: TradeLayerMainnet,
) -> UnverifiedJointBlock {
    UnverifiedJointBlock {
        network,
        header: BlockHeader {
            height: parent.height + 1,
            parent: parent.id,
            id: BlockId([id; 32]),
        },
        transactions: inputs,
    }
}
#[test]
fn actual_joint_owner_couples_type2_and_restores_rolls_back_and_rebranches_on_three_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let (mut ledger, policy) = fixture(network, NativeDriver::MonotonicDeltaV1);
        ledger
            .adopt_native_entitlements_unverified(&policy)
            .unwrap();
        assert!(JointProtocolState::from_bootstrap_unverified(
            network,
            a("admin"),
            ledger.clone(),
            BTreeSet::new()
        )
        .is_err());
        let mut chain = JointChainState::new_with_native_policy_unverified(
            network,
            policy.bootstrap,
            a("admin"),
            ledger,
            BTreeSet::new(),
            2,
            &policy,
        )
        .unwrap();
        let b = block(
            chain.tip(),
            80,
            vec![UnverifiedJointTx {
                txid: tx(80),
                actor: a("admin"),
                operation: JointOperation::Registry(RegistryAction::Activate {
                    types: vec![2, 11, 12],
                    code_hash: [1; 32],
                }),
            }],
            network,
        );
        chain.apply_unverified_block(&b).unwrap();
        let b = block(
            chain.tip(),
            81,
            vec![UnverifiedJointTx {
                txid: tx(81),
                actor: a("alice"),
                operation: JointOperation::Send {
                    id: p(2),
                    recipient: a("bob"),
                    amount_atoms: 110_000_000,
                },
            }],
            network,
        );
        chain.apply_unverified_block(&b).unwrap();
        assert_eq!(
            chain
                .state()
                .ledger()
                .native_remaining_atoms(p(2), &a("bob")),
            11 * U
        );
        let bytes = chain.checkpoint_bytes().unwrap();
        assert_eq!(&bytes[..5], b"TLJC\x03");
        assert!(JointChainState::from_checkpoint_bytes(
            &bytes,
            network,
            policy.bootstrap,
            &a("admin"),
            &BTreeSet::new(),
            2
        )
        .is_err());
        let mut restored = JointChainState::from_checkpoint_bytes_with_native_policy(
            &bytes,
            network,
            policy.bootstrap,
            &a("admin"),
            &BTreeSet::new(),
            2,
            &policy,
        )
        .unwrap();
        assert_eq!(restored, chain);
        restored.rollback_tip(restored.tip()).unwrap();
        assert_eq!(
            restored
                .state()
                .ledger()
                .native_remaining_atoms(p(2), &a("bob")),
            10 * U
        );
        let b = block(
            restored.tip(),
            82,
            vec![UnverifiedJointTx {
                txid: tx(82),
                actor: a("alice"),
                operation: JointOperation::Send {
                    id: p(2),
                    recipient: a("bob"),
                    amount_atoms: 2 * U,
                },
            }],
            network,
        );
        restored.apply_unverified_block(&b).unwrap();
        assert_eq!(
            restored
                .state()
                .ledger()
                .native_remaining_atoms(p(2), &a("bob")),
            12 * U
        );
        assert!(!restored.state().ledger().is_processed(tx(81)));
        let before = restored.clone();
        let b = block(
            restored.tip(),
            83,
            vec![UnverifiedJointTx {
                txid: tx(83),
                actor: a("funder"),
                operation: JointOperation::Grant {
                    id: p(1),
                    recipient: a("alice"),
                    amount_atoms: U,
                },
            }],
            network,
        );
        assert!(restored.apply_unverified_block(&b).is_err());
        assert_eq!(restored, before);
    }
}
#[test]
fn oldest_native_history_floor_roundtrips_without_fabricating_ancestry() {
    let network = TradeLayerMainnet::Bitcoin;
    let (mut ledger, policy) = fixture(network, NativeDriver::SourcePerCallV1);
    ledger
        .adopt_native_entitlements_unverified(&policy)
        .unwrap();
    let mut chain = JointChainState::new_with_native_policy_unverified(
        network,
        policy.bootstrap,
        a("admin"),
        ledger,
        BTreeSet::new(),
        1,
        &policy,
    )
    .unwrap();
    for id in 80..83 {
        chain
            .apply_unverified_block(&block(chain.tip(), id, vec![], network))
            .unwrap();
    }
    chain.rollback_tip(chain.tip()).unwrap();
    assert_eq!(chain.tip().height, 2);
    assert_eq!(chain.rollback_depth(), 0);
    let bytes = chain.checkpoint_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLJC\x04");
    let mut reopened = JointChainState::from_checkpoint_bytes_with_native_policy(
        &bytes,
        network,
        policy.bootstrap,
        &a("admin"),
        &BTreeSet::new(),
        1,
        &policy,
    )
    .unwrap();
    reopened
        .apply_unverified_block(&block(reopened.tip(), 90, vec![], network))
        .unwrap();
    assert_eq!(reopened.tip().height, 3);
}
