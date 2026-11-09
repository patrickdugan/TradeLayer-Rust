//! Tests for the Nova verifier math layer (`tradelayer_core::zk_nova`).
//!
//! Vector files under `fixtures/zk_nova/` come from halo2curves 0.9.0, nova-snark 0.76.0, sha2 and
//! sha3. A missing vector file fails the matching test rather than being skipped.

// The algebraic-law tests compare an expression with itself on purpose (a - a, p + q vs q + p).
#![allow(clippy::eq_op, clippy::useless_vec)]

use std::collections::BTreeMap;
use std::time::Instant;

use tradelayer_core::zk_nova::curve::{
    batch_normalize, Affine, BaseOf, Curve, CurveError, Pallas, Projective, ScalarOf, Vesta,
};
use tradelayer_core::zk_nova::field::{Field, FieldError, FieldParams, Fp, FpParams, Fq, FqParams};
use tradelayer_core::zk_nova::h2c::{
    expand_message_xmd_sha256, from_label, hash_to_curve, sha256_digest, H2cCurve, H2cError,
    HashToCurve, Svdw, FROM_LABEL_DOMAIN_PREFIX, MAX_FROM_LABEL_POINTS,
};
use tradelayer_core::zk_nova::keccak::{keccak256, sha3_256, shake256, Keccak256, Shake256};
use tradelayer_core::zk_nova::msm::{msm, msm_with_threads, MsmError};

// ---------------------------------------------------------------------------------------------
// helpers

fn hex_decode(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "odd hex length");
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).expect("hex digit"))
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn array32(text: &str) -> [u8; 32] {
    hex_decode(text).try_into().expect("32 bytes")
}

fn fixture_rows(name: &str) -> Vec<Vec<String>> {
    let path = format!("{}/fixtures/zk_nova/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect()
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn bytes<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        for chunk in out.chunks_mut(8) {
            let word = self.next().to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
        out
    }

    fn field<P: FieldParams>(&mut self) -> Field<P> {
        Field::<P>::from_uniform_bytes_64(&self.bytes::<64>())
    }
}

fn fe<P: FieldParams>(text: &str) -> Field<P> {
    Field::<P>::from_repr(&array32(text)).expect("canonical fixture element")
}

fn point<C: Curve>(token: &str) -> Affine<C> {
    if token == "inf" {
        return Affine::identity();
    }
    let (x, y) = token.split_once(':').expect("x:y");
    Affine::new(fe(x), fe(y)).expect("fixture point on curve")
}

// ---------------------------------------------------------------------------------------------
// fields

fn modulus_bytes<P: FieldParams>() -> [u8; 32] {
    let mut out = [0u8; 32];
    for (chunk, limb) in out.chunks_mut(8).zip(P::MODULUS) {
        chunk.copy_from_slice(&limb.to_le_bytes());
    }
    out
}

fn run_field_vectors<P: FieldParams>(tag: &str, rows: &[Vec<String>]) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows.iter().filter(|row| row[0] == tag) {
        let op = row[1].as_str();
        *counts.entry(op.to_string()).or_default() += 1;
        let ctx = format!("{tag} {op} {:?}", &row[2..]);
        match op {
            "const" => {
                let value = &row[3];
                match row[2].as_str() {
                    "modulus_le" => assert_eq!(hex_encode(&modulus_bytes::<P>()), *value),
                    "num_bits" => {
                        let top = P::MODULUS[3];
                        assert_eq!(
                            (255 - (top.leading_zeros() as usize - 1)).to_string(),
                            *value
                        );
                    }
                    "s" => assert_eq!(Field::<P>::S.to_string(), *value),
                    "root_of_unity" => {
                        assert_eq!(hex_encode(&Field::<P>::ROOT_OF_UNITY.to_repr()), *value)
                    }
                    "root_of_unity_inv" => {
                        let inv = Field::<P>::ROOT_OF_UNITY.invert().unwrap();
                        assert_eq!(hex_encode(&inv.to_repr()), *value);
                    }
                    "delta" => {
                        // delta = generator^(2^S)
                        let delta = Field::<P>::from_u64(5).pow(&[1u64 << Field::<P>::S]);
                        assert_eq!(hex_encode(&delta.to_repr()), *value);
                    }
                    "multiplicative_generator" => {
                        assert_eq!(hex_encode(&Field::<P>::from_u64(5).to_repr()), *value)
                    }
                    "two_inv" => {
                        let inv = Field::<P>::from_u64(2).invert().unwrap();
                        assert_eq!(hex_encode(&inv.to_repr()), *value);
                    }
                    other => panic!("unknown const {other}"),
                }
            }
            "add" | "sub" | "mul" => {
                let a = fe::<P>(&row[2]);
                let b = fe::<P>(&row[3]);
                let expected = fe::<P>(&row[4]);
                let got = match op {
                    "add" => a + b,
                    "sub" => a - b,
                    _ => a * b,
                };
                assert_eq!(got, expected, "{ctx}");
            }
            "sqr" => assert_eq!(fe::<P>(&row[2]).square(), fe::<P>(&row[3]), "{ctx}"),
            "neg" => assert_eq!(-fe::<P>(&row[2]), fe::<P>(&row[3]), "{ctx}"),
            "inv" => {
                let a = fe::<P>(&row[2]);
                if row[3] == "none" {
                    assert!(a.invert().is_none(), "{ctx}");
                } else {
                    assert_eq!(a.invert(), Some(fe::<P>(&row[3])), "{ctx}");
                }
            }
            "sqrt" => {
                let a = fe::<P>(&row[2]);
                if row[3] == "none" {
                    assert!(a.sqrt().is_none(), "{ctx}");
                    assert_eq!(a.legendre(), -1, "{ctx}");
                } else {
                    assert_eq!(a.sqrt(), Some(fe::<P>(&row[3])), "{ctx}");
                    assert!(a.is_square(), "{ctx}");
                }
            }
            "from_uniform64" => {
                let bytes: [u8; 64] = hex_decode(&row[2]).try_into().unwrap();
                assert_eq!(
                    Field::<P>::from_uniform_bytes_64(&bytes),
                    fe::<P>(&row[3]),
                    "{ctx}"
                );
            }
            "from_uniform48" => {
                let bytes: [u8; 48] = hex_decode(&row[2]).try_into().unwrap();
                assert_eq!(
                    Field::<P>::from_uniform_bytes_48(&bytes),
                    fe::<P>(&row[3]),
                    "{ctx}"
                );
            }
            "from_repr" => {
                let parsed = Field::<P>::from_repr(&array32(&row[2]));
                if row[3] == "none" {
                    assert_eq!(parsed, Err(FieldError::NonCanonical), "{ctx}");
                } else {
                    assert_eq!(hex_encode(&parsed.unwrap().to_repr()), row[3], "{ctx}");
                }
            }
            other => panic!("unknown field op {other}"),
        }
    }
    assert!(counts.len() >= 11, "{tag}: missing ops, saw {counts:?}");
}

