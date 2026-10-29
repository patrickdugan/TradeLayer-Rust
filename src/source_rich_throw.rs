//! Actual-value abrupt completions. One borrowed arena and owner context;
//! this layer never substitutes a financial result or a display-text latch.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichThrownRef {
    pub epoch: u64,
    pub value: Atom,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Abrupt {
    Source(RichThrownRef),
    Local(Error),
}
impl From<Error> for Abrupt {
    fn from(value: Error) -> Self {
        Self::Local(value)
    }
}
pub type Eval<T> = Result<T, Abrupt>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichDependencyReturnV1 {
    Immediate(Atom),
    Pending(Atom),
    Throw(RichThrownRef),
    PendingThrow(RichThrownRef),
}
pub trait RichThrowContext {
    fn epoch(&self) -> u64;
    fn source_realm(&self) -> Realm;
    fn replace_realm(&mut self, realm: Realm) -> Realm;
    fn construct_native_error(
        &mut self,
        arena: &mut Arena,
        kind: NativeKind,
        realm: Realm,
        message: Option<Units>,
    ) -> Eval<RichThrownRef>;
    fn intrinsic_property(
        &mut self,
        arena: &mut Arena,
        intrinsic: &Intrinsic,
        key: &Key,
    ) -> Eval<Option<PropertyValue>>;
    fn intrinsic_well_known_property(
        &mut self,
        arena: &mut Arena,
        intrinsic: &Intrinsic,
        name: &[u16],
    ) -> Eval<Option<PropertyValue>>;
    fn intrinsic_prototype(&mut self, intrinsic: &Intrinsic) -> Eval<Atom>;
    fn owner_well_known_property(
        &mut self,
        arena: &mut Arena,
        owner: &OwnerRef,
        name: &[u16],
    ) -> Eval<Option<PropertyValue>>;
    fn owner_prototype(&mut self, owner: &OwnerRef) -> Eval<Atom>;
    fn number_to_string(&mut self, bits: u64) -> Eval<Units>;
    fn owner_property(
        &mut self,
        arena: &mut Arena,
        owner: &OwnerRef,
        key: &Key,
    ) -> Eval<Option<PropertyValue>>;
    fn call_pinned(
        &mut self,
        arena: &mut Arena,
        function: &Atom,
        receiver: &Atom,
        args: &[Atom],
        depth: usize,
    ) -> Eval<Atom>;
    fn is_callable(&self, arena: &Arena, value: &Atom) -> bool;
}
fn key(value: &str) -> Key {
    Key::Utf16(value.encode_utf16().collect())
}
fn native<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    context: &mut C,
    message: &str,
) -> Eval<Abrupt> {
    Ok(Abrupt::Source(context.construct_native_error(
        arena,
        NativeKind::TypeError,
        context.source_realm(),
        Some(message.encode_utf16().collect()),
    )?))
}
fn primitive(value: &Atom) -> bool {
    !matches!(
        value,
        Atom::Reference(_) | Atom::Owner(_) | Atom::Intrinsic(_)
    )
}
fn property_value<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    context: &mut C,
    property: PropertyValue,
    receiver: &Atom,
    depth: usize,
) -> Eval<Atom> {
    match property {
        PropertyValue::Data { value, .. } => Ok(value),
        PropertyValue::Accessor {
            get: Atom::Undefined,
            ..
        } => Ok(Atom::Undefined),
        PropertyValue::Accessor { get, .. } => {
            context.call_pinned(arena, &get, receiver, &[], depth)
        }
    }
}
pub fn get_property<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    context: &mut C,
    value: Atom,
    wanted: &Key,
    depth: usize,
) -> Eval<Atom> {
    if matches!(value, Atom::Null | Atom::Undefined) {
        let kind = if value == Atom::Null {
            "null"
        } else {
            "undefined"
        };
        let mut message: Units = format!("Cannot read properties of {kind} (reading '")
            .encode_utf16()
            .collect();
        match wanted {
            Key::Utf16(v) => message.extend(v),
            Key::Symbol(id) => message.extend(symbol_display(arena, *id)?),
        }
        message.extend("')".encode_utf16());
        return Err(Abrupt::Source(context.construct_native_error(
            arena,
            NativeKind::TypeError,
            context.source_realm(),
            Some(message),
        )?));
    }

    if let Atom::Utf16(text) = &value {
        if *wanted == key("length") {
            return Ok(Atom::Binary64((text.len() as f64).to_bits()));
        }
        if let Key::Utf16(index) = wanted {
            if let Some(index) = index_key(index) {
                if let Some(unit) = text.get(index as usize) {
                    return Ok(Atom::Utf16(vec![*unit]));
                }
            }
        }
    }
    let mut current = match &value {
        Atom::Boolean(_) => Atom::Intrinsic(Intrinsic {
            realm: context.source_realm(),
            name: "Boolean.prototype".encode_utf16().collect(),
        }),
        Atom::Binary64(_) => Atom::Intrinsic(Intrinsic {
            realm: context.source_realm(),
            name: "Number.prototype".encode_utf16().collect(),
        }),
        Atom::Utf16(_) => Atom::Intrinsic(Intrinsic {
            realm: context.source_realm(),
            name: "String.prototype".encode_utf16().collect(),
        }),
        Atom::BigInt(_) => Atom::Intrinsic(Intrinsic {
            realm: context.source_realm(),
            name: "BigInt.prototype".encode_utf16().collect(),
        }),
        Atom::Symbol(_) => Atom::Intrinsic(Intrinsic {
            realm: context.source_realm(),
            name: "Symbol.prototype".encode_utf16().collect(),
        }),
        other => other.clone(),
    };
    for _ in 0..=arena.objects.len() + 16 {
        match current {
            Atom::Reference(id) => {
                let object = arena
                    .objects
                    .get(id as usize)
                    .ok_or(Error::InvalidReference)?;
                let found = object
                    .properties
                    .iter()
                    .find(|p| &p.key == wanted)
                    .map(|p| p.value.clone());
                let prototype = object.prototype.clone();
                if let Some(property) = found {
                    return property_value(arena, context, property, &value, depth);
                }
                current = prototype;
            }
            Atom::Intrinsic(intrinsic) => {
                if let Some(property) = context.intrinsic_property(arena, &intrinsic, wanted)? {
                    return property_value(arena, context, property, &value, depth);
                }
                current = context.intrinsic_prototype(&intrinsic)?;
            }
            Atom::Owner(owner) => {
                if let Some(property) = context.owner_property(arena, &owner, wanted)? {
                    return property_value(arena, context, property, &value, depth);
                }
                current = context.owner_prototype(&owner)?;
            }
            Atom::Null => return Ok(Atom::Undefined),
            _ => return Err(Error::InvalidPrototype.into()),
        }
    }
    Err(Error::Capacity.into())
}
pub fn get_well_known_property<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    context: &mut C,
    value: Atom,
    name: &[u16],
    depth: usize,
) -> Eval<Atom> {
    if !arena.caps.well_known_symbols.contains(name) {
        return Err(Error::UndeclaredCapability.into());
    }
    if let Some(id) = arena.well_known.get(name).copied() {
        return get_property(arena, context, value, &Key::Symbol(id), depth);
    }
    if matches!(value, Atom::Null | Atom::Undefined) {
        let mut message: Units = format!(
            "Cannot read properties of {} (reading 'Symbol(Symbol.",
            if value == Atom::Null {
                "null"
            } else {
                "undefined"
            }
        )
        .encode_utf16()
        .collect();
        message.extend(name);
        message.extend(")')".encode_utf16());
        return Err(Abrupt::Source(context.construct_native_error(
            arena,
            NativeKind::TypeError,
            context.source_realm(),
            Some(message),
        )?));
    }
    // No heap-own property can carry this well-known identity when it has
    // never been imported/observed. Native intrinsic prototypes still can.
    let receiver = value.clone();
    let mut current = match value {
        Atom::BigInt(_) => Atom::Intrinsic(Intrinsic {
            realm: context.source_realm(),
            name: "BigInt.prototype".encode_utf16().collect(),
        }),
        Atom::Symbol(_) => Atom::Intrinsic(Intrinsic {
            realm: context.source_realm(),
            name: "Symbol.prototype".encode_utf16().collect(),
        }),
        Atom::Boolean(_) => Atom::Intrinsic(Intrinsic {
            realm: context.source_realm(),
            name: "Boolean.prototype".encode_utf16().collect(),
        }),
        Atom::Binary64(_) => Atom::Intrinsic(Intrinsic {
            realm: context.source_realm(),
            name: "Number.prototype".encode_utf16().collect(),
        }),
        Atom::Utf16(_) => Atom::Intrinsic(Intrinsic {
            realm: context.source_realm(),
            name: "String.prototype".encode_utf16().collect(),
        }),
        other => other,
    };
    for _ in 0..=arena.objects.len() + 16 {
        match current {
            Atom::Reference(id) => {
                current = arena
                    .objects
                    .get(id as usize)
                    .ok_or(Error::InvalidReference)?
                    .prototype
                    .clone();
            }
            Atom::Intrinsic(intrinsic) => {
                if let Some(property) =
                    context.intrinsic_well_known_property(arena, &intrinsic, name)?
                {
                    return property_value(arena, context, property, &receiver, depth);
                }
                current = context.intrinsic_prototype(&intrinsic)?;
            }
            Atom::Owner(owner) => {
                if let Some(property) = context.owner_well_known_property(arena, &owner, name)? {
                    return property_value(arena, context, property, &receiver, depth);
                }
                current = context.owner_prototype(&owner)?;
            }
            Atom::Null => return Ok(Atom::Undefined),
            Atom::Undefined => return Ok(Atom::Undefined),
            _ => return Err(Error::InvalidPrototype.into()),
        }
    }
    Err(Error::Capacity.into())
}
fn to_primitive<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    context: &mut C,
    value: Atom,
    depth: usize,
) -> Eval<Atom> {
    if primitive(&value) {
        return Ok(value);
    }
    if depth > 1024 {
        return Err(Error::Capacity.into());
    }
    {
        let method = get_well_known_property(
            arena,
            context,
            value.clone(),
            &"toPrimitive".encode_utf16().collect::<Vec<_>>(),
            depth + 1,
        )?;
        if !matches!(method, Atom::Undefined | Atom::Null) {
            if !context.is_callable(arena, &method) {
                let message = not_callable_message(arena, context, &method)?;
                return Err(Abrupt::Source(context.construct_native_error(
                    arena,
                    NativeKind::TypeError,
                    context.source_realm(),
                    Some(message),
                )?));
            }
            let result = context.call_pinned(
                arena,
                &method,
                &value,
                &[Atom::Utf16("string".encode_utf16().collect())],
                depth + 1,
            )?;
            if primitive(&result) {
                return Ok(result);
            }
            return Err(native(
                arena,
                context,
                "Cannot convert object to primitive value",
            )?);
        }
    }
    for name in ["toString", "valueOf"] {
        let method = get_property(arena, context, value.clone(), &key(name), depth + 1)?;
        if context.is_callable(arena, &method) {
            let result = context.call_pinned(arena, &method, &value, &[], depth + 1)?;
            if primitive(&result) {
                return Ok(result);
            }
        }
    }
    Err(native(
        arena,
        context,
        "Cannot convert object to primitive value",
    )?)
}
pub fn implicit_to_string<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    context: &mut C,
    value: Atom,
    depth: usize,
) -> Eval<Units> {
    match value {
        Atom::Undefined => Ok("undefined".encode_utf16().collect()),
        Atom::Null => Ok("null".encode_utf16().collect()),
        Atom::Boolean(v) => Ok(if v { "true" } else { "false" }.encode_utf16().collect()),
        Atom::Binary64(v) => context.number_to_string(v),
        Atom::Utf16(v) => Ok(v),
        Atom::BigInt(v) => Ok(v.encode_utf16().collect()),
        Atom::Symbol(_) => Err(native(
            arena,
            context,
            "Cannot convert a Symbol value to a string",
        )?),
        other => {
            let value = to_primitive(arena, context, other, depth + 1)?;
            implicit_to_string(arena, context, value, depth + 1)
        }
    }
}

