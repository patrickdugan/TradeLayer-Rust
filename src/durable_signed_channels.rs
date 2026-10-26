//! One synced owner for opt-in co-signed channel allocations. This supplies local
//! freshness and atomic recovery, not L1 funding authentication or revocation.
use crate::chain::{
    CandidateBlock, Tip, MAX_REORG_HISTORY, MAX_TRANSACTIONS_PER_BLOCK, MAX_TRANSACTION_BYTES,
};
use crate::channel_revision::{self, ChannelRevisionPolicy, RevisionError};
use crate::execution::SignedChannelInput;
use crate::ledger::{AccountId, ExpectedChannelRevisionPolicies};
use crate::network_profile::TradeLayerMainnet;
use crate::storage::{Journal, Mutation, Namespace, Recovery, RecoveryReport, StorageError};
use crate::unified_chain::{
    TrustedProtocolAdapter, UnifiedChainError, UnifiedChainState, UnifiedCheckpointError,
};
use std::path::{Path, PathBuf};

const STATE_KEY: &str = "channels/co-signed/full-chain-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedChannelChainConfig {
    pub network: TradeLayerMainnet,
    pub anchor: Tip,
    pub admin: AccountId,
    pub history_limit: usize,
    /// Must be supplied independently of candidates and restored checkpoints.
    pub expected_policies: ExpectedChannelRevisionPolicies,
}
impl SignedChannelChainConfig {
    pub fn validate(&self) -> Result<(), DurableSignedChannelError> {
        if self.admin.as_str().starts_with('~')
            || !(1..=MAX_REORG_HISTORY).contains(&self.history_limit)
            || self.anchor.height > channel_revision::MAX_SAFE_HEIGHT
            || self
                .network
                .check_genesis_if_applicable(self.anchor)
                .is_err()
            || self.expected_policies.is_empty()
            || self.expected_policies.len() > channel_revision::MAX_CHANNELS
        {
            return Err(DurableSignedChannelError::InvalidConfig);
        }
        for (channel, policy) in &self.expected_policies {
            self.validate_policy(channel, policy)?;
        }
        Ok(())
    }
    fn validate_policy(
        &self,
        channel: &AccountId,
        policy: &ChannelRevisionPolicy,
    ) -> Result<(), DurableSignedChannelError> {
        if channel != &policy.channel
            || policy.network != self.network
            || policy.bootstrap.height < self.anchor.height
            || (policy.bootstrap.height == self.anchor.height && policy.bootstrap != self.anchor)
        {
            return Err(DurableSignedChannelError::InvalidConfig);
        }
        channel_revision::canonical_policy_bytes(policy)?;
        Ok(())
    }
}

/// An independent exact freshness pin. Sequence changes on rollback/reapply,
/// including when the tip and allocation return to previously observed values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedChannelHead {
    pub sequence: u64,
    pub tip: Tip,
    pub commitment: [u8; 32],
}

#[derive(Debug)]
pub enum DurableSignedChannelError {
    Storage(StorageError),
    Chain(UnifiedChainError),
    Checkpoint(UnifiedCheckpointError),
    Revision(RevisionError),
    InvalidConfig,
    InvalidStore,
    WrongTip,
    StaleHead,
    Capacity,
}
impl core::fmt::Display for DurableSignedChannelError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DurableSignedChannelError {}
impl From<StorageError> for DurableSignedChannelError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}
impl From<UnifiedChainError> for DurableSignedChannelError {
    fn from(e: UnifiedChainError) -> Self {
        Self::Chain(e)
    }
}
impl From<UnifiedCheckpointError> for DurableSignedChannelError {
    fn from(e: UnifiedCheckpointError) -> Self {
        Self::Checkpoint(e)
    }
}
impl From<RevisionError> for DurableSignedChannelError {
    fn from(e: RevisionError) -> Self {
        Self::Revision(e)
    }
}

