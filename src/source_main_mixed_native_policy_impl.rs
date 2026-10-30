pub const MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE: &str =
    "TL-MAIN-SEGMENTED-MIXED-NATIVE-FACTS-WAL-13-1";
pub const MIXED_SOURCE_NATIVE_FACT_SCOPE: &str = "StandaloneMixedInlineValiditySource-1";
fn mixed_native_fact_origin(origin: &V) -> bool {
    origin.get("profile") == s(MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE)
        && origin.get("actualNativeFactScope") == s(MIXED_SOURCE_NATIVE_FACT_SCOPE)
        && origin.get("requiredNativeFactProfile") == s(ACTUAL_NATIVE_FACT_PROFILE)
        && origin.get("requiredOrdinaryNativeFactProfile") == s(ACTUAL_VALIDITY_PLAN_FACT_PROFILE)
        && origin.get("ordinaryNativeFactScope") == s(ACTUAL_VALIDITY_FACT_SCOPE)
        && origin.get("requiredInlineServiceProfile")
            == s(crate::source_contract_runtime::NATIVE_MAIN_INLINE_ACTIVE_SERVICE_PROFILE)
        && origin.get("underlyingSourceHistoryProfile") == s(MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE)
        && origin.get("acknowledgedWindowLimit") == s("256")
}
