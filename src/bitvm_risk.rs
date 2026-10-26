//! Source binary64 risk counters with atomic metadata updates, never custody.
use crate::bitvm_bundle::*;
use crate::issuance_intent::{JsonObject, JsonValue};
use std::collections::BTreeMap;
pub const SOURCE_SHA256: &str = "14e6f4edcb6590e043a46188a3ef122adcae88cbf18bc2853d64e59b8791ef66";
pub const REPLAY_POLICY: &str = "SourceRepeatCallsAtomicV1";
const MAGIC: &[u8; 5] = b"TLBR\x01";
pub fn environment_number(environment: &Environment, name: &str, fallback: f64) -> Result<f64> {
    validate_env(environment)?;
    let Some(raw) = environment.get(name).filter(|s| !s.is_empty()) else {
        return Ok(if fallback.is_nan() || fallback == 0.0 {
            0.0
        } else {
            fallback
        });
    };
    let n = number(Some(&text(raw)))?;
    Ok(if n.is_finite() {
        n
    } else if fallback.is_nan() || fallback == 0.0 {
        0.0
    } else {
        fallback
    })
}
pub fn clamp_nonnegative(value: f64) -> f64 {
    if !value.is_finite() || value <= 0.0 {
        0.0
    } else {
        value
    }
}
pub fn escrow_global_key(property_id: f64) -> Result<String> {
    Ok(format!(
        "bitvm-risk-escrow-global-{}",
        number_text(property_id)?
    ))
}
pub fn escrow_dlc_key(property_id: f64, dlc_ref: Option<&JsonValue>) -> Result<String> {
    if let Some(value) = dlc_ref {
        value.to_json().map_err(|_| BitvmError::Capacity)?;
    }
    Ok(format!(
        "bitvm-risk-escrow-dlc-{}-{}",
        number_text(property_id)?,
        js_string(or(dlc_ref, &text("default")))?
    ))
}
pub fn window_key(
    environment: &Environment,
    property_id: f64,
    block: Option<&JsonValue>,
) -> Result<String> {
    if let Some(value) = block {
        value.to_json().map_err(|_| BitvmError::Capacity)?;
    }
    let win = environment_number(environment, "TL_BITVM_SCHED_WINDOW_BLOCKS", 144.0)?
        .floor()
        .max(1.0);
    let block = number(Some(or(block, &num(0.0)?)))?;
    let index = if block.is_nan() {
        f64::NAN
    } else {
        (block.max(0.0) / win).floor()
    };
    Ok(format!(
        "bitvm-risk-window-{}-{}",
        number_text(property_id)?,
        number_text(index)?
    ))
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitvmRiskBook {
    environment: Environment,
    records: BTreeMap<String, JsonObject>,
    journal: Vec<MetadataEvent>,
}
impl BitvmRiskBook {
    pub fn new(environment: Environment) -> Result<Self> {
        validate_env(&environment)?;
        Ok(Self {
            environment,
            records: BTreeMap::new(),
            journal: Vec::new(),
        })
    }
    pub fn records(&self) -> impl Iterator<Item = (&str, &JsonObject)> {
        self.records.iter().map(|(k, v)| (k.as_str(), v))
    }
    pub fn get(&self, key: &str) -> Option<&JsonObject> {
        self.records.get(key)
    }
    pub fn journal(&self) -> &[MetadataEvent] {
        &self.journal
    }
    pub const fn replay_policy(&self) -> &'static str {
        REPLAY_POLICY
    }
    fn cap(&self, name: &str) -> Result<f64> {
        Ok(clamp_nonnegative(environment_number(
            &self.environment,
            name,
            0.0,
        )?))
    }
    fn read_or_init(
        &self,
        key: &str,
        kind: &str,
        pid: f64,
        dlc: Option<&JsonValue>,
    ) -> Result<JsonObject> {
        if let Some(doc) = self.records.get(key) {
            return Ok(doc.clone());
        }
        let mut doc = make_object([
            ("_id", text(key)),
            ("type", text(kind)),
            ("propertyId", num(pid)?),
        ])?;
        if kind == "bitvmRiskWindow" {
            for k in ["depositAmount", "withdrawAmount", "sweepAmount"] {
                put(&mut doc, k, num(0.0)?)?;
            }
        } else {
            if kind == "bitvmRiskEscrowDlc" {
                put(&mut doc, "dlcRef", text(&js_string(or(dlc, &text("")))?))?;
            }
            put(&mut doc, "pendingAmount", num(0.0)?)?;
        }
        Ok(doc)
    }
    fn commit(&mut self, e: MetadataEvent, changes: Vec<(String, JsonObject)>) -> Result<()> {
        budget(&self.records, &self.journal, &changes, &e)?;
        for (k, v) in changes {
            self.records.insert(k, v);
        }
        self.journal.push(e);
        Ok(())
    }
    pub fn on_cache_open(&mut self, input: &JsonObject, at_ms: u64) -> Result<()> {
        self.apply("open", input, at_ms)
    }
    pub fn on_escrow_release(&mut self, input: &JsonObject, at_ms: u64) -> Result<()> {
        self.apply("release", input, at_ms)
    }
    pub fn on_sweep(&mut self, input: &JsonObject, at_ms: u64) -> Result<()> {
        self.apply("sweep", input, at_ms)
    }
    fn apply(&mut self, operation: &str, input: &JsonObject, at_ms: u64) -> Result<()> {
        let e = event(
            operation,
            "",
            input,
            &JsonObject::new(),
            at_ms,
            (&self.records, &self.journal),
        )?;
        let pid = number_field(input, "propertyId")?;
        let amount = clamp_nonnegative(number_field(input, "amount")?);
        if !pid.is_finite() || pid <= 0.0 || amount <= 0.0 {
            return self.commit(e, vec![]);
        }
        let wkey = window_key(&self.environment, pid, input.get("block"))?;
        if operation == "sweep" {
            let cap = self.cap("TL_BITVM_MAX_SWEEP_PER_WINDOW")?;
            if cap <= 0.0 {
                return self.commit(e, vec![]);
            }
            let mut doc = self.read_or_init(&wkey, "bitvmRiskWindow", pid, None)?;
            let next = clamp_nonnegative(number_field(&doc, "sweepAmount")?) + amount;
            if next > cap {
                return Err(source(&format!(
                    "BitVM sweep window cap exceeded: {} > {}",
                    number_text(next)?,
                    number_text(cap)?
                )));
            }
            put(&mut doc, "sweepAmount", num(next)?)?;
            put(&mut doc, "updatedAt", millis(at_ms)?)?;
            return self.commit(e, vec![(wkey, doc)]);
        }
        let gkey = escrow_global_key(pid)?;
        let dkey = escrow_dlc_key(pid, input.get("dlcRef"))?;
        let mut global = self.read_or_init(&gkey, "bitvmRiskEscrowGlobal", pid, None)?;
        let mut dlc = self.read_or_init(&dkey, "bitvmRiskEscrowDlc", pid, input.get("dlcRef"))?;
        let mut window = self.read_or_init(&wkey, "bitvmRiskWindow", pid, None)?;
        let global_pending = clamp_nonnegative(number_field(&global, "pendingAmount")?);
        let dlc_pending = clamp_nonnegative(number_field(&dlc, "pendingAmount")?);
        if operation == "open" {
            let next_global = global_pending + amount;
            let next_dlc = dlc_pending + amount;
            let next_deposit = clamp_nonnegative(number_field(&window, "depositAmount")?) + amount;
            let global_cap = self.cap("TL_BITVM_MAX_PENDING_ESCROW")?;
            let dlc_cap = self.cap("TL_BITVM_MAX_PENDING_ESCROW_PER_DLC")?;
            let deposit_cap = self.cap("TL_BITVM_MAX_DEPOSIT_PER_WINDOW")?;
            for (limit, next, label) in [
                (
                    global_cap,
                    next_global,
                    "BitVM escrow cap exceeded: global pending",
                ),
                (dlc_cap, next_dlc, "BitVM escrow cap exceeded: dlc pending"),
                (
                    deposit_cap,
                    next_deposit,
                    "BitVM deposit window cap exceeded:",
                ),
            ] {
                if limit > 0.0 && next > limit {
                    return Err(source(&format!(
                        "{label} {} > {}",
                        number_text(next)?,
                        number_text(limit)?
                    )));
                }
            }
            put(&mut global, "pendingAmount", num(next_global)?)?;
            put(&mut dlc, "pendingAmount", num(next_dlc)?)?;
            put(&mut window, "depositAmount", num(next_deposit)?)?;
        } else {
            let next = clamp_nonnegative(number_field(&window, "withdrawAmount")?) + amount;
            let cap = self.cap("TL_BITVM_MAX_WITHDRAW_PER_WINDOW")?;
            if cap > 0.0 && next > cap {
                return Err(source(&format!(
                    "BitVM withdraw window cap exceeded: {} > {}",
                    number_text(next)?,
                    number_text(cap)?
                )));
            }
            put(
                &mut global,
                "pendingAmount",
                num((global_pending - amount).max(0.0))?,
            )?;
            put(
                &mut dlc,
                "pendingAmount",
                num((dlc_pending - amount).max(0.0))?,
            )?;
            put(&mut window, "withdrawAmount", num(next)?)?;
        }
        for doc in [&mut global, &mut dlc, &mut window] {
            put(doc, "updatedAt", millis(at_ms)?)?;
        }
        self.commit(e, vec![(gkey, global), (dkey, dlc), (wkey, window)])
    }
    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>> {
        checkpoint(MAGIC, &self.environment, &self.records, &self.journal)
    }
    pub fn from_checkpoint_bytes(bytes: &[u8], expected_env: &Environment) -> Result<Self> {
        let (events, records) = read_checkpoint(bytes, MAGIC, expected_env)?;
        let mut book = Self::new(expected_env.clone())?;
        for e in events {
            if !e.key.is_empty() || e.context.iter().next().is_some() {
                return Err(BitvmError::InvalidCheckpoint);
            }
            match e.operation.as_str() {
                "open" => book.on_cache_open(&e.input, e.at_ms)?,
                "release" => book.on_escrow_release(&e.input, e.at_ms)?,
                "sweep" => book.on_sweep(&e.input, e.at_ms)?,
                _ => return Err(BitvmError::InvalidCheckpoint),
            }
        }
        if book.records != records || book.checkpoint_bytes()? != bytes {
            return Err(BitvmError::InvalidCheckpoint);
        }
        Ok(book)
    }
}
