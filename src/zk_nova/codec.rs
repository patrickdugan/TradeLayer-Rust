//! Decoders for the bincode 1.3 (fixed-int, little-endian) layouts of nova-snark 0.76.0
//! (`docs/ZK_NOVA_SPEC.md` sections 2 and 3).
//!
//! Every length prefix is checked against the bytes that remain before anything is allocated, so
//! the memory a decode can claim is bounded by the input length. Trailing bytes are rejected.

use super::curve::{Affine, Curve, Pallas, ScalarOf, Vesta};
use super::error::DecodeError;
use super::field::{Field, FieldParams, Fp, Fq};

/// Bytes of the Poseidon constants block (`PoseidonConstantsCircuit`) inside a `VerifierKey`.
pub const POSEIDON_BLOCK_BYTES: usize = 271_800;

/// Total bytes of the verifier key with the four generator lists removed.
pub const VK_NOGENS_BYTES: usize = 544_328;

/// A bounds-checked cursor over the input.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Starts reading at the beginning of `buf`.
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Bytes not yet consumed.
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Consumes `n` bytes.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if n > self.remaining() {
            return Err(DecodeError::Truncated);
        }
        let out = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    /// A `u64`, little endian.
    pub fn u64(&mut self) -> Result<u64, DecodeError> {
        let mut word = [0u8; 8];
        word.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(word))
    }

    /// A `Vec` length prefix whose elements are at least `min_element_bytes` each; fails when the
    /// remaining input cannot hold that many elements.
    pub fn length(&mut self, min_element_bytes: usize) -> Result<usize, DecodeError> {
        let announced = self.u64()?;
        let capacity = (self.remaining() / min_element_bytes.max(1)) as u64;
        if announced > capacity {
            return Err(DecodeError::LengthExceedsInput);
        }
        Ok(announced as usize)
    }

    /// A canonical field element.
    pub fn scalar<P: FieldParams>(&mut self) -> Result<Field<P>, DecodeError> {
        Field::<P>::from_repr_slice(self.take(32)?).map_err(|_| DecodeError::NonCanonicalScalar)
    }

    /// A compressed curve point.
    pub fn point<C: Curve>(&mut self) -> Result<Affine<C>, DecodeError> {
        Affine::<C>::from_compressed_slice(self.take(32)?).map_err(|_| DecodeError::InvalidPoint)
    }

    /// A `Vec` of field elements.
    pub fn scalars<P: FieldParams>(&mut self) -> Result<Vec<Field<P>>, DecodeError> {
        let count = self.length(32)?;
        (0..count).map(|_| self.scalar::<P>()).collect()
    }

    /// A `Vec` of points.
    pub fn points<C: Curve>(&mut self) -> Result<Vec<Affine<C>>, DecodeError> {
        let count = self.length(32)?;
        (0..count).map(|_| self.point::<C>()).collect()
    }

    /// Fails unless every byte was consumed.
    pub fn finish(&self) -> Result<(), DecodeError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(DecodeError::TrailingBytes)
        }
    }
}

/// `R1CSInstance<E>`.
#[derive(Clone, Debug)]
pub struct R1csInstance<C: Curve> {
    /// Witness commitment.
    pub comm_w: Affine<C>,
    /// Public values.
    pub x: Vec<ScalarOf<C>>,
}

/// `RelaxedR1CSInstance<E>`.
#[derive(Clone, Debug)]
pub struct RelaxedInstance<C: Curve> {
    /// Witness commitment.
    pub comm_w: Affine<C>,
    /// Error commitment.
    pub comm_e: Affine<C>,
    /// Public values.
    pub x: Vec<ScalarOf<C>>,
    /// Relaxation scalar.
    pub u: ScalarOf<C>,
}

/// `ipa_pc::InnerProductArgument`.
#[derive(Clone, Debug)]
pub struct IpaProof<C: Curve> {
    /// Left cross terms.
    pub l_vec: Vec<Affine<C>>,
    /// Right cross terms.
    pub r_vec: Vec<Affine<C>>,
    /// Final folded witness value.
    pub a_hat: ScalarOf<C>,
}

