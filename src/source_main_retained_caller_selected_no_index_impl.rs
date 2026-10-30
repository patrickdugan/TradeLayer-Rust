// Whole-owner move. This carrier contains the SAME Replay and source frames;
// it never constructs a temporary index or imports current financial state.
enum RetainedCallerSelectedReplacementV1 {
    Inline {
        write: Box<crate::source_compact_index_v2::NativePreparedInlineDatabaseV1>,
        mapping: Box<crate::source_compact_index_v2::NativeInlineSupersessionV1>,
    },
    Validity {
        write: Box<crate::source_compact_index_v2::NativePreparedValidityWriteV1>,
        mapping: Box<crate::source_compact_index_v2::NativePreparedValiditySupersessionV1>,
    },
}
struct RetainedCallerSelectedPartsV1 {
    frame: Option<Box<crate::source_contract_runtime::NativeMainRetainedCallerFrameV1>>,
    source_start_error: Option<SourceRuleError>,
    documents: V,
    origin: NativeMainSessionExpectation,
    command: NativeMainRetainedCallerV1,
    scope: u64,
    generation: u64,
}
struct RetainedCallerSelectedConsensusPartsV1 {
    service: Box<crate::source_contract_runtime::NativeMainServiceContinuationV1>,
    attempted: Option<NativeMainRetainedConsensusPendingV1>,
    route: NativeMainRetainedCallerRouteV1,
}
struct RetainedCallerSelectedWithoutIndexV1 {
    financial: NativeMainFinancialWithoutIndexV1,
    namespace: crate::storage::Namespace,
    caller: RetainedCallerSelectedPartsV1,
    consensus: Option<RetainedCallerSelectedConsensusPartsV1>,
}
impl RetainedCallerSelectedOwnerV1 {
    fn into_without_index(self) -> RetainedCallerSelectedWithoutIndexV1 {
        let (caller, consensus) = match self {
            Self::Caller(c) => (*c, None),
            Self::Consensus(c) => {
                let SourceMainNativeRetainedConsensusV1 {
                    caller,
                    service,
                    attempted,
                    route,
                } = *c;
                (
                    caller,
                    Some(RetainedCallerSelectedConsensusPartsV1 {
                        service,
                        attempted,
                        route,
                    }),
                )
            }
        };
        let SourceMainNativeRetainedCallerV1 {
            owner,
            frame,
            source_start_error,
            documents,
            origin,
            command,
            scope,
            generation,
        } = caller;
        let SourceMainNativeSessionV13 {
            financial,
            index,
            foundation,
            foundation_head,
            origin_rows,
            binding,
            origin_physical,
            head,
            history,
            halted,
        } = owner;
        let namespace = index.native_namespace().clone();
        drop(index); // ONLY old mutable index.
        RetainedCallerSelectedWithoutIndexV1 {
            financial: NativeMainFinancialWithoutIndexV1 {
                financial,
                foundation,
                foundation_head,
                origin_rows,
                binding,
                origin_physical,
                head,
                history,
                halted,
            },
            namespace,
            caller: RetainedCallerSelectedPartsV1 {
                frame,
                source_start_error,
                documents,
                origin,
                command,
                scope,
                generation,
            },
            consensus,
        }
    }
}
impl RetainedCallerSelectedWithoutIndexV1 {
    fn native(
        &self,
        operation: &NativeMainRetainedConsensusPendingV1,
    ) -> CallerSelectedResult<RetainedCallerSelectedNativeRefV1<'_>> {
        use crate::source_contract_runtime::NativeMainRetainedConsensusOperationKindV1 as Kind;
        match operation.kind() {
            Kind::CallerInline => self
                .caller
                .frame
                .as_ref()
                .and_then(|f| f.pending_write())
                .map(RetainedCallerSelectedNativeRefV1::Inline),
            Kind::ServiceInline => self
                .consensus
                .as_ref()
                .and_then(|c| c.service.inline_pending_write())
                .map(RetainedCallerSelectedNativeRefV1::Inline),
            Kind::ServiceValidity => self
                .consensus
                .as_ref()
                .and_then(|c| c.service.pending_write())
                .map(RetainedCallerSelectedNativeRefV1::Validity),
        }
        .ok_or(RetainedCallerSelectedWalErrorV1::WrongSelector)
    }
    fn rejoin(self, index: SourceCompactIndex) -> RetainedCallerSelectedOwnerV1 {
        let Self {
            financial,
            namespace: _,
            caller,
            consensus,
        } = self;
        let NativeMainFinancialWithoutIndexV1 {
            financial,
            foundation,
            foundation_head,
            origin_rows,
            binding,
            origin_physical,
            head,
            history,
            halted,
        } = financial;
        let RetainedCallerSelectedPartsV1 {
            frame,
            source_start_error,
            documents,
            origin,
            command,
            scope,
            generation,
        } = caller;
        let caller = SourceMainNativeRetainedCallerV1 {
            owner: SourceMainNativeSessionV13 {
                financial,
                index,
                foundation,
                foundation_head,
                origin_rows,
                binding,
                origin_physical,
                head,
                history,
                halted,
            },
            frame,
            source_start_error,
            documents,
            origin,
            command,
            scope,
            generation,
        };
        match consensus {
            None => RetainedCallerSelectedOwnerV1::Caller(Box::new(caller)),
            Some(RetainedCallerSelectedConsensusPartsV1 {
                service,
                attempted,
                route,
            }) => RetainedCallerSelectedOwnerV1::Consensus(Box::new(
                SourceMainNativeRetainedConsensusV1 {
                    caller,
                    service,
                    attempted,
                    route,
                },
            )),
        }
    }
}
pub struct SourceMainNativeSelectedCallerNoIndexV1 {
    controller: SourceMainNativeDurableSelectedCallerV1,
    actual: RetainedCallerSelectedWithoutIndexV1,
}
pub struct RejectedSelectedCallerNoIndexV1 {
    pub error: RetainedCallerSelectedWalErrorV1,
    controller: SourceMainNativeDurableSelectedCallerV1,
}
impl RejectedSelectedCallerNoIndexV1 {
    pub fn into_controller(self) -> SourceMainNativeDurableSelectedCallerV1 {
        self.controller
    }
}
impl core::fmt::Debug for RejectedSelectedCallerNoIndexV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedSelectedCallerNoIndexV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
pub struct RejectedSelectedCallerNativeReopenV1 {
    pub error: RetainedCallerSelectedWalErrorV1,
    owner: SourceMainNativeSelectedCallerNoIndexV1,
}
impl RejectedSelectedCallerNativeReopenV1 {
    pub fn into_owner(self) -> SourceMainNativeSelectedCallerNoIndexV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedSelectedCallerNativeReopenV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedSelectedCallerNativeReopenV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl SourceMainNativeDurableSelectedCallerV1 {
    pub fn into_no_native_index(
        mut self,
    ) -> Result<SourceMainNativeSelectedCallerNoIndexV1, Box<RejectedSelectedCallerNoIndexV1>> {
        let checked = (|| -> CallerSelectedResult<()> {
            if self.phase != RetainedCallerSelectedWalPhaseV1::NativeRecoveryRequired {
                return Err(RetainedCallerSelectedWalErrorV1::WrongAction);
            }
            let p = self
                .pending
                .as_ref()
                .ok_or(RetainedCallerSelectedWalErrorV1::WrongAction)?;
            if !p.executed||p.plan.is_some()||!matches!(p.action,crate::source_main_retained_caller_action::RetainedMainCallerActionV1::AppendSelected{..}){return Err(RetainedCallerSelectedWalErrorV1::WrongAction)}
            let actual = self
                .owner
                .as_ref()
                .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?
                .native(
                    p.operation
                        .as_ref()
                        .ok_or(RetainedCallerSelectedWalErrorV1::WrongSelector)?,
                )?;
            if actual.append().is_none() {
                return Err(RetainedCallerSelectedWalErrorV1::WrongAction);
            }
            Ok(())
        })();
        if let Err(error) = checked {
            return Err(Box::new(RejectedSelectedCallerNoIndexV1 {
                error,
                controller: self,
            }));
        }
        let actual = self
            .owner
            .take()
            .expect("checked actual owner")
            .into_without_index();
        Ok(SourceMainNativeSelectedCallerNoIndexV1 {
            controller: self,
            actual,
        })
    }
}
impl SourceMainNativeSelectedCallerNoIndexV1 {
    pub fn effects(&self) -> &[V] {
        self.actual
            .financial
            .financial
            .runtime()
            .native_main_row_effects()
    }
    fn operation(&self) -> &NativeMainRetainedConsensusPendingV1 {
        self.controller
            .pending
            .as_ref()
            .and_then(|p| p.operation.as_ref())
            .expect("retained actual operation")
    }
    pub fn try_reopen_post(
        self,
        root: &std::path::Path,
        recovery: crate::storage::Recovery,
    ) -> Result<SourceMainNativeDurableSelectedCallerV1, Box<RejectedSelectedCallerNativeReopenV1>>
    {
        self.try_reopen(
            root,
            crate::source_compact_index_v2::NativePreparedValidityCandidateV1::Post,
            recovery,
        )
    }
    pub fn try_reopen(
        self,
        root: &std::path::Path,
        candidate: crate::source_compact_index_v2::NativePreparedValidityCandidateV1,
        recovery: crate::storage::Recovery,
    ) -> Result<SourceMainNativeDurableSelectedCallerV1, Box<RejectedSelectedCallerNativeReopenV1>>
    {
        use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
        let opened=(||->CallerSelectedResult<(SourceCompactIndex,V,Option<RetainedCallerSelectedReplacementV1>)>{
            match self.actual.native(self.operation())? {
                RetainedCallerSelectedNativeRefV1::Inline(original)=>{
                    let (index,observed)=SourceCompactIndex::reopen_inline_database(root,self.actual.namespace.clone(),original,candidate,recovery).map_err(caller_selected_backend)?;
                    if candidate==Candidate::Post{index.resolve_inline_database_post(original).map_err(caller_selected_backend)?;Ok((index,V::Null,None))}
                    else{let (replacement,mapping)=index.supersede_inline_database(original,&observed).map_err(caller_selected_backend)?;let binding=index.require_inline_native_supersession(original,&observed,&replacement,&mapping).map_err(caller_selected_backend)?;
                        let parts=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(replacement.append_plan().ok_or(RetainedCallerSelectedWalErrorV1::WrongAction)?,replacement.durable_read_context()).map_err(caller_selected_backend)?;
                        let proof=obj(&[("binding",binding),("replacementCandidates",parts.independently_retained_facts().clone()),("replacementMutations",CallerStartBlob::of(parts.exact_prepared_mutations())?.value())]);
                        Ok((index,proof,Some(RetainedCallerSelectedReplacementV1::Inline{write:Box::new(replacement),mapping:Box::new(mapping)})))
                    }
                }
                RetainedCallerSelectedNativeRefV1::Validity(original)=>{
                    let (index,observed)=SourceCompactIndex::reopen_prepared_validity(root,self.actual.namespace.clone(),original,candidate,recovery).map_err(caller_selected_backend)?;
                    if candidate==Candidate::Post{index.resolve_prepared_validity_post(original).map_err(caller_selected_backend)?;Ok((index,V::Null,None))}
                    else{let (replacement,mapping)=index.supersede_prepared_validity(original,&observed).map_err(caller_selected_backend)?;let binding=index.require_actual_native_supersession(original,&observed,&replacement,&mapping).map_err(caller_selected_backend)?;
                        let parts=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(replacement.append_plan(),replacement.durable_read_context()).map_err(caller_selected_backend)?;
                        let proof=obj(&[("binding",binding),("replacementCandidates",parts.independently_retained_facts().clone()),("replacementMutations",CallerStartBlob::of(parts.exact_prepared_mutations())?.value())]);
                        Ok((index,proof,Some(RetainedCallerSelectedReplacementV1::Validity{write:Box::new(replacement),mapping:Box::new(mapping)})))
                    }
                }
            }
        })();
        let (index, mapping, replacement) = match opened {
            Ok(result) => result,
            Err(error) => {
                return Err(Box::new(RejectedSelectedCallerNativeReopenV1 {
                    error,
                    owner: self,
                }))
            }
        };
        let checked = (|| -> CallerSelectedResult<(Vec<u8>, Option<CallerStartBlob>)> {
            let physical = physical(&index).map_err(caller_selected_backend)?;
            let prior = self
                .controller
                .pending
                .as_ref()
                .and_then(|p| p.native_recovery.as_ref())
                .map(|raw| CallerStartBlob::of(raw))
                .transpose()?;
            let proof = caller_selected_bytes(&obj(&[
                ("profile", s("TL-MAIN-SELECTED-CALLER-NATIVE-RECOVERY-13-1")),
                (
                    "candidate",
                    s(match candidate {
                        Candidate::Before => "Before",
                        Candidate::Intermediate => "Intermediate",
                        Candidate::Post => "Post",
                    }),
                ),
                (
                    "previous",
                    prior.as_ref().map_or(V::Null, CallerStartBlob::value),
                ),
                (
                    "actualOriginalRoute",
                    self.controller
                        .route
                        .as_ref()
                        .expect("retained route")
                        .value()
                        .clone(),
                ),
                ("actualReopenedPhysical", physical_value(&physical)),
                ("selectedOperation", self.operation().value()),
                ("supersession", mapping),
            ]))?;
            Ok((proof, prior))
        })();
        let replacement_mutations=match replacement.as_ref(){
            None=>None,
            Some(RetainedCallerSelectedReplacementV1::Inline{write,..})=>match crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(write.append_plan().expect("checked replacement append"),write.durable_read_context()){Ok(actual)=>Some(actual.exact_prepared_mutations().to_vec()),Err(error)=>return Err(Box::new(RejectedSelectedCallerNativeReopenV1{error:caller_selected_backend(error),owner:self}))},
            Some(RetainedCallerSelectedReplacementV1::Validity{write,..})=>match crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(write.append_plan(),write.durable_read_context()){Ok(actual)=>Some(actual.exact_prepared_mutations().to_vec()),Err(error)=>return Err(Box::new(RejectedSelectedCallerNativeReopenV1{error:caller_selected_backend(error),owner:self}))},
        };
        let (proof, prior) = match checked {
            Ok(result) => result,
            Err(error) => {
                return Err(Box::new(RejectedSelectedCallerNativeReopenV1 {
                    error,
                    owner: self,
                }))
            }
        };
        let Self {
            mut controller,
            actual,
        } = self;
        controller.owner = Some(actual.rejoin(index));
        let p = controller.pending.as_mut().expect("retained pending");
        p.native_recovery = Some(proof);
        p.recovery_prior = prior;
        p.replacement = replacement;
        p.replacement_mutations = replacement_mutations;
        p.escrow_permit = None;
        controller.phase = if candidate == Candidate::Post {
            RetainedCallerSelectedWalPhaseV1::Observe
        } else {
            RetainedCallerSelectedWalPhaseV1::NativeSupersessionIntent
        };
        Ok(controller)
    }
}
