use tradelayer_core::decimal::{Decimal, MAX_DECIMAL_DIGITS};
use tradelayer_core::ledger::AccountId;
use tradelayer_core::oracle::{
    FraudClaim, Oracle, OracleConfig, OracleData, OracleError, OracleRecord, OracleStore,
    ParsedRelayFields, PriceRecord, Publication, RelayRecord, StakeRecord, MAX_JS_SAFE_INTEGER,
    MAX_ORACLE_TEXT_BYTES,
};

fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}

fn account(value: &str) -> AccountId {
    value.try_into().unwrap()
}

fn config(admin: &str) -> OracleConfig {
    OracleConfig {
        name: "test".into(),
        ticker: "test".into(),
        url: String::new(),
        admin: account(admin),
        backup: Some(account("backup")),
        clearlists: vec![],
        lag: d("1"),
    }
}

fn data(a: &[&str]) -> OracleData {
    let optional = |value: &str| (value != "-").then(|| d(value));
    OracleData {
        price: d(a[0]),
        high: optional(a[1]),
        low: optional(a[2]),
        close: optional(a[3]),
    }
}

fn data_text(data: &OracleData) -> String {
    [
        Some(&data.price),
        data.high.as_ref(),
        data.low.as_ref(),
        data.close.as_ref(),
    ]
    .into_iter()
    .map(|value| value.map(ToString::to_string).unwrap_or_else(|| "-".into()))
    .collect::<Vec<_>>()
    .join("/")
}

fn stake_text(row: Option<&StakeRecord>) -> String {
    row.map(|row| {
        format!(
            "{}/{}/{}/{}/{}",
            row.oracle_id,
            row.staker.as_str(),
            row.property_id,
            row.amount,
            row.block
        )
    })
    .unwrap_or_else(|| "null".into())
}

fn info_text(row: Option<&Oracle>) -> String {
    row.map(|row| {
        [
            row.id.to_string(),
            row.config.name.clone(),
            row.config.ticker.clone(),
            row.config.admin.as_str().into(),
            row.config
                .backup
                .as_ref()
                .map(|value| value.as_str().into())
                .unwrap_or_else(|| "-".into()),
            if row.config.url.is_empty() {
                "-".into()
            } else {
                row.config.url.clone()
            },
            row.config.lag.to_string(),
            if row.config.clearlists.is_empty() {
                "-".into()
            } else {
                row.config
                    .clearlists
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            },
            row.closed.to_string(),
            row.last_published_block
                .map(|block| block.to_string())
                .unwrap_or_else(|| "-".into()),
            row.data
                .as_ref()
                .map(data_text)
                .unwrap_or_else(|| "-".into()),
        ]
        .join(";")
    })
    .unwrap_or_else(|| "null".into())
}

