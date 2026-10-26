//! Source-shaped balance-query payloads over an immutable sole Ledger.
//! JSON, Base64 and fingerprints establish neither balances nor authority.
use crate::decimal::{Decimal, DecimalError};
use crate::hash256::sha256;
use crate::issuance_intent::JsonValue;
use crate::ledger::{AccountId, Ledger, PropertyId};
use core::cmp::Ordering;

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "4a8a06227544ffa12c43c5fe5f987dc02b1ae2c1c0f27aa70de3645fdcc57988";
pub const SCHEMA: &str = "tl-state-oracle-balance-v1";
pub const MAX_ADDRESSES: usize = 4096;
pub const MAX_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_BASE64_BYTES: usize = 3 * 1024 * 1024;
pub const DEFAULT_BUCKET_SIZE: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateOracleError {
    NonFinite,
    Capacity,
    Arithmetic,
    InvalidBase64,
    InvalidJson,
    NonCanonical,
}
impl core::fmt::Display for StateOracleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for StateOracleError {}
impl From<DecimalError> for StateOracleError {
    fn from(_: DecimalError) -> Self {
        Self::Arithmetic
    }
}
type Result<T> = core::result::Result<T, StateOracleError>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bucket {
    pub bucket_min: f64,
    pub bucket_max: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BalanceRow {
    pub address: AccountId,
    pub available: f64,
    pub bucket_min: f64,
    pub bucket_max: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BalancePayload {
    property_id: u32,
    bucket_size: f64,
    rows: Vec<BalanceRow>,
}

/// Binary64 source floor, including negative dust. Nonfinite inputs/results
/// reject instead of producing the source's NaN/Infinity JSON null fields.
pub fn quantize8(value: f64) -> Result<f64> {
    if !value.is_finite() {
        return Err(StateOracleError::NonFinite);
    }
    // The source's value || 0 converts negative zero to positive zero.
    if value == 0.0 {
        return Ok(0.0);
    }
    let result = (value * 1e8).floor() / 1e8;
    if !result.is_finite() {
        return Err(StateOracleError::NonFinite);
    }
    Ok(result)
}
pub fn bucketize(balance: f64, bucket_size: f64) -> Result<Bucket> {
    let balance = quantize8(balance)?;
    let step = quantize8(bucket_size)?.max(0.00000001);
    let min = (balance / step).floor() * step;
    Ok(Bucket {
        bucket_min: quantize8(min)?,
        bucket_max: quantize8(min + step)?,
    })
}

// en-US default ICU primary order on the complete printable AccountId ASCII
// domain. Case is tertiary: compare the entire folded string before case.
// This is an explicit fixed policy; JS's ambient locale may differ on a host.
const PRIMARY: &[u8] = b"_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789abcdefghijklmnopqrstuvwxyz";
pub fn compare_addresses(first: &AccountId, second: &AccountId) -> Ordering {
    let rank = |byte: u8| {
        PRIMARY
            .iter()
            .position(|v| *v == byte.to_ascii_lowercase())
            .expect("AccountId printable ASCII")
    };
    let primary = first
        .as_str()
        .bytes()
        .map(rank)
        .cmp(second.as_str().bytes().map(rank));
    if primary != Ordering::Equal {
        return primary;
    }
    first
        .as_str()
        .bytes()
        .map(|v| v.is_ascii_uppercase())
        .cmp(second.as_str().bytes().map(|v| v.is_ascii_uppercase()))
}
pub fn build_address_balance_payload(
    ledger: &Ledger,
    property: PropertyId,
    addresses: &[AccountId],
    bucket_size: f64,
) -> Result<BalancePayload> {
    build_address_balance_payload_with(property.get(), addresses, bucket_size, |address, _| {
        let atoms = ledger.available_atoms(property, address);
        // Number(decimal token scalar), rather than an extra rounded atoms-f64
        // division. Unknown properties and absent rows both return source zero.
        let number = format!("{}.{:08}", atoms / 100_000_000, atoms % 100_000_000)
            .parse::<f64>()
            .map_err(|_| StateOracleError::Arithmetic)?;
        Ok(Some(number))
    })
}
pub fn build_address_balance_payload_with<F>(
    property_id: u32,
    addresses: &[AccountId],
    bucket_size: f64,
    mut query: F,
) -> Result<BalancePayload>
where
    F: FnMut(&AccountId, u32) -> Result<Option<f64>>,
{
    if addresses.len() > MAX_ADDRESSES {
        return Err(StateOracleError::Capacity);
    }
    let stored_bucket_size = quantize8(bucket_size)?;
    let mut rows = Vec::with_capacity(addresses.len());
    for address in addresses {
        let available = quantize8(query(address, property_id)?.unwrap_or(0.0))?;
        let bucket = bucketize(available, bucket_size)?;
        rows.push(BalanceRow {
            address: address.clone(),
            available,
            bucket_min: bucket.bucket_min,
            bucket_max: bucket.bucket_max,
        });
    }
    rows.sort_by(|a, b| compare_addresses(&a.address, &b.address));
    let payload = BalancePayload {
        property_id,
        bucket_size: stored_bucket_size,
        rows,
    };
    payload.canonical_json()?;
    Ok(payload)
}

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
fn escaped_account(account: &AccountId) -> String {
    let mut out = String::from("\"");
    for byte in account.as_str().bytes() {
        if matches!(byte, b'"' | b'\\') {
            out.push('\\');
        }
        out.push(char::from(byte));
    }
    out.push('"');
    out
}
impl BalancePayload {
    pub fn property_id(&self) -> u32 {
        self.property_id
    }
    pub fn bucket_size(&self) -> f64 {
        self.bucket_size
    }
    pub fn rows(&self) -> &[BalanceRow] {
        &self.rows
    }
    pub fn canonical_json(&self) -> Result<String> {
        if self.rows.len() > MAX_ADDRESSES {
            return Err(StateOracleError::Capacity);
        }
        let mut out = format!(
            "{{\"schema\":\"{SCHEMA}\",\"propertyId\":{},\"bucketSize\":{},\"rows\":[",
            self.property_id,
            source_number_text(self.bucket_size)?
        );
        for (index, row) in self.rows.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"address\":{},\"available\":{},\"bucketMin\":{},\"bucketMax\":{}}}",
                escaped_account(&row.address),
                source_number_text(row.available)?,
                source_number_text(row.bucket_min)?,
                source_number_text(row.bucket_max)?
            ));
            if out.len() > MAX_PAYLOAD_BYTES {
                return Err(StateOracleError::Capacity);
            }
        }
        out.push_str("]}");
        if out.len() > MAX_PAYLOAD_BYTES {
            return Err(StateOracleError::Capacity);
        }
        Ok(out)
    }
    pub fn encode_base64(&self) -> Result<String> {
        encode_bytes(self.canonical_json()?.as_bytes())
    }
    pub fn sha256_hex(&self) -> Result<String> {
        Ok(hash_hex(self.canonical_json()?.as_bytes()))
    }
}
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
fn encode_bytes(bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(StateOracleError::Capacity);
    }
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for block in bytes.chunks(3) {
        let a = block[0];
        let b = block.get(1).copied().unwrap_or(0);
        let c = block.get(2).copied().unwrap_or(0);
        out.push(char::from(ALPHABET[(a >> 2) as usize]));
        out.push(char::from(ALPHABET[((a & 3) << 4 | b >> 4) as usize]));
        out.push(if block.len() > 1 {
            char::from(ALPHABET[((b & 15) << 2 | c >> 6) as usize])
        } else {
            '='
        });
        out.push(if block.len() > 2 {
            char::from(ALPHABET[(c & 63) as usize])
        } else {
            '='
        });
    }
    Ok(out)
}
fn source_decode(value: &str) -> Result<Vec<u8>> {
    if value.len() > MAX_BASE64_BYTES {
        return Err(StateOracleError::Capacity);
    }
    let mut out = Vec::new();
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    // Buffer.from uses each UTF-16 code unit's low byte, even for non-ASCII.
    for unit in value.encode_utf16() {
        let byte = unit as u8;
        if byte == b'=' {
            break;
        }
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        accumulator = (accumulator << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
            if out.len() > MAX_PAYLOAD_BYTES {
                return Err(StateOracleError::Capacity);
            }
        }
    }
    Ok(out)
}
fn hash_hex(bytes: &[u8]) -> String {
    sha256(bytes).iter().map(|b| format!("{b:02x}")).collect()
}
/// Source Buffer.from(...,'base64') compatibility. This is intentionally
/// separate from strict canonical payload decoding.
pub fn payload_hash_from_b64(value: &str) -> Result<String> {
    Ok(hash_hex(&source_decode(value)?))
}
/// General bounded JSON-safe source encodeBalancePayload(payload || {}).
/// Generic JSON limits belong to JsonValue; fixed balance rows use their own cap.
pub fn encode_balance_payload(payload: Option<&JsonValue>) -> Result<String> {
    let falsy = match payload {
        None | Some(JsonValue::Null) | Some(JsonValue::Bool(false)) => true,
        Some(JsonValue::Number(n)) => n.as_f64() == 0.0,
        Some(JsonValue::String(s)) => s.is_empty(),
        _ => false,
    };
    let json = if falsy {
        "{}".into()
    } else {
        payload
            .expect("nonfalsy exists")
            .to_json()
            .map_err(|_| StateOracleError::InvalidJson)?
    };
    encode_bytes(json.as_bytes())
}

