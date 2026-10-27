// Included by the future OptionLifecycleV4 Core. All positions and balances
// remain in the existing Margin arena and shared Tally owner.
struct SourceOptionValidityBackend<'a> {
    core: &'a mut ContractCore,
    receiver: Option<V>,
}
impl<'a> SourceOptionValidityBackend<'a> {
    fn new(core: &'a mut ContractCore) -> Self {
        Self {
            core,
            receiver: None,
        }
    }
}
impl SourceBackend for SourceOptionValidityBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if call.target == "Margin.margins.get" {
            let receiver = self
                .receiver
                .as_ref()
                .ok_or(SourceRuleError::UnknownDependency)?;
            let result = if matches!(receiver, V::Undefined | V::Null) {
                Err(type_error(format!(
                    "Cannot read properties of {} (reading 'margins')",
                    text(receiver).map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
                )))
            } else {
                let id = number(&receiver.get("sourceMarginInstance"))
                    .map_err(|_| SourceRuleError::InvalidInput)?;
                if !id.is_finite()
                    || id < 1.0
                    || id.fract() != 0.0
                    || id > crate::margin_map_source_owner::MAX_INSTANCES as f64
                {
                    return Err(SourceRuleError::InvalidInput);
                }
                self.core
                    .margins
                    .inspect_position(id as u64, &arg(&call.arguments, 0))
                    .map_err(Fault::Local)
            };
            let mut event = obj(&[
                ("kind", s("dependency")),
                ("target", s(&call.target)),
                ("args", V::Array(call.arguments.clone())),
            ]);
            let returned = match result {
                Ok(value) => {
                    set(&mut event, "returned", value.clone())
                        .map_err(|_| SourceRuleError::Capacity)?;
                    DependencyReturn::Immediate(value)
                }
                Err(Fault::Thrown(error)) => {
                    set(
                        &mut event,
                        "thrown",
                        obj(&[("name", s(&error.name)), ("message", s(&error.message))]),
                    )
                    .map_err(|_| SourceRuleError::Capacity)?;
                    DependencyReturn::Throw(error)
                }
                Err(Fault::Local(error)) => return Err(error),
            };
            self.core.emit(event);
            return Ok(returned);
        }
        let returned = self.core.invoke(call)?;
        if call.target == "Margin.getInstance" {
            if let DependencyReturn::Immediate(value) | DependencyReturn::Pending(value) = &returned
            {
                self.receiver = Some(value.clone());
            }
        }
        Ok(returned)
    }
}
fn option_number(value: &V) -> R<f64> {
    let value = number(value)?;
    Ok(if value == 0.0 || value.is_nan() {
        0.0
    } else {
        value
    })
}
fn option_sign(value: f64) -> f64 {
    if value == 0.0 || value.is_nan() {
        value
    } else if value > 0.0 {
        1.0
    } else {
        -1.0
    }
}
fn option_ticker_value(value: &V) -> V {
    let V::String(raw) = value else {
        return V::Null;
    };
    let Some(parsed) = crate::options::parse_ticker(raw) else {
        return V::Null;
    };
    obj(&[
        ("seriesId", s(parsed.series_id)),
        ("expiryBlock", n(parsed.expiry_block)),
        (
            "type",
            s(match parsed.option_type {
                crate::options::OptionType::Call => "Call",
                crate::options::OptionType::Put => "Put",
            }),
        ),
        ("strike", parsed.strike.map_or(V::Null, n)),
        ("raw", value.clone()),
    ])
}
// Logic object methods are non-strict; primitive parameter assignments are
// ignored. MarginMap class methods use strict property assignment instead.
fn option_logic_set(value: &mut V, key: &str, next: V) -> R<()> {
    if matches!(value, V::Object(_)) {
        set(value, key, next)
    } else if matches!(value, V::Undefined | V::Null) {
        Err(type_error(format!(
            "Cannot set properties of {} (setting '{key}')",
            text(value)?
        )))
    } else {
        Ok(())
    }
}
fn option_class_set(value: &mut V, key: &str, next: V) -> R<()> {
    if matches!(value, V::Object(_)) {
        return set(value, key, next);
    }
    if matches!(value, V::Undefined | V::Null) {
        return Err(type_error(format!(
            "Cannot set properties of {} (setting '{key}')",
            text(value)?
        )));
    }
    let kind = match value {
        V::String(_) => "string",
        V::Number(_) => "number",
        V::Bool(_) => "boolean",
        _ => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
    };
    Err(type_error(format!(
        "Cannot create property '{key}' on {kind} '{}'",
        text(value)?
    )))
}
impl SourceContractRuntime {
    fn source_option_raw_logic(&mut self, params: &V) -> Result<SourceOutcome, SourceRuleError> {
        Ok(
            match self.invoke_dependency(&DependencyCall {
                target: "Logic.processOptionTrade".into(),
                arguments: vec![
                    params.get("senderAddress"),
                    params.clone(),
                    params.get("txid"),
                ],
            })? {
                DependencyReturn::Immediate(_) | DependencyReturn::Pending(_) => {
                    SourceOutcome::Returned(V::Undefined)
                }
                DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error) => {
                    SourceOutcome::Thrown(error)
                }
            },
        )
    }
}
impl ContractCore {
    fn source_option_validate(&mut self, a: &[V]) -> R<V> {
        let evaluated = {
            let mut backend = SourceOptionValidityBackend::new(self);
            crate::validity_source::evaluate_unverified(
                &SourceCall {
                    method: "validateOptionTrade".into(),
                    arguments: a.to_vec(),
                },
                &mut backend,
            )?
        };
        for (key, value) in evaluated.global_writes {
            self.globals.insert(&key, value)?;
        }
        for error in evaluated.deferred_rejections {
            self.source_supply_rejection(error)?;
        }
        outcome(evaluated.outcome)
    }
    fn source_option_logic(&mut self, a: &[V]) -> R<V> {
        let sender = arg(a, 0);
        let mut params = arg(a, 1);
        let txid = arg(a, 2);
        let contract = funding_prop(&params, "contractId")?;
        let contract = if truth(&contract) {
            contract
        } else {
            funding_prop(&params, "ticker")?
        };
        option_logic_set(&mut params, "contractId", contract)?;
        if matches!(
            funding_prop(&params, "blockHeight")?,
            V::Null | V::Undefined
        ) {
            let block = funding_prop(&params, "block")?;
            option_logic_set(&mut params, "blockHeight", block)?;
        }
        let res = self.source_option_validate(&[sender.clone(), params, txid.clone()])?;
        if !truth(&funding_prop(&res, "valid")?) {
            return Ok(res);
        }
        let params = &res;
        let meta = option_ticker_value(&params.get("contractId"));
        let block = option_number(&funding_or(params.get("blockHeight")))?;
        let series = number(&meta.get("seriesId"))?;
        if !series.is_finite() {
            return Ok(obj(&[
                ("valid", V::Bool(false)),
                ("reason", s("Invalid option series id; ")),
            ]));
        }
        let info = self.call("Contract.getContractInfo", vec![n(series)])?;
        let collateral = funding_prop(&info, "collateralPropertyId")?;
        let commits = self.call("Channels.getCommitAddresses", vec![sender.clone()])?;
        let flag = params.get("columnAIsSeller");
        let a_seller =
            flag == V::Bool(true) || same(&flag, &n(1.0)) || flag == s("1") || flag == s("true");
        let seller = funding_prop(
            &commits,
            if a_seller {
                "commitAddressA"
            } else {
                "commitAddressB"
            },
        )?;
        let buyer = funding_prop(
            &commits,
            if a_seller {
                "commitAddressB"
            } else {
                "commitAddressA"
            },
        )?;
        let premium = option_number(&funding_or(params.get("netPremium")))?;
        if premium != 0.0 {
            self.call(
                "Tally.updateBalance",
                vec![
                    buyer.clone(),
                    collateral.clone(),
                    n(-premium),
                    n(0.),
                    n(0.),
                    n(0.),
                    s("optionPremiumPay"),
                    n(block),
                    txid.clone(),
                ],
            )?;
            self.call(
                "Tally.updateBalance",
                vec![
                    seller.clone(),
                    collateral.clone(),
                    n(premium),
                    n(0.),
                    n(0.),
                    n(0.),
                    s("optionPremiumReceive"),
                    n(block),
                    txid.clone(),
                ],
            )?;
        }
        let credit = option_number(&funding_or(params.get("creditMargin")))?;
        let realized = option_number(&funding_or(params.get("rpnlSeller")))?;
        if truth(&params.get("sellerReducing")) || credit != 0.0 {
            self.call(
                "Tally.updateBalance",
                vec![
                    seller.clone(),
                    collateral.clone(),
                    n(realized - credit),
                    n(0.),
                    n(credit),
                    n(0.),
                    s("optionSellerMarginTransition"),
                    n(block),
                    txid.clone(),
                ],
            )?;
        }
        if truth(&params.get("buyerReducing")) {
            let realized = option_number(&funding_or(params.get("rpnlBuyer")))?;
            self.call(
                "Tally.updateBalance",
                vec![
                    buyer.clone(),
                    collateral,
                    n(realized),
                    n(0.),
                    n(0.),
                    n(0.),
                    s("optionReduceBuyer"),
                    n(block),
                    txid.clone(),
                ],
            )?;
        }
        let handle = self.call("Margin.getInstance", vec![n(series)])?;
        let amount = option_number(&funding_or(params.get("amount")))?.abs();
        self.call(
            "Margin.applyOptionTrade",
            vec![
                handle.clone(),
                seller.clone(),
                params.get("contractId"),
                n(-amount),
                params.get("price"),
                n(block),
                n(credit),
            ],
        )?;
        self.call(
            "Margin.applyOptionTrade",
            vec![
                handle.clone(),
                buyer.clone(),
                params.get("contractId"),
                n(amount),
                params.get("price"),
                n(block),
                n(0.),
            ],
        )?;
        if truth(&params.get("comboTicker")) && truth(&params.get("comboAmount")) {
            let combo = option_ticker_value(&params.get("comboTicker"));
            if truth(&combo) && truth(&combo.get("type")) {
                let amount = option_number(&funding_or(params.get("comboAmount")))?.abs();
                let price = funding_or(params.get("comboPrice"));
                self.call(
                    "Margin.applyOptionTrade",
                    vec![
                        handle.clone(),
                        seller,
                        params.get("comboTicker"),
                        n(amount),
                        price.clone(),
                        n(block),
                        n(0.),
                    ],
                )?;
                self.call(
                    "Margin.applyOptionTrade",
                    vec![
                        handle.clone(),
                        buyer,
                        params.get("comboTicker"),
                        n(-amount),
                        price,
                        n(block),
                        n(0.),
                    ],
                )?;
            } else {
                return Err(Fault::Thrown(crate::validity_source::SourceException {
                    name: "ReferenceError".into(),
                    message: "tradeContractChannel is not defined".into(),
                }));
            }
        }
        self.call("Margin.saveMarginMap", vec![handle, n(block)])?;
        // The genuine TradeHistory class has no static recordTrade method.
        Ok(res)
    }
    fn source_option_margin_apply(&mut self, a: &[V]) -> R<V> {
        let handle = arg(a, 0);
        if matches!(handle, V::Undefined | V::Null) {
            return Err(type_error(format!(
                "Cannot read properties of {} (reading 'applyOptionTrade')",
                text(&handle)?
            )));
        }
        let id = number(&handle.get("sourceMarginInstance"))?;
        if !id.is_finite()
            || id < 1.0
            || id.fract() != 0.0
            || id > crate::margin_map_source_owner::MAX_INSTANCES as f64
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let id = id as u64;
        let address = arg(a, 1);
        let ticker = arg(a, 2);
        let key = text(&ticker)?;
        if ["__proto__", "constructor", "prototype"].contains(&key.as_str()) {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        let cached = self
            .margins
            .position_object_entries(id)?
            .into_iter()
            .find(|(key, _)| oracle_source_same_key(key, &address));
        let prior = if let Some((_, object)) = cached.as_ref() {
            self.margins.inspect_position_object(*object)?
        } else {
            V::Undefined
        };
        let existing = if truth(&prior) {
            cached.map(|(_, object)| object)
        } else {
            None
        };
        let mut position = if existing.is_some() {
            prior
        } else {
            let series = self.margins.instance_value(id)?.get("seriesId");
            obj(&[
                ("address", address.clone()),
                ("contractId", if truth(&series) { series } else { V::Null }),
                ("contracts", n(0.)),
                ("avgPrice", n(0.)),
                ("unrealizedPNL", n(0.)),
                ("margin", n(0.)),
                ("options", obj(&[])),
            ])
        };
        let mut bag = funding_prop(&position, "options")?;
        if !truth(&bag) {
            bag = obj(&[]);
            option_class_set(&mut position, "options", bag.clone())?;
            if let Some(object) = existing {
                self.margins
                    .replace_position_object(object, position.clone())?;
            }
        }
        let previous = funding_prop(&bag, &key)?;
        let attached = truth(&previous);
        let mut option = if attached {
            previous
        } else {
            obj(&[("contracts", n(0.)), ("avgPrice", n(0.)), ("margin", n(0.))])
        };
        let delta = option_number(&arg(a, 3))?;
        let price = option_number(&arg(a, 4))?;
        let before = option_number(&funding_prop(&option, "contracts")?)?;
        let after = before + delta;
        let meta = option_ticker_value(&ticker);
        // Exact wrong leading argument: tickerMeta receives fullTicker, not meta.
        self.source_option_touch(&[
            handle.clone(),
            handle.clone(),
            n(before),
            n(after),
            ticker.clone(),
            meta,
        ])?;
        let flip = before != 0.0
            && delta != 0.0
            && option_sign(before) != option_sign(delta)
            && delta.abs() > before.abs();
        let average = if after == 0.0 {
            0.0
        } else if !flip && (before == 0.0 || option_sign(before) == option_sign(after)) {
            (before * number(&funding_prop(&option, "avgPrice")?)? + delta * price) / after
        } else {
            price
        };
        option_class_set(&mut option, "avgPrice", n(average))?;
        self.source_option_store_attached(
            existing,
            &mut position,
            &mut bag,
            &key,
            &option,
            attached,
        )?;
        option_class_set(&mut option, "contracts", n(after))?;
        self.source_option_store_attached(
            existing,
            &mut position,
            &mut bag,
            &key,
            &option,
            attached,
        )?;
        let margin = option_number(&funding_or(funding_prop(&option, "margin")?))?
            + option_number(&funding_or(arg(a, 6)))?;
        option_class_set(&mut option, "margin", n(margin))?;
        option_class_set(&mut bag, &key, option.clone())?;
        option_class_set(&mut position, "options", bag)?;
        if let Some(object) = existing {
            self.margins.replace_position_object(object, position)?;
        } else {
            self.margins
                .set_cached_position(id, address.clone(), position)?;
        }
        self.call(
            "Margin.recordMarginMapDelta",
            vec![
                handle,
                address,
                ticker,
                n(after),
                n(delta),
                n(price),
                n(0.),
                n(option_number(&funding_or(arg(a, 6)))?),
                s("optionTrade"),
                arg(a, 5),
            ],
        )?;
        let closed = if before != 0.0 && option_sign(before) != option_sign(delta) {
            before.abs().min(delta.abs())
        } else {
            0.0
        };
        let flipped = if before != 0.0
            && option_sign(before) != option_sign(delta)
            && delta.abs() > before.abs()
        {
            delta.abs() - before.abs()
        } else {
            0.0
        };
        Ok(obj(&[
            ("contracts", option.get("contracts")),
            ("avgPrice", option.get("avgPrice")),
            ("margin", option.get("margin")),
            ("closedQty", n(closed)),
            ("flipQty", n(flipped)),
        ]))
    }
    fn source_option_store_attached(
        &mut self,
        object: Option<usize>,
        position: &mut V,
        bag: &mut V,
        key: &str,
        option: &V,
        attached: bool,
    ) -> R<()> {
        if attached {
            option_class_set(bag, key, option.clone())?;
            option_class_set(position, "options", bag.clone())?;
            if let Some(object) = object {
                self.margins
                    .replace_position_object(object, position.clone())?;
            }
        }
        Ok(())
    }
    fn source_option_touch(&mut self, a: &[V]) -> R<V> {
        let meta = arg(a, 4);
        if !truth(&meta) || !truth(&funding_prop(&meta, "expiryBlock")?) {
            return Ok(V::Undefined);
        }
        let handle = arg(a, 0);
        let raw = handle.get("sourceMarginInstance");
        let id = number(&raw)?;
        if !matches!(raw, V::Number(_))
            || !id.is_finite()
            || id < 1.0
            || id.fract() != 0.0
            || id > crate::margin_map_source_owner::MAX_INSTANCES as f64
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let mut owner = std::mem::take(&mut self.margins);
        let result = owner.evaluate_instance(
            id as u64,
            &SourceCall {
                method: "_touchExpiryIndex".into(),
                arguments: a[1..].to_vec(),
            },
            self,
        );
        self.margins = owner;
        outcome(result?)
    }
}
