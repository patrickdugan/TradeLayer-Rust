// Actual administrative Journal/index/copy controls. No financial execution or
// source result is supplied or claimed by this metadata test fixture.
use super::*;

fn swap_past_barrier_frames(raw: &[u8]) -> Vec<u8> {
    let header = raw.windows(4).position(|value| value == b"TXN1").unwrap();
    let mut at = header;
    let mut frames = Vec::new();
    while at < raw.len() {
        assert_eq!(&raw[at..at + 4], b"TXN1");
        let payload = u32::from_be_bytes(raw[at + 4..at + 8].try_into().unwrap()) as usize;
        let end = at + payload + 32;
        frames.push(raw[at..end].to_vec());
        at = end;
    }
    assert_eq!(frames.len(), 4); // Created, Intent, Accepted, Ack.
    frames.swap(1, 2);
    let mut changed = raw[..header].to_vec();
    for (ordinal, mut frame) in frames.into_iter().enumerate() {
        frame[8..16].copy_from_slice(&(ordinal as u64 + 1).to_be_bytes());
        changed.extend(frame);
    }
    rechain_wal(&mut changed);
    changed
}

#[test]
fn swapping_complete_crc_valid_past_barriers_preserves_current_kv_but_never_closes_ack_order() {
    let temp = Temp::new();
    let index = metadata(&temp, false);
    let mut fixture = fixture(&temp, &index);
    fixture.start(&index);
    let expected = fixture.store.capture_inline_cold_pin(&meta_ns()).unwrap();
    let path = fixture.store.journal_path();
    let native_before = index.capture_verified_history_pin().unwrap();
    drop(fixture);
    let original = fs::read(&path).unwrap();
    let changed = swap_past_barrier_frames(&original);
    assert_ne!(changed, original);
    assert_eq!(changed.len(), original.len());
    fs::write(&path, &changed).unwrap();

    // The original final sequence, full KV and physical path still verify.
    let (mut journal, _) = Journal::open_existing_verified_prefix(
        &temp.wal,
        wal_ns(),
        Recovery::RejectIncompleteTail,
        &expected.wal.prefix,
    )
    .unwrap();
    let changed_trace = journal
        .read_verified_current_commits(
            &expected.wal.prefix,
            65536,
            MAX_VERIFIED_ACK_HISTORY_BYTES as usize,
        )
        .unwrap()
        .trace()
        .clone();
    assert_ne!(changed_trace, expected.trace);
    drop(journal);
    assert!(VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected).is_err());

    // A separately supplied replacement trace does not repair the invalid
    // Intent/Accepted ordering. This is conditional integrity, not authority.
    let mut rebound = expected;
    rebound.trace = changed_trace;
    assert!(VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &rebound).is_err());
    assert_eq!(fs::read(&path).unwrap(), changed);
    assert_eq!(index.capture_verified_history_pin().unwrap(), native_before);
}

#[derive(Clone, Copy)]
enum BarrierFault {
    None,
    OmitSupersession,
    DuplicateSupersession,
    EscrowAfterSupersession,
    ActualAlternativeMutation,
}

