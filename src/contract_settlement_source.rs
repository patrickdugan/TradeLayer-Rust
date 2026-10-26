//! Pinned Orderbook contract settlement over explicit live source owners.
//! Financial effects belong to SourceBackend; this module owns no balances.
use crate::decimal::{Decimal, Rounding};
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};
use core::str::FromStr;
pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98";
const MAX_CALLS: usize = 16384;
const MAX_BYTES: usize = 16 * 1024 * 1024;
type Eval<T> = Result<T, Fault>;
#[derive(Debug)]
enum Fault {
    Local(SourceRuleError),
    Source(SourceException),
}
impl From<SourceRuleError> for Fault {
    fn from(e: SourceRuleError) -> Self {
        Self::Local(e)
    }
}
fn throw(name: &str, message: impl Into<String>) -> Fault {
    Fault::Source(SourceException {
        name: name.into(),
        message: message.into(),
    })
}
fn s(v: &str) -> V {
    V::String(v.into())
}
fn n(v: f64) -> V {
    V::number(v)
}
fn object(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn truth(v: &V) -> bool {
    match v {
        V::Undefined | V::Null | V::Bool(false) => false,
        V::Number(b) => {
            let x = f64::from_bits(*b);
            x != 0.0 && !x.is_nan()
        }
        V::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn nullish(v: &V) -> bool {
    matches!(v, V::Null | V::Undefined)
}
fn js_text(v: &V) -> String {
    match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::Number(b) => {
            let x = f64::from_bits(*b);
            if x.is_nan() {
                "NaN".into()
            } else if x == f64::INFINITY {
                "Infinity".into()
            } else if x == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                crate::state_oracle::source_number_text(x).unwrap_or_else(|_| "NaN".into())
            }
        }
        V::String(s) | V::BigNumber(s) => s.clone(),
        V::Array(a) => a
            .iter()
            .map(|v| {
                if nullish(v) {
                    String::new()
                } else {
                    js_text(v)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        V::Object(_) => "[object Object]".into(),
        V::Promise => "[object Promise]".into(),
    }
}
fn js_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn prefixed_integer(text: &str, radix: u32) -> f64 {
    if text.is_empty() || !text.chars().all(|c| c.is_digit(radix)) {
        return f64::NAN;
    }
    let significant = text.trim_start_matches('0');
    let binary_bound = significant.len().saturating_mul(match radix {
        16 => 4,
        8 => 3,
        _ => 1,
    });
    if binary_bound > 1028 {
        return f64::INFINITY;
    }
    let mut value = Decimal::zero();
    let base = Decimal::from(radix as i128);
    for c in significant.chars() {
        let Some(d) = c.to_digit(radix) else {
            return f64::NAN;
        };
        let Ok(v) = value
            .checked_mul(&base)
            .and_then(|v| v.checked_add(&Decimal::from(d as i128)))
        else {
            return f64::NAN;
        };
        value = v
    }
    value.to_string().parse().unwrap_or(f64::NAN)
}
fn num(v: &V) -> f64 {
    match v {
        V::Number(b) => f64::from_bits(*b),
        V::Null => 0.0,
        V::Bool(b) => f64::from(*b),
        V::String(_) | V::Array(_) | V::BigNumber(_) => {
            let t = js_text(v);
            let t = t.trim_matches(js_space);
            if t.is_empty() {
                0.0
            } else if t == "Infinity" || t == "+Infinity" {
                f64::INFINITY
            } else if t == "-Infinity" {
                f64::NEG_INFINITY
            } else if let Some(x) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                prefixed_integer(x, 16)
            } else if let Some(x) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
                prefixed_integer(x, 2)
            } else if let Some(x) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
                prefixed_integer(x, 8)
            } else if t
                .bytes()
                .any(|b| !b.is_ascii_digit() && !b"+-.eE".contains(&b))
            {
                f64::NAN
            } else {
                t.parse().unwrap_or(f64::NAN)
            }
        }
        _ => f64::NAN,
    }
}
fn strict(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Number(a), V::Number(b)) => f64::from_bits(*a) == f64::from_bits(*b),
        (V::Undefined, V::Undefined) | (V::Null, V::Null) => true,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::String(a), V::String(b)) => a == b,
        _ => false,
    }
}
fn loose(a: &V, b: &V) -> bool {
    if nullish(a) || nullish(b) {
        nullish(a) && nullish(b)
    } else if matches!(
        (a, b),
        (V::String(_), V::String(_)) | (V::Bool(_), V::Bool(_))
    ) {
        strict(a, b)
    } else {
        num(a) == num(b)
    }
}
fn decompose(old: &V, amount: &V, buyer: bool) -> (f64, f64, V) {
    let incoming = if buyer { num(amount) } else { -num(amount) };
    let old_number = num(old);
    let sign = |v: f64| {
        if v == 0.0 || v.is_nan() {
            v
        } else {
            v.signum()
        }
    };
    let same = sign(old_number) == sign(incoming) || strict(old, &n(0.0));
    if !same && !strict(&old.get("amount"), &n(0.0)) {
        let closed = min(old_number.abs(), incoming.abs());
        let flipped = incoming.abs() - closed;
        let position = if flipped == 0.0 {
            add(old, &n(incoming))
        } else {
            n(sign(incoming) * flipped)
        };
        (closed, flipped, position)
    } else {
        (0.0, 0.0, add(old, &n(incoming)))
    }
}
impl<B: SourceBackend> Engine<'_, B> {
    fn process(&mut self, args: &[V]) -> Eval<V> {
        let matches = match arg(args, 0) {
            V::Array(a) => a,
            _ => vec![],
        };
        let block = arg(args, 1);
        let channel = arg(args, 2);
        let last = match arg(args, 3) {
            V::Undefined => V::Null,
            v => v,
        };
        let mut trades = vec![];
        for mut m in matches {
            let mut buy = prop(&m, "buyOrder")?;
            let mut sell = prop(&m, "sellOrder")?;
            let liquidation = truth(&prop(&buy, "isLiq")?) || truth(&prop(&sell, "isLiq")?);
            let buyer = prop(&buy, "buyerAddress")?;
            let seller = prop(&sell, "sellerAddress")?;
            if loose(&buyer, &seller) {
                continue;
            }
            let contract = prop(&sell, "contractId")?;
            let buy_contract = prop(&buy, "contractId")?;
            let price = prop(&m, "tradePrice")?;
            let handle = self.call("Margin.loadMarginMap", vec![contract.clone()])?;
            let inverse = self.call("Contract.isInverse", vec![contract.clone()])?;
            let info = self.call(
                "Clearing.isPriceUpdatedForBlockHeight",
                vec![contract.clone(), block.clone()],
            )?;
            let mut last_price = prop(&info, "lastPrice")?;
            let this_price = prop(&info, "thisPrice")?;
            if liquidation && !nullish(&last) {
                last_price = last.clone();
            }
            set(&mut m, "inverse", inverse.clone())?;
            let collateral = self.call("Contract.getCollateralId", vec![buy_contract.clone()])?;
            let notional = self.call(
                "Contract.getNotionalValue",
                vec![contract.clone(), price.clone()],
            )?;
            let notional_value = prop(&notional, "notionalValue")?;
            let per_contract = prop(&notional, "notionalPerContract")?;
            self.call("Tally.getTally", vec![seller.clone(), collateral.clone()])?;
            self.call("Tally.getTally", vec![buyer.clone(), collateral.clone()])?;
            let mut bp = self.margin(
                &handle,
                "getPositionForAddress",
                vec![buyer.clone(), buy_contract.clone()],
            )?;
            let mut sp = self.margin(
                &handle,
                "getPositionForAddress",
                vec![seller.clone(), buy_contract.clone()],
            )?;
            if loose(&prop(&bp, "address")?, &V::Undefined) {
                set(&mut bp, "address", buyer.clone())?;
            }
            if loose(&prop(&sp, "address")?, &V::Undefined) {
                set(&mut sp, "address", seller.clone())?;
            }
            set(&mut m, "buyerPosition", bp.clone())?;
            set(&mut m, "sellerPosition", sp.clone())?;
            let buyer_reducing = num(&prop(&bp, "contracts")?) < 0.0;
            let seller_reducing = num(&prop(&sp, "contracts")?) > 0.0;
            let fee_input = object(&[
                ("amountBuy", prop(&buy, "amount")?),
                ("amountSell", prop(&sell, "amount")?),
                ("buyMaker", prop(&buy, "maker")?),
                ("sellMaker", prop(&sell, "maker")?),
                ("isInverse", inverse.clone()),
                ("lastMark", price.clone()),
                ("notionalValue", notional_value.clone()),
                ("channel", channel.clone()),
            ]);
            let (buyer_fee, seller_fee) = fee_pair(&fee_input)?;
            if buyer_fee.gt0() && seller_fee.lt0() {
                self.call(
                    "Tally.updateFeeCache",
                    vec![
                        collateral.clone(),
                        n(buyer_fee.clone().op(Bn::new(&n(2.0))?, 3)?.number()),
                        buy_contract.clone(),
                        block.clone(),
                        V::Bool(true),
                    ],
                )?;
            }
            if seller_fee.gt0() && buyer_fee.lt0() {
                self.call(
                    "Tally.updateFeeCache",
                    vec![
                        collateral.clone(),
                        n(seller_fee.clone().op(Bn::new(&n(2.0))?, 3)?.number()),
                        contract.clone(),
                        block.clone(),
                        V::Bool(true),
                    ],
                )?;
            }
            if buyer_fee.gt0() && seller_fee.gt0() {
                self.call(
                    "Tally.updateFeeCache",
                    vec![
                        collateral.clone(),
                        n(seller_fee.number()),
                        contract.clone(),
                        block.clone(),
                    ],
                )?;
                self.call(
                    "Tally.updateFeeCache",
                    vec![
                        collateral.clone(),
                        n(buyer_fee.number()),
                        contract.clone(),
                        block.clone(),
                    ],
                )?;
            }
            let fee_info = self.locate(&m, &collateral, &buyer_fee, &seller_fee, &block)?;
            let (buyer_closed, mut flip_long, buyer_new) = decompose(
                &or(prop(&bp, "contracts")?, n(0.0)),
                &prop(&buy, "amount")?,
                true,
            );
            let (seller_closed, mut flip_short, seller_new) = decompose(
                &or(prop(&sp, "contracts")?, n(0.0)),
                &prop(&sell, "amount")?,
                false,
            );
            let initial_margin = self.call(
                "Contract.getInitialMargin",
                vec![buy_contract.clone(), price.clone()],
            )?;
            let buyer_flipping = flip_long > 0.0;
            let seller_flipping = flip_short > 0.0;
            let buyer_full = strict(&buyer_new, &n(0.0)) && buyer_closed > 0.0;
            let seller_full = strict(&seller_new, &n(0.0)) && seller_closed > 0.0;
            for is_buyer in [true, false] {
                let flipping = if is_buyer {
                    buyer_flipping
                } else {
                    seller_flipping
                };
                if !flipping {
                    continue;
                }
                let (address, order, position, fee, closed, flipped) = if is_buyer {
                    (
                        &buyer,
                        &mut buy,
                        &bp,
                        &buyer_fee,
                        buyer_closed,
                        &mut flip_long,
                    )
                } else {
                    (
                        &seller,
                        &mut sell,
                        &sp,
                        &seller_fee,
                        seller_closed,
                        &mut flip_short,
                    )
                };
                let margin_flag = truth(&prop(
                    &fee_info,
                    if is_buyer {
                        "buyFeeFromMargin"
                    } else {
                        "sellFeeFromMargin"
                    },
                )?);
                if is_buyer && margin_flag {
                    set(
                        order,
                        "marginUsed",
                        n(Bn::new(&prop(order, "marginUsed")?)?
                            .op(fee.clone(), 1)?
                            .q(8, Rounding::HalfUp)?
                            .number()),
                    )?;
                }
                let mut required = Bn::new(&initial_margin)?.op(Bn::new(&n(*flipped))?, 2)?;
                let mut required_value = if is_buyer {
                    required.value()
                } else {
                    n(required.clone().q(8, Rounding::HalfUp)?.number())
                };
                if !truth(&channel) {
                    let release = Bn::new(&initial_margin)?.op(Bn::new(&n(closed))?, 2)?;
                    let difference = release
                        .clone()
                        .op(Bn::new(&required_value)?, 1)?
                        .q(8, Rounding::HalfUp)?
                        .number();
                    if difference > 0.0 {
                        self.call(
                            "Tally.updateBalance",
                            vec![
                                address.clone(),
                                collateral.clone(),
                                release.value(),
                                neg(&release.value()),
                                n(0.0),
                                n(0.0),
                                s("contractMarginRelease"),
                                block.clone(),
                            ],
                        )?;
                    } else {
                        required_value = n(num(&required_value) - (-difference));
                    }
                } else {
                    let difference = Bn::new(&required_value)?
                        .op(Bn::new(&or(prop(position, "margin")?, n(0.0)))?, 1)?
                        .q(8, Rounding::HalfUp)?
                        .number();
                    if difference != 0.0 {
                        self.call(
                            "Tally.updateBalance",
                            vec![
                                address.clone(),
                                collateral.clone(),
                                n(-difference),
                                n(0.0),
                                n(difference),
                                n(0.0),
                                s("contractTradeInitMargin_channelFlip"),
                                block.clone(),
                            ],
                        )?;
                    }
                }
                if !is_buyer && margin_flag {
                    required_value = n(Bn::new(&required_value)?
                        .op(fee.clone(), 1)?
                        .q(8, Rounding::HalfUp)?
                        .number());
                }
                let sufficient = self.call(
                    "Tally.hasSufficientBalance",
                    vec![address.clone(), collateral.clone(), required_value.clone()],
                )?;
                if !truth(&prop(&sufficient, "hasSufficient")?) {
                    let global = self.call("SourceGlobal.get", vec![s("hasSufficientBalance")])?;
                    let quotient =
                        Bn::new(&prop(&global, "shortfall")?)?.op(Bn::new(&initial_margin)?, 3)?;
                    let mut undo = quotient.clone().q(0, Rounding::Down)?;
                    if quotient.gt0() && quotient.compare(&undo) != Some(core::cmp::Ordering::Equal)
                    {
                        undo = undo.op(Bn::new(&n(1.0))?, 0)?;
                    }
                    *flipped -= undo.number();
                    required = Bn::new(&initial_margin)?.op(Bn::new(&n(*flipped))?, 2)?;
                    required_value = n(required.q(8, Rounding::HalfUp)?.number());
                }
                self.call(
                    "Tally.updateBalance",
                    vec![
                        address.clone(),
                        collateral.clone(),
                        neg(&required_value),
                        n(0.0),
                        required_value.clone(),
                        n(0.0),
                        s("contractTradeInitMargin"),
                        block.clone(),
                    ],
                )?;
                self.margin(
                    &handle,
                    "setInitialMargin",
                    vec![address.clone(), prop(order, "contractId")?, required_value],
                )?;
                let total = if is_buyer {
                    add(&prop(position, "contracts")?, &prop(order, "amount")?)
                } else {
                    n(num(&prop(position, "contracts")?) - num(&prop(order, "amount")?))
                };
                self.margin(
                    &handle,
                    "recordMarginMapDelta",
                    vec![
                        address.clone(),
                        prop(order, "contractId")?,
                        total,
                        prop(order, "amount")?,
                        n(0.0),
                        n(0.0),
                        n(0.0),
                        s("updateContractBalancesFlip"),
                        block.clone(),
                    ],
                )?;
                self.call("Tally.getTally", vec![address.clone(), collateral.clone()])?;
            }
            set(&mut m, "buyOrder", buy.clone())?;
            set(&mut m, "sellOrder", sell.clone())?;
            let channel_false = loose(&channel, &V::Bool(false));
            let channel_true = loose(&channel, &V::Bool(true));
            if !buyer_reducing && !truth(&prop(&buy, "liq")?) && (channel_false || channel_true) {
                let entry_price = if channel_false {
                    price.clone()
                } else {
                    prop(&buy, "price")?
                };
                let channel_address = if channel_false {
                    V::Null
                } else {
                    prop(&m, "channelAddress")?
                };
                bp = self.call(
                    "Contract.moveCollateralToMargin",
                    vec![
                        buyer.clone(),
                        buy_contract.clone(),
                        prop(&buy, "amount")?,
                        entry_price,
                        prop(&buy, "price")?,
                        V::Bool(false),
                        prop(&buy, "marginUsed")?,
                        channel.clone(),
                        channel_address,
                        block.clone(),
                        fee_info.clone(),
                        prop(&buy, "maker")?,
                        V::Bool(false),
                        or(prop(&m, "txid")?, s("")),
                        bp,
                    ],
                )?;
            }
            if !seller_reducing && !truth(&prop(&sell, "liq")?) && (channel_false || channel_true) {
                let entry_price = if channel_false {
                    price.clone()
                } else {
                    prop(&sell, "price")?
                };
                let channel_address = if channel_false {
                    V::Null
                } else {
                    prop(&m, "channelAddress")?
                };
                sp = self.call(
                    "Contract.moveCollateralToMargin",
                    vec![
                        seller.clone(),
                        contract.clone(),
                        prop(&sell, "amount")?,
                        entry_price,
                        prop(&sell, "price")?,
                        V::Bool(true),
                        prop(&sell, "marginUsed")?,
                        channel.clone(),
                        channel_address,
                        block.clone(),
                        fee_info.clone(),
                        prop(&buy, "maker")?,
                        sp,
                    ],
                )?;
            }
            set(&mut m, "buyerPosition", bp.clone())?;
            set(&mut m, "sellerPosition", sp.clone())?;
            let buyer_avg = prop(&bp, "avgPrice")?;
            let seller_avg = prop(&sp, "avgPrice")?;
            let positions = self.margin(
                &handle,
                "updateContractBalancesWithMatch",
                vec![
                    m.clone(),
                    channel.clone(),
                    n(buyer_closed),
                    n(flip_long),
                    n(seller_closed),
                    n(flip_short),
                    block.clone(),
                ],
            )?;
            bp = prop(&positions, "bp")?;
            sp = prop(&positions, "sp")?;
            let mut trade = object(&[
                ("buyerPosition", bp.clone()),
                ("sellerPosition", sp.clone()),
                (
                    "buyerFee",
                    n(buyer_fee.clone().q(8, Rounding::Down)?.number()),
                ),
                (
                    "sellerFee",
                    n(seller_fee.clone().q(8, Rounding::Down)?.number()),
                ),
                ("contractId", contract.clone()),
                ("amount", prop(&sell, "amount")?),
                ("price", price.clone()),
                ("buyerAddress", buyer.clone()),
                ("sellerAddress", seller.clone()),
                ("sellerTx", prop(&sell, "sellerTx")?),
                ("buyerTx", prop(&buy, "buyerTx")?),
                ("buyerClose", n(buyer_closed)),
                ("sellerClose", n(seller_closed)),
                ("block", block.clone()),
                ("buyerFullClose", V::Bool(buyer_full)),
                ("sellerFullClose", V::Bool(seller_full)),
                ("flipLong", n(flip_long)),
                ("flipShort", n(flip_short)),
                ("channel", channel.clone()),
                (
                    "liquidation",
                    V::Bool(truth(&prop(&sell, "liq")?) || truth(&prop(&buy, "liq")?)),
                ),
                ("remainderLiq", n(0.0)),
            ]);
            let trade_amount = prop(&buy, "amount")?;
            let buyer_opened_delta = if flip_long > 0.0 {
                flip_long
            } else {
                num(&trade_amount) - buyer_closed
            };
            let seller_opened_delta = if flip_short > 0.0 {
                flip_short
            } else {
                num(&trade_amount) - seller_closed
            };
            let buyer_record = self.dep(
                "Clearing.recordTrade",
                vec![
                    contract.clone(),
                    buyer.clone(),
                    n(buyer_opened_delta),
                    n(buyer_closed),
                    price.clone(),
                    prop(&buy, "txid")?,
                    V::Bool(true),
                ],
                false,
            )?;
            let seller_record = self.dep(
                "Clearing.recordTrade",
                vec![
                    contract.clone(),
                    seller.clone(),
                    n(seller_opened_delta),
                    n(seller_closed),
                    price.clone(),
                    prop(&sell, "txid")?,
                    V::Bool(false),
                ],
                false,
            )?;
            prop(&buyer_record, "consumedFromOpened")?;
            prop(&seller_record, "consumedFromOpened")?;
            for (address, opened, is_buyer) in [
                (
                    &buyer,
                    max(
                        0.0,
                        num(&or(prop(&trade, "amount")?, n(0.0)))
                            - num(&or(prop(&trade, "buyerClose")?, n(0.0))),
                    ),
                    true,
                ),
                (
                    &seller,
                    max(
                        0.0,
                        num(&or(prop(&trade, "amount")?, n(0.0)))
                            - num(&or(prop(&trade, "sellerClose")?, n(0.0))),
                    ),
                    false,
                ),
            ] {
                if opened > 0.0 {
                    let exit = if liquidation {
                        this_price.clone()
                    } else {
                        last_price.clone()
                    };
                    let signed = if is_buyer { opened } else { -opened };
                    let pnl = self.margin(
                        &handle,
                        "settlePNL",
                        vec![
                            address.clone(),
                            n(signed),
                            exit,
                            price.clone(),
                            contract.clone(),
                            block.clone(),
                            inverse.clone(),
                            per_contract.clone(),
                        ],
                    )?;
                    if num(&pnl) > 0.0 {
                        let reason = if is_buyer {
                            "buyerNewContractTieOff"
                        } else if liquidation {
                            "sellerNewContractTieOffLiq"
                        } else {
                            "sellerNewContractTieOff"
                        };
                        self.call(
                            "Tally.updateBalance",
                            vec![
                                address.clone(),
                                collateral.clone(),
                                pnl,
                                n(0.0),
                                n(0.0),
                                n(0.0),
                                s(reason),
                                block.clone(),
                            ],
                        )?;
                    } else {
                        self.loss(&[
                            address.clone(),
                            collateral.clone(),
                            n(num(&pnl).abs()),
                            block.clone(),
                            contract.clone(),
                            V::Bool(true),
                        ])?;
                    }
                }
            }
            // These references were established by WithMatch; this is an adapter-only
            // projection of their live objects, not a new source financial read.
            bp = self.inspect(&handle, &buyer)?;
            sp = self.inspect(&handle, &seller)?;
            set(&mut trade, "buyerPosition", bp.clone())?;
            set(&mut trade, "sellerPosition", sp.clone())?;
            self.call(
                "Orderbook.recordContractTrade",
                vec![trade.clone(), block.clone()],
            )?;
            let mut buyer_pnl = Bn::new(&n(0.0))?;
            let mut seller_pnl = Bn::new(&n(0.0))?;
            let mut buyer_loss = n(0.0);
            let mut seller_loss = n(0.0);
            let mut buyer_profit = n(0.0);
            let mut seller_profit = n(0.0);
            for is_buyer in [true, false] {
                let reducing = if is_buyer {
                    buyer_reducing
                } else {
                    seller_reducing
                };
                let flipping = if is_buyer {
                    buyer_flipping
                } else {
                    seller_flipping
                };
                if !reducing && !flipping {
                    continue;
                }
                let (address, order, pos, avg_before, closed, flipped, fee, margin_key) =
                    if is_buyer {
                        (
                            &buyer,
                            &buy,
                            &mut bp,
                            &buyer_avg,
                            buyer_closed,
                            flip_long,
                            &buyer_fee,
                            "buyFeeFromMargin",
                        )
                    } else {
                        (
                            &seller,
                            &sell,
                            &mut sp,
                            &seller_avg,
                            seller_closed,
                            flip_short,
                            &seller_fee,
                            "sellFeeFromMargin",
                        )
                    };
                let closed_contracts = if flipping {
                    n(num(&prop(order, "amount")?) - flipped)
                } else {
                    prop(order, "amount")?
                };
                let avg = or(avg_before.clone(), prop(pos, "avgPrice")?);
                *pos = self.alias_margin(
                    &handle,
                    "realizePnl",
                    vec![
                        address.clone(),
                        closed_contracts.clone(),
                        price.clone(),
                        avg.clone(),
                        inverse.clone(),
                        per_contract.clone(),
                        pos.clone(),
                        V::Bool(is_buyer),
                        prop(order, "contractId")?,
                    ],
                    6,
                    address,
                )?;
                let pnl = self.margin(
                    &handle,
                    "settlePNL",
                    vec![
                        address.clone(),
                        n(if is_buyer { -closed } else { closed }),
                        price.clone(),
                        last_price.clone(),
                        contract.clone(),
                        block.clone(),
                        inverse.clone(),
                        per_contract.clone(),
                    ],
                )?;
                *pos = self.inspect(&handle, address)?;
                let reduction = self.alias_margin(
                    &handle,
                    "reduceMargin",
                    vec![
                        pos.clone(),
                        closed_contracts.clone(),
                        initial_margin.clone(),
                        prop(order, "contractId")?,
                        address.clone(),
                        V::Bool(false),
                        prop(&fee_info, margin_key)?,
                        fee.value(),
                    ],
                    0,
                    address,
                )?;
                let enough = self.call(
                    "Tally.hasSufficientMargin",
                    vec![address.clone(), collateral.clone(), reduction.clone()],
                )?;
                if !strict(&reduction, &n(0.0)) && truth(&prop(&enough, "hasSufficient")?) {
                    self.call(
                        "Tally.updateBalance",
                        vec![
                            address.clone(),
                            collateral.clone(),
                            reduction.clone(),
                            n(0.0),
                            neg(&reduction),
                            n(0.0),
                            s("contractTradeMarginReturn"),
                            block.clone(),
                        ],
                    )?;
                }
                let mut loss = n(0.0);
                let mut profit = n(0.0);
                if num(&pnl) < 0.0 {
                    let recovery = self.loss(&[
                        address.clone(),
                        collateral.clone(),
                        n(num(&pnl).abs()),
                        block.clone(),
                        contract.clone(),
                    ])?;
                    loss = prop(&recovery, "totalUsed")?;
                    if num(&prop(&recovery, "remaining")?) > 0.0 {
                        set(&mut trade, "remainderLiq", prop(&recovery, "remainder")?)?;
                    }
                } else {
                    profit = pnl.clone();
                }
                let timestamp = self.call("Clock.nowISOString", vec![])?;
                let history = object(&[
                    ("height", block.clone()),
                    ("contractId", prop(order, "contractId")?),
                    ("accountingPNL", prop(pos, "realizedPNL")?),
                    ("isBuyer", V::Bool(is_buyer)),
                    ("address", address.clone()),
                    ("amount", closed_contracts),
                    ("tradePrice", price.clone()),
                    ("collateralPropertyId", collateral.clone()),
                    ("timestamp", timestamp),
                    (
                        "txid",
                        prop(order, if is_buyer { "buyerTx" } else { "sellerTx" })?,
                    ),
                    ("settlementPNL", pnl.clone()),
                    ("marginReduction", reduction),
                    ("avgEntry", avg),
                ]);
                self.dep("TradeHistory.savePNL", vec![history], false)?;
                if is_buyer {
                    buyer_pnl = Bn::new(&pnl)?;
                    buyer_loss = loss;
                    buyer_profit = profit;
                } else {
                    seller_pnl = Bn::new(&pnl)?;
                    seller_loss = loss;
                    seller_profit = profit;
                }
            }
            for is_buyer in [false, true] {
                let (address, profit, loss, opposite_reducing, opposite_flipping, key) = if is_buyer
                {
                    (
                        &buyer,
                        &buyer_profit,
                        &seller_loss,
                        seller_reducing,
                        seller_flipping,
                        "buyerDeferredPnl",
                    )
                } else {
                    (
                        &seller,
                        &seller_profit,
                        &buyer_loss,
                        buyer_reducing,
                        buyer_flipping,
                        "sellerDeferredPnl",
                    )
                };
                if is_buyer && num(profit) <= 0.0 {
                    continue;
                }
                let profit_bn = Bn::new(profit)?;
                if liquidation {
                    self.call(
                        "Tally.updateBalance",
                        vec![
                            address.clone(),
                            collateral.clone(),
                            n(profit_bn.number()),
                            n(0.0),
                            n(0.0),
                            n(0.0),
                            s("liquidationProfit"),
                            block.clone(),
                        ],
                    )?;
                } else {
                    let mut immediate = profit_bn.clone().min(Bn::new(loss)?);
                    let deferred = profit_bn.clone().op(immediate.clone(), 1)?;
                    if !opposite_reducing || opposite_flipping {
                        immediate = profit_bn;
                    }
                    if immediate.gt0() {
                        self.call(
                            "Tally.updateBalance",
                            vec![
                                address.clone(),
                                collateral.clone(),
                                n(immediate.number()),
                                n(0.0),
                                n(0.0),
                                n(0.0),
                                s("contractTradeSettlement"),
                                block.clone(),
                            ],
                        )?;
                    }
                    if deferred.gt0() {
                        set(&mut trade, key, n(deferred.number()))?;
                    }
                }
            }
            let ltc = self.call(
                "VolumeIndex.getContractUnitLTCValue",
                vec![contract.clone()],
            )?;
            let total = Bn::new(&ltc)?
                .op(Bn::new(&prop(&trade, "amount")?)?, 2)?
                .q(8, Rounding::HalfUp)?
                .number();
            if !total.is_finite() {
                return Err(throw(
                    "Error",
                    format!("{} {}", js_text(&ltc), js_text(&prop(&trade, "amount")?)),
                ));
            }
            if loose(&ltc, &n(0.0)) {
                return Err(throw("Error", ""));
            }
            self.call(
                "VolumeIndex.saveVolumeDataById",
                vec![
                    contract.clone(),
                    prop(&trade, "amount")?,
                    n(total),
                    price.clone(),
                    block.clone(),
                    s("contract"),
                ],
            )?;
            set(&mut m, "buyerPosition", bp.clone())?;
            set(&mut m, "sellerPosition", sp.clone())?;
            let (basic, enhanced) = self.reward_flags(&m, &channel)?;
            for (enabled, method, reason) in [
                (
                    basic,
                    "VolumeIndex.baselineLiquidityReward",
                    "baselineLiquidityReward",
                ),
                (
                    enhanced,
                    "VolumeIndex.calculateLiquidityReward",
                    "enhancedLiquidityReward",
                ),
            ] {
                if enabled {
                    let tokens = n(num(&notional_value) * num(&prop(&trade, "amount")?));
                    let reward = self.call(
                        method,
                        if basic && method == "VolumeIndex.baselineLiquidityReward" {
                            vec![tokens, n(0.000025), collateral.clone()]
                        } else {
                            vec![tokens]
                        },
                    )?;
                    for address in [&seller, &buyer] {
                        self.dep(
                            "Tally.updateBalance",
                            vec![
                                address.clone(),
                                n(3.0),
                                reward.clone(),
                                n(0.0),
                                n(0.0),
                                n(0.0),
                                s(reason),
                            ],
                            false,
                        )?;
                    }
                }
            }
            self.margin(&handle, "saveMarginMap", vec![block.clone()])?;
            bp = self.inspect(&handle, &buyer)?;
            sp = self.inspect(&handle, &seller)?;
            set(&mut trade, "buyerPosition", bp)?;
            set(&mut trade, "sellerPosition", sp)?;
            let delta = buyer_pnl.clone().op(seller_pnl.clone(), 0)?;
            if !liquidation
                && (seller_reducing || seller_flipping)
                && (buyer_reducing || buyer_flipping)
            {
                if delta.gt0() {
                    self.call(
                        "PnlIou.addIouClaims",
                        vec![
                            contract.clone(),
                            collateral.clone(),
                            block.clone(),
                            buyer,
                            seller,
                            buyer_pnl.value(),
                            seller_pnl.value(),
                            delta.value(),
                        ],
                    )?;
                    self.call(
                        "PnlIou.addProfit",
                        vec![contract, collateral, delta.value(), block.clone()],
                    )?;
                } else if delta.lt0() {
                    self.call(
                        "PnlIou.addLoss",
                        vec![
                            contract,
                            collateral,
                            delta.clone().abs().value(),
                            block.clone(),
                        ],
                    )?;
                }
            }
            set(&mut trade, "delta", delta.value())?;
            trades.push(trade);
        }
        Ok(V::Array(trades))
    }
}
fn finish(result: Eval<V>) -> Result<SourceOutcome, SourceRuleError> {
    match result {
        Ok(v) => {
            v.validate()?;
            Ok(SourceOutcome::Returned(v))
        }
        Err(Fault::Source(e)) => Ok(SourceOutcome::Thrown(e)),
        Err(Fault::Local(e)) => Err(e),
    }
}
/// Executes the source method. Prefix writes and deferred rejections belong to
/// the live backend; this stateless entry never rolls those writes back.
pub fn process_contract_matches<B: SourceBackend>(
    arguments: &[V],
    backend: &mut B,
) -> Result<SourceOutcome, SourceRuleError> {
    if arguments.len() > 4 {
        return Err(SourceRuleError::Capacity);
    }
    for value in arguments {
        value.validate()?;
    }
    finish(
        Engine {
            backend,
            calls: 0,
            bytes: 0,
        }
        .process(arguments),
    )
}

