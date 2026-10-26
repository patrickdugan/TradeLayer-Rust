use std::collections::{BTreeMap, BTreeSet};
use tradelayer_core::client::RpcValue;
use tradelayer_core::issuance_intent::JsonValue;
use tradelayer_core::service::*;
use tradelayer_core::service_routes::*;
fn value(s: &str) -> RpcValue {
    RpcValue::parse(s).unwrap()
}
fn request(method: ServiceMethod, path: &str, body: &str) -> ServiceRequest {
    ServiceRequest::new(method, path, BTreeMap::new(), value(body)).unwrap()
}
#[derive(Default)]
struct Backend {
    calls: Vec<ServiceCall>,
    result: Option<RpcValue>,
    error: Option<ServiceError>,
}
impl ServiceBackend for Backend {
    fn call(&mut self, call: ServiceCall) -> Result<RpcValue, ServiceError> {
        self.calls.push(call);
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        Ok(self.result.clone().unwrap_or_else(|| value("{}")))
    }
}
fn unhex(s: &str) -> String {
    String::from_utf8(
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect(),
    )
    .unwrap()
}
fn fields(v: &JsonValue) -> BTreeMap<String, String> {
    let JsonValue::Object(object) = v else {
        panic!()
    };
    object
        .iter()
        .map(|(k, v)| {
            let JsonValue::String(v) = v else { panic!() };
            (k.to_owned(), v.clone())
        })
        .collect()
}
fn expected(path: &str) -> &'static str {
    match path
        .split('/')
        .nth(1)
        .unwrap()
        .to_ascii_lowercase()
        .as_str()
    {
        "initmain" | "tl_initmain" => "runtime.initialize",
        "initstatus" => "runtime.status",
        "getallbalancesforaddress" | "tl_getallbalancesforaddress" => "ledger.balances",
        "listproperties" | "tl_listproperties" => "ledger.properties",
        "addresseswithproperty" => "ledger.addresses_with_property",
        "getactivations" | "tl_getactivations" => "registry.activations",
        "getorderbook" | "tl_getorderbook" => "spot.book",
        "getcontractorderbook" | "tl_getcontractorderbook" => "contract.book",
        "listcontractseries" => "contract.list",
        "tl_listcontractseries" | "tl_getcontractinfo" => "contract.info",
        "listoracles" | "tl_listoracles" => "registry.oracles",
        "contracttradehistory" | "tl_contracttradehistory" => "history.contract",
        "tradehistory" | "tl_tradehistory" => "history.spot",
        "fundinghistory" | "tl_fundinghistory" => "history.funding",
        "oraclehistory" | "tl_oraclehistory" => "history.oracle",
        "contractposition" | "tl_contractposition" => "contract.position",
        "clearinghistory" => "history.clearing",
        "walletpositions" => "wallet.positions",
        "walletbalances" => "wallet.balances",
        "walletposition" => "wallet.position",
        "tl_validateaddress" => "node.validate_address",
        "tl_gettransaction" => "transactions.get",
        "tl_getchannelcolumn" => "channel.predict_column",
        "tl_getattestations" => "registry.attestations",
        "tl_getissuanceintents" => "issuance.intents",
        "tl_preparecommitfromissuance" => "issuance.prepare_commit",
        "tl_loadwallet" => "wallet.load",
        "tl_gettransactionsforaddress" => "transactions.address",
        "tl_gettransactionforblock" => "transactions.block",
        "tl_getmaxprocessedheight"
        | "tl_getmaxparsedheight"
        | "tl_gettrackheight"
        | "tl_checksync" => "runtime.heights",
        "tl_pause" => "runtime.pause",
        "tl_getchannel" => "channel.get",
        "tl_createrawtx_opreturn" => "wallet.add_op_return",
        "tl_getproperty" => "ledger.property",
        "tl_listclearlists" => "registry.clear_lists",
        "tl_showclearlist" | "tl_getclearlistbyid" => "registry.clear_list",
        "tl_listfeecache" => "fees.all",
        "tl_propertyfeecache" => "fees.property",
        "tl_bitvmstatus" => "procedural.bitvm_status",
        "tl_channelbalanceforcommiter" => "channel.balance_for_committer",
        "tl_getmaxsynth" => "synthetic.max",
        "tl_tokentradehistoryforaddress" => "history.spot_for_address",
        "tl_getinitmargin" => "contract.initial_margin",
        "tl_contracttradehistoryforaddress" => "history.contract_for_address",
        "tl_totaltradehistoryforaddress" => "history.address",
        other => panic!("unknown active source route {other}"),
    }
}

