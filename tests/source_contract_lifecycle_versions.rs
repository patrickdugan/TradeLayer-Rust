//! Frozen pre-lifecycle bytes bind the old method graph independently of V3.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    SourceContractReplayHead,
};
use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy as P;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
};
const LEGACY: &[u8] = include_bytes!("../fixtures/source_contract_replay_v2.bin");
fn s(v: &str) -> V {
    V::String(v.into())
}
fn n(v: f64) -> V {
    V::number(v)
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn invoke(target: &str, arguments: Vec<V>) -> C {
    C::Invoke(DependencyCall {
        target: target.into(),
        arguments,
    })
}
fn legacy_head() -> SourceContractReplayHead {
    let hex = "7f7338c21e5036f7b1fc1e2808cf738bd7f8fd69c8427ff9ef5f40113b202b9a";
    let mut commitment = [0; 32];
    for (i, b) in commitment.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    SourceContractReplayHead {
        commands: 6,
        commitment,
    }
}
fn repair_crc(bytes: &mut Vec<u8>) {
    bytes.truncate(bytes.len() - 4);
    let mut crc = !0u32;
    for b in bytes.iter() {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes.extend_from_slice(&(!crc).to_be_bytes());
}
fn raw16(payload: &str) -> Vec<V> {
    vec![
        s("definition-version-test"),
        n(16.),
        s("tl"),
        s(payload),
        s("1BoatSLRHtKNngkdXEeobR76b53LETtpyT"),
        V::Null,
        n(0.),
        n(0.),
        n(10.),
    ]
}
#[test]
fn original_compiled_v2_restores_exactly_and_rejects_lifecycle_capabilities() {
    let mut replay = SourceContractReplay::from_checkpoint(LEGACY, legacy_head()).unwrap();
    assert_eq!(replay.runtime().policy(), P::GlobalClearingV2);
    assert_eq!(replay.checkpoint().unwrap(), LEGACY);
    assert_eq!(replay.receipts().count(), 6);
    let recorded: Vec<_> = replay.receipts().map(|v| v.outcome.clone()).collect();
    assert_eq!(
        recorded[2],
        O::LocalError(SourceRuleError::UnknownDependency)
    );
    assert_eq!(
        recorded[3],
        O::LocalError(SourceRuleError::UnknownDependency)
    );
    assert_eq!(
        recorded[4],
        O::LocalError(SourceRuleError::UnknownDependency)
    );
    assert_eq!(recorded[5], O::LocalError(SourceRuleError::UnknownMethod));
    assert_eq!(
        replay
            .runtime()
            .state_value()
            .unwrap()
            .get("registryLifecycle"),
        V::Undefined
    );
    for target in [
        "Contract.createContractSeries",
        "Contract.getNextIdFromMap",
        "Contract.generateContracts",
        "Contract.saveDataToDb",
        "Contract.isDuplicateNativeContract",
        "Contract.getContractSeries",
        "Contract.getNextId",
        "Contract.getAllContractsForCollateral",
        "Contract.lookupInverseNativeByNotionalPid",
        "Contract.getContractType",
        "Contract.getAMM",
        "Contract.isValidSeriesId",
        "Contract.saveAllData",
        "Contract.getInstance",
        "Logic.createContractSeries",
        "Property.getPropertyData",
        "Volume.getLastPrice",
        "VolumeIndex.getLastPrice",
    ] {
        let before = replay.runtime().clone();
        let receipt = replay.apply(invoke(target, vec![])).unwrap();
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::UnknownDependency),
            "{target}"
        );
        assert!(receipt.effects.is_empty());
        assert_eq!(replay.runtime(), &before);
    }
    let receipt = replay
        .apply(C::ExecuteContract(raw16("1,0,1:7,1,2,7,10,o,3,0,0")))
        .unwrap();
    assert_eq!(
        receipt.outcome,
        O::LocalError(SourceRuleError::InvalidInput)
    );
    assert!(receipt.effects.is_empty());
    let next = replay.checkpoint().unwrap();
    assert_eq!(&next[..5], b"TLCR\x02");
    assert_eq!(
        SourceContractReplay::from_checkpoint(&next, replay.head()).unwrap(),
        replay
    );
    let default = SourceContractReplay::new().unwrap();
    assert_eq!(default.runtime().policy(), P::GlobalClearingV2);
    assert_eq!(&default.checkpoint().unwrap()[..5], b"TLCR\x02");
}
#[test]
fn corrected_crc_cannot_promote_frozen_v2_history() {
    let mut promoted = LEGACY.to_vec();
    promoted[4] = 3;
    repair_crc(&mut promoted);
    assert!(SourceContractReplay::from_checkpoint(&promoted, legacy_head()).is_err());
    let v3 = SourceContractReplay::with_policy(P::SourceLifecycleV3).unwrap();
    let bootstrap = v3.checkpoint().unwrap();
    promoted[5..37].copy_from_slice(&bootstrap[5..37]);
    repair_crc(&mut promoted);
    assert!(SourceContractReplay::from_checkpoint(&promoted, legacy_head()).is_err());
    assert_ne!(
        SourceContractReplay::with_policy(P::GlobalClearingV2)
            .unwrap()
            .head(),
        v3.head()
    );
}
#[test]
fn v3_actual_registry_journal_records_source_results_without_local_errors() {
    let address = "1BoatSLRHtKNngkdXEeobR76b53LETtpyT";
    let mut replay = SourceContractReplay::with_policy(P::SourceLifecycleV3).unwrap();
    let properties = obj(&[
        ("_id", s("propertyIndex")),
        (
            "value",
            s("[[1,{\"ticker\":\"P1\",\"type\":1}],[7,{\"ticker\":\"P7\",\"type\":1}]]"),
        ),
    ]);
    let oracle = obj(&[
        ("_id", s("oracle-1")),
        ("ticker", s("FLAT")),
        ("adminAddress", s(address)),
    ]);
    let commands = vec![
        C::Import {
            collection: "propertyList".into(),
            rows: vec![properties],
        },
        C::Import {
            collection: "oracleList".into(),
            rows: vec![oracle],
        },
        invoke(
            "Logic.createContractSeries",
            vec![
                s(address),
                V::Bool(true),
                n(0.),
                V::Array(vec![V::Array(vec![n(1.), n(7.)])]),
                n(1.),
                n(2.),
                n(7.),
                n(10.),
                n(24.),
                n(3.),
                V::Bool(false),
                V::Bool(false),
                n(10.),
            ],
        ),
        invoke("Contract.getContractSeries", vec![n(1.)]),
        invoke(
            "Contract.isDuplicateNativeContract",
            vec![n(7.), V::Array(vec![n(1.), n(7.)]), n(1.)],
        ),
        invoke("Contract.getNextId", vec![]),
        invoke("Property.getPropertyData", vec![n(7.)]),
        invoke("Volume.getLastPrice", vec![V::Undefined, n(10.)]),
        C::ExecuteContract(raw16("1,0,1:7,1,2,7,10,o,3,0,0")),
        C::ExecuteContract(raw16("0,1,,0,2,7,10,o,3,0,0")),
    ];
    let mut source_exceptions = 0;
    for command in commands {
        let receipt = replay.apply(command).unwrap();
        assert!(
            !matches!(receipt.outcome, O::LocalError(_)),
            "{:?}",
            receipt.outcome
        );
        if let O::Source(SourceOutcome::Thrown(error)) = &receipt.outcome {
            source_exceptions += 1;
            assert_eq!(error.name, "TypeError");
            assert_eq!(
                error.message,
                "Cannot read properties of undefined (reading 'ticker')"
            );
        }
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(&bytes[..5], b"TLCR\x03");
        let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
        assert_eq!(restored, replay);
        assert_eq!(restored.checkpoint().unwrap(), bytes);
        replay = restored;
    }
    assert_eq!(source_exceptions, 1);
    assert_eq!(replay.runtime().collection("contractList").len(), 1);
    let receipts: Vec<_> = replay.receipts().collect();
    assert_eq!(
        receipts[2].outcome,
        O::Dependency(DependencyReturn::Pending(n(1.)))
    );
    assert_eq!(
        receipts[4].outcome,
        O::Dependency(DependencyReturn::Pending(V::Bool(true)))
    );
    let O::Source(SourceOutcome::Returned(native)) = &receipts[8].outcome else {
        panic!("native decoder result")
    };
    assert_eq!(native.get("params").get("valid"), V::Bool(false));
    assert_eq!(native.get("params").get("onChainData"), V::Array(vec![]));
    let mut downgraded = replay.checkpoint().unwrap();
    downgraded[4] = 2;
    repair_crc(&mut downgraded);
    assert!(SourceContractReplay::from_checkpoint(&downgraded, replay.head()).is_err());
}

