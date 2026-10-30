use super::*;
struct RejectedRead<'a> {
    actual: &'a mut Backend,
    target: &'static str,
}
impl SourceBackend for RejectedRead<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if call.target == self.target {
            self.actual.calls.push(call.clone());
            return Ok(DependencyReturn::PendingThrow(SourceException {
                name: "Error".into(),
                message: format!("recorded raw IO rejection {}", self.target),
            }));
        }
        self.actual.invoke(call)
    }
}
#[test]
fn actual_factory_and_read_rejections_keep_original_source_throw_and_do_not_reenter_io() {
    for target in ["db.getDatabase", "db.findOneAsync"] {
        let temp = Temp::new();
        let mut actual = Backend::new(&temp, None);
        let mut owner = initialized(&mut actual);
        let before = actual.index.capture_verified_history_pin().unwrap();
        let mut io = RejectedRead {
            actual: &mut actual,
            target,
        };
        let expected = owner
            .evaluate(
                None,
                &SourceCall {
                    method: "findMaxIndexedBlock".into(),
                    arguments: vec![],
                },
                &mut io,
            )
            .unwrap();
        let calls = io.actual.calls.clone();
        io.actual.calls.clear();
        let mut frame = owner.begin_find_max_read(904, &mut io).unwrap();
        owner.enter_find_max_read(904, &mut frame, &mut io).unwrap();
        owner.settle_find_max_read(904, &mut frame).unwrap();
        let observed = frame.take_evaluation().unwrap();
        assert_eq!(observed, expected);
        assert!(matches!(observed.outcome, SourceOutcome::Thrown(_)));
        assert_eq!(io.actual.calls, calls);
        let calls = io.actual.calls.clone();
        assert!(owner.enter_find_max_read(904, &mut frame, &mut io).is_err());
        assert_eq!(io.actual.calls, calls);
        assert_eq!(
            io.actual.index.capture_verified_history_pin().unwrap(),
            before
        );
    }
}
#[test]
fn genuine_metadata_find_max_three_segments_match_existing_source_evaluation_and_refuse_repeat() {
    let temp = Temp::new();
    let mut b = Backend::new(&temp, None);
    let mut owner = initialized(&mut b);
    let old = owner
        .evaluate(
            None,
            &SourceCall {
                method: "findMaxIndexedBlock".into(),
                arguments: vec![],
            },
            &mut b,
        )
        .unwrap();
    let expected = b.calls.clone();
    b.calls.clear();
    let pin = b.index.capture_verified_history_pin().unwrap();
    let mut c = owner.begin_find_max_read(901, &mut b).unwrap();
    assert_eq!(b.calls.len(), 1);
    assert_eq!(b.calls[0].target, "db.getDatabase");
    let mut fields = vec![];
    c.visit_fields(&mut |p, v| fields.push((p.to_owned(), v.clone())));
    assert!(!fields.is_empty());
    assert!(owner.enter_find_max_read(902, &mut c, &mut b).is_err());
    assert_eq!(b.calls.len(), 1);
    owner.enter_find_max_read(901, &mut c, &mut b).unwrap();
    assert_eq!(b.calls, expected);
    assert!(c.take_evaluation().is_err());
    owner.settle_find_max_read(901, &mut c).unwrap();
    let actual = c.take_evaluation().unwrap();
    assert_eq!(actual, old);
    assert!(c.take_evaluation().is_err());
    assert!(owner.settle_find_max_read(901, &mut c).is_err());
    assert_eq!(b.index.capture_verified_history_pin().unwrap(), pin);
}
#[test]
fn real_initializer_source_prefix_yields_without_insert_and_reuses_same_native_write_frame() {
    let temp = Temp::new();
    let mut b = Backend::new(&temp, None);
    let owner = initialized(&mut b);
    let before = b.index.capture_verified_history_pin().unwrap();
    let mut c = owner
        .begin_inline_source(903, TxIndexInlineMethodV1::Initialize, vec![V::number(10.)])
        .unwrap();
    owner.null_initializer_factory(903, &mut c, &mut b).unwrap();
    assert_eq!(b.calls.len(), 1);
    assert!(c.pending_call().is_none());
    assert!(owner.null_initializer_read(903, &mut c, &mut b).unwrap());
    assert_eq!(b.calls.len(), 2);
    assert!(c.pending_call().is_none());
    owner.null_initializer_settle_read(903, &mut c).unwrap();
    let mut owner = owner;
    assert_eq!(
        owner.advance_inline_source(903, &mut c, &mut b).unwrap(),
        TxIndexInlineStopV1::AwaitDatabase
    );
    let call = c.pending_call().unwrap().clone();
    assert_eq!(call.target, "db.insertAsync");
    assert_eq!(b.index.capture_verified_history_pin().unwrap(), before);
    let prepared = b
        .index
        .prepare_inline_database(
            &before.metadata,
            "insertAsync",
            call.arguments[1..].to_vec(),
        )
        .unwrap();
    b.index.append_inline_database(&prepared, None).unwrap();
    let result = b.index.resolve_inline_database_post(&prepared).unwrap();
    owner.resolve_inline_source(903, &mut c, result).unwrap();
    assert_eq!(
        owner.advance_inline_source(903, &mut c, &mut b).unwrap(),
        TxIndexInlineStopV1::Complete
    );
    assert_eq!(
        c.take_evaluation().unwrap().outcome,
        SourceOutcome::Returned(V::Undefined)
    );
}
