//! `tradelayer-listenerd` as a child process over a seeded demo store.
//! The demo history is synthetic (see `listener_demo`); these tests check the
//! daemon's CLI, config, head discipline, loopback policy and HTTP surface.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tradelayer_core::client::RpcValue;
use tradelayer_core::listener_config::{
    head_token, parse_config, parse_expected_head, render_config, ListenerConfigError,
};
use tradelayer_core::listener_demo::demo_config;

const BIN: &str = env!("CARGO_BIN_EXE_tradelayer-listenerd");
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-listenerd-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("tl-listenerd-"));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn field(json: &str, name: &str) -> String {
    RpcValue::parse(json)
        .unwrap()
        .field(name)
        .unwrap()
        .unwrap()
        .as_string()
        .unwrap()
}

struct Demo {
    /// Holds the store and a sibling directory for stop files and configs.
    base: Directory,
    root: PathBuf,
    config: PathBuf,
    head: String,
    seed_output: String,
}
fn seeded() -> Demo {
    let base = Directory::new();
    let root = base.join("store");
    let config = base.join("demo.conf");
    let out = Command::new(BIN)
        .arg("--demo-seed")
        .arg(&root)
        .arg("--config")
        .arg(&config)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    let head = field(&text, "expectedHead");
    Demo {
        base,
        root,
        config,
        head,
        seed_output: text,
    }
}

