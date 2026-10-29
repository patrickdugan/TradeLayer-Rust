// New raw-action wire profile. No legacy decoder is changed.
const ACTIVE_SERVICE_ACTION_MAGIC: &[u8] = b"TLMASA\x01";
const MAX_ACTIVE_SERVICE_ACTION_BYTES: usize = 16 * 1024 * 1024;
const ACTIVE_SERVICE_ACTION_DOMAIN: &str = "Native13/ActiveServiceRawAction/v1";
impl NativeMainActiveServiceActionV1 {
    pub fn canonical_action_bytes(&self) -> Result<Vec<u8>, NativeMainActiveServiceErrorV1> {
        match self {
            Self::Start {
                start, documents, ..
            } => {
                start
                    .validate()
                    .map_err(|_| NativeMainActiveServiceErrorV1::InvalidInput)?;
                documents
                    .validate()
                    .map_err(|_| NativeMainActiveServiceErrorV1::InvalidInput)?;
            }
            Self::Shutdown(arguments) => V::Array(arguments.clone())
                .validate()
                .map_err(|_| NativeMainActiveServiceErrorV1::InvalidInput)?,
            _ => (),
        }
        let mut fields = NativeFieldCollector::new(
            ACTIVE_SERVICE_ACTION_DOMAIN,
            true,
            MAX_ACTIVE_SERVICE_ACTION_BYTES - ACTIVE_SERVICE_ACTION_MAGIC.len(),
        );
        self.visit_fields(&mut |p, v| fields.emit(p, v))
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        let mut bytes = ACTIVE_SERVICE_ACTION_MAGIC.to_vec();
        bytes.extend_from_slice(fields.frames.as_deref().expect("retained"));
        Ok(bytes)
    }
    pub fn from_canonical_action_bytes(
        bytes: &[u8],
    ) -> Result<Self, NativeMainActiveServiceErrorV1> {
        if bytes.len() > MAX_ACTIVE_SERVICE_ACTION_BYTES {
            return Err(NativeMainActiveServiceErrorV1::Capacity);
        }
        let raw = bytes
            .strip_prefix(ACTIVE_SERVICE_ACTION_MAGIC)
            .ok_or(NativeMainActiveServiceErrorV1::InvalidInput)?;
        let (fields, _) = native_step_parse_fields(raw, ACTIVE_SERVICE_ACTION_DOMAIN)
            .map_err(|_| NativeMainActiveServiceErrorV1::InvalidInput)?;
        let mut reader = ActiveServiceActionReader {
            fields,
            at: 0,
            nodes: 0,
        };
        let kind = reader.text("kind")?;
        let action = match kind.as_str() {
            "Start" => Self::Start {
                start: reader.value("start", 0)?,
                generation: reader.uint("generation")?,
                documents: reader.value("documents", 0)?,
            },
            "Advance" => Self::Advance {
                limit: usize::try_from(reader.uint("limit")?)
                    .map_err(|_| NativeMainActiveServiceErrorV1::Capacity)?,
            },
            "Pause" => Self::Pause,
            "AppendPending" => Self::AppendPending,
            "ResolvePost" => Self::ResolvePost,
            "FireTimer" => Self::FireTimer {
                id: reader.uint("id")?,
                generation: reader.uint("generation")?,
            },
            "ClearTimer" => Self::ClearTimer {
                id: reader.uint("id")?,
                generation: reader.uint("generation")?,
            },
            "Shutdown" => {
                let V::Array(arguments) = reader.value("arguments", 0)? else {
                    return Err(NativeMainActiveServiceErrorV1::InvalidInput);
                };
                Self::Shutdown(arguments)
            }
            _ => return Err(NativeMainActiveServiceErrorV1::InvalidInput),
        };
        if reader.at != reader.fields.len() || action.canonical_action_bytes()?.as_slice() != bytes
        {
            return Err(NativeMainActiveServiceErrorV1::InvalidInput);
        }
        Ok(action)
    }
}
struct ActiveServiceActionReader {
    fields: Vec<(String, V)>,
    at: usize,
    nodes: usize,
}
impl ActiveServiceActionReader {
    fn take(&mut self, path: &str) -> Result<V, NativeMainActiveServiceErrorV1> {
        let (actual, value) = self
            .fields
            .get(self.at)
            .ok_or(NativeMainActiveServiceErrorV1::InvalidInput)?;
        if actual != path {
            return Err(NativeMainActiveServiceErrorV1::InvalidInput);
        }
        self.at += 1;
        Ok(value.clone())
    }
    fn text(&mut self, path: &str) -> Result<String, NativeMainActiveServiceErrorV1> {
        let V::String(value) = self.take(path)? else {
            return Err(NativeMainActiveServiceErrorV1::InvalidInput);
        };
        Ok(value)
    }
    fn uint(&mut self, path: &str) -> Result<u64, NativeMainActiveServiceErrorV1> {
        let raw = self.text(path)?;
        let n = raw
            .parse::<u64>()
            .map_err(|_| NativeMainActiveServiceErrorV1::InvalidInput)?;
        if n.to_string() != raw {
            return Err(NativeMainActiveServiceErrorV1::InvalidInput);
        }
        Ok(n)
    }
    fn value(&mut self, path: &str, depth: usize) -> Result<V, NativeMainActiveServiceErrorV1> {
        self.nodes += 1;
        if depth > 64 || self.nodes > 65536 {
            return Err(NativeMainActiveServiceErrorV1::Capacity);
        }
        if self.fields.get(self.at).is_some_and(|(p, _)| p == path) {
            return self.take(path);
        }
        let kind = self.text(&format!("{path}/kind"))?;
        let count = self.uint(&format!("{path}/count"))?;
        if count > 1024 {
            return Err(NativeMainActiveServiceErrorV1::Capacity);
        }
        let value = match kind.as_str() {
            "Array" => {
                let mut values = vec![];
                for i in 0..count {
                    values.push(self.value(&format!("{path}/{i}"), depth + 1)?)
                }
                V::Array(values)
            }
            "Object" => {
                let mut values = vec![];
                for i in 0..count {
                    let k = self.text(&format!("{path}/{i}/key"))?;
                    let v = self.value(&format!("{path}/{i}/value"), depth + 1)?;
                    values.push((k, v));
                }
                V::Object(values)
            }
            _ => return Err(NativeMainActiveServiceErrorV1::InvalidInput),
        };
        Ok(value)
    }
}
