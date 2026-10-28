// V8-only literal lower send/commit graph. All spendable balances and channel
// records remain in the existing Core Tally/Channels owners.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceSendTallyReadJob {
    address: V,
    property: V,
    load_pending: bool,
    phase: u8,
    trace: usize,
}
fn send_reference_error(message: &str) -> Fault {
    Fault::Thrown(crate::validity_source::SourceException {
        name: "ReferenceError".into(),
        message: message.into(),
    })
}
fn send_length(value: &V) -> R<V> {
    match value {
        V::Array(values) => Ok(n(values.len() as f64)),
        V::String(value) => Ok(n(value.encode_utf16().count() as f64)),
        other => funding_prop(other, "length"),
    }
}
impl ContractCore {
    fn source_send_commit_state(&self) -> V {
        obj(&[
            (
                "tallyInstance",
                if self.source_send_tally_instance_initialized {
                    obj(&[
                        ("addresses", V::Array(vec![])),
                        ("feeCache", V::Array(vec![])),
                        ("modFlag", V::Bool(false)),
                    ])
                } else {
                    V::Undefined
                },
            ),
            (
                "pendingRead",
                V::Bool(self.source_send_tally_read_job.is_some()),
            ),
        ])
    }
    fn source_send_tally_snapshot(&self, address: &V, property: &V) -> R<V> {
        if !self.tally_static_initialized {
            return Err(type_error(
                "Cannot read properties of undefined (reading 'has')",
            ));
        }
        let Some((_, properties)) = self.tally.iter().find(|(key, _)| same(key, address)) else {
            return Ok(n(0.));
        };
        let balance = funding_prop(properties, &source_channel_string(property)?)?;
        if !truth(&balance) {
            return Ok(n(0.));
        }
        Ok(obj(&[
            ("amount", funding_prop(&balance, "amount")?),
            ("available", funding_prop(&balance, "available")?),
            ("reserved", funding_prop(&balance, "reserved")?),
            ("margin", funding_prop(&balance, "margin")?),
            ("vesting", funding_prop(&balance, "vesting")?),
            ("channel", funding_prop(&balance, "channelBalance")?),
        ]))
    }
    fn source_send_read_begin(&mut self, address: V, property: V) -> R<()> {
        if self.source_send_tally_read_job.is_some() {
            return Err(SourceRuleError::Backend.into());
        }
        let trace = self.effects.len();
        self.emit(obj(&[
            ("kind", s("dependency")),
            ("target", s("Tally.getTally")),
            ("args", V::Array(vec![address.clone(), property.clone()])),
        ]));
        let load_pending = match self.database("tallyMap") {
            Ok(_) => true,
            Err(Fault::Thrown(_)) => false,
            Err(error) => return Err(error),
        };
        self.source_send_tally_read_job = Some(SourceSendTallyReadJob {
            address,
            property,
            load_pending,
            phase: 0,
            trace,
        });
        Ok(())
    }
    fn source_send_read_after_database(&mut self) -> R<()> {
        let Some(mut job) = self.source_send_tally_read_job.take() else {
            return Ok(());
        };
        if job.phase != 0 {
            self.source_send_tally_read_job = Some(job);
            return Ok(());
        }
        if job.load_pending {
            let result = (|| -> R<()> {
                let row = self.find("tallyMap", obj(&[("_id", s("tallyMap"))]))?;
                let entries = Self::tally_parsed_entries(&row)?;
                self.tally = entries;
                self.tally_static_initialized = true;
                Ok(())
            })();
            match result {
                Err(Fault::Thrown(_)) => (),
                Err(error) => return Err(error),
                Ok(()) => (),
            }
        }
        job.phase = 1;
        self.source_send_tally_read_job = Some(job);
        Ok(())
    }
    fn source_send_read_after_load(&mut self) -> R<()> {
        if !matches!(&self.source_send_tally_read_job,Some(job)if job.phase==1) {
            return Ok(());
        }
        let job = self
            .source_send_tally_read_job
            .take()
            .ok_or(SourceRuleError::Backend)?;
        let result = self.source_send_tally_snapshot(&job.address, &job.property);
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
            Err(error) => return Err(error),
        }
        Ok(())
    }
    fn source_send_read_drain(&mut self) -> R<()> {
        self.source_send_read_after_database()?;
        self.source_send_read_after_load()
    }
    fn source_send_tally(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "getInstance" => {
                self.source_send_tally_instance_initialized = true;
                self.call("Tally.loadFromDB", vec![])?;
                Ok(obj(&[("sourceTallyInstance", V::Bool(true))]))
            }
            "instanceGetAddressBalances" => {
                if arg(a, 0) != obj(&[("sourceTallyInstance", V::Bool(true))])
                    || !self.source_send_tally_instance_initialized
                {
                    return Err(SourceRuleError::InvalidInput.into());
                }
                // Constructor receiver addresses is a DISTINCT empty Map;
                // source static load/update methods do not populate this Map.
                Ok(V::Array(vec![]))
            }
            "getAddressBalances" => {
                self.call("Tally.loadFromDB", vec![])?;
                self.source_activation_await_turns(SOURCE_ACTIVATION_VM_RETURN)?;
                if !self.tally_static_initialized {
                    return Err(type_error(
                        "Cannot read properties of undefined (reading 'has')",
                    ));
                }
                let Some((_, properties)) =
                    self.tally.iter().find(|(key, _)| same(key, &arg(a, 0)))
                else {
                    return Ok(V::Array(vec![]));
                };
                let properties = properties.clone();
                let V::Object(fields) = properties else {
                    return Err(SourceRuleError::InvalidInput.into());
                };
                let mut balances = vec![];
                for (id, balance) in fields {
                    let info = self.call("Property.getPropertyData", vec![s(&id)])?;
                    self.source_activation_await_turns(SOURCE_ACTIVATION_VM_RETURN)?;
                    // Source unconditionally dereferences info.ticker after an
                    // unused guarded ticker assignment; null still throws.
                    let ticker = funding_prop(&info, "ticker")?;
                    balances.push(obj(&[
                        ("propertyId", s(id)),
                        ("ticker", ticker),
                        ("amount", funding_prop(&balance, "amount")?),
                        ("available", funding_prop(&balance, "available")?),
                        ("reserved", funding_prop(&balance, "reserved")?),
                        ("margin", funding_prop(&balance, "margin")?),
                        ("vesting", funding_prop(&balance, "vesting")?),
                        ("channel", funding_prop(&balance, "channelBalance")?),
                    ]));
                }
                Ok(V::Array(balances))
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_send_commit_logic(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "sendToken" => {
                if truth(&arg(a, 0)) {
                    return Err(type_error("sendAll is not a function"));
                }
                let ids = arg(a, 3);
                let amounts = arg(a, 4);
                if let (V::Array(ids), V::Array(amounts)) = (&ids, &amounts) {
                    if ids.len() != amounts.len()
                        || !same(&n(ids.len() as f64), &send_length(&arg(a, 2))?)
                    {
                        return Err(error("Property IDs, amounts, and recipient addresses arrays must have the same length."));
                    }
                    if !ids.is_empty() {
                        return Err(send_reference_error("processSend is not defined"));
                    }
                } else if registry_equal(&ids, &n(2.))? || registry_equal(&ids, &n(3.))? {
                    self.call(
                        "Logic.vestingSend",
                        vec![arg(a, 1), arg(a, 2), ids, amounts, arg(a, 5)],
                    )?;
                } else if !matches!(ids, V::Null | V::Undefined) {
                    self.call(
                        "Logic.sendSingle",
                        vec![arg(a, 1), arg(a, 2), ids, amounts, arg(a, 5)],
                    )?;
                }
                Ok(V::Undefined)
            }
            "sendSingle" => {
                lower_primitive_identity(&arg(a, 0))?;
                lower_primitive_identity(&arg(a, 1))?;
                self.call("Tally.getInstance", vec![])?;
                self.source_send_read_begin(arg(a, 0), arg(a, 2))?;
                // The Promise< amount comparison can only log a message; the
                // source throw is commented out. Its result is never financial.
                let debit = self.call(
                    "Tally.updateBalance",
                    vec![
                        arg(a, 0),
                        arg(a, 2),
                        n(-number(&arg(a, 3))?),
                        n(0.),
                        n(0.),
                        n(0.),
                        s("send"),
                        arg(a, 4),
                    ],
                );
                self.source_send_read_drain()?;
                debit?;
                self.call(
                    "Tally.updateBalance",
                    vec![
                        arg(a, 1),
                        arg(a, 2),
                        arg(a, 3),
                        n(0.),
                        n(0.),
                        n(0.),
                        s("receive"),
                        arg(a, 4),
                    ],
                )?;
                Ok(s("Send operation successful"))
            }
            "sendAll" => {
                let instance = self.call("Tally.getInstance", vec![])?;
                let balances = self.call(
                    "Tally.instanceGetAddressBalances",
                    vec![instance, arg(a, 0)],
                )?;
                if array(&balances)?.is_empty() {
                    return Err(error("No balances to send"));
                }
                // No admitted source method mutates instance.addresses; this
                // branch is unreachable without an external object mutation.
                Err(SourceRuleError::InvalidInput.into())
            }
            "processSend" => {
                let tally = self.call("SourceGlobal.get", vec![s("tallyMap")])?;
                if tally == obj(&[("sourceTally", V::Bool(true))]) {
                    return Err(type_error("tallyMap.getAvailableBalance is not a function"));
                }
                Err(SourceRuleError::InvalidInput.into())
            }
            "calculateVestingMovement" => {
                let tally = arg(a, 1);
                let amount = FundingBn::new(&arg(a, 0))?;
                let available = FundingBn::new(&funding_prop(&tally, "available")?)?;
                let vesting = FundingBn::new(&funding_prop(&tally, "vesting")?)?;
                let movement = vesting
                    .op(amount.op(available, 3)?, 2)?
                    .q(0, FundingRounding::Down)?
                    .value();
                Ok(s(crate::channels_source::source_big_number_text(
                    &movement,
                )?))
            }
            "roundToEightDecimals" => Ok(n((number(&arg(a, 0))? * 1e8).floor() / 1e8)),
            "vestingSend" => {
                let amount = arg(a, 3);
                if matches!(
                    FundingBn::new(&amount)?
                        .q(0, FundingRounding::Down)?
                        .compare(&FundingBn::new(&n(0.))?),
                    Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
                ) {
                    return Err(error("Amount must be greater than zero"));
                }
                let tally = self.call("Tally.getTally", vec![arg(a, 0), arg(a, 2)])?;
                let movement = self.call(
                    "Logic.calculateVestingMovement",
                    vec![amount.clone(), tally],
                )?;
                self.call(
                    "Tally.updateBalance",
                    vec![
                        arg(a, 0),
                        arg(a, 2),
                        n(-number(&amount)?),
                        n(0.),
                        n(0.),
                        n(0.),
                        s("vestingSend"),
                        arg(a, 4),
                    ],
                )?;
                self.call(
                    "Tally.updateBalance",
                    vec![
                        arg(a, 1),
                        arg(a, 2),
                        amount,
                        n(0.),
                        n(0.),
                        n(0.),
                        s("vestingSend"),
                        arg(a, 4),
                    ],
                )?;
                self.call(
                    "Tally.updateBalance",
                    vec![
                        arg(a, 0),
                        arg(a, 2),
                        n(0.),
                        n(0.),
                        n(0.),
                        n(-number(&movement)?),
                        s("vestingDrag"),
                        arg(a, 4),
                    ],
                )?;
                self.call(
                    "Tally.updateBalance",
                    vec![
                        arg(a, 1),
                        arg(a, 2),
                        n(0.),
                        n(0.),
                        n(0.),
                        movement,
                        s("vestingFollow"),
                        arg(a, 4),
                    ],
                )?;
                Ok(V::Undefined)
            }
            "commitToken" => {
                self.call(
                    "Tally.updateBalance",
                    vec![
                        arg(a, 0),
                        arg(a, 2),
                        n(-number(&arg(a, 3))?),
                        n(0.),
                        n(0.),
                        n(0.),
                        s("commit"),
                        arg(a, 6),
                    ],
                )?;
                self.call(
                    "Tally.updateChannelBalance",
                    vec![
                        arg(a, 1),
                        arg(a, 2),
                        arg(a, 3),
                        s("channelReceive"),
                        arg(a, 6),
                    ],
                )?;
                self.call(
                    "Channels.recordCommitToChannel",
                    vec![
                        arg(a, 1),
                        arg(a, 0),
                        arg(a, 2),
                        arg(a, 3),
                        arg(a, 4),
                        arg(a, 5),
                        arg(a, 6),
                        arg(a, 7),
                        lower_default(arg(a, 8), V::Null),
                    ],
                )?;
                Ok(V::Undefined)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
impl SourceContractRuntime {
    fn source_send_commit_raw_logic(
        &mut self,
        kind: &V,
        params: &V,
    ) -> Result<SourceOutcome, SourceRuleError> {
        let (method, fields): (&str, &[&str]) = if same(kind, &n(2.)) {
            (
                "sendToken",
                &[
                    "sendAll",
                    "senderAddress",
                    "address",
                    "propertyIds",
                    "amounts",
                    "block",
                ],
            )
        } else if same(kind, &n(4.)) {
            (
                "commitToken",
                &[
                    "senderAddress",
                    "channelAddress",
                    "propertyId",
                    "amount",
                    "payEnabled",
                    "clearLists",
                    "block",
                    "txid",
                    "commitClearlistId",
                ],
            )
        } else {
            return Err(SourceRuleError::InvalidInput);
        };
        match self.invoke_dependency(&DependencyCall {
            target: format!("Logic.{method}"),
            arguments: fields.iter().map(|field| params.get(field)).collect(),
        })? {
            DependencyReturn::Immediate(_) | DependencyReturn::Pending(_) => {
                Ok(SourceOutcome::Returned(V::Undefined))
            }
            DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error) => {
                Ok(SourceOutcome::Thrown(error))
            }
        }
    }
}
