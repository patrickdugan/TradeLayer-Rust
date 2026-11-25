//! Canonical Receipt/v1 wire types, tagged SHA-256 digests, and PublicEpochManifest.
//!
//! Preserves byte-for-byte parity with `research/tap-vwap/receipt/receipt.cjs`,
//! `research/tap-vwap/receipt/independent_encoder.py`, and `tapvwapadapter`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Exact byte length of `TradeLayer/TAP-VWAP/Receipt/v1`.
pub const RECEIPT_V1_BYTES: usize = 526;

/// Exact byte length of `TradeLayer/TAP-VWAP/AssetHTLC/v1` payload.
pub const ASSET_HTLC_V1_BYTES: usize = 77;

/// Experimental wire version.
pub const RECEIPT_VERSION_V1: u8 = 1;

/// Evidence tier 1: three-party BIP-340 signed assertions (NOT channel-state proof).
pub const EVIDENCE_TIER_1_SIGNED_ASSERTION: u8 = 1;

/// Domain separation tag for Receipt/v1.
pub const TAG_RECEIPT_V1: &[u8] = b"TradeLayer/TAP-VWAP/Receipt/v1";

/// Domain separation tag for AssetHTLC/v1 canonical nullifier.
pub const TAG_ASSET_HTLC_V1: &[u8] = b"TradeLayer/TAP-VWAP/AssetHTLC/v1";

/// Domain separation tag for BIP-340 Schnorr challenge.
pub const TAG_BIP0340_CHALLENGE: &[u8] = b"BIP0340/challenge";

/// Compute BIP-340 style tagged SHA-256: `SHA256(SHA256(tag) || SHA256(tag) || msg)`.
pub fn tagged_sha256(tag: &[u8], msg: &[u8]) -> [u8; 32] {
    let tag_hash = Sha256::digest(tag);
    let mut h = Sha256::new();
    h.update(tag_hash);
    h.update(tag_hash);
    h.update(msg);
    h.finalize().into()
}

/// Canonical 36-byte Bitcoin outpoint (32-byte display-order txid || u32 BE vout).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CanonicalOutpoint {
    /// Display-order (RPC big-endian) funding txid.
    pub txid_display: [u8; 32],
    /// Output index (big-endian on wire).
    pub vout: u32,
}

impl CanonicalOutpoint {
    /// Encode to 36 canonical wire bytes. Rejects null/unresolved outpoints.
    pub fn to_bytes(&self) -> Result<[u8; 36], String> {
        if self.txid_display == [0u8; 32] {
            return Err("unresolved/zero funding outpoint txid".to_string());
        }
        if self.vout == u32::MAX {
            return Err("unresolved funding outpoint vout 0xffffffff".to_string());
        }
        let mut out = [0u8; 36];
        out[..32].copy_from_slice(&self.txid_display);
        out[32..36].copy_from_slice(&self.vout.to_be_bytes());
        Ok(out)
    }

    /// Decode from 36 canonical wire bytes, rejecting unresolved outpoints.
    pub fn from_bytes(bytes: &[u8; 36]) -> Result<Self, String> {
        let mut txid_display = [0u8; 32];
        txid_display.copy_from_slice(&bytes[..32]);
        let vout = u32::from_be_bytes([bytes[32], bytes[33], bytes[34], bytes[35]]);
        let op = Self { txid_display, vout };
        let _ = op.to_bytes()?;
        Ok(op)
    }
}

