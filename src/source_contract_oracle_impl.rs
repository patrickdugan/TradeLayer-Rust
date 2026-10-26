// Included in source_contract_runtime.rs. Literal oracle singleton/cache and
// publish lifecycle using this Core's one shared database and failure schedule.
// Numeric boundaries use the complete verified OracleSourceOwner's helpers.
fn oracle_source_same_key(left: &V, right: &V) -> bool {
    same(left, right)
        || matches!((left,right),(V::Number(a),V::Number(b))if f64::from_bits(*a).is_nan()&&f64::from_bits(*b).is_nan())
}
fn oracle_source_map_get<'a>(map: &'a [(V, V)], key: &V) -> Option<&'a V> {
    map.iter()
        .find(|(k, _)| oracle_source_same_key(k, key))
        .map(|(_, v)| v)
}
fn oracle_source_map_set(map: &mut Vec<(V, V)>, key: V, value: V) -> R<()> {
    if matches!(
        key,
        V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise
    ) {
        return Err(SourceRuleError::InvalidInput.into());
    }
    key.validate()?;
    value.validate()?;
    let key = if matches!(&key,V::Number(bits)if f64::from_bits(*bits)==0.0) {
        n(0.0)
    } else {
        key
    };
    if let Some((_, old)) = map
        .iter_mut()
        .find(|(k, _)| oracle_source_same_key(k, &key))
    {
        *old = value;
    } else {
        if map.len() >= MAX_TABLE_ROWS {
            return Err(SourceRuleError::Capacity.into());
        }
        map.push((key, value));
    }
    Ok(())
}
impl ContractCore {
    fn oracle_source_initialize(&mut self) {
        self.oracle_source_initialized = true;
    }
    fn oracle_source_load(&mut self) -> R<V> {
        let result = (|| -> R<V> {
            self.database("oracleList")?;
            let rows = array(&self.db("oracleList", "findAsync", vec![obj(&[])])?)?;
            self.oracle_source_initialize();
            for row in rows {
                oracle_source_map_set(&mut self.oracle_source_cache, row.get("_id"), row)?;
            }
            Ok(V::Undefined)
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Undefined),
            other => other,
        }
    }
    fn oracle_source_info(&mut self, id: V) -> R<V> {
        self.oracle_source_initialize();
        if self.oracle_source_cache.is_empty() {
            self.oracle_source_load()?;
        }
        let key = s(format!("oracle-{}", text(&id)?));
        if let Some(value) =
            oracle_source_map_get(&self.oracle_source_cache, &key).filter(|v| truth(v))
        {
            return Ok(value.clone());
        }
        self.database("oracleList")?;
        self.find("oracleList", obj(&[("_id", key)]))
    }
    fn oracle_publish(&mut self, args: &[V]) -> R<V> {
        let id = arg(args, 0);
        let block = arg(args, 5);
        if let Some(last) = oracle_source_map_get(&self.oracle_source_watermarks, &id) {
            if !matches!(last, V::Undefined)
                && crate::oracle_source_owner::source_greater_equal_unverified(last, &block)?
            {
                return Ok(V::Undefined);
            }
        }
        // Deliberately precedes initialization and all DB effects. Failed
        // publication still blocks this height (and older heights) on retry.
        oracle_source_map_set(
            &mut self.oracle_source_watermarks,
            id.clone(),
            block.clone(),
        )?;
        self.oracle_source_initialize();
        let mut data = obj(&[
            ("price", arg(args, 1)),
            ("high", arg(args, 2)),
            ("low", arg(args, 3)),
            ("close", arg(args, 4)),
        ]);
        let last = self.oracle_price(id.clone())?;
        let price = outcome(crate::oracle_source_owner::source_circuit_price_unverified(
            &arg(args, 1),
            &last,
        )?)?;
        set(&mut data, "price", price)?;
        let key = s(format!("oracle-{}", text(&id)?));
        let mut metadata = oracle_source_map_get(&self.oracle_source_cache, &key)
            .filter(|v| truth(v))
            .cloned()
            .unwrap_or(V::Null);
        if !truth(&metadata) {
            self.database("oracleList")?;
            metadata = self.find("oracleList", obj(&[("_id", key.clone())]))?;
        }
        if !truth(&metadata) {
            metadata = obj(&[
                ("_id", key.clone()),
                (
                    "id",
                    n(crate::oracle_source_owner::source_number_unverified(&id)?),
                ),
            ]);
        }
        // Source spreads full metadata and changes only data/latest height.
        set(&mut metadata, "data", data.clone())?;
        set(&mut metadata, "lastPublishedBlock", block.clone())?;
        oracle_source_map_set(&mut self.oracle_source_cache, key, metadata)?;
        self.database("oracleData")?;
        let record = s(format!("oracle-{}-{}", text(&id)?, text(&block)?));
        self.db(
            "oracleData",
            "updateAsync",
            vec![
                obj(&[("_id", record.clone())]),
                obj(&[
                    ("_id", record),
                    ("type", s("oracle")),
                    ("oracleId", id),
                    ("data", data),
                    ("blockHeight", block),
                ]),
                obj(&[("upsert", V::Bool(true))]),
            ],
        )?;
        Ok(V::Undefined)
    }
    fn oracle_source_state(&self) -> V {
        let pairs = |map: &[(V, V)]| {
            V::Array(
                map.iter()
                    .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                    .collect(),
            )
        };
        obj(&[
            (
                "instance",
                if self.oracle_source_initialized {
                    obj(&[("oracles", pairs(&self.oracle_source_cache))])
                } else {
                    V::Null
                },
            ),
            (
                "lastOracleUpdateBlock",
                pairs(&self.oracle_source_watermarks),
            ),
        ])
    }
}
