//! TLSW3 conditional witnesses over the actual V7/V8 replay owner.
use tradelayer_core::source_contract_replay::{
    source_recent_lifecycle_entry_profile as entry,
    source_recent_lifecycle_state_commitment as state,
    source_recent_lifecycle_transition_commands_commitment as digest, SourceContractCommand as C,
    SourceContractCommandOutcome as O, SourceContractReceipt, SourceContractReplay as R,
    SourceContractReplayError, SourceRecentLifecycleActivationProfile as A,
    SourceRecentLifecycleEntryProfile as Entry,
    SourceRecentLifecycleTransitionDescriptor as Descriptor,
    SourceRecentLifecycleTransitionError as E,
    SourceRecentLifecycleTransitionExpectation as Expected,
    SourceRecentLifecycleTransitionScope as Scope, SourceRecentLifecycleTransitionWitness as W,
};
use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy as P;
use tradelayer_core::validity_source::{DependencyCall, SourceValue as V};
fn n(v: f64) -> V {
    V::number(v)
}
fn s(v: &str) -> V {
    V::String(v.into())
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn invoke(target: &str, arguments: Vec<V>) -> C {
    C::Invoke(DependencyCall {
        target: target.into(),
        arguments,
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
fn repair_envelope(bytes: &mut [u8]) {
    let prefix = bytes.len() - 36;
    let hex = bytes[..prefix]
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    let output=std::process::Command::new("node").args(["-e","process.stdout.write(require('node:crypto').createHash('sha256').update('TradeLayer/SourceRecentLifecycleTransitionWitness/v3/').update(Buffer.from(process.argv[1],'hex')).digest('hex'))",&hex]).output().unwrap();
    assert!(output.status.success());
    let hash = std::str::from_utf8(&output.stdout).unwrap();
    for i in 0..32 {
        bytes[prefix + i] = u8::from_str_radix(&hash[i * 2..i * 2 + 2], 16).unwrap();
    }
    let crc_at = bytes.len() - 4;
    let mut crc = !0u32;
    for b in &bytes[..crc_at] {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes[crc_at..].copy_from_slice(&(!crc).to_be_bytes());
}
struct Pins {
    policy: P,
    scope: Scope,
    descriptor: Descriptor,
    pre: R,
    post: R,
    pre_entry: Entry,
    post_entry: Entry,
    commands: [u8; 32],
}
impl Pins {
    fn new(before: &R, commands: &[C], scope: Scope) -> Self {
        let mut post = before.clone();
        for command in commands {
            post.apply(command.clone()).unwrap();
        }
        Self {
            policy: before.runtime().policy(),
            scope,
            descriptor: Descriptor::reference(),
            pre: before.clone(),
            pre_entry: entry(before).unwrap(),
            post_entry: entry(&post).unwrap(),
            post,
            commands: digest(before.runtime().policy(), scope, commands).unwrap(),
        }
    }
    fn expected(&self) -> Expected<'_> {
        Expected {
            policy: self.policy,
            scope: self.scope,
            descriptor: &self.descriptor,
            pre_head: self.pre.head(),
            pre_state_commitment: state(&self.pre).unwrap(),
            pre_entry: &self.pre_entry,
            commands_commitment: self.commands,
            post_head: self.post.head(),
            post_state_commitment: state(&self.post).unwrap(),
            post_entry: &self.post_entry,
        }
    }
    fn prove(&self, commands: &[C]) -> W {
        let before = self.pre.clone();
        let witness = W::produce(&self.pre, commands, &self.expected()).unwrap();
        assert_eq!(self.pre, before, "producer cannot mutate input owner");
        let bytes = witness.canonical_bytes().unwrap();
        assert_eq!(&bytes[..5], b"TLSW\x03");
        let verified = W::verify_bytes(&bytes, &self.expected()).unwrap();
        assert_eq!(verified.replay(), &self.post, "exact reconstructed owner");
        assert_eq!(verified.receipts().count(), commands.len());
        assert_eq!(verified, witness.verify(&self.expected()).unwrap());
        witness
    }
}
#[test]
fn command_digest_matches_independent_node_buffer_frames() {
    // Independently assembled with literal tagged SetHeight diagnostic frames,
    // Node Buffer big-endian lengths and crypto SHA256. No witness bytes or Rust
    // encoder output were used by scripts/recent_lifecycle_witness_command_pins.cjs.
    let commands = [C::SetHeight(n(-0.)), C::SetHeight(n(10.))];
    for (policy, scope, pin) in [
        (
            P::LowerLifecycleV7,
            Scope::RawTransactionSlice,
            "810dedb415d2509280ddac1b006227046484f8b310ab0b9e02b9b286af8a3ba1",
        ),
        (
            P::LowerLifecycleV7,
            Scope::SourceMethodSlice,
            "9f57c4f0197f748afe9ce78f4f044a21955589880f7ef6c425ffc48e32307f42",
        ),
        (
            P::MainCallableV8,
            Scope::RawTransactionSlice,
            "e5e56e08f3b711dbd88499cee12bae02d5d941880ee6b9b085611f9914978a32",
        ),
        (
            P::MainCallableV8,
            Scope::SourceMethodSlice,
            "a25c1505559cb48b695d6f3e69784fc2a5559d8d0f64b827ae4199d467772000",
        ),
        (
            P::MainCallableV8,
            Scope::MainCallableSlice,
            "da9239d32ffd22fdda6d6ff297278785d9cbbc042ef0a2ff64a25deb3196893b",
        ),
    ] {
        assert_eq!(
            digest(policy, scope, &commands)
                .unwrap()
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>(),
            pin
        );
    }
}
#[test]
fn tlsw3_policy_scope_and_admission_are_separate_from_old_profiles() {
    use tradelayer_core::source_contract_replay::{
        source_lifecycle_transition_commands_commitment as old_digest,
        SourceLifecycleTransitionError as OldError, SourceLifecycleTransitionScope as OldScope,
    };
    for policy in [P::LowerLifecycleV7, P::MainCallableV8] {
        assert_eq!(
            old_digest(policy, OldScope::RawTransactionSlice, &[raw(18.)]),
            Err(OldError::WrongPolicy)
        );
        let before = R::with_policy(policy).unwrap();
        for kind in [
            1., 8., 11., 12., 13., 14., 15., 16., 17., 18., 19., 20., 21., 22., 23., 24., 25., 26.,
            27., 28., 29., 30., 31., 32., 33., 34., 35.,
        ] {
            assert!(digest(policy, Scope::RawTransactionSlice, &[raw(kind)]).is_ok());
        }
        for kind in [0., 5., 6., 10., 36.] {
            assert_eq!(
                digest(policy, Scope::RawTransactionSlice, &[raw(kind)]),
                Err(E::UnsupportedCommand)
            );
        }
        Pins::new(
            &before,
            &[C::SetHeight(n(-0.)), C::SetHeight(n(10.))],
            Scope::RawTransactionSlice,
        )
        .prove(&[C::SetHeight(n(-0.)), C::SetHeight(n(10.))]);
        for command in [
            C::SetActivation(true),
            C::SetEntry("types-first".into()),
            C::Import {
                collection: "tallyMap".into(),
                rows: vec![],
            },
            invoke("Tally.updateBalance", vec![]),
            invoke("SourceMain.configure", vec![]),
            invoke("SourceGlobal.bindRpcTransactions", vec![]),
            invoke("Main.constructConsensusFromIndex", vec![]),
        ] {
            assert_eq!(
                digest(policy, Scope::SourceMethodSlice, &[command]),
                Err(E::UnsupportedCommand)
            );
        }
    }
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
        P::ProofLifecycleV6,
        P::SpotLifecycleV9,
    ] {
        assert_eq!(
            digest(policy, Scope::RawTransactionSlice, &[]),
            Err(E::WrongPolicy)
        );
    }
    assert_eq!(
        digest(P::LowerLifecycleV7, Scope::MainCallableSlice, &[]),
        Err(E::WrongScope)
    );
    for kind in [2., 3., 4., 7., 9.] {
        assert_eq!(
            digest(
                P::LowerLifecycleV7,
                Scope::RawTransactionSlice,
                &[raw(kind)]
            ),
            Err(E::UnsupportedCommand)
        );
        assert!(digest(P::MainCallableV8, Scope::RawTransactionSlice, &[raw(kind)]).is_ok());
    }
}
#[test]
fn mandatory_independent_heads_states_commands_domains_and_profiles_are_checked() {
    let before = R::with_policy(P::MainCallableV8).unwrap();
    let commands = [invoke("Main.loadTrackHeight", vec![])];
    let pins = Pins::new(&before, &commands, Scope::MainCallableSlice);
    let witness = pins.prove(&commands);
    let bytes = witness.canonical_bytes().unwrap();
    let mut expected = pins.expected();
    expected.pre_head.commitment[0] ^= 1;
    assert_eq!(W::verify_bytes(&bytes, &expected), Err(E::WrongPreHead));
    let mut expected = pins.expected();
    expected.pre_state_commitment[0] ^= 1;
    assert_eq!(W::verify_bytes(&bytes, &expected), Err(E::WrongPreState));
    let mut expected = pins.expected();
    expected.post_head.commitment[0] ^= 1;
    assert_eq!(W::verify_bytes(&bytes, &expected), Err(E::WrongPostHead));
    let mut expected = pins.expected();
    expected.post_state_commitment[0] ^= 1;
    assert_eq!(W::verify_bytes(&bytes, &expected), Err(E::WrongPostState));
    let mut expected = pins.expected();
    expected.commands_commitment[0] ^= 1;
    assert_eq!(W::verify_bytes(&bytes, &expected), Err(E::WrongCommands));
    let mut descriptor = Descriptor::reference();
    descriptor.numeric_domain.push('x');
    let mut expected = pins.expected();
    expected.descriptor = &descriptor;
    assert_eq!(W::verify_bytes(&bytes, &expected), Err(E::WrongDescriptor));
    descriptor = Descriptor::reference();
    descriptor.dependency_domain.push('x');
    let mut expected = pins.expected();
    expected.descriptor = &descriptor;
    assert_eq!(W::verify_bytes(&bytes, &expected), Err(E::WrongDescriptor));
    let profile = Entry::Main {
        module_entry: "types-first".into(),
        activation_profile: A::SourceRegistry,
        activation_locked: false,
    };
    let mut expected = pins.expected();
    expected.pre_entry = &profile;
    assert_eq!(
        W::verify_bytes(&bytes, &expected),
        Err(E::WrongEntryProfile)
    );
    let profile = Entry::Main {
        module_entry: "main-subgraph".into(),
        activation_profile: A::SourceRegistry,
        activation_locked: false,
    };
    let mut expected = pins.expected();
    expected.post_entry = &profile;
    assert_eq!(
        W::verify_bytes(&bytes, &expected),
        Err(E::WrongEntryProfile)
    );
    for length in [0, 4, 8, bytes.len() - 1] {
        assert!(W::verify_bytes(&bytes[..length], &pins.expected()).is_err());
    }
    let mut changed = bytes.clone();
    changed[6] = 2;
    assert!(W::verify_bytes(&changed, &pins.expected()).is_err());
    let mut extended = bytes.clone();
    extended.push(0);
    assert!(W::verify_bytes(&extended, &pins.expected()).is_err());
    let mut forged = bytes.clone();
    let at = forged.len() - 68;
    forged[at] ^= 1;
    repair_envelope(&mut forged);
    assert_eq!(
        W::verify_bytes(&forged, &pins.expected()),
        Err(E::WrongPostState),
        "repaired self hashes cannot replace an independent state pin"
    );
    let mut forged = bytes.clone();
    forged[8] = 7;
    repair_envelope(&mut forged);
    assert_eq!(
        W::verify_bytes(&forged, &pins.expected()),
        Err(E::WrongPolicy),
        "repaired self hashes cannot promote another policy"
    );
    assert_ne!(
        digest(
            P::MainCallableV8,
            Scope::RawTransactionSlice,
            &[C::SetHeight(n(-0.))]
        )
        .unwrap(),
        digest(
            P::MainCallableV8,
            Scope::RawTransactionSlice,
            &[C::SetHeight(n(0.))]
        )
        .unwrap()
    );
    assert_ne!(
        digest(
            P::MainCallableV8,
            Scope::RawTransactionSlice,
            &[C::SetHeight(n(1.)), C::SetHeight(n(2.))]
        )
        .unwrap(),
        digest(
            P::MainCallableV8,
            Scope::RawTransactionSlice,
            &[C::SetHeight(n(2.)), C::SetHeight(n(1.))]
        )
        .unwrap()
    );
}
#[test]
fn aggregate_command_bytes_and_count_are_bounded_before_replay() {
    assert_eq!(
        digest(
            P::MainCallableV8,
            Scope::RawTransactionSlice,
            &vec![C::SetHeight(n(1.)); 4097]
        ),
        Err(E::Capacity)
    );
    let commands = vec![C::SetHeight(s(&"x".repeat(32_700))); 1040];
    assert_eq!(
        digest(P::MainCallableV8, Scope::RawTransactionSlice, &commands),
        Err(E::Replay(SourceContractReplayError::Capacity))
    );
}
#[test]
fn tlsw1_tlsw2_frozen_bytes_apis_and_v8_refusals_remain_exact() {
    use tradelayer_core::source_contract_replay::{
        source_lifecycle_transition_commands_commitment, SourceLifecycleTransitionExpectation,
        SourceLifecycleTransitionScope, SourceLifecycleTransitionWitness,
        SourceTransitionDescriptor, SourceTransitionExpectation, SourceTransitionWitness,
    };
    let before = R::with_policy(P::DistributionLifecycleV5).unwrap();
    let commands = [C::SetHeight(n(-0.)), C::SetHeight(n(10.))];
    let mut after = before.clone();
    for c in &commands {
        after.apply(c.clone()).unwrap();
    }
    let descriptor = SourceTransitionDescriptor::reference();
    let old = SourceTransitionExpectation {
        policy: P::DistributionLifecycleV5,
        descriptor: &descriptor,
        pre_head: before.head(),
        commands_commitment: None,
        post_head: Some(after.head()),
    };
    let old_bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/source_transition_witness_v1_compat.bin"),
    )
    .unwrap();
    assert_eq!(
        SourceTransitionWitness::produce(&before, &commands, &old)
            .unwrap()
            .canonical_bytes()
            .unwrap(),
        old_bytes
    );
    assert_eq!(
        SourceTransitionWitness::verify_bytes(&old_bytes, &old)
            .unwrap()
            .replay(),
        &after
    );
    let v2 = SourceLifecycleTransitionExpectation {
        policy: P::DistributionLifecycleV5,
        scope: SourceLifecycleTransitionScope::RawTransactionSlice,
        descriptor: &descriptor,
        pre_head: before.head(),
        commands_commitment: source_lifecycle_transition_commands_commitment(
            P::DistributionLifecycleV5,
            SourceLifecycleTransitionScope::RawTransactionSlice,
            &commands,
        )
        .unwrap(),
        post_head: Some(after.head()),
    };
    let v2_bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/source_lifecycle_transition_witness_v2.bin"),
    )
    .unwrap();
    assert_eq!(
        SourceLifecycleTransitionWitness::produce(&before, &commands, &v2)
            .unwrap()
            .canonical_bytes()
            .unwrap(),
        v2_bytes
    );
    assert_eq!(
        SourceLifecycleTransitionWitness::verify_bytes(&v2_bytes, &v2)
            .unwrap()
            .replay(),
        &after
    );
    assert!(SourceTransitionWitness::verify_bytes(&v2_bytes, &old).is_err());
    assert!(SourceLifecycleTransitionWitness::verify_bytes(&old_bytes, &v2).is_err());
}
#[test]
fn declared_activation_is_explicit_and_main_never_accepts_it() {
    let mut before = R::with_policy(P::MainCallableV8).unwrap();
    before
        .apply(invoke(
            "SourceMain.configure",
            vec![obj(&[("activationProfile", s("declared-answer"))])],
        ))
        .unwrap();
    before.apply(C::SetActivation(false)).unwrap();
    let commands = [raw(18.)];
    let pins = Pins::new(&before, &commands, Scope::RawTransactionSlice);
    assert!(matches!(
        pins.pre_entry,
        Entry::Main {
            activation_profile: A::DeclaredAnswer,
            activation_locked: false,
            ..
        }
    ));
    assert!(matches!(
        pins.post_entry,
        Entry::Main {
            activation_profile: A::DeclaredAnswer,
            activation_locked: true,
            ..
        }
    ));
    pins.prove(&commands);
    let main = [invoke("Main.loadTrackHeight", vec![])];
    let pins = Pins::new(&before, &main, Scope::MainCallableSlice);
    assert_eq!(
        W::produce(&before, &main, &pins.expected()),
        Err(E::WrongEntryProfile)
    );
    let unavailable = R::with_policy(P::MainCallableV8).unwrap();
    // The exact unbound-capability/local receipt cannot be promoted to a
    // successful source slice merely because the replay journal records it.
    let row = obj(&[
        ("txId", s("unowned-spot")),
        ("payload", s("5")),
        (
            "sender",
            obj(&[("senderAddress", s("actor")), ("amount", n(0.))]),
        ),
        ("reference", V::Array(vec![])),
    ]);
    let commands = [invoke("Main.processTx", vec![V::Array(vec![row]), n(10.)])];
    let pins = Pins::new(&unavailable, &commands, Scope::MainCallableSlice);
    assert!(matches!(
        pins.post.receipts().last().unwrap().outcome,
        O::LocalError(_)
    ));
    assert_eq!(
        W::produce(&unavailable, &commands, &pins.expected()),
        Err(E::UnavailableExecution)
    );
}

// The original corpus tests below retain their source outcome/state/effect
// assertions. This wrapper substitutes verified TLSW3 replay for each admitted
// raw/Logic/Main call. Bootstrap/owner metadata commands remain pinned history.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckedReplay(R);
impl std::ops::Deref for CheckedReplay {
    type Target = R;
    fn deref(&self) -> &R {
        &self.0
    }
}
impl CheckedReplay {
    fn with_policy(policy: P) -> Result<Self, SourceContractReplayError> {
        R::with_policy(policy).map(Self)
    }
    fn from_checkpoint(
        bytes: &[u8],
        head: tradelayer_core::source_contract_replay::SourceContractReplayHead,
    ) -> Result<Self, SourceContractReplayError> {
        R::from_checkpoint(bytes, head).map(Self)
    }
    fn apply(&mut self, command: C) -> Result<SourceContractReceipt, SourceContractReplayError> {
        let scope = match &command {
            C::Invoke(call) if call.target.starts_with("Main.") => Scope::MainCallableSlice,
            C::Invoke(_) => Scope::SourceMethodSlice,
            _ => Scope::RawTransactionSlice,
        };
        if digest(
            self.runtime().policy(),
            scope,
            std::slice::from_ref(&command),
        )
        .is_err()
        {
            return self.0.apply(command);
        }
        let pins = Pins::new(&self.0, std::slice::from_ref(&command), scope);
        let receipt = pins.post.receipts().last().unwrap().clone();
        if matches!(receipt.outcome, O::LocalError(_)) {
            self.0 = pins.post;
            return Ok(receipt);
        }
        let witness = pins.prove(std::slice::from_ref(&command));
        self.0 = witness.verify(&pins.expected()).unwrap().into_replay();
        COVERAGE.with(|v| {
            v.borrow_mut()
                .push((self.runtime().policy(), scope, command))
        });
        Ok(receipt)
    }
}
thread_local! { static COVERAGE:std::cell::RefCell<Vec<(P,Scope,C)>>=const { std::cell::RefCell::new(Vec::new()) }; }

#[allow(dead_code)]
mod original_property {
    use super::CheckedReplay as Replay;
    // Unchanged pinned lower Property/Logic graph over the sole actual Core stores.
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O,
    };
    use tradelayer_core::source_contract_runtime::{
        SourceContractRuntimePolicy as P, StorageFailure,
    };
    use tradelayer_core::validity_source::{
        DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
    };
    fn n(v: f64) -> V {
        V::number(v)
    }
    fn s(v: &str) -> V {
        V::String(v.into())
    }
    fn obj(v: &[(&str, V)]) -> V {
        V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
    }
    fn arr(v: V) -> Vec<V> {
        if let V::Array(v) = v {
            v
        } else {
            panic!("array {v:?}")
        }
    }
    fn txt(v: V) -> String {
        if let V::String(v) = v {
            v
        } else {
            panic!("string {v:?}")
        }
    }
    fn invoke(target: &str, arguments: Vec<V>) -> C {
        C::Invoke(DependencyCall {
            target: target.into(),
            arguments,
        })
    }
    const REFERENCE: &str = r####"
'use strict';
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=process.env.PROPERTY_REPO||path.resolve(__dirname,'../../../outputs/TradeLayer-Rust'),flags=process.argv.slice(1),lock=flags.includes('--lock')||process.env.TL_SOURCE_BN_VERSION==='9.1.2',numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(lock){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],raw=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(raw).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const loaded={exports:{}};vm.runInNewContext(raw.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'driver anchor '+before);code=code.replace(before,after);}
replace('const sources = Object.fromEntries(',`pins.procedural=['legacy_procedural_source.txt','0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e'];\nconst sources = Object.fromEntries(`);
replace("'vaults','procedural','bitvmCache'","'vaults','bitvmCache'");
// Ordinary host console methods return undefined; the reused audit driver's
// logs.push return length must not become a fabricated addProperty result.
replace("(...values)=>logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))})","(...values)=>{logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))});}");
replace('return {types,logic,Orderbook,effects','return {context,storage,tables,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const A='LKKG9A9a8n7CeHK8Nk7RdNZiCuHZW2U789',B='LKQa26zKBtyM2kw6aMEYyj4fCbCqxJyrPw',records=[];let e,lastParams,unhandled=[];process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toString()];if(Object.prototype.toString.call(v)==='[object Error]')return tag({name:v.name,message:v.message});if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),P=e.load('property');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,propertyCache:P.instance?.propertyIndex??new Map(),propertyInitialized:!!P.instance,lastParams,unhandled,unsupported:e.state().unsupported};}
function emit(name,command,result){const snapshot=e.copy(state());assert.deepEqual(snapshot.unsupported,[],name+' unowned boundary');records.push({name,command,result,state:snapshot,effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','activation'].includes(v.kind))});}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:structuredClone(args)});return Reflect.apply(original,this,args);};}
async function reset(name,options={}){e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally',...options}:options.entry==='volume-first'?{preload:'volumeIndex',...options}:options);lastParams=undefined;unhandled=[];if(Object.hasOwn(options,'propertyRows'))e.table('propertyList').splice(0,Infinity,...structuredClone(options.propertyRows));for(const method of ['tokenIssue','grantManagedToken','redeemManagedToken'])observe(e.logic,method,'Logic.'+method);for(const method of ['getInstance','load','getPropertyData','getPropertyIndex','isManagedAndAdmin','doesTickerExist'])observe(e.load('property'),method,'Property.'+method);for(const method of ['createToken','getNextPropertyId','addProperty','save','grantTokens','redeemTokens'])observe(e.load('property').prototype,method,'Property.'+method);const P=e.load('procedural').ProceduralRegistry;for(const method of ['_base','getTemplate','getContract','ensureIssuanceContext','ensureRedemptionContext','_normalizeHash','upsertTemplate','upsertContract','transitionContract'])observe(P,method,'Procedural.'+method);await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function raw(name,type,payload,sender=A,reference=null,failure){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[name,type,'tl',payload,sender,reference,0,0,10];let outcome;try{lastParams=await e.types.decodePayload(...args);let returned;if(lastParams.valid===true)returned=await e.logic.typeSwitch(type,lastParams);outcome={params:lastParams,returned};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
async function call(name,target,args=[],failure,unsupported=false){e.effects.length=0;if(failure)fail(failure);let receiver;const [ns,method]=target.split('.');if(ns==='Logic')receiver=e.logic;else if(ns==='Procedural')receiver=e.load('procedural').ProceduralRegistry;else if(ns==='PropertyInstance')receiver=e.load('property').instance;else receiver=e.load('property');let transport='Immediate',outcome;try{const value=Reflect.apply(receiver[method],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target:target.replace('PropertyInstance.','Property.'),args,failure,unsupported},{transport,outcome});}
async function seed(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{outcome:{returned:undefined}});}
const amount=v=>(v*1e8).toString(36),issue=(ticker,managed=false,extra={})=>[1000..toString(36),ticker,extra.whitelists||'',+managed,B,+!!extra.nft,0,extra.proceduralType??''].join(',');
(async()=>{
await reset('first-ten');
await raw('raw1-fixed-issuance',1,issue('FIXED'));
await raw('raw11-fixed-admin-refused',11,'8,'+amount(1)+','+A);
await raw('raw1-managed-issuance',1,issue('MNG',true));
await raw('raw11-managed-reference-recipient',11,'9,'+amount(2)+',',A,{address:B});
await raw('raw11-managed-defaults-sender',11,'9,'+amount(3)+',');
await raw('raw12-managed-redemption',12,'9,'+amount(2));
await raw('raw12-outsider-refused',12,'9,'+amount(1),B);
await raw('raw1-duplicate-ticker-refused',1,issue('MNG',true));
await raw('raw11-zero-amount-refused',11,'9,0,'+B);
await raw('raw1-procedural-issuance',1,issue('PROC',true,{proceduralType:'2'}));
if(!flags.includes('--proof10')){
await call('procedural-grant-missing-context','Logic.grantManagedToken',[10,1,B,A,11]);
await raw('procedural-raw-grant-missing-context',11,'a,'+amount(1)+','+B);
await call('procedural-template-real-write','Procedural.upsertTemplate',['template-A',{templateHash:'ABCD'}]);
await call('procedural-contract-real-write','Procedural.upsertContract',['contract-A','template-A','OPEN']);
await raw('procedural-raw-grant-success',11,['a',amount(2),B,'','template-A','contract-A','OPEN',' abcd '].join(','));
await raw('procedural-raw-redeem-open-invalid',12,['a',amount(1),'template-A','contract-A','SETTLED'].join(','),B);
await call('procedural-contract-real-settled','Procedural.transitionContract',['contract-A','SETTLED']);
await raw('procedural-raw-nonadmin-redeem-success',12,['a',amount(1),'template-A','contract-A','SETTLED'].join(','),B);
await call('procedural-direct-wrong-hash','Logic.grantManagedToken',[10,1,B,A,12,'wrong','template-A','contract-A','SETTLED']);
for(const entry of ['types-first','tally-first','volume-first']){
 await reset('entry-'+entry,{entry});await raw(entry+'-raw-managed-issue',1,issue('MNG',true));await raw(entry+'-grant',11,'8,'+amount(1)+','+B);await raw(entry+'-redeem',12,'8,'+amount(1));
}
await reset('direct-issue-fields');
await call('direct-fixed-issue','Logic.tokenIssue',[A,1.123456789,'DIRECT','unused-url',4,false,B,false,12]);
await call('direct-duplicate-returns-error-id-message','Logic.tokenIssue',[A,2,'DIRECT','',0,false,'',false,12]);
await call('direct-nft-managed-type-priority','Logic.tokenIssue',[A,2,'NFT','',0,true,B,true,12]);
await call('direct-procedural-overrides-nft','Logic.tokenIssue',[A,2,'PRNFT','',0,true,B,true,12,0]);
await call('direct-create-negative-initial-tally-error-caught','Logic.tokenIssue',[A,-1,'NEG','',0,false,'',false,12]);
await call('direct-invalid-total-nan-caught','Logic.tokenIssue',[A,'NaN','NAN','',0,false,'',false,12]);
await reset('raw1-encoder-multiple-whitelist');
const encoded=e.load('txEncoder').encodeTokenIssue({initialAmount:1000,ticker:'SHIFT',whitelists:[1,2],managed:true,backupAddress:B,nft:false,coloredCoinHybrid:false});
await raw('raw1-actual-encoder-comma-whitelists-shifts-fields',1,encoded.slice(3));
await raw('raw1-semicolon-whitelists-exact-decode',1,issue('LIST',true,{whitelists:'1;2'}));
await raw('raw1-lowercase-s-rejected',1,issue('sBAD',false));
await raw('raw1-invalid-initial-integer',1,'0,EMPTY,,0,,0,0,');
for(const failure of [{collection:'propertyList',op:'update'},{collection:'tallyMapDelta',op:'insert'},{collection:'tallyMap',op:'update'},{collection:'propertyList',op:'findOne',skip:1},{collection:'propertyList',op:'getDatabase',skip:1}]){
 const label=failure.collection+'-'+failure.op+'-'+(failure.skip??0);await reset('issue-failure-'+label);await raw('issue-first-'+label,1,issue('FAIL',true),A,null,{...failure,message:label});await raw('issue-retry-'+label,1,issue('FAIL',true));await raw('issue-next-after-'+label,1,issue('NEXT',true));
}
for(const skip of [0,1,2,3]){
 await reset('grant-read-failure-'+skip);await raw('managed-issue-before-grant-'+skip,1,issue('MNG',true));await call('grant-read-prefix-'+skip,'Logic.grantManagedToken',[8,1,B,A,12],{collection:'propertyList',op:'findOne',skip,message:'grant-read-'+skip});await call('grant-read-retry-'+skip,'Logic.grantManagedToken',[8,1,B,A,12]);
}
for(const failure of [{collection:'tallyMapDelta',op:'insert'},{collection:'tallyMap',op:'update'},{collection:'propertyList',op:'update'}]){
 const label=failure.collection+'-'+failure.op;await reset('grant-write-failure-'+label);await raw('managed-issue-for-'+label,1,issue('MNG',true));await call('grant-write-prefix-'+label,'Logic.grantManagedToken',[8,1,B,A,12],{...failure,message:label});await call('grant-write-retry-'+label,'Logic.grantManagedToken',[8,1,B,A,12]);await call('redeem-owned-prefix-'+label,'Logic.redeemManagedToken',[8,1001,A,13]);
}
await reset('cache-duplicate-direct');
await call('load-cached-index','Property.getPropertyIndex',[]);
await call('duplicate-cached-ticker-real-error-return','PropertyInstance.createToken',['P1',1,'Fixed',0,A,B]);
await seed('string-key-ticker-index','propertyList',[{_id:'propertyIndex',value:JSON.stringify([['KEY',{ticker:'OTHER',totalInCirculation:1,type:1,issuer:A}]])}]);
await call('load-string-key-index','Property.load',[]);
await call('property-type-own-tostring-rejects-first','PropertyInstance.addProperty',[99,'OBJECTTYPE',1,{toString:1},0,A,'',{}]);
await call('property-type-array-primitive-real-write','PropertyInstance.addProperty',[99,'ARRAYTYPE',1,['Fixed'],0,A,'',{}]);
await call('cache-has-ticker-constructor-error-caught','Logic.tokenIssue',[A,1,'KEY','',0,false,B,false,13]);
await call('nonnumeric-key-next-id-source-nan','PropertyInstance.getNextPropertyId',[]);
await call('nonnumeric-key-issuance-source-nan-id','Logic.tokenIssue',[A,1,'NANID','',0,false,B,false,13]);
for(const [label,rows]of [['missing',[]],['false-value',[{_id:'propertyIndex',value:false}]],['zero-value',[{_id:'propertyIndex',value:0}]],['null-value',[{_id:'propertyIndex',value:null}]],['json-null',[{_id:'propertyIndex',value:'null'}]],['bad-pair',[{_id:'propertyIndex',value:'[null]'}]]]){
 await reset('property-index-'+label,{propertyRows:rows});
 await call('ticker-read-'+label,'Property.doesTickerExist',['NEW']);
 await call('managed-read-'+label,'Property.isManagedAndAdmin',[1,A]);
 await call('load-index-'+label,'Property.load',[]);
 await call('direct-create-index-'+label,'Logic.tokenIssue',[A,1,'NEW','',0,false,B,false,13]);
}
await reset('uncaught-malformed-json-domain',{propertyRows:[{_id:'propertyIndex',value:'bad'}]});
await call('ticker-uncaught-malformed-json-local-refusal','Property.doesTickerExist',['NEW'],undefined,true);

}

})().then(()=>{const output=records.map(record=>JSON.stringify(tag(record))).join('\n')+'\n',destination=path.resolve(__dirname,flags.includes('--proof10')?'../fixtures/lower_property_proof10.jsonl':'../fixtures/lower_property_source.jsonl');if(flags.includes('--stdout')){process.stdout.write(output);return;}if(flags.includes('--check'))assert.equal(fs.readFileSync(destination,'utf8'),output);else fs.writeFileSync(destination,output);console.log(JSON.stringify({observations:records.length,calls:records.filter(r=>r.command.raw||r.command.target).length,version:lock?'9.1.2':'9.3.1',summary:records.map(r=>({name:r.name,result:r.result,properties:[...r.state.propertyCache],unhandled:r.state.unhandled}))},null,2));}).catch(error=>{console.error(error);process.exitCode=1;});
"####;

    fn source() -> Vec<V> {
        let output = std::process::Command::new("node")
            .args(["-e", REFERENCE, "--", "--stdout"])
            .env("PROPERTY_REPO", env!("CARGO_MANIFEST_DIR"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
            .collect()
    }
    fn thrown(e: &tradelayer_core::validity_source::SourceException) -> V {
        obj(&[("name", s(&e.name)), ("message", s(&e.message))])
    }
    fn result(o: &O, raw: bool, params: V) -> V {
        if raw {
            return obj(&[(
                "outcome",
                match o {
                    O::Source(SourceOutcome::Returned(v)) => v.clone(),
                    O::Source(SourceOutcome::Thrown(e)) => {
                        obj(&[("params", params), ("thrown", thrown(e))])
                    }
                    _ => panic!("raw {o:?}"),
                },
            )]);
        }
        let (transport, outcome) = match o {
            O::Dependency(DependencyReturn::Immediate(v)) => {
                ("Immediate", obj(&[("returned", v.clone())]))
            }
            O::Dependency(DependencyReturn::Pending(v)) => {
                ("Pending", obj(&[("returned", v.clone())]))
            }
            O::Dependency(DependencyReturn::Throw(e)) => ("Throw", obj(&[("thrown", thrown(e))])),
            O::Dependency(DependencyReturn::PendingThrow(e)) => {
                ("PendingThrow", obj(&[("thrown", thrown(e))]))
            }
            _ => panic!("direct {o:?}"),
        };
        obj(&[("transport", s(transport)), ("outcome", outcome)])
    }
    #[test]
    fn tlsw3_original_lower_property_issuance_and_prefixes() {
        super::COVERAGE.with(|v| v.borrow_mut().clear());
        let mut replay = Replay::with_policy(P::LowerLifecycleV7).unwrap();
        let (mut resets, mut seeds, mut calls, mut refusals) = (0, 0, 0, 0);
        for case in source() {
            let name = txt(case.get("name"));
            let command = case.get("command");
            if matches!(command.get("reset"), V::Object(_)) {
                resets += 1;
                replay = Replay::with_policy(P::LowerLifecycleV7).unwrap();
                let options = command.get("reset");
                if let V::String(entry) = options.get("entry") {
                    replay.apply(C::SetEntry(entry)).unwrap();
                }
                if options.get("inactive") == V::Bool(true) {
                    replay.apply(C::SetActivation(false)).unwrap();
                }
                for table in arr(case.get("state").get("tables")) {
                    replay
                        .apply(C::Import {
                            collection: txt(table.get("name")),
                            rows: arr(table.get("rows")),
                        })
                        .unwrap();
                }
                continue;
            }
            if matches!(command.get("seed"), V::Object(_)) {
                seeds += 1;
                let seed = command.get("seed");
                replay
                    .apply(C::Import {
                        collection: txt(seed.get("collection")),
                        rows: arr(seed.get("rows")),
                    })
                    .unwrap();
                continue;
            }
            if matches!(command.get("failure"), V::Object(_)) {
                let f = command.get("failure");
                let op = txt(f.get("op"));
                replay
                    .apply(C::FailStorage(StorageFailure {
                        collection: txt(f.get("collection")),
                        operation: if op == "getDatabase" {
                            op
                        } else {
                            format!("{op}Async")
                        },
                        skip: if let V::Number(b) = f.get("skip") {
                            f64::from_bits(b) as usize
                        } else {
                            0
                        },
                        message: txt(f.get("message")),
                    }))
                    .unwrap();
            }
            let raw = matches!(command.get("raw"), V::Array(_));
            let before = replay.runtime().state_value().unwrap();
            let receipt = replay
                .apply(if raw {
                    C::ExecuteContract(arr(command.get("raw")))
                } else {
                    invoke(&txt(command.get("target")), arr(command.get("args")))
                })
                .unwrap();
            if command.get("unsupported") == V::Bool(true) {
                assert_eq!(
                    receipt.outcome,
                    O::LocalError(SourceRuleError::UnsupportedNumericDomain),
                    "{name} explicit native JSON SyntaxError domain"
                );
                assert_eq!(
                    replay.runtime().state_value().unwrap(),
                    before,
                    "{name} local admission rollback"
                );
                assert!(receipt.effects.is_empty());
                refusals += 1;
                continue;
            }
            calls += 1;
            assert!(
                !matches!(receipt.outcome, O::LocalError(_)),
                "{name} {:?} effects={:?}",
                receipt.outcome,
                receipt.effects
            );
            let params = replay
                .runtime()
                .last_contract_parameters()
                .cloned()
                .unwrap_or(V::Undefined);
            assert_eq!(
                result(&receipt.outcome, raw, params),
                case.get("result"),
                "{name} exact source outcome"
            );
            let state = replay.runtime().state_value().unwrap();
            let expected = case.get("state");
            for table in arr(expected.get("tables")) {
                assert_eq!(
                    V::Array(
                        replay
                            .runtime()
                            .collection(&txt(table.get("name")))
                            .to_vec()
                    ),
                    table.get("rows"),
                    "{name} persisted {:?}",
                    table.get("name")
                );
            }
            for (actual, wanted) in [
                (state.get("tally"), "tally"),
                (
                    state.get("globalClearing").get("tallyStaticInitialized"),
                    "tallyInitialized",
                ),
                (state.get("globalClearing").get("tallyMod"), "tallyMod"),
                (
                    state.get("globalClearing").get("propertyCache"),
                    "propertyCache",
                ),
                (
                    state.get("proofLifecycle").get("propertyInitialized"),
                    "propertyInitialized",
                ),
                (
                    state.get("globalClearing").get("unhandledRejections"),
                    "unhandled",
                ),
            ] {
                assert_eq!(actual, expected.get(wanted), "{name} live source {wanted}");
            }
            let effects=receipt.effects.iter().filter_map(|e|{
            if matches!(e.get("kind"),V::String(t)if["database","db","uuid","random","rpc-clock","activation"].contains(&t.as_str())){Some(e.clone())}
            else if e.get("kind")==s("dependency")&&matches!(e.get("target"),V::String(t)if t.starts_with("Logic.")||t.starts_with("Property.")||t.starts_with("Procedural.")){Some(obj(&[("kind",s("source-function")),("target",e.get("target")),("args",e.get("args"))]))}
            else if e.get("kind")==s("dependency")&&e.get("target")==s("Activation.isTxTypeActive"){Some(obj(&[("kind",s("activation")),("method",s("isTxTypeActive")),("args",e.get("args")),("returned",e.get("returned"))]))}
            else{None}
        }).collect::<Vec<_>>();
            assert_eq!(
                V::Array(effects),
                case.get("effects"),
                "{name} complete ordered DB/environment/observed-source calls"
            );
            let bytes = replay.checkpoint().unwrap();
            assert_eq!(
                Replay::from_checkpoint(&bytes, replay.head()).unwrap(),
                replay,
                "{name} full real replay/cache/failure prefix"
            );
        }
        assert_eq!((resets, seeds, calls, refusals), (26, 1, 109, 1));

        super::COVERAGE.with(|coverage| {
            let coverage = coverage.borrow();
            let observed = coverage
                .iter()
                .filter_map(|(_, _, c)| {
                    if let super::C::ExecuteContract(a) = c {
                        if let super::V::Number(b) = a[1] {
                            Some(f64::from_bits(b) as u8)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<std::collections::BTreeSet<_>>();
            eprintln!(
                "TLSW3 property witnessed={} raw={observed:?}",
                coverage.len()
            );
            assert!(
                [1, 11, 12].iter().all(|kind| observed.contains(kind)),
                "missing genuine source raw kind"
            );
        });
    }
}
#[allow(dead_code)]
mod original_oracle {
    use super::CheckedReplay as SourceContractReplay;
    // Whole unchanged Oracle/Logic/Types source over the same Core collections.
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O,
    };
    use tradelayer_core::source_contract_runtime::{
        SourceContractRuntime, SourceContractRuntimePolicy as P,
    };
    use tradelayer_core::validity_source::{
        DependencyCall, DependencyReturn, SourceOutcome, SourceValue as V,
    };
    fn s(value: &str) -> V {
        V::String(value.into())
    }
    fn obj(fields: &[(&str, V)]) -> V {
        V::Object(
            fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }
    const REFERENCE: &str = r###"'use strict';
// Audit only: unchanged pinned business modules; real shared in-memory DB calls.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const stage=__dirname,root=process.env.PROOF_REPO||path.resolve(stage,'../../outputs/TradeLayer-Rust'),flags=process.argv.slice(2),lock=flags.includes('--lock');
const numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(lock){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],code=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(code).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const loaded={exports:{}};vm.runInNewContext(code.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'driver anchor '+before);code=code.replace(before,after);}
replace('return {types,logic,Orderbook,effects','return {context,storage,tables,sources,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',C='LKKG9A9a8n7CeHK8Nk7RdNZiCuHZW2U789',records=[];let e,lastParams;
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toString()];if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),O=e.load('oracle'),P=e.load('property');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,oracleCache:O.instance?.oracles??new Map(),oracleInitialized:!!O.instance,oracleWatermarks:O.lastOracleUpdateBlock,propertyCache:P.instance?.propertyIndex??new Map(),propertyInitialized:!!P.instance,insurance:e.load('insurance').instances,lastParams,unsupported:e.state().unsupported};}
function emit(name,command,result){const snapshot=e.copy(state());assert.deepEqual(snapshot.unsupported,[],name+' unowned boundary');records.push({name,command,result:e.copy(result),state:snapshot,effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','activation'].includes(v.kind)&&!['Oracle.getInstance','Oracle.load','Oracle.getOraclePrice','Insurance.getInstance'].includes(v.target))});}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:e.copy(args)});return Reflect.apply(original,this,args);};}
async function reset(name,options={}){e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally',...options}:options.entry==='volume-first'?{preload:'volumeIndex',...options}:options);lastParams=undefined;for(const method of ['createOracle','publishOracleData','closeOracle','updateAdmin'])observe(e.logic,method,'Logic.'+method);for(const method of ['getInstance','load','getNextId','createOracle','getOracleInfo','isAdmin','updateAdmin','closeOracle','publishData','getOraclePrice'])observe(e.load('oracle'),method,'Oracle.'+method);observe(e.load('tally'),'updateBalance','Tally.updateBalance');for(const method of ['liquidate','getInstance'])observe(e.load('insurance'),method,'Insurance.'+method);await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function call(name,target,args=[],failure,unsupported=false){e.effects.length=0;if(failure)fail(failure);const [module,method]=target.split('.'),receiver=module==='Logic'?e.logic:module==='Oracle'?e.load('oracle'):module==='Tally'?e.load('tally'):e.load('insurance');let transport='Immediate',outcome;try{let value=Reflect.apply(receiver[method],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}if(target==='Oracle.getInstance'&&outcome?.returned)outcome.returned={oracles:outcome.returned.oracles};await flush();emit(name,{target,args,failure,...(unsupported?{unsupported:true}:{})},{transport,outcome});}
async function raw(name,type,payload,sender=A,failure,block=10){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[name,type,'tl',payload,sender,null,0,0,block];let outcome;try{lastParams=await e.types.decodePayload(...args);let value;if(lastParams.valid===true)value=await e.logic.typeSwitch(type,lastParams);outcome={params:lastParams,returned:value};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
async function seed(name,collection,rows){e.effects.length=0;e.tables.set(collection,e.copy(rows));emit(name,{import:{collection,rows}},{outcome:{returned:undefined}});}
(async()=>{
await reset('first10-reset');
await raw('raw13-create',13,'FEED,url,'+B+',1;2,2');
await raw('raw14-publish',14,'2,'+(100*1e4).toString(36)+',a,b,c');
await raw('raw8-new-admin',8,C+',0,1,0,2,0');
await raw('raw14-new-admin',14,'2,'+(101*1e4).toString(36),C,undefined,11);
await raw('raw15-wrong-validator',15,'2',B);
await call('direct-create-scalar','Logic.createOracle',[A,'DIRECT','u',B,[1,2],3,null,12]);
await call('direct-close-missing','Logic.closeOracle',[999,12]);
await call('direct-close-existing','Logic.closeOracle',[2,12]);
await call('publish-prefix-read-failure','Logic.publishOracleData',[2,200,undefined,undefined,undefined,13],{collection:'oracleData',op:'find',message:'price-read'});

for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
 await reset('entry-'+entry,{entry});
 await raw(entry+'-empty-create',13,'');
 await raw(entry+'-duplicate-ticker-create',13,'TEST,u,,1;2,0');
 await raw(entry+'-lag-nan-create',13,'FEED,,,,-');
 await raw(entry+'-valid-publish',14,'1,'+(1010000).toString(36));
 await raw(entry+'-same-height-publish',14,'1,'+(5000000).toString(36));
 await raw(entry+'-outsider-publish',14,'1,'+(1000000).toString(36),'outsider',undefined,11);
 await raw(entry+'-backup-publish',14,'1,'+(1010000).toString(36),B,undefined,11);
 await raw(entry+'-missing-feed',14,'999,1');
 await raw(entry+'-zero-price',14,'1,0');
 await raw(entry+'-nan-price',14,'1,-');
 await raw(entry+'-nan-high',14,'1,1,-');
 await raw(entry+'-positive-high',14,'1,1,10,20,30');
 await raw(entry+'-backup-update',8,C+',0,1,0,1,1');
 await raw(entry+'-unauthorized-update',8,C+',0,1,0,1,0','outsider');
 await raw(entry+'-missing-admin-target',8,C+',0,1,0,99,0');
 await raw(entry+'-no-admin-target',8,C+',0,0,0,1,0');
 await raw(entry+'-token-and-oracle-targets',8,C+',0,1,1,1,0');
 await raw(entry+'-raw15-negative',15,'-1');
 await call(entry+'-close-one','Logic.closeOracle',[1,12]);
 await raw(entry+'-publish-closed',14,'1,2',A,undefined,12);
}
await reset('inactive',{inactive:true});await raw('inactive-create',13,'NO');await raw('inactive-publish',14,'1,1');await raw('inactive-admin',8,C+',0,1,0,1,0');
await reset('direct-shapes');
for(const [name,args]of [['scalar',['name',A]],['undefined',[]],['null',[null,A]],['zero',[0,A]],['array',[[],A]],['config',[{name:'n',ticker:'t',lag:'2',clearlists:[1,'x'],backupAddress:B,url:'u'},A]],['fallbacks',[{name:0,ticker:false,lag:null,clearlists:{},adminAddress:'',backupAddress:false},A]],['nan-lag',[{ticker:'N',lag:'invalid'}]],['infinite-lag',[{ticker:'I',lag:Infinity}]],['array-lag',[{ticker:'A',lag:[]}]],['throw-lag',[{ticker:'bad',lag:{valueOf:1,toString:1}}]]])await call('create-'+name,'Oracle.createOracle',args);
await call('read-next-id','Oracle.getNextId');await call('read-instance','Oracle.getInstance');
await call('undefined-admin-missing-feed','Oracle.isAdmin',[undefined,999]);await call('null-admin-missing-feed','Oracle.isAdmin',[null,999]);await call('existing-primary','Oracle.isAdmin',[A,1]);await call('existing-backup','Oracle.isAdmin',[B,1]);
await call('set-backup-direct','Oracle.updateAdmin',[1,'new-backup',[]]);await call('set-primary-undefined','Oracle.updateAdmin',[1,undefined,false]);await call('logic-token-receiver-error','Logic.updateAdmin',[false,true,true,1,C,false,10]);await call('logic-no-target','Logic.updateAdmin',[false,false,false,1,C,false,10]);
await call('missing-direct-update','Oracle.updateAdmin',[999,C,false]);await call('missing-direct-close','Oracle.closeOracle',[999]);
for(const [name,target,args,collection,op,skip]of[
 ['create-next-factory','Logic.createOracle',[A,'new'],'oracleList','getDatabase',0],['create-next-find','Logic.createOracle',[A,'new'],'oracleList','find',0],['create-insert-factory','Logic.createOracle',[A,'new'],'oracleList','getDatabase',1],['create-insert','Logic.createOracle',[A,'new'],'oracleList','insert',0],
 ['admin-factory','Oracle.updateAdmin',[1,C,false],'oracleList','getDatabase',0],['admin-read','Oracle.updateAdmin',[1,C,false],'oracleList','findOne',0],['admin-write','Oracle.updateAdmin',[1,C,false],'oracleList','update',0],
 ['admin-validity-read','raw',[],'oracleList','findOne',0],
 ['close-factory','Logic.closeOracle',[1,10],'oracleList','getDatabase',0],['close-read','Logic.closeOracle',[1,10],'oracleList','findOne',0],['close-write','Logic.closeOracle',[1,10],'oracleList','update',0],['close-insurance-factory','Logic.closeOracle',[1,10],'insurance','getDatabase',0],['close-insurance-load','Logic.closeOracle',[1,10],'insurance','findOne',0],
 ['publish-read','Logic.publishOracleData',[1,110,undefined,undefined,undefined,11],'oracleData','find',0],['publish-write','Logic.publishOracleData',[1,110,undefined,undefined,undefined,11],'oracleData','update',0]
]){await reset('failure-reset-'+name);const failure={collection,op,skip,message:name};if(target==='raw'){await raw(name+'-failed',14,'1,1',A,failure);await raw(name+'-retry',14,'1,1');}else{await call(name+'-failed',target,args,failure);await call(name+'-retry',target,args);}}
await reset('id-fallback-cache');
await seed('id-source-rows','oracleList',[{_id:'oracle-4foo'},{_id:'oracle-0x10'},{_id:'oracle-2-9',id:null},{_id:'oracle-x',id:'bad'}]);await call('load-id-source','Oracle.load');await seed('id-clear-db','oracleList',[]);await call('next-cache-parseInt','Oracle.getNextId');await call('create-cache-max','Oracle.createOracle',['cache',A]);
await reset('id-numeric-rows');await seed('numeric-id-import','oracleList',[{_id:'oracle-2',id:2.5},{_id:'oracle-100',id:'10.5'},{_id:'oracle-infinite',id:Infinity},{_id:'oracle-nan',id:NaN}]);await call('numeric-next','Oracle.getNextId');await call('numeric-create','Oracle.createOracle',[{ticker:'FRACTION'},A]);
await reset('bad-cache-key');await seed('numeric-key-rows','oracleList',[{_id:4}]);await call('load-bad-key','Oracle.load');await seed('clear-bad-key-db','oracleList',[]);await call('bad-cache-key-source-error','Oracle.getNextId');await seed('positive-id-avoids-cache','oracleList',[{_id:'x',id:5}]);await call('positive-id-returns-before-bad-key','Oracle.getNextId');
await reset('nested-legacy');await seed('nested-oracle-import','oracleList',[{_id:'oracle-1',name:{adminAddress:A,backupAddress:B,ticker:'legacy'}}]);await raw('nested-publish',14,'1,1');await raw('nested-backup-admin',8,C+',0,1,0,1,0',B);await call('nested-close-primary-undefined','Logic.closeOracle',[1,15]);


for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
 await reset('closed-insurance-prefix-'+entry,{entry});
 await seed('closed-insurance-snapshot-'+entry,'insurance',[{_id:'snapshot',key:'true',value:{contractSeriesId:true,balances:[{propertyId:7,amountAvailable:20}],hedgeRatio:0.5,block:10}}]);
 await call('closed-insurance-tally-prefix-'+entry,'Logic.closeOracle',[1,10]);
 await call('closed-insurance-repeat-'+entry,'Logic.closeOracle',[1,11]);
}
await reset('post-close-write-error');await seed('post-close-snapshot','insurance',[{_id:'snapshot',key:'true',value:{contractSeriesId:true,balances:[{propertyId:7,amountAvailable:20}],hedgeRatio:0.5,block:10}}]);await call('insurance-tally-save-caught','Oracle.closeOracle',[1],{collection:'tallyMap',op:'update',message:'insurance-tally-save'});await call('insurance-tally-save-retry','Oracle.closeOracle',[1]);

await reset('composite-actor-boundary');
await call('create-bignumber-admin','Oracle.createOracle',[{ticker:'BNADMIN',adminAddress:new BN(7)}]);
await call('bignumber-actor-actual-false-local-identity-boundary','Oracle.isAdmin',[new BN(7),2],undefined,true);
for(const record of records)console.log(JSON.stringify(tag(record)));
})().catch(error=>{console.error(error.stack||error);process.exitCode=1});
"###;
    fn text(value: V) -> String {
        if let V::String(value) = value {
            value
        } else {
            panic!("expected source string: {value:?}")
        }
    }
    fn array(value: V) -> Vec<V> {
        if let V::Array(value) = value {
            value
        } else {
            panic!("expected source array: {value:?}")
        }
    }
    fn cases(lock: bool) -> Vec<V> {
        let output = std::process::Command::new("node")
            .env("PROOF_REPO", env!("CARGO_MANIFEST_DIR"))
            .args([
                "-e",
                REFERENCE,
                "lower-oracle-reference",
                if lock { "--lock" } else { "--baseline" },
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
            .collect()
    }
    fn returned(value: DependencyReturn) -> V {
        let (transport, outcome) = match value {
            DependencyReturn::Immediate(value) => ("Immediate", obj(&[("returned", value)])),
            DependencyReturn::Pending(value) => ("Pending", obj(&[("returned", value)])),
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
    fn fault(command: &V) -> Option<tradelayer_core::source_contract_runtime::StorageFailure> {
        let f = command.get("failure");
        if f == V::Undefined {
            return None;
        }
        let op = text(f.get("op"));
        Some(tradelayer_core::source_contract_runtime::StorageFailure {
            collection: text(f.get("collection")),
            operation: if op == "getDatabase" {
                op
            } else {
                format!("{op}Async")
            },
            skip: if let V::Number(bits) = f.get("skip") {
                f64::from_bits(bits) as usize
            } else {
                0
            },
            message: text(f.get("message")),
        })
    }
    fn command(c: &V) -> C {
        if let V::Object(_) = c.get("import") {
            let i = c.get("import");
            return C::Import {
                collection: text(i.get("collection")),
                rows: array(i.get("rows")),
            };
        }
        if let V::Array(args) = c.get("raw") {
            C::ExecuteContract(args)
        } else {
            C::Invoke(DependencyCall {
                target: text(c.get("target")),
                arguments: array(c.get("args")),
            })
        }
    }
    fn actual(runtime: &mut SourceContractRuntime, case: &V) -> V {
        let c = case.get("command");
        if let V::Object(_) = c.get("import") {
            let i = c.get("import");
            runtime
                .import_collection(&text(i.get("collection")), array(i.get("rows")))
                .unwrap();
            return obj(&[("outcome", obj(&[("returned", V::Undefined)]))]);
        }
        if let Some(f) = fault(&c) {
            runtime.fail_storage(f).unwrap();
        }
        if let V::Array(args) = c.get("raw") {
            let result = match runtime.execute_contract_unverified(&args).unwrap() {
                SourceOutcome::Returned(v) => v,
                SourceOutcome::Thrown(e) => obj(&[
                    (
                        "params",
                        runtime
                            .last_contract_parameters()
                            .cloned()
                            .unwrap_or(V::Undefined),
                    ),
                    (
                        "thrown",
                        obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                    ),
                ]),
            };
            obj(&[("outcome", result)])
        } else {
            returned(
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: text(c.get("target")),
                        arguments: array(c.get("args")),
                    })
                    .unwrap(),
            )
        }
    }
    fn assert_state(runtime: &SourceContractRuntime, case: &V) {
        let expected = case.get("state");
        let name = text(case.get("name"));
        let mut known = std::collections::BTreeSet::new();
        for table in array(expected.get("tables")) {
            let collection = text(table.get("name"));
            known.insert(collection.clone());
            assert_eq!(
                V::Array(runtime.collection(&collection).to_vec()),
                table.get("rows"),
                "{name} full rows {collection}"
            );
        }
        let state = runtime.state_value().unwrap();
        assert_eq!(state.get("runtimePolicy"), s("LowerLifecycleV7"));
        for table in array(state.get("tables")) {
            if !known.contains(&text(table.get("name"))) {
                assert_eq!(
                    table.get("rows"),
                    V::Array(vec![]),
                    "{name} no invented rows"
                );
            }
        }
        for (key, wanted) in [
            ("oracleSourceCache", "oracleCache"),
            ("oracleSourceInitialized", "oracleInitialized"),
            ("oracleSourceWatermarks", "oracleWatermarks"),
        ] {
            assert_eq!(state.get(key), expected.get(wanted), "{name} {key}");
        }
        // The existing Core stores the actual string-keyed Insurance Map as object
        // fields. Project its exact entries; compare every populated instance too.
        let V::Object(insurance) = state.get("insurance") else {
            panic!("insurance Map projection")
        };
        assert_eq!(
            V::Array(
                insurance
                    .into_iter()
                    .map(|(key, value)| V::Array(vec![V::String(key), value]))
                    .collect()
            ),
            expected.get("insurance"),
            "{name} complete Insurance Map entries"
        );
        let global = state.get("globalClearing");
        for (key, wanted) in [
            ("tallyStaticInitialized", "tallyInitialized"),
            ("tallyMod", "tallyMod"),
            ("propertyCache", "propertyCache"),
        ] {
            assert_eq!(global.get(key), expected.get(wanted), "{name} {key}");
        }
        assert_eq!(
            state.get("proofLifecycle").get("propertyInitialized"),
            expected.get("propertyInitialized"),
            "{name} shared property constructor"
        );
        assert_eq!(
            runtime.tally_cache(),
            expected.get("tally"),
            "{name} sole Tally"
        );
        assert_eq!(
            runtime
                .last_contract_parameters()
                .cloned()
                .unwrap_or(V::Undefined),
            expected.get("lastParams"),
            "{name} original raw parameters"
        );
    }
    fn effects(values: Vec<V>) -> V {
        const TARGETS: &[&str] = &[
            "Logic.createOracle",
            "Logic.publishOracleData",
            "Logic.closeOracle",
            "Logic.updateAdmin",
            "Oracle.createOracle",
            "Oracle.getNextId",
            "Oracle.isAdmin",
            "Oracle.updateAdmin",
            "Oracle.closeOracle",
            "Oracle.publishData",
            "Oracle.getOracleInfo",
            "Insurance.liquidate",
            "Tally.updateBalance",
        ];
        V::Array(values.into_iter().filter_map(|v|{
        if matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str())) {Some(v)}
        else if v.get("kind")==s("dependency")&&matches!(v.get("target"),V::String(t)if TARGETS.contains(&t.as_str())){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}
        else if v.get("kind")==s("dependency")&&v.get("target")==s("Activation.isTxTypeActive"){Some(obj(&[("kind",s("activation")),("method",s("isTxTypeActive")),("args",v.get("args")),("returned",v.get("returned"))]))}
        else{None}
    }).collect())
    }
    fn setup(runtime: &mut SourceContractRuntime, case: &V) {
        let options = case.get("command").get("reset");
        runtime
            .set_entry(&if options.get("entry") == V::Undefined {
                "main-subgraph".into()
            } else {
                text(options.get("entry"))
            })
            .unwrap();
        runtime.set_activation_answer(options.get("inactive") != V::Bool(true));
        for table in array(case.get("state").get("tables")) {
            runtime
                .import_collection(&text(table.get("name")), array(table.get("rows")))
                .unwrap();
        }
    }
    #[test]
    fn tlsw3_original_lower_oracle_admin_publication_and_prefixes() {
        super::COVERAGE.with(|v| v.borrow_mut().clear());
        let records = cases(false);
        let mut runs = 0;
        let (mut source_calls, mut input_frames) = (0, 0);
        for start in records.iter().enumerate().filter_map(|(i, r)| {
            let name = text(r.get("name"));
            (r.get("command").get("reset") != V::Undefined
                && (name.starts_with("failure-reset-")
                    || name.starts_with("closed-insurance-prefix-")
                    || [
                        "first10-reset",
                        "id-fallback-cache",
                        "bad-cache-key",
                        "nested-legacy",
                        "post-close-write-error",
                    ]
                    .contains(&name.as_str())))
            .then_some(i)
        }) {
            runs += 1;
            let mut replay = SourceContractReplay::with_policy(P::LowerLifecycleV7).unwrap();
            let options = records[start].get("command").get("reset");
            if options.get("entry") != V::Undefined {
                replay
                    .apply(C::SetEntry(text(options.get("entry"))))
                    .unwrap();
            }
            for table in array(records[start].get("state").get("tables")) {
                replay
                    .apply(C::Import {
                        collection: text(table.get("name")),
                        rows: array(table.get("rows")),
                    })
                    .unwrap();
            }
            for case in records[start + 1..]
                .iter()
                .take_while(|r| r.get("command").get("reset") == V::Undefined)
            {
                let c = case.get("command");
                if let Some(f) = fault(&c) {
                    replay.apply(C::FailStorage(f)).unwrap();
                }
                if c.get("import") == V::Undefined {
                    source_calls += 1;
                } else {
                    input_frames += 1;
                }
                let receipt = replay.apply(command(&c)).unwrap();
                assert!(
                    !matches!(receipt.outcome, O::LocalError(_)),
                    "actual source receipt"
                );
                let observed = match &receipt.outcome {
                    O::Source(SourceOutcome::Returned(value)) => obj(&[("outcome", value.clone())]),
                    O::Source(SourceOutcome::Thrown(error)) => obj(&[(
                        "outcome",
                        obj(&[
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
                                obj(&[("name", s(&error.name)), ("message", s(&error.message))]),
                            ),
                        ]),
                    )]),
                    O::Dependency(value) => returned(value.clone()),
                    O::Void => obj(&[("outcome", obj(&[("returned", V::Undefined)]))]),
                    value => panic!("unavailable source witness {value:?}"),
                };
                assert_eq!(observed, case.get("result"), "TLSW3 exact original outcome");
                assert_eq!(
                    effects(receipt.effects.clone()),
                    case.get("effects"),
                    "TLSW3 exact ordered original effects"
                );
                assert_state(replay.runtime(), case);
                let bytes = replay.checkpoint().unwrap();
                assert_eq!(&bytes[..5], b"TLCR\x07");
                let restored =
                    SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
                assert_eq!(restored, replay);
                assert_eq!(restored.checkpoint().unwrap(), bytes);
                replay = restored;
            }
        }
        eprintln!("TLSW3 oracle source_calls={source_calls} input_frames={input_frames} resets={runs} local_refusals=0 (composite actor scene excluded)");
        assert_eq!(runs, 24);

        super::COVERAGE.with(|coverage| {
            let coverage = coverage.borrow();
            let observed = coverage
                .iter()
                .filter_map(|(_, _, c)| {
                    if let super::C::ExecuteContract(a) = c {
                        if let super::V::Number(b) = a[1] {
                            Some(f64::from_bits(b) as u8)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<std::collections::BTreeSet<_>>();
            eprintln!("TLSW3 oracle witnessed={} raw={observed:?}", coverage.len());
            assert!(
                [8, 13, 14, 15].iter().all(|kind| observed.contains(kind)),
                "missing genuine source raw kind"
            );
        });
    }
}
#[allow(dead_code)]
mod original_main {
    use super::CheckedReplay as SourceContractReplay;
    // Callable Main evidence only: daemon import/startup/RPC are not supplied.
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O,
    };
    use tradelayer_core::source_contract_runtime::{
        SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
    };
    use tradelayer_core::validity_source::{DependencyCall, DependencyReturn, SourceValue as V};
    fn s(v: impl Into<String>) -> V {
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
    fn text(v: V) -> String {
        if let V::String(v) = v {
            v
        } else {
            panic!("expected string")
        }
    }
    fn array(v: V) -> Vec<V> {
        if let V::Array(v) = v {
            v
        } else {
            panic!("expected array")
        }
    }
    // Observer projection only. The pinned original class/module methods execute
    // in the checked-in generator; no returned validity/financial verdict is input.
    const OBSERVER: &str = r#"
const path=require('node:path'),cp=require('node:child_process');
const root=process.env.PROOF_REPO,stage=process.env.TL_MAIN_EVIDENCE_ROOT||root;
const args=[path.join(stage,'scripts/main_phase_vectors.cjs'),'--print','--check'];if(process.argv.includes('--lock'))args.push('--lock');
const raw=cp.execFileSync(process.execPath,args,{env:{...process.env,PROOF_REPO:root},maxBuffer:16*1024*1024,encoding:'utf8'});
const U=['undefined'],A=x=>['array',x],S=x=>['string',x],B=x=>['boolean',x],O=x=>['object',Object.entries(x)];
function get(v,key){return v?.[0]==='object'?(v[1].find(([k])=>k===key)?.[1]||U):U;}
function fallback(v,x){return v[0]==='undefined'?x:v;}
function norm(v){const t=v[0];if(t==='map')return A(v[1].map(([k,x])=>A([norm(k),norm(x)])));if(t==='set')return A(v[1].map(norm));if(t==='array')return A(v[1].map(norm));if(t==='object')return ['object',v[1].map(([k,x])=>[k,norm(x)])];if(t==='regexp'){return v[1]==='^tx'&&v[2]===''?S('^tx'):O({'$regexp':S(v[1]),'$flags':S(v[2])});}if(t==='function'||t==='cycle')throw Error('unadmitted identity transport');return v;}
for(const line of raw.trim().split('\n')){const r=JSON.parse(line),c=r.command;
 if(get(c,'profile')[1]==='full-import')continue;
 const st=r.state,src=get(st,'source'),t=get(st,'tally'),p=get(st,'property'),k=get(st,'contract'),ki=get(k,'instance'),o=get(src,'oracle'),oi=get(o,'instance'),cl=get(src,'clearing'),ch=get(st,'channels');
 const state=O({tables:get(src,'tables'),books:get(src,'instances'),margins:get(st,'marginInstances'),tally:fallback(get(t,'addresses'),A([])),tallyInitialized:get(t,'initialized'),tallyMod:get(t,'modFlag'),contractMod:get(k,'modFlag'),propertyCache:fallback(get(p,'propertyIndex'),A([])),propertyInitialized:B(p[0]!=='undefined'),contractSeries:fallback(get(ki,'contractSeries'),A([])),registryInitialized:B(ki[0]!=='undefined'),seriesInitialized:B(get(ki,'contractSeries')[0]!=='undefined'),activation:get(st,'activation'),channels:get(ch,'registry'),pendingWithdrawals:get(ch,'pending'),contexts:get(st,'clearingContexts'),blockTrades:get(cl,'blockTrades'),deleverageTrades:get(cl,'deleverageTrades'),liquidationRecords:get(cl,'liquidationRecords'),oracleMarks:get(cl,'latestOracleMarkById'),nativeMarks:get(cl,'latestNativeMarkById'),oracleCache:fallback(get(oi,'oracles'),A([])),oracleInitialized:B(oi[0]!=='null'&&oi[0]!=='undefined'),oracleWatermarks:get(o,'lastOracleUpdateBlock'),insurance:get(src,'insurance'),globals:get(st,'globals')});
 const effects=A(r.effects[1].filter(v=>{const kind=get(v,'kind')[1];return ['database','db','uuid','random','rpc-clock','unhandled-rejection'].includes(kind)||(kind==='source-function'&&get(v,'target')[1]!=='Types.decodePayload');}));
 console.log(JSON.stringify(norm(O({name:S(r.name),command:c,result:r.result,state,effects}))));
}
"#;
    fn cases(lock: bool) -> Vec<V> {
        let out = std::process::Command::new("node")
            .env("PROOF_REPO", env!("CARGO_MANIFEST_DIR"))
            .args([
                "-e",
                OBSERVER,
                "main-observer",
                if lock { "--lock" } else { "--baseline" },
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        std::str::from_utf8(&out.stdout)
            .unwrap()
            .lines()
            .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
            .collect()
    }
    fn call(target: &str, args: Vec<V>) -> DependencyCall {
        DependencyCall {
            target: target.into(),
            arguments: args,
        }
    }
    fn result(value: DependencyReturn) -> V {
        let (transport, outcome) = match value {
            DependencyReturn::Immediate(v) => ("Immediate", obj(&[("returned", v)])),
            DependencyReturn::Pending(v) => ("Pending", obj(&[("returned", v)])),
            DependencyReturn::Throw(e) => (
                "Throw",
                obj(&[(
                    "thrown",
                    obj(&[("name", s(e.name)), ("message", s(e.message))]),
                )]),
            ),
            DependencyReturn::PendingThrow(e) => (
                "PendingThrow",
                obj(&[(
                    "thrown",
                    obj(&[("name", s(e.name)), ("message", s(e.message))]),
                )]),
            ),
        };
        obj(&[("transport", s(transport)), ("outcome", outcome)])
    }
    fn failure(c: &V) -> Option<StorageFailure> {
        let f = c.get("failure");
        if f == V::Undefined {
            return None;
        }
        let op = text(f.get("op"));
        Some(StorageFailure {
            collection: text(f.get("collection")),
            operation: if op == "getDatabase" {
                op
            } else {
                format!("{op}Async")
            },
            skip: if let V::Number(b) = f.get("skip") {
                f64::from_bits(b) as usize
            } else {
                0
            },
            message: text(f.get("message")),
        })
    }
    fn compare(a: &V, b: &V, label: &str) {
        fn diff(a: &V, b: &V, path: String) -> String {
            match (a, b) {
                (V::Array(a), V::Array(b)) => {
                    for (i, (a, b)) in a.iter().zip(b).enumerate() {
                        if a != b {
                            return diff(a, b, format!("{path}[{i}]"));
                        }
                    }
                    format!("{path} array lengths {} != {}", a.len(), b.len())
                }
                (V::Object(a), V::Object(b)) => {
                    for ((ak, av), (bk, bv)) in a.iter().zip(b) {
                        if ak != bk {
                            return format!("{path} fields {ak} != {bk}");
                        }
                        if av != bv {
                            return diff(av, bv, format!("{path}.{ak}"));
                        }
                    }
                    format!("{path} field counts {} != {}", a.len(), b.len())
                }
                _ => format!("{path}: {a:?} != {b:?}"),
            }
        }
        assert!(a == b, "{label}: {}", diff(a, b, String::new()));
    }
    fn normalized(v: V) -> V {
        match v {
            V::BigNumber(t) => V::BigNumber(
                t.parse::<tradelayer_core::decimal::Decimal>()
                    .map(|d| d.to_string())
                    .unwrap_or(t),
            ),
            V::Array(a) => V::Array(a.into_iter().map(normalized).collect()),
            V::Object(o) => V::Object(o.into_iter().map(|(k, v)| (k, normalized(v))).collect()),
            _ => v,
        }
    }
    fn state(runtime: &SourceContractRuntime, case: &V) {
        let name = text(case.get("name"));
        let expected = case.get("state");
        let all = runtime.state_value().unwrap();
        let g = all.get("globalClearing");
        let mut known = std::collections::BTreeSet::new();
        for t in array(expected.get("tables")) {
            let k = text(t.get("name"));
            known.insert(k.clone());
            compare(
                &V::Array(runtime.collection(&k).to_vec()),
                &t.get("rows"),
                &format!("{name} full {k}"),
            );
        }
        for t in array(all.get("tables")) {
            if !known.contains(&text(t.get("name"))) {
                compare(
                    &t.get("rows"),
                    &V::Array(vec![]),
                    &format!("{name} extra rows"),
                );
            }
        }
        for (actual, wanted) in [
            (runtime.tally_cache(), expected.get("tally")),
            (
                g.get("tallyStaticInitialized"),
                expected.get("tallyInitialized"),
            ),
            (g.get("tallyMod"), expected.get("tallyMod")),
            (g.get("contractModRaw"), expected.get("contractMod")),
            (g.get("propertyCache"), expected.get("propertyCache")),
            (
                all.get("proofLifecycle").get("propertyInitialized"),
                expected.get("propertyInitialized"),
            ),
            (g.get("contractSeries"), expected.get("contractSeries")),
            (
                all.get("registryLifecycle").get("registryInitialized"),
                expected.get("registryInitialized"),
            ),
            (
                all.get("registryLifecycle").get("seriesInitialized"),
                expected.get("seriesInitialized"),
            ),
            (
                all.get("mainCallable").get("activation"),
                expected.get("activation"),
            ),
            (g.get("channels").get("registry"), expected.get("channels")),
            (
                g.get("channels").get("pendingWithdrawals"),
                expected.get("pendingWithdrawals"),
            ),
            (all.get("oracleSourceCache"), expected.get("oracleCache")),
            (
                all.get("oracleSourceInitialized"),
                expected.get("oracleInitialized"),
            ),
            (
                all.get("oracleSourceWatermarks"),
                expected.get("oracleWatermarks"),
            ),
        ] {
            compare(&actual, &wanted, &format!("{name} source caches"));
        }
        for key in ["blockTrades", "oracleMarks", "nativeMarks"] {
            compare(&all.get(key), &expected.get(key), &format!("{name} {key}"));
        }
        for key in ["deleverageTrades", "liquidationRecords"] {
            compare(&g.get(key), &expected.get(key), &format!("{name} {key}"));
        }
        let V::Object(ins) = all.get("insurance") else {
            panic!()
        };
        compare(
            &V::Array(
                ins.into_iter()
                    .map(|(k, v)| V::Array(vec![s(k), v]))
                    .collect(),
            ),
            &expected.get("insurance"),
            &format!("{name} insurance"),
        );
        let margins = array(expected.get("margins"));
        assert_eq!(
            array(runtime.margins().state_value().unwrap().get("instances")).len(),
            margins.len(),
            "{name} margin receiver count"
        );
        for (i, m) in margins.iter().enumerate() {
            compare(
                &runtime.margins().instance_value(i as u64 + 1).unwrap(),
                m,
                &format!("{name} margin{i}"),
            );
        }
        let books = array(expected.get("books"));
        assert_eq!(
            array(runtime.queue().state_value().get("instances")).len(),
            books.len(),
            "{name} book receiver count"
        );
        for b in books {
            let V::Number(id) = b.get("id") else { panic!() };
            let V::Object(fields) = b else { panic!() };
            let wanted = V::Object(fields.into_iter().filter(|(k, _)| k != "id").collect());
            compare(
                &normalized(
                    runtime
                        .queue()
                        .instance_value(f64::from_bits(id) as u64)
                        .unwrap(),
                ),
                &normalized(wanted),
                &format!("{name} whole queue receiver"),
            );
        }
        let objects = array(all.get("margins").get("objects"));
        let contexts = V::Array(
            array(g.get("contexts"))
                .into_iter()
                .map(|p| {
                    let p = array(p);
                    V::Array(vec![
                        p[0].clone(),
                        V::Array(
                            array(p[1].clone())
                                .into_iter()
                                .map(|id| {
                                    let V::Number(id) = id else { panic!() };
                                    objects[f64::from_bits(id) as usize].clone()
                                })
                                .collect(),
                        ),
                    ])
                })
                .collect(),
        );
        compare(
            &contexts,
            &expected.get("contexts"),
            &format!("{name} clearing aliases"),
        );
        compare(
            &all.get("liquidityRewards").get("queue"),
            &expected.get("globals").get("liquidityQueue"),
            &format!("{name} reward queue"),
        );
        compare(
            &all.get("liquidityRewards").get("volumeCache"),
            &expected.get("globals").get("liquidityVolume"),
            &format!("{name} reward volume"),
        );
    }
    fn effects(values: Vec<V>) -> V {
        V::Array(values.into_iter().filter_map(|v|{
    let kind=v.get("kind");if matches!(&kind,V::String(k)if ["database","db","uuid","random","rpc-clock","unhandled-rejection"].contains(&k.as_str())){Some(v)}else if kind==s("main-phase"){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}else{None}
}).collect())
    }
    fn imports(case: &V) -> Vec<C> {
        array(case.get("state").get("tables"))
            .into_iter()
            .map(|t| C::Import {
                collection: text(t.get("name")),
                rows: array(t.get("rows")),
            })
            .collect()
    }
    #[test]
    fn tlsw3_original_main_transactions_and_six_postphases() {
        super::COVERAGE.with(|v| v.borrow_mut().clear());
        let baseline = cases(false);
        let mut replay = SourceContractReplay::with_policy(P::MainCallableV8).unwrap();
        let mut admitted = 0;
        for case in &baseline {
            let c = case.get("command");
            if c.get("reset") != V::Undefined {
                replay = SourceContractReplay::with_policy(P::MainCallableV8).unwrap();
                for i in imports(case) {
                    replay.apply(i).unwrap();
                }
                continue;
            }
            let target = text(c.get("target"));
            if matches!(
                target.as_str(),
                "Main.constructConsensusFromIndex" | "Main.blockHandlerBegin"
            ) {
                continue;
            }
            if let Some(f) = failure(&c) {
                replay.apply(C::FailStorage(f)).unwrap();
            }
            let receipt = replay
                .apply(C::Invoke(call(&target, array(c.get("args")))))
                .unwrap();
            let O::Dependency(v) = receipt.outcome.clone() else {
                panic!("Main must execute, not record a supplied verdict")
            };
            compare(&result(v), &case.get("result"), "replayed genuine outcome");
            compare(
                &effects(receipt.effects.clone()),
                &case.get("effects"),
                "TLSW3 exact Main ordered effects",
            );
            state(replay.runtime(), case);
            let bytes = replay.checkpoint().unwrap();
            let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
            assert!(
                restored == replay,
                "whole replay state including aliases/counters/profile"
            );
            assert!(
                restored.checkpoint().unwrap() == bytes,
                "canonical checkpoint"
            );
            let mut wrong = replay.head();
            wrong.commitment[0] ^= 1;
            assert!(SourceContractReplay::from_checkpoint(&bytes, wrong).is_err());
            let mut altered = bytes;
            let at = altered.len() / 2;
            altered[at] ^= 1;
            assert!(SourceContractReplay::from_checkpoint(&altered, replay.head()).is_err());
            admitted += 1;
        }
        assert_eq!(admitted, 18);
        super::COVERAGE.with(|v| {
            let v = v.borrow();
            assert_eq!(
                v.iter()
                    .filter(|(_, scope, _)| *scope == super::Scope::MainCallableSlice)
                    .count(),
                18
            );
            eprintln!("TLSW3 Main original source slices=18; raw guard remains18/32");
        });
    }
}

#[allow(dead_code)]
mod original_send_commit {
    use super::CheckedReplay as Replay;
    // Unchanged pinned send/commit graph over the sole actual Core stores.
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O,
    };
    use tradelayer_core::source_contract_runtime::{
        SourceContractRuntimePolicy as P, StorageFailure,
    };
    use tradelayer_core::validity_source::{
        DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
    };
    fn n(v: f64) -> V {
        V::number(v)
    }
    fn s(v: &str) -> V {
        V::String(v.into())
    }
    fn obj(v: &[(&str, V)]) -> V {
        V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
    }
    fn arr(v: V) -> Vec<V> {
        if let V::Array(v) = v {
            v
        } else {
            panic!("array {v:?}")
        }
    }
    fn txt(v: V) -> String {
        if let V::String(v) = v {
            v
        } else {
            panic!("string {v:?}")
        }
    }
    fn invoke(target: &str, arguments: Vec<V>) -> C {
        C::Invoke(DependencyCall {
            target: target.into(),
            arguments,
        })
    }
    const REFERENCE: &str = r####"
'use strict';
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=process.env.SEND_COMMIT_REPO||path.resolve(__dirname,'../../../outputs/TradeLayer-Rust'),flags=process.argv.slice(1),lock=flags.includes('--lock')||process.env.TL_SOURCE_BN_VERSION==='9.1.2',numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(lock){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],raw=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(raw).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const loaded={exports:{}};vm.runInNewContext(raw.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'driver anchor '+before);code=code.replace(before,after);}
replace('const sources = Object.fromEntries(',`pins.procedural=['legacy_procedural_source.txt','0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e'];\nconst sources = Object.fromEntries(`);
replace("'vaults','procedural','bitvmCache'","'vaults','bitvmCache'");
// Ordinary host console methods return undefined; the reused audit driver's
// logs.push return length must not become a fabricated addProperty result.
replace("(...values)=>logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))})","(...values)=>{logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))});}");
replace('return {types,logic,Orderbook,effects','return {adapters,context,storage,tables,unsupported,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);

const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='LKQa26zKBtyM2kw6aMEYyj4fCbCqxJyrPw',records=[];let e,lastParams,unhandled=[];process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toString()];if(Object.prototype.toString.call(v)==='[object Error]')return tag({name:v.name,message:v.message});if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<6;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),P=e.load('property'),C=e.load('channels');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,tallyInstance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined,propertyCache:P.instance?.propertyIndex??new Map(),propertyInitialized:!!P.instance,channels:C.channelsRegistry,withdrawals:C.pendingWithdrawals,lastParams,unhandled,unsupported:e.unsupported.slice()};}
function emit(name,command,result){const snapshot=e.copy(state());if(!command.unsupported)assert.deepEqual(snapshot.unsupported,[],name+' unowned boundary');records.push({name,command,result:e.copy(result),state:snapshot,effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','rpc','activation'].includes(v.kind))});}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:structuredClone(args)});return Reflect.apply(original,this,args);};}
function bindRpc(){
 const source=fs.readFileSync(path.join(root,'fixtures/source_channel_txutils_reference.txt'),'utf8');assert.equal(crypto.createHash('sha256').update(source).digest('hex'),'38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3');
 const moduleRoot=process.env.TRADELAYER_JS_ROOT||path.resolve(root,'../../../../tradelayer.js'),external=Module.createRequire(path.join(moduleRoot,'package.json')),own={exports:{}};
 const wrapper=vm.runInContext('(function(exports,require,module){\n'+source+'\n})',e.context,{filename:'pinned/txUtils.js'});
 wrapper(own.exports,key=>key==='bitcore-lib-ltc'?external(key):key==='bignumber.js'?BN:key==='./txEncoder.js'?e.load('txEncoder'):key==='./consensus.js'?e.load('consensus'):key==='./client'?{getInstance(){return Promise.resolve({})}}:(()=>{throw Error('Unmapped TxUtils require '+key)})(),own);
 // This is the pre-existing explicit raw-RPC boundary profile. Original raw
 // transaction documents drive unchanged pure classifier/pubkey methods.
 Object.assign(e.adapters['./txUtils'],{getAddressTypeUniversal:own.exports.getAddressTypeUniversal,extractPubkeyByType:own.exports.extractPubkeyByType,async getRawTransaction(...args){const value=structuredClone(e.table('sourceRpcTransactions').find(row=>row.txid===args[0])?.transaction);e.effects.push({kind:'rpc',method:'TxUtils.getRawTransaction',args,returned:structuredClone(value)});return value;}});
}
async function reset(name,options={}){
 e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally',...options}:options.entry==='volume-first'?{preload:'volumeIndex',...options}:options);lastParams=undefined;unhandled=[];
 if(options.tallyRows)e.table('tallyMap').splice(0,Infinity,...structuredClone(options.tallyRows));
 if(options.rpc){e.table('sourceRpcTransactions').push({txid:'raw4-first-commit',transaction:{vin:[{scriptSig:{asm:'sig '+'02'+'11'.repeat(32)}}],vout:[]}});bindRpc();}
 for(const method of ['sendToken','sendSingle','sendAll','processSend','vestingSend','calculateVestingMovement','roundToEightDecimals','commitToken'])observe(e.logic,method,'Logic.'+method);
 for(const method of ['getInstance','getTally','loadFromDB','updateBalance','updateChannelBalance','getAddressBalances'])observe(e.load('tally'),method,'Tally.'+method);
 observe(e.load('tally').prototype,'getAddressBalances','Tally.instance.getAddressBalances');
 for(const method of ['getChannel','loadChannelsRegistry','recordCommitToChannel','handleChannelPubkey','predictColumnForAddress','assignColumnBasedOnAddress','resolveColumn','recordChannelDelta','saveChannelsRegistry','recordParticipantChange'])observe(e.load('channels'),method,'Channels.'+method);
 await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});
}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function raw(name,type,payload,sender=A,reference=null,failure){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[name,type,'tl',payload,sender,reference,0,0,10];let outcome;try{lastParams=await e.types.decodePayload(...args);let returned;if(lastParams.valid===true)returned=await e.logic.typeSwitch(type,lastParams);outcome={params:lastParams,returned};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
async function call(name,target,args=[],failure,unsupported=false){e.effects.length=0;if(failure)fail(failure);const [ns,method]=target.split('.'),receiver=ns==='Logic'?e.logic:e.load('tally');let transport='Immediate',outcome;try{const value=Reflect.apply(receiver[method],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target,args,failure,unsupported},{transport,outcome});}
async function seed(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{outcome:{returned:undefined}});}
async function bindTally(name){e.effects.length=0;e.context.tallyMap=e.load('tally');emit(name,{bindTally:true},{outcome:{returned:undefined}});}
const balance=(available,vesting=0)=>({amount:available+vesting,available,reserved:0,margin:0,vesting,channelBalance:0}),tallyRows=(available=1000,vesting=0,id=7)=>[{_id:'tallyMap',data:JSON.stringify([[A,{[id]:balance(available,vesting)}]])}];
const amount=v=>(v*1e8).toString(36),send=(v,p=7)=>'0;'+B+';'+p.toString(36)+';'+v;
(async()=>{
 await reset('first10',{rpc:true});
 await raw('raw2-funded-scalar-send',2,send('2'));
 await raw('raw2-decimal-send',2,send(amount(.12345678)+'~'));
 await raw('raw2-negative-send-invalid',2,send('-1'));
 await raw('raw2-insufficient-send-invalid',2,send((2000).toString(36)));
 await raw('raw2-sendall-rejected-before-logic',2,'1;'+B);
 await call('direct-sendall-shadowed-boolean','Logic.sendToken',[true,A,B,7,1,10]);
 await call('direct-multisend-unbound-helper','Logic.sendToken',[false,A,[B],[7],[1],10]);
 await call('direct-sendall-separate-empty-instance','Logic.sendAll',[A,B]);
 await raw('raw4-zero-commit-invalid',4,'7,0,'+B+',0,,,');
 await raw('raw4-first-commit',4,'7,'+amount(2)+','+B+',1,0,,');
if(!flags.includes('--proof10')){
for(const entry of ['types-first','tally-first','volume-first']){
 await reset('send-entry-'+entry,{entry});await raw('send-entry-raw-'+entry,2,send('1'));await call('send-entry-instance-'+entry,'Logic.sendAll',[A,B]);
}
await reset('direct-shapes');
await call('empty-array-send-noop','Logic.sendToken',[false,A,[],[],[],10]);
await call('mismatched-array-send-source-error','Logic.sendToken',[false,A,[B],[7],[1,2],10]);
await call('null-recipient-array-length-source-error','Logic.sendToken',[false,A,null,[7],[1],10]);
await call('null-property-send-noop','Logic.sendToken',[false,A,B,null,1,10]);
await call('default-unbound-process-send','Logic.processSend',[A,B,7,1,10]);
await bindTally('bind-original-static-tally');
await call('bound-static-tally-missing-method','Logic.processSend',[A,B,7,1,10]);
for(const quantity of [0,-0,'0',-2,1001]){
 const label=Object.is(quantity,-0)?'minuszero':String(quantity)+(typeof quantity==='string'?'-string':'');await reset('direct-send-'+label);
 await call('direct-send-outcome-'+label,'Logic.sendSingle',[A,B,7,quantity,10]);
 await call('direct-send-retry-'+label,'Logic.sendSingle',[A,B,7,1,11]);
}
for(const f of [{collection:'tallyMap',op:'getDatabase',skip:0},{collection:'tallyMap',op:'getDatabase',skip:1},{collection:'tallyMap',op:'getDatabase',skip:2},{collection:'tallyMap',op:'findOne',skip:0},{collection:'tallyMap',op:'findOne',skip:1},{collection:'tallyMap',op:'findOne',skip:2},{collection:'tallyMapDelta',op:'insert'},{collection:'tallyMap',op:'update'}]){
 const label=f.collection+'-'+f.op+'-'+(f.skip??0);await reset('send-failure-'+label);await call('send-failure-prefix-'+label,'Logic.sendSingle',[A,B,7,1,10],{...f,message:label});await call('send-failure-retry-'+label,'Logic.sendSingle',[A,B,7,1,11]);
}
for(const [label,rows]of [['absent',[]],['false-data',[{_id:'tallyMap',data:false}]],['malformed',[{_id:'tallyMap',data:'bad'}]]]){
 await reset('send-cache-'+label);await call('preload-'+label,'Tally.getTally',[A,7]);await seed('replace-tally-'+label,'tallyMap',rows);await call('send-cache-prefix-'+label,'Logic.sendSingle',[A,B,7,1,10]);
}
await reset('static-tally-address-balances');await call('static-funded-balances','Tally.getAddressBalances',[A]);await call('static-missing-balances','Tally.getAddressBalances',[B]);
await seed('unknown-property-tally','tallyMap',tallyRows(3,0,999));await call('static-unknown-property-ticker-null-error','Tally.getAddressBalances',[A]);
await reset('vesting-literal-id',{tallyRows:tallyRows(10,5,3)});
await call('vesting-subwhole-rejected','Logic.vestingSend',[A,B,3,.5,10]);
await call('vesting-original-fractional-amount','Logic.sendToken',[false,A,B,3,1.5,10]);
await call('vesting-whole-follow','Logic.vestingSend',[A,B,3,2,11]);
for(const f of [{collection:'tallyMapDelta',op:'insert',skip:1},{collection:'tallyMapDelta',op:'insert',skip:2},{collection:'tallyMap',op:'update',skip:2}]){
 const label=f.collection+'-'+f.op+'-'+f.skip;await reset('vesting-failure-'+label,{tallyRows:tallyRows(10,5,3)});await call('vesting-failure-prefix-'+label,'Logic.vestingSend',[A,B,3,2,10],{...f,message:label});await call('vesting-failure-retry-'+label,'Logic.vestingSend',[A,B,3,2,11]);
}
await reset('pure-vesting-math');
for(const [name,args]of [['ordinary',[3,{available:10,vesting:7}]],['thirds',[1,{available:3,vesting:9}]],['zero-denominator',[1,{available:0,vesting:2}]],['missing-values',[1,{}]]])await call('movement-'+name,'Logic.calculateVestingMovement',args);
for(const [name,value]of [['positive',1.123456789],['negative',-1.123456789],['string','2.000000019']])await call('round-floor-'+name,'Logic.roundToEightDecimals',[value]);
for(const f of [{collection:'tallyMapDelta',op:'insert'},{collection:'tallyMap',op:'update'},{collection:'channelDelta',op:'insert'},{collection:'channels',op:'update'}]){
 const label=f.collection+'-'+f.op;await reset('commit-failure-'+label,{rpc:true});await call('commit-failure-prefix-'+label,'Logic.commitToken',[A,B,7,2,true,[0],10,'raw4-first-commit'],{...f,message:label});await call('commit-failure-retry-'+label,'Logic.commitToken',[A,B,7,2,true,[0],11,'raw4-first-commit']);
}
await reset('commit-bound-missing-rawdoc',{rpc:true});await call('commit-bound-missing-rawdoc-prefix','Logic.commitToken',[A,B,7,2,false,[],10,'original-missing-id']);
await reset('commit-default-unbound');await call('commit-default-unbound-local-refusal','Logic.commitToken',[A,B,7,2,false,[],10,'original-unbound-id'],undefined,true);
}
})().then(()=>{const output=records.map(record=>JSON.stringify(tag(record))).join('\n')+'\n';const destination=path.resolve(__dirname,flags.includes('--proof10')?'../fixtures/lower_send_commit_proof10.jsonl':'../fixtures/lower_send_commit_source.jsonl');if(flags.includes('--stdout')){process.stdout.write(output);return;}if(flags.includes('--check'))assert.equal(fs.readFileSync(destination,'utf8'),output);else fs.writeFileSync(destination,output);console.log(JSON.stringify({observations:records.length,calls:records.filter(r=>r.command.raw||r.command.target).length,version:lock?'9.1.2':'9.3.1',summary:records.map(r=>({name:r.name,result:r.result,tallyInstance:r.state.tallyInstance,channels:[...r.state.channels],unhandled:r.state.unhandled}))},null,2));}).catch(error=>{console.error(error);process.exitCode=1;});
"####;

    fn source(lock: bool) -> Vec<V> {
        let output = std::process::Command::new("node")
            .args(["-e", REFERENCE, "--", "--stdout"])
            .env("SEND_COMMIT_REPO", env!("CARGO_MANIFEST_DIR"))
            .env("TL_SOURCE_BN_VERSION", if lock { "9.1.2" } else { "9.3.1" })
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
            .collect()
    }
    fn thrown(e: &tradelayer_core::validity_source::SourceException) -> V {
        obj(&[("name", s(&e.name)), ("message", s(&e.message))])
    }
    fn result(o: &O, raw: bool, params: V) -> V {
        if raw {
            return obj(&[(
                "outcome",
                match o {
                    O::Source(SourceOutcome::Returned(v)) => v.clone(),
                    O::Source(SourceOutcome::Thrown(e)) => {
                        obj(&[("params", params), ("thrown", thrown(e))])
                    }
                    _ => panic!("raw {o:?}"),
                },
            )]);
        }
        let (transport, outcome) = match o {
            O::Dependency(DependencyReturn::Immediate(v)) => {
                ("Immediate", obj(&[("returned", v.clone())]))
            }
            O::Dependency(DependencyReturn::Pending(v)) => {
                ("Pending", obj(&[("returned", v.clone())]))
            }
            O::Dependency(DependencyReturn::Throw(e)) => ("Throw", obj(&[("thrown", thrown(e))])),
            O::Dependency(DependencyReturn::PendingThrow(e)) => {
                ("PendingThrow", obj(&[("thrown", thrown(e))]))
            }
            _ => panic!("direct {o:?}"),
        };
        obj(&[("transport", s(transport)), ("outcome", outcome)])
    }
    fn genuine_lower_send_commit_cases(lock: bool) {
        let mut replay = Replay::with_policy(P::MainCallableV8).unwrap();
        let (mut resets, mut seeds, mut calls, mut refusals, mut bindings) = (0, 0, 0, 0, 0);
        for case in source(lock) {
            let name = txt(case.get("name"));
            let command = case.get("command");
            if matches!(command.get("reset"), V::Object(_)) {
                resets += 1;
                replay = Replay::with_policy(P::MainCallableV8).unwrap();
                let options = command.get("reset");
                replay
                    .apply(invoke(
                        "SourceMain.configure",
                        vec![obj(&[("activationProfile", s("declared-answer"))])],
                    ))
                    .unwrap();
                if let V::String(entry) = options.get("entry") {
                    replay.apply(C::SetEntry(entry)).unwrap();
                }
                if options.get("inactive") == V::Bool(true) {
                    replay.apply(C::SetActivation(false)).unwrap();
                }
                for table in arr(case.get("state").get("tables")) {
                    replay
                        .apply(C::Import {
                            collection: txt(table.get("name")),
                            rows: arr(table.get("rows")),
                        })
                        .unwrap();
                }
                if options.get("rpc") == V::Bool(true) {
                    replay
                        .apply(invoke("SourceGlobal.bindRpcTransactions", vec![]))
                        .unwrap();
                }
                continue;
            }
            if command.get("bindTally") == V::Bool(true) {
                bindings += 1;
                replay
                    .apply(invoke("SourceGlobal.bindTally", vec![]))
                    .unwrap();
                continue;
            }
            if matches!(command.get("seed"), V::Object(_)) {
                seeds += 1;
                let seed = command.get("seed");
                replay
                    .apply(C::Import {
                        collection: txt(seed.get("collection")),
                        rows: arr(seed.get("rows")),
                    })
                    .unwrap();
                continue;
            }
            if matches!(command.get("failure"), V::Object(_)) {
                let f = command.get("failure");
                let op = txt(f.get("op"));
                replay
                    .apply(C::FailStorage(StorageFailure {
                        collection: txt(f.get("collection")),
                        operation: if op == "getDatabase" {
                            op
                        } else {
                            format!("{op}Async")
                        },
                        skip: if let V::Number(b) = f.get("skip") {
                            f64::from_bits(b) as usize
                        } else {
                            0
                        },
                        message: txt(f.get("message")),
                    }))
                    .unwrap();
            }
            let raw = matches!(command.get("raw"), V::Array(_));
            let before = replay.runtime().state_value().unwrap();
            let receipt = replay
                .apply(if raw {
                    C::ExecuteContract(arr(command.get("raw")))
                } else {
                    invoke(&txt(command.get("target")), arr(command.get("args")))
                })
                .unwrap();
            if command.get("unsupported") == V::Bool(true) {
                assert_eq!(
                    receipt.outcome,
                    O::LocalError(SourceRuleError::UnknownDependency),
                    "{name} explicit unbound RPC domain"
                );
                assert_eq!(
                    replay.runtime().state_value().unwrap(),
                    before,
                    "{name} local admission rollback"
                );
                assert!(receipt.effects.is_empty());
                refusals += 1;
                continue;
            }
            calls += 1;
            assert!(
                !matches!(receipt.outcome, O::LocalError(_)),
                "{name} {:?} effects={:?}",
                receipt.outcome,
                receipt.effects
            );
            let params = replay
                .runtime()
                .last_contract_parameters()
                .cloned()
                .unwrap_or(V::Undefined);
            assert_eq!(
                result(&receipt.outcome, raw, params),
                case.get("result"),
                "{name} exact source outcome"
            );
            let state = replay.runtime().state_value().unwrap();
            let expected = case.get("state");
            for table in arr(expected.get("tables")) {
                assert_eq!(
                    V::Array(
                        replay
                            .runtime()
                            .collection(&txt(table.get("name")))
                            .to_vec()
                    ),
                    table.get("rows"),
                    "{name} persisted {:?}",
                    table.get("name")
                );
            }
            for (actual, wanted) in [
                (state.get("tally"), "tally"),
                (
                    state.get("globalClearing").get("tallyStaticInitialized"),
                    "tallyInitialized",
                ),
                (state.get("globalClearing").get("tallyMod"), "tallyMod"),
                (
                    state.get("globalClearing").get("propertyCache"),
                    "propertyCache",
                ),
                (
                    state.get("proofLifecycle").get("propertyInitialized"),
                    "propertyInitialized",
                ),
                (
                    state.get("globalClearing").get("unhandledRejections"),
                    "unhandled",
                ),
            ] {
                assert_eq!(actual, expected.get(wanted), "{name} live source {wanted}");
            }
            let channels = state.get("globalClearing").get("channels");
            assert_eq!(
                channels.get("registry"),
                expected.get("channels"),
                "{name} actual Channels receiver cache"
            );
            assert_eq!(
                channels.get("pendingWithdrawals"),
                expected.get("withdrawals"),
                "{name} actual withdrawals"
            );
            assert_eq!(
                state.get("sendCommit").get("tallyInstance"),
                expected.get("tallyInstance"),
                "{name} distinct empty instance cache"
            );
            assert_eq!(
                state.get("sendCommit").get("pendingRead"),
                V::Bool(false),
                "{name} declared deferred read drained at public boundary"
            );
            let effects=receipt.effects.iter().filter_map(|e|{
            if matches!(e.get("kind"),V::String(t)if["database","db","uuid","random","rpc-clock","rpc","activation"].contains(&t.as_str())){Some(e.clone())}
            else if e.get("kind")==s("dependency")&&matches!(e.get("target"),V::String(t)if t.starts_with("Logic.")){Some(obj(&[("kind",s("source-function")),("target",e.get("target")),("args",e.get("args"))]))}
            else if e.get("kind")==s("dependency")&&matches!(e.get("target"),V::String(t)if ["Activation.isTxTypeActive","Activation.getAdmin"].contains(&t.as_str())){Some(obj(&[("kind",s("activation")),("method",if e.get("target")==s("Activation.getAdmin"){s("getAdmin")}else{s("isTxTypeActive")}), ("args",e.get("args")),("returned",e.get("returned"))]))}
            else{None}
        }).collect::<Vec<_>>();
            assert_eq!(
                V::Array(effects),
                V::Array(
                    arr(case.get("effects"))
                        .into_iter()
                        .filter(|v| v.get("kind") != s("source-function")
                            || matches!(v.get("target"),V::String(t)if t.starts_with("Logic.")))
                        .collect()
                ),
                "{name} complete ordered DB/environment/observed-source calls"
            );
            let bytes = replay.checkpoint().unwrap();
            assert_eq!(
                Replay::from_checkpoint(&bytes, replay.head()).unwrap(),
                replay,
                "{name} full real replay/cache/failure prefix"
            );
        }
        eprintln!("TLSW3 send_commit source calls={calls} local_refusals={refusals} resets={resets} seeds={seeds} bindings={bindings}");
        assert_eq!(
            (resets, seeds, calls, refusals, bindings),
            (33, 4, 82, 1, 1)
        );
    }

    #[test]
    fn tlsw3_original_send_commit_source_slices() {
        super::COVERAGE.with(|v| v.borrow_mut().clear());
        genuine_lower_send_commit_cases(
            std::env::var("TL_SOURCE_BN_VERSION").as_deref() == Ok("9.1.2"),
        );

        super::COVERAGE.with(|coverage| {
            let coverage = coverage.borrow();
            let observed = coverage
                .iter()
                .filter_map(|(_, _, c)| {
                    if let super::C::ExecuteContract(a) = c {
                        if let super::V::Number(b) = a[1] {
                            Some(f64::from_bits(b) as u8)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<std::collections::BTreeSet<_>>();
            eprintln!(
                "TLSW3 send_commit witnessed={} raw={observed:?}",
                coverage.len()
            );
            assert!(
                [2, 4].iter().all(|kind| observed.contains(kind)),
                "missing genuine source raw kind"
            );
        });
    }
}

#[allow(dead_code)]
mod original_raw3 {
    use super::CheckedReplay as Replay;
    // Unchanged pinned raw3 UTXO-trade ingress over the sole actual Core stores.
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O,
    };
    use tradelayer_core::source_contract_runtime::{
        SourceContractRuntimePolicy as P, StorageFailure,
    };
    use tradelayer_core::validity_source::{
        DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
    };
    fn n(v: f64) -> V {
        V::number(v)
    }
    fn s(v: &str) -> V {
        V::String(v.into())
    }
    fn obj(v: &[(&str, V)]) -> V {
        V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
    }
    fn arr(v: V) -> Vec<V> {
        if let V::Array(v) = v {
            v
        } else {
            panic!("array {v:?}")
        }
    }
    fn txt(v: V) -> String {
        if let V::String(v) = v {
            v
        } else {
            panic!("string {v:?}")
        }
    }
    fn invoke(target: &str, arguments: Vec<V>) -> C {
        C::Invoke(DependencyCall {
            target: target.into(),
            arguments,
        })
    }
    const REFERENCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/lower_utxo_trade_vectors.cjs"
    ));

    fn source(lock: bool) -> Vec<V> {
        let output = std::process::Command::new("node")
            .args([
                "-e",
                REFERENCE.trim_start_matches("#!/usr/bin/env node"),
                "--",
                "--stdout",
            ])
            .env("UTXO_TRADE_REPO", env!("CARGO_MANIFEST_DIR"))
            .env("TL_SOURCE_BN_VERSION", if lock { "9.1.2" } else { "9.3.1" })
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
            .collect()
    }
    fn thrown(e: &tradelayer_core::validity_source::SourceException) -> V {
        obj(&[("name", s(&e.name)), ("message", s(&e.message))])
    }
    fn result(o: &O, raw: bool, params: V) -> V {
        if raw {
            return obj(&[(
                "outcome",
                match o {
                    O::Source(SourceOutcome::Returned(v)) => v.clone(),
                    O::Source(SourceOutcome::Thrown(e)) => {
                        obj(&[("params", params), ("thrown", thrown(e))])
                    }
                    _ => panic!("raw {o:?}"),
                },
            )]);
        }
        let (transport, outcome) = match o {
            O::Dependency(DependencyReturn::Immediate(v)) => {
                ("Immediate", obj(&[("returned", v.clone())]))
            }
            O::Dependency(DependencyReturn::Pending(v)) => {
                ("Pending", obj(&[("returned", v.clone())]))
            }
            O::Dependency(DependencyReturn::Throw(e)) => ("Throw", obj(&[("thrown", thrown(e))])),
            O::Dependency(DependencyReturn::PendingThrow(e)) => {
                ("PendingThrow", obj(&[("thrown", thrown(e))]))
            }
            _ => panic!("direct {o:?}"),
        };
        obj(&[("transport", s(transport)), ("outcome", outcome)])
    }
    fn genuine_lower_utxo_trade_cases(lock: bool) {
        let mut replay = Replay::with_policy(P::MainCallableV8).unwrap();
        let (mut resets, mut seeds, mut calls, mut refusals, mut bindings) = (0, 0, 0, 0, 0);
        for case in source(lock) {
            let name = txt(case.get("name"));
            let command = case.get("command");
            if matches!(command.get("reset"), V::Object(_)) {
                resets += 1;
                replay = Replay::with_policy(P::MainCallableV8).unwrap();
                let options = command.get("reset");
                replay
                    .apply(invoke(
                        "SourceMain.configure",
                        vec![obj(&[("activationProfile", s("source-registry"))])],
                    ))
                    .unwrap();
                if let V::String(entry) = options.get("entry") {
                    replay.apply(C::SetEntry(entry)).unwrap();
                }

                for table in arr(case.get("state").get("tables")) {
                    replay
                        .apply(C::Import {
                            collection: txt(table.get("name")),
                            rows: arr(table.get("rows")),
                        })
                        .unwrap();
                }
                if options.get("rpc") == V::Bool(true) {
                    replay
                        .apply(invoke("SourceGlobal.bindRpcTransactions", vec![]))
                        .unwrap();
                }
                continue;
            }
            if command.get("bindTally") == V::Bool(true) {
                bindings += 1;
                replay
                    .apply(invoke("SourceGlobal.bindTally", vec![]))
                    .unwrap();
                continue;
            }
            if matches!(command.get("seed"), V::Object(_)) {
                seeds += 1;
                let seed = command.get("seed");
                replay
                    .apply(C::Import {
                        collection: txt(seed.get("collection")),
                        rows: arr(seed.get("rows")),
                    })
                    .unwrap();
                continue;
            }
            if matches!(command.get("failure"), V::Object(_)) {
                let f = command.get("failure");
                let op = txt(f.get("op"));
                replay
                    .apply(C::FailStorage(StorageFailure {
                        collection: txt(f.get("collection")),
                        operation: if op == "getDatabase" {
                            op
                        } else {
                            format!("{op}Async")
                        },
                        skip: if let V::Number(b) = f.get("skip") {
                            f64::from_bits(b) as usize
                        } else {
                            0
                        },
                        message: txt(f.get("message")),
                    }))
                    .unwrap();
            }
            let raw = matches!(command.get("raw"), V::Array(_));
            let before = replay.runtime().state_value().unwrap();
            let receipt = replay
                .apply(if raw {
                    C::ExecuteContract(arr(command.get("raw")))
                } else {
                    invoke(&txt(command.get("target")), arr(command.get("args")))
                })
                .unwrap();
            if command.get("unsupported") == V::Bool(true) {
                assert_eq!(
                    receipt.outcome,
                    O::LocalError(SourceRuleError::UnknownDependency),
                    "{name} explicit unbound RPC domain"
                );
                assert_eq!(
                    replay.runtime().state_value().unwrap(),
                    before,
                    "{name} local admission rollback"
                );
                assert!(receipt.effects.is_empty());
                refusals += 1;
                continue;
            }
            calls += 1;
            assert!(
                !matches!(receipt.outcome, O::LocalError(_)),
                "{name} {:?} effects={:?}",
                receipt.outcome,
                receipt.effects
            );
            let params = replay
                .runtime()
                .last_contract_parameters()
                .cloned()
                .unwrap_or(V::Undefined);
            assert_eq!(
                result(&receipt.outcome, raw, params),
                case.get("result"),
                "{name} exact source outcome"
            );
            let state = replay.runtime().state_value().unwrap();
            let expected = case.get("state");
            for table in arr(expected.get("tables")) {
                assert_eq!(
                    V::Array(
                        replay
                            .runtime()
                            .collection(&txt(table.get("name")))
                            .to_vec()
                    ),
                    table.get("rows"),
                    "{name} persisted {:?}",
                    table.get("name")
                );
            }
            for (actual, wanted) in [
                (state.get("tally"), "tally"),
                (
                    state.get("globalClearing").get("tallyStaticInitialized"),
                    "tallyInitialized",
                ),
                (state.get("globalClearing").get("tallyMod"), "tallyMod"),
                (
                    state.get("globalClearing").get("propertyCache"),
                    "propertyCache",
                ),
                (
                    state.get("proofLifecycle").get("propertyInitialized"),
                    "propertyInitialized",
                ),
                (
                    state.get("globalClearing").get("unhandledRejections"),
                    "unhandled",
                ),
            ] {
                assert_eq!(actual, expected.get(wanted), "{name} live source {wanted}");
            }
            assert_eq!(
                state.get("mainCallable").get("activation"),
                expected.get("activation"),
                "{name} real Activation receiver"
            );
            let channels = state.get("globalClearing").get("channels");
            assert_eq!(
                channels.get("registry"),
                expected.get("channels"),
                "{name} actual Channels receiver cache"
            );
            assert_eq!(
                channels.get("pendingWithdrawals"),
                expected.get("withdrawals"),
                "{name} actual withdrawals"
            );
            assert_eq!(
                state.get("sendCommit").get("tallyInstance"),
                expected.get("tallyInstance"),
                "{name} distinct empty instance cache"
            );
            assert_eq!(
                state.get("sendCommit").get("pendingRead"),
                V::Bool(false),
                "{name} declared deferred read drained at public boundary"
            );
            let effects=receipt.effects.iter().filter_map(|e|{
            if matches!(e.get("kind"),V::String(t)if["database","db","uuid","random","rpc-clock","rpc","activation"].contains(&t.as_str())){Some(e.clone())}
            else if e.get("kind")==s("dependency")&&matches!(e.get("target"),V::String(t)if t.starts_with("Logic.")){Some(obj(&[("kind",s("source-function")),("target",e.get("target")),("args",e.get("args"))]))}
            else{None}
        }).collect::<Vec<_>>();
            assert_eq!(
                V::Array(effects),
                V::Array(
                    arr(case.get("effects"))
                        .into_iter()
                        .filter(|v| v.get("kind") != s("source-function")
                            || matches!(v.get("target"),V::String(t)if t.starts_with("Logic.")))
                        .collect()
                ),
                "{name} complete ordered DB/environment/observed-source calls"
            );
            let bytes = replay.checkpoint().unwrap();
            assert_eq!(
                Replay::from_checkpoint(&bytes, replay.head()).unwrap(),
                replay,
                "{name} full real replay/cache/failure prefix"
            );
        }
        eprintln!("TLSW3 raw3 source calls={calls} local_refusals={refusals} resets={resets} seeds={seeds} bindings={bindings}");
        assert_eq!((resets, seeds, calls, refusals, bindings), (8, 0, 34, 0, 0));
    }

    #[test]
    fn tlsw3_original_raw3_source_slices() {
        super::COVERAGE.with(|v| v.borrow_mut().clear());
        genuine_lower_utxo_trade_cases(
            std::env::var("TL_SOURCE_BN_VERSION").as_deref() == Ok("9.1.2"),
        );

        super::COVERAGE.with(|coverage| {
            let coverage = coverage.borrow();
            let observed = coverage
                .iter()
                .filter_map(|(_, _, c)| {
                    if let super::C::ExecuteContract(a) = c {
                        if let super::V::Number(b) = a[1] {
                            Some(f64::from_bits(b) as u8)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<std::collections::BTreeSet<_>>();
            eprintln!("TLSW3 raw3 witnessed={} raw={observed:?}", coverage.len());
            assert!(
                [3].iter().all(|kind| observed.contains(kind)),
                "missing genuine source raw kind"
            );
        });
    }
}

#[allow(dead_code)]
mod original_clearlist {
    use super::CheckedReplay as SourceContractReplay;
    // Literal ClearList/Logic/Types over the same Core database and true Maps.
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O,
    };
    use tradelayer_core::source_contract_runtime::{
        SourceContractRuntime, SourceContractRuntimePolicy as P,
    };
    use tradelayer_core::validity_source::{
        DependencyCall, DependencyReturn, SourceOutcome, SourceValue as V,
    };
    fn s(value: &str) -> V {
        V::String(value.into())
    }
    fn obj(fields: &[(&str, V)]) -> V {
        V::Object(
            fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }
    const REFERENCE: &str = r###"'use strict';
// Audit only: unchanged pinned business modules; real shared in-memory DB calls.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const stage=__dirname,root=process.env.PROOF_REPO||path.resolve(stage,'../../outputs/TradeLayer-Rust'),flags=process.argv.slice(2),lock=flags.includes('--lock');
const numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(lock){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],code=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(code).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const loaded={exports:{}};vm.runInNewContext(code.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'driver anchor '+before);code=code.replace(before,after);}
replace('return {types,logic,Orderbook,effects','return {context,storage,tables,sources,copy,types,logic,Orderbook,effects');replace('level,(...values)=>logs.push({level,values:values.map(value=>typeof value===\'string\'?value:String(value))})','level,(...values)=>{logs.push({level,values:values.map(value=>typeof value===\'string\'?value:String(value))});}');
replace("if(Object.hasOwn(adapters,normalized))return adapters[normalized];","if(normalized==='bitcore-lib-ltc'){if(scene.bitcoinFallback)throw Error('explicit Litecoin module-load failure');return external('bitcore-lib-ltc');} if(normalized==='bitcore-lib')return external('bitcore-lib'); if(Object.hasOwn(adapters,normalized))return adapters[normalized];");
code+='\nmodule.exports={environment};';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',C='LKKG9A9a8n7CeHK8Nk7RdNZiCuHZW2U789',records=[];let e,lastParams,libraryLocked=false;
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toString()];if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),P=e.load('property'),Q=e.load('clearlist');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,propertyCache:P.instance?.propertyIndex??new Map(),propertyInitialized:!!P.instance,clearlist:{clearlistsMap:Q.clearlists,banlist:Q.banlist,xpubCache:Q.xpubCache,merkleXpubMap:Q.merkleXpubMap,libraryLocked,library:flags.includes('--fallback')?'bitcoin-fallback':'litecoin'},lastParams,unsupported:e.state().unsupported};}
function emit(name,command,result){const snapshot=e.copy(state());assert.deepEqual(snapshot.unsupported,[],name+' unowned boundary');records.push({name,command:e.copy(command),result:e.copy(result),state:snapshot,effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','activation'].includes(v.kind)).map(v=>e.copy(v))});}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);target[method]=function(...args){if(label==='ClearList.deriveAddressesFromXpub')libraryLocked=true;e.effects.push({kind:'source-function',target:label,args:e.copy(args)});return Reflect.apply(original,this,args);};}
async function reset(name,options={}){options={...options,...(flags.includes('--fallback')?{bitcoinFallback:true}:{})};e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally',...options}:options.entry==='volume-first'?{preload:'volumeIndex',...options}:options);lastParams=undefined;libraryLocked=false;for(const method of ['createClearList','updateAdmin','issueOrRevokeAttestation','updateBannedCountries'])observe(e.logic,method,'Logic.'+method);for(const method of ['createClearlist','createclearlist','loadClearlists','getClearlistById','getList','verifyAdmin','updateAdmin','getNextId','addAttestation','addAttestationWithXpub','deriveAddressesFromXpub','isAddressInClearlistOrDerived','storeMerkleRoot','registerXpubForRoot','verifyMerkleProof','buildMerkleTree','generateMerkleProof','revokeAttestation','getCountryCodeByAddress','setBanlist','setBanList','getBanlist','getAttestations','getAttestationHistory','isAddressInClearlist'])observe(e.load('clearlist'),method,'ClearList.'+method);await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function call(name,target,args=[],failure,unsupported=false){e.effects.length=0;if(failure)fail(failure);const [module,method]=target.split('.'),receiver=module==='Logic'?e.logic:e.load('clearlist');let transport='Immediate',outcome;try{let value=Reflect.apply(receiver[method],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target,args,failure,...(unsupported?{unsupported:true}:{})},{transport,outcome});}
async function raw(name,type,payload,sender=A,failure,block=10){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[name,type,'tl',payload,sender,null,0,0,block];let outcome;try{lastParams=await e.types.decodePayload(...args);let value;if(lastParams.valid===true)value=await e.logic.typeSwitch(type,lastParams);outcome={params:lastParams,returned:value};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
async function seed(name,collection,rows){e.effects.length=0;e.tables.set(collection,e.copy(rows));emit(name,{import:{collection,rows}},{outcome:{returned:undefined}});}
(async()=>{
await reset('first10-reset');
await raw('raw7-create',7,C+',LIST,url,description');
await raw('raw9-list1-add-address',9,'0,1,'+A+',ca');
await raw('raw9-list0-overwrites-address',9,'0,0,'+A+',gb');
await raw('raw8-whitelist-wrong-validation-collection',8,C+',1,0,0,1,0');
await call('direct-updateAdmin-string-shadow','Logic.updateAdmin',[true,false,false,1,C,false,10]);
await raw('raw9-reload-original-numeric-admin',9,'0,1,'+C+',ca');
await raw('raw9-admin-banlist',9,'0,0,BANLIST,CA|US','admin');
await call('direct-root-attestation','Logic.issueOrRevokeAttestation',[A,1,C,'xpub:invalid',false,11,'root']);
await call('direct-revoke-retains-root','Logic.issueOrRevokeAttestation',[A,1,C,'reason',true,12]);

for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
 await reset('entry-'+entry,{entry});
 await raw(entry+'-create',7,C+',NAME,u,d');
 await raw(entry+'-add',9,'0,1,'+B+',one');
 await raw(entry+'-revoke',9,'1,1,'+B+',reason');
 await raw(entry+'-revoked-again-invalid',9,'1,1,'+B+',reason');
 await raw(entry+'-missing-list',9,'0,99,'+B+',one');
 await raw(entry+'-outsider',9,'0,1,'+B+',one','other');
 await raw(entry+'-self-cert-not-self',9,'0,0,'+B+',ca');
 await raw(entry+'-self-cert-country-shape',9,'0,0,'+A+',CAN');
 await raw(entry+'-banlist-not-admin',9,'0,0,BANLIST,CA|US');
 await raw(entry+'-banlist-empty',9,'0,0,BANLIST,','admin');
 await raw(entry+'-missing-metadata-decoder-throw',9,'0,1,'+B);
 await raw(entry+'-uppercase-metadata-drops-xpub-prefix',9,'0,1,'+B+',xpub:abc');
 await call(entry+'-country-trim','ClearList.getCountryCodeByAddress',[A]);
 await call(entry+'-banlist-array-dot-data-bug','ClearList.getBanlist');
}
await reset('inactive',{inactive:true});await raw('inactive7',7,C+',NAME,u,d');await raw('inactive9',9,'0,0,'+A+',ca');await raw('inactive8',8,C+',1,0,0,1,0');
await reset('admin-stale-cache');
await call('direct-create','ClearList.createClearlist',[A,'L','ignored-url','description',B]);
await call('create-does-not-cache','ClearList.verifyAdmin',[1,A]);
await call('load-created','ClearList.getClearlistById',[1]);
await seed('raw8-validator-row','attestations',[{_id:1,data:{adminAddress:A,backupAddress:B}}]);
await raw('raw8-actual-whitelist-update',8,C+',1,0,0,1,0');
await call('cache-admin-now-C','ClearList.verifyAdmin',[1,C]);
await call('numeric-reload-old','ClearList.getClearlistById',[1]);
await call('string-shadow-new','ClearList.getClearlistById',['1']);
await raw('raw8-backup-actual-update',8,C+',1,0,0,1,1',B);
await call('direct-no-admin','Logic.createClearList',[0,'L']);
await call('direct-no-admin-undefined','Logic.createClearList',[]);
await call('alias-config-create','ClearList.createclearlist',[{adminAddress:'',admin:A,name:'alias',url:'ignored',description:{hello:1},backupAddress:false,backup:B,id:'12tail'}]);
await call('alias-positional-create','ClearList.createclearlist',[A,'alias2','url','d',B]);
await call('numeric-prefix-next','ClearList.getNextId');
await seed('clear-persisted-rows','clearlists',[]);await call('stale-load-keeps-cache','ClearList.loadClearlists');await call('stale-next-id','ClearList.getNextId');
await call('set-backup-truthy-array','ClearList.updateAdmin',[1,'new',[]]);
await call('missing-admin-write','ClearList.updateAdmin',[999,'new',false]);
for(const [name,target,args,collection,op,skip]of[
 ['create-load-factory','Logic.createClearList',[A,'L',null,'d',B,10],'clearlists','getDatabase',0],['create-load-find','Logic.createClearList',[A,'L',null,'d',B,10],'clearlists','find',0],['create-persist-factory','Logic.createClearList',[A,'L',null,'d',B,10],'clearlists','getDatabase',1],['create-update','Logic.createClearList',[A,'L',null,'d',B,10],'clearlists','update',0],
 ['admin-factory','ClearList.updateAdmin',[1,C,false],'clearlists','getDatabase',0],['admin-update','ClearList.updateAdmin',[1,C,false],'clearlists','update',0],
 ['load-factory','ClearList.getClearlistById',[1],'clearlists','getDatabase',0],['load-find','ClearList.getClearlistById',[1],'clearlists','find',0],
 ['attest-factory','Logic.issueOrRevokeAttestation',[A,1,C,'ca',false,10,'root'],'attestations','getDatabase',0],['attest-update','Logic.issueOrRevokeAttestation',[A,1,C,'ca',false,10,'root'],'attestations','update',0],['root-after-attestation','Logic.issueOrRevokeAttestation',[A,1,C,'ca',false,10,'root'],'attestations','update',1],
 ['revoke-read','ClearList.revokeAttestation',[1,C,'reason',11],'attestations','find',0],['revoke-write','ClearList.revokeAttestation',[1,C,'reason',11],'attestations','update',0],
 ['country-catches','ClearList.getCountryCodeByAddress',[C],'attestations','find',0],['banlist-rethrow','ClearList.setBanlist',[['CA'],10],'clearlists','update',0]
]){await reset('failure-'+name);if(name.startsWith('admin-')||name.startsWith('load-')){await call(name+'-create','ClearList.createClearlist',[A,'L',null,'d',B]);await call(name+'-hydrate','ClearList.loadClearlists');}if(name.startsWith('revoke-'))await call(name+'-attest','ClearList.addAttestation',[1,C,'meta',10]);await call(name+'-failed',target,args,{collection,op,skip,message:name});await call(name+'-retry',target,args);}
await reset('revocation-legacy-multi');await seed('legacy-attestation-rows','attestations',[{_id:'one',data:{address:C,listId:1,status:'active',timestamp:1}},{_id:'two',data:{address:C,clearlistId:1,status:'active',timestamp:1}},{_id:'attestation:'+B,data:{address:B,status:'active'}}]);
await call('revoke-partial-first-write','ClearList.revokeAttestation',[1,C,'why',20],{collection:'attestations',op:'update',skip:1,message:'second-revoke'});await call('revoke-only-remaining','ClearList.revokeAttestation',[1,C,'retry',21]);await call('revoke-legacy-id','ClearList.revokeAttestation',[99,B,'legacy',22]);await call('revoke-missing','ClearList.revokeAttestation',[1,'absent','why',23]);await call('all-list-attestations','ClearList.getAttestations',[1]);await call('ordered-history','ClearList.getAttestationHistory',[C,1]);
await reset('country-banlist');await seed('country-shapes','attestations',[{_id:'country1',data:{address:C,listId:0,status:'revoked',data:'us'}},{_id:'country2',data:{address:C,clearlistId:'0',data:{country:' gb '},timestamp:0,blockHeight:12}},{_id:'country3',data:{address:B,listId:null,data:' CA '}}]);
await call('legacy-country-object','ClearList.getCountryCodeByAddress',[C]);await call('null-list-is-Number-zero','ClearList.getCountryCodeByAddress',[B]);await call('set-alias-banlist','ClearList.setBanList',[['CA','GB'],30]);await call('get-banlist-literal-array-bug','ClearList.getBanlist');await call('direct-invalid-bancodes-return-console-undefined','Logic.updateBannedCountries',[['abc'],31]);await call('direct-normalize-banlist','Logic.issueOrRevokeAttestation',['admin',0,' banlist ','us;ca |gb',false,32]);await call('truthy-nonboolean-revoke-no-op','Logic.issueOrRevokeAttestation',[A,1,C,'meta','truthy',33]);
await seed('country-revoked-malformed-id-before-valid','attestations',[{_id:'bad-revoked',data:{address:C,status:'revoked',listId:{toString:1},data:'US'}},{_id:'later-active',data:{address:C,status:'active',listId:0,data:'CA'}}]);
await call('country-number-before-revoked-status-caught','ClearList.getCountryCodeByAddress',[C]);
assert.equal(records.at(-1).result.outcome.returned,null,'Number conversion must execute before revoked status guard');
await seed('country-after-malformed-row-removal','attestations',[{_id:'later-active',data:{address:C,status:'active',listId:0,data:'CA'}}]);
await call('country-valid-after-malformed-row-removal','ClearList.getCountryCodeByAddress',[C]);
assert.equal(records.at(-1).result.outcome.returned.countryCode,'CA');
const XPUB='xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8';
await reset('xpub-roots');
await call('derive-actual-library','ClearList.deriveAddressesFromXpub',[XPUB,'ignored-network']);
const addresses=e.load('clearlist').xpubCache.get(XPUB);
await call('derive-cache-reuse-network-ignored','ClearList.deriveAddressesFromXpub',[XPUB,'testnet']);await call('derive-invalid-empty-no-cache','ClearList.deriveAddressesFromXpub',['bad']);await call('direct-xpub-prefix','ClearList.addAttestation',[1,'issuer','xpub:'+XPUB,40]);await call('inline-derived-first','ClearList.isAddressInClearlistOrDerived',[1,addresses[0]]);await call('inline-derived-last','ClearList.isAddressInClearlistOrDerived',[1,addresses[39]]);await call('inline-derived-absent','ClearList.isAddressInClearlistOrDerived',[2,addresses[39]]);
await call('explicit-xpub-row','ClearList.addAttestationWithXpub',[2,'issuer','meta',XPUB,41]);await call('explicit-derived','ClearList.isAddressInClearlistOrDerived',[2,addresses[2]]);
await call('build-single-tree','ClearList.buildMerkleTree',[[XPUB]]);const tree=e.load('clearlist').buildMerkleTree([XPUB]),proof=e.load('clearlist').generateMerkleProof([XPUB],XPUB);await call('generate-single-proof','ClearList.generateMerkleProof',[[XPUB],XPUB]);await call('verify-single-proof','ClearList.verifyMerkleProof',[XPUB,proof,tree.root]);await call('register-absent-root','ClearList.registerXpubForRoot',[tree.root,XPUB,proof]);await call('store-root','ClearList.storeMerkleRoot',[3,tree.root,'root-admin',42]);await call('register-verified-xpub','ClearList.registerXpubForRoot',[tree.root,XPUB,proof]);await call('register-duplicate-xpub','ClearList.registerXpubForRoot',[tree.root,XPUB,proof]);await call('root-derived-address','ClearList.isAddressInClearlistOrDerived',[3,addresses[4]]);await call('register-invalid-proof','ClearList.registerXpubForRoot',[tree.root,'other',proof]);await call('build-empty-tree','ClearList.buildMerkleTree',[[]]);await call('generate-absent-proof','ClearList.generateMerkleProof',[[XPUB],'other']);await call('three-leaf-tree','ClearList.buildMerkleTree',[['one','two','three']]);await call('three-leaf-proof','ClearList.generateMerkleProof',[['one','two','three'],'three']);


await reset('coercion-source-guards');
for(const [name,target,args]of[
 ['direct-target-own-toString','Logic.issueOrRevokeAttestation',[A,1,{toString:1},'meta',false,10]],
 ['direct-target-array-nested-toString','Logic.issueOrRevokeAttestation',[A,1,[{toString:null}],'meta',false,10]],
 ['country-thrown-number-caught','ClearList.getCountryCodeByAddress',[C]],
 ['create-bad-numeric-cache-data','ClearList.createClearlist',[A,'L',null,'d',B,1]]
])await call(name,target,args);
await seed('primitive-cache-data','clearlists',[{_id:1,data:2}]);await call('load-primitive-cache-data','ClearList.loadClearlists');await call('primitive-cache-admin-typeerror','ClearList.updateAdmin',[1,C,false]);
await call('banlist-JS-whitespace-only','Logic.issueOrRevokeAttestation',['admin',0,'BANLIST',String.fromCharCode(0x85)+'US'+String.fromCharCode(0x85),false,10]);

await reset('explicit-identity-domain');
await call('direct-target-plain-object-local-refusal','Logic.issueOrRevokeAttestation',[A,1,{},'meta',false,10],undefined,true);
for(const record of records)console.log(JSON.stringify(tag(record)));
})().catch(error=>{console.error(error.stack||error);process.exitCode=1});
"###;
    fn text(value: V) -> String {
        let V::String(value) = value else {
            panic!("string {value:?}")
        };
        value
    }
    fn array(value: V) -> Vec<V> {
        let V::Array(value) = value else {
            panic!("array {value:?}")
        };
        value
    }
    fn cases(lock: bool, fallback: bool) -> Vec<V> {
        let output = std::process::Command::new("node")
            .env("PROOF_REPO", env!("CARGO_MANIFEST_DIR"))
            .args([
                "-e",
                REFERENCE,
                "clearlist-reference",
                if lock { "--lock" } else { "--baseline" },
                if fallback { "--fallback" } else { "--litecoin" },
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
            .map(|s| tradelayer_core::property_catalog::parse_diagnostic_value(s).unwrap())
            .collect()
    }
    fn returned(value: DependencyReturn) -> V {
        let (transport, result) = match value {
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
        obj(&[("transport", s(transport)), ("outcome", result)])
    }
    fn failure(c: &V) -> Option<tradelayer_core::source_contract_runtime::StorageFailure> {
        let f = c.get("failure");
        if f == V::Undefined {
            return None;
        }
        let op = text(f.get("op"));
        Some(tradelayer_core::source_contract_runtime::StorageFailure {
            collection: text(f.get("collection")),
            operation: if op == "getDatabase" {
                op
            } else {
                format!("{op}Async")
            },
            skip: if let V::Number(v) = f.get("skip") {
                f64::from_bits(v) as usize
            } else {
                0
            },
            message: text(f.get("message")),
        })
    }
    fn command(c: &V) -> C {
        if let V::Object(_) = c.get("import") {
            let i = c.get("import");
            return C::Import {
                collection: text(i.get("collection")),
                rows: array(i.get("rows")),
            };
        }
        if let V::Array(args) = c.get("raw") {
            C::ExecuteContract(args)
        } else {
            C::Invoke(DependencyCall {
                target: text(c.get("target")),
                arguments: array(c.get("args")),
            })
        }
    }
    fn actual(runtime: &mut SourceContractRuntime, case: &V) -> V {
        let c = case.get("command");
        if let V::Object(_) = c.get("import") {
            let i = c.get("import");
            runtime
                .import_collection(&text(i.get("collection")), array(i.get("rows")))
                .unwrap();
            return obj(&[("outcome", obj(&[("returned", V::Undefined)]))]);
        }
        if let Some(f) = failure(&c) {
            runtime.fail_storage(f).unwrap();
        }
        if let V::Array(args) = c.get("raw") {
            let value = match runtime.execute_contract_unverified(&args).unwrap() {
                SourceOutcome::Returned(v) => v,
                SourceOutcome::Thrown(e) => obj(&[
                    (
                        "params",
                        runtime
                            .last_contract_parameters()
                            .cloned()
                            .unwrap_or(V::Undefined),
                    ),
                    (
                        "thrown",
                        obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                    ),
                ]),
            };
            obj(&[("outcome", value)])
        } else {
            returned(
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: text(c.get("target")),
                        arguments: array(c.get("args")),
                    })
                    .unwrap(),
            )
        }
    }
    fn assert_state(runtime: &SourceContractRuntime, case: &V) {
        let expected = case.get("state");
        let name = text(case.get("name"));
        let mut known = std::collections::BTreeSet::new();
        for table in array(expected.get("tables")) {
            let collection = text(table.get("name"));
            known.insert(collection.clone());
            assert_eq!(
                V::Array(runtime.collection(&collection).to_vec()),
                table.get("rows"),
                "{name} complete {collection}"
            );
        }
        let state = runtime.state_value().unwrap();
        for table in array(state.get("tables")) {
            if !known.contains(&text(table.get("name"))) {
                assert_eq!(
                    table.get("rows"),
                    V::Array(vec![]),
                    "{name} no invented rows"
                );
            }
        }
        assert_eq!(state.get("runtimePolicy"), s("MainCallableV8"));
        assert_eq!(
            state.get("clearlistLifecycle"),
            expected.get("clearlist"),
            "{name} four genuine Maps/banlist and declared library"
        );
        for (key, wanted) in [
            ("tallyStaticInitialized", "tallyInitialized"),
            ("tallyMod", "tallyMod"),
            ("propertyCache", "propertyCache"),
        ] {
            assert_eq!(
                state.get("globalClearing").get(key),
                expected.get(wanted),
                "{name} inherited {key}"
            );
        }
        assert_eq!(
            state.get("proofLifecycle").get("propertyInitialized"),
            expected.get("propertyInitialized"),
            "{name} sole Property"
        );
        assert_eq!(
            runtime.tally_cache(),
            expected.get("tally"),
            "{name} sole Tally"
        );
        assert_eq!(
            runtime
                .last_contract_parameters()
                .cloned()
                .unwrap_or(V::Undefined),
            expected.get("lastParams"),
            "{name} actual raw fields"
        );
    }
    fn effects(values: Vec<V>) -> V {
        V::Array(values.into_iter().filter_map(|v|{
 if matches!(v.get("kind"),V::String(k)if["database","db","uuid","random","rpc-clock"].contains(&k.as_str())){Some(v)}
 else if v.get("kind")==s("dependency")&&matches!(v.get("target"),V::String(t)if t.starts_with("ClearList.")||["Logic.createClearList","Logic.updateAdmin","Logic.issueOrRevokeAttestation","Logic.updateBannedCountries"].contains(&t.as_str())){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}
 else if v.get("kind")==s("dependency")&&v.get("target")==s("Activation.isTxTypeActive"){Some(obj(&[("kind",s("activation")),("method",s("isTxTypeActive")),("args",v.get("args")),("returned",v.get("returned"))]))}
 else if v.get("kind")==s("dependency")&&v.get("target")==s("Activation.getAdmin"){Some(obj(&[("kind",s("activation")),("method",s("getAdmin")),("args",v.get("args")),("returned",v.get("returned"))]))}else{None}}).collect())
    }
    fn setup(runtime: &mut SourceContractRuntime, case: &V) {
        let options = case.get("command").get("reset");
        assert_eq!(
            runtime
                .invoke_dependency(&DependencyCall {
                    target: "SourceMain.configure".into(),
                    arguments: vec![obj(&[("activationProfile", s("declared-answer"))])]
                })
                .unwrap(),
            DependencyReturn::Immediate(V::Undefined)
        );
        runtime.take_effects();
        runtime
            .set_entry(&if options.get("entry") == V::Undefined {
                "main-subgraph".into()
            } else {
                text(options.get("entry"))
            })
            .unwrap();
        runtime.set_activation_answer(options.get("inactive") != V::Bool(true));
        if options.get("bitcoinFallback") == V::Bool(true) {
            assert_eq!(
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: "SourceClearList.configureLibrary".into(),
                        arguments: vec![s("bitcoin-fallback")]
                    })
                    .unwrap(),
                DependencyReturn::Immediate(V::Undefined)
            );
            runtime.take_effects();
        }
        for table in array(case.get("state").get("tables")) {
            runtime
                .import_collection(&text(table.get("name")), array(table.get("rows")))
                .unwrap();
        }
    }
    #[test]
    fn tlsw3_original_clearlist_raw_logic_and_bound_library_scenes() {
        super::COVERAGE.with(|v| v.borrow_mut().clear());
        for fallback in [false, true] {
            let records = cases(
                std::env::var("TL_SOURCE_BN_VERSION").as_deref() == Ok("9.1.2"),
                fallback,
            );
            let (mut source_calls, mut input_frames) = (0, 0);
            let mut count = 0;
            for start in records.iter().enumerate().filter_map(|(i, r)| {
                let name = text(r.get("name"));
                (r.get("command").get("reset") != V::Undefined
                    && (name.starts_with("failure-")
                        || [
                            "first10-reset",
                            "admin-stale-cache",
                            "revocation-legacy-multi",
                            "country-banlist",
                            "xpub-roots",
                        ]
                        .contains(&name.as_str())))
                .then_some(i)
            }) {
                count += 1;
                let mut replay = SourceContractReplay::with_policy(P::MainCallableV8).unwrap();
                replay
                    .apply(C::Invoke(DependencyCall {
                        target: "SourceMain.configure".into(),
                        arguments: vec![obj(&[("activationProfile", s("declared-answer"))])],
                    }))
                    .unwrap();
                if fallback {
                    replay
                        .apply(C::Invoke(DependencyCall {
                            target: "SourceClearList.configureLibrary".into(),
                            arguments: vec![s("bitcoin-fallback")],
                        }))
                        .unwrap();
                }
                for table in array(records[start].get("state").get("tables")) {
                    replay
                        .apply(C::Import {
                            collection: text(table.get("name")),
                            rows: array(table.get("rows")),
                        })
                        .unwrap();
                }
                for case in records[start + 1..]
                    .iter()
                    .take_while(|r| r.get("command").get("reset") == V::Undefined)
                {
                    let c = case.get("command");
                    if let Some(f) = failure(&c) {
                        replay.apply(C::FailStorage(f)).unwrap();
                    }
                    let receipt = replay.apply(command(&c)).unwrap();
                    assert!(
                        !matches!(receipt.outcome, O::LocalError(_)),
                        "genuine source receipt"
                    );
                    if c.get("import") == V::Undefined {
                        source_calls += 1;
                    } else {
                        input_frames += 1;
                    }
                    let observed = match &receipt.outcome {
                        O::Source(SourceOutcome::Returned(value)) => {
                            obj(&[("outcome", value.clone())])
                        }
                        O::Source(SourceOutcome::Thrown(error)) => obj(&[(
                            "outcome",
                            obj(&[
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
                                    obj(&[
                                        ("name", s(&error.name)),
                                        ("message", s(&error.message)),
                                    ]),
                                ),
                            ]),
                        )]),
                        O::Dependency(value) => returned(value.clone()),
                        O::Void => obj(&[("outcome", obj(&[("returned", V::Undefined)]))]),
                        value => panic!("unavailable source witness {value:?}"),
                    };
                    assert_eq!(
                        observed,
                        case.get("result"),
                        "TLSW3 original ClearList outcome"
                    );
                    assert_eq!(
                        effects(receipt.effects.clone()),
                        case.get("effects"),
                        "TLSW3 ordered original ClearList effects"
                    );
                    assert_state(replay.runtime(), case);
                    let name = text(case.get("name"));
                    if !text(records[start].get("name")).starts_with("xpub-")
                        || [
                            "derive-actual-library",
                            "store-root",
                            "register-verified-xpub",
                            "root-derived-address",
                            "three-leaf-proof",
                        ]
                        .contains(&name.as_str())
                    {
                        let bytes = replay.checkpoint().unwrap();
                        assert_eq!(&bytes[..5], b"TLCR\x08");
                        let restored =
                            SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
                        assert_eq!(restored, replay);
                        assert_eq!(restored.checkpoint().unwrap(), bytes);
                        replay = restored;
                    }
                }
            }
            eprintln!("TLSW3 clearlist fallback={fallback} source_calls={source_calls} input_frames={input_frames} resets={count} local_refusals=0 (unsupported object-key scene excluded)");
            assert_eq!(count, 20);
        }

        super::COVERAGE.with(|coverage| {
            let coverage = coverage.borrow();
            let observed = coverage
                .iter()
                .filter_map(|(_, _, c)| {
                    if let super::C::ExecuteContract(a) = c {
                        if let super::V::Number(b) = a[1] {
                            Some(f64::from_bits(b) as u8)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<std::collections::BTreeSet<_>>();
            eprintln!(
                "TLSW3 clearlist witnessed={} raw={observed:?}",
                coverage.len()
            );
            assert!(
                [7, 9].iter().all(|kind| observed.contains(kind)),
                "missing genuine source raw kind"
            );
        });
    }
}
