use std::str::FromStr;
use tradelayer_core::decimal::{Decimal, MAX_DECIMAL_DIGITS};
use tradelayer_core::ledger::PropertyId;
use tradelayer_core::vaults::{
    parse_legacy_synthetic_token_id, CollateralMovement, SyntheticTokenId, VaultError, VaultId,
    VaultRedemption, VaultRegistry, VaultUpdate, MAX_VAULTS,
};

fn d(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}
fn id(value: &str) -> VaultId {
    value.parse().unwrap()
}
fn property(value: u32) -> PropertyId {
    PropertyId::try_from(value).unwrap()
}
fn update(contracts: &str, margin: &str, available: &str, outstanding: &str) -> VaultUpdate {
    VaultUpdate {
        contracts_delta: d(contracts),
        margin_delta: d(margin),
        available_delta: d(available),
        outstanding_delta: d(outstanding),
    }
}

#[test]
fn actual_pinned_js_lifecycle_and_bignumber_query_vectors() {
    let mut registry = VaultRegistry::new();
    let mut operations = 0;
    let mut queries = 0;
    for line in include_str!("../fixtures/vault_vectors.tsv").lines() {
        let c: Vec<_> = line.split('\t').collect();
        match c[0] {
            "C" => {
                registry.create_vault(id(c[1])).unwrap();
                operations += 1;
            }
            "U" | "S" => {
                registry
                    .update_vault(id(c[1]), &update(c[2], c[3], c[4], c[5]))
                    .unwrap();
                operations += 1;
            }
            "R" => {
                registry
                    .redeem(
                        id(c[1]),
                        &VaultRedemption {
                            contracts_delta: d(c[2]),
                            margin_released: d(c[3]),
                            available_released: d(c[4]),
                            amount_redeemed: d(c[5]).negated(),
                        },
                    )
                    .unwrap();
                operations += 1;
            }
            "Q" => {
                let result = if c[1] == "B" {
                    registry.total_collateral_for_property(property(c[2].parse().unwrap()))
                } else if c[2].starts_with("s-") {
                    registry.outstanding_for_vault(id(c[2]))
                } else {
                    registry.total_outstanding_for_property(property(c[2].parse().unwrap()))
                }
                .unwrap();
                assert_eq!(result, d(c[3]), "{line}");
                queries += 1;
            }
            _ => {}
        }
    }
    assert_eq!((operations, queries), (9, 13));
    let first = registry.vault(id("s-1-2")).unwrap();
    assert_eq!(
        (
            &first.contracts,
            &first.margin,
            &first.available,
            &first.outstanding
        ),
        (&d("-2"), &d("3"), &d("4"), &d("6"))
    );
}

#[test]
fn canonical_ids_round_trip_and_reject_ambiguous_spelling() {
    for value in ["s-1-2", "s-4294967295-4294967295"] {
        assert_eq!(id(value).to_string(), value);
        let token = SyntheticTokenId::for_vault(id(value));
        assert_eq!(
            token.to_string().parse::<SyntheticTokenId>().unwrap(),
            token
        );
    }
    for value in [
        "s-0-2",
        "s-1-0",
        "s-01-2",
        "s-1-02",
        "s-+1-2",
        "s-1-2 ",
        "s-1-2-3",
        "1-2",
        "S-1-2",
        "s-4294967296-2",
        "s--2",
        "s-1-٢",
    ] {
        assert_eq!(
            value.parse::<VaultId>(),
            Err(VaultError::InvalidIdentifier),
            "{value}"
        );
    }
    for value in ["s-1-2", "1-0", "01-2", "1-2-3", "-1-2", "1-٢"] {
        assert_eq!(
            value.parse::<SyntheticTokenId>(),
            Err(VaultError::InvalidIdentifier),
            "{value}"
        );
    }
    assert_eq!(
        VaultId::new(property(1), 0),
        Err(VaultError::InvalidIdentifier)
    );
}

#[test]
fn legacy_parser_keeps_effective_async_three_string_parts_quirk() {
    assert!(parse_legacy_synthetic_token_id("1-2").is_err());
    let parts = parse_legacy_synthetic_token_id("s-1-2").unwrap();
    assert_eq!(
        (
            parts.underlying_property_id,
            parts.hedge_contract_id,
            parts.vault_id
        ),
        ("s", "1", "2")
    );
    let empty = parse_legacy_synthetic_token_id("--").unwrap();
    assert_eq!(
        (
            empty.underlying_property_id,
            empty.hedge_contract_id,
            empty.vault_id
        ),
        ("", "", "")
    );
    assert!(parse_legacy_synthetic_token_id("1-2-3-4").is_err());
}

