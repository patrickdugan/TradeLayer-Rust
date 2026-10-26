// Opt-in signed allocations in the existing ProtocolState/sole Ledger.
use crate::chain::Tip as ChannelParentTip;
use crate::channel_revision::{
    ChannelRevisionPolicy, RevisionContext, RevisionError, SignedRevision,
};
use crate::ledger::ExpectedChannelRevisionPolicies;

// Bounded block input; retain the stable inline signature envelope API.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignedChannelInput {
    Enroll { txid: TxId, channel: AccountId },
    Revision(SignedRevision),
}
impl SignedChannelInput {
    pub fn replay_id(&self) -> Result<TxId, ExecutionError> {
        Ok(match self {
            Self::Enroll { txid, .. } => *txid,
            Self::Revision(r) => TxId::from_bytes(
                crate::channel_revision::revision_id(&r.revision).map_err(LedgerError::from)?,
            ),
        })
    }
}
impl ProtocolState {
    /// Independently configured opt-in policy, moved with this actual owner.
    /// It cannot replace an already enabled table or existing enrollment.
    pub fn with_channel_revision_policies_unverified(
        mut self,
        expected: ExpectedChannelRevisionPolicies,
    ) -> Result<Self, ExecutionError> {
        if !self.channel_revision_policies.is_empty() || self.ledger.has_signed_channel_revisions()
        {
            return Err(LedgerError::from(RevisionError::WrongPolicy).into());
        }
        validate_protocol_policy_table(self.chain, &expected)?;
        self.channel_revision_policies = expected;
        self.snapshot_bytes()
            .map_err(|_| LedgerError::from(RevisionError::Capacity))?;
        Ok(self)
    }
    pub fn channel_revision_policies(&self) -> &ExpectedChannelRevisionPolicies {
        &self.channel_revision_policies
    }
    /// Local co-signed allocation processing. Actual chain owners must supply
    /// their real parent tip; this unverified standalone method proves no L1 header.
    pub fn apply_signed_channel_inputs_unverified(
        &mut self,
        parent: ChannelParentTip,
        height: u64,
        inputs: &[SignedChannelInput],
    ) -> Result<(), ExecutionError> {
        if inputs.is_empty() {
            return Err(ExecutionError::EmptyBatch);
        }
        if inputs.len() > MAX_UNVERIFIED_BATCH {
            return Err(ExecutionError::BatchTooLarge);
        }
        if parent.height.checked_add(1) != Some(height)
            || self.finalized_height.unwrap_or(0) != parent.height
            || self.last_height.is_some_and(|h| h > height)
        {
            return Err(ExecutionError::InvalidHeight);
        }
        let network = validate_protocol_policy_table(self.chain, &self.channel_revision_policies)?
            .ok_or(LedgerError::from(RevisionError::WrongPolicy))?;
        let context = RevisionContext {
            network,
            parent,
            height,
        };
        let mut pending = self.clone();
        for input in inputs {
            let id = input.replay_id()?;
            if !pending.processed.insert(id) {
                return Err(ExecutionError::Replay(id));
            }
            match input {
                SignedChannelInput::Enroll { txid, channel } => {
                    pending.ledger.enroll_channel_revision_unverified(
                        *txid,
                        channel,
                        &pending.channel_revision_policies,
                        context,
                    )?;
                }
                SignedChannelInput::Revision(signed) => {
                    pending
                        .ledger
                        .apply_channel_revision_unverified(signed, context)?;
                }
            }
            pending.last_height = Some(height);
        }
        pending
            .encode_channel_stage(true)
            .map_err(|_| LedgerError::from(RevisionError::Capacity))?;
        *self = pending;
        Ok(())
    }
}
fn validate_protocol_policy_table(
    chain: ChainId,
    table: &ExpectedChannelRevisionPolicies,
) -> Result<Option<crate::network_profile::TradeLayerMainnet>, LedgerError> {
    if table.len() > crate::channel_revision::MAX_CHANNELS {
        return Err(RevisionError::Capacity.into());
    }
    let mut network = None;
    for (channel, p) in table {
        if channel != &p.channel
            || chain.0 != p.network.genesis().0
            || network.is_some_and(|n| n != p.network)
        {
            return Err(RevisionError::WrongPolicy.into());
        }
        crate::channel_revision::policy_id(p)?;
        network = Some(p.network);
    }
    Ok(network)
}
fn protocol_write_channel_policies(
    out: &mut Vec<u8>,
    table: &ExpectedChannelRevisionPolicies,
) -> Result<(), ProtocolSnapshotError> {
    out.extend_from_slice(&(table.len() as u32).to_be_bytes());
    for p in table.values() {
        write_embedded(
            out,
            &crate::channel_revision::canonical_policy_bytes(p)
                .map_err(|_| protocol_bad("invalid signed policy"))?,
        )?;
    }
    Ok(())
}
fn protocol_read_channel_policies(
    reader: &mut Cursor<'_>,
    expected: &ExpectedChannelRevisionPolicies,
) -> Result<ExpectedChannelRevisionPolicies, ProtocolSnapshotError> {
    let n = reader
        .u32()
        .map_err(|_| protocol_bad("signed policy count"))? as usize;
    if n == 0 || n > crate::channel_revision::MAX_CHANNELS {
        return Err(protocol_bad("signed policy capacity"));
    }
    let mut table = ExpectedChannelRevisionPolicies::new();
    let mut previous = None;
    for _ in 0..n {
        let p: ChannelRevisionPolicy =
            crate::channel_revision::parse_policy(read_embedded(reader)?)
                .map_err(|_| protocol_bad("invalid signed policy"))?;
        if previous.as_ref().is_some_and(|v| v >= &p.channel)
            || expected.get(&p.channel) != Some(&p)
        {
            return Err(protocol_bad("independent signed policy mismatch"));
        }
        previous = Some(p.channel.clone());
        table.insert(p.channel.clone(), p);
    }
    if &table != expected {
        return Err(protocol_bad("incomplete signed policy table"));
    }
    Ok(table)
}
