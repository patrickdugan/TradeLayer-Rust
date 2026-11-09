//! ZkVerifyV1 phase 1: type 36 `ZK Verify` and Registry `RegisterZkProgram` over the joint owner,
//! with the deterministic mock verifier. All actors and blocks are UNVERIFIED test fixtures.

use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::joint::{
    JointBlockItem, JointError, JointOperation, UnverifiedJointTx, UnverifiedJointZkTx,
};
use tradelayer_core::joint_chain::{
    JointChainError, JointChainState, UnverifiedJointZkBlock, ZkTxReport,
};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::{Registry, RegistryAction, RegistryError, RegistryTx};
use tradelayer_core::zk_nova::RegisteredNovaKey;
use tradelayer_core::zk_verify::*;

const NET: TradeLayerMainnet = TradeLayerMainnet::Bitcoin;

fn hex_decode(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).expect("hex"))
        .collect()
}
fn fixture_hex(name: &str) -> Vec<u8> {
    let path = format!("{}/fixtures/zk_nova/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    hex_decode(text.trim())
}
/// The registered key of the cubic test circuit (`z' = z^3 + z + 5`, arity 1, layout 2).
fn cubic_key() -> Vec<u8> {
    RegisteredNovaKey::from_vk_nogens(&fixture_hex("cubic_3steps.vk_nogens.hex"))
        .expect("cubic key")
        .encode()
}

fn p(n: u32) -> PropertyId {
    PropertyId::try_from(n).unwrap()
}
fn a(s: &str) -> AccountId {
    AccountId::try_from(s).unwrap()
}
fn tx(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn anchor() -> Tip {
    Tip {
        height: 0,
        id: NET.genesis(),
    }
}
fn ctx() -> ZkBindingContext {
    ZkBindingContext {
        genesis: NET.genesis().0,
        anchor: anchor().id.0,
    }
}
fn ledger() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            tx(1),
            Operation::CreateProperty {
                id: p(7),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 1000,
            },
        )
        .unwrap();
    ledger
}
fn fee() -> ZkFeePolicy {
    ZkFeePolicy {
        property: p(7),
        amount_atoms: 5,
        sink: a("treasury"),
    }
}
fn chain_with(policy: bool) -> JointChainState {
    let mut chain = JointChainState::new_unverified(
        NET,
        anchor(),
        a("admin"),
        ledger(),
        BTreeSet::from([p(7)]),
        4,
    )
    .unwrap();
    if policy {
        chain.enable_zk_verify_v1(fee()).unwrap();
    }
    chain
}
fn block(
    chain: &JointChainState,
    marker: u8,
    items: Vec<JointBlockItem>,
) -> UnverifiedJointZkBlock {
    UnverifiedJointZkBlock {
        network: NET,
        header: BlockHeader {
            height: chain.tip().height + 1,
            id: BlockId([marker; 32]),
            parent: chain.tip().id,
        },
        items,
    }
}
fn admin_tx(n: u8, action: RegistryAction) -> JointBlockItem {
    JointBlockItem::Tx(UnverifiedJointTx {
        txid: tx(n),
        actor: a("admin"),
        operation: JointOperation::Registry(action),
    })
}
fn register_action(layout: u32, ms: u32, key: &[u8]) -> RegistryAction {
    RegistryAction::RegisterZkProgram {
        system: ZkSystem::Nova,
        program_id: program_id_of(
            ZkSystem::Nova,
            key,
            layout,
            ZkEffect::StatementReceipt,
            1,
            ms,
        ),
        key_bytes: key.to_vec(),
        public_input_layout: layout,
        effect: ZkEffect::StatementReceipt,
        verifier_version: 1,
        max_verify_ms: ms,
    }
}
fn program(layout: u32, ms: u32) -> [u8; 32] {
    program_id_of(
        ZkSystem::Nova,
        &cubic_key(),
        layout,
        ZkEffect::StatementReceipt,
        1,
        ms,
    )
}
fn good_proof() -> Vec<u8> {
    let mut proof = 3u32.to_be_bytes().to_vec();
    proof.extend_from_slice(MOCK_PROOF_MAGIC);
    proof
}
fn payload_for(sender: &str, program_id: [u8; 32], expiry: u64) -> ZkVerifyPayload {
    ZkVerifyPayload {
        system: ZkSystem::Nova.code(),
        program_id,
        public_inputs: vec![
            binding_element(&ctx(), &a(sender), expiry, &program_id),
            [9; 32],
        ],
        proof: good_proof(),
        expiry,
    }
}
fn zk(n: u8, sender: &str, payload: ZkVerifyPayload) -> JointBlockItem {
    JointBlockItem::Zk(UnverifiedJointZkTx {
        txid: tx(n),
        actor: a(sender),
        payload,
    })
}
fn activate_36() -> JointBlockItem {
    admin_tx(
        10,
        RegistryAction::Activate {
            types: vec![36],
            code_hash: [7; 32],
        },
    )
}
/// Policy on, type 36 activated at height 1, program registered at height 2; tip is height 2.
fn ready(layout: u32, ms: u32) -> JointChainState {
    let mut chain = chain_with(true);
    let verifier = MockZkVerifier::new();
    let b1 = block(&chain, 1, vec![activate_36()]);
    chain.apply_unverified_zk_block(&b1, &verifier).unwrap();
    let b2 = block(
        &chain,
        2,
        vec![admin_tx(11, register_action(layout, ms, &cubic_key()))],
    );
    chain.apply_unverified_zk_block(&b2, &verifier).unwrap();
    chain
}
fn run(
    chain: &mut JointChainState,
    marker: u8,
    items: Vec<JointBlockItem>,
    verifier: &MockZkVerifier,
) -> Result<Vec<ZkTxReport>, JointChainError> {
    let b = block(chain, marker, items);
    chain.apply_unverified_zk_block(&b, verifier)
}
fn balance(chain: &JointChainState, who: &str) -> u128 {
    chain.state().ledger().available_atoms(p(7), &a(who))
}
fn registry_err(result: Result<Vec<ZkTxReport>, JointChainError>) -> RegistryError {
    match result {
        Err(JointChainError::Joint(JointError::Registry(e))) => e,
        other => panic!("expected registry error, got {other:?}"),
    }
}

