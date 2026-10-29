use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
use tradelayer_core::source_rich_value::{
    self as rich, Atom, Graph, Intrinsic, Key, Kind, Limits, Object, Property, PropertyValue, Realm,
};
use tradelayer_core::validity_source::{DependencyCall, SourceValue as V};
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn bytes(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn caps() -> rich::Capabilities {
    rich::intrinsics::capabilities()
}
fn data(name: &str, value: Atom) -> Property {
    Property {
        key: Key::Utf16(u(name)),
        enumerable: true,
        configurable: true,
        value: PropertyValue::Data {
            value,
            writable: true,
        },
    }
}
fn setup(mut facts: Graph) -> rich::protocol::InvokeRichV1 {
    let root = facts.roots[0].clone();
    let id = facts.objects.len() as u32;
    facts.objects.push(Object {
        kind: Kind::Ordinary,
        prototype: Atom::Intrinsic(Intrinsic {
            realm: Realm::HostIo,
            name: u("Object.prototype"),
        }),
        properties: vec![
            data("chain", Atom::Utf16(u("LTC"))),
            data("test", Atom::Boolean(false)),
        ],
    });
    facts.roots = vec![Atom::Reference(id), root, Atom::Binary64(10f64.to_bits())];
    rich::protocol::InvokeRichV1 {
        target: "SourceMain.configureRichIO".into(),
        arguments: facts,
    }
}
fn call(target: &str) -> rich::protocol::InvokeRichV1 {
    rich::protocol::InvokeRichV1 {
        target: target.into(),
        arguments: Graph {
            roots: vec![],
            objects: vec![],
            symbols: vec![],
        },
    }
}
fn sans_metadata(value: V) -> V {
    let V::Object(fields) = value else { panic!() };
    V::Object(
        fields
            .into_iter()
            .filter(|(k, _)| !["sourceRich", "mainProduction"].contains(&k.as_str()))
            .collect(),
    )
}
#[test]
fn actual_core14_rpc_carrier_uses_same_owners_and_complete_original_source_graphs() {
    let mut count = 0;
    for line in include_str!("../fixtures/rich_main_v14.tsv").lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let input = rich::wire::decode_external(&bytes(f[4]), &caps(), Limits::default()).unwrap();
        let mut runtime = SourceContractRuntime::with_policy(P::RichThrownV14);
        let configured = runtime
            .invoke_dependency_rich(&setup(input.graph().clone()))
            .unwrap();
        assert_eq!(configured.transport, rich::protocol::Transport::Immediate);
        runtime.take_effects();
        let before = runtime.state_value().unwrap();
        let result = runtime.invoke_dependency_rich(&call(f[1])).unwrap();
        let after = runtime.state_value().unwrap();
        assert_eq!(
            sans_metadata(before.clone()),
            sans_metadata(after.clone()),
            "{} all actual non-metadata Core owners",
            f[0]
        );
        assert_eq!(
            before.get("mainProduction").get("index"),
            after.get("mainProduction").get("index"),
            "{} same actual TxIndex owner",
            f[0]
        );
        assert_eq!(
            rich::wire::encode(&result.graph).unwrap(),
            bytes(f[5]),
            "{} complete source result and causal identities",
            f[0]
        );
        assert_eq!(
            result.transport,
            if f[2] == "Pending" {
                rich::protocol::Transport::Pending
            } else {
                rich::protocol::Transport::PendingThrow
            },
            "{} source transport",
            f[0]
        );
        let effects = runtime.take_effects();
        assert_eq!(
            effects.len(),
            1 + f[3].parse::<usize>().unwrap(),
            "{} all RPC/log effects retained",
            f[0]
        );
        assert_eq!(effects[0].get("kind"), V::String("rpc-input".into()));
        assert!(effects[1..]
            .iter()
            .all(|x| x.get("kind") == V::String("source-log".into())));
        assert_eq!(
            runtime.rich_effect_snapshots().unwrap()[0],
            bytes(f[9]),
            "{} exact original RPC prefix snapshot",
            f[0]
        );
        assert!(!runtime.rich_state_material().unwrap().is_empty());
        count += 1;
    }
    assert_eq!(count, 46);
}
#[test]
fn actual_rich_journal14_reexecutes_and_refuses_legacy_profiles_and_native13_magic() {
    let first = include_str!("../fixtures/rich_main_v14.tsv")
        .lines()
        .next()
        .unwrap()
        .split('\t')
        .collect::<Vec<_>>();
    let facts = rich::wire::decode_external(&bytes(first[4]), &caps(), Limits::default()).unwrap();
    let mut replay = SourceContractReplay::with_policy(P::RichThrownV14).unwrap();
    replay
        .apply(C::InvokeRichV1(setup(facts.graph().clone())))
        .unwrap();
    for _ in 0..3 {
        let receipt = replay
            .apply(C::InvokeRichV1(call("Main.getCurrentBlockHeight")))
            .unwrap();
        assert!(matches!(receipt.outcome, O::RichDependencyV1(_)));
        let checkpoint = replay.checkpoint().unwrap();
        assert_eq!(&checkpoint[..5], b"TLCR\x0e");
        let rebuilt = SourceContractReplay::from_checkpoint(&checkpoint, replay.head()).unwrap();
        assert_eq!(
            rebuilt.runtime().state_value().unwrap(),
            replay.runtime().state_value().unwrap()
        );
        assert_eq!(
            rebuilt.runtime().rich_state_material().unwrap(),
            replay.runtime().rich_state_material().unwrap()
        );
    }
    let before = replay.checkpoint().unwrap();
    assert!(replay
        .apply(C::Invoke(DependencyCall {
            target: "Main.getCurrentBlockHeight".into(),
            arguments: vec![]
        }))
        .is_err());
    assert_eq!(replay.checkpoint().unwrap(), before);
    for policy in [
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
    ] {
        let mut old = SourceContractReplay::with_policy(policy).unwrap();
        let before = old.checkpoint().unwrap();
        assert!(old
            .apply(C::InvokeRichV1(call("Main.getCurrentBlockHeight")))
            .is_err());
        assert_eq!(old.checkpoint().unwrap(), before);
        assert!(old.runtime().rich_state_material().is_err());
    }
    let mut native_magic = replay.checkpoint().unwrap();
    native_magic[4] = 13;
    let payload = native_magic.len() - 4;
    let mut crc = !0u32;
    for byte in &native_magic[..payload] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32.wrapping_mul(crc & 1));
        }
    }
    native_magic[payload..].copy_from_slice(&(!crc).to_be_bytes());
    assert!(SourceContractReplay::from_checkpoint(&native_magic, replay.head()).is_err());
    assert_eq!(SourceContractRuntime::new().policy(), P::GlobalClearingV2);
}
