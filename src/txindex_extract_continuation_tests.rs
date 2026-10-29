use super::*;
struct ExtractBackend {
    base: Backend,
    tip: V,
    blocks: Vec<V>,
}
impl ExtractBackend {
    fn new(temp: &Temp, with_tx: bool, fail_at: Option<usize>) -> Self {
        let blocks = [10., 11.]
            .into_iter()
            .map(|height| {
                let mut block = raw_block();
                block.insert("height", V::number(height)).unwrap();
                block
                    .insert("hash", s(format!("caller-block{height}")))
                    .unwrap();
                block
                    .insert(
                        "previousblockhash",
                        s(format!("caller-block{}", height - 1.)),
                    )
                    .unwrap();
                if !with_tx || height == 10. {
                    block.insert("tx", V::Array(vec![])).unwrap();
                }
                block
            })
            .collect();
        Self {
            base: Backend::new(temp, fail_at),
            tip: V::number(11.),
            blocks,
        }
    }
}
impl SourceBackend for ExtractBackend {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let returned = match call.target.as_str() {
            "client.getBlockCount" => Some(self.tip.clone()),
            "client.getBlockHash" => Some(
                self.blocks
                    .iter()
                    .find(|b| b.get("height") == arg(&call.arguments, 1))
                    .ok_or(SourceRuleError::UnknownDependency)?
                    .get("hash"),
            ),
            "client.getBlock" => Some(
                self.blocks
                    .iter()
                    .find(|b| b.get("hash") == arg(&call.arguments, 1))
                    .ok_or(SourceRuleError::UnknownDependency)?
                    .clone(),
            ),
            _ => None,
        };
        if let Some(value) = returned {
            self.base.calls.push(call.clone());
            Ok(DependencyReturn::Pending(value))
        } else {
            self.base.invoke(call)
        }
    }
}
fn run_extract(
    owner: &mut TxIndexSourceOwner,
    c: &mut TxIndexExtractContinuationV1,
    b: &mut ExtractBackend,
) -> SourceEvaluation {
    for _ in 0..100 {
        match owner.advance_retained_extract(501, c, b).unwrap() {
            TxIndexExtractStopV1::Complete => return c.take_evaluation().unwrap(),
            TxIndexExtractStopV1::CallerReturned => {
                assert!(matches!(
                    c.caller_outcome(),
                    Some(SourceOutcome::Returned(V::Undefined))
                ));
                assert!(c.detached_pending());
            }
            TxIndexExtractStopV1::AwaitDatabase => {
                let call = c.pending_call().unwrap().clone();
                let descriptor = c.pending_descriptor().unwrap();
                b.base.calls.push(call.clone());
                let returned = if let Some(error) = b.base.injected() {
                    error
                } else {
                    let plan = b
                        .base
                        .index
                        .prepare_inline_database(
                            &b.base.index.snapshot().unwrap(),
                            call.target.strip_prefix("db.").unwrap(),
                            call.arguments[1..].to_vec(),
                        )
                        .unwrap();
                    b.base.index.append_inline_database(&plan, None).unwrap();
                    assert_eq!(
                        owner.advance_retained_extract(501, c, b).unwrap(),
                        TxIndexExtractStopV1::AwaitDatabase,
                        "native apply never resumes source"
                    );
                    assert_eq!(c.pending_descriptor().as_ref(), Some(&descriptor));
                    b.base.index.resolve_inline_database_post(&plan).unwrap()
                };
                owner
                    .resolve_retained_extract_source(501, c, &descriptor, returned)
                    .unwrap();
            }
        }
    }
    panic!("bounded actual extract source did not finish")
}
#[test]
fn retained_extract_matches_actual_old_owner_after_all_work_but_exposes_separate_caller_return() {
    for failure in [None, Some(0), Some(1), Some(2), Some(3), Some(4)] {
        let old_temp = Temp::new();
        let new_temp = Temp::new();
        let mut old = ExtractBackend::new(&old_temp, true, failure);
        let mut new = ExtractBackend::new(&new_temp, true, failure);
        let mut old_owner = initialized(&mut old.base);
        let mut new_owner = initialized(&mut new.base);
        let expected = old_owner
            .evaluate(
                None,
                &SourceCall {
                    method: "extractBlockData".into(),
                    arguments: vec![V::number(10.)],
                },
                &mut old,
            )
            .unwrap();
        let mut c = new_owner
            .begin_retained_extract(501, V::number(10.))
            .unwrap();
        let actual = run_extract(&mut new_owner, &mut c, &mut new);
        assert_eq!(
            actual, expected,
            "actual same-source stable boundary failure={failure:?}"
        );
        assert_eq!(new.base.snapshot_rows(), old.base.snapshot_rows());
        assert_eq!(new.base.calls, old.base.calls);
        assert_eq!(
            new_owner.state_value().unwrap(),
            old_owner.state_value().unwrap()
        );
    }
}
#[test]
fn retained_extract_caller_fulfills_before_final_maxheight_write_and_same_return_is_held() {
    let temp = Temp::new();
    let mut b = ExtractBackend::new(&temp, false, None);
    let mut owner = initialized(&mut b.base);
    let mut c = owner.begin_retained_extract(501, V::number(10.)).unwrap();
    assert_eq!(
        owner.advance_retained_extract(501, &mut c, &mut b).unwrap(),
        TxIndexExtractStopV1::CallerReturned
    );
    assert!(matches!(
        c.caller_outcome(),
        Some(SourceOutcome::Returned(V::Undefined))
    ));
    assert!(c.detached_pending());
    let descriptor = c.pending_descriptor().unwrap();
    assert_eq!(descriptor.role, TxIndexExtractAwaitRoleV1::Detached);
    assert_eq!(descriptor.source_job, 2);
    assert_eq!(descriptor.operation_ordinal, 0);
    assert!(!b.base.calls.iter().any(|c| c.target == "db.updateAsync"));
    let call = c.pending_call().unwrap().clone();
    let mut before = vec![];
    c.visit_fields(&mut |k, v| before.push((k.to_string(), v.clone())));
    assert_eq!(
        owner.advance_retained_extract(501, &mut c, &mut b).unwrap(),
        TxIndexExtractStopV1::AwaitDatabase
    );
    assert_eq!(
        b.base.index.get("MaxHeight").unwrap().unwrap().get("value"),
        V::number(10.)
    );
    let plan = b
        .base
        .index
        .prepare_inline_database(
            &b.base.index.snapshot().unwrap(),
            "updateAsync",
            call.arguments[1..].to_vec(),
        )
        .unwrap();
    b.base.index.append_inline_database(&plan, None).unwrap();
    assert_eq!(
        b.base.index.get("MaxHeight").unwrap().unwrap().get("value"),
        V::number(11.)
    );
    assert!(b.base.index.get("indexExists").unwrap().is_none());
    assert_eq!(
        owner.advance_retained_extract(501, &mut c, &mut b).unwrap(),
        TxIndexExtractStopV1::AwaitDatabase
    );
    assert_eq!(c.pending_descriptor().as_ref(), Some(&descriptor));
    assert!(matches!(
        c.caller_outcome(),
        Some(SourceOutcome::Returned(V::Undefined))
    ));
    let returned = b.base.index.resolve_inline_database_post(&plan).unwrap();
    // Borrow the actual held child used by the forthcoming shared IO adapter.
    owner
        .resolve_inline_source(
            descriptor.method_scope(),
            c.pending_source_mut().unwrap(),
            returned,
        )
        .unwrap();
    assert_eq!(
        owner.advance_retained_extract(501, &mut c, &mut b).unwrap(),
        TxIndexExtractStopV1::AwaitDatabase
    );
    let next = c.pending_descriptor().unwrap();
    assert_eq!(next.operation_ordinal, 1);
    assert_eq!(next.source_job, descriptor.source_job);
    assert_eq!(next.method_scope, descriptor.method_scope);
    assert!(
        owner
            .resolve_retained_extract_source(
                501,
                &mut c,
                &descriptor,
                DependencyReturn::Pending(V::number(1.))
            )
            .is_err(),
        "old operation selection cannot resolve next await"
    );
    assert!(
        c.take_evaluation().is_err(),
        "caller fulfillment cannot drop detached work"
    );
}
#[test]
fn retained_extract_detached_rejection_is_deferred_and_awaited_rejection_is_not() {
    for (with_tx, failure, detached) in [(false, 0, true), (true, 2, false)] {
        let temp = Temp::new();
        let mut b = ExtractBackend::new(&temp, with_tx, Some(failure));
        let mut owner = initialized(&mut b.base);
        let mut c = owner.begin_retained_extract(501, V::number(10.)).unwrap();
        let actual = run_extract(&mut owner, &mut c, &mut b);
        assert_eq!(
            matches!(actual.outcome, SourceOutcome::Returned(V::Undefined)),
            detached
        );
        assert_eq!(actual.deferred_rejections.len(), usize::from(detached));
        if !detached {
            assert!(!b
                .base
                .calls
                .iter()
                .any(|call| call.target == "client.getBlockHash"
                    && arg(&call.arguments, 1) == V::number(12.)));
        }
    }
}
#[test]
fn retained_extract_empty_early_no_work_and_forward_resume_keep_original_source_order() {
    for (start, forward) in [(12., false), (10., true)] {
        let old_temp = Temp::new();
        let new_temp = Temp::new();
        let mut old = ExtractBackend::new(&old_temp, false, None);
        let mut new = ExtractBackend::new(&new_temp, false, None);
        let mut old_owner = initialized(&mut old.base);
        let mut new_owner = initialized(&mut new.base);
        if forward {
            old_owner.forward = s("1");
            new_owner.forward = s("1");
        }
        let expected = old_owner
            .evaluate(
                None,
                &SourceCall {
                    method: "extractBlockData".into(),
                    arguments: vec![V::number(start)],
                },
                &mut old,
            )
            .unwrap();
        let mut c = new_owner
            .begin_retained_extract(501, V::number(start))
            .unwrap();
        let actual = run_extract(&mut new_owner, &mut c, &mut new);
        assert_eq!(actual, expected);
        assert_eq!(new.base.calls, old.base.calls);
        assert_eq!(new.base.snapshot_rows(), old.base.snapshot_rows());
    }
}
