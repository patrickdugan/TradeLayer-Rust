//! Complete economic history and durable journal recovery. All chain data and
//! actors are unverified; CRC repairs test structural checks, not authenticity.
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{EconomicConfig, EconomicInput, SettlementPolicy};
use tradelayer_core::economic_chain::{
    DurableEconomicChain, DurableEconomicError, EconomicChain, EconomicChainConfig,
    UnverifiedEconomicBlock,
};
use tradelayer_core::iou::IouKey;
use tradelayer_core::joint::{JointOperation, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::storage::{Journal, Namespace, Recovery, StorageError};
use tradelayer_core::tally::FeeRoute;

const ATOMS: u128 = 100_000_000;
const LOCK_PROBE_ROOT: &str = "TL_ECONOMIC_CHAIN_LOCK_PROBE_ROOT";
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-economic-chain-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
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
            .starts_with("tl-economic-chain-"));
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}
fn a(text: &str) -> AccountId {
    AccountId::try_from(text).unwrap()
}
fn p() -> PropertyId {
    PropertyId::try_from(7).unwrap()
}
fn c() -> ContractId {
    ContractId::try_from(1).unwrap()
}
fn id(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn config(network: TradeLayerMainnet, rollback_limit: usize) -> EconomicChainConfig {
    EconomicChainConfig {
        economics: EconomicConfig {
            network,
            contract: c(),
            collateral: p(),
            inverse: false,
            notional: d("1"),
            initial_margin_per_contract: d("10"),
            fee_route: FeeRoute::Native(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
            marking: None,
            liquidation: None,
            amm: None,
        },
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p()]),
        rollback_limit,
    }
}
fn ledger() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(1),
            Operation::CreateProperty {
                id: p(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 4000 * ATOMS,
            },
        )
        .unwrap();
    for (n, owner) in [(2, "bob"), (3, "carol"), (4, "dave")] {
        ledger
            .apply(
                id(n),
                Operation::Transfer {
                    id: p(),
                    from: a("alice"),
                    to: a(owner),
                    amount_atoms: 1000 * ATOMS,
                },
            )
            .unwrap();
    }
    ledger
}
fn initial(network: TradeLayerMainnet, limit: usize) -> EconomicChain {
    EconomicChain::new_unverified(config(network, limit), ledger()).unwrap()
}
fn block(chain: &EconomicChain, marker: u8, inputs: Vec<EconomicInput>) -> UnverifiedEconomicBlock {
    UnverifiedEconomicBlock {
        network: chain.config().economics.network,
        header: BlockHeader {
            height: chain.tip().height + 1,
            id: BlockId([marker; 32]),
            parent: chain.tip().id,
        },
        inputs,
    }
}
fn activate() -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(10),
        actor: a("admin"),
        operation: JointOperation::Registry(RegistryAction::Activate {
            types: vec![2, 6, 18],
            code_hash: [0x71; 32],
        }),
    })
}
fn order(
    n: u8,
    owner: &str,
    side: Side,
    quantity: &str,
    price: &str,
    height: u64,
) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: id(n),
        owner: a(owner),
        contract: c(),
        height,
        side,
        quantity: d(quantity),
        price: d(price),
        post: false,
        is_market: false,
    })
}
fn opening() -> Vec<EconomicInput> {
    vec![
        order(20, "alice", Side::Buy, "1", "100", 2),
        order(21, "bob", Side::Sell, "1", "100", 2),
    ]
}
fn deferred_claim() -> Vec<EconomicInput> {
    vec![
        order(30, "carol", Side::Buy, "0.5", "110", 3),
        order(31, "alice", Side::Sell, "0.5", "110", 3),
    ]
}
fn funded_payout() -> Vec<EconomicInput> {
    vec![
        order(40, "bob", Side::Buy, "0.5", "110", 4),
        order(41, "dave", Side::Sell, "0.5", "110", 4),
    ]
}
fn with_claim(network: TradeLayerMainnet, limit: usize) -> EconomicChain {
    let mut chain = initial(network, limit);
    chain
        .apply_unverified_block(&block(&chain, 11, vec![activate()]))
        .unwrap();
    chain
        .apply_unverified_block(&block(&chain, 22, opening()))
        .unwrap();
    chain
        .apply_unverified_block(&block(&chain, 33, deferred_claim()))
        .unwrap();
    assert_unpaid_claim(&chain);
    chain
}
fn assert_unpaid_claim(chain: &EconomicChain) {
    assert_eq!(
        chain
            .state()
            .ious()
            .claim_map(IouKey::new(1, 7))
            .unwrap()
            .claim(&a("alice")),
        Some(&d("5"))
    );
    assert_eq!(chain.state().loss_pool_atoms(), 0);
    assert_eq!(
        chain
            .state()
            .positions()
            .get(&a("alice"))
            .unwrap()
            .contracts,
        d("0.5")
    );
    assert_eq!(
        chain.state().positions().get(&a("bob")).unwrap().contracts,
        d("-1")
    );
    assert_eq!(chain.state().margin_atoms(&a("alice")), 5 * ATOMS);
    assert!(
        chain
            .state()
            .fees()
            .expected_custody_atoms(FeeRoute::Native(1).key(p()).unwrap())
            .unwrap()
            > 0
    );
    chain.state().verify().unwrap();
    chain.state().ledger().verify_conservation().unwrap();
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
fn checksum(mut bytes: Vec<u8>) -> Vec<u8> {
    let n = bytes.len() - 4;
    let crc = crc32(&bytes[..n]);
    bytes[n..].copy_from_slice(&crc.to_be_bytes());
    bytes
}

#[test]
fn full_matched_state_claim_checkpoint_reopens_and_rolls_back_on_each_network() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let mut chain = with_claim(network, 4);
        let before = chain.clone();
        let bytes = chain.checkpoint_bytes().unwrap();
        let restored =
            EconomicChain::from_checkpoint_bytes(&bytes, chain.config().clone(), chain.tip())
                .unwrap();
        assert_eq!(restored, before);
        assert_eq!(restored.checkpoint_bytes().unwrap(), bytes);
        chain
            .apply_unverified_block(&block(&chain, 44, funded_payout()))
            .unwrap();
        assert!(chain
            .state()
            .ious()
            .claim_map(IouKey::new(1, 7))
            .unwrap()
            .claims()
            .is_empty());
        assert_eq!(chain.state().receipts()[2].paid_claim_atoms, 5 * ATOMS);
        assert_eq!(chain.rollback_tip(chain.tip()).unwrap(), before.tip());
        assert_eq!(chain, before);
        assert_unpaid_claim(&chain);
        assert_eq!(chain.checkpoint_bytes().unwrap(), bytes);
    }
}

