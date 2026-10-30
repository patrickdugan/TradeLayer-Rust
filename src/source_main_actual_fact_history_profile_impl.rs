// Private exact dispatch. Existing public HISTORY/Mode4 constructors select
// their old variants; only the new branded reader may select ActualSourceFacts.
#[derive(Clone, Copy)]
enum ExactAckHistoryProfileV1 {
    History,
    Mode4,
    ActualSourceFacts,
}
impl ExactAckHistoryProfileV1 {
    fn inline(self) -> bool {
        !matches!(self, Self::History)
    }
    fn facts(self) -> bool {
        matches!(self, Self::ActualSourceFacts)
    }
    fn profile(self) -> &'static str {
        match self {
            Self::History => ENHANCED_HISTORY_PROFILE,
            Self::Mode4 => MODE4_INLINE_WAL_PROFILE,
            Self::ActualSourceFacts => ACTUAL_SOURCE_FACT_WAL_PROFILE,
        }
    }
}
const ACTUAL_SOURCE_FACT_ENTRY_FIELDS: &[&str] = &[
    "requiredNativeFactProfile",
    "actualPendingPlanBefore",
    "actualSelectionBefore",
    "actualPendingPlanAfter",
    "actualSelectionAfter",
    "actualNativeIoFact",
    "actualNativeIoPendingFact",
    "actualIoSelection",
    "actualSourceSelection",
    "plannedReplacementSelection",
    "actualNativeReadContext",
    "actualNativeBeforeHistory",
    "actualNativeCandidates",
    "actualNativeMutations",
    "actualNativeIoChain",
];
fn actual_source_fact_origin(origin: &V) -> Result<()> {
    if origin.get("profile") != s(ACTUAL_SOURCE_FACT_WAL_PROFILE)
        || origin.get("requiredNativeFactProfile") != s(ACTUAL_NATIVE_FACT_PROFILE)
        || origin.get("actualNativeFactScope") != s(ACTUAL_SOURCE_FACT_SCOPE)
    {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    // Reuse only the existing exact INLINE/escrow schema validator, never a
    // branded old reader/pin. The actual origin bytes/profile stay unchanged.
    let mut shape = origin.clone();
    shape
        .insert("profile", s(MODE4_INLINE_WAL_PROFILE))
        .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
    mode4_require_origin(&shape)
}
fn actual_source_fact_selection(value: &V) -> Result<()> {
    history_keys(
        value,
        &[
            "profile",
            "sourceProfile",
            "originCommands",
            "originHead",
            "originPhysical",
            "serviceActions",
            "serviceHead",
            "pending",
        ],
        &[],
    )?;
    if value.get("profile") != s("TL-MAIN-ACTUAL-SINGLE-INLINE-SELECTION-13-1")
        || value.get("sourceProfile") != s(MODE4_INLINE_SERVICE_PROFILE)
    {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    history_count(&value.get("originCommands"))?;
    history_count(&value.get("serviceActions"))?;
    history_hash(&value.get("originHead"))?;
    history_hash(&value.get("serviceHead"))?;
    if !matches!(value.get("originPhysical"), V::Object(_)) {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    let pending = value.get("pending");
    history_keys(
        &pending,
        &[
            "profile",
            "kind",
            "callerScope",
            "sourceJob",
            "methodScope",
            "operationOrdinal",
            "role",
            "context",
            "route",
            "caller",
        ],
        &[],
    )?;
    if pending.get("profile") != s("TL-MAIN-RETAINED-CONSENSUS-PENDING-13-1")
        || pending.get("kind") != s("ServiceInline")
        || pending.get("callerScope") != s("0")
        || pending.get("route") != V::Null
        || pending.get("caller") != V::Null
        || !matches!(history_text(&pending.get("role"))?, "Awaited" | "Detached")
    {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    for name in ["sourceJob", "methodScope", "operationOrdinal"] {
        history_count(&pending.get(name))?;
    }
    if pending.get("context").get("profile")
        != s(crate::source_compact_index_v2::INLINE_DATABASE_PROFILE)
    {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    Ok(())
}
fn actual_source_fact_blob_or_null(value: &V) -> Result<()> {
    if *value == V::Null {
        return Ok(());
    }
    if history_blob(value)?.bytes == 0 {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    Ok(())
}
fn actual_source_fact_entry_shape(value: &V) -> Result<()> {
    if value.get("requiredNativeFactProfile") != s(ACTUAL_NATIVE_FACT_PROFILE) {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    for side in ["Before", "After"] {
        let raw = value.get(&format!("actualPendingPlan{side}"));
        let selection = value.get(&format!("actualSelection{side}"));
        actual_source_fact_blob_or_null(&raw)?;
        if raw == V::Null {
            if selection != V::Null {
                return Err(ActiveServiceWalErrorV1::InvalidRecord);
            }
        } else {
            actual_source_fact_selection(&selection)?;
        }
    }
    let io = [
        "actualNativeIoFact",
        "actualNativeIoPendingFact",
        "actualIoSelection",
    ];
    if io.iter().any(|name| value.get(name) != V::Undefined) {
        for name in &io[..2] {
            let raw = value.get(name);
            actual_source_fact_blob_or_null(&raw)?;
            if raw == V::Null {
                return Err(ActiveServiceWalErrorV1::InvalidRecord);
            }
        }
        actual_source_fact_selection(&value.get(io[2]))?;
    }
    let aliases = [
        "actualSourceSelection",
        "plannedReplacementSelection",
        "actualNativeReadContext",
        "actualNativeBeforeHistory",
        "actualNativeCandidates",
        "actualNativeMutations",
    ];
    if aliases.iter().any(|name| value.get(name) != V::Undefined) {
        actual_source_fact_selection(&value.get(aliases[0]))?;
        if value.get(aliases[1]) != V::Null {
            actual_source_fact_selection(&value.get(aliases[1]))?;
            if value.get("nativeRecovery") == V::Undefined {
                return Err(ActiveServiceWalErrorV1::InvalidRecord);
            }
        }
        if value.get(aliases[2]).get("profile")
            != s(crate::source_compact_index_v2::INLINE_DATABASE_PROFILE)
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let history = value.get(aliases[3]);
        actual_source_fact_blob_or_null(&history)?;
        if history == V::Null {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        let candidates = value.get(aliases[4]);
        let mutations = value.get(aliases[5]);
        actual_source_fact_blob_or_null(&candidates)?;
        actual_source_fact_blob_or_null(&mutations)?;
        if (candidates == V::Null) != (mutations == V::Null) {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
    }
    if value.get("actualNativeIoChain") != V::Undefined {
        let chain = value.get("actualNativeIoChain");
        actual_source_fact_blob_or_null(&chain)?;
        if chain == V::Null || value.get("actualNativeIoFact") == V::Undefined {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
    }
    Ok(())
}
