//! `tl_check_kyc` and `tl_check_commits` against real registry, clearlist-owner and channel-owner
//! state transitions.
//!
//! Primary source: tradelayer.js as pinned in `fixtures/legacy_clearlist_audit_reference.txt` and
//! `fixtures/legacy_channels_source.txt`. Cross-check: tradelayer-core
//! 9a9c27e0bece0914e685d625e927ac9d3924f3eb, `src/tradelayer/rpc.cpp` (`tl_check_kyc` 2676-2713,
//! `tl_check_commits` 2588-2629) and `tradelayer.cpp` (`checkAttestationReg` 5622-5675,
//! `getAllCommits` 7198-7254).
use std::collections::{BTreeMap, BTreeSet};
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::channels_source::SourceChannels;
use tradelayer_core::clearlist_source_owner::ClearlistSourceOwner;
use tradelayer_core::client::RpcValue;
use tradelayer_core::consensus_source::{SourceConsensusConfig, SourceConsensusPolicy};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    EconomicConfig, EconomicInput, EconomicState, FundingConfig, FundingPolicy, LiquidationConfig,
    LiquidationPolicy, MarkConfig, MarkPolicy, SettlementPolicy,
};
use tradelayer_core::economic_chain::{
    EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{
    AccountId, ChannelSide, Ledger, Operation, PropertyId, PropertyKind, TxId,
};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::property_catalog::PropertyCatalog;
use tradelayer_core::registry::{AdminTarget, RegistryAction};
use tradelayer_core::service::{ServiceError, ServiceMethod, ServiceRequest};
use tradelayer_core::service_routes::dispatch;
use tradelayer_core::service_view::EconomicReadBackend;
use tradelayer_core::source_catalogs::{self, SourceCatalogOwners};
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceRuleError, SourceValue as V,
};

const ATOMS: u128 = 100_000_000;
fn a(s: &str) -> AccountId {
    s.try_into().unwrap()
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}
fn id(n: u64) -> TxId {
    let mut b = [0; 32];
    b[24..].copy_from_slice(&n.to_be_bytes());
    TxId::from_bytes(b)
}
fn registry(n: u64, actor: &str, action: RegistryAction) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(n),
        actor: a(actor),
        operation: JointOperation::Registry(action),
    })
}
fn config() -> EconomicConfig {
    EconomicConfig {
        native_entitlements: None,
        insurance: None,
        source_consensus: None,
        network: TradeLayerMainnet::Bitcoin,
        contract: 1.try_into().unwrap(),
        collateral: p(),
        inverse: false,
        notional: d("1"),
        initial_margin_per_contract: d("10"),
        fee_route: FeeRoute::Oracle(1),
        policy: SettlementPolicy::ConservingV1,
        funding: Some(FundingConfig {
            oracle_id: 1,
            max_staleness_blocks: 32,
            policy: FundingPolicy::ConservingFundingV1,
        }),
        marking: Some(MarkConfig {
            oracle_id: 1,
            policy: MarkPolicy::ConservingMarkBasisV1,
        }),
        liquidation: Some(LiquidationConfig {
            max_price_deviation_bps: 100,
            max_fills_per_owner: 64,
            policy: LiquidationPolicy::ConservingLiquidationV1,
        }),
        amm: None,
        contract_binding: None,
    }
}
fn bootstrap_ledger() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(1),
            Operation::CreateProperty {
                id: p(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 4000 * ATOMS,
            },
        )
        .unwrap();
    for (n, owner) in [(2, "bob"), (3, "carol")] {
        ledger
            .apply(
                id(n),
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
fn apply(chain: &mut EconomicChain, inputs: Vec<EconomicInput>) {
    let height = chain.tip().height + 1;
    chain
        .apply_unverified_block(&UnverifiedEconomicBlock {
            network: chain.config().economics.network,
            header: BlockHeader {
                height,
                parent: chain.tip().id,
                id: BlockId([height as u8; 32]),
            },
            inputs,
        })
        .unwrap();
}
fn chain_config() -> EconomicChainConfig {
    EconomicChainConfig {
        economics: config(),
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p()]),
        rollback_limit: 4,
    }
}
fn activated() -> EconomicChain {
    let mut chain = EconomicChain::new_unverified(chain_config(), bootstrap_ledger()).unwrap();
    apply(
        &mut chain,
        vec![registry(
            10,
            "admin",
            RegistryAction::Activate {
                types: vec![7, 8, 9],
                code_hash: [0x76; 32],
            },
        )],
    );
    chain
}
fn list(n: u64, admin: &str, name: &str) -> EconomicInput {
    registry(
        n,
        admin,
        RegistryAction::CreateWhitelist {
            name: name.into(),
            url: "https://example.invalid/list".into(),
            description: "fixture".into(),
            backup: a("backup"),
        },
    )
}
fn attest(n: u64, admin: &str, list_id: u32, subject: &str) -> EconomicInput {
    registry(
        n,
        admin,
        RegistryAction::Attest {
            list_id,
            subject: a(subject),
            metadata: "ok".into(),
        },
    )
}
fn revoke(n: u64, admin: &str, list_id: u32, subject: &str) -> EconomicInput {
    registry(
        n,
        admin,
        RegistryAction::RevokeAttestation {
            list_id,
            subject: a(subject),
        },
    )
}
fn post(chain: &EconomicChain, route: &str, body: &str) -> Result<(u16, String), ServiceError> {
    let mut backend = EconomicReadBackend::from_chain(chain).unwrap();
    let request = ServiceRequest::new(
        ServiceMethod::Post,
        route,
        BTreeMap::new(),
        RpcValue::parse(body).unwrap(),
    )
    .unwrap();
    dispatch(&request, &mut backend).map(|r| (r.status, r.body))
}
fn kyc(chain: &EconomicChain, who: &str) -> String {
    let body = format!("{{\"address\":\"{who}\"}}");
    let (status, body) = post(chain, "/tl_check_kyc", &body).unwrap();
    assert_eq!(status, 200);
    body
}

