#![forbid(unsafe_code)]
extern crate tradelayer_core;
pub use tradelayer_core::{
    compact_journal, property_catalog, source_contract_replay, storage, validity_source,
};
#[allow(dead_code)]
#[path = "legacy_hash256.rs"]
mod hash256;
#[allow(dead_code)]
#[path = "legacy_store_13_2.rs"]
mod old;
use source_contract_replay::source_durable::SourceDurableFilesystem;
use std::path::PathBuf;
use validity_source::SourceValue as V;
fn main() {
    let output = PathBuf::from(std::env::args_os().nth(1).expect("output directory"));
    std::fs::create_dir_all(&output).unwrap();
    let parent = output.canonicalize().unwrap();
    let root = parent.join(format!("legacy-control-capture-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let mut store = old::NativeIntentStoreV1::create(
        &root,
        storage::Namespace::new("LTC", false, "consensus").unwrap(),
        V::Null,
        V::Null,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let mut states = vec![("idle", store.state().unwrap().diagnostic_json().unwrap())];
    let mut permit = store
        .sync_command_intent(
            &store.pin().unwrap(),
            b"legacy command bytes",
            V::Bool(false),
            1024,
        )
        .unwrap();
    states.push(("command", store.state().unwrap().diagnostic_json().unwrap()));
    store
        .sync_accepted(&mut permit, b"legacy accepted bytes", V::Bool(true))
        .unwrap();
    states.push((
        "accepted",
        store.state().unwrap().diagnostic_json().unwrap(),
    ));
    store.sync_ack(&mut permit).unwrap();
    states.push(("ack", store.state().unwrap().diagnostic_json().unwrap()));
    drop(store);
    for (name, state) in states {
        std::fs::write(parent.join(format!("native_store_13_2_{name}.json")), state).unwrap();
    }
    let mut blobs = std::fs::read_dir(root.join("native13-control-blobs"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .collect::<Vec<_>>();
    blobs.sort();
    let mut observations = String::new();
    for path in blobs {
        let raw = std::fs::read(&path).unwrap();
        let hex = raw.iter().map(|b| format!("{b:02x}")).collect::<String>();
        observations.push_str(&format!(
            "{}\t{}\n",
            path.file_name().unwrap().to_str().unwrap(),
            hex
        ));
    }
    std::fs::write(parent.join("native_store_13_2_blobs.tsv"), observations).unwrap();
    let canonical = root.canonicalize().unwrap();
    assert_eq!(canonical.parent(), Some(parent.as_path()));
    std::fs::remove_dir_all(canonical).unwrap();
    println!("captured four exact legacy states and content-addressed blobs");
}
