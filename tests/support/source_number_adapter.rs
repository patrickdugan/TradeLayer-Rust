use tradelayer_core::decimal::Decimal;
use tradelayer_core::state_oracle::StateOracleError;
type Result<T> = core::result::Result<T, StateOracleError>;
fn power(base: u32, mut exponent: u32) -> Result<Decimal> {
    let mut value = Decimal::from(1u32);
    let mut factor = Decimal::from(base);
    while exponent != 0 {
        if exponent & 1 != 0 {
            value = value.checked_mul(&factor)?;
        }
        exponent >>= 1;
        if exponent != 0 {
            factor = factor.checked_mul(&factor)?;
        }
    }
    Ok(value)
}
fn exact_float(value: f64) -> Result<Decimal> {
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (mantissa, shift) = if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1u64 << 52), exponent - 1023 - 52)
    };
    let coefficient = Decimal::from(mantissa)
        .checked_mul(&power(if shift < 0 { 5 } else { 2 }, shift.unsigned_abs())?)?;
    if shift < 0 {
        format!("{coefficient}e{shift}").parse().map_err(Into::into)
    } else {
        Ok(coefficient)
    }
}
fn render_decimal(value: &Decimal) -> String {
    let full = value.to_string();
    let exponent = value.coefficient_digits() as i32 - value.decimal_places() as i32 - 1;
    let mut digits: String = full
        .bytes()
        .filter(|b| *b != b'.')
        .map(char::from)
        .collect();
    let leading = digits.bytes().take_while(|b| *b == b'0').count();
    digits.drain(..leading);
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }
    if !(-6..21).contains(&exponent) {
        let mut text = digits[..1].to_owned();
        if digits.len() > 1 {
            text.push('.');
            text.push_str(&digits[1..]);
        }
        text.push('e');
        if exponent >= 0 {
            text.push('+');
        }
        text.push_str(&exponent.to_string());
        text
    } else {
        let point = exponent + 1;
        if point <= 0 {
            format!("0.{}{digits}", "0".repeat((-point) as usize))
        } else if point as usize >= digits.len() {
            format!("{digits}{}", "0".repeat(point as usize - digits.len()))
        } else {
            format!(
                "{}.{}",
                &digits[..point as usize],
                &digits[point as usize..]
            )
        }
    }
}
/// ECMAScript finite Number rendering, including -0, shortest-decimal ties and
/// the 1e-6/1e21 exponent thresholds. Does not call JSON or the intent module.
pub(crate) fn source_number_text(value: f64) -> Result<String> {
    if !value.is_finite() {
        return Err(StateOracleError::NonFinite);
    }
    if value == 0.0 {
        return Ok("0".into());
    }
    let negative = value.is_sign_negative();
    let magnitude = value.abs();
    let exact = exact_float(magnitude)?;
    let exponent = exact.coefficient_digits() as i32 - exact.decimal_places() as i32 - 1;
    let full = exact.to_string();
    let significant: String = full
        .bytes()
        .filter(|b| *b != b'.')
        .map(char::from)
        .collect::<String>()
        .trim_start_matches('0')
        .to_owned();
    for length in 1..=17usize {
        let used = length.min(significant.len());
        let coefficient = significant[..used]
            .parse::<u64>()
            .map_err(|_| StateOracleError::Arithmetic)?;
        let shift = exponent - used as i32 + 1;
        let mut choices = Vec::new();
        for candidate in [coefficient, coefficient + 1] {
            let text = format!("{candidate}e{shift}");
            if text.parse::<f64>().ok() != Some(magnitude) {
                continue;
            }
            let decimal: Decimal = text.parse()?;
            let distance = exact.checked_sub(&decimal)?.abs();
            choices.push((decimal, distance, candidate.is_multiple_of(2)));
        }
        if !choices.is_empty() {
            choices.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| b.2.cmp(&a.2)));
            let text = render_decimal(&choices[0].0);
            return Ok(if negative { format!("-{text}") } else { text });
        }
    }
    Err(StateOracleError::Arithmetic)
}
