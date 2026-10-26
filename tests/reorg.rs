use std::collections::BTreeMap;
use tradelayer_core::reorg::*;

fn decode(hex: &str) -> String {
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    assert_eq!(hex.len() % 2, 0);
    String::from_utf8(bytes).unwrap()
}
fn encode(text: &str) -> String {
    text.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}
fn packed(items: impl IntoIterator<Item = String>) -> String {
    let v: Vec<_> = items.into_iter().collect();
    if v.is_empty() {
        "-".to_owned()
    } else {
        v.join(";")
    }
}
fn failure(text: &str) -> RpcFailure {
    RpcFailure::new(text).unwrap()
}

#[derive(Clone)]
struct Rpc {
    tips: Result<Vec<ChainTip>, RpcFailure>,
    blocks: BTreeMap<String, Result<ReportedBlock, RpcFailure>>,
    trace: Vec<String>,
}
impl ReorgRpc for Rpc {
    fn get_chain_tips(&mut self) -> Result<Vec<ChainTip>, RpcFailure> {
        self.trace.push("tips".to_owned());
        self.tips.clone()
    }
    fn get_block(&mut self, hash: &str) -> Result<ReportedBlock, RpcFailure> {
        self.trace.push(format!("block:{}", encode(hash)));
        self.blocks
            .get(hash)
            .expect("unexpected RPC block request")
            .clone()
    }
}
fn parse_rpc(row: &[&str]) -> Rpc {
    let tips = if row[3] != "-" {
        Err(failure(&decode(row[3])))
    } else {
        Ok(if row[2] == "-" {
            vec![]
        } else {
            row[2]
                .split(';')
                .map(|spec| {
                    let (status, hash) = spec.split_once(':').unwrap();
                    ChainTip::new(&decode(hash), &decode(status)).unwrap()
                })
                .collect()
        })
    };
    let blocks = if row[4] == "-" {
        BTreeMap::new()
    } else {
        row[4]
            .split(';')
            .map(|spec| {
                let f: Vec<_> = spec.split(':').collect();
                let hash = decode(f[0]);
                let result = match f[1] {
                    "err" => Err(failure(&decode(f[2]))),
                    "ok" => Ok(ReportedBlock::new(f[2].parse().unwrap(), &decode(f[3])).unwrap()),
                    _ => panic!("unknown block fixture"),
                };
                (hash, result)
            })
            .collect()
    };
    // The JS adapter records client construction before it hands the client
    // to the class. Rust explicitly receives that caller-owned transport.
    Rpc {
        tips,
        blocks,
        trace: vec!["construct".to_owned()],
    }
}

#[test]
fn actual_unmodified_source_scenarios_match_observations_errors_logs_and_order() {
    let mut count = 0;
    for line in include_str!("../fixtures/reorg_vectors.tsv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 10);
        let name = row[0];
        let mut checker = ReorgChecker::new(parse_rpc(&row));
        assert_eq!(checker.rpc().trace, ["construct"]);
        let (logs, observations, outcome) = match row[1] {
            "construct" => (Vec::new(), Vec::new(), "constructed".to_owned()),
            "get" => {
                let request = decode(row[5]);
                let fetched = checker.get_block(&request);
                match fetched.result {
                    Ok(block) => (fetched.logs, vec![(request, block)], "ok".to_owned()),
                    Err(ObserverFailure::Rpc(error)) => (
                        fetched.logs,
                        vec![],
                        format!("throw:{}", encode(error.message())),
                    ),
                    result => panic!("unexpected direct result {result:?}"),
                }
            }
            "check" => {
                let report = checker.check_reorg();
                let outcome = match &report.swallowed_failure {
                    None => "undefined".to_owned(),
                    Some(ObserverFailure::Rpc(error)) => {
                        format!("swallowed:{}", encode(error.message()))
                    }
                    result => panic!("unexpected checked result {result:?}"),
                };
                assert_eq!(report.completed(), report.swallowed_failure.is_none());
                let observations = report
                    .observations
                    .into_iter()
                    .map(|o| (o.tip.reported_hash().to_owned(), o.block))
                    .collect();
                (report.logs, observations, outcome)
            }
            method => panic!("unknown source method {method}"),
        };
        assert_eq!(
            packed(checker.into_rpc().trace),
            row[6],
            "{name} RPC ordering"
        );
        assert_eq!(
            packed(logs.iter().map(|l| encode(&l.message()))),
            row[7],
            "{name} source logs"
        );
        assert_eq!(
            packed(observations.iter().map(|(request, block)| format!(
                "{}:{}:{}",
                encode(request),
                block.height(),
                encode(block.reported_hash())
            ))),
            row[8],
            "{name} observed prefix"
        );
        assert_eq!(outcome, row[9], "{name} source outcome");
        count += 1;
    }
    assert_eq!(count, 20);
}

