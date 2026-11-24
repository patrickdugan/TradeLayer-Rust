//! Authenticated Poseidon Merkle trees (native + R1CS gadgets) for TAP-VWAP-03:
//! 1. `ChannelRegistryTree`: authenticates channel outpoint, immutable peer order,
//!    rotatable receipt x-only keys, asset binding, and funding resolution.
//! 2. `IndexedNullifierTree`: persistent cross-epoch 256-bit non-membership + insertion
//!    tree over canonical `AssetHTLC/v1` nullifiers (no epoch resets).
//! 3. `QuoteConsumptionTree`: persistent cumulative quote-capacity tree enforcing
//!    `consumed_msat + delta_msat <= max_out_msat` across partial fills and epochs.

use ff::{PrimeField, PrimeFieldBits};
use generic_array::typenum::U5;
use nova_snark::frontend::{
    gadgets::poseidon::{
        Elt, IOPattern, PoseidonConstants, Simplex, Sponge, SpongeAPI, SpongeCircuit, SpongeOp,
        SpongeTrait, Strength,
    },
    num::AllocatedNum,
    Boolean, ConstraintSystem, LinearCombination, SynthesisError,
};
use serde::{Deserialize, Serialize};

/// Tree depth for registry, indexed nullifier, and quote trees (2^12 = 4,096 leaves).
pub const STATE_TREE_DEPTH: usize = 12;

/// Split a 32-byte big-endian value bijectively into `(hi_128, lo_128)`.
pub fn bytes32_to_u128_pair(bytes: &[u8; 32]) -> (u128, u128) {
    let mut hi = [0u8; 16];
    let mut lo = [0u8; 16];
    hi.copy_from_slice(&bytes[0..16]);
    lo.copy_from_slice(&bytes[16..32]);
    (u128::from_be_bytes(hi), u128::from_be_bytes(lo))
}

/// Convert `u128` into a prime field element (`2^128 << p`).
pub fn u128_to_scalar<F: PrimeField>(v: u128) -> F {
    let lo = (v & ((1u128 << 64) - 1)) as u64;
    let hi = (v >> 64) as u64;
    F::from(hi) * F::from(1u64 << 32) * F::from(1u64 << 32) + F::from(lo)
}

/// Serialize a field element's canonical representation to 32 little-endian bytes.
pub fn scalar_to_bytes32<F: PrimeField>(s: &F) -> [u8; 32] {
    let repr = s.to_repr();
    let slice = repr.as_ref();
    let mut out = [0u8; 32];
    out.copy_from_slice(&slice[..32]);
    out
}

/// Deserialize a canonical field element from 32 little-endian bytes.
pub fn bytes32_to_scalar<F: PrimeField>(bytes: &[u8; 32]) -> Result<F, String> {
    let mut repr = F::Repr::default();
    repr.as_mut()[..32].copy_from_slice(bytes);
    Option::from(F::from_repr(repr)).ok_or_else(|| "non-canonical scalar bytes".to_string())
}

/// Width-5 Poseidon sponge constants cached for native and R1CS hashing.
#[derive(Clone)]
pub struct PoseidonHasher<F: PrimeField> {
    /// Width-5 Poseidon constants (`Strength::Standard`).
    pub constants: PoseidonConstants<F, U5>,
}

impl<F: PrimeField + PrimeFieldBits> Default for PoseidonHasher<F> {
    fn default() -> Self {
        Self::new()
    }
}

impl<F: PrimeField + PrimeFieldBits> PoseidonHasher<F> {
    /// Construct standard width-5 Poseidon constants.
    pub fn new() -> Self {
        Self {
            constants: Sponge::<F, U5>::api_constants(Strength::Standard),
        }
    }

