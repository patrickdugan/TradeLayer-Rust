// Included in MarginMapSourceOwner's module. These are the constructor-owned
// source Maps/Sets on each existing instance, never a separate position store.
fn option_index_key(value: &V) -> Result<V, SourceRuleError> {
    primitive(value)?;
    value.validate()?;
    Ok(
        if matches!(value,V::Number(bits)if f64::from_bits(*bits)==0.0) {
            n(0.0)
        } else {
            value.clone()
        },
    )
}
fn option_index_get_map<'a>(map: &'a [(V, V)], key: &V) -> Option<&'a V> {
    map.iter().find(|(k, _)| key_eq(k, key)).map(|(_, v)| v)
}
fn option_index_set_map(map: &mut Vec<(V, V)>, key: V, value: V) -> Result<(), SourceRuleError> {
    let key = option_index_key(&key)?;
    value.validate()?;
    if let Some((_, prior)) = map.iter_mut().find(|(k, _)| key_eq(k, &key)) {
        *prior = value;
    } else {
        if map.len() >= MAX_OBJECTS {
            return Err(SourceRuleError::Capacity);
        }
        map.push((key, value));
    }
    Ok(())
}
fn option_index_map_value(map: &[(V, V)]) -> V {
    V::Array(
        map.iter()
            .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
            .collect(),
    )
}
fn option_index_expiry_value(map: &[(V, Vec<V>)]) -> V {
    V::Array(
        map.iter()
            .map(|(k, v)| V::Array(vec![k.clone(), V::Array(v.clone())]))
            .collect(),
    )
}
fn option_index_state_append(value: &mut V, instance: &Instance) -> Result<(), SourceRuleError> {
    if !instance.expiry_index.is_empty()
        || !instance.ticker_expiry.is_empty()
        || !instance.option_oi.is_empty()
    {
        value.insert(
            "expiryIndex",
            option_index_expiry_value(&instance.expiry_index),
        )?;
        value.insert(
            "tickerExpiry",
            option_index_map_value(&instance.ticker_expiry),
        )?;
        value.insert("optionOI", option_index_map_value(&instance.option_oi))?;
    }
    Ok(())
}
fn option_index_read_map(value: V) -> Result<Vec<(V, V)>, SourceRuleError> {
    let V::Array(rows) = value else {
        return Err(SourceRuleError::InvalidInput);
    };
    if rows.len() > MAX_OBJECTS {
        return Err(SourceRuleError::Capacity);
    }
    let mut out = Vec::new();
    for row in rows {
        let V::Array(pair) = row else {
            return Err(SourceRuleError::InvalidInput);
        };
        if pair.len() != 2 {
            return Err(SourceRuleError::InvalidInput);
        }
        let key = option_index_key(&pair[0])?;
        if key != pair[0] || out.iter().any(|(k, _)| key_eq(k, &key)) {
            return Err(SourceRuleError::InvalidInput);
        }
        out.push((key, pair[1].clone()));
    }
    Ok(out)
}
impl MarginMapSourceOwner {
    fn option_index_state_load(&mut self, id: u64, item: &V) -> Result<(), SourceRuleError> {
        let expiry = item.get("expiryIndex");
        let ticker = item.get("tickerExpiry");
        let oi = item.get("optionOI");
        if expiry == V::Undefined && ticker == V::Undefined && oi == V::Undefined {
            return Ok(());
        }
        let expiry = option_index_read_map(expiry)?;
        let mut sets = Vec::new();
        for (key, value) in expiry {
            let V::Array(values) = value else {
                return Err(SourceRuleError::InvalidInput);
            };
            if values.len() > MAX_OBJECTS {
                return Err(SourceRuleError::Capacity);
            }
            let mut unique = Vec::new();
            for value in values {
                let normalized = option_index_key(&value)?;
                if normalized != value || unique.iter().any(|v| key_eq(v, &value)) {
                    return Err(SourceRuleError::InvalidInput);
                }
                unique.push(value);
            }
            sets.push((key, unique));
        }
        let ticker = option_index_read_map(ticker)?;
        for (_, expiry) in &ticker {
            primitive(expiry)?;
        }
        let oi = option_index_read_map(oi)?;
        for (_, count) in &oi {
            primitive(count)?;
        }
        let instance = self.inst_mut(id)?;
        instance.expiry_index = sets;
        instance.ticker_expiry = ticker;
        instance.option_oi = oi;
        Ok(())
    }
    fn option_index_touch(&mut self, id: u64, args: &[V]) -> Eval<V> {
        let meta = arg(args, 3);
        if !truth(&meta) || !truth(&prop(&meta, "expiryBlock")?) {
            return Ok(V::Undefined);
        }
        let before = number(&arg(args, 0))?;
        let after = number(&arg(args, 1))?;
        let was_zero = before == 0.0 || before.is_nan();
        let is_zero = after == 0.0 || after.is_nan();
        if was_zero && !is_zero {
            let ticker = option_index_key(&arg(args, 2))?;
            let previous = option_index_get_map(&self.inst(id)?.option_oi, &ticker)
                .cloned()
                .unwrap_or(V::Undefined);
            let previous = if truth(&previous) { previous } else { n(0.0) };
            // The source uses JavaScript + here, unlike its numeric decrement.
            let count = if matches!(previous, V::String(_)) {
                s(&format!("{}1", text(&previous)?))
            } else {
                n(number(&previous)? + 1.0)
            };
            option_index_set_map(&mut self.inst_mut(id)?.option_oi, ticker.clone(), count)?;
            if !self
                .inst(id)?
                .ticker_expiry
                .iter()
                .any(|(k, _)| key_eq(k, &ticker))
            {
                let expiry = option_index_key(&prop(&meta, "expiryBlock")?)?;
                option_index_set_map(
                    &mut self.inst_mut(id)?.ticker_expiry,
                    ticker.clone(),
                    expiry.clone(),
                )?;
                let instance = self.inst_mut(id)?;
                if let Some((_, set)) = instance
                    .expiry_index
                    .iter_mut()
                    .find(|(k, _)| key_eq(k, &expiry))
                {
                    if !set.iter().any(|k| key_eq(k, &ticker)) {
                        if set.len() >= MAX_OBJECTS {
                            return Err(SourceRuleError::Capacity.into());
                        }
                        set.push(ticker);
                    }
                } else {
                    if instance.expiry_index.len() >= MAX_OBJECTS {
                        return Err(SourceRuleError::Capacity.into());
                    }
                    instance.expiry_index.push((expiry, vec![ticker]));
                }
            }
        } else if !was_zero && is_zero {
            let ticker = option_index_key(&arg(args, 2))?;
            let previous = option_index_get_map(&self.inst(id)?.option_oi, &ticker)
                .cloned()
                .unwrap_or(V::Undefined);
            let previous = if truth(&previous) { previous } else { n(0.0) };
            let raw = number(&previous)? - 1.0;
            let count = if raw.is_nan() { f64::NAN } else { raw.max(0.0) };
            if count == 0.0 {
                self.inst_mut(id)?
                    .option_oi
                    .retain(|(k, _)| !key_eq(k, &ticker));
                let expiry = option_index_get_map(&self.inst(id)?.ticker_expiry, &ticker)
                    .cloned()
                    .unwrap_or(V::Undefined);
                if !matches!(expiry, V::Null | V::Undefined) {
                    let instance = self.inst_mut(id)?;
                    if let Some(index) = instance
                        .expiry_index
                        .iter()
                        .position(|(k, _)| key_eq(k, &expiry))
                    {
                        let set = &mut instance.expiry_index[index].1;
                        set.retain(|k| !key_eq(k, &ticker));
                        if set.is_empty() {
                            instance.expiry_index.remove(index);
                        }
                    }
                }
                self.inst_mut(id)?
                    .ticker_expiry
                    .retain(|(k, _)| !key_eq(k, &ticker));
            } else {
                option_index_set_map(&mut self.inst_mut(id)?.option_oi, ticker, n(count))?;
            }
        }
        Ok(V::Undefined)
    }
    fn option_index_get(&self, id: u64, height: &V) -> Eval<V> {
        let mut out = Vec::new();
        for (expiry, set) in &self.inst(id)?.expiry_index {
            if number(expiry)? <= number(height)? {
                if out.len().saturating_add(set.len()) > MAX_OBJECTS {
                    return Err(SourceRuleError::Capacity.into());
                }
                out.extend(set.clone());
            }
        }
        Ok(V::Array(out))
    }
    fn option_index_cleanup(&mut self, id: u64, height: &V) -> Eval<V> {
        let entries = self.inst(id)?.expiry_index.clone();
        for (expiry, set) in entries {
            if number(&expiry)? <= number(height)? {
                let instance = self.inst_mut(id)?;
                instance.expiry_index.retain(|(k, _)| !key_eq(k, &expiry));
                for ticker in set {
                    instance.ticker_expiry.retain(|(k, _)| !key_eq(k, &ticker));
                    instance.option_oi.retain(|(k, _)| !key_eq(k, &ticker));
                }
            }
        }
        Ok(V::Undefined)
    }
}