#[test]
fn field_vectors_fields_tsv() {
    let rows = fixture_rows("fields.tsv");
    run_field_vectors::<FpParams>("fp", &rows);
    run_field_vectors::<FqParams>("fq", &rows);
}

#[test]
fn moduli_and_constants_match_halo2curves_source() {
    // halo2curves 0.9.0 pasta/fp.rs and pasta/fq.rs `modulus = ...`
    assert_eq!(
        hex_encode(&modulus_bytes::<FpParams>()),
        hex_encode(
            &hex_decode("40000000000000000000000000000000224698fc094cf91b992d30ed00000001")
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
        )
    );
    assert_eq!(
        hex_encode(&modulus_bytes::<FqParams>()),
        hex_encode(
            &hex_decode("40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001")
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
        )
    );
    assert_eq!(Fp::S, 32);
    assert_eq!(Fq::S, 32);
    // pasta_curves ROOT_OF_UNITY (big-endian hex), the generator 5 raised to the odd part t.
    let be = |f: &dyn Fn() -> [u8; 32]| {
        let mut bytes = f();
        bytes.reverse();
        hex_encode(&bytes)
    };
    assert_eq!(
        be(&|| Fp::ROOT_OF_UNITY.to_repr()),
        "2bce74deac30ebda362120830561f81aea322bf2b7bb7584bdad6fabd87ea32f"
    );
    assert_eq!(
        be(&|| Fq::ROOT_OF_UNITY.to_repr()),
        "2de6a9b8746d3f589e5c4dfd492ae26e9bb97ea3c106f049a70e2c1102b6d05f"
    );
    // The root has order exactly 2^32.
    let mut x = Fp::ROOT_OF_UNITY;
    for _ in 0..31 {
        x = x.square();
    }
    assert_eq!(x, -Fp::ONE);
    let mut y = Fq::ROOT_OF_UNITY;
    for _ in 0..31 {
        y = y.square();
    }
    assert_eq!(y, -Fq::ONE);
}

fn field_laws<P: FieldParams>(seed: u64) {
    let mut rng = Rng(seed);
    for _ in 0..200 {
        let a = rng.field::<P>();
        let b = rng.field::<P>();
        let c = rng.field::<P>();
        assert_eq!(a + b, b + a);
        assert_eq!(a * b, b * a);
        assert_eq!((a + b) + c, a + (b + c));
        assert_eq!((a * b) * c, a * (b * c));
        assert_eq!(a * (b + c), a * b + a * c);
        assert_eq!(a - a, Field::<P>::ZERO);
        assert_eq!(a + (-a), Field::<P>::ZERO);
        assert_eq!(a * Field::<P>::ONE, a);
        assert_eq!(a.square(), a * a);
        assert_eq!(a.double(), a + a);
        assert_eq!((a - b) + b, a);
        if !a.is_zero() {
            assert_eq!(a * a.invert().unwrap(), Field::<P>::ONE);
        }
        // sqrt agrees with the Legendre symbol and squares back
        let sq = a.square();
        let root = sq.sqrt().expect("a square has a root");
        assert!(root == a || root == -a);
        assert_eq!(sq.legendre(), if a.is_zero() { 0 } else { 1 });
        // pow
        assert_eq!(a.pow(&[3]), a * a * a);
        assert_eq!(a.pow(&[]), Field::<P>::ONE);
        // encoding round trip
        assert_eq!(Field::<P>::from_repr(&a.to_repr()), Ok(a));
        assert_eq!(Field::<P>::from_limbs(a.to_limbs()), Some(a));
    }
    // Fermat: a^(p-1) = 1
    let a = rng.field::<P>();
    let mut p_minus_1 = P::MODULUS;
    p_minus_1[0] -= 1;
    assert_eq!(a.pow(&p_minus_1), Field::<P>::ONE);
    assert_eq!(Field::<P>::from_u64(0), Field::<P>::ZERO);
    assert_eq!(Field::<P>::from_u64(1), Field::<P>::ONE);
    assert_eq!(
        Field::<P>::from_u64(7) * Field::<P>::from_u64(6),
        Field::<P>::from_u64(42)
    );
}

#[test]
fn field_algebraic_laws() {
    field_laws::<FpParams>(0x1234_5678_9abc_def1);
    field_laws::<FqParams>(0x0fed_cba9_8765_4321);
}

#[test]
fn field_hostile_encodings() {
    // exactly the modulus, modulus + 1 and all-ones are non-canonical; modulus - 1 is canonical
    let modulus = modulus_bytes::<FpParams>();
    assert_eq!(Fp::from_repr(&modulus), Err(FieldError::NonCanonical));
    let mut plus_one = modulus;
    plus_one[0] += 1;
    assert_eq!(Fp::from_repr(&plus_one), Err(FieldError::NonCanonical));
    assert_eq!(Fp::from_repr(&[0xff; 32]), Err(FieldError::NonCanonical));
    let mut minus_one = modulus;
    minus_one[0] -= 1;
    assert_eq!(Fp::from_repr(&minus_one), Ok(-Fp::ONE));
    // the Fq modulus is a different bound: Fq's modulus is below Fp's? both near 2^254
    let q_modulus = modulus_bytes::<FqParams>();
    assert_eq!(Fq::from_repr(&q_modulus), Err(FieldError::NonCanonical));
    // wrong lengths
    for len in [0usize, 1, 31, 33, 64] {
        assert_eq!(
            Fp::from_repr_slice(&vec![0u8; len]),
            Err(FieldError::WrongLength {
                expected: 32,
                found: len
            })
        );
    }
    assert_eq!(Fp::from_repr_slice(&[0u8; 32]), Ok(Fp::ZERO));
    // from_limbs rejects values at or above the modulus
    assert_eq!(Fp::from_limbs(FpParams::MODULUS), None);
    // from_uniform reduces the all-ones string without panicking
    let all_ones = Fp::from_uniform_bytes_64(&[0xff; 64]);
    assert_eq!(Fp::from_repr(&all_ones.to_repr()), Ok(all_ones));
    // 2^512 - 1 = (2^256 - 1)(2^256 + 1)
    let two_256 = Fp::from_u64(2).pow(&[256]);
    let low = Fp::from_uniform_bytes_64(&{
        let mut b = [0u8; 64];
        b[..32].fill(0xff);
        b
    });
    assert_eq!(low, two_256 - Fp::ONE);
    assert_eq!(all_ones, (two_256 - Fp::ONE) * (two_256 + Fp::ONE));
}

// ---------------------------------------------------------------------------------------------
// curves

