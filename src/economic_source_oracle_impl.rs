// Included in economic.rs. Actual oracle documents, caches and watermarks are
// independent of the legacy strict Registry; neither owner authenticates L1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarketOracleQuote {
    pub(crate) height: u64,
    pub(crate) price: Decimal,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarketOracleView {
    pub(crate) ticker: String,
    pub(crate) closed: bool,
    pub(crate) latest: Option<MarketOracleQuote>,
}

impl EconomicState {
    /// Read the actual source owner when configured, including its metadata
    /// cache. A missing source ID never falls through to a second Registry.
    /// Source-free typed owners retain their established Registry projection.
    pub(crate) fn market_oracle_view(&self, id: u32) -> Result<Option<MarketOracleView>> {
        let actual = self
            .source_catalog_owners()
            .and_then(|owners| owners.oracles.as_ref())
            .or(self.catalog_inner_oracle.as_ref());
        if let Some(owner) = actual {
            return source_market_oracle_view(owner, id);
        }
        let Some(oracle) = self.joint.registry().oracle(id) else {
            return Ok(None);
        };
        Ok(Some(MarketOracleView {
            ticker: oracle.ticker.clone(),
            closed: oracle.closed,
            latest: oracle
                .latest
                .as_ref()
                .map(|quote| -> Result<_> {
                    Ok(MarketOracleQuote {
                        height: quote.height,
                        price: Decimal::from(quote.price_1e4).checked_div(
                            &Decimal::from(10_000),
                            4,
                            Rounding::Down,
                        )?,
                    })
                })
                .transpose()?,
        }))
    }
}

fn source_market_oracle_view(
    owner: &crate::oracle_source_owner::OracleSourceOwner,
    id: u32,
) -> Result<Option<MarketOracleView>> {
    use crate::oracle_source_owner::{source_greater_unverified, source_number_unverified};
    use crate::validity_source::DependencyReturn;
    let key = SourceValue::number(f64::from(id));
    let mut query = owner.clone();
    let metadata = match query
        .invoke_source("getOracleInfo", std::slice::from_ref(&key))
        .map_err(|_| EconomicError::Invariant)?
    {
        DependencyReturn::Immediate(value) | DependencyReturn::Pending(value) => value,
        _ => return Err(EconomicError::Invariant),
    };
    if matches!(metadata, SourceValue::Null | SourceValue::Undefined) {
        return Ok(None);
    }
    let SourceValue::String(ticker) = metadata.get("ticker") else {
        return Err(EconomicError::Invariant);
    };
    let mut latest: Option<&SourceValue> = None;
    for row in owner
        .rows("oracleData")
        .map_err(|_| EconomicError::Invariant)?
    {
        // Source NeDB equality preserves the numeric/string ID distinction.
        // getOraclePrice deliberately does not filter the record's type.
        if row.get("oracleId") != key || !catalog_truth(&row.get("data")) {
            continue;
        }
        if !source_number_unverified(&row.get("data").get("price"))
            .map_err(|_| EconomicError::Invariant)?
            .is_finite()
        {
            continue;
        }
        let newer = match latest {
            None => true,
            Some(previous) => {
                source_greater_unverified(&row.get("blockHeight"), &previous.get("blockHeight"))
                    .map_err(|_| EconomicError::Invariant)?
            }
        };
        if newer {
            latest = Some(row);
        }
    }
    let latest = latest
        .map(|row| -> Result<_> {
            let height = source_number_unverified(&row.get("blockHeight"))
                .map_err(|_| EconomicError::Invariant)?;
            if !height.is_finite()
                || height < 0.0
                || height.fract() != 0.0
                || height > 9_007_199_254_740_991.0
            {
                return Err(EconomicError::Invariant);
            }
            let price = source_number_unverified(&row.get("data").get("price"))
                .map_err(|_| EconomicError::Invariant)?;
            // Number() is only source's finite-row filter. Its subsequent
            // BigNumber callers retain a string's full decimal precision.
            let text = match row.get("data").get("price") {
                SourceValue::String(value) | SourceValue::BigNumber(value) => {
                    value.trim_matches(catalog_js_space).to_owned()
                }
                SourceValue::Number(_) => crate::state_oracle::source_number_text(price)
                    .map_err(|_| EconomicError::Invariant)?,
                _ => return Err(EconomicError::Invariant),
            };
            Ok(MarketOracleQuote {
                height: height as u64,
                price: text.parse().map_err(|_| EconomicError::Invariant)?,
            })
        })
        .transpose()?;
    Ok(Some(MarketOracleView {
        ticker,
        closed: catalog_truth(&metadata.get("closed")),
        latest,
    }))
}

