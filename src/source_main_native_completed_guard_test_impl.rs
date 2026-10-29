#[cfg(test)]
std::thread_local! {
    static NATIVE_PUBLICATION_REMOVE_ACCEPTED_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(test)]
fn native_main_publication_guard_test_hook(controller: &SourceMainNativeRowSessionV1) {
    if !NATIVE_PUBLICATION_REMOVE_ACCEPTED_TEST.with(|flag| flag.replace(false)) {
        return;
    }
    assert!(matches!(
        controller.pc,
        NativeMainRowControllerPcV1::Accepted
    ));
    assert_eq!(controller.store.state().unwrap().get("kind"), s("Idle"));
    let accepted = controller
        .publication
        .prepared
        .as_ref()
        .unwrap()
        .accepted_blob
        .as_ref()
        .unwrap();
    let root = &controller
        .permit
        .independently_retained_pin()
        .canonical_root;
    let path = root
        .join("native13-control-blobs")
        .join(format!("{}.tlc13", hex(&accepted.sha256)));
    assert!(path.starts_with(root));
    std::fs::remove_file(path).unwrap();
}
