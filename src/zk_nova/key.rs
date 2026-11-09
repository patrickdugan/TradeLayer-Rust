//! Verifier keys: the compact registered form, the generator cache, and the expanded key.
//!
//! A [`RegisteredNovaKey`] holds only the data that cannot be derived from the profile
//! (`docs/ZK_NOVA_SPEC.md` section 4): the circuit arity, `pp_digest`, and per curve the padded
//! shape, the spark vector length `N`, the generator count and the seven spark commitments.
//! [`RegisteredNovaKey::expand`] derives everything else.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use super::codec::{decode_vk_nogens, Reader, SparkComm};
use super::curve::{Affine, Curve, Pallas, ScalarOf, Vesta};
use super::error::{DecodeError, NovaVerifyError};
use super::field::{FpParams, Fq, FqParams};
use super::h2c::{from_label, sha256_digest, H2cCurve};
use super::keccak::keccak_f1600;
use super::poseidon::PoseidonParams;
use super::{MAX_ARITY, MAX_N};

/// Profile tag of the only supported profile: nova-snark 0.76.0, ppsnark + IPA on Pallas/Vesta,
/// Keccak256 transcript, wide Poseidon (width 25, 8 + 59 rounds).
pub const PROFILE_PPSNARK_IPA_PASTA_V1: u8 = 1;

/// Generator label of the public-parameter commitment key.
pub const LABEL_CK: &[u8] = b"ck";
/// Generator label of the IPA scale key.
pub const LABEL_IPA: &[u8] = b"ipa";

/// Bytes of an encoded [`RegisteredNovaKey`].
pub const REGISTERED_KEY_BYTES: usize = 1 + 8 + 32 + 2 * (4 * 8 + 7 * 32);

type GenMap<C> = HashMap<(Vec<u8>, usize), Arc<Vec<Affine<C>>>>;

/// A curve whose generator tables the [`GeneratorCache`] stores.
pub trait CacheCurve: H2cCurve {
    /// The cache's table for this curve.
    fn table(cache: &GeneratorCache) -> &Mutex<GenMap<Self>>;
}

impl CacheCurve for Pallas {
    fn table(cache: &GeneratorCache) -> &Mutex<GenMap<Self>> {
        &cache.pallas
    }
}

impl CacheCurve for Vesta {
    fn table(cache: &GeneratorCache) -> &Mutex<GenMap<Self>> {
        &cache.vesta
    }
}

/// In-memory table of `from_label(label, count)` results, built once per `(curve, label, count)`.
#[derive(Default)]
pub struct GeneratorCache {
    pallas: Mutex<GenMap<Pallas>>,
    vesta: Mutex<GenMap<Vesta>>,
}

impl GeneratorCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// The `count` generators `from_label(label, count)`, derived on first use. Concurrent callers
    /// of the same curve wait for the build instead of repeating it.
    pub fn generators<C: CacheCurve>(
        &self,
        label: &[u8],
        count: usize,
    ) -> Result<Arc<Vec<Affine<C>>>, NovaVerifyError> {
        if count == 0 || count > MAX_N + 1 {
            return Err(NovaVerifyError::Generators("generator count out of range"));
        }
        let mut guard: MutexGuard<'_, GenMap<C>> = C::table(self)
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let slot = (label.to_vec(), count);
        if let Some(found) = guard.get(&slot) {
            return Ok(Arc::clone(found));
        }
        let built = from_label::<C>(label, count)
            .map_err(|_| NovaVerifyError::Generators("hash-to-curve failed"))?;
        let built = Arc::new(built);
        guard.insert(slot, Arc::clone(&built));
        Ok(built)
    }

    /// Number of generator tables held.
    pub fn len(&self) -> usize {
        let a = self.pallas.lock().map(|g| g.len()).unwrap_or(0);
        let b = self.vesta.lock().map(|g| g.len()).unwrap_or(0);
        a + b
    }

    /// Whether the cache holds no tables.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The circuit-specific data of one curve's ppsnark verifier key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurveKey<C: Curve> {
    /// Padded constraint count (a power of two).
    pub num_cons: u64,
    /// Padded variable count (a power of two, at least 8).
    pub num_vars: u64,
    /// Number of generators in `ck_v` (a power of two, at least `N`).
    pub ck_len: u64,
    /// The spark vector length `N` and the seven spark commitments.
    pub spark: SparkComm<C>,
}

