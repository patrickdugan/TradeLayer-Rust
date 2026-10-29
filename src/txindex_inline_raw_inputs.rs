// Exact declared raw inputs only; no source outcomes or financial state are imported.
fn raw_block() -> V {
    V::Object(vec![
        ("hash".into(), s("inline-block11")),
        ("previousblockhash".into(), s("inline-block10")),
        ("height".into(), V::Number(0x4026000000000000)),
        (
            "tx".into(),
            V::Array(vec![s(
                "f5c34838011b98ebab8aeee92d0f2421bc7248d5a2d6127f914dfc823a92d577",
            )]),
        ),
    ])
}
fn raw_documents() -> V {
    V::Object(vec![("id".into(),s("f5c34838011b98ebab8aeee92d0f2421bc7248d5a2d6127f914dfc823a92d577")),("raw".into(),s("010000000122222222222222222222222222222222222222222222222222222222222222220000000000ffffffff020000000000000000186a16746c30302c736572766963652d6c6976652d636f646500e1f5050000000017a914d9feb2d55d2c022d4fc463ab54dcbdd75f7b0ebc8700000000")),("document".into(),V::Object(vec![("txid".into(),s("f5c34838011b98ebab8aeee92d0f2421bc7248d5a2d6127f914dfc823a92d577")),("vin".into(),V::Array(vec![V::Object(vec![("txid".into(),s("2222222222222222222222222222222222222222222222222222222222222222")),("vout".into(),V::Number(0x0000000000000000)),("scriptSig".into(),V::Object(vec![("asm".into(),s("")),("hex".into(),s(""))])),("sequence".into(),V::Number(0x41efffffffe00000))])])),("vout".into(),V::Array(vec![V::Object(vec![("n".into(),V::Number(0x0000000000000000)),("value".into(),V::Number(0x0000000000000000)),("scriptPubKey".into(),V::Object(vec![("type".into(),s("nulldata")),("hex".into(),s("6a16746c30302c736572766963652d6c6976652d636f6465"))]))]),V::Object(vec![("n".into(),V::Number(0x3ff0000000000000)),("value".into(),V::Number(0x3ff0000000000000)),("scriptPubKey".into(),V::Object(vec![("type".into(),s("scripthash")),("hex".into(),s("a914d9feb2d55d2c022d4fc463ab54dcbdd75f7b0ebc87")),("addresses".into(),V::Array(vec![s("MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB")]))]))])]))])),("parent".into(),V::Object(vec![("txid".into(),s("2222222222222222222222222222222222222222222222222222222222222222")),("vout".into(),V::Array(vec![V::Object(vec![("n".into(),V::Number(0x0000000000000000)),("value".into(),V::Number(0x4000000000000000)),("scriptPubKey".into(),V::Object(vec![("type".into(),s("scripthash")),("hex".into(),s("a914d9feb2d55d2c022d4fc463ab54dcbdd75f7b0ebc87")),("addresses".into(),V::Array(vec![s("MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB")]))]))])]))]))])
}
fn raw_native_rows() -> V {
    V::Array(vec![V::Object(vec![
        ("_id".into(), s("MaxHeight")),
        ("value".into(), V::Number(0x4024000000000000)),
    ])])
}
