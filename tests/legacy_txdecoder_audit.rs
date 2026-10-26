//! File6/49: pinned txDecoder.js, ten typed BODY layouts and no authority.
use tradelayer_core::legacy_txdecoder_audit::{
    source_decode_body_unverified, source_decode_property_id_unverified,
    source_standalone_abstraction_bridge_helper_unverified,
    source_type23_uses_later_base94_decoder, LegacyDecoderError, SourceChannelLocation,
    SourceDecoderCaveat, SourcePropertyId, UnverifiedSourceBody,
};

fn decode(
    ty: u8,
    body: &str,
) -> Result<tradelayer_core::legacy_txdecoder_audit::UnverifiedDecodedBody, LegacyDecoderError> {
    source_decode_body_unverified(ty, body)
}

#[test]
fn all_17_shared_source_vectors_have_intentional_strict_outcomes() {
    let lines = include_str!("../fixtures/legacy_txdecoder_body_vectors.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 17);
    for line in lines {
        let f = line.split('\t').collect::<Vec<_>>();
        assert_eq!(f.len(), 3);
        let ty = f[0].parse::<u8>().unwrap();
        let body = f[1];
        match f[2] {
            "commit-singleton-list" => {
                let obs = decode(ty, body).unwrap();
                assert_eq!(obs.type_id(), 4);
                match obs.source_fields() {
                    UnverifiedSourceBody::Commit {
                        property,
                        amount_atoms,
                        channel,
                        pay_enabled,
                        clear_lists,
                        colored_output,
                        commit_clearlist_id,
                    } => {
                        assert_eq!(*property, SourcePropertyId::Regular(7));
                        assert_eq!(*amount_atoms, 10);
                        assert_eq!(*channel, SourceChannelLocation::Address("channel1".into()));
                        assert!(*pay_enabled);
                        assert_eq!(clear_lists, &[7]);
                        assert!(!colored_output);
                        assert_eq!(*commit_clearlist_id, Some(11));
                    }
                    _ => panic!("source Type4 commit expected"),
                }
                assert!(obs
                    .caveats()
                    .contains(&SourceDecoderCaveat::CommitBracketedClearlistCommasMisparse));
            }
            "commit-synthetic-raw-ref" => {
                let obs = decode(ty, body).unwrap();
                match obs.source_fields() {
                    UnverifiedSourceBody::Commit {
                        property,
                        amount_atoms,
                        channel,
                        clear_lists,
                        colored_output,
                        commit_clearlist_id,
                        pay_enabled,
                    } => {
                        assert_eq!(
                            *property,
                            SourcePropertyId::Synthetic {
                                collateral_id: 1,
                                contract_id: 10,
                            }
                        );
                        assert_eq!(*amount_atoms, 2);
                        assert_eq!(*channel, SourceChannelLocation::RawReference("04".into()));
                        assert_eq!(clear_lists, &[11]);
                        assert!(*colored_output);
                        assert!(!pay_enabled);
                        assert_eq!(*commit_clearlist_id, Some(2));
                    }
                    _ => panic!("source synthetic Type4 expected"),
                }
            }
            "commit-comma-bracket-misparse" => {
                assert_eq!(
                    decode(ty, body),
                    Err(LegacyDecoderError::BracketedListSplitBeforeParsing)
                );
            }
            "commit-missing-channel-throws" => {
                assert_eq!(
                    decode(ty, body),
                    Err(LegacyDecoderError::MissingIndexedFieldWouldThrow(
                        "commit channel field"
                    ))
                );
            }
            "whitelist-four-fields" => {
                let x = decode(ty, body).unwrap();
                assert_eq!(
                    x.source_fields(),
                    &UnverifiedSourceBody::CreateWhitelist {
                        backup_address: "backupB".into(),
                        name: "ListOne".into(),
                        url: "url".into(),
                        description: "desc".into(),
                    }
                );
            }
            "admin-base36-and-backup" => {
                let x = decode(ty, body).unwrap();
                assert_eq!(
                    x.source_fields(),
                    &UnverifiedSourceBody::UpdateAdmin {
                        new_address: "adminC".into(),
                        whitelist: true,
                        oracle: false,
                        token: false,
                        list_id: 11,
                        update_backup: true,
                    }
                );
            }
            "attestation-revoke-uppercase" => {
                let x = decode(ty, body).unwrap();
                assert_eq!(
                    x.source_fields(),
                    &UnverifiedSourceBody::Attestation {
                        revoke: true,
                        list_id: 11,
                        target_address: "alice".into(),
                        metadata_upper: "OK".into(),
                    }
                );
                assert!(x
                    .caveats()
                    .contains(&SourceDecoderCaveat::AttestationMissingMetadataThrows));
                assert_eq!(
                    x.require_accepted_registry_transition(),
                    Err(LegacyDecoderError::RegistryAndActivationUnverified)
                );
            }
            "attestation-missing-meta-throws" => {
                assert_eq!(
                    decode(ty, body),
                    Err(LegacyDecoderError::MissingAttestationMetadataWouldThrow)
                );
            }
            "attestation-post-marker-selector-is-not-revoke" => {
                let x = decode(ty, body).unwrap();
                assert_eq!(
                    x.source_fields(),
                    &UnverifiedSourceBody::Attestation {
                        revoke: false,
                        list_id: 7,
                        target_address: "alice".into(),
                        metadata_upper: "OK".into(),
                    }
                );
            }
            "close-oracle-primitive" => {
                let x = decode(ty, body).unwrap();
                assert_eq!(
                    x.source_fields(),
                    &UnverifiedSourceBody::CloseOraclePrimitive(15)
                );
                assert_eq!(
                    x.require_historically_accepted_transaction(),
                    Err(LegacyDecoderError::HistoricalTransactionNotAuthenticated)
                );
            }
            "withdrawal-decimal-ref" => {
                let x = decode(ty, body).unwrap();
                assert_eq!(
                    x.source_fields(),
                    &UnverifiedSourceBody::Withdrawal {
                        withdraw_all: true,
                        property: SourcePropertyId::Regular(7),
                        amount_atoms: 10,
                        column_flag: true,
                        channel: SourceChannelLocation::WithdrawalReference(Some(10)),
                    }
                );
            }
            "withdrawal-bad-ref-false" => {
                let x = decode(ty, body).unwrap();
                assert_eq!(
                    x.source_fields(),
                    &UnverifiedSourceBody::Withdrawal {
                        withdraw_all: false,
                        property: SourcePropertyId::Synthetic {
                            collateral_id: 1,
                            contract_id: 10,
                        },
                        amount_atoms: 2,
                        column_flag: false,
                        channel: SourceChannelLocation::WithdrawalReference(None),
                    }
                );
            }
            "transfer-text-ref" => {
                let x = decode(ty, body).unwrap();
                assert_eq!(
                    x.source_fields(),
                    &UnverifiedSourceBody::Transfer {
                        property: SourcePropertyId::Regular(7),
                        amount_atoms: 10,
                        column_a: true,
                        destination: SourceChannelLocation::RawReference("0".into()),
                        pay: true,
                        pay_reference: "payRef".into(),
                    }
                );
            }
            "transfer-missing-destination-throws" => {
                assert_eq!(
                    decode(ty, body),
                    Err(LegacyDecoderError::MissingIndexedFieldWouldThrow(
                        "transfer destination"
                    ))
                );
            }
            "stake-fraud-all-fields" => {
                let x = decode(ty, body).unwrap();
                assert_eq!(
                    x.source_fields(),
                    &UnverifiedSourceBody::StakeFraudRelay {
                        action: 1,
                        oracle_id: 11,
                        staked_property_id: 7,
                        amount_atoms: 10,
                        accused_address: "alice".into(),
                        evidence_hash: "hash".into(),
                        relay_type: 2,
                        state_hash: "state".into(),
                        dlc_ref: "ref".into(),
                        settlement_state: "settle".into(),
                        relay_blob: "blob".into(),
                        auto_roll: true,
                        next_dlc_ref: "next".into(),
                    }
                );
            }
            "zk-batch-opaque-not-validated" => {
                let x = decode(ty, body).unwrap();
                assert_eq!(
                    x.source_fields(),
                    &UnverifiedSourceBody::ZkBatchOpaque {
                        ordinal_reveal_json: "{\"a\":1}".into(),
                    }
                );
                assert!(x
                    .caveats()
                    .contains(&SourceDecoderCaveat::Types32And33ValidatorCommentedOut));
                assert_eq!(
                    x.require_trusted_execution(),
                    Err(LegacyDecoderError::TrustedEconomicExecutionForbidden)
                );
            }
            "colored-decimal-action-base36-ratio" => {
                let x = decode(ty, body).unwrap();
                assert_eq!(
                    x.source_fields(),
                    &UnverifiedSourceBody::ColoredCoin {
                        action_decimal: 2,
                        property_id: 10,
                        sats_ratio: 36,
                        home_address: "home".into(),
                    }
                );
                assert!(x
                    .caveats()
                    .contains(&SourceDecoderCaveat::Types32And33ValidatorCommentedOut));
            }
            other => panic!("unhandled pinned source fixture {other}"),
        }
    }
}

#[test]
fn source_property_synthetic_id_is_parsed_in_two_legacy_forms() {
    let expected = SourcePropertyId::Synthetic {
        collateral_id: 1,
        contract_id: 10,
    };
    assert_eq!(
        source_decode_property_id_unverified("s-1-a"),
        Ok(expected.clone())
    );
    assert_eq!(source_decode_property_id_unverified("s1-a"), Ok(expected));
    assert_eq!(
        source_decode_property_id_unverified("z"),
        Ok(SourcePropertyId::Regular(35))
    );
    for malformed in ["s", "s-", "s-1", "s--", "s-1-a-b", "s1-a-1"] {
        assert!(
            source_decode_property_id_unverified(malformed).is_err(),
            "{malformed}"
        );
    }
    assert!(source_decode_property_id_unverified("").is_err());
    assert!(source_decode_property_id_unverified("s-1-?").is_err());
}

#[test]
fn no_implicit_marker_stripping_unicode_or_numeric_compatibility_fallback() {
    assert_eq!(
        decode(9, "tl91,7,alice,ok"),
        Err(LegacyDecoderError::NestedOrFramedBody)
    );
    assert_eq!(
        decode(32, "x\nbad"),
        Err(LegacyDecoderError::NonPrintableSourceText)
    );
    assert_eq!(
        decode(32, "☃"),
        Err(LegacyDecoderError::NonPrintableSourceText)
    );
    assert_eq!(
        decode(32, &"x".repeat(4097)),
        Err(LegacyDecoderError::BodyTooLong)
    );
    assert_eq!(
        decode(9, "1,not_base36!,alice,OK"),
        Err(LegacyDecoderError::NonCanonicalLegacyNumeric(
            "attestation list id"
        ))
    );
    assert_eq!(
        decode(4, "7,zzzzzzzzzzzzzz,chan,1,,0"),
        Err(LegacyDecoderError::NonCanonicalLegacyNumeric("amount"))
    );
    assert_eq!(
        decode(4, "7,2,chan,0,[7,b],0"),
        Err(LegacyDecoderError::BracketedListSplitBeforeParsing)
    );
    assert_eq!(
        decode(7, "a,b,c,d\n"),
        Err(LegacyDecoderError::NonPrintableSourceText)
    );
}

#[test]
fn absent_source_decoder_exports_are_never_silently_aliased() {
    for ty in [28, 29, 34, 35] {
        assert_eq!(
            decode(ty, "a,b"),
            Err(LegacyDecoderError::MissingSourceDecoderExport(ty))
        );
    }
    assert_eq!(
        decode(0, "0,hash"),
        Err(LegacyDecoderError::UnsupportedOrUnportedLayout(0))
    );
    assert_eq!(
        decode(23, "a,b"),
        Err(LegacyDecoderError::UnsupportedOrUnportedLayout(23))
    );
    assert!(source_type23_uses_later_base94_decoder());
    // Standalone helper exists but Types source calls a DIFFERENT name.
    let bridge =
        source_standalone_abstraction_bridge_helper_unverified("a,b,ignored-destination").unwrap();
    assert_eq!(bridge.property_id(), 10);
    assert_eq!(bridge.amount(), 11);
    assert_eq!(
        bridge.require_trusted_cross_layer_bridge(),
        Err(LegacyDecoderError::TrustedEconomicExecutionForbidden)
    );
    // No Type34 route is enabled merely by the helper's existence.
    assert_eq!(
        decode(34, "a,b,ignored-destination"),
        Err(LegacyDecoderError::MissingSourceDecoderExport(34))
    );
}

#[test]
fn no_record_can_authorize_ledger_clearlist_channel_or_economics() {
    for (ty, body) in [
        (4, "7,a,channel1,1,[7],0,b"),
        (7, "backupB,Name,URL,desc"),
        (8, "adminC,1,0,0,b,1"),
        (9, "1,b,alice,OK"),
        (15, "f"),
        (21, "1,7,a,1,ref:10"),
        (22, "7,a,1,ref:0,1,payRef"),
        (30, "1,b,7,a,alice,hash,2,state,ref,settle,blob,1,next"),
        (32, "{\"a\":1}"),
        (33, "2,a,10,home"),
    ] {
        let observation = decode(ty, body).unwrap();
        assert_eq!(observation.type_id(), ty);
        assert_eq!(
            observation.require_historically_accepted_transaction(),
            Err(LegacyDecoderError::HistoricalTransactionNotAuthenticated)
        );
        assert_eq!(
            observation.require_accepted_registry_transition(),
            Err(LegacyDecoderError::RegistryAndActivationUnverified)
        );
        assert_eq!(
            observation.require_canonical_signed_channel_revision(),
            Err(LegacyDecoderError::CanonicalSignedChannelRevisionAbsent)
        );
        assert_eq!(
            observation.require_trusted_execution(),
            Err(LegacyDecoderError::TrustedEconomicExecutionForbidden)
        );
    }
}

use tradelayer_core::legacy_txdecoder_audit::{
    source_decode_historical_method_unverified, source_decode_historical_type_unverified,
    HistoricalDecoderError, HistoricalValue, HISTORICAL_DECODER_METHODS,
    MAX_HISTORICAL_OUTPUT_BYTES,
};

fn historical_hex(raw: &str) -> String {
    if raw == "-" {
        return String::new();
    }
    assert_eq!(raw.len() % 2, 0);
    String::from_utf8(
        raw.as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}

#[test]
fn complete_historical_source_corpus_matches_every_active_method() {
    let mut covered = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    let mut count = 0;
    for line in include_str!("../fixtures/legacy_txdecoder_historical_vectors.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let method = fields[0];
        let body = historical_hex(fields[2]);
        covered.insert(method);
        count += 1;
        let actual = source_decode_historical_method_unverified(method, &body);
        let mismatch = match fields[3] {
            "ok" => {
                let expected = historical_hex(fields[4]);
                match actual {
                    Ok(observation) => match observation.diagnostic_json() {
                        Ok(json) if json == expected => None,
                        other => Some(format!("expected {expected}; actual {other:?}")),
                    },
                    Err(error) => Some(format!("expected {expected}; error {error:?}")),
                }
            }
            "throw" => {
                let message = historical_hex(fields[5]);
                match actual {
                    Err(HistoricalDecoderError::SourceThrow {
                        name,
                        message: actual_message,
                    }) if name == fields[4] && message == actual_message => None,
                    other => Some(format!(
                        "expected {} {message}; actual {other:?}",
                        fields[4]
                    )),
                }
            }
            other => panic!("unknown source result {other}"),
        };
        if let Some(mismatch) = mismatch {
            failures.push(format!("{method}/{} body={body:?}: {mismatch}", fields[1]));
        }
    }
    assert_eq!(count, 1447);
    assert_eq!(
        covered,
        HISTORICAL_DECODER_METHODS.iter().copied().collect()
    );
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn historical_values_preserve_missing_nonfinite_and_signed_zero() {
    let missing =
        source_decode_historical_method_unverified("decodeActivateTradeLayer", "").unwrap();
    assert_eq!(
        missing.value().get("codeHash"),
        Some(&HistoricalValue::Undefined)
    );
    assert!(missing.diagnostic_json().unwrap().contains("NaN"));
    let zero = source_decode_historical_method_unverified("decodeAmount", "-0~").unwrap();
    let HistoricalValue::Number(zero) = zero.value() else {
        panic!("number expected")
    };
    assert_eq!(zero.as_f64().to_bits(), (-0.0_f64).to_bits());
    let malformed =
        source_decode_historical_method_unverified("decodeCommitToken", "7,a,chan,1,[7,b],1,2")
            .unwrap();
    assert!(malformed.diagnostic_json().unwrap().contains("NaN"));
    assert_eq!(
        decode(4, "7,a,chan,1,[7,b],1,2"),
        Err(LegacyDecoderError::BracketedListSplitBeforeParsing)
    );
    for text in ["Infinity", "-Infinity"] {
        assert!(source_decode_historical_method_unverified(
            "decodeTradeTokenForUTXO",
            &format!("7,{text},0,0")
        )
        .unwrap()
        .diagnostic_json()
        .unwrap()
        .contains(text));
    }
}

#[test]
fn historical_dispatch_is_complete_without_aliases_or_implicit_frame_stripping() {
    for ty in (0..=27).chain(30..=33) {
        let result = source_decode_historical_type_unverified(ty, "");
        assert!(
            !matches!(result, Err(HistoricalDecoderError::MissingMethod(_))),
            "{ty}"
        );
    }
    for ty in [28, 29, 34, 35, 36, 255] {
        assert!(matches!(
            source_decode_historical_type_unverified(ty, ""),
            Err(HistoricalDecoderError::MissingMethod(_))
        ));
    }
    assert!(source_decode_historical_method_unverified("decodeAbstractionBridge", "a,b").is_ok());
    assert!(matches!(
        source_decode_historical_method_unverified("decodeCrossLayerBridge", "a,b"),
        Err(HistoricalDecoderError::MissingMethod(_))
    ));
    let opaque =
        source_decode_historical_method_unverified("decodePublishNewTx", "tl32☃\u{0000}\n")
            .unwrap();
    assert_eq!(
        opaque.value().get("ordinalRevealJSON"),
        Some(&HistoricalValue::String("tl32☃\u{0000}\n".into()))
    );
    let framed = source_decode_historical_method_unverified(
        "decodeIssueOrRevokeAttestation",
        "tl91,7,alice,ok",
    )
    .unwrap();
    assert_eq!(
        framed.value().get("revoke"),
        Some(&HistoricalValue::Bool(false))
    );
}

#[test]
fn historical_malformed_and_shadowed_paths_are_observations_not_repairs() {
    let decode = |method, body| source_decode_historical_method_unverified(method, body).unwrap();
    assert_eq!(
        decode("decodeSend", "0;alice;7").value(),
        &HistoricalValue::Undefined
    );
    let send = decode("decodeSend", "0;alice;5,7;a,b;1;1");
    assert_eq!(send.value().get("address"), None);
    let cancel = decode("decodeCancelOrder", "0,7,b,8,99,1");
    assert_eq!(
        cancel.value().get("isContract"),
        Some(&HistoricalValue::Bool(false))
    );
    let HistoricalValue::Number(price) = cancel
        .value()
        .get("cancelParams")
        .unwrap()
        .get("price")
        .unwrap()
    else {
        panic!("price expected")
    };
    assert_eq!(price.as_f64(), 1.0); // Source reads cancelAll (8), then divides by8.
    let mint = decode("decodeMintSynthetic", "s-1-a,1,a");
    let redeem = decode("decodeRedeemSynthetic", "s-1-a,1,a");
    assert_eq!(
        mint.value().get("propertyId"),
        Some(&HistoricalValue::String("s-1-10".into()))
    );
    let HistoricalValue::Number(property) = redeem.value().get("propertyId").unwrap() else {
        panic!("number expected")
    };
    assert_eq!(property.as_f64(), 28.0);
    let settle = decode("decodeSettleChannelPNL", "");
    let HistoricalValue::Number(mark) = settle.value().get("markPrice").unwrap() else {
        panic!("number expected")
    };
    assert_eq!(mark.as_f64(), 15.0); // Base94 '0' is digit15.
    assert_eq!(settle.value().get("close"), None);
    assert_eq!(
        settle.value().get("blockStart"),
        Some(&HistoricalValue::Undefined)
    );
}

#[test]
fn historical_resource_errors_precede_output_clones_and_authority_stays_absent() {
    assert_eq!(
        source_decode_historical_method_unverified(&"x".repeat(129), ""),
        Err(HistoricalDecoderError::Capacity)
    );
    let nested_radix = format!("0,7,b,{}1,99,1", "0x".repeat(34));
    assert_eq!(
        source_decode_historical_method_unverified("decodeCancelOrder", &nested_radix),
        Err(HistoricalDecoderError::Capacity)
    );
    for body in ["x".repeat(4097), "☃".repeat(1366)] {
        assert_eq!(
            source_decode_historical_method_unverified("decodePublishNewTx", &body),
            Err(HistoricalDecoderError::Capacity)
        );
    }
    assert!(
        source_decode_historical_method_unverified("decodePublishNewTx", &"x".repeat(4096)).is_ok()
    );
    let mut value = HistoricalValue::Null;
    for _ in 0..34 {
        value = HistoricalValue::Array(vec![value]);
    }
    assert_eq!(
        value.diagnostic_json(),
        Err(HistoricalDecoderError::Capacity)
    );
    assert_eq!(
        HistoricalValue::String("x".repeat(MAX_HISTORICAL_OUTPUT_BYTES + 1)).diagnostic_json(),
        Err(HistoricalDecoderError::Capacity)
    );
    let many = HistoricalValue::Array(vec![HistoricalValue::Null; 32769]);
    assert_eq!(
        many.diagnostic_json(),
        Err(HistoricalDecoderError::Capacity)
    );
    for method in HISTORICAL_DECODER_METHODS {
        if let Ok(observation) = source_decode_historical_method_unverified(method, "") {
            assert_eq!(
                observation.require_trusted_execution(),
                Err(LegacyDecoderError::TrustedEconomicExecutionForbidden)
            );
            assert_eq!(
                observation.require_historically_accepted_transaction(),
                Err(LegacyDecoderError::HistoricalTransactionNotAuthenticated)
            );
        }
    }
}
