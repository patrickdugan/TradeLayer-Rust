//! Literal intrinsic semantics used by the rich control tests. Owner/property
//! callbacks remain borrowed from the Runtime integration; no fixture answers.
use super::thrown::*;
use super::*;
fn u(v: &str) -> Units {
    v.encode_utf16().collect()
}
fn k(v: &str) -> Key {
    Key::Utf16(u(v))
}
fn i(realm: Realm, name: &str) -> Atom {
    Atom::Intrinsic(Intrinsic {
        realm,
        name: u(name),
    })
}
fn d(value: Atom) -> PropertyValue {
    PropertyValue::Data {
        value,
        writable: true,
    }
}
fn name(kind: &NativeKind) -> &'static str {
    match kind {
        NativeKind::Error => "Error",
        NativeKind::TypeError => "TypeError",
        NativeKind::RangeError => "RangeError",
        NativeKind::SyntaxError => "SyntaxError",
        NativeKind::ReferenceError => "ReferenceError",
        NativeKind::UriError => "URIError",
        NativeKind::EvalError => "EvalError",
        NativeKind::AggregateError => "AggregateError",
    }
}
fn function_signature(intrinsic: &Intrinsic) -> Option<(&'static str, usize)> {
    for (name, length) in [
        ("Object", 1),
        ("Array", 1),
        ("Function", 1),
        ("Error", 1),
        ("TypeError", 1),
        ("RangeError", 1),
        ("SyntaxError", 1),
        ("ReferenceError", 1),
        ("URIError", 1),
        ("EvalError", 1),
        ("AggregateError", 2),
        ("Boolean", 1),
        ("Number", 1),
        ("String", 1),
        ("BigInt", 1),
        ("Symbol", 0),
    ] {
        if intrinsic.name == u(name) {
            return Some((name, length));
        }
    }
    for (binding, name, length) in [
        ("Function.prototype", "", 0),
        ("Function.prototype.toString", "toString", 0),
        ("Error.stack.get", "", 0),
        ("Error.stack.set", "", 1),
        ("Object.prototype.toString", "toString", 0),
        ("Object.prototype.valueOf", "valueOf", 0),
        ("Array.prototype.toString", "toString", 0),
        ("Array.prototype.join", "join", 1),
        ("Error.prototype.toString", "toString", 0),
        ("Symbol.prototype.toPrimitive", "[Symbol.toPrimitive]", 1),
        ("Symbol.prototype.toString", "toString", 0),
        ("Symbol.prototype.valueOf", "valueOf", 0),
        ("Number.prototype.toString", "toString", 1),
        ("Number.prototype.valueOf", "valueOf", 0),
        ("Boolean.prototype.toString", "toString", 0),
        ("Boolean.prototype.valueOf", "valueOf", 0),
        ("String.prototype.toString", "toString", 0),
        ("String.prototype.valueOf", "valueOf", 0),
        ("BigInt.prototype.toString", "toString", 1),
        ("BigInt.prototype.valueOf", "valueOf", 0),
    ] {
        if intrinsic.name == u(binding) {
            return Some((name, length));
        }
    }
    None
}
pub fn capabilities() -> Capabilities {
    let mut out = Capabilities::default();
    for realm in [Realm::HostIo, Realm::SourceVm] {
        for name in [
            "Object",
            "Array",
            "Function",
            "Error",
            "TypeError",
            "RangeError",
            "SyntaxError",
            "ReferenceError",
            "URIError",
            "EvalError",
            "AggregateError",
            "Boolean",
            "Number",
            "String",
            "BigInt",
            "Symbol",
        ] {
            let constructor = Intrinsic {
                realm: realm.clone(),
                name: u(name),
            };
            out.intrinsics.insert(constructor.clone());
            out.callable_intrinsics.insert(constructor);
            out.intrinsics.insert(Intrinsic {
                realm: realm.clone(),
                name: u(&format!("{name}.prototype")),
            });
        }
        for name in [
            "Function.prototype",
            "Function.prototype.toString",
            "Number.prototype.toString",
            "Number.prototype.valueOf",
            "Boolean.prototype.toString",
            "Boolean.prototype.valueOf",
            "String.prototype.toString",
            "String.prototype.valueOf",
            "BigInt.prototype.toString",
            "BigInt.prototype.valueOf",
            "Object.prototype.toString",
            "Object.prototype.valueOf",
            "Array.prototype.toString",
            "Array.prototype.join",
            "Error.prototype.toString",
            "Error.stack.get",
            "Error.stack.set",
            "Symbol.prototype.toPrimitive",
            "Symbol.prototype.toString",
            "Symbol.prototype.valueOf",
        ] {
            let intrinsic = Intrinsic {
                realm: realm.clone(),
                name: u(name),
            };
            out.intrinsics.insert(intrinsic.clone());
            out.callable_intrinsics.insert(intrinsic);
        }
    }
    out.well_known_symbols.insert(u("toPrimitive"));
    out.well_known_symbols.insert(u("toStringTag"));
    out
}
pub struct LiteralIntrinsics {
    pub epoch: u64,
    pub realm: Realm,
    pub number_renderer: fn(u64) -> Eval<Units>,
    array_join_stack: Vec<Atom>,
}
impl LiteralIntrinsics {
    pub fn new(epoch: u64, number_renderer: fn(u64) -> Eval<Units>) -> Self {
        Self {
            epoch,
            realm: Realm::SourceVm,
            number_renderer,
            array_join_stack: vec![],
        }
    }
    fn object_to_string(&mut self, arena: &mut Arena, receiver: &Atom, depth: usize) -> Eval<Atom> {
        let default = match receiver {
            Atom::Undefined => "Undefined",
            Atom::Null => "Null",
            Atom::Boolean(_) => "Boolean",
            Atom::Binary64(_) => "Number",
            Atom::Utf16(_) => "String",
            Atom::Symbol(_) => "Symbol",
            Atom::BigInt(_) => "BigInt",
            Atom::Reference(id) => match &arena
                .objects
                .get(*id as usize)
                .ok_or(Error::InvalidReference)?
                .kind
            {
                Kind::Array => "Array",
                Kind::NativeError { .. } => "Error",
                Kind::Callable { .. } => "Function",
                Kind::Ordinary => "Object",
            },
            Atom::Intrinsic(v) if arena.caps.callable_intrinsics.contains(v) => "Function",
            Atom::Intrinsic(v) if v.name == u("Array.prototype") => "Array",
            Atom::Intrinsic(v) if v.name == u("Number.prototype") => "Number",
            Atom::Intrinsic(v) if v.name == u("Boolean.prototype") => "Boolean",
            Atom::Intrinsic(v) if v.name == u("String.prototype") => "String",
            _ => "Object",
        };
        let tag = if matches!(receiver, Atom::Null | Atom::Undefined) {
            None
        } else {
            match get_well_known_property(
                arena,
                self,
                receiver.clone(),
                &u("toStringTag"),
                depth + 1,
            )? {
                Atom::Utf16(v) => Some(v),
                _ => None,
            }
        };
        let mut out = u("[object ");
        out.extend(tag.unwrap_or_else(|| u(default)));
        out.push(93);
        Ok(Atom::Utf16(out))
    }
}
impl RichThrowContext for LiteralIntrinsics {
    fn epoch(&self) -> u64 {
        self.epoch
    }
    fn source_realm(&self) -> Realm {
        self.realm.clone()
    }
    fn replace_realm(&mut self, realm: Realm) -> Realm {
        std::mem::replace(&mut self.realm, realm)
    }
    fn construct_native_error(
        &mut self,
        arena: &mut Arena,
        kind: NativeKind,
        realm: Realm,
        message: Option<Units>,
    ) -> Eval<RichThrownRef> {
        let mut properties = vec![Property {
            key: k("stack"),
            enumerable: false,
            configurable: true,
            value: PropertyValue::Accessor {
                get: i(realm.clone(), "Error.stack.get"),
                set: i(realm.clone(), "Error.stack.set"),
            },
        }];
        if let Some(value) = message {
            properties.push(Property {
                key: k("message"),
                enumerable: false,
                configurable: true,
                value: d(Atom::Utf16(value)),
            });
        }
        let graph = Graph {
            roots: vec![Atom::Reference(0)],
            objects: vec![Object {
                prototype: i(realm.clone(), &format!("{}.prototype", name(&kind))),
                kind: Kind::NativeError {
                    constructor: kind,
                    realm,
                },
                properties,
            }],
            symbols: vec![],
        };
        let roots = arena.allocate_data_graph(graph)?;
        Ok(RichThrownRef {
            epoch: self.epoch,
            value: roots[0].clone(),
        })
    }
    fn intrinsic_property(
        &mut self,
        arena: &mut Arena,
        intrinsic: &Intrinsic,
        key: &Key,
    ) -> Eval<Option<PropertyValue>> {
        let n = &intrinsic.name;
        let r = intrinsic.realm.clone();
        if let Some((name, length)) = function_signature(intrinsic) {
            if *key == k("name") {
                return Ok(Some(d(Atom::Utf16(u(name)))));
            }
            if *key == k("length") {
                return Ok(Some(d(Atom::Binary64((length as f64).to_bits()))));
            }
        }
        if ["Array.prototype", "String.prototype"]
            .iter()
            .any(|name| *n == u(name))
            && *key == k("length")
        {
            return Ok(Some(d(Atom::Binary64(0f64.to_bits()))));
        }
        if let Key::Symbol(id) = key {
            if let Some(Symbol::WellKnown(name)) = arena.symbols.get(*id as usize).cloned() {
                return self.intrinsic_well_known_property(arena, intrinsic, &name);
            }
        }
        for primitive in ["Number", "Boolean", "String", "BigInt"] {
            if *n == u(&format!("{primitive}.prototype")) {
                if *key == k("toString") {
                    return Ok(Some(d(i(r, &format!("{primitive}.prototype.toString")))));
                }
                if *key == k("valueOf") {
                    return Ok(Some(d(i(r, &format!("{primitive}.prototype.valueOf")))));
                }
            }
        }
        if *n == u("Function.prototype") && *key == k("toString") {
            return Ok(Some(d(i(r, "Function.prototype.toString"))));
        }

        if [
            "Error",
            "TypeError",
            "RangeError",
            "SyntaxError",
            "ReferenceError",
            "URIError",
            "EvalError",
            "AggregateError",
        ]
        .iter()
        .any(|kind| *n == u(&format!("{kind}.prototype")))
        {
            if *key == k("name") {
                let stem = n[..n.len() - 10].to_vec();
                return Ok(Some(d(Atom::Utf16(stem))));
            }
            if *key == k("message") {
                return Ok(Some(d(Atom::Utf16(vec![]))));
            }
            if *n == u("Error.prototype") && *key == k("toString") {
                return Ok(Some(d(i(r, "Error.prototype.toString"))));
            }
        }
        if *n == u("Symbol.prototype") {
            if let Some(id) = arena.well_known.get(&u("toPrimitive")) {
                if *key == Key::Symbol(*id) {
                    return Ok(Some(d(i(r, "Symbol.prototype.toPrimitive"))));
                }
            }
        }
        let target = if *n == u("Object.prototype") && *key == k("toString") {
            Some("Object.prototype.toString")
        } else if *n == u("Object.prototype") && *key == k("valueOf") {
            Some("Object.prototype.valueOf")
        } else if *n == u("Array.prototype") && *key == k("toString") {
            Some("Array.prototype.toString")
        } else if *n == u("Array.prototype") && *key == k("join") {
            Some("Array.prototype.join")
        } else if *n == u("Symbol.prototype") && *key == k("toString") {
            Some("Symbol.prototype.toString")
        } else if *n == u("Symbol.prototype") && *key == k("valueOf") {
            Some("Symbol.prototype.valueOf")
        } else {
            None
        };
        Ok(target.map(|n| d(i(r, n))))
    }
    fn intrinsic_well_known_property(
        &mut self,
        _arena: &mut Arena,
        intrinsic: &Intrinsic,
        name: &[u16],
    ) -> Eval<Option<PropertyValue>> {
        if intrinsic.name == u("Symbol.prototype") && name == u("toPrimitive") {
            return Ok(Some(d(i(
                intrinsic.realm.clone(),
                "Symbol.prototype.toPrimitive",
            ))));
        }
        if name == u("toStringTag") {
            if intrinsic.name == u("Symbol.prototype") {
                return Ok(Some(d(Atom::Utf16(u("Symbol")))));
            }
            if intrinsic.name == u("BigInt.prototype") {
                return Ok(Some(d(Atom::Utf16(u("BigInt")))));
            }
        }
        Ok(None)
    }
    fn owner_well_known_property(
        &mut self,
        _arena: &mut Arena,
        _owner: &OwnerRef,
        _name: &[u16],
    ) -> Eval<Option<PropertyValue>> {
        Err(Error::UndeclaredCapability.into())
    }
    fn intrinsic_prototype(&mut self, intrinsic: &Intrinsic) -> Eval<Atom> {
        let r = intrinsic.realm.clone();
        let n = &intrinsic.name;
        if *n == u("Object.prototype") {
            Ok(Atom::Null)
        } else if [
            "TypeError",
            "RangeError",
            "SyntaxError",
            "ReferenceError",
            "URIError",
            "EvalError",
            "AggregateError",
        ]
        .iter()
        .any(|kind| *n == u(&format!("{kind}.prototype")))
        {
            Ok(i(r, "Error.prototype"))
        } else if n.ends_with(&u(".prototype")) {
            Ok(i(r, "Object.prototype"))
        } else {
            Ok(i(r, "Function.prototype"))
        }
    }
    fn owner_prototype(&mut self, _: &OwnerRef) -> Eval<Atom> {
        Err(Error::UndeclaredCapability.into())
    }
    fn owner_property(
        &mut self,
        _: &mut Arena,
        _: &OwnerRef,
        _: &Key,
    ) -> Eval<Option<PropertyValue>> {
        Err(Error::UndeclaredCapability.into())
    }
    fn number_to_string(&mut self, bits: u64) -> Eval<Units> {
        (self.number_renderer)(bits)
    }
    fn is_callable(&self, arena: &Arena, value: &Atom) -> bool {
        match value {
            Atom::Intrinsic(v) => arena.caps.callable_intrinsics.contains(v),
            Atom::Reference(id) => arena
                .objects
                .get(*id as usize)
                .is_some_and(|v| matches!(v.kind, Kind::Callable { .. })),
            _ => false,
        }
    }
    fn call_pinned(
        &mut self,
        arena: &mut Arena,
        function: &Atom,
        receiver: &Atom,
        args: &[Atom],
        depth: usize,
    ) -> Eval<Atom> {
        let Atom::Intrinsic(intrinsic) = function else {
            return Err(Error::UndeclaredCapability.into());
        };
        let old = std::mem::replace(&mut self.realm, intrinsic.realm.clone());
        let result = self.call_intrinsic(arena, function, receiver, args, depth);
        self.realm = old;
        result
    }
}
impl LiteralIntrinsics {
    fn call_intrinsic(
        &mut self,
        arena: &mut Arena,
        function: &Atom,
        receiver: &Atom,
        args: &[Atom],
        depth: usize,
    ) -> Eval<Atom> {
        if depth > 1024 {
            return Err(Error::Capacity.into());
        }
        let Atom::Intrinsic(intrinsic) = function else {
            return Err(Error::UndeclaredCapability.into());
        };
        if !arena.caps.callable_intrinsics.contains(intrinsic) {
            return Err(Error::UndeclaredCapability.into());
        }
        let n = &intrinsic.name;
        if *n == u("Function.prototype") {
            return Ok(Atom::Undefined);
        }
        if *n == u("Function.prototype.toString") {
            if let Atom::Intrinsic(receiver) = receiver {
                if let Some((name, _)) = function_signature(receiver) {
                    return Ok(Atom::Utf16(u(&format!(
                        "function {name}() {{ [native code] }}"
                    ))));
                }
            }
            if matches!(receiver,Atom::Reference(id) if arena.objects.get(*id as usize).is_some_and(|v|matches!(v.kind,Kind::Callable{..})))
            {
                return Err(Error::UndeclaredCapability.into());
            }
            return Err(Abrupt::Source(self.construct_native_error(
                arena,
                NativeKind::TypeError,
                self.source_realm(),
                Some(u(
                    "Function.prototype.toString requires that 'this' be a Function",
                )),
            )?));
        }
        for primitive in ["Number", "Boolean", "String", "BigInt"] {
            let tostring = *n == u(&format!("{primitive}.prototype.toString"));
            let valueof = *n == u(&format!("{primitive}.prototype.valueOf"));
            if !tostring && !valueof {
                continue;
            }
            let value = match (primitive, receiver) {
                ("Number", Atom::Binary64(_))
                | ("Boolean", Atom::Boolean(_))
                | ("String", Atom::Utf16(_))
                | ("BigInt", Atom::BigInt(_)) => Some(receiver.clone()),
                ("Number", Atom::Intrinsic(v)) if v.name == u("Number.prototype") => {
                    Some(Atom::Binary64(0f64.to_bits()))
                }
                ("Boolean", Atom::Intrinsic(v)) if v.name == u("Boolean.prototype") => {
                    Some(Atom::Boolean(false))
                }
                ("String", Atom::Intrinsic(v)) if v.name == u("String.prototype") => {
                    Some(Atom::Utf16(vec![]))
                }
                _ => None,
            };
            let value = if let Some(value) = value {
                value
            } else {
                let method = if tostring { "toString" } else { "valueOf" };
                return Err(Abrupt::Source(self.construct_native_error(
                    arena,
                    NativeKind::TypeError,
                    self.source_realm(),
                    Some(u(&format!(
                        "{primitive}.prototype.{method} requires that 'this' be a {primitive}"
                    ))),
                )?));
            };
            if valueof {
                return Ok(value);
            }
            if matches!(primitive, "Number" | "BigInt")
                && args.first().is_some_and(|v| {
                    !matches!(v, Atom::Undefined) && *v != Atom::Binary64(10f64.to_bits())
                })
            {
                return Err(Error::UndeclaredCapability.into());
            }
            return implicit_to_string(arena, self, value, depth + 1).map(Atom::Utf16);
        }
        if *n == u("Object.prototype.valueOf") {
            return Ok(receiver.clone());
        }
        if *n == u("Object.prototype.toString") {
            return self.object_to_string(arena, receiver, depth + 1);
        }
        if *n == u("Error.prototype.toString") {
            return error_to_string(arena, self, receiver.clone(), depth + 1).map(Atom::Utf16);
        }
        if *n == u("Symbol.prototype.toPrimitive") || *n == u("Symbol.prototype.valueOf") {
            return match receiver {
                Atom::Symbol(_) => Ok(receiver.clone()),
                _ => {
                    let method = if *n == u("Symbol.prototype.toPrimitive") {
                        "Symbol.prototype [ @@toPrimitive ]"
                    } else {
                        "Symbol.prototype.valueOf"
                    };
                    Err(Abrupt::Source(self.construct_native_error(
                        arena,
                        NativeKind::TypeError,
                        self.source_realm(),
                        Some(u(&format!("{method} requires that 'this' be a Symbol"))),
                    )?))
                }
            };
        }
        if *n == u("Symbol.prototype.toString") {
            return match receiver {
                Atom::Symbol(id) => Ok(Atom::Utf16(super::thrown::symbol_display(arena, *id)?)),
                _ => Err(Abrupt::Source(self.construct_native_error(
                    arena,
                    NativeKind::TypeError,
                    self.source_realm(),
                    Some(u(
                        "Symbol.prototype.toString requires that 'this' be a Symbol",
                    )),
                )?)),
            };
        }
        if *n == u("Array.prototype.toString") {
            let join = get_property(arena, self, receiver.clone(), &k("join"), depth + 1)?;
            return if self.is_callable(arena, &join) {
                self.call_pinned(arena, &join, receiver, &[], depth + 1)
            } else {
                self.object_to_string(arena, receiver, depth + 1)
            };
        }
        if *n == u("Array.prototype.join") {
            let length = get_property(arena, self, receiver.clone(), &k("length"), depth + 1)?;
            // Imported Arrays have a validated own Number length. Other generic
            // ToLength domains belong to the Runtime numeric coercion bridge.
            let length = match length {
                Atom::Undefined => 0,
                Atom::Binary64(bits) => {
                    let n = f64::from_bits(bits);
                    if n.is_nan() || n <= 0.0 {
                        0
                    } else if n.is_finite() && n <= 1_048_576.0 {
                        n.floor() as usize
                    } else {
                        return Err(Error::Capacity.into());
                    }
                }
                _ => return Err(Error::UndeclaredCapability.into()),
            };
            let separator = match args.first() {
                None | Some(Atom::Undefined) => u(","),
                Some(v) => implicit_to_string(arena, self, v.clone(), depth + 1)?,
            };
            // V8's Array join protects recursive arrays. This is execution state,
            // never a last-error identity or a fixture fingerprint.
            if self.array_join_stack.contains(receiver) {
                return Ok(Atom::Utf16(vec![]));
            }
            self.array_join_stack.push(receiver.clone());
            let result = (|| {
                let mut out = vec![];
                for at in 0..length {
                    if at != 0 {
                        out.extend(&separator)
                    }
                    let value = get_property(
                        arena,
                        self,
                        receiver.clone(),
                        &k(&at.to_string()),
                        depth + 1,
                    )?;
                    if !matches!(value, Atom::Null | Atom::Undefined) {
                        out.extend(implicit_to_string(arena, self, value, depth + 1)?)
                    }
                    if out.len() > arena.limits.units {
                        return Err(Error::Capacity.into());
                    }
                }
                Ok(Atom::Utf16(out))
            })();
            self.array_join_stack.pop();
            return result;
        }
        Err(Error::UndeclaredCapability.into())
    }
}
