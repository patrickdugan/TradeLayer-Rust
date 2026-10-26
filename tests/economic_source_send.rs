//! Full raw Types/validity/source catalogs compose with the sole actual Ledger.
//! Expected balances come from unchanged composed pinned JavaScript sources.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::clearlist_source_owner::ClearlistSourceOwner;
use tradelayer_core::consensus_source::{
    SourceBlockMode, SourceBlockReport, SourceConsensusConfig, SourceConsensusPolicy,
    SourceDisposition, SourceTransaction,
};
use tradelayer_core::economic::{EconomicConfig, EconomicState, SettlementPolicy};
use tradelayer_core::joint::JointProtocolState;
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::native_entitlement::*;
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::property_catalog::PropertyCatalog;
use tradelayer_core::source_catalogs::{
    bootstrap_commitment as catalog_commitment, SourceCatalogOwners,
};
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceOutcome, SourceRuleError,
    SourceValue as V,
};

const U: u128 = 100_000_000;
const A: &str = "1BoatSLRHtKNngkdXEeobR76b53LETtpyT";
const B: &str = "1BitcoinEaterAddressDontSendf59kuE";
const C: &str = "1CounterpartyXXXXXXXXXXXXXXXUWLpVr";
fn a(value: &str) -> AccountId {
    value.try_into().unwrap()
}
fn p(value: u32) -> PropertyId {
    value.try_into().unwrap()
}
fn id(value: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&value.to_be_bytes());
    TxId::from_bytes(bytes)
}
fn s(value: &str) -> V {
    V::String(value.into())
}
fn n(value: f64) -> V {
    V::number(value)
}
struct NoTally;
impl SourceBackend for NoTally {
    fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        Err(SourceRuleError::UnknownDependency)
    }
}
#[derive(Default)]
struct Scene {
    native: bool,
    whitelist: bool,
    empty: bool,
    huge: bool,
    procedural: bool,
}
fn fixture(scene: &Scene) -> EconomicState {
    let network = TradeLayerMainnet::Bitcoin;
    let mut ledger = Ledger::new();
    let huge = if scene.huge {
        9_007_199_254_740_992 * U
    } else {
        0
    };
    for property in [1, 2, 3, 4, 7] {
        ledger
            .apply(
                id(900 + property as u64),
                Operation::CreateProperty {
                    id: p(property),
                    issuer: a(if scene.empty { C } else { A }),
                    kind: if property == 7 {
                        PropertyKind::Fixed
                    } else {
                        PropertyKind::Managed
                    },
                    initial_atoms: if property == 7 {
                        100 * U + huge
                    } else {
                        20 * U
                    },
                },
            )
            .unwrap();
    }
    for property in [2, 3] {
        ledger
            .apply(
                id(920 + property as u64),
                Operation::Transfer {
                    id: p(property),
                    from: a(if scene.empty { C } else { A }),
                    to: a(B),
                    amount_atoms: 10 * U,
                },
            )
            .unwrap();
    }
    if huge != 0 {
        ledger
            .apply(
                id(930),
                Operation::Transfer {
                    id: p(7),
                    from: a(A),
                    to: a(B),
                    amount_atoms: huge,
                },
            )
            .unwrap();
    }
    let native = scene.native.then(|| NativeEntitlementPolicy {
        network,
        bootstrap: Tip {
            height: 0,
            id: network.genesis(),
        },
        bootstrap_ledger_commitment: tradelayer_core::native_entitlement::bootstrap_commitment(
            &ledger.snapshot_bytes().unwrap(),
        ),
        target_operator: a("operator"),
        assets: [
            NativeRole::Base,
            NativeRole::VestingBase,
            NativeRole::VestingIncome,
            NativeRole::Income,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, role)| NativeAssetSpec {
            role,
            property: p(index as u32 + 1),
            issuer: a(A),
            ledger_kind: PropertyKind::Managed,
            catalog_class: match role {
                NativeRole::Base => SourceCatalogClass::Fixed,
                NativeRole::Income => SourceCatalogClass::Native,
                _ => SourceCatalogClass::Vesting,
            },
            declared_figure_atoms: 20 * U,
            issuance_ceiling_atoms: None,
            expected_initial_issued_atoms: 20 * U,
        })
        .collect(),
        bindings: [(2, 1), (3, 4)]
            .into_iter()
            .map(|(coupon, target)| NativeBindingSpec {
                coupon: p(coupon),
                target: p(target),
                curve_scale_atoms: 20 * U,
                initial_markers: [B, A]
                    .into_iter()
                    .map(|owner| NativeMarkerRow {
                        owner: a(owner),
                        atoms: 10 * U,
                    })
                    .collect(),
                funding: PrefundedTargetSpec {
                    funding_owner: a(A),
                    target_atoms: 20 * U,
                },
            })
            .collect(),
        quote_policy: NativeQuotePolicy::SourceQuoteExactOrRejectV1,
        driver: NativeDriver::SourcePerCallV1,
        payout_order: NativePayoutOrder::AsciiAccountV1,
        coverage: NativeCoverage::CoverInitialMarkersV1,
    });
    let joint = match &native {
        Some(policy) => {
            ledger.adopt_native_entitlements_unverified(policy).unwrap();
            JointProtocolState::from_native_bootstrap_unverified(
                network,
                a("admin"),
                ledger,
                BTreeSet::from([p(7)]),
                policy,
            )
            .unwrap()
        }
        None => JointProtocolState::from_bootstrap_unverified(
            network,
            a("admin"),
            ledger,
            BTreeSet::from([p(7)]),
        )
        .unwrap(),
    };
    let mut properties = PropertyCatalog::new();
    for property in [1, 2, 3, 4, 7, 8] {
        let kind = if property == 7 && scene.procedural {
            "Procedural"
        } else if matches!(property, 2 | 3) {
            "Vesting"
        } else {
            "Fixed"
        };
        let mut args = vec![
            n(property as f64),
            s(&format!("P{property}")),
            n(100.0),
            s(kind),
            n(if property == 7 && scene.whitelist {
                5.0
            } else {
                0.0
            }),
            s(A),
            s(C),
        ];
        if property == 7 && scene.procedural {
            args.push(V::Object(vec![("proceduralType".into(), n(1.0))]));
        }
        assert_eq!(
            properties
                .execute("addProperty", &args, &mut NoTally)
                .unwrap()
                .outcome,
            SourceOutcome::Returned(V::Undefined)
        );
    }
    let owners = SourceCatalogOwners::new(properties, ClearlistSourceOwner::new());
    let config = EconomicConfig {
        network,
        contract: 1.try_into().unwrap(),
        collateral: p(7),
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
        native_entitlements: native,
        insurance: None,
        source_consensus: Some(SourceConsensusConfig {
            bootstrap_commitment: EconomicState::source_bootstrap_commitment(&joint).unwrap(),
            catalog_bootstrap_commitment: Some(catalog_commitment(
                &owners.checkpoint_bytes().unwrap(),
            )),
            policy: SourceConsensusPolicy::ConservingSequentialV1,
        }),
    };
    EconomicState::from_joint_with_source_catalogs_unverified(config, joint, owners).unwrap()
}
fn tx(position: u32, number: u64, body: &str, actor: &str) -> SourceTransaction {
    SourceTransaction {
        position,
        txid: id(number),
        payload: format!("2{body}"),
        marker: s("tl"),
        sender: s(actor),
        reference: V::Null,
        sender_amount: n(1.0),
        reference_amount: n(0.0),
    }
}
fn apply(state: &mut EconomicState, transactions: &[SourceTransaction]) -> SourceBlockReport {
    let last = state.source_consensus_state().frames.last();
    let height = last.map_or(1, |frame| frame.header.height + 1);
    let mut bytes = [0x7a; 32];
    bytes[24..].copy_from_slice(&height.to_be_bytes());
    let header = BlockHeader {
        height,
        id: BlockId(bytes),
        parent: last.map_or(state.config().network.genesis(), |frame| frame.header.id),
    };
    state
        .apply_source_block_unverified(header, SourceBlockMode::IndexedReplay, transactions)
        .unwrap()
}
fn activate(state: &mut EconomicState) {
    let mut command = tx(0, 1, "", "admin");
    command.payload = format!("02,{}", "11".repeat(32));
    assert_eq!(apply(state, &[command]).accepted, 1);
}
fn restart(state: &EconomicState) -> EconomicState {
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x0a");
    let reopened = EconomicState::from_snapshot_bytes(
        &bytes,
        state.config(),
        &a("admin"),
        &BTreeSet::from([p(7)]),
    )
    .unwrap();
    assert_eq!(&reopened, state);
    reopened
}
fn vectors() -> impl Iterator<Item = Vec<&'static str>> {
    include_str!("../fixtures/source_send_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("scene\t") && !line.is_empty())
        .map(|line| line.split('\t').collect())
}