#[test]
fn complete_create_issue_reserve_release_redeem_close_lifecycle() {
    let vault_id = id("s-1-2");
    let token = SyntheticTokenId::for_vault(vault_id);
    let mut registry = VaultRegistry::new();
    registry.create_vault(vault_id).unwrap();
    registry.register_synthetic_token(token, vault_id).unwrap();
    registry.register_synthetic_token(token, vault_id).unwrap();
    assert_eq!((registry.vault_count(), registry.synthetic_count()), (1, 1));
    registry
        .update_vault(vault_id, &update("-10", "4", "6", "100"))
        .unwrap();
    let collateral = registry
        .synthetic_vault(token)
        .unwrap()
        .collateral()
        .unwrap();
    assert_eq!(collateral.property_id, property(1));
    assert_eq!(collateral.total, d("10"));
    registry
        .move_collateral(vault_id, CollateralMovement::Reserve, &d("3"))
        .unwrap();
    let collateral = registry.vault(vault_id).unwrap().collateral().unwrap();
    assert_eq!(
        (collateral.margin, collateral.available, collateral.total),
        (d("7"), d("3"), d("10"))
    );
    registry
        .move_collateral(vault_id, CollateralMovement::Release, &d("2.5"))
        .unwrap();
    let collateral = registry.vault(vault_id).unwrap().collateral().unwrap();
    assert_eq!(
        (collateral.margin, collateral.available, collateral.total),
        (d("4.5"), d("5.5"), d("10"))
    );
    registry
        .redeem(
            vault_id,
            &VaultRedemption {
                contracts_delta: d("10"),
                margin_released: d("4.5"),
                available_released: d("5.5"),
                amount_redeemed: d("100"),
            },
        )
        .unwrap();
    registry.close_vault(vault_id).unwrap();
    assert_eq!((registry.vault_count(), registry.synthetic_count()), (0, 0));
    assert_eq!(
        registry.synthetic_vault(token),
        Err(VaultError::SyntheticNotFound(token))
    );
    registry.create_vault(vault_id).unwrap();
    assert_eq!(
        registry.vaults().map(|v| v.id).collect::<Vec<_>>(),
        vec![vault_id]
    );
}

#[test]
fn every_invalid_update_and_redemption_is_atomic() {
    let vault_id = id("s-1-2");
    let mut registry = VaultRegistry::new();
    registry
        .update_vault(vault_id, &update("-10", "4", "6", "100"))
        .unwrap();
    let baseline = registry.clone();
    for (change, error) in [
        (update("1", "-5", "0", "0"), VaultError::NegativeMargin),
        (update("1", "0", "-7", "0"), VaultError::NegativeAvailable),
        (
            update("1", "0", "0", "-101"),
            VaultError::NegativeOutstanding,
        ),
    ] {
        assert_eq!(registry.update_vault(vault_id, &change), Err(error));
        assert_eq!(registry, baseline);
    }
    for (margin, available, redeemed, error) in [
        ("-1", "0", "0", VaultError::NegativeAmount),
        ("0", "-1", "0", VaultError::NegativeAmount),
        ("0", "0", "-1", VaultError::NegativeAmount),
        ("5", "0", "0", VaultError::NegativeMargin),
        ("0", "7", "0", VaultError::NegativeAvailable),
        ("0", "0", "101", VaultError::NegativeOutstanding),
    ] {
        assert_eq!(
            registry.redeem(
                vault_id,
                &VaultRedemption {
                    contracts_delta: d("10"),
                    margin_released: d(margin),
                    available_released: d(available),
                    amount_redeemed: d(redeemed),
                }
            ),
            Err(error)
        );
        assert_eq!(registry, baseline);
    }
    assert_eq!(
        registry.create_vault(vault_id),
        Err(VaultError::AlreadyExists(vault_id))
    );
    assert_eq!(
        registry.close_vault(vault_id),
        Err(VaultError::NotEmpty(vault_id))
    );
    assert_eq!(registry, baseline);
}

