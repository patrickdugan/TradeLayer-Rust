use tradelayer_core::source_contract_replay::{
    source_transition_commands_commitment, ConditionalSourceTransition, SourceContractCommand as C,
    SourceContractCommandOutcome as O, SourceContractReplay, SourceContractReplayHead,
    SourceTransitionDescriptor, SourceTransitionError as E, SourceTransitionExpectation,
    SourceTransitionWitness, MAX_COMMANDS, MAX_FRAME_BYTES,
};
use tradelayer_core::source_contract_runtime::{SourceContractRuntimePolicy as P, StorageFailure};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceCall, SourceOutcome, SourceValue as V,
};
fn s(v: &str) -> V {
    V::String(v.into())
}
fn n(v: f64) -> V {
    V::number(v)
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn text(v: V) -> String {
    let V::String(v) = v else { panic!("string") };
    v
}
fn array(v: V) -> Vec<V> {
    let V::Array(v) = v else { panic!("array") };
    v
}
fn observed(outcome: &O) -> V {
    if let O::Source(SourceOutcome::Thrown(e)) = outcome {
        return obj(&[
            ("params", V::Undefined),
            (
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            ),
        ]);
    }
    let (returned, error) = match outcome {
        O::Void => (Some(V::Undefined), None),
        O::Source(SourceOutcome::Returned(v)) => (Some(v.clone()), None),
        O::Source(SourceOutcome::Thrown(e)) => (None, Some(e)),
        O::Dependency(DependencyReturn::Immediate(v) | DependencyReturn::Pending(v)) => {
            (Some(v.clone()), None)
        }
        O::Dependency(DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e)) => {
            (None, Some(e))
        }
        O::LocalError(e) => panic!("source path unavailable: {e:?}"),
        O::RichDependencyV1(_) => panic!("rich outcome outside this typed legacy witness corpus"),
    };
    returned.map_or_else(
        || {
            let e = error.unwrap();
            obj(&[(
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )])
        },
        |v| obj(&[("returned", v)]),
    )
}
fn output(reference: &str, extra: &[&str]) -> Vec<V> {
    let result = std::process::Command::new("node")
        .args(["-e", reference, env!("CARGO_MANIFEST_DIR")])
        .args(extra)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    std::str::from_utf8(&result.stdout)
        .unwrap()
        .lines()
        .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect()
}
fn invoke(target: &str, arguments: Vec<V>) -> C {
    C::Invoke(DependencyCall {
        target: target.into(),
        arguments,
    })
}
fn returned(outcome: O) -> V {
    match outcome {
        O::Dependency(DependencyReturn::Immediate(v) | DependencyReturn::Pending(v)) => v,
        other => panic!("returned value expected: {other:?}"),
    }
}
fn expectation<'a>(
    descriptor: &'a SourceTransitionDescriptor,
    before: &SourceContractReplay,
) -> SourceTransitionExpectation<'a> {
    SourceTransitionExpectation {
        policy: before.runtime().policy(),
        descriptor,
        pre_head: before.head(),
        commands_commitment: None,
        post_head: None,
    }
}
fn verify_slice(
    before: &SourceContractReplay,
    commands: &[C],
) -> (SourceTransitionWitness, ConditionalSourceTransition) {
    let descriptor = SourceTransitionDescriptor::reference();
    let mut expected = expectation(&descriptor, before);
    let mut direct = before.clone();
    for command in commands {
        direct.apply(command.clone()).unwrap();
    }
    expected.commands_commitment = Some(source_transition_commands_commitment(commands).unwrap());
    expected.post_head = Some(direct.head());
    let saved = before.clone();
    let witness = SourceTransitionWitness::produce(before, commands, &expected).unwrap();
    assert!(before == &saved, "producer mutated before owner");
    let encoded = witness.canonical_bytes().unwrap();
    assert_eq!(&encoded[..5], b"TLSW\x01");
    let evidence = SourceTransitionWitness::verify_bytes(&encoded, &expected).unwrap();
    assert_eq!(evidence.scope(), "conditional-source-execution-only");
    assert!(
        evidence.replay() == &direct,
        "actual whole continuation owner differs"
    );
    assert_eq!(evidence.post_head(), direct.head());
    assert_eq!(
        evidence.post_state_commitment(),
        witness.post_state_commitment()
    );
    assert_eq!(evidence.receipts().count(), commands.len());
    assert!(witness.verify(&expected).unwrap() == evidence);
    (witness, evidence)
}