#[test]
fn actual_scalar_and_native_sends_match_complete_composed_source_balances() {
    for row in vectors().filter(|row| {
        matches!(
            row[0],
            "scalar-whole"
                | "scalar-fraction"
                | "scalar-self"
                | "comma-four-fields-collapse-to-scalar"
                | "source-native2-coupled"
                | "source-native3-coupled"
        )
    }) {
        let native = row[0].starts_with("source-native");
        let property = if row[0].contains("native2") {
            2
        } else if row[0].contains("native3") {
            3
        } else {
            7
        };
        let mut state = fixture(&Scene {
            native,
            ..Scene::default()
        });
        activate(&mut state);
        let processed_before = state.ledger().processed_count();
        let command = tx(0, 10, row[1], A);
        let result = apply(&mut state, std::slice::from_ref(&command));
        assert_eq!(
            result.receipts[0].disposition,
            SourceDisposition::Applied,
            "{}: {:?}",
            row[0],
            result
        );
        for (owner, column, balance) in [(A, 4, false), (B, 5, false), (A, 6, true), (B, 7, true)] {
            let actual = if balance {
                state
                    .ledger()
                    .native_remaining_atoms(p(property), &a(owner))
            } else {
                state.ledger().available_atoms(p(property), &a(owner))
            };
            assert_eq!(
                actual,
                row[column].parse::<u128>().unwrap(),
                "{} column {column}",
                row[0]
            );
        }
        assert!(state.ledger().is_processed(command.txid));
        assert_eq!(state.ledger().processed_count(), processed_before + 1);
        state.verify().unwrap();
        state = restart(&state);
        assert_eq!(apply(&mut state, &[command]).already_processed, 1);
    }
}