    /// Native Poseidon hash absorbing `inputs` with domain tag `domain_tag`.
    pub fn hash_native(&self, domain_tag: u64, inputs: &[F]) -> F {
        let mut all = Vec::with_capacity(inputs.len() + 1);
        all.push(F::from(domain_tag));
        all.extend_from_slice(inputs);

        let mut sponge = Sponge::new_with_constants(&self.constants, Simplex);
        let acc = &mut ();
        let pattern = IOPattern(vec![
            SpongeOp::Absorb(all.len() as u32),
            SpongeOp::Squeeze(1),
        ]);
        sponge.start(pattern, None, acc);
        SpongeAPI::absorb(&mut sponge, all.len() as u32, &all, acc);
        let out = SpongeAPI::squeeze(&mut sponge, 1, acc);
        sponge.finish(acc).expect("poseidon native finish");
        out[0]
    }

    /// Native 2-to-1 Merkle internal node hash (`domain_tag = 0x4d4e_3031`).
    pub fn hash_node_native(&self, left: F, right: F) -> F {
        self.hash_native(0x4d4e_3031, &[left, right])
    }

    /// R1CS Poseidon hash absorbing `[AllocatedNum(domain_tag), inputs...]`.
    pub fn hash_circuit<CS: ConstraintSystem<F>>(
        &self,
        mut cs: CS,
        domain_tag: u64,
        inputs: &[AllocatedNum<F>],
    ) -> Result<AllocatedNum<F>, SynthesisError> {
        let tag_alloc = AllocatedNum::alloc(cs.namespace(|| "domain_tag"), || {
            Ok(F::from(domain_tag))
        })?;
        cs.enforce(
            || "constrain domain_tag constant",
            |lc| lc + tag_alloc.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + (F::from(domain_tag), CS::one()),
        );

        let mut elts = Vec::with_capacity(inputs.len() + 1);
        elts.push(Elt::Allocated(tag_alloc));
        for x in inputs {
            elts.push(Elt::Allocated(x.clone()));
        }

        let pattern = IOPattern(vec![
            SpongeOp::Absorb(elts.len() as u32),
            SpongeOp::Squeeze(1),
        ]);
        let mut ns = cs.namespace(|| "poseidon_sponge");
        let mut sponge = SpongeCircuit::new_with_constants(&self.constants, Simplex);
        sponge.start(pattern, None, &mut ns);
        SpongeAPI::absorb(&mut sponge, elts.len() as u32, &elts, &mut ns);
        let out = SpongeAPI::squeeze(&mut sponge, 1, &mut ns);
        sponge
            .finish(&mut ns)
            .map_err(|_| SynthesisError::Unsatisfiable("poseidon finish".to_string()))?;
        let res = Elt::ensure_allocated(&out[0], &mut ns.namespace(|| "ensure_out"))?;
        Ok(res)
    }

    /// R1CS 2-to-1 Merkle node hash.
    pub fn hash_node_circuit<CS: ConstraintSystem<F>>(
        &self,
        cs: CS,
        left: &AllocatedNum<F>,
        right: &AllocatedNum<F>,
    ) -> Result<AllocatedNum<F>, SynthesisError> {
        self.hash_circuit(cs, 0x4d4e_3031, &[left.clone(), right.clone()])
    }
}

/// Enforce `x` fits in `num_bits` (`num_bits <= 128`) and return its little-endian bits.
pub fn range_check_bits_le<F: PrimeField, CS: ConstraintSystem<F>>(
    mut cs: CS,
    x: &AllocatedNum<F>,
    num_bits: usize,
) -> Result<Vec<Boolean>, SynthesisError> {
    assert!(num_bits <= 128);
    let val_opt = x.get_value();
    let mut bits = Vec::with_capacity(num_bits);
    let mut lc = LinearCombination::<F>::zero();
    let mut coeff = F::ONE;

    let raw_bytes = val_opt.map(|v| {
        let mut b = [0u8; 32];
        b.copy_from_slice(&v.to_repr().as_ref()[..32]);
        b
    });

    for i in 0..num_bits {
        let bit_val = raw_bytes.map(|b| ((b[i / 8] >> (i % 8)) & 1) == 1);
        let b = Boolean::from(nova_snark::frontend::AllocatedBit::alloc(
            cs.namespace(|| format!("bit_{i}")),
            bit_val,
        )?);
        lc = lc + &b.lc(CS::one(), coeff);
        bits.push(b);
        coeff = coeff.double();
    }

    cs.enforce(
        || "repack range-checked bits",
        |_| lc,
        |lc_one| lc_one + CS::one(),
        |lc_x| lc_x + x.get_variable(),
    );
    Ok(bits)
}

