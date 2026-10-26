// Included in economic.rs. The TLECv7 wrapper binds the independently expected
// source policy/bootstrap and full raw-candidate history to the old owner codec.
use crate::chain::{BlockHeader as SourceHeader, BlockId as SourceBlockId, Tip as SourceTip};
use crate::consensus_source::{
    self, SourceBlockFrame, SourceBlockMode, SourceBlockReport, SourceConsensusConfig,
    SourceConsensusPolicy, SourceConsensusState, SourceDisposition, SourceReceipt,
    SourceTransaction, MAX_SOURCE_FRAMES, MAX_SOURCE_HISTORY_BYTES, MAX_SOURCE_REASON_BYTES,
    MAX_SOURCE_RECORDS, MAX_SOURCE_TRANSACTION_BYTES,
};
use crate::validity_source::SourceValue;

const ECONOMIC_SOURCE_MAGIC: &[u8; 5] = b"TLEC\x07";

impl EconomicState {
    pub fn source_consensus_state(&self) -> &SourceConsensusState {
        &self.source_consensus
    }

    pub fn source_bootstrap_commitment(joint: &JointProtocolState) -> Result<[u8; 32]> {
        joint.verify_components_unverified()?;
        if joint.finalized_height().is_some() {
            return Err(EconomicError::InvalidConfig);
        }
        let bytes = joint
            .snapshot_for_economic_bytes()
            .map_err(|_| EconomicError::InvalidSnapshot)?;
        Ok(consensus_source::bootstrap_commitment(&bytes))
    }

    pub(crate) fn source_parent_unverified(
        &self,
    ) -> core::result::Result<SourceTip, consensus_source::ConsensusSourceError> {
        if self.config.source_consensus.is_none() {
            return Err(consensus_source::ConsensusSourceError::Disabled);
        }
        Ok(self.source_consensus.frames.last().map_or(
            SourceTip {
                height: 0,
                id: self.config.network.genesis(),
            },
            |f| SourceTip {
                height: f.header.height,
                id: f.header.id,
            },
        ))
    }

    pub(crate) fn preflight_source_frame(
        &self,
        header: SourceHeader,
        mode: SourceBlockMode,
        transactions: &[SourceTransaction],
    ) -> Result<()> {
        let mut out = EconomicWriter(Vec::new());
        let mut budget = SourceBudget::default();
        econ_write_source_state(&mut out, &self.source_consensus, &mut budget)?;
        if self.source_consensus.frames.len() >= MAX_SOURCE_FRAMES {
            return Err(EconomicError::Capacity);
        }
        budget.take(1)?;
        out.u64(header.height)?;
        out.put(&header.id.0)?;
        out.put(&header.parent.0)?;
        out.byte(source_mode_tag(mode))?;
        budget.take(transactions.len())?;
        out.count(transactions.len(), MAX_ECONOMIC_INPUTS)?;
        for tx in transactions {
            econ_write_source_transaction(&mut out, tx, &mut budget)?;
        }
        source_size(&out)
    }

    pub fn apply_source_block_unverified(
        &mut self,
        header: SourceHeader,
        mode: SourceBlockMode,
        transactions: &[SourceTransaction],
    ) -> core::result::Result<SourceBlockReport, consensus_source::ConsensusSourceError> {
        let (next, report) =
            consensus_source::execute_block_unverified(self, header, mode, transactions)?;
        *self = next;
        Ok(report)
    }

