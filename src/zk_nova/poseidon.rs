//! The Poseidon random oracle of nova-snark 0.76.0 (`provider/poseidon.rs`, wide mode).
//!
//! Width 25 (rate 24, capacity 1), S-box `x^5`, 8 full and 59 partial rounds, a Cauchy MDS matrix
//! `M[i][j] = 1 / (i + t + j)` and round constants from the Poseidon reference Grain LFSR. Every
//! constant is derived here from the field; nothing is embedded. The hash is a SAFE sponge with
//! the I/O pattern `[Absorb(n), Squeeze(1)]`, run once per oracle.

use super::field::{Field, FieldParams};

/// Sponge width.
pub const WIDTH: usize = 25;
/// Sponge rate (`WIDTH - 1`).
pub const RATE: usize = WIDTH - 1;
/// Full rounds.
pub const FULL_ROUNDS: usize = 8;
/// Partial rounds.
pub const PARTIAL_ROUNDS: usize = 59;
/// Bits of a Pasta field modulus, input to the Grain initialisation.
const FIELD_BITS: usize = 255;

/// Derived constants for one field.
#[derive(Clone)]
pub struct PoseidonParams<P: FieldParams> {
    mds: Vec<Field<P>>,
    round_constants: Vec<Field<P>>,
}

struct Grain {
    bits: [bool; 80],
}

impl Grain {
    fn new() -> Self {
        let mut bits = [false; 80];
        let mut at = 0;
        let mut put = |value: u64, width: usize| {
            for i in (0..width).rev() {
                bits[at] = (value >> i) & 1 == 1;
                at += 1;
            }
        };
        put(1, 2); // prime field
        put(1, 4); // x^5
        put(FIELD_BITS as u64, 12);
        put(WIDTH as u64, 12);
        put(FULL_ROUNDS as u64, 10);
        put(PARTIAL_ROUNDS as u64, 10);
        put((1 << 30) - 1, 30);
        let mut grain = Self { bits };
        for _ in 0..160 {
            grain.step();
        }
        grain
    }

    fn step(&mut self) -> bool {
        let b = &self.bits;
        let new = b[62] ^ b[51] ^ b[38] ^ b[23] ^ b[13] ^ b[0];
        self.bits.copy_within(1.., 0);
        self.bits[79] = new;
        new
    }

    /// Self-shrinking output bit.
    fn bit(&mut self) -> bool {
        loop {
            let first = self.step();
            let second = self.step();
            if first {
                return second;
            }
        }
    }

    /// One 255-bit candidate as a big-endian 32-byte string.
    fn candidate(&mut self) -> [u8; 32] {
        let mut out = [0u8; 32];
        // 7 bits in byte 0, then 31 full bytes.
        for bit_index in 0..FIELD_BITS {
            let position = bit_index + 1; // skip the unused top bit of byte 0
            if self.bit() {
                out[position / 8] |= 0x80 >> (position % 8);
            }
        }
        out
    }
}

impl<P: FieldParams> PoseidonParams<P> {
    /// Derives the MDS matrix and the `(8 + 59) * 25` round constants.
    pub fn derive() -> Self {
        let mut mds = Vec::with_capacity(WIDTH * WIDTH);
        for i in 0..WIDTH {
            for j in 0..WIDTH {
                let denominator = Field::<P>::from_u64((i + WIDTH + j) as u64);
                // i + t + j is between 25 and 72, so it is invertible.
                mds.push(denominator.invert().unwrap_or(Field::<P>::ZERO));
            }
        }
        let total = (FULL_ROUNDS + PARTIAL_ROUNDS) * WIDTH;
        let mut grain = Grain::new();
        let mut round_constants = Vec::with_capacity(total);
        while round_constants.len() < total {
            let mut be = grain.candidate();
            be.reverse();
            if let Ok(value) = Field::<P>::from_repr(&be) {
                round_constants.push(value);
            }
        }
        Self {
            mds,
            round_constants,
        }
    }

