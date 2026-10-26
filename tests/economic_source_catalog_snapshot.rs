extern crate tradelayer_core as native_owner;
#[path = "support/native_fixture.rs"]
mod native_support;

use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::clearlist_source_owner::ClearlistSourceOwner;
use tradelayer_core::consensus_source::{
    SourceBlockMode, SourceConsensusConfig, SourceConsensusPolicy, SourceDisposition,
    SourceTransaction,
};
use tradelayer_core::economic::{EconomicConfig, EconomicInput, EconomicState, SettlementPolicy};
use tradelayer_core::joint::JointProtocolState;
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::native_entitlement::NativeDriver;
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::property_catalog::{DatabaseFailure, PropertyCatalog};
use tradelayer_core::source_catalogs::{self, SourceCatalogOwners};
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceException, SourceRuleError,
    SourceValue as V,
};

fn a(text: &str) -> AccountId {
    text.try_into().unwrap()
}
fn p(n: u32) -> PropertyId {
    n.try_into().unwrap()
}
fn id(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn text(s: &str) -> V {
    V::String(s.into())
}
fn object(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
}
struct NoTally;
impl SourceBackend for NoTally {
    fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        panic!("fixture metadata reads must not manufacture a Tally call");
    }
}

fn catalogs(label: &str) -> SourceCatalogOwners {
    let row = object(&[
        ("ticker", text("COLL")),
        ("type", V::number(1.0)),
        ("issuer", text("alice")),
        ("totalInCirculation", V::number(100.0)),
        ("unknownPropertyMetadata", text(label)),
    ]);
    let document = object(&[
        ("_id", text("propertyIndex")),
        ("value", V::Array(vec![V::Array(vec![V::number(7.0), row])])),
        (
            "unknownDocumentMetadata",
            object(&[("nested", text(label))]),
        ),
    ]);
    let mut properties = PropertyCatalog::from_document(Some(document)).unwrap();
    properties.execute("load", &[], &mut NoTally).unwrap();
    // A caught DB failure is part of the complete property journal, even when
    // its final document/cache is indistinguishable from an earlier state.
    properties
        .execute_with_faults(
            "load",
            &[],
            &[DatabaseFailure {
                operation: 0,
                exception: SourceException {
                    name: "Error".into(),
                    message: format!("retained-bootstrap-error-{label}"),
                },
            }],
            &mut NoTally,
        )
        .unwrap();
    let mut clearlists = ClearlistSourceOwner::new();
    clearlists
        .import_rows(
            "clearlists",
            vec![object(&[
                ("_id", text("preserved-list")),
                ("id", V::number(4.0)),
                ("name", text(label)),
                ("admin", text("admin")),
                ("unknownRowMetadata", object(&[("retained", V::Bool(true))])),
            ])],
        )
        .unwrap();
    clearlists
        .import_rows(
            "attestations",
            vec![object(&[
                ("_id", text("preserved-attestation")),
                (
                    "data",
                    object(&[
                        ("listId", V::number(4.0)),
                        ("status", text("active")),
                        ("address", text("alice")),
                        ("unknownAttestationMetadata", text(label)),
                    ]),
                ),
            ])],
        )
        .unwrap();
    clearlists
        .evaluate(&SourceCall {
            method: "loadClearlists".into(),
            arguments: vec![],
        })
        .unwrap();
    SourceCatalogOwners {
        properties,
        clearlists,
    }
}