#[test]
fn retained_history_evicts_to_real_claim_floor_then_empty_floor_reopens_and_branches() {
    let network = TradeLayerMainnet::Litecoin;
    let mut chain = with_claim(network, 2);
    let floor_state = chain.state().clone();
    let floor_tip = chain.tip();
    chain
        .apply_unverified_block(&block(&chain, 44, funded_payout()))
        .unwrap();
    chain
        .apply_unverified_block(&block(&chain, 55, vec![]))
        .unwrap();
    assert_eq!(chain.rollback_floor(), floor_tip);
    assert_eq!(chain.rollback_depth(), 2);
    chain.rollback_tip(chain.tip()).unwrap();
    chain.rollback_tip(chain.tip()).unwrap();
    assert_eq!(chain.tip(), floor_tip);
    assert_eq!(chain.rollback_depth(), 0);
    assert_eq!(chain.state(), &floor_state);
    assert_unpaid_claim(&chain);
    let bytes = chain.checkpoint_bytes().unwrap();
    let mut restored =
        EconomicChain::from_checkpoint_bytes(&bytes, chain.config().clone(), floor_tip).unwrap();
    let before = restored.clone();
    assert!(restored.rollback_tip(floor_tip).is_err());
    assert_eq!(restored, before);
    restored
        .apply_unverified_block(&block(&restored, 66, funded_payout()))
        .unwrap();
    assert_eq!(restored.rollback_floor(), floor_tip);
    assert_eq!(restored.state().receipts()[2].paid_claim_atoms, 5 * ATOMS);
    restored.rollback_tip(restored.tip()).unwrap();
    assert_eq!(restored.checkpoint_bytes().unwrap(), bytes);
}

