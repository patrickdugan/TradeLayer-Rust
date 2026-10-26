#![allow(dead_code, unused_imports)]
use native_owner::chain::Tip;
use native_owner::ledger::{
    AccountId, ChannelSide, Ledger, LedgerError, NativeEvent, Operation, PropertyId, PropertyKind,
    TxId,
};
use native_owner::native_entitlement::*;
use native_owner::network_profile::TradeLayerMainnet;
pub fn a(s: &str) -> AccountId {
    AccountId::try_from(s).unwrap()
}
pub fn p(n: u32) -> PropertyId {
    PropertyId::try_from(n).unwrap()
}
pub fn tx(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
pub const U: u128 = ATOMS_PER_TOKEN;
pub fn fixture(
    network: TradeLayerMainnet,
    driver: NativeDriver,
) -> (Ledger, NativeEntitlementPolicy) {
    let mut l = Ledger::new();
    for id in 1..=4 {
        l.apply(
            tx(id as u8),
            Operation::CreateProperty {
                id: p(id),
                issuer: a("funder"),
                kind: PropertyKind::Managed,
                initial_atoms: 20 * U,
            },
        )
        .unwrap();
    }
    for id in [2, 3] {
        for (n, owner) in [(10, "alice"), (11, "bob")] {
            l.apply(
                tx(n + id as u8 * 10),
                Operation::Transfer {
                    id: p(id),
                    from: a("funder"),
                    to: a(owner),
                    amount_atoms: 10 * U,
                },
            )
            .unwrap();
        }
    }
    let roles = [
        NativeRole::Base,
        NativeRole::VestingBase,
        NativeRole::VestingIncome,
        NativeRole::Income,
    ];
    let classes = [
        SourceCatalogClass::Fixed,
        SourceCatalogClass::Vesting,
        SourceCatalogClass::Vesting,
        SourceCatalogClass::Native,
    ];
    let policy = NativeEntitlementPolicy {
        network,
        bootstrap: Tip {
            height: 0,
            id: network.genesis(),
        },
        bootstrap_ledger_commitment: bootstrap_commitment(&l.snapshot_bytes().unwrap()),
        target_operator: a("operator"),
        assets: (1..=4)
            .map(|id| NativeAssetSpec {
                role: roles[id - 1],
                property: p(id as u32),
                issuer: a("funder"),
                ledger_kind: PropertyKind::Managed,
                catalog_class: classes[id - 1],
                declared_figure_atoms: 20 * U,
                issuance_ceiling_atoms: None,
                expected_initial_issued_atoms: 20 * U,
            })
            .collect(),
        bindings: [(2, 1), (3, 4)]
            .into_iter()
            .map(|(c, t)| NativeBindingSpec {
                coupon: p(c),
                target: p(t),
                curve_scale_atoms: 20 * U,
                initial_markers: ["alice", "bob"]
                    .into_iter()
                    .map(|s| NativeMarkerRow {
                        owner: a(s),
                        atoms: 10 * U,
                    })
                    .collect(),
                funding: PrefundedTargetSpec {
                    funding_owner: a("funder"),
                    target_atoms: 20 * U,
                },
            })
            .collect(),
        quote_policy: NativeQuotePolicy::SourceQuoteExactOrRejectV1,
        driver,
        payout_order: NativePayoutOrder::AsciiAccountV1,
        coverage: NativeCoverage::CoverInitialMarkersV1,
    };
    (l, policy)
}
pub fn context(p: &NativeEntitlementPolicy, height: u64) -> NativeEventContext {
    NativeEventContext {
        network: p.network,
        height,
    }
}
pub fn request(id: u8, atoms: u128) -> UnverifiedNativeTarget {
    UnverifiedNativeTarget {
        txid: tx(id),
        claimed_actor: a("operator"),
        coupon: p(2),
        requested_target_atoms: atoms,
    }
}
