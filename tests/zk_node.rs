//! `ZkVerifyV1` on the node path: type 36, `RegisterZkProgram` and the type-36 activation reach the
//! economic owner as source rows (payload `~zk:` plus the frame in hex). The proof verifier is the
//! deterministic mock; the real-proof case lives in `tests/node_ingest.rs`.
//! All blocks, senders and rows are UNVERIFIED test fixtures.

use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::clearlist_source_owner::ClearlistSourceOwner;
use tradelayer_core::client::RpcValue;
use tradelayer_core::consensus_source::{
    SourceBlockMode, SourceConsensusConfig, SourceConsensusPolicy, SourceDisposition,
    SourceTransaction, UnverifiedSourceBlock,
};
use tradelayer_core::economic::{EconomicConfig, EconomicError, EconomicState, SettlementPolicy};
use tradelayer_core::economic_chain::{DurableEconomicChain, EconomicChain, EconomicChainConfig};
use tradelayer_core::joint::JointProtocolState;
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::property_catalog::PropertyCatalog;
use tradelayer_core::service::{ServiceBackend, ServiceCall};
use tradelayer_core::service_view::EconomicReadBackend;
use tradelayer_core::source_catalogs::{
    bootstrap_commitment as catalog_commitment, SourceCatalogOwners,
};
use tradelayer_core::storage::Recovery;
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::validity_source::SourceValue as V;
use tradelayer_core::zk_nova::RegisteredNovaKey;
use tradelayer_core::zk_verify::*;

const NET: TradeLayerMainnet = TradeLayerMainnet::Bitcoin;

fn a(s: &str) -> AccountId {
    AccountId::try_from(s).unwrap()
}
fn p7() -> PropertyId {
    PropertyId::try_from(7).unwrap()
}
fn txid(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn hex_decode(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).expect("hex"))
        .collect()
}
fn cubic_key() -> Vec<u8> {
    let path = format!(
        "{}/fixtures/zk_nova/cubic_3steps.vk_nogens.hex",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    RegisteredNovaKey::from_vk_nogens(&hex_decode(text.trim()))
        .unwrap()
        .encode()
}
fn fee() -> ZkFeePolicy {
    ZkFeePolicy {
        property: p7(),
        amount_atoms: 5,
        sink: a("treasury"),
    }
}
fn anchor() -> Tip {
    EconomicConfig::genesis_anchor(NET)
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
            txid(200),
            Operation::CreateProperty {
                id: p7(),
                issuer: a("admin"),
                kind: PropertyKind::Fixed,
                initial_atoms: 10_000,
            },
        )
        .unwrap();
    for (n, who, atoms) in [(201, "alice", 12), (202, "bob", 100)] {
        ledger
            .apply(
                txid(n),
                Operation::Transfer {
                    id: p7(),
                    from: a("admin"),
                    to: a(who),
                    amount_atoms: atoms,
                },
            )
            .unwrap();
    }
    ledger
}

/// The economic chain a node builds. `catalogs` selects the TLEC 0a (node) variant over TLEC 07.
fn chain(zk: bool, catalogs: bool, rollback_limit: usize) -> EconomicChain {
    let permitted = BTreeSet::from([p7()]);
    let joint =
        JointProtocolState::from_bootstrap_unverified(NET, a("admin"), ledger(), permitted.clone())
            .unwrap();
    let mut properties = PropertyCatalog::new();
    properties
        .execute(
            "addProperty",
            &[
                V::number(7.0),
                V::String("P7".into()),
                V::number(100.0),
                V::String("Fixed".into()),
                V::number(0.0),
                V::String("admin".into()),
                V::String("admin".into()),
            ],
            &mut NoTally,
        )
        .unwrap();
    let owners = SourceCatalogOwners::new(properties, ClearlistSourceOwner::new());
    let economics = EconomicConfig {
        native_entitlements: None,
        insurance: None,
        network: NET,
        anchor: anchor(),
        contract: 1.try_into().unwrap(),
        collateral: p7(),
        inverse: false,
        notional: "1".parse().unwrap(),
        initial_margin_per_contract: "1".parse().unwrap(),
        fee_route: FeeRoute::Native(1),
        policy: SettlementPolicy::ConservingV1,
        funding: None,
        marking: None,
        liquidation: None,
        amm: None,
        contract_binding: None,
        source_consensus: Some(SourceConsensusConfig {
            zk_verify: zk.then(fee),
            bootstrap_commitment: EconomicState::source_bootstrap_commitment(&joint).unwrap(),
            catalog_bootstrap_commitment: catalogs
                .then(|| catalog_commitment(&owners.checkpoint_bytes().unwrap())),
            policy: SourceConsensusPolicy::ConservingSequentialV1,
        }),
    };
    let config = EconomicChainConfig {
        economics,
        admin: a("admin"),
        permitted_spot_properties: permitted,
        rollback_limit,
    };
    if catalogs {
        EconomicChain::new_with_source_catalogs_unverified(config, ledger(), owners).unwrap()
    } else {
        EconomicChain::new_unverified(config, ledger()).unwrap()
    }
}

