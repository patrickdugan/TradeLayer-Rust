// Included inside contract_queue_source_owner.rs. Separate opt-in entry point:
// the original nine-method dispatch and checkpoint envelope remain unchanged.
impl ContractQueueSourceOwner {
    pub fn evaluate_channel_with_capability<F>(
        &mut self,
        call: &SourceCall,
        capability: &mut F,
    ) -> Result<SourceOutcome, SourceRuleError>
    where
        F: FnMut(&mut Self, &DependencyCall) -> Result<DependencyReturn, SourceRuleError>,
    {
        if !matches!(
            call.method.as_str(),
            "queueChannelTrade" | "processQueuedChannelTradesForBlock"
        ) {
            return Err(SourceRuleError::UnknownMethod);
        }
        if call.arguments.len() > 8 {
            return Err(SourceRuleError::Capacity);
        }
        for value in &call.arguments {
            value.validate()?;
        }
        let before = self.clone();
        let result = if call.method == "queueChannelTrade" {
            self.channel_queue(&call.arguments)
        } else {
            self.process_channel_queue(arg(&call.arguments, 0), capability)
        };
        if let Err(Fault::Local(e)) = &result {
            let error = e.clone();
            *self = before;
            return Err(error);
        }
        if let Err(error) = self.checkpoint() {
            *self = before;
            return Err(error);
        }
        match result {
            Ok(value) => Ok(SourceOutcome::Returned(value)),
            Err(Fault::Thrown(error)) => Ok(SourceOutcome::Thrown(error)),
            Err(Fault::Local(error) | Fault::Boundary(error)) => Err(error),
        }
    }

    fn channel_queue(&mut self, a: &[V]) -> Eval<V> {
        self.database()?;
        let pair = text(&arg(a, 1))?;
        let key = s(&format!("channel-queue-{pair}"));
        let row = self.find(key.clone())?;
        let mut trades = match row.get("trades") {
            V::Array(a) => a,
            _ => vec![],
        };
        trades.push(obj(&[
            ("kind", arg(a, 0)),
            ("pairKey", s(&pair)),
            ("sender", arg(a, 2)),
            ("blockHeight", V::number(number(&arg(a, 4))?)),
            ("txid", arg(a, 5)),
            ("match", arg(a, 3)),
        ]));
        self.update(
            key.clone(),
            obj(&[("_id", key), ("trades", V::Array(trades))]),
        )?;
        self.reload()?;
        self.active_pair(s(&format!("channel-{pair}")))
    }

