// Moved-owner production entry. A private frame never escapes its actual
// financial/index owner. Every actual write is preceded by durable intent;
// its exact-post prefix ack precedes resolving the stored source await.
use crate::source_contract_runtime::{
    NativeMainCatchupContinuationV1, NativeMainRowContinuationV1, NativeMainRowStopV1,
};
use crate::source_main_native_intent_store::{
    NativeCommandIntentPermitV1, NativeIntentStorePinV1, NativeIntentStoreV1,
    NativePreparedPrefixAckV1, NativePreparedSupersedingIntentV1, NativeWriteIntentCandidatesV1,
};

pub struct RejectedNativeMainRowSessionV1 {
    pub cause: String,
    owner: SourceMainNativeSessionV13,
    store: NativeIntentStoreV1,
}
impl RejectedNativeMainRowSessionV1 {
    pub fn into_owners(self) -> (SourceMainNativeSessionV13, NativeIntentStoreV1) {
        (self.owner, self.store)
    }
}
impl core::fmt::Debug for RejectedNativeMainRowSessionV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainRowSessionV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
enum NativeMainRowControllerPcV1 {
    Source,
    NativeIntent,
    NativeAppendAttempted,
    NativePost,
    RecoveryBefore,
    SupersessionPrepared,
    SupersessionStoreAttempted,
    PrefixAckPrepared,
    PrefixAckAttempted,
    PrefixAckSynced,
    SupersessionIntentSynced,
    Accepted,
    Complete,
    Blocked(String),
}
enum NativeMainOwnedFrameV1 {
    Rows(Box<NativeMainRowContinuationV1>),
    Catchup(Box<NativeMainCatchupContinuationV1>),
}
impl NativeMainOwnedFrameV1 {
    fn result(&self) -> Option<&Result<DependencyReturn, SourceRuleError>> {
        match self {
            Self::Rows(frame) => frame.result(),
            Self::Catchup(frame) => frame.result(),
        }
    }
    fn pending_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedValidityWriteV1> {
        match self {
            Self::Rows(frame) => frame.pending_write(),
            Self::Catchup(frame) => frame.pending_write(),
        }
    }
    fn validity(&self) -> &[StreamedValidityEvaluation] {
        match self {
            Self::Rows(frame) => frame.validity(),
            Self::Catchup(frame) => frame.validity(),
        }
    }
    fn consumed_rows(&self) -> u64 {
        match self {
            Self::Rows(frame) => frame.consumed_rows(),
            Self::Catchup(frame) => frame.consumed_rows(),
        }
    }
    fn grouping(&self) -> Option<&RowStreamReceipt> {
        match self {
            Self::Rows(_) => None,
            Self::Catchup(frame) => frame.grouping(),
        }
    }
    fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        match self {
            Self::Rows(frame) => frame.visit_canonical_fields(visitor),
            Self::Catchup(frame) => frame.visit_canonical_fields(visitor),
        }
    }
    fn advance(
        &mut self,
        runtime: &mut SourceContractRuntime,
        index: &mut SourceCompactIndex,
    ) -> NativeMainRowStopV1 {
        match self {
            Self::Rows(frame) => runtime.native_main_rows_advance(index, frame),
            Self::Catchup(frame) => runtime.native_main_catchup_advance(index, frame),
        }
    }
    fn append(
        &mut self,
        runtime: &mut SourceContractRuntime,
        index: &mut SourceCompactIndex,
    ) -> NativeMainRowStopV1 {
        match self {
            Self::Rows(frame) => runtime.native_main_rows_append(index, frame),
            Self::Catchup(frame) => runtime.native_main_catchup_append(index, frame),
        }
    }
    fn resolve_post(
        &mut self,
        runtime: &mut SourceContractRuntime,
        index: &mut SourceCompactIndex,
    ) -> NativeMainRowStopV1 {
        match self {
            Self::Rows(frame) => runtime.native_main_rows_resolve_post(index, frame),
            Self::Catchup(frame) => runtime.native_main_catchup_resolve_post(index, frame),
        }
    }
}
/// No Clone, no mutable runtime/index accessor, no return to a frozen head.
/// This profile retains the original owner and source await on every failure.
pub struct SourceMainNativeRowSessionV1 {
    owner: SourceMainNativeSessionV13,
    store: NativeIntentStoreV1,
    permit: NativeCommandIntentPermitV1,
    frame: Option<NativeMainOwnedFrameV1>,
    source_start_error: Option<SourceRuleError>,
    pc: NativeMainRowControllerPcV1,
    receipt_budget: usize,
    command: NativeMainSessionCommand,
    reopened: Option<crate::source_compact_index_v2::NativePreparedValidityRecoveryObservationV1>,
    supersession: Option<NativeMainPreparedRowSupersessionV1>,
    superseding_active: bool,
    prefix_ack: Option<NativePreparedPrefixAckV1>,
    prefix_ack_recovered: Option<NativeIntentStorePinV1>,
}
impl SourceMainNativeSessionV13 {
    pub fn into_row_session(
        mut self,
        mut store: NativeIntentStoreV1,
        expected: &NativeMainSessionExpectation,
        store_pin: &NativeIntentStorePinV1,
        command: NativeMainSessionCommand,
        receipt_budget: usize,
    ) -> Result<SourceMainNativeRowSessionV1, Box<RejectedNativeMainRowSessionV1>> {
        let checked = (|| {
            if self.halted || self.expectation().map_err(|e| format!("{e:?}"))? != *expected {
                return Err("halted or mismatched moved native owner".to_owned());
            }
            match &command {
                NativeMainSessionCommand::ProcessRows { rows, height, .. } => {
                    rows.validate().map_err(|e| e.to_string())?;
                    height.validate().map_err(|e| e.to_string())?;
                    if !matches!(rows, V::Array(rows) if rows.len() <= crate::source_contract_runtime::MAX_TABLE_ROWS)
                    {
                        return Err("bounded raw row input required".into());
                    }
                }
                NativeMainSessionCommand::ConstructConsensusFromIndex { start } => {
                    start.validate().map_err(|e| e.to_string())?;
                    self.financial
                        .runtime()
                        .native_main_catchup_continuation_preflight()
                        .map_err(|e| e.to_string())?;
                }
                _ => return Err("actual row/catchup command required".into()),
            }
            let raw = command
                .value()
                .diagnostic_json()
                .map_err(|e| e.to_string())?;
            let financial = self
                .financial
                .native_main_financial_commitment()
                .map_err(|e| e.to_string())?;
            let before = obj(&[
                ("profile", s("TL-MAIN-ACTUAL-ROW-CONTROL-13-1")),
                ("financial", s(&hex(&financial))),
                ("physical", physical_value(&expected.physical)),
            ]);
            store
                .sync_command_intent(store_pin, raw.as_bytes(), before, receipt_budget)
                .map_err(|e| e.to_string())
        })();
        let permit = match checked {
            Ok(permit) => permit,
            Err(cause) => {
                return Err(Box::new(RejectedNativeMainRowSessionV1 {
                    cause,
                    owner: self,
                    store,
                }))
            }
        };
        // Raw intent is already synced. Even a header refusal retains its
        // owner/store/permit in this controller, never silently abandons intent.
        let begun = match &command {
            NativeMainSessionCommand::ProcessRows { rows, height, live } => self
                .financial
                .native_main_runtime_mut()
                .native_main_rows_begin(rows.clone(), height.clone(), *live)
                .map(|frame| NativeMainOwnedFrameV1::Rows(Box::new(frame))),
            NativeMainSessionCommand::ConstructConsensusFromIndex { start } => self
                .financial
                .native_main_runtime_mut()
                .native_main_catchup_begin(start.clone())
                .map(|frame| NativeMainOwnedFrameV1::Catchup(Box::new(frame))),
            _ => unreachable!("checked raw source command"),
        };
        let (frame, source_start_error) = match begun {
            Ok(frame) => (Some(frame), None),
            Err(error) => (None, Some(error)),
        };
        Ok(SourceMainNativeRowSessionV1 {
            owner: self,
            store,
            permit,
            frame,
            source_start_error,
            pc: NativeMainRowControllerPcV1::Source,
            receipt_budget,
            command,
            reopened: None,
            supersession: None,
            superseding_active: false,
            prefix_ack: None,
            prefix_ack_recovered: None,
        })
    }
}
impl SourceMainNativeRowSessionV1 {
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.owner.financial.runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        &self.owner.index
    }
    pub fn store_pin(&self) -> Result<NativeIntentStorePinV1, String> {
        self.store.pin().map_err(|e| e.to_string())
    }
    pub fn result(&self) -> Option<&Result<DependencyReturn, SourceRuleError>> {
        if !matches!(self.pc, NativeMainRowControllerPcV1::Complete) {
            return None;
        }
        self.frame.as_ref()?.result()
    }
    pub fn validity(&self) -> &[StreamedValidityEvaluation] {
        self.frame.as_ref().map_or(&[], |frame| frame.validity())
    }
    pub fn consumed_rows(&self) -> u64 {
        self.frame.as_ref().map_or(0, |frame| frame.consumed_rows())
    }
    pub fn grouping(&self) -> Option<&RowStreamReceipt> {
        self.frame.as_ref()?.grouping()
    }
    pub fn effects(&self) -> &[V] {
        self.owner.financial.runtime().native_main_row_effects()
    }
    pub fn pending_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedValidityWriteV1> {
        self.frame.as_ref()?.pending_write()
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s("TL-MAIN-ACTUAL-ROW-CONTROL-13-1"))?;
        visitor(
            "controllerPc",
            &s(match self.pc {
                NativeMainRowControllerPcV1::Source => "Source",
                NativeMainRowControllerPcV1::NativeIntent => "NativeIntent",
                NativeMainRowControllerPcV1::NativeAppendAttempted => "NativeAppendAttempted",
                NativeMainRowControllerPcV1::NativePost => "NativePost",
                NativeMainRowControllerPcV1::RecoveryBefore => "RecoveryBefore",
                NativeMainRowControllerPcV1::SupersessionPrepared => "SupersessionPrepared",
                NativeMainRowControllerPcV1::SupersessionStoreAttempted => {
                    "SupersessionStoreAttempted"
                }
                NativeMainRowControllerPcV1::PrefixAckPrepared => "PrefixAckPrepared",
                NativeMainRowControllerPcV1::PrefixAckAttempted => "PrefixAckAttempted",
                NativeMainRowControllerPcV1::PrefixAckSynced => "PrefixAckSynced",
                NativeMainRowControllerPcV1::SupersessionIntentSynced => "SupersessionIntentSynced",
                NativeMainRowControllerPcV1::Accepted => "Accepted",
                NativeMainRowControllerPcV1::Complete => "Complete",
                NativeMainRowControllerPcV1::Blocked(_) => "Blocked",
            }),
        )?;
        if let NativeMainRowControllerPcV1::Blocked(error) = &self.pc {
            visitor("controllerError", &s(error))?;
        }
        if let Some(error) = &self.source_start_error {
            visitor("sourceStartError", &s(&format!("{error:?}")))?;
        }
        if self.reopened.is_some() {
            native_main_reopen_fields(self.reopened.as_ref(), visitor)?;
        }
        if self.supersession.is_some() || self.superseding_active {
            native_main_supersession_fields(
                self.supersession.as_ref(),
                self.superseding_active,
                visitor,
            )?;
        }
        if matches!(
            self.pc,
            NativeMainRowControllerPcV1::PrefixAckPrepared
                | NativeMainRowControllerPcV1::PrefixAckAttempted
        ) || self.prefix_ack_recovered.is_some()
        {
            native_main_prefix_ack_fields(
                self.prefix_ack.as_ref(),
                self.prefix_ack_recovered.as_ref(),
                visitor,
            )?;
        }
        match &self.command {
            NativeMainSessionCommand::ProcessRows { rows, height, live } => {
                visitor("command/kind", &s("ProcessRows"))?;
                crate::source_contract_runtime::native_main_row_visit_value(
                    "command/rows",
                    rows,
                    visitor,
                )?;
                crate::source_contract_runtime::native_main_row_visit_value(
                    "command/height",
                    height,
                    visitor,
                )?;
                visitor("command/live", &V::Bool(*live))?;
            }
            NativeMainSessionCommand::ConstructConsensusFromIndex { start } => {
                visitor("command/kind", &s("ConstructConsensusFromIndex"))?;
                crate::source_contract_runtime::native_main_row_visit_value(
                    "command/start",
                    start,
                    visitor,
                )?;
            }
            _ => unreachable!("actual checked source command"),
        }
        if let Some(frame) = &self.frame {
            frame.visit_canonical_fields(&mut |path, value| {
                visitor(&format!("frame/{path}"), value)
            })?;
        }
        visitor("effects/count", &s(&self.effects().len().to_string()))?;
        for (at, effect) in self.effects().iter().enumerate() {
            crate::source_contract_runtime::native_main_row_visit_value(
                &format!("effects/{at}"),
                effect,
                visitor,
            )?;
        }
        Ok(())
    }
    fn observe_control(&self, retain: bool) -> Result<NativeFieldCollector, String> {
        let mut collector =
            NativeFieldCollector::new("Native13/ActualRowControl/v1", retain, self.receipt_budget);
        self.visit_canonical_fields(&mut |path, value| collector.emit(path, value))
            .map_err(|e| e.to_string())?;
        Ok(collector)
    }
    fn block(&mut self, error: String) -> NativeMainRowStopV1 {
        self.pc = NativeMainRowControllerPcV1::Blocked(error.clone());
        NativeMainRowStopV1::StorageStopped(error)
    }
    pub fn advance(&mut self) -> NativeMainRowStopV1 {
        if matches!(
            self.pc,
            NativeMainRowControllerPcV1::PrefixAckSynced
                | NativeMainRowControllerPcV1::SupersessionIntentSynced
        ) {
            return self.resume_recovered_control_post();
        }
        match &self.pc {
            NativeMainRowControllerPcV1::Complete => return NativeMainRowStopV1::Complete,
            NativeMainRowControllerPcV1::Blocked(error) => {
                return NativeMainRowStopV1::StorageStopped(error.clone())
            }
            NativeMainRowControllerPcV1::Source => (),
            _ => {
                return NativeMainRowStopV1::StorageStopped(
                    "native intent await requires exact post acknowledgement".into(),
                )
            }
        }
        if let Some(error) = &self.source_start_error {
            return NativeMainRowStopV1::SourceStopped(error.clone());
        }
        let frame = self.frame.as_mut().expect("actual source frame");
        let stop = frame.advance(
            self.owner.financial.native_main_runtime_mut(),
            &mut self.owner.index,
        );
        if stop != NativeMainRowStopV1::Complete {
            return stop;
        }
        let collected = match self.observe_control(true) {
            Ok(collected) => collected,
            Err(error) => return self.block(error),
        };
        let financial = match self.owner.financial.native_main_financial_commitment() {
            Ok(financial) => financial,
            Err(error) => return self.block(error.to_string()),
        };
        let after = obj(&[
            ("profile", s("TL-MAIN-ACTUAL-ROW-CONTROL-13-1")),
            ("financial", s(&hex(&financial))),
            ("controlFields", s(&collected.summary.fields.to_string())),
            ("controlBytes", s(&collected.summary.bytes.to_string())),
            ("controlCommitment", s(&hex(&collected.summary.commitment))),
        ]);
        if let Err(error) = self.store.sync_accepted(
            &mut self.permit,
            collected.frames.as_ref().expect("retained scalar frames"),
            after,
        ) {
            return self.block(error.to_string());
        }
        self.pc = NativeMainRowControllerPcV1::Accepted;
        if let Err(error) = self.store.sync_ack(&mut self.permit) {
            return self.block(error.to_string());
        }
        self.pc = NativeMainRowControllerPcV1::Complete;
        NativeMainRowStopV1::Complete
    }
    pub fn prepare_pending(&mut self) -> NativeMainRowStopV1 {
        if !matches!(self.pc, NativeMainRowControllerPcV1::Source) {
            return self.advance();
        }
        let collected = match self.observe_control(false) {
            Ok(collected) => collected,
            Err(error) => return self.block(error),
        };
        let financial = match self.owner.financial.native_main_financial_commitment() {
            Ok(financial) => financial,
            Err(error) => return self.block(error.to_string()),
        };
        let Some(write) = self.pending_write() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let context = obj(&[
            ("actualRead", write.durable_read_context()),
            ("financial", s(&hex(&financial))),
            ("controlFields", s(&collected.summary.fields.to_string())),
            ("controlBytes", s(&collected.summary.bytes.to_string())),
            ("controlCommitment", s(&hex(&collected.summary.commitment))),
        ]);
        let candidates =
            match NativeWriteIntentCandidatesV1::from_prepared_append(write.append_plan(), context)
            {
                Ok(candidates) => candidates,
                Err(error) => return self.block(error.to_string()),
            };
        if let Err(error) = self.store.sync_native_intent(&mut self.permit, &candidates) {
            return self.block(error.to_string());
        }
        self.pc = NativeMainRowControllerPcV1::NativeIntent;
        NativeMainRowStopV1::AwaitNativeWrite
    }
    pub fn commit_pending(&mut self) -> NativeMainRowStopV1 {
        if matches!(
            self.pc,
            NativeMainRowControllerPcV1::SupersessionIntentSynced
        ) {
            let stop = self.resume_recovered_control_post();
            if stop != NativeMainRowStopV1::AwaitNativeWrite {
                return stop;
            }
        }
        if matches!(self.pc, NativeMainRowControllerPcV1::RecoveryBefore) {
            let stop = self.prepare_superseding();
            if stop != NativeMainRowStopV1::AwaitNativeWrite {
                return stop;
            }
        }
        if matches!(self.pc, NativeMainRowControllerPcV1::SupersessionPrepared) {
            let stop = self.sync_prepared_superseding();
            if stop != NativeMainRowStopV1::AwaitNativeWrite {
                return stop;
            }
        }
        if matches!(self.pc, NativeMainRowControllerPcV1::Source) {
            let stop = self.prepare_pending();
            if stop != NativeMainRowStopV1::AwaitNativeWrite {
                return stop;
            }
        }
        if !matches!(self.pc, NativeMainRowControllerPcV1::NativeIntent) {
            return self.resolve_pending_post();
        }
        // Even an ambiguous native result must never reexecute this append.
        self.pc = NativeMainRowControllerPcV1::NativeAppendAttempted;
        let stop = self.frame.as_mut().expect("actual source frame").append(
            self.owner.financial.native_main_runtime_mut(),
            &mut self.owner.index,
        );
        if stop != NativeMainRowStopV1::AwaitNativeWrite {
            return stop;
        }
        self.pc = NativeMainRowControllerPcV1::NativePost;
        self.resolve_pending_post()
    }
    pub fn resolve_pending_post(&mut self) -> NativeMainRowStopV1 {
        self.native_main_resolve_prepared_prefix_ack()
    }
}
