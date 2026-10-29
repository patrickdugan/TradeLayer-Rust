use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    source: PathBuf,
    escrow: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("NATIVE_ESCROW_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-generation-escrow-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let source = root.join("source");
        let escrow = root.join("escrow");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&escrow).unwrap();
        Self {
            root,
            source: source.canonicalize().unwrap(),
            escrow: escrow.canonicalize().unwrap(),
        }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let parent = self.root.parent().unwrap();
        if let Ok(root) = self.root.canonicalize() {
            assert_eq!(root.parent(), Some(parent));
            let _ = fs::remove_dir_all(root);
        }
    }
}
fn ns() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn store(temp: &Temp) -> CompactingJournal {
    CompactingJournal::open(&temp.source, ns(), Recovery::RejectIncompleteTail)
        .unwrap()
        .0
}
fn put(j: &mut CompactingJournal, key: &str, value: &[u8]) {
    j.commit(vec![Mutation::Put {
        key: key.into(),
        value: value.to_vec(),
    }])
    .unwrap();
}
#[test]
fn escrow_actual_old_generation_survives_pruning_and_process_owner_close() {
    let temp = Temp::new();
    let mut j = store(&temp);
    put(&mut j, "a", b"one");
    let first = j.verified_prefix_pin().unwrap();
    put(&mut j, "a", b"two");
    put(&mut j, "b", b"present");
    let last = j.verified_prefix_pin().unwrap();
    j.configure(CompactionConfig {
        target_log_bytes: 65536,
        retained_ancestors: 0,
    })
    .unwrap();
    let permit = j
        .retain_verified_generation(
            &temp.escrow,
            &[first.clone(), last.clone()],
            GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let pin = permit.independently_retained_pin().clone();
    let plan = j.prepare_compaction().unwrap();
    j.publish_compaction_with_escrow(&plan, &permit).unwrap();
    drop(plan);
    j.prune_retention().unwrap();
    assert!(!gen_path(&j.catalog, first.publication.generation).exists());
    drop(permit);
    drop(j);
    let mut legacy =
        CompactingJournal::open_existing(&temp.source, ns(), Recovery::RejectIncompleteTail)
            .unwrap()
            .0;
    assert_eq!(legacy.get("a").unwrap(), Some(&b"two"[..]));
    legacy.prune_retention().unwrap();
    drop(legacy);
    let early = pin.open_historical(&first).unwrap();
    assert_eq!(early.get("a"), Some(&b"one"[..]));
    assert_eq!(early.get("b"), None);
    assert_ne!(
        early.copied_file,
        ns().path(&gen_path(
            &first.canonical_catalog,
            first.publication.generation
        ))
    );
    assert_eq!(early.original, first);
    let latest = pin.open_historical(&last).unwrap();
    assert_eq!(latest.get("a"), Some(&b"two"[..]));
    assert_eq!(latest.complete_kv_receipt().unwrap(), last.content);
}
#[test]
fn escrow_foreign_and_stale_permits_cannot_authorize_publication() {
    let temp = Temp::new();
    let other = Temp::new();
    let mut a = store(&temp);
    let mut b = store(&other);
    put(&mut a, "a", b"1");
    put(&mut b, "a", b"1");
    let before = a.verified_prefix_pin().unwrap();
    let permit = a
        .retain_verified_generation(
            &temp.escrow,
            &[before],
            GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let plan = b.prepare_compaction().unwrap();
    assert!(b.publish_compaction_with_escrow(&plan, &permit).is_err());
    put(&mut a, "x", b"new");
    let plan = a.prepare_compaction().unwrap();
    assert!(a.publish_compaction_with_escrow(&plan, &permit).is_err());
}
#[test]
fn escrow_missing_or_corrupt_copy_fails_before_original_publication() {
    for part in ["log", "manifest", "record"] {
        let temp = Temp::new();
        let mut j = store(&temp);
        put(&mut j, "a", b"1");
        let before = j.verified_prefix_pin().unwrap();
        let permit = j
            .retain_verified_generation(
                &temp.escrow,
                std::slice::from_ref(&before),
                GenerationEscrowDurabilityV1::FileSyncOnly,
            )
            .unwrap();
        let pin = permit.independently_retained_pin();
        let path = match part {
            "log" => ns().path(&pin.copied_generation),
            "manifest" => pin.copied_generation.join("GENINFO"),
            _ => pin.copied_generation.join("RECORD"),
        };
        let mut raw = fs::read(&path).unwrap();
        raw[0] ^= 1;
        fs::write(&path, raw).unwrap();
        let plan = j.prepare_compaction().unwrap();
        assert!(j.publish_compaction_with_escrow(&plan, &permit).is_err());
        assert_eq!(j.verified_prefix_pin().unwrap(), before);
        assert!(pin.open_historical(&before).is_err());
    }
}
#[test]
fn escrow_wrong_independent_origin_or_copy_identity_is_never_relabelled() {
    let temp = Temp::new();
    let mut j = store(&temp);
    put(&mut j, "a", b"1");
    let before = j.verified_prefix_pin().unwrap();
    let permit = j
        .retain_verified_generation(
            &temp.escrow,
            std::slice::from_ref(&before),
            GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let pin = permit.independently_retained_pin();
    let mut wrong = pin.clone();
    wrong.original_current.persistent_identity.push('x');
    assert!(wrong.open_historical(&before).is_err());
    let mut wrong = pin.clone();
    wrong.copied_generation = wrong.original_current.canonical_catalog.clone();
    assert!(wrong.open_historical(&before).is_err());
    let mut wrong = before.clone();
    wrong.content.chain_sha256d[0] ^= 1;
    assert!(pin.open_historical(&wrong).is_err());
}
#[test]
fn escrow_ordinary_native_append_requires_no_copy_or_escrow_permit() {
    let temp = Temp::new();
    let mut j = store(&temp);
    let plan = j
        .prepare_native_append(vec![Mutation::Put {
            key: "a".into(),
            value: b"1".to_vec(),
        }])
        .unwrap();
    assert!(plan.intermediate().is_none());
    j.commit_prepared_native_append_with_escrow(&plan, None)
        .unwrap();
    assert_eq!(fs::read_dir(&temp.escrow).unwrap().count(), 0);
}
#[test]
fn escrow_publication_record_is_required_even_when_copy_bytes_are_present() {
    let temp = Temp::new();
    let mut j = store(&temp);
    put(&mut j, "a", b"1");
    let point = j.verified_prefix_pin().unwrap();
    let permit = j
        .retain_verified_generation(
            &temp.escrow,
            std::slice::from_ref(&point),
            GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let pin = permit.independently_retained_pin();
    fs::remove_file(pin.copied_generation.join("RECORD")).unwrap();
    assert!(pin.open_historical(&point).is_err());
    let plan = j.prepare_compaction().unwrap();
    assert!(j.publish_compaction_with_escrow(&plan, &permit).is_err());
    assert_eq!(j.verified_prefix_pin().unwrap(), point);
}

#[test]
fn escrow_unpublished_durable_copy_produces_no_prune_permit() {
    let temp = Temp::new();
    let mut j = store(&temp);
    put(&mut j, "a", b"1");
    let before = j.verified_prefix_pin().unwrap();
    ESCROW_FAIL_BEFORE_PUBLISH.with(|flag| flag.set(true));
    assert!(j
        .retain_verified_generation(
            &temp.escrow,
            std::slice::from_ref(&before),
            GenerationEscrowDurabilityV1::FileSyncOnly
        )
        .is_err());
    assert_eq!(j.verified_prefix_pin().unwrap(), before);
    assert!(fs::read_dir(&temp.escrow).unwrap().all(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".pending-")));
    // Only rerun the metadata copy, with no source/action execution.
    let permit = j
        .retain_verified_generation(
            &temp.escrow,
            std::slice::from_ref(&before),
            GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    assert_eq!(
        permit
            .independently_retained_pin()
            .open_historical(&before)
            .unwrap()
            .get("a"),
        Some(&b"1"[..])
    );
}

#[test]
fn escrow_actual_cross_publication_native_append_requires_sealed_synced_copy() {
    let temp = Temp::new();
    let mut j = store(&temp);
    j.configure(CompactionConfig {
        target_log_bytes: 65536,
        retained_ancestors: 0,
    })
    .unwrap();
    let mut attempts = 0;
    let plan = loop {
        attempts += 1;
        assert!(
            attempts < 1000,
            "finite raw metadata fixture must reach real compaction"
        );
        let plan = j
            .prepare_native_append(vec![Mutation::Put {
                key: "raw-metadata".into(),
                value: vec![7; 8000],
            }])
            .unwrap();
        if plan.intermediate().is_some() {
            break plan;
        }
        j.commit_prepared_native_append_with_escrow(&plan, None)
            .unwrap();
    };
    let before = j.verified_prefix_pin().unwrap();
    assert!(j
        .commit_prepared_native_append_with_escrow(&plan, None)
        .is_err());
    assert_eq!(j.verified_prefix_pin().unwrap(), before);
    let permit = j
        .retain_verified_generation(
            &temp.escrow,
            std::slice::from_ref(&before),
            GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let pin = permit.independently_retained_pin().clone();
    let post = j
        .commit_prepared_native_append_with_escrow(&plan, Some(&permit))
        .unwrap();
    assert_ne!(post.publication, before.publication);
    drop(plan);
    j.prune_retention().unwrap();
    drop(j);
    drop(permit);
    assert_eq!(
        pin.open_historical(&before)
            .unwrap()
            .get("raw-metadata")
            .unwrap(),
        &[7u8; 8000]
    );
}

#[path = "compact_independent_escrow_tests.rs"]
mod independent_prefix_tests;

#[path = "compact_escrow_descriptor_codec_tests.rs"]
mod descriptor_codec_tests;
