//! Bounded policy14 thrown-data graph. Financial objects remain owner references.
use std::collections::{BTreeMap, BTreeSet};
pub type Units = Vec<u16>;
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Realm {
    HostIo,
    SourceVm,
    Shared,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Intrinsic {
    pub realm: Realm,
    pub name: Units,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct OwnerRef {
    pub domain: Units,
    pub id: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Atom {
    Undefined,
    Null,
    Boolean(bool),
    Binary64(u64),
    Utf16(Units),
    BigInt(String),
    Symbol(u32),
    Reference(u32),
    Intrinsic(Intrinsic),
    Owner(OwnerRef),
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Utf16(Units),
    Symbol(u32),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyValue {
    Data { value: Atom, writable: bool },
    Accessor { get: Atom, set: Atom },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub key: Key,
    pub enumerable: bool,
    pub configurable: bool,
    pub value: PropertyValue,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeKind {
    Error,
    TypeError,
    RangeError,
    SyntaxError,
    ReferenceError,
    UriError,
    EvalError,
    AggregateError,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Ordinary,
    Array,
    NativeError {
        constructor: NativeKind,
        realm: Realm,
    },
    Callable {
        binding: Units,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    pub kind: Kind,
    pub prototype: Atom,
    pub properties: Vec<Property>,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Symbol {
    Unique(Option<Units>),
    Registered(Units),
    WellKnown(Units),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graph {
    pub roots: Vec<Atom>,
    pub objects: Vec<Object>,
    pub symbols: Vec<Symbol>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum externally imported namespaces, including empty graphs.
    pub imports: usize,
    pub objects: usize,
    pub properties: usize,
    pub units: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            imports: 4096,
            objects: 4096,
            properties: 262_144,
            units: 1_048_576,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Backend,
    UnknownDependency,
    UnsupportedNumericDomain,
    Capacity,
    InvalidReference,
    InvalidProperty,
    InvalidPrototype,
    InvalidBigInt,
    InvalidArray,
    UndeclaredCapability,
    ExternalOwnerReference,
    InvalidFrame,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub intrinsics: BTreeSet<Intrinsic>,
    pub callable_intrinsics: BTreeSet<Intrinsic>,
    pub callables: BTreeSet<Units>,
    pub well_known_symbols: BTreeSet<Units>,
}
#[derive(Debug, Clone)]
pub struct ValidatedInput {
    graph: Graph,
}
impl ValidatedInput {
    pub fn graph(&self) -> &Graph {
        &self.graph
    }
}
fn index_key(key: &[u16]) -> Option<u32> {
    if key.is_empty() || (key.len() > 1 && key[0] == 48) {
        return None;
    }
    let mut n = 0u32;
    for c in key {
        if !(48..=57).contains(c) {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(u32::from(c - 48))?;
    }
    if n == u32::MAX {
        None
    } else {
        Some(n)
    }
}
fn bigint_valid(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    !digits.is_empty()
        && digits.bytes().all(|c| c.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && value != "-0"
}
struct Check<'a> {
    graph: &'a Graph,
    caps: &'a Capabilities,
    limits: Limits,
    units: usize,
    properties: usize,
}
impl Check<'_> {
    fn units(&mut self, value: &[u16]) -> Result<(), Error> {
        self.units = self.units.checked_add(value.len()).ok_or(Error::Capacity)?;
        if self.units > self.limits.units {
            Err(Error::Capacity)
        } else {
            Ok(())
        }
    }
    fn atom(&mut self, value: &Atom) -> Result<(), Error> {
        match value {
            Atom::Symbol(id) if *id as usize >= self.graph.symbols.len() => {
                Err(Error::InvalidReference)
            }
            Atom::Reference(id) if *id as usize >= self.graph.objects.len() => {
                Err(Error::InvalidReference)
            }
            Atom::Utf16(value) => self.units(value),
            Atom::BigInt(value) => {
                if !bigint_valid(value) {
                    return Err(Error::InvalidBigInt);
                }
                self.units = self.units.checked_add(value.len()).ok_or(Error::Capacity)?;
                if self.units > self.limits.units {
                    Err(Error::Capacity)
                } else {
                    Ok(())
                }
            }
            Atom::Intrinsic(value) => {
                if !self.caps.intrinsics.contains(value) {
                    return Err(Error::UndeclaredCapability);
                }
                self.units(&value.name)
            }
            Atom::Owner(_) => Err(Error::ExternalOwnerReference),
            _ => Ok(()),
        }
    }
    fn callable(&self, value: &Atom) -> bool {
        match value {
            Atom::Undefined => true,
            Atom::Intrinsic(i) => self.caps.callable_intrinsics.contains(i),
            Atom::Reference(id) => self
                .graph
                .objects
                .get(*id as usize)
                .is_some_and(|o| matches!(o.kind, Kind::Callable { .. })),
            _ => false,
        }
    }
}
pub fn validate_external(
    graph: Graph,
    caps: &Capabilities,
    limits: Limits,
) -> Result<ValidatedInput, Error> {
    validate_ref(&graph, caps, limits)?;
    Ok(ValidatedInput { graph })
}
pub fn validate_ref(graph: &Graph, caps: &Capabilities, limits: Limits) -> Result<(), Error> {
    if graph.roots.len() > limits.properties
        || graph.symbols.len() > limits.objects
        || graph.objects.len() > limits.objects
        || graph.objects.len() > u32::MAX as usize
        || graph.symbols.len() > u32::MAX as usize
    {
        return Err(Error::Capacity);
    }
    let mut check = Check {
        graph,
        caps,
        limits,
        units: 0,
        properties: 0,
    };
    let mut shared = BTreeSet::new();
    for symbol in &graph.symbols {
        match symbol {
            Symbol::Unique(Some(v)) => check.units(v)?,
            Symbol::Unique(None) => (),
            Symbol::Registered(v) => {
                check.units(v)?;
                if !shared.insert(symbol.clone()) {
                    return Err(Error::InvalidReference);
                }
            }
            Symbol::WellKnown(v) => {
                check.units(v)?;
                if !caps.well_known_symbols.contains(v) {
                    return Err(Error::UndeclaredCapability);
                }
                if !shared.insert(symbol.clone()) {
                    return Err(Error::InvalidReference);
                }
            }
        }
    }
    for root in &graph.roots {
        check.atom(root)?;
    }
    for object in &graph.objects {
        if !matches!(
            object.prototype,
            Atom::Null | Atom::Reference(_) | Atom::Intrinsic(_)
        ) {
            return Err(Error::InvalidPrototype);
        }
        check.atom(&object.prototype)?;
        if let Kind::Callable { binding } = &object.kind {
            check.units(binding)?;
            if !caps.callables.contains(binding) {
                return Err(Error::UndeclaredCapability);
            }
        }
        check.properties = check
            .properties
            .checked_add(object.properties.len())
            .ok_or(Error::Capacity)?;
        if check.properties > limits.properties {
            return Err(Error::Capacity);
        }
        let (mut phase, mut last_index) = (0, None);
        let mut keys = BTreeSet::new();
        for property in &object.properties {
            if !keys.insert(property.key.clone()) {
                return Err(Error::InvalidProperty);
            }
            match &property.key {
                Key::Utf16(key) => {
                    check.units(key)?;
                    if let Some(index) = index_key(key) {
                        if phase != 0 || last_index.is_some_and(|old| index <= old) {
                            return Err(Error::InvalidProperty);
                        }
                        last_index = Some(index);
                    } else {
                        if phase == 2 {
                            return Err(Error::InvalidProperty);
                        }
                        phase = 1;
                    }
                }
                Key::Symbol(id) => {
                    phase = 2;
                    if *id as usize >= graph.symbols.len() {
                        return Err(Error::InvalidReference);
                    }
                }
            }
            match &property.value {
                PropertyValue::Data { value, .. } => check.atom(value)?,
                PropertyValue::Accessor { get, set } => {
                    check.atom(get)?;
                    check.atom(set)?;
                    if !check.callable(get) || !check.callable(set) {
                        return Err(Error::InvalidProperty);
                    }
                }
            }
        }
        if object.kind == Kind::Array {
            let length = object
                .properties
                .iter()
                .find(|p| p.key == Key::Utf16("length".encode_utf16().collect()))
                .ok_or(Error::InvalidArray)?;
            let PropertyValue::Data {
                value: Atom::Binary64(bits),
                ..
            } = length.value
            else {
                return Err(Error::InvalidArray);
            };
            let length_value = f64::from_bits(bits);
            if length.enumerable
                || length.configurable
                || !length_value.is_finite()
                || bits == (-0f64).to_bits()
                || length_value < 0.
                || length_value > u32::MAX as f64
                || length_value.fract() != 0.
            {
                return Err(Error::InvalidArray);
            }
            for property in &object.properties {
                if let Key::Utf16(key) = &property.key {
                    if index_key(key).is_some_and(|index| f64::from(index) >= length_value) {
                        return Err(Error::InvalidArray);
                    }
                }
            }
        }
    }
    let mut colors = vec![0u8; graph.objects.len()];
    for start in 0..graph.objects.len() {
        let mut path = vec![];
        let mut at = start;
        loop {
            if colors[at] == 1 {
                return Err(Error::InvalidPrototype);
            }
            if colors[at] == 2 {
                break;
            }
            colors[at] = 1;
            path.push(at);
            if let Atom::Reference(next) = graph.objects[at].prototype {
                at = next as usize;
            } else {
                break;
            }
        }
        for at in path {
            colors[at] = 2;
        }
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub namespace: u64,
    pub roots: Vec<Atom>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arena {
    objects: Vec<Object>,
    symbols: Vec<Symbol>,
    registered: BTreeMap<Units, u32>,
    well_known: BTreeMap<Units, u32>,
    imports: Vec<Import>,
    caps: Capabilities,
    limits: Limits,
}
impl Arena {
    pub fn new(caps: Capabilities, limits: Limits) -> Self {
        Self {
            objects: vec![],
            symbols: vec![],
            registered: BTreeMap::new(),
            well_known: BTreeMap::new(),
            imports: vec![],
            caps,
            limits,
        }
    }
    pub fn prevalidate(&self, graph: &Graph) -> Result<(), Error> {
        validate_ref(graph, &self.caps, self.limits)
    }
    pub fn validate(&self, graph: Graph) -> Result<ValidatedInput, Error> {
        validate_external(graph, &self.caps, self.limits)
    }
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }
    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }
    pub fn imports(&self) -> &[Import] {
        &self.imports
    }
    /// Trusted execution allocation. Existing Atom references are internal
    /// arena IDs, never directly supplied external frame IDs.
    pub fn allocate(&mut self, object: Object) -> Result<Atom, Error> {
        let id = u32::try_from(self.objects.len()).map_err(|_| Error::Capacity)?;
        let mut objects = self.objects.clone();
        objects.push(object.clone());
        let mut roots = self
            .imports
            .iter()
            .flat_map(|v| v.roots.iter().cloned())
            .collect::<Vec<_>>();
        roots.push(Atom::Reference(id));
        let graph = Graph {
            roots,
            objects,
            symbols: self.symbols.clone(),
        };
        validate_ref(&graph, &self.caps, self.limits)?;
        self.objects.push(object);
        Ok(Atom::Reference(id))
    }
    /// A descriptor operation for native data construction, not JS assignment
    /// or a proxy/setter shortcut. Actual property callbacks use RichContext.
    pub fn define_data(
        &mut self,
        receiver: &Atom,
        key: Key,
        value: Atom,
        writable: bool,
        enumerable: bool,
        configurable: bool,
    ) -> Result<(), Error> {
        let Atom::Reference(id) = receiver else {
            return Err(Error::InvalidReference);
        };
        let mut object = self
            .objects
            .get(*id as usize)
            .ok_or(Error::InvalidReference)?
            .clone();
        let property = Property {
            key,
            enumerable,
            configurable,
            value: PropertyValue::Data { value, writable },
        };
        if let Some(old) = object.properties.iter_mut().find(|p| p.key == property.key) {
            *old = property
        } else {
            object.properties.push(property)
        }
        let mut objects = self.objects.clone();
        objects[*id as usize] = object.clone();
        let mut roots = self
            .imports
            .iter()
            .flat_map(|v| v.roots.iter().cloned())
            .collect::<Vec<_>>();
        roots.push(receiver.clone());
        validate_ref(
            &Graph {
                roots,
                objects,
                symbols: self.symbols.clone(),
            },
            &self.caps,
            self.limits,
        )?;
        self.objects[*id as usize] = object;
        Ok(())
    }
    pub fn import(&mut self, input: ValidatedInput) -> Result<Import, Error> {
        self.absorb(input, true)
    }
    pub fn allocate_data_graph(&mut self, graph: Graph) -> Result<Vec<Atom>, Error> {
        let input = self.validate(graph)?;
        Ok(self.absorb(input, false)?.roots)
    }
    fn absorb(&mut self, input: ValidatedInput, record_namespace: bool) -> Result<Import, Error> {
        if record_namespace && self.imports.len() >= self.limits.imports {
            return Err(Error::Capacity);
        }
        // Revalidate in this arena's immutable capability/limit context; callers
        // cannot validate with a broader capability table then smuggle that token.
        let input = validate_external(input.graph, &self.caps, self.limits)?;
        let graph = input.graph;
        let end = self
            .objects
            .len()
            .checked_add(graph.objects.len())
            .ok_or(Error::Capacity)?;
        if end > self.limits.objects || end > u32::MAX as usize {
            return Err(Error::Capacity);
        }
        let base = self.objects.len() as u32;
        let mut symbols = self.symbols.clone();
        let mut registered = self.registered.clone();
        let mut well_known = self.well_known.clone();
        let mut ids = vec![];
        for symbol in graph.symbols {
            let old = match &symbol {
                Symbol::Unique(_) => None,
                Symbol::Registered(key) => registered.get(key).copied(),
                Symbol::WellKnown(key) => well_known.get(key).copied(),
            };
            let id = if let Some(id) = old {
                id
            } else {
                let id = u32::try_from(symbols.len()).map_err(|_| Error::Capacity)?;
                match &symbol {
                    Symbol::Registered(key) => {
                        registered.insert(key.clone(), id);
                    }
                    Symbol::WellKnown(key) => {
                        well_known.insert(key.clone(), id);
                    }
                    _ => (),
                }
                symbols.push(symbol);
                id
            };
            ids.push(id);
        }
        let map = |a: Atom| match a {
            Atom::Reference(id) => Atom::Reference(base + id),
            Atom::Symbol(id) => Atom::Symbol(ids[id as usize]),
            other => other,
        };
        let mut objects = vec![];
        for mut object in graph.objects {
            object.prototype = map(object.prototype);
            for property in &mut object.properties {
                if let Key::Symbol(id) = &mut property.key {
                    *id = ids[*id as usize];
                }
                property.value = match property.value.clone() {
                    PropertyValue::Data { value, writable } => PropertyValue::Data {
                        value: map(value),
                        writable,
                    },
                    PropertyValue::Accessor { get, set } => PropertyValue::Accessor {
                        get: map(get),
                        set: map(set),
                    },
                };
            }
            objects.push(object);
        }
        let import = Import {
            namespace: u64::try_from(self.imports.len()).map_err(|_| Error::Capacity)?,
            roots: graph.roots.into_iter().map(map).collect(),
        };
        let mut all_objects = self.objects.clone();
        all_objects.extend(objects.iter().cloned());
        let mut all_roots = self
            .imports
            .iter()
            .flat_map(|v| v.roots.iter().cloned())
            .collect::<Vec<_>>();
        all_roots.extend(import.roots.iter().cloned());
        validate_external(
            Graph {
                roots: all_roots,
                objects: all_objects,
                symbols: symbols.clone(),
            },
            &self.caps,
            self.limits,
        )?;
        // All fallible work precedes mutation. References/counters remain unchanged
        // for rejected graphs, including symbol registry insertion and namespaces.
        self.objects.extend(objects);
        self.symbols = symbols;
        self.registered = registered;
        self.well_known = well_known;
        if record_namespace {
            self.imports.push(import.clone());
        }
        Ok(import)
    }
}
#[path = "source_rich_value_wire.rs"]
pub mod wire;

#[path = "source_rich_intrinsics.rs"]
pub mod intrinsics;
#[path = "source_rich_throw.rs"]
pub mod thrown;

#[path = "source_rich_protocol.rs"]
pub mod protocol;

#[path = "source_rich_io.rs"]
pub mod io;
#[path = "source_rich_main.rs"]
pub mod main;

#[path = "source_rich_exposure.rs"]
pub mod exposure;

#[cfg(test)]
mod tests {
    use super::*;
    fn u(s: &str) -> Units {
        s.encode_utf16().collect()
    }
    fn data(name: &str, value: Atom) -> Property {
        Property {
            key: Key::Utf16(u(name)),
            enumerable: true,
            configurable: true,
            value: PropertyValue::Data {
                value,
                writable: true,
            },
        }
    }
    fn object(properties: Vec<Property>) -> Object {
        Object {
            kind: Kind::Ordinary,
            prototype: Atom::Null,
            properties,
        }
    }
    fn graph(properties: Vec<Property>) -> Graph {
        Graph {
            roots: vec![Atom::Reference(0)],
            objects: vec![object(properties)],
            symbols: vec![],
        }
    }
    #[test]
    fn empty_import_namespaces_have_an_atomic_practical_bound() {
        let limits = Limits {
            imports: 3,
            ..Limits::default()
        };
        let mut arena = Arena::new(Capabilities::default(), limits);
        let empty = Graph {
            roots: vec![],
            objects: vec![],
            symbols: vec![],
        };
        let input = arena.validate(empty).unwrap();
        for expected in 0..3 {
            assert_eq!(arena.import(input.clone()).unwrap().namespace, expected);
        }
        let before = arena.clone();
        assert_eq!(arena.import(input), Err(Error::Capacity));
        assert_eq!(arena, before);
        assert!(arena.objects.is_empty() && arena.symbols.is_empty());
    }
    #[test]
    fn external_ids_are_fresh_and_shared_references_stay_shared() {
        let mut arena = Arena::new(Capabilities::default(), Limits::default());
        let input = graph(vec![
            data("self", Atom::Reference(0)),
            data("alias", Atom::Reference(0)),
        ]);
        let first = arena
            .import(arena.validate(input.clone()).unwrap())
            .unwrap();
        let second = arena.import(arena.validate(input).unwrap()).unwrap();
        assert_eq!(first.roots, vec![Atom::Reference(0)]);
        assert_eq!(second.roots, vec![Atom::Reference(1)]);
        let PropertyValue::Data { value, .. } = &arena.objects()[1].properties[0].value else {
            panic!()
        };
        assert_eq!(value, &Atom::Reference(1));
    }
    #[test]
    fn invalid_import_is_atomic_including_namespace_and_symbols() {
        let mut arena = Arena::new(Capabilities::default(), Limits::default());
        let good = graph(vec![]);
        arena.import(arena.validate(good).unwrap()).unwrap();
        let before = arena.clone();
        let invalid = Graph {
            roots: vec![Atom::Reference(0)],
            objects: vec![],
            symbols: vec![Symbol::Registered(u("x"))],
        };
        assert_eq!(
            arena.validate(invalid).unwrap_err(),
            Error::InvalidReference
        );
        assert_eq!(arena, before);
    }
    #[test]
    fn a_broader_validator_cannot_authorize_arena_capabilities() {
        let cap = u("untrusted-verifier");
        let mut broad = Capabilities::default();
        broad.callables.insert(cap.clone());
        let mut g = graph(vec![]);
        g.objects[0].kind = Kind::Callable { binding: cap };
        let input = validate_external(g, &broad, Limits::default()).unwrap();
        let mut arena = Arena::new(Capabilities::default(), Limits::default());
        let before = arena.clone();
        assert_eq!(arena.import(input), Err(Error::UndeclaredCapability));
        assert_eq!(arena, before);
    }
    #[test]
    fn prototype_cycles_are_invalid_but_data_cycles_are_not() {
        let mut g = graph(vec![data("self", Atom::Reference(0))]);
        assert!(validate_external(g.clone(), &Capabilities::default(), Limits::default()).is_ok());
        g.objects[0].prototype = Atom::Reference(0);
        assert_eq!(
            validate_external(g, &Capabilities::default(), Limits::default()).unwrap_err(),
            Error::InvalidPrototype
        );
    }
    #[test]
    fn array_length_rejects_impossible_negative_zero_atomically() {
        let mut graph = Graph {
            roots: vec![Atom::Reference(0)],
            objects: vec![Object {
                kind: Kind::Array,
                prototype: Atom::Null,
                properties: vec![Property {
                    key: Key::Utf16(u("length")),
                    enumerable: false,
                    configurable: false,
                    value: PropertyValue::Data {
                        value: Atom::Binary64(0f64.to_bits()),
                        writable: true,
                    },
                }],
            }],
            symbols: vec![],
        };
        let mut arena = Arena::new(Capabilities::default(), Limits::default());
        arena
            .import(arena.validate(graph.clone()).unwrap())
            .unwrap();
        let before = arena.clone();
        graph.objects[0].properties[0].value = PropertyValue::Data {
            value: Atom::Binary64((-0f64).to_bits()),
            writable: true,
        };
        assert_eq!(
            arena.validate(graph.clone()).unwrap_err(),
            Error::InvalidArray
        );
        let encoded = wire::encode(&graph).unwrap();
        assert_eq!(
            wire::decode_external(&encoded, &Capabilities::default(), Limits::default())
                .unwrap_err(),
            Error::InvalidArray
        );
        assert_eq!(arena, before);
    }
    #[test]
    fn array_named_fields_keep_real_length_and_order() {
        let g = Graph {
            roots: vec![Atom::Reference(0)],
            objects: vec![Object {
                kind: Kind::Array,
                prototype: Atom::Null,
                properties: vec![
                    data("0", Atom::Boolean(false)),
                    Property {
                        key: Key::Utf16(u("length")),
                        enumerable: false,
                        configurable: false,
                        value: PropertyValue::Data {
                            value: Atom::Binary64(1f64.to_bits()),
                            writable: true,
                        },
                    },
                    data("code", Atom::Utf16(u("EIO"))),
                ],
            }],
            symbols: vec![],
        };
        assert!(validate_external(g, &Capabilities::default(), Limits::default()).is_ok());
    }
}
