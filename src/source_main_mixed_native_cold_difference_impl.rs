// Comparison diagnostics only. Bytes are never decoded into execution state.
fn mixed_cold_comparison_difference(expected: &[u8], actual: &[u8]) -> String {
    fn fields(raw: &[u8]) -> Option<&[u8]> {
        if !raw.starts_with(b"TLMAP\x01") {
            return Some(raw);
        }
        let count = usize::try_from(u64::from_be_bytes(raw.get(6..14)?.try_into().ok()?)).ok()?;
        raw.get(14..14usize.checked_add(count)?)
    }
    fn next<'a>(raw: &'a [u8], at: &mut usize) -> Option<(&'a [u8], &'a [u8])> {
        let length = usize::try_from(u32::from_be_bytes(
            raw.get(*at..at.checked_add(4)?)?.try_into().ok()?,
        ))
        .ok()?;
        *at = at.checked_add(4)?;
        let path = raw.get(*at..at.checked_add(length)?)?;
        *at = at.checked_add(length)?;
        let length = usize::try_from(u32::from_be_bytes(
            raw.get(*at..at.checked_add(4)?)?.try_into().ok()?,
        ))
        .ok()?;
        *at = at.checked_add(4)?;
        let body = raw.get(*at..at.checked_add(length)?)?;
        *at = at.checked_add(length)?;
        Some((path, body))
    }
    fn show(raw: &[u8]) -> String {
        String::from_utf8_lossy(raw).chars().take(240).collect()
    }
    let (Some(left), Some(right)) = (fields(expected), fields(actual)) else {
        return "malformed Accepted field envelope".into();
    };
    let (mut a, mut b, mut ordinal) = (0, 0, 0);
    while a < left.len() || b < right.len() {
        let (left_at, right_at) = (a, b);
        let x = next(left, &mut a);
        let y = next(right, &mut b);
        match(x,y) {
            (Some(x),Some(y)) if x==y=>ordinal+=1,
            (Some((p,v)),Some((q,w)))=>return format!("field {ordinal}: expected {}={} actual {}={}",show(p),show(v),show(q),show(w)),
            (None,Some((p,v))) if left_at==left.len()=>return format!("field {ordinal}: expected END actual {}={}",show(p),show(v)),
            (Some((p,v)),None) if right_at==right.len()=>return format!("field {ordinal}: expected {}={} actual END",show(p),show(v)),
            _=>return format!("malformed scalar frame {ordinal} at expected byte {left_at}, actual byte {right_at}"),
        }
    }
    let byte = expected
        .iter()
        .zip(actual)
        .position(|(a, b)| a != b)
        .unwrap_or(expected.len().min(actual.len()));
    format!("equal {ordinal} scalar fields; envelope/trailer differs at byte {byte} (expected {} bytes, actual {} bytes)",expected.len(),actual.len())
}
