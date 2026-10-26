//! Shared bounded types for the local service, its adapters and HTTP clients.
use crate::bootstrap::BootstrapControl;
use crate::client::RpcValue;
use std::collections::BTreeMap;

pub const MAX_SERVICE_PATH: usize = 8192;
pub const MAX_SERVICE_REQUEST: usize = 1024 * 1024;
pub const MAX_SERVICE_RESPONSE: usize = 16 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceMethod {
    Get,
    Post,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceError {
    InvalidRequest,
    NotFound,
    Unavailable(&'static str),
    Backend(&'static str),
    Capacity,
    Cancelled,
    Timeout,
    Transport(std::io::ErrorKind),
    InvalidHttp,
}
impl core::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ServiceError {}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRequest {
    pub method: ServiceMethod,
    /// Decoded path. Transport decodes each path segment exactly once.
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub body: RpcValue,
    /// Distinguishes an absent HTTP body from an explicit JSON null/object.
    pub body_present: bool,
}
impl ServiceRequest {
    pub fn new(
        method: ServiceMethod,
        path: impl Into<String>,
        query: BTreeMap<String, String>,
        body: RpcValue,
    ) -> Result<Self, ServiceError> {
        let request = Self {
            method,
            path: path.into(),
            query,
            body,
            body_present: true,
        };
        request.validate()?;
        Ok(request)
    }
    pub fn without_body(mut self) -> Self {
        self.body = RpcValue::null();
        self.body_present = false;
        self
    }
    pub fn validate(&self) -> Result<(), ServiceError> {
        if !self.path.starts_with('/')
            || self.path.len() > MAX_SERVICE_PATH
            || self.path.bytes().any(|b| b.is_ascii_control())
            || self.query.len() > 128
        {
            return Err(ServiceError::InvalidRequest);
        }
        let bytes = self
            .query
            .iter()
            .try_fold(self.path.len(), |total, (key, value)| {
                total
                    .checked_add(key.len())
                    .and_then(|n| n.checked_add(value.len()))
                    .ok_or(ServiceError::Capacity)
            })?;
        if bytes > MAX_SERVICE_PATH || self.body.as_json().len() > MAX_SERVICE_REQUEST {
            return Err(ServiceError::Capacity);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceContentType {
    Json,
    Text,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceResponse {
    pub status: u16,
    pub content_type: ServiceContentType,
    pub body: String,
}
impl ServiceResponse {
    pub fn json(status: u16, body: RpcValue) -> Result<Self, ServiceError> {
        Self::new(status, ServiceContentType::Json, body.as_json().to_owned())
    }
    pub fn text(status: u16, body: impl Into<String>) -> Result<Self, ServiceError> {
        Self::new(status, ServiceContentType::Text, body.into())
    }
    pub fn new(
        status: u16,
        content_type: ServiceContentType,
        body: String,
    ) -> Result<Self, ServiceError> {
        if !(200..=599).contains(&status) {
            return Err(ServiceError::InvalidRequest);
        }
        if body.len() > MAX_SERVICE_RESPONSE {
            return Err(ServiceError::Capacity);
        }
        Ok(Self {
            status,
            content_type,
            body,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceCall {
    pub target: &'static str,
    pub arguments: Vec<RpcValue>,
}
pub trait ServiceBackend {
    fn call(&mut self, call: ServiceCall) -> Result<RpcValue, ServiceError>;
}
pub trait ServiceTransport {
    fn send(
        &mut self,
        request: &ServiceRequest,
        control: &BootstrapControl,
    ) -> Result<ServiceResponse, ServiceError>;
}
