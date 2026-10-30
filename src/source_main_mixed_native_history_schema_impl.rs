const MIXED_ORDINARY_FACT_FIELDS: &[&str] = &[
    "mixedOperationKind",
    "requiredOrdinaryNativeFactProfile",
    "ordinaryPendingPlanBefore",
    "ordinarySelectionBefore",
    "ordinaryPendingPlanAfter",
    "ordinarySelectionAfter",
    "ordinaryNativeIoFact",
    "ordinaryNativeIoPendingFact",
    "ordinaryIoSelection",
    "ordinarySourceSelection",
    "ordinaryPlannedReplacementSelection",
    "ordinaryNativeReadContext",
    "ordinaryNativeBeforeHistory",
    "ordinaryNativeCandidates",
    "ordinaryNativeMutations",
    "ordinaryNativeIoChain",
];
#[derive(Clone, Copy, PartialEq, Eq)]
enum MixedNativeHistoryOperationV1 {
    Source,
    Inline,
    Validity,
}
impl MixedNativeHistoryOperationV1 {
    fn from_record(record: &V) -> Result<Self> {
        if record.get("profile") != s(MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE)
            || record.get("requiredNativeFactProfile") != s(ACTUAL_NATIVE_FACT_PROFILE)
            || record.get("requiredOrdinaryNativeFactProfile")
                != s(ACTUAL_VALIDITY_PLAN_FACT_PROFILE)
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let kind = match history_text(&record.get("mixedOperationKind"))? {
            "SourceAction" => Self::Source,
            "InlineDatabase" => Self::Inline,
            "OrdinaryValidity" => Self::Validity,
            _ => return Err(ActiveServiceWalErrorV1::WrongContext),
        };
        let inline = record.get("actualSourceSelection") != V::Undefined;
        let ordinary = record.get("ordinarySourceSelection") != V::Undefined;
        if (kind == Self::Inline) != inline || (kind == Self::Validity) != ordinary {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        if kind != Self::Inline && record.get("actualNativeIoFact") != V::Undefined
            || kind != Self::Validity && record.get("ordinaryNativeIoFact") != V::Undefined
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        Ok(kind)
    }
    fn io_key(self) -> &'static str {
        match self {
            Self::Validity => "ordinaryNativeIoFact",
            _ => "actualNativeIoFact",
        }
    }
    fn pending_key(self) -> &'static str {
        match self {
            Self::Validity => "ordinaryNativeIoPendingFact",
            _ => "actualNativeIoPendingFact",
        }
    }
    fn replacement_key(self) -> &'static str {
        match self {
            Self::Validity => "ordinaryPlannedReplacementSelection",
            _ => "plannedReplacementSelection",
        }
    }
    fn chain_key(self) -> &'static str {
        match self {
            Self::Validity => "ordinaryNativeIoChain",
            _ => "actualNativeIoChain",
        }
    }
}
fn mixed_history_require_origin(origin: &V) -> Result<()> {
    history_keys(
        origin,
        &[
            "profile",
            "archive",
            "observation",
            "requiredVerifiedHistory",
            "generationEscrow",
            "requiredInlineServiceProfile",
            "requiredNativeFactProfile",
            "actualNativeFactScope",
            "requiredOrdinaryNativeFactProfile",
            "ordinaryNativeFactScope",
            "underlyingSourceHistoryProfile",
            "acknowledgedWindowLimit",
        ],
        &[],
    )?;
    if !mixed_native_fact_origin(origin) {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    for key in ["archive", "observation", "requiredVerifiedHistory"] {
        history_blob(&origin.get(key))?;
    }
    let escrow = origin.get("generationEscrow");
    history_keys(&escrow, &["profile", "root", "durability"], &[])?;
    if escrow.get("profile") != s("TL-COMPACT-GENERATION-ESCROW-13-1")
        || !Path::new(history_text(&escrow.get("root"))?).is_absolute()
        || !matches!(
            history_text(&escrow.get("durability"))?,
            "FileSyncOnly" | "FileAndDirectorySync"
        )
    {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    Ok(())
}