struct NoTally;
impl tradelayer_core::validity_source::SourceBackend for NoTally {
    fn invoke(
        &mut self,
        _: &tradelayer_core::validity_source::DependencyCall,
    ) -> Result<
        tradelayer_core::validity_source::DependencyReturn,
        tradelayer_core::validity_source::SourceRuleError,
    > {
        Err(tradelayer_core::validity_source::SourceRuleError::UnknownDependency)
    }
}

// --- frames and rows ---------------------------------------------------------------------

fn activate_frame() -> Vec<u8> {
    ZkActivatePayload { code_hash: [7; 32] }.encode_frame()
}
fn program() -> (ZkRegisterPayload, [u8; 32]) {
    let key = cubic_key();
    let id = program_id_of(ZkSystem::Nova, &key, 2, ZkEffect::StatementReceipt, 1, 1000);
    (
        ZkRegisterPayload {
            system: ZkSystem::Nova.code(),
            program_id: id,
            key_bytes: key,
            public_input_layout: 2,
            effect: ZkEffect::StatementReceipt.code(),
            verifier_version: 1,
            max_verify_ms: 1000,
        },
        id,
    )
}
fn register_frame() -> Vec<u8> {
    program().0.encode_frame()
}
fn mock_proof() -> Vec<u8> {
    let mut proof = 3u32.to_be_bytes().to_vec();
    proof.extend_from_slice(MOCK_PROOF_MAGIC);
    proof
}
fn verify_payload(sender: &str, expiry: u64) -> ZkVerifyPayload {
    let id = program().1;
    ZkVerifyPayload {
        system: ZkSystem::Nova.code(),
        program_id: id,
        public_inputs: vec![binding_element(&ctx(), &a(sender), expiry, &id), [9; 32]],
        proof: mock_proof(),
        expiry,
    }
}
fn verify_frame(sender: &str) -> Vec<u8> {
    verify_payload(sender, 50).encode_frame()
}
fn row(position: u32, id: u8, sender: &str, frame: &[u8]) -> SourceTransaction {
    SourceTransaction {
        position,
        txid: txid(id),
        payload: zk_row_payload(frame),
        marker: V::String("tl".into()),
        sender: V::String(sender.into()),
        reference: V::Array(Vec::new()),
        sender_amount: V::number(0.0),
        reference_amount: V::number(0.0),
    }
}
fn block_of(
    chain: &EconomicChain,
    marker: u8,
    rows: Vec<SourceTransaction>,
) -> UnverifiedSourceBlock {
    UnverifiedSourceBlock {
        network: NET,
        header: BlockHeader {
            height: chain.tip().height + 1,
            id: BlockId([marker; 32]),
            parent: chain.tip().id,
        },
        mode: SourceBlockMode::Realtime,
        transactions: rows,
    }
}
/// Applies a block and returns each row's `(disposition, reason)` in row order.
fn run(
    chain: &mut EconomicChain,
    marker: u8,
    rows: Vec<SourceTransaction>,
    verifier: &MockZkVerifier,
) -> Vec<(SourceDisposition, String)> {
    let block = block_of(chain, marker, rows);
    let report = chain
        .apply_source_zk_unverified_block(&block, verifier)
        .expect("block applies");
    let mut out: Vec<_> = report
        .receipts
        .iter()
        .map(|r| (r.position, r.disposition, r.reason.clone()))
        .collect();
    out.sort_by_key(|r| r.0);
    out.into_iter().map(|(_, d, r)| (d, r)).collect()
}
fn balance(chain: &EconomicChain, who: &str) -> u128 {
    chain.state().ledger().available_atoms(p7(), &a(who))
}
fn receipts(chain: &EconomicChain) -> Vec<StatementReceipt> {
    chain.state().joint().registry().zk_receipts().to_vec()
}
/// Policy on, type 36 activated at height 1 and the program registered at height 2.
fn ready(catalogs: bool, rollback_limit: usize) -> EconomicChain {
    let mut chain = chain(true, catalogs, rollback_limit);
    let v = MockZkVerifier::new();
    let r1 = run(
        &mut chain,
        1,
        vec![row(1, 10, "admin", &activate_frame())],
        &v,
    );
    assert_eq!(r1, [(SourceDisposition::Applied, "zk:activated".into())]);
    let r2 = run(
        &mut chain,
        2,
        vec![row(1, 11, "admin", &register_frame())],
        &v,
    );
    assert_eq!(r2, [(SourceDisposition::Applied, "zk:registered".into())]);
    chain
}
const APPLIED: SourceDisposition = SourceDisposition::Applied;
const REJECTED: SourceDisposition = SourceDisposition::EconomicRejected;
fn verified() -> (SourceDisposition, String) {
    (APPLIED, ZK_REASON_VERIFIED.into())
}
fn rejected(reason: ZkRejectReason) -> (SourceDisposition, String) {
    (REJECTED, zk_reason_rejected(&reason))
}

