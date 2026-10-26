use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::activation_history::{ActivationHistoryError as E, ActivationHistoryView};
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::consensus_source::{
    SourceBlockMode, SourceConsensusConfig, SourceConsensusPolicy, SourceDisposition,
    SourceTransaction, UnverifiedSourceBlock,
};
use tradelayer_core::economic::{EconomicConfig, EconomicState, SettlementPolicy};
use tradelayer_core::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet as Network;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::storage::Recovery;
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceValue as V,
};

fn account(s: &str) -> AccountId {
    s.try_into().unwrap()
}
fn property() -> PropertyId {
    7.try_into().unwrap()
}
fn txid(n: u64) -> TxId {
    let mut b = [0; 32];
    b[24..].copy_from_slice(&n.to_be_bytes());
    TxId::from_bytes(b)
}
fn bid(n: u64) -> BlockId {
    let mut b = [0x32; 32];
    b[24..].copy_from_slice(&n.to_be_bytes());
    BlockId(b)
}
fn fixture(network: Network, source: bool) -> EconomicChain {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            txid(900),
            Operation::CreateProperty {
                id: property(),
                issuer: account("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 100_000_000,
            },
        )
        .unwrap();
    let joint = JointProtocolState::from_bootstrap_unverified(
        network,
        account("admin"),
        ledger.clone(),
        BTreeSet::from([property()]),
    )
    .unwrap();
    let config = EconomicConfig {
        native_entitlements: None,
        insurance: None,
        network,
        contract: 1.try_into().unwrap(),
        collateral: property(),
        inverse: false,
        notional: 1.into(),
        initial_margin_per_contract: 10.into(),
        fee_route: FeeRoute::Native(1),
        policy: SettlementPolicy::ConservingV1,
        funding: None,
        marking: None,
        liquidation: None,
        amm: None,
        contract_binding: None,
        source_consensus: source.then(|| SourceConsensusConfig {
            bootstrap_commitment: EconomicState::source_bootstrap_commitment(&joint).unwrap(),
            policy: SourceConsensusPolicy::ConservingSequentialV1,
        }),
    };
    EconomicChain::new_unverified(
        EconomicChainConfig {
            economics: config,
            admin: account("admin"),
            permitted_spot_properties: BTreeSet::from([property()]),
            rollback_limit: 2,
        },
        ledger,
    )
    .unwrap()
}
fn raw(position: u32, n: u64, payload: String, sender: &str) -> SourceTransaction {
    SourceTransaction {
        position,
        txid: txid(n),
        payload,
        marker: V::String("tl".into()),
        sender: V::String(sender.into()),
        reference: V::Null,
        sender_amount: V::number(1.0),
        reference_amount: V::number(0.0),
    }
}
fn activate(position: u32, n: u64, targets: &str, hash: u8) -> SourceTransaction {
    raw(
        position,
        n,
        format!("0{targets},{}", format!("{hash:02x}").repeat(32)),
        "admin",
    )
}
fn block(c: &EconomicChain, n: u64, transactions: Vec<SourceTransaction>) -> UnverifiedSourceBlock {
    UnverifiedSourceBlock {
        network: c.config().economics.network,
        header: BlockHeader {
            height: c.tip().height + 1,
            id: bid(n),
            parent: c.tip().id,
        },
        mode: SourceBlockMode::IndexedReplay,
        transactions,
    }
}

