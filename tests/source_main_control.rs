use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractReplay, SourceContractReplayError, MAX_COMMANDS,
};
use tradelayer_core::source_contract_runtime::{SourceContractRuntimePolicy as P, StorageFailure};
use tradelayer_core::source_main_control::{
    SourceMainControlCommandV1 as Control, SourceMainControlErrorV1 as E,
    SourceMainControlOutcomeV1 as O, SourceMainControlSessionV1 as Session,
    SourceMainProcessExitV1,
};
use tradelayer_core::validity_source::{DependencyCall, SourceValue as V};

fn policies() -> [P; 12] {
    [
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
        P::MainOrchestrationV12,
    ]
}
fn s(value: &str) -> V {
    V::String(value.into())
}
fn object(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn invoke(target: &str) -> C {
    C::Invoke(DependencyCall {
        target: target.into(),
        arguments: vec![],
    })
}
fn populated() -> SourceContractReplay {
    let mut replay = SourceContractReplay::with_policy(P::MainOrchestrationV12).unwrap();
    replay
        .apply(C::Import {
            collection: "consensus".into(),
            rows: vec![object(&[
                ("_id", s("TrackHeight")),
                ("value", V::number(17.)),
            ])],
        })
        .unwrap();
    replay
        .apply(C::FailStorage(StorageFailure {
            collection: "consensus".into(),
            operation: "updateAsync".into(),
            skip: 0,
            message: "must remain unconsumed".into(),
        }))
        .unwrap();
    replay
}

#[test]
fn actual_node_children_exit_before_return_or_microtask_and_match_terminal_prefix() {
    let root = std::env::var("TL_MAIN_CONTROL_SOURCE_ROOT")
        .unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").into());
    let out = std::process::Command::new("node")
        .env("PROOF_REPO", root)
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/main_shutdown_control_vectors.cjs"
        ))
        .arg("--check")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut observed = 0;
    for line in include_str!("../fixtures/main_shutdown_control_vectors.jsonl").lines() {
        let row = tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap();
        let args = if row.get("profile") == s("ignored-values") {
            vec![
                V::Undefined,
                V::Null,
                V::Bool(false),
                V::number(f64::NAN),
                V::number(f64::INFINITY),
                object(&[("value", s("ignored"))]),
            ]
        } else {
            vec![]
        };
        let mut session = Session::from_replay(populated());
        let before = session.replay().checkpoint().unwrap();
        let state = session.replay().runtime().state_value().unwrap();
        let head = session.replay().head();
        let receipt = session.apply(Control::MainShutdown(args)).unwrap();
        assert_eq!(
            receipt.outcome,
            O::Exited(SourceMainProcessExitV1 { code: 0 })
        );
        assert_eq!(V::Array(receipt.effects), row.get("effects"));
        assert_eq!(row.get("normalReturn"), V::Bool(false));
        assert_eq!(row.get("exitCode"), V::number(0.));
        assert_eq!(
            session.replay().head(),
            head,
            "terminal is outside legacy Invoke history"
        );
        assert_eq!(
            session.replay().checkpoint().unwrap(),
            before,
            "no DB write, pending failure consumption or generic async drain"
        );
        assert_eq!(session.replay().runtime().state_value().unwrap(), state);
        observed += 1;
    }
    assert_eq!(observed, 3);
}

#[test]
fn all_twelve_legacy_policies_keep_actual_receipts_and_invoke_bytes() {
    for policy in policies() {
        let mut expected = SourceContractReplay::with_policy(policy).unwrap();
        expected.apply(C::SetHeight(V::number(3.))).unwrap();
        let origin = expected.head();
        let mut actual = Session::from_replay(expected.clone());
        assert_eq!(actual.adopted_head(), origin);
        assert_eq!(actual.head().controls, 0);
        for command in [
            C::SetHeight(V::number(4.)),
            invoke("Main.loadTrackHeight"),
            invoke("Main.shutdown"),
        ] {
            let receipt = expected.apply(command.clone()).unwrap();
            let control = actual.apply(Control::Replay(command)).unwrap();
            assert_eq!(control.effects, receipt.effects);
            assert_eq!(control.outcome, O::Replay(Box::new(receipt)));
            assert_eq!(
                actual.replay().checkpoint().unwrap(),
                expected.checkpoint().unwrap()
            );
            assert_eq!(actual.replay().runtime(), expected.runtime());
            assert_eq!(
                actual.terminal(),
                None,
                "legacy Invoke is not reinterpreted"
            );
        }
        assert_eq!(actual.ordinary_controls().count(), 3);
        let bytes = actual.checkpoint().unwrap();
        let recovered = Session::from_checkpoint(&bytes, actual.head()).unwrap();
        assert_eq!(recovered, actual);
        assert_eq!(
            recovered.replay().checkpoint().unwrap(),
            expected.checkpoint().unwrap()
        );
    }
}