fn run_curve_vectors<C: Curve>(tag: &str, rows: &[Vec<String>]) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows.iter().filter(|row| row[0] == tag) {
        let op = row[1].as_str();
        *seen.entry(op.to_string()).or_default() += 1;
        let ctx = format!("{tag} {op} {:?}", &row[2..]);
        match op {
            "const" => match row[2].as_str() {
                "a" => assert_eq!(row[3], "0"),
                "b" => {
                    // b = 5: the generator (-1, 2) is on the curve, x = 0 is not
                    assert_eq!(row[3], "5");
                    assert!(Affine::<C>::new(BaseOf::<C>::ZERO, BaseOf::<C>::from_u64(5)).is_err());
                }
                other => panic!("unknown curve const {other}"),
            },
            "gen" => assert_eq!(Affine::<C>::generator(), point::<C>(&row[2]), "{ctx}"),
            "id" => assert_eq!(Affine::<C>::identity(), point::<C>(&row[2]), "{ctx}"),
            "order_minus_one_scalar" => {
                let k = fe::<C::Scalar>(&row[2]);
                let g = Projective::<C>::generator();
                assert_eq!(g.mul_scalar(&k), -g, "{ctx}");
                assert_eq!((g.mul_scalar(&k) + g), Projective::identity());
            }
            "add" => {
                let p = point::<C>(&row[2]);
                let q = point::<C>(&row[3]);
                let r = point::<C>(&row[4]);
                let sum = p.to_projective().add_projective(&q.to_projective());
                assert_eq!(sum.to_affine(), r, "{ctx}");
                assert_eq!(p.to_projective().add_affine(&q).to_affine(), r, "{ctx}");
                assert!(sum.is_on_curve());
            }
            "dbl" => {
                let p = point::<C>(&row[2]);
                assert_eq!(p.to_projective().double().to_affine(), point::<C>(&row[3]));
            }
            "neg" => assert_eq!((-point::<C>(&row[2])), point::<C>(&row[3]), "{ctx}"),
            "mul" => {
                let p = point::<C>(&row[2]);
                let k = fe::<C::Scalar>(&row[3]);
                assert_eq!(p.mul_scalar(&k).to_affine(), point::<C>(&row[4]), "{ctx}");
            }
            "comp" => {
                let p = point::<C>(&row[2]);
                assert_eq!(hex_encode(&p.to_compressed()), row[3], "{ctx}");
                assert_eq!(Affine::<C>::from_compressed(&array32(&row[3])), Ok(p));
            }
            "decomp" => {
                let bytes = array32(&row[2]);
                let got = Affine::<C>::from_compressed(&bytes);
                if row[3] == "none" {
                    assert!(got.is_err(), "{ctx}");
                } else {
                    assert_eq!(got, Ok(point::<C>(&row[3])), "{ctx}");
                }
            }
            "from_xy" => {
                let x = Field::<C::Base>::from_repr(&array32(&row[2]));
                let y = Field::<C::Base>::from_repr(&array32(&row[3]));
                let ok = row[4] == "1";
                match (x, y) {
                    (Ok(x), Ok(y)) => {
                        assert_eq!(Affine::<C>::new(x, y).is_ok(), ok, "{ctx}");
                    }
                    _ => assert!(!ok, "{ctx}"),
                }
            }
            other => panic!("unknown curve op {other}"),
        }
    }
    for op in [
        "const", "gen", "id", "add", "dbl", "neg", "mul", "comp", "decomp", "from_xy",
    ] {
        assert!(seen.contains_key(op), "{tag}: no {op} vectors");
    }
}

#[test]
fn curve_vectors_curves_tsv() {
    let rows = fixture_rows("curves.tsv");
    run_curve_vectors::<Pallas>("pallas", &rows);
    run_curve_vectors::<Vesta>("vesta", &rows);
}

fn random_point<C: Curve>(rng: &mut Rng) -> Projective<C> {
    Projective::<C>::generator().mul_scalar(&rng.field::<C::Scalar>())
}

fn curve_laws<C: Curve>(seed: u64) {
    let mut rng = Rng(seed);
    let id = Projective::<C>::identity();
    assert!(Affine::<C>::generator().is_on_curve());
    assert!(Affine::<C>::identity().is_on_curve());
    assert_eq!(Affine::<C>::default(), Affine::<C>::identity());
    for _ in 0..30 {
        let p = random_point::<C>(&mut rng);
        let q = random_point::<C>(&mut rng);
        let r = random_point::<C>(&mut rng);
        assert!(p.is_on_curve());
        assert!(p.to_affine().is_on_curve());
        assert_eq!(p + q, q + p);
        assert_eq!((p + q) + r, p + (q + r));
        assert_eq!(p + id, p);
        assert_eq!(id + p, p);
        assert_eq!(p + (-p), id);
        assert_eq!(p - p, id);
        assert_eq!(p + p, p.double());
        assert_eq!(p.add_affine(&q.to_affine()), p + q);
        assert_eq!(p.add_affine(&p.to_affine()), p.double());
        assert_eq!(p.add_affine(&(-p).to_affine()), id);
        // scalar multiplication is a ring action
        let a = rng.field::<C::Scalar>();
        let b = rng.field::<C::Scalar>();
        assert_eq!(p.mul_scalar(&a) + p.mul_scalar(&b), p.mul_scalar(&(a + b)));
        assert_eq!(p.mul_scalar(&a).mul_scalar(&b), p.mul_scalar(&(a * b)));
        assert_eq!((p + q).mul_scalar(&a), p.mul_scalar(&a) + q.mul_scalar(&a));
        // the group order annihilates every point
        assert_eq!(p.mul_limbs(&<C::Scalar as FieldParams>::MODULUS), id);
        // encodings round-trip
        let affine = p.to_affine();
        assert_eq!(
            Affine::<C>::from_compressed(&affine.to_compressed()),
            Ok(affine)
        );
        assert_eq!(
            Affine::<C>::from_uncompressed(&affine.to_uncompressed()),
            Ok(affine)
        );
        // equality is representation independent
        let z = rng.field::<C::Base>();
        assert_eq!(p, p.to_affine().to_projective());
        let _ = z;
    }
    // batch normalization matches individual conversion, including identities
    let mut points: Vec<Projective<C>> = (0..17).map(|_| random_point::<C>(&mut rng)).collect();
    points[0] = id;
    points[7] = id;
    points[16] = id;
    let batch = batch_normalize(&points);
    for (p, a) in points.iter().zip(&batch) {
        assert_eq!(p.to_affine(), *a);
    }
    assert!(batch_normalize::<C>(&[]).is_empty());
    // identity edge cases
    assert_eq!(id.double(), id);
    assert_eq!(id.to_affine(), Affine::<C>::identity());
    assert_eq!(Projective::<C>::generator().mul_limbs(&[0, 0, 0, 0]), id);
    assert_eq!(
        Projective::<C>::generator().mul_limbs(&[1, 0, 0, 0]),
        Projective::<C>::generator()
    );
}

