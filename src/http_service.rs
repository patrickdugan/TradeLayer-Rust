//! Bounded one-request HTTP connections for the explicitly owned local API.
use crate::bootstrap::{BootstrapControl, StopReason};
use crate::client::{RpcError, RpcValue};
use crate::service::{
    ServiceContentType, ServiceError, ServiceMethod, ServiceRequest, ServiceResponse,
    ServiceTransport, MAX_SERVICE_PATH, MAX_SERVICE_REQUEST, MAX_SERVICE_RESPONSE,
};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

const MAX_HEADERS: usize = 16 * 1024;
type Result<T> = core::result::Result<T, ServiceError>;
fn control_check(control: &BootstrapControl) -> Result<()> {
    control.check().map_err(|reason| match reason {
        StopReason::Cancelled => ServiceError::Cancelled,
        StopReason::DeadlineExceeded => ServiceError::Timeout,
    })
}
fn io_error(error: std::io::Error) -> ServiceError {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => ServiceError::Timeout,
        kind => ServiceError::Transport(kind),
    }
}
fn rpc_error(error: RpcError) -> ServiceError {
    match error {
        RpcError::Timeout => ServiceError::Timeout,
        RpcError::Cancelled => ServiceError::Cancelled,
        RpcError::Capacity => ServiceError::Capacity,
        RpcError::Transport(kind) => ServiceError::Transport(kind),
        _ => ServiceError::InvalidHttp,
    }
}
fn timeout_valid(timeout: Duration) -> bool {
    !timeout.is_zero() && timeout <= Duration::from_secs(60)
}
fn remaining(start: Instant, timeout: Duration, control: &BootstrapControl) -> Result<Duration> {
    control_check(control)?;
    let now = Instant::now();
    let left = timeout
        .checked_sub(now.duration_since(start))
        .filter(|d| !d.is_zero())
        .ok_or(ServiceError::Timeout)?;
    match control.deadline() {
        Some(deadline) => Ok(left.min(
            deadline
                .checked_duration_since(now)
                .filter(|d| !d.is_zero())
                .ok_or(ServiceError::Timeout)?,
        )),
        None => Ok(left),
    }
}
fn write_all(
    stream: &mut TcpStream,
    mut bytes: &[u8],
    start: Instant,
    timeout: Duration,
    control: &BootstrapControl,
) -> Result<()> {
    while !bytes.is_empty() {
        stream
            .set_write_timeout(Some(remaining(start, timeout, control)?))
            .map_err(io_error)?;
        let count = stream.write(bytes).map_err(io_error)?;
        if count == 0 {
            return Err(ServiceError::Transport(std::io::ErrorKind::WriteZero));
        }
        bytes = &bytes[count..];
    }
    control_check(control)
}

/// A literal configured endpoint, one bounded connection per request, no retry.
#[derive(Debug)]
pub struct HttpServiceClient {
    endpoint: SocketAddr,
    timeout: Duration,
}
impl HttpServiceClient {
    pub fn new(endpoint: SocketAddr, timeout: Duration) -> Result<Self> {
        if endpoint.port() == 0 || !timeout_valid(timeout) {
            return Err(ServiceError::InvalidRequest);
        }
        Ok(Self { endpoint, timeout })
    }
    fn exchange(
        &self,
        request: &ServiceRequest,
        control: &BootstrapControl,
    ) -> Result<ServiceResponse> {
        control_check(control)?;
        request.validate()?;
        let path = encode_target(request)?;
        let method = match request.method {
            ServiceMethod::Get => "GET",
            ServiceMethod::Post => "POST",
        };
        let body = if request.body_present {
            request.body.as_json()
        } else {
            ""
        };
        let body = if request.method == ServiceMethod::Get {
            ""
        } else {
            body
        };
        let bytes=format!("{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",self.endpoint,body.len());
        let start = Instant::now();
        let timeout = remaining(start, self.timeout, control)?;
        let mut stream = TcpStream::connect_timeout(&self.endpoint, timeout).map_err(io_error)?;
        write_all(&mut stream, bytes.as_bytes(), start, timeout, control)?;
        let (status, body) = crate::client::read_http(&mut stream, start, timeout, Some(control))
            .map_err(rpc_error)?;
        remaining(start, timeout, control)?;
        let kind = if RpcValue::parse(&body).is_ok() {
            ServiceContentType::Json
        } else {
            ServiceContentType::Text
        };
        let response = ServiceResponse::new(status, kind, body)?;
        remaining(start, timeout, control)?;
        Ok(response)
    }
}
impl ServiceTransport for HttpServiceClient {
    fn send(
        &mut self,
        request: &ServiceRequest,
        control: &BootstrapControl,
    ) -> Result<ServiceResponse> {
        let result = self.exchange(request, control);
        control_check(control)?;
        result
    }
}

