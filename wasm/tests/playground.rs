use tradelayer_core::client::RpcValue;
use tradelayer_wasm::handle;

const ATOMS: u64 = 100_000_000;

fn call(req: &str) -> RpcValue {
    let text = String::from_utf8(handle(req.as_bytes())).unwrap();
    RpcValue::parse(&text).unwrap()
}

fn ok(req: &str) -> RpcValue {
    let out = call(req);
    assert_eq!(
        get(&out, "ok"),
        "true",
        "request {req} failed: {}",
        out.as_json()
    );
    out
}

fn code(req: &str) -> String {
    let out = call(req);
    assert_eq!(get(&out, "ok"), "false", "request {req} unexpectedly ok");
    unquote(&get(&out, "code"))
}

fn get(value: &RpcValue, name: &str) -> String {
    value
        .field(name)
        .unwrap()
        .unwrap_or_else(|| panic!("missing `{name}` in {}", value.as_json()))
        .as_json()
        .to_owned()
}

fn unquote(raw: &str) -> String {
    RpcValue::parse(raw).unwrap().as_string().unwrap()
}

fn path(value: &RpcValue, names: &[&str]) -> String {
    let mut current = value.clone();
    for name in names {
        current = current.field(name).unwrap().unwrap();
    }
    current.as_json().to_owned()
}

fn num(value: &RpcValue, names: &[&str]) -> u64 {
    path(value, names).parse().unwrap()
}

fn text(value: &RpcValue, names: &[&str]) -> String {
    unquote(&path(value, names))
}

fn new_session(id: &str) -> RpcValue {
    new_session_with(id, false)
}

fn new_session_with(id: &str, marks: bool) -> RpcValue {
    ok(&format!(
        r#"{{"op":"pg_new","id":"{id}","anchor_height":120000,"leverage":10,"notional":"0.01","marks":{marks},
        "price_1e4":1000000000,"accounts":[
          {{"name":"mm","tusd_atoms":{}}},{{"name":"alice","tusd_atoms":{}}},{{"name":"bob","tusd_atoms":{}}}]}}"#,
        200_000 * ATOMS,
        20_000 * ATOMS,
        20_000 * ATOMS
    ))
}