#[test]
fn curve_algebraic_laws() {
    curve_laws::<Pallas>(0xdead_beef_0001);
    curve_laws::<Vesta>(0xdead_beef_0002);
}

fn curve_hostile<C: Curve>() {
    let mut rng = Rng(77);
    // wrong lengths
    for len in [0usize, 1, 31, 33, 64] {
        assert_eq!(
            Affine::<C>::from_compressed_slice(&vec![1u8; len]),
            Err(CurveError::WrongLength {
                expected: 32,
                found: len
            })
        );
    }
    for len in [0usize, 32, 63, 65] {
        assert_eq!(
            Affine::<C>::from_uncompressed_slice(&vec![0u8; len]),
            Err(CurveError::WrongLength {
                expected: 64,
                found: len
            })
        );
    }
    // non-canonical x (>= modulus) in compressed form
    let mut modulus = [0u8; 32];
    for (chunk, limb) in modulus.chunks_mut(8).zip(<C::Base as FieldParams>::MODULUS) {
        chunk.copy_from_slice(&limb.to_le_bytes());
    }
    assert_eq!(
        Affine::<C>::from_compressed(&modulus),
        Err(CurveError::NonCanonical)
    );
    let mut x_plus_one = modulus;
    x_plus_one[0] = 0x02;
    assert_eq!(
        Affine::<C>::from_compressed(&x_plus_one),
        Err(CurveError::NonCanonical)
    );
    // flag bit set on top of a non-canonical x is still non-canonical
    let mut flagged = modulus;
    flagged[31] |= 0x80;
    assert_eq!(
        Affine::<C>::from_compressed(&flagged),
        Err(CurveError::NonCanonical)
    );
    // bit 6 of the top byte makes x >= 2^254 > modulus: non-canonical
    let mut high = [0u8; 32];
    high[31] = 0x7f;
    assert_eq!(
        Affine::<C>::from_compressed(&high),
        Err(CurveError::NonCanonical)
    );
    // an x with no curve point
    let mut found_off_curve = 0;
    for _ in 0..40 {
        let x = rng.field::<C::Base>();
        let rhs = x.square() * x + BaseOf::<C>::from_u64(5);
        let mut bytes = x.to_repr();
        if rhs.sqrt().is_none() {
            found_off_curve += 1;
            assert_eq!(
                Affine::<C>::from_compressed(&bytes),
                Err(CurveError::NotOnCurve)
            );
            bytes[31] |= 0x80;
            assert_eq!(
                Affine::<C>::from_compressed(&bytes),
                Err(CurveError::NotOnCurve)
            );
        } else {
            let even = Affine::<C>::from_compressed(&bytes).unwrap();
            bytes[31] |= 0x80;
            let odd = Affine::<C>::from_compressed(&bytes).unwrap();
            assert_eq!(even.x(), x);
            assert!(!even.y().is_odd());
            assert!(odd.y().is_odd());
            assert_eq!(odd, -even);
        }
    }
    assert!(found_off_curve > 0);
    // identity: x = 0 is accepted as the identity whatever the sign bit says (halo2curves)
    let mut zero_flag = [0u8; 32];
    assert_eq!(
        Affine::<C>::from_compressed(&zero_flag),
        Ok(Affine::identity())
    );
    zero_flag[31] = 0x80;
    assert_eq!(
        Affine::<C>::from_compressed(&zero_flag),
        Ok(Affine::identity())
    );
    // uncompressed: off-curve, non-canonical and half-identity
    let g = Affine::<C>::generator();
    let mut off = g.to_uncompressed();
    off[32] ^= 1;
    assert_eq!(
        Affine::<C>::from_uncompressed(&off),
        Err(CurveError::NotOnCurve)
    );
    let mut noncanon = g.to_uncompressed();
    noncanon[..32].copy_from_slice(&modulus);
    assert_eq!(
        Affine::<C>::from_uncompressed(&noncanon),
        Err(CurveError::NonCanonical)
    );
    let mut half = [0u8; 64];
    half[32] = 1;
    assert_eq!(
        Affine::<C>::from_uncompressed(&half),
        Err(CurveError::NotOnCurve)
    );
    assert_eq!(
        Affine::<C>::from_uncompressed(&[0u8; 64]),
        Ok(Affine::identity())
    );
    // constructing from an off-curve pair
    assert_eq!(
        Affine::<C>::new(BaseOf::<C>::ONE, BaseOf::<C>::ONE),
        Err(CurveError::NotOnCurve)
    );
    assert!(g.mul_scalar(&ScalarOf::<C>::ZERO).is_identity());
}

#[test]
fn curve_hostile_inputs() {
    curve_hostile::<Pallas>();
    curve_hostile::<Vesta>();
}

#[test]
fn generator_matches_halo2curves_constants() {
    // halo2curves pasta/pallas.rs: generator (-1, 2) on both curves, b = 5
    assert_eq!(Affine::<Pallas>::generator().x(), -Fp::ONE);
    assert_eq!(Affine::<Pallas>::generator().y(), Fp::from_u64(2));
    assert_eq!(Affine::<Vesta>::generator().x(), -Fq::ONE);
    assert_eq!(Affine::<Vesta>::generator().y(), Fq::from_u64(2));
    // Z = -13 limbs from halo2curves SVDW_Z
    let pallas = Svdw::<Pallas>::new().unwrap().constants()[0];
    assert_eq!(
        pallas.to_limbs(),
        [
            0x992d30ecfffffff4,
            0x224698fc094cf91b,
            0x0000000000000000,
            0x4000000000000000
        ]
    );
    let vesta = Svdw::<Vesta>::new().unwrap().constants()[0];
    assert_eq!(
        vesta.to_limbs(),
        [
            0x8c46eb20fffffff4,
            0x224698fc0994a8dd,
            0x0000000000000000,
            0x4000000000000000
        ]
    );
}

// ---------------------------------------------------------------------------------------------
// serialization as nova's bincode writes it

