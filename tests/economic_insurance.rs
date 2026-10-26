//! Actual accepted-contract owner, real custody and complete insurance replay.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::contract_registry::{ContractAsset, ContractParameters};
use tradelayer_core::economic::*;
use tradelayer_core::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::storage::Recovery;
use tradelayer_core::tally::FeeRoute;

const A: u128 = 100_000_000;
fn d(s: &str) -> tradelayer_core::decimal::Decimal {
    s.parse().unwrap()
}
fn a(s: &str) -> AccountId {
    s.try_into().unwrap()
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn id(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
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
                initial_atoms: 20_000_000_000_000_013,
            },
        )
        .unwrap();
    for (n, owner, atoms) in [
        (2, "bob", 12 * A),
        (3, "carol", 1000 * A),
        (4, "sponsor", 10_000_000_000_000_001),
    ] {
        ledger
            .apply(
                id(n),
                Operation::Transfer {
                    id: p(),
                    from: a("alice"),
                    to: a(owner),
                    amount_atoms: atoms,
                },
            )
            .unwrap();
    }
    ledger
}
fn creation() -> CreateContractInput {
    CreateContractInput {
        txid: id(40),
        issuer: a("alice"),
        parameters: ContractParameters {
            native: false,
            underlying_oracle_id: Some(1),
            on_chain_data: vec![],
            notional_property_id: ContractAsset::BaseChain,
            notional_value: d("1"),
            collateral_property_id: ContractAsset::Property(p()),
            leverage: d("10"),
            expiry_period: 0,
            series: 1,
            inverse: false,
            fee: d("0"),
            whitelist: Some(0),
            ticker: None,
        },
    }
}
fn config(network: TradeLayerMainnet) -> EconomicChainConfig {
    let bootstrap = JointProtocolState::from_bootstrap_unverified(
        network,
        a("admin"),
        ledger(),
        BTreeSet::from([p()]),
    )
    .unwrap();
    let definition_hash = contract_definition_commitment(&creation()).unwrap();
    EconomicChainConfig {
        economics: EconomicConfig {
            native_entitlements: None,
            network,
            contract: 1.try_into().unwrap(),
            collateral: p(),
            inverse: false,
            notional: d("1"),
            initial_margin_per_contract: d("0"),
            fee_route: FeeRoute::Oracle(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
            marking: Some(MarkConfig {
                oracle_id: 1,
                policy: MarkPolicy::ConservingMarkBasisV1,
            }),
            liquidation: Some(LiquidationConfig {
                max_price_deviation_bps: 100,
                max_fills_per_owner: 8,
                policy: LiquidationPolicy::ConservingLiquidationV1,
            }),
            amm: None,
            source_consensus: None,
            contract_binding: Some(ContractBindingConfig {
                creation_txid: id(40),
                definition_hash,
                bootstrap_commitment: EconomicState::contract_bootstrap_commitment(&bootstrap)
                    .unwrap(),
                policy: ContractRiskPolicy::SourceContextsConservingV1,
            }),
            insurance: Some(InsuranceConfig {
                fund: InsuranceFundIdentity {
                    network,
                    contract: 1.try_into().unwrap(),
                    collateral: p(),
                    creation_txid: id(40),
                    definition_hash,
                    oracle_id: 1,
                },
                sponsor: a("sponsor"),
                operator: a("operator"),
                policy: InsurancePolicy::ConservingOracleBackstopV1,
            }),
        },
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p()]),
        rollback_limit: 16,
    }
}
fn joint(n: u8, action: RegistryAction) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(n),
        actor: a("admin"),
        operation: JointOperation::Registry(action),
    })
}
fn publication(n: u8, price: u128) -> EconomicInput {
    joint(
        n,
        RegistryAction::PublishOracle {
            oracle_id: 1,
            price_1e4: price * 10_000,
            high_1e4: None,
            low_1e4: None,
            close_1e4: None,
        },
    )
}
fn order(n: u8, owner: &str, side: Side, price: &str, height: u64) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: id(n),
        owner: a(owner),
        contract: 1.try_into().unwrap(),
        height,
        side,
        quantity: d("1"),
        price: d(price),
        post: false,
        is_market: false,
    })
}
fn insurance(n: u8, actor: &str, action: InsuranceAction) -> EconomicInput {
    EconomicInput::Insurance(UnverifiedInsuranceInput {
        txid: id(n),
        actor: a(actor),
        action,
    })
}
fn block(chain: &EconomicChain, inputs: Vec<EconomicInput>) -> UnverifiedEconomicBlock {
    let h = chain.tip().height + 1;
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&h.to_be_bytes());
    UnverifiedEconomicBlock {
        network: chain.config().economics.network,
        header: BlockHeader {
            height: h,
            id: BlockId(bytes),
            parent: chain.tip().id,
        },
        inputs,
    }
}
fn apply(chain: &mut EconomicChain, inputs: Vec<EconomicInput>) {
    chain.apply_unverified_block(&block(chain, inputs)).unwrap();
}
fn ready(network: TradeLayerMainnet) -> EconomicChain {
    ready_with_deposit(network, 100 * A + 1)
}
fn ready_with_deposit(network: TradeLayerMainnet, initial_deposit: u128) -> EconomicChain {
    let mut chain = EconomicChain::new_unverified(config(network), ledger()).unwrap();
    apply(
        &mut chain,
        vec![joint(
            10,
            RegistryAction::Activate {
                types: vec![2, 6, 13, 14, 15, 16, 18],
                code_hash: [0x51; 32],
            },
        )],
    );
    apply(
        &mut chain,
        vec![
            joint(
                11,
                RegistryAction::CreateOracle {
                    ticker: "TEST".into(),
                    url: "https://example.invalid".into(),
                    backup: a("backup"),
                    lag: 1,
                },
            ),
            publication(12, 100),
            EconomicInput::CreateContract(creation()),
            insurance(
                41,
                "sponsor",
                InsuranceAction::Deposit {
                    atoms: initial_deposit,
                },
            ),
        ],
    );
    apply(
        &mut chain,
        vec![
            order(20, "alice", Side::Buy, "100", 3),
            order(21, "bob", Side::Sell, "100", 3),
        ],
    );
    chain
}
fn closeout_inputs(requests: Vec<EconomicInput>) -> Vec<EconomicInput> {
    let mut inputs = requests;
    inputs.push(publication(30, 200));
    inputs.push(order(31, "carol", Side::Sell, "200", 4));
    inputs
}
fn remaining_claims(state: &EconomicState) -> u128 {
    state
        .insurance_state()
        .receipts
        .iter()
        .rev()
        .find_map(|receipt| receipt.plan.as_ref())
        .map(|plan| {
            plan.claims_before.iter().map(|row| row.atoms).sum::<u128>()
                - plan
                    .existing_funded_payouts
                    .iter()
                    .map(|row| row.atoms)
                    .sum::<u128>()
                - plan.cover_atoms
        })
        .unwrap_or(0)
}

