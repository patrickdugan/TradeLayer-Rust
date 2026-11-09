//! Four-limb Montgomery prime fields for the Pasta cycle.
//!
//! `Field<P>` is generic over a [`FieldParams`] marker that supplies the modulus. Every Montgomery
//! constant (`R`, `R^2`, `R^3`, `-p^-1 mod 2^64`, the 2^S-th root of unity, the exponent used by
//! Tonelli-Shanks) is derived from the modulus by `const fn`s, so the two instantiations share one
//! code path:
//!
//! * [`Fp`]: the Pallas base field and Vesta scalar field,
//!   `p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001`.
//! * [`Fq`]: the Vesta base field and Pallas scalar field,
//!   `q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001`.
//!
//! The behaviour follows halo2curves 0.9.0 exactly: the byte representation is 32 bytes
//! little-endian and canonical-only, `sqrt` is the `ff` Tonelli-Shanks routine (multiplicative
//! generator 5, 2-adicity 32), and `from_uniform_bytes_*` is the little-endian integer reduced
//! modulo the field order.
//!
//! The verifier handles only public data, so the arithmetic is variable time.

use core::fmt;
use core::marker::PhantomData;
use core::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign};

/// Number of 64-bit limbs of an element.
pub const LIMBS: usize = 4;
/// Byte length of a canonical element encoding.
pub const REPR_BYTES: usize = 32;

/// Errors from decoding field elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldError {
    /// The input slice has the wrong length.
    WrongLength { expected: usize, found: usize },
    /// The encoded integer is not smaller than the modulus.
    NonCanonical,
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongLength { expected, found } => {
                write!(
                    f,
                    "field element needs {expected} bytes, found {found} bytes"
                )
            }
            Self::NonCanonical => write!(f, "field element encoding is not below the modulus"),
        }
    }
}

impl std::error::Error for FieldError {}

/// Supplies the prime modulus of a [`Field`].
///
/// The modulus must be an odd prime below `2^255` with multiplicative generator 5 and
/// 2-adicity of at least 4; both Pasta fields satisfy this.
pub trait FieldParams: Copy + Clone + fmt::Debug + PartialEq + Eq + Send + Sync + 'static {
    /// The modulus as little-endian 64-bit limbs.
    const MODULUS: [u64; LIMBS];
    /// A short name used in diagnostics.
    const NAME: &'static str;
}

/// Parameters of the Pallas base field (Vesta scalar field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FpParams;

/// Parameters of the Vesta base field (Pallas scalar field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FqParams;

impl FieldParams for FpParams {
    const MODULUS: [u64; LIMBS] = [
        0x992d30ed00000001,
        0x224698fc094cf91b,
        0x0000000000000000,
        0x4000000000000000,
    ];
    const NAME: &'static str = "Fp";
}

impl FieldParams for FqParams {
    const MODULUS: [u64; LIMBS] = [
        0x8c46eb2100000001,
        0x224698fc0994a8dd,
        0x0000000000000000,
        0x4000000000000000,
    ];
    const NAME: &'static str = "Fq";
}

/// Pallas base field, Vesta scalar field.
pub type Fp = Field<FpParams>;
/// Vesta base field, Pallas scalar field.
pub type Fq = Field<FqParams>;

type Limbs = [u64; LIMBS];

const fn geq(a: &Limbs, b: &Limbs) -> bool {
    let mut i = LIMBS;
    while i > 0 {
        i -= 1;
        if a[i] > b[i] {
            return true;
        }
        if a[i] < b[i] {
            return false;
        }
    }
    true
}

/// `a - b` with the final borrow.
const fn sbb_all(a: &Limbs, b: &Limbs) -> (Limbs, bool) {
    let mut out = [0u64; LIMBS];
    let mut borrow = 0u64;
    let mut i = 0;
    while i < LIMBS {
        let (d1, b1) = a[i].overflowing_sub(b[i]);
        let (d2, b2) = d1.overflowing_sub(borrow);
        out[i] = d2;
        borrow = (b1 | b2) as u64;
        i += 1;
    }
    (out, borrow != 0)
}