#[test]
fn actual_v7_owner_origins_match_registry_and_prior_height_gates_on_all_networks() {
    for network in [Network::Bitcoin, Network::Litecoin, Network::Dogecoin] {
        let mut c = fixture(network, true);
        let before = c.checkpoint_bytes().unwrap();
        {
            let v = ActivationHistoryView::from_chain_unverified(&c, c.tip()).unwrap();
            assert!(v.records().is_empty());
            assert_eq!(v.activation_at(2, 0).unwrap(), None);
            assert!(!v.is_effective_at(2, 1).unwrap());
        }
        assert_eq!(c.checkpoint_bytes().unwrap(), before);
        let b = block(
            &c,
            1,
            vec![
                activate(0, 1, "14;13", 0x11),
                raw(2, 2, "dBTC,https://x,backup,,1".into(), "admin"),
            ],
        );
        let report = c.apply_source_unverified_block(&b).unwrap();
        assert_eq!(report.accepted, 1);
        assert_eq!(
            report.receipts[1].disposition,
            SourceDisposition::EconomicRejected
        );
        {
            let v = ActivationHistoryView::from_chain_unverified(&c, c.tip()).unwrap();
            let r = &v.records()[0];
            assert_eq!(r.txid, txid(1));
            assert_eq!(r.claimed_actor, account("admin"));
            assert_eq!(r.header, b.header);
            assert_eq!(r.position, 0);
            assert_eq!(r.execution_index, 0);
            assert_eq!(r.types, vec![14, 13]);
            assert_eq!(r.code_hash, [0x11; 32]);
            assert_eq!(v.network(), network);
            assert!(!v.is_effective_at(13, 1).unwrap());
            assert!(v.is_effective_at(13, 2).unwrap());
            assert_eq!(v.activation_at(13, 0).unwrap(), None);
            assert_eq!(v.activation_at(13, 1).unwrap().unwrap().height, 1);
            assert!(v.origin(2).unwrap().is_none());
            assert_eq!(v.origin(36), Err(E::InvalidType(36)));
            assert_eq!(v.activation_at(13, 2), Err(E::HeightOutsideHistory));
            assert_eq!(v.is_effective_at(13, 3), Err(E::HeightOutsideHistory));
        }
        let b2 = block(
            &c,
            2,
            vec![
                activate(1, 3, "2;18", 0x22),
                raw(3, 4, "dBTC,https://x,backup,,1".into(), "admin"),
            ],
        );
        assert_eq!(c.apply_source_unverified_block(&b2).unwrap().accepted, 2);
        let snapshot = c.checkpoint_bytes().unwrap();
        let v = ActivationHistoryView::from_chain_unverified(&c, c.tip()).unwrap();
        assert_eq!(v.records().len(), 2);
        assert_eq!(v.origin(18).unwrap().unwrap().code_hash, [0x22; 32]);
        assert!(!v.is_effective_at(18, 2).unwrap());
        assert!(v.is_effective_at(18, 3).unwrap());
        assert_eq!(
            v.require_authenticated_admission(),
            Err(E::UnauthenticatedInputAndState)
        );
        assert_eq!(
            v.records()[0].require_authenticated_admission(),
            Err(E::UnauthenticatedInputAndState)
        );
        assert_eq!(snapshot, c.checkpoint_bytes().unwrap());
        // Source-shaped presence is intentionally NOT changed into a prior-height read.
        let mut backend =
            tradelayer_core::economic_validity::EconomicSourceBackend::from_chain_unverified(
                &c,
                c.tip(),
            )
            .unwrap();
        assert_eq!(
            backend
                .invoke(&DependencyCall {
                    target: "Activation.isTxTypeActive".into(),
                    arguments: vec![V::number(18.0)]
                })
                .unwrap(),
            DependencyReturn::Pending(V::Bool(true))
        );
    }
}

#[test]
fn only_applied_origins_survive_rejections_and_type_zero_does_not_mint_genesis() {
    let mut c = fixture(Network::Litecoin, true);
    let before = c
        .state()
        .ledger()
        .available_atoms(property(), &account("alice"));
    let mut unauthorized = activate(0, 1, "2", 0x11);
    unauthorized.sender = V::String("mallory".into());
    let b = block(
        &c,
        1,
        vec![
            unauthorized,
            activate(1, 2, "13;14", 0x22),
            activate(2, 3, "13", 0x33),
            activate(3, 4, "34", 0x44),
            activate(4, 5, "0", 0x55),
        ],
    );
    let report = c.apply_source_unverified_block(&b).unwrap();
    assert_eq!(report.accepted, 2);
    let v = ActivationHistoryView::from_chain_unverified(&c, c.tip()).unwrap();
    assert_eq!(
        v.records().iter().map(|r| r.txid).collect::<Vec<_>>(),
        vec![txid(2), txid(5)]
    );
    assert_eq!(v.records()[0].execution_index, 1);
    assert_eq!(v.records()[1].execution_index, 4);
    assert!(v.origin(2).unwrap().is_none());
    assert!(v.origin(34).unwrap().is_none());
    assert!(v
        .origin(0)
        .unwrap()
        .unwrap()
        .requires_native_genesis_policy());
    assert!(!v
        .origin(13)
        .unwrap()
        .unwrap()
        .requires_native_genesis_policy());
    assert_eq!(
        c.state()
            .ledger()
            .available_atoms(property(), &account("alice")),
        before
    );
    assert_eq!(
        c.state()
            .ledger()
            .available_atoms(property(), &account("admin")),
        0
    );
}

