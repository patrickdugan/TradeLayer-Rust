use tradelayer_core::property_catalog::{
    parse_diagnostic_value, CatalogError, DatabaseFailure, PropertyCatalog, MAX_HISTORY,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};

struct Tally {
    fail: bool,
    calls: Vec<DependencyCall>,
}
impl SourceBackend for Tally {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        assert_eq!(call.target, "Tally.updateBalance");
        self.calls.push(call.clone());
        Ok(if self.fail {
            DependencyReturn::PendingThrow(SourceException {
                name: "Error".into(),
                message: "injected tally failure".into(),
            })
        } else {
            DependencyReturn::Pending(V::Undefined)
        })
    }
}
fn array(v: &V) -> &[V] {
    let V::Array(a) = v else {
        panic!("expected array: {v:?}")
    };
    a
}
fn text(v: &V) -> &str {
    let V::String(s) = v else {
        panic!("expected string: {v:?}")
    };
    s
}
fn option(v: &V) -> Option<V> {
    let a = array(v);
    assert!(a.len() <= 1);
    a.first().cloned()
}

#[test]
fn complete_unchanged_source_corpus_matches_values_exceptions_effects_cache_and_document() {
    let mut count = 0;
    for line in include_str!("../fixtures/property_catalog_vectors.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let name = columns[0];
        let initial =
            parse_diagnostic_value(columns[1]).unwrap_or_else(|e| panic!("{name}: initial {e:?}"));
        let actions =
            parse_diagnostic_value(columns[2]).unwrap_or_else(|e| panic!("{name}: actions {e:?}"));
        let expected =
            parse_diagnostic_value(columns[3]).unwrap_or_else(|e| panic!("{name}: expected {e:?}"));
        let mut catalog = PropertyCatalog::from_document(option(&initial)).unwrap();
        let mut results = Vec::new();
        for action in array(&actions) {
            let a = array(action);
            if text(&a[0]) == "document" {
                catalog.replace_document(option(&a[1])).unwrap();
                results.push(V::Array(vec![V::String("document".into())]));
                continue;
            }
            let failures: Vec<_> = array(&a[3])
                .iter()
                .map(|f| {
                    let f = array(f);
                    let e = array(&f[1]);
                    DatabaseFailure {
                        operation: text(&f[0]).parse().unwrap(),
                        exception: SourceException {
                            name: text(&e[0]).into(),
                            message: text(&e[1]).into(),
                        },
                    }
                })
                .collect();
            let mut tally = Tally {
                fail: a[4] == V::Bool(true),
                calls: Vec::new(),
            };
            if text(&a[1]) == "getTallyMap" {
                assert_eq!(
                    catalog.execute("getTallyMap", array(&a[2]), &mut tally),
                    Err(CatalogError::UnsupportedDomain)
                );
                results.push(V::Array(vec![
                    V::String("unsupported".into()),
                    V::String("module-export-reference".into()),
                ]));
                continue;
            }
            let evaluation = catalog
                .execute_with_faults(text(&a[1]), array(&a[2]), &failures, &mut tally)
                .unwrap_or_else(|e| panic!("{name}: {}: {e:?}", text(&a[1])));
            results.push(evaluation.diagnostic_value());
        }
        let actual = V::Array(vec![
            V::Array(results),
            V::Array(catalog.document().cloned().into_iter().collect()),
            V::Array(
                catalog
                    .cache_entries()
                    .iter()
                    .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                    .collect(),
            ),
        ]);
        assert_eq!(actual, expected, "vector {name}");
        let bytes = catalog
            .checkpoint_bytes()
            .unwrap_or_else(|e| panic!("{name}: encode {e:?}"));
        let restored = PropertyCatalog::from_checkpoint_bytes(&bytes)
            .unwrap_or_else(|e| panic!("{name}: restore {e:?}"));
        assert_eq!(restored, catalog, "replay {name}");
        count += 1;
    }
    assert_eq!(count, 103);
}

