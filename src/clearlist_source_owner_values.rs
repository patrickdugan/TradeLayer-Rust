//! Bounded JavaScript values, adapter matching and canonical owner encoding.
use super::{ClearlistSourceOwner, MAX_ROWS, MAX_STATE_BYTES};
use crate::codec::{verify_crc, with_crc, Cursor};
use crate::decimal::Decimal;
use crate::validity_source::{SourceException, SourceRuleError, SourceValue as V};

pub(super) type Eval<T> = Result<T, Fault>;
#[derive(Debug)]
pub(super) enum Fault {
    Thrown(SourceException),
    Local(SourceRuleError),
}
impl From<SourceRuleError> for Fault {
    fn from(e: SourceRuleError) -> Self {
        Self::Local(e)
    }
}
pub(super) fn error(message: impl Into<String>) -> Fault {
    Fault::Thrown(SourceException {
        name: "Error".into(),
        message: message.into(),
    })
}
pub(super) fn type_error(message: impl Into<String>) -> Fault {
    Fault::Thrown(SourceException {
        name: "TypeError".into(),
        message: message.into(),
    })
}
pub(super) fn s(text: &str) -> V {
    V::String(text.into())
}
pub(super) fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
pub(super) fn arg(args: &[V], index: usize) -> V {
    args.get(index).cloned().unwrap_or(V::Undefined)
}
pub(super) fn truth(v: &V) -> bool {
    match v {
        V::Undefined | V::Null | V::Bool(false) => false,
        V::Number(b) => {
            let n = f64::from_bits(*b);
            n != 0.0 && !n.is_nan()
        }
        V::String(s) => !s.is_empty(),
        _ => true,
    }
}
pub(super) fn or(values: &[V]) -> V {
    values
        .iter()
        .find(|v| truth(v))
        .cloned()
        .unwrap_or_else(|| values.last().cloned().unwrap_or(V::Undefined))
}
pub(super) fn js_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
pub(super) fn strict(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Number(x), V::Number(y)) => f64::from_bits(*x) == f64::from_bits(*y),
        (V::Undefined, V::Undefined) | (V::Null, V::Null) => true,
        (V::Bool(x), V::Bool(y)) => x == y,
        (V::String(x), V::String(y)) => x == y,
        _ => false,
    }
}
pub(super) fn same_key(a: &V, b: &V) -> bool {
    strict(a, b)
        || matches!((a,b),(V::Number(x),V::Number(y))if f64::from_bits(*x).is_nan()&&f64::from_bits(*y).is_nan())
}
pub(super) fn primitive_key(v: &V) -> Result<(), SourceRuleError> {
    if matches!(v, V::Object(_) | V::Array(_) | V::Promise | V::BigNumber(_)) {
        Err(SourceRuleError::InvalidInput)
    } else {
        Ok(())
    }
}
pub(super) fn map_index(map: &[(V, V)], key: &V) -> Option<usize> {
    map.iter().position(|(k, _)| same_key(k, key))
}
pub(super) fn map_get<'a>(map: &'a [(V, V)], key: &V) -> Option<&'a V> {
    map_index(map, key).map(|i| &map[i].1)
}
pub(super) fn map_set(map: &mut Vec<(V, V)>, key: V, value: V) -> Result<(), SourceRuleError> {
    primitive_key(&key)?;
    let key = if matches!(&key,V::Number(b)if f64::from_bits(*b)==0.0) {
        V::number(0.0)
    } else {
        key
    };
    if let Some(i) = map_index(map, &key) {
        map[i].1 = value;
    } else {
        if map.len() == MAX_ROWS {
            return Err(SourceRuleError::Capacity);
        }
        map.push((key, value));
    }
    Ok(())
}
pub(super) fn text(v: &V) -> Eval<String> {
    Ok(match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::String(s) | V::BigNumber(s) => s.clone(),
        V::Promise => "[object Promise]".into(),
        V::Number(b) => {
            let n = f64::from_bits(*b);
            if n.is_nan() {
                "NaN".into()
            } else if n == f64::INFINITY {
                "Infinity".into()
            } else if n == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                crate::state_oracle::source_number_text(n)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
            }
        }
        V::Array(values) => values
            .iter()
            .map(|v| {
                if matches!(v, V::Null | V::Undefined) {
                    Ok(String::new())
                } else {
                    text(v)
                }
            })
            .collect::<Eval<Vec<_>>>()?
            .join(","),
        V::Object(fields) => {
            if fields.iter().any(|(key, _)| key == "toString") {
                return Err(type_error("Cannot convert object to primitive value"));
            }
            "[object Object]".into()
        }
    })
}
fn radix(text: &str, radix: u32) -> Eval<f64> {
    if text.is_empty() || !text.chars().all(|c| c.is_digit(radix)) {
        return Ok(f64::NAN);
    }
    let text = text.trim_start_matches('0');
    if text.len() > 1024 {
        return Ok(f64::INFINITY);
    }
    let mut number = Decimal::zero();
    for digit in text.chars() {
        number = number
            .checked_mul(&Decimal::from(radix))
            .and_then(|n| n.checked_add(&Decimal::from(digit.to_digit(radix).unwrap_or(0))))
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    }
    Ok(number.to_string().parse().unwrap_or(f64::INFINITY))
}
pub(super) fn number(v: &V) -> Eval<f64> {
    Ok(match v {
        V::Undefined | V::Promise => f64::NAN,
        V::Null => 0.0,
        V::Bool(b) => f64::from(*b),
        V::Number(b) => f64::from_bits(*b),
        _ => {
            let t = text(v)?;
            let t = t.trim_matches(js_space);
            if t.is_empty() {
                0.0
            } else if matches!(t, "Infinity" | "+Infinity") {
                f64::INFINITY
            } else if t == "-Infinity" {
                f64::NEG_INFINITY
            } else if let Some(t) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                radix(t, 16)?
            } else if let Some(t) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
                radix(t, 2)?
            } else if let Some(t) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
                radix(t, 8)?
            } else if t
                .bytes()
                .all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b))
            {
                t.parse().unwrap_or(f64::NAN)
            } else {
                f64::NAN
            }
        }
    })
}
pub(super) fn parse_int(v: &V) -> Eval<f64> {
    let t = text(v)?;
    let t = t.trim_start_matches(js_space);
    let (sign, t) = if let Some(t) = t.strip_prefix('-') {
        (-1.0, t)
    } else {
        (1.0, t.strip_prefix('+').unwrap_or(t))
    };
    let (rad, t) = if let Some(t) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        (16, t)
    } else {
        (10, t)
    };
    let digits: String = t.chars().take_while(|c| c.is_digit(rad)).collect();
    Ok(if digits.is_empty() {
        f64::NAN
    } else if rad == 10 {
        sign * digits.parse::<f64>().unwrap_or(f64::INFINITY)
    } else {
        sign * radix(&digits, rad)?
    })
}
pub(super) fn prop(v: &V, key: &str) -> Eval<V> {
    if matches!(v, V::Undefined | V::Null) {
        return Err(type_error(format!(
            "Cannot read properties of {} (reading '{key}')",
            text(v)?
        )));
    }
    Ok(match v {
        V::Array(a) if key == "length" => V::number(a.len() as f64),
        V::String(s) if key == "length" => V::number(s.encode_utf16().count() as f64),
        V::Array(a) => key
            .parse::<usize>()
            .ok()
            .filter(|i| i.to_string() == key)
            .and_then(|i| a.get(i))
            .cloned()
            .unwrap_or(V::Undefined),
        // A standalone surrogate cannot be represented by SourceValue's UTF-8
        // String, so callers never receive an invented replacement scalar.
        V::String(s) => {
            if let Some(unit) = key
                .parse::<usize>()
                .ok()
                .filter(|i| i.to_string() == key)
                .and_then(|i| s.encode_utf16().nth(i))
            {
                V::String(String::from_utf16(&[unit]).map_err(|_| SourceRuleError::InvalidInput)?)
            } else {
                V::Undefined
            }
        }
        _ => v.get(key),
    })
}
pub(super) fn set_prop(v: &mut V, key: &str, value: V) -> Eval<()> {
    if matches!(v, V::Null | V::Undefined) {
        return Err(type_error(format!(
            "Cannot set properties of {} (setting '{key}')",
            text(v)?
        )));
    }
    if let V::Object(fields) = v {
        if let Some((_, old)) = fields.iter_mut().find(|(k, _)| k == key) {
            *old = value;
        } else {
            fields.push((key.into(), value));
        }
        Ok(())
    } else {
        Err(type_error(format!(
            "Cannot create property '{key}' on {} '{}'",
            match v {
                V::String(_) => "string",
                V::Number(_) => "number",
                V::Bool(_) => "boolean",
                _ => return Err(SourceRuleError::InvalidInput.into()),
            },
            text(v)?
        )))
    }
}
pub(super) fn array(v: &V) -> Eval<&[V]> {
    if let V::Array(a) = v {
        Ok(a)
    } else {
        Err(SourceRuleError::InvalidInput.into())
    }
}
pub(super) fn iterable(v: &V, name: &str) -> Eval<Vec<V>> {
    match v {
        V::Array(a) => Ok(a.clone()),
        V::String(s) => Ok(s.chars().map(|c| V::String(c.to_string())).collect()),
        _ => Err(type_error(format!("{name} is not iterable"))),
    }
}
pub(super) fn includes(v: &V, wanted: &V) -> Eval<bool> {
    Ok(array(v)?.iter().any(|v| same_key(v, wanted)))
}
pub(super) fn received(v: &V) -> String {
    match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => format!("type boolean ({b})"),
        V::Number(_) => format!("type number ({})", text(v).unwrap_or_default()),
        V::Array(_) => "an instance of Array".into(),
        V::Object(_) => "an instance of Object".into(),
        _ => "an instance of Object".into(),
    }
}
fn path(row: &V, key: &str) -> V {
    key.split('.').fold(row.clone(), |row, key| row.get(key))
}
pub(super) fn matches_query(row: &V, query: &V) -> bool {
    let V::Object(fields) = query else {
        return false;
    };
    fields.iter().all(|(key, value)| {
        if key == "$or" {
            return matches!(value,V::Array(queries)if queries.iter().any(|q|matches_query(row,q)));
        }
        let actual = path(row, key);
        if let V::Object(operators) = value {
            operators.iter().all(|(op, v)| match op.as_str() {
                "$exists" => strict(&V::Bool(actual != V::Undefined), v),
                "$ne" => !strict(&actual, v),
                _ => false,
            })
        } else {
            strict(&actual, value)
        }
    })
}
pub(super) fn validate_rows(rows: &[V]) -> Result<(), SourceRuleError> {
    if rows.len() > MAX_ROWS {
        return Err(SourceRuleError::Capacity);
    }
    let mut bytes = 0usize;
    for (i, row) in rows.iter().enumerate() {
        row.validate()?;
        if !matches!(row, V::Object(_)) {
            return Err(SourceRuleError::InvalidInput);
        }
        primitive_key(&row.get("_id"))?;
        if rows[..i]
            .iter()
            .any(|r| same_key(&r.get("_id"), &row.get("_id")))
        {
            return Err(SourceRuleError::InvalidInput);
        }
        bytes = bytes
            .checked_add(row.diagnostic_json()?.len())
            .ok_or(SourceRuleError::Capacity)?;
        if bytes > MAX_STATE_BYTES {
            return Err(SourceRuleError::Capacity);
        }
    }
    Ok(())
}

