use tradelayer_core::ledger::{AccountId, PropertyId, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::reporting::{OfflineSpotReports, ReportingError, SpotFillObservation};
use tradelayer_core::spot::{SpotFill, SpotPair};
use tradelayer_core::trade_history::{TradeDetails, TradeHistoryError, TradeId};
use tradelayer_core::volume_index::{VolumeError, VolumePair};

fn observation(id: u8, height: u64, event_index: u32) -> SpotFillObservation {
    SpotFillObservation {
        network: TradeLayerMainnet::Litecoin,
        fill: SpotFill {
            sell_txid: TxId::from_bytes([1; 32]),
            buy_txid: TxId::from_bytes([2; 32]),
            pair: SpotPair::new(
                PropertyId::try_from(5).unwrap(),
                PropertyId::try_from(6).unwrap(),
            )
            .unwrap(),
            base_atoms: 3,
            quote_atoms: 2,
            trigger_txid: TxId::from_bytes([3; 32]),
            at_height: height,
        },
        trade_id: TradeId([id; 32]),
        event_index,
        buyer: AccountId::try_from("buyer").unwrap(),
        seller: AccountId::try_from("seller").unwrap(),
        ltc_volume_atoms: 10,
    }
}

#[test]
fn reports_existing_fill_in_both_indexes_with_explicit_rounding() {
    let mut reports = OfflineSpotReports::new(TradeLayerMainnet::Litecoin, 8).unwrap();
    let event = observation(4, 10, 0);
    let fill_before = event.fill.clone();
    reports.record_spot_fill(event).unwrap();
    assert_eq!(reports.network(), TradeLayerMainnet::Litecoin);
    let record = reports.history().get(TradeId([4; 32])).unwrap();
    assert_eq!(record.buyer.as_str(), "buyer");
    assert_eq!(record.buyer_tx, fill_before.buy_txid);
    assert!(matches!(
        record.details,
        TradeDetails::Token {
            amount1_atoms: 3,
            amount2_atoms: 2,
            ..
        }
    ));
    let price = reports
        .volume()
        .vwap(
            VolumePair {
                first: 5,
                second: 6,
            },
            10,
            10,
        )
        .unwrap()
        .unwrap()
        .price_atoms_half_up()
        .unwrap();
    assert_eq!(price, 66_666_667);
    assert_eq!(
        reports.volume().totals_through(10).unwrap().global_atoms,
        10
    );
}

#[test]
fn second_index_failure_leaves_first_index_unchanged() {
    let mut reports = OfflineSpotReports::new(TradeLayerMainnet::Litecoin, 1).unwrap();
    reports.record_spot_fill(observation(4, 10, 0)).unwrap();
    let before = reports.clone();
    assert_eq!(
        reports.record_spot_fill(observation(5, 11, 0)),
        Err(ReportingError::Volume(VolumeError::CapacityExceeded))
    );
    assert_eq!(reports, before);
    assert_eq!(reports.history().get(TradeId([5; 32])), None);
}

#[test]
fn conflicting_block_event_and_duplicate_valuation_cannot_double_count() {
    let mut reports = OfflineSpotReports::new(TradeLayerMainnet::Litecoin, 8).unwrap();
    reports.record_spot_fill(observation(4, 10, 0)).unwrap();
    let before = reports.clone();
    assert_eq!(
        reports.record_spot_fill(observation(5, 10, 0)),
        Err(ReportingError::History(
            TradeHistoryError::ConflictingPosition {
                block_height: 10,
                event_index: 0
            }
        ))
    );
    let mut duplicate = observation(4, 10, 1);
    duplicate.ltc_volume_atoms = 999;
    assert_eq!(
        reports.record_spot_fill(duplicate),
        Err(ReportingError::DuplicateObservation)
    );
    assert_eq!(reports, before);
}

#[test]
fn replacement_suffix_restores_history_and_volume_together() {
    let mut reports = OfflineSpotReports::new(TradeLayerMainnet::Litecoin, 8).unwrap();
    reports.record_spot_fill(observation(4, 10, 0)).unwrap();
    let retained = reports.clone();
    reports.record_spot_fill(observation(5, 11, 0)).unwrap();
    assert_eq!(reports.rollback_after(10), 1);
    assert_eq!(reports, retained);
    let mut replacement = observation(5, 11, 0);
    replacement.ltc_volume_atoms = 7;
    reports.record_spot_fill(replacement).unwrap();
    assert_eq!(
        reports.volume().totals_through(11).unwrap().global_atoms,
        17
    );
    assert_eq!(reports.rollback_after(u64::MAX), 0);
}

#[test]
fn network_mixing_and_unrepresentable_prices_fail_atomically() {
    let mut reports = OfflineSpotReports::new(TradeLayerMainnet::Litecoin, 8).unwrap();
    let mut event = observation(4, 10, 0);
    event.network = TradeLayerMainnet::Bitcoin;
    assert_eq!(
        reports.record_spot_fill(event),
        Err(ReportingError::WrongNetwork)
    );
    let before = reports.clone();
    let mut event = observation(4, 10, 0);
    event.fill.base_atoms = 1;
    event.fill.quote_atoms = u64::MAX;
    assert_eq!(
        reports.record_spot_fill(event),
        Err(ReportingError::Volume(VolumeError::ArithmeticOverflow))
    );
    let mut event = observation(4, 10, 0);
    event.fill.base_atoms = 0;
    assert_eq!(
        reports.record_spot_fill(event),
        Err(ReportingError::InvalidFill)
    );
    assert_eq!(reports, before);
}
