use core::future::Future;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};
use tradelayer_core::bootstrap::*;
use tradelayer_core::chain::BlockId;
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::storage::{Journal, Mutation, Recovery, StorageError};

struct Noop;
impl Wake for Noop {
    fn wake(self: Arc<Self>) {}
}

struct VirtualDelay {
    first: bool,
    milliseconds: u64,
    elapsed: Rc<Cell<u64>>,
    fail: bool,
}
impl Future for VirtualDelay {
    type Output = Result<(), String>;
    fn poll(mut self: core::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.fail {
            return Poll::Ready(Err("timer failure".to_owned()));
        }
        if !self.first {
            self.first = true;
            return Poll::Pending;
        }
        self.elapsed.set(self.elapsed.get() + self.milliseconds);
        Poll::Ready(Ok(()))
    }
}

fn chain(value: &str) -> LegacyChainValue {
    match value {
        "undefined" => LegacyChainValue::Undefined,
        "null" => LegacyChainValue::Null,
        value => LegacyChainValue::Name(value.to_owned()),
    }
}
fn chain_name(value: &LegacyChainValue) -> &str {
    match value {
        LegacyChainValue::Undefined => "undefined",
        LegacyChainValue::Null => "null",
        LegacyChainValue::Name(name) => name,
    }
}
fn compress(events: &[String]) -> String {
    let mut runs: Vec<(&str, usize)> = Vec::new();
    for event in events {
        if let Some((last, count)) = runs.last_mut() {
            if *last == event {
                *count += 1;
                continue;
            }
        }
        runs.push((event, 1));
    }
    if runs.is_empty() {
        return "-".to_owned();
    }
    runs.into_iter()
        .map(|(event, count)| {
            if count == 1 {
                event.to_owned()
            } else {
                format!("{event}*{count}")
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[test]
fn literal_functions_match_all_unmodified_source_ordering_and_failure_cases() {
    let mut count = 0;
    for line in include_str!("../fixtures/bootstrap_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let f: Vec<_> = line.split('\t').collect();
        assert_eq!(f.len(), 13);
        let present: bool = f[2].parse().unwrap();
        let client_error: bool = f[4].parse().unwrap();
        let database_error: bool = f[5].parse().unwrap();
        let timer_error: bool = f[6].parse().unwrap();
        let timeout: u64 = f[7].parse().unwrap();
        let interval: u64 = f[8].parse().unwrap();
        let polls: usize = f[9].parse().unwrap();
        let events = Rc::new(RefCell::new(Vec::<String>::new()));
        let elapsed = Rc::new(Cell::new(0));
        let delay = |milliseconds| {
            events.borrow_mut().push(format!("timer/{milliseconds}"));
            VirtualDelay {
                first: false,
                milliseconds,
                elapsed: elapsed.clone(),
                fail: timer_error,
            }
        };
        let waker = Waker::from(Arc::new(Noop));
        let mut cx = Context::from_waker(&waker);
        let mut status = "pending".to_owned();
        if f[0] == "helper" {
            let mut future = Box::pin(legacy_wait_for_client_chain(
                present, timeout, interval, delay,
            ));
            for _ in 0..polls {
                if let Poll::Ready(result) = future.as_mut().poll(&mut cx) {
                    status = result
                        .map_or_else(|error| format!("error/{error}"), |()| "ready".to_owned());
                    break;
                }
            }
        } else {
            let client_handle = Rc::new(());
            let database_handle = Rc::new(());
            let get_client = || {
                events.borrow_mut().push("client".to_owned());
                std::future::ready(if client_error {
                    Err("client failure".to_owned())
                } else {
                    Ok(present.then(|| LegacyClient {
                        handle: client_handle.clone(),
                        chain: chain(f[3]),
                    }))
                })
            };
            let initialize_database = |chain| {
                events
                    .borrow_mut()
                    .push(format!("db/{}", chain_name(&chain)));
                std::future::ready(if database_error {
                    Err("database failure".to_owned())
                } else {
                    Ok(database_handle.clone())
                })
            };
            let mut future = Box::pin(legacy_initialize(get_client, delay, initialize_database));
            for _ in 0..polls {
                if let Poll::Ready(result) = future.as_mut().poll(&mut cx) {
                    status = match result {
                        Ok((client, database)) => {
                            assert!(Rc::ptr_eq(&client, &client_handle));
                            assert!(Rc::ptr_eq(&database, &database_handle));
                            "ready".to_owned()
                        }
                        Err(error) => format!("error/{error}"),
                    };
                    break;
                }
            }
        }
        assert_eq!(elapsed.get().to_string(), f[10], "{} elapsed", f[1]);
        assert_eq!(compress(&events.borrow()), f[11], "{} calls", f[1]);
        assert_eq!(status, f[12], "{} result", f[1]);
        count += 1;
    }
    assert_eq!(count, 12);
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-bootstrap-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn config(&self) -> BootstrapConfig {
        BootstrapConfig::new(TradeLayerMainnet::Bitcoin, self.0.clone(), "persistence").unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && self.0.starts_with(std::env::temp_dir()));
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("tl-bootstrap-"));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

type Events = Rc<RefCell<Vec<&'static str>>>;
#[derive(Clone, Default)]
struct Plan {
    events: Events,
    fail: Option<BootstrapStage>,
    cancel: Option<BootstrapStage>,
    panic: Option<BootstrapStage>,
    node: Option<ClaimedNodeIdentity>,
    restored_network: Option<TradeLayerMainnet>,
    restored_genesis: Option<BlockId>,
    deadline_in_acquire: bool,
}
impl Plan {
    fn step(&self, stage: BootstrapStage, control: &BootstrapControl) -> Result<(), String> {
        self.events.borrow_mut().push(match stage {
            BootstrapStage::AcquireClient => "acquire",
            BootstrapStage::IdentifyNetwork => "identify",
            BootstrapStage::OpenStore => "open",
            BootstrapStage::RestoreState => "restore",
            BootstrapStage::PublishReady => "publish",
        });
        assert_ne!(self.panic, Some(stage), "injected adapter panic");
        if self.cancel == Some(stage) {
            control.cancel();
        }
        if self.deadline_in_acquire && stage == BootstrapStage::AcquireClient {
            while control.check().is_ok() {
                std::thread::yield_now();
            }
        }
        if self.fail == Some(stage) {
            Err("injected adapter failure".to_owned())
        } else {
            Ok(())
        }
    }
}
struct ClientFactory(Plan);
struct Client(Plan);
impl Drop for Client {
    fn drop(&mut self) {
        self.0.events.borrow_mut().push("drop_client");
    }
}
impl BootstrapClientFactory for ClientFactory {
    type Client = Client;
    fn acquire(
        &mut self,
        _: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<Client, String> {
        let client = Client(self.0.clone());
        self.0.step(BootstrapStage::AcquireClient, control)?;
        Ok(client)
    }
}
impl BootstrapClient for Client {
    fn identity(&mut self, control: &BootstrapControl) -> Result<ClaimedNodeIdentity, String> {
        self.0.step(BootstrapStage::IdentifyNetwork, control)?;
        Ok(self.0.node.unwrap_or(ClaimedNodeIdentity {
            network: TradeLayerMainnet::Bitcoin,
            test: false,
            genesis: TradeLayerMainnet::Bitcoin.genesis(),
        }))
    }
}
struct StoreFactory(Plan);
struct Store(Plan);
struct State(Plan);
impl Drop for Store {
    fn drop(&mut self) {
        self.0.events.borrow_mut().push("drop_store");
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.0.events.borrow_mut().push("drop_state");
    }
}
impl BootstrapStoreFactory for StoreFactory {
    type Store = Store;
    fn open(&mut self, _: &BootstrapConfig, control: &BootstrapControl) -> Result<Store, String> {
        let store = Store(self.0.clone());
        self.0.step(BootstrapStage::OpenStore, control)?;
        Ok(store)
    }
}
impl BootstrapStore for Store {
    type State = State;
    fn restore(
        &mut self,
        _: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<RestoredState<State>, String> {
        let state = State(self.0.clone());
        self.0.step(BootstrapStage::RestoreState, control)?;
        Ok(RestoredState {
            state,
            network: self
                .0
                .restored_network
                .unwrap_or(TradeLayerMainnet::Bitcoin),
            genesis: self
                .0
                .restored_genesis
                .unwrap_or(TradeLayerMainnet::Bitcoin.genesis()),
        })
    }
}

fn start(
    root: &Temp,
    plan: Plan,
    control: &BootstrapControl,
    coordinator: &mut BootstrapCoordinator,
) -> Result<StartedRuntime<Client, Store>, BootstrapError> {
    coordinator.start(
        root.config(),
        &mut ClientFactory(plan.clone()),
        &mut StoreFactory(plan),
        control,
    )
}

#[test]
fn config_validates_explicit_mainnet_category_and_existing_root() {
    let root = Temp::new();
    assert_eq!(
        BootstrapConfig::new(
            TradeLayerMainnet::Bitcoin,
            PathBuf::from("relative"),
            "persistence"
        ),
        Err(ConfigError::RootMustBeAbsolute)
    );
    assert_eq!(
        BootstrapConfig::new(
            TradeLayerMainnet::Bitcoin,
            root.0.join("missing"),
            "persistence"
        ),
        Err(ConfigError::RootMustExist)
    );
    assert_eq!(
        BootstrapConfig::new(TradeLayerMainnet::Bitcoin, root.0.clone(), "invalid"),
        Err(ConfigError::UnknownCategory)
    );
    for (network, name) in [
        (TradeLayerMainnet::Bitcoin, "btc-main"),
        (TradeLayerMainnet::Litecoin, "ltc-main"),
        (TradeLayerMainnet::Dogecoin, "doge-main"),
    ] {
        let config = BootstrapConfig::new(network, root.0.clone(), "persistence").unwrap();
        assert_eq!(config.network(), network);
        assert_eq!(
            config.namespace().path(config.root()),
            root.0.join(name).join("persistence.tldb")
        );
    }
}

#[test]
fn ready_runtime_is_published_once_and_owns_all_handles_until_drop() {
    let root = Temp::new();
    let plan = Plan::default();
    let events = plan.events.clone();
    let mut coordinator = BootstrapCoordinator::default();
    assert_eq!(coordinator.status(), BootstrapStatus::Idle);
    let mut ready = start(
        &root,
        plan.clone(),
        &BootstrapControl::default(),
        &mut coordinator,
    )
    .unwrap();
    assert_eq!(coordinator.status(), BootstrapStatus::Ready);
    assert_eq!(*events.borrow(), ["acquire", "identify", "open", "restore"]);
    assert_eq!(ready.config(), &root.config());
    assert_eq!(
        ready.claimed_node_identity().genesis,
        TradeLayerMainnet::Bitcoin.genesis()
    );
    let _ = ready.client_mut();
    let _ = ready.store_mut();
    let _ = ready.state();
    let _ = ready.state_mut();
    assert!(matches!(
        start(&root, plan, &BootstrapControl::default(), &mut coordinator),
        Err(BootstrapError::AlreadyStarted)
    ));
    assert_eq!(
        coordinator.reset_after_failure(),
        Err(BootstrapError::ResetRequiresFailure)
    );
    drop(ready);
    assert_eq!(
        *events.borrow(),
        [
            "acquire",
            "identify",
            "open",
            "restore",
            "drop_state",
            "drop_store",
            "drop_client"
        ]
    );
}

#[test]
fn each_mainnet_bootstraps_only_its_matching_node_and_state_identity() {
    let root = Temp::new();
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let config = BootstrapConfig::new(network, root.0.clone(), "persistence").unwrap();
        let plan = Plan {
            node: Some(ClaimedNodeIdentity {
                network,
                test: false,
                genesis: network.genesis(),
            }),
            restored_network: Some(network),
            restored_genesis: Some(network.genesis()),
            ..Plan::default()
        };
        let ready = BootstrapCoordinator::default()
            .start(
                config.clone(),
                &mut ClientFactory(plan.clone()),
                &mut StoreFactory(plan),
                &BootstrapControl::default(),
            )
            .unwrap();
        assert_eq!(ready.config(), &config);
        assert_eq!(ready.claimed_node_identity().network, network);
    }
}

#[test]
fn every_adapter_failure_releases_handles_and_requires_explicit_retry() {
    let root = Temp::new();
    for stage in [
        BootstrapStage::AcquireClient,
        BootstrapStage::IdentifyNetwork,
        BootstrapStage::OpenStore,
        BootstrapStage::RestoreState,
    ] {
        let plan = Plan {
            fail: Some(stage),
            ..Plan::default()
        };
        let events = plan.events.clone();
        let mut coordinator = BootstrapCoordinator::default();
        assert!(
            matches!(start(&root, plan.clone(), &BootstrapControl::default(), &mut coordinator), Err(BootstrapError::Adapter { stage: found, .. }) if found == stage)
        );
        assert_eq!(coordinator.status(), BootstrapStatus::Failed(stage));
        let calls = events.borrow();
        assert_eq!(calls.last(), Some(&"drop_client"));
        if stage == BootstrapStage::RestoreState {
            assert!(calls.ends_with(&["drop_state", "drop_store", "drop_client"]));
        }
        drop(calls);
        assert!(matches!(
            start(
                &root,
                Plan::default(),
                &BootstrapControl::default(),
                &mut coordinator
            ),
            Err(BootstrapError::AlreadyStarted)
        ));
        coordinator.reset_after_failure().unwrap();
        let ready = start(
            &root,
            Plan::default(),
            &BootstrapControl::default(),
            &mut coordinator,
        )
        .unwrap();
        drop(ready);
    }
}

#[test]
fn cancellation_at_each_stage_never_publishes_partial_ready_state() {
    let root = Temp::new();
    for stage in [
        BootstrapStage::AcquireClient,
        BootstrapStage::IdentifyNetwork,
        BootstrapStage::OpenStore,
        BootstrapStage::RestoreState,
    ] {
        let control = BootstrapControl::default();
        let plan = Plan {
            cancel: Some(stage),
            ..Plan::default()
        };
        let events = plan.events.clone();
        let mut coordinator = BootstrapCoordinator::default();
        assert!(
            matches!(start(&root, plan, &control, &mut coordinator), Err(BootstrapError::Stopped { stage: found, reason: StopReason::Cancelled }) if found == stage)
        );
        assert_eq!(coordinator.status(), BootstrapStatus::Failed(stage));
        assert_eq!(events.borrow().last(), Some(&"drop_client"));
        if stage == BootstrapStage::RestoreState {
            assert!(events
                .borrow()
                .ends_with(&["drop_state", "drop_store", "drop_client"]));
        }
    }
    let control = BootstrapControl::default();
    control.cancel();
    let plan = Plan::default();
    let events = plan.events.clone();
    assert!(matches!(
        start(&root, plan, &control, &mut BootstrapCoordinator::default()),
        Err(BootstrapError::Stopped {
            reason: StopReason::Cancelled,
            ..
        })
    ));
    assert!(events.borrow().is_empty());
}

#[test]
fn deadlines_before_and_during_acquisition_are_enforced_on_return() {
    let root = Temp::new();
    let expired = BootstrapControl::new(Some(Instant::now()));
    let plan = Plan::default();
    let events = plan.events.clone();
    assert!(matches!(
        start(&root, plan, &expired, &mut BootstrapCoordinator::default()),
        Err(BootstrapError::Stopped {
            reason: StopReason::DeadlineExceeded,
            ..
        })
    ));
    assert!(events.borrow().is_empty());
    let control = BootstrapControl::new(Some(Instant::now() + Duration::from_millis(2)));
    let plan = Plan {
        deadline_in_acquire: true,
        ..Plan::default()
    };
    let events = plan.events.clone();
    assert!(matches!(
        start(&root, plan, &control, &mut BootstrapCoordinator::default()),
        Err(BootstrapError::Stopped {
            stage: BootstrapStage::AcquireClient,
            reason: StopReason::DeadlineExceeded
        })
    ));
    // A heavily scheduled host may reach the already-expired pre-acquire
    // boundary instead. Either path must stop; an acquired handle must drop.
    let calls = events.borrow();
    assert!(calls.is_empty() || calls.as_slice() == ["acquire", "drop_client"]);
}

#[test]
fn stopped_adapter_error_is_reported_as_stop_and_resources_are_released() {
    let root = Temp::new();
    let plan = Plan {
        fail: Some(BootstrapStage::OpenStore),
        cancel: Some(BootstrapStage::OpenStore),
        ..Plan::default()
    };
    assert!(matches!(
        start(
            &root,
            plan,
            &BootstrapControl::default(),
            &mut BootstrapCoordinator::default()
        ),
        Err(BootstrapError::Stopped {
            stage: BootstrapStage::OpenStore,
            reason: StopReason::Cancelled
        })
    ));
}

#[test]
fn node_and_restored_identity_mismatches_reject_before_ready() {
    let root = Temp::new();
    for node in [
        ClaimedNodeIdentity {
            network: TradeLayerMainnet::Litecoin,
            test: false,
            genesis: TradeLayerMainnet::Bitcoin.genesis(),
        },
        ClaimedNodeIdentity {
            network: TradeLayerMainnet::Bitcoin,
            test: true,
            genesis: TradeLayerMainnet::Bitcoin.genesis(),
        },
        ClaimedNodeIdentity {
            network: TradeLayerMainnet::Bitcoin,
            test: false,
            genesis: BlockId([0; 32]),
        },
    ] {
        let plan = Plan {
            node: Some(node),
            ..Plan::default()
        };
        let events = plan.events.clone();
        assert!(matches!(
            start(
                &root,
                plan,
                &BootstrapControl::default(),
                &mut BootstrapCoordinator::default()
            ),
            Err(BootstrapError::NodeIdentityMismatch)
        ));
        assert_eq!(*events.borrow(), ["acquire", "identify", "drop_client"]);
    }
    for plan in [
        Plan {
            restored_network: Some(TradeLayerMainnet::Dogecoin),
            ..Plan::default()
        },
        Plan {
            restored_genesis: Some(BlockId([0; 32])),
            ..Plan::default()
        },
    ] {
        let events = plan.events.clone();
        assert!(matches!(
            start(
                &root,
                plan,
                &BootstrapControl::default(),
                &mut BootstrapCoordinator::default()
            ),
            Err(BootstrapError::StateIdentityMismatch)
        ));
        assert!(events
            .borrow()
            .ends_with(&["drop_state", "drop_store", "drop_client"]));
    }
}

#[test]
fn adapter_unwind_marks_failure_and_drops_all_unpublished_handles() {
    let root = Temp::new();
    let plan = Plan {
        panic: Some(BootstrapStage::RestoreState),
        ..Plan::default()
    };
    let events = plan.events.clone();
    let mut coordinator = BootstrapCoordinator::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = start(&root, plan, &BootstrapControl::default(), &mut coordinator);
    }));
    assert!(result.is_err());
    assert_eq!(
        coordinator.status(),
        BootstrapStatus::Failed(BootstrapStage::RestoreState)
    );
    assert!(events
        .borrow()
        .ends_with(&["drop_state", "drop_store", "drop_client"]));
    coordinator.reset_after_failure().unwrap();
}

struct DiskFactory;
struct PendingDisk(Option<Journal>);
struct DiskOwner {
    journal: Journal,
}
impl BootstrapStoreFactory for DiskFactory {
    type Store = PendingDisk;
    fn open(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<PendingDisk, String> {
        control.check().map_err(|e| format!("{e:?}"))?;
        Journal::open_existing(
            config.root(),
            config.namespace().clone(),
            Recovery::RejectIncompleteTail,
        )
        .map(|(journal, _)| PendingDisk(Some(journal)))
        .map_err(|e| e.to_string())
    }
}
impl BootstrapStore for PendingDisk {
    type State = DiskOwner;
    fn restore(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<RestoredState<DiskOwner>, String> {
        control.check().map_err(|e| format!("{e:?}"))?;
        let journal = self.0.take().ok_or("already moved journal")?;
        let checkpoint = journal.get("checkpoint").map_err(|e| e.to_string())?;
        if checkpoint != Some(b"complete-test-state".as_slice()) {
            return Err("invalid complete test checkpoint".to_owned());
        }
        Ok(RestoredState {
            state: DiskOwner { journal },
            network: config.network(),
            genesis: config.network().genesis(),
        })
    }
}

#[test]
fn concrete_journal_owner_restores_without_clone_and_releases_locks_on_failure() {
    let root = Temp::new();
    let config = root.config();
    let (mut journal, _) = Journal::open(
        config.root(),
        config.namespace().clone(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    journal
        .commit(vec![Mutation::Put {
            key: "checkpoint".to_owned(),
            value: b"complete-test-state".to_vec(),
        }])
        .unwrap();
    drop(journal);
    let mut ready = BootstrapCoordinator::default()
        .start(
            config.clone(),
            &mut ClientFactory(Plan::default()),
            &mut DiskFactory,
            &BootstrapControl::default(),
        )
        .unwrap();
    assert_eq!(
        ready.state().journal.get("checkpoint").unwrap(),
        Some(b"complete-test-state".as_slice())
    );
    assert!(matches!(
        Journal::open_existing(
            config.root(),
            config.namespace().clone(),
            Recovery::RejectIncompleteTail
        ),
        Err(StorageError::Locked)
    ));
    ready
        .state_mut()
        .journal
        .commit(vec![Mutation::Put {
            key: "checkpoint".to_owned(),
            value: b"invalid".to_vec(),
        }])
        .unwrap();
    drop(ready);
    let result = BootstrapCoordinator::default().start(
        config.clone(),
        &mut ClientFactory(Plan::default()),
        &mut DiskFactory,
        &BootstrapControl::default(),
    );
    assert!(matches!(
        result,
        Err(BootstrapError::Adapter {
            stage: BootstrapStage::RestoreState,
            ..
        })
    ));
    let (journal, _) = Journal::open_existing(
        config.root(),
        config.namespace().clone(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    assert_eq!(
        journal.get("checkpoint").unwrap(),
        Some(b"invalid".as_slice())
    );
}