// rpc.cpp:2707-2710 builds {"result: ": <verdict>} with the trailing space inside the key.
const ENABLED: &str = "{\"result: \":\"enabled(kyc_0)\"}";
const DISABLED: &str = "{\"result: \":\"disabled\"}";

#[test]
fn kyc_verdict_follows_real_attestation_issue_and_revoke() {
    let mut chain = activated();
    // No attestation anywhere: C++ answers "disabled" for an address absent from both registers.
    assert_eq!(kyc(&chain, "bob"), DISABLED);

    apply(
        &mut chain,
        vec![
            list(11, "issuer", "first"),
            list(12, "issuer2", "second"),
            attest(13, "issuer", 1, "bob"),
        ],
    );
    assert_eq!(kyc(&chain, "bob"), ENABLED);
    assert_eq!(kyc(&chain, "carol"), DISABLED);

    // checkAttestationReg has no list or kyc_id filter: a row in any list is enough.
    apply(&mut chain, vec![attest(14, "issuer2", 2, "carol")]);
    assert_eq!(kyc(&chain, "carol"), ENABLED);

    // Two rows for one address: revoking one keeps the verdict, revoking the last drops it.
    apply(&mut chain, vec![attest(15, "issuer2", 2, "bob")]);
    apply(&mut chain, vec![revoke(16, "issuer", 1, "bob")]);
    assert_eq!(kyc(&chain, "bob"), ENABLED);
    apply(&mut chain, vec![revoke(17, "issuer2", 2, "bob")]);
    // deleteAttestationReg removes the C++ record (tradelayer.cpp:5677-5725), so the next lookup
    // misses.
    assert_eq!(kyc(&chain, "bob"), DISABLED);
    assert_eq!(kyc(&chain, "carol"), ENABLED);

    // Reissue after revoke is a fresh attestation in both models.
    apply(&mut chain, vec![attest(18, "issuer", 1, "bob")]);
    assert_eq!(kyc(&chain, "bob"), ENABLED);
}

#[test]
fn kyc_accepts_the_params_form_and_rejects_unparseable_addresses() {
    let mut chain = activated();
    apply(
        &mut chain,
        vec![list(11, "issuer", "first"), attest(12, "issuer", 1, "bob")],
    );
    let (status, body) = post(&chain, "/tl_check_kyc", "{\"params\":\"bob\"}").unwrap();
    assert_eq!((status, body.as_str()), (200, ENABLED));
    // ParseAddress throws "Invalid address" in C++ (rpcvalues.cpp:29-37).
    for body in [
        "{}",
        "{\"address\":\"\"}",
        "{\"address\":\"~econ:x\"}",
        "{\"address\":7}",
    ] {
        assert_eq!(
            post(&chain, "/tl_check_kyc", body),
            Err(ServiceError::InvalidRequest),
            "{body}"
        );
    }
}

fn update_admin(n: u64, actor: &str, list_id: u32, new_address: &str) -> EconomicInput {
    registry(
        n,
        actor,
        RegistryAction::UpdateAdmin {
            target: AdminTarget::Whitelist(list_id),
            new_address: a(new_address),
            update_backup: false,
        },
    )
}

