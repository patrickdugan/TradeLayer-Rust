// Included only by the explicit lower lifecycle policy. The existing Core
// property cache, actual shared DB and Tally remain the sole authorities.
fn lower_default(value: V, fallback: V) -> V {
    if value == V::Undefined {
        fallback
    } else {
        value
    }
}
fn lower_primitive_identity(value: &V) -> R<()> {
    if matches!(
        value,
        V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise
    ) {
        return Err(SourceRuleError::InvalidInput.into());
    }
    Ok(())
}
fn lower_property_write(value: &mut V, field: &str, next: V) -> R<()> {
    match value {
        V::Object(_) => set(value, field, next),
        V::Null | V::Undefined => Err(type_error(format!(
            "Cannot set properties of {} (setting '{field}')",
            text(value)?
        ))),
        V::String(_) | V::Number(_) | V::Bool(_) => {
            let kind = match value {
                V::String(_) => "string",
                V::Bool(_) => "boolean",
                _ => "number",
            };
            Err(type_error(format!(
                "Cannot create property '{field}' on {kind} '{}'",
                text(value)?
            )))
        }
        // Non-index own fields of array/BigNumber instances need reference
        // identity and descriptors absent from this bounded SourceValue domain.
        _ => Err(SourceRuleError::InvalidInput.into()),
    }
}
impl ContractCore {
    fn source_lower_property(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "getNextPropertyId" => {
                self.call("Property.load", vec![])?;
                let mut max = 0.0_f64;
                for (key, _) in &self.property_cache {
                    let key = number(key)?;
                    if key.is_nan() || max.is_nan() {
                        max = f64::NAN;
                    } else if key > max {
                        max = key;
                    }
                }
                Ok(n(max + 1.0))
            }
            "createToken" => {
                let ticker = arg(a, 0);
                lower_primitive_identity(&ticker)?;
                if self
                    .property_cache
                    .iter()
                    .any(|(id, _)| oracle_source_same_key(id, &ticker))
                {
                    return Err(type_error("ticker is not a constructor"));
                }
                for (_, property) in &self.property_cache {
                    if same(&funding_prop(property, "ticker")?, &ticker) {
                        // Native returned Error is represented by its diagnostic
                        // name/message projection, as in existing source owners.
                        return Ok(obj(&[
                            ("name", s("Error")),
                            (
                                "message",
                                s(format!("Ticker \"{}\" already exists.", text(&ticker)?)),
                            ),
                        ]));
                    }
                }
                let next = self.call("Property.getNextPropertyId", vec![])?;
                let mut args = vec![next.clone()];
                args.extend((0..6).map(|index| arg(a, index)));
                args.push(lower_default(arg(a, 6), obj(&[])));
                self.call("Property.addProperty", args)?;
                Ok(next)
            }
            "addProperty" => {
                let property_type = source_channel_string(&arg(a, 3))?;
                if [
                    "constructor",
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
                    "__proto__",
                ]
                .contains(&property_type.as_str())
                {
                    return Err(SourceRuleError::InvalidInput.into());
                }
                let kind = match property_type.as_str() {
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
                let meta = lower_default(arg(a, 7), obj(&[]));
                if truth(&property) {
                    let total = FundingBn::new(&funding_prop(&property, "totalInCirculation")?)?
                        .op(FundingBn::new(&arg(a, 2))?, 0)?
                        .value();
                    lower_property_write(
                        &mut property,
                        "totalInCirculation",
                        synthetic_quantize(&total)?,
                    )?;
                    oracle_source_map_set(&mut self.property_cache, id.clone(), property.clone())?;
                    for (field, index) in [
                        ("ticker", 1),
                        ("type", 3),
                        ("whitelistId", 4),
                        ("issuer", 5),
                        ("backupAddress", 6),
                    ] {
                        let value = if field == "type" {
                            n(kind)
                        } else {
                            let supplied = arg(a, index);
                            if truth(&supplied) {
                                supplied
                            } else {
                                funding_prop(&property, field)?
                            }
                        };
                        lower_property_write(&mut property, field, value)?;
                        oracle_source_map_set(
                            &mut self.property_cache,
                            id.clone(),
                            property.clone(),
                        )?;
                    }
                    let procedural = funding_prop(&meta, "proceduralType")?;
                    if !matches!(procedural, V::Null | V::Undefined) {
                        lower_property_write(
                            &mut property,
                            "proceduralType",
                            n(number(&procedural)?),
                        )?;
                        oracle_source_map_set(
                            &mut self.property_cache,
                            id.clone(),
                            property.clone(),
                        )?;
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
                            if matches!(procedural, V::Null | V::Undefined) {
                                V::Undefined
                            } else {
                                n(number(&procedural)?)
                            },
                        ),
                    ]);
                }
                // Source evaluates an unused `blob`, including quantize/meta
                // getters, before installing a newly created property.
                synthetic_quantize(&arg(a, 2))?;
                let procedural = funding_prop(&meta, "proceduralType")?;
                if !matches!(procedural, V::Null | V::Undefined) {
                    number(&procedural)?;
                }
                oracle_source_map_set(&mut self.property_cache, id, property)?;
                self.call("Property.save", vec![])?;
                Ok(V::Undefined)
            }
            "doesTickerExist" => {
                self.database("propertyList")?;
                let row = self.find("propertyList", obj(&[("_id", s("propertyIndex"))]))?;
                if !truth(&row) {
                    return Ok(V::Bool(false));
                }
                let parsed = match parse(&text(&funding_prop(&row, "value")?)?) {
                    // An uncaught native JSON SyntaxError includes engine-specific
                    // text. Never manufacture its name/message from the older
                    // Core catch marker; this domain is explicitly local.
                    Err(Fault::Thrown(_)) => {
                        return Err(SourceRuleError::UnsupportedNumericDomain.into())
                    }
                    other => other?,
                };
                let V::Array(rows) = parsed else {
                    return Err(type_error("parsedIndex is not iterable"));
                };
                for row in rows {
                    let entry = match row {
                        V::Array(v) => v,
                        V::String(t) => t.chars().map(|c| s(c.to_string())).collect(),
                        _ => return Err(type_error(".for is not iterable")),
                    };
                    if same(&funding_prop(&arg(&entry, 1), "ticker")?, &arg(a, 0)) {
                        return Ok(V::Bool(true));
                    }
                }
                Ok(V::Bool(false))
            }
            "isManagedAndAdmin" => {
                lower_primitive_identity(&arg(a, 0))?;
                lower_primitive_identity(&arg(a, 1))?;
                let result = (|| -> R<V> {
                    self.database("propertyList")?;
                    let row = self.find("propertyList", obj(&[("_id", s("propertyIndex"))]))?;
                    if !truth(&row) || !truth(&row.get("value")) {
                        return Err(error("Property index not found."));
                    }
                    let parsed = parse(&text(&row.get("value"))?)?;
                    let V::Array(rows) = parsed else {
                        return Err(type_error("parsedIndex.find is not a function"));
                    };
                    for row in rows {
                        if same(&registry_index(&row, 0)?, &arg(a, 0)) {
                            let property = registry_index(&row, 1)?;
                            let kind = funding_prop(&property, "type")?;
                            return Ok(V::Bool(
                                (same(&kind, &n(2.)) || same(&kind, &n(7.)))
                                    && same(&funding_prop(&property, "issuer")?, &arg(a, 1)),
                            ));
                        }
                    }
                    Err(error(format!(
                        "Property with ID {} not found.",
                        text(&arg(a, 0))?
                    )))
                })();
                funding_caught(result, V::Bool(false))
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_lower_property_logic(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "tokenIssue" => {
                lower_primitive_identity(&arg(a, 0))?;
                self.call("Property.getInstance", vec![])?;
                let managed = lower_default(arg(a, 5), V::Bool(false));
                let nft = lower_default(arg(a, 7), V::Bool(false));
                let procedural = lower_default(arg(a, 9), V::Null);
                let mut kind = if truth(&nft) {
                    "Non-Fungible"
                } else if truth(&managed) {
                    "Managed"
                } else {
                    "Fixed"
                };
                if !matches!(procedural, V::Null | V::Undefined) && truth(&managed) {
                    kind = "Procedural";
                }
                let result = (|| -> R<V> {
                    let id = self.call(
                        "Property.createToken",
                        vec![
                            arg(a, 2),
                            arg(a, 1),
                            s(kind),
                            lower_default(arg(a, 4), n(0.)),
                            arg(a, 0),
                            lower_default(arg(a, 6), s("")),
                            obj(&[("proceduralType", procedural)]),
                        ],
                    )?;
                    self.call(
                        "Tally.updateBalance",
                        vec![
                            arg(a, 0),
                            id.clone(),
                            arg(a, 1),
                            n(0.),
                            n(0.),
                            n(0.),
                            s("issuance"),
                            arg(a, 8),
                        ],
                    )?;
                    // Only this real createToken call can return the native
                    // Error projection here; arbitrary caller objects are not
                    // reclassified as Error instances.
                    let id_text = if matches!(id, V::Object(_)) && id.get("name") == s("Error") {
                        format!("Error: {}", text(&id.get("message"))?)
                    } else {
                        text(&id)?
                    };
                    Ok(s(format!(
                        "Token {} (ID: {id_text}) created. Type: {kind}",
                        text(&arg(a, 2))?
                    )))
                })();
                match result {
                    Err(Fault::Thrown(error)) => Ok(s(error.message)),
                    other => other,
                }
            }
            "grantManagedToken" | "redeemManagedToken" => {
                let grant = method == "grantManagedToken";
                lower_primitive_identity(&arg(a, if grant { 3 } else { 2 }))?;
                if grant {
                    lower_primitive_identity(&arg(a, 2))?;
                }
                let sender = arg(a, if grant { 3 } else { 2 });
                let id = arg(a, 0);
                let admin = self.call(
                    "Property.isManagedAndAdmin",
                    vec![id.clone(), sender.clone()],
                )?;
                if grant && !truth(&admin) {
                    return Err(error("Sender is not admin of a managed property"));
                }
                self.call("Property.getInstance", vec![])?;
                let property = self.call("Property.getPropertyData", vec![id.clone()])?;
                let procedural = !matches!(property, V::Null | V::Undefined)
                    && number(&property.get("type"))? == 7.;
                if !grant && !truth(&admin) && !procedural {
                    return Err(error("Sender is not admin of a managed property"));
                }
                if procedural {
                    let offset = if grant { 6 } else { 4 };
                    let mut args = (offset..offset + 3)
                        .map(|i| lower_default(arg(a, i), s("")))
                        .collect::<Vec<_>>();
                    if grant {
                        args.push(lower_default(arg(a, 5), s("")));
                    }
                    let gate = self.call(
                        if grant {
                            "Procedural.ensureIssuanceContext"
                        } else {
                            "Procedural.ensureRedemptionContext"
                        },
                        args,
                    )?;
                    if !truth(&funding_prop(&gate, "valid")?) {
                        return Err(error(text(&funding_prop(&gate, "reason")?)?));
                    }
                }
                self.call(
                    if grant {
                        "Property.grantTokens"
                    } else {
                        "Property.redeemTokens"
                    },
                    vec![
                        id,
                        if grant { arg(a, 2) } else { sender },
                        arg(a, 1),
                        arg(a, if grant { 4 } else { 3 }),
                    ],
                )?;
                Ok(V::Undefined)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
impl SourceContractRuntime {
    fn source_lower_property_raw_logic(
        &mut self,
        kind: &V,
        params: &V,
    ) -> Result<SourceOutcome, SourceRuleError> {
        let (method, args) = if same(kind, &n(1.)) {
            let lists = params.get("whitelists");
            let clear = if let V::Array(values) = lists {
                proof_or(arg(&values, 0), n(0.))
            } else {
                proof_or(params.get("whitelistId"), n(0.))
            };
            let managed = if matches!(params.get("managed"), V::Null | V::Undefined) {
                params.get("isManaged")
            } else {
                params.get("managed")
            };
            let nft = if matches!(params.get("nft"), V::Null | V::Undefined) {
                params.get("isNFT")
            } else {
                params.get("nft")
            };
            (
                "tokenIssue",
                vec![
                    params.get("senderAddress"),
                    params.get("initialAmount"),
                    params.get("ticker"),
                    params.get("url"),
                    clear,
                    managed,
                    params.get("backupAddress"),
                    nft,
                    params.get("block"),
                    params.get("proceduralType"),
                ],
            )
        } else if same(kind, &n(11.)) {
            (
                "grantManagedToken",
                vec![
                    params.get("propertyId"),
                    params.get("amountGranted"),
                    params.get("addressToGrantTo"),
                    params.get("senderAddress"),
                    params.get("block"),
                    params.get("dlcHash"),
                    params.get("dlcTemplateId"),
                    params.get("dlcContractId"),
                    params.get("settlementState"),
                ],
            )
        } else if same(kind, &n(12.)) {
            (
                "redeemManagedToken",
                vec![
                    params.get("propertyId"),
                    params.get("amountDestroyed"),
                    params.get("senderAddress"),
                    params.get("block"),
                    params.get("dlcTemplateId"),
                    params.get("dlcContractId"),
                    params.get("settlementState"),
                ],
            )
        } else {
            return Err(SourceRuleError::InvalidInput);
        };
        Ok(
            match self.invoke_dependency(&DependencyCall {
                target: format!("Logic.{method}"),
                arguments: args,
            })? {
                DependencyReturn::Immediate(_) | DependencyReturn::Pending(_) => {
                    SourceOutcome::Returned(V::Undefined)
                }
                DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                    SourceOutcome::Thrown(e)
                }
            },
        )
    }
}
