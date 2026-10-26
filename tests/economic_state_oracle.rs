//! Producer integration tests over the actual funded sole owner, not balance mocks.
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::contract_registry::{ContractAsset, ContractId, ContractParameters};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    contract_definition_commitment, ContractBindingConfig, ContractRiskPolicy, CreateContractInput,
    EconomicConfig, EconomicInput, EconomicState, MarkConfig, MarkPolicy, SettlementPolicy,
};
use tradelayer_core::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, EconomicHead, UnverifiedEconomicBlock,
};
use tradelayer_core::economic_state_oracle::*;
use tradelayer_core::iou::IouKey;
use tradelayer_core::issuance_intent::JsonValue;
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::storage::{Namespace, Recovery};
use tradelayer_core::tally::FeeRoute;

const ATOMS: u128 = 100_000_000;
const PRECISE: u128 = 9_007_199_254_740_993;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-economic-state-oracle-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && self.0.starts_with(std::env::temp_dir()));
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("tl-economic-state-oracle-"));
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn a(value: &str) -> AccountId {
    value.try_into().unwrap()
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn c() -> ContractId {
    1.try_into().unwrap()
}
fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}
fn id(number: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&number.to_be_bytes());
    TxId::from_bytes(bytes)
}
fn config(network: TradeLayerMainnet) -> EconomicChainConfig {
    EconomicChainConfig {
        economics: EconomicConfig {
            native_entitlements: None,
            insurance: None,
            source_consensus: None,
            network,
            contract: c(),
            collateral: p(),
            inverse: false,
            notional: d("1"),
            initial_margin_per_contract: d("10"),
            fee_route: FeeRoute::Native(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
            marking: None,
            liquidation: None,
            amm: None,
            contract_binding: None,
        },
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p()]),
        rollback_limit: 4,
    }
}
fn ledger(extra: u128) -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(1),
            Operation::CreateProperty {
                id: p(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: PRECISE + 4000 * ATOMS + extra,
            },
        )
        .unwrap();
    for (number, owner) in [(2, "bob"), (3, "carol"), (4, "dave")] {
        ledger
            .apply(
                id(number),
                Operation::Transfer {
                    id: p(),
                    from: a("alice"),
                    to: a(owner),
                    amount_atoms: 1000 * ATOMS,
                },
            )
            .unwrap();
    }
    ledger
}
fn block(chain: &EconomicChain, marker: u8, inputs: Vec<EconomicInput>) -> UnverifiedEconomicBlock {
    UnverifiedEconomicBlock {
        network: chain.config().economics.network,
        header: BlockHeader {
            height: chain.tip().height + 1,
            id: BlockId([marker; 32]),
            parent: chain.tip().id,
        },
        inputs,
    }
}
fn activation() -> Vec<EconomicInput> {
    vec![EconomicInput::Joint(UnverifiedJointTx {
        txid: id(10),
        actor: a("admin"),
        operation: JointOperation::Registry(RegistryAction::Activate {
            types: vec![2, 6, 18],
            code_hash: [9; 32],
        }),
    })]
}
fn order(
    number: u64,
    owner: &str,
    side: Side,
    quantity: &str,
    price: &str,
    height: u64,
) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: id(number),
        owner: a(owner),
        contract: c(),
        height,
        side,
        quantity: d(quantity),
        price: d(price),
        post: false,
        is_market: false,
    })
}
fn pair(height: u64) -> Vec<EconomicInput> {
    match height {
        2 => vec![
            order(20, "alice", Side::Buy, "1", "100", 2),
            order(21, "bob", Side::Sell, "1", "100", 2),
        ],
        3 => vec![
            order(30, "alice", Side::Sell, "0.5", "110", 3),
            order(31, "carol", Side::Buy, "0.5", "110", 3),
        ],
        _ => panic!("fixture height"),
    }
}
fn funded(
    directory: &Directory,
    network: TradeLayerMainnet,
    extra: u128,
) -> (EconomicChainConfig, DurableEconomicChain) {
    let config = config(network);
    let chain = EconomicChain::new_unverified(config.clone(), ledger(extra)).unwrap();
    let tip = chain.tip();
    let mut owner = DurableEconomicChain::initialize(&directory.0, chain, tip).unwrap();
    for (height, inputs) in [(1, activation()), (2, pair(2)), (3, pair(3))] {
        let b = block(owner.chain().unwrap(), 100 + height as u8, inputs);
        owner
            .apply_unverified_block(owner.head().unwrap(), &b)
            .unwrap();
    }
    assert_eq!(
        owner
            .chain()
            .unwrap()
            .state()
            .ious()
            .claim_map(IouKey::new(1, 7))
            .unwrap()
            .claim(&a("alice")),
        Some(&d("5"))
    );
    (config, owner)
}
fn expected(config: &EconomicChainConfig, head: EconomicHead) -> ExportExpectation<'_> {
    ExportExpectation {
        config,
        head,
        state_commitment: None,
    }
}
fn decode_state(export: &EconomicStateOracleExport, config: &EconomicChainConfig) -> EconomicState {
    EconomicState::from_snapshot_bytes(
        export.state_bytes(),
        &config.economics,
        &config.admin,
        &config.permitted_spot_properties,
    )
    .unwrap()
}