struct FixedReader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl FixedReader<'_> {
    fn literal(&mut self, value: &[u8]) -> Result<()> {
        if self.bytes.get(self.at..self.at + value.len()) != Some(value) {
            return Err(StateOracleError::InvalidJson);
        }
        self.at += value.len();
        Ok(())
    }
    fn number(&mut self) -> Result<f64> {
        let start = self.at;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| !matches!(b, b',' | b'}'))
        {
            self.at += 1;
        }
        let text = core::str::from_utf8(&self.bytes[start..self.at])
            .map_err(|_| StateOracleError::InvalidJson)?;
        let number = text
            .parse::<f64>()
            .map_err(|_| StateOracleError::InvalidJson)?;
        if !number.is_finite() {
            return Err(StateOracleError::NonFinite);
        }
        Ok(number)
    }
    fn account(&mut self) -> Result<AccountId> {
        self.literal(b"\"")?;
        let mut account = String::new();
        loop {
            let byte = *self
                .bytes
                .get(self.at)
                .ok_or(StateOracleError::InvalidJson)?;
            self.at += 1;
            match byte {
                b'"' => break,
                b'\\' => {
                    let escaped = *self
                        .bytes
                        .get(self.at)
                        .ok_or(StateOracleError::InvalidJson)?;
                    self.at += 1;
                    if !matches!(escaped, b'"' | b'\\') {
                        return Err(StateOracleError::NonCanonical);
                    }
                    account.push(char::from(escaped));
                }
                0x21..=0x7e => account.push(char::from(byte)),
                _ => return Err(StateOracleError::InvalidJson),
            }
            if account.len() > crate::ledger::MAX_ACCOUNT_BYTES {
                return Err(StateOracleError::Capacity);
            }
        }
        account
            .as_str()
            .try_into()
            .map_err(|_| StateOracleError::InvalidJson)
    }
}
/// Strict fixed-schema parser: exact key order, finite numbers, supported ASCII
/// addresses, stable sorted rows and canonical JSON/Base64. It checks structure,
/// not the truth of balances or bucket provenance against a ledger/request.
pub fn decode_balance_payload(value: &str) -> Result<BalancePayload> {
    let bytes = source_decode(value)?;
    if encode_bytes(&bytes)? != value {
        return Err(StateOracleError::InvalidBase64);
    }
    let mut reader = FixedReader {
        bytes: &bytes,
        at: 0,
    };
    reader.literal(format!("{{\"schema\":\"{SCHEMA}\",\"propertyId\":").as_bytes())?;
    let property_start = reader.at;
    while reader.bytes.get(reader.at).is_some_and(u8::is_ascii_digit) {
        reader.at += 1;
    }
    let property_id = core::str::from_utf8(&reader.bytes[property_start..reader.at])
        .map_err(|_| StateOracleError::InvalidJson)?
        .parse::<u32>()
        .map_err(|_| StateOracleError::InvalidJson)?;
    reader.literal(b",\"bucketSize\":")?;
    let bucket_size = reader.number()?;
    reader.literal(b",\"rows\":[")?;
    let mut rows = Vec::new();
    if reader.bytes.get(reader.at) != Some(&b']') {
        loop {
            if rows.len() >= MAX_ADDRESSES {
                return Err(StateOracleError::Capacity);
            }
            reader.literal(b"{\"address\":")?;
            let address = reader.account()?;
            reader.literal(b",\"available\":")?;
            let available = reader.number()?;
            reader.literal(b",\"bucketMin\":")?;
            let bucket_min = reader.number()?;
            reader.literal(b",\"bucketMax\":")?;
            let bucket_max = reader.number()?;
            reader.literal(b"}")?;
            if rows.last().is_some_and(|last: &BalanceRow| {
                compare_addresses(&last.address, &address) == Ordering::Greater
            }) {
                return Err(StateOracleError::NonCanonical);
            }
            rows.push(BalanceRow {
                address,
                available,
                bucket_min,
                bucket_max,
            });
            if reader.bytes.get(reader.at) != Some(&b',') {
                break;
            }
            reader.at += 1;
        }
    }
    reader.literal(b"]}")?;
    if reader.at != bytes.len() {
        return Err(StateOracleError::InvalidJson);
    }
    let payload = BalancePayload {
        property_id,
        bucket_size,
        rows,
    };
    if payload.canonical_json()?.as_bytes() != bytes {
        return Err(StateOracleError::NonCanonical);
    }
    Ok(payload)
}
