//! Canonical co-signed fixed-pool allocations. Signature possession and local
//! supersession do not authenticate L1 funding, actors, admission or revocation.
use crate::chain::{BlockId, Tip};
use crate::hash256::double_sha256_parts;
use crate::ledger::{AccountId, PropertyId};
use crate::network_profile::TradeLayerMainnet;
use crate::secp256k1::{canonical_compressed_public_key, verify_ecdsa_digest_strict, SecpError};

pub const MAX_CHANNELS: usize = 128;
pub const MAX_FUNDING_OUTPOINTS: usize = 64;
pub const MAX_PROPERTIES: usize = 256;
pub const MAX_REVISIONS: usize = 4096;
pub const MAX_HISTORY_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_POLICY_BYTES: usize = 4096;
pub const MAX_SIGNED_REVISION_BYTES: usize = 16_384;
pub const MAX_SAFE_HEIGHT: u64 = 9_007_199_254_740_991;
const POLICY_DOMAIN: &[u8] = b"TradeLayer/SignedChannelPolicy/v1/";
const REVISION_DOMAIN: &[u8] = b"TradeLayer/SignedChannelRevision/v1/";
const ALLOCATION_DOMAIN: &[u8] = b"TradeLayer/ChannelAllocation/v1/";
const BOOTSTRAP_ALLOCATION_DOMAIN: &[u8] = b"TradeLayer/ChannelBootstrapAllocation/v1/";
const BOOTSTRAP_LEDGER_DOMAIN: &[u8] = b"TradeLayer/ChannelBootstrapLedger/v1/";
const BOOTSTRAP_HEAD_DOMAIN: &[u8] = b"TradeLayer/ChannelRevisionBootstrap/v1/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionError {
    Capacity,
    InvalidInput,
    InvalidPolicy,
    InvalidKey,
    DuplicateKey,
    InvalidAllocation,
    Arithmetic,
    WrongPolicy,
    WrongBootstrap,
    WrongPreviousHead,
    WrongSequence,
    WrongCustody,
    WrongContext,
    Expired,
    NotYetValid,
    InvalidSignature,
    NonCanonical,
    Truncated,
    AuthenticationUnavailable,
    L1RevocationUnavailable,
}
impl core::fmt::Display for RevisionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RevisionError {}
type Result<T> = core::result::Result<T, RevisionError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FundingOutpointDisplay {
    pub txid_display: [u8; 32],
    pub vout: u32,
}
/// Independent application configuration. Public construction is not an
/// authenticated key/address/funding binding. Wire policy=1, binding=0 only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelRevisionPolicy {
    pub network: TradeLayerMainnet,
    pub channel: AccountId,
    pub participant_a: AccountId,
    pub participant_b: AccountId,
    pub public_key_a: [u8; 33],
    pub public_key_b: [u8; 33],
    pub funding_epoch: u64,
    pub funding_outpoints: Vec<FundingOutpointDisplay>,
    pub script_pubkey_sha256: [u8; 32],
    pub bootstrap: Tip,
    pub bootstrap_ledger_commitment: [u8; 32],
    pub bootstrap_allocation_commitment: [u8; 32],
}
impl ChannelRevisionPolicy {
    pub const fn require_authenticated_binding(&self) -> Result<()> {
        Err(RevisionError::AuthenticationUnavailable)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocationRow {
    pub property: PropertyId,
    pub atoms_a: u128,
    pub atoms_b: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionBody {
    pub policy_id: [u8; 32],
    pub sequence: u64,
    pub previous_revision_id: [u8; 32],
    pub before_allocation_digest: [u8; 32],
    pub parent: Tip,
    pub valid_from_height: u64,
    pub valid_until_height: u64,
    pub allocation: Vec<AllocationRow>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRevision {
    pub revision: RevisionBody,
    pub signature_a: [u8; 64],
    pub signature_b: [u8; 64],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisionHead {
    pub sequence: u64,
    pub revision_id: [u8; 32],
    pub allocation_digest: [u8; 32],
}
impl RevisionHead {
    pub const fn require_authenticated_admission(&self) -> Result<()> {
        Err(RevisionError::AuthenticationUnavailable)
    }
    pub const fn require_l1_revocation(&self) -> Result<()> {
        Err(RevisionError::L1RevocationUnavailable)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisionContext {
    pub network: TradeLayerMainnet,
    pub parent: Tip,
    pub height: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionStatus {
    Unknown,
    Current,
    Superseded,
}

fn height(h: u64) -> Result<()> {
    if h > MAX_SAFE_HEIGHT {
        Err(RevisionError::InvalidInput)
    } else {
        Ok(())
    }
}
fn normal(account: &AccountId) -> Result<()> {
    if account.as_str().starts_with('~') {
        Err(RevisionError::InvalidPolicy)
    } else {
        Ok(())
    }
}
fn check_policy(policy: &ChannelRevisionPolicy) -> Result<()> {
    if policy.funding_outpoints.is_empty() || policy.funding_outpoints.len() > MAX_FUNDING_OUTPOINTS
    {
        return Err(RevisionError::Capacity);
    }
    normal(&policy.channel)?;
    normal(&policy.participant_a)?;
    normal(&policy.participant_b)?;
    if policy.participant_a == policy.participant_b || policy.funding_epoch == 0 {
        return Err(RevisionError::InvalidPolicy);
    }
    height(policy.bootstrap.height)?;
    if policy.bootstrap.height == 0 && policy.bootstrap.id != policy.network.genesis() {
        return Err(RevisionError::InvalidPolicy);
    }
    if !policy.funding_outpoints.windows(2).all(|w| w[0] < w[1]) {
        return Err(RevisionError::NonCanonical);
    }
    for key in [&policy.public_key_a, &policy.public_key_b] {
        if canonical_compressed_public_key(key).map_err(|_| RevisionError::InvalidKey)? != *key {
            return Err(RevisionError::InvalidKey);
        }
    }
    if policy.public_key_a == policy.public_key_b {
        return Err(RevisionError::DuplicateKey);
    }
    Ok(())
}
fn check_allocation(rows: &[AllocationRow]) -> Result<()> {
    if rows.is_empty() || rows.len() > MAX_PROPERTIES {
        return Err(RevisionError::Capacity);
    }
    let mut previous = None;
    for row in rows {
        if previous.is_some_and(|p| p >= row.property) {
            return Err(RevisionError::NonCanonical);
        }
        previous = Some(row.property);
        if row.atoms_a == 0 && row.atoms_b == 0 {
            return Err(RevisionError::InvalidAllocation);
        }
        row.atoms_a
            .checked_add(row.atoms_b)
            .ok_or(RevisionError::Arithmetic)?;
    }
    Ok(())
}
fn check_body(body: &RevisionBody) -> Result<()> {
    check_allocation(&body.allocation)?;
    height(body.parent.height)?;
    height(body.valid_from_height)?;
    height(body.valid_until_height)?;
    if body.sequence == 0 || body.valid_from_height > body.valid_until_height {
        return Err(RevisionError::InvalidInput);
    }
    Ok(())
}
fn put_account(out: &mut Vec<u8>, account: &AccountId) {
    out.push(account.as_str().len() as u8);
    out.extend_from_slice(account.as_str().as_bytes());
}
fn write_policy(policy: &ChannelRevisionPolicy) -> Vec<u8> {
    let mut out = POLICY_DOMAIN.to_vec();
    out.extend_from_slice(&[1, policy.network.tag()]);
    out.extend_from_slice(&policy.network.genesis().0);
    for a in [
        &policy.channel,
        &policy.participant_a,
        &policy.participant_b,
    ] {
        put_account(&mut out, a);
    }
    out.extend_from_slice(&policy.public_key_a);
    out.extend_from_slice(&policy.public_key_b);
    out.extend_from_slice(&policy.funding_epoch.to_be_bytes());
    out.extend_from_slice(&(policy.funding_outpoints.len() as u16).to_be_bytes());
    for point in &policy.funding_outpoints {
        out.extend_from_slice(&point.txid_display);
        out.extend_from_slice(&point.vout.to_be_bytes());
    }
    out.extend_from_slice(&policy.script_pubkey_sha256);
    out.extend_from_slice(&policy.bootstrap.height.to_be_bytes());
    out.extend_from_slice(&policy.bootstrap.id.0);
    out.extend_from_slice(&policy.bootstrap_ledger_commitment);
    out.extend_from_slice(&policy.bootstrap_allocation_commitment);
    out.push(0);
    out
}
fn write_allocation(out: &mut Vec<u8>, rows: &[AllocationRow]) {
    out.extend_from_slice(&(rows.len() as u16).to_be_bytes());
    for row in rows {
        out.extend_from_slice(&row.property.get().to_be_bytes());
        out.extend_from_slice(&row.atoms_a.to_be_bytes());
        out.extend_from_slice(&row.atoms_b.to_be_bytes());
    }
}
fn write_body(body: &RevisionBody) -> Vec<u8> {
    let mut out = REVISION_DOMAIN.to_vec();
    out.push(0);
    out.extend_from_slice(&body.policy_id);
    out.extend_from_slice(&body.sequence.to_be_bytes());
    out.extend_from_slice(&body.previous_revision_id);
    out.extend_from_slice(&body.before_allocation_digest);
    out.extend_from_slice(&body.parent.height.to_be_bytes());
    out.extend_from_slice(&body.parent.id.0);
    out.extend_from_slice(&body.valid_from_height.to_be_bytes());
    out.extend_from_slice(&body.valid_until_height.to_be_bytes());
    write_allocation(&mut out, &body.allocation);
    out
}
pub fn canonical_policy_bytes(policy: &ChannelRevisionPolicy) -> Result<Vec<u8>> {
    check_policy(policy)?;
    Ok(write_policy(policy))
}
pub fn policy_id(policy: &ChannelRevisionPolicy) -> Result<[u8; 32]> {
    Ok(double_sha256_parts(&[&canonical_policy_bytes(policy)?]))
}
pub fn bootstrap_ledger_commitment(bytes: &[u8]) -> [u8; 32] {
    double_sha256_parts(&[BOOTSTRAP_LEDGER_DOMAIN, bytes])
}
pub fn canonical_allocation_bytes(rows: &[AllocationRow]) -> Result<Vec<u8>> {
    check_allocation(rows)?;
    let mut out = Vec::new();
    write_allocation(&mut out, rows);
    Ok(out)
}
pub fn bootstrap_allocation_commitment(rows: &[AllocationRow]) -> Result<[u8; 32]> {
    Ok(double_sha256_parts(&[
        BOOTSTRAP_ALLOCATION_DOMAIN,
        &canonical_allocation_bytes(rows)?,
    ]))
}
pub fn allocation_digest(policy_id: &[u8; 32], rows: &[AllocationRow]) -> Result<[u8; 32]> {
    Ok(double_sha256_parts(&[
        ALLOCATION_DOMAIN,
        policy_id,
        &canonical_allocation_bytes(rows)?,
    ]))
}
pub fn initial_head(
    policy: &ChannelRevisionPolicy,
    rows: &[AllocationRow],
) -> Result<RevisionHead> {
    let id = policy_id(policy)?;
    if bootstrap_allocation_commitment(rows)? != policy.bootstrap_allocation_commitment {
        return Err(RevisionError::WrongBootstrap);
    }
    let allocation_digest = allocation_digest(&id, rows)?;
    Ok(RevisionHead {
        sequence: 0,
        revision_id: double_sha256_parts(&[BOOTSTRAP_HEAD_DOMAIN, &id, &allocation_digest]),
        allocation_digest,
    })
}
pub fn canonical_revision_body_bytes(body: &RevisionBody) -> Result<Vec<u8>> {
    check_body(body)?;
    Ok(write_body(body))
}
pub fn revision_id(body: &RevisionBody) -> Result<[u8; 32]> {
    Ok(double_sha256_parts(&[&canonical_revision_body_bytes(
        body,
    )?]))
}
pub fn canonical_signed_revision_bytes(signed: &SignedRevision) -> Result<Vec<u8>> {
    let mut out = canonical_revision_body_bytes(&signed.revision)?;
    out.extend_from_slice(&signed.signature_a);
    out.extend_from_slice(&signed.signature_b);
    Ok(out)
}
fn verify_signatures(
    policy: &ChannelRevisionPolicy,
    signed: &SignedRevision,
    digest: &[u8; 32],
) -> Result<bool> {
    let one = |key: &[u8; 33], signature: &[u8; 64]| {
        verify_ecdsa_digest_strict(digest, key, signature).map_err(|e| match e {
            SecpError::InvalidPublicKey => RevisionError::InvalidKey,
            SecpError::InvalidSignature => RevisionError::InvalidSignature,
            SecpError::Arithmetic => RevisionError::Arithmetic,
        })
    };
    // Every successful result has verified both fixed policy slots.
    Ok(one(&policy.public_key_a, &signed.signature_a)?
        && one(&policy.public_key_b, &signed.signature_b)?)
}
/// Signature relation only. This does not establish the supplied policy's real
/// participant/funding ownership, freshness, current head or L1 admission.
pub fn verify_revision_signatures(
    policy: &ChannelRevisionPolicy,
    signed: &SignedRevision,
) -> Result<bool> {
    let id = policy_id(policy)?;
    if id != signed.revision.policy_id {
        return Err(RevisionError::WrongPolicy);
    }
    let digest = revision_id(&signed.revision)?;
    verify_signatures(policy, signed, &digest)
}
/// Pure owner preflight. The caller must supply its actual private head,
/// actual free custody and actual enclosing context; public structs are not
/// capabilities. The funded owner applies the returned head and allocation
/// atomically and independently checks actual property/owner registration.
pub fn validate_transition(
    policy: &ChannelRevisionPolicy,
    previous: &RevisionHead,
    actual_before: &[AllocationRow],
    context: RevisionContext,
    signed: &SignedRevision,
) -> Result<RevisionHead> {
    check_allocation(actual_before)?;
    check_body(&signed.revision)?;
    let id = policy_id(policy)?;
    let body = &signed.revision;
    if id != body.policy_id {
        return Err(RevisionError::WrongPolicy);
    }
    let before = allocation_digest(&id, actual_before)?;
    if before != previous.allocation_digest || before != body.before_allocation_digest {
        return Err(RevisionError::WrongCustody);
    }
    if previous.sequence == 0 && initial_head(policy, actual_before)? != *previous {
        return Err(RevisionError::WrongPreviousHead);
    }
    if previous.sequence.checked_add(1) != Some(body.sequence) {
        return Err(RevisionError::WrongSequence);
    }
    if previous.revision_id != body.previous_revision_id {
        return Err(RevisionError::WrongPreviousHead);
    }
    height(context.height)?;
    height(context.parent.height)?;
    if context.network != policy.network
        || context.parent != body.parent
        || context.parent.height.checked_add(1) != Some(context.height)
        || context.height <= policy.bootstrap.height
        || (context.parent.height == 0 && context.parent.id != policy.network.genesis())
    {
        return Err(RevisionError::WrongContext);
    }
    if context.height < body.valid_from_height {
        return Err(RevisionError::NotYetValid);
    }
    if context.height > body.valid_until_height {
        return Err(RevisionError::Expired);
    }
    if actual_before.len() != body.allocation.len() {
        return Err(RevisionError::WrongCustody);
    }
    for (before, after) in actual_before.iter().zip(&body.allocation) {
        if before.property != after.property
            || before.atoms_a.checked_add(before.atoms_b)
                != after.atoms_a.checked_add(after.atoms_b)
        {
            return Err(RevisionError::WrongCustody);
        }
    }
    let revision_id = revision_id(body)?;
    if !verify_signatures(policy, signed, &revision_id)? {
        return Err(RevisionError::InvalidSignature);
    }
    Ok(RevisionHead {
        sequence: body.sequence,
        revision_id,
        allocation_digest: allocation_digest(&id, &body.allocation)?,
    })
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(RevisionError::Capacity)?;
        let value = self
            .bytes
            .get(self.at..end)
            .ok_or(RevisionError::Truncated)?;
        self.at = end;
        Ok(value)
    }
    fn fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?
            .try_into()
            .map_err(|_| RevisionError::Truncated)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.fixed::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.fixed()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.fixed()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.fixed()?))
    }
    fn u128(&mut self) -> Result<u128> {
        Ok(u128::from_be_bytes(self.fixed()?))
    }
    fn expect(&mut self, value: &[u8]) -> Result<()> {
        if self.take(value.len())? == value {
            Ok(())
        } else {
            Err(RevisionError::InvalidInput)
        }
    }
    fn account(&mut self) -> Result<AccountId> {
        let n = usize::from(self.u8()?);
        if n == 0 || n > crate::ledger::MAX_ACCOUNT_BYTES {
            return Err(RevisionError::InvalidPolicy);
        }
        AccountId::try_from(
            core::str::from_utf8(self.take(n)?).map_err(|_| RevisionError::InvalidPolicy)?,
        )
        .map_err(|_| RevisionError::InvalidPolicy)
    }
    fn allocation(&mut self) -> Result<Vec<AllocationRow>> {
        let n = usize::from(self.u16()?);
        if n == 0 || n > MAX_PROPERTIES {
            return Err(RevisionError::Capacity);
        }
        if n.checked_mul(36)
            .is_none_or(|n| n > self.bytes.len() - self.at)
        {
            return Err(RevisionError::Truncated);
        }
        let mut rows = Vec::with_capacity(n);
        for _ in 0..n {
            rows.push(AllocationRow {
                property: PropertyId::try_from(self.u32()?)
                    .map_err(|_| RevisionError::InvalidAllocation)?,
                atoms_a: self.u128()?,
                atoms_b: self.u128()?,
            });
        }
        check_allocation(&rows)?;
        Ok(rows)
    }
    fn body(&mut self) -> Result<RevisionBody> {
        self.expect(REVISION_DOMAIN)?;
        if self.u8()? != 0 {
            return Err(RevisionError::InvalidInput);
        }
        let body = RevisionBody {
            policy_id: self.fixed()?,
            sequence: self.u64()?,
            previous_revision_id: self.fixed()?,
            before_allocation_digest: self.fixed()?,
            parent: Tip {
                height: self.u64()?,
                id: BlockId(self.fixed()?),
            },
            valid_from_height: self.u64()?,
            valid_until_height: self.u64()?,
            allocation: self.allocation()?,
        };
        check_body(&body)?;
        Ok(body)
    }
    fn end(&self) -> Result<()> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(RevisionError::NonCanonical)
        }
    }
}
pub fn parse_policy(bytes: &[u8]) -> Result<ChannelRevisionPolicy> {
    if bytes.len() > MAX_POLICY_BYTES {
        return Err(RevisionError::Capacity);
    }
    let mut r = Reader { bytes, at: 0 };
    r.expect(POLICY_DOMAIN)?;
    if r.u8()? != 1 {
        return Err(RevisionError::InvalidPolicy);
    }
    let network = TradeLayerMainnet::from_tag(r.u8()?).map_err(|_| RevisionError::InvalidPolicy)?;
    if BlockId(r.fixed()?) != network.genesis() {
        return Err(RevisionError::InvalidPolicy);
    }
    let channel = r.account()?;
    let participant_a = r.account()?;
    let participant_b = r.account()?;
    let public_key_a = r.fixed()?;
    let public_key_b = r.fixed()?;
    let funding_epoch = r.u64()?;
    let n = usize::from(r.u16()?);
    if n == 0 || n > MAX_FUNDING_OUTPOINTS {
        return Err(RevisionError::Capacity);
    }
    if n.checked_mul(36).is_none_or(|n| n > bytes.len() - r.at) {
        return Err(RevisionError::Truncated);
    }
    let mut funding_outpoints = Vec::with_capacity(n);
    for _ in 0..n {
        funding_outpoints.push(FundingOutpointDisplay {
            txid_display: r.fixed()?,
            vout: r.u32()?,
        });
    }
    let policy = ChannelRevisionPolicy {
        network,
        channel,
        participant_a,
        participant_b,
        public_key_a,
        public_key_b,
        funding_epoch,
        funding_outpoints,
        script_pubkey_sha256: r.fixed()?,
        bootstrap: Tip {
            height: r.u64()?,
            id: BlockId(r.fixed()?),
        },
        bootstrap_ledger_commitment: r.fixed()?,
        bootstrap_allocation_commitment: r.fixed()?,
    };
    if r.u8()? != 0 {
        return Err(RevisionError::InvalidPolicy);
    }
    r.end()?;
    if canonical_policy_bytes(&policy)? != bytes {
        return Err(RevisionError::NonCanonical);
    }
    Ok(policy)
}
pub fn parse_allocation(bytes: &[u8]) -> Result<Vec<AllocationRow>> {
    if bytes.len() > 2 + MAX_PROPERTIES * 36 {
        return Err(RevisionError::Capacity);
    }
    let mut r = Reader { bytes, at: 0 };
    let rows = r.allocation()?;
    r.end()?;
    Ok(rows)
}
pub fn parse_revision_body(bytes: &[u8]) -> Result<RevisionBody> {
    if bytes.len() > MAX_SIGNED_REVISION_BYTES - 128 {
        return Err(RevisionError::Capacity);
    }
    let mut r = Reader { bytes, at: 0 };
    let body = r.body()?;
    r.end()?;
    Ok(body)
}
/// Canonical structure only. Signature validity and policy/context binding are
/// separate checks; a successfully parsed signature is not a verified update.
pub fn parse_signed_revision(bytes: &[u8]) -> Result<SignedRevision> {
    if bytes.len() > MAX_SIGNED_REVISION_BYTES {
        return Err(RevisionError::Capacity);
    }
    let mut r = Reader { bytes, at: 0 };
    let revision = r.body()?;
    let signature_a = r.fixed()?;
    let signature_b = r.fixed()?;
    r.end()?;
    Ok(SignedRevision {
        revision,
        signature_a,
        signature_b,
    })
}