fn actual_recovered_fixture(
    temp: &Temp,
    fault: BarrierFault,
) -> (
    Fixture,
    SourceCompactIndex,
    NativeHistoricalInlineDatabaseV1,
) {
    let compact = matches!(fault, BarrierFault::EscrowAfterSupersession);
    let mut index = metadata(temp, compact);
    let mut fixture = fixture(temp, &index);
    fixture.start(&index);
    let before = index.capture_verified_history_pin().unwrap();
    let view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let source = view
        .prepare_historical_inline_database("updateAsync", arguments())
        .unwrap();
    drop(view);
    let original = index
        .prepare_inline_database(&before.metadata, "updateAsync", arguments())
        .unwrap();
    let mut pending = fixture.begin(2, Action::AppendPending, &index, Some(&original));

    // Real original-copy publication occurs before native IO. Only the WAL
    // record is deliberately delayed in the EscrowAfterSupersession case.
    let original_permit = if compact {
        assert!(original.append_plan().unwrap().intermediate().is_some());
        let permit = index
            .retain_verified_history_generation(
                &temp.escrow,
                std::slice::from_ref(&before),
                crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
            )
            .unwrap();
        let scalar = permit
            .independently_retained_pin()
            .canonical_scalar_bytes()
            .unwrap();
        pending.escrow = Some(
            bytes(&obj(&[
                ("profile", s("TL-ACTIVE-GENERATION-ESCROW-CHAIN-13-1")),
                ("previous", V::Null),
                ("nativeReadContext", original.durable_read_context()),
                ("expectedEscrowFieldsHex", s(hex(&scalar))),
            ]))
            .unwrap(),
        );
        Some(permit)
    } else {
        None
    };
    let candidate = if compact {
        index.native_main_test_compaction_fault("after-current-sync");
        Candidate::Intermediate
    } else {
        index.test_fail_after_partial_write();
        Candidate::Before
    };
    assert!(index
        .append_inline_database(&original, original_permit.as_ref())
        .is_err());
    drop(index);
    let recovery = if compact {
        Recovery::RejectIncompleteTail
    } else {
        Recovery::DiscardIncompleteTail
    };
    let (mut index, observation) = SourceCompactIndex::reopen_inline_database(
        &temp.index,
        meta_ns(),
        &original,
        candidate,
        recovery,
    )
    .unwrap();
    let reopened = index.capture_verified_history_pin().unwrap();
    let (replacement, mapping) = index
        .supersede_inline_database(&original, &observation)
        .unwrap();
    let binding = index
        .require_inline_native_supersession(&original, &observation, &replacement, &mapping)
        .unwrap();
    let candidates = crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(
        replacement.append_plan().unwrap(), replacement.durable_read_context(),
    ).unwrap();
    let mut mutations_hash = sha256(candidates.exact_prepared_mutations());
    if matches!(fault, BarrierFault::ActualAlternativeMutation) {
        let mut other_arguments = arguments();
        other_arguments[1] = obj(&[("$set", obj(&[("value", V::number(12.))]))]);
        let alternative = index
            .prepare_inline_database(&index.snapshot().unwrap(), "updateAsync", other_arguments)
            .unwrap();
        let alternative_candidates = crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(
            alternative.append_plan().unwrap(), alternative.durable_read_context(),
        ).unwrap();
        mutations_hash = sha256(alternative_candidates.exact_prepared_mutations());
        assert_ne!(
            mutations_hash,
            sha256(candidates.exact_prepared_mutations())
        );
    }
    pending.recovery = Some(
        bytes(&obj(&[
            ("profile", s("TL-MAIN-ACTIVE-INLINE-NATIVE-RECOVERY-13-1")),
            (
                "candidate",
                s(if compact { "Intermediate" } else { "Before" }),
            ),
            ("snapshot", mode4_physical(&reopened)),
            (
                "discardedBytes",
                s(observation.recovery_report().discarded_bytes.to_string()),
            ),
            ("previousRecovery", V::Null),
            (
                "verifiedReopenedHistory",
                s(hex(&reopened.canonical_scalar_bytes().unwrap())),
            ),
            ("binding", binding),
            (
                "replacementCandidates",
                candidates.independently_retained_facts().clone(),
            ),
            ("replacementMutationsSha256", s(hex(&mutations_hash))),
        ]))
        .unwrap(),
    );
    if !matches!(fault, BarrierFault::OmitSupersession) {
        fixture.barrier(&pending, "NativeSupersessionIntent", None, None, None);
    }
    if matches!(fault, BarrierFault::DuplicateSupersession) {
        fixture.barrier(&pending, "NativeSupersessionIntent", None, None, None);
    }
    if compact {
        fixture.barrier(&pending, "EscrowBoundIntent", None, None, None);
    }
    let permit = copy_before_attempt(temp, &mut fixture, &mut pending, &mut index, &replacement);
    index
        .append_inline_database(&replacement, permit.as_ref())
        .unwrap();
    index.resolve_inline_database_post(&replacement).unwrap();
    fixture.finish(pending, &index);
    (fixture, index, source)
}

#[test]
fn real_before_replacement_fixture_closes_and_retains_unresolved_source_await() {
    let temp = Temp::new();
    let (mut fixture, mut index, mut source) = actual_recovered_fixture(&temp, BarrierFault::None);
    let expected = fixture.store.capture_inline_cold_pin(&meta_ns()).unwrap();
    drop(fixture);
    let reader =
        VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected).unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let closed = reader
        .verify_inline_operation(&mut index, 2, &mut source)
        .unwrap();
    assert_eq!(closed.recovery_links(), 1);
    assert!(source.append_verified());
    assert!(!source.resolved());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
}