#[test]
fn registration_is_admin_only_immutable_and_rejects_system_one() {
    let mut chain = chain_with(true);
    let v = MockZkVerifier::new();
    run(&mut chain, 1, vec![activate_36()], &v).unwrap();
    let reg = || register_action(2, 100, &cubic_key());
    // Not admin.
    let outsider = JointBlockItem::Tx(UnverifiedJointTx {
        txid: tx(11),
        actor: a("alice"),
        operation: JointOperation::Registry(reg()),
    });
    assert_eq!(
        registry_err(run(&mut chain, 2, vec![outsider], &v)),
        RegistryError::Unauthorized
    );
    // System 1 (STARK) is reserved.
    let stark = RegistryAction::RegisterZkProgram {
        system: ZkSystem::Stark,
        program_id: program_id_of(ZkSystem::Stark, b"k", 2, ZkEffect::StatementReceipt, 1, 100),
        key_bytes: b"k".to_vec(),
        public_input_layout: 2,
        effect: ZkEffect::StatementReceipt,
        verifier_version: 1,
        max_verify_ms: 100,
    };
    assert_eq!(
        registry_err(run(&mut chain, 2, vec![admin_tx(12, stark)], &v)),
        RegistryError::ZkSystemReserved
    );
    // program_id must be the hash of the other fields.
    let RegistryAction::RegisterZkProgram {
        system,
        key_bytes,
        public_input_layout,
        effect,
        verifier_version,
        max_verify_ms,
        ..
    } = reg()
    else {
        unreachable!()
    };
    let forged = RegistryAction::RegisterZkProgram {
        system,
        program_id: [1; 32],
        key_bytes,
        public_input_layout,
        effect,
        verifier_version,
        max_verify_ms,
    };
    assert_eq!(
        registry_err(run(&mut chain, 2, vec![admin_tx(13, forged)], &v)),
        RegistryError::ZkProgramIdMismatch
    );
    // Layout 0 has no room for the binding element.
    assert!(matches!(
        registry_err(run(
            &mut chain,
            2,
            vec![admin_tx(14, register_action(0, 100, &cubic_key()))],
            &v
        )),
        RegistryError::InvalidField(_)
    ));
    // Registered once, then immutable: the duplicate is refused and the entry is unchanged.
    run(&mut chain, 2, vec![admin_tx(15, reg())], &v).unwrap();
    let id = program(2, 100);
    let before = chain.state().registry().zk_program(&id).unwrap().clone();
    assert_eq!(
        registry_err(run(&mut chain, 3, vec![admin_tx(16, reg())], &v)),
        RegistryError::DuplicateZkProgram
    );
    assert_eq!(chain.state().registry().zk_program(&id), Some(&before));
    assert_eq!(chain.state().registry().zk_program_count(), 1);
}