/// Decoded `TradeLayer/TAP-VWAP/Receipt/v1` record (526 bytes on wire).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptV1 {
    /// Chain genesis hash (32B).
    pub genesis: [u8; 32],
    /// Epoch/index policy hash (32B).
    pub policy: [u8; 32],
    /// Taproot Asset ID (32B).
    pub asset: [u8; 32],
    /// Edge receipt signing x-only pubkey (32B).
    pub edge_key: [u8; 32],
    /// BTC-leg peer receipt signing x-only pubkey (32B).
    pub btc_peer_key: [u8; 32],
    /// Asset-leg peer receipt signing x-only pubkey (32B).
    pub asset_peer_key: [u8; 32],
    /// Full 32-byte RFQ session ID (never truncated SCID).
    pub rfq_id: [u8; 32],
    /// 2-party signed QuoteTranscriptV1 digest (32B).
    pub quote_transcript: [u8; 32],
    /// Lightning payment hash (32B).
    pub payment_hash: [u8; 32],
    /// Asset channel pre-HTLC state summary (32B).
    pub asset_pre: [u8; 32],
    /// Asset channel post-removal state summary (32B).
    pub asset_post: [u8; 32],
    /// BTC channel pre-HTLC state summary (32B).
    pub btc_pre: [u8; 32],
    /// BTC channel post-removal state summary (32B).
    pub btc_post: [u8; 32],
    /// 0 = purchase by edge (asset in, BTC out); 1 = sale by edge (BTC in, asset out).
    pub direction: u8,
    /// Edge's immutable BOLT-2 node-pubkey side on asset channel (0 or 1).
    pub asset_edge_side: u8,
    /// Edge's immutable BOLT-2 node-pubkey side on BTC channel (0 or 1).
    pub btc_edge_side: u8,
    /// Canonical asset-channel funding outpoint (36B).
    pub asset_outpoint: CanonicalOutpoint,
    /// Immutable BOLT-2 side of the peer offering the asset HTLC (0 or 1).
    pub asset_offerer: u8,
    /// Asset-channel local HTLC ID (u64).
    pub asset_htlc: u64,
    /// Canonical BTC-channel funding outpoint (36B).
    pub btc_outpoint: CanonicalOutpoint,
    /// Immutable BOLT-2 side of the peer offering the BTC HTLC (0 or 1).
    pub btc_offerer: u8,
    /// BTC-channel local HTLC ID (u64).
    pub btc_htlc: u64,
    /// Positive settled asset atoms delta (> 0).
    pub asset_atoms: u64,
    /// Positive settled gross BTC leg delta in millisatoshis (> 0).
    pub gross_btc_msat: u64,
}

impl ReceiptV1 {
    /// Validate structural invariants and encode to exact 526-byte wire representation.
    pub fn encode(&self) -> Result<[u8; RECEIPT_V1_BYTES], String> {
        if self.direction > 1 {
            return Err(format!("invalid direction {}", self.direction));
        }
        if self.asset_edge_side > 1
            || self.btc_edge_side > 1
            || self.asset_offerer > 1
            || self.btc_offerer > 1
        {
            return Err("invalid side/offerer bit (>1)".to_string());
        }
        if self.asset_atoms == 0 {
            return Err("asset_atoms must be > 0".to_string());
        }
        if self.gross_btc_msat == 0 {
            return Err("gross_btc_msat must be > 0".to_string());
        }
        let asset_op_bytes = self.asset_outpoint.to_bytes()?;
        let btc_op_bytes = self.btc_outpoint.to_bytes()?;
        if asset_op_bytes == btc_op_bytes {
            return Err("asset and BTC outpoints must be distinct".to_string());
        }
        if self.asset_pre == self.asset_post {
            return Err("asset pre/post commitments must differ".to_string());
        }
        if self.btc_pre == self.btc_post {
            return Err("BTC pre/post commitments must differ".to_string());
        }
        if self.edge_key == self.btc_peer_key
            || self.edge_key == self.asset_peer_key
            || self.btc_peer_key == self.asset_peer_key
        {
            return Err("three receipt signer keys must be distinct".to_string());
        }
        if self.direction == 0 {
            if self.asset_offerer != 1 - self.asset_edge_side {
                return Err("purchase requires asset_offerer == 1 - asset_edge_side".to_string());
            }
            if self.btc_offerer != self.btc_edge_side {
                return Err("purchase requires btc_offerer == btc_edge_side".to_string());
            }
        } else {
            if self.asset_offerer != self.asset_edge_side {
                return Err("sale requires asset_offerer == asset_edge_side".to_string());
            }
            if self.btc_offerer != 1 - self.btc_edge_side {
                return Err("sale requires btc_offerer == 1 - btc_edge_side".to_string());
            }
        }

        let mut out = [0u8; RECEIPT_V1_BYTES];
        let mut o = 0usize;
        out[o] = RECEIPT_VERSION_V1;
        o += 1;
        for f in [
            &self.genesis,
            &self.policy,
            &self.asset,
            &self.edge_key,
            &self.btc_peer_key,
            &self.asset_peer_key,
            &self.rfq_id,
            &self.quote_transcript,
            &self.payment_hash,
            &self.asset_pre,
            &self.asset_post,
            &self.btc_pre,
            &self.btc_post,
        ] {
            out[o..o + 32].copy_from_slice(f);
            o += 32;
        }
        out[o] = self.direction;
        out[o + 1] = self.asset_edge_side;
        out[o + 2] = self.btc_edge_side;
        o += 3;

        out[o..o + 36].copy_from_slice(&asset_op_bytes);
        o += 36;
        out[o] = self.asset_offerer;
        o += 1;
        out[o..o + 8].copy_from_slice(&self.asset_htlc.to_be_bytes());
        o += 8;

        out[o..o + 36].copy_from_slice(&btc_op_bytes);
        o += 36;
        out[o] = self.btc_offerer;
        o += 1;
        out[o..o + 8].copy_from_slice(&self.btc_htlc.to_be_bytes());
        o += 8;

        out[o..o + 8].copy_from_slice(&self.asset_atoms.to_be_bytes());
        o += 8;
        out[o..o + 8].copy_from_slice(&self.gross_btc_msat.to_be_bytes());
        o += 8;
        debug_assert_eq!(o, RECEIPT_V1_BYTES);
        Ok(out)
    }

