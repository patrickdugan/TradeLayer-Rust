// Work-only explicit RichThrownV14 integration. One existing Core owns all
// financial/index data; rich heap holds only raw language/IO values.
mod rich_runtime_v14 {
    use super::*;
    use crate::source_rich_value as rich;
    use rich::thrown::{Abrupt, Eval, RichThrowContext, RichThrownRef};
    use rich::{
        Arena, Atom, Graph, Intrinsic, Key, NativeKind, OwnerRef, PropertyValue, Realm, Units,
    };
    const SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(super) struct State {
        arena: Arena,
        exposure: rich::exposure::Exposure,
        io: Option<Atom>,
        snapshots: Vec<Vec<u8>>,
        snapshot_bytes: usize,
        epoch: u64,
    }
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
    fn rule(error: rich::Error) -> SourceRuleError {
        match error {
            rich::Error::Capacity => SourceRuleError::Capacity,
            rich::Error::Backend => SourceRuleError::Backend,
            rich::Error::UnknownDependency | rich::Error::UndeclaredCapability => {
                SourceRuleError::UnknownDependency
            }
            rich::Error::UnsupportedNumericDomain => SourceRuleError::UnsupportedNumericDomain,
            _ => SourceRuleError::InvalidInput,
        }
    }
    fn number(bits: u64) -> Eval<Units> {
        let value = f64::from_bits(bits);
        let text = if value.is_nan() {
            "NaN".into()
        } else if value == f64::INFINITY {
            "Infinity".into()
        } else if value == f64::NEG_INFINITY {
            "-Infinity".into()
        } else {
            crate::state_oracle::source_number_text(value)
                .map_err(|_| rich::Error::UnsupportedNumericDomain)?
        };
        Ok(text.encode_utf16().collect())
    }
    impl State {
        pub(super) fn new() -> Self {
            Self {
                arena: Arena::new(rich::intrinsics::capabilities(), rich::Limits::default()),
                exposure: rich::exposure::Exposure::default(),
                io: None,
                snapshots: vec![],
                snapshot_bytes: 0,
                epoch: 1,
            }
        }
        fn material(&self) -> Result<Vec<u8>, SourceRuleError> {
            let mut roots = vec![self.io.clone().unwrap_or(Atom::Undefined)];
            roots.extend(
                self.arena
                    .imports()
                    .iter()
                    .flat_map(|v| v.roots.iter().cloned()),
            );
            let graph = Graph {
                roots,
                objects: self.arena.objects().to_vec(),
                symbols: self.arena.symbols().to_vec(),
            };
            let frame = rich::wire::encode(&graph).map_err(rule)?;
            let mut out = b"TradeLayer/SourceRichState/v14/".to_vec();
            out.extend(self.epoch.to_be_bytes());
            out.extend((self.arena.imports().len() as u32).to_be_bytes());
            out.extend((frame.len() as u32).to_be_bytes());
            out.extend(frame);
            for ids in [
                self.exposure.object_bindings(),
                self.exposure.symbol_bindings(),
            ] {
                out.extend((ids.len() as u32).to_be_bytes());
                for id in ids {
                    out.extend(id.to_be_bytes());
                }
            }
            out.extend((self.snapshots.len() as u32).to_be_bytes());
            for snapshot in &self.snapshots {
                out.extend((snapshot.len() as u32).to_be_bytes());
                out.extend(snapshot);
            }
            if out.len() > 24 * 1024 * 1024 {
                return Err(SourceRuleError::Capacity);
            }
            Ok(out)
        }
        fn value(&self) -> Result<V, SourceRuleError> {
            let material = self.material()?;
            Ok(obj(&[
                ("profile", s("source-rich-main-rpc-v1")),
                ("activationProfile", s("source-registry")),
                ("epoch", s(self.epoch.to_string())),
                ("materialSha256", s(hex(&crate::hash256::sha256(&material)))),
                ("materialBytes", n(material.len() as f64)),
                ("snapshots", n(self.snapshots.len() as f64)),
                ("snapshotBytes", n(self.snapshot_bytes as f64)),
                ("inputNamespaces", n(self.arena.imports().len() as f64)),
            ]))
        }
    }
    struct Context<'a> {
        core: &'a mut ContractCore,
        base: rich::intrinsics::LiteralIntrinsics,
        facts: Atom,
        exposure: &'a mut rich::exposure::Exposure,
        snapshots: &'a mut Vec<Vec<u8>>,
        snapshot_bytes: &'a mut usize,
    }
    impl Context<'_> {
        fn snapshot(&mut self, arena: &Arena, roots: &[Atom]) -> Eval<(usize, String)> {
            let graph = self.exposure.expose(arena, roots)?;
            let bytes = rich::wire::encode(&graph)?;
            let total = self
                .snapshot_bytes
                .checked_add(bytes.len())
                .ok_or(rich::Error::Capacity)?;
            if total > SNAPSHOT_BYTES || self.snapshots.len() >= 4096 {
                return Err(rich::Error::Capacity.into());
            }
            let index = self.snapshots.len();
            let hash = hex(&crate::hash256::sha256(&bytes));
            self.snapshots.push(bytes);
            *self.snapshot_bytes = total;
            Ok((index, hash))
        }
    }

    impl RichThrowContext for Context<'_> {
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
    impl rich::main::MainContext for Context<'_> {
        fn rpc(&mut self, arena: &mut Arena, method: &str, args: &[Atom]) -> Eval<Atom> {
            let owner = self
                .core
                .main_production
                .as_mut()
                .ok_or(rich::Error::UnknownDependency)?;
            let ordinal = if let Some((_, n)) = owner.counts.iter_mut().find(|(k, _)| k == method) {
                *n = n.checked_add(1).ok_or(rich::Error::Capacity)?;
                *n
            } else {
                if owner.counts.len() >= 16 {
                    return Err(rich::Error::Capacity.into());
                }
                owner.counts.push((method.into(), 1));
                1
            };
            let failure =
                rich::io::select_failure(arena, self, self.facts.clone(), method, ordinal)?;
            let (thrown, value) = if let Some(failure) = failure {
                (true, rich::io::rejection(arena, self, failure)?)
            } else {
                let field = match method {
                    "Client.getBlockCount" => "height",
                    "Client.getNetworkInfo" => "networkInfo",
                    _ => return Err(rich::Error::UnknownDependency.into()),
                };
                (
                    false,
                    rich::thrown::get_property(
                        arena,
                        self,
                        self.facts.clone(),
                        &Key::Utf16(field.encode_utf16().collect()),
                        0,
                    )?,
                )
            };
            let mut roots = args.to_vec();
            roots.push(value.clone());
            let (index, hash) = self.snapshot(arena, &roots)?;
            self.core.emit(obj(&[
                ("kind", s("rpc-input")),
                ("method", s(method)),
                ("ordinal", n(ordinal as f64)),
                ("argumentCount", n(args.len() as f64)),
                ("transport", s(if thrown { "Throw" } else { "Return" })),
                ("richSnapshot", s(index.to_string())),
                ("richSnapshotSha256", s(hash)),
                ("richEpoch", s(self.epoch().to_string())),
            ]));
            if thrown {
                Err(Abrupt::Source(RichThrownRef {
                    epoch: self.epoch(),
                    value,
                }))
            } else {
                rich::io::clone_plain_data(arena, self, value)
            }
        }

        fn index_rpc(&mut self, arena: &mut Arena, method: &str, args: &[Atom]) -> Eval<Atom> {
            // Read the actual already-initialized class client of the same owner.
            let client = self
                .core
                .main_production
                .as_ref()
                .ok_or(rich::Error::UnknownDependency)?
                .index
                .source_rich_class_client()
                .clone();
            let bad = if matches!(client, V::Null | V::Undefined) {
                Some(format!(
                    "Cannot read properties of {} (reading '{method}')",
                    if client == V::Null {
                        "null"
                    } else {
                        "undefined"
                    }
                ))
            } else if !matches!(client.get("$rpcClient"),V::Number(bits)if {let id=f64::from_bits(bits);id.is_finite()&&id>=1.&&id.fract()==0.})
            {
                Some(format!("this.client.{method} is not a function"))
            } else {
                None
            };
            if let Some(message) = bad {
                return Err(Abrupt::Source(self.construct_native_error(
                    arena,
                    NativeKind::TypeError,
                    Realm::SourceVm,
                    Some(message.encode_utf16().collect()),
                )?));
            }
            self.rpc(arena, &format!("Client.{method}"), args)
        }
        fn log(&mut self, arena: &mut Arena, level: &str, args: &[Atom]) -> Eval<()> {
            let (index, hash) = self.snapshot(arena, args)?;
            self.core.emit(obj(&[
                ("kind", s("source-log")),
                ("level", s(level)),
                ("argumentCount", n(args.len() as f64)),
                ("richSnapshot", s(index.to_string())),
                ("richSnapshotSha256", s(hash)),
                ("richEpoch", s(self.epoch().to_string())),
            ]));
            Ok(())
        }
    }
    impl ContractCore {
        pub(super) fn source_rich_state_value(&self) -> Result<V, SourceRuleError> {
            self.source_rich.as_ref().map_or(Ok(V::Null), State::value)
        }
    }
    impl SourceContractRuntime {
        pub fn rich_state_material(&self) -> Result<Vec<u8>, SourceRuleError> {
            if self.policy() != SourceContractRuntimePolicy::RichThrownV14 {
                return Err(SourceRuleError::InvalidInput);
            }
            self.core
                .source_rich
                .as_ref()
                .ok_or(SourceRuleError::Backend)?
                .material()
        }
        pub fn rich_effect_snapshots(&self) -> Result<&[Vec<u8>], SourceRuleError> {
            if self.policy() != SourceContractRuntimePolicy::RichThrownV14 {
                return Err(SourceRuleError::InvalidInput);
            }
            Ok(&self
                .core
                .source_rich
                .as_ref()
                .ok_or(SourceRuleError::Backend)?
                .snapshots)
        }
    }
    fn client_metadata(arena: &Arena, value: &Atom) -> Result<V, SourceRuleError> {
        let Atom::Reference(id) = value else {
            return Err(SourceRuleError::InvalidInput);
        };
        let object = arena
            .objects()
            .get(*id as usize)
            .ok_or(SourceRuleError::InvalidInput)?;
        if object.kind != rich::Kind::Ordinary {
            return Err(SourceRuleError::InvalidInput);
        }
        let mut fields = vec![];
        for property in &object.properties {
            let Key::Utf16(name) = &property.key else {
                return Err(SourceRuleError::InvalidInput);
            };
            let name =
                String::from_utf16(name).map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
            if !["chain", "test"].contains(&name.as_str()) {
                return Err(SourceRuleError::InvalidInput);
            }
            let PropertyValue::Data { value, .. } = &property.value else {
                return Err(SourceRuleError::InvalidInput);
            };
            let value = match value {
                Atom::Undefined => V::Undefined,
                Atom::Null => V::Null,
                Atom::Boolean(v) => V::Bool(*v),
                Atom::Binary64(v) => V::Number(*v),
                Atom::Utf16(v) => V::String(
                    String::from_utf16(v).map_err(|_| SourceRuleError::UnsupportedNumericDomain)?,
                ),
                _ => return Err(SourceRuleError::InvalidInput),
            };
            fields.push((name, value));
        }
        Ok(V::Object(fields))
    }
    impl SourceContractRuntime {
        pub fn invoke_dependency_rich(
            &mut self,
            call: &rich::protocol::InvokeRichV1,
        ) -> Result<rich::protocol::RichDependencyV1, SourceRuleError> {
            // Protocol fence before input allocation or any owner execution.
            if self.policy() != SourceContractRuntimePolicy::RichThrownV14
                || self.core.entry != "main-subgraph"
                || self.core.main_activation_profile != "source-registry"
            {
                return Err(SourceRuleError::InvalidInput);
            }
            self.core
                .source_rich
                .as_ref()
                .ok_or(SourceRuleError::Backend)?
                .arena
                .prevalidate(&call.arguments)
                .map_err(rule)?;
            let before = self.clone();
            let result = self.source_rich_invoke_inner(call);
            if result.is_err() || self.core.effects_exhausted {
                *self = before;
                return result.and(Err(SourceRuleError::Capacity));
            }
            result
        }
        fn source_rich_invoke_inner(
            &mut self,
            call: &rich::protocol::InvokeRichV1,
        ) -> Result<rich::protocol::RichDependencyV1, SourceRuleError> {
            let mut state = self
                .core
                .source_rich
                .take()
                .ok_or(SourceRuleError::Backend)?;
            let input = state.arena.validate(call.arguments.clone()).map_err(rule)?;
            let args = state.arena.import(input).map_err(rule)?.roots;
            let (transport, root) = if call.target == "SourceMain.configureRichOwnerInputs" {
                if state.io.is_some() || args.len() != 2 {
                    return Err(SourceRuleError::InvalidInput);
                }
                let client = client_metadata(&state.arena, &args[0])?;
                let Atom::Utf16(network) = &args[1] else {
                    return Err(SourceRuleError::InvalidInput);
                };
                let network =
                    String::from_utf16(network).map_err(|_| SourceRuleError::InvalidInput)?;
                self.core
                    .source_activation_configure(&[client])
                    .map_err(|e| match e {
                        Fault::Local(e) => e,
                        Fault::Thrown(_) => SourceRuleError::Backend,
                    })?;
                let value = self
                    .core
                    .dispatch(&DependencyCall {
                        target: "Activation.init".into(),
                        arguments: vec![s(network)],
                    })
                    .map_err(|e| match e {
                        Fault::Local(e) => e,
                        Fault::Thrown(_) => SourceRuleError::Backend,
                    })?;
                if value != V::Undefined {
                    return Err(SourceRuleError::Backend);
                }
                (rich::protocol::Transport::Immediate, Atom::Undefined)
            } else if call.target == "SourceMain.configureRichIO" {
                if state.io.is_some()
                    || args.len() < 2
                    || args.len() > 3
                    || self.core.main_activation_profile != "source-registry"
                {
                    return Err(SourceRuleError::InvalidInput);
                }
                let client = client_metadata(&state.arena, &args[0])?;
                let genesis = match args.get(2) {
                    None | Some(Atom::Undefined) => n(600000.),
                    Some(Atom::Binary64(bits)) => V::Number(*bits),
                    _ => return Err(SourceRuleError::InvalidInput),
                };
                // Reuse the existing same-DB TxIndex owner and its real client
                // initialization; rich IO data never becomes a balance owner.
                self.core
                    .source_main_production_configure(&[obj(&[
                        ("client", client),
                        ("io", obj(&[])),
                        ("genesisBlock", genesis),
                    ])])
                    .map_err(|e| match e {
                        Fault::Local(e) => e,
                        Fault::Thrown(_) => SourceRuleError::Backend,
                    })?;
                self.core.main_activation_locked = true;
                state.io = Some(args[1].clone());
                state
                    .exposure
                    .expose(&state.arena, std::slice::from_ref(&args[1]))
                    .map_err(rule)?;
                (rich::protocol::Transport::Immediate, Atom::Undefined)
            } else {
                if !args.is_empty() && call.target != "Main.constructConsensusFromIndex" {
                    return Err(SourceRuleError::InvalidInput);
                }
                let facts = state.io.clone().ok_or(SourceRuleError::InvalidInput)?;
                let result = if matches!(
                    call.target.as_str(),
                    "Main.constructConsensusFromIndex" | "Main.initOrLoadTxIndex"
                ) {
                    self.source_rich_owner_prefix(&call.target, &args, &mut state)
                } else {
                    let mut context = Context {
                        core: &mut self.core,
                        base: rich::intrinsics::LiteralIntrinsics::new(state.epoch, number),
                        facts,
                        exposure: &mut state.exposure,
                        snapshots: &mut state.snapshots,
                        snapshot_bytes: &mut state.snapshot_bytes,
                    };
                    match call.target.as_str() {
                        "Main.getCurrentBlockHeight" => {
                            rich::main::current_block_height(&mut state.arena, &mut context)
                        }
                        "Main.checkNetworkStatus" => {
                            rich::main::check_network_status(&mut state.arena, &mut context)
                        }
                        "TxIndex.fetchChainTip" => {
                            rich::main::fetch_chain_tip(&mut state.arena, &mut context)
                        }
                        _ => return Err(SourceRuleError::UnknownDependency),
                    }
                };
                match result {
                    Ok(value) => (rich::protocol::Transport::Pending, value),
                    Err(Abrupt::Source(error)) => {
                        if error.epoch != state.epoch {
                            return Err(SourceRuleError::Backend);
                        }
                        (rich::protocol::Transport::PendingThrow, error.value)
                    }
                    Err(Abrupt::Local(error)) => return Err(rule(error)),
                }
            };
            let graph = state.exposure.expose(&state.arena, &[root]).map_err(rule)?;
            let result = rich::protocol::RichDependencyV1 {
                transport,
                epoch: state.epoch,
                graph,
            };
            self.core.source_rich = Some(state);
            Ok(result)
        }
    }
    include!("source_rich_owner_prefix_impl.rs");
}