#[test]
fn funded_owner_exports_exact_canonical_state_and_context_on_three_networks() {
    let mut roots = BTreeSet::new();
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let directory = Directory::new();
        let (config, owner) = funded(&directory, network, 0);
        let head = owner.head().unwrap();
        let before = owner.chain().unwrap().state().snapshot_bytes().unwrap();
        let export =
            EconomicStateOracleExport::from_durable(&owner, &expected(&config, head)).unwrap();
        assert_eq!(owner.head().unwrap(), head);
        assert_eq!(
            owner.chain().unwrap().state().snapshot_bytes().unwrap(),
            before
        );
        assert_eq!(export.state_bytes(), before);
        assert_eq!(export.network(), network);
        assert_eq!(export.genesis(), network.genesis());
        assert_eq!(export.head(), head);
        assert_eq!(export.state_codec_version(), 1);
        assert_eq!(export.checkpoint_commitment(), head.commitment);
        assert_ne!(export.state_commitment(), export.checkpoint_commitment());
        assert!(roots.insert(export.export_commitment()));
        let state = decode_state(&export, &config);
        let atoms = state.ledger().available_atoms(p(), &a("alice"));
        assert!(atoms > 9_007_199_254_740_992);
        assert_ne!(atoms, (atoms as f64) as u128);
        assert_eq!(
            atoms,
            owner
                .chain()
                .unwrap()
                .state()
                .ledger()
                .available_atoms(p(), &a("alice"))
        );
        assert_eq!(
            state
                .ious()
                .claim_map(IouKey::new(1, 7))
                .unwrap()
                .claim(&a("alice")),
            Some(&d("5"))
        );
        let bytes = export.canonical_bytes().unwrap();
        let decoded = EconomicStateOracleExport::decode(&bytes, &expected(&config, head)).unwrap();
        assert_eq!(decoded, export);
        assert_eq!(decoded.canonical_bytes().unwrap(), bytes);
        assert!(export.manifest_json().len() < 4096);
        assert!(JsonValue::parse(&export.manifest_json()).is_ok());
        assert!(export
            .manifest_json()
            .contains("\"authenticatedAdmission\":false"));
        assert!(export
            .manifest_json()
            .contains("\"transitionWitness\":\"not-exported\""));
        let namespace = Namespace::for_mainnet(network, "clearing").unwrap();
        assert!(namespace.path(&directory.0).exists());
    }
}

