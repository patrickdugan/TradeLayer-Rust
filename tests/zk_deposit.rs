//! M16 deposit attestation parser against the real mainnet public values
//! (`fixtures/zk_m16_public.bin`, tip 975,072, payment in block 975,067).

use tradelayer_core::zk_deposit::*;

const FIXTURE: &[u8] = include_bytes!("../fixtures/zk_m16_public.bin");

fn policy() -> DepositPolicy {
    DepositPolicy::bitcoin_mainnet(6)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn mutated(at: usize, value: u8) -> Vec<u8> {
    let mut raw = FIXTURE.to_vec();
    raw[at] = value;
    raw
}

fn set_u32(raw: &mut [u8], at: usize, value: u32) {
    raw[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn fixture_is_the_pinned_statement() {
    assert_eq!(FIXTURE.len(), PUBLIC_VALUES_LEN);
    assert_eq!(
        hex(&committed_values_sha256(FIXTURE)),
        "cae6068f7ad8c4891cb3220702a88b0bc3df54013f45e65df11a9ced531bc869"
    );
}

#[test]
fn accepts_the_mainnet_deposit() {
    let att = parse_public_values(FIXTURE, &policy()).expect("accepted");
    assert_eq!(att.tip_height, 975_072);
    assert_eq!(att.block_height, 975_067);
    assert_eq!(att.confirmations, 6);
    assert_eq!(att.segments, 2);
    assert_eq!(att.outpoint.vout, 0);
    assert_eq!(att.value_sats, 940_000);
    assert_eq!(att.script_len, 22);
    // txid a652ab0c...0045 is shown in explorer order; the record holds it reversed.
    let mut display = att.outpoint.txid;
    display.reverse();
    let display = hex(&display);
    assert!(display.starts_with("a652ab0c"), "{display}");
    assert!(display.ends_with("0045"), "{display}");
    assert_eq!(
        hex(&att.script_sha256),
        "58432f41ce2b63d9a8e65e9b5bed183cd2ed12a03fe2b6020cda00235c147283"
    );
    assert_eq!(att.tip_state.len(), STATE_BYTES);
}

#[test]
fn accepts_threshold_at_exactly_the_confirmations() {
    assert!(parse_public_values(FIXTURE, &DepositPolicy::bitcoin_mainnet(1)).is_ok());
    assert!(parse_public_values(FIXTURE, &DepositPolicy::bitcoin_mainnet(6)).is_ok());
}

#[test]
fn refuses_wrong_length() {
    assert_eq!(
        parse_public_values(&FIXTURE[..PUBLIC_VALUES_LEN - 1], &policy()),
        Err(DepositRefusal::Length { got: 1291 })
    );
    let mut long = FIXTURE.to_vec();
    long.push(0);
    assert_eq!(
        parse_public_values(&long, &policy()),
        Err(DepositRefusal::Length { got: 1293 })
    );
    assert_eq!(
        parse_public_values(&[], &policy()),
        Err(DepositRefusal::Length { got: 0 })
    );
}

#[test]
fn refuses_chain_magic() {
    // NHT4 is the testnet4 statement; this policy pins the mainnet circuit.
    let mut raw = FIXTURE.to_vec();
    raw[0..4].copy_from_slice(b"NHT4");
    assert_eq!(
        parse_public_values(&raw, &policy()),
        Err(DepositRefusal::ChainMagic)
    );
}

#[test]
fn refuses_other_headers_per_step() {
    let mut raw = FIXTURE.to_vec();
    set_u32(&mut raw, 4, 10);
    assert_eq!(
        parse_public_values(&raw, &policy()),
        Err(DepositRefusal::StepsPerFold { got: 10 })
    );
}

#[test]
fn refuses_foreign_genesis_state() {
    assert_eq!(
        parse_public_values(&mutated(12, FIXTURE[12] ^ 1), &policy()),
        Err(DepositRefusal::GenesisState)
    );
    assert_eq!(
        parse_public_values(
            &mutated(12 + STATE_BYTES - 1, FIXTURE[555] ^ 0x80),
            &policy()
        ),
        Err(DepositRefusal::GenesisState)
    );
}

#[test]
fn refuses_unpinned_nova_key() {
    let at = 12 + 2 * STATE_BYTES;
    assert_eq!(
        parse_public_values(&mutated(at, FIXTURE[at] ^ 1), &policy()),
        Err(DepositRefusal::NovaKey)
    );
}

#[test]
fn refuses_pay1_magic() {
    let mut raw = FIXTURE.to_vec();
    raw[CHAIN_STATEMENT_LEN..CHAIN_STATEMENT_LEN + 4].copy_from_slice(b"PAY2");
    assert_eq!(
        parse_public_values(&raw, &policy()),
        Err(DepositRefusal::Pay1Magic)
    );
}

#[test]
fn refuses_script_length_over_limit() {
    let mut raw = FIXTURE.to_vec();
    set_u32(&mut raw, CHAIN_STATEMENT_LEN + 92, MAX_SCRIPT_LEN + 1);
    assert_eq!(
        parse_public_values(&raw, &policy()),
        Err(DepositRefusal::ScriptLength {
            got: MAX_SCRIPT_LEN + 1
        })
    );
    set_u32(&mut raw, CHAIN_STATEMENT_LEN + 92, MAX_SCRIPT_LEN);
    assert!(parse_public_values(&raw, &policy()).is_ok());
}

#[test]
fn refuses_confirmations_under_threshold() {
    assert_eq!(
        parse_public_values(FIXTURE, &DepositPolicy::bitcoin_mainnet(7)),
        Err(DepositRefusal::ConfirmationsBelowThreshold { got: 6, need: 7 })
    );
}

#[test]
fn refuses_inflated_confirmations() {
    let mut raw = FIXTURE.to_vec();
    set_u32(&mut raw, CHAIN_STATEMENT_LEN + 44, 60);
    assert_eq!(
        parse_public_values(&raw, &policy()),
        Err(DepositRefusal::ConfirmationsInconsistent)
    );
}

#[test]
fn refuses_payment_above_tip_and_zero_value() {
    let mut raw = FIXTURE.to_vec();
    set_u32(&mut raw, CHAIN_STATEMENT_LEN + 8, 975_073);
    assert_eq!(
        parse_public_values(&raw, &policy()),
        Err(DepositRefusal::HeightAboveTip)
    );
    let mut raw = FIXTURE.to_vec();
    raw[CHAIN_STATEMENT_LEN + 84..CHAIN_STATEMENT_LEN + 92].fill(0);
    assert_eq!(
        parse_public_values(&raw, &policy()),
        Err(DepositRefusal::ZeroValue)
    );
}

#[test]
fn wrap_statement_binds_program_and_values() {
    let good = WrapStatement {
        program_vk_digest: M16_PROGRAM_KEY,
        committed_values_sha256: committed_values_sha256(FIXTURE),
    };
    assert!(check_wrap_statement(&good, FIXTURE, &policy()).is_ok());

    let other_program = WrapStatement {
        program_vk_digest: [7; 32],
        ..good
    };
    assert_eq!(
        check_wrap_statement(&other_program, FIXTURE, &policy()),
        Err(DepositRefusal::ProgramKey)
    );

    // Public values that differ from what the proof committed are refused before parsing.
    let tampered = mutated(CHAIN_STATEMENT_LEN + 84, 0xff);
    assert_eq!(
        check_wrap_statement(&good, &tampered, &policy()),
        Err(DepositRefusal::CommittedValuesDigest)
    );
}

#[test]
fn attestation_matches_the_expected_script_only() {
    let att = parse_public_values(FIXTURE, &policy()).unwrap();
    assert!(!att.pays_script(b"not the script"));
    assert!(!att.pays_script(&[]));
}

#[test]
fn consumed_outpoint_is_credited_once() {
    let att = parse_public_values(FIXTURE, &policy()).unwrap();
    let mut set = ConsumedOutpoints::new();
    assert!(set.is_empty());
    assert_eq!(set.consume(&att), Ok(()));
    assert!(set.contains(&att.outpoint));
    assert_eq!(
        set.consume(&att),
        Err(ReplayRefusal::AlreadyConsumed {
            block_hash: att.block_hash
        })
    );

    // The same outpoint re-confirmed in another block after a reorg is still refused.
    let mut reorged = att.clone();
    reorged.block_hash = [9; 32];
    assert_eq!(
        set.consume(&reorged),
        Err(ReplayRefusal::AlreadyConsumed {
            block_hash: att.block_hash
        })
    );

    // A different output of the same transaction is a different outpoint.
    let mut sibling = att.clone();
    sibling.outpoint.vout = 1;
    assert_eq!(set.consume(&sibling), Ok(()));
    assert_eq!(set.len(), 2);
}
