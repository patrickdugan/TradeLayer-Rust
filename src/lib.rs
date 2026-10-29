//! Core protocol types for the incremental TradeLayer Rust migration.
//!
//! Economic transitions and source-compatible dispatch are explicitly unverified.
//! Constructing a transaction type does not validate or authorize a transaction.

#![forbid(unsafe_code)]

pub mod activation; // Complete source activation metadata; funded genesis stays with the owner.
pub mod activation_history; // Origins derived from the actual complete owner replay.
pub mod adjuster; // Source tick adjustment metadata; no automatic orderbook policy.
pub mod amm;
pub mod bankruptcy_source_audit; // Stage25F pure source-level bankruptcy algebra, no market authority.
pub mod bitcoin_chainwork; // Bitcoin mainnet nBits/MTP/chainwork from explicit checkpoints; not full consensus.
pub mod bitcoin_forks; // Bounded greatest-work selection among observed header forks; no active-chain trust.
pub mod bitcoin_inclusion;
pub mod bitcoin_witness_commitment; // Stage25M unverified Bitcoin BIP141 full-tx witness evidence.
pub mod bitvm_bundle; // Bounded source artifact integrity, no execution-proof verification.
pub mod bitvm_cache; // Source cache challenge lifecycle metadata, no funded custody.
pub mod bitvm_risk; // Atomic source counter/window metadata, no spend authorization.
                    // Bitcoin SHA256d header and tx Merkle checks; no full consensus.
