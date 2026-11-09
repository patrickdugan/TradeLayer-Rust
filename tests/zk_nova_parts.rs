//! Tests for the Nova verifier building blocks against the recorded nova-snark 0.76.0 vectors:
//! Keccak transcript, Poseidon random oracle, polynomial helpers, sum-check, codecs, and the
//! registered key.

use std::time::Instant;

use tradelayer_core::zk_nova::codec::{
    decode_compressed_snark, decode_vk_nogens, Reader, VK_NOGENS_BYTES,
};
use tradelayer_core::zk_nova::curve::{Affine, Pallas};
use tradelayer_core::zk_nova::error::{DecodeError, NovaVerifyError};
use tradelayer_core::zk_nova::field::{Field, FieldParams, FpParams, Fq, FqParams};
use tradelayer_core::zk_nova::h2c::sha256_digest;
use tradelayer_core::zk_nova::keccak::sha3_256;
use tradelayer_core::zk_nova::key::{
    GeneratorCache, RegisteredNovaKey, Sha3Stream, REGISTERED_KEY_BYTES,
};
use tradelayer_core::zk_nova::polys::{
    decompress_univariate, eq_eval, eq_table, eval_univariate, identity_eval, masked_eq_eval,
    power_eval, powers, sparse_eval,
};
use tradelayer_core::zk_nova::poseidon::{PoseidonParams, RandomOracle};
use tradelayer_core::zk_nova::sumcheck;
use tradelayer_core::zk_nova::transcript::Transcript;

fn hex_decode(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2));
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).expect("hex"))
        .collect()
}