pub(super) fn symbol_display(arena: &Arena, id: u32) -> Result<Units, Error> {
    let symbol = arena
        .symbols
        .get(id as usize)
        .ok_or(Error::InvalidReference)?;
    let mut out: Units = "Symbol(".encode_utf16().collect();
    match symbol {
        Symbol::Unique(Some(v)) | Symbol::Registered(v) => out.extend(v),
        Symbol::WellKnown(v) => {
            out.extend("Symbol.".encode_utf16());
            out.extend(v)
        }
        Symbol::Unique(None) => (),
    }
    out.push(41);
    Ok(out)
}
fn not_callable_message<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    context: &mut C,
    value: &Atom,
) -> Eval<Units> {
    let mut out: Units = match value {
        Atom::Binary64(bits) => {
            let mut v: Units = "number ".encode_utf16().collect();
            v.extend(context.number_to_string(*bits)?);
            v
        }
        Atom::Utf16(v) => {
            let mut out: Units = "string \"".encode_utf16().collect();
            out.extend(v);
            out.push(34);
            out
        }
        Atom::Boolean(v) => format!("boolean {v}").encode_utf16().collect(),
        Atom::BigInt(_) => "bigint".encode_utf16().collect(),
        Atom::Symbol(_) => "symbol".encode_utf16().collect(),
        _ => {
            let _ = arena;
            "object".encode_utf16().collect()
        }
    };
    out.extend(" is not a function".encode_utf16());
    Ok(out)
}
pub fn strict_same(left: &Atom, right: &Atom) -> bool {
    match (left, right) {
        (Atom::Binary64(a), Atom::Binary64(b)) => {
            let (a, b) = (f64::from_bits(*a), f64::from_bits(*b));
            a == b
        }
        _ => left == right,
    }
}
pub fn new_native_error<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    context: &mut C,
    kind: NativeKind,
    realm: Realm,
    message: Atom,
) -> Eval<RichThrownRef> {
    let old = context.replace_realm(realm.clone());
    let result = (|| {
        let message = if message == Atom::Undefined {
            None
        } else {
            Some(implicit_to_string(arena, context, message, 0)?)
        };
        context.construct_native_error(arena, kind, realm, message)
    })();
    context.replace_realm(old);
    result
}
pub fn error_to_string<C: RichThrowContext + ?Sized>(
    arena: &mut Arena,
    context: &mut C,
    value: Atom,
    depth: usize,
) -> Eval<Units> {
    if primitive(&value) {
        let mut message: Units = "Method Error.prototype.toString called on incompatible receiver "
            .encode_utf16()
            .collect();
        message.extend(match value {
            Atom::Symbol(id) => symbol_display(arena, id)?,
            other => implicit_to_string(arena, context, other, depth + 1)?,
        });
        return Err(Abrupt::Source(context.construct_native_error(
            arena,
            NativeKind::TypeError,
            context.source_realm(),
            Some(message),
        )?));
    }

    let name = get_property(arena, context, value.clone(), &key("name"), depth + 1)?;
    let name = if name == Atom::Undefined {
        "Error".encode_utf16().collect()
    } else {
        implicit_to_string(arena, context, name, depth + 1)?
    };
    let message = get_property(arena, context, value, &key("message"), depth + 1)?;
    let message = if message == Atom::Undefined {
        vec![]
    } else {
        implicit_to_string(arena, context, message, depth + 1)?
    };
    if name.is_empty() {
        return Ok(message);
    }
    if message.is_empty() {
        return Ok(name);
    }
    let mut out = name;
    out.extend(": ".encode_utf16());
    out.extend(message);
    Ok(out)
}
