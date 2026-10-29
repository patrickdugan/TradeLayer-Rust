use tradelayer_core::bitcoin_standard_input::{
    BitcoinInputError, BitcoinInputTrustError, BITCOIN_STANDARD_INPUT_FLAGS,
};
use tradelayer_core::bitcoin_supported_transaction::*;
use tradelayer_core::bitcoin_witness_binding::{BitcoinWitnessRules, ExpectedWitnessBlock};
use tradelayer_core::chain::{BlockId, Tip};
use tradelayer_core::network_profile::TradeLayerMainnet;

const VECTORS: &str = include_str!("../fixtures/bitcoin_supported_transaction.tsv");
fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn row(name: &str) -> Vec<&'static str> {
    VECTORS
        .lines()
        .find(|l| l.starts_with(&format!("{name}\t")))
        .unwrap()
        .split('\t')
        .collect()
}
fn expected(r: &[&str]) -> ExpectedWitnessBlock {
    ExpectedWitnessBlock {
        tip: Tip {
            height: 42,
            id: BlockId(bytes(r[3]).try_into().unwrap()),
        },
        parent: BlockId([0x33; 32]),
        rules: if r[4] == "true" {
            BitcoinWitnessRules::SegwitActive
        } else {
            BitcoinWitnessRules::NoWitness
        },
    }
}
fn error_code(error: &BitcoinSupportedTransactionError) -> String {
    use BitcoinSupportedTransactionError as E;
    match error {
        E::SpendingBasic(BitcoinBasicTransactionError::NegativeOutput { .. }) => {
            "NegativeOutput".into()
        }
        E::SpendingBasic(BitcoinBasicTransactionError::OutputTooLarge { .. }) => {
            "OutputTooLarge".into()
        }
        E::SpendingBasic(BitcoinBasicTransactionError::OutputTotalTooLarge { .. }) => {
            "OutputTotalTooLarge".into()
        }
        E::SpendingBasic(BitcoinBasicTransactionError::DuplicateInput {
            first: 0,
            second: 1,
        }) => "DuplicateInput".into(),
        E::InputScript {
            input: 1,
            source: BitcoinInputError::InvalidSignature { slot: 0 },
        } => "InputScript1".into(),
        E::InputScript {
            input: 1,
            source: BitcoinInputError::UnsupportedFundingScript,
        } => "UnsupportedScript1".into(),
        E::InvalidFundingValue { input } => format!("InvalidFundingValue{input}"),
        E::InputTotalTooLarge { input } => format!("InputTotalTooLarge{input}"),
        E::MissingFunding { input, .. } => format!("MissingFunding{input}"),
        E::FundingOutput {
            input: 0,
            vout: 100,
        } => "FundingOutput0".into(),
        E::UnexpectedExternalFunding { index } => format!("UnexpectedFunding{index}"),
        E::DuplicateFunding { index } => format!("DuplicateFunding{index}"),
        E::ForwardOrSelfReference {
            input: 0,
            producer: 2,
        } => "ForwardReference".into(),
        E::SameBlockCoinbaseSpend { input: 0 } => "SameBlockCoinbase".into(),
        E::InputsBelowOutputs => "InputsBelowOutputs".into(),
        other => panic!("unexpected actual error: {other:?}"),
    }
}
#[test]
fn complete_input_and_amount_corpus_has_no_partial_success_or_authority() {
    let core: std::collections::BTreeMap<_, _> =
        include_str!("../fixtures/bitcoin_supported_input_core_outcomes.tsv")
            .lines()
            .map(|line| {
                let r: Vec<_> = line.split('\t').collect();
                (r[0], r[1])
            })
            .collect();
    assert_eq!(core.len(), 133);
    let mut accepted = 0;
    for line in VECTORS.lines() {
        let r: Vec<_> = line.split('\t').collect();
        assert_eq!(r.len(), 10);
        let block = bytes(r[2]);
        let funding: Vec<_> = if r[5] == "-" {
            vec![]
        } else {
            r[5].split('|').map(bytes).collect()
        };
        let external: Vec<_> = funding.iter().map(Vec::as_slice).collect();
        let result = check_bitcoin_supported_transaction(
            TradeLayerMainnet::Bitcoin,
            &block,
            expected(&r),
            BITCOIN_STANDARD_INPUT_FLAGS,
            r[1].parse().unwrap(),
            &external,
        );
        if r[6] != "OK" {
            assert_eq!(error_code(&result.unwrap_err()), r[6], "{}", r[0]);
            continue;
        }
        let relation = result.unwrap_or_else(|e| panic!("{}: {e:?}", r[0]));
        assert_eq!(relation.profile(), BITCOIN_SUPPORTED_TRANSACTION_PROFILE);
        assert_eq!(
            relation.transaction_position(),
            r[1].parse::<u32>().unwrap()
        );
        assert_eq!(relation.inputs().len(), r[7].parse::<usize>().unwrap());
        assert_eq!(relation.input_satoshis(), r[8].parse::<u64>().unwrap());
        assert_eq!(relation.output_satoshis(), r[9].parse::<u64>().unwrap());
        assert_eq!(
            relation.conditional_fee_satoshis(),
            relation.input_satoshis() - relation.output_satoshis()
        );
        assert_eq!(relation.block_relation().claimed_context(), expected(&r));
        let spending = relation
            .block_relation()
            .raw_transaction(relation.transaction_position() as usize)
            .unwrap();
        for (vin, checked) in relation.inputs().iter().enumerate() {
            assert_eq!(
                core[format!("{}/{vin}", r[0]).as_str()],
                "true",
                "{} input {vin} disagrees with genuine Core",
                r[0]
            );
            let satisfaction = checked.satisfaction();
            assert_eq!(satisfaction.spending_vin(), vin as u32);
            assert_eq!(
                satisfaction.raw_spending_transaction().as_ptr(),
                spending.as_ptr()
            );
            match checked.origin() {
                BitcoinFundingOrigin::SuppliedExternalRaw { index } => assert_eq!(
                    satisfaction.raw_funding_transaction().as_ptr(),
                    external[index].as_ptr()
                ),
                BitcoinFundingOrigin::EarlierBlockTransaction { position } => {
                    assert!(position < relation.transaction_position());
                    assert_eq!(
                        satisfaction.raw_funding_transaction().as_ptr(),
                        relation
                            .block_relation()
                            .raw_transaction(position as usize)
                            .unwrap()
                            .as_ptr()
                    );
                }
            }
        }
        assert_eq!(
            relation.require_authenticated_parent_utxo(),
            Err(BitcoinInputTrustError::FundingInclusionAndUnspentnessUnauthenticated)
        );
        assert_eq!(
            relation.require_complete_transaction_consensus(),
            Err(BitcoinInputTrustError::CompleteTransactionAndChainConsensusUnverified)
        );
        assert_eq!(
            relation.require_tradelayer_actor_admission(),
            Err(BitcoinInputTrustError::TradeLayerActorAndEconomicAdmissionUnavailable)
        );
        accepted += 1;
    }
    assert_eq!(VECTORS.lines().count(), 67);
    assert_eq!(accepted, 48);
}
fn core_reason(error: &BitcoinBasicTransactionError) -> &str {
    use BitcoinBasicTransactionError as E;
    match error {
        E::EmptyInputs => "bad-txns-vin-empty",
        E::EmptyOutputs => "bad-txns-vout-empty",
        E::StrippedSize => "bad-txns-oversize",
        E::NegativeOutput { .. } => "bad-txns-vout-negative",
        E::OutputTooLarge { .. } => "bad-txns-vout-toolarge",
        E::OutputTotalTooLarge { .. } => "bad-txns-txouttotal-toolarge",
        E::DuplicateInput { .. } => "bad-txns-inputs-duplicate",
        E::CoinbaseScriptLength => "bad-cb-length",
        E::NullPrevout { .. } => "bad-txns-prevout-null",
        other => panic!("outside admitted parser subset: {other:?}"),
    }
}
#[test]
fn literal_context_free_rules_match_executed_unchanged_core() {
    let raw: Vec<_> = include_str!("../fixtures/bitcoin_context_free_raw.tsv")
        .lines()
        .collect();
    let actual: Vec<_> = include_str!("../fixtures/bitcoin_context_free_core_outcomes.tsv")
        .lines()
        .collect();
    assert_eq!(raw.len(), 72);
    assert_eq!(raw.len(), actual.len());
    for (raw, expected) in raw.iter().zip(actual) {
        let (label, raw) = raw.split_once('\t').unwrap();
        let expected: Vec<_> = expected.split('\t').collect();
        assert_eq!(label, expected[0]);
        match check_bitcoin_context_free_transaction(&bytes(raw)) {
            Ok(_) => assert_eq!(expected[1], "true", "{label}"),
            Err(error) => {
                assert_eq!(expected[1], "false", "{label}: {error:?}");
                assert_eq!(core_reason(&error), expected[2], "{label}");
            }
        }
    }
}
#[test]
fn explicit_network_flags_context_and_position_are_not_inferred() {
    let r = row("p2wpkh-mode-1");
    let block = bytes(r[2]);
    let funds: Vec<_> = r[5].split('|').map(bytes).collect();
    let external: Vec<_> = funds.iter().map(Vec::as_slice).collect();
    for network in [TradeLayerMainnet::Litecoin, TradeLayerMainnet::Dogecoin] {
        assert_eq!(
            check_bitcoin_supported_transaction(
                network,
                &block,
                expected(&r),
                BITCOIN_STANDARD_INPUT_FLAGS,
                1,
                &external
            )
            .unwrap_err(),
            BitcoinSupportedTransactionError::UnsupportedNetwork
        );
    }
    assert_eq!(
        check_bitcoin_supported_transaction(
            TradeLayerMainnet::Bitcoin,
            &block,
            expected(&r),
            0,
            1,
            &external
        )
        .unwrap_err(),
        BitcoinSupportedTransactionError::UnsupportedFlags
    );
    let mut wrong = expected(&r);
    wrong.tip.id.0[0] ^= 1;
    assert!(matches!(
        check_bitcoin_supported_transaction(
            TradeLayerMainnet::Bitcoin,
            &block,
            wrong,
            BITCOIN_STANDARD_INPUT_FLAGS,
            1,
            &external
        ),
        Err(BitcoinSupportedTransactionError::WitnessBinding(_))
    ));
    let mut wrong = expected(&r);
    wrong.parent.0[0] ^= 1;
    assert!(matches!(
        check_bitcoin_supported_transaction(
            TradeLayerMainnet::Bitcoin,
            &block,
            wrong,
            BITCOIN_STANDARD_INPUT_FLAGS,
            1,
            &external
        ),
        Err(BitcoinSupportedTransactionError::WitnessBinding(_))
    ));
    assert_eq!(
        check_bitcoin_supported_transaction(
            TradeLayerMainnet::Bitcoin,
            &block,
            expected(&r),
            BITCOIN_STANDARD_INPUT_FLAGS,
            0,
            &external
        )
        .unwrap_err(),
        BitcoinSupportedTransactionError::CoinbasePosition
    );
    assert_eq!(
        check_bitcoin_supported_transaction(
            TradeLayerMainnet::Bitcoin,
            &block,
            expected(&r),
            BITCOIN_STANDARD_INPUT_FLAGS,
            9,
            &external
        )
        .unwrap_err(),
        BitcoinSupportedTransactionError::TransactionPosition
    );
}

