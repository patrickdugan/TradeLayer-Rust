// Finite Number source projections and an atomic, replayed metadata owner.
// This owner has no Ledger and cannot turn a source default into price evidence.
use crate::decimal::{Decimal, Rounding};
pub const SOURCE_METADATA_POLICY: &str = "SourceMetadataAtomicV1";
pub const MAX_SOURCE_ACTIONS: usize = 16_384;
pub const MAX_SOURCE_HEIGHT: u64 = 9_007_199_254_740_991;
pub const MAX_SOURCE_TIME: i64 = 8_640_000_000_000_000;
pub const SOURCE_METHODS: [&str; 27] = [
    "saveVolumeDataById",
    "getUTXOEquivalentVolume",
    "updateCumulativeVolumes",
    "getTokenPriceInLTC",
    "calculateVolIndex",
    "_normalizeContractInfo",
    "getContractUnitLTCValue",
    "getContractUnitLTCValueDetails",
    "getLastPrice",
    "getCumulativeVolumes",
    "getBlockVolumes",
    "getVolumeDataById",
    "sampleVolumesByBlock",
    "sampleVolumesByBlockRange",
    "calculateCumulativeVolume",
    "saveCumulativeVolume",
    "auditVWAP",
    "calculateVWAP",
    "getVwapData",
    "saveVWAP",
    "computeLiquidityRewardShare",
    "calculateAttenuatedLiquidityReward",
    "calculateLiquidityReward",
    "getVWAP",
    "baselineLiquidityReward",
    "getTLPriceInLTC",
    "vestTokens",
];
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceVolumeError {
    Capacity,
    InvalidDocument,
    InvalidRange,
    NonFinite,
    Arithmetic,
    InvalidCheckpoint,
    UnsupportedSourcePath(&'static str),
    ZeroContractValue,
}
impl core::fmt::Display for SourceVolumeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SourceVolumeError {}
type SourceResult<T> = core::result::Result<T, SourceVolumeError>;
impl From<crate::decimal::DecimalError> for SourceVolumeError {
    fn from(_: crate::decimal::DecimalError) -> Self {
        Self::Arithmetic
    }
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceVolumeValue {
    pub block_height: Option<u64>,
    pub raw_volume: Option<f64>,
    pub ltc_volume: Option<f64>,
    pub price: Option<f64>,
    pub volume: Option<f64>,
    pub kind: Option<String>,
    pub block: Option<u64>,
    pub global_cumulative_volume: Option<f64>,
    pub ltc_pair_total_volume: Option<f64>,
    pub contract_cumulative_volume: Option<f64>,
    pub volatility: Option<f64>,
    pub updated_at_ms: Option<i64>,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceVolumeDocument {
    pub id: String,
    pub value: SourceVolumeValue,
    pub top_block_height: Option<u64>,
    pub top_volume: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct SourceVolumeSave {
    pub id: String,
    pub raw_volume: f64,
    pub ltc_volume: f64,
    pub price: f64,
    pub block_height: u64,
    pub kind: String,
}
#[derive(Debug, Clone, PartialEq)]
pub enum SourceVolumeAction {
    /// Explicit imported/upserted document, including legacy top-level schema.
    Put(SourceVolumeDocument),
    SaveVolume(SourceVolumeSave),
    UpdateCumulative {
        volume: f64,
        kind: String,
        block: u64,
    },
    /// Useful cursor/await repair; actual source's default Promise.sort fails.
    SaveVolatility {
        lookback: u32,
        now_ms: i64,
    },
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceVolumeEffect {
    pub changed_documents: Vec<String>,
    pub value: Option<f64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourcePriceOrigin {
    Direct,
    Inverse,
    Fallback,
}
#[derive(Debug, Clone, PartialEq)]
pub struct SourcePrice {
    pub price: f64,
    pub origin: SourcePriceOrigin,
    pub block_height: Option<u64>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum SourceValuationVolume {
    Scalar(f64),
    Token([f64; 2]),
}
#[derive(Debug, Clone, PartialEq)]
pub struct SourceValuationRequest {
    pub volume: SourceValuationVolume,
    pub id: String,
    pub kind: String,
    pub collateral_id: Option<u32>,
    pub notional_value: Option<f64>,
    pub inverse: Option<bool>,
    pub trade_price: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct SourceValuation {
    pub value: f64,
    pub prices: Vec<SourcePrice>,
}
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SourceCumulativeVolumes {
    pub global_cumulative_volume: f64,
    pub ltc_pair_total_volume: f64,
    pub contract_cumulative_volume: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct SourceSample {
    pub id: String,
    pub volume: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum SourceBlockVolumeResult {
    EmptyZero,
    Totals { ltc_pairs: f64, global: f64 },
}
#[derive(Debug, Clone, PartialEq)]
pub struct SourceVolumeStore {
    capacity: usize,
    documents: BTreeMap<String, SourceVolumeDocument>,
    order: Vec<String>,
    actions: Vec<SourceVolumeAction>,
}
impl SourceVolumeStore {
    pub fn new(capacity: usize) -> SourceResult<Self> {
        if capacity == 0 || capacity > MAX_VOLUME_RECORDS {
            return Err(SourceVolumeError::Capacity);
        }
        Ok(Self {
            capacity,
            documents: BTreeMap::new(),
            order: vec![],
            actions: vec![],
        })
    }
    pub fn len(&self) -> usize {
        self.documents.len()
    }
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
    pub fn documents(&self) -> impl Iterator<Item = &SourceVolumeDocument> {
        self.order.iter().filter_map(|id| self.documents.get(id))
    }
    pub fn actions(&self) -> &[SourceVolumeAction] {
        &self.actions
    }
    pub fn document(&self, id: &str) -> Option<&SourceVolumeDocument> {
        self.documents.get(id)
    }
    pub fn apply(&mut self, action: SourceVolumeAction) -> SourceResult<SourceVolumeEffect> {
        if self.actions.len() >= MAX_SOURCE_ACTIONS {
            return Err(SourceVolumeError::Capacity);
        }
        validate_action(&action)?;
        let mut next = self.clone();
        let effect = next.apply_inner(&action)?;
        next.actions.push(action);
        *self = next;
        Ok(effect)
    }
    /// One stage for a complete caller batch; no quadratic prefix cloning.
    pub fn apply_batch(
        &mut self,
        actions: &[SourceVolumeAction],
    ) -> SourceResult<Vec<SourceVolumeEffect>> {
        if actions.len() > MAX_SOURCE_ACTIONS - self.actions.len() {
            return Err(SourceVolumeError::Capacity);
        }
        for a in actions {
            validate_action(a)?;
        }
        let mut next = self.clone();
        let mut effects = Vec::with_capacity(actions.len());
        for a in actions {
            effects.push(next.apply_inner(a)?);
            next.actions.push(a.clone());
        }
        *self = next;
        Ok(effects)
    }
    /// Explicit action-prefix repair, not source reorg/height selection.
    pub fn rollback_to_action_count(&mut self, retain: usize) -> SourceResult<usize> {
        if retain > self.actions.len() {
            return Err(SourceVolumeError::InvalidRange);
        }
        let removed = self.actions.len() - retain;
        if removed == 0 {
            return Ok(0);
        }
        let mut next = Self::new(self.capacity)?;
        next.apply_batch(&self.actions[..retain])?;
        *self = next;
        Ok(removed)
    }
    fn put(&mut self, doc: SourceVolumeDocument) -> SourceResult<()> {
        validate_document(&doc)?;
        if !self.documents.contains_key(&doc.id) {
            if self.len() >= self.capacity {
                return Err(SourceVolumeError::Capacity);
            }
            self.order.push(doc.id.clone());
        }
        self.documents.insert(doc.id.clone(), doc);
        Ok(())
    }
    fn update_cumulative(
        &mut self,
        volume: f64,
        kind: &str,
        block: u64,
    ) -> SourceResult<Vec<String>> {
        let global = self
            .document("globalCumulativeVolume")
            .and_then(|d| d.value.global_cumulative_volume)
            .unwrap_or(0.0);
        let ltc = self
            .document("ltcPairCumulativeVolume")
            .and_then(|d| d.value.ltc_pair_total_volume)
            .filter(|n| *n != 0.0)
            .unwrap_or(0.0);
        let new_global = bn_quantized(
            &number_decimal(volume)?.checked_add(&number_decimal(global)?)?,
            8,
            Rounding::HalfUp,
        )?;
        let new_ltc = if kind == "UTXO" {
            bn_quantized(
                &number_decimal(volume)?.checked_add(&number_decimal(ltc)?)?,
                8,
                Rounding::HalfUp,
            )?
        } else {
            ltc
        };
        let mut ltc_doc = SourceVolumeDocument {
            id: "ltcPairCumulativeVolume".into(),
            ..Default::default()
        };
        ltc_doc.value.block = Some(block);
        ltc_doc.value.ltc_pair_total_volume = Some(new_ltc);
        self.put(ltc_doc)?;
        let mut global_doc = SourceVolumeDocument {
            id: "globalCumulativeVolume".into(),
            ..Default::default()
        };
        global_doc.value.block = Some(block);
        global_doc.value.global_cumulative_volume = Some(new_global);
        self.put(global_doc)?;
        Ok(vec![
            "ltcPairCumulativeVolume".into(),
            "globalCumulativeVolume".into(),
        ])
    }
    fn apply_inner(&mut self, action: &SourceVolumeAction) -> SourceResult<SourceVolumeEffect> {
        let mut effect = SourceVolumeEffect::default();
        match action {
            SourceVolumeAction::Put(d) => {
                self.put(d.clone())?;
                effect.changed_documents.push(d.id.clone());
            }
            SourceVolumeAction::SaveVolume(r) => {
                let doc = SourceVolumeDocument {
                    id: r.id.clone(),
                    value: SourceVolumeValue {
                        block_height: Some(r.block_height),
                        raw_volume: Some(r.raw_volume),
                        ltc_volume: Some(r.ltc_volume),
                        price: Some(r.price),
                        kind: Some(r.kind.clone()),
                        ..Default::default()
                    },
                    ..Default::default()
                };
                self.put(doc)?;
                effect.changed_documents.push(r.id.clone());
                effect.changed_documents.extend(self.update_cumulative(
                    r.ltc_volume,
                    &r.kind,
                    r.block_height,
                )?);
            }
            SourceVolumeAction::UpdateCumulative {
                volume,
                kind,
                block,
            } => effect.changed_documents = self.update_cumulative(*volume, kind, *block)?,
            SourceVolumeAction::SaveVolatility { lookback, now_ms } => {
                let mut docs = self.documents().collect::<Vec<_>>();
                docs.sort_by(|a, b| b.value.block_height.cmp(&a.value.block_height));
                if *lookback > 0 {
                    docs.truncate(*lookback as usize);
                }
                let value = source_volatility(
                    &docs
                        .iter()
                        .map(|d| d.value.price.unwrap_or(0.0))
                        .collect::<Vec<_>>(),
                )?;
                effect.value = Some(value);
                if docs.len() >= 2 {
                    let mut doc = SourceVolumeDocument {
                        id: "volIndex".into(),
                        ..Default::default()
                    };
                    doc.value.volatility = Some(value);
                    doc.value.updated_at_ms = Some(*now_ms);
                    self.put(doc)?;
                    effect.changed_documents.push("volIndex".into());
                }
            }
        }
        Ok(effect)
    }
    pub fn get_token_price_in_ltc(&self, token_id: &str) -> SourceResult<SourcePrice> {
        source_id(token_id)?;
        self.price_or_default(&format!("0-{token_id}"))
    }
    pub fn get_tl_price_in_ltc(&self) -> SourceResult<SourcePrice> {
        self.price_or_default("vwap-1")
    }
    pub fn get_utxo_equivalent_volume(
        &self,
        r: &SourceValuationRequest,
    ) -> SourceResult<Option<SourceValuation>> {
        source_id(&r.id)?;
        if r.kind == "contract"
            && r.collateral_id.is_some_and(|n| n != 0)
            && r.notional_value.is_some_and(|n| n != 0.0)
            && r.inverse.is_some()
        {
            let SourceValuationVolume::Scalar(volume) = r.volume else {
                return Err(SourceVolumeError::InvalidDocument);
            };
            let price = self.get_token_price_in_ltc(&r.collateral_id.unwrap_or(0).to_string())?;
            let inverse = r.inverse.unwrap_or(false);
            let value = source_contract_ltc_volume(
                volume,
                price.price,
                r.notional_value.unwrap_or(0.0),
                inverse,
                if inverse {
                    r.trade_price.unwrap_or(0.0)
                } else {
                    r.trade_price.ok_or(SourceVolumeError::NonFinite)?
                },
            )?;
            return Ok(Some(SourceValuation {
                value,
                prices: vec![price],
            }));
        }
        if r.kind == "token" {
            let SourceValuationVolume::Token(volumes) = r.volume else {
                return Err(SourceVolumeError::InvalidDocument);
            };
            let mut parts = r.id.split('-');
            let first = parts.next().unwrap_or("");
            let second = parts.next().unwrap_or("undefined");
            let prices = [
                self.get_token_price_in_ltc(first)?,
                self.get_token_price_in_ltc(second)?,
            ];
            let value = source_token_ltc_volume(volumes, [prices[0].price, prices[1].price])?;
            return Ok(Some(SourceValuation {
                value,
                prices: prices.to_vec(),
            }));
        }
        Ok(None)
    }
    fn price_or_default(&self, id: &str) -> SourceResult<SourcePrice> {
        if let Some(d) = self.document(id) {
            if let Some(price) = d.value.price.filter(|p| *p != 0.0) {
                return Ok(SourcePrice {
                    price,
                    origin: SourcePriceOrigin::Direct,
                    block_height: d.value.block_height,
                });
            }
        }
        Ok(SourcePrice {
            price: 0.001,
            origin: SourcePriceOrigin::Fallback,
            block_height: None,
        })
    }
    /// Exact source selection: supplied height is ignored, direct truthy price
    /// wins and inverse lookup only uses the first two hyphen components.
    pub fn get_last_price(&self, pair: &str, _height: u64) -> SourceResult<Option<SourcePrice>> {
        self.lookup_last_price(pair, None)
    }
    fn lookup_last_price(
        &self,
        pair: &str,
        as_of: Option<u64>,
    ) -> SourceResult<Option<SourcePrice>> {
        source_id(pair)?;
        let Some((a, rest)) = pair.split_once('-') else {
            return Ok(None);
        };
        if let Some(d) = self
            .document(pair)
            .filter(|d| as_of.is_none_or(|h| d.value.block_height.is_some_and(|b| b <= h)))
        {
            if let Some(price) = d.value.price.filter(|p| *p != 0.0) {
                return Ok(Some(SourcePrice {
                    price,
                    origin: SourcePriceOrigin::Direct,
                    block_height: d.value.block_height,
                }));
            }
        }
        let b = rest.split('-').next().unwrap_or("");
        let Some(d) = self
            .document(&format!("{b}-{a}"))
            .filter(|d| as_of.is_none_or(|h| d.value.block_height.is_some_and(|b| b <= h)))
        else {
            return Ok(None);
        };
        let Some(price) = d.value.price.filter(|p| *p != 0.0) else {
            return Ok(None);
        };
        let mut inverse = bn_number(&Decimal::from(1_u32).checked_div(
            &number_decimal(price)?,
            20,
            Rounding::HalfUp,
        )?)?;
        if inverse == 0.0 && price.is_sign_negative() {
            inverse = -0.0;
        }
        Ok(Some(SourcePrice {
            price: inverse,
            origin: SourcePriceOrigin::Inverse,
            block_height: d.value.block_height,
        }))
    }
    /// Named repair: never substitutes a future document or fallback constant.
    /// The overwrite store can only see currently retained documents, not a
    /// fabricated old version of a subsequently overwritten row.
    pub fn retained_price_as_of(
        &self,
        pair: &str,
        height: u64,
    ) -> SourceResult<Option<SourcePrice>> {
        source_height(height)?;
        self.lookup_last_price(pair, Some(height))
    }
    pub fn get_cumulative_volumes(&self, block: u64) -> SourceResult<SourceCumulativeVolumes> {
        source_height(block)?;
        Ok(self.cumulative_before(block))
    }
    // Internal exclusive upper bounds may equal MAX_SOURCE_HEIGHT+1 while
    // admitted context/document heights and public query arguments stay safe.
    fn cumulative_before(&self, block: u64) -> SourceCumulativeVolumes {
        let value = |id, read: fn(&SourceVolumeValue) -> Option<f64>| {
            self.document(id)
                .filter(|d| d.value.block.is_some_and(|h| h < block))
                .and_then(|d| read(&d.value))
                .filter(|n| *n != 0.0)
                .unwrap_or(0.0)
        };
        SourceCumulativeVolumes {
            global_cumulative_volume: value("globalCumulativeVolume", |v| {
                v.global_cumulative_volume
            }),
            ltc_pair_total_volume: value("ltcPairCumulativeVolume", |v| v.ltc_pair_total_volume),
            contract_cumulative_volume: value("contractCumulativeVolume", |v| {
                v.contract_cumulative_volume
            }),
        }
    }
    pub fn get_block_volumes(&self, height: u64) -> SourceResult<SourceBlockVolumeResult> {
        source_height(height)?;
        let docs = self
            .documents()
            .filter(|d| d.value.block_height == Some(height))
            .collect::<Vec<_>>();
        if docs.is_empty() {
            return Ok(SourceBlockVolumeResult::EmptyZero);
        }
        let (mut all, mut ltc) = (Decimal::zero(), Decimal::zero());
        for d in docs {
            let volume = number_decimal(d.value.ltc_volume.ok_or(SourceVolumeError::NonFinite)?)?;
            all = all.checked_add(&volume)?;
            if d.id.contains('0') {
                ltc = ltc.checked_add(&volume)?;
            }
        }
        Ok(SourceBlockVolumeResult::Totals {
            global: bn_number(&all)?,
            ltc_pairs: bn_number(&ltc)?,
        })
    }
    pub fn sample_volumes_by_block(&self, height: u64) -> SourceResult<Vec<SourceSample>> {
        self.sample_volumes_by_block_range(height, height)
    }
    pub fn sample_volumes_by_block_range(
        &self,
        start: u64,
        end: u64,
    ) -> SourceResult<Vec<SourceSample>> {
        source_height(start)?;
        source_height(end)?;
        if start > end {
            return Err(SourceVolumeError::InvalidRange);
        }
        Ok(self
            .documents()
            .filter(|d| d.top_block_height.is_some_and(|h| h >= start && h <= end))
            .map(|d| SourceSample {
                id: d.id.clone(),
                volume: d.top_volume,
            })
            .collect())
    }
    /// Numeric property IDs bound the source's otherwise unescaped regex.
    pub fn calculate_cumulative_volume(&self, id1: u32, id2: u32) -> SourceResult<f64> {
        let prefix = format!("{id1}-{id2}-");
        let mut total = 0.0;
        for d in self.documents().filter(|d| d.id.starts_with(&prefix)) {
            total = finite(total + d.top_volume.ok_or(SourceVolumeError::NonFinite)?)?;
        }
        Ok(total)
    }
    pub fn audit_vwap(&self, height: u64) -> SourceResult<Option<f64>> {
        let samples = self.sample_volumes_by_block(height)?;
        let (mut total, mut cost) = (0.0, 0.0);
        for s in samples {
            let volume = s.volume.ok_or(SourceVolumeError::NonFinite)?;
            let price = self.get_token_price_in_ltc(&s.id)?.price;
            total = finite(total + volume)?;
            cost = finite(cost + volume * price)?;
        }
        if total == 0.0 {
            Ok(None)
        } else {
            Ok(Some(finite(cost / total)?))
        }
    }
    pub fn get_vwap(
        &self,
        pair: VolumePair,
        height: u64,
        trailing: u64,
    ) -> SourceResult<Option<f64>> {
        source_height(height)?;
        source_height(trailing)?;
        let ids = [
            format!("{}-{}", pair.first, pair.second),
            format!("{}-{}", pair.second, pair.first),
        ];
        let docs = self.documents().filter(|d| {
            ids.contains(&d.id)
                && d.value
                    .block_height
                    .is_some_and(|h| h <= height && (trailing >= height || h >= height - trailing))
        });
        let (mut total, mut cost) = (Decimal::zero(), Decimal::zero());
        for d in docs {
            let volume = number_decimal(d.value.volume.ok_or(SourceVolumeError::NonFinite)?)?;
            let price = number_decimal(d.value.price.ok_or(SourceVolumeError::NonFinite)?)?;
            total = total.checked_add(&volume)?;
            cost = cost.checked_add(&volume.checked_mul(&price)?)?;
        }
        if total.is_zero() {
            Ok(None)
        } else {
            let negative = cost.is_negative() != total.is_negative();
            let result = bn_quantized(
                &cost.checked_div(&total, 20, Rounding::HalfUp)?,
                8,
                Rounding::HalfUp,
            )?;
            Ok(Some(if result == 0.0 && negative {
                -0.0
            } else {
                result
            }))
        }
    }
    /// Explicit useful repair for the source's broken Promise chain/options:
    /// select matching prefix occurrences, stable top-level height descending,
    /// then the requested number of rows. Does not claim source DB parity.
    pub fn repaired_recent_vwap(
        &self,
        id1: u32,
        id2: u32,
        limit: usize,
    ) -> SourceResult<Option<f64>> {
        if limit > MAX_VOLUME_RECORDS {
            return Err(SourceVolumeError::Capacity);
        }
        let pattern = format!("{id1}-{id2}-");
        let mut docs = self
            .documents()
            .filter(|d| d.id.contains(&pattern))
            .collect::<Vec<_>>();
        docs.sort_by(|a, b| b.top_block_height.cmp(&a.top_block_height));
        docs.truncate(limit);
        source_calculate_vwap(
            &docs
                .into_iter()
                .map(|d| {
                    Ok((
                        d.value.volume.ok_or(SourceVolumeError::NonFinite)?,
                        d.value.price.ok_or(SourceVolumeError::NonFinite)?,
                    ))
                })
                .collect::<SourceResult<Vec<_>>>()?,
        )
    }
    pub fn liquidity_reward(
        &self,
        trade_volume: f64,
        context: &SourceRewardContext,
    ) -> SourceResult<f64> {
        context.validate()?;
        let fee = if let Some(fee) = context.fee_paid {
            fee
        } else {
            bn_number(
                &number_decimal(trade_volume)?
                    .checked_mul(&number_decimal(context.fee_rate.unwrap_or(0.00005))?)?,
            )?
        };
        let mut cumulative = context.cumulative_ltc.unwrap_or(0.0);
        if cumulative <= 0.0 {
            let block = context.block.unwrap_or(0);
            let c = self.cumulative_before(if block > 0 {
                block
                    .checked_add(1)
                    .ok_or(SourceVolumeError::InvalidRange)?
            } else {
                2
            });
            cumulative = if c.ltc_pair_total_volume != 0.0 {
                c.ltc_pair_total_volume
            } else {
                c.global_cumulative_volume
            };
        }
        source_attenuated_liquidity_reward(fee, cumulative, &context.options)
    }
    pub fn baseline_liquidity_reward(
        &self,
        trade_volume: f64,
        fee: f64,
        context: &SourceRewardContext,
    ) -> SourceResult<f64> {
        let mut context = context.clone();
        context.fee_paid = Some(fee);
        self.liquidity_reward(trade_volume, &context)
    }
    pub fn unsupported_source_path(method: &str) -> SourceVolumeError {
        SourceVolumeError::UnsupportedSourcePath(match method {
            "constructor" => "constructor.dbPath",
            "saveCumulativeVolume" => "saveCumulativeVolume.undefined_function",
            "saveVWAP" => "saveVWAP.undefined_volume_price",
            "vestTokens" => "vestTokens.undefined_totalVolume",
            _ => "unresolved_source_database_chain",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceContractInfo {
    pub notional_property_id: u32,
    pub notional_value: f64,
    pub leverage: f64,
    pub native: bool,
    pub inverse: bool,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceContractFields {
    pub id: Option<u32>,
    pub ticker: Option<String>,
    pub native: bool,
    pub inverse: bool,
    pub notional_property_id: Option<u32>,
    pub notional_value: Option<f64>,
    pub collateral_property_id: Option<u32>,
    pub leverage: Option<f64>,
    pub on_chain_data: Vec<(u32, u32)>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct SourceContractMetadata {
    pub contract_id: u32,
    pub ticker: Option<String>,
    pub info: SourceContractInfo,
    pub collateral_property_id: Option<u32>,
    pub on_chain_data: Vec<(u32, u32)>,
}
/// Caller selects raw.data when truthy, otherwise raw, as the source adapter
/// does. This typed finite shape excludes coercive strings/objects; those stay
/// actual-source observations, not accepted contract definitions.
pub fn source_normalize_contract_info(
    requested_id: u32,
    outer_id: Option<u32>,
    fields: SourceContractFields,
) -> SourceResult<SourceContractMetadata> {
    if fields
        .ticker
        .as_ref()
        .is_some_and(|t| t.len() > MAX_IDENTIFIER_BYTES || t.chars().any(char::is_control))
        || fields.on_chain_data.len() > 64
    {
        return Err(SourceVolumeError::InvalidDocument);
    }
    Ok(SourceContractMetadata {
        contract_id: fields.id.or(outer_id).unwrap_or(requested_id),
        ticker: fields.ticker,
        info: SourceContractInfo {
            notional_property_id: fields.notional_property_id.unwrap_or(0),
            notional_value: fields.notional_value.unwrap_or(1.0),
            leverage: fields.leverage.unwrap_or(1.0),
            native: fields.native,
            inverse: fields.inverse,
        }
        .normalized()?,
        collateral_property_id: fields.collateral_property_id,
        on_chain_data: fields.on_chain_data,
    })
}
impl Default for SourceContractInfo {
    fn default() -> Self {
        Self {
            notional_property_id: 0,
            notional_value: 1.0,
            leverage: 1.0,
            native: false,
            inverse: false,
        }
    }
}
impl SourceContractInfo {
    pub fn normalized(mut self) -> SourceResult<Self> {
        finite(self.notional_value)?;
        finite(self.leverage)?;
        if self.leverage == 0.0 {
            self.leverage = 1.0;
        }
        Ok(self)
    }
}
pub fn source_contract_unit_ltc(
    info: SourceContractInfo,
    token_price: f64,
    details: bool,
) -> SourceResult<f64> {
    let info = info.normalized()?;
    let token_price = if info.notional_property_id == 0 {
        1.0
    } else {
        finite(token_price)?
    };
    let notional = info.notional_value * token_price;
    if !details && (!notional.is_finite() || notional <= 0.0) {
        return Ok(0.0);
    }
    if !notional.is_finite() {
        return Err(SourceVolumeError::ZeroContractValue);
    }
    let result = number_fixed_eight(finite(notional / info.leverage)?)?;
    if details && result == 0.0 {
        return Err(SourceVolumeError::ZeroContractValue);
    }
    Ok(result)
}
pub fn source_token_ltc_volume(volumes: [f64; 2], prices: [f64; 2]) -> SourceResult<f64> {
    for n in volumes.into_iter().chain(prices) {
        finite(n)?;
    }
    finite(prices[0] * volumes[0] + prices[1] * volumes[1])
}
pub fn source_contract_ltc_volume(
    volume: f64,
    collateral_price: f64,
    notional: f64,
    inverse: bool,
    trade_price: f64,
) -> SourceResult<f64> {
    let mut amount = number_decimal(volume)?
        .checked_mul(&number_decimal(collateral_price)?)?
        .checked_mul(&number_decimal(notional)?)?;
    if !inverse {
        amount = amount.checked_mul(&number_decimal(trade_price)?)?;
    }
    let result = bn_quantized(&amount, 8, Rounding::HalfUp)?;
    let negative = volume.is_sign_negative()
        ^ collateral_price.is_sign_negative()
        ^ notional.is_sign_negative()
        ^ (!inverse && trade_price.is_sign_negative());
    Ok(if result == 0.0 && negative {
        -0.0
    } else {
        result
    })
}
pub fn source_calculate_vwap(entries: &[(f64, f64)]) -> SourceResult<Option<f64>> {
    if entries.len() > MAX_VOLUME_RECORDS {
        return Err(SourceVolumeError::Capacity);
    }
    let (mut total, mut cost) = (0.0, 0.0);
    for &(v, p) in entries {
        finite(v)?;
        finite(p)?;
        total = finite(total + v)?;
        cost = finite(cost + v * p)?;
    }
    if total == 0.0 {
        Ok(None)
    } else {
        Ok(Some(finite(cost / total)?))
    }
}
/// Input is the already queried/sorted document-price list, before filtering.
/// Annualization is the source's 144*365, independent of actual network cadence.
pub fn source_volatility(prices: &[f64]) -> SourceResult<f64> {
    if prices.len() > MAX_VOLUME_RECORDS {
        return Err(SourceVolumeError::Capacity);
    }
    for &p in prices {
        finite(p)?;
    }
    if prices.len() < 2 {
        return Ok(0.0);
    }
    let prices = prices
        .iter()
        .copied()
        .filter(|p| *p > 0.0)
        .collect::<Vec<_>>();
    let returns = prices
        .windows(2)
        .map(|p| (p[1] / p[0]).ln())
        .collect::<Vec<_>>();
    let mean = returns.iter().fold(0.0, |sum, r| sum + r) / returns.len() as f64;
    let variance =
        returns.iter().fold(0.0, |sum, r| sum + (r - mean).powi(2)) / (returns.len() as f64 - 1.0);
    finite(variance.sqrt() * (144.0_f64 * 365.0).sqrt())
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceRewardOptions {
    pub max_share: f64,
    pub min_share: f64,
    pub pivot_ltc: f64,
    pub slope: f64,
}
impl Default for SourceRewardOptions {
    fn default() -> Self {
        Self {
            max_share: 0.35,
            min_share: 0.02,
            pivot_ltc: 1000.0,
            slope: 1.0,
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceRewardContext {
    pub fee_paid: Option<f64>,
    pub fee_rate: Option<f64>,
    pub cumulative_ltc: Option<f64>,
    pub block: Option<u64>,
    pub options: SourceRewardOptions,
}
impl SourceRewardContext {
    fn validate(&self) -> SourceResult<()> {
        for n in [self.fee_paid, self.fee_rate, self.cumulative_ltc]
            .into_iter()
            .flatten()
        {
            finite(n)?;
        }
        if let Some(h) = self.block {
            source_height(h)?;
        }
        validate_options(&self.options)
    }
}
fn validate_options(o: &SourceRewardOptions) -> SourceResult<()> {
    for n in [o.max_share, o.min_share, o.pivot_ltc, o.slope] {
        finite(n)?;
    }
    Ok(())
}
pub fn source_liquidity_reward_share(
    cumulative: f64,
    o: &SourceRewardOptions,
) -> SourceResult<f64> {
    finite(cumulative)?;
    validate_options(o)?;
    let scaled = source_math_max(cumulative, 0.0) / source_math_max(o.pivot_ltc, 1.0);
    let attenuation = o.max_share / (1.0 + (1.0 + scaled).log10() * source_math_max(o.slope, 0.0));
    if attenuation.is_nan() {
        return Err(SourceVolumeError::NonFinite);
    }
    finite(source_math_min(
        0.99,
        source_math_max(o.min_share, source_math_min(o.max_share, attenuation)),
    ))
}
// Rust's float min/max do not specify the JS signed-zero tie rule. Keep it
// explicit instead of depending on operand order, target or optimization.
fn source_math_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() && b.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else if a > b {
        a
    } else {
        b
    }
}
fn source_math_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() || b.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else if a < b {
        a
    } else {
        b
    }
}
pub fn source_attenuated_liquidity_reward(
    fee: f64,
    cumulative: f64,
    o: &SourceRewardOptions,
) -> SourceResult<f64> {
    finite(fee)?;
    if fee <= 0.0 {
        return Ok(0.0);
    }
    let share = source_liquidity_reward_share(cumulative, o)?;
    let result = bn_quantized(
        &number_decimal(fee)?.checked_mul(&number_decimal(share)?)?,
        8,
        Rounding::Down,
    )?;
    Ok(if result == 0.0 && share.is_sign_negative() {
        -0.0
    } else {
        result
    })
}
/// Source static globalCumulativeVolume is normally undefined, so vestTokens
/// returns zeros; above1e9 it reaches undefined totalVolume. No issuance occurs.
pub fn source_vesting_projection(global: Option<f64>) -> SourceResult<(f64, f64)> {
    if let Some(g) = global {
        finite(g)?;
        if (g / 1e9).log10() > 0.0 {
            return Err(SourceVolumeStore::unsupported_source_path("vestTokens"));
        }
    }
    Ok((0.0, 0.0))
}
fn finite(n: f64) -> SourceResult<f64> {
    if n.is_finite() {
        Ok(n)
    } else {
        Err(SourceVolumeError::NonFinite)
    }
}
fn number_decimal(n: f64) -> SourceResult<Decimal> {
    finite(n)?;
    crate::state_oracle::source_number_text(n)
        .map_err(|_| SourceVolumeError::NonFinite)?
        .parse()
        .map_err(|_| SourceVolumeError::Arithmetic)
}
fn bn_number(d: &Decimal) -> SourceResult<f64> {
    finite(
        d.to_string()
            .parse()
            .map_err(|_| SourceVolumeError::Arithmetic)?,
    )
}
fn number_fixed_eight(n: f64) -> SourceResult<f64> {
    finite(n)?;
    // Number((-0).toFixed(8)) is positive zero. A nonzero negative input
    // which rounds to zero still emits a minus sign and returns negative zero.
    if n == 0.0 {
        return Ok(0.0);
    }
    if n.abs() >= 1e21 {
        return Ok(n);
    }
    if n.abs() < 0.000000005 {
        return Ok(if n.is_sign_negative() { -0.0 } else { 0.0 });
    }
    let bits = n.to_bits();
    let mantissa = (bits & ((1_u64 << 52) - 1)) | if n.is_normal() { 1_u64 << 52 } else { 0 };
    let exponent = if n.is_normal() {
        ((bits >> 52) & 2047) as i32 - 1023 - 52
    } else {
        -1074
    };
    let mut d = Decimal::from(mantissa);
    if exponent >= 0 {
        for _ in 0..exponent {
            d = d.checked_mul(&Decimal::from(2_u32))?;
        }
    } else {
        for _ in 0..-exponent {
            d = d.checked_mul(&Decimal::from(5_u32))?;
        }
        d = format!("{d}e{exponent}")
            .parse()
            .map_err(|_| SourceVolumeError::Arithmetic)?;
    }
    if n.is_sign_negative() {
        d = d.negated();
    }
    let rounded = d.quantize(8, Rounding::HalfUp)?;
    if rounded.is_zero() && n.is_sign_negative() {
        Ok(-0.0)
    } else {
        bn_number(&rounded)
    }
}
fn bn_quantized(d: &Decimal, places: u32, rounding: Rounding) -> SourceResult<f64> {
    let result = d.quantize(places, rounding)?;
    if result.is_zero() && d.is_negative() {
        Ok(-0.0)
    } else {
        bn_number(&result)
    }
}
fn source_id(id: &str) -> SourceResult<()> {
    if id.is_empty() || id.len() > MAX_IDENTIFIER_BYTES || id.chars().any(char::is_control) {
        Err(SourceVolumeError::InvalidDocument)
    } else {
        Ok(())
    }
}
fn source_height(h: u64) -> SourceResult<()> {
    if h <= MAX_SOURCE_HEIGHT {
        Ok(())
    } else {
        Err(SourceVolumeError::InvalidRange)
    }
}
fn source_kind(s: &str) -> SourceResult<()> {
    if s.len() > 32 || s.bytes().any(|b| b.is_ascii_control()) {
        Err(SourceVolumeError::InvalidDocument)
    } else {
        Ok(())
    }
}
fn validate_document(d: &SourceVolumeDocument) -> SourceResult<()> {
    source_id(&d.id)?;
    for h in [d.top_block_height, d.value.block_height, d.value.block]
        .into_iter()
        .flatten()
    {
        source_height(h)?;
    }
    for n in [
        d.top_volume,
        d.value.raw_volume,
        d.value.ltc_volume,
        d.value.price,
        d.value.volume,
        d.value.global_cumulative_volume,
        d.value.ltc_pair_total_volume,
        d.value.contract_cumulative_volume,
        d.value.volatility,
    ]
    .into_iter()
    .flatten()
    {
        finite(n)?;
    }
    if let Some(k) = &d.value.kind {
        source_kind(k)?;
    }
    if d.value
        .updated_at_ms
        .is_some_and(|n| !(-MAX_SOURCE_TIME..=MAX_SOURCE_TIME).contains(&n))
    {
        return Err(SourceVolumeError::InvalidRange);
    }
    Ok(())
}
fn validate_action(a: &SourceVolumeAction) -> SourceResult<()> {
    match a {
        SourceVolumeAction::Put(d) => validate_document(d),
        SourceVolumeAction::SaveVolume(r) => {
            source_id(&r.id)?;
            source_kind(&r.kind)?;
            source_height(r.block_height)?;
            for n in [r.raw_volume, r.ltc_volume, r.price] {
                finite(n)?;
            }
            Ok(())
        }
        SourceVolumeAction::UpdateCumulative {
            volume,
            kind,
            block,
        } => {
            finite(*volume)?;
            source_kind(kind)?;
            source_height(*block)
        }
        SourceVolumeAction::SaveVolatility { lookback, now_ms } => {
            if *lookback as usize > MAX_VOLUME_RECORDS
                || !(-MAX_SOURCE_TIME..=MAX_SOURCE_TIME).contains(now_ms)
            {
                Err(SourceVolumeError::InvalidRange)
            } else {
                Ok(())
            }
        }
    }
}

// Complete replayed checkpoint: action log plus redundant derived documents.
impl SourceVolumeStore {
    pub fn checkpoint_bytes(&self) -> SourceResult<Vec<u8>> {
        let mut replay = Self::new(self.capacity)?;
        for a in &self.actions {
            validate_action(a)?;
            replay.apply_inner(a)?;
        }
        if document_bytes(self)? != document_bytes(&replay)? {
            return Err(SourceVolumeError::InvalidCheckpoint);
        }
        let mut out = b"TLVI\x01".to_vec();
        out.extend_from_slice(&(self.capacity as u32).to_be_bytes());
        out.extend_from_slice(&(self.actions.len() as u32).to_be_bytes());
        for a in &self.actions {
            encode_action(&mut out, a)?;
        }
        out.extend(document_bytes(self)?);
        crate::codec::with_crc(out).map_err(|_| SourceVolumeError::Capacity)
    }
    pub fn from_checkpoint_bytes(bytes: &[u8]) -> SourceResult<Self> {
        let payload =
            crate::codec::verify_crc(bytes).map_err(|_| SourceVolumeError::InvalidCheckpoint)?;
        let mut c = crate::codec::Cursor::new(payload);
        if c.take(5).map_err(codec_error)? != b"TLVI\x01" {
            return Err(SourceVolumeError::InvalidCheckpoint);
        }
        let mut store = Self::new(c.u32().map_err(codec_error)? as usize)?;
        let count = c.u32().map_err(codec_error)? as usize;
        if count > MAX_SOURCE_ACTIONS {
            return Err(SourceVolumeError::Capacity);
        }
        for _ in 0..count {
            let a = decode_action(&mut c)?;
            validate_action(&a)?;
            store.apply_inner(&a)?;
            store.actions.push(a);
        }
        let cache = document_bytes(&store)?;
        if c.take(cache.len()).map_err(codec_error)? != cache {
            return Err(SourceVolumeError::InvalidCheckpoint);
        }
        c.end().map_err(codec_error)?;
        if store.checkpoint_bytes()? != bytes {
            return Err(SourceVolumeError::InvalidCheckpoint);
        }
        Ok(store)
    }
}
fn codec_error(_: crate::codec::CodecError) -> SourceVolumeError {
    SourceVolumeError::InvalidCheckpoint
}
fn write_string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}
fn read_string(c: &mut crate::codec::Cursor<'_>, limit: usize) -> SourceResult<String> {
    let n = c.u32().map_err(codec_error)? as usize;
    if n > limit {
        return Err(SourceVolumeError::Capacity);
    }
    String::from_utf8(c.take(n).map_err(codec_error)?.to_vec())
        .map_err(|_| SourceVolumeError::InvalidCheckpoint)
}
fn write_opt_u64(out: &mut Vec<u8>, n: Option<u64>) {
    out.push(u8::from(n.is_some()));
    if let Some(n) = n {
        out.extend_from_slice(&n.to_be_bytes());
    }
}
fn read_opt_u64(c: &mut crate::codec::Cursor<'_>) -> SourceResult<Option<u64>> {
    match c.u8().map_err(codec_error)? {
        0 => Ok(None),
        1 => Ok(Some(c.u64().map_err(codec_error)?)),
        _ => Err(SourceVolumeError::InvalidCheckpoint),
    }
}
fn write_opt_float(out: &mut Vec<u8>, n: Option<f64>) {
    write_opt_u64(out, n.map(f64::to_bits));
}
fn read_opt_float(c: &mut crate::codec::Cursor<'_>) -> SourceResult<Option<f64>> {
    read_opt_u64(c)?
        .map(|n| finite(f64::from_bits(n)))
        .transpose()
}
fn encode_doc(out: &mut Vec<u8>, d: &SourceVolumeDocument) -> SourceResult<()> {
    validate_document(d)?;
    write_string(out, &d.id);
    write_opt_u64(out, d.top_block_height);
    write_opt_float(out, d.top_volume);
    write_opt_u64(out, d.value.block_height);
    for n in [
        d.value.raw_volume,
        d.value.ltc_volume,
        d.value.price,
        d.value.volume,
    ] {
        write_opt_float(out, n);
    }
    out.push(u8::from(d.value.kind.is_some()));
    if let Some(k) = &d.value.kind {
        write_string(out, k);
    }
    write_opt_u64(out, d.value.block);
    for n in [
        d.value.global_cumulative_volume,
        d.value.ltc_pair_total_volume,
        d.value.contract_cumulative_volume,
        d.value.volatility,
    ] {
        write_opt_float(out, n);
    }
    write_opt_u64(
        out,
        d.value
            .updated_at_ms
            .map(|n| u64::from_be_bytes(n.to_be_bytes())),
    );
    Ok(())
}
fn decode_doc(c: &mut crate::codec::Cursor<'_>) -> SourceResult<SourceVolumeDocument> {
    let id = read_string(c, MAX_IDENTIFIER_BYTES)?;
    let top_block_height = read_opt_u64(c)?;
    let top_volume = read_opt_float(c)?;
    let block_height = read_opt_u64(c)?;
    let raw_volume = read_opt_float(c)?;
    let ltc_volume = read_opt_float(c)?;
    let price = read_opt_float(c)?;
    let volume = read_opt_float(c)?;
    let kind = match c.u8().map_err(codec_error)? {
        0 => None,
        1 => Some(read_string(c, 32)?),
        _ => return Err(SourceVolumeError::InvalidCheckpoint),
    };
    let block = read_opt_u64(c)?;
    let global_cumulative_volume = read_opt_float(c)?;
    let ltc_pair_total_volume = read_opt_float(c)?;
    let contract_cumulative_volume = read_opt_float(c)?;
    let volatility = read_opt_float(c)?;
    let updated_at_ms = read_opt_u64(c)?.map(|n| i64::from_be_bytes(n.to_be_bytes()));
    Ok(SourceVolumeDocument {
        id,
        top_block_height,
        top_volume,
        value: SourceVolumeValue {
            block_height,
            raw_volume,
            ltc_volume,
            price,
            volume,
            kind,
            block,
            global_cumulative_volume,
            ltc_pair_total_volume,
            contract_cumulative_volume,
            volatility,
            updated_at_ms,
        },
    })
}
fn encode_action(out: &mut Vec<u8>, a: &SourceVolumeAction) -> SourceResult<()> {
    match a {
        SourceVolumeAction::Put(d) => {
            out.push(0);
            encode_doc(out, d)?;
        }
        SourceVolumeAction::SaveVolume(r) => {
            out.push(1);
            write_string(out, &r.id);
            for n in [r.raw_volume, r.ltc_volume, r.price] {
                out.extend_from_slice(&n.to_bits().to_be_bytes());
            }
            out.extend_from_slice(&r.block_height.to_be_bytes());
            write_string(out, &r.kind);
        }
        SourceVolumeAction::UpdateCumulative {
            volume,
            kind,
            block,
        } => {
            out.push(2);
            out.extend_from_slice(&volume.to_bits().to_be_bytes());
            write_string(out, kind);
            out.extend_from_slice(&block.to_be_bytes());
        }
        SourceVolumeAction::SaveVolatility { lookback, now_ms } => {
            out.push(3);
            out.extend_from_slice(&lookback.to_be_bytes());
            out.extend_from_slice(&now_ms.to_be_bytes());
        }
    }
    Ok(())
}
fn decode_action(c: &mut crate::codec::Cursor<'_>) -> SourceResult<SourceVolumeAction> {
    match c.u8().map_err(codec_error)? {
        0 => Ok(SourceVolumeAction::Put(decode_doc(c)?)),
        1 => Ok(SourceVolumeAction::SaveVolume(SourceVolumeSave {
            id: read_string(c, MAX_IDENTIFIER_BYTES)?,
            raw_volume: finite(f64::from_bits(c.u64().map_err(codec_error)?))?,
            ltc_volume: finite(f64::from_bits(c.u64().map_err(codec_error)?))?,
            price: finite(f64::from_bits(c.u64().map_err(codec_error)?))?,
            block_height: c.u64().map_err(codec_error)?,
            kind: read_string(c, 32)?,
        })),
        2 => Ok(SourceVolumeAction::UpdateCumulative {
            volume: finite(f64::from_bits(c.u64().map_err(codec_error)?))?,
            kind: read_string(c, 32)?,
            block: c.u64().map_err(codec_error)?,
        }),
        3 => Ok(SourceVolumeAction::SaveVolatility {
            lookback: c.u32().map_err(codec_error)?,
            now_ms: i64::from_be_bytes(c.fixed().map_err(codec_error)?),
        }),
        _ => Err(SourceVolumeError::InvalidCheckpoint),
    }
}
fn document_bytes(s: &SourceVolumeStore) -> SourceResult<Vec<u8>> {
    let mut out = Vec::new();
    out.extend_from_slice(&(s.len() as u32).to_be_bytes());
    for d in s.documents() {
        encode_doc(&mut out, d)?;
    }
    Ok(out)
}