pub mod bip32_public; // Public BIP32 derivation with strict and pinned-source modes.
pub mod bootstrap; // Owned startup readiness with explicit source characterization.
pub mod bootstrap_storage; // Concrete restoration of complete durable state owners.
pub mod candidate_evidence;
pub mod chain; // Trust-boundary and reversible block progression, prototype only.
pub mod channel_admission_audit; // Stage25M claimed as-of type4/type19 direct clearlist evidence, never authorization.
pub mod channel_bip143_audit; // Stage 25I exact SegWit-v0 SIGHASH_ALL digest; no signature verification.
pub mod channel_inclusion_audit; // Stage25L relative Bitcoin Merkle/witness non-authority.
pub mod channel_p2sh_audit; // Stage 25K legacy P2SH HASH160 outpoint and redeem script, not ECDSA.
pub mod channel_p2sh_sighash_audit; // Stage 25L read-only legacy P2SH SIGHASH_ALL digest, no ECDSA authority.
pub mod channel_revision; // Explicit co-signed allocation policy; L1 funding/revocation external.
pub mod channel_type9_inclusion_audit; // Stage25N raw type9 event/Bitcoin relative Merkle evidence, no admission.
pub mod channel_type9_sender_audit; // Stage25O claimed Bitcoin P2WPKH first-input digest and sender-bound candidate, never permission.
pub mod channel_witness_audit; // Stage 25H raw native P2WSH script/outpoint only; not ECDSA authorization.
pub mod channels_source; // Complete bounded source metadata; no funded custody authority.
pub mod clearing_cadence; // Stage 24C read-only JS clearing callback and funding schedule observations.
pub mod clearing_chain; // Stage 25D unverified combined spot-clearing rollback and TLCC checkpoints.
pub mod clearing_math; // Stage 24A diagnostic loss, funding and PnL equations; no ledger writes.
pub mod clearing_pnl; // Stage 25C read-only per-address PnL and residual projection.
pub mod clearing_sandbox; // Stage 25A isolated single-ledger derivative margin and funding research.
pub mod clearlist_source_owner; // Actual pinned-source rows and singleton/cache state.
pub mod client; // Bounded Core HTTP JSON-RPC; readiness is bound to network and genesis.
mod codec; // Bounded, checksummed snapshot codec.
pub mod consensus_source; // Replayed sequential source dispatch in the sole economic owner.
pub mod contract_orderbook; // Source matching metadata; custody stays in the economic owner.
pub mod contract_placement_source; // Literal source placement over the actual queue.
pub mod contract_queue_source_owner; // Actual source queue documents and placement capability.
pub mod contract_registry;
pub mod contract_settlement_source; // Literal source settlement over explicit live owners.
pub mod daemon; // Synced bounded raw-block observations; no economic admission authority.
pub mod decimal; // Finite decimal arithmetic with explicit precision and rounding.
pub mod dlc_oracle_bridge; // Source relay verification and separate trusted payload-bound evidence.
pub mod durable_iou; // Synced single-owner funded IOU operation batches and recovery.
pub mod durable_signed_channels; // Synced opt-in signed allocation owner and exact-head recovery.
pub mod economic; // Atomic caller-driven contract matching with the sole joint Ledger.
pub mod economic_chain; // Complete economic rollback and synced durable ownership.
pub mod economic_reporting; // Exact immutable execution reports from the verified owner.
pub mod economic_spot_prices; // Exact directed fill prices and VWAP from the original owner.
pub mod economic_state_oracle; // Canonical sole-owner export for an external proof-checking consumer.
pub mod economic_validity; // Actual immutable owner reads for source rules.
pub mod execution; // Prototype unified registry+ledger replay; input unverified.
pub mod funding; // Fractional source funding math and conserving payment plans.
pub mod funding_audit; // Stage 24B read-only signed funding and conservation projections.
mod hash160; // Dependency-free research RIPEMD160/HASH160, no production authorization.
mod hash256; // Offline SHA-256d txid fingerprints; not trusted consensus.
pub mod http_service;
pub mod iou; // Source-compatible standalone IOU bucket and claim calculations.
pub mod iou_ledger; // Funded atomic IOU payouts with an explicit local atom policy.
pub mod issuance_intent; // Bounded issuance request metadata; no token creation authority.
pub mod joint; // Stage 22 independent registry + sole spot-owned ledger in one mixed state.
pub mod joint_chain; // Stage 22 mixed unverified block replay and TLJC checkpoints.
pub mod ledger;
pub mod legacy_activation_audit; // Pinned activation.js read-only source semantics; genesis and authority denied.
pub mod legacy_base; // Bounded historical Base94/custom Base256 string compatibility.
pub mod legacy_channel_audit; // Pinned channels.js direct A/B commit projections, never signed-state authority.
pub mod legacy_clearlist_audit; // Pinned clearlist.js read-only NeDB/root source behavior; authorization denied.
pub mod legacy_txdecoder_audit; // Pinned txDecoder.js typed body observations, never consensus.
pub mod legacy_txencoder_audit; // Source-exact txEncoder.js strings, never spend-ready consensus.
pub mod legacy_txindex_audit; // Pinned txIndex.js marker/index source diagnostics; never authority.
pub mod legacy_types_audit; // Exact pinned types.js 36-route source dispatch; never trusted admission.
pub mod legacy_validity_audit; // Pinned validity.js source Type0/7/8/9 predicates, never admission authority.
pub mod liquidation_audit; // Stage 25E1 read-only maintenance, bankruptcy and insurance-deficit preflight.
pub mod liquidation_input_gate; // Stage 25G read-only current-block provenance and mandatory authorization gap.
pub mod liquidation_source_characterization; // Source-pinned read-only AMM priority and avg-vs-mark audit.
pub mod margin_map; // Local position accounting; authenticated collateral orchestration external.
pub mod margin_map_source_owner; // Literal source instance maps and position identity.
pub mod mark_basis; // Exact settled basis and subatom carry; no separate Ledger.
pub mod network_bound; // Stage 21 profile-bound replay and versioned checkpoint envelopes; unverified.
pub mod network_profile; // Independent Bitcoin/Litecoin/Dogecoin mainnet identities and proof capabilities.
pub mod options; // Source options ticker, PnL, pricing and portfolio calculations.
pub mod oracle;
pub mod oracle_source_owner; // Actual source documents, cache and publication watermarks.
pub mod parallel_txindex; // Bounded parallel read-only runtime TXID/OP_RETURN scan and ordered fetch; never consensus.
pub mod persistence; // Atomic persisted joint replay and rollback, no chain authentication.
pub mod procedural; // Source procedural metadata and context gates, no proof authority.
pub mod property_catalog; // Owned source-compatible property metadata and effect replay.
pub mod protocol; // Audited legacy transaction catalog, not consensus execution.
pub mod raw_block_ingest; // Stage8C borrowed raw Bitcoin block preindex, not consensus.
pub mod read_only; // Offline transaction scanning; cannot authorize ledger state.
pub mod registry; // Standalone tx0/7-9/13-15 registry state; authentication external.
pub mod reorg; // Source RPC tip observations; never authorization to roll back.
pub mod reporting; // Atomic network-scoped offline spot history and volume observations.
pub mod rewards; // In-memory atomic accounting primitives; not chain consensus.
pub mod root_reconciliation; // Read-only TLPT and zkTL metadata consistency.
pub mod runtime; // Concrete RPC/startup adapters with independent durable expectations.
pub mod scaling; // Bounded L2 settlement lifecycle and checked channel custody plans.
pub mod secp256k1; // Public ECDSA verification; no private-key or Bitcoin script API.
pub mod service;
pub mod service_client;
pub mod service_routes;
pub mod service_runtime;
pub mod service_view;
pub mod source_catalogs; // Actual property/clearlist and optional oracle metadata owners.
pub mod source_contract_replay; // Literal source contract owners; alternative to conserving settlement.
pub mod source_contract_runtime;
mod source_contract_values;
mod source_index_metadata_algorithms;
pub mod source_main_control; // Explicit terminal control around the existing source replay.
pub mod spot; // Stage 19 isolated type-5/6 rational matching experiment; unverified.
pub mod spot_chain; // Stage 20 typed unverified spot block replay and complete TLSP/TLSC checkpoints.
pub mod state_commitment; // Experimental domain-separated state and transition metadata; no proof verifier.
pub mod state_oracle; // Immutable Ledger balance payloads; hashes are not signed attestations.
pub mod storage; // Bounded OS-locked disk journal and explicit restart recovery.
pub mod tally; // Source tally projections and fee metadata, no independent spendable ledger.
pub mod token_settlement_source;
pub mod trade_history; // Caller-supplied trade reporting and canonical checkpoints; no settlement authority.
pub mod txindex_source_owner;
pub mod types_economic; // Unverified exact mapping into supported economic handlers.
pub mod types_source; // Full source decode-to-validity orchestration.
pub mod unified_chain; // Offline reversible registry+ledger block projection; adapter external.
pub mod validity_source; // Complete bounded source predicates over explicit dependencies.
pub mod vaults; // Standalone vault metadata and collateral lifecycle.
pub mod vesting; // Source initialization journals and explicit numerical repair.
pub mod volume_index; // Caller-supplied exact volume and VWAP reporting; no price authority.
pub mod wire; // Read-only legacy ASCII format analysis; never auto-fallback.
pub mod wire_v1;
pub mod withdrawal_parity; // Read-only JS-like deferred queue oracle for differential tests.
pub mod zk_batch; // zkTL/Cairo claim inspection; no proof verification or mutation. // Proposed versioned binary format.

