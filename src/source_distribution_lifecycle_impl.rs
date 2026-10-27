// Included by the explicit DistributionLifecycleV5 policy only.
// The pinned handlers fail at these actual source member lookups. Replacing
// them with functional distribution or financial helpers would repair source.
impl ContractCore {
    fn source_distribution_logic(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "payToTokens" => {
                // The ordinary Logic closure has no lowercase tallyMap binding.
                // SourceGlobal.bindTally binds the actual Tally export; its
                // totalTokens method exists only on the constructor prototype.
                let receiver = self.globals.get("tallyMap");
                match receiver {
                    V::Undefined => Err(Fault::Thrown(crate::validity_source::SourceException {
                        name: "ReferenceError".into(),
                        message: "tallyMap is not defined".into(),
                    })),
                    V::Null => Err(type_error(
                        "Cannot read properties of null (reading 'totalTokens')",
                    )),
                    _ => Err(type_error("tallyMap.totalTokens is not a function")),
                }
            }
            "tradeBaiUrbun" => {
                // Genuine Channels has isValidChannel, but no hasChannel.
                // Plain SourceValue records cannot contain callable methods.
                let receiver = arg(a, 2);
                if matches!(receiver, V::Undefined | V::Null) {
                    Err(type_error(format!(
                        "Cannot read properties of {} (reading 'hasChannel')",
                        text(&receiver)?
                    )))
                } else {
                    Err(type_error("channelRegistry.hasChannel is not a function"))
                }
            }
            "tradeMurabaha" => {
                // Called as a member of the actual Logic object, which does not
                // own channelsRegistry. The imported Channels map is distinct.
                Err(type_error(
                    "Cannot read properties of undefined (reading 'get')",
                ))
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
impl SourceContractRuntime {
    fn source_distribution_raw_logic(
        &mut self,
        kind: &V,
        p: &V,
    ) -> Result<SourceOutcome, SourceRuleError> {
        // These literal typeSwitch bridges are retained even though genuine
        // raw26 validation and raw28/29 decoders throw before reaching them.
        let (method, fields): (&str, &[&str]) =
            match number(kind).map_err(|_| SourceRuleError::InvalidInput)? as u32 {
                26 => (
                    "payToTokens",
                    &[
                        "tallyMap",
                        "propertyIdTarget",
                        "propertyIdUsed",
                        "amount",
                        "block",
                    ],
                ),
                28 => (
                    "tradeBaiUrbun",
                    &[
                        "channelAddress",
                        "propertyIdDownPayment",
                        "propertyIdToBeSold",
                        "downPaymentPercent",
                        "amount",
                        "expiryBlock",
                        "tradeExpiryBlock",
                        "block",
                    ],
                ),
                29 => (
                    "tradeMurabaha",
                    &[
                        "channelAddress",
                        "buyerAddress",
                        "sellerAddress",
                        "propertyId",
                        "costPrice",
                        "profitMargin",
                        "paymentBlockHeight",
                        "block",
                    ],
                ),
                _ => return Err(SourceRuleError::InvalidInput),
            };
        let args = fields.iter().map(|field| p.get(field)).collect();
        match self.invoke_dependency(&DependencyCall {
            target: format!("Logic.{method}"),
            arguments: args,
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
