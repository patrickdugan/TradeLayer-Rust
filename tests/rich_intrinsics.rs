#[path = "support/source_number_adapter.rs"]
mod numbers;
pub use tradelayer_core::source_rich_value as value;
use value::thrown::*;
use value::*;
fn units(s: &str) -> Units {
    s.encode_utf16().collect()
}
fn decoded(s: &str) -> Units {
    s.as_bytes()
        .chunks_exact(4)
        .map(|v| u16::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
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
#[test]
fn genuine_native_intrinsic_prototypes_and_functions_match_both_realms() {
    let mut count = 0;
    for line in include_str!("../fixtures/rich_intrinsic_controls.tsv").lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let realm = if f[0] == "HostIO" {
            Realm::HostIo
        } else {
            Realm::SourceVm
        };
        let mut arena = Arena::new(intrinsics::capabilities(), Limits::default());
        let mut context = intrinsics::LiteralIntrinsics::new(42, renderer);
        let value = Atom::Intrinsic(Intrinsic {
            realm: realm.clone(),
            name: units(f[1]),
        });
        let result = match f[2] {
            "implicit" => implicit_to_string(&mut arena, &mut context, value, 0).map(Atom::Utf16),
            "brand" => context.call_pinned(
                &mut arena,
                &Atom::Intrinsic(Intrinsic {
                    realm: realm.clone(),
                    name: units("Object.prototype.toString"),
                }),
                &value,
                &[],
                0,
            ),
            "functionText" => context.call_pinned(
                &mut arena,
                &Atom::Intrinsic(Intrinsic {
                    realm: realm.clone(),
                    name: units("Function.prototype.toString"),
                }),
                &value,
                &[],
                0,
            ),
            "name" | "length" => {
                get_property(&mut arena, &mut context, value, &Key::Utf16(units(f[2])), 0)
            }
            _ => panic!(),
        };
        match (f[3], result) {
            ("Return", Ok(value)) => {
                let expected = match f[4] {
                    "Undefined" => Atom::Undefined,
                    "UTF16" => Atom::Utf16(decoded(f[5])),
                    "Binary64" => Atom::Binary64(u64::from_str_radix(f[5], 16).unwrap()),
                    _ => panic!(),
                };
                assert_eq!(value, expected, "{} {} {}", f[0], f[1], f[2])
            }
            ("Throw", Err(Abrupt::Source(error))) => {
                assert_eq!(error.epoch, 42);
                let Atom::Reference(id) = error.value else {
                    panic!()
                };
                let object = &arena.objects()[id as usize];
                assert_eq!(
                    object.kind,
                    Kind::NativeError {
                        constructor: NativeKind::TypeError,
                        realm: realm.clone()
                    }
                );
                let message = get_property(
                    &mut arena,
                    &mut context,
                    Atom::Reference(id),
                    &Key::Utf16(units("message")),
                    0,
                )
                .unwrap();
                assert_eq!(
                    message,
                    Atom::Utf16(decoded(f[5])),
                    "{} {} {}",
                    f[0],
                    f[1],
                    f[2]
                );
            }
            (_, result) => panic!("{} {} {} wrong completion {result:?}", f[0], f[1], f[2]),
        }
        count += 1;
    }
    assert_eq!(count, 320);
}
