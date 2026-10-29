//! Policy14-only binary command/outcome frames nested in the new TLCR0e
//! journal. Legacy diagnostic-value readers never parse these frames.
use super::*;
pub const POLICY: u8 = 14;
pub const CALL_MAGIC: &[u8; 5] = b"TLRC\x0e";
pub const OUTCOME_MAGIC: &[u8; 5] = b"TLRO\x0e";
const MAX_FRAME: usize = 4 * 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    WrongPolicy,
    InvalidFrame,
    Capacity,
    Graph(Error),
}
impl From<Error> for ProtocolError {
    fn from(v: Error) -> Self {
        Self::Graph(v)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvokeRichV1 {
    pub target: String,
    pub arguments: Graph,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Immediate,
    Pending,
    Throw,
    PendingThrow,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichDependencyV1 {
    pub transport: Transport,
    pub epoch: u64,
    pub graph: Graph,
}
fn policy(value: u8) -> Result<(), ProtocolError> {
    if value == POLICY {
        Ok(())
    } else {
        Err(ProtocolError::WrongPolicy)
    }
}
fn push_frame(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ProtocolError> {
    let n = u32::try_from(bytes.len()).map_err(|_| ProtocolError::Capacity)?;
    let end = out
        .len()
        .checked_add(4)
        .and_then(|v| v.checked_add(bytes.len()))
        .ok_or(ProtocolError::Capacity)?;
    if end > MAX_FRAME {
        return Err(ProtocolError::Capacity);
    }
    out.extend(n.to_be_bytes());
    out.extend(bytes);
    Ok(())
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self.at.checked_add(n).ok_or(ProtocolError::InvalidFrame)?;
        let value = self
            .bytes
            .get(self.at..end)
            .ok_or(ProtocolError::InvalidFrame)?;
        self.at = end;
        Ok(value)
    }
    fn frame(&mut self) -> Result<&'a [u8], ProtocolError> {
        let n = u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| ProtocolError::InvalidFrame)?,
        ) as usize;
        self.take(n)
    }
    fn done(&self) -> Result<(), ProtocolError> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(ProtocolError::InvalidFrame)
        }
    }
}
impl InvokeRichV1 {
    pub fn encode(
        &self,
        selected_policy: u8,
        caps: &Capabilities,
        limits: Limits,
    ) -> Result<Vec<u8>, ProtocolError> {
        policy(selected_policy)?;
        validate_ref(&self.arguments, caps, limits)?;
        if self.target.is_empty() || self.target.len() > 256 {
            return Err(ProtocolError::InvalidFrame);
        }
        let graph = wire::encode(&self.arguments)?;
        let mut out = CALL_MAGIC.to_vec();
        push_frame(&mut out, self.target.as_bytes())?;
        push_frame(&mut out, &graph)?;
        Ok(out)
    }
    pub fn decode(
        selected_policy: u8,
        bytes: &[u8],
        caps: &Capabilities,
        limits: Limits,
    ) -> Result<Self, ProtocolError> {
        // Fence policy before graph parsing, capability validation or owner execution.
        policy(selected_policy)?;
        if bytes.len() > MAX_FRAME {
            return Err(ProtocolError::Capacity);
        }
        let mut r = Reader { bytes, at: 0 };
        if r.take(5)? != CALL_MAGIC {
            return Err(ProtocolError::InvalidFrame);
        }
        let target = std::str::from_utf8(r.frame()?)
            .map_err(|_| ProtocolError::InvalidFrame)?
            .to_owned();
        let arguments = wire::decode_external(r.frame()?, caps, limits)?
            .graph()
            .clone();
        r.done()?;
        let result = Self { target, arguments };
        if result.encode(selected_policy, caps, limits)? != bytes {
            return Err(ProtocolError::InvalidFrame);
        }
        Ok(result)
    }
}
impl RichDependencyV1 {
    pub fn encode(
        &self,
        selected_policy: u8,
        caps: &Capabilities,
        limits: Limits,
    ) -> Result<Vec<u8>, ProtocolError> {
        policy(selected_policy)?;
        if self.graph.roots.len() != 1 {
            return Err(ProtocolError::InvalidFrame);
        }
        validate_ref(&self.graph, caps, limits)?;
        let mut out = OUTCOME_MAGIC.to_vec();
        out.push(match self.transport {
            Transport::Immediate => 0,
            Transport::Pending => 1,
            Transport::Throw => 2,
            Transport::PendingThrow => 3,
        });
        out.extend(self.epoch.to_be_bytes());
        push_frame(&mut out, &wire::encode(&self.graph)?)?;
        Ok(out)
    }
    pub fn decode(
        selected_policy: u8,
        bytes: &[u8],
        caps: &Capabilities,
        limits: Limits,
    ) -> Result<Self, ProtocolError> {
        policy(selected_policy)?;
        if bytes.len() > MAX_FRAME {
            return Err(ProtocolError::Capacity);
        }
        let mut r = Reader { bytes, at: 0 };
        if r.take(5)? != OUTCOME_MAGIC {
            return Err(ProtocolError::InvalidFrame);
        }
        let transport = match r.take(1)?[0] {
            0 => Transport::Immediate,
            1 => Transport::Pending,
            2 => Transport::Throw,
            3 => Transport::PendingThrow,
            _ => return Err(ProtocolError::InvalidFrame),
        };
        let epoch = u64::from_be_bytes(
            r.take(8)?
                .try_into()
                .map_err(|_| ProtocolError::InvalidFrame)?,
        );
        let graph = wire::decode_external(r.frame()?, caps, limits)?
            .graph()
            .clone();
        r.done()?;
        let result = Self {
            transport,
            epoch,
            graph,
        };
        if result.encode(selected_policy, caps, limits)? != bytes {
            return Err(ProtocolError::InvalidFrame);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn graph() -> Graph {
        Graph {
            roots: vec![Atom::Null],
            objects: vec![],
            symbols: vec![],
        }
    }
    #[test]
    fn legacy_and_native13_fence_precedes_even_malformed_input() {
        let call = InvokeRichV1 {
            target: "Main.getCurrentBlockHeight".into(),
            arguments: graph(),
        };
        let caps = Capabilities::default();
        let limits = Limits::default();
        let bytes = call.encode(14, &caps, limits).unwrap();
        for old in 0..=13 {
            assert_eq!(
                call.encode(old, &caps, limits),
                Err(ProtocolError::WrongPolicy)
            );
            assert_eq!(
                InvokeRichV1::decode(old, &bytes, &caps, limits),
                Err(ProtocolError::WrongPolicy)
            );
            assert_eq!(
                InvokeRichV1::decode(old, b"malformed", &caps, limits),
                Err(ProtocolError::WrongPolicy)
            );
            assert_eq!(
                RichDependencyV1::decode(old, b"malformed", &caps, limits),
                Err(ProtocolError::WrongPolicy)
            );
        }
    }
    #[test]
    fn every_transport_roundtrips_without_legacy_string_error_fields() {
        let caps = Capabilities::default();
        let limits = Limits::default();
        for transport in [
            Transport::Immediate,
            Transport::Pending,
            Transport::Throw,
            Transport::PendingThrow,
        ] {
            for atom in [
                Atom::Null,
                Atom::Undefined,
                Atom::Boolean(false),
                Atom::Binary64((-0f64).to_bits()),
                Atom::Utf16(vec![0xd800]),
                Atom::BigInt("9007199254740993".into()),
            ] {
                let r = RichDependencyV1 {
                    transport,
                    epoch: 7,
                    graph: Graph {
                        roots: vec![atom],
                        objects: vec![],
                        symbols: vec![],
                    },
                };
                let bytes = r.encode(14, &caps, limits).unwrap();
                assert_eq!(RichDependencyV1::decode(14, &bytes, &caps, limits), Ok(r));
            }
        }
    }
    #[test]
    fn call_roundtrip_is_exact_and_rejects_truncation_trailing_or_wrong_magic() {
        let caps = Capabilities::default();
        let limits = Limits::default();
        let call = InvokeRichV1 {
            target: "Main.getCurrentBlockHeight".into(),
            arguments: graph(),
        };
        let bytes = call.encode(14, &caps, limits).unwrap();
        assert_eq!(InvokeRichV1::decode(14, &bytes, &caps, limits), Ok(call));
        for n in 0..bytes.len() {
            assert!(InvokeRichV1::decode(14, &bytes[..n], &caps, limits).is_err())
        }
        let mut bad = bytes.clone();
        bad.push(0);
        assert!(InvokeRichV1::decode(14, &bad, &caps, limits).is_err());
        let mut bad = bytes;
        bad[4] = 13;
        assert!(InvokeRichV1::decode(14, &bad, &caps, limits).is_err());
    }
}