fn run_serde_vectors<C: Curve>(tag: &str, rows: &[Vec<String>]) {
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows.iter().filter(|row| row[0] == tag) {
        *kinds.entry(row[1].clone()).or_default() += 1;
        match row[1].as_str() {
            "affine" | "proj" => {
                let p = point::<C>(&row[2]);
                assert_eq!(hex_encode(&p.to_compressed()), row[3]);
                assert_eq!(Affine::<C>::from_compressed(&array32(&row[3])), Ok(p));
            }
            "affine_vec5" => {
                let points: Vec<Affine<C>> = row[2].split(',').map(point::<C>).collect();
                assert_eq!(points.len(), 5);
                let mut bytes = (points.len() as u64).to_le_bytes().to_vec();
                for p in &points {
                    bytes.extend_from_slice(&p.to_compressed());
                }
                assert_eq!(hex_encode(&bytes), row[3]);
                // and back
                let decoded = hex_decode(&row[3]);
                assert_eq!(u64::from_le_bytes(decoded[..8].try_into().unwrap()), 5);
                for (i, p) in points.iter().enumerate() {
                    let chunk = &decoded[8 + 32 * i..8 + 32 * (i + 1)];
                    assert_eq!(Affine::<C>::from_compressed_slice(chunk), Ok(*p));
                }
            }
            "scalar" => {
                let k = fe::<C::Scalar>(&row[2]);
                assert_eq!(hex_encode(&k.to_repr()), row[3]);
            }
            "scalar_vec4" => {
                let scalars: Vec<ScalarOf<C>> = row[2].split(',').map(fe::<C::Scalar>).collect();
                assert_eq!(scalars.len(), 4);
                let mut bytes = 4u64.to_le_bytes().to_vec();
                for k in &scalars {
                    bytes.extend_from_slice(&k.to_repr());
                }
                assert_eq!(hex_encode(&bytes), row[3]);
            }
            other => panic!("unknown serde kind {other}"),
        }
    }
    assert_eq!(kinds.len(), 5, "{tag}: {kinds:?}");
}

#[test]
fn serde_points_tsv() {
    let rows = fixture_rows("serde_points.tsv");
    run_serde_vectors::<Pallas>("pallas", &rows);
    run_serde_vectors::<Vesta>("vesta", &rows);
}

// ---------------------------------------------------------------------------------------------
// keccak