#[test]
fn type_36_is_inert_until_activation_and_off_by_default() {
    // Default registry: type 36 is not activatable and RegisterZkProgram is disabled.
    let mut registry = Registry::new(a("admin"));
    let activate = RegistryTx {
        txid: tx(1),
        actor: a("admin"),
        height: 1,
        action: RegistryAction::Activate {
            types: vec![36],
            code_hash: [0; 32],
        },
    };
    assert_eq!(
        registry.apply(activate),
        Err(RegistryError::InvalidType(36))
    );
    assert_eq!(registry.activation(36), None);
    let reg = RegistryTx {
        txid: tx(2),
        actor: a("admin"),
        height: 2,
        action: register_action(2, 100, &cubic_key()),
    };
    assert_eq!(registry.apply(reg), Err(RegistryError::ZkVerifyDisabled));

    // Policy on but not yet activated: rejected as not active, no fee.
    let v = MockZkVerifier::new();
    let mut chain = chain_with(true);
    let item = zk(20, "alice", payload_for("alice", program(2, 100), 50));
    assert_eq!(
        registry_err(run(&mut chain, 1, vec![item.clone()], &v)),
        RegistryError::NotActive(36)
    );
    assert_eq!(balance(&chain, "alice"), 1000);
    // Activation and use in the same block is refused.
    let same_block = run(&mut chain, 1, vec![activate_36(), item], &v);
    assert_eq!(
        registry_err(same_block),
        RegistryError::SameBlockActivation(36)
    );
    // Policy off entirely.
    let mut off = chain_with(false);
    assert_eq!(
        registry_err(run(
            &mut off,
            1,
            vec![zk(21, "alice", payload_for("alice", [0; 32], 5))],
            &v
        )),
        RegistryError::ZkVerifyDisabled
    );
    assert_eq!(v.calls(), 0);
}

#[test]
fn verified_proof_records_receipt_charges_fee_and_is_queryable() {
    let mut chain = ready(2, 100);
    let v = MockZkVerifier::new();
    let id = program(2, 100);
    let payload = payload_for("alice", id, 50);
    let hash = public_inputs_hash(&payload.public_inputs);
    let reports = run(&mut chain, 3, vec![zk(20, "alice", payload)], &v).unwrap();
    assert_eq!(v.calls(), 1);
    let ZkOutcome::Verified(receipt) = &reports[0].outcome else {
        panic!("{reports:?}")
    };
    assert_eq!(
        receipt,
        &StatementReceipt {
            program_id: id,
            public_inputs_hash: hash,
            sender: a("alice"),
            height: 3,
            txid: tx(20),
        }
    );
    let registry = chain.state().registry();
    assert_eq!(registry.zk_receipts(), std::slice::from_ref(receipt));
    assert_eq!(registry.zk_receipts_for_program(&id).count(), 1);
    assert_eq!(registry.zk_receipts_for_sender(&a("alice")).count(), 1);
    assert_eq!(registry.zk_receipts_for_sender(&a("bob")).count(), 0);
    // The fee moved; nothing else did.
    assert_eq!(balance(&chain, "alice"), 995);
    assert_eq!(balance(&chain, "treasury"), 5);
    // Replay of the same txid is invalid and leaves the state untouched.
    let before = chain.clone();
    let again = payload_for("alice", id, 50);
    assert!(run(&mut chain, 4, vec![zk(20, "alice", again)], &v).is_err());
    assert_eq!(chain, before);
}

