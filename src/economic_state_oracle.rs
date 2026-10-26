//! Producer-side canonical economic state data for an external consumer.
//! Self-integrity and local replay checks do not authenticate chain admission.
use crate::chain::{BlockId, Tip};
use crate::economic::EconomicError;
use crate::economic_chain::{
    DurableEconomicChain, DurableEconomicError, EconomicChain, EconomicChainConfig, EconomicHead,
};
use crate::hash256::double_sha256_parts;
use crate::network_profile::TradeLayerMainnet;
use core::ops::Range;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub const SCHEMA: &str = "tl-economic-state-oracle-v1";
pub const MAX_CHECKPOINT_BYTES: usize = crate::codec::MAX_CHECKPOINT_BYTES;
pub const HEADER_BYTES: usize = 231;
pub const MAX_EXPORT_BYTES: usize = MAX_CHECKPOINT_BYTES + HEADER_BYTES;
const MAGIC: &[u8; 5] = b"TLSE\x01";
const STATE_DOMAIN: &[u8] = b"TradeLayer/EconomicStateOracle/State/v1/";
const EXPORT_DOMAIN: &[u8] = b"TradeLayer/EconomicStateOracle/Export/v1/";
const CHECKPOINT_DOMAIN: &[u8] = b"TradeLayer/DurableEconomicChain/v1/";
const TLEH_STATE_OFFSET: usize = 123;

#[derive(Debug)]
pub enum EconomicOracleError {
    Economic(EconomicError),
    Durable(DurableEconomicError),
    Capacity,
    InvalidEnvelope(&'static str),
    WrongNetwork,
    WrongHead,
    WrongState,
    WrongConfig,
    InvalidPath,
    AlreadyExists,
    Io {
        operation: &'static str,
        kind: std::io::ErrorKind,
    },
}
impl core::fmt::Display for EconomicOracleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io { operation, kind } => write!(f, "{operation}: {kind}"),
            other => write!(f, "{other:?}"),
        }
    }
}
impl std::error::Error for EconomicOracleError {}
impl From<EconomicError> for EconomicOracleError {
    fn from(error: EconomicError) -> Self {
        Self::Economic(error)
    }
}
impl From<DurableEconomicError> for EconomicOracleError {
    fn from(error: DurableEconomicError) -> Self {
        Self::Durable(error)
    }
}
type Result<T> = core::result::Result<T, EconomicOracleError>;
fn bad(reason: &'static str) -> EconomicOracleError {
    EconomicOracleError::InvalidEnvelope(reason)
}

/// Values must be supplied independently of the untrusted export. A durable
/// head commits the complete TLEH checkpoint; an optional state pin adds a
/// separate assertion about the canonical TLEC bytes alone.
pub struct ExportExpectation<'a> {
    pub config: &'a EconomicChainConfig,
    pub head: EconomicHead,
    pub state_commitment: Option<[u8; 32]>,
}

