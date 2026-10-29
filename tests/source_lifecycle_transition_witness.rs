use std::collections::BTreeSet;
use tradelayer_core::source_contract_replay::{
    source_lifecycle_transition_commands_commitment as digest, SourceContractCommand as C,
    SourceContractReplay as R, SourceLifecycleTransitionError as E,
    SourceLifecycleTransitionExpectation as Expected, SourceLifecycleTransitionScope as Scope,
    SourceLifecycleTransitionWitness as W, SourceTransitionDescriptor,
    SourceTransitionExpectation as ExpectedV1, SourceTransitionWitness as V1,
};
use tradelayer_core::source_contract_replay::{
    SourceContractCommandOutcome as O, SourceContractReceipt,
};
use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy as P;
use tradelayer_core::source_contract_runtime::StorageFailure;
use tradelayer_core::validity_source::{DependencyCall, SourceCall, SourceValue as V};
use tradelayer_core::validity_source::{DependencyReturn, SourceOutcome};
fn n(v: f64) -> V {
    V::number(v)
}
fn s(v: &str) -> V {
    V::String(v.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn text(v: V) -> String {
    if let V::String(t) = v {
        t
    } else {
        panic!("string expected {v:?}")
    }
}
fn array(v: V) -> Vec<V> {
    if let V::Array(a) = v {
        a
    } else {
        panic!("array expected {v:?}")
    }
}
fn invoke(target: &str, args: Vec<V>) -> C {
    C::Invoke(DependencyCall {
        target: target.into(),
        arguments: args,
    })
}
fn raw(kind: f64) -> C {
    C::ExecuteContract(vec![
        s("original-tx"),
        n(kind),
        s("tl"),
        s("1,2"),
        s("actor"),
        V::Null,
        n(0.),
        n(0.),
        n(10.),
    ])
}
fn witness(before: &R, scope: Scope, commands: &[C]) -> W {
    let descriptor = SourceTransitionDescriptor::reference();
    let expected = Expected {
        policy: before.runtime().policy(),
        scope,
        descriptor: &descriptor,
        pre_head: before.head(),
        commands_commitment: digest(before.runtime().policy(), scope, commands).unwrap(),
        post_head: None,
    };
    let saved = before.clone();
    let w = W::produce(before, commands, &expected).unwrap();
    assert_eq!(*before, saved);
    let bytes = w.canonical_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLSW\x02");
    let mut direct = before.clone();
    for c in commands {
        direct.apply(c.clone()).unwrap();
    }
    let checked = W::verify_bytes(&bytes, &expected).unwrap();
    assert_eq!(checked.scope(), scope);
    assert_eq!(checked.replay(), &direct);
    assert_eq!(checked.receipts().count(), commands.len());
    assert_eq!(checked.post_head(), w.post_head());
    assert_eq!(w.verify(&expected).unwrap(), checked);
    w
}
#[test]
fn versioned_scope_and_policy_matrix_preserve_tlsw1_admissions() {
    let v3 = [16., 17., 18., 19., 20., 21., 22., 23., 31.];
    let v4 = [24., 25., 27.];
    let v5 = [26., 28., 29.];
    for p in [
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
    ] {
        let before = R::with_policy(p).unwrap();
        for kind in v3 {
            witness(&before, Scope::RawTransactionSlice, &[raw(kind)]);
        }
        for kind in v4 {
            let result = digest(p, Scope::RawTransactionSlice, &[raw(kind)]);
            if p == P::SourceLifecycleV3 {
                assert_eq!(result, Err(E::UnsupportedCommand));
            } else {
                witness(&before, Scope::RawTransactionSlice, &[raw(kind)]);
            }
        }
        for kind in v5 {
            let result = digest(p, Scope::RawTransactionSlice, &[raw(kind)]);
            if p != P::DistributionLifecycleV5 {
                assert_eq!(result, Err(E::UnsupportedCommand));
            } else {
                witness(&before, Scope::RawTransactionSlice, &[raw(kind)]);
            }
        }
        assert_eq!(
            digest(p, Scope::RawTransactionSlice, &[raw(30.)]),
            Err(E::UnsupportedCommand)
        );
        let descriptor = SourceTransitionDescriptor::reference();
        let old_expected = ExpectedV1 {
            policy: p,
            descriptor: &descriptor,
            pre_head: before.head(),
            commands_commitment: None,
            post_head: None,
        };
        assert_eq!(
            V1::produce(&before, &[raw(19.)], &old_expected).unwrap_err(),
            tradelayer_core::source_contract_replay::SourceTransitionError::UnsupportedCommand
        );
        let old = V1::produce(&before, &[raw(18.)], &old_expected).unwrap();
        assert_eq!(&old.canonical_bytes().unwrap()[..5], b"TLSW\x01");
    }
}
#[test]
fn lifecycle_scope_does_not_admit_supplied_financial_verdicts_or_bootstrap_commands() {
    let p = P::DistributionLifecycleV5;
    for forbidden in [
        invoke("Tally.updateBalance", vec![]),
        invoke("Margin.updateMargin", vec![]),
        invoke("Orderbook.processContractMatches", vec![]),
        invoke("Orderbook.processTokenMatches", vec![]),
        invoke("SourceGlobal.bindTally", vec![]),
        C::Import {
            collection: "tallyMap".into(),
            rows: vec![],
        },
        C::SetActivation(true),
        C::SetEntry("types-first".into()),
        C::EvaluateQueue(SourceCall {
            method: "queueChannelTrade".into(),
            arguments: vec![],
        }),
    ] {
        assert_eq!(
            digest(p, Scope::SourceMethodSlice, &[forbidden]),
            Err(E::UnsupportedCommand)
        );
    }
    let direct = invoke("Logic.tradeTokensChannel", vec![]);
    assert_eq!(
        digest(p, Scope::RawTransactionSlice, std::slice::from_ref(&direct)),
        Err(E::UnsupportedCommand)
    );
    witness(
        &R::with_policy(p).unwrap(),
        Scope::SourceMethodSlice,
        &[direct],
    );
    assert_eq!(
        digest(P::OnchainV1, Scope::RawTransactionSlice, &[]),
        Err(E::WrongPolicy)
    );
    assert_eq!(
        digest(P::GlobalClearingV2, Scope::SourceMethodSlice, &[]),
        Err(E::WrongPolicy)
    );
}

#[test]
fn v6_explicitly_inherits_slices_and_adds_owned_proof_and_reward_routes() {
    let policy = P::ProofLifecycleV6;
    let before = R::with_policy(policy).unwrap();
    for kind in [
        16., 17., 18., 19., 20., 21., 22., 23., 24., 25., 26., 27., 28., 29., 30., 31., 32., 33.,
        34., 35.,
    ] {
        witness(&before, Scope::RawTransactionSlice, &[raw(kind)]);
    }
    for target in [
        "Logic.processStakeFraudProof",
        "Logic.applyProceduralOracleSettlement",
        "Logic.queueLiquidityReward",
        "Logic.tradeTokenForUTXO",
        "Logic.batchMoveZkRollup",
    ] {
        let command = invoke(target, vec![]);
        assert_eq!(
            digest(
                policy,
                Scope::RawTransactionSlice,
                std::slice::from_ref(&command)
            ),
            Err(E::UnsupportedCommand)
        );
        witness(
            &before,
            Scope::SourceMethodSlice,
            std::slice::from_ref(&command),
        );
        assert_eq!(
            digest(
                P::DistributionLifecycleV5,
                Scope::SourceMethodSlice,
                &[command]
            ),
            Err(E::UnsupportedCommand)
        );
    }
    let settle = invoke("Logic.settleLiquidityRewards", vec![n(10.)]);
    witness(
        &before,
        Scope::RawTransactionSlice,
        std::slice::from_ref(&settle),
    );
    assert_eq!(
        digest(
            P::DistributionLifecycleV5,
            Scope::RawTransactionSlice,
            &[settle]
        ),
        Err(E::UnsupportedCommand)
    );
    for target in [
        "SourceEnvironment.configure",
        "SourceArtifacts.configure",
        "Procedural.transitionContract",
        "BitvmCache.open",
        "BitvmRisk.onSweep",
    ] {
        assert_eq!(
            digest(policy, Scope::SourceMethodSlice, &[invoke(target, vec![])]),
            Err(E::UnsupportedCommand)
        );
    }
    for args in [vec![], vec![n(10.), V::Null]] {
        assert_eq!(
            digest(
                policy,
                Scope::RawTransactionSlice,
                &[invoke("Logic.settleLiquidityRewards", args)]
            ),
            Err(E::UnsupportedCommand)
        );
    }
    for kind in [3., 36.] {
        assert_eq!(
            digest(policy, Scope::RawTransactionSlice, &[raw(kind)]),
            Err(E::UnsupportedCommand)
        );
    }
}

fn source_cases(file: &str, constant: &str, extra: &str) -> Vec<V> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let rust = std::fs::read_to_string(root.join("tests").join(file)).unwrap();
    let marker = format!("const {constant}: &str = r###\"");
    let mut program = rust
        .split_once(&marker)
        .unwrap()
        .1
        .split_once("\"###;")
        .unwrap()
        .0
        .to_owned();
    if !extra.is_empty() {
        let at = program.rfind("})().catch").unwrap();
        program.insert_str(at, extra);
    }
    let version = std::env::var("TL_SOURCE_BN_VERSION").unwrap_or_else(|_| "9.3.1".into());
    assert!(["9.3.1", "9.1.2"].contains(&version.as_str()));
    if version == "9.1.2" {
        program = format!(
            r#"(function(){{const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto'),root=process.argv[1],numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs')),packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],source=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));if(crypto.createHash('sha256').update(source).digest('hex')!=='20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e')throw Error('genuine BigNumber9.1.2');const m={{exports:{{}}}};vm.runInNewContext(source.toString('utf8'),{{module:m,exports:m.exports}},{{timeout:1000}});numeric.loadBigNumber=()=>m.exports;}})();
{program}"#
        );
    }
    let output = std::process::Command::new("node")
        .args(["-e", &program, env!("CARGO_MANIFEST_DIR"), &version])
        .env("PROOF_REPO", env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "genuine source {file}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect()
}
fn source_command(command: &V) -> C {
    if let V::Array(args) = command.get("raw") {
        return C::ExecuteContract(args);
    }
    let target = if let V::String(t) = command.get("target") {
        t
    } else {
        format!("Logic.{}", text(command.get("method")))
    };
    let args = array(command.get("args"));
    if let Some(method) = target.strip_prefix("Orderbook.").filter(|m| {
        matches!(
            *m,
            "processQueuedOnChainOrdersForBlock" | "processQueuedChannelTradesForBlock"
        )
    }) {
        C::EvaluateQueue(SourceCall {
            method: method.into(),
            arguments: args,
        })
    } else {
        invoke(&target, args)
    }
}
fn failure(v: &V) -> StorageFailure {
    let op = text(v.get("op"));
    StorageFailure {
        collection: text(v.get("collection")),
        operation: if op == "getDatabase" || op == "loadDatabase" {
            op
        } else {
            format!("{op}Async")
        },
        skip: if let V::Number(bits) = v.get("skip") {
            f64::from_bits(bits) as usize
        } else {
            0
        },
        message: text(v.get("message")),
    }
}
fn plain_outcome(outcome: &O) -> V {
    match outcome {
        O::Void => obj(&[("returned", V::Undefined)]),
        O::Source(SourceOutcome::Returned(v)) => obj(&[("returned", v.clone())]),
        O::Dependency(DependencyReturn::Immediate(v) | DependencyReturn::Pending(v)) => {
            obj(&[("returned", v.clone())])
        }
        O::Source(SourceOutcome::Thrown(e))
        | O::Dependency(DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e)) => {
            obj(&[(
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )])
        }
        O::LocalError(e) => panic!("actual source command unavailable {e:?}"),
        O::RichDependencyV1(_) => panic!("rich outcome outside this typed legacy witness corpus"),
    }
}
fn receipt_result(replay: &R, receipt: &SourceContractReceipt, case: &V) -> V {
    let nested = case.get("result");
    if matches!(case.get("command").get("raw"), V::Array(_)) {
        if nested != V::Undefined {
            let value = match &receipt.outcome {
                O::Source(SourceOutcome::Returned(v)) => v.clone(),
                O::Source(SourceOutcome::Thrown(e)) => obj(&[
                    (
                        "params",
                        replay
                            .runtime()
                            .last_contract_parameters()
                            .cloned()
                            .unwrap_or(V::Undefined),
                    ),
                    (
                        "thrown",
                        obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                    ),
                ]),
                o => panic!("raw outcome {o:?}"),
            };
            return obj(&[("outcome", value)]);
        }
        return plain_outcome(&receipt.outcome);
    }
    if nested != V::Undefined {
        let transport = match &receipt.outcome {
            O::Dependency(DependencyReturn::Immediate(_)) => "Immediate",
            O::Dependency(DependencyReturn::Pending(_)) => "Pending",
            O::Dependency(DependencyReturn::Throw(_)) => "Throw",
            O::Dependency(DependencyReturn::PendingThrow(_)) => "PendingThrow",
            o => panic!("dependency outcome {o:?}"),
        };
        obj(&[
            ("transport", s(transport)),
            ("outcome", plain_outcome(&receipt.outcome)),
        ])
    } else {
        plain_outcome(&receipt.outcome)
    }
}
fn assert_source(replay: &R, receipt: &SourceContractReceipt, case: &V) {
    let name = text(case.get("name"));
    let expected = if case.get("result") != V::Undefined {
        case.get("result")
    } else {
        case.get("outcome")
    };
    assert!(
        !matches!(receipt.outcome, O::LocalError(_)),
        "{name} actual command local error: {:?}\nactual={:?}\nsource={:?}",
        receipt.outcome,
        receipt.effects,
        case.get("effects")
    );
    assert_eq!(
        receipt_result(replay, receipt, case),
        expected,
        "{name} genuine original outcome"
    );
    for table in array(case.get("state").get("tables")) {
        let collection = text(table.get("name"));
        assert_eq!(
            V::Array(replay.runtime().collection(&collection).to_vec()),
            table.get("rows"),
            "{name} actual persisted {collection}"
        );
    }
    let actual = replay.runtime().state_value().unwrap();
    let expected = case.get("state");
    if expected.get("tally") != V::Undefined {
        assert_eq!(
            replay.runtime().tally_cache(),
            expected.get("tally"),
            "{name} actual sole Tally cache"
        );
    }
    if expected.get("marginState") != V::Undefined {
        assert_eq!(
            replay.runtime().margins().state_value().unwrap(),
            expected.get("marginState"),
            "{name} actual shared Margin identities"
        );
    }
    if expected.get("channels") != V::Undefined {
        assert_eq!(
            actual.get("globalClearing").get("channels").get("registry"),
            expected.get("channels"),
            "{name} live channel cache"
        );
    }
    for field in ["feeBuffer", "blockTrades", "oracleMarks", "nativeMarks"] {
        if expected.get(field) != V::Undefined {
            assert_eq!(actual.get(field), expected.get(field), "{name} {field}");
        }
    }
    for field in ["unhandled", "unhandledRejections"] {
        if expected.get(field) != V::Undefined {
            assert_eq!(
                actual.get("globalClearing").get("unhandledRejections"),
                expected.get(field),
                "{name} independent rejected continuations"
            );
        }
    }
    for field in ["pendingWithdrawals", "withdrawals"] {
        if expected.get(field) != V::Undefined {
            assert_eq!(
                actual
                    .get("globalClearing")
                    .get("channels")
                    .get("pendingWithdrawals"),
                expected.get(field),
                "{name} pending withdrawals"
            );
        }
    }
    if let V::Array(books) = expected.get("books") {
        assert_eq!(
            array(actual.get("queue").get("instances")).len(),
            books.len(),
            "{name} actual book instance count"
        );
        for (i, book) in books.iter().enumerate() {
            assert_eq!(
                replay
                    .runtime()
                    .queue()
                    .instance_value(i as u64 + 1)
                    .unwrap(),
                *book,
                "{name} actual queue instance{i}"
            );
        }
    }
    if let V::Array(instances) = expected.get("instances") {
        assert_eq!(
            array(
                replay
                    .runtime()
                    .margins()
                    .state_value()
                    .unwrap()
                    .get("instances")
            )
            .len(),
            instances.len(),
            "{name} actual Margin instance count"
        );
        for (i, instance) in instances.iter().enumerate() {
            let got = replay
                .runtime()
                .margins()
                .instance_value(i as u64 + 1)
                .unwrap();
            let V::Object(fields) = instance else {
                panic!("source Margin instance")
            };
            for (k, v) in fields {
                assert_eq!(got.get(k), *v, "{name} Margin instance{i}.{k}");
            }
        }
    }
    for field in ["vaults", "syntheticTokens"] {
        if expected.get(field) != V::Undefined {
            assert_eq!(
                actual.get("syntheticLifecycle").get(field),
                expected.get(field),
                "{name} actual synthetic cache {field}"
            );
        }
    }
    if expected.get("property") != V::Undefined {
        assert_eq!(
            actual.get("globalClearing").get("propertyCache"),
            expected.get("property"),
            "{name} actual Property cache"
        );
    }
    if expected.get("queue") != V::Undefined {
        for key in ["queue", "volumeCache"] {
            assert_eq!(
                actual.get("liquidityRewards").get(key),
                expected.get(key),
                "{name} actual private Logic {key}"
            );
        }
    }
    if expected.get("environment") != V::Undefined {
        assert_eq!(
            actual.get("proofLifecycle").get("environment"),
            expected.get("environment"),
            "{name} bound original environment"
        );
    }
    if expected.get("artifacts") != V::Undefined {
        assert_eq!(
            actual.get("proofLifecycle").get("artifacts"),
            expected.get("artifacts"),
            "{name} bound original artifact bytes"
        );
    }
    if expected.get("propertyInitialized") != V::Undefined {
        assert_eq!(
            actual.get("proofLifecycle").get("propertyInitialized"),
            expected.get("propertyInitialized"),
            "{name} actual property initialization"
        );
    }
    for (got, wanted) in [
        ("oracleSourceCache", "oracleCache"),
        ("oracleSourceInitialized", "oracleInitialized"),
        ("oracleSourceWatermarks", "oracleWatermarks"),
    ] {
        if expected.get(wanted) != V::Undefined {
            assert_eq!(actual.get(got), expected.get(wanted), "{name} actual {got}");
        }
    }
    for (got, wanted) in [
        ("propertyCache", "propertyCache"),
        ("tallyStaticInitialized", "tallyInitialized"),
        ("tallyMod", "tallyMod"),
        ("contractModRaw", "contractMod"),
    ] {
        if expected.get(wanted) != V::Undefined {
            let mut observed = actual.get("globalClearing").get(got);
            // The older channel-Logic observer records !!modFlag; the proof
            // observer records the raw value. Mirror only that declared view.
            if wanted == "contractMod" && expected.get("pendingWithdrawals") != V::Undefined {
                let value = match &observed {
                    V::Undefined | V::Null | V::Bool(false) => false,
                    V::Number(bits) => {
                        let f = f64::from_bits(*bits);
                        f != 0. && !f.is_nan()
                    }
                    V::String(s) => !s.is_empty(),
                    _ => true,
                };
                observed = V::Bool(value);
            }
            assert_eq!(observed, expected.get(wanted), "{name} actual {got}");
        }
    }
    let source_effects = array(case.get("effects"));
    let include_source = source_effects
        .iter()
        .any(|e| e.get("kind") == s("source-function"));
    let include_activation = source_effects
        .iter()
        .any(|e| e.get("kind") == s("activation"));
    let projected=receipt.effects.iter().filter_map(|e|{
        if matches!(e.get("kind"),V::String(t)if["database","db","uuid","random","rpc-clock","rpc","artifact-read"].contains(&t.as_str())){return Some(e.clone());}
        if e.get("kind")==s("dependency"){
            let target=e.get("target");
            if include_source&&matches!(&target,V::String(t)if t.starts_with("Logic.")||(expected.get("propertyInitialized")!=V::Undefined&&["Oracle.recordStake","Oracle.getStake","Oracle.applyFraudProof","Oracle.relayTradeLayerState","Dlc.parseRelayBlob","Dlc.validateRelayBundle","Property.getInstance","Property.load","Property.getPropertyData","Property.getPropertyIndex","Property.save","Property.grantTokens","Property.redeemTokens","Procedural.transitionContract","Procedural._base","BitvmCache._base","BitvmCache._challengeBlocks","BitvmCache.buildCacheId","BitvmCache.get","BitvmCache.open","BitvmCache.challenge","BitvmCache.finalize","BitvmCache.resolve","BitvmRisk.onCacheOpen","BitvmRisk.onEscrowRelease","BitvmRisk.onSweep"].contains(&t.as_str()))){return Some(obj(&[("kind",s("source-function")),("target",target),("args",e.get("args"))]));}
            if target==s("Activation.isTxTypeActive"){
                return Some(if include_activation{obj(&[("kind",s("activation")),("method",s("isTxTypeActive")),("args",e.get("args")),("returned",e.get("returned"))])}else if source_effects.iter().any(|v|v.get("target")==target){e.clone()}else{return None;});
            }
        }None
    }).collect::<Vec<_>>();
    assert_eq!(
        V::Array(projected),
        case.get("effects"),
        "{name} complete ordered DB/RPC/source effects"
    );
}
fn true_slices(
    file: &str,
    constant: &str,
    policy: P,
    selected: &[&str],
    extra: &str,
    seen: &mut BTreeSet<u8>,
) -> usize {
    let cases = source_cases(file, constant, extra);
    let mut replay = R::with_policy(policy).unwrap();
    let mut chosen = false;
    let mut slices = 0;
    for case in cases {
        let command = case.get("command");
        let name = text(case.get("name"));
        if matches!(command.get("reset"), V::Object(_)) {
            chosen = selected.contains(&name.as_str());
            if !chosen {
                continue;
            }
            replay = R::with_policy(policy).unwrap();
            let options = command.get("reset");
            if let V::Object(fields) = options.get("env") {
                replay
                    .apply(invoke(
                        "SourceEnvironment.configure",
                        vec![V::Object(fields)],
                    ))
                    .unwrap();
            }
            if let V::String(entry) = options.get("entry") {
                replay.apply(C::SetEntry(entry)).unwrap();
            }
            if options.get("inactive") == V::Bool(true) {
                replay.apply(C::SetActivation(false)).unwrap();
            }
            for table in array(case.get("state").get("tables")) {
                replay
                    .apply(C::Import {
                        collection: text(table.get("name")),
                        rows: array(table.get("rows")),
                    })
                    .unwrap();
            }
            for (flag, target) in [
                ("bindTally", "SourceGlobal.bindTally"),
                ("rpc", "SourceGlobal.bindRpcTransactions"),
            ] {
                if options.get(flag) == V::Bool(true) {
                    replay.apply(invoke(target, vec![])).unwrap();
                }
            }
            continue;
        }
        if !chosen || command.get("nodeOnly") == V::Bool(true) {
            continue;
        }
        if command.get("environment") != V::Undefined {
            replay
                .apply(invoke(
                    "SourceEnvironment.configure",
                    vec![command.get("environment")],
                ))
                .unwrap();
            continue;
        }
        if command.get("artifacts") != V::Undefined {
            replay
                .apply(invoke(
                    "SourceArtifacts.configure",
                    vec![command.get("artifacts")],
                ))
                .unwrap();
            continue;
        }
        if matches!(command.get("seed"), V::Object(_)) {
            let seed = command.get("seed");
            replay
                .apply(C::Import {
                    collection: text(seed.get("collection")),
                    rows: array(seed.get("rows")),
                })
                .unwrap();
            continue;
        }
        if matches!(command.get("failure"), V::Object(_)) {
            replay
                .apply(C::FailStorage(failure(&command.get("failure"))))
                .unwrap();
        }
        let original = source_command(&command);
        if command.get("unsupported") == V::Bool(true) {
            let receipt = replay.apply(original).unwrap();
            assert!(
                matches!(receipt.outcome, O::LocalError(_)),
                "{name} explicit local boundary"
            );
            continue;
        }
        let reward_continuation = matches!(&original,C::Invoke(c)if c.target=="Logic.settleLiquidityRewards"&&c.arguments.len()==1);
        let scope = if reward_continuation
            || matches!(original, C::ExecuteContract(_) | C::EvaluateQueue(_))
        {
            Scope::RawTransactionSlice
        } else {
            Scope::SourceMethodSlice
        };
        if digest(policy, scope, std::slice::from_ref(&original)).is_err() {
            replay.apply(original).unwrap();
            continue;
        }
        let descriptor = SourceTransitionDescriptor::reference();
        let expected = Expected {
            policy,
            scope,
            descriptor: &descriptor,
            pre_head: replay.head(),
            commands_commitment: digest(policy, scope, std::slice::from_ref(&original)).unwrap(),
            post_head: None,
        };
        let witness = W::produce(&replay, std::slice::from_ref(&original), &expected)
            .unwrap_or_else(|e| panic!("{name} produce {e:?}"));
        let checked = W::verify_bytes(&witness.canonical_bytes().unwrap(), &expected)
            .unwrap_or_else(|e| panic!("{name} verify {e:?}"));
        let mut direct = replay.clone();
        let receipt = direct.apply(original.clone()).unwrap();
        assert_eq!(
            checked.replay(),
            &direct,
            "{name} whole actual continuation"
        );
        assert_source(checked.replay(), checked.receipts().next().unwrap(), &case);
        if let C::ExecuteContract(args) = original {
            if let V::Number(bits) = args[1] {
                seen.insert(f64::from_bits(bits) as u8);
            }
        }
        assert_eq!(checked.receipts().next().unwrap(), &receipt);
        replay = checked.into_replay();
        slices += 1;
    }
    slices
}

#[test]
fn genuinely_composed_lifecycle_source_calls_export_scope_pinned_slices() {
    let mut seen = BTreeSet::new();
    let mut count = 0;
    count += true_slices(
        "source_distribution_lifecycle.rs",
        "REFERENCE",
        P::DistributionLifecycleV5,
        &["entry-main-subgraph"],
        "",
        &mut seen,
    );
    count += true_slices(
        "source_synthetic_lifecycle.rs",
        "REFERENCE",
        P::OptionLifecycleV4,
        &["cold-mint", "warm-mint", "redeem-native", "oracle-redeem"],
        "",
        &mut seen,
    );
    count += true_slices(
        "source_contract_registry_lifecycle.rs",
        "ORACLE",
        P::SourceLifecycleV3,
        &["definition-main-subgraph", "nested-main-subgraph"],
        "",
        &mut seen,
    );
    count += true_slices(
        "source_derivative_exercise.rs",
        "REFERENCE",
        P::OptionLifecycleV4,
        &[
            "entry-main-subgraph",
            "failure-getDatabase",
            "failure-findOne",
            "malformed-json",
            "arg-eval-before-margin",
            "inactive-raw",
        ],
        "",
        &mut seen,
    );
    count += true_slices(
        "source_channel_financial.rs",
        "ORACLE",
        P::SourceLifecycleV3,
        &[
            "raw-contract-channel-funded",
            "raw-contract-channel-shortfall",
            "raw-token-funded-true",
            "raw-token-funded-false",
            "own-channel-queue-direct-drain",
            "own-channel-queue-main-order",
            "imported-payload-channel-queue",
            "imported-payload-late-failure",
            "transfer-rpc-cached-6-failure--1",
            "transfer-rpc-cached-6-failure-0",
            "transfer-same-live-reference-credit-failure",
            "transfer-stale-cache",
            "zero-token-volume-failure-save-factory",
            "witness-raw21-22",
        ],
        r#"
 await reset('witness-raw21-22',{rpc:true});
 await raw('witness-withdraw21',21,e.load('txEncoder').encodeWithdrawal({withdrawAll:false,propertyId:7,amountOffered:3,column:0,channelAddress:C}).slice(3));
 await raw('source-transfer',22,e.load('txEncoder').encodeTransfer({propertyId:7,amount:3,isColumnA:true,pay:false,payRef:'',destinationAddr:'LKDxGDJq5fF4FohAB8zJH24mDDNHDNtqsE'}).slice(3));
"#,
        &mut seen,
    );
    count += true_slices(
        "source_channel_logic.rs",
        "REFERENCE",
        P::SourceLifecycleV3,
        &[
            "direct-metadata",
            "raw-23",
            "raw-31",
            "withdraw-cold",
            "failure-channel-save",
            "failure-late-scaling",
            "settle-record-failure",
            "close-contract-offset-live-alias-failure",
        ],
        "",
        &mut seen,
    );
    count += true_slices(
        "source_option_trade.rs",
        "ORACLE",
        P::OptionLifecycleV4,
        &[
            "ordinary-main-subgraph",
            "raw-main-subgraph",
            "raw-second-validation-failure",
            "raw-execution-receiver-failure",
        ],
        "",
        &mut seen,
    );
    eprintln!("independently composed TLSW2 slices={count}, raw={seen:?}");
    assert!(count > 20, "real exported slices {count}");
    assert!(seen.is_superset(&BTreeSet::from([
        16, 17, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 31
    ])));
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(text: &str) -> [u8; 32] {
    assert_eq!(text.len(), 64);
    let mut bytes = [0; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap();
    }
    bytes
}
fn independent_sha(bytes: &[u8]) -> [u8; 32] {
    let result=std::process::Command::new("node").args(["-e","process.stdout.write(require('node:crypto').createHash('sha256').update(Buffer.from(process.argv[1],'hex')).digest())",&hex(bytes)]).output().unwrap();
    assert!(result.status.success());
    result.stdout.try_into().unwrap()
}
fn crc(bytes: &mut Vec<u8>) {
    let mut v = !0u32;
    for b in bytes.iter() {
        v ^= u32::from(*b);
        for _ in 0..8 {
            v = (v >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(v & 1)));
        }
    }
    bytes.extend_from_slice(&(!v).to_be_bytes());
}
fn repair(bytes: &mut Vec<u8>) {
    bytes.truncate(bytes.len() - 36);
    let mut preimage = b"TradeLayer/SourceLifecycleTransitionWitness/v2/".to_vec();
    preimage.extend_from_slice(bytes);
    bytes.extend_from_slice(&independent_sha(&preimage));
    crc(bytes);
}
fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap()) as usize
}
#[test]
fn independently_pinned_scope_commands_heads_and_repaired_hashes_reject_substitution() {
    let policy = P::DistributionLifecycleV5;
    let scope = Scope::RawTransactionSlice;
    let before = R::with_policy(policy).unwrap();
    let descriptor = SourceTransitionDescriptor::reference();
    let commands = [C::SetHeight(n(-0.)), C::SetHeight(n(10.))];
    // Independently assembled with Node Buffer framing/crypto, literal JSON
    // diagnostic commands, policy5 and scope1. Not read from this witness.
    let pin = unhex("87249c4213685bf833f447e03445af2208c2186fd3644e16324ceb5f042bd0aa");
    assert_eq!(digest(policy, scope, &commands).unwrap(), pin);
    assert_eq!(
        digest(policy, Scope::SourceMethodSlice, &commands).unwrap(),
        unhex("27059781bedb9877f61d6eed45aca6be6eaf446ad99a7b186c414103b1210bea")
    );
    let expected = Expected {
        policy,
        scope,
        descriptor: &descriptor,
        pre_head: before.head(),
        commands_commitment: pin,
        post_head: None,
    };
    let w = W::produce(&before, &commands, &expected).unwrap();
    let encoded = w.canonical_bytes().unwrap();
    let boundary_len = u16::from_be_bytes(encoded[49..51].try_into().unwrap()) as usize;
    let pre = 51 + boundary_len;
    let before_len_at = pre + 40;
    let before_start = before_len_at + 4;
    let count_at = before_start + u32_at(&encoded, before_len_at);
    let first = count_at + 4;
    let second = first + 4 + u32_at(&encoded, first);
    let tail = second + 4 + u32_at(&encoded, second);
    for (offset, error) in [
        (5, E::InvalidEnvelope),
        (6, E::InvalidEnvelope),
        (7, E::WrongScope),
        (8, E::WrongPolicy),
        (9, E::WrongDescriptor),
        (pre + 8, E::WrongPreHead),
        (tail, E::WrongCommands),
        (tail + 32 + 8, E::WrongPostHead),
        (tail + 72, E::WrongPostState),
    ] {
        let mut bad = encoded.clone();
        if offset == 7 {
            bad[offset] = 2;
        } else {
            bad[offset] ^= 1;
        }
        repair(&mut bad);
        assert_eq!(
            W::verify_bytes(&bad, &expected),
            Err(error),
            "repaired self hash at {offset}"
        );
    }
    let mut bad = encoded.clone();
    let one = encoded[first..second].to_vec();
    let two = encoded[second..tail].to_vec();
    bad[first..tail].copy_from_slice(&[two, one].concat());
    let reverse = [commands[1].clone(), commands[0].clone()];
    bad[tail..tail + 32].copy_from_slice(&digest(policy, scope, &reverse).unwrap());
    repair(&mut bad);
    assert_eq!(W::verify_bytes(&bad, &expected), Err(E::WrongCommands));
    for altered in [
        vec![commands[0].clone()],
        vec![commands[0].clone(), commands[0].clone()],
        reverse.to_vec(),
    ] {
        let other = Expected {
            commands_commitment: digest(policy, scope, &altered).unwrap(),
            ..expected
        };
        let alternative = W::produce(&before, &altered, &other).unwrap();
        assert!(alternative.verify(&other).is_ok());
        assert_eq!(alternative.verify(&expected), Err(E::WrongCommands));
    }
    let wrongpost = Expected {
        post_head: Some(before.head()),
        ..expected
    };
    assert_eq!(w.verify(&wrongpost), Err(E::WrongPostHead));
    assert_eq!(
        W::produce(&before, &commands, &wrongpost),
        Err(E::WrongPostHead)
    );
    let mut other = before.clone();
    other.apply(C::SetEntry("types-first".into())).unwrap();
    let alternative = other.checkpoint().unwrap();
    let mut bad = encoded[..before_start].to_vec();
    bad[before_len_at..before_start].copy_from_slice(&(alternative.len() as u32).to_be_bytes());
    bad.extend_from_slice(&alternative);
    bad.extend_from_slice(&encoded[count_at..]);
    repair(&mut bad);
    assert!(
        W::verify_bytes(&bad, &expected).is_err(),
        "alternate module profile cannot replace pinned prehistory"
    );
    let mut bad = encoded.clone();
    bad[before_start + 4] = 3;
    repair(&mut bad);
    assert_eq!(W::verify_bytes(&bad, &expected), Err(E::WrongPolicy));
    let mut wrong = descriptor.clone();
    wrong.boundary.push('!');
    assert_eq!(
        W::verify_bytes(
            &encoded,
            &Expected {
                descriptor: &wrong,
                ..expected
            }
        ),
        Err(E::WrongDescriptor)
    );
    // Correct integrity never upgrades to an actor, UTXO, block-completeness,
    // producer or chain-authority claim.
    assert_eq!(
        w.verify(&expected).unwrap().claim(),
        "conditional-source-raw-command-slice-only"
    );
}
#[test]
fn bounded_framing_and_original_source_value_domains_are_enforced() {
    use tradelayer_core::source_contract_replay::{
        MAX_COMMANDS, MAX_FRAME_BYTES, MAX_SOURCE_LIFECYCLE_TRANSITION_BYTES,
    };
    let policy = P::DistributionLifecycleV5;
    let scope = Scope::RawTransactionSlice;
    let before = R::with_policy(policy).unwrap();
    let descriptor = SourceTransitionDescriptor::reference();
    let commands = [C::SetHeight(n(10.))];
    let expected = Expected {
        policy,
        scope,
        descriptor: &descriptor,
        pre_head: before.head(),
        commands_commitment: digest(policy, scope, &commands).unwrap(),
        post_head: None,
    };
    let b = W::produce(&before, &commands, &expected)
        .unwrap()
        .canonical_bytes()
        .unwrap();
    let pre = 51 + u16::from_be_bytes(b[49..51].try_into().unwrap()) as usize;
    let before_len_at = pre + 40;
    let count_at = before_len_at + 4 + u32_at(&b, before_len_at);
    let first = count_at + 4;
    for (at, value, error) in [
        (before_len_at, u32::MAX, E::Capacity),
        (count_at, (MAX_COMMANDS + 1) as u32, E::Capacity),
        (
            first,
            (MAX_FRAME_BYTES + 1) as u32,
            E::Replay(tradelayer_core::source_contract_replay::SourceContractReplayError::Capacity),
        ),
    ] {
        let mut bad = b.clone();
        bad[at..at + 4].copy_from_slice(&value.to_be_bytes());
        repair(&mut bad);
        assert_eq!(W::verify_bytes(&bad, &expected), Err(error));
    }
    let mut bad = b.clone();
    bad.insert(first + 4, b' ');
    bad[first..first + 4].copy_from_slice(&((u32_at(&b, first) + 1) as u32).to_be_bytes());
    repair(&mut bad);
    assert!(
        W::verify_bytes(&bad, &expected).is_err(),
        "noncanonical JSON whitespace"
    );
    for length in [0, 4, 8, b.len() - 1] {
        assert!(W::verify_bytes(&b[..length], &expected).is_err());
    }
    let mut bad = b.clone();
    bad.extend_from_slice(&[0]);
    assert!(W::verify_bytes(&bad, &expected).is_err());
    assert_eq!(
        W::verify_bytes(
            &vec![0; MAX_SOURCE_LIFECYCLE_TRANSITION_BYTES + 1],
            &expected
        ),
        Err(E::Capacity)
    );
    assert_eq!(
        digest(policy, scope, &vec![commands[0].clone(); MAX_COMMANDS + 1]),
        Err(E::Capacity)
    );
    for original in [
        C::ExecuteContract(vec![n(22.); 8]),
        C::ExecuteContract(vec![n(22.); 10]),
        {
            let mut a = if let C::ExecuteContract(a) = raw(22.) {
                a
            } else {
                unreachable!()
            };
            a[1] = s("22");
            C::ExecuteContract(a)
        },
        invoke("Channels.processWithdrawals", vec![]),
        invoke("Logic.settleLiquidityRewards", vec![]),
    ] {
        assert_eq!(
            digest(policy, scope, &[original]),
            Err(E::UnsupportedCommand)
        );
    }
    let mut deep = V::Null;
    for _ in 0..40 {
        deep = V::Array(vec![deep]);
    }
    assert!(digest(policy, scope, &[C::SetHeight(deep)]).is_err());
    // Local failures are explicit receipts with rollback, never fabricated
    // JavaScript success. The conditional claim must retain this distinction.
    let unsupported = invoke("Logic.transfer", vec![]);
    let w = witness(&before, Scope::SourceMethodSlice, &[unsupported]);
    let expected = Expected {
        scope: Scope::SourceMethodSlice,
        commands_commitment: w.commands_commitment(),
        ..expected
    };
    let check = w.verify(&expected).unwrap();
    assert!(matches!(
        check.receipts().next().unwrap().outcome,
        O::Dependency(DependencyReturn::PendingThrow(_)) | O::LocalError(_)
    ));
}
#[test]
fn frozen_tlsw1_tlsw2_and_original_compiled_tlcr_versions_remain_exact() {
    use tradelayer_core::source_contract_replay::SourceContractReplayHead as Head;
    let policy = P::DistributionLifecycleV5;
    let scope = Scope::RawTransactionSlice;
    let before = R::with_policy(policy).unwrap();
    let descriptor = SourceTransitionDescriptor::reference();
    let expected = Expected {
        policy,
        scope,
        descriptor: &descriptor,
        pre_head: Head {
            commands: 0,
            commitment: unhex("d47df8fa8ae9298bc8c11c40c34a187e4f07ad21539e828eccb525857b3cb6f1"),
        },
        commands_commitment: unhex(
            "87249c4213685bf833f447e03445af2208c2186fd3644e16324ceb5f042bd0aa",
        ),
        post_head: Some(Head {
            commands: 2,
            commitment: unhex("a9676e50c5177affc566773a5f0c6750b0663ddcf92b7c8d31ae25742024e4f3"),
        }),
    };
    let bytes = include_bytes!("../fixtures/source_lifecycle_transition_witness_v2.bin");
    assert_eq!(
        hex(&independent_sha(bytes)),
        "a7a829edb561d46462bfafd98677286b7c38aa0f74a0c73ac68df31d6b80e2f3"
    );
    let checked = W::verify_bytes(bytes, &expected).unwrap();
    assert_eq!(
        checked.post_state_commitment(),
        unhex("4d207158218500f15a72e8fb6e0723582998f0f0a0173276ac70486dd3bc62a1")
    );
    assert_eq!(
        W::produce(
            &before,
            &[C::SetHeight(n(-0.)), C::SetHeight(n(10.))],
            &expected
        )
        .unwrap()
        .canonical_bytes()
        .unwrap(),
        bytes
    );
    let old_bytes = include_bytes!("../fixtures/source_transition_witness_v1_compat.bin");
    assert_eq!(
        hex(&independent_sha(old_bytes)),
        "55fb2bab30af832996cd20f0d37a6119654a4d619abf29a2c0dd726ef5da86cc"
    );
    let old = ExpectedV1 {
        policy,
        descriptor: &descriptor,
        pre_head: expected.pre_head,
        commands_commitment: None,
        post_head: expected.post_head,
    };
    V1::verify_bytes(old_bytes, &old).unwrap();
    assert_eq!(
        V1::produce(&before, &[C::SetHeight(n(-0.)), C::SetHeight(n(10.))], &old)
            .unwrap()
            .canonical_bytes()
            .unwrap(),
        old_bytes
    );
    assert_eq!(
        W::verify_bytes(old_bytes, &expected),
        Err(E::InvalidEnvelope)
    );
    assert!(V1::verify_bytes(bytes, &old).is_err());
    // These four snapshots were captured from earlier independently compiled
    // engines, not created by this new profile or its producer.
    for (version, count, head) in [
        (
            1,
            5,
            "b637ad06da4ccd4d88dfaaf31a6cb898589fc1cb5c5c78355f8c57f54eba011c",
        ),
        (
            2,
            6,
            "7f7338c21e5036f7b1fc1e2808cf738bd7f8fd69c8427ff9ef5f40113b202b9a",
        ),
        (
            3,
            10,
            "65e94c85b2a693163ad9638cae62f49c1175b8a6596f43df32cc7bd4e856e3fa",
        ),
        (
            4,
            14,
            "a294259075ae5cb069be1f549fae755f02e0d6d166321ca85084e6a7454dd5ee",
        ),
    ] {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(format!("source_contract_replay_v{version}.bin")),
        )
        .unwrap();
        let replay = R::from_checkpoint(
            &bytes,
            Head {
                commands: count,
                commitment: unhex(head),
            },
        )
        .unwrap();
        assert_eq!(replay.checkpoint().unwrap(), bytes);
    }
}

