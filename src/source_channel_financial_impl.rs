// Included in source_contract_runtime.rs. Actual pinned channel collateral
// movement over the same Tally cache and Channels owner as the surrounding Core.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceChannelBalanceChange {
    address: V,
    properties: V,
    delta: Option<Vec<V>>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceChannelBalanceJob {
    arguments: Vec<V>,
    phase: u8,
    load_pending: bool,
    early: bool,
    cache_epoch: u8,
    loaded_entries: Option<Vec<(V, V)>>,
    nested_entries: Option<(Vec<(V, V)>, bool)>,
    change: Option<SourceChannelBalanceChange>,
    delta: Option<SourceSupplyDelta>,
    save_pending: bool,
}
impl ContractCore {
    fn source_channel_tally_update(&mut self, a: &[V]) -> R<V> {
        self.tally_load_rows()?;
        let change = self.source_channel_tally_mutation(a)?;
        if let Some(delta) = &change.delta {
            self.call("Tally.recordTallyMapDelta", delta.clone())?;
        }
        self.source_channel_tally_restore(&change)?;
        self.tally_save(arg(a, 4))?;
        Ok(V::Undefined)
    }
    fn source_channel_tally_mutation(&mut self, a: &[V]) -> R<SourceChannelBalanceChange> {
        let address = arg(a, 0);
        let property = arg(a, 1);
        let change = arg(a, 2);
        if !self.tally_static_initialized {
            return Err(type_error(
                "Cannot read properties of undefined (reading 'has')",
            ));
        }
        let index = match self.tally.iter().position(|(key, _)| same(key, &address)) {
            Some(i) => i,
            None => {
                if self.tally.len() >= MAX_TABLE_ROWS {
                    return Err(SourceRuleError::Capacity.into());
                }
                self.tally.push((address.clone(), obj(&[])));
                self.tally.len() - 1
            }
        };
        let mut properties = self.tally[index].1.clone();
        let key = text(&property)?;
        let mut balance = funding_prop(&properties, &key)?;
        if !truth(&balance) {
            balance = obj(&[
                ("amount", n(0.0)),
                ("available", n(0.0)),
                ("reserved", n(0.0)),
                ("margin", n(0.0)),
                ("vesting", n(0.0)),
                ("channelBalance", n(0.0)),
            ]);
            set(&mut properties, &key, balance.clone())?;
            self.tally[index].1 = properties.clone();
        }
        if matches!(funding_prop(&balance, "channelBalance")?, V::Undefined) {
            set(&mut balance, "channelBalance", n(0.0))?;
            set(&mut properties, &key, balance.clone())?;
            self.tally[index].1 = properties.clone();
        }
        let next = FundingBn::new(&funding_prop(&balance, "channelBalance")?)?
            .op(FundingBn::new(&change)?, 0)?;
        if next.compare(&FundingBn::new(&n(0.0))?) == Some(core::cmp::Ordering::Less) {
            return Err(error(format!(
                "Channel balance cannot go negative for property {}",
                text(&property)?
            )));
        }
        set(&mut balance, "channelBalance", n(next.number()))?;
        let mut total = FundingBn::new(&funding_or(funding_prop(&balance, "available")?))?
            .q(8, FundingRounding::HalfUp)?;
        for field in ["reserved", "margin", "channelBalance"] {
            total = total.op(
                FundingBn::new(&funding_or(funding_prop(&balance, field)?))?
                    .q(8, FundingRounding::HalfUp)?,
                0,
            )?;
        }
        set(&mut balance, "amount", n(total.number()))?;
        set(&mut properties, &key, balance)?;
        self.tally[index].1 = properties.clone();
        let delta = if !same(&change, &n(0.0)) {
            Some(vec![
                address.clone(),
                arg(a, 4),
                property,
                n(total.number()),
                n(0.0),
                n(0.0),
                n(0.0),
                n(0.0),
                change,
                arg(a, 3),
            ])
        } else {
            None
        };
        Ok(SourceChannelBalanceChange {
            address,
            properties,
            delta,
        })
    }
    fn source_channel_tally_restore(&mut self, change: &SourceChannelBalanceChange) -> R<()> {
        if let Some(i) = self
            .tally
            .iter()
            .position(|(key, _)| same(key, &change.address))
        {
            self.tally[i].1 = change.properties.clone();
        } else {
            if self.tally.len() >= MAX_TABLE_ROWS {
                return Err(SourceRuleError::Capacity.into());
            }
            self.tally
                .push((change.address.clone(), change.properties.clone()));
        }
        Ok(())
    }
    fn source_channel_job_begin(&mut self, arguments: &[V]) -> R<SourceChannelBalanceJob> {
        if arguments.len() > 8 {
            return Err(SourceRuleError::Capacity.into());
        }
        for argument in arguments {
            argument.validate()?;
        }
        // loadFromDB catches its own factory rejection. The update continuation
        // still runs later against whatever cached addresses remain.
        let load_pending = match self.database("tallyMap") {
            Ok(_) => true,
            Err(Fault::Thrown(_)) => false,
            Err(e) => return Err(e),
        };
        Ok(SourceChannelBalanceJob {
            arguments: arguments.to_vec(),
            phase: 0,
            load_pending,
            early: !load_pending,
            cache_epoch: 0,
            loaded_entries: None,
            nested_entries: None,
            change: None,
            delta: None,
            save_pending: false,
        })
    }
    fn source_channel_jobs_after_first_write(
        &mut self,
        jobs: &mut [SourceChannelBalanceJob],
    ) -> R<()> {
        // A caught factory rejection resolves loadFromDB one await earlier.
        // Its update observes the OLD cache before successful load callbacks
        // replace it, which is observable for cold and stale cached Tally rows.
        for index in 0..jobs.len() {
            if jobs[index].early && jobs[index].phase == 1 {
                self.source_channel_job_advance(&mut jobs[index], 1)?;
                Self::source_channel_jobs_link(jobs, index);
            }
        }
        let mut replaced = false;
        for job in jobs.iter_mut() {
            if let Some(entries) = job.loaded_entries.take() {
                self.tally = entries;
                self.tally_static_initialized = true;
                replaced = true;
            }
        }
        if replaced {
            for job in jobs {
                if job.change.is_none() {
                    job.cache_epoch = 1;
                }
            }
        }
        Ok(())
    }
    fn source_channel_jobs_link(jobs: &mut [SourceChannelBalanceJob], index: usize) {
        let Some(change) = jobs[index].change.clone() else {
            return;
        };
        let epoch = jobs[index].cache_epoch;
        // Same known Map entry and cache generation: a load replacement
        // separates references even when imported row values happen to match.
        for job in jobs {
            if job.cache_epoch == epoch {
                if let Some(captured) = &mut job.change {
                    if same(&captured.address, &change.address) {
                        captured.properties = change.properties.clone();
                    }
                }
            }
        }
    }
    fn source_channel_jobs_tick(&mut self, jobs: &mut [SourceChannelBalanceJob]) -> R<()> {
        let mut order = (0..jobs.len()).collect::<Vec<_>>();
        order.sort_by(|&a, &b| {
            let (a, b) = (&jobs[a], &jobs[b]);
            if a.early && a.phase == 4 && !b.early && b.phase == 3 {
                core::cmp::Ordering::Greater
            } else if b.early && b.phase == 4 && !a.early && a.phase == 3 {
                core::cmp::Ordering::Less
            } else {
                core::cmp::Ordering::Equal
            }
        });
        for index in order {
            let job = &mut jobs[index];
            if job.phase != 6 {
                let phase = job.phase;
                self.source_channel_job_advance(job, phase)?;
                if phase == 1 {
                    Self::source_channel_jobs_link(jobs, index);
                }
            }
        }
        // getTally's nested load resumes independently of recordTallyMapDelta.
        // Its find event precedes the delta insert, but the parsed Map is
        // installed after the other ready continuations in this tick. In the
        // stale-cache profile that can overwrite an early update's restoration
        // between its save factory and its eventual JSON serialization.
        for job in jobs {
            if let Some((entries, initialized)) = job.nested_entries.take() {
                self.tally = entries;
                self.tally_static_initialized = initialized;
            }
        }
        Ok(())
    }
    // Stages: 0 initial load find; 1 cache mutation + delta factory; 2 nested
    // Tally factory + delta find; 3 nested Tally find + delta write; 4 address
    // restoration + save factory; 5 save serialization/write. A source rejection
    // belongs to the un-awaited update and never replaces the Logic result.
    fn source_channel_job_advance(
        &mut self,
        job: &mut SourceChannelBalanceJob,
        stage: u8,
    ) -> R<()> {
        if job.phase == 6 {
            return Ok(());
        }
        if stage != job.phase || stage > 5 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let mut next = stage + 1;
        let result = (|| {
            match stage {
                0 => {
                    if job.load_pending {
                        let load = (|| {
                            let row = self.find("tallyMap", obj(&[("_id", s("tallyMap"))]))?;
                            job.loaded_entries = Some(Self::tally_parsed_entries(&row)?);
                            Ok(())
                        })();
                        match load {
                            Err(Fault::Thrown(_)) => (),
                            Err(e) => return Err(e),
                            Ok(_) => (),
                        }
                    }
                    job.load_pending = false;
                }
                1 => {
                    let change = self.source_channel_tally_mutation(&job.arguments)?;
                    job.change = Some(change.clone());
                    if let Some(arguments) = &change.delta {
                        job.delta = Some(self.source_record_tally_start(arguments)?);
                    } else {
                        self.source_channel_job_save_start(job)?;
                        next = 5;
                    }
                }
                2 => {
                    if let Some(delta) = &mut job.delta {
                        self.source_record_tally_prefix(delta)?;
                    }
                }
                3 => {
                    if let Some(delta) = &mut job.delta {
                        let before = self.tally.clone();
                        let initialized = self.tally_static_initialized;
                        let pending = delta.tally_pending;
                        let result = self.source_record_tally_finish(delta);
                        if pending {
                            job.nested_entries =
                                Some((self.tally.clone(), self.tally_static_initialized));
                        }
                        self.tally = before;
                        self.tally_static_initialized = initialized;
                        result?;
                    }
                }
                4 => {
                    self.source_channel_job_save_start(job)?;
                }
                5 => {
                    if job.save_pending {
                        let pairs = V::Array(
                            self.tally
                                .iter()
                                .map(|(key, value)| V::Array(vec![key.clone(), value.clone()]))
                                .collect(),
                        );
                        let save = self.update(
                            "tallyMap",
                            obj(&[("_id", s("tallyMap"))]),
                            obj(&[(
                                "$set",
                                obj(&[
                                    ("block", arg(&job.arguments, 4)),
                                    ("data", s(json(&pairs)?)),
                                ]),
                            )]),
                            true,
                        );
                        match save {
                            Err(Fault::Thrown(_)) => (),
                            Err(e) => return Err(e),
                            Ok(_) => (),
                        }
                    }
                    job.save_pending = false;
                }
                _ => unreachable!(),
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                job.phase = next;
                Ok(())
            }
            Err(Fault::Thrown(e)) => {
                job.phase = 6;
                job.delta = None;
                job.change = None;
                self.source_supply_rejection(e)
            }
            Err(e) => Err(e),
        }
    }
    fn source_channel_job_save_start(&mut self, job: &mut SourceChannelBalanceJob) -> R<()> {
        self.source_channel_tally_restore(
            job.change.as_ref().ok_or(SourceRuleError::InvalidInput)?,
        )?;
        job.save_pending = match self.database("tallyMap") {
            Ok(_) => true,
            Err(Fault::Thrown(_)) => false,
            Err(e) => return Err(e),
        };
        Ok(())
    }

    // Only the literal channel===true branch of moveCollateralToMargin.
    // The caller retains the common reads, fee adjustment and final position
    // call, including the source seller's missing positional arguments.
    fn source_channel_collateral_move(
        &mut self,
        sender: V,
        property: V,
        total: V,
        channel: V,
        block: V,
    ) -> R<()> {
        let has = self.call(
            "Tally.hasSufficientChannel",
            vec![channel.clone(), property.clone(), total.clone()],
        )?;
        if truth(&funding_prop(&has, "hasSufficient")?) {
            self.call(
                "Tally.updateChannelBalance",
                vec![
                    channel.clone(),
                    property.clone(),
                    n(-number(&total)?),
                    s("debitChannelContractTradeInitMargin"),
                    block.clone(),
                ],
            )?;
            self.call(
                "Tally.updateBalance",
                vec![
                    sender.clone(),
                    property.clone(),
                    n(0.0),
                    n(0.0),
                    total.clone(),
                    n(0.0),
                    s("creditChannelContractTradeInitMargin"),
                    block.clone(),
                ],
            )?;
            self.call(
                "Channels.debitInitMarginFromChannel",
                vec![channel, sender, property, total, block],
            )?;
        } else if funding_prop(&has, "reason")? != s("undefined") {
            let shortfall = FundingBn::new(&funding_prop(&has, "shortfall")?)?;
            let debit = FundingBn::new(&total)?
                .op(shortfall.clone(), 1)?
                .q(8, FundingRounding::HalfUp)?;
            self.call(
                "Tally.updateChannelBalance",
                vec![
                    channel.clone(),
                    property.clone(),
                    n(-debit.number()),
                    s("contractTradeInitMargin"),
                    block.clone(),
                ],
            )?;
            self.call(
                "Tally.updateBalance",
                vec![
                    sender.clone(),
                    property.clone(),
                    n(-shortfall.number()),
                    n(0.0),
                    total,
                    n(0.0),
                    s("contractTradeInitMargin"),
                    block.clone(),
                ],
            )?;
            self.call(
                "Channels.debitInitMarginFromChannel",
                vec![channel, sender, property, n(debit.number()), block],
            )?;
        } else {
            return Err(error(format!(
                "reserve balance is undefined in tallymap for {}",
                text(&property)?
            )));
        }
        Ok(())
    }

    fn source_channel_record_token_trade(&mut self, a: &[V]) -> R<V> {
        let trade = arg(a, 0);
        let key = format!(
            "token-{}-{}",
            text(&funding_prop(&trade, "offeredPropertyId")?)?,
            text(&funding_prop(&trade, "desiredPropertyId")?)?
        );
        let block = arg(a, 1);
        let record = obj(&[
            ("key", s(&key)),
            ("type", s("token")),
            ("trade", trade),
            ("blockHeight", block.clone()),
            ("txid", arg(a, 2)),
        ]);
        self.database("tradeHistory")?;
        let uuid = self.new_uuid();
        let id = format!("{key}-{uuid}-{}", text(&block)?);
        let mut doc = obj(&[("_id", s(&id))]);
        if let V::Object(fields) = record {
            for (key, value) in fields {
                set(&mut doc, &key, value)?;
            }
        }
        self.update("tradeHistory", obj(&[("_id", s(id))]), doc, true)?;
        Ok(V::Undefined)
    }
}
