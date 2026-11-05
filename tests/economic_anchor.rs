//! The economic chain starts at a configured anchor tip and sees absolute
//! block heights. All chain data and actors are unverified.
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    EconomicConfig, EconomicError, EconomicInput, EconomicState, SettlementPolicy,
};
use tradelayer_core::economic_chain::{
    DurableEconomicChain, DurableEconomicError, EconomicChain, EconomicChainConfig,
    UnverifiedEconomicBlock,
};
use tradelayer_core::joint::{JointOperation, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::storage::Recovery;
use tradelayer_core::tally::FeeRoute;

const ATOMS: u128 = 100_000_000;
const H: u64 = 40;
const NETWORK: TradeLayerMainnet = TradeLayerMainnet::Bitcoin;
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-economic-anchor-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
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
fn anchor() -> Tip {
    Tip {
        height: H,
        id: BlockId([0xA0; 32]),
    }
}
fn config(anchor: Tip, rollback_limit: usize) -> EconomicChainConfig {
    EconomicChainConfig {
        economics: EconomicConfig {
            native_entitlements: None,
            insurance: None,
            source_consensus: None,
            contract_binding: None,
            network: NETWORK,
            anchor,
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
fn genesis_tip() -> Tip {
    EconomicConfig::genesis_anchor(NETWORK)
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
fn chain_at(anchor: Tip, limit: usize) -> EconomicChain {
    EconomicChain::new_unverified(config(anchor, limit), ledger()).unwrap()
}
/// The id of the filler block at `height`; never the anchor's id.
fn filler_id(height: u64) -> BlockId {
    let mut bytes = [0xF0; 32];
    bytes[..8].copy_from_slice(&height.to_be_bytes());
    BlockId(bytes)
}
/// A genesis-anchored chain that has applied `H` empty blocks, so its next
/// block is at the same absolute height as the first block of an H-anchored one.
fn genesis_chain_at_h(limit: usize) -> EconomicChain {
    let mut chain = chain_at(genesis_tip(), limit);
    for height in 1..=H {
        chain
            .apply_unverified_block(&UnverifiedEconomicBlock {
                network: NETWORK,
                header: BlockHeader {
                    height,
                    id: filler_id(height),
                    parent: chain.tip().id,
                },
                inputs: Vec::new(),
            })
            .unwrap();
    }
    assert_eq!(chain.tip().height, H);
    chain
}
fn block_with_id(
    chain: &EconomicChain,
    id: BlockId,
    inputs: Vec<EconomicInput>,
) -> UnverifiedEconomicBlock {
    UnverifiedEconomicBlock {
        network: NETWORK,
        header: BlockHeader {
            height: chain.tip().height + 1,
            id,
            parent: chain.tip().id,
        },
        inputs,
    }
}
fn block(chain: &EconomicChain, marker: u8, inputs: Vec<EconomicInput>) -> UnverifiedEconomicBlock {
    block_with_id(chain, BlockId([marker; 32]), inputs)
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
fn send() -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(11),
        actor: a("bob"),
        operation: JointOperation::Send {
            id: p(),
            recipient: a("carol"),
            amount_atoms: 5 * ATOMS,
        },
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
fn pair(
    first: u8,
    buyer: &str,
    seller: &str,
    quantity: &str,
    price: &str,
    height: u64,
) -> Vec<EconomicInput> {
    vec![
        order(first, buyer, Side::Buy, quantity, price, height),
        order(first + 1, seller, Side::Sell, quantity, price, height),
    ]
}

/// FNV-1a over `bytes`, with the length: a stable fingerprint for a pinned
/// byte layout.
fn fingerprint(bytes: &[u8]) -> (usize, u64) {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    (bytes.len(), hash)
}

/// The tests apply this sequence to both chains. `height` is absolute.
fn script(first: u64) -> Vec<(u8, Vec<EconomicInput>)> {
    vec![
        (0x11, vec![activate()]),
        (0x22, vec![send()]),
        (0x33, pair(20, "alice", "bob", "1", "100", first + 3)),
        (0x44, pair(30, "carol", "alice", "0.5", "110", first + 4)),
        (0x55, pair(40, "bob", "dave", "0.5", "110", first + 5)),
        (0x66, Vec::new()),
    ]
}

#[test]
fn an_anchored_chain_matches_a_genesis_chain_at_the_same_absolute_heights() {
    let mut anchored = chain_at(anchor(), 8);
    let mut genesis = genesis_chain_at_h(8);
    assert_eq!(anchored.tip(), anchor());
    assert_eq!(anchored.rollback_floor(), anchor());
    assert_eq!(anchored.state().anchor(), anchor());
    assert_eq!(anchored.state().finalized_height(), None);
    assert_eq!(genesis.state().finalized_height(), Some(H));

    // The activation record names the absolute height, and activation takes
    // effect after its block: a send in the activation block is refused, at
    // the absolute height, by both chains alike.
    for chain in [&mut anchored, &mut genesis] {
        let before = chain.clone();
        let same_block = block(chain, 0x10, vec![activate(), send()]);
        let error = chain.apply_unverified_block(&same_block).unwrap_err();
        assert!(matches!(error, EconomicError::Joint(_)), "{error:?}");
        assert_eq!(*chain, before);
    }

    for (marker, inputs) in script(H) {
        for chain in [&mut anchored, &mut genesis] {
            let next = block(chain, marker, inputs.clone());
            assert_eq!(next.header.height, chain.tip().height + 1);
            chain.apply_unverified_block(&next).unwrap();
        }
        assert_eq!(anchored.tip().height, genesis.tip().height);
        assert_eq!(
            anchored.state().finalized_height(),
            genesis.state().finalized_height()
        );
        assert_eq!(anchored.state().ledger(), genesis.state().ledger());
        assert_eq!(anchored.state().receipts(), genesis.state().receipts());
        for who in ["alice", "bob", "carol", "dave"] {
            assert_eq!(
                anchored.state().margin_atoms(&a(who)),
                genesis.state().margin_atoms(&a(who))
            );
            assert_eq!(
                anchored
                    .state()
                    .positions()
                    .get(&a(who))
                    .map(|p| p.contracts.clone()),
                genesis
                    .state()
                    .positions()
                    .get(&a(who))
                    .map(|p| p.contracts.clone())
            );
        }
        assert_eq!(
            anchored.state().loss_pool_atoms(),
            genesis.state().loss_pool_atoms()
        );
        anchored.state().verify().unwrap();
    }
    assert_eq!(anchored.tip().height, H + 6);
    for chain in [&anchored, &genesis] {
        let activation = chain.state().joint().registry().activation(2).unwrap();
        assert_eq!(activation.height, H + 1);
        assert_eq!(chain.state().receipts().len(), 3);
        assert_eq!(chain.state().receipts()[0].height, H + 3);
        assert_eq!(chain.state().receipts()[2].height, H + 5);
    }
    // The anchor is part of the owner: the two states differ only by it.
    assert_ne!(anchored.state(), genesis.state());
    assert_eq!(anchored.state().anchor(), anchor());
    assert_eq!(genesis.state().anchor(), genesis_tip());
}

#[test]
fn the_first_block_extends_the_anchor_at_the_next_absolute_height() {
    let chain = chain_at(anchor(), 4);
    let valid = block(&chain, 0x11, vec![activate()]);
    assert_eq!(valid.header.height, H + 1);
    assert_eq!(valid.header.parent, anchor().id);
    for mutation in 0..5 {
        let mut chain = chain.clone();
        let mut bad = valid.clone();
        match mutation {
            0 => bad.header.height = 1,
            1 => bad.header.height = H,
            2 => bad.header.height = H + 2,
            3 => bad.header.parent = genesis_tip().id,
            _ => bad.header.id = anchor().id,
        }
        assert!(chain.apply_unverified_block(&bad).is_err(), "{mutation}");
        assert_eq!(chain.tip(), anchor());
    }
    let mut chain = chain;
    chain.apply_unverified_block(&valid).unwrap();
    assert_eq!(chain.tip().height, H + 1);
}

#[test]
fn an_order_stamped_with_a_relative_height_is_refused_and_an_absolute_one_fills() {
    let mut chain = chain_at(anchor(), 4);
    chain
        .apply_unverified_block(&block(&chain, 0x11, vec![activate()]))
        .unwrap();
    let before = chain.clone();
    let relative = block(&chain, 0x22, pair(20, "alice", "bob", "1", "100", 2));
    assert!(chain.apply_unverified_block(&relative).is_err());
    assert_eq!(chain, before);
    let absolute = block(&chain, 0x22, pair(20, "alice", "bob", "1", "100", H + 2));
    chain.apply_unverified_block(&absolute).unwrap();
    assert_eq!(chain.state().receipts().len(), 1);
    assert_eq!(chain.state().receipts()[0].height, H + 2);
}

#[test]
fn rollback_stops_at_the_anchor() {
    let mut chain = chain_at(anchor(), 4);
    let fresh = chain.clone();
    for (marker, inputs) in script(H).into_iter().take(2) {
        chain
            .apply_unverified_block(&block(&chain, marker, inputs))
            .unwrap();
    }
    assert_eq!(chain.rollback_depth(), 2);
    chain.rollback_tip(chain.tip()).unwrap();
    let back = chain.rollback_tip(chain.tip()).unwrap();
    assert_eq!(back, anchor());
    assert_eq!(chain, fresh);
    // Nothing sits below the anchor: the anchor itself is not a block to undo.
    for target in [
        anchor(),
        Tip {
            height: H - 1,
            id: filler_id(H - 1),
        },
        genesis_tip(),
    ] {
        let mut crossing = chain.clone();
        assert!(matches!(
            crossing.rollback_tip(target),
            Err(EconomicError::WrongTip)
        ));
        assert_eq!(crossing, fresh);
    }

    // The same through the durable owner.
    let dir = Directory::new();
    let mut durable = DurableEconomicChain::initialize(&dir.0, fresh.clone(), anchor()).unwrap();
    for (marker, inputs) in script(H).into_iter().take(2) {
        let head = durable.head().unwrap();
        let next = block(durable.chain().unwrap(), marker, inputs);
        durable.apply_unverified_block(head, &next).unwrap();
    }
    let head = durable.head().unwrap();
    for below in [
        Tip {
            height: H - 1,
            id: filler_id(H - 1),
        },
        genesis_tip(),
    ] {
        assert!(durable.rollback_to(head, below).is_err());
        assert_eq!(durable.head().unwrap(), head);
        assert_eq!(durable.chain().unwrap().tip().height, H + 2);
    }
    let head = durable.rollback_to(head, anchor()).unwrap();
    assert_eq!(head.tip, anchor());
    assert_eq!(durable.chain().unwrap(), &fresh);
}

#[test]
fn rollback_limit_floors_stay_above_the_anchor() {
    let mut chain = chain_at(anchor(), 2);
    for (marker, inputs) in script(H).into_iter().take(4) {
        chain
            .apply_unverified_block(&block(&chain, marker, inputs))
            .unwrap();
    }
    assert_eq!(chain.tip().height, H + 4);
    assert_eq!(chain.rollback_floor().height, H + 2);
    chain.rollback_tip(chain.tip()).unwrap();
    chain.rollback_tip(chain.tip()).unwrap();
    assert_eq!(chain.tip(), chain.rollback_floor());
    assert!(chain.rollback_tip(chain.tip()).is_err());
    let bytes = chain.checkpoint_bytes().unwrap();
    let restored =
        EconomicChain::from_checkpoint_bytes(&bytes, chain.config().clone(), chain.tip()).unwrap();
    assert_eq!(restored, chain);
}

#[test]
fn a_checkpoint_and_snapshot_load_only_under_the_anchor_that_wrote_them() {
    let mut chain = chain_at(anchor(), 4);
    for (marker, inputs) in script(H).into_iter().take(3) {
        chain
            .apply_unverified_block(&block(&chain, marker, inputs))
            .unwrap();
    }
    let bytes = chain.checkpoint_bytes().unwrap();
    let restored =
        EconomicChain::from_checkpoint_bytes(&bytes, chain.config().clone(), chain.tip()).unwrap();
    assert_eq!(restored, chain);
    assert_eq!(restored.checkpoint_bytes().unwrap(), bytes);

    let other_anchors = [
        Tip {
            height: H,
            id: BlockId([0xA1; 32]),
        },
        Tip {
            height: H + 1,
            id: anchor().id,
        },
        Tip {
            height: H - 1,
            id: anchor().id,
        },
        genesis_tip(),
    ];
    for other in other_anchors {
        let mut cfg = chain.config().clone();
        cfg.economics.anchor = other;
        assert!(
            EconomicChain::from_checkpoint_bytes(&bytes, cfg.clone(), chain.tip()).is_err(),
            "{other:?}"
        );
        let state_bytes = chain.state().snapshot_bytes().unwrap();
        assert!(EconomicState::from_snapshot_bytes(
            &state_bytes,
            &cfg.economics,
            &cfg.admin,
            &cfg.permitted_spot_properties
        )
        .is_err());
    }
    let state_bytes = chain.state().snapshot_bytes().unwrap();
    let cfg = chain.config();
    let state = EconomicState::from_snapshot_bytes(
        &state_bytes,
        &cfg.economics,
        &cfg.admin,
        &cfg.permitted_spot_properties,
    )
    .unwrap();
    assert_eq!(&state, chain.state());

    // A genesis checkpoint does not load under an anchored configuration.
    let mut genesis = chain_at(genesis_tip(), 4);
    genesis
        .apply_unverified_block(&block(&genesis, 0x11, vec![activate()]))
        .unwrap();
    let genesis_bytes = genesis.checkpoint_bytes().unwrap();
    let mut cfg = genesis.config().clone();
    cfg.economics.anchor = anchor();
    assert!(EconomicChain::from_checkpoint_bytes(&genesis_bytes, cfg, genesis.tip()).is_err());

    // The durable owner refuses the same way.
    let dir = Directory::new();
    let owner = DurableEconomicChain::initialize(&dir.0, chain.clone(), chain.tip()).unwrap();
    let head = owner.head().unwrap();
    drop(owner);
    for other in other_anchors {
        let mut cfg = chain.config().clone();
        cfg.economics.anchor = other;
        let loaded = DurableEconomicChain::load(&dir.0, cfg, head, Recovery::RejectIncompleteTail);
        assert!(
            matches!(loaded, Err(DurableEconomicError::Economic(_))),
            "{other:?}"
        );
    }
    let (owner, _) = DurableEconomicChain::load(
        &dir.0,
        chain.config().clone(),
        head,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    assert_eq!(owner.chain().unwrap(), &chain);
}

#[test]
fn anchors_the_network_cannot_hold_are_refused() {
    let wrong_genesis = Tip {
        height: 0,
        id: BlockId([0xA0; 32]),
    };
    let mut cfg = config(wrong_genesis, 4);
    assert!(cfg.validate().is_err());
    assert!(EconomicChain::new_unverified(cfg.clone(), ledger()).is_err());
    cfg.economics.anchor = Tip {
        height: u64::MAX,
        id: BlockId([0xA0; 32]),
    };
    assert!(cfg.validate().is_err());
    cfg.economics.anchor = anchor();
    cfg.validate().unwrap();
    assert_eq!(cfg.anchor(), anchor());
}

#[test]
fn genesis_anchored_bytes_keep_their_layout() {
    let mut genesis = chain_at(genesis_tip(), 4);
    let anchored = chain_at(anchor(), 4);
    // The encoded configuration marks a non-genesis anchor on the network tag
    // byte and nothing else; a genesis anchor adds no byte.
    let genesis_state = genesis.state().snapshot_bytes().unwrap();
    let anchored_state = anchored.state().snapshot_bytes().unwrap();
    assert_eq!(&genesis_state[..5], b"TLEC\x01");
    assert_eq!(genesis_state[5], NETWORK.tag());
    assert_eq!(anchored_state[5], NETWORK.tag() | 0x80);
    assert_eq!(
        anchored_state.len(),
        genesis_state.len() + 8 + 32,
        "the anchor adds its height and id after the genesis hash"
    );
    for (marker, inputs) in script(0).into_iter().take(5) {
        genesis
            .apply_unverified_block(&block(&genesis, marker, inputs))
            .unwrap();
    }
    let checkpoint = genesis.checkpoint_bytes().unwrap();
    let state = genesis.state().snapshot_bytes().unwrap();
    // Pinned from the tree before the anchor existed.
    assert_eq!(
        (fingerprint(&checkpoint), fingerprint(&state)),
        GENESIS_FINGERPRINT,
        "{:?}",
        (fingerprint(&checkpoint), fingerprint(&state))
    );
}

const GENESIS_FINGERPRINT: ((usize, u64), (usize, u64)) =
    ((10998, 18014078772672211621), (3730, 9363959089641769727));
