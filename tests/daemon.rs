use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tradelayer_core::bootstrap::{BootstrapControl, ClaimedNodeIdentity};
use tradelayer_core::chain::{BlockId, Tip};
use tradelayer_core::daemon::*;
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::raw_block_ingest::UnverifiedRawBlockView;
use tradelayer_core::read_only::decode_hex;
use tradelayer_core::storage::Recovery;
static SERIAL: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "tl-daemon-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn anchor(n: TradeLayerMainnet) -> Tip {
    Tip {
        height: 0,
        id: n.genesis(),
    }
}
fn block(parent: BlockId, nonce: u32) -> (BlockId, Vec<u8>) {
    let mut raw =
        decode_hex(include_str!("../fixtures/raw_block_ingest_synthetic.hex").trim()).unwrap();
    let mut p = parent.0;
    p.reverse();
    raw[4..36].copy_from_slice(&p);
    raw[76..80].copy_from_slice(&nonce.to_le_bytes());
    let mut h = UnverifiedRawBlockView::parse(&raw, None)
        .unwrap()
        .header_hash_raw();
    h.reverse();
    (BlockId(h), raw)
}
#[derive(Clone)]
struct Mock(Rc<RefCell<Node>>);
struct Node {
    network: TradeLayerMainnet,
    tip: Tip,
    hashes: BTreeMap<u64, BlockId>,
    blocks: BTreeMap<BlockId, Vec<u8>>,
    calls: Vec<String>,
    errors: VecDeque<ObservationRpcError>,
    wrong_identity: bool,
    race: Option<(u64, BlockId)>,
    cancel_on_hash: Option<BootstrapControl>,
    cancel_on_call: Option<(String, BootstrapControl, ObservationRpcError)>,
}
impl Mock {
    fn new(network: TradeLayerMainnet) -> Self {
        Self(Rc::new(RefCell::new(Node {
            network,
            tip: anchor(network),
            hashes: BTreeMap::from([(0, network.genesis())]),
            blocks: BTreeMap::new(),
            calls: vec![],
            errors: VecDeque::new(),
            wrong_identity: false,
            race: None,
            cancel_on_hash: None,
            cancel_on_call: None,
        })))
    }
    fn add(&self, height: u64, parent: BlockId, nonce: u32) -> BlockId {
        let (h, r) = block(parent, nonce);
        let mut n = self.0.borrow_mut();
        n.blocks.insert(h, r);
        n.hashes.insert(height, h);
        n.tip = Tip { height, id: h };
        h
    }
    fn call(&self, name: &str) -> Result<(), ObservationRpcError> {
        let mut n = self.0.borrow_mut();
        n.calls.push(name.into());
        if n.cancel_on_call
            .as_ref()
            .is_some_and(|(at, _, _)| at == name)
        {
            let (_, control, error) = n.cancel_on_call.take().unwrap();
            control.cancel();
            return Err(error);
        }
        if let Some(e) = n.errors.pop_front() {
            return Err(e);
        }
        Ok(())
    }
}
impl ObservationRpc for Mock {
    fn verify_identity(&mut self) -> Result<ClaimedNodeIdentity, ObservationRpcError> {
        self.call("identity")?;
        let n = self.0.borrow();
        Ok(ClaimedNodeIdentity {
            network: n.network,
            test: n.wrong_identity,
            genesis: n.network.genesis(),
        })
    }
    fn reported_tip(&mut self) -> Result<Tip, ObservationRpcError> {
        self.call("tip")?;
        Ok(self.0.borrow().tip)
    }
    fn block_hash(&mut self, height: u64) -> Result<BlockId, ObservationRpcError> {
        self.call(&format!("hash:{height}"))?;
        let mut n = self.0.borrow_mut();
        if let Some(control) = n.cancel_on_hash.take() {
            control.cancel();
        }
        if let Some((at, h)) = n.race {
            if at == height {
                n.hashes.insert(height, h);
                n.race = None;
            }
        }
        n.hashes
            .get(&height)
            .copied()
            .ok_or(ObservationRpcError::Unavailable)
    }
    fn raw_block(&mut self, hash: BlockId) -> Result<Vec<u8>, ObservationRpcError> {
        self.call("raw")?;
        self.0
            .borrow()
            .blocks
            .get(&hash)
            .cloned()
            .ok_or(ObservationRpcError::Unavailable)
    }
}
fn create(root: &Directory, n: TradeLayerMainnet, source: Mock) -> Daemon<Mock> {
    let mut c = ObserverConfig::new(n, anchor(n)).unwrap();
    c.scan.workers = 1;
    Daemon::new(source, ObservationStore::initialize(&root.0, c).unwrap())
}
fn step(d: &mut Daemon<Mock>, now: &mut u64) -> StepOutcome {
    let result = d.step(*now, &BootstrapControl::default()).unwrap();
    *now += 1;
    result
}
fn recorded(d: &mut Daemon<Mock>, source: &Mock, now: &mut u64) -> ObservationHead {
    for _ in 0..25 {
        let before = source.0.borrow().calls.len();
        let result = step(d, now);
        assert!(source.0.borrow().calls.len() - before <= 1);
        match result {
            StepOutcome::Recorded { head } => return head,
            StepOutcome::Quarantined { reason } => panic!("unexpected quarantine {reason:?}"),
            _ => {}
        }
    }
    panic!("not recorded")
}
fn quarantined(d: &mut Daemon<Mock>, now: &mut u64) -> QuarantineReason {
    for _ in 0..100 {
        if let StepOutcome::Quarantined { reason } = step(d, now) {
            return reason;
        }
    }
    panic!("not quarantined")
}

