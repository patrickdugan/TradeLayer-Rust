#[allow(dead_code)]
#[path = "support/signed_channel_fixture.rs"]
mod support;
use support::*;
use tradelayer_core::channel_revision::{self, RevisionContext, RevisionStatus};
use tradelayer_core::execution::{ProtocolState, SignedChannelInput};
use tradelayer_core::ledger::{
    ChannelSide, ExpectedChannelRevisionPolicies, Ledger, LedgerError, Operation,
};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::unified_chain::UnifiedChainState;
fn networks() -> [TradeLayerMainnet; 3] {
    [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ]
}
fn context(network: TradeLayerMainnet, height: u64) -> RevisionContext {
    RevisionContext {
        network,
        parent: chain_block(network, height).header.into_tip_for_test(),
        height,
    }
}
trait TestTip {
    fn into_tip_for_test(self) -> tradelayer_core::chain::Tip;
}
impl TestTip for tradelayer_core::chain::BlockHeader {
    fn into_tip_for_test(self) -> tradelayer_core::chain::Tip {
        tradelayer_core::chain::Tip {
            height: self.height - 1,
            id: self.parent,
        }
    }
}
fn restore(owner: &UnifiedChainState) -> UnifiedChainState {
    UnifiedChainState::from_checkpoint_bytes_with_channel_policies(
        &owner.checkpoint_bytes().unwrap(),
        owner.chain_id(),
        owner.anchor(),
        &account("admin"),
        owner.history_limit(),
        owner.channel_revision_policies(),
    )
    .unwrap()
}
fn repaired(mut bytes: Vec<u8>) -> Vec<u8> {
    let n = bytes.len() - 4;
    let mut crc = !0u32;
    for b in &bytes[..n] {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ ((0u32.wrapping_sub(crc & 1)) & 0xedb88320);
        }
    }
    bytes[n..].copy_from_slice(&(!crc).to_be_bytes());
    bytes
}
fn replace_once(bytes: &mut [u8], from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len());
    let at = bytes.windows(from.len()).position(|p| p == from).unwrap();
    bytes[at..at + from.len()].copy_from_slice(to);
}
#[test]
fn actual_funded_three_network_lifecycle_and_successive_finalization() {
    for n in networks() {
        let mut owner = enrolled(n, 4);
        assert_eq!(
            &owner.state().ledger().snapshot_bytes().unwrap()[..4],
            b"TLL\x04"
        );
        let h0 = owner
            .state()
            .ledger()
            .channel_revision_head(&account("channel"))
            .unwrap();
        assert_eq!(h0.sequence, 0);
        assert!(owner
            .state()
            .ledger()
            .require_current_signed_channel_head(&account("channel"), h0)
            .is_err());
        let (_, first) = signed(n, "first");
        owner
            .apply_signed_channel_block_unverified(
                &chain_block(n, 4),
                &EmptySyntheticAdapter,
                &[SignedChannelInput::Revision(first)],
            )
            .unwrap();
        assert_eq!(owner.state().finalized_height(), Some(4));
        let h1 = owner
            .state()
            .ledger()
            .channel_revision_head(&account("channel"))
            .unwrap();
        assert_eq!(
            owner
                .state()
                .ledger()
                .channel_atoms(property(), &account("channel"), ChannelSide::A),
            350
        );
        assert_eq!(
            owner
                .state()
                .ledger()
                .channel_revision_status(&account("channel"), h0.revision_id),
            RevisionStatus::Superseded
        );
        owner
            .state()
            .ledger()
            .require_current_signed_channel_head(&account("channel"), h1)
            .unwrap();
        let (_, second) = signed(n, "second");
        owner
            .apply_signed_channel_block_unverified(
                &chain_block(n, 5),
                &EmptySyntheticAdapter,
                &[SignedChannelInput::Revision(second)],
            )
            .unwrap();
        assert_eq!(
            owner
                .state()
                .ledger()
                .channel_atoms(property(), &account("channel"), ChannelSide::B),
            999
        );
        assert!(owner
            .state()
            .ledger()
            .require_current_signed_channel_head(&account("channel"), h1)
            .is_err());
        assert_eq!(owner, restore(&owner));
        let tip = owner.tip();
        owner.rollback_tip(tip).unwrap();
        assert_eq!(
            owner
                .state()
                .ledger()
                .channel_revision_head(&account("channel")),
            Some(h1)
        );
        assert_eq!(owner, restore(&owner));
        owner.state().ledger().verify_conservation().unwrap();
    }
}
#[test]
fn second_signature_and_late_batch_failure_leave_all_owner_state_unchanged() {
    let n = TradeLayerMainnet::Bitcoin;
    let mut owner = enrolled(n, 4);
    let before = owner.clone();
    let (_, mut first) = signed(n, "first");
    first.signature_b[0] ^= 1;
    assert!(owner
        .apply_signed_channel_block_unverified(
            &chain_block(n, 4),
            &EmptySyntheticAdapter,
            &[SignedChannelInput::Revision(first)]
        )
        .is_err());
    assert_eq!(owner, before);
    let (_, first) = signed(n, "first");
    let (_, second) = signed(n, "second");
    assert!(owner
        .apply_signed_channel_block_unverified(
            &chain_block(n, 4),
            &EmptySyntheticAdapter,
            &[
                SignedChannelInput::Revision(first.clone()),
                SignedChannelInput::Revision(second)
            ]
        )
        .is_err());
    assert_eq!(owner, before);
    owner
        .apply_signed_channel_block_unverified(
            &chain_block(n, 4),
            &EmptySyntheticAdapter,
            &[SignedChannelInput::Revision(first.clone())],
        )
        .unwrap();
    let before = owner.clone();
    assert!(owner
        .apply_signed_channel_block_unverified(
            &chain_block(n, 5),
            &EmptySyntheticAdapter,
            &[SignedChannelInput::Revision(first)]
        )
        .is_err());
    assert_eq!(owner, before);
}
#[test]
fn legacy_five_mutators_and_both_transfer_ends_are_fenced() {
    let n = TradeLayerMainnet::Bitcoin;
    let owner = enrolled(n, 4);
    let mut l = owner.state().ledger().clone();
    let c = account("channel");
    let p = property();
    let operations = [
        Operation::ChannelCommit {
            id: p,
            owner: account("alice"),
            channel: c.clone(),
            side: ChannelSide::A,
            amount_atoms: 1,
        },
        Operation::ChannelWithdraw {
            id: p,
            owner: account("alice"),
            channel: c.clone(),
            side: ChannelSide::A,
            amount_atoms: 1,
            unlock_height: 10,
        },
        Operation::ChannelTransfer {
            id: p,
            from_channel: c.clone(),
            from_side: ChannelSide::A,
            to_channel: account("other"),
            to_side: ChannelSide::A,
            amount_atoms: 1,
        },
        Operation::ChannelTransfer {
            id: p,
            from_channel: account("other"),
            from_side: ChannelSide::A,
            to_channel: c.clone(),
            to_side: ChannelSide::A,
            amount_atoms: 1,
        },
        Operation::ChannelNetClaim {
            id: p,
            channel: c.clone(),
            payer_side: ChannelSide::A,
            amount_atoms: 1,
            trade_reference: id(90),
            height: 4,
            expiry_height: 8,
        },
        Operation::ChannelKingSettle {
            id: p,
            channel: c.clone(),
            start_height: 4,
            end_height: 4,
            settled_at: 4,
            expected_root: [0; 32],
            expected_count: 1,
            payer_side: ChannelSide::A,
            net_atoms: 1,
        },
    ];
    for (i, op) in operations.into_iter().enumerate() {
        let before = l.clone();
        assert!(matches!(
            l.apply(id(110 + i as u8), op),
            Err(LedgerError::SignedChannelFenced(_))
        ));
        assert_eq!(before, l);
    }
    assert_eq!(l.settle_due_withdrawals_unverified(100).unwrap(), 0);
}
#[test]
fn pending_withdrawal_or_claim_prevents_enrollment_and_does_not_consume_id() {
    let n = TradeLayerMainnet::Bitcoin;
    for withdrawal in [true, false] {
        let mut l = funded_protocol(n).ledger().clone();
        let op = if withdrawal {
            Operation::ChannelWithdraw {
                id: property(),
                owner: account("alice"),
                channel: account("channel"),
                side: ChannelSide::A,
                amount_atoms: 1,
                unlock_height: 10,
            }
        } else {
            Operation::ChannelNetClaim {
                id: property(),
                channel: account("channel"),
                payer_side: ChannelSide::A,
                amount_atoms: 1,
                trade_reference: id(91),
                height: 2,
                expiry_height: 10,
            }
        };
        l.apply(id(90), op).unwrap();
        let (p, _) = signed(n, "first");
        let tx = tradelayer_core::ledger::channel_enrollment_replay_id(&p, context(n, 3)).unwrap();
        let before = l.clone();
        assert!(l
            .enroll_channel_revision_unverified(
                tx,
                &p.channel,
                &ExpectedChannelRevisionPolicies::from([(p.channel.clone(), p.clone())]),
                context(n, 3)
            )
            .is_err());
        assert_eq!(before, l);
    }
}
#[test]
fn independent_policy_table_and_legacy_version_fences() {
    let n = TradeLayerMainnet::Bitcoin;
    let before = fixture(n, 4);
    let enabled = before.state().snapshot_bytes().unwrap();
    assert_eq!(&enabled[..5], b"TLPS\x03");
    assert!(
        ProtocolState::from_snapshot_bytes(&enabled, before.chain_id(), &account("admin")).is_err()
    );
    let ordinary = funded_protocol(n);
    let old = ordinary.snapshot_bytes().unwrap();
    assert!(ProtocolState::from_snapshot_bytes_with_channel_policies(
        &old,
        before.chain_id(),
        &account("admin"),
        &policies(n)
    )
    .is_err());
    assert_eq!(
        old,
        ProtocolState::from_snapshot_bytes(&old, before.chain_id(), &account("admin"))
            .unwrap()
            .snapshot_bytes()
            .unwrap()
    );
    let owner = enrolled(n, 4);
    let bytes = owner.state().ledger().snapshot_bytes().unwrap();
    assert!(Ledger::from_snapshot_bytes(&bytes).is_err());
    let mut wrong = policies(n);
    wrong.get_mut(&account("channel")).unwrap().funding_epoch += 1;
    assert!(Ledger::from_snapshot_bytes_with_channel_policies(&bytes, &wrong).is_err());
    assert_eq!(
        bytes,
        Ledger::from_snapshot_bytes_with_channel_policies(&bytes, &policies(n))
            .unwrap()
            .snapshot_bytes()
            .unwrap()
    );
    assert!(UnifiedChainState::from_checkpoint_bytes(
        &owner.checkpoint_bytes().unwrap(),
        owner.chain_id(),
        owner.anchor(),
        &account("admin"),
        4
    )
    .is_err());
}
#[test]
fn enrollment_identity_substitution_and_witness_tampering_rejected_on_restore() {
    let n = TradeLayerMainnet::Bitcoin;
    let owner = enrolled(n, 4);
    let mut l = owner.state().ledger().clone();
    l.apply(
        id(200),
        Operation::CreateProperty {
            id: tradelayer_core::ledger::PropertyId::try_from(9).unwrap(),
            issuer: account("alice"),
            kind: tradelayer_core::ledger::PropertyKind::Fixed,
            initial_atoms: 1,
        },
    )
    .unwrap();
    let (p, _) = signed(n, "first");
    let policy = channel_revision::canonical_policy_bytes(&p).unwrap();
    let bytes = l.snapshot_bytes().unwrap();
    let at = bytes
        .windows(policy.len())
        .position(|w| w == policy)
        .unwrap()
        + policy.len();
    for substitute in [id(1), id(4), id(200)] {
        let mut forged = bytes.clone();
        forged[at..at + 32].copy_from_slice(&substitute.as_bytes());
        assert!(
            Ledger::from_snapshot_bytes_with_channel_policies(&repaired(forged), &policies(n))
                .is_err()
        );
    }
    let mut forged = bytes.clone();
    let state_height = at + 32 + 8 + 32;
    forged[state_height..state_height + 8]
        .copy_from_slice(&(channel_revision::MAX_SAFE_HEIGHT + 1).to_be_bytes());
    assert!(
        Ledger::from_snapshot_bytes_with_channel_policies(&repaired(forged), &policies(n)).is_err()
    );
    let bootstrap = hex(include_str!("../fixtures/channel_owner_bootstrap.hex").trim());
    let mut changed = bootstrap.clone();
    changed[12] ^= 1;
    let changed = repaired(changed);
    let mut forged = bytes;
    replace_once(&mut forged, &bootstrap, &changed);
    assert!(
        Ledger::from_snapshot_bytes_with_channel_policies(&repaired(forged), &policies(n)).is_err()
    );
}
#[test]
fn signed_payload_tampering_and_funded_total_change_are_atomic() {
    let n = TradeLayerMainnet::Bitcoin;
    let mut owner = enrolled(n, 4);
    let (_, first) = signed(n, "first");
    for variant in 0..4 {
        let mut changed = first.clone();
        match variant {
            0 => changed.revision.allocation[0].atoms_a += 1,
            1 => changed.revision.sequence = 2,
            2 => changed.revision.parent.id.0[0] ^= 1,
            _ => changed.revision.valid_until_height = 3,
        }
        let before = owner.clone();
        assert!(owner
            .apply_signed_channel_block_unverified(
                &chain_block(n, 4),
                &EmptySyntheticAdapter,
                &[SignedChannelInput::Revision(changed)]
            )
            .is_err());
        assert_eq!(owner, before);
    }
    owner
        .apply_signed_channel_block_unverified(
            &chain_block(n, 4),
            &EmptySyntheticAdapter,
            &[SignedChannelInput::Revision(first.clone())],
        )
        .unwrap();
    let bytes = owner.state().ledger().snapshot_bytes().unwrap();
    let signed = channel_revision::canonical_signed_revision_bytes(&first).unwrap();
    let mut corrupt = signed.clone();
    let len = corrupt.len();
    corrupt[len - 1] ^= 1;
    let mut forged = bytes;
    replace_once(&mut forged, &signed, &corrupt);
    assert!(
        Ledger::from_snapshot_bytes_with_channel_policies(&repaired(forged), &policies(n)).is_err()
    );
}
#[test]
fn retained_history_rejects_valid_signed_branch_splice() {
    let n = TradeLayerMainnet::Bitcoin;
    let mut a = enrolled(n, 4);
    let (_, first) = signed(n, "first");
    a.apply_signed_channel_block_unverified(
        &chain_block(n, 4),
        &EmptySyntheticAdapter,
        &[SignedChannelInput::Revision(first)],
    )
    .unwrap();
    let mut b = enrolled(n, 4);
    let (_, alternate) = signed(n, "alternate");
    b.apply_signed_channel_block_unverified(
        &chain_block(n, 4),
        &EmptySyntheticAdapter,
        &[SignedChannelInput::Revision(alternate)],
    )
    .unwrap();
    let prior = b.state().snapshot_bytes().unwrap();
    b.apply_block(&chain_block(n, 5), &EmptySyntheticAdapter)
        .unwrap();
    let mut forged = b.checkpoint_bytes().unwrap();
    replace_once(&mut forged, &prior, &a.state().snapshot_bytes().unwrap());
    assert!(
        UnifiedChainState::from_checkpoint_bytes_with_channel_policies(
            &repaired(forged),
            b.chain_id(),
            b.anchor(),
            &account("admin"),
            4,
            &policies(n)
        )
        .is_err()
    );
}
#[test]
fn history_eviction_then_empty_floor_reopen_and_new_branch() {
    let n = TradeLayerMainnet::Litecoin;
    let mut owner = enrolled(n, 2);
    let (_, first) = signed(n, "first");
    owner
        .apply_signed_channel_block_unverified(
            &chain_block(n, 4),
            &EmptySyntheticAdapter,
            &[SignedChannelInput::Revision(first)],
        )
        .unwrap();
    let (_, second) = signed(n, "second");
    owner
        .apply_signed_channel_block_unverified(
            &chain_block(n, 5),
            &EmptySyntheticAdapter,
            &[SignedChannelInput::Revision(second)],
        )
        .unwrap();
    assert_eq!(owner.rollback_floor().height, 3);
    while owner.rollback_depth() > 0 {
        owner.rollback_tip(owner.tip()).unwrap();
    }
    assert_eq!(owner.tip().height, 3);
    assert_eq!(owner.rollback_floor(), owner.tip());
    owner = restore(&owner);
    let (_, alternate) = signed(n, "alternate");
    owner
        .apply_signed_channel_block_unverified(
            &chain_block(n, 4),
            &EmptySyntheticAdapter,
            &[SignedChannelInput::Revision(alternate)],
        )
        .unwrap();
    assert_eq!(
        owner
            .state()
            .ledger()
            .channel_atoms(property(), &account("channel"), ChannelSide::A),
        700
    );
    assert_eq!(owner, restore(&owner));
}
#[test]
fn all_snapshot_prefixes_and_tail_garbage_fail_closed() {
    let n = TradeLayerMainnet::Dogecoin;
    let owner = enrolled(n, 2);
    let bytes = owner.state().ledger().snapshot_bytes().unwrap();
    for cut in [0, 1, 4, 100, bytes.len() - 1] {
        assert!(
            Ledger::from_snapshot_bytes_with_channel_policies(&bytes[..cut], &policies(n)).is_err()
        );
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(Ledger::from_snapshot_bytes_with_channel_policies(&trailing, &policies(n)).is_err());
}
#[test]
fn retained_branch_parent_rejects_otherwise_valid_two_signature_context() {
    let n = TradeLayerMainnet::Bitcoin;
    let mut owner = enrolled(n, 4);
    let mut foreign = owner.state().clone();
    let (_, wrong) = signed(n, "wrong-parent");
    foreign
        .apply_signed_channel_inputs_unverified(
            wrong.revision.parent,
            4,
            &[SignedChannelInput::Revision(wrong)],
        )
        .unwrap();
    foreign.finalize_block_unverified(4).unwrap();
    let (_, first) = signed(n, "first");
    owner
        .apply_signed_channel_block_unverified(
            &chain_block(n, 4),
            &EmptySyntheticAdapter,
            &[SignedChannelInput::Revision(first)],
        )
        .unwrap();
    let mut forged = owner.checkpoint_bytes().unwrap();
    replace_once(
        &mut forged,
        &owner.state().snapshot_bytes().unwrap(),
        &foreign.snapshot_bytes().unwrap(),
    );
    assert!(
        UnifiedChainState::from_checkpoint_bytes_with_channel_policies(
            &repaired(forged),
            owner.chain_id(),
            owner.anchor(),
            &account("admin"),
            4,
            &policies(n)
        )
        .is_err()
    );
}
#[test]
fn unsupported_single_ledger_embeddings_refuse_revision_metadata() {
    let n = TradeLayerMainnet::Bitcoin;
    let owner = enrolled(n, 2);
    assert!(matches!(
        tradelayer_core::spot::SpotExchange::from_ledger_unverified(
            owner.state().ledger().clone(),
            std::collections::BTreeSet::from([property()])
        ),
        Err(tradelayer_core::spot::SpotError::SignedChannelUnsupported)
    ));
    assert!(matches!(
        tradelayer_core::iou_ledger::FundedIouLedger::from_ledger_unverified(
            n,
            tradelayer_core::iou_ledger::PayoutAtomPolicy::Exact,
            owner.state().ledger().clone(),
            tradelayer_core::iou::IouStore::new()
        ),
        Err(tradelayer_core::iou_ledger::FundedIouError::SignedChannelUnsupported)
    ));
}
#[test]
fn retained_history_rejects_retroactive_valid_revision_tail() {
    let n = TradeLayerMainnet::Bitcoin;
    let mut a = enrolled(n, 4);
    let (_, first) = signed(n, "first");
    a.apply_signed_channel_block_unverified(
        &chain_block(n, 4),
        &EmptySyntheticAdapter,
        &[SignedChannelInput::Revision(first)],
    )
    .unwrap();
    let old_prior = a.state().snapshot_bytes().unwrap();
    a.apply_block(&chain_block(n, 5), &EmptySyntheticAdapter)
        .unwrap();
    let mut b = enrolled(n, 4);
    b.apply_block(&chain_block(n, 4), &EmptySyntheticAdapter)
        .unwrap();
    let new_prior = b.state().snapshot_bytes().unwrap();
    let mut forged = a.checkpoint_bytes().unwrap();
    let at = forged
        .windows(old_prior.len())
        .position(|w| w == old_prior)
        .unwrap();
    forged.splice(at..at + old_prior.len(), new_prior.clone());
    forged[at - 4..at].copy_from_slice(&(new_prior.len() as u32).to_be_bytes());
    assert!(
        UnifiedChainState::from_checkpoint_bytes_with_channel_policies(
            &repaired(forged),
            a.chain_id(),
            a.anchor(),
            &account("admin"),
            4,
            &policies(n)
        )
        .is_err()
    );
}