#[test]
fn clearlist_admins_are_enabled_and_attestation_outranks_admin() {
    let mut chain = activated();
    apply(
        &mut chain,
        vec![
            list(11, "issuer", "first"),
            list(12, "issuer2", "second"),
            attest(13, "issuer", 1, "bob"),
        ],
    );
    // createClearlist stores the admin (clearlist reference lines 10-30): not attested, so "enabled".
    assert_eq!(kyc(&chain, "issuer"), "{\"result: \":\"enabled\"}");
    assert_eq!(kyc(&chain, "issuer2"), "{\"result: \":\"enabled\"}");
    // updateAdmin with backup=false replaces the admin (lines 112-130).
    apply(&mut chain, vec![update_admin(14, "issuer", 1, "issuer3")]);
    assert_eq!(kyc(&chain, "issuer"), DISABLED);
    assert_eq!(kyc(&chain, "issuer3"), "{\"result: \":\"enabled\"}");
    // An address that is both attested and an admin reports the attestation first, as
    // checkAttestationReg is tested before checkKYCRegister in rpc.cpp:2707.
    apply(&mut chain, vec![attest(15, "issuer2", 2, "issuer3")]);
    assert_eq!(kyc(&chain, "issuer3"), ENABLED);
}

fn clearlist_rows() -> SourceCatalogOwners {
    let mut clearlists = ClearlistSourceOwner::new();
    let mut call = |method: &str, arguments: Vec<V>| {
        clearlists
            .evaluate(&SourceCall {
                method: method.into(),
                arguments,
            })
            .unwrap()
    };
    let s = |t: &str| V::String(t.into());
    let n = V::number;
    call(
        "createClearlist",
        vec![
            s("tl-admin"),
            s("kyc"),
            s("u"),
            s("d"),
            s("tl-backup"),
            n(1.0),
        ],
    );
    call("loadClearlists", vec![]);
    call("addAttestation", vec![n(1.0), s("tl-kyc"), s("m"), n(10.0)]);
    call(
        "addAttestation",
        vec![n(1.0), s("tl-revoked"), s("m"), n(11.0)],
    );
    call(
        "revokeAttestation",
        vec![n(1.0), s("tl-revoked"), s("why"), n(12.0)],
    );
    call("updateAdmin", vec![n(1.0), s("tl-admin2"), V::Bool(false)]);
    SourceCatalogOwners::new(PropertyCatalog::new(), clearlists)
}

#[test]
fn kyc_reads_tradelayer_js_clearlist_rows_when_the_state_carries_them() {
    let owners = clearlist_rows();
    let ledger = bootstrap_ledger();
    let permitted = BTreeSet::from([p()]);
    let joint = JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Bitcoin,
        a("admin"),
        ledger.clone(),
        permitted,
    )
    .unwrap();
    let mut economics = config();
    economics.source_consensus = Some(SourceConsensusConfig {
        bootstrap_commitment: EconomicState::source_bootstrap_commitment(&joint).unwrap(),
        policy: SourceConsensusPolicy::ConservingSequentialV1,
        catalog_bootstrap_commitment: Some(source_catalogs::bootstrap_commitment(
            &owners.checkpoint_bytes().unwrap(),
        )),
    });
    let chain = EconomicChain::new_with_source_catalogs_unverified(
        EconomicChainConfig {
            economics,
            ..chain_config()
        },
        ledger,
        owners,
    )
    .unwrap();
    // addAttestation rows with status active: enabled(kyc_0). A revoked row does not count
    // (revokeAttestation, reference lines 415-445). The updated admin is enabled. The original
    // admin stays enabled too: createClearlist writes row _id 1 (number) and updateAdmin writes
    // row _id "1" (string, reference line 113), so the stored rows keep both admins.
    assert_eq!(kyc(&chain, "tl-kyc"), ENABLED);
    assert_eq!(kyc(&chain, "tl-revoked"), DISABLED);
    assert_eq!(kyc(&chain, "tl-admin2"), "{\"result: \":\"enabled\"}");
    assert_eq!(kyc(&chain, "tl-admin"), "{\"result: \":\"enabled\"}");
    assert_eq!(kyc(&chain, "tl-backup"), DISABLED);
}

const CHANNEL: &str = "LKQa26zKBtyM2kw6aMEYyj4fCbCqxJyrPw";
const SENDER: &str = "1BoatSLRHtKNngkdXEeobR76b53LETtpyT";
const OTHER: &str = "1CounterpartyXXXXXXXXXXXXXXXUWLpVr";

