use std::collections::VecDeque;
use tradelayer_core::ledger::{AccountId, PropertyId};
use tradelayer_core::vesting::*;

fn fixtures() -> impl Iterator<Item = Vec<&'static str>> {
    include_str!("../fixtures/vesting_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').collect())
}
fn n(value: &str) -> f64 {
    match value {
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        _ => value.parse().unwrap(),
    }
}
fn ns(value: &str) -> Vec<f64> {
    if value == "-" {
        vec![]
    } else {
        value.split(',').map(n).collect()
    }
}
fn close(actual: f64, expected: f64, context: &str) {
    if expected.is_nan() {
        assert!(actual.is_nan(), "{context}: {actual} expected NaN");
    } else if expected.is_infinite() {
        assert_eq!(actual, expected, "{context}");
    } else {
        let tolerance = 256.0 * f64::EPSILON * expected.abs().max(1e-6);
        assert!(
            (actual - expected).abs() <= tolerance,
            "{context}: {actual:.17e} != {expected:.17e}; tolerance {tolerance}"
        );
    }
}
fn address(value: &str) -> Option<AccountId> {
    (value != "undefined").then(|| AccountId::try_from(value).unwrap())
}
fn show_address(value: &Option<AccountId>) -> &str {
    value.as_ref().map_or("undefined", AccountId::as_str)
}
fn manager(chain: &str, test: bool, admin: Option<AccountId>) -> VestingManager {
    let mut manager = VestingManager::new(admin, Some(chain.to_owned()), Some(test));
    manager.set_chain_params(Some("IGNORED"));
    manager
}

#[derive(Default)]
struct Backend {
    initialized: bool,
    next_property: u32,
    journal: Vec<String>,
    balances: Vec<InitializationBalance>,
    fail_balance: Option<usize>,
}
impl Backend {
    fn new(next_property: u32, initialized: bool) -> Self {
        Self {
            next_property,
            initialized,
            ..Self::default()
        }
    }
}
impl InitializationBackend for Backend {
    fn call(&mut self, call: InitializationCall) -> Result<InitializationReply, VestingError> {
        let mut reply = InitializationReply::None;
        let text = match call {
            InitializationCall::CheckInitializationFlag => {
                reply = InitializationReply::Flag(self.initialized);
                "check".to_owned()
            }
            InitializationCall::AcquirePropertyManager => "pm".to_owned(),
            InitializationCall::CreateToken {
                ticker,
                amount,
                kind,
                whitelist_id,
                issuer,
                backup,
            } => {
                reply = InitializationReply::Property(
                    PropertyId::try_from(self.next_property).unwrap(),
                );
                self.next_property += 1;
                let kind = match kind {
                    InitialTokenKind::Fixed => "Fixed",
                    InitialTokenKind::Vesting => "Vesting",
                    InitialTokenKind::Native => "Native",
                };
                format!(
                    "token/{ticker}/{amount}/{kind}/{whitelist_id}/{}/{}",
                    show_address(&issuer),
                    show_address(&backup)
                )
            }
            InitializationCall::CreateContractSeries { block, request: p } => {
                assert_eq!(p.on_chain_data, [(1, 0)]);
                reply = InitializationReply::ContractId(7);
                format!(
                    "contract/{}/{}/{}/{}/{}/{}/{}/1,0/{}/{}/{}/{}/{}/{}/{}",
                    show_address(&p.admin),
                    p.expiry_interval
                        .map_or("undefined".to_owned(), |v| v.to_string()),
                    p.leverage,
                    block,
                    p.initial_margin,
                    p.maintenance_margin,
                    p.expiry,
                    p.series_length,
                    p.native,
                    p.inverse,
                    p.fee,
                    p.notional_property_id,
                    p.notional_value,
                    p.collateral_property_id
                )
            }
            InitializationCall::ConstructInsurance {
                contract_id,
                initial_balance,
                distribution_share,
                hedge,
            } => {
                format!("insurance/{contract_id}/{initial_balance}/{distribution_share}/{hedge}")
            }
            InitializationCall::LaunchInsuranceDeposit {
                property,
                amount,
                is_hedge,
            } => {
                format!("deposit/{}/{amount}/{is_hedge}", property.get())
            }
            InitializationCall::UpdateBalance(balance) => {
                let text = format!(
                    "balance/{}/{}/{}/{}/{}/{}",
                    show_address(&balance.address),
                    balance.property.get(),
                    balance.available,
                    balance.reserved,
                    balance.margin,
                    balance.vesting
                );
                if self.fail_balance == Some(self.balances.len() + 1) {
                    self.journal.push(text);
                    return Err(VestingError::Backend("injected balance failure".to_owned()));
                }
                self.balances.push(balance);
                text
            }
            InitializationCall::GetAddressBalances(address) => {
                reply = InitializationReply::Balances(
                    self.balances
                        .iter()
                        .filter(|b| b.address == address)
                        .cloned()
                        .collect(),
                );
                format!("balances/{}", show_address(&address))
            }
            InitializationCall::LaunchClearlist {
                id,
                admin,
                name,
                url,
                description,
                backup,
            } => {
                format!(
                    "clear/{}/{name}/{url}/{description}/{}/{id}",
                    show_address(&admin),
                    show_address(&backup)
                )
            }
            InitializationCall::SetInitializationFlag => {
                self.initialized = true;
                "flag".to_owned()
            }
        };
        self.journal.push(text);
        Ok(reply)
    }
}

