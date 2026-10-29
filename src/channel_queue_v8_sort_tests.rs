use super::*;
fn decode(raw: &str) -> V {
    let bytes = (0..raw.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&raw[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    crate::property_catalog::parse_diagnostic_value(std::str::from_utf8(&bytes).unwrap()).unwrap()
}
fn observed(outcome: SourceOutcome) -> V {
    match outcome {
        SourceOutcome::Returned(v) => obj(&[("returned", v)]),
        SourceOutcome::Thrown(e) => obj(&[(
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )]),
    }
}
fn sort_observed(outcome: Eval<()>) -> V {
    match outcome {
        Ok(()) => observed(SourceOutcome::Returned(V::Undefined)),
        Err(Fault::Thrown(e)) => observed(SourceOutcome::Thrown(e)),
        Err(other) => panic!("genuine source domain stopped locally: {other:?}"),
    }
}
fn number_value(v: V) -> usize {
    let V::Number(bits) = v else { panic!("number") };
    f64::from_bits(bits) as usize
}
fn fields(line: &str) -> Vec<&str> {
    line.split('\t').collect()
}
fn source_fixture() -> &'static str {
    include_str!("../fixtures/channel_queue_v8_vectors.tsv")
}

#[test]
fn exact_v8_comparator_schedule_and_partial_array_match_unchanged_source() {
    let mut count = 0;
    for line in source_fixture().lines().filter(|l| l.starts_with("SORT\t")) {
        let f = fields(line);
        let V::Array(mut rows) = decode(f[2]) else {
            panic!("sort input")
        };
        let mut pairs = vec![];
        let result = source_v8_stable_sort(&mut rows, |a, b| {
            pairs.push(format!(
                "{},{}",
                number_value(a.get("auditOrdinal")),
                number_value(b.get("auditOrdinal"))
            ));
            channel_queue_compare(a, b)
        });
        assert_eq!(
            pairs.join(";"),
            f[5],
            "{} complete comparator schedule",
            f[1]
        );
        assert_eq!(
            sort_observed(result),
            decode(f[3]),
            "{} exact source throw",
            f[1]
        );
        assert_eq!(
            V::Array(rows),
            decode(f[4]),
            "{} partial-array observation",
            f[1]
        );
        count += 1;
    }
    assert_eq!(count, 16);
}

#[test]
fn full_channel_queue_documents_instances_calls_and_retained_failures_match_source() {
    let mut count = 0;
    for line in source_fixture().lines().filter(|l| l.starts_with("CASE\t")) {
        let f = fields(line);
        let input = decode(f[2]);
        let V::Array(rows) = input.get("rows") else {
            panic!("raw rows")
        };
        let mut owner = ContractQueueSourceOwner::new();
        owner.import_rows(rows).unwrap();
        let V::Array(failures) = input.get("failures") else {
            panic!("failures")
        };
        for failure in failures {
            owner
                .fail_database_call(QueueDbFailure {
                    operation: match failure.get("operation") {
                        V::String(s) => s,
                        _ => panic!("operation"),
                    },
                    skip: number_value(failure.get("skip")),
                    exception: SourceException {
                        name: "Error".into(),
                        message: match failure.get("message") {
                            V::String(s) => s,
                            _ => panic!("message"),
                        },
                    },
                })
                .unwrap();
        }
        let mut calls = 0;
        let mut external = vec![];
        let result = owner
            .evaluate_channel_with_capability(
                &SourceCall {
                    method: "processQueuedChannelTradesForBlock".into(),
                    arguments: vec![input.get("height")],
                },
                &mut |owner, call| {
                    calls += 1;
                    external.push(obj(&[
                        ("target", s(&call.target)),
                        ("attempt", V::number(calls as f64)),
                        ("txid", arg(&call.arguments, 3)),
                    ]));
                    if input.get("write") == V::Bool(true) {
                        let id = number_value(call.arguments[0].get("orderbookInstance")) as u64;
                        let key = owner.instance_value(id).unwrap().get("orderBookKey");
                        match owner.evaluate_instance(
                            id,
                            &SourceCall {
                                method: "saveOrderBook".into(),
                                arguments: vec![
                                    obj(&[
                                        ("buy", V::Array(vec![])),
                                        ("sell", V::Array(vec![])),
                                        ("externalCapability", V::Bool(true)),
                                        ("attempt", V::number(calls as f64)),
                                    ]),
                                    key,
                                ],
                            },
                        )? {
                            SourceOutcome::Returned(_) => (),
                            SourceOutcome::Thrown(e) => {
                                return Ok(DependencyReturn::PendingThrow(e))
                            }
                        }
                    }
                    Ok(if number_value(input.get("rejectAt")) == calls {
                        DependencyReturn::PendingThrow(SourceException {
                            name: "Error".into(),
                            message: "injected placement rejection".into(),
                        })
                    } else {
                        DependencyReturn::Pending(V::Undefined)
                    })
                },
            )
            .unwrap_or_else(|e| panic!("{} local boundary: {e:?}", f[1]));
        assert_eq!(observed(result), decode(f[3]), "{} outcome", f[1]);
        assert_eq!(owner.state_value(), decode(f[4]), "{} complete state", f[1]);
        assert_eq!(
            V::Array(
                owner
                    .take_db_calls()
                    .into_iter()
                    .map(|c| c.value())
                    .collect()
            ),
            decode(f[5]),
            "{} ordered DB calls",
            f[1]
        );
        assert_eq!(
            V::Array(
                owner
                    .take_placement_calls()
                    .into_iter()
                    .map(|c| obj(&[
                        ("target", s(&c.target)),
                        ("arguments", V::Array(c.arguments))
                    ]))
                    .collect()
            ),
            decode(f[6]),
            "{} exact placement boundary",
            f[1]
        );
        assert_eq!(
            V::Array(external),
            decode(f[7]),
            "{} earlier external effects",
            f[1]
        );
        let bytes = owner.checkpoint().unwrap();
        let restored = ContractQueueSourceOwner::from_checkpoint(&bytes).unwrap();
        assert_eq!(restored, owner);
        assert_eq!(restored.checkpoint().unwrap(), bytes);
        count += 1;
    }
    assert_eq!(count, 16);
}

#[test]
fn prior_compiled_owner_checkpoint_bytes_remain_exact_in_its_admitted_domain() {
    let mut count = 0;
    for line in include_str!("../fixtures/channel_queue_prior_checkpoints.tsv").lines() {
        let f = fields(line);
        let input = source_fixture()
            .lines()
            .filter(|l| l.starts_with("CASE\t"))
            .map(fields)
            .find(|p| p[1] == f[0])
            .unwrap();
        let V::Array(rows) = decode(input[2]).get("rows") else {
            panic!("raw input")
        };
        let mut owner = ContractQueueSourceOwner::new();
        owner.import_rows(rows).unwrap();
        let hex = |raw: Vec<u8>| raw.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(hex(owner.checkpoint().unwrap()), f[1]);
        owner
            .evaluate_channel_with_capability(
                &SourceCall {
                    method: "processQueuedChannelTradesForBlock".into(),
                    arguments: vec![V::number(10.)],
                },
                &mut |_, _| panic!("unknown kinds have no finance"),
            )
            .unwrap();
        assert_eq!(hex(owner.checkpoint().unwrap()), f[2]);
        count += 1;
    }
    assert_eq!(count, 4);
}

#[test]
fn explicit_shared_source_value_limit_refuses_1025_before_sort_mutation() {
    let mut rows = vec![V::Undefined; MAX_ROWS + 1];
    let before = rows.clone();
    assert!(matches!(
        channel_queue_sort(&mut rows),
        Err(Fault::Local(SourceRuleError::Capacity))
    ));
    assert_eq!(rows, before);
}