fn fixture(native: bool, label: &str) -> (EconomicState, EconomicConfig, BTreeSet<PropertyId>) {
    let network = TradeLayerMainnet::Bitcoin;
    let owners = catalogs(label);
    let (joint, native_policy, collateral, permitted) = if native {
        let (mut ledger, policy) = native_support::fixture(network, NativeDriver::MonotonicDeltaV1);
        ledger
            .adopt_native_entitlements_unverified(&policy)
            .unwrap();
        let joint = JointProtocolState::from_native_bootstrap_unverified(
            network,
            a("admin"),
            ledger,
            BTreeSet::new(),
            &policy,
        )
        .unwrap();
        (joint, Some(policy), p(1), BTreeSet::new())
    } else {
        let mut ledger = Ledger::new();
        ledger
            .apply(
                id(90),
                Operation::CreateProperty {
                    id: p(7),
                    issuer: a("alice"),
                    kind: PropertyKind::Fixed,
                    initial_atoms: 10_000_000_000,
                },
            )
            .unwrap();
        let permitted = BTreeSet::from([p(7)]);
        let joint = JointProtocolState::from_bootstrap_unverified(
            network,
            a("admin"),
            ledger,
            permitted.clone(),
        )
        .unwrap();
        (joint, None, p(7), permitted)
    };
    let config = EconomicConfig {
        network,
        contract: 1.try_into().unwrap(),
        collateral,
        inverse: false,
        notional: "1".parse().unwrap(),
        initial_margin_per_contract: "1".parse().unwrap(),
        fee_route: FeeRoute::Native(1),
        policy: SettlementPolicy::ConservingV1,
        funding: None,
        marking: None,
        liquidation: None,
        amm: None,
        contract_binding: None,
        native_entitlements: native_policy,
        insurance: None,
        source_consensus: Some(SourceConsensusConfig {
            bootstrap_commitment: EconomicState::source_bootstrap_commitment(&joint).unwrap(),
            policy: SourceConsensusPolicy::ConservingSequentialV1,
            catalog_bootstrap_commitment: Some(source_catalogs::bootstrap_commitment(
                &owners.checkpoint_bytes().unwrap(),
            )),
        }),
    };
    let state =
        EconomicState::from_joint_with_source_catalogs_unverified(config.clone(), joint, owners)
            .unwrap();
    (state, config, permitted)
}
fn tx(position: u32, n: u8, payload: &str) -> SourceTransaction {
    SourceTransaction {
        position,
        txid: id(n),
        payload: payload.into(),
        marker: text("tl"),
        sender: text("admin"),
        reference: V::Null,
        sender_amount: V::number(1.0),
        reference_amount: V::number(0.0),
    }
}
fn history(state: &mut EconomicState) {
    let first = [tx(0, 60, &format!("01;2;13;14;18;30,{}", "11".repeat(32)))];
    let report = state
        .apply_source_block_unverified(
            BlockHeader {
                height: 1,
                id: BlockId([1; 32]),
                parent: state.config().network.genesis(),
            },
            SourceBlockMode::IndexedReplay,
            &first,
        )
        .unwrap();
    assert_eq!(report.accepted, 1);
    let mut rejected = tx(0, 61, "?invalid-source-rejected-EVIDENCE");
    rejected.reference = object(&[(
        "unusedRawMetadata",
        V::Array(vec![V::Undefined, text("preserve me")]),
    )]);
    let original = [rejected, tx(1, 60, "?duplicate-raw-candidate")];
    let report = state
        .apply_source_block_unverified(
            BlockHeader {
                height: 2,
                id: BlockId([2; 32]),
                parent: BlockId([1; 32]),
            },
            SourceBlockMode::IndexedReplay,
            &original,
        )
        .unwrap();
    assert_eq!(
        state.source_consensus_state().frames[1].transactions,
        original
    );
    assert!(report
        .receipts
        .iter()
        .any(|r| r.disposition != SourceDisposition::Applied));
}

