impl SourceContractRuntime {
    fn source_main_file_block_tail(
        &mut self,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        block: V,
        height: V,
        validity: &mut Vec<crate::source_compact_index_v2::StreamedValidityEvaluation>,
        consumed_rows: &mut u64,
        storage_stop: &mut Option<String>,
    ) -> R<V> {
        let original_block = block.clone();
        let other = (|| -> R<V> {
            let txs = array(&self.core.native_main_index(
                index,
                "processBlockData",
                vec![block, height.clone()],
            )?)?;
            let (mut funding, mut other) = (vec![], vec![]);
            for tx in txs {
                let payload = funding_prop(&tx, "payload")?;
                let V::String(payload) = payload else {
                    return Err(type_error("tx.payload.startsWith is not a function"));
                };
                if payload.starts_with('4') || payload.starts_with('m') {
                    funding.push(tx);
                } else {
                    other.push(tx);
                }
            }
            if !funding.is_empty() {
                self.source_main_step_rows(
                    index,
                    V::Array(funding),
                    height.clone(),
                    validity,
                    consumed_rows,
                    storage_stop,
                )?;
            }
            Ok(V::Array(other))
        })()?;
        let height = self.source_main_step_mid(
            index,
            &[other, height],
            validity,
            consumed_rows,
            storage_stop,
        )?;
        self.source_main_production_dispatch(
            "Main.blockHandlerEnd",
            &[funding_prop(&original_block, "hash")?, height.clone()],
        )?;
        Ok(obj(&[("reOrg", V::Bool(false)), ("restore", height)]))
    }
}