#[test]
fn legacy_and_incomplete_origins_are_explicit_and_genesis_source_view_is_complete() {
    let mut c = fixture(Network::Bitcoin, false);
    assert!(matches!(
        ActivationHistoryView::from_chain_unverified(&c, c.tip()),
        Err(E::MissingOriginHistory)
    ));
    c.apply_unverified_block(&UnverifiedEconomicBlock {
        network: Network::Bitcoin,
        header: BlockHeader {
            height: 1,
            id: bid(1),
            parent: c.tip().id,
        },
        inputs: vec![tradelayer_core::economic::EconomicInput::Joint(
            UnverifiedJointTx {
                txid: txid(1),
                actor: account("admin"),
                operation: JointOperation::Registry(RegistryAction::Activate {
                    types: vec![2],
                    code_hash: [1; 32],
                }),
            },
        )],
    })
    .unwrap();
    assert!(c.state().joint().registry().activation(2).is_some());
    assert!(matches!(
        ActivationHistoryView::from_chain_unverified(&c, c.tip()),
        Err(E::MissingOriginHistory)
    ));
    let fresh = fixture(Network::Bitcoin, true);
    let wrong = Tip {
        height: fresh.tip().height,
        id: bid(77),
    };
    assert!(matches!(
        ActivationHistoryView::from_chain_unverified(&fresh, wrong),
        Err(E::WrongTip)
    ));
    assert!(EconomicChain::from_checkpoint_bytes(
        &c.checkpoint_bytes().unwrap(),
        fresh.config().clone(),
        c.tip()
    )
    .is_err());
}

#[test]
fn claimed_fabricated_reordered_or_prebootstrap_origins_are_rejected() {
    let mut c = fixture(Network::Dogecoin, true);
    let b = block(
        &c,
        1,
        vec![activate(0, 1, "13;14", 0x11), activate(2, 2, "2", 0x22)],
    );
    c.apply_source_unverified_block(&b).unwrap();
    let v = ActivationHistoryView::from_chain_unverified(&c, c.tip()).unwrap();
    let good = v.records().to_vec();
    v.verify_claimed_history(&good).unwrap();
    for variant in 0..10 {
        let mut changed = good.clone();
        match variant {
            0 => changed[0].txid = txid(77),
            1 => changed[0].claimed_actor = account("mallory"),
            2 => changed[0].header.height = 0,
            3 => changed[0].header.id = bid(99),
            4 => changed[0].position = 1,
            5 => changed[0].execution_index = 1,
            6 => changed[0].types.reverse(),
            7 => changed[0].code_hash = [0; 32],
            8 => {
                changed.pop();
            }
            _ => changed.reverse(),
        };
        assert_eq!(
            v.verify_claimed_history(&changed),
            Err(E::ClaimedHistoryMismatch)
        );
    }
    assert_eq!(
        v.verify_claimed_history(&vec![good[0].clone(); 37]),
        Err(E::Capacity)
    );
}

