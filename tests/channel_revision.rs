use tradelayer_core::chain::{BlockId, Tip};
use tradelayer_core::channel_revision::*;
use tradelayer_core::ledger::PropertyId;
use tradelayer_core::network_profile::TradeLayerMainnet;

const FIXTURE: &str = include_str!("../fixtures/channel_revision_vectors.tsv");
fn bytes(s: &str) -> Vec<u8> {
    if s == "-" {
        return Vec::new();
    }
    assert_eq!(s.len() % 2, 0);
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(core::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn hash(s: &str) -> [u8; 32] {
    bytes(s).try_into().unwrap()
}
fn row(name: &str) -> Vec<&'static str> {
    FIXTURE
        .lines()
        .find(|l| l.starts_with(&format!("V\t{name}\t")))
        .unwrap()
        .split('\t')
        .collect()
}
fn fixture(
    name: &str,
) -> (
    ChannelRevisionPolicy,
    Vec<AllocationRow>,
    RevisionHead,
    RevisionContext,
    SignedRevision,
) {
    let f = row(name);
    (
        parse_policy(&bytes(f[2])).unwrap(),
        parse_allocation(&bytes(f[4])).unwrap(),
        RevisionHead {
            sequence: f[5].parse().unwrap(),
            revision_id: hash(f[6]),
            allocation_digest: hash(f[7]),
        },
        RevisionContext {
            network: TradeLayerMainnet::from_tag(f[8].parse().unwrap()).unwrap(),
            parent: Tip {
                height: f[9].parse().unwrap(),
                id: BlockId(hash(f[10])),
            },
            height: f[11].parse().unwrap(),
        },
        parse_signed_revision(&bytes(f[13])).unwrap(),
    )
}

#[test]
fn independent_genuine_two_signature_transcripts_match_every_canonical_byte_and_transition() {
    let mut count = 0;
    for line in FIXTURE.lines().filter(|l| l.starts_with("V\t")) {
        let f = line.split('\t').collect::<Vec<_>>();
        let (policy, before, head, context, signed) = fixture(f[1]);
        assert_eq!(
            canonical_policy_bytes(&policy).unwrap(),
            bytes(f[2]),
            "{}",
            f[1]
        );
        assert_eq!(policy_id(&policy).unwrap(), hash(f[3]));
        assert_eq!(canonical_allocation_bytes(&before).unwrap(), bytes(f[4]));
        assert_eq!(
            canonical_revision_body_bytes(&signed.revision).unwrap(),
            bytes(f[12])
        );
        assert_eq!(parse_revision_body(&bytes(f[12])).unwrap(), signed.revision);
        assert_eq!(
            canonical_signed_revision_bytes(&signed).unwrap(),
            bytes(f[13])
        );
        assert_eq!(revision_id(&signed.revision).unwrap(), hash(f[14]));
        assert_eq!(
            bootstrap_allocation_commitment(&before).unwrap(),
            hash(f[16])
        );
        if head.sequence == 0 {
            assert_eq!(initial_head(&policy, &before).unwrap(), head);
        }
        assert!(verify_revision_signatures(&policy, &signed).unwrap());
        let after = validate_transition(&policy, &head, &before, context, &signed).unwrap();
        assert_eq!(after.sequence, head.sequence + 1);
        assert_eq!(after.revision_id, hash(f[14]));
        assert_eq!(after.allocation_digest, hash(f[15]));
        assert_eq!(
            after.require_authenticated_admission(),
            Err(RevisionError::AuthenticationUnavailable)
        );
        assert_eq!(
            after.require_l1_revocation(),
            Err(RevisionError::L1RevocationUnavailable)
        );
        assert_eq!(
            policy.require_authenticated_binding(),
            Err(RevisionError::AuthenticationUnavailable)
        );
        count += 1;
    }
    assert_eq!(count, 19);
}

#[test]
fn genuine_signatures_reject_high_s_wrong_slots_scalars_and_message_mutations() {
    let mut count = 0;
    for line in FIXTURE.lines().filter(|l| l.starts_with("S\t")) {
        let f = line.split('\t').collect::<Vec<_>>();
        let policy = parse_policy(&bytes(f[2])).unwrap();
        let signed = parse_signed_revision(&bytes(f[3])).unwrap();
        let actual = match verify_revision_signatures(&policy, &signed) {
            Ok(v) => v.to_string(),
            Err(e) => format!("{e:?}"),
        };
        assert_eq!(actual, f[4], "{}", f[1]);
        count += 1;
    }
    assert_eq!(count, 30);
}

#[test]
fn adversarial_canonical_parsers_reject_the_independent_malformed_corpus() {
    let mut count = 0;
    for line in FIXTURE.lines().filter(|l| l.starts_with("X\t")) {
        let f = line.split('\t').collect::<Vec<_>>();
        let b = bytes(f[3]);
        let error = match f[2] {
            "signed" => parse_signed_revision(&b).unwrap_err(),
            "body" => parse_revision_body(&b).unwrap_err(),
            "policy" => parse_policy(&b).unwrap_err(),
            "allocation" => parse_allocation(&b).unwrap_err(),
            _ => panic!("unknown parser"),
        };
        assert_eq!(format!("{error:?}"), f[4], "{}", f[1]);
        count += 1;
    }
    assert_eq!(count, 32);
}

#[test]
fn actual_context_custody_and_exact_successor_are_required_even_for_signed_bytes() {
    let (policy, before, head, context, signed) = fixture("network-1");
    let mut forged = head;
    forged.sequence = 1;
    assert_eq!(
        validate_transition(&policy, &forged, &before, context, &signed),
        Err(RevisionError::WrongSequence)
    );
    forged = head;
    forged.revision_id[0] ^= 1;
    assert_eq!(
        validate_transition(&policy, &forged, &before, context, &signed),
        Err(RevisionError::WrongPreviousHead)
    );
    forged = head;
    forged.allocation_digest[0] ^= 1;
    assert_eq!(
        validate_transition(&policy, &forged, &before, context, &signed),
        Err(RevisionError::WrongCustody)
    );
    let mut wrong = context;
    wrong.network = TradeLayerMainnet::Dogecoin;
    assert_eq!(
        validate_transition(&policy, &head, &before, wrong, &signed),
        Err(RevisionError::WrongContext)
    );
    wrong = context;
    wrong.parent.id.0[0] ^= 1;
    assert_eq!(
        validate_transition(&policy, &head, &before, wrong, &signed),
        Err(RevisionError::WrongContext)
    );
    wrong = context;
    wrong.height = 2;
    assert_eq!(
        validate_transition(&policy, &head, &before, wrong, &signed),
        Err(RevisionError::WrongContext)
    );
    let mut custody = before.clone();
    custody[0].atoms_a += 1;
    assert_eq!(
        validate_transition(&policy, &head, &custody, context, &signed),
        Err(RevisionError::WrongCustody)
    );
    custody = before.clone();
    custody.swap(0, 1);
    assert_eq!(
        validate_transition(&policy, &head, &custody, context, &signed),
        Err(RevisionError::NonCanonical)
    );
    let mut changed = policy.clone();
    changed.bootstrap_allocation_commitment[0] ^= 1;
    assert_eq!(
        initial_head(&changed, &before),
        Err(RevisionError::WrongBootstrap)
    );
    assert!(validate_transition(&policy, &head, &before, context, &signed).is_ok());
}

#[test]
fn every_signed_body_field_is_bound_and_signatures_cannot_supply_funding() {
    let (policy, before, head, context, signed) = fixture("network-1");
    for variant in 0..13 {
        let mut changed = signed.clone();
        let b = &mut changed.revision;
        match variant {
            0 => b.policy_id[0] ^= 1,
            1 => b.sequence += 1,
            2 => b.previous_revision_id[0] ^= 1,
            3 => b.before_allocation_digest[0] ^= 1,
            4 => b.parent.height += 1,
            5 => b.parent.id.0[0] ^= 1,
            6 => b.valid_from_height += 1,
            7 => b.valid_until_height += 1,
            8 => {
                b.allocation[0].atoms_a += 1;
                b.allocation[0].atoms_b -= 1;
            }
            9 => b.allocation[0].atoms_a += 1,
            10 => b.allocation[0].property = PropertyId::try_from(8).unwrap(),
            11 => {
                b.allocation.pop();
            }
            _ => {
                b.allocation.push(AllocationRow {
                    property: 10.try_into().unwrap(),
                    atoms_a: 1,
                    atoms_b: 0,
                });
            }
        }
        assert!(
            validate_transition(&policy, &head, &before, context, &changed).is_err(),
            "variant{variant}"
        );
    }
    for variant in 0..11 {
        let mut changed = policy.clone();
        match variant {
            0 => changed.network = TradeLayerMainnet::Litecoin,
            1 => changed.channel = "other-channel".try_into().unwrap(),
            2 => changed.participant_a = "other-alice".try_into().unwrap(),
            3 => changed.participant_b = "other-bob".try_into().unwrap(),
            4 => changed.public_key_a[0] ^= 1,
            5 => changed.funding_epoch += 1,
            6 => changed.funding_outpoints[0].txid_display[0] ^= 1,
            7 => changed.funding_outpoints[0].vout += 1,
            8 => changed.script_pubkey_sha256[0] ^= 1,
            9 => changed.bootstrap_ledger_commitment[0] ^= 1,
            _ => changed.bootstrap_allocation_commitment[0] ^= 1,
        }
        assert!(
            validate_transition(&changed, &head, &before, context, &signed).is_err(),
            "policy{variant}"
        );
    }
}

#[test]
fn chained_revision_replay_cannot_reuse_a_mathematically_valid_superseded_head() {
    for network in 1..=3 {
        let (policy, before, head, context, first) = fixture(&format!("network-{network}"));
        let h1 = validate_transition(&policy, &head, &before, context, &first).unwrap();
        let (p2, before2, head2, context2, second) = fixture(&format!("second-{network}"));
        assert_eq!(policy, p2);
        assert_eq!(head2, h1);
        assert_eq!(before2, first.revision.allocation);
        let h2 = validate_transition(&policy, &h1, &before2, context2, &second).unwrap();
        assert_eq!(h2.sequence, 2);
        assert_ne!(h1.revision_id, h2.revision_id);
        assert!(verify_revision_signatures(&policy, &first).unwrap());
        assert!(validate_transition(&policy, &h1, &before2, context, &first).is_err());
        assert!(
            validate_transition(&policy, &h2, &second.revision.allocation, context2, &second)
                .is_err()
        );
        let mut max = head2;
        max.sequence = u64::MAX;
        assert_eq!(
            validate_transition(&policy, &max, &before2, context2, &second),
            Err(RevisionError::WrongSequence)
        );
    }
}

#[test]
fn borrowed_encoding_preflights_caps_and_allocation_totals_before_crypto() {
    let (policy, before, head, context, mut signed) = fixture("network-1");
    let mut rows = vec![
        AllocationRow {
            property: 1.try_into().unwrap(),
            atoms_a: 1,
            atoms_b: 0
        };
        MAX_PROPERTIES + 1
    ];
    assert_eq!(
        canonical_allocation_bytes(&rows),
        Err(RevisionError::Capacity)
    );
    signed.revision.allocation = rows.clone();
    assert_eq!(
        validate_transition(&policy, &head, &before, context, &signed),
        Err(RevisionError::Capacity)
    );
    rows.truncate(1);
    rows[0].atoms_a = u128::MAX;
    rows[0].atoms_b = 1;
    assert_eq!(
        canonical_allocation_bytes(&rows),
        Err(RevisionError::Arithmetic)
    );
    let mut p = policy.clone();
    p.funding_outpoints = vec![p.funding_outpoints[0]; MAX_FUNDING_OUTPOINTS + 1];
    assert_eq!(canonical_policy_bytes(&p), Err(RevisionError::Capacity));
    assert_eq!(
        parse_signed_revision(&vec![0; MAX_SIGNED_REVISION_BYTES + 1]),
        Err(RevisionError::Capacity)
    );
    assert_eq!(
        parse_policy(&vec![0; MAX_POLICY_BYTES + 1]),
        Err(RevisionError::Capacity)
    );
    let mut encoded = canonical_allocation_bytes(&before).unwrap();
    encoded[0..2].copy_from_slice(&257u16.to_be_bytes());
    assert_eq!(parse_allocation(&encoded), Err(RevisionError::Capacity));
    let mut body = fixture("network-1").4.revision;
    body.valid_from_height = 0;
    body.valid_until_height = 0;
    assert_eq!(
        validate_transition(
            &policy,
            &head,
            &before,
            context,
            &SignedRevision {
                revision: body,
                ..fixture("network-1").4
            }
        ),
        Err(RevisionError::Expired)
    );
}

#[test]
fn all_canonical_entry_points_reject_truncation_trailing_and_reserved_policy_accounts() {
    let (policy, _, _, _, signed) = fixture("network-1");
    let d = canonical_policy_bytes(&policy).unwrap();
    let p = canonical_revision_body_bytes(&signed.revision).unwrap();
    let s = canonical_signed_revision_bytes(&signed).unwrap();
    for n in 0..d.len() {
        assert!(parse_policy(&d[..n]).is_err(), "policy prefix{n}");
    }
    for n in 0..p.len() {
        assert!(parse_revision_body(&p[..n]).is_err(), "body prefix{n}");
    }
    for n in 0..s.len() {
        assert!(parse_signed_revision(&s[..n]).is_err(), "signed prefix{n}");
    }
    let mut p2 = policy;
    p2.channel = "~econ:forbidden".try_into().unwrap();
    assert_eq!(
        canonical_policy_bytes(&p2),
        Err(RevisionError::InvalidPolicy)
    );
}
