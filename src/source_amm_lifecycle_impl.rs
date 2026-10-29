// AmmLifecycleV11 only. AMMPool receivers and the genuine PropertyManager
// ammIndex live beside the existing registry/Tally owners, never a second ledger.
const MAX_SOURCE_AMM_INSTANCES: usize = 1024;
fn amm_reference_error(name: &str) -> Fault {
    Fault::Thrown(crate::validity_source::SourceException {
        name: "ReferenceError".into(),
        message: format!("{name} is not defined"),
    })
}
fn amm_or_zero(value: V) -> V {
    if truth(&value) {
        value
    } else {
        n(0.)
    }
}
fn amm_handle(index: usize) -> V {
    obj(&[("sourceAmmInstance", n((index + 1) as f64))])
}
fn amm_receiver(value: &V, length: usize) -> R<usize> {
    let id = number(&value.get("sourceAmmInstance"))?;
    if !id.is_finite() || id < 1. || id.fract() != 0. || id > length as f64 {
        return Err(SourceRuleError::InvalidInput.into());
    }
    Ok(id as usize - 1)
}
fn amm_own_address(value: &V) -> R<String> {
    // These source keys reach Object.prototype rather than own balance/share
    // entries. Full inherited accessors/functions require a separate heap model.
    let key = source_channel_string(value)?;
    if [
        "__proto__",
        "constructor",
        "toString",
        "valueOf",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toLocaleString",
        "__defineGetter__",
        "__defineSetter__",
        "__lookupGetter__",
        "__lookupSetter__",
    ]
    .contains(&key.as_str())
    {
        return Err(SourceRuleError::UnsupportedNumericDomain.into());
    }
    Ok(key)
}
impl ContractCore {
    fn amm_lifecycle_enabled(&self) -> bool {
        matches!(
            self.policy,
            SourceContractRuntimePolicy::AmmLifecycleV11
                | SourceContractRuntimePolicy::MainOrchestrationV12
                | SourceContractRuntimePolicy::RichThrownV14
        )
    }
    fn source_amm_insert(&mut self, value: V) -> R<usize> {
        if self.source_amm_instances.len() == MAX_SOURCE_AMM_INSTANCES {
            return Err(SourceRuleError::Capacity.into());
        }
        self.source_amm_instances.push(value);
        Ok(self.source_amm_instances.len() - 1)
    }
    fn source_amm_state(&self) -> V {
        obj(&[
            (
                "instances",
                V::Array(
                    self.source_amm_instances
                        .iter()
                        .enumerate()
                        .map(|(id, value)| {
                            let mut fields = vec![("id".to_string(), n((id + 1) as f64))];
                            if let V::Object(own) = value {
                                fields.extend(own.clone());
                            }
                            V::Object(fields)
                        })
                        .collect(),
                ),
            ),
            (
                "propertyAmms",
                V::Array(
                    self.source_amm_property_index
                        .iter()
                        .map(|(key, id)| {
                            V::Array(vec![s(key), self.source_amm_instances[*id].clone()])
                        })
                        .collect(),
                ),
            ),
            (
                "propertyRegistryBound",
                V::Bool(self.globals.get("sourcePropertyRegistry") == V::Bool(true)),
            ),
            ("staticFields", self.source_amm_static_fields()),
            ("caughtTokenOrderErrors", self.source_amm_token_failures()),
            (
                "staticTallyBound",
                V::Bool(self.globals.get("sourceAmmStaticTally") == V::Bool(true)),
            ),
        ])
    }
    fn source_amm_construct(&mut self, a: &[V]) -> R<V> {
        if a.len() > 4 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let value = obj(&[
            ("position", arg(a, 0)),
            ("maxPosition", arg(a, 1)),
            ("maxQuoteSize", arg(a, 2)),
            ("contractType", arg(a, 3)),
            ("lpAddresses", obj(&[])),
            ("ammOrders", V::Array(vec![])),
        ]);
        Ok(amm_handle(self.source_amm_insert(value)?))
    }
    fn source_amm_bind_property_input(&mut self, a: &[V]) -> R<V> {
        // Explicit initial cache input: invoke the real singleton constructor and
        // insert the same receiver into its actual Map; never accept balances.
        if a.len() != 3 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let id = amm_receiver(&arg(a, 2), self.source_amm_instances.len())?;
        let key = format!(
            "{}-{}",
            source_channel_string(&arg(a, 0))?,
            source_channel_string(&arg(a, 1))?
        );
        self.call("Property.getInstance", vec![])?;
        if let Some((_, value)) = self
            .source_amm_property_index
            .iter_mut()
            .find(|(k, _)| k == &key)
        {
            *value = id;
        } else {
            if self.source_amm_property_index.len() == MAX_SOURCE_AMM_INSTANCES {
                return Err(SourceRuleError::Capacity.into());
            }
            self.source_amm_property_index.push((key, id));
        }
        Ok(V::Undefined)
    }
    fn source_amm_property_get(&mut self, a: &[V]) -> R<V> {
        let key = format!(
            "{}-{}",
            source_channel_string(&arg(a, 0))?,
            source_channel_string(&arg(a, 1))?
        );
        if self.globals.get("sourcePropertyRegistry") != V::Bool(true) {
            return Err(amm_reference_error("PropertyRegistry"));
        }
        self.call("Property.getInstance", vec![])?;
        if let Some((_, id)) = self
            .source_amm_property_index
            .iter()
            .find(|(k, _)| k == &key)
        {
            self.source_amm_returned_origin = Some(*id);
            return Ok(self.source_amm_instances[*id].clone());
        }
        Err(amm_reference_error("initializeAMM"))
    }
    fn source_amm_contract_get(&mut self, a: &[V]) -> R<V> {
        // Reuse the pinned registry's actual DB lookup, native Number hydration
        // and object spread. This getter does not cache the hydrated class back.
        let pool = self.source_registry_lifecycle_call("getAMM", a)?;
        let id = self.source_amm_insert(pool.clone())?;
        self.source_amm_returned_origin = Some(id);
        Ok(pool)
    }
    fn source_amm_tally(&mut self, address: V, id: V, amount: f64, kind: &str, block: V) -> R<()> {
        // A fulfilled Error from invalid LP string IDs is deliberately ignored,
        // just as each original awaited updateBalance call ignores its result.
        self.call(
            "Tally.updateBalance",
            vec![address, id, n(amount), n(0.), n(0.), n(0.), s(kind), block],
        )?;
        Ok(())
    }
    fn source_amm_capital(&mut self, id: usize, redeem: bool, a: &[V]) -> R<V> {
        let address = arg(a, 0);
        let key = amm_own_address(&address)?;
        let asset = arg(a, 1);
        let amount = number(&amm_or_zero(arg(a, 2)))?;
        let is_contract = truth(&arg(a, 3));
        let asset2 = arg(a, 4);
        let amount2 = number(&amm_or_zero(arg(a, 5)))?;
        let block = arg(a, 6);
        let pool = self.source_amm_instances[id].clone();
        let mut shares = pool.get("lpAddresses");
        let current = number(&amm_or_zero(funding_prop(&shares, &key)?))?;
        if !amount.is_finite() || amount <= 0. {
            return Err(error(if redeem {
                "Invalid AMM redeem amount"
            } else {
                "Invalid AMM capital amount"
            }));
        }
        let position = if redeem {
            if current < amount {
                return Err(error("Insufficient LP shares to redeem"));
            }
            n(number(&pool.get("position"))? - amount)
        } else {
            let next = synthetic_plus(&pool.get("position"), &n(amount))?;
            if crate::oracle_source_owner::source_comparison_unverified(
                &next,
                &pool.get("maxPosition"),
            )
            .map_err(Fault::Local)?
                == Some(core::cmp::Ordering::Greater)
            {
                return Err(error("Pool has reached its maximum position"));
            }
            next
        };
        // Statement order matters: the original position assignment precedes
        // the LP property assignment, then the second-asset guard and Tally.
        set(&mut self.source_amm_instances[id], "position", position)?;
        let next_share = if redeem {
            current - amount
        } else {
            current + amount
        };
        set(&mut shares, &key, n(next_share))?;
        if redeem && next_share <= 0. {
            let V::Object(fields) = &mut shares else {
                return Err(SourceRuleError::UnsupportedNumericDomain.into());
            };
            fields.retain(|(k, _)| k != &key);
        }
        set(&mut self.source_amm_instances[id], "lpAddresses", shares)?;
        if !is_contract && (!amount2.is_finite() || amount2 <= 0.) {
            return Err(error(if redeem {
                "Invalid second AMM redeem amount"
            } else {
                "Invalid second AMM capital amount"
            }));
        }
        let suffix = if is_contract {
            format!("{}-LP", source_channel_string(&asset)?)
        } else {
            format!(
                "{}-{}-LP",
                source_channel_string(&asset)?,
                source_channel_string(&asset2)?
            )
        };
        if redeem {
            self.source_amm_tally(address.clone(), s(suffix), -amount, "LPBurn", block.clone())?;
            self.source_amm_tally(
                address.clone(),
                asset.clone(),
                amount,
                "AMMRedeem",
                block.clone(),
            )?;
        } else {
            self.source_amm_tally(
                address.clone(),
                asset.clone(),
                -amount,
                "AMMPledge",
                block.clone(),
            )?;
            self.source_amm_tally(address.clone(), s(suffix), amount, "LPIssue", block.clone())?;
        }
        if !is_contract {
            let reverse = s(format!(
                "{}-{}-LP",
                source_channel_string(&asset2)?,
                source_channel_string(&asset)?
            ));
            if redeem {
                self.source_amm_tally(address.clone(), reverse, -amount2, "LPBurn", block.clone())?;
                self.source_amm_tally(address, asset2, amount2, "AMMRedeem", block)?;
            } else {
                self.source_amm_tally(
                    address.clone(),
                    asset2,
                    -amount2,
                    "AMMPledge",
                    block.clone(),
                )?;
                self.source_amm_tally(address, reverse, amount2, "LPIssue", block)?;
            }
        }
        Ok(V::Undefined)
    }
    fn source_amm_instance_call(&mut self, method: &str, a: &[V]) -> R<V> {
        let id = amm_receiver(&arg(a, 0), self.source_amm_instances.len())?;
        let a = &a[1..];
        match method {
            "quoteContractOrders" => self.source_amm_quote(id, a),
            "addCapital" | "redeemCapital" => {
                self.source_amm_capital(id, method == "redeemCapital", a)
            }
            "createAMM" => {
                self.source_amm_require_resolved_factory("ammRegistry")?;
                self.source_amm_require_resolved_factory("ammState")?;
                self.database("ammRegistry")?;
                self.database("ammState")?;
                Err(type_error("registryDB.findAsync is not a function"))
            }
            "saveAMMState" => {
                self.source_amm_require_resolved_factory("ammState")?;
                self.database("ammState")?;
                let mut update = arg(a, 1);
                lower_property_write(&mut update, "updatedAt", n(self.time_millis as f64))?;
                Err(type_error("stateDB.updateAsync is not a function"))
            }
            "loadAMMState" | "loadAMMRegistry" => {
                let state = method == "loadAMMState";
                self.source_amm_require_resolved_factory(if state {
                    "ammState"
                } else {
                    "ammRegistry"
                })?;
                self.database(if state { "ammState" } else { "ammRegistry" })?;
                Err(type_error(if state {
                    "stateDB.findOneAsync is not a function"
                } else {
                    "registryDB.findOneAsync is not a function"
                }))
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_amm_require_resolved_factory(&self, name: &str) -> R<()> {
        // Original helpers never await these factories. A rejected factory
        // creates a separately scheduled unhandled rejection; that optional
        // helper graph is outside the raw10 financial/capital graph owned here.
        if self
            .failures
            .iter()
            .any(|failure| failure.collection == name && failure.operation == "getDatabase")
        {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        Ok(())
    }
    fn source_amm_logic(&mut self, a: &[V]) -> R<V> {
        let sender = arg(a, 0);
        let block = arg(a, 1);
        let redeem = truth(&arg(a, 2));
        let contract = truth(&arg(a, 3));
        let asset = arg(a, 4);
        let getter = if contract {
            "Contract.getAMM"
        } else {
            "Property.getAMM"
        };
        self.source_amm_returned_origin = None;
        let pool = self.call(
            getter,
            if contract {
                vec![asset.clone()]
            } else {
                vec![asset.clone(), arg(a, 6)]
            },
        )?;
        if !truth(&pool) {
            return Err(error("AMM instance not found"));
        }
        let id = self
            .source_amm_returned_origin
            .take()
            .ok_or(SourceRuleError::Backend)?;
        let mut call = vec![amm_handle(id), sender, asset.clone(), arg(a, 5), arg(a, 3)];
        // The source contract branch passes block as id2 and leaves block itself
        // undefined. Do not repair the original five-versus-seven argument bug.
        if contract {
            call.push(block);
        } else {
            call.extend([arg(a, 6), arg(a, 7), block]);
        }
        self.call(
            if redeem {
                "AMM.redeemCapital"
            } else {
                "AMM.addCapital"
            },
            call,
        )?;
        if contract {
            let mut info = self.call("Contract.getContractInfo", vec![asset.clone()])?;
            if truth(&info) {
                set(&mut info, "ammPool", self.source_amm_instances[id].clone())?;
                self.database("contractList")?;
                let parsed = n(registry_parse_int(&asset)?);
                self.db(
                    "contractList",
                    "updateAsync",
                    vec![
                        obj(&[("id", parsed.clone()), ("type", s("contractSeries"))]),
                        obj(&[
                            ("id", parsed),
                            ("type", s("contractSeries")),
                            ("data", info),
                        ]),
                        obj(&[("upsert", V::Bool(true))]),
                    ],
                )?;
            }
        }
        Ok(V::Undefined)
    }
}
impl SourceContractRuntime {
    fn source_amm_raw_logic(&mut self, params: &V) -> Result<SourceOutcome, SourceRuleError> {
        match self.invoke_dependency(&DependencyCall {
            target: "Logic.AMMPool".into(),
            arguments: [
                "senderAddress",
                "block",
                "isRedeem",
                "isContract",
                "id",
                "amount",
                "id2",
                "amount2",
            ]
            .iter()
            .map(|field| params.get(field))
            .collect(),
        })? {
            DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
                Ok(SourceOutcome::Returned(v))
            }
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                Ok(SourceOutcome::Thrown(e))
            }
        }
    }
}
