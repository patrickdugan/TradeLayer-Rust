//! Persistent portable exposure IDs. Internal allocations/input namespaces are
//! never interpreted as external authority, and equal diagnostics don't merge.
use super::*;
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Exposure {
    object_ids: Vec<u32>,
    symbol_ids: Vec<u32>,
    object_map: Vec<Option<u32>>,
    symbol_map: Vec<Option<u32>>,
}
impl Exposure {
    fn atom(&mut self, arena: &Arena, value: Atom) -> Result<Atom, Error> {
        match value {
            Atom::Reference(id) => {
                if id as usize >= arena.objects.len() {
                    return Err(Error::InvalidReference);
                }
                self.object_map.resize(arena.objects.len(), None);
                let mapped = if let Some(id) = self.object_map[id as usize] {
                    id
                } else {
                    let next = u32::try_from(self.object_ids.len()).map_err(|_| Error::Capacity)?;
                    self.object_map[id as usize] = Some(next);
                    self.object_ids.push(id);
                    next
                };
                Ok(Atom::Reference(mapped))
            }
            Atom::Symbol(id) => {
                if id as usize >= arena.symbols.len() {
                    return Err(Error::InvalidReference);
                }
                self.symbol_map.resize(arena.symbols.len(), None);
                let mapped = if let Some(id) = self.symbol_map[id as usize] {
                    id
                } else {
                    let next = u32::try_from(self.symbol_ids.len()).map_err(|_| Error::Capacity)?;
                    self.symbol_map[id as usize] = Some(next);
                    self.symbol_ids.push(id);
                    next
                };
                Ok(Atom::Symbol(mapped))
            }
            Atom::Owner(_) => Err(Error::ExternalOwnerReference),
            other => Ok(other),
        }
    }
    fn graph(&mut self, arena: &Arena, roots: &[Atom]) -> Result<Graph, Error> {
        let roots = roots
            .iter()
            .cloned()
            .map(|v| self.atom(arena, v))
            .collect::<Result<Vec<_>, _>>()?;
        let mut objects = vec![];
        let mut at = 0;
        while at < self.object_ids.len() {
            let mut object = arena.objects[self.object_ids[at] as usize].clone();
            // The genuine observer obtains descriptors/own keys first and discovers
            // their values in that order, then observes the prototype.
            for property in &mut object.properties {
                if let Key::Symbol(id) = property.key {
                    let Atom::Symbol(mapped) = self.atom(arena, Atom::Symbol(id))? else {
                        unreachable!()
                    };
                    property.key = Key::Symbol(mapped)
                }
                property.value = match property.value.clone() {
                    PropertyValue::Data { value, writable } => PropertyValue::Data {
                        value: self.atom(arena, value)?,
                        writable,
                    },
                    PropertyValue::Accessor { get, set } => PropertyValue::Accessor {
                        get: self.atom(arena, get)?,
                        set: self.atom(arena, set)?,
                    },
                };
            }
            object.prototype = self.atom(arena, object.prototype)?;
            objects.push(object);
            at += 1;
        }
        let symbols = self
            .symbol_ids
            .iter()
            .map(|id| arena.symbols[*id as usize].clone())
            .collect();
        Ok(Graph {
            roots,
            objects,
            symbols,
        })
    }
    pub fn expose(&mut self, arena: &Arena, roots: &[Atom]) -> Result<Graph, Error> {
        let mut next = self.clone();
        let graph = next.graph(arena, roots)?;
        validate_ref(&graph, &arena.caps, arena.limits)?;
        wire::encoded_size(&graph)?;
        *self = next;
        Ok(graph)
    }
    pub fn object_bindings(&self) -> &[u32] {
        &self.object_ids
    }
    pub fn symbol_bindings(&self) -> &[u32] {
        &self.symbol_ids
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn object(properties: Vec<Property>) -> Object {
        Object {
            kind: Kind::Ordinary,
            prototype: Atom::Null,
            properties,
        }
    }
    fn property(name: &str, value: Atom) -> Property {
        Property {
            key: Key::Utf16(name.encode_utf16().collect()),
            enumerable: true,
            configurable: true,
            value: PropertyValue::Data {
                value,
                writable: true,
            },
        }
    }
    #[test]
    fn unrelated_internal_allocations_do_not_define_public_ids() {
        let mut arena = Arena::new(Capabilities::default(), Limits::default());
        arena.allocate(object(vec![])).unwrap();
        let child = arena.allocate(object(vec![])).unwrap();
        let root = arena
            .allocate(object(vec![
                property("a", child.clone()),
                property("b", child),
            ]))
            .unwrap();
        let mut exposure = Exposure::default();
        let graph = exposure.expose(&arena, &[root]).unwrap();
        assert_eq!(graph.roots, vec![Atom::Reference(0)]);
        assert_eq!(exposure.object_bindings(), &[2, 1]);
        assert_eq!(
            graph.objects[0].properties[0].value,
            graph.objects[0].properties[1].value
        );
    }
    #[test]
    fn identical_objects_keep_separate_identity_across_receipts() {
        let mut arena = Arena::new(Capabilities::default(), Limits::default());
        let first = arena.allocate(object(vec![])).unwrap();
        let second = arena.allocate(object(vec![])).unwrap();
        let mut exposure = Exposure::default();
        let a = exposure
            .expose(&arena, std::slice::from_ref(&first))
            .unwrap();
        let b = exposure.expose(&arena, &[second]).unwrap();
        assert_eq!(a.roots, vec![Atom::Reference(0)]);
        assert_eq!(b.roots, vec![Atom::Reference(1)]);
        assert_eq!(b.objects.len(), 2);
        let before = exposure.clone();
        assert_eq!(
            exposure.expose(&arena, &[Atom::Reference(999)]),
            Err(Error::InvalidReference)
        );
        assert_eq!(exposure, before);
    }
    #[test]
    fn data_cycles_and_unique_symbols_remain_causal_and_distinct() {
        let mut arena = Arena::new(Capabilities::default(), Limits::default());
        let graph = Graph {
            roots: vec![Atom::Reference(0)],
            objects: vec![object(vec![
                property("self", Atom::Reference(0)),
                property("one", Atom::Symbol(1)),
                property("two", Atom::Symbol(0)),
            ])],
            symbols: vec![Symbol::Unique(None), Symbol::Unique(None)],
        };
        let input = arena.validate(graph).unwrap();
        let root = arena.import(input).unwrap().roots[0].clone();
        let mut exposure = Exposure::default();
        let graph = exposure.expose(&arena, &[root]).unwrap();
        assert_eq!(exposure.symbol_bindings(), &[1, 0]);
        assert_eq!(graph.symbols.len(), 2);
        assert_eq!(
            graph.objects[0].properties[0].value,
            PropertyValue::Data {
                value: Atom::Reference(0),
                writable: true
            }
        );
    }
}
