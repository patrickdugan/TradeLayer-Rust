use tradelayer_core::decimal::{Decimal, MAX_DECIMAL_DIGITS};
use tradelayer_core::iou::{Claim, IouKey, IouSnapshotError, IouStore, MAX_IOU_CLAIMS};
use tradelayer_core::iou_ledger::{FundedIouError, FundedIouLedger, IouInput, PayoutAtomPolicy};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;

fn account(text: &str) -> AccountId {
    AccountId::try_from(text).unwrap()
}

fn property() -> PropertyId {
    PropertyId::try_from(1).unwrap()
}

fn txid(number: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[..8].copy_from_slice(&number.to_be_bytes());
    TxId::from_bytes(bytes)
}

fn funded_ledger() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            txid(0),
            Operation::CreateProperty {
                id: property(),
                issuer: account("payer"),
                kind: PropertyKind::Fixed,
                initial_atoms: 1,
            },
        )
        .unwrap();
    ledger
}

fn self_transfer() -> Operation {
    Operation::Transfer {
        id: property(),
        from: account("payer"),
        to: account("payer"),
        amount_atoms: 1,
    }
}

#[test]
fn funded_import_rejects_source_state_larger_than_the_checkpoint_byte_cap() {
    let amount: Decimal = "9".repeat(MAX_DECIMAL_DIGITS).parse().unwrap();
    let claims: Vec<_> = (0..MAX_IOU_CLAIMS)
        .map(|index| Claim {
            address: account(&format!("c{index}")),
            amount: amount.clone(),
        })
        .collect();
    // TLIO header, one claim-map header and CRC take 41 bytes; each
    // claim adds its account length, account, decimal length and decimal.
    let canonical_bytes = 41
        + claims
            .iter()
            .map(|claim| 1 + claim.address.as_str().len() + 4 + MAX_DECIMAL_DIGITS)
            .sum::<usize>();
    assert!(canonical_bytes > 64 * 1024 * 1024);

    let mut source = IouStore::new();
    source
        .save_claim_map(IouKey::new(42, 1), 0, claims)
        .unwrap();
    assert_eq!(source.claim_count(), MAX_IOU_CLAIMS);
    assert!(matches!(
        source.snapshot_bytes(),
        Err(IouSnapshotError::InvalidFormat("IOU snapshot too large"))
    ));

    // The standalone arithmetic store has independent serialization bounds.
    // The funded owner must reject this import before accepting its state.
    assert!(matches!(
        FundedIouLedger::from_ledger_unverified(
            TradeLayerMainnet::Bitcoin,
            PayoutAtomPolicy::Exact,
            funded_ledger(),
            source,
        ),
        Err(FundedIouError::UncheckpointableState)
    ));
}

#[test]
fn funded_import_and_transaction_enforce_the_embedded_ledger_journal_cap() {
    // Build the replay journal in a single atomic batch. Repeated apply()
    // calls would clone a growing journal for every harmless self-transfer.
    let mut ledger = funded_ledger();
    ledger
        .apply_batch((1..100_000).map(|number| (txid(number), self_transfer())))
        .unwrap();
    assert_eq!(ledger.processed_count(), 100_000);
    assert!(ledger.snapshot_bytes().is_ok());

    let mut funded = FundedIouLedger::from_ledger_unverified(
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Exact,
        ledger.clone(),
        IouStore::new(),
    )
    .unwrap();
    let before = funded.clone();
    // Collection needs one internal transfer ID. It would cross the TLL
    // journal limit, so both source accounting and the atom debit must roll
    // back, and the caller's outer ID must remain unconsumed.
    assert!(matches!(
        funded.collect_loss_unverified(
            IouInput {
                network: TradeLayerMainnet::Bitcoin,
                txid: txid(1_000_000),
                height: 1,
            },
            IouKey::new(42, 1),
            account("payer"),
            1,
        ),
        Err(FundedIouError::UncheckpointableState)
    ));
    assert!(funded == before);
    assert!(funded.snapshot_bytes().is_ok());

    // The general ledger permits this journal independently of checkpoint
    // limits, but the funded owner's constructor may not import it.
    ledger.apply(txid(100_000), self_transfer()).unwrap();
    assert_eq!(ledger.processed_count(), 100_001);
    assert!(ledger.snapshot_bytes().is_err());
    assert!(matches!(
        FundedIouLedger::from_ledger_unverified(
            TradeLayerMainnet::Bitcoin,
            PayoutAtomPolicy::Exact,
            ledger,
            IouStore::new(),
        ),
        Err(FundedIouError::UncheckpointableState)
    ));
}
