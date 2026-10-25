use std::str::FromStr;
use tradelayer_core::contract_registry::{
    calculate_average_price, calculate_funding_amount, first_oracle_record, first_price_pair,
    generate_contracts, has_positive_open_position, next_id_from_map_keys, CollateralBalance,
    CollateralMovement, ContractAsset, ContractError, ContractId, ContractParameters,
    ContractRegistry, ContractType, FeeInformation, FundingEvent, MarginRequest, ReserveRequest,
    TickerContext, MAX_EXPIRIES_PER_SERIES, MAX_JS_SAFE_HEIGHT,
};
use tradelayer_core::decimal::{Decimal, DecimalError};
use tradelayer_core::ledger::AccountId;

fn d(text: &str) -> Decimal {
    Decimal::from_str(text).unwrap()
}
fn id(value: u32) -> ContractId {
    ContractId::try_from(value).unwrap()
}
fn account(text: &str) -> AccountId {
    AccountId::try_from(text).unwrap()
}
fn optional(text: &str) -> Option<u32> {
    if text == "null" {
        None
    } else {
        Some(text.parse().unwrap())
    }
}
fn asset(text: &str) -> ContractAsset {
    ContractAsset::from(text.parse::<u32>().unwrap())
}
fn blank(text: String) -> String {
    if text.is_empty() {
        "-".to_owned()
    } else {
        text
    }
}
fn ids(values: &[ContractId]) -> String {
    blank(
        values
            .iter()
            .map(|id| id.get().to_string())
            .collect::<Vec<_>>()
            .join(","),
    )
}
fn generated(values: &[tradelayer_core::contract_registry::GeneratedContract]) -> String {
    blank(
        values
            .iter()
            .map(|value| format!("{}:{}", value.id, value.expiration_block))
            .collect::<Vec<_>>()
            .join(";"),
    )
}
fn decode_hex(text: &str) -> String {
    let bytes = text
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    String::from_utf8(bytes).unwrap()
}
fn movements(values: &[CollateralMovement]) -> String {
    blank(
        values
            .iter()
            .map(|value| {
                format!(
                    "{}:{}:{}:{}:{}",
                    value.available, value.reserved, value.margin, value.channel, value.reason
                )
            })
            .collect::<Vec<_>>()
            .join(";"),
    )
}

fn create_from_fixture(registry: &mut ContractRegistry, input: &str) -> ContractId {
    let c: Vec<_> = input.split('|').collect();
    let pairs = if c[13] == "-" {
        Vec::new()
    } else {
        c[13]
            .split(';')
            .map(|pair| {
                let p: Vec<_> = pair.split(':').collect();
                (asset(p[0]), asset(p[1]))
            })
            .collect()
    };
    let params = ContractParameters {
        native: c[0] == "1",
        inverse: c[1] == "1",
        underlying_oracle_id: optional(c[2]),
        notional_property_id: asset(c[3]),
        notional_value: d(c[4]),
        collateral_property_id: asset(c[5]),
        leverage: d(c[6]),
        expiry_period: c[7].parse().unwrap(),
        series: c[8].parse().unwrap(),
        fee: d(c[9]),
        whitelist: optional(c[10]),
        ticker: if c[11] == "-" {
            None
        } else {
            Some(c[11].to_owned())
        },
        on_chain_data: pairs,
    };
    let ticker = |value: &str| match value {
        "0" => "ignored",
        "1" => "USD",
        "2" => "GOLD",
        "3" => "SILVER",
        _ => panic!("unknown ticker"),
    };
    registry
        .create_contract_series(
            account("issuer"),
            params,
            TickerContext {
                notional_ticker: Some(ticker(c[3])),
                collateral_ticker: Some(ticker(c[5])),
                oracle_ticker: Some("BTC"),
            },
            c[12].parse().unwrap(),
        )
        .unwrap()
}

fn summary(registry: &ContractRegistry, contract_id: ContractId) -> String {
    let series = registry.get_contract_series(contract_id).unwrap();
    let p = &series.parameters;
    [
        contract_id.get().to_string(),
        series.ticker.clone(),
        series.issuer.as_str().to_owned(),
        u8::from(p.native).to_string(),
        u8::from(p.inverse).to_string(),
        p.underlying_oracle_id
            .map_or_else(|| "null".to_owned(), |value| value.to_string()),
        p.notional_property_id.get().to_string(),
        p.notional_value.to_string(),
        p.collateral_property_id.get().to_string(),
        p.leverage.to_string(),
        p.expiry_period.to_string(),
        p.series.to_string(),
        p.fee.to_string(),
        p.whitelist.unwrap().to_string(),
        generated(&series.unexpired),
        format!(
            "{}:{}:{}:{}",
            series.amm.position,
            series.amm.max_position,
            series.amm.max_quote_size,
            series.amm.contract_type.get()
        ),
    ]
    .join("|")
}

