//! Typed errors of the Nova verifier. Every hostile input maps to one of these; nothing in the
//! verifier panics on malformed bytes, keys, or proof contents.

use std::fmt;

/// A failure while decoding a proof or a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// The input ended before a fixed-size field was complete.
    Truncated,
    /// A length prefix announces more elements than the remaining bytes can hold.
    LengthExceedsInput,
    /// A field element is not below the modulus.
    NonCanonicalScalar,
    /// A 32-byte point encoding is not a curve point.
    InvalidPoint,
    /// Bytes remain after the last field.
    TrailingBytes,
    /// A decoded size field is outside the supported range.
    SizeOutOfRange,
    /// An unknown profile tag.
    UnknownProfile,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Truncated => "input ended inside a field",
            Self::LengthExceedsInput => "a length prefix exceeds the remaining input",
            Self::NonCanonicalScalar => "a field element is not canonical",
            Self::InvalidPoint => "a point encoding is not on the curve",
            Self::TrailingBytes => "trailing bytes after the last field",
            Self::SizeOutOfRange => "a size field is outside the supported range",
            Self::UnknownProfile => "unknown key profile tag",
        };
        f.write_str(text)
    }
}

impl std::error::Error for DecodeError {}

/// Why a Nova proof was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NovaVerifyError {
    /// The proof is longer than [`crate::zk_nova::MAX_PROOF_BYTES`].
    ProofTooLarge {
        /// Length of the supplied proof.
        len: usize,
        /// The limit.
        max: usize,
    },
    /// The proof bytes do not decode.
    Decode(DecodeError),
    /// The registered key is malformed or outside the supported shapes.
    InvalidKey(&'static str),
    /// Generator derivation failed or exceeded the supported size.
    Generators(&'static str),
    /// `num_steps` is zero.
    ZeroSteps,
    /// `z0` or `zn` does not have `F_arity` entries.
    ArityMismatch,
    /// A folded or running instance does not carry exactly two public values.
    InstanceLength,
    /// The Poseidon hash of the running instances differs from the public values of `l_u_secondary`.
    HashBindingMismatch,
    /// A sum-check proof failed: wrong round count, degree above 3, or a failed final claim.
    InvalidSumcheckProof,
    /// The inner-product argument has inconsistent lengths.
    InvalidInputLength,
    /// An inner-product challenge is zero.
    ZeroChallenge,
    /// The inner-product (polynomial commitment) equation does not hold.
    InvalidPcs,
    /// A worker failed while computing a multi-scalar product.
    Internal(&'static str),
}

impl From<DecodeError> for NovaVerifyError {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

impl fmt::Display for NovaVerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProofTooLarge { len, max } => {
                write!(f, "proof of {len} bytes exceeds the {max} byte limit")
            }
            Self::Decode(e) => write!(f, "proof decoding failed: {e}"),
            Self::InvalidKey(why) => write!(f, "invalid verifier key: {why}"),
            Self::Generators(why) => write!(f, "generator derivation failed: {why}"),
            Self::ZeroSteps => f.write_str("num_steps is zero"),
            Self::ArityMismatch => f.write_str("z0 or zn length differs from the circuit arity"),
            Self::InstanceLength => f.write_str("an instance does not have two public values"),
            Self::HashBindingMismatch => f.write_str("running-instance hash binding mismatch"),
            Self::InvalidSumcheckProof => f.write_str("invalid sum-check proof"),
            Self::InvalidInputLength => f.write_str("invalid inner-product argument length"),
            Self::ZeroChallenge => f.write_str("zero inner-product challenge"),
            Self::InvalidPcs => f.write_str("inner-product equation failed"),
            Self::Internal(why) => write!(f, "internal failure: {why}"),
        }
    }
}

impl std::error::Error for NovaVerifyError {}
