//! Diagnostic framing has its own parser depth; decoded values retain the
//! shared SourceValue node/depth/byte limits.
use super::{CatalogError, Result, V};
use crate::issuance_intent::JsonValue;

pub(super) fn decode(text: &str) -> Result<V> {
    if text.len() > crate::validity_source::MAX_BYTES * 4 {
        return Err(CatalogError::Capacity);
    }
    let mut parser = Parser {
        text,
        at: 0,
        nodes: 0,
    };
    let value = parser.value(0)?;
    parser.space();
    if parser.at != text.len() {
        return Err(CatalogError::InvalidInput);
    }
    value.validate()?;
    Ok(value)
}
struct Parser<'a> {
    text: &'a str,
    at: usize,
    nodes: usize,
}
impl Parser<'_> {
    fn space(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.at)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.at += 1
        }
    }
    fn take(&mut self, byte: u8) -> Result<()> {
        self.space();
        if self.text.as_bytes().get(self.at) != Some(&byte) {
            return Err(CatalogError::InvalidInput);
        }
        self.at += 1;
        Ok(())
    }
    fn next(&mut self, byte: u8) -> bool {
        self.space();
        self.text.as_bytes().get(self.at) == Some(&byte)
    }
    fn string(&mut self) -> Result<String> {
        self.space();
        let start = self.at;
        self.take(b'"')?;
        loop {
            match self.text.as_bytes().get(self.at) {
                Some(b'"') => {
                    self.at += 1;
                    break;
                }
                Some(b'\\') => self.at += 2,
                Some(_) => self.at += 1,
                None => return Err(CatalogError::InvalidInput),
            }
        }
        match JsonValue::parse(
            self.text
                .get(start..self.at)
                .ok_or(CatalogError::InvalidInput)?,
        )
        .map_err(|_| CatalogError::InvalidInput)?
        {
            JsonValue::String(s) => Ok(s),
            _ => Err(CatalogError::InvalidInput),
        }
    }
    fn value(&mut self, depth: usize) -> Result<V> {
        self.nodes += 1;
        if depth > crate::validity_source::MAX_DEPTH
            || self.nodes > crate::validity_source::MAX_NODES
        {
            return Err(CatalogError::Capacity);
        }
        self.take(b'[')?;
        let tag = self.string()?;
        let value = match tag.as_str() {
            "undefined" => V::Undefined,
            "null" => V::Null,
            "promise" => V::Promise,
            "string" | "bignumber" => {
                self.take(b',')?;
                let s = self.string()?;
                if tag == "string" {
                    V::String(s)
                } else {
                    V::BigNumber(s)
                }
            }
            "number" => {
                self.take(b',')?;
                let s = self.string()?;
                V::number(match s.as_str() {
                    "NaN" => f64::NAN,
                    "Infinity" => f64::INFINITY,
                    "-Infinity" => f64::NEG_INFINITY,
                    _ => {
                        if s.len() != 16
                            || !s
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                        {
                            return Err(CatalogError::InvalidInput);
                        }
                        f64::from_bits(
                            u64::from_str_radix(&s, 16).map_err(|_| CatalogError::InvalidInput)?,
                        )
                    }
                })
            }
            "boolean" => {
                self.take(b',')?;
                self.space();
                if self.text[self.at..].starts_with("true") {
                    self.at += 4;
                    V::Bool(true)
                } else if self.text[self.at..].starts_with("false") {
                    self.at += 5;
                    V::Bool(false)
                } else {
                    return Err(CatalogError::InvalidInput);
                }
            }
            "array" => {
                self.take(b',')?;
                self.take(b'[')?;
                let mut values = Vec::new();
                if !self.next(b']') {
                    loop {
                        values.push(self.value(depth + 1)?);
                        if !self.next(b',') {
                            break;
                        }
                        self.take(b',')?
                    }
                }
                self.take(b']')?;
                V::Array(values)
            }
            "object" => {
                self.take(b',')?;
                self.take(b'[')?;
                let mut fields = Vec::new();
                if !self.next(b']') {
                    loop {
                        self.take(b'[')?;
                        let key = self.string()?;
                        self.take(b',')?;
                        let value = self.value(depth + 1)?;
                        self.take(b']')?;
                        fields.push((key, value));
                        if !self.next(b',') {
                            break;
                        }
                        self.take(b',')?
                    }
                }
                self.take(b']')?;
                V::Object(fields)
            }
            _ => return Err(CatalogError::InvalidInput),
        };
        self.take(b']')?;
        Ok(value)
    }
}