/// One retained binary checkpoint, not another retained economic owner. The
/// canonical TLEC payload is a borrowed slice inside its TLEH witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EconomicStateOracleExport {
    network: TradeLayerMainnet,
    head: EconomicHead,
    floor: Tip,
    state_version: u8,
    state_range: Range<usize>,
    state_commitment: [u8; 32],
    export_commitment: [u8; 32],
    checkpoint: Vec<u8>,
}
impl EconomicStateOracleExport {
    /// Reads the original locked owner without applying transactions, changing
    /// its journal, or cloning a Ledger. Existing invariant checks may perform
    /// bounded temporary replay validation internally.
    pub fn from_durable(
        owner: &DurableEconomicChain,
        expected: &ExportExpectation<'_>,
    ) -> Result<Self> {
        expected.config.validate()?;
        if owner.head()? != expected.head {
            return Err(EconomicOracleError::WrongHead);
        }
        let chain = owner.chain()?;
        if chain.config() != expected.config {
            return Err(EconomicOracleError::WrongConfig);
        }
        let checkpoint = chain.checkpoint_bytes()?;
        if checkpoint.len() > MAX_CHECKPOINT_BYTES {
            return Err(EconomicOracleError::Capacity);
        }
        if checkpoint_commitment(&checkpoint) != expected.head.commitment {
            return Err(EconomicOracleError::WrongHead);
        }
        let (range, state_version) = state_range(
            &checkpoint,
            expected.config,
            expected.head.tip,
            chain.rollback_floor(),
        )?;
        let state_commitment = calculate_state_commitment(&checkpoint[range.clone()])?;
        if expected
            .state_commitment
            .is_some_and(|want| want != state_commitment)
        {
            return Err(EconomicOracleError::WrongState);
        }
        // The immutable borrow excludes sequence/state changes. Recheck journal
        // health without redundantly hashing the unchanged full checkpoint.
        if owner.chain()?.tip() != expected.head.tip {
            return Err(EconomicOracleError::WrongHead);
        }
        let mut export = Self {
            network: expected.config.economics.network,
            head: expected.head,
            floor: chain.rollback_floor(),
            state_version,
            state_range: range,
            state_commitment,
            export_commitment: [0; 32],
            checkpoint,
        };
        export.export_commitment = export.calculated_export_commitment();
        Ok(export)
    }

    /// Validates byte integrity, independent identities/head/state pin, full
    /// TLEH/TLEC canonical decoding and the owner's complete available replay
    /// policy. A self-consistent attacker export cannot replace expected head.
    pub fn decode(bytes: &[u8], expected: &ExportExpectation<'_>) -> Result<Self> {
        if bytes.len() > MAX_EXPORT_BYTES {
            return Err(EconomicOracleError::Capacity);
        }
        if bytes.len() < HEADER_BYTES {
            return Err(bad("truncated header"));
        }
        expected.config.validate()?;
        let mut reader = crate::codec::Cursor::new(&bytes[..HEADER_BYTES]);
        if reader.fixed::<5>().map_err(|_| bad("magic"))? != *MAGIC {
            return Err(bad("magic/version"));
        }
        let network = TradeLayerMainnet::from_tag(reader.u8().map_err(|_| bad("network"))?)
            .map_err(|_| EconomicOracleError::WrongNetwork)?;
        let genesis = reader.fixed::<32>().map_err(|_| bad("genesis"))?;
        if network != expected.config.economics.network || genesis != network.genesis().0 {
            return Err(EconomicOracleError::WrongNetwork);
        }
        let sequence = reader.u64().map_err(|_| bad("sequence"))?;
        let tip = read_tip(&mut reader)?;
        let floor = read_tip(&mut reader)?;
        let state_version = reader.u8().map_err(|_| bad("state version"))?;
        let state_length = reader.u32().map_err(|_| bad("state length"))? as usize;
        let checkpoint_length = reader.u32().map_err(|_| bad("checkpoint length"))? as usize;
        let state_commitment = reader.fixed::<32>().map_err(|_| bad("state commitment"))?;
        let checkpoint_hash = reader
            .fixed::<32>()
            .map_err(|_| bad("checkpoint commitment"))?;
        let export_commitment = reader.fixed::<32>().map_err(|_| bad("export commitment"))?;
        reader.end().map_err(|_| bad("header trailing data"))?;
        if checkpoint_length > MAX_CHECKPOINT_BYTES || state_length > MAX_CHECKPOINT_BYTES {
            return Err(EconomicOracleError::Capacity);
        }
        let length = HEADER_BYTES
            .checked_add(checkpoint_length)
            .ok_or(EconomicOracleError::Capacity)?;
        if bytes.len() != length {
            return Err(bad("checkpoint framing"));
        }
        let head = EconomicHead {
            sequence,
            tip,
            commitment: checkpoint_hash,
        };
        if head != expected.head {
            return Err(EconomicOracleError::WrongHead);
        }
        let checkpoint = &bytes[HEADER_BYTES..];
        if checkpoint_commitment(checkpoint) != checkpoint_hash {
            return Err(EconomicOracleError::WrongHead);
        }
        let (range, actual_version) = state_range(checkpoint, expected.config, tip, floor)?;
        if state_length != range.len() || state_version != actual_version {
            return Err(bad("state metadata"));
        }
        if calculate_state_commitment(&checkpoint[range.clone()])? != state_commitment {
            return Err(EconomicOracleError::WrongState);
        }
        if expected
            .state_commitment
            .is_some_and(|want| want != state_commitment)
        {
            return Err(EconomicOracleError::WrongState);
        }
        if double_sha256_parts(&[EXPORT_DOMAIN, &bytes[..HEADER_BYTES - 32], checkpoint])
            != export_commitment
        {
            return Err(bad("export commitment"));
        }
        // This reconstructs only temporary validation state; the export retains
        // bytes, never a second mutable economic/Ledger owner.
        let chain = EconomicChain::from_checkpoint_bytes(
            checkpoint,
            expected.config.clone(),
            expected.head.tip,
        )?;
        if chain.rollback_floor() != floor {
            return Err(bad("rollback floor"));
        }
        Ok(Self {
            network,
            head,
            floor,
            state_version,
            state_range: range,
            state_commitment,
            export_commitment,
            checkpoint: checkpoint.to_vec(),
        })
    }

