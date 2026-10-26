use tradelayer_core::chain::{BlockHeader, BlockId, CandidateBlock, ChainId};
use tradelayer_core::channel_revision::{
    parse_policy, parse_signed_revision, ChannelRevisionPolicy, RevisionContext, SignedRevision,
};
use tradelayer_core::execution::{
    ProtocolOperation, ProtocolState, SignedChannelInput, UnverifiedTx,
};
use tradelayer_core::ledger::{
    channel_enrollment_replay_id, AccountId, ChannelSide, ExpectedChannelRevisionPolicies,
    PropertyId, PropertyKind, TxId,
};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::unified_chain::{
    AuthenticatedProtocolOperation, TrustedProtocolAdapter, UnifiedChainState,
};
pub fn account(s: &str) -> AccountId {
    AccountId::try_from(s).unwrap()
}
pub fn property() -> PropertyId {
    PropertyId::try_from(7).unwrap()
}
pub fn id(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
pub fn hex(s: &str) -> Vec<u8> {
    assert!(s.len().is_multiple_of(2));
    s.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(core::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
pub fn network_number(network: TradeLayerMainnet) -> u8 {
    match network {
        TradeLayerMainnet::Bitcoin => 1,
        TradeLayerMainnet::Litecoin => 2,
        TradeLayerMainnet::Dogecoin => 3,
    }
}
pub fn signed(network: TradeLayerMainnet, name: &str) -> (ChannelRevisionPolicy, SignedRevision) {
    let key = format!("{}-{name}", network_number(network));
    let line = include_str!("../../fixtures/channel_owner_vectors.tsv")
        .lines()
        .find(|l| l.split('\t').next() == Some(key.as_str()))
        .unwrap();
    let f: Vec<_> = line.split('\t').collect();
    (
        parse_policy(&hex(f[1])).unwrap(),
        parse_signed_revision(&hex(f[2])).unwrap(),
    )
}
pub fn funded_protocol(network: TradeLayerMainnet) -> ProtocolState {
    let chain = ChainId(network.genesis().0);
    let mut s = ProtocolState::new_unverified(chain, account("admin"));
    let tx = |n, actor: &str, height, operation| UnverifiedTx {
        chain,
        txid: id(n),
        claimed_actor: account(actor),
        claimed_height: height,
        operation,
    };
    s.apply_unverified(tx(
        100,
        "admin",
        1,
        ProtocolOperation::Registry(RegistryAction::Activate {
            types: vec![0, 1, 2, 4, 21, 22, 23, 31],
            code_hash: [9; 32],
        }),
    ))
    .unwrap();
    s.finalize_block_unverified(1).unwrap();
    for (n, actor, operation) in [
        (
            1,
            "alice",
            ProtocolOperation::Issue {
                id: property(),
                kind: PropertyKind::Fixed,
                initial_atoms: 1000,
            },
        ),
        (
            2,
            "alice",
            ProtocolOperation::Send {
                id: property(),
                recipient: account("bob"),
                amount_atoms: 400,
            },
        ),
        (
            3,
            "alice",
            ProtocolOperation::ChannelCommit {
                id: property(),
                channel: account("channel"),
                side: ChannelSide::A,
                amount_atoms: 600,
            },
        ),
        (
            4,
            "bob",
            ProtocolOperation::ChannelCommit {
                id: property(),
                channel: account("channel"),
                side: ChannelSide::B,
                amount_atoms: 400,
            },
        ),
    ] {
        s.apply_unverified(tx(n, actor, 2, operation)).unwrap();
    }
    s.finalize_block_unverified(2).unwrap();
    assert_eq!(
        s.ledger().snapshot_bytes().unwrap(),
        hex(include_str!("../../fixtures/channel_owner_bootstrap.hex").trim())
    );
    s
}
pub fn fixture(network: TradeLayerMainnet, history_limit: usize) -> UnifiedChainState {
    let (p, _) = signed(network, "first");
    let table = ExpectedChannelRevisionPolicies::from([(p.channel.clone(), p.clone())]);
    UnifiedChainState::from_protocol_with_channel_revision_policies_unverified(
        funded_protocol(network),
        p.bootstrap,
        account("admin"),
        history_limit,
        table,
    )
    .unwrap()
}
pub fn policies(network: TradeLayerMainnet) -> ExpectedChannelRevisionPolicies {
    let (p, _) = signed(network, "first");
    ExpectedChannelRevisionPolicies::from([(p.channel.clone(), p)])
}
pub fn enrollment(network: TradeLayerMainnet) -> SignedChannelInput {
    let (p, _) = signed(network, "first");
    let context = RevisionContext {
        network,
        parent: p.bootstrap,
        height: 3,
    };
    SignedChannelInput::Enroll {
        txid: channel_enrollment_replay_id(&p, context).unwrap(),
        channel: p.channel,
    }
}
pub fn block(network: TradeLayerMainnet, height: u64) -> CandidateBlock {
    CandidateBlock {
        chain: ChainId(network.genesis().0),
        header: BlockHeader {
            height,
            id: BlockId([((height - 1) * 0x11) as u8; 32]),
            parent: BlockId([((height - 2) * 0x11) as u8; 32]),
        },
        raw_transactions: vec![],
    }
}
// The supplied height2 bootstrap id22 and following ids33,44,55 are synthetic.
pub fn chain_block(network: TradeLayerMainnet, height: u64) -> CandidateBlock {
    let mut b = block(network, height);
    b.header.id = BlockId([(height * 0x11) as u8; 32]);
    b.header.parent = BlockId([((height - 1) * 0x11) as u8; 32]);
    b
}
pub struct EmptySyntheticAdapter;
impl TrustedProtocolAdapter for EmptySyntheticAdapter {
    fn verify_block(&self, _: &CandidateBlock) -> Result<(), String> {
        Ok(())
    }
    fn authenticate_and_decode(
        &self,
        _: ChainId,
        _: &BlockHeader,
        _: &[u8],
    ) -> Result<Option<AuthenticatedProtocolOperation>, String> {
        Ok(None)
    }
}
pub fn enrolled(network: TradeLayerMainnet, history_limit: usize) -> UnifiedChainState {
    let mut owner = fixture(network, history_limit);
    owner
        .apply_signed_channel_block_unverified(
            &chain_block(network, 3),
            &EmptySyntheticAdapter,
            &[enrollment(network)],
        )
        .unwrap();
    owner
}