/// Select `(left, right)` from `(cur, sibling)` given path bit `is_right`:
/// - `is_right == 0` -> `(cur, sibling)`
/// - `is_right == 1` -> `(sibling, cur)`
pub fn merkle_select_pair<F: PrimeField, CS: ConstraintSystem<F>>(
    mut cs: CS,
    cur: &AllocatedNum<F>,
    sibling: &AllocatedNum<F>,
    is_right: &Boolean,
) -> Result<(AllocatedNum<F>, AllocatedNum<F>), SynthesisError> {
    // left = cur + is_right * (sibling - cur)
    // right = cur + sibling - left
    let left = AllocatedNum::alloc(cs.namespace(|| "left"), || {
        let c = cur.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let s = sibling.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let r = is_right.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(if r { s } else { c })
    })?;
    // Enforce is_right * (sibling - cur) == left - cur
    cs.enforce(
        || "select left",
        |lc| lc + &is_right.lc(CS::one(), F::ONE),
        |lc| lc + sibling.get_variable() - cur.get_variable(),
        |lc| lc + left.get_variable() - cur.get_variable(),
    );
    let right = AllocatedNum::alloc(cs.namespace(|| "right"), || {
        let c = cur.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let s = sibling.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let l = left.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(c + s - l)
    })?;
    cs.enforce(
        || "select right",
        |lc| lc + cur.get_variable() + sibling.get_variable() - left.get_variable(),
        |lc| lc + CS::one(),
        |lc| lc + right.get_variable(),
    );
    Ok((left, right))
}

/// Compute Merkle root in R1CS from `leaf`, `index_bits_le` (length `DEPTH`), and `siblings`.
pub fn compute_merkle_root_circuit<F: PrimeField + PrimeFieldBits, CS: ConstraintSystem<F>>(
    mut cs: CS,
    hasher: &PoseidonHasher<F>,
    leaf: &AllocatedNum<F>,
    index_bits_le: &[Boolean],
    siblings: &[AllocatedNum<F>; STATE_TREE_DEPTH],
) -> Result<AllocatedNum<F>, SynthesisError> {
    assert_eq!(index_bits_le.len(), STATE_TREE_DEPTH);
    let mut cur = leaf.clone();
    for lvl in 0..STATE_TREE_DEPTH {
        let (l, r) = merkle_select_pair(
            cs.namespace(|| format!("sel_{lvl}")),
            &cur,
            &siblings[lvl],
            &index_bits_le[lvl],
        )?;
        cur = hasher.hash_node_circuit(cs.namespace(|| format!("hash_{lvl}")), &l, &r)?;
    }
    Ok(cur)
}