#[test]
fn real_book_executes_before_backstop_and_positions_remain_market_positions() {
    let mut chain = ready(TradeLayerMainnet::Bitcoin);
    apply(
        &mut chain,
        closeout_inputs(vec![insurance(
            50,
            "operator",
            InsuranceAction::Backstop { max_atoms: 30 * A },
        )]),
    );
    let state = chain.state();
    let receipt = state.insurance_state().receipts.last().unwrap();
    let plan = receipt.plan.as_ref().unwrap();
    assert_eq!(plan.cover_atoms, 30 * A);
    assert_eq!(state.positions().get(&a("bob")).unwrap().contracts, d("0"));
    assert_eq!(
        state.positions().get(&a("alice")).unwrap().contracts,
        d("1")
    );
    assert_eq!(
        state.positions().get(&a("carol")).unwrap().contracts,
        d("-1")
    );
    assert!(state
        .liquidation_state()
        .events
        .iter()
        .any(|event| event.height == 4 && !event.fills.is_empty()));
    assert!(state
        .mark_state()
        .events
        .iter()
        .filter(|event| event.height == 4)
        .any(|event| matches!(event.kind, MarkEventKind::Forced { .. })));
    assert_eq!(
        plan.insurer_funded_payouts
            .iter()
            .map(|row| row.atoms)
            .sum::<u128>(),
        30 * A
    );
    assert!(remaining_claims(state) > 0);
    assert!(state.unfunded_loss_atoms() > 30 * A);
    assert_eq!(state.insurance_state().reserve_atoms, 70 * A + 1);
    state.verify().unwrap();
}

