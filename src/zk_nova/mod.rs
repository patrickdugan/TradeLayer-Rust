//! Zero-dependency verifier for nova-snark 0.76.0 `CompressedSNARK` proofs over the
//! Pallas/Vesta cycle, compressed with Spartan's preprocessing SNARK (`spartan::ppsnark`) and the
//! IPA evaluation engine on both curves. This is the Nova profile of the in-protocol ZK
//! verifier (`docs/ZK_VERIFIER.md`, `docs/ZK_VERIFIER_NOVA.md`); the exact algorithm it
//! implements is specified in `docs/ZK_NOVA_SPEC.md`.
//!
//! Every function here is a pure function of its inputs, bit-exact with the pinned reference
//! prover, and returns a typed error on hostile input instead of panicking.
//!
//! # Public API
//!
//! [`verify`] takes a [`RegisteredNovaKey`] (about 553 bytes: the circuit data that cannot be
//! derived), a [`GeneratorCache`], the step count, `z0` and the proof bytes, and returns `zn`.
//! The cache derives each generator table once; keep one cache per process.
//!
//! # Size limits
//!
//! * [`MAX_PROOF_BYTES`]: proofs longer than this are rejected before decoding.
//! * [`MAX_N`]: the largest spark vector length `N` and generator count a key may declare. The
//!   generator tables and the IPA scalar vector cost about 100 bytes per unit of `N` and curve.
//! * [`MAX_ARITY`]: the largest step-circuit arity.
//! * Every length prefix is checked against the remaining input before allocation, so a decode
//!   can claim at most a small multiple of the proof length.

pub mod codec;
pub mod curve;
pub mod error;
pub mod field;
pub mod h2c;
pub mod ipa;
pub mod keccak;
pub mod key;
pub mod msm;
pub mod nova;
pub mod polys;
pub mod poseidon;
pub mod ppsnark;
pub mod sumcheck;
pub mod transcript;

pub use error::{DecodeError, NovaVerifyError};
pub use key::{GeneratorCache, RegisteredNovaKey, VerifierKey};

use field::Fq;

/// Longest accepted proof, in bytes. Real proofs are 11 to 14 KiB plus 32 bytes per state element.
pub const MAX_PROOF_BYTES: usize = 262_144;
/// Largest supported spark vector length `N` (and generator count): `2^23`.
pub const MAX_N: usize = 1 << 23;
/// Largest supported step-circuit arity.
pub const MAX_ARITY: usize = 4096;

/// Verifies a `CompressedSNARK` proof of `num_steps` steps from `z0` under `key`, returning `zn`.
///
/// Rejects with a typed error on any malformed or invalid input; never panics.
pub fn verify(
    key: &RegisteredNovaKey,
    cache: &GeneratorCache,
    num_steps: u64,
    z0: &[Fq],
    proof: &[u8],
) -> Result<Vec<Fq>, NovaVerifyError> {
    let snark = nova::decode(proof)?;
    nova::precheck(key.f_arity as usize, num_steps, z0, &snark)?;
    let vk = key.expand(cache)?;
    nova::verify_decoded(&vk, num_steps, z0, snark)
}