#[test]
fn wrong_header_tip_network_and_deployment_configuration_are_atomic() {
    let mut chain = with_claim(TradeLayerMainnet::Litecoin, 4);
    let valid = block(&chain, 44, vec![]);
    for mutation in 0..6 {
        let mut bad = valid.clone();
        match mutation {
            0 => bad.network = TradeLayerMainnet::Bitcoin,
            1 => bad.header.height += 1,
            2 => bad.header.parent = BlockId([0x77; 32]),
            3 => bad.header.id = chain.tip().id,
            4 => bad.header.id = BlockId([22; 32]),
            _ => bad.header.id = chain.config().anchor().id,
        }
        let before = chain.clone();
        assert!(chain.apply_unverified_block(&bad).is_err());
        assert_eq!(chain, before);
    }
    let before = chain.clone();
    assert!(chain
        .rollback_tip(Tip {
            height: chain.tip().height,
            id: BlockId([0x77; 32])
        })
        .is_err());
    assert_eq!(chain, before);
    let bytes = chain.checkpoint_bytes().unwrap();
    assert!(EconomicChain::from_checkpoint_bytes(
        &bytes,
        chain.config().clone(),
        chain.config().anchor()
    )
    .is_err());
    for mutation in 0..7 {
        let mut cfg = chain.config().clone();
        match mutation {
            0 => cfg.economics.network = TradeLayerMainnet::Dogecoin,
            1 => cfg.admin = a("otheradmin"),
            2 => cfg.permitted_spot_properties.clear(),
            3 => cfg.rollback_limit += 1,
            4 => cfg.economics.notional = d("2"),
            5 => cfg.economics.fee_route = FeeRoute::Oracle(1),
            _ => cfg.economics.initial_margin_per_contract = d("11"),
        }
        assert!(
            EconomicChain::from_checkpoint_bytes(&bytes, cfg, chain.tip()).is_err(),
            "config mutation {mutation}"
        );
    }
    for n in [0, 4, 5, 38, 39, 118, bytes.len() - 1] {
        assert!(EconomicChain::from_checkpoint_bytes(
            &bytes[..n],
            chain.config().clone(),
            chain.tip()
        )
        .is_err());
    }
    // Repair the outer CRC so this reaches actual retained-frame linkage checks.
    let state_len = u32::from_be_bytes(bytes[119..123].try_into().unwrap()) as usize;
    let first_header = 123 + state_len + 1;
    let mut broken_parent = bytes.clone();
    broken_parent[first_header + 40] ^= 1;
    assert!(EconomicChain::from_checkpoint_bytes(
        &checksum(broken_parent),
        chain.config().clone(),
        chain.tip()
    )
    .is_err());
}

#[test]
fn repaired_crc_cannot_relabel_positive_height_floor_as_genesis() {
    let mut chain = with_claim(TradeLayerMainnet::Litecoin, 1);
    chain.rollback_tip(chain.tip()).unwrap();
    assert_eq!(chain.rollback_depth(), 0);
    assert!(chain.rollback_floor().height > 0);
    let mut bytes = chain.checkpoint_bytes().unwrap();
    let genesis = chain.config().anchor().id;
    bytes[47..79].copy_from_slice(&genesis.0);
    bytes[87..119].copy_from_slice(&genesis.0);
    let expected = Tip {
        height: chain.tip().height,
        id: genesis,
    };
    assert!(EconomicChain::from_checkpoint_bytes(
        &checksum(bytes),
        chain.config().clone(),
        expected
    )
    .is_err());
}

