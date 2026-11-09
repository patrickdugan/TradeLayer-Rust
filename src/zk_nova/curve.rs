//! The Pasta curves `y^2 = x^3 + 5`.
//!
//! * [`Pallas`]: base field [`Fp`], scalar field [`Fq`].
//! * [`Vesta`]: base field [`Fq`], scalar field [`Fp`].
//!
//! Both have prime order (cofactor 1), so every on-curve point is in the group. [`Affine`] points
//! use `(0, 0)` for the identity, as halo2curves does (the point `(0, 0)` is not on the curve).
//! [`Projective`] points are Jacobian triples `(X, Y, Z)` with `x = X / Z^2`, `y = Y / Z^3`, and
//! the identity is any triple with `Z = 0`. All arithmetic is variable time.
//!
//! # Encodings
//!
//! The compressed form is the halo2curves 0.9.0 `GroupEncoding` (also what nova-snark's bincode
//! writes for a point): 32 bytes, the little-endian `x` coordinate, with bit 7 of byte 31 set when
//! the canonical `y` is odd. The identity is 32 zero bytes. Decoding rejects a non-canonical `x`
//! and an `x` with no point on the curve; like halo2curves it accepts any encoding whose `x` is
//! zero as the identity, regardless of the sign bit.
//!
//! The uncompressed form is 64 bytes, `x || y`, each 32 bytes little-endian, the identity being
//! 64 zero bytes. Decoding checks canonical coordinates and the curve equation.

use core::fmt;
use core::marker::PhantomData;
use core::ops::{Add, Neg, Sub};

use super::field::{Field, FieldError, FieldParams, FpParams, FqParams, LIMBS, REPR_BYTES};

/// Byte length of a compressed point.
pub const COMPRESSED_BYTES: usize = REPR_BYTES;
/// Byte length of an uncompressed point.
pub const UNCOMPRESSED_BYTES: usize = 2 * REPR_BYTES;

/// Errors from decoding curve points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveError {
    /// The input has the wrong length.
    WrongLength { expected: usize, found: usize },
    /// A coordinate is not smaller than the base field modulus.
    NonCanonical,
    /// The coordinates do not satisfy `y^2 = x^3 + 5`.
    NotOnCurve,
}

impl fmt::Display for CurveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongLength { expected, found } => {
                write!(f, "point needs {expected} bytes, found {found} bytes")
            }
            Self::NonCanonical => write!(f, "point coordinate is not below the field modulus"),
            Self::NotOnCurve => write!(f, "point is not on the curve"),
        }
    }
}

impl std::error::Error for CurveError {}

impl From<FieldError> for CurveError {
    fn from(error: FieldError) -> Self {
        match error {
            FieldError::WrongLength { expected, found } => Self::WrongLength { expected, found },
            FieldError::NonCanonical => Self::NonCanonical,
        }
    }
}

/// A Pasta curve `y^2 = x^3 + 5` over `Base` with prime group order equal to the `Scalar` modulus.
pub trait Curve: Copy + Clone + fmt::Debug + PartialEq + Eq + Send + Sync + 'static {
    /// Parameters of the coordinate field.
    type Base: FieldParams;
    /// Parameters of the scalar field (the group order).
    type Scalar: FieldParams;
    /// A short name used in diagnostics.
    const NAME: &'static str;
}

/// The Pallas curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pallas;

/// The Vesta curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vesta;

impl Curve for Pallas {
    type Base = FpParams;
    type Scalar = FqParams;
    const NAME: &'static str = "Pallas";
}

impl Curve for Vesta {
    type Base = FqParams;
    type Scalar = FpParams;
    const NAME: &'static str = "Vesta";
}

/// Coordinate field element of curve `C`.
pub type BaseOf<C> = Field<<C as Curve>::Base>;
/// Scalar field element of curve `C`.
pub type ScalarOf<C> = Field<<C as Curve>::Scalar>;

/// A Pallas point in affine form.
pub type PallasAffine = Affine<Pallas>;
/// A Pallas point in Jacobian form.
pub type PallasProjective = Projective<Pallas>;
/// A Vesta point in affine form.
pub type VestaAffine = Affine<Vesta>;
/// A Vesta point in Jacobian form.
pub type VestaProjective = Projective<Vesta>;

