use super::*;
#[test]
fn same_real_detached_job_remains_selectable_between_actual_db_resolves() {
    let temp = Temp::new();
    let mut b = ExtractBackend::new(&temp, false, None);
    let mut owner = initialized(&mut b.base);
    let mut c = owner.begin_retained_extract(501, V::number(10.)).unwrap();
    assert_eq!(
        owner.advance_retained_extract(501, &mut c, &mut b).unwrap(),
        TxIndexExtractStopV1::CallerReturned
    );
    let first = c.pending_descriptor().unwrap();
    let call = c.pending_call().unwrap().clone();
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
    let returned = b.base.index.resolve_inline_database_post(&plan).unwrap();
    owner
        .resolve_retained_extract_source(501, &mut c, &first, returned)
        .unwrap();
    assert!(
        c.pending_descriptor().is_none(),
        "actual callback resolved; next DB await has not executed"
    );
    let ready = c.active_descriptor().unwrap();
    assert_eq!(ready.source_job(), first.source_job());
    assert_eq!(ready.method_scope(), first.method_scope());
    assert_eq!(ready.role(), TxIndexExtractAwaitRoleV1::Detached);
    assert_eq!(ready.operation_ordinal(), first.operation_ordinal() + 1);
    let pin = b.base.index.snapshot().unwrap();
    let calls = b.base.calls.clone();
    assert_eq!(c.active_descriptor(), Some(ready.clone()));
    assert_eq!(b.base.index.snapshot().unwrap(), pin);
    assert_eq!(
        b.base.calls, calls,
        "descriptor observation performs no source dispatch"
    );
    assert_eq!(
        owner.advance_retained_extract(501, &mut c, &mut b).unwrap(),
        TxIndexExtractStopV1::AwaitDatabase
    );
    assert_eq!(c.pending_descriptor(), Some(ready));
    assert_eq!(
        c.pending_call().unwrap().arguments[1].get("_id"),
        s("indexExists")
    );
}