#[test]
fn exact_source_initialization_journals_and_retry_flag() {
    let mut count = 0;
    for row in fixtures().filter(|row| row[0] == "INIT") {
        let mut manager = manager(row[1], row[2].parse().unwrap(), address(row[3]));
        let mut backend = Backend::new(row[4].parse().unwrap(), row[5].parse().unwrap());
        let result = manager.initialize_tokens(77, &mut backend).unwrap();
        assert_eq!(show_address(&manager.admin), row[6]);
        assert_eq!(
            result.map_or("undefined".to_owned(), |r| r.len().to_string()),
            row[7]
        );
        assert_eq!(backend.journal.join(";"), row[8], "{} {}", row[1], row[2]);
        assert!(backend.initialized);
        backend.journal.clear();
        assert_eq!(manager.initialize_tokens(77, &mut backend).unwrap(), None);
        assert_eq!(backend.journal, ["check"]);
        count += 1;
    }
    assert_eq!(count, 9);
}

#[test]
fn initialization_failure_exposes_source_partial_effects_without_setting_flag() {
    let expected = fixtures().find(|row| row[0] == "INIT_FAILURE").unwrap();
    let mut manager = manager("LTC", false, address("admin"));
    let mut backend = Backend::new(1, false);
    backend.fail_balance = Some(3);
    assert_eq!(
        manager.initialize_tokens(77, &mut backend),
        Err(VestingError::Backend("injected balance failure".to_owned()))
    );
    assert_eq!(backend.journal.join(";"), expected[1]);
    assert_eq!(backend.balances.len(), 2);
    assert!(!backend.initialized);
    assert_eq!(backend.next_property, 5);
}

#[test]
fn native_request_preserves_static_fields_and_registry_key_mismatch() {
    let mut backend = Backend::new(1, false);
    assert_eq!(
        VestingManager::initialize_contract_series(77, &mut backend).unwrap(),
        7
    );
    assert_eq!(NativeContractRequest::default().admin, None);
    assert_eq!(NativeContractRequest::default().expiry_interval, None);
    assert_eq!(NativeContractRequest::default().leverage, 5);
    assert_eq!(NativeContractRequest::default().series_length, 6);
    VestingManager::initialize_clearlists(&mut backend).unwrap();
    assert_eq!(backend.journal.len(), 4);
    assert!(backend.journal[1].starts_with("clear/undefined/Issuer Clearlist/"));
    assert!(backend.journal[1].ends_with("/undefined/1"));
}

#[test]
fn chain_configuration_and_rebate_source_vectors() {
    let mut counts = [0; 2];
    for row in fixtures() {
        match row[0] {
            "CONFIG" => {
                let p = ChainParameters::for_chain(Some(row[1]));
                close(p.base_volume, n(row[2]), "base");
                close(p.min_rebate, n(row[3]), "min");
                close(p.max_rebate, n(row[4]), "max");
                assert_eq!(p.initial_token_amount.to_string(), row[5]);
                assert_eq!(p.ticker, row[6]);
                assert_eq!(p.hedge_leverage.to_string(), row[7]);
                assert_eq!(p.expiry_interval.to_string(), row[8]);
                assert_eq!(p.sponsor.as_str(), row[9]);
                for (value, expected) in [
                    &p.free_portion,
                    &p.insurance_portion,
                    &p.vesting_portion,
                    &p.sale_portion,
                ]
                .into_iter()
                .zip(&row[10..14])
                {
                    assert_eq!(value.to_string(), *expected);
                }
                counts[0] += 1;
            }
            "REBATE" => {
                close(
                    manager(row[1], false, None)
                        .calculate_trade_rebates(n(row[2]))
                        .unwrap(),
                    n(row[3]),
                    "rebate",
                );
                counts[1] += 1;
            }
            _ => {}
        }
    }
    assert_eq!(counts, [5, 45]);
}

