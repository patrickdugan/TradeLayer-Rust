//! Bitcoin TL + TLVEST native policy. Source four-asset fixtures remain frozen.
use tradelayer_core::chain::Tip;
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::native_entitlement::{
    bootstrap_commitment, canonical_policy_bytes, parse_policy, BitcoinTlVestingSpec,
    NativeCoverage, NativeDriver, NativeEntitlementPolicy, NativeEventContext, NativeMarkerRow,
    NativeRole, UnverifiedNativeTarget, ATOMS_PER_TOKEN, BITCOIN_TLVEST_PROPERTY_ID,
    BITCOIN_TLVEST_TICKER, BITCOIN_TL_PROPERTY_ID, BITCOIN_TL_TICKER,
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
    for (id, kind, atoms) in [
        (
            BITCOIN_TL_PROPERTY_ID,
            PropertyKind::Fixed,
            100 * ATOMS_PER_TOKEN,
        ),
        (
            BITCOIN_TLVEST_PROPERTY_ID,
            PropertyKind::Managed,
            20 * ATOMS_PER_TOKEN,
        ),
    ] {
        ledger
            .apply(
                tx(id as u8),
                Operation::CreateProperty {
                    id: property(id),
                    issuer: account("issuer"),
                    kind,
                    initial_atoms: atoms,
                },
            )
            .unwrap();
    }
    let policy = NativeEntitlementPolicy::bitcoin_tl_with_vesting(BitcoinTlVestingSpec {
        bootstrap: Tip {
            height: 0,
            id: TradeLayerMainnet::Bitcoin.genesis(),
        },
        bootstrap_ledger_commitment: bootstrap_commitment(&ledger.snapshot_bytes().unwrap()),
        target_operator: account("operator"),
        issuer: account("issuer"),
        tl_issued_atoms: 100 * ATOMS_PER_TOKEN,
        tlvest_issued_atoms: 20 * ATOMS_PER_TOKEN,
        vesting_funding_owner: account("issuer"),
        prefunded_tl_atoms: 20 * ATOMS_PER_TOKEN,
        initial_markers: vec![NativeMarkerRow {
            owner: account("issuer"),
            atoms: 20 * ATOMS_PER_TOKEN,
        }],
    })
    .unwrap();
    (ledger, policy)
}

#[test]
fn bitcoin_tl_and_vesting_have_two_assets_one_binding_and_distinct_v2_encoding() {
    let (mut ledger, policy) = fixture();
    assert_eq!(BITCOIN_TL_TICKER, "TL");
    assert_eq!(BITCOIN_TLVEST_TICKER, "TLVEST");
    assert!(policy.is_bitcoin_tl_with_vesting());
    assert_eq!(policy.assets.len(), 2);
    assert_eq!(policy.assets[0].role, NativeRole::Base);
    assert_eq!(policy.assets[1].role, NativeRole::VestingBase);
    assert_eq!(policy.bindings.len(), 1);
    assert_eq!(policy.bindings[0].coupon.get(), BITCOIN_TLVEST_PROPERTY_ID);
    assert_eq!(policy.bindings[0].target.get(), BITCOIN_TL_PROPERTY_ID);
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
        80 * ATOMS_PER_TOKEN
    );
    assert_eq!(
        ledger.native_remaining_atoms(property(2), &account("issuer")),
        20 * ATOMS_PER_TOKEN
    );
    let saved = ledger.snapshot_bytes().unwrap();
    assert_eq!(
        Ledger::from_snapshot_bytes_with_native_policy(&saved, &policy).unwrap(),
        ledger
    );

    ledger
        .send_native_coupon_unverified(
            tx(3),
            NativeEventContext {
                network: TradeLayerMainnet::Bitcoin,
                height: 1,
            },
            property(2),
            account("issuer"),
            account("alice"),
            5 * ATOMS_PER_TOKEN,
        )
        .unwrap();
    assert_eq!(
        ledger.available_atoms(property(2), &account("alice")),
        5 * ATOMS_PER_TOKEN
    );
    assert_eq!(
        ledger.native_remaining_atoms(property(2), &account("alice")),
        5 * ATOMS_PER_TOKEN
    );

    ledger
        .apply_native_target_unverified(
            &UnverifiedNativeTarget {
                txid: tx(4),
                claimed_actor: account("operator"),
                coupon: property(2),
                requested_target_atoms: 20 * ATOMS_PER_TOKEN,
            },
            NativeEventContext {
                network: TradeLayerMainnet::Bitcoin,
                height: 2,
            },
        )
        .unwrap();
    assert_eq!(
        ledger.available_atoms(property(1), &account("alice")),
        5 * ATOMS_PER_TOKEN
    );
    assert_eq!(
        ledger.available_atoms(property(1), &account("issuer")),
        95 * ATOMS_PER_TOKEN
    );
    assert_eq!(
        ledger.native_remaining_atoms(property(2), &account("alice")),
        0
    );
    let saved = ledger.snapshot_bytes().unwrap();
    assert_eq!(
        Ledger::from_snapshot_bytes_with_native_policy(&saved, &policy).unwrap(),
        ledger
    );
}

#[test]
fn tl_vesting_rejects_extra_roles_supply_changes_and_noncanonical_modes() {
    let (_, policy) = fixture();
    let mut extra = policy.clone();
    let mut ghost = extra.assets[0].clone();
    ghost.role = NativeRole::VestingIncome;
    ghost.property = property(3);
    extra.assets.push(ghost);
    assert!(extra.validate().is_err());

    let mut wrong_id = policy.clone();
    wrong_id.assets[1].property = property(3);
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

    let mut mismatched_claims = policy.clone();
    mismatched_claims.bindings[0].initial_markers[0].atoms = 25 * ATOMS_PER_TOKEN;
    assert!(mismatched_claims.validate().is_err());

    let mut legacy_tag = canonical_policy_bytes(&policy).unwrap();
    let v2 = b"TradeLayer/NativeEntitlementPolicy/v2/";
    let v1 = b"TradeLayer/NativeEntitlementPolicy/v1/";
    legacy_tag[..v2.len()].copy_from_slice(v1);
    legacy_tag[v1.len()] = 1;
    assert!(parse_policy(&legacy_tag).is_err());
}
