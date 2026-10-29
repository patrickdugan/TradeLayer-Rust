// Included inside rich_runtime_v14. Prefixes borrow the existing Core/TxIndex;
// the rich continuation preserves the actual raw IO reference and source catch.
fn owner_rule(e: SourceRuleError) -> rich::Error {
    match e {
        SourceRuleError::Capacity => rich::Error::Capacity,
        SourceRuleError::UnsupportedNumericDomain => rich::Error::UnsupportedNumericDomain,
        SourceRuleError::UnknownDependency => rich::Error::UnknownDependency,
        _ => rich::Error::Backend,
    }
}
fn prefix<T>(result: R<T>) -> Eval<T> {
    result.map_err(|e| match e {
        Fault::Local(e) => Abrupt::Local(owner_rule(e)),
        // The older typed owner cannot transport arbitrary DB rejection graphs.
        // Do not invent a native constructor from SourceException.name.
        Fault::Thrown(_) => Abrupt::Local(rich::Error::UndeclaredCapability),
    })
}
fn plain_atom(value: V) -> Eval<Atom> {
    Ok(match value {
        V::Undefined => Atom::Undefined,
        V::Null => Atom::Null,
        V::Bool(v) => Atom::Boolean(v),
        V::Number(v) => Atom::Binary64(v),
        V::String(v) => Atom::Utf16(v.encode_utf16().collect()),
        _ => return Err(rich::Error::UndeclaredCapability.into()),
    })
}
fn with_owner_context<T>(
    runtime: &mut SourceContractRuntime,
    state: &mut State,
    f: impl FnOnce(&mut Arena, &mut Context<'_>) -> Eval<T>,
) -> Eval<T> {
    let mut context = Context {
        core: &mut runtime.core,
        base: rich::intrinsics::LiteralIntrinsics::new(state.epoch, number),
        facts: state.io.clone().ok_or(rich::Error::UnknownDependency)?,
        exposure: &mut state.exposure,
        snapshots: &mut state.snapshots,
        snapshot_bytes: &mut state.snapshot_bytes,
    };
    f(&mut state.arena, &mut context)
}
fn owner_log(
    runtime: &mut SourceContractRuntime,
    state: &mut State,
    event: SourceMainPrefixLog,
) -> R<()> {
    let formatted: R<(&str, Vec<V>)> = (|| {
        use SourceMainPrefixLog::*;
        let concat = |prefix: &str, v: &V| -> R<V> {
            Ok(s(format!("{prefix}{}", source_channel_string(v)?)))
        };
        Ok(match event {
            MaxIndexed(v) => ("log", vec![s(format!("max Indexed Block {}", json(&v)?))]),
            AboutToInit(v) => ("log", vec![concat("about to init index with ", &v)?]),
            GenesisExisting(v) => ("log", vec![s("Genesis block is already initialized:"), v]),
            GenesisInserted(v) => ("log", vec![s("Genesis block initialized:"), v]),
            TallyMissing => (
                "log",
                vec![s("failed to load tallyMap, starting a new map")],
            ),
            ConsensusMissing => (
                "log",
                vec![s("MaxProcessedHeight not found in the database.")],
            ),
            NoTransactions => ("log", vec![s("No transactions to process.")]),
            Catchup(last, start) => (
                "log",
                vec![
                    concat("construct Consensus from Index max indexed block ", &last)?,
                    concat("start height ", &start)?,
                ],
            ),
            ParseHeight(v) => ("log", vec![concat("parsing towards real-time mode ", &v)?]),
            FundingSkips(v) => ("log", vec![concat("skips? ", &v)?]),
            TradingSkips(v) => ("log", vec![concat("skips 2? ", &v)?]),
            Remaining(one, two, funding, trading) => (
                "log",
                vec![s(format!(
                    "skip to my lou my darlin {} {} {funding} {trading}",
                    source_channel_string(&one)?,
                    source_channel_string(&two)?
                ))],
            ),
            PartialReplay => (
                "warn",
                vec![s(
                    "partially processed block detected; continuing idempotent replay",
                )],
            ),
        })
    })();
    let (level, values) = formatted?;
    let values = values
        .into_iter()
        .map(plain_atom)
        .collect::<Eval<Vec<_>>>()
        .map_err(|e| match e {
            Abrupt::Local(e) => Fault::Local(rule(e)),
            Abrupt::Source(_) => Fault::Local(SourceRuleError::Backend),
        })?;
    with_owner_context(runtime, state, |arena, context| {
        rich::main::MainContext::log(context, arena, level, &values)
    })
    .map_err(|e| match e {
        Abrupt::Local(e) => Fault::Local(rule(e)),
        Abrupt::Source(_) => Fault::Local(SourceRuleError::Backend),
    })
}
impl SourceContractRuntime {
    fn source_rich_owner_prefix(
        &mut self,
        target: &str,
        args: &[Atom],
        state: &mut State,
    ) -> Eval<Atom> {
        use rich::main::MainContext;
        match target {
            "Main.constructConsensusFromIndex" => {
                let start = match args {
                    [Atom::Binary64(bits)] => V::Number(*bits),
                    _ => return Err(rich::Error::UndeclaredCapability.into()),
                };
                let tail = prefix(self.source_main_production_catchup_prefix_observed(
                    start,
                    &mut |runtime, event| owner_log(runtime, state, event),
                ))?;
                if matches!(tail, SourceMainCatchupTail::Complete) {
                    return Ok(Atom::Undefined);
                }
                // Source returns syncIfNecessary's Promise, so its rejection
                // escapes with the saved height and the exact raw reference.
                with_owner_context(self, state, |arena, context| {
                    context.rpc(arena, "Client.getBlockCount", &[])
                })?;
                Err(rich::Error::UndeclaredCapability.into())
            }
            "Main.initOrLoadTxIndex" => {
                if !args.is_empty() {
                    return Err(rich::Error::UndeclaredCapability.into());
                }
                let max = prefix(self.source_main_production_init_prefix_observed(
                    &mut |runtime, event| owner_log(runtime, state, event),
                ))?;
                let max_text = prefix(source_channel_string(&max))?;
                with_owner_context(self, state, |arena, context| {
                    context.log(
                        arena,
                        "log",
                        &[Atom::Utf16(
                            format!("sync Index maxIndexedBlock {max_text}")
                                .encode_utf16()
                                .collect(),
                        )],
                    )
                })?;
                let tip = match with_owner_context(self, state, |arena, context| {
                    context.rpc(arena, "Client.getBlockCount", &[])
                }) {
                    Ok(v) => v,
                    Err(Abrupt::Source(error)) => {
                        with_owner_context(self, state, |arena, context| {
                            context.log(
                                arena,
                                "error",
                                &[
                                    Atom::Utf16("Error during syncIndex:".encode_utf16().collect()),
                                    error.value,
                                ],
                            )
                        })?;
                        return Ok(Atom::Undefined);
                    }
                    Err(e) => return Err(e),
                };
                with_owner_context(self, state, |arena, context| {
                    let text = rich::thrown::implicit_to_string(arena, context, tip.clone(), 0)?;
                    let mut message: Units =
                        "sync index retrieved chaintip ".encode_utf16().collect();
                    message.extend(text);
                    context.log(arena, "log", &[Atom::Utf16(message)])
                })?;
                let (V::Number(max), Atom::Binary64(tip)) = (max, tip) else {
                    return Err(rich::Error::UndeclaredCapability.into());
                };
                let (max, tip) = (f64::from_bits(max), f64::from_bits(tip));
                if max > tip {
                    return Ok(Atom::Undefined);
                }
                // Successful extraction and the next rich financial/live graph
                // remain explicit capabilities, not a claimed source result.
                Err(rich::Error::UndeclaredCapability.into())
            }
            _ => Err(rich::Error::UnknownDependency.into()),
        }
    }
}
