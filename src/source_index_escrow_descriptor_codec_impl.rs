// Existing source-escrow scalar fields only. No financial value or writer
// capability can be decoded through this metadata expectation reader.
const MAX_SOURCE_ESCROW_SCALAR_BYTES: usize = 1024 * 1024;
impl SourceIndexGenerationEscrowPinV1 {
    pub fn canonical_scalar_bytes(&self) -> std::result::Result<Vec<u8>, SourceRuleError> {
        let mut raw = vec![];
        self.visit_canonical_fields(&mut |path, value| {
            source_escrow_scalar_field(&mut raw, path, value).map_err(|_| SourceRuleError::Capacity)
        })?;
        Ok(raw)
    }
    pub fn from_canonical_scalar_bytes(namespace: &Namespace, raw: &[u8]) -> Result<Self> {
        if raw.len() > MAX_SOURCE_ESCROW_SCALAR_BYTES {
            return Err(CompactIndexError::Limit);
        }
        let mut cursor = crate::codec::Cursor::new(raw);
        let mut fields = vec![];
        while cursor.end().is_err() {
            if fields.len() >= 128 * 256 + 4 {
                return Err(CompactIndexError::Limit);
            }
            let path = history_pin_wire_text(&mut cursor, 4096)?;
            let body = history_pin_wire_text(&mut cursor, MAX_SOURCE_ESCROW_SCALAR_BYTES)?;
            let value = V::from_diagnostic_json(&body)?;
            if matches!(value, V::Array(_) | V::Object(_)) || value.diagnostic_json()? != body {
                return Err(CompactIndexError::Corrupt);
            }
            fields.push((path, value));
        }
        let mut reader = HistoryPinFieldReader { fields, next: 0 };
        if reader.text("profile")? != "TL-TXINDEX-EXTERNAL-GENERATION-ESCROW-13-1" {
            return Err(CompactIndexError::WrongContext);
        }
        let descriptor = source_escrow_decode_hex(&reader.text("storageDescriptorHex")?)?;
        let record = reader.hash("storageRecordSha256d")?;
        let count = reader.uint("points/count")?;
        if count == 0 || count > 128 {
            return Err(CompactIndexError::Limit);
        }
        let storage =
            crate::compact_journal::GenerationEscrowPinV1::from_canonical_descriptor_bytes(
                namespace,
                &descriptor,
                record,
            )?;
        let mut points = Vec::with_capacity(count as usize);
        for i in 0..count {
            let prefix = format!("points/{i}/");
            let mut body = vec![];
            while let Some((path, value)) = reader.fields.get(reader.next) {
                let Some(path) = path.strip_prefix(&prefix) else {
                    break;
                };
                source_escrow_scalar_field(&mut body, path, value)?;
                reader.next += 1;
            }
            points.push(SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
                namespace, &body,
            )?);
        }
        if reader.next != reader.fields.len()
            || storage.requested.len() != points.len()
            || !storage
                .requested
                .iter()
                .zip(&points)
                .all(|(s, p)| s == &p.storage)
        {
            return Err(CompactIndexError::WrongContext);
        }
        let pin = Self { storage, points };
        if pin.canonical_scalar_bytes()?.as_slice() != raw {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(pin)
    }
}
fn source_escrow_scalar_field(raw: &mut Vec<u8>, path: &str, value: &V) -> Result<()> {
    let value = value.diagnostic_json()?;
    let needed = 8usize
        .checked_add(path.len())
        .and_then(|n| n.checked_add(value.len()))
        .and_then(|n| n.checked_add(raw.len()))
        .ok_or(CompactIndexError::Limit)?;
    if needed > MAX_SOURCE_ESCROW_SCALAR_BYTES {
        return Err(CompactIndexError::Limit);
    }
    raw.extend_from_slice(
        &u32::try_from(path.len())
            .map_err(|_| CompactIndexError::Limit)?
            .to_be_bytes(),
    );
    raw.extend_from_slice(path.as_bytes());
    raw.extend_from_slice(
        &u32::try_from(value.len())
            .map_err(|_| CompactIndexError::Limit)?
            .to_be_bytes(),
    );
    raw.extend_from_slice(value.as_bytes());
    Ok(())
}
fn source_escrow_decode_hex(text: &str) -> Result<Vec<u8>> {
    if text.len() > 2 * MAX_SOURCE_ESCROW_SCALAR_BYTES
        || !text.len().is_multiple_of(2)
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(CompactIndexError::Corrupt);
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let a = if pair[0].is_ascii_digit() {
                pair[0] - b'0'
            } else {
                pair[0] - b'a' + 10
            };
            let b = if pair[1].is_ascii_digit() {
                pair[1] - b'0'
            } else {
                pair[1] - b'a' + 10
            };
            Ok(a * 16 + b)
        })
        .collect()
}
