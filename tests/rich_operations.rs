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
fn capabilities() -> Capabilities {
    let mut c = value::intrinsics::capabilities();
    for line in include_str!("../fixtures/rich_operations_capabilities.tsv").lines() {
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
fn original_source_rich_values_preserve_property_coercion_and_constructor_results() {
    let c = capabilities();
    let mut count = 0;
    for line in include_str!("../fixtures/rich_operations.tsv").lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let name = f[0];
        let mut arena = Arena::new(c.clone(), Limits::default());
        let input = wire::decode_external(&bytes(f[3]), &c, Limits::default()).unwrap();
        let import = arena.import(input).unwrap();
        let raw = import.roots[0].clone();
        let mut context = intrinsics::LiteralIntrinsics::new(42, renderer);
        let result = match f[1] {
            "name" | "message" | "code" => {
                get_property(&mut arena, &mut context, raw, &Key::Utf16(units(f[1])), 0)
            }
            "template" => implicit_to_string(&mut arena, &mut context, raw, 0).map(Atom::Utf16),
            "errorToString" => error_to_string(&mut arena, &mut context, raw, 0).map(Atom::Utf16),
            "newErrorMessage" => get_property(
                &mut arena,
                &mut context,
                raw,
                &Key::Utf16(units("message")),
                0,
            )
            .and_then(|v| {
                new_native_error(
                    &mut arena,
                    &mut context,
                    NativeKind::Error,
                    Realm::SourceVm,
                    v,
                )
            })
            .map(|v| v.value),
            _ => panic!(),
        };
        let (transport, result) = match result {
            Ok(v) => ("Return", v),
            Err(Abrupt::Source(v)) => {
                assert_eq!(v.epoch, 42);
                ("Throw", v.value)
            }
            Err(e) => panic!("{name} unexpected local result {e:?}"),
        };
        assert_eq!(transport, f[2], "{name}");
        let graph = Graph {
            roots: vec![result],
            objects: arena.objects().to_vec(),
            symbols: arena.symbols().to_vec(),
        };
        let actual = wire::encode(&graph).unwrap();
        let expected = bytes(f[4]);
        assert_eq!(
            actual, expected,
            "{name} complete graph including aliases/realms/descriptors"
        );
        count += 1;
    }
    assert_eq!(count, 225);
}