#[test]
fn missing_duplicate_late_and_genuine_alternative_replacement_claims_cannot_resume_same_source() {
    for fault in [
        BarrierFault::OmitSupersession,
        BarrierFault::DuplicateSupersession,
        BarrierFault::EscrowAfterSupersession,
        BarrierFault::ActualAlternativeMutation,
    ] {
        let temp = Temp::new();
        let (mut fixture, mut index, mut source) = actual_recovered_fixture(&temp, fault);
        let expected = fixture.store.capture_inline_cold_pin(&meta_ns()).unwrap();
        let path = fixture.store.journal_path();
        drop(fixture);
        let raw = fs::read(&path).unwrap();
        let before = index.capture_verified_history_pin().unwrap();
        if let Ok(reader) =
            VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected)
        {
            assert!(reader
                .verify_inline_operation(&mut index, 2, &mut source)
                .is_err());
        }
        assert!(!source.append_verified());
        assert!(!source.resolved());
        assert_eq!(index.capture_verified_history_pin().unwrap(), before);
        assert_eq!(fs::read(&path).unwrap(), raw);
    }
}

#[test]
fn a_genuine_same_kv_copy_at_the_configured_root_cannot_replace_the_original_store_identity() {
    let temp = Temp::new();
    let foreign_temp = Temp::new();
    let mut index = metadata(&temp, true);
    // Run the same genuine fixture operations on a second actual store. This
    // retains matching logical sequence as well as matching complete KV.
    let mut foreign_index = metadata(&foreign_temp, true);
    let before = index.capture_verified_history_pin().unwrap();
    let foreign_before = foreign_index.capture_verified_history_pin().unwrap();
    assert_eq!(before.storage.content, foreign_before.storage.content);
    assert_eq!(before.metadata.sequence, foreign_before.metadata.sequence);
    assert_eq!(
        before.storage.publication.generation,
        foreign_before.storage.publication.generation
    );
    assert_ne!(
        before.storage.persistent_identity,
        foreign_before.storage.persistent_identity
    );
    let view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let mut source = view
        .prepare_historical_inline_database("updateAsync", arguments())
        .unwrap();
    drop(view);
    let actual = index
        .prepare_inline_database(&before.metadata, "updateAsync", arguments())
        .unwrap();
    assert!(actual.append_plan().unwrap().intermediate().is_some());
    let mut fixture = fixture(&temp, &index);
    fixture.start(&index);
    let mut pending = fixture.begin(2, Action::AppendPending, &index, Some(&actual));
    let actual_permit = index
        .retain_verified_history_generation(
            &temp.escrow,
            std::slice::from_ref(&before),
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    // This is a real, separately published copy at the SAME configured root.
    // Its origin is foreign despite genuinely identical complete KV contents.
    let foreign_permit = foreign_index
        .retain_verified_history_generation(
            &temp.escrow,
            std::slice::from_ref(&foreign_before),
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let foreign_pin = foreign_permit.independently_retained_pin();
    assert!(VerifiedOriginalMetadataViewV1::open_copy(foreign_pin, &foreign_before).is_ok());
    pending.escrow = Some(
        bytes(&obj(&[
            ("profile", s("TL-ACTIVE-GENERATION-ESCROW-CHAIN-13-1")),
            ("previous", V::Null),
            ("nativeReadContext", actual.durable_read_context()),
            (
                "expectedEscrowFieldsHex",
                s(hex(&foreign_pin.canonical_scalar_bytes().unwrap())),
            ),
        ]))
        .unwrap(),
    );
    fixture.barrier(&pending, "EscrowBoundIntent", None, None, None);
    index
        .append_inline_database(&actual, Some(&actual_permit))
        .unwrap();
    index.resolve_inline_database_post(&actual).unwrap();
    fixture.finish(pending, &index);
    let expected = fixture.store.capture_inline_cold_pin(&meta_ns()).unwrap();
    let path = fixture.store.journal_path();
    drop(fixture);
    let raw = fs::read(&path).unwrap();
    let after = index.capture_verified_history_pin().unwrap();
    let foreign_after = foreign_index.capture_verified_history_pin().unwrap();
    let reader =
        VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected).unwrap();
    assert!(reader
        .verify_inline_operation(&mut index, 2, &mut source)
        .is_err());
    assert!(!source.append_verified());
    assert!(!source.resolved());
    assert_eq!(index.capture_verified_history_pin().unwrap(), after);
    assert_eq!(
        foreign_index.capture_verified_history_pin().unwrap(),
        foreign_after
    );
    drop(reader); // Windows retains the WAL file lock throughout verification.
    assert_eq!(fs::read(&path).unwrap(), raw);
}