#[test]
fn persists_raw_blocks_cursor_and_exact_restart_on_bitcoin_and_litecoin() {
    for network in [TradeLayerMainnet::Bitcoin, TradeLayerMainnet::Litecoin] {
        let root = Directory::new();
        let source = Mock::new(network);
        let first = source.add(1, network.genesis(), 1);
        source.add(2, first, 2);
        let mut d = create(&root, network, source.clone());
        let mut now = 0;
        let h1 = recorded(&mut d, &source, &mut now);
        assert_eq!(
            h1.tip,
            Tip {
                height: 1,
                id: first
            }
        );
        let h2 = recorded(&mut d, &source, &mut now);
        assert_eq!(h2.tip.height, 2);
        assert_eq!(
            d.store().records().unwrap()[0].raw(),
            source.0.borrow().blocks[&first]
        );
        assert_eq!(
            d.store().records().unwrap()[0].summary(),
            Some(BlockSummary {
                transaction_count: 4,
                candidate_count: 2
            })
        );
        let config = d.store().config().unwrap().clone();
        let (_, store) = d.into_parts();
        drop(store);
        let (store, report) =
            ObservationStore::load(&root.0, config, h2, Recovery::RejectIncompleteTail).unwrap();
        assert_eq!(report.discarded_bytes, 0);
        assert_eq!(store.head().unwrap(), h2);
        assert_eq!(store.canonical_headers().unwrap().len(), 2);
        let mut d = Daemon::new(source.clone(), store);
        assert!(!d.status().unwrap().identity_checked);
        assert_eq!(step(&mut d, &mut now), StepOutcome::Progress);
        assert!(d.status().unwrap().identity_checked);
    }
}

#[test]
fn side_branch_is_retained_and_only_observation_selection_rewinds() {
    let n = TradeLayerMainnet::Bitcoin;
    let root = Directory::new();
    let source = Mock::new(n);
    let a = source.add(1, n.genesis(), 1);
    let b = source.add(2, a, 2);
    let mut d = create(&root, n, source.clone());
    let mut now = 0;
    recorded(&mut d, &source, &mut now);
    recorded(&mut d, &source, &mut now);
    let branch = source.add(2, a, 3);
    assert_eq!(
        quarantined(&mut d, &mut now),
        QuarantineReason::ForkDetected
    );
    for _ in 0..5 {
        step(&mut d, &mut now);
        if d.status().unwrap().phase == ObserverPhase::Idle {
            break;
        }
    }
    let evidence = d.store().fork_evidence().unwrap().unwrap();
    assert_eq!(evidence.common_ancestor, Some(Tip { height: 1, id: a }));
    let current = d.head().unwrap();
    assert!(d.resume(current).is_err());
    assert!(d
        .reconcile(
            current,
            Tip {
                height: 1,
                id: BlockId([9; 32])
            }
        )
        .is_err());
    let reconciled = d.reconcile(current, Tip { height: 1, id: a }).unwrap();
    assert_eq!(d.store().records().unwrap().len(), 2);
    assert!(d.reconcile(current, Tip { height: 1, id: a }).is_err());
    let head = recorded(&mut d, &source, &mut now);
    assert_eq!(
        head.tip,
        Tip {
            height: 2,
            id: branch
        }
    );
    assert_eq!(d.store().records().unwrap().len(), 3);
    assert_eq!(d.store().records().unwrap()[1].header().id, b);
    assert_ne!(head, reconciled);
}