    fn verify_source_consensus(&self) -> Result<()> {
        let Some(config) = &self.config.source_consensus else {
            return if self.source_consensus == SourceConsensusState::default()
                && !self.replaying_source_history
            {
                Ok(())
            } else {
                Err(EconomicError::Invariant)
            };
        };
        let mut out = EconomicWriter(Vec::new());
        let initial_catalogs = self.source_catalog_bootstrap()?;
        econ_write_source_state(
            &mut out,
            &self.source_consensus,
            &mut SourceBudget::default(),
        )?;
        if consensus_source::bootstrap_commitment(&self.source_consensus.bootstrap)
            != config.bootstrap_commitment
            || self.source_consensus.frames.len() as u64 != self.finalized_height().unwrap_or(0)
        {
            return Err(EconomicError::Invariant);
        }
        let mut parent = SourceTip {
            height: 0,
            id: self.config.network.genesis(),
        };
        let mut blocks = BTreeSet::from([parent.id]);
        for frame in &self.source_consensus.frames {
            if frame.header.height != parent.height + 1
                || frame.header.parent != parent.id
                || !blocks.insert(frame.header.id)
                || frame.receipts.len() != frame.transactions.len()
            {
                return Err(EconomicError::Invariant);
            }
            consensus_source::preflight_transactions(&frame.transactions)
                .map_err(|_| EconomicError::InvalidSnapshot)?;
            parent = SourceTip {
                height: frame.header.height,
                id: frame.header.id,
            };
        }
        if self.replaying_source_history || self.replaying_native_history {
            return Ok(());
        }
        let bootstrap = self
            .decode_joint_bootstrap(&self.source_consensus.bootstrap)
            .map_err(|_| EconomicError::InvalidSnapshot)?;
        if Self::source_bootstrap_commitment(&bootstrap)? != config.bootstrap_commitment {
            return Err(EconomicError::Invariant);
        }
        // Construct the entire pristine owner, rather than comparing only its
        // Ledger/Registry and accidentally permitting invented economic books.
        // The legacy constructor also avoids recursive source verification.
        let mut base_config = self.config.clone();
        base_config.source_consensus = None;
        let mut replay = Self::from_joint_unverified(base_config, bootstrap)?;
        replay.config = self.config.clone();
        replay.source_consensus = SourceConsensusState {
            bootstrap: self.source_consensus.bootstrap.clone(),
            frames: Vec::new(),
            catalog_bootstrap: self.source_consensus.catalog_bootstrap.clone(),
            catalogs: initial_catalogs,
        };
        replay.replaying_source_history = true;
        for frame in &self.source_consensus.frames {
            let (next, _) = consensus_source::execute_block_unverified(
                &replay,
                frame.header,
                frame.mode,
                &frame.transactions,
            )
            .map_err(|_| EconomicError::InvalidSnapshot)?;
            replay = next;
            if replay.source_consensus.frames.last() != Some(frame) {
                return Err(EconomicError::Invariant);
            }
        }
        replay.replaying_source_history = false;
        if replay != *self {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }

    fn source_snapshot_bytes(&self) -> Result<Vec<u8>> {
        self.verify()?;
        let config = self
            .config
            .source_consensus
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let mut base = self.clone();
        base.config.source_consensus = None;
        base.source_consensus = SourceConsensusState::default();
        base.replaying_source_history = false;
        // Source replay has checked every original input; the embedded legacy
        // owner is still structurally verified and never changes its bytes.
        let bytes = base.snapshot_bytes()?;
        let mut out = EconomicWriter(ECONOMIC_SOURCE_MAGIC.to_vec());
        out.put(&config.bootstrap_commitment)?;
        match config.policy {
            SourceConsensusPolicy::ConservingSequentialV1 => out.byte(1)?,
        }
        out.embedded(&bytes)?;
        let mut source = EconomicWriter(Vec::new());
        econ_write_source_state(
            &mut source,
            &self.source_consensus,
            &mut SourceBudget::default(),
        )?;
        out.embedded(&source.0)?;
        with_crc(out.0).map_err(|_| EconomicError::Capacity)
    }

    fn from_source_snapshot_bytes(
        bytes: &[u8],
        expected: &EconomicConfig,
        admin: &AccountId,
        permitted: &BTreeSet<PropertyId>,
    ) -> Result<Self> {
        let expected_source = expected
            .source_consensus
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let raw = verify_crc(bytes).map_err(|_| econ_snapshot_bad())?;
        let mut reader = Cursor::new(raw);
        if reader.fixed::<5>().map_err(|_| econ_snapshot_bad())? != *ECONOMIC_SOURCE_MAGIC {
            return Err(econ_snapshot_bad());
        }
        let config = SourceConsensusConfig {
            catalog_bootstrap_commitment: None,
            bootstrap_commitment: reader.fixed().map_err(|_| econ_snapshot_bad())?,
            policy: match reader.u8().map_err(|_| econ_snapshot_bad())? {
                1 => SourceConsensusPolicy::ConservingSequentialV1,
                _ => return Err(econ_snapshot_bad()),
            },
        };
        if &config != expected_source {
            return Err(EconomicError::InvalidConfig);
        }
        let mut base_config = expected.clone();
        base_config.source_consensus = None;
        let mut state = Self::from_snapshot_bytes(
            econ_read_embedded(&mut reader)?,
            &base_config,
            admin,
            permitted,
        )?;
        let source_bytes = econ_read_embedded(&mut reader)?;
        if source_bytes.len() > MAX_SOURCE_HISTORY_BYTES {
            return Err(EconomicError::Capacity);
        }
        let mut source_reader = Cursor::new(source_bytes);
        state.source_consensus =
            econ_read_source_state(&mut source_reader, &mut SourceBudget::default())?;
        source_reader.end().map_err(|_| econ_snapshot_bad())?;
        state.config = expected.clone();
        state.replaying_source_history = false;
        reader.end().map_err(|_| econ_snapshot_bad())?;
        if state.snapshot_bytes()?.as_slice() != bytes {
            return Err(econ_snapshot_bad());
        }
        Ok(state)
    }
}

#[derive(Default)]
struct SourceBudget {
    records: usize,
}
impl SourceBudget {
    fn take(&mut self, count: usize) -> Result<()> {
        self.records = self
            .records
            .checked_add(count)
            .ok_or(EconomicError::Capacity)?;
        if self.records > MAX_SOURCE_RECORDS {
            return Err(EconomicError::Capacity);
        }
        Ok(())
    }
}
fn source_size(out: &EconomicWriter) -> Result<()> {
    if out.0.len() > MAX_SOURCE_HISTORY_BYTES {
        Err(EconomicError::Capacity)
    } else {
        Ok(())
    }
}
fn source_text(out: &mut EconomicWriter, text: &str, limit: usize) -> Result<()> {
    out.count(text.len(), limit)?;
    out.put(text.as_bytes())
}
fn source_read_text(reader: &mut Cursor<'_>, limit: usize) -> Result<String> {
    let count = econ_read_count(reader, limit)?;
    let bytes = reader.take(count).map_err(|_| econ_snapshot_bad())?;
    core::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| econ_snapshot_bad())
}
fn source_value(
    out: &mut EconomicWriter,
    value: &SourceValue,
    budget: &mut SourceBudget,
) -> Result<()> {
    budget.take(1)?;
    match value {
        SourceValue::Undefined => out.byte(0),
        SourceValue::Null => out.byte(1),
        SourceValue::Bool(false) => out.byte(2),
        SourceValue::Bool(true) => out.byte(3),
        SourceValue::Number(bits) if f64::from_bits(*bits).is_finite() => {
            out.byte(4)?;
            out.u64(*bits)
        }
        SourceValue::String(text) => {
            out.byte(5)?;
            source_text(out, text, MAX_SOURCE_TRANSACTION_BYTES)
        }
        SourceValue::Array(values) => {
            out.byte(6)?;
            out.count(values.len(), crate::validity_source::MAX_NODES)?;
            for value in values {
                source_value(out, value, budget)?;
            }
            Ok(())
        }
        SourceValue::Object(fields) => {
            out.byte(7)?;
            out.count(fields.len(), crate::validity_source::MAX_NODES)?;
            for (key, value) in fields {
                source_text(out, key, MAX_SOURCE_TRANSACTION_BYTES)?;
                source_value(out, value, budget)?;
            }
            Ok(())
        }
        _ => Err(EconomicError::InvalidInput),
    }
}
fn source_read_value(reader: &mut Cursor<'_>, budget: &mut SourceBudget) -> Result<SourceValue> {
    source_read_value_depth(reader, budget, 0)
}
fn source_read_value_depth(
    reader: &mut Cursor<'_>,
    budget: &mut SourceBudget,
    depth: usize,
) -> Result<SourceValue> {
    if depth > crate::validity_source::MAX_DEPTH {
        return Err(EconomicError::Capacity);
    }
    budget.take(1)?;
    Ok(match reader.u8().map_err(|_| econ_snapshot_bad())? {
        0 => SourceValue::Undefined,
        1 => SourceValue::Null,
        2 => SourceValue::Bool(false),
        3 => SourceValue::Bool(true),
        4 => {
            let bits = econ_read_u64(reader)?;
            if !f64::from_bits(bits).is_finite() {
                return Err(econ_snapshot_bad());
            }
            SourceValue::Number(bits)
        }
        5 => SourceValue::String(source_read_text(reader, MAX_SOURCE_TRANSACTION_BYTES)?),
        6 => {
            let count = econ_read_count(reader, crate::validity_source::MAX_NODES)?;
            if count > MAX_SOURCE_RECORDS - budget.records {
                return Err(EconomicError::Capacity);
            }
            let mut values = Vec::new();
            for _ in 0..count {
                values.push(source_read_value_depth(reader, budget, depth + 1)?);
            }
            SourceValue::Array(values)
        }
        7 => {
            let count = econ_read_count(reader, crate::validity_source::MAX_NODES)?;
            if count > MAX_SOURCE_RECORDS - budget.records {
                return Err(EconomicError::Capacity);
            }
            let mut fields = Vec::new();
            for _ in 0..count {
                fields.push((
                    source_read_text(reader, MAX_SOURCE_TRANSACTION_BYTES)?,
                    source_read_value_depth(reader, budget, depth + 1)?,
                ));
            }
            SourceValue::Object(fields)
        }
        _ => return Err(econ_snapshot_bad()),
    })
}
fn source_mode_tag(mode: SourceBlockMode) -> u8 {
    match mode {
        SourceBlockMode::IndexedReplay => 0,
        SourceBlockMode::Realtime => 1,
    }
}
fn source_disposition_tag(disposition: SourceDisposition) -> u8 {
    match disposition {
        SourceDisposition::Applied => 0,
        SourceDisposition::Invalid => 1,
        SourceDisposition::ReturnedError => 2,
        SourceDisposition::Thrown => 3,
        SourceDisposition::UnsupportedMapping => 4,
        SourceDisposition::EconomicRejected => 5,
        SourceDisposition::AlreadyProcessed => 6,
        SourceDisposition::DependencyUnavailable => 7,
    }
}
fn econ_write_source_transaction(
    out: &mut EconomicWriter,
    tx: &SourceTransaction,
    budget: &mut SourceBudget,
) -> Result<()> {
    let mut inner = EconomicWriter(Vec::new());
    econ_write_source_transaction_inner(&mut inner, tx, budget)?;
    if inner.0.len() > MAX_SOURCE_TRANSACTION_BYTES {
        return Err(EconomicError::Capacity);
    }
    out.embedded(&inner.0)
}
fn econ_write_source_transaction_inner(
    out: &mut EconomicWriter,
    tx: &SourceTransaction,
    budget: &mut SourceBudget,
) -> Result<()> {
    out.u32(tx.position)?;
    out.txid(tx.txid)?;
    source_text(out, &tx.payload, MAX_SOURCE_TRANSACTION_BYTES)?;
    for value in [
        &tx.marker,
        &tx.sender,
        &tx.reference,
        &tx.sender_amount,
        &tx.reference_amount,
    ] {
        source_value(out, value, budget)?;
    }
    Ok(())
}
fn econ_write_source_state(
    out: &mut EconomicWriter,
    state: &SourceConsensusState,
    budget: &mut SourceBudget,
) -> Result<()> {
    let start = out.0.len();
    out.embedded(&state.bootstrap)?;
    budget.take(state.frames.len())?;
    out.count(state.frames.len(), MAX_SOURCE_FRAMES)?;
    for frame in &state.frames {
        out.u64(frame.header.height)?;
        out.put(&frame.header.id.0)?;
        out.put(&frame.header.parent.0)?;
        out.byte(source_mode_tag(frame.mode))?;
        budget.take(frame.transactions.len())?;
        out.count(frame.transactions.len(), MAX_ECONOMIC_INPUTS)?;
        for tx in &frame.transactions {
            econ_write_source_transaction(out, tx, budget)?;
        }
        budget.take(frame.receipts.len())?;
        out.count(frame.receipts.len(), MAX_ECONOMIC_INPUTS)?;
        for receipt in &frame.receipts {
            out.u32(receipt.position)?;
            out.txid(receipt.txid)?;
            out.byte(source_disposition_tag(receipt.disposition))?;
            out.put(&receipt.evaluation_commitment)?;
            source_text(out, &receipt.reason, MAX_SOURCE_REASON_BYTES)?;
        }
    }
    if out.0.len() - start > MAX_SOURCE_HISTORY_BYTES {
        return Err(EconomicError::Capacity);
    }
    Ok(())
}
fn econ_read_source_state(
    reader: &mut Cursor<'_>,
    budget: &mut SourceBudget,
) -> Result<SourceConsensusState> {
    let bootstrap_bytes = econ_read_embedded(reader)?;
    if bootstrap_bytes.len() > MAX_SOURCE_HISTORY_BYTES {
        return Err(EconomicError::Capacity);
    }
    let bootstrap = bootstrap_bytes.to_vec();
    let count = econ_read_count(reader, MAX_SOURCE_FRAMES)?;
    budget.take(count)?;
    let mut frames = Vec::new();
    for _ in 0..count {
        let header = SourceHeader {
            height: econ_read_u64(reader)?,
            id: SourceBlockId(reader.fixed().map_err(|_| econ_snapshot_bad())?),
            parent: SourceBlockId(reader.fixed().map_err(|_| econ_snapshot_bad())?),
        };
        let mode = match reader.u8().map_err(|_| econ_snapshot_bad())? {
            0 => SourceBlockMode::IndexedReplay,
            1 => SourceBlockMode::Realtime,
            _ => return Err(econ_snapshot_bad()),
        };
        let tx_count = econ_read_count(reader, MAX_ECONOMIC_INPUTS)?;
        budget.take(tx_count)?;
        let mut transactions = Vec::new();
        for _ in 0..tx_count {
            let bytes = econ_read_embedded(reader)?;
            if bytes.len() > MAX_SOURCE_TRANSACTION_BYTES {
                return Err(EconomicError::Capacity);
            }
            let mut transaction_reader = Cursor::new(bytes);
            let reader = &mut transaction_reader;
            transactions.push(SourceTransaction {
                position: reader.u32().map_err(|_| econ_snapshot_bad())?,
                txid: econ_read_txid(reader)?,
                payload: source_read_text(reader, MAX_SOURCE_TRANSACTION_BYTES)?,
                marker: source_read_value(reader, budget)?,
                sender: source_read_value(reader, budget)?,
                reference: source_read_value(reader, budget)?,
                sender_amount: source_read_value(reader, budget)?,
                reference_amount: source_read_value(reader, budget)?,
            });
            transaction_reader.end().map_err(|_| econ_snapshot_bad())?;
        }
        let receipt_count = econ_read_count(reader, MAX_ECONOMIC_INPUTS)?;
        budget.take(receipt_count)?;
        let mut receipts = Vec::new();
        for _ in 0..receipt_count {
            let position = reader.u32().map_err(|_| econ_snapshot_bad())?;
            let txid = econ_read_txid(reader)?;
            let disposition = match reader.u8().map_err(|_| econ_snapshot_bad())? {
                0 => SourceDisposition::Applied,
                1 => SourceDisposition::Invalid,
                2 => SourceDisposition::ReturnedError,
                3 => SourceDisposition::Thrown,
                4 => SourceDisposition::UnsupportedMapping,
                5 => SourceDisposition::EconomicRejected,
                6 => SourceDisposition::AlreadyProcessed,
                7 => SourceDisposition::DependencyUnavailable,
                _ => return Err(econ_snapshot_bad()),
            };
            receipts.push(SourceReceipt {
                position,
                txid,
                disposition,
                evaluation_commitment: reader.fixed().map_err(|_| econ_snapshot_bad())?,
                reason: source_read_text(reader, MAX_SOURCE_REASON_BYTES)?,
            });
        }
        frames.push(SourceBlockFrame {
            header,
            mode,
            transactions,
            receipts,
        });
    }
    Ok(SourceConsensusState {
        bootstrap,
        frames,
        catalog_bootstrap: Vec::new(),
        catalogs: None,
    })
}

