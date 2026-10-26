//! Pure, explicitly unverified projection of real Types output into handlers
//! already present in the sole economic owner. This supplies no authority.
use crate::contract_orderbook::{CancelCriteria, Order, Side};
use crate::contract_registry::{ContractAsset, ContractId, ContractParameters};
use crate::decimal::Decimal;
use crate::economic::{CreateContractInput, EconomicInput};
use crate::joint::{JointOperation, UnverifiedJointTx};
use crate::ledger::{AccountId, PropertyId, PropertyKind, TxId};
use crate::registry::{AdminTarget, RegistryAction};
use crate::spot::{CancelSelector, SpotIntent};
use crate::types_source::{TypesEvaluation, TypesOutcome};
use crate::validity_source::{SourceOutcome, SourceValue as V};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingError {
    Unsupported(u8),
    Unrepresentable(&'static str),
    InvalidInput,
    SourceFailure,
    SourceRejected,
    MissingValidation,
    MismatchedEvaluation,
}

#[cfg(test)]
mod source_issuance_mapping_tests {
    use super::*;
    use crate::types_source::{decode_with_validity_unverified, SourceValidityAdapter};
    use crate::validity_source::{
        DependencyCall, DependencyReturn, SourceBackend, SourceException, SourceRuleError,
    };

    // Runs the actual Types decoder and source validator over explicit test
    // dependency reads. These fixed replies supply no real owner authority.
    struct IssuanceReads {
        ticker_exists: bool,
    }
    impl SourceBackend for IssuanceReads {
        fn invoke(
            &mut self,
            call: &DependencyCall,
        ) -> core::result::Result<DependencyReturn, SourceRuleError> {
            Ok(DependencyReturn::Pending(match call.target.as_str() {
                "Activation.isTxTypeActive" => {
                    assert_eq!(call.arguments, vec![V::number(1.0)]);
                    V::Bool(true)
                }
                "Property.doesTickerExist" => V::Bool(self.ticker_exists),
                _ => return Err(SourceRuleError::UnknownDependency),
            }))
        }
    }
    fn evaluated(body: &str) -> (Vec<V>, TypesEvaluation) {
        let args = vec![
            V::String("ab".repeat(32)),
            V::number(1.0),
            V::String("tl".into()),
            V::String(body.into()),
            V::String("alice".into()),
            V::Array(vec![]),
            V::number(0.0),
            V::number(0.0),
            V::number(101.0),
        ];
        let evaluation = decode_with_validity_unverified(
            &args,
            &mut SourceValidityAdapter {
                backend: &mut IssuanceReads {
                    ticker_exists: false,
                },
            },
        )
        .unwrap();
        (args, evaluation)
    }
    fn mapped(args: &[V], evaluation: &TypesEvaluation) -> Result<SourceIssuanceMapping> {
        map_source_issuance_unverified(args, evaluation, 101, PropertyId::try_from(42).unwrap())
    }

    #[test]
    fn allocated_source_issuance_preserves_all_class_precedence_and_original_atoms() {
        for (body, class, kind) in [
            ("a,ABC,,0,backup,0,0", "Fixed", PropertyKind::Fixed),
            ("a,ABC,,1,backup,0,0", "Managed", PropertyKind::Managed),
            ("a,ABC,,0,backup,1,0", "Non-Fungible", PropertyKind::Fixed),
            ("a,ABC,,1,backup,1,0", "Non-Fungible", PropertyKind::Fixed),
            ("a,ABC,,1,backup,1,0,0", "Procedural", PropertyKind::Managed),
            ("a,ABC,,1,backup,0,0,2", "Procedural", PropertyKind::Managed),
            // Logic.tokenIssue ignores coloredCoinHybrid in this source path.
            ("a,ABC,,0,backup,0,1", "Fixed", PropertyKind::Fixed),
        ] {
            let (args, evaluation) = evaluated(body);
            let before = evaluation.clone();
            let mapping = mapped(&args, &evaluation).unwrap();
            assert_eq!(mapping.catalog_type, class);
            let EconomicInput::Joint(tx) = mapping.input else {
                panic!("joint issuance expected")
            };
            assert_eq!(tx.txid.as_bytes(), [0xab; 32]);
            assert_eq!(tx.actor.as_str(), "alice");
            assert_eq!(
                tx.operation,
                JointOperation::Issue {
                    id: PropertyId::try_from(42).unwrap(),
                    kind,
                    initial_atoms: 1_000_000_000
                }
            );
            assert_eq!(evaluation, before);
            assert_eq!(
                map_economic_input_unverified(
                    &args,
                    &evaluation,
                    101,
                    ContractId::try_from(1).unwrap()
                ),
                Err(MappingError::Unsupported(1))
            );
        }
        let (args, evaluation) = evaluated("1,ABC,,0,backup,0,0");
        let second = map_source_issuance_unverified(
            &args,
            &evaluation,
            101,
            PropertyId::try_from(43).unwrap(),
        )
        .unwrap();
        let EconomicInput::Joint(tx) = second.input else {
            panic!("joint issuance expected")
        };
        assert!(
            matches!(tx.operation,JointOperation::Issue{id,initial_atoms:100_000_000,..}if id.get()==43)
        );
    }

    #[test]
    fn allocated_source_issuance_retains_original_binding_and_failure_guards() {
        let (args, evaluation) = evaluated("a,ABC,,1,backup,0,0");
        for (index, value) in [
            (0, V::String("cd".repeat(32))),
            (1, V::number(2.0)),
            (3, V::String("b,ABC,,1,backup,0,0".into())),
            (4, V::String("bob".into())),
            (8, V::number(102.0)),
        ] {
            let mut changed = args.clone();
            changed[index] = value;
            assert_eq!(
                mapped(&changed, &evaluation),
                Err(MappingError::MismatchedEvaluation)
            );
        }
        assert_eq!(
            map_source_issuance_unverified(
                &args,
                &evaluation,
                102,
                PropertyId::try_from(42).unwrap()
            ),
            Err(MappingError::MismatchedEvaluation)
        );
        assert_eq!(
            map_source_issuance_unverified(
                &args,
                &evaluation,
                0,
                PropertyId::try_from(42).unwrap()
            ),
            Err(MappingError::InvalidInput)
        );
        let mut changed = evaluation.clone();
        changed.validity = None;
        assert_eq!(
            mapped(&args, &changed),
            Err(MappingError::MissingValidation)
        );
        let mut changed = evaluation.clone();
        changed.decoder.as_mut().unwrap().method = "decodeSend".into();
        assert_eq!(
            mapped(&args, &changed),
            Err(MappingError::MismatchedEvaluation)
        );
        let mut changed = evaluation.clone();
        changed
            .validity
            .as_mut()
            .unwrap()
            .settlement
            .evaluation
            .deferred_rejections
            .push(SourceException {
                name: "Error".into(),
                message: "deferred source rejection".into(),
            });
        assert_eq!(mapped(&args, &changed), Err(MappingError::SourceFailure));
        let mut changed = evaluation.clone();
        let TypesOutcome::Returned(params) = &mut changed.outcome else {
            unreachable!()
        };
        params.insert("initialAmount", V::number(20.0)).unwrap();
        assert_eq!(
            mapped(&args, &changed),
            Err(MappingError::MismatchedEvaluation)
        );
        for body in [
            "0,ABC,,0,backup,0,0",
            "-1,ABC,,0,backup,0,0",
            "a,ABC,,0,backup,0,0,1",
            "a,sBAD,,0,backup,0,0",
        ] {
            let (args, evaluation) = evaluated(body);
            assert_eq!(
                mapped(&args, &evaluation),
                Err(MappingError::SourceRejected),
                "{body}"
            );
        }
    }

    #[test]
    fn shared_catalog_guard_is_bound_and_type1_atoms_never_round_or_use_caller_ids() {
        let (args, mut evaluation) = evaluated("a,ABC,,1,backup,0,0");
        let (params, txid, actor, ty) = bound_source_params(&args, &evaluation, 101).unwrap();
        assert_eq!(ty, 1);
        assert_eq!(txid.as_bytes(), [0xab; 32]);
        assert_eq!(actor.as_str(), "alice");
        assert_eq!(params.get("initialAmount"), V::number(10.0));
        // Deliberately forged diagnostic output tests representability only; the
        // caller still must obtain a fresh evaluation from its actual owner.
        let set = |evaluation: &mut TypesEvaluation, key: &str, value: V| {
            let TypesOutcome::Returned(params) = &mut evaluation.outcome else {
                unreachable!()
            };
            params.insert(key, value).unwrap();
            evaluation
                .validity
                .as_mut()
                .unwrap()
                .settlement
                .evaluation
                .outcome = SourceOutcome::Returned(params.clone());
        };
        set(&mut evaluation, "initialAmount", V::number(1.000_000_001));
        assert_eq!(
            mapped(&args, &evaluation),
            Err(MappingError::Unrepresentable("exact atom precision"))
        );
        set(
            &mut evaluation,
            "initialAmount",
            V::number(9_007_199_254_740_992.0),
        );
        assert_eq!(
            mapped(&args, &evaluation),
            Err(MappingError::Unrepresentable("finite safe Number"))
        );
        set(
            &mut evaluation,
            "initialAmount",
            V::number(9_007_199_254_740_991.0),
        );
        set(&mut evaluation, "propertyId", V::number(999.0));
        let EconomicInput::Joint(tx) = mapped(&args, &evaluation).unwrap().input else {
            panic!("joint issuance expected")
        };
        assert!(
            matches!(tx.operation,JointOperation::Issue{id,initial_atoms:900_719_925_474_099_100_000_000,..} if id.get()==42)
        );
        set(&mut evaluation, "senderAddress", V::String("bob".into()));
        assert_eq!(
            mapped(&args, &evaluation),
            Err(MappingError::MismatchedEvaluation)
        );
    }
}
impl core::fmt::Display for MappingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MappingError {}
type Result<T> = core::result::Result<T, MappingError>;
fn number(value: &V) -> Result<f64> {
    let V::Number(bits) = value else {
        return Err(MappingError::Unrepresentable("number"));
    };
    let n = f64::from_bits(*bits);
    if !n.is_finite() || n.abs() > 9_007_199_254_740_991.0 {
        return Err(MappingError::Unrepresentable("finite safe Number"));
    }
    Ok(n)
}
fn integer(value: &V) -> Result<u64> {
    let n = number(value)?;
    if n < 0.0 || n.fract() != 0.0 {
        return Err(MappingError::Unrepresentable("nonnegative integer"));
    }
    Ok(n as u64)
}
fn id(value: &V) -> Result<u32> {
    u32::try_from(integer(value)?).map_err(|_| MappingError::Unrepresentable("u32 identifier"))
}
fn boolean(value: &V) -> Result<bool> {
    if let V::Bool(v) = value {
        Ok(*v)
    } else {
        Err(MappingError::Unrepresentable("boolean"))
    }
}
fn source_truthy(value: &V) -> bool {
    match value {
        V::Undefined | V::Null | V::Bool(false) => false,
        V::Number(bits) => {
            let number = f64::from_bits(*bits);
            number != 0.0 && !number.is_nan()
        }
        V::String(value) => !value.is_empty(),
        _ => true,
    }
}
fn text(value: &V) -> Result<&str> {
    if let V::String(v) = value {
        Ok(v)
    } else {
        Err(MappingError::Unrepresentable("string"))
    }
}
fn account(value: &V) -> Result<AccountId> {
    AccountId::try_from(text(value)?).map_err(|_| MappingError::Unrepresentable("account"))
}
fn property(value: &V) -> Result<PropertyId> {
    PropertyId::try_from(id(value)?)
        .map_err(|_| MappingError::Unrepresentable("property identifier"))
}
fn hash(value: &V) -> Result<[u8; 32]> {
    let t = text(value)?;
    if t.len() != 64
        || !t
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(MappingError::Unrepresentable("canonical 32-byte hex"));
    }
    let mut bytes = [0; 32];
    for (i, pair) in t.as_bytes().chunks_exact(2).enumerate() {
        bytes[i] = u8::from_str_radix(
            core::str::from_utf8(pair).map_err(|_| MappingError::InvalidInput)?,
            16,
        )
        .map_err(|_| MappingError::InvalidInput)?;
    }
    Ok(bytes)
}
fn decimal(value: &V) -> Result<Decimal> {
    crate::state_oracle::source_number_text(number(value)?)
        .map_err(|_| MappingError::Unrepresentable("Number decimal text"))?
        .parse()
        .map_err(|_| MappingError::Unrepresentable("decimal"))
}
fn decimal_or_source_text(value: &V) -> Result<Decimal> {
    if let V::String(s) = value {
        if s.len() > 128 {
            return Err(MappingError::Unrepresentable("decimal length"));
        }
        s.parse()
            .map_err(|_| MappingError::Unrepresentable("decimal text"))
    } else {
        decimal(value)
    }
}
fn scaled(value: &V, scale: u32) -> Result<u128> {
    let factor: Decimal = 10_u128.pow(scale).into();
    let atoms = decimal(value)?
        .checked_mul(&factor)
        .map_err(|_| MappingError::Unrepresentable("amount"))?;
    if atoms.is_negative() || atoms.decimal_places() != 0 {
        return Err(MappingError::Unrepresentable("exact atom precision"));
    }
    atoms
        .to_string()
        .parse()
        .map_err(|_| MappingError::Unrepresentable("atom overflow"))
}
fn optional_scaled(value: &V, scale: u32) -> Result<Option<u128>> {
    if matches!(value, V::Undefined | V::Null) {
        Ok(None)
    } else {
        Ok(Some(scaled(value, scale)?))
    }
}
fn no_extra_context(params: &V, names: &[&str]) -> Result<()> {
    for name in names {
        match params.get(name) {
            V::Undefined | V::Null => (),
            V::String(s) if s.is_empty() => (),
            _ => {
                return Err(MappingError::Unrepresentable(
                    "DLC/procedural context requires its own handler",
                ))
            }
        }
    }
    Ok(())
}