    /// Decode and validate a 526-byte Receipt/v1 wire buffer.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != RECEIPT_V1_BYTES {
            return Err(format!(
                "expected {} receipt bytes, got {}",
                RECEIPT_V1_BYTES,
                bytes.len()
            ));
        }
        if bytes[0] != RECEIPT_VERSION_V1 {
            return Err(format!("unsupported receipt version {}", bytes[0]));
        }
        let read32 = |off: usize| -> [u8; 32] {
            let mut a = [0u8; 32];
            a.copy_from_slice(&bytes[off..off + 32]);
            a
        };
        let read36 = |off: usize| -> [u8; 36] {
            let mut a = [0u8; 36];
            a.copy_from_slice(&bytes[off..off + 36]);
            a
        };
        let read_u64 = |off: usize| -> u64 {
            let mut a = [0u8; 8];
            a.copy_from_slice(&bytes[off..off + 8]);
            u64::from_be_bytes(a)
        };

        let rec = Self {
            genesis: read32(1),
            policy: read32(33),
            asset: read32(65),
            edge_key: read32(97),
            btc_peer_key: read32(129),
            asset_peer_key: read32(161),
            rfq_id: read32(193),
            quote_transcript: read32(225),
            payment_hash: read32(257),
            asset_pre: read32(289),
            asset_post: read32(321),
            btc_pre: read32(353),
            btc_post: read32(385),
            direction: bytes[417],
            asset_edge_side: bytes[418],
            btc_edge_side: bytes[419],
            asset_outpoint: CanonicalOutpoint::from_bytes(&read36(420))?,
            asset_offerer: bytes[456],
            asset_htlc: read_u64(457),
            btc_outpoint: CanonicalOutpoint::from_bytes(&read36(465))?,
            btc_offerer: bytes[501],
            btc_htlc: read_u64(502),
            asset_atoms: read_u64(510),
            gross_btc_msat: read_u64(518),
        };
        // Re-run full invariant validation.
        let _ = rec.encode()?;
        Ok(rec)
    }

    /// Tagged SHA-256 conversion digest (`TradeLayer/TAP-VWAP/Receipt/v1`).
    pub fn digest(&self) -> Result<[u8; 32], String> {
        let raw = self.encode()?;
        Ok(tagged_sha256(TAG_RECEIPT_V1, &raw))
    }

    /// Exact 77-byte canonical nullifier payload (`genesis || asset_outpoint || asset_offerer || asset_htlc`).
    pub fn nullifier_payload(&self) -> Result<[u8; ASSET_HTLC_V1_BYTES], String> {
        let _ = self.encode()?;
        let op = self.asset_outpoint.to_bytes()?;
        let mut buf = [0u8; ASSET_HTLC_V1_BYTES];
        buf[0..32].copy_from_slice(&self.genesis);
        buf[32..68].copy_from_slice(&op);
        buf[68] = self.asset_offerer;
        buf[69..77].copy_from_slice(&self.asset_htlc.to_be_bytes());
        Ok(buf)
    }

    /// Global cross-epoch canonical asset-HTLC nullifier (`TradeLayer/TAP-VWAP/AssetHTLC/v1`).
    pub fn nullifier(&self) -> Result<[u8; 32], String> {
        let payload = self.nullifier_payload()?;
        Ok(tagged_sha256(TAG_ASSET_HTLC_V1, &payload))
    }
}

/// Three 64-byte BIP-340 Schnorr signatures over `ReceiptV1::digest()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreePartySignatures {
    /// Edge signature (64B).
    pub edge: [u8; 64],
    /// BTC-leg peer signature (64B).
    pub btc_peer: [u8; 64],
    /// Asset-leg peer signature (64B).
    pub asset_peer: [u8; 64],
}