fn state(id: &str) -> RpcValue {
    ok(&format!(r#"{{"op":"pg_state","session":"{id}"}}"#))
}

fn block(id: &str, height: u64, ops: &str) -> RpcValue {
    ok(&format!(
        r#"{{"op":"pg_block","session":"{id}","height":{height},"ops":[{ops}]}}"#
    ))
}

fn account_row(state: &RpcValue, name: &str) -> RpcValue {
    let rows = state
        .field("accounts")
        .unwrap()
        .unwrap()
        .as_array()
        .unwrap();
    rows.into_iter()
        .find(|r| text(r, &["name"]) == name)
        .unwrap_or_else(|| panic!("no account {name}"))
}

fn position(state: &RpcValue, name: &str) -> Option<RpcValue> {
    state
        .field("positions")
        .unwrap()
        .unwrap()
        .as_array()
        .unwrap()
        .into_iter()
        .find(|r| text(r, &["account"]) == name)
}

fn quote_ladder() -> &'static str {
    r#"{"t":"cancel_all","account":"mm"},
    {"t":"order","account":"mm","side":"buy","qty":"5","price":"99950","post":true},
    {"t":"order","account":"mm","side":"buy","qty":"5","price":"99900","post":true},
    {"t":"order","account":"mm","side":"sell","qty":"5","price":"100050","post":true},
    {"t":"order","account":"mm","side":"sell","qty":"5","price":"100100","post":true}"#
}

#[test]
fn issue_grant_redeem_conserve_supply() {
    let id = "supply";
    let genesis = new_session(id);
    assert_eq!(num(&genesis, &["next_height"]), 120_000);
    let s0 = state(id);
    let supply0 = num(&s0, &["custody", "supply_atoms"]);
    assert_eq!(supply0, 1_000_000 * ATOMS + 240_000 * ATOMS);
    assert_eq!(path(&s0, &["custody", "balanced"]), "true");

    // Grant mints new supply to alice; the issuer's own balance is unchanged.
    block(
        id,
        120_000,
        r#"{"t":"grant","to":"alice","atoms":5000000000}"#,
    );
    let s1 = state(id);
    assert_eq!(
        num(&s1, &["custody", "supply_atoms"]),
        supply0 + 5_000_000_000
    );
    assert_eq!(
        num(&account_row(&s1, "alice"), &["available_atoms"]),
        20_000 * ATOMS + 5_000_000_000
    );
    assert_eq!(path(&s1, &["custody", "balanced"]), "true");

    // Redeem: alice sends back to the issuer, who burns it. Supply drops by the same amount.
    block(
        id,
        120_001,
        r#"{"t":"redeem","from":"alice","atoms":3000000000}"#,
    );
    let s2 = state(id);
    assert_eq!(
        num(&s2, &["custody", "supply_atoms"]),
        supply0 + 2_000_000_000
    );
    assert_eq!(
        num(&account_row(&s2, "alice"), &["available_atoms"]),
        20_000 * ATOMS + 2_000_000_000
    );
    assert_eq!(
        num(&account_row(&s2, "issuer"), &["available_atoms"]),
        1_000_000 * ATOMS
    );
    assert_eq!(path(&s2, &["custody", "balanced"]), "true");

    // Only the property issuer can grant.
    let before = state(id).as_json().to_owned();
    assert_eq!(
        code(&format!(
            r#"{{"op":"pg_block","session":"{id}","height":120002,"ops":[{{"t":"grant","by":"alice","to":"alice","atoms":1}}]}}"#
        )),
        "NotIssuer"
    );
    assert_eq!(state(id).as_json(), before);
    // The state verifier and ledger conservation both pass.
    ok(&format!(
        r#"{{"op":"pg_state","session":"{id}","verify":true}}"#
    ));
    ok(&format!(r#"{{"op":"pg_drop","session":"{id}"}}"#));
}

#[test]
fn market_maker_quotes_and_taker_fill_give_opposite_positions() {
    let id = "fill";
    new_session(id);
    block(
        id,
        120_000,
        &format!(
            r#"{{"t":"oracle","price_1e4":1000000000}},{}"#,
            quote_ladder()
        ),
    );
    let s = state(id);
    assert_eq!(num(&s, &["book", "bid_count"]), 2);
    assert_eq!(num(&s, &["book", "ask_count"]), 2);
    assert_eq!(text(&s, &["net_contracts"]), "0");

    // Alice lifts the best ask for 2 contracts, Bob hits the best bid for 1.
    let b = block(
        id,
        120_001,
        r#"{"t":"order","account":"alice","side":"buy","qty":"2","market":true},
           {"t":"order","account":"bob","side":"sell","qty":"1","market":true}"#,
    );
    assert_eq!(num(&b, &["fill_count"]), 2);
    let s = state(id);
    let alice = position(&s, "alice").unwrap();
    let bob = position(&s, "bob").unwrap();
    let mm = position(&s, "mm").unwrap();
    assert_eq!(text(&alice, &["contracts"]), "2");
    assert_eq!(text(&bob, &["contracts"]), "-1");
    assert_eq!(text(&mm, &["contracts"]), "-1");
    assert_eq!(text(&s, &["net_contracts"]), "0");
    assert_eq!(text(&s, &["open_interest"]), "2");
    // Taker paid the maker's resting price.
    assert_eq!(text(&alice, &["entry"]), "100050");
    assert_eq!(text(&bob, &["entry"]), "99950");
    assert_eq!(path(&s, &["custody", "balanced"]), "true");
    ok(&format!(
        r#"{{"op":"pg_state","session":"{id}","verify":true}}"#
    ));
}

#[test]
fn oracle_move_changes_unrealized_pnl() {
    let id = "pnl";
    new_session_with(id, true);
    block(
        id,
        120_000,
        &format!(
            r#"{{"t":"oracle","price_1e4":1000000000}},{}"#,
            quote_ladder()
        ),
    );
    block(
        id,
        120_001,
        r#"{"t":"order","account":"alice","side":"buy","qty":"1","market":true}"#,
    );
    let flat = state(id);
    let alice = position(&flat, "alice").unwrap();
    // Entry 100050, mark 100000, 0.01 BTC per contract: (100000 - 100050) * 1 * 0.01.
    assert_eq!(text(&alice, &["unrealized_pnl"]), "-0.5");

    block(id, 120_002, r#"{"t":"oracle","price_1e4":1020000000}"#);
    let up = state(id);
    let alice_up = position(&up, "alice").unwrap();
    assert_eq!(text(&alice_up, &["unrealized_pnl"]), "19.5");
    let mm_up = position(&up, "mm").unwrap();
    assert_eq!(text(&mm_up, &["unrealized_pnl"]), "-19.5");

    block(id, 120_003, r#"{"t":"oracle","price_1e4":980000000}"#);
    let down = state(id);
    assert_eq!(
        text(&position(&down, "alice").unwrap(), &["unrealized_pnl"]),
        "-20.5"
    );
    assert_ne!(
        text(&alice_up, &["unrealized_pnl"]),
        text(&position(&down, "alice").unwrap(), &["unrealized_pnl"])
    );
    // The marks settle into tUSD: the winner's equity rose, the loser's fell, supply is fixed.
    let equity = |s: &RpcValue, who: &str| num(&account_row(s, who), &["equity_atoms"]);
    assert!(equity(&up, "alice") > equity(&flat, "alice"));
    assert!(equity(&down, "alice") < equity(&up, "alice"));
    assert_eq!(path(&down, &["custody", "balanced"]), "true");
    assert_eq!(text(&down, &["net_contracts"]), "0");
}

#[test]
fn rejected_block_leaves_state_unchanged() {
    let id = "atomic";
    new_session(id);
    block(
        id,
        120_000,
        &format!(
            r#"{{"t":"oracle","price_1e4":1000000000}},{}"#,
            quote_ladder()
        ),
    );
    let before = state(id).as_json().to_owned();
    // A valid grant followed by an overspend: the whole block is rejected, including the grant.
    let overspend = format!(
        r#"{{"op":"pg_block","session":"{id}","height":120001,"ops":[
            {{"t":"grant","to":"alice","atoms":1000}},
            {{"t":"send","from":"alice","to":"bob","atoms":{}}}]}}"#,
        50_000 * ATOMS
    );
    assert_eq!(code(&overspend), "InsufficientFunds");
    assert_eq!(state(id).as_json(), before);

    // An order the account cannot fund is rejected the same way.
    let unfunded = r#"{"op":"pg_block","session":"atomic","height":120001,"ops":[
        {"t":"order","account":"alice","side":"buy","qty":"100000","price":"100050"}]}"#;
    assert_eq!(code(unfunded), "InsufficientFunds");
    assert_eq!(state(id).as_json(), before);

    // A zero quantity is rejected too.
    let zero_qty = r#"{"op":"pg_block","session":"atomic","height":120001,"ops":[
        {"t":"order","account":"alice","side":"buy","qty":"0","price":"100000"}]}"#;
    assert_eq!(code(zero_qty), "OrderRejected");
    assert_eq!(state(id).as_json(), before);

    // The same height still applies afterwards: nothing was consumed.
    block(id, 120_001, r#"{"t":"grant","to":"alice","atoms":1000}"#);
    assert_eq!(num(&state(id), &["next_height"]), 120_002);
}

