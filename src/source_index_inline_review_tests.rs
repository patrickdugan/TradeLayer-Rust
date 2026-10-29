use super::*;
use std::io::Write;

fn tail(path: &std::path::Path) {
    let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(b"TXN1short").unwrap();
    file.sync_all().unwrap();
}

fn tree(root: &std::path::Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &std::path::Path, at: &std::path::Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in std::fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.push((
                    path.strip_prefix(root).unwrap().into(),
                    std::fs::read(path).unwrap(),
                ));
            }
        }
    }
    let mut out = vec![];
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn independently_forged_same_sequence_inline_kv_and_extra_key_never_repair_tail() {
    for extra in [false, true] {
        let temp = Temp::new();
        let index = make(&temp);
        let retained = plan(&index);
        let path = index.journal.path().to_owned();
        let directory = index.journal.get(DIRECTORY).unwrap().unwrap().to_vec();
        let mut rows = index.page(&index.directory.pages[0]).unwrap();
        if !extra {
            rows[0].insert("value", V::number(90.)).unwrap();
        }
        let mut changes = vec![
            crate::storage::Mutation::Put {
                key: DIRECTORY.into(),
                value: directory.clone(),
            },
            crate::storage::Mutation::Put {
                key: page_key(0),
                value: encode_page(&rows).unwrap(),
            },
        ];
        if extra {
            changes.push(crate::storage::Mutation::Put {
                key: "outside-selected-inline-row".into(),
                value: b"independent extra KV".to_vec(),
            });
        }
        drop(index);
        let alternate = Temp::new();
        let (mut raw, _) =
            crate::storage::Journal::open(&alternate.root, ns(), Recovery::RejectIncompleteTail)
                .unwrap();
        raw.commit(vec![crate::storage::Mutation::Put {
            key: DIRECTORY.into(),
            value: directory,
        }])
        .unwrap();
        raw.commit(changes).unwrap();
        assert_eq!(
            raw.sequence().unwrap(),
            retained.append_plan().unwrap().before().logical_sequence
        );
        let alternate_path = raw.path().to_owned();
        drop(raw);
        std::fs::write(&path, std::fs::read(alternate_path).unwrap()).unwrap();
        tail(&path);
        let before = tree(&temp.root);
        assert!(matches!(
            SourceCompactIndex::reopen_inline_database(
                &temp.root,
                ns(),
                &retained,
                NativePreparedValidityCandidateV1::Before,
                Recovery::DiscardIncompleteTail,
            ),
            Err(CompactIndexError::Storage(
                crate::storage::StorageError::Corrupt(
                    "unexpected independently pinned complete KV prefix"
                )
            ))
        ));
        assert_eq!(
            tree(&temp.root),
            before,
            "rejected full-KV pin changed files"
        );
    }
}

#[test]
fn inline_mapping_rejects_foreign_current_and_stale_install_and_post_supersession() {
    let temp = Temp::new();
    let index = make(&temp);
    let original = plan(&index);
    drop(index);
    let (mut index, before_observation) = SourceCompactIndex::reopen_inline_database(
        &temp.root,
        ns(),
        &original,
        NativePreparedValidityCandidateV1::Before,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    let (replacement, mapping) = index
        .supersede_inline_database(&original, &before_observation)
        .unwrap();
    let other_temp = Temp::new();
    let other = make(&other_temp);
    let other_before = other.capture_verified_history_pin().unwrap();
    assert!(other
        .require_inline_installation(&original, &replacement, &mapping)
        .is_err());
    assert!(other
        .supersede_inline_database(&original, &before_observation)
        .is_err());
    assert_eq!(other.capture_verified_history_pin().unwrap(), other_before);
    drop(other);
    commit(&mut index, &replacement, &temp);
    let post = index.capture_verified_history_pin().unwrap();
    assert!(index
        .require_inline_installation(&original, &replacement, &mapping)
        .is_err());
    assert!(index
        .supersede_inline_database(&original, &before_observation)
        .is_err());
    assert!(index.append_inline_database(&replacement, None).is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    assert_eq!(
        index.resolve_inline_database_post(&replacement).unwrap(),
        DependencyReturn::Pending(V::number(1.))
    );
    drop(index);
    let post_files = tree(&temp.root);
    let (index, observed_post) = SourceCompactIndex::reopen_inline_database(
        &temp.root,
        ns(),
        &replacement,
        NativePreparedValidityCandidateV1::Post,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    assert!(index
        .supersede_inline_database(&replacement, &observed_post)
        .is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    // The writer lock remains held; compare persisted journal and catalog files
    // after dropping it rather than attempting a second concurrent writer.
    drop(index);
    assert_eq!(tree(&temp.root), post_files);
}