    fn process_channel_queue<F>(&mut self, block: V, capability: &mut F) -> Eval<V>
    where
        F: FnMut(&mut Self, &DependencyCall) -> Result<DependencyReturn, SourceRuleError>,
    {
        let height = number(&block)?;
        self.database()?;
        let doc = self.find(s("activePairs"))?;
        let pairs = doc.get("pairs");
        if !truth(&channel_queue_optional_length(&pairs)) {
            return Ok(V::Undefined);
        }
        // A string has .slice() and is iterable, but none of its one-character
        // elements can begin with the seven-character prefix.
        if matches!(pairs, V::String(_)) {
            return self.update_active(pairs);
        }
        let original = match pairs {
            V::Array(a) => a,
            _ => return Err(type_error("activeDoc.pairs.slice is not a function")),
        };
        let mut active = original.iter().cloned().enumerate().collect::<Vec<_>>();
        for (index, pair) in original.into_iter().enumerate() {
            let V::String(pair_text) = &pair else {
                if matches!(pair, V::Undefined | V::Null) {
                    return Err(type_error(format!(
                        "Cannot read properties of {} (reading 'startsWith')",
                        text(&pair)?
                    )));
                }
                return Err(type_error("pair.startsWith is not a function"));
            };
            let Some(pair_key) = pair_text.strip_prefix("channel-") else {
                continue;
            };
            let queue = s(&format!("channel-queue-{pair_key}"));
            let row = self.find(queue.clone())?;
            let trades = row.get("trades");
            if !truth(&channel_queue_optional_length(&trades)) {
                remove_active_pair(&mut active, index, &pair);
                continue;
            }
            let entries = match trades {
                V::Array(a) => a,
                V::String(t) => t.chars().map(|c| V::String(c.to_string())).collect(),
                _ => return Err(type_error("qDoc.trades is not iterable")),
            };
            let mut ready = vec![];
            let mut future = vec![];
            for entry in entries {
                if number(&prop(&entry, "blockHeight")?)? <= height {
                    ready.push(entry);
                } else {
                    future.push(entry);
                }
            }
            if ready.is_empty() {
                continue;
            }
            channel_queue_sort(&mut ready)?;
            let instance = self.get_instance(s(pair_key))?;
            for entry in ready {
                let kind = prop(&entry, "kind")?;
                if kind == s("token") || kind == s("contract") {
                    // Deliberately not entry.match: the pinned enqueue and drain
                    // disagree, so ordinary own-enqueued ready rows throw here.
                    let m = prop(&prop(&entry, "payload")?, "match")?;
                    let mut arguments = vec![
                        self.instance(instance)?.handle(),
                        V::Array(vec![m]),
                        prop(&entry, "blockHeight")?,
                    ];
                    if kind == s("token") {
                        arguments.push(prop(&entry, "txid")?);
                    }
                    arguments.push(V::Bool(true));
                    self.placement(
                        DependencyCall {
                            target: if kind == s("token") {
                                "Orderbook.processTokenMatches"
                            } else {
                                "Orderbook.processContractMatches"
                            }
                            .into(),
                            arguments,
                        },
                        capability,
                    )?;
                }
            }
            self.update(
                queue.clone(),
                obj(&[("_id", queue), ("trades", V::Array(future.clone()))]),
            )?;
            if future.is_empty() {
                remove_active_pair(&mut active, index, &pair);
            }
        }
        self.update_active(V::Array(
            active.into_iter().map(|(_, value)| value).collect(),
        ))
    }
}
fn channel_queue_optional_length(value: &V) -> V {
    match value {
        V::Array(a) => V::number(a.len() as f64),
        V::String(t) => V::number(t.encode_utf16().count() as f64),
        _ => value.get("length"),
    }
}
fn channel_queue_compare(a: &V, b: &V) -> Eval<core::cmp::Ordering> {
    let ha = prop(a, "blockHeight")?;
    let hb = prop(b, "blockHeight")?;
    if !strict(&ha, &hb) {
        return Ok((number(&ha)? - number(&hb)?)
            .partial_cmp(&0.0)
            .unwrap_or(core::cmp::Ordering::Equal));
    }
    let txa = prop(a, "txid")?;
    let txb = prop(b, "txid")?;
    let compared = compare_sender(&prop(a, "sender")?, &prop(b, "sender")?, &txa, &txb)?;
    if compared != core::cmp::Ordering::Equal {
        return Ok(compared);
    }
    let V::String(txa) = txa else {
        return Err(type_error(if matches!(txa, V::Undefined | V::Null) {
            format!(
                "Cannot read properties of {} (reading 'localeCompare')",
                text(&txa)?
            )
        } else {
            "a.txid.localeCompare is not a function".into()
        }));
    };
    Ok(locale_ascii(&txa, &text(&txb)?)?)
}
// The shared safe V8 implementation is also included by the Main IO runtime.
// Including it here changes no Main caller or existing snapshot encoding.
include!("source_v8_sort_impl.rs");

fn channel_queue_sort(rows: &mut [V]) -> Eval<()> {
    if rows.len() > MAX_ROWS {
        return Err(SourceRuleError::Capacity.into());
    }
    source_v8_stable_sort(rows, channel_queue_compare)
}

#[cfg(test)]
#[path = "channel_queue_v8_sort_tests.rs"]
mod channel_queue_v8_sort_tests;
