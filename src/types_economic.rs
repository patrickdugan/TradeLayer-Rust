//! Pure, explicitly unverified projection of real Types output into handlers
//! already present in the sole economic owner. This supplies no authority.
use crate::contract_orderbook::{CancelCriteria, Order, Side};
use crate::contract_registry::{ContractAsset, ContractId, ContractParameters};
use crate::decimal::Decimal;
use crate::economic::{CreateContractInput, EconomicInput};
use crate::joint::{JointOperation, UnverifiedJointTx};
use crate::ledger::{AccountId, PropertyId, TxId};
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

/// The consensus stage must compute `evaluation` itself against its actual staged
/// owner. This public diagnostic struct is not an authenticated capability.
/// Original txid/actor/type/height are checked; no caller `valid` argument is read.
pub fn map_economic_input_unverified(
    original_arguments: &[V],
    evaluation: &TypesEvaluation,
    height: u64,
    configured_contract: ContractId,
) -> Result<EconomicInput> {
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
    if !matches!(ty, 0 | 2 | 5 | 6 | 7 | 8 | 9 | 11 | 12 | 13 | 14 | 16 | 18) {
        return Err(MappingError::Unsupported(ty));
    }
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
            let subject = account(&get("targetAddress"))?;
            registry(if boolean(&get("revoke"))? {
                RegistryAction::RevokeAttestation { list_id, subject }
            } else {
                RegistryAction::Attest {
                    list_id,
                    subject,
                    metadata: text(&get("metaData"))?.into(),
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