#[test]
fn cumulative_vesting_targets_are_not_incremental_debits() {
    let mut count = 0;
    for row in fixtures().filter(|row| row[0] == "VEST") {
        let v = ns(row[1]);
        let out = update_vesting(v[0], v[1], v[2], v[3], Some(250000.0), Some(1500000.0)).unwrap();
        let expected = ns(row[2]);
        close(out.two, expected[0], "vesting two");
        close(out.three, expected[1], "vesting three");
        count += 1;
    }
    assert_eq!(count, 8);
    assert_eq!(update_vesting(1.0, 2.0, 3.0, 4.0, None, Some(1.0)), None);
    assert_eq!(update_vesting(1.0, 2.0, 3.0, 4.0, Some(1.0), None), None);
    assert_eq!(
        update_vesting(1e9, 0.0, 1e12, 0.0, Some(10.0), Some(20.0)),
        Some(VestingAmounts {
            two: 10.0,
            three: 20.0
        })
    );
}

#[test]
fn all_pure_binary64_helpers_match_unmodified_source() {
    let mut count = 0;
    for row in fixtures() {
        let actual = match row[0] {
            "FRACTION" => {
                let v = ns(row[1]);
                vest_fraction(v[0], v[1], v[2], v[3])
            }
            "PROJECT" => {
                let v = ns(row[1]);
                project_next_year_volume_btc(v[0], v[1])
            }
            "MONTHS" => {
                let v = ns(row[1]);
                months_to_reach(v[0], v[1], v[2], v[3])
            }
            "ANNUAL" => annualize_from_cmgr(n(row[1])),
            "DISCOUNT" => {
                let v = ns(row[1]);
                discount_factor_months(v[0], v[1])
            }
            "CMGR" => cmgr_from_series(&ns(row[1])),
            "EPS" => EPS,
            _ => continue,
        };
        close(actual, n(row[row.len() - 1]), row[0]);
        count += 1;
    }
    assert_eq!(count, 49);
}

#[test]
fn buyback_order_is_retained_and_execution_stub_is_explicit() {
    let mut count = 0;
    for row in fixtures().filter(|row| row[0] == "BUYBACK") {
        let orders = if row[2] == "-" {
            vec![]
        } else {
            row[2]
                .split(';')
                .map(|pair| {
                    let v = ns(pair);
                    BuybackOrder {
                        price: v[0],
                        amount: v[1],
                    }
                })
                .collect()
        };
        close(
            calculate_buyback_amount(n(row[1]), &orders),
            n(row[3]),
            "buyback",
        );
        count += 1;
    }
    assert_eq!(count, 10);
    assert_eq!(fetch_orderbook_for_token(1), None);
    assert_eq!(perform_buyback(0), Ok(()));
    assert_eq!(
        perform_buyback(1),
        Err(VestingError::UnimplementedOrderbook)
    );
}

fn quote_parameters(input: &str) -> TradeAwardParameters {
    let mut p = TradeAwardParameters::default();
    if input == "-" {
        return p;
    }
    for pair in input.split(',') {
        let (key, value) = pair.split_once('=').unwrap();
        let value = n(value);
        match key {
            "cmgr12" => p.cmgr12 = value,
            "monthlyVolumeBTC" => p.monthly_volume_btc = value,
            "tlvestEffectiveSupply" => p.tlvest_effective_supply = value,
            "priceTLIinBTC" => p.price_tli_in_btc = value,
            "cumulativeVolumeBTC" => p.cumulative_volume_btc = value,
            "discountRateAnnual" => p.discount_rate_annual = value,
            "feeRate" => p.fee_rate = value,
            _ => panic!("unhandled fixture key {key}"),
        }
    }
    p
}

#[test]
fn compute_source_defect_is_preserved_and_repaired_quotes_are_separate() {
    assert_eq!(
        compute_tokens_for_trade(1.0, false, &TradeAwardParameters::default()),
        Err(VestingError::UndefinedProjectedYearVolume)
    );
    let mut count = 0;
    for row in fixtures().filter(|row| row[0] == "REPAIRED_QUOTE") {
        let p = quote_parameters(row[3]);
        let out = quote_tokens_for_trade_repaired(n(row[1]), row[2].parse().unwrap(), &p);
        let actual = [
            out.tokens,
            out.award_value_btc,
            out.pv_token_btc,
            out.vest_fraction_now,
            out.months_to_full_vest,
            out.pv_dividend_btc,
            out.pv_conversion_btc,
            out.annual_growth,
            out.projected_year_volume_btc,
            out.per_token_dividend_btc,
            out.fee_btc,
        ];
        for (actual, expected) in actual.into_iter().zip(ns(row[4])) {
            close(actual, expected, row[3]);
        }
        count += 1;
    }
    assert_eq!(count, 12);
}

type ProbeValue = Result<(Option<String>, Option<bool>), String>;
#[derive(Default)]
struct Probe {
    values: VecDeque<ProbeValue>,
    attempts: usize,
    delays: Vec<u64>,
}
impl ChainProbe for Probe {
    fn probe(&mut self) -> ProbeValue {
        self.attempts += 1;
        self.values.pop_front().unwrap_or(Ok((None, None)))
    }
    fn retry_delay(&mut self, ms: u64) {
        self.delays.push(ms);
    }
}
fn chain_pair(chain: &str, test: bool) -> ProbeValue {
    Ok((Some(chain.to_owned()), Some(test)))
}

