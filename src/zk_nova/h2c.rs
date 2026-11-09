//! Hash-to-curve for the Pasta curves, bit-exact with halo2curves 0.9.0, and nova-snark's
//! `from_label` generator derivation.
//!
//! * [`expand_message_xmd_sha256`] is RFC 9380 `expand_message_xmd` with SHA-256.
//! * [`HashToCurve`] is halo2curves' `Suite<C, Sha256, 48>`: two field elements from 96 expanded
//!   bytes (each 48-byte chunk reversed, then read as a little-endian integer modulo the field
//!   order), each mapped with the SVDW method (Z = -13), then added. The DST is the caller's
//!   `domain_prefix` followed by the suite domain `"pallas:SHA-256_SVDW_RO_"` or
//!   `"vesta:SHA-256_SVDW_RO_"`.
//! * [`from_label`] is nova-snark's `DlogGroup::from_label`: SHAKE256 of the label, 32 bytes per
//!   point, `hash_to_curve("from_uniform_bytes", chunk)`, batch normalization to affine.

use core::fmt;

use crate::hash256::sha256;

use super::curve::{batch_normalize, Affine, BaseOf, Curve, Pallas, Projective, Vesta};
use super::keccak::Shake256;

/// The `domain_prefix` nova-snark passes to `hash_to_curve` in `from_label`.
pub const FROM_LABEL_DOMAIN_PREFIX: &[u8] = b"from_uniform_bytes";
/// Bytes of SHAKE256 output consumed per generator.
pub const FROM_LABEL_BYTES_PER_POINT: usize = 32;
/// Largest number of generators `from_label` accepts (bounds allocation on hostile input).
pub const MAX_FROM_LABEL_POINTS: usize = 1 << 24;

const SHA256_BLOCK_BYTES: usize = 64;
const SHA256_OUTPUT_BYTES: usize = 32;
/// Bytes drawn per field element in halo2curves' SHA-256 suites.
const FIELD_ELEMENT_BYTES: usize = 48;

/// Errors from the hash-to-curve routines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H2cError {
    /// The domain separation tag exceeds 255 bytes.
    DstTooLong,
    /// The requested expansion length is zero or exceeds what `expand_message_xmd` supports.
    InvalidOutputLength,
    /// The SVDW map found no square root; unreachable for the supported curves.
    NoSquareRoot,
    /// The suite constants could not be derived; unreachable for the supported curves.
    InvalidSuite,
    /// A worker thread panicked.
    WorkerFailed,
    /// More generators were requested than `MAX_FROM_LABEL_POINTS`.
    TooManyPoints,
}

impl fmt::Display for H2cError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DstTooLong => write!(f, "domain separation tag is longer than 255 bytes"),
            Self::InvalidOutputLength => write!(f, "invalid expand_message_xmd output length"),
            Self::NoSquareRoot => write!(f, "SVDW map found no square root"),
            Self::InvalidSuite => write!(f, "hash-to-curve suite constants are inconsistent"),
            Self::WorkerFailed => write!(f, "a hashing worker thread failed"),
            Self::TooManyPoints => write!(f, "more generators requested than the supported bound"),
        }
    }
}

impl std::error::Error for H2cError {}

/// SHA-256 of `data`, the crate's own implementation (exposed for fixture checks).
pub fn sha256_digest(data: &[u8]) -> [u8; 32] {
    sha256(data)
}

/// RFC 9380 `expand_message_xmd` instantiated with SHA-256.
///
/// `dst` is the complete domain separation tag (at most 255 bytes). `len_in_bytes` must be in
/// `1..=8160` (255 blocks of 32 bytes).
pub fn expand_message_xmd_sha256(
    msg: &[u8],
    dst: &[u8],
    len_in_bytes: usize,
) -> Result<Vec<u8>, H2cError> {
    if dst.len() > 255 {
        return Err(H2cError::DstTooLong);
    }
    let ell = len_in_bytes.div_ceil(SHA256_OUTPUT_BYTES);
    if len_in_bytes == 0 || ell > 255 || len_in_bytes > 0xffff {
        return Err(H2cError::InvalidOutputLength);
    }
    let dst_len = dst.len() as u8;

    let mut input = Vec::with_capacity(SHA256_BLOCK_BYTES + msg.len() + 3 + dst.len() + 1);
    input.extend_from_slice(&[0u8; SHA256_BLOCK_BYTES]);
    input.extend_from_slice(msg);
    input.extend_from_slice(&(len_in_bytes as u16).to_be_bytes());
    input.push(0);
    input.extend_from_slice(dst);
    input.push(dst_len);
    let b0 = sha256(&input);

    let mut out = Vec::with_capacity(ell * SHA256_OUTPUT_BYTES);
    let mut block_input = Vec::with_capacity(SHA256_OUTPUT_BYTES + 1 + dst.len() + 1);
    block_input.extend_from_slice(&b0);
    block_input.push(1);
    block_input.extend_from_slice(dst);
    block_input.push(dst_len);
    let mut bi = sha256(&block_input);
    out.extend_from_slice(&bi);

    for i in 2..=ell {
        block_input.clear();
        block_input.extend(b0.iter().zip(bi.iter()).map(|(a, b)| a ^ b));
        block_input.push(i as u8);
        block_input.extend_from_slice(dst);
        block_input.push(dst_len);
        bi = sha256(&block_input);
        out.extend_from_slice(&bi);
    }
    out.truncate(len_in_bytes);
    Ok(out)
}

