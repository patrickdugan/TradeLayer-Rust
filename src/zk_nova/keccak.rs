//! Keccak-f[1600], Keccak256 and SHAKE256.
//!
//! `Keccak256` uses the original Keccak padding (`0x01 ... 0x80`), as the `sha3` crate's
//! `Keccak256` does, and differs from NIST SHA3-256. `Shake256` is the FIPS 202 extendable
//! output function (domain byte `0x1f`), as used by nova-snark's `from_label`.

const ROUNDS: usize = 24;
/// Sponge rate in bytes for capacity 512 (Keccak256 and SHAKE256).
const RATE: usize = 136;

const ROUND_CONSTANTS: [u64; ROUNDS] = [
    0x0000000000000001,
    0x0000000000008082,
    0x800000000000808a,
    0x8000000080008000,
    0x000000000000808b,
    0x0000000080000001,
    0x8000000080008081,
    0x8000000000008009,
    0x000000000000008a,
    0x0000000000000088,
    0x0000000080008009,
    0x000000008000000a,
    0x000000008000808b,
    0x800000000000008b,
    0x8000000000008089,
    0x8000000000008003,
    0x8000000000008002,
    0x8000000000000080,
    0x000000000000800a,
    0x800000008000000a,
    0x8000000080008081,
    0x8000000000008080,
    0x0000000080000001,
    0x8000000080008008,
];

/// Rotation offsets for the rho step, indexed `x + 5 * y`.
const RHO: [u32; 25] = [
    0, 1, 62, 28, 27, 36, 44, 6, 55, 20, 3, 10, 43, 25, 39, 41, 45, 15, 21, 8, 18, 2, 61, 56, 14,
];

/// The Keccak-f[1600] permutation on a state of 25 little-endian lanes, lane `x + 5 * y`.
pub fn keccak_f1600(state: &mut [u64; 25]) {
    for &round_constant in &ROUND_CONSTANTS {
        // theta
        let mut c = [0u64; 5];
        for (x, column) in c.iter_mut().enumerate() {
            *column = state[x] ^ state[x + 5] ^ state[x + 10] ^ state[x + 15] ^ state[x + 20];
        }
        for x in 0..5 {
            let d = c[(x + 4) % 5] ^ c[(x + 1) % 5].rotate_left(1);
            for y in 0..5 {
                state[x + 5 * y] ^= d;
            }
        }
        // rho and pi
        let mut b = [0u64; 25];
        for x in 0..5 {
            for y in 0..5 {
                b[y + 5 * ((2 * x + 3 * y) % 5)] = state[x + 5 * y].rotate_left(RHO[x + 5 * y]);
            }
        }
        // chi
        for y in 0..5 {
            for x in 0..5 {
                state[x + 5 * y] =
                    b[x + 5 * y] ^ ((!b[(x + 1) % 5 + 5 * y]) & b[(x + 2) % 5 + 5 * y]);
            }
        }
        // iota
        state[0] ^= round_constant;
    }
}

/// A rate-136 sponge that absorbs bytes and finishes with a caller-chosen domain byte.
#[derive(Clone)]
struct Sponge {
    state: [u64; 25],
    buffer: [u8; RATE],
    filled: usize,
}

impl Sponge {
    fn new() -> Self {
        Self {
            state: [0; 25],
            buffer: [0; RATE],
            filled: 0,
        }
    }

    fn absorb_block(state: &mut [u64; 25], block: &[u8; RATE]) {
        for (lane, chunk) in state.iter_mut().zip(block.chunks_exact(8)) {
            let mut word = [0u8; 8];
            word.copy_from_slice(chunk);
            *lane ^= u64::from_le_bytes(word);
        }
        keccak_f1600(state);
    }

