use tradelayer_core::bitcoin_standard_input::*;
const FIXTURE: &str = include_str!("../fixtures/bitcoin_standard_input.tsv");
fn bytes(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(core::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn row(name: &str) -> Vec<&str> {
    FIXTURE
        .lines()
        .find(|l| l.starts_with(&format!("{name}\t")))
        .unwrap()
        .split('\t')
        .collect()
}
fn error(error: &BitcoinInputError) -> String {
    match error {
        BitcoinInputError::InvalidSignature { slot } => format!("InvalidSignature{slot}"),
        BitcoinInputError::InvalidPublicKey { slot } => format!("InvalidPublicKey{slot}"),
        other => format!("{other:?}"),
    }
}
#[test]
fn core_reference_templates_all_six_modes_and_adversarial_corpus() {
    let (mut count, mut accepted, mut core_accepted) = (0, 0, 0);
    let mut coverage = std::collections::BTreeSet::new();
    for line in FIXTURE.lines().filter(|l| !l.starts_with('#')) {
        let r: Vec<_> = line.split('\t').collect();
        assert_eq!(r.len(), 15);
        let funding = bytes(r[4]);
        let spending = bytes(r[5]);
        match check_bitcoin_standard_input(
            r[3].parse().unwrap(),
            &funding,
            r[1].parse().unwrap(),
            &spending,
            r[2].parse().unwrap(),
        ) {
            Ok(checked) => {
                assert_eq!(r[8], "OK", "{} unexpectedly accepted", r[0]);
                assert_eq!(r[9], "true", "{} disagrees with genuine Core", r[0]);
                assert_eq!(format!("{:?}", checked.template()), r[10]);
                assert_eq!(checked.raw_funding_transaction().as_ptr(), funding.as_ptr());
                assert_eq!(
                    checked.raw_spending_transaction().as_ptr(),
                    spending.as_ptr()
                );
                assert_eq!(checked.funding_vout(), r[1].parse().unwrap());
                assert_eq!(checked.spending_vin(), r[2].parse().unwrap());
                assert_eq!(checked.funding_satoshis(), r[6].parse().unwrap());
                assert_eq!(checked.witness_amount_is_signed(), r[14] == "true");
                let modes: Vec<u8> = r[11].split(',').map(|x| x.parse().unwrap()).collect();
                let digests: Vec<_> = r[12].split(',').map(bytes).collect();
                let bugs: Vec<_> = r[13].split(',').map(|x| x == "true").collect();
                assert_eq!(checked.signatures().len(), modes.len());
                assert_eq!(checked.actual_script_keys().len(), modes.len());
                for (index, signature) in checked.signatures().iter().enumerate() {
                    assert_eq!(signature.hash_type, modes[index]);
                    assert_eq!(signature.digest_raw.as_slice(), digests[index]);
                    assert_eq!(signature.legacy_single_bug, bugs[index]);
                    coverage.insert((r[10], signature.hash_type));
                }
                assert_eq!(
                    checked.require_authenticated_parent_utxo(),
                    Err(BitcoinInputTrustError::FundingInclusionAndUnspentnessUnauthenticated)
                );
                assert_eq!(
                    checked.require_complete_transaction_consensus(),
                    Err(BitcoinInputTrustError::CompleteTransactionAndChainConsensusUnverified)
                );
                assert_eq!(
                    checked.require_tradelayer_actor_admission(),
                    Err(BitcoinInputTrustError::TradeLayerActorAndEconomicAdmissionUnavailable)
                );
                accepted += 1;
            }
            Err(e) => assert_eq!(error(&e), r[8], "{}: {e:?}", r[0]),
        }
        if r[9] == "true" {
            core_accepted += 1;
        }
        count += 1;
    }
    assert_eq!((count, accepted, core_accepted), (212, 78, 100));
    assert_eq!(coverage.len(), 36);
}
#[test]
fn weak_hash_modes_retain_actual_commitment_limits_and_never_grant_actor_admission() {
    for name in [
        "legacy-single-bug-output-can-change",
        "witness-none-output-can-change",
        "witness-anyonecanpay-other-input-can-change",
    ] {
        let r = row(name);
        let funding = bytes(r[4]);
        let spending = bytes(r[5]);
        let receipt = check_bitcoin_standard_input(
            BITCOIN_STANDARD_INPUT_FLAGS,
            &funding,
            0,
            &spending,
            r[2].parse().unwrap(),
        )
        .unwrap();
        assert!(receipt.require_tradelayer_actor_admission().is_err());
        assert!(receipt.require_authenticated_parent_utxo().is_err());
        if name.starts_with("legacy-single") {
            let mut one = [0; 32];
            one[0] = 1;
            assert_eq!(receipt.signatures()[0].digest_raw, one);
            assert!(receipt.signatures()[0].legacy_single_bug);
            assert!(!receipt.witness_amount_is_signed());
        }
    }
}
#[test]
fn actual_pinned_builder_scripts_and_malformed_boundaries() {
    for (name, script) in [
        (
            "p2pkh-mode-1",
            "76a914751e76e8199196d454941c45d1b3a323f1433bd688ac",
        ),
        (
            "p2wpkh-mode-1",
            "0014751e76e8199196d454941c45d1b3a323f1433bd6",
        ),
        (
            "p2sh-mode-1",
            "a91412fcac201d73f5b5dba0f1f22c40f02da17bb4a487",
        ),
        (
            "p2wsh-mode-1",
            "00209b984c7bae3efddc3a3f0a20ff81bfe89ed1fe07ff13e562149ee654bed845db",
        ),
        (
            "p2wsh-reversed-actual-key-order",
            "002006538ec0f5c9bbb294ee4740fbbe753ba06f4922b439f4fae52a426becbcbfba",
        ),
    ] {
        assert_eq!(
            row(name)[7],
            script,
            "unchanged builder source/genuine library script"
        );
    }
    let r = row("wrapped-p2wsh-mode-1");
    let funding = bytes(r[4]);
    let spending = bytes(r[5]);
    for cut in [0, 4, spending.len() - 1] {
        assert!(check_bitcoin_standard_input(
            BITCOIN_STANDARD_INPUT_FLAGS,
            &funding,
            0,
            &spending[..cut],
            1
        )
        .is_err());
    }
    let mut trailing = spending.clone();
    trailing.push(0);
    assert!(
        check_bitcoin_standard_input(BITCOIN_STANDARD_INPUT_FLAGS, &funding, 0, &trailing, 1)
            .is_err()
    );
    let mut flag = spending.clone();
    flag[5] = 3;
    assert!(
        check_bitcoin_standard_input(BITCOIN_STANDARD_INPUT_FLAGS, &funding, 0, &flag, 1).is_err()
    );
    assert!(check_bitcoin_standard_input(
        BITCOIN_STANDARD_INPUT_FLAGS,
        &funding[..funding.len() - 1],
        0,
        &spending,
        1
    )
    .is_err());
    assert!(check_bitcoin_standard_input(
        BITCOIN_STANDARD_INPUT_FLAGS,
        &funding,
        0,
        &spending,
        u32::MAX
    )
    .is_err());
}

#[test]
fn raw_funding_identity_is_required_even_when_legacy_scripts_ignore_amount() {
    let r = row("p2pkh-funding-amount-changed");
    assert_eq!(
        r[9], "true",
        "Core's selected legacy input API ignores the supplied amount"
    );
    let funding = bytes(r[4]);
    let spending = bytes(r[5]);
    assert!(
        matches!(
            check_bitcoin_standard_input(BITCOIN_STANDARD_INPUT_FLAGS, &funding, 0, &spending, 1),
            Err(BitcoinInputError::WrongPrevout)
        ),
        "inventing different raw funding bytes cannot reuse the old outpoint signature"
    );
}
