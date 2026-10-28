use tradelayer_core::read_only::litecoin_transparent::*;
use tradelayer_core::read_only::{decode_hex, hex_bytes, id_hex};
use tradelayer_core::validity_source::SourceValue as V;
const P: LitecoinTransparentProfile = LitecoinTransparentProfile::Core021MwebStructureV2;
fn text(v: V) -> String {
    if let V::String(s) = v {
        s
    } else {
        panic!("expected string {v:?}")
    }
}
fn number(v: V) -> u64 {
    if let V::Number(n) = v {
        f64::from_bits(n) as u64
    } else {
        panic!("expected number")
    }
}
fn money(v: V) -> u64 {
    if let V::Number(n) = v {
        (f64::from_bits(n) * 100000000.).round() as u64
    } else {
        panic!("expected amount")
    }
}
fn list(v: V) -> Vec<V> {
    if let V::Array(a) = v {
        a
    } else {
        panic!("expected array")
    }
}
fn cases() -> Vec<V> {
    include_str!("../fixtures/litecoin_core_regtest_goldens.jsonl")
        .lines()
        .map(|s| tradelayer_core::property_catalog::parse_diagnostic_value(s).unwrap())
        .collect()
}
fn check_body(raw: &[u8], body: &UnverifiedMwebBody, core: &V) {
    let inputs = list(core.get("inputs"));
    let outputs = list(core.get("outputs"));
    let kernels = list(core.get("kernels"));
    assert_eq!(
        (body.inputs.len(), body.outputs.len(), body.kernels.len()),
        (inputs.len(), outputs.len(), kernels.len())
    );
    for (i, c) in body.inputs.iter().zip(inputs) {
        for (span, key) in [
            (&i.output_id, "output_id"),
            (&i.commitment, "commit"),
            (&i.output_pubkey, "output_pubkey"),
            (&i.signature, "sig"),
        ] {
            assert_eq!(hex_bytes(&raw[span.clone()]), text(c.get(key)));
        }
        if let Some(span) = &i.input_pubkey {
            assert_eq!(hex_bytes(&raw[span.clone()]), text(c.get("input_pubkey")));
        }
    }
    for (o, c) in body.outputs.iter().zip(outputs) {
        for (span, key) in [
            (&o.commitment, "commit"),
            (&o.sender_pubkey, "sender_pubkey"),
            (&o.receiver_pubkey, "receiver_pubkey"),
            (&o.range_proof, "range_proof"),
            (&o.message, "message"),
        ] {
            assert_eq!(hex_bytes(&raw[span.clone()]), text(c.get(key)));
        }
        assert_eq!(
            hex_bytes(&o.output_id_raw(raw).unwrap()),
            text(c.get("output_id"))
        );
        assert_eq!(o.range_proof.len(), 675);
    }
    for (k, c) in body.kernels.iter().zip(kernels) {
        assert_eq!(u64::from(k.features), number(c.get("features")));
        assert_eq!(k.fee.unwrap_or(0), number(c.get("fee")));
        assert_eq!(
            u64::from(k.lock_height.unwrap_or(0)),
            number(c.get("lock_height"))
        );
        assert_eq!(hex_bytes(&raw[k.excess.clone()]), text(c.get("excess")));
        assert_eq!(
            hex_bytes(&raw[k.signature.clone()]),
            text(c.get("signature"))
        );
        assert_eq!(
            hex_bytes(&k.kernel_id_raw(raw).unwrap()),
            text(c.get("kernel_id"))
        );
        assert_eq!(k.canonical_bytes(raw).unwrap(), raw[k.range.clone()]);
    }
    assert_eq!(body.canonical_bytes(raw).unwrap(), raw[body.range.clone()]);
}
#[test]
fn all_seven_real_core_blocks_match_every_exposed_field_id_and_roundtrip() {
    let mut count = 0;
    let mut body_count = 0;
    for row in cases() {
        let core = row.get("core");
        if core.get("decodedByCore") != V::Undefined {
            continue;
        }
        let raw = decode_hex(&text(row.get("rawHex"))).unwrap();
        let block = UnverifiedLitecoinTransparentBlock::parse(&raw, P, None).unwrap();
        count += 1;
        assert_eq!(id_hex(&block.block_id_raw()), text(core.get("hash")));
        assert_eq!(
            id_hex(&block.transparent_merkle_root_raw()),
            text(core.get("merkleroot"))
        );
        assert_eq!(block.transparent_weight(), number(core.get("weight")));
        assert_eq!(block.canonical_bytes().unwrap(), raw);
        let txs = list(core.get("tx"));
        assert_eq!(block.transactions().len(), txs.len());
        for (tx, c) in block.transactions().iter().zip(txs) {
            let o = tx.inspect();
            assert_eq!(id_hex(&o.txid), text(c.get("txid")));
            assert_eq!(id_hex(&o.wtxid), text(c.get("hash")));
            assert_eq!(tx.canonical_bytes().unwrap(), tx.raw_bytes());
        }
        let mw = core.get("mweb");
        if mw == V::Undefined {
            assert!(block.mweb_body().is_none());
            continue;
        }
        let header = block.mweb_header().unwrap();
        assert_eq!(u64::from(header.height), number(mw.get("height")));
        assert_eq!(header.output_mmr_size, number(mw.get("num_txos")));
        assert_eq!(header.kernel_mmr_size, number(mw.get("num_kernels")));
        for (bytes, key) in [
            (&header.output_root, "output_root"),
            (&header.kernel_root, "kernel_root"),
            (&header.leafset_root, "leaf_root"),
            (&header.kernel_offset, "kernel_offset"),
            (&header.stealth_offset, "stealth_offset"),
        ] {
            assert_eq!(hex_bytes(bytes), text(mw.get(key)));
        }
        assert_eq!(
            hex_bytes(&block.mweb_header_id_raw().unwrap()),
            text(mw.get("hash"))
        );
        let body = block.mweb_body().unwrap();
        check_body(&raw, body, &mw);
        body_count += 1;
        if number(mw.get("height")) == 444 {
            assert_eq!(body.kernels[0].pegouts.len(), 1);
            assert_eq!(body.kernels[0].pegouts[0].amount, 10_000_000);
        }
        assert!(block.require_full_litecoin_consensus().is_err());
    }
    assert_eq!((count, body_count), (7, 6));
}
#[test]
fn hybrid_and_pure_mweb_envelopes_use_the_genuine_core_identity_algorithm() {
    let mut count = 0;
    let mut pure = 0;
    for row in cases() {
        let c = row.get("core").get("decodedByCore");
        if c == V::Undefined {
            continue;
        }
        let raw = decode_hex(&text(row.get("rawHex"))).unwrap();
        let envelope = UnverifiedLitecoinTransactionEnvelope::parse(&raw, P).unwrap();
        count += 1;
        assert_eq!(envelope.canonical_bytes().unwrap(), raw);
        let kernels = list(c.get("vkern"));
        let mweb = envelope.mweb_transaction().unwrap();
        assert_eq!(mweb.body.kernels.len(), kernels.len());
        for (k, g) in mweb.body.kernels.iter().zip(kernels) {
            assert_eq!(
                hex_bytes(&k.kernel_id_raw(&raw).unwrap()),
                text(g.get("kernel_id"))
            );
            assert_eq!(k.fee.unwrap_or(0), money(g.get("fee")));
            assert_eq!(k.pegin.unwrap_or(0), money(g.get("pegin")));
            let outs = list(g.get("pegout"));
            assert_eq!(k.pegouts.len(), outs.len());
            for (p, g) in k.pegouts.iter().zip(outs) {
                assert_eq!(p.amount, money(g.get("value")));
                assert_eq!(
                    hex_bytes(&raw[p.script.clone()]),
                    text(g.get("scriptPubKey").get("hex"))
                );
            }
        }
        match envelope.identity().unwrap() {
            UnverifiedLitecoinTransactionIdentity::TransparentSha256d { txid, wtxid } => {
                assert!(envelope.transparent().is_some());
                assert_eq!(id_hex(&txid), text(c.get("txid")));
                assert_eq!(id_hex(&wtxid), text(c.get("hash")));
            }
            UnverifiedLitecoinTransactionIdentity::MwebFirstKernelBlake3 { txid, kernel_id } => {
                pure += 1;
                assert!(envelope.transparent().is_none());
                assert_eq!(txid, kernel_id);
                assert_eq!(id_hex(&txid), text(c.get("txid")));
                assert!(matches!(
                    UnverifiedLitecoinTransparentTransaction::parse(&raw, P),
                    Err(LitecoinTransparentError::UnsupportedMwebOnlyTransaction)
                ));
            }
        }
    }
    assert_eq!((count, pure), (4, 2));
}