fn large_raw_funding(nonce: u32) -> Vec<u8> {
    let mut raw = 2_u32.to_le_bytes().to_vec();
    raw.push(1);
    raw.extend([0x42; 32]);
    raw.extend([0; 4]);
    raw.push(0);
    raw.extend([0xff; 4]);
    raw.push(1);
    raw.extend([0; 8]);
    raw.push(254);
    raw.extend(900000_u32.to_le_bytes());
    raw.resize(raw.len() + 900000, 0x51);
    raw.extend(nonce.to_le_bytes());
    raw
}
#[test]
fn funding_bounds_and_malformed_evidence_refuse_instead_of_a_partial_receipt() {
    let r = row("p2wpkh-mode-1");
    let block = bytes(r[2]);
    let funds: Vec<_> = r[5].split('|').map(bytes).collect();
    let over_count = vec![funds[0].as_slice(); 100001];
    assert_eq!(
        check_bitcoin_supported_transaction(
            TradeLayerMainnet::Bitcoin,
            &block,
            expected(&r),
            BITCOIN_STANDARD_INPUT_FLAGS,
            1,
            &over_count
        )
        .unwrap_err(),
        BitcoinSupportedTransactionError::Capacity
    );
    let mut malformed = funds[0].clone();
    malformed.push(0);
    assert!(matches!(
        check_bitcoin_supported_transaction(
            TradeLayerMainnet::Bitcoin,
            &block,
            expected(&r),
            BITCOIN_STANDARD_INPUT_FLAGS,
            1,
            &[&malformed, &funds[1]]
        ),
        Err(BitcoinSupportedTransactionError::FundingMalformed { index: 0, .. })
    ));
    let mut large = Vec::new();
    for nonce in 0..19 {
        large.push(large_raw_funding(nonce));
    }
    let external: Vec<_> = large.iter().map(Vec::as_slice).collect();
    assert_eq!(
        check_bitcoin_supported_transaction(
            TradeLayerMainnet::Bitcoin,
            &block,
            expected(&r),
            BITCOIN_STANDARD_INPUT_FLAGS,
            1,
            &external
        )
        .unwrap_err(),
        BitcoinSupportedTransactionError::Capacity
    );
}
