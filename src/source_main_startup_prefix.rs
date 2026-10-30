//! Literal callable Main.initialize cold-DB prefix. Metadata only: no Runtime,
//! Ledger, Journal, index rows or imported financial/source result snapshots.
use crate::validity_source::{SourceException, SourceRuleError, SourceValue as V};
use std::collections::VecDeque;

pub const PROFILE: &str = "TL-MAIN-COLD-CALLABLE-STARTUP-PREFIX-13-1";
pub const MAIN_SHA256: &str = "006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009";
pub const INIT_SHA256: &str = "55c7dd6da798a58dc57d34ad75be357f9811574db5e6f384620055f966447cc0";
pub const DB_SHA256: &str = "b92a05fee6652494b72bf6893d9633850c697be773e88b24d62798a8bc8f5e38";
// Exact declared recorded filesystem boundary in the pinned source driver.
// This profile does not claim arbitrary TL_NEDB_ROOT/path/environment coercion.
pub const RECORDED_NEDB_ROOT: &str = "/tl-data";
pub const CATEGORIES: [&str; 32] = [
    "ammRegistry",
    "ammState",
    "txIndex",
    "propertyList",
    "oracleList",
    "oracleData",
    "contractList",
    "tallyMap",
    "tallyMapDelta",
    "marginMapDelta",
    "marginMaps",
    "clearlists",
    "attestations",
    "clearing",
    "consensus",
    "persistence",
    "volumeIndex",
    "channels",
    "withdrawQueue",
    "activations",
    "insurance",
    "orderBooks",
    "feeCache",
    "tradeHistory",
    "fundingEvents",
    "vaults",
    "syntheticTokens",
    "liquidations",
    "scaling",
    "channelDelta",
    "iou",
    "procedural",
];
const MAX_STEPS: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupSiteV1 {
    AutomaticDatabaseClient,
    DatabaseRetryClient,
    MainConstructor,
    GlobalInitializeClient,
    DatabaseChain,
    DatabaseTest,
    Datastore {
        category: &'static str,
        directory: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupThrowV1 {
    pub identity: u64,
    pub exception: SourceException,
    pub code: V,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StartupCompletionV1 {
    Returned(V),
    Thrown(StartupThrowV1),
    Held(u64),
}
pub(crate) mod sealed {
    pub trait Sealed {}
}
/// Only an actual borrowed Core/source IO adapter may implement this capability.
/// settle receives a retained IO token, never a supplied completion value.
pub(crate) trait StartupOwnerV1: sealed::Sealed {
    fn invoke(&mut self, site: &StartupSiteV1) -> Result<StartupCompletionV1, SourceRuleError>;
    fn settle(&mut self, token: u64) -> Result<StartupCompletionV1, SourceRuleError>;
    fn main_client(&self) -> V;
    fn native_error(&mut self, name: &str, message: String) -> StartupThrowV1;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupCommandV1 {
    ConstructMain,
    EnterInitialize,
    SettleClient { token: u64 },
    FireTimer { id: u64, generation: u64 },
    Drain { limit: usize },
    EndTurn,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupStopV1 {
    Pending,
    InitializeSuffixReady,
    Thrown(StartupThrowV1),
    Local(SourceRuleError),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupErrorV1 {
    InvalidInput,
    Capacity,
    WrongGeneration,
    TimerNotNext,
    NotQuiescent,
    AlreadyStopped,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum DbState {
    AwaitClient,
    Chain,
    Test,
    Open,
    Complete,
    Thrown(StartupThrowV1),
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum MainState {
    Unconstructed,
    Constructed,
    Timer1500,
    GlobalClient,
    GlobalTimer500,
    AwaitDatabase,
    Timer300,
    SuffixReady,
    Thrown(StartupThrowV1),
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Job {
    AutoClient(StartupCompletionV1),
    GlobalClient(StartupCompletionV1),
    After1500,
    After500,
    After300,
    Database,
    CompleteDatabase,
    NotifyModuleRejection,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Timer {
    id: u64,
    delay: u64,
    due: u64,
    site: &'static str,
    continuation: Job,
}
/// Intentionally no Clone: a live caller/shared-promise prefix cannot be forked.
pub struct MainStartupPrefixV1 {
    generation: u64,
    next_timer: u64,
    logical_time: u64,
    main: MainState,
    database: DbState,
    database_chain: V,
    database_test: V,
    database_path: String,
    categories: Vec<&'static str>,
    open_cursor: usize,
    database_round: u64,
    automatic_token: Option<u64>,
    global_token: Option<u64>,
    ready: VecDeque<Job>,
    timers: VecDeque<Timer>,
    module_rejection: Option<StartupThrowV1>,
    notification_pending: bool,
    notified: bool,
    stopped: Option<SourceRuleError>,
    steps: usize,
    events: Vec<V>,
}
fn s(value: impl Into<String>) -> V {
    V::String(value.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(key, value)| ((*key).into(), value.clone()))
            .collect(),
    )
}
fn truth(value: &V) -> bool {
    match value {
        V::Undefined | V::Null => false,
        V::Bool(value) => *value,
        V::Number(bits) => {
            let value = f64::from_bits(*bits);
            value != 0. && !value.is_nan()
        }
        V::String(value) => !value.is_empty(),
        _ => true,
    }
}
fn throw_value(error: &StartupThrowV1) -> V {
    obj(&[
        ("identity", s(error.identity.to_string())),
        ("name", s(&error.exception.name)),
        ("message", s(&error.exception.message)),
        ("code", error.code.clone()),
    ])
}
impl MainStartupPrefixV1 {
    /// Called at the real original DB module import site, before Main's constructor.
    pub(crate) fn import_database(
        owner: &mut impl StartupOwnerV1,
        generation: u64,
    ) -> Result<Self, SourceRuleError> {
        let completion = owner.invoke(&StartupSiteV1::AutomaticDatabaseClient)?;
        let (automatic_token, ready) = match completion {
            StartupCompletionV1::Held(token) => (Some(token), VecDeque::new()),
            other => (None, VecDeque::from([Job::AutoClient(other)])),
        };
        Ok(Self {
            generation,
            next_timer: 1,
            logical_time: 0,
            main: MainState::Unconstructed,
            database: DbState::AwaitClient,
            database_chain: V::Undefined,
            database_test: V::Bool(false),
            database_path: String::new(),
            categories: vec![],
            open_cursor: 0,
            database_round: 1,
            automatic_token,
            global_token: None,
            ready,
            timers: VecDeque::new(),
            module_rejection: None,
            notification_pending: false,
            notified: false,
            stopped: None,
            steps: 0,
            events: vec![],
        })
    }
    pub fn next_timer(&self) -> Option<(u64, u64, &'static str)> {
        self.timers
            .front()
            .map(|timer| (timer.id, timer.delay, timer.site))
    }
    pub fn pending_clients(&self) -> Vec<(&'static str, u64)> {
        [
            self.automatic_token
                .map(|id| ("AutomaticDatabaseClient", id)),
            self.global_token.map(|id| ("GlobalInitializeClient", id)),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    pub fn stop(&self) -> StartupStopV1 {
        if let Some(error) = &self.stopped {
            return StartupStopV1::Local(error.clone());
        }
        match &self.main {
            MainState::SuffixReady => StartupStopV1::InitializeSuffixReady,
            MainState::Thrown(error) => StartupStopV1::Thrown(error.clone()),
            _ => StartupStopV1::Pending,
        }
    }
    pub fn events(&self) -> &[V] {
        &self.events
    }
    fn timer(&mut self, delay: u64, site: &'static str, continuation: Job) {
        let id = self.next_timer;
        self.next_timer += 1;
        self.timers.push_back(Timer {
            id,
            delay,
            due: self.logical_time + delay,
            site,
            continuation,
        });
        self.events.push(obj(&[
            ("kind", s("timer-scheduled")),
            ("id", s(id.to_string())),
            ("delay", V::number(delay as f64)),
            ("site", s(site)),
        ]));
    }
    pub(crate) fn apply(
        &mut self,
        owner: &mut impl StartupOwnerV1,
        command: StartupCommandV1,
    ) -> Result<StartupStopV1, StartupErrorV1> {
        if self.stopped.is_some() {
            return Err(StartupErrorV1::AlreadyStopped);
        }
        if self.steps >= MAX_STEPS {
            return Err(StartupErrorV1::Capacity);
        }
        match command {
            StartupCommandV1::ConstructMain => {
                if self.main != MainState::Unconstructed {
                    return Err(StartupErrorV1::InvalidInput);
                }
                match owner.invoke(&StartupSiteV1::MainConstructor) {
                    Ok(StartupCompletionV1::Returned(_)) => self.main = MainState::Constructed,
                    Ok(StartupCompletionV1::Thrown(error)) => self.main = MainState::Thrown(error),
                    Ok(StartupCompletionV1::Held(_)) => return Err(StartupErrorV1::InvalidInput),
                    Err(error) => self.stopped = Some(error),
                }
            }
            StartupCommandV1::EnterInitialize => {
                if self.main != MainState::Constructed {
                    return Err(StartupErrorV1::InvalidInput);
                }
                self.main = MainState::Timer1500;
                self.timer(1500, "Main.delay", Job::After1500);
            }
            StartupCommandV1::SettleClient { token } => {
                let auto = self.automatic_token == Some(token);
                let global = self.global_token == Some(token);
                if auto == global {
                    return Err(StartupErrorV1::InvalidInput);
                }
                match owner.settle(token) {
                    Ok(StartupCompletionV1::Held(_)) => return Err(StartupErrorV1::InvalidInput),
                    Ok(completion) => {
                        if auto {
                            self.automatic_token = None;
                            self.ready.push_back(Job::AutoClient(completion));
                        } else {
                            self.global_token = None;
                            self.ready.push_back(Job::GlobalClient(completion));
                        }
                    }
                    Err(error) => self.stopped = Some(error),
                }
            }
            StartupCommandV1::FireTimer { id, generation } => {
                if generation != self.generation {
                    return Err(StartupErrorV1::WrongGeneration);
                }
                if !self.ready.is_empty() {
                    return Err(StartupErrorV1::NotQuiescent);
                }
                if self.timers.front().map(|timer| timer.id) != Some(id) {
                    return Err(StartupErrorV1::TimerNotNext);
                }
                let timer = self.timers.pop_front().expect("checked actual timer");
                self.logical_time = timer.due;
                self.events.push(obj(&[
                    ("kind", s("timer-fired")),
                    ("id", s(id.to_string())),
                ]));
                self.ready.push_back(timer.continuation);
            }
            StartupCommandV1::Drain { limit } => {
                if limit == 0 || limit > MAX_STEPS {
                    return Err(StartupErrorV1::InvalidInput);
                }
                for _ in 0..limit {
                    let Some(job) = self.ready.pop_front() else {
                        break;
                    };
                    if let Err(error) = self.run(owner, job) {
                        self.stopped = Some(error);
                        break;
                    }
                }
            }
            StartupCommandV1::EndTurn => {
                if !self.ready.is_empty() {
                    return Err(StartupErrorV1::NotQuiescent);
                }
                if self.notification_pending {
                    self.notification_pending = false;
                    self.notified = true;
                    self.events.push(obj(&[
                        ("kind", s("unhandled-rejection")),
                        ("promise", s("OriginalDbModuleIife")),
                        (
                            "error",
                            throw_value(
                                self.module_rejection
                                    .as_ref()
                                    .expect("actual module rejection"),
                            ),
                        ),
                    ]));
                }
            }
        }
        self.steps += 1;
        Ok(self.stop())
    }
    fn run(&mut self, owner: &mut impl StartupOwnerV1, job: Job) -> Result<(), SourceRuleError> {
        match job {
            Job::AutoClient(StartupCompletionV1::Returned(client)) => {
                if client == V::Null || client == V::Undefined {
                    let error = owner.native_error(
                        "TypeError",
                        format!(
                            "Cannot read properties of {} (reading 'getChain')",
                            if client == V::Null {
                                "null"
                            } else {
                                "undefined"
                            }
                        ),
                    );
                    self.fail_database(error);
                } else {
                    self.database = DbState::Chain;
                    self.ready.push_back(Job::Database);
                }
            }
            Job::AutoClient(StartupCompletionV1::Thrown(error)) => self.fail_database(error),
            Job::GlobalClient(StartupCompletionV1::Returned(client)) => {
                if !truth(&client) {
                    return Err(SourceRuleError::UnknownDependency);
                }
                self.main = MainState::GlobalTimer500;
                self.timer(500, "init.initialize/setTimeout", Job::After500);
            }
            Job::GlobalClient(StartupCompletionV1::Thrown(error)) => {
                self.main = MainState::Thrown(error)
            }
            Job::AutoClient(StartupCompletionV1::Held(_))
            | Job::GlobalClient(StartupCompletionV1::Held(_)) => {
                return Err(SourceRuleError::InvalidInput)
            }
            Job::After1500 => {
                if self.database == DbState::Complete {
                    self.main = MainState::SuffixReady;
                    return Ok(());
                }
                let client = owner.main_client();
                if client == V::Null || client == V::Undefined {
                    self.main = MainState::Thrown(owner.native_error(
                        "TypeError",
                        format!(
                            "Cannot read properties of {} (reading 'chain')",
                            if client == V::Null {
                                "null"
                            } else {
                                "undefined"
                            }
                        ),
                    ));
                } else if truth(&client.get("chain")) {
                    self.main = MainState::SuffixReady;
                } else {
                    self.main = MainState::GlobalClient;
                    match owner.invoke(&StartupSiteV1::GlobalInitializeClient)? {
                        StartupCompletionV1::Held(token) => {
                            if Some(token) == self.automatic_token {
                                return Err(SourceRuleError::InvalidInput);
                            }
                            self.global_token = Some(token);
                        }
                        result => self.ready.push_back(Job::GlobalClient(result)),
                    }
                }
            }
            Job::After500 => {
                self.main = MainState::AwaitDatabase;
                // Literal DB.init shares only an IN-PROGRESS initPromise.
                // A finalized previous rejection leaves initializing=false;
                // this later real call creates a fresh internal promise.
                if matches!(self.database, DbState::Thrown(_)) {
                    self.database_round += 1;
                    self.database = DbState::AwaitClient;
                    self.database_chain = V::Undefined;
                    self.open_cursor = 0;
                    let completion = owner.invoke(&StartupSiteV1::DatabaseRetryClient)?;
                    if matches!(completion, StartupCompletionV1::Held(_)) {
                        return Err(SourceRuleError::UnknownDependency);
                    }
                    self.ready.push_back(Job::AutoClient(completion));
                } else {
                    self.resume_database_waiter();
                }
            }
            Job::After300 => self.main = MainState::SuffixReady,
            Job::Database => {
                let site = match self.database {
                    DbState::Chain => StartupSiteV1::DatabaseChain,
                    DbState::Test => StartupSiteV1::DatabaseTest,
                    DbState::Open => return self.open_categories(owner),
                    _ => return Err(SourceRuleError::InvalidInput),
                };
                match owner.invoke(&site)? {
                    StartupCompletionV1::Thrown(error) => self.fail_database(error),
                    StartupCompletionV1::Held(_) => return Err(SourceRuleError::UnknownDependency),
                    StartupCompletionV1::Returned(value) => match self.database {
                        DbState::Chain => {
                            self.database_chain = value;
                            self.database = DbState::Test;
                            self.ready.push_back(Job::Database);
                        }
                        DbState::Test => {
                            self.database_test = value;
                            if !truth(&self.database_chain) {
                                return Err(SourceRuleError::UnknownDependency);
                            }
                            let V::String(chain) = &self.database_chain else {
                                let error = owner.native_error(
                                    "TypeError",
                                    "chain.toLowerCase is not a function".into(),
                                );
                                self.fail_database(error);
                                return Ok(());
                            };
                            self.database_path = format!(
                                "{RECORDED_NEDB_ROOT}/{}-{}",
                                chain.to_lowercase(),
                                if self.database_test == V::Bool(false) {
                                    "main"
                                } else {
                                    "test"
                                }
                            );
                            self.database = DbState::Open;
                            self.ready.push_back(Job::Database);
                        }
                        _ => return Err(SourceRuleError::InvalidInput),
                    },
                }
            }
            Job::CompleteDatabase => {
                self.database = DbState::Complete;
                self.resume_database_waiter();
            }
            Job::NotifyModuleRejection => self.notification_pending = true,
        }
        Ok(())
    }
    fn open_categories(&mut self, owner: &mut impl StartupOwnerV1) -> Result<(), SourceRuleError> {
        // Original Array.forEach is synchronous. No controller command or job
        // can interleave between category opens, including the retained throw.
        while self.open_cursor < CATEGORIES.len() {
            let category = CATEGORIES[self.open_cursor];
            match owner.invoke(&StartupSiteV1::Datastore {
                category,
                directory: self.database_path.clone(),
            })? {
                StartupCompletionV1::Returned(_) => {
                    if !self.categories.contains(&category) {
                        self.categories.push(category);
                    }
                    self.open_cursor += 1;
                }
                StartupCompletionV1::Thrown(error) => {
                    self.fail_database(error);
                    return Ok(());
                }
                StartupCompletionV1::Held(_) => return Err(SourceRuleError::InvalidInput),
            }
        }
        self.ready.push_back(Job::CompleteDatabase);
        Ok(())
    }
    fn fail_database(&mut self, error: StartupThrowV1) {
        self.database = DbState::Thrown(error.clone());
        if self.database_round == 1 {
            self.module_rejection = Some(error);
            self.ready.push_back(Job::NotifyModuleRejection);
        }
        self.resume_database_waiter();
    }
    fn resume_database_waiter(&mut self) {
        if self.main != MainState::AwaitDatabase {
            return;
        }
        match &self.database {
            DbState::Complete => {
                self.main = MainState::Timer300;
                self.timer(300, "Main.delay", Job::After300);
            }
            DbState::Thrown(error) => self.main = MainState::Thrown(error.clone()),
            _ => (),
        }
    }
    pub fn visit_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s(PROFILE))?;
        visitor("generation", &s(self.generation.to_string()))?;
        visitor("nextTimer", &s(self.next_timer.to_string()))?;
        visitor("logicalTime", &s(self.logical_time.to_string()))?;
        visitor("steps", &s(self.steps.to_string()))?;
        visitor(
            "main/pc",
            &s(match self.main {
                MainState::Unconstructed => "Unconstructed",
                MainState::Constructed => "Constructed",
                MainState::Timer1500 => "Timer1500",
                MainState::GlobalClient => "GlobalClient",
                MainState::GlobalTimer500 => "GlobalTimer500",
                MainState::AwaitDatabase => "AwaitDatabase",
                MainState::Timer300 => "Timer300",
                MainState::SuffixReady => "SuffixReady",
                MainState::Thrown(_) => "Thrown",
            }),
        )?;
        if let MainState::Thrown(error) = &self.main {
            visitor("main/throw", &throw_value(error))?;
        }
        visitor(
            "database/pc",
            &s(match self.database {
                DbState::AwaitClient => "AwaitClient",
                DbState::Chain => "Chain",
                DbState::Test => "Test",
                DbState::Open => "Open",
                DbState::Complete => "Complete",
                DbState::Thrown(_) => "Thrown",
            }),
        )?;
        if let DbState::Thrown(error) = &self.database {
            visitor("database/throw", &throw_value(error))?;
        }
        visitor("database/chain", &self.database_chain)?;
        visitor("database/round", &s(self.database_round.to_string()))?;
        visitor("database/openCursor", &s(self.open_cursor.to_string()))?;
        visitor("database/test", &self.database_test)?;
        visitor("database/path", &s(&self.database_path))?;
        visitor(
            "database/initialized",
            &V::Bool(self.database == DbState::Complete),
        )?;
        visitor(
            "database/initializing",
            &V::Bool(!matches!(
                self.database,
                DbState::Complete | DbState::Thrown(_)
            )),
        )?;
        visitor(
            "database/categories/count",
            &s(self.categories.len().to_string()),
        )?;
        for (i, category) in self.categories.iter().enumerate() {
            visitor(&format!("database/categories/{i}"), &s(*category))?;
        }
        visitor("notification/pending", &V::Bool(self.notification_pending))?;
        visitor("notification/notified", &V::Bool(self.notified))?;
        if let Some(error) = &self.module_rejection {
            visitor("notification/error", &throw_value(error))?;
        }
        visitor("ready/count", &s(self.ready.len().to_string()))?;
        for (i, job) in self.ready.iter().enumerate() {
            visit_job(job, &format!("ready/{i}"), visitor)?;
        }
        visitor(
            "pending/automatic",
            &self.automatic_token.map_or(V::Null, |id| s(id.to_string())),
        )?;
        visitor(
            "pending/global",
            &self.global_token.map_or(V::Null, |id| s(id.to_string())),
        )?;
        visitor("timers/count", &s(self.timers.len().to_string()))?;
        for (i, timer) in self.timers.iter().enumerate() {
            visitor(&format!("timers/{i}/id"), &s(timer.id.to_string()))?;
            visitor(&format!("timers/{i}/delay"), &V::number(timer.delay as f64))?;
            visitor(&format!("timers/{i}/site"), &s(timer.site))?;
            visitor(&format!("timers/{i}/due"), &s(timer.due.to_string()))?;
            visit_job(
                &timer.continuation,
                &format!("timers/{i}/continuation"),
                visitor,
            )?;
        }
        visitor("events/count", &s(self.events.len().to_string()))?;
        for (i, event) in self.events.iter().enumerate() {
            visitor(&format!("events/{i}"), event)?;
        }
        visitor(
            "stopped",
            &self
                .stopped
                .as_ref()
                .map_or(V::Null, |e| s(format!("{e:?}"))),
        )
    }
}
fn visit_job(
    job: &Job,
    path: &str,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    let (kind, completion) = match job {
        Job::AutoClient(value) => ("AutoClient", Some(value)),
        Job::GlobalClient(value) => ("GlobalClient", Some(value)),
        Job::After1500 => ("After1500", None),
        Job::After500 => ("After500", None),
        Job::After300 => ("After300", None),
        Job::Database => ("Database", None),
        Job::CompleteDatabase => ("CompleteDatabase", None),
        Job::NotifyModuleRejection => ("NotifyModuleRejection", None),
    };
    visitor(&format!("{path}/kind"), &s(kind))?;
    if let Some(completion) = completion {
        let (kind, value) = match completion {
            StartupCompletionV1::Returned(value) => ("Returned", value.clone()),
            StartupCompletionV1::Thrown(error) => ("Thrown", throw_value(error)),
            StartupCompletionV1::Held(id) => ("Held", s(id.to_string())),
        };
        visitor(&format!("{path}/completion/kind"), &s(kind))?;
        visitor(&format!("{path}/completion/value"), &value)?;
    }
    Ok(())
}