#[test]
fn reported_retreat_and_changed_anchor_stop_without_dropping_material() {
    let n = TradeLayerMainnet::Bitcoin;
    let root = Directory::new();
    let source = Mock::new(n);
    source.add(1, n.genesis(), 1);
    let mut d = create(&root, n, source.clone());
    let mut now = 0;
    recorded(&mut d, &source, &mut now);
    source.0.borrow_mut().tip = anchor(n);
    assert_eq!(
        quarantined(&mut d, &mut now),
        QuarantineReason::CursorAboveTip
    );
    step(&mut d, &mut now);
    assert_eq!(
        d.store().fork_evidence().unwrap().unwrap().common_ancestor,
        Some(anchor(n))
    );
    assert_eq!(d.head().unwrap().tip.height, 1);
    assert_eq!(d.store().records().unwrap().len(), 1);
}

#[test]
fn retry_backoff_checks_identity_and_exhausts_selected_read_failures() {
    let n = TradeLayerMainnet::Bitcoin;
    let root = Directory::new();
    let source = Mock::new(n);
    let mut d = create(&root, n, source.clone());
    source
        .0
        .borrow_mut()
        .errors
        .push_back(ObservationRpcError::Transient);
    let result = d.step(0, &BootstrapControl::default()).unwrap();
    assert_eq!(result, StepOutcome::Waiting { until_ms: 1000 });
    let calls = source.0.borrow().calls.len();
    assert_eq!(d.step(999, &BootstrapControl::default()).unwrap(), result);
    assert_eq!(source.0.borrow().calls.len(), calls);
    d.step(1000, &BootstrapControl::default()).unwrap();
    d.step(1001, &BootstrapControl::default()).unwrap();
    assert_eq!(source.0.borrow().calls.last().unwrap(), "identity");
    assert!(d.status().unwrap().identity_checked);
    source
        .0
        .borrow_mut()
        .errors
        .extend([ObservationRpcError::Transient; 3]);
    let mut now = 2000;
    let mut reason = None;
    for _ in 0..20 {
        let r = d.step(now, &BootstrapControl::default()).unwrap();
        if let StepOutcome::Waiting { until_ms } = r {
            now = until_ms;
        } else {
            now += 1;
        }
        if let StepOutcome::Quarantined { reason: r } = r {
            reason = Some(r);
            break;
        }
    }
    assert_eq!(reason, Some(QuarantineReason::RetryExhausted));
}

#[test]
fn pause_stop_resume_and_cancellation_use_comparison_tokens() {
    let n = TradeLayerMainnet::Bitcoin;
    let root = Directory::new();
    let source = Mock::new(n);
    let mut d = create(&root, n, source.clone());
    let h = d.head().unwrap();
    let paused = d.pause(h).unwrap();
    assert!(d.resume(h).is_err());
    let calls = source.0.borrow().calls.len();
    assert_eq!(
        d.step(1, &BootstrapControl::default()).unwrap(),
        StepOutcome::Paused
    );
    assert_eq!(source.0.borrow().calls.len(), calls);
    let resumed = d.resume(paused).unwrap();
    assert_ne!(resumed, paused);
    let control = BootstrapControl::default();
    control.cancel();
    assert_eq!(d.step(2, &control).unwrap(), StepOutcome::Stopped);
    assert_eq!(d.status().unwrap().mode, ObserverMode::Stopped);
    assert_eq!(source.0.borrow().calls.len(), calls);
}

