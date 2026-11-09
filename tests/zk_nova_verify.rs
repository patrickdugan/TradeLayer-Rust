//! End-to-end tests of the zero-dependency Nova verifier against real nova-snark 0.76.0
//! `CompressedSNARK` proofs (ppsnark + IPA, Pallas/Vesta).
//!
//! Run in release mode; the NHC1 fixture derives 2^20 generators. Timings are printed with
//! `-- --nocapture --test-threads=1`.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::OnceLock;
use std::time::Instant;

use tradelayer_core::zk_nova::curve::{Pallas, Vesta};
use tradelayer_core::zk_nova::error::{DecodeError, NovaVerifyError};
use tradelayer_core::zk_nova::field::Fq;
use tradelayer_core::zk_nova::key::{GeneratorCache, RegisteredNovaKey, LABEL_CK, LABEL_IPA};
use tradelayer_core::zk_nova::{nova, verify, MAX_PROOF_BYTES};

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

fn fixture_bytes(name: &str) -> Vec<u8> {
    hex_decode(fixture_text(name).trim())
}

struct Fixture {
    name: &'static str,
    key: RegisteredNovaKey,
    proof: Vec<u8>,
    corrupt: Vec<u8>,
    num_steps: u64,
    z0: Vec<Fq>,
    zn: Vec<Fq>,
    regions: Vec<(String, usize, usize)>,
}

fn scalars(list: &str) -> Vec<Fq> {
    list.split(',')
        .map(|h| Fq::from_repr_slice(&hex_decode(h)).expect("canonical"))
        .collect()
}

fn load(name: &'static str) -> Fixture {
    let meta = fixture_text(&format!("{name}.meta.tsv"));
    let mut num_steps = 0;
    let mut z0 = Vec::new();
    let mut zn = Vec::new();
    let mut regions = Vec::new();
    for line in meta.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "num_steps" => num_steps = f[1].parse().expect("steps"),
            "z0" => z0 = scalars(f[1]),
            "zn" => zn = scalars(f[1]),
            "proof_region" => regions.push((
                f[1].to_string(),
                f[2].parse().unwrap(),
                f[3].parse().unwrap(),
            )),
            _ => {}
        }
    }
    let key = RegisteredNovaKey::from_vk_nogens(&fixture_bytes(&format!("{name}.vk_nogens.hex")))
        .expect("key");
    Fixture {
        name,
        key,
        proof: fixture_bytes(&format!("{name}.proof.hex")),
        corrupt: fixture_bytes(&format!("{name}.proof_corrupt0.hex")),
        num_steps,
        z0,
        zn,
        regions,
    }
}

fn cache() -> &'static GeneratorCache {
    static CACHE: OnceLock<GeneratorCache> = OnceLock::new();
    CACHE.get_or_init(GeneratorCache::new)
}

fn region(f: &Fixture, name: &str) -> (usize, usize) {
    let (_, offset, len) = f
        .regions
        .iter()
        .find(|(n, _, _)| n == name)
        .unwrap_or_else(|| panic!("no region {name}"));
    (*offset, *len)
}

/// Replaces `remove` bytes at `offset` with `insert`.
fn splice(proof: &[u8], offset: usize, remove: usize, insert: &[u8]) -> Vec<u8> {
    let mut out = proof[..offset].to_vec();
    out.extend_from_slice(insert);
    out.extend_from_slice(&proof[offset + remove..]);
    out
}

fn cubic() -> &'static Fixture {
    static F: OnceLock<Fixture> = OnceLock::new();
    F.get_or_init(|| load("cubic_3steps"))
}