#[test]
fn fresh_valid_crc_and_self_hashes_cannot_replace_an_independent_head_or_state_pin() {
    let directory = Directory::new();
    let other_directory = Directory::new();
    let (config, owner) = funded(&directory, TradeLayerMainnet::Bitcoin, 0);
    let (_, other) = funded(&other_directory, TradeLayerMainnet::Bitcoin, 1);
    let head = owner.head().unwrap();
    let other_head = other.head().unwrap();
    assert_eq!(head.tip, other_head.tip);
    assert_eq!(head.sequence, other_head.sequence);
    assert_ne!(head.commitment, other_head.commitment);
    let original =
        EconomicStateOracleExport::from_durable(&owner, &expected(&config, head)).unwrap();
    let changed =
        EconomicStateOracleExport::from_durable(&other, &expected(&config, other_head)).unwrap();
    // Both artifacts have genuinely recomputed nested CRCs, replay-valid owner
    // state, state digest, full checkpoint digest, and complete envelope digest.
    let bytes = changed.canonical_bytes().unwrap();
    assert!(matches!(
        EconomicStateOracleExport::decode(&bytes, &expected(&config, head)),
        Err(EconomicOracleError::WrongHead)
    ));
    let pinned = ExportExpectation {
        config: &config,
        head: other_head,
        state_commitment: Some(original.state_commitment()),
    };
    assert!(matches!(
        EconomicStateOracleExport::decode(&bytes, &pinned),
        Err(EconomicOracleError::WrongState)
    ));
    let changed_expectation = ExportExpectation {
        config: &config,
        head,
        state_commitment: Some(changed.state_commitment()),
    };
    assert!(matches!(
        EconomicStateOracleExport::from_durable(&owner, &changed_expectation),
        Err(EconomicOracleError::WrongState)
    ));
}

#[test]
fn independent_configuration_and_network_are_checked() {
    let directory = Directory::new();
    let (config, owner) = funded(&directory, TradeLayerMainnet::Litecoin, 0);
    let head = owner.head().unwrap();
    let export = EconomicStateOracleExport::from_durable(&owner, &expected(&config, head)).unwrap();
    let bytes = export.canonical_bytes().unwrap();
    let mut changed = config.clone();
    changed.admin = a("other-admin");
    assert!(matches!(
        EconomicStateOracleExport::from_durable(&owner, &expected(&changed, head)),
        Err(EconomicOracleError::WrongConfig)
    ));
    assert!(EconomicStateOracleExport::decode(&bytes, &expected(&changed, head)).is_err());
    changed = config.clone();
    changed.economics.initial_margin_per_contract = d("11");
    assert!(EconomicStateOracleExport::decode(&bytes, &expected(&changed, head)).is_err());
    changed = config.clone();
    changed
        .permitted_spot_properties
        .insert(8.try_into().unwrap());
    assert!(EconomicStateOracleExport::decode(&bytes, &expected(&changed, head)).is_err());
    changed = config.clone();
    changed.economics.network = TradeLayerMainnet::Dogecoin;
    assert!(matches!(
        EconomicStateOracleExport::decode(&bytes, &expected(&changed, head)),
        Err(EconomicOracleError::WrongNetwork)
    ));
    changed = config.clone();
    changed.rollback_limit = 3;
    assert!(matches!(
        EconomicStateOracleExport::decode(&bytes, &expected(&changed, head)),
        Err(EconomicOracleError::WrongConfig)
    ));
}