impl EconomicBlockStage {
    /// A freshly bound actual Types/validity result is required before any
    /// source mutation. Actual source exceptions retain their metadata prefix;
    /// admission and economic failures retain the runner's rollback policy.
    pub(crate) fn try_apply_source_oracle_unverified(
        &mut self,
        arguments: &[SourceValue],
        evaluation: &crate::types_source::TypesEvaluation,
    ) -> Option<core::result::Result<(), SourceCatalogExecutionError>> {
        self.state.source_catalog_owners()?.oracles.as_ref()?;
        let ty = match arguments.get(1) {
            Some(SourceValue::Number(bits)) => f64::from_bits(*bits),
            _ => return None,
        };
        if ![8.0, 13.0, 14.0].contains(&ty) {
            return None;
        }
        if ty == 8.0 {
            let crate::types_source::TypesOutcome::Returned(params) = &evaluation.outcome else {
                return None;
            };
            if !catalog_truth(&params.get("oracle"))
                || catalog_truth(&params.get("whitelist"))
                || catalog_truth(&params.get("token"))
            {
                return None;
            }
        }
        Some((|| {
            let (params, txid, _, ty) =
                crate::types_economic::bound_source_params(arguments, evaluation, self.height)
                    .map_err(|error| SourceCatalogExecutionError::Mapping(format!("{error:?}")))?;
            let mut candidate = self.clone();
            let mut owners = candidate
                .state
                .source_consensus
                .catalogs
                .clone()
                .ok_or(EconomicError::Invariant)?;
            let owner = owners.oracles.as_mut().ok_or(EconomicError::Invariant)?;
            let before = owner.clone();
            let get = |name| params.get(name);
            let (method, args) = match ty {
                13 => (
                    "createOracle",
                    vec![SourceValue::Object(vec![
                        ("adminAddress".into(), get("senderAddress")),
                        ("ticker".into(), get("ticker")),
                        ("url".into(), get("url")),
                        ("backupAddress".into(), get("backupAddress")),
                        // Types decodes whitelists; Logic actually passes clearlists.
                        ("clearlists".into(), get("clearlists")),
                        ("lag".into(), get("lag")),
                    ])],
                ),
                14 => (
                    "publishData",
                    vec![
                        get("oracleId"),
                        get("price"),
                        get("high"),
                        get("low"),
                        get("close"),
                        get("block"),
                    ],
                ),
                8 => (
                    "updateAdmin",
                    vec![get("id"), get("newAddress"), get("updateBackup")],
                ),
                _ => return Err(EconomicError::Invariant.into()),
            };
            match owner
                .invoke_source(method, &args)
                .map_err(SourceCatalogExecutionError::Rule)?
            {
                crate::validity_source::DependencyReturn::Immediate(_)
                | crate::validity_source::DependencyReturn::Pending(_) => (),
                crate::validity_source::DependencyReturn::Throw(error)
                | crate::validity_source::DependencyReturn::PendingThrow(error) => {
                    // Actual JS exceptions can follow observable metadata,
                    // cache or watermark writes. Retain that exact prefix;
                    // no SourceMetadata identity or funded hook was applied.
                    self.state.source_consensus.catalogs = Some(owners);
                    return Err(SourceCatalogExecutionError::Thrown(error));
                }
            }
            // Actual duplicate-height publication is a successful no-op. Its
            // metadata identity is retained, but it cannot replace the first
            // publication's funding or mark identity.
            let published = ty == 14 && *owner != before;
            candidate.state.source_consensus.catalogs = Some(owners);
            candidate.try_apply_unverified(&EconomicInput::SourceMetadata { txid, ty })?;
            if published {
                // Non-market metadata retains source's unrestricted IDs and
                // shapes. Only configured markets request a funded price view.
                let funding = candidate
                    .state
                    .config
                    .funding
                    .as_ref()
                    .map(|config| config.oracle_id);
                let marking = candidate
                    .state
                    .config
                    .marking
                    .as_ref()
                    .map(|config| config.oracle_id);
                let id = funding
                    .into_iter()
                    .chain(marking)
                    .find(|id| get("oracleId") == SourceValue::number(f64::from(*id)));
                if let Some(id) = id {
                    let quote = candidate
                        .state
                        .market_oracle_view(id)?
                        .and_then(|oracle| oracle.latest);
                    if quote.is_some_and(|quote| quote.height == self.height) {
                        if funding == Some(id) {
                            candidate.state.capture_funding_publication(
                                self.height,
                                txid,
                                false,
                            )?;
                        }
                        if marking == Some(id) {
                            candidate.mark_publication = Some(txid);
                        }
                    }
                }
            }
            *self = candidate;
            Ok(())
        })())
    }
}

