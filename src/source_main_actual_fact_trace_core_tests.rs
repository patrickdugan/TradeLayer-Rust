// Actual-source Core controls, never dummy Before/Accepted/source verdicts.
use super::*;
use crate::source_main_active_service_wal::{
    ActualSourceFactColdPinV1, VerifiedActualSourceFactTraceV1,
};
fn trace_pin(owner: &mut SourceMainNativeActualFactServiceV1) -> ActualSourceFactColdPinV1 {
    let namespace = owner.inner.index().native_namespace().clone();
    owner
        .inner
        .wal
        .capture_actual_source_fact_cold_pin(&namespace)
        .unwrap()
}
#[test]
fn actual_source_fact_trace_revalidates_real_producer_prefixes_through_three_plans() {
    let t = temp();
    let mut owner = fact_setup(&t);
    for i in 0..3 {
        fact_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
        if i < 2 {
            fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
        }
    }
    let count = owner.history().len();
    let head = owner.head();
    let pin = trace_pin(&mut owner);
    drop(owner);
    let namespace = Namespace::new("LTC", false, "consensus").unwrap();
    let reader = VerifiedActualSourceFactTraceV1::open(&t.root, namespace.clone(), &pin).unwrap();
    assert_eq!(reader.entries().len(), count);
    assert_eq!(reader.entries().last().unwrap().head, head);
    drop(reader);
    let mut wrong = pin.clone();
    wrong.trace.chain_sha256d[0] ^= 1;
    assert!(VerifiedActualSourceFactTraceV1::open(&t.root, namespace.clone(), &wrong).is_err());
    let old = crate::source_main_active_service_wal::InlineActiveServiceColdPinV1 {
        profile: crate::source_main_active_service_wal::MODE4_COLD_PIN_PROFILE.into(),
        wal: pin.wal.clone(),
        metadata_namespace: pin.metadata_namespace.clone(),
        trace: pin.trace.clone(),
    };
    assert!(
        crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1::open(
            &t.root, namespace, &old
        )
        .is_err()
    );
}
#[test]
fn actual_source_fact_trace_missing_pending_blob_refuses_without_repair() {
    let t = temp();
    let mut owner = fact_setup(&t);
    fact_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    let reference = owner
        .inner
        .wal
        .record()
        .unwrap()
        .get("actualNativeIoPendingFact");
    let V::String(hash) = reference.get("sha256") else {
        panic!("real produced blob hash")
    };
    let pin = trace_pin(&mut owner);
    drop(owner);
    let file = t
        .root
        .join("active-service-blobs-v1")
        .join(format!("{hash}.asw1"))
        .canonicalize()
        .unwrap();
    assert!(file.starts_with(t.root.canonicalize().unwrap()));
    let mut raw = std::fs::read(&file).unwrap();
    raw[0] ^= 1;
    std::fs::write(&file, &raw).unwrap();
    assert!(VerifiedActualSourceFactTraceV1::open(
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &pin
    )
    .is_err());
    assert_eq!(std::fs::read(&file).unwrap(), raw);
}
