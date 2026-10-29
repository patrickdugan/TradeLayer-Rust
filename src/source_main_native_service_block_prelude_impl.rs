// Derived from CURRENT actual source_main_step_begin before its row loops.
impl SourceContractRuntime {
    fn source_main_service_block_prelude(
        &mut self,
        index: &mut SourceCompactIndex,
        block: V,
        height: V,
        documents: &V,
    ) -> R<V> {
        self.core.source_main_service_persistence()?;
        let reorg = self.core.source_main_service_reorg_check(
            height.clone(),
            funding_prop(&block, "previousblockhash")?,
        )?;
        if truth(&reorg) {
            let info = self.core.source_main_service_reorg_no_snapshots()?;
            if matches!(info.get("restoredFrom"), V::Number(_)) {
                return Ok(info.get("restoredFrom"));
            }
        }
        self.core.database("persistence")?;
        self.core.update(
            "persistence",
            obj(&[("_id", s(format!("block-{}", text(&height)?)))]),
            obj(&[(
                "$set",
                obj(&[
                    ("height", height.clone()),
                    ("hash", funding_prop(&block, "hash")?),
                    ("prevHash", funding_prop(&block, "previousblockhash")?),
                    ("createdAt", n(self.core.time_millis as f64)),
                ]),
            )]),
            true,
        )?;
        self.core.source_main_snapshot_checkpoint(height.clone())?;
        let txs = array(&self.core.native_main_service_index(
            index,
            "processBlockData",
            vec![block, height.clone()],
            documents,
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
        Ok(obj(&[
            ("funding", V::Array(funding)),
            ("other", V::Array(other)),
        ]))
    }
}
