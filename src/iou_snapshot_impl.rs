// TLIO v1 preserves the source bucket counters and JavaScript claim order.
// CRC32 detects accidental damage. These bytes are not an authenticated state.

const IOU_SNAPSHOT_MAGIC: &[u8; 5] = b"TLIO\x01";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IouSnapshotError {
    InvalidFormat(&'static str),
    InvalidIou(IouError),
}

impl core::fmt::Display for IouSnapshotError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for IouSnapshotError {}

fn iou_snapshot_bad(reason: &'static str) -> IouSnapshotError {
    IouSnapshotError::InvalidFormat(reason)
}

fn iou_snapshot_append(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), IouSnapshotError> {
    if out.len().saturating_add(bytes.len()) > crate::codec::MAX_CHECKPOINT_BYTES - 4 {
        return Err(iou_snapshot_bad("IOU snapshot too large"));
    }
    out.extend_from_slice(bytes);
    Ok(())
}

fn iou_snapshot_count(
    reader: &mut crate::codec::Cursor<'_>,
    maximum: usize,
) -> Result<usize, IouSnapshotError> {
    let count = reader
        .u32()
        .map_err(|_| iou_snapshot_bad("count truncated"))? as usize;
    if count > maximum {
        return Err(iou_snapshot_bad("count exceeds IOU limit"));
    }
    Ok(count)
}

fn iou_snapshot_key(reader: &mut crate::codec::Cursor<'_>) -> Result<IouKey, IouSnapshotError> {
    Ok(IouKey::new(
        reader
            .u32()
            .map_err(|_| iou_snapshot_bad("key truncated"))?,
        reader
            .u32()
            .map_err(|_| iou_snapshot_bad("key truncated"))?,
    ))
}

fn iou_snapshot_write_key(out: &mut Vec<u8>, key: IouKey) -> Result<(), IouSnapshotError> {
    iou_snapshot_append(out, &key.contract_id.to_be_bytes())?;
    iou_snapshot_append(out, &key.property_id.to_be_bytes())
}

fn iou_snapshot_height(reader: &mut crate::codec::Cursor<'_>) -> Result<u64, IouSnapshotError> {
    let height = reader
        .u64()
        .map_err(|_| iou_snapshot_bad("block truncated"))?;
    check_block(height).map_err(IouSnapshotError::InvalidIou)?;
    Ok(height)
}

fn iou_snapshot_write_decimal(out: &mut Vec<u8>, value: &Decimal) -> Result<(), IouSnapshotError> {
    let value = value.to_string();
    iou_snapshot_append(out, &(value.len() as u32).to_be_bytes())?;
    iou_snapshot_append(out, value.as_bytes())
}

fn iou_snapshot_decimal(
    reader: &mut crate::codec::Cursor<'_>,
) -> Result<Decimal, IouSnapshotError> {
    let length = reader
        .u32()
        .map_err(|_| iou_snapshot_bad("decimal length"))? as usize;
    // Check before copying or invoking the coefficient parser. The parser's
    // independent digit/scale limits apply as well as this wire-byte limit.
    if length == 0 || length > crate::decimal::MAX_DECIMAL_INPUT_BYTES {
        return Err(iou_snapshot_bad("decimal length exceeds limit"));
    }
    let raw = reader
        .take(length)
        .map_err(|_| iou_snapshot_bad("decimal truncated"))?;
    let text = core::str::from_utf8(raw).map_err(|_| iou_snapshot_bad("decimal encoding"))?;
    let value: Decimal = text
        .parse()
        .map_err(|_| iou_snapshot_bad("invalid decimal"))?;
    if value.to_string() != text {
        return Err(iou_snapshot_bad("noncanonical decimal"));
    }
    Ok(value)
}

fn iou_snapshot_account(
    reader: &mut crate::codec::Cursor<'_>,
) -> Result<AccountId, IouSnapshotError> {
    let length = reader
        .u8()
        .map_err(|_| iou_snapshot_bad("account length"))? as usize;
    if length == 0 || length > crate::ledger::MAX_ACCOUNT_BYTES {
        return Err(iou_snapshot_bad("account length exceeds limit"));
    }
    let raw = reader
        .take(length)
        .map_err(|_| iou_snapshot_bad("account truncated"))?;
    let text = core::str::from_utf8(raw).map_err(|_| iou_snapshot_bad("account encoding"))?;
    let account = AccountId::try_from(text).map_err(|_| iou_snapshot_bad("invalid account"))?;
    check_address(&account).map_err(IouSnapshotError::InvalidIou)?;
    Ok(account)
}

impl IouStore {
    fn verify_snapshot_state(&self) -> Result<(), IouSnapshotError> {
        if self.record_count() > MAX_IOU_RECORDS || self.claim_count > MAX_IOU_CLAIMS {
            return Err(iou_snapshot_bad("IOU record or claim limit"));
        }
        let mut total = 0_usize;
        for (&key, bucket) in &self.buckets {
            if key != bucket.key {
                return Err(iou_snapshot_bad("bucket key mismatch"));
            }
            check_block(bucket.last_block).map_err(IouSnapshotError::InvalidIou)?;
            if bucket.block_losses.is_negative() || bucket.block_profits.is_negative() {
                return Err(iou_snapshot_bad("negative loss/profit counter"));
            }
            if [
                &bucket.amount,
                &bucket.block_start_amount,
                &bucket.block_delta,
                &bucket.block_losses,
                &bucket.block_profits,
            ]
            .iter()
            .any(|field| field.decimal_places() > JS_DIVISION_PLACES)
            {
                return Err(iou_snapshot_bad(
                    "bucket precision exceeds source persistence",
                ));
            }
        }
        for (&key, map) in &self.claim_maps {
            if key != map.key {
                return Err(iou_snapshot_bad("claim map key mismatch"));
            }
            check_block(map.last_block).map_err(IouSnapshotError::InvalidIou)?;
            total = total
                .checked_add(map.claims.len())
                .ok_or_else(|| iou_snapshot_bad("claim count overflow"))?;
            if total > MAX_IOU_CLAIMS {
                return Err(iou_snapshot_bad("claim count exceeds limit"));
            }
            let mut seen = BTreeSet::new();
            let mut previous_index = None;
            let mut ordinary_key_seen = false;
            for claim in &map.claims {
                check_address(&claim.address).map_err(IouSnapshotError::InvalidIou)?;
                if !seen.insert(&claim.address) {
                    return Err(iou_snapshot_bad("duplicate claimant"));
                }
                match array_index(&claim.address) {
                    Some(index) => {
                        if ordinary_key_seen || previous_index.is_some_and(|last| index <= last) {
                            return Err(iou_snapshot_bad("noncanonical claim order"));
                        }
                        previous_index = Some(index);
                    }
                    None => ordinary_key_seen = true,
                }
            }
        }
        if total != self.claim_count {
            return Err(iou_snapshot_bad("claim count mismatch"));
        }
        Ok(())
    }

    /// Canonical local checkpoint, retaining signed/zero claims and all
    /// source bucket counters. It carries no network or funding authority.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, IouSnapshotError> {
        self.verify_snapshot_state()?;
        let mut out = IOU_SNAPSHOT_MAGIC.to_vec();
        iou_snapshot_append(&mut out, &(self.buckets.len() as u32).to_be_bytes())?;
        for bucket in self.buckets.values() {
            iou_snapshot_write_key(&mut out, bucket.key)?;
            iou_snapshot_append(&mut out, &bucket.last_block.to_be_bytes())?;
            for value in [
                &bucket.amount,
                &bucket.block_start_amount,
                &bucket.block_delta,
                &bucket.block_losses,
                &bucket.block_profits,
            ] {
                iou_snapshot_write_decimal(&mut out, value)?;
            }
        }
        iou_snapshot_append(&mut out, &(self.claim_maps.len() as u32).to_be_bytes())?;
        iou_snapshot_append(&mut out, &(self.claim_count as u32).to_be_bytes())?;
        for map in self.claim_maps.values() {
            iou_snapshot_write_key(&mut out, map.key)?;
            iou_snapshot_append(&mut out, &map.last_block.to_be_bytes())?;
            iou_snapshot_append(&mut out, &(map.claims.len() as u32).to_be_bytes())?;
            for claim in &map.claims {
                let raw = claim.address.as_str().as_bytes();
                iou_snapshot_append(&mut out, &[raw.len() as u8])?;
                iou_snapshot_append(&mut out, raw)?;
                iou_snapshot_write_decimal(&mut out, &claim.amount)?;
            }
        }
        crate::codec::with_crc(out).map_err(|_| iou_snapshot_bad("IOU snapshot too large"))
    }

    /// Decode canonical TLIO v1 only. Counts are checked before allocations;
    /// the total input, every field and reconstructed state are bounded.
    pub fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self, IouSnapshotError> {
        let raw =
            crate::codec::verify_crc(bytes).map_err(|_| iou_snapshot_bad("IOU CRC or size"))?;
        let mut reader = crate::codec::Cursor::new(raw);
        if reader
            .fixed::<5>()
            .map_err(|_| iou_snapshot_bad("IOU magic"))?
            != *IOU_SNAPSHOT_MAGIC
        {
            return Err(iou_snapshot_bad("IOU checkpoint version"));
        }
        let mut result = Self::new();
        let buckets = iou_snapshot_count(&mut reader, MAX_IOU_RECORDS)?;
        let mut previous = None;
        for _ in 0..buckets {
            let key = iou_snapshot_key(&mut reader)?;
            if previous.is_some_and(|last| key <= last) {
                return Err(iou_snapshot_bad("unsorted or duplicate bucket key"));
            }
            previous = Some(key);
            let bucket = Bucket {
                key,
                last_block: iou_snapshot_height(&mut reader)?,
                amount: iou_snapshot_decimal(&mut reader)?,
                block_start_amount: iou_snapshot_decimal(&mut reader)?,
                block_delta: iou_snapshot_decimal(&mut reader)?,
                block_losses: iou_snapshot_decimal(&mut reader)?,
                block_profits: iou_snapshot_decimal(&mut reader)?,
            };
            result.buckets.insert(key, bucket);
        }
        let maps = iou_snapshot_count(&mut reader, MAX_IOU_RECORDS - buckets)?;
        let expected_claims = iou_snapshot_count(&mut reader, MAX_IOU_CLAIMS)?;
        let mut previous = None;
        for _ in 0..maps {
            let key = iou_snapshot_key(&mut reader)?;
            if previous.is_some_and(|last| key <= last) {
                return Err(iou_snapshot_bad("unsorted or duplicate claim key"));
            }
            previous = Some(key);
            let last_block = iou_snapshot_height(&mut reader)?;
            let count = iou_snapshot_count(&mut reader, MAX_IOU_CLAIMS - result.claim_count)?;
            if result.claim_count + count > expected_claims {
                return Err(iou_snapshot_bad("claim count mismatch"));
            }
            // No capacity is reserved from an untrusted count; individual
            // complete, validated claims are appended within the total cap.
            let mut claims = Vec::new();
            for _ in 0..count {
                claims.push(Claim {
                    address: iou_snapshot_account(&mut reader)?,
                    amount: iou_snapshot_decimal(&mut reader)?,
                });
            }
            result.claim_count += count;
            result.claim_maps.insert(
                key,
                ClaimMap {
                    key,
                    last_block,
                    claims,
                },
            );
        }
        reader
            .end()
            .map_err(|_| iou_snapshot_bad("IOU trailing bytes"))?;
        if result.claim_count != expected_claims {
            return Err(iou_snapshot_bad("claim count mismatch"));
        }
        result.verify_snapshot_state()?;
        if result.snapshot_bytes()?.as_slice() != bytes {
            return Err(iou_snapshot_bad("noncanonical IOU checkpoint"));
        }
        Ok(result)
    }
}