#[test]
fn actual_source_vectors() {
    let mut registry = ContractRegistry::new();
    let mut checked = 0;
    let mut source_quirks = 0;
    for line in include_str!("../fixtures/contract_registry_vectors.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4, "{line}");
        let kind = columns[0];
        let input = columns[2];
        let expected = columns[3];
        let c: Vec<_> = input.split('|').collect();
        let actual = match kind {
            "create" => {
                let new_id = create_from_fixture(&mut registry, input);
                summary(&registry, new_id)
            }
            "query" => match input {
                "all" => ids(&registry
                    .all_contracts()
                    .map(|series| series.id)
                    .collect::<Vec<_>>()),
                "perpetual" => ids(&registry.all_perpetual_contracts()),
                "next" => registry.next_id().unwrap().get().to_string(),
                "next-map" => next_id_from_map_keys(["12-tail", "bad", "3", "+0x10-tail"])
                    .unwrap()
                    .get()
                    .to_string(),
                _ => panic!("unknown query"),
            },
            "collateral" => ids(&registry.contracts_for_collateral(asset(input))),
            "lookup" => registry
                .get_contract_info(&decode_hex(input))
                .map_or_else(|| "null".to_owned(), |series| series.id.get().to_string()),
            "duplicate" => u8::from(registry.is_duplicate_native_contract(
                asset(c[0]),
                (asset(c[1]), asset(c[2])),
                asset(c[3]),
            ))
            .to_string(),
            "inverse-native" => blank(
                registry
                    .lookup_inverse_native_by_notional_pid(asset(input))
                    .iter()
                    .map(|matched| {
                        format!(
                            "{}:{}:{}:{}",
                            matched.contract_id.get(),
                            matched.series_id.get(),
                            matched.symbol,
                            matched.notional_property_id.get()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(";"),
            ),
            "flags" => {
                let contract_id = id(input.parse().unwrap());
                // Source null && predicate returns null on an absent series.
                let oracle = if registry.get_contract_series(contract_id).is_none() {
                    "null".to_owned()
                } else {
                    u8::from(registry.is_oracle_contract(contract_id)).to_string()
                };
                [
                    u8::from(registry.is_native_contract(contract_id)).to_string(),
                    oracle,
                    u8::from(registry.is_inverse(contract_id)).to_string(),
                ]
                .join("|")
            }
            "amm" => {
                let amm = registry
                    .instantiate_amm_pool(id(input.parse().unwrap()))
                    .unwrap();
                format!(
                    "{}|{}|{}|{}",
                    amm.position(),
                    amm.max_position(),
                    amm.max_quote_size(),
                    amm.contract_id()
                )
            }
            "metadata" => {
                let contract_id = id(input.parse().unwrap());
                let kind = match registry.contract_type(contract_id).unwrap() {
                    ContractType::Native => "native",
                    ContractType::Oracle => "oracle",
                };
                format!(
                    "{}|{}|{}",
                    kind,
                    registry.collateral_id(contract_id).unwrap().get(),
                    registry
                        .oracle_id(contract_id)
                        .unwrap()
                        .map_or_else(|| "null".to_owned(), |oracle| oracle.to_string())
                )
            }
            "mod-flag" => {
                registry.set_mod_flag(input == "1");
                u8::from(registry.mod_flag()).to_string()
            }
            "notional" => {
                let contract_id = id(c[0].parse().unwrap());
                let mark = c[1].parse::<Decimal>().ok();
                let fallback = registry
                    .get_contract_series(contract_id)
                    .and_then(|series| match series.parameters.underlying_oracle_id {
                        Some(0) => Some(d("9")),
                        Some(5) => Some(d("3")),
                        Some(7) => Some(d("0")),
                        _ => None,
                    });
                let value = registry
                    .notional_value(contract_id, mark.as_ref(), fallback.as_ref())
                    .unwrap();
                format!("{}|{}", value.notional_value, value.notional_per_contract)
            }
            "margin" => match registry.initial_margin(id(c[0].parse().unwrap()), &d(c[1])) {
                Ok(value) => value.to_string(),
                Err(ContractError::Arithmetic(DecimalError::DivisionByZero)) => {
                    assert!(
                        expected.ends_with("Infinity") || expected == "NaN",
                        "{line}"
                    );
                    expected.to_owned()
                }
                Err(error) => panic!("{line}: {error}"),
            },
            "collateral-value" => registry
                .collateral_value(
                    id(input.parse().unwrap()),
                    Some(&d("17.25")),
                    Some(&d("23.75")),
                )
                .unwrap()
                .to_string(),
            "generate" => generated(
                &generate_contracts(
                    c[0].parse().unwrap(),
                    c[1].parse().unwrap(),
                    id(c[2].parse().unwrap()),
                    c[3].parse().unwrap(),
                )
                .unwrap(),
            ),
            "average" => {
                let prices = if input == "-" {
                    Vec::new()
                } else {
                    c.iter().map(|value| value.parse().unwrap()).collect()
                };
                d(&calculate_average_price(&prices).to_string()).to_string()
            }
            "funding-amount" => d(&calculate_funding_amount(
                c[0].parse().unwrap(),
                c[1].parse().unwrap(),
            )
            .to_string())
            .to_string(),
            "reserve" => {
                let balance = CollateralBalance {
                    available: d(c[0]),
                    reserved: d(c[1]),
                    margin: Decimal::zero(),
                };
                let request = ReserveRequest {
                    contract_id: id(1),
                    amount: d(c[2]),
                    price: d(c[3]),
                    in_process: c[4] == "1",
                };
                match registry
                    .plan_collateral_to_reserve(&balance, &request)
                    .unwrap()
                {
                    Some(plan) => format!(
                        "{}|{}",
                        plan.position_initial_margin,
                        movements(&plan.movements)
                    ),
                    None => "null|-".to_owned(),
                }
            }
            "transfer" => {
                let balance = CollateralBalance {
                    available: d(c[0]),
                    reserved: d(c[1]),
                    margin: d(c[2]),
                };
                let request = MarginRequest {
                    sender: account(c[9]),
                    contract_id: id(1),
                    amount: d(c[5]),
                    price: d("2"),
                    order_price: d(c[4]),
                    buy_side: c[12] == "1",
                    initial_margin: d(c[3]),
                    maker: c[6] == "1",
                    channel: c[7] == "1",
                    fee_information: FeeInformation {
                        buy_fee_from_reserve: c[10] != "0",
                        sell_fee_from_reserve: c[11] != "0",
                        buyer_fee: d(c[10]),
                        seller_fee: d(c[11]),
                    },
                };
                match registry.plan_collateral_to_margin(&balance, Some(&d(c[8])), &request) {
                    Ok(plan) => format!(
                        "{}|{}|{}",
                        plan.position_initial_margin,
                        plan.channel_initial_margin_debit,
                        movements(&plan.movements)
                    ),
                    Err(ContractError::InsufficientCollateral) => "insufficient".to_owned(),
                    Err(error) => panic!("{line}: {error}"),
                }
            }
            "quirk" => {
                source_quirks += 1;
                continue;
            }
            _ => panic!("unknown kind {kind}"),
        };
        assert_eq!(actual, expected, "{} {}", kind, columns[1]);
        checked += 1;
    }
    assert_eq!(checked, 208);
    assert_eq!(source_quirks, 11);
}

fn sample_registry() -> ContractRegistry {
    let mut registry = ContractRegistry::new();
    create_from_fixture(&mut registry, "1|1|0|0|1|1|10|0|3|0.01|null|-|100|0:1");
    registry
}

#[test]
fn typed_restoration_queries_and_defaults_are_consistent() {
    let mut registry = sample_registry();
    assert!(registry.is_valid_series_id(id(1)));
    assert_eq!(registry.contract_type(id(1)), Ok(ContractType::Native));
    assert_eq!(registry.collateral_id(id(1)), Ok(ContractAsset::from(1)));
    assert_eq!(registry.oracle_id(id(1)), Ok(Some(0)));
    assert_eq!(
        registry.amm_configuration(id(1)).unwrap().max_quote_size,
        d("10")
    );
    assert_eq!(
        first_price_pair(registry.get_contract_series(id(1)).unwrap()).unwrap(),
        (ContractAsset::BaseChain, ContractAsset::from(1))
    );
    assert_eq!(registry.calculate_funding_rate(id(1)), Ok(0.0));
    assert_eq!(registry.calculate_funding_rate(id(99)), Ok(0.0));
    registry.set_mod_flag(true);
    assert!(registry.mod_flag());
    assert_eq!(registry.contract_type(id(99)), Err(ContractError::NotFound));
    assert_eq!(registry.collateral_id(id(99)), Err(ContractError::NotFound));
    assert!(!registry.is_inverse(id(99)));
    assert_eq!(
        registry
            .notional_value(id(99), None, None)
            .unwrap()
            .notional_value,
        Decimal::zero()
    );
    let mut loaded = registry.get_contract_series(id(1)).unwrap().clone();
    assert_eq!(
        registry.insert_loaded(loaded.clone()),
        Err(ContractError::DuplicateIdentifier)
    );
    loaded.id = id(10);
    loaded.ticker = "Term".to_owned();
    loaded.parameters.expiry_period = 20;
    registry.insert_loaded(loaded.clone()).unwrap();
    loaded.id = id(3);
    registry.insert_loaded(loaded).unwrap();
    assert_eq!(
        ids(&registry
            .all_contracts()
            .map(|series| series.id)
            .collect::<Vec<_>>()),
        "1,10,3"
    );
    assert_eq!(registry.next_id(), Ok(id(11)));
}

#[test]
fn limits_fail_atomically_and_zero_expiry_preserves_duplicate_identifiers() {
    let mut registry = sample_registry();
    let before = registry.clone();
    let original = registry.get_contract_series(id(1)).unwrap().clone();
    let mut invalid = original.clone();
    invalid.id = id(2);
    invalid.ticker = "bad\nname".to_owned();
    assert_eq!(
        registry.insert_loaded(invalid),
        Err(ContractError::InvalidTicker)
    );
    assert_eq!(registry, before);
    let mut invalid = original.clone();
    invalid.id = id(2);
    invalid.unexpired[0].expiration_block = MAX_JS_SAFE_HEIGHT + 1;
    assert_eq!(
        registry.insert_loaded(invalid),
        Err(ContractError::HeightOutOfRange)
    );
    assert_eq!(registry, before);
    assert_eq!(
        generate_contracts(1, MAX_EXPIRIES_PER_SERIES as u32 + 1, id(1), 100),
        Err(ContractError::LimitExceeded)
    );
    assert_eq!(
        generate_contracts(1, 2, id(1), MAX_JS_SAFE_HEIGHT - 1),
        Err(ContractError::HeightOutOfRange)
    );
    assert_eq!(
        generate_contracts(0, 0, id(1), MAX_JS_SAFE_HEIGHT + 1),
        Err(ContractError::HeightOutOfRange)
    );
    assert_eq!(
        ContractId::try_from(0),
        Err(ContractError::InvalidIdentifier)
    );
    let duplicate = generate_contracts(0, 3, id(1), 100).unwrap();
    assert_eq!(duplicate[0], duplicate[2]);
}

#[test]
fn journal_order_and_source_numeric_helpers() {
    let mut registry = sample_registry();
    for (contract_id, funding_rate, block_height) in
        [(id(1), 0.01, 90), (id(2), -0.02, 80), (id(1), 0.03, 70)]
    {
        registry
            .save_funding_event(FundingEvent {
                contract_id,
                funding_rate,
                block_height,
            })
            .unwrap();
    }
    assert_eq!(
        registry
            .funding_events(id(1))
            .map(|event| event.block_height)
            .collect::<Vec<_>>(),
        [90, 70]
    );
    let before = registry.clone();
    assert_eq!(
        registry.save_funding_event(FundingEvent {
            contract_id: id(1),
            funding_rate: f64::NAN,
            block_height: 0
        }),
        Err(ContractError::NonFiniteNumber)
    );
    assert_eq!(registry, before);
    assert_eq!(
        first_oracle_record(&[(5, (1, 10)), (5, (999, 20))], 5),
        Some(&(1, 10))
    );
    assert_eq!(first_oracle_record(&[(5, 1)], 6), None);
    assert!(!has_positive_open_position(&[d("-1"), d("0")]));
    assert!(has_positive_open_position(&[d("-1"), d("0.01")]));
    assert_eq!(calculate_average_price(&[1e16, 1.0, -1e16]), 0.0);
    assert_eq!(calculate_average_price(&[1e16, -1e16, 1.0]), 1.0 / 3.0);
    assert!(calculate_funding_amount(f64::INFINITY, 0.0).is_nan());
}

#[test]
fn collateral_errors_leave_input_and_catalog_unchanged() {
    let registry = sample_registry();
    let balance = CollateralBalance {
        available: d("0"),
        reserved: d("1"),
        margin: d("0"),
    };
    let before = balance.clone();
    let catalog_before = registry.clone();
    let mut request = MarginRequest {
        sender: account("trader"),
        contract_id: id(1),
        amount: d("1"),
        price: d("2"),
        order_price: d("1"),
        buy_side: true,
        initial_margin: d("3"),
        maker: false,
        channel: false,
        fee_information: FeeInformation::default(),
    };
    assert_eq!(
        registry.plan_collateral_to_margin(&balance, None, &request),
        Err(ContractError::InsufficientCollateral)
    );
    assert_eq!(balance, before);
    assert_eq!(registry, catalog_before);
    request.maker = true;
    request.channel = true;
    assert_eq!(
        registry.plan_collateral_to_margin(&balance, None, &request),
        Err(ContractError::MissingChannelBalance)
    );
    request.channel = false;
    request.fee_information.buy_fee_from_reserve = true;
    request.fee_information.buyer_fee = d("4");
    assert_eq!(
        registry.plan_collateral_to_margin(&balance, None, &request),
        Err(ContractError::NegativeCollateral)
    );
    request.fee_information = FeeInformation::default();
    request.price = d("0");
    assert_eq!(
        registry.plan_collateral_to_margin(&balance, None, &request),
        Err(ContractError::Arithmetic(DecimalError::DivisionByZero))
    );
    assert_eq!(balance, before);
    assert_eq!(registry, catalog_before);
}

#[test]
fn staged_inverse_rounding_and_float_bridge_are_observable() {
    let mut registry = sample_registry();
    let mut series = registry.get_contract_series(id(1)).unwrap().clone();
    series.id = id(2);
    series.parameters.notional_value = d("100000000000000000000");
    registry.insert_loaded(series).unwrap();
    // 1/3 rounds at 20 places before multiplying by the large notional.
    assert_eq!(
        registry
            .notional_value(id(2), Some(&d("3")), None)
            .unwrap()
            .notional_value,
        d("33333333333333330000")
    );
    // Inverse initial margin uses positive ceiling, including tiny fractions.
    assert_eq!(registry.initial_margin(id(1), &d("3")), Ok(d("0.03333334")));
    assert_eq!(
        registry.initial_margin(id(1), &d("-3")),
        Ok(d("-0.03333333"))
    );
}

#[test]
fn native_inverse_invalid_mark_also_converts_notional_metadata_to_number() {
    let mut registry = sample_registry();
    let mut series = registry.get_contract_series(id(1)).unwrap().clone();
    series.id = id(2);
    series.parameters.underlying_oracle_id = None;
    series.parameters.notional_value = d("1.00000000000000000001");
    registry.insert_loaded(series.clone()).unwrap();
    for mark in [None, Some(d("0")), Some(d("-1"))] {
        let result = registry.notional_value(id(2), mark.as_ref(), None).unwrap();
        assert_eq!(result.notional_value, Decimal::zero());
        assert_eq!(result.notional_per_contract, d("1"));
    }
    let valid = registry.notional_value(id(2), Some(&d("1")), None).unwrap();
    assert_eq!(valid.notional_per_contract, d("1.00000000000000000001"));

    // The invalid-mark oracle-inverse branch keeps the original metadata.
    series.id = id(3);
    series.parameters.native = false;
    registry.insert_loaded(series.clone()).unwrap();
    let oracle = registry.notional_value(id(3), None, None).unwrap();
    assert_eq!(oracle.notional_value, Decimal::zero());
    assert_eq!(oracle.notional_per_contract, d("1.00000000000000000001"));

    // A valid fallback also bypasses native-inverse's numeric early return.
    series.id = id(4);
    series.parameters.native = true;
    series.parameters.underlying_oracle_id = Some(0);
    registry.insert_loaded(series).unwrap();
    let fallback = registry.notional_value(id(4), None, Some(&d("1"))).unwrap();
    assert_eq!(fallback.notional_per_contract, d("1.00000000000000000001"));
}
