//! Complete bounded BitVM cache metadata lifecycle; no balances or proof authority.
use crate::bitvm_bundle::*;
use crate::issuance_intent::{JsonObject, JsonValue};
use std::collections::BTreeMap;
pub const SOURCE_SHA256: &str = "053d2c2518485a0dcc0f42a76b100a945e0c568f54bcfb04a5b8fa97b80ba0ca";
const MAGIC: &[u8; 5] = b"TLBC\x01";
pub const CACHE_STATUS: [&str; 4] = ["PENDING", "CHALLENGED", "RELEASED", "RESOLVED_UPHELD"];
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitvmCacheRegistry {
    environment: Environment,
    records: BTreeMap<String, JsonObject>,
    journal: Vec<MetadataEvent>,
}
impl BitvmCacheRegistry {
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
    pub fn journal(&self) -> &[MetadataEvent] {
        &self.journal
    }
    pub fn get(&self, cache_id: &str) -> Option<&JsonObject> {
        if cache_id.len() > 1024 {
            return None;
        }
        self.records.get(&format!("bitvm-cache-{cache_id}"))
    }
    pub fn challenge_blocks(&self, raw: Option<&JsonValue>) -> Result<f64> {
        if let Some(value) = raw {
            value.to_json().map_err(|_| BitvmError::Capacity)?;
        }
        let n = number(raw)?;
        if n.is_finite() && n >= 0.0 {
            return Ok(n.floor());
        }
        let raw = self
            .environment
            .get("TL_BITVM_CHALLENGE_BLOCKS")
            .filter(|v| !v.is_empty())
            .map_or("6", String::as_str);
        let fallback = number(Some(&text(raw)))?;
        Ok(if fallback.is_nan() {
            f64::NAN
        } else {
            fallback.max(0.0)
        })
    }
    pub fn build_cache_id(parts: &JsonObject) -> Result<String> {
        preflight(parts)?;
        if let Some(value) = parts.get("cacheId").filter(|v| truthy(v)) {
            let value = js_string(value)?;
            if value.len() > 1024 {
                return Err(BitvmError::Capacity);
            }
            return Ok(value);
        }
        let mut raw = JsonObject::new();
        for key in ["dlcRef", "stateHash", "bundleHash"] {
            put(&mut raw, key, or(parts.get(key), &text("")).clone())?;
        }
        for key in ["propertyId", "amount"] {
            let n = number_field(parts, key)?;
            put(
                &mut raw,
                key,
                if n.is_finite() {
                    num(n)?
                } else {
                    JsonValue::Null
                },
            )?;
        }
        for key in ["fromAddress", "toAddress", "cacheAddress"] {
            put(&mut raw, key, or(parts.get(key), &text("")).clone())?;
        }
        Ok(hash_hex(
            raw.to_json().map_err(|_| BitvmError::Capacity)?.as_bytes(),
        ))
    }
    pub fn open(
        &mut self,
        settlement: &JsonObject,
        context: &JsonObject,
        at_ms: u64,
    ) -> Result<JsonObject> {
        preflight(settlement)?;
        preflight(context)?;
        millis(at_ms)?;
        let mut pending_event = event(
            "open",
            "",
            settlement,
            context,
            at_ms,
            (&self.records, &self.journal),
        )?;
        let pid = number(settlement.get("propertyId"))?;
        let amount = number_field(settlement, "amount")?;
        if !pid.is_finite() || pid <= 0.0 {
            return Err(source("Invalid bitvm cache propertyId"));
        }
        if !amount.is_finite() || amount <= 0.0 {
            return Err(source("Invalid bitvm cache amount"));
        }
        let sender = or(context.get("senderAddress"), &text("")).clone();
        let from = or(
            settlement.get("fromAddress"),
            or(settlement.get("holderAddress"), &sender),
        )
        .clone();
        let to = or(
            settlement.get("toAddress"),
            or(settlement.get("recipientAddress"), &sender),
        )
        .clone();
        let dlc = or(context.get("dlcRef"), &text("")).clone();
        let state = or(context.get("stateHash"), &text("")).clone();
        let default_cache = format!(
            "BITVM_CACHE::{}",
            js_string(or(Some(&dlc), or(Some(&state), &text("default"))))?
        );
        let cache = or(settlement.get("cacheAddress"), &text(&default_cache)).clone();
        let blocks = self.challenge_blocks(settlement.get("challengeBlocks"))?;
        let bond = number_field(settlement, "cacheBondAmount")?;
        let bond_pid = number(Some(or(settlement.get("cacheBondPropertyId"), &num(pid)?)))?;
        if !bond.is_finite() || bond < 0.0 {
            return Err(source("Invalid bitvm cacheBondAmount"));
        }
        if bond > 0.0 && (!bond_pid.is_finite() || bond_pid <= 0.0) {
            return Err(source("Invalid bitvm cacheBondPropertyId"));
        }
        let bundle = or(settlement.get("bundleHash"), &text("")).clone();
        let parts = make_object([
            ("cacheId", or(settlement.get("cacheId"), &text("")).clone()),
            ("dlcRef", dlc.clone()),
            ("stateHash", state.clone()),
            ("bundleHash", bundle.clone()),
            ("propertyId", num(pid)?),
            ("amount", num(amount)?),
            ("fromAddress", from.clone()),
            ("toAddress", to.clone()),
            ("cacheAddress", cache.clone()),
        ])?;
        let id = Self::build_cache_id(&parts)?;
        let key = format!("bitvm-cache-{id}");
        if self
            .records
            .get(&key)
            .is_some_and(|d| d.get("status") != Some(&text("RELEASED")))
        {
            return Err(source(&format!(
                "BitVM cache already pending/challenged: {id}"
            )));
        }
        let created = number_field(context, "block")?;
        let bonds = make_object([
            (
                "cacheBond",
                JsonValue::Object(make_object([
                    ("amount", num(bond)?),
                    ("propertyId", num(bond_pid)?),
                    ("payerAddress", sender.clone()),
                    (
                        "vaultAddress",
                        text(&if bond > 0.0 {
                            format!("BITVM_BOND_CACHE::{id}")
                        } else {
                            String::new()
                        }),
                    ),
                ])?),
            ),
            (
                "challengeBond",
                JsonValue::Object(make_object([
                    ("amount", num(0.0)?),
                    ("propertyId", num(pid)?),
                    ("payerAddress", text("")),
                    ("vaultAddress", text(&format!("BITVM_BOND_CHALLENGE::{id}"))),
                ])?),
            ),
        ])?;
        let doc = make_object([
            ("_id", text(&key)),
            ("type", text("bitvmCache")),
            ("cacheId", text(&id)),
            ("status", text("PENDING")),
            ("createdBlock", num(created)?),
            ("challengeDeadlineBlock", num(created + blocks)?),
            ("challengeBlocks", num(blocks)?),
            ("dlcRef", dlc),
            ("stateHash", state),
            ("bundleHash", bundle),
            ("fromAddress", from),
            ("toAddress", to),
            ("cacheAddress", cache),
            ("openerAddress", sender),
            ("propertyId", num(pid)?),
            ("amount", num(amount)?),
            ("cacheBondAmount", num(bond)?),
            ("cacheBondPropertyId", num(bond_pid)?),
            ("challenged", JsonValue::Array(vec![])),
            ("releasedAtBlock", JsonValue::Null),
            ("bonds", JsonValue::Object(bonds)),
            ("createdAt", millis(at_ms)?),
        ])?;
        pending_event.key = id;
        self.commit(pending_event, &key, doc)
    }
    fn load(&self, id: &str) -> Result<JsonObject> {
        if id.len() > 1024 {
            return Err(BitvmError::Capacity);
        }
        self.get(id)
            .cloned()
            .ok_or_else(|| source(&format!("Unknown BitVM cacheId: {id}")))
    }
    fn commit(&mut self, event: MetadataEvent, key: &str, doc: JsonObject) -> Result<JsonObject> {
        let mut merged = self.records.get(key).cloned().unwrap_or_default();
        for (k, v) in doc.iter() {
            put(&mut merged, k, v.clone())?;
        }
        budget(
            &self.records,
            &self.journal,
            &[(key.into(), merged.clone())],
            &event,
        )?;
        self.records.insert(key.into(), merged);
        self.journal.push(event);
        Ok(doc)
    }
    pub fn challenge(&mut self, id: &str, detail: &JsonObject, at_ms: u64) -> Result<JsonObject> {
        let e = event(
            "challenge",
            id,
            detail,
            &JsonObject::new(),
            at_ms,
            (&self.records, &self.journal),
        )?;
        let mut doc = self.load(id)?;
        if doc.get("status") == Some(&text("RELEASED")) {
            return Err(source(&format!("BitVM cache already released: {id}")));
        }
        let mut challenged = match doc.get("challenged") {
            Some(JsonValue::Array(a)) => a.clone(),
            _ => vec![],
        };
        challenged.push(JsonValue::Object(make_object([
            (
                "challengerAddress",
                or(detail.get("challengerAddress"), &text("")).clone(),
            ),
            (
                "evidenceHash",
                or(detail.get("evidenceHash"), &text("")).clone(),
            ),
            ("reason", or(detail.get("reason"), &text("")).clone()),
            ("block", num(number_field(detail, "block")?)?),
            ("ts", millis(at_ms)?),
        ])?));
        put(&mut doc, "status", text("CHALLENGED"))?;
        put(&mut doc, "challenged", JsonValue::Array(challenged))?;
        put(
            &mut doc,
            "lastChallengeBlock",
            num(number_field(detail, "block")?)?,
        )?;
        let amount = number_field(detail, "challengeBondAmount")?;
        let pid = number(Some(or(
            detail.get("challengeBondPropertyId"),
            or(doc.get("propertyId"), &num(0.0)?),
        )))?;
        if !amount.is_finite() || amount < 0.0 {
            return Err(source("Invalid bitvm challengeBondAmount"));
        }
        if amount > 0.0 && (!pid.is_finite() || pid <= 0.0) {
            return Err(source("Invalid bitvm challengeBondPropertyId"));
        }
        let mut bonds = object(doc.get("bonds")).cloned().unwrap_or_default();
        let mut bond = match object(bonds.get("challengeBond")) {
            Some(bond) => bond.clone(),
            None => make_object([
                ("amount", num(0.0)?),
                ("propertyId", num(pid)?),
                ("payerAddress", text("")),
                ("vaultAddress", text(&format!("BITVM_BOND_CHALLENGE::{id}"))),
            ])?,
        };
        if amount > 0.0 && number_field(&bond, "amount")? == 0.0 {
            bond = make_object([
                ("amount", num(amount)?),
                ("propertyId", num(pid)?),
                (
                    "payerAddress",
                    or(detail.get("challengerAddress"), &text("")).clone(),
                ),
                ("vaultAddress", text(&format!("BITVM_BOND_CHALLENGE::{id}"))),
            ])?;
        }
        put(&mut bonds, "challengeBond", JsonValue::Object(bond))?;
        put(&mut doc, "bonds", JsonValue::Object(bonds))?;
        let key = format!("bitvm-cache-{id}");
        self.commit(e, &key, doc)
    }
    pub fn finalize(&mut self, id: &str, detail: &JsonObject, at_ms: u64) -> Result<JsonObject> {
        let e = event(
            "finalize",
            id,
            detail,
            &JsonObject::new(),
            at_ms,
            (&self.records, &self.journal),
        )?;
        let mut doc = self.load(id)?;
        match string_field(&doc, "status")?.as_str() {
            "CHALLENGED" => {
                return Err(source(&format!(
                    "BitVM cache challenged; payout blocked for {id}"
                )))
            }
            "RESOLVED_UPHELD" => {
                return Err(source(&format!(
                    "BitVM cache challenge upheld; payout blocked for {id}"
                )))
            }
            "RELEASED" => return Err(source(&format!("BitVM cache already released: {id}"))),
            _ => (),
        }
        let block = number_field(detail, "block")?;
        let deadline = number_field(&doc, "challengeDeadlineBlock")?;
        if !block.is_finite() || block < deadline {
            return Err(source(&format!(
                "BitVM challenge window still open for {id} (deadline {}, block {})",
                number_text(deadline)?,
                number_text(block)?
            )));
        }
        let to = or(
            detail.get("toAddress"),
            doc.get("toAddress").expect("record"),
        );
        let supplied_object = detail
            .get("toAddress")
            .is_some_and(|v| matches!(v, JsonValue::Array(_) | JsonValue::Object(_)));
        if supplied_object || to != doc.get("toAddress").expect("record") {
            return Err(source(&format!("BitVM payout recipient mismatch for {id}")));
        }
        let amount = number(Some(or(
            detail.get("amount"),
            or(doc.get("amount"), &num(0.0)?),
        )))?;
        if amount != number(doc.get("amount"))? {
            return Err(source(&format!("BitVM payout amount mismatch for {id}")));
        }
        let pid = number(Some(or(
            detail.get("propertyId"),
            or(doc.get("propertyId"), &num(0.0)?),
        )))?;
        if pid != number(doc.get("propertyId"))? {
            return Err(source(&format!("BitVM payout property mismatch for {id}")));
        }
        put(&mut doc, "status", text("RELEASED"))?;
        put(&mut doc, "releasedAtBlock", num(block)?)?;
        put(
            &mut doc,
            "releaseTxid",
            or(detail.get("txid"), &text("")).clone(),
        )?;
        put(&mut doc, "updatedAt", millis(at_ms)?)?;
        self.commit(e, &format!("bitvm-cache-{id}"), doc)
    }
    pub fn resolve(&mut self, id: &str, detail: &JsonObject, at_ms: u64) -> Result<JsonObject> {
        let e = event(
            "resolve",
            id,
            detail,
            &JsonObject::new(),
            at_ms,
            (&self.records, &self.journal),
        )?;
        let mut doc = self.load(id)?;
        if doc.get("status") == Some(&text("RELEASED")) {
            return Err(source(&format!("BitVM cache already released: {id}")));
        }
        let verdict = string_field(detail, "verdict")?.to_lowercase();
        if verdict != "uphold" && verdict != "reject" {
            return Err(source("bitvm_resolve requires verdict=uphold|reject"));
        }
        let block = number_field(detail, "block")?;
        put(
            &mut doc,
            "status",
            text(if verdict == "uphold" {
                "RESOLVED_UPHELD"
            } else {
                "PENDING"
            }),
        )?;
        if verdict == "reject" {
            let old = number(Some(or(doc.get("challengeDeadlineBlock"), &num(block)?)))?;
            put(
                &mut doc,
                "challengeDeadlineBlock",
                num(if old.is_nan() || block.is_nan() {
                    f64::NAN
                } else {
                    old.min(block)
                })?,
            )?;
        }
        put(
            &mut doc,
            "resolution",
            JsonValue::Object(make_object([
                ("verdict", text(&verdict)),
                (
                    "resolverAddress",
                    or(detail.get("resolverAddress"), &text("")).clone(),
                ),
                ("reason", or(detail.get("reason"), &text("")).clone()),
                ("block", num(block)?),
                ("ts", millis(at_ms)?),
            ])?),
        )?;
        self.commit(e, &format!("bitvm-cache-{id}"), doc)
    }
    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>> {
        checkpoint(MAGIC, &self.environment, &self.records, &self.journal)
    }
    pub fn from_checkpoint_bytes(bytes: &[u8], expected_env: &Environment) -> Result<Self> {
        let (events, records) = read_checkpoint(bytes, MAGIC, expected_env)?;
        let mut state = Self::new(expected_env.clone())?;
        for e in events {
            match e.operation.as_str() {
                "open" => {
                    state.open(&e.input, &e.context, e.at_ms)?;
                }
                "challenge" => {
                    state.challenge(&e.key, &e.input, e.at_ms)?;
                }
                "finalize" => {
                    state.finalize(&e.key, &e.input, e.at_ms)?;
                }
                "resolve" => {
                    state.resolve(&e.key, &e.input, e.at_ms)?;
                }
                _ => return Err(BitvmError::InvalidCheckpoint),
            }
        }
        if state.records != records || state.checkpoint_bytes()? != bytes {
            return Err(BitvmError::InvalidCheckpoint);
        }
        Ok(state)
    }
}