#[test]
fn durable_restart_multirollback_and_same_tip_new_sequence_preserve_full_economics() {
    let dir = Directory::new();
    let cfg = config(TradeLayerMainnet::Litecoin, 4);
    let chain = initial(cfg.economics.network, cfg.rollback_limit);
    let mut owner = DurableEconomicChain::initialize(&dir.0, chain, cfg.anchor()).unwrap();
    let mut head = owner.head().unwrap();
    assert_eq!(head.sequence, 1);
    for (marker, inputs) in [
        (11, vec![activate()]),
        (22, opening()),
        (33, deferred_claim()),
    ] {
        let next = block(owner.chain().unwrap(), marker, inputs);
        head = owner.apply_unverified_block(head, &next).unwrap();
    }
    let claim_head = head;
    let claim_chain = owner.chain().unwrap().clone();
    let file = owner.path();
    drop(owner);
    let bytes = fs::read(&file).unwrap();
    for mutation in 0..3 {
        let mut wrong = head;
        match mutation {
            0 => wrong.sequence += 1,
            1 => wrong.commitment[0] ^= 1,
            _ => wrong.tip.id = BlockId([0x99; 32]),
        }
        assert!(DurableEconomicChain::load(
            &dir.0,
            cfg.clone(),
            wrong,
            Recovery::RejectIncompleteTail
        )
        .is_err());
        assert_eq!(fs::read(&file).unwrap(), bytes);
    }
    let mut wrong_config = cfg.clone();
    wrong_config.admin = a("otheradmin");
    assert!(
        DurableEconomicChain::load(&dir.0, wrong_config, head, Recovery::RejectIncompleteTail)
            .is_err()
    );
    assert_eq!(fs::read(&file).unwrap(), bytes);
    let (owner, report) =
        DurableEconomicChain::load(&dir.0, cfg.clone(), head, Recovery::RejectIncompleteTail)
            .unwrap();
    assert_eq!(report.discarded_bytes, 0);
    assert_eq!(owner.chain().unwrap(), &claim_chain);
    drop(owner);
    assert_eq!(fs::read(&file).unwrap(), bytes);
    let (mut owner, _) =
        DurableEconomicChain::load(&dir.0, cfg.clone(), head, Recovery::RejectIncompleteTail)
            .unwrap();
    assert_unpaid_claim(owner.chain().unwrap());
    let payout = block(owner.chain().unwrap(), 44, funded_payout());
    head = owner.apply_unverified_block(head, &payout).unwrap();
    assert_eq!(
        owner.chain().unwrap().state().receipts()[2].paid_claim_atoms,
        5 * ATOMS
    );
    head = owner.rollback_to(head, claim_head.tip).unwrap();
    assert_eq!(owner.chain().unwrap(), &claim_chain);
    assert_eq!(head.tip, claim_head.tip);
    assert_eq!(head.commitment, claim_head.commitment);
    assert!(head.sequence > claim_head.sequence);
    drop(owner);
    let before = fs::read(&file).unwrap();
    let (mut owner, _) =
        DurableEconomicChain::load(&dir.0, cfg.clone(), head, Recovery::RejectIncompleteTail)
            .unwrap();
    assert!(matches!(
        owner.apply_unverified_block(claim_head, &payout),
        Err(DurableEconomicError::StaleHead)
    ));
    assert_eq!(owner.head().unwrap(), head);
    drop(owner);
    assert_eq!(fs::read(&file).unwrap(), before);
    assert!(matches!(
        DurableEconomicChain::load(
            &dir.0,
            cfg.clone(),
            claim_head,
            Recovery::RejectIncompleteTail
        ),
        Err(DurableEconomicError::StaleHead)
    ));
    let (mut owner, _) =
        DurableEconomicChain::load(&dir.0, cfg, head, Recovery::RejectIncompleteTail).unwrap();
    let branch = block(owner.chain().unwrap(), 66, funded_payout());
    let branch_head = owner.apply_unverified_block(head, &branch).unwrap();
    assert_ne!(branch_head.tip.id, payout.header.id);
    assert_eq!(
        owner.chain().unwrap().state().receipts()[2].paid_claim_atoms,
        5 * ATOMS
    );
    owner
        .chain()
        .unwrap()
        .state()
        .ledger()
        .verify_conservation()
        .unwrap();
}