/// Exact half-up rounded q8 VWAP price:
/// `floor_half_up(10^(19 - d) * sum_asset_atoms / sum_btc_msat)`.
pub fn compute_vwap_q8_half_up(
    sum_asset_atoms: u128,
    sum_btc_msat: u128,
    asset_decimals: u8,
) -> Result<(u128, u128, u128, bool), String> {
    if sum_btc_msat == 0 || sum_asset_atoms == 0 {
        return Err("VWAP sums must be strictly positive".to_string());
    }
    if asset_decimals > 19 {
        return Err(format!("unsupported asset_decimals {} > 19", asset_decimals));
    }
    let scale = 10u128
        .checked_pow((19 - asset_decimals) as u32)
        .ok_or_else(|| "scale overflow".to_string())?;
    // Use 256-bit intermediate via high/low split if needed; for S_U < 2^110 and scale <= 10^19 (< 2^64),
    // numerator fits in u128 when S_U <= 1.8e19, or we compute exact (q_raw, rem) in big-int style:
    let (q_raw, rem) = mul_div_rem_u128(sum_asset_atoms, scale, sum_btc_msat)?;
    // Half-up tie check: 2 * rem >= sum_btc_msat
    let round_up = match rem.checked_mul(2) {
        Some(two_rem) => two_rem >= sum_btc_msat,
        None => true,
    };
    let price_q8 = q_raw
        .checked_add(if round_up { 1 } else { 0 })
        .ok_or_else(|| "price_q8 overflow".to_string())?;
    Ok((price_q8, q_raw, rem, round_up))
}

/// Exact `(a * b) / d` and `(a * b) % d` without 128-bit intermediate overflow when quotient fits in `u128`.
pub fn mul_div_rem_u128(a: u128, b: u128, d: u128) -> Result<(u128, u128), String> {
    if d == 0 {
        return Err("division by zero".to_string());
    }
    // Schoolbook 256-bit product from 64-bit halves, then 256-by-128 bit-by-bit division.
    let a_lo = a & ((1u128 << 64) - 1);
    let a_hi = a >> 64;
    let b_lo = b & ((1u128 << 64) - 1);
    let b_hi = b >> 64;

    let p0 = a_lo * b_lo;
    let p1 = a_lo * b_hi;
    let p2 = a_hi * b_lo;
    let p3 = a_hi * b_hi;

    let mid = (p0 >> 64) + (p1 & ((1u128 << 64) - 1)) + (p2 & ((1u128 << 64) - 1));
    let prod_lo = (p0 & ((1u128 << 64) - 1)) | (mid << 64);
    let prod_hi = p3 + (p1 >> 64) + (p2 >> 64) + (mid >> 64);

    if prod_hi >= d {
        return Err("quotient overflows u128".to_string());
    }
    let mut rem = 0u128;
    let mut quot = 0u128;
    for bit in (0..256).rev() {
        let in_bit = if bit >= 128 {
            (prod_hi >> (bit - 128)) & 1
        } else {
            (prod_lo >> bit) & 1
        };
        let overflow = (rem >> 127) == 1;
        rem = (rem << 1) | in_bit;
        if overflow || rem >= d {
            rem = rem.wrapping_sub(d);
            if bit >= 128 {
                return Err("quotient overflows u128".to_string());
            }
            quot |= 1u128 << bit;
        }
    }
    Ok((quot, rem))
}

/// Public inputs and outputs committed by an epoch proof (`TAP-VWAP-03`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicEpochManifest {
    /// Policy hash binding chain, asset, decimals, registry snapshot, and verifier version.
    pub policy_hash: [u8; 32],
    /// Chain genesis hash (32B).
    pub chain_genesis: [u8; 32],
    /// Taproot Asset ID (32B).
    pub asset_id: [u8; 32],
    /// Asset decimal places `d` (`0 <= d <= 19`).
    pub asset_decimals: u8,
    /// Monotonic epoch identifier.
    pub epoch_id: u64,
    /// Authenticated Poseidon Merkle root of the channel/funding peer registry.
    pub registry_root: [u8; 32],
    /// Previous global spent-nullifier Indexed Merkle root (persisted across epochs; never reset).
    pub prev_nullifier_root: [u8; 32],
    /// Updated global spent-nullifier Indexed Merkle root after folding all receipts in this epoch.
    pub next_nullifier_root: [u8; 32],
    /// Previous global cumulative quote-consumption Merkle root (persisted across epochs).
    pub prev_quote_root: [u8; 32],
    /// Updated global cumulative quote-consumption Merkle root.
    pub next_quote_root: [u8; 32],
    /// Hash-chain commitment over the admitted receipt digests in this epoch.
    pub batch_log_commitment: [u8; 32],
    /// Total settled gross BTC volume `S_B` in millisatoshis.
    pub sum_btc_msat: u128,
    /// Total settled asset volume `S_U` in asset atoms.
    pub sum_asset_atoms: u128,
    /// Number of verified conversions folded in this epoch (`n > 0`).
    pub conversion_count: u64,
    /// Exact half-up rounded `q8` price: `floor_half_up(10^(19 - d) * S_U / S_B)`.
    pub vwap_price_q8: u128,
    /// Cryptographic evidence tier (`1` = three-party signed assertions; NOT channel-state proof).
    pub evidence_tier: u8,
    /// Consensus index eligibility (`false` for Tier 1).
    pub eligible_for_consensus_index: bool,
}