/// Enforce strict 256-bit unsigned inequality `(a_hi, a_lo) < (b_hi, b_lo)` where all 4 limbs
/// are already range-checked to `< 2^128`.
pub fn enforce_u256_lt_circuit<F: PrimeField, CS: ConstraintSystem<F>>(
    mut cs: CS,
    a_hi: &AllocatedNum<F>,
    a_lo: &AllocatedNum<F>,
    b_hi: &AllocatedNum<F>,
    b_lo: &AllocatedNum<F>,
) -> Result<(), SynthesisError> {
    // Witness boolean `hi_lt`: true iff a_hi < b_hi (otherwise a_hi == b_hi and a_lo < b_lo).
    let hi_eq_bool = Boolean::from(nova_snark::frontend::AllocatedBit::alloc(
        cs.namespace(|| "hi_eq"),
        a_hi.get_value()
            .zip(b_hi.get_value())
            .map(|(ah, bh)| ah == bh),
    )?);

    // If hi_eq == 1: enforce a_hi == b_hi, and diff = b_lo - a_lo - 1 >= 0 (< 2^128).
    // If hi_eq == 0: diff = b_hi - a_hi - 1 >= 0 (< 2^128).
    cs.enforce(
        || "if hi_eq then a_hi == b_hi",
        |lc| lc + &hi_eq_bool.lc(CS::one(), F::ONE),
        |lc| lc + b_hi.get_variable() - a_hi.get_variable(),
        |lc| lc,
    );

    let diff = AllocatedNum::alloc(cs.namespace(|| "lt_diff"), || {
        let ah = a_hi.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let al = a_lo.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let bh = b_hi.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let bl = b_lo.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        let eq = hi_eq_bool.get_value().ok_or(SynthesisError::AssignmentMissing)?;
        Ok(if eq {
            bl - al - F::ONE
        } else {
            bh - ah - F::ONE
        })
    })?;

    // Constrain diff == (b_hi - a_hi - 1) + hi_eq * ((b_lo - a_lo) - (b_hi - a_hi))
    // Note: when hi_eq == 1, b_hi - a_hi == 0 is already enforced above, so
    // diff - (b_hi - a_hi - 1) == hi_eq * (b_lo - a_lo)!
    cs.enforce(
        || "select hi or lo strict diff",
        |lc| lc + &hi_eq_bool.lc(CS::one(), F::ONE),
        |lc| lc + b_lo.get_variable() - a_lo.get_variable(),
        |lc| lc + diff.get_variable() - b_hi.get_variable() + a_hi.get_variable() + CS::one(),
    );

    // Range check diff < 2^128. Since all limbs are in [0, 2^128), b - a - 1 in [0, 2^128)
    // holds in F_p iff b > a as integers! (If b <= a, b - a - 1 wraps to p - (a - b + 1) >= p - 2^128 >> 2^128.)
    let _ = range_check_bits_le(cs.namespace(|| "diff_u128"), &diff, 128)?;
    Ok(())
}

// ============================================================================
// Native Fixed-Depth Sparse/Dense Poseidon Merkle Tree Helper
// ============================================================================

/// Generic depth-12 Poseidon Merkle tree over scalar leaves (default empty leaf = `F::ZERO`).
#[derive(Clone, Debug)]
pub struct SparseMerkleTree<F: PrimeField> {
    leaves: Vec<F>,
    empty_hashes: [F; STATE_TREE_DEPTH + 1],
}

impl<F: PrimeField + PrimeFieldBits> SparseMerkleTree<F> {
    /// Construct a new empty depth-12 tree (`4,096` slots initialized to `F::ZERO`).
    pub fn new(hasher: &PoseidonHasher<F>) -> Self {
        let mut empty_hashes = [F::ZERO; STATE_TREE_DEPTH + 1];
        for lvl in 0..STATE_TREE_DEPTH {
            empty_hashes[lvl + 1] = hasher.hash_node_native(empty_hashes[lvl], empty_hashes[lvl]);
        }
        Self {
            leaves: vec![F::ZERO; 1 << STATE_TREE_DEPTH],
            empty_hashes,
        }
    }

    /// Set a leaf scalar at `idx`.
    pub fn set_leaf(&mut self, idx: usize, val: F) {
        self.leaves[idx] = val;
    }

    /// Get leaf scalar at `idx`.
    pub fn get_leaf(&self, idx: usize) -> F {
        self.leaves[idx]
    }

    /// Compute current root and Merkle siblings for `target_idx`.
    pub fn root_and_path(&self, hasher: &PoseidonHasher<F>, target_idx: usize) -> (F, [F; STATE_TREE_DEPTH]) {
        let mut layer = self.leaves.clone();
        let mut siblings = [F::ZERO; STATE_TREE_DEPTH];
        let mut idx = target_idx;
        for lvl in 0..STATE_TREE_DEPTH {
            siblings[lvl] = layer[idx ^ 1];
            let next_len = layer.len() / 2;
            let mut next = Vec::with_capacity(next_len);
            for i in 0..next_len {
                let l = layer[2 * i];
                let r = layer[2 * i + 1];
                if l == self.empty_hashes[lvl] && r == self.empty_hashes[lvl] {
                    next.push(self.empty_hashes[lvl + 1]);
                } else {
                    next.push(hasher.hash_node_native(l, r));
                }
            }
            layer = next;
            idx >>= 1;
        }
        (layer[0], siblings)
    }