#[test]
fn v3_uses_javascript_id_whitespace_and_string_witnesses_cannot_be_pubkeys() {
    // Independently observed against the unchanged pinned Contract class:
    // parseInt("\u00851",10) is NaN; FEFF and NBSP are JavaScript whitespace.
    // V2 deliberately retains its historical Rust trim_start behavior.
    let data = obj(&[("id", n(1.)), ("ticker", s("TRIM"))]);
    let row = obj(&[
        ("_id", s("contract-1")),
        ("id", n(1.)),
        ("type", s("contractSeries")),
        ("data", data.clone()),
    ]);
    for policy in [P::GlobalClearingV2, P::SourceLifecycleV3] {
        let mut replay = SourceContractReplay::with_policy(policy).unwrap();
        replay
            .apply(C::Import {
                collection: "contractList".into(),
                rows: vec![row.clone()],
            })
            .unwrap();
        for (input, found) in if policy == P::SourceLifecycleV3 {
            [
                ("\u{0085}1", false),
                ("\u{feff}1", true),
                ("\u{00a0}1", true),
            ]
        } else {
            [
                ("\u{0085}1", true),
                ("\u{feff}1", false),
                ("\u{00a0}1", true),
            ]
        } {
            let receipt = replay
                .apply(invoke("Contract.getContractInfo", vec![s(input)]))
                .unwrap();
            assert_eq!(
                receipt.outcome,
                O::Dependency(DependencyReturn::Pending(if found {
                    data.clone()
                } else {
                    V::Null
                })),
                "{policy:?} {input:?}"
            );
            let read = receipt
                .effects
                .iter()
                .find(|v| v.get("method") == s("findOneAsync"))
                .unwrap();
            let V::Array(arguments) = read.get("args") else {
                panic!("original database arguments")
            };
            assert_eq!(
                arguments[0].get("id"),
                n(if found { 1. } else { f64::NAN }),
                "{policy:?} exact parseInt query"
            );
        }
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(
            SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap(),
            replay
        );
    }
    let mut v3 = SourceContractReplay::with_policy(P::SourceLifecycleV3).unwrap();
    // The unchanged async TxUtils extractor indexes one UTF16 unit of a
    // string. It cannot match either 66- or 130-character ASCII-hex regex.
    for witness in ["", "a", "ab", "😀", &"a".repeat(130)] {
        let receipt = v3
            .apply(invoke(
                "TxUtils.extractPubkeyByType",
                vec![obj(&[("txinwitness", s(witness))]), s("p2wpkh")],
            ))
            .unwrap();
        assert_eq!(
            receipt.outcome,
            O::Dependency(DependencyReturn::Pending(V::Null)),
            "string witness {witness:?}"
        );
    }
}

