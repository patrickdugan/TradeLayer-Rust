// Pinned C++ field framing; proofs, keys and signatures remain unverified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedMwebInput {
    pub range: Range<usize>,
    pub features: u8,
    pub output_id: Range<usize>,
    pub commitment: Range<usize>,
    pub output_pubkey: Range<usize>,
    pub input_pubkey: Option<Range<usize>>,
    pub extra_data: Option<Range<usize>>,
    pub signature: Range<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedMwebOutput {
    pub range: Range<usize>,
    pub commitment: Range<usize>,
    pub sender_pubkey: Range<usize>,
    pub receiver_pubkey: Range<usize>,
    pub message: Range<usize>,
    pub features: u8,
    pub key_exchange_pubkey: Option<Range<usize>>,
    pub view_tag: Option<u8>,
    pub masked_value: Option<u64>,
    pub masked_nonce: Option<Range<usize>>,
    pub extra_data: Option<Range<usize>>,
    pub range_proof: Range<usize>,
    pub signature: Range<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedMwebPegout {
    pub amount: u64,
    pub script: Range<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedMwebKernel {
    pub range: Range<usize>,
    pub features: u8,
    pub fee: Option<u64>,
    pub pegin: Option<u64>,
    pub pegouts: Vec<UnverifiedMwebPegout>,
    pub lock_height: Option<u32>,
    pub stealth_excess: Option<Range<usize>>,
    pub extra_data: Option<Range<usize>>,
    pub excess: Range<usize>,
    pub signature: Range<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedMwebBody {
    pub range: Range<usize>,
    pub inputs: Vec<UnverifiedMwebInput>,
    pub outputs: Vec<UnverifiedMwebOutput>,
    pub kernels: Vec<UnverifiedMwebKernel>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedMwebTransaction {
    pub range: Range<usize>,
    pub kernel_offset: Range<usize>,
    pub stealth_offset: Range<usize>,
    pub body: UnverifiedMwebBody,
}
fn mweb_span(r: &mut Cursor<'_>, size: usize) -> Result<Range<usize>> {
    let start = r.offset;
    r.read(size)?;
    Ok(start..r.offset)
}
fn mweb_bytes(r: &mut Cursor<'_>) -> Result<Range<usize>> {
    let b = r.variable_bytes()?;
    Ok(r.offset - b.len()..r.offset)
}
fn mweb_count(r: &mut Cursor<'_>) -> Result<usize> {
    let n = r.varint()?;
    if n > MAX_IO_COUNT {
        return Err(ScanError::TooLong.into());
    }
    Ok(n as usize)
}
fn mweb_features(r: &mut Cursor<'_>, mask: u8, object: &'static str) -> Result<u8> {
    let features = r.byte()?;
    if features & !mask != 0 {
        return Err(LitecoinTransparentError::UnsupportedMwebFeatures { object, features });
    }
    Ok(features)
}
fn mweb_body(r: &mut Cursor<'_>) -> Result<UnverifiedMwebBody> {
    let start = r.offset;
    let mut inputs = vec![];
    for _ in 0..mweb_count(r)? {
        let start = r.offset;
        let features = mweb_features(r, 3, "input")?;
        let output_id = mweb_span(r, 32)?;
        let commitment = mweb_span(r, 33)?;
        let output_pubkey = mweb_span(r, 33)?;
        let input_pubkey = if features & 1 != 0 {
            Some(mweb_span(r, 33)?)
        } else {
            None
        };
        let extra_data = if features & 2 != 0 {
            Some(mweb_bytes(r)?)
        } else {
            None
        };
        let signature = mweb_span(r, 64)?;
        inputs.push(UnverifiedMwebInput {
            range: start..r.offset,
            features,
            output_id,
            commitment,
            output_pubkey,
            input_pubkey,
            extra_data,
            signature,
        });
    }
    let mut outputs = vec![];
    for _ in 0..mweb_count(r)? {
        let start = r.offset;
        let commitment = mweb_span(r, 33)?;
        let sender_pubkey = mweb_span(r, 33)?;
        let receiver_pubkey = mweb_span(r, 33)?;
        let message_start = r.offset;
        let features = mweb_features(r, 3, "output-message")?;
        let (key_exchange_pubkey, view_tag, masked_value, masked_nonce) = if features & 1 != 0 {
            (
                Some(mweb_span(r, 33)?),
                Some(r.byte()?),
                Some(u64::from_le_bytes(r.read(8)?.try_into().expect("8 bytes"))),
                Some(mweb_span(r, 16)?),
            )
        } else {
            (None, None, None, None)
        };
        let extra_data = if features & 2 != 0 {
            Some(mweb_bytes(r)?)
        } else {
            None
        };
        let message = message_start..r.offset;
        let range_proof = mweb_span(r, 675)?;
        let signature = mweb_span(r, 64)?;
        outputs.push(UnverifiedMwebOutput {
            range: start..r.offset,
            commitment,
            sender_pubkey,
            receiver_pubkey,
            message,
            features,
            key_exchange_pubkey,
            view_tag,
            masked_value,
            masked_nonce,
            extra_data,
            range_proof,
            signature,
        });
    }
    let mut kernels = vec![];
    for _ in 0..mweb_count(r)? {
        let start = r.offset;
        let features = mweb_features(r, 63, "kernel")?;
        let fee = if features & 1 != 0 {
            Some(litecoin_header_varint(r, i64::MAX as u64)?)
        } else {
            None
        };
        let pegin = if features & 2 != 0 {
            Some(litecoin_header_varint(r, i64::MAX as u64)?)
        } else {
            None
        };
        let mut pegouts = vec![];
        if features & 4 != 0 {
            for _ in 0..mweb_count(r)? {
                let amount = litecoin_header_varint(r, i64::MAX as u64)?;
                let script = mweb_bytes(r)?;
                if script.is_empty() {
                    return Err(LitecoinTransparentError::InvalidMwebTransaction);
                }
                pegouts.push(UnverifiedMwebPegout { amount, script });
            }
        }
        let lock_height = if features & 8 != 0 {
            Some(litecoin_header_varint(r, i32::MAX as u64)? as u32)
        } else {
            None
        };
        let stealth_excess = if features & 16 != 0 {
            Some(mweb_span(r, 33)?)
        } else {
            None
        };
        let extra_data = if features & 32 != 0 {
            Some(mweb_bytes(r)?)
        } else {
            None
        };
        let excess = mweb_span(r, 33)?;
        let signature = mweb_span(r, 64)?;
        kernels.push(UnverifiedMwebKernel {
            range: start..r.offset,
            features,
            fee,
            pegin,
            pegouts,
            lock_height,
            stealth_excess,
            extra_data,
            excess,
            signature,
        });
    }
    Ok(UnverifiedMwebBody {
        range: start..r.offset,
        inputs,
        outputs,
        kernels,
    })
}
fn mweb_transaction(r: &mut Cursor<'_>) -> Result<UnverifiedMwebTransaction> {
    let start = r.offset;
    let kernel_offset = mweb_span(r, 32)?;
    let stealth_offset = mweb_span(r, 32)?;
    let body = mweb_body(r)?;
    if body.kernels.is_empty() {
        return Err(LitecoinTransparentError::InvalidMwebTransaction);
    }
    Ok(UnverifiedMwebTransaction {
        range: start..r.offset,
        kernel_offset,
        stealth_offset,
        body,
    })
}
fn mweb_write_size(out: &mut Vec<u8>, n: usize) {
    let n = n as u64;
    match n {
        0..=252 => out.push(n as u8),
        253..=65535 => {
            out.push(253);
            out.extend((n as u16).to_le_bytes());
        }
        65536..=4294967295 => {
            out.push(254);
            out.extend((n as u32).to_le_bytes());
        }
        _ => {
            out.push(255);
            out.extend(n.to_le_bytes());
        }
    }
}
fn mweb_write_varint(out: &mut Vec<u8>, mut n: u64) {
    let mut bytes = vec![(n & 127) as u8];
    while n > 127 {
        n = (n >> 7) - 1;
        bytes.push(((n & 127) as u8) | 128);
    }
    out.extend(bytes.iter().rev());
}
fn mweb_copy(out: &mut Vec<u8>, raw: &[u8], range: &Range<usize>) -> Result<()> {
    out.extend(raw.get(range.clone()).ok_or(ScanError::Truncated)?);
    Ok(())
}
impl UnverifiedMwebKernel {
    /// Kernel::Serialize omits empty pegout/extra vectors despite set bits.
    pub fn canonical_bytes(&self, raw: &[u8]) -> Result<Vec<u8>> {
        let mut out = vec![self.features];
        if let Some(n) = self.fee {
            mweb_write_varint(&mut out, n);
        }
        if let Some(n) = self.pegin {
            mweb_write_varint(&mut out, n);
        }
        if !self.pegouts.is_empty() {
            mweb_write_size(&mut out, self.pegouts.len());
            for p in &self.pegouts {
                mweb_write_varint(&mut out, p.amount);
                mweb_write_size(&mut out, p.script.len());
                mweb_copy(&mut out, raw, &p.script)?;
            }
        }
        if let Some(n) = self.lock_height {
            mweb_write_varint(&mut out, u64::from(n));
        }
        if let Some(range) = &self.stealth_excess {
            mweb_copy(&mut out, raw, range)?;
        }
        if let Some(range) = &self.extra_data {
            if !range.is_empty() {
                mweb_write_size(&mut out, range.len());
                mweb_copy(&mut out, raw, range)?;
            }
        }
        mweb_copy(&mut out, raw, &self.excess)?;
        mweb_copy(&mut out, raw, &self.signature)?;
        Ok(out)
    }
    pub fn kernel_id_raw(&self, raw: &[u8]) -> Result<[u8; 32]> {
        Ok(crate::blake3::blake3_256(&self.canonical_bytes(raw)?))
    }
}
impl UnverifiedMwebBody {
    pub fn canonical_bytes(&self, raw: &[u8]) -> Result<Vec<u8>> {
        let mut out = vec![];
        mweb_write_size(&mut out, self.inputs.len());
        for x in &self.inputs {
            mweb_copy(&mut out, raw, &x.range)?;
        }
        mweb_write_size(&mut out, self.outputs.len());
        for x in &self.outputs {
            mweb_copy(&mut out, raw, &x.range)?;
        }
        mweb_write_size(&mut out, self.kernels.len());
        for x in &self.kernels {
            out.extend(x.canonical_bytes(raw)?);
        }
        Ok(out)
    }
}
impl UnverifiedMwebTransaction {
    pub fn canonical_bytes(&self, raw: &[u8]) -> Result<Vec<u8>> {
        let mut out = vec![];
        mweb_copy(&mut out, raw, &self.kernel_offset)?;
        mweb_copy(&mut out, raw, &self.stealth_offset)?;
        out.extend(self.body.canonical_bytes(raw)?);
        Ok(out)
    }
}
impl UnverifiedMwebOutput {
    pub fn output_id_raw(&self, raw: &[u8]) -> Result<[u8; 32]> {
        let message = raw.get(self.message.clone()).ok_or(ScanError::Truncated)?;
        let proof = raw
            .get(self.range_proof.clone())
            .ok_or(ScanError::Truncated)?;
        let mut preimage = vec![];
        for field in [&self.commitment, &self.sender_pubkey, &self.receiver_pubkey] {
            mweb_copy(&mut preimage, raw, field)?;
        }
        preimage.extend(crate::blake3::blake3_256(message));
        preimage.extend(crate::blake3::blake3_256(proof));
        mweb_copy(&mut preimage, raw, &self.signature)?;
        Ok(crate::blake3::blake3_256(&preimage))
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnverifiedLitecoinTransactionIdentity {
    TransparentSha256d {
        txid: [u8; 32],
        wtxid: [u8; 32],
    },
    /// Natural kernel bytes; display TXID reverses them exactly as uint256.
    MwebFirstKernelBlake3 {
        txid: [u8; 32],
        kernel_id: [u8; 32],
    },
}
#[derive(Debug, Clone)]
pub struct UnverifiedLitecoinTransactionEnvelope<'a> {
    inner: UnverifiedLitecoinTransparentTransaction<'a>,
}
impl<'a> UnverifiedLitecoinTransactionEnvelope<'a> {
    pub fn parse(raw: &'a [u8], profile: LitecoinTransparentProfile) -> Result<Self> {
        if raw.len() > MAX_RAW_TRANSACTION {
            return Err(ScanError::TooLong.into());
        }
        let inner = litecoin_envelope_prefix(raw, profile)?;
        if inner.raw.len() != raw.len() {
            return Err(LitecoinTransparentError::TrailingBytes);
        }
        Ok(Self { inner })
    }
    pub fn raw_bytes(&self) -> &'a [u8] {
        self.inner.raw
    }
    pub fn transparent(&self) -> Option<&UnverifiedLitecoinTransparentTransaction<'a>> {
        if self.inner.is_mweb_only() {
            None
        } else {
            Some(&self.inner)
        }
    }
    pub fn mweb_transaction(&self) -> Option<&UnverifiedMwebTransaction> {
        self.inner.mweb.as_ref()
    }
    pub fn identity(&self) -> Result<UnverifiedLitecoinTransactionIdentity> {
        if self.inner.is_mweb_only() {
            let m = self
                .inner
                .mweb
                .as_ref()
                .ok_or(LitecoinTransparentError::InvalidMwebTransaction)?;
            let id = m
                .body
                .kernels
                .first()
                .ok_or(LitecoinTransparentError::InvalidMwebTransaction)?
                .kernel_id_raw(self.inner.raw)?;
            Ok(
                UnverifiedLitecoinTransactionIdentity::MwebFirstKernelBlake3 {
                    txid: id,
                    kernel_id: id,
                },
            )
        } else {
            let o = self.inner.inspect();
            Ok(UnverifiedLitecoinTransactionIdentity::TransparentSha256d {
                txid: o.txid,
                wtxid: o.wtxid,
            })
        }
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.inner.canonical_bytes()
    }
}
