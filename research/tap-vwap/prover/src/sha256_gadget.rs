//! R1CS Tagged SHA-256 gadget with precomputed 64-byte tag midstate (`SHA256(tag) || SHA256(tag)`).
//!
//! Uses `nova_snark::frontend::gadgets::sha256::sha256_compression_function` directly.
//! Saves one full 512-bit SHA-256 block (~25,800 R1CS constraints) per tagged hash while
//! remaining bit-for-bit identical to `SHA256(SHA256(tag) || SHA256(tag) || msg)`.

use ff::PrimeField;
use nova_snark::frontend::{
    gadgets::{
        boolean::Boolean,
        sha256::sha256_compression_function,
        uint32::UInt32,
    },
    ConstraintSystem, SynthesisError,
};
use sha2::{compress256, Digest, Sha256};

/// Standard FIPS 180-4 SHA-256 initial state words.
pub const SHA256_IV: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Compute the exact SHA-256 midstate (`[u32; 8]`) after compressing the 64-byte prefix
/// `SHA256(tag) || SHA256(tag)`.
pub fn compute_tagged_midstate(tag: &[u8]) -> [u32; 8] {
    let th = Sha256::digest(tag);
    let mut block = [0u8; 64];
    block[..32].copy_from_slice(&th);
    block[32..64].copy_from_slice(&th);
    let mut state = SHA256_IV;
    let ga = generic_array::GenericArray::clone_from_slice(&block);
    compress256(&mut state, &[ga]);
    state
}

/// Synthesize tagged SHA-256 in R1CS over `msg_bits_be` (must be byte-aligned, i.e., `len % 8 == 0`),
/// starting from the precomputed 64-byte tag block midstate.
///
/// Total message bit length encoded into the FIPS 180-4 trailer is `(64 * 8) + msg_bits_be.len()`.
pub fn tagged_sha256_circuit<F: PrimeField, CS: ConstraintSystem<F>>(
    mut cs: CS,
    tag: &[u8],
    msg_bits_be: &[Boolean],
) -> Result<Vec<Boolean>, SynthesisError> {
    assert_eq!(msg_bits_be.len() % 8, 0, "message bits must be byte-aligned");
    let midstate = compute_tagged_midstate(tag);

    let total_len_bits = 512u64 + (msg_bits_be.len() as u64);
    let mut padded = msg_bits_be.to_vec();
    padded.push(Boolean::constant(true));
    while (padded.len() + 64) % 512 != 0 {
        padded.push(Boolean::constant(false));
    }
    for b in (0..64).rev().map(|i| ((total_len_bits >> i) & 1) == 1) {
        padded.push(Boolean::constant(b));
    }
    debug_assert_eq!(padded.len() % 512, 0);

    let mut cur: Vec<UInt32> = midstate.iter().map(|&w| UInt32::constant(w)).collect();
    for (blk_idx, block) in padded.chunks(512).enumerate() {
        cur = sha256_compression_function(
            cs.namespace(|| format!("tagged_sha256_blk_{blk_idx}")),
            block,
            &cur,
        )?;
    }

    Ok(cur.into_iter().flat_map(|w| w.into_bits_be()).collect())
}

/// Allocate a byte slice as big-endian `Boolean` bits (8 bits per byte, MSB first).
pub fn alloc_bytes_be<F: PrimeField, CS: ConstraintSystem<F>>(
    mut cs: CS,
    bytes: Option<&[u8]>,
    num_bytes: usize,
) -> Result<Vec<Boolean>, SynthesisError> {
    let mut out = Vec::with_capacity(num_bytes * 8);
    for byte_i in 0..num_bytes {
        let b_opt = bytes.map(|s| s[byte_i]);
        for bit_j in (0..8).rev() {
            let bit_val = b_opt.map(|v| ((v >> bit_j) & 1) == 1);
            let bit = Boolean::from(nova_snark::frontend::AllocatedBit::alloc(
                cs.namespace(|| format!("byte_{byte_i}_bit_{bit_j}")),
                bit_val,
            )?);
            out.push(bit);
        }
    }
    Ok(out)
}

/// Extract big-endian bytes from a slice of `Boolean` witness values (for tests/debugging).
pub fn booleans_be_to_bytes(bits: &[Boolean]) -> Option<Vec<u8>> {
    assert_eq!(bits.len() % 8, 0);
    let mut out = Vec::with_capacity(bits.len() / 8);
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for b in chunk {
            byte = (byte << 1) | (b.get_value()? as u8);
        }
        out.push(byte);
    }
    Some(out)
}
