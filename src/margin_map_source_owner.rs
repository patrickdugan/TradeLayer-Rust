//! Actual pinned MarginMap position metadata and instance identities.
//! Database and financial dependencies belong to the composing SourceBackend.
use crate::codec::{verify_crc, with_crc, Cursor};
use crate::decimal::{Decimal, Rounding};
use crate::validity_source::{
    DependencyCall, DependencyObservation, DependencyReturn, SourceBackend, SourceCall,
    SourceEvaluation, SourceException, SourceOutcome, SourceRuleError, SourceValue as V,
};
use core::str::FromStr;

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636";
pub const MAX_INSTANCES: usize = 1024;
pub const MAX_OBJECTS: usize = 1024;
pub const MAX_STATE_BYTES: usize = 2 * 1024 * 1024;
pub const METHODS: [&str; 40] = [
    "getInstance",
    "loadMarginMap",
    "getPositionForAddress",
    "getAllPositions",
    "setInitialMargin",
    "updateContractBalances",
    "updateContractBalancesWithMatch",
    "saveMarginMap",
    "recordMarginMapDelta",
    "realizePnl",
    "settlePNL",
    "reduceMargin",
    "feeMarginReduce",
    "updateAveragePrice",
    "calculateLiquidationPrice",
    "calculateMarginRequirement",
    "estimatePNL",
    "writePositionToMap",
    "mergePositions",
    "cleanupFlatPositions",
    "computeMaintenanceMarginRequirement",
    "isBelowMaintenanceMargin",
    "updateMargin",
    "clear",
    "generateLiquidationOrder",
    "saveLiquidationOrders",
    "simpleDeleverage",
    "calcPriceDelta",
    "adjustDeleveraging",
    "dynamicDeleverage",
    "calculateNetExposure",
    "executeDeleveraging",
    "fetchLiquidationVolume",
    "needsLiquidation",
    "getMarketPrice",
    "recordContractTrade",
    "saveTrade",
    "getExpiringTickersUpTo",
    "cleanupExpiredTickersUpTo",
    "_touchExpiryIndex",
];
type Eval<T> = Result<T, Fault>;
#[derive(Debug)]
enum Fault {
    Thrown(SourceException),
    Local(SourceRuleError),
}
impl From<SourceRuleError> for Fault {
    fn from(e: SourceRuleError) -> Self {
        Self::Local(e)
    }
}
fn thrown(name: &str, message: impl Into<String>) -> Fault {
    Fault::Thrown(SourceException {
        name: name.into(),
        message: message.into(),
    })
}
fn initial_position(address: V) -> V {
    obj(&[
        ("contracts", n(0.0)),
        ("margin", n(0.0)),
        ("unrealizedPNL", n(0.0)),
        ("avgPrice", n(0.0)),
        ("address", address),
    ])
}
fn average(position: &V, amount: &V, price: &V, sell: &V) -> Eval<V> {
    let old = prop(position, "contracts")?;
    let size = if truth(sell) {
        Bn::from(&old)?.sub(&Bn::from(amount)?)?
    } else {
        Bn::from(&old)?.add(&Bn::from(amount)?)?
    };
    if size.zero() {
        return Ok(V::Null);
    }
    Ok(Bn::from(&or_zero(prop(position, "avgPrice")?))?
        .mul(&Bn::from(&n(number(&old)?.abs()))?)?
        .add(&Bn::from(price)?.mul(&Bn::from(amount)?)?)?
        .div(&Bn::from(&n(size.f64().abs()))?)?
        .dp(8)?
        .value())
}
fn liquidation(args: &[V]) -> Eval<V> {
    let balance = Bn::from(&arg(args, 0))?;
    let margin = Bn::from(&arg(args, 1))?;
    let upnl = Bn::from(&or_zero(arg(args, 7)))?;
    let total = balance.add(&margin)?.sub(&upnl.max(&Bn::c(0)))?;
    let contracts = Bn::from(&n(number(&arg(args, 2))?.abs()))?;
    let notional = Bn::from(&arg(args, 3))?;
    let avg = Bn::from(&arg(args, 6))?;
    let position_notional = notional.mul(&contracts)?;
    let adjust = margin.div(&Bn::c(2))?.div(&contracts)?;
    let no_prices = || obj(&[("bankruptcyPrice", V::Null), ("liquidationPrice", V::Null)]);
    let (bank, liq) = if !truth(&arg(args, 4)) {
        if truth(&arg(args, 5)) {
            if total
                .cmp(&position_notional.mul(&avg)?)
                .is_some_and(|x| x != core::cmp::Ordering::Less)
            {
                return Ok(no_prices());
            }
            let bank = avg
                .sub(&total.div(&position_notional)?)?
                .mul(&Bn::from(&s("1.005"))?)?;
            let liq = bank.add(&adjust)?;
            (bank, liq)
        } else {
            let bank = avg
                .add(&total.div(&position_notional)?)?
                .mul(&Bn::from(&s("0.995"))?)?;
            let liq = bank.sub(&adjust)?;
            (bank, liq)
        }
    } else {
        let term = margin.div(&Bn::c(2))?.div(&contracts.mul(&notional)?)?;
        let reciprocal = Bn::c(1).div(&avg)?;
        if truth(&arg(args, 5)) {
            (
                Bn::c(1).div(&reciprocal.add(&term.mul(&Bn::from(&s("1.005"))?)?)?)?,
                Bn::c(1).div(&reciprocal.add(&term)?)?,
            )
        } else {
            let liq = reciprocal.sub(&term)?;
            if liq
                .cmp(&Bn::c(0))
                .is_some_and(|x| x != core::cmp::Ordering::Greater)
            {
                return Ok(no_prices());
            }
            (
                Bn::c(1).div(&reciprocal.sub(&term.mul(&Bn::from(&s("1.005"))?)?)?)?,
                Bn::c(1).div(&liq)?,
            )
        }
    };
    Ok(obj(&[
        ("bankruptcyPrice", n(bank.dp(4)?.f64().abs())),
        ("liquidationPrice", n(liq.dp(4)?.f64().abs())),
    ]))
}
impl MarginMapSourceOwner {
    fn set_initial<B: SourceBackend>(&mut self, id: u64, f: &mut Frame, b: &mut B) -> Eval<V> {
        let sender = arg(&f.args, 0);
        let contract = arg(&f.args, 1);
        let delta = arg(&f.args, 2);
        let block = arg(&f.args, 3);
        let was_empty = self.inst(id)?.margins.is_empty();
        if was_empty {
            // getAllPositions returns an Array, whose entries method exists,
            // but new Map(nonempty array of position objects) throws. The
            // source catches that hydration failure; no positions are copied.
            match self.all_positions(contract.clone(), V::Undefined, f, b) {
                Ok(_) | Err(Fault::Thrown(_)) => {}
                Err(e) => return Err(e),
            }
        }
        let p = if truth(&arg(&f.args, 4)) {
            self.root(f, 4, None)?
        } else if let Some(p) = self.cached(id, &sender)? {
            if truth(&self.objects[p]) {
                p
            } else {
                self.alloc(initial_position(sender.clone()))?
            }
        } else {
            self.alloc(initial_position(sender.clone()))?
        };
        for field in [
            "contracts",
            "margin",
            "unrealizedPNL",
            "avgPrice",
            "address",
        ] {
            if matches!(prop(&self.objects[p], field)?, V::Null | V::Undefined) {
                set(
                    &mut self.objects[p],
                    field,
                    if field == "address" {
                        sender.clone()
                    } else {
                        n(0.0)
                    },
                )?
            }
        }
        let margin = Bn::from(&or_zero(prop(&self.objects[p], "margin")?))?
            .add(&Bn::from(&or_zero(delta.clone()))?)?
            .dp(8)?
            .value();
        set(&mut self.objects[p], "margin", margin)?;
        self.cache_set(id, sender.clone(), p)?;
        self.delta(
            &[
                sender,
                contract,
                prop(&self.objects[p], "contracts")?,
                n(0.0),
                delta,
                n(0.0),
                or_zero(prop(&self.objects[p], "avgPrice")?),
                s("initialMargin"),
                block,
            ],
            f,
            b,
        )?;
        if !was_empty {
            self.save(id, V::Bool(true), f, b)?;
        }
        if f.track_return {
            f.returned_position = Some(p);
        }
        Ok(self.objects[p].clone())
    }
    fn update<B: SourceBackend>(
        &mut self,
        id: u64,
        p: usize,
        args: &[V],
        f: &mut Frame,
        b: &mut B,
    ) -> Eval<V> {
        let address = arg(args, 0);
        let amount = arg(args, 1);
        let price = arg(args, 2);
        let sell = arg(args, 3);
        let old = prop(&self.objects[p], "contracts")?;
        if number(&arg(args, 7))? > 0.0 {
            set(&mut self.objects[p], "avgPrice", price.clone())?;
        } else if strict(&arg(args, 6), &n(0.0)) {
            if strict(&old, &n(0.0)) {
                set(&mut self.objects[p], "avgPrice", price.clone())?;
            } else if (number(&old)? > 0.0 && !truth(&sell))
                || (number(&old)? < 0.0 && truth(&sell))
            {
                let avg = average(&self.objects[p], &amount, &price, &sell)?;
                set(&mut self.objects[p], "avgPrice", avg)?;
            }
        }
        let size = if truth(&sell) {
            Bn::from(&old)?.sub(&Bn::from(&amount)?)?
        } else {
            Bn::from(&old)?.add(&Bn::from(&amount)?)?
        }
        .value();
        set(&mut self.objects[p], "contracts", size.clone())?;
        let info = Self::call(f, b, "Contract.getContractInfo", vec![arg(args, 8)], true)?;
        let notional = prop(&info, "notionalValue")?;
        let collateral = prop(&info, "collateralPropertyId")?;
        let tally = Self::call(
            f,
            b,
            "Tally.getTally",
            vec![address.clone(), collateral],
            true,
        )?;
        let liq = liquidation(&[
            prop(&tally, "available")?,
            prop(&self.objects[p], "margin")?,
            size.clone(),
            notional,
            arg(args, 5),
            V::Bool(number(&size)? > 0.0),
            prop(&self.objects[p], "avgPrice")?,
        ])?;
        if strict(&size, &n(0.0)) {
            set(&mut self.objects[p], "liqPrice", n(0.0))?;
            set(&mut self.objects[p], "bankruptcyPrice", n(0.0))?;
            set(&mut self.objects[p], "avgPrice", V::Null)?;
        } else {
            let l = liq.get("liquidationPrice");
            set(
                &mut self.objects[p],
                "liqPrice",
                if truth(&l) { l } else { V::Null },
            )?;
            set(
                &mut self.objects[p],
                "bankruptcyPrice",
                liq.get("bankruptcyPrice"),
            )?;
        }
        self.cache_set(id, address.clone(), p)?;
        let tag = if truth(&arg(args, 9)) {
            if truth(&arg(args, 11)) {
                "initialLiq"
            } else {
                "liquidatingContract"
            }
        } else {
            "updateContractBalances"
        };
        self.save(id, arg(args, 10), f, b)?;
        self.delta(
            &[
                address,
                arg(args, 8),
                size,
                amount,
                n(0.0),
                n(0.0),
                n(0.0),
                s(tag),
                arg(args, 10),
                prop(&self.objects[p], "bankruptcyPrice")?,
            ],
            f,
            b,
        )?;
        Ok(self.objects[p].clone())
    }
    fn with_match<B: SourceBackend>(&mut self, id: u64, f: &mut Frame, b: &mut B) -> Eval<V> {
        let m = arg(&f.args, 0);
        let buy = prop(&m, "buyOrder")?;
        let sell = prop(&m, "sellOrder")?;
        let bp = self.root(f, 0, Some("buyerPosition"))?;
        self.update(
            id,
            bp,
            &[
                prop(&buy, "buyerAddress")?,
                prop(&buy, "amount")?,
                prop(&m, "tradePrice")?,
                prop(&buy, "sell")?,
                self.objects[bp].clone(),
                prop(&m, "inverse")?,
                arg(&f.args, 2),
                arg(&f.args, 3),
                prop(&buy, "contractId")?,
                prop(&buy, "isLiq")?,
                arg(&f.args, 6),
            ],
            f,
            b,
        )?;
        let sp = self.root(f, 0, Some("sellerPosition"))?;
        self.update(
            id,
            sp,
            &[
                prop(&sell, "sellerAddress")?,
                prop(&sell, "amount")?,
                prop(&m, "tradePrice")?,
                prop(&sell, "sell")?,
                self.objects[sp].clone(),
                prop(&m, "inverse")?,
                arg(&f.args, 4),
                arg(&f.args, 5),
                prop(&sell, "contractId")?,
                prop(&sell, "isLiq")?,
                arg(&f.args, 6),
            ],
            f,
            b,
        )?;
        Ok(obj(&[
            ("bp", self.objects[bp].clone()),
            ("sp", self.objects[sp].clone()),
        ]))
    }
    fn realize<B: SourceBackend>(&mut self, f: &mut Frame, b: &mut B) -> Eval<V> {
        if !truth(&arg(&f.args, 6)) {
            return Ok(V::BigNumber("0".into()));
        }
        let p = self.root(f, 6, None)?;
        let price = Bn::from(&arg(&f.args, 2))?;
        let avg = Bn::from(&arg(&f.args, 3))?;
        let change = if truth(&arg(&f.args, 4)) {
            Bn::c(1).div(&avg)?.sub(&Bn::c(1).div(&price)?)?
        } else {
            price.sub(&avg)?
        };
        let pnl = change
            .mul(&Bn::from(&arg(&f.args, 1))?)?
            .mul(&Bn::from(&arg(&f.args, 5))?)?;
        let pnl = if truth(&arg(&f.args, 7)) {
            pnl.neg()
        } else {
            pnl
        }
        .dp(8)?
        .f64();
        let upnl = prop(&self.objects[p], "unrealizedPNL")?;
        let sign = if number(&upnl)? > 0.0 { 1 } else { -1 };
        let next = Bn::from(&n(number(&upnl)?.abs()))?
            .sub(&Bn::from(&n(pnl.abs()))?)?
            .mul(&Bn::c(sign))?
            .dp(8)?
            .value();
        set(&mut self.objects[p], "unrealizedPNL", next)?;
        set(&mut self.objects[p], "realizedPNL", n(pnl))?;
        self.delta(
            &[
                arg(&f.args, 0),
                arg(&f.args, 8),
                n(0.0),
                n(0.0),
                n(0.0),
                n(pnl),
                n(0.0),
                s("rPNL"),
            ],
            f,
            b,
        )?;
        Ok(self.objects[p].clone())
    }
    fn settle<B: SourceBackend>(&mut self, id: u64, f: &mut Frame, b: &mut B) -> Eval<V> {
        let address = arg(&f.args, 0);
        let p = self
            .cached(id, &address)?
            .ok_or_else(|| ty("Cannot read properties of undefined (reading 'avgPrice')"))?;
        let avg = prop(&self.objects[p], "avgPrice")?;
        let _avg_bn = Bn::from(&avg)?;
        let price = arg(&f.args, 2);
        let contracts = arg(&f.args, 1);
        if !truth(&self.objects[p]) {
            return Ok(n(0.0));
        }
        let mut notional = arg(&f.args, 7);
        if matches!(notional, V::Null | V::Undefined) || number(&notional)?.is_nan() {
            let info = Self::call(
                f,
                b,
                "Contract.getContractInfo",
                vec![arg(&f.args, 4)],
                true,
            )?;
            notional =
                if truth(&info) && !matches!(info.get("notionalValue"), V::Null | V::Undefined) {
                    info.get("notionalValue")
                } else {
                    n(1.0)
                }
        }
        let notional = Bn::from(&notional)?;
        let last = arg(&f.args, 3);
        let last = if matches!(last, V::Null | V::Undefined) {
            avg
        } else {
            last
        };
        let pnl = if !truth(&arg(&f.args, 6)) {
            Bn::from(&n((number(&price)? - number(&last)?) * number(&contracts)?))?
        } else {
            Bn::c(1)
                .div(&Bn::from(&last)?)?
                .sub(&Bn::c(1).div(&Bn::from(&price)?)?)?
                .mul(&Bn::from(&contracts)?)?
                .mul(&notional)?
        };
        let old = prop(&self.objects[p], "unrealizedPNL")?;
        let correction = Bn::from(&old)?.sub(&pnl)?.dp(8)?.f64();
        set(
            &mut self.objects[p],
            "unrealizedPNL",
            n(number(&old)? - correction),
        )?;
        self.cache_set(id, prop(&self.objects[p], "address")?, p)?;
        self.delta(
            &[
                address,
                arg(&f.args, 4),
                prop(&self.objects[p], "contracts")?,
                contracts,
                n(0.0),
                n(-pnl.f64()),
                n(0.0),
                s("settlementPNL"),
                arg(&f.args, 5),
            ],
            f,
            b,
        )?;
        Ok(pnl.dp(8)?.value())
    }
    fn reduce<B: SourceBackend>(&mut self, id: u64, f: &mut Frame, b: &mut B) -> Eval<V> {
        if !truth(&arg(&f.args, 0)) {
            return Ok(obj(&[
                ("netMargin", V::BigNumber("0".into())),
                ("mode", s("none")),
            ]));
        }
        let p = self.root(f, 0, None)?;
        self.reduce_position(id, p, f, b)
    }
    fn reduce_position<B: SourceBackend>(
        &mut self,
        id: u64,
        p: usize,
        f: &mut Frame,
        b: &mut B,
    ) -> Eval<V> {
        let mut margin = Bn::from(&prop(&self.objects[p], "margin")?)?;
        let required = Bn::from(&prop(&self.objects[p], "contracts")?)?
            .abs()
            .mul(&Bn::from(&arg(&f.args, 2))?)?;
        let mut excess = margin.sub(&required)?;
        if excess.negative() {
            return Ok(n(0.0));
        }
        if truth(&arg(&f.args, 6)) {
            let fee = Bn::from(&or_zero(arg(&f.args, 7)))?;
            excess = excess.sub(&fee)?;
            margin = margin.sub(&fee)?;
        }
        let reduction = excess.max(&Bn::c(0));
        margin = margin.sub(&reduction)?;
        set(&mut self.objects[p], "margin", margin.dp(8)?.value())?;
        let reduction = reduction.dp(8)?.value();
        self.cache_set(id, prop(&self.objects[p], "address")?, p)?;
        self.delta(
            &[
                arg(&f.args, 4),
                arg(&f.args, 3),
                n(0.0),
                n(0.0),
                n(-number(&reduction)?),
                n(0.0),
                n(0.0),
                s("marginReduction"),
            ],
            f,
            b,
        )?;
        self.save(id, arg(&f.args, 8), f, b)?;
        Ok(reduction)
    }
    fn fee_reduce<B: SourceBackend>(&mut self, id: u64, f: &mut Frame, b: &mut B) -> Eval<V> {
        let p = self.root(f, 1, None)?;
        let margin = Bn::from(&prop(&self.objects[p], "margin")?)?
            .sub(&Bn::from(&arg(&f.args, 2))?)?
            .dp(8)?
            .value();
        set(&mut self.objects[p], "margin", margin)?;
        self.cache_set(id, prop(&self.objects[p], "address")?, p)?;
        self.delta(
            &[
                arg(&f.args, 0),
                arg(&f.args, 3),
                n(0.0),
                n(0.0),
                n(-number(&arg(&f.args, 2))?),
                n(0.0),
                n(0.0),
                s("marginFeeReduction"),
            ],
            f,
            b,
        )?;
        self.save(id, arg(&f.args, 4), f, b)?;
        Ok(self.objects[p].clone())
    }
    fn merge_positions<B: SourceBackend>(&mut self, id: u64, f: &mut Frame, b: &mut B) -> Eval<V> {
        let input = arg(&f.args, 0);
        let positions = match &input {
            V::String(s) => s.chars().map(|c| V::String(c.to_string())).collect(),
            _ => source_array(&input, "finalPositions is not iterable")?,
        };
        let flatten = if matches!(arg(&f.args, 2), V::Undefined) {
            true
        } else {
            truth(&arg(&f.args, 2))
        };
        let mut entries: Vec<(V, usize)> = vec![];
        for (index, position) in positions.iter().enumerate() {
            let p = if matches!(input, V::Array(_)) {
                self.array_root(f, 0, index)?
            } else {
                self.alloc(position.clone())?
            };
            let pos = &self.objects[p];
            if flatten {
                let iou = Bn::from(&or_zero(prop(pos, "iouClaim")?))?;
                let contracts = prop(pos, "contracts")?;
                if (!truth(&contracts) || strict(&contracts, &n(0.0))) && iou.zero() {
                    continue;
                }
            }
            let address = prop(pos, "address")?;
            primitive(&address)?;
            let address = if matches!(&address,V::Number(bits)if f64::from_bits(*bits)==0.0) {
                n(0.0)
            } else {
                address
            };
            if let Some((_, previous)) = entries.iter_mut().find(|(k, _)| key_eq(k, &address)) {
                *previous = p;
            } else {
                entries.push((address, p));
            }
        }
        self.inst_mut(id)?.margins = entries;
        self.save(id, V::Undefined, f, b)
    }
    fn cleanup_flat<B: SourceBackend>(&mut self, id: u64, f: &mut Frame, b: &mut B) -> Eval<V> {
        let entries = self.inst(id)?.margins.clone();
        for (key, p) in entries {
            let pos = &self.objects[p];
            let contracts = prop(pos, "contracts")?;
            if !strict(&contracts, &n(0.0)) {
                continue;
            }
            let margin = prop(pos, "margin")?;
            if truth(&margin) && !strict(&margin, &n(0.0)) {
                continue;
            }
            let upnl = prop(pos, "unrealizedPNL")?;
            if truth(&upnl) && !strict(&upnl, &n(0.0)) {
                continue;
            }
            if Bn::from(&or_zero(prop(pos, "iouClaim")?))?.zero() {
                self.inst_mut(id)?.margins.retain(|(k, _)| !key_eq(k, &key));
            }
        }
        self.save(id, V::Bool(true), f, b)
    }
    fn maintenance<B: SourceBackend>(
        contract: V,
        position: V,
        f: &mut Frame,
        b: &mut B,
    ) -> Eval<V> {
        if !truth(&position) {
            return Ok(n(0.0));
        }
        let initial = Self::call(
            f,
            b,
            "Contract.getInitialMargin",
            vec![contract, prop(&position, "avgPrice")?],
            true,
        )?;
        Ok(
            Bn::from(&n(number(&or_zero(prop(&position, "contracts")?))?.abs()))?
                .mul(&Bn::from(&or_zero(initial))?)?
                .mul(&Bn::from(&s("0.5"))?)?
                .dp(8)?
                .value(),
        )
    }
    fn below_maintenance<B: SourceBackend>(
        &mut self,
        id: u64,
        f: &mut Frame,
        b: &mut B,
    ) -> Eval<V> {
        let mut position = arg(&f.args, 2);
        if !truth(&position) {
            position = self.inspect_position(id, &arg(&f.args, 0))?;
        }
        if !truth(&position) {
            return Ok(V::Undefined);
        }
        let required = Self::maintenance(arg(&f.args, 1), position.clone(), f, b)?;
        Ok(V::Bool(
            number(&prop(&position, "margin")?)? < number(&required)?,
        ))
    }
    fn update_margin<B: SourceBackend>(&mut self, id: u64, f: &mut Frame, b: &mut B) -> Eval<V> {
        let p = if truth(&arg(&f.args, 4)) {
            self.root(f, 4, None)?
        } else if let Some(p) = self.cached(id, &arg(&f.args, 0))? {
            if truth(&self.objects[p]) {
                p
            } else {
                self.alloc(default_margin_position())?
            }
        } else {
            self.alloc(default_margin_position())?
        };
        let change = Bn::from(&arg(&f.args, 2))?
            .add(&Bn::from(&prop(&self.objects[p], "margin")?)?)?
            .dp(8)?
            .value();
        set(&mut self.objects[p], "margin", change.clone())?;
        self.cache_set(id, prop(&self.objects[p], "address")?, p)?;
        self.delta(
            &[
                arg(&f.args, 0),
                arg(&f.args, 1),
                prop(&self.objects[p], "contracts")?,
                n(0.0),
                change,
                n(0.0),
                n(0.0),
                s("updateMargin"),
                arg(&f.args, 3),
            ],
            f,
            b,
        )?;
        self.save(id, arg(&f.args, 3), f, b)?;
        Ok(self.objects[p].clone())
    }
    fn clear_position<B: SourceBackend>(&mut self, id: u64, f: &mut Frame, b: &mut B) -> Eval<V> {
        let p = self.root(f, 0, None)?;
        if matches!(
            prop(&self.objects[p], "unrealizedPNL")?,
            V::Null | V::Undefined
        ) {
            set(&mut self.objects[p], "unrealizedPNL", n(0.0))?;
        }
        set(&mut self.objects[p], "oldMark", arg(&f.args, 7))?;
        set(&mut self.objects[p], "lastMark", arg(&f.args, 6))?;
        let upnl = Bn::from(&arg(&f.args, 2))?
            .add(&Bn::from(&prop(&self.objects[p], "unrealizedPNL")?)?)?
            .dp(8)?
            .value();
        set(&mut self.objects[p], "unrealizedPNL", upnl)?;
        self.cache_set(id, prop(&self.objects[p], "address")?, p)?;
        let outer = f.args.clone();
        f.args = vec![arg(&outer, 4), self.objects[p].clone()];
        let written = self.write_position(f, b);
        f.args = outer;
        written?;
        self.delta(
            &[
                arg(&f.args, 1),
                arg(&f.args, 4),
                prop(&self.objects[p], "contracts")?,
                n(0.0),
                n(0.0),
                arg(&f.args, 2),
                arg(&f.args, 3),
                s("markPrice"),
                arg(&f.args, 5),
                arg(&f.args, 6),
            ],
            f,
            b,
        )?;
        Ok(self.objects[p].clone())
    }
    fn generate_liquidation(args: &[V]) -> Eval<V> {
        let amount = arg(args, 2);
        if !truth(&amount) || number(&amount)? <= 0.0 {
            return Ok(V::Null);
        }
        let position = arg(args, 0);
        let contracts = prop(&position, "contracts")?;
        if strict(&contracts, &n(0.0)) {
            return Ok(V::Null);
        }
        let sell = number(&contracts)? > 0.0;
        let equity = Bn::from(&or_zero(prop(&position, "margin")?))?
            .add(&Bn::from(&or_zero(prop(&position, "available")?))?)?;
        if equity
            .cmp(&Bn::c(0))
            .is_some_and(|c| c != core::cmp::Ordering::Greater)
        {
            return Ok(V::Null);
        }
        let address = prop(&position, "address")?;
        let bankruptcy = arg(args, 5);
        if !matches!(bankruptcy, V::BigNumber(_)) {
            if matches!(bankruptcy, V::Undefined | V::Null) {
                prop(&bankruptcy, "toNumber")?;
            }
            return Err(ty("bankruptcyPrice.toNumber is not a function"));
        }
        Ok(obj(&[
            ("address", address),
            ("contractId", arg(args, 1)),
            ("amount", n(number(&amount)?.abs())),
            ("price", Bn::from(&bankruptcy)?.value()),
            ("sell", V::Bool(sell)),
            ("isLiq", V::Bool(true)),
            ("isMarket", V::Bool(true)),
            ("blockTime", arg(args, 3)),
        ]))
    }
    fn save_liquidations<B: SourceBackend>(f: &mut Frame, b: &mut B) -> Eval<V> {
        let db = Self::database(f, b, "liquidations")?;
        let key = format!(
            "liquidationOrders-{}-{}",
            text(&arg(&f.args, 0))?,
            text(&arg(&f.args, 4))?
        );
        let value = obj(&[
            ("_id", s(&key)),
            ("order", arg(&f.args, 2)),
            ("position", arg(&f.args, 1)),
            ("reason", arg(&f.args, 3)),
            ("blockHeight", arg(&f.args, 4)),
            ("liquidationLoss", arg(&f.args, 5)),
            ("contractsDeleveraged", arg(&f.args, 6)),
            ("realizedLiquidation", arg(&f.args, 7)),
            ("deleverage", arg(&f.args, 8)),
            ("info", arg(&f.args, 9)),
        ]);
        Self::db(
            f,
            b,
            &db,
            "updateAsync",
            vec![
                obj(&[("_id", s(&key))]),
                obj(&[("$set", value)]),
                obj(&[("upsert", V::Bool(true))]),
            ],
        )?;
        Ok(V::Undefined)
    }
    fn price_delta(args: &[V]) -> Eval<V> {
        let contracts = Bn::from(&arg(args, 0))?;
        let from = Bn::from(&arg(args, 1))?;
        let to = Bn::from(&arg(args, 2))?;
        let supplied = arg(args, 4);
        let notional = Bn::from(&if truth(&supplied) { supplied } else { n(1.0) })?;
        let delta = if truth(&arg(args, 3)) {
            Bn::c(1)
                .div(&from)?
                .sub(&Bn::c(1).div(&to)?)?
                .mul(&contracts)?
                .mul(&notional)?
        } else {
            to.sub(&from)?.mul(&contracts)?.mul(&notional)?
        };
        Ok(if truth(&arg(args, 5)) {
            delta
        } else {
            delta.neg()
        }
        .dp(8)?
        .tagged())
    }
    fn adjust_deleveraging<B: SourceBackend>(
        &mut self,
        id: u64,
        f: &mut Frame,
        b: &mut B,
    ) -> Eval<V> {
        let requested = number(&arg(&f.args, 4))?;
        let cache = arg(&f.args, 0);
        let value = prop(&cache, &text(&arg(&f.args, 1))?)?;
        if requested == 0.0 || requested.is_nan() || requested <= 0.0 {
            return Ok(value);
        }
        if !truth(&value) || !truth(&prop(&value, "contracts")?) {
            return Ok(value);
        }
        let index = number(&arg(&f.args, 1))?;
        if !index.is_finite() || index < 0.0 || index.fract() != 0.0 {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        let p = self.array_root(f, 0, index as usize)?;
        self.adjust_position(id, p, f, b)
    }
    fn adjust_position<B: SourceBackend>(
        &mut self,
        id: u64,
        p: usize,
        f: &mut Frame,
        b: &mut B,
    ) -> Eval<V> {
        let requested = number(&arg(&f.args, 4))?;
        if requested == 0.0
            || requested.is_nan()
            || requested <= 0.0
            || !truth(&prop(&self.objects[p], "contracts")?)
        {
            return Ok(self.objects[p].clone());
        }
        let before = number(&prop(&self.objects[p], "contracts")?)?;
        let maximum = before.abs();
        if maximum <= 0.0 {
            return Ok(self.objects[p].clone());
        }
        let effective = js_min(requested, maximum);
        let change = if before > 0.0 { -effective } else { effective };
        let after = before + change;
        let stored_after = if (before > 0.0 && after < 0.0) || (before < 0.0 && after > 0.0) {
            0.0
        } else {
            after
        };
        set(&mut self.objects[p], "contracts", n(stored_after))?;
        let initial = Self::call(
            f,
            b,
            "Contract.getInitialMargin",
            vec![arg(&f.args, 3), arg(&f.args, 7)],
            true,
        )?;
        let collateral = Self::call(
            f,
            b,
            "Contract.getCollateralId",
            vec![arg(&f.args, 3)],
            true,
        )?;
        let outer = f.args.clone();
        f.args = vec![
            self.objects[p].clone(),
            n(change.abs()),
            initial,
            arg(&outer, 3),
            arg(&outer, 2),
            arg(&outer, 5),
            V::Bool(false),
            n(0.0),
        ];
        let reduced = self.reduce_position(id, p, f, b);
        f.args = outer;
        let mut reduction = reduced?;
        let sufficient = Self::call(
            f,
            b,
            "Tally.hasSufficientMargin",
            vec![arg(&f.args, 2), collateral.clone(), reduction.clone()],
            true,
        )?;
        if !truth(&prop(&sufficient, "hasSufficient")?) {
            reduction = Bn::from(&reduction)?
                .sub(&Bn::from(&prop(&sufficient, "shortfall")?)?)?
                .dp(8)?
                .value();
        }
        if !strict(&reduction, &n(0.0)) {
            Self::call(
                f,
                b,
                "Tally.updateBalance",
                vec![
                    arg(&f.args, 2),
                    collateral,
                    reduction.clone(),
                    n(0.0),
                    n(-number(&reduction)?),
                    n(0.0),
                    s("contractDelevMarginReturn"),
                    arg(&f.args, 6),
                ],
                true,
            )?;
        }
        let delta = vec![
            arg(&f.args, 2),
            arg(&f.args, 3),
            n(after),
            n(change),
            n(-number(&reduction)?),
            n(0.0),
            n(0.0),
            s("deleverage"),
        ];
        self.delta(&delta, f, b)?;
        if strict(&prop(&self.objects[p], "contracts")?, &n(0.0)) {
            set(&mut self.objects[p], "bankruptcyPrice", V::Null)?;
            set(&mut self.objects[p], "averagePrice", V::Null)?;
            set(&mut self.objects[p], "unrealizedPNL", n(0.0))?;
        }
        self.delta(&delta, f, b)?;
        Ok(self.objects[p].clone())
    }
    fn simple_deleverage<B: SourceBackend>(
        &mut self,
        id: u64,
        f: &mut Frame,
        b: &mut B,
    ) -> Eval<V> {
        let db = Self::database(f, b, "liquidations")?;
        let mut result = obj(&[
            ("contractId", arg(&f.args, 1)),
            ("liquidatingAddress", arg(&f.args, 5)),
            ("attemptedDeleverage", arg(&f.args, 2)),
            ("totalDeleveraged", n(0.0)),
            ("counterparties", V::Array(vec![])),
            ("poolAssignments", V::Array(vec![])),
        ]);
        let mut remaining = Bn::from(&or_zero(arg(&f.args, 2)))?;
        let positions = source_array(&arg(&f.args, 0), "positionCache.map is not a function")?;
        let mut cps: Vec<(usize, V, Bn)> = vec![];
        for (index, position) in positions.iter().enumerate() {
            let mut cp = spread(position)?;
            set(&mut cp, "_i", n(index as f64))?;
            if strict(&prop(&cp, "address")?, &arg(&f.args, 5)) {
                continue;
            }
            let count = number(&prop(&cp, "contracts")?)?;
            if !(if truth(&arg(&f.args, 3)) {
                count < 0.0
            } else {
                count > 0.0
            }) {
                continue;
            }
            let size = Bn::from(&or_zero(prop(&cp, "contracts")?))?;
            let last = Bn::from(&or_zero(prop(&cp, "lastMark")?))?;
            let mark = Bn::from(&or_zero(arg(&f.args, 9)))?;
            let mut pnl = Bn::c(0);
            if bn_gt(&last, &Bn::c(0)) && bn_gt(&mark, &Bn::c(0)) {
                pnl = if !truth(&arg(&f.args, 6)) {
                    size.mul(&mark.sub(&last)?)?
                        .mul(&Bn::from(&arg(&f.args, 7))?)?
                } else {
                    size.mul(&Bn::c(1).div(&last)?.sub(&Bn::c(1).div(&mark)?)?)?
                };
            }
            set(&mut cp, "movePnl", pnl.tagged())?;
            cps.push((index, cp, pnl));
        }
        if cps.iter().any(|(_, _, pnl)| bn_gt(pnl, &Bn::c(0))) {
            cps.retain(|(_, _, pnl)| bn_gt(pnl, &Bn::c(0)));
        } else {
            sort_counterparties(&mut cps, false)?;
        }
        // The source sorts once inside the winners branch and once again
        // unconditionally. Its comparator is pure; stable ordering is retained.
        sort_counterparties(&mut cps, true)?;
        let mut counterparties = vec![];
        let mut returned_positions = vec![];
        let mut total = 0.0;
        for (index, cp, pnl) in cps {
            if remaining
                .cmp(&Bn::c(0))
                .is_some_and(|c| c != core::cmp::Ordering::Greater)
            {
                break;
            }
            let abs = number(&prop(&cp, "contracts")?)?.abs();
            if abs <= 0.0 {
                continue;
            }
            let matched = js_min(abs, remaining.f64());
            let p = self.array_root(f, 0, index)?;
            let outer = f.args.clone();
            f.args = vec![
                arg(&outer, 0),
                n(index as f64),
                prop(&cp, "address")?,
                arg(&outer, 1),
                n(matched),
                arg(&outer, 3),
                arg(&outer, 8),
                arg(&outer, 4),
            ];
            let adjusted = self.adjust_position(id, p, f, b);
            f.args = outer;
            let updated = adjusted?;
            remaining = remaining.sub(&Bn::from(&n(matched))?)?;
            total += matched;
            let key = format!(
                "adl-{}-{}-{}-{}",
                text(&arg(&f.args, 1))?,
                text(&arg(&f.args, 8))?,
                text(&arg(&f.args, 5))?,
                text(&prop(&cp, "address")?)?
            );
            let after = if matches!(updated, V::Undefined | V::Null) {
                n(0.0)
            } else {
                let v = prop(&updated, "contracts")?;
                if matches!(v, V::Undefined | V::Null) {
                    n(0.0)
                } else {
                    v
                }
            };
            Self::db(
                f,
                b,
                &db,
                "updateAsync",
                vec![
                    obj(&[("_id", s(&key))]),
                    obj(&[
                        ("_id", s(&key)),
                        ("block", arg(&f.args, 8)),
                        ("contractId", arg(&f.args, 1)),
                        ("liquidatingAddress", arg(&f.args, 5)),
                        ("counterparty", prop(&cp, "address")?),
                        ("contractsReduced", n(matched)),
                        ("contractsBefore", prop(&cp, "contracts")?),
                        ("contractsAfter", after),
                        ("markPrice", arg(&f.args, 9)),
                        ("liqPrice", arg(&f.args, 4)),
                        ("reason", s("autoDeleverage")),
                    ]),
                    obj(&[("upsert", V::Bool(true))]),
                ],
            )?;
            counterparties.push(obj(&[
                ("address", prop(&cp, "address")?),
                ("matchSize", n(matched)),
                ("updatedPosition", updated),
                ("_adlAbsPos", n(abs)),
                ("_adlMovePnl", pnl.tagged()),
            ]));
            returned_positions.push(p);
        }
        let pool = Bn::from(&or_zero(arg(&f.args, 11)))?;
        let mut assignments = vec![];
        if bn_gt(&pool, &Bn::c(0)) && !counterparties.is_empty() {
            let mut denominator = Bn::c(0);
            for cp in &counterparties {
                let raw = prop(cp, "_adlAbsPos")?;
                let count = if truth(&raw) { raw } else { n(1.0) };
                let per = Bn::from(&or_zero(prop(cp, "_adlMovePnl")?))?
                    .div(&Bn::from(&or_zero(count))?)?;
                denominator =
                    denominator.add(&per.mul(&Bn::from(&or_zero(prop(cp, "matchSize")?))?)?)?;
            }
            let total_bn = Bn::from(&or_zero(n(total)))?;
            for cp in &mut counterparties {
                let mut share = Bn::c(0);
                if bn_gt(&denominator, &Bn::c(0)) {
                    let raw = prop(cp, "_adlAbsPos")?;
                    let count = if truth(&raw) { raw } else { n(1.0) };
                    let weight = Bn::from(&or_zero(prop(cp, "_adlMovePnl")?))?
                        .div(&Bn::from(&or_zero(count))?)?
                        .mul(&Bn::from(&or_zero(prop(cp, "matchSize")?))?)?;
                    share = pool.mul(&weight.div(&denominator)?)?;
                } else if bn_gt(&total_bn, &Bn::c(0)) {
                    share =
                        pool.mul(&Bn::from(&or_zero(prop(cp, "matchSize")?))?.div(&total_bn)?)?;
                }
                assignments.push(obj(&[
                    ("address", prop(cp, "address")?),
                    ("poolShare", share.dp(8)?.value()),
                ]));
                if let V::Object(fields) = cp {
                    fields.retain(|(k, _)| k != "_adlAbsPos" && k != "_adlMovePnl");
                }
            }
        }
        for (cp, p) in counterparties.iter_mut().zip(returned_positions) {
            set(cp, "updatedPosition", self.objects[p].clone())?;
        }
        set(&mut result, "totalDeleveraged", n(total))?;
        set(&mut result, "counterparties", V::Array(counterparties))?;
        set(&mut result, "poolAssignments", V::Array(assignments))?;
        Ok(result)
    }
    fn dynamic_deleverage<B: SourceBackend>(&mut self, f: &mut Frame, b: &mut B) -> Eval<V> {
        Bn::from(&arg(&f.args, 2))?;
        let map = self.load(arg(&f.args, 0), V::Undefined, f, b)?;
        if matches!(map, V::Null | V::Undefined) {
            prop(&map, "getAllPositions")?;
        }
        self.all_positions(V::Undefined, V::Undefined, f, b)?;
        Err(thrown("ReferenceError", "ContractRegistry is not defined"))
    }
    fn execute_deleveraging<B: SourceBackend>(&mut self, f: &mut Frame, b: &mut B) -> Eval<V> {
        let contract = arg(&f.args, 1);
        let loaded = self.load(contract.clone(), V::Undefined, f, b)?;
        if matches!(loaded, V::Null | V::Undefined) {
            prop(&loaded, "getPositionForAddress")?;
        }
        let id = number(&loaded.get("sourceMarginInstance"))? as u64;
        let before_instances = self.instances.len();
        let position = self.get_position(arg(&f.args, 0), contract, f, b)?;
        if !truth(&position) {
            return Ok(V::Undefined);
        }
        // getPositionForAddress independently loaded another map. Reuse the
        // actual returned object's identity from that load, not an equal row.
        let p = if self.instances.len() > before_instances {
            let last = self.instances.len() as u64;
            let address = arg(&f.args, 0);
            let entries = self.inst(last)?.margins.clone();
            let found = entries.iter().find(|(key, _)| strict(key, &address))
                .or_else(|| entries.iter().find(|(key, _)| {
                    matches!((&address,key),(V::String(a),V::String(k))
                        if ["ltc1","tltc1","bc1","tb1"].iter().any(|p|a.starts_with(p)) && a.to_lowercase()==k.to_lowercase())
                }));
            if let Some((_, p)) = found {
                *p
            } else {
                self.alloc(position)?
            }
        } else {
            self.alloc(position)?
        };
        let size = arg(&f.args, 2);
        let change = if truth(&arg(&f.args, 3)) {
            size
        } else {
            n(-number(&size)?)
        };
        let contracts = Bn::from(&prop(&self.objects[p], "contracts")?)?
            .add(&Bn::from(&change)?)?
            .value();
        set(&mut self.objects[p], "contracts", contracts.clone())?;
        if strict(&contracts, &n(0.0)) {
            set(&mut self.objects[p], "liqPrice", V::Null)?;
            set(&mut self.objects[p], "bankruptcyPrice", V::Null)?;
        }
        self.cache_set(id, prop(&self.objects[p], "address")?, p)?;
        self.save(id, arg(&f.args, 5), f, b)
    }
    fn fetch_liquidation_volume<B: SourceBackend>(f: &mut Frame, b: &mut B) -> Eval<V> {
        let db = Self::database(f, b, "liquidations")?;
        let key = format!(
            "liquidationOrders-{}-{}",
            text(&arg(&f.args, 1))?,
            text(&arg(&f.args, 0))?
        );
        let liquidations = match Self::db(f, b, &db, "findOneAsync", vec![obj(&[("_id", s(&key))])])
        {
            Err(Fault::Thrown(_)) => V::Array(vec![]),
            other => other?,
        };
        let mut count = Bn::c(0);
        let mut orders = Bn::c(0);
        let mut bank = Bn::c(0);
        let mut average_bankruptcy = Bn::c(0);
        if truth(&liquidations) && number(&prop(&liquidations, "length")?)? > 0.0 {
            for liquidation in
                source_array(&liquidations, "liquidations.forEach is not a function")?
            {
                orders = orders.add(&Bn::c(1))?;
                count = count.add(&Bn::from(&prop(&liquidation, "contractCount")?)?)?;
                bank = bank.add(
                    &Bn::from(&prop(&liquidation, "size")?)?
                        .mul(&Bn::from(&prop(&liquidation, "bankruptcyPrice")?)?)?,
                )?;
                // avgBankrupcyPrice and zero-valued buy/sell counters are
                // calculated but never returned by the pinned source.
                average_bankruptcy =
                    average_bankruptcy.add(&Bn::from(&prop(&liquidation, "bankruptcyPrice")?)?)?;
            }
        }
        bank = bank.div(&count)?;
        let _unused_average = average_bankruptcy.div(&orders)?;
        let db = Self::database(f, b, "tradeHistory")?;
        let trades = Self::db(f, b, &db, "findAsync", vec![])?;
        let mut filled = Bn::c(0);
        let mut vwap = Bn::c(0);
        if matches!(trades, V::Null | V::Undefined) {
            prop(&trades, "forEach")?;
        }
        for trade in source_array(&trades, "trades.forEach is not a function")? {
            let inner = prop(&trade, "trade")?;
            if strict(&prop(&inner, "isLiq")?, &V::Bool(true))
                && loose_equal(&prop(&trade, "blockHeight")?, &arg(&f.args, 0))?
            {
                filled = filled.add(&Bn::from(&prop(&inner, "amount")?)?)?;
                vwap = vwap.add(&Bn::from(&prop(&inner, "tradePrice")?)?)?;
            }
        }
        vwap = vwap.div(&filled)?;
        Ok(obj(&[
            ("liqTotal", count.value()),
            ("liqOrders", orders.value()),
            ("unfilled", count.sub(&filled)?.value()),
            ("bankruptcyVWAPPreFill", bank.value()),
            ("filledVWAP", vwap.value()),
            ("lossDelta", bank.sub(&vwap)?.value()),
        ]))
    }
    fn record_trade<B: SourceBackend>(f: &mut Frame, b: &mut B) -> Eval<V> {
        let trade = arg(&f.args, 0);
        let record = obj(&[
            (
                "key",
                s(&format!("contract-{}", text(&prop(&trade, "contractId")?)?)),
            ),
            ("type", s("contract")),
            ("trade", trade),
            ("blockHeight", arg(&f.args, 1)),
            ("sellerTx", arg(&f.args, 2)),
            ("buyerTx", arg(&f.args, 3)),
        ]);
        Self::save_trade(record, f, b)
    }
    fn save_trade<B: SourceBackend>(record: V, f: &mut Frame, b: &mut B) -> Eval<V> {
        let db = Self::database(f, b, "tradeHistory")?;
        let uuid = Self::call(f, b, "uuid.v4", vec![], false)?;
        let key = format!(
            "{}-{}-{}",
            text(&prop(&record, "key")?)?,
            text(&uuid)?,
            text(&prop(&record, "blockHeight")?)?
        );
        let mut document = obj(&[("_id", s(&key))]);
        for (k, v) in object_fields(&spread(&record)?)? {
            set(&mut document, &k, v)?;
        }
        Self::db(
            f,
            b,
            &db,
            "updateAsync",
            vec![
                obj(&[("_id", s(&key))]),
                document,
                obj(&[("upsert", V::Bool(true))]),
            ],
        )?;
        Ok(V::Undefined)
    }
    fn run<B: SourceBackend>(
        &mut self,
        receiver: Option<u64>,
        method: &str,
        f: &mut Frame,
        b: &mut B,
    ) -> Eval<V> {
        match method {
            "loadMarginMap" | "getInstance" => {
                return self.load(
                    arg(&f.args, 0),
                    if method == "loadMarginMap" {
                        arg(&f.args, 1)
                    } else {
                        V::Undefined
                    },
                    f,
                    b,
                )
            }
            "getPositionForAddress" => {
                return self.get_position(arg(&f.args, 0), arg(&f.args, 1), f, b)
            }
            "getAllPositions" => return self.all_positions(arg(&f.args, 0), arg(&f.args, 1), f, b),
            "calculateLiquidationPrice" => return liquidation(&f.args),
            "updateAveragePrice" => {
                return average(
                    &arg(&f.args, 0),
                    &arg(&f.args, 1),
                    &arg(&f.args, 2),
                    &arg(&f.args, 4),
                )
            }
            "calculateMarginRequirement" => {
                let a = Bn::from(&arg(&f.args, 0))?;
                let p = Bn::from(&arg(&f.args, 1))?;
                return Ok(if strict(&arg(&f.args, 2), &V::Bool(true)) {
                    a.div(&p)?
                } else {
                    a.mul(&p)?
                }
                .mul(&Bn::from(&s("0.1"))?)?
                .dp(8)?
                .value());
            }
            "estimatePNL" => {
                let entry = Bn::from(&arg(&f.args, 1))?;
                let exit = Bn::from(&arg(&f.args, 2))?;
                let supplied = arg(&f.args, 4);
                let notional = if truth(&supplied) { supplied } else { n(1.0) };
                let change = if truth(&arg(&f.args, 3)) {
                    Bn::c(1)
                        .div(&entry)?
                        .sub(&Bn::c(1).div(&exit)?)?
                        .mul(&Bn::from(&notional)?)?
                } else {
                    exit.sub(&entry)?
                };
                return Ok(change.mul(&Bn::from(&arg(&f.args, 0))?)?.dp(8)?.value());
            }
            "recordMarginMapDelta" => return self.delta(&f.args.clone(), f, b),
            "computeMaintenanceMarginRequirement" => {
                return Self::maintenance(arg(&f.args, 0), arg(&f.args, 1), f, b)
            }
            "generateLiquidationOrder" => return Self::generate_liquidation(&f.args),
            "calcPriceDelta" => return Self::price_delta(&f.args),
            "calculateNetExposure" => {
                return Err(thrown("ReferenceError", "ContractRegistry is not defined"))
            }
            "getMarketPrice" => {
                return Err(thrown("ReferenceError", "ContractsRegistry is not defined"))
            }
            "fetchLiquidationVolume" => return Self::fetch_liquidation_volume(f, b),
            "saveLiquidationOrders" => return Self::save_liquidations(f, b),
            "recordContractTrade" => return Self::record_trade(f, b),
            "saveTrade" => return Self::save_trade(arg(&f.args, 0), f, b),
            _ => {}
        }
        let id = receiver.ok_or(SourceRuleError::InvalidInput)?;
        self.inst(id)?;
        match method {
            "setInitialMargin" => self.set_initial(id, f, b),
            "saveMarginMap" => self.save(id, arg(&f.args, 0), f, b),
            "updateContractBalances" => {
                let p = self.root(f, 4, None)?;
                self.update(id, p, &f.args.clone(), f, b)
            }
            "updateContractBalancesWithMatch" => self.with_match(id, f, b),
            "realizePnl" => self.realize(f, b),
            "settlePNL" => self.settle(id, f, b),
            "reduceMargin" => self.reduce(id, f, b),
            "feeMarginReduce" => self.fee_reduce(id, f, b),
            "writePositionToMap" => self.write_position(f, b),
            "mergePositions" => self.merge_positions(id, f, b),
            "cleanupFlatPositions" => self.cleanup_flat(id, f, b),
            "isBelowMaintenanceMargin" => self.below_maintenance(id, f, b),
            "updateMargin" => self.update_margin(id, f, b),
            "clear" => self.clear_position(id, f, b),
            "adjustDeleveraging" => self.adjust_deleveraging(id, f, b),
            "simpleDeleverage" => self.simple_deleverage(id, f, b),
            "dynamicDeleverage" => self.dynamic_deleverage(f, b),
            "executeDeleveraging" => self.execute_deleveraging(f, b),
            "needsLiquidation" => {
                let key = text(&prop(&arg(&f.args, 0), "id")?)?;
                if [
                    "size",
                    "get",
                    "set",
                    "has",
                    "delete",
                    "clear",
                    "entries",
                    "keys",
                    "values",
                    "forEach",
                    "constructor",
                    "__proto__",
                    "toString",
                    "valueOf",
                    "hasOwnProperty",
                    "isPrototypeOf",
                    "propertyIsEnumerable",
                    "toLocaleString",
                    "__defineGetter__",
                    "__defineSetter__",
                    "__lookupGetter__",
                    "__lookupSetter__",
                ]
                .contains(&key.as_str())
                {
                    Ok(V::Bool(false))
                } else {
                    Err(ty("Cannot convert undefined or null to object"))
                }
            }
            "_touchExpiryIndex" => self.option_index_touch(id, &f.args),
            "getExpiringTickersUpTo" => self.option_index_get(id, &arg(&f.args, 0)),
            "cleanupExpiredTickersUpTo" => self.option_index_cleanup(id, &arg(&f.args, 0)),
            _ => Err(SourceRuleError::UnknownMethod.into()),
        }
    }
    fn write_position<B: SourceBackend>(&mut self, f: &mut Frame, b: &mut B) -> Eval<V> {
        let result = (|| {
            let series = arg(&f.args, 0);
            let position = arg(&f.args, 1);
            let key = stringify(&obj(&[("seriesId", series)]))?;
            let db = Self::database(f, b, "marginMaps")?;
            let row = Self::db(f, b, &db, "findOneAsync", vec![obj(&[("_id", s(&key))])])?;
            let mut pairs = if truth(&row) {
                let parsed = parse_json(&prop(&row, "value")?)?;
                let V::Array(p) = parsed else {
                    return Err(ty("object is not iterable"));
                };
                p
            } else {
                vec![]
            };
            let mut normalized: Vec<V> = vec![];
            for pair in pairs {
                if !matches!(pair, V::Array(_) | V::Object(_)) {
                    return Err(ty("Iterator value is not an entry object"));
                }
                let key = prop(&pair, "0")?;
                primitive(&key)?;
                let key = if matches!(&key,V::Number(bits)if f64::from_bits(*bits)==0.0) {
                    n(0.0)
                } else {
                    key
                };
                let value = prop(&pair, "1")?;
                if let Some(previous) = normalized
                    .iter_mut()
                    .find(|p| key_eq(&prop(p, "0").unwrap_or(V::Undefined), &key))
                {
                    *previous = V::Array(vec![key, value]);
                } else {
                    normalized.push(V::Array(vec![key, value]));
                }
            }
            pairs = normalized;
            let address = prop(&position, "address")?;
            primitive(&address)?;
            if let Some(pair) = pairs
                .iter_mut()
                .find(|pair| key_eq(&prop(pair, "0").unwrap_or(V::Undefined), &address))
            {
                *pair = V::Array(vec![address, position])
            } else {
                pairs.push(V::Array(vec![address, position]))
            }
            let value = stringify(&V::Array(pairs))?;
            Self::db(
                f,
                b,
                &db,
                "updateAsync",
                vec![
                    obj(&[("_id", s(&key))]),
                    obj(&[("_id", s(&key)), ("value", s(&value))]),
                    obj(&[("upsert", V::Bool(true))]),
                ],
            )?;
            Ok(V::Bool(true))
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Bool(false)),
            other => other,
        }
    }
    pub fn evaluate<B: SourceBackend>(
        &mut self,
        call: &SourceCall,
        backend: &mut B,
    ) -> Result<SourceOutcome, SourceRuleError> {
        Ok(self.evaluate_detailed(None, call, &[], backend)?.outcome)
    }
    pub fn evaluate_instance<B: SourceBackend>(
        &mut self,
        instance: u64,
        call: &SourceCall,
        backend: &mut B,
    ) -> Result<SourceOutcome, SourceRuleError> {
        Ok(self
            .evaluate_detailed(Some(instance), call, &[], backend)?
            .outcome)
    }
    pub fn evaluate_detailed<B: SourceBackend>(
        &mut self,
        instance: Option<u64>,
        call: &SourceCall,
        aliases: &[PositionAlias],
        backend: &mut B,
    ) -> Result<SourceEvaluation, SourceRuleError> {
        self.evaluate_detailed_with_array_aliases(instance, call, aliases, &[], backend)
    }
    /// Array-element aliases require the same explicit established-reference
    /// knowledge as root position aliases. Equal copied values are insufficient.
    pub fn evaluate_detailed_with_array_aliases<B: SourceBackend>(
        &mut self,
        instance: Option<u64>,
        call: &SourceCall,
        aliases: &[PositionAlias],
        array_aliases: &[PositionArrayAlias],
        backend: &mut B,
    ) -> Result<SourceEvaluation, SourceRuleError> {
        self.evaluate_detailed_with_object_aliases(
            instance,
            call,
            aliases,
            array_aliases,
            &[],
            backend,
        )
    }
    pub fn evaluate_detailed_with_object_aliases<B: SourceBackend>(
        &mut self,
        instance: Option<u64>,
        call: &SourceCall,
        aliases: &[PositionAlias],
        array_aliases: &[PositionArrayAlias],
        object_aliases: &[PositionObjectAlias],
        backend: &mut B,
    ) -> Result<SourceEvaluation, SourceRuleError> {
        Ok(self
            .evaluate_detailed_with_position_identity(
                instance,
                call,
                aliases,
                array_aliases,
                object_aliases,
                &[],
                false,
                backend,
            )?
            .evaluation)
    }
    /// Tracks actual returned object identity and explicit nested-field aliases.
    /// The caller must have established provenance from this owner's original
    /// return; equal position values do not establish an alias.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_detailed_with_position_identity<B: SourceBackend>(
        &mut self,
        instance: Option<u64>,
        call: &SourceCall,
        aliases: &[PositionAlias],
        array_aliases: &[PositionArrayAlias],
        object_aliases: &[PositionObjectAlias],
        field_aliases: &[PositionFieldObjectAlias],
        track_return: bool,
        backend: &mut B,
    ) -> Result<PositionIdentityEvaluation, SourceRuleError> {
        if !METHODS.contains(&call.method.as_str()) {
            return Err(SourceRuleError::UnknownMethod);
        }
        if call.arguments.len() > 16
            || aliases.len() > 16
            || array_aliases.len() > MAX_OBJECTS
            || object_aliases.len() > MAX_OBJECTS
            || field_aliases.len() > MAX_OBJECTS
        {
            return Err(SourceRuleError::Capacity);
        }
        for a in &call.arguments {
            a.validate()?
        }
        if let Some(id) = instance {
            self.inst(id)?;
        }
        let mut f = Frame {
            args: call.arguments.clone(),
            aliases: aliases.to_vec(),
            array_aliases: array_aliases.to_vec(),
            object_aliases: object_aliases.to_vec(),
            field_aliases: field_aliases.to_vec(),
            track_return,
            ..Frame::default()
        };
        for alias in &f.aliases {
            let value = self.inspect_position(alias.instance, &alias.address)?;
            if matches!(value, V::Undefined) || alias.argument >= f.args.len() {
                return Err(SourceRuleError::InvalidInput);
            }
            f.args[alias.argument] = value;
        }
        for alias in &f.array_aliases {
            let value = self.inspect_position(alias.instance, &alias.address)?;
            if matches!(value, V::Undefined) {
                return Err(SourceRuleError::InvalidInput);
            }
            let Some(V::Array(array)) = f.args.get_mut(alias.argument) else {
                return Err(SourceRuleError::InvalidInput);
            };
            let Some(element) = array.get_mut(alias.index) else {
                return Err(SourceRuleError::InvalidInput);
            };
            *element = value;
        }
        for (index, alias) in f.object_aliases.iter().enumerate() {
            if f.object_aliases[..index].iter().any(|previous| {
                previous.argument == alias.argument && previous.index == alias.index
            }) || (alias.index.is_none()
                && f.aliases.iter().any(|a| a.argument == alias.argument))
                || f.array_aliases
                    .iter()
                    .any(|a| a.argument == alias.argument && Some(a.index) == alias.index)
            {
                return Err(SourceRuleError::InvalidInput);
            }
            let value = self.inspect_position_object(alias.object)?;
            let Some(argument) = f.args.get_mut(alias.argument) else {
                return Err(SourceRuleError::InvalidInput);
            };
            if let Some(index) = alias.index {
                let V::Array(array) = argument else {
                    return Err(SourceRuleError::InvalidInput);
                };
                let Some(element) = array.get_mut(index) else {
                    return Err(SourceRuleError::InvalidInput);
                };
                *element = value;
            } else {
                *argument = value;
            }
        }
        for (index, alias) in f.field_aliases.iter().enumerate() {
            if f.field_aliases[..index]
                .iter()
                .any(|a| a.argument == alias.argument && a.field == alias.field)
                || f.object_aliases
                    .iter()
                    .any(|a| a.argument == alias.argument && a.index.is_none())
                || f.aliases.iter().any(|a| a.argument == alias.argument)
            {
                return Err(SourceRuleError::InvalidInput);
            }
            let value = self.inspect_position_object(alias.object)?;
            let Some(argument) = f.args.get_mut(alias.argument) else {
                return Err(SourceRuleError::InvalidInput);
            };
            if !matches!(argument, V::Object(_)) {
                return Err(SourceRuleError::InvalidInput);
            }
            set(argument, &alias.field, value).map_err(|_| SourceRuleError::InvalidInput)?;
        }
        let result = self.run(instance, &call.method, &mut f, backend);
        for (index, field, p) in &f.roots {
            if let Some(v) = f.args.get_mut(*index) {
                if let Some(field) = field {
                    set(v, field, self.objects[*p].clone())
                        .map_err(|_| SourceRuleError::InvalidInput)?
                } else {
                    *v = self.objects[*p].clone()
                }
            }
        }
        for (argument, index, p) in &f.array_roots {
            let Some(V::Array(array)) = f.args.get_mut(*argument) else {
                return Err(SourceRuleError::InvalidInput);
            };
            let Some(element) = array.get_mut(*index) else {
                return Err(SourceRuleError::InvalidInput);
            };
            *element = self.objects[*p].clone();
        }
        self.state_value()?.validate()?;
        let outcome = match result {
            Ok(v) => SourceOutcome::Returned(v),
            Err(Fault::Thrown(e)) => SourceOutcome::Thrown(e),
            Err(Fault::Local(e)) => return Err(e),
        };
        let returned_position = if matches!(outcome, SourceOutcome::Returned(_)) {
            f.returned_position
        } else {
            None
        };
        Ok(PositionIdentityEvaluation {
            evaluation: SourceEvaluation {
                outcome,
                arguments_after: f.args,
                dependencies: f.observations,
                deferred_rejections: f.deferred,
                global_writes: vec![],
            },
            returned_position,
        })
    }
    pub fn invoke_source<B: SourceBackend>(
        &mut self,
        method: &str,
        args: &[V],
        backend: &mut B,
    ) -> Result<DependencyReturn, SourceRuleError> {
        self.invoke_source_instance(None, method, args, &[], backend)
    }
    pub fn invoke_source_instance<B: SourceBackend>(
        &mut self,
        instance: Option<u64>,
        method: &str,
        args: &[V],
        aliases: &[PositionAlias],
        backend: &mut B,
    ) -> Result<DependencyReturn, SourceRuleError> {
        self.invoke_source_instance_with_array_aliases(
            instance,
            method,
            args,
            aliases,
            &[],
            backend,
        )
    }
    pub fn invoke_source_instance_with_array_aliases<B: SourceBackend>(
        &mut self,
        instance: Option<u64>,
        method: &str,
        args: &[V],
        aliases: &[PositionAlias],
        array_aliases: &[PositionArrayAlias],
        backend: &mut B,
    ) -> Result<DependencyReturn, SourceRuleError> {
        self.invoke_source_instance_with_object_aliases(
            instance,
            method,
            args,
            aliases,
            array_aliases,
            &[],
            backend,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn invoke_source_instance_with_object_aliases<B: SourceBackend>(
        &mut self,
        instance: Option<u64>,
        method: &str,
        args: &[V],
        aliases: &[PositionAlias],
        array_aliases: &[PositionArrayAlias],
        object_aliases: &[PositionObjectAlias],
        backend: &mut B,
    ) -> Result<DependencyReturn, SourceRuleError> {
        let synchronous = matches!(
            method,
            "_touchExpiryIndex"
                | "calculateLiquidationPrice"
                | "calculateMarginRequirement"
                | "estimatePNL"
                | "generateLiquidationOrder"
                | "calcPriceDelta"
                | "needsLiquidation"
        );
        Ok(
            match self
                .evaluate_detailed_with_object_aliases(
                    instance,
                    &SourceCall {
                        method: method.into(),
                        arguments: args.to_vec(),
                    },
                    aliases,
                    array_aliases,
                    object_aliases,
                    backend,
                )?
                .outcome
            {
                SourceOutcome::Returned(v) if synchronous => DependencyReturn::Immediate(v),
                SourceOutcome::Thrown(e) if synchronous => DependencyReturn::Throw(e),
                SourceOutcome::Returned(v) => DependencyReturn::Pending(v),
                SourceOutcome::Thrown(e) => DependencyReturn::PendingThrow(e),
            },
        )
    }
}

impl MarginMapSourceOwner {
    /// Complete live metadata, including shared position identities between
    /// map keys. Database rows remain exclusively in the shared backend.
    pub fn state_value(&self) -> Result<V, SourceRuleError> {
        let state = obj(&[
            (
                "instances",
                V::Array(
                    self.instances
                        .iter()
                        .map(|i| {
                            let mut value = obj(&[
                                ("seriesId", i.series.clone()),
                                (
                                    "margins",
                                    V::Array(
                                        i.margins
                                            .iter()
                                            .map(|(k, p)| V::Array(vec![k.clone(), n(*p as f64)]))
                                            .collect(),
                                    ),
                                ),
                            ]);
                            option_index_state_append(&mut value, i)?;
                            Ok(value)
                        })
                        .collect::<Result<Vec<_>, SourceRuleError>>()?,
                ),
            ),
            ("objects", V::Array(self.objects.clone())),
        ]);
        state.validate()?;
        Ok(state)
    }
    pub fn from_state_value(state: &V) -> Result<Self, SourceRuleError> {
        state.validate()?;
        let V::Array(instances) = state.get("instances") else {
            return Err(SourceRuleError::InvalidInput);
        };
        let V::Array(objects) = state.get("objects") else {
            return Err(SourceRuleError::InvalidInput);
        };
        if instances.len() > MAX_INSTANCES || objects.len() > MAX_OBJECTS {
            return Err(SourceRuleError::Capacity);
        }
        let mut owner = Self {
            instances: vec![],
            objects,
        };
        for item in instances {
            let series = item.get("seriesId");
            let V::Array(pairs) = item.get("margins") else {
                return Err(SourceRuleError::InvalidInput);
            };
            let id = owner.create_instance(series)?;
            owner.option_index_state_load(id, &item)?;
            for pair in pairs {
                let V::Array(pair) = pair else {
                    return Err(SourceRuleError::InvalidInput);
                };
                if pair.len() != 2 {
                    return Err(SourceRuleError::InvalidInput);
                }
                primitive(&pair[0])?;
                if matches!(&pair[0],V::Number(bits)if f64::from_bits(*bits)==0.0&&f64::from_bits(*bits).is_sign_negative())
                {
                    return Err(SourceRuleError::InvalidInput);
                }
                let V::Number(bits) = pair[1] else {
                    return Err(SourceRuleError::InvalidInput);
                };
                let raw = f64::from_bits(bits);
                if !raw.is_finite()
                    || raw < 0.0
                    || raw.fract() != 0.0
                    || raw as usize >= owner.objects.len()
                {
                    return Err(SourceRuleError::InvalidInput);
                }
                if owner
                    .inst(id)?
                    .margins
                    .iter()
                    .any(|(k, _)| key_eq(k, &pair[0]))
                {
                    return Err(SourceRuleError::InvalidInput);
                }
                owner
                    .inst_mut(id)?
                    .margins
                    .push((pair[0].clone(), raw as usize));
            }
        }
        if owner.state_value()? != *state {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(owner)
    }
    pub fn import_state(&mut self, state: &V) -> Result<(), SourceRuleError> {
        *self = Self::from_state_value(state)?;
        Ok(())
    }
    pub fn checkpoint(&self) -> Result<Vec<u8>, SourceRuleError> {
        let mut bytes = b"TLMP\x01".to_vec();
        put_value(&mut bytes, &self.state_value()?)?;
        if bytes.len() > MAX_STATE_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        with_crc(bytes).map_err(|_| SourceRuleError::Capacity)
    }
    pub fn from_checkpoint(bytes: &[u8]) -> Result<Self, SourceRuleError> {
        if bytes.len() > MAX_STATE_BYTES + 4 {
            return Err(SourceRuleError::Capacity);
        }
        let raw = verify_crc(bytes).map_err(|_| SourceRuleError::InvalidInput)?;
        let mut c = Cursor::new(raw);
        if c.take(5).map_err(|_| SourceRuleError::InvalidInput)? != b"TLMP\x01" {
            return Err(SourceRuleError::InvalidInput);
        }
        let state = get_value(&mut c, 0, &mut Budget::default())?;
        c.end().map_err(|_| SourceRuleError::InvalidInput)?;
        let owner = Self::from_state_value(&state)?;
        if owner.checkpoint()? != bytes {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(owner)
    }
}
fn put_string(bytes: &mut Vec<u8>, s: &str) -> Result<(), SourceRuleError> {
    let len = u32::try_from(s.len()).map_err(|_| SourceRuleError::Capacity)?;
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(s.as_bytes());
    Ok(())
}
fn put_value(bytes: &mut Vec<u8>, v: &V) -> Result<(), SourceRuleError> {
    match v {
        V::Undefined => bytes.push(0),
        V::Null => bytes.push(1),
        V::Bool(false) => bytes.push(2),
        V::Bool(true) => bytes.push(3),
        V::Number(bits) => {
            bytes.push(4);
            bytes.extend_from_slice(&bits.to_be_bytes())
        }
        V::String(s) => {
            bytes.push(5);
            put_string(bytes, s)?
        }
        V::Array(a) => {
            bytes.push(6);
            bytes.extend_from_slice(&(a.len() as u32).to_be_bytes());
            for v in a {
                put_value(bytes, v)?
            }
        }
        V::Object(o) => {
            bytes.push(7);
            bytes.extend_from_slice(&(o.len() as u32).to_be_bytes());
            for (k, v) in o {
                put_string(bytes, k)?;
                put_value(bytes, v)?
            }
        }
        V::Promise => bytes.push(8),
        V::BigNumber(s) => {
            bytes.push(9);
            put_string(bytes, s)?
        }
    }
    if bytes.len() > MAX_STATE_BYTES {
        return Err(SourceRuleError::Capacity);
    }
    Ok(())
}
#[derive(Default)]
struct Budget {
    nodes: usize,
    text: usize,
}
fn count(c: &mut Cursor<'_>) -> Result<usize, SourceRuleError> {
    let len = c.u32().map_err(|_| SourceRuleError::InvalidInput)? as usize;
    if len > 1024 {
        Err(SourceRuleError::Capacity)
    } else {
        Ok(len)
    }
}
fn get_string(c: &mut Cursor<'_>, b: &mut Budget) -> Result<String, SourceRuleError> {
    let len = c.u32().map_err(|_| SourceRuleError::InvalidInput)? as usize;
    b.text = b.text.checked_add(len).ok_or(SourceRuleError::Capacity)?;
    if b.text > crate::validity_source::MAX_BYTES {
        return Err(SourceRuleError::Capacity);
    }
    let value = c.take(len).map_err(|_| SourceRuleError::InvalidInput)?;
    String::from_utf8(value.to_vec()).map_err(|_| SourceRuleError::InvalidInput)
}
fn get_value(c: &mut Cursor<'_>, depth: usize, b: &mut Budget) -> Result<V, SourceRuleError> {
    b.nodes = b.nodes.checked_add(1).ok_or(SourceRuleError::Capacity)?;
    if b.nodes > crate::validity_source::MAX_NODES || depth > crate::validity_source::MAX_DEPTH {
        return Err(SourceRuleError::Capacity);
    }
    Ok(match c.u8().map_err(|_| SourceRuleError::InvalidInput)? {
        0 => V::Undefined,
        1 => V::Null,
        2 => V::Bool(false),
        3 => V::Bool(true),
        4 => {
            let bits = c.u64().map_err(|_| SourceRuleError::InvalidInput)?;
            let f = f64::from_bits(bits);
            if f.is_nan() && bits != f64::NAN.to_bits() {
                return Err(SourceRuleError::InvalidInput);
            }
            V::Number(bits)
        }
        5 => V::String(get_string(c, b)?),
        6 => {
            let len = count(c)?;
            if b.nodes.saturating_add(len) > crate::validity_source::MAX_NODES {
                return Err(SourceRuleError::Capacity);
            }
            let mut a = vec![];
            for _ in 0..len {
                a.push(get_value(c, depth + 1, b)?)
            }
            V::Array(a)
        }
        7 => {
            let len = count(c)?;
            if b.nodes.saturating_add(len) > crate::validity_source::MAX_NODES {
                return Err(SourceRuleError::Capacity);
            }
            let mut a = vec![];
            for _ in 0..len {
                let key = get_string(c, b)?;
                if a.iter().any(|(k, _)| *k == key) {
                    return Err(SourceRuleError::InvalidInput);
                }
                a.push((key, get_value(c, depth + 1, b)?))
            }
            V::Object(a)
        }
        8 => V::Promise,
        9 => V::BigNumber(get_string(c, b)?),
        _ => return Err(SourceRuleError::InvalidInput),
    })
}
fn ty(message: impl Into<String>) -> Fault {
    thrown("TypeError", message)
}
fn s(v: &str) -> V {
    V::String(v.into())
}
fn n(v: f64) -> V {
    V::number(v)
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn arg(v: &[V], i: usize) -> V {
    v.get(i).cloned().unwrap_or(V::Undefined)
}
fn truth(v: &V) -> bool {
    match v {
        V::Undefined | V::Null | V::Bool(false) => false,
        V::Number(b) => {
            let f = f64::from_bits(*b);
            f != 0.0 && !f.is_nan()
        }
        V::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn or_zero(v: V) -> V {
    if truth(&v) {
        v
    } else {
        n(0.0)
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
fn key_eq(a: &V, b: &V) -> bool {
    strict(a, b)
        || matches!((a,b),(V::Number(a),V::Number(b)) if f64::from_bits(*a).is_nan()&&f64::from_bits(*b).is_nan())
}
fn primitive(v: &V) -> Result<(), SourceRuleError> {
    if matches!(v, V::Object(_) | V::Array(_) | V::Promise | V::BigNumber(_)) {
        Err(SourceRuleError::UnsupportedNumericDomain)
    } else {
        Ok(())
    }
}
fn prop(v: &V, key: &str) -> Eval<V> {
    if matches!(v, V::Undefined | V::Null) {
        Err(ty(format!(
            "Cannot read properties of {} (reading '{key}')",
            if matches!(v, V::Null) {
                "null"
            } else {
                "undefined"
            }
        )))
    } else {
        Ok(match v {
            V::Array(a) if key != "length" => key
                .parse::<usize>()
                .ok()
                .filter(|i| i.to_string() == key)
                .and_then(|i| a.get(i))
                .cloned()
                .unwrap_or(V::Undefined),
            _ => v.get(key),
        })
    }
}
fn default_margin_position() -> V {
    obj(&[
        ("contracts", n(0.0)),
        ("margin", n(0.0)),
        ("unrealizedPNL", n(0.0)),
        ("avgPrice", n(0.0)),
    ])
}
fn source_array(v: &V, message: &str) -> Eval<Vec<V>> {
    match v {
        V::Array(a) => Ok(a.clone()),
        V::Null | V::Undefined if message.contains(".map") => {
            prop(v, "map")?;
            unreachable!()
        }
        V::Null | V::Undefined if message.contains(".forEach") => {
            prop(v, "forEach")?;
            unreachable!()
        }
        _ => Err(ty(message)),
    }
}
fn object_fields(v: &V) -> Eval<Vec<(String, V)>> {
    match v {
        V::Object(fields) => Ok(fields.clone()),
        _ => Err(SourceRuleError::UnsupportedNumericDomain.into()),
    }
}
fn spread(v: &V) -> Eval<V> {
    Ok(match v {
        V::Object(fields) => V::Object(fields.clone()),
        V::Array(a) => V::Object(
            a.iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect(),
        ),
        V::String(s) if s.is_ascii() => V::Object(
            s.chars()
                .enumerate()
                .map(|(i, v)| (i.to_string(), V::String(v.to_string())))
                .collect(),
        ),
        V::String(_) | V::BigNumber(_) => {
            return Err(SourceRuleError::UnsupportedNumericDomain.into())
        }
        _ => obj(&[]),
    })
}
fn bn_gt(a: &Bn, b: &Bn) -> bool {
    a.cmp(b) == Some(core::cmp::Ordering::Greater)
}
fn sort_counterparties(cps: &mut [(usize, V, Bn)], by_pnl: bool) -> Eval<()> {
    let mut fault = None;
    cps.sort_by(|(_, a, ap), (_, b, bp)| {
        let comparison = (|| {
            let ap = ap.abs();
            let bp = bp.abs();
            let difference = if by_pnl && ap.cmp(&bp) != Some(core::cmp::Ordering::Equal) {
                bp.sub(&ap)?.f64()
            } else {
                number(&prop(b, "contracts")?)?.abs() - number(&prop(a, "contracts")?)?.abs()
            };
            Ok(difference
                .partial_cmp(&0.0)
                .unwrap_or(core::cmp::Ordering::Equal))
        })();
        match comparison {
            Ok(ordering) => ordering,
            Err(e) => {
                fault = Some(e);
                core::cmp::Ordering::Equal
            }
        }
    });
    if let Some(e) = fault {
        Err(e)
    } else {
        Ok(())
    }
}
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() || b.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else {
        a.min(b)
    }
}
fn loose_equal(a: &V, b: &V) -> Eval<bool> {
    if matches!(a, V::Null | V::Undefined) || matches!(b, V::Null | V::Undefined) {
        return Ok(matches!(a, V::Null | V::Undefined) && matches!(b, V::Null | V::Undefined));
    }
    if matches!(
        (a, b),
        (V::String(_), V::String(_)) | (V::Bool(_), V::Bool(_))
    ) {
        return Ok(strict(a, b));
    }
    if matches!(a, V::Object(_) | V::Array(_) | V::Promise | V::BigNumber(_))
        || matches!(b, V::Object(_) | V::Array(_) | V::Promise | V::BigNumber(_))
    {
        return Err(SourceRuleError::UnsupportedNumericDomain.into());
    }
    Ok(number(a)? == number(b)?)
}
fn set(v: &mut V, key: &str, value: V) -> Eval<()> {
    if !matches!(v, V::Object(_)) {
        if matches!(v, V::Array(_) | V::BigNumber(_) | V::Promise) {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        if matches!(v, V::Null | V::Undefined) {
            return Err(ty(format!(
                "Cannot set properties of {} (setting '{key}')",
                text(v)?
            )));
        }
        let kind = match v {
            V::Number(_) => "number",
            V::Bool(_) => "boolean",
            _ => "string",
        };
        return Err(ty(format!(
            "Cannot create property '{key}' on {kind} '{}'",
            text(v)?
        )));
    }
    v.insert(key, value)?;
    Ok(())
}
fn number(v: &V) -> Eval<f64> {
    crate::oracle_source_owner::source_number_unverified(v).map_err(Fault::Local)
}
fn text(v: &V) -> Eval<String> {
    Ok(match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(v) => v.to_string(),
        V::Number(b) => {
            let v = f64::from_bits(*b);
            if v.is_nan() {
                "NaN".into()
            } else if v == f64::INFINITY {
                "Infinity".into()
            } else if v == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                crate::state_oracle::source_number_text(v)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
            }
        }
        V::String(v) => v.clone(),
        V::BigNumber(v) => v.clone(),
        V::Promise => "[object Promise]".into(),
        V::Array(a) => a
            .iter()
            .map(|v| {
                if matches!(v, V::Undefined | V::Null) {
                    Ok(String::new())
                } else {
                    text(v)
                }
            })
            .collect::<Eval<Vec<_>>>()?
            .join(","),
        V::Object(fields) => {
            if fields.iter().any(|(k, _)| k == "toString") {
                return Err(ty("Cannot convert object to primitive value"));
            }
            "[object Object]".into()
        }
    })
}

// BigNumber's default 20-place half-up division. Nonfinite values and signed
// zero remain source arithmetic, rather than being converted to economic errors.
#[derive(Clone, Debug)]
enum Bn {
    Finite(Decimal, bool),
    Infinity(bool),
    Nan,
}
impl Bn {
    fn from(v: &V) -> Eval<Self> {
        let raw = match v {
            V::Number(b) => {
                let f = f64::from_bits(*b);
                if f.is_nan() {
                    return Ok(Self::Nan);
                }
                if f.is_infinite() {
                    return Ok(Self::Infinity(f.is_sign_negative()));
                }
                if f == 0.0 {
                    return Ok(Self::Finite(Decimal::zero(), f.is_sign_negative()));
                }
                text(v)?
            }
            V::String(v) | V::BigNumber(v) => v.trim().to_string(),
            V::Array(_) | V::Object(_) => text(v)?,
            _ => return Ok(Self::Nan),
        };
        if raw == "NaN" || raw.is_empty() {
            return Ok(Self::Nan);
        }
        if raw == "Infinity" || raw == "+Infinity" {
            return Ok(Self::Infinity(false));
        }
        if raw == "-Infinity" {
            return Ok(Self::Infinity(true));
        }
        let body = raw
            .strip_prefix('-')
            .or_else(|| raw.strip_prefix('+'))
            .unwrap_or(&raw);
        if body.len() >= 2 {
            let radix = match body.get(..2) {
                Some("0x" | "0X") => Some(16),
                Some("0o" | "0O") => Some(8),
                Some("0b" | "0B") => Some(2),
                _ => None,
            };
            if let Some(radix) = radix {
                if body.len() > 4098 {
                    return Err(SourceRuleError::UnsupportedNumericDomain.into());
                }
                let digits = &body[2..];
                if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                    return Ok(Self::Nan);
                }
                let mut d = Decimal::zero();
                for c in digits.chars() {
                    d = d
                        .checked_mul(&Decimal::from(radix))
                        .and_then(|d| d.checked_add(&Decimal::from(c.to_digit(radix).unwrap_or(0))))
                        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                }
                let minus = raw.starts_with('-');
                let zero = d.is_zero() && minus;
                return Ok(Self::Finite(if minus { d.negated() } else { d }, zero));
            }
        }
        match Decimal::from_str(&raw) {
            Ok(d) => {
                let neg = d.is_zero() && raw.starts_with('-');
                Ok(Self::Finite(d, neg))
            }
            Err(crate::decimal::DecimalError::InvalidSyntax) => Ok(Self::Nan),
            Err(_) => Err(SourceRuleError::UnsupportedNumericDomain.into()),
        }
    }
    fn c(v: i32) -> Self {
        Self::Finite(Decimal::from(v), false)
    }
    fn negative(&self) -> bool {
        match self {
            Self::Finite(d, z) => d.is_negative() || (d.is_zero() && *z),
            Self::Infinity(s) => *s,
            Self::Nan => false,
        }
    }
    fn zero(&self) -> bool {
        matches!(self,Self::Finite(d,_)if d.is_zero())
    }
    fn neg(&self) -> Self {
        match self {
            Self::Finite(d, z) => Self::Finite(d.negated(), d.is_zero() && !*z),
            Self::Infinity(s) => Self::Infinity(!s),
            Self::Nan => Self::Nan,
        }
    }
    fn abs(&self) -> Self {
        match self {
            Self::Finite(d, _) => Self::Finite(d.abs(), false),
            Self::Infinity(_) => Self::Infinity(false),
            Self::Nan => Self::Nan,
        }
    }
    fn add(&self, b: &Self) -> Eval<Self> {
        Ok(match (self, b) {
            (Self::Nan, _) | (_, Self::Nan) => Self::Nan,
            (Self::Infinity(a), Self::Infinity(b)) if a != b => Self::Nan,
            (Self::Infinity(s), _) | (_, Self::Infinity(s)) => Self::Infinity(*s),
            (Self::Finite(a, az), Self::Finite(b, bz)) => Self::Finite(
                a.checked_add(b)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?,
                a.is_zero() && b.is_zero() && *az && *bz,
            ),
        })
    }
    fn sub(&self, b: &Self) -> Eval<Self> {
        self.add(&b.neg())
    }
    fn mul(&self, b: &Self) -> Eval<Self> {
        let neg = self.negative() != b.negative();
        Ok(match (self, b) {
            (Self::Nan, _) | (_, Self::Nan) => Self::Nan,
            (Self::Infinity(_), _) | (_, Self::Infinity(_)) => {
                if self.zero() || b.zero() {
                    Self::Nan
                } else {
                    Self::Infinity(neg)
                }
            }
            (Self::Finite(a, _), Self::Finite(b, _)) => {
                let d = a
                    .checked_mul(b)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                let z = d.is_zero() && neg;
                Self::Finite(d, z)
            }
        })
    }
    fn div(&self, b: &Self) -> Eval<Self> {
        let neg = self.negative() != b.negative();
        Ok(match (self, b) {
            (Self::Nan, _) | (_, Self::Nan) | (Self::Infinity(_), Self::Infinity(_)) => Self::Nan,
            (_, _) if b.zero() => {
                if self.zero() {
                    Self::Nan
                } else {
                    Self::Infinity(neg)
                }
            }
            (Self::Infinity(_), _) => Self::Infinity(neg),
            (_, Self::Infinity(_)) => Self::Finite(Decimal::zero(), neg),
            (Self::Finite(a, _), Self::Finite(b, _)) => {
                let d = a
                    .checked_div(b, 20, Rounding::HalfUp)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                let z = d.is_zero() && neg;
                Self::Finite(d, z)
            }
        })
    }
    fn dp(&self, places: u32) -> Eval<Self> {
        Ok(match self {
            Self::Finite(d, _) => {
                let d = d
                    .quantize(places, Rounding::HalfUp)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                let z = d.is_zero() && self.negative();
                Self::Finite(d, z)
            }
            other => other.clone(),
        })
    }
    fn f64(&self) -> f64 {
        match self {
            Self::Nan => f64::NAN,
            Self::Infinity(s) => {
                if *s {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                }
            }
            Self::Finite(d, z) => {
                if d.is_zero() && *z {
                    -0.0
                } else {
                    d.to_string().parse().unwrap_or(if d.is_negative() {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    })
                }
            }
        }
    }
    fn value(&self) -> V {
        n(self.f64())
    }
    fn tagged(&self) -> V {
        V::BigNumber(match self {
            Self::Finite(d, negative_zero) if d.is_zero() && *negative_zero => "-0".into(),
            Self::Finite(d, _) => big_text(d),
            Self::Infinity(true) => "-Infinity".into(),
            Self::Infinity(false) => "Infinity".into(),
            Self::Nan => "NaN".into(),
        })
    }
    fn cmp(&self, b: &Self) -> Option<core::cmp::Ordering> {
        match (self, b) {
            (Self::Nan, _) | (_, Self::Nan) => None,
            (Self::Finite(a, _), Self::Finite(b, _)) => Some(a.cmp(b)),
            _ => self.f64().partial_cmp(&b.f64()),
        }
    }
    fn max(&self, b: &Self) -> Self {
        match self.cmp(b) {
            None => Self::Nan,
            Some(core::cmp::Ordering::Less) => b.clone(),
            _ => self.clone(),
        }
    }
}
fn norm_json(v: &V) -> Eval<V> {
    Ok(match v {
        V::BigNumber(raw) => {
            let b = Bn::from(v)?;
            s(&match b {
                Bn::Finite(d, z) => {
                    if d.is_zero() && z {
                        "-0".into()
                    } else {
                        let raw = raw.trim();
                        if raw.is_empty() {
                            "NaN".into()
                        } else {
                            big_text(&d)
                        }
                    }
                }
                Bn::Infinity(s) => {
                    if s {
                        "-Infinity".into()
                    } else {
                        "Infinity".into()
                    }
                }
                Bn::Nan => "NaN".into(),
            })
        }
        V::Promise => obj(&[]),
        V::Array(a) => V::Array(a.iter().map(norm_json).collect::<Eval<_>>()?),
        V::Object(o) => V::Object(
            o.iter()
                .map(|(k, v)| Ok((k.clone(), norm_json(v)?)))
                .collect::<Eval<_>>()?,
        ),
        _ => v.clone(),
    })
}
fn big_text(d: &Decimal) -> String {
    let exp = d.coefficient_digits() as i32 - d.decimal_places() as i32 - 1;
    if d.is_zero() || (-6..21).contains(&exp) {
        return d.to_string();
    }
    let full = d.abs().to_string();
    let digits = full.chars().filter(|c| *c != '.').collect::<String>();
    let digits = digits.trim_start_matches('0').trim_end_matches('0');
    format!(
        "{}{}{}e{}{exp}",
        if d.is_negative() { "-" } else { "" },
        &digits[..1],
        if digits.len() > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        },
        if exp >= 0 { "+" } else { "" }
    )
}
fn stringify(v: &V) -> Eval<String> {
    crate::property_catalog::json_stringify(&norm_json(v)?).map_err(|e| match e {
        crate::property_catalog::CatalogError::Capacity => SourceRuleError::Capacity.into(),
        _ => SourceRuleError::UnsupportedNumericDomain.into(),
    })
}
fn parse_json(v: &V) -> Eval<V> {
    let text = text(v)?;
    crate::contract_queue_source_owner::parse_book_json_unverified(&text).map_err(|e| match e {
        crate::issuance_intent::IntentError::Capacity
        | crate::issuance_intent::IntentError::DepthLimit => {
            Fault::Local(SourceRuleError::Capacity)
        }
        crate::issuance_intent::IntentError::InvalidJson => {
            thrown("SyntaxError", "Invalid margin map JSON")
        }
        _ => Fault::Local(SourceRuleError::UnsupportedNumericDomain),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Instance {
    series: V,
    margins: Vec<(V, usize)>,
    expiry_index: Vec<(V, Vec<V>)>,
    ticker_expiry: Vec<(V, V)>,
    option_oi: Vec<(V, V)>,
}
/// Explicit adapter knowledge that an argument is an established cached alias.
/// Equal values alone never establish reference identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionAlias {
    pub argument: usize,
    pub instance: u64,
    pub address: V,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionArrayAlias {
    pub argument: usize,
    pub index: usize,
    pub instance: u64,
    pub address: V,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionObjectAlias {
    pub argument: usize,
    pub index: Option<usize>,
    pub object: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionFieldObjectAlias {
    pub argument: usize,
    pub field: String,
    pub object: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionIdentityEvaluation {
    pub evaluation: SourceEvaluation,
    pub returned_position: Option<usize>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarginMapSourceOwner {
    instances: Vec<Instance>,
    objects: Vec<V>,
}
#[derive(Default)]
struct Frame {
    args: Vec<V>,
    roots: Vec<(usize, Option<String>, usize)>,
    aliases: Vec<PositionAlias>,
    array_roots: Vec<(usize, usize, usize)>,
    array_aliases: Vec<PositionArrayAlias>,
    object_aliases: Vec<PositionObjectAlias>,
    field_aliases: Vec<PositionFieldObjectAlias>,
    track_return: bool,
    returned_position: Option<usize>,
    observations: Vec<DependencyObservation>,
    deferred: Vec<SourceException>,
}
impl MarginMapSourceOwner {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn create_instance(&mut self, series: V) -> Result<u64, SourceRuleError> {
        series.validate()?;
        if self.instances.len() >= MAX_INSTANCES {
            return Err(SourceRuleError::Capacity);
        }
        self.instances.push(Instance {
            series,
            margins: vec![],
            expiry_index: vec![],
            ticker_expiry: vec![],
            option_oi: vec![],
        });
        Ok(self.instances.len() as u64)
    }
    fn inst(&self, id: u64) -> Result<&Instance, SourceRuleError> {
        id.checked_sub(1)
            .and_then(|n| self.instances.get(n as usize))
            .ok_or(SourceRuleError::InvalidInput)
    }
    fn inst_mut(&mut self, id: u64) -> Result<&mut Instance, SourceRuleError> {
        id.checked_sub(1)
            .and_then(|n| self.instances.get_mut(n as usize))
            .ok_or(SourceRuleError::InvalidInput)
    }
    fn alloc(&mut self, v: V) -> Eval<usize> {
        v.validate()?;
        if self.objects.len() >= MAX_OBJECTS {
            return Err(SourceRuleError::Capacity.into());
        }
        self.objects.push(v);
        Ok(self.objects.len() - 1)
    }
    /// Allocate an actual source object known by the composing runtime. IDs are
    /// zero-based arena indices and remain stable across the owner checkpoint.
    pub fn allocate_position_object(&mut self, value: V) -> Result<usize, SourceRuleError> {
        self.alloc(value).map_err(|fault| match fault {
            Fault::Local(e) => e,
            Fault::Thrown(_) => SourceRuleError::InvalidInput,
        })
    }
    pub fn inspect_position_object(&self, object: usize) -> Result<V, SourceRuleError> {
        self.objects
            .get(object)
            .cloned()
            .ok_or(SourceRuleError::InvalidInput)
    }
    /// Actual map entries and their established arena identities, in Map order.
    /// This adapter projection never loads, allocates, or infers references.
    pub fn position_object_entries(
        &self,
        instance: u64,
    ) -> Result<Vec<(V, usize)>, SourceRuleError> {
        Ok(self.inst(instance)?.margins.clone())
    }
    pub fn replace_position_object(
        &mut self,
        object: usize,
        value: V,
    ) -> Result<(), SourceRuleError> {
        value.validate()?;
        let position = self
            .objects
            .get_mut(object)
            .ok_or(SourceRuleError::InvalidInput)?;
        *position = value;
        Ok(())
    }
    fn cached(&self, id: u64, address: &V) -> Eval<Option<usize>> {
        primitive(address)?;
        Ok(self
            .inst(id)?
            .margins
            .iter()
            .find(|(k, _)| key_eq(k, address))
            .map(|(_, p)| *p))
    }
    fn cache_set(&mut self, id: u64, key: V, p: usize) -> Eval<()> {
        primitive(&key)?;
        let key = if matches!(&key,V::Number(bits)if f64::from_bits(*bits)==0.0) {
            n(0.0)
        } else {
            key
        };
        let i = self.inst_mut(id)?;
        if let Some((_, old)) = i.margins.iter_mut().find(|(k, _)| key_eq(k, &key)) {
            *old = p
        } else {
            if i.margins.len() >= 1024 {
                return Err(SourceRuleError::Capacity.into());
            }
            i.margins.push((key, p))
        }
        Ok(())
    }
    pub fn inspect_position(&self, id: u64, address: &V) -> Result<V, SourceRuleError> {
        match self.cached(id, address) {
            Ok(Some(p)) => Ok(self.objects[p].clone()),
            Ok(None) => Ok(V::Undefined),
            Err(Fault::Local(e)) => Err(e),
            Err(_) => Err(SourceRuleError::InvalidInput),
        }
    }
    pub fn set_cached_position(
        &mut self,
        id: u64,
        address: V,
        position: V,
    ) -> Result<(), SourceRuleError> {
        let result = (|| {
            let p = self.alloc(position)?;
            self.cache_set(id, address, p)
        })();
        match result {
            Ok(()) => Ok(()),
            Err(Fault::Local(e)) => Err(e),
            Err(_) => Err(SourceRuleError::InvalidInput),
        }
    }
    pub fn instance_value(&self, id: u64) -> Result<V, SourceRuleError> {
        let i = self.inst(id)?;
        Ok(obj(&[
            ("seriesId", i.series.clone()),
            (
                "margins",
                V::Array(
                    i.margins
                        .iter()
                        .map(|(k, p)| V::Array(vec![k.clone(), self.objects[*p].clone()]))
                        .collect(),
                ),
            ),
            ("expiryIndex", option_index_expiry_value(&i.expiry_index)),
            ("tickerExpiry", option_index_map_value(&i.ticker_expiry)),
            ("optionOI", option_index_map_value(&i.option_oi)),
        ]))
    }
    fn root(&mut self, f: &mut Frame, index: usize, field: Option<&str>) -> Eval<usize> {
        let p = if field.is_none() {
            if let Some(alias) = f
                .object_aliases
                .iter()
                .find(|a| a.argument == index && a.index.is_none())
            {
                alias.object
            } else if let Some(alias) = f.aliases.iter().find(|a| a.argument == index) {
                self.cached(alias.instance, &alias.address)?
                    .ok_or(SourceRuleError::InvalidInput)?
            } else {
                self.alloc(arg(&f.args, index))?
            }
        } else if let Some(alias) = f
            .field_aliases
            .iter()
            .find(|a| a.argument == index && Some(a.field.as_str()) == field)
        {
            alias.object
        } else {
            self.alloc(prop(&arg(&f.args, index), field.unwrap_or_default())?)?
        };
        f.roots.push((index, field.map(str::to_string), p));
        Ok(p)
    }
    fn array_root(&mut self, f: &mut Frame, argument: usize, index: usize) -> Eval<usize> {
        if let Some((_, _, p)) = f
            .array_roots
            .iter()
            .find(|(a, i, _)| *a == argument && *i == index)
        {
            return Ok(*p);
        }
        let p = if let Some(alias) = f
            .object_aliases
            .iter()
            .find(|a| a.argument == argument && a.index == Some(index))
        {
            alias.object
        } else if let Some(alias) = f
            .array_aliases
            .iter()
            .find(|a| a.argument == argument && a.index == index)
        {
            self.cached(alias.instance, &alias.address)?
                .ok_or(SourceRuleError::InvalidInput)?
        } else {
            let array = source_array(&arg(&f.args, argument), "Position cache is not an array")?;
            let value = array
                .get(index)
                .ok_or(SourceRuleError::InvalidInput)?
                .clone();
            self.alloc(value)?
        };
        f.array_roots.push((argument, index, p));
        Ok(p)
    }
    fn handle(id: u64) -> V {
        obj(&[("sourceMarginInstance", n(id as f64))])
    }
    fn call<B: SourceBackend>(
        f: &mut Frame,
        b: &mut B,
        target: &str,
        args: Vec<V>,
        awaited: bool,
    ) -> Eval<V> {
        let call = DependencyCall {
            target: target.into(),
            arguments: args,
        };
        let returned = b.invoke(&call)?;
        f.observations.push(DependencyObservation {
            call,
            returned: returned.clone(),
        });
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
            DependencyReturn::Throw(e) => Err(Fault::Thrown(e)),
            DependencyReturn::PendingThrow(e) => {
                if awaited {
                    Err(Fault::Thrown(e))
                } else {
                    f.deferred.push(e);
                    Ok(V::Promise)
                }
            }
        }
    }
    fn database<B: SourceBackend>(f: &mut Frame, b: &mut B, name: &str) -> Eval<V> {
        Self::call(f, b, "db.getDatabase", vec![s(name)], true)
    }
    fn db<B: SourceBackend>(
        f: &mut Frame,
        b: &mut B,
        handle: &V,
        method: &str,
        mut args: Vec<V>,
    ) -> Eval<V> {
        if matches!(handle, V::Null | V::Undefined) {
            return Err(ty(format!(
                "Cannot read properties of {} (reading '{method}')",
                text(handle)?
            )));
        }
        args.insert(0, handle.clone());
        Self::call(f, b, &format!("db.{method}"), args, true)
    }
    fn load<B: SourceBackend>(&mut self, series: V, flag: V, f: &mut Frame, b: &mut B) -> Eval<V> {
        let key = stringify(&obj(&[("seriesId", series.clone())]))?;
        let db = Self::database(f, b, "marginMaps")?;
        let result = (|| {
            let doc = Self::db(f, b, &db, "findOneAsync", vec![obj(&[("_id", s(&key))])])?;
            if !truth(&doc) {
                return Ok(Self::handle(self.create_instance(series)?));
            }
            if truth(&flag) {
                return prop(&doc, "value");
            }
            let id = self.create_instance(series)?;
            let parsed = parse_json(&prop(&doc, "value")?)?;
            if let V::Array(pairs) = parsed {
                for pair in pairs {
                    if !matches!(pair, V::Array(_) | V::Object(_)) {
                        return Err(ty("Iterator value is not an entry object"));
                    }
                    let key = prop(&pair, "0")?;
                    let p = self.alloc(prop(&pair, "1")?)?;
                    self.cache_set(id, key, p)?
                }
            }
            Ok(Self::handle(id))
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Undefined),
            other => other,
        }
    }
    fn load_id<B: SourceBackend>(&mut self, series: V, f: &mut Frame, b: &mut B) -> Eval<u64> {
        let v = self.load(series, V::Bool(false), f, b)?;
        let id = v.get("sourceMarginInstance");
        if matches!(id, V::Undefined) {
            return Err(ty(
                "Cannot read properties of undefined (reading 'margins')",
            ));
        }
        Ok(number(&id)? as u64)
    }
    fn all_positions<B: SourceBackend>(
        &mut self,
        series: V,
        flag: V,
        f: &mut Frame,
        b: &mut B,
    ) -> Eval<V> {
        let v = self.load(series, flag, f, b)?;
        if matches!(v, V::Undefined | V::Null) {
            prop(&v, "margins")?;
        }
        let raw = v.get("sourceMarginInstance");
        if matches!(raw, V::Undefined) {
            return Err(ty(
                "Cannot read properties of undefined (reading 'entries')",
            ));
        }
        let id = number(&raw)? as u64;
        let mut out = vec![];
        for (key, p) in &self.inst(id)?.margins {
            if !truth(key) {
                continue;
            }
            let p = &self.objects[*p];
            let mut fields = vec![("address", key.clone())];
            for k in [
                "contracts",
                "margin",
                "unrealizedPNL",
                "avgPrice",
                "liqPrice",
                "bankruptcyPrice",
                "newPosThisBlock",
            ] {
                fields.push((k, prop(p, k)?))
            }
            out.push(obj(&fields));
        }
        Ok(V::Array(out))
    }
    fn get_position<B: SourceBackend>(
        &mut self,
        address: V,
        series: V,
        f: &mut Frame,
        b: &mut B,
    ) -> Eval<V> {
        let id = self.load_id(series, f, b)?;
        let entries = self.inst(id)?.margins.clone();
        let mut found = entries
            .iter()
            .find(|(key, _)| strict(key, &address))
            .map(|(key, p)| (key.clone(), *p));
        if found.is_none() {
            let V::String(input) = &address else {
                if matches!(address, V::Null | V::Undefined) {
                    return Err(ty(format!(
                        "Cannot read properties of {} (reading 'startsWith')",
                        text(&address)?
                    )));
                }
                return Err(ty("address.startsWith is not a function"));
            };
            if ["ltc1", "tltc1", "bc1", "tb1"]
                .iter()
                .any(|prefix| input.starts_with(prefix))
            {
                for (key, p) in entries {
                    let V::String(key) = key else {
                        return Err(ty("addr.toLowerCase is not a function"));
                    };
                    if key.to_lowercase() == input.to_lowercase() {
                        found = Some((s(&key), p));
                        break;
                    }
                }
            }
        }
        if let Some((key, p)) = found {
            if !truth(&prop(&self.objects[p], "address")?) {
                set(&mut self.objects[p], "address", key)?
            }
            if f.track_return {
                f.returned_position = Some(p);
            }
            return Ok(self.objects[p].clone());
        }
        let position = obj(&[
            ("contracts", n(0.0)),
            ("margin", n(0.0)),
            ("unrealizedPNL", n(0.0)),
        ]);
        if f.track_return {
            f.returned_position = Some(self.alloc(position.clone())?);
        }
        Ok(position)
    }
    fn save<B: SourceBackend>(&mut self, id: u64, block: V, f: &mut Frame, b: &mut B) -> Eval<V> {
        let key = stringify(&obj(&[("seriesId", self.inst(id)?.series.clone())]))?;
        let db = Self::database(f, b, "marginMaps")?;
        let entries = self.instance_value(id)?.get("margins");
        let value = stringify(&entries)?;
        Self::db(
            f,
            b,
            &db,
            "updateAsync",
            vec![
                obj(&[("_id", s(&key))]),
                obj(&[("$set", obj(&[("block", block), ("value", s(&value))]))]),
                obj(&[("upsert", V::Bool(true))]),
            ],
        )?;
        Ok(V::Undefined)
    }
    fn delta<B: SourceBackend>(&mut self, args: &[V], f: &mut Frame, b: &mut B) -> Eval<V> {
        let uuid = Self::call(f, b, "uuid.v4", vec![], false)?;
        let db = Self::database(f, b, "marginMapDelta")?;
        let key = format!(
            "{}-{}-{}",
            text(&arg(args, 0))?,
            text(&arg(args, 1))?,
            text(&uuid)?
        );
        let mut contracts = arg(args, 3);
        if matches!(contracts, V::BigNumber(_)) {
            contracts = Bn::from(&contracts)?.value()
        }
        let delta = obj(&[
            ("address", arg(args, 0)),
            ("contract", arg(args, 1)),
            ("totalPosition", arg(args, 2)),
            ("position", contracts),
            ("margin", arg(args, 4)),
            ("uPNL", arg(args, 5)),
            ("avgEntry", arg(args, 6)),
            ("mode", arg(args, 7)),
            ("block", arg(args, 8)),
            ("lastPrice", arg(args, 9)),
        ]);
        let query = obj(&[("_id", s(&key))]);
        let existing = Self::db(f, b, &db, "findOneAsync", vec![query.clone()])?;
        if truth(&existing) {
            Self::db(
                f,
                b,
                &db,
                "updateAsync",
                vec![query, obj(&[("$set", obj(&[("data", delta)]))])],
            )?;
        } else {
            Self::db(
                f,
                b,
                &db,
                "insertAsync",
                vec![obj(&[("_id", s(&key)), ("data", delta)])],
            )?;
        }
        Self::call(f, b, "Contract.setModFlag", vec![V::Bool(true)], false)?;
        Ok(V::Undefined)
    }
}

include!("margin_option_index_source_impl.rs");