// --- frame codec -------------------------------------------------------------------------

#[test]
fn register_and_activate_frames_round_trip_and_reject_malformed_input() {
    let (register, _) = program();
    let raw = register.encode_frame();
    assert_eq!(&raw[..3], b"tl\x25");
    assert_eq!(ZkRegisterPayload::decode_frame(&raw).unwrap(), register);
    assert_eq!(
        ZkNodeFrame::decode(&raw).unwrap(),
        ZkNodeFrame::Register(register.clone())
    );
    let act = ZkActivatePayload { code_hash: [3; 32] };
    let raw_act = act.encode_frame();
    assert_eq!(&raw_act[..3], b"tl\x26");
    assert_eq!(raw_act.len(), 35);
    assert_eq!(ZkActivatePayload::decode_frame(&raw_act).unwrap(), act);
    assert_eq!(
        ZkNodeFrame::decode(&raw_act).unwrap(),
        ZkNodeFrame::Activate(act)
    );
    let verify = verify_payload("alice", 50);
    assert_eq!(
        ZkNodeFrame::decode(&verify.encode_frame()).unwrap(),
        ZkNodeFrame::Verify(verify)
    );
    // Every strict prefix of every frame is refused.
    for frame in [&raw, &raw_act, &verify_frame("alice")] {
        for cut in 0..frame.len() {
            assert!(
                ZkNodeFrame::decode(&frame[..cut]).is_err(),
                "prefix {cut} of {} accepted",
                frame.len()
            );
        }
        let mut longer = frame.clone();
        longer.push(0);
        assert!(ZkNodeFrame::decode(&longer).is_err());
    }
    // Wrong marker, wrong frame byte, oversized key.
    assert_eq!(ZkNodeFrame::decode(b"xx\x25"), Err(ZkWireError::NotZkFrame));
    assert_eq!(
        ZkNodeFrame::decode(b"tl\x27abc"),
        Err(ZkWireError::NotZkFrame)
    );
    let mut huge = register.clone();
    huge.key_bytes = vec![0; MAX_ZK_KEY_BYTES + 1];
    assert_eq!(
        ZkRegisterPayload::decode_frame(&huge.encode_frame()),
        Err(ZkWireError::TooLong)
    );
    // Unknown system or effect codes decode, then yield no action.
    let mut odd = register.clone();
    odd.system = 9;
    assert!(ZkRegisterPayload::decode_frame(&odd.encode_frame())
        .unwrap()
        .into_action()
        .is_none());
    let mut odd = register;
    odd.effect = 9;
    assert!(odd.into_action().is_none());
}

