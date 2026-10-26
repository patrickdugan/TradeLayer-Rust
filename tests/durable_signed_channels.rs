//! Real disk recovery and fixed-pool custody under synthetic enclosing headers.
//! Test adapters authenticate no L1 transaction, funding output or actor.
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use tradelayer_core::chain::{BlockHeader, CandidateBlock, ChainId};
use tradelayer_core::channel_revision::{verify_revision_signatures, RevisionStatus};
use tradelayer_core::durable_signed_channels::{
    DurableSignedChannelChain as Owner, DurableSignedChannelError as Error,
    SignedChannelChainConfig as Config,
};
use tradelayer_core::execution::SignedChannelInput;
use tradelayer_core::ledger::ChannelSide;
use tradelayer_core::network_profile::TradeLayerMainnet as Network;
use tradelayer_core::storage::{Recovery, StorageError};
use tradelayer_core::unified_chain::{AuthenticatedProtocolOperation, TrustedProtocolAdapter};

#[allow(dead_code)]
#[path = "support/signed_channel_fixture.rs"]
mod fixture;
use fixture::{account, chain_block, enrollment, signed, EmptySyntheticAdapter};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "tl-durable-signed-channels-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let root = self.0.canonicalize().unwrap();
        let parent = std::env::temp_dir().canonicalize().unwrap();
        assert!(
            root.starts_with(parent)
                && root
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("tl-durable-signed-channels-")
        );
        let _ = fs::remove_dir_all(root);
    }
}
fn config(network: Network, limit: usize) -> Config {
    let (policy, _) = signed(network, "first");
    Config {
        network,
        anchor: policy.bootstrap,
        admin: account("admin"),
        history_limit: limit,
        expected_policies: fixture::policies(network),
    }
}
fn create(root: &Directory, network: Network, limit: usize) -> Owner {
    let chain = fixture::fixture(network, limit);
    let tip = chain.tip();
    Owner::initialize(&root.0, config(network, limit), chain, tip).unwrap()
}
fn enroll(owner: &mut Owner, network: Network) {
    let head = owner.head().unwrap();
    owner
        .apply_unverified_block(
            head,
            &chain_block(network, 3),
            &EmptySyntheticAdapter,
            &[enrollment(network)],
        )
        .unwrap();
}

#[test]
fn real_custody_signed_supersession_and_exact_restart_work_on_all_networks() {
    for network in [Network::Bitcoin, Network::Litecoin, Network::Dogecoin] {
        let root = Directory::new();
        let mut owner = create(&root, network, 4);
        enroll(&mut owner, network);
        let (policy, first) = signed(network, "first");
        let first_id = tradelayer_core::channel_revision::revision_id(&first.revision).unwrap();
        let before = owner.head().unwrap();
        owner
            .apply_unverified_block(
                before,
                &chain_block(network, 4),
                &EmptySyntheticAdapter,
                &[SignedChannelInput::Revision(first.clone())],
            )
            .unwrap();
        let (_, second) = signed(network, "second");
        let second_id = tradelayer_core::channel_revision::revision_id(&second.revision).unwrap();
        let before = owner.head().unwrap();
        owner
            .apply_unverified_block(
                before,
                &chain_block(network, 5),
                &EmptySyntheticAdapter,
                &[SignedChannelInput::Revision(second.clone())],
            )
            .unwrap();
        let ledger = owner.chain().unwrap().state().ledger();
        assert_eq!(
            ledger.channel_revision_status(&policy.channel, first_id),
            RevisionStatus::Superseded
        );
        assert_eq!(
            ledger.channel_revision_status(&policy.channel, second_id),
            RevisionStatus::Current
        );
        assert!(verify_revision_signatures(&policy, &first).unwrap());
        for row in &second.revision.allocation {
            assert_eq!(
                ledger.channel_atoms(row.property, &policy.channel, ChannelSide::A),
                row.atoms_a
            );
            assert_eq!(
                ledger.channel_atoms(row.property, &policy.channel, ChannelSide::B),
                row.atoms_b
            );
            assert_eq!(
                ledger.available_atoms(row.property, &policy.participant_a),
                0
            );
            assert_eq!(
                ledger.available_atoms(row.property, &policy.participant_b),
                0
            );
        }
        let expected = owner.head().unwrap();
        let bytes = owner.chain().unwrap().checkpoint_bytes().unwrap();
        drop(owner);
        let (restored, _) = Owner::load(
            &root.0,
            config(network, 4),
            expected,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        assert_eq!(restored.head().unwrap(), expected);
        assert_eq!(restored.chain().unwrap().checkpoint_bytes().unwrap(), bytes);
        assert_eq!(
            restored
                .chain()
                .unwrap()
                .state()
                .ledger()
                .channel_revision_status(&policy.channel, first_id),
            RevisionStatus::Superseded
        );
    }
}
struct CountingAdapter(AtomicUsize);
impl TrustedProtocolAdapter for CountingAdapter {
    fn verify_block(&self, _: &CandidateBlock) -> Result<(), String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err("must not run for stale head".into())
    }
    fn authenticate_and_decode(
        &self,
        _: ChainId,
        _: &BlockHeader,
        _: &[u8],
    ) -> Result<Option<AuthenticatedProtocolOperation>, String> {
        unreachable!()
    }
}