    /// Compute current root.
    pub fn root(&self, hasher: &PoseidonHasher<F>) -> F {
        self.root_and_path(hasher, 0).0
    }
}

// ============================================================================
// 1. ChannelRegistryLeaf & Tree
// ============================================================================

/// Domain tag for ChannelRegistry leaves (`"CREG"`).
pub const DOMAIN_CHANNEL_REGISTRY_LEAF: u64 = 0x4352_4547;

/// Registered channel entry binding canonical funding outpoint, immutable peer ordering,
/// rotatable receipt x-only keys, asset flag/ID, and resolved funding state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelRegistryLeaf {
    /// Display-order funding txid (32B).
    pub txid_display: [u8; 32],
    /// Funding output index (`u32`).
    pub vout: u32,
    /// Receipt x-only key for BOLT-2 `party_0` (lexicographically smaller 33B node pubkey).
    pub party0_rec_key: [u8; 32],
    /// Receipt x-only key for BOLT-2 `party_1`.
    pub party1_rec_key: [u8; 32],
    /// `true` for Taproot Asset custom channel; `false` for standard BTC channel.
    pub is_asset_channel: bool,
    /// Bound Taproot Asset ID (zero for standard BTC channel).
    pub asset_id: [u8; 32],
    /// `true` iff funding outpoint is confirmed and no splice is unresolved.
    pub funding_resolved: bool,
}

impl ChannelRegistryLeaf {
    /// Compute Poseidon leaf commitment in `F`.
    pub fn hash<F: PrimeField + PrimeFieldBits>(&self, hasher: &PoseidonHasher<F>) -> F {
        let (tx_hi, tx_lo) = bytes32_to_u128_pair(&self.txid_display);
        let (p0_hi, p0_lo) = bytes32_to_u128_pair(&self.party0_rec_key);
        let (p1_hi, p1_lo) = bytes32_to_u128_pair(&self.party1_rec_key);
        let (as_hi, as_lo) = bytes32_to_u128_pair(&self.asset_id);
        let flags = (self.vout as u64)
            | ((self.is_asset_channel as u64) << 32)
            | ((self.funding_resolved as u64) << 33)
            | (1u64 << 34); // active bit
        hasher.hash_native(
            DOMAIN_CHANNEL_REGISTRY_LEAF,
            &[
                u128_to_scalar(tx_hi),
                u128_to_scalar(tx_lo),
                u128_to_scalar(p0_hi),
                u128_to_scalar(p0_lo),
                u128_to_scalar(p1_hi),
                u128_to_scalar(p1_lo),
                u128_to_scalar(as_hi),
                u128_to_scalar(as_lo),
                F::from(flags),
            ],
        )
    }
}

// ============================================================================
// 2. IndexedNullifierTree (Persistent Cross-Epoch 256-bit Non-Membership Tree)
// ============================================================================

/// Domain tag for IndexedNullifier leaves (`"INUL"`).
pub const DOMAIN_INDEXED_NULLIFIER_LEAF: u64 = 0x494e_554c;

/// Single active leaf in the Indexed Merkle Tree over 256-bit nullifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedNullifierLeaf {
    /// Active 256-bit nullifier `(hi, lo)`. Sentinel slot 0 has `(0, 0)`.
    pub val_hi: u128,
    /// Lower 128 bits of `val`.
    pub val_lo: u128,
    /// Next-higher active 256-bit nullifier `(hi, lo)` (`(u128::MAX, u128::MAX)` at tail).
    pub next_hi: u128,
    /// Lower 128 bits of `next`.
    pub next_lo: u128,
    /// Whether this slot is populated (`false` -> leaf hash is `F::ZERO`).
    pub active: bool,
}

impl IndexedNullifierLeaf {
    /// Compute Poseidon leaf hash (`F::ZERO` when `active == false`).
    pub fn hash<F: PrimeField + PrimeFieldBits>(&self, hasher: &PoseidonHasher<F>) -> F {
        if !self.active {
            return F::ZERO;
        }
        hasher.hash_native(
            DOMAIN_INDEXED_NULLIFIER_LEAF,
            &[
                u128_to_scalar(self.val_hi),
                u128_to_scalar(self.val_lo),
                u128_to_scalar(self.next_hi),
                u128_to_scalar(self.next_lo),
                F::ONE,
            ],
        )
    }
}