#[test]
fn row_payload_hex_is_canonical() {
    let frame = activate_frame();
    let payload = zk_row_payload(&frame);
    assert!(payload.starts_with("~zk:") && payload.is_ascii());
    assert_eq!(parse_zk_row_payload(&payload).unwrap(), frame);
    assert!(parse_zk_row_payload(&payload.to_uppercase()).is_none());
    assert!(parse_zk_row_payload(&payload[..payload.len() - 1]).is_none());
    assert!(parse_zk_row_payload("tl20;x;7;1").is_none());
    assert!(parse_zk_row_payload("~zk:zz").is_none());
}

#[test]
fn recorded_verifier_replays_each_verdict() {
    let inputs = [[1u8; 32], [2u8; 32]];
    let call = |reason: &str| {
        RecordedZkVerifier::from_reason(reason).verify(ZkSystem::Nova, b"k", 1, &inputs, b"p")
    };
    assert_eq!(
        call(ZK_REASON_VERIFIED).unwrap().public_inputs_hash,
        public_inputs_hash(&inputs)
    );
    for reject in [
        ZkReject::UnsupportedSystem,
        ZkReject::MalformedKey,
        ZkReject::MalformedProof,
        ZkReject::InvalidProof,
    ] {
        assert_eq!(
            call(&zk_reason_rejected(&ZkRejectReason::Verifier(reject))),
            Err(reject)
        );
    }
    let wrong = call(&zk_reason_rejected(&ZkRejectReason::StatementMismatch)).unwrap();
    assert_ne!(wrong.public_inputs_hash, public_inputs_hash(&inputs));
    // A record that never reached the verifier contradicts a call now.
    assert!(call("zk:invalid:payload").is_err());
}

// --- the policy --------------------------------------------------------------------------

#[test]
fn policy_off_changes_nothing() {
    for catalogs in [false, true] {
        let mut off = chain(false, catalogs, 4);
        let before = off.checkpoint_bytes().unwrap();
        assert!(off.state().joint().registry().zk_policy().is_none());
        // A ZK row is an unknown payload to the source rules: recorded, never applied.
        let block = block_of(&off, 1, vec![row(1, 10, "admin", &activate_frame())]);
        let report = off.apply_source_unverified_block(&block).unwrap();
        assert_eq!(report.accepted, 0);
        assert_eq!(report.rejected, 1);
        assert!(off.state().joint().registry().zk_policy().is_none());
        assert!(off.state().joint().registry().zk_receipts().is_empty());
        assert_ne!(before, off.checkpoint_bytes().unwrap());
    }
}

#[test]
fn config_with_the_policy_creates_the_registry_with_it() {
    for catalogs in [false, true] {
        let on = chain(true, catalogs, 4);
        assert_eq!(on.state().joint().registry().zk_policy(), Some(&fee()));
        // The bootstrap stays the pristine joint the deployment pinned.
        let off = chain(false, catalogs, 4);
        let pins = |c: &EconomicChain| {
            let s = c.config().economics.source_consensus.clone().unwrap();
            (s.bootstrap_commitment, s.catalog_bootstrap_commitment)
        };
        assert_eq!(pins(&on), pins(&off));
        // Snapshots differ only when the policy is set.
        assert_ne!(
            on.checkpoint_bytes().unwrap(),
            off.checkpoint_bytes().unwrap()
        );
    }
}

#[test]
fn a_chain_without_a_verifier_refuses_zk_rows() {
    let mut on = chain(true, true, 4);
    let block = block_of(&on, 1, vec![row(1, 10, "admin", &activate_frame())]);
    assert!(on.apply_source_unverified_block(&block).is_err());
    assert_eq!(on.tip(), anchor());
}

// --- semantics ---------------------------------------------------------------------------