#[test]
fn official_offline_codec_optional_fields_source_errors_and_omission_rules_match() {
    let mut count = 0;
    let mut accepted = 0;
    let mut differences = 0;
    let mut source_errors = 0;
    let mut future_refusals = 0;
    for line in include_str!("../fixtures/litecoin_core_codec_goldens.jsonl").lines() {
        let c = tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap();
        let name = text(c.get("name"));
        let raw = decode_hex(&text(c.get("rawHex"))).unwrap();
        count += 1;
        let result = UnverifiedLitecoinTransactionEnvelope::parse(&raw, P);
        if text(c.get("scope")) == "unknown-feature-refusal" {
            future_refusals += 1;
            assert_eq!(number(c.get("decoderExit")), 0);
            assert!(
                matches!(
                    result,
                    Err(LitecoinTransparentError::UnsupportedMwebFeatures { .. })
                ),
                "{name}"
            );
            continue;
        }
        if number(c.get("decoderExit")) != 0 {
            source_errors += 1;
            assert!(
                result.is_err(),
                "Core source error must not become Rust success: {name}"
            );
            continue;
        }
        let envelope = result.unwrap_or_else(|e| panic!("{name}: {e:?}"));
        accepted += 1;
        let canonical = decode_hex(&text(c.get("canonicalHex"))).unwrap();
        assert_eq!(
            envelope.canonical_bytes().unwrap(),
            canonical,
            "genuine Core Serialize result: {name}"
        );
        differences += usize::from(raw != canonical);
        if raw != canonical {
            assert_ne!(
                number(c.get("canonicalDecoderExit")),
                0,
                "preserve Core serialization defect: {name}"
            );
        }
        let core = c.get("decoded");
        match envelope.identity().unwrap() {
            UnverifiedLitecoinTransactionIdentity::MwebFirstKernelBlake3 { txid, kernel_id } => {
                assert_eq!(id_hex(&txid), text(core.get("txid")), "{name}");
                assert_eq!(txid, kernel_id);
            }
            _ => panic!("derived source input stays a pure MWEB object"),
        }
        let m = envelope.mweb_transaction().unwrap();
        let outputs = list(core.get("vout"));
        assert_eq!(m.body.outputs.len(), outputs.len());
        // Tx::GetOutputIDs exposes a std::set, while the body retains wire
        // vector order. Mutating a message can change only that set order.
        let mut actual = m
            .body
            .outputs
            .iter()
            .map(|o| hex_bytes(&o.output_id_raw(&raw).unwrap()))
            .collect::<Vec<_>>();
        let mut expected = outputs
            .into_iter()
            .map(|g| text(g.get("output_id")))
            .collect::<Vec<_>>();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "{name}");
        let kernels = list(core.get("vkern"));
        for (k, g) in m.body.kernels.iter().zip(kernels) {
            assert_eq!(
                hex_bytes(&k.kernel_id_raw(&raw).unwrap()),
                text(g.get("kernel_id")),
                "{name}"
            );
        }
    }
    assert_eq!(
        (count, accepted, differences, source_errors, future_refusals),
        (29, 22, 3, 4, 3)
    );
}