#[test]
fn rejected_proofs_keep_the_fee_write_no_receipt_and_do_not_reach_the_verifier() {
    let mut chain = ready(2, 100);
    let v = MockZkVerifier::new();
    let id = program(2, 100);

    let mut oversize = payload_for("alice", id, 50);
    oversize.proof.resize(MAX_ZK_TX_PROOF_BYTES + 1, 0);
    let mut bad_binding = payload_for("alice", id, 50);
    bad_binding.public_inputs[0][0] ^= 1;
    // Binding computed for another sender: a copied proof is useless to a third party.
    let stolen = payload_for("bob", id, 50);
    let expired = payload_for("alice", id, 2);
    let unknown = payload_for("alice", [0xee; 32], 50);
    let mut wrong_count = payload_for("alice", id, 50);
    wrong_count.public_inputs.push([1; 32]);
    let mut wrong_system = payload_for("alice", id, 50);
    wrong_system.system = ZkSystem::Stark.code();
    let mut no_envelope = payload_for("alice", id, 50);
    no_envelope.proof = vec![0, 0];

    let cases = [
        (oversize, ZkRejectReason::OversizeProof),
        (bad_binding, ZkRejectReason::BindingMismatch),
        (stolen, ZkRejectReason::BindingMismatch),
        (expired, ZkRejectReason::Expired),
        (unknown, ZkRejectReason::UnknownProgram),
        (wrong_count, ZkRejectReason::PublicInputCount),
        (wrong_system, ZkRejectReason::SystemMismatch),
        (no_envelope, ZkRejectReason::ProofEnvelope),
    ];
    let items: Vec<_> = cases
        .iter()
        .enumerate()
        .map(|(i, (payload, _))| zk(30 + i as u8, "alice", payload.clone()))
        .collect();
    let reports = run(&mut chain, 3, items, &v).unwrap();
    for (report, (_, reason)) in reports.iter().zip(&cases) {
        assert_eq!(report.outcome, ZkOutcome::Rejected(*reason));
    }
    assert_eq!(v.calls(), 0);
    assert_eq!(balance(&chain, "alice"), 1000 - 5 * cases.len() as u128);
    assert!(chain.state().registry().zk_receipts().is_empty());

    // A proof the verifier refuses also keeps its fee.
    let mut invalid = payload_for("alice", id, 50);
    invalid.proof = 3u32.to_be_bytes().to_vec();
    invalid.proof.extend_from_slice(b"garbage");
    let reports = run(&mut chain, 4, vec![zk(50, "alice", invalid)], &v).unwrap();
    assert_eq!(
        reports[0].outcome,
        ZkOutcome::Rejected(ZkRejectReason::Verifier(ZkReject::InvalidProof))
    );
    assert_eq!(v.calls(), 1);
    assert_eq!(
        balance(&chain, "alice"),
        1000 - 5 * (cases.len() as u128 + 1)
    );
    // A rejected transaction is consumed: replaying its txid is invalid.
    assert!(run(
        &mut chain,
        5,
        vec![zk(50, "alice", payload_for("alice", id, 50))],
        &v
    )
    .is_err());
}

#[test]
fn unpaid_fee_makes_the_transaction_invalid() {
    let mut chain = ready(2, 100);
    let v = MockZkVerifier::new();
    let before = chain.clone();
    let result = run(
        &mut chain,
        3,
        vec![zk(20, "pauper", payload_for("pauper", program(2, 100), 50))],
        &v,
    );
    assert!(matches!(
        result,
        Err(JointChainError::Joint(JointError::ZkFee(_)))
    ));
    assert_eq!(chain, before);
    assert_eq!(v.calls(), 0);
}