/// Answers every dependency the channel owner raises while recording a commit.
struct Scripted;
impl SourceBackend for Scripted {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        Ok(DependencyReturn::Immediate(match call.target.as_str() {
            "DB.findAsync" => V::Array(vec![]),
            "TxUtils.getRawTransaction" => {
                V::Object(vec![("vin".into(), V::Array(vec![V::Object(vec![])]))])
            }
            "TxUtils.getAddressTypeUniversal" => V::String("p2pkh".into()),
            "TxUtils.extractPubkeyByType" => V::String("02".repeat(33)),
            "UUID.v4" => V::String("00000000-0000-4000-8000-000000000000".into()),
            "Clock.now" => V::number(1_700_000_000_000.0),
            _ => V::Undefined,
        }))
    }
}
fn commit(owner: &mut SourceChannels, sender: &str, property: f64, amount: f64, block: f64) {
    let outcome = owner
        .execute_unverified(
            &SourceCall {
                method: "recordCommitToChannel".into(),
                arguments: vec![
                    V::String(CHANNEL.into()),
                    V::String(sender.into()),
                    V::number(property),
                    V::number(amount),
                    V::Bool(false),
                    V::Array(vec![]),
                    V::number(block),
                    V::String(format!("txid-{block}")),
                ],
            },
            &mut Scripted,
        )
        .unwrap();
    assert!(
        outcome.deferred_rejections.is_empty(),
        "{:?}",
        outcome.deferred_rejections
    );
}
fn commits(chain: &EconomicChain, channels: &SourceChannels, who: &str) -> String {
    let mut backend = EconomicReadBackend::from_chain(chain)
        .unwrap()
        .with_source_channels(channels.state());
    let body = format!("{{\"address\":\"{who}\"}}");
    let request = ServiceRequest::new(
        ServiceMethod::Post,
        "/tl_check_commits",
        BTreeMap::new(),
        RpcValue::parse(&body).unwrap(),
    )
    .unwrap();
    let response = dispatch(&request, &mut backend).unwrap();
    assert_eq!(response.status, 200);
    response.body
}

#[test]
fn commits_list_every_commit_record_of_the_sender_across_channels() {
    let chain = activated();
    let mut channels = SourceChannels::new();
    // recordCommitToChannel (legacy_channels_source.txt:802-870) pushes one record per commit
    // onto channel.commits and assigns the first sender to column A.
    commit(&mut channels, SENDER, 7.0, 2.5, 100.0);
    commit(&mut channels, SENDER, 7.0, 1.0, 105.0);
    commit(&mut channels, SENDER, 9.0, 0.125, 110.0);
    let body = commits(&chain, &channels, SENDER);
    // Newest first, amount as an 8-decimal string and propertyId as a decimal string, the shapes
    // FormatByType produces in tradelayer.cpp:7237-7242; block_index has no tradelayer.js source.
    assert_eq!(
        body,
        format!(
            concat!(
                "[{{\"sender\":\"{s}\",\"channel\":\"{c}\",\"propertyId\":\"9\",\"amount\":\"0.12500000\",\"block\":110}},",
                "{{\"sender\":\"{s}\",\"channel\":\"{c}\",\"propertyId\":\"7\",\"amount\":\"1.00000000\",\"block\":105}},",
                "{{\"sender\":\"{s}\",\"channel\":\"{c}\",\"propertyId\":\"7\",\"amount\":\"2.50000000\",\"block\":100}}]"
            ),
            s = SENDER,
            c = CHANNEL
        )
    );
    // A different address has no commit record, and an unknown channel owner attached to the
    // backend still yields an empty list rather than an error.
    assert_eq!(commits(&chain, &channels, OTHER), "[]");
}

#[test]
fn commits_are_empty_without_a_channel_owner_and_never_unavailable() {
    // The economic chain refuses a ledger with channel liabilities, so it can hold no commit.
    let mut ledger = bootstrap_ledger();
    ledger
        .apply(
            id(4),
            Operation::ChannelCommit {
                id: p(),
                owner: a("bob"),
                channel: a("chan"),
                side: ChannelSide::A,
                amount_atoms: 25 * ATOMS,
            },
        )
        .unwrap();
    assert!(EconomicChain::new_unverified(chain_config(), ledger).is_err());
    let chain = activated();
    for body in ["{\"address\":\"bob\"}", "{\"params\":\"bob\"}"] {
        assert_eq!(
            post(&chain, "/tl_check_commits", body),
            Ok((200, "[]".to_owned())),
            "{body}"
        );
    }
    assert_eq!(
        post(&chain, "/tl_check_commits", "{\"address\":\"~econ:x\"}"),
        Err(ServiceError::InvalidRequest)
    );
}

#[test]
fn commits_are_capped_at_the_newest_101_like_getallcommits() {
    let chain = activated();
    let mut channels = SourceChannels::new();
    for block in 1..=120 {
        commit(&mut channels, SENDER, 7.0, 1.0, f64::from(block));
    }
    let body = commits(&chain, &channels, SENDER);
    let parsed = RpcValue::parse(&body).unwrap();
    assert_eq!(body.matches("\"sender\"").count(), 101);
    assert!(parsed.as_json().starts_with("[{\"sender\""));
    assert!(body.contains("\"block\":120}"));
    assert!(body.contains("\"block\":20}"));
    assert!(!body.contains("\"block\":19}"));
}
