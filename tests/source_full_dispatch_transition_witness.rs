use tradelayer_core::source_contract_replay::{
    source_full_dispatch_commands_commitment as commands_hash,
    source_full_dispatch_context_commitment as context_hash,
    source_full_dispatch_receipts_commitment as receipts_hash,
    source_full_dispatch_state_commitment as state_hash, SourceContractCommand as C,
    SourceContractCommandOutcome as O, SourceContractReceipt, SourceContractReplay as R,
    SourceFullDispatchDescriptor as Descriptor, SourceFullDispatchError as E,
    SourceFullDispatchExpectation as Expected, SourceFullDispatchScope as Scope,
    SourceFullDispatchTransitionWitness as W,
};
use tradelayer_core::source_contract_runtime::{SourceContractRuntimePolicy as P, StorageFailure};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceRuleError, SourceValue as V,
};

fn s(x: &str) -> V {
    V::String(x.into())
}
fn n(x: f64) -> V {
    V::number(x)
}
fn text(x: V) -> String {
    if let V::String(x) = x {
        x
    } else {
        panic!("expected string: {x:?}")
    }
}
fn array(x: V) -> Vec<V> {
    if let V::Array(x) = x {
        x
    } else {
        panic!("expected array: {x:?}")
    }
}
fn obj(x: &[(&str, V)]) -> V {
    V::Object(x.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn invoke(target: &str, arguments: Vec<V>) -> C {
    C::Invoke(DependencyCall {
        target: target.into(),
        arguments,
    })
}
fn parse_command(x: V) -> C {
    match text(x.get("kind")).as_str() {
        "Import" => C::Import {
            collection: text(x.get("collection")),
            rows: array(x.get("rows")),
        },
        "SetEntry" => C::SetEntry(text(x.get("entry"))),
        "SetHeight" => C::SetHeight(x.get("height")),
        "Invoke" => invoke(&text(x.get("target")), array(x.get("arguments"))),
        other => panic!("unprojected command {other}"),
    }
}
fn dependency(value: DependencyReturn) -> V {
    let (transport, outcome) = match value {
        DependencyReturn::Immediate(v) => ("Immediate", obj(&[("returned", v)])),
        DependencyReturn::Pending(v) => ("Pending", obj(&[("returned", v)])),
        DependencyReturn::Throw(e) => (
            "Throw",
            obj(&[(
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )]),
        ),
        DependencyReturn::PendingThrow(e) => (
            "PendingThrow",
            obj(&[(
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )]),
        ),
    };
    obj(&[("transport", s(transport)), ("outcome", outcome)])
}
struct Pins {
    before: R,
    after: R,
    policy: P,
    scope: Scope,
    descriptor: Descriptor,
    commands: [u8; 32],
    receipts: [u8; 32],
}
impl Pins {
    fn new(before: &R, commands: &[C], scope: Scope) -> Self {
        let mut after = before.clone();
        let receipts: Vec<SourceContractReceipt> = commands
            .iter()
            .map(|c| after.apply(c.clone()).unwrap())
            .collect();
        Self {
            before: before.clone(),
            policy: before.runtime().policy(),
            scope,
            descriptor: Descriptor::reference(),
            commands: commands_hash(before.runtime().policy(), scope, commands).unwrap(),
            receipts: receipts_hash(before.runtime().policy(), scope, &receipts).unwrap(),
            after,
        }
    }
    fn expected(&self) -> Expected<'_> {
        Expected {
            policy: self.policy,
            scope: self.scope,
            descriptor: &self.descriptor,
            pre_head: self.before.head(),
            pre_state_commitment: state_hash(&self.before).unwrap(),
            pre_context_commitment: context_hash(&self.before, self.scope).unwrap(),
            commands_commitment: self.commands,
            receipts_commitment: self.receipts,
            post_head: self.after.head(),
            post_state_commitment: state_hash(&self.after).unwrap(),
            post_context_commitment: context_hash(&self.after, self.scope).unwrap(),
        }
    }
    fn roundtrip(&self, commands: &[C]) -> Vec<u8> {
        let w = W::produce(&self.before, commands, &self.expected()).unwrap();
        assert_eq!(w.verify(&self.expected()).unwrap().replay(), &self.after);
        let bytes = w.canonical_bytes().unwrap();
        assert_eq!(&bytes[..5], b"TLSW\x04");
        let checked = W::verify_bytes(&bytes, &self.expected()).unwrap();
        assert_eq!(checked.replay(), &self.after);
        assert_eq!(
            checked.receipts().cloned().collect::<Vec<_>>(),
            w.claimed_receipts().cloned().collect::<Vec<_>>()
        );
        assert_eq!(
            checked.require_authenticated_input_and_state(),
            Err(SourceRuleError::UnauthenticatedInputAndState)
        );
        bytes
    }
}
fn fee() -> C {
    invoke(
        "Tally.queueFeeAccrual",
        vec![n(7.), n(0.00000001), n(1.), n(10.)],
    )
}

#[test]
fn independent_node_command_domains_pin_policy_scope_order_and_number_bits() {
    let source = [C::SetHeight(n(-0.)), fee()];
    let main = [invoke("Main.saveMaxProcessedHeight", vec![n(10.)])];
    for (policy, scope, expected) in [
        (
            P::SpotLifecycleV9,
            Scope::SourceCommandSlice,
            "1958912ce26ed1be9bc2ca445e9c39234c984ec01fb9346ba66795635d60e660",
        ),
        (
            P::SpotLifecycleV9,
            Scope::MainCallableSlice,
            "2deebdc24618fb965bcb463af7492c31184f6d074be22d46b2db57917eed14a1",
        ),
        (
            P::ActivationLifecycleV10,
            Scope::SourceCommandSlice,
            "f72fac2a67f5372ff0ae70db1ac0a0167e932fe37f2dcd5b61a4d9f512e68362",
        ),
        (
            P::ActivationLifecycleV10,
            Scope::MainCallableSlice,
            "74925dbfc7107a1e666a9076553f355d7429c50563e24c586352fc5810718f6a",
        ),
        (
            P::AmmLifecycleV11,
            Scope::SourceCommandSlice,
            "d7f5ed9c8a1ad870284ac76ccae9d62c0706dfd6536232b04959d76c3ac7b0e0",
        ),
        (
            P::AmmLifecycleV11,
            Scope::MainCallableSlice,
            "584f7e89f48dbe91636a1bf5814822410c5947ad0782b0004350026d58a5ab41",
        ),
    ] {
        let commands = if scope == Scope::SourceCommandSlice {
            &source[..]
        } else {
            &main[..]
        };
        assert_eq!(
            commands_hash(policy, scope, commands)
                .unwrap()
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect::<String>(),
            expected
        );
    }
    let mut reversed = source.clone();
    reversed.reverse();
    assert_ne!(
        commands_hash(P::SpotLifecycleV9, Scope::SourceCommandSlice, &source).unwrap(),
        commands_hash(P::SpotLifecycleV9, Scope::SourceCommandSlice, &reversed).unwrap()
    );
    let positive = [C::SetHeight(n(0.)), fee()];
    assert_ne!(
        commands_hash(P::SpotLifecycleV9, Scope::SourceCommandSlice, &source).unwrap(),
        commands_hash(P::SpotLifecycleV9, Scope::SourceCommandSlice, &positive).unwrap()
    );
}

#[test]
fn all_new_policies_reexecute_native_mutations_and_bind_distinct_contexts() {
    let mut hashes = std::collections::BTreeSet::new();
    for policy in [
        P::SpotLifecycleV9,
        P::ActivationLifecycleV10,
        P::AmmLifecycleV11,
    ] {
        let before = R::with_policy(policy).unwrap();
        let commands = vec![fee(), fee()];
        let pins = Pins::new(&before, &commands, Scope::SourceCommandSlice);
        assert_ne!(
            state_hash(&before).unwrap(),
            state_hash(&pins.after).unwrap()
        );
        assert!(pins
            .after
            .receipts()
            .all(|r| !matches!(r.outcome, O::LocalError(_))));
        hashes.insert(pins.roundtrip(&commands));
    }
    assert_eq!(hashes.len(), 3);
}

#[test]
fn every_independent_pin_is_mandatory_and_failure_does_not_mutate_the_owner() {
    let before = R::with_policy(P::AmmLifecycleV11).unwrap();
    let commands = vec![fee()];
    let pins = Pins::new(&before, &commands, Scope::SourceCommandSlice);
    let w = W::produce(&before, &commands, &pins.expected()).unwrap();
    macro_rules! bad {
        ($field:ident,$change:expr,$error:expr) => {{
            let mut e = pins.expected();
            e.$field = $change;
            assert_eq!(w.verify(&e), Err($error));
        }};
    }
    bad!(policy, P::ActivationLifecycleV10, E::WrongPolicy);
    bad!(scope, Scope::MainCallableSlice, E::WrongScope);
    let mut descriptor = Descriptor::reference();
    descriptor.source_commit.push('0');
    bad!(descriptor, &descriptor, E::WrongDescriptor);
    let mut head = before.head();
    head.commitment[0] ^= 1;
    bad!(pre_head, head, E::WrongPreHead);
    let mut head = pins.after.head();
    head.commitment[0] ^= 1;
    bad!(post_head, head, E::WrongPostHead);
    bad!(pre_state_commitment, [7; 32], E::WrongPreState);
    bad!(post_state_commitment, [7; 32], E::WrongPostState);
    bad!(pre_context_commitment, [7; 32], E::WrongPreContext);
    bad!(post_context_commitment, [7; 32], E::WrongPostContext);
    bad!(commands_commitment, [7; 32], E::WrongCommands);
    bad!(receipts_commitment, [7; 32], E::WrongReceipts);
    assert_eq!(before, pins.before);
}

fn repair(bytes: &mut [u8]) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let at = bytes.len() - 36;
    let mut child=Command::new("node").args(["-e","const c=require('node:crypto'),a=[];process.stdin.on('data',x=>a.push(x));process.stdin.on('end',()=>process.stdout.write(c.createHash('sha256').update('TradeLayer/SourceFullDispatchWitness/v4/').update(Buffer.concat(a)).digest()));"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(&bytes[..at]).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout.len(), 32);
    bytes[at..at + 32].copy_from_slice(&output.stdout);
    let mut crc = !0u32;
    for b in &bytes[..bytes.len() - 4] {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    let n = bytes.len();
    bytes[n - 4..].copy_from_slice(&(!crc).to_be_bytes());
}

#[test]
fn exact_reexecution_rejects_a_forged_receipt_even_with_matching_external_receipt_digest() {
    let before = R::with_policy(P::AmmLifecycleV11).unwrap();
    let commands = vec![fee()];
    let pins = Pins::new(&before, &commands, Scope::SourceCommandSlice);
    let witness = W::produce(&before, &commands, &pins.expected()).unwrap();
    let mut bytes = witness.canonical_bytes().unwrap();
    let original = witness.claimed_receipts().next().unwrap().clone();
    let mut forged = original.clone();
    // Native async return may be undefined. Replace it with a different value
    // of the same encoded length by changing transport Pending -> Throw is not
    // canonical, so instead change Pending -> Immediate with a resized frame.
    forged.outcome = O::Dependency(DependencyReturn::Immediate(V::Undefined));
    assert_ne!(forged.outcome, original.outcome);
    let old = original
        .outcome
        .value()
        .diagnostic_json()
        .unwrap()
        .into_bytes();
    let new = forged
        .outcome
        .value()
        .diagnostic_json()
        .unwrap()
        .into_bytes();
    let at = bytes.windows(old.len()).position(|x| x == old).unwrap();
    assert_eq!(
        u32::from_be_bytes(bytes[at - 4..at].try_into().unwrap()) as usize,
        old.len()
    );
    bytes.splice(at..at + old.len(), new.iter().copied());
    bytes[at - 4..at].copy_from_slice(&(new.len() as u32).to_be_bytes());
    let new_digest = receipts_hash(pins.policy, pins.scope, &[forged]).unwrap();
    let old_digest = pins.receipts;
    let digest_at = bytes.windows(32).position(|x| x == old_digest).unwrap();
    bytes[digest_at..digest_at + 32].copy_from_slice(&new_digest);
    repair(&mut bytes);
    let mut expected = pins.expected();
    expected.receipts_commitment = new_digest;
    assert_eq!(W::verify_bytes(&bytes, &expected), Err(E::WrongReceipts));
}

#[test]
fn native_source_throw_retains_financial_prefix_and_pending_main_jobs_are_bound() {
    for policy in [
        P::SpotLifecycleV9,
        P::ActivationLifecycleV10,
        P::AmmLifecycleV11,
    ] {
        let mut before = R::with_policy(policy).unwrap();
        let properties = r#"[[7,{"ticker":"WITNESS","totalInCirculation":10,"type":1,"whitelistId":0,"issuer":"alice"}]]"#;
        let tally = r#"[["alice",{"7":{"amount":10,"available":10,"reserved":0,"margin":0,"vesting":0,"channelBalance":0}}]]"#;
        for input in [
            C::Import {
                collection: "propertyList".into(),
                rows: vec![obj(&[
                    ("_id", s("propertyIndex")),
                    ("value", s(properties)),
                ])],
            },
            C::Import {
                collection: "tallyMap".into(),
                rows: vec![obj(&[("_id", s("tallyMap")), ("data", s(tally))])],
            },
            invoke("Tally.loadFromDB", vec![]),
            invoke("Property.getPropertyData", vec![n(7.)]),
        ] {
            before.apply(input).unwrap();
        }
        let command = invoke(
            "Tally.updateBalance",
            vec![
                s("alice"),
                n(7.),
                n(1.),
                n(-1.),
                n(0.),
                n(0.),
                s("literal-prefix"),
                n(10.),
            ],
        );
        let pins = Pins::new(
            &before,
            std::slice::from_ref(&command),
            Scope::SourceCommandSlice,
        );
        assert!(matches!(
            pins.after.receipts().last().unwrap().outcome,
            O::Dependency(DependencyReturn::PendingThrow(_))
        ));
        assert_ne!(
            before.runtime(),
            pins.after.runtime(),
            "native source prefix must remain after throw"
        );
        pins.roundtrip(std::slice::from_ref(&command));
        let retry = Pins::new(
            &pins.after,
            std::slice::from_ref(&command),
            Scope::SourceCommandSlice,
        );
        retry.roundtrip(&[command]);
        let before = R::with_policy(policy).unwrap();
        let command = invoke("Main.saveMaxProcessedHeight", vec![n(123.)]);
        let pins = Pins::new(
            &before,
            std::slice::from_ref(&command),
            Scope::MainCallableSlice,
        );
        assert_ne!(
            context_hash(&before, pins.scope).unwrap(),
            context_hash(&pins.after, pins.scope).unwrap(),
            "pending source height write is context"
        );
        let bytes = pins.roundtrip(&[command]);
        let resumed = W::verify_bytes(&bytes, &pins.expected())
            .unwrap()
            .into_replay();
        let next = invoke("Main.loadMaxProcessedHeight", vec![]);
        let retry = Pins::new(
            &resumed,
            std::slice::from_ref(&next),
            Scope::MainCallableSlice,
        );
        retry.roundtrip(&[next]);
    }
}

#[test]
fn older_witness_formats_pins_and_default_policy_stay_unchanged() {
    use tradelayer_core::source_contract_replay::{
        source_lifecycle_transition_commands_commitment as v2_digest,
        source_recent_lifecycle_entry_profile as old_entry,
        source_recent_lifecycle_state_commitment as old_state,
        source_recent_lifecycle_transition_commands_commitment as old_digest,
        SourceLifecycleTransitionError as V2Error, SourceLifecycleTransitionScope as V2Scope,
        SourceRecentLifecycleTransitionDescriptor as OldDescriptor,
        SourceRecentLifecycleTransitionError as OldError,
        SourceRecentLifecycleTransitionExpectation as OldExpected,
        SourceRecentLifecycleTransitionScope as OldScope,
        SourceRecentLifecycleTransitionWitness as OldWitness,
    };
    let height_commands = [C::SetHeight(n(-0.)), C::SetHeight(n(10.))];
    for (policy, scope, pin) in [
        (
            P::LowerLifecycleV7,
            OldScope::RawTransactionSlice,
            "810dedb415d2509280ddac1b006227046484f8b310ab0b9e02b9b286af8a3ba1",
        ),
        (
            P::MainCallableV8,
            OldScope::SourceMethodSlice,
            "a25c1505559cb48b695d6f3e69784fc2a5559d8d0f64b827ae4199d467772000",
        ),
        (
            P::MainCallableV8,
            OldScope::MainCallableSlice,
            "da9239d32ffd22fdda6d6ff297278785d9cbbc042ef0a2ff64a25deb3196893b",
        ),
    ] {
        assert_eq!(
            old_digest(policy, scope, &height_commands)
                .unwrap()
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect::<String>(),
            pin
        );
    }
    for policy in [
        P::SpotLifecycleV9,
        P::ActivationLifecycleV10,
        P::AmmLifecycleV11,
    ] {
        assert_eq!(
            old_digest(policy, OldScope::RawTransactionSlice, &height_commands),
            Err(OldError::WrongPolicy)
        );
        assert_eq!(
            v2_digest(policy, V2Scope::RawTransactionSlice, &height_commands),
            Err(V2Error::WrongPolicy)
        );
    }
    let default = R::new().unwrap();
    assert_eq!(default.runtime().policy(), P::GlobalClearingV2);
    assert_eq!(&default.checkpoint().unwrap()[..5], b"TLCR\x02");
    assert_eq!(state_hash(&default), Err(E::WrongPolicy));
    let before = R::with_policy(P::MainCallableV8).unwrap();
    let mut after = before.clone();
    for c in &height_commands {
        after.apply(c.clone()).unwrap();
    }
    let descriptor = OldDescriptor::reference();
    let pre_entry = old_entry(&before).unwrap();
    let post_entry = old_entry(&after).unwrap();
    let expected = OldExpected {
        policy: P::MainCallableV8,
        scope: OldScope::SourceMethodSlice,
        descriptor: &descriptor,
        pre_head: before.head(),
        pre_state_commitment: old_state(&before).unwrap(),
        pre_entry: &pre_entry,
        commands_commitment: old_digest(
            P::MainCallableV8,
            OldScope::SourceMethodSlice,
            &height_commands,
        )
        .unwrap(),
        post_head: after.head(),
        post_state_commitment: old_state(&after).unwrap(),
        post_entry: &post_entry,
    };
    let bytes = OldWitness::produce(&before, &height_commands, &expected)
        .unwrap()
        .canonical_bytes()
        .unwrap();
    assert_eq!(&bytes[..5], b"TLSW\x03");
    assert_eq!(
        OldWitness::verify_bytes(&bytes, &expected)
            .unwrap()
            .replay(),
        &after
    );
    let current = R::with_policy(P::AmmLifecycleV11).unwrap();
    let pins = Pins::new(&current, &[fee()], Scope::SourceCommandSlice);
    assert_eq!(
        W::verify_bytes(&bytes, &pins.expected()),
        Err(E::InvalidEnvelope)
    );
}
#[test]
fn repaired_envelopes_still_reject_forged_commands_receipts_context_and_checkpoint() {
    let before = R::with_policy(P::AmmLifecycleV11).unwrap();
    let commands = vec![fee(), fee()];
    let pins = Pins::new(&before, &commands, Scope::SourceCommandSlice);
    let bytes = pins.roundtrip(&commands);
    for size in [0, 4, 5, bytes.len() - 1] {
        assert!(W::verify_bytes(&bytes[..size], &pins.expected()).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(W::verify_bytes(&trailing, &pins.expected()).is_err());
    // Change every region's bytes, including the embedded TLCR, and repair both
    // public envelope checksums. Actual reexecution/independent pins still gate.
    let mut attempts = 0;
    for needle in [
        b"sourceCommit".as_slice(),
        b"moduleEntry",
        b"TLCR\x0b",
        b"Tally.queueFeeAccrual",
        b"maker",
        b"Pending",
        b"head",
    ] {
        for (at, _) in bytes
            .windows(needle.len())
            .enumerate()
            .filter(|(_, w)| *w == needle)
            .take(3)
        {
            let mut forged = bytes.clone();
            forged[at] ^= 1;
            repair(&mut forged);
            assert!(
                W::verify_bytes(&forged, &pins.expected()).is_err(),
                "accepted repaired forgery at {at}"
            );
            attempts += 1;
        }
    }
    assert!(
        attempts >= 9,
        "must reach multiple nested command/receipt/checkpoint fields"
    );
    let mut opposite = bytes.clone();
    opposite[4] = 3;
    repair(&mut opposite);
    assert!(W::verify_bytes(&opposite, &pins.expected()).is_err());
}

#[test]
fn unowned_history_and_supplied_configuration_cannot_be_promoted_to_source_execution() {
    let before = R::with_policy(P::AmmLifecycleV11).unwrap();
    for command in [
        C::SetActivation(true),
        C::SetEntry("main-subgraph".into()),
        C::Import {
            collection: "tally".into(),
            rows: vec![],
        },
        invoke("SourceGlobal.bindTally", vec![]),
        invoke("Main.processTxSet", vec![V::Array(vec![]), n(1.)]),
    ] {
        assert_eq!(
            commands_hash(P::AmmLifecycleV11, Scope::SourceCommandSlice, &[command]),
            Err(E::UnsupportedCommand)
        );
    }
    assert_eq!(
        commands_hash(
            P::AmmLifecycleV11,
            Scope::SourceCommandSlice,
            &[C::SetHeight(n(1.))]
        ),
        Err(E::UnsupportedCommand)
    );
    assert_eq!(
        commands_hash(P::MainCallableV8, Scope::SourceCommandSlice, &[fee()]),
        Err(E::WrongPolicy)
    );
    let bad = invoke("Tally.noSuchMethod", vec![]);
    let pins = Pins::new(
        &before,
        std::slice::from_ref(&bad),
        Scope::SourceCommandSlice,
    );
    assert_eq!(
        W::produce(&before, &[bad], &pins.expected()),
        Err(E::UnavailableExecution)
    );
    let mut history = before.clone();
    history.apply(invoke("Unknown.unowned", vec![])).unwrap();
    let pins = Pins::new(&history, &[fee()], Scope::SourceCommandSlice);
    assert_eq!(
        W::produce(&history, &[fee()], &pins.expected()),
        Err(E::UnavailableHistory)
    );
}

#[test]
fn empty_oversized_and_noncanonical_dispatch_is_bounded_before_execution() {
    assert_eq!(
        commands_hash(P::AmmLifecycleV11, Scope::SourceCommandSlice, &[]),
        Err(E::Capacity)
    );
    let oversized = vec![fee(); 4097];
    assert_eq!(
        commands_hash(P::AmmLifecycleV11, Scope::SourceCommandSlice, &oversized),
        Err(E::Capacity)
    );
    for kind in [-1., 36., f64::NAN, f64::INFINITY] {
        let raw = C::ExecuteContract(vec![
            s("tx"),
            n(kind),
            s("tl"),
            s("payload"),
            s("sender"),
            V::Null,
            n(0.),
            n(0.),
            n(10.),
        ]);
        assert_eq!(
            commands_hash(P::AmmLifecycleV11, Scope::SourceCommandSlice, &[raw]),
            Err(E::UnsupportedCommand)
        );
    }
    let before = R::with_policy(P::AmmLifecycleV11).unwrap();
    let pins = Pins::new(&before, &[fee()], Scope::SourceCommandSlice);
    let mut e = pins.expected();
    e.post_head.commands = 4097;
    assert_eq!(W::produce(&before, &[fee()], &e), Err(E::Capacity));
    assert_eq!(
        W::verify_bytes(&vec![0; 32 * 1024 * 1024 + 1], &pins.expected()),
        Err(E::Capacity)
    );
}

#[test]
fn actual_main_all_36_routed_types_have_exact_source_outcomes_and_conditional_witnesses() {
    let rows = include_str!("../fixtures/source_full_dispatch_main_inputs.jsonl");
    let mut replay = R::with_policy(P::AmmLifecycleV11).unwrap();
    let (mut calls, mut witnesses, mut refused, mut source_throws) = (0, 0, 0, 0);
    let mut kinds = std::collections::BTreeSet::new();
    for line in rows.lines() {
        let row = tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap();
        let name = text(row.get("name"));
        if row.get("reset") == V::Bool(true) {
            replay = R::with_policy(P::AmmLifecycleV11).unwrap();
            for input in array(row.get("inputs")) {
                let receipt = replay.apply(parse_command(input)).unwrap();
                assert!(
                    !matches!(receipt.outcome, O::LocalError(_)),
                    "{name}: invalid declared input {receipt:?}"
                );
            }
            continue;
        }
        if row.get("unavailable") == V::Bool(true) {
            refused += 1;
            continue;
        }
        if row.get("input") != V::Undefined {
            replay.apply(parse_command(row.get("input"))).unwrap();
            continue;
        }
        let failure = row.get("failure");
        if failure != V::Undefined {
            let op = text(failure.get("op"));
            let skip = match failure.get("skip") {
                V::Number(bits) => f64::from_bits(bits) as usize,
                V::Undefined => 0,
                other => panic!("{other:?}"),
            };
            replay
                .apply(C::FailStorage(StorageFailure {
                    collection: text(failure.get("collection")),
                    operation: if op == "getDatabase" {
                        op
                    } else {
                        format!("{op}Async")
                    },
                    skip,
                    message: text(failure.get("message")),
                }))
                .unwrap();
        }
        let command = invoke(&text(row.get("target")), array(row.get("args")));
        let before = replay.clone();
        let receipt = replay.apply(command.clone()).unwrap();
        let O::Dependency(value) = receipt.outcome else {
            panic!("{name}: unowned execution {:?}", receipt.outcome)
        };
        if matches!(
            value,
            DependencyReturn::Throw(_) | DependencyReturn::PendingThrow(_)
        ) {
            source_throws += 1;
        }
        assert_eq!(
            dependency(value),
            row.get("expected"),
            "{name}: genuine pinned Main result"
        );
        calls += 1;
        let selected = array(row.get("select"));
        if row.get("witness") != V::Bool(true) {
            continue;
        }
        assert!(text(row.get("target")).starts_with("Main."));
        for kind in selected {
            assert!(kinds.insert(text(kind).parse::<u32>().unwrap()));
        }
        let pins = Pins::new(
            &before,
            std::slice::from_ref(&command),
            Scope::MainCallableSlice,
        );
        assert_eq!(
            pins.after, replay,
            "{name}: same complete Core including aliases, jobs, failures and effects"
        );
        pins.roundtrip(&[command]);
        witnesses += 1;
    }
    assert_eq!(kinds, (0..36).collect());
    assert!(calls > 250);
    assert!(witnesses >= 25);
    assert!(refused >= 5);
    assert!(source_throws >= 10);
    eprintln!("Main TLSW4: {calls} actual Main/support calls, {witnesses} selected command witnesses, 36 kinds, {refused} isolated capability exclusions, {source_throws} source throws");
}