/// A Pasta curve with halo2curves' default hash-to-curve suite.
pub trait H2cCurve: Curve {
    /// The suite domain appended to the caller's domain prefix to form the DST.
    const SUITE_DOMAIN: &'static [u8];
    /// The SVDW parameter is `Z = -NEG_Z` (`-13` for both curves).
    const SVDW_NEG_Z: u64;
}

impl H2cCurve for Pallas {
    const SUITE_DOMAIN: &'static [u8] = b"pallas:SHA-256_SVDW_RO_";
    const SVDW_NEG_Z: u64 = 13;
}

impl H2cCurve for Vesta {
    const SUITE_DOMAIN: &'static [u8] = b"vesta:SHA-256_SVDW_RO_";
    const SVDW_NEG_Z: u64 = 13;
}

/// The SVDW map with its precomputed constants (RFC 9380 section 6.6.1, `A = 0`, `B = 5`).
#[derive(Clone, Copy, Debug)]
pub struct Svdw<C: H2cCurve> {
    z: BaseOf<C>,
    c1: BaseOf<C>,
    c2: BaseOf<C>,
    c3: BaseOf<C>,
    c4: BaseOf<C>,
}

impl<C: H2cCurve> Svdw<C> {
    /// Derives `c1..c4` for `Z = -SVDW_NEG_Z`.
    pub fn new() -> Result<Self, H2cError> {
        let z = -BaseOf::<C>::from_u64(C::SVDW_NEG_Z);
        let b = BaseOf::<C>::from_u64(5);
        let three = BaseOf::<C>::from_u64(3);
        let four = BaseOf::<C>::from_u64(4);
        let tmp = three * z.square();
        // c1 = g(Z)
        let c1 = z.square() * z + b;
        // c2 = -Z / 2
        let two_inv = BaseOf::<C>::from_u64(2)
            .invert()
            .ok_or(H2cError::InvalidSuite)?;
        let c2 = -z * two_inv;
        // c3 = sqrt(-g(Z) * (3 Z^2)) with sgn0(c3) = 0
        let c3 = (-c1 * tmp).sqrt().ok_or(H2cError::InvalidSuite)?;
        let c3 = if c3.is_odd() { -c3 } else { c3 };
        // c4 = -4 g(Z) / (3 Z^2)
        let c4 = -four * c1 * tmp.invert().ok_or(H2cError::InvalidSuite)?;
        Ok(Self { z, c1, c2, c3, c4 })
    }

    /// The constants `[Z, c1, c2, c3, c4]`.
    pub fn constants(&self) -> [BaseOf<C>; 5] {
        [self.z, self.c1, self.c2, self.c3, self.c4]
    }

    /// Maps a field element to a curve point.
    pub fn map(&self, u: &BaseOf<C>) -> Result<Projective<C>, H2cError> {
        let one = BaseOf::<C>::ONE;
        let b = BaseOf::<C>::from_u64(5);
        let g = |x: BaseOf<C>| x.square() * x + b;

        let tv1 = u.square() * self.c1;
        let tv2 = one + tv1;
        let tv1 = one - tv1;
        let tv3 = (tv1 * tv2).invert().unwrap_or(BaseOf::<C>::ZERO);
        let tv4 = *u * tv1 * tv3 * self.c3;
        let x1 = self.c2 - tv4;
        let e1 = g(x1).is_square();
        let x2 = self.c2 + tv4;
        let e2 = g(x2).is_square() && !e1;
        let x3 = tv2.square() * tv3;
        let x3 = x3.square() * self.c4 + self.z;
        let x = if e1 {
            x1
        } else if e2 {
            x2
        } else {
            x3
        };
        let y = g(x).sqrt().ok_or(H2cError::NoSquareRoot)?;
        let y = if u.is_odd() == y.is_odd() { y } else { -y };
        Ok(Affine::<C>::new(x, y)
            .map_err(|_| H2cError::NoSquareRoot)?
            .to_projective())
    }
}