fn curve_b<C: Curve>() -> BaseOf<C> {
    BaseOf::<C>::from_u64(5)
}

/// `x^3 + 5`.
fn rhs<C: Curve>(x: &BaseOf<C>) -> BaseOf<C> {
    x.square() * *x + curve_b::<C>()
}

/// An affine point; `(0, 0)` is the identity.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Affine<C: Curve> {
    x: BaseOf<C>,
    y: BaseOf<C>,
    _curve: PhantomData<C>,
}

impl<C: Curve> fmt::Debug for Affine<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_identity() {
            write!(f, "{}Affine(identity)", C::NAME)
        } else {
            write!(f, "{}Affine({:?}, {:?})", C::NAME, self.x, self.y)
        }
    }
}

impl<C: Curve> Default for Affine<C> {
    fn default() -> Self {
        Self::identity()
    }
}

impl<C: Curve> Affine<C> {
    /// The identity element.
    pub fn identity() -> Self {
        Self {
            x: BaseOf::<C>::ZERO,
            y: BaseOf::<C>::ZERO,
            _curve: PhantomData,
        }
    }

    /// The generator `(-1, 2)` shared by both Pasta curves.
    pub fn generator() -> Self {
        Self {
            x: -BaseOf::<C>::ONE,
            y: BaseOf::<C>::from_u64(2),
            _curve: PhantomData,
        }
    }

    /// Builds a point from coordinates, rejecting off-curve pairs. `(0, 0)` yields the identity.
    pub fn new(x: BaseOf<C>, y: BaseOf<C>) -> Result<Self, CurveError> {
        if x.is_zero() && y.is_zero() {
            return Ok(Self::identity());
        }
        let point = Self {
            x,
            y,
            _curve: PhantomData,
        };
        if point.is_on_curve() {
            Ok(point)
        } else {
            Err(CurveError::NotOnCurve)
        }
    }

    /// The `x` coordinate (zero for the identity).
    pub fn x(&self) -> BaseOf<C> {
        self.x
    }

    /// The `y` coordinate (zero for the identity).
    pub fn y(&self) -> BaseOf<C> {
        self.y
    }

    /// Whether this is the identity.
    pub fn is_identity(&self) -> bool {
        self.x.is_zero() && self.y.is_zero()
    }

    /// Whether the point satisfies the curve equation (the identity does).
    pub fn is_on_curve(&self) -> bool {
        self.is_identity() || self.y.square() == rhs::<C>(&self.x)
    }

    /// The Jacobian form of the point.
    pub fn to_projective(&self) -> Projective<C> {
        if self.is_identity() {
            Projective::identity()
        } else {
            Projective {
                x: self.x,
                y: self.y,
                z: BaseOf::<C>::ONE,
            }
        }
    }

    /// `k * self` for a scalar field element.
    pub fn mul_scalar(&self, k: &ScalarOf<C>) -> Projective<C> {
        self.to_projective().mul_scalar(k)
    }

    /// The 32-byte compressed encoding.
    pub fn to_compressed(&self) -> [u8; COMPRESSED_BYTES] {
        if self.is_identity() {
            return [0u8; COMPRESSED_BYTES];
        }
        let mut bytes = self.x.to_repr();
        if self.y.is_odd() {
            bytes[COMPRESSED_BYTES - 1] |= 0x80;
        }
        bytes
    }

    /// Decodes a 32-byte compressed point.
    pub fn from_compressed(bytes: &[u8; COMPRESSED_BYTES]) -> Result<Self, CurveError> {
        let mut x_bytes = *bytes;
        let sign = x_bytes[COMPRESSED_BYTES - 1] >> 7 == 1;
        x_bytes[COMPRESSED_BYTES - 1] &= 0x7f;
        let x = BaseOf::<C>::from_repr(&x_bytes)?;
        if x.is_zero() {
            return Ok(Self::identity());
        }
        let y = rhs::<C>(&x).sqrt().ok_or(CurveError::NotOnCurve)?;
        let y = if y.is_odd() == sign { y } else { -y };
        Ok(Self {
            x,
            y,
            _curve: PhantomData,
        })
    }