    fn update(&mut self, mut data: &[u8]) {
        if self.filled > 0 {
            let take = (RATE - self.filled).min(data.len());
            self.buffer[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled < RATE {
                return;
            }
            let block = self.buffer;
            Self::absorb_block(&mut self.state, &block);
            self.filled = 0;
        }
        let mut blocks = data.chunks_exact(RATE);
        for chunk in &mut blocks {
            let mut block = [0u8; RATE];
            block.copy_from_slice(chunk);
            Self::absorb_block(&mut self.state, &block);
        }
        let rest = blocks.remainder();
        self.buffer[..rest.len()].copy_from_slice(rest);
        self.filled = rest.len();
    }

    /// Applies the pad10*1 rule with `domain` as the first padding byte and permutes.
    fn pad(&mut self, domain: u8) {
        self.buffer[self.filled..].fill(0);
        self.buffer[self.filled] ^= domain;
        self.buffer[RATE - 1] ^= 0x80;
        let block = self.buffer;
        Self::absorb_block(&mut self.state, &block);
        self.filled = 0;
    }

    fn squeeze_block(&self) -> [u8; RATE] {
        let mut out = [0u8; RATE];
        for (chunk, lane) in out.chunks_exact_mut(8).zip(self.state.iter()) {
            chunk.copy_from_slice(&lane.to_le_bytes());
        }
        out
    }
}

/// Incremental Keccak256 (original Keccak padding).
#[derive(Clone)]
pub struct Keccak256 {
    sponge: Sponge,
}

impl Default for Keccak256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Keccak256 {
    /// Starts an empty hash.
    pub fn new() -> Self {
        Self {
            sponge: Sponge::new(),
        }
    }

    /// Absorbs bytes.
    pub fn update(&mut self, data: &[u8]) {
        self.sponge.update(data);
    }

    /// Finishes and returns the 32-byte digest.
    pub fn finalize(mut self) -> [u8; 32] {
        self.sponge.pad(0x01);
        let block = self.sponge.squeeze_block();
        let mut out = [0u8; 32];
        out.copy_from_slice(&block[..32]);
        out
    }
}

/// One-shot Keccak256.
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    hasher.finalize()
}

/// One-shot NIST SHA3-256 (padding byte `0x06`), used by nova's `DigestComputer`.
pub fn sha3_256(data: &[u8]) -> [u8; 32] {
    let mut sponge = Sponge::new();
    sponge.update(data);
    sponge.pad(0x06);
    let block = sponge.squeeze_block();
    let mut out = [0u8; 32];
    out.copy_from_slice(&block[..32]);
    out
}

/// Incremental SHAKE256 absorbing phase.
#[derive(Clone)]
pub struct Shake256 {
    sponge: Sponge,
}

impl Default for Shake256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Shake256 {
    /// Starts an empty XOF.
    pub fn new() -> Self {
        Self {
            sponge: Sponge::new(),
        }
    }

    /// Absorbs bytes.
    pub fn update(&mut self, data: &[u8]) {
        self.sponge.update(data);
    }

    /// Ends absorption and returns the squeezing reader.
    pub fn finalize_xof(mut self) -> Shake256Reader {
        self.sponge.pad(0x1f);
        Shake256Reader {
            block: self.sponge.squeeze_block(),
            offset: 0,
            state: self.sponge.state,
        }
    }
}

/// Squeezing phase of SHAKE256; yields an unbounded byte stream.
#[derive(Clone)]
pub struct Shake256Reader {
    state: [u64; 25],
    block: [u8; RATE],
    offset: usize,
}

impl Shake256Reader {
    /// Fills `out` with the next output bytes.
    pub fn read(&mut self, out: &mut [u8]) {
        let mut written = 0;
        while written < out.len() {
            if self.offset == RATE {
                keccak_f1600(&mut self.state);
                for (chunk, lane) in self.block.chunks_exact_mut(8).zip(self.state.iter()) {
                    chunk.copy_from_slice(&lane.to_le_bytes());
                }
                self.offset = 0;
            }
            let take = (RATE - self.offset).min(out.len() - written);
            out[written..written + take]
                .copy_from_slice(&self.block[self.offset..self.offset + take]);
            self.offset += take;
            written += take;
        }
    }
}

/// One-shot SHAKE256 with `out.len()` bytes of output.
pub fn shake256(data: &[u8], out: &mut [u8]) {
    let mut xof = Shake256::new();
    xof.update(data);
    xof.finalize_xof().read(out);
}