#[test]
fn rollback_reapply_changes_sequence_and_stale_heads_fail_before_adapter_work() {
    let network = Network::Bitcoin;
    let root = Directory::new();
    let mut owner = create(&root, network, 4);
    enroll(&mut owner, network);
    let enrollment_head = owner.head().unwrap();
    let (_, first) = signed(network, "first");
    let head1 = owner
        .apply_unverified_block(
            enrollment_head,
            &chain_block(network, 4),
            &EmptySyntheticAdapter,
            &[SignedChannelInput::Revision(first.clone())],
        )
        .unwrap();
    let rolled = owner.rollback_to(head1, enrollment_head.tip).unwrap();
    assert!(rolled.sequence > head1.sequence);
    assert_eq!(rolled.tip, enrollment_head.tip);
    assert_eq!(rolled.commitment, enrollment_head.commitment);
    let head2 = owner
        .apply_unverified_block(
            rolled,
            &chain_block(network, 4),
            &EmptySyntheticAdapter,
            &[SignedChannelInput::Revision(first)],
        )
        .unwrap();
    assert_eq!(head1.tip, head2.tip);
    assert_eq!(head1.commitment, head2.commitment);
    assert_ne!(head1.sequence, head2.sequence);
    let spy = CountingAdapter(AtomicUsize::new(0));
    let (_, second) = signed(network, "second");
    assert!(matches!(
        owner.apply_unverified_block(
            head1,
            &chain_block(network, 5),
            &spy,
            &[SignedChannelInput::Revision(second)]
        ),
        Err(Error::StaleHead)
    ));
    assert_eq!(spy.0.load(Ordering::SeqCst), 0);
    assert_eq!(owner.head().unwrap(), head2);
    drop(owner);
    assert!(matches!(
        Owner::load(
            &root.0,
            config(network, 4),
            head1,
            Recovery::RejectIncompleteTail
        ),
        Err(Error::StaleHead)
    ));
    assert!(Owner::load(
        &root.0,
        config(network, 4),
        head2,
        Recovery::RejectIncompleteTail
    )
    .is_ok());
}

#[test]
fn invalid_second_signature_or_block_context_leaves_disk_and_cached_owner_exact() {
    let network = Network::Litecoin;
    let root = Directory::new();
    let mut owner = create(&root, network, 4);
    enroll(&mut owner, network);
    let expected = owner.head().unwrap();
    let state = owner.chain().unwrap().checkpoint_bytes().unwrap();
    let path = owner.path();
    drop(owner);
    let disk = fs::read(&path).unwrap();
    let (mut owner, _) = Owner::load(
        &root.0,
        config(network, 4),
        expected,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    let (_, first) = signed(network, "first");
    let mut invalid = first.clone();
    invalid.signature_b[0] ^= 1;
    assert!(owner
        .apply_unverified_block(
            expected,
            &chain_block(network, 4),
            &EmptySyntheticAdapter,
            &[SignedChannelInput::Revision(invalid)]
        )
        .is_err());
    let mut wrong = chain_block(network, 4);
    wrong.header.parent.0[0] ^= 1;
    assert!(owner
        .apply_unverified_block(
            expected,
            &wrong,
            &EmptySyntheticAdapter,
            &[SignedChannelInput::Revision(first)]
        )
        .is_err());
    assert_eq!(owner.head().unwrap(), expected);
    assert_eq!(owner.chain().unwrap().checkpoint_bytes().unwrap(), state);
    drop(owner);
    assert_eq!(fs::read(path).unwrap(), disk);
}

#[test]
fn independent_configuration_and_expected_tip_cannot_be_replaced_by_checkpoint_fields() {
    let network = Network::Dogecoin;
    let root = Directory::new();
    let mut owner = create(&root, network, 4);
    enroll(&mut owner, network);
    let expected = owner.head().unwrap();
    let path = owner.path();
    drop(owner);
    let before = fs::read(&path).unwrap();
    let base = config(network, 4);
    for variant in 0..6 {
        let mut changed = base.clone();
        match variant {
            0 => changed.admin = account("wrong-admin"),
            1 => changed.anchor.id.0[0] ^= 1,
            2 => changed.history_limit = 3,
            3 => {
                changed
                    .expected_policies
                    .get_mut(&account("channel"))
                    .unwrap()
                    .funding_epoch += 1
            }
            4 => {
                changed
                    .expected_policies
                    .get_mut(&account("channel"))
                    .unwrap()
                    .public_key_a = changed.expected_policies[&account("channel")].public_key_b
            }
            _ => changed.expected_policies.clear(),
        }
        assert!(
            Owner::load(&root.0, changed, expected, Recovery::RejectIncompleteTail).is_err(),
            "variant {variant}"
        );
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    let mut wrong_tip = expected;
    wrong_tip.tip.id.0[0] ^= 1;
    assert!(matches!(
        Owner::load(
            &root.0,
            base.clone(),
            wrong_tip,
            Recovery::RejectIncompleteTail
        ),
        Err(Error::WrongTip)
    ));
    assert!(Owner::load(&root.0, base, expected, Recovery::RejectIncompleteTail).is_ok());
}

#[test]
fn real_lock_and_existing_store_fences_preserve_the_published_owner() {
    let network = Network::Bitcoin;
    let root = Directory::new();
    let owner = create(&root, network, 4);
    let expected = owner.head().unwrap();
    let path = owner.path();
    assert!(matches!(
        Owner::load(
            &root.0,
            config(network, 4),
            expected,
            Recovery::RejectIncompleteTail
        ),
        Err(Error::Storage(StorageError::Locked))
    ));
    drop(owner);
    let disk = fs::read(&path).unwrap();
    let chain = fixture::fixture(network, 4);
    let tip = chain.tip();
    assert!(matches!(
        Owner::initialize(&root.0, config(network, 4), chain, tip),
        Err(Error::InvalidStore)
    ));
    assert_eq!(fs::read(path).unwrap(), disk);
    assert!(Owner::load(
        &root.0,
        config(network, 4),
        expected,
        Recovery::RejectIncompleteTail
    )
    .is_ok());
}
