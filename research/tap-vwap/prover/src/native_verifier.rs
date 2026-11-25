//! Native fail-closed witness verifier for TAP-VWAP-03 (`libsecp256k1` BIP-340).
//!
//! Runs BEFORE Nova proving (`CARBON_HANDOFF.md` §4): validates every structural,
//! signature, registry-inclusion, persistent Indexed-Merkle nullifier, cumulative
//! quote-capacity, and exact half-up `q8` VWAP relation natively.

use ff::{PrimeField, PrimeFieldBits};
use secp256k1::{schnorr::Signature, Secp256k1, XOnlyPublicKey};

use crate::manifest::{
    compute_vwap_q8_half_up, tagged_sha256, PublicEpochManifest, ReceiptV1, ThreePartySignatures,
    EVIDENCE_TIER_1_SIGNED_ASSERTION,
};
use crate::merkle::{
    scalar_to_bytes32, ChannelRegistryLeaf, IndexedNullifierTree, NullifierInsertWitness,
    PoseidonHasher, QuoteConsumeWitness, QuoteConsumptionTree, SparseMerkleTree, STATE_TREE_DEPTH,
};

/// Complete witness for a single receipt conversion step.
#[derive(Clone, Debug)]
pub struct ReceiptStepWitness {
    /// Decoded Receipt/v1 record (526B on wire).
    pub receipt: ReceiptV1,
    /// Three 64-byte BIP-340 Schnorr signatures over `receipt.digest()`.
    pub signatures: ThreePartySignatures,
    /// Registry slot index of the asset custom channel.
    pub asset_chan_idx: usize,
    /// Registry slot index of the BTC channel.
    pub btc_chan_idx: usize,
    /// Quote consumption slot index.
    pub quote_idx: usize,
    /// Native quote outgoing msat consumed (`== gross_btc_msat` for purchase;
    /// `<= gross_btc_msat` for sale, where difference is edge routing fee).
    pub quote_delta_msat: u64,
}

/// Prepared step witness with precomputed Merkle paths ready for Nova `StepCircuit`.
#[derive(Clone, Debug)]
pub struct PreparedReceiptStep<F: PrimeField> {
    /// Raw 526-byte canonical wire encoding.
    pub raw_receipt: [u8; 526],
    /// Decoded receipt.
    pub receipt: ReceiptV1,
    /// Three BIP-340 signatures.
    pub signatures: ThreePartySignatures,
    /// Asset channel registry leaf and Merkle siblings.
    pub asset_reg_idx: usize,
    /// Asset channel registry leaf.
    pub asset_reg_leaf: ChannelRegistryLeaf,
    /// Asset channel registry Merkle siblings.
    pub asset_reg_siblings: [F; STATE_TREE_DEPTH],
    /// BTC channel registry index.
    pub btc_reg_idx: usize,
    /// BTC channel registry leaf.
    pub btc_reg_leaf: ChannelRegistryLeaf,
    /// BTC channel registry Merkle siblings.
    pub btc_reg_siblings: [F; STATE_TREE_DEPTH],
    /// Indexed Merkle nullifier non-membership + insertion witness.
    pub nullifier_witness: NullifierInsertWitness<F>,
    /// Quote capacity check + update witness.
    pub quote_witness: QuoteConsumeWitness<F>,
    /// Previous batch log hash-chain digest.
    pub prev_batch_log: [u8; 32],
    /// Next batch log hash-chain digest.
    pub next_batch_log: [u8; 32],
}

/// Verify a single BIP-340 Schnorr signature using reviewed `rust-secp256k1` (`libsecp256k1`).
pub fn verify_bip340_libsecp(
    pubkey_xonly: &[u8; 32],
    msg32: &[u8; 32],
    sig64: &[u8; 64],
) -> Result<(), String> {
    let secp = Secp256k1::verification_only();
    let pk = XOnlyPublicKey::from_slice(pubkey_xonly)
        .map_err(|e| format!("invalid BIP-340 x-only pubkey: {e}"))?;
    let sig =
        Signature::from_slice(sig64).map_err(|e| format!("invalid BIP-340 sig bytes: {e}"))?;
    secp.verify_schnorr(&sig, msg32, &pk)
        .map_err(|e| format!("BIP-340 verification failed: {e}"))
}