#[test]
fn v3_king_settlement_writes_literal_fractional_and_dotted_property_keys() {
    // Independently executed against unchanged pinned Logic and Channels with
    // these persisted channel/scaling documents: both transfer directions use
    // bracket keys, including numeric 7.5 and the string "a.b".
    let mut replay = SourceContractReplay::with_policy(P::SourceLifecycleV3).unwrap();
    let column = |seven, fractional, dotted| {
        obj(&[("7", n(seven)), ("7.5", n(fractional)), ("a.b", n(dotted))])
    };
    let channel = obj(&[
        ("channel", s("c")),
        (
            "participants",
            obj(&[
                ("A", s("1BoatSLRHtKNngkdXEeobR76b53LETtpyT")),
                ("B", s("1BitcoinEaterAddressDontSendf59kuE")),
            ]),
        ),
        ("A", column(99., 10., 20.)),
        ("B", column(88., 3., 4.)),
        ("commits", V::Array(vec![])),
        ("clearLists", V::Array(vec![])),
    ]);
    let initial_trade = obj(&[
        ("txid", s("old")),
        ("block", n(2.)),
        ("status", s("pending")),
    ]);
    for (collection, row) in [
        (
            "channels",
            obj(&[("_id", s("c")), ("data", channel.clone())]),
        ),
        (
            "scaling",
            obj(&[
                ("_id", s("c")),
                ("trades", V::Array(vec![initial_trade])),
                ("kingSettlements", V::Array(vec![])),
            ]),
        ),
    ] {
        replay
            .apply(C::Import {
                collection: collection.into(),
                rows: vec![row],
            })
            .unwrap();
    }
    assert_eq!(
        replay.runtime().collection("channels")[0].get("data"),
        channel
    );
    let mut settlements = vec![];
    for (property, direction, txid, expected_a, expected_b) in [
        (
            n(7.5),
            true,
            "fractional",
            column(99., 8., 20.),
            column(88., 5., 4.),
        ),
        (
            s("a.b"),
            false,
            "dotted",
            column(99., 8., 22.),
            column(88., 5., 2.),
        ),
    ] {
        let receipt = replay
            .apply(invoke(
                "Logic.batchSettlement",
                vec![obj(&[
                    ("senderAddress", s("c")),
                    ("blockStart", n(1.)),
                    ("blockEnd", n(9.)),
                    ("propertyId", property.clone()),
                    ("netAmount", n(2.)),
                    ("aPaysBDirection", V::Bool(direction)),
                    ("channelRoot", s("root")),
                    ("totalContracts", n(2.)),
                    ("neutralizedCount", n(0.)),
                    ("block", n(10.)),
                    ("txid", s(txid)),
                ])],
            ))
            .unwrap();
        assert_eq!(
            receipt.outcome,
            O::Dependency(DependencyReturn::Pending(V::Undefined))
        );
        let persisted = replay.runtime().collection("channels")[0].get("data");
        // Exact columns also reject extra nested "a" paths and replacement of
        // the unrelated scalar property "7" with a nested "5" object.
        assert_eq!(persisted.get("A"), expected_a);
        assert_eq!(persisted.get("B"), expected_b);
        settlements.push(obj(&[
            ("txid", s(txid)),
            ("blockStart", n(1.)),
            ("blockEnd", n(9.)),
            ("propertyId", property),
            ("netAmount", n(2.)),
            ("aPaysBDirection", V::Bool(direction)),
            ("channelRoot", s("root")),
            ("totalContracts", n(2.)),
            ("neutralizedCount", n(0.)),
            ("settledAtBlock", n(10.)),
            ("timestamp", n(1_700_000_000_000.)),
        ]));
        assert_eq!(
            replay.runtime().collection("scaling"),
            &[obj(&[
                ("_id", s("c")),
                (
                    "trades",
                    V::Array(vec![obj(&[
                        ("txid", s("old")),
                        ("block", n(2.)),
                        ("status", s("swept")),
                        ("sweptBy", s("fractional")),
                    ])]),
                ),
                ("kingSettlements", V::Array(settlements.clone())),
            ])]
        );
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(
            SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap(),
            replay
        );
    }

    // Run unchanged full modules over the same recording DB, independently
    // with both genuine BigNumber versions. Only the dependency constructor is
    // substituted; no source method or financial result is supplied.
    const COERCION_REFERENCE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),crypto=require('node:crypto'),vm=require('node:vm');