#[test]
fn unsupported_domains_and_capacity_are_explicit_and_do_not_mutate() {
    let mut catalog = PropertyCatalog::new();
    let mut backend = Tally {
        fail: false,
        calls: Vec::new(),
    };
    let before = catalog.clone();
    assert_eq!(
        catalog.execute(
            "addProperty",
            &[
                V::number(1.0),
                V::String("ABC".into()),
                V::number(1.0),
                V::String("toString".into())
            ],
            &mut backend
        ),
        Err(CatalogError::UnsupportedDomain)
    );
    assert_eq!(catalog, before);
    assert_eq!(
        catalog.execute("quantize8", &[V::String("0xff".into())], &mut backend),
        Err(CatalogError::UnsupportedDomain)
    );
    assert_eq!(catalog, before);
    assert_eq!(
        catalog.execute("getTallyMap", &[], &mut backend),
        Err(CatalogError::UnsupportedDomain)
    );
    assert_eq!(catalog, before);
    assert!(backend.calls.is_empty());
    let oversized = V::Array(vec![V::Null; 1025]);
    assert_eq!(
        catalog.execute("getPropertyData", &[oversized], &mut backend),
        Err(CatalogError::Capacity)
    );
    assert_eq!(catalog, before);
    let bad_document = V::Object(vec![("value".into(), V::String("bad-json".into()))]);
    catalog.replace_document(Some(bad_document)).unwrap();
    let before = catalog.clone();
    assert_eq!(
        catalog.execute("doesTickerExist", &[V::String("ABC".into())], &mut backend),
        Err(CatalogError::UnsupportedDomain)
    );
    assert_eq!(catalog, before);
}

#[test]
fn real_backend_syntax_exception_is_preserved_and_replayed() {
    struct Reject;
    impl SourceBackend for Reject {
        fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
            Ok(DependencyReturn::PendingThrow(SourceException {
                name: "SyntaxError".into(),
                message: "Invalid JSON".into(),
            }))
        }
    }
    let mut catalog = PropertyCatalog::new();
    catalog
        .execute(
            "createToken",
            &[
                V::String("ABC".into()),
                V::number(10.0),
                V::String("Managed".into()),
            ],
            &mut Reject,
        )
        .unwrap();
    let evaluation = catalog
        .execute(
            "grantTokens",
            &[
                V::number(1.0),
                V::String("bob".into()),
                V::number(2.0),
                V::number(50.0),
            ],
            &mut Reject,
        )
        .unwrap();
    assert_eq!(
        evaluation.outcome,
        SourceOutcome::Thrown(SourceException {
            name: "SyntaxError".into(),
            message: "Invalid JSON".into()
        })
    );
    assert_eq!(
        catalog.cache_entries()[0].1.get("totalInCirculation"),
        V::number(12.0)
    );
    assert_eq!(
        catalog
            .get_property_data(V::number(1.0))
            .unwrap()
            .get("totalInCirculation"),
        V::number(10.0)
    );
    assert_eq!(
        PropertyCatalog::from_checkpoint_bytes(&catalog.checkpoint_bytes().unwrap()).unwrap(),
        catalog
    );
}

#[test]
fn checkpoint_corruption_noncanonical_frames_and_forged_replay_fail() {
    let mut catalog = PropertyCatalog::new();
    let mut backend = Tally {
        fail: false,
        calls: Vec::new(),
    };
    catalog
        .execute(
            "createToken",
            &[
                V::String("ABC".into()),
                V::number(1.0),
                V::String("Fixed".into()),
            ],
            &mut backend,
        )
        .unwrap();
    let original = catalog.checkpoint_bytes().unwrap();
    let mut corrupt = original.clone();
    corrupt[7] ^= 1;
    assert!(PropertyCatalog::from_checkpoint_bytes(&corrupt).is_err());
    let mut truncated = original.clone();
    truncated.pop();
    assert!(PropertyCatalog::from_checkpoint_bytes(&truncated).is_err());
    let mut forged = original[..original.len() - 4].to_vec();
    let index = forged.windows(3).position(|w| w == b"ABC").unwrap();
    forged[index] = b'Z';
    let mut crc = !0u32;
    for byte in &forged {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320u32 & (0u32.wrapping_sub(crc & 1)))
        }
    }
    forged.extend_from_slice(&(!crc).to_be_bytes());
    assert!(PropertyCatalog::from_checkpoint_bytes(&forged).is_err());
}

#[test]
fn bounded_complete_history_and_same_owned_singleton() {
    let mut catalog = PropertyCatalog::new();
    let pointer = &catalog as *const PropertyCatalog;
    assert_eq!(catalog.get_instance() as *const PropertyCatalog, pointer);
    let mut backend = Tally {
        fail: false,
        calls: Vec::new(),
    };
    for _ in 0..MAX_HISTORY {
        assert_eq!(
            catalog
                .execute("getInstance", &[], &mut backend)
                .unwrap()
                .outcome,
            SourceOutcome::Returned(V::Undefined)
        );
    }
    assert_eq!(catalog.history_len(), MAX_HISTORY);
    assert_eq!(
        catalog.execute("getInstance", &[], &mut backend),
        Err(CatalogError::Capacity)
    );
    let bytes = catalog.checkpoint_bytes().unwrap();
    assert_eq!(
        PropertyCatalog::from_checkpoint_bytes(&bytes).unwrap(),
        catalog
    );
}