#[test]
fn genuinely_populated_v6_proof_and_reward_slices_bind_original_contexts() {
    let mut raw = BTreeSet::new();
    let proof = true_slices(
        "source_proof_lifecycle.rs",
        "REFERENCE",
        P::ProofLifecycleV6,
        &[
            "entry-main-subgraph",
            "ordinary-settlement",
            "cache-lifecycle-uphold",
            "cache-lifecycle-reject",
            "cache-lifecycle-payout",
            "risk-environment",
            "state-root-environment",
            "bundle-missing",
            "failure-settlement-redeem-save",
            "failure-settlement-cache-record",
        ],
        "",
        &mut raw,
    );
    let rewards = true_slices(
        "source_liquidity_rewards.rs",
        "REFERENCE",
        P::ProofLifecycleV6,
        &[
            "initial-ten",
            "producer-eligible-seven",
            "producer-default-property-zero",
            "producer-failure-tradeHistory-update-0",
            "settlement-failure-tallyMapDelta-insert-1",
            "producer-nan-environment-LIQUIDITY_REWARD_MAX_SHARE",
        ],
        "",
        &mut raw,
    );
    eprintln!("actual V6 proof slices={proof}, rewards={rewards}, raw={raw:?}");
    assert_eq!((proof, rewards), (76, 24));
    assert!(raw.contains(&30));
}
