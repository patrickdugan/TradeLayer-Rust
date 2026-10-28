use super::*;
use crate::bitcoin_standard_input::BITCOIN_STANDARD_INPUT_FLAGS;
use crate::bitcoin_witness_binding::BitcoinWitnessRules;
use crate::chain::{BlockId, Tip};
use crate::channel_witness_audit::Cursor;
use crate::hash256::double_sha256;
use crate::raw_block_ingest::merkle_root_unverified;
use crate::read_only::inspect_transaction;

const VECTORS: &str = include_str!("../fixtures/bitcoin_standard_input.tsv");
fn bytes(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn vector(name: &str) -> Vec<&'static str> {
    VECTORS
        .lines()
        .find(|line| line.starts_with(&format!("{name}\t")))
        .unwrap()
        .split('\t')
        .collect()
}
fn block(spending: &[u8]) -> (Vec<u8>, ExpectedWitnessBlock) {
    // Synthetic structural fixture, with no PoW or active-chain claim. The
    // spending/funding/signature pairs come from genuine Core-checked cases.
    let observation = inspect_transaction(spending).unwrap();
    let witness = observation.has_witness;
    let mut coinbase = 2u32.to_le_bytes().to_vec();
    if witness {
        coinbase.extend([0, 1]);
    }
    coinbase.push(1);
    coinbase.extend([0; 32]);
    coinbase.extend([255; 4]);
    coinbase.extend([2, 1, 1]);
    coinbase.extend([255; 4]);
    coinbase.push(1);
    coinbase.extend(0u64.to_le_bytes());
    if witness {
        let root = merkle_root_unverified(&[[0; 32], observation.wtxid]).unwrap();
        let mut preimage = root.to_vec();
        preimage.extend([0; 32]);
        coinbase.extend([38, 0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed]);
        coinbase.extend(double_sha256(&preimage));
        coinbase.extend([1, 32]);
        coinbase.extend([0; 32]);
    } else {
        coinbase.extend([1, 0x51]);
    }
    coinbase.extend([0; 4]);
    let txids = [
        inspect_transaction(&coinbase).unwrap().txid,
        observation.txid,
    ];
    let mut raw = 4u32.to_le_bytes().to_vec();
    raw.extend([0x33; 32]);
    raw.extend(merkle_root_unverified(&txids).unwrap());
    raw.extend(1_700_000_000u32.to_le_bytes());
    raw.extend(0x207fffffu32.to_le_bytes());
    raw.extend([0; 4]);
    let mut display = double_sha256(&raw);
    display.reverse();
    raw.push(2);
    raw.extend(coinbase);
    raw.extend(spending);
    (
        raw,
        ExpectedWitnessBlock {
            tip: Tip {
                height: 42,
                id: BlockId(display),
            },
            parent: BlockId([0x33; 32]),
            rules: if witness {
                BitcoinWitnessRules::SegwitActive
            } else {
                BitcoinWitnessRules::NoWitness
            },
        },
    )
}
fn corrupt_witness_signature(spending: &mut [u8], vin: usize) {
    let mut cursor = Cursor::new(spending);
    cursor.take(6).unwrap();
    for _ in 0..cursor.count().unwrap() {
        cursor.take(36).unwrap();
        cursor.data().unwrap();
        cursor.take(4).unwrap();
    }
    for _ in 0..cursor.count().unwrap() {
        cursor.take(8).unwrap();
        cursor.data().unwrap();
    }
    for index in 0..=vin {
        for _ in 0..cursor.compact().unwrap() {
            let item = cursor.data().unwrap();
            if index == vin && item.first() == Some(&0x30) {
                let at = item.as_ptr() as usize - spending.as_ptr() as usize;
                let last_r = at + 3 + item[3] as usize;
                spending[last_r] ^= 1;
                return;
            }
        }
    }
    panic!("fixture has no selected signature");
}