#[cfg(test)]
mod source_consensus_poison_tests {
    use super::*;
    use crate::economic_chain::{DurableEconomicChain, EconomicChain, EconomicChainConfig};
    use crate::ledger::{Operation, PropertyKind};
    use crate::network_profile::TradeLayerMainnet;
    use crate::storage::Recovery;

    #[test]
    fn uncertain_source_commit_denies_cached_state_and_reopen_requires_new_head() {
        let root = std::env::temp_dir().join(format!("tl-source-poison-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let property: PropertyId = 7.try_into().unwrap();
        let admin: AccountId = "admin".try_into().unwrap();
        let mut ledger = Ledger::new();
        ledger
            .apply(
                TxId::from_bytes([9; 32]),
                Operation::CreateProperty {
                    id: property,
                    issuer: admin.clone(),
                    kind: PropertyKind::Fixed,
                    initial_atoms: 10,
                },
            )
            .unwrap();
        let joint = JointProtocolState::from_bootstrap_unverified(
            TradeLayerMainnet::Bitcoin,
            admin.clone(),
            ledger.clone(),
            BTreeSet::from([property]),
        )
        .unwrap();
        let source = SourceConsensusConfig {
            catalog_bootstrap_commitment: None,
            bootstrap_commitment: EconomicState::source_bootstrap_commitment(&joint).unwrap(),
            policy: SourceConsensusPolicy::ConservingSequentialV1,
        };
        let config = EconomicChainConfig {
            economics: EconomicConfig {
                native_entitlements: None,
                insurance: None,
                network: TradeLayerMainnet::Bitcoin,
                contract: 1.try_into().unwrap(),
                collateral: property,
                inverse: false,
                notional: 1.into(),
                initial_margin_per_contract: 1.into(),
                fee_route: FeeRoute::Native(1),
                policy: SettlementPolicy::ConservingV1,
                funding: None,
                marking: None,
                liquidation: None,
                amm: None,
                contract_binding: None,
                source_consensus: Some(source),
            },
            admin,
            permitted_spot_properties: BTreeSet::from([property]),
            rollback_limit: 2,
        };
        let chain = EconomicChain::new_unverified(config.clone(), ledger).unwrap();
        let tip = chain.tip();
        let mut owner = DurableEconomicChain::initialize(&root, chain, tip).unwrap();
        let before = owner.head().unwrap();
        let block = crate::consensus_source::UnverifiedSourceBlock {
            network: TradeLayerMainnet::Bitcoin,
            header: SourceHeader {
                height: 1,
                id: SourceBlockId([5; 32]),
                parent: tip.id,
            },
            mode: SourceBlockMode::IndexedReplay,
            transactions: Vec::new(),
        };
        owner.test_fail_next_commit_after_sync();
        assert!(owner.apply_source_unverified_block(before, &block).is_err());
        assert!(owner.head().is_err());
        assert!(owner.chain().is_err());
        drop(owner);
        assert!(
            DurableEconomicChain::load(&root, config, before, Recovery::RejectIncompleteTail)
                .is_err()
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