// V8's short-array path discovers an initial natural run then performs stable
// binary insertion. This also preserves its observable ordering when malformed
// timestamp coercions return NaN, an inconsistent source comparator. For large
// arrays only a consistent numeric comparison has a runtime-independent order.
fn timestamp(row: &V) -> Eval<f64> {
    number(&prop(&prop(row, "data")?, "timestamp")?)
}
fn history_compare(a: &V, b: &V) -> Eval<f64> {
    // JavaScript evaluates b's operand first in `b.timestamp - a.timestamp`.
    Ok(timestamp(b)? - timestamp(a)?)
}
pub(super) fn sort_history(rows: &mut [V]) -> Eval<()> {
    if rows.len() < 2 {
        return Ok(());
    }
    if rows.len() >= 64 {
        let mut numbers = Vec::with_capacity(rows.len());
        for row in rows.iter() {
            numbers.push(timestamp(row)?);
        }
        let first = numbers.iter().copied().find(|n| !n.is_nan());
        if numbers.iter().any(|n| n.is_nan()) {
            if first.is_none() || numbers.iter().all(|n| n.is_nan() || Some(*n) == first) {
                return Ok(());
            }
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        for i in 1..rows.len() {
            let mut j = i;
            while j > 0 && numbers[j] > numbers[j - 1] {
                rows.swap(j, j - 1);
                numbers.swap(j, j - 1);
                j -= 1;
            }
        }
        return Ok(());
    }
    let descending = history_compare(&rows[1], &rows[0])? < 0.0;
    let mut run = 2;
    while run < rows.len() {
        let comparison = history_compare(&rows[run], &rows[run - 1])?;
        if if descending {
            comparison >= 0.0 || comparison.is_nan()
        } else {
            comparison < 0.0
        } {
            break;
        }
        run += 1;
    }
    if descending {
        rows[..run].reverse();
    }
    for start in run..rows.len() {
        let pivot = rows[start].clone();
        let mut low = 0;
        let mut high = start;
        while low < high {
            let middle = low + (high - low) / 2;
            if history_compare(&pivot, &rows[middle])? < 0.0 {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        rows[low..=start].rotate_right(1);
    }
    Ok(())
}

fn put_u32(out: &mut Vec<u8>, v: usize) -> Result<(), SourceRuleError> {
    out.extend_from_slice(
        &u32::try_from(v)
            .map_err(|_| SourceRuleError::Capacity)?
            .to_be_bytes(),
    );
    Ok(())
}
fn put_string(out: &mut Vec<u8>, s: &str) -> Result<(), SourceRuleError> {
    put_u32(out, s.len())?;
    out.extend_from_slice(s.as_bytes());
    Ok(())
}
fn put_value(out: &mut Vec<u8>, v: &V) -> Result<(), SourceRuleError> {
    match v {
        V::Undefined => out.push(0),
        V::Null => out.push(1),
        V::Bool(false) => out.push(2),
        V::Bool(true) => out.push(3),
        V::Number(b) => {
            out.push(4);
            out.extend_from_slice(&b.to_be_bytes());
        }
        V::String(s) => {
            out.push(5);
            put_string(out, s)?;
        }
        V::Array(a) => {
            out.push(6);
            put_u32(out, a.len())?;
            for v in a {
                put_value(out, v)?;
            }
        }
        V::Object(a) => {
            out.push(7);
            put_u32(out, a.len())?;
            for (k, v) in a {
                put_string(out, k)?;
                put_value(out, v)?;
            }
        }
        V::Promise => out.push(8),
        V::BigNumber(s) => {
            out.push(9);
            put_string(out, s)?;
        }
    }
    if out.len() > MAX_STATE_BYTES {
        return Err(SourceRuleError::Capacity);
    }
    Ok(())
}
fn get_u32(c: &mut Cursor<'_>) -> Result<usize, SourceRuleError> {
    c.u32()
        .map(|v| v as usize)
        .map_err(|_| SourceRuleError::InvalidInput)
}
fn get_string(c: &mut Cursor<'_>) -> Result<String, SourceRuleError> {
    let len = get_u32(c)?;
    if len > crate::validity_source::MAX_BYTES {
        return Err(SourceRuleError::Capacity);
    }
    String::from_utf8(
        c.take(len)
            .map_err(|_| SourceRuleError::InvalidInput)?
            .to_vec(),
    )
    .map_err(|_| SourceRuleError::InvalidInput)
}
fn get_value(c: &mut Cursor<'_>, depth: usize) -> Result<V, SourceRuleError> {
    if depth > crate::validity_source::MAX_DEPTH {
        return Err(SourceRuleError::Capacity);
    }
    let v = match c.u8().map_err(|_| SourceRuleError::InvalidInput)? {
        0 => V::Undefined,
        1 => V::Null,
        2 => V::Bool(false),
        3 => V::Bool(true),
        4 => V::Number(c.u64().map_err(|_| SourceRuleError::InvalidInput)?),
        5 => V::String(get_string(c)?),
        6 => {
            let n = get_u32(c)?;
            if n > MAX_ROWS {
                return Err(SourceRuleError::Capacity);
            }
            let mut a = vec![];
            for _ in 0..n {
                a.push(get_value(c, depth + 1)?);
            }
            V::Array(a)
        }
        7 => {
            let n = get_u32(c)?;
            if n > MAX_ROWS {
                return Err(SourceRuleError::Capacity);
            }
            let mut a = vec![];
            for _ in 0..n {
                let k = get_string(c)?;
                let v = get_value(c, depth + 1)?;
                a.push((k, v));
            }
            V::Object(a)
        }
        8 => V::Promise,
        9 => V::BigNumber(get_string(c)?),
        _ => return Err(SourceRuleError::InvalidInput),
    };
    v.validate()?;
    Ok(v)
}
fn put_rows(out: &mut Vec<u8>, rows: &[V]) -> Result<(), SourceRuleError> {
    put_u32(out, rows.len())?;
    for row in rows {
        put_value(out, row)?;
    }
    Ok(())
}
fn get_rows(c: &mut Cursor<'_>) -> Result<Vec<V>, SourceRuleError> {
    let n = get_u32(c)?;
    if n > MAX_ROWS {
        return Err(SourceRuleError::Capacity);
    }
    let mut rows = vec![];
    for _ in 0..n {
        rows.push(get_value(c, 0)?);
    }
    Ok(rows)
}
fn put_map(out: &mut Vec<u8>, map: &[(V, V)]) -> Result<(), SourceRuleError> {
    if map.len() > MAX_ROWS {
        return Err(SourceRuleError::Capacity);
    }
    put_u32(out, map.len())?;
    for (k, v) in map {
        primitive_key(k)?;
        k.validate()?;
        v.validate()?;
        put_value(out, k)?;
        put_value(out, v)?;
    }
    Ok(())
}
fn get_map(c: &mut Cursor<'_>) -> Result<Vec<(V, V)>, SourceRuleError> {
    let n = get_u32(c)?;
    if n > MAX_ROWS {
        return Err(SourceRuleError::Capacity);
    }
    let mut map = vec![];
    for _ in 0..n {
        let k = get_value(c, 0)?;
        let v = get_value(c, 0)?;
        if map_get(&map, &k).is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        map_set(&mut map, k, v)?;
    }
    Ok(map)
}
impl ClearlistSourceOwner {
    pub(super) fn encode_checkpoint(&self, complete: bool) -> Result<Vec<u8>, SourceRuleError> {
        validate_rows(&self.list_rows)?;
        validate_rows(&self.attestation_rows)?;
        let mut bytes = b"TLCO\x01".to_vec();
        bytes.push(u8::from(complete));
        put_rows(&mut bytes, &self.list_rows)?;
        put_rows(&mut bytes, &self.attestation_rows)?;
        if complete {
            self.banlist.validate()?;
            bytes.push(match self.derivation_library {
                crate::bip32_public::SourceLibrary::Litecoin => 0,
                crate::bip32_public::SourceLibrary::BitcoinFallback => 1,
            });
            put_map(&mut bytes, &self.lists)?;
            put_value(&mut bytes, &self.banlist)?;
            put_map(&mut bytes, &self.xpub_cache)?;
            put_map(&mut bytes, &self.roots)?;
        }
        if bytes.len() > MAX_STATE_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        with_crc(bytes).map_err(|_| SourceRuleError::Capacity)
    }
    pub(super) fn decode_checkpoint(bytes: &[u8], complete: bool) -> Result<Self, SourceRuleError> {
        if bytes.len() > MAX_STATE_BYTES + 4 {
            return Err(SourceRuleError::Capacity);
        }
        let raw = verify_crc(bytes).map_err(|_| SourceRuleError::InvalidInput)?;
        let mut cursor = Cursor::new(raw);
        if cursor.take(5).map_err(|_| SourceRuleError::InvalidInput)? != b"TLCO\x01"
            || cursor.u8().map_err(|_| SourceRuleError::InvalidInput)? != u8::from(complete)
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let mut owner = Self::new();
        owner.list_rows = get_rows(&mut cursor)?;
        owner.attestation_rows = get_rows(&mut cursor)?;
        validate_rows(&owner.list_rows)?;
        validate_rows(&owner.attestation_rows)?;
        if complete {
            owner.derivation_library =
                match cursor.u8().map_err(|_| SourceRuleError::InvalidInput)? {
                    0 => crate::bip32_public::SourceLibrary::Litecoin,
                    1 => crate::bip32_public::SourceLibrary::BitcoinFallback,
                    _ => return Err(SourceRuleError::InvalidInput),
                };
            owner.lists = get_map(&mut cursor)?;
            owner.banlist = get_value(&mut cursor, 0)?;
            owner.xpub_cache = get_map(&mut cursor)?;
            owner.roots = get_map(&mut cursor)?;
            for (_, v) in &owner.roots {
                if !matches!(v, V::Array(_)) {
                    return Err(SourceRuleError::InvalidInput);
                }
            }
            for (k, v) in &owner.xpub_cache {
                if !matches!(k, V::String(_))
                    || !matches!(v,V::Array(a)if a.len()==40&&a.iter().all(|v|matches!(v,V::String(_))))
                {
                    return Err(SourceRuleError::InvalidInput);
                }
            }
        }
        cursor.end().map_err(|_| SourceRuleError::InvalidInput)?;
        if owner.encode_checkpoint(complete)? != bytes {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(owner)
    }
}
