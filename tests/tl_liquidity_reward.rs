use tradelayer_core::native_entitlement::{
    ATOMS_PER_TOKEN, BITCOIN_TLVEST_PROPERTY_ID, BITCOIN_TL_PROPERTY_ID,
};
use tradelayer_core::tl_liquidity_reward::{
    decade_halvings, quote_reward, source_fee_share_ppm, PriceOrigin, RewardObservation,
    RewardPhase, FALLBACK_TL_PRICE_BTC_ATOMS,
};

const U: u128 = ATOMS_PER_TOKEN;
fn observation(cumulative_btc_atoms: u128) -> RewardObservation {
    RewardObservation {
        trade_btc_atoms: 10 * U,
        traded_tl_atoms: 10_000 * U,
        net_fee_btc_atoms: 100_000,
        cumulative_btc_atoms,
        vest_complete_btc_atoms: 21_000_000 * U,
        tl_vwap_price_btc_atoms: None,
    }
}

#[test]
fn source_curve_descends_with_volume_and_keeps_fee_share_bounded() {
    let low = source_fee_share_ppm(100 * U);
    let mid = source_fee_share_ppm(1_000 * U);
    let high = source_fee_share_ppm(10_000_000 * U);
    assert!(low > mid);
    assert!(mid > high);
    assert!(low <= 350_000);
    assert!(high >= 20_000);
}

#[test]
fn missing_vwap_preserves_low_base_coin_price_and_pays_tlvest_before_vesting() {
    let quote = quote_reward(observation(100 * U)).unwrap();
    assert_eq!(quote.phase, RewardPhase::Vesting);
    assert_eq!(quote.reward_property_id, BITCOIN_TLVEST_PROPERTY_ID);
    assert_eq!(quote.price_origin, PriceOrigin::LowPriceFallback);
    assert_eq!(quote.tl_price_btc_atoms, FALLBACK_TL_PRICE_BTC_ATOMS);
    assert_eq!(quote.tl_price_btc_atoms, U / 1_000);
    assert_eq!(quote.eligible_trade_btc_atoms, 10 * U);
    assert!(quote.reward_value_btc_atoms < 100_000);
    assert!(quote.reward_token_atoms > 0);
}

#[test]
fn vwap_caps_executed_trade_value_and_both_quotes_are_fee_bounded() {
    let reference = quote_reward(observation(100 * U)).unwrap();
    let mut expensive_trade = observation(100 * U);
    expensive_trade.traded_tl_atoms = 100 * U;
    let quote = quote_reward(expensive_trade).unwrap();
    assert_eq!(quote.eligible_trade_btc_atoms, U / 10);
    assert!(quote.reward_value_btc_atoms < reference.reward_value_btc_atoms);
    assert!(quote.reward_token_atoms < reference.reward_token_atoms);

    expensive_trade.tl_vwap_price_btc_atoms = Some(200_000);
    let priced = quote_reward(expensive_trade).unwrap();
    assert_eq!(priced.price_origin, PriceOrigin::ProvidedVwap);
    assert_eq!(priced.tl_price_btc_atoms, 200_000);
    assert_eq!(priced.eligible_trade_btc_atoms, U / 5);
    assert!(priced.reward_value_btc_atoms < expensive_trade.net_fee_btc_atoms);
}

#[test]
fn fully_vested_rewards_are_tl_and_halve_at_each_btc_decade() {
    let full = 21_000_000 * U;
    let a = quote_reward(observation(full)).unwrap();
    let b = quote_reward(observation(full * 10)).unwrap();
    let c = quote_reward(observation(full * 100)).unwrap();
    assert_eq!(a.reward_property_id, BITCOIN_TL_PROPERTY_ID);
    assert_eq!(a.phase, RewardPhase::FullyVested);
    assert_eq!(a.decade_halvings, 0);
    assert_eq!(b.decade_halvings, 1);
    assert_eq!(c.decade_halvings, 2);
    assert_eq!(b.fee_share_ppm, a.fee_share_ppm / 2);
    assert_eq!(c.fee_share_ppm, a.fee_share_ppm / 4);
    assert_eq!(a.reward_value_btc_atoms, a.fee_share_ppm / 10);
    assert!(a.reward_value_btc_atoms < observation(full).net_fee_btc_atoms);

    let before_decade = quote_reward(observation(full * 10 - 1)).unwrap();
    assert_eq!(before_decade.decade_halvings, 0);
    assert_eq!(decade_halvings(full * 100, full).unwrap(), 2);
}

#[test]
fn malformed_prices_and_decade_zero_are_rejected() {
    let mut x = observation(100 * U);
    x.tl_vwap_price_btc_atoms = Some(0);
    assert!(quote_reward(x).is_err());
    x.tl_vwap_price_btc_atoms = None;
    x.vest_complete_btc_atoms = 0;
    assert!(quote_reward(x).is_err());
    assert!(decade_halvings(100, 0).is_err());

    let mut zero = observation(0);
    zero.net_fee_btc_atoms = 0;
    assert_eq!(quote_reward(zero).unwrap().reward_token_atoms, 0);
}
