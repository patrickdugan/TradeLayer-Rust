// Future ProofLifecycleV6 only. The existing Core owns every row and balance.
// Oracle action methods are stateless; their verified source math is reused.
fn proof_json(value: &V) -> R<crate::issuance_intent::JsonValue> {
    fn supported(v: &V) -> bool {
        match v {
            V::Null | V::Bool(_) | V::String(_) => true,
            V::Number(bits) => f64::from_bits(*bits).is_finite(),
            V::Array(values) => values.iter().all(supported),
            V::Object(fields) => fields.iter().all(|(_, v)| supported(v)),
            _ => false,
        }
    }
    if !supported(value) {
        return Err(SourceRuleError::UnsupportedNumericDomain.into());
    }
    crate::issuance_intent::JsonValue::parse(&json(value)?)
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain.into())
}
fn proof_dlc_error(error: crate::dlc_oracle_bridge::DlcBridgeError) -> Fault {
    use crate::dlc_oracle_bridge::DlcBridgeError as E;
    match error {
        E::Capacity => SourceRuleError::Capacity.into(),
        E::SourceTypeError(message) => type_error(message),
        _ => SourceRuleError::UnsupportedNumericDomain.into(),
    }
}
fn proof_relay_parsed(value: &V) -> R<V> {
    if !truth(value) {
        return Ok(V::Null);
    }
    let text = match source_channel_string(value) {
        Ok(text) => text,
        Err(Fault::Thrown(_)) => return Ok(V::Null),
        Err(error) => return Err(error),
    };
    crate::dlc_oracle_bridge::source_parse_relay_blob(&crate::issuance_intent::JsonValue::String(
        text,
    ))
    .map_err(proof_dlc_error)?
    .map_or(Ok(V::Null), |v| V::from_json(&v).map_err(Fault::Local))
}
fn proof_raw_relay(value: &V) -> R<V> {
    if !truth(value) {
        return Ok(V::Null);
    }
    let mut raw = match source_channel_string(value) {
        Ok(text) => text,
        Err(Fault::Thrown(_)) => return Ok(V::Null),
        Err(error) => return Err(error),
    };
    if matches!(value,V::String(text)if text.starts_with("b64:")) {
        // Same bounded Node-compatible base64 bytes as the existing bridge;
        // Logic's raw parser must not normalize away settlement fields.
        let mut bytes = Vec::new();
        let (mut acc, mut bits) = (0u32, 0u32);
        for unit in raw[4..].encode_utf16() {
            let byte = unit as u8;
            if byte == b'=' {
                break;
            }
            let digit = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' | b'-' => 62,
                b'/' | b'_' => 63,
                _ => continue,
            };
            acc = (acc << 6) | u32::from(digit);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                if bytes.len() == crate::dlc_oracle_bridge::MAX_PAYLOAD_BYTES {
                    return Err(SourceRuleError::Capacity.into());
                }
                bytes.push((acc >> bits) as u8);
            }
        }
        raw = String::from_utf8_lossy(&bytes).into_owned();
    }
    match parse(&raw) {
        Ok(value) => Ok(value),
        Err(Fault::Thrown(_)) => Ok(V::Null),
        Err(error) => Err(error),
    }
}
impl ContractCore {
    fn source_proof_dlc(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "parseRelayBlob" => proof_relay_parsed(&arg(a, 0)),
            "validateRelayBundle" => {
                let bundle = arg(a, 0);
                if !matches!(bundle, V::Object(_) | V::Array(_)) {
                    return Ok(obj(&[
                        ("valid", V::Bool(false)),
                        ("reason", s("Missing relay bundle")),
                    ]));
                }
                let bundle = proof_json(&bundle)?;
                let expected = if arg(a, 1) == V::Undefined {
                    None
                } else {
                    Some(proof_json(&arg(a, 1))?)
                };
                let verdict = crate::dlc_oracle_bridge::source_validate_relay_bundle(
                    &bundle,
                    expected.as_ref(),
                )
                .map_err(proof_dlc_error)?;
                parse(&verdict.to_json())
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_proof_oracle(&mut self, method: &str, a: &[V]) -> R<V> {
        // Preserve getDatabase before source key/coercion failures.
        self.database("oracleData")?;
        let id = arg(a, 0);
        match method {
            "getStake" => {
                let key = s(format!(
                    "oracle-stake-{}-{}",
                    source_channel_string(&id)?,
                    source_channel_string(&arg(a, 1))?
                ));
                self.find("oracleData", obj(&[("_id", key)]))
            }
            "recordStake" => {
                let key = s(format!(
                    "oracle-stake-{}-{}",
                    source_channel_string(&id)?,
                    source_channel_string(&arg(a, 1))?
                ));
                let previous = self.find("oracleData", obj(&[("_id", key.clone())]))?;
                let prior = number(&funding_or(previous.get("amount")))?;
                let amount = FundingBn::new(&n(prior))?
                    .op(FundingBn::new(&arg(a, 3))?, 0)?
                    .q(8, FundingRounding::Down)?
                    .number();
                let doc = obj(&[
                    ("_id", key.clone()),
                    ("type", s("stake")),
                    ("oracleId", id),
                    ("stakerAddress", arg(a, 1)),
                    ("stakedPropertyId", arg(a, 2)),
                    ("amount", n(amount)),
                    ("blockHeight", arg(a, 4)),
                ]);
                self.update(
                    "oracleData",
                    obj(&[("_id", key)]),
                    obj(&[("$set", doc.clone())]),
                    true,
                )?;
                Ok(doc)
            }
            "applyFraudProof" => {
                let key = s(format!(
                    "oracle-stake-{}-{}",
                    source_channel_string(&id)?,
                    source_channel_string(&arg(a, 1))?
                ));
                let previous = self.find("oracleData", obj(&[("_id", key.clone())]))?;
                if !truth(&previous) {
                    return Ok(obj(&[("slashed", n(0.))]));
                }
                let current = number(&funding_or(funding_prop(&previous, "amount")?))?;
                let wanted = number(&funding_or(arg(a, 3)))?;
                let slash = if current.is_nan() || wanted.is_nan() {
                    f64::NAN
                } else if current == 0. && wanted == 0. {
                    if current.is_sign_negative() || wanted.is_sign_negative() {
                        -0.
                    } else {
                        0.
                    }
                } else {
                    current.min(wanted)
                };
                let next = FundingBn::new(&n(current))?
                    .op(FundingBn::new(&n(slash))?, 1)?
                    .q(8, FundingRounding::Down)?
                    .number();
                self.update(
                    "oracleData",
                    obj(&[("_id", key)]),
                    obj(&[(
                        "$set",
                        obj(&[("amount", n(next)), ("blockHeight", arg(a, 5))]),
                    )]),
                    true,
                )?;
                let evidence =
                    source_channel_string(&if truth(&arg(a, 4)) { arg(a, 4) } else { s("") })?;
                let prefix =
                    String::from_utf16(&evidence.encode_utf16().take(24).collect::<Vec<_>>())
                        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                let key = s(format!(
                    "oracle-fraud-{}-{}-{prefix}",
                    source_channel_string(&id)?,
                    source_channel_string(&arg(a, 5))?
                ));
                let doc = obj(&[
                    ("_id", key.clone()),
                    ("type", s("fraudProof")),
                    ("oracleId", id),
                    ("accusedAddress", arg(a, 1)),
                    ("challengerAddress", arg(a, 2)),
                    ("slashAmount", n(slash)),
                    ("evidenceHash", arg(a, 4)),
                    ("blockHeight", arg(a, 5)),
                ]);
                self.update(
                    "oracleData",
                    obj(&[("_id", key)]),
                    obj(&[("$set", doc)]),
                    true,
                )?;
                Ok(obj(&[("slashed", n(slash))]))
            }
            "relayTradeLayerState" => {
                let blob = if arg(a, 6) == V::Undefined {
                    s("")
                } else {
                    arg(a, 6)
                };
                let parsed = self.call("Dlc.parseRelayBlob", vec![blob.clone()])?;
                let signature = source_channel_string(&if truth(&parsed.get("signatureHex")) {
                    parsed.get("signatureHex")
                } else {
                    s("")
                })?
                .trim_matches(funding_js_space)
                .to_lowercase();
                if !signature.is_ascii() {
                    return Err(SourceRuleError::UnsupportedNumericDomain.into());
                }
                let state = source_channel_string(&if truth(&arg(a, 3)) {
                    arg(a, 3)
                } else if truth(&parsed.get("stateHash")) {
                    parsed.get("stateHash")
                } else {
                    s("")
                })?;
                let dlc =
                    source_channel_string(&if truth(&arg(a, 4)) { arg(a, 4) } else { s("") })?;
                if !signature.is_empty() {
                    let digest = crate::hash256::sha256(signature.as_bytes())
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    let key = s(format!(
                        "oracle-relay-sig-{}-{digest}",
                        source_channel_string(&id)?
                    ));
                    let previous = self.find("oracleData", obj(&[("_id", key.clone())]))?;
                    if truth(&previous) {
                        let old_state = source_channel_string(&if truth(&funding_prop(
                            &previous,
                            "stateHash",
                        )?) {
                            funding_prop(&previous, "stateHash")?
                        } else {
                            s("")
                        })?;
                        let old_dlc = source_channel_string(&if truth(&funding_prop(
                            &previous, "dlcRef",
                        )?) {
                            funding_prop(&previous, "dlcRef")?
                        } else {
                            s("")
                        })?;
                        if old_state != state || old_dlc != dlc {
                            return Err(error(
                                "Relay signature replay detected for different stateHash/dlcRef",
                            ));
                        }
                    } else {
                        let doc = obj(&[
                            ("_id", key.clone()),
                            ("type", s("relaySigUse")),
                            ("oracleId", id.clone()),
                            ("signatureHex", s(signature)),
                            ("stateHash", s(state)),
                            ("dlcRef", s(dlc)),
                            ("firstSeenBlock", arg(a, 5)),
                        ]);
                        self.update("oracleData", obj(&[("_id", key)]), doc, true)?;
                    }
                }
                let key = s(format!(
                    "oracle-relay-{}-{}-{}",
                    source_channel_string(&id)?,
                    source_channel_string(&arg(a, 2))?,
                    source_channel_string(&arg(a, 5))?
                ));
                let doc = obj(&[
                    ("_id", key.clone()),
                    ("type", s("relay")),
                    ("oracleId", id),
                    ("senderAddress", arg(a, 1)),
                    ("relayType", arg(a, 2)),
                    ("stateHash", arg(a, 3)),
                    ("dlcRef", arg(a, 4)),
                    ("relayBlob", blob),
                    ("blockHeight", arg(a, 5)),
                ]);
                self.update("oracleData", obj(&[("_id", key)]), doc.clone(), true)?;
                Ok(doc)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_proof_logic(&mut self, method: &str, a: &[V]) -> R<V> {
        let sender = arg(a, 0);
        let params = arg(a, 1);
        let block = arg(a, 2);
        if method == "applyProceduralOracleSettlement" {
            return self.source_proof_settlement(sender, params, block);
        }
        if method != "processStakeFraudProof" {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        let action = number(&funding_or(funding_prop(&params, "action")?))?;
        if action == 0. {
            let amount = funding_prop(&params, "amount")?;
            self.call(
                "Tally.updateBalance",
                vec![
                    sender.clone(),
                    funding_prop(&params, "stakedPropertyId")?,
                    n(-number(&amount)?),
                    n(0.),
                    amount,
                    n(0.),
                    s("oracleStake"),
                    block.clone(),
                ],
            )?;
            self.call(
                "Oracle.recordStake",
                vec![
                    funding_prop(&params, "oracleId")?,
                    sender,
                    funding_prop(&params, "stakedPropertyId")?,
                    funding_prop(&params, "amount")?,
                    block,
                ],
            )?;
            return Ok(V::Undefined);
        }
        if action == 1. {
            let result = self.call(
                "Oracle.applyFraudProof",
                vec![
                    funding_prop(&params, "oracleId")?,
                    funding_prop(&params, "accusedAddress")?,
                    sender.clone(),
                    funding_prop(&params, "amount")?,
                    funding_prop(&params, "evidenceHash")?,
                    block.clone(),
                ],
            )?;
            if truth(&result) && number(&funding_or(funding_prop(&result, "slashed")?))? > 0. {
                self.call(
                    "Tally.updateBalance",
                    vec![
                        sender,
                        funding_prop(&params, "stakedPropertyId")?,
                        funding_prop(&result, "slashed")?,
                        n(0.),
                        n(0.),
                        n(0.),
                        s("oracleFraudReward"),
                        block,
                    ],
                )?;
            }
            return Ok(V::Undefined);
        }
        if action == 2. {
            self.call(
                "Oracle.relayTradeLayerState",
                vec![
                    funding_prop(&params, "oracleId")?,
                    sender.clone(),
                    funding_prop(&params, "relayType")?,
                    funding_prop(&params, "stateHash")?,
                    funding_prop(&params, "dlcRef")?,
                    block.clone(),
                    funding_prop(&params, "relayBlob")?,
                ],
            )?;
            self.call(
                "Logic.applyProceduralOracleSettlement",
                vec![sender, params.clone(), block.clone()],
            )?;
            if truth(&funding_prop(&params, "dlcRef")?)
                && truth(&funding_prop(&params, "settlementState")?)
            {
                self.call(
                    "Procedural.transitionContract",
                    vec![
                        params.get("dlcRef"),
                        params.get("settlementState"),
                        obj(&[
                            ("oracleId", params.get("oracleId")),
                            ("stateHash", params.get("stateHash")),
                            ("blockHeight", block.clone()),
                        ]),
                    ],
                )?;
                if truth(&params.get("autoRoll")) && truth(&params.get("nextDlcRef")) {
                    self.call(
                        "Procedural.transitionContract",
                        vec![
                            params.get("nextDlcRef"),
                            s("FUNDED"),
                            obj(&[
                                ("rolledFrom", params.get("dlcRef")),
                                ("oracleId", params.get("oracleId")),
                                ("stateHash", params.get("stateHash")),
                                ("blockHeight", block),
                            ]),
                        ],
                    )?;
                }
            }
            return Ok(V::Undefined);
        }
        Err(error(format!(
            "Unknown stake/fraud/relay action {}",
            source_channel_string(&n(action))?
        )))
    }
}
impl SourceContractRuntime {
    fn source_proof_raw_logic(&mut self, p: &V) -> Result<SourceOutcome, SourceRuleError> {
        match self.invoke_dependency(&DependencyCall {
            target: "Logic.processStakeFraudProof".into(),
            arguments: vec![p.get("senderAddress"), p.clone(), p.get("block")],
        })? {
            DependencyReturn::Immediate(_) | DependencyReturn::Pending(_) => {
                Ok(SourceOutcome::Returned(V::Undefined))
            }
            DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error) => {
                Ok(SourceOutcome::Thrown(error))
            }
        }
    }
}
