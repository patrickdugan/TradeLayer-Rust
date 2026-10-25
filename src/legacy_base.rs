//! Bounded string-domain compatibility with the pinned legacy Base94/Base256 helpers.
//!
//! These are source observations, not transaction admission or consensus codecs.
//! In particular, Base94 uses `|` both as a digit and a fraction separator and
//! loses fractional leading zeroes. No escaping or automatic repair is applied.
//! The original dependency supports arbitrary sized integers; this adapter caps
//! every input at [`MAX_INPUT_CODE_UNITS`] UTF-16 code units before conversion.

use core::fmt;

/// Local resource restriction, not a historical protocol rule.
pub const MAX_INPUT_CODE_UNITS: usize = 4_096;

/// Pinned JavaScript source revision; source blobs and dependency are archived.
pub const SOURCE_REVISION: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyBaseError {
    InputTooLong,
    InvalidDecimalDigit(char),
    InvalidAlphabetCharacter(char),
    InvalidBase94Character(char),
    UnrepresentableUtf16Error,
    InvalidBigInt(String),
    NullBigInt,
}

impl fmt::Display for LegacyBaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLong => write!(f, "local input limit exceeded"),
            Self::InvalidDecimalDigit(c) => write!(f, "Digit is invalid: {c}"),
            Self::InvalidAlphabetCharacter(c) => write!(f, "Digit is not in alphabet: {c}"),
            Self::InvalidBase94Character(c) => {
                write!(f, "Invalid Base94 character detected: {c}")
            }
            Self::UnrepresentableUtf16Error => {
                write!(f, "source error contains an unpaired UTF-16 surrogate")
            }
            Self::InvalidBigInt(s) => write!(f, "Cannot convert {s} to a BigInt"),
            Self::NullBigInt => write!(f, "Cannot convert null to a BigInt"),
        }
    }
}

impl std::error::Error for LegacyBaseError {}

type Result<T> = core::result::Result<T, LegacyBaseError>;

fn bounded(input: &str) -> Result<()> {
    if input.encode_utf16().count() > MAX_INPUT_CODE_UNITS {
        Err(LegacyBaseError::InputTooLong)
    } else {
        Ok(())
    }
}

// ECMAScript WhiteSpace + LineTerminator, deliberately not Rust's broader set.
fn js_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

/// Exactly the 256 BMP characters selected by the legacy JavaScript regex.
/// This alphabet contains `,`, `;`, `|`, `-` and other framing punctuation.
pub fn custom_base256_alphabet() -> String {
    (33..=323)
        .filter_map(char::from_u32)
        .filter(|c| !matches!(c, '\u{007f}'..='\u{00a0}' | '\u{00ad}') && !js_space(*c))
        .take(256)
        .collect()
}

fn alphabet(base: u32) -> Vec<char> {
    if base == 94 {
        (33..=126).filter_map(char::from_u32).collect()
    } else {
        custom_base256_alphabet().chars().collect()
    }
}

// A nonnegative little-endian integer. At most 4096 base-256 input digits
// produce 4096 limbs. All intermediate arithmetic fits comfortably in u32.
#[derive(Default)]
struct Natural(Vec<u8>);

impl Natural {
    fn multiply_add(&mut self, radix: u32, digit: u32) {
        let mut carry = digit;
        for limb in &mut self.0 {
            carry += u32::from(*limb) * radix;
            *limb = carry as u8;
            carry >>= 8;
        }
        while carry > 0 {
            self.0.push(carry as u8);
            carry >>= 8;
        }
    }

    fn render(mut self, chars: &[char]) -> String {
        if self.0.is_empty() {
            return chars[0].to_string();
        }
        let radix = chars.len() as u32;
        let mut out = Vec::new();
        while !self.0.is_empty() {
            let mut carry = 0;
            for limb in self.0.iter_mut().rev() {
                let value = carry * 256 + u32::from(*limb);
                *limb = (value / radix) as u8;
                carry = value % radix;
            }
            out.push(chars[carry as usize]);
            while self.0.last() == Some(&0) {
                self.0.pop();
            }
        }
        out.iter().rev().collect()
    }
}

const DECIMAL: [char; 10] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
const HEX: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