#[test]
fn durable_oldest_floor_restart_branch_and_failed_deeper_rollback_are_atomic() {
    let dir = Directory::new();
    let mut chain = with_claim(TradeLayerMainnet::Litecoin, 2);
    let cfg = chain.config().clone();
    let floor = chain.tip();
    chain
        .apply_unverified_block(&block(&chain, 44, funded_payout()))
        .unwrap();
    chain
        .apply_unverified_block(&block(&chain, 55, vec![]))
        .unwrap();
    let owner = DurableEconomicChain::initialize(&dir.0, chain.clone(), chain.tip()).unwrap();
    let head = owner.head().unwrap();
    let path = owner.path();
    drop(owner);
    let before = fs::read(&path).unwrap();
    let (mut owner, _) =
        DurableEconomicChain::load(&dir.0, cfg.clone(), head, Recovery::RejectIncompleteTail)
            .unwrap();
    assert!(owner
        .rollback_to(
            head,
            Tip {
                height: floor.height - 1,
                id: BlockId([22; 32])
            }
        )
        .is_err());
    assert_eq!(owner.head().unwrap(), head);
    assert_eq!(owner.chain().unwrap(), &chain);
    drop(owner);
    assert_eq!(fs::read(&path).unwrap(), before);
    let (mut owner, _) =
        DurableEconomicChain::load(&dir.0, cfg.clone(), head, Recovery::RejectIncompleteTail)
            .unwrap();
    let head = owner.rollback_to(head, floor).unwrap();
    assert_eq!(owner.chain().unwrap().rollback_depth(), 0);
    assert_unpaid_claim(owner.chain().unwrap());
    drop(owner);
    let (mut owner, _) =
        DurableEconomicChain::load(&dir.0, cfg, head, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(owner.chain().unwrap().rollback_floor(), floor);
    assert_eq!(owner.chain().unwrap().rollback_depth(), 0);
    let branch = block(owner.chain().unwrap(), 66, funded_payout());
    owner.apply_unverified_block(head, &branch).unwrap();
    assert_eq!(
        owner.chain().unwrap().state().receipts()[2].paid_claim_atoms,
        5 * ATOMS
    );
}

#[test]
fn durable_incomplete_tail_requires_explicit_recovery_and_preserves_expected_head() {
    let dir = Directory::new();
    let chain = with_claim(TradeLayerMainnet::Litecoin, 4);
    let cfg = chain.config().clone();
    let owner = DurableEconomicChain::initialize(&dir.0, chain.clone(), chain.tip()).unwrap();
    let head = owner.head().unwrap();
    let path = owner.path();
    drop(owner);
    let bytes = fs::read(&path).unwrap();
    let mut append = OpenOptions::new().append(true).open(&path).unwrap();
    append.write_all(b"partial").unwrap();
    append.sync_all().unwrap();
    drop(append);
    assert!(matches!(
        DurableEconomicChain::load(&dir.0, cfg.clone(), head, Recovery::RejectIncompleteTail),
        Err(DurableEconomicError::Storage(
            StorageError::IncompleteTail { .. }
        ))
    ));
    assert_eq!(fs::metadata(&path).unwrap().len(), bytes.len() as u64 + 7);
    let (owner, report) =
        DurableEconomicChain::load(&dir.0, cfg, head, Recovery::DiscardIncompleteTail).unwrap();
    assert_eq!(report.discarded_bytes, 7);
    assert_eq!(owner.head().unwrap(), head);
    assert_eq!(owner.chain().unwrap(), &chain);
    drop(owner);
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn durable_os_lock_excludes_second_process_and_release_allows_reopen() {
    let dir = Directory::new();
    let chain = with_claim(TradeLayerMainnet::Litecoin, 4);
    let cfg = chain.config().clone();
    let owner = DurableEconomicChain::initialize(&dir.0, chain.clone(), chain.tip()).unwrap();
    let head = owner.head().unwrap();
    assert!(matches!(
        DurableEconomicChain::load(&dir.0, cfg.clone(), head, Recovery::RejectIncompleteTail),
        Err(DurableEconomicError::Storage(StorageError::Locked))
    ));
    let probe = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("economic_lock_probe_child")
        .arg("--nocapture")
        .env(LOCK_PROBE_ROOT, &dir.0)
        .output()
        .unwrap();
    assert!(
        probe.status.success(),
        "child stdout={} stderr={}",
        String::from_utf8_lossy(&probe.stdout),
        String::from_utf8_lossy(&probe.stderr)
    );
    assert!(String::from_utf8_lossy(&probe.stdout).contains("running 1 test"));
    assert_eq!(owner.head().unwrap(), head);
    drop(owner);
    let (owner, _) =
        DurableEconomicChain::load(&dir.0, cfg, head, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(owner.chain().unwrap(), &chain);
}

#[test]
fn economic_lock_probe_child() {
    let Some(root) = std::env::var_os(LOCK_PROBE_ROOT) else {
        return;
    };
    let result = Journal::open_existing(
        &PathBuf::from(root),
        Namespace::for_mainnet(TradeLayerMainnet::Litecoin, "clearing").unwrap(),
        Recovery::RejectIncompleteTail,
    );
    assert!(matches!(result, Err(StorageError::Locked)));
}