/// `a + b` with the final carry.
const fn adc_all(a: &Limbs, b: &Limbs) -> (Limbs, bool) {
    let mut out = [0u64; LIMBS];
    let mut carry = 0u64;
    let mut i = 0;
    while i < LIMBS {
        let (s1, c1) = a[i].overflowing_add(b[i]);
        let (s2, c2) = s1.overflowing_add(carry);
        out[i] = s2;
        carry = (c1 | c2) as u64;
        i += 1;
    }
    (out, carry != 0)
}

const fn add_mod(a: &Limbs, b: &Limbs, p: &Limbs) -> Limbs {
    let (sum, carry) = adc_all(a, b);
    if carry || geq(&sum, p) {
        sbb_all(&sum, p).0
    } else {
        sum
    }
}

const fn sub_mod(a: &Limbs, b: &Limbs, p: &Limbs) -> Limbs {
    let (diff, borrow) = sbb_all(a, b);
    if borrow {
        adc_all(&diff, p).0
    } else {
        diff
    }
}

/// `-p^-1 mod 2^64` by Newton iteration.
const fn montgomery_inv(p0: u64) -> u64 {
    let mut inv = 1u64;
    let mut i = 0;
    while i < 63 {
        inv = inv.wrapping_mul(inv);
        inv = inv.wrapping_mul(p0);
        i += 1;
    }
    inv.wrapping_neg()
}

/// Montgomery product `a * b / 2^256 mod p` (coarsely integrated operand scanning). The result is
/// fully reduced when `a * b < 2^256 * p`.
const fn mont_mul(a: &Limbs, b: &Limbs, p: &Limbs, inv: u64) -> Limbs {
    let mut t = [0u64; LIMBS + 2];
    let mut i = 0;
    while i < LIMBS {
        let mut carry = 0u128;
        let mut j = 0;
        while j < LIMBS {
            let s = t[j] as u128 + (a[j] as u128) * (b[i] as u128) + carry;
            t[j] = s as u64;
            carry = s >> 64;
            j += 1;
        }
        let s = t[LIMBS] as u128 + carry;
        t[LIMBS] = s as u64;
        t[LIMBS + 1] = (s >> 64) as u64;

        let m = t[0].wrapping_mul(inv);
        let s = t[0] as u128 + (m as u128) * (p[0] as u128);
        let mut carry = s >> 64;
        let mut j = 1;
        while j < LIMBS {
            let s = t[j] as u128 + (m as u128) * (p[j] as u128) + carry;
            t[j - 1] = s as u64;
            carry = s >> 64;
            j += 1;
        }
        let s = t[LIMBS] as u128 + carry;
        t[LIMBS - 1] = s as u64;
        t[LIMBS] = t[LIMBS + 1] + (s >> 64) as u64;
        t[LIMBS + 1] = 0;
        i += 1;
    }
    let r = [t[0], t[1], t[2], t[3]];
    if t[LIMBS] != 0 || geq(&r, p) {
        sbb_all(&r, p).0
    } else {
        r
    }
}

/// Montgomery product for operands below the modulus when the modulus is below `2^255`
/// (the "no-carry" variant: the running sum never overflows four limbs). This is the hot path of
/// every field multiplication.
const fn mont_mul_reduced(a: &Limbs, b: &Limbs, p: &Limbs, inv: u64) -> Limbs {
    let mut t = [0u64; LIMBS];
    let mut i = 0;
    while i < LIMBS {
        let bi = b[i] as u128;
        let s = t[0] as u128 + (a[0] as u128) * bi;
        let mut c1 = (s >> 64) as u64;
        let t0 = s as u64;
        let m = t0.wrapping_mul(inv);
        let s2 = t0 as u128 + (m as u128) * (p[0] as u128);
        let mut c2 = (s2 >> 64) as u64;
        let mut j = 1;
        while j < LIMBS {
            let s = t[j] as u128 + (a[j] as u128) * bi + c1 as u128;
            c1 = (s >> 64) as u64;
            let s2 = (s as u64) as u128 + (m as u128) * (p[j] as u128) + c2 as u128;
            c2 = (s2 >> 64) as u64;
            t[j - 1] = s2 as u64;
            j += 1;
        }
        t[LIMBS - 1] = c1 + c2;
        i += 1;
    }
    if geq(&t, p) {
        sbb_all(&t, p).0
    } else {
        t
    }
}