#[test]
fn keccak_known_answers() {
    assert_eq!(
        hex_encode(&keccak256(b"")),
        "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
    );
    assert_eq!(
        hex_encode(&keccak256(b"abc")),
        "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
    );
    assert_eq!(
        hex_encode(&sha3_256(b"")),
        "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a"
    );
    assert_eq!(
        hex_encode(&sha3_256(b"abc")),
        "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
    );
    let mut out = [0u8; 32];
    shake256(b"", &mut out);
    assert_eq!(
        hex_encode(&out),
        "46b9dd2b0ba88d13233b3feb743eeb243fcd52ea62b81b82b50c27646ed5762f"
    );
    assert_eq!(
        hex_encode(&sha256_digest(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn keccak_incremental_matches_one_shot() {
    let mut rng = Rng(5);
    for len in [0usize, 1, 135, 136, 137, 271, 272, 273, 1000] {
        let data: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        for split in [0, len / 3, len / 2, len] {
            let mut hasher = Keccak256::new();
            hasher.update(&data[..split]);
            hasher.update(&data[split..]);
            assert_eq!(
                hasher.finalize(),
                keccak256(&data),
                "len {len} split {split}"
            );
        }
        // reading the XOF in pieces equals reading it at once
        let mut whole = vec![0u8; 400];
        shake256(&data, &mut whole);
        let mut xof = Shake256::new();
        xof.update(&data[..len / 2]);
        xof.update(&data[len / 2..]);
        let mut reader = xof.finalize_xof();
        let mut pieces = Vec::new();
        for size in [1usize, 7, 128, 135, 1, 128] {
            let mut piece = vec![0u8; size];
            reader.read(&mut piece);
            pieces.extend(piece);
        }
        assert_eq!(pieces, whole[..pieces.len()]);
    }
}

#[test]
fn keccak_vectors_keccak_tsv() {
    let rows = fixture_rows("keccak.tsv");
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in &rows {
        *counts.entry(row[0].clone()).or_default() += 1;
        let msg = hex_decode(&row[1]);
        match row[0].as_str() {
            "keccak256" => assert_eq!(hex_encode(&keccak256(&msg)), row[2]),
            "sha3_256" => assert_eq!(hex_encode(&sha3_256(&msg)), row[2]),
            "sha256" => assert_eq!(hex_encode(&sha256_digest(&msg)), row[2]),
            "shake256" => {
                let len: usize = row[2].parse().unwrap();
                let mut out = vec![0u8; len];
                shake256(&msg, &mut out);
                assert_eq!(hex_encode(&out), row[3]);
            }
            other => panic!("unknown keccak op {other}"),
        }
    }
    assert_eq!(counts.len(), 4, "{counts:?}");
}

// ---------------------------------------------------------------------------------------------
// transcript (checks keccak256 and FromUniformBytes<64> on scripted nova sessions)

fn lohi(prefix: &[u8], input: &[u8]) -> [u8; 64] {
    let make = |suffix: Option<u8>| {
        let mut h = Keccak256::new();
        h.update(prefix);
        h.update(input);
        if let Some(byte) = suffix {
            h.update(&[byte]);
        }
        h.finalize()
    };
    let mut out = [0u8; 64];
    out[..32].copy_from_slice(&make(Some(0)));
    out[32..].copy_from_slice(&make(Some(1)));
    out
}

fn run_transcript<P: FieldParams>(session: &[Vec<String>]) -> usize {
    let mut state = [0u8; 64];
    let mut round = 0u64;
    let mut buf: Vec<u8> = Vec::new();
    let mut squeezes = 0;
    for row in session {
        match row[0].as_str() {
            "new" => {
                let mut input = b"NoTR".to_vec();
                input.extend_from_slice(row[1].as_bytes());
                state = lohi(b"", &input);
                round = 0;
                buf.clear();
            }
            "absorb" => {
                buf.extend_from_slice(row[1].as_bytes());
                buf.extend_from_slice(&hex_decode(&row[3]));
            }
            "domsep" => {
                buf.extend_from_slice(b"NoDS");
                buf.extend_from_slice(row[1].as_bytes());
            }
            "squeeze" => {
                let mut input = b"NoDS".to_vec();
                input.extend_from_slice(&round.to_le_bytes());
                input.extend_from_slice(&state);
                input.extend_from_slice(row[1].as_bytes());
                state = lohi(&buf, &input);
                round += 1;
                buf.clear();
                assert_eq!(hex_encode(&state), row[3], "state after squeeze");
                assert_eq!(round - 1, row[4].parse::<u64>().unwrap());
                let challenge = Field::<P>::from_uniform_bytes_64(&state);
                assert_eq!(hex_encode(&challenge.to_repr()), row[2], "challenge");
                squeezes += 1;
            }
            "squeeze_bits" => {
                let nb: usize = row[2].parse().unwrap();
                let one = row[3] == "1";
                let mut input = b"NoDS".to_vec();
                input.extend_from_slice(&round.to_le_bytes());
                input.extend_from_slice(&state);
                input.extend_from_slice(row[1].as_bytes());
                state = lohi(&buf, &input);
                round += 1;
                buf.clear();
                assert_eq!(hex_encode(&state), row[5], "state after squeeze_bits");
                let mut value = [0u8; 32];
                let full = nb / 8;
                value[..full].copy_from_slice(&state[..full]);
                if !nb.is_multiple_of(8) {
                    value[full] = state[full] & ((1u8 << (nb % 8)) - 1);
                }
                if one && nb > 0 {
                    value[(nb - 1) / 8] |= 1 << ((nb - 1) % 8);
                }
                assert_eq!(hex_encode(&value), row[4], "squeeze_bits value");
                squeezes += 1;
            }
            other => panic!("unknown transcript record {other}"),
        }
    }
    squeezes
}

#[test]
fn transcript_tsv() {
    let rows = fixture_rows("transcript.tsv");
    let mut total = 0;
    let mut session: Vec<Vec<String>> = Vec::new();
    let mut engine = String::new();
    for row in rows {
        match row[0].as_str() {
            "begin" => {
                session.clear();
                engine = row[2].clone();
            }
            "end" => {
                total += match engine.as_str() {
                    "pallas" => run_transcript::<FqParams>(&session),
                    "vesta" => run_transcript::<FpParams>(&session),
                    other => panic!("unknown engine {other}"),
                };
            }
            _ => session.push(row),
        }
    }
    assert!(total > 10, "only {total} squeezes checked");
}

// ---------------------------------------------------------------------------------------------
// hash to curve

fn ietf_message(name: &str) -> Vec<u8> {
    match name {
        "empty" => Vec::new(),
        "abc" => b"abc".to_vec(),
        "abcdef" => b"abcdef0123456789".to_vec(),
        "q128" => {
            let mut m = b"q128_".to_vec();
            m.extend(std::iter::repeat_n(b'q', 128));
            m
        }
        "a512" => {
            let mut m = b"a512_".to_vec();
            m.extend(std::iter::repeat_n(b'a', 512));
            m
        }
        _ => unreachable!(),
    }
}

#[test]
fn expand_message_xmd_ietf_vectors() {
    // RFC 9380 K.1, DST = QUUX-V01-CS02-with-expander-SHA256-128 (also halo2curves
    // hash_to_curve.rs `test_expand_message`).
    let dst = b"QUUX-V01-CS02-with-expander-SHA256-128";
    let cases: [(&str, &str); 10] = [
        ("empty", "68a985b87eb6b46952128911f2a4412bbc302a9d759667f87f7a21d803f07235"),
        ("abc", "d8ccab23b5985ccea865c6c97b6e5b8350e794e603b4b97902f53a8a0d605615"),
        ("abcdef", "eff31487c770a893cfb36f912fbfcbff40d5661771ca4b2cb4eafe524333f5c1"),
        ("q128", "b23a1d2b4d97b2ef7785562a7e8bac7eed54ed6e97e29aa51bfe3f12ddad1ff9"),
        ("a512", "4623227bcc01293b8c130bf771da8c298dede7383243dc0993d2d94823958c4c"),
        ("empty", "af84c27ccfd45d41914fdff5df25293e221afc53d8ad2ac06d5e3e29485dadbee0d121587713a3e0dd4d5e69e93eb7cd4f5df4cd103e188cf60cb02edc3edf18eda8576c412b18ffb658e3dd6ec849469b979d444cf7b26911a08e63cf31f9dcc541708d3491184472c2c29bb749d4286b004ceb5ee6b9a7fa5b646c993f0ced"),
        ("abc", "abba86a6129e366fc877aab32fc4ffc70120d8996c88aee2fe4b32d6c7b6437a647e6c3163d40b76a73cf6a5674ef1d890f95b664ee0afa5359a5c4e07985635bbecbac65d747d3d2da7ec2b8221b17b0ca9dc8a1ac1c07ea6a1e60583e2cb00058e77b7b72a298425cd1b941ad4ec65e8afc50303a22c0f99b0509b4c895f40"),
        ("abcdef", "ef904a29bffc4cf9ee82832451c946ac3c8f8058ae97d8d629831a74c6572bd9ebd0df635cd1f208e2038e760c4994984ce73f0d55ea9f22af83ba4734569d4bc95e18350f740c07eef653cbb9f87910d833751825f0ebefa1abe5420bb52be14cf489b37fe1a72f7de2d10be453b2c9d9eb20c7e3f6edc5a60629178d9478df"),
        ("q128", "80be107d0884f0d881bb460322f0443d38bd222db8bd0b0a5312a6fedb49c1bbd88fd75d8b9a09486c60123dfa1d73c1cc3169761b17476d3c6b7cbbd727acd0e2c942f4dd96ae3da5de368d26b32286e32de7e5a8cb2949f866a0b80c58116b29fa7fabb3ea7d520ee603e0c25bcaf0b9a5e92ec6a1fe4e0391d1cdbce8c68a"),
        ("a512", "546aff5444b5b79aa6148bd81728704c32decb73a3ba76e9e75885cad9def1d06d6792f8a7d12794e90efed817d96920d728896a4510864370c207f99bd4a608ea121700ef01ed879745ee3e4ceef777eda6d9e5e38b90c86ea6fb0b36504ba4a45d22e86f6db5dd43d98a294bebb9125d5b794e9d2a81181066eb954966a487"),
    ];
    for (name, expected) in cases {
        let out = expand_message_xmd_sha256(&ietf_message(name), dst, expected.len() / 2).unwrap();
        assert_eq!(hex_encode(&out), expected, "{name}");
    }
}

#[test]
fn expand_message_xmd_hostile_inputs() {
    let dst = b"dst";
    assert_eq!(
        expand_message_xmd_sha256(b"m", dst, 0),
        Err(H2cError::InvalidOutputLength)
    );
    assert_eq!(
        expand_message_xmd_sha256(b"m", dst, 255 * 32 + 1),
        Err(H2cError::InvalidOutputLength)
    );
    assert_eq!(
        expand_message_xmd_sha256(b"m", dst, 255 * 32)
            .unwrap()
            .len(),
        255 * 32
    );
    assert_eq!(
        expand_message_xmd_sha256(b"m", &[b'x'; 256], 32),
        Err(H2cError::DstTooLong)
    );
    assert_eq!(
        expand_message_xmd_sha256(b"m", &[b'x'; 255], 32)
            .unwrap()
            .len(),
        32
    );
    // the requested length is part of the hashed input, so outputs of different lengths differ
    let long = expand_message_xmd_sha256(b"m", dst, 96).unwrap();
    let short = expand_message_xmd_sha256(b"m", dst, 50).unwrap();
    assert_eq!(short.len(), 50);
    assert_ne!(short[..], long[..50]);
}

fn run_h2c_vectors<C: H2cCurve>(tag: &str, rows: &[Vec<String>]) {
    let suite = HashToCurve::<C>::new().unwrap();
    let mut dst = FROM_LABEL_DOMAIN_PREFIX.to_vec();
    dst.extend_from_slice(C::SUITE_DOMAIN);
    let mut h2c_rows = 0;
    let mut const_rows = 0;
    for row in rows.iter().filter(|row| row[0] == tag) {
        match row[1].as_str() {
            "svdw_const" => {
                const_rows += 1;
                let got = suite.svdw().constants();
                for (i, value) in got.iter().enumerate() {
                    assert_eq!(
                        hex_encode(&value.to_repr()),
                        row[2 + i],
                        "{tag} constant {i}"
                    );
                }
            }
            "h2c" => {
                h2c_rows += 1;
                let msg = hex_decode(&row[2]);
                let (u0, u1) = suite.hash_to_field(&dst, &msg).unwrap();
                assert_eq!(hex_encode(&u0.to_repr()), row[3], "u0");
                assert_eq!(hex_encode(&u1.to_repr()), row[4], "u1");
                let q0 = suite.svdw().map(&u0).unwrap().to_affine();
                let q1 = suite.svdw().map(&u1).unwrap().to_affine();
                assert_eq!(q0, point::<C>(&row[5]), "Q0");
                assert_eq!(q1, point::<C>(&row[6]), "Q1");
                let result = suite.hash(FROM_LABEL_DOMAIN_PREFIX, &msg).unwrap();
                assert_eq!(result.to_affine(), point::<C>(&row[7]), "result");
                assert_eq!(
                    hash_to_curve::<C>(FROM_LABEL_DOMAIN_PREFIX, &msg)
                        .unwrap()
                        .to_affine(),
                    point::<C>(&row[7])
                );
                assert!(result.is_on_curve());
            }
            other => panic!("unknown h2c record {other}"),
        }
    }
    assert!(
        h2c_rows >= 10 && const_rows == 1,
        "{tag}: {h2c_rows} {const_rows}"
    );
}

#[test]
fn hash_to_curve_tsv() {
    let rows = fixture_rows("hash_to_curve.tsv");
    let mut xmd = 0;
    for row in rows.iter().filter(|row| row[0] == "xmd") {
        let dst = hex_decode(&row[1]);
        let msg = hex_decode(&row[2]);
        let len: usize = row[3].parse().unwrap();
        let out = expand_message_xmd_sha256(&msg, &dst, len).unwrap();
        assert_eq!(hex_encode(&out), row[4]);
        xmd += 1;
    }
    assert!(xmd >= 20, "only {xmd} xmd vectors");
    let rest: Vec<Vec<String>> = rows.into_iter().filter(|row| row[0] != "xmd").collect();
    run_h2c_vectors::<Pallas>("pallas", &rest);
    run_h2c_vectors::<Vesta>("vesta", &rest);
}

#[test]
fn svdw_properties() {
    // the map lands on the curve for degenerate inputs too, with no panic
    fn check<C: H2cCurve>() {
        let svdw = Svdw::<C>::new().unwrap();
        let mut rng = Rng(11);
        let mut inputs = vec![
            BaseOf::<C>::ZERO,
            BaseOf::<C>::ONE,
            -BaseOf::<C>::ONE,
            BaseOf::<C>::from_u64(2),
        ];
        // u^2 * c1 = +-1 makes the inverse term vanish
        let c1 = svdw.constants()[1];
        for sign in [BaseOf::<C>::ONE, -BaseOf::<C>::ONE] {
            if let Some(root) = (sign * c1.invert().unwrap()).sqrt() {
                inputs.push(root);
                inputs.push(-root);
            }
        }
        inputs.extend((0..50).map(|_| rng.field::<C::Base>()));
        for u in inputs {
            let p = svdw.map(&u).unwrap();
            assert!(p.is_on_curve() && !p.is_identity());
        }
    }
    check::<Pallas>();
    check::<Vesta>();
}

#[test]
fn from_label_matches_generators_tsv() {
    let rows = fixture_rows("generators.tsv");
    let mut cache: BTreeMap<(String, String, usize), Vec<String>> = BTreeMap::new();
    let mut gens = 0;
    let mut digests = 0;
    for row in &rows {
        let curve = row[0].as_str();
        let label = row[2].clone();
        let n: usize = row[3].parse().unwrap();
        let key = (curve.to_string(), label.clone(), n);
        match row[1].as_str() {
            "gen" => {
                gens += 1;
                let index: usize = row[4].parse().unwrap();
                let points = cache
                    .entry(key)
                    .or_insert_with(|| compute_generators(curve, &label, n));
                let expected_compressed = &row[6];
                assert_eq!(
                    &points[index], expected_compressed,
                    "{curve} {label} {n} #{index}"
                );
            }
            "gens_sha256" => {
                digests += 1;
                let (compressed, uncompressed, bincode) = digest_generators(curve, &label, n);
                assert_eq!(compressed, row[4], "{curve} {label} {n} compressed digest");
                assert_eq!(uncompressed, row[5], "{curve} {label} {n} x||y digest");
                assert_eq!(bincode, row[6], "{curve} {label} {n} bincode digest");
            }
            other => panic!("unknown generators record {other}"),
        }
    }
    assert!(gens >= 32 && digests >= 6, "gens {gens} digests {digests}");
}

fn compute_generators(curve: &str, label: &str, n: usize) -> Vec<String> {
    fn go<C: H2cCurve>(label: &str, n: usize) -> Vec<String> {
        from_label::<C>(label.as_bytes(), n)
            .unwrap()
            .iter()
            .take(16)
            .map(|p| hex_encode(&p.to_compressed()))
            .collect()
    }
    match curve {
        "pallas" => go::<Pallas>(label, n),
        "vesta" => go::<Vesta>(label, n),
        other => panic!("unknown curve {other}"),
    }
}

fn digest_generators(curve: &str, label: &str, n: usize) -> (String, String, String) {
    fn go<C: H2cCurve>(label: &str, n: usize) -> (String, String, String) {
        let points = from_label::<C>(label.as_bytes(), n).unwrap();
        assert_eq!(points.len(), n);
        let mut compressed = Vec::new();
        let mut uncompressed = Vec::new();
        let mut bincode = (n as u64).to_le_bytes().to_vec();
        for p in &points {
            compressed.extend_from_slice(&p.to_compressed());
            uncompressed.extend_from_slice(&p.to_uncompressed());
            bincode.extend_from_slice(&p.to_compressed());
        }
        (
            hex_encode(&sha256_digest(&compressed)),
            hex_encode(&sha256_digest(&uncompressed)),
            hex_encode(&sha256_digest(&bincode)),
        )
    }
    match curve {
        "pallas" => go::<Pallas>(label, n),
        "vesta" => go::<Vesta>(label, n),
        other => panic!("unknown curve {other}"),
    }
}

#[test]
fn from_label_is_deterministic_and_prefix_stable() {
    let small = from_label::<Pallas>(b"prefix", 5).unwrap();
    let large = from_label::<Pallas>(b"prefix", 200).unwrap();
    assert_eq!(small[..], large[..5]);
    assert_eq!(from_label::<Pallas>(b"prefix", 200).unwrap(), large);
    assert!(from_label::<Vesta>(b"prefix", 0).unwrap().is_empty());
    assert_ne!(
        from_label::<Vesta>(b"other", 3).unwrap(),
        from_label::<Vesta>(b"prefix", 3).unwrap()
    );
    for p in &large {
        assert!(p.is_on_curve() && !p.is_identity());
    }
    assert_eq!(
        from_label::<Pallas>(b"x", MAX_FROM_LABEL_POINTS + 1),
        Err(H2cError::TooManyPoints)
    );
}

// ---------------------------------------------------------------------------------------------
// msm

fn naive_msm<C: Curve>(scalars: &[ScalarOf<C>], bases: &[Affine<C>]) -> Projective<C> {
    let mut acc = Projective::<C>::identity();
    for (k, p) in scalars.iter().zip(bases) {
        acc = acc + p.mul_scalar(k);
    }
    acc
}

fn msm_checks<C: Curve>(seed: u64) {
    let mut rng = Rng(seed);
    for n in [0usize, 1, 2, 3, 7, 31, 32, 33, 100, 520] {
        let bases: Vec<Affine<C>> = (0..n)
            .map(|_| random_point::<C>(&mut rng).to_affine())
            .collect();
        let scalars: Vec<ScalarOf<C>> = (0..n).map(|_| rng.field::<C::Scalar>()).collect();
        let got = msm(&scalars, &bases).unwrap();
        assert_eq!(got, naive_msm(&scalars, &bases), "n = {n}");
        assert_eq!(msm(&scalars, &bases).unwrap().to_affine(), got.to_affine());
    }
    // structured scalars: zero, one, p-1 (= -1), repeated bases, identity bases, inverse pairs
    let g = Affine::<C>::generator();
    let bases = vec![g, g, g, Affine::identity(), g, (-g), g];
    let minus_one = -ScalarOf::<C>::ONE;
    let scalars = vec![
        ScalarOf::<C>::ZERO,
        ScalarOf::<C>::ONE,
        minus_one,
        ScalarOf::<C>::from_u64(9),
        ScalarOf::<C>::from_u64(3),
        ScalarOf::<C>::from_u64(3),
        minus_one,
    ];
    assert_eq!(msm(&scalars, &bases).unwrap(), naive_msm(&scalars, &bases));
    // all-zero and all-identity inputs
    assert!(msm(&vec![ScalarOf::<C>::ZERO; 5], &vec![g; 5])
        .unwrap()
        .is_identity());
    assert!(msm(&vec![minus_one; 5], &vec![Affine::<C>::identity(); 5])
        .unwrap()
        .is_identity());
    // a large run crosses the parallel threshold
    let n = 600;
    let bases: Vec<Affine<C>> = {
        let mut points = Vec::new();
        let mut p = Projective::<C>::generator().double();
        for _ in 0..n {
            points.push(p);
            p = p.add_projective(&Projective::generator());
        }
        batch_normalize(&points)
    };
    let scalars: Vec<ScalarOf<C>> = (0..n).map(|_| rng.field::<C::Scalar>()).collect();
    assert_eq!(msm(&scalars, &bases).unwrap(), naive_msm(&scalars, &bases));
}

#[test]
fn msm_matches_naive_sum() {
    msm_checks::<Pallas>(31337);
    msm_checks::<Vesta>(31338);
}

#[test]
fn msm_is_independent_of_thread_count() {
    let mut rng = Rng(4242);
    let n = 900;
    let bases = from_label::<Vesta>(b"msm-threads", n).unwrap();
    let scalars: Vec<Fp> = (0..n).map(|_| rng.field::<FpParams>()).collect();
    let single = msm_with_threads(&scalars, &bases, 1).unwrap();
    for threads in [0usize, 2, 3, 7, 64] {
        let multi = msm_with_threads(&scalars, &bases, threads).unwrap();
        assert_eq!(multi.to_affine(), single.to_affine(), "threads {threads}");
    }
    assert_eq!(
        msm(&scalars, &bases).unwrap().to_affine(),
        single.to_affine()
    );
}

#[test]
fn msm_hostile_inputs() {
    let g = Affine::<Pallas>::generator();
    assert_eq!(
        msm::<Pallas>(&[Fq::ONE], &[g, g]),
        Err(MsmError::LengthMismatch {
            scalars: 1,
            bases: 2
        })
    );
    assert_eq!(
        msm::<Pallas>(&[], &[g]),
        Err(MsmError::LengthMismatch {
            scalars: 0,
            bases: 1
        })
    );
    assert!(msm::<Pallas>(&[], &[]).unwrap().is_identity());
}

// ---------------------------------------------------------------------------------------------
// benchmarks (ignored by default; run with --release -- --ignored --nocapture)

#[test]
#[ignore]
fn bench_msm_2_pow_19() {
    let n = 1usize << 19;
    let bases = from_label::<Pallas>(b"bench-msm", n).unwrap();
    let mut rng = Rng(99);
    let scalars: Vec<Fq> = (0..n).map(|_| rng.field::<FqParams>()).collect();
    let start = Instant::now();
    let result = msm(&scalars, &bases).unwrap();
    let elapsed = start.elapsed();
    println!(
        "msm 2^19 points (Pallas): {elapsed:?}, is_identity={}",
        result.is_identity()
    );
    let start = Instant::now();
    let again = msm(&scalars, &bases).unwrap();
    println!(
        "msm 2^19 points (Pallas), second run: {:?}",
        start.elapsed()
    );
    assert_eq!(result, again);
    println!(
        "target 2 s: {}",
        if elapsed.as_secs_f64() < 2.0 {
            "met"
        } else {
            "missed"
        }
    );
}

#[test]
#[ignore]
fn bench_from_label_2_pow_19_plus_1() {
    let n = (1usize << 19) + 1;
    let start = Instant::now();
    let points = from_label::<Pallas>(b"ck", n).unwrap();
    let elapsed = start.elapsed();
    println!("from_label(ck, 2^19+1) (Pallas): {elapsed:?}");
    assert_eq!(points.len(), n);
    assert!(elapsed.as_secs() < 300);
}

#[test]
#[ignore]
fn bench_primitives() {
    let mut rng = Rng(5);
    let a = rng.field::<FpParams>();
    let mut x = a;
    let start = Instant::now();
    for _ in 0..4_000_000 {
        x *= a;
    }
    let per = start.elapsed().as_nanos() as f64 / 4e6;
    println!("field mul: {per:.1} ns (x={})", x.is_zero());
    let g = Projective::<Pallas>::generator().double();
    let q = Affine::<Pallas>::generator();
    let mut acc = g;
    let start = Instant::now();
    for _ in 0..200_000 {
        acc = acc.add_affine(&q);
    }
    println!(
        "mixed add: {:.1} ns (acc={})",
        start.elapsed().as_nanos() as f64 / 2e5,
        acc.is_identity()
    );
    println!(
        "available_parallelism: {:?}",
        std::thread::available_parallelism()
    );
}
