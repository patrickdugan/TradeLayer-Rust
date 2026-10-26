//! Public-input secp256k1 ECDSA verification. Fixed-size, variable-time math;
//! no private-key operation, signing, hashing or transaction authorization.
use core::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecpError {
    InvalidPublicKey,
    InvalidSignature,
    Arithmetic,
}
impl core::fmt::Display for SecpError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SecpError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Uint([u64; 4]);
impl Uint {
    const ZERO: Self = Self([0; 4]);
    const ONE: Self = Self([1, 0, 0, 0]);
    fn from_be(bytes: &[u8; 32]) -> Self {
        let mut limbs = [0; 4];
        for (i, limb) in limbs.iter_mut().enumerate() {
            let mut word = [0; 8];
            word.copy_from_slice(&bytes[(3 - i) * 8..(4 - i) * 8]);
            *limb = u64::from_be_bytes(word);
        }
        Self(limbs)
    }
    fn to_be(self) -> [u8; 32] {
        let mut out = [0; 32];
        for (i, limb) in self.0.iter().enumerate() {
            out[(3 - i) * 8..(4 - i) * 8].copy_from_slice(&limb.to_be_bytes());
        }
        out
    }
    fn compare(self, other: Self) -> Ordering {
        for i in (0..4).rev() {
            let order = self.0[i].cmp(&other.0[i]);
            if order != Ordering::Equal {
                return order;
            }
        }
        Ordering::Equal
    }
    fn bit(self, bit: usize) -> bool {
        self.0[bit / 64] & (1u64 << (bit % 64)) != 0
    }
    fn subtract(self, other: Self) -> (Self, bool) {
        let mut out = [0; 4];
        let mut borrow = false;
        for (i, word) in out.iter_mut().enumerate() {
            let (a, b1) = self.0[i].overflowing_sub(other.0[i]);
            let (b, b2) = a.overflowing_sub(u64::from(borrow));
            *word = b;
            borrow = b1 || b2;
        }
        (Self(out), borrow)
    }
}
const P: Uint = Uint([
    0xfffffffefffffc2f,
    0xffffffffffffffff,
    0xffffffffffffffff,
    0xffffffffffffffff,
]);
const N: Uint = Uint([
    0xbfd25e8cd0364141,
    0xbaaedce6af48a03b,
    0xfffffffffffffffe,
    0xffffffffffffffff,
]);
const HALF_N: Uint = Uint([
    0xdfe92f46681b20a0,
    0x5d576e7357a4501d,
    0xffffffffffffffff,
    0x7fffffffffffffff,
]);
const SQRT_EXP: Uint = Uint([
    0xffffffffbfffff0c,
    0xffffffffffffffff,
    0xffffffffffffffff,
    0x3fffffffffffffff,
]);
struct Modulus {
    modulus: Uint,
    r2: Uint,
    one: Uint,
    neg_inverse: u64,
}
const FIELD: Modulus = Modulus {
    modulus: P,
    r2: Uint([0x000007a2000e90a1, 1, 0, 0]),
    one: Uint([0x1000003d1, 0, 0, 0]),
    neg_inverse: 0xd838091dd2253531,
};
const SCALAR: Modulus = Modulus {
    modulus: N,
    r2: Uint([
        0x896cf21467d7d140,
        0x741496c20e7cf878,
        0xe697f5e45bcd07c6,
        0x9d671cd581c69bc5,
    ]),
    one: Uint([0x402da1732fc9bebf, 0x4551231950b75fc4, 1, 0]),
    neg_inverse: 0x4b0dff665588b13f,
};
impl Modulus {
    fn reduce_once(&self, a: Uint) -> Uint {
        if a.compare(self.modulus) != Ordering::Less {
            a.subtract(self.modulus).0
        } else {
            a
        }
    }
    fn add(&self, a: Uint, b: Uint) -> Uint {
        let mut out = [0; 4];
        let mut carry = 0u128;
        for (i, word) in out.iter_mut().enumerate() {
            let sum = u128::from(a.0[i]) + u128::from(b.0[i]) + carry;
            *word = sum as u64;
            carry = sum >> 64;
        }
        let value = Uint(out);
        if carry != 0 || value.compare(self.modulus) != Ordering::Less {
            value.subtract(self.modulus).0
        } else {
            value
        }
    }
    fn sub(&self, a: Uint, b: Uint) -> Uint {
        let (difference, borrow) = a.subtract(b);
        if borrow {
            let mut out = [0; 4];
            let mut carry = 0u128;
            for (i, word) in out.iter_mut().enumerate() {
                let sum = u128::from(difference.0[i]) + u128::from(self.modulus.0[i]) + carry;
                *word = sum as u64;
                carry = sum >> 64;
            }
            Uint(out)
        } else {
            difference
        }
    }
    /// REDC(a*b), R=2^256, a,b<m. Each inner accumulation is at most
    /// (2^64-1)^2+2*(2^64-1)=2^128-1. Nine words retain every carry.
    fn mul(&self, a: Uint, b: Uint) -> Uint {
        let mut t = [0u64; 9];
        for i in 0..4 {
            let mut carry = 0u128;
            for j in 0..4 {
                let product =
                    u128::from(a.0[i]) * u128::from(b.0[j]) + u128::from(t[i + j]) + carry;
                t[i + j] = product as u64;
                carry = product >> 64;
            }
            for word in &mut t[i + 4..] {
                let sum = u128::from(*word) + carry;
                *word = sum as u64;
                carry = sum >> 64;
            }
            debug_assert_eq!(carry, 0);
        }
        for i in 0..4 {
            let q = t[i].wrapping_mul(self.neg_inverse);
            let mut carry = 0u128;
            for j in 0..4 {
                let product =
                    u128::from(q) * u128::from(self.modulus.0[j]) + u128::from(t[i + j]) + carry;
                t[i + j] = product as u64;
                carry = product >> 64;
            }
            for word in &mut t[i + 4..] {
                let sum = u128::from(*word) + carry;
                *word = sum as u64;
                carry = sum >> 64;
            }
            debug_assert_eq!(carry, 0);
            debug_assert_eq!(t[i], 0);
        }
        // a*b<m^2 implies REDC(a*b)<2m, so at most one subtraction.
        debug_assert!(t[8] <= 1);
        let value = Uint([t[4], t[5], t[6], t[7]]);
        if t[8] != 0 || value.compare(self.modulus) != Ordering::Less {
            value.subtract(self.modulus).0
        } else {
            value
        }
    }
    fn encode(&self, a: Uint) -> Uint {
        self.mul(self.reduce_once(a), self.r2)
    }
    fn decode(&self, a: Uint) -> Uint {
        self.mul(a, Uint::ONE)
    }
    fn pow(&self, a: Uint, exponent: Uint) -> Uint {
        let mut out = self.one;
        for bit in (0..256).rev() {
            out = self.mul(out, out);
            if exponent.bit(bit) {
                out = self.mul(out, a);
            }
        }
        out
    }
    fn inverse(&self, a: Uint) -> Option<Uint> {
        if a == Uint::ZERO {
            None
        } else {
            Some(self.pow(a, self.modulus.subtract(Uint([2, 0, 0, 0])).0))
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Field(Uint);
impl Field {
    const ZERO: Self = Self(Uint::ZERO);
    const ONE: Self = Self(FIELD.one);
    fn from_raw(a: Uint) -> Self {
        Self(FIELD.encode(a))
    }
    fn raw(self) -> Uint {
        FIELD.decode(self.0)
    }
    fn add(self, b: Self) -> Self {
        Self(FIELD.add(self.0, b.0))
    }
    fn sub(self, b: Self) -> Self {
        Self(FIELD.sub(self.0, b.0))
    }
    fn mul(self, b: Self) -> Self {
        Self(FIELD.mul(self.0, b.0))
    }
    fn square(self) -> Self {
        self.mul(self)
    }
    fn double(self) -> Self {
        self.add(self)
    }
    fn neg(self) -> Self {
        Self::ZERO.sub(self)
    }
    fn inverse(self) -> Option<Self> {
        FIELD.inverse(self.0).map(Self)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Point {
    x: Field,
    y: Field,
    z: Field,
}
impl Point {
    const INFINITY: Self = Self {
        x: Field::ZERO,
        y: Field::ONE,
        z: Field::ZERO,
    };
    fn affine(x: Field, y: Field) -> Self {
        Self {
            x,
            y,
            z: Field::ONE,
        }
    }
    fn generator() -> Self {
        Self::affine(
            Field::from_raw(Uint([
                0x59f2815b16f81798,
                0x029bfcdb2dce28d9,
                0x55a06295ce870b07,
                0x79be667ef9dcbbac,
            ])),
            Field::from_raw(Uint([
                0x9c47d08ffb10d4b8,
                0xfd17b448a6855419,
                0x5da4fbfc0e1108a8,
                0x483ada7726a3c465,
            ])),
        )
    }
    fn is_infinity(self) -> bool {
        self.z == Field::ZERO
    }
    fn double(self) -> Self {
        if self.is_infinity() || self.y == Field::ZERO {
            return Self::INFINITY;
        }
        let a = self.x.square();
        let b = self.y.square();
        let c = b.square();
        let d = self.x.add(b).square().sub(a).sub(c).double();
        let e = a.double().add(a);
        let f = e.square();
        let x = f.sub(d.double());
        let y = e.mul(d.sub(x)).sub(c.double().double().double());
        let z = self.y.mul(self.z).double();
        Self { x, y, z }
    }
    fn add(self, other: Self) -> Self {
        if self.is_infinity() {
            return other;
        }
        if other.is_infinity() {
            return self;
        }
        let z1z1 = self.z.square();
        let z2z2 = other.z.square();
        let u1 = self.x.mul(z2z2);
        let u2 = other.x.mul(z1z1);
        let s1 = self.y.mul(other.z).mul(z2z2);
        let s2 = other.y.mul(self.z).mul(z1z1);
        if u1 == u2 {
            return if s1 == s2 {
                self.double()
            } else {
                Self::INFINITY
            };
        }
        let h = u2.sub(u1);
        let i = h.double().square();
        let j = h.mul(i);
        let r = s2.sub(s1).double();
        let v = u1.mul(i);
        let x = r.square().sub(j).sub(v.double());
        let y = r.mul(v.sub(x)).sub(s1.mul(j).double());
        let z = self.z.add(other.z).square().sub(z1z1).sub(z2z2).mul(h);
        Self { x, y, z }
    }
    fn x_affine(self) -> Option<Uint> {
        let inverse = self.z.inverse()?;
        Some(self.x.mul(inverse.square()).raw())
    }
}
fn parse_key(key: &[u8]) -> Result<Point, SecpError> {
    let mut xb = [0; 32];
    let mut yb = [0; 32];
    let compressed = match (key.len(), key.first()) {
        (33, Some(2 | 3)) => true,
        (65, Some(4)) => false,
        _ => return Err(SecpError::InvalidPublicKey),
    };
    xb.copy_from_slice(&key[1..33]);
    let xr = Uint::from_be(&xb);
    if xr.compare(P) != Ordering::Less {
        return Err(SecpError::InvalidPublicKey);
    }
    let x = Field::from_raw(xr);
    let rhs = x.square().mul(x).add(Field::from_raw(Uint([7, 0, 0, 0])));
    let y = if compressed {
        let mut y = Field(FIELD.pow(rhs.0, SQRT_EXP));
        if y.square() != rhs {
            return Err(SecpError::InvalidPublicKey);
        }
        if y.raw().0[0] & 1 != u64::from(key[0] & 1) {
            y = y.neg();
        }
        if y.raw().0[0] & 1 != u64::from(key[0] & 1) {
            return Err(SecpError::InvalidPublicKey);
        }
        y
    } else {
        yb.copy_from_slice(&key[33..65]);
        let yr = Uint::from_be(&yb);
        if yr.compare(P) != Ordering::Less {
            return Err(SecpError::InvalidPublicKey);
        }
        let y = Field::from_raw(yr);
        if y.square() != rhs {
            return Err(SecpError::InvalidPublicKey);
        }
        y
    };
    // SEC2's secp256k1 has cofactor one and prime order n: every finite curve
    // point is in the required subgroup. Infinity has no accepted encoding.
    Ok(Point::affine(x, y))
}

/// Strict SEC1 compressed/uncompressed parsing and canonical compressed output.
/// Hybrid, x-only, infinity and noncanonical field coordinates are rejected.
pub fn canonical_compressed_public_key(public_key: &[u8]) -> Result<[u8; 33], SecpError> {
    let point = parse_key(public_key)?;
    let mut output = [0; 33];
    output[0] = 2 | (point.y.raw().0[0] as u8 & 1);
    output[1..].copy_from_slice(&point.x.raw().to_be());
    Ok(output)
}
/// ECDSA over an already-computed 32-byte digest. Like tiny-secp256k1's
/// default verify, mathematically valid high-S signatures are accepted.
/// Noncanonical or zero signature scalars are explicit parse errors.
pub fn verify_ecdsa_digest(
    digest: &[u8; 32],
    public_key: &[u8],
    compact_signature: &[u8; 64],
) -> Result<bool, SecpError> {
    verify_inner(digest, public_key, compact_signature, false)
}
/// The same operation with S<=floor(n/2). High R is not restricted.
pub fn verify_ecdsa_digest_strict(
    digest: &[u8; 32],
    public_key: &[u8],
    compact_signature: &[u8; 64],
) -> Result<bool, SecpError> {
    verify_inner(digest, public_key, compact_signature, true)
}
fn verify_inner(
    digest: &[u8; 32],
    key: &[u8],
    signature: &[u8; 64],
    low_s: bool,
) -> Result<bool, SecpError> {
    let q = parse_key(key)?;
    let mut rb = [0; 32];
    let mut sb = [0; 32];
    rb.copy_from_slice(&signature[..32]);
    sb.copy_from_slice(&signature[32..]);
    let r = Uint::from_be(&rb);
    let s = Uint::from_be(&sb);
    if r == Uint::ZERO
        || s == Uint::ZERO
        || r.compare(N) != Ordering::Less
        || s.compare(N) != Ordering::Less
    {
        return Err(SecpError::InvalidSignature);
    }
    if low_s && s.compare(HALF_N) == Ordering::Greater {
        return Ok(false);
    }
    let inverse = SCALAR
        .inverse(SCALAR.encode(s))
        .ok_or(SecpError::Arithmetic)?;
    let u1 = SCALAR.decode(SCALAR.mul(SCALAR.encode(Uint::from_be(digest)), inverse));
    let u2 = SCALAR.decode(SCALAR.mul(SCALAR.encode(r), inverse));
    let g = Point::generator();
    let sum = g.add(q);
    let mut result = Point::INFINITY;
    // Joint double-and-add shares doublings and uses only fixed-sized stack
    // values. Branches depend exclusively on public verification inputs.
    for bit in (0..256).rev() {
        result = result.double();
        result = match (u1.bit(bit), u2.bit(bit)) {
            (true, true) => result.add(sum),
            (true, false) => result.add(g),
            (false, true) => result.add(q),
            (false, false) => result,
        };
    }
    let Some(x) = result.x_affine() else {
        return Ok(false);
    };
    Ok(SCALAR.reduce_once(x) == r)
}

#[cfg(test)]
mod arithmetic_tests {
    use super::*;
    fn parse(raw: &str) -> Uint {
        let mut bytes = [0; 32];
        for (i, pair) in raw.as_bytes().chunks_exact(2).enumerate() {
            bytes[i] = u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap();
        }
        Uint::from_be(&bytes)
    }
    #[test]
    fn independent_bigint_modular_corpus() {
        let mut count = 0;
        for line in include_str!("../fixtures/secp256k1_vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("M\t"))
        {
            let f: Vec<_> = line.split('\t').collect();
            let modulus = if f[2] == "p" { &FIELD } else { &SCALAR };
            let a = parse(f[3]);
            let b = parse(f[4]);
            let am = modulus.encode(a);
            let bm = modulus.encode(b);
            assert_eq!(modulus.decode(am), a, "{} encoding", f[1]);
            assert_eq!(
                modulus.decode(modulus.add(am, bm)),
                parse(f[5]),
                "{} add",
                f[1]
            );
            assert_eq!(
                modulus.decode(modulus.sub(am, bm)),
                parse(f[6]),
                "{} sub",
                f[1]
            );
            assert_eq!(
                modulus.decode(modulus.mul(am, bm)),
                parse(f[7]),
                "{} mul",
                f[1]
            );
            if f[8] != "-" {
                assert_eq!(
                    modulus.decode(modulus.inverse(am).unwrap()),
                    parse(f[8]),
                    "{} inv",
                    f[1]
                );
            }
            count += 1;
        }
        assert_eq!(count, 1090);
    }
    #[test]
    fn group_exceptional_cases() {
        let g = Point::generator();
        let negative = Point {
            x: g.x,
            y: g.y.neg(),
            z: g.z,
        };
        assert!(g.add(negative).is_infinity());
        assert_eq!(g.add(Point::INFINITY), g);
        assert_eq!(Point::INFINITY.add(g), g);
        assert_eq!(g.add(g), g.double());
        assert!(Point::INFINITY.double().is_infinity());
        let mut n_g = Point::INFINITY;
        for bit in (0..256).rev() {
            n_g = n_g.double();
            if N.bit(bit) {
                n_g = n_g.add(g);
            }
        }
        assert!(n_g.is_infinity());
        // Projectively equivalent inputs trigger the equal-point branch.
        let z = Field::from_raw(Uint([17, 0, 0, 0]));
        let alternate = Point {
            x: g.x.mul(z.square()),
            y: g.y.mul(z.square().mul(z)),
            z,
        };
        assert_eq!(alternate.x_affine(), g.x_affine());
        assert_eq!(alternate.add(g).x_affine(), g.double().x_affine());
    }
}
