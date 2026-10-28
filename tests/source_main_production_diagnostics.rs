use tradelayer_core::source_contract_replay::SourceContractReplay;
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
const OLD: &str = include_str!("../fixtures/source_main_legacy_debug.tsv");
fn decode(v: &str) -> Vec<u8> {
    assert_eq!(v.len() % 2, 0);
    v.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn all_legacy_runtime_debug_state_and_checkpoint_bytes_match_independent_pre_v12_provider() {
    let rows = OLD
        .lines()
        .filter(|s| !s.starts_with('#'))
        .map(|s| s.split('\t').collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 33);
    for (i, p) in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
        P::ProofLifecycleV6,
        P::LowerLifecycleV7,
        P::MainCallableV8,
        P::SpotLifecycleV9,
        P::ActivationLifecycleV10,
        P::AmmLifecycleV11,
    ]
    .into_iter()
    .enumerate()
    {
        let get = |kind: &str| {
            let n = (i + 1).to_string();
            let matching = rows
                .iter()
                .filter(|r| r[0] == kind && r[1] == n)
                .collect::<Vec<_>>();
            assert_eq!(matching.len(), 1);
            decode(matching[0][2])
        };
        let rt = SourceContractRuntime::with_policy(p);
        assert_eq!(
            format!("{rt:?}").as_bytes(),
            get("debug"),
            "complete independent original debug policy{}",
            i + 1
        );
        assert_eq!(
            rt.state_value()
                .unwrap()
                .diagnostic_json()
                .unwrap()
                .as_bytes(),
            get("state"),
            "complete original state policy{}",
            i + 1
        );
        assert_eq!(
            SourceContractReplay::with_policy(p)
                .unwrap()
                .checkpoint()
                .unwrap(),
            get("checkpoint"),
            "exact original checkpoint policy{}",
            i + 1
        );
    }
}
#[test]
fn new_profile_diagnostic_exposes_its_additional_private_owner() {
    let rt = SourceContractRuntime::with_policy(P::MainOrchestrationV12);
    assert!(format!("{rt:?}").contains("main_production: None"));
}