#[test]
fn dogecoin_opaque_raw_is_retained_and_never_advances_cursor() {
    let n = TradeLayerMainnet::Dogecoin;
    let root = Directory::new();
    let source = Mock::new(n);
    source.add(1, n.genesis(), 1);
    let mut d = create(&root, n, source.clone());
    let mut now = 0;
    assert_eq!(
        quarantined(&mut d, &mut now),
        QuarantineReason::UnsupportedDogecoinLayout
    );
    assert_eq!(d.head().unwrap().tip, anchor(n));
    assert_eq!(d.store().records().unwrap().len(), 1);
    assert_eq!(d.store().records().unwrap()[0].summary(), None);
    assert!(d.resume(d.head().unwrap()).is_err());
}

#[test]
fn scan_rejects_parent_hash_and_merkle_mismatches_before_cursor_commit() {
    let n = TradeLayerMainnet::Bitcoin;
    for kind in [0, 1, 2] {
        let root = Directory::new();
        let source = Mock::new(n);
        let first = source.add(1, n.genesis(), 1);
        {
            let mut node = source.0.borrow_mut();
            let raw = node.blocks.get_mut(&first).unwrap();
            match kind {
                0 => raw[0] ^= 1,
                1 => {
                    let other = BlockId([3; 32]);
                    let (h, r) = block(other, 2);
                    node.blocks.remove(&first);
                    node.blocks.insert(h, r);
                    node.hashes.insert(1, h);
                    node.tip = Tip { height: 1, id: h };
                }
                _ => {
                    let last = raw.len() - 1;
                    raw[last] ^= 1;
                }
            }
        }
        let mut d = create(&root, n, source);
        let mut now = 0;
        assert_eq!(
            quarantined(&mut d, &mut now),
            match kind {
                0 => QuarantineReason::HashMismatch,
                1 => QuarantineReason::ParentMismatch,
                _ => QuarantineReason::InvalidRawBlock,
            }
        );
        assert_eq!(d.head().unwrap().tip, anchor(n));
        assert!(d.store().records().unwrap().is_empty());
    }
}

#[test]
fn canonical_hash_change_during_fetch_quarantines_without_publication() {
    let n = TradeLayerMainnet::Bitcoin;
    let root = Directory::new();
    let source = Mock::new(n);
    source.add(1, n.genesis(), 1);
    let mut d = create(&root, n, source.clone());
    let mut now = 0;
    while d.status().unwrap().phase != ObserverPhase::Recheck {
        step(&mut d, &mut now);
    }
    source.0.borrow_mut().race = Some((1, BlockId([7; 32])));
    assert_eq!(
        quarantined(&mut d, &mut now),
        QuarantineReason::ForkDetected
    );
    assert_eq!(d.head().unwrap().tip, anchor(n));
    assert!(d.store().records().unwrap().is_empty());
}

#[test]
fn spool_limits_do_not_evict_or_silently_skip_a_block() {
    let n = TradeLayerMainnet::Bitcoin;
    let root = Directory::new();
    let source = Mock::new(n);
    let h = source.add(1, n.genesis(), 1);
    source.add(2, h, 2);
    let mut c = ObserverConfig::new(n, anchor(n)).unwrap();
    c.max_blocks = 1;
    c.scan.workers = 1;
    let mut d = Daemon::new(
        source.clone(),
        ObservationStore::initialize(&root.0, c).unwrap(),
    );
    let mut now = 0;
    recorded(&mut d, &source, &mut now);
    assert_eq!(quarantined(&mut d, &mut now), QuarantineReason::SpoolFull);
    assert_eq!(d.head().unwrap().tip.height, 1);
    assert_eq!(d.store().records().unwrap().len(), 1);
    assert!(d.resume(d.head().unwrap()).is_err());
}