#[test]
fn over_budget_transactions_are_rejected_in_block_order() {
    // Count cap: the first MAX verify, the rest are rejected, in order.
    let mut chain = ready(2, 100);
    let v = MockZkVerifier::new();
    let id = program(2, 100);
    let total = MAX_ZK_VERIFY_PER_BLOCK + 2;
    let items: Vec<_> = (0..total)
        .map(|i| {
            let mut payload = payload_for("alice", id, 50);
            payload.public_inputs[1] = [i as u8; 32];
            zk(40 + i as u8, "alice", payload)
        })
        .collect();
    let reports = run(&mut chain, 3, items, &v).unwrap();
    for (i, report) in reports.iter().enumerate() {
        if i < MAX_ZK_VERIFY_PER_BLOCK {
            assert!(matches!(report.outcome, ZkOutcome::Verified(_)), "{i}");
        } else {
            assert_eq!(
                report.outcome,
                ZkOutcome::Rejected(ZkRejectReason::BudgetCount)
            );
        }
    }
    assert_eq!(v.calls(), MAX_ZK_VERIFY_PER_BLOCK);
    assert_eq!(
        chain.state().registry().zk_receipts().len(),
        MAX_ZK_VERIFY_PER_BLOCK
    );
    assert_eq!(
        balance(&chain, "alice"),
        1000 - 5 * total as u128,
        "fee kept"
    );

    // Summed max_verify_ms cap: 1500 ms programs, two fit under 4000, the third does not.
    let mut chain = ready(2, 1500);
    let id = program(2, 1500);
    let items: Vec<_> = (0..3)
        .map(|i| {
            let mut payload = payload_for("alice", id, 50);
            payload.public_inputs[1] = [i; 32];
            zk(60 + i, "alice", payload)
        })
        .collect();
    let reports = run(&mut chain, 3, items, &v).unwrap();
    assert!(matches!(reports[1].outcome, ZkOutcome::Verified(_)));
    assert_eq!(
        reports[2].outcome,
        ZkOutcome::Rejected(ZkRejectReason::BudgetVerifyMs)
    );

    // Proof byte cap: five 60 KiB proofs exceed 256 KiB, so the fifth is over budget.
    let mut chain = ready(2, 10);
    let id = program(2, 10);
    let items: Vec<_> = (0..5)
        .map(|i| {
            let mut payload = payload_for("alice", id, 50);
            payload.public_inputs[1] = [i; 32];
            payload.proof.resize(60 * 1024, 0);
            zk(80 + i, "alice", payload)
        })
        .collect();
    let reports = run(&mut chain, 3, items, &v).unwrap();
    // The padded proof fails the mock's exact-magic check, but it is charged first.
    assert_eq!(
        reports[4].outcome,
        ZkOutcome::Rejected(ZkRejectReason::BudgetProofBytes)
    );
    assert!(matches!(
        reports[3].outcome,
        ZkOutcome::Rejected(ZkRejectReason::Verifier(_))
    ));
}

#[test]
fn receipts_survive_checkpoint_and_rollback() {
    let mut chain = ready(2, 100);
    let v = MockZkVerifier::new();
    let id = program(2, 100);
    let tip_before = chain.tip();
    let balance_before = balance(&chain, "alice");
    run(
        &mut chain,
        3,
        vec![zk(20, "alice", payload_for("alice", id, 50))],
        &v,
    )
    .unwrap();
    let tip_after = chain.tip();
    assert_eq!(chain.state().registry().zk_receipts().len(), 1);

    let bytes = chain.checkpoint_bytes().unwrap();
    let restored = JointChainState::from_checkpoint_bytes(
        &bytes,
        NET,
        anchor(),
        &a("admin"),
        &BTreeSet::from([p(7)]),
        4,
    )
    .unwrap();
    assert_eq!(restored, chain);
    assert_eq!(
        restored.state().registry().zk_receipts(),
        chain.state().registry().zk_receipts()
    );
    assert_eq!(restored.checkpoint_bytes().unwrap(), bytes);

    // Reorg: the receipt, the fee and the replay marker all go with the block.
    chain.rollback_tip(tip_after).unwrap();
    assert_eq!(chain.tip(), tip_before);
    assert!(chain.state().registry().zk_receipts().is_empty());
    assert_eq!(balance(&chain, "alice"), balance_before);
    // The same txid can be used again on the new branch.
    run(
        &mut chain,
        9,
        vec![zk(20, "alice", payload_for("alice", id, 50))],
        &v,
    )
    .unwrap();
    assert_eq!(chain.state().registry().zk_receipts().len(), 1);
}