#[test]
fn terminal_body_ignores_arguments_and_needs_no_main_or_v12_configuration() {
    for policy in policies() {
        let replay = SourceContractReplay::with_policy(policy).unwrap();
        let original = replay.checkpoint().unwrap();
        let mut session = Session::from_replay(replay);
        let arguments = vec![
            V::Promise,
            V::BigNumber("NaN".into()),
            V::number(-0.),
            V::number(f64::INFINITY),
            V::Undefined,
            V::Null,
        ];
        let receipt = session
            .apply(Control::MainShutdown(arguments.clone()))
            .unwrap();
        assert_eq!(
            receipt.outcome,
            O::Exited(SourceMainProcessExitV1 { code: 0 })
        );
        assert_eq!(session.shutdown_arguments(), Some(arguments.as_slice()));
        assert_eq!(session.replay().checkpoint().unwrap(), original);
        assert_eq!(session.head().controls, 1);
        assert_eq!(receipt.effects.len(), 3);
        assert_eq!(
            Session::from_checkpoint(&session.checkpoint().unwrap(), session.head()).unwrap(),
            session
        );
    }
}

#[test]
fn terminal_checkpoint_reexecutes_original_history_and_blocks_every_later_control() {
    let mut session = Session::from_replay(populated());
    session
        .apply(Control::Replay(C::SetHeight(V::number(18.))))
        .unwrap();
    session
        .apply(Control::MainShutdown(vec![s("ignored")]))
        .unwrap();
    let head = session.head();
    let bytes = session.checkpoint().unwrap();
    assert_eq!(&bytes[..5], b"TLMS\x01");
    let mut recovered = Session::from_checkpoint(&bytes, head).unwrap();
    assert_eq!(recovered, session);
    for command in [
        Control::Replay(C::SetHeight(V::number(999.))),
        Control::Replay(invoke("Main.getCurrentBlockHeight")),
        Control::MainShutdown(vec![]),
    ] {
        assert_eq!(
            recovered.apply(command),
            Err(E::AlreadyExited(SourceMainProcessExitV1 { code: 0 }))
        );
        assert_eq!(recovered.head(), head);
        assert_eq!(recovered.checkpoint().unwrap(), bytes);
    }
    assert!(
        SourceContractReplay::from_checkpoint(&bytes, recovered.replay().head()).is_err(),
        "new envelope never masquerades as TLCR1--12"
    );
}

