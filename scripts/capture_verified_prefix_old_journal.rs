#![forbid(unsafe_code)]
use std::io::Write;
use std::path::PathBuf;
use tradelayer_core::storage::{Journal, Mutation, Namespace, Recovery};

fn put(key: &str, value: &[u8]) -> Mutation {
    Mutation::Put {
        key: key.into(),
        value: value.to_vec(),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn main() {
    let output = PathBuf::from(std::env::args_os().nth(1).expect("output directory"));
    std::fs::create_dir_all(&output).unwrap();
    let parent = output.canonicalize().unwrap();
    let root = parent.join(format!("old-journal-capture-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let namespace = Namespace::new("LTC", false, "txIndex").unwrap();
    let (mut prior, initial_report) =
        Journal::open(&root, namespace.clone(), Recovery::RejectIncompleteTail).unwrap();
    prior
        .commit(vec![put("b", b"second"), put("a", b"first")])
        .unwrap();
    prior
        .commit(vec![
            Mutation::Delete { key: "a".into() },
            put("c", b"third"),
        ])
        .unwrap();
    let path = prior.path().to_owned();
    let before_sequence = prior.sequence().unwrap();
    let before_rows: Vec<_> = prior
        .entries()
        .unwrap()
        .map(|(k, v)| (k.to_owned(), v.to_vec()))
        .collect();
    drop(prior);
    std::fs::write(
        parent.join("verified_prefix_old_journal_v1.before.bin"),
        std::fs::read(&path).unwrap(),
    )
    .unwrap();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    file.write_all(b"TXN1short").unwrap();
    file.sync_all().unwrap();
    drop(file);
    std::fs::write(
        parent.join("verified_prefix_old_journal_v1.damaged.bin"),
        std::fs::read(&path).unwrap(),
    )
    .unwrap();
    let (restored, report) =
        Journal::open_existing(&root, namespace, Recovery::DiscardIncompleteTail).unwrap();
    let mut observations = format!("initial_discarded_bytes\t{}\nbefore_sequence\t{}\ndiscarded_bytes\t{}\nafter_sequence\t{}\n", initial_report.discarded_bytes, before_sequence, report.discarded_bytes, restored.sequence().unwrap());
    for (key, value) in before_rows {
        observations.push_str(&format!(
            "before_row\t{}\t{}\n",
            hex(key.as_bytes()),
            hex(&value)
        ));
    }
    for (key, value) in restored.entries().unwrap() {
        observations.push_str(&format!(
            "after_row\t{}\t{}\n",
            hex(key.as_bytes()),
            hex(value)
        ));
    }
    drop(restored);
    std::fs::write(
        parent.join("verified_prefix_old_journal_v1.after.bin"),
        std::fs::read(&path).unwrap(),
    )
    .unwrap();
    std::fs::write(
        parent.join("verified_prefix_old_journal_v1.observations.tsv"),
        &observations,
    )
    .unwrap();
    let canonical = root.canonicalize().unwrap();
    assert_eq!(canonical.parent(), Some(parent.as_path()));
    std::fs::remove_dir_all(&canonical).unwrap();
    print!("{observations}");
}