/// halo2curves' `hash_to_curve` suite for curve `C` with precomputed SVDW constants.
#[derive(Clone, Copy, Debug)]
pub struct HashToCurve<C: H2cCurve> {
    svdw: Svdw<C>,
}

impl<C: H2cCurve> HashToCurve<C> {
    /// Builds the suite.
    pub fn new() -> Result<Self, H2cError> {
        Ok(Self { svdw: Svdw::new()? })
    }

    /// `hash_to_curve(domain_prefix)(message)`: the DST is `domain_prefix || SUITE_DOMAIN`.
    pub fn hash(&self, domain_prefix: &[u8], message: &[u8]) -> Result<Projective<C>, H2cError> {
        let mut dst = Vec::with_capacity(domain_prefix.len() + C::SUITE_DOMAIN.len());
        dst.extend_from_slice(domain_prefix);
        dst.extend_from_slice(C::SUITE_DOMAIN);
        self.hash_with_dst(&dst, message)
    }

    /// The SVDW map this suite uses.
    pub fn svdw(&self) -> &Svdw<C> {
        &self.svdw
    }

    /// The two field elements `(u0, u1)` derived for `message` under the full DST `dst`.
    pub fn hash_to_field(
        &self,
        dst: &[u8],
        message: &[u8],
    ) -> Result<(BaseOf<C>, BaseOf<C>), H2cError> {
        let out = expand_message_xmd_sha256(message, dst, 2 * FIELD_ELEMENT_BYTES)?;
        let to_field = |chunk: &[u8]| {
            let mut bytes = [0u8; FIELD_ELEMENT_BYTES];
            for (dst_byte, src_byte) in bytes.iter_mut().zip(chunk.iter().rev()) {
                *dst_byte = *src_byte;
            }
            BaseOf::<C>::from_uniform_bytes_48(&bytes)
        };
        Ok((
            to_field(&out[..FIELD_ELEMENT_BYTES]),
            to_field(&out[FIELD_ELEMENT_BYTES..]),
        ))
    }

    fn hash_with_dst(&self, dst: &[u8], message: &[u8]) -> Result<Projective<C>, H2cError> {
        let (u0, u1) = self.hash_to_field(dst, message)?;
        Ok(self.svdw.map(&u0)?.add_projective(&self.svdw.map(&u1)?))
    }
}

/// One-shot `hash_to_curve(domain_prefix)(message)` for curve `C`.
pub fn hash_to_curve<C: H2cCurve>(
    domain_prefix: &[u8],
    message: &[u8],
) -> Result<Projective<C>, H2cError> {
    HashToCurve::<C>::new()?.hash(domain_prefix, message)
}

/// nova-snark's `from_label(label, n)`: `n` affine generators for the curve `C`.
///
/// Work is split across std threads in contiguous chunks; the output depends only on the inputs.
pub fn from_label<C: H2cCurve>(label: &[u8], n: usize) -> Result<Vec<Affine<C>>, H2cError> {
    if n > MAX_FROM_LABEL_POINTS {
        return Err(H2cError::TooManyPoints);
    }
    let suite = HashToCurve::<C>::new()?;
    let mut shake = Shake256::new();
    shake.update(label);
    let mut reader = shake.finalize_xof();
    let mut uniform = vec![0u8; n * FROM_LABEL_BYTES_PER_POINT];
    reader.read(&mut uniform);

    let mut dst = Vec::with_capacity(FROM_LABEL_DOMAIN_PREFIX.len() + C::SUITE_DOMAIN.len());
    dst.extend_from_slice(FROM_LABEL_DOMAIN_PREFIX);
    dst.extend_from_slice(C::SUITE_DOMAIN);

    let threads = if n < 64 {
        1
    } else {
        std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1)
            .min(n / 16)
            .max(1)
    };
    let chunk_points = n.div_ceil(threads).max(1);
    let process = |bytes: &[u8]| -> Result<Vec<Affine<C>>, H2cError> {
        let mut points = Vec::with_capacity(bytes.len() / FROM_LABEL_BYTES_PER_POINT);
        for message in bytes.chunks_exact(FROM_LABEL_BYTES_PER_POINT) {
            points.push(suite.hash_with_dst(&dst, message)?);
        }
        Ok(batch_normalize(&points))
    };

    if threads == 1 {
        return process(&uniform);
    }
    let mut out = Vec::with_capacity(n);
    let results: Vec<Result<Vec<Affine<C>>, H2cError>> = std::thread::scope(|scope| {
        let handles: Vec<_> = uniform
            .chunks(chunk_points * FROM_LABEL_BYTES_PER_POINT)
            .map(|chunk| scope.spawn(|| process(chunk)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or(Err(H2cError::WorkerFailed)))
            .collect()
    });
    for result in results {
        out.extend(result?);
    }
    Ok(out)
}
