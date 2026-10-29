//! Recorded data/IO fact construction. This does not accept balance, validity,
//! signature or proof-verifier results. Raw external graphs are prevalidated
//! into a fresh namespace before this module is reached.
use super::thrown::*;
use super::*;
fn u(s: &str) -> Units {
    s.encode_utf16().collect()
}
fn k(s: &str) -> Key {
    Key::Utf16(u(s))
}
fn prop<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    c: &mut C,
    v: &Atom,
    name: &str,
) -> Eval<Atom> {
    get_property(arena, c, v.clone(), &k(name), 0)
}
fn own(arena: &Arena, v: &Atom, name: &str) -> Result<bool, Error> {
    let Atom::Reference(id) = v else {
        return Err(Error::InvalidReference);
    };
    Ok(arena
        .objects
        .get(*id as usize)
        .ok_or(Error::InvalidReference)?
        .properties
        .iter()
        .any(|p| p.key == k(name)))
}
fn assign(arena: &mut Arena, receiver: &Atom, name: &str, value: Atom) -> Eval<()> {
    let Atom::Reference(id) = receiver else {
        return Err(Error::InvalidReference.into());
    };
    let old = arena
        .objects
        .get(*id as usize)
        .ok_or(Error::InvalidReference)?
        .properties
        .iter()
        .find(|p| p.key == k(name))
        .cloned();
    let (w, e, c) = old
        .map(|p| {
            (
                match p.value {
                    PropertyValue::Data { writable, .. } => writable,
                    _ => false,
                },
                p.enumerable,
                p.configurable,
            )
        })
        .unwrap_or((true, true, true));
    arena.define_data(receiver, k(name), value, w, e, c)?;
    Ok(())
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
fn array(arena: &Arena, value: &Atom) -> Result<Vec<Atom>, Error> {
    let Atom::Reference(id) = value else {
        return Err(Error::InvalidArray);
    };
    let object = arena
        .objects
        .get(*id as usize)
        .ok_or(Error::InvalidReference)?;
    if object.kind != Kind::Array {
        return Err(Error::InvalidArray);
    }
    let length = object
        .properties
        .iter()
        .find(|p| p.key == k("length"))
        .and_then(|p| match p.value {
            PropertyValue::Data {
                value: Atom::Binary64(v),
                ..
            } => Some(f64::from_bits(v) as usize),
            _ => None,
        })
        .ok_or(Error::InvalidArray)?;
    if length > 1024 {
        return Err(Error::Capacity);
    }
    Ok((0..length)
        .map(|at| {
            object
                .properties
                .iter()
                .find(|p| p.key == k(&at.to_string()))
                .and_then(|p| match &p.value {
                    PropertyValue::Data { value, .. } => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or(Atom::Undefined)
        })
        .collect())
}
/// Construct the actual IO rejection value from declared failure facts. The
/// caller retains it in the RPC effect and passes the same Atom into catches.
pub fn rejection<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    c: &mut C,
    failure: Atom,
) -> Eval<Atom> {
    if own(arena, &failure, "rawThrow")? {
        return prop(arena, c, &failure, "rawThrow");
    }
    let kind = prop(arena, c, &failure, "rawKind")?;
    if kind == Atom::Utf16(u("intrinsic-function-prototype")) {
        let realm = prop(arena, c, &failure, "realm")?;
        let realm = if realm == Atom::Utf16(u("HostIO")) {
            Realm::HostIo
        } else {
            Realm::SourceVm
        };
        return Ok(Atom::Intrinsic(Intrinsic {
            realm,
            name: u("Function.prototype"),
        }));
    }
    if kind == Atom::Utf16(u("symbol")) {
        let description = prop(arena, c, &failure, "description")?;
        let description = match description {
            Atom::Undefined => None,
            v => Some(implicit_to_string(arena, c, v, 0)?),
        };
        let graph = Graph {
            roots: vec![Atom::Symbol(0)],
            objects: vec![],
            symbols: vec![Symbol::Unique(description)],
        };
        return Ok(arena.allocate_data_graph(graph)?[0].clone());
    }
    if kind == Atom::Utf16(u("cycle")) {
        let id = u32::try_from(arena.objects.len()).map_err(|_| Error::Capacity)?;
        return arena
            .allocate(Object {
                kind: Kind::Ordinary,
                prototype: Atom::Intrinsic(Intrinsic {
                    realm: Realm::HostIo,
                    name: u("Object.prototype"),
                }),
                properties: vec![
                    Property {
                        key: k("kind"),
                        enumerable: true,
                        configurable: true,
                        value: PropertyValue::Data {
                            value: Atom::Utf16(u("cycle")),
                            writable: true,
                        },
                    },
                    Property {
                        key: k("self"),
                        enumerable: true,
                        configurable: true,
                        value: PropertyValue::Data {
                            value: Atom::Reference(id),
                            writable: true,
                        },
                    },
                ],
            })
            .map_err(Abrupt::from);
    }
    if kind == Atom::Utf16(u("bigint")) {
        let decimal = prop(arena, c, &failure, "decimal")?;
        let Atom::Utf16(v) = decimal else {
            return Err(Error::InvalidBigInt.into());
        };
        let text = String::from_utf16(&v).map_err(|_| Error::InvalidBigInt)?;
        if !bigint_valid(&text) {
            return Err(Error::InvalidBigInt.into());
        }
        return Ok(Atom::BigInt(text));
    }
    if kind == Atom::Utf16(u("null-prototype")) {
        let properties = prop(arena, c, &failure, "properties")?;
        let fields = if truth(&properties) {
            let Atom::Reference(id) = properties else {
                return Err(Error::InvalidProperty.into());
            };
            arena
                .objects
                .get(id as usize)
                .ok_or(Error::InvalidReference)?
                .properties
                .iter()
                .filter(|p| p.enumerable)
                .map(|p| p.key.clone())
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        let value = arena.allocate(Object {
            kind: Kind::Ordinary,
            prototype: Atom::Null,
            properties: vec![],
        })?;
        for field in fields {
            let v = get_property(arena, c, properties.clone(), &field, 0)?;
            arena.define_data(&value, field, v, true, true, true)?;
        }
        return Ok(value);
    }
    if kind == Atom::Utf16(u("self-error")) {
        let value = new_native_error(
            arena,
            c,
            NativeKind::Error,
            Realm::HostIo,
            Atom::Utf16(u("self")),
        )?
        .value;
        assign(arena, &value, "name", value.clone())?;
        return Ok(value);
    }
    let message = prop(arena, c, &failure, "message")?;
    let error = new_native_error(arena, c, NativeKind::Error, Realm::HostIo, message)?.value;
    let code = prop(arena, c, &failure, "code")?;
    assign(arena, &error, "code", code)?;
    let name = prop(arena, c, &failure, "name")?;
    assign(
        arena,
        &error,
        "name",
        if truth(&name) {
            name
        } else {
            Atom::Utf16(u("Error"))
        },
    )?;
    for (from, to) in [("errorMessage", "message"), ("errorName", "name")] {
        if own(arena, &failure, from)? {
            let value = prop(arena, c, &failure, from)?;
            assign(arena, &error, to, value)?;
        }
    }
    Ok(error)
}
pub fn select_failure<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    c: &mut C,
    facts: Atom,
    method: &str,
    ordinal: u64,
) -> Eval<Option<Atom>> {
    let failures = prop(arena, c, &facts, "failures")?;
    if failures == Atom::Undefined {
        return Ok(None);
    }
    for failure in array(arena, &failures)? {
        if strict_same(
            &prop(arena, c, &failure, "method")?,
            &Atom::Utf16(u(method)),
        ) && strict_same(
            &prop(arena, c, &failure, "ordinal")?,
            &Atom::Binary64((ordinal as f64).to_bits()),
        ) {
            return Ok(Some(failure));
        }
    }
    Ok(None)
}
/// Structured clone for the ordinary data/Array RPC document profile. Native
/// Error/DOM/function/symbol clone transports are separate, unfinished gates;
/// rejection values above never pass through this clone and retain raw identity.
pub fn clone_plain_data<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    c: &mut C,
    root: Atom,
) -> Eval<Atom> {
    fn map(
        value: Atom,
        arena: &Arena,
        ids: &mut Vec<u32>,
        map: &mut Vec<Option<u32>>,
    ) -> Result<Atom, Error> {
        match value {
            Atom::Reference(id) => {
                if id as usize >= arena.objects.len() {
                    return Err(Error::InvalidReference);
                }
                map.resize(arena.objects.len(), None);
                let next = if let Some(v) = map[id as usize] {
                    v
                } else {
                    let v = u32::try_from(ids.len()).map_err(|_| Error::Capacity)?;
                    map[id as usize] = Some(v);
                    ids.push(id);
                    v
                };
                Ok(Atom::Reference(next))
            }
            Atom::Symbol(_) | Atom::Intrinsic(_) | Atom::Owner(_) => {
                Err(Error::UndeclaredCapability)
            }
            other => Ok(other),
        }
    }
    let mut ids = vec![];
    let mut remap = vec![];
    let root = map(root, arena, &mut ids, &mut remap)?;
    let mut objects = vec![];
    let mut at = 0;
    while at < ids.len() {
        let source = arena.objects[ids[at] as usize].clone();
        if !matches!(source.kind, Kind::Ordinary | Kind::Array) {
            return Err(Error::UndeclaredCapability.into());
        }
        let receiver = Atom::Reference(ids[at]);
        let mut properties = vec![];
        for property in &source.properties {
            let Key::Utf16(name) = &property.key else {
                continue;
            };
            if source.kind == Kind::Array && *name == u("length") {
                let value = get_property(arena, c, receiver.clone(), &property.key, 0)?;
                properties.push(Property {
                    key: property.key.clone(),
                    enumerable: false,
                    configurable: false,
                    value: PropertyValue::Data {
                        value,
                        writable: true,
                    },
                });
                continue;
            }
            if !property.enumerable {
                continue;
            }
            let value = get_property(arena, c, receiver.clone(), &property.key, 0)?;
            let value = map(value, arena, &mut ids, &mut remap)?;
            properties.push(Property {
                key: property.key.clone(),
                enumerable: true,
                configurable: true,
                value: PropertyValue::Data {
                    value,
                    writable: true,
                },
            });
        }
        let prototype = Atom::Intrinsic(Intrinsic {
            realm: Realm::HostIo,
            name: u(if source.kind == Kind::Array {
                "Array.prototype"
            } else {
                "Object.prototype"
            }),
        });
        objects.push(Object {
            kind: source.kind,
            prototype,
            properties,
        });
        at += 1;
    }
    Ok(arena.allocate_data_graph(Graph {
        roots: vec![root],
        objects,
        symbols: vec![],
    })?[0]
        .clone())
}