fn source_header<'a>(
    original_arguments: &[V],
    evaluation: &'a TypesEvaluation,
    height: u64,
) -> Result<(u8, &'a V)> {
    if original_arguments.len() < 9
        || original_arguments.len() > crate::types_source::MAX_ARGUMENTS
        || height == 0
        || height > 9_007_199_254_740_991
    {
        return Err(MappingError::InvalidInput);
    }
    for value in original_arguments {
        value.validate().map_err(|_| MappingError::InvalidInput)?;
    }
    let ty =
        u8::try_from(integer(&original_arguments[1])?).map_err(|_| MappingError::InvalidInput)?;
    let TypesOutcome::Returned(params) = &evaluation.outcome else {
        return Err(MappingError::SourceFailure);
    };
    params.validate().map_err(|_| MappingError::InvalidInput)?;
    Ok((ty, params))
}

fn source_bindings<'a>(
    original_arguments: &[V],
    evaluation: &TypesEvaluation,
    height: u64,
    ty: u8,
    params: &'a V,
) -> Result<(&'a V, TxId, AccountId, u8)> {
    let validity = evaluation
        .validity
        .as_ref()
        .ok_or(MappingError::MissingValidation)?;
    let SourceOutcome::Returned(validated) = &validity.settlement.evaluation.outcome else {
        return Err(MappingError::SourceFailure);
    };
    if validated != params {
        return Err(MappingError::MismatchedEvaluation);
    }
    if params.get("valid") != V::Bool(true) {
        return Err(MappingError::SourceRejected);
    }
    if !validity
        .settlement
        .evaluation
        .deferred_rejections
        .is_empty()
    {
        return Err(MappingError::SourceFailure);
    }
    let route = crate::legacy_types_audit::source_route(ty).ok_or(MappingError::InvalidInput)?;
    let decoded = evaluation
        .decoder
        .as_ref()
        .ok_or(MappingError::MismatchedEvaluation)?;
    if decoded.method != route.decoder
        || decoded.body != text(&original_arguments[3])?
        || Some(validity.call.method.as_str()) != route.validator
        || validity.call.arguments.first() != Some(&original_arguments[4])
        || validity.call.arguments.get(2) != Some(&original_arguments[0])
        || integer(&original_arguments[8])? != height
        || integer(&params.get("block"))? != height
    {
        return Err(MappingError::MismatchedEvaluation);
    }
    let txid = TxId::from_bytes(hash(&original_arguments[0])?);
    let actor = account(&original_arguments[4])?;
    Ok((params, txid, actor, ty))
}