    /// Decodes a compressed point from a slice of exactly 32 bytes.
    pub fn from_compressed_slice(bytes: &[u8]) -> Result<Self, CurveError> {
        let array: &[u8; COMPRESSED_BYTES] =
            bytes.try_into().map_err(|_| CurveError::WrongLength {
                expected: COMPRESSED_BYTES,
                found: bytes.len(),
            })?;
        Self::from_compressed(array)
    }

    /// The 64-byte uncompressed encoding `x || y`.
    pub fn to_uncompressed(&self) -> [u8; UNCOMPRESSED_BYTES] {
        let mut out = [0u8; UNCOMPRESSED_BYTES];
        out[..REPR_BYTES].copy_from_slice(&self.x.to_repr());
        out[REPR_BYTES..].copy_from_slice(&self.y.to_repr());
        out
    }

    /// Decodes a 64-byte uncompressed point, checking canonical coordinates and the curve
    /// equation.
    pub fn from_uncompressed(bytes: &[u8; UNCOMPRESSED_BYTES]) -> Result<Self, CurveError> {
        let x = BaseOf::<C>::from_repr_slice(&bytes[..REPR_BYTES])?;
        let y = BaseOf::<C>::from_repr_slice(&bytes[REPR_BYTES..])?;
        Self::new(x, y)
    }

    /// Decodes an uncompressed point from a slice of exactly 64 bytes.
    pub fn from_uncompressed_slice(bytes: &[u8]) -> Result<Self, CurveError> {
        let array: &[u8; UNCOMPRESSED_BYTES] =
            bytes.try_into().map_err(|_| CurveError::WrongLength {
                expected: UNCOMPRESSED_BYTES,
                found: bytes.len(),
            })?;
        Self::from_uncompressed(array)
    }
}

impl<C: Curve> Neg for Affine<C> {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x: self.x,
            y: -self.y,
            _curve: PhantomData,
        }
    }
}

/// A point in Jacobian coordinates.
#[derive(Clone, Copy)]
pub struct Projective<C: Curve> {
    x: BaseOf<C>,
    y: BaseOf<C>,
    z: BaseOf<C>,
}

impl<C: Curve> fmt::Debug for Projective<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}Projective({:?})", C::NAME, self.to_affine())
    }
}

impl<C: Curve> Default for Projective<C> {
    fn default() -> Self {
        Self::identity()
    }
}

impl<C: Curve> PartialEq for Projective<C> {
    fn eq(&self, other: &Self) -> bool {
        match (self.is_identity(), other.is_identity()) {
            (true, true) => true,
            (false, false) => {
                let z1z1 = self.z.square();
                let z2z2 = other.z.square();
                self.x * z2z2 == other.x * z1z1
                    && self.y * (other.z * z2z2) == other.y * (self.z * z1z1)
            }
            _ => false,
        }
    }
}

impl<C: Curve> Eq for Projective<C> {}

impl<C: Curve> Projective<C> {
    /// The identity element.
    pub fn identity() -> Self {
        Self {
            x: BaseOf::<C>::ZERO,
            y: BaseOf::<C>::ONE,
            z: BaseOf::<C>::ZERO,
        }
    }

    /// The generator.
    pub fn generator() -> Self {
        Affine::<C>::generator().to_projective()
    }

    /// Whether this is the identity.
    pub fn is_identity(&self) -> bool {
        self.z.is_zero()
    }

    /// Whether the point satisfies the Jacobian curve equation `Y^2 = X^3 + 5 Z^6`.
    pub fn is_on_curve(&self) -> bool {
        if self.is_identity() {
            return true;
        }
        let z2 = self.z.square();
        let z6 = z2.square() * z2;
        self.y.square() == self.x.square() * self.x + curve_b::<C>() * z6
    }

    /// `2 * self`.
    pub fn double(&self) -> Self {
        if self.is_identity() {
            return *self;
        }
        let a = self.x.square();
        let b = self.y.square();
        let c = b.square();
        let d = ((self.x + b).square() - a - c).double();
        let e = a.double() + a;
        let f = e.square();
        let x3 = f - d.double();
        let y3 = e * (d - x3) - c.double().double().double();
        let z3 = (self.y * self.z).double();
        Self {
            x: x3,
            y: y3,
            z: z3,
        }
    }