/// `2^k * x mod p` for reduced `x` by repeated doubling.
const fn double_n(x: Limbs, k: usize, p: &Limbs) -> Limbs {
    let mut out = x;
    let mut i = 0;
    while i < k {
        out = add_mod(&out, &out, p);
        i += 1;
    }
    out
}

const fn is_zero_limbs(a: &Limbs) -> bool {
    a[0] | a[1] | a[2] | a[3] == 0
}

/// `base^exp` in Montgomery form, exponent as little-endian limbs.
const fn mont_pow(base: &Limbs, exp: &Limbs, one: &Limbs, p: &Limbs, inv: u64) -> Limbs {
    let mut acc = *one;
    let mut i = LIMBS * 64;
    while i > 0 {
        i -= 1;
        acc = mont_mul(&acc, &acc, p, inv);
        if (exp[i / 64] >> (i % 64)) & 1 == 1 {
            acc = mont_mul(&acc, base, p, inv);
        }
    }
    acc
}

const fn shr1(a: &Limbs) -> Limbs {
    [
        (a[0] >> 1) | (a[1] << 63),
        (a[1] >> 1) | (a[2] << 63),
        (a[2] >> 1) | (a[3] << 63),
        a[3] >> 1,
    ]
}

/// Two-adicity `S` of `p - 1` and the odd part `t = (p - 1) >> S`.
const fn two_adicity(p: &Limbs) -> (u32, Limbs) {
    let mut t = sbb_all(p, &[1, 0, 0, 0]).0;
    let mut s = 0u32;
    while t[0] & 1 == 0 {
        t = shr1(&t);
        s += 1;
    }
    (s, t)
}

/// An element of the prime field defined by `P`, stored in Montgomery form and always fully
/// reduced, so derived equality is value equality.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Field<P: FieldParams> {
    limbs: Limbs,
    _params: PhantomData<P>,
}

impl<P: FieldParams> fmt::Debug for Field<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = self.to_repr();
        write!(f, "{}(0x", P::NAME)?;
        for byte in repr.iter().rev() {
            write!(f, "{byte:02x}")?;
        }
        write!(f, ")")
    }
}

impl<P: FieldParams> Default for Field<P> {
    fn default() -> Self {
        Self::ZERO
    }
}

impl<P: FieldParams> Field<P> {
    /// The modulus as little-endian limbs.
    pub const MODULUS: Limbs = P::MODULUS;
    /// The 2-adicity of the multiplicative group (`modulus - 1 = 2^S * t`, `t` odd).
    pub const S: u32 = two_adicity(&P::MODULUS).0;

    const INV: u64 = montgomery_inv(P::MODULUS[0]);
    const R1: Limbs = double_n([1, 0, 0, 0], 256, &P::MODULUS);
    const R2: Limbs = double_n(Self::R1, 256, &P::MODULUS);
    const R3: Limbs = double_n(Self::R2, 256, &P::MODULUS);
    /// `(t - 1) / 2` where `modulus - 1 = 2^S * t`.
    const T_MINUS_1_OVER_2: Limbs = shr1(&sbb_all(&two_adicity(&P::MODULUS).1, &[1, 0, 0, 0]).0);
    /// `modulus - 2`, the Fermat inversion exponent.
    const P_MINUS_2: Limbs = sbb_all(&P::MODULUS, &[2, 0, 0, 0]).0;
    /// `(modulus - 1) / 2`, the Legendre exponent.
    const P_MINUS_1_OVER_2: Limbs = shr1(&sbb_all(&P::MODULUS, &[1, 0, 0, 0]).0);
    /// Multiplicative generator 5 in Montgomery form.
    const GENERATOR: Limbs = mont_mul(&[5, 0, 0, 0], &Self::R2, &P::MODULUS, Self::INV);
    /// `GENERATOR^t`, a primitive `2^S`-th root of unity in Montgomery form.
    const ROOT_OF_UNITY_LIMBS: Limbs = mont_pow(
        &Self::GENERATOR,
        &two_adicity(&P::MODULUS).1,
        &Self::R1,
        &P::MODULUS,
        Self::INV,
    );