#[test]
fn checkpoint_restart_floor_rollback_and_new_branch_derive_only_current_origins() {
    let mut c = fixture(Network::Litecoin, true);
    for (n, target) in [(1, "13"), (2, "14"), (3, "2"), (4, "18")] {
        let b = block(&c, n, vec![activate(0, n, target, n as u8)]);
        c.apply_source_unverified_block(&b).unwrap();
    }
    let bytes = c.checkpoint_bytes().unwrap();
    let expected = ActivationHistoryView::from_chain_unverified(&c, c.tip())
        .unwrap()
        .records()
        .to_vec();
    let mut reopened =
        EconomicChain::from_checkpoint_bytes(&bytes, c.config().clone(), c.tip()).unwrap();
    assert_eq!(
        ActivationHistoryView::from_chain_unverified(&reopened, reopened.tip())
            .unwrap()
            .records(),
        expected
    );
    assert_eq!(reopened.rollback_floor().height, 2);
    reopened.rollback_tip(reopened.tip()).unwrap();
    reopened.rollback_tip(reopened.tip()).unwrap();
    assert_eq!(reopened.rollback_depth(), 0);
    assert_eq!(reopened.tip().height, 2);
    let floor_bytes = reopened.checkpoint_bytes().unwrap();
    let mut floor = EconomicChain::from_checkpoint_bytes(
        &floor_bytes,
        reopened.config().clone(),
        reopened.tip(),
    )
    .unwrap();
    assert_eq!(
        ActivationHistoryView::from_chain_unverified(&floor, floor.tip())
            .unwrap()
            .records()
            .len(),
        2
    );
    let branch = block(&floor, 99, vec![activate(0, 99, "2;18", 0xaa)]);
    floor.apply_source_unverified_block(&branch).unwrap();
    let v = ActivationHistoryView::from_chain_unverified(&floor, floor.tip()).unwrap();
    assert_eq!(v.records().len(), 3);
    assert_eq!(v.origin(2).unwrap().unwrap().header.id, bid(99));
    assert_eq!(v.origin(18).unwrap().unwrap().txid, txid(99));
    assert_eq!(v.origin(13).unwrap().unwrap().txid, txid(1));
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "tl-activation-history-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let root = self.0.canonicalize().unwrap();
        let parent = std::env::temp_dir().canonicalize().unwrap();
        assert!(root.is_absolute() && root.starts_with(&parent));
        assert!(root
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("tl-activation-history-"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn durable_health_head_restart_and_aba_are_bound_without_mutating_owner() {
    let temp = Temp::new();
    let mut c = fixture(Network::Bitcoin, true);
    let b = block(&c, 1, vec![activate(0, 1, "13;14", 1)]);
    c.apply_source_unverified_block(&b).unwrap();
    let cfg = c.config().clone();
    let first_tip = c.tip();
    let mut owner = DurableEconomicChain::initialize(&temp.0, c, first_tip).unwrap();
    let h1 = owner.head().unwrap();
    let first = ActivationHistoryView::from_durable(&owner, h1)
        .unwrap()
        .records()
        .to_vec();
    let b2 = block(owner.chain().unwrap(), 2, vec![activate(0, 2, "2", 2)]);
    let (h2, _) = owner.apply_source_unverified_block(h1, &b2).unwrap();
    assert!(matches!(
        ActivationHistoryView::from_durable(&owner, h1),
        Err(E::WrongHead)
    ));
    let all = ActivationHistoryView::from_durable(&owner, h2)
        .unwrap()
        .records()
        .to_vec();
    assert_eq!(all.len(), 2);
    drop(owner);
    let (mut owner, _) =
        DurableEconomicChain::load(&temp.0, cfg.clone(), h2, Recovery::RejectIncompleteTail)
            .unwrap();
    assert_eq!(
        ActivationHistoryView::from_durable(&owner, h2)
            .unwrap()
            .records(),
        all
    );
    let h3 = owner.rollback_to(h2, first_tip).unwrap();
    assert!(h3.sequence > h2.sequence);
    assert!(matches!(
        ActivationHistoryView::from_durable(&owner, h1),
        Err(E::WrongHead)
    ));
    let v = ActivationHistoryView::from_durable(&owner, h3).unwrap();
    assert_eq!(v.records(), first);
    assert_eq!(v.durable_head(), Some(h3));
    assert_eq!(owner.head().unwrap(), h3);
    drop(v);
    drop(owner);
    assert!(
        DurableEconomicChain::load(&temp.0, cfg.clone(), h2, Recovery::RejectIncompleteTail)
            .is_err()
    );
    let (owner, _) =
        DurableEconomicChain::load(&temp.0, cfg, h3, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(
        ActivationHistoryView::from_durable(&owner, h3)
            .unwrap()
            .records(),
        first
    );
}

fn repair_crc(bytes: &mut [u8]) {
    let n = bytes.len();
    let mut c = !0u32;
    for b in &bytes[..n - 4] {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb8_8320u32.wrapping_mul(c & 1));
        }
    }
    bytes[n - 4..].copy_from_slice(&(!c).to_be_bytes());
}
#[test]
fn forged_raw_origins_with_repaired_crc_cannot_become_an_actual_history_owner() {
    let mut c = fixture(Network::Bitcoin, true);
    let b = block(&c, 1, vec![activate(0, 1, "13;14", 0x11)]);
    c.apply_source_unverified_block(&b).unwrap();
    let bytes = c.state().snapshot_bytes().unwrap();
    let needle = format!("013;14,{}", "11".repeat(32));
    let offset = bytes
        .windows(needle.len())
        .position(|w| w == needle.as_bytes())
        .unwrap();
    let mut changed = bytes.clone();
    changed[offset + 2] = b'5';
    repair_crc(&mut changed);
    assert!(EconomicState::from_snapshot_bytes(
        &changed,
        &c.config().economics,
        &account("admin"),
        &BTreeSet::from([property()])
    )
    .is_err());
    let mut changed = bytes;
    let source_start = 42 + u32::from_be_bytes(changed[38..42].try_into().unwrap()) as usize + 4;
    let bootstrap_len =
        u32::from_be_bytes(changed[source_start..source_start + 4].try_into().unwrap()) as usize;
    let frame_start = source_start + 4 + bootstrap_len + 4;
    changed[frame_start..frame_start + 8].copy_from_slice(&2u64.to_be_bytes());
    repair_crc(&mut changed);
    assert!(EconomicState::from_snapshot_bytes(
        &changed,
        &c.config().economics,
        &account("admin"),
        &BTreeSet::from([property()])
    )
    .is_err());
}