/// `ppsnark::RelaxedR1CSSNARK<E, IPA>`.
#[derive(Clone, Debug)]
pub struct PpsnarkProof<C: Curve> {
    /// Commitment to `L_row`.
    pub comm_l_row: Affine<C>,
    /// Commitment to `L_col`.
    pub comm_l_col: Affine<C>,
    /// Commitment to `1 / (t + r)` over the row memory.
    pub comm_t_plus_r_inv_row: Affine<C>,
    /// Commitment to `1 / (w + r)` over the row memory.
    pub comm_w_plus_r_inv_row: Affine<C>,
    /// Commitment to `1 / (t + r)` over the column memory.
    pub comm_t_plus_r_inv_col: Affine<C>,
    /// Commitment to `1 / (w + r)` over the column memory.
    pub comm_w_plus_r_inv_col: Affine<C>,
    /// Outer sum-check round polynomials (stored without the linear term).
    pub sc_outer: Vec<Vec<ScalarOf<C>>>,
    /// `Az(r_outer)`.
    pub eval_az_at_r_outer: ScalarOf<C>,
    /// `Bz(r_outer)`.
    pub eval_bz_at_r_outer: ScalarOf<C>,
    /// `Cz(r_outer)`.
    pub eval_cz_at_r_outer: ScalarOf<C>,
    /// `E(r_outer)`.
    pub eval_e_at_r_outer: ScalarOf<C>,
    /// Inner (batched) sum-check round polynomials.
    pub sc_inner_batched: Vec<Vec<ScalarOf<C>>>,
    /// `E(r_in)`.
    pub eval_e: ScalarOf<C>,
    /// `L_row(r_in)`.
    pub eval_l_row: ScalarOf<C>,
    /// `L_col(r_in)`.
    pub eval_l_col: ScalarOf<C>,
    /// `val_A(r_in)`.
    pub eval_val_a: ScalarOf<C>,
    /// `val_B(r_in)`.
    pub eval_val_b: ScalarOf<C>,
    /// `val_C(r_in)`.
    pub eval_val_c: ScalarOf<C>,
    /// `W(r_in)`.
    pub eval_w: ScalarOf<C>,
    /// `t_plus_r_inv_row(r_in)`.
    pub eval_t_plus_r_inv_row: ScalarOf<C>,
    /// `row(r_in)`.
    pub eval_row: ScalarOf<C>,
    /// `w_plus_r_inv_row(r_in)`.
    pub eval_w_plus_r_inv_row: ScalarOf<C>,
    /// `ts_row(r_in)`.
    pub eval_ts_row: ScalarOf<C>,
    /// `t_plus_r_inv_col(r_in)`.
    pub eval_t_plus_r_inv_col: ScalarOf<C>,
    /// `col(r_in)`.
    pub eval_col: ScalarOf<C>,
    /// `w_plus_r_inv_col(r_in)`.
    pub eval_w_plus_r_inv_col: ScalarOf<C>,
    /// `ts_col(r_in)`.
    pub eval_ts_col: ScalarOf<C>,
    /// The batched opening argument.
    pub eval_arg: IpaProof<C>,
}

/// `nova::CompressedSNARK` over Pallas (primary) and Vesta (secondary).
#[derive(Clone, Debug)]
pub struct CompressedSnark {
    /// Running instance of the secondary circuit.
    pub r_u_secondary: RelaxedInstance<Vesta>,
    /// Randomness of the secondary hash.
    pub ri_secondary: Fp,
    /// Fresh instance of the secondary circuit.
    pub l_u_secondary: R1csInstance<Vesta>,
    /// Cross term of the first secondary fold.
    pub nifs_uf_secondary: Affine<Vesta>,
    /// Random relaxed instance of the secondary circuit.
    pub l_ur_secondary: RelaxedInstance<Vesta>,
    /// Cross term of the second secondary fold.
    pub nifs_un_secondary: Affine<Vesta>,
    /// Running instance of the primary circuit.
    pub r_u_primary: RelaxedInstance<Pallas>,
    /// Randomness of the primary hash.
    pub ri_primary: Fq,
    /// Random relaxed instance of the primary circuit.
    pub l_ur_primary: RelaxedInstance<Pallas>,
    /// Cross term of the primary fold.
    pub nifs_un_primary: Affine<Pallas>,
    /// Blind of the folded primary witness commitment.
    pub wit_blind_r_wn_primary: Fq,
    /// Blind of the folded primary error commitment.
    pub err_blind_r_wn_primary: Fq,
    /// Blind of the folded secondary witness commitment.
    pub wit_blind_r_wn_secondary: Fp,
    /// Blind of the folded secondary error commitment.
    pub err_blind_r_wn_secondary: Fp,
    /// SNARK for the folded primary instance.
    pub snark_primary: PpsnarkProof<Pallas>,
    /// SNARK for the folded secondary instance.
    pub snark_secondary: PpsnarkProof<Vesta>,
    /// Claimed final state.
    pub zn: Vec<Fq>,
}

