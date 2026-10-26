// Byte/node/depth-bounded raw JSON parser for large Core responses.
struct RpcJsonParser<'a> {
    text: &'a str,
    at: usize,
    nodes: usize,
}
impl<'a> RpcJsonParser<'a> {
    fn space(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.at)
            .is_some_and(|b| b" \r\n\t".contains(b))
        {
            self.at += 1;
        }
    }
    fn byte(&mut self, byte: u8) -> Result<()> {
        if self.text.as_bytes().get(self.at) != Some(&byte) {
            return Err(RpcError::InvalidJson);
        }
        self.at += 1;
        Ok(())
    }
    fn string(&mut self) -> Result<()> {
        self.byte(b'"')?;
        loop {
            let b = *self
                .text
                .as_bytes()
                .get(self.at)
                .ok_or(RpcError::InvalidJson)?;
            self.at += 1;
            match b {
                b'"' => return Ok(()),
                0..=0x1f => return Err(RpcError::InvalidJson),
                b'\\' => {
                    let esc = *self
                        .text
                        .as_bytes()
                        .get(self.at)
                        .ok_or(RpcError::InvalidJson)?;
                    self.at += 1;
                    match esc {
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {}
                        b'u' => {
                            let n = self.hex4()?;
                            if (0xd800..=0xdbff).contains(&n) {
                                self.byte(b'\\')?;
                                self.byte(b'u')?;
                                if !(0xdc00..=0xdfff).contains(&self.hex4()?) {
                                    return Err(RpcError::InvalidJson);
                                }
                            } else if (0xdc00..=0xdfff).contains(&n) {
                                return Err(RpcError::InvalidJson);
                            }
                        }
                        _ => return Err(RpcError::InvalidJson),
                    }
                }
                _ => {}
            }
        }
    }
    fn hex4(&mut self) -> Result<u16> {
        let bytes = self
            .text
            .as_bytes()
            .get(self.at..self.at + 4)
            .ok_or(RpcError::InvalidJson)?;
        if !bytes.iter().all(u8::is_ascii_hexdigit) {
            return Err(RpcError::InvalidJson);
        }
        self.at += 4;
        u16::from_str_radix(
            std::str::from_utf8(bytes).map_err(|_| RpcError::InvalidJson)?,
            16,
        )
        .map_err(|_| RpcError::InvalidJson)
    }
    fn value(&mut self, depth: usize) -> Result<()> {
        if depth > MAX_RPC_DEPTH {
            return Err(RpcError::Capacity);
        }
        self.nodes += 1;
        if self.nodes > MAX_RPC_NODES {
            return Err(RpcError::Capacity);
        }
        self.space();
        match self.text.as_bytes().get(self.at).copied() {
            Some(b'"') => self.string(),
            Some(b'{') => {
                self.at += 1;
                self.space();
                if self.text.as_bytes().get(self.at) == Some(&b'}') {
                    self.at += 1;
                    return Ok(());
                }
                let mut keys = std::collections::BTreeSet::new();
                loop {
                    let start = self.at;
                    self.string()?;
                    if !keys.insert(decode_json_string(&self.text[start..self.at])?) {
                        return Err(RpcError::InvalidJson);
                    }
                    self.space();
                    self.byte(b':')?;
                    self.value(depth + 1)?;
                    self.space();
                    match self.text.as_bytes().get(self.at) {
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(());
                        }
                        Some(b',') => {
                            self.at += 1;
                            self.space();
                        }
                        _ => return Err(RpcError::InvalidJson),
                    }
                }
            }
            Some(b'[') => {
                self.at += 1;
                self.space();
                if self.text.as_bytes().get(self.at) == Some(&b']') {
                    self.at += 1;
                    return Ok(());
                }
                loop {
                    self.value(depth + 1)?;
                    self.space();
                    match self.text.as_bytes().get(self.at) {
                        Some(b']') => {
                            self.at += 1;
                            return Ok(());
                        }
                        Some(b',') => {
                            self.at += 1;
                        }
                        _ => return Err(RpcError::InvalidJson),
                    }
                }
            }
            Some(b't') => self.literal("true"),
            Some(b'f') => self.literal("false"),
            Some(b'n') => self.literal("null"),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(RpcError::InvalidJson),
        }
    }
    fn literal(&mut self, literal: &str) -> Result<()> {
        if !self.text[self.at..].starts_with(literal) {
            return Err(RpcError::InvalidJson);
        }
        self.at += literal.len();
        Ok(())
    }
    fn digits(&mut self) -> Result<()> {
        let start = self.at;
        while self
            .text
            .as_bytes()
            .get(self.at)
            .is_some_and(u8::is_ascii_digit)
        {
            self.at += 1;
        }
        if self.at == start {
            Err(RpcError::InvalidJson)
        } else {
            Ok(())
        }
    }
    fn number(&mut self) -> Result<()> {
        if self.text.as_bytes().get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        if self.text.as_bytes().get(self.at) == Some(&b'0') {
            self.at += 1;
        } else {
            self.digits()?;
        }
        if self.text.as_bytes().get(self.at) == Some(&b'.') {
            self.at += 1;
            self.digits()?;
        }
        if matches!(self.text.as_bytes().get(self.at), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.text.as_bytes().get(self.at), Some(b'+' | b'-')) {
                self.at += 1;
            }
            self.digits()?;
        }
        Ok(())
    }
}
fn validate_json(text: &str) -> Result<()> {
    if text.len() > MAX_RPC_BODY {
        return Err(RpcError::Capacity);
    }
    let mut parser = RpcJsonParser {
        text,
        at: 0,
        nodes: 0,
    };
    parser.value(0)?;
    parser.space();
    if parser.at != text.len() {
        return Err(RpcError::InvalidJson);
    }
    Ok(())
}
fn json_fields(text: &str) -> Result<Vec<(String, &str)>> {
    validate_json(text)?;
    let mut parser = RpcJsonParser {
        text,
        at: 0,
        nodes: 0,
    };
    parser.space();
    parser.byte(b'{')?;
    parser.space();
    let mut fields = Vec::new();
    if text.as_bytes().get(parser.at) == Some(&b'}') {
        return Ok(fields);
    }
    loop {
        let start = parser.at;
        parser.string()?;
        let key = decode_json_string(&text[start..parser.at])?;
        parser.space();
        parser.byte(b':')?;
        parser.space();
        let start = parser.at;
        parser.value(1)?;
        fields.push((key, &text[start..parser.at]));
        parser.space();
        if text.as_bytes().get(parser.at) == Some(&b'}') {
            break;
        }
        parser.byte(b',')?;
        parser.space();
    }
    Ok(fields)
}
fn json_string(text: &str) -> Result<String> {
    validate_json(text)?;
    decode_json_string(text.trim())
}
fn json_array(text: &str) -> Result<Vec<&str>> {
    validate_json(text)?;
    let mut parser = RpcJsonParser {
        text,
        at: 0,
        nodes: 0,
    };
    parser.space();
    parser.byte(b'[')?;
    parser.space();
    let mut items = Vec::new();
    if text.as_bytes().get(parser.at) == Some(&b']') {
        return Ok(items);
    }
    loop {
        parser.space();
        let start = parser.at;
        parser.value(1)?;
        items.push(&text[start..parser.at]);
        parser.space();
        if text.as_bytes().get(parser.at) == Some(&b']') {
            return Ok(items);
        }
        parser.byte(b',')?;
    }
}
fn decode_json_string(text: &str) -> Result<String> {
    let mut chars = text.chars();
    if chars.next() != Some('"') || !text.ends_with('"') || text.len() < 2 {
        return Err(RpcError::InvalidJson);
    }
    let mut chars = text[1..text.len() - 1].chars();
    let mut out = String::new();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let ch = match chars.next().ok_or(RpcError::InvalidJson)? {
            '"' => '"',
            '\\' => '\\',
            '/' => '/',
            'b' => '\u{8}',
            'f' => '\u{c}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'u' => {
                let mut n = 0u32;
                for _ in 0..4 {
                    n = n * 16
                        + chars
                            .next()
                            .and_then(|c| c.to_digit(16))
                            .ok_or(RpcError::InvalidJson)?;
                }
                if (0xd800..=0xdbff).contains(&n) {
                    if chars.next() != Some('\\') || chars.next() != Some('u') {
                        return Err(RpcError::InvalidJson);
                    }
                    let mut low = 0u32;
                    for _ in 0..4 {
                        low = low * 16
                            + chars
                                .next()
                                .and_then(|c| c.to_digit(16))
                                .ok_or(RpcError::InvalidJson)?;
                    }
                    if !(0xdc00..=0xdfff).contains(&low) {
                        return Err(RpcError::InvalidJson);
                    }
                    n = 0x10000 + ((n - 0xd800) << 10) + (low - 0xdc00);
                }
                char::from_u32(n).ok_or(RpcError::InvalidJson)?
            }
            _ => return Err(RpcError::InvalidJson),
        };
        out.push(ch);
    }
    Ok(out)
}
fn quote_json(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch < '\u{20}' => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}
