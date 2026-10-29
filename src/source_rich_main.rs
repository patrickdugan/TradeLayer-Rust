//! Actual pinned Main/TxIndex RPC method bodies over borrowed existing owners.
//! The context supplies recorded IO and the actual TxIndex class client. It is
//! not a balance/index/verifier result oracle or another financial runtime.
use super::thrown::*;
use super::*;
fn u(s: &str) -> Units {
    s.encode_utf16().collect()
}
fn k(s: &str) -> Key {
    Key::Utf16(u(s))
}
pub trait MainContext: RichThrowContext {
    fn rpc(&mut self, arena: &mut Arena, method: &str, args: &[Atom]) -> Eval<Atom>;
    fn index_rpc(&mut self, arena: &mut Arena, method: &str, args: &[Atom]) -> Eval<Atom>;
    fn log(&mut self, arena: &mut Arena, level: &str, args: &[Atom]) -> Eval<()>;
}
fn truth(v: &Atom) -> bool {
    match v {
        Atom::Undefined | Atom::Null | Atom::Boolean(false) => false,
        Atom::Binary64(bits) => {
            let n = f64::from_bits(*bits);
            n != 0.0 && !n.is_nan()
        }
        Atom::Utf16(v) => !v.is_empty(),
        Atom::BigInt(v) => v != "0",
        _ => true,
    }
}
fn object(arena: &mut Arena, fields: &[(&str, Atom)]) -> Eval<Atom> {
    arena
        .allocate(Object {
            kind: Kind::Ordinary,
            prototype: Atom::Intrinsic(Intrinsic {
                realm: Realm::SourceVm,
                name: u("Object.prototype"),
            }),
            properties: fields
                .iter()
                .map(|(name, value)| Property {
                    key: k(name),
                    enumerable: true,
                    configurable: true,
                    value: PropertyValue::Data {
                        value: value.clone(),
                        writable: true,
                    },
                })
                .collect(),
        })
        .map_err(Abrupt::from)
}
pub fn current_block_height<C: MainContext + ?Sized>(
    arena: &mut Arena,
    context: &mut C,
) -> Eval<Atom> {
    let result = (|| {
        let value = context.rpc(arena, "Client.getBlockCount", &[])?;
        context.log(arena, "log", std::slice::from_ref(&value))?;
        Ok(value)
    })();
    match result {
        Err(Abrupt::Source(error)) => {
            context.log(
                arena,
                "error",
                &[
                    Atom::Utf16(u("Error fetching current block height:")),
                    error.value.clone(),
                ],
            )?;
            Err(Abrupt::Source(error))
        }
        other => other,
    }
}
pub fn check_network_status<C: MainContext + ?Sized>(
    arena: &mut Arena,
    context: &mut C,
) -> Eval<Atom> {
    let network = (|| {
        let info = context.rpc(arena, "Client.getNetworkInfo", &[])?;
        let active = get_property(arena, context, info.clone(), &k("networkactive"), 0)?;
        let connections = get_property(arena, context, info, &k("connections"), 0)?;
        if !truth(&active) {
            context.log(
                arena,
                "error",
                &[Atom::Utf16(u(
                    "Network is inactive! The node is not connected to the Bitcoin network.",
                ))],
            )?;
            return object(
                arena,
                &[
                    ("status", Atom::Boolean(false)),
                    ("reason", Atom::Utf16(u("Network inactive"))),
                ],
            );
        }
        if strict_same(&connections, &Atom::Binary64(0f64.to_bits())) {
            context.log(
                arena,
                "warn",
                &[Atom::Utf16(u(
                    "Node has 0 connections. It may be isolated from the network.",
                ))],
            )?;
            return object(
                arena,
                &[
                    ("status", Atom::Boolean(false)),
                    ("reason", Atom::Utf16(u("No connections"))),
                ],
            );
        }
        object(
            arena,
            &[
                ("status", Atom::Boolean(true)),
                ("connections", connections),
            ],
        )
    })();
    let error = match network {
        Err(Abrupt::Source(error)) => error,
        other => return other,
    };
    // The source reads code twice when the first strict comparison is false;
    // getters and replacement exceptions therefore retain their actual order.
    let first = get_property(arena, context, error.value.clone(), &k("code"), 0)?;
    let caught = if strict_same(&first, &Atom::Utf16(u("ECONNREFUSED"))) {
        true
    } else {
        let second = get_property(arena, context, error.value.clone(), &k("code"), 0)?;
        strict_same(&second, &Atom::Utf16(u("ETIMEDOUT")))
    };
    if caught {
        let message = get_property(arena, context, error.value, &k("message"), 0)?;
        let message = implicit_to_string(arena, context, message, 0)?;
        let mut text = u("Network error: ");
        text.extend(message);
        text.extend(u(". Could not reach the Bitcoin node."));
        context.log(arena, "error", &[Atom::Utf16(text)])?;
        object(
            arena,
            &[
                ("status", Atom::Boolean(false)),
                ("reason", Atom::Utf16(u("Connection refused or timeout"))),
            ],
        )
    } else {
        context.log(
            arena,
            "error",
            &[
                Atom::Utf16(u("An unexpected error occurred:")),
                error.value.clone(),
            ],
        )?;
        Err(Abrupt::Source(error))
    }
}
pub fn fetch_chain_tip<C: MainContext + ?Sized>(arena: &mut Arena, context: &mut C) -> Eval<Atom> {
    match context.index_rpc(arena, "getBlockCount", &[]) {
        Err(Abrupt::Source(error)) => {
            let rendered = implicit_to_string(arena, context, error.value, 0)?;
            let mut message = u("Error fetching chain tip: ");
            message.extend(rendered);
            Err(Abrupt::Source(new_native_error(
                arena,
                context,
                NativeKind::Error,
                Realm::SourceVm,
                Atom::Utf16(message),
            )?))
        }
        other => other,
    }
}
pub fn fetch_block_data<C: MainContext + ?Sized>(
    arena: &mut Arena,
    context: &mut C,
    height: Atom,
) -> Eval<Atom> {
    let result = (|| {
        let hash = context.index_rpc(arena, "getBlockHash", std::slice::from_ref(&height))?;
        context.index_rpc(arena, "getBlock", &[hash])
    })();
    match result {
        Err(Abrupt::Source(error)) => {
            let mut text = u("Error fetching block data for height ");
            text.extend(implicit_to_string(arena, context, height, 0)?);
            text.push(58);
            context.log(arena, "error", &[Atom::Utf16(text), error.value.clone()])?;
            Err(Abrupt::Source(error))
        }
        other => other,
    }
}