fn fixture_text(name: &str) -> String {
    let path = format!("{}/fixtures/zk_nova/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

fn rows(name: &str) -> Vec<Vec<String>> {
    fixture_text(name)
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect()
}

fn scalar<P: FieldParams>(hex: &str) -> Field<P> {
    Field::<P>::from_repr_slice(&hex_decode(hex)).expect("canonical scalar")
}

// ---------------------------------------------------------------------------------------------
// transcript

fn replay_transcript<P: FieldParams>(script: &[Vec<String>]) -> usize {
    let mut t: Option<Transcript<P>> = None;
    let mut squeezes = 0;
    for row in script {
        match row[0].as_str() {
            "new" | "te_begin" => {
                let label = row.last().expect("label");
                t = Some(Transcript::<P>::new(label.as_bytes()));
            }
            "absorb" => {
                let bytes = hex_decode(&row[3]);
                t.as_mut()
                    .expect("transcript")
                    .absorb_bytes(row[1].as_bytes(), &bytes);
            }
            "te_absorb" => {
                let bytes = hex_decode(&row[2]);
                t.as_mut()
                    .expect("transcript")
                    .absorb_bytes(row[1].as_bytes(), &bytes);
            }
            "domsep" | "te_dom_sep" => t.as_mut().expect("transcript").dom_sep(row[1].as_bytes()),
            "squeeze" => {
                let got = t.as_mut().expect("transcript").squeeze(row[1].as_bytes());
                assert_eq!(got.to_repr().to_vec(), hex_decode(&row[2]), "challenge");
                assert_eq!(
                    t.as_ref().expect("transcript").state().to_vec(),
                    hex_decode(&row[3]),
                    "state"
                );
                squeezes += 1;
            }
            "te_squeeze" => {
                let got = t.as_mut().expect("transcript").squeeze(row[1].as_bytes());
                assert_eq!(got.to_repr().to_vec(), hex_decode(&row[2]), "challenge");
                squeezes += 1;
            }
            other => panic!("unexpected record {other}"),
        }
    }
    squeezes
}

#[test]
fn transcript_matches_scripted_sessions() {
    let mut sessions: Vec<(String, String, Vec<Vec<String>>)> = Vec::new();
    for row in rows("transcript.tsv") {
        match row[0].as_str() {
            "begin" => sessions.push((row[1].clone(), row[2].clone(), Vec::new())),
            "end" => {}
            _ => sessions.last_mut().expect("begin").2.push(row),
        }
    }
    let mut replayed = 0;
    let mut total = 0;
    for (id, engine, script) in &sessions {
        total += 1;
        // squeeze_bits advances the state like squeeze but reads fewer bits; the verifier never
        // calls it, so sessions that use it are not replayed.
        if script.iter().any(|r| r[0] == "squeeze_bits") {
            continue;
        }
        let squeezes = match engine.as_str() {
            "pallas" => replay_transcript::<FqParams>(script),
            "vesta" => replay_transcript::<FpParams>(script),
            other => panic!("engine {other} in {id}"),
        };
        assert!(squeezes > 0 || script.iter().all(|r| r[0] != "squeeze"));
        replayed += 1;
    }
    assert!(
        replayed >= 10,
        "only {replayed} of {total} sessions replayed"
    );
}

// ---------------------------------------------------------------------------------------------
// poseidon

#[test]
fn poseidon_constants_match_recorded_parameters() {
    let fp = PoseidonParams::<FpParams>::derive();
    let fq = PoseidonParams::<FqParams>::derive();
    let mut checked_rc = 0;
    let mut checked_mds = 0;
    for row in rows("poseidon_params.tsv") {
        if row.len() < 4 || row[1] != "wide" {
            continue;
        }
        match row[2].as_str() {
            "mds_row" => {
                let i: usize = row[3].parse().expect("row");
                let bytes = hex_decode(&row[4]);
                for j in 0..25 {
                    let (got, want) = match row[0].as_str() {
                        "fp" => (
                            fp.mds(i, j).to_repr().to_vec(),
                            bytes[32 * j..32 * j + 32].to_vec(),
                        ),
                        _ => (
                            fq.mds(i, j).to_repr().to_vec(),
                            bytes[32 * j..32 * j + 32].to_vec(),
                        ),
                    };
                    assert_eq!(got, want, "mds {} {i} {j}", row[0]);
                    checked_mds += 1;
                }
            }
            "rc_round" => {
                let r: usize = row[3].parse().expect("round");
                let bytes = hex_decode(&row[4]);
                for j in 0..25 {
                    let got = match row[0].as_str() {
                        "fp" => fp.round_constants()[r * 25 + j].to_repr(),
                        _ => fq.round_constants()[r * 25 + j].to_repr(),
                    };
                    assert_eq!(
                        got.to_vec(),
                        bytes[32 * j..32 * j + 32],
                        "rc {} {r} {j}",
                        row[0]
                    );
                    checked_rc += 1;
                }
            }
            _ => {}
        }
    }
    assert_eq!(checked_rc, 2 * 67 * 25);
    assert_eq!(checked_mds, 2 * 25 * 25);
}

fn run_ro_session<P: FieldParams>(params: &PoseidonParams<P>, script: &[Vec<String>]) -> usize {
    let mut items: Vec<Field<P>> = Vec::new();
    let mut squeezes = 0;
    for row in script {
        match row[0].as_str() {
            "absorb" => items.push(scalar::<P>(&row[1])),
            "squeeze" => {
                let bits: usize = row[1].parse().expect("bits");
                let start_with_one = row[2] == "1";
                let mut ro = RandomOracle::new(params);
                for item in &items {
                    ro.absorb(*item);
                }
                let raw = params.hash(&items);
                assert_eq!(raw.to_repr().to_vec(), hex_decode(&row[4]), "raw hash");
                if !start_with_one {
                    assert_eq!(ro.squeeze(bits).to_repr().to_vec(), hex_decode(&row[3]));
                }
                items = vec![raw];
                squeezes += 1;
            }
            other => panic!("unexpected {other}"),
        }
    }
    squeezes
}

#[test]
fn poseidon_matches_scripted_sessions() {
    let fp = PoseidonParams::<FpParams>::derive();
    let fq = PoseidonParams::<FqParams>::derive();
    let mut sessions: Vec<(String, String, Vec<Vec<String>>)> = Vec::new();
    for row in rows("poseidon.tsv") {
        match row[0].as_str() {
            "begin" => sessions.push((row[2].clone(), row[3].clone(), Vec::new())),
            "end" => {}
            _ => sessions.last_mut().expect("begin").2.push(row),
        }
    }
    let mut squeezes = 0;
    for (field, mode, script) in &sessions {
        if mode != "wide" {
            continue;
        }
        squeezes += match field.as_str() {
            "fp" => run_ro_session(&fp, script),
            "fq" => run_ro_session(&fq, script),
            other => panic!("field {other}"),
        };
    }
    assert!(squeezes >= 100, "only {squeezes} squeezes replayed");
}

#[test]
fn recorded_verification_traces_replay() {
    let fp = PoseidonParams::<FpParams>::derive();
    let fq = PoseidonParams::<FqParams>::derive();
    for name in ["cubic_3steps", "cubic_7steps", "nhc1_ppsnark_k1x10"] {
        let mut ro_records = 0;
        let mut transcript_rows: Vec<Vec<String>> = Vec::new();
        let mut engine = String::new();
        let mut pallas = Vec::new();
        let mut vesta = Vec::new();
        for row in rows(&format!("{name}.trace.tsv")) {
            match row[0].as_str() {
                "ro" => {
                    let bits: usize = row[2].parse().expect("bits");
                    let elements: Vec<&str> = row[5].split(',').filter(|s| !s.is_empty()).collect();
                    let (output, ok) = if row[1] == "fp" {
                        let mut ro = RandomOracle::new(&fp);
                        for e in &elements {
                            ro.absorb(scalar::<FpParams>(e));
                        }
                        (ro.squeeze(bits).to_repr().to_vec(), true)
                    } else {
                        let mut ro = RandomOracle::new(&fq);
                        for e in &elements {
                            ro.absorb(scalar::<FqParams>(e));
                        }
                        (ro.squeeze(bits).to_repr().to_vec(), true)
                    };
                    assert!(ok);
                    assert_eq!(output, hex_decode(&row[4]), "{name} ro record {ro_records}");
                    ro_records += 1;
                }
                "te_begin" => {
                    if !transcript_rows.is_empty() {
                        match engine.as_str() {
                            "pallas" => pallas = std::mem::take(&mut transcript_rows),
                            _ => vesta = std::mem::take(&mut transcript_rows),
                        }
                    }
                    engine = row[1].clone();
                    transcript_rows.push(row.clone());
                }
                r if r.starts_with("te_") => transcript_rows.push(row.clone()),
                _ => {}
            }
        }
        match engine.as_str() {
            "pallas" => pallas = transcript_rows,
            _ => vesta = transcript_rows,
        }
        assert_eq!(ro_records, 5, "{name}");
        let primary_squeezes = replay_transcript::<FqParams>(&pallas);
        let secondary_squeezes = replay_transcript::<FpParams>(&vesta);
        assert!(primary_squeezes > 30 && secondary_squeezes > 30, "{name}");
    }
}

// ---------------------------------------------------------------------------------------------
// polynomials and sum-check

fn fq(v: u64) -> Fq {
    Fq::from_u64(v)
}

#[test]
fn polynomial_helpers_agree_with_tables() {
    let a = [fq(3), fq(5), fq(7), fq(11)];
    let table = eq_table(&a);
    assert_eq!(table.len(), 16);
    for (index, entry) in table.iter().enumerate() {
        let bits: Vec<Fq> = (0..4)
            .map(|j| {
                if (index >> (3 - j)) & 1 == 1 {
                    Fq::ONE
                } else {
                    Fq::ZERO
                }
            })
            .collect();
        assert_eq!(*entry, eq_eval(&a, &bits), "eq table {index}");
        assert_eq!(identity_eval(&bits), fq(index as u64));
    }
    // sum over the cube of the table is 1
    assert_eq!(table.iter().fold(Fq::ZERO, |s, e| s + *e), Fq::ONE);
    // masked eq on the cube: table entry with the first 2^m entries zeroed
    let x4 = [fq(13), fq(17), fq(19), fq(23)];
    for m in 0..=4usize {
        let got = masked_eq_eval(&a, m, &x4).expect("m <= len");
        // multilinear extension of eq with entries 0..2^m zeroed, evaluated by the table
        let mut want = Fq::ZERO;
        let x_table = eq_table(&x4);
        for (i, entry) in table.iter().enumerate() {
            if i >= (1usize << m) {
                want += *entry * x_table[i];
            }
        }
        assert_eq!(got, want, "masked m={m}");
    }
    assert!(masked_eq_eval(&a, 5, &x4).is_none());
    // sparse_eval equals the multilinear extension of [z0, z1, z2, 0...]
    let y = [fq(2), fq(4), fq(6), fq(8), fq(10)];
    let z = [fq(31), fq(37), fq(41)];
    let chi = eq_table(&y);
    let want = z[0] * chi[0] + z[1] * chi[1] + z[2] * chi[2];
    assert_eq!(sparse_eval(&z, &y), Some(want));
    assert_eq!(sparse_eval(&z, &y[..2]), None);
    // powers and the power polynomial
    let p = powers(&fq(3), 5);
    assert_eq!(p, vec![fq(1), fq(3), fq(9), fq(27), fq(81)]);
    let x = [fq(0), fq(1), fq(1)];
    // pow(t) at a boolean point: t^(sum of x_i 2^i) = t^(2 + 4) here
    assert_eq!(power_eval(&fq(2), &x), fq(64));
}

#[test]
fn univariate_decompression_and_sumcheck() {
    let stored = [fq(5), fq(7), fq(11)];
    let hint = fq(100);
    let coefficients = decompress_univariate(&stored, &hint).expect("non-empty");
    assert_eq!(coefficients.len(), 4);
    let at_zero = eval_univariate(&coefficients, &Fq::ZERO);
    let at_one = eval_univariate(&coefficients, &Fq::ONE);
    assert_eq!(at_zero + at_one, hint);
    assert!(decompress_univariate::<FqParams>(&[], &hint).is_none());

    // wrong round count, empty polynomial, degree above 3
    let mut t = Transcript::<FqParams>::new(b"x");
    let claim = Fq::ZERO;
    assert_eq!(
        sumcheck::verify(&claim, 2, 3, &[vec![fq(1)]], &mut t).unwrap_err(),
        NovaVerifyError::InvalidSumcheckProof
    );
    let mut t = Transcript::<FqParams>::new(b"x");
    assert_eq!(
        sumcheck::verify(&claim, 1, 3, &[vec![]], &mut t).unwrap_err(),
        NovaVerifyError::InvalidSumcheckProof
    );
    let mut t = Transcript::<FqParams>::new(b"x");
    assert_eq!(
        sumcheck::verify(&claim, 1, 3, &[vec![fq(1), fq(2), fq(3), fq(4)]], &mut t).unwrap_err(),
        NovaVerifyError::InvalidSumcheckProof
    );
    // a well-formed round returns the polynomial value at the challenge
    let mut t = Transcript::<FqParams>::new(b"x");
    let (e, r) = sumcheck::verify(&hint, 1, 3, &[stored.to_vec()], &mut t).expect("valid");
    assert_eq!(e, eval_univariate(&coefficients, &r[0]));
}

// ---------------------------------------------------------------------------------------------
// hashing helpers

#[test]
fn streaming_sha3_matches_one_shot() {
    for len in [0usize, 1, 135, 136, 137, 272, 1000, 4097] {
        let data: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
        let mut stream = Sha3Stream::new();
        for chunk in data.chunks(97) {
            stream.update(chunk);
        }
        assert_eq!(stream.finalize(), sha3_256(&data), "len {len}");
    }
}

// ---------------------------------------------------------------------------------------------
// codec and key

fn meta(name: &str) -> Vec<Vec<String>> {
    rows(&format!("{name}.meta.tsv"))
}

fn meta_value(rows: &[Vec<String>], key: &str) -> String {
    rows.iter()
        .find(|r| r[0] == key)
        .unwrap_or_else(|| panic!("missing {key}"))[1]
        .clone()
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    hex_decode(fixture_text(name).trim())
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only (debug build is too slow): cargo test --release --test zk_nova_parts"
)]
fn registered_key_round_trips_and_expands_to_the_fixture_vk() {
    let name = "cubic_3steps";
    let m = meta(name);
    let nogens = fixture_bytes(&format!("{name}.vk_nogens.hex"));
    assert_eq!(nogens.len(), VK_NOGENS_BYTES);
    assert_eq!(
        hex_encode(&sha256_digest(&nogens)),
        meta_value(&m, "vk_nogens_sha256")
    );

    let key = RegisteredNovaKey::from_vk_nogens(&nogens).expect("key");
    let encoded = key.encode();
    assert_eq!(encoded.len(), REGISTERED_KEY_BYTES);
    assert_eq!(RegisteredNovaKey::decode(&encoded).expect("decode"), key);
    assert_eq!(key.sha256(), sha256_digest(&encoded));
    // trailing byte, truncation, unknown profile
    let mut longer = encoded.clone();
    longer.push(0);
    assert!(RegisteredNovaKey::decode(&longer).is_err());
    for cut in [0, 1, 40, encoded.len() - 1] {
        assert!(RegisteredNovaKey::decode(&encoded[..cut]).is_err());
    }
    let mut wrong_profile = encoded.clone();
    wrong_profile[0] = 9;
    assert_eq!(
        RegisteredNovaKey::decode(&wrong_profile).unwrap_err(),
        NovaVerifyError::Decode(DecodeError::UnknownProfile)
    );

    let cache = GeneratorCache::new();
    let started = Instant::now();
    let vk = key.expand(&cache).expect("expand");
    println!(
        "TIMING expand cold (2 x ck 2^17+1, ipa 2 per curve): {:?}",
        started.elapsed()
    );
    let started = Instant::now();
    let _again = key.expand(&cache).expect("expand warm");
    println!("TIMING expand warm: {:?}", started.elapsed());

    // vk digests and the generator-list hashes equal nova's.
    assert_eq!(
        hex_encode(&vk.primary.digest.to_repr()),
        meta_value(&m, "vk_primary_digest")
    );
    assert_eq!(
        hex_encode(&vk.secondary.digest.to_repr()),
        meta_value(&m, "vk_secondary_digest")
    );
    // The recorded hash covers `from_label(b"ck", n + 1)`: `h` first, then the `ck_v` generators.
    let concat = |h: Affine<Pallas>, points: &[Affine<Pallas>]| -> Vec<u8> {
        std::iter::once(h)
            .chain(points.iter().copied())
            .flat_map(|p| p.to_compressed())
            .collect()
    };
    assert_eq!(
        hex_encode(&sha256_digest(&concat(
            vk.primary.ck_h(),
            vk.primary.ck_v()
        ))),
        meta_value(&m, "sha256_concat_compressed_from_label_ck_primary")
    );
    let concat_v: Vec<u8> = std::iter::once(vk.secondary.ck_h())
        .chain(vk.secondary.ck_v().iter().copied())
        .flat_map(|p| p.to_compressed())
        .collect();
    assert_eq!(
        hex_encode(&sha256_digest(&concat_v)),
        meta_value(&m, "sha256_concat_compressed_from_label_ck_secondary")
    );

    // The expanded key's fixed points equal the fixture's.
    let parsed = decode_vk_nogens(&nogens).expect("parse");
    assert_eq!(vk.dk_primary, parsed.dk_primary);
    assert_eq!(vk.dk_secondary, parsed.dk_secondary);
    assert_eq!(vk.primary.ck_h(), parsed.primary.ck_v_h);
    assert_eq!(vk.primary.ck_s_h(), parsed.primary.ck_s_h);
    assert_eq!(vk.secondary.ck_h(), parsed.secondary.ck_v_h);
    assert_eq!(vk.secondary.ck_s_h(), parsed.secondary.ck_s_h);
    assert_eq!(vk.pp_digest, parsed.pp_digest);
    assert_eq!(vk.f_arity as u64, parsed.f_arity);

    // Splice the derived generator lists into the generator-free bytes: the result is the full
    // vk and its SHA-256 equals the recorded one.
    let mut inserts: Vec<(usize, Vec<u8>)> = Vec::new();
    let list = |count: usize, points: Vec<[u8; 32]>| -> Vec<u8> {
        let mut out = (count as u64).to_le_bytes().to_vec();
        for p in points {
            out.extend_from_slice(&p);
        }
        out
    };
    let ranges: Vec<(String, usize)> = m
        .iter()
        .filter(|r| r[0] == "vk_removed_generator_range")
        .map(|r| (r[1].clone(), r[2].parse().expect("offset")))
        .collect();
    assert_eq!(ranges.len(), 4);
    let ck_s_primary = vec![vk.primary.ck_s_generator().to_compressed()];
    let ck_s_secondary = vec![vk.secondary.ck_s_generator().to_compressed()];
    let contents = [
        list(
            vk.primary.ck_v().len(),
            vk.primary
                .ck_v()
                .iter()
                .map(Affine::to_compressed)
                .collect(),
        ),
        list(1, ck_s_primary),
        list(
            vk.secondary.ck_v().len(),
            vk.secondary
                .ck_v()
                .iter()
                .map(Affine::to_compressed)
                .collect(),
        ),
        list(1, ck_s_secondary),
    ];
    for ((_, offset), bytes) in ranges.iter().zip(contents) {
        inserts.push((*offset, bytes));
    }
    let mut full = Vec::new();
    let mut full_pos = 0usize;
    let mut nogens_pos = 0usize;
    for (offset, bytes) in &inserts {
        let copy = offset - full_pos;
        full.extend_from_slice(&nogens[nogens_pos..nogens_pos + copy]);
        nogens_pos += copy;
        full.extend_from_slice(bytes);
        full_pos = offset + bytes.len();
    }
    full.extend_from_slice(&nogens[nogens_pos..]);
    assert_eq!(full.len().to_string(), meta_value(&m, "vk_bytes"));
    assert_eq!(
        hex_encode(&sha256_digest(&full)),
        meta_value(&m, "vk_sha256")
    );
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn registered_key_rejects_invalid_shapes() {
    let nogens = fixture_bytes("cubic_3steps.vk_nogens.hex");
    let key = RegisteredNovaKey::from_vk_nogens(&nogens).expect("key");
    let cache = GeneratorCache::new();
    let mut top = [0u8; 32];
    top[31] = 0x04;
    let wide_digest = Fq::from_repr_slice(&top).expect("2^250 is canonical in Fq");
    let bad = |mutate: &dyn Fn(&mut RegisteredNovaKey)| {
        let mut k = key;
        mutate(&mut k);
        k.expand(&cache).err().expect("rejected")
    };
    for error in [
        bad(&|k| k.primary.num_cons = 0),
        bad(&|k| k.primary.num_cons = 3),
        bad(&|k| k.primary.num_vars = 4),
        bad(&|k| k.primary.num_vars = 24),
        bad(&|k| k.secondary.spark.n = 0),
        bad(&|k| k.secondary.spark.n = 3 << 10),
        bad(&|k| k.primary.spark.n = key.primary.num_vars),
        bad(&|k| k.primary.spark.n = 1 << 40),
        bad(&|k| k.primary.ck_len = k.primary.spark.n / 2),
        bad(&|k| k.primary.ck_len = u64::MAX),
        bad(&|k| k.f_arity = 0),
        bad(&|k| k.f_arity = u64::MAX),
        bad(&|k| k.primary.num_cons = k.primary.spark.n * 2),
        bad(&|k| k.pp_digest = wide_digest),
    ] {
        assert!(matches!(error, NovaVerifyError::InvalidKey(_)), "{error:?}");
    }
    // A pp_digest at 2^250 or above is canonical in Fq but never produced by nova; the encoded
    // key is rejected at decode too.
    let mut encoded = key.encode();
    encoded[1 + 8 + 31] = 0x04;
    assert!(matches!(
        RegisteredNovaKey::decode(&encoded),
        Err(NovaVerifyError::InvalidKey(_))
    ));
    encoded[1 + 8 + 31] = 0x03;
    assert!(RegisteredNovaKey::decode(&encoded).is_ok());
    assert!(
        cache.is_empty(),
        "a rejected key must not trigger generator work"
    );
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only (debug build is too slow): cargo test --release --test zk_nova_parts"
)]
fn codec_rejects_malformed_proofs_without_allocating() {
    let proof = fixture_bytes("cubic_3steps.proof.hex");
    let decoded = decode_compressed_snark(&proof).expect("decodes");
    assert_eq!(decoded.zn.len(), 1);
    // trailing byte
    let mut longer = proof.clone();
    longer.push(0);
    assert_eq!(
        decode_compressed_snark(&longer).unwrap_err(),
        DecodeError::TrailingBytes
    );
    // every truncation fails
    for cut in 0..proof.len() {
        assert!(decode_compressed_snark(&proof[..cut]).is_err(), "cut {cut}");
    }
    // inflate every u64 that could be a length prefix to a huge value, one at a time
    for offset in (0..proof.len() - 8).step_by(8) {
        let mut bad = proof.clone();
        bad[offset..offset + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        let _ = decode_compressed_snark(&bad);
    }
    let mut r = Reader::new(&[0xff; 8]);
    assert_eq!(r.length(32).unwrap_err(), DecodeError::LengthExceedsInput);
    let mut zero_vector = vec![0u8; 8];
    zero_vector.extend_from_slice(&[0u8; 5]);
    let mut r = Reader::new(&zero_vector);
    assert_eq!(r.length(0).expect("empty vector"), 0);
    // generator-free vk: wrong sizes
    let nogens = fixture_bytes("cubic_3steps.vk_nogens.hex");
    assert!(decode_vk_nogens(&nogens[..nogens.len() - 1]).is_err());
    let mut extended = nogens.clone();
    extended.push(0);
    assert!(decode_vk_nogens(&extended).is_err());
}
