// Included in source_contract_runtime.rs only under the next explicit policy.
// The sole Core Tally/Margin/Property/Vault authorities retain source defects.
fn synthetic_up(value: FundingBn, places: u32) -> R<FundingBn> {
    let FundingBn::Finite(value) = value else {
        return Ok(value);
    };
    let truncated = value
        .quantize(places, FundingRounding::Down)
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    if truncated == value {
        return Ok(FundingBn::Finite(truncated));
    }
    let unit = FundingDecimal::from_str(&format!("1e-{places}"))
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    let result = if value.is_negative() {
        truncated.checked_sub(&unit)
    } else {
        truncated.checked_add(&unit)
    }
    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    Ok(FundingBn::Finite(result))
}
fn synthetic_plus(left: &V, right: &V) -> R<V> {
    // BigNumber.valueOf supplies its string primitive in the source += paths.
    let primitive = |v: &V| -> R<V> {
        Ok(match v {
            V::BigNumber(_) | V::Array(_) => s(text(v)?),
            V::Object(_) => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
            other => other.clone(),
        })
    };
    source_js_plus(&primitive(left)?, &primitive(right)?)
}
fn synthetic_value(map: &[(V, V)], key: &V) -> V {
    oracle_source_map_get(map, key)
        .cloned()
        .unwrap_or(V::Undefined)
}
fn synthetic_quantize(value: &V) -> R<V> {
    Ok(n(FundingBn::new(&funding_or(value.clone()))?
        .q(8, FundingRounding::HalfUp)?
        .number()))
}
fn synthetic_parse_int10(value: &V) -> R<f64> {
    let value = text(value)?;
    let value = value.trim_start_matches(funding_js_space);
    let (negative, digits) = if let Some(v) = value.strip_prefix('-') {
        (true, v)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    let mut accumulated = 0u128;
    let mut count = 0;
    for digit in digits.bytes().take_while(u8::is_ascii_digit) {
        accumulated = accumulated
            .checked_mul(10)
            .and_then(|v| v.checked_add(u128::from(digit - b'0')))
            .ok_or(SourceRuleError::UnsupportedNumericDomain)?;
        count += 1;
    }
    if count == 0 {
        return Ok(f64::NAN);
    }
    Ok(if negative {
        -(accumulated as f64)
    } else {
        accumulated as f64
    })
}
impl SourceContractRuntime {
    fn source_synthetic_raw_logic(
        &mut self,
        kind: &V,
        p: &V,
    ) -> Result<SourceOutcome, SourceRuleError> {
        let mut arguments = vec![
            p.get("senderAddress"),
            p.get("propertyId"),
            p.get("contractId"),
            p.get("amount"),
            p.get("block"),
        ];
        let method = if *kind == n(24.) {
            arguments.extend([p.get("grossRequired"), p.get("contracts"), p.get("margin")]);
            "mintSynthetic"
        } else if *kind == n(25.) {
            "redeemSynthetic"
        } else {
            return Err(SourceRuleError::InvalidInput);
        };
        Ok(
            match self.invoke_dependency(&DependencyCall {
                target: format!("Logic.{method}"),
                arguments,
            })? {
                DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
                    SourceOutcome::Returned(v)
                }
                DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                    SourceOutcome::Thrown(e)
                }
            },
        )
    }
}
impl ContractCore {
    fn source_synthetic_initial_margin(&mut self, id: V, price: V) -> R<V> {
        let info = self.call("Contract.getContractInfo", vec![id.clone()])?;
        self.source_synthetic_initial_margin_value(&id, &price, &info)
    }
    fn source_synthetic_initial_margin_value(&self, id: &V, price: &V, info: &V) -> R<V> {
        if !truth(info) {
            return Err(error(format!(
                "Contract info not found for contract ID: {}",
                text(id)?
            )));
        }
        let notional = FundingBn::new(&funding_prop(info, "notionalValue")?)?;
        let leverage = FundingBn::new(&if truth(&info.get("leverage")) {
            info.get("leverage")
        } else {
            n(10.)
        })?;
        let price = FundingBn::new(price)?;
        let value = if truth(&info.get("inverse")) {
            let value = notional.op(price, 3)?.op(leverage, 3)?;
            // BigNumber ROUND_CEIL rounds negative values toward zero.
            if value.number().is_sign_negative() {
                value.q(8, FundingRounding::Down)?
            } else {
                synthetic_up(value, 8)?
            }
        } else {
            notional
                .op(price, 2)?
                .op(leverage, 3)?
                .q(8, FundingRounding::HalfUp)?
        };
        Ok(n(value.number()))
    }
    fn source_synthetic_oracle_remainder(&mut self, id: V) -> R<()> {
        let rows = self.db(
            "oracleData",
            "findAsync",
            vec![obj(&[("oracleId", id.clone())])],
        )?;
        // Only the actual Core read above produces storage effects. A temporary
        // read projection reuses the complete Oracle owner's filter, Number
        // coercion and latest-height reduction with the original query ID. It
        // is discarded immediately and owns no persistent cache or authority.
        let mut projection = crate::oracle_source_owner::OracleSourceOwner::new();
        projection.import_rows("oracleData", array(&rows)?)?;
        outcome(projection.evaluate(&SourceCall {
            method: "getOraclePrice".into(),
            arguments: vec![id],
        })?)
        .map(|_| ())
    }
    fn source_synthetic_unawaited_oracle_margin(&mut self, contract: &V, oracle: V) -> R<V> {
        let oracle_factory = self.database("oracleData");
        // Contract.getInitialMargin starts before the Oracle await resumes.
        let contract_factory = self.database("contractList");
        let oracle_result = match oracle_factory {
            Ok(_) => self.source_synthetic_oracle_remainder(oracle),
            Err(e) => Err(e),
        };
        match oracle_result {
            Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
            Err(e) => return Err(e),
            Ok(_) => (),
        }
        contract_factory?;
        let row = self.find(
            "contractList",
            obj(&[
                ("id", n(synthetic_parse_int10(contract)?)),
                ("type", s("contractSeries")),
            ]),
        )?;
        let info = if truth(&row) {
            row.get("data")
        } else {
            V::Null
        };
        self.source_synthetic_initial_margin_value(contract, &V::Promise, &info)
    }
    fn source_synthetic_margin_slot(&self, instance: u64, address: &V) -> R<Option<usize>> {
        Ok(self
            .margins
            .position_object_entries(instance)?
            .into_iter()
            .find(|(key, _)| oracle_source_same_key(key, address))
            .map(|(_, object)| object))
    }
    fn source_synthetic_position_write(
        &mut self,
        object: usize,
        value: &mut V,
        key: &str,
        next: V,
    ) -> R<()> {
        set(value, key, next)?;
        self.margins
            .replace_position_object(object, value.clone())?;
        Ok(())
    }
    fn source_synthetic_margin(&mut self, method: &str, args: &[V]) -> R<V> {
        let handle = arg(args, 0);
        let raw = handle.get("sourceMarginInstance");
        let number = number(&raw)?;
        if !matches!(raw, V::Number(_))
            || !number.is_finite()
            || number < 1.
            || number.fract() != 0.
            || number > u64::MAX as f64
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let instance = number as u64;
        let a = &args[1..];
        match method {
            "moveMarginAndContractsForMint" => {
                self.source_synthetic_margin_mint(instance, handle, a)
            }
            "moveMarginAndContractsForRedeem" => {
                self.source_synthetic_margin_redeem(instance, handle, a)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_synthetic_margin_mint(&mut self, instance: u64, handle: V, a: &[V]) -> R<V> {
        let address = arg(a, 0);
        let property = arg(a, 1);
        let contract = arg(a, 2);
        let contracts = arg(a, 3);
        let mut margin = arg(a, 4);
        let synth = s(format!("s-{}-{}", text(&property)?, text(&contract)?));
        let vault_object = match self.source_synthetic_margin_slot(instance, &synth)? {
            Some(object) if truth(&self.margins.inspect_position_object(object)?) => Some(object),
            _ => None,
        };
        let mut vault = if let Some(object) = vault_object {
            self.margins.inspect_position_object(object)?
        } else {
            obj(&[
                ("contracts", n(0.)),
                ("margin", n(0.)),
                ("avgPrice", n(0.)),
                ("liqPrice", V::Null),
                ("address", synth.clone()),
            ])
        };
        let Some(object) = self.source_synthetic_margin_slot(instance, &address)? else {
            return Ok(V::Undefined);
        };
        let mut position = self.margins.inspect_position_object(object)?;
        if !truth(&position) {
            return Ok(V::Undefined);
        }
        let next = n(FundingBn::new(&funding_prop(&position, "contracts")?)?
            .op(FundingBn::new(&contracts)?, 1)?
            .number());
        self.source_synthetic_position_write(object, &mut position, "contracts", next)?;
        let mut excess = n(0.);
        if crate::oracle_source_owner::source_greater_unverified(
            &margin,
            &funding_prop(&position, "margin")?,
        )? {
            if number(&funding_prop(&position, "contracts")?)?.abs() > number(&contracts)?.abs() {
                excess = margin;
                margin = n(0.);
            } else {
                excess = n(FundingBn::new(&margin)?
                    .op(FundingBn::new(&funding_prop(&position, "margin")?)?, 1)?
                    .q(8, FundingRounding::HalfUp)?
                    .number());
                margin = funding_prop(&position, "margin")?;
            }
        }
        let previous = funding_prop(&position, "margin")?;
        let next = n(FundingBn::new(&previous)?
            .op(FundingBn::new(&margin)?, 1)?
            .q(8, FundingRounding::HalfUp)?
            .number());
        self.source_synthetic_position_write(object, &mut position, "margin", next)?;
        let _margin_change = FundingBn::new(&previous)?
            .op(FundingBn::new(&funding_prop(&position, "margin")?)?, 1)?;
        let avg_delta;
        if let Some(vault_object) = vault_object {
            vault = self.margins.inspect_position_object(vault_object)?;
            let next = n(FundingBn::new(&funding_prop(&vault, "contracts")?)?
                .op(FundingBn::new(&contracts)?, 0)?
                .number());
            self.source_synthetic_position_write(vault_object, &mut vault, "contracts", next)?;
            if vault_object == object {
                position = self.margins.inspect_position_object(object)?;
            }
            // Source uses the user's remaining margin here, not the vault's.
            let next = n(FundingBn::new(&funding_prop(&position, "margin")?)?
                .op(FundingBn::new(&margin)?, 0)?
                .q(8, FundingRounding::HalfUp)?
                .number());
            self.source_synthetic_position_write(vault_object, &mut vault, "margin", next)?;
            // Call arguments evaluate before the undefined method is called.
            // `amount` is an undeclared identifier in this source method.
            self.call("SourceGlobal.get", vec![s("amount")])?;
            funding_prop(&position, "avgPrice")?;
            return Err(type_error("this.updatedAvgPrice is not a function"));
        } else {
            set(&mut vault, "contracts", contracts.clone())?;
            set(&mut vault, "margin", margin.clone())?;
            let price = funding_prop(&position, "avgPrice")?;
            set(&mut vault, "avgPrice", price.clone())?;
            avg_delta = price;
        }
        self.margins
            .set_cached_position(instance, synth.clone(), vault.clone())?;
        self.call(
            "Margin.recordMarginMapDelta",
            vec![
                handle.clone(),
                synth,
                contract.clone(),
                funding_prop(&vault, "contracts")?,
                contracts.clone(),
                margin.clone(),
                n(0.),
                avg_delta,
                s("mintMarginAndContractsToVault"),
            ],
        )?;
        self.call(
            "Margin.recordMarginMapDelta",
            vec![
                handle.clone(),
                property,
                contract,
                funding_prop(&position, "contracts")?,
                n(-number(&contracts)?),
                n(-number(&margin)?),
                n(0.),
                n(0.),
                s("moveMarginAndContractsForMint"),
            ],
        )?;
        self.call("Margin.saveMarginMap", vec![handle, arg(a, 5)])?;
        Ok(obj(&[
            ("contracts", contracts),
            ("margin", margin),
            ("excess", excess),
        ]))
    }
    fn source_synthetic_margin_redeem(&mut self, instance: u64, handle: V, a: &[V]) -> R<V> {
        let address = arg(a, 0);
        let property = arg(a, 1);
        let contract = arg(a, 2);
        let amount = arg(a, 3);
        let vault = arg(a, 4);
        let notional = arg(a, 5);
        let initial = arg(a, 6);
        let object = self.source_synthetic_margin_slot(instance, &address)?;
        let mut position = if let Some(object) = object {
            self.margins.inspect_position_object(object)?
        } else {
            V::Undefined
        };
        let vault_position = self.margins.inspect_position(instance, &property)?;
        let contracts = funding_prop(&vault, "contracts")?;
        let margin = funding_prop(&vault, "margin")?;
        let short = FundingBn::new(&amount)?
            .op(FundingBn::new(&notional)?, 3)?
            .number();
        let quantity = funding_prop(&position, "contracts")?;
        let mut long_closed = 0.;
        let mut shorts_added = 0.;
        let mut transfer = false;
        let mut modify = false;
        if number(&quantity)? > 0. && (short < number(&quantity)? || same(&n(short), &quantity)) {
            long_closed = short;
        } else if number(&quantity)? > 0. && short > number(&quantity)? {
            long_closed = number(&quantity)?;
            shorts_added = FundingBn::new(&n(short))?
                .op(FundingBn::new(&n(long_closed))?, 1)?
                .number();
            transfer = true;
        } else if number(&funding_prop(&position, "contract")?)? < 0. {
            modify = true;
            shorts_added = short;
        }
        if shorts_added > 0. {
            funding_prop(&vault_position, "avgPrice")?;
            return Err(type_error("this.updatedAvgPrice is not a function"));
        }
        let object = object.ok_or(SourceRuleError::InvalidInput)?;
        let next = n(FundingBn::new(&quantity)?
            .op(FundingBn::new(&contracts)?, 0)?
            .number());
        self.source_synthetic_position_write(object, &mut position, "contracts", next)?;
        let pro_rata = FundingBn::new(&amount)?
            .op(FundingBn::new(&funding_prop(&vault, "outstanding")?)?, 3)?
            .q(8, FundingRounding::HalfUp)?;
        let margin_return = FundingBn::new(&funding_prop(&vault, "margin")?)?
            .op(pro_rata.clone(), 2)?
            .q(8, FundingRounding::HalfUp)?;
        let available_return = FundingBn::new(&funding_prop(&vault, "available")?)?
            .op(pro_rata, 2)?
            .q(8, FundingRounding::HalfUp)?;
        let return_margin = FundingBn::new(&n(short))?
            .op(FundingBn::new(&initial)?, 2)?
            .q(8, FundingRounding::HalfUp)?;
        let returned_margin = return_margin.number();
        let mut _return_available = margin_return
            .clone()
            .op(return_margin.clone(), 1)?
            .q(8, FundingRounding::HalfUp)?
            .number();
        let not_covered = self.call("SourceGlobal.get", vec![s("notCovered")])?;
        if number(&not_covered)? > 0. {
            _return_available =
                margin_return.number() - (margin_return.number() * (number(&not_covered)? / short));
        }
        let excess = FundingBn::new(&margin)?
            .op(margin_return.clone(), 1)?
            .q(8, FundingRounding::HalfUp)?;
        let next = n(FundingBn::new(&funding_prop(&position, "margin")?)?
            .op(return_margin, 0)?
            .q(8, FundingRounding::HalfUp)?
            .number());
        self.source_synthetic_position_write(object, &mut position, "margin", next)?;
        if !modify && !transfer {
            let next = n(FundingBn::new(&funding_prop(&position, "contracts")?)?
                .op(FundingBn::new(&n(long_closed))?, 1)?
                .number());
            self.source_synthetic_position_write(object, &mut position, "contracts", next)?;
        }
        funding_prop(&position, "avgPrice")?;
        if transfer {
            let price = funding_prop(&vault_position, "avgPrice")?;
            self.source_synthetic_position_write(object, &mut position, "avgPrice", price)?;
            let next = n(FundingBn::new(&funding_prop(&position, "contracts")?)?
                .op(FundingBn::new(&n(long_closed))?, 1)?
                .op(FundingBn::new(&n(shorts_added))?, 1)?
                .number());
            self.source_synthetic_position_write(object, &mut position, "contracts", next)?;
        }
        if modify {
            funding_prop(&vault_position, "avgPrice")?;
            return Err(type_error("this.updatedAvgPrice is not a function"));
        }
        let mut pnl = n(0.);
        let mut reduction = n(0.);
        if long_closed > 0. {
            let mut aliased = handle.clone();
            set(
                &mut aliased,
                "sourcePositionObjectAliases",
                V::Array(vec![obj(&[
                    ("argument", n(6.)),
                    ("object", n(object as f64)),
                ])]),
            )?;
            pnl = self.call(
                "Margin.realizePnl",
                vec![
                    aliased,
                    address.clone(),
                    n(long_closed),
                    funding_prop(&vault_position, "avgPrice")?,
                    funding_prop(&position, "avgPrice")?,
                    V::Bool(true),
                    notional.clone(),
                    position.clone(),
                    V::Bool(false),
                    contract.clone(),
                ],
            )?;
            position = self.margins.inspect_position_object(object)?;
            let mut aliased = handle.clone();
            set(
                &mut aliased,
                "sourcePositionObjectAliases",
                V::Array(vec![obj(&[
                    ("argument", n(0.)),
                    ("object", n(object as f64)),
                ])]),
            )?;
            // Preserve every wrong positional argument from the source call.
            reduction = self.call(
                "Margin.reduceMargin",
                vec![
                    aliased,
                    position.clone(),
                    n(long_closed),
                    pnl.clone(),
                    V::Bool(true),
                    contract.clone(),
                    address.clone(),
                    V::Bool(false),
                    V::Bool(false),
                    n(0.),
                ],
            )?;
            position = self.margins.inspect_position_object(object)?;
        }
        self.call(
            "Margin.recordMarginMapDelta",
            vec![
                handle.clone(),
                property,
                contract.clone(),
                funding_prop(&vault, "contracts")?,
                n(short),
                n(-returned_margin),
                n(-number(&pnl)?),
                n(0.),
                s("redeemMarginAndContractsFromVault"),
            ],
        )?;
        self.call(
            "Margin.recordMarginMapDelta",
            vec![
                handle.clone(),
                address,
                contract,
                funding_prop(&position, "contracts")?,
                n(short),
                n(returned_margin),
                pnl.clone(),
                n(0.),
                s("moveMarginAndContractsForRedeem"),
            ],
        )?;
        self.call("Margin.saveMarginMap", vec![handle, arg(a, 8)])?;
        Ok(obj(&[
            ("contracts", n(short)),
            ("margin", n(margin_return.number())),
            ("available", n(available_return.number())),
            ("excess", n(excess.number())),
            ("rPNL", pnl),
            ("reduction", reduction),
        ]))
    }
    fn source_synthetic_state(&self) -> V {
        let map = |values: &[(V, V)]| {
            V::Array(
                values
                    .iter()
                    .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                    .collect(),
            )
        };
        obj(&[
            (
                "vaults",
                if self.vault_initialized {
                    map(&self.vault_cache)
                } else {
                    V::Undefined
                },
            ),
            (
                "syntheticTokens",
                if self.vault_initialized {
                    map(&self.synthetic_token_cache)
                } else {
                    V::Undefined
                },
            ),
        ])
    }
    fn source_synthetic_initialize(&mut self) -> R<()> {
        if !self.vault_initialized {
            // Source allocates both Maps before awaiting its loader. Failure
            // leaves initialized empty Maps and future calls skip this load.
            self.vault_initialized = true;
            self.vault_cache.clear();
            self.synthetic_token_cache.clear();
            self.source_synthetic_load()?;
        }
        Ok(())
    }
    fn source_synthetic_load(&mut self) -> R<V> {
        self.database("vaults")?;
        let rows = self.find("vaults", obj(&[]))?;
        if let V::Array(rows) = rows {
            if !rows.is_empty() && !self.vault_initialized {
                return Err(type_error(
                    "Cannot read properties of undefined (reading 'set')",
                ));
            }
            for row in rows {
                oracle_source_map_set(
                    &mut self.vault_cache,
                    funding_prop(&row, "_id")?,
                    funding_prop(&row, "data")?,
                )?;
            }
        }
        self.database("syntheticTokens")?;
        // This find is not awaited. Array.isArray(Promise) is false even when
        // its eventual result is an array. Its rejection is independent.
        match self.find("syntheticTokens", obj(&[])) {
            Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
            Err(e) => return Err(e),
            Ok(_) => (),
        }
        Ok(V::Undefined)
    }
    fn source_synthetic_vault(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "initializeIfNeeded" => {
                self.source_synthetic_initialize()?;
                Ok(V::Undefined)
            }
            "loadFromDatabase" => self.source_synthetic_load(),
            "exists" => {
                self.database("syntheticTokens")?;
                // The source compares the un-awaited Promise with null.
                match self.find("syntheticTokens", obj(&[("_id", arg(a, 0))])) {
                    Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
                    Err(e) => return Err(e),
                    Ok(_) => (),
                }
                Ok(V::Bool(true))
            }
            "createVault" => {
                self.source_synthetic_initialize()?;
                let id = s(format!("s-{}-{}", text(&arg(a, 0))?, text(&arg(a, 1))?));
                oracle_source_map_set(
                    &mut self.vault_cache,
                    id.clone(),
                    obj(&[
                        ("propertyId", arg(a, 0)),
                        ("contractId", arg(a, 1)),
                        ("contracts", n(0.)),
                        ("margin", n(0.)),
                        ("available", n(0.)),
                        ("outstanding", n(0.)),
                    ]),
                )?;
                // The actual source omits its second saveVault argument.
                self.source_synthetic_vault("saveVault", core::slice::from_ref(&id))?;
                Ok(id)
            }
            "registerSyntheticToken" => {
                self.source_synthetic_initialize()?;
                oracle_source_map_set(
                    &mut self.synthetic_token_cache,
                    arg(a, 0),
                    obj(&[("contract", arg(a, 1)), ("property", arg(a, 2))]),
                )?;
                self.source_synthetic_vault("saveSyntheticToken", &[arg(a, 0)])
            }
            "getVault" => {
                self.source_synthetic_initialize()?;
                Ok(synthetic_value(&self.vault_cache, &arg(a, 0)))
            }
            "getVaultId" => {
                self.source_synthetic_initialize()?;
                let data = synthetic_value(&self.synthetic_token_cache, &arg(a, 0));
                Ok(if matches!(data, V::Null | V::Undefined) {
                    V::Undefined
                } else {
                    data.get("vaultId")
                })
            }
            "saveVault" => {
                self.source_synthetic_initialize()?;
                self.database("vaults")?;
                let vault = arg(a, 1);
                let value = if vault == V::Undefined {
                    V::Undefined
                } else {
                    s(json(&vault)?)
                };
                self.update(
                    "vaults",
                    obj(&[("_id", arg(a, 0))]),
                    obj(&[("_id", arg(a, 0)), ("value", value)]),
                    true,
                )?;
                Ok(V::Undefined)
            }
            "saveSyntheticToken" => {
                self.source_synthetic_initialize()?;
                self.database("syntheticTokens")?;
                let id = s(text(&arg(a, 0))?);
                let data = synthetic_value(&self.synthetic_token_cache, &arg(a, 0));
                self.update(
                    "syntheticTokens",
                    obj(&[("_id", id.clone())]),
                    obj(&[
                        ("_id", id),
                        (
                            "value",
                            if data == V::Undefined {
                                V::Undefined
                            } else {
                                s(json(&data)?)
                            },
                        ),
                    ]),
                    true,
                )?;
                Ok(V::Undefined)
            }
            "updateVault" | "updateVaultRedeem" => {
                self.source_synthetic_initialize()?;
                let id = arg(a, 0);
                let delta = arg(a, 1);
                let mut vault = synthetic_value(&self.vault_cache, &id);
                if !truth(&vault) {
                    if method == "updateVaultRedeem" {
                        return Ok(V::Undefined);
                    }
                    vault = obj(&[
                        ("contracts", n(0.)),
                        ("margin", n(0.)),
                        ("available", n(0.)),
                        ("outstanding", n(0.)),
                    ]);
                }
                let existing = truth(&synthetic_value(&self.vault_cache, &id));
                for field in ["contracts", "margin", "available", "outstanding"] {
                    let previous = funding_prop(&vault, field)?;
                    let change = match field {
                        "contracts" => funding_prop(&delta, "contracts")?,
                        "margin" => funding_prop(&delta, "margin")?,
                        "available" if method == "updateVault" => arg(a, 3),
                        "available" => funding_prop(&delta, "available")?,
                        _ => arg(a, 2),
                    };
                    let value = if method == "updateVault" {
                        synthetic_plus(&previous, &funding_or(change))?
                    } else if matches!(field, "margin" | "available") {
                        n(number(&previous)? - number(&change)?)
                    } else {
                        synthetic_plus(&previous, &change)?
                    };
                    set(&mut vault, field, value)?;
                    // Mutations on an existing Map value are visible before
                    // the next property access or save can throw.
                    if existing {
                        oracle_source_map_set(&mut self.vault_cache, id.clone(), vault.clone())?;
                    }
                }
                oracle_source_map_set(&mut self.vault_cache, id.clone(), vault.clone())?;
                self.source_synthetic_vault("saveVault", &[id, vault])
            }
            "getTotalBalanceForProperty" | "getTotalOutstandingForProperty" => {
                self.source_synthetic_initialize()?;
                let property = arg(a, 0);
                let mut total = FundingBn::new(&n(0.))?;
                for (id, vault) in &self.vault_cache {
                    let V::String(id) = id else {
                        return Err(type_error("vaultId.split is not a function"));
                    };
                    let parts = id.split('-').collect::<Vec<_>>();
                    if parts.len() < 3 {
                        continue;
                    }
                    let is_synth = matches!(&property,V::String(v)if v.starts_with("s-"));
                    if method == "getTotalOutstandingForProperty" && is_synth {
                        if !same(&s(id), &property) {
                            continue;
                        }
                    } else if !same(
                        &n(if method == "getTotalBalanceForProperty" {
                            registry_parse_int(&s(parts[1]))?
                        } else {
                            synthetic_parse_int10(&s(parts[1]))?
                        }),
                        &property,
                    ) {
                        continue;
                    }
                    let amount = if method == "getTotalBalanceForProperty" {
                        FundingBn::new(&funding_or(funding_prop(vault, "margin")?))?.op(
                            FundingBn::new(&funding_or(funding_prop(vault, "available")?))?,
                            0,
                        )?
                    } else {
                        FundingBn::new(&funding_or(funding_prop(vault, "outstanding")?))?
                    };
                    total = total.op(amount, 0)?;
                }
                Ok(total.q(8, FundingRounding::HalfUp)?.value())
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_synthetic_property(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "getInstance" => Ok(obj(&[("sourcePropertyInstance", V::Bool(true))])),
            "addProperty" => {
                let kind = match text(&arg(a, 3))?.as_str() {
                    "Fixed" => 1.,
                    "Managed" => 2.,
                    "Native" => 3.,
                    "Vesting" => 4.,
                    "Synthetic" => 5.,
                    "Non-Fungible" => 6.,
                    "Procedural" => 7.,
                    _ => return Err(error("Invalid property type.")),
                };
                let id = arg(a, 0);
                let mut property = synthetic_value(&self.property_cache, &id);
                let meta = if arg(a, 7) == V::Undefined {
                    obj(&[])
                } else {
                    arg(a, 7)
                };
                if truth(&property) {
                    let total = FundingBn::new(&funding_prop(&property, "totalInCirculation")?)?
                        .op(FundingBn::new(&arg(a, 2))?, 0)?
                        .value();
                    let writes = [
                        ("totalInCirculation", synthetic_quantize(&total)?),
                        (
                            "ticker",
                            if truth(&arg(a, 1)) {
                                arg(a, 1)
                            } else {
                                property.get("ticker")
                            },
                        ),
                        ("type", n(kind)),
                        (
                            "whitelistId",
                            if truth(&arg(a, 4)) {
                                arg(a, 4)
                            } else {
                                property.get("whitelistId")
                            },
                        ),
                        (
                            "issuer",
                            if truth(&arg(a, 5)) {
                                arg(a, 5)
                            } else {
                                property.get("issuer")
                            },
                        ),
                        (
                            "backupAddress",
                            if truth(&arg(a, 6)) {
                                arg(a, 6)
                            } else {
                                property.get("backupAddress")
                            },
                        ),
                    ];
                    for (field, value) in writes {
                        set(&mut property, field, value)?;
                        oracle_source_map_set(
                            &mut self.property_cache,
                            id.clone(),
                            property.clone(),
                        )?;
                    }
                    let procedural = funding_prop(&meta, "proceduralType")?;
                    if !matches!(procedural, V::Undefined | V::Null) {
                        set(&mut property, "proceduralType", n(number(&procedural)?))?;
                    }
                } else {
                    let procedural = funding_prop(&meta, "proceduralType")?;
                    property = obj(&[
                        ("ticker", arg(a, 1)),
                        ("totalInCirculation", synthetic_quantize(&arg(a, 2))?),
                        ("type", n(kind)),
                        ("whitelistId", arg(a, 4)),
                        ("issuer", arg(a, 5)),
                        ("backupAddress", arg(a, 6)),
                        (
                            "proceduralType",
                            if matches!(procedural, V::Undefined | V::Null) {
                                V::Undefined
                            } else {
                                n(number(&procedural)?)
                            },
                        ),
                    ]);
                }
                oracle_source_map_set(&mut self.property_cache, id, property)?;
                let data = V::Array(
                    self.property_cache
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                );
                self.database("propertyList")?;
                self.update(
                    "propertyList",
                    obj(&[("_id", s("propertyIndex"))]),
                    obj(&[("_id", s("propertyIndex")), ("value", s(json(&data)?))]),
                    true,
                )?;
                Ok(V::Undefined)
            }
            "updateTotalInCirculation" => {
                let id = arg(a, 0);
                let mut property = self.call("Property.getPropertyData", vec![id.clone()])?;
                if !truth(&property) {
                    return Err(error("Property not found"));
                }
                let next = FundingBn::new(&funding_prop(&property, "totalInCirculation")?)?
                    .op(FundingBn::new(&arg(a, 1))?, 0)?
                    .value();
                set(
                    &mut property,
                    "totalInCirculation",
                    synthetic_quantize(&next)?,
                )?;
                self.database("propertyList")?;
                let row = self.find("propertyList", obj(&[("_id", s("propertyIndex"))]))?;
                let parsed = parse(&text(&funding_prop(&row, "value")?)?)?;
                let V::Array(entries) = parsed else {
                    return Err(type_error("parsedData.find is not a function"));
                };
                let mut found = false;
                for entry in entries {
                    if same(&registry_index(&entry, 0)?, &id) {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Err(error(
                        "Failed to update totalInCirculation, property not found in index",
                    ));
                }
                // `const base = db.getDatabase(...)` is intentionally not
                // awaited. Its Promise has no updateAsync method.
                match self.database("propertyList") {
                    Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
                    Err(e) => return Err(e),
                    Ok(_) => (),
                }
                Err(type_error("base.updateAsync is not a function"))
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_synthetic_logic(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "mintSynthetic" => {
                let address = arg(a, 0);
                let property = arg(a, 1);
                let contract = arg(a, 2);
                let id = s(format!("s-{}-{}", text(&property)?, text(&contract)?));
                self.call("Property.getInstance", vec![])?;
                let amount = n(FundingBn::new(&n(number(&arg(a, 3))?.abs()))?
                    .q(8, FundingRounding::Down)?
                    .number());
                let margin = n(FundingBn::new(&n(number(&arg(a, 7))?.abs()))?
                    .q(8, FundingRounding::Down)?
                    .number());
                let map = self.call("Margin.getInstance", vec![contract.clone()])?;
                let info = self.call("Property.getPropertyData", vec![property.clone()])?;
                let ticker = s(format!(
                    "s{}-{}",
                    text(&funding_prop(&info, "ticker")?)?,
                    text(&contract)?
                ));
                let info = self.call("Contract.getContractInfo", vec![contract.clone()])?;
                self.call(
                    "Property.addProperty",
                    vec![
                        id.clone(),
                        ticker,
                        amount.clone(),
                        s("Synthetic"),
                        funding_prop(&info, "whitelist")?,
                        id.clone(),
                        V::Null,
                    ],
                )?;
                let moved = self.call(
                    "Margin.moveMarginAndContractsForMint",
                    vec![
                        map,
                        address.clone(),
                        property.clone(),
                        contract.clone(),
                        arg(a, 6),
                        margin,
                        arg(a, 4),
                    ],
                )?;
                let gross = n(synthetic_up(
                    FundingBn::new(&arg(a, 5))?
                        .op(FundingBn::new(&funding_prop(&moved, "excess")?)?, 0)?,
                    8,
                )?
                .number());
                self.call(
                    "Tally.updateBalance",
                    vec![
                        address.clone(),
                        id.clone(),
                        amount.clone(),
                        n(0.),
                        n(0.),
                        n(0.),
                        s("issueSynth"),
                        arg(a, 4),
                    ],
                )?;
                self.call(
                    "Tally.updateBalance",
                    vec![
                        address,
                        property.clone(),
                        n(synthetic_up(FundingBn::new(&n(-number(&gross)?))?, 8)?.number()),
                        n(0.),
                        n(synthetic_up(
                            FundingBn::new(&n(-number(&funding_prop(&moved, "margin")?)?))?,
                            8,
                        )?
                        .number()),
                        n(0.),
                        s("issueSynth"),
                        arg(a, 4),
                    ],
                )?;
                let exists = self.call("Vaults.exists", vec![id.clone()])?;
                if !truth(&exists) {
                    self.call(
                        "Vaults.createVault",
                        vec![property.clone(), contract.clone()],
                    )?;
                    self.call(
                        "Vaults.registerSyntheticToken",
                        vec![id.clone(), contract, property],
                    )?;
                }
                self.call("Vaults.updateVault", vec![id, moved, amount, gross])?;
                Ok(V::Undefined)
            }
            "redeemSynthetic" => self.source_synthetic_redeem(a),
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_synthetic_redeem(&mut self, a: &[V]) -> R<V> {
        let address = arg(a, 0);
        let property = arg(a, 1);
        let contract = arg(a, 2);
        let amount = arg(a, 3);
        let V::String(id) = &property else {
            return Err(type_error(if matches!(property, V::Null | V::Undefined) {
                format!(
                    "Cannot read properties of {} (reading 'split')",
                    text(&property)?
                )
            } else {
                "propertyId.split is not a function".into()
            }));
        };
        let collateral = n(synthetic_parse_int10(&s(id
            .split('-')
            .nth(1)
            .unwrap_or("undefined")))?);
        let vault = self.call("Vaults.getVault", vec![property.clone()])?;
        // Source log dereferences outstanding before its missing-vault guard.
        let outstanding = funding_prop(&vault, "outstanding")?;
        if crate::oracle_source_owner::source_greater_unverified(&amount, &outstanding)? {
            return Err(error("Insufficient synthetic token balance for redemption"));
        }
        let map = self.call("Margin.getInstance", vec![contract.clone()])?;
        let info = self.call("Contract.getContractInfo", vec![contract.clone()])?;
        let native = truth(&funding_prop(&info, "native")?);
        let mark = if native {
            let pairs = funding_prop(&info, "onChainData")?;
            let pair = registry_index(&pairs, 0)?;
            self.call(
                "Volume.getLastPrice",
                vec![
                    s(format!(
                        "{}-{}",
                        text(&registry_index(&pair, 0)?)?,
                        text(&registry_index(&pair, 1)?)?
                    )),
                    arg(a, 4),
                ],
            )?
        } else {
            V::Promise
        };
        let initial = if native {
            self.call(
                "Contract.getInitialMargin",
                vec![contract.clone(), mark.clone()],
            )?
        } else {
            self.source_synthetic_unawaited_oracle_margin(
                &contract,
                funding_prop(&info, "underlyingOracleId")?,
            )?
        };
        let moved = self.call(
            "Margin.moveMarginAndContractsForRedeem",
            vec![
                map,
                address.clone(),
                property.clone(),
                contract,
                amount.clone(),
                vault,
                funding_prop(&info, "notionalValue")?,
                initial,
                mark,
            ],
        )?;
        self.call(
            "Property.updateTotalInCirculation",
            vec![property.clone(), n(-number(&amount)?)],
        )?;
        self.call(
            "Tally.updateBalance",
            vec![
                address.clone(),
                property.clone(),
                n(-number(&amount)?),
                n(0.),
                n(0.),
                n(0.),
                s("redeemSynth"),
                arg(a, 4),
            ],
        )?;
        self.call(
            "Tally.updateBalance",
            vec![
                address.clone(),
                collateral.clone(),
                funding_prop(&moved, "available")?,
                n(0.),
                funding_prop(&moved, "margin")?,
                n(0.),
                s("redeemSynth"),
                arg(a, 4),
            ],
        )?;
        let pnl = funding_prop(&moved, "rPNL")?;
        if !registry_equal(&pnl, &n(0.))? {
            self.call(
                "Tally.updateBalance",
                vec![
                    address.clone(),
                    collateral.clone(),
                    pnl,
                    n(0.),
                    n(0.),
                    n(0.),
                    s("closingLongsWithRedemption"),
                    arg(a, 4),
                ],
            )?;
        }
        let reduction = funding_prop(&moved, "reduction")?;
        if !registry_equal(&reduction, &n(0.))? {
            self.call(
                "Tally.updateBalance",
                vec![
                    address,
                    collateral,
                    reduction.clone(),
                    n(0.),
                    n(-number(&reduction)?),
                    n(0.),
                    s("contractRedeemMarginReturn"),
                    arg(a, 4),
                ],
            )?;
        }
        self.call(
            "Vaults.updateVaultRedeem",
            vec![property, moved, n(-number(&amount)?)],
        )?;
        Ok(V::Undefined)
    }
}
