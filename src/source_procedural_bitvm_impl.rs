// Included inside source_contract_runtime.rs. One Core table/clock authority.
fn pb_or(value: V, fallback: V) -> V {
    if truth(&value) {
        value
    } else {
        fallback
    }
}
fn pb_default(value: V, fallback: V) -> V {
    if value == V::Undefined {
        fallback
    } else {
        value
    }
}
fn pb_number(value: &V) -> R<f64> {
    if matches!(value, V::Object(_) | V::Array(_)) {
        source_channel_string(value)?;
    }
    number(value)
}
fn pb_hash(value: &V) -> R<String> {
    Ok(crate::hash256::sha256(json(value)?.as_bytes())
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect())
}
fn pb_spread(target: &mut V, value: &V) -> R<()> {
    match value {
        V::Object(fields) => {
            for (k, v) in fields {
                set(target, k, v.clone())?;
            }
        }
        V::Array(values) => {
            for (i, v) in values.iter().enumerate() {
                set(target, &i.to_string(), v.clone())?;
            }
        }
        V::String(text) => {
            for (i, c) in text.encode_utf16().enumerate() {
                let text = String::from_utf16(&[c])
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                set(target, &i.to_string(), s(text))?;
            }
        }
        V::Null | V::Undefined | V::Bool(_) | V::Number(_) => (),
        _ => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
    }
    Ok(())
}
fn pb_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0. && b == 0. {
        if a.is_sign_negative() || b.is_sign_negative() {
            -0.
        } else {
            0.
        }
    } else {
        a.min(b)
    }
}
fn pb_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0. && b == 0. {
        if a.is_sign_positive() || b.is_sign_positive() {
            0.
        } else {
            -0.
        }
    } else {
        a.max(b)
    }
}
fn pb_clamp(value: &V) -> R<f64> {
    let x = pb_number(&funding_or(value.clone()))?;
    Ok(if !x.is_finite() || x < 0. { 0. } else { x })
}
fn pb_invalid(reason: impl Into<String>) -> V {
    obj(&[("valid", V::Bool(false)), ("reason", s(reason))])
}
impl ContractCore {
    fn source_proof_environment_configure(&mut self, a: &[V]) -> R<V> {
        let V::Object(fields) = arg(a, 0) else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        const NAMES: &[&str] = &[
            "TL_BITVM_CHALLENGE_BLOCKS",
            "TL_BITVM_SCHED_WINDOW_BLOCKS",
            "TL_BITVM_MAX_PENDING_ESCROW",
            "TL_BITVM_MAX_PENDING_ESCROW_PER_DLC",
            "TL_BITVM_MAX_DEPOSIT_PER_WINDOW",
            "TL_BITVM_MAX_WITHDRAW_PER_WINDOW",
            "TL_BITVM_MAX_SWEEP_PER_WINDOW",
            "TL_BITVM_REQUIRE_BUNDLE",
            "TL_BITVM_BUNDLE_PATH",
            "TL_ORACLE_REQUIRE_STATE_ROOT",
            "TL_ORACLE_STATE_COMMIT_SCHEME",
            "LIQUIDITY_REWARD_ADDRESS_CLEARLIST_ID",
            "LIQUIDITY_REWARD_ISSUER_CLEARLIST_ID",
            "LIQUIDITY_REWARD_PROPERTY_ALLOWLIST",
            "LIQUIDITY_REWARD_MAX_SHARE",
            "LIQUIDITY_REWARD_MIN_SHARE",
            "LIQUIDITY_REWARD_PIVOT_LTC",
            "LIQUIDITY_REWARD_SLOPE",
        ];
        if a.len() != 1
            || fields
                .iter()
                .any(|(k, v)| !NAMES.contains(&k.as_str()) || !matches!(v, V::String(_)))
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        self.source_proof_environment = V::Object(fields);
        Ok(V::Undefined)
    }
    fn source_proof_env_number(&self, name: &str, fallback: f64) -> R<f64> {
        let raw = self.source_proof_environment.get(name);
        if matches!(raw, V::Undefined | V::Null) || raw == s("") {
            return Ok(if fallback.is_nan() || fallback == 0. {
                0.
            } else {
                fallback
            });
        }
        let value = pb_number(&raw)?;
        Ok(if value.is_finite() { value } else { fallback })
    }
    fn source_procedural(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "_base" => self.database("procedural"),
            "_normalizeHash" => Ok(s(source_channel_string(&pb_or(arg(a, 0), s("")))?
                .trim_matches(funding_js_space)
                .to_lowercase())),
            "getContract" | "getTemplate" => {
                self.call("Procedural._base", vec![])?;
                let prefix = if method == "getContract" {
                    "contract"
                } else {
                    "template"
                };
                self.find(
                    "procedural",
                    obj(&[(
                        "_id",
                        s(format!("{prefix}-{}", source_channel_string(&arg(a, 0))?)),
                    )]),
                )
            }
            "upsertTemplate" | "upsertContract" => {
                self.call("Procedural._base", vec![])?;
                let contract = method == "upsertContract";
                let kind = if contract { "contract" } else { "template" };
                let key = s(format!("{kind}-{}", source_channel_string(&arg(a, 0))?));
                let mut doc = obj(&[("_id", key.clone()), ("type", s(kind))]);
                if contract {
                    set(&mut doc, "contractId", arg(a, 0))?;
                    set(&mut doc, "templateId", arg(a, 1))?;
                    set(&mut doc, "state", pb_default(arg(a, 2), s("OPEN")))?;
                } else {
                    set(&mut doc, "templateId", arg(a, 0))?;
                }
                pb_spread(
                    &mut doc,
                    &pb_default(arg(a, if contract { 3 } else { 1 }), obj(&[])),
                )?;
                self.update(
                    "procedural",
                    obj(&[("_id", key)]),
                    obj(&[("$set", doc.clone())]),
                    true,
                )?;
                Ok(doc)
            }
            "transitionContract" => {
                self.call("Procedural._base", vec![])?;
                let key = s(format!("contract-{}", source_channel_string(&arg(a, 0))?));
                let previous = self.find("procedural", obj(&[("_id", key.clone())]))?;
                let mut merged = obj(&[]);
                pb_spread(
                    &mut merged,
                    &pb_or(
                        previous,
                        obj(&[
                            ("_id", key.clone()),
                            ("type", s("contract")),
                            ("contractId", arg(a, 0)),
                        ]),
                    ),
                )?;
                pb_spread(&mut merged, &pb_default(arg(a, 2), obj(&[])))?;
                set(
                    &mut merged,
                    "state",
                    s(source_channel_string(&pb_or(arg(a, 1), s("")))?.to_uppercase()),
                )?;
                self.update(
                    "procedural",
                    obj(&[("_id", key)]),
                    obj(&[("$set", merged.clone())]),
                    true,
                )?;
                Ok(merged)
            }
            "ensureIssuanceContext" | "ensureRedemptionContext" => {
                let issuance = method == "ensureIssuanceContext";
                let template_id = arg(a, 0);
                let contract_id = arg(a, 1);
                let state = pb_default(arg(a, 2), s(""));
                if !truth(&template_id) || !truth(&contract_id) {
                    return Ok(pb_invalid(if issuance {
                        "Missing dlcTemplateId/dlcContractId for procedural token"
                    } else {
                        "Missing dlcTemplateId/dlcContractId for procedural redemption"
                    }));
                }
                let mut template = V::Undefined;
                if issuance {
                    let hash = pb_default(arg(a, 3), s(""));
                    if !truth(&hash) {
                        return Ok(pb_invalid("Missing dlcHash for procedural token issuance"));
                    }
                    template = self.call("Procedural.getTemplate", vec![template_id.clone()])?;
                    if !truth(&template) {
                        return Ok(pb_invalid("Unknown DLC template id for procedural token"));
                    }
                    let expected = pb_or(
                        funding_prop(&template, "templateHash")?,
                        pb_or(
                            funding_prop(&template, "dlcHash")?,
                            funding_prop(&template, "hash")?,
                        ),
                    );
                    let expected = self.call("Procedural._normalizeHash", vec![expected])?;
                    if !truth(&expected) {
                        return Ok(pb_invalid("DLC template missing published hash reference"));
                    }
                    if self.call("Procedural._normalizeHash", vec![hash])? != expected {
                        return Ok(pb_invalid("DLC template hash mismatch"));
                    }
                }
                let contract = self.call("Procedural.getContract", vec![contract_id])?;
                if !truth(&contract) {
                    return Ok(pb_invalid(if issuance {
                        "Unknown DLC contract id for procedural token"
                    } else {
                        "Unknown DLC contract id for procedural redemption"
                    }));
                }
                if source_channel_string(&funding_prop(&contract, "templateId")?)?
                    != source_channel_string(&template_id)?
                {
                    return Ok(pb_invalid("DLC template/contract mismatch"));
                }
                let current =
                    source_channel_string(&pb_or(funding_prop(&contract, "state")?, s("")))?
                        .to_uppercase();
                let states = if issuance {
                    ["FUNDED", "OPEN"]
                } else {
                    ["SETTLED", "CLOSED"]
                };
                if !states.contains(&current.as_str()) {
                    return Ok(pb_invalid(format!(
                        "DLC contract state {current} not {}",
                        if issuance { "mintable" } else { "redeemable" }
                    )));
                }
                if truth(&state) && source_channel_string(&state)?.to_uppercase() != current {
                    return Ok(pb_invalid(
                        "Provided settlementState does not match DLC contract state",
                    ));
                }
                let mut result = obj(&[("valid", V::Bool(true)), ("contract", contract)]);
                if issuance {
                    set(&mut result, "template", template)?;
                }
                Ok(result)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_bitvm_cache(&mut self, method: &str, a: &[V]) -> R<V> {
        if method == "_base" {
            return self.database("procedural");
        }
        if method == "_challengeBlocks" {
            let fallback = pb_number(&pb_or(
                self.source_proof_environment
                    .get("TL_BITVM_CHALLENGE_BLOCKS"),
                n(6.),
            ))?;
            let value = pb_number(&arg(a, 0))?;
            return Ok(n(if !value.is_finite() || value < 0. {
                pb_max(0., fallback)
            } else {
                value.floor()
            }));
        }
        if method == "buildCacheId" {
            let p = pb_default(arg(a, 0), obj(&[]));
            let id = funding_prop(&p, "cacheId")?;
            if truth(&id) {
                return Ok(s(source_channel_string(&id)?));
            }
            let mut raw = obj(&[]);
            for k in ["dlcRef", "stateHash", "bundleHash"] {
                set(&mut raw, k, pb_or(funding_prop(&p, k)?, s("")))?;
            }
            for k in ["propertyId", "amount"] {
                set(
                    &mut raw,
                    k,
                    n(pb_number(&funding_or(funding_prop(&p, k)?))?),
                )?;
            }
            for k in ["fromAddress", "toAddress", "cacheAddress"] {
                set(&mut raw, k, pb_or(funding_prop(&p, k)?, s("")))?;
            }
            return Ok(s(pb_hash(&raw)?));
        }
        if method == "open" {
            return self.source_bitvm_open(a);
        }
        if !["get", "challenge", "finalize", "resolve"].contains(&method) {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        self.call("BitvmCache._base", vec![])?;
        let cache_id = arg(a, 0);
        let id = source_channel_string(&cache_id)?;
        let key = s(format!("bitvm-cache-{id}"));
        let mut doc = self.find("procedural", obj(&[("_id", key.clone())]))?;
        if method == "get" {
            return Ok(doc);
        }
        if !truth(&doc) {
            return Err(error(format!("Unknown BitVM cacheId: {id}")));
        }
        let detail = pb_default(arg(a, 1), obj(&[]));
        let status = funding_prop(&doc, "status")?;
        if method == "finalize" {
            if status == s("CHALLENGED") {
                return Err(error(format!(
                    "BitVM cache challenged; payout blocked for {id}"
                )));
            }
            if status == s("RESOLVED_UPHELD") {
                return Err(error(format!(
                    "BitVM cache challenge upheld; payout blocked for {id}"
                )));
            }
        }
        if status == s("RELEASED") {
            return Err(error(format!("BitVM cache already released: {id}")));
        }
        match method {
            "challenge" => {
                let mut challenged = match funding_prop(&doc, "challenged")? {
                    V::Array(values) => values,
                    _ => vec![],
                };
                if challenged.len() >= 1024 {
                    return Err(SourceRuleError::Capacity.into());
                }
                challenged.push(obj(&[
                    (
                        "challengerAddress",
                        pb_or(funding_prop(&detail, "challengerAddress")?, s("")),
                    ),
                    (
                        "evidenceHash",
                        pb_or(funding_prop(&detail, "evidenceHash")?, s("")),
                    ),
                    ("reason", pb_or(funding_prop(&detail, "reason")?, s(""))),
                    (
                        "block",
                        n(pb_number(&funding_or(funding_prop(&detail, "block")?))?),
                    ),
                    ("ts", n(self.time_millis as f64)),
                ]));
                set(&mut doc, "status", s("CHALLENGED"))?;
                set(&mut doc, "challenged", V::Array(challenged))?;
                set(
                    &mut doc,
                    "lastChallengeBlock",
                    n(pb_number(&funding_or(funding_prop(&detail, "block")?))?),
                )?;
                let amount = pb_number(&funding_or(funding_prop(&detail, "challengeBondAmount")?))?;
                let pid = pb_number(&pb_or(
                    funding_prop(&detail, "challengeBondPropertyId")?,
                    funding_or(funding_prop(&doc, "propertyId")?),
                ))?;
                if !amount.is_finite() || amount < 0. {
                    return Err(error("Invalid bitvm challengeBondAmount"));
                }
                if amount > 0. && (!pid.is_finite() || pid <= 0.) {
                    return Err(error("Invalid bitvm challengeBondPropertyId"));
                }
                let mut bonds = pb_or(funding_prop(&doc, "bonds")?, obj(&[]));
                let vault = s(format!("BITVM_BOND_CHALLENGE::{id}"));
                let mut bond = funding_prop(&bonds, "challengeBond")?;
                if !truth(&bond) {
                    bond = obj(&[
                        ("amount", n(0.)),
                        ("propertyId", n(pid)),
                        ("payerAddress", s("")),
                        ("vaultAddress", vault.clone()),
                    ]);
                }
                if amount > 0. && pb_number(&funding_or(funding_prop(&bond, "amount")?))? == 0. {
                    bond = obj(&[
                        ("amount", n(amount)),
                        ("propertyId", n(pid)),
                        (
                            "payerAddress",
                            pb_or(funding_prop(&detail, "challengerAddress")?, s("")),
                        ),
                        ("vaultAddress", vault),
                    ]);
                }
                set(&mut bonds, "challengeBond", bond)?;
                set(&mut doc, "bonds", bonds)?;
            }
            "finalize" => {
                let at = pb_number(&funding_or(funding_prop(&detail, "block")?))?;
                if !at.is_finite()
                    || at < pb_number(&funding_or(funding_prop(&doc, "challengeDeadlineBlock")?))?
                {
                    return Err(error(format!(
                        "BitVM challenge window still open for {id} (deadline {}, block {})",
                        source_channel_string(&funding_prop(&doc, "challengeDeadlineBlock")?)?,
                        source_channel_string(&n(at))?
                    )));
                }
                let to = funding_prop(&detail, "toAddress")?;
                let amount = pb_number(&pb_or(
                    funding_prop(&detail, "amount")?,
                    funding_or(funding_prop(&doc, "amount")?),
                ))?;
                let pid = pb_number(&pb_or(
                    funding_prop(&detail, "propertyId")?,
                    funding_or(funding_prop(&doc, "propertyId")?),
                ))?;
                // A falsey override uses the very same live doc.toAddress value.
                if truth(&to) && !same(&to, &funding_prop(&doc, "toAddress")?) {
                    return Err(error(format!("BitVM payout recipient mismatch for {id}")));
                }
                if amount != pb_number(&funding_prop(&doc, "amount")?)? {
                    return Err(error(format!("BitVM payout amount mismatch for {id}")));
                }
                if pid != pb_number(&funding_prop(&doc, "propertyId")?)? {
                    return Err(error(format!("BitVM payout property mismatch for {id}")));
                }
                set(&mut doc, "status", s("RELEASED"))?;
                set(&mut doc, "releasedAtBlock", n(at))?;
                set(
                    &mut doc,
                    "releaseTxid",
                    pb_or(funding_prop(&detail, "txid")?, s("")),
                )?;
                set(&mut doc, "updatedAt", n(self.time_millis as f64))?;
            }
            "resolve" => {
                let verdict =
                    source_channel_string(&pb_or(funding_prop(&detail, "verdict")?, s("")))?
                        .to_lowercase();
                if !["uphold", "reject"].contains(&verdict.as_str()) {
                    return Err(error("bitvm_resolve requires verdict=uphold|reject"));
                }
                let at = pb_number(&funding_or(funding_prop(&detail, "block")?))?;
                set(
                    &mut doc,
                    "status",
                    s(if verdict == "uphold" {
                        "RESOLVED_UPHELD"
                    } else {
                        "PENDING"
                    }),
                )?;
                if verdict == "reject" {
                    let previous =
                        pb_number(&pb_or(funding_prop(&doc, "challengeDeadlineBlock")?, n(at)))?;
                    set(&mut doc, "challengeDeadlineBlock", n(pb_min(previous, at)))?;
                }
                set(
                    &mut doc,
                    "resolution",
                    obj(&[
                        ("verdict", s(verdict)),
                        (
                            "resolverAddress",
                            pb_or(funding_prop(&detail, "resolverAddress")?, s("")),
                        ),
                        ("reason", pb_or(funding_prop(&detail, "reason")?, s(""))),
                        ("block", n(at)),
                        ("ts", n(self.time_millis as f64)),
                    ]),
                )?;
            }
            _ => unreachable!(),
        }
        self.update(
            "procedural",
            obj(&[("_id", key)]),
            obj(&[("$set", doc.clone())]),
            true,
        )?;
        Ok(doc)
    }
    fn source_bitvm_open(&mut self, a: &[V]) -> R<V> {
        let settlement = arg(a, 0);
        let context = pb_default(arg(a, 1), obj(&[]));
        let pid = pb_number(&funding_prop(&settlement, "propertyId")?)?;
        let amount = pb_number(&funding_or(funding_prop(&settlement, "amount")?))?;
        if !pid.is_finite() || pid <= 0. {
            return Err(error("Invalid bitvm cache propertyId"));
        }
        if !amount.is_finite() || amount <= 0. {
            return Err(error("Invalid bitvm cache amount"));
        }
        let mut from = funding_prop(&settlement, "fromAddress")?;
        if !truth(&from) {
            from = funding_prop(&settlement, "holderAddress")?;
        }
        if !truth(&from) {
            from = pb_or(funding_prop(&context, "senderAddress")?, s(""));
        }
        let mut to = funding_prop(&settlement, "toAddress")?;
        if !truth(&to) {
            to = funding_prop(&settlement, "recipientAddress")?;
        }
        if !truth(&to) {
            to = pb_or(funding_prop(&context, "senderAddress")?, s(""));
        }
        let mut cache = funding_prop(&settlement, "cacheAddress")?;
        if !truth(&cache) {
            let mut name = funding_prop(&context, "dlcRef")?;
            if !truth(&name) {
                name = funding_prop(&context, "stateHash")?;
            }
            cache = s(format!(
                "BITVM_CACHE::{}",
                source_channel_string(&pb_or(name, s("default")))?
            ));
        }
        let blocks = self.call(
            "BitvmCache._challengeBlocks",
            vec![funding_prop(&settlement, "challengeBlocks")?],
        )?;
        let bond = pb_number(&funding_or(funding_prop(&settlement, "cacheBondAmount")?))?;
        let bond_pid = pb_number(&pb_or(
            funding_prop(&settlement, "cacheBondPropertyId")?,
            n(pid),
        ))?;
        if !bond.is_finite() || bond < 0. {
            return Err(error("Invalid bitvm cacheBondAmount"));
        }
        if bond > 0. && (!bond_pid.is_finite() || bond_pid <= 0.) {
            return Err(error("Invalid bitvm cacheBondPropertyId"));
        }
        let bundle = pb_or(funding_prop(&settlement, "bundleHash")?, s(""));
        let dlc = funding_prop(&context, "dlcRef")?;
        let state = funding_prop(&context, "stateHash")?;
        let id = self.call(
            "BitvmCache.buildCacheId",
            vec![obj(&[
                ("cacheId", funding_prop(&settlement, "cacheId")?),
                ("dlcRef", dlc.clone()),
                ("stateHash", state.clone()),
                ("bundleHash", bundle.clone()),
                ("propertyId", n(pid)),
                ("amount", n(amount)),
                ("fromAddress", from.clone()),
                ("toAddress", to.clone()),
                ("cacheAddress", cache.clone()),
            ])],
        )?;
        self.call("BitvmCache._base", vec![])?;
        let key = s(format!("bitvm-cache-{}", source_channel_string(&id)?));
        let previous = self.find("procedural", obj(&[("_id", key.clone())]))?;
        if truth(&previous) && funding_prop(&previous, "status")? != s("RELEASED") {
            return Err(error(format!(
                "BitVM cache already pending/challenged: {}",
                source_channel_string(&id)?
            )));
        }
        let created = pb_number(&funding_or(funding_prop(&context, "block")?))?;
        let sender = pb_or(funding_prop(&context, "senderAddress")?, s(""));
        let doc = obj(&[
            ("_id", key.clone()),
            ("type", s("bitvmCache")),
            ("cacheId", id.clone()),
            ("status", s("PENDING")),
            ("createdBlock", n(created)),
            ("challengeDeadlineBlock", n(created + pb_number(&blocks)?)),
            ("challengeBlocks", blocks),
            ("dlcRef", pb_or(dlc, s(""))),
            ("stateHash", pb_or(state, s(""))),
            ("bundleHash", bundle),
            ("fromAddress", from),
            ("toAddress", to),
            ("cacheAddress", cache),
            ("openerAddress", sender.clone()),
            ("propertyId", n(pid)),
            ("amount", n(amount)),
            ("cacheBondAmount", n(bond)),
            ("cacheBondPropertyId", n(bond_pid)),
            ("challenged", V::Array(vec![])),
            ("releasedAtBlock", V::Null),
            (
                "bonds",
                obj(&[
                    (
                        "cacheBond",
                        obj(&[
                            ("amount", n(bond)),
                            ("propertyId", n(bond_pid)),
                            ("payerAddress", sender),
                            (
                                "vaultAddress",
                                if bond > 0. {
                                    s(format!("BITVM_BOND_CACHE::{}", source_channel_string(&id)?))
                                } else {
                                    s("")
                                },
                            ),
                        ]),
                    ),
                    (
                        "challengeBond",
                        obj(&[
                            ("amount", n(0.)),
                            ("propertyId", n(pid)),
                            ("payerAddress", s("")),
                            (
                                "vaultAddress",
                                s(format!(
                                    "BITVM_BOND_CHALLENGE::{}",
                                    source_channel_string(&id)?
                                )),
                            ),
                        ]),
                    ),
                ]),
            ),
            ("createdAt", n(self.time_millis as f64)),
        ]);
        self.update(
            "procedural",
            obj(&[("_id", key)]),
            obj(&[("$set", doc.clone())]),
            true,
        )?;
        Ok(doc)
    }
    fn source_bitvm_risk_read(&mut self, key: V, defaults: V) -> R<V> {
        self.database("procedural")?;
        let row = self.find("procedural", obj(&[("_id", key.clone())]))?;
        if truth(&row) {
            return Ok(row);
        }
        let mut row = obj(&[("_id", key)]);
        pb_spread(&mut row, &defaults)?;
        Ok(row)
    }
    fn source_bitvm_risk_write(&mut self, doc: V) -> R<()> {
        self.database("procedural")?;
        self.update(
            "procedural",
            obj(&[("_id", funding_prop(&doc, "_id")?)]),
            obj(&[("$set", doc)]),
            true,
        )?;
        Ok(())
    }
    fn source_bitvm_risk(&mut self, method: &str, a: &[V]) -> R<V> {
        if !["onCacheOpen", "onEscrowRelease", "onSweep"].contains(&method) {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        let input = arg(a, 0);
        // Parameter destructuring rejects null/undefined before any reads.
        if matches!(input, V::Null | V::Undefined) {
            return Err(type_error(format!(
                "Cannot destructure property 'propertyId' of '{}' as it is {}.",
                if input == V::Null {
                    "object null".into()
                } else {
                    text(&input)?
                },
                text(&input)?
            )));
        }
        let pid = pb_number(&funding_or(funding_prop(&input, "propertyId")?))?;
        let amount = pb_clamp(&funding_prop(&input, "amount")?)?;
        if !pid.is_finite() || pid <= 0. || amount <= 0. {
            return Ok(V::Undefined);
        }
        let cap = |this: &Self, name: &str| -> R<f64> {
            pb_clamp(&n(this.source_proof_env_number(name, 0.)?))
        };
        let sweep_cap = if method == "onSweep" {
            cap(self, "TL_BITVM_MAX_SWEEP_PER_WINDOW")?
        } else {
            0.
        };
        if method == "onSweep" && sweep_cap <= 0. {
            return Ok(V::Undefined);
        }
        let max_pending = if method == "onCacheOpen" {
            cap(self, "TL_BITVM_MAX_PENDING_ESCROW")?
        } else {
            0.
        };
        let max_dlc = if method == "onCacheOpen" {
            cap(self, "TL_BITVM_MAX_PENDING_ESCROW_PER_DLC")?
        } else {
            0.
        };
        let max_window = if method == "onCacheOpen" {
            cap(self, "TL_BITVM_MAX_DEPOSIT_PER_WINDOW")?
        } else if method == "onEscrowRelease" {
            cap(self, "TL_BITVM_MAX_WITHDRAW_PER_WINDOW")?
        } else {
            0.
        };
        let dlc = funding_prop(&input, "dlcRef")?;
        let g_key = s(format!(
            "bitvm-risk-escrow-global-{}",
            source_channel_string(&n(pid))?
        ));
        let d_key = if method == "onSweep" {
            V::Undefined
        } else {
            s(format!(
                "bitvm-risk-escrow-dlc-{}-{}",
                source_channel_string(&n(pid))?,
                source_channel_string(&pb_or(dlc.clone(), s("default")))?
            ))
        };
        let win = pb_max(
            1.,
            self.source_proof_env_number("TL_BITVM_SCHED_WINDOW_BLOCKS", 144.)?
                .floor(),
        );
        let block = pb_number(&funding_or(funding_prop(&input, "block")?))?;
        let index = (pb_max(0., block) / win).floor();
        let w_key = s(format!(
            "bitvm-risk-window-{}-{}",
            source_channel_string(&n(pid))?,
            source_channel_string(&n(index))?
        ));
        let mut g = V::Undefined;
        let mut d = V::Undefined;
        if method != "onSweep" {
            g = self.source_bitvm_risk_read(
                g_key,
                obj(&[
                    ("type", s("bitvmRiskEscrowGlobal")),
                    ("propertyId", n(pid)),
                    ("pendingAmount", n(0.)),
                ]),
            )?;
            d = self.source_bitvm_risk_read(
                d_key,
                obj(&[
                    ("type", s("bitvmRiskEscrowDlc")),
                    ("propertyId", n(pid)),
                    ("dlcRef", s(source_channel_string(&pb_or(dlc, s("")))?)),
                    ("pendingAmount", n(0.)),
                ]),
            )?;
        }
        let mut w = self.source_bitvm_risk_read(
            w_key,
            obj(&[
                ("type", s("bitvmRiskWindow")),
                ("propertyId", n(pid)),
                ("depositAmount", n(0.)),
                ("withdrawAmount", n(0.)),
                ("sweepAmount", n(0.)),
            ]),
        )?;
        let field = if method == "onCacheOpen" {
            "depositAmount"
        } else if method == "onEscrowRelease" {
            "withdrawAmount"
        } else {
            "sweepAmount"
        };
        let next = pb_clamp(&funding_prop(&w, field)?)? + amount;
        if method == "onCacheOpen" {
            let next_g = pb_clamp(&funding_prop(&g, "pendingAmount")?)? + amount;
            let next_d = pb_clamp(&funding_prop(&d, "pendingAmount")?)? + amount;
            for (limit, value, label) in [
                (
                    max_pending,
                    next_g,
                    "BitVM escrow cap exceeded: global pending",
                ),
                (max_dlc, next_d, "BitVM escrow cap exceeded: dlc pending"),
                (max_window, next, "BitVM deposit window cap exceeded:"),
            ] {
                if limit > 0. && value > limit {
                    return Err(error(format!(
                        "{label} {} > {}",
                        source_channel_string(&n(value))?,
                        source_channel_string(&n(limit))?
                    )));
                }
            }
            set(&mut g, "pendingAmount", n(next_g))?;
            set(&mut d, "pendingAmount", n(next_d))?;
        } else if method == "onEscrowRelease" {
            if max_window > 0. && next > max_window {
                return Err(error(format!(
                    "BitVM withdraw window cap exceeded: {} > {}",
                    source_channel_string(&n(next))?,
                    source_channel_string(&n(max_window))?
                )));
            }
            let next_g = pb_max(0., pb_clamp(&funding_prop(&g, "pendingAmount")?)? - amount);
            set(&mut g, "pendingAmount", n(next_g))?;
            let next_d = pb_max(0., pb_clamp(&funding_prop(&d, "pendingAmount")?)? - amount);
            set(&mut d, "pendingAmount", n(next_d))?;
        } else if next > sweep_cap {
            return Err(error(format!(
                "BitVM sweep window cap exceeded: {} > {}",
                source_channel_string(&n(next))?,
                source_channel_string(&n(sweep_cap))?
            )));
        }
        set(&mut w, field, n(next))?;
        if method != "onSweep" {
            set(&mut g, "updatedAt", n(self.time_millis as f64))?;
            set(&mut d, "updatedAt", n(self.time_millis as f64))?;
        }
        set(&mut w, "updatedAt", n(self.time_millis as f64))?;
        if method != "onSweep" {
            self.source_bitvm_risk_write(g)?;
            self.source_bitvm_risk_write(d)?;
        }
        self.source_bitvm_risk_write(w)?;
        Ok(V::Undefined)
    }
}
