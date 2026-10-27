// Included in source_contract_runtime.rs. Only the genuinely un-awaited
// Orderbook.processContractMatches -> TradeHistory.savePNL call uses this path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceHistoryJob {
    key: String,
    params: V,
    trace: usize,
}
impl ContractCore {
    fn source_history_begin(&mut self, a: &[V]) -> R<V> {
        if a.len() > 1 || self.source_history_jobs.len() >= MAX_TABLE_ROWS {
            return Err(SourceRuleError::Capacity.into());
        }
        let params = arg(a, 0);
        params.validate()?;
        let bytes = self
            .source_history_jobs
            .iter()
            .try_fold(params.diagnostic_json()?.len(), |n, job| {
                n.checked_add(job.params.diagnostic_json()?.len())
                    .ok_or(SourceRuleError::Capacity)
            })
            .map_err(Fault::Local)?;
        if bytes > 8 * 1024 * 1024 {
            return Err(SourceRuleError::Capacity.into());
        }
        let trace = self.effects.len();
        self.emit(obj(&[
            ("kind", s("dependency")),
            ("target", s("TradeHistory.savePNL")),
            ("args", V::Array(a.to_vec())),
        ]));
        let start = (|| {
            let key = format!(
                "rPNL-{}-{}-{}",
                text(&funding_prop(&params, "address")?)?,
                text(&funding_prop(&params, "contractId")?)?,
                text(&funding_prop(&params, "height")?)?
            );
            self.database("tradeHistory")?;
            Ok(key)
        })();
        match start {
            Ok(key) => {
                self.source_history_jobs.push(SourceHistoryJob {
                    key: key.clone(),
                    params,
                    trace,
                });
                Ok(s(key))
            }
            Err(Fault::Thrown(e)) => {
                self.source_supply_rejection(e.clone())?;
                if let Some(event) = self.effects.get_mut(trace) {
                    set(
                        event,
                        "thrown",
                        obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                    )?;
                }
                Err(Fault::Thrown(e))
            }
            Err(e) => Err(e),
        }
    }
    fn source_history_after_database(&mut self, _name: &str) -> R<()> {
        self.source_history_drain()
    }
    fn source_history_drain(&mut self) -> R<()> {
        let jobs = std::mem::take(&mut self.source_history_jobs);
        for job in jobs {
            let result = (|| {
                let value = json(&job.params)?;
                self.update(
                    "tradeHistory",
                    obj(&[("_id", s(&job.key))]),
                    obj(&[("$set", obj(&[("value", s(value))]))]),
                    true,
                )?;
                Ok(s(&job.key))
            })();
            match result {
                Ok(value) => {
                    if let Some(event) = self.effects.get_mut(job.trace) {
                        set(event, "returned", value)?;
                    }
                }
                Err(Fault::Thrown(e)) => {
                    self.source_supply_rejection(e.clone())?;
                    if let Some(event) = self.effects.get_mut(job.trace) {
                        set(
                            event,
                            "thrown",
                            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                        )?;
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