#[test]
fn ordered_partial_requests_do_not_reuse_a_reserve_or_paid_default() {
    let mut chain = ready(TradeLayerMainnet::Bitcoin);
    apply(
        &mut chain,
        closeout_inputs(vec![
            insurance(
                50,
                "operator",
                InsuranceAction::Backstop { max_atoms: 10 * A },
            ),
            insurance(
                51,
                "operator",
                InsuranceAction::Backstop { max_atoms: 20 * A },
            ),
        ]),
    );
    let receipts = &chain.state().insurance_state().receipts;
    let first = &receipts[receipts.len() - 2];
    let second = receipts.last().unwrap();
    assert_eq!(first.input.txid, id(50));
    assert_eq!(second.input.txid, id(51));
    assert_eq!(first.reserve_after_atoms, second.reserve_before_atoms);
    assert_eq!(first.plan.as_ref().unwrap().cover_atoms, 10 * A);
    assert_eq!(second.plan.as_ref().unwrap().cover_atoms, 20 * A);
    let debt_before: u128 = first
        .plan
        .as_ref()
        .unwrap()
        .unpaid_losses_before
        .iter()
        .map(|row| row.atoms)
        .sum();
    let debt_after: u128 = chain.state().mark_state().unpaid_losses.values().sum();
    assert_eq!(debt_before - debt_after, 30 * A);
    let before = chain.checkpoint_bytes().unwrap();
    let duplicate = block(
        &chain,
        vec![insurance(
            50,
            "operator",
            InsuranceAction::Backstop { max_atoms: 1 },
        )],
    );
    assert!(chain.apply_unverified_block(&duplicate).is_err());
    assert_eq!(chain.checkpoint_bytes().unwrap(), before);
}

#[test]
fn unsupported_actor_empty_cover_and_late_failure_reject_the_whole_block() {
    let mut chain = ready(TradeLayerMainnet::Bitcoin);
    let before = chain.checkpoint_bytes().unwrap();
    for inputs in [
        vec![insurance(
            50,
            "alice",
            InsuranceAction::Backstop { max_atoms: A },
        )],
        vec![insurance(
            50,
            "operator",
            InsuranceAction::Backstop { max_atoms: A },
        )],
        closeout_inputs(vec![
            insurance(
                50,
                "operator",
                InsuranceAction::Backstop {
                    max_atoms: u128::MAX,
                },
            ),
            insurance(51, "operator", InsuranceAction::Backstop { max_atoms: 1 }),
        ]),
    ] {
        assert!(chain
            .apply_unverified_block(&block(&chain, inputs))
            .is_err());
        assert_eq!(chain.checkpoint_bytes().unwrap(), before);
    }
}

#[test]
fn fee_adoption_moves_only_actual_pending_fee_atoms_and_cannot_repeat_them() {
    let mut chain = ready(TradeLayerMainnet::Bitcoin);
    let pending = chain.state().insurance_custody_atoms();
    assert!(pending > 0);
    let reserve = chain.state().insurance_state().reserve_atoms;
    apply(
        &mut chain,
        vec![insurance(
            50,
            "operator",
            InsuranceAction::AdoptFees {
                max_atoms: u128::MAX,
            },
        )],
    );
    assert_eq!(chain.state().insurance_custody_atoms(), 0);
    assert_eq!(chain.state().insurance_state().adopted_fee_atoms, pending);
    assert_eq!(
        chain.state().insurance_state().reserve_atoms,
        reserve + pending
    );
    let before = chain.checkpoint_bytes().unwrap();
    assert!(chain
        .apply_unverified_block(&block(
            &chain,
            vec![insurance(
                51,
                "operator",
                InsuranceAction::AdoptFees { max_atoms: 1 }
            )]
        ))
        .is_err());
    assert_eq!(chain.checkpoint_bytes().unwrap(), before);
}

#[test]
fn exact_above_binary64_range_deposit_and_independent_fund_config() {
    let mut chain = ready(TradeLayerMainnet::Bitcoin);
    let exact = 9_007_199_254_740_993;
    let old = chain.state().ledger().available_atoms(p(), &a("sponsor"));
    let reserve = chain.state().insurance_state().reserve_atoms;
    apply(
        &mut chain,
        vec![insurance(
            50,
            "sponsor",
            InsuranceAction::Deposit { atoms: exact },
        )],
    );
    assert_eq!(
        chain.state().ledger().available_atoms(p(), &a("sponsor")),
        old - exact
    );
    assert_eq!(
        chain.state().insurance_state().reserve_atoms,
        reserve + exact
    );
    let bytes = chain.state().snapshot_bytes().unwrap();
    let mut wrong = chain.config().economics.clone();
    wrong.insurance.as_mut().unwrap().operator = a("attacker");
    assert!(EconomicState::from_snapshot_bytes(
        &bytes,
        &wrong,
        &a("admin"),
        &BTreeSet::from([p()])
    )
    .is_err());
    let mut wrong = chain.config().economics.clone();
    wrong.insurance.as_mut().unwrap().fund.oracle_id = 2;
    assert!(EconomicState::from_snapshot_bytes(
        &bytes,
        &wrong,
        &a("admin"),
        &BTreeSet::from([p()])
    )
    .is_err());
}