// ---------------------------------------------------------------------------------------------

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only (debug build is too slow): cargo test --release --test zk_nova_verify"
)]
fn real_proofs_verify_and_zn_matches() {
    for name in ["cubic_3steps", "cubic_7steps", "nhc1_ppsnark_k1x10"] {
        let f = load(name);
        let zn = verify(&f.key, cache(), f.num_steps, &f.z0, &f.proof)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(zn, f.zn, "{name}");
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only (debug build is too slow): cargo test --release --test zk_nova_verify"
)]
fn timings_report() {
    for name in ["cubic_3steps", "nhc1_ppsnark_k1x10"] {
        let f = load(name);
        let fresh = GeneratorCache::new();
        let ck_p = f.key.primary.ck_len as usize + 1;
        let ck_s = f.key.secondary.ck_len as usize + 1;
        let t = Instant::now();
        fresh.generators::<Pallas>(LABEL_CK, ck_p).expect("gens");
        println!(
            "TIMING {name}: from_label Pallas ck n={ck_p}: {:?}",
            t.elapsed()
        );
        let t = Instant::now();
        fresh.generators::<Vesta>(LABEL_CK, ck_s).expect("gens");
        println!(
            "TIMING {name}: from_label Vesta  ck n={ck_s}: {:?}",
            t.elapsed()
        );
        let t = Instant::now();
        fresh.generators::<Pallas>(LABEL_IPA, 2).expect("gens");
        fresh.generators::<Vesta>(LABEL_IPA, 2).expect("gens");
        println!(
            "TIMING {name}: from_label ipa n=2 (both curves): {:?}",
            t.elapsed()
        );
        let t = Instant::now();
        let vk = f.key.expand(&fresh).expect("expand");
        println!(
            "TIMING {name}: expand with warm generators (digests, Poseidon): {:?}",
            t.elapsed()
        );
        let mut best = std::time::Duration::MAX;
        for _ in 0..3 {
            let t = Instant::now();
            let zn = nova::verify(&vk, f.num_steps, &f.z0, &f.proof).expect("verify");
            assert_eq!(zn, f.zn);
            best = best.min(t.elapsed());
        }
        println!("TIMING {name}: nova::verify with expanded key, best of 3: {best:?}");
        let mut best = std::time::Duration::MAX;
        for _ in 0..3 {
            let t = Instant::now();
            let zn = verify(&f.key, &fresh, f.num_steps, &f.z0, &f.proof).expect("verify");
            assert_eq!(zn, f.zn);
            best = best.min(t.elapsed());
        }
        println!(
            "TIMING {name}: public verify (key expanded per call, cache warm), best of 3: {best:?}"
        );
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only (debug build is too slow): cargo test --release --test zk_nova_verify"
)]
fn corrupt_proofs_are_rejected() {
    for name in ["cubic_3steps", "cubic_7steps", "nhc1_ppsnark_k1x10"] {
        let f = load(name);
        assert_ne!(f.proof, f.corrupt);
        let error = verify(&f.key, cache(), f.num_steps, &f.z0, &f.corrupt).unwrap_err();
        assert_eq!(error, NovaVerifyError::InvalidSumcheckProof, "{name}");
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only (debug build is too slow): cargo test --release --test zk_nova_verify"
)]
fn wrong_steps_wrong_z0_wrong_key_are_rejected() {
    let f = cubic();
    let ok =
        |steps: u64, z0: &[Fq], key: &RegisteredNovaKey| verify(key, cache(), steps, z0, &f.proof);

    assert_eq!(
        ok(f.num_steps + 1, &f.z0, &f.key).unwrap_err(),
        NovaVerifyError::HashBindingMismatch
    );
    assert_eq!(
        ok(f.num_steps - 1, &f.z0, &f.key).unwrap_err(),
        NovaVerifyError::HashBindingMismatch
    );
    assert_eq!(
        ok(0, &f.z0, &f.key).unwrap_err(),
        NovaVerifyError::ZeroSteps
    );
    let mut z0 = f.z0.clone();
    z0[0] += Fq::ONE;
    assert_eq!(
        ok(f.num_steps, &z0, &f.key).unwrap_err(),
        NovaVerifyError::HashBindingMismatch
    );
    assert_eq!(
        ok(f.num_steps, &[], &f.key).unwrap_err(),
        NovaVerifyError::ArityMismatch
    );
    let two = [f.z0[0], f.z0[0]];
    assert_eq!(
        ok(f.num_steps, &two, &f.key).unwrap_err(),
        NovaVerifyError::ArityMismatch
    );

    // Keys of other circuits: a different public-parameter digest, swapped spark commitments, and
    // the NHC1 key (arity 17).
    let mut other_digest = f.key;
    other_digest.pp_digest += Fq::ONE;
    assert!(ok(f.num_steps, &f.z0, &other_digest).is_err());
    let mut swapped = f.key;
    std::mem::swap(
        &mut swapped.primary.spark.comm_row,
        &mut swapped.primary.spark.comm_col,
    );
    assert!(ok(f.num_steps, &f.z0, &swapped).is_err());
    let mut swapped_secondary = f.key;
    std::mem::swap(
        &mut swapped_secondary.secondary.spark.comm_val_a,
        &mut swapped_secondary.secondary.spark.comm_val_b,
    );
    assert!(ok(f.num_steps, &f.z0, &swapped_secondary).is_err());
    let nhc1 = load("nhc1_ppsnark_k1x10");
    assert_eq!(
        verify(&nhc1.key, cache(), nhc1.num_steps, &nhc1.z0, &f.proof).unwrap_err(),
        NovaVerifyError::ArityMismatch
    );
    // Same proof bytes, the other fixture's z0 and steps with the cubic key.
    let seven = load("cubic_7steps");
    assert!(verify(&f.key, cache(), seven.num_steps, &seven.z0, &f.proof).is_err());
}