#[derive(Clone)]
struct Parts {
    pins: Vec<u8>,
    sections: Vec<Vec<u8>>,
}
fn unpack(bytes: &[u8]) -> Parts {
    assert_eq!(&bytes[..5], b"TLEC\x0a");
    let mut offset = 70; // magic + joint pin + policy + catalog pin
    let mut sections = Vec::new();
    for _ in 0..4 {
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        sections.push(bytes[offset..offset + length].to_vec());
        offset += length;
    }
    assert_eq!(offset + 4, bytes.len());
    Parts {
        pins: bytes[..70].to_vec(),
        sections,
    }
}
fn crc(data: &[u8]) -> u32 {
    let mut value = !0u32;
    for byte in data {
        value ^= u32::from(*byte);
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(value & 1));
        }
    }
    !value
}
fn pack(parts: &Parts) -> Vec<u8> {
    let mut bytes = parts.pins.clone();
    for section in &parts.sections {
        bytes.extend_from_slice(&(section.len() as u32).to_be_bytes());
        bytes.extend_from_slice(section);
    }
    bytes.extend_from_slice(&crc(&bytes).to_be_bytes());
    bytes
}
fn rejects(bytes: &[u8], config: &EconomicConfig, permitted: &BTreeSet<PropertyId>) {
    assert!(EconomicState::from_snapshot_bytes(bytes, config, &a("admin"), permitted).is_err());
}

#[test]
fn complete_metadata_original_rejections_and_catalog_histories_roundtrip() {
    for native in [false, true] {
        let (mut state, config, permitted) = fixture(native, "unstripped");
        let before = state.source_consensus_state().catalogs.clone().unwrap();
        history(&mut state);
        let bytes = state.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..5], b"TLEC\x0a");
        let parts = unpack(&bytes);
        if native {
            assert_eq!(&parts.sections[0][..5], b"TLEC\x08");
        }
        let restored =
            EconomicState::from_snapshot_bytes(&bytes, &config, &a("admin"), &permitted).unwrap();
        assert_eq!(restored, state);
        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
        assert_eq!(
            restored.source_consensus_state().catalogs.as_ref().unwrap(),
            &before
        );
        assert!(before.properties.history_len() >= 2);
        assert_eq!(
            restored.source_consensus_state().frames[1]
                .transactions
                .len(),
            2
        );
        if native {
            assert_eq!(restored.native_entitlement_state().frames().len(), 2);
        }
    }
}

#[test]
fn accepted_catalog_metadata_replays_through_complete_native_and_generic_owners() {
    for native in [false, true] {
        let (mut state, config, permitted) = fixture(native, "native-metadata");
        let activate = tx(0, 50, &format!("07,{}", "11".repeat(32)));
        assert_eq!(
            state
                .apply_source_block_unverified(
                    BlockHeader {
                        height: 1,
                        id: BlockId([50; 32]),
                        parent: config.network.genesis()
                    },
                    SourceBlockMode::IndexedReplay,
                    std::slice::from_ref(&activate),
                )
                .unwrap()
                .accepted,
            1,
        );
        let create = tx(0, 51, "71BoatSLRHtKNngkdXEeobR76b53LETtpyT,Native catalog list,https://example.test,complete metadata");
        let report = state
            .apply_source_block_unverified(
                BlockHeader {
                    height: 2,
                    id: BlockId([51; 32]),
                    parent: BlockId([50; 32]),
                },
                SourceBlockMode::IndexedReplay,
                std::slice::from_ref(&create),
            )
            .unwrap();
        assert_eq!(report.accepted, 1, "{:?}", report.receipts);
        if native {
            assert_eq!(
                state.native_entitlement_state().frames()[1].inputs,
                vec![EconomicInput::SourceMetadata {
                    txid: id(51),
                    ty: 7
                }]
            );
        }
        let bytes = state.snapshot_bytes().unwrap();
        assert_eq!(
            &unpack(&bytes).sections[0][..5],
            if native { b"TLEC\x08" } else { b"TLEC\x01" }
        );
        if !native {
            let mut bare_config = config.clone();
            bare_config.source_consensus = None;
            rejects(&unpack(&bytes).sections[0], &bare_config, &permitted);
        }
        let restored =
            EconomicState::from_snapshot_bytes(&bytes, &config, &a("admin"), &permitted).unwrap();
        assert_eq!(restored, state);
        assert_eq!(
            restored
                .source_consensus_state()
                .catalogs
                .as_ref()
                .unwrap()
                .clearlists
                .rows("clearlists")
                .unwrap()
                .len(),
            2
        );
        // Both histories are genuine and produce identical funded/native owners.
        // Their metadata differs, so a valid audit from one cannot be paired with
        // the final catalogs from the other, even after repairing the outer CRC.
        let (mut alternate, alternate_config, _) = fixture(native, "native-metadata");
        assert_eq!(alternate_config, config);
        alternate
            .apply_source_block_unverified(
                BlockHeader {
                    height: 1,
                    id: BlockId([50; 32]),
                    parent: config.network.genesis(),
                },
                SourceBlockMode::IndexedReplay,
                &[activate],
            )
            .unwrap();
        let mut alternate_create = create;
        alternate_create.payload = alternate_create
            .payload
            .replace("Native catalog list", "Other catalog list");
        assert_eq!(
            alternate
                .apply_source_block_unverified(
                    BlockHeader {
                        height: 2,
                        id: BlockId([51; 32]),
                        parent: BlockId([50; 32])
                    },
                    SourceBlockMode::IndexedReplay,
                    &[alternate_create],
                )
                .unwrap()
                .accepted,
            1
        );
        let alternate_parts = unpack(&alternate.snapshot_bytes().unwrap());
        let mut mixed = unpack(&bytes);
        assert_eq!(mixed.sections[0], alternate_parts.sections[0]);
        assert_ne!(mixed.sections[3], alternate_parts.sections[3]);
        mixed.sections[1] = alternate_parts.sections[1].clone();
        rejects(&pack(&mixed), &config, &permitted);
    }
}

