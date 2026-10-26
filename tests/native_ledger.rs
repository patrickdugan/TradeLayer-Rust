extern crate tradelayer_core as native_owner;
use native_owner::chain::Tip;
use native_owner::ledger::{
    AccountId, ChannelSide, Ledger, LedgerError, NativeEvent, Operation, PropertyId, PropertyKind,
    TxId,
};
use native_owner::native_entitlement::*;
use native_owner::network_profile::TradeLayerMainnet;
fn a(s: &str) -> AccountId {
    AccountId::try_from(s).unwrap()
}
fn p(n: u32) -> PropertyId {
    PropertyId::try_from(n).unwrap()
}
fn tx(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
const U: u128 = ATOMS_PER_TOKEN;
fn fixture(network: TradeLayerMainnet, driver: NativeDriver) -> (Ledger, NativeEntitlementPolicy) {
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
fn context(p: &NativeEntitlementPolicy, height: u64) -> NativeEventContext {
    NativeEventContext {
        network: p.network,
        height,
    }
}
fn request(id: u8, atoms: u128) -> UnverifiedNativeTarget {
    UnverifiedNativeTarget {
        txid: tx(id),
        claimed_actor: a("operator"),
        coupon: p(2),
        requested_target_atoms: atoms,
    }
}
#[test]
fn funded_adoption_on_three_networks_conserves_and_uses_independent_policy() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let (mut l, policy) = fixture(network, NativeDriver::MonotonicDeltaV1);
        let old = l.clone();
        let id = l.adopt_native_entitlements_unverified(&policy).unwrap();
        assert_eq!(id, adoption_id(&policy).unwrap());
        assert!(l.is_processed(id));
        assert_eq!(l.available_atoms(p(1), &a("funder")), 0);
        assert_eq!(
            l.available_atoms(p(1), &escrow(&policy, p(2)).unwrap()),
            20 * U
        );
        assert_eq!(l.property(p(1)), old.property(p(1)));
        assert_eq!(l.native_remaining_atoms(p(2), &a("alice")), 10 * U);
        let bytes = l.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..4], b"TLL\x05");
        assert!(Ledger::from_snapshot_bytes(&bytes).is_err());
        assert_eq!(
            Ledger::from_snapshot_bytes_with_native_policy(&bytes, &policy).unwrap(),
            l
        );
        let mut wrong = policy.clone();
        wrong.target_operator = a("other");
        assert!(Ledger::from_snapshot_bytes_with_native_policy(&bytes, &wrong).is_err());
        // The valid-CRC inner cannot shed native metadata through the ordinary decoder.
        let len = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert!(Ledger::from_snapshot_bytes(&bytes[8..8 + len]).is_err());
        assert!(l.adopt_native_entitlements_unverified(&policy).is_err());
    }
}
#[test]
fn reachable_whole_unit_marker_drift_rejects_entire_target_without_partial_payout() {
    let (mut l, policy) = fixture(TradeLayerMainnet::Bitcoin, NativeDriver::SourcePerCallV1);
    l.adopt_native_entitlements_unverified(&policy).unwrap();
    let drag = l
        .send_native_coupon_unverified(
            tx(80),
            context(&policy, 1),
            p(2),
            a("alice"),
            a("bob"),
            110_000_000,
        )
        .unwrap();
    assert_eq!(drag.atoms, U);
    assert_eq!(l.available_atoms(p(2), &a("alice")), 890_000_000);
    assert_eq!(l.native_remaining_atoms(p(2), &a("alice")), 9 * U);
    assert_eq!(l.available_atoms(p(2), &a("bob")), 1_110_000_000);
    assert_eq!(l.native_remaining_atoms(p(2), &a("bob")), 11 * U);
    let before = l.snapshot_bytes().unwrap();
    assert_eq!(
        l.apply_native_target_unverified(&request(81, 20 * U), context(&policy, 2)),
        Err(LedgerError::NativeEntitlement(
            NativeError::InsufficientMarker
        ))
    );
    assert_eq!(l.snapshot_bytes().unwrap(), before);
    assert!(!l.is_processed(tx(81)));
    assert_eq!(l.available_atoms(p(1), &a("alice")), 0);
}
#[test]
fn explicit_drivers_have_distinct_repeated_target_behavior() {
    for (driver, paid) in [
        (NativeDriver::SourcePerCallV1, 12 * U),
        (NativeDriver::MonotonicDeltaV1, 6 * U),
    ] {
        let (mut l, policy) = fixture(TradeLayerMainnet::Litecoin, driver);
        l.adopt_native_entitlements_unverified(&policy).unwrap();
        for (id, height) in [(80, 1), (81, 2)] {
            l.apply_native_target_unverified(&request(id, 6 * U), context(&policy, height))
                .unwrap();
        }
        assert_eq!(l.available_atoms(p(1), &a("alice")), paid / 2);
        assert_eq!(
            l.available_atoms(p(1), &escrow(&policy, p(2)).unwrap()),
            20 * U - paid
        );
        assert_eq!(
            l.native_remaining_atoms(p(2), &a("alice")),
            10 * U - paid / 2
        );
        let n = l.native_state().unwrap().binding(p(2)).unwrap();
        assert_eq!(n.released_atoms, paid);
        assert_eq!(n.last_observed_target_atoms, 6 * U);
        let bytes = l.snapshot_bytes().unwrap();
        assert_eq!(
            Ledger::from_snapshot_bytes_with_native_policy(&bytes, &policy).unwrap(),
            l
        );
        if driver == NativeDriver::MonotonicDeltaV1 {
            let before = l.clone();
            assert!(l
                .apply_native_target_unverified(&request(82, 5 * U), context(&policy, 3))
                .is_err());
            assert_eq!(l, before);
        }
    }
}
#[test]
fn ordinary_target_transfers_are_replayed_and_every_legacy_coupon_supply_channel_path_is_fenced() {
    let (mut l, policy) = fixture(TradeLayerMainnet::Dogecoin, NativeDriver::MonotonicDeltaV1);
    l.adopt_native_entitlements_unverified(&policy).unwrap();
    l.apply_native_target_unverified(&request(80, 4 * U), context(&policy, 1))
        .unwrap();
    l.apply(
        tx(81),
        Operation::Transfer {
            id: p(1),
            from: a("alice"),
            to: a("merchant"),
            amount_atoms: U,
        },
    )
    .unwrap();
    assert!(matches!(
        l.native_state().unwrap().events().last().unwrap(),
        NativeEvent::OrdinaryTargetTransfer { .. }
    ));
    for (n, op) in [
        Operation::Transfer {
            id: p(2),
            from: a("alice"),
            to: a("bob"),
            amount_atoms: U,
        },
        Operation::Mint {
            id: p(1),
            issuer: a("funder"),
            to: a("alice"),
            amount_atoms: U,
        },
        Operation::Burn {
            id: p(2),
            issuer: a("funder"),
            amount_atoms: U,
        },
        Operation::ChannelCommit {
            id: p(1),
            owner: a("alice"),
            channel: a("channel"),
            side: ChannelSide::A,
            amount_atoms: U,
        },
        Operation::Transfer {
            id: p(1),
            from: escrow(&policy, p(2)).unwrap(),
            to: a("alice"),
            amount_atoms: U,
        },
        Operation::Transfer {
            id: p(1),
            from: a("alice"),
            to: escrow(&policy, p(2)).unwrap(),
            amount_atoms: U,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let before = l.clone();
        assert!(l.apply(tx(90 + n as u8), op).is_err());
        assert_eq!(l, before);
    }
    let bytes = l.snapshot_bytes().unwrap();
    assert_eq!(
        Ledger::from_snapshot_bytes_with_native_policy(&bytes, &policy).unwrap(),
        l
    );
}
#[test]
fn wrong_actor_context_underfunded_bootstrap_and_nonatom_quote_fail_atomically() {
    let (mut l, policy) = fixture(TradeLayerMainnet::Bitcoin, NativeDriver::MonotonicDeltaV1);
    let mut wrong = policy.clone();
    wrong.bindings[0].funding.target_atoms += 1;
    let before = l.clone();
    assert!(l.adopt_native_entitlements_unverified(&wrong).is_err());
    assert_eq!(l, before);
    l.adopt_native_entitlements_unverified(&policy).unwrap();
    let mut wrong = request(80, U);
    wrong.claimed_actor = a("alice");
    let before = l.clone();
    assert!(l
        .apply_native_target_unverified(&wrong, context(&policy, 1))
        .is_err());
    assert_eq!(l, before);
    assert!(l
        .apply_native_target_unverified(&request(80, U), context(&policy, 0))
        .is_err());
    assert_eq!(l, before);
    assert_eq!(
        source_vesting_quote(U, U, 3 * U),
        Err(NativeError::UnrepresentableQuote)
    );
    assert_eq!(
        source_marker_drag(5 * U, 6 * U, 18_014_398_509_481_984 * U),
        Err(NativeError::UnsupportedNumericDomain)
    );
}
#[test]
fn canonical_policy_and_native_checkpoint_reject_truncation_and_extra_bytes() {
    let (mut l, policy) = fixture(TradeLayerMainnet::Bitcoin, NativeDriver::MonotonicDeltaV1);
    let policybytes = canonical_policy_bytes(&policy).unwrap();
    assert_eq!(parse_policy(&policybytes).unwrap(), policy);
    l.adopt_native_entitlements_unverified(&policy).unwrap();
    let bytes = l.snapshot_bytes().unwrap();
    for len in [0, 3, 4, 7, bytes.len() / 2, bytes.len() - 1] {
        assert!(Ledger::from_snapshot_bytes_with_native_policy(&bytes[..len], &policy).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(Ledger::from_snapshot_bytes_with_native_policy(&trailing, &policy).is_err());
    assert_eq!(
        policy.require_authenticated(),
        Err(NativeError::Unauthenticated)
    );
    assert_eq!(
        policy.require_source_schedule(),
        Err(NativeError::UnavailableSourceSchedule)
    );
}
#[test]
fn exact_quote_rejects_lossy_post_credit_storage_before_any_payout() {
    let (mut l, mut policy) = fixture(TradeLayerMainnet::Bitcoin, NativeDriver::MonotonicDeltaV1);
    let huge = 9_007_199_254_740_992 * U;
    l.apply(
        tx(70),
        Operation::Mint {
            id: p(1),
            issuer: a("funder"),
            to: a("alice"),
            amount_atoms: huge,
        },
    )
    .unwrap();
    policy.assets[0].expected_initial_issued_atoms += huge;
    policy.bootstrap_ledger_commitment = bootstrap_commitment(&l.snapshot_bytes().unwrap());
    l.adopt_native_entitlements_unverified(&policy).unwrap();
    let before = l.snapshot_bytes().unwrap();
    assert_eq!(
        l.apply_native_target_unverified(&request(80, 2 * U), context(&policy, 1)),
        Err(LedgerError::NativeEntitlement(
            NativeError::UnsupportedNumericDomain
        ))
    );
    assert_eq!(l.snapshot_bytes().unwrap(), before);
    assert!(!l.is_processed(tx(80)));
}
fn crc(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in bytes {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = (c >> 1) ^ if c & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    !c
}
fn checked(mut payload: Vec<u8>) -> Vec<u8> {
    let c = crc(&payload);
    payload.extend_from_slice(&c.to_be_bytes());
    payload
}
#[test]
fn forged_foreign_property_in_real_native_escrow_rejects_with_valid_nested_checksums() {
    let (mut l, mut policy) = fixture(TradeLayerMainnet::Bitcoin, NativeDriver::MonotonicDeltaV1);
    l.apply(
        tx(60),
        Operation::CreateProperty {
            id: p(9),
            issuer: a("alice"),
            kind: PropertyKind::Fixed,
            initial_atoms: 5 * U,
        },
    )
    .unwrap();
    policy.bootstrap_ledger_commitment = bootstrap_commitment(&l.snapshot_bytes().unwrap());
    l.adopt_native_entitlements_unverified(&policy).unwrap();
    let bytes = l.snapshot_bytes().unwrap();
    let size = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let inner = &bytes[8..8 + size];
    let mut needle = 9u32.to_be_bytes().to_vec();
    needle.push(5);
    needle.extend_from_slice(b"alice");
    needle.extend_from_slice(&(5 * U).to_be_bytes());
    let at = inner
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap();
    let to = escrow(&policy, p(2)).unwrap();
    let mut replacement = 9u32.to_be_bytes().to_vec();
    replacement.push(to.as_str().len() as u8);
    replacement.extend_from_slice(to.as_str().as_bytes());
    replacement.extend_from_slice(&(5 * U).to_be_bytes());
    let mut forgedinner = inner[..at].to_vec();
    forgedinner.extend(replacement);
    forgedinner.extend_from_slice(&inner[at + needle.len()..inner.len() - 4]);
    let forgedinner = checked(forgedinner);
    let mut payload = b"TLL\x05".to_vec();
    payload.extend_from_slice(&(forgedinner.len() as u32).to_be_bytes());
    payload.extend(forgedinner);
    payload.extend_from_slice(&bytes[8 + size..bytes.len() - 4]);
    let forged = checked(payload);
    assert!(Ledger::from_snapshot_bytes_with_native_policy(&forged, &policy).is_err());
}
