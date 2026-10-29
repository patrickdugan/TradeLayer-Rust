// Raw commands only. Output/kernel/financial state is never imported.
struct NativeStepFieldReader {
    fields: Vec<(String, V)>,
    at: usize,
}
fn native_step_invalid() -> NativeStepSessionError {
    NativeStepSessionError::Backend("invalid Native13 step stream".into())
}
fn native_step_parse_fields(
    bytes: &[u8],
    domain: &str,
) -> Result<(Vec<(String, V)>, NativeCanonicalFieldsV1), NativeStepSessionError> {
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(NativeStepSessionError::Capacity);
    }
    let mut cursor = crate::codec::Cursor::new(bytes);
    let mut fields = vec![];
    let mut collector = NativeFieldCollector::new(domain, true, bytes.len());
    while cursor.end().is_err() {
        let count = cursor.u32().map_err(|_| native_step_invalid())? as usize;
        if count > 65536 {
            return Err(NativeStepSessionError::Capacity);
        }
        let path = std::str::from_utf8(cursor.take(count).map_err(|_| native_step_invalid())?)
            .map_err(|_| native_step_invalid())?
            .to_string();
        let count = cursor.u32().map_err(|_| native_step_invalid())? as usize;
        if count > 4 * 1024 * 1024 {
            return Err(NativeStepSessionError::Capacity);
        }
        let raw = std::str::from_utf8(cursor.take(count).map_err(|_| native_step_invalid())?)
            .map_err(|_| native_step_invalid())?;
        let value = crate::property_catalog::parse_diagnostic_value(raw)
            .map_err(|_| native_step_invalid())?;
        collector
            .emit(&path, &value)
            .map_err(NativeStepSessionError::Input)?;
        fields.push((path, value));
    }
    if collector.frames.as_deref() != Some(bytes) {
        return Err(native_step_invalid());
    }
    Ok((fields, collector.summary))
}
impl NativeStepFieldReader {
    fn take(&mut self, path: &str) -> Result<V, NativeStepSessionError> {
        let (actual, value) = self.fields.get(self.at).ok_or_else(native_step_invalid)?;
        if actual != path {
            return Err(native_step_invalid());
        }
        self.at += 1;
        Ok(value.clone())
    }
    fn text(&mut self, path: &str) -> Result<String, NativeStepSessionError> {
        let V::String(value) = self.take(path)? else {
            return Err(native_step_invalid());
        };
        Ok(value)
    }
    fn uint(&mut self, path: &str) -> Result<u64, NativeStepSessionError> {
        let value = self.text(path)?;
        let n = value.parse::<u64>().map_err(|_| native_step_invalid())?;
        if n.to_string() != value {
            return Err(native_step_invalid());
        }
        Ok(n)
    }
    fn original_value(
        &mut self,
        path: &str,
        depth: usize,
        nodes: &mut usize,
    ) -> Result<V, NativeStepSessionError> {
        *nodes = nodes
            .checked_add(1)
            .ok_or(NativeStepSessionError::Capacity)?;
        if depth > 64 || *nodes > 8192 {
            return Err(NativeStepSessionError::Capacity);
        }
        if self
            .fields
            .get(self.at)
            .is_some_and(|(next, _)| next == &format!("{path}/atom"))
        {
            return self.take(&format!("{path}/atom"));
        }
        let kind = self.text(&format!("{path}/kind"))?;
        let count = self.uint(&format!("{path}/length"))?;
        if count > 1024 {
            return Err(NativeStepSessionError::Capacity);
        }
        let value = match kind.as_str() {
            "Array" => {
                let mut values = Vec::with_capacity(count as usize);
                for at in 0..count {
                    values.push(self.original_value(
                        &format!("{path}/items/{at}"),
                        depth + 1,
                        nodes,
                    )?);
                }
                V::Array(values)
            }
            "Object" => {
                let mut fields = Vec::with_capacity(count as usize);
                for at in 0..count {
                    let key = self.text(&format!("{path}/fields/{at}/key"))?;
                    let value = self.original_value(
                        &format!("{path}/fields/{at}/value"),
                        depth + 1,
                        nodes,
                    )?;
                    fields.push((key, value));
                }
                V::Object(fields)
            }
            _ => return Err(native_step_invalid()),
        };
        Ok(value)
    }
    fn command(mut self) -> Result<MainStepCommandV1, NativeStepSessionError> {
        if self.text("profile")? != crate::source_main_step_kernel::CANONICAL_FIELD_PROFILE {
            return Err(native_step_invalid());
        }
        let kind = self.text("kind")?;
        let command = match kind.as_str() {
            "Start" => {
                let method = self.text("call/target")?;
                let count = self.uint("call/arguments/length")?;
                if count > 64 {
                    return Err(NativeStepSessionError::Capacity);
                }
                let mut arguments = vec![];
                for at in 0..count {
                    arguments.push(self.original_value(
                        &format!("call/arguments/{at}"),
                        0,
                        &mut 0,
                    )?);
                }
                MainStepCommandV1::Start { method, arguments }
            }
            "DrainReady" => MainStepCommandV1::DrainReady {
                limit: usize::try_from(self.uint("limit")?)
                    .map_err(|_| NativeStepSessionError::Capacity)?,
            },
            "FireTimer" => MainStepCommandV1::FireTimer {
                id: self.uint("id")?,
                generation: self.uint("generation")?,
            },
            "ClearTimer" => MainStepCommandV1::ClearTimer {
                id: self.uint("id")?,
                generation: self.uint("generation")?,
            },
            "UpdateRecordedIo" => {
                MainStepCommandV1::UpdateRecordedIo(self.original_value("input", 0, &mut 0)?)
            }
            "Shutdown" => {
                let count = self.uint("arguments/length")?;
                if count > 8 {
                    return Err(NativeStepSessionError::Capacity);
                }
                let mut arguments = vec![];
                for at in 0..count {
                    arguments.push(self.original_value(&format!("arguments/{at}"), 0, &mut 0)?);
                }
                MainStepCommandV1::Shutdown(arguments)
            }
            _ => return Err(native_step_invalid()),
        };
        if self.at != self.fields.len() {
            return Err(native_step_invalid());
        }
        native_step_validate_input(&command).map_err(NativeStepSessionError::Input)?;
        Ok(command)
    }
}
fn native_step_decode_command(bytes: &[u8]) -> Result<MainStepCommandV1, NativeStepSessionError> {
    let (fields, _) = native_step_parse_fields(bytes, "Native13/StepCommand/v1")?;
    let command = NativeStepFieldReader { fields, at: 0 }.command()?;
    let mut collector = NativeFieldCollector::new("Native13/StepCommand/v1", true, bytes.len());
    command
        .visit_canonical_fields(&mut |path, value| collector.emit(path, value))
        .map_err(NativeStepSessionError::Input)?;
    if collector.frames.as_deref() != Some(bytes) {
        return Err(native_step_invalid());
    }
    Ok(command)
}
