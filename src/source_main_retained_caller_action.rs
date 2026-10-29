//! Raw retained-caller inputs only. No source result/state/plan decoder exists.
use crate::validity_source::SourceValue as V;
pub const PROFILE: &str = "TL-MAIN-RETAINED-CALLER-RAW-ACTION-13-1";
const MAGIC: &[u8] = b"TLMACA\x01";
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_NODES: usize = 65536;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedMainCallerStartV1 {
    Initialize,
    InitOrLoad,
    SyncIndex { max: V },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedMainCallerTimerLaneV1 {
    CallerDelay,
    ConsensusService,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedMainCallerActionV1 {
    Start {
        caller: RetainedMainCallerStartV1,
        scope: u64,
        generation: u64,
        documents: V,
    },
    AdvanceCaller,
    EnterConsensus,
    AdvanceConsensus {
        limit: usize,
    },
    AdvanceDetached {
        selector: V,
    },
    AppendSelected {
        selector: V,
    },
    ResolveSelectedPost {
        selector: V,
    },
    FireTimer {
        lane: RetainedMainCallerTimerLaneV1,
        id: u64,
        generation: u64,
    },
    Pause,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedMainCallerActionErrorV1 {
    InvalidEncoding,
    InvalidInput,
    Capacity,
}
type Result<T> = std::result::Result<T, RetainedMainCallerActionErrorV1>;
fn count(out: &mut Vec<u8>, n: usize) -> Result<()> {
    out.extend_from_slice(
        &u32::try_from(n)
            .map_err(|_| RetainedMainCallerActionErrorV1::Capacity)?
            .to_be_bytes(),
    );
    Ok(())
}
fn string(out: &mut Vec<u8>, s: &str) -> Result<()> {
    count(out, s.len())?;
    out.extend_from_slice(s.as_bytes());
    bounded(out)
}
fn bounded(out: &[u8]) -> Result<()> {
    if out.len() > MAX_BYTES {
        Err(RetainedMainCallerActionErrorV1::Capacity)
    } else {
        Ok(())
    }
}
fn value(out: &mut Vec<u8>, v: &V, depth: usize, nodes: &mut usize) -> Result<()> {
    *nodes = nodes
        .checked_add(1)
        .ok_or(RetainedMainCallerActionErrorV1::Capacity)?;
    if depth > 64 || *nodes > MAX_NODES {
        return Err(RetainedMainCallerActionErrorV1::Capacity);
    }
    match v {
        V::Undefined => out.push(0),
        V::Null => out.push(1),
        V::Bool(b) => {
            out.push(2);
            out.push(u8::from(*b));
        }
        V::Number(bits) => {
            out.push(3);
            out.extend_from_slice(&bits.to_be_bytes());
        }
        V::String(s) => {
            out.push(4);
            string(out, s)?;
        }
        V::Array(a) => {
            out.push(5);
            count(out, a.len())?;
            for v in a {
                value(out, v, depth + 1, nodes)?;
            }
        }
        V::Object(a) => {
            out.push(6);
            count(out, a.len())?;
            for (k, v) in a {
                string(out, k)?;
                value(out, v, depth + 1, nodes)?;
            }
        }
        V::Promise => out.push(7),
        V::BigNumber(s) => {
            out.push(8);
            string(out, s)?;
        }
    }
    bounded(out)
}
struct Reader<'a> {
    raw: &'a [u8],
    at: usize,
    nodes: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or(RetainedMainCallerActionErrorV1::Capacity)?;
        let bytes = self
            .raw
            .get(self.at..end)
            .ok_or(RetainedMainCallerActionErrorV1::InvalidEncoding)?;
        self.at = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn uint(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().map_err(
            |_| RetainedMainCallerActionErrorV1::InvalidEncoding,
        )?))
    }
    fn count(&mut self) -> Result<usize> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| RetainedMainCallerActionErrorV1::InvalidEncoding)?,
        ) as usize)
    }
    fn string(&mut self) -> Result<String> {
        let n = self.count()?;
        let bytes = self.take(n)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| RetainedMainCallerActionErrorV1::InvalidEncoding)
    }
    fn value(&mut self, depth: usize) -> Result<V> {
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or(RetainedMainCallerActionErrorV1::Capacity)?;
        if depth > 64 || self.nodes > MAX_NODES {
            return Err(RetainedMainCallerActionErrorV1::Capacity);
        }
        Ok(match self.byte()? {
            0 => V::Undefined,
            1 => V::Null,
            2 => V::Bool(match self.byte()? {
                0 => false,
                1 => true,
                _ => return Err(RetainedMainCallerActionErrorV1::InvalidEncoding),
            }),
            3 => V::Number(self.uint()?),
            4 => V::String(self.string()?),
            7 => V::Promise,
            8 => V::BigNumber(self.string()?),
            kind @ (5 | 6) => {
                let n = self.count()?;
                if n > 1024 {
                    return Err(RetainedMainCallerActionErrorV1::Capacity);
                }
                if kind == 5 {
                    let mut a = Vec::new();
                    for _ in 0..n {
                        a.push(self.value(depth + 1)?);
                    }
                    V::Array(a)
                } else {
                    let mut a = Vec::new();
                    for _ in 0..n {
                        let k = self.string()?;
                        let v = self.value(depth + 1)?;
                        a.push((k, v));
                    }
                    V::Object(a)
                }
            }
            _ => return Err(RetainedMainCallerActionErrorV1::InvalidEncoding),
        })
    }
}
impl RetainedMainCallerActionV1 {
    pub fn canonical_action_bytes(&self) -> Result<Vec<u8>> {
        let mut out = MAGIC.to_vec();
        let mut nodes = 0;
        let checked = |v: &V| {
            v.validate()
                .map_err(|_| RetainedMainCallerActionErrorV1::InvalidInput)
        };
        match self {
            Self::Start {
                caller,
                scope,
                generation,
                documents,
            } => {
                out.push(0);
                match caller {
                    RetainedMainCallerStartV1::Initialize => out.push(0),
                    RetainedMainCallerStartV1::InitOrLoad => out.push(1),
                    RetainedMainCallerStartV1::SyncIndex { max } => {
                        out.push(2);
                        checked(max)?;
                        value(&mut out, max, 0, &mut nodes)?;
                    }
                }
                out.extend_from_slice(&scope.to_be_bytes());
                out.extend_from_slice(&generation.to_be_bytes());
                checked(documents)?;
                value(&mut out, documents, 0, &mut nodes)?;
            }
            Self::AdvanceCaller => out.push(1),
            Self::EnterConsensus => out.push(2),
            Self::AdvanceConsensus { limit } => {
                out.push(3);
                out.extend_from_slice(
                    &u64::try_from(*limit)
                        .map_err(|_| RetainedMainCallerActionErrorV1::Capacity)?
                        .to_be_bytes(),
                );
            }
            Self::AdvanceDetached { selector }
            | Self::AppendSelected { selector }
            | Self::ResolveSelectedPost { selector } => {
                out.push(match self {
                    Self::AdvanceDetached { .. } => 4,
                    Self::AppendSelected { .. } => 5,
                    _ => 6,
                });
                checked(selector)?;
                value(&mut out, selector, 0, &mut nodes)?;
            }
            Self::FireTimer {
                lane,
                id,
                generation,
            } => {
                out.push(7);
                out.push(match lane {
                    RetainedMainCallerTimerLaneV1::CallerDelay => 0,
                    RetainedMainCallerTimerLaneV1::ConsensusService => 1,
                });
                out.extend_from_slice(&id.to_be_bytes());
                out.extend_from_slice(&generation.to_be_bytes());
            }
            Self::Pause => out.push(8),
        }
        bounded(&out)?;
        Ok(out)
    }
    pub fn from_canonical_action_bytes(raw: &[u8]) -> Result<Self> {
        bounded(raw)?;
        let raw_body = raw
            .strip_prefix(MAGIC)
            .ok_or(RetainedMainCallerActionErrorV1::InvalidEncoding)?;
        let mut reader = Reader {
            raw: raw_body,
            at: 0,
            nodes: 0,
        };
        let action = match reader.byte()? {
            0 => {
                let caller = match reader.byte()? {
                    0 => RetainedMainCallerStartV1::Initialize,
                    1 => RetainedMainCallerStartV1::InitOrLoad,
                    2 => RetainedMainCallerStartV1::SyncIndex {
                        max: reader.value(0)?,
                    },
                    _ => return Err(RetainedMainCallerActionErrorV1::InvalidEncoding),
                };
                Self::Start {
                    caller,
                    scope: reader.uint()?,
                    generation: reader.uint()?,
                    documents: reader.value(0)?,
                }
            }
            1 => Self::AdvanceCaller,
            2 => Self::EnterConsensus,
            3 => Self::AdvanceConsensus {
                limit: usize::try_from(reader.uint()?)
                    .map_err(|_| RetainedMainCallerActionErrorV1::Capacity)?,
            },
            4 => Self::AdvanceDetached {
                selector: reader.value(0)?,
            },
            5 => Self::AppendSelected {
                selector: reader.value(0)?,
            },
            6 => Self::ResolveSelectedPost {
                selector: reader.value(0)?,
            },
            7 => {
                let lane = match reader.byte()? {
                    0 => RetainedMainCallerTimerLaneV1::CallerDelay,
                    1 => RetainedMainCallerTimerLaneV1::ConsensusService,
                    _ => return Err(RetainedMainCallerActionErrorV1::InvalidEncoding),
                };
                Self::FireTimer {
                    lane,
                    id: reader.uint()?,
                    generation: reader.uint()?,
                }
            }
            8 => Self::Pause,
            _ => return Err(RetainedMainCallerActionErrorV1::InvalidEncoding),
        };
        if reader.at != raw_body.len() || action.canonical_action_bytes()?.as_slice() != raw {
            return Err(RetainedMainCallerActionErrorV1::InvalidEncoding);
        }
        Ok(action)
    }
}
#[cfg(test)]
#[path = "source_main_retained_caller_action_tests.rs"]
mod tests;