    /// The MDS entry `M[i][j]`.
    pub fn mds(&self, i: usize, j: usize) -> Field<P> {
        self.mds[i * WIDTH + j]
    }

    /// The flat round-constant list; round `r` uses `[r * 25, (r + 1) * 25)`.
    pub fn round_constants(&self) -> &[Field<P>] {
        &self.round_constants
    }

    fn permute(&self, state: &mut [Field<P>; WIDTH]) {
        let rounds = FULL_ROUNDS + PARTIAL_ROUNDS;
        for round in 0..rounds {
            let constants = &self.round_constants[round * WIDTH..(round + 1) * WIDTH];
            for (value, constant) in state.iter_mut().zip(constants) {
                *value += *constant;
            }
            let partial = (FULL_ROUNDS / 2..FULL_ROUNDS / 2 + PARTIAL_ROUNDS).contains(&round);
            let full = !partial;
            let sbox_count = if full { WIDTH } else { 1 };
            for value in state.iter_mut().take(sbox_count) {
                let square = value.square();
                *value = square.square() * *value;
            }
            let mut next = [Field::<P>::ZERO; WIDTH];
            for (i, out) in next.iter_mut().enumerate() {
                let row = &self.mds[i * WIDTH..(i + 1) * WIDTH];
                let mut sum = Field::<P>::ZERO;
                for (m, s) in row.iter().zip(state.iter()) {
                    sum += *m * *s;
                }
                *out = sum;
            }
            *state = next;
        }
    }

    /// The raw sponge output for `inputs` (`inputs.len() >= 1`; zero inputs use the tag `x`).
    pub fn hash(&self, inputs: &[Field<P>]) -> Field<P> {
        let x: u128 = u128::MAX - 158; // 2^128 - 159
        let count = (inputs.len() as u128).wrapping_add(1u128 << 31);
        let tag = if inputs.is_empty() {
            x
        } else {
            x.wrapping_mul(count).wrapping_add(x.wrapping_mul(x))
        };
        let tag_limbs = [tag as u64, (tag >> 64) as u64, 0, 0];
        let mut state = [Field::<P>::ZERO; WIDTH];
        state[0] = Field::<P>::from_limbs(tag_limbs).unwrap_or(Field::<P>::ZERO);
        let mut position = 0;
        for input in inputs {
            if position == RATE {
                self.permute(&mut state);
                position = 0;
            }
            state[1 + position] += *input;
            position += 1;
        }
        self.permute(&mut state);
        state[1]
    }
}

/// A one-shot random oracle: absorb elements, then squeeze once.
pub struct RandomOracle<'a, P: FieldParams> {
    params: &'a PoseidonParams<P>,
    items: Vec<Field<P>>,
}

impl<'a, P: FieldParams> RandomOracle<'a, P> {
    /// An empty oracle over `params`.
    pub fn new(params: &'a PoseidonParams<P>) -> Self {
        Self {
            params,
            items: Vec::new(),
        }
    }

    /// Appends one element.
    pub fn absorb(&mut self, value: Field<P>) {
        self.items.push(value);
    }

    /// The elements absorbed so far.
    pub fn items(&self) -> &[Field<P>] {
        &self.items
    }

    /// Hashes the list and keeps the low `num_bits` bits of the output (`num_bits <= 250`).
    pub fn squeeze(&self, num_bits: usize) -> Field<P> {
        let raw = self.params.hash(&self.items);
        let mut limbs = raw.to_limbs();
        for (index, limb) in limbs.iter_mut().enumerate() {
            let low = index * 64;
            if num_bits <= low {
                *limb = 0;
            } else if num_bits < low + 64 {
                *limb &= (1u64 << (num_bits - low)) - 1;
            }
        }
        Field::<P>::from_limbs(limbs).unwrap_or(Field::<P>::ZERO)
    }
}
