use tradelayer_core::bitcoin_p2wsh_verify::*;
use tradelayer_core::channel_bip143_audit::audit_native_p2wsh_bip143_sighash_all_unverified;
use tradelayer_core::channel_witness_audit::{ClaimedTwoOfTwoPubkeys, StructuralChannelError};
use tradelayer_core::network_profile::TradeLayerMainnet;

const FIXTURE: &str = include_str!("../fixtures/bitcoin_p2wsh_verify.tsv");
fn bytes(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(core::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn row(name: &str) -> Vec<&str> {
    FIXTURE
        .lines()
        .find(|line| line.starts_with(&format!("{name}\t")))
        .unwrap()
        .split('\t')
        .collect()
}
fn error_name(error: &BitcoinTwoOfTwoError) -> String {
    match error {
        BitcoinTwoOfTwoError::Structural(value) => format!("{value:?}"),
        BitcoinTwoOfTwoError::InvalidSignature { slot } => format!("InvalidSignature{slot}"),
        BitcoinTwoOfTwoError::InvalidPublicKey { slot } => format!("InvalidPublicKey{slot}"),
        other => format!("{other:?}"),
    }
}

#[test]
fn actual_core_reference_corpus_and_explicit_supported_subset() {
    let mut count = 0;
    let mut accepted = 0;
    let mut core_accepted = 0;
    for line in FIXTURE.lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 10);
        let funding = bytes(row[4]);
        let spending = bytes(row[5]);
        let result = verify_bitcoin_native_two_of_two_input(
            row[3].parse().unwrap(),
            &funding,
            row[1].parse().unwrap(),
            &spending,
            row[2].parse().unwrap(),
        );
        match result {
            Ok(checked) => {
                assert_eq!(row[8], "OK", "{} unexpectedly accepted", row[0]);
                assert_eq!(row[9], "true", "{} disagrees with genuine Core", row[0]);
                assert_eq!(checked.raw_funding_transaction(), funding);
                assert_eq!(checked.raw_funding_transaction().as_ptr(), funding.as_ptr());
                assert_eq!(checked.raw_spending_transaction(), spending);
                assert_eq!(
                    checked.raw_spending_transaction().as_ptr(),
                    spending.as_ptr()
                );
                assert_eq!(checked.funding_satoshis(), row[6].parse().unwrap());
                assert_eq!(checked.funding_vout(), row[1].parse().unwrap());
                assert_eq!(checked.spending_vin(), row[2].parse().unwrap());
                assert_ne!(
                    checked.actual_script_keys()[0],
                    checked.actual_script_keys()[1]
                );
                assert_eq!(
                    checked.require_authenticated_parent_utxo(),
                    Err(BitcoinTwoOfTwoTrustError::FundingInclusionAndUnspentnessUnauthenticated)
                );
                assert_eq!(
                    checked.require_complete_transaction_consensus(),
                    Err(BitcoinTwoOfTwoTrustError::CompleteTransactionAndChainConsensusUnverified)
                );
                assert_eq!(
                    checked.require_tradelayer_actor_admission(),
                    Err(BitcoinTwoOfTwoTrustError::TradeLayerActorAndEconomicAdmissionUnavailable)
                );
                accepted += 1;
            }
            Err(error) => assert_eq!(error_name(&error), row[8], "{}: {error:?}", row[0]),
        }
        if row[9] == "true" {
            core_accepted += 1;
        }
        count += 1;
    }
    assert_eq!((count, accepted, core_accepted), (57, 10, 20));
}

#[test]
fn real_signature_check_closes_the_preexisting_digest_only_gap() {
    let positive = row("single-low-s");
    let funding = bytes(positive[4]);
    let spending = bytes(positive[5]);
    let checked =
        verify_bitcoin_native_two_of_two_input(BITCOIN_TWO_OF_TWO_FLAGS, &funding, 0, &spending, 0)
            .unwrap();
    let keys = checked.actual_script_keys();
    for name in [
        "first-signature-wrong",
        "second-signature-wrong",
        "signed-with-wrong-amount",
    ] {
        let negative = row(name);
        let mutated = bytes(negative[5]);
        let old_digest = audit_native_p2wsh_bip143_sighash_all_unverified(
            TradeLayerMainnet::Bitcoin,
            &funding,
            0,
            &mutated,
            0,
            ClaimedTwoOfTwoPubkeys {
                participant_a: keys[0],
                participant_b: keys[1],
            },
        )
        .unwrap();
        assert_eq!(
            old_digest.require_verified_channel_signatures(),
            Err(StructuralChannelError::SignaturesNotVerified)
        );
        assert_eq!(old_digest.digest_raw(), checked.digest_raw());
        assert!(
            matches!(
                verify_bitcoin_native_two_of_two_input(
                    BITCOIN_TWO_OF_TWO_FLAGS,
                    &funding,
                    0,
                    &mutated,
                    0
                ),
                Err(BitcoinTwoOfTwoError::InvalidSignature { .. })
            ),
            "{name}"
        );
    }
}

#[test]
fn exact_signed_fixture_digest_high_s_and_other_input_scope_remain_explicit() {
    let original = row("existing-genuine-high-s");
    let funding = bytes(original[4]);
    let spending = bytes(original[5]);
    let checked =
        verify_bitcoin_native_two_of_two_input(BITCOIN_TWO_OF_TWO_FLAGS, &funding, 0, &spending, 1)
            .unwrap();
    assert_eq!(
        checked.digest_raw(),
        bytes("35e95e9e4200ab42a0bb2470a1528f4c32c898800b49dba080d551855ed4e4f0").as_slice()
    );
    // Core's selected-input API and this fixed template do not verify vin0.
    let scoped = row("other-input-invalid-not-verified");
    let f = bytes(scoped[4]);
    let s = bytes(scoped[5]);
    let selected =
        verify_bitcoin_native_two_of_two_input(BITCOIN_TWO_OF_TWO_FLAGS, &f, 0, &s, 1).unwrap();
    assert!(selected.require_complete_transaction_consensus().is_err());
    for cut in [0, 4, spending.len() - 1] {
        assert!(verify_bitcoin_native_two_of_two_input(
            BITCOIN_TWO_OF_TWO_FLAGS,
            &funding,
            0,
            &spending[..cut],
            1
        )
        .is_err());
    }
    let mut trailing = spending.clone();
    trailing.push(0);
    assert!(verify_bitcoin_native_two_of_two_input(
        BITCOIN_TWO_OF_TWO_FLAGS,
        &funding,
        0,
        &trailing,
        1
    )
    .is_err());
    assert!(verify_bitcoin_native_two_of_two_input(
        BITCOIN_TWO_OF_TWO_FLAGS,
        &funding[..funding.len() - 1],
        0,
        &spending,
        1
    )
    .is_err());
    assert!(verify_bitcoin_native_two_of_two_input(
        BITCOIN_TWO_OF_TWO_FLAGS,
        &funding,
        0,
        &spending,
        u32::MAX
    )
    .is_err());
}
