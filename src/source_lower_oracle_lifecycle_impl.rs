// LowerLifecycleV7 only. The existing Core cache/collections remain the sole
// Oracle authority; these literal metadata methods introduce no Ledger.
fn lower_oracle_number(value: &V) -> R<f64> {
    match value {
        V::Object(_) | V::Array(_) => number(&s(source_channel_string(value)?)),
        _ => number(value),
    }
}
fn lower_oracle_key(id: &V) -> R<V> {
    Ok(s(format!("oracle-{}", source_channel_string(id)?)))
}
fn lower_oracle_or(value: V, fallback: V) -> V {
    if truth(&value) {
        value
    } else {
        fallback
    }
}
impl ContractCore {
    fn source_lower_oracle(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "getInstance" => {
                self.oracle_source_initialize();
                Ok(self.oracle_source_state().get("instance"))
            }
            "getNextId" => {
                self.database("oracleList")?;
                let rows = array(&self.db("oracleList", "findAsync", vec![obj(&[])])?)?;
                let mut maximum = 0.0;
                for row in rows {
                    let mut id = row.get("id");
                    if matches!(id, V::Null | V::Undefined) {
                        let key = source_channel_string(&lower_oracle_or(row.get("_id"), s("")))?;
                        id = key.split('-').nth(1).map(s).unwrap_or(V::Undefined);
                    }
                    let id = lower_oracle_number(&id)?;
                    if id.is_finite() && id > maximum {
                        maximum = id;
                    }
                }
                if maximum > 0.0 {
                    return Ok(n(maximum + 1.0));
                }
                self.call("Oracle.getInstance", vec![])?;
                for (key, _) in &self.oracle_source_cache {
                    let V::String(key) = key else {
                        return Err(type_error(if matches!(key, V::Null | V::Undefined) {
                            format!("Cannot read properties of {} (reading 'split')", text(key)?)
                        } else {
                            "key.split is not a function".into()
                        }));
                    };
                    let id =
                        registry_parse_int(&key.split('-').nth(1).map(s).unwrap_or(V::Undefined))?;
                    if id > maximum {
                        maximum = id;
                    }
                }
                Ok(n(maximum + 1.0))
            }
            "createOracle" => {
                self.call("Oracle.getInstance", vec![])?;
                let id = self.call("Oracle.getNextId", vec![])?;
                let key = lower_oracle_key(&id)?;
                let input = arg(a, 0);
                let config = if truth(&input)
                    && matches!(
                        input,
                        V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise
                    ) {
                    input
                } else {
                    obj(&[("name", input), ("adminAddress", arg(a, 1))])
                };
                let name = lower_oracle_or(
                    config.get("name"),
                    lower_oracle_or(config.get("ticker"), key.clone()),
                );
                let lag = lower_oracle_number(&config.get("lag"))?;
                let row = obj(&[
                    ("_id", key.clone()),
                    ("id", id.clone()),
                    ("name", name.clone()),
                    ("ticker", lower_oracle_or(config.get("ticker"), name)),
                    ("url", lower_oracle_or(config.get("url"), s(""))),
                    (
                        "backupAddress",
                        lower_oracle_or(config.get("backupAddress"), s("")),
                    ),
                    (
                        "clearlists",
                        if matches!(config.get("clearlists"), V::Array(_)) {
                            config.get("clearlists")
                        } else {
                            V::Array(vec![])
                        },
                    ),
                    ("lag", n(if lag.is_finite() { lag } else { 1.0 })),
                    (
                        "adminAddress",
                        lower_oracle_or(
                            config.get("adminAddress"),
                            lower_oracle_or(arg(a, 1), s("")),
                        ),
                    ),
                    ("data", obj(&[])),
                ]);
                self.database("oracleList")?;
                self.db("oracleList", "insertAsync", vec![row.clone()])?;
                oracle_source_map_set(&mut self.oracle_source_cache, key, row)?;
                Ok(id)
            }
            "isAdmin" => {
                // Source object identity cannot be supplied through by-value actor
                // arguments. This is a local admission boundary, not a JS throw.
                if matches!(
                    arg(a, 0),
                    V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise
                ) {
                    return Err(SourceRuleError::UnsupportedNumericDomain.into());
                }
                let key = lower_oracle_key(&arg(a, 1))?;
                self.database("oracleList")?;
                let row = self.find("oracleList", obj(&[("_id", key)]))?;
                let admin =
                    lower_oracle_or(row.get("adminAddress"), row.get("name").get("adminAddress"));
                let backup = lower_oracle_or(
                    row.get("backupAddress"),
                    row.get("name").get("backupAddress"),
                );
                Ok(V::Bool(
                    same(&admin, &arg(a, 0)) || same(&backup, &arg(a, 0)),
                ))
            }
            "updateAdmin" => {
                let key = lower_oracle_key(&arg(a, 0))?;
                self.call("Oracle.getInstance", vec![])?;
                self.database("oracleList")?;
                let mut row = self.find("oracleList", obj(&[("_id", key.clone())]))?;
                if !truth(&row) {
                    return Err(error("Oracle not found"));
                }
                let field = if truth(&arg(a, 2)) {
                    "backupAddress"
                } else {
                    "adminAddress"
                };
                set(&mut row, field, arg(a, 1))?;
                self.db(
                    "oracleList",
                    "updateAsync",
                    vec![
                        obj(&[("_id", key.clone())]),
                        obj(&[("$set", obj(&[(field, arg(a, 1))]))]),
                        obj(&[]),
                    ],
                )?;
                oracle_source_map_set(&mut self.oracle_source_cache, key, row)?;
                Ok(V::Undefined)
            }
            "closeOracle" => {
                self.call("Oracle.getInstance", vec![])?;
                let key = lower_oracle_key(&arg(a, 0))?;
                self.database("oracleList")?;
                let mut row = self.find("oracleList", obj(&[("_id", key.clone())]))?;
                if !truth(&row) {
                    return Err(error("Oracle not found"));
                }
                set(&mut row, "closed", V::Bool(true))?;
                self.db(
                    "oracleList",
                    "updateAsync",
                    vec![
                        obj(&[("_id", key.clone())]),
                        obj(&[("$set", obj(&[("closed", V::Bool(true))]))]),
                        obj(&[]),
                    ],
                )?;
                oracle_source_map_set(&mut self.oracle_source_cache, key, row.clone())?;
                // Actual source supplies true as contractId, omits isOracle.
                self.call(
                    "Insurance.liquidate",
                    vec![row.get("adminAddress"), V::Bool(true)],
                )?;
                Ok(V::Undefined)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_lower_oracle_logic(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "createOracle" => self.call(
                "Oracle.createOracle",
                vec![obj(&[
                    ("adminAddress", arg(a, 0)),
                    ("ticker", arg(a, 1)),
                    ("url", arg(a, 2)),
                    ("backupAddress", arg(a, 3)),
                    ("clearlists", arg(a, 4)),
                    ("lag", arg(a, 5)),
                ])],
            ),
            "publishOracleData" => {
                self.call("Oracle.publishData", (0..6).map(|i| arg(a, i)).collect())?;
                Ok(V::Undefined)
            }
            "closeOracle" => {
                self.call("Oracle.closeOracle", vec![arg(a, 0)])?;
                Ok(V::Undefined)
            }
            "updateAdmin" => {
                if truth(&arg(a, 0)) {
                    // The actual ClearList owner has not yet been composed here.
                    self.call(
                        "ClearList.updateAdmin",
                        vec![arg(a, 3), arg(a, 4), arg(a, 5)],
                    )?;
                } else if truth(&arg(a, 1)) {
                    // Literal unbound source identifier; never redirected to PropertyManager.
                    return Err(Fault::Thrown(crate::validity_source::SourceException {
                        name: "ReferenceError".into(),
                        message: "PropertyList is not defined".into(),
                    }));
                } else if truth(&arg(a, 2)) {
                    self.call("Oracle.updateAdmin", vec![arg(a, 3), arg(a, 4), arg(a, 5)])?;
                }
                Ok(V::Undefined)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
impl SourceContractRuntime {
    fn source_lower_oracle_raw_logic(
        &mut self,
        kind: &V,
        p: &V,
    ) -> Result<SourceOutcome, SourceRuleError> {
        let (method, arguments) = if *kind == n(13.) {
            (
                "createOracle",
                vec![
                    p.get("senderAddress"),
                    p.get("ticker"),
                    p.get("url"),
                    p.get("backupAddress"),
                    p.get("clearlists"),
                    p.get("lag"),
                    p.get("oracleRegistry"),
                    p.get("block"),
                ],
            )
        } else if *kind == n(14.) {
            (
                "publishOracleData",
                vec![
                    p.get("oracleId"),
                    p.get("price"),
                    p.get("high"),
                    p.get("low"),
                    p.get("close"),
                    p.get("block"),
                ],
            )
        } else if *kind == n(15.) {
            (
                "closeOracle",
                vec![p.get("oracleId"), p.get("oracleRegistry"), p.get("block")],
            )
        } else {
            (
                "updateAdmin",
                vec![
                    p.get("whitelist"),
                    p.get("token"),
                    p.get("oracle"),
                    p.get("id"),
                    p.get("newAddress"),
                    p.get("updateBackup"),
                    p.get("block"),
                ],
            )
        };
        match self.invoke_dependency(&DependencyCall {
            target: format!("Logic.{method}"),
            arguments,
        })? {
            DependencyReturn::Immediate(_) | DependencyReturn::Pending(_) => {
                Ok(SourceOutcome::Returned(V::Undefined))
            }
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                Ok(SourceOutcome::Thrown(e))
            }
        }
    }
}