    pub fn network(&self) -> TradeLayerMainnet {
        self.network
    }
    pub fn genesis(&self) -> BlockId {
        self.network.genesis()
    }
    pub fn head(&self) -> EconomicHead {
        self.head
    }
    pub fn rollback_floor(&self) -> Tip {
        self.floor
    }
    pub fn state_codec_version(&self) -> u8 {
        self.state_version
    }
    pub fn state_bytes(&self) -> &[u8] {
        &self.checkpoint[self.state_range.clone()]
    }
    pub fn checkpoint_bytes(&self) -> &[u8] {
        &self.checkpoint
    }
    pub fn state_commitment(&self) -> [u8; 32] {
        self.state_commitment
    }
    pub fn checkpoint_commitment(&self) -> [u8; 32] {
        self.head.commitment
    }
    pub fn export_commitment(&self) -> [u8; 32] {
        self.export_commitment
    }
    pub fn encoded_len(&self) -> usize {
        HEADER_BYTES + self.checkpoint.len()
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        if self.checkpoint.len() > MAX_CHECKPOINT_BYTES || self.encoded_len() > MAX_EXPORT_BYTES {
            return Err(EconomicOracleError::Capacity);
        }
        let mut output = Vec::with_capacity(self.encoded_len());
        output.extend_from_slice(&self.header());
        output.extend_from_slice(&self.checkpoint);
        Ok(output)
    }
    /// Fixed schema and order; large state is not expanded to JSON/Base64.
    /// Heights/sequence are strings so consumers never round u64 through f64.
    pub fn manifest_json(&self) -> String {
        let network = match self.network {
            TradeLayerMainnet::Bitcoin => "bitcoin",
            TradeLayerMainnet::Litecoin => "litecoin",
            TradeLayerMainnet::Dogecoin => "dogecoin",
        };
        format!("{{\"schema\":\"{SCHEMA}\",\"scope\":\"single-economic-owner\",\"authenticatedAdmission\":false,\"network\":\"{network}\",\"genesis\":\"{}\",\"tip\":{{\"height\":\"{}\",\"blockId\":\"{}\"}},\"rollbackFloor\":{{\"height\":\"{}\",\"blockId\":\"{}\"}},\"durableSequence\":\"{}\",\"stateCodec\":\"TLEC\",\"stateCodecVersion\":{},\"stateBytes\":{},\"checkpointCodec\":\"TLEH\",\"checkpointCodecVersion\":1,\"checkpointBytes\":{},\"commitmentScheme\":\"sha256d-domain-separated-v1\",\"stateCommitment\":\"{}\",\"checkpointCommitment\":\"{}\",\"exportCommitment\":\"{}\",\"transitionWitness\":\"not-exported\"}}",hex(&self.genesis().0),self.head.tip.height,hex(&self.head.tip.id.0),self.floor.height,hex(&self.floor.id.0),self.head.sequence,self.state_version,self.state_range.len(),self.checkpoint.len(),hex(&self.state_commitment),hex(&self.head.commitment),hex(&self.export_commitment))
    }

