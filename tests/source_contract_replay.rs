use std::collections::BTreeMap;
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    SourceContractReplayError as E, MAX_COMMANDS, MAX_FRAME_BYTES,
};
use tradelayer_core::source_contract_runtime::StorageFailure;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceCall, SourceOutcome, SourceRuleError, SourceValue as V,
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
fn number(v: V) -> usize {
    let V::Number(v) = v else { panic!("number") };
    f64::from_bits(v) as usize
}
fn decode(hex: &str) -> V {
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    tradelayer_core::property_catalog::parse_diagnostic_value(std::str::from_utf8(&bytes).unwrap())
        .unwrap()
}
fn present(v: &V, k: &str) -> bool {
    !matches!(v.get(k), V::Undefined)
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
        O::LocalError(e) => panic!("actual source path unavailable: {e:?}"),
        O::RichDependencyV1(_) => panic!("rich outcome outside this typed legacy source corpus"),
    };
    if let Some(v) = returned {
        obj(&[("returned", v)])
    } else {
        let e = error.unwrap();
        obj(&[(
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )])
    }
}
const PROJECT: &str = r#"
const fs=require('node:fs'),path=require('node:path');const root=process.argv[1],scene=process.argv[2];
const rows=fs.readFileSync(path.join(root,'fixtures/source_contract_execution_vectors.tsv'),'utf8').split('\n');
const norm=v=>v[0]==='map'?['array',v[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:v[0]==='object'?['object',v[1].map(([k,v])=>[k,norm(v)])]:v[0]==='array'?['array',v[1].map(norm)]:v;
const get=(v,key)=>v[1].find(([k])=>k===key)?.[1]??['undefined'];const hex=v=>Buffer.from(JSON.stringify(norm(v))).toString('hex');
for(const line of rows){const f=line.split('\t');if(f[0]!=='V'||!f[1].startsWith(scene+'/'))continue;const cmd=JSON.parse(Buffer.from(f[2],'hex')),out=JSON.parse(Buffer.from(f[3],'hex')),state=JSON.parse(Buffer.from(f[4],'hex'));console.log(['V',f[1],hex(cmd),hex(['object',out[1].filter(([k])=>k!=='sourceLogs')])].join('\t'));for(const table of get(state,'tables')[1]){const name=get(table,'name')[1];console.log(['C',name].join('\t'));for(const row of get(table,'rows')[1])console.log(['R',name,hex(row)].join('\t'));}}
"#;
#[derive(Default)]
struct Case {
    name: String,
    command: Option<V>,
    outcome: Option<V>,
    tables: BTreeMap<String, Vec<V>>,
}
fn cases(scene: &str) -> Vec<Case> {
    let output = std::process::Command::new("node")
        .args(["-e", PROJECT, env!("CARGO_MANIFEST_DIR"), scene])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut out = vec![];
    let mut current = Case::default();
    for line in String::from_utf8(output.stdout).unwrap().lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields[0] {
            "V" => {
                if current.command.is_some() {
                    out.push(current)
                }
                current = Case {
                    name: fields[1].into(),
                    command: Some(decode(fields[2])),
                    outcome: Some(decode(fields[3])),
                    tables: BTreeMap::new(),
                };
            }
            "C" => {
                current.tables.insert(fields[1].into(), vec![]);
            }
            "R" => current
                .tables
                .get_mut(fields[1])
                .unwrap()
                .push(decode(fields[2])),
            _ => panic!(),
        }
    }
    if current.command.is_some() {
        out.push(current)
    }
    assert!(!out.is_empty());
    out
}
fn bootstrap(case: &Case) -> SourceContractReplay {
    let mut replay = SourceContractReplay::new().unwrap();
    let scene = case.command.as_ref().unwrap().get("scene");
    let entry = if scene.get("preload") == s("tally") {
        "tally-first"
    } else if scene.get("preload") == s("volumeIndex") {
        "volume-first"
    } else if scene.get("entry") == s("types-first") {
        "types-first"
    } else {
        "main-subgraph"
    };
    replay.apply(C::SetEntry(entry.into())).unwrap();
    replay
        .apply(C::SetActivation(scene.get("inactive") != V::Bool(true)))
        .unwrap();
    for (collection, rows) in &case.tables {
        replay
            .apply(C::Import {
                collection: collection.clone(),
                rows: rows.clone(),
            })
            .unwrap();
    }
    replay
}
fn command(case: &Case) -> C {
    let v = case.command.as_ref().unwrap();
    if let V::String(raw) = v.get("raw") {
        return C::ExecuteContract(vec![
            v.get("txid"),
            n(18.0),
            s("tl"),
            s(&raw),
            v.get("sender"),
            V::Null,
            n(0.0),
            n(0.0),
            if present(v, "block") {
                v.get("block")
            } else {
                n(10.0)
            },
        ]);
    }
    if present(v, "drain") {
        return C::EvaluateQueue(SourceCall {
            method: "processQueuedOnChainOrdersForBlock".into(),
            arguments: vec![v.get("drain")],
        });
    }
    if present(v, "flushFees") {
        return C::Invoke(DependencyCall {
            target: "Tally.flushQueuedFeeAccruals".into(),
            arguments: vec![v.get("flushFees")],
        });
    }
    if present(v, "failure") {
        let f = v.get("failure");
        return C::FailStorage(StorageFailure {
            collection: text(f.get("collection")),
            operation: format!("{}Async", text(f.get("op"))),
            skip: if present(&f, "skip") {
                number(f.get("skip"))
            } else {
                0
            },
            message: text(f.get("message")),
        });
    }
    panic!("unhandled actual source command: {v:?}")
}
fn assert_source(case: &Case, replay: &SourceContractReplay, outcome: &O) {
    let actual = if present(case.command.as_ref().unwrap(), "raw") {
        match outcome {
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
            _ => panic!("{} raw source outcome {outcome:?}", case.name),
        }
    } else {
        observed(outcome)
    };
    assert_eq!(
        &actual,
        case.outcome.as_ref().unwrap(),
        "{} genuine source outcome",
        case.name
    );
    for (name, expected) in &case.tables {
        assert_eq!(
            replay.runtime().collection(name),
            expected.as_slice(),
            "{} complete {name}",
            case.name
        )
    }
}

#[test]
fn genuine_positions_fees_iou_and_failed_volume_prefix_replay_from_pinned_commands() {
    for scene in [
        "ordinary-matched-quantity-two",
        "tally-preload-matched-fee-flush",
        "late-volume-write-failure-retry",
        "imported-partial-close-no-mark-iou-profit",
    ] {
        let cases = cases(scene);
        let mut replay = bootstrap(&cases[0]);
        let mut threw = false;
        for case in &cases[1..] {
            let input = case.command.as_ref().unwrap();
            let height = if present(input, "block") {
                input.get("block")
            } else if present(input, "drain") {
                input.get("drain")
            } else if present(input, "flushFees") {
                input.get("flushFees")
            } else {
                n(10.0)
            };
            replay.apply(C::SetHeight(height)).unwrap();
            let cmd = command(case);
            let receipt = replay.apply(cmd.clone()).unwrap();
            threw |= matches!(receipt.outcome, O::Source(SourceOutcome::Thrown(_)));
            assert_source(case, &replay, &receipt.outcome);
            let bytes = replay.checkpoint().unwrap();
            let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
            assert_eq!(
                restored, replay,
                "{} exact replay including continuation",
                case.name
            );
            assert_eq!(restored.checkpoint().unwrap(), bytes);
            replay = restored;
        }
        if scene == "late-volume-write-failure-retry" {
            assert!(threw, "actual late source financial throw retained");
        }
        let mut restored =
            SourceContractReplay::from_checkpoint(&replay.checkpoint().unwrap(), replay.head())
                .unwrap();
        // A new real fee command consumes clocks/DB IDs/Insurance state after
        // restore. It must have exactly the uninterrupted outcome and effects.
        let tail = C::Invoke(DependencyCall {
            target: "Tally.accrueFee".into(),
            arguments: vec![n(7.0), n(0.0002), n(1.0), n(11.0)],
        });
        assert_eq!(
            replay.apply(tail.clone()).unwrap(),
            restored.apply(tail).unwrap()
        );
        assert_eq!(replay, restored);
    }
}

fn fix_crc(bytes: &mut Vec<u8>) {
    bytes.truncate(bytes.len() - 4);
    let mut crc = !0u32;
    for byte in bytes.iter() {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    bytes.extend_from_slice(&(!crc).to_be_bytes());
}
fn u32_at(bytes: &[u8], at: usize) -> usize {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}
fn ranges(bytes: &[u8]) -> Vec<core::ops::Range<usize>> {
    let mut at = 73;
    let mut rows = vec![];
    for _ in 0..u32_at(bytes, 37) {
        let start = at;
        at += 40;
        for _ in 0..2 {
            let len = u32_at(bytes, at);
            at += 4 + len;
        }
        let count = u32_at(bytes, at);
        at += 4;
        for _ in 0..count {
            let len = u32_at(bytes, at);
            at += 4 + len;
        }
        at += 96;
        rows.push(start..at)
    }
    rows
}
#[test]
fn external_head_and_actual_replay_reject_checksum_repaired_history_splices() {
    let mut replay = SourceContractReplay::new().unwrap();
    replay.apply(C::SetHeight(n(10.0))).unwrap();
    replay.apply(C::SetHeight(n(20.0))).unwrap();
    replay.apply(C::SetActivation(false)).unwrap();
    let expected = replay.head();
    let bytes = replay.checkpoint().unwrap();
    let spans = ranges(&bytes);
    assert_eq!(spans.len(), 3);
    let mut wrong = expected;
    wrong.commitment[0] ^= 1;
    assert_eq!(
        SourceContractReplay::from_checkpoint(&bytes, wrong),
        Err(E::WrongHead)
    );
    let mut bad = bytes.clone();
    bad[spans[0].start + 7] ^= 1;
    fix_crc(&mut bad);
    assert!(SourceContractReplay::from_checkpoint(&bad, expected).is_err());
    let mut reordered = bytes[..73].to_vec();
    reordered.extend_from_slice(&bytes[spans[1].clone()]);
    reordered.extend_from_slice(&bytes[spans[0].clone()]);
    reordered.extend_from_slice(&bytes[spans[2].clone()]);
    reordered.extend_from_slice(&bytes[bytes.len() - 4..]);
    fix_crc(&mut reordered);
    assert!(SourceContractReplay::from_checkpoint(&reordered, expected).is_err());
    let mut duplicate = bytes[..spans[1].start].to_vec();
    duplicate.extend_from_slice(&bytes[spans[0].clone()]);
    duplicate.extend_from_slice(&bytes[spans[1].end..]);
    fix_crc(&mut duplicate);
    assert!(SourceContractReplay::from_checkpoint(&duplicate, expected).is_err());
    let mut edited = bytes.clone();
    let start = spans[0].start + 44;
    let command_len = u32_at(&bytes, spans[0].start + 40);
    let from = b"4024000000000000";
    let offset = edited[start..start + command_len]
        .windows(from.len())
        .position(|w| w == from)
        .unwrap()
        + start;
    edited[offset..offset + 16].copy_from_slice(b"403e000000000000");
    fix_crc(&mut edited);
    assert_eq!(
        SourceContractReplay::from_checkpoint(&edited, expected),
        Err(E::ReplayMismatch)
    );
    let mut receipt = bytes.clone();
    let state_at = spans[1].end - 96;
    receipt[state_at] ^= 1;
    fix_crc(&mut receipt);
    assert_eq!(
        SourceContractReplay::from_checkpoint(&receipt, expected),
        Err(E::ReplayMismatch)
    );
    let mut oversized = bytes.clone();
    oversized[37..41].copy_from_slice(&((MAX_COMMANDS + 1) as u32).to_be_bytes());
    fix_crc(&mut oversized);
    assert_eq!(
        SourceContractReplay::from_checkpoint(&oversized, expected),
        Err(E::Capacity)
    );
    let mut frame = bytes.clone();
    frame[spans[0].start + 40..spans[0].start + 44]
        .copy_from_slice(&((MAX_FRAME_BYTES + 1) as u32).to_be_bytes());
    fix_crc(&mut frame);
    assert_eq!(
        SourceContractReplay::from_checkpoint(&frame, expected),
        Err(E::Capacity)
    );
}
#[test]
fn local_rejection_is_rolled_back_but_attempt_is_replayed_and_bounded() {
    let mut replay = SourceContractReplay::new().unwrap();
    let before = replay.runtime().state_value().unwrap();
    let initial = replay.head();
    let receipt = replay
        .apply(C::Invoke(DependencyCall {
            target: "Unowned.actualMissing".into(),
            arguments: vec![],
        }))
        .unwrap();
    assert_eq!(
        receipt.outcome,
        O::LocalError(SourceRuleError::UnknownDependency)
    );
    assert!(receipt.effects.is_empty());
    assert_eq!(replay.runtime().state_value().unwrap(), before);
    assert_ne!(initial, replay.head());
    let bad = replay.apply(C::SetEntry("invented-entry".into())).unwrap();
    assert_eq!(bad.outcome, O::LocalError(SourceRuleError::InvalidInput));
    let head = replay.head();
    let bytes = replay.checkpoint().unwrap();
    assert_eq!(
        SourceContractReplay::from_checkpoint(&bytes, head).unwrap(),
        replay
    );
    let before = replay.clone();
    assert!(replay
        .apply(C::SetHeight(s(&"x".repeat(MAX_FRAME_BYTES + 1))))
        .is_err());
    assert_eq!(replay, before);
    // Identical source commands are valid separate journal occurrences. The
    // journal's sequence/prior-head identity, rather than value equality,
    // distinguishes them and rejects duplicated encoded entries.
    replay.apply(C::SetHeight(n(10.0))).unwrap();
    replay.apply(C::SetHeight(n(10.0))).unwrap();
    assert_eq!(replay.head().commands, 4);
    let empty = SourceContractReplay::new().unwrap();
    let bytes = empty.checkpoint().unwrap();
    for end in 0..bytes.len() {
        assert!(SourceContractReplay::from_checkpoint(&bytes[..end], empty.head()).is_err())
    }
    for i in 0..bytes.len() {
        let mut corrupt = bytes.clone();
        corrupt[i] ^= 1;
        assert!(SourceContractReplay::from_checkpoint(&corrupt, empty.head()).is_err())
    }
}
