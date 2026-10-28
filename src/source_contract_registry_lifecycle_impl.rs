// Included in source_contract_runtime.rs. Actual pinned registry definitions;
// the existing Core DB/cache are the only metadata and balance authorities.
fn registry_field(value: &V, key: &str) -> R<V> {
    if matches!(value, V::Undefined | V::Null) {
        return Err(type_error(format!(
            "Cannot read properties of {} (reading '{key}')",
            text(value)?
        )));
    }
    Ok(value.get(key))
}
fn registry_index(value: &V, index: usize) -> R<V> {
    match value {
        V::Array(values) => Ok(arg(values, index)),
        V::String(value) => match value.encode_utf16().nth(index) {
            None => Ok(V::Undefined),
            Some(unit) => Ok(s(String::from_utf16(&[unit])
                .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?)),
        },
        _ => registry_field(value, &index.to_string()),
    }
}
fn registry_equal(a: &V, b: &V) -> R<bool> {
    if matches!(a, V::Null | V::Undefined) || matches!(b, V::Null | V::Undefined) {
        return Ok(matches!(a, V::Null | V::Undefined) && matches!(b, V::Null | V::Undefined));
    }
    if matches!((a, b), (V::String(_), V::String(_))) {
        return Ok(same(a, b));
    }
    if matches!(
        (a, b),
        (V::Object(_) | V::Array(_), V::Object(_) | V::Array(_))
    ) {
        return Ok(false);
    }
    if matches!((a, b), (V::Object(_) | V::Array(_), V::String(_))) {
        return Ok(text(a)? == text(b)?);
    }
    if matches!((a, b), (V::String(_), V::Object(_) | V::Array(_))) {
        return Ok(text(a)? == text(b)?);
    }
    Ok(number(a)? == number(b)?)
}
fn registry_parse_int(value: &V) -> R<f64> {
    let value = text(value)?;
    let value = value.trim_start_matches(funding_js_space);
    let (negative, value) = if let Some(v) = value.strip_prefix('-') {
        (true, v)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    let (radix, digits) = if value.starts_with("0x") || value.starts_with("0X") {
        (16, &value[2..])
    } else {
        (10, value)
    };
    let mut accumulated = 0u128;
    let mut count = 0;
    for digit in digits.chars().map_while(|v| v.to_digit(radix)) {
        accumulated = accumulated
            .checked_mul(u128::from(radix))
            .and_then(|v| v.checked_add(u128::from(digit)))
            .ok_or(SourceRuleError::UnsupportedNumericDomain)?;
        count += 1;
    }
    if count == 0 {
        return Ok(f64::NAN);
    }
    let value = accumulated as f64;
    Ok(if negative { -value } else { value })
}
fn registry_map(value: &V) -> R<Vec<(V, V)>> {
    let mut map = Vec::new();
    for entry in array(value)? {
        oracle_source_map_set(
            &mut map,
            registry_index(&entry, 0)?,
            registry_index(&entry, 1)?,
        )?;
    }
    Ok(map)
}
fn registry_next_id(map: &[(V, V)]) -> R<V> {
    let mut maximum = 0.0;
    for (key, _) in map {
        let value = registry_parse_int(key)?;
        if value > maximum {
            maximum = value;
        }
    }
    Ok(n(maximum + 1.0))
}
fn registry_generate(a: &[V]) -> R<V> {
    let period = registry_parse_int(&arg(a, 0))?;
    let count = number(&arg(a, 1))?;
    if count > MAX_TABLE_ROWS as f64 {
        return Err(SourceRuleError::Capacity.into());
    }
    let mut expiration = registry_parse_int(&arg(a, 3))? + period;
    let mut generated = Vec::new();
    let mut index = 0.0;
    while index < count {
        generated.push(obj(&[
            (
                "id",
                s(format!("{}-{}", text(&arg(a, 2))?, text(&n(expiration))?)),
            ),
            ("expirationBlock", n(expiration)),
        ]));
        expiration += period;
        index += 1.0;
    }
    Ok(V::Array(generated))
}
impl ContractCore {
    fn source_registry_logic_create(&mut self, a: &[V]) -> R<V> {
        let fields = [
            "sender",
            "native",
            "underlyingOracleId",
            "onChainData",
            "notionalPropertyId",
            "notionalValue",
            "collateralPropertyId",
            "leverage",
            "expiryPeriod",
            "series",
            "inverse",
            "fee",
            "block",
            "whitelist",
        ];
        let mut params = obj(&[]);
        for (index, field) in fields.iter().enumerate() {
            set(&mut params, field, arg(a, index))?;
        }
        self.source_registry_create(arg(a, 0), &mut params, arg(a, 12))
    }
    fn source_registry_save(&mut self, map: &[(V, V)], kind: V) -> R<V> {
        self.database("contractList")?;
        for (id, data) in map {
            self.update(
                "contractList",
                obj(&[("id", id.clone())]),
                obj(&[
                    ("id", id.clone()),
                    ("data", data.clone()),
                    ("type", kind.clone()),
                ]),
                true,
            )?;
        }
        Ok(V::Undefined)
    }
    fn source_registry_create(&mut self, sender: V, params: &mut V, block: V) -> R<V> {
        self.database("contractList")?;
        let rows = array(&self.db(
            "contractList",
            "findAsync",
            vec![obj(&[("type", s("contractSeries"))])],
        )?)?;
        let mut map = Vec::new();
        for row in rows {
            oracle_source_map_set(&mut map, row.get("id"), row.get("data"))?;
        }
        let id = registry_next_id(&map)?;
        let amm = obj(&[
            ("position", n(0.0)),
            ("maxPosition", n(1.0)),
            ("maxQuoteSize", n(10.0)),
            ("contractType", id.clone()),
            ("lpAddresses", obj(&[])),
            ("ammOrders", V::Array(vec![])),
        ]);
        if self.amm_lifecycle_enabled() {
            self.source_amm_insert(amm.clone())?;
        }
        if matches!(registry_field(params, "whitelist")?, V::Undefined | V::Null) {
            set(params, "whitelist", n(0.0))?;
        }
        if truth(&params.get("native")) {
            let mut notional = self.call(
                "Property.getPropertyData",
                vec![params.get("notionalPropertyId")],
            )?;
            let collateral = self.call(
                "Property.getPropertyData",
                vec![params.get("collateralPropertyId")],
            )?;
            if registry_equal(&params.get("notionalPropertyId"), &n(0.0))? {
                notional = obj(&[("ticker", s("LTC"))]);
            }
            set(
                params,
                "ticker",
                s(format!(
                    "{}/{}-PERP",
                    text(&registry_field(&notional, "ticker")?)?,
                    text(&registry_field(&collateral, "ticker")?)?
                )),
            )?;
        } else if !registry_equal(&params.get("underlyingOracleId"), &n(0.0))? {
            let oracle = self.call(
                "Oracle.getOracleInfo",
                vec![params.get("underlyingOracleId")],
            )?;
            let name = registry_field(&oracle, "name")?;
            let ticker = registry_field(&name, "ticker")?;
            set(
                params,
                "ticker",
                s(format!(
                    "{}-OPERP{}-{}",
                    text(&ticker)?,
                    text(&params.get("underlyingOracleId"))?,
                    text(&id)?
                )),
            )?;
        }
        let mut definition = obj(&[
            ("id", id.clone()),
            (
                "ticker",
                if truth(&params.get("ticker")) {
                    params.get("ticker")
                } else {
                    s(format!("{}-PERP", text(&id)?))
                },
            ),
            ("issuer", sender),
        ]);
        for field in [
            "native",
            "underlyingOracleId",
            "onChainData",
            "notionalPropertyId",
            "notionalValue",
            "collateralPropertyId",
            "leverage",
            "expiryPeriod",
            "series",
            "inverse",
            "fee",
            "whitelist",
        ] {
            set(&mut definition, field, params.get(field))?;
        }
        let children = registry_generate(&[
            params.get("expiryPeriod"),
            params.get("series"),
            id.clone(),
            block,
        ])?;
        set(
            &mut definition,
            "contracts",
            obj(&[("expired", V::Array(vec![])), ("unexpired", children)]),
        )?;
        set(&mut definition, "ammPool", amm)?;
        oracle_source_map_set(&mut map, id.clone(), definition)?;
        self.source_registry_save(&map, s("contractSeries"))?;
        Ok(id)
    }
    fn source_registry_duplicate(&mut self, a: &[V]) -> R<V> {
        let result = (|| -> R<V> {
            self.contract_registry_initialized = true;
            if !self.contract_series_initialized {
                self.call("Contract.loadContractSeries", vec![])?;
            }
            if !self.contract_series_initialized {
                return Err(type_error("contractSeries is not iterable"));
            }
            let requested = arg(a, 1);
            for (_, definition) in &self.contract_series {
                if truth(&registry_field(definition, "native")?)
                    && same(&definition.get("collateralPropertyId"), &arg(a, 0))
                {
                    for pair in array(&definition.get("onChainData"))? {
                        let a0 = registry_index(&pair, 0)?;
                        let a1 = registry_index(&pair, 1)?;
                        let b0 = registry_index(&requested, 0)?;
                        let b1 = registry_index(&requested, 1)?;
                        if (same(&a0, &b0) && same(&a1, &b1))
                            || (same(&a0, &b1)
                                && same(&a1, &b0)
                                && registry_equal(
                                    &definition.get("notionalPropertyId"),
                                    &arg(a, 2),
                                )?)
                        {
                            return Ok(V::Bool(true));
                        }
                    }
                }
            }
            Ok(V::Bool(false))
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Bool(false)),
            other => other,
        }
    }
    fn source_registry_lifecycle_call(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "createContractSeries" => {
                self.source_registry_create(arg(a, 0), &mut arg(a, 1), arg(a, 2))
            }
            "getNextIdFromMap" => registry_next_id(&registry_map(&arg(a, 0))?),
            "generateContracts" => registry_generate(a),
            "saveDataToDb" => self.source_registry_save(&registry_map(&arg(a, 0))?, arg(a, 1)),
            "isDuplicateNativeContract" => self.source_registry_duplicate(a),
            "getContractSeries" => {
                self.database("contractList")?;
                let row = self.find(
                    "contractList",
                    obj(&[("id", arg(a, 0)), ("type", s("contractSeries"))]),
                )?;
                Ok(if truth(&row) {
                    row.get("data")
                } else {
                    V::Null
                })
            }
            "getNextId" => {
                self.database("contractList")?;
                let rows = array(&self.db(
                    "contractList",
                    "findAsync",
                    vec![obj(&[("type", s("contractSeries"))])],
                )?)?;
                let mut maximum: f64 = 0.0;
                for row in rows {
                    let value = registry_parse_int(&row.get("id"))?;
                    maximum = if maximum.is_nan() || value.is_nan() {
                        f64::NAN
                    } else {
                        maximum.max(value)
                    };
                }
                Ok(n(maximum + 1.0))
            }
            "getAllContractsForCollateral" => {
                let all = array(&self.call("Contract.getAllContracts", vec![])?)?;
                let mut found = Vec::new();
                for definition in all {
                    if same(
                        &registry_field(&definition, "collateralPropertyId")?,
                        &arg(a, 1),
                    ) {
                        found.push(definition.get("id"));
                    }
                }
                Ok(V::Array(found))
            }
            "lookupInverseNativeByNotionalPid" => {
                let all = array(&self.call("Contract.getAllContracts", vec![])?)?;
                let mut found = Vec::new();
                for definition in all {
                    if !truth(&definition) || !matches!(definition, V::Object(_) | V::Array(_)) {
                        continue;
                    }
                    let id = definition.get("id");
                    if !truth(&id) {
                        continue;
                    }
                    let inverse = self.call("Contract.isInverse", vec![id.clone()])?;
                    let native = self.call("Contract.isNativeContract", vec![id.clone()])?;
                    if !truth(&inverse)
                        || !truth(&native)
                        || number(&definition.get("collateralPropertyId"))? != number(&arg(a, 0))?
                    {
                        continue;
                    }
                    let symbol = [
                        definition.get("symbol"),
                        definition.get("ticker"),
                        definition.get("name"),
                    ]
                    .into_iter()
                    .find(truth)
                    .unwrap_or(s(format!("contract-{}", text(&id)?)));
                    found.push(obj(&[
                        ("contractId", id.clone()),
                        ("seriesId", id),
                        ("symbol", symbol),
                        (
                            "notionalPropertyId",
                            n(number(&definition.get("notionalPropertyId"))?),
                        ),
                    ]));
                }
                Ok(V::Array(found))
            }
            "getContractType" => {
                let info = self.call("Contract.getContractInfo", vec![arg(a, 0)])?;
                if !truth(&info) {
                    return Err(error(format!(
                        "Contract type not found for contract ID: {}",
                        text(&arg(a, 0))?
                    )));
                }
                Ok(s(if truth(&info.get("native")) {
                    "native"
                } else {
                    "oracle"
                }))
            }
            "getAMM" => {
                let info = self.call("Contract.getContractInfo", vec![arg(a, 0)])?;
                let pool = info.get("ammPool");
                if !truth(&info) || !truth(&pool) {
                    return Err(error(format!(
                        "AMM object not found for contract ID {}",
                        text(&arg(a, 0))?
                    )));
                }
                let mut hydrated = obj(&[]);
                for (field, fallback) in [
                    ("position", n(0.0)),
                    ("maxPosition", n(1.0)),
                    ("maxQuoteSize", n(10.0)),
                    ("contractType", arg(a, 0)),
                ] {
                    let value = pool.get(field);
                    set(
                        &mut hydrated,
                        field,
                        n(if self.amm_lifecycle_enabled() {
                            amm_quote_number(&if truth(&value) { value } else { fallback })?
                        } else {
                            number(&if truth(&value) { value } else { fallback })?
                        }),
                    )?;
                }
                let lp = pool.get("lpAddresses");
                let fields = match lp {
                    V::Object(fields) => fields,
                    V::Array(values) => values
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| (i.to_string(), v))
                        .collect(),
                    V::String(value) => value
                        .chars()
                        .enumerate()
                        .map(|(i, v)| (i.to_string(), s(v.to_string())))
                        .collect(),
                    _ => vec![],
                };
                set(&mut hydrated, "lpAddresses", V::Object(fields))?;
                set(
                    &mut hydrated,
                    "ammOrders",
                    if matches!(pool.get("ammOrders"), V::Array(_)) {
                        pool.get("ammOrders")
                    } else {
                        V::Array(vec![])
                    },
                )?;
                Ok(hydrated)
            }
            "isValidSeriesId" => {
                self.contract_registry_initialized = true;
                Ok(V::Bool(false))
            }
            "saveAllData" => {
                self.contract_registry_initialized = true;
                for kind in ["contractSeries", "oracleContracts", "nativeContracts"] {
                    self.source_registry_save(&[], s(kind))?;
                }
                Ok(V::Undefined)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_registry_property_data(&mut self, a: &[V]) -> R<V> {
        let result = (|| -> R<V> {
            self.database("propertyList")?;
            let row = self.find("propertyList", obj(&[("_id", s("propertyIndex"))]))?;
            if !truth(&row) || !truth(&row.get("value")) {
                return Ok(V::Null);
            }
            let parsed = parse(&text(&row.get("value"))?)?;
            let entries = array(&parsed)?;
            let id = arg(a, 0);
            if matches!(&id,V::String(v)if v.starts_with('s')) {
                for entry in &entries {
                    if same(&registry_index(entry, 0)?, &id) {
                        return registry_index(entry, 1);
                    }
                }
            }
            let requested = number(&id)?;
            for entry in entries {
                if number(&registry_index(&entry, 0)?)? == requested {
                    return registry_index(&entry, 1);
                }
            }
            Ok(V::Null)
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Null),
            other => other,
        }
    }
    fn source_registry_last_price(&mut self, a: &[V]) -> R<V> {
        let result = (|| -> R<V> {
            self.database("volumeIndex")?;
            let V::String(pair) = arg(a, 0) else {
                return Ok(V::Null);
            };
            if !pair.contains('-') {
                return Ok(V::Null);
            }
            let direct = self.find("volumeIndex", obj(&[("_id", s(&pair))]))?;
            let price = direct.get("value").get("price");
            if truth(&price) {
                return Ok(price);
            }
            let mut parts = pair.split('-');
            let first = parts.next().unwrap_or("undefined");
            let second = parts.next().unwrap_or("undefined");
            let inverse = self.find(
                "volumeIndex",
                obj(&[("_id", s(format!("{second}-{first}")))]),
            )?;
            let price = inverse.get("value").get("price");
            if truth(&price) {
                return Ok(n(FundingBn::new(&n(1.0))?
                    .op(FundingBn::new(&price)?, 3)?
                    .number()));
            }
            Ok(V::Null)
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Null),
            other => other,
        }
    }
}