    /// The additive identity.
    pub const ZERO: Self = Self {
        limbs: [0; LIMBS],
        _params: PhantomData,
    };
    /// The multiplicative identity.
    pub const ONE: Self = Self {
        limbs: Self::R1,
        _params: PhantomData,
    };
    /// A primitive `2^S`-th root of unity (`5^t`, halo2curves `ROOT_OF_UNITY`).
    pub const ROOT_OF_UNITY: Self = Self {
        limbs: Self::ROOT_OF_UNITY_LIMBS,
        _params: PhantomData,
    };

    const fn raw(limbs: Limbs) -> Self {
        Self {
            limbs,
            _params: PhantomData,
        }
    }

    /// Converts a small integer.
    pub const fn from_u64(value: u64) -> Self {
        Self::raw(mont_mul(
            &[value, 0, 0, 0],
            &Self::R2,
            &P::MODULUS,
            Self::INV,
        ))
    }

    /// Builds an element from canonical little-endian limbs; `None` if `limbs >= modulus`.
    pub fn from_limbs(limbs: [u64; LIMBS]) -> Option<Self> {
        if geq(&limbs, &P::MODULUS) {
            return None;
        }
        Some(Self::raw(mont_mul(
            &limbs,
            &Self::R2,
            &P::MODULUS,
            Self::INV,
        )))
    }

    /// The canonical (non-Montgomery) little-endian limbs of the value.
    pub fn to_limbs(&self) -> [u64; LIMBS] {
        mont_mul(&self.limbs, &[1, 0, 0, 0], &P::MODULUS, Self::INV)
    }

    /// Decodes a canonical 32-byte little-endian representation.
    pub fn from_repr(bytes: &[u8; REPR_BYTES]) -> Result<Self, FieldError> {
        let mut limbs = [0u64; LIMBS];
        for (limb, chunk) in limbs.iter_mut().zip(bytes.chunks_exact(8)) {
            let mut word = [0u8; 8];
            word.copy_from_slice(chunk);
            *limb = u64::from_le_bytes(word);
        }
        Self::from_limbs(limbs).ok_or(FieldError::NonCanonical)
    }

    /// Decodes a canonical representation from a slice of exactly 32 bytes.
    pub fn from_repr_slice(bytes: &[u8]) -> Result<Self, FieldError> {
        let array: &[u8; REPR_BYTES] = bytes.try_into().map_err(|_| FieldError::WrongLength {
            expected: REPR_BYTES,
            found: bytes.len(),
        })?;
        Self::from_repr(array)
    }

    /// The canonical 32-byte little-endian representation.
    pub fn to_repr(&self) -> [u8; REPR_BYTES] {
        let limbs = self.to_limbs();
        let mut out = [0u8; REPR_BYTES];
        for (chunk, limb) in out.chunks_exact_mut(8).zip(limbs) {
            chunk.copy_from_slice(&limb.to_le_bytes());
        }
        out
    }

    /// Reduces a little-endian 64-byte string modulo the field order
    /// (halo2curves `FromUniformBytes<64>`).
    pub fn from_uniform_bytes_64(bytes: &[u8; 64]) -> Self {
        let (low, high) = Self::wide_limbs(bytes);
        Self::combine_wide(low, high)
    }

    /// Reduces a little-endian 48-byte string modulo the field order
    /// (halo2curves `FromUniformBytes<48>`).
    pub fn from_uniform_bytes_48(bytes: &[u8; 48]) -> Self {
        let mut wide = [0u8; 64];
        wide[..48].copy_from_slice(bytes);
        Self::from_uniform_bytes_64(&wide)
    }

    fn wide_limbs(bytes: &[u8; 64]) -> (Limbs, Limbs) {
        let mut low = [0u64; LIMBS];
        let mut high = [0u64; LIMBS];
        for (i, chunk) in bytes.chunks_exact(8).enumerate() {
            let mut word = [0u8; 8];
            word.copy_from_slice(chunk);
            let value = u64::from_le_bytes(word);
            if i < LIMBS {
                low[i] = value;
            } else {
                high[i - LIMBS] = value;
            }
        }
        (low, high)
    }

