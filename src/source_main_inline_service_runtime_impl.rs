pub const NATIVE_MAIN_INLINE_ACTIVE_SERVICE_PROFILE: &str =
    "TL-MAIN-NATIVE-INLINE-ACTIVE-SERVICE-13-1";
impl NativeMainServiceContinuationV1 {
    pub(crate) fn enable_inline_profile(&mut self) {
        self.inline_profile = true;
    }
    pub(crate) fn inline_pending_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedInlineDatabaseV1> {
        self.block.as_ref()?.inline_index.as_ref()?.pending_write()
    }
    pub(crate) fn pending_native_context(&self) -> Option<V> {
        self.inline_pending_write()
            .map(|w| w.durable_read_context())
            .or_else(|| self.pending_write().map(|w| w.durable_read_context()))
    }
}
impl SourceContractRuntime {
    fn native_main_inline_block_prefix(&mut self, block: &V, height: &V) -> R<V> {
        self.core.source_main_service_persistence()?;
        let reorg = self.core.source_main_service_reorg_check(
            height.clone(),
            funding_prop(block, "previousblockhash")?,
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
            obj(&[("_id", s(format!("block-{}", text(height)?)))]),
            obj(&[(
                "$set",
                obj(&[
                    ("height", height.clone()),
                    ("hash", funding_prop(block, "hash")?),
                    ("prevHash", funding_prop(block, "previousblockhash")?),
                    ("createdAt", n(self.core.time_millis as f64)),
                ]),
            )]),
            true,
        )?;
        self.core.source_main_snapshot_checkpoint(height.clone())?;
        Ok(V::Undefined)
    }
    fn native_main_inline_block_advance(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceBlockFrame,
        documents: &V,
    ) -> NativeMainRowStopV1 {
        self.native_main_inline_block_advance_with(frame, documents, |core, frame, documents| {
            core.native_main_inline_index_advance(index, frame, documents)
        })
    }
    fn native_main_inline_block_advance_with(
        &mut self,
        frame: &mut NativeMainServiceBlockFrame,
        documents: &V,
        mut advance: impl FnMut(
            &mut ContractCore,
            &mut NativeMainInlineIndexFrameV1,
            &V,
        ) -> NativeMainRowStopV1,
    ) -> NativeMainRowStopV1 {
        if frame.inline_index.is_none() {
            if !frame.file_prefix_done {
                match self
                    .native_main_inline_block_prefix(&frame.request.block, &frame.request.height)
                {
                    Ok(v) if matches!(v, V::Number(_)) => {
                        self.native_main_service_block_finish(
                            frame,
                            Ok(obj(&[("reOrg", V::Bool(true)), ("restore", v)])),
                        );
                        return NativeMainRowStopV1::Complete;
                    }
                    Err(e) => {
                        self.native_main_service_block_fault(frame, e);
                        return NativeMainRowStopV1::Complete;
                    }
                    Ok(_) => (),
                }
            }
            let Some(scope) = frame.request.job.checked_add(1) else {
                frame.stopped = Some(SourceRuleError::Capacity);
                return NativeMainRowStopV1::SourceStopped(SourceRuleError::Capacity);
            };
            match self.core.native_main_inline_index_begin(
                scope,
                crate::txindex_source_owner::TxIndexInlineMethodV1::ProcessBlock,
                vec![frame.request.block.clone(), frame.request.height.clone()],
            ) {
                Ok(method) => frame.inline_index = Some(method),
                Err(e) => {
                    frame.stopped = Some(e.clone());
                    return NativeMainRowStopV1::SourceStopped(e);
                }
            }
        }
        let method = frame
            .inline_index
            .as_mut()
            .expect("same retained source method");
        let stop = advance(&mut self.core, method, documents);
        if stop != NativeMainRowStopV1::Complete {
            return stop;
        }
        let result = method
            .result
            .take()
            .expect("actual completed source method");
        frame.inline_index = None;
        let result = result.and_then(|value| {
            let txs = array(&value)?;
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
            Ok((funding, other))
        });
        match result {
            Ok((funding, other)) => {
                frame.funding = funding;
                frame.other = other;
                frame.pc = NativeMainServiceBlockPc::Funding;
            }
            Err(e) => self.native_main_service_block_fault(frame, e),
        }
        NativeMainRowStopV1::Complete
    }
}