/// Highest transaction type dispatched by the historical JavaScript
/// implementation when this scaffold was created.
pub const MAX_LEGACY_TX_TYPE: u8 = 35;

/// A transaction type known to the historical JavaScript dispatcher.
///
/// This is only a type-range check. It makes no claim about whether a
/// transaction is activated, authorized, well formed, or valid on-chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionType(u8);

impl TransactionType {
    /// Returns the numeric transaction type identifier.
    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

/// Transaction type outside the range supported by the legacy dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownTransactionType(pub u8);

impl core::fmt::Display for UnknownTransactionType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown legacy transaction type: {}", self.0)
    }
}

impl std::error::Error for UnknownTransactionType {}

impl TryFrom<u8> for TransactionType {
    type Error = UnknownTransactionType;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value <= MAX_LEGACY_TX_TYPE {
            Ok(Self(value))
        } else {
            Err(UnknownTransactionType(value))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_each_legacy_transaction_type() {
        for value in 0..=MAX_LEGACY_TX_TYPE {
            assert_eq!(TransactionType::try_from(value).unwrap().as_u8(), value);
        }
    }

    #[test]
    fn rejects_unknown_transaction_types() {
        assert_eq!(
            TransactionType::try_from(36),
            Err(UnknownTransactionType(36))
        );
        assert_eq!(
            TransactionType::try_from(u8::MAX),
            Err(UnknownTransactionType(u8::MAX))
        );
    }
}

pub mod blake3;
pub mod compact_candidate_snapshot;
pub mod litecoin_scrypt;
pub mod native_entitlement;

pub mod bitcoin_witness_binding;

pub mod bitcoin_p2wsh_verify;

pub mod bitcoin_block_input;
pub mod bitcoin_standard_input;

pub mod compact_journal;
#[cfg(test)]
mod compact_journal_tests;
pub mod source_compact_index_v2;
#[cfg(test)]
mod source_compact_index_v2_tests;

pub mod source_main_native_intent_store;
pub mod source_main_native_owners;

pub mod source_main_active_service_wal;
pub mod source_main_native_session;
pub mod source_main_step_kernel;

pub mod source_rich_value;

pub mod fees;