/// Authenticated ChannelRegistry state backing `registry_root`.
#[derive(Clone, Debug)]
pub struct AuthenticatedChannelRegistry<F: PrimeField> {
    /// Registered channel leaves.
    pub entries: Vec<Option<ChannelRegistryLeaf>>,
    /// Poseidon Merkle tree.
    pub tree: SparseMerkleTree<F>,
}

impl<F: PrimeField + PrimeFieldBits> AuthenticatedChannelRegistry<F> {
    /// Construct an empty registry tree.
    pub fn new(hasher: &PoseidonHasher<F>) -> Self {
        let cap = 1 << STATE_TREE_DEPTH;
        Self {
            entries: vec![None; cap],
            tree: SparseMerkleTree::new(hasher),
        }
    }

    /// Insert a channel leaf at `idx`.
    pub fn register_channel(
        &mut self,
        hasher: &PoseidonHasher<F>,
        idx: usize,
        leaf: ChannelRegistryLeaf,
    ) -> Result<(), String> {
        if !leaf.funding_resolved {
            return Err("cannot register channel with unresolved funding/splice".to_string());
        }
        if leaf.txid_display == [0u8; 32] || leaf.vout == u32::MAX {
            return Err("unresolved funding outpoint in registry".to_string());
        }
        if leaf.party0_rec_key == leaf.party1_rec_key {
            return Err("channel peers must have distinct receipt keys".to_string());
        }
        let h = leaf.hash(hasher);
        self.entries[idx] = Some(leaf);
        self.tree.set_leaf(idx, h);
        Ok(())
    }

    /// Current `registry_root`.
    pub fn root(&self, hasher: &PoseidonHasher<F>) -> F {
        self.tree.root(hasher)
    }
}

