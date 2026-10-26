extern crate tradelayer_core as native_owner;
#[path = "support/native_fixture.rs"]
mod support;
use native_owner::chain::{BlockHeader, BlockId};
use native_owner::contract_orderbook::{Order, Side};
use native_owner::contract_registry::ContractId;
use native_owner::economic::{EconomicConfig, EconomicInput, EconomicState, SettlementPolicy};
use native_owner::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use native_owner::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use native_owner::native_entitlement::{NativeDriver, NativeEntitlementPolicy};
use native_owner::network_profile::TradeLayerMainnet;
use native_owner::registry::RegistryAction;
use native_owner::tally::FeeRoute;
use std::collections::BTreeSet;
use support::*;
fn config(policy: NativeEntitlementPolicy) -> EconomicConfig {
    EconomicConfig {
        insurance: None,
        network: policy.network,
        contract: ContractId::try_from(1).unwrap(),
        collateral: p(1),
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
        source_consensus: None,
        native_entitlements: Some(policy),
    }
}
fn input(id: u8, actor: &str, op: JointOperation) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: tx(id),
        actor: a(actor),
        operation: op,
    })
}
fn activate() -> EconomicInput {
    input(
        80,
        "admin",
        JointOperation::Registry(RegistryAction::Activate {
            types: vec![2, 6, 18],
            code_hash: [1; 32],
        }),
    )
}
fn order(id: u8, owner: &str, side: Side, height: u64) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: tx(id),
        owner: a(owner),
        contract: ContractId::try_from(1).unwrap(),
        height,
        side,
        quantity: "1".parse().unwrap(),
        price: "100".parse().unwrap(),
        post: false,
        is_market: false,
    })
}
fn funded_inputs() -> Vec<EconomicInput> {
    vec![
        EconomicInput::NativeTarget(request(81, 10 * U)),
        order(82, "alice", Side::Buy, 2),
        order(83, "bob", Side::Sell, 2),
    ]
}
fn ready(network: TradeLayerMainnet) -> (EconomicState, EconomicConfig) {
    let (mut ledger, policy) = fixture(network, NativeDriver::MonotonicDeltaV1);
    ledger
        .adopt_native_entitlements_unverified(&policy)
        .unwrap();
    let joint = JointProtocolState::from_native_bootstrap_unverified(
        network,
        a("admin"),
        ledger,
        BTreeSet::new(),
        &policy,
    )
    .unwrap();
    let c = config(policy);
    (
        EconomicState::from_joint_unverified(c.clone(), joint).unwrap(),
        c,
    )
}
#[test]
fn native_target_at_input_position_funds_real_contract_matching_and_complete_v8_restart() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let (mut s, c) = ready(network);
        s.apply_block_unverified(1, vec![activate()]).unwrap();
        assert_eq!(s.apply_block_unverified(2, funded_inputs()).unwrap(), 1);
        assert_eq!(s.margin_atoms(&a("alice")), U);
        assert_eq!(s.margin_atoms(&a("bob")), U);
        assert_eq!(s.ledger().native_remaining_atoms(p(2), &a("alice")), 5 * U);
        assert_eq!(
            s.positions().get(&a("alice")).unwrap().contracts,
            "1".parse().unwrap()
        );
        assert_eq!(s.native_entitlement_state().frames().len(), 2);
        assert_eq!(
            s.native_entitlement_state().frames()[1].inputs,
            funded_inputs()
        );
        assert_eq!(s.joint().processed_count(), 4);
        assert!(s.ledger().is_processed(tx(81)));
        let bytes = s.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..5], b"TLEC\x08");
        let reopened =
            EconomicState::from_snapshot_bytes(&bytes, &c, &a("admin"), &BTreeSet::new()).unwrap();
        assert_eq!(s, reopened);
        let mut wrong = c.clone();
        wrong.native_entitlements = None;
        assert!(
            EconomicState::from_snapshot_bytes(&bytes, &wrong, &a("admin"), &BTreeSet::new())
                .is_err()
        );
    }
}
#[test]
fn failed_last_local_target_rolls_back_coupon_send_and_can_retry_same_original_id() {
    let (mut s, _) = ready(TradeLayerMainnet::Bitcoin);
    s.apply_block_unverified(1, vec![activate()]).unwrap();
    s.apply_block_unverified(2, funded_inputs()).unwrap();
    let before = s.snapshot_bytes().unwrap();
    let send = input(
        84,
        "alice",
        JointOperation::Send {
            id: p(2),
            recipient: a("bob"),
            amount_atoms: 110_000_000,
        },
    );
    assert!(s
        .apply_block_unverified(
            3,
            vec![send, EconomicInput::NativeTarget(request(85, 20 * U))]
        )
        .is_err());
    assert_eq!(s.snapshot_bytes().unwrap(), before);
    assert!(!s.ledger().is_processed(tx(84)));
    assert!(!s.ledger().is_processed(tx(85)));
    s.apply_block_unverified(3, vec![EconomicInput::NativeTarget(request(85, 12 * U))])
        .unwrap();
    assert_eq!(s.ledger().native_remaining_atoms(p(2), &a("alice")), 4 * U);
}
#[test]
fn native_full_chain_rollback_floor_and_durable_compare_and_swap_restart() {
    use native_owner::storage::Recovery;
    let network = TradeLayerMainnet::Dogecoin;
    let (ledger, policy) = fixture(network, NativeDriver::MonotonicDeltaV1);
    let cfg = EconomicChainConfig {
        economics: config(policy),
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::new(),
        rollback_limit: 2,
    };
    let mut chain = EconomicChain::new_unverified(cfg.clone(), ledger).unwrap();
    let apply = |chain: &mut EconomicChain, id: u8, inputs: Vec<EconomicInput>| {
        let h = BlockHeader {
            height: chain.tip().height + 1,
            parent: chain.tip().id,
            id: BlockId([id; 32]),
        };
        chain
            .apply_unverified_block(&UnverifiedEconomicBlock {
                network,
                header: h,
                inputs,
            })
            .unwrap();
    };
    apply(&mut chain, 80, vec![activate()]);
    apply(&mut chain, 81, funded_inputs());
    apply(&mut chain, 82, vec![]);
    let before = chain.checkpoint_bytes().unwrap();
    let mut reopened =
        EconomicChain::from_checkpoint_bytes(&before, cfg.clone(), chain.tip()).unwrap();
    reopened.rollback_tip(reopened.tip()).unwrap();
    reopened.rollback_tip(reopened.tip()).unwrap();
    assert_eq!(reopened.tip().height, 1);
    assert_eq!(reopened.rollback_depth(), 0);
    let floorbytes = reopened.checkpoint_bytes().unwrap();
    reopened =
        EconomicChain::from_checkpoint_bytes(&floorbytes, cfg.clone(), reopened.tip()).unwrap();
    apply(&mut reopened, 90, funded_inputs());
    assert_eq!(reopened.state().receipts().len(), 1);
    let root = std::env::temp_dir().join(format!(
        "tl-native-owner-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let tip = reopened.tip();
    let owner = DurableEconomicChain::initialize(&root, reopened, tip).unwrap();
    let head = owner.head().unwrap();
    assert!(
        DurableEconomicChain::load(&root, cfg.clone(), head, Recovery::RejectIncompleteTail)
            .is_err()
    );
    drop(owner);
    let (mut owner, _) =
        DurableEconomicChain::load(&root, cfg.clone(), head, Recovery::RejectIncompleteTail)
            .unwrap();
    let block = UnverifiedEconomicBlock {
        network,
        header: BlockHeader {
            height: 3,
            parent: head.tip.id,
            id: BlockId([91; 32]),
        },
        inputs: vec![EconomicInput::NativeTarget(request(92, 12 * U))],
    };
    let newer = owner.apply_unverified_block(head, &block).unwrap();
    assert!(owner.apply_unverified_block(head, &block).is_err());
    let rolled = owner.rollback_to(newer, head.tip).unwrap();
    assert_ne!(rolled.sequence, head.sequence);
    assert_ne!(rolled.commitment, newer.commitment);
    drop(owner);
    assert!(
        DurableEconomicChain::load(&root, cfg.clone(), head, Recovery::RejectIncompleteTail)
            .is_err()
    );
    let (owner, _) =
        DurableEconomicChain::load(&root, cfg, rolled, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(owner.head().unwrap(), rolled);
    drop(owner);
    let resolved = std::fs::canonicalize(&root).unwrap();
    let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(resolved.is_absolute() && resolved.starts_with(&temp));
    assert!(resolved
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("tl-native-owner-"));
    std::fs::remove_dir_all(&resolved).unwrap();
}
#[test]
fn source_execution_composes_native_snapshot_but_forbids_explicit_target_inputs() {
    use native_owner::consensus_source::{
        SourceBlockMode, SourceConsensusConfig, SourceConsensusPolicy,
    };
    let (mut s, mut c) = ready(TradeLayerMainnet::Bitcoin);
    c.source_consensus = Some(SourceConsensusConfig {
        bootstrap_commitment: EconomicState::source_bootstrap_commitment(s.joint()).unwrap(),
        policy: SourceConsensusPolicy::ConservingSequentialV1,
    });
    s = EconomicState::from_joint_unverified(c.clone(), s.joint().clone()).unwrap();
    let before = s.snapshot_bytes().unwrap();
    assert!(s
        .apply_block_unverified(1, vec![EconomicInput::NativeTarget(request(80, U))])
        .is_err());
    assert_eq!(s.snapshot_bytes().unwrap(), before);
    use native_owner::consensus_source::{SourceDisposition, SourceTransaction};
    use native_owner::validity_source::SourceValue as V;
    let source = |id: u8, payload: String, sender: &str| SourceTransaction {
        position: 0,
        txid: tx(id),
        payload,
        marker: V::String("tl".into()),
        sender: V::String(sender.into()),
        reference: V::Null,
        sender_amount: V::number(1.0),
        reference_amount: V::number(0.0),
    };
    let activation = source(80, format!("02,{}", "11".repeat(32)), "admin");
    assert_eq!(
        s.apply_source_block_unverified(
            BlockHeader {
                height: 1,
                parent: c.network.genesis(),
                id: BlockId([80; 32])
            },
            SourceBlockMode::IndexedReplay,
            &[activation]
        )
        .unwrap()
        .accepted,
        1
    );
    let send = source(
        81,
        "20;1BoatSLRHtKNngkdXEeobR76b53LETtpyT;2;1".into(),
        "alice",
    );
    let report = s
        .apply_source_block_unverified(
            BlockHeader {
                height: 2,
                parent: BlockId([80; 32]),
                id: BlockId([81; 32]),
            },
            SourceBlockMode::IndexedReplay,
            &[send],
        )
        .unwrap();
    assert_eq!(report.accepted, 0);
    assert_eq!(report.rejected, 1);
    assert_eq!(
        report.receipts[0].disposition,
        SourceDisposition::DependencyUnavailable
    );
    assert_eq!(s.ledger().available_atoms(p(2), &a("alice")), 10 * U);
    assert_eq!(s.ledger().native_remaining_atoms(p(2), &a("alice")), 10 * U);
    let bytes = s.snapshot_bytes().unwrap();
    assert_eq!(
        EconomicState::from_snapshot_bytes(&bytes, &c, &a("admin"), &BTreeSet::new()).unwrap(),
        s
    );
    assert_eq!(s.native_entitlement_state().frames().len(), 2);
}
#[test]
fn actual_read_backend_projects_marker_only_metadata_without_spendable_credit() {
    use native_owner::economic_validity::EconomicSourceBackend;
    use native_owner::validity_source::{
        DependencyCall, DependencyReturn, SourceBackend, SourceValue as V,
    };
    let (ledger, mut policy) = fixture(TradeLayerMainnet::Bitcoin, NativeDriver::MonotonicDeltaV1);
    policy.bindings[0].initial_markers[0].atoms = 9 * U;
    policy.bindings[0]
        .initial_markers
        .push(native_owner::native_entitlement::NativeMarkerRow {
            owner: a("carol"),
            atoms: U,
        });
    let chain = EconomicChain::new_unverified(
        EconomicChainConfig {
            economics: config(policy),
            admin: a("admin"),
            permitted_spot_properties: BTreeSet::new(),
            rollback_limit: 2,
        },
        ledger,
    )
    .unwrap();
    let before = chain.checkpoint_bytes().unwrap();
    let mut backend = EconomicSourceBackend::from_chain_unverified(&chain, chain.tip()).unwrap();
    let DependencyReturn::Pending(value) = backend
        .invoke(&DependencyCall {
            target: "Tally.getTally".into(),
            arguments: vec![V::String("carol".into()), V::number(2.0)],
        })
        .unwrap()
    else {
        panic!("actual source async read expected")
    };
    assert_eq!(value.get("vesting"), V::number(1.0));
    assert_eq!(value.get("available"), V::number(0.0));
    assert_eq!(value.get("amount"), V::number(0.0));
    assert_eq!(chain.checkpoint_bytes().unwrap(), before);
}
#[test]
fn native_v8_keeps_complete_accepted_contract_frames_and_replays_actual_margin_fees() {
    use native_owner::contract_registry::{ContractAsset, ContractParameters};
    use native_owner::economic::{
        contract_definition_commitment, ContractBindingConfig, ContractRiskPolicy,
        CreateContractInput, MarkConfig, MarkPolicy,
    };
    let (mut s, mut c) = ready(TradeLayerMainnet::Litecoin);
    let creation = CreateContractInput {
        txid: tx(90),
        issuer: a("issuer"),
        parameters: ContractParameters {
            native: false,
            underlying_oracle_id: Some(1),
            on_chain_data: vec![],
            notional_property_id: ContractAsset::Property(p(1)),
            notional_value: "0.01".parse().unwrap(),
            collateral_property_id: ContractAsset::Property(p(1)),
            leverage: "10".parse().unwrap(),
            expiry_period: 0,
            series: 1,
            inverse: false,
            fee: "0".parse().unwrap(),
            whitelist: Some(0),
            ticker: None,
        },
    };
    c.notional = "0.01".parse().unwrap();
    c.initial_margin_per_contract = "0".parse().unwrap();
    c.fee_route = FeeRoute::Oracle(1);
    c.marking = Some(MarkConfig {
        oracle_id: 1,
        policy: MarkPolicy::ConservingMarkBasisV1,
    });
    c.contract_binding = Some(ContractBindingConfig {
        creation_txid: creation.txid,
        definition_hash: contract_definition_commitment(&creation).unwrap(),
        bootstrap_commitment: EconomicState::contract_bootstrap_commitment(s.joint()).unwrap(),
        policy: ContractRiskPolicy::SourceContextsConservingV1,
    });
    s = EconomicState::from_joint_unverified(c.clone(), s.joint().clone()).unwrap();
    s.apply_block_unverified(
        1,
        vec![input(
            80,
            "admin",
            JointOperation::Registry(RegistryAction::Activate {
                types: vec![2, 6, 13, 14, 16, 18],
                code_hash: [1; 32],
            }),
        )],
    )
    .unwrap();
    let mut inputs = vec![
        input(
            87,
            "publisher",
            JointOperation::Registry(RegistryAction::CreateOracle {
                ticker: "INDEX".into(),
                url: "https://x.invalid".into(),
                backup: a("backup"),
                lag: 1,
            }),
        ),
        input(
            88,
            "publisher",
            JointOperation::Registry(RegistryAction::PublishOracle {
                oracle_id: 1,
                price_1e4: 1_000_000,
                high_1e4: None,
                low_1e4: None,
                close_1e4: None,
            }),
        ),
        EconomicInput::CreateContract(creation),
    ];
    inputs.extend(funded_inputs());
    assert_eq!(s.apply_block_unverified(2, inputs.clone()).unwrap(), 1);
    assert!(s.margin_atoms(&a("alice")) > 0);
    assert_eq!(
        s.contract_binding_state().frames,
        s.native_entitlement_state().frames()
    );
    assert_eq!(s.contract_binding_state().frames[1].inputs, inputs);
    let bytes = s.snapshot_bytes().unwrap();
    assert_eq!(
        EconomicState::from_snapshot_bytes(&bytes, &c, &a("admin"), &BTreeSet::new()).unwrap(),
        s
    );
}
fn replace_embedded(bytes: &[u8], old: &[u8], new: &[u8]) -> Vec<u8> {
    let hits: Vec<_> = bytes
        .windows(old.len())
        .enumerate()
        .filter(|(_, b)| *b == old)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(hits.len(), 1);
    let at = hits[0];
    assert!(at >= 4);
    assert_eq!(
        u32::from_be_bytes(bytes[at - 4..at].try_into().unwrap()) as usize,
        old.len()
    );
    let mut out = bytes[..at - 4].to_vec();
    out.extend_from_slice(&(new.len() as u32).to_be_bytes());
    out.extend_from_slice(new);
    out.extend_from_slice(&bytes[at + old.len()..bytes.len() - 4]);
    let mut c = !0u32;
    for b in &out {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = (c >> 1) ^ if c & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    out.extend_from_slice(&(!c).to_be_bytes());
    out
}
#[test]
fn valid_crc_branch_splice_cannot_retroactively_insert_native_input_history() {
    let (ledger, policy) = fixture(TradeLayerMainnet::Bitcoin, NativeDriver::MonotonicDeltaV1);
    let cfg = EconomicChainConfig {
        economics: config(policy),
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::new(),
        rollback_limit: 4,
    };
    let mut chain = EconomicChain::new_unverified(cfg.clone(), ledger).unwrap();
    let b1 = UnverifiedEconomicBlock {
        network: cfg.economics.network,
        header: BlockHeader {
            height: 1,
            parent: chain.tip().id,
            id: BlockId([80; 32]),
        },
        inputs: vec![activate()],
    };
    chain.apply_unverified_block(&b1).unwrap();
    let mut alternate = chain.clone();
    let b2 = UnverifiedEconomicBlock {
        network: cfg.economics.network,
        header: BlockHeader {
            height: 2,
            parent: chain.tip().id,
            id: BlockId([81; 32]),
        },
        inputs: vec![EconomicInput::NativeTarget(request(81, 10 * U))],
    };
    chain.apply_unverified_block(&b2).unwrap();
    let old = chain.state().snapshot_bytes().unwrap();
    let mut empty = b2;
    empty.inputs.clear();
    alternate.apply_unverified_block(&empty).unwrap();
    let replacement = alternate.state().snapshot_bytes().unwrap();
    chain
        .apply_unverified_block(&UnverifiedEconomicBlock {
            network: cfg.economics.network,
            header: BlockHeader {
                height: 3,
                parent: chain.tip().id,
                id: BlockId([82; 32]),
            },
            inputs: vec![],
        })
        .unwrap();
    let forged = replace_embedded(&chain.checkpoint_bytes().unwrap(), &old, &replacement);
    assert!(EconomicChain::from_checkpoint_bytes(&forged, cfg, chain.tip()).is_err());
}
