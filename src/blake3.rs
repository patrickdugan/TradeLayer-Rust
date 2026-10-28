//! Scalar, safe, dependency-free unkeyed BLAKE3 with a 32-byte output.
//!
//! This module hashes the exact supplied bytes. It supplies no framing, tag,
//! serialization, chain-membership or actor-authorization interpretation.

const CHUNK_BYTES: usize = 1024;
const BLOCK_BYTES: usize = 64;
const CHUNK_START: u32 = 1;
const CHUNK_END: u32 = 2;
const PARENT: u32 = 4;
const ROOT: u32 = 8;
const IV: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];
const PERMUTATION: [usize; 16] = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8];

/// Ordinary unkeyed BLAKE3-256 of exactly `input`, in natural digest order.
///
/// No keyed/KDF/XOF mode, prefix or byte reversal is applied. Memory use is
/// logarithmic stack space for the fixed BLAKE3 tree; input is borrowed and
/// the implementation performs no heap allocation or unsafe operation.
pub fn blake3_256(input: &[u8]) -> [u8; 32] {
    let words = subtree(input, 0, true);
    let mut output = [0u8; 32];
    for (bytes, word) in output.chunks_exact_mut(4).zip(words) {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    output
}

fn subtree(input: &[u8], first_chunk: u64, is_root: bool) -> [u32; 8] {
    if input.len() <= CHUNK_BYTES {
        return chunk(input, first_chunk, is_root);
    }
    // A left subtree is complete and at least as large as its right sibling.
    // Using len-1 keeps exact powers of two split into equal siblings and
    // leaves an exact final chunk as real data, never an extra empty chunk.
    let chunks_before_last = (input.len() - 1) / CHUNK_BYTES;
    let left_chunks = 1usize << chunks_before_last.ilog2();
    let split = left_chunks * CHUNK_BYTES; // <= input.len()-1 by construction
    let left = subtree(&input[..split], first_chunk, false);
    let right = subtree(&input[split..], first_chunk + left_chunks as u64, false);
    let mut block = [0u32; 16];
    block[..8].copy_from_slice(&left);
    block[8..].copy_from_slice(&right);
    compress(
        IV,
        block,
        0,
        BLOCK_BYTES as u32,
        PARENT | root_flag(is_root),
    )
}

fn root_flag(is_root: bool) -> u32 {
    if is_root {
        ROOT
    } else {
        0
    }
}

fn chunk(input: &[u8], counter: u64, is_root: bool) -> [u32; 8] {
    let mut cv = IV;
    let blocks = input.len().div_ceil(BLOCK_BYTES).max(1);
    for index in 0..blocks {
        let start = index * BLOCK_BYTES;
        let end = (start + BLOCK_BYTES).min(input.len());
        let bytes = &input[start..end];
        let mut padded = [0u8; BLOCK_BYTES];
        padded[..bytes.len()].copy_from_slice(bytes);
        let words = std::array::from_fn(|i| {
            u32::from_le_bytes(padded[i * 4..i * 4 + 4].try_into().expect("4-byte word"))
        });
        let mut flags = if index == 0 { CHUNK_START } else { 0 };
        if index + 1 == blocks {
            flags |= CHUNK_END | root_flag(is_root);
        }
        cv = compress(cv, words, counter, bytes.len() as u32, flags);
    }
    cv
}

fn mix(state: &mut [u32; 16], [a, b, c, d]: [usize; 4], x: u32, y: u32) {
    state[a] = state[a].wrapping_add(state[b]).wrapping_add(x);
    state[d] = (state[d] ^ state[a]).rotate_right(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_right(12);
    state[a] = state[a].wrapping_add(state[b]).wrapping_add(y);
    state[d] = (state[d] ^ state[a]).rotate_right(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_right(7);
}

fn compress(cv: [u32; 8], mut message: [u32; 16], counter: u64, len: u32, flags: u32) -> [u32; 8] {
    let mut state = [0u32; 16];
    state[..8].copy_from_slice(&cv);
    state[8..12].copy_from_slice(&IV[..4]);
    state[12] = counter as u32;
    state[13] = (counter >> 32) as u32;
    state[14] = len;
    state[15] = flags;
    for round in 0..7 {
        for (indexes, offset) in [
            ([0, 4, 8, 12], 0),
            ([1, 5, 9, 13], 2),
            ([2, 6, 10, 14], 4),
            ([3, 7, 11, 15], 6),
            ([0, 5, 10, 15], 8),
            ([1, 6, 11, 12], 10),
            ([2, 7, 8, 13], 12),
            ([3, 4, 9, 14], 14),
        ] {
            mix(&mut state, indexes, message[offset], message[offset + 1]);
        }
        if round != 6 {
            message = std::array::from_fn(|i| message[PERMUTATION[i]]);
        }
    }
    // Only the first 32 output bytes are exposed: this is both a node CV and,
    // with ROOT set at the final compression, output block zero's first half.
    std::array::from_fn(|i| state[i] ^ state[i + 8])
}