    /// Creates only a new explicitly named absolute file in an existing
    /// directory. Errors during write/sync can leave a partial new file; the
    /// path is never overwritten or silently removed. Directory-entry sync is
    /// performed on Unix; Windows only promises the file sync available here.
    pub fn write_new(&self, path: &Path) -> Result<()> {
        if self.encoded_len() > MAX_EXPORT_BYTES {
            return Err(EconomicOracleError::Capacity);
        }
        check_path(path)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    EconomicOracleError::AlreadyExists
                } else {
                    io("create export", error)
                }
            })?;
        file.write_all(&self.header())
            .and_then(|_| file.write_all(&self.checkpoint))
            .map_err(|e| io("write export (partial file may remain)", e))?;
        file.sync_all()
            .map_err(|e| io("sync export (new file remains)", e))?;
        #[cfg(unix)]
        File::open(path.parent().ok_or(EconomicOracleError::InvalidPath)?)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| io("sync export directory (new file remains)", e))?;
        Ok(())
    }
    pub fn read_file(path: &Path, expected: &ExportExpectation<'_>) -> Result<Self> {
        expected.config.validate()?;
        check_path(path)?;
        let mut file = File::open(path).map_err(|e| io("open export", e))?;
        let metadata = file.metadata().map_err(|e| io("inspect export", e))?;
        if !metadata.is_file() {
            return Err(EconomicOracleError::InvalidPath);
        }
        let length = usize::try_from(metadata.len()).map_err(|_| EconomicOracleError::Capacity)?;
        if length > MAX_EXPORT_BYTES {
            return Err(EconomicOracleError::Capacity);
        }
        if length < HEADER_BYTES {
            return Err(bad("truncated file"));
        }
        let mut header = [0; HEADER_BYTES];
        file.read_exact(&mut header)
            .map_err(|e| io("read export header", e))?;
        preflight_file_header(&header, length, expected)?;
        // Even a file with a permitted metadata size cannot cause the large
        // allocation before its fixed header, identities and length claims pass.
        let mut bytes = vec![0; length];
        bytes[..HEADER_BYTES].copy_from_slice(&header);
        file.read_exact(&mut bytes[HEADER_BYTES..])
            .map_err(|e| io("read export", e))?;
        let mut extra = [0];
        if file
            .read(&mut extra)
            .map_err(|e| io("check export length", e))?
            != 0
        {
            return Err(bad("file changed or trailing bytes"));
        }
        Self::decode(&bytes, expected)
    }
    fn header(&self) -> [u8; HEADER_BYTES] {
        let mut bytes = [0; HEADER_BYTES];
        let mut at = 0;
        let mut put = |value: &[u8]| {
            bytes[at..at + value.len()].copy_from_slice(value);
            at += value.len();
        };
        put(MAGIC);
        put(&[self.network.tag()]);
        put(&self.genesis().0);
        put(&self.head.sequence.to_be_bytes());
        put(&self.head.tip.height.to_be_bytes());
        put(&self.head.tip.id.0);
        put(&self.floor.height.to_be_bytes());
        put(&self.floor.id.0);
        put(&[self.state_version]);
        put(&(self.state_range.len() as u32).to_be_bytes());
        put(&(self.checkpoint.len() as u32).to_be_bytes());
        put(&self.state_commitment);
        put(&self.head.commitment);
        put(&self.export_commitment);
        debug_assert_eq!(at, HEADER_BYTES);
        bytes
    }
    fn calculated_export_commitment(&self) -> [u8; 32] {
        let header = self.header();
        double_sha256_parts(&[
            EXPORT_DOMAIN,
            &header[..HEADER_BYTES - 32],
            &self.checkpoint,
        ])
    }
}

