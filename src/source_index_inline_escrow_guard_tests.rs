use super::*;
#[test]
fn active_inline_cross_publication_guard_refuses_before_any_native_put() {
    let temp = Temp::new();
    let mut index =
        SourceCompactIndex::create(&temp.source, ns(), "actual raw inline guard metadata").unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row(1))
        .unwrap();
    index.native_main_test_compaction_fixture().unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let scope = index.begin_inline_escrow_guard().unwrap();
    assert!(index.inline_escrow_scope_active());
    assert!(index
        .import_row(&index.snapshot().unwrap(), row(2))
        .is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    index.finish_inline_escrow_guard(scope).unwrap();
    assert!(!index.inline_escrow_scope_active());
    // Legacy mode is unchanged; this is a safety refusal, not full inline parity.
    index
        .import_row(&index.snapshot().unwrap(), row(2))
        .unwrap();
    assert_ne!(
        index.snapshot().unwrap().publication,
        before.metadata.publication
    );
}
#[test]
fn active_inline_ordinary_write_runs_and_foreign_scope_cannot_clear_guard() {
    let temp = Temp::new();
    let other = Temp::new();
    let mut index = SourceCompactIndex::create(&temp.source, ns(), "raw inline one").unwrap();
    let mut foreign = SourceCompactIndex::create(&other.source, ns(), "raw inline two").unwrap();
    let scope = index.begin_inline_escrow_guard().unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row(1))
        .unwrap();
    let wrong = SourceIndexInlineEscrowScopeV1 { token: scope.token };
    assert!(foreign.finish_inline_escrow_guard(wrong).is_err());
    assert!(index.inline_escrow_scope_active());
    index.finish_inline_escrow_guard(scope).unwrap();
    assert!(!index.inline_escrow_scope_active());
    assert_eq!(index.get("tx-000001-raw").unwrap(), Some(row(1)));
    let pin = index.capture_verified_history_pin().unwrap();
    let cold = index.begin_verified_cold_execution(&pin).unwrap();
    assert!(index.begin_inline_escrow_guard().is_err());
    index.finish_verified_cold_execution(&cold).unwrap();
}