fn decimal_natural(input: &str) -> Result<Natural> {
    // bigint-base-converter parses in reverse order, including its errors.
    for c in input.chars().rev() {
        if !c.is_ascii_digit() {
            if c.len_utf16() != 1 {
                return Err(LegacyBaseError::UnrepresentableUtf16Error);
            }
            return Err(LegacyBaseError::InvalidDecimalDigit(c));
        }
    }
    let mut n = Natural::default();
    for c in input.bytes() {
        n.multiply_add(10, u32::from(c - b'0'));
    }
    Ok(n)
}

fn alphabet_natural(input: &str, chars: &[char]) -> Result<Natural> {
    for c in input.chars().rev() {
        if !chars.contains(&c) {
            if c.len_utf16() != 1 {
                return Err(LegacyBaseError::UnrepresentableUtf16Error);
            }
            return Err(LegacyBaseError::InvalidAlphabetCharacter(c));
        }
    }
    let mut n = Natural::default();
    for c in input.chars() {
        let digit = chars.iter().position(|a| *a == c).expect("validated");
        n.multiply_add(chars.len() as u32, digit as u32);
    }
    Ok(n)
}

// JavaScript BigInt(string) syntax, restricted to Rust's valid Unicode strings.
fn bigint_string(input: &str) -> Result<(bool, Natural)> {
    let invalid = || LegacyBaseError::InvalidBigInt(input.to_owned());
    let trimmed = input.trim_matches(js_space);
    if trimmed.is_empty() {
        return Ok((false, Natural::default()));
    }
    let (negative, unsigned, signed) = match trimmed.as_bytes()[0] {
        b'-' => (true, &trimmed[1..], true),
        b'+' => (false, &trimmed[1..], true),
        _ => (false, trimmed, false),
    };
    let (radix, digits) = if unsigned.starts_with("0x") || unsigned.starts_with("0X") {
        (16, &unsigned[2..])
    } else if unsigned.starts_with("0o") || unsigned.starts_with("0O") {
        (8, &unsigned[2..])
    } else if unsigned.starts_with("0b") || unsigned.starts_with("0B") {
        (2, &unsigned[2..])
    } else {
        (10, unsigned)
    };
    if digits.is_empty() || (signed && radix != 10) {
        return Err(invalid());
    }
    let mut n = Natural::default();
    for c in digits.chars() {
        if !c.is_ascii() {
            return Err(invalid());
        }
        let digit = c.to_digit(radix).ok_or_else(invalid)?;
        n.multiply_add(radix, digit);
    }
    Ok((negative && !n.0.is_empty(), n))
}

fn render_hex(input: &str, even: bool) -> Result<String> {
    let (negative, n) = bigint_string(input)?;
    let hex = n.render(&HEX);
    let mut result = if negative { format!("-{hex}") } else { hex };
    // Preserve the source bug: padding includes the minus sign in its length.
    if even && !result.len().is_multiple_of(2) {
        result.insert(0, '0');
    }
    Ok(result)
}

pub fn base94_from_decimal(input: &str) -> Result<String> {
    bounded(input)?;
    Ok(decimal_natural(input)?.render(&alphabet(94)))
}

/// Source `decimalToBase94`; `None` models either null or undefined.
/// Only the first two dot-separated components are read. Fractional zeroes
/// are discarded by integer conversion, so `1.001` decodes as `1.1`.
pub fn base94_encode_decimal(input: Option<&str>) -> Result<Option<String>> {
    let Some(input) = input else { return Ok(None) };
    bounded(input)?;
    let mut parts = input.split('.');
    let integer = parts.next().expect("split has one element");
    let encoded = base94_from_decimal(if integer.is_empty() { "0" } else { integer })?;
    let fraction = parts.next().unwrap_or("");
    if fraction.is_empty() {
        Ok(Some(encoded))
    } else {
        Ok(Some(format!(
            "{encoded}|{}",
            base94_from_decimal(fraction)?
        )))
    }
}