fn preflight_file_header(
    bytes: &[u8; HEADER_BYTES],
    actual_length: usize,
    expected: &ExportExpectation<'_>,
) -> Result<()> {
    let mut reader = crate::codec::Cursor::new(bytes);
    if reader.fixed::<5>().map_err(|_| bad("magic"))? != *MAGIC {
        return Err(bad("magic/version"));
    }
    let network = reader.u8().map_err(|_| bad("network"))?;
    let genesis = reader.fixed::<32>().map_err(|_| bad("genesis"))?;
    if network != expected.config.economics.network.tag()
        || genesis != expected.config.economics.network.genesis().0
    {
        return Err(EconomicOracleError::WrongNetwork);
    }
    let sequence = reader.u64().map_err(|_| bad("sequence"))?;
    let tip = read_tip(&mut reader)?;
    let _floor = read_tip(&mut reader)?;
    let version = reader.u8().map_err(|_| bad("state version"))?;
    if !(1..=7).contains(&version) {
        return Err(bad("state codec/version"));
    }
    let state_length = reader.u32().map_err(|_| bad("state length"))? as usize;
    let checkpoint_length = reader.u32().map_err(|_| bad("checkpoint length"))? as usize;
    if state_length > MAX_CHECKPOINT_BYTES || checkpoint_length > MAX_CHECKPOINT_BYTES {
        return Err(EconomicOracleError::Capacity);
    }
    if actual_length != HEADER_BYTES + checkpoint_length || state_length > checkpoint_length {
        return Err(bad("checkpoint framing"));
    }
    let state = reader.fixed::<32>().map_err(|_| bad("state commitment"))?;
    let checkpoint = reader
        .fixed::<32>()
        .map_err(|_| bad("checkpoint commitment"))?;
    if (EconomicHead {
        sequence,
        tip,
        commitment: checkpoint,
    }) != expected.head
    {
        return Err(EconomicOracleError::WrongHead);
    }
    if expected.state_commitment.is_some_and(|want| want != state) {
        return Err(EconomicOracleError::WrongState);
    }
    Ok(())
}

