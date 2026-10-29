#[path = "support/source_number_adapter.rs"]
mod numbers;
pub use tradelayer_core::source_rich_value as value;
use value::thrown::*;
use value::*;
fn units(s: &str) -> Units {
    s.encode_utf16().collect()
}
fn bytes(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn renderer(bits: u64) -> Eval<Units> {
    let n = f64::from_bits(bits);
    if n.is_nan() {
        return Ok(units("NaN"));
    }
    if n == f64::INFINITY {
        return Ok(units("Infinity"));
    }
    if n == f64::NEG_INFINITY {
        return Ok(units("-Infinity"));
    }
    numbers::source_number_text(n)
        .map(|v| units(&v))
        .map_err(|_| Abrupt::Local(Error::Capacity))
}
struct Context {
    base: intrinsics::LiteralIntrinsics,
    facts: Atom,
    counts: Vec<(String, u64)>,
    logs: Vec<(String, Vec<Atom>)>,
    observed: Vec<(String, u64, Vec<u8>)>,
    exposure: value::exposure::Exposure,
}
impl RichThrowContext for Context {
    fn epoch(&self) -> u64 {
        self.base.epoch()
    }
    fn source_realm(&self) -> Realm {
        self.base.source_realm()
    }
    fn replace_realm(&mut self, realm: Realm) -> Realm {
        self.base.replace_realm(realm)
    }
    fn construct_native_error(
        &mut self,
        arena: &mut Arena,
        kind: NativeKind,
        realm: Realm,
        message: Option<Units>,
    ) -> Eval<RichThrownRef> {
        self.base
            .construct_native_error(arena, kind, realm, message)
    }
    fn intrinsic_property(
        &mut self,
        arena: &mut Arena,
        intrinsic: &Intrinsic,
        key: &Key,
    ) -> Eval<Option<PropertyValue>> {
        self.base.intrinsic_property(arena, intrinsic, key)
    }
    fn intrinsic_well_known_property(
        &mut self,
        arena: &mut Arena,
        intrinsic: &Intrinsic,
        name: &[u16],
    ) -> Eval<Option<PropertyValue>> {
        self.base
            .intrinsic_well_known_property(arena, intrinsic, name)
    }
    fn intrinsic_prototype(&mut self, intrinsic: &Intrinsic) -> Eval<Atom> {
        self.base.intrinsic_prototype(intrinsic)
    }
    fn owner_well_known_property(
        &mut self,
        arena: &mut Arena,
        owner: &OwnerRef,
        name: &[u16],
    ) -> Eval<Option<PropertyValue>> {
        self.base.owner_well_known_property(arena, owner, name)
    }
    fn owner_prototype(&mut self, owner: &OwnerRef) -> Eval<Atom> {
        self.base.owner_prototype(owner)
    }
    fn number_to_string(&mut self, bits: u64) -> Eval<Units> {
        self.base.number_to_string(bits)
    }
    fn owner_property(
        &mut self,
        arena: &mut Arena,
        owner: &OwnerRef,
        key: &Key,
    ) -> Eval<Option<PropertyValue>> {
        self.base.owner_property(arena, owner, key)
    }
    fn call_pinned(
        &mut self,
        arena: &mut Arena,
        function: &Atom,
        receiver: &Atom,
        args: &[Atom],
        depth: usize,
    ) -> Eval<Atom> {
        self.base
            .call_pinned(arena, function, receiver, args, depth)
    }
    fn is_callable(&self, arena: &Arena, value: &Atom) -> bool {
        self.base.is_callable(arena, value)
    }
}
impl value::main::MainContext for Context {
    fn rpc(&mut self, arena: &mut Arena, method: &str, _args: &[Atom]) -> Eval<Atom> {
        let ordinal = if let Some((_, n)) = self.counts.iter_mut().find(|(k, _)| k == method) {
            *n += 1;
            *n
        } else {
            self.counts.push((method.into(), 1));
            1
        };
        let failure = value::io::select_failure(arena, self, self.facts.clone(), method, ordinal)?;
        let (thrown, value) = if let Some(failure) = failure {
            (true, value::io::rejection(arena, self, failure)?)
        } else {
            let field = match method {
                "Client.getBlockCount" => "height",
                "Client.getNetworkInfo" => "networkInfo",
                _ => return Err(Error::UnknownDependency.into()),
            };
            (
                false,
                get_property(
                    arena,
                    self,
                    self.facts.clone(),
                    &Key::Utf16(units(field)),
                    0,
                )?,
            )
        };
        let snapshot = self.exposure.expose(arena, std::slice::from_ref(&value))?;
        self.observed
            .push((method.into(), ordinal, wire::encode(&snapshot)?));
        if thrown {
            Err(Abrupt::Source(RichThrownRef {
                epoch: self.epoch(),
                value,
            }))
        } else {
            value::io::clone_plain_data(arena, self, value)
        }
    }

    fn index_rpc(&mut self, arena: &mut Arena, method: &str, args: &[Atom]) -> Eval<Atom> {
        self.rpc(arena, &format!("Client.{method}"), args)
    }
    fn log(&mut self, _arena: &mut Arena, level: &str, args: &[Atom]) -> Eval<()> {
        self.logs.push((level.into(), args.to_vec()));
        Ok(())
    }
}
fn capabilities() -> Capabilities {
    let mut c = intrinsics::capabilities();
    for line in include_str!("../fixtures/rich_main_v14_capabilities.tsv").lines() {
        let f = line.splitn(3, '\t').collect::<Vec<_>>();
        match f[0] {
            "I" => {
                c.intrinsics.insert(Intrinsic {
                    realm: match f[1] {
                        "0" => Realm::HostIo,
                        "1" => Realm::SourceVm,
                        _ => Realm::Shared,
                    },
                    name: units(f[2]),
                });
            }
            "C" => {
                c.callables.insert(units(f[1]));
            }
            _ => panic!(),
        }
    }
    c
}
#[test]
fn actual_original_main_rpc_carrier_methods_match_outcome_graphs_io_prefixes_and_counts() {
    let caps = capabilities();
    let mut count = 0;
    for line in include_str!("../fixtures/rich_main_v14.tsv").lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let mut arena = Arena::new(caps.clone(), Limits::default());
        let input = wire::decode_external(&bytes(f[4]), &caps, Limits::default()).unwrap();
        let root = arena.import(input).unwrap().roots[0].clone();
        let mut context = Context {
            base: intrinsics::LiteralIntrinsics::new(42, renderer),
            facts: root,
            counts: vec![],
            logs: vec![],
            observed: vec![],
            exposure: value::exposure::Exposure::default(),
        };
        context
            .exposure
            .expose(&arena, std::slice::from_ref(&context.facts))
            .unwrap();
        let result = match f[1] {
            "Main.getCurrentBlockHeight" => {
                value::main::current_block_height(&mut arena, &mut context)
            }
            "Main.checkNetworkStatus" => {
                value::main::check_network_status(&mut arena, &mut context)
            }
            "TxIndex.fetchChainTip" => value::main::fetch_chain_tip(&mut arena, &mut context),
            _ => panic!(),
        };
        let (transport, value) = match result {
            Ok(v) => ("Pending", v),
            Err(Abrupt::Source(e)) => {
                assert_eq!(e.epoch, 42);
                ("PendingThrow", e.value)
            }
            Err(e) => panic!("{} unexpected local refusal {e:?}", f[0]),
        };
        assert_eq!(transport, f[2], "{} transport", f[0]);
        assert_eq!(context.logs.len().to_string(), f[3], "{} log calls", f[0]);
        let actual = wire::encode(&context.exposure.expose(&arena, &[value]).unwrap()).unwrap();
        assert_eq!(actual, bytes(f[5]), "{} complete actual result graph", f[0]);
        assert_eq!(
            context
                .counts
                .iter()
                .map(|(k, n)| format!("{k}={n}"))
                .collect::<Vec<_>>()
                .join(","),
            f[6],
            "{} actual IO counters",
            f[0]
        );
        assert_eq!(
            context.observed,
            vec![(f[7].into(), f[8].parse().unwrap(), bytes(f[9]))],
            "{} complete causal IO graph",
            f[0]
        );
        count += 1;
    }
    assert_eq!(count, 46);
}
