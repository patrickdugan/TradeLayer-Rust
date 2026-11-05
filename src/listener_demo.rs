//! Labelled demo history for `tradelayer-listenerd --demo-seed`.
//!
//! DEMO DATA. Every block id, transaction id, actor and height below is a
//! synthetic caller assertion built through the public economic API. Nothing is
//! derived from, signed for or broadcast to any chain. The setup follows the
//! wasm playground: an issuer-managed token `tUSD` (property 7) collateralizes
//! one oracle-priced linear BTC/tUSD perpetual (contract 1) in fixed-margin
//! mode (no marks, no funding, no accepted-contract binding).
//!
//! Block 1 activates the opcodes. Block 2 creates the oracle, publishes the
//! first price, issues a second managed token (property 9, `Issue`) and grants
//! tUSD and property 9 (`Grant`). Block 3 rests the market maker's ladder.
//! Block 4 is one taker order that sweeps two ask levels: two fills and one
//! open trader position (`DemoAlice` long 2 against `DemoMM` short 2).

use crate::chain::{BlockHeader, BlockId};
use crate::contract_orderbook::{Order, Side};
use crate::contract_registry::ContractId;
use crate::economic::{EconomicConfig, EconomicInput, SettlementPolicy};
use crate::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, EconomicHead, UnverifiedEconomicBlock,
};
use crate::joint::{JointOperation, UnverifiedJointTx};
use crate::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use crate::network_profile::TradeLayerMainnet;
use crate::registry::RegistryAction;
use crate::tally::FeeRoute;
use std::collections::BTreeSet;
use std::path::Path;

pub const DEMO_LABEL: &str =
    "DEMO DATA: synthetic history built through the economic API; not derived from any chain";
const ATOMS: u128 = 100_000_000;
const TUSD: u32 = 7;
const SIDE_TOKEN: u32 = 9;
/// Oracle price of 100000.0000 in 1e-4 units.
const PRICE_1E4: u128 = 1_000_000_000;

fn account(name: &str) -> AccountId {
    AccountId::try_from(name).expect("demo account names are valid")
}
fn property(id: u32) -> PropertyId {
    PropertyId::try_from(id).expect("demo property ids are nonzero")
}
fn tx(number: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&number.to_be_bytes());
    TxId::from_bytes(bytes)
}
/// `64656d6f` ("demo") then the big-endian height: obviously synthetic.
fn block_id(height: u64) -> BlockId {
    let mut bytes = [0; 32];
    bytes[..4].copy_from_slice(b"demo");
    bytes[4..12].copy_from_slice(&height.to_be_bytes());
    BlockId(bytes)
}
fn joint(number: u64, actor: &str, operation: JointOperation) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: tx(number),
        actor: account(actor),
        operation,
    })
}

