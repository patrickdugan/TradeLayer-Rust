//! Nova's Keccak256 transcript (`nova-snark` `provider/keccak.rs`).
//!
//! State is 64 bytes, `K2(m) = Keccak256(m || 0x00) || Keccak256(m || 0x01)`. A squeeze hashes the
//! absorbed bytes, the round counter and the previous state, and reduces all 64 output bytes into
//! the scalar field `P`.

use std::marker::PhantomData;

use super::curve::{Affine, Curve};
use super::field::{Field, FieldParams};
use super::keccak::Keccak256;

/// Bytes of a transcript-encoded commitment: `x || y || infinity`.
pub const COMMITMENT_BYTES: usize = 65;

fn k2(parts: &[&[u8]]) -> [u8; 64] {
    let mut hasher = Keccak256::new();
    for part in parts {
        hasher.update(part);
    }
    let mut lo = hasher.clone();
    lo.update(&[0x00]);
    let mut hi = hasher;
    hi.update(&[0x01]);
    let mut out = [0u8; 64];
    out[..32].copy_from_slice(&lo.finalize());
    out[32..].copy_from_slice(&hi.finalize());
    out
}

/// The transcript encoding of a commitment: `x || y || infinity` (identity: zeros and 1).
pub fn commitment_bytes<C: Curve>(point: &Affine<C>) -> [u8; COMMITMENT_BYTES] {
    let mut out = [0u8; COMMITMENT_BYTES];
    if point.is_identity() {
        out[64] = 1;
    } else {
        out[..32].copy_from_slice(&point.x().to_repr());
        out[32..64].copy_from_slice(&point.y().to_repr());
    }
    out
}

/// A Keccak256 transcript whose challenges live in the field `Field<P>`.
#[derive(Clone)]
pub struct Transcript<P: FieldParams> {
    round: u64,
    state: [u8; 64],
    buf: Vec<u8>,
    _field: PhantomData<P>,
}

impl<P: FieldParams> Transcript<P> {
    /// `new(label)`: `state = K2("NoTR" || label)`.
    pub fn new(label: &[u8]) -> Self {
        Self {
            round: 0,
            state: k2(&[b"NoTR", label]),
            buf: Vec::new(),
            _field: PhantomData,
        }
    }

    /// Appends raw bytes after `label`.
    pub fn absorb_bytes(&mut self, label: &[u8], bytes: &[u8]) {
        self.buf.extend_from_slice(label);
        self.buf.extend_from_slice(bytes);
    }

    /// Appends one scalar (32 bytes little endian).
    pub fn absorb_scalar(&mut self, label: &[u8], value: &Field<P>) {
        self.absorb_bytes(label, &value.to_repr());
    }

    /// Appends the concatenation of scalars, with no length prefix.
    pub fn absorb_scalars(&mut self, label: &[u8], values: &[Field<P>]) {
        self.buf.extend_from_slice(label);
        for value in values {
            self.buf.extend_from_slice(&value.to_repr());
        }
    }

    /// Appends the concatenation of commitments, each as `x || y || infinity`.
    pub fn absorb_commitments<C: Curve<Scalar = P>>(&mut self, label: &[u8], points: &[Affine<C>]) {
        self.buf.extend_from_slice(label);
        for point in points {
            self.buf.extend_from_slice(&commitment_bytes(point));
        }
    }

    /// Appends a domain separator `"NoDS" || tag`.
    pub fn dom_sep(&mut self, tag: &[u8]) {
        self.buf.extend_from_slice(b"NoDS");
        self.buf.extend_from_slice(tag);
    }

    /// Squeezes a challenge and advances the state.
    pub fn squeeze(&mut self, label: &[u8]) -> Field<P> {
        let round = self.round.to_le_bytes();
        let out = k2(&[&self.buf, b"NoDS", &round, &self.state, label]);
        self.state = out;
        self.round += 1;
        self.buf.clear();
        Field::<P>::from_uniform_bytes_64(&out)
    }

    /// The current 64-byte state (for tests against recorded sessions).
    pub fn state(&self) -> &[u8; 64] {
        &self.state
    }
}