#[test]
fn six_core_templates_bind_the_input_to_the_exact_block_slice() {
    for template in [
        "p2pkh",
        "p2wpkh",
        "p2sh",
        "p2wsh",
        "wrapped-p2wpkh",
        "wrapped-p2wsh",
    ] {
        let row = vector(&format!("{template}-mode-1"));
        let funding = bytes(row[4]);
        let spending = bytes(row[5]);
        let (raw, expected) = block(&spending);
        let position = BitcoinBlockInputPosition {
            transaction: 1,
            input: row[2].parse().unwrap(),
        };
        let checked = check_bitcoin_block_input(
            &raw,
            expected,
            BITCOIN_STANDARD_INPUT_FLAGS,
            position,
            &funding,
            row[1].parse().unwrap(),
        )
        .unwrap();
        assert_eq!(checked.position(), position);
        assert_eq!(checked.block_relation().claimed_context(), expected);
        let actual = checked.block_relation().raw_transaction(1).unwrap();
        assert_eq!(actual, spending);
        assert_eq!(
            actual.as_ptr(),
            checked
                .input_satisfaction()
                .raw_spending_transaction()
                .as_ptr()
        );
        assert!(checked.require_authenticated_parent_utxo().is_err());
        assert!(checked.require_complete_transaction_consensus().is_err());
        assert!(checked.require_tradelayer_actor_admission().is_err());
    }
}
#[test]
fn unchanged_txid_mutation_and_recommitted_invalid_witness_both_refuse() {
    let row = vector("p2wsh-mode-1");
    let funding = bytes(row[4]);
    let spending = bytes(row[5]);
    let mut mutant = spending.clone();
    let vin = row[2].parse().unwrap();
    corrupt_witness_signature(&mut mutant, vin);
    assert_eq!(
        inspect_transaction(&spending).unwrap().txid,
        inspect_transaction(&mutant).unwrap().txid
    );
    assert_ne!(
        inspect_transaction(&spending).unwrap().wtxid,
        inspect_transaction(&mutant).unwrap().wtxid
    );
    let (original, original_context) = block(&spending);
    let mut old_commitment = original.clone();
    let offset = old_commitment.len() - spending.len();
    old_commitment[offset..].copy_from_slice(&mutant);
    let position = BitcoinBlockInputPosition {
        transaction: 1,
        input: vin as u32,
    };
    assert!(matches!(
        check_bitcoin_block_input(
            &old_commitment,
            original_context,
            BITCOIN_STANDARD_INPUT_FLAGS,
            position,
            &funding,
            0
        ),
        Err(BitcoinBlockInputError::WitnessBinding(
            WitnessBindingError::CommitmentMismatch
        ))
    ));
    let (recommitted, new_context) = block(&mutant);
    inspect_bitcoin_witness_binding(&recommitted, new_context).unwrap();
    // Detached valid signatures cannot rescue the actual invalid witness bytes
    // in an otherwise self-consistent newly committed block.
    check_bitcoin_standard_input(
        BITCOIN_STANDARD_INPUT_FLAGS,
        &funding,
        0,
        &spending,
        vin as u32,
    )
    .unwrap();
    assert!(matches!(
        check_bitcoin_block_input(
            &recommitted,
            new_context,
            BITCOIN_STANDARD_INPUT_FLAGS,
            position,
            &funding,
            0
        ),
        Err(BitcoinBlockInputError::Input(
            BitcoinInputError::InvalidSignature { .. }
        ))
    ));
}
#[test]
fn context_flags_positions_and_funding_cannot_be_substituted() {
    let row = vector("p2wpkh-mode-1");
    let funding = bytes(row[4]);
    let spending = bytes(row[5]);
    let (raw, expected) = block(&spending);
    let position = BitcoinBlockInputPosition {
        transaction: 1,
        input: row[2].parse().unwrap(),
    };
    let check = |context, flags, at, prev: &[u8], vout| {
        check_bitcoin_block_input(&raw, context, flags, at, prev, vout).map(|_| ())
    };
    assert_eq!(
        check(
            expected,
            BITCOIN_STANDARD_INPUT_FLAGS,
            BitcoinBlockInputPosition {
                transaction: 0,
                input: 0
            },
            &funding,
            0
        ),
        Err(BitcoinBlockInputError::CoinbasePosition)
    );
    assert_eq!(
        check(
            expected,
            BITCOIN_STANDARD_INPUT_FLAGS,
            BitcoinBlockInputPosition {
                transaction: 2,
                input: 0
            },
            &funding,
            0
        ),
        Err(BitcoinBlockInputError::TransactionPosition)
    );
    assert_eq!(
        check(expected, 0, position, &funding, 0),
        Err(BitcoinBlockInputError::Input(
            BitcoinInputError::UnsupportedFlags
        ))
    );
    assert_eq!(
        check(
            expected,
            BITCOIN_STANDARD_INPUT_FLAGS,
            BitcoinBlockInputPosition {
                transaction: 1,
                input: u32::MAX
            },
            &funding,
            0
        ),
        Err(BitcoinBlockInputError::Input(
            BitcoinInputError::SpendingIndex
        ))
    );
    let mut wrong = expected;
    wrong.parent.0[0] ^= 1;
    assert!(matches!(
        check(wrong, BITCOIN_STANDARD_INPUT_FLAGS, position, &funding, 0),
        Err(BitcoinBlockInputError::WitnessBinding(
            WitnessBindingError::WrongParent
        ))
    ));
    let other = bytes(vector("p2pkh-mode-1")[4]);
    assert_eq!(
        check(expected, BITCOIN_STANDARD_INPUT_FLAGS, position, &other, 0),
        Err(BitcoinBlockInputError::Input(
            BitcoinInputError::WrongPrevout
        ))
    );
}

#[test]
fn weak_commitments_are_real_input_satisfaction_without_actor_authority() {
    for name in [
        "legacy-single-bug-output-can-change",
        "witness-none-output-can-change",
    ] {
        let row = vector(name);
        let funding = bytes(row[4]);
        let spending = bytes(row[5]);
        let (raw, expected) = block(&spending);
        let checked = check_bitcoin_block_input(
            &raw,
            expected,
            BITCOIN_STANDARD_INPUT_FLAGS,
            BitcoinBlockInputPosition {
                transaction: 1,
                input: row[2].parse().unwrap(),
            },
            &funding,
            row[1].parse().unwrap(),
        )
        .unwrap();
        let signatures = checked.input_satisfaction().signatures();
        if name.starts_with("legacy-single") {
            assert!(signatures
                .iter()
                .all(|signature| signature.legacy_single_bug));
            let mut one = [0; 32];
            one[0] = 1;
            assert!(signatures
                .iter()
                .all(|signature| signature.digest_raw == one));
        } else {
            assert!(signatures
                .iter()
                .all(|signature| signature.hash_type & 31 == 2));
            assert!(signatures
                .iter()
                .all(|signature| !signature.legacy_single_bug));
        }
        assert!(checked.require_authenticated_parent_utxo().is_err());
        assert!(checked.require_complete_transaction_consensus().is_err());
        assert!(checked.require_tradelayer_actor_admission().is_err());
    }
}