#[test]
fn metadata_bounds_are_explicit_and_do_not_validate_hash_syntax() {
    assert!(ChainTip::new("", "valid-fork").is_ok());
    assert!(ReportedBlock::new(0, "non hex!").is_ok());
    assert_eq!(
        ChainTip::new(&"x".repeat(MAX_REPORTED_HASH_BYTES + 1), "active"),
        Err(ObserverInputError::HashTooLong)
    );
    assert_eq!(
        ChainTip::new("x", &"s".repeat(MAX_TIP_STATUS_BYTES + 1)),
        Err(ObserverInputError::StatusTooLong)
    );
    assert_eq!(
        RpcFailure::new(&"e".repeat(MAX_RPC_ERROR_BYTES + 1)),
        Err(ObserverInputError::ErrorTooLong)
    );
    assert_eq!(
        ReportedBlock::new(MAX_REPORTED_BLOCK_HEIGHT + 1, "x"),
        Err(ObserverInputError::HeightOutOfRange)
    );
    assert!(ReportedBlock::new(
        MAX_REPORTED_BLOCK_HEIGHT,
        &"h".repeat(MAX_REPORTED_HASH_BYTES)
    )
    .is_ok());
    // Byte limits, including multibyte UTF-8 metadata.
    assert!(ChainTip::new(&"é".repeat(MAX_REPORTED_HASH_BYTES / 2), "active").is_ok());
    assert_eq!(
        ChainTip::new(&"é".repeat(MAX_REPORTED_HASH_BYTES / 2 + 1), "active"),
        Err(ObserverInputError::HashTooLong)
    );
}

#[test]
fn too_many_tips_are_reported_before_any_block_fetch() {
    let mut checker = ReorgChecker::new(Rpc {
        tips: Ok(vec![
            ChainTip::new("x", "valid-fork").unwrap();
            MAX_REORG_TIPS + 1
        ]),
        blocks: BTreeMap::new(),
        trace: vec![],
    });
    let report = checker.check_reorg();
    assert_eq!(report.total_tip_count, Some(MAX_REORG_TIPS + 1));
    assert_eq!(report.selected_tip_count, None);
    assert_eq!(
        report.swallowed_failure,
        Some(ObserverFailure::TooManyTips {
            actual: MAX_REORG_TIPS + 1,
            maximum: MAX_REORG_TIPS
        })
    );
    assert!(report.observations.is_empty());
    assert_eq!(report.logs.len(), 1);
    assert_eq!(checker.into_rpc().trace, ["tips"]);
}

#[test]
fn direct_hash_limit_prevents_rpc_and_has_no_source_rpc_error_log() {
    let mut checker = ReorgChecker::new(Rpc {
        tips: Ok(vec![]),
        blocks: BTreeMap::new(),
        trace: vec![],
    });
    let report = checker.get_block(&"x".repeat(MAX_REPORTED_HASH_BYTES + 1));
    assert_eq!(
        report.result,
        Err(ObserverFailure::Input(ObserverInputError::HashTooLong))
    );
    assert!(report.logs.is_empty());
    assert!(checker.into_rpc().trace.is_empty());
}

#[test]
fn maximum_tip_list_is_bounded_and_duplicate_observations_are_retained() {
    let mut checker = ReorgChecker::new(Rpc {
        tips: Ok(vec![
            ChainTip::new("x", "valid-fork").unwrap();
            MAX_REORG_TIPS
        ]),
        blocks: BTreeMap::from([("x".to_owned(), Ok(ReportedBlock::new(1, "x").unwrap()))]),
        trace: vec![],
    });
    let report = checker.check_reorg();
    assert!(report.completed());
    assert_eq!(report.observations.len(), MAX_REORG_TIPS);
    assert_eq!(report.logs.len(), MAX_REORG_TIPS);
    assert_eq!(report.selected_tip_count, Some(MAX_REORG_TIPS));
}

#[test]
fn partial_error_report_has_full_candidate_count_and_does_not_fetch_tail() {
    let mut checker = ReorgChecker::new(Rpc {
        tips: Ok(vec![
            ChainTip::new("a", "valid-fork").unwrap(),
            ChainTip::new("b", "headers-only").unwrap(),
            ChainTip::new("c", "valid-fork").unwrap(),
        ]),
        blocks: BTreeMap::from([
            ("a".to_owned(), Ok(ReportedBlock::new(1, "a").unwrap())),
            ("b".to_owned(), Err(failure("missing"))),
        ]),
        trace: vec![],
    });
    let report = checker.check_reorg();
    assert_eq!(report.total_tip_count, Some(3));
    assert_eq!(report.selected_tip_count, Some(3));
    assert_eq!(report.observations.len(), 1);
    assert_eq!(report.failed_tip.unwrap().reported_hash(), "b");
    assert_eq!(report.logs.len(), 3);
    assert_eq!(checker.into_rpc().trace, ["tips", "block:61", "block:62"]);
}

#[test]
fn repeated_checks_are_independent_observations_without_retry_or_deduplication() {
    let mut checker = ReorgChecker::new(Rpc {
        tips: Err(failure("offline")),
        blocks: BTreeMap::new(),
        trace: vec![],
    });
    let first = checker.check_reorg();
    let second = checker.check_reorg();
    assert_eq!(first, second);
    assert_eq!(first.logs.len(), 1);
    assert_eq!(checker.into_rpc().trace, ["tips", "tips"]);
}