/// Byte ranges of the two inner-product arguments (`L_vec`, `R_vec`, `a_hat`), the only fields
/// whose corruption is detected after a full-size multi-scalar product.
fn ipa_ranges(f: &Fixture) -> Vec<(usize, usize)> {
    ["snark_primary", "snark_secondary"]
        .iter()
        .map(|side| {
            let (start, _) = region(f, &format!("{side}.eval_arg.L_vec"));
            let (a_hat, a_len) = region(f, &format!("{side}.eval_arg.a_hat"));
            (start, a_hat + a_len)
        })
        .collect()
}

/// Flips `mask` in every byte for which `select(index)` holds, verifying each corrupted proof on
/// worker threads; returns the indices that panicked or were accepted.
fn flip_bytes(
    f: &Fixture,
    mask: u8,
    select: &(dyn Fn(usize) -> bool + Sync),
) -> (usize, Vec<String>) {
    let vk = f.key.expand(cache()).expect("expand");
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(8);
    let len = f.proof.len();
    let results: Vec<(usize, Vec<String>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|worker| {
                let vk = &vk;
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    let mut tried = 0;
                    let mut index = worker;
                    while index < len {
                        if select(index) {
                            tried += 1;
                            let mut bad = f.proof.clone();
                            bad[index] ^= mask;
                            let outcome = catch_unwind(AssertUnwindSafe(|| {
                                nova::verify(vk, f.num_steps, &f.z0, &bad)
                            }));
                            match outcome {
                                Err(_) => failures.push(format!("byte {index}: panic")),
                                Ok(Ok(_)) => failures.push(format!("byte {index}: accepted")),
                                Ok(Err(_)) => {}
                            }
                        }
                        index += threads;
                    }
                    (tried, failures)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("worker"))
            .collect()
    });
    let tried = results.iter().map(|(t, _)| *t).sum();
    let failures = results.into_iter().flat_map(|(_, f)| f).collect();
    (tried, failures)
}

#[test]
#[ignore = "exhaustive sweep, about 25 min in release: cargo test --release --test zk_nova_verify -- --ignored"]
fn every_single_byte_flip_is_rejected_without_panic() {
    let f = cubic();
    let (tried, failures) = flip_bytes(f, 0x01, &|_| true);
    assert_eq!(tried, f.proof.len());
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
#[ignore = "exhaustive sweep, about 25 min in release: cargo test --release --test zk_nova_verify -- --ignored"]
fn flipping_the_high_bit_is_rejected_without_panic() {
    // Bit 7 of every byte outside the inner-product arguments, plus bit 7 of the top byte of every
    // point inside them (the point sign flag). The remaining argument bytes are covered by the
    // bit-0 test above.
    let f = cubic();
    let ranges = ipa_ranges(f);
    let in_ipa = move |i: usize| ranges.iter().any(|(s, e)| i >= *s && i < *e);
    let ranges2 = ipa_ranges(f);
    // L_vec and R_vec are 8 + 17 * 32 bytes each; point j ends at relative offset 8 + 32 j + 31.
    let sign_byte = move |i: usize| {
        ranges2
            .iter()
            .any(|(start, end)| i >= *start + 8 && i < *end - 32 && (i - *start - 8) % 32 == 31)
    };
    let (tried, failures) = flip_bytes(f, 0x80, &|i| !in_ipa(i) || sign_byte(i));
    assert!(tried > f.proof.len() - 2 * 1136);
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only (debug build is too slow): cargo test --release --test zk_nova_verify"
)]
fn every_truncation_is_rejected() {
    let f = cubic();
    for cut in 0..f.proof.len() {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            verify(&f.key, cache(), f.num_steps, &f.z0, &f.proof[..cut])
        }));
        match outcome {
            Ok(Err(_)) => {}
            other => panic!("cut {cut}: {other:?}"),
        }
    }
    let mut longer = f.proof.clone();
    longer.push(0);
    assert_eq!(
        verify(&f.key, cache(), f.num_steps, &f.z0, &longer).unwrap_err(),
        NovaVerifyError::Decode(DecodeError::TrailingBytes)
    );
    let huge = vec![0u8; MAX_PROOF_BYTES + 1];
    assert!(matches!(
        verify(&f.key, cache(), f.num_steps, &f.z0, &huge).unwrap_err(),
        NovaVerifyError::ProofTooLarge { .. }
    ));
}

