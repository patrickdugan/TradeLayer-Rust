//! The rollback window keeps one state at the floor and the inputs of each
//! retained block. These tests compare every rebuilt state with the state that
//! the same chain held when it applied the block (the state the earlier layout
//! stored as a frame), load a version 1 checkpoint, and run the chain far past
//! the point where one full state per frame filled the checkpoint.
use std::collections::BTreeSet;
use std::time::Instant;
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::contract_orderbook::{CancelCriteria, Order, Side};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{EconomicConfig, EconomicError, EconomicInput, SettlementPolicy};
use tradelayer_core::economic_chain::{
    EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::joint::{JointOperation, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::tally::FeeRoute;

const ATOMS: u128 = 100_000_000;
const TRADERS: u32 = 4;

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
fn id(number: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&number.to_be_bytes());
    TxId::from_bytes(bytes)
}
fn block_id(number: u64) -> BlockId {
    let mut bytes = [0xb1; 32];
    bytes[24..].copy_from_slice(&number.to_be_bytes());
    BlockId(bytes)
}

fn config(limit: usize) -> EconomicChainConfig {
    EconomicChainConfig {
        economics: EconomicConfig {
            native_entitlements: None,
            insurance: None,
            source_consensus: None,
            contract_binding: None,
            network: TradeLayerMainnet::Litecoin,
            anchor: EconomicConfig::genesis_anchor(TradeLayerMainnet::Litecoin),
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
        rollback_limit: limit,
    }
}

fn ledger() -> Ledger {
    let mut ledger = Ledger::new();
    let holders: Vec<(String, u128)> = std::iter::once(("mm".to_owned(), 5_000_000))
        .chain((0..TRADERS).map(|t| (format!("t{t}"), 500_000)))
        .collect();
    let total: u128 = holders.iter().map(|(_, tokens)| tokens).sum::<u128>() + 10;
    ledger
        .apply(
            id(1),
            Operation::CreateProperty {
                id: p(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: total * ATOMS,
            },
        )
        .unwrap();
    for (n, (owner, tokens)) in holders.iter().enumerate() {
        ledger
            .apply(
                id(2 + n as u64),
                Operation::Transfer {
                    id: p(),
                    from: a("alice"),
                    to: a(owner),
                    amount_atoms: tokens * ATOMS,
                },
            )
            .unwrap();
    }
    ledger
}

/// mulberry32, as in the wasm playground benchmark.
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

fn cents(c: u64) -> String {
    format!("{}.{:02}", c / 100, c % 100)
}

fn order(
    next: &mut u64,
    owner: &str,
    side: Side,
    qty: &str,
    price: &str,
    height: u64,
) -> EconomicInput {
    *next += 1;
    EconomicInput::Order(Order {
        txid: id(*next),
        owner: a(owner),
        contract: c(),
        height,
        side,
        quantity: d(qty),
        price: d(price),
        post: false,
        is_market: false,
    })
}

/// The schedule of the playground benchmark: the market maker cancels and
/// quotes a three-level ladder on each side, and a taker order follows with
/// probability 0.6.
struct Schedule {
    rng: Rng,
    price: f64,
    next_id: u64,
}
impl Schedule {
    fn new(seed: u32) -> Self {
        Self {
            rng: Rng(seed),
            price: 100.0,
            next_id: 1000,
        }
    }
    fn activation(&mut self) -> Vec<EconomicInput> {
        vec![EconomicInput::Joint(UnverifiedJointTx {
            txid: id(10),
            actor: a("admin"),
            operation: JointOperation::Registry(RegistryAction::Activate {
                types: vec![2, 6, 18],
                code_hash: [0x71; 32],
            }),
        })]
    }
    fn inputs(&mut self, height: u64) -> Vec<EconomicInput> {
        self.price *= 1.0 + (self.rng.next() - 0.5) * 2.0 * 0.001;
        let mid = (self.price * 100.0 + 0.5).floor() as u64;
        self.next_id += 1;
        let mut inputs = vec![EconomicInput::Cancel {
            txid: id(self.next_id),
            owner: a("mm"),
            criteria: CancelCriteria::All,
        }];
        for level in 0..3 {
            let off = mid * (5 + 5 * level) / 10_000;
            inputs.push(order(
                &mut self.next_id,
                "mm",
                Side::Buy,
                "0.5",
                &cents(mid - off),
                height,
            ));
            inputs.push(order(
                &mut self.next_id,
                "mm",
                Side::Sell,
                "0.5",
                &cents(mid + off),
                height,
            ));
        }
        if self.rng.next() < 0.6 {
            let trader = format!("t{}", (self.rng.next() * f64::from(TRADERS)).floor() as u32);
            let buy = self.rng.next() < 0.5;
            let qty = format!("0.{}", 1 + (self.rng.next() * 3.0).floor() as u64);
            let (side, px) = if buy {
                (Side::Buy, mid * 10_020 / 10_000)
            } else {
                (Side::Sell, mid * 9_980 / 10_000)
            };
            inputs.push(order(
                &mut self.next_id,
                &trader,
                side,
                &qty,
                &cents(px),
                height,
            ));
        }
        inputs
    }
}

fn block(
    chain: &EconomicChain,
    ids: &mut u64,
    inputs: Vec<EconomicInput>,
) -> UnverifiedEconomicBlock {
    *ids += 1;
    UnverifiedEconomicBlock {
        network: chain.config().economics.network,
        header: BlockHeader {
            height: chain.tip().height + 1,
            id: block_id(*ids),
            parent: chain.tip().id,
        },
        inputs,
    }
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

fn embedded(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

/// The version 1 checkpoint as the earlier layout wrote it: the state at the
/// tip, then one stored before-state per retained block.
fn version_1_bytes(
    chain: &EconomicChain,
    headers: &[BlockHeader],
    stored: &std::collections::BTreeMap<u64, Vec<u8>>,
) -> Vec<u8> {
    let config = chain.config();
    let mut out = b"TLEH\x01".to_vec();
    out.push(config.economics.network.tag());
    out.extend_from_slice(&config.economics.network.genesis().0);
    out.push(config.rollback_limit as u8);
    for tip in [chain.rollback_floor(), chain.tip()] {
        out.extend_from_slice(&tip.height.to_be_bytes());
        out.extend_from_slice(&tip.id.0);
    }
    embedded(&mut out, &chain.state().snapshot_bytes().unwrap());
    out.push(chain.rollback_depth() as u8);
    for header in headers {
        out.extend_from_slice(&header.height.to_be_bytes());
        out.extend_from_slice(&header.id.0);
        out.extend_from_slice(&header.parent.0);
        embedded(&mut out, &stored[&(header.height - 1)]);
    }
    let crc = crc32(&out);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

/// A chain and the model of what the earlier layout would have stored.
struct Model {
    chain: EconomicChain,
    schedule: Schedule,
    ids: u64,
    /// State snapshot at each height of the current branch, taken when the
    /// chain applied the block, as the earlier layout took each frame.
    stored: std::collections::BTreeMap<u64, Vec<u8>>,
    headers: Vec<BlockHeader>,
}
impl Model {
    fn new(limit: usize, seed: u32) -> Self {
        let chain = EconomicChain::new_unverified(config(limit), ledger()).unwrap();
        let mut stored = std::collections::BTreeMap::new();
        stored.insert(0, chain.state().snapshot_bytes().unwrap());
        Self {
            chain,
            schedule: Schedule::new(seed),
            ids: 0,
            stored,
            headers: Vec::new(),
        }
    }
    fn apply(&mut self) {
        let height = self.chain.tip().height + 1;
        let inputs = if height == 1 {
            self.schedule.activation()
        } else {
            self.schedule.inputs(height)
        };
        let block = block(&self.chain, &mut self.ids, inputs);
        self.chain.apply_unverified_block(&block).unwrap();
        self.stored
            .insert(height, self.chain.state().snapshot_bytes().unwrap());
        self.headers.push(block.header);
        let keep = self.chain.rollback_depth();
        while self.headers.len() > keep {
            self.headers.remove(0);
        }
    }
    fn check(&self) {
        let height = self.chain.tip().height;
        assert_eq!(
            self.chain.state().snapshot_bytes().unwrap(),
            self.stored[&height],
            "state at {height}"
        );
        assert_eq!(self.headers.len(), self.chain.rollback_depth());
        let v1 = version_1_bytes(&self.chain, &self.headers, &self.stored);
        assert_eq!(
            self.chain.checkpoint_bytes_v1().unwrap(),
            v1,
            "the rebuilt frames are the stored frames at {height}"
        );
    }
    fn rollback_to(&mut self, target: u64) {
        let tip = if target == self.chain.rollback_floor().height {
            self.chain.rollback_floor()
        } else {
            let header = self
                .headers
                .iter()
                .find(|h| h.height == target)
                .expect("retained");
            Tip {
                height: header.height,
                id: header.id,
            }
        };
        self.chain.rollback_to_tip(tip).unwrap();
        self.headers.retain(|h| h.height <= target);
        self.stored.retain(|height, _| *height <= target);
    }
}

#[test]
fn rebuilt_frames_equal_the_stored_frames_over_random_histories() {
    for (limit, seed) in [(64_usize, 1_u32), (5, 7), (1, 3)] {
        let mut model = Model::new(limit, seed);
        let mut rng = Rng(seed ^ 0x9e37);
        let mut rollbacks = 0;
        let mut applied = 0;
        while applied < 200 {
            let heavy = if model.chain.rollback_depth() > 0 && rng.next() < 0.06 {
                let depth = model.chain.rollback_depth() as u64;
                let by = 1 + (rng.next().powi(3) * depth as f64).floor() as u64;
                let target = model.chain.tip().height - by.min(depth);
                model.rollback_to(target);
                rollbacks += 1;
                rollbacks % 4 == 1
            } else {
                model.apply();
                applied += 1;
                applied % 50 == 0
            };
            if heavy {
                model.check();
                let bytes = model.chain.checkpoint_bytes().unwrap();
                let reloaded = EconomicChain::from_checkpoint_bytes(
                    &bytes,
                    model.chain.config().clone(),
                    model.chain.tip(),
                )
                .unwrap();
                assert_eq!(reloaded, model.chain);
                assert_eq!(reloaded.checkpoint_bytes().unwrap(), bytes);
            }
            assert_eq!(
                model.chain.state().snapshot_bytes().unwrap(),
                model.stored[&model.chain.tip().height]
            );
        }
        model.check();
        assert!(rollbacks >= 5, "limit {limit}: {rollbacks} rollbacks");
        assert!(model.chain.tip().height > 30);
    }
}

#[test]
fn single_step_rollback_walks_the_whole_window() {
    let mut model = Model::new(8, 11);
    for _ in 0..30 {
        model.apply();
    }
    for _ in 0..8 {
        let tip = model.chain.tip();
        model.chain.rollback_tip(tip).unwrap();
        model.headers.pop();
        model.stored.remove(&tip.height);
        model.check();
    }
    assert_eq!(model.chain.rollback_depth(), 0);
    assert!(model.chain.rollback_tip(model.chain.tip()).is_err());
    // The branch continues from the floor.
    for _ in 0..12 {
        model.apply();
    }
    model.check();
}

#[test]
fn version_1_checkpoints_load_roll_back_and_migrate() {
    let mut model = Model::new(6, 5);
    for _ in 0..40 {
        model.apply();
    }
    let v1 = version_1_bytes(&model.chain, &model.headers, &model.stored);
    assert_eq!(model.chain.checkpoint_bytes_v1().unwrap(), v1);
    let v2 = model.chain.checkpoint_bytes().unwrap();
    assert!(v2.starts_with(b"TLEH\x02") && v1.starts_with(b"TLEH\x01"));
    assert!(v2.len() < v1.len());

    let loaded =
        EconomicChain::from_checkpoint_bytes(&v1, model.chain.config().clone(), model.chain.tip())
            .unwrap();
    assert_eq!(loaded.state(), model.chain.state());
    assert_eq!(loaded.tip(), model.chain.tip());
    assert_eq!(loaded.rollback_floor(), model.chain.rollback_floor());
    assert_eq!(loaded.rollback_depth(), 6);
    // The loaded chain still writes both layouts, byte for byte.
    assert_eq!(loaded.checkpoint_bytes_v1().unwrap(), v1);
    assert_eq!(
        EconomicChain::from_checkpoint_bytes(
            &loaded.checkpoint_bytes().unwrap(),
            loaded.config().clone(),
            loaded.tip()
        )
        .unwrap(),
        loaded
    );
    // A tampered version 1 checkpoint is refused.
    let mut broken = v1.clone();
    let at = broken.len() / 2;
    broken[at] ^= 1;
    let n = broken.len() - 4;
    let crc = crc32(&broken[..n]);
    broken[n..].copy_from_slice(&crc.to_be_bytes());
    assert!(EconomicChain::from_checkpoint_bytes(
        &broken,
        model.chain.config().clone(),
        model.chain.tip()
    )
    .is_err());

    // Apply past the migrated frames, then roll back across the seam.
    model.chain = loaded.clone();
    for _ in 0..10 {
        model.apply();
        model.check();
    }
    for target in [
        model.chain.tip().height - 3,
        model.chain.tip().height - 5,
        model.chain.rollback_floor().height,
    ] {
        let mut branch = Model {
            chain: model.chain.clone(),
            schedule: Schedule {
                next_id: 1_000_000,
                ..Schedule::new(99)
            },
            ids: 10_000,
            stored: model.stored.clone(),
            headers: model.headers.clone(),
        };
        branch.rollback_to(target);
        branch.check();
        for _ in 0..4 {
            branch.apply();
        }
        branch.check();
    }
    // Migration while the seam is inside the window, and after it left.
    let again = EconomicChain::from_checkpoint_bytes(
        &model.chain.checkpoint_bytes().unwrap(),
        model.chain.config().clone(),
        model.chain.tip(),
    )
    .unwrap();
    assert_eq!(again, model.chain);
    for _ in 0..8 {
        model.apply();
    }
    model.check();
}

#[test]
fn damaged_windows_are_refused() {
    let mut model = Model::new(4, 21);
    for _ in 0..12 {
        model.apply();
    }
    let good = model.chain.checkpoint_bytes().unwrap();
    let load = |bytes: &[u8]| {
        EconomicChain::from_checkpoint_bytes(bytes, model.chain.config().clone(), model.chain.tip())
    };
    assert!(load(&good).is_ok());
    // Flip one byte at many offsets and repair the checksum: nothing loads a
    // window that is not the replay of its inputs, unless the byte is inert.
    let mut refused = 0;
    for at in (100..good.len() - 4).step_by(good.len() / 97 + 1) {
        let mut bad = good.clone();
        bad[at] ^= 0x04;
        let n = bad.len() - 4;
        let crc = crc32(&bad[..n]);
        bad[n..].copy_from_slice(&crc.to_be_bytes());
        match load(&bad) {
            Err(_) => refused += 1,
            Ok(chain) => assert_eq!(chain, model.chain, "offset {at}"),
        }
    }
    assert!(refused > 50, "{refused}");
    assert_eq!(
        load(&good[..good.len() - 1]).unwrap_err(),
        EconomicError::InvalidSnapshot
    );
}

/// The window past the point where one state per frame halted the chain.
#[test]
#[cfg_attr(debug_assertions, ignore = "release builds only")]
fn five_thousand_blocks_at_rollback_limit_64_never_hit_capacity() {
    let blocks: u64 = std::env::var("WINDOW_BLOCKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5_000);
    let mut chain = EconomicChain::new_unverified(config(64), ledger()).unwrap();
    let mut schedule = Schedule::new(1);
    let mut ids = 0;
    let mut checkpoint_len = 0;
    let started = Instant::now();
    let mut last = started;
    for height in 1..=blocks {
        let inputs = if height == 1 {
            schedule.activation()
        } else {
            schedule.inputs(height)
        };
        let next = block(&chain, &mut ids, inputs);
        match chain.apply_unverified_block(&next) {
            Ok(_) => {}
            Err(error) => panic!("block {height}: {error:?}"),
        }
        if height % 500 == 0 || height == blocks {
            let state_bytes = chain.state().snapshot_bytes().unwrap().len();
            checkpoint_len = chain.checkpoint_bytes().unwrap().len();
            println!(
                "height {height}: state {state_bytes} B ({} B/block), checkpoint {checkpoint_len} B, \
                 {:.1} ms/block over the last {} blocks",
                state_bytes as u64 / height,
                last.elapsed().as_secs_f64() * 1000.0 / 500.0,
                if height % 500 == 0 { 500 } else { height % 500 },
            );
            last = Instant::now();
        }
    }
    println!(
        "{blocks} blocks in {:.1} s",
        started.elapsed().as_secs_f64()
    );
    // One rollback of the deepest retained block, measured.
    let before = chain.clone();
    let target = chain.rollback_floor();
    let timer = Instant::now();
    chain.rollback_to_tip(target).unwrap();
    println!(
        "rollback of 64 blocks at height {blocks}: {:.1} ms",
        timer.elapsed().as_secs_f64() * 1000.0
    );
    assert_eq!(chain.tip(), target);
    let timer = Instant::now();
    let mut single = before.clone();
    single.rollback_tip(single.tip()).unwrap();
    println!(
        "rollback of 1 block at height {blocks}: {:.1} ms",
        timer.elapsed().as_secs_f64() * 1000.0
    );
    assert!(checkpoint_len < 64 * 1024 * 1024);
    assert!(blocks > 908);
}