#[cfg(test)]
mod actual_market_oracle_tests {
    use super::*;
    use crate::oracle_source_owner::OracleSourceOwner;
    fn n(value: f64) -> SourceValue {
        SourceValue::number(value)
    }
    fn s(value: &str) -> SourceValue {
        SourceValue::String(value.into())
    }
    fn obj(fields: &[(&str, SourceValue)]) -> SourceValue {
        SourceValue::Object(
            fields
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone()))
                .collect(),
        )
    }
    fn owner() -> OracleSourceOwner {
        let mut owner = OracleSourceOwner::new();
        assert!(
            matches!(owner.invoke_source("createOracle", &[obj(&[("ticker",s("EXACT")),("adminAddress",s("publisher"))])]).unwrap(), crate::validity_source::DependencyReturn::Pending(n) if n==SourceValue::number(1.0))
        );
        owner
    }
    fn row(id: SourceValue, height: SourceValue, price: SourceValue) -> SourceValue {
        obj(&[
            ("_id", s(&format!("{id:?}/{height:?}"))),
            ("oracleId", id),
            ("blockHeight", height),
            ("data", obj(&[("price", price)])),
            ("type", s("non-oracle-source-price-row")),
        ])
    }
    #[test]
    fn actual_market_filter_preserves_typed_ids_full_decimal_strings_and_no_default_one() {
        let mut owner = owner();
        let empty = source_market_oracle_view(&owner, 1).unwrap().unwrap();
        assert_eq!(empty.latest, None);
        assert_eq!(
            owner.invoke_source("getOraclePrice", &[n(1.0)]).unwrap(),
            crate::validity_source::DependencyReturn::Pending(n(1.0))
        );
        owner
            .import_rows(
                "oracleData",
                vec![
                    row(s("1"), n(999.0), s("999")),
                    row(n(1.0), n(1.0), n(f64::NAN)),
                    row(n(1.0), n(2.0), s(" 1.234567890123456789 ")),
                    row(n(1.0), n(3.0), s("Infinity")),
                ],
            )
            .unwrap();
        let before = owner.encode().unwrap();
        let view = source_market_oracle_view(&owner, 1).unwrap().unwrap();
        assert_eq!(view.ticker, "EXACT");
        assert_eq!(
            view.latest,
            Some(MarketOracleQuote {
                height: 2,
                price: "1.234567890123456789".parse().unwrap()
            })
        );
        assert_eq!(source_market_oracle_view(&owner, 2).unwrap(), None);
        assert_eq!(
            owner.encode().unwrap(),
            before,
            "pure read preserves owner/cache"
        );
    }
    #[test]
    fn latest_market_row_uses_source_relational_height_comparison() {
        let mut owner = owner();
        owner
            .import_rows(
                "oracleData",
                vec![row(n(1.0), s("9"), n(10.0)), row(n(1.0), s("10"), n(20.0))],
            )
            .unwrap();
        assert_eq!(
            source_market_oracle_view(&owner, 1)
                .unwrap()
                .unwrap()
                .latest,
            Some(MarketOracleQuote {
                height: 9,
                price: Decimal::from(10u32)
            })
        );
    }
}
