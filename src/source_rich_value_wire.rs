use super::*;
const MAGIC: &[u8; 5] = b"TLRV\x01";
const MAX_BYTES: usize = 4 * 1024 * 1024;
struct Writer(Vec<u8>);
impl Writer {
    fn byte(&mut self, v: u8) {
        self.0.push(v)
    }
    fn count(&mut self, v: usize) -> Result<(), Error> {
        let n = u32::try_from(v).map_err(|_| Error::Capacity)?;
        self.0.extend(n.to_be_bytes());
        Ok(())
    }
    fn units(&mut self, v: &[u16]) -> Result<(), Error> {
        self.count(v.len())?;
        for c in v {
            self.0.extend(c.to_be_bytes());
        }
        Ok(())
    }
    fn realm(&mut self, v: &Realm) {
        self.byte(match v {
            Realm::HostIo => 0,
            Realm::SourceVm => 1,
            Realm::Shared => 2,
        })
    }
    fn intrinsic(&mut self, v: &Intrinsic) -> Result<(), Error> {
        self.realm(&v.realm);
        self.units(&v.name)
    }
    fn atom(&mut self, v: &Atom) -> Result<(), Error> {
        match v {
            Atom::Undefined => self.byte(0),
            Atom::Null => self.byte(1),
            Atom::Boolean(v) => {
                self.byte(2);
                self.byte(u8::from(*v));
            }
            Atom::Binary64(v) => {
                self.byte(3);
                self.0.extend(v.to_be_bytes());
            }
            Atom::Utf16(v) => {
                self.byte(4);
                self.units(v)?;
            }
            Atom::BigInt(v) => {
                self.byte(5);
                self.units(&v.encode_utf16().collect::<Vec<_>>())?;
            }
            Atom::Symbol(v) => {
                self.byte(6);
                self.0.extend(v.to_be_bytes());
            }
            Atom::Reference(v) => {
                self.byte(7);
                self.0.extend(v.to_be_bytes());
            }
            Atom::Intrinsic(v) => {
                self.byte(8);
                self.intrinsic(v)?;
            }
            Atom::Owner(v) => {
                self.byte(9);
                self.units(&v.domain)?;
                self.0.extend(v.id.to_be_bytes());
            }
        }
        if self.0.len() > MAX_BYTES {
            return Err(Error::Capacity);
        }
        Ok(())
    }
}
fn native(kind: &NativeKind) -> u8 {
    match kind {
        NativeKind::Error => 0,
        NativeKind::TypeError => 1,
        NativeKind::RangeError => 2,
        NativeKind::SyntaxError => 3,
        NativeKind::ReferenceError => 4,
        NativeKind::UriError => 5,
        NativeKind::EvalError => 6,
        NativeKind::AggregateError => 7,
    }
}
pub fn encode(graph: &Graph) -> Result<Vec<u8>, Error> {
    let mut w = Writer(MAGIC.to_vec());
    w.count(graph.roots.len())?;
    for root in &graph.roots {
        w.atom(root)?;
    }
    w.count(graph.symbols.len())?;
    for symbol in &graph.symbols {
        match symbol {
            Symbol::Unique(None) => w.byte(0),
            Symbol::Unique(Some(v)) => {
                w.byte(1);
                w.units(v)?;
            }
            Symbol::Registered(v) => {
                w.byte(2);
                w.units(v)?;
            }
            Symbol::WellKnown(v) => {
                w.byte(3);
                w.units(v)?;
            }
        }
    }
    w.count(graph.objects.len())?;
    for object in &graph.objects {
        match &object.kind {
            Kind::Ordinary => w.byte(0),
            Kind::Array => w.byte(1),
            Kind::NativeError { constructor, realm } => {
                w.byte(2);
                w.byte(native(constructor));
                w.realm(realm);
            }
            Kind::Callable { binding } => {
                w.byte(3);
                w.units(binding)?;
            }
        }
        w.atom(&object.prototype)?;
        w.count(object.properties.len())?;
        for p in &object.properties {
            match &p.key {
                Key::Utf16(v) => {
                    w.byte(0);
                    w.units(v)?;
                }
                Key::Symbol(v) => {
                    w.byte(1);
                    w.0.extend(v.to_be_bytes());
                }
            }
            w.byte(u8::from(p.enumerable) | (u8::from(p.configurable) << 1));
            match &p.value {
                PropertyValue::Data { value, writable } => {
                    w.byte(0);
                    w.byte(u8::from(*writable));
                    w.atom(value)?;
                }
                PropertyValue::Accessor { get, set } => {
                    w.byte(1);
                    w.atom(get)?;
                    w.atom(set)?;
                }
            }
        }
    }
    if w.0.len() > MAX_BYTES {
        return Err(Error::Capacity);
    }
    Ok(w.0)
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    limits: Limits,
    units: usize,
    properties: usize,
}
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], Error> {
        let end = self.at.checked_add(n).ok_or(Error::InvalidFrame)?;
        let bytes = self.bytes.get(self.at..end).ok_or(Error::InvalidFrame)?;
        self.at = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> Result<bool, Error> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::InvalidFrame),
        }
    }
    fn count(&mut self, max: usize) -> Result<usize, Error> {
        let n = u32::from_be_bytes(self.take(4)?.try_into().unwrap()) as usize;
        if n > max {
            Err(Error::Capacity)
        } else {
            Ok(n)
        }
    }
    fn id(&mut self) -> Result<u32, Error> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn units(&mut self) -> Result<Units, Error> {
        let len = self.count(self.limits.units)?;
        self.units = self.units.checked_add(len).ok_or(Error::Capacity)?;
        if self.units > self.limits.units {
            return Err(Error::Capacity);
        }
        let bytes = self.take(len.checked_mul(2).ok_or(Error::Capacity)?)?;
        Ok(bytes
            .chunks_exact(2)
            .map(|v| u16::from_be_bytes([v[0], v[1]]))
            .collect())
    }
    fn realm(&mut self) -> Result<Realm, Error> {
        match self.byte()? {
            0 => Ok(Realm::HostIo),
            1 => Ok(Realm::SourceVm),
            2 => Ok(Realm::Shared),
            _ => Err(Error::InvalidFrame),
        }
    }
    fn intrinsic(&mut self) -> Result<Intrinsic, Error> {
        Ok(Intrinsic {
            realm: self.realm()?,
            name: self.units()?,
        })
    }
    fn atom(&mut self) -> Result<Atom, Error> {
        Ok(match self.byte()? {
            0 => Atom::Undefined,
            1 => Atom::Null,
            2 => Atom::Boolean(self.bool()?),
            3 => Atom::Binary64(u64::from_be_bytes(self.take(8)?.try_into().unwrap())),
            4 => Atom::Utf16(self.units()?),
            5 => {
                Atom::BigInt(String::from_utf16(&self.units()?).map_err(|_| Error::InvalidBigInt)?)
            }
            6 => Atom::Symbol(self.id()?),
            7 => Atom::Reference(self.id()?),
            8 => Atom::Intrinsic(self.intrinsic()?),
            9 => {
                let domain = self.units()?;
                Atom::Owner(OwnerRef {
                    domain,
                    id: u64::from_be_bytes(self.take(8)?.try_into().unwrap()),
                })
            }
            _ => return Err(Error::InvalidFrame),
        })
    }
}
fn parse_native(tag: u8) -> Result<NativeKind, Error> {
    Ok(match tag {
        0 => NativeKind::Error,
        1 => NativeKind::TypeError,
        2 => NativeKind::RangeError,
        3 => NativeKind::SyntaxError,
        4 => NativeKind::ReferenceError,
        5 => NativeKind::UriError,
        6 => NativeKind::EvalError,
        7 => NativeKind::AggregateError,
        _ => return Err(Error::InvalidFrame),
    })
}
pub fn decode_external(
    bytes: &[u8],
    caps: &Capabilities,
    limits: Limits,
) -> Result<ValidatedInput, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Capacity);
    }
    let mut r = Reader {
        bytes,
        at: 0,
        limits,
        units: 0,
        properties: 0,
    };
    if r.take(5)? != MAGIC {
        return Err(Error::InvalidFrame);
    }
    let n = r.count(limits.properties)?;
    let mut roots = Vec::with_capacity(n);
    for _ in 0..n {
        roots.push(r.atom()?);
    }
    let n = r.count(limits.objects)?;
    let mut symbols = Vec::with_capacity(n);
    for _ in 0..n {
        symbols.push(match r.byte()? {
            0 => Symbol::Unique(None),
            1 => Symbol::Unique(Some(r.units()?)),
            2 => Symbol::Registered(r.units()?),
            3 => Symbol::WellKnown(r.units()?),
            _ => return Err(Error::InvalidFrame),
        });
    }
    let n = r.count(limits.objects)?;
    let mut objects = Vec::with_capacity(n);
    for _ in 0..n {
        let kind = match r.byte()? {
            0 => Kind::Ordinary,
            1 => Kind::Array,
            2 => Kind::NativeError {
                constructor: parse_native(r.byte()?)?,
                realm: r.realm()?,
            },
            3 => Kind::Callable {
                binding: r.units()?,
            },
            _ => return Err(Error::InvalidFrame),
        };
        let prototype = r.atom()?;
        let n = r.count(limits.properties)?;
        r.properties = r.properties.checked_add(n).ok_or(Error::Capacity)?;
        if r.properties > limits.properties {
            return Err(Error::Capacity);
        }
        let mut properties = Vec::with_capacity(n);
        for _ in 0..n {
            let key = match r.byte()? {
                0 => Key::Utf16(r.units()?),
                1 => Key::Symbol(r.id()?),
                _ => return Err(Error::InvalidFrame),
            };
            let flags = r.byte()?;
            if flags & !3 != 0 {
                return Err(Error::InvalidFrame);
            }
            let value = match r.byte()? {
                0 => PropertyValue::Data {
                    writable: r.bool()?,
                    value: r.atom()?,
                },
                1 => PropertyValue::Accessor {
                    get: r.atom()?,
                    set: r.atom()?,
                },
                _ => return Err(Error::InvalidFrame),
            };
            properties.push(Property {
                key,
                enumerable: flags & 1 != 0,
                configurable: flags & 2 != 0,
                value,
            });
        }
        objects.push(Object {
            kind,
            prototype,
            properties,
        });
    }
    if r.at != bytes.len() {
        return Err(Error::InvalidFrame);
    }
    let graph = Graph {
        roots,
        objects,
        symbols,
    };
    if encode(&graph)? != bytes {
        return Err(Error::InvalidFrame);
    }
    validate_external(graph, caps, limits)
}
#[cfg(test)]
mod source_tests {
    use super::*;
    fn units(v: &str) -> Units {
        v.encode_utf16().collect()
    }
    fn bytes(s: &str) -> Vec<u8> {
        assert_eq!(s.len() % 2, 0);
        s.as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect()
    }
    fn caps() -> Capabilities {
        let mut caps = Capabilities::default();
        for line in include_str!("../fixtures/rich_source_capabilities.tsv").lines() {
            let f = line.splitn(3, '\t').collect::<Vec<_>>();
            match f[0] {
                "I" => {
                    caps.intrinsics.insert(Intrinsic {
                        realm: match f[1] {
                            "0" => Realm::HostIo,
                            "1" => Realm::SourceVm,
                            "2" => Realm::Shared,
                            _ => panic!(),
                        },
                        name: units(f[2]),
                    });
                }
                "C" => {
                    caps.callables.insert(units(f[1]));
                }
                _ => panic!(),
            }
        }
        caps
    }
    #[test]
    fn actual_source_graphs_cross_decode_encode_and_import_without_alias_loss() {
        let caps = caps();
        let mut count = 0;
        for line in include_str!("../fixtures/rich_source_graphs.tsv").lines() {
            let (name, hex) = line.split_once('\t').unwrap();
            let bytes = bytes(hex);
            let input = decode_external(&bytes, &caps, Limits::default())
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(
                encode(input.graph()).unwrap(),
                bytes,
                "{name} independent JS encoder"
            );
            let length = input.graph().objects.len();
            let mut arena = Arena::new(caps.clone(), Limits::default());
            let first = arena.import(input.clone()).unwrap();
            let second = arena.import(input).unwrap();
            assert_eq!(first.namespace, 0);
            assert_eq!(second.namespace, 1);
            if let (Atom::Reference(a), Atom::Reference(b)) = (&first.roots[0], &second.roots[0]) {
                assert_eq!(*b as usize, *a as usize + length, "{name} fresh import IDs");
            }
            count += 1;
        }
        assert_eq!(count, 76);
    }
    #[test]
    fn malformed_wire_does_not_enter_an_existing_namespace() {
        let caps = caps();
        let first = include_str!("../fixtures/rich_source_graphs.tsv")
            .lines()
            .next()
            .unwrap();
        let (_, hex) = first.split_once('\t').unwrap();
        let encoded = bytes(hex);
        let mut arena = Arena::new(caps.clone(), Limits::default());
        arena
            .import(decode_external(&encoded, &caps, Limits::default()).unwrap())
            .unwrap();
        let before = arena.clone();
        for n in 0..encoded.len() {
            assert!(decode_external(&encoded[..n], &caps, Limits::default()).is_err());
            assert_eq!(arena, before);
        }
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert_eq!(
            decode_external(&trailing, &caps, Limits::default()).unwrap_err(),
            Error::InvalidFrame
        );
        let mut wrong = encoded;
        wrong[4] = 0;
        assert_eq!(
            decode_external(&wrong, &caps, Limits::default()).unwrap_err(),
            Error::InvalidFrame
        );
        assert_eq!(arena, before);
    }
}