fn read_r1cs<C: Curve>(r: &mut Reader<'_>) -> Result<R1csInstance<C>, DecodeError> {
    Ok(R1csInstance {
        comm_w: r.point()?,
        x: r.scalars()?,
    })
}

fn read_relaxed<C: Curve>(r: &mut Reader<'_>) -> Result<RelaxedInstance<C>, DecodeError> {
    Ok(RelaxedInstance {
        comm_w: r.point()?,
        comm_e: r.point()?,
        x: r.scalars()?,
        u: r.scalar()?,
    })
}

fn read_sumcheck<C: Curve>(r: &mut Reader<'_>) -> Result<Vec<Vec<ScalarOf<C>>>, DecodeError> {
    let rounds = r.length(8)?;
    (0..rounds).map(|_| r.scalars()).collect()
}

fn read_ppsnark<C: Curve>(r: &mut Reader<'_>) -> Result<PpsnarkProof<C>, DecodeError> {
    Ok(PpsnarkProof {
        comm_l_row: r.point()?,
        comm_l_col: r.point()?,
        comm_t_plus_r_inv_row: r.point()?,
        comm_w_plus_r_inv_row: r.point()?,
        comm_t_plus_r_inv_col: r.point()?,
        comm_w_plus_r_inv_col: r.point()?,
        sc_outer: read_sumcheck::<C>(r)?,
        eval_az_at_r_outer: r.scalar()?,
        eval_bz_at_r_outer: r.scalar()?,
        eval_cz_at_r_outer: r.scalar()?,
        eval_e_at_r_outer: r.scalar()?,
        sc_inner_batched: read_sumcheck::<C>(r)?,
        eval_e: r.scalar()?,
        eval_l_row: r.scalar()?,
        eval_l_col: r.scalar()?,
        eval_val_a: r.scalar()?,
        eval_val_b: r.scalar()?,
        eval_val_c: r.scalar()?,
        eval_w: r.scalar()?,
        eval_t_plus_r_inv_row: r.scalar()?,
        eval_row: r.scalar()?,
        eval_w_plus_r_inv_row: r.scalar()?,
        eval_ts_row: r.scalar()?,
        eval_t_plus_r_inv_col: r.scalar()?,
        eval_col: r.scalar()?,
        eval_w_plus_r_inv_col: r.scalar()?,
        eval_ts_col: r.scalar()?,
        eval_arg: IpaProof {
            l_vec: r.points()?,
            r_vec: r.points()?,
            a_hat: r.scalar()?,
        },
    })
}

/// Decodes a `CompressedSNARK`, rejecting truncation, oversized length prefixes, non-canonical
/// scalars, invalid points and trailing bytes.
pub fn decode_compressed_snark(bytes: &[u8]) -> Result<CompressedSnark, DecodeError> {
    let mut r = Reader::new(bytes);
    let snark = CompressedSnark {
        r_u_secondary: read_relaxed(&mut r)?,
        ri_secondary: r.scalar()?,
        l_u_secondary: read_r1cs(&mut r)?,
        nifs_uf_secondary: r.point()?,
        l_ur_secondary: read_relaxed(&mut r)?,
        nifs_un_secondary: r.point()?,
        r_u_primary: read_relaxed(&mut r)?,
        ri_primary: r.scalar()?,
        l_ur_primary: read_relaxed(&mut r)?,
        nifs_un_primary: r.point()?,
        wit_blind_r_wn_primary: r.scalar()?,
        err_blind_r_wn_primary: r.scalar()?,
        wit_blind_r_wn_secondary: r.scalar()?,
        err_blind_r_wn_secondary: r.scalar()?,
        snark_primary: read_ppsnark(&mut r)?,
        snark_secondary: read_ppsnark(&mut r)?,
        zn: r.scalars()?,
    };
    r.finish()?;
    Ok(snark)
}

