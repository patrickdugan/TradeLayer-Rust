use tradelayer_core::contract_orderbook::{CancelCriteria, Side};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::economic::EconomicInput;
use tradelayer_core::joint::JointOperation;
use tradelayer_core::types_economic::*;
use tradelayer_core::types_source::*;
use tradelayer_core::validity_source::{
    SourceCall, SourceEvaluation, SourceOutcome, SourceRuleError, SourceValue as V,
};

// Explicit dispatch-only test adapter. Typed mapping does not turn this into
// actual validity or an accepted chain capability.
struct DiagnosticShape;
impl TypesValidity for DiagnosticShape {
    fn validate(&mut self, call: &SourceCall) -> Result<ValiditySettlement, SourceRuleError> {
        let mut args = call.arguments.clone();
        args[1].insert("valid", V::Bool(true))?;
        Ok(ValiditySettlement {
            pending: true,
            evaluation: SourceEvaluation {
                outcome: SourceOutcome::Returned(args[1].clone()),
                arguments_after: args,
                dependencies: vec![],
                deferred_rejections: vec![],
                global_writes: vec![],
            },
        })
    }
}
fn make_args(ty: u8, body: &str) -> Vec<V> {
    vec![
        V::String("ab".repeat(32)),
        V::number(ty as f64),
        V::String("tl".into()),
        V::String(body.into()),
        V::String("alice".into()),
        V::Array(vec![]),
        V::number(0.0),
        V::number(0.0),
        V::number(101.0),
    ]
}
fn evaluated(ty: u8, body: &str) -> (Vec<V>, TypesEvaluation) {
    let args = make_args(ty, body);
    let result = decode_with_validity_unverified(&args, &mut DiagnosticShape).unwrap();
    (args, result)
}
fn c(id: u32) -> ContractId {
    id.try_into().unwrap()
}

#[test]
fn scalar_send_uses_exact_source_number_atom_projection_and_original_identity() {
    let (args, result) = evaluated(2, "0;receiver;7;1~");
    let before = result.diagnostic_json().unwrap();
    let input = map_economic_input_unverified(&args, &result, 101, c(1)).unwrap();
    let EconomicInput::Joint(tx) = input else {
        panic!("joint expected")
    };
    assert_eq!(tx.txid.as_bytes(), [0xab; 32]);
    assert_eq!(tx.actor.as_str(), "alice");
    let JointOperation::Send {
        id,
        recipient,
        amount_atoms,
    } = tx.operation
    else {
        panic!("send expected")
    };
    assert_eq!(id.get(), 7);
    assert_eq!(recipient.as_str(), "receiver");
    assert_eq!(amount_atoms, 1);
    assert_eq!(result.diagnostic_json().unwrap(), before);
    let mut changed = args.clone();
    changed[0] = V::String("cd".repeat(32));
    assert_eq!(
        map_economic_input_unverified(&changed, &result, 101, c(1)),
        Err(MappingError::MismatchedEvaluation)
    );
}

#[test]
fn contract_order_and_creation_preserve_source_fields_without_catalog_fabrication() {
    let (args, result) = evaluated(18, "1,4ldqpds,2,1,0,0,1,0");
    let EconomicInput::Order(order) =
        map_economic_input_unverified(&args, &result, 101, c(1)).unwrap()
    else {
        panic!("order expected")
    };
    assert_eq!(order.price.to_string(), "100");
    assert_eq!(order.quantity.to_string(), "2");
    assert_eq!(order.side, Side::Sell);
    assert!(order.post);
    assert!(!order.is_market);
    assert_eq!(order.height, 101);
    let (args, result) = evaluated(16, "0,1,,8,1,7,10,0,1,0,0");
    let EconomicInput::CreateContract(creation) =
        map_economic_input_unverified(&args, &result, 101, c(1)).unwrap()
    else {
        panic!("creation expected")
    };
    assert_eq!(creation.issuer.as_str(), "alice");
    assert_eq!(creation.parameters.underlying_oracle_id, Some(1));
    assert_eq!(creation.parameters.whitelist, None);
    assert_eq!(creation.parameters.ticker, None);
    assert_eq!(creation.parameters.collateral_property_id.get(), 7);
    assert_eq!(creation.parameters.notional_property_id.get(), 8);
}

#[test]
fn contract_cancellation_binds_actual_owner_contract_before_using_idless_runtime_variant() {
    let (args, result) = evaluated(6, "1,2,1,0,0");
    assert_eq!(
        map_economic_input_unverified(&args, &result, 101, c(1)),
        Err(MappingError::Unrepresentable(
            "cancellation targets a different owner contract"
        ))
    );
    let EconomicInput::Cancel { criteria, .. } =
        map_economic_input_unverified(&args, &result, 101, c(2)).unwrap()
    else {
        panic!("cancel expected")
    };
    assert_eq!(criteria, CancelCriteria::All);
}

#[test]
fn absent_or_rejected_validity_and_unrepresentable_handlers_never_become_typed_success() {
    let (args, mut result) = evaluated(2, "0;receiver;7;1~");
    result.validity = None;
    assert_eq!(
        map_economic_input_unverified(&args, &result, 101, c(1)),
        Err(MappingError::MissingValidation)
    );
    let (args, result) = evaluated(2, "1;receiver");
    assert!(matches!(
        map_economic_input_unverified(&args, &result, 101, c(1)),
        Err(MappingError::Unrepresentable(_))
    ));
    let (args, result) = evaluated(11, "7,1,receiver,hash");
    assert!(matches!(
        map_economic_input_unverified(&args, &result, 101, c(1)),
        Err(MappingError::Unrepresentable(_))
    ));
    let (args, result) = evaluated(13, "INDEX,url,backup,1,1");
    assert!(matches!(
        map_economic_input_unverified(&args, &result, 101, c(1)),
        Err(MappingError::Unrepresentable(_))
    ));
    let args = make_args(32, "payload");
    let result = decode_with_validity_unverified(&args, &mut DiagnosticShape).unwrap();
    assert_eq!(
        map_economic_input_unverified(&args, &result, 101, c(1)),
        Err(MappingError::Unsupported(32))
    );
    assert_eq!(
        result.require_authenticated_admission(),
        Err(TypesError::UnauthenticatedInputAndState)
    );
}
