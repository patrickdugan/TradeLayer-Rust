// TLFI v1 contains one complete TLL ledger and one complete TLIO store.
// CRCs detect accidental damage; they neither authenticate the snapshot nor
// prove the caller's network, actor, loss, match or chain-height assertions.

const FUNDED_IOU_SNAPSHOT_MAGIC: &[u8; 5] = b"TLFI\x01";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FundedIouSnapshotError {
    InvalidFormat(&'static str),
    Iou(crate::iou::IouSnapshotError),
    Ledger(crate::ledger::SnapshotError),
    InvalidState(FundedIouError),
}
impl core::fmt::Display for FundedIouSnapshotError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FundedIouSnapshotError {}

fn funded_iou_snapshot_bad(reason: &'static str) -> FundedIouSnapshotError {
    FundedIouSnapshotError::InvalidFormat(reason)
}

fn funded_iou_snapshot_append(
    out: &mut Vec<u8>,
    data: &[u8],
) -> Result<(), FundedIouSnapshotError> {
    if out.len().saturating_add(data.len()) > crate::codec::MAX_CHECKPOINT_BYTES - 4 {
        return Err(funded_iou_snapshot_bad("funded IOU snapshot too large"));
    }
    out.extend_from_slice(data);
    Ok(())
}

fn funded_iou_snapshot_embedded<'a>(
    reader: &mut crate::codec::Cursor<'a>,
) -> Result<&'a [u8], FundedIouSnapshotError> {
    let count = reader
        .u32()
        .map_err(|_| funded_iou_snapshot_bad("embedded length"))? as usize;
    if count == 0 || count > crate::codec::MAX_CHECKPOINT_BYTES {
        return Err(funded_iou_snapshot_bad("embedded length exceeds limit"));
    }
    reader
        .take(count)
        .map_err(|_| funded_iou_snapshot_bad("embedded snapshot truncated"))
}

fn funded_iou_snapshot_count(
    reader: &mut crate::codec::Cursor<'_>,
    maximum: usize,
) -> Result<usize, FundedIouSnapshotError> {
    let count = reader
        .u32()
        .map_err(|_| funded_iou_snapshot_bad("count truncated"))? as usize;
    if count > maximum {
        return Err(funded_iou_snapshot_bad("count exceeds funded IOU limit"));
    }
    Ok(count)
}

fn funded_iou_snapshot_account(
    reader: &mut crate::codec::Cursor<'_>,
) -> Result<AccountId, FundedIouSnapshotError> {
    let length = reader
        .u8()
        .map_err(|_| funded_iou_snapshot_bad("escrow account length"))? as usize;
    if length == 0 || length > crate::ledger::MAX_ACCOUNT_BYTES {
        return Err(funded_iou_snapshot_bad(
            "escrow account length exceeds limit",
        ));
    }
    let raw = reader
        .take(length)
        .map_err(|_| funded_iou_snapshot_bad("escrow account truncated"))?;
    let text = core::str::from_utf8(raw)
        .map_err(|_| funded_iou_snapshot_bad("escrow account encoding"))?;
    AccountId::try_from(text).map_err(|_| funded_iou_snapshot_bad("invalid escrow account"))
}

impl FundedIouLedger {
    fn verify_snapshot_state(&self) -> Result<(), FundedIouSnapshotError> {
        self.verify()
            .map_err(FundedIouSnapshotError::InvalidState)?;
        if self.inputs.len() > MAX_FUNDED_IOU_INPUTS
            || self.escrows.len() > crate::iou::MAX_IOU_RECORDS
        {
            return Err(funded_iou_snapshot_bad("funded IOU count exceeds limit"));
        }
        if !self
            .inputs
            .is_disjoint(self.ledger.input_txids_unverified())
        {
            return Err(funded_iou_snapshot_bad(
                "outer and ledger replay IDs overlap",
            ));
        }
        let source_height = self
            .ious
            .buckets()
            .map(|row| row.last_block)
            .chain(self.ious.claim_maps().map(|row| row.last_block))
            .max();
        if self
            .last_height
            .is_some_and(|height| height > MAX_JS_SAFE_BLOCK)
            || (!self.inputs.is_empty() && self.last_height.is_none())
            || (self.inputs.is_empty() && self.last_height != source_height)
        {
            return Err(funded_iou_snapshot_bad("inconsistent last height"));
        }
        for key in self.escrows.keys() {
            if self.ious.bucket(*key).is_none() {
                return Err(funded_iou_snapshot_bad("escrow has no source bucket"));
            }
        }
        Ok(())
    }

