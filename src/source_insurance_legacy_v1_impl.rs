// Frozen V1 fee graph, retained for actual recorded TLCR v1 replay.
impl ContractCore {
    fn legacy_source_insurance_load(&mut self, instance: &mut V) -> R<()> {
        let key = Self::source_insurance_key(
            &instance.get("contractSeriesId"),
            truth(&instance.get("oracle")),
        )?;
        self.database("insurance")?;
        let row = self.find("insurance", obj(&[("key", s(key))]))?;
        if truth(&row) && truth(&row.get("value")) {
            let value = row.get("value");
            let balances = value.get("balances");
            let hedge = value.get("hedgeRatio");
            set(
                instance,
                "balances",
                if truth(&balances) {
                    balances
                } else {
                    V::Array(vec![])
                },
            )?;
            set(
                instance,
                "hedgeRatio",
                if truth(&hedge) { hedge } else { n(0.5) },
            )?;
        }
        Ok(())
    }
    fn legacy_source_insurance_instance(&mut self, contract: V, oracle: bool) -> R<String> {
        let key = Self::source_insurance_key(&contract, oracle)?;
        if matches!(self.insurance.get(&key), V::Undefined) {
            let mut instance = obj(&[
                ("contractSeriesId", contract),
                ("balances", V::Array(vec![])),
                ("hedgeRatio", n(0.5)),
                ("oracle", V::Bool(oracle)),
            ]);
            self.legacy_source_insurance_load(&mut instance)?;
            set(&mut self.insurance, &key, instance)?;
        }
        Ok(key)
    }
    fn legacy_source_insurance_deposit(
        &mut self,
        key: &str,
        property: V,
        amount: V,
        block: V,
    ) -> R<V> {
        let mut instance = self.insurance.get(key);
        self.legacy_source_insurance_load(&mut instance)?;
        set(&mut self.insurance, key, instance.clone())?;
        let prop = if matches!(property, V::Null | V::Undefined) {
            "unknown".into()
        } else {
            text(&property)?
        };
        // Invalid/nonfinite amount forms are outside this runtime's admitted
        // finite BigNumber domain; they never become a supplied deposit.
        let amount = fee_bn0(&amount)?;
        if amount <= FeeDecimal::zero() {
            return Ok(V::Undefined);
        }
        let mut balances = match instance.get("balances") {
            V::Array(a) => a,
            _ => return Err(type_error("this.balances.find is not a function")),
        };
        if let Some(i) = balances
            .iter()
            .position(|balance| text(&balance.get("propertyId")).is_ok_and(|p| p == prop))
        {
            let before = fee_bn0(&balances[i].get("amountAvailable"))?;
            let after = fee_down(&add(&before, &amount)?, 8)?;
            set(&mut balances[i], "amountAvailable", s(after.to_string()))?;
        } else {
            balances.push(obj(&[
                ("propertyId", s(&prop)),
                ("amountAvailable", s(fee_down(&amount, 8)?.to_string())),
            ]));
        }
        set(&mut instance, "balances", V::Array(balances.clone()))?;
        set(&mut self.insurance, key, instance.clone())?;
        let contract = text(&instance.get("contractSeriesId"))?;
        let event = obj(&[
            ("type", s("deposit")),
            (
                "data",
                obj(&[
                    ("contractId", s(&contract)),
                    ("propertyId", s(prop)),
                    ("amount", s(fee_down(&amount, 8)?.to_string())),
                ]),
            ),
            ("timestamp", block.clone()),
        ]);
        self.database("insurance")?;
        self.db(
            "insurance",
            "insertAsync",
            vec![obj(&[
                ("key", s(format!("event-{}", text(&block)?))),
                ("value", event),
            ])],
        )?;
        let snapshot = obj(&[
            ("balances", V::Array(balances)),
            ("contractSeriesId", s(contract)),
            ("hedgeRatio", instance.get("hedgeRatio")),
            ("block", block),
        ]);
        // Source always calls this RPC boundary, even with a supplied snapshot.
        self.call("TxUtils.getBlockCount", vec![])?;
        self.database("insurance")?;
        self.update(
            "insurance",
            obj(&[("key", s(key))]),
            obj(&[("$set", obj(&[("value", snapshot)]))]),
            true,
        )?;
        Ok(V::Undefined)
    }
}