fn calculate_state_commitment(bytes: &[u8]) -> Result<[u8; 32]> {
    if bytes.len() > MAX_CHECKPOINT_BYTES {
        return Err(EconomicOracleError::Capacity);
    }
    let length = u32::try_from(bytes.len())
        .map_err(|_| EconomicOracleError::Capacity)?
        .to_be_bytes();
    Ok(double_sha256_parts(&[STATE_DOMAIN, &length, bytes]))
}
fn checkpoint_commitment(bytes: &[u8]) -> [u8; 32] {
    double_sha256_parts(&[CHECKPOINT_DOMAIN, bytes])
}
fn state_range(
    checkpoint: &[u8],
    config: &EconomicChainConfig,
    tip: Tip,
    floor: Tip,
) -> Result<(Range<usize>, u8)> {
    if checkpoint.len() > MAX_CHECKPOINT_BYTES {
        return Err(EconomicOracleError::Capacity);
    }
    let raw = crate::codec::verify_crc(checkpoint).map_err(|_| bad("checkpoint CRC/size"))?;
    let mut reader = crate::codec::Cursor::new(raw);
    if reader.fixed::<5>().map_err(|_| bad("checkpoint version"))? != *b"TLEH\x01" {
        return Err(bad("checkpoint version"));
    }
    if reader.u8().map_err(|_| bad("checkpoint network"))? != config.economics.network.tag()
        || reader
            .fixed::<32>()
            .map_err(|_| bad("checkpoint genesis"))?
            != config.economics.network.genesis().0
    {
        return Err(EconomicOracleError::WrongNetwork);
    }
    if reader.u8().map_err(|_| bad("rollback limit"))? as usize != config.rollback_limit {
        return Err(EconomicOracleError::WrongConfig);
    }
    if read_tip(&mut reader)? != floor || read_tip(&mut reader)? != tip {
        return Err(bad("checkpoint tips"));
    }
    let length = reader.u32().map_err(|_| bad("embedded state length"))? as usize;
    if length > MAX_CHECKPOINT_BYTES {
        return Err(EconomicOracleError::Capacity);
    }
    let state = reader
        .take(length)
        .map_err(|_| bad("embedded state framing"))?;
    if state.len() < 5 || &state[..4] != b"TLEC" || !(1..=7).contains(&state[4]) {
        return Err(bad("state codec/version"));
    }
    let end = TLEH_STATE_OFFSET
        .checked_add(length)
        .ok_or(EconomicOracleError::Capacity)?;
    if checkpoint.get(TLEH_STATE_OFFSET..end) != Some(state) {
        return Err(bad("TLEH v1 state offset"));
    }
    Ok((TLEH_STATE_OFFSET..end, state[4]))
}
fn read_tip(reader: &mut crate::codec::Cursor<'_>) -> Result<Tip> {
    Ok(Tip {
        height: reader.u64().map_err(|_| bad("tip height"))?,
        id: BlockId(reader.fixed::<32>().map_err(|_| bad("tip block"))?),
    })
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn io(operation: &'static str, error: std::io::Error) -> EconomicOracleError {
    EconomicOracleError::Io {
        operation,
        kind: error.kind(),
    }
}
fn check_path(path: &Path) -> Result<()> {
    if !path.is_absolute() || path.file_name().is_none() || !path.parent().is_some_and(Path::is_dir)
    {
        return Err(EconomicOracleError::InvalidPath);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::BlockHeader;
    use crate::contract_registry::ContractId;
    use crate::decimal::Decimal;
    use crate::economic::SettlementPolicy;
    use crate::economic_chain::UnverifiedEconomicBlock;
    use crate::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
    use crate::storage::StorageError;
    use crate::tally::FeeRoute;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            assert!(self.0.is_absolute() && self.0.starts_with(std::env::temp_dir()));
            assert!(self
                .0
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("tl-economic-oracle-poison-"));
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn poisoned_original_owner_cannot_export_cached_state_after_uncertain_commit() {
        let path = std::env::temp_dir().join(format!(
            "tl-economic-oracle-poison-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        let root = Directory(path);
        let account = |value: &str| AccountId::try_from(value).unwrap();
        let property = PropertyId::try_from(7).unwrap();
        let config = EconomicChainConfig {
            economics: crate::economic::EconomicConfig {
                source_consensus: None,
                network: TradeLayerMainnet::Litecoin,
                contract: ContractId::try_from(1).unwrap(),
                collateral: property,
                inverse: false,
                notional: Decimal::from(1u32),
                initial_margin_per_contract: Decimal::from(10u32),
                fee_route: FeeRoute::Native(1),
                policy: SettlementPolicy::ConservingV1,
                funding: None,
                marking: None,
                liquidation: None,
                amm: None,
                contract_binding: None,
            },
            admin: account("admin"),
            permitted_spot_properties: BTreeSet::from([property]),
            rollback_limit: 2,
        };
        let mut ledger = Ledger::new();
        ledger
            .apply(
                TxId::from_bytes([1; 32]),
                Operation::CreateProperty {
                    id: property,
                    issuer: account("alice"),
                    kind: PropertyKind::Fixed,
                    initial_atoms: 100_000_000_000,
                },
            )
            .unwrap();
        let chain = EconomicChain::new_unverified(config.clone(), ledger).unwrap();
        let tip = chain.tip();
        let mut owner = DurableEconomicChain::initialize(&root.0, chain, tip).unwrap();
        let head = owner.head().unwrap();
        let expected = ExportExpectation {
            config: &config,
            head,
            state_commitment: None,
        };
        assert!(EconomicStateOracleExport::from_durable(&owner, &expected).is_ok());
        owner.test_fail_next_commit_after_sync();
        let block = UnverifiedEconomicBlock {
            network: config.economics.network,
            header: BlockHeader {
                height: 1,
                id: BlockId([9; 32]),
                parent: head.tip.id,
            },
            inputs: vec![],
        };
        assert!(owner.apply_unverified_block(head, &block).is_err());
        assert!(matches!(
            EconomicStateOracleExport::from_durable(&owner, &expected),
            Err(EconomicOracleError::Durable(DurableEconomicError::Storage(
                StorageError::Poisoned
            )))
        ));
    }
}
