// Literal ordinary settlement orchestration; all spendable state is Core Tally.
fn proof_or(a: V, b: V) -> V {
    if truth(&a) {
        a
    } else {
        b
    }
}
fn proof_string(value: V) -> R<String> {
    source_channel_string(&value)
}
fn proof_hash(bytes: &[u8]) -> String {
    crate::hash256::sha256(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn proof_canonical_artifact_path(path: &str) -> bool {
    let value = std::path::Path::new(path);
    value.is_absolute()
        && !path
            .split(['/', '\\'])
            .any(|part| matches!(part, "." | ".."))
        && value.components().collect::<std::path::PathBuf>().to_str() == Some(path)
}
fn proof_base64(raw: &str) -> R<Vec<u8>> {
    let (mut acc, mut bits) = (0u32, 0u32);
    let mut bytes = Vec::new();
    for unit in raw.encode_utf16() {
        let b = unit as u8;
        if b == b'=' {
            break;
        }
        let d = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        acc = (acc << 6) | u32::from(d);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            if bytes.len() >= crate::bitvm_bundle::MAX_BUNDLE_BYTES {
                return Err(SourceRuleError::Capacity.into());
            }
            bytes.push((acc >> bits) as u8);
        }
    }
    Ok(bytes)
}
impl ContractCore {
    fn source_proof_artifacts_configure(&mut self, a: &[V]) -> R<V> {
        let value = arg(a, 0);
        let V::Object(fields) = &value else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if fields.len() > 32 {
            return Err(SourceRuleError::Capacity.into());
        }
        for (path, raw) in fields {
            if !proof_canonical_artifact_path(path) {
                return Err(SourceRuleError::InvalidInput.into());
            }
            let V::String(raw) = raw else {
                return Err(SourceRuleError::InvalidInput.into());
            };
            if raw.len() > crate::bitvm_bundle::MAX_BUNDLE_BYTES * 2 {
                return Err(SourceRuleError::Capacity.into());
            }
            crate::read_only::decode_hex(raw).map_err(|_| SourceRuleError::InvalidInput)?;
        }
        self.source_proof_artifacts = value;
        Ok(V::Undefined)
    }
    fn source_proof_property(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "getInstance" => {
                self.source_proof_property_initialized = true;
                Ok(obj(&[("sourcePropertyInstance", V::Bool(true))]))
            }
            "getPropertyIndex" => {
                self.call("Property.getInstance", vec![])?;
                if self.property_cache.is_empty() {
                    self.call("Property.load", vec![])?;
                }
                Ok(V::Array(
                    self.property_cache
                        .iter()
                        .map(|(id, p)| {
                            Ok(obj(&[
                                ("id", id.clone()),
                                ("ticker", funding_prop(p, "ticker")?),
                                ("totalInCirculation", funding_prop(p, "totalInCirculation")?),
                                ("type", funding_prop(p, "type")?),
                            ]))
                        })
                        .collect::<R<Vec<_>>>()?,
                ))
            }
            "load" => {
                self.call("Property.getInstance", vec![])?;
                let result = (|| -> R<()> {
                    self.database("propertyList")?;
                    let row = self.find("propertyList", obj(&[("_id", s("propertyIndex"))]))?;
                    let value = if truth(&row) && truth(&row.get("value")) {
                        match row.get("value") {
                            V::String(t) => parse(&t)?,
                            v => v,
                        }
                    } else {
                        V::Array(vec![])
                    };
                    let mut entries = vec![];
                    if let V::Array(rows) = value {
                        if rows.iter().all(|r| matches!(r,V::Array(v)if v.len()==2)) {
                            for pair in rows {
                                let values = array(&pair)?;
                                oracle_source_map_set(
                                    &mut entries,
                                    arg(&values, 0),
                                    arg(&values, 1),
                                )?;
                            }
                        }
                    }
                    self.property_cache = entries;
                    Ok(())
                })();
                match result {
                    Err(Fault::Thrown(_)) => (),
                    Err(e) => return Err(e),
                    _ => (),
                };
                Ok(V::Undefined)
            }
            "save" => {
                let pairs = V::Array(
                    self.property_cache
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                );
                let data = s(json(&pairs)?);
                self.database("propertyList")?;
                self.update(
                    "propertyList",
                    obj(&[("_id", s("propertyIndex"))]),
                    obj(&[("_id", s("propertyIndex")), ("value", data)]),
                    true,
                )?;
                Ok(V::Undefined)
            }
            "grantTokens" | "redeemTokens" => {
                let id = arg(a, 0);
                let amount = arg(a, 2);
                let mut property = self.call("Property.getPropertyData", vec![id.clone()])?;
                if !truth(&property) {
                    return Err(error(format!(
                        "Property with ID {} not found.",
                        proof_string(id)?
                    )));
                }
                let current = FundingBn::new(&funding_prop(&property, "totalInCirculation")?)?;
                if method == "redeemTokens"
                    && current.compare(&FundingBn::new(&amount)?) == Some(core::cmp::Ordering::Less)
                {
                    return Err(error(format!(
                        "Insufficient managed tokens for redemption for property {}.",
                        proof_string(id)?
                    )));
                }
                let next = current
                    .op(
                        FundingBn::new(&amount)?,
                        if method == "redeemTokens" { 1 } else { 0 },
                    )?
                    .value();
                set(
                    &mut property,
                    "totalInCirculation",
                    synthetic_quantize(&next)?,
                )?;
                self.call("Property.load", vec![])?;
                oracle_source_map_set(&mut self.property_cache, n(number(&id)?), property)?;
                self.call(
                    "Tally.updateBalance",
                    vec![
                        arg(a, 1),
                        id,
                        if method == "redeemTokens" {
                            n(-number(&amount)?)
                        } else {
                            amount
                        },
                        n(0.),
                        n(0.),
                        n(0.),
                        s(if method == "redeemTokens" {
                            "redeemToken"
                        } else {
                            "grantToken"
                        }),
                        arg(a, 3),
                    ],
                )?;
                self.call("Property.save", vec![])?;
                Ok(V::Undefined)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_proof_state_gate(&mut self, params: &V, parsed: &V, settlement: &V) -> R<()> {
        if proof_string(proof_or(
            self.source_proof_environment
                .get("TL_ORACLE_REQUIRE_STATE_ROOT"),
            s(""),
        ))?
        .trim_matches(funding_js_space)
            != "1"
        {
            return Ok(());
        }
        if !matches!(parsed, V::Object(_) | V::Array(_)) {
            return Err(error("State-root gate: missing parsed relay bundle"));
        }
        let hash = proof_string(proof_or(parsed.get("payloadHash"), s("")))?
            .trim_matches(funding_js_space)
            .to_lowercase();
        let b64 = proof_string(proof_or(parsed.get("balancePayloadB64"), s("")))?
            .trim_matches(funding_js_space)
            .to_string();
        if hash.is_empty() || b64.is_empty() {
            return Err(error(
                "State-root gate: payloadHash/balancePayloadB64 required",
            ));
        }
        let bytes = proof_base64(&b64)?;
        if proof_hash(&bytes) != hash {
            return Err(error("State-root gate: payload hash mismatch"));
        }
        let payload = match parse(&String::from_utf8_lossy(&bytes)) {
            Ok(v) => v,
            Err(Fault::Thrown(_)) => {
                return Err(error("State-root gate: invalid balance payload encoding"))
            }
            Err(e) => return Err(e),
        };
        let state = proof_string(proof_or(
            settlement.get("stateRoot"),
            proof_or(payload.get("stateRoot"), s("")),
        ))?
        .trim_matches(funding_js_space)
        .to_lowercase();
        let payload_state = proof_string(proof_or(payload.get("stateRoot"), s("")))?
            .trim_matches(funding_js_space)
            .to_lowercase();
        if state.is_empty() || payload_state.is_empty() {
            return Err(error(
                "State-root gate: missing settlement/payload stateRoot",
            ));
        }
        if state != payload_state {
            return Err(error("State-root gate: stateRoot mismatch"));
        }
        let transition = proof_string(proof_or(settlement.get("transitionHash"), s("")))?
            .trim_matches(funding_js_space)
            .to_lowercase();
        let expected = proof_hash(
            json(&obj(&[
                (
                    "mode",
                    s(proof_string(proof_or(
                        settlement.get("mode"),
                        proof_or(settlement.get("action"), s("")),
                    ))?
                    .to_lowercase()),
                ),
                (
                    "propertyId",
                    n(number(&funding_or(settlement.get("propertyId")))?),
                ),
                ("amount", n(number(&funding_or(settlement.get("amount")))?)),
                (
                    "fromAddress",
                    s(proof_string(proof_or(
                        settlement.get("fromAddress"),
                        proof_or(settlement.get("holderAddress"), s("")),
                    ))?),
                ),
                (
                    "toAddress",
                    s(proof_string(proof_or(
                        settlement.get("toAddress"),
                        proof_or(settlement.get("recipientAddress"), s("")),
                    ))?),
                ),
                (
                    "cacheId",
                    s(proof_string(proof_or(settlement.get("cacheId"), s("")))?),
                ),
                (
                    "dlcRef",
                    s(proof_string(proof_or(params.get("dlcRef"), s("")))?),
                ),
                (
                    "stateHash",
                    s(proof_string(proof_or(params.get("stateHash"), s("")))?),
                ),
            ]))?
            .as_bytes(),
        );
        if !transition.is_empty() && transition != expected {
            return Err(error("State-root gate: transitionHash mismatch"));
        }
        if let V::Array(values) = payload.get("transitions") {
            let transitions = values
                .into_iter()
                .map(|v| Ok(proof_string(proof_or(v, s("")))?.to_lowercase()))
                .collect::<R<Vec<_>>>()?;
            if !transitions.is_empty() && !transitions.contains(&expected) {
                return Err(error(
                    "State-root gate: transition not included in payload transitions",
                ));
            }
        }
        let scheme = proof_string(proof_or(
            self.source_proof_environment
                .get("TL_ORACLE_STATE_COMMIT_SCHEME"),
            s(""),
        ))?
        .trim_matches(funding_js_space)
        .to_lowercase();
        if scheme == "binohash" {
            let root = proof_string(proof_or(
                payload.get("binohash").get("root"),
                proof_or(payload.get("binoRoot"), s("")),
            ))?
            .trim_matches(funding_js_space)
            .to_lowercase();
            if root.is_empty() {
                return Err(error("State-root gate: binohash root missing"));
            }
            if state != root {
                return Err(error("State-root gate: stateRoot must equal binohash root"));
            }
            // Only the optional experimental verifier itself is uncomposed.
            return Err(SourceRuleError::UnknownDependency.into());
        }
        Ok(())
    }
    fn source_proof_sufficient(&mut self, address: V, pid: V, amount: V, label: &str) -> R<()> {
        let check = self.call("Tally.hasSufficientBalance", vec![address, pid, amount])?;
        if !truth(&check.get("hasSufficient")) {
            return Err(error(format!(
                "Insufficient balance for {label}: {}",
                proof_string(proof_or(check.get("reason"), s("unknown")))?
            )));
        }
        Ok(())
    }
    fn source_proof_transfer(
        &mut self,
        from: V,
        to: V,
        pid: V,
        amount: V,
        label: &str,
        block: &V,
    ) -> R<()> {
        self.call(
            "Tally.updateBalance",
            vec![
                from,
                pid.clone(),
                n(-number(&amount)?),
                n(0.),
                n(0.),
                n(0.),
                s(label),
                block.clone(),
            ],
        )?;
        self.call(
            "Tally.updateBalance",
            vec![
                to,
                pid,
                amount,
                n(0.),
                n(0.),
                n(0.),
                s(label),
                block.clone(),
            ],
        )?;
        Ok(())
    }
    fn source_proof_settlement(&mut self, sender: V, params: V, block: V) -> R<V> {
        let parsed = proof_raw_relay(&funding_prop(&params, "relayBlob")?)?;
        let mut settlement = proof_or(
            parsed.get("settlement"),
            proof_or(parsed.get("settle"), V::Null),
        );
        if !truth(&settlement) || !matches!(settlement, V::Object(_) | V::Array(_)) {
            return Ok(V::Undefined);
        }
        self.source_proof_state_gate(&params, &parsed, &settlement)?;
        let mode = proof_string(proof_or(
            settlement.get("mode"),
            proof_or(settlement.get("action"), s("")),
        ))?
        .to_lowercase();
        let pid = number(&settlement.get("propertyId"))?;
        let amount = number(&funding_or(settlement.get("amount")))?;
        let from = proof_or(
            settlement.get("fromAddress"),
            proof_or(settlement.get("holderAddress"), sender.clone()),
        );
        let to = proof_or(
            settlement.get("toAddress"),
            proof_or(settlement.get("recipientAddress"), sender.clone()),
        );
        self.call("Property.getInstance", vec![])?;
        let require_property = || {
            if !pid.is_finite() || pid <= 0. {
                Err(error("Invalid settlement propertyId in relayBlob"))
            } else {
                Ok(())
            }
        };
        if matches!(mode.as_str(), "redemption" | "redeem" | "rollover" | "roll") {
            require_property()?;
            let roll = matches!(mode.as_str(), "rollover" | "roll");
            let next = if roll {
                number(&settlement.get("nextPropertyId"))?
            } else {
                0.
            };
            if !amount.is_finite() || amount <= 0. {
                return Err(error(if roll {
                    "Invalid rollover amount in relayBlob"
                } else {
                    "Invalid redemption amount in relayBlob"
                }));
            }
            if roll && (!next.is_finite() || next <= 0.) {
                return Err(error("Invalid rollover nextPropertyId in relayBlob"));
            }
            self.call(
                "Property.redeemTokens",
                vec![n(pid), from, n(amount), block.clone()],
            )?;
            if roll {
                self.call("Property.grantTokens", vec![n(next), to, n(amount), block])?;
            }
            return Ok(V::Undefined);
        }
        if matches!(mode.as_str(), "pnl_sweep" | "pnlsweep" | "sweep") {
            require_property()?;
            self.call(
                "BitvmRisk.onSweep",
                vec![obj(&[
                    ("propertyId", n(pid)),
                    ("amount", n(amount)),
                    ("block", block.clone()),
                ])],
            )?;
            self.source_proof_sufficient(from.clone(), n(pid), n(amount), "pnl sweep")?;
            self.source_proof_transfer(from, to, n(pid), n(amount), "oraclePnlSweep", &block)?;
            return Ok(V::Undefined);
        }
        if matches!(mode.as_str(), "bitvm_cache" | "bitvmcache" | "cache") {
            require_property()?;
            if proof_string(proof_or(
                self.source_proof_environment.get("TL_BITVM_REQUIRE_BUNDLE"),
                s(""),
            ))?
            .trim_matches(funding_js_space)
                == "1"
            {
                let hash = proof_or(
                    settlement.get("bundleHash"),
                    proof_or(params.get("bundleHash"), s("")),
                );
                let path = proof_or(
                    settlement.get("bundlePath"),
                    proof_or(params.get("bundlePath"), s("")),
                );
                let check = self.source_proof_bundle(&hash, &path)?;
                if !truth(&check.get("valid")) {
                    return Err(error(format!(
                        "BitVM bundle verification failed: {}",
                        proof_string(proof_or(check.get("reason"), s("unknown")))?
                    )));
                }
                set(
                    &mut settlement,
                    "bundleHash",
                    proof_or(check.get("bundleHash"), hash),
                )?;
                set(
                    &mut settlement,
                    "bundlePath",
                    proof_or(check.get("bundlePath"), path),
                )?;
            }
            self.source_proof_sufficient(from.clone(), n(pid), n(amount), "bitvm cache")?;
            let bond = number(&funding_or(settlement.get("cacheBondAmount")))?;
            let bond_pid = number(&proof_or(settlement.get("cacheBondPropertyId"), n(pid)))?;
            if !bond.is_finite() || bond < 0. {
                return Err(error("Invalid bitvm cacheBondAmount"));
            }
            self.call(
                "BitvmRisk.onCacheOpen",
                vec![obj(&[
                    ("propertyId", n(pid)),
                    ("amount", n(amount)),
                    ("dlcRef", proof_or(params.get("dlcRef"), s(""))),
                    ("block", block.clone()),
                ])],
            )?;
            if bond > 0. {
                self.source_proof_sufficient(
                    sender.clone(),
                    n(bond_pid),
                    n(bond),
                    "bitvm cache bond",
                )?;
            }
            let doc = self.call(
                "BitvmCache.open",
                vec![
                    settlement,
                    obj(&[
                        ("senderAddress", sender.clone()),
                        ("dlcRef", params.get("dlcRef")),
                        ("stateHash", params.get("stateHash")),
                        ("block", block.clone()),
                    ]),
                ],
            )?;
            self.source_proof_transfer(
                from,
                funding_prop(&doc, "cacheAddress")?,
                n(pid),
                n(amount),
                "bitvmCacheLock",
                &block,
            )?;
            let vault = doc.get("bonds").get("cacheBond").get("vaultAddress");
            if bond > 0. && truth(&vault) {
                self.source_proof_transfer(
                    sender,
                    vault,
                    n(bond_pid),
                    n(bond),
                    "bitvmCacheBondLock",
                    &block,
                )?;
            }
            return Ok(V::Undefined);
        }
        if !matches!(
            mode.as_str(),
            "bitvm_challenge"
                | "bitvmchallenge"
                | "challenge"
                | "bitvm_payout"
                | "bitvmpayout"
                | "payout"
                | "bitvm_resolve"
                | "bitvmresolve"
                | "resolve"
        ) {
            return Ok(V::Undefined);
        }
        // Payout validates property before String(cacheId); challenge/resolve do not.
        if matches!(mode.as_str(), "bitvm_payout" | "bitvmpayout" | "payout") {
            require_property()?;
        }
        let cache_id = proof_string(proof_or(settlement.get("cacheId"), s("")))?;
        if matches!(
            mode.as_str(),
            "bitvm_challenge" | "bitvmchallenge" | "challenge"
        ) {
            if cache_id.is_empty() {
                return Err(error("bitvm_challenge requires cacheId"));
            }
            let challenger = proof_or(settlement.get("challengerAddress"), sender);
            let bond = number(&funding_or(settlement.get("challengeBondAmount")))?;
            let bond_pid = number(&proof_or(
                settlement.get("challengeBondPropertyId"),
                funding_or(n(pid)),
            ))?;
            if !bond.is_finite() || bond < 0. {
                return Err(error("Invalid bitvm challengeBondAmount"));
            }
            if bond > 0. {
                self.source_proof_sufficient(
                    challenger.clone(),
                    n(bond_pid),
                    n(bond),
                    "bitvm challenge bond",
                )?;
            }
            self.call(
                "BitvmCache.challenge",
                vec![
                    s(&cache_id),
                    obj(&[
                        ("challengerAddress", challenger.clone()),
                        (
                            "evidenceHash",
                            proof_or(
                                settlement.get("evidenceHash"),
                                proof_or(params.get("evidenceHash"), s("")),
                            ),
                        ),
                        ("reason", proof_or(settlement.get("reason"), s(""))),
                        ("block", block.clone()),
                        ("challengeBondAmount", n(bond)),
                        ("challengeBondPropertyId", n(bond_pid)),
                    ]),
                ],
            )?;
            if bond > 0. {
                let doc = self.call("BitvmCache.get", vec![s(&cache_id)])?;
                let vault = proof_or(
                    doc.get("bonds").get("challengeBond").get("vaultAddress"),
                    s(format!("BITVM_BOND_CHALLENGE::{cache_id}")),
                );
                self.source_proof_transfer(
                    challenger,
                    vault,
                    n(bond_pid),
                    n(bond),
                    "bitvmChallengeBondLock",
                    &block,
                )?;
            }
            return Ok(V::Undefined);
        }
        if matches!(mode.as_str(), "bitvm_payout" | "bitvmpayout" | "payout") {
            require_property()?;
            if cache_id.is_empty() {
                return Err(error("bitvm_payout requires cacheId"));
            }
            let doc = self.call(
                "BitvmCache.finalize",
                vec![
                    s(cache_id),
                    obj(&[
                        ("toAddress", to),
                        ("amount", n(amount)),
                        ("propertyId", n(pid)),
                        ("block", block.clone()),
                        ("txid", proof_or(params.get("txid"), s(""))),
                    ]),
                ],
            )?;
            let address = funding_prop(&doc, "cacheAddress")?;
            let pid = funding_prop(&doc, "propertyId")?;
            let amount = funding_prop(&doc, "amount")?;
            self.source_proof_sufficient(
                address.clone(),
                pid.clone(),
                amount.clone(),
                "bitvm payout",
            )?;
            self.source_proof_transfer(
                address,
                funding_prop(&doc, "toAddress")?,
                pid.clone(),
                amount.clone(),
                "bitvmPayoutRelease",
                &block,
            )?;
            self.call(
                "BitvmRisk.onEscrowRelease",
                vec![obj(&[
                    ("propertyId", pid),
                    ("amount", amount),
                    (
                        "dlcRef",
                        proof_or(doc.get("dlcRef"), proof_or(params.get("dlcRef"), s(""))),
                    ),
                    ("block", block),
                ])],
            )?;
            return Ok(V::Undefined);
        }
        if matches!(mode.as_str(), "bitvm_resolve" | "bitvmresolve" | "resolve") {
            if cache_id.is_empty() {
                return Err(error("bitvm_resolve requires cacheId"));
            }
            let declared = proof_string(proof_or(settlement.get("resolverAddress"), s("")))?
                .trim_matches(funding_js_space)
                .to_string();
            if !declared.is_empty() && !same(&s(declared), &sender) {
                return Err(error("bitvm_resolve sender must match resolverAddress"));
            }
            let doc = self.call(
                "BitvmCache.resolve",
                vec![
                    s(cache_id),
                    obj(&[
                        ("verdict", settlement.get("verdict")),
                        (
                            "resolverAddress",
                            proof_or(settlement.get("resolverAddress"), sender),
                        ),
                        ("reason", proof_or(settlement.get("reason"), s(""))),
                        ("block", block.clone()),
                    ]),
                ],
            )?;
            let upheld = proof_string(doc.get("status"))? == "RESOLVED_UPHELD";
            if upheld {
                let address = funding_prop(&doc, "cacheAddress")?;
                let pid = funding_prop(&doc, "propertyId")?;
                let amount = funding_prop(&doc, "amount")?;
                self.source_proof_sufficient(
                    address.clone(),
                    pid.clone(),
                    amount.clone(),
                    "bitvm uphold refund",
                )?;
                self.source_proof_transfer(
                    address,
                    funding_prop(&doc, "fromAddress")?,
                    pid.clone(),
                    amount.clone(),
                    "bitvmChallengeRefund",
                    &block,
                )?;
                self.call(
                    "BitvmRisk.onEscrowRelease",
                    vec![obj(&[
                        ("propertyId", pid),
                        ("amount", amount),
                        (
                            "dlcRef",
                            proof_or(doc.get("dlcRef"), proof_or(params.get("dlcRef"), s(""))),
                        ),
                        ("block", block.clone()),
                    ])],
                )?;
            }
            let cache_bond = doc.get("bonds").get("cacheBond");
            if truth(&cache_bond) && number(&funding_or(cache_bond.get("amount")))? > 0. {
                let amount = n(number(&cache_bond.get("amount"))?);
                let pid = n(number(&cache_bond.get("propertyId"))?);
                let vault = s(proof_string(proof_or(
                    cache_bond.get("vaultAddress"),
                    s(""),
                ))?);
                let target = if upheld {
                    let challenged = doc.get("challenged");
                    let last = if let V::Array(v) = challenged {
                        v.last().cloned().unwrap_or(V::Undefined)
                    } else {
                        V::Undefined
                    };
                    proof_or(
                        last.get("challengerAddress"),
                        proof_or(
                            doc.get("bonds").get("challengeBond").get("payerAddress"),
                            s(""),
                        ),
                    )
                } else {
                    s(proof_string(proof_or(
                        cache_bond.get("payerAddress"),
                        s(""),
                    ))?)
                };
                if truth(&target) && truth(&vault) {
                    let (label, reason) = if upheld {
                        ("bitvmCacheBondSlash", "bitvm cache bond slash")
                    } else {
                        ("bitvmCacheBondReturn", "bitvm cache bond return")
                    };
                    self.source_proof_sufficient(
                        vault.clone(),
                        pid.clone(),
                        amount.clone(),
                        reason,
                    )?;
                    self.source_proof_transfer(vault, target, pid, amount, label, &block)?;
                }
            }
            let bond = doc.get("bonds").get("challengeBond");
            if truth(&bond) && number(&funding_or(bond.get("amount")))? > 0. {
                let amount = n(number(&bond.get("amount"))?);
                let pid = n(number(&bond.get("propertyId"))?);
                let vault = s(proof_string(proof_or(bond.get("vaultAddress"), s("")))?);
                let target = s(proof_string(if upheld {
                    proof_or(bond.get("payerAddress"), s(""))
                } else {
                    proof_or(
                        doc.get("bonds").get("cacheBond").get("payerAddress"),
                        proof_or(
                            doc.get("openerAddress"),
                            proof_or(doc.get("fromAddress"), s("")),
                        ),
                    )
                })?);
                if truth(&vault) && truth(&target) {
                    let (label, reason) = if upheld {
                        ("bitvmChallengeBondReturn", "bitvm challenge bond return")
                    } else {
                        ("bitvmChallengeBondSlash", "bitvm challenge bond slash")
                    };
                    self.source_proof_sufficient(
                        vault.clone(),
                        pid.clone(),
                        amount.clone(),
                        reason,
                    )?;
                    self.source_proof_transfer(vault, target, pid, amount, label, &block)?;
                }
            }
        }
        Ok(V::Undefined)
    }
    fn source_proof_bundle(&mut self, hash: &V, path: &V) -> R<V> {
        let want = proof_string(proof_or(hash.clone(), s("")))?
            .trim_matches(funding_js_space)
            .to_lowercase();
        let bad = |reason: String| Ok(obj(&[("valid", V::Bool(false)), ("reason", s(reason))]));
        if want.is_empty() {
            return bad("Missing bundleHash".into());
        }
        let result = (|| -> R<V> {
            let path = proof_string(proof_or(
                path.clone(),
                proof_or(
                    self.source_proof_environment.get("TL_BITVM_BUNDLE_PATH"),
                    s(crate::bitvm_bundle::DEFAULT_BUNDLE_PATH),
                ),
            ))?;
            if !proof_canonical_artifact_path(&path) {
                return Err(SourceRuleError::UnknownDependency.into());
            }
            let raw = self.source_proof_artifacts.get(&path);
            let V::String(hex) = raw else {
                return Err(SourceRuleError::UnknownDependency.into());
            };
            if hex.len() > crate::bitvm_bundle::MAX_BUNDLE_BYTES * 2 {
                return Err(SourceRuleError::Capacity.into());
            }
            let bytes =
                crate::read_only::decode_hex(&hex).map_err(|_| SourceRuleError::InvalidInput)?;
            self.emit(obj(&[
                ("kind", s("artifact-read")),
                ("path", s(&path)),
                ("encoding", s("utf8")),
                ("rawHex", s(crate::read_only::hex_bytes(&bytes))),
            ]));
            let text = String::from_utf8_lossy(&bytes);
            let parsed = match parse(&text) {
                Ok(v) => v,
                // Node's parse diagnostics are runtime-version-specific; malformed
                // raw artifact JSON is an explicit local representation boundary.
                Err(Fault::Thrown(_)) => return Err(SourceRuleError::UnknownDependency.into()),
                Err(e) => return Err(e),
            };
            let declared = proof_string(proof_or(funding_prop(&parsed, "bundleHash")?, s("")))?
                .trim_matches(funding_js_space)
                .to_lowercase();
            if declared.is_empty() {
                return bad(format!("Bundle at {path} has no bundleHash"));
            }
            let computed = crate::bitvm_bundle::compute_bundle_hash(&proof_json(&parsed)?)
                .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
            if declared != computed {
                return bad(format!("Bundle hash self-check failed at {path}"));
            }
            if declared != want {
                return bad(format!(
                    "Provided bundleHash does not match artifact at {path}"
                ));
            }
            Ok(obj(&[
                ("valid", V::Bool(true)),
                ("bundlePath", s(path)),
                ("bundleHash", s(declared)),
                (
                    "selectedBucketPct",
                    n(number(&funding_or(parsed.get("selectedBucketPct")))?),
                ),
                (
                    "sourceHashes",
                    proof_or(parsed.get("sourceHashes"), obj(&[])),
                ),
            ]))
        })();
        match result {
            Err(Fault::Thrown(e)) => bad(format!("Bundle load/verify failed: {}", e.message)),
            other => other,
        }
    }
}
