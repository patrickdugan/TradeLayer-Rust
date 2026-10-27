// Included in source_contract_runtime.rs. Literal Tally and collateral movement.
impl ContractCore {
    fn tally_parsed_entries(row: &V) -> R<Vec<(V, V)>> {
        let mut entries = vec![];
        if truth(&row.get("data")) {
            let parsed = parse(&text(&row.get("data"))?)?;
            for pair in array(&parsed)? {
                let pair = match pair {
                    V::Array(a) => a,
                    V::String(t) => t.chars().map(|c| s(c.to_string())).collect(),
                    _ => return Err(type_error("object is not iterable")),
                };
                let key = arg(&pair, 0);
                let value = arg(&pair, 1);
                if let Some(i) = entries
                    .iter()
                    .position(|(k, _)| oracle_source_same_key(k, &key))
                {
                    entries[i] = (key, value)
                } else {
                    entries.push((key, value))
                }
            }
        };
        Ok(entries)
    }
    fn tally_load_rows(&mut self) -> R<V> {
        let result = (|| {
            self.database("tallyMap")?;
            let row = self.find("tallyMap", obj(&[("_id", s("tallyMap"))]))?;
            let entries = Self::tally_parsed_entries(&row)?;
            self.tally = entries;
            if self.global_clearing_enabled() {
                self.tally_static_initialized = true;
            }
            Ok(V::Array(
                self.tally
                    .iter()
                    .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                    .collect(),
            ))
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Undefined),
            other => other,
        }
    }
    fn tally_get(&mut self, address: V, property: V) -> R<V> {
        self.tally_load_rows()?;
        if self.global_clearing_enabled() && !self.tally_static_initialized {
            return Err(type_error(
                "Cannot read properties of undefined (reading 'has')",
            ));
        }
        let Some((_, properties)) = self.tally.iter().find(|(key, _)| same(key, &address)) else {
            return Ok(n(0.0));
        };
        let b = properties.get(&text(&property)?);
        if !truth(&b) {
            return Ok(n(0.0));
        };
        Ok(obj(&[
            ("amount", b.get("amount")),
            ("available", b.get("available")),
            ("reserved", b.get("reserved")),
            ("margin", b.get("margin")),
            ("vesting", b.get("vesting")),
            ("channel", b.get("channelBalance")),
        ]))
    }
    fn tally_save(&mut self, block: V) -> R<()> {
        let pairs = V::Array(
            self.tally
                .iter()
                .map(|(key, value)| V::Array(vec![key.clone(), value.clone()]))
                .collect(),
        );
        let result = (|| {
            self.database("tallyMap")?;
            if self.global_clearing_enabled() && !self.tally_static_initialized {
                return Err(type_error("this.addresses is not iterable"));
            }
            self.update(
                "tallyMap",
                obj(&[("_id", s("tallyMap"))]),
                obj(&[("$set", obj(&[("block", block), ("data", s(json(&pairs)?))]))]),
                true,
            )?;
            Ok(())
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(()),
            other => other,
        }
    }
    fn tally_delta(&mut self, a: &[V], total: V, changes: &[V]) -> R<()> {
        let address = arg(a, 0);
        let property = arg(a, 1);
        let block = arg(a, 7);
        let id = self.new_uuid();
        self.database("tallyMapDelta")?;
        let key = format!(
            "{}-{}-{id}-{}",
            text(&address)?,
            text(&property)?,
            text(&block)?
        );
        // Source getTally is intentionally un-awaited. Its Promise properties
        // produce NaN total, and its load resumes after delta find starts.
        self.database("tallyMap")?;
        let existing = self.find("tallyMapDelta", obj(&[("_id", s(&key))]))?;
        let persisted = self.find("tallyMap", obj(&[("_id", s("tallyMap"))]))?;
        match Self::tally_parsed_entries(&persisted) {
            Ok(entries) => {
                self.tally = entries;
                if self.global_clearing_enabled() {
                    self.tally_static_initialized = true;
                }
            }
            Err(Fault::Thrown(_)) => (),
            Err(e) => return Err(e),
        }
        let _ = total;
        let tx = if truth(&arg(a, 8)) { arg(a, 8) } else { s("") };
        let data = obj(&[
            ("address", address),
            ("block", block),
            ("property", property),
            ("total", n(f64::NAN)),
            ("avail", changes[0].clone()),
            ("res", changes[1].clone()),
            ("mar", changes[2].clone()),
            ("vest", changes[3].clone()),
            ("channel", n(0.0)),
            ("type", arg(a, 6)),
            ("tx", tx),
        ]);
        if truth(&existing) {
            self.db(
                "tallyMapDelta",
                "updateAsync",
                vec![
                    obj(&[("_id", s(&key))]),
                    obj(&[("$set", obj(&[("data", data)]))]),
                ],
            )?;
        } else {
            self.db(
                "tallyMapDelta",
                "insertAsync",
                vec![obj(&[("_id", s(&key)), ("data", data)])],
            )?;
        };
        if self.global_clearing_enabled() {
            self.tally_mod = V::Bool(true);
        }
        Ok(())
    }
    fn tally_update(&mut self, a: &[V]) -> R<V> {
        let address = arg(a, 0);
        let property = arg(a, 1);
        let mut changes = vec![];
        for i in 2..6 {
            let x = arg(a, i);
            if matches!(x, V::Undefined | V::Null) || number(&x)?.is_nan() {
                return Err(error(format!("Somehow null passed into updateBalance... avail. {} reserved {} margin{} vesting {}",text(&arg(a,2))?,text(&arg(a,3))?,text(&arg(a,4))?,text(&arg(a,5))?)));
            };
            changes.push(if matches!(x, V::Number(_)) {
                x
            } else {
                dn(&dec(&x)?)?
            })
        }
        let synthetic = matches!(&property,V::String(x)if x.starts_with("s-"));
        let p = number(&property)?;
        if !synthetic && (!matches!(property, V::Number(_)) || !p.is_finite() || p.fract() != 0.0) {
            return Ok(obj(&[
                ("name", s("Error")),
                (
                    "message",
                    s(format!("Invalid propertyId: {}", text(&property)?)),
                ),
            ]));
        }
        self.tally_load_rows()?;
        if self.global_clearing_enabled() && !self.tally_static_initialized {
            return Err(type_error(
                "Cannot read properties of undefined (reading 'has')",
            ));
        }
        let index = if let Some(i) = self.tally.iter().position(|(key, _)| same(key, &address)) {
            i
        } else {
            if self.tally.len() >= MAX_TABLE_ROWS {
                return Err(SourceRuleError::Capacity.into());
            };
            self.tally.push((address.clone(), obj(&[])));
            self.tally.len() - 1
        };
        let mut properties = self.tally[index].1.clone();
        let key = text(&property)?;
        let mut balance = properties.get(&key);
        if !truth(&balance) {
            balance = obj(&[
                ("amount", n(0.0)),
                ("available", n(0.0)),
                ("reserved", n(0.0)),
                ("margin", n(0.0)),
                ("vesting", n(0.0)),
            ]);
            set(&mut properties, &key, balance.clone())?;
            self.tally[index].1 = properties.clone()
        }
        for (i, name) in ["available", "reserved", "margin", "vesting"]
            .iter()
            .enumerate()
        {
            let old = dec(&balance.get(name))?;
            let next = add(&old, &dec(&changes[i])?)?;
            if next.is_negative() {
                let label = match i {
                    0 => "Available",
                    1 => "Reserved",
                    2 => "Margin",
                    _ => "Vesting",
                };
                return Err(error(format!(
                    "{label} balance cannot go negative {old} change {}",
                    text(&changes[i])?
                )));
            };
            set(&mut balance, name, dn(&next)?)?;
            set(&mut properties, &key, balance.clone())?;
            self.tally[index].1 = properties.clone();
        }
        let mut total = crate::decimal::Decimal::zero();
        for k in ["available", "reserved", "margin", "channelBalance"] {
            total = add(&total, &q(&d0(&balance.get(k))?, 8)?)?
        }
        set(&mut balance, "amount", dn(&total)?)?;
        if matches!(balance.get("channelBalance"), V::Undefined) {
            set(&mut balance, "channelBalance", n(0.0))?
        };
        set(&mut properties, &key, balance.clone())?;
        self.tally[index].1 = properties.clone();
        if changes.iter().any(|v| number(v).unwrap_or(f64::NAN) != 0.0) {
            self.tally_delta(a, dn(&total)?, &changes)?
        }
        if let Some(i) = self.tally.iter().position(|(key, _)| same(key, &address)) {
            self.tally[i].1 = properties
        } else {
            self.tally.push((address, properties))
        };
        self.tally_save(arg(a, 7))?;
        Ok(V::Undefined)
    }
    fn tally_sufficient(&mut self, bucket: &str, a: &[V]) -> R<V> {
        let result = (|| {
            let tally = self.tally_get(arg(a, 0), arg(a, 1))?;
            let have = tally.get(bucket);
            let required = arg(a, 2);
            if !truth(&tally) || matches!(have, V::Undefined) {
                return Ok(obj(&[
                    ("hasSufficient", V::Bool(false)),
                    ("reason", s("undefined")),
                    ("shortfall", required),
                ]));
            };
            if crate::oracle_source_owner::source_greater_unverified(&required, &have)? {
                let diff = sub(&dec(&required)?, &dec(&have)?)?;
                let diff = if bucket == "available" {
                    q(&diff, 8)?
                } else {
                    diff
                };
                let mut fields = vec![
                    ("hasSufficient", V::Bool(false)),
                    ("reason", s("Insufficient available balance")),
                    ("shortfall", dn(&diff)?),
                ];
                if bucket == "available" {
                    fields.push(("available", have))
                };
                return Ok(obj(&fields));
            };
            Ok(obj(&[("hasSufficient", V::Bool(true)), ("reason", s(""))]))
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(obj(&[
                ("hasSufficient", V::Bool(false)),
                ("reason", s("Unexpected error checking balance")),
            ])),
            other => other,
        }
    }
    fn tally_call(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "recordTallyMapDelta" if self.global_clearing_enabled() => {
                self.source_clearing_record_tally_delta(a)
            }
            "getTotalForProperty" | "getTotalTally" | "loadFeeCacheForProperty"
                if self.global_clearing_enabled() =>
            {
                self.source_clearing_tally_query(method, a)
            }
            "setModFlag" if self.global_clearing_enabled() => {
                self.tally_mod = arg(a, 0);
                Ok(V::Undefined)
            }
            "getTally" => self.tally_get(arg(a, 0), arg(a, 1)),
            "updateBalance" => self.tally_update(a),
            "hasSufficientBalance" => self.tally_sufficient("available", a),
            "hasSufficientReserve" => self.tally_sufficient("reserved", a),
            "hasSufficientMargin" => self.tally_sufficient("margin", a),
            "hasSufficientChannel" => self.tally_sufficient("channel", a),
            "updateFeeCache" => self.source_update_fee_cache(a),
            "queueFeeAccrual" => self.source_queue_fee(a),
            "loadFromDB" => self.tally_load_rows(),
            "saveToDB" => {
                self.tally_save(arg(a, 0))?;
                Ok(V::Undefined)
            }
            "flushQueuedFeeAccruals" => self.source_flush_fees(a),
            "flushAllQueuedFeeAccruals" => self.source_flush_all_fees(),
            "accrueFee" => self.source_accrue_fee(a),
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn contract_info(&mut self, id: V) -> R<V> {
        let t = text(&id)?;
        let trimmed = t.trim_start();
        let offset = usize::from(trimmed.starts_with(['-', '+']));
        let end = offset
            + trimmed[offset..]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
        let prefix = &trimmed[..end];
        let id = n(prefix.parse::<f64>().unwrap_or(f64::NAN));
        self.database("contractList")?;
        let row = self.find(
            "contractList",
            obj(&[("id", id), ("type", s("contractSeries"))]),
        )?;
        Ok(if truth(&row) {
            row.get("data")
        } else {
            V::Null
        })
    }
    fn initial_margin(&mut self, id: V, price: V) -> R<V> {
        let info = self.call("Contract.getContractInfo", vec![id.clone()])?;
        if !truth(&info) {
            return Err(error(format!(
                "Contract info not found for contract ID: {}",
                text(&id)?
            )));
        };
        let notional = dec(&info.get("notionalValue"))?;
        let leverage = if truth(&info.get("leverage")) {
            dec(&info.get("leverage"))?
        } else {
            crate::decimal::Decimal::from(10i128)
        };
        let price = dec(&price)?;
        let margin = if truth(&info.get("inverse")) {
            let m = div(&div(&notional, &price)?, &leverage)?;
            let down = m
                .quantize(8, crate::decimal::Rounding::Down)
                .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
            if !m.is_negative() && down != m {
                add(&down, &"0.00000001".parse().unwrap())?
            } else {
                down
            }
        } else {
            q(&div(&mul(&notional, &price)?, &leverage)?, 8)?
        };
        dn(&margin)
    }
    fn contract_call(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "getAllPerpContracts" if self.global_clearing_enabled() => {
                self.source_funding_perp_contracts()
            }
            "loadContractSeries" if self.global_clearing_enabled() => {
                self.source_clearing_load_contracts()
            }
            "getContractInfo" => self.contract_info(arg(a, 0)),
            "getAllContracts" => {
                self.database("contractList")?;
                let rows = array(&self.db(
                    "contractList",
                    "findAsync",
                    vec![obj(&[("type", s("contractSeries"))])],
                )?)?;
                Ok(V::Array(rows.iter().map(|v| v.get("data")).collect()))
            }
            "setModFlag" => {
                self.contract_mod = truth(&arg(a, 0));
                if self.global_clearing_enabled() {
                    self.contract_mod_raw = arg(a, 0);
                }
                Ok(V::Undefined)
            }
            "isInverse" | "isNativeContract" | "isOracleContract" | "getOracleId"
            | "getCollateralId" => {
                let info = self.call("Contract.getContractInfo", vec![arg(a, 0)])?;
                Ok(match method {
                    "isInverse" => {
                        if matches!(info.get("inverse"), V::Undefined) {
                            V::Bool(false)
                        } else {
                            info.get("inverse")
                        }
                    }
                    "isNativeContract" => {
                        if truth(&info) {
                            info.get("native")
                        } else {
                            V::Bool(false)
                        }
                    }
                    "isOracleContract" => {
                        if truth(&info) {
                            V::Bool(info.get("native") == V::Bool(false))
                        } else {
                            info.clone()
                        }
                    }
                    "getOracleId" => {
                        if matches!(info, V::Null | V::Undefined) {
                            return Err(type_error(
                                "Cannot read properties of null (reading 'underlyingOracleId')",
                            ));
                        };
                        info.get("underlyingOracleId")
                    }
                    _ => {
                        if !truth(&info) {
                            return Err(type_error(
                                "Cannot read properties of null (reading 'collateralPropertyId')",
                            ));
                        };
                        info.get("collateralPropertyId")
                    }
                })
            }
            "getInitialMargin" => self.initial_margin(arg(a, 0), arg(a, 1)),
            "getNotionalValue" => {
                let info = self.call("Contract.getContractInfo", vec![arg(a, 0)])?;
                if !truth(&info) {
                    return Ok(obj(&[
                        ("notionalValue", n(0.0)),
                        ("notionalPerContract", n(0.0)),
                    ]));
                };
                let notional = d0(&info.get("notionalValue"))?;
                let mut mark = d0(&arg(a, 1))?;
                if mark <= crate::decimal::Decimal::zero()
                    && !matches!(info.get("underlyingOracleId"), V::Undefined | V::Null)
                {
                    mark = dec(&self.oracle_price(info.get("underlyingOracleId"))?)?
                };
                let value = if truth(&info.get("inverse")) {
                    if mark <= crate::decimal::Decimal::zero() {
                        crate::decimal::Decimal::zero()
                    } else {
                        q(
                            &mul(
                                &div(&crate::decimal::Decimal::from(1i128), &mark)?,
                                &notional,
                            )?,
                            8,
                        )?
                    }
                } else if !truth(&info.get("native")) && mark > crate::decimal::Decimal::zero() {
                    q(&mul(&notional, &mark)?, 8)?
                } else {
                    notional
                };
                Ok(obj(&[
                    ("notionalValue", dn(&value)?),
                    ("notionalPerContract", info.get("notionalValue")),
                ]))
            }
            "moveCollateralToReserve" => {
                let im = self.call("Contract.getInitialMargin", vec![arg(a, 1), arg(a, 3)])?;
                let property = self.call("Contract.getCollateralId", vec![arg(a, 1)])?;
                let amount = dec(&arg(a, 2))?.abs();
                let total = dn(&q(&mul(&dec(&im)?, &amount)?, 8)?)?;
                let has = self.call(
                    "Tally.hasSufficientBalance",
                    vec![arg(a, 0), property.clone(), total.clone()],
                )?;
                if !truth(&has.get("hasSufficient")) {
                    return Ok(V::Null);
                };
                self.call(
                    "Tally.updateBalance",
                    vec![
                        arg(a, 0),
                        property,
                        n(-number(&total)?),
                        total.clone(),
                        n(0.0),
                        n(0.0),
                        s(if truth(&arg(a, 6)) {
                            "contractReserveFromMatchProcess"
                        } else {
                            "contractReserveInitMargin"
                        }),
                        arg(a, 4),
                        arg(a, 5),
                    ],
                )?;
                Ok(total)
            }
            "moveCollateralToMargin" => self.collateral_to_margin(a),
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn collateral_to_margin(&mut self, a: &[V]) -> R<V> {
        let owner = arg(a, 0);
        let contract = arg(a, 1);
        let handle = self.call("Margin.getInstance", vec![contract.clone()])?;
        self.call(
            "Contract.getInitialMargin",
            vec![contract.clone(), arg(a, 3)],
        )?;
        let compare = self.call(
            "Contract.getInitialMargin",
            vec![contract.clone(), arg(a, 4)],
        )?;
        let property = self.call("Contract.getCollateralId", vec![contract.clone()])?;
        let mut total = q(&d0(&arg(a, 6))?, 8)?;
        let compared = q(&mul(&dec(&compare)?, &dec(&arg(a, 2))?)?, 8)?;
        if arg(a, 7) == V::Bool(false) && arg(a, 11) == V::Bool(false) {
            let diff = q(&sub(&compared, &total)?, 8)?;
            if diff > crate::decimal::Decimal::zero() {
                self.call(
                    "Tally.updateBalance",
                    vec![
                        owner.clone(),
                        property.clone(),
                        dn(&diff)?,
                        dn(&diff.negated())?,
                        n(0.0),
                        n(0.0),
                        s("returnExcessMargin"),
                        arg(a, 9),
                    ],
                )?;
            } else if diff.is_negative() {
                let top = diff.abs();
                let has = self.call(
                    "Tally.hasSufficientBalance",
                    vec![owner.clone(), property.clone(), dn(&top)?],
                )?;
                if truth(&has.get("hasSufficient")) {
                    self.call(
                        "Tally.updateBalance",
                        vec![
                            owner.clone(),
                            property.clone(),
                            dn(&top.negated())?,
                            dn(&top)?,
                            n(0.0),
                            n(0.0),
                            s("topUpReserveToFillPriceMargin"),
                            arg(a, 9),
                        ],
                    )?;
                }
            }
        }
        let fee = arg(a, 10);
        if truth(&fee.get("buyFeeFromReserve")) && arg(a, 5) == V::Bool(true) {
            total = q(&sub(&total, &dec(&fee.get("buyerFee"))?)?, 8)?
        } else if truth(&fee.get("sellFeeFromReserve")) && arg(a, 5) == V::Bool(false) {
            total = q(&sub(&total, &dec(&fee.get("sellerFee"))?)?, 8)?
        };
        let virtual_amm = matches!(&owner,V::String(s)if s=="amm"||s.starts_with("amm:"));
        if arg(a, 7) == V::Bool(false) && !virtual_amm {
            let tally = self.call("Tally.getTally", vec![owner.clone(), property.clone()])?;
            let available = d0(&tally.get("available"))?;
            let reserve = d0(&tally.get("reserved"))?;
            let margin = d0(&tally.get("margin"))?;
            let debit = min(reserve.clone(), total.clone());
            let remaining = q(&sub(&total, &debit)?, 8)?;
            if remaining <= crate::decimal::Decimal::zero() {
                if debit > crate::decimal::Decimal::zero() {
                    self.call(
                        "Tally.updateBalance",
                        vec![
                            owner.clone(),
                            property.clone(),
                            n(0.0),
                            dn(&debit.negated())?,
                            dn(&debit)?,
                            n(0.0),
                            s("contractTradeInitMargin"),
                            arg(a, 9),
                        ],
                    )?;
                }
            } else if available >= remaining {
                self.call(
                    "Tally.updateBalance",
                    vec![
                        owner.clone(),
                        property.clone(),
                        dn(&remaining.negated())?,
                        dn(&debit.negated())?,
                        dn(&total)?,
                        n(0.0),
                        s("contractTradeInitMargin"),
                        arg(a, 9),
                    ],
                )?;
            } else if margin >= remaining || add(&available, &margin)? >= remaining {
                if debit > crate::decimal::Decimal::zero() {
                    self.call(
                        "Tally.updateBalance",
                        vec![
                            owner.clone(),
                            property.clone(),
                            n(0.0),
                            dn(&debit.negated())?,
                            dn(&debit)?,
                            n(0.0),
                            s("contractTradeInitMargin"),
                            arg(a, 9),
                        ],
                    )?;
                };
                if margin < remaining && available > crate::decimal::Decimal::zero() {
                    self.call(
                        "Tally.updateBalance",
                        vec![
                            owner.clone(),
                            property.clone(),
                            dn(&available.negated())?,
                            n(0.0),
                            dn(&available)?,
                            n(0.0),
                            s("contractTradeInitMargin"),
                            arg(a, 9),
                        ],
                    )?;
                }
            } else {
                return Err(error(format!("Insufficient collateral for contractTradeInitMargin sender={} prop={} need={} avail={} res={} mar={}",text(&owner)?,text(&property)?,total,available,reserve,margin)));
            }
        } else if arg(a, 7) == V::Bool(true) {
            return Err(SourceRuleError::UnknownDependency.into());
        };
        self.call(
            "Margin.setInitialMargin",
            vec![handle, owner, contract, dn(&total)?, arg(a, 9), arg(a, 14)],
        )
    }
}
