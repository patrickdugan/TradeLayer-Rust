// Exact V11 own-class-data profile. Static AMMPool fields are distinct from
// constructor instance fields; neither owns balances apart from the Core Tally.
const SOURCE_AMM_STATIC_FIELDS: &[&str] = &[
    "position",
    "maxPosition",
    "maxQuoteSize",
    "contractType",
    "lpShares",
    "lpAddresses",
    "ammOrders",
    "strategyBlob",
];
fn amm_static_plus(left: &V, right: &V) -> R<V> {
    source_js_plus(&amm_quote_primitive(left)?, &amm_quote_primitive(right)?)
}
fn amm_static_greater(left: &V, right: &V) -> R<bool> {
    let left = amm_quote_primitive(left)?;
    let right = amm_quote_primitive(right)?;
    if let (V::String(left), V::String(right)) = (&left, &right) {
        Ok(left.encode_utf16().cmp(right.encode_utf16()).is_gt())
    } else {
        Ok(amm_quote_number(&left)? > amm_quote_number(&right)?)
    }
}
fn amm_static_entries(value: &V) -> R<Vec<(String, V)>> {
    let mut values = match value {
        V::Undefined | V::Null => {
            return Err(type_error("Cannot convert undefined or null to object"));
        }
        V::Object(fields) => fields.clone(),
        V::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v.clone()))
            .collect(),
        V::String(value) if value.is_ascii() => value
            .bytes()
            .enumerate()
            .map(|(i, v)| (i.to_string(), s(char::from(v).to_string())))
            .collect(),
        V::String(_) | V::BigNumber(_) | V::Promise => {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        _ => vec![],
    };
    // Object.keys/values enumerate canonical array indices before ordinary
    // strings, retaining insertion order for the remaining own data fields.
    let index = |key: &str| {
        key.parse::<u32>()
            .ok()
            .filter(|n| *n != u32::MAX && n.to_string() == key)
    };
    values.sort_by(|(a, _), (b, _)| match (index(a), index(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => core::cmp::Ordering::Less,
        (None, Some(_)) => core::cmp::Ordering::Greater,
        (None, None) => core::cmp::Ordering::Equal,
    });
    Ok(values)
}
impl ContractCore {
    fn source_amm_token_failures(&self) -> V {
        match self.globals.get("sourceAmmTokenErrors") {
            V::Undefined => V::Array(vec![]),
            values => values,
        }
    }
    fn source_amm_record_token_failures(&mut self, pair: &V, orders: f64) -> R<()> {
        let V::Array(mut errors) = self.source_amm_token_failures() else {
            return Err(SourceRuleError::Backend.into());
        };
        let count = orders.floor().max(0.) as usize;
        if errors
            .len()
            .checked_add(count)
            .is_none_or(|n| n > MAX_TABLE_ROWS)
        {
            return Err(SourceRuleError::Capacity.into());
        }
        let message = s(format!(
            "Error placing orders for pair {}: txid is not defined",
            source_channel_string(pair)?
        ));
        errors.extend(core::iter::repeat_n(message, count));
        self.globals
            .insert("sourceAmmTokenErrors", V::Array(errors))?;
        Ok(())
    }

