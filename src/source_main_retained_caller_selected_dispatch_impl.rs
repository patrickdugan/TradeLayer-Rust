// Private actual-owner dispatch. Only the durable controller may call this
// AFTER its raw action Intent is synced. No first-pending fallback is used.
enum RetainedCallerSelectedOwnerV1 {
    Caller(Box<SourceMainNativeRetainedCallerV1>),
    Consensus(Box<SourceMainNativeRetainedConsensusV1>),
}
enum RetainedCallerSelectedNativeRefV1<'a> {
    Inline(&'a crate::source_compact_index_v2::NativePreparedInlineDatabaseV1),
    Validity(&'a crate::source_compact_index_v2::NativePreparedValidityWriteV1),
}
impl RetainedCallerSelectedNativeRefV1<'_> {
    fn context(&self) -> V {
        match self {
            Self::Inline(w) => w.durable_read_context(),
            Self::Validity(w) => w.durable_read_context(),
        }
    }
    fn append(&self) -> Option<&crate::compact_journal::NativeAppendPlan> {
        match self {
            Self::Inline(w) => w.append_plan(),
            Self::Validity(w) => Some(w.append_plan()),
        }
    }
    fn verify_post(&self, index: &SourceCompactIndex) -> Result<(), String> {
        match self {
            Self::Inline(w) => index
                .resolve_inline_database_post(w)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Self::Validity(w) => index
                .resolve_prepared_validity_post(w)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        }
    }
}
impl RetainedCallerSelectedOwnerV1 {
    fn route_matches(&self, route: &NativeMainRetainedCallerRouteV1) -> bool {
        match self {
            Self::Caller(_) => true,
            Self::Consensus(c) => &c.route == route,
        }
    }
    fn session(&self) -> &SourceMainNativeSessionV13 {
        match self {
            Self::Caller(c) => &c.owner,
            Self::Consensus(c) => &c.caller.owner,
        }
    }
    fn session_mut(&mut self) -> &mut SourceMainNativeSessionV13 {
        match self {
            Self::Caller(c) => &mut c.owner,
            Self::Consensus(c) => &mut c.caller.owner,
        }
    }
    fn operations(
        &self,
        route: &NativeMainRetainedCallerRouteV1,
    ) -> Vec<NativeMainRetainedConsensusPendingV1> {
        match self {
            Self::Caller(c) => c
                .frame
                .as_ref()
                .and_then(|f| f.native_main_retained_consensus_descriptor())
                .map(|p| vec![p.bind_route(route)])
                .unwrap_or_default(),
            Self::Consensus(c) => {
                if &c.route == route {
                    c.pending_operations()
                } else {
                    vec![]
                }
            }
        }
    }
    fn select(
        &self,
        route: &NativeMainRetainedCallerRouteV1,
        raw: &V,
    ) -> CallerSelectedResult<NativeMainRetainedConsensusPendingV1> {
        let selected = self
            .operations(route)
            .into_iter()
            .filter(|p| p.value() == *raw)
            .collect::<Vec<_>>();
        if selected.len() != 1 {
            return Err(RetainedCallerSelectedWalErrorV1::WrongSelector);
        }
        Ok(selected
            .into_iter()
            .next()
            .expect("one exact actual selector"))
    }
    fn native<'a>(
        &'a self,
        operation: &NativeMainRetainedConsensusPendingV1,
    ) -> CallerSelectedResult<RetainedCallerSelectedNativeRefV1<'a>> {
        use crate::source_contract_runtime::NativeMainRetainedConsensusOperationKindV1 as Kind;
        let selected = match (self, operation.kind()) {
            (Self::Caller(c), Kind::CallerInline) => c
                .pending_write()
                .map(RetainedCallerSelectedNativeRefV1::Inline),
            (Self::Consensus(c), Kind::CallerInline) => c
                .caller
                .pending_write()
                .map(RetainedCallerSelectedNativeRefV1::Inline),
            (Self::Consensus(c), Kind::ServiceInline) => c
                .service
                .inline_pending_write()
                .map(RetainedCallerSelectedNativeRefV1::Inline),
            (Self::Consensus(c), Kind::ServiceValidity) => c
                .service
                .pending_write()
                .map(RetainedCallerSelectedNativeRefV1::Validity),
            _ => None,
        }
        .ok_or(RetainedCallerSelectedWalErrorV1::WrongSelector)?;
        if operation.value().get("context") != selected.context() {
            return Err(RetainedCallerSelectedWalErrorV1::WrongSelector);
        }
        Ok(selected)
    }
    fn observe(&self, fields: &mut NativeFieldCollector) -> CallerSelectedResult<()> {
        match self {
            Self::Caller(c) => c.visit_canonical_fields(&mut |p, v| fields.emit(p, v)),
            Self::Consensus(c) => c.visit_canonical_fields(&mut |p, v| fields.emit(p, v)),
        }
        .map_err(caller_selected_backend)
    }
}
fn caller_selected_action_selector(
    action: &crate::source_main_retained_caller_action::RetainedMainCallerActionV1,
) -> Option<&V> {
    use crate::source_main_retained_caller_action::RetainedMainCallerActionV1 as Action;
    match action {
        Action::AdvanceDetached { selector }
        | Action::AppendSelected { selector }
        | Action::ResolveSelectedPost { selector } => Some(selector),
        _ => None,
    }
}
fn caller_selected_execute(
    owner: RetainedCallerSelectedOwnerV1,
    route: &NativeMainRetainedCallerRouteV1,
    action: &crate::source_main_retained_caller_action::RetainedMainCallerActionV1,
    permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
) -> (RetainedCallerSelectedOwnerV1, V) {
    use crate::source_main_retained_caller_action::{
        RetainedMainCallerActionV1 as Action, RetainedMainCallerTimerLaneV1 as Lane,
    };
    // Selection failure is a control rejection. It never creates a source
    // returned value or mutates the actual financial owner.
    let operation = match caller_selected_action_selector(action) {
        Some(raw) => match owner.select(route, raw) {
            Ok(p) => Some(p),
            Err(e) => return (owner, obj(&[("apiRejection", s(&format!("{e:?}")))])),
        },
        None => None,
    };
    match owner {
        RetainedCallerSelectedOwnerV1::Caller(mut c) => {
            let result = match action {
                Action::AdvanceCaller => {
                    if c.frame
                        .as_ref()
                        .and_then(|f| f.native_main_retained_consensus_descriptor())
                        .is_some_and(|p| p.value().get("role") == s("Detached"))
                    {
                        "API rejected: explicit detached selector required".into()
                    } else {
                        format!("{:?}", c.advance())
                    }
                }
                Action::EnterConsensus => {
                    return match c.into_retained_consensus() {
                        Ok(c) => {
                            if &c.route != route {
                                (
                                    RetainedCallerSelectedOwnerV1::Consensus(Box::new(c)),
                                    obj(&[("apiRejection", s("actual retained route changed"))]),
                                )
                            } else {
                                (
                                    RetainedCallerSelectedOwnerV1::Consensus(Box::new(c)),
                                    obj(&[("enteredConsensus", V::Bool(true))]),
                                )
                            }
                        }
                        Err(rejected) => {
                            let error = format!("{:?}", rejected.error);
                            (
                                RetainedCallerSelectedOwnerV1::Caller(Box::new(
                                    rejected.into_caller(),
                                )),
                                obj(&[("apiRejection", s(&error))]),
                            )
                        }
                    }
                }
                Action::AdvanceDetached { .. } => {
                    let operation = operation.as_ref().expect("actual selection");
                    if operation.value().get("role") != s("Detached") {
                        "API rejected: detached role required".into()
                    } else {
                        format!("{:?}", c.advance())
                    }
                }
                Action::AppendSelected { .. } => {
                    let p = operation.as_ref().expect("actual selection");
                    let raw = p.value().get("caller");
                    let actual = c.pending_descriptor();
                    if actual.as_ref().map(|v| v.value()) != Some(raw) {
                        "API rejected: actual pending write required".into()
                    } else {
                        format!(
                            "{:?}",
                            c.append_pending(actual.as_ref().expect("checked"), permit)
                        )
                    }
                }
                Action::ResolveSelectedPost { .. } => {
                    let p = operation.as_ref().expect("actual selection");
                    let raw = p.value().get("caller");
                    let actual = c.pending_descriptor();
                    if actual.as_ref().map(|v| v.value()) != Some(raw) {
                        "API rejected: actual pending write required".into()
                    } else {
                        format!("{:?}", c.resolve_post(actual.as_ref().expect("checked")))
                    }
                }
                Action::FireTimer {
                    lane: Lane::CallerDelay,
                    id,
                    generation,
                } => format!("{:?}", c.fire_timer(*id, *generation)),
                Action::Pause => format!(
                    "{:?}",
                    c.owner
                        .financial
                        .native_main_runtime_mut()
                        .native_main_retained_caller_selected_pause()
                ),
                _ => "API rejected: action does not match actual caller owner".into(),
            };
            (
                RetainedCallerSelectedOwnerV1::Caller(c),
                obj(&[("actualControl", s(&result))]),
            )
        }
        RetainedCallerSelectedOwnerV1::Consensus(mut c) => {
            let result = match action {
                Action::AdvanceConsensus { limit } => format!("{:?}", c.advance_consensus(*limit)),
                Action::AdvanceDetached { .. } => format!(
                    "{:?}",
                    c.advance_detached(operation.as_ref().expect("actual selection"))
                ),
                Action::AppendSelected { .. } => format!(
                    "{:?}",
                    c.append_pending(operation.as_ref().expect("actual selection"), permit)
                ),
                Action::ResolveSelectedPost { .. } => format!(
                    "{:?}",
                    c.resolve_post(operation.as_ref().expect("actual selection"))
                ),
                Action::FireTimer {
                    lane: Lane::ConsensusService,
                    id,
                    generation,
                } => format!("{:?}", c.fire_timer(*id, *generation)),
                Action::Pause => format!("{:?}", c.pause()),
                _ => "API rejected: action does not match actual consensus owner".into(),
            };
            (
                RetainedCallerSelectedOwnerV1::Consensus(c),
                obj(&[("actualControl", s(&result))]),
            )
        }
    }
}