struct Daemon {
    child: Child,
    addr: String,
    stdout: BufReader<std::process::ChildStdout>,
    stop: PathBuf,
}
fn serve_with(
    root: &Path,
    config: &Path,
    head: &str,
    stop: PathBuf,
    bind: &str,
) -> Result<Daemon, std::process::Output> {
    let mut child = Command::new(BIN)
        .arg("--root")
        .arg(root)
        .args(["--bind", bind, "--expected-head", head])
        .arg("--config")
        .arg(config)
        .arg("--stop-file")
        .arg(&stop)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    if line.is_empty() {
        return Err(child.wait_with_output().unwrap());
    }
    assert_eq!(field(&line, "event"), "listening");
    assert_eq!(
        RpcValue::parse(&line)
            .unwrap()
            .field("authenticated_admission")
            .unwrap()
            .unwrap()
            .as_json(),
        "false"
    );
    Ok(Daemon {
        child,
        addr: field(&line, "addr"),
        stdout,
        stop,
    })
}
impl Daemon {
    /// Creates the stop file and returns the exit code and the rest of stdout.
    fn stop(mut self) -> (i32, String) {
        std::fs::write(&self.stop, b"").unwrap();
        let end = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < end, "daemon ignored its stop file");
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut rest = String::new();
        self.stdout.read_to_string(&mut rest).unwrap();
        (status.code().unwrap(), rest)
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn http(addr: &str, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let request = if method == "POST" {
        format!("POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
    } else {
        format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n")
    };
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let status = response
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap();
    let body = response.split_once("\r\n\r\n").unwrap().1.to_owned();
    (status, body)
}

fn run(args: &[&std::ffi::OsStr]) -> (i32, String) {
    let out = Command::new(BIN).args(args).output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_demo_is_labelled_and_serves_its_seeded_history_over_http() {
    let demo = seeded();
    assert!(demo.seed_output.contains("DEMO DATA"));
    assert_eq!(
        RpcValue::parse(&demo.seed_output)
            .unwrap()
            .field("demo")
            .unwrap()
            .unwrap()
            .as_json(),
        "true"
    );
    assert!(std::fs::read_to_string(&demo.config)
        .unwrap()
        .starts_with("# DEMO DATA"));
    let daemon = serve_with(
        &demo.root,
        &demo.config,
        &demo.head,
        demo.base.join("stop"),
        "127.0.0.1:0",
    )
    .map_err(|o| String::from_utf8_lossy(&o.stderr).into_owned())
    .unwrap();
    assert!(daemon.addr.starts_with("127.0.0.1:"));

    // Balances: the trader holds tUSD (margin posted for the open position) and the side token.
    let (status, balances) = http(
        &daemon.addr,
        "POST",
        "/tl_getAllBalancesForAddress",
        r#"{"address":"DemoAlice"}"#,
    );
    assert_eq!(status, 200, "{balances}");
    assert!(balances.contains("\"propertyId\":7"));
    assert!(balances.contains("\"propertyId\":9"));
    assert!(balances.contains("\"contractMarginAtoms\":\"2000000000000\""));

    // Orderbook: the market maker's bids and the one ask the taker left alone.
    let (status, book) = http(
        &daemon.addr,
        "POST",
        "/tl_getContractOrderbook",
        r#"{"contractId":1}"#,
    );
    assert_eq!(status, 200, "{book}");
    assert!(book.contains("DemoMM"));
    assert!(book.contains("\"price\":\"99900\""));
    assert!(book.contains("\"price\":\"100300\""));
    assert!(!book.contains("\"price\":\"100100\""));

    // Positions: two fills left DemoAlice long 2 at the average price.
    let (status, positions) = http(&daemon.addr, "GET", "/walletPositions/DemoAlice", "");
    assert_eq!(status, 200, "{positions}");
    assert!(positions.contains("\"contracts\":\"2\""));
    assert!(positions.contains("\"averagePrice\":\"100150\""));
    let (status, maker) = http(&daemon.addr, "GET", "/walletPositions/DemoMM", "");
    assert_eq!(status, 200, "{maker}");
    assert!(maker.contains("\"contracts\":\"-2\""));

    // Status: served as restored, with no authenticated admission.
    let (status, init) = http(&daemon.addr, "GET", "/initStatus", "");
    assert_eq!(status, 200, "{init}");
    let init = RpcValue::parse(&init).unwrap();
    assert_eq!(
        init.field("authenticated_admission")
            .unwrap()
            .unwrap()
            .as_json(),
        "false"
    );
    assert_eq!(
        init.field("economicHead")
            .unwrap()
            .unwrap()
            .as_string()
            .unwrap(),
        demo.head
    );
    assert_eq!(
        init.field("state")
            .unwrap()
            .unwrap()
            .field("height")
            .unwrap()
            .unwrap()
            .as_json(),
        "4"
    );

    // tl_check_kyc and tl_check_commits are protocol queries answered from TradeLayer state. The
    // demo store holds no attestation, clearlist admin or channel commit, so the verdict is
    // "disabled" (the key keeps its trailing space) and the commit list is empty.
    let (status, body) = http(
        &daemon.addr,
        "POST",
        "/tl_check_kyc",
        r#"{"address":"DemoAlice"}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, "{\"result: \":\"disabled\"}");
    let (status, body) = http(
        &daemon.addr,
        "POST",
        "/tl_check_commits",
        r#"{"address":"DemoAlice"}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, "[]");
    // A mutating or unmapped call stays refused.
    let (status, _) = http(&daemon.addr, "POST", "/tl_initmain", r#"{"test":true}"#);
    assert_eq!(status, 503);

    let (code, rest) = daemon.stop();
    assert_eq!(code, 0);
    assert_eq!(field(rest.lines().next().unwrap(), "event"), "stopped");
}

#[test]
fn the_store_restarts_to_the_same_head_after_a_stop() {
    let demo = seeded();
    for round in 0..2 {
        let daemon = serve_with(
            &demo.root,
            &demo.config,
            &demo.head,
            demo.base.join(&format!("stop{round}")),
            "127.0.0.1:0",
        )
        .map_err(|o| String::from_utf8_lossy(&o.stderr).into_owned())
        .unwrap();
        let (status, init) = http(&daemon.addr, "GET", "/initStatus", "");
        assert_eq!(status, 200);
        assert_eq!(field(&init, "economicHead"), demo.head);
        assert_eq!(daemon.stop().0, 0);
    }
}

#[test]
fn non_loopback_binds_are_refused_before_the_store_is_opened() {
    let demo = seeded();
    for bind in ["0.0.0.0:0", "[::]:0", "8.8.8.8:3000", "192.168.1.5:3000"] {
        let refused = serve_with(
            &demo.root,
            &demo.config,
            &demo.head,
            demo.base.join("stop-bind"),
            bind,
        )
        .err()
        .unwrap_or_else(|| panic!("{bind} was served"));
        assert_eq!(refused.status.code(), Some(2), "{bind}");
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(stderr.contains("loopback"), "{stderr}");
    }
    // The store was never opened, so a loopback start still works afterwards.
    let daemon = serve_with(
        &demo.root,
        &demo.config,
        &demo.head,
        demo.base.join("stop-ok"),
        "127.0.0.1:0",
    )
    .map_err(|o| String::from_utf8_lossy(&o.stderr).into_owned())
    .unwrap();
    assert_eq!(daemon.stop().0, 0);
}

#[test]
fn a_wrong_expected_head_is_refused() {
    let demo = seeded();
    let mut wrong = demo.head.clone();
    let last = wrong.pop().unwrap();
    wrong.push(if last == '0' { '1' } else { '0' });
    let refused = serve_with(
        &demo.root,
        &demo.config,
        &wrong,
        demo.base.join("stop-wrong"),
        "127.0.0.1:0",
    )
    .err()
    .expect("a wrong head was served");
    assert_eq!(refused.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("StaleHead"));
    // A head with the wrong sequence is refused the same way.
    let mut parts: Vec<String> = demo.head.split(':').map(str::to_owned).collect();
    parts[0] = "4".into();
    let refused = serve_with(
        &demo.root,
        &demo.config,
        &parts.join(":"),
        demo.base.join("stop-wrong2"),
        "127.0.0.1:0",
    )
    .err()
    .expect("a wrong sequence was served");
    assert_eq!(refused.status.code(), Some(4));
    // A malformed token is a usage error.
    let refused = serve_with(
        &demo.root,
        &demo.config,
        "not-a-head",
        demo.base.join("stop-wrong3"),
        "127.0.0.1:0",
    )
    .err()
    .expect("a malformed head was served");
    assert_eq!(refused.status.code(), Some(2));
}

#[test]
fn a_config_that_differs_from_the_stores_is_refused_and_an_invalid_one_is_a_config_error() {
    let demo = seeded();
    let text = std::fs::read_to_string(&demo.config).unwrap();
    let different = demo.base.join("different.conf");
    std::fs::write(
        &different,
        text.replace(
            "initial_margin_per_contract = 10000",
            "initial_margin_per_contract = 9999",
        ),
    )
    .unwrap();
    let refused = serve_with(
        &demo.root,
        &different,
        &demo.head,
        demo.base.join("stop-cfg"),
        "127.0.0.1:0",
    )
    .err()
    .expect("a different config was served");
    assert_eq!(refused.status.code(), Some(4));
    let broken = demo.base.join("broken.conf");
    std::fs::write(&broken, text.replace("marking = none\n", "")).unwrap();
    let refused = serve_with(
        &demo.root,
        &broken,
        &demo.head,
        demo.base.join("stop-cfg2"),
        "127.0.0.1:0",
    )
    .err()
    .expect("a broken config was served");
    assert_eq!(refused.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("missing config key `marking`"));
}

#[test]
fn seeding_never_reuses_a_store_and_a_second_daemon_cannot_share_one() {
    let demo = seeded();
    let (code, stderr) = run(&["--demo-seed".as_ref(), demo.root.as_os_str()]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("not empty"));
    // The config file of an earlier seed is never overwritten either.
    let fresh = demo.base.join("store2");
    let (code, stderr) = run(&[
        "--demo-seed".as_ref(),
        fresh.as_os_str(),
        "--config".as_ref(),
        demo.config.as_os_str(),
    ]);
    assert_eq!(code, 2, "{stderr}");
    assert!(!fresh.exists());
    let (code, _) = run(&["--demo-seed".as_ref(), "relative-store".as_ref()]);
    assert_eq!(code, 2);

    let first = serve_with(
        &demo.root,
        &demo.config,
        &demo.head,
        demo.base.join("stop-a"),
        "127.0.0.1:0",
    )
    .map_err(|o| String::from_utf8_lossy(&o.stderr).into_owned())
    .unwrap();
    let second = serve_with(
        &demo.root,
        &demo.config,
        &demo.head,
        demo.base.join("stop-b"),
        "127.0.0.1:0",
    )
    .err()
    .expect("two daemons shared one store");
    assert_eq!(second.status.code(), Some(4));
    assert_eq!(first.stop().0, 0);
}

#[test]
fn argument_errors_are_typed_and_a_stale_stop_file_blocks_the_start() {
    let demo = seeded();
    let (code, stderr) = run(&["--nonsense".as_ref(), "x".as_ref()]);
    assert_eq!(code, 2, "{stderr}");
    let (code, _) = run(&[]);
    assert_eq!(code, 2);
    let (code, _) = run(&[
        "--demo-seed".as_ref(),
        demo.base.join("s").as_os_str(),
        "--bind".as_ref(),
        "127.0.0.1:0".as_ref(),
    ]);
    assert_eq!(code, 2);
    let stop = demo.base.join("stale-stop");
    std::fs::write(&stop, b"").unwrap();
    let refused = serve_with(&demo.root, &demo.config, &demo.head, stop, "127.0.0.1:0")
        .err()
        .expect("started with a stale stop file");
    assert_eq!(refused.status.code(), Some(2));
    let help = Command::new(BIN).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--demo-seed"));
}

#[test]
fn config_parsing_is_strict_and_round_trips() {
    let config = demo_config();
    let text = render_config(&config).unwrap();
    assert_eq!(parse_config(&text).unwrap(), config);
    // CRLF, comments and blank lines are accepted.
    let noisy = format!("# note\r\n\r\n{}", text.replace('\n', "\r\n"));
    assert_eq!(parse_config(&noisy).unwrap(), config);

    let unknown = format!("{text}extra = 1\n");
    assert_eq!(
        parse_config(&unknown),
        Err(ListenerConfigError::UnknownKey("extra".into()))
    );
    let duplicate = format!("{text}contract = 1\n");
    assert_eq!(
        parse_config(&duplicate),
        Err(ListenerConfigError::DuplicateKey("contract".into()))
    );
    for key in [
        "format",
        "network",
        "admin",
        "contract",
        "collateral",
        "permitted_spot_properties",
        "rollback_limit",
        "inverse",
        "notional",
        "initial_margin_per_contract",
        "fee_route",
        "marking",
    ] {
        let without: String = text
            .lines()
            .filter(|l| !l.starts_with(&format!("{key} =")))
            .map(|l| format!("{l}\n"))
            .collect();
        assert!(
            matches!(parse_config(&without), Err(ListenerConfigError::MissingKey(k)) if k == key),
            "{key}"
        );
        let empty = text.replace(
            text.lines()
                .find(|l| l.starts_with(&format!("{key} =")))
                .unwrap(),
            &format!("{key} ="),
        );
        assert!(parse_config(&empty).is_err(), "{key} empty");
    }
    for (from, to) in [
        ("network = bitcoin", "network = Bitcoin"),
        ("contract = 1", "contract = 01"),
        ("contract = 1", "contract = 0"),
        ("collateral = 7", "collateral = 4294967296"),
        (
            "permitted_spot_properties = 7",
            "permitted_spot_properties = 7,7",
        ),
        (
            "permitted_spot_properties = 7",
            "permitted_spot_properties = 7, 9",
        ),
        ("inverse = false", "inverse = 0"),
        ("notional = 1", "notional = -1"),
        ("notional = 1", "notional = 1e3"),
        ("fee_route = oracle:1", "fee_route = spot"),
        ("fee_route = oracle:1", "fee_route = oracle:2"),
        ("marking = none", "marking = oracle:0"),
        ("rollback_limit = 16", "rollback_limit = 0"),
        ("admin = DemoAdmin", "admin = ~hidden"),
        ("format = tradelayer-listenerd-config-v1", "format = v2"),
    ] {
        assert!(
            parse_config(&text.replace(from, to)).is_err(),
            "{from} -> {to}"
        );
    }
    assert_eq!(
        parse_config("no equals sign\n"),
        Err(ListenerConfigError::Syntax(1))
    );
    assert_eq!(
        parse_config(&"#".repeat(5000)),
        Err(ListenerConfigError::TooLarge)
    );
    assert_eq!(
        parse_config("format = \u{e9}\n"),
        Err(ListenerConfigError::NotAscii)
    );
    // Errors never echo values.
    let secretish = text.replace("admin = DemoAdmin", "admin = ~topsecret");
    assert!(!parse_config(&secretish)
        .unwrap_err()
        .to_string()
        .contains("topsecret"));
}

#[test]
fn head_tokens_round_trip_and_reject_malformed_text() {
    let demo = seeded();
    let head = parse_expected_head(&demo.head).unwrap();
    assert_eq!(head_token(head), demo.head);
    for bad in [
        "",
        "1:2:3",
        "1:2:3:4:5",
        "x:4:aa:bb",
        "5:4:zz:bb",
        "-1:4:00:00",
    ] {
        assert_eq!(parse_expected_head(bad), Err(ListenerConfigError::Head));
    }
}
