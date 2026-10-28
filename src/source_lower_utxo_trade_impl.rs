// Literal raw3 typeSwitch mapping to the already owned V6 producer.
// Ordinary raw3 Types omits outputs when calling Validity; preserve that throw.
impl SourceContractRuntime {
    fn source_lower_utxo_trade_raw_logic(
        &mut self,
        params: &V,
    ) -> Result<SourceOutcome, SourceRuleError> {
        let fields = [
            "senderAddress",
            "satsPaymentAddress",
            "propertyId",
            "amount",
            "columnA",
            "satsExpected",
            "tokenDeliveryAddress",
            "satsReceived",
            "price",
            "paymentPercent",
            "tagWithdraw",
            "block",
            "txid",
        ];
        match self.invoke_dependency(&DependencyCall {
            target: "Logic.tradeTokenForUTXO".into(),
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