#[test]
fn independently_expected_pins_admin_network_and_source_presence_are_required() {
    let (state, config, permitted) = fixture(false, "pins");
    let bytes = state.snapshot_bytes().unwrap();
    let mut wrong = config.clone();
    wrong
        .source_consensus
        .as_mut()
        .unwrap()
        .bootstrap_commitment[0] ^= 1;
    rejects(&bytes, &wrong, &permitted);
    let mut wrong = config.clone();
    wrong
        .source_consensus
        .as_mut()
        .unwrap()
        .catalog_bootstrap_commitment
        .as_mut()
        .unwrap()[0] ^= 1;
    rejects(&bytes, &wrong, &permitted);
    let mut wrong = config.clone();
    wrong
        .source_consensus
        .as_mut()
        .unwrap()
        .catalog_bootstrap_commitment = None;
    rejects(&bytes, &wrong, &permitted);
    let mut wrong = config.clone();
    wrong.source_consensus = None;
    rejects(&bytes, &wrong, &permitted);
    let mut wrong = config.clone();
    wrong.network = TradeLayerMainnet::Litecoin;
    rejects(&bytes, &wrong, &permitted);
    assert!(
        EconomicState::from_snapshot_bytes(&bytes, &config, &a("mallory"), &permitted).is_err()
    );
    rejects(&bytes, &config, &BTreeSet::new());
}

#[test]
fn canonical_catalog_splices_and_history_stripping_fail_with_fresh_crcs() {
    let (mut state, config, permitted) = fixture(false, "original");
    history(&mut state);
    let bytes = state.snapshot_bytes().unwrap();
    let good = unpack(&bytes);
    let (other, _, _) = fixture(false, "spliced");
    let other_parts = unpack(&other.snapshot_bytes().unwrap());
    for section in [2, 3] {
        let mut parts = good.clone();
        parts.sections[section] = other_parts.sections[section].clone();
        rejects(&pack(&parts), &config, &permitted);
    }
    let mut stripped = state.source_consensus_state().catalogs.clone().unwrap();
    stripped.properties =
        PropertyCatalog::from_document(stripped.properties.document().cloned()).unwrap();
    stripped
        .properties
        .execute("load", &[], &mut NoTally)
        .unwrap();
    assert_eq!(
        stripped.properties.document(),
        state
            .source_consensus_state()
            .catalogs
            .as_ref()
            .unwrap()
            .properties
            .document()
    );
    assert_eq!(
        stripped.properties.cache_entries(),
        state
            .source_consensus_state()
            .catalogs
            .as_ref()
            .unwrap()
            .properties
            .cache_entries()
    );
    assert!(
        stripped.properties.history_len()
            < state
                .source_consensus_state()
                .catalogs
                .as_ref()
                .unwrap()
                .properties
                .history_len()
    );
    let mut parts = good.clone();
    parts.sections[3] = stripped.checkpoint_bytes().unwrap();
    rejects(&pack(&parts), &config, &permitted);
    let mut stripped = state.source_consensus_state().catalogs.clone().unwrap();
    stripped.clearlists = ClearlistSourceOwner::new();
    let mut parts = good.clone();
    parts.sections[3] = stripped.checkpoint_bytes().unwrap();
    rejects(&pack(&parts), &config, &permitted);
    let mut parts = good;
    parts.sections[3] = Vec::new();
    rejects(&pack(&parts), &config, &permitted);
}