#[test]
fn activation_registration_and_verification_follow_the_joint_rules() {
    for catalogs in [false, true] {
        let v = MockZkVerifier::new();
        let mut c = chain(true, catalogs, 8);
        // Type 36 before activation: invalid, no fee, no replay marker.
        let out = run(
            &mut c,
            1,
            vec![row(1, 20, "alice", &verify_frame("alice"))],
            &v,
        );
        assert_eq!(out[0].0, REJECTED);
        assert!(out[0].1.starts_with("zk:invalid:"), "{}", out[0].1);
        assert_eq!(balance(&c, "alice"), 12);
        // Only the admin activates.
        let out = run(&mut c, 2, vec![row(1, 21, "alice", &activate_frame())], &v);
        assert!(out[0].1.contains("Unauthorized"), "{}", out[0].1);
        let out = run(&mut c, 3, vec![row(1, 22, "admin", &activate_frame())], &v);
        assert_eq!(out, [(APPLIED, "zk:activated".into())]);
        // Registering in the block that activates is invalid; so is a non-admin registration.
        let out = run(&mut c, 4, vec![row(1, 23, "alice", &register_frame())], &v);
        assert!(out[0].1.contains("Unauthorized"), "{}", out[0].1);
        let out = run(&mut c, 5, vec![row(1, 24, "admin", &register_frame())], &v);
        assert_eq!(out, [(APPLIED, "zk:registered".into())]);
        // A second registration of the same program is refused.
        let out = run(&mut c, 6, vec![row(1, 25, "admin", &register_frame())], &v);
        assert!(out[0].1.contains("DuplicateZkProgram"), "{}", out[0].1);

        // Alice (12 atoms, fee 5) pays for two attempts and cannot pay a third.
        let (_, id) = program();
        let mut wrong_binding = verify_payload("alice", 50);
        wrong_binding.public_inputs[0] = binding_element(&ctx(), &a("bob"), 50, &id);
        let out = run(
            &mut c,
            7,
            vec![
                row(1, 30, "alice", &verify_frame("alice")),
                row(2, 31, "alice", &wrong_binding.encode_frame()),
                row(3, 32, "alice", &verify_frame("alice")),
            ],
            &v,
        );
        assert_eq!(out[0], verified());
        assert_eq!(out[1], rejected(ZkRejectReason::BindingMismatch));
        assert_eq!(out[2].0, REJECTED);
        assert!(out[2].1.starts_with("zk:invalid:"), "{}", out[2].1);
        assert_eq!(balance(&c, "alice"), 12 - 10);
        assert_eq!(balance(&c, "treasury"), 10);
        let got = receipts(&c);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].sender, a("alice"));
        assert_eq!(got[0].txid, txid(30));
        assert_eq!(got[0].height, 7);
        assert_eq!(got[0].program_id, id);
        // The rejected-after-fee attempt reached the verifier only if it passed the binding.
        assert_eq!(v.calls(), 1);

        // A repeated txid is a replay marker hit, not a second charge.
        let out = run(&mut c, 8, vec![row(1, 30, "bob", &verify_frame("bob"))], &v);
        assert_eq!(out[0].0, SourceDisposition::AlreadyProcessed);
        assert_eq!(balance(&c, "bob"), 100);

        // Garbage rows are recorded and change nothing.
        let mut bad = row(1, 33, "bob", &verify_frame("bob"));
        bad.payload = "~zk:zz".into();
        let mut bad_frame = row(2, 34, "bob", &[1, 2, 3]);
        bad_frame.payload = zk_row_payload(&[b't', b'l', 0x24, 1]);
        let mut no_sender = row(3, 35, "bob", &verify_frame("bob"));
        no_sender.sender = V::Null;
        let mut system_account = row(4, 36, "bob", &verify_frame("bob"));
        system_account.sender = V::String("~econ:x".into());
        let out = run(
            &mut c,
            9,
            vec![bad, bad_frame, no_sender, system_account],
            &v,
        );
        assert!(
            out.iter().all(|(d, _)| *d == SourceDisposition::Invalid),
            "{out:?}"
        );
        assert_eq!(balance(&c, "bob"), 100);
        assert_eq!(receipts(&c).len(), 1);
    }
}