// These are frozen observations from unchanged hash-pinned CommonJS modules.
// This projector only converts diagnostic Map tags; it supplies no dependency
// result, validity decision, match, balance mutation, or state to execution.
const CORPUS: &str = r###"
const fs=require('node:fs'),path=require('node:path');const root=process.argv[1],scene=process.argv[2];
const norm=v=>v[0]==='map'?['array',v[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:v[0]==='object'?['object',v[1].map(([k,v])=>[k,norm(v)])]:v[0]==='array'?['array',v[1].map(norm)]:v;
for(const line of fs.readFileSync(path.join(root,'fixtures/source_contract_execution_vectors.tsv'),'utf8').split('\n')){const f=line.split('\t');if(f[0]!=='V'||!f[1].startsWith(scene+'/'))continue;const read=i=>norm(JSON.parse(Buffer.from(f[i],'hex')));console.log(JSON.stringify(['object',[['name',['string',f[1]]],['command',read(2)],['outcome',read(3)],['state',read(4)]]]));}
"###;
fn tables(replay: &SourceContractReplay, state: V, label: &str) {
    for table in array(state.get("tables")) {
        let name = text(table.get("name"));
        assert_eq!(
            replay.runtime().collection(&name),
            array(table.get("rows")),
            "{label} genuine complete {name}"
        );
    }
}
fn original_command(v: &V) -> C {
    if matches!(v.get("raw"), V::String(_)) {
        return C::ExecuteContract(vec![
            v.get("txid"),
            n(18.0),
            s("tl"),
            v.get("raw"),
            v.get("sender"),
            V::Null,
            n(0.0),
            n(0.0),
            if v.get("block") == V::Undefined {
                n(10.0)
            } else {
                v.get("block")
            },
        ]);
    }
    if v.get("drain") != V::Undefined {
        return C::EvaluateQueue(SourceCall {
            method: "processQueuedOnChainOrdersForBlock".into(),
            arguments: vec![v.get("drain")],
        });
    }
    if v.get("flushFees") != V::Undefined {
        return invoke("Tally.flushQueuedFeeAccruals", vec![v.get("flushFees")]);
    }
    panic!("unexpected genuine command")
}
#[test]
fn genuine_type18_matches_fees_iou_source_failure_and_retry_export_actual_slices() {
    let mut slices = 0;
    let mut source_threw = false;
    for scene in [
        "ordinary-matched-quantity-two",
        "tally-preload-matched-fee-flush",
        "late-volume-write-failure-retry",
        "imported-partial-close-no-mark-iou-profit",
    ] {
        let cases = output(CORPUS, &[scene]);
        let mut before = SourceContractReplay::with_policy(P::GlobalClearingV2).unwrap();
        let options = cases[0].get("command").get("scene");
        let entry = if options.get("preload") == s("tally") {
            "tally-first"
        } else if options.get("preload") == s("volumeIndex") {
            "volume-first"
        } else if options.get("entry") == s("types-first") {
            "types-first"
        } else {
            "main-subgraph"
        };
        before.apply(C::SetEntry(entry.into())).unwrap();
        before
            .apply(C::SetActivation(options.get("inactive") != V::Bool(true)))
            .unwrap();
        for table in array(cases[0].get("state").get("tables")) {
            before
                .apply(C::Import {
                    collection: text(table.get("name")),
                    rows: array(table.get("rows")),
                })
                .unwrap();
        }
        for case in &cases[1..] {
            let command = case.get("command");
            if matches!(command.get("failure"), V::Object(_)) {
                let failure = command.get("failure");
                before
                    .apply(C::FailStorage(StorageFailure {
                        collection: text(failure.get("collection")),
                        operation: format!("{}Async", text(failure.get("op"))),
                        skip: match failure.get("skip") {
                            V::Number(bits) => f64::from_bits(bits) as usize,
                            _ => 0,
                        },
                        message: text(failure.get("message")),
                    }))
                    .unwrap();
                continue;
            }
            let cmd = original_command(&command);
            let height = if command.get("block") != V::Undefined {
                command.get("block")
            } else if command.get("drain") != V::Undefined {
                command.get("drain")
            } else if command.get("flushFees") != V::Undefined {
                command.get("flushFees")
            } else {
                n(10.0)
            };
            let (_, evidence) = verify_slice(&before, &[C::SetHeight(height), cmd]);
            let receipt = evidence.receipts().last().unwrap();
            let actual = if matches!(command.get("raw"), V::String(_)) {
                match &receipt.outcome {
                    O::Source(SourceOutcome::Returned(v)) => v.clone(),
                    O::Source(SourceOutcome::Thrown(e)) => obj(&[
                        (
                            "params",
                            evidence
                                .replay()
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
                    other => panic!("raw source outcome {other:?}"),
                }
            } else {
                observed(&receipt.outcome)
            };
            // Source fixture includes diagnostic logs as an extra output field.
            let expected = case.get("outcome");
            let fields = match expected {
                V::Object(v) => v.into_iter().filter(|(k, _)| k != "sourceLogs").collect(),
                _ => panic!(),
            };
            assert_eq!(
                actual,
                V::Object(fields),
                "{} genuine outcome",
                text(case.get("name"))
            );
            tables(
                evidence.replay(),
                case.get("state"),
                &text(case.get("name")),
            );
            source_threw |= matches!(receipt.outcome, O::Source(SourceOutcome::Thrown(_)));
            before = evidence.into_replay();
            slices += 1;
        }
    }
    assert!(
        source_threw,
        "late genuine financial write must throw before retry"
    );
    assert_eq!(slices, 14, "all four genuine scenes must execute");
}

const GLOBAL_ALIAS: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));code=code.replace('return {types,logic,Orderbook,effects','return {tables,types,logic,Orderbook,effects');code+='\nmodule.exports={environment,tag};\n';const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);const{environment,tag}=driver.exports;
const norm=v=>v[0]==='map'?['array',v[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:v[0]==='object'?['object',v[1].map(([k,v])=>[k,norm(v)])]:v[0]==='array'?['array',v[1].map(norm)]:v;
const emit=v=>console.log(JSON.stringify(norm(tag(v))));
(async()=>{const e=environment({});e.table('contractList').splice(0);e.table('marginMaps').splice(0);const c=e.load('clearing'),Margin=e.load('marginMap');
emit({tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)}))});const m=await Margin.getInstance(1);const pos={address:'actor',contracts:1,avgPrice:100,margin:10};const key=c.initPositionCache(1,10,[pos]);await m.mergePositions(c.getPositionsFromCache(key),1,true);
let outcome;try{outcome={returned:await c.clearingFunction(10)}}catch(e){outcome={thrown:{name:e.name,message:e.message}}};emit({outcome,tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)}))});
c.applyAssignedUnderlyingPosition(c.getPositionsFromCache(key)[0],1,102,2);emit({positions:c.getPositionsFromCache(key),position:m.margins.get('actor')});
})().catch(e=>{console.error(e);process.exitCode=1});
"###;
#[test]
fn genuine_global_clearing_preserves_live_margin_context_aliases_across_witness_restore() {
    let source = output(GLOBAL_ALIAS, &[]);
    let mut before = SourceContractReplay::with_policy(P::GlobalClearingV2).unwrap();
    for table in array(source[0].get("tables")) {
        before
            .apply(C::Import {
                collection: text(table.get("name")),
                rows: array(table.get("rows")),
            })
            .unwrap();
    }
    let mut handle = returned(
        before
            .apply(invoke("Margin.getInstance", vec![n(1.0)]))
            .unwrap()
            .outcome,
    );
    let position = obj(&[
        ("address", s("actor")),
        ("contracts", n(1.0)),
        ("avgPrice", n(100.0)),
        ("margin", n(10.0)),
    ]);
    before
        .apply(invoke(
            "Clearing.initPositionCache",
            vec![n(1.0), n(10.0), V::Array(vec![position.clone()])],
        ))
        .unwrap();
    handle
        .insert(
            "sourcePositionObjectAliases",
            V::Array(vec![obj(&[
                ("argument", n(0.0)),
                ("index", n(0.0)),
                ("object", n(0.0)),
            ])]),
        )
        .unwrap();
    before
        .apply(invoke(
            "Margin.mergePositions",
            vec![
                handle.clone(),
                V::Array(vec![position]),
                n(1.0),
                V::Bool(true),
            ],
        ))
        .unwrap();
    let (_, evidence) = verify_slice(
        &before,
        &[
            C::SetHeight(n(10.0)),
            invoke("Clearing.clearingFunction", vec![n(10.0)]),
        ],
    );
    assert_eq!(
        observed(&evidence.receipts().last().unwrap().outcome),
        source[1].get("outcome")
    );
    tables(
        evidence.replay(),
        source[1].clone(),
        "genuine global clearing",
    );
    let mut after = evidence.into_replay();
    after
        .apply(invoke(
            "Clearing.applyAssignedUnderlyingPosition",
            vec![
                obj(&[("sourcePositionObject", n(0.0))]),
                n(1.0),
                n(102.0),
                n(2.0),
            ],
        ))
        .unwrap();
    let positions = returned(
        after
            .apply(invoke("Clearing.getPositionsFromCache", vec![s("1:10")]))
            .unwrap()
            .outcome,
    );
    let position = returned(
        after
            .apply(invoke("Margin.inspectPosition", vec![handle, s("actor")]))
            .unwrap()
            .outcome,
    );
    assert_eq!(positions, source[2].get("positions"));
    assert_eq!(position, source[2].get("position"));
}