#[test]
fn raw_rejected_audit_and_receipt_splices_fail_even_when_owner_balances_match() {
    let (mut state, config, permitted) = fixture(false, "audit");
    history(&mut state);
    let bytes = state.snapshot_bytes().unwrap();
    let good = unpack(&bytes);
    let mut parts = good.clone();
    let needle = b"?invalid-source-rejected-EVIDENCE";
    let location = parts.sections[1]
        .windows(needle.len())
        .position(|p| p == needle)
        .unwrap();
    parts.sections[1][location] = b'0'; // now a different Types evaluation, same length
    rejects(&pack(&parts), &config, &permitted);
    let mut parts = good.clone();
    let bootstrap_len = u32::from_be_bytes(parts.sections[1][..4].try_into().unwrap()) as usize;
    let frames_offset = 4 + bootstrap_len;
    parts.sections[1][frames_offset..frames_offset + 4].copy_from_slice(&0u32.to_be_bytes());
    parts.sections[1].truncate(frames_offset + 4);
    rejects(&pack(&parts), &config, &permitted);
    let mut parts = good.clone();
    let length = parts.sections[1].len();
    parts.sections[1][length - 1] ^= 1;
    rejects(&pack(&parts), &config, &permitted);
    let mut parts = good;
    parts.pins[37] = 2;
    rejects(&pack(&parts), &config, &permitted);
}

#[test]
fn truncation_trailing_bytes_and_source_layer_stripping_are_rejected() {
    let (state, config, permitted) = fixture(true, "structure");
    let bytes = state.snapshot_bytes().unwrap();
    let good = unpack(&bytes);
    for length in [0, 4, 5, 69, 70, bytes.len() - 1] {
        rejects(&bytes[..length], &config, &permitted);
    }
    let mut extra = bytes[..bytes.len() - 4].to_vec();
    extra.push(0);
    extra.extend_from_slice(&crc(&extra).to_be_bytes());
    rejects(&extra, &config, &permitted);
    rejects(&good.sections[0], &config, &permitted);
    let mut no_catalog_pin = config.clone();
    no_catalog_pin
        .source_consensus
        .as_mut()
        .unwrap()
        .catalog_bootstrap_commitment = None;
    rejects(&bytes, &no_catalog_pin, &permitted);
    let mut bare_config = config.clone();
    bare_config.source_consensus = None;
    let mut bare = EconomicState::from_snapshot_bytes(
        &good.sections[0],
        &bare_config,
        &a("admin"),
        &permitted,
    )
    .unwrap();
    let bare_before = bare.snapshot_bytes().unwrap();
    assert!(bare
        .apply_block_unverified(
            1,
            vec![EconomicInput::SourceMetadata {
                txid: id(81),
                ty: 7
            }]
        )
        .is_err());
    assert_eq!(bare.snapshot_bytes().unwrap(), bare_before);
    let mut state = state;
    let before = state.snapshot_bytes().unwrap();
    assert!(state
        .apply_block_unverified(
            1,
            vec![EconomicInput::SourceMetadata {
                txid: id(80),
                ty: 30
            }]
        )
        .is_err());
    assert_eq!(state.snapshot_bytes().unwrap(), before);
}