    fn source_amm_static_fields(&self) -> V {
        match self.globals.get("sourceAmmStatics") {
            V::Undefined => obj(&[]),
            fields => fields,
        }
    }
    fn source_amm_static_get(&self, key: &str) -> V {
        self.source_amm_static_fields().get(key)
    }
    fn source_amm_static_set(&mut self, key: &str, value: V) -> R<()> {
        let mut fields = self.source_amm_static_fields();
        set(&mut fields, key, value)?;
        self.globals.insert("sourceAmmStatics", fields)?;
        Ok(())
    }
    fn source_amm_configure_statics(&mut self, a: &[V]) -> R<V> {
        if a.len() != 1 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let V::Object(fields) = &a[0] else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        // Validate the entire data input before applying any class assignment.
        for (key, value) in fields {
            if !SOURCE_AMM_STATIC_FIELDS.contains(&key.as_str()) {
                return Err(SourceRuleError::InvalidInput.into());
            }
            value.validate()?;
        }
        for (key, value) in fields {
            self.source_amm_static_set(key, value.clone())?;
        }
        Ok(V::Undefined)
    }
    fn source_amm_static_capital(&mut self, redeem: bool, a: &[V]) -> R<V> {
        let address = arg(a, 0);
        let capital = arg(a, 2);
        let mut shares = self.source_amm_static_get("lpShares");
        if redeem {
            let key = amm_own_address(&address)?;
            let current = funding_prop(&shares, &key)?;
            if !truth(&current) || amm_static_greater(&capital, &current)? {
                return Err(error("Insufficient LP shares to redeem"));
            }
            self.source_amm_static_set(
                "position",
                n(amm_quote_number(&self.source_amm_static_get("position"))?
                    - amm_quote_number(&capital)?),
            )?;
            let next = n(amm_quote_number(&current)? - amm_quote_number(&capital)?);
            lower_property_write(&mut shares, &key, next.clone())?;
            if same(&next, &n(0.)) {
                let V::Object(fields) = &mut shares else {
                    return Err(SourceRuleError::UnsupportedNumericDomain.into());
                };
                fields.retain(|(name, _)| name != &key);
            }
            self.source_amm_static_set("lpShares", shares)?;
        } else {
            let next = amm_static_plus(&self.source_amm_static_get("position"), &capital)?;
            if amm_static_greater(&next, &self.source_amm_static_get("maxPosition"))? {
                return Err(error("Pool has reached its maximum position"));
            }
            self.source_amm_static_set("position", next)?;
            // Original assignment writes position BEFORE evaluating its
            // computed address key, so a coercion failure retains that prefix.
            let key = amm_own_address(&address)?;
            let current = funding_prop(&shares, &key)?;
            let current = if truth(&current) { current } else { n(0.) };
            lower_property_write(&mut shares, &key, amm_static_plus(&current, &capital)?)?;
            self.source_amm_static_set("lpShares", shares)?;
        }
        let asset = arg(a, 1);
        let asset2 = arg(a, 4);
        let contract = truth(&arg(a, 3));
        // LP template conversions precede the first free TallyMap lookup.
        // The contract branch never evaluates its unused second-asset input.
        let lp1 = s(if contract {
            format!("{}-LP", source_channel_string(&asset)?)
        } else {
            format!(
                "{}-{}-LP",
                source_channel_string(&asset)?,
                source_channel_string(&asset2)?
            )
        });
        let lp2 = if contract {
            V::Undefined
        } else {
            s(format!(
                "{}-{}-LP",
                source_channel_string(&asset2)?,
                source_channel_string(&asset)?
            ))
        };
        if self.globals.get("sourceAmmStaticTally") != V::Bool(true) {
            return Err(amm_reference_error("TallyMap"));
        }
        let block = arg(a, 6);
        let update = |core: &mut Self, id: V, amount: V, label: &str| -> R<()> {
            core.call(
                "Tally.updateBalance",
                vec![
                    address.clone(),
                    id,
                    amount,
                    n(0.),
                    n(0.),
                    n(0.),
                    s(label),
                    block.clone(),
                ],
            )?;
            Ok(())
        };
        if redeem {
            update(self, lp1, n(-amm_quote_number(&capital)?), "LPBurn")?;
            update(self, asset, capital, "AMMRedeem")?;
            if !contract {
                let second = arg(a, 5);
                // Original third-leg coercion occurs AFTER two real awaited
                // Tally calls. Do not precompute later financial legs.
                update(self, lp2, n(-amm_quote_number(&second)?), "LPBurn")?;
                update(self, asset2, second, "AMMRedeem")?;
            }
        } else {
            update(self, asset, n(-amm_quote_number(&capital)?), "AMMPledge")?;
            update(self, lp1, capital, "LPIssue")?;
            if !contract {
                let second = arg(a, 5);
                update(self, asset2, n(-amm_quote_number(&second)?), "AMMPledge")?;
                update(self, lp2, second, "LPIssue")?;
            }
        }
        Ok(V::Undefined)
    }
    fn source_amm_build_strategy(&self, method: &str, a: &[V]) -> R<V> {
        let price = arg(a, 0);
        let block = arg(a, 1);
        let size = self.source_amm_static_get("maxQuoteSize");
        let row = |side: &str, kind: &str, strike: V, distance: f64| -> R<V> {
            Ok(obj(&[
                ("side", s(side)),
                ("type", s(kind)),
                ("strike", strike),
                ("expiry", amm_static_plus(&block, &n(distance))?),
                ("size", size.clone()),
            ]))
        };
        Ok(V::Array(match method {
            "buildStraddle" => vec![
                row("buy", "call", price.clone(), 1.)?,
                row("buy", "put", price, 1.)?,
            ],
            "buildIronFly" => {
                let up = n(amm_quote_number(&price)? * 1.02);
                let down = n(amm_quote_number(&price)? * 0.98);
                vec![
                    row("sell", "call", price.clone(), 1.)?,
                    row("sell", "put", price, 1.)?,
                    row("buy", "call", up, 1.)?,
                    row("buy", "put", down, 1.)?,
                ]
            }
            "buildCalendar" => vec![
                row("sell", "call", price.clone(), 1.)?,
                row("buy", "call", price, 10.)?,
            ],
            _ => return Err(SourceRuleError::UnknownDependency.into()),
        }))
    }
    fn source_amm_generate_contract_static(&mut self, linear: bool, a: &[V]) -> R<V> {
        let distance = if linear { 0.01 } else { 0.001 };
        let size = self.call(
            "AMMStatic.calculateOrderSize",
            vec![n(distance), arg(a, 1), arg(a, 2)],
        )?;
        let quote = amm_quote_min(
            amm_quote_number(&self.source_amm_static_get("position"))?,
            amm_quote_number(&self.source_amm_static_get("maxQuoteSize"))?,
        );
        let amount = n(amm_quote_min(amm_quote_number(&size)?, quote));
        let position = amm_static_plus(&self.source_amm_static_get("position"), &amount)?;
        self.source_amm_static_set("position", position.clone())?;
        Ok(obj(&[
            ("price", amm_static_plus(&arg(a, 0), &n(distance))?),
            ("size", amount),
            (
                "side",
                s(if linear && amm_quote_number(&position)? > 0. {
                    "buy"
                } else {
                    "sell"
                }),
            ),
        ]))
    }
    fn source_amm_remaining_call(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "insertCapital" | "redeemCapital" => {
                self.source_amm_static_capital(method == "redeemCapital", a)
            }
            "requestLiquidity" => {
                let order = arg(a, 0);
                let size = funding_prop(&order, "size")?;
                let next = amm_static_plus(&self.source_amm_static_get("position"), &size)?;
                if amm_static_greater(&next, &self.source_amm_static_get("maxPosition"))? {
                    return Ok(V::Null);
                }
                let sign = if funding_prop(&order, "side")? == s("buy") {
                    1.
                } else {
                    -1.
                };
                self.source_amm_static_set(
                    "position",
                    amm_static_plus(
                        &self.source_amm_static_get("position"),
                        &n(amm_quote_number(&size)? * sign),
                    )?,
                )?;
                let orders = self.source_amm_static_get("ammOrders");
                if matches!(orders, V::Null | V::Undefined) {
                    return Err(type_error(format!(
                        "Cannot read properties of {} (reading 'push')",
                        text(&orders)?
                    )));
                }
                let V::Array(mut orders) = orders else {
                    return Err(type_error("this.ammOrders.push is not a function"));
                };
                if orders.len() >= MAX_TABLE_ROWS {
                    return Err(SourceRuleError::Capacity.into());
                }
                orders.push(order.clone());
                self.source_amm_static_set("ammOrders", V::Array(orders))?;
                Ok(order)
            }
            "runOptionStrategy" => {
                let blob = self.source_amm_static_get("strategyBlob");
                if !truth(&blob) {
                    return Ok(V::Array(vec![]));
                }
                let parsed = match source_channel_string(&blob).and_then(|v| parse(&v)) {
                    Ok(v) => v,
                    Err(_) => return Ok(V::Array(vec![])),
                };
                let method = match funding_prop(&parsed, "type")? {
                    V::String(kind) => match kind.as_str() {
                        "straddle" => "buildStraddle",
                        "ironFly" => "buildIronFly",
                        "calendar" => "buildCalendar",
                        _ => return Ok(V::Array(vec![])),
                    },
                    _ => return Ok(V::Array(vec![])),
                };
                self.call(&format!("AMMStatic.{method}"), vec![arg(a, 0), arg(a, 2)])
            }
            "buildStraddle" | "buildIronFly" | "buildCalendar" => {
                self.source_amm_build_strategy(method, a)
            }
            "calculateTotalPosition" => {
                let address = arg(a, 0);
                let shares = self.source_amm_static_get("lpShares");
                let value = if matches!(address, V::Null | V::Undefined) {
                    let mut total = n(0.);
                    for (_, v) in amm_static_entries(&shares)? {
                        total = amm_static_plus(&total, &v)?;
                    }
                    total
                } else {
                    let current = funding_prop(&shares, &amm_own_address(&address)?)?;
                    if !truth(&current) {
                        return Ok(n(0.));
                    }
                    current
                };
                Ok(n((amm_quote_number(&value)?
                    / amm_quote_number(
                        &self.source_amm_static_get("maxPosition"),
                    )?)
                    * 100.))
            }
            "getLPAddresses" => Ok(V::Array(
                amm_static_entries(&self.source_amm_static_get("lpAddresses"))?
                    .into_iter()
                    .map(|(key, _)| s(key))
                    .collect(),
            )),
            "getAMMOrdersAndPositions" => Ok(obj(&[
                ("orders", self.source_amm_static_get("ammOrders")),
                ("position", self.source_amm_static_get("position")),
                ("maxPosition", self.source_amm_static_get("maxPosition")),
                ("lpAddresses", self.source_amm_static_get("lpAddresses")),
            ])),
            "generateOrdersForInverse" | "generateOrdersForLinear" => {
                self.source_amm_generate_contract_static(method == "generateOrdersForLinear", a)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