#[test]
fn actual_decoder_validator_and_logic_failures_leave_ledger_untouched() {
    for row in vectors().filter(|row| {
        matches!(
            row[0],
            "source-send-all-missing-property"
                | "source-wire-multi-missing-address"
                | "source-insufficient"
                | "source-no-tally"
                | "source-clearlist-recipient-field-defect"
                | "source-inactive"
                | "source-native-subwhole-throws"
        )
    }) {
        let native = row[0] == "source-native-subwhole-throws";
        let mut state = fixture(&Scene {
            native,
            whitelist: row[0].contains("clearlist"),
            empty: row[0] == "source-no-tally",
            ..Scene::default()
        });
        if row[0] == "source-inactive" {
            apply(&mut state, &[]);
        } else {
            activate(&mut state);
        }
        let before = state.ledger().snapshot_bytes().unwrap();
        let command = tx(0, 10, row[1], A);
        let result = apply(&mut state, std::slice::from_ref(&command));
        let expected = if row[0].contains("wire-multi") || native {
            SourceDisposition::Thrown
        } else {
            SourceDisposition::Invalid
        };
        assert_eq!(
            result.receipts[0].disposition, expected,
            "{}: {:?}",
            row[0], result
        );
        if row[9] != "-" {
            assert!(
                result.receipts[0].reason.contains(row[9]),
                "{}: {}",
                row[0],
                result.receipts[0].reason
            );
        }
        if native {
            assert_eq!(result.receipts[0].reason, row[3]);
        }
        assert_eq!(
            state.ledger().snapshot_bytes().unwrap(),
            before,
            "{}",
            row[0]
        );
        assert!(!state.ledger().is_processed(command.txid));
        state = restart(&state);
        assert_eq!(apply(&mut state, &[command]).already_processed, 1);
    }
}

