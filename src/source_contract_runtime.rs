//! Literal pinned contract execution state. This owner is an alternative to the
//! conserving EconomicState, not a shadow spendable balance store beside it.
//! Its single Tally collection/cache retains source Number and failure behavior.
use crate::contract_queue_source_owner::{ContractQueueSourceOwner, QueueDbFailure};
use crate::margin_map_source_owner::{
    MarginMapSourceOwner, PositionAlias, PositionFieldObjectAlias, PositionObjectAlias,
};
use crate::source_contract_values::*;
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceOutcome, SourceRuleError,
    SourceValue as V,
};

pub const MAX_TABLE_ROWS: usize = 1024;
pub const MAX_TABLES: usize = 32;
/// Replay binds the admitted method graph as well as the original commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceContractRuntimePolicy {
    OnchainV1,
    GlobalClearingV2,
    SourceLifecycleV3,
    OptionLifecycleV4,
    DistributionLifecycleV5,
    ProofLifecycleV6,
    LowerLifecycleV7,
    MainCallableV8,
    SpotLifecycleV9,
    ActivationLifecycleV10,
    AmmLifecycleV11,
    MainOrchestrationV12,
    RichThrownV14,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageFailure {
    pub collection: String,
    pub operation: String,
    pub skip: usize,
    pub message: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Table {
    name: String,
    rows: Vec<V>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceContractRuntime {
    pub(crate) queue: ContractQueueSourceOwner,
    pub(crate) core: ContractCore,
    last_parameters: Option<V>,
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ContractCore {
    policy: SourceContractRuntimePolicy,
    queue_context: Option<ContractQueueSourceOwner>,
    clearing_contexts: Vec<(String, Vec<usize>)>,
    deleverage_trades: Vec<(String, Vec<V>)>,
    liquidation_marks: Vec<(String, V)>,
    contract_series: Vec<(V, V)>,
    contract_series_initialized: bool,
    contract_registry_initialized: bool,
    channel_inverse_jobs: Vec<(V, Option<crate::validity_source::SourceException>)>,
    channel_balance_jobs: Vec<SourceChannelBalanceJob>,
    channel_balance_stage: u8,
    channel_save_writes: usize,
    channel_token_validation: bool,
    source_token_threshold_reads: Vec<SourceTokenThresholdRead>,
    source_main_withdrawal_save: Option<SourceMainWithdrawalSave>,
    channel_sufficiency_reads: Vec<bool>,
    channel_token_price_job: Option<SourceChannelTokenPriceJob>,
    tally_mod: V,
    tally_static_initialized: bool,
    channels: crate::channels_source::SourceChannels,
    property_cache: Vec<(V, V)>,
    source_amm_instances: Vec<V>,
    source_amm_property_index: Vec<(String, usize)>,
    source_amm_returned_origin: Option<usize>,
    source_proof_environment: V,
    source_proof_artifacts: V,
    source_proof_property_initialized: bool,
    source_clearlist_lists: Vec<(V, V)>,
    source_clearlist_banlist: V,
    source_clearlist_xpub_cache: Vec<(V, V)>,
    source_clearlist_roots: Vec<(V, V)>,
    source_clearlist_library: crate::bip32_public::SourceLibrary,
    source_clearlist_library_locked: bool,
    vault_initialized: bool,
    vault_cache: Vec<(V, V)>,
    synthetic_token_cache: Vec<(V, V)>,
    source_unhandled_rejections: Vec<V>,
    source_supply_delta: Option<SourceSupplyDelta>,
    source_history_jobs: Vec<SourceHistoryJob>,
    tables: Vec<Table>,
    margins: MarginMapSourceOwner,
    tally: Vec<(V, V)>,
    fee_buffer: Vec<(V, Vec<(String, V)>)>,
    block_trades: Vec<(String, V)>,
    oracle_marks: Vec<(V, V)>,
    native_marks: Vec<(V, V)>,
    globals: V,
    effects: Vec<V>,
    effects_exhausted: bool,
    failures: Vec<StorageFailure>,
    uuid: u64,
    db_id: u64,
    random: u32,
    height: V,
    time_millis: u64,
    contract_mod: bool,
    contract_mod_raw: V,
    entry: String,
    active: bool,
    insurance: V,
    oracle_source_cache: Vec<(V, V)>,
    oracle_source_watermarks: Vec<(V, V)>,
    oracle_source_initialized: bool,
    returned_position_origin: Option<usize>,
    collateral_position_origin: Option<usize>,
    position_identity_active: bool,
    source_send_tally_instance_initialized: bool,
    source_send_tally_read_job: Option<SourceSendTallyReadJob>,
    activation_client_profile: V,
    activation_client_locked: bool,
    activation_vesting: V,
    activation_consensus_vector: Vec<V>,
    activation_native_errors: Vec<V>,
    activation_insurance_job: Option<SourceActivationInsuranceJob>,
    activation_clearlist_jobs: Vec<SourceActivationClearlistJob>,
    activation_async_active: bool,
    activation_async_clock: u64,
    activation_async_serial: u64,
    activation_async_running: bool,
    activation_async_child_depth: u8,
    main_activation: crate::activation::SourceActivation,
    main_activation_profile: String,
    main_activation_locked: bool,
    main_height_job: Option<SourceMainHeightJob>,
    main_production: Option<SourceMainProduction>,
    source_rich: Option<rich_runtime_v14::State>,
    native_main_binding: Option<V>,
    native_active_service_client_scope: Option<&'static str>,
    main_height_rejections: Vec<crate::validity_source::SourceException>,
    source_spot_main_active: bool,
    source_spot_orders_read: Option<SourceSpotOrdersReadJob>,
    source_spot_fee_buffer_initialized: bool,
}
impl std::fmt::Debug for ContractCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut view = f.debug_struct("ContractCore");
        view.field("policy", &self.policy);
        view.field("queue_context", &self.queue_context);
        view.field("clearing_contexts", &self.clearing_contexts);
        view.field("deleverage_trades", &self.deleverage_trades);
        view.field("liquidation_marks", &self.liquidation_marks);
        view.field("contract_series", &self.contract_series);
        view.field(
            "contract_series_initialized",
            &self.contract_series_initialized,
        );
        view.field(
            "contract_registry_initialized",
            &self.contract_registry_initialized,
        );
        view.field("channel_inverse_jobs", &self.channel_inverse_jobs);
        view.field("channel_balance_jobs", &self.channel_balance_jobs);
        view.field("channel_balance_stage", &self.channel_balance_stage);
        view.field("channel_save_writes", &self.channel_save_writes);
        view.field("channel_token_validation", &self.channel_token_validation);
        view.field(
            "source_token_threshold_reads",
            &self.source_token_threshold_reads,
        );
        view.field(
            "source_main_withdrawal_save",
            &self.source_main_withdrawal_save,
        );
        view.field("channel_sufficiency_reads", &self.channel_sufficiency_reads);
        view.field("channel_token_price_job", &self.channel_token_price_job);
        view.field("tally_mod", &self.tally_mod);
        view.field("tally_static_initialized", &self.tally_static_initialized);
        view.field("channels", &self.channels);
        view.field("property_cache", &self.property_cache);
        view.field("source_amm_instances", &self.source_amm_instances);
        view.field("source_amm_property_index", &self.source_amm_property_index);
        view.field(
            "source_amm_returned_origin",
            &self.source_amm_returned_origin,
        );
        view.field("source_proof_environment", &self.source_proof_environment);
        view.field("source_proof_artifacts", &self.source_proof_artifacts);
        view.field(
            "source_proof_property_initialized",
            &self.source_proof_property_initialized,
        );
        view.field("source_clearlist_lists", &self.source_clearlist_lists);
        view.field("source_clearlist_banlist", &self.source_clearlist_banlist);
        view.field(
            "source_clearlist_xpub_cache",
            &self.source_clearlist_xpub_cache,
        );
        view.field("source_clearlist_roots", &self.source_clearlist_roots);
        view.field("source_clearlist_library", &self.source_clearlist_library);
        view.field(
            "source_clearlist_library_locked",
            &self.source_clearlist_library_locked,
        );
        view.field("vault_initialized", &self.vault_initialized);
        view.field("vault_cache", &self.vault_cache);
        view.field("synthetic_token_cache", &self.synthetic_token_cache);
        view.field(
            "source_unhandled_rejections",
            &self.source_unhandled_rejections,
        );
        view.field("source_supply_delta", &self.source_supply_delta);
        view.field("source_history_jobs", &self.source_history_jobs);
        view.field("tables", &self.tables);
        view.field("margins", &self.margins);
        view.field("tally", &self.tally);
        view.field("fee_buffer", &self.fee_buffer);
        view.field("block_trades", &self.block_trades);
        view.field("oracle_marks", &self.oracle_marks);
        view.field("native_marks", &self.native_marks);
        view.field("globals", &self.globals);
        view.field("effects", &self.effects);
        view.field("effects_exhausted", &self.effects_exhausted);
        view.field("failures", &self.failures);
        view.field("uuid", &self.uuid);
        view.field("db_id", &self.db_id);
        view.field("random", &self.random);
        view.field("height", &self.height);
        view.field("time_millis", &self.time_millis);
        view.field("contract_mod", &self.contract_mod);
        view.field("contract_mod_raw", &self.contract_mod_raw);
        view.field("entry", &self.entry);
        view.field("active", &self.active);
        view.field("insurance", &self.insurance);
        view.field("oracle_source_cache", &self.oracle_source_cache);
        view.field("oracle_source_watermarks", &self.oracle_source_watermarks);
        view.field("oracle_source_initialized", &self.oracle_source_initialized);
        view.field("returned_position_origin", &self.returned_position_origin);
        view.field(
            "collateral_position_origin",
            &self.collateral_position_origin,
        );
        view.field("position_identity_active", &self.position_identity_active);
        view.field(
            "source_send_tally_instance_initialized",
            &self.source_send_tally_instance_initialized,
        );
        view.field(
            "source_send_tally_read_job",
            &self.source_send_tally_read_job,
        );
        view.field("activation_client_profile", &self.activation_client_profile);
        view.field("activation_client_locked", &self.activation_client_locked);
        view.field("activation_vesting", &self.activation_vesting);
        view.field(
            "activation_consensus_vector",
            &self.activation_consensus_vector,
        );
        view.field("activation_native_errors", &self.activation_native_errors);
        view.field("activation_insurance_job", &self.activation_insurance_job);
        view.field("activation_clearlist_jobs", &self.activation_clearlist_jobs);
        view.field("activation_async_active", &self.activation_async_active);
        view.field("activation_async_clock", &self.activation_async_clock);
        view.field("activation_async_serial", &self.activation_async_serial);
        view.field("activation_async_running", &self.activation_async_running);
        view.field(
            "activation_async_child_depth",
            &self.activation_async_child_depth,
        );
        view.field("main_activation", &self.main_activation);
        view.field("main_activation_profile", &self.main_activation_profile);
        view.field("main_activation_locked", &self.main_activation_locked);
        view.field("main_height_job", &self.main_height_job);
        // Historical profiles have no production owner. Unexpected data
        // remains visible instead of being hidden by the compatibility view.
        if self.main_production_enabled() || self.main_production.is_some() {
            view.field("main_production", &self.main_production);
        }
        if let Some(profile) = self.native_active_service_client_scope {
            view.field("native_active_service_client_scope", &profile);
        }
        if let Some(binding) = &self.native_main_binding {
            view.field("native_main_binding", binding);
        }
        if self.policy == SourceContractRuntimePolicy::RichThrownV14 || self.source_rich.is_some() {
            view.field("source_rich", &self.source_rich);
        }
        view.field("main_height_rejections", &self.main_height_rejections);
        view.field("source_spot_main_active", &self.source_spot_main_active);
        view.field("source_spot_orders_read", &self.source_spot_orders_read);
        view.field(
            "source_spot_fee_buffer_initialized",
            &self.source_spot_fee_buffer_initialized,
        );
        view.finish()
    }
}

impl Default for SourceContractRuntime {
    fn default() -> Self {
        Self::new()
    }
}
impl SourceContractRuntime {
    pub fn new() -> Self {
        Self::with_policy(SourceContractRuntimePolicy::GlobalClearingV2)
    }
    pub fn with_policy(policy: SourceContractRuntimePolicy) -> Self {
        Self {
            queue: ContractQueueSourceOwner::new(),
            core: ContractCore::new(policy),
            last_parameters: None,
        }
    }
    pub fn policy(&self) -> SourceContractRuntimePolicy {
        self.core.policy
    }
    pub fn last_contract_parameters(&self) -> Option<&V> {
        self.last_parameters.as_ref()
    }
    /// Import complete persisted source collections, not supplied rule outcomes.
    pub fn import_collection(&mut self, name: &str, rows: Vec<V>) -> Result<(), SourceRuleError> {
        if name == "orderBooks" {
            if self.core.amm_lifecycle_enabled()
                && self.core.tables.len() >= MAX_TABLES
                && !self.core.tables.iter().any(|table| table.name == name)
            {
                return Err(SourceRuleError::Capacity);
            }
            self.queue.import_rows(rows)?;
            if self.core.amm_lifecycle_enabled() {
                self.core.source_shared_orderbook_slot()?;
            }
            Ok(())
        } else {
            self.core.import(name, rows)
        }
    }
    pub fn collection(&self, name: &str) -> &[V] {
        if name == "orderBooks" {
            self.queue.rows()
        } else {
            self.core.rows(name)
        }
    }
    pub fn set_height(&mut self, height: V) -> Result<(), SourceRuleError> {
        height.validate()?;
        self.core.height = height;
        Ok(())
    }
    pub fn set_activation_answer(&mut self, active: bool) {
        self.core.active = active;
    }
    pub fn set_entry(&mut self, entry: &str) -> Result<(), SourceRuleError> {
        if !matches!(
            entry,
            "main-subgraph" | "types-first" | "tally-first" | "volume-first"
        ) {
            return Err(SourceRuleError::InvalidInput);
        };
        self.core.entry = entry.into();
        Ok(())
    }
    pub fn fail_storage(&mut self, failure: StorageFailure) -> Result<(), SourceRuleError> {
        if failure.collection.len() > 128
            || failure.message.len() > 4096
            || failure.skip > 65_536
            || self.core.failures.len() >= MAX_TABLE_ROWS
        {
            return Err(SourceRuleError::Capacity);
        }
        if failure.operation == "removeAsync" && !self.core.global_clearing_enabled() {
            return Err(SourceRuleError::InvalidInput);
        }
        if !matches!(
            failure.operation.as_str(),
            "getDatabase"
                | "findOneAsync"
                | "findAsync"
                | "removeAsync"
                | "updateAsync"
                | "insertAsync"
                | "loadDatabase"
                | "get"
                | "put"
        ) {
            return Err(SourceRuleError::InvalidInput);
        }
        if failure.collection == "orderBooks"
            && (!self.core.amm_lifecycle_enabled()
                || matches!(
                    failure.operation.as_str(),
                    "getDatabase" | "findOneAsync" | "updateAsync" | "loadDatabase"
                ))
        {
            self.queue.fail_database_call(QueueDbFailure {
                operation: failure.operation,
                skip: failure.skip,
                exception: crate::validity_source::SourceException {
                    name: "Error".into(),
                    message: failure.message,
                },
            })
        } else {
            self.core.failures.push(failure);
            Ok(())
        }
    }
    pub fn queue(&self) -> &ContractQueueSourceOwner {
        &self.queue
    }
    pub fn margins(&self) -> &MarginMapSourceOwner {
        &self.core.margins
    }
    pub fn tally_cache(&self) -> V {
        V::Array(
            self.core
                .tally
                .iter()
                .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                .collect(),
        )
    }
    pub fn take_effects(&mut self) -> Vec<V> {
        std::mem::take(&mut self.core.effects)
    }
    pub fn state_value(&self) -> Result<V, SourceRuleError> {
        let mut value = obj(&[
            (
                "tables",
                V::Array(
                    self.core
                        .tables
                        .iter()
                        .map(|t| {
                            let rows =
                                if self.core.amm_lifecycle_enabled() && t.name == "orderBooks" {
                                    self.core
                                        .queue_context
                                        .as_ref()
                                        .map_or(self.queue.rows(), |queue| queue.rows())
                                } else {
                                    &t.rows
                                };
                            obj(&[("name", s(&t.name)), ("rows", V::Array(rows.to_vec()))])
                        })
                        .collect(),
                ),
            ),
            ("queue", self.queue.state_value()),
            ("margins", self.core.margins.state_value()?),
            ("tally", self.tally_cache()),
            (
                "feeBuffer",
                V::Array(
                    self.core
                        .fee_buffer
                        .iter()
                        .map(|(b, entries)| {
                            V::Array(vec![
                                b.clone(),
                                V::Array(
                                    entries
                                        .iter()
                                        .map(|(k, v)| V::Array(vec![s(k), v.clone()]))
                                        .collect(),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "blockTrades",
                V::Array(
                    self.core
                        .block_trades
                        .iter()
                        .map(|(k, v)| V::Array(vec![s(k), v.clone()]))
                        .collect(),
                ),
            ),
            (
                "oracleMarks",
                V::Array(
                    self.core
                        .oracle_marks
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                ),
            ),
            (
                "nativeMarks",
                V::Array(
                    self.core
                        .native_marks
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                ),
            ),
            ("globals", self.core.globals.clone()),
            ("insurance", self.core.insurance.clone()),
            (
                "oracleSourceCache",
                V::Array(
                    self.core
                        .oracle_source_cache
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                ),
            ),
            (
                "oracleSourceWatermarks",
                V::Array(
                    self.core
                        .oracle_source_watermarks
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                ),
            ),
            (
                "oracleSourceInitialized",
                V::Bool(self.core.oracle_source_initialized),
            ),
            ("sourceOracle", self.core.oracle_source_state()),
            (
                "continuation",
                obj(&[
                    ("uuid", n(self.core.uuid as f64)),
                    ("databaseId", n(self.core.db_id as f64)),
                    ("random", n(f64::from(self.core.random))),
                    ("height", self.core.height.clone()),
                    ("clock", n(self.core.time_millis as f64)),
                    ("contractMod", V::Bool(self.core.contract_mod)),
                    ("entry", s(&self.core.entry)),
                    ("active", V::Bool(self.core.active)),
                    (
                        "lastParameters",
                        self.last_parameters.clone().unwrap_or(V::Undefined),
                    ),
                    (
                        "failures",
                        V::Array(
                            self.core
                                .failures
                                .iter()
                                .map(|f| {
                                    obj(&[
                                        ("collection", s(&f.collection)),
                                        ("operation", s(&f.operation)),
                                        ("skip", n(f.skip as f64)),
                                        ("message", s(&f.message)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    (
                        "queueCheckpoint",
                        s(self
                            .queue
                            .checkpoint()?
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<String>()),
                    ),
                ]),
            ),
        ]);
        if self.core.global_clearing_enabled() {
            set(
                &mut value,
                "runtimePolicy",
                s(
                    if self.core.policy == SourceContractRuntimePolicy::RichThrownV14 {
                        "RichThrownV14"
                    } else if self.core.main_production_enabled() {
                        "MainOrchestrationV12"
                    } else if self.core.amm_lifecycle_enabled() {
                        "AmmLifecycleV11"
                    } else if self.core.activation_lifecycle_enabled() {
                        "ActivationLifecycleV10"
                    } else if self.core.spot_lifecycle_enabled() {
                        "SpotLifecycleV9"
                    } else if self.core.main_callable_enabled() {
                        "MainCallableV8"
                    } else if self.core.lower_lifecycle_enabled() {
                        "LowerLifecycleV7"
                    } else if self.core.proof_lifecycle_enabled() {
                        "ProofLifecycleV6"
                    } else if self.core.distribution_lifecycle_enabled() {
                        "DistributionLifecycleV5"
                    } else if self.core.option_lifecycle_enabled() {
                        "OptionLifecycleV4"
                    } else if self.core.lifecycle_enabled() {
                        "SourceLifecycleV3"
                    } else {
                        "GlobalClearingV2"
                    },
                ),
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
            set(
                &mut value,
                "globalClearing",
                self.core.source_clearing_state(),
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if self.core.lifecycle_enabled() {
            set(
                &mut value,
                "registryLifecycle",
                obj(&[
                    (
                        "registryInitialized",
                        V::Bool(self.core.contract_registry_initialized),
                    ),
                    (
                        "seriesInitialized",
                        V::Bool(self.core.contract_series_initialized),
                    ),
                ]),
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if self.core.option_lifecycle_enabled() {
            set(
                &mut value,
                "syntheticLifecycle",
                self.core.source_synthetic_state(),
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if self.core.proof_lifecycle_enabled() {
            set(
                &mut value,
                "proofLifecycle",
                obj(&[
                    ("environment", self.core.source_proof_environment.clone()),
                    ("artifacts", self.core.source_proof_artifacts.clone()),
                    (
                        "propertyInitialized",
                        V::Bool(self.core.source_proof_property_initialized),
                    ),
                ]),
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if self.core.proof_lifecycle_enabled() {
            set(
                &mut value,
                "liquidityRewards",
                self.core.source_liquidity_rewards_state(),
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if self.core.main_callable_enabled() {
            set(&mut value, "mainCallable", self.core.source_main_state())
                .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if self.core.main_callable_enabled() {
            set(
                &mut value,
                "clearlistLifecycle",
                self.core.source_clearlist_state(),
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if self.core.main_callable_enabled() {
            set(
                &mut value,
                "sendCommit",
                self.core.source_send_commit_state(),
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if self.core.spot_lifecycle_enabled() {
            set(
                &mut value,
                "spotLifecycle",
                self.core.source_spot_schedule_state(),
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if self.core.activation_lifecycle_enabled() {
            set(
                &mut value,
                "activationLifecycle",
                self.core.source_activation_state(),
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if self.core.amm_lifecycle_enabled() {
            set(&mut value, "ammLifecycle", self.core.source_amm_state())
                .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if self.core.main_production_enabled() {
            set(
                &mut value,
                "mainProduction",
                self.core.source_main_production_state()?,
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if let Some(binding) = &self.core.native_main_binding {
            set(&mut value, "nativeMainBinding", binding.clone())
                .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        if self.core.policy == SourceContractRuntimePolicy::RichThrownV14
            || self.core.source_rich.is_some()
        {
            set(
                &mut value,
                "sourceRich",
                self.core.source_rich_state_value()?,
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        Ok(value)
    }
    pub fn invoke_dependency(
        &mut self,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        if self.policy() == SourceContractRuntimePolicy::RichThrownV14 {
            return Err(SourceRuleError::InvalidInput);
        }
        for v in &call.arguments {
            v.validate()?
        }
        if call.target == "SourceMainBootstrap.step"
            && self.policy() == SourceContractRuntimePolicy::MainOrchestrationV12
            && self.core.ordered_bootstrap_present()
        {
            return self.ordered_bootstrap_invoke(call);
        }
        if self.core.ordered_bootstrap_present()
            && (!self.ordered_bootstrap_closed_for_adoption()
                || [
                    "SourceMain.configureColdStartup",
                    "SourceMain.configureProduction",
                    "SourceMainBootstrap.configure",
                ]
                .contains(&call.target.as_str()))
        {
            return Err(SourceRuleError::UnknownDependency);
        }
        if self.core.main_callable_enabled()
            && (call.target.starts_with("Main.")
                || (self.core.main_production_enabled()
                    && self.core.source_main_service_enabled()
                    && (call.target.starts_with("Persistence.")
                        || call.target == "SourceMain.importStartupModules")))
        {
            return self.source_main_invoke(call);
        }
        let before = self.clone();
        let borrowed_spot_main = self.core.spot_lifecycle_enabled()
            && self.core.source_spot_main_active
            && self.core.queue_context.is_some();
        if !borrowed_spot_main {
            self.core.queue_context = Some(std::mem::replace(
                &mut self.queue,
                ContractQueueSourceOwner::new(),
            ));
        }
        let result = self.core.invoke(call);
        let result = if result.is_ok() && self.core.global_clearing_enabled() {
            self.core
                .source_history_drain()
                .map_err(|e| match e {
                    Fault::Local(e) => e,
                    Fault::Thrown(_) => SourceRuleError::Backend,
                })
                .and(result)
        } else {
            result
        };
        if !borrowed_spot_main {
            self.queue = self
                .core
                .queue_context
                .take()
                .ok_or(SourceRuleError::Backend)?;
        }
        let result = if self.core.effects_exhausted {
            Err(SourceRuleError::Capacity)
        } else {
            result
        };
        self.core.source_amm_returned_origin = None;
        self.core.returned_position_origin = None;
        self.core.collateral_position_origin = None;
        self.core.position_identity_active = false;
        if result.is_err() {
            *self = before
        };
        result
    }
    fn source_spot_evaluate_queue_inner(
        &mut self,
        call: &SourceCall,
    ) -> Result<SourceOutcome, SourceRuleError> {
        let before = self.clone();
        let core = &mut self.core;
        let channel = core.lifecycle_enabled()
            && matches!(
                call.method.as_str(),
                "queueChannelTrade" | "processQueuedChannelTradesForBlock"
            );
        let mut capability = |queue: &mut ContractQueueSourceOwner, call: &DependencyCall| {
            core.harvest_queue(queue);
            if core.spot_lifecycle_enabled() && call.target == "Orderbook.addTokenOrder" {
                core.source_spot_add_token_in_queue(queue, &call.arguments)
            } else if call.target == "Orderbook.addContractOrder" {
                let mut backend = PlacementBackend { core };
                match crate::contract_placement_source::add_contract_order(
                    queue,
                    &call.arguments,
                    &mut backend,
                )? {
                    SourceOutcome::Returned(v) => Ok(DependencyReturn::Pending(v)),
                    SourceOutcome::Thrown(e) => Ok(DependencyReturn::PendingThrow(e)),
                }
            } else if core.lifecycle_enabled() && call.target == "Orderbook.processContractMatches"
            {
                let mut backend = FinancialBackend { core, queue };
                returned_source(process_contract_matches_actual(
                    &call.arguments[1..],
                    &mut backend,
                )?)
            } else if core.lifecycle_enabled() && call.target == "Orderbook.processTokenMatches" {
                let mut backend = FinancialBackend { core, queue };
                returned_source(crate::token_settlement_source::process_token_matches(
                    &call.arguments[1..],
                    &mut backend,
                )?)
            } else {
                Err(SourceRuleError::UnknownDependency)
            }
        };
        let result = if channel {
            self.queue
                .evaluate_channel_with_capability(call, &mut capability)
        } else {
            self.queue.evaluate_with_capability(call, &mut capability)
        };
        self.core.harvest_queue(&mut self.queue);
        let result = if result.is_ok() && self.core.global_clearing_enabled() {
            self.core
                .source_history_drain()
                .map_err(|e| match e {
                    Fault::Local(e) => e,
                    Fault::Thrown(_) => SourceRuleError::Backend,
                })
                .and(result)
        } else {
            result
        };
        let result = if self.core.effects_exhausted {
            Err(SourceRuleError::Capacity)
        } else {
            result
        };
        self.core.source_amm_returned_origin = None;
        self.core.returned_position_origin = None;
        self.core.collateral_position_origin = None;
        self.core.position_identity_active = false;
        if result.is_err() {
            *self = before
        };
        result
    }
    /// Actual Types/Validity and literal Logic type18 enqueue. Actor claims and
    /// imported source state are explicit unverified inputs, as in the oracle.
    pub fn execute_contract_unverified(
        &mut self,
        arguments: &[V],
    ) -> Result<SourceOutcome, SourceRuleError> {
        if self.policy() == SourceContractRuntimePolicy::RichThrownV14 {
            return Err(SourceRuleError::InvalidInput);
        }
        for v in arguments {
            v.validate()?
        }
        let before = self.clone();
        let result = self.execute_contract_inner(arguments);
        let result = if self.core.effects_exhausted {
            Err(SourceRuleError::Capacity)
        } else {
            result
        };
        self.core.source_amm_returned_origin = None;
        self.core.returned_position_origin = None;
        self.core.collateral_position_origin = None;
        self.core.position_identity_active = false;
        if result.is_err() {
            *self = before
        };
        result
    }
    fn execute_contract_inner(
        &mut self,
        arguments: &[V],
    ) -> Result<SourceOutcome, SourceRuleError> {
        let result = self.source_main_decode_reused(arguments)?;
        match result {
            crate::types_source::TypesOutcome::Returned(params) => {
                self.last_parameters = Some(params.clone());
                let logic = self.source_main_logic_reused(arguments, &params)?;
                match logic {
                    SourceOutcome::Returned(value) => Ok(SourceOutcome::Returned(obj(&[
                        ("params", params),
                        ("returned", value),
                    ]))),
                    SourceOutcome::Thrown(error) => Ok(SourceOutcome::Thrown(error)),
                }
            }
            crate::types_source::TypesOutcome::Rejected(error) => Ok(SourceOutcome::Thrown(error)),
            crate::types_source::TypesOutcome::ReturnedError(error) => {
                Ok(SourceOutcome::Returned(obj(&[
                    ("name", s(error.name)),
                    ("message", s(error.message)),
                ])))
            }
        }
    }
}
impl ContractCore {
    fn new(policy: SourceContractRuntimePolicy) -> Self {
        Self {
            policy,
            queue_context: None,
            clearing_contexts: vec![],
            deleverage_trades: vec![],
            liquidation_marks: vec![],
            contract_series: vec![],
            contract_series_initialized: false,
            contract_registry_initialized: false,
            channel_inverse_jobs: vec![],
            channel_balance_jobs: vec![],
            channel_balance_stage: 0,
            channel_save_writes: 0,
            channel_token_validation: false,
            source_token_threshold_reads: vec![],
            source_main_withdrawal_save: None,
            channel_sufficiency_reads: vec![],
            channel_token_price_job: None,
            tally_mod: V::Undefined,
            tally_static_initialized: false,
            channels: crate::channels_source::SourceChannels::new(),
            property_cache: vec![],
            source_amm_instances: vec![],
            source_amm_property_index: vec![],
            source_amm_returned_origin: None,
            source_proof_environment: obj(&[]),
            source_proof_artifacts: obj(&[]),
            source_proof_property_initialized: false,
            source_clearlist_lists: vec![],
            source_clearlist_banlist: V::Array(
                ["US", "KP", "SY", "RU", "IR", "CU"]
                    .into_iter()
                    .map(s)
                    .collect(),
            ),
            source_clearlist_xpub_cache: vec![],
            source_clearlist_roots: vec![],
            source_clearlist_library: crate::bip32_public::SourceLibrary::Litecoin,
            source_clearlist_library_locked: false,
            vault_initialized: false,
            vault_cache: vec![],
            synthetic_token_cache: vec![],
            source_unhandled_rejections: vec![],
            source_supply_delta: None,
            source_history_jobs: vec![],
            tables: vec![],
            margins: MarginMapSourceOwner::new(),
            tally: vec![],
            fee_buffer: vec![],
            block_trades: vec![],
            oracle_marks: vec![],
            native_marks: vec![],
            globals: obj(&[]),
            effects: vec![],
            effects_exhausted: false,
            failures: vec![],
            uuid: 0,
            db_id: 0,
            random: 1,
            height: n(10.0),
            time_millis: 1_700_000_000_000,
            contract_mod: false,
            contract_mod_raw: V::Undefined,
            entry: "main-subgraph".into(),
            active: true,
            insurance: obj(&[]),
            oracle_source_cache: vec![],
            oracle_source_watermarks: vec![],
            oracle_source_initialized: false,
            returned_position_origin: None,
            collateral_position_origin: None,
            position_identity_active: false,
            source_send_tally_instance_initialized: false,
            source_send_tally_read_job: None,
            activation_client_profile: V::Null,
            activation_client_locked: false,
            activation_vesting: V::Undefined,
            activation_consensus_vector: vec![],
            activation_native_errors: vec![],
            activation_insurance_job: None,
            activation_clearlist_jobs: vec![],
            activation_async_active: false,
            activation_async_clock: 0,
            activation_async_serial: 0,
            activation_async_running: false,
            activation_async_child_depth: 0,
            main_activation: crate::activation::SourceActivation::new(),
            main_activation_profile: "source-registry".into(),
            main_activation_locked: false,
            main_height_job: None,
            main_production: None,
            source_rich: if policy == SourceContractRuntimePolicy::RichThrownV14 {
                Some(rich_runtime_v14::State::new())
            } else {
                None
            },
            native_main_binding: None,
            native_active_service_client_scope: None,
            main_height_rejections: vec![],
            source_spot_main_active: false,
            source_spot_orders_read: None,
            source_spot_fee_buffer_initialized: false,
        }
    }
    fn global_clearing_enabled(&self) -> bool {
        (matches!(
            self.policy,
            SourceContractRuntimePolicy::GlobalClearingV2
                | SourceContractRuntimePolicy::SourceLifecycleV3
                | SourceContractRuntimePolicy::OptionLifecycleV4
                | SourceContractRuntimePolicy::DistributionLifecycleV5
                | SourceContractRuntimePolicy::ProofLifecycleV6
                | SourceContractRuntimePolicy::LowerLifecycleV7
                | SourceContractRuntimePolicy::MainCallableV8
                | SourceContractRuntimePolicy::SpotLifecycleV9
                | SourceContractRuntimePolicy::ActivationLifecycleV10
        )) || self.amm_lifecycle_enabled()
    }
    fn lifecycle_enabled(&self) -> bool {
        (matches!(
            self.policy,
            SourceContractRuntimePolicy::SourceLifecycleV3
                | SourceContractRuntimePolicy::OptionLifecycleV4
                | SourceContractRuntimePolicy::DistributionLifecycleV5
                | SourceContractRuntimePolicy::ProofLifecycleV6
                | SourceContractRuntimePolicy::LowerLifecycleV7
                | SourceContractRuntimePolicy::MainCallableV8
                | SourceContractRuntimePolicy::SpotLifecycleV9
                | SourceContractRuntimePolicy::ActivationLifecycleV10
        )) || self.amm_lifecycle_enabled()
    }
    fn option_lifecycle_enabled(&self) -> bool {
        (matches!(
            self.policy,
            SourceContractRuntimePolicy::OptionLifecycleV4
                | SourceContractRuntimePolicy::DistributionLifecycleV5
                | SourceContractRuntimePolicy::ProofLifecycleV6
                | SourceContractRuntimePolicy::LowerLifecycleV7
                | SourceContractRuntimePolicy::MainCallableV8
                | SourceContractRuntimePolicy::SpotLifecycleV9
                | SourceContractRuntimePolicy::ActivationLifecycleV10
        )) || self.amm_lifecycle_enabled()
    }
    fn distribution_lifecycle_enabled(&self) -> bool {
        (matches!(
            self.policy,
            SourceContractRuntimePolicy::DistributionLifecycleV5
                | SourceContractRuntimePolicy::ProofLifecycleV6
                | SourceContractRuntimePolicy::LowerLifecycleV7
                | SourceContractRuntimePolicy::MainCallableV8
                | SourceContractRuntimePolicy::SpotLifecycleV9
                | SourceContractRuntimePolicy::ActivationLifecycleV10
        )) || self.amm_lifecycle_enabled()
    }
    fn proof_lifecycle_enabled(&self) -> bool {
        (matches!(
            self.policy,
            SourceContractRuntimePolicy::ProofLifecycleV6
                | SourceContractRuntimePolicy::LowerLifecycleV7
                | SourceContractRuntimePolicy::MainCallableV8
                | SourceContractRuntimePolicy::SpotLifecycleV9
                | SourceContractRuntimePolicy::ActivationLifecycleV10
        )) || self.amm_lifecycle_enabled()
    }
    fn lower_lifecycle_enabled(&self) -> bool {
        (matches!(
            self.policy,
            SourceContractRuntimePolicy::LowerLifecycleV7
                | SourceContractRuntimePolicy::MainCallableV8
                | SourceContractRuntimePolicy::SpotLifecycleV9
                | SourceContractRuntimePolicy::ActivationLifecycleV10
        )) || self.amm_lifecycle_enabled()
    }
    fn main_callable_enabled(&self) -> bool {
        (matches!(
            self.policy,
            SourceContractRuntimePolicy::MainCallableV8
                | SourceContractRuntimePolicy::SpotLifecycleV9
                | SourceContractRuntimePolicy::ActivationLifecycleV10
        )) || self.amm_lifecycle_enabled()
    }
    fn spot_lifecycle_enabled(&self) -> bool {
        (matches!(
            self.policy,
            SourceContractRuntimePolicy::SpotLifecycleV9
                | SourceContractRuntimePolicy::ActivationLifecycleV10
        )) || self.amm_lifecycle_enabled()
    }
    fn source_shared_orderbook_slot(&mut self) -> Result<(), SourceRuleError> {
        if !self.tables.iter().any(|table| table.name == "orderBooks") {
            if self.tables.len() >= MAX_TABLES {
                return Err(SourceRuleError::Capacity);
            }
            // Insertion-order metadata only. The Queue is the sole row store.
            self.tables.push(Table {
                name: "orderBooks".into(),
                rows: vec![],
            });
        }
        Ok(())
    }
    fn rows(&self, name: &str) -> &[V] {
        if self.amm_lifecycle_enabled() && name == "orderBooks" {
            return self
                .queue_context
                .as_ref()
                .map_or(&[], |queue| queue.rows());
        }
        self.tables
            .iter()
            .find(|t| t.name == name)
            .map_or(&[], |t| t.rows.as_slice())
    }
    fn table(&mut self, name: &str) -> R<&mut Vec<V>> {
        if self.amm_lifecycle_enabled() && name == "orderBooks" {
            self.source_shared_orderbook_slot()?;
            return Ok(self
                .queue_context
                .as_mut()
                .ok_or(SourceRuleError::UnknownDependency)?
                .source_storage_rows_mut());
        }
        if let Some(i) = self.tables.iter().position(|t| t.name == name) {
            return Ok(&mut self.tables[i].rows);
        };
        if self.tables.len() >= MAX_TABLES {
            return Err(SourceRuleError::Capacity.into());
        };
        self.tables.push(Table {
            name: name.into(),
            rows: vec![],
        });
        Ok(&mut self.tables.last_mut().unwrap().rows)
    }
    fn import(&mut self, name: &str, rows: Vec<V>) -> Result<(), SourceRuleError> {
        if name == "txIndex" && self.native_main_binding.is_some() {
            return Err(SourceRuleError::Backend);
        }
        if name.len() > 128 || rows.len() > MAX_TABLE_ROWS {
            return Err(SourceRuleError::Capacity);
        };
        for row in &rows {
            row.validate()?;
            if !matches!(row, V::Object(_)) {
                return Err(SourceRuleError::InvalidInput);
            }
        }
        self.table(name)
            .map_err(|_| SourceRuleError::Capacity)?
            .clone_from(&rows);
        Ok(())
    }
    fn emit(&mut self, event: V) {
        if self.effects.len() < 65_536 {
            self.effects.push(event)
        } else {
            self.effects_exhausted = true
        }
    }
    fn check_failure(&mut self, collection: &str, operation: &str) -> R<()> {
        if self.amm_lifecycle_enabled() && collection == "orderBooks" {
            if let Some(error) = self
                .queue_context
                .as_mut()
                .ok_or(SourceRuleError::UnknownDependency)?
                .source_storage_failure(operation)
            {
                return Err(Fault::Thrown(error));
            }
        }
        if let Some(i) = self
            .failures
            .iter()
            .position(|f| f.collection == collection && f.operation == operation)
        {
            if self.failures[i].skip > 0 {
                self.failures[i].skip -= 1
            } else {
                return Err(error(self.failures.remove(i).message));
            }
        };
        Ok(())
    }
    fn database(&mut self, name: &str) -> R<V> {
        if !self.activation_lifecycle_enabled() {
            self.table(name)?;
        }
        let result = self
            .check_failure(name, "getDatabase")
            .map(|()| obj(&[("sourceDatabase", s(name))]));
        let mut event = obj(&[
            ("kind", s("database")),
            ("name", s(name)),
            ("method", s("getDatabase")),
            ("args", V::Array(vec![s(name)])),
        ]);
        match &result {
            Ok(_) => set(&mut event, "returned", obj(&[("database", s(name))]))?,
            Err(Fault::Thrown(e)) => set(
                &mut event,
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )?,
            _ => (),
        };
        self.emit(event);
        if self.activation_lifecycle_enabled() {
            self.source_activation_after_database(name)?;
        }
        self.source_main_withdrawal_after_database(name)?;
        self.source_token_threshold_after_database(name)?;
        self.source_transfer_balance_after_await(name)?;
        if self.spot_lifecycle_enabled() && self.source_spot_main_active {
            self.source_spot_orders_read_after_database()?;
        }
        if self.main_callable_enabled() && name == "tallyMap" {
            self.source_send_read_after_database()?;
        }
        if self.global_clearing_enabled() {
            self.source_history_after_database(name)?;
            self.source_supply_after_database(name)?;
        }
        if self.lifecycle_enabled() && name == "marginMaps" {
            self.source_channel_finish_inverse()?;
        }
        if self.lifecycle_enabled() && name == "propertyList" {
            self.source_channel_sufficiency_drain()?;
        }
        if self.lifecycle_enabled() && name == "channels" && !self.channel_balance_jobs.is_empty() {
            self.source_channel_balance_capture()?;
        }
        if self.lifecycle_enabled() && name == "volumeIndex" {
            self.source_channel_price_after_database()?;
        }
        if self.main_callable_enabled() {
            self.source_main_after_database()?;
        }
        result
    }
    fn matches(&self, row: &V, query: &V) -> R<bool> {
        if self.global_clearing_enabled() && matches!(query, V::Undefined) {
            return Ok(true);
        }
        let V::Object(fields) = query else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        for (k, wanted) in fields {
            if k == "$or" {
                let mut found = false;
                for q in array(wanted)? {
                    if self.matches(row, &q)? {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Ok(false);
                };
                continue;
            };
            let actual = get_path(row, k);
            let matched = if let V::Object(ops) = wanted {
                let mut yes = true;
                for (op, v) in ops {
                    yes &= match op.as_str() {
                        "$ne" => !same(&actual, v),
                        "$exists" => matches!(actual, V::Undefined) != truth(v),
                        "$in" => array(v)?.iter().any(|v| same(&actual, v)),
                        "$lt" | "$lte" | "$gt" | "$gte" if self.proof_lifecycle_enabled() => {
                            let compared =
                                crate::oracle_source_owner::source_comparison_unverified(
                                    &actual, v,
                                )
                                .map_err(Fault::Local)?;
                            use core::cmp::Ordering::{Equal, Greater, Less};
                            match op.as_str() {
                                "$lt" => compared == Some(Less),
                                "$lte" => matches!(compared, Some(Less | Equal)),
                                "$gt" => compared == Some(Greater),
                                _ => matches!(compared, Some(Greater | Equal)),
                            }
                        }
                        "$gte" => number(&actual)? >= number(v)?,
                        "$lte" => number(&actual)? <= number(v)?,
                        "$gt" => number(&actual)? > number(v)?,
                        "$lt" => number(&actual)? < number(v)?,
                        "$regex" if self.global_clearing_enabled() => {
                            let pattern = text(v)?;
                            let Some(prefix) = pattern.strip_prefix('^') else {
                                return Err(SourceRuleError::UnknownDependency.into());
                            };
                            if prefix.chars().any(|c| "\\.[](){}*+?$|^".contains(c)) {
                                return Err(SourceRuleError::UnknownDependency.into());
                            }
                            matches!(&actual,V::String(value) if value.starts_with(prefix))
                        }
                        _ => return Err(SourceRuleError::UnknownDependency.into()),
                    }
                }
                yes
            } else {
                same(&actual, wanted)
            };
            if !matched {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn storage_clone(&self, v: &V) -> R<V> {
        if self.global_clearing_enabled() {
            source_storage_clone(v)
        } else {
            Ok(v.clone())
        }
    }
    fn db(&mut self, name: &str, op: &str, args: Vec<V>) -> R<V> {
        if name == "txIndex" && self.native_main_binding.is_some() {
            return Err(SourceRuleError::Backend.into());
        }
        let result = (|| {
            self.check_failure(name, op)?;
            // The source fixture adapter calls table(name) inside these
            // operations, after its injected-failure check. A factory handle
            // or failed operation alone does not create persisted rows.
            if self.activation_lifecycle_enabled()
                && matches!(
                    op,
                    "findOneAsync"
                        | "findAsync"
                        | "get"
                        | "put"
                        | "removeAsync"
                        | "insertAsync"
                        | "updateAsync"
                )
            {
                self.table(name)?;
            }
            match op {
                "findOneAsync" => {
                    for row in self.rows(name) {
                        if self.matches(row, &arg(&args, 0))? {
                            return Ok(row.clone());
                        }
                    }
                    Ok(V::Null)
                }
                "findAsync" => {
                    let mut found = vec![];
                    for row in self.rows(name) {
                        if self.matches(row, &arg(&args, 0))? {
                            found.push(row.clone())
                        }
                    }
                    Ok(V::Array(found))
                }
                "get" => Ok(self
                    .rows(name)
                    .iter()
                    .find(|r| same(&r.get("_id"), &arg(&args, 0)))
                    .cloned()
                    .unwrap_or(V::Null)),
                "put" => {
                    let key = arg(&args, 0);
                    let mut doc = obj(&[("_id", key.clone())]);
                    let V::Object(fields) = self.storage_clone(&arg(&args, 1))? else {
                        return Err(SourceRuleError::InvalidInput.into());
                    };
                    for (k, v) in fields {
                        set(&mut doc, &k, v)?
                    }
                    let rows = self.table(name)?;
                    if let Some(i) = rows.iter().position(|r| same(&r.get("_id"), &key)) {
                        rows[i] = doc
                    } else {
                        if rows.len() >= MAX_TABLE_ROWS {
                            return Err(SourceRuleError::Capacity.into());
                        };
                        rows.push(doc)
                    };
                    Ok(V::Undefined)
                }
                "loadDatabase" => Ok(V::Undefined),
                "removeAsync" if self.global_clearing_enabled() => {
                    let query = arg(&args, 0);
                    let multi = truth(&arg(&args, 1).get("multi"));
                    let mut removed = 0;
                    let mut keep = Vec::new();
                    for row in self.rows(name) {
                        if (multi || removed == 0) && self.matches(row, &query)? {
                            removed += 1;
                        } else {
                            keep.push(row.clone());
                        }
                    }
                    *self.table(name)? = keep;
                    Ok(n(f64::from(removed)))
                }
                "insertAsync" => {
                    let mut doc = self.storage_clone(&arg(&args, 0))?;
                    if matches!(doc.get("_id"), V::Undefined) {
                        self.db_id += 1;
                        set(&mut doc, "_id", s(format!("db-{}", self.db_id)))?
                    };
                    let rows = self.table(name)?;
                    if rows.iter().any(|r| same(&r.get("_id"), &doc.get("_id"))) {
                        return Err(error("uniqueViolated"));
                    };
                    if rows.len() >= MAX_TABLE_ROWS {
                        return Err(SourceRuleError::Capacity.into());
                    };
                    rows.push(doc.clone());
                    Ok(doc)
                }
                "updateAsync" => {
                    let query = arg(&args, 0);
                    let update = arg(&args, 1);
                    let options = arg(&args, 2);
                    let mut indexes = vec![];
                    for (i, row) in self.rows(name).iter().enumerate() {
                        if self.matches(row, &query)? {
                            indexes.push(i)
                        }
                    }
                    if !truth(&options.get("multi")) {
                        indexes.truncate(1)
                    };
                    if indexes.is_empty() && truth(&options.get("upsert")) {
                        let mut doc = self.storage_clone(&query)?;
                        if matches!(doc.get("_id"), V::Undefined) {
                            self.db_id += 1;
                            set(&mut doc, "_id", s(format!("db-{}", self.db_id)))?
                        };
                        let rows = self.table(name)?;
                        if rows.len() >= MAX_TABLE_ROWS {
                            return Err(SourceRuleError::Capacity.into());
                        };
                        indexes.push(rows.len());
                        rows.push(doc)
                    };
                    for i in &indexes {
                        let old = self.rows(name)[*i].clone();
                        let V::Object(fields) = &update else {
                            return Err(SourceRuleError::InvalidInput.into());
                        };
                        let mut row = if fields.iter().any(|(k, _)| k.starts_with('$')) {
                            let mut row = old.clone();
                            for (op, values) in fields {
                                let V::Object(values) = values else {
                                    return Err(SourceRuleError::InvalidInput.into());
                                };
                                for (k, v) in values {
                                    let next = match op.as_str() {
                                        "$set" => self.storage_clone(v)?,
                                        "$inc" => n(number(&nullish(get_path(&row, k), n(0.0)))?
                                            + number(v)?),
                                        "$push" if self.lifecycle_enabled() => {
                                            let mut entries = match get_path(&row, k) {
                                                V::Undefined => vec![],
                                                V::Array(entries) => entries,
                                                _ => return Err(error(
                                                    "Can't $push an element on non-array values",
                                                )),
                                            };
                                            if entries.len() >= MAX_TABLE_ROWS {
                                                return Err(SourceRuleError::Capacity.into());
                                            }
                                            entries.push(self.storage_clone(v)?);
                                            V::Array(entries)
                                        }
                                        _ => return Err(SourceRuleError::UnknownDependency.into()),
                                    };
                                    set_path(&mut row, k, next)?
                                }
                            }
                            row
                        } else {
                            self.storage_clone(&update)?
                        };
                        if matches!(row.get("_id"), V::Undefined)
                            && (!self.global_clearing_enabled()
                                || !matches!(old.get("_id"), V::Undefined))
                        {
                            set(&mut row, "_id", old.get("_id"))?
                        };
                        self.table(name)?[*i] = row
                    }
                    Ok(n(indexes.len() as f64))
                }
                _ => Err(SourceRuleError::UnknownDependency.into()),
            }
        })();
        let result = if self.amm_lifecycle_enabled()
            && name == "orderBooks"
            && matches!(&result, Ok(_) | Err(Fault::Thrown(_)))
        {
            self.queue_context
                .as_ref()
                .ok_or(SourceRuleError::UnknownDependency)?
                .source_storage_validate()?;
            result
        } else {
            result
        };
        let result = result.and_then(|v| self.storage_clone(&v));
        let short = op.strip_suffix("Async").unwrap_or(op);
        let mut event = obj(&[
            ("kind", s("db")),
            ("collection", s(name)),
            ("op", s(short)),
            ("method", s(op)),
            ("args", self.storage_clone(&V::Array(args))?),
        ]);
        match &result {
            Ok(v) => set(&mut event, "returned", v.clone())?,
            Err(Fault::Thrown(e)) => set(
                &mut event,
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )?,
            _ => (),
        };
        self.emit(event);
        if self.activation_lifecycle_enabled() {
            self.source_activation_after_db(name, op)?;
        }
        self.source_token_threshold_after_read(name)?;
        self.source_transfer_balance_after_await(name)?;
        if self.global_clearing_enabled() {
            self.source_supply_after_db(name, op)?;
        }
        if self.lifecycle_enabled()
            && name == "channels"
            && op == "updateAsync"
            && !self.channel_balance_jobs.is_empty()
            // Main's un-awaited Tally jobs progress across the awaited channel
            // write even when that write rejects. Older profiles stay exact.
            && (result.is_ok()
                || (self.spot_lifecycle_enabled()
                    && self.source_spot_main_active
                    && matches!(&result, Err(Fault::Thrown(_)))))
        {
            self.channel_save_writes += 1;
            if self.channel_save_writes == 1 {
                self.source_channel_balance_first_write()?;
            } else {
                self.source_channel_balance_tick()?;
            }
        }
        result
    }
    fn find(&mut self, name: &str, query: V) -> R<V> {
        self.db(name, "findOneAsync", vec![query])
    }
    fn update(&mut self, name: &str, query: V, update: V, upsert: bool) -> R<V> {
        self.db(
            name,
            "updateAsync",
            vec![query, update, obj(&[("upsert", V::Bool(upsert))])],
        )
    }
    fn call(&mut self, target: &str, args: Vec<V>) -> R<V> {
        awaited(self.invoke(&DependencyCall {
            target: target.into(),
            arguments: args,
        })?)
    }
    fn new_uuid(&mut self) -> String {
        self.uuid += 1;
        let id = format!("source-contract-{}", self.uuid);
        self.emit(obj(&[("kind", s("uuid")), ("value", s(&id))]));
        id
    }
    fn margin_call(&mut self, method: &str, args: &[V]) -> R<V> {
        if !self.option_lifecycle_enabled() && method == "_touchExpiryIndex" {
            return Err(SourceRuleError::UnknownMethod.into());
        }
        if self.option_lifecycle_enabled() && method == "applyOptionTrade" {
            return self.source_option_margin_apply(args);
        }
        if self.option_lifecycle_enabled() && method == "_touchExpiryIndex" {
            return self.source_option_touch(args);
        }
        let handle = arg(args, 0);
        let handle_id = handle.get("sourceMarginInstance");
        let (instance, original) = if matches!(handle_id, V::Number(_)) {
            (Some(number(&handle_id)? as u64), &args[1..])
        } else {
            (None, args)
        };
        let aliases = if let V::Array(entries) = handle.get("sourcePositionAliases") {
            entries
                .iter()
                .map(|v| {
                    Ok(PositionAlias {
                        argument: number(&v.get("argument"))? as usize,
                        instance: instance.ok_or(SourceRuleError::InvalidInput)?,
                        address: v.get("address"),
                    })
                })
                .collect::<R<Vec<_>>>()?
        } else {
            vec![]
        };
        if method == "inspectPosition" {
            return self
                .margins
                .inspect_position(
                    instance.ok_or(SourceRuleError::InvalidInput)?,
                    &arg(original, 0),
                )
                .map_err(Fault::Local);
        };
        let mut object_aliases = if let V::Array(entries) = if self.global_clearing_enabled() {
            handle.get("sourcePositionObjectAliases")
        } else {
            V::Undefined
        } {
            entries
                .iter()
                .map(|v| {
                    Ok(PositionObjectAlias {
                        argument: number(&v.get("argument"))? as usize,
                        index: if matches!(v.get("index"), V::Undefined) {
                            None
                        } else {
                            Some(number(&v.get("index"))? as usize)
                        },
                        object: number(&v.get("object"))? as usize,
                    })
                })
                .collect::<R<Vec<_>>>()?
        } else {
            vec![]
        };
        if self.lifecycle_enabled() && method == "setInitialMargin" {
            if let Some(object) = self.collateral_position_origin.take() {
                object_aliases.push(PositionObjectAlias {
                    argument: 4,
                    index: None,
                    object,
                });
            }
        }
        let field_aliases = if self.lifecycle_enabled() {
            self.source_margin_field_aliases(&handle)?
        } else {
            vec![]
        };
        let mut owner = std::mem::replace(&mut self.margins, MarginMapSourceOwner::new());
        let result = if self.lifecycle_enabled() {
            owner
                .evaluate_detailed_with_position_identity(
                    instance,
                    &SourceCall {
                        method: method.into(),
                        arguments: original.to_vec(),
                    },
                    &aliases,
                    &[],
                    &object_aliases,
                    &field_aliases,
                    self.position_identity_active,
                    self,
                )
                .map(|v| {
                    self.returned_position_origin = v.returned_position;
                    v.evaluation.outcome
                })
        } else {
            owner
                .evaluate_detailed_with_object_aliases(
                    instance,
                    &SourceCall {
                        method: method.into(),
                        arguments: original.to_vec(),
                    },
                    &aliases,
                    &[],
                    &object_aliases,
                    self,
                )
                .map(|v| v.outcome)
        };
        self.margins = owner;
        outcome(result?)
    }
    fn dispatch(&mut self, call: &DependencyCall) -> R<V> {
        let a = &call.arguments;
        let target = call.target.as_str();
        if self.amm_lifecycle_enabled() {
            match target {
                "SourceGlobal.bindPropertyRegistry" => {
                    if !a.is_empty() {
                        return Err(SourceRuleError::InvalidInput.into());
                    }
                    self.globals
                        .insert("sourcePropertyRegistry", V::Bool(true))?;
                    return Ok(V::Undefined);
                }
                "SourceAMM.configureStatic" => return self.source_amm_configure_statics(a),
                "SourceAMM.bindStaticTally" => {
                    if !a.is_empty() {
                        return Err(SourceRuleError::InvalidInput.into());
                    }
                    self.globals.insert("sourceAmmStaticTally", V::Bool(true))?;
                    return Ok(V::Undefined);
                }
                "SourceAMM.construct" => return self.source_amm_construct(a),
                "SourceAMM.attachPropertyPool" => return self.source_amm_bind_property_input(a),
                "Property.getAMM" => return self.source_amm_property_get(a),
                "Contract.getAMM" => return self.source_amm_contract_get(a),
                "Logic.AMMPool" => return self.source_amm_logic(a),
                _ => (),
            }
            if let Some(method) = target.strip_prefix("AMMStatic.") {
                return self.source_amm_static_call(method, a);
            }
            if let Some(method) = target.strip_prefix("AMM.") {
                return self.source_amm_instance_call(method, a);
            }
        }
        if self.activation_lifecycle_enabled() {
            if target == "SourceMain.configure"
                && arg(a, 0).get("activationProfile") != s("source-registry")
            {
                return Err(SourceRuleError::InvalidInput.into());
            }
            match target {
                "SourceActivation.configureClient" => return self.source_activation_configure(a),
                "Logic.activateTradeLayer" => return self.source_activation_logic(a),
                "Tally.checkInitializationFlag" => {
                    return self.source_activation_flags("checkInitializationFlag")
                }
                "Tally.setInitializationFlag" => {
                    return self.source_activation_flags("setInitializationFlag")
                }
                "Vesting.getInstance" => return self.source_activation_vesting_instance(a),
                "Vesting.initializeTokens" => {
                    if self.activation_vesting == V::Undefined {
                        return Err(SourceRuleError::InvalidInput.into());
                    }
                    return self.source_activation_initialize(arg(a, 0));
                }
                "Consensus.pushLatestActivationToConsensusVector" => {
                    return self.source_activation_consensus_push()
                }
                "Consensus.hashFiles" => return self.source_activation_code_hash(),
                "Consensus.stateConsensusHash" => {
                    if !a.is_empty() {
                        return Err(SourceRuleError::UnknownDependency.into());
                    }
                    return self.source_activation_consensus_state_hash();
                }
                _ => (),
            }
        }
        if self.spot_lifecycle_enabled() {
            if target == "SourceGlobal.bindSpotRpcDocuments" {
                if !a.is_empty() {
                    return Err(SourceRuleError::InvalidInput.into());
                }
                self.globals
                    .insert("sourceSpotRpcDocuments", V::Bool(true))?;
                return Ok(V::Undefined);
            }
            if matches!(
                target,
                "TxUtils.getRawTransaction" | "TxUtils.getBlockHeight"
            ) && self.globals.get("sourceSpotRpcDocuments") == V::Bool(true)
            {
                return self.source_spot_rpc(target.strip_prefix("TxUtils.").unwrap(), a);
            }
            if let Some(method) = target.strip_prefix("Logic.") {
                if ["onChainTokenToToken", "cancelOrder"].contains(&method) {
                    return self.source_spot_logic(method, a);
                }
            }
            if let Some(method) = target.strip_prefix("Orderbook.") {
                if [
                    "saveOrderBook",
                    "addTokenOrder",
                    "calculatePrice",
                    "normalizeOrderBookKey",
                    "matchTokenOrders",
                    "getOrdersForAddress",
                    "cancelOrdersByCriteria",
                    "cancelAllContractOrders",
                    "cancelContractOrderByTxid",
                    "cancelContractBuyOrdersByPrice",
                    "cancelAllTokenOrders",
                    "cancelTokenOrderByTxid",
                    "cancelTokenBuyOrdersByPrice",
                    "cancelTokenSellOrdersByPrice",
                ]
                .contains(&method)
                {
                    return self.source_spot_orderbook(method, a);
                }
            }
        }
        if self.main_callable_enabled() {
            if ["Tally.getTotalForProperty", "Tally.getTotalTally"].contains(&target) {
                self.source_send_tally_instance_initialized = true;
            }
            if let Some(method) = target.strip_prefix("Tally.") {
                if [
                    "getInstance",
                    "getAddressBalances",
                    "instanceGetAddressBalances",
                ]
                .contains(&method)
                {
                    return self.source_send_tally(method, a);
                }
            }
            if let Some(method) = target.strip_prefix("Logic.") {
                if [
                    "sendToken",
                    "sendSingle",
                    "sendAll",
                    "processSend",
                    "vestingSend",
                    "calculateVestingMovement",
                    "roundToEightDecimals",
                    "commitToken",
                ]
                .contains(&method)
                {
                    return self.source_send_commit_logic(method, a);
                }
            }
        }
        if self.main_callable_enabled() {
            if target == "SourceClearList.configureLibrary" {
                return self.source_clearlist_configure_library(a);
            }
            if let Some(method) = target.strip_prefix("ClearList.") {
                return self.source_clearlist_call(method, a);
            }
            if let Some(method) = target.strip_prefix("Logic.") {
                if [
                    "createClearList",
                    "issueOrRevokeAttestation",
                    "updateBannedCountries",
                ]
                .contains(&method)
                {
                    return self.source_clearlist_logic(method, a);
                }
            }
        }
        if self.policy == SourceContractRuntimePolicy::MainOrchestrationV12
            && target == "SourceMainBootstrap.configure"
        {
            return self.ordered_bootstrap_configure(a);
        }
        if self.main_production_enabled() && target == "SourceMain.configureColdStartup" {
            return self.source_main_startup_configure();
        }
        if self.main_production_enabled() && target == "SourceMain.configureService" {
            return self.source_main_service_configure(a);
        }
        if self.main_production_enabled() && target == "SourceMain.configureProduction" {
            return self.source_main_production_configure(a);
        }
        if self.main_callable_enabled() {
            if target == "SourceMain.configure" {
                return self.source_main_configure(a);
            }
            if let Some(method) = target.strip_prefix("Activation.") {
                self.main_activation_locked = true;
                if self.main_activation_profile == "source-registry" {
                    return self.source_main_activation(method, a);
                }
            }
        }
        if self.lower_lifecycle_enabled() {
            if let Some(method) = target.strip_prefix("Oracle.") {
                if [
                    "getInstance",
                    "getNextId",
                    "createOracle",
                    "isAdmin",
                    "updateAdmin",
                    "closeOracle",
                ]
                .contains(&method)
                {
                    return self.source_lower_oracle(method, a);
                }
            }
            if let Some(method) = target.strip_prefix("Logic.") {
                if [
                    "createOracle",
                    "publishOracleData",
                    "closeOracle",
                    "updateAdmin",
                ]
                .contains(&method)
                {
                    return self.source_lower_oracle_logic(method, a);
                }
            }
        }
        if self.lower_lifecycle_enabled() {
            if let Some(method) = target.strip_prefix("Property.") {
                if [
                    "createToken",
                    "getNextPropertyId",
                    "addProperty",
                    "isManagedAndAdmin",
                    "doesTickerExist",
                ]
                .contains(&method)
                {
                    return self.source_lower_property(method, a);
                }
            }
            if let Some(method) = target.strip_prefix("Logic.") {
                if ["tokenIssue", "grantManagedToken", "redeemManagedToken"].contains(&method) {
                    return self.source_lower_property_logic(method, a);
                }
            }
        }
        if self.proof_lifecycle_enabled() {
            if target == "Logic.batchMoveZkRollup" {
                return self.source_tail_batch(a);
            }
            if target == "VolumeIndex.calculateLiquidityReward" {
                return self.source_liquidity_calculate_reward(a);
            }
            if target == "VolumeIndex.getCumulativeVolumes" {
                return self.source_liquidity_cumulative_volumes(arg(a, 0));
            }
            if let Some(method) = target.strip_prefix("Logic.") {
                if [
                    "queueLiquidityReward",
                    "settleLiquidityRewards",
                    "tradeTokenForUTXO",
                ]
                .contains(&method)
                {
                    return self.source_liquidity_rewards_logic(method, a);
                }
            }
            if target == "SourceEnvironment.configure" {
                return self.source_proof_environment_configure(a);
            }
            if let Some(method) = target.strip_prefix("Procedural.") {
                return self.source_procedural(method, a);
            }
            if let Some(method) = target.strip_prefix("BitvmCache.") {
                return self.source_bitvm_cache(method, a);
            }
            if let Some(method) = target.strip_prefix("BitvmRisk.") {
                return self.source_bitvm_risk(method, a);
            }
            if target == "SourceArtifacts.configure" {
                return self.source_proof_artifacts_configure(a);
            }
            if let Some(method) = target.strip_prefix("Property.") {
                if [
                    "getInstance",
                    "getPropertyIndex",
                    "load",
                    "save",
                    "grantTokens",
                    "redeemTokens",
                ]
                .contains(&method)
                {
                    return self.source_proof_property(method, a);
                }
            }
            if let Some(method) = target.strip_prefix("Logic.") {
                if ["processStakeFraudProof", "applyProceduralOracleSettlement"].contains(&method) {
                    return self.source_proof_logic(method, a);
                }
            }
            if let Some(method) = target.strip_prefix("Oracle.") {
                if [
                    "recordStake",
                    "getStake",
                    "applyFraudProof",
                    "relayTradeLayerState",
                ]
                .contains(&method)
                {
                    return self.source_proof_oracle(method, a);
                }
            }
            if let Some(method) = target.strip_prefix("Dlc.") {
                if ["parseRelayBlob", "validateRelayBundle"].contains(&method) {
                    return self.source_proof_dlc(method, a);
                }
            }
        }
        if self.distribution_lifecycle_enabled() {
            if let Some(method) = target.strip_prefix("Logic.") {
                if ["payToTokens", "tradeBaiUrbun", "tradeMurabaha"].contains(&method) {
                    return self.source_distribution_logic(method, a);
                }
            }
        }
        if self.option_lifecycle_enabled() {
            if let Some(method) = target.strip_prefix("Logic.") {
                if [
                    "exerciseDerivative",
                    "deliverContract",
                    "handleOptionExercise",
                ]
                .contains(&method)
                {
                    return self.source_derivative_logic(method, a);
                }
            }
            match target {
                "Logic.processOptionTrade" => return self.source_option_logic(a),
                "Validity.validateOptionTrade" => return self.source_option_validate(a),
                "Contract.getInitialMargin" => {
                    return self.source_synthetic_initial_margin(arg(a, 0), arg(a, 1))
                }
                "Synthetic.exists" => return self.source_synthetic_vault("exists", a),
                _ => {}
            }
            if let Some(method) = target.strip_prefix("Logic.") {
                if ["mintSynthetic", "redeemSynthetic"].contains(&method) {
                    return self.source_synthetic_logic(method, a);
                }
            }
            if let Some(method) = target.strip_prefix("Vaults.") {
                return self.source_synthetic_vault(method, a);
            }
            if let Some(method) = target.strip_prefix("Property.") {
                if ["getInstance", "addProperty", "updateTotalInCirculation"].contains(&method) {
                    return self.source_synthetic_property(method, a);
                }
            }
            if let Some(method) = target.strip_prefix("Margin.") {
                if [
                    "moveMarginAndContractsForMint",
                    "moveMarginAndContractsForRedeem",
                ]
                .contains(&method)
                {
                    return self.source_synthetic_margin(method, a);
                }
            }
        }
        if self.channel_token_validation && target == "Tally.hasSufficientChannel" {
            return self.source_channel_sufficiency_begin();
        }
        if let Some(method) = target.strip_prefix("Logic.") {
            if self.lifecycle_enabled() {
                return self.source_lifecycle_logic(method, a);
            }
        }
        if let Some(method) = target.strip_prefix("Scaling.") {
            if self.lifecycle_enabled() {
                return self.source_lifecycle_scaling(method, a);
            }
        }
        if target == "Volume.getLastPrice" && self.lifecycle_enabled() {
            return self.source_registry_last_price(a);
        }
        if let Some(method) = target.strip_prefix("db.") {
            if method == "getDatabase" {
                return self.database(&text(&arg(a, 0))?);
            };
            if a.is_empty() || !matches!(arg(a, 0).get("sourceDatabase"), V::String(_)) {
                return Err(SourceRuleError::InvalidInput.into());
            };
            return self.db(
                &text(&arg(a, 0).get("sourceDatabase"))?,
                method,
                a[1..].to_vec(),
            );
        }
        if let Some(method) = target.strip_prefix("Margin.") {
            if !self.global_clearing_enabled()
                && !crate::margin_map_source_owner::METHODS[..18].contains(&method)
                && method != "inspectPosition"
            {
                return Err(SourceRuleError::UnknownDependency.into());
            }
            return self.margin_call(method, a);
        }
        if let Some(method) = target.strip_prefix("Channels.") {
            if !self.global_clearing_enabled() {
                return Err(SourceRuleError::UnknownDependency.into());
            }
            let mut owner = std::mem::take(&mut self.channels);
            let result = owner.execute_unverified(
                &SourceCall {
                    method: method.into(),
                    arguments: a.clone(),
                },
                &mut SourceChannelsBackend(self),
            );
            self.channels = owner;
            return outcome(result?.outcome);
        }
        if let Some(method) = target.strip_prefix("Property.") {
            if method == "getPropertyData" && self.lifecycle_enabled() {
                return self.source_registry_property_data(a);
            }
            if self.global_clearing_enabled() {
                return self.source_clearing_property(method, a);
            }
        }
        if let Some(method) = target.strip_prefix("Vaults.") {
            if self.global_clearing_enabled() {
                return self.source_clearing_vault_query(method, a);
            }
        }
        if let Some(method) = target.strip_prefix("Insurance.") {
            if !self.global_clearing_enabled() {
                return Err(SourceRuleError::UnknownDependency.into());
            }
            return self.source_insurance_call(method, a);
        }
        if target == "Orderbook.estimateLiquidation" && self.global_clearing_enabled() {
            return self.source_liquidation_estimate(a);
        }
        if target == "Orderbook._pruneInstaLiqOrdersFromFreshBook" && self.global_clearing_enabled()
        {
            return self.source_liquidation_prune(a);
        }
        if let Some(method) = target.strip_prefix("Orderbook.") {
            if self.lifecycle_enabled() && method == "recordTokenTrade" {
                return self.source_channel_record_token_trade(a);
            }
            if self.lifecycle_enabled()
                && matches!(method, "processTokenMatches" | "processTokenChannelTrades")
            {
                let arguments = if matches!(arg(a, 0).get("orderbookInstance"), V::Number(_)) {
                    &a[1..]
                } else {
                    a
                };
                return outcome(if method == "processTokenMatches" {
                    crate::token_settlement_source::process_token_matches(arguments, self)?
                } else {
                    crate::token_settlement_source::process_token_channel_trades(arguments, self)?
                });
            }
            if self.global_clearing_enabled() && method != "recordContractTrade" {
                return self.source_clearing_orderbook_call(method, a);
            }
        }
        if let Some(method) = target.strip_prefix("Tally.") {
            return self.tally_call(method, a);
        }
        if let Some(method) = target.strip_prefix("Contract.") {
            return self.contract_call(method, a);
        }
        if let Some(method) = target.strip_prefix("Clearing.") {
            return self.clearing_call(method, a);
        }
        if let Some(method) = target.strip_prefix("VolumeIndex.") {
            return self.volume_call(method, a);
        }
        if let Some(method) = target.strip_prefix("PnlIou.") {
            return self.iou_call(method, a);
        }
        match target {
            "DB.getDatabase" if self.lifecycle_enabled() => self.database(&text(&arg(a, 0))?),
            "DB.findOneAsync" if self.lifecycle_enabled() => self.find("scaling", arg(a, 0)),
            "SourceGlobal.bindTally" if self.lifecycle_enabled() && a.is_empty() => {
                self.globals
                    .insert("tallyMap", obj(&[("sourceTally", V::Bool(true))]))?;
                Ok(V::Undefined)
            }
            "SourceGlobal.bindRpcTransactions" if self.lifecycle_enabled() && a.is_empty() => {
                self.globals
                    .insert("sourceRpcTransactions", V::Bool(true))?;
                Ok(V::Undefined)
            }
            "TxUtils.getRawTransaction"
            | "TxUtils.getAddressTypeUniversal"
            | "TxUtils.extractPubkeyByType"
                if self.lifecycle_enabled() =>
            {
                self.source_channel_txutils(target, a)
            }
            "uuid.v4" => Ok(s(self.new_uuid())),
            "Clock.nowISOString" => Ok(s("2023-11-14T22:13:20.000Z")),
            "Clock.now" => Ok(n(self.time_millis as f64)),
            "TxUtils.getBlockCount" => {
                self.emit(obj(&[
                    ("kind", s("rpc-clock")),
                    ("method", s("TxUtils.getBlockCount")),
                    ("args", V::Array(a.clone())),
                    ("returned", self.height.clone()),
                ]));
                Ok(self.height.clone())
            }
            "SourceGlobal.get" => {
                let key = text(&arg(a, 0))?;
                let value = self.globals.get(&key);
                if matches!(value, V::Undefined) {
                    Err(Fault::Thrown(crate::validity_source::SourceException {
                        name: "ReferenceError".into(),
                        message: format!("{key} is not defined"),
                    }))
                } else {
                    Ok(value)
                }
            }
            "ClearList.isAddressInClearlist" => self.clearlist_contains(a),
            "Orderbook.recordContractTrade" => self.record_contract_trade(a),
            "TradeHistory.savePNL" => self.save_pnl(a),
            "Oracle.load" => self.oracle_source_load(),
            "Oracle.publishData" => self.oracle_publish(a),
            "Oracle.getOracleInfo" => self.oracle_source_info(arg(a, 0)),
            "Oracle.getOraclePrice" => self.oracle_price(arg(a, 0)),
            "Oracle.getTWAP" if self.global_clearing_enabled() => {
                self.source_funding_oracle_twap(a)
            }
            "Activation.isTxTypeActive" => Ok(V::Bool(self.active)),
            "Activation.getAdmin" => Ok(s("admin")),
            "Activation.areActivationsAboveThreshold" => Ok(V::Bool(false)),
            "ClearList.getBanlist" => {
                if self.lifecycle_enabled() {
                    self.database("clearlists")?;
                    Ok(self
                        .db(
                            "clearlists",
                            "findAsync",
                            vec![obj(&[("_id", s("globalBanlist"))])],
                        )?
                        .get("data"))
                } else {
                    self.database("banlist")?;
                    self.find("banlist", obj(&[]))?;
                    Ok(V::Undefined)
                }
            }
            "ClearList.getCountryCodeByAddress" => {
                self.database("attestations")?;
                let rows = array(&self.db(
                    "attestations",
                    "findAsync",
                    vec![obj(&[("data.address", arg(a, 0))])],
                )?)?;
                for row in rows {
                    let row = row.get("data");
                    if number(&nullish(row.get("listId"), row.get("clearlistId")))? == 0.0
                        && nullish(row.get("status"), s("active")) == s("active")
                    {
                        let data = row.get("data");
                        if let V::String(code) = data {
                            return Ok(obj(&[("countryCode", s(code)), ("address", arg(a, 0))]));
                        }
                    }
                }
                Ok(V::Null)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
impl SourceBackend for ContractCore {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if self.lifecycle_enabled() {
            self.returned_position_origin = None;
        }
        let start = self.effects.len();
        let trace = !matches!(
            call.target.as_str(),
            "Margin.inspectPosition"
                | "SourceGlobal.get"
                | "Orderbook.inspectBook"
                | "Orderbook.setCachedBook"
                | "Orderbook.newInstance"
        ) && !call.target.starts_with("db.");
        if trace {
            self.emit(obj(&[
                ("kind", s("dependency")),
                ("target", s(&call.target)),
                ("args", V::Array(call.arguments.clone())),
            ]))
        }
        let result = self.dispatch(call);
        if trace && start < self.effects.len() {
            match &result {
                Ok(v) => {
                    let _ = set(&mut self.effects[start], "returned", v.clone());
                }
                Err(Fault::Thrown(e)) => {
                    let _ = set(
                        &mut self.effects[start],
                        "thrown",
                        obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                    );
                }
                _ => (),
            }
        }
        if self.main_callable_enabled()
            && (call.target == "SourceMain.configure"
                || self.main_production_enabled()
                    && [
                        "SourceMain.configureProduction",
                        "SourceMain.configureService",
                        "SourceMain.configureColdStartup",
                        "SourceMainBootstrap.configure",
                    ]
                    .contains(&call.target.as_str()))
        {
            return match result {
                Ok(v) => Ok(DependencyReturn::Immediate(v)),
                Err(Fault::Thrown(e)) => Ok(DependencyReturn::Throw(e)),
                Err(Fault::Local(e)) => Err(e),
            };
        }
        if self.main_callable_enabled()
            && self.main_activation_profile == "source-registry"
            && call.target.starts_with("Activation.")
        {
            let sync = matches!(
                call.target.as_str(),
                "Activation.getAdmin"
                    | "Activation.getActivationBlock"
                    | "Activation.initializeTxRegistry"
            );
            return match result {
                Ok(v) => Ok(if sync {
                    DependencyReturn::Immediate(v)
                } else {
                    DependencyReturn::Pending(v)
                }),
                Err(Fault::Thrown(e)) => Ok(if sync {
                    DependencyReturn::Throw(e)
                } else {
                    DependencyReturn::PendingThrow(e)
                }),
                Err(Fault::Local(e)) => Err(e),
            };
        }
        if self.main_callable_enabled()
            && matches!(
                call.target.as_str(),
                "Logic.calculateVestingMovement"
                    | "Logic.roundToEightDecimals"
                    | "Tally.instanceGetAddressBalances"
            )
        {
            return match result {
                Ok(v) => Ok(DependencyReturn::Immediate(v)),
                Err(Fault::Thrown(e)) => Ok(DependencyReturn::Throw(e)),
                Err(Fault::Local(e)) => Err(e),
            };
        }
        if self.spot_lifecycle_enabled()
            && matches!(
                call.target.as_str(),
                "Orderbook.calculatePrice" | "Orderbook.normalizeOrderBookKey"
            )
        {
            return match result {
                Ok(v) => Ok(DependencyReturn::Immediate(v)),
                Err(Fault::Thrown(e)) => Ok(DependencyReturn::Throw(e)),
                Err(Fault::Local(e)) => Err(e),
            };
        }
        if self.amm_lifecycle_enabled()
            && matches!(
                call.target.as_str(),
                "SourceGlobal.bindPropertyRegistry"
                    | "SourceAMM.construct"
                    | "SourceAMM.attachPropertyPool"
            )
        {
            return match result {
                Ok(v) => Ok(DependencyReturn::Immediate(v)),
                Err(Fault::Thrown(e)) => Ok(DependencyReturn::Throw(e)),
                Err(Fault::Local(e)) => Err(e),
            };
        }
        if self.amm_lifecycle_enabled()
            && matches!(
                call.target.as_str(),
                "AMM.quoteContractOrders"
                    | "AMMStatic.calculateOrderSize"
                    | "AMMStatic.calculateRedemptionValue"
                    | "AMMStatic.generateOrders"
            )
        {
            return match result {
                Ok(v) => Ok(DependencyReturn::Immediate(v)),
                Err(Fault::Thrown(e)) => Ok(DependencyReturn::Throw(e)),
                Err(Fault::Local(e)) => Err(e),
            };
        }
        if self.amm_lifecycle_enabled()
            && matches!(
                call.target.as_str(),
                "SourceAMM.configureStatic"
                    | "SourceAMM.bindStaticTally"
                    | "AMMStatic.buildStraddle"
                    | "AMMStatic.buildIronFly"
                    | "AMMStatic.buildCalendar"
                    | "AMMStatic.calculateTotalPosition"
                    | "AMMStatic.getLPAddresses"
                    | "AMMStatic.getAMMOrdersAndPositions"
                    | "AMMStatic.generateOrdersForInverse"
                    | "AMMStatic.generateOrdersForLinear"
            )
        {
            return match result {
                Ok(v) => Ok(DependencyReturn::Immediate(v)),
                Err(Fault::Thrown(e)) => Ok(DependencyReturn::Throw(e)),
                Err(Fault::Local(e)) => Err(e),
            };
        }
        if matches!(
            call.target.as_str(),
            "uuid.v4"
                | "Activation.getAdmin"
                | "Activation.areActivationsAboveThreshold"
                | "Clock.nowISOString"
                | "Clock.now"
        ) || (self.main_callable_enabled()
            && (call.target == "SourceClearList.configureLibrary"
                || matches!(
                    call.target.as_str(),
                    "ClearList.deriveAddressesFromXpub"
                        | "ClearList.registerXpubForRoot"
                        | "ClearList.verifyMerkleProof"
                        | "ClearList.buildMerkleTree"
                        | "ClearList.generateMerkleProof"
                )))
            || (self.lower_lifecycle_enabled() && call.target == "Oracle.getInstance")
            || (self.proof_lifecycle_enabled() && call.target == "Logic.batchMoveZkRollup")
            || (self.proof_lifecycle_enabled() && call.target == "Logic.queueLiquidityReward")
            || (self.proof_lifecycle_enabled()
                && (call.target.starts_with("Dlc.")
                    || matches!(
                        call.target.as_str(),
                        "SourceEnvironment.configure"
                            | "Procedural._normalizeHash"
                            | "BitvmCache._challengeBlocks"
                            | "BitvmCache.buildCacheId"
                    )))
            || (self.proof_lifecycle_enabled() && call.target == "SourceArtifacts.configure")
            || (self.distribution_lifecycle_enabled() && call.target == "Logic.tradeMurabaha")
            || (self.lifecycle_enabled()
                && matches!(
                    call.target.as_str(),
                    "Contract.getInstance"
                        | "Contract.isValidSeriesId"
                        | "Logic.withdrawal"
                        | "TxUtils.getAddressTypeUniversal"
                ))
            || (self.option_lifecycle_enabled()
                && matches!(
                    call.target.as_str(),
                    "Margin._touchExpiryIndex"
                        | "Property.getInstance"
                        | "Logic.exerciseDerivative"
                        | "Logic.deliverContract"
                        | "Logic.handleOptionExercise"
                ))
            || (self.global_clearing_enabled()
                && matches!(
                    call.target.as_str(),
                    "Clearing.clampFundingRate"
                        | "Clearing.calculateClearingPNL"
                        | "Clearing.calculateNewContractPNL"
                        | "Clearing.computeLossCoverage"
                        | "Clearing.initPositionCache"
                        | "Clearing.getPositionsFromCache"
                        | "Clearing.flushPositionCache"
                        | "Clearing.addOrUpdatePositionInCache"
                        | "Clearing.recordDeleverageTrade"
                        | "Clearing.recordLiquidation"
                        | "Clearing.recordTrade"
                        | "Clearing.getLiquidation"
                        | "Clearing.getDeleverageTradesThisBlock"
                        | "Clearing.resetBlockTracking"
                        | "Clearing.resetBlockTrades"
                        | "Margin.generateLiquidationOrder"
                        | "Margin.calcPriceDelta"
                        | "Margin.needsLiquidation"
                        | "Margin.calculateLiquidationPrice"
                        | "Margin.calculateMarginRequirement"
                        | "Margin.estimatePNL"
                        | "PnlIou.key"
                        | "PnlIou.claimKey"
                        | "PnlIou.blockReductionTowardZero"
                        | "Clearing.resolveOptionExerciseTarget"
                        | "Clearing.computeExerciseContracts"
                        | "Clearing.applyAssignedUnderlyingPosition"
                        | "Clearing.computeLiquidationPriceFromLoss"
                ))
        {
            match result {
                Ok(v) => Ok(DependencyReturn::Immediate(v)),
                Err(Fault::Thrown(e)) => Ok(DependencyReturn::Throw(e)),
                Err(Fault::Local(e)) => Err(e),
            }
        } else {
            returned(result)
        }
    }
}
struct PlacementBackend<'a> {
    core: &'a mut ContractCore,
}
impl SourceBackend for PlacementBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        self.core.invoke(call)
    }
}
impl crate::contract_placement_source::ContractPlacementBackend for PlacementBackend<'_> {
    fn invoke_in_queue(
        &mut self,
        queue: &mut ContractQueueSourceOwner,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        self.core.harvest_queue(queue);
        if call.target == "Orderbook.processContractMatches" {
            let mut backend = FinancialBackend {
                core: self.core,
                queue,
            };
            match process_contract_matches_actual(&call.arguments[1..], &mut backend)? {
                SourceOutcome::Returned(v) => Ok(DependencyReturn::Pending(v)),
                SourceOutcome::Thrown(e) => Ok(DependencyReturn::PendingThrow(e)),
            }
        } else {
            self.core.invoke(call)
        }
    }
}
struct FinancialBackend<'a> {
    core: &'a mut ContractCore,
    queue: &'a mut ContractQueueSourceOwner,
}
impl SourceBackend for FinancialBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        self.core.harvest_queue(self.queue);
        if call.target == "TradeHistory.savePNL" && self.core.global_clearing_enabled() {
            return returned(self.core.source_history_begin(&call.arguments));
        }
        if matches!(
            call.target.as_str(),
            "Orderbook.cancelExcessOrders" | "Orderbook.sourceCrossContractReserve"
        ) {
            let mut backend = PlacementBackend { core: self.core };
            let result = if call.target == "Orderbook.cancelExcessOrders" {
                crate::contract_placement_source::cancel_excess_orders(
                    self.queue,
                    &call.arguments,
                    &mut backend,
                )
            } else {
                crate::contract_placement_source::source_cross_contract_reserve(
                    self.queue,
                    &call.arguments,
                    &mut backend,
                )
            }?;
            self.core.harvest_queue(self.queue);
            match result {
                SourceOutcome::Returned(v) => Ok(DependencyReturn::Pending(v)),
                SourceOutcome::Thrown(e) => Ok(DependencyReturn::PendingThrow(e)),
            }
        } else {
            self.core.invoke(call)
        }
    }
}
impl ContractCore {
    fn harvest_queue(&mut self, queue: &mut ContractQueueSourceOwner) {
        let old = queue.take_db_calls();
        let detailed = queue.take_detailed_db_calls();
        if self.global_clearing_enabled() {
            match detailed {
                Ok(events) => {
                    for event in events {
                        if self.amm_lifecycle_enabled()
                            && event.get("kind") == s("db")
                            && event.get("collection") == s("orderBooks")
                            && matches!(event.get("method"),V::String(method)if ["findOneAsync","updateAsync"].contains(&method.as_str()))
                            && matches!(&event,V::Object(fields)if fields.iter().any(|(key,_)|key=="returned"))
                            && self.source_shared_orderbook_slot().is_err()
                        {
                            self.effects_exhausted = true;
                        }
                        self.emit(event);
                    }
                }
                Err(_) => self.effects_exhausted = true,
            }
        } else {
            for call in old {
                self.emit(call.value());
            }
        }
    }
}
include!("source_contract_accounting_impl.rs");
include!("source_contract_aux_impl.rs");

include!("source_contract_fee_impl.rs");

include!("source_contract_oracle_impl.rs");

include!("source_clearing_funding_impl.rs");
include!("source_insurance_lifecycle_impl.rs");
include!("source_clearing_global_impl.rs");

include!("source_insurance_legacy_v1_impl.rs");

include!("source_iou_settlement_impl.rs");
include!("source_clearing_liquidation_impl.rs");
include!("source_clearing_expiry_impl.rs");

include!("source_clearing_mark_settlement_impl.rs");

include!("source_clearing_supply_impl.rs");

include!("source_contract_history_schedule_impl.rs");
include!("source_contract_registry_lifecycle_impl.rs");
include!("source_channel_financial_impl.rs");
include!("source_channel_volume_impl.rs");
include!("source_channel_logic_impl.rs");
include!("source_contract_position_identity_impl.rs");

include!("source_option_trade_impl.rs");
include!("source_synthetic_lifecycle_impl.rs");
include!("source_derivative_exercise_impl.rs");

include!("source_distribution_lifecycle_impl.rs");

include!("source_proof_lifecycle_impl.rs");
include!("source_proof_settlement_impl.rs");

include!("source_procedural_bitvm_impl.rs");

include!("source_liquidity_rewards_impl.rs");

include!("source_tail_lifecycle_impl.rs");

include!("source_lower_oracle_lifecycle_impl.rs");

include!("source_lower_property_lifecycle_impl.rs");

impl SourceContractRuntime {
    fn source_main_decode_reused(
        &mut self,
        arguments: &[V],
    ) -> Result<crate::types_source::TypesOutcome, SourceRuleError> {
        self.last_parameters = None;
        if arguments.get(1) != Some(&n(18.0))
            && !(self.core.amm_lifecycle_enabled() && arguments.get(1) == Some(&n(10.)))
            && !(self.core.activation_lifecycle_enabled() && arguments.get(1) == Some(&n(0.)))
            && !(self.core.spot_lifecycle_enabled()
                && [5., 6.]
                    .iter()
                    .any(|kind| arguments.get(1) == Some(&n(*kind))))
            && !(self.core.main_callable_enabled() && arguments.get(1) == Some(&n(3.0)))
            && !(self.core.main_callable_enabled()
                && [2., 4.]
                    .iter()
                    .any(|kind| arguments.get(1) == Some(&n(*kind))))
            && !(self.core.main_callable_enabled()
                && [7., 9.]
                    .iter()
                    .any(|kind| arguments.get(1) == Some(&n(*kind))))
            && !(self.core.lower_lifecycle_enabled()
                && [1., 11., 12.]
                    .iter()
                    .any(|kind| arguments.get(1) == Some(&n(*kind))))
            && !(self.core.lower_lifecycle_enabled()
                && [8., 13., 14., 15.]
                    .iter()
                    .any(|k| arguments.get(1) == Some(&n(*k))))
            && !(self.core.proof_lifecycle_enabled()
                && [30., 32., 33., 34., 35.]
                    .iter()
                    .any(|kind| arguments.get(1) == Some(&n(*kind))))
            && !(self.core.distribution_lifecycle_enabled()
                && [26., 28., 29.]
                    .iter()
                    .any(|kind| arguments.get(1) == Some(&n(*kind))))
            && !(self.core.option_lifecycle_enabled()
                && [24., 25., 27.]
                    .iter()
                    .any(|kind| arguments.get(1) == Some(&n(*kind))))
            && !(self.core.lifecycle_enabled()
                && [16., 17., 19., 20., 21., 22., 23., 31.]
                    .iter()
                    .any(|kind| arguments.get(1) == Some(&n(*kind))))
        {
            return Err(SourceRuleError::InvalidInput);
        };
        self.core.channel_token_validation =
            self.core.lifecycle_enabled() && arguments.get(1) == Some(&n(20.));
        let spot_lend_queue = self.core.spot_lifecycle_enabled()
            && self.core.queue_context.is_none()
            && [5., 6.]
                .iter()
                .any(|kind| arguments.get(1) == Some(&n(*kind)));
        if spot_lend_queue {
            if self.core.queue_context.is_some() {
                return Err(SourceRuleError::Backend);
            }
            self.core.queue_context = Some(std::mem::replace(
                &mut self.queue,
                ContractQueueSourceOwner::new(),
            ));
        }
        let result = if self.core.spot_lifecycle_enabled() && arguments.get(1) == Some(&n(6.0)) {
            let mut backend = SourceSpotValidityBackend::new(&mut self.core);
            crate::types_source::decode_payload_unverified(arguments, &mut backend)
        } else if self.core.spot_lifecycle_enabled()
            && self.core.source_spot_main_active
            && arguments.get(1) == Some(&n(22.0))
        {
            let mut backend = SourceTransferValidityBackend::new(&mut self.core);
            let result = crate::types_source::decode_payload_unverified(arguments, &mut backend);
            backend.finish()?;
            result
        } else if self.core.option_lifecycle_enabled() && arguments.get(1) == Some(&n(27.0)) {
            let mut backend = SourceOptionValidityBackend::new(&mut self.core);
            crate::types_source::decode_payload_unverified(arguments, &mut backend)
        } else {
            crate::types_source::decode_payload_unverified(arguments, &mut self.core)
        };
        if spot_lend_queue {
            self.queue = self
                .core
                .queue_context
                .take()
                .ok_or(SourceRuleError::Backend)?;
        }
        self.core
            .source_token_threshold_drain()
            .map_err(|e| match e {
                Fault::Local(e) => e,
                Fault::Thrown(_) => SourceRuleError::Backend,
            })?;
        self.core.channel_token_validation = false;
        if self.core.lifecycle_enabled() {
            self.core
                .source_channel_sufficiency_drain()
                .map_err(|e| match e {
                    Fault::Local(e) => e,
                    Fault::Thrown(_) => SourceRuleError::Backend,
                })?;
        }
        let result = result.map_err(|error| match error {
            // Preserve the actual unavailable dependency at the new Main
            // boundary. Older policies retain their historical error receipt.
            crate::types_source::TypesError::Validity(SourceRuleError::UnknownDependency)
                if self.core.spot_lifecycle_enabled() && self.core.source_spot_main_active =>
            {
                SourceRuleError::UnknownDependency
            }
            _ => SourceRuleError::Backend,
        })?;
        if let Some(validity) = &result.validity {
            for (key, value) in &validity.settlement.evaluation.global_writes {
                self.core.globals.insert(key, value.clone())?
            }
        }
        Ok(result.outcome)
    }
    fn source_main_logic_reused(
        &mut self,
        arguments: &[V],
        params: &V,
    ) -> Result<SourceOutcome, SourceRuleError> {
        if params.get("valid") == V::Bool(true)
            && self.core.main_callable_enabled()
            && arguments.get(1) == Some(&n(3.0))
        {
            return self.source_lower_utxo_trade_raw_logic(params);
        }
        if self.core.activation_lifecycle_enabled()
            && arguments.get(1) == Some(&n(0.))
            && params.get("valid") == V::Bool(true)
        {
            return self.source_activation_raw_logic(params);
        }
        if self.core.amm_lifecycle_enabled()
            && arguments.get(1) == Some(&n(10.))
            && params.get("valid") == V::Bool(true)
        {
            return self.source_amm_raw_logic(params);
        }
        let logic = if params.get("valid") == V::Bool(true)
            && self.core.spot_lifecycle_enabled()
            && [5., 6.]
                .iter()
                .any(|kind| arguments.get(1) == Some(&n(*kind)))
        {
            self.source_spot_raw_logic(&arguments[1], params)?
        } else if params.get("valid") == V::Bool(true)
            && self.core.main_callable_enabled()
            && [2., 4.]
                .iter()
                .any(|kind| arguments.get(1) == Some(&n(*kind)))
        {
            self.source_send_commit_raw_logic(&arguments[1], params)?
        } else if params.get("valid") == V::Bool(true)
            && self.core.main_callable_enabled()
            && [7., 9.]
                .iter()
                .any(|kind| arguments.get(1) == Some(&n(*kind)))
        {
            self.source_clearlist_raw_logic(&arguments[1], params)?
        } else if params.get("valid") == V::Bool(true)
            && self.core.lower_lifecycle_enabled()
            && [1., 11., 12.]
                .iter()
                .any(|kind| arguments.get(1) == Some(&n(*kind)))
        {
            self.source_lower_property_raw_logic(&arguments[1], params)?
        } else if params.get("valid") == V::Bool(true)
            && self.core.lower_lifecycle_enabled()
            && [8., 13., 14., 15.]
                .iter()
                .any(|k| arguments.get(1) == Some(&n(*k)))
        {
            self.source_lower_oracle_raw_logic(&arguments[1], params)?
        } else if params.get("valid") == V::Bool(true)
            && self.core.proof_lifecycle_enabled()
            && arguments.get(1) == Some(&n(30.0))
        {
            self.source_proof_raw_logic(params)?
        } else if params.get("valid") == V::Bool(true)
            && self.core.distribution_lifecycle_enabled()
            && [26., 28., 29.]
                .iter()
                .any(|kind| arguments.get(1) == Some(&n(*kind)))
        {
            self.source_distribution_raw_logic(&arguments[1], params)?
        } else if params.get("valid") == V::Bool(true)
            && self.core.option_lifecycle_enabled()
            && arguments.get(1) == Some(&n(27.0))
        {
            self.source_option_raw_logic(params)?
        } else if params.get("valid") == V::Bool(true)
            && self.core.option_lifecycle_enabled()
            && [24., 25.]
                .iter()
                .any(|kind| arguments.get(1) == Some(&n(*kind)))
        {
            self.source_synthetic_raw_logic(&arguments[1], params)?
        } else if params.get("valid") == V::Bool(true) && arguments.get(1) != Some(&n(18.0)) {
            self.source_lifecycle_raw_logic(&arguments[1], params)?
        } else if params.get("valid") == V::Bool(true) {
            let queued = obj(&[
                ("contractId", params.get("contractId")),
                ("price", params.get("price")),
                ("amount", params.get("amount")),
                ("sell", params.get("sell")),
                ("insurance", params.get("insurance")),
                ("blockTime", params.get("block")),
                ("isLiq", params.get("reduce")),
                ("reduce", params.get("post")),
                ("post", params.get("stop")),
                ("stop", params.get("block")),
            ]);
            self.evaluate_queue(&SourceCall {
                method: "queueOnChainContractOrder".into(),
                arguments: vec![
                    s(text(&params.get("contractId"))
                        .map_err(|_| SourceRuleError::InvalidInput)?),
                    params.get("senderAddress"),
                    queued,
                    params.get("block"),
                    params.get("txid"),
                ],
            })?
        } else {
            SourceOutcome::Returned(V::Undefined)
        };
        Ok(logic)
    }
}

include!("source_main_callable_impl.rs");

include!("source_lower_clearlist_impl.rs");

include!("source_lower_send_commit_impl.rs");

include!("source_lower_utxo_trade_impl.rs");

include!("source_spot_matching_impl.rs");
include!("source_spot_lifecycle_impl.rs");

include!("source_spot_schedule_impl.rs");

include!("source_transfer_schedule_impl.rs");

include!("source_token_channel_schedule_impl.rs");

include!("source_withdrawal_schedule_impl.rs");

include!("source_activation_lifecycle_impl.rs");
include!("source_activation_hash_impl.rs");

include!("source_amm_lifecycle_impl.rs");

include!("source_amm_quote_impl.rs");

include!("source_amm_remaining_impl.rs");

include!("source_main_production_impl.rs");

include!("source_main_service_impl.rs");
include!("source_main_snapshot_impl.rs");
include!("source_main_startup_impl.rs");

include!("source_main_native_runtime_impl.rs");
include!("source_main_native_historical_rows_impl.rs");
include!("source_main_native_row_continuation_impl.rs");
include!("source_main_native_historical_row_bridge_impl.rs");
include!("source_main_native_completed_runtime_test_impl.rs");
include!("source_main_native_no_index_runtime_tests_impl.rs");
include!("source_main_native_catchup_prepare_impl.rs");
include!("source_main_native_catchup_continuation_impl.rs");
include!("source_main_native_service_io_impl.rs");
include!("source_main_native_service_block_prelude_impl.rs");
include!("source_main_native_service_block_impl.rs");
include!("source_main_native_service_runtime_impl.rs");
include!("source_main_inline_index_runtime_impl.rs");
include!("source_main_inline_service_runtime_impl.rs");
include!("source_main_active_service_recovery_runtime_impl.rs");
include!("source_main_service_escrow_runtime_impl.rs");
include!("source_main_native_service_fields_impl.rs");
include!("source_main_native_supersession_runtime_impl.rs");
include!("source_main_step_block_impl.rs");
include!("source_main_step_runtime_impl.rs");
include!("source_main_step_file_core_impl.rs");
include!("source_main_step_file_block_impl.rs");
include!("source_main_step_file_runtime_impl.rs");

include!("source_rich_runtime_impl.rs");

#[cfg(test)]
#[path = "source_main_step_file_runtime_tests.rs"]
mod main_step_file_runtime_tests;
#[cfg(test)]
#[path = "source_main_step_runtime_tests.rs"]
mod main_step_runtime_tests;

#[cfg(test)]
mod main_production_parse_int_tests {
    #[test]
    fn omitted_radix_parser_matches_independent_native_node_binary64_results() {
        let fixture = include_str!("../fixtures/main_production_parse_int.tsv");
        let mut count = 0;
        for line in fixture.lines().filter(|line| !line.starts_with('#')) {
            let parts = line.split('\t').collect::<Vec<_>>();
            assert_eq!(parts.len(), 3);
            assert_eq!(parts[1].len() % 2, 0);
            let bytes = parts[1]
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            let text = std::str::from_utf8(&bytes).unwrap();
            let expected = u64::from_str_radix(parts[2], 16).unwrap();
            assert_eq!(
                super::source_main_parse_int10(text).to_bits(),
                expected,
                "{}: {text:?}",
                parts[0]
            );
            count += 1;
        }
        assert_eq!(count, 716);
    }
}

#[cfg(test)]
#[path = "source_main_native_row_continuation_tests.rs"]
mod native_row_continuation_tests;

#[cfg(test)]
#[path = "source_main_native_catchup_continuation_tests.rs"]
mod native_catchup_continuation_tests;

#[cfg(test)]
#[path = "source_main_native_service_block_tests.rs"]
mod native_main_service_block_tests;

#[cfg(test)]
#[path = "source_main_native_service_client_scope_tests.rs"]
mod native_main_service_client_scope_tests;

include!("source_main_bootstrap_state_impl.rs");
include!("source_main_bootstrap_core_impl.rs");
include!("source_main_bootstrap_runtime_impl.rs");
include!("source_main_bootstrap_promises_state_impl.rs");
include!("source_main_bootstrap_promises_core_impl.rs");
include!("source_main_bootstrap_promises_runtime_impl.rs");
#[cfg(test)]
#[path = "source_main_bootstrap_tests.rs"]
mod ordered_bootstrap_tests;
