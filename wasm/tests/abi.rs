use tradelayer_wasm::handle;

fn call(req: &str) -> String {
    String::from_utf8(handle(req.as_bytes())).unwrap()
}

#[test]
fn version_and_typed_errors() {
    assert!(call(r#"{"op":"version"}"#).starts_with(r#"{"ok":true,"crate":"0.1.0-rc.2"}"#));
    assert!(call("not json").contains(r#""code":"BadRequest""#));
    assert!(call(r#"{"op":"nope"}"#).contains(r#""code":"UnknownOp""#));
    assert!(call(r#"{"op":"pg_nope"}"#).contains(r#""code":"UnknownOp""#));
}