/// Source `fromBase94`; `|` is interpreted as a separator even when it was
/// emitted as the legal digit 91. Additional components are ignored.
pub fn base94_to_decimal(input: Option<&str>) -> Result<Option<String>> {
    let Some(input) = input else { return Ok(None) };
    bounded(input)?;
    if input.is_empty() {
        return Ok(None);
    }
    let chars = alphabet(94);
    let mut parts = input.split('|');
    let integer = parts.next().expect("split has one element");
    // Literal '0' is digit 15 in this alphabet: this source fallback is not !.
    let integer = if integer.is_empty() { "0" } else { integer };
    let decoded = alphabet_natural(integer, &chars)?.render(&DECIMAL);
    let fraction = parts.next().unwrap_or("");
    if fraction.is_empty() {
        Ok(Some(decoded))
    } else {
        Ok(Some(format!(
            "{decoded}.{}",
            alphabet_natural(fraction, &chars)?.render(&DECIMAL)
        )))
    }
}

pub fn base94_validate_input(input: &str) -> Result<()> {
    bounded(input)?;
    for c in input.chars() {
        if !('!'..='~').contains(&c) {
            return Err(LegacyBaseError::InvalidBase94Character(c));
        }
    }
    Ok(())
}

pub fn base94_hex_to_decimal(input: &str) -> Result<String> {
    bounded(input)?;
    Ok(bigint_string(&format!("0x{input}"))?.1.render(&DECIMAL))
}

pub fn base94_decimal_to_hex(input: &str) -> Result<String> {
    bounded(input)?;
    render_hex(input, false)
}

pub fn base94_hex_to_base94(input: &str) -> Result<String> {
    bounded(input)?;
    // Do not reapply the external input limit to an expanded internal decimal.
    let decimal = base94_hex_to_decimal(input)?;
    Ok(decimal_natural(&decimal)?.render(&alphabet(94)))
}

pub fn base94_to_hex(input: Option<&str>) -> Result<String> {
    let decoded = base94_to_decimal(input)?.ok_or(LegacyBaseError::NullBigInt)?;
    render_hex(&decoded, false)
}

fn clean_decimal(input: &str) -> String {
    input
        .chars()
        .filter(|c| *c != ',' && !js_space(*c))
        .collect()
}

pub fn base256_from_decimal(input: &str) -> Result<String> {
    bounded(input)?;
    Ok(decimal_natural(&clean_decimal(input))?.render(&alphabet(256)))
}

pub fn base256_to_decimal(input: &str) -> Result<String> {
    bounded(input)?;
    Ok(alphabet_natural(input, &alphabet(256))?.render(&DECIMAL))
}

pub fn base256_hex_to_decimal(input: Option<&str>) -> Result<String> {
    let Some(input) = input else {
        return Ok("0".to_owned());
    };
    bounded(input)?;
    if input.trim_matches(js_space).is_empty() {
        return Ok("0".to_owned());
    }
    let clean: String = input.chars().filter(|c| !js_space(*c)).collect();
    let padded = if !clean.encode_utf16().count().is_multiple_of(2) {
        format!("0{clean}")
    } else {
        clean
    };
    Ok(bigint_string(&format!("0x{padded}"))?.1.render(&DECIMAL))
}

pub fn base256_decimal_to_hex(input: &str) -> Result<String> {
    bounded(input)?;
    render_hex(&clean_decimal(input), true)
}

pub fn base256_hex_to_base256(input: Option<&str>) -> Result<String> {
    let Some(input) = input else {
        return Ok(String::new());
    };
    bounded(input)?;
    if input.trim_matches(js_space).is_empty() {
        return Ok(String::new());
    }
    let decimal = base256_hex_to_decimal(Some(input))?;
    Ok(decimal_natural(&decimal)?.render(&alphabet(256)))
}

pub fn base256_to_hex(input: Option<&str>) -> Result<String> {
    let Some(input) = input else {
        return Ok(String::new());
    };
    bounded(input)?;
    if input.is_empty() {
        return Ok(String::new());
    }
    // Render directly so expanding 4096 base256 digits to decimal cannot hit
    // the public input cap halfway through a valid bounded conversion.
    let mut hex = alphabet_natural(input, &alphabet(256))?.render(&HEX);
    if !hex.len().is_multiple_of(2) {
        hex.insert(0, '0');
    }
    Ok(hex)
}