impl<C: Curve> CurveKey<C> {
    fn validate(&self) -> Result<(), NovaVerifyError> {
        let bad = |why| Err(NovaVerifyError::InvalidKey(why));
        let n = self.spark.n;
        if self.num_cons == 0 || !self.num_cons.is_power_of_two() {
            return bad("num_cons is not a power of two");
        }
        if self.num_vars < 8 || !self.num_vars.is_power_of_two() {
            return bad("num_vars is not a power of two of at least 8");
        }
        if n == 0 || !n.is_power_of_two() || n > MAX_N as u64 {
            return bad("N is not a power of two within the supported range");
        }
        if n < 2 * self.num_vars {
            return bad("N is below 2 * num_vars");
        }
        if n < self.num_cons {
            return bad("N is below num_cons");
        }
        if !self.ck_len.is_power_of_two() || self.ck_len < n || self.ck_len > MAX_N as u64 {
            return bad("ck_len is not a power of two within [N, MAX_N]");
        }
        Ok(())
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.num_cons.to_le_bytes());
        out.extend_from_slice(&self.num_vars.to_le_bytes());
        out.extend_from_slice(&self.spark.n.to_le_bytes());
        out.extend_from_slice(&self.ck_len.to_le_bytes());
        for point in self.spark.points() {
            out.extend_from_slice(&point.to_compressed());
        }
    }

    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let num_cons = r.u64()?;
        let num_vars = r.u64()?;
        let n = r.u64()?;
        let ck_len = r.u64()?;
        let comm = [
            r.point()?,
            r.point()?,
            r.point()?,
            r.point()?,
            r.point()?,
            r.point()?,
            r.point()?,
        ];
        Ok(Self {
            num_cons,
            num_vars,
            ck_len,
            spark: SparkComm {
                n,
                comm_row: comm[0],
                comm_col: comm[1],
                comm_val_a: comm[2],
                comm_val_b: comm[3],
                comm_val_c: comm[4],
                comm_ts_row: comm[5],
                comm_ts_col: comm[6],
            },
        })
    }
}

/// The compact per-circuit key (about 553 bytes) that a program registry pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegisteredNovaKey {
    /// Step-circuit arity.
    pub f_arity: u64,
    /// `pp_digest` of the public parameters.
    pub pp_digest: Fq,
    /// Primary (Pallas) data.
    pub primary: CurveKey<Pallas>,
    /// Secondary (Vesta) data.
    pub secondary: CurveKey<Vesta>,
}

impl RegisteredNovaKey {
    fn validate(&self) -> Result<(), NovaVerifyError> {
        if self.f_arity == 0 || self.f_arity > MAX_ARITY as u64 {
            return Err(NovaVerifyError::InvalidKey("arity out of range"));
        }
        // nova truncates `pp_digest` to 250 bits (`NUM_HASH_BITS`), so a genuine digest is below
        // the Pallas base modulus and the scalar-to-base switch of the secondary fold is lossless.
        if self.pp_digest.to_repr()[31] >= 0x04 {
            return Err(NovaVerifyError::InvalidKey("pp_digest exceeds 250 bits"));
        }
        self.primary.validate()?;
        self.secondary.validate()
    }

