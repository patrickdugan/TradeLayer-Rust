// Exact complete-KV hash over verified actual entries; no supplied KV import.
fn prepared_metadata_state_entries<'a>(
    entries: impl Iterator<Item = (&'a str, &'a [u8])>,
    mutations: &[Mutation],
) -> Result<NativeMetadataStateReceiptV1> {
    let mut overlay = std::collections::BTreeMap::<&str, Option<&[u8]>>::new();
    for mutation in mutations {
        let (key, value) = match mutation {
            Mutation::Put { key, value } => (key.as_str(), Some(value.as_slice())),
            Mutation::Delete { key } => (key.as_str(), None),
        };
        if overlay.insert(key, value).is_some() {
            return Err(CompactIndexError::Corrupt);
        }
    }
    let mut receipt = NativeMetadataStateReceiptV1 {
        profile: PREPARED_METADATA_STATE_PROFILE.into(),
        keys: 0,
        bytes: 0,
        chain_sha256d: crate::hash256::double_sha256(PREPARED_METADATA_STATE_PROFILE.as_bytes()),
        complete: false,
    };
    let mut emit = |key: &str, value: &[u8]| -> Result<()> {
        let mut raw = receipt.chain_sha256d.to_vec();
        raw.extend_from_slice(&receipt.keys.to_be_bytes());
        raw.extend_from_slice(&(key.len() as u32).to_be_bytes());
        raw.extend_from_slice(key.as_bytes());
        raw.extend_from_slice(&(value.len() as u64).to_be_bytes());
        raw.extend_from_slice(value);
        receipt.chain_sha256d = crate::hash256::double_sha256(&raw);
        receipt.keys = receipt
            .keys
            .checked_add(1)
            .ok_or(CompactIndexError::Limit)?;
        receipt.bytes = receipt
            .bytes
            .checked_add(key.len() as u64)
            .and_then(|n| n.checked_add(value.len() as u64))
            .ok_or(CompactIndexError::Limit)?;
        Ok(())
    };
    let mut changes = overlay.into_iter().peekable();
    for (key, value) in entries {
        while changes.peek().is_some_and(|(next, _)| *next < key) {
            let (key, value) = changes.next().expect("peeked change");
            if let Some(value) = value {
                emit(key, value)?;
            }
        }
        if changes.peek().is_some_and(|(next, _)| *next == key) {
            let (key, value) = changes.next().expect("peeked equal key");
            if let Some(value) = value {
                emit(key, value)?;
            }
        } else {
            emit(key, value)?;
        }
    }
    for (key, value) in changes {
        if let Some(value) = value {
            emit(key, value)?;
        }
    }
    receipt.complete = true;
    Ok(receipt)
}
