// Included in Core only under the next explicit lifecycle policy. These are
// the final active Logic object methods, including the duplicate overwrite.
struct SourceDerivativeMarginJob {
    contract: V,
    factory: DependencyReturn,
    observation: usize,
}
struct SourceDerivativeMarginSuffix<'a> {
    core: &'a mut ContractCore,
    factory: Option<DependencyReturn>,
}
impl SourceBackend for SourceDerivativeMarginSuffix<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if let Some(factory) = self.factory.take() {
            if call.target != "db.getDatabase" || call.arguments != vec![s("marginMaps")] {
                return Err(SourceRuleError::Backend);
            }
            return Ok(factory);
        }
        self.core.invoke(call)
    }
}
impl ContractCore {
    fn source_derivative_margin_begin(&mut self, contract: V) -> R<SourceDerivativeMarginJob> {
        let observation = self.effects.len();
        self.emit(obj(&[
            ("kind", s("dependency")),
            ("target", s("Margin.getInstance")),
            ("args", V::Array(vec![contract.clone()])),
        ]));
        let factory = match self.database("marginMaps") {
            Ok(handle) => DependencyReturn::Pending(handle),
            Err(Fault::Thrown(error)) => DependencyReturn::PendingThrow(error),
            Err(error) => return Err(error),
        };
        Ok(SourceDerivativeMarginJob {
            contract,
            factory,
            observation,
        })
    }
    fn source_derivative_margin_finish(&mut self, job: SourceDerivativeMarginJob) -> R<()> {
        let mut owner = std::mem::take(&mut self.margins);
        let loaded = owner.evaluate(
            &SourceCall {
                method: "getInstance".into(),
                arguments: vec![job.contract],
            },
            &mut SourceDerivativeMarginSuffix {
                core: self,
                factory: Some(job.factory),
            },
        );
        self.margins = owner;
        match loaded? {
            SourceOutcome::Returned(value) => {
                if let Some(event) = self.effects.get_mut(job.observation) {
                    set(event, "returned", value)?;
                }
            }
            SourceOutcome::Thrown(error) => {
                if let Some(event) = self.effects.get_mut(job.observation) {
                    set(
                        event,
                        "thrown",
                        obj(&[("name", s(&error.name)), ("message", s(&error.message))]),
                    )?;
                }
                self.source_supply_rejection(error)?;
            }
        }
        Ok(())
    }
    fn source_derivative_logic(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "exerciseDerivative" => self.source_derivative_exercise(a),
            "deliverContract" => self.source_derivative_deliver(a),
            "handleOptionExercise" => self.source_derivative_option(a),
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_derivative_exercise(&mut self, a: &[V]) -> R<V> {
        let params = arg(a, 1);
        if matches!(params, V::Null | V::Undefined) {
            return Err(type_error(format!(
                "Cannot destructure property 'contractId' of 'txParams' as it is {}.",
                text(&params)?
            )));
        }
        let contract = params.get("contractId");
        let ticker = match &contract {
            V::String(ticker) => ticker,
            V::Null | V::Undefined => {
                return Err(type_error(format!(
                    "Cannot read properties of {} (reading 'match')",
                    text(&contract)?
                )))
            }
            _ => return Err(type_error("contractId.match is not a function")),
        };
        let is_option = ticker.bytes().filter(|v| *v == b'-').count() >= 2
            && ticker.bytes().any(|v| v == b'P' || v == b'C');

        // The source does not await this Promise. The genuine async factory
        // starts before the synchronous branch; its real owner suffix resumes
        // after that branch has failed, retaining any independent rejection.
        let job = self.source_derivative_margin_begin(contract.clone())?;

        // Literal extra leading sender argument: the delivery formal contractId
        // receives sender; option formals receive sender/contractId/amount/block.
        let shifted = [arg(a, 0), contract, params.get("amount"), arg(a, 2)];
        let result = if is_option {
            self.call("Logic.handleOptionExercise", shifted.to_vec())
        } else {
            self.call("Logic.deliverContract", shifted.to_vec())
        };
        self.source_derivative_margin_finish(job)?;
        result?;
        Ok(V::Undefined)
    }
    fn source_derivative_deliver(&mut self, _a: &[V]) -> R<V> {
        // The pinned ContractRegistry export has no getContractDetails method.
        // This property-call TypeError precedes all placeholder balance reads,
        // the unbound propertyId, token transfers and removeContract.
        Err(type_error(
            "ContractRegistry.getContractDetails is not a function",
        ))
    }
    fn source_derivative_option(&mut self, _a: &[V]) -> R<V> {
        // The genuine Logic member receiver has no getContractDetails helper.
        // No strike, balance, token exchange or removal is reached in this graph.
        Err(type_error("this.getContractDetails is not a function"))
    }
}