#[test]
fn registry_snapshot_v1_is_unchanged_and_v2_round_trips() {
    // Policy off: the v1 encoding, and it decodes with the policy off.
    let plain = Registry::new(a("admin"));
    let v1 = plain.snapshot_bytes().unwrap();
    assert_eq!(&v1[..5], b"TLRG\x01");
    let back = Registry::from_snapshot_bytes(&v1, &a("admin")).unwrap();
    assert_eq!(back, plain);
    assert!(back.zk_policy().is_none());

    // Policy on: v2.
    let on = Registry::new_zk_verify_v1(a("admin"), fee());
    let v2 = on.snapshot_bytes().unwrap();
    assert_eq!(&v2[..5], b"TLRG\x02");
    assert_eq!(Registry::from_snapshot_bytes(&v2, &a("admin")).unwrap(), on);
    // A v2 body under the v1 magic is refused.
    let mut forged = v2.clone();
    forged[4] = 1;
    assert!(Registry::from_snapshot_bytes(&forged, &a("admin")).is_err());

    // Populated: activation, program and a receipt.
    let mut chain = ready(2, 100);
    let v = MockZkVerifier::new();
    run(
        &mut chain,
        3,
        vec![zk(20, "alice", payload_for("alice", program(2, 100), 50))],
        &v,
    )
    .unwrap();
    let bytes = chain.state().registry().snapshot_bytes().unwrap();
    let restored = Registry::from_snapshot_bytes(&bytes, &a("admin")).unwrap();
    assert_eq!(&restored, chain.state().registry());
    assert!(restored.zk_program(&program(2, 100)).is_some());
    assert_eq!(restored.activation(36).unwrap().height, 1);
    assert_eq!(restored.zk_receipts().len(), 1);
}

#[test]
fn frame_codec_round_trips_and_rejects_malformed_input() {
    let payload = payload_for("alice", program(2, 100), 77);
    let frame = payload.encode_frame();
    assert_eq!(&frame[..3], b"tl\x24");
    assert_eq!(ZkVerifyPayload::decode_frame(&frame).unwrap(), payload);
    // The legacy ASCII parser keeps rejecting it.
    assert!(tradelayer_core::wire::parse_frame(&String::from_utf8_lossy(&frame)).is_err());
    assert_eq!(
        ZkVerifyPayload::decode_frame(&frame[..frame.len() - 1]),
        Err(ZkWireError::Truncated)
    );
    let mut trailing = frame.clone();
    trailing.push(0);
    assert_eq!(
        ZkVerifyPayload::decode_frame(&trailing),
        Err(ZkWireError::TrailingBytes)
    );
    assert_eq!(
        ZkVerifyPayload::decode_frame(b"tl2abc"),
        Err(ZkWireError::NotZkFrame)
    );
    let mut many = frame.clone();
    many[44..48].copy_from_slice(&1000u32.to_be_bytes());
    assert_eq!(
        ZkVerifyPayload::decode_frame(&many),
        Err(ZkWireError::TooManyInputs)
    );
    // The Nova adapter refuses a key that is not a RegisteredNovaKey before any proof work.
    assert_eq!(
        ZkNovaAdapter.verify(ZkSystem::Nova, b"k", 1, &[[0; 32]], b"p"),
        Err(ZkReject::MalformedKey)
    );
}

#[test]
fn registration_requires_a_nova_key_whose_arity_matches_the_layout() {
    let key = cubic_key();
    assert_eq!(check_nova_key(&key, 2), Ok(()));
    assert_eq!(check_nova_key(&key, 1), Err(ZkReject::MalformedKey));
    assert_eq!(check_nova_key(&key, 3), Err(ZkReject::MalformedKey));
    assert_eq!(check_nova_key(b"key-bytes", 2), Err(ZkReject::MalformedKey));
    let mut truncated = key.clone();
    truncated.pop();
    assert_eq!(check_nova_key(&truncated, 2), Err(ZkReject::MalformedKey));

    let mut chain = chain_with(true);
    let verifier = MockZkVerifier::new();
    let b1 = block(&chain, 1, vec![activate_36()]);
    chain.apply_unverified_zk_block(&b1, &verifier).unwrap();
    for (n, (layout, key)) in [(3u32, key.clone()), (2, b"key-bytes".to_vec())]
        .into_iter()
        .enumerate()
    {
        let b = block(
            &chain,
            2 + n as u8,
            vec![admin_tx(30 + n as u8, register_action(layout, 100, &key))],
        );
        assert!(matches!(
            registry_err(chain.apply_unverified_zk_block(&b, &verifier)),
            RegistryError::InvalidField("zk nova key")
        ));
    }
    assert_eq!(chain.state().registry().zk_program_count(), 0);
}