/// Shared binding checks for source-catalog execution. The caller must compute
/// this evaluation freshly against its actual staged owner; a public diagnostic
/// struct and a successful binding check are not authenticated authority.
pub(crate) fn bound_source_params<'a>(
    original_arguments: &[V],
    evaluation: &'a TypesEvaluation,
    height: u64,
) -> Result<(&'a V, TxId, AccountId, u8)> {
    let (ty, params) = source_header(original_arguments, evaluation, height)?;
    source_bindings(original_arguments, evaluation, height, ty, params)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceIssuanceMapping {
    pub(crate) input: EconomicInput,
    pub(crate) catalog_type: &'static str,
}

/// Project type1 only after the actual PropertyCatalog has allocated its ID.
/// Catalog class precedence is the pinned Logic.tokenIssue rule. NFT custody
/// uses the existing Fixed Ledger and Procedural custody the existing Managed
/// Ledger; this supplies no per-unit NFT or procedural contract semantics.
pub(crate) fn map_source_issuance_unverified(
    original_arguments: &[V],
    evaluation: &TypesEvaluation,
    height: u64,
    allocated_id: PropertyId,
) -> Result<SourceIssuanceMapping> {
    let (params, txid, actor, ty) = bound_source_params(original_arguments, evaluation, height)?;
    if ty != 1 {
        return Err(MappingError::Unsupported(ty));
    }
    if params.get("senderAddress") != original_arguments[4] {
        return Err(MappingError::MismatchedEvaluation);
    }
    let fallback = |primary: &str, legacy: &str| {
        let value = params.get(primary);
        if matches!(value, V::Null | V::Undefined) {
            params.get(legacy)
        } else {
            value
        }
    };
    let managed = source_truthy(&fallback("managed", "isManaged"));
    let nft = source_truthy(&fallback("nft", "isNFT"));
    let procedural = params.get("proceduralType");
    let catalog_type = if managed && !matches!(procedural, V::Null | V::Undefined) {
        "Procedural"
    } else if nft {
        "Non-Fungible"
    } else if managed {
        "Managed"
    } else {
        "Fixed"
    };
    let kind = match catalog_type {
        "Managed" | "Procedural" => PropertyKind::Managed,
        _ => PropertyKind::Fixed,
    };
    Ok(SourceIssuanceMapping {
        input: EconomicInput::Joint(UnverifiedJointTx {
            txid,
            actor,
            operation: JointOperation::Issue {
                id: allocated_id,
                kind,
                initial_atoms: scaled(&params.get("initialAmount"), 8)?,
            },
        }),
        catalog_type,
    })
}

/// The consensus stage must compute `evaluation` itself against its actual staged
/// owner. This public diagnostic struct is not an authenticated capability.
/// Original txid/actor/type/height are checked; no caller `valid` argument is read.
pub fn map_economic_input_unverified(
    original_arguments: &[V],
    evaluation: &TypesEvaluation,
    height: u64,
    configured_contract: ContractId,
) -> Result<EconomicInput> {
    let (ty, params) = source_header(original_arguments, evaluation, height)?;
    if !matches!(ty, 0 | 2 | 5 | 6 | 7 | 8 | 9 | 11 | 12 | 13 | 14 | 16 | 18) {
        return Err(MappingError::Unsupported(ty));
    }
    let (_, txid, actor, _) = source_bindings(original_arguments, evaluation, height, ty, params)?;
    let get = |name| params.get(name);
    let registry = |action| {
        EconomicInput::Joint(UnverifiedJointTx {
            txid,
            actor: actor.clone(),
            operation: JointOperation::Registry(action),
        })
    };
    let joint = |operation| {
        EconomicInput::Joint(UnverifiedJointTx {
            txid,
            actor: actor.clone(),
            operation,
        })
    };
    Ok(match ty {
        0 => {
            let V::Array(types) = get("txTypesToActivate") else {
                return Err(MappingError::Unrepresentable("activation list"));
            };
            let types = types
                .iter()
                .map(|v| {
                    u8::try_from(integer(v)?)
                        .map_err(|_| MappingError::Unrepresentable("activation type"))
                })
                .collect::<Result<Vec<_>>>()?;
            registry(RegistryAction::Activate {
                types,
                code_hash: hash(&get("codeHash"))?,
            })
        }
        2 => {
            if boolean(&get("sendAll"))? {
                return Err(MappingError::Unrepresentable(
                    "sendAll needs explicit owner balance projection",
                ));
            }
            if boolean(&get("isColoredOutput"))? {
                return Err(MappingError::Unrepresentable("colored output"));
            }
            joint(JointOperation::Send {
                id: property(&get("propertyIds"))?,
                recipient: account(&get("address"))?,
                amount_atoms: scaled(&get("amounts"), 8)?,
            })
        }
        5 => joint(JointOperation::SpotPlace(SpotIntent {
            offered: property(&get("propertyIdOffered"))?,
            desired: property(&get("propertyIdDesired"))?,
            offered_atoms: u64::try_from(scaled(&get("amountOffered"), 8)?)
                .map_err(|_| MappingError::Unrepresentable("spot u64 atoms"))?,
            expected_atoms: u64::try_from(scaled(&get("amountExpected"), 8)?)
                .map_err(|_| MappingError::Unrepresentable("spot u64 atoms"))?,
            post: boolean(&get("post"))?,
            stop: boolean(&get("stop"))?,
        })),
        6 => {
            let cancel_all = integer(&get("cancelAll"))?;
            if cancel_all > 1 {
                return Err(MappingError::Unrepresentable("cancelAll zero/one"));
            }
            let criteria = if cancel_all == 1 {
                CancelCriteria::All
            } else {
                match get("cancelParams").get("txid") {
                    V::String(t) => CancelCriteria::Txid(TxId::from_bytes(hash(&V::String(t))?)),
                    _ => {
                        return Err(MappingError::Unrepresentable(
                            "source cancellation price/side requires a specified bridge",
                        ))
                    }
                }
            };
            if boolean(&get("isContract"))? {
                if id(&get("offeredPropertyId"))? != configured_contract.get() {
                    return Err(MappingError::Unrepresentable(
                        "cancellation targets a different owner contract",
                    ));
                }
                EconomicInput::Cancel {
                    txid,
                    owner: actor,
                    criteria,
                }
            } else {
                let selector = match criteria {
                    CancelCriteria::All => CancelSelector::All,
                    CancelCriteria::Txid(id) => CancelSelector::ByOrderTxId(id),
                    _ => unreachable!(),
                };
                joint(JointOperation::SpotCancel {
                    offered: property(&get("offeredPropertyId"))?,
                    desired: property(&get("desiredPropertyId"))?,
                    is_contract: false,
                    selector,
                })
            }
        }
        7 => registry(RegistryAction::CreateWhitelist {
            name: text(&get("name"))?.into(),
            url: text(&get("url"))?.into(),
            description: text(&get("description"))?.into(),
            backup: account(&get("backupAddress"))?,
        }),
        8 => {
            let (list, oracle, token) = (
                boolean(&get("whitelist"))?,
                boolean(&get("oracle"))?,
                boolean(&get("token"))?,
            );
            if u8::from(list) + u8::from(oracle) + u8::from(token) != 1 || token {
                return Err(MappingError::Unrepresentable(
                    "single supported admin target",
                ));
            }
            let target = if list {
                AdminTarget::Whitelist(id(&get("id"))?)
            } else {
                AdminTarget::Oracle(id(&get("id"))?)
            };
            registry(RegistryAction::UpdateAdmin {
                target,
                new_address: account(&get("newAddress"))?,
                update_backup: boolean(&get("updateBackup"))?,
            })
        }
        9 => {
            let list_id = id(&get("id"))?;
            if list_id == 0 {
                return Err(MappingError::Unrepresentable(
                    "list0 country/banlist effects",
                ));
            }
            let subject = account(&get("targetAddress"))?;
            registry(if boolean(&get("revoke"))? {
                if !matches!(get("metaData"), V::String(ref reason) if reason.is_empty()) {
                    return Err(MappingError::Unrepresentable("attestation revoke reason"));
                }
                RegistryAction::RevokeAttestation { list_id, subject }
            } else {
                // Logic stores a root only for issuance, and addAttestation
                // recognizes the exact case-sensitive inline xpub: prefix.
                // RegistryAction represents neither of those extra effects.
                if source_truthy(&get("merkleRoot")) {
                    return Err(MappingError::Unrepresentable("attestation Merkle root"));
                }
                let metadata = text(&get("metaData"))?.to_owned();
                if metadata.starts_with("xpub:") {
                    return Err(MappingError::Unrepresentable("attestation xpub metadata"));
                }
                RegistryAction::Attest {
                    list_id,
                    subject,
                    metadata,
                }
            })
        }
        11 => {
            no_extra_context(
                params,
                &[
                    "dlcHash",
                    "dlcTemplateId",
                    "dlcContractId",
                    "settlementState",
                ],
            )?;
            joint(JointOperation::Grant {
                id: property(&get("propertyId"))?,
                recipient: account(&get("addressToGrantTo"))?,
                amount_atoms: scaled(&get("amountGranted"), 8)?,
            })
        }
        12 => {
            no_extra_context(
                params,
                &["dlcTemplateId", "dlcContractId", "settlementState"],
            )?;
            joint(JointOperation::Redeem {
                id: property(&get("propertyId"))?,
                amount_atoms: scaled(&get("amountDestroyed"), 8)?,
            })
        }
        13 => {
            if !matches!(get("whitelists"), V::Array(ref a) if a.is_empty()) {
                return Err(MappingError::Unrepresentable("oracle whitelist metadata"));
            }
            registry(RegistryAction::CreateOracle {
                ticker: text(&get("ticker"))?.into(),
                url: text(&get("url"))?.into(),
                backup: account(&get("backupAddress"))?,
                lag: id(&get("lag"))?,
            })
        }
        14 => registry(RegistryAction::PublishOracle {
            oracle_id: id(&get("oracleId"))?,
            price_1e4: scaled(&get("price"), 4)?,
            high_1e4: optional_scaled(&get("high"), 4)?,
            low_1e4: optional_scaled(&get("low"), 4)?,
            close_1e4: optional_scaled(&get("close"), 4)?,
        }),
        16 => {
            if boolean(&get("native"))? {
                return Err(MappingError::Unsupported(16));
            }
            let V::Array(pairs) = get("onChainData") else {
                return Err(MappingError::Unrepresentable("contract pairs"));
            };
            let pairs = pairs
                .iter()
                .map(|p| {
                    let V::Array(p) = p else {
                        return Err(MappingError::Unrepresentable("contract pair"));
                    };
                    if p.len() != 2 {
                        return Err(MappingError::Unrepresentable("contract pair size"));
                    }
                    Ok((
                        ContractAsset::from(id(&p[0])?),
                        ContractAsset::from(id(&p[1])?),
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let whitelist = match get("whitelist") {
                V::Undefined | V::Null => None,
                v => Some(id(&v)?),
            };
            EconomicInput::CreateContract(CreateContractInput {
                txid,
                issuer: actor,
                parameters: ContractParameters {
                    native: false,
                    underlying_oracle_id: Some(id(&get("underlyingOracleId"))?),
                    on_chain_data: pairs,
                    notional_property_id: ContractAsset::from(id(&get("notionalPropertyId"))?),
                    notional_value: decimal(&get("notionalValue"))?,
                    collateral_property_id: ContractAsset::from(id(&get("collateralPropertyId"))?),
                    leverage: decimal(&get("leverage"))?,
                    expiry_period: integer(&get("expiryPeriod"))?,
                    series: id(&get("series"))?,
                    inverse: boolean(&get("inverse"))?,
                    fee: Decimal::from(u32::from(boolean(&get("fee"))?)),
                    whitelist,
                    ticker: match get("ticker") {
                        V::Undefined | V::Null => None,
                        v => Some(text(&v)?.into()),
                    },
                },
            })
        }
        18 => {
            if boolean(&get("insurance"))? || boolean(&get("reduce"))? || boolean(&get("stop"))? {
                return Err(MappingError::Unrepresentable(
                    "insurance/reduce/stop order flags",
                ));
            }
            let price = decimal_or_source_text(&get("price"))?;
            let quantity = decimal(&get("amount"))?;
            if price <= Decimal::zero() || quantity <= Decimal::zero() {
                return Err(MappingError::Unrepresentable(
                    "positive limit price/quantity",
                ));
            }
            EconomicInput::Order(Order {
                txid,
                owner: actor,
                contract: ContractId::try_from(id(&get("contractId"))?)
                    .map_err(|_| MappingError::Unrepresentable("contract identifier"))?,
                height,
                side: if boolean(&get("sell"))? {
                    Side::Sell
                } else {
                    Side::Buy
                },
                quantity,
                price,
                post: boolean(&get("post"))?,
                is_market: false,
            })
        }
        _ => return Err(MappingError::Unsupported(ty)),
    })
}