fn crc(bytes: &mut Vec<u8>) {
    let mut value = !0u32;
    for byte in bytes.iter() {
        value ^= u32::from(*byte);
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(value & 1)));
        }
    }
    bytes.extend_from_slice(&(!value).to_be_bytes());
}
fn repair(bytes: &mut Vec<u8>) {
    bytes.truncate(bytes.len() - 36);
    let mut preimage = b"TradeLayer/SourceTransitionWitness/v1/".to_vec();
    preimage.extend_from_slice(bytes);
    // Independent test-side SHA256 repairs the attacker's self hash. Production
    // verification uses the existing Rust implementation, never Node.
    let hex = preimage
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let digest = std::process::Command::new("node").args([
        "-e", "process.stdout.write(require('node:crypto').createHash('sha256').update(Buffer.from(process.argv[1],'hex')).digest())", &hex,
    ]).output().unwrap();
    assert!(digest.status.success());
    assert_eq!(digest.stdout.len(), 32);
    bytes.extend_from_slice(&digest.stdout);
    crc(bytes);
}
fn u32_at(bytes: &[u8], at: usize) -> usize {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}
#[test]
fn policy_head_descriptor_command_profile_and_repaired_self_hashes_never_supply_authority() {
    let before = SourceContractReplay::with_policy(P::GlobalClearingV2).unwrap();
    let descriptor = SourceTransitionDescriptor::reference();
    let expected = expectation(&descriptor, &before);
    let commands = [C::SetHeight(n(10.0)), C::SetHeight(n(11.0))];
    let (witness, _) = verify_slice(&before, &commands);
    let encoded = witness.canonical_bytes().unwrap();
    let boundary_len = u16::from_be_bytes(encoded[48..50].try_into().unwrap()) as usize;
    let pre = 50 + boundary_len;
    let before_len_at = pre + 40;
    let before_start = before_len_at + 4;
    let count_at = before_start + u32_at(&encoded, before_len_at);
    let first_len_at = count_at + 4;
    let second_len_at = first_len_at + 4 + u32_at(&encoded, first_len_at);
    let commands_hash_at = second_len_at + 4 + u32_at(&encoded, second_len_at);
    for (offset, expected_error) in [
        (5, E::InvalidEnvelope),
        (7, E::WrongPolicy),
        (8, E::WrongDescriptor),
        (pre + 8, E::WrongPreHead),
    ] {
        let mut bad = encoded.clone();
        bad[offset] ^= 1;
        repair(&mut bad);
        assert_eq!(
            SourceTransitionWitness::verify_bytes(&bad, &expected),
            Err(expected_error)
        );
    }
    let mut bad = encoded.clone();
    bad[commands_hash_at + 32 + 40] ^= 1;
    repair(&mut bad);
    assert_eq!(
        SourceTransitionWitness::verify_bytes(&bad, &expected),
        Err(E::WrongPostState)
    );
    let mut bad = encoded.clone();
    bad[before_len_at..before_len_at + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    repair(&mut bad);
    assert_eq!(
        SourceTransitionWitness::verify_bytes(&bad, &expected),
        Err(E::Capacity)
    );
    let mut bad = encoded.clone();
    bad[count_at..count_at + 4].copy_from_slice(&((MAX_COMMANDS + 1) as u32).to_be_bytes());
    repair(&mut bad);
    assert_eq!(
        SourceTransitionWitness::verify_bytes(&bad, &expected),
        Err(E::Capacity)
    );
    let mut bad = encoded.clone();
    bad[first_len_at..first_len_at + 4]
        .copy_from_slice(&((MAX_FRAME_BYTES + 1) as u32).to_be_bytes());
    repair(&mut bad);
    assert!(SourceTransitionWitness::verify_bytes(&bad, &expected).is_err());
    let mut bad = encoded.clone();
    bad.insert(first_len_at + 4, b' ');
    bad[first_len_at..first_len_at + 4]
        .copy_from_slice(&((u32_at(&encoded, first_len_at) + 1) as u32).to_be_bytes());
    repair(&mut bad);
    assert!(
        SourceTransitionWitness::verify_bytes(&bad, &expected).is_err(),
        "valid JSON whitespace is not canonical command framing"
    );
    let mut other_before = before.clone();
    other_before.apply(C::SetHeight(n(99.0))).unwrap();
    let other_bytes = other_before.checkpoint().unwrap();
    let mut bad = encoded[..before_start].to_vec();
    bad[before_len_at..before_start].copy_from_slice(&(other_bytes.len() as u32).to_be_bytes());
    bad.extend_from_slice(&other_bytes);
    bad.extend_from_slice(&encoded[count_at..]);
    repair(&mut bad);
    assert!(
        SourceTransitionWitness::verify_bytes(&bad, &expected).is_err(),
        "a valid alternate prestate journal cannot replace the independent head"
    );
    let mut bad = encoded.clone();
    let second = encoded[second_len_at..commands_hash_at].to_vec();
    let first = encoded[first_len_at..second_len_at].to_vec();
    bad[first_len_at..commands_hash_at].copy_from_slice(&[second, first].concat());
    let reversed = [commands[1].clone(), commands[0].clone()];
    bad[commands_hash_at..commands_hash_at + 32]
        .copy_from_slice(&source_transition_commands_commitment(&reversed).unwrap());
    repair(&mut bad);
    assert_eq!(
        SourceTransitionWitness::verify_bytes(&bad, &expected),
        Err(E::WrongPostHead)
    );
    let pinned = SourceTransitionExpectation {
        commands_commitment: Some(witness.commands_commitment()),
        ..expectation(&descriptor, &before)
    };
    assert_eq!(
        SourceTransitionWitness::verify_bytes(&bad, &pinned),
        Err(E::WrongCommands)
    );
    let altered = SourceTransitionWitness::produce(&before, &reversed, &expected).unwrap();
    assert!(altered.verify(&expected).is_ok(), "a different declared slice is valid conditional execution without an independent command pin");
    assert_eq!(altered.verify(&pinned), Err(E::WrongCommands));
    for command in [
        C::Import {
            collection: "tallyMap".into(),
            rows: vec![],
        },
        C::SetEntry("main-subgraph".into()),
        C::SetActivation(true),
        C::FailStorage(StorageFailure {
            collection: "tallyMap".into(),
            operation: "findAsync".into(),
            skip: 0,
            message: "fake".into(),
        }),
        invoke("Tally.updateBalance", vec![]),
        invoke("Oracle.publishData", vec![n(1.0)]),
        C::ExecuteContract(vec![V::Undefined; 9]),
        C::ExecuteContract(vec![n(18.0); 8]),
        C::EvaluateQueue(SourceCall {
            method: "queueOnChainContractOrder".into(),
            arguments: vec![n(10.0)],
        }),
        invoke("Clearing.clearingFunction", vec![]),
    ] {
        assert_eq!(
            SourceTransitionWitness::produce(&before, &[command], &expected),
            Err(E::UnsupportedCommand)
        );
    }
    let mut deep = V::Null;
    for _ in 0..40 {
        deep = V::Array(vec![deep]);
    }
    assert!(SourceTransitionWitness::produce(&before, &[C::SetHeight(deep)], &expected).is_err());
    let legacy = SourceContractReplay::with_policy(P::OnchainV1).unwrap();
    let old = legacy.checkpoint().unwrap();
    let legacy_expected = expectation(&descriptor, &legacy);
    let legacy_witness = SourceTransitionWitness::produce(
        &legacy,
        &[invoke("Clearing.clearingFunction", vec![n(10.0)])],
        &legacy_expected,
    )
    .unwrap();
    let evidence = legacy_witness.verify(&legacy_expected).unwrap();
    assert!(matches!(
        evidence.receipts().last().unwrap().outcome,
        O::LocalError(_)
    ));
    assert_eq!(
        evidence.pre_state_commitment(),
        evidence.post_state_commitment(),
        "local unavailable dependency keeps the complete prestate"
    );
    assert_eq!(legacy.checkpoint().unwrap(), old);
    assert_eq!(&legacy_witness.before_checkpoint()[..5], b"TLCR\x01");
    assert_eq!(
        SourceTransitionWitness::verify_bytes(
            &legacy_witness.canonical_bytes().unwrap(),
            &expected
        ),
        Err(E::WrongPolicy)
    );
    let lifecycle = SourceContractReplay::with_policy(P::SourceLifecycleV3).unwrap();
    let (current, current_evidence) = verify_slice(
        &lifecycle,
        &[
            C::SetHeight(n(10.0)),
            invoke("Clearing.clearingFunction", vec![n(10.0), V::Bool(false)]),
        ],
    );
    assert_eq!(&current.before_checkpoint()[..5], b"TLCR\x03");
    assert_eq!(
        current_evidence.replay().runtime().policy(),
        P::SourceLifecycleV3
    );
    assert_eq!(
        SourceTransitionWitness::verify_bytes(&current.canonical_bytes().unwrap(), &expected),
        Err(E::WrongPolicy)
    );
    let wrong_descriptor = SourceTransitionDescriptor {
        boundary: "untrusted-backend".into(),
        ..descriptor.clone()
    };
    let wrong = SourceTransitionExpectation {
        descriptor: &wrong_descriptor,
        ..expectation(&descriptor, &before)
    };
    assert_eq!(witness.verify(&wrong), Err(E::WrongDescriptor));
    let wrong_head = SourceTransitionExpectation {
        pre_head: SourceContractReplayHead {
            commands: 1,
            ..before.head()
        },
        ..expectation(&descriptor, &before)
    };
    assert_eq!(witness.verify(&wrong_head), Err(E::WrongPreHead));
}