#[test]
fn identity_and_nonretryable_failures_are_terminal_until_explicit_resume() {
    let n = TradeLayerMainnet::Bitcoin;
    for (err, reason) in [
        (
            ObservationRpcError::WrongIdentity,
            QuarantineReason::WrongIdentity,
        ),
        (
            ObservationRpcError::InvalidResponse,
            QuarantineReason::InvalidRpcResponse,
        ),
        (
            ObservationRpcError::Unavailable,
            QuarantineReason::HistoricalBlockUnavailable,
        ),
    ] {
        let root = Directory::new();
        let source = Mock::new(n);
        source.0.borrow_mut().errors.push_back(err);
        let mut d = create(&root, n, source.clone());
        let mut now = 0;
        assert_eq!(quarantined(&mut d, &mut now), reason);
        let calls = source.0.borrow().calls.len();
        step(&mut d, &mut now);
        assert_eq!(source.0.borrow().calls.len(), calls);
        d.resume(d.head().unwrap()).unwrap();
        step(&mut d, &mut now);
        assert!(d.status().unwrap().identity_checked);
    }
}

#[test]
fn wrong_expected_head_config_missing_store_and_live_lock_fail_closed() {
    let n = TradeLayerMainnet::Bitcoin;
    let root = Directory::new();
    let source = Mock::new(n);
    let d = create(&root, n, source);
    let config = d.store().config().unwrap().clone();
    let h = d.head().unwrap();
    assert!(
        ObservationStore::load(&root.0, config.clone(), h, Recovery::RejectIncompleteTail).is_err()
    );
    let (_, store) = d.into_parts();
    drop(store);
    let mut wrong = h;
    wrong.sequence += 1;
    assert!(matches!(
        ObservationStore::load(
            &root.0,
            config.clone(),
            wrong,
            Recovery::RejectIncompleteTail
        ),
        Err(DaemonError::StaleHead)
    ));
    let mut wrong_config = config.clone();
    wrong_config.poll_interval_ms += 1;
    assert!(
        ObservationStore::load(&root.0, wrong_config, h, Recovery::RejectIncompleteTail).is_err()
    );
    let missing = Directory::new();
    assert!(ObservationStore::load(&missing.0, config, h, Recovery::RejectIncompleteTail).is_err());
    assert!(std::fs::read_dir(&missing.0).unwrap().next().is_none());
}

#[test]
fn stopped_fork_requires_reconciliation_and_probe_failure_is_not_history_exhaustion() {
    let n = TradeLayerMainnet::Bitcoin;
    let root = Directory::new();
    let source = Mock::new(n);
    source.add(1, n.genesis(), 1);
    let mut d = create(&root, n, source.clone());
    let mut now = 0;
    recorded(&mut d, &source, &mut now);
    source.0.borrow_mut().hashes.insert(1, BlockId([8; 32]));
    assert_eq!(
        quarantined(&mut d, &mut now),
        QuarantineReason::ForkDetected
    );
    source
        .0
        .borrow_mut()
        .errors
        .push_back(ObservationRpcError::Unavailable);
    step(&mut d, &mut now);
    let f = d.store().fork_evidence().unwrap().unwrap();
    assert!(f.probe_failed);
    assert!(!f.history_exhausted);
    let head = d.head().unwrap();
    let stopped = d.stop(head).unwrap();
    assert!(d.resume(stopped).is_err());
    let after = d.reconcile(stopped, anchor(n)).unwrap();
    assert_eq!(after.tip, anchor(n));
    assert_eq!(d.store().records().unwrap().len(), 1);
}

#[test]
fn complete_source_vm_inventory_and_concrete_defects_remain_characterized() {
    let text = include_str!("../fixtures/daemon_vectors.tsv");
    let inventory = text.lines().find(|line| line.starts_with("M\t")).unwrap();
    let fields: Vec<_> = inventory.split('\t').collect();
    assert_eq!(fields[1], "30");
    assert_eq!(fields[2].split(',').count(), 30);
    for name in [
        "singleton-published-before-client",
        "catchup-funding-order",
        "live-pre-apply-checkpoint",
        "realtime-unbound-activation",
        "fetch-four-attempts",
        "recovery-resumes-saved-height",
        "shutdown-no-flush",
    ] {
        assert!(
            text.lines()
                .any(|line| line.starts_with(&format!("V\t{name}\t"))),
            "missing actual-source observation {name}"
        );
    }
}