fn crc(bytes: &[u8]) -> u32 {
    let mut n = !0u32;
    for byte in bytes {
        n ^= u32::from(*byte);
        for _ in 0..8 {
            n = (n >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(n & 1)));
        }
    }
    !n
}
fn seal(bytes: &mut Vec<u8>) {
    bytes.truncate(bytes.len() - 4);
    bytes.extend_from_slice(&crc(bytes).to_be_bytes());
}
#[test]
fn repaired_crc_cannot_change_receipts_funding_identity_or_complete_history() {
    let mut chain = ready(TradeLayerMainnet::Bitcoin);
    apply(
        &mut chain,
        closeout_inputs(vec![insurance(
            50,
            "operator",
            InsuranceAction::Backstop { max_atoms: 30 * A },
        )]),
    );
    let bytes = chain.state().snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x09");
    for offset in [6, bytes.len() - 13, bytes.len() - 29, bytes.len() - 45] {
        let mut forged = bytes.clone();
        forged[offset] ^= 1;
        seal(&mut forged);
        assert!(EconomicState::from_snapshot_bytes(
            &forged,
            &chain.config().economics,
            &a("admin"),
            &BTreeSet::from([p()])
        )
        .is_err());
    }
    let mut forged = bytes.clone();
    forged[4] = 8;
    seal(&mut forged);
    assert!(EconomicState::from_snapshot_bytes(
        &forged,
        &chain.config().economics,
        &a("admin"),
        &BTreeSet::from([p()])
    )
    .is_err());
    for len in [0, 4, 5, bytes.len() - 1] {
        assert!(EconomicState::from_snapshot_bytes(
            &bytes[..len],
            &chain.config().economics,
            &a("admin"),
            &BTreeSet::from([p()])
        )
        .is_err());
    }
}