#[test]
fn canonical_framing_rejects_truncation_versions_length_claims_and_changed_payload() {
    let directory = Directory::new();
    let (config, owner) = funded(&directory, TradeLayerMainnet::Bitcoin, 0);
    let head = owner.head().unwrap();
    let export = EconomicStateOracleExport::from_durable(&owner, &expected(&config, head)).unwrap();
    let bytes = export.canonical_bytes().unwrap();
    for end in [0, 4, HEADER_BYTES - 1, HEADER_BYTES, bytes.len() - 1] {
        assert!(
            EconomicStateOracleExport::decode(&bytes[..end], &expected(&config, head)).is_err(),
            "end{end}"
        );
    }
    for index in [
        0,
        4,
        5,
        6,
        38,
        46,
        54,
        86,
        94,
        126,
        135,
        167,
        199,
        HEADER_BYTES + 123,
    ] {
        let mut changed = bytes.clone();
        changed[index] ^= 0x40;
        assert!(
            EconomicStateOracleExport::decode(&changed, &expected(&config, head)).is_err(),
            "index{index}"
        );
    }
    for offset in [127, 131] {
        let mut changed = bytes.clone();
        changed[offset..offset + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(
            EconomicStateOracleExport::decode(&changed, &expected(&config, head)),
            Err(EconomicOracleError::Capacity)
        ));
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(EconomicStateOracleExport::decode(&trailing, &expected(&config, head)).is_err());
    let oversized = vec![0; MAX_EXPORT_BYTES + 1];
    assert!(matches!(
        EconomicStateOracleExport::decode(&oversized, &expected(&config, head)),
        Err(EconomicOracleError::Capacity)
    ));
}

#[test]
fn new_file_export_never_overwrites_and_bounded_read_roundtrips() {
    let directory = Directory::new();
    let (config, owner) = funded(&directory, TradeLayerMainnet::Bitcoin, 0);
    let head = owner.head().unwrap();
    let export = EconomicStateOracleExport::from_durable(&owner, &expected(&config, head)).unwrap();
    let path = directory.0.join("state.tlse");
    export.write_new(&path).unwrap();
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        export.write_new(&path),
        Err(EconomicOracleError::AlreadyExists)
    ));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(
        EconomicStateOracleExport::read_file(&path, &expected(&config, head)).unwrap(),
        export
    );
    assert!(matches!(
        export.write_new(std::path::Path::new("relative.tlse")),
        Err(EconomicOracleError::InvalidPath)
    ));
    let huge = directory.0.join("oversized.tlse");
    let file = fs::File::create(&huge).unwrap();
    file.set_len((MAX_EXPORT_BYTES + 1) as u64).unwrap();
    drop(file);
    assert!(matches!(
        EconomicStateOracleExport::read_file(&huge, &expected(&config, head)),
        Err(EconomicOracleError::Capacity)
    ));
    let truncated = directory.0.join("short.tlse");
    fs::write(&truncated, b"TLSE").unwrap();
    assert!(EconomicStateOracleExport::read_file(&truncated, &expected(&config, head)).is_err());
}