/// Fail-closed native epoch witness verifier.
///
/// Validates all receipts, mutates `nullifier_tree` and `quote_tree` ONLY on full epoch
/// validity (clones internally until success), and returns `(PublicEpochManifest, Vec<PreparedReceiptStep<F>>)`.
#[allow(clippy::too_many_arguments)]
pub fn verify_and_prepare_epoch_native<F: PrimeField + PrimeFieldBits>(
    hasher: &PoseidonHasher<F>,
    policy_hash: [u8; 32],
    chain_genesis: [u8; 32],
    asset_id: [u8; 32],
    asset_decimals: u8,
    epoch_id: u64,
    registry: &AuthenticatedChannelRegistry<F>,
    nullifier_tree: &mut IndexedNullifierTree<F>,
    quote_tree: &mut QuoteConsumptionTree<F>,
    steps: &[ReceiptStepWitness],
) -> Result<(PublicEpochManifest, Vec<PreparedReceiptStep<F>>), String> {
    if steps.is_empty() {
        return Err("empty epoch window rejected".to_string());
    }
    if asset_decimals > 19 {
        return Err("asset_decimals > 19 unsupported".to_string());
    }

    let mut working_nullifiers = nullifier_tree.clone();
    let mut working_quotes = quote_tree.clone();

    let reg_root_scalar = registry.root(hasher);
    let prev_nul_scalar = working_nullifiers.root(hasher);
    let prev_quote_scalar = working_quotes.root(hasher);

    let mut batch_log = [0u8; 32];
    let mut sum_btc_msat: u128 = 0;
    let mut sum_asset_atoms: u128 = 0;
    let mut prepared = Vec::with_capacity(steps.len());

    for (step_i, w) in steps.iter().enumerate() {
        let raw_receipt = w
            .receipt
            .encode()
            .map_err(|e| format!("step {step_i}: encode failed: {e}"))?;

        // Policy, genesis, and asset binding.
        if w.receipt.policy != policy_hash {
            return Err(format!("step {step_i}: policy_hash mismatch"));
        }
        if w.receipt.genesis != chain_genesis {
            return Err(format!("step {step_i}: chain_genesis mismatch"));
        }
        if w.receipt.asset != asset_id {
            return Err(format!("step {step_i}: asset_id mismatch"));
        }

        // Three-party BIP-340 verification via reviewed libsecp256k1.
        let digest = w.receipt.digest()?;
        verify_bip340_libsecp(&w.receipt.edge_key, &digest, &w.signatures.edge)
            .map_err(|e| format!("step {step_i} edge sig: {e}"))?;
        verify_bip340_libsecp(&w.receipt.btc_peer_key, &digest, &w.signatures.btc_peer)
            .map_err(|e| format!("step {step_i} btc_peer sig: {e}"))?;
        verify_bip340_libsecp(&w.receipt.asset_peer_key, &digest, &w.signatures.asset_peer)
            .map_err(|e| format!("step {step_i} asset_peer sig: {e}"))?;

        // Authenticated ChannelRegistry lookup (no prover-chosen keys!).
        if w.asset_chan_idx == w.btc_chan_idx {
            return Err(format!("step {step_i}: asset and BTC registry slots must differ"));
        }
        let asset_leaf = registry
            .entries
            .get(w.asset_chan_idx)
            .and_then(|x| x.clone())
            .ok_or_else(|| format!("step {step_i}: unregistered asset channel"))?;
        let btc_leaf = registry
            .entries
            .get(w.btc_chan_idx)
            .and_then(|x| x.clone())
            .ok_or_else(|| format!("step {step_i}: unregistered BTC channel"))?;

        if !asset_leaf.is_asset_channel || !asset_leaf.funding_resolved {
            return Err(format!("step {step_i}: invalid asset channel flags in registry"));
        }
        if asset_leaf.asset_id != asset_id {
            return Err(format!("step {step_i}: registry asset_id mismatch"));
        }
        if asset_leaf.txid_display != w.receipt.asset_outpoint.txid_display
            || asset_leaf.vout != w.receipt.asset_outpoint.vout
        {
            return Err(format!("step {step_i}: asset outpoint mismatch vs registry"));
        }

        if btc_leaf.is_asset_channel || !btc_leaf.funding_resolved {
            return Err(format!("step {step_i}: invalid BTC channel flags in registry"));
        }
        if btc_leaf.txid_display != w.receipt.btc_outpoint.txid_display
            || btc_leaf.vout != w.receipt.btc_outpoint.vout
        {
            return Err(format!("step {step_i}: BTC outpoint mismatch vs registry"));
        }

        // Verify immutable peer ordering (`asset_edge_side`, `btc_edge_side`) maps to exact registered keys!
        let (exp_edge_a, exp_asset_peer) = if w.receipt.asset_edge_side == 0 {
            (asset_leaf.party0_rec_key, asset_leaf.party1_rec_key)
        } else {
            (asset_leaf.party1_rec_key, asset_leaf.party0_rec_key)
        };
        if w.receipt.edge_key != exp_edge_a || w.receipt.asset_peer_key != exp_asset_peer {
            return Err(format!("step {step_i}: asset channel peer key binding mismatch"));
        }

        let (exp_edge_b, exp_btc_peer) = if w.receipt.btc_edge_side == 0 {
            (btc_leaf.party0_rec_key, btc_leaf.party1_rec_key)
        } else {
            (btc_leaf.party1_rec_key, btc_leaf.party0_rec_key)
        };
        if w.receipt.edge_key != exp_edge_b || w.receipt.btc_peer_key != exp_btc_peer {
            return Err(format!("step {step_i}: BTC channel peer key binding mismatch"));
        }

        let (_, asset_reg_siblings) = registry.tree.root_and_path(hasher, w.asset_chan_idx);
        let (_, btc_reg_siblings) = registry.tree.root_and_path(hasher, w.btc_chan_idx);

        // Quote delta reconciliation against direction & gross BTC msat:
        // - Purchase (direction 0): outgoing leg IS the BTC leg -> quote_delta_msat == gross_btc_msat.
        // - Sale (direction 1): incoming leg is gross BTC (`AmtInMsat`), outgoing msat (`AmtOutMsat`)
        //   is `<= gross_btc_msat` (difference is non-negative edge routing fee).
        if w.quote_delta_msat == 0 {
            return Err(format!("step {step_i}: quote_delta_msat must be > 0"));
        }
        if w.receipt.direction == 0 {
            if w.quote_delta_msat != w.receipt.gross_btc_msat {
                return Err(format!(
                    "step {step_i}: purchase requires quote_delta_msat == gross_btc_msat"
                ));
            }
        } else if w.quote_delta_msat > w.receipt.gross_btc_msat {
            return Err(format!(
                "step {step_i}: sale requires quote_delta_msat <= gross_btc_msat"
            ));
        }

        // Persistent Indexed Merkle nullifier non-membership + insertion.
        let nul = w.receipt.nullifier()?;
        let nul_wit = working_nullifiers
            .insert_and_prove(hasher, &nul)
            .map_err(|e| format!("step {step_i} nullifier: {e}"))?;

        // Persistent cumulative quote-capacity check + update.
        let quote_wit = working_quotes
            .consume_and_prove(
                hasher,
                w.quote_idx,
                &w.receipt.rfq_id,
                &w.receipt.quote_transcript,
                w.quote_delta_msat,
            )
            .map_err(|e| format!("step {step_i} quote: {e}"))?;

        // Bounded volume accumulation (< 2^80 msat, < 2^112 asset atoms).
        sum_btc_msat = sum_btc_msat
            .checked_add(w.receipt.gross_btc_msat as u128)
            .ok_or_else(|| "sum_btc_msat overflow".to_string())?;
        if sum_btc_msat >= (1u128 << 80) {
            return Err("sum_btc_msat exceeds 80-bit circuit bound".to_string());
        }
        sum_asset_atoms = sum_asset_atoms
            .checked_add(w.receipt.asset_atoms as u128)
            .ok_or_else(|| "sum_asset_atoms overflow".to_string())?;
        if sum_asset_atoms >= (1u128 << 112) {
            return Err("sum_asset_atoms exceeds 112-bit circuit bound".to_string());
        }

        let prev_batch_log = batch_log;
        let mut log_preimage = [0u8; 64];
        log_preimage[..32].copy_from_slice(&prev_batch_log);
        log_preimage[32..64].copy_from_slice(&digest);
        batch_log = tagged_sha256(b"TradeLayer/TAP-VWAP/BatchLog/v1", &log_preimage);

        prepared.push(PreparedReceiptStep {
            raw_receipt,
            receipt: w.receipt.clone(),
            signatures: w.signatures,
            asset_reg_idx: w.asset_chan_idx,
            asset_reg_leaf: asset_leaf,
            asset_reg_siblings,
            btc_reg_idx: w.btc_chan_idx,
            btc_reg_leaf: btc_leaf,
            btc_reg_siblings,
            nullifier_witness: nul_wit,
            quote_witness: quote_wit,
            prev_batch_log,
            next_batch_log: batch_log,
        });
    }

    let (vwap_price_q8, _, _, _) =
        compute_vwap_q8_half_up(sum_asset_atoms, sum_btc_msat, asset_decimals)?;

    let next_nul_scalar = working_nullifiers.root(hasher);
    let next_quote_scalar = working_quotes.root(hasher);

    // Commit state mutations only after entire epoch passed!
    *nullifier_tree = working_nullifiers;
    *quote_tree = working_quotes;

    let manifest = PublicEpochManifest {
        policy_hash,
        chain_genesis,
        asset_id,
        asset_decimals,
        epoch_id,
        registry_root: scalar_to_bytes32(&reg_root_scalar),
        prev_nullifier_root: scalar_to_bytes32(&prev_nul_scalar),
        next_nullifier_root: scalar_to_bytes32(&next_nul_scalar),
        prev_quote_root: scalar_to_bytes32(&prev_quote_scalar),
        next_quote_root: scalar_to_bytes32(&next_quote_scalar),
        batch_log_commitment: batch_log,
        sum_btc_msat,
        sum_asset_atoms,
        conversion_count: steps.len() as u64,
        vwap_price_q8,
        evidence_tier: EVIDENCE_TIER_1_SIGNED_ASSERTION,
        eligible_for_consensus_index: false,
    };

    Ok((manifest, prepared))
}
