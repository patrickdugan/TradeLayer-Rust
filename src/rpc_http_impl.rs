// Strict single-response HTTP/1.x framing, bounded before body allocation.
struct RpcReader<'a> {
    stream: &'a mut TcpStream,
    buffer: Vec<u8>,
    at: usize,
    start: Instant,
    timeout: Duration,
    control: Option<&'a BootstrapControl>,
}
impl RpcReader<'_> {
    fn more(&mut self) -> Result<()> {
        rpc_check_control(self.control)?;
        self.stream
            .set_read_timeout(Some(remaining(self.start, self.timeout)?))
            .map_err(io_error)?;
        let mut bytes = [0u8; 8192];
        let n = self.stream.read(&mut bytes).map_err(io_error)?;
        if n == 0 {
            return Err(RpcError::InvalidHttp);
        }
        // Compact consumed bytes so a chunked body cannot retain wire history.
        if self.at > 0 {
            self.buffer.drain(..self.at);
            self.at = 0;
        }
        self.buffer.extend_from_slice(&bytes[..n]);
        Ok(())
    }
    fn line(&mut self, cap: usize) -> Result<String> {
        loop {
            rpc_check_control(self.control)?;
            let rest = &self.buffer[self.at..];
            if let Some(end) = rest.windows(2).position(|w| w == b"\r\n") {
                if end > cap {
                    return Err(RpcError::Capacity);
                }
                let line = std::str::from_utf8(&rest[..end]).map_err(|_| RpcError::InvalidHttp)?;
                if line.bytes().any(|b| b < 0x20 && b != b'\t' || b == 0x7f) {
                    return Err(RpcError::InvalidHttp);
                }
                let line = line.to_owned();
                self.at += end + 2;
                return Ok(line);
            }
            if rest.len() > cap {
                return Err(RpcError::Capacity);
            }
            self.more()?;
        }
    }
    fn exact_into(&mut self, count: usize, output: &mut Vec<u8>) -> Result<()> {
        if count > MAX_RPC_BODY
            || output
                .len()
                .checked_add(count)
                .is_none_or(|n| n > MAX_RPC_BODY)
        {
            return Err(RpcError::Capacity);
        }
        let mut left = count;
        while left > 0 {
            rpc_check_control(self.control)?;
            if self.at == self.buffer.len() {
                self.more()?;
            }
            let take = left.min(self.buffer.len() - self.at);
            output.extend_from_slice(&self.buffer[self.at..self.at + take]);
            self.at += take;
            left -= take;
        }
        Ok(())
    }
}
fn header(line: &str) -> Result<(String, &str)> {
    let (name, value) = line.split_once(':').ok_or(RpcError::InvalidHttp)?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
    {
        return Err(RpcError::InvalidHttp);
    }
    Ok((name.to_ascii_lowercase(), value.trim_matches([' ', '\t'])))
}
pub(crate) fn read_http(
    stream: &mut TcpStream,
    start: Instant,
    timeout: Duration,
    control: Option<&BootstrapControl>,
) -> Result<(u16, String)> {
    let mut reader = RpcReader {
        stream,
        buffer: Vec::new(),
        at: 0,
        start,
        timeout,
        control,
    };
    let line = reader.line(1024)?;
    let mut parts = line.split(' ');
    if !matches!(parts.next(), Some("HTTP/1.0" | "HTTP/1.1")) {
        return Err(RpcError::InvalidHttp);
    }
    let status = parts
        .next()
        .filter(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()))
        .ok_or(RpcError::InvalidHttp)?
        .parse::<u16>()
        .map_err(|_| RpcError::InvalidHttp)?;
    if !(200..=599).contains(&status) {
        return Err(RpcError::InvalidHttp);
    }
    let mut length = None;
    let mut chunked = false;
    let mut encoding = false;
    let mut headers = line.len() + 2;
    for count in 0..=64 {
        let line = reader.line(MAX_RPC_HEADERS.saturating_sub(headers))?;
        headers += line.len() + 2;
        if headers > MAX_RPC_HEADERS {
            return Err(RpcError::Capacity);
        }
        if line.is_empty() {
            break;
        }
        if count == 64 {
            return Err(RpcError::Capacity);
        }
        let (name, value) = header(&line)?;
        match name.as_str() {
            "content-length" => {
                if length.is_some()
                    || chunked
                    || value.is_empty()
                    || !value.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(RpcError::InvalidHttp);
                }
                let n = value.parse::<usize>().map_err(|_| RpcError::Capacity)?;
                if n > MAX_RPC_BODY {
                    return Err(RpcError::Capacity);
                }
                length = Some(n);
            }
            "transfer-encoding" => {
                if chunked || length.is_some() || !value.eq_ignore_ascii_case("chunked") {
                    return Err(RpcError::InvalidHttp);
                }
                chunked = true;
            }
            "content-encoding" => {
                if encoding || !value.eq_ignore_ascii_case("identity") {
                    return Err(RpcError::InvalidHttp);
                }
                encoding = true;
            }
            _ => {}
        }
    }
    let mut body = Vec::new();
    if chunked {
        let mut wire_overhead = 0usize;
        let mut chunks = 0usize;
        loop {
            let line = reader.line(1024)?;
            wire_overhead = wire_overhead
                .checked_add(line.len() + 4)
                .ok_or(RpcError::Capacity)?;
            chunks += 1;
            if chunks > MAX_RPC_NODES || wire_overhead > MAX_RPC_BODY {
                return Err(RpcError::Capacity);
            }
            let size = line.split(';').next().ok_or(RpcError::InvalidHttp)?;
            if size.is_empty() || !size.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(RpcError::InvalidHttp);
            }
            let size = usize::from_str_radix(size, 16).map_err(|_| RpcError::Capacity)?;
            if size == 0 {
                break;
            }
            reader.exact_into(size, &mut body)?;
            let mut crlf = Vec::new();
            reader.exact_into(2, &mut crlf)?;
            if crlf != b"\r\n" {
                return Err(RpcError::InvalidHttp);
            }
        }
        let mut trailers = 0;
        for count in 0..=64 {
            let line = reader.line(MAX_RPC_HEADERS.saturating_sub(trailers))?;
            trailers += line.len() + 2;
            if trailers > MAX_RPC_HEADERS {
                return Err(RpcError::Capacity);
            }
            if line.is_empty() {
                break;
            }
            if count == 64 {
                return Err(RpcError::Capacity);
            }
            let (name, _) = header(&line)?;
            if matches!(
                name.as_str(),
                "content-length" | "transfer-encoding" | "content-encoding"
            ) {
                return Err(RpcError::InvalidHttp);
            }
        }
    } else if let Some(length) = length {
        reader.exact_into(length, &mut body)?;
    } else {
        // Core supplies explicit length. An unframed persistent body must not
        // turn a request into an indefinite wait or a truncation ambiguity.
        return Err(RpcError::InvalidHttp);
    }
    if reader.at != reader.buffer.len() {
        return Err(RpcError::InvalidHttp);
    }
    remaining(start, timeout)?;
    Ok((
        status,
        String::from_utf8(body).map_err(|_| RpcError::InvalidResponse)?,
    ))
}
