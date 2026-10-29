// Included in paged metadata. It plans the original initial Main query/grouping
// only, and supplies no transaction, validity or financial result.
pub const MAIN_STREAM_PROFILE: &str = "TL-MAIN-FIXED-INDEX-GROUPING-1";
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum HeightKey {
    Number(u64),
    Nan,
}
#[derive(Clone, Copy)]
struct RowLocation {
    page: usize,
    slot: usize,
}
#[derive(Default)]
struct RowGroup {
    funding: Vec<RowLocation>,
    trade: Vec<RowLocation>,
}
pub struct MainIndexStreamPlan {
    snapshot: SourceIndexReadSnapshot,
    receipt: RowStreamReceipt,
    groups: std::collections::BTreeMap<HeightKey, RowGroup>,
}
pub enum MainIndexPlanningOutcome {
    Ready(Box<MainIndexStreamPlan>),
    Thrown {
        exception: SourceException,
        receipt: Box<RowStreamReceipt>,
    },
}
pub struct MainIndexPhaseCursor {
    snapshot: SourceIndexReadSnapshot,
    locations: Vec<RowLocation>,
    at: usize,
    page: Option<usize>,
    buffer: Vec<V>,
}
impl MainIndexPhaseCursor {
    pub fn next_row(&mut self) -> Result<Option<V>> {
        let Some(location) = self.locations.get(self.at).copied() else {
            return Ok(None);
        };
        if self.page != Some(location.page) {
            self.buffer = self.snapshot.page(location.page)?;
            self.page = Some(location.page);
        }
        let row = self
            .buffer
            .get(location.slot)
            .ok_or(CompactIndexError::Corrupt)?
            .clone();
        self.at += 1;
        Ok(Some(row))
    }
}
impl MainIndexStreamPlan {
    pub fn receipt(&self) -> &RowStreamReceipt {
        &self.receipt
    }
    pub fn max_index_block(&self) -> V {
        if self.groups.contains_key(&HeightKey::Nan) {
            V::number(f64::NAN)
        } else {
            self.groups
                .keys()
                .filter_map(|h| match h {
                    HeightKey::Number(n) => Some(*n),
                    HeightKey::Nan => None,
                })
                .max()
                .map_or(V::Null, |n| V::number(n as f64))
        }
    }
    pub fn phase(&self, height: u64, funding: bool) -> MainIndexPhaseCursor {
        let locations = self
            .groups
            .get(&HeightKey::Number(height))
            .map(|g| {
                if funding {
                    g.funding.clone()
                } else {
                    g.trade.clone()
                }
            })
            .unwrap_or_default();
        MainIndexPhaseCursor {
            snapshot: self.snapshot.clone(),
            locations,
            at: 0,
            page: None,
            buffer: vec![],
        }
    }
    pub fn heights(&self) -> impl Iterator<Item = u64> + '_ {
        self.groups.keys().filter_map(|key| match key {
            HeightKey::Number(n) => Some(*n),
            HeightKey::Nan => None,
        })
    }
    pub fn require_authenticated_admission(&self) -> std::result::Result<(), SourceRuleError> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }
}
fn js_truth(v: &V) -> bool {
    match v {
        V::Undefined | V::Null | V::Bool(false) => false,
        V::Number(b) => {
            let n = f64::from_bits(*b);
            n != 0.0 && !n.is_nan()
        }
        V::String(t) => !t.is_empty(),
        _ => true,
    }
}
fn source_type_error(message: &str) -> SourceException {
    SourceException {
        name: "TypeError".into(),
        message: message.into(),
    }
}
fn source_text(v: &V) -> std::result::Result<String, SourceException> {
    Ok(match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::String(t) => t.clone(),
        V::Number(b) => {
            let n = f64::from_bits(*b);
            if n.is_nan() {
                "NaN".into()
            } else if n == f64::INFINITY {
                "Infinity".into()
            } else if n == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                crate::issuance_intent::JsonValue::Number(
                    crate::issuance_intent::JsonNumber::from_f64(n)
                        .map_err(|_| source_type_error("unsupported number"))?,
                )
                .to_json()
                .map_err(|_| source_type_error("unsupported number"))?
            }
        }
        V::Array(rows) => rows
            .iter()
            .map(|v| {
                if matches!(v, V::Undefined | V::Null) {
                    Ok(String::new())
                } else {
                    source_text(v)
                }
            })
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(","),
        V::Object(fields) => {
            if fields.iter().any(|(k, _)| k == "toString") {
                return Err(source_type_error(
                    "Cannot convert object to primitive value",
                ));
            }
            "[object Object]".into()
        }
        V::Promise => "[object Promise]".into(),
        V::BigNumber(_) => return Err(source_type_error("unsupported BigNumber object")),
    })
}
fn js_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn funding_base36(text: &str) -> bool {
    let mut t = text.trim_start_matches(js_space);
    if t.starts_with('-') {
        return false;
    }
    if let Some(rest) = t.strip_prefix('+') {
        t = rest;
    }
    let mut n = 0u32;
    let mut found = false;
    for c in t.chars() {
        let Some(d) = c.to_digit(36).filter(|_| c.is_ascii()) else {
            break;
        };
        found = true;
        n = n.saturating_mul(36).saturating_add(d);
        if n > 20 {
            return false;
        }
    }
    found && matches!(n, 4 | 20)
}
fn funding_payload(payload: &V) -> std::result::Result<bool, SourceException> {
    match payload {
        V::String(t) => Ok(t
            .chars()
            .next()
            .is_some_and(|c| matches!(c, '4' | 'k' | 'K'))),
        V::Array(a) => {
            let text = if let Some(first) = a.first() {
                if matches!(first, V::Null | V::Undefined) {
                    String::new()
                } else {
                    source_text(first)?
                }
            } else {
                String::new()
            };
            Ok(funding_base36(&text))
        }
        _ => Err(source_type_error("payload.slice is not a function")),
    }
}
fn source_height(id: &str) -> Result<HeightKey> {
    let mut t = id.split('-').nth(1).unwrap_or("").trim_start_matches(' ');
    if let Some(rest) = t.strip_prefix('+') {
        t = rest;
    }
    let (radix, t) = if t.starts_with("0x") || t.starts_with("0X") {
        (16, &t[2..])
    } else {
        (10, t)
    };
    let mut n = 0u64;
    let mut found = false;
    for c in t.chars() {
        let Some(d) = c.to_digit(radix).filter(|_| c.is_ascii()) else {
            break;
        };
        found = true;
        n = n
            .checked_mul(radix as u64)
            .and_then(|n| n.checked_add(d as u64))
            .ok_or(SourceRuleError::UnsupportedNumericDomain)?;
        if n > 9_007_199_254_740_991 {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
    }
    Ok(if found {
        HeightKey::Number(n)
    } else {
        HeightKey::Nan
    })
}
impl SourceIndexReadSnapshot {
    pub fn plan_main_groups(&self) -> Result<MainIndexPlanningOutcome> {
        let mut cursor = self.cursor();
        let mut groups = std::collections::BTreeMap::<HeightKey, RowGroup>::new();
        let mut first_error = None;
        // Original Main completes the initial query and the whole reduce before
        // processTxSet. A later bad payload must prevent earlier financial effects.
        while let Some(row) = cursor.next_row()? {
            if first_error.is_some() {
                continue;
            }
            let id = row_id(&row)?;
            if !id.starts_with("tx-") {
                continue;
            }
            let height = source_height(&id)?;
            if !groups.contains_key(&height) && groups.len() >= 4096 {
                return Err(SourceRuleError::Capacity.into());
            }
            let group = groups.entry(height).or_default();
            let value = row.get("value");
            let payload = value.get("payload");
            if !js_truth(&value) || !js_truth(&payload) {
                continue;
            }
            let location = RowLocation {
                page: cursor.page - 1,
                slot: cursor.position - 1,
            };
            match funding_payload(&payload) {
                Ok(true) => group.funding.push(location),
                Ok(false) => group.trade.push(location),
                Err(e) => first_error = Some(e),
            }
        }
        let receipt = cursor.receipt();
        if let Some(exception) = first_error {
            return Ok(MainIndexPlanningOutcome::Thrown {
                exception,
                receipt: Box::new(receipt),
            });
        }
        Ok(MainIndexPlanningOutcome::Ready(Box::new(
            MainIndexStreamPlan {
                snapshot: self.clone(),
                receipt,
                groups,
            },
        )))
    }
}
