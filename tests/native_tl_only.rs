//! Bitcoin TL-only native token policy. Legacy four-property source fixtures
//! remain frozen separately for replay and compatibility tests.
use tradelayer_core::chain::Tip;
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::native_entitlement::{
    bootstrap_commitment, canonical_policy_bytes, parse_policy, NativeEntitlementPolicy,
    NativeCoverage, NativeDriver, NativeRole, ATOMS_PER_TOKEN, BITCOIN_TL_PROPERTY_ID,
    BITCOIN_TL_TICKER,
};
use tradelayer_core::network_profile::TradeLayerMainnet;

fn account(s: &str) -> AccountId {
    AccountId::try_from(s).unwrap()
}
fn property(n: u32) -> PropertyId {
    PropertyId::try_from(n).unwrap()
}
fn tx(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn fixture() -> (Ledger, NativeEntitlementPolicy) {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            tx(1),
            Operation::CreateProperty {
                id: property(BITCOIN_TL_PROPERTY_ID),
                issuer: account("issuer"),
                kind: PropertyKind::Fixed,
                initial_atoms: 100 * ATOMS_PER_TOKEN,
            },
        )
        .unwrap();
    let policy = NativeEntitlementPolicy::bitcoin_tl_only(
        Tip {
            height: 0,
            id: TradeLayerMainnet::Bitcoin.genesis(),
        },
        bootstrap_commitment(&ledger.snapshot_bytes().unwrap()),
        account("operator"),
        account("issuer"),
        100 * ATOMS_PER_TOKEN,
    )
    .unwrap();
    (ledger, policy)
}

#[test]
fn bitcoin_tl_only_has_one_asset_no_coupon_and_distinct_v2_encoding() {
    let (mut ledger, policy) = fixture();
    assert_eq!(BITCOIN_TL_TICKER, "TL");
    assert!(policy.is_bitcoin_tl_only());
    assert_eq!(policy.assets.len(), 1);
    assert_eq!(policy.assets[0].role, NativeRole::Base);
    assert_eq!(policy.assets[0].property.get(), BITCOIN_TL_PROPERTY_ID);
    assert!(policy.bindings.is_empty());
    assert!(ledger.property(property(2)).is_none());
    assert!(ledger.property(property(3)).is_none());
    assert!(ledger.property(property(4)).is_none());

    let bytes = canonical_policy_bytes(&policy).unwrap();
    assert!(bytes.starts_with(b"TradeLayer/NativeEntitlementPolicy/v2/\x02"));
    assert_eq!(parse_policy(&bytes).unwrap(), policy);

    ledger
        .adopt_native_entitlements_unverified(&policy)
        .unwrap();
    assert_eq!(ledger.native_policy(), Some(&policy));
    assert_eq!(
        ledger.available_atoms(property(1), &account("issuer")),
        100 * ATOMS_PER_TOKEN,
    );
    let saved = ledger.snapshot_bytes().unwrap();
    assert_eq!(
        Ledger::from_snapshot_bytes_with_native_policy(&saved, &policy).unwrap(),
        ledger
    );

    ledger
        .apply(
            tx(2),
            Operation::Transfer {
                id: property(1),
                from: account("issuer"),
                to: account("alice"),
                amount_atoms: 25 * ATOMS_PER_TOKEN,
            },
        )
        .unwrap();
    assert_eq!(
        ledger.available_atoms(property(1), &account("alice")),
        25 * ATOMS_PER_TOKEN
    );
    assert_eq!(
        ledger.available_atoms(property(1), &account("issuer")),
        75 * ATOMS_PER_TOKEN
    );
    let saved = ledger.snapshot_bytes().unwrap();
    assert_eq!(
        Ledger::from_snapshot_bytes_with_native_policy(&saved, &policy).unwrap(),
        ledger
    );
}

#[test]
fn tl_only_rejects_extra_token_roles_and_supply_changes() {
    let (_, policy) = fixture();
    let mut extra = policy.clone();
    let mut ghost = extra.assets[0].clone();
    ghost.role = NativeRole::VestingIncome;
    ghost.property = property(3);
    extra.assets.push(ghost);
    assert!(extra.validate().is_err());

    let mut wrong_id = policy.clone();
    wrong_id.assets[0].property = property(3);
    assert!(wrong_id.validate().is_err());

    let mut expanded_supply = policy.clone();
    expanded_supply.assets[0].issuance_ceiling_atoms = None;
    assert!(expanded_supply.validate().is_err());

    let mut wrong_network = policy.clone();
    wrong_network.network = TradeLayerMainnet::Litecoin;
    assert!(wrong_network.validate().is_err());

    let mut noncanonical_driver = policy.clone();
    noncanonical_driver.driver = NativeDriver::SourcePerCallV1;
    assert!(noncanonical_driver.validate().is_err());

    let mut noncanonical_coverage = policy.clone();
    noncanonical_coverage.coverage = NativeCoverage::PerRequestOnlyV1;
    assert!(noncanonical_coverage.validate().is_err());

    let mut late_bootstrap = policy.clone();
    late_bootstrap.bootstrap.height = 1;
    assert!(late_bootstrap.validate().is_err());

    let mut legacy_tag = canonical_policy_bytes(&policy).unwrap();
    let v2 = b"TradeLayer/NativeEntitlementPolicy/v2/";
    let v1 = b"TradeLayer/NativeEntitlementPolicy/v1/";
    legacy_tag[..v2.len()].copy_from_slice(v1);
    legacy_tag[v1.len()] = 1;
    assert!(parse_policy(&legacy_tag).is_err());
}
