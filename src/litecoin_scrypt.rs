//! Fixed-parameter Litecoin header hashing, independent of chain validation.
//!
//! The RPC block identifier is SHA256d; the separate proof-of-work digest is
//! Scrypt with password=salt=the 80 serialized header bytes, N=1024, r=1, p=1,
//! dkLen=32. Both APIs return raw digest bytes. Reverse once for Core's usual
//! uint256 hexadecimal display. Computing either hash authenticates no chain.

use crate::hash256::{double_sha256, sha256};

pub const LITECOIN_HEADER_BYTES: usize = 80;
const ROMIX_ROWS: usize = 1024;
const WORDS_PER_ROW: usize = 32;
/// Fixed ROMix table allocation; small fixed working arrays are additional.
pub const LITECOIN_SCRYPT_ROMIX_BYTES: usize = ROMIX_ROWS * WORDS_PER_ROW * 4;

/// Litecoin RPC block identifier, not its Scrypt proof-of-work digest.
pub fn litecoin_header_id(header: &[u8; LITECOIN_HEADER_BYTES]) -> [u8; 32] {
    double_sha256(header)
}

/// Compute the separate Scrypt PoW digest with a fresh 128 KiB workspace.
/// Reuse [`LitecoinScrypt`] when hashing several headers serially.
pub fn litecoin_scrypt_pow_hash(header: &[u8; LITECOIN_HEADER_BYTES]) -> [u8; 32] {
    LitecoinScrypt::new().hash(header)
}

/// Reusable fixed-size ROMix memory. Each hash overwrites all rows before any
/// data-dependent read; no result depends on a previously hashed header.
///
/// This is a hashing primitive. It does not interpret nBits, check a target,
/// implement a difficulty schedule, establish chainwork or validate a block.
pub struct LitecoinScrypt {
    memory: Box<[[u32; WORDS_PER_ROW]]>,
}

impl Default for LitecoinScrypt {
    fn default() -> Self {
        Self::new()
    }
}

impl LitecoinScrypt {
    /// Allocate exactly 1024 rows of 128 bytes on the heap, avoiding a large
    /// stack array. Allocation failure follows Rust's standard allocation path.
    pub fn new() -> Self {
        Self {
            memory: vec![[0; WORDS_PER_ROW]; ROMIX_ROWS].into_boxed_slice(),
        }
    }

    /// Hash exactly one already-serialized Litecoin header, in wire order.
    /// No input-sized allocation, adjustable cost or arbitrary-length header
    /// input is exposed. The returned digest is in raw byte order.
    pub fn hash(&mut self, header: &[u8; LITECOIN_HEADER_BYTES]) -> [u8; 32] {
        let hmac = HeaderHmac::new(header);
        let mut block = [0u8; 128];
        // PBKDF2-HMAC-SHA256, c=1, dkLen=128: four independent U1 blocks.
        for (index, part) in block.chunks_exact_mut(32).enumerate() {
            part.copy_from_slice(&hmac.salted_block(header, (index + 1) as u32));
        }
        let mut words = [0u32; WORDS_PER_ROW];
        for (word, part) in words.iter_mut().zip(block.chunks_exact(4)) {
            *word = u32::from_le_bytes(part.try_into().expect("4-byte word"));
        }
        for row in &mut self.memory {
            *row = words;
            block_mix_r1(&mut words);
        }
        for _ in 0..ROMIX_ROWS {
            // Integerify reads the final 64-byte block little-endian. With
            // N=1024 only its low ten bits matter, all within words[16].
            let index = (words[16] & (ROMIX_ROWS as u32 - 1)) as usize;
            for (word, stored) in words.iter_mut().zip(&self.memory[index]) {
                *word ^= stored;
            }
            block_mix_r1(&mut words);
        }
        for (part, word) in block.chunks_exact_mut(4).zip(words) {
            part.copy_from_slice(&word.to_le_bytes());
        }
        // Final PBKDF2-HMAC-SHA256, c=1, dkLen=32: only U1 is needed.
        hmac.salted_block(&block, 1)
    }
}

// HMAC always uses the fixed 80-byte password here. SHA256 shortens that
// over-block-size key before the standard 64-byte ipad/opad are applied.
struct HeaderHmac {
    inner_pad: [u8; 64],
    outer_pad: [u8; 64],
}

impl HeaderHmac {
    fn new(header: &[u8; LITECOIN_HEADER_BYTES]) -> Self {
        let key = sha256(header);
        let mut inner_pad = [0x36; 64];
        let mut outer_pad = [0x5c; 64];
        for ((inner, outer), key) in inner_pad.iter_mut().zip(&mut outer_pad).zip(key) {
            *inner ^= key;
            *outer ^= key;
        }
        Self {
            inner_pad,
            outer_pad,
        }
    }

    // This private generic is instantiated only with N=80 and N=128. Both
    // salt||INT_32_BE(index) messages fit the fixed 132-byte message capacity.
    fn salted_block<const N: usize>(&self, salt: &[u8; N], index: u32) -> [u8; 32] {
        let mut inner = [0u8; 64 + 128 + 4];
        inner[..64].copy_from_slice(&self.inner_pad);
        inner[64..64 + N].copy_from_slice(salt);
        inner[64 + N..68 + N].copy_from_slice(&index.to_be_bytes());
        let inner_digest = sha256(&inner[..68 + N]);
        let mut outer = [0u8; 64 + 32];
        outer[..64].copy_from_slice(&self.outer_pad);
        outer[64..].copy_from_slice(&inner_digest);
        sha256(&outer)
    }
}

fn quarter_round(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    state[b] ^= state[a].wrapping_add(state[d]).rotate_left(7);
    state[c] ^= state[b].wrapping_add(state[a]).rotate_left(9);
    state[d] ^= state[c].wrapping_add(state[b]).rotate_left(13);
    state[a] ^= state[d].wrapping_add(state[c]).rotate_left(18);
}

fn xor_salsa8(block: &mut [u32], other: &[u32]) {
    let mut input = [0u32; 16];
    for ((word, left), right) in input.iter_mut().zip(block.iter()).zip(other) {
        *word = left ^ right;
    }
    let mut state = input;
    for _ in 0..4 {
        quarter_round(&mut state, 0, 4, 8, 12);
        quarter_round(&mut state, 5, 9, 13, 1);
        quarter_round(&mut state, 10, 14, 2, 6);
        quarter_round(&mut state, 15, 3, 7, 11);
        quarter_round(&mut state, 0, 1, 2, 3);
        quarter_round(&mut state, 5, 6, 7, 4);
        quarter_round(&mut state, 10, 11, 8, 9);
        quarter_round(&mut state, 15, 12, 13, 14);
    }
    for ((word, mixed), original) in block.iter_mut().zip(state).zip(input) {
        *word = mixed.wrapping_add(original);
    }
}

fn block_mix_r1(words: &mut [u32; WORDS_PER_ROW]) {
    let (first, second) = words.split_at_mut(16);
    xor_salsa8(first, second);
    xor_salsa8(second, first);
}