/// Witness returned when proving non-membership and inserting a 256-bit nullifier.
#[derive(Clone, Debug)]
pub struct NullifierInsertWitness<F: PrimeField> {
    /// Index of the low leaf satisfying `low.val < new_nul < low.next`.
    pub low_idx: usize,
    /// Pre-insertion low leaf.
    pub low_leaf_before: IndexedNullifierLeaf,
    /// Merkle siblings of `low_idx` against `root_before`.
    pub low_siblings: [F; STATE_TREE_DEPTH],
    /// Root before insertion.
    pub root_before: F,
    /// Intermediate root after updating `low_leaf.next = new_nul`.
    pub root_mid: F,
    /// Next free slot index (`>= 1`).
    pub insert_idx: usize,
    /// Merkle siblings of `insert_idx` against `root_mid` (where leaf at `insert_idx` is `0`).
    pub insert_siblings: [F; STATE_TREE_DEPTH],
    /// Root after writing new leaf `(val = new_nul, next = low_leaf_before.next, active = true)`.
    pub root_after: F,
}

/// Persistent cross-epoch Indexed Merkle Tree over 256-bit canonical HTLC nullifiers.
#[derive(Clone, Debug)]
pub struct IndexedNullifierTree<F: PrimeField> {
    /// Leaf records (slot 0 is sentinel `(0, MAX)`).
    pub entries: Vec<IndexedNullifierLeaf>,
    /// Next empty leaf index.
    pub next_free_idx: usize,
    /// Underlying Poseidon Merkle tree.
    pub tree: SparseMerkleTree<F>,
}

impl<F: PrimeField + PrimeFieldBits> IndexedNullifierTree<F> {
    /// Create a fresh global nullifier tree with sentinel leaf 0 (`(0, 0) -> (MAX, MAX)`).
    pub fn new(hasher: &PoseidonHasher<F>) -> Self {
        let cap = 1 << STATE_TREE_DEPTH;
        let mut entries = vec![
            IndexedNullifierLeaf {
                val_hi: 0,
                val_lo: 0,
                next_hi: 0,
                next_lo: 0,
                active: false,
            };
            cap
        ];
        let sentinel = IndexedNullifierLeaf {
            val_hi: 0,
            val_lo: 0,
            next_hi: u128::MAX,
            next_lo: u128::MAX,
            active: true,
        };
        entries[0] = sentinel;
        let mut tree = SparseMerkleTree::new(hasher);
        tree.set_leaf(0, sentinel.hash(hasher));
        Self {
            entries,
            next_free_idx: 1,
            tree,
        }
    }

    /// Current root.
    pub fn root(&self, hasher: &PoseidonHasher<F>) -> F {
        self.tree.root(hasher)
    }

    /// Check whether `nul` is already present.
    pub fn contains(&self, nul: &[u8; 32]) -> bool {
        let (nh, nl) = bytes32_to_u128_pair(nul);
        self.entries[..self.next_free_idx]
            .iter()
            .any(|e| e.active && e.val_hi == nh && e.val_lo == nl)
    }

