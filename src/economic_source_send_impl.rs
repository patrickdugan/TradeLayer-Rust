// Included in economic.rs. Only the private raw-source runner supplies this
// freshly recomputed evaluation; public diagnostic values confer no authority.
impl EconomicBlockStage {
    pub(crate) fn try_apply_source_send_unverified(
        &mut self,
        arguments: &[SourceValue],
        evaluation: &crate::types_source::TypesEvaluation,
    ) -> Option<core::result::Result<(), SourceCatalogExecutionError>> {
        if arguments.get(1) != Some(&SourceValue::number(2.0)) {
            return None;
        }
        Some((|| {
            let (params, _, _, ty) =
                crate::types_economic::bound_source_params(arguments, evaluation, self.height)
                    .map_err(|e| SourceCatalogExecutionError::Mapping(format!("{e:?}")))?;
            if ty != 2
                || params.get("senderAddress") != arguments[4]
                || params.get("txid") != arguments[0]
            {
                return Err(SourceCatalogExecutionError::Mapping(
                    "source send actor/transaction binding".into(),
                ));
            }
            // These branches are normally rejected earlier by the unchanged
            // decoder/validator. Preserve Logic's actual failures if reached;
            // do not replace them with an intended batch or send-all algorithm.
            if catalog_truth(&params.get("sendAll")) {
                return Err(source_send_throw("TypeError", "sendAll is not a function"));
            }
            if let (SourceValue::Array(ids), SourceValue::Array(amounts)) =
                (params.get("propertyIds"), params.get("amounts"))
            {
                let address = params.get("address");
                if ids.len() == amounts.len()
                    && matches!(address, SourceValue::Undefined | SourceValue::Null)
                {
                    return Err(source_send_throw(
                        "TypeError",
                        if address == SourceValue::Null {
                            "Cannot read properties of null (reading 'length')"
                        } else {
                            "Cannot read properties of undefined (reading 'length')"
                        },
                    ));
                }
                let length = match address {
                    SourceValue::Array(values) => Some(values.len()),
                    SourceValue::String(value) => Some(value.encode_utf16().count()),
                    _ => None,
                };
                if ids.len() != amounts.len() || Some(ids.len()) != length {
                    return Err(source_send_throw(
                        "Error",
                        "Property IDs, amounts, and recipient addresses arrays must have the same length.",
                    ));
                }
                if ids.is_empty() {
                    return Err(SourceCatalogExecutionError::Mapping(
                        "source empty multi-send has no Ledger operation".into(),
                    ));
                }
                return Err(source_send_throw(
                    "ReferenceError",
                    "processSend is not defined",
                ));
            }
            let input = crate::types_economic::map_economic_input_unverified(
                arguments,
                evaluation,
                self.height,
                self.state.config.contract,
            )
            .map_err(|e| SourceCatalogExecutionError::Mapping(format!("{e:?}")))?;
            let EconomicInput::Joint(UnverifiedJointTx {
                actor,
                operation:
                    crate::joint::JointOperation::Send {
                        id,
                        recipient,
                        amount_atoms,
                    },
                ..
            }) = &input
            else {
                return Err(EconomicError::Invariant.into());
            };
            let native = self.state.ledger().native_policy();
            if matches!(id.get(), 2 | 3) {
                // Logic chooses vesting by these literal IDs, independently of
                // catalog class. It first rejects a sub-whole-token send.
                if *amount_atoms < ATOMS_PER_TOKEN {
                    return Err(source_send_throw(
                        "Error",
                        "Amount must be greater than zero",
                    ));
                }
                if native.is_none_or(|policy| !policy.is_coupon(*id)) {
                    return Err(SourceCatalogExecutionError::Mapping(
                        "source vesting send requires the actual native marker owner".into(),
                    ));
                }
            } else if native.is_some_and(|policy| policy.is_coupon(*id)) {
                return Err(SourceCatalogExecutionError::Mapping(
                    "source Logic only couples vesting markers for property IDs 2 and 3".into(),
                ));
            }
            // BigNumber updateBalance stores each result as a JS Number. In
            // particular self-send has an observable intermediate debit. Never
            // silently retain integer precision that the source would lose.
            let before = self.state.ledger().available_atoms(*id, actor);
            source_send_exact_number(before)?;
            source_send_exact_number(*amount_atoms)?;
            let debited = before
                .checked_sub(*amount_atoms)
                .ok_or(EconomicError::Arithmetic)?;
            source_send_exact_number(debited)?;
            if actor != recipient {
                let received = self.state.ledger().available_atoms(*id, recipient);
                source_send_exact_number(received)?;
                source_send_exact_number(
                    received
                        .checked_add(*amount_atoms)
                        .ok_or(EconomicError::Arithmetic)?,
                )?;
            }
            let mut candidate = self.clone();
            candidate.try_apply_unverified(&input)?;
            // Also verify the source's stored total/other buckets after the
            // changes against the same actual sole-owner read projection.
            let parent = candidate
                .state
                .source_parent_unverified()
                .map_err(|_| EconomicError::Invariant)?;
            let context = crate::economic_validity::ValidationContext {
                network: candidate.state.config.network,
                height: self.height,
                parent,
            };
            let mut reads = crate::economic_validity::EconomicSourceBackend::for_staged_input(
                &candidate.state,
                context,
            )
            .map_err(|_| EconomicError::Invariant)?;
            for owner in [actor, recipient] {
                crate::validity_source::SourceBackend::invoke(
                    &mut reads,
                    &crate::validity_source::DependencyCall {
                        target: "Tally.getTally".into(),
                        arguments: vec![
                            SourceValue::String(owner.as_str().into()),
                            SourceValue::number(f64::from(id.get())),
                        ],
                    },
                )
                .map_err(|error| match error {
                    crate::validity_source::SourceRuleError::UnsupportedNumericDomain => {
                        SourceCatalogExecutionError::Mapping(
                            "source send result loses Number precision".into(),
                        )
                    }
                    other => SourceCatalogExecutionError::Rule(other),
                })?;
            }
            *self = candidate;
            Ok(())
        })())
    }
}

fn source_send_throw(name: &str, message: &str) -> SourceCatalogExecutionError {
    SourceCatalogExecutionError::Thrown(crate::validity_source::SourceException {
        name: name.into(),
        message: message.into(),
    })
}
fn source_send_exact_number(atoms: u128) -> core::result::Result<(), SourceCatalogExecutionError> {
    crate::native_entitlement::source_tokens(atoms)
        .map(|_| ())
        .map_err(|_| {
            SourceCatalogExecutionError::Mapping("source send result loses Number precision".into())
        })
}