/// The config the demo store is built under and must be served under.
pub fn demo_config() -> EconomicChainConfig {
    EconomicChainConfig {
        economics: EconomicConfig {
            native_entitlements: None,
            insurance: None,
            source_consensus: None,
            network: TradeLayerMainnet::Bitcoin,
            anchor: EconomicConfig::genesis_anchor(TradeLayerMainnet::Bitcoin),
            contract: ContractId::try_from(1).expect("nonzero"),
            collateral: property(TUSD),
            inverse: false,
            notional: "1".parse().expect("decimal"),
            // 10x leverage at the first oracle price: 1 * 100000 / 10.
            initial_margin_per_contract: "10000.00".parse().expect("decimal"),
            fee_route: FeeRoute::Oracle(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
            marking: None,
            liquidation: None,
            amm: None,
            contract_binding: None,
        },
        admin: account("DemoAdmin"),
        permitted_spot_properties: BTreeSet::from([property(TUSD)]),
        rollback_limit: 16,
    }
}

pub struct DemoSeed {
    pub config: EconomicChainConfig,
    pub head: EconomicHead,
    pub blocks: u64,
    pub fills: usize,
}

fn demo_blocks() -> Vec<Vec<EconomicInput>> {
    let issuer = "DemoIssuer";
    let oracle = "DemoOracle";
    let mm = "DemoMM";
    let alice = "DemoAlice";
    let contract = ContractId::try_from(1).expect("nonzero");
    let order = |number: u64, owner: &str, side: Side, qty: &str, price: &str, post: bool| {
        EconomicInput::Order(Order {
            txid: tx(number),
            owner: account(owner),
            contract,
            height: 0,
            side,
            quantity: qty.parse().expect("decimal"),
            price: price.parse().expect("decimal"),
            post,
            is_market: false,
        })
    };
    let activate = vec![joint(
        10,
        "DemoAdmin",
        JointOperation::Registry(RegistryAction::Activate {
            types: vec![1, 2, 6, 11, 12, 13, 14, 15, 18],
            code_hash: [0x79; 32],
        }),
    )];
    let setup = vec![
        joint(
            20,
            oracle,
            JointOperation::Registry(RegistryAction::CreateOracle {
                ticker: "BTCUSD".into(),
                url: "https://mempool.space/api/v1/prices".into(),
                backup: account("DemoOracleBackup"),
                lag: 1,
            }),
        ),
        joint(
            21,
            oracle,
            JointOperation::Registry(RegistryAction::PublishOracle {
                oracle_id: 1,
                price_1e4: PRICE_1E4,
                high_1e4: None,
                low_1e4: None,
                close_1e4: None,
            }),
        ),
        joint(
            30,
            issuer,
            JointOperation::Issue {
                id: property(SIDE_TOKEN),
                kind: PropertyKind::Managed,
                initial_atoms: 1_000 * ATOMS,
            },
        ),
        joint(
            31,
            issuer,
            JointOperation::Grant {
                id: property(TUSD),
                recipient: account(mm),
                amount_atoms: 500_000 * ATOMS,
            },
        ),
        joint(
            32,
            issuer,
            JointOperation::Grant {
                id: property(TUSD),
                recipient: account(alice),
                amount_atoms: 100_000 * ATOMS,
            },
        ),
        joint(
            33,
            issuer,
            JointOperation::Grant {
                id: property(SIDE_TOKEN),
                recipient: account(alice),
                amount_atoms: 100 * ATOMS,
            },
        ),
    ];
    let ladder = vec![
        order(100, mm, Side::Buy, "1", "99900", true),
        order(101, mm, Side::Buy, "1", "99800", true),
        order(102, mm, Side::Buy, "1", "99700", true),
        order(103, mm, Side::Sell, "1", "100100", true),
        order(104, mm, Side::Sell, "1", "100200", true),
        order(105, mm, Side::Sell, "1", "100300", true),
    ];
    let taker = vec![order(200, alice, Side::Buy, "2", "100200", false)];
    vec![activate, setup, ladder, taker]
}

/// Builds a new store at `root`. A missing `root` is created; an existing one
/// must be an empty directory, so a store is never extended or overwritten.
pub fn seed(root: &Path) -> Result<DemoSeed, String> {
    if !root.is_absolute() {
        return Err("demo store path must be absolute".into());
    }
    if root.exists() {
        let mut entries = std::fs::read_dir(root).map_err(|e| format!("demo store dir: {e}"))?;
        if entries.next().is_some() {
            return Err("demo store directory is not empty; refusing to reuse it".into());
        }
    } else {
        std::fs::create_dir(root).map_err(|e| format!("demo store dir: {e}"))?;
    }
    let config = demo_config();
    let mut ledger = Ledger::new();
    // The collateral must exist in the bootstrap ledger. This is the ledger
    // operation a type-1 `Issue` applies; every later grant is an ordinary input.
    ledger
        .apply(
            tx(1),
            Operation::CreateProperty {
                id: property(TUSD),
                issuer: account("DemoIssuer"),
                kind: PropertyKind::Managed,
                initial_atoms: 10_000_000 * ATOMS,
            },
        )
        .map_err(|e| format!("demo ledger: {e:?}"))?;
    let chain = EconomicChain::new_unverified(config.clone(), ledger)
        .map_err(|e| format!("demo chain: {e:?}"))?;
    let mut owner = DurableEconomicChain::initialize(root, chain, config.anchor())
        .map_err(|e| format!("demo store: {e}"))?;
    let mut parent = config.anchor().id;
    let mut blocks = 0;
    for (index, mut inputs) in demo_blocks().into_iter().enumerate() {
        let height = index as u64 + 1;
        for input in &mut inputs {
            if let EconomicInput::Order(order) = input {
                order.height = height;
            }
        }
        let header = BlockHeader {
            height,
            id: block_id(height),
            parent,
        };
        let block = UnverifiedEconomicBlock {
            network: config.economics.network,
            header,
            inputs,
        };
        let expected = owner.head().map_err(|e| format!("demo store: {e}"))?;
        owner
            .apply_unverified_block(expected, &block)
            .map_err(|e| format!("demo block {height}: {e}"))?;
        parent = header.id;
        blocks = height;
    }
    let state = owner
        .chain()
        .map_err(|e| format!("demo store: {e}"))?
        .state();
    state.verify().map_err(|e| format!("demo state: {e:?}"))?;
    let fills = usize::try_from(state.receipt_count()).unwrap_or(usize::MAX);
    if fills != 2 {
        return Err(format!("demo expected 2 fills, found {fills}"));
    }
    let head = owner.head().map_err(|e| format!("demo store: {e}"))?;
    Ok(DemoSeed {
        config,
        head,
        blocks,
        fills,
    })
}