#[test]
fn cancellation_inside_successful_recheck_stops_before_raw_cursor_publication() {
    let n = TradeLayerMainnet::Bitcoin;
    let root = Directory::new();
    let source = Mock::new(n);
    source.add(1, n.genesis(), 1);
    let mut d = create(&root, n, source.clone());
    let mut now = 0;
    while d.status().unwrap().phase != ObserverPhase::Recheck {
        step(&mut d, &mut now);
    }
    let before = d.head().unwrap();
    let config = d.store().config().unwrap().clone();
    let control = BootstrapControl::default();
    source.0.borrow_mut().cancel_on_hash = Some(control.clone());
    assert_eq!(d.step(now, &control).unwrap(), StepOutcome::Stopped);
    let after = d.head().unwrap();
    assert_eq!(after.sequence, before.sequence + 1);
    assert_eq!(after.tip, anchor(n));
    assert!(d.store().records().unwrap().is_empty());
    assert_eq!(d.status().unwrap().mode, ObserverMode::Stopped);
    assert_eq!(d.step(now + 1, &control).unwrap(), StepOutcome::Stopped);
    assert_eq!(d.head().unwrap(), after);
    let (_, store) = d.into_parts();
    drop(store);
    let (restored, _) =
        ObservationStore::load(&root.0, config, after, Recovery::RejectIncompleteTail).unwrap();
    assert!(restored.records().unwrap().is_empty());
    assert_eq!(restored.mode().unwrap(), ObserverMode::Stopped);
}

#[test]
fn cancellation_returned_as_rpc_error_stops_in_every_rpc_phase() {
    for phase in [
        ObserverPhase::Identity,
        ObserverPhase::Tip,
        ObserverPhase::Cursor,
        ObserverPhase::NextHash,
        ObserverPhase::Fetch,
        ObserverPhase::Recheck,
        ObserverPhase::ForkSearch,
    ] {
        let n = TradeLayerMainnet::Bitcoin;
        let root = Directory::new();
        let source = Mock::new(n);
        source.add(1, n.genesis(), 1);
        let mut d = create(&root, n, source.clone());
        let mut now = 0;
        if phase == ObserverPhase::ForkSearch {
            recorded(&mut d, &source, &mut now);
            source.0.borrow_mut().hashes.insert(1, BlockId([8; 32]));
            assert_eq!(
                quarantined(&mut d, &mut now),
                QuarantineReason::ForkDetected
            );
        }
        while d.status().unwrap().phase != phase {
            step(&mut d, &mut now);
            assert!(now < 100);
        }
        let before = d.head().unwrap();
        let records = d.store().records().unwrap().len();
        let evidence = d.store().fork_evidence().unwrap();
        let control = BootstrapControl::default();
        let method = match phase {
            ObserverPhase::Identity => "identity",
            ObserverPhase::Tip => "tip",
            ObserverPhase::Cursor => "hash:0",
            ObserverPhase::NextHash | ObserverPhase::Recheck | ObserverPhase::ForkSearch => {
                "hash:1"
            }
            ObserverPhase::Fetch => "raw",
            _ => unreachable!(),
        };
        source.0.borrow_mut().cancel_on_call = Some((
            method.into(),
            control.clone(),
            ObservationRpcError::Transient,
        ));
        assert_eq!(
            d.step(now, &control).unwrap(),
            StepOutcome::Stopped,
            "{phase:?}"
        );
        let after = d.head().unwrap();
        assert_eq!(after.sequence, before.sequence + 1);
        assert_eq!(after.tip, before.tip);
        assert_eq!(d.store().records().unwrap().len(), records);
        assert_eq!(d.store().fork_evidence().unwrap(), evidence);
        assert_eq!(d.status().unwrap().mode, ObserverMode::Stopped);
        assert_eq!(d.status().unwrap().retry_attempt, 0);
        let calls = source.0.borrow().calls.len();
        assert_eq!(d.step(now + 1, &control).unwrap(), StepOutcome::Stopped);
        assert_eq!(d.head().unwrap(), after);
        assert_eq!(source.0.borrow().calls.len(), calls);
    }
}