pub struct DurableSignedChannelChain {
    config: SignedChannelChainConfig,
    journal: Journal,
    chain: UnifiedChainState,
}
impl core::fmt::Debug for DurableSignedChannelChain {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Do not expose a poisoned cached Ledger through derived Debug output.
        f.debug_struct("DurableSignedChannelChain")
            .field("network", &self.config.network)
            .field("path", &self.journal.path())
            .field("head", &self.head())
            .finish_non_exhaustive()
    }
}
impl DurableSignedChannelChain {
    pub fn initialize(
        root: &Path,
        config: SignedChannelChainConfig,
        chain: UnifiedChainState,
        expected_tip: Tip,
    ) -> Result<Self, DurableSignedChannelError> {
        config.validate()?;
        check_context(&chain, &config, expected_tip)?;
        let bytes = chain.checkpoint_bytes()?;
        // Independently configured decoding verifies every retained state before
        // an initial store is created. Keep only the resulting sole owner.
        let chain = decode(&bytes, &config, expected_tip)?;
        let namespace = Namespace::for_mainnet(config.network, "channels")?;
        let (mut journal, _) = Journal::open(root, namespace, Recovery::RejectIncompleteTail)?;
        if journal.sequence()? != 0 || journal.entries()?.next().is_some() {
            return Err(DurableSignedChannelError::InvalidStore);
        }
        journal.commit(vec![Mutation::Put {
            key: STATE_KEY.into(),
            value: bytes,
        }])?;
        Ok(Self {
            config,
            journal,
            chain,
        })
    }
    pub fn load(
        root: &Path,
        config: SignedChannelChainConfig,
        expected: SignedChannelHead,
        recovery: Recovery,
    ) -> Result<(Self, RecoveryReport), DurableSignedChannelError> {
        config.validate()?;
        let namespace = Namespace::for_mainnet(config.network, "channels")?;
        let (journal, report) = Journal::open_existing(root, namespace, recovery)?;
        if journal.entries()?.count() != 1 {
            return Err(DurableSignedChannelError::InvalidStore);
        }
        let bytes = journal
            .get(STATE_KEY)?
            .ok_or(DurableSignedChannelError::InvalidStore)?;
        if journal.sequence()? != expected.sequence
            || checkpoint_commitment(bytes) != expected.commitment
        {
            return Err(DurableSignedChannelError::StaleHead);
        }
        let chain = decode(bytes, &config, expected.tip)?;
        Ok((
            Self {
                config,
                journal,
                chain,
            },
            report,
        ))
    }
    pub fn config(&self) -> &SignedChannelChainConfig {
        &self.config
    }
    pub fn chain(&self) -> Result<&UnifiedChainState, DurableSignedChannelError> {
        self.journal.sequence()?;
        Ok(&self.chain)
    }
    pub fn head(&self) -> Result<SignedChannelHead, DurableSignedChannelError> {
        let sequence = self.journal.sequence()?;
        let bytes = self
            .journal
            .get(STATE_KEY)?
            .ok_or(DurableSignedChannelError::InvalidStore)?;
        Ok(SignedChannelHead {
            sequence,
            tip: self.chain.tip(),
            commitment: checkpoint_commitment(bytes),
        })
    }
    pub fn path(&self) -> PathBuf {
        self.journal.path().to_owned()
    }
    pub fn apply_unverified_block<A: TrustedProtocolAdapter>(
        &mut self,
        expected: SignedChannelHead,
        block: &CandidateBlock,
        adapter: &A,
        inputs: &[SignedChannelInput],
    ) -> Result<SignedChannelHead, DurableSignedChannelError> {
        self.check_head(expected)?;
        preflight(block, inputs)?;
        let mut next = self.chain.clone();
        next.apply_signed_channel_block_unverified(block, adapter, inputs)?;
        self.publish(next)
    }
    pub fn rollback_to(
        &mut self,
        expected: SignedChannelHead,
        target: Tip,
    ) -> Result<SignedChannelHead, DurableSignedChannelError> {
        self.check_head(expected)?;
        if target.height >= self.chain.tip().height {
            return Err(DurableSignedChannelError::WrongTip);
        }
        let mut next = self.chain.clone();
        while next.tip().height > target.height {
            next.rollback_tip(next.tip())?;
        }
        if next.tip() != target {
            return Err(DurableSignedChannelError::WrongTip);
        }
        self.publish(next)
    }
    fn check_head(&self, expected: SignedChannelHead) -> Result<(), DurableSignedChannelError> {
        if self.head()? != expected {
            return Err(DurableSignedChannelError::StaleHead);
        }
        Ok(())
    }
    fn publish(
        &mut self,
        next: UnifiedChainState,
    ) -> Result<SignedChannelHead, DurableSignedChannelError> {
        check_context(&next, &self.config, next.tip())?;
        let bytes = next.checkpoint_bytes()?;
        // Synced Journal commit is the only publication point. An uncertain
        // write poisons Journal and therefore all cached head/state reads.
        self.journal.commit(vec![Mutation::Put {
            key: STATE_KEY.into(),
            value: bytes,
        }])?;
        self.chain = next;
        self.head()
    }
}
fn check_context(
    chain: &UnifiedChainState,
    config: &SignedChannelChainConfig,
    tip: Tip,
) -> Result<(), DurableSignedChannelError> {
    if chain.chain_id() != config.network.chain_id()
        || chain.anchor() != config.anchor
        || chain.history_limit() != config.history_limit
        || chain.channel_revision_policies() != &config.expected_policies
    {
        return Err(DurableSignedChannelError::InvalidConfig);
    }
    if chain.tip() != tip {
        return Err(DurableSignedChannelError::WrongTip);
    }
    Ok(())
}
fn decode(
    bytes: &[u8],
    config: &SignedChannelChainConfig,
    tip: Tip,
) -> Result<UnifiedChainState, DurableSignedChannelError> {
    let chain = UnifiedChainState::from_checkpoint_bytes_with_channel_policies(
        bytes,
        config.network.chain_id(),
        config.anchor,
        &config.admin,
        config.history_limit,
        &config.expected_policies,
    )?;
    check_context(&chain, config, tip)?;
    Ok(chain)
}
fn preflight(
    block: &CandidateBlock,
    inputs: &[SignedChannelInput],
) -> Result<(), DurableSignedChannelError> {
    if block
        .raw_transactions
        .len()
        .checked_add(inputs.len())
        .is_none_or(|n| n > MAX_TRANSACTIONS_PER_BLOCK)
        || block
            .raw_transactions
            .iter()
            .any(|raw| raw.is_empty() || raw.len() > MAX_TRANSACTION_BYTES)
    {
        return Err(DurableSignedChannelError::Capacity);
    }
    let mut bytes = 0usize;
    for input in inputs {
        let n = match input {
            SignedChannelInput::Enroll { channel, .. } => 32 + channel.as_str().len() + 1,
            SignedChannelInput::Revision(signed) => {
                channel_revision::canonical_signed_revision_bytes(signed)?.len()
            }
        };
        bytes = bytes
            .checked_add(n)
            .ok_or(DurableSignedChannelError::Capacity)?;
        if bytes > channel_revision::MAX_HISTORY_BYTES {
            return Err(DurableSignedChannelError::Capacity);
        }
    }
    Ok(())
}
fn checkpoint_commitment(bytes: &[u8]) -> [u8; 32] {
    crate::hash256::double_sha256_parts(&[b"TradeLayer/DurableSignedChannels/v1/", bytes])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(dead_code)]
    mod fixture {
        use crate as tradelayer_core;
        include!("../tests/support/signed_channel_fixture.rs");
    }

    #[test]
    fn synced_but_unacknowledged_owner_commit_poison_fences_cached_state_and_recovers_exactly() {
        let root = std::env::temp_dir().join(format!(
            "tl-signed-channel-sync-fault-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let network = TradeLayerMainnet::Bitcoin;
        std::fs::create_dir(&root).unwrap();
        let chain = fixture::fixture(network, 4);
        let config = SignedChannelChainConfig {
            network,
            anchor: chain.anchor(),
            admin: fixture::account("admin"),
            history_limit: 4,
            expected_policies: fixture::policies(network),
        };
        let tip = chain.tip();
        let mut owner =
            DurableSignedChannelChain::initialize(&root, config.clone(), chain, tip).unwrap();
        let before = owner.head().unwrap();
        owner.journal.test_fail_after_sync();
        assert!(matches!(
            owner.apply_unverified_block(
                before,
                &fixture::chain_block(network, 3),
                &fixture::EmptySyntheticAdapter,
                &[fixture::enrollment(network)]
            ),
            Err(DurableSignedChannelError::Storage(StorageError::Io(_)))
        ));
        assert!(matches!(
            owner.head(),
            Err(DurableSignedChannelError::Storage(StorageError::Poisoned))
        ));
        assert!(matches!(
            owner.chain(),
            Err(DurableSignedChannelError::Storage(StorageError::Poisoned))
        ));
        let debug = format!("{owner:?}");
        assert!(debug.contains("Poisoned"));
        assert!(!debug.contains("Ledger"));
        drop(owner);
        assert!(matches!(
            DurableSignedChannelChain::load(
                &root,
                config.clone(),
                before,
                Recovery::RejectIncompleteTail
            ),
            Err(DurableSignedChannelError::StaleHead)
        ));
        // Inspect the synced journal only after releasing the poisoned owner.
        // Do not infer whether a write committed from its lost acknowledgement.
        let namespace = Namespace::for_mainnet(network, "channels").unwrap();
        let (journal, _) =
            Journal::open_existing(&root, namespace, Recovery::RejectIncompleteTail).unwrap();
        let bytes = journal.get(STATE_KEY).unwrap().unwrap();
        let expected = SignedChannelHead {
            sequence: journal.sequence().unwrap(),
            tip: Tip {
                height: 3,
                id: fixture::chain_block(network, 3).header.id,
            },
            commitment: checkpoint_commitment(bytes),
        };
        assert_eq!(expected.sequence, before.sequence + 1);
        let recovered_bytes = bytes.to_vec();
        drop(journal);
        let (restored, _) = DurableSignedChannelChain::load(
            &root,
            config,
            expected,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        assert_eq!(restored.head().unwrap(), expected);
        assert_eq!(
            restored.chain().unwrap().checkpoint_bytes().unwrap(),
            recovered_bytes
        );
        drop(restored);
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(resolved
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("tl-signed-channel-sync-fault-"));
        std::fs::remove_dir_all(resolved).unwrap();
    }
}
