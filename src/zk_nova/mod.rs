//! Zero-dependency verifier for nova-snark 0.76.0 `CompressedSNARK` proofs over the
//! Pallas/Vesta cycle, compressed with Spartan's preprocessing SNARK (`spartan::ppsnark`) and the
//! IPA evaluation engine on both curves. This is the Nova profile of the in-protocol ZK
//! verifier (`docs/ZK_VERIFIER.md`, `docs/ZK_VERIFIER_NOVA.md`); the exact algorithm it
//! implements is specified in `docs/ZK_NOVA_SPEC.md`.
//!
//! Every function here is a pure function of its inputs, bit-exact with the pinned reference
//! prover, and returns a typed error on hostile input instead of panicking.

pub mod curve;
pub mod field;
pub mod h2c;
pub mod keccak;
pub mod msm;
