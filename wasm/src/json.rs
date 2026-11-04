//! Request reading and response writing on top of `tradelayer_core::client::RpcValue`, so the
//! binding keeps the core's zero-dependency rule.

use tradelayer_core::client::RpcValue;

/// A typed failure returned as `{"ok":false,"code":...,"message":...}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpError {
    pub code: String,
    pub message: String,
}

impl OpError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
    pub fn bad(message: &str) -> Self {
        Self::new("BadRequest", message)
    }
    /// Maps any core error to its `Debug` variant name as the code.
    pub fn from_debug(error: impl core::fmt::Debug) -> Self {
        let text = format!("{error:?}");
        let code = text
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("Error")
            .to_owned();
        Self {
            code,
            message: text,
        }
    }
}

pub type Res<T> = Result<T, OpError>;

pub fn field(req: &RpcValue, name: &str) -> Res<RpcValue> {
    req.field(name)
        .map_err(|_| OpError::bad("request is not an object"))?
        .ok_or_else(|| OpError::bad(&format!("missing field `{name}`")))
}

pub fn str_field(req: &RpcValue, name: &str) -> Res<String> {
    field(req, name)?
        .as_string()
        .map_err(|_| OpError::bad(&format!("field `{name}` is not a string")))
}

pub fn u64_field(req: &RpcValue, name: &str) -> Res<u64> {
    field(req, name)?
        .as_u64()
        .map_err(|_| OpError::bad(&format!("field `{name}` is not an unsigned integer")))
}

/// JSON string literal for `text`.
pub fn quote(text: &str) -> String {
    RpcValue::string(text).map_or_else(|_| "\"\"".to_owned(), |v| v.as_json().to_owned())
}

/// Builds one JSON object from already-encoded values.
#[derive(Default)]
pub struct Obj(Vec<(String, String)>);

impl Obj {
    pub fn new() -> Self {
        Self::default()
    }
    /// Adds a raw JSON value.
    pub fn raw(mut self, key: &str, json: impl Into<String>) -> Self {
        self.0.push((key.to_owned(), json.into()));
        self
    }
    pub fn str(self, key: &str, value: &str) -> Self {
        let q = quote(value);
        self.raw(key, q)
    }
    pub fn num(self, key: &str, value: impl core::fmt::Display) -> Self {
        self.raw(key, value.to_string())
    }
    pub fn bool(self, key: &str, value: bool) -> Self {
        self.raw(key, if value { "true" } else { "false" })
    }
    /// Puts `"ok":true` first.
    pub fn with_ok(mut self) -> Self {
        self.0.insert(0, ("ok".to_owned(), "true".to_owned()));
        self
    }
    pub fn build(self) -> String {
        let body: Vec<String> = self
            .0
            .into_iter()
            .map(|(k, v)| format!("{}:{v}", quote(&k)))
            .collect();
        format!("{{{}}}", body.join(","))
    }
}

/// JSON array from already-encoded values.
pub fn array(items: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", items.into_iter().collect::<Vec<_>>().join(","))
}