    /// Prove non-membership of `nul` and insert it, returning the circuit witness.
    /// Fails closed if `nul` is zero, `MAX`, already spent, or tree is full.
    pub fn insert_and_prove(
        &mut self,
        hasher: &PoseidonHasher<F>,
        nul: &[u8; 32],
    ) -> Result<NullifierInsertWitness<F>, String> {
        let (nh, nl) = bytes32_to_u128_pair(nul);
        if (nh == 0 && nl == 0) || (nh == u128::MAX && nl == u128::MAX) {
            return Err("reserved boundary nullifier".to_string());
        }
        if self.next_free_idx >= self.entries.len() {
            return Err("nullifier tree capacity exhausted".to_string());
        }
        // Check duplicate first and locate unique low leaf with val < nul < next.
        let mut found_low: Option<usize> = None;
        for idx in 0..self.next_free_idx {
            let e = &self.entries[idx];
            if !e.active {
                continue;
            }
            if (e.val_hi, e.val_lo) == (nh, nl) {
                return Err(format!("duplicate nullifier {}", hex::encode(nul)));
            }
            if (e.val_hi, e.val_lo) < (nh, nl) && (nh, nl) < (e.next_hi, e.next_lo) {
                found_low = Some(idx);
                break;
            }
        }
        let low_idx = found_low.ok_or_else(|| "no valid low-leaf interval for nullifier".to_string())?;
        let low_leaf_before = self.entries[low_idx];
        let (root_before, low_siblings) = self.tree.root_and_path(hasher, low_idx);

        // Step A: update low leaf's next pointer to (nh, nl).
        let updated_low = IndexedNullifierLeaf {
            val_hi: low_leaf_before.val_hi,
            val_lo: low_leaf_before.val_lo,
            next_hi: nh,
            next_lo: nl,
            active: true,
        };
        self.entries[low_idx] = updated_low;
        self.tree.set_leaf(low_idx, updated_low.hash(hasher));

        let insert_idx = self.next_free_idx;
        let (root_mid, insert_siblings) = self.tree.root_and_path(hasher, insert_idx);

        // Step B: write new leaf at `insert_idx`.
        let new_leaf = IndexedNullifierLeaf {
            val_hi: nh,
            val_lo: nl,
            next_hi: low_leaf_before.next_hi,
            next_lo: low_leaf_before.next_lo,
            active: true,
        };
        self.entries[insert_idx] = new_leaf;
        self.tree.set_leaf(insert_idx, new_leaf.hash(hasher));
        self.next_free_idx += 1;

        let root_after = self.tree.root(hasher);
        Ok(NullifierInsertWitness {
            low_idx,
            low_leaf_before,
            low_siblings,
            root_before,
            root_mid,
            insert_idx,
            insert_siblings,
            root_after,
        })
    }
}

// ============================================================================
// 3. QuoteConsumptionTree (Persistent Cumulative Quote-Capacity Tree)
// ============================================================================

/// Domain tag for QuoteConsumption leaves (`"QCAP"`).
pub const DOMAIN_QUOTE_CONSUMPTION_LEAF: u64 = 0x5143_4150;

/// Quote capacity state leaf keyed by registered quote slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteConsumptionLeaf {
    /// Full 32-byte RFQ session ID.
    pub rfq_id: [u8; 32],
    /// 2-party signed QuoteTranscriptV1 digest (32B).
    pub quote_transcript: [u8; 32],
    /// Maximum cumulative outgoing millisatoshis authorized by the quote policy.
    pub max_out_msat: u64,
    /// Cumulative outgoing millisatoshis consumed so far across all fills/epochs.
    pub consumed_out_msat: u64,
    /// Active flag (`false` -> `F::ZERO`).
    pub active: bool,
}

impl QuoteConsumptionLeaf {
    /// Compute Poseidon leaf hash.
    pub fn hash<F: PrimeField + PrimeFieldBits>(&self, hasher: &PoseidonHasher<F>) -> F {
        if !self.active {
            return F::ZERO;
        }
        let (rh, rl) = bytes32_to_u128_pair(&self.rfq_id);
        let (qh, ql) = bytes32_to_u128_pair(&self.quote_transcript);
        hasher.hash_native(
            DOMAIN_QUOTE_CONSUMPTION_LEAF,
            &[
                u128_to_scalar(rh),
                u128_to_scalar(rl),
                u128_to_scalar(qh),
                u128_to_scalar(ql),
                F::from(self.max_out_msat),
                F::from(self.consumed_out_msat),
                F::ONE,
            ],
        )
    }
}