fn reseal(payload: &[u8]) -> Vec<u8> {
    let mut crc = !0u32;
    for byte in payload {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    let mut bytes = payload.to_vec();
    bytes.extend_from_slice(&(!crc).to_be_bytes());
    bytes
}
#[test]
fn independently_pinned_head_and_reexecution_reject_tampered_control_history() {
    let mut session = Session::from_replay(populated());
    session.apply(Control::MainShutdown(vec![])).unwrap();
    let bytes = session.checkpoint().unwrap();
    let head = session.head();
    let end = bytes.len() - 4;
    let mut wrong = head;
    wrong.commitment[0] ^= 1;
    assert_eq!(Session::from_checkpoint(&bytes, wrong), Err(E::WrongHead));
    let mut payload = bytes[..end].to_vec();
    payload[5] ^= 1;
    assert!(Session::from_checkpoint(&reseal(&payload), head).is_err());
    let mut payload = bytes[..end].to_vec();
    payload[4] = 2;
    assert_eq!(
        Session::from_checkpoint(&reseal(&payload), head),
        Err(E::InvalidCheckpoint)
    );
    let mut payload = bytes[..end].to_vec();
    payload[45..53].copy_from_slice(&u64::MAX.to_be_bytes());
    assert_eq!(
        Session::from_checkpoint(&reseal(&payload), head),
        Err(E::InvalidCheckpoint)
    );
    let mut payload = bytes[..end].to_vec();
    payload.push(0);
    assert_eq!(
        Session::from_checkpoint(&reseal(&payload), head),
        Err(E::InvalidCheckpoint)
    );
    let replay_len = u32::from_be_bytes(bytes[85..89].try_into().unwrap()) as usize;
    let terminal_offset = 89 + replay_len;
    let mut payload = bytes[..end].to_vec();
    payload[terminal_offset] = 2;
    assert_eq!(
        Session::from_checkpoint(&reseal(&payload), head),
        Err(E::InvalidCheckpoint)
    );
    let mut payload = bytes[..end].to_vec();
    payload[89 + replay_len / 2] ^= 1;
    assert!(
        Session::from_checkpoint(&reseal(&payload), head).is_err(),
        "embedded original journal must reexecute, not import a state snapshot"
    );
    // Reseal both checksums after changing a genuine command argument while
    // retaining the original receipts/head. Only actual replay detects this.
    let mut inner = bytes[89..89 + replay_len - 4].to_vec();
    let position = inner
        .windows(16)
        .position(|w| w == b"4031000000000000")
        .unwrap();
    inner[position + 3] = b'2'; // imported TrackHeight17 ->18, same frame length
    let inner = reseal(&inner);
    let mut payload = bytes[..end].to_vec();
    payload[89..89 + replay_len].copy_from_slice(&inner);
    assert_eq!(
        Session::from_checkpoint(&reseal(&payload), head),
        Err(E::Replay(SourceContractReplayError::ReplayMismatch))
    );
    let mut payload = bytes[..end].to_vec();
    *payload.last_mut().unwrap() ^= 1;
    assert_eq!(
        Session::from_checkpoint(&reseal(&payload), head),
        Err(E::WrongHead)
    );
}

#[test]
fn saturated_legacy_journal_does_not_prevent_unconditional_shutdown() {
    let mut replay = SourceContractReplay::with_policy(P::OnchainV1).unwrap();
    for i in 0..MAX_COMMANDS {
        replay.apply(C::SetHeight(V::number(i as f64))).unwrap();
    }
    let mut session = Session::from_replay(replay);
    let before = session.head();
    assert_eq!(
        session.apply(Control::Replay(C::SetHeight(V::number(0.)))),
        Err(E::Replay(SourceContractReplayError::Capacity))
    );
    assert_eq!(session.head(), before);
    assert_eq!(
        session
            .apply(Control::MainShutdown(vec![]))
            .unwrap()
            .outcome,
        O::Exited(SourceMainProcessExitV1 { code: 0 })
    );
    assert_eq!(session.head().controls, 1);
}

#[test]
fn failed_control_transport_is_atomic_before_source_prefix() {
    let mut session = Session::from_replay(populated());
    let bytes = session.checkpoint().unwrap();
    let head = session.head();
    let duplicate = V::Object(vec![("same".into(), V::Null), ("same".into(), V::Null)]);
    assert!(matches!(
        session.apply(Control::MainShutdown(vec![duplicate])),
        Err(E::Input(_))
    ));
    assert_eq!(session.terminal(), None);
    assert_eq!(session.head(), head);
    assert_eq!(session.checkpoint().unwrap(), bytes);
    session.apply(Control::MainShutdown(vec![])).unwrap();
}

#[test]
fn adoption_boundary_is_committed_without_duplicating_the_replay_journal() {
    let mut replay = SourceContractReplay::with_policy(P::OnchainV1).unwrap();
    let mut early = Session::from_replay(replay.clone());
    let command = C::SetHeight(V::number(1.));
    replay.apply(command.clone()).unwrap();
    early.apply(Control::Replay(command)).unwrap();
    let late = Session::from_replay(replay);
    assert_eq!(early.replay(), late.replay());
    assert_ne!(early.head(), late.head());
    assert_eq!(early.head().controls, 1);
    assert_eq!(late.head().controls, 0);
    for session in [early, late] {
        assert_eq!(
            Session::from_checkpoint(&session.checkpoint().unwrap(), session.head()).unwrap(),
            session
        );
    }
}