/// A real nova-snark proof bound to sender `alice`: `z0` is the binding element of
/// (genesis, anchor, alice, expiry 50, cubic program with max_verify_ms 1000), proved off-chain by
/// the reference prover (`fixtures/zk_nova/cubic_bound_alice.*`).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only (debug derives 2^17 generators slowly): cargo test --release --test zk_verify"
)]
fn bound_real_proof_records_a_receipt_and_cannot_be_reused_by_another_sender() {
    let id = program(2, 1000);
    let binding = binding_element(&ctx(), &a("alice"), 50, &id);
    let meta = std::fs::read_to_string(format!(
        "{}/fixtures/zk_nova/cubic_bound_alice.meta.tsv",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_default();
    let field = |key: &str| {
        meta.lines()
            .find_map(|l| l.strip_prefix(&format!("{key}\t")))
            .map(hex_decode)
    };
    let z0 = field("z0").unwrap_or_default();
    assert_eq!(
        z0,
        binding.to_vec(),
        "fixture z0 must be the binding element {}",
        binding
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let zn: [u8; 32] = field("zn").expect("zn").try_into().expect("32 bytes");
    let mut proof = 3u32.to_be_bytes().to_vec();
    proof.extend_from_slice(&fixture_hex("cubic_bound_alice.proof.hex"));
    let payload = |first: [u8; 32]| ZkVerifyPayload {
        system: ZkSystem::Nova.code(),
        program_id: id,
        public_inputs: vec![first, zn],
        proof: proof.clone(),
        expiry: 50,
    };

    let mut chain = ready(2, 1000);
    // Activate Send (type 2), then fund bob so his two attempts can pay the fee.
    let activate_send = admin_tx(
        41,
        RegistryAction::Activate {
            types: vec![2],
            code_hash: [7; 32],
        },
    );
    run(&mut chain, 3, vec![activate_send], &MockZkVerifier::new()).unwrap();
    let send = JointBlockItem::Tx(UnverifiedJointTx {
        txid: tx(40),
        actor: a("alice"),
        operation: JointOperation::Send {
            id: p(7),
            recipient: a("bob"),
            amount_atoms: 100,
        },
    });
    run(&mut chain, 4, vec![send], &MockZkVerifier::new()).unwrap();

    let bob_binding = binding_element(&ctx(), &a("bob"), 50, &id);
    let b5 = block(
        &chain,
        5,
        vec![
            zk(20, "alice", payload(binding)),
            // Bob copies the transaction unchanged: his binding differs; refused before verifying.
            zk(21, "bob", payload(binding)),
            // Bob substitutes his own binding element: the proof does not open to it.
            zk(22, "bob", payload(bob_binding)),
        ],
    );
    let reports = chain
        .apply_unverified_zk_block(&b5, &ZkNovaAdapter)
        .unwrap();
    let outcomes: Vec<&ZkOutcome> = reports.iter().map(|r| &r.outcome).collect();
    assert_eq!(
        outcomes,
        [
            &ZkOutcome::Verified(StatementReceipt {
                program_id: id,
                public_inputs_hash: public_inputs_hash(&[binding, zn]),
                sender: a("alice"),
                height: 5,
                txid: tx(20),
            }),
            &ZkOutcome::Rejected(ZkRejectReason::BindingMismatch),
            &ZkOutcome::Rejected(ZkRejectReason::Verifier(ZkReject::InvalidProof)),
        ]
    );
    let registry = chain.state().registry();
    assert_eq!(registry.zk_receipts().len(), 1);
    assert_eq!(registry.zk_receipts_for_sender(&a("bob")).count(), 0);
    // Every attempt paid the fee.
    assert_eq!(balance(&chain, "alice"), 1000 - 100 - 5);
    assert_eq!(balance(&chain, "bob"), 100 - 10);
    assert_eq!(balance(&chain, "treasury"), 15);
}

/// Real nova-snark proofs through the adapter: the cubic fixture proves `2 -> zn` in 3 steps.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only (debug derives 2^17 generators slowly): cargo test --release --test zk_verify"
)]
fn nova_adapter_verifies_real_proofs_against_z0_and_zn() {
    let key = cubic_key();
    let proof = fixture_hex("cubic_3steps.proof.hex");
    let mut z0 = [0u8; 32];
    z0[0] = 2;
    let mut zn = [0u8; 32];
    zn[..5].copy_from_slice(&[0x23, 0x83, 0x61, 0x1c, 0x09]);
    let inputs = [z0, zn];
    let verified = ZkNovaAdapter.verify(ZkSystem::Nova, &key, 3, &inputs, &proof);
    assert_eq!(
        verified,
        Ok(VerifiedStatement {
            public_inputs_hash: public_inputs_hash(&inputs)
        })
    );
    // A claimed zn the proof does not reach, a wrong step count, and a corrupted proof.
    let mut wrong_zn = zn;
    wrong_zn[0] ^= 1;
    assert_eq!(
        ZkNovaAdapter.verify(ZkSystem::Nova, &key, 3, &[z0, wrong_zn], &proof),
        Err(ZkReject::InvalidProof)
    );
    assert_eq!(
        ZkNovaAdapter.verify(ZkSystem::Nova, &key, 4, &inputs, &proof),
        Err(ZkReject::InvalidProof)
    );
    let corrupt = fixture_hex("cubic_3steps.proof_corrupt0.hex");
    assert_eq!(
        ZkNovaAdapter.verify(ZkSystem::Nova, &key, 3, &inputs, &corrupt),
        Err(ZkReject::InvalidProof)
    );
    // Layout and encoding errors are refused before verification.
    assert_eq!(
        ZkNovaAdapter.verify(ZkSystem::Nova, &key, 3, &[z0], &proof),
        Err(ZkReject::MalformedProof)
    );
    assert_eq!(
        ZkNovaAdapter.verify(ZkSystem::Nova, &key, 3, &[[0xff; 32], zn], &proof),
        Err(ZkReject::MalformedProof)
    );
    assert_eq!(
        ZkNovaAdapter.verify(ZkSystem::Nova, &key, 3, &inputs, &proof[..100]),
        Err(ZkReject::MalformedProof)
    );
    assert_eq!(
        ZkNovaAdapter.verify(ZkSystem::Stark, &key, 3, &inputs, &proof),
        Err(ZkReject::UnsupportedSystem)
    );
}

#[test]
fn route_dispatch_validates_receipt_queries() {
    use std::collections::BTreeMap;
    use tradelayer_core::client::RpcValue;
    use tradelayer_core::service::*;
    use tradelayer_core::service_routes::dispatch;
    #[derive(Default)]
    struct Backend(Vec<ServiceCall>);
    impl ServiceBackend for Backend {
        fn call(&mut self, call: ServiceCall) -> Result<RpcValue, ServiceError> {
            self.0.push(call);
            Ok(RpcValue::parse("[]").unwrap())
        }
    }
    let req = |body: &str| {
        ServiceRequest::new(
            ServiceMethod::Post,
            "/tl_getZkReceipts",
            BTreeMap::new(),
            RpcValue::parse(body).unwrap(),
        )
        .unwrap()
    };
    let mut backend = Backend::default();
    let program = "AB".repeat(32);
    dispatch(
        &req(&format!("{{\"programId\":\"{program}\"}}")),
        &mut backend,
    )
    .unwrap();
    dispatch(&req("{\"address\":\"alice\"}"), &mut backend).unwrap();
    assert!(backend.0.iter().all(|c| c.target == "registry.zk_receipts"));
    assert_eq!(
        backend.0[0].arguments[0].as_string().unwrap(),
        "ab".repeat(32)
    );
    for bad in ["{}", "{\"programId\":\"abc\"}", "{\"programId\":\"zz\"}"] {
        assert_eq!(
            dispatch(&req(bad), &mut backend),
            Err(ServiceError::InvalidRequest),
            "{bad}"
        );
    }
}