/// Pure source fee helper, including its BigNumber return values and same-block
/// half-fee branch. It supplies no economic admission or balance authority.
pub fn calculate_fee(arguments: &[V]) -> Result<SourceOutcome, SourceRuleError> {
    if arguments.len() > 1 {
        return Err(SourceRuleError::Capacity);
    }
    let input = arg(arguments, 0);
    input.validate()?;
    finish(fee_pair(&input).map(|(buyer, seller)| {
        object(&[("buyerFee", buyer.value()), ("sellerFee", seller.value())])
    }))
}

/// The source loss helper over actual backend balance/cancellation operations.
pub fn source_funds_for_loss<B: SourceBackend>(
    arguments: &[V],
    backend: &mut B,
) -> Result<SourceOutcome, SourceRuleError> {
    if arguments.len() > 6 {
        return Err(SourceRuleError::Capacity);
    }
    for value in arguments {
        value.validate()?;
    }
    finish(
        Engine {
            backend,
            calls: 0,
            bytes: 0,
        }
        .loss(arguments),
    )
}
#[derive(Clone)]
enum Bn {
    Finite(Decimal),
    NegativeZero,
    Special(f64),
}
impl Bn {
    fn new(v: &V) -> Eval<Self> {
        if matches!(v,V::Number(bits)if *bits==(-0.0f64).to_bits()) {
            return Ok(Self::NegativeZero);
        }
        let text = match v {
            V::String(s) | V::BigNumber(s) => s.trim_matches(js_space).to_string(),
            V::Number(_) => js_text(v),
            _ => return Ok(Self::Special(f64::NAN)),
        };
        if text.len() > 512 {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        let unsigned = text.trim_start_matches(['+', '-']);
        if ["0x", "0X", "0b", "0B", "0o", "0O"]
            .iter()
            .any(|p| unsigned.starts_with(p))
        {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        if let Ok(d) = Decimal::from_str(&text) {
            if d.is_zero() && text.starts_with('-') {
                Ok(Self::NegativeZero)
            } else {
                Ok(Self::Finite(d))
            }
        } else {
            if matches!(
                Decimal::from_str(&text),
                Err(crate::decimal::DecimalError::LimitExceeded)
            ) {
                return Err(SourceRuleError::UnsupportedNumericDomain.into());
            }
            Ok(Self::Special(match text.as_str() {
                "Infinity" | "+Infinity" => f64::INFINITY,
                "-Infinity" => f64::NEG_INFINITY,
                _ => f64::NAN,
            }))
        }
    }
    fn number(&self) -> f64 {
        match self {
            Self::Finite(d) => d.to_string().parse().unwrap_or(f64::NAN),
            Self::NegativeZero => -0.0,
            Self::Special(n) => *n,
        }
    }
    fn text(&self) -> String {
        match self {
            Self::Finite(d) => d.to_string(),
            Self::NegativeZero => "0".into(),
            Self::Special(n) => js_text(&V::number(*n)),
        }
    }
    fn op(self, b: Self, kind: u8) -> Eval<Self> {
        let finite = |v: &Self| match v {
            Self::Finite(d) => Some(d.clone()),
            Self::NegativeZero => Some(Decimal::zero()),
            Self::Special(_) => None,
        };
        let aneg = matches!(self, Self::NegativeZero)
            || matches!(&self,Self::Finite(d)if d.is_negative())
            || matches!(&self,Self::Special(n)if *n<0.0);
        let bneg = matches!(b, Self::NegativeZero)
            || matches!(&b,Self::Finite(d)if d.is_negative())
            || matches!(&b,Self::Special(n)if *n<0.0);
        if let (Some(a), Some(b)) = (finite(&self), finite(&b)) {
            if kind == 3 && b.is_zero() {
                return Ok(Self::Special(if a.is_zero() {
                    f64::NAN
                } else if aneg ^ bneg {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                }));
            }
            let d = match kind {
                0 => a.checked_add(&b),
                1 => a.checked_sub(&b),
                2 => a.checked_mul(&b),
                _ => a.checked_div(&b, 20, Rounding::HalfUp),
            }
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
            let negzero = d.is_zero()
                && match kind {
                    0 => a.is_zero() && b.is_zero() && aneg && bneg,
                    1 => a.is_zero() && b.is_zero() && aneg && !bneg,
                    _ => aneg ^ bneg,
                };
            Ok(if negzero {
                Self::NegativeZero
            } else {
                Self::Finite(d)
            })
        } else {
            let a_special = match &self {
                Self::Special(n) => Some(*n),
                _ => None,
            };
            let b_special = match &b {
                Self::Special(n) => Some(*n),
                _ => None,
            };
            if a_special.is_some_and(f64::is_nan) || b_special.is_some_and(f64::is_nan) {
                return Ok(Self::Special(f64::NAN));
            }
            let signed_infinity = || {
                Self::Special(if aneg ^ bneg {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                })
            };
            Ok(match kind {
                0 => match (a_special, b_special) {
                    (Some(a), Some(b)) => Self::Special(a + b),
                    (Some(a), None) => Self::Special(a),
                    (None, Some(b)) => Self::Special(b),
                    _ => unreachable!(),
                },
                1 => match (a_special, b_special) {
                    (Some(a), Some(b)) => Self::Special(a - b),
                    (Some(a), None) => Self::Special(a),
                    (None, Some(b)) => Self::Special(-b),
                    _ => unreachable!(),
                },
                2 if finite(&self).is_some_and(|d| d.is_zero())
                    || finite(&b).is_some_and(|d| d.is_zero()) =>
                {
                    Self::Special(f64::NAN)
                }
                2 => signed_infinity(),
                _ if a_special.is_some() && b_special.is_some() => Self::Special(f64::NAN),
                _ if b_special.is_some() => {
                    if aneg ^ bneg {
                        Self::NegativeZero
                    } else {
                        Self::Finite(Decimal::zero())
                    }
                }
                _ => signed_infinity(),
            })
        }
    }
    fn q(self, places: u32, rounding: Rounding) -> Eval<Self> {
        match self {
            Self::Finite(d) => {
                let q = d
                    .quantize(places, rounding)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                Ok(if q.is_zero() && d.is_negative() {
                    Self::NegativeZero
                } else {
                    Self::Finite(q)
                })
            }
            _ => Ok(self),
        }
    }
    fn abs(self) -> Self {
        match self {
            Self::Finite(d) => Self::Finite(d.abs()),
            Self::NegativeZero => Self::Finite(Decimal::zero()),
            Self::Special(n) => Self::Special(n.abs()),
        }
    }
    fn compare(&self, other: &Self) -> Option<core::cmp::Ordering> {
        let decimal = |v: &Self| match v {
            Self::Finite(d) => Some(d.clone()),
            Self::NegativeZero => Some(Decimal::zero()),
            _ => None,
        };
        if let (Some(a), Some(b)) = (decimal(self), decimal(other)) {
            Some(a.cmp(&b))
        } else {
            match (self, other) {
                (Self::Special(a), Self::Special(b)) => a.partial_cmp(b),
                (Self::Special(a), _) => {
                    if a.is_nan() {
                        None
                    } else if *a > 0.0 {
                        Some(core::cmp::Ordering::Greater)
                    } else {
                        Some(core::cmp::Ordering::Less)
                    }
                }
                (_, Self::Special(b)) => {
                    if b.is_nan() {
                        None
                    } else if *b > 0.0 {
                        Some(core::cmp::Ordering::Less)
                    } else {
                        Some(core::cmp::Ordering::Greater)
                    }
                }
                _ => None,
            }
        }
    }
}
fn arg(a: &[V], i: usize) -> V {
    a.get(i).cloned().unwrap_or(V::Undefined)
}
fn prop(v: &V, key: &str) -> Eval<V> {
    if nullish(v) {
        Err(throw(
            "TypeError",
            format!("Cannot read properties of {} (reading '{key}')", js_text(v)),
        ))
    } else {
        Ok(v.get(key))
    }
}
fn set(v: &mut V, key: &str, value: V) -> Eval<()> {
    if matches!(v, V::Object(_)) {
        return v.insert(key, value).map_err(Fault::Local);
    }
    if nullish(v) {
        return Err(throw(
            "TypeError",
            format!("Cannot set properties of {} (setting '{key}')", js_text(v)),
        ));
    }
    let kind = match v {
        V::String(_) => "string",
        V::Number(_) => "number",
        V::Bool(_) => "boolean",
        _ => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
    };
    Err(throw(
        "TypeError",
        format!("Cannot create property '{key}' on {kind} '{}'", js_text(v)),
    ))
}
fn or(v: V, fallback: V) -> V {
    if truth(&v) {
        v
    } else {
        fallback
    }
}
fn neg(v: &V) -> V {
    n(-num(v))
}
fn add(a: &V, b: &V) -> V {
    if matches!(
        a,
        V::String(_) | V::Array(_) | V::Object(_) | V::Promise | V::BigNumber(_)
    ) || matches!(
        b,
        V::String(_) | V::Array(_) | V::Object(_) | V::Promise | V::BigNumber(_)
    ) {
        s(&(js_text(a) + &js_text(b)))
    } else {
        n(num(a) + num(b))
    }
}
fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}
fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}
impl Bn {
    fn value(&self) -> V {
        V::BigNumber(if matches!(self, Self::NegativeZero) {
            "-0".into()
        } else {
            self.text()
        })
    }
    fn negated(self) -> Self {
        match self {
            Self::Finite(d) if d.is_zero() => Self::NegativeZero,
            Self::Finite(d) => Self::Finite(d.negated()),
            Self::NegativeZero => Self::Finite(Decimal::zero()),
            Self::Special(n) => Self::Special(-n),
        }
    }
    fn cmp0(&self) -> Option<core::cmp::Ordering> {
        self.compare(&Self::Finite(Decimal::zero()))
    }
    fn gt0(&self) -> bool {
        self.cmp0() == Some(core::cmp::Ordering::Greater)
    }
    fn lt0(&self) -> bool {
        self.cmp0() == Some(core::cmp::Ordering::Less)
    }
    fn min(self, b: Self) -> Self {
        let zero =
            |v: &Self| matches!(v, Self::NegativeZero) || matches!(v,Self::Finite(d)if d.is_zero());
        if zero(&self)
            && zero(&b)
            && (matches!(self, Self::NegativeZero) || matches!(b, Self::NegativeZero))
        {
            return Self::NegativeZero;
        }
        match self.compare(&b) {
            Some(core::cmp::Ordering::Greater) => b,
            Some(_) => self,
            None => Self::Special(f64::NAN),
        }
    }
    fn floor(self) -> Eval<Self> {
        let q = self.clone().q(0, Rounding::Down)?;
        if self.lt0() && self.compare(&q) != Some(core::cmp::Ordering::Equal) {
            q.op(Self::new(&n(1.0))?, 1)
        } else {
            Ok(q)
        }
    }
    fn odd(&self) -> bool {
        match self {
            Self::Finite(d) => d
                .to_string()
                .as_bytes()
                .last()
                .is_some_and(|b| (b - b'0') % 2 == 1),
            _ => !self.number().is_finite(),
        }
    }
}
struct Engine<'a, B: SourceBackend> {
    backend: &'a mut B,
    calls: usize,
    bytes: usize,
}
impl<B: SourceBackend> Engine<'_, B> {
    fn dep(&mut self, target: &str, args: Vec<V>, awaited: bool) -> Eval<V> {
        self.calls += 1;
        if self.calls > MAX_CALLS {
            return Err(SourceRuleError::Capacity.into());
        }
        let aggregate = V::Array(args.clone());
        aggregate.validate()?;
        self.bytes = self
            .bytes
            .checked_add(aggregate.diagnostic_json()?.len() + target.len())
            .ok_or(SourceRuleError::Capacity)?;
        if self.bytes > MAX_BYTES {
            return Err(SourceRuleError::Capacity.into());
        }
        let returned = self.backend.invoke(&DependencyCall {
            target: target.into(),
            arguments: args,
        })?;
        match &returned {
            DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
                v.validate()?;
                self.bytes = self
                    .bytes
                    .checked_add(v.diagnostic_json()?.len())
                    .ok_or(SourceRuleError::Capacity)?;
            }
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                self.bytes = self
                    .bytes
                    .checked_add(e.name.len())
                    .and_then(|n| n.checked_add(e.message.len()))
                    .ok_or(SourceRuleError::Capacity)?;
            }
        }
        if self.bytes > MAX_BYTES {
            return Err(SourceRuleError::Capacity.into());
        }
        match returned {
            DependencyReturn::Immediate(v) => {
                v.validate()?;
                Ok(v)
            }
            DependencyReturn::Pending(v) => {
                v.validate()?;
                if awaited {
                    Ok(v)
                } else {
                    Ok(V::Promise)
                }
            }
            DependencyReturn::Throw(e) => Err(Fault::Source(e)),
            DependencyReturn::PendingThrow(e) => {
                if awaited {
                    Err(Fault::Source(e))
                } else {
                    Ok(V::Promise)
                }
            }
        }
    }
    fn call(&mut self, target: &str, args: Vec<V>) -> Eval<V> {
        self.dep(target, args, true)
    }
    fn margin(&mut self, handle: &V, method: &str, args: Vec<V>) -> Eval<V> {
        let mut all = vec![handle.clone()];
        all.extend(args);
        self.call(&format!("Margin.{method}"), all)
    }
    fn alias_margin(
        &mut self,
        handle: &V,
        method: &str,
        args: Vec<V>,
        index: usize,
        address: &V,
    ) -> Eval<V> {
        let mut alias = handle.clone();
        set(
            &mut alias,
            "sourcePositionAliases",
            V::Array(vec![object(&[
                ("argument", n(index as f64)),
                ("address", address.clone()),
            ])]),
        )?;
        self.margin(&alias, method, args)
    }
    fn inspect(&mut self, handle: &V, address: &V) -> Eval<V> {
        self.margin(handle, "inspectPosition", vec![address.clone()])
    }
}
fn fee_pair(input: &V) -> Eval<(Bn, Bn)> {
    let notional = Bn::new(&prop(input, "notionalValue")?)?;
    let mark = Bn::new(&prop(input, "lastMark")?)?;
    let amount_buy = Bn::new(&prop(input, "amountBuy")?)?;
    let amount_sell = Bn::new(&prop(input, "amountSell")?)?;
    let mut rate = Bn::new(&n(0.0005))?;
    if strict(&prop(input, "channel")?, &V::Bool(true)) {
        rate = rate.op(Bn::new(&n(10.0))?, 3)?;
    }
    let base = |amount: Bn| -> Eval<Bn> {
        if truth(&prop(input, "isInverse")?) {
            rate.clone()
                .op(notional.clone(), 2)?
                .op(mark.clone(), 3)?
                .op(amount, 2)
        } else {
            rate.clone()
                .op(mark.clone(), 2)?
                .op(notional.clone(), 3)?
                .op(amount, 2)
        }
    };
    let buy_maker = prop(input, "buyMaker")?;
    let sell_maker = prop(input, "sellMaker")?;
    let neither = !truth(&buy_maker) && !truth(&sell_maker);
    let buyer_taker = strict(&buy_maker, &V::Bool(false));
    let amount = if neither || buyer_taker {
        amount_buy
    } else {
        amount_sell
    };
    let mut sats = base(amount)?.abs().op(Bn::new(&n(1e8))?, 2)?.floor()?;
    if sats.odd() {
        sats = sats.op(Bn::new(&n(1.0))?, 0)?;
    }
    let half = sats
        .clone()
        .op(Bn::new(&n(2.0))?, 3)?
        .q(0, Rounding::Down)?;
    if neither {
        let fee = half.op(Bn::new(&n(1e8))?, 3)?;
        Ok((fee.clone(), fee))
    } else {
        let full = sats.op(Bn::new(&n(1e8))?, 3)?;
        let rebate = half.negated().op(Bn::new(&n(1e8))?, 3)?;
        Ok(if buyer_taker {
            (full, rebate)
        } else {
            (rebate, full)
        })
    }
}
impl<B: SourceBackend> Engine<'_, B> {
    fn loss(&mut self, args: &[V]) -> Eval<V> {
        let address = arg(args, 0);
        let property = arg(args, 1);
        let amount = arg(args, 2);
        let block = arg(args, 3);
        let contract = arg(args, 4);
        let tie = truth(&arg(args, 5));
        let tally = self.call("Tally.getTally", vec![address.clone(), property.clone()])?;
        if !truth(&tally) {
            return Ok(object(&[
                ("hasSufficient", V::Bool(false)),
                ("reason", s("undefined tally")),
                ("remaining", amount),
            ]));
        }
        let mut remaining = Bn::new(&amount)?;
        let mut breakdown = object(&[
            ("fromAvailable", n(0.0)),
            ("fromMarginCap", n(0.0)),
            ("fromReserve", n(0.0)),
            ("fromMarginFinal", n(0.0)),
        ]);
        let available = remaining
            .clone()
            .min(Bn::new(&or(prop(&tally, "available")?, n(0.0)))?);
        if available.gt0() {
            self.call(
                "Tally.updateBalance",
                vec![
                    address.clone(),
                    property.clone(),
                    neg(&available.value()),
                    n(0.0),
                    n(0.0),
                    n(0.0),
                    s(if tie {
                        "loss_from_available_tieOff"
                    } else {
                        "loss_from_available"
                    }),
                    block.clone(),
                ],
            )?;
            set(&mut breakdown, "fromAvailable", n(available.number()))?;
            remaining = remaining.op(available, 1)?;
        }
        if remaining.gt0() {
            let cap = Bn::new(&or(prop(&tally, "margin")?, n(0.0)))?.op(Bn::new(&n(0.499))?, 2)?;
            let used = remaining.clone().min(cap);
            if used.gt0() {
                self.call(
                    "Tally.updateBalance",
                    vec![
                        address.clone(),
                        property.clone(),
                        n(0.0),
                        n(0.0),
                        neg(&used.value()),
                        n(0.0),
                        s(if tie {
                            "loss_from_margin_tieOff"
                        } else {
                            "loss_from_margin"
                        }),
                        block.clone(),
                    ],
                )?;
                set(&mut breakdown, "fromMarginCap", n(used.number()))?;
                remaining = remaining.op(used, 1)?;
            }
        }
        if remaining.gt0() {
            let before = self.call("Tally.getTally", vec![address.clone(), property.clone()])?;
            let before_avail = Bn::new(&or(prop(&before, "available")?, n(0.0)))?;
            let before_reserve = Bn::new(&or(prop(&before, "reserved")?, n(0.0)))?;
            if before_reserve.gt0() {
                self.call(
                    "Orderbook.cancelExcessOrders",
                    vec![
                        address.clone(),
                        contract.clone(),
                        remaining.value(),
                        property.clone(),
                        block.clone(),
                    ],
                )?;
            }
            let after = self.call("Tally.getTally", vec![address.clone(), property.clone()])?;
            let after_avail = Bn::new(&or(prop(&after, "available")?, n(0.0)))?;
            let mut freed = after_avail.op(before_avail, 1)?;
            if freed.lt0() {
                freed = Bn::new(&n(0.0))?;
            }
            let used = remaining.clone().min(freed);
            if used.gt0() {
                self.call(
                    "Tally.updateBalance",
                    vec![
                        address.clone(),
                        property.clone(),
                        neg(&used.value()),
                        n(0.0),
                        n(0.0),
                        n(0.0),
                        s("loss_from_reserve"),
                        block.clone(),
                    ],
                )?;
                let total_reserve =
                    num(&or(prop(&breakdown, "fromReserve")?, n(0.0))) + used.number();
                set(&mut breakdown, "fromReserve", n(total_reserve))?;
                remaining = remaining.op(used, 1)?;
            }
            if remaining.gt0() {
                let cross = self.call(
                    "Orderbook.sourceCrossContractReserve",
                    vec![address, property, remaining.value(), contract, block],
                )?;
                let cross_remaining = prop(&cross, "remaining")?;
                set(
                    &mut breakdown,
                    "fromCrossReserve",
                    or(
                        prop(&prop(&cross, "breakdown")?, "fromCrossReserve")?,
                        n(0.0),
                    ),
                )?;
                if !matches!(cross_remaining, V::BigNumber(_)) {
                    return Err(if nullish(&cross_remaining) {
                        throw(
                            "TypeError",
                            format!(
                                "Cannot read properties of {} (reading 'lte')",
                                js_text(&cross_remaining)
                            ),
                        )
                    } else {
                        throw("TypeError", "remaining.lte is not a function")
                    });
                }
                remaining = Bn::new(&cross_remaining)?;
            }
        }
        let success = matches!(
            remaining.cmp0(),
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        );
        Ok(object(&[
            ("hasSufficient", V::Bool(success)),
            (
                "reason",
                s(if success {
                    ""
                } else {
                    "Insufficient total balance after all buckets"
                }),
            ),
            ("remaining", remaining.value()),
            ("totalUsed", n(num(&amount) - remaining.number())),
            ("breakdown", breakdown),
        ]))
    }
    fn locate(&mut self, m: &V, collateral: &V, buyer: &Bn, seller: &Bn, block: &V) -> Eval<V> {
        let sell = prop(m, "sellOrder")?;
        let buy = prop(m, "buyOrder")?;
        self.call("Margin.loadMarginMap", vec![prop(&sell, "contractId")?])?;
        let buyer = buyer.clone().q(8, Rounding::Down)?.number();
        let seller = seller.clone().q(8, Rounding::Down)?.number();
        let mut info = object(&[
            ("sellFeeFromAvailable", V::Bool(false)),
            ("sellFeeFromReserve", V::Bool(false)),
            ("sellFeeFromMargin", V::Bool(false)),
            ("buyFeeFromAvailable", V::Bool(false)),
            ("buyFeeFromReserve", V::Bool(false)),
            ("buyFeeFromMargin", V::Bool(false)),
            ("sellerFee", n(seller)),
            ("buyerFee", n(buyer)),
        ]);
        let txid = or(
            prop(m, "txid")?,
            s(&format!("contract-fee-{}", js_text(block))),
        );
        for (order, key, fee, prefix) in [
            (&buy, "buyerAddress", buyer, "buy"),
            (&sell, "sellerAddress", seller, "sell"),
        ] {
            let address = prop(order, key)?;
            let virtual_amm = matches!(&address,V::String(x)if x=="amm"||x.starts_with("amm:"));
            if virtual_amm {
                continue;
            }
            if fee < 0.0 {
                self.call(
                    "Tally.updateBalance",
                    vec![
                        address,
                        collateral.clone(),
                        n(-fee),
                        n(0.0),
                        n(0.0),
                        n(0.0),
                        s("contractFeeRebate"),
                        block.clone(),
                        txid.clone(),
                    ],
                )?;
                set(
                    &mut info,
                    &format!("{prefix}FeeFromAvailable"),
                    V::Bool(true),
                )?;
            } else if fee > 0.0 {
                let available = self.call(
                    "Tally.hasSufficientBalance",
                    vec![address.clone(), collateral.clone(), n(fee)],
                )?;
                let reserve = self.call(
                    "Tally.hasSufficientReserve",
                    vec![address.clone(), collateral.clone(), n(fee)],
                )?;
                let margin = self.call(
                    "Tally.hasSufficientMargin",
                    vec![address.clone(), collateral.clone(), n(fee)],
                )?;
                let bucket = if truth(&prop(&available, "hasSufficient")?) {
                    Some((0, "Available"))
                } else if truth(&prop(&reserve, "hasSufficient")?) {
                    Some((1, "Reserve"))
                } else if truth(&prop(&margin, "hasSufficient")?) {
                    Some((2, "Margin"))
                } else {
                    None
                };
                if let Some((index, name)) = bucket {
                    let mut changes = vec![n(0.0); 4];
                    changes[index] = n(-fee);
                    let mut args = vec![address, collateral.clone()];
                    args.extend(changes);
                    args.extend([s("contractFee"), block.clone(), txid.clone()]);
                    self.call("Tally.updateBalance", args)?;
                    set(&mut info, &format!("{prefix}FeeFrom{name}"), V::Bool(true))?;
                }
            }
        }
        Ok(info)
    }
    fn reward_flags(&mut self, m: &V, channel: &V) -> Eval<(bool, bool)> {
        let contract = self.call(
            "Contract.getContractInfo",
            vec![prop(&prop(m, "sellOrder")?, "contractId")?],
        )?;
        let mut basic = false;
        if truth(&contract) {
            let issuer = prop(&contract, "issuerAddress")?;
            if truth(&issuer) {
                basic = truth(&self.call("ClearList.isAddressInClearlist", vec![n(1.0), issuer])?);
            }
        }
        let addresses = if strict(&prop(m, "type")?, &s("channel")) {
            let commits = self.call("Channels.getCommitAddresses", vec![prop(m, "address")?])?;
            if nullish(&commits) {
                return Err(throw("TypeError",format!("Cannot destructure property 'commitAddressA' of '(intermediate value)' as it is {}.",js_text(&commits))));
            }
            vec![
                prop(&prop(channel, "A")?, "address")?,
                prop(&prop(channel, "B")?, "address")?,
            ]
        } else {
            vec![prop(m, "buyerAddress")?, prop(m, "sellerAddress")?]
        };
        let mut enhanced = false;
        for address in addresses {
            if truth(&self.call("ClearList.isAddressInClearlist", vec![n(2.0), address])?) {
                enhanced = true;
            }
        }
        Ok((basic, enhanced))
    }
}
