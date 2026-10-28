use tradelayer_core::bitcoin_witness_binding::*;
use tradelayer_core::chain::{BlockId, Tip};
use tradelayer_core::raw_block_ingest::{RawBlockIngestError, UnverifiedRawBlockView};

const FIXTURE: &str = include_str!("../fixtures/bitcoin_witness_binding.tsv");
fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn hash(hex: &str) -> [u8; 32] {
    bytes(hex).try_into().unwrap()
}
fn context(row: &[&str]) -> ExpectedWitnessBlock {
    ExpectedWitnessBlock {
        tip: Tip {
            height: row[1].parse().unwrap(),
            id: BlockId(hash(row[2])),
        },
        parent: BlockId(hash(row[3])),
        rules: if row[4] == "active" {
            BitcoinWitnessRules::SegwitActive
        } else {
            BitcoinWitnessRules::NoWitness
        },
    }
}
fn row(name: &str) -> Vec<&str> {
    FIXTURE
        .lines()
        .find(|line| line.starts_with(&format!("{name}\t")))
        .unwrap()
        .split('\t')
        .collect()
}
fn result_name(error: &WitnessBindingError) -> &'static str {
    match error {
        WitnessBindingError::RawBlock(RawBlockIngestError::RequestedHeaderHashMismatch) => {
            "HeaderMismatch"
        }
        WitnessBindingError::WrongParent => "WrongParent",
        WitnessBindingError::MissingFirstCoinbase => "MissingFirstCoinbase",
        WitnessBindingError::AdditionalCoinbase { .. } => "AdditionalCoinbase",
        WitnessBindingError::InvalidReservedValue => "InvalidReservedValue",
        WitnessBindingError::UnexpectedWitness => "UnexpectedWitness",
        WitnessBindingError::CommitmentMismatch => "CommitmentMismatch",
        _ => "OtherParserError",
    }
}

#[test]
fn official_core_and_independent_bip141_adversarial_corpus() {
    let mut count = 0;
    let mut successes = 0;
    for line in FIXTURE.lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 11);
        let raw = bytes(row[10]);
        match inspect_bitcoin_witness_binding(&raw, context(&row)) {
            Ok(relation) => {
                assert_eq!(row[5], "OK", "{} unexpectedly accepted", row[0]);
                assert_eq!(relation.claimed_context(), context(&row));
                assert_eq!(relation.raw_block(), raw);
                assert_eq!(relation.raw_block().as_ptr(), raw.as_ptr());
                assert!(relation.raw_transaction(0).is_some());
                assert!(relation
                    .raw_transaction(relation.transaction_count())
                    .is_none());
                assert_eq!(
                    relation.commitment_vout(),
                    if row[6] == "-" {
                        None
                    } else {
                        Some(row[6].parse().unwrap())
                    }
                );
                assert_eq!(
                    relation.witness_root_raw(),
                    if row[7] == "-" {
                        None
                    } else {
                        Some(hash(row[7]))
                    }
                );
                assert_eq!(
                    relation.reserved_value(),
                    if row[8] == "-" {
                        None
                    } else {
                        Some(hash(row[8]))
                    }
                );
                assert_eq!(
                    relation.require_authenticated_chain(),
                    Err(WitnessBindingTrustError::ActiveChainAndRuleContextUnauthenticated)
                );
                assert_eq!(
                    relation.require_authenticated_parent_utxo(),
                    Err(WitnessBindingTrustError::ParentUtxoStateUnavailable)
                );
                assert_eq!(
                    relation.require_verified_scripts(),
                    Err(WitnessBindingTrustError::FullTransactionScriptsUnverified)
                );
                assert_eq!(
                    relation.require_protocol_actor_admission(),
                    Err(WitnessBindingTrustError::ProtocolAndActorAuthorityUnavailable)
                );
                successes += 1;
            }
            Err(error) => assert_eq!(result_name(&error), row[5], "{}: {error:?}", row[0]),
        }
        count += 1;
    }
    assert_eq!((count, successes), (23, 7));
}

#[test]
fn genuine_witness_only_mutations_pass_existing_txid_ingest_but_fail_new_binding() {
    let original = row("official-testnet-witness");
    let raw_original = bytes(original[10]);
    let original_index = UnverifiedRawBlockView::parse(&raw_original, None)
        .unwrap()
        .inspect_serial()
        .unwrap();
    for name in [
        "official-reserved-mutation",
        "official-spend-witness-mutation",
    ] {
        let changed = row(name);
        let raw = bytes(changed[10]);
        assert_ne!(raw, raw_original);
        assert_eq!(&raw[..80], &raw_original[..80]);
        let index = UnverifiedRawBlockView::parse(&raw, None)
            .unwrap()
            .inspect_serial()
            .unwrap();
        let txids = |index: &tradelayer_core::raw_block_ingest::UnverifiedRawBlockIndex| {
            index
                .ordered_preindex()
                .ordered()
                .iter()
                .map(|item| item.observation.txid)
                .collect::<Vec<_>>()
        };
        assert_eq!(txids(&index), txids(&original_index));
        assert_eq!(
            inspect_bitcoin_witness_binding(&raw, context(&original)).unwrap_err(),
            WitnessBindingError::CommitmentMismatch
        );
    }
}

#[test]
fn malformed_complete_block_and_wrong_context_cannot_reuse_a_relation() {
    let original = row("official-testnet-witness");
    let raw = bytes(original[10]);
    let wanted = context(&original);
    let relation = inspect_bitcoin_witness_binding(&raw, wanted).unwrap();
    for cut in [0, 4, 79, 80, raw.len() - 1] {
        assert!(inspect_bitcoin_witness_binding(&raw[..cut], wanted).is_err());
    }
    let mut trailing = raw.clone();
    trailing.push(0);
    assert_eq!(
        inspect_bitcoin_witness_binding(&trailing, wanted).unwrap_err(),
        WitnessBindingError::RawBlock(RawBlockIngestError::TrailingRawBlockData)
    );
    let mut different = wanted;
    different.parent = BlockId([0x33; 32]);
    assert_eq!(
        inspect_bitcoin_witness_binding(&raw, different).unwrap_err(),
        WitnessBindingError::WrongParent
    );
    different = wanted;
    different.tip.id = BlockId([0x44; 32]);
    assert_eq!(
        inspect_bitcoin_witness_binding(&raw, different).unwrap_err(),
        WitnessBindingError::RawBlock(RawBlockIngestError::RequestedHeaderHashMismatch)
    );
    // Height/rule activation are not proved by any matching header hash.
    different = wanted;
    different.tip.height += 1;
    let untrusted_height = inspect_bitcoin_witness_binding(&raw, different).unwrap();
    assert!(untrusted_height.require_authenticated_chain().is_err());
    different = wanted;
    different.rules = BitcoinWitnessRules::NoWitness;
    assert_eq!(
        inspect_bitcoin_witness_binding(&raw, different).unwrap_err(),
        WitnessBindingError::UnexpectedWitness
    );
    assert_eq!(relation.raw_block(), raw);
}
