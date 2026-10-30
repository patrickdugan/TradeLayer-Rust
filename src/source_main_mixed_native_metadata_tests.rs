use super::*;
use crate::source_compact_index_v2::{
    NativePreparedInlineDatabaseV1, NativePreparedValidityOperationV1,
    NativePreparedValidityQueryV1, NativePreparedValidityWriteV1, SourceCompactIndex,
};
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("MIXED_NATIVE_META_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&parent).unwrap();
        let root = parent.canonicalize().unwrap().join(format!(
            "mixed-facts-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn origin() -> V {
    obj(&[
        ("profile", s(MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE)),
        ("actualNativeFactScope", s(MIXED_SOURCE_NATIVE_FACT_SCOPE)),
        ("requiredNativeFactProfile", s(ACTUAL_NATIVE_FACT_PROFILE)),
        (
            "requiredOrdinaryNativeFactProfile",
            s(ACTUAL_VALIDITY_PLAN_FACT_PROFILE),
        ),
        ("ordinaryNativeFactScope", s(ACTUAL_VALIDITY_FACT_SCOPE)),
        (
            "requiredInlineServiceProfile",
            s(crate::source_contract_runtime::NATIVE_MAIN_INLINE_ACTIVE_SERVICE_PROFILE),
        ),
        (
            "underlyingSourceHistoryProfile",
            s(MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE),
        ),
        ("acknowledgedWindowLimit", s("256")),
    ])
}
fn setup(
    raw_origin: V,
) -> (
    Temp,
    SourceCompactIndex,
    NativePreparedValidityWriteV1,
    NativePreparedInlineDatabaseV1,
    ActiveServiceWalV1,
) {
    let t = Temp::new();
    let mut index = SourceCompactIndex::create(
        &t.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "explicit metadata operations",
    )
    .unwrap();
    index
        .import_row(
            &index.snapshot().unwrap(),
            obj(&[
                ("_id", s("tx-1-id")),
                (
                    "value",
                    obj(&[("payload", s("metadata input")), ("txId", s("id"))]),
                ),
            ]),
        )
        .unwrap();
    let NativePreparedValidityQueryV1::Selected(selected) = index
        .prepare_validity_read(
            &index.snapshot().unwrap(),
            s("id"),
            V::number(1.),
            V::Bool(true),
            V::Undefined,
        )
        .unwrap()
    else {
        panic!("selected")
    };
    let NativePreparedValidityOperationV1::Write(write) = selected
        .prepare_write(&index, &index.snapshot().unwrap())
        .unwrap()
    else {
        panic!("write")
    };
    let inline = index
        .prepare_inline_database(
            &index.snapshot().unwrap(),
            "updateAsync",
            vec![
                obj(&[("_id", s("MaxHeight"))]),
                obj(&[("$set", obj(&[("value", V::number(11.))]))]),
                obj(&[("upsert", V::Bool(true))]),
            ],
        )
        .unwrap();
    let wal = ActiveServiceWalV1::create(
        &t.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        raw_origin,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    (t, index, *write, inline, wal)
}
fn intent(
    index: &SourceCompactIndex,
    write: &NativePreparedValidityWriteV1,
    inline: &NativePreparedInlineDatabaseV1,
    wal: &mut ActiveServiceWalV1,
    ordinary: bool,
    stage: &str,
) {
    let (context, append) = if ordinary {
        (write.durable_read_context(), write.append_plan())
    } else {
        (inline.durable_read_context(), inline.append_plan().unwrap())
    };
    let actual=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(append,context.clone()).unwrap();
    let history = index
        .capture_verified_history_pin()
        .unwrap()
        .canonical_scalar_bytes()
        .unwrap();
    let blobs = vec![
        history,
        bytes(actual.independently_retained_facts()).unwrap(),
        actual.exact_prepared_mutations().to_vec(),
    ];
    let refs = blobs
        .iter()
        .map(|b| ActiveServiceWalBlobV1::of(b).unwrap())
        .collect::<Vec<_>>();
    let names = if ordinary {
        [
            "ordinaryNativeReadContext",
            "ordinaryNativeBeforeHistory",
            "ordinaryNativeCandidates",
            "ordinaryNativeMutations",
            "ordinarySourceSelection",
        ]
    } else {
        [
            "actualNativeReadContext",
            "actualNativeBeforeHistory",
            "actualNativeCandidates",
            "actualNativeMutations",
            "actualSourceSelection",
        ]
    };
    let selection = if ordinary {
        obj(&[
            ("profile", s("TL-MAIN-MIXED-NATIVE-SELECTION-13-1")),
            ("kind", s("OrdinaryValidity")),
        ])
    } else {
        obj(&[("profile", s("TL-MAIN-ACTUAL-SINGLE-INLINE-SELECTION-13-1"))])
    };
    let record = obj(&[
        (
            "profile",
            wal.origin_context().unwrap().get("origin").get("profile"),
        ),
        ("stage", s(stage)),
        (names[0], context),
        (names[1], refs[0].value()),
        (names[2], refs[1].value()),
        (names[3], refs[2].value()),
        (names[4], selection),
    ]);
    let mut p = wal
        .prepare(&wal.pin().unwrap(), record, blobs, refs)
        .unwrap();
    wal.attempt(&mut p).unwrap();
}
#[test]
fn explicit_mixed_origin_binds_each_actual_kind_only_to_its_own_synced_fields() {
    for ordinary in [false, true] {
        let (_t, index, write, inline, mut wal) = setup(origin());
        intent(&index, &write, &inline, &mut wal, ordinary, "Intent");
        let before = index.capture_verified_history_pin().unwrap();
        let one = wal.bind_actual_validity_fact(&index, &write).unwrap();
        let two = wal.bind_actual_inline_native_fact(&index, &inline).unwrap();
        if ordinary {
            one.require_synced_intent(&wal).unwrap();
            assert!(two.require_synced_native_intent(&wal).is_err());
        } else {
            two.require_synced_native_intent(&wal).unwrap();
            assert!(one.require_synced_intent(&wal).is_err());
        }
        one.capture_pending_fact(&wal, &index, &write).unwrap();
        two.capture_pending_plan_fact(&wal, &index, &inline)
            .unwrap();
        assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    }
}
#[test]
fn mixed_replacement_and_escrow_stages_use_actual_tlnm_context_before_any_native_io() {
    for stage in ["NativeSupersessionIntent", "EscrowBoundIntent"] {
        for ordinary in [false, true] {
            let (_t, index, write, inline, mut wal) = setup(origin());
            intent(&index, &write, &inline, &mut wal, ordinary, stage);
            if ordinary {
                wal.bind_actual_validity_fact(&index, &write)
                    .unwrap()
                    .require_synced_intent(&wal)
                    .unwrap();
            } else {
                wal.bind_actual_inline_native_fact(&index, &inline)
                    .unwrap()
                    .require_synced_native_intent(&wal)
                    .unwrap();
            }
        }
    }
}
#[test]
fn profile_downgrade_or_missing_ordinary_requirement_cannot_create_a_mixed_binding() {
    for field in [
        "profile",
        "ordinaryNativeFactScope",
        "requiredOrdinaryNativeFactProfile",
        "acknowledgedWindowLimit",
    ] {
        let mut raw = origin();
        raw.insert(field, s("wrong")).unwrap();
        let (_t, index, write, inline, wal) = setup(raw);
        let before = index.capture_verified_history_pin().unwrap();
        assert!(wal.bind_actual_validity_fact(&index, &write).is_err());
        assert!(wal.bind_actual_inline_native_fact(&index, &inline).is_err());
        assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    }
}
#[test]
fn old_inline_origin_stays_inline_and_never_gains_ordinary_by_extra_profile_field() {
    let mut raw = origin();
    raw.insert("profile", s(ACTUAL_SOURCE_FACT_WAL_PROFILE))
        .unwrap();
    raw.insert("actualNativeFactScope", s(ACTUAL_SOURCE_FACT_SCOPE))
        .unwrap();
    let (_t, index, write, inline, mut wal) = setup(raw);
    intent(&index, &write, &inline, &mut wal, false, "Intent");
    wal.bind_actual_inline_native_fact(&index, &inline)
        .unwrap()
        .require_synced_native_intent(&wal)
        .unwrap();
    assert!(wal.bind_actual_validity_fact(&index, &write).is_err());
}
