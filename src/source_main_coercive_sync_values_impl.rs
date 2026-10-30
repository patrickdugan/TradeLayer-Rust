// Primitive-only JavaScript branch semantics for a distinct caller profile.
// No result, runtime, index, financial data or RPC backend is stored here.
pub(crate) const NATIVE_MAIN_COERCIVE_SYNC_PROFILE: &str =
    "TL-MAIN-RETAINED-COERCIVE-SYNC-SCALAR-13-1";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeMainCoerciveSyncDecision {
    ExtractOriginal,
    ExtractGenesis,
    Consensus,
    Return,
}
fn native_main_sync_scalar(value: &V) -> bool {
    matches!(
        value,
        V::Undefined | V::Null | V::Bool(_) | V::Number(_) | V::String(_)
    )
}
fn native_main_sync_admitted_max(value: &V) -> bool {
    native_main_sync_scalar(value) && *value != V::Null
}
fn native_main_sync_scalar_equal<E>(
    left: &V,
    right: &V,
    number: &mut impl FnMut(&V) -> Result<f64, E>,
) -> Result<bool, E> {
    Ok(match (left, right) {
        (V::Undefined | V::Null, V::Undefined | V::Null) => true,
        (V::Undefined | V::Null, _) | (_, V::Undefined | V::Null) => false,
        (V::String(left), V::String(right)) => left == right,
        (V::Bool(left), V::Bool(right)) => left == right,
        _ => number(left)? == number(right)?,
    })
}
fn native_main_sync_scalar_decision<E>(
    max: &V,
    tip: &V,
    mut number: impl FnMut(&V) -> Result<f64, E>,
) -> Result<NativeMainCoerciveSyncDecision, E> {
    // Validation is performed before source entry. These assertions add no
    // conversion or source effects to the original invocation.
    debug_assert!(native_main_sync_admitted_max(max));
    debug_assert!(native_main_sync_scalar(tip));
    let greater = match (tip, max) {
        (V::String(left), V::String(right)) => left.encode_utf16().gt(right.encode_utf16()),
        _ => number(tip)? > number(max)?,
    };
    let strict_zero = matches!(max,V::Number(bits) if f64::from_bits(*bits)==0.0);
    if greater && !strict_zero {
        return Ok(NativeMainCoerciveSyncDecision::ExtractOriginal);
    }
    // The source's fresh {} has default primitive "[object Object]". In this
    // admitted primitive domain max=={} is TRUE for exactly that string; it
    // is not an object-identity comparison and cannot be discarded.
    if native_main_sync_scalar_equal(max, &V::number(0.), &mut number)?
        || matches!(max,V::String(value) if value=="[object Object]")
    {
        return Ok(NativeMainCoerciveSyncDecision::ExtractGenesis);
    }
    Ok(if native_main_sync_scalar_equal(max, tip, &mut number)? {
        NativeMainCoerciveSyncDecision::Consensus
    } else {
        NativeMainCoerciveSyncDecision::Return
    })
}
