struct NativeCompletedBytes {
    data: Vec<u8>,
    maximum: usize,
    failed: bool,
}
impl NativeCompletedBytes {
    fn new(maximum: usize) -> Self {
        Self {
            data: vec![],
            maximum,
            failed: false,
        }
    }
    fn extend_from_slice(&mut self, bytes: &[u8]) {
        if self.failed {
            return;
        }
        let Some(length) = self.data.len().checked_add(bytes.len()) else {
            self.failed = true;
            return;
        };
        if length > self.maximum || self.data.try_reserve(bytes.len()).is_err() {
            self.failed = true;
            return;
        }
        self.data.extend_from_slice(bytes);
    }
    fn finish(self) -> Result<Vec<u8>, NativeMainSessionError> {
        if self.failed {
            Err(NativeMainSessionError::Capacity)
        } else {
            Ok(self.data)
        }
    }
}
fn native_completed_frame(
    bytes: &mut NativeCompletedBytes,
    value: &V,
) -> Result<(), NativeMainSessionError> {
    let text = value
        .diagnostic_json()
        .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
    let length = u32::try_from(text.len()).map_err(|_| NativeMainSessionError::Capacity)?;
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(text.as_bytes());
    if bytes.failed {
        Err(NativeMainSessionError::Capacity)
    } else {
        Ok(())
    }
}
