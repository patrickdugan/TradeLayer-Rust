// Child of the actual Journal/WAL checkpoint metadata tests. No source owner.
use super::*;
#[test]
fn many_segments_keep_global_metadata_action_lineage_above4096_without_a_source_claim() {
    let t = Temp::new();
    let mut store = store(&t);
    let mut previous = None;
    let mut all_entries = Vec::new();
    for segment in 0..17 {
        let mut entries = Vec::new();
        for _ in 0..256 {
            append_ack(&mut store, previous.as_ref(), &mut entries);
        }
        all_entries.extend(entries.iter().cloned());
        let mut plan = store
            .prepare_acknowledged_history_checkpoint(
                &store.pin().unwrap(),
                previous.as_ref(),
                &entries,
            )
            .unwrap();
        store
            .attempt_acknowledged_history_checkpoint(&mut plan)
            .unwrap();
        let proof = store
            .finish_acknowledged_history_checkpoint(&mut plan)
            .unwrap();
        assert_eq!(proof.cursor().actions, (segment + 1) * 256);
        if let Some(prior) = &previous {
            let descriptor = parse(&store.read_blob(&proof.cursor().segment).unwrap()).unwrap();
            assert_eq!(descriptor.get("previousSegment"), prior.segment.value());
        }
        previous = Some(proof.cursor().clone());
    }
    assert_eq!(previous.as_ref().unwrap().actions, 4352);
    for (at, pin) in all_entries.iter().enumerate() {
        let entry = parse(&store.read_blob(pin).unwrap()).unwrap();
        assert_eq!(
            segment_count(&entry.get("sequence")).unwrap(),
            at as u64 + 1
        );
        let predecessor = if at == 0 {
            V::Null
        } else {
            all_entries[at - 1].value()
        };
        assert_eq!(entry.get("previousEntry"), predecessor);
    }
}
#[test]
fn forged_previous_cursor_cannot_skip_an_unarchived_actual_entry() {
    let t = Temp::new();
    let mut store = store(&t);
    let mut entries = Vec::new();
    append_ack(&mut store, None, &mut entries);
    let mut plan = store
        .prepare_acknowledged_history_checkpoint(&store.pin().unwrap(), None, &entries)
        .unwrap();
    store
        .attempt_acknowledged_history_checkpoint(&mut plan)
        .unwrap();
    let proof = store
        .finish_acknowledged_history_checkpoint(&mut plan)
        .unwrap();
    let mut cursor = proof.cursor().clone();
    let mut next = Vec::new();
    append_ack(&mut store, Some(&cursor), &mut next);
    let actual = store.pin().unwrap();
    cursor.actions += 1;
    assert!(store
        .prepare_acknowledged_history_checkpoint(&actual, Some(&cursor), &next)
        .is_err());
    assert_eq!(store.pin().unwrap(), actual);
    assert!(store.read_blob(&proof.cursor().segment).is_ok());
}