/// Offsets of every `Vec` length prefix in the cubic proof.
fn length_prefix_offsets(f: &Fixture) -> Vec<usize> {
    let mut offsets = vec![64, 232, 400, 600, 800];
    for side in ["snark_primary", "snark_secondary"] {
        for sc in ["sc_outer", "sc_inner_batched"] {
            let (offset, len) = region(f, &format!("{side}.{sc}"));
            offsets.push(offset);
            let rounds = (len - 8) / 104;
            for i in 0..rounds {
                offsets.push(offset + 8 + 104 * i);
            }
        }
        offsets.push(region(f, &format!("{side}.eval_arg.L_vec")).0);
        offsets.push(region(f, &format!("{side}.eval_arg.R_vec")).0);
    }
    offsets.push(region(f, "zn").0);
    offsets
}

#[test]
fn inflated_length_prefixes_are_rejected_without_allocation() {
    let f = cubic();
    let offsets = length_prefix_offsets(f);
    assert!(offsets.len() > 60);
    for offset in offsets {
        let announced = u64::from_le_bytes(f.proof[offset..offset + 8].try_into().unwrap());
        assert!(announced < 64, "offset {offset} is not a length prefix");
        for value in [u64::MAX, 1 << 40, 1 << 32, 1 << 20, f.proof.len() as u64] {
            let mut bad = f.proof.clone();
            bad[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
            let outcome = catch_unwind(AssertUnwindSafe(|| nova::decode(&bad)));
            match outcome {
                Ok(Err(NovaVerifyError::Decode(
                    DecodeError::LengthExceedsInput | DecodeError::Truncated,
                ))) => {}
                other => panic!("offset {offset} value {value}: {other:?}"),
            }
        }
    }
    // The same holds when the prefix is read through the public entry point.
    let mut bad = f.proof.clone();
    bad[1256..1264].copy_from_slice(&u64::MAX.to_le_bytes());
    assert_eq!(
        verify(&f.key, cache(), f.num_steps, &f.z0, &bad).unwrap_err(),
        NovaVerifyError::Decode(DecodeError::LengthExceedsInput)
    );
}

// ---------------------------------------------------------------------------------------------
// the panic shapes of nova (spec section 11) as proofs and keys

fn expect_err(f: &Fixture, proof: &[u8], want: NovaVerifyError, what: &str) {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        verify(&f.key, cache(), f.num_steps, &f.z0, proof)
    }));
    match outcome {
        Ok(Err(got)) => assert_eq!(got, want, "{what}"),
        other => panic!("{what}: {other:?}"),
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only (debug build is too slow): cargo test --release --test zk_nova_verify"
)]
fn section_11_panic_shapes_return_typed_errors() {
    let f = cubic();
    let name = f.name;
    assert_eq!(name, "cubic_3steps");
    let (outer, outer_len) = region(f, "snark_primary.sc_outer");
    let (inner, _) = region(f, "snark_primary.sc_inner_batched");

    // A round polynomial with an empty coefficient list (nova indexes `[0]` and panics).
    let empty_poly = splice(&f.proof, outer + 8, 104, &0u64.to_le_bytes());
    expect_err(
        f,
        &empty_poly,
        NovaVerifyError::InvalidSumcheckProof,
        "empty round polynomial",
    );
    let empty_inner = splice(&f.proof, inner + 8, 104, &0u64.to_le_bytes());
    expect_err(
        f,
        &empty_inner,
        NovaVerifyError::InvalidSumcheckProof,
        "empty inner polynomial",
    );

    // Degree above three: a four-entry polynomial.
    let mut quartic = 4u64.to_le_bytes().to_vec();
    quartic.extend_from_slice(&f.proof[outer + 16..outer + 8 + 104]);
    quartic.extend_from_slice(&[0u8; 32]);
    let too_high = splice(&f.proof, outer + 8, 104, &quartic);
    expect_err(
        f,
        &too_high,
        NovaVerifyError::InvalidSumcheckProof,
        "degree four",
    );

    // Wrong number of rounds: drop the last outer round, add a round to the inner sum-check.
    let rounds = (outer_len - 8) / 104;
    let mut fewer = splice(&f.proof, outer + 8 + 104 * (rounds - 1), 104, &[]);
    fewer[outer..outer + 8].copy_from_slice(&((rounds - 1) as u64).to_le_bytes());
    expect_err(
        f,
        &fewer,
        NovaVerifyError::InvalidSumcheckProof,
        "one outer round fewer",
    );
    let mut more = splice(&f.proof, inner + 8, 0, &f.proof[inner + 8..inner + 8 + 104]);
    let inner_rounds = u64::from_le_bytes(f.proof[inner..inner + 8].try_into().unwrap());
    more[inner..inner + 8].copy_from_slice(&(inner_rounds + 1).to_le_bytes());
    expect_err(
        f,
        &more,
        NovaVerifyError::InvalidSumcheckProof,
        "one inner round more",
    );

    // IPA lengths: |L| = 33 (above 31), |L| != |R|, |L| one below the round count.
    let (l_off, l_len) = region(f, "snark_primary.eval_arg.L_vec");
    let (r_off, r_len) = region(f, "snark_primary.eval_arg.R_vec");
    assert_eq!((l_len, r_len), (552, 552));
    let grow = |extra: usize, region_offset: usize| {
        let mut bytes = f.proof.clone();
        let count = u64::from_le_bytes(bytes[region_offset..region_offset + 8].try_into().unwrap());
        let tail = vec![0u8; 32 * extra];
        bytes = splice(&bytes, region_offset + 8 + 32 * count as usize, 0, &tail);
        bytes[region_offset..region_offset + 8]
            .copy_from_slice(&(count + extra as u64).to_le_bytes());
        bytes
    };
    let both_33 = {
        let first = grow(16, l_off);
        // R moves by 16 * 32 bytes after growing L.
        let mut bytes = first;
        let r_new = r_off + 16 * 32;
        let count = u64::from_le_bytes(bytes[r_new..r_new + 8].try_into().unwrap());
        let tail = vec![0u8; 32 * 16];
        bytes = splice(&bytes, r_new + 8 + 32 * count as usize, 0, &tail);
        bytes[r_new..r_new + 8].copy_from_slice(&(count + 16).to_le_bytes());
        bytes
    };
    expect_err(f, &both_33, NovaVerifyError::InvalidInputLength, "|L| = 33");
    expect_err(
        f,
        &grow(1, l_off),
        NovaVerifyError::InvalidInputLength,
        "|L| != |R|",
    );
    let shorter = {
        let mut bytes = splice(&f.proof, l_off + 8 + 32 * 16, 32, &[]);
        bytes[l_off..l_off + 8].copy_from_slice(&16u64.to_le_bytes());
        let r_new = r_off - 32;
        bytes = splice(&bytes, r_new + 8 + 32 * 16, 32, &[]);
        bytes[r_new..r_new + 8].copy_from_slice(&16u64.to_le_bytes());
        bytes
    };
    expect_err(
        f,
        &shorter,
        NovaVerifyError::InvalidInputLength,
        "|L| = |R| = k - 1",
    );

    // Public-value counts: X with three entries in r_U_primary.
    let mut three = splice(&f.proof, 600 + 8 + 64, 0, &[0u8; 32]);
    three[600..608].copy_from_slice(&3u64.to_le_bytes());
    expect_err(
        f,
        &three,
        NovaVerifyError::InstanceLength,
        "three public values",
    );
    let mut one = splice(&f.proof, 232 + 8 + 32, 32, &[]);
    one[232..240].copy_from_slice(&1u64.to_le_bytes());
    expect_err(f, &one, NovaVerifyError::InstanceLength, "one public value");
    let mut none = splice(&f.proof, 800 + 8, 64, &[]);
    none[800..808].copy_from_slice(&0u64.to_le_bytes());
    expect_err(
        f,
        &none,
        NovaVerifyError::InstanceLength,
        "no public values",
    );

    // Empty and oversized state vector.
    let mut no_zn = f.proof[..region(f, "zn").0].to_vec();
    no_zn.extend_from_slice(&0u64.to_le_bytes());
    expect_err(f, &no_zn, NovaVerifyError::ArityMismatch, "empty zn");
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only (debug build is too slow): cargo test --release --test zk_nova_verify"
)]
fn identity_points_encode_two_ways_and_both_decode() {
    // nova accepts x = 0 with either sign bit as the identity; the decoder follows. Replace the
    // `nifs_Un_primary` commitment: the proof must then fail verification, not decoding.
    let f = cubic();
    let (offset, _) = region(f, "nifs_Un_primary");
    for tail in [0x00u8, 0x80u8] {
        let mut bytes = f.proof.clone();
        bytes[offset..offset + 31].fill(0);
        bytes[offset + 31] = tail;
        assert!(nova::decode(&bytes).is_ok(), "tail {tail:#x}");
        assert!(verify(&f.key, cache(), f.num_steps, &f.z0, &bytes).is_err());
    }
}