#[test]
fn every_active_source_route_has_real_validated_dispatch() {
    let mut count = BTreeMap::<String, usize>::new();
    for line in include_str!("../fixtures/service_routes_vectors.tsv").lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f.first() != Some(&"R") || f[2] != "baseline" {
            continue;
        }
        let JsonValue::Object(raw) = JsonValue::parse(&unhex(f[5])).unwrap() else {
            panic!()
        };
        let params = fields(raw.get("params").unwrap());
        let path = f[4]
            .split('/')
            .map(|p| {
                if let Some(k) = p.strip_prefix(':') {
                    params.get(k).unwrap().as_str()
                } else {
                    p
                }
            })
            .collect::<Vec<_>>()
            .join("/");
        let body = raw.get("body").unwrap().to_json().unwrap();
        let mut req = request(
            if f[3] == "GET" {
                ServiceMethod::Get
            } else {
                ServiceMethod::Post
            },
            &path,
            &body,
        );
        req.query = fields(raw.get("query").unwrap());
        let mut backend = Backend::default();
        let response =
            dispatch(&req, &mut backend).unwrap_or_else(|e| panic!("{} {} {e:?}", f[1], f[4]));
        assert_eq!(response.status, 200);
        assert_eq!(backend.calls.len(), 1);
        assert_eq!(backend.calls[0].target, expected(&path));
        *count.entry(f[1].into()).or_default() += 1;
    }
    assert_eq!(count.get("listener"), Some(&19));
    assert_eq!(count.get("wallet"), Some(&43));
}

#[test]
fn route_matching_is_case_insensitive_but_argument_locations_and_case_are_exact() {
    let mut b = Backend::default();
    let r = request(ServiceMethod::Get, "/CoNtRaCtPoSiTiOn/ALICE/7/", "{}");
    dispatch(&r, &mut b).unwrap();
    assert_eq!(
        b.calls[0],
        ServiceCall {
            target: "contract.position",
            arguments: vec![value("\"ALICE\""), value("7")]
        }
    );
    let mut r = request(
        ServiceMethod::Get,
        "/TL_CONTRACTPOSITION",
        r#"{"address":"body","contractId":2}"#,
    );
    assert_eq!(dispatch(&r, &mut b), Err(ServiceError::InvalidRequest));
    r.query = BTreeMap::from([
        ("address".into(), "query".into()),
        ("contractId".into(), "3".into()),
    ]);
    dispatch(&r, &mut b).unwrap();
    assert_eq!(
        b.calls.last().unwrap().arguments,
        vec![value("\"query\""), value("3")]
    );
    let before = b.calls.len();
    for path in [
        "//tl_listProperties",
        "/tl_listProperties//",
        "/tl_listProperties/extra",
    ] {
        assert_eq!(
            dispatch(&request(ServiceMethod::Post, path, "{}"), &mut b),
            Err(ServiceError::NotFound)
        );
    }
    assert_eq!(
        dispatch(
            &request(ServiceMethod::Get, "/tl_listProperties", "{}"),
            &mut b
        ),
        Err(ServiceError::NotFound)
    );
    assert_eq!(b.calls.len(), before);
}

