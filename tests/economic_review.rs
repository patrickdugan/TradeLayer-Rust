use tradelayer_core::decimal::Decimal;
use tradelayer_core::iou::{Claim, IouKey, IouStore};
use tradelayer_core::ledger::AccountId;

fn decimal(value: &str) -> Decimal {
    value.parse().unwrap()
}

fn address(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

#[test]
fn same_address_accrual_crosses_negative_claim_with_source_persistence_rounding() {
    // Independently reproduced with the unmodified pinned iou.js and archived
    // BigNumber 9.3.1. Rounding the two half-shares sequentially gives zero;
    // adding both unrounded shares to -1 before persistence would give 1e-20.
    let key = IouKey::new(1, 2);
    let mut store = IouStore::new();
    store
        .save_claim_map(
            key,
            1,
            [
                ("negative", "-1"),
                ("zero", "0"),
                ("tiny", "0.000000000000000000001"),
            ]
            .into_iter()
            .map(|(name, amount)| Claim {
                address: address(name),
                amount: decimal(amount),
            })
            .collect(),
        )
        .unwrap();
    store
        .add_iou_claims(
            key,
            1,
            &address("negative"),
            &address("negative"),
            &decimal("1"),
            &decimal("1"),
            &decimal("1.000000000000000000005"),
        )
        .unwrap();
    assert_eq!(
        store.claim_map(key).unwrap().claim(&address("negative")),
        Some(&decimal("0"))
    );

    store.add_loss(key, &decimal("1"), 1).unwrap();
    let allocations = store.pay_outstanding_ious(key, &decimal("1"), 1).unwrap();
    assert_eq!(allocations.len(), 1);
    assert_eq!(allocations[0].address, address("tiny"));
    assert_eq!(allocations[0].amount, decimal("0"));

    // A positive subprecision remaining claim is stored as zero, rather than
    // deleted: the source tests remaining <= 0 before calling toString(10).
    let claims = store.claim_map(key).unwrap().claims();
    assert_eq!(claims.len(), 3);
    assert_eq!(claims[0].address, address("negative"));
    assert_eq!(claims[1].address, address("zero"));
    assert_eq!(claims[2].address, address("tiny"));
    assert!(claims.iter().all(|claim| claim.amount.is_zero()));
    assert_eq!(store.claim_count(), 3);
    assert_eq!(store.bucket(key).unwrap().block_losses, decimal("1"));
}

mod combined_execution_review {
    use std::collections::BTreeSet;
    use tradelayer_core::contract_orderbook::{Order, Side};
    use tradelayer_core::contract_registry::ContractId;
    use tradelayer_core::economic::{
        EconomicConfig, EconomicInput, EconomicState, SettlementPolicy,
    };
    use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
    use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
    use tradelayer_core::network_profile::TradeLayerMainnet;
    use tradelayer_core::registry::RegistryAction;
    use tradelayer_core::tally::FeeRoute;

    fn a(value: &str) -> AccountId {
        value.try_into().unwrap()
    }
    fn id(value: u8) -> TxId {
        TxId::from_bytes([value; 32])
    }

    #[test]
    fn partial_fill_then_self_cross_releases_only_remaining_funded_reserve() {
        const ATOMS: u128 = 100_000_000;
        let property = PropertyId::try_from(7).unwrap();
        let contract = ContractId::try_from(1).unwrap();
        let config = EconomicConfig {
            network: TradeLayerMainnet::Litecoin,
            contract,
            collateral: property,
            inverse: false,
            notional: "1".parse().unwrap(),
            initial_margin_per_contract: "10".parse().unwrap(),
            fee_route: FeeRoute::Native(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
        };
        let mut ledger = Ledger::new();
        ledger
            .apply(
                id(1),
                Operation::CreateProperty {
                    id: property,
                    issuer: a("alice"),
                    kind: PropertyKind::Fixed,
                    initial_atoms: 2000 * ATOMS,
                },
            )
            .unwrap();
        ledger
            .apply(
                id(2),
                Operation::Transfer {
                    id: property,
                    from: a("alice"),
                    to: a("bob"),
                    amount_atoms: 1000 * ATOMS,
                },
            )
            .unwrap();
        let joint = JointProtocolState::from_bootstrap_unverified(
            config.network,
            a("admin"),
            ledger,
            BTreeSet::from([property]),
        )
        .unwrap();
        let mut state = EconomicState::from_joint_unverified(config.clone(), joint).unwrap();
        state
            .apply_block_unverified(
                1,
                vec![EconomicInput::Joint(UnverifiedJointTx {
                    txid: id(10),
                    actor: a("admin"),
                    operation: JointOperation::Registry(RegistryAction::Activate {
                        types: vec![6, 18],
                        code_hash: [0x71; 32],
                    }),
                })],
            )
            .unwrap();
        let order = |tx, owner, side, quantity: &str, price: &str| {
            EconomicInput::Order(Order {
                txid: id(tx),
                owner: a(owner),
                contract,
                height: 2,
                side,
                quantity: quantity.parse().unwrap(),
                price: price.parse().unwrap(),
                post: false,
                is_market: false,
            })
        };
        // Sender ordering places bob before alice. Alice's same-owner txid
        // ordering then admits her sell before her buy. The buy first fills
        // against bob, then its remaining half is removed by self-cross logic.
        assert_eq!(
            state
                .apply_block_unverified(
                    2,
                    vec![
                        order(20, "alice", Side::Sell, "1", "100"),
                        order(21, "bob", Side::Sell, "1", "90"),
                        order(22, "alice", Side::Buy, "2", "100"),
                    ]
                )
                .unwrap(),
            1
        );
        assert_eq!(state.receipts().len(), 1);
        assert_eq!(state.receipts()[0].quantity, "1".parse().unwrap());
        assert_eq!(state.reserve_atoms(id(22)), 0);
        assert_eq!(state.reserve_atoms(id(21)), 0);
        assert_eq!(state.reserve_atoms(id(20)), 10 * ATOMS);
        assert_eq!(state.book().resting().len(), 1);
        assert_eq!(state.book().resting()[0].order.txid, id(20));
        assert_eq!(state.margin_atoms(&a("alice")), 10 * ATOMS);
        assert_eq!(state.margin_atoms(&a("bob")), 10 * ATOMS);
        let fees = state
            .fees()
            .expected_custody_atoms(config.fee_route.key(property).unwrap())
            .unwrap();
        let balances = state.ledger().available_atoms(property, &a("alice"))
            + state.ledger().available_atoms(property, &a("bob"))
            + state.margin_atoms(&a("alice"))
            + state.margin_atoms(&a("bob"))
            + state.reserve_atoms(id(20))
            + fees
            + state.loss_pool_atoms()
            + state.insurance_custody_atoms();
        assert_eq!(balances, 2000 * ATOMS);
        state.verify().unwrap();
        state.ledger().verify_conservation().unwrap();
        let bytes = state.snapshot_bytes().unwrap();
        assert_eq!(
            EconomicState::from_snapshot_bytes(
                &bytes,
                &config,
                &a("admin"),
                &BTreeSet::from([property])
            )
            .unwrap(),
            state
        );
    }
}