#[test]
fn every_genuine_nonempty_boundary_is_bounded_and_no_tail_is_skipped() {
    for row in cases() {
        let raw = decode_hex(&text(row.get("rawHex"))).unwrap();
        let envelope = row.get("core").get("decodedByCore") != V::Undefined;
        for end in 0..raw.len() {
            let failed = if envelope {
                UnverifiedLitecoinTransactionEnvelope::parse(&raw[..end], P).is_err()
            } else {
                UnverifiedLitecoinTransparentBlock::parse(&raw[..end], P, None).is_err()
            };
            assert!(failed, "{} truncated at {end}", text(row.get("name")));
        }
        let mut extra = raw.clone();
        extra.push(0);
        if envelope {
            assert!(matches!(
                UnverifiedLitecoinTransactionEnvelope::parse(&extra, P),
                Err(LitecoinTransparentError::TrailingBytes)
            ));
        } else {
            assert!(matches!(
                UnverifiedLitecoinTransparentBlock::parse(&extra, P, None),
                Err(LitecoinTransparentError::TrailingBytes)
            ));
        }
    }
}

#[test]
fn syntactic_crypto_payloads_never_become_mweb_validity_or_chain_authority() {
    let row = cases()
        .into_iter()
        .find(|r| text(r.get("name")) == "pegin_block_432")
        .unwrap();
    let raw = decode_hex(&text(row.get("rawHex"))).unwrap();
    let original = UnverifiedLitecoinTransparentBlock::parse(&raw, P, None).unwrap();
    let output = &original.mweb_body().unwrap().outputs[0];
    let old_id = output.output_id_raw(&raw).unwrap();
    let position = output.range_proof.start;
    let mut changed = raw.clone();
    changed[position] ^= 1;
    let observed = UnverifiedLitecoinTransparentBlock::parse(&changed, P, None).unwrap();
    assert_eq!(observed.block_id_raw(), original.block_id_raw());
    assert_ne!(
        observed.mweb_body().unwrap().outputs[0]
            .output_id_raw(&changed)
            .unwrap(),
        old_id
    );
    assert!(observed.require_full_litecoin_consensus().is_err());
    assert!(UnverifiedLitecoinTransparentBlock::parse(
        &raw,
        LitecoinTransparentProfile::Core021EmptyMwebV1,
        None
    )
    .is_err());
}

#[test]
fn full_profile_size_and_mweb_count_caps_refuse_before_allocating_declared_payloads() {
    let oversized = vec![0; MAX_LITECOIN_MWEB_STRUCTURE_BLOCK_BYTES + 1];
    assert!(matches!(
        UnverifiedLitecoinTransparentBlock::parse(&oversized, P, None),
        Err(LitecoinTransparentError::OversizedBlock)
    ));
    let row = cases()
        .into_iter()
        .find(|r| text(r.get("name")) == "pegin_block_432")
        .unwrap();
    let raw = decode_hex(&text(row.get("rawHex"))).unwrap();
    let block = UnverifiedLitecoinTransparentBlock::parse(&raw, P, None).unwrap();
    let at = block.mweb_body().unwrap().range.start;
    let mut excessive = raw[..at].to_vec();
    excessive.push(254);
    excessive.extend(100001u32.to_le_bytes());
    assert!(matches!(
        UnverifiedLitecoinTransparentBlock::parse(&excessive, P, None),
        Err(LitecoinTransparentError::Syntax(
            tradelayer_core::read_only::ScanError::TooLong
        ))
    ));
}