#[test]
fn numeric_ids_and_addresses_reject_coercion_before_backend_call() {
    for body in [
        r#"{"params":0}"#,
        r#"{"params":-1}"#,
        r#"{"params":1.5}"#,
        r#"{"params":1e0}"#,
        r#"{"params":"01"}"#,
        r#"{"params":"1junk"}"#,
        r#"{"params":4294967296}"#,
    ] {
        let mut b = Backend::default();
        assert!(dispatch(
            &request(ServiceMethod::Post, "/tl_getProperty", body),
            &mut b
        )
        .is_err());
        assert!(b.calls.is_empty());
    }
    for address in ["", "two words", "~econ:pool", "é"] {
        let mut b = Backend::default();
        let body = format!(
            "{{\"address\":{}}}",
            RpcValue::string(address).unwrap().as_json()
        );
        assert!(dispatch(
            &request(ServiceMethod::Post, "/getAllBalancesForAddress", &body),
            &mut b
        )
        .is_err());
        assert!(b.calls.is_empty());
    }
    let mut b = Backend::default();
    dispatch(
        &request(ServiceMethod::Post, "/tl_getClearlistById", r#"{"id":0}"#),
        &mut b,
    )
    .unwrap();
    assert_eq!(b.calls[0].arguments, vec![value("0")]);
}

#[test]
fn wallet_balance_source_params_and_repaired_address_alias_agree() {
    let mut b = Backend::default();
    for body in [
        r#"{"params":"alice"}"#,
        r#"{"address":"alice"}"#,
        r#"{"address":"alice","params":"alice"}"#,
    ] {
        dispatch(
            &request(ServiceMethod::Post, "/tl_getAllBalancesForAddress", body),
            &mut b,
        )
        .unwrap();
        assert_eq!(b.calls.last().unwrap().target, "ledger.balances");
        assert_eq!(b.calls.last().unwrap().arguments, vec![value("\"alice\"")]);
    }
    let before = b.calls.len();
    for body in [
        r#"{"address":"alice","params":"bob"}"#,
        r#"{"address":"alice","params":null}"#,
        r#"{"address":1,"params":"alice"}"#,
        r#"{}"#,
    ] {
        assert_eq!(
            dispatch(
                &request(ServiceMethod::Post, "/tl_getAllBalancesForAddress", body),
                &mut b
            ),
            Err(ServiceError::InvalidRequest)
        );
    }
    assert_eq!(b.calls.len(), before);
}

#[test]
fn body_absence_and_explicit_null_have_different_repaired_contracts() {
    let mut b = Backend::default();
    dispatch(
        &request(ServiceMethod::Post, "/tl_listProperties", "null").without_body(),
        &mut b,
    )
    .unwrap();
    let n = b.calls.len();
    assert_eq!(
        dispatch(
            &request(ServiceMethod::Post, "/tl_listProperties", "null"),
            &mut b
        ),
        Err(ServiceError::InvalidRequest)
    );
    assert_eq!(
        dispatch(
            &request(ServiceMethod::Post, "/tl_getProperty", "{}").without_body(),
            &mut b
        ),
        Err(ServiceError::InvalidRequest)
    );
    assert_eq!(b.calls.len(), n);
}

#[test]
fn unavailable_capabilities_and_broadcast_never_turn_into_empty_success() {
    let mut b = Backend {
        error: Some(ServiceError::Unavailable("no_authenticated_owner")),
        ..Default::default()
    };
    assert_eq!(
        dispatch(
            &request(ServiceMethod::Post, "/tl_listProperties", "{}"),
            &mut b
        ),
        Err(ServiceError::Unavailable("no_authenticated_owner"))
    );
    let mut b = Backend::default();
    let body=format!("{{\"issuanceTxid\":\"{}\",\"fromAddress\":\"alice\",\"channelAddress\":\"chan\",\"autoSend\":true}}","ab".repeat(32));
    assert_eq!(
        dispatch(
            &request(ServiceMethod::Post, "/tl_prepareCommitFromIssuance", &body),
            &mut b
        ),
        Err(ServiceError::Unavailable("authenticated_commit_broadcast"))
    );
    assert!(b.calls.is_empty());
    assert_eq!(
        dispatch(
            &request(ServiceMethod::Post, "/initMain", r#"{"test":true}"#),
            &mut b
        ),
        Err(ServiceError::Unavailable("testnet_service"))
    );
    assert!(b.calls.is_empty());
}

#[test]
fn explicit_commit_preparation_normalizes_only_valid_bounded_parameters() {
    let mut b = Backend::default();
    let body=format!("{{\"issuanceTxid\":\"{}\",\"fromAddress\":\"alice\",\"channelAddress\":\"chan\",\"payEnabled\":true,\"clearLists\":[0,\"7\"]}}","AB".repeat(32));
    dispatch(
        &request(ServiceMethod::Post, "/tl_prepareCommitFromIssuance", &body),
        &mut b,
    )
    .unwrap();
    assert_eq!(b.calls[0].target, "issuance.prepare_commit");
    assert_eq!(
        b.calls[0].arguments,
        vec![
            value(&format!("\"{}\"", "ab".repeat(32))),
            value("\"alice\""),
            value("\"chan\""),
            value("true"),
            value("[0,7]")
        ]
    );
    for lists in ["[{}]".to_owned(), format!("[{}]", vec!["1"; 65].join(","))] {
        let body = body.replace("[0,\"7\"]", &lists);
        let before = b.calls.len();
        assert!(dispatch(
            &request(ServiceMethod::Post, "/tl_prepareCommitFromIssuance", &body),
            &mut b
        )
        .is_err());
        assert_eq!(b.calls.len(), before);
    }
}

#[test]
fn missing_observed_or_parser_heights_do_not_claim_sync() {
    let mut b = Backend {
        result: Some(value(
            r#"{"maxProcessedHeight":10,"maxParsedHeight":null,"trackHeight":null}"#,
        )),
        ..Default::default()
    };
    let r = dispatch(&request(ServiceMethod::Post, "/tl_checkSync", "{}"), &mut b).unwrap();
    assert_eq!(
        value(&r.body).field("available").unwrap().unwrap(),
        value("false")
    );
    assert_eq!(
        value(&r.body).field("synced").unwrap().unwrap(),
        value("null")
    );
    let r = dispatch(
        &request(ServiceMethod::Post, "/tl_getTrackHeight", "{}"),
        &mut b,
    )
    .unwrap();
    assert_eq!(
        value(&r.body).field("source").unwrap().unwrap(),
        value("\"maxProcessed\"")
    );
    b.result = Some(value(
        r#"{"maxProcessedHeight":10,"maxParsedHeight":10,"trackHeight":10}"#,
    ));
    let r = dispatch(&request(ServiceMethod::Post, "/tl_checkSync", "{}"), &mut b).unwrap();
    assert_eq!(
        value(&r.body).field("synced").unwrap().unwrap(),
        value("true")
    );
    b.result = Some(value(
        r#"{"maxProcessedHeight":10,"maxParsedHeight":11,"trackHeight":11}"#,
    ));
    let r = dispatch(&request(ServiceMethod::Post, "/tl_checkSync", "{}"), &mut b).unwrap();
    assert_eq!(
        value(&r.body).field("synced").unwrap().unwrap(),
        value("false")
    );
}

#[test]
fn repaired_aliases_fix_missing_wallet_address_and_req_id_fee_bug() {
    let mut b = Backend::default();
    let mut r = request(ServiceMethod::Get, "/walletPositions", "{}");
    assert_eq!(dispatch(&r, &mut b), Err(ServiceError::InvalidRequest));
    r.query.insert("address".into(), "alice".into());
    dispatch(&r, &mut b).unwrap();
    assert_eq!(b.calls[0].target, "wallet.positions");
    dispatch(
        &request(
            ServiceMethod::Post,
            "/tl_propertyFeeCache",
            r#"{"propertyId":7,"id":8}"#,
        ),
        &mut b,
    )
    .unwrap();
    assert_eq!(
        b.calls[1],
        ServiceCall {
            target: "fees.property",
            arguments: vec![value("7")]
        }
    );
    dispatch(
        &request(ServiceMethod::Post, "/tl_listContractSeries", "{}"),
        &mut b,
    )
    .unwrap();
    assert_eq!(b.calls[2].target, "contract.list");
}

#[test]
fn funding_ranges_and_margin_prices_are_explicit_and_bounded() {
    let mut b = Backend::default();
    let mut r = request(ServiceMethod::Get, "/tl_fundingHistory", "{}");
    r.query = BTreeMap::from([
        ("contractId".into(), "7".into()),
        ("startBlock".into(), "10".into()),
        ("endBlock".into(), "9".into()),
    ]);
    assert_eq!(dispatch(&r, &mut b), Err(ServiceError::InvalidRequest));
    r.query.remove("startBlock");
    dispatch(&r, &mut b).unwrap();
    assert_eq!(
        b.calls[0].arguments,
        vec![value("7"), value("null"), value("9")]
    );
    let mut r = request(ServiceMethod::Get, "/tl_getInitMargin", "{}");
    r.query = BTreeMap::from([
        ("contractId".into(), "7".into()),
        ("price".into(), "1e2".into()),
    ]);
    dispatch(&r, &mut b).unwrap();
    assert_eq!(b.calls[1].arguments, vec![value("7"), value("\"100\"")]);
    for bad in ["0", "-1", "NaN", "1e99"] {
        r.query.insert("price".into(), bad.into());
        assert!(dispatch(&r, &mut b).is_err());
    }
}

#[test]
fn source_helper_vectors_match_supported_json_observable_domain() {
    let mut helpers = BTreeSet::new();
    for line in include_str!("../fixtures/service_routes_vectors.tsv").lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f.first() != Some(&"H") {
            continue;
        }
        helpers.insert(f[1]);
        let input = value(&unhex(f[2]));
        let output = value(&unhex(f[3]));
        match f[1] {
            "parseValue" => {
                let expected = output.field("value").unwrap().unwrap();
                assert_eq!(parse_value(&input).unwrap(), expected);
            }
            "envNum" => {
                let raw = input.field("raw").unwrap();
                let mut env = BTreeMap::new();
                if let Some(raw) = raw {
                    if let Ok(raw) = raw.as_string() {
                        env.insert("TEST".into(), raw);
                    }
                }
                let expected = output
                    .field("value")
                    .unwrap()
                    .unwrap()
                    .as_json()
                    .parse::<f64>()
                    .unwrap();
                assert_eq!(env_num(&env, "TEST", 6.0).unwrap(), expected);
            }
            "parseWindowIndex" => {
                let raw = input.as_string().unwrap();
                let expected = output.field("value").unwrap().unwrap();
                if raw.contains("9007199254740993") {
                    assert!(parse_window_index(&raw).is_err());
                } else {
                    let actual = parse_window_index(&raw)
                        .unwrap()
                        .map_or_else(RpcValue::null, RpcValue::unsigned);
                    assert_eq!(actual, expected);
                }
            }
            _ => {}
        }
    }
    assert_eq!(
        helpers,
        BTreeSet::from([
            "parseValue",
            "envNum",
            "parseWindowIndex",
            "parsePropertyKey",
            "waitForClientWrapper"
        ])
    );
}

#[test]
fn synthetic_property_keys_are_query_metadata_not_mint_authority() {
    assert_eq!(
        parse_property_key("S1-7").unwrap(),
        PropertyKey {
            base_property: 1,
            contract_id: Some(7)
        }
    );
    assert_eq!(parse_property_key("1").unwrap().canonical(), "1");
    for raw in ["s-1-7", "S01-007", "-1", "1.5", "0", "s1-0", "1junk"] {
        assert!(parse_property_key(raw).is_err());
    }
    assert!(parse_value(&value(r#"{"value":"1e999"}"#)).is_err());
}