#[test]
fn expiry_budget_and_unknown_program_reject_with_the_fee_kept() {
    let v = MockZkVerifier::new();
    let mut c = ready(true, 8);
    let (_, id) = program();
    let mut unknown = verify_payload("bob", 50);
    unknown.program_id = [9; 32];
    let mut expired = verify_payload("bob", 2);
    expired.expiry = 2;
    expired.public_inputs[0] = binding_element(&ctx(), &a("bob"), 2, &id);
    let out = run(
        &mut c,
        3,
        vec![
            row(1, 40, "bob", &unknown.encode_frame()),
            row(2, 41, "bob", &expired.encode_frame()),
        ],
        &v,
    );
    assert_eq!(out[0], rejected(ZkRejectReason::UnknownProgram));
    assert_eq!(out[1], rejected(ZkRejectReason::Expired));
    assert_eq!(balance(&c, "bob"), 90);
    assert_eq!(v.calls(), 0);

    // The program declares 1000 ms, so four verifications fit the 4000 ms block budget; the fifth
    // is over budget, never reaches the verifier and still pays.
    let fits = (MAX_ZK_BLOCK_VERIFY_MS / 1000) as usize;
    let mut c3 = ready(true, 8);
    let mut batch = Vec::new();
    for i in 0..=fits as u32 {
        batch.push(row(i + 1, 60 + i as u8, "bob", &verify_frame("bob")));
    }
    let out = run(&mut c3, 3, batch, &v);
    assert!(out[..fits].iter().all(|o| *o == verified()));
    assert_eq!(out[fits], rejected(ZkRejectReason::BudgetVerifyMs));
    assert_eq!(receipts(&c3).len(), fits);
    assert_eq!(balance(&c3, "bob"), 100 - 5 * (fits as u128 + 1));
    assert_eq!(v.calls(), fits);
}

#[test]
fn rows_apply_in_position_order_and_fees_follow_it() {
    let v = MockZkVerifier::new();
    // Alice holds 12 atoms: two fees, then she cannot pay. The first row in position order wins.
    for (first, second) in [(70u8, 71u8), (71, 70)] {
        let mut c = ready(true, 8);
        let rows = vec![
            row(1, first, "alice", &verify_frame("alice")),
            row(2, second, "alice", &verify_frame("alice")),
            row(3, 72, "alice", &verify_frame("alice")),
        ];
        let out = run(&mut c, 3, rows, &v);
        assert_eq!(out[0], verified());
        assert_eq!(out[1], verified());
        assert_eq!(out[2].0, REJECTED);
        let got: Vec<_> = receipts(&c).iter().map(|r| r.txid).collect();
        assert_eq!(got, [txid(first), txid(second)]);
        assert_eq!(balance(&c, "alice"), 2);
    }
}

// --- snapshot, rollback, restart ---------------------------------------------------------

#[test]
fn receipts_survive_checkpoint_restore_and_rollback() {
    for catalogs in [false, true] {
        let v = MockZkVerifier::new();
        // A window of 2 blocks, so older blocks fall into the floor state by replay.
        let mut c = ready(catalogs, 2);
        let b3 = run(
            &mut c,
            3,
            vec![row(1, 80, "alice", &verify_frame("alice"))],
            &v,
        );
        assert_eq!(b3, [verified()]);
        let tip3 = c.tip();
        let state3 = c.state().clone();
        let b4 = run(&mut c, 4, vec![row(1, 81, "bob", &verify_frame("bob"))], &v);
        assert_eq!(b4, [verified()]);
        let b5 = run(
            &mut c,
            5,
            vec![
                row(1, 82, "bob", &verify_frame("bob")),
                row(2, 83, "alice", &verify_frame("alice")),
            ],
            &v,
        );
        assert_eq!(b5, [verified(), verified()]);
        assert_eq!(receipts(&c).len(), 4);
        assert_eq!(c.rollback_floor().height, 3);

        // Restore from bytes: the replay takes proof verdicts from the recorded receipts.
        let bytes = c.checkpoint_bytes().unwrap();
        let restored =
            EconomicChain::from_checkpoint_bytes(&bytes, c.config().clone(), c.tip()).unwrap();
        assert_eq!(restored.state(), c.state());
        assert_eq!(restored.checkpoint_bytes().unwrap(), bytes);
        assert_eq!(receipts(&restored), receipts(&c));

        // Roll back to the floor: receipts and fees of blocks 4 and 5 disappear exactly.
        let mut back = c.clone();
        back.rollback_to_tip(tip3).unwrap();
        assert_eq!(back.state(), &state3);
        assert_eq!(receipts(&back).len(), 1);
        assert_eq!(balance(&back, "bob"), 100);
        assert_eq!(balance(&back, "alice"), 12 - 5);

        // Applying the same blocks again reproduces the chain byte for byte.
        let mut again = back;
        run(
            &mut again,
            4,
            vec![row(1, 81, "bob", &verify_frame("bob"))],
            &v,
        );
        run(
            &mut again,
            5,
            vec![
                row(1, 82, "bob", &verify_frame("bob")),
                row(2, 83, "alice", &verify_frame("alice")),
            ],
            &v,
        );
        assert_eq!(again.checkpoint_bytes().unwrap(), bytes);
    }
}