    /// `self + other`.
    pub fn add_projective(&self, other: &Self) -> Self {
        if self.is_identity() {
            return *other;
        }
        if other.is_identity() {
            return *self;
        }
        let z1z1 = self.z.square();
        let z2z2 = other.z.square();
        let u1 = self.x * z2z2;
        let u2 = other.x * z1z1;
        let s1 = self.y * other.z * z2z2;
        let s2 = other.y * self.z * z1z1;
        let h = u2 - u1;
        let r = (s2 - s1).double();
        if h.is_zero() {
            return if r.is_zero() {
                self.double()
            } else {
                Self::identity()
            };
        }
        let i = h.double().square();
        let j = h * i;
        let v = u1 * i;
        let x3 = r.square() - j - v.double();
        let y3 = r * (v - x3) - (s1 * j).double();
        let z3 = ((self.z + other.z).square() - z1z1 - z2z2) * h;
        Self {
            x: x3,
            y: y3,
            z: z3,
        }
    }

    /// `self + other` for an affine `other` (mixed addition).
    pub fn add_affine(&self, other: &Affine<C>) -> Self {
        if other.is_identity() {
            return *self;
        }
        if self.is_identity() {
            return other.to_projective();
        }
        let z1z1 = self.z.square();
        let u2 = other.x * z1z1;
        let s2 = other.y * self.z * z1z1;
        let h = u2 - self.x;
        let r = (s2 - self.y).double();
        if h.is_zero() {
            return if r.is_zero() {
                self.double()
            } else {
                Self::identity()
            };
        }
        let hh = h.square();
        let i = hh.double().double();
        let j = h * i;
        let v = self.x * i;
        let x3 = r.square() - j - v.double();
        let y3 = r * (v - x3) - (self.y * j).double();
        let z3 = (self.z + h).square() - z1z1 - hh;
        Self {
            x: x3,
            y: y3,
            z: z3,
        }
    }

    /// `k * self` for a scalar field element.
    pub fn mul_scalar(&self, k: &ScalarOf<C>) -> Self {
        self.mul_limbs(&k.to_limbs())
    }

    /// `k * self` for a little-endian integer scalar.
    pub fn mul_limbs(&self, k: &[u64; LIMBS]) -> Self {
        let mut acc = Self::identity();
        for limb in k.iter().rev() {
            for bit in (0..64).rev() {
                acc = acc.double();
                if (limb >> bit) & 1 == 1 {
                    acc = acc.add_projective(self);
                }
            }
        }
        acc
    }

    /// The affine form; the identity maps to `(0, 0)`.
    pub fn to_affine(&self) -> Affine<C> {
        match self.z.invert() {
            None => Affine::identity(),
            Some(z_inv) => {
                let z_inv2 = z_inv.square();
                Affine {
                    x: self.x * z_inv2,
                    y: self.y * z_inv2 * z_inv,
                    _curve: PhantomData,
                }
            }
        }
    }
}

impl<C: Curve> Add for Projective<C> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        self.add_projective(&rhs)
    }
}

impl<C: Curve> Neg for Projective<C> {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x: self.x,
            y: -self.y,
            z: self.z,
        }
    }
}

impl<C: Curve> Sub for Projective<C> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self.add_projective(&-rhs)
    }
}

/// Converts many projective points to affine with a single field inversion (Montgomery's trick).
/// Identity inputs map to the affine identity.
pub fn batch_normalize<C: Curve>(points: &[Projective<C>]) -> Vec<Affine<C>> {
    let mut prefix = Vec::with_capacity(points.len());
    let mut acc = BaseOf::<C>::ONE;
    for point in points {
        if !point.z.is_zero() {
            acc *= point.z;
        }
        prefix.push(acc);
    }
    let mut inv = acc.invert().unwrap_or(BaseOf::<C>::ONE);
    let mut out = vec![Affine::<C>::identity(); points.len()];
    for i in (0..points.len()).rev() {
        let point = &points[i];
        if point.z.is_zero() {
            continue;
        }
        let before = if i == 0 {
            BaseOf::<C>::ONE
        } else {
            prefix[i - 1]
        };
        let z_inv = inv * before;
        inv *= point.z;
        let z_inv2 = z_inv.square();
        out[i] = Affine {
            x: point.x * z_inv2,
            y: point.y * z_inv2 * z_inv,
            _curve: PhantomData,
        };
    }
    out
}