/// Witness for updating cumulative quote consumption on a conversion step.
#[derive(Clone, Debug)]
pub struct QuoteConsumeWitness<F: PrimeField> {
    /// Quote slot index.
    pub quote_idx: usize,
    /// Leaf state before applying this fill.
    pub leaf_before: QuoteConsumptionLeaf,
    /// Merkle siblings of `quote_idx`.
    pub siblings: [F; STATE_TREE_DEPTH],
    /// Quote root before fill.
    pub root_before: F,
    /// Outgoing msat consumed by this fill (`> 0`).
    pub quote_delta_msat: u64,
    /// Quote root after fill.
    pub root_after: F,
}

/// Persistent cumulative quote-capacity Merkle tree.
#[derive(Clone, Debug)]
pub struct QuoteConsumptionTree<F: PrimeField> {
    /// Leaf records.
    pub entries: Vec<QuoteConsumptionLeaf>,
    /// Underlying Poseidon Merkle tree.
    pub tree: SparseMerkleTree<F>,
}

impl<F: PrimeField + PrimeFieldBits> QuoteConsumptionTree<F> {
    /// Construct an empty quote tree.
    pub fn new(hasher: &PoseidonHasher<F>) -> Self {
        let cap = 1 << STATE_TREE_DEPTH;
        Self {
            entries: vec![
                QuoteConsumptionLeaf {
                    rfq_id: [0u8; 32],
                    quote_transcript: [0u8; 32],
                    max_out_msat: 0,
                    consumed_out_msat: 0,
                    active: false,
                };
                cap
            ],
            tree: SparseMerkleTree::new(hasher),
        }
    }

    /// Register or initialize a quote slot prior to proving (or when admitted to registry).
    pub fn register_quote(
        &mut self,
        hasher: &PoseidonHasher<F>,
        idx: usize,
        rfq_id: [u8; 32],
        quote_transcript: [u8; 32],
        max_out_msat: u64,
    ) -> Result<(), String> {
        if max_out_msat == 0 {
            return Err("quote max_out_msat must be > 0".to_string());
        }
        let leaf = QuoteConsumptionLeaf {
            rfq_id,
            quote_transcript,
            max_out_msat,
            consumed_out_msat: 0,
            active: true,
        };
        self.entries[idx] = leaf;
        self.tree.set_leaf(idx, leaf.hash(hasher));
        Ok(())
    }

    /// Current root.
    pub fn root(&self, hasher: &PoseidonHasher<F>) -> F {
        self.tree.root(hasher)
    }

    /// Verify quote binding & capacity and consume `quote_delta_msat`.
    pub fn consume_and_prove(
        &mut self,
        hasher: &PoseidonHasher<F>,
        idx: usize,
        expected_rfq_id: &[u8; 32],
        expected_transcript: &[u8; 32],
        quote_delta_msat: u64,
    ) -> Result<QuoteConsumeWitness<F>, String> {
        if quote_delta_msat == 0 {
            return Err("quote_delta_msat must be > 0".to_string());
        }
        let leaf_before = self.entries[idx];
        if !leaf_before.active {
            return Err(format!("quote slot {} is inactive", idx));
        }
        if &leaf_before.rfq_id != expected_rfq_id {
            return Err("quote slot rfq_id mismatch".to_string());
        }
        if &leaf_before.quote_transcript != expected_transcript {
            return Err("quote slot transcript commitment mismatch".to_string());
        }
        let new_consumed = leaf_before
            .consumed_out_msat
            .checked_add(quote_delta_msat)
            .ok_or_else(|| "quote consumed u64 overflow".to_string())?;
        if new_consumed > leaf_before.max_out_msat {
            return Err(format!(
                "quote capacity exceeded: {} + {} = {} > {}",
                leaf_before.consumed_out_msat,
                quote_delta_msat,
                new_consumed,
                leaf_before.max_out_msat
            ));
        }
        let (root_before, siblings) = self.tree.root_and_path(hasher, idx);
        let mut leaf_after = leaf_before;
        leaf_after.consumed_out_msat = new_consumed;
        self.entries[idx] = leaf_after;
        self.tree.set_leaf(idx, leaf_after.hash(hasher));
        let root_after = self.tree.root(hasher);
        Ok(QuoteConsumeWitness {
            quote_idx: idx,
            leaf_before,
            siblings,
            root_before,
            quote_delta_msat,
            root_after,
        })
    }
}