    /// Canonical encoding: profile tag, `F_arity`, `pp_digest`, then per curve `num_cons`,
    /// `num_vars`, `N`, `ck_len` and the seven spark points (compressed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(REGISTERED_KEY_BYTES);
        out.push(PROFILE_PPSNARK_IPA_PASTA_V1);
        out.extend_from_slice(&self.f_arity.to_le_bytes());
        out.extend_from_slice(&self.pp_digest.to_repr());
        self.primary.encode_into(&mut out);
        self.secondary.encode_into(&mut out);
        out
    }

    /// Decodes and validates an encoded key; rejects trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, NovaVerifyError> {
        let mut r = Reader::new(bytes);
        let tag = r.take(1)?[0];
        if tag != PROFILE_PPSNARK_IPA_PASTA_V1 {
            return Err(DecodeError::UnknownProfile.into());
        }
        let f_arity = r.u64()?;
        let pp_digest = r.scalar::<FqParams>()?;
        let primary = CurveKey::<Pallas>::decode(&mut r)?;
        let secondary = CurveKey::<Vesta>::decode(&mut r)?;
        r.finish()?;
        let key = Self {
            f_arity,
            pp_digest,
            primary,
            secondary,
        };
        key.validate()?;
        Ok(key)
    }

    /// SHA-256 of [`Self::encode`]: the value a registry pins.
    pub fn sha256(&self) -> [u8; 32] {
        sha256_digest(&self.encode())
    }

    /// Builds the key from a generator-free nova `VerifierKey` (`*.vk_nogens.hex`). The generator
    /// count is taken as `N`, the length of every key the prover library produces.
    pub fn from_vk_nogens(bytes: &[u8]) -> Result<Self, NovaVerifyError> {
        let vk = decode_vk_nogens(bytes)?;
        let key = Self {
            f_arity: vk.f_arity,
            pp_digest: vk.pp_digest,
            primary: CurveKey {
                num_cons: vk.primary.num_cons,
                num_vars: vk.primary.num_vars,
                ck_len: vk.primary.spark.n,
                spark: vk.primary.spark,
            },
            secondary: CurveKey {
                num_cons: vk.secondary.num_cons,
                num_vars: vk.secondary.num_vars,
                ck_len: vk.secondary.spark.n,
                spark: vk.secondary.spark,
            },
        };
        key.validate()?;
        Ok(key)
    }

    /// Derives the full verifier key: generators from `generators`, Poseidon constants, `dk`, and
    /// the two ppsnark key digests.
    pub fn expand(&self, generators: &GeneratorCache) -> Result<VerifierKey, NovaVerifyError> {
        self.validate()?;
        let primary = CurveVk::<Pallas>::expand(&self.primary, generators)?;
        let secondary = CurveVk::<Vesta>::expand(&self.secondary, generators)?;
        Ok(VerifierKey {
            f_arity: self.f_arity as usize,
            pp_digest: self.pp_digest,
            ro_primary: PoseidonParams::<FpParams>::derive(),
            ro_secondary: PoseidonParams::<FqParams>::derive(),
            dk_primary: primary.ck_h(),
            dk_secondary: secondary.ck_h(),
            primary,
            secondary,
        })
    }
}

/// One curve's expanded ppsnark verifier key.
#[derive(Clone)]
pub struct CurveVk<C: Curve> {
    /// Padded constraint count.
    pub num_cons: usize,
    /// Padded variable count.
    pub num_vars: usize,
    /// Spark vector length.
    pub n: usize,
    /// Spark commitments.
    pub spark: SparkComm<C>,
    /// `from_label(b"ck", ck_len + 1)`: element 0 is `h`, the rest is `ck_v`.
    ck_gens: Arc<Vec<Affine<C>>>,
    /// `from_label(b"ipa", 2)`: element 0 is `ck_s.h`, element 1 is `ck_s.ck[0]`.
    ipa_gens: Arc<Vec<Affine<C>>>,
    /// The ppsnark key digest absorbed into the transcript.
    pub digest: ScalarOf<C>,
}

impl<C: CacheCurve> CurveVk<C> {
    fn expand(key: &CurveKey<C>, cache: &GeneratorCache) -> Result<Self, NovaVerifyError> {
        let ck_len = key.ck_len as usize;
        let ck_gens = cache.generators::<C>(LABEL_CK, ck_len + 1)?;
        let ipa_gens = cache.generators::<C>(LABEL_IPA, 2)?;
        let digest = vk_digest::<C>(key, &ck_gens, &ipa_gens);
        Ok(Self {
            num_cons: key.num_cons as usize,
            num_vars: key.num_vars as usize,
            n: key.spark.n as usize,
            spark: key.spark,
            ck_gens,
            ipa_gens,
            digest,
        })
    }
}

impl<C: Curve> CurveVk<C> {
    /// `ck_v.h`, also the derandomization key.
    pub fn ck_h(&self) -> Affine<C> {
        self.ck_gens[0]
    }

    /// The `ck_v` generators.
    pub fn ck_v(&self) -> &[Affine<C>] {
        &self.ck_gens[1..]
    }

    /// `ck_s.ck[0]`, the IPA scale generator.
    pub fn ck_s_generator(&self) -> Affine<C> {
        self.ipa_gens[1]
    }