    /// A complete, canonical local checkpoint. The embedded Ledger format
    /// retains channel and pending/settled withdrawal liabilities as well as
    /// tokens and replay markers. No ledger is reconstructed from balances.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, FundedIouSnapshotError> {
        self.verify_snapshot_state()?;
        let mut out = FUNDED_IOU_SNAPSHOT_MAGIC.to_vec();
        funded_iou_snapshot_append(&mut out, &[self.network.tag()])?;
        funded_iou_snapshot_append(&mut out, &self.network.genesis().0)?;
        funded_iou_snapshot_append(
            &mut out,
            &[match self.atom_policy {
                PayoutAtomPolicy::Exact => 1,
                PayoutAtomPolicy::Down => 2,
                PayoutAtomPolicy::HalfUp => 3,
            }],
        )?;
        match self.last_height {
            None => funded_iou_snapshot_append(&mut out, &[0])?,
            Some(height) => {
                funded_iou_snapshot_append(&mut out, &[1])?;
                funded_iou_snapshot_append(&mut out, &height.to_be_bytes())?;
            }
        }
        for embedded in [
            self.ledger
                .snapshot_bytes()
                .map_err(FundedIouSnapshotError::Ledger)?,
            self.ious
                .snapshot_bytes()
                .map_err(FundedIouSnapshotError::Iou)?,
        ] {
            funded_iou_snapshot_append(&mut out, &(embedded.len() as u32).to_be_bytes())?;
            funded_iou_snapshot_append(&mut out, &embedded)?;
        }
        funded_iou_snapshot_append(&mut out, &(self.escrows.len() as u32).to_be_bytes())?;
        for (key, account) in &self.escrows {
            funded_iou_snapshot_append(&mut out, &key.contract_id.to_be_bytes())?;
            funded_iou_snapshot_append(&mut out, &key.property_id.to_be_bytes())?;
            let raw = account.as_str().as_bytes();
            funded_iou_snapshot_append(&mut out, &[raw.len() as u8])?;
            funded_iou_snapshot_append(&mut out, raw)?;
        }
        funded_iou_snapshot_append(&mut out, &(self.inputs.len() as u32).to_be_bytes())?;
        for id in &self.inputs {
            funded_iou_snapshot_append(&mut out, &id.as_bytes())?;
        }
        crate::codec::with_crc(out)
            .map_err(|_| funded_iou_snapshot_bad("funded IOU snapshot too large"))
    }

    /// Restore TLFI v1 only when independent deployment configuration agrees
    /// with its network/genesis and local decimal-to-atom payout policy.
    pub fn from_snapshot_bytes(
        bytes: &[u8],
        expected_network: TradeLayerMainnet,
        expected_atom_policy: PayoutAtomPolicy,
    ) -> Result<Self, FundedIouSnapshotError> {
        let raw = crate::codec::verify_crc(bytes)
            .map_err(|_| funded_iou_snapshot_bad("funded IOU CRC or size"))?;
        let mut reader = crate::codec::Cursor::new(raw);
        if reader
            .fixed::<5>()
            .map_err(|_| funded_iou_snapshot_bad("funded IOU magic"))?
            != *FUNDED_IOU_SNAPSHOT_MAGIC
        {
            return Err(funded_iou_snapshot_bad("funded IOU checkpoint version"));
        }
        let network = TradeLayerMainnet::from_tag(
            reader
                .u8()
                .map_err(|_| funded_iou_snapshot_bad("network tag"))?,
        )
        .map_err(|_| funded_iou_snapshot_bad("invalid network tag"))?;
        let genesis = reader
            .fixed::<32>()
            .map_err(|_| funded_iou_snapshot_bad("genesis truncated"))?;
        if network != expected_network || genesis != expected_network.genesis().0 {
            return Err(funded_iou_snapshot_bad("wrong network or genesis"));
        }
        let atom_policy = match reader
            .u8()
            .map_err(|_| funded_iou_snapshot_bad("atom policy tag"))?
        {
            1 => PayoutAtomPolicy::Exact,
            2 => PayoutAtomPolicy::Down,
            3 => PayoutAtomPolicy::HalfUp,
            _ => return Err(funded_iou_snapshot_bad("invalid atom policy tag")),
        };
        if atom_policy != expected_atom_policy {
            return Err(funded_iou_snapshot_bad("wrong atom policy"));
        }
        let last_height = match reader
            .u8()
            .map_err(|_| funded_iou_snapshot_bad("height flag"))?
        {
            0 => None,
            1 => Some(
                reader
                    .u64()
                    .map_err(|_| funded_iou_snapshot_bad("height truncated"))?,
            ),
            _ => return Err(funded_iou_snapshot_bad("invalid height flag")),
        };
        let ledger = Ledger::from_snapshot_bytes(funded_iou_snapshot_embedded(&mut reader)?)
            .map_err(FundedIouSnapshotError::Ledger)?;
        let ious = IouStore::from_snapshot_bytes(funded_iou_snapshot_embedded(&mut reader)?)
            .map_err(FundedIouSnapshotError::Iou)?;
        let count = funded_iou_snapshot_count(&mut reader, crate::iou::MAX_IOU_RECORDS)?;
        let mut escrows = BTreeMap::new();
        let mut previous = None;
        for _ in 0..count {
            let key = IouKey::new(
                reader
                    .u32()
                    .map_err(|_| funded_iou_snapshot_bad("escrow key"))?,
                reader
                    .u32()
                    .map_err(|_| funded_iou_snapshot_bad("escrow key"))?,
            );
            if previous.is_some_and(|last| key <= last) {
                return Err(funded_iou_snapshot_bad("unsorted or duplicate escrow key"));
            }
            previous = Some(key);
            escrows.insert(key, funded_iou_snapshot_account(&mut reader)?);
        }
        let count = funded_iou_snapshot_count(&mut reader, MAX_FUNDED_IOU_INPUTS)?;
        let mut inputs = BTreeSet::new();
        let mut previous = None;
        for _ in 0..count {
            let id = TxId::from_bytes(
                reader
                    .fixed::<32>()
                    .map_err(|_| funded_iou_snapshot_bad("replay ID truncated"))?,
            );
            if previous.is_some_and(|last| id <= last) {
                return Err(funded_iou_snapshot_bad(
                    "unsorted or duplicate outer replay ID",
                ));
            }
            previous = Some(id);
            inputs.insert(id);
        }
        reader
            .end()
            .map_err(|_| funded_iou_snapshot_bad("funded IOU trailing bytes"))?;
        let result = Self {
            network,
            atom_policy,
            ledger,
            ious,
            escrows,
            inputs,
            last_height,
        };
        result.verify_snapshot_state()?;
        if result.snapshot_bytes()?.as_slice() != bytes {
            return Err(funded_iou_snapshot_bad(
                "noncanonical funded IOU checkpoint",
            ));
        }
        Ok(result)
    }
}
