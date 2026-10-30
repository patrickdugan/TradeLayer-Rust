// Read-only test transport. Borrow complete JSON slices rather than forcing a
// large source observation through production's bounded SourceValue decoder.
// Every selected source value and effect is decoded; nothing is truncated.
fn json_span(bytes: &[u8], start: usize) -> usize {
    match bytes[start] {
        b'"' => {
            let mut at = start + 1;
            loop {
                match bytes[at] {
                    b'"' => return at + 1,
                    b'\\' => at += 2,
                    _ => at += 1,
                }
            }
        }
        b'[' | b'{' => {
            let close = if bytes[start] == b'[' { b']' } else { b'}' };
            let mut at = start + 1;
            loop {
                match bytes[at] {
                    byte if byte == close => return at + 1,
                    b'"' | b'[' | b'{' => at = json_span(bytes, at),
                    b']' | b'}' => panic!("mismatched frozen JSON container"),
                    _ => at += 1,
                }
            }
        }
        _ => {
            let mut at = start;
            while at < bytes.len()
                && !matches!(bytes[at], b',' | b']' | b'}' | b' ' | b'\t' | b'\r' | b'\n')
            {
                at += 1;
            }
            assert!(at > start);
            at
        }
    }
}
fn json_parts(raw: &str) -> Vec<&str> {
    let raw = raw.trim();
    assert!(raw.starts_with('[') && raw.ends_with(']'));
    let mut at = 1;
    let mut output = vec![];
    while at < raw.len() - 1 {
        while raw.as_bytes()[at].is_ascii_whitespace() {
            at += 1;
        }
        if raw.as_bytes()[at] == b']' {
            break;
        }
        let end = json_span(raw.as_bytes(), at);
        output.push(&raw[at..end]);
        at = end;
        while raw.as_bytes()[at].is_ascii_whitespace() {
            at += 1;
        }
        if raw.as_bytes()[at] == b']' {
            break;
        }
        assert_eq!(raw.as_bytes()[at], b',');
        at += 1;
    }
    output
}
fn json_string(raw: &str) -> String {
    match crate::issuance_intent::JsonValue::parse(raw).unwrap() {
        crate::issuance_intent::JsonValue::String(value) => value,
        _ => panic!("frozen JSON string required"),
    }
}
fn raw_get<'a>(raw: &'a str, name: &str) -> &'a str {
    let raw = raw.trim();
    assert!(raw.starts_with('{') && raw.ends_with('}'));
    let mut at = 1;
    while at < raw.len() - 1 {
        while raw.as_bytes()[at].is_ascii_whitespace() {
            at += 1;
        }
        if raw.as_bytes()[at] == b'}' {
            break;
        }
        let end = json_span(raw.as_bytes(), at);
        let key = json_string(&raw[at..end]);
        at = end;
        while raw.as_bytes()[at].is_ascii_whitespace() {
            at += 1;
        }
        assert_eq!(raw.as_bytes()[at], b':');
        at += 1;
        while raw.as_bytes()[at].is_ascii_whitespace() {
            at += 1;
        }
        let end = json_span(raw.as_bytes(), at);
        if key == name {
            return &raw[at..end];
        }
        at = end;
        while raw.as_bytes()[at].is_ascii_whitespace() {
            at += 1;
        }
        if raw.as_bytes()[at] == b'}' {
            break;
        }
        assert_eq!(raw.as_bytes()[at], b',');
        at += 1;
    }
    panic!("missing frozen field {name}")
}

fn untag(raw: &str) -> V {
    let parts = json_parts(raw);
    match json_string(parts[0]).as_str() {
        "object" => V::Object(
            json_parts(parts[1])
                .into_iter()
                .map(|pair| {
                    let pair = json_parts(pair);
                    (json_string(pair[0]), untag(pair[1]))
                })
                .collect(),
        ),
        "array" => V::Array(json_parts(parts[1]).into_iter().map(untag).collect()),
        "undefined" => V::Undefined,
        "null" => V::Null,
        "promise" => V::Promise,
        "string" => V::String(json_string(parts[1])),
        "boolean" => {
            assert!(matches!(parts[1], "true" | "false"));
            V::Bool(parts[1] == "true")
        }
        "number" => {
            let bits = json_string(parts[1]);
            match bits.as_str() {
                "NaN" => V::number(f64::NAN),
                "Infinity" => V::number(f64::INFINITY),
                "-Infinity" => V::number(f64::NEG_INFINITY),
                "-0" => V::number(-0.),
                _ => V::Number(u64::from_str_radix(&bits, 16).unwrap()),
            }
        }
        "regexp" => obj(&[
            ("$regexp", V::String(json_string(parts[1]))),
            ("$flags", V::String(json_string(parts[2]))),
        ]),
        tag => panic!("unmapped selected source value {tag}"),
    }
}