#[test]
fn durable_restart_rollback_reapply_and_stale_head_are_exact_on_three_networks() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let chain = ready(network);
        let config = chain.config().clone();
        let before = chain.checkpoint_bytes().unwrap();
        let before_tip = chain.tip();
        let path = std::env::temp_dir().join(format!(
            "tl-insurance-{}-{}-{}",
            std::process::id(),
            network.tag(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        let mut owner = DurableEconomicChain::initialize(&path, chain, before_tip).unwrap();
        let previous = owner.head().unwrap();
        let next = block(
            owner.chain().unwrap(),
            closeout_inputs(vec![insurance(
                50,
                "operator",
                InsuranceAction::Backstop { max_atoms: 30 * A },
            )]),
        );
        let head = owner.apply_unverified_block(previous, &next).unwrap();
        let after = owner.chain().unwrap().checkpoint_bytes().unwrap();
        assert!(owner.apply_unverified_block(previous, &next).is_err());
        drop(owner);
        let (mut owner, _) =
            DurableEconomicChain::load(&path, config.clone(), head, Recovery::RejectIncompleteTail)
                .unwrap();
        assert_eq!(owner.chain().unwrap().checkpoint_bytes().unwrap(), after);
        let rolled = owner.rollback_to(head, before_tip).unwrap();
        assert_eq!(owner.chain().unwrap().checkpoint_bytes().unwrap(), before);
        assert_ne!(rolled.sequence, previous.sequence);
        let reapplied = owner.apply_unverified_block(rolled, &next).unwrap();
        assert_eq!(owner.chain().unwrap().checkpoint_bytes().unwrap(), after);
        drop(owner);
        assert!(DurableEconomicChain::load(
            &path,
            config.clone(),
            head,
            Recovery::RejectIncompleteTail
        )
        .is_err());
        let (owner, _) =
            DurableEconomicChain::load(&path, config, reapplied, Recovery::RejectIncompleteTail)
                .unwrap();
        assert_eq!(owner.chain().unwrap().checkpoint_bytes().unwrap(), after);
        drop(owner);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn real_amm_first_closeout_and_pool_claim_payment_keep_exact_cash_history() {
    let mut cfg = config(TradeLayerMainnet::Bitcoin);
    cfg.economics.amm = Some(FundedAmmConfig {
        pool_id: 1,
        spread_bps: 40,
        max_quote_quantity: d("1"),
        max_abs_inventory: d("4"),
        max_collateral_atoms: 1000 * A,
        policy: FundedAmmPolicy::ConservingFundedContractAmmV1,
    });
    let mut chain = EconomicChain::new_unverified(cfg, ledger()).unwrap();
    apply(
        &mut chain,
        vec![joint(
            10,
            RegistryAction::Activate {
                types: vec![2, 6, 10, 13, 14, 15, 16, 18],
                code_hash: [0x51; 32],
            },
        )],
    );
    apply(
        &mut chain,
        vec![
            joint(
                11,
                RegistryAction::CreateOracle {
                    ticker: "TEST".into(),
                    url: "https://example.invalid".into(),
                    backup: a("backup"),
                    lag: 1,
                },
            ),
            publication(12, 100),
            EconomicInput::CreateContract(creation()),
            insurance(
                41,
                "sponsor",
                InsuranceAction::Deposit { atoms: 100 * A + 1 },
            ),
            EconomicInput::Amm(AmmCapitalInput::Deposit {
                txid: id(42),
                owner: a("sponsor"),
                collateral_atoms: 500 * A,
                min_share_units: 500 * A,
            }),
        ],
    );
    apply(
        &mut chain,
        vec![
            order(20, "alice", Side::Buy, "100", 3),
            order(21, "bob", Side::Sell, "100", 3),
        ],
    );
    apply(
        &mut chain,
        vec![
            insurance(
                50,
                "operator",
                InsuranceAction::Backstop { max_atoms: 30 * A },
            ),
            publication(30, 200),
        ],
    );
    assert!(chain
        .state()
        .liquidation_state()
        .events
        .iter()
        .any(|event| !event.amm_fills.is_empty()));
    assert_eq!(
        chain.state().positions().get(&a("bob")).unwrap().contracts,
        d("0")
    );
    apply(
        &mut chain,
        vec![
            publication(32, 190),
            insurance(
                51,
                "operator",
                InsuranceAction::Backstop {
                    max_atoms: u128::MAX,
                },
            ),
        ],
    );
    let receipt = chain.state().insurance_state().receipts.last().unwrap();
    assert!(receipt
        .plan
        .as_ref()
        .unwrap()
        .insurer_funded_payouts
        .iter()
        .any(|row| row.owner.as_str().starts_with("~econ:")));
    let bytes = chain.state().snapshot_bytes().unwrap();
    let decoded = EconomicState::from_snapshot_bytes(
        &bytes,
        &chain.config().economics,
        &a("admin"),
        &BTreeSet::from([p()]),
    )
    .unwrap();
    assert_eq!(&decoded, chain.state());
    assert_eq!(decoded.snapshot_bytes().unwrap(), bytes);
}

#[test]
fn later_reversal_cannot_offset_or_repay_a_default_already_extinguished_by_insurance() {
    let mut chain = ready(TradeLayerMainnet::Bitcoin);
    apply(
        &mut chain,
        vec![
            insurance(
                50,
                "operator",
                InsuranceAction::Backstop { max_atoms: 30 * A },
            ),
            publication(30, 200),
        ],
    );
    assert_eq!(
        chain.state().positions().get(&a("bob")).unwrap().contracts,
        d("-1")
    );
    let historical = chain.state().unfunded_loss_atoms();
    let before: u128 = chain.state().mark_state().unpaid_losses.values().sum();
    assert!(before > 0);
    apply(&mut chain, vec![publication(32, 100)]);
    assert!(chain.state().mark_state().unpaid_losses.is_empty());
    assert!(chain
        .state()
        .ious()
        .claim_maps()
        .all(|map| map.claims().is_empty()));
    assert_eq!(chain.state().loss_pool_atoms(), 0);
    assert_eq!(chain.state().unfunded_loss_atoms(), historical);
    assert_eq!(chain.state().insurance_state().covered_atoms, 30 * A);
    assert_eq!(chain.state().insurance_state().reserve_atoms, 70 * A + 1);
    let bytes = chain.state().snapshot_bytes().unwrap();
    assert_eq!(
        EconomicState::from_snapshot_bytes(
            &bytes,
            &chain.config().economics,
            &a("admin"),
            &BTreeSet::from([p()])
        )
        .unwrap(),
        *chain.state()
    );
}

#[test]
fn bounded_receipt_count_and_source_history_configuration_fail_closed() {
    let mut chain = ready(TradeLayerMainnet::Bitcoin);
    apply(
        &mut chain,
        closeout_inputs(vec![insurance(
            50,
            "operator",
            InsuranceAction::Backstop { max_atoms: 30 * A },
        )]),
    );
    let state = chain.state().insurance_state();
    let marker: Vec<_> = [
        state.reserve_atoms,
        state.deposited_atoms,
        state.adopted_fee_atoms,
        state.covered_atoms,
    ]
    .into_iter()
    .flat_map(u128::to_be_bytes)
    .collect();
    let mut bytes = chain.state().snapshot_bytes().unwrap();
    let start = bytes
        .windows(marker.len())
        .rposition(|part| part == marker)
        .unwrap();
    bytes[start + 64..start + 68]
        .copy_from_slice(&((MAX_INSURANCE_RECEIPTS + 1) as u32).to_be_bytes());
    seal(&mut bytes);
    assert!(matches!(
        EconomicState::from_snapshot_bytes(
            &bytes,
            &chain.config().economics,
            &a("admin"),
            &BTreeSet::from([p()])
        ),
        Err(EconomicError::Capacity)
    ));
    let mut cfg = config(TradeLayerMainnet::Bitcoin);
    cfg.economics.source_consensus =
        Some(tradelayer_core::consensus_source::SourceConsensusConfig {
            catalog_bootstrap_commitment: None,
            bootstrap_commitment: [0; 32],
            policy:
                tradelayer_core::consensus_source::SourceConsensusPolicy::ConservingSequentialV1,
        });
    assert!(cfg.validate().is_err());
    assert!(EconomicChain::new_unverified(cfg, ledger()).is_err());
}

#[test]
fn two_valid_insurance_branches_cannot_splice_a_different_rollback_history() {
    let mut branch_a = ready_with_deposit(TradeLayerMainnet::Bitcoin, 100 * A + 1);
    let branch_b = ready_with_deposit(TradeLayerMainnet::Bitcoin, 99 * A + 1);
    assert_eq!(branch_a.config(), branch_b.config());
    assert_eq!(branch_a.tip(), branch_b.tip());
    let original_a_before = branch_a.state().snapshot_bytes().unwrap();
    let b_before = branch_b.state().snapshot_bytes().unwrap();
    assert_ne!(original_a_before, b_before);
    // Both histories use the same independent bootstrap/config, block IDs and
    // external input IDs; the earlier sponsor deposit amount differs.
    assert_eq!(
        EconomicState::from_snapshot_bytes(
            &b_before,
            &branch_b.config().economics,
            &a("admin"),
            &BTreeSet::from([p()])
        )
        .unwrap(),
        *branch_b.state()
    );
    apply(
        &mut branch_a,
        closeout_inputs(vec![insurance(
            50,
            "operator",
            InsuranceAction::Backstop { max_atoms: 30 * A },
        )]),
    );
    let original = branch_a.checkpoint_bytes().unwrap();
    assert_eq!(
        EconomicChain::from_checkpoint_bytes(&original, branch_a.config().clone(), branch_a.tip())
            .unwrap(),
        branch_a
    );
    let mut offset = 5 + 1 + 32 + 1 + 40 + 40;
    let state_len = u32::from_be_bytes(original[offset..offset + 4].try_into().unwrap()) as usize;
    offset += 4 + state_len;
    let count = original[offset] as usize;
    offset += 1;
    let mut replacement = None;
    for index in 0..count {
        offset += 8 + 32 + 32;
        let len_offset = offset;
        let len = u32::from_be_bytes(original[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        if index + 1 == count {
            assert_eq!(
                &original[offset..offset + len],
                original_a_before.as_slice()
            );
            replacement = Some((len_offset, offset + len));
        }
        offset += len;
    }
    let (start, end) = replacement.unwrap();
    let mut substitute = (b_before.len() as u32).to_be_bytes().to_vec();
    substitute.extend_from_slice(&b_before);
    let mut forged = original.clone();
    drop(forged.splice(start..end, substitute));
    seal(&mut forged);
    // Nested B TLECv9 and the outer TLEH checksum are individually valid. The
    // retained adjacent typed-history prefix is the reason to reject this.
    assert!(EconomicChain::from_checkpoint_bytes(
        &forged,
        branch_a.config().clone(),
        branch_a.tip()
    )
    .is_err());
    assert_eq!(
        EconomicChain::from_checkpoint_bytes(&original, branch_a.config().clone(), branch_a.tip())
            .unwrap(),
        branch_a
    );
}