#[test]
fn sends_see_preceding_actual_source_custody_and_keep_original_ids() {
    let mut state = fixture(&Scene::default());
    activate(&mut state);
    let first = tx(0, 10, &format!("0;{B};7;a"), A);
    let second = tx(1, 11, &format!("0;{C};7;4"), B);
    let overspend = tx(2, 12, &format!("0;{C};7;7"), B);
    let fourth = tx(3, 13, &format!("0;{C};7;1"), B);
    let result = apply(&mut state, &[first, second, overspend, fourth]);
    assert_eq!(
        result
            .receipts
            .iter()
            .map(|r| r.disposition)
            .collect::<Vec<_>>(),
        vec![
            SourceDisposition::Applied,
            SourceDisposition::Applied,
            SourceDisposition::Invalid,
            SourceDisposition::Applied
        ]
    );
    assert_eq!(state.ledger().available_atoms(p(7), &a(A)), 90 * U);
    assert_eq!(state.ledger().available_atoms(p(7), &a(B)), 5 * U);
    assert_eq!(state.ledger().available_atoms(p(7), &a(C)), 5 * U);
    for number in [10, 11, 13] {
        assert!(state.ledger().is_processed(id(number)));
    }
    assert!(!state.ledger().is_processed(id(12)));
    restart(&state);
}

#[test]
fn actual_marker_owner_is_required_and_number_storage_loss_is_explicitly_unsupported() {
    let mut state = fixture(&Scene::default());
    activate(&mut state);
    let before = state.ledger().snapshot_bytes().unwrap();
    let result = apply(&mut state, &[tx(0, 10, &format!("0;{B};2;1"), A)]);
    assert_eq!(
        result.receipts[0].disposition,
        SourceDisposition::UnsupportedMapping
    );
    assert!(result.receipts[0]
        .reason
        .contains("actual native marker owner"));
    assert_eq!(state.ledger().snapshot_bytes().unwrap(), before);
    let row = vectors()
        .find(|row| row[0] == "source-recipient-number-loss")
        .unwrap();
    // This is observed source value loss, rather than a theoretical u128 cap.
    assert_eq!(row[2], "true");
    assert_eq!(row[5], "900719925474099200000000");
    let mut state = fixture(&Scene {
        huge: true,
        ..Scene::default()
    });
    activate(&mut state);
    let before = state.ledger().snapshot_bytes().unwrap();
    let result = apply(&mut state, &[tx(0, 10, row[1], A)]);
    assert_eq!(
        result.receipts[0].disposition,
        SourceDisposition::UnsupportedMapping
    );
    assert!(result.receipts[0].reason.contains("Number precision"));
    assert_eq!(state.ledger().snapshot_bytes().unwrap(), before);
    restart(&state);
}

#[test]
fn source_restricted_procedural_catalog_rejects_send_before_ledger_execution() {
    let mut state = fixture(&Scene {
        procedural: true,
        ..Scene::default()
    });
    activate(&mut state);
    let before = state.ledger().snapshot_bytes().unwrap();
    let result = apply(&mut state, &[tx(0, 10, &format!("0;{B};7;1"), A)]);
    assert_eq!(result.receipts[0].disposition, SourceDisposition::Invalid);
    assert!(result.receipts[0]
        .reason
        .contains("restricted procedural and non-transferable"));
    assert_eq!(state.ledger().snapshot_bytes().unwrap(), before);
    restart(&state);
}

#[test]
fn oracle_records_unrepaired_direct_logic_send_all_and_multi_failures() {
    for (name,error) in [
        ("direct-logic-send-all-shadow","TypeError: sendAll is not a function"),
        ("direct-logic-multi-unbound-processSend","ReferenceError: processSend is not defined"),
        ("direct-logic-multi-length-error","Error: Property IDs, amounts, and recipient addresses arrays must have the same length."),
    ] {
        let row = vectors().find(|row|row[0]==name).unwrap();
        assert_eq!(row[3],error);
        assert_eq!(row[8],"-");
    }
}