    /// `low + high * 2^256` as a field element. The operands may exceed the modulus; the
    /// Montgomery products by `R^2` and `R^3` reduce them (`low * R^2 / R = low * R`).
    fn combine_wide(low: Limbs, high: Limbs) -> Self {
        let a = Self::raw(mont_mul(&low, &Self::R2, &P::MODULUS, Self::INV));
        let b = Self::raw(mont_mul(&high, &Self::R3, &P::MODULUS, Self::INV));
        a + b
    }

    /// Whether the value is zero.
    pub fn is_zero(&self) -> bool {
        is_zero_limbs(&self.limbs)
    }

    /// Whether the canonical representative is odd (halo2curves `is_odd`).
    pub fn is_odd(&self) -> bool {
        self.to_limbs()[0] & 1 == 1
    }

    /// `2 * self`.
    pub fn double(&self) -> Self {
        *self + *self
    }

    /// `self^2`.
    pub fn square(&self) -> Self {
        *self * *self
    }

    /// `self^exp` for a little-endian multi-limb exponent (any length).
    pub fn pow(&self, exp: &[u64]) -> Self {
        let mut acc = Self::ONE;
        for limb in exp.iter().rev() {
            for bit in (0..64).rev() {
                acc = acc.square();
                if (limb >> bit) & 1 == 1 {
                    acc *= *self;
                }
            }
        }
        acc
    }

    /// The multiplicative inverse, or `None` for zero.
    pub fn invert(&self) -> Option<Self> {
        if self.is_zero() {
            None
        } else {
            Some(self.pow(&Self::P_MINUS_2))
        }
    }

    /// The Legendre symbol: 0 for zero, 1 for a nonzero square, -1 for a non-square.
    pub fn legendre(&self) -> i8 {
        if self.is_zero() {
            return 0;
        }
        if self.pow(&Self::P_MINUS_1_OVER_2) == Self::ONE {
            1
        } else {
            -1
        }
    }

    /// Whether the value is a square (zero counts as a square).
    pub fn is_square(&self) -> bool {
        self.legendre() != -1
    }

    /// A square root chosen exactly as halo2curves does: the `ff` Tonelli-Shanks routine for
    /// `p ≡ 1 (mod 16)` with the root of unity `5^t`. `None` if the value is a non-square.
    pub fn sqrt(&self) -> Option<Self> {
        let w = self.pow(&Self::T_MINUS_1_OVER_2);
        let mut v = Self::S;
        let mut x = w * *self;
        let mut b = x * w;
        let mut z = Self::ROOT_OF_UNITY;

        for max_v in (1..=Self::S).rev() {
            let mut k = 1u32;
            let mut b2k = b.square();
            let mut j_less_than_v = true;
            for j in 2..max_v {
                let b2k_is_one = b2k == Self::ONE;
                let squared = if b2k_is_one { z } else { b2k }.square();
                if !b2k_is_one {
                    b2k = squared;
                }
                let new_z = if b2k_is_one { squared } else { z };
                j_less_than_v &= j != v;
                if !b2k_is_one {
                    k = j;
                }
                if j_less_than_v {
                    z = new_z;
                }
            }
            let result = x * z;
            if b != Self::ONE {
                x = result;
            }
            z = z.square();
            b *= z;
            v = k;
        }

        if x.square() == *self {
            Some(x)
        } else {
            None
        }
    }
}

impl<P: FieldParams> Add for Field<P> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::raw(add_mod(&self.limbs, &rhs.limbs, &P::MODULUS))
    }
}

impl<P: FieldParams> Sub for Field<P> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::raw(sub_mod(&self.limbs, &rhs.limbs, &P::MODULUS))
    }
}

impl<P: FieldParams> Mul for Field<P> {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::raw(mont_mul_reduced(
            &self.limbs,
            &rhs.limbs,
            &P::MODULUS,
            Self::INV,
        ))
    }
}

impl<P: FieldParams> Neg for Field<P> {
    type Output = Self;
    fn neg(self) -> Self {
        if self.is_zero() {
            self
        } else {
            Self::raw(sbb_all(&P::MODULUS, &self.limbs).0)
        }
    }
}

impl<P: FieldParams> AddAssign for Field<P> {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl<P: FieldParams> SubAssign for Field<P> {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl<P: FieldParams> MulAssign for Field<P> {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}