#[test]
fn failed_auto_create_and_failed_batch_leave_no_partial_vaults() {
    let mut registry = VaultRegistry::new();
    assert_eq!(
        registry.update_vault(id("s-1-2"), &update("-10", "0", "0", "-1")),
        Err(VaultError::NegativeOutstanding)
    );
    assert_eq!(registry.vault_count(), 0);
    registry
        .update_vault(id("s-1-2"), &update("-1", "2", "3", "4"))
        .unwrap();
    let baseline = registry.clone();
    assert_eq!(
        registry.update_batch(&[
            (id("s-2-2"), update("-1", "2", "3", "4")),
            (id("s-1-2"), update("1", "-3", "0", "0")),
        ]),
        Err(VaultError::NegativeMargin)
    );
    assert_eq!(registry, baseline);
    registry
        .update_batch(&[
            (id("s-2-2"), update("-1", "2", "3", "4")),
            (id("s-1-2"), update("1", "-2", "0", "0")),
        ])
        .unwrap();
    assert_eq!(registry.vault_count(), 2);
}

#[test]
fn collateral_movement_failure_is_atomic_and_never_changes_supply_or_hedge() {
    let vault_id = id("s-1-2");
    let mut registry = VaultRegistry::new();
    registry
        .update_vault(vault_id, &update("-11", "2", "3", "42"))
        .unwrap();
    let baseline = registry.clone();
    for (movement, amount, expected) in [
        (
            CollateralMovement::Reserve,
            "4",
            VaultError::NegativeAvailable,
        ),
        (CollateralMovement::Release, "3", VaultError::NegativeMargin),
        (
            CollateralMovement::Reserve,
            "-1",
            VaultError::NegativeAmount,
        ),
    ] {
        assert_eq!(
            registry.move_collateral(vault_id, movement, &d(amount)),
            Err(expected)
        );
        assert_eq!(registry, baseline);
    }
    registry
        .move_collateral(
            vault_id,
            CollateralMovement::Reserve,
            &d("0.0000000000000000001"),
        )
        .unwrap();
    let vault = registry.vault(vault_id).unwrap();
    assert_eq!(
        (&vault.contracts, &vault.outstanding),
        (&d("-11"), &d("42"))
    );
    assert_eq!(vault.collateral().unwrap().total, d("5"));
}

#[test]
fn missing_vault_operations_and_conflicting_token_links_are_rejected() {
    let mut registry = VaultRegistry::new();
    let vault_id = id("s-1-2");
    let token = SyntheticTokenId::for_vault(vault_id);
    let zero = VaultRedemption {
        contracts_delta: d("0"),
        margin_released: d("0"),
        available_released: d("0"),
        amount_redeemed: d("0"),
    };
    assert_eq!(
        registry.redeem(vault_id, &zero),
        Err(VaultError::NotFound(vault_id))
    );
    assert_eq!(
        registry.move_collateral(vault_id, CollateralMovement::Reserve, &d("0")),
        Err(VaultError::NotFound(vault_id))
    );
    assert_eq!(
        registry.close_vault(vault_id),
        Err(VaultError::NotFound(vault_id))
    );
    assert_eq!(
        registry.register_synthetic_token(token, vault_id),
        Err(VaultError::NotFound(vault_id))
    );
    registry.create_vault(vault_id).unwrap();
    let baseline = registry.clone();
    assert_eq!(
        registry.register_synthetic_token(token, id("s-1-3")),
        Err(VaultError::SyntheticVaultMismatch)
    );
    assert_eq!(registry, baseline);
    assert_eq!(
        registry.synthetic_vault(token),
        Err(VaultError::SyntheticNotFound(token))
    );
}

#[test]
fn aggregate_rounds_once_after_sum_and_keeps_property_namespaces_separate() {
    let mut registry = VaultRegistry::new();
    registry
        .update_vault(id("s-1-2"), &update("0", "0.000000004", "0", "0.000000004"))
        .unwrap();
    registry
        .update_vault(id("s-1-3"), &update("0", "0.000000004", "0", "0.000000004"))
        .unwrap();
    registry
        .update_vault(id("s-11-2"), &update("0", "99", "1", "1000"))
        .unwrap();
    assert_eq!(
        registry.total_collateral_for_property(property(1)).unwrap(),
        d("0.00000001")
    );
    assert_eq!(
        registry
            .total_outstanding_for_property(property(1))
            .unwrap(),
        d("0.00000001")
    );
    assert_eq!(registry.outstanding_for_vault(id("s-1-2")).unwrap(), d("0"));
    assert_eq!(
        registry
            .total_collateral_for_property(property(11))
            .unwrap(),
        d("100")
    );
}

