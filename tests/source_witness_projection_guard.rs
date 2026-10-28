use tradelayer_core::source_contract_replay::{
    source_full_dispatch_commands_commitment as command_hash,
    source_full_dispatch_context_commitment as context_hash,
    source_full_dispatch_projection_consistency as consistent,
    source_full_dispatch_receipts_commitment as receipt_hash,
    source_full_dispatch_state_commitment as state_hash, SourceContractCommand as C,
    SourceContractReceipt, SourceContractReplay as R, SourceFullDispatchDescriptor as Descriptor,
    SourceFullDispatchError as E, SourceFullDispatchExpectation as Expected,
    SourceFullDispatchScope as Scope, SourceFullDispatchTransitionWitness as W,
};
use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy as P;
use tradelayer_core::validity_source::{DependencyCall, SourceValue as V};
fn array(v: V) -> Vec<V> {
    if let V::Array(v) = v {
        v
    } else {
        panic!("array")
    }
}
fn text(v: V) -> String {
    if let V::String(v) = v {
        v
    } else {
        panic!("string")
    }
}
fn parse(v: V) -> C {
    match text(v.get("kind")).as_str() {
        "Import" => C::Import {
            collection: text(v.get("collection")),
            rows: array(v.get("rows")),
        },
        "SetHeight" => C::SetHeight(v.get("height")),
        "Invoke" => C::Invoke(DependencyCall {
            target: text(v.get("target")),
            arguments: array(v.get("arguments")),
        }),
        _ => panic!("input"),
    }
}
fn pins<'a>(
    before: &R,
    after: &R,
    command: &C,
    receipt: &SourceContractReceipt,
    descriptor: &'a Descriptor,
) -> Expected<'a> {
    let scope = Scope::MainCallableSlice;
    Expected {
        policy: P::AmmLifecycleV11,
        scope,
        descriptor,
        pre_head: before.head(),
        pre_state_commitment: state_hash(before).unwrap(),
        pre_context_commitment: context_hash(before, scope).unwrap(),
        commands_commitment: command_hash(P::AmmLifecycleV11, scope, std::slice::from_ref(command))
            .unwrap(),
        receipts_commitment: receipt_hash(P::AmmLifecycleV11, scope, std::slice::from_ref(receipt))
            .unwrap(),
        post_head: after.head(),
        post_state_commitment: state_hash(after).unwrap(),
        post_context_commitment: context_hash(after, scope).unwrap(),
    }
}

#[test]
fn actual_mixed_split_view_refuses_even_an_initial_witness_with_matching_pins_and_checksums() {
    let mut replay = R::with_policy(P::AmmLifecycleV11).unwrap();
    let descriptor = Descriptor::reference();
    let mut cases = 0;
    for line in include_str!("../fixtures/source_full_dispatch_mixed_prefix.jsonl").lines() {
        let row = tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap();
        if row.get("inputs") != V::Undefined {
            for input in array(row.get("inputs")) {
                replay.apply(parse(input)).unwrap();
            }
            continue;
        }
        let before = replay.clone();
        let command = parse(row.get("command"));
        let receipt = replay.apply(command.clone()).unwrap();
        let name = text(row.get("name"));
        cases += 1;
        let authoritative = V::Array(replay.runtime().collection("orderBooks").to_vec());
        assert_eq!(
            authoritative,
            row.get("expectedOrderBooks"),
            "{name}: exact original collection, no normalization"
        );
        let expected = pins(&before, &replay, &command, &receipt, &descriptor);
        if name.contains("04-spot-pair")
            && consistent(&replay) == Err(E::InconsistentStateProjection("orderBooks".into()))
        {
            assert_eq!(
                consistent(&replay),
                Err(E::InconsistentStateProjection("orderBooks".into()))
            );
            assert_eq!(
                W::produce(&before, &[command], &expected),
                Err(E::InconsistentStateProjection("orderBooks".into()))
            );
            // This envelope was produced AND verified by the immutable initial
            // candidate on the same60ec Core. All pins and public checksums are
            // correct. Strict endpoint consistency independently rejects it.
            let initial = include_bytes!("../fixtures/mixed04_initial_tlsw4.bin");
            assert_eq!(
                W::verify_bytes(initial, &expected),
                Err(E::InconsistentStateProjection("orderBooks".into()))
            );
        } else {
            consistent(&replay).unwrap();
            let w = W::produce(&before, &[command], &expected).unwrap();
            assert_eq!(w.verify(&expected).unwrap().replay(), &replay);
            if name.contains("04-spot-pair") {
                // A corrected engine must expose the exact two source rows and
                // support a NEW positive witness. The stale old-provider
                // envelope cannot become its current independently bound proof.
                let initial = include_bytes!("../fixtures/mixed04_initial_tlsw4.bin");
                assert!(W::verify_bytes(initial, &expected).is_err());
            }
        }
    }
    assert_eq!(cases, 4);
}

#[test]
fn fresh_real_policies_have_no_enumerated_projection_inconsistency() {
    for policy in [
        P::SpotLifecycleV9,
        P::ActivationLifecycleV10,
        P::AmmLifecycleV11,
    ] {
        consistent(&R::with_policy(policy).unwrap()).unwrap();
    }
}

#[test]
fn a_new_witness_does_not_enable_activation_or_amm_on_an_older_policy() {
    use tradelayer_core::source_contract_replay::SourceContractCommandOutcome as O;
    use tradelayer_core::validity_source::SourceRuleError;
    for (policy, kind) in [
        (P::SpotLifecycleV9, 0.),
        (P::SpotLifecycleV9, 10.),
        (P::ActivationLifecycleV10, 10.),
    ] {
        let before = R::with_policy(policy).unwrap();
        let command = C::ExecuteContract(vec![
            V::String("tx".into()),
            V::number(kind),
            V::String("tl".into()),
            V::String("0;18".into()),
            V::String("actor".into()),
            V::Null,
            V::number(0.),
            V::number(0.),
            V::number(10.),
        ]);
        let mut after = before.clone();
        let receipt = after.apply(command.clone()).unwrap();
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::InvalidInput)
        );
        assert_eq!(before.runtime(), after.runtime());
        let descriptor = Descriptor::reference();
        let scope = Scope::SourceCommandSlice;
        let expected = Expected {
            policy,
            scope,
            descriptor: &descriptor,
            pre_head: before.head(),
            pre_state_commitment: state_hash(&before).unwrap(),
            pre_context_commitment: context_hash(&before, scope).unwrap(),
            commands_commitment: command_hash(policy, scope, std::slice::from_ref(&command))
                .unwrap(),
            receipts_commitment: receipt_hash(policy, scope, &[receipt]).unwrap(),
            post_head: after.head(),
            post_state_commitment: state_hash(&after).unwrap(),
            post_context_commitment: context_hash(&after, scope).unwrap(),
        };
        assert_eq!(
            W::produce(&before, &[command], &expected),
            Err(E::UnavailableExecution)
        );
    }
}