#[test]
fn chain_retry_fallback_and_static_get_instance_match_source() {
    let inputs = [
        (Some("LTC"), Some(false), vec![]),
        (Some("BTC"), None, vec![chain_pair("DOGE", true)]),
        (
            None,
            None,
            vec![
                Ok((None, None)),
                chain_pair("", false),
                chain_pair("LTC", false),
            ],
        ),
        (Some("DOGE"), None, vec![]),
        (None, Some(true), vec![]),
        (
            None,
            None,
            vec![Err("offline".to_owned()), chain_pair("BTC", false)],
        ),
    ];
    let expected: Vec<_> = fixtures().filter(|row| row[0] == "CHAIN").collect();
    for (index, (chain, test, values)) in inputs.into_iter().enumerate() {
        let mut manager = VestingManager::new(None, chain.map(str::to_owned), test);
        let mut probe = Probe {
            values: values.into(),
            ..Probe::default()
        };
        let report = manager.ensure_chain_initialized(&mut probe);
        let row = &expected[index];
        assert_eq!(manager.get_chain().unwrap(), row[2]);
        assert_eq!(
            manager
                .get_test()
                .map_or("undefined".to_owned(), |value| value.to_string()),
            row[3]
        );
        assert_eq!(probe.attempts.to_string(), row[4]);
        assert_eq!(probe.delays.len().to_string(), row[5]);
        assert!(probe.delays.iter().all(|delay| *delay == 2000));
        assert_eq!(report.attempts, probe.attempts);
        assert_eq!(report.used_fallback, probe.attempts == 10);
        assert_eq!(manager.parameters().unwrap().ticker, row[6]);
    }
    let mut slot = ManagerSlot::default();
    let mut probe = Probe {
        values: vec![chain_pair("LTC", true)].into(),
        ..Probe::default()
    };
    let first = slot
        .get_instance(address("first-admin"), Some("BTC".to_owned()), &mut probe)
        .clone();
    let second = slot.get_instance(address("second-admin"), Some("DOGE".to_owned()), &mut probe);
    assert_eq!(second, &first);
    let expected = fixtures().find(|row| row[0] == "SINGLETON").unwrap();
    assert_eq!(show_address(&second.admin), expected[1]);
    assert_eq!(second.get_chain().unwrap(), expected[2]);
    assert_eq!(second.get_test(), Some(true));
    assert_eq!(probe.attempts, 1);
}

#[test]
fn initialization_reply_validation_and_local_bounds_precede_effects() {
    struct Bad;
    impl InitializationBackend for Bad {
        fn call(&mut self, _: InitializationCall) -> Result<InitializationReply, VestingError> {
            Ok(InitializationReply::None)
        }
    }
    let mut manager = VestingManager::new(None, Some("LTC".to_owned()), Some(false));
    assert_eq!(
        manager.initialize_tokens(0, &mut Bad),
        Err(VestingError::UnexpectedReply)
    );
    let mut backend = Backend::new(1, false);
    assert_eq!(
        manager.initialize_tokens(0, &mut backend),
        Err(VestingError::MissingParameters)
    );
    assert_eq!(backend.journal, ["check"]);
    backend.journal.clear();
    assert_eq!(
        manager.initialize_tokens(MAX_JS_SAFE_BLOCK + 1, &mut backend),
        Err(VestingError::InvalidBlock)
    );
    assert!(backend.journal.is_empty());
    assert_eq!(
        manager.calculate_trade_rebates(1.0),
        Err(VestingError::MissingParameters)
    );
    assert_eq!(
        VestingManager::initialize_contract_series(MAX_JS_SAFE_BLOCK + 1, &mut backend),
        Err(VestingError::InvalidBlock)
    );
}

#[test]
fn every_recorded_oracle_case_has_a_known_consumer() {
    let rows: Vec<_> = fixtures().collect();
    assert_eq!(rows.len(), 151);
    for row in &rows {
        assert!(matches!(
            row[0],
            "CONFIG"
                | "REBATE"
                | "VEST"
                | "FRACTION"
                | "PROJECT"
                | "MONTHS"
                | "ANNUAL"
                | "DISCOUNT"
                | "CMGR"
                | "BUYBACK"
                | "REPAIRED_QUOTE"
                | "INIT"
                | "INIT_FAILURE"
                | "CHAIN"
                | "SINGLETON"
                | "EPS"
                | "ERROR"
        ));
    }
    assert_eq!(rows.iter().filter(|row| row[0] == "ERROR").count(), 5);
}