#[test]
fn exact_durable_restart_and_rollback_exports_preserve_fences_and_aba_sequence() {
    let directory = Directory::new();
    let (config, owner) = funded(&directory, TradeLayerMainnet::Dogecoin, 0);
    let original_head = owner.head().unwrap();
    let original =
        EconomicStateOracleExport::from_durable(&owner, &expected(&config, original_head)).unwrap();
    drop(owner);
    let (mut owner, _) = DurableEconomicChain::load(
        &directory.0,
        config.clone(),
        original_head,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    assert_eq!(
        EconomicStateOracleExport::from_durable(&owner, &expected(&config, original_head)).unwrap(),
        original
    );
    let mut prior = EconomicChain::from_checkpoint_bytes(
        original.checkpoint_bytes(),
        config.clone(),
        original_head.tip,
    )
    .unwrap();
    let prior_tip = prior.rollback_tip(prior.tip()).unwrap();
    let prior_state = prior.state().snapshot_bytes().unwrap();
    let rolled_head = owner.rollback_to(original_head, prior_tip).unwrap();
    assert!(matches!(
        EconomicStateOracleExport::from_durable(&owner, &expected(&config, original_head)),
        Err(EconomicOracleError::WrongHead)
    ));
    let rolled =
        EconomicStateOracleExport::from_durable(&owner, &expected(&config, rolled_head)).unwrap();
    assert_eq!(rolled.state_bytes(), prior_state);
    assert_ne!(rolled.state_commitment(), original.state_commitment());
    let replay = block(owner.chain().unwrap(), 103, pair(3));
    let reapplied = owner.apply_unverified_block(rolled_head, &replay).unwrap();
    assert_eq!(reapplied.tip, original_head.tip);
    assert_eq!(reapplied.commitment, original_head.commitment);
    assert!(reapplied.sequence > original_head.sequence);
    assert!(matches!(
        EconomicStateOracleExport::from_durable(&owner, &expected(&config, original_head)),
        Err(EconomicOracleError::WrongHead)
    ));
    let reexport =
        EconomicStateOracleExport::from_durable(&owner, &expected(&config, reapplied)).unwrap();
    assert_eq!(reexport.state_bytes(), original.state_bytes());
    assert_eq!(reexport.checkpoint_bytes(), original.checkpoint_bytes());
    assert_ne!(reexport.export_commitment(), original.export_commitment());
}

#[test]
fn active_v6_owner_export_restores_full_bound_input_and_mark_replay() {
    let directory = Directory::new();
    let mut config = config(TradeLayerMainnet::Litecoin);
    let creation = CreateContractInput {
        txid: id(30),
        issuer: a("issuer"),
        parameters: ContractParameters {
            native: false,
            underlying_oracle_id: Some(1),
            on_chain_data: vec![],
            notional_property_id: ContractAsset::Property(8.try_into().unwrap()),
            notional_value: d("1"),
            collateral_property_id: ContractAsset::Property(p()),
            leverage: d("10"),
            expiry_period: 0,
            series: 1,
            inverse: false,
            fee: Decimal::zero(),
            whitelist: Some(0),
            ticker: None,
        },
    };
    let bootstrap = JointProtocolState::from_bootstrap_unverified(
        config.economics.network,
        config.admin.clone(),
        ledger(0),
        config.permitted_spot_properties.clone(),
    )
    .unwrap();
    config.economics.initial_margin_per_contract = Decimal::zero();
    config.economics.fee_route = FeeRoute::Oracle(1);
    config.economics.marking = Some(MarkConfig {
        oracle_id: 1,
        policy: MarkPolicy::ConservingMarkBasisV1,
    });
    config.economics.contract_binding = Some(ContractBindingConfig {
        creation_txid: creation.txid,
        definition_hash: contract_definition_commitment(&creation).unwrap(),
        bootstrap_commitment: EconomicState::contract_bootstrap_commitment(&bootstrap).unwrap(),
        policy: ContractRiskPolicy::SourceContextsConservingV1,
    });
    let chain = EconomicChain::new_unverified(config.clone(), ledger(0)).unwrap();
    let tip = chain.tip();
    let mut owner = DurableEconomicChain::initialize(&directory.0, chain, tip).unwrap();
    let registry = |number, actor: &str, action| {
        EconomicInput::Joint(UnverifiedJointTx {
            txid: id(number),
            actor: a(actor),
            operation: JointOperation::Registry(action),
        })
    };
    let publish = |number: u64, price: u128| {
        registry(
            number,
            "publisher",
            RegistryAction::PublishOracle {
                oracle_id: 1,
                price_1e4: price * 10_000,
                high_1e4: None,
                low_1e4: None,
                close_1e4: None,
            },
        )
    };
    let frames = vec![
        vec![registry(
            10,
            "admin",
            RegistryAction::Activate {
                types: vec![2, 6, 13, 14, 15, 16, 18],
                code_hash: [5; 32],
            },
        )],
        vec![
            registry(
                20,
                "publisher",
                RegistryAction::CreateOracle {
                    ticker: "INDEX".into(),
                    url: "https://example.invalid".into(),
                    backup: a("backup"),
                    lag: 1,
                },
            ),
            EconomicInput::CreateContract(creation),
            publish(21, 100),
        ],
        vec![
            order(40, "alice", Side::Buy, "1", "100", 3),
            order(41, "bob", Side::Sell, "1", "100", 3),
        ],
        vec![publish(50, 110)],
    ];
    for (index, inputs) in frames.into_iter().enumerate() {
        let b = block(owner.chain().unwrap(), 110 + index as u8, inputs);
        owner
            .apply_unverified_block(owner.head().unwrap(), &b)
            .unwrap();
    }
    let head = owner.head().unwrap();
    let export = EconomicStateOracleExport::from_durable(&owner, &expected(&config, head)).unwrap();
    assert_eq!(export.state_codec_version(), 6);
    assert_eq!(&export.state_bytes()[..5], b"TLEC\x06");
    let restored = decode_state(&export, &config);
    assert_eq!(restored.contract_binding_state().frames.len(), 4);
    assert!(!restored.mark_state().events.is_empty());
    assert_eq!(
        restored.snapshot_bytes().unwrap(),
        owner.chain().unwrap().state().snapshot_bytes().unwrap()
    );
    let bytes = export.canonical_bytes().unwrap();
    assert_eq!(
        EconomicStateOracleExport::decode(&bytes, &expected(&config, head)).unwrap(),
        export
    );
}