    /// `ck_s.h`.
    pub fn ck_s_h(&self) -> Affine<C> {
        self.ipa_gens[0]
    }
}

/// The fully expanded verifier key.
#[derive(Clone)]
pub struct VerifierKey {
    /// Step-circuit arity.
    pub f_arity: usize,
    /// Public-parameter digest.
    pub pp_digest: Fq,
    /// Poseidon constants of the primary random oracle (field `Fp`).
    pub ro_primary: PoseidonParams<FpParams>,
    /// Poseidon constants of the secondary random oracle (field `Fq`).
    pub ro_secondary: PoseidonParams<FqParams>,
    /// Primary (Pallas) ppsnark key.
    pub primary: CurveVk<Pallas>,
    /// Secondary (Vesta) ppsnark key.
    pub secondary: CurveVk<Vesta>,
    /// Derandomization key of the primary side.
    pub dk_primary: Affine<Pallas>,
    /// Derandomization key of the secondary side.
    pub dk_secondary: Affine<Vesta>,
}

/// Incremental NIST SHA3-256.
pub struct Sha3Stream {
    state: [u64; 25],
    buffer: [u8; Self::RATE],
    filled: usize,
}

impl Default for Sha3Stream {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha3Stream {
    const RATE: usize = 136;

    /// An empty hash.
    pub fn new() -> Self {
        Self {
            state: [0; 25],
            buffer: [0; Self::RATE],
            filled: 0,
        }
    }

    fn absorb_buffer(&mut self) {
        for (lane, chunk) in self.state.iter_mut().zip(self.buffer.chunks_exact(8)) {
            let mut word = [0u8; 8];
            word.copy_from_slice(chunk);
            *lane ^= u64::from_le_bytes(word);
        }
        keccak_f1600(&mut self.state);
    }

    /// Absorbs bytes.
    pub fn update(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            let take = (Self::RATE - self.filled).min(data.len());
            self.buffer[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled == Self::RATE {
                self.absorb_buffer();
                self.filled = 0;
            }
        }
    }

    /// Finishes and returns the digest.
    pub fn finalize(mut self) -> [u8; 32] {
        self.buffer[self.filled..].fill(0);
        self.buffer[self.filled] ^= 0x06;
        self.buffer[Self::RATE - 1] ^= 0x80;
        self.absorb_buffer();
        let mut out = [0u8; 32];
        for (chunk, lane) in out.chunks_exact_mut(8).zip(self.state.iter()) {
            chunk.copy_from_slice(&lane.to_le_bytes());
        }
        out
    }
}

/// nova's `DigestComputer`: SHA3-256 of the bincode bytes, low 250 bits, as a scalar.
fn digest_scalar<S: super::field::FieldParams>(hash: [u8; 32]) -> super::field::Field<S> {
    let mut bytes = hash;
    bytes[31] &= 0x03;
    super::field::Field::<S>::from_repr(&bytes).unwrap_or(super::field::Field::<S>::ZERO)
}

/// The digest of `ppsnark::VerifierKey` (`docs/ZK_NOVA_SPEC.md` 5.4), streamed over the bincode
/// bytes of section 3.3 without materializing them.
fn vk_digest<C: Curve>(
    key: &CurveKey<C>,
    ck_gens: &[Affine<C>],
    ipa_gens: &[Affine<C>],
) -> ScalarOf<C> {
    let mut hasher = Sha3Stream::new();
    hasher.update(&key.num_cons.to_le_bytes());
    hasher.update(&key.num_vars.to_le_bytes());
    hasher.update(&((ck_gens.len() - 1) as u64).to_le_bytes());
    let mut chunk = Vec::with_capacity(32 * 1024);
    for point in &ck_gens[1..] {
        chunk.extend_from_slice(&point.to_compressed());
        if chunk.len() >= 32 * 1024 {
            hasher.update(&chunk);
            chunk.clear();
        }
    }
    hasher.update(&chunk);
    hasher.update(&ck_gens[0].to_compressed());
    hasher.update(&1u64.to_le_bytes());
    hasher.update(&ipa_gens[1].to_compressed());
    hasher.update(&ipa_gens[0].to_compressed());
    hasher.update(&key.spark.n.to_le_bytes());
    for point in key.spark.points() {
        hasher.update(&point.to_compressed());
    }
    digest_scalar::<C::Scalar>(hasher.finalize())
}