fn operate(store: &mut OracleStore, op: &str, a: &[&str], calls: &mut Vec<String>) -> String {
    let id = || a[0].parse::<u64>().unwrap();
    match op {
        "create" => store
            .create(OracleConfig {
                name: a[0].into(),
                ticker: a[1].into(),
                admin: account(a[2]),
                backup: (!a[3].is_empty()).then(|| account(a[3])),
                url: a[4].into(),
                lag: d(a[5]),
                clearlists: if a[6].is_empty() {
                    vec![]
                } else {
                    a[6].split(',')
                        .map(|value| value.parse().unwrap())
                        .collect()
                },
            })
            .unwrap()
            .to_string(),
        "next" => store.next_id().unwrap().to_string(),
        "all" => store
            .get_all()
            .iter()
            .map(|row| row.id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        "info" => info_text(store.get(id())),
        "admin" => store.is_admin(&account(a[1]), id()).to_string(),
        "verify" => store.verify_admin(id(), &account(a[1])).to_string(),
        "update" => {
            let actor = store.get(id()).unwrap().config.admin.clone();
            store
                .update_admin(&actor, id(), account(a[1]), a[2] == "backup")
                .unwrap();
            "ok".into()
        }
        "price" => store.price(id()).to_string(),
        "twap" => store
            .twap(id(), a[1].parse().unwrap(), a[2].parse().unwrap())
            .unwrap()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".into()),
        "publish" => {
            let actor = store.get(id()).unwrap().config.admin.clone();
            match store
                .publish(&actor, id(), data(&a[2..]), a[1].parse().unwrap())
                .unwrap()
            {
                Publication::Published(row) => data_text(&row.data),
                Publication::Skipped { last_update_block } => format!("skip:{last_update_block}"),
            }
        }
        "save" => {
            store
                .save_price_record(PriceRecord {
                    oracle_id: id(),
                    block: a[1].parse().unwrap(),
                    data: data(&a[2..]),
                })
                .unwrap();
            "ok".into()
        }
        "history" => {
            let rows = store
                .history(id(), a[1].parse().unwrap(), a[2].parse().unwrap())
                .into_iter()
                .map(|row| format!("{}:{}", row.block, data_text(&row.data)))
                .collect::<Vec<_>>()
                .join(",");
            if rows.is_empty() {
                "-".into()
            } else {
                rows
            }
        }
        "reset" => {
            store.reset_publication_watermarks();
            "ok".into()
        }
        "close" => {
            let Some(actor) = store.get(id()).map(|row| row.config.admin.clone()) else {
                assert!(matches!(
                    store.close(&account("unknown"), id()),
                    Err(OracleError::UnknownOracle(_))
                ));
                return "error:Oracle not found".into();
            };
            let request = store.close(&actor, id()).unwrap();
            calls.push(format!(
                "{}:{}",
                request.admin.as_str(),
                request.oracle_closure
            ));
            "ok".into()
        }
        "calls" => calls.join(","),
        "stake" => {
            let change = store
                .record_stake_metadata(
                    id(),
                    account(a[1]),
                    a[2].parse().unwrap(),
                    &d(a[3]),
                    a[4].parse().unwrap(),
                )
                .unwrap();
            stake_text(Some(&change.current))
        }
        "stakeget" => stake_text(store.stake(id(), &account(a[1]))),
        "fraud" => store
            .apply_fraud_metadata(FraudClaim {
                oracle_id: id(),
                accused: account(a[1]),
                challenger: account(a[2]),
                slash_amount: d(a[3]),
                evidence_hash: a[4].into(),
                block: a[5].parse().unwrap(),
            })
            .unwrap()
            .to_string(),
        "relay" => {
            let row = RelayRecord {
                oracle_id: id(),
                sender: account(a[1]),
                relay_type: a[2].into(),
                state_hash: a[3].into(),
                dlc_ref: a[4].into(),
                block: a[5].parse().unwrap(),
                blob: format!(r#"{{"signatureHex":"{}","stateHash":"{}"}}"#, a[6], a[7]),
            };
            let previous = store.clone();
            match store.relay_metadata(
                row,
                ParsedRelayFields {
                    signature_hex: a[6].into(),
                    state_hash: a[7].into(),
                },
            ) {
                Ok(row) => format!(
                    "{}/{}/{}/{}/{}/{}",
                    row.oracle_id,
                    row.sender.as_str(),
                    row.relay_type,
                    if row.state_hash.is_empty() {
                        "-"
                    } else {
                        &row.state_hash
                    },
                    if row.dlc_ref.is_empty() {
                        "-"
                    } else {
                        &row.dlc_ref
                    },
                    row.block
                ),
                Err(OracleError::ReplayConflict) => {
                    assert_eq!(*store, previous);
                    "error:replay".into()
                }
                Err(error) => panic!("unexpected {error:?}"),
            }
        }
        "relayrows" => store
            .records()
            .iter()
            .filter_map(|record| match record {
                OracleRecord::SignatureUse(row) => Some(format!(
                    "{}/{}/{}/{}",
                    row.signature_hex,
                    if row.state_hash.is_empty() {
                        "-"
                    } else {
                        &row.state_hash
                    },
                    if row.dlc_ref.is_empty() {
                        "-"
                    } else {
                        &row.dlc_ref
                    },
                    row.first_seen_block
                )),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(","),
        _ => panic!("unknown operation {op}"),
    }
}

#[test]
fn pinned_source_lifecycle_vectors() {
    let mut store = OracleStore::new();
    let mut scenario = "";
    let mut calls = Vec::new();
    let mut count = 0;
    for line in include_str!("../fixtures/oracle_vectors.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 4, "{line}");
        if fields[0] != scenario {
            store = OracleStore::new();
            calls.clear();
            scenario = fields[0];
        }
        let a = fields[2].split('|').collect::<Vec<_>>();
        let actual = operate(&mut store, fields[1], &a, &mut calls);
        assert_eq!(actual, fields[3], "{line}");
        count += 1;
    }
    assert_eq!(count, 246);
}

#[test]
fn logical_authorization_and_error_atomicity() {
    let mut store = OracleStore::new();
    let id = store.create(config("admin")).unwrap();
    let original = store.clone();
    assert_eq!(
        store.publish(&account("eve"), id, OracleData::price(d("10")), 1),
        Err(OracleError::Unauthorized)
    );
    assert_eq!(
        store.close(&account("eve"), id),
        Err(OracleError::Unauthorized)
    );
    assert_eq!(
        store.update_admin(&account("eve"), id, account("eve"), false),
        Err(OracleError::Unauthorized)
    );
    assert_eq!(store, original);
    store
        .update_admin(&account("backup"), id, account("newadmin"), false)
        .unwrap();
    assert!(!store.is_admin(&account("admin"), id));
    assert!(store.verify_admin(id, &account("newadmin")));
    assert!(!store.verify_admin(id, &account("backup")));
}

#[test]
fn source_cached_publication_is_lost_on_metadata_reload_or_admin_update() {
    let mut store = OracleStore::new();
    let id = store.create(config("admin")).unwrap();
    store
        .publish(&account("admin"), id, OracleData::price(d("10")), 1)
        .unwrap();
    assert!(store.get(id).unwrap().data.is_some());
    store.load_metadata();
    assert!(store.get(id).unwrap().data.is_none());
    assert_eq!(store.price(id), d("10"));
    assert_eq!(store.last_update_block(id), Some(1));
    store
        .publish(&account("admin"), id, OracleData::price(d("10")), 2)
        .unwrap();
    store
        .update_admin(&account("admin"), id, account("new"), false)
        .unwrap();
    assert!(store.get(id).unwrap().data.is_none());
}

#[test]
fn failed_local_arithmetic_does_not_poison_publication_watermark() {
    let mut store = OracleStore::new();
    let id = store.create(config("admin")).unwrap();
    store
        .save_price_record(PriceRecord {
            oracle_id: id,
            block: 1,
            data: OracleData::price(d(&"9".repeat(MAX_DECIMAL_DIGITS))),
        })
        .unwrap();
    let before = store.clone();
    assert!(store
        .publish_metadata(id, OracleData::price(d("10")), 2)
        .is_err());
    assert_eq!(store, before);
    assert_eq!(store.last_update_block(id), None);
    store
        .save_price_record(PriceRecord {
            oracle_id: id,
            block: 1,
            data: OracleData::price(d("10")),
        })
        .unwrap();
    assert!(matches!(
        store.publish_metadata(id, OracleData::price(d("10")), 2),
        Ok(Publication::Published(_))
    ));
}

#[test]
fn finite_limits_invalid_imports_and_number_bridge() {
    let mut store = OracleStore::new();
    let mut invalid = config("admin");
    invalid.name = "x".repeat(MAX_ORACLE_TEXT_BYTES + 1);
    assert_eq!(
        store.create(invalid),
        Err(OracleError::InvalidField("name"))
    );
    assert!(store.oracles().is_empty());
    let id = store.create(config("admin")).unwrap();
    let imported = store.get(id).unwrap().clone();
    let previous = store.clone();
    assert_eq!(store.add(imported), Err(OracleError::DuplicateOracle(id)));
    assert_eq!(store, previous);
    assert_eq!(
        store.publish_metadata(id, OracleData::price(d("2")), MAX_JS_SAFE_INTEGER + 1),
        Err(OracleError::InvalidBlock)
    );
    assert_eq!(
        store.publish_metadata(99, OracleData::price(d("2")), 1),
        Err(OracleError::UnknownOracle(99))
    );
    assert_eq!(
        store.record_stake_metadata(id, account("staker"), 2, &d("1e400"), 1),
        Err(OracleError::NonFiniteNumber)
    );
    assert_eq!(store, previous);
    let mut maximum = store.get(id).unwrap().clone();
    maximum.id = MAX_JS_SAFE_INTEGER;
    store.add(maximum).unwrap();
    assert_eq!(store.next_id(), Err(OracleError::InvalidId));
}

#[test]
fn slash_prefix_collision_and_invalid_evidence_are_explicit() {
    let mut store = OracleStore::new();
    store
        .record_stake_metadata(1, account("staker"), 2, &d("10"), 1)
        .unwrap();
    let mut claim = FraudClaim {
        oracle_id: 1,
        accused: account("staker"),
        challenger: account("challenger"),
        slash_amount: d("1"),
        evidence_hash: "a".repeat(25),
        block: 2,
    };
    store.apply_fraud_metadata(claim.clone()).unwrap();
    claim.evidence_hash = format!("{}b", "a".repeat(24));
    store.apply_fraud_metadata(claim.clone()).unwrap();
    assert_eq!(store.stake(1, &account("staker")).unwrap().amount, d("8"));
    assert_eq!(
        store
            .records()
            .iter()
            .filter(|row| matches!(row, OracleRecord::Fraud(_)))
            .count(),
        1
    );
    let before = store.clone();
    claim.evidence_hash = "🔑".into();
    assert_eq!(
        store.apply_fraud_metadata(claim),
        Err(OracleError::InvalidField("evidence_hash_ascii"))
    );
    assert_eq!(store, before);
}

#[test]
fn record_key_hash_matches_source_and_empty_signature_has_no_claim() {
    let mut store = OracleStore::new();
    let row = RelayRecord {
        oracle_id: 1,
        sender: account("sender"),
        relay_type: "state".into(),
        state_hash: String::new(),
        dlc_ref: String::new(),
        blob: "opaque".into(),
        block: 1,
    };
    store
        .relay_metadata(row.clone(), ParsedRelayFields::default())
        .unwrap();
    assert_eq!(store.records().len(), 1);
    store
        .relay_metadata(
            row,
            ParsedRelayFields {
                signature_hex: " ABC ".into(),
                state_hash: "x".into(),
            },
        )
        .unwrap();
    let signature = store
        .records()
        .iter()
        .find(|row| matches!(row, OracleRecord::SignatureUse(_)))
        .unwrap();
    assert_eq!(
        signature.key(),
        "oracle-relay-sig-1-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(store.price(1), d("1"));
}