#[test]
fn large_integer_and_tiny_decimal_collateral_remain_exact() {
    let mut registry = VaultRegistry::new();
    let amount = "340282366920938463463374607431768211456.0000000000000000001";
    registry
        .update_vault(
            id("s-1-2"),
            &update(
                "-1000000000000000000000000000000000000000",
                amount,
                "0",
                amount,
            ),
        )
        .unwrap();
    let vault = registry.vault(id("s-1-2")).unwrap();
    assert_eq!(vault.collateral().unwrap().total, d(amount));
    registry
        .redeem(
            id("s-1-2"),
            &VaultRedemption {
                contracts_delta: d("1000000000000000000000000000000000000000"),
                margin_released: d(amount),
                available_released: d("0"),
                amount_redeemed: d(amount),
            },
        )
        .unwrap();
    registry.close_vault(id("s-1-2")).unwrap();
}

#[test]
fn nonempty_close_guard_checks_each_balance_independently() {
    let vault_id = id("s-1-2");
    for change in [
        update("-1", "0", "0", "0"),
        update("0", "1", "0", "0"),
        update("0", "0", "1", "0"),
        update("0", "0", "0", "1"),
    ] {
        let mut registry = VaultRegistry::new();
        registry.update_vault(vault_id, &change).unwrap();
        let token = SyntheticTokenId::for_vault(vault_id);
        registry.register_synthetic_token(token, vault_id).unwrap();
        let baseline = registry.clone();
        assert_eq!(
            registry.close_vault(vault_id),
            Err(VaultError::NotEmpty(vault_id))
        );
        assert_eq!(registry, baseline);
    }
}

#[test]
fn capacity_is_enforced_before_create_or_update_and_freed_on_close() {
    let mut registry = VaultRegistry::new();
    for contract in 1..=u32::try_from(MAX_VAULTS).unwrap() {
        registry
            .create_vault(VaultId::new(property(1), contract).unwrap())
            .unwrap();
    }
    let next = VaultId::new(property(1), u32::try_from(MAX_VAULTS + 1).unwrap()).unwrap();
    assert_eq!(
        registry.create_vault(next),
        Err(VaultError::CapacityExceeded)
    );
    assert_eq!(
        registry.update_vault(next, &update("0", "0", "0", "0")),
        Err(VaultError::CapacityExceeded)
    );
    assert_eq!(registry.vault_count(), MAX_VAULTS);
    assert!(registry.vault(next).is_none());
    // Updating an existing vault remains possible at capacity.
    registry
        .update_vault(id("s-1-1"), &update("0", "0", "0", "0"))
        .unwrap();
    registry.close_vault(id("s-1-1")).unwrap();
    registry.create_vault(next).unwrap();
    assert_eq!(registry.vault_count(), MAX_VAULTS);
}

#[test]
fn property_range_uses_canonical_order_and_covers_both_contract_boundaries() {
    let mut registry = VaultRegistry::new();
    for text in ["s-2-2", "s-1-4294967295", "s-3-1", "s-1-1", "s-1-5"] {
        registry.create_vault(id(text)).unwrap();
    }
    assert_eq!(
        registry
            .vaults_for_property(property(1))
            .map(|vault| vault.id)
            .collect::<Vec<_>>(),
        vec![id("s-1-1"), id("s-1-5"), id("s-1-4294967295")]
    );
    assert_eq!(registry.vaults_for_property(property(99)).count(), 0);
}

#[test]
fn checked_arithmetic_resource_errors_preserve_all_state() {
    let vault_id = id("s-1-2");
    let huge = "9".repeat(MAX_DECIMAL_DIGITS);
    let mut registry = VaultRegistry::new();
    registry
        .update_vault(vault_id, &update(&huge, &huge, "1", &huge))
        .unwrap();
    let baseline = registry.clone();
    assert_eq!(
        registry.update_vault(vault_id, &update("0", "1", "0", "0")),
        Err(VaultError::Arithmetic)
    );
    assert_eq!(registry, baseline);
    assert_eq!(
        registry.redeem(
            vault_id,
            &VaultRedemption {
                contracts_delta: d("1"),
                margin_released: d("0"),
                available_released: d("0"),
                amount_redeemed: d("0"),
            }
        ),
        Err(VaultError::Arithmetic)
    );
    assert_eq!(registry, baseline);
    assert_eq!(
        registry.move_collateral(vault_id, CollateralMovement::Reserve, &d("1")),
        Err(VaultError::Arithmetic)
    );
    assert_eq!(registry, baseline);
    assert_eq!(
        registry.update_batch(&[
            (id("s-2-2"), update("-1", "2", "3", "4")),
            (vault_id, update("0", "1", "0", "0")),
        ]),
        Err(VaultError::Arithmetic)
    );
    assert_eq!(registry, baseline);
    assert_eq!(
        registry.total_collateral_for_property(property(1)),
        Err(VaultError::Arithmetic)
    );
    assert_eq!(registry, baseline);
}