#[test]
fn a_reorg_to_another_branch_replaces_receipts() {
    let v = MockZkVerifier::new();
    let mut c = ready(true, 4);
    let fork = c.tip();
    run(
        &mut c,
        3,
        vec![row(1, 90, "alice", &verify_frame("alice"))],
        &v,
    );
    assert_eq!(receipts(&c)[0].txid, txid(90));
    c.rollback_to_tip(fork).unwrap();
    assert!(receipts(&c).is_empty());
    // The winning branch carries a rejected proof instead.
    let mut forged = verify_payload("alice", 50);
    forged.proof = [3u32.to_be_bytes().to_vec(), b"forged".to_vec()].concat();
    let out = run(
        &mut c,
        13,
        vec![row(1, 91, "alice", &forged.encode_frame())],
        &v,
    );
    assert_eq!(
        out,
        [rejected(ZkRejectReason::Verifier(ZkReject::InvalidProof))]
    );
    assert!(receipts(&c).is_empty());
    assert_eq!(balance(&c, "alice"), 7);
    // And it restores from its checkpoint with the recorded rejection.
    let bytes = c.checkpoint_bytes().unwrap();
    let restored =
        EconomicChain::from_checkpoint_bytes(&bytes, c.config().clone(), c.tip()).unwrap();
    assert_eq!(restored.state(), c.state());
}

#[test]
fn durable_chain_restarts_with_receipts_and_serves_them() {
    let v = MockZkVerifier::new();
    let root = std::env::temp_dir().join(format!("tl-zk-node-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let c = ready(true, 4);
    let tip = c.tip();
    let mut durable = DurableEconomicChain::initialize(&root, c, tip).unwrap();
    let head = durable.head().unwrap();
    let block = {
        let chain = durable.chain().unwrap();
        block_of(chain, 3, vec![row(1, 96, "alice", &verify_frame("alice"))])
    };
    let (head, report) = durable
        .apply_source_zk_unverified_block(head, &block, &v)
        .unwrap();
    assert_eq!(report.accepted, 1);
    let config = durable.chain().unwrap().config().clone();
    let expected = durable.chain().unwrap().state().clone();
    drop(durable);
    let (reopened, _) =
        DurableEconomicChain::load(&root, config, head, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(reopened.chain().unwrap().state(), &expected);

    // `tl_getZkReceipts` reads the economic state.
    let chain = reopened.chain().unwrap();
    let mut backend = EconomicReadBackend::from_chain(chain).unwrap();
    let rows = backend
        .call(ServiceCall {
            target: "registry.zk_receipts",
            arguments: vec![RpcValue::null(), RpcValue::string("alice").unwrap()],
        })
        .unwrap();
    let json = rows.as_json();
    assert!(
        json.contains("\"address\":\"alice\"") && json.contains("\"height\":3"),
        "{json}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_policy_requires_a_plain_source_owner() {
    // A fee sink in the system namespace, or a native/insurance owner, is refused.
    let mut config = chain(true, false, 4).config().clone();
    config
        .economics
        .source_consensus
        .as_mut()
        .unwrap()
        .zk_verify
        .as_mut()
        .unwrap()
        .sink = a("~econ:sink");
    assert!(matches!(
        config.validate(),
        Err(EconomicError::InvalidConfig)
    ));
}
