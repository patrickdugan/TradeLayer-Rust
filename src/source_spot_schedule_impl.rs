// V9-only Main composition. One queue arena is moved through the existing
// Runtime/Core interfaces; the job is a source continuation, never a verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceSpotOrdersReadJob {
    receiver: V,
    key: V,
    address: V,
    get_trace: usize,
    load_trace: usize,
    factory_error: Option<crate::validity_source::SourceException>,
}
// The isolated validity API names instance methods but does not transport its
// local `orderbook` variable. Bind that variable to the actual returned handle.
struct SourceSpotValidityBackend<'a> {
    core: &'a mut ContractCore,
    orderbook_receiver: Option<V>,
}
impl<'a> SourceSpotValidityBackend<'a> {
    fn new(core: &'a mut ContractCore) -> Self {
        Self {
            core,
            orderbook_receiver: None,
        }
    }
}
impl SourceBackend for SourceSpotValidityBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let result = if call.target == "Orderbook.getOrdersForAddress" {
            let mut arguments = vec![self
                .orderbook_receiver
                .clone()
                .ok_or(SourceRuleError::UnknownDependency)?];
            arguments.extend(call.arguments.clone());
            self.core.invoke(&DependencyCall {
                target: call.target.clone(),
                arguments,
            })
        } else {
            self.core.invoke(call)
        };
        if call.target == "Orderbook.getOrderbookInstance" {
            if let Ok(DependencyReturn::Immediate(value) | DependencyReturn::Pending(value)) =
                &result
            {
                self.orderbook_receiver = Some(value.clone());
            }
        }
        result
    }
}
impl ContractCore {
    fn source_spot_schedule_state(&self) -> V {
        obj(&[
            ("entry", s(&self.entry)),
            ("mainActive", V::Bool(self.source_spot_main_active)),
            (
                "feeBufferInitialized",
                V::Bool(self.source_spot_fee_buffer_initialized),
            ),
            ("readSchedule", s("MainDeferredStandaloneEagerV9")),
            (
                "pendingTokenThresholdReads",
                self.source_token_threshold_pending(),
            ),
            (
                "pendingWithdrawalSave",
                self.source_main_withdrawal_pending(),
            ),
            (
                "pendingOrdersRead",
                self.source_spot_orders_read.as_ref().map_or(V::Null, |j| {
                    obj(&[
                        ("receiver", j.receiver.clone()),
                        ("key", j.key.clone()),
                        ("address", j.address.clone()),
                        ("getTrace", n(j.get_trace as f64)),
                        ("loadTrace", n(j.load_trace as f64)),
                        (
                            "factoryError",
                            j.factory_error.as_ref().map_or(V::Null, |e| {
                                obj(&[("name", s(&e.name)), ("message", s(&e.message))])
                            }),
                        ),
                    ])
                }),
            ),
        ])
    }
    fn source_spot_orders_read_begin(&mut self, receiver: &V, a: &[V]) -> R<V> {
        if self.source_spot_orders_read.is_some() {
            return Err(SourceRuleError::Capacity.into());
        }
        let contract = arg(a, 1);
        let key = if truth(&contract) {
            s(source_channel_string(&contract)?)
        } else {
            s(format!(
                "{}-{}",
                source_channel_string(&arg(a, 2))?,
                source_channel_string(&arg(a, 3))?
            ))
        };
        let get_trace = self
            .effects
            .iter()
            .rposition(|e| {
                e.get("kind") == s("dependency")
                    && e.get("target") == s("Orderbook.getOrdersForAddress")
            })
            .ok_or(SourceRuleError::Backend)?;
        let load_trace = self.source_spot_trace(
            "Orderbook.loadOrderBook",
            vec![receiver.clone(), key.clone()],
        );
        let mut queue = self
            .queue_context
            .take()
            .ok_or(SourceRuleError::UnknownDependency)?;
        let begun = (|| -> Result<SourceOutcome, SourceRuleError> {
            queue.instance_value(
                spot_handle(receiver).map_err(|_| SourceRuleError::InvalidInput)?,
            )?;
            queue.source_book_factory(key.clone())
        })();
        self.harvest_queue(&mut queue);
        self.queue_context = Some(queue);
        let factory_error = match begun? {
            SourceOutcome::Returned(_) => None,
            SourceOutcome::Thrown(e) => Some(e),
        };
        self.source_spot_orders_read = Some(SourceSpotOrdersReadJob {
            receiver: receiver.clone(),
            key,
            address: arg(a, 0),
            get_trace,
            load_trace,
            factory_error,
        });
        // The original validator observes an un-awaited Promise, not its rows.
        Ok(V::Undefined)
    }
    fn source_spot_orders_read_finish_with_queue(
        &mut self,
        queue: &mut ContractQueueSourceOwner,
    ) -> R<()> {
        let Some(job) = self.source_spot_orders_read.take() else {
            return Ok(());
        };
        let loaded = if let Some(error) = job.factory_error {
            Err(Fault::Thrown(error))
        } else {
            queue
                .source_finish_book_load(spot_handle(&job.receiver)?, job.key)
                .map_err(Fault::Local)
                .and_then(|(result, _)| outcome(result))
        };
        self.harvest_queue(queue);
        self.source_spot_trace_end(job.load_trace, &loaded);
        let result = (|| -> R<V> {
            let book = loaded?;
            let buy = funding_prop(&book, "buy")?;
            let sell = funding_prop(&book, "sell")?;
            if !truth(&buy) || !truth(&sell) {
                return Ok(V::Array(vec![]));
            }
            let mut found = vec![];
            for rows in [array(&buy)?, array(&sell)?] {
                for order in rows {
                    if spot_strict(&funding_prop(&order, "sender")?, &job.address) {
                        found.push(order);
                    }
                }
            }
            Ok(V::Array(found))
        })();
        let result = match result {
            Err(Fault::Thrown(_)) => Ok(V::Array(vec![])),
            other => other,
        };
        self.source_spot_trace_end(job.get_trace, &result);
        result.map(|_| ())
    }
    fn source_spot_orders_read_after_database(&mut self) -> R<()> {
        if self.source_spot_orders_read.is_none() {
            return Ok(());
        }
        let mut queue = self
            .queue_context
            .take()
            .ok_or(SourceRuleError::UnknownDependency)?;
        let result = self.source_spot_orders_read_finish_with_queue(&mut queue);
        self.harvest_queue(&mut queue);
        self.queue_context = Some(queue);
        result
    }
    fn source_spot_load_around_pending(
        &mut self,
        queue: &mut ContractQueueSourceOwner,
        id: u64,
        key: V,
    ) -> R<(SourceOutcome, Option<String>)> {
        if self.source_spot_orders_read.is_none() {
            return queue.load_book_with_alias(id, key).map_err(Fault::Local);
        }
        let begun = queue.source_book_factory(key).map_err(Fault::Local)?;
        self.harvest_queue(queue);
        self.source_spot_orders_read_finish_with_queue(queue)?;
        match begun {
            SourceOutcome::Returned(key) => {
                queue.source_finish_book_load(id, key).map_err(Fault::Local)
            }
            SourceOutcome::Thrown(e) => Ok((SourceOutcome::Thrown(e), None)),
        }
    }
    fn source_spot_save_around_pending(&mut self, a: &[V]) -> R<V> {
        if self.source_spot_orders_read.is_none() {
            return self.source_clearing_orderbook_call("saveOrderBook", a);
        }
        let mut queue = self
            .queue_context
            .take()
            .ok_or(SourceRuleError::UnknownDependency)?;
        let result = (|| -> R<V> {
            let begun = queue.source_book_factory(arg(a, 2)).map_err(Fault::Local)?;
            self.harvest_queue(&mut queue);
            self.source_spot_orders_read_finish_with_queue(&mut queue)?;
            match begun {
                SourceOutcome::Returned(key) => {
                    outcome(queue.source_finish_book_save(arg(a, 1), key)?)
                }
                SourceOutcome::Thrown(e) => Err(Fault::Thrown(e)),
            }
        })();
        self.harvest_queue(&mut queue);
        self.queue_context = Some(queue);
        result
    }
}
impl SourceContractRuntime {
    fn source_spot_main_enter(&mut self) -> Result<(), SourceRuleError> {
        if self.core.source_spot_main_active || self.core.queue_context.is_some() {
            return Err(SourceRuleError::Backend);
        }
        self.core.source_spot_main_active = true;
        self.core.queue_context = Some(std::mem::replace(
            &mut self.queue,
            ContractQueueSourceOwner::new(),
        ));
        Ok(())
    }
    fn source_spot_main_leave(&mut self) -> R<()> {
        self.core.source_main_withdrawal_finish()?;
        self.core.source_transfer_balance_finish()?;
        let result = self.core.source_spot_orders_read_after_database();
        let queue = self
            .core
            .queue_context
            .take()
            .ok_or(SourceRuleError::Backend);
        self.core.source_spot_main_active = false;
        if let Ok(queue) = queue {
            self.queue = queue;
        } else {
            return Err(SourceRuleError::Backend.into());
        }
        result
    }
    pub fn evaluate_queue(&mut self, call: &SourceCall) -> Result<SourceOutcome, SourceRuleError> {
        if self.policy() == SourceContractRuntimePolicy::RichThrownV14 {
            return Err(SourceRuleError::InvalidInput);
        }
        let borrowed = self.core.spot_lifecycle_enabled()
            && self.core.source_spot_main_active
            && self.core.queue_context.is_some();
        if borrowed {
            self.queue = self
                .core
                .queue_context
                .take()
                .ok_or(SourceRuleError::Backend)?;
        }
        let result = self.source_spot_evaluate_queue_inner(call);
        if borrowed {
            self.core.queue_context = Some(std::mem::replace(
                &mut self.queue,
                ContractQueueSourceOwner::new(),
            ));
        }
        result
    }
}