const root=process.argv[1],version=process.argv[2],numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));
if(version==='9.1.2'){
 const archive=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8'),packed=archive.match(/const BIGNUMBER_GZIP="([^"]+)"/)[1];
 const source=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));
 if(crypto.createHash('sha256').update(source).digest('hex')!=='20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e')throw Error('genuine9.1.2 hash');
 const loaded={exports:{}};vm.runInNewContext(source.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});numeric.loadBigNumber=()=>loaded.exports;
}else if(version!=='9.3.1')throw Error('test version');
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');
const end=code.indexOf('const scenes=[');if(end<0)throw Error('source environment boundary changed');code=code.slice(0,end);
if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('source environment inventory changed');
code=code.replace('return {types,logic,Orderbook,effects','return {tables,types,logic,Orderbook,effects');code+='\nmodule.exports={environment,tag};';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const {environment,tag}=driver.exports;
(async()=>{for(const [name,credit]of [['array',[]],['object',{}],['nested-own-toString',[{toString:null}]]]){
 const e=environment({entry:'main-subgraph'});e.load('channels');
 const channels=[{_id:'c',data:{channel:'c',participants:{A:'A',B:'B'},A:{7:10},B:{7:credit},commits:[],clearLists:[]}}];
 const scaling=[{_id:'c',kingSettlements:[]}];
 e.table('channels').splice(0,Infinity,...structuredClone(channels));e.table('scaling').splice(0,Infinity,...structuredClone(scaling));
 for(let i=0;i<6;i++)await new Promise(resolve=>setImmediate(resolve));e.effects.length=0;
 const params={senderAddress:'c',blockStart:1,blockEnd:9,propertyId:7,netAmount:2,aPaysBDirection:true,channelRoot:'root',totalContracts:2,neutralizedCount:0,block:10,txid:name};
 let outcome;try{outcome={transport:'Pending',returned:await e.logic.batchSettlement(params)};}catch(error){outcome={transport:'PendingThrow',thrown:{name:error.name,message:error.message}};}
 console.log(JSON.stringify(tag({name,before:{channels,scaling},params,outcome,stored:e.table('channels'),scaling:e.table('scaling'),cache:[...e.load('channels').channelsRegistry],effects:e.effects.filter(v=>['database','db','uuid','random','rpc-clock'].includes(v.kind))})));
}})().catch(error=>{console.error(error);process.exitCode=1;});
"###;
    let source_cases = |version: &str| {
        let output = std::process::Command::new("node")
            .args([
                "-e",
                COERCION_REFERENCE,
                env!("CARGO_MANIFEST_DIR"),
                version,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::str::from_utf8(&output.stdout)
            .unwrap()
            .lines()
            .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
            .collect::<Vec<_>>()
    };
    let archived = source_cases("9.3.1");
    assert_eq!(archived, source_cases("9.1.2"));
    assert_eq!(archived.len(), 3);
    for case in archived {
        let mut replay = SourceContractReplay::with_policy(P::SourceLifecycleV3).unwrap();
        for collection in ["channels", "scaling"] {
            let V::Array(rows) = case.get("before").get(collection) else {
                panic!("source seed rows")
            };
            replay
                .apply(C::Import {
                    collection: collection.into(),
                    rows,
                })
                .unwrap();
        }
        let receipt = replay
            .apply(invoke("Logic.batchSettlement", vec![case.get("params")]))
            .unwrap();
        let actual = match receipt.outcome {
            O::Dependency(DependencyReturn::Pending(value)) => {
                obj(&[("transport", s("Pending")), ("returned", value)])
            }
            O::Dependency(DependencyReturn::PendingThrow(error)) => obj(&[
                ("transport", s("PendingThrow")),
                (
                    "thrown",
                    obj(&[("name", s(&error.name)), ("message", s(&error.message))]),
                ),
            ]),
            other => panic!("unexpected King transport {other:?}"),
        };
        assert_eq!(actual, case.get("outcome"));
        assert_eq!(
            V::Array(replay.runtime().collection("channels").to_vec()),
            case.get("stored")
        );
        assert_eq!(
            V::Array(replay.runtime().collection("scaling").to_vec()),
            case.get("scaling")
        );
        let cache = replay
            .runtime()
            .state_value()
            .unwrap()
            .get("globalClearing")
            .get("channels")
            .get("registry");
        assert_eq!(cache, case.get("cache"));
        let effects = receipt
            .effects
            .into_iter()
            .filter(|v| {
                matches!(v.get("kind"), V::String(k) if ["database", "db", "uuid", "random", "rpc-clock"].contains(&k.as_str()))
            })
            .collect();
        assert_eq!(V::Array(effects), case.get("effects"));
        let V::Array(registry) = cache else {
            panic!("actual channel registry")
        };
        let V::Array(entry) = &registry[0] else {
            panic!("actual channel cache entry")
        };
        let cached = &entry[1];
        let stored = replay.runtime().collection("channels")[0].get("data");
        assert_eq!(cached.get("A").get("7"), n(8.));
        if case.get("name") == s("nested-own-toString") {
            assert_eq!(stored.get("A").get("7"), n(10.));
            assert_eq!(
                actual.get("thrown"),
                obj(&[
                    ("name", s("TypeError")),
                    ("message", s("Cannot convert object to primitive value")),
                ])
            );
            assert_eq!(
                stored.get("B").get("7"),
                V::Array(vec![obj(&[("toString", V::Null)])])
            );
        } else {
            assert_eq!(stored.get("A").get("7"), n(8.));
            assert_eq!(
                stored.get("B").get("7"),
                if case.get("name") == s("array") {
                    s("2")
                } else {
                    s("[object Object]2")
                }
            );
        }
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(
            SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap(),
            replay
        );
    }
}
