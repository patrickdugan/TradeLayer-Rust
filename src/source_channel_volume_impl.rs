// Included in source_contract_runtime.rs. The one un-awaited token price read
// in Logic.tradeTokensChannel starts before saveVolumeDataById's factory and
// resumes at that genuinely awaited factory; no price result is supplied.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceChannelTokenPriceJob {
    token: V,
    factory_failure: Option<crate::validity_source::SourceException>,
    trace: usize,
}
impl ContractCore {
    fn source_channel_token_price_read(&mut self, token: &V) -> R<V> {
        let row = self.find(
            "volumeIndex",
            obj(&[("_id", s(format!("0-{}", source_channel_string(token)?)))]),
        )?;
        if truth(&row) {
            let value = funding_prop(&row, "value")?;
            if truth(&value) {
                let price = funding_prop(&value, "price")?;
                if truth(&price) {
                    return Ok(price);
                }
            }
        }
        Ok(n(0.001))
    }
    fn source_channel_token_price(&mut self, a: &[V]) -> R<V> {
        self.database("volumeIndex")?;
        self.source_channel_token_price_read(&arg(a, 0))
    }
    fn source_channel_token_price_begin(&mut self, token: V) -> R<()> {
        if self.channel_token_price_job.is_some() {
            return Err(SourceRuleError::Backend.into());
        }
        token.validate()?;
        let trace = self.effects.len();
        self.emit(obj(&[
            ("kind", s("dependency")),
            ("target", s("VolumeIndex.getTokenPriceInLTC")),
            ("args", V::Array(vec![token.clone()])),
        ]));
        let factory_failure = match self.database("volumeIndex") {
            Ok(_) => None,
            Err(Fault::Thrown(e)) => Some(e),
            Err(e) => return Err(e),
        };
        self.channel_token_price_job = Some(SourceChannelTokenPriceJob {
            token,
            factory_failure,
            trace,
        });
        Ok(())
    }
    fn source_channel_price_after_database(&mut self) -> R<()> {
        let Some(job) = self.channel_token_price_job.take() else {
            return Ok(());
        };
        let result = if let Some(error) = job.factory_failure {
            Err(Fault::Thrown(error))
        } else {
            self.source_channel_token_price_read(&job.token)
        };
        match result {
            Ok(value) => {
                if let Some(event) = self.effects.get_mut(job.trace) {
                    set(event, "returned", value)?;
                }
            }
            Err(Fault::Thrown(error)) => {
                self.source_supply_rejection(error.clone())?;
                if let Some(event) = self.effects.get_mut(job.trace) {
                    set(
                        event,
                        "thrown",
                        obj(&[("name", s(error.name)), ("message", s(error.message))]),
                    )?;
                }
            }
            Err(e) => return Err(e),
        }
        Ok(())
    }
}