fn encode_component(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
fn encode_target(request: &ServiceRequest) -> Result<String> {
    let mut path = request
        .path
        .split('/')
        .map(encode_component)
        .collect::<Vec<_>>()
        .join("/");
    if !request.query.is_empty() {
        path.push('?');
        path.push_str(
            &request
                .query
                .iter()
                .map(|(key, value)| {
                    format!("{}={}", encode_component(key), encode_component(value))
                })
                .collect::<Vec<_>>()
                .join("&"),
        );
    }
    if path.len() > MAX_SERVICE_PATH {
        return Err(ServiceError::Capacity);
    }
    Ok(path)
}
fn decode_component(text: &str, query: bool) -> Result<String> {
    let mut decoded = Vec::with_capacity(text.len());
    let mut at = 0;
    let bytes = text.as_bytes();
    while at < bytes.len() {
        let value = if bytes[at] == b'%' {
            let pair = bytes
                .get(at + 1..at + 3)
                .ok_or(ServiceError::InvalidRequest)?;
            let digit = |byte: u8| {
                char::from(byte)
                    .to_digit(16)
                    .ok_or(ServiceError::InvalidRequest)
            };
            at += 2;
            ((digit(pair[0])? << 4) + digit(pair[1])?) as u8
        } else if query && bytes[at] == b'+' {
            b' '
        } else {
            bytes[at]
        };
        if value.is_ascii_control() || !query && matches!(value, b'/' | b'\\') {
            return Err(ServiceError::InvalidRequest);
        }
        decoded.push(value);
        at += 1;
    }
    String::from_utf8(decoded).map_err(|_| ServiceError::InvalidRequest)
}
fn decode_target(target: &str) -> Result<(String, BTreeMap<String, String>)> {
    if !target.starts_with('/')
        || target.len() > MAX_SERVICE_PATH
        || target.contains('#')
        || !target.is_ascii()
    {
        return Err(ServiceError::InvalidRequest);
    }
    let (raw, query) = target.split_once('?').unwrap_or((target, ""));
    let path = raw
        .split('/')
        .map(|part| decode_component(part, false))
        .collect::<Result<Vec<_>>>()?
        .join("/");
    let mut values = BTreeMap::new();
    if !query.is_empty() {
        for field in query.split('&') {
            if values.len() == 128 {
                return Err(ServiceError::Capacity);
            }
            let (key, value) = field.split_once('=').unwrap_or((field, ""));
            let key = decode_component(key, true)?;
            let value = decode_component(value, true)?;
            if key.is_empty() || values.insert(key, value).is_some() {
                return Err(ServiceError::InvalidRequest);
            }
        }
    }
    Ok((path, values))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServeOutcome {
    Idle,
    Responded { status: u16 },
    Stopped,
}
#[derive(Debug)]
pub struct HttpServiceServer {
    listener: TcpListener,
    timeout: Duration,
}
impl HttpServiceServer {
    /// Loopback binding keeps this unauthenticated read API local to its owner.
    pub fn bind(endpoint: SocketAddr, timeout: Duration) -> Result<Self> {
        if !endpoint.ip().is_loopback() || !timeout_valid(timeout) {
            return Err(ServiceError::InvalidRequest);
        }
        let listener = TcpListener::bind(endpoint).map_err(io_error)?;
        listener.set_nonblocking(true).map_err(io_error)?;
        Ok(Self { listener, timeout })
    }
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.listener.local_addr().map_err(io_error)
    }
    /// At most one accepted request per call; no background thread is installed.
    pub fn serve_one<F>(&self, control: &BootstrapControl, handler: &mut F) -> Result<ServeOutcome>
    where
        F: FnMut(&ServiceRequest) -> Result<ServiceResponse>,
    {
        if control.check().is_err() {
            return Ok(ServeOutcome::Stopped);
        }
        let (mut stream, peer) = match self.listener.accept() {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Ok(ServeOutcome::Idle)
            }
            Err(error) => return Err(io_error(error)),
        };
        if !peer.ip().is_loopback() {
            return Err(ServiceError::InvalidRequest);
        }
        stream.set_nonblocking(false).map_err(io_error)?;
        let start = Instant::now();
        let response = match read_request(
            &mut stream,
            self.local_addr()?,
            start,
            self.timeout,
            control,
        ) {
            Ok(request) => {
                if control.check().is_err() {
                    return Ok(ServeOutcome::Stopped);
                }
                match handler(&request) {
                    Ok(response) => response,
                    Err(error) => error_response(&error)?,
                }
            }
            Err(error) => {
                if control.check().is_err() {
                    return Ok(ServeOutcome::Stopped);
                }
                error_response(&error)?
            }
        };
        if control.check().is_err() {
            return Ok(ServeOutcome::Stopped);
        }
        let written = write_response(&mut stream, &response, start, self.timeout, control);
        if control.check().is_err() {
            return Ok(ServeOutcome::Stopped);
        }
        written?;
        Ok(ServeOutcome::Responded {
            status: response.status,
        })
    }
}
fn read_more(
    stream: &mut TcpStream,
    bytes: &mut Vec<u8>,
    start: Instant,
    timeout: Duration,
    control: &BootstrapControl,
) -> Result<()> {
    let mut buffer = [0; 4096];
    let count = loop {
        stream
            .set_read_timeout(Some(
                remaining(start, timeout, control)?.min(Duration::from_millis(100)),
            ))
            .map_err(io_error)?;
        match stream.read(&mut buffer) {
            Ok(count) => break count,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                continue
            }
            Err(error) => return Err(io_error(error)),
        }
    };
    if count == 0 {
        return Err(ServiceError::InvalidHttp);
    }
    if bytes
        .len()
        .checked_add(count)
        .is_none_or(|n| n > MAX_HEADERS + MAX_SERVICE_REQUEST)
    {
        return Err(ServiceError::Capacity);
    }
    bytes.extend_from_slice(&buffer[..count]);
    control_check(control)
}
fn read_request(
    stream: &mut TcpStream,
    endpoint: SocketAddr,
    start: Instant,
    timeout: Duration,
    control: &BootstrapControl,
) -> Result<ServiceRequest> {
    let mut bytes = Vec::new();
    let end = loop {
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break end;
        }
        if bytes.len() > MAX_HEADERS {
            return Err(ServiceError::Capacity);
        }
        read_more(stream, &mut bytes, start, timeout, control)?;
    };
    if end + 4 > MAX_HEADERS {
        return Err(ServiceError::Capacity);
    }
    let headers = std::str::from_utf8(&bytes[..end]).map_err(|_| ServiceError::InvalidHttp)?;
    if headers
        .bytes()
        .any(|b| b < 0x20 && !matches!(b, b'\r' | b'\n' | b'\t') || b == 0x7f)
    {
        return Err(ServiceError::InvalidHttp);
    }
    let mut lines = headers.split("\r\n");
    let first = lines.next().ok_or(ServiceError::InvalidHttp)?;
    let mut parts = first.split(' ');
    let method = match parts.next() {
        Some("GET") => ServiceMethod::Get,
        Some("POST") => ServiceMethod::Post,
        _ => return Err(ServiceError::InvalidRequest),
    };
    let target = parts.next().ok_or(ServiceError::InvalidHttp)?.to_owned();
    if parts.next() != Some("HTTP/1.1") || parts.next().is_some() {
        return Err(ServiceError::InvalidHttp);
    }
    let mut length = None;
    let mut host = false;
    let mut content_type = false;
    let mut encoding = false;
    for (index, line) in lines.enumerate() {
        if index >= 64 || line.starts_with([' ', '\t']) || line.contains(['\r', '\n']) {
            return Err(ServiceError::InvalidHttp);
        }
        let (key, value) = line.split_once(':').ok_or(ServiceError::InvalidHttp)?;
        if key.is_empty()
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
        {
            return Err(ServiceError::InvalidHttp);
        }
        let value = value.trim_matches([' ', '\t']);
        match key.to_ascii_lowercase().as_str() {
            "content-length" => {
                if length.is_some()
                    || value.is_empty()
                    || !value.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(ServiceError::InvalidHttp);
                }
                let count = value.parse::<usize>().map_err(|_| ServiceError::Capacity)?;
                if count > MAX_SERVICE_REQUEST {
                    return Err(ServiceError::Capacity);
                }
                length = Some(count);
            }
            "host" => {
                if host
                    || !(value == endpoint.to_string()
                        || value.eq_ignore_ascii_case(&format!("localhost:{}", endpoint.port())))
                {
                    return Err(ServiceError::InvalidHttp);
                }
                host = true;
            }
            "content-type" => {
                if content_type
                    || !value
                        .split(';')
                        .next()
                        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
                {
                    return Err(ServiceError::InvalidRequest);
                }
                content_type = true;
            }
            "content-encoding" => {
                if encoding || !value.eq_ignore_ascii_case("identity") {
                    return Err(ServiceError::InvalidRequest);
                }
                encoding = true;
            }
            "transfer-encoding" | "expect" => return Err(ServiceError::InvalidHttp),
            _ => {}
        }
    }
    if !host {
        return Err(ServiceError::InvalidHttp);
    }
    let count = length.unwrap_or(0);
    if count != 0 && !content_type {
        return Err(ServiceError::InvalidRequest);
    }
    let total = end + 4 + count;
    while bytes.len() < total {
        read_more(stream, &mut bytes, start, timeout, control)?;
    }
    if bytes.len() != total {
        return Err(ServiceError::InvalidHttp);
    }
    let body = if count == 0 {
        RpcValue::parse("{}").map_err(|_| ServiceError::InvalidRequest)?
    } else {
        let text =
            std::str::from_utf8(&bytes[end + 4..]).map_err(|_| ServiceError::InvalidRequest)?;
        if !text.trim_start().starts_with('{') {
            return Err(ServiceError::InvalidRequest);
        }
        RpcValue::parse(text).map_err(|_| ServiceError::InvalidRequest)?
    };
    let (path, query) = decode_target(&target)?;
    let mut request = ServiceRequest::new(method, path, query, body)?;
    request.body_present = count != 0;
    Ok(request)
}
fn error_response(error: &ServiceError) -> Result<ServiceResponse> {
    let (status, code) = match error {
        ServiceError::InvalidRequest | ServiceError::InvalidHttp => (400, "invalid_request"),
        ServiceError::NotFound => (404, "not_found"),
        ServiceError::Capacity => (413, "capacity"),
        ServiceError::Timeout => (408, "timeout"),
        ServiceError::Cancelled => (503, "cancelled"),
        ServiceError::Unavailable(_) => (503, "unavailable"),
        ServiceError::Backend(_) => (500, "backend"),
        ServiceError::Transport(_) => (502, "transport"),
    };
    let detail = match error {
        ServiceError::Unavailable(detail) | ServiceError::Backend(detail) => Some(*detail),
        _ => None,
    };
    let mut body = format!("{{\"error\":\"{code}\"");
    if let Some(detail) = detail {
        body.push_str(",\"capability\":");
        body.push_str(RpcValue::string(detail).map_err(rpc_error)?.as_json());
    }
    body.push('}');
    ServiceResponse::json(status, RpcValue::parse(&body).map_err(rpc_error)?)
}
fn write_response(
    stream: &mut TcpStream,
    response: &ServiceResponse,
    start: Instant,
    timeout: Duration,
    control: &BootstrapControl,
) -> Result<()> {
    if !(200..=599).contains(&response.status) || response.body.len() > MAX_SERVICE_RESPONSE {
        return Err(ServiceError::InvalidRequest);
    }
    if response.content_type == ServiceContentType::Json {
        RpcValue::parse(&response.body).map_err(|_| ServiceError::InvalidRequest)?;
    }
    let content = match response.content_type {
        ServiceContentType::Json => "application/json",
        ServiceContentType::Text => "text/plain; charset=utf-8",
    };
    let header=format!("HTTP/1.1 {} Response\r\nContent-Type: {content}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",response.status,response.body.len());
    write_all(stream, header.as_bytes(), start, timeout, control)?;
    write_all(stream, response.body.as_bytes(), start, timeout, control)
}