#[test]
fn unknown_session_and_malformed_input_give_typed_errors() {
    assert_eq!(
        code(r#"{"op":"pg_state","session":"nope"}"#),
        "UnknownSession"
    );
    assert_eq!(
        code(r#"{"op":"pg_block","session":"nope","height":1,"ops":[]}"#),
        "UnknownSession"
    );
    assert_eq!(
        code(r#"{"op":"pg_drop","session":"nope"}"#),
        "UnknownSession"
    );
    assert_eq!(code(r#"{"op":"pg_nope"}"#), "UnknownOp");
    assert_eq!(code(r#"{"op":"pg_state"}"#), "BadRequest");
    assert_eq!(code(r#"{"op":"pg_new","anchor_height":1}"#), "BadRequest");
    assert_eq!(code(r#"{"op":"pg_new","leverage":0}"#), "BadRequest");
    assert_eq!(
        code(r#"{"op":"pg_new","accounts":["a","a"]}"#),
        "BadRequest"
    );

    let id = "typed";
    new_session(id);
    assert_eq!(
        code(&format!(
            r#"{{"op":"pg_new","id":"{id}","anchor_height":10}}"#
        )),
        "SessionExists"
    );
    let block = |height: &str, ops: &str| {
        code(&format!(
            r#"{{"op":"pg_block","session":"{id}","height":{height},"ops":[{ops}]}}"#
        ))
    };
    assert_eq!(block("120001", ""), "HeightMismatch");
    assert_eq!(block("120000", r#"{"t":"teleport"}"#), "BadRequest");
    assert_eq!(block("120000", r#"{"x":1}"#), "BadRequest");
    assert_eq!(
        block(
            "120000",
            r#"{"t":"order","account":"alice","side":"up","qty":"1","price":"1"}"#
        ),
        "BadRequest"
    );
    assert_eq!(
        block(
            "120000",
            r#"{"t":"order","account":"alice","side":"buy","qty":"x","price":"1"}"#
        ),
        "BadRequest"
    );
    assert_eq!(
        block("120000", r#"{"t":"grant","to":"~econ:x","atoms":1}"#),
        "BadRequest"
    );
    assert_eq!(
        block(
            "120000",
            r#"{"t":"grant","to":"alice","atoms":9007199254740993}"#
        ),
        "AmountTooLarge"
    );
    assert_eq!(
        block("120000", r#"{"t":"grant","to":"alice","atoms":0}"#),
        "ZeroAmount"
    );
    assert_eq!(
        block("120000", r#"{"t":"issue","property":1,"atoms":5}"#),
        "BadRequest"
    );
    assert_eq!(
        code(&format!(
            r#"{{"op":"pg_block","session":"{id}","height":120000,"ops":"no"}}"#
        )),
        "BadRequest"
    );
    ok(&format!(r#"{{"op":"pg_drop","session":"{id}"}}"#));
    assert_eq!(
        code(&format!(r#"{{"op":"pg_state","session":"{id}"}}"#)),
        "UnknownSession"
    );
}

#[test]
fn extra_property_and_cancel_by_id() {
    let id = "extra";
    new_session(id);
    let b = block(
        id,
        120_000,
        r#"{"t":"oracle","price_1e4":1000000000},{"t":"issue","property":9,"kind":"fixed","atoms":777},
            {"t":"order","account":"mm","side":"buy","qty":"1","price":"99950","post":true}"#,
    );
    let ops = b.field("ops").unwrap().unwrap().as_array().unwrap();
    let order_id = num(&ops[2], &["id"]);
    let s = state(id);
    assert_eq!(num(&s, &["book", "bid_count"]), 1);
    let props = s.field("properties").unwrap().unwrap().as_array().unwrap();
    assert_eq!(props.len(), 2);
    block(
        id,
        120_001,
        &format!(r#"{{"t":"cancel","account":"mm","order_id":{order_id}}}"#),
    );
    let s = state(id);
    assert_eq!(num(&s, &["book", "bid_count"]), 0);
    assert_eq!(num(&account_row(&s, "mm"), &["reserved_atoms"]), 0);
    assert_eq!(path(&s, &["custody", "balanced"]), "true");
}
