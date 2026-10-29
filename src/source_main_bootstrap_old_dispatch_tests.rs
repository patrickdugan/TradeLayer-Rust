#[test]
fn old_unconfigured_step_dispatch_preserves_actual_old_outcomes_heads_and_checkpoint_bytes() {
    use crate::source_contract_replay::SourceContractReplayHead;
    let cases = [
        ("v2-empty", SourceContractRuntimePolicy::GlobalClearingV2, false, true, include_bytes!("../fixtures/ordered_bootstrap_step_reference/old_bootstrap_step_v2-empty.tlcr").as_slice()),
        ("v2-profile", SourceContractRuntimePolicy::GlobalClearingV2, false, false, include_bytes!("../fixtures/ordered_bootstrap_step_reference/old_bootstrap_step_v2-profile.tlcr").as_slice()),
        ("v1-profile", SourceContractRuntimePolicy::OnchainV1, false, false, include_bytes!("../fixtures/ordered_bootstrap_step_reference/old_bootstrap_step_v1-profile.tlcr").as_slice()),
        ("v11-profile", SourceContractRuntimePolicy::AmmLifecycleV11, false, false, include_bytes!("../fixtures/ordered_bootstrap_step_reference/old_bootstrap_step_v11-profile.tlcr").as_slice()),
        ("v12-unconfigured", SourceContractRuntimePolicy::MainOrchestrationV12, false, false, include_bytes!("../fixtures/ordered_bootstrap_step_reference/old_bootstrap_step_v12-unconfigured.tlcr").as_slice()),
        ("v12-configured", SourceContractRuntimePolicy::MainOrchestrationV12, true, false, include_bytes!("../fixtures/ordered_bootstrap_step_reference/old_bootstrap_step_v12-configured.tlcr").as_slice()),
        ("v12-configured-empty", SourceContractRuntimePolicy::MainOrchestrationV12, true, true, include_bytes!("../fixtures/ordered_bootstrap_step_reference/old_bootstrap_step_v12-configured-empty.tlcr").as_slice()),
    ];
    for (label, policy, configured, empty, golden) in cases {
        let mut replay = SourceContractReplay::with_policy(policy).unwrap();
        if configured {
            invoke(
                &mut replay,
                "SourceMain.configureProduction",
                configuration().get("production"),
            );
        }
        let before = replay.runtime().state_value().unwrap();
        let receipt = replay
            .apply(SourceContractCommand::Invoke(DependencyCall {
                target: "SourceMainBootstrap.step".into(),
                arguments: if empty {
                    vec![]
                } else {
                    vec![command("ImportTxUtils")]
                },
            }))
            .unwrap();
        assert_eq!(
            receipt.outcome,
            SourceContractCommandOutcome::LocalError(SourceRuleError::UnknownDependency),
            "{label}"
        );
        assert_eq!(replay.runtime().state_value().unwrap(), before, "{label}");
        let row = include_str!(
            "../fixtures/ordered_bootstrap_step_reference/old_bootstrap_step_heads.tsv"
        )
        .lines()
        .find(|row| row.starts_with(&format!("{label}\t")))
        .unwrap();
        let fields = row.split('\t').collect::<Vec<_>>();
        let commitment = fields[2]
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let expected = SourceContractReplayHead {
            commands: fields[1].parse().unwrap(),
            commitment,
        };
        assert_eq!(replay.head(), expected, "{label}");
        assert_eq!(replay.checkpoint().unwrap(), golden, "{label}");
        let restored = SourceContractReplay::from_checkpoint(golden, expected).unwrap();
        assert_eq!(
            restored.checkpoint().unwrap(),
            golden,
            "old recorded refusal must remain the same outcome: {label}"
        );
    }
}
