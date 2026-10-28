// Exact V9 Main: Logic.withdrawal is synchronous; the queue save it starts
// is not awaited. The actual Channels owner owns the row before Main resumes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceMainWithdrawalSave {
    loading: Option<DependencyReturn>,
    saving: DependencyReturn,
    row: V,
}
impl ContractCore {
    fn source_main_withdrawal_begin(&mut self, a: &[V]) -> R<V> {
        if self.source_main_withdrawal_save.is_some() {
            return Err(SourceRuleError::Capacity.into());
        }
        let cached = self
            .channels
            .state()
            .registry
            .iter()
            .any(|(key, value)| same(key, &arg(a, 1)) && truth(value));
        let loading = if cached {
            None
        } else {
            Some(returned(self.database("channels"))?)
        };
        let row = self.channels.begin_withdrawal(&[
            arg(a, 5),
            arg(a, 4),
            arg(a, 3),
            arg(a, 1),
            arg(a, 2),
            arg(a, 0),
            arg(a, 6),
        ])?;
        let saving = returned(self.database("withdrawQueue"))?;
        self.source_main_withdrawal_save = Some(SourceMainWithdrawalSave {
            loading,
            saving,
            row,
        });
        Ok(V::Undefined)
    }
    fn source_main_withdrawal_after_database(&mut self, name: &str) -> R<()> {
        if self.spot_lifecycle_enabled() && self.source_spot_main_active && name == "consensus" {
            self.source_main_withdrawal_finish()?;
        }
        Ok(())
    }
    fn source_main_withdrawal_finish(&mut self) -> R<()> {
        let Some(job) = self.source_main_withdrawal_save.take() else {
            return Ok(());
        };
        if let Some(loading) = job.loading {
            let result = awaited(loading)
                .and_then(|_| self.db("channels", "findAsync", vec![obj(&[])]))
                .and_then(|rows| {
                    self.channels.replace_loaded_registry(array(&rows)?)?;
                    Ok(V::Undefined)
                });
            match result {
                Err(Fault::Thrown(e)) if e.message.contains("does not exist") => {
                    self.channels.replace_loaded_registry(vec![])?
                }
                Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
                Err(e) => return Err(e),
                Ok(_) => (),
            }
        }
        let result = awaited(job.saving).and_then(|_| {
            self.update(
                "withdrawQueue",
                obj(&[(
                    "_id",
                    s(format!(
                        "withdrawal-{}-{}",
                        text(&job.row.get("blockHeight"))?,
                        text(&job.row.get("senderAddress"))?
                    )),
                )]),
                obj(&[("$set", obj(&[("data", job.row)]))]),
                true,
            )
        });
        match result {
            Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
            Err(e) => return Err(e),
            Ok(_) => (),
        }
        Ok(())
    }
    fn source_main_withdrawal_pending(&self) -> V {
        fn transport(value: &DependencyReturn) -> V {
            match value {
                DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
                    obj(&[("returned", v.clone())])
                }
                DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => obj(&[(
                    "thrown",
                    obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                )]),
            }
        }
        self.source_main_withdrawal_save
            .as_ref()
            .map_or(V::Null, |job| {
                obj(&[
                    ("loading", job.loading.as_ref().map_or(V::Null, transport)),
                    ("saving", transport(&job.saving)),
                    ("row", job.row.clone()),
                ])
            })
    }
}