/// The seven spark commitments of one curve and the spark vector length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SparkComm<C: Curve> {
    /// Spark vector length `N`.
    pub n: u64,
    /// Commitment to `row`.
    pub comm_row: Affine<C>,
    /// Commitment to `col`.
    pub comm_col: Affine<C>,
    /// Commitment to `val_A`.
    pub comm_val_a: Affine<C>,
    /// Commitment to `val_B`.
    pub comm_val_b: Affine<C>,
    /// Commitment to `val_C`.
    pub comm_val_c: Affine<C>,
    /// Commitment to `ts_row`.
    pub comm_ts_row: Affine<C>,
    /// Commitment to `ts_col`.
    pub comm_ts_col: Affine<C>,
}

impl<C: Curve> SparkComm<C> {
    /// Reads `N` followed by the seven commitments.
    pub fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            n: r.u64()?,
            comm_row: r.point()?,
            comm_col: r.point()?,
            comm_val_a: r.point()?,
            comm_val_b: r.point()?,
            comm_val_c: r.point()?,
            comm_ts_row: r.point()?,
            comm_ts_col: r.point()?,
        })
    }

    /// The seven commitments in wire order.
    pub fn points(&self) -> [Affine<C>; 7] {
        [
            self.comm_row,
            self.comm_col,
            self.comm_val_a,
            self.comm_val_b,
            self.comm_val_c,
            self.comm_ts_row,
            self.comm_ts_col,
        ]
    }
}

/// One curve's part of a verifier key without generator lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurveNoGens<C: Curve> {
    /// Padded constraint count.
    pub num_cons: u64,
    /// Padded variable count.
    pub num_vars: u64,
    /// `ck_v.h`.
    pub ck_v_h: Affine<C>,
    /// `ck_s.h`.
    pub ck_s_h: Affine<C>,
    /// Spark commitments.
    pub spark: SparkComm<C>,
}

/// `nova::VerifierKey` with the four generator lists cut out (`fixtures/zk_nova/*.vk_nogens.hex`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VkNoGens {
    /// Step-circuit arity.
    pub f_arity: u64,
    /// Public-parameter digest.
    pub pp_digest: Fq,
    /// Primary (Pallas) part.
    pub primary: CurveNoGens<Pallas>,
    /// Secondary (Vesta) part.
    pub secondary: CurveNoGens<Vesta>,
    /// `dk_primary`.
    pub dk_primary: Affine<Pallas>,
    /// `dk_secondary`.
    pub dk_secondary: Affine<Vesta>,
}

fn read_curve_nogens<C: Curve>(r: &mut Reader<'_>) -> Result<CurveNoGens<C>, DecodeError> {
    let num_cons = r.u64()?;
    let num_vars = r.u64()?;
    let ck_v_h = r.point()?;
    let ck_s_h = r.point()?;
    let spark = SparkComm::read(r)?;
    Ok(CurveNoGens {
        num_cons,
        num_vars,
        ck_v_h,
        ck_s_h,
        spark,
    })
}

/// Decodes the generator-free verifier key. The two Poseidon constant blocks are skipped (they are
/// a function of the field alone and are re-derived by the verifier).
pub fn decode_vk_nogens(bytes: &[u8]) -> Result<VkNoGens, DecodeError> {
    if bytes.len() != VK_NOGENS_BYTES {
        return Err(if bytes.len() < VK_NOGENS_BYTES {
            DecodeError::Truncated
        } else {
            DecodeError::TrailingBytes
        });
    }
    let mut r = Reader::new(bytes);
    let f_arity = r.u64()?;
    r.take(POSEIDON_BLOCK_BYTES)?;
    r.take(POSEIDON_BLOCK_BYTES)?;
    let pp_digest = r.scalar()?;
    let primary = read_curve_nogens(&mut r)?;
    let secondary = read_curve_nogens(&mut r)?;
    let dk_primary = r.point()?;
    let dk_secondary = r.point()?;
    r.finish()?;
    Ok(VkNoGens {
        f_arity,
        pp_digest,
        primary,
        secondary,
        dk_primary,
        dk_secondary,
    })
}
