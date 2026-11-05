//! Economic playground: in-memory sessions over the `ConservingV1` economic owner.
//!
//! A session is one `EconomicState` in the legacy fixed-margin mode: an issuer-managed token
//! `tUSD` (property 7) is the collateral of one oracle-priced linear BTC/tUSD perpetual
//! (contract 1) with a fixed margin per contract. The accepted-contract mode is not used: its
//! integrity check re-executes the complete input history on every block, so the cost of a block
//! grows with the number of earlier blocks (about 1.3 s per block after 50 blocks in release).
//! Ops: `pg_new`, `pg_block`, `pg_state`, `pg_drop`. Every actor, transaction id and height is an
//! unverified caller assertion; nothing is signed or broadcast.
//!
//! Heights: the engine requires contiguous heights from 1. Blocks 1 and 2 are the genesis setup
//! (activation, oracle, first price, initial grants); `anchor_height` is the external height of
//! engine block 3, the first block the caller applies. `pg_block` takes the external height and
//! rejects anything but the next one (`HeightMismatch`).

use crate::json::{array, field, quote, str_field, u64_field, Obj, OpError, Res};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use tradelayer_core::client::RpcValue;
use tradelayer_core::contract_orderbook::{CancelCriteria, Order, RestingOrder, Side};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::{Decimal, Rounding};
use tradelayer_core::economic::{
    AuditPolicy, EconomicConfig, EconomicError, EconomicInput, EconomicState, MarkConfig,
    MarkPolicy, SettlementPolicy, TradeReceipt,
};
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::tally::FeeRoute;

const ATOMS: u128 = 100_000_000;
/// Largest amount accepted from JSON, so every number round-trips through a JS double.
const MAX_AMOUNT: u64 = 1 << 53;
const MAX_SESSIONS: usize = 64;
const MAX_ACCOUNTS: usize = 64;
const MAX_OPS: usize = 2_000;
const MAX_FILLS_IN_BLOCK: usize = 200;
const ESCROW_PREFIX: &str = "~econ:";
/// tUSD, the collateral token.
const TUSD: u32 = 7;
/// First property id a caller may issue.
const FIRST_FREE_PROPERTY: u64 = 9;
/// First transaction number of caller inputs; below it are genesis inputs.
const FIRST_INPUT_ID: u64 = 1_000;
const DEFAULT_PRICE_1E4: u64 = 1_000_000_000;

thread_local! {
    static STORE: RefCell<Store> = const { RefCell::new(Store { map: BTreeMap::new(), seq: 0 }) };
}

struct Store {
    map: BTreeMap<String, Session>,
    seq: u64,
}

struct Session {
    state: EconomicState,
    issuer: AccountId,
    oracle: AccountId,
    accounts: BTreeSet<AccountId>,
    /// Order id (the input counter value) to owner, for naming fill counterparties.
    order_owner: BTreeMap<u64, AccountId>,
    leverage: u64,
    margin: Decimal,
    marks: bool,
    notional: Decimal,
    next_id: u64,
    anchor: u64,
    blocks: u64,
}

impl Session {
    fn tusd() -> PropertyId {
        PropertyId::try_from(TUSD).expect("nonzero")
    }
    fn engine_next(&self) -> u64 {
        self.state.finalized_height().unwrap_or(0) + 1
    }
    /// External height of an engine height (engine 3 is the anchor).
    fn ext(&self, engine: u64) -> u64 {
        (engine + self.anchor).saturating_sub(3)
    }
    fn last_price_1e4(&self) -> Option<u128> {
        self.state
            .joint()
            .registry()
            .oracle(1)
            .and_then(|o| o.latest.as_ref())
            .map(|q| q.price_1e4)
    }
}

pub fn handle(op: &str, req: &RpcValue) -> Res<Obj> {
    match op {
        "pg_new" => new_session(req),
        "pg_drop" => {
            let id = str_field(req, "session")?;
            let gone = STORE.with(|s| s.borrow_mut().map.remove(&id).is_some());
            if gone {
                Ok(Obj::new().str("session", &id))
            } else {
                Err(unknown_session(&id))
            }
        }
        "pg_block" => with_session(req, |_, s| block(s, req)),
        "pg_state" => with_session(req, |id, s| state_view(id, s, req)),
        other => Err(OpError::new("UnknownOp", format!("unknown op `{other}`"))),
    }
}

fn unknown_session(id: &str) -> OpError {
    OpError::new("UnknownSession", format!("no session `{id}`"))
}

fn with_session<T>(req: &RpcValue, f: impl FnOnce(&str, &mut Session) -> Res<T>) -> Res<T> {
    let id = str_field(req, "session")?;
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        let session = store.map.get_mut(&id).ok_or_else(|| unknown_session(&id))?;
        f(&id, session)
    })
}

// ---- request parsing ------------------------------------------------------------------------

fn bad(message: impl AsRef<str>) -> OpError {
    OpError::bad(message.as_ref())
}

fn opt(req: &RpcValue, name: &str) -> Option<RpcValue> {
    req.field(name)
        .ok()
        .flatten()
        .filter(|v| v.as_json().trim() != "null")
}

fn opt_str(req: &RpcValue, name: &str) -> Res<Option<String>> {
    opt(req, name)
        .map(|v| {
            v.as_string()
                .map_err(|_| bad(format!("field `{name}` is not a string")))
        })
        .transpose()
}

fn opt_u64(req: &RpcValue, name: &str) -> Res<Option<u64>> {
    opt(req, name)
        .map(|v| {
            v.as_u64()
                .map_err(|_| bad(format!("field `{name}` is not an unsigned integer")))
        })
        .transpose()
}

fn opt_bool(req: &RpcValue, name: &str) -> bool {
    opt(req, name).is_some_and(|v| v.as_json().trim() == "true")
}

fn amount_field(req: &RpcValue, name: &str) -> Res<u128> {
    let n = u64_field(req, name)?;
    if n > MAX_AMOUNT {
        return Err(OpError::new(
            "AmountTooLarge",
            format!("`{name}` exceeds 2^53 atoms"),
        ));
    }
    Ok(u128::from(n))
}

/// A decimal given as a JSON string or number.
fn decimal_field(req: &RpcValue, name: &str) -> Res<Decimal> {
    let value = field(req, name)?;
    let raw = value.as_json().trim().to_owned();
    let text = if raw.starts_with('"') {
        value
            .as_string()
            .map_err(|_| bad(format!("field `{name}` is not a string")))?
    } else {
        raw
    };
    Decimal::from_str(&text).map_err(|_| bad(format!("field `{name}` is not a decimal")))
}

fn account(name: &str) -> Res<AccountId> {
    if name.starts_with(ESCROW_PREFIX) {
        return Err(bad("account names cannot start with the reserved `~econ:`"));
    }
    AccountId::try_from(name).map_err(|_| bad(format!("invalid account name `{name}`")))
}

fn account_field(req: &RpcValue, name: &str) -> Res<AccountId> {
    account(&str_field(req, name)?)
}

fn opt_account(req: &RpcValue, name: &str) -> Res<Option<AccountId>> {
    opt_str(req, name)?.map(|n| account(&n)).transpose()
}

fn property_of(req: &RpcValue) -> Res<PropertyId> {
    let raw = opt_u64(req, "property")?.unwrap_or(u64::from(TUSD));
    u32::try_from(raw)
        .ok()
        .and_then(|n| PropertyId::try_from(n).ok())
        .ok_or_else(|| bad("invalid property id"))
}

fn tx(number: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&number.to_be_bytes());
    TxId::from_bytes(bytes)
}

fn tx_number(id: TxId) -> u64 {
    let bytes = id.as_bytes();
    let mut tail = [0; 8];
    tail.copy_from_slice(&bytes[24..]);
    u64::from_be_bytes(tail)
}

fn price_decimal(price_1e4: u128) -> Decimal {
    // Four decimals; always parses.
    format!("{}.{:04}", price_1e4 / 10_000, price_1e4 % 10_000)
        .parse()
        .unwrap_or_else(|_| Decimal::zero())
}

// ---- error mapping --------------------------------------------------------------------------

/// Maps an engine error to a stable code; the message keeps the full `Debug` text.
fn econ_error(error: &EconomicError) -> OpError {
    let text = format!("{error:?}");
    let table: [(&str, &str); 18] = [
        ("InsufficientAvailable", "InsufficientFunds"),
        ("UnauthorizedIssuer", "NotIssuer"),
        ("Unauthorized", "Unauthorized"),
        ("FixedSupply", "FixedSupply"),
        ("PropertyAlreadyExists", "PropertyExists"),
        ("UnknownProperty", "UnknownProperty"),
        ("ZeroAmount", "ZeroAmount"),
        ("ReplayLimit", "Capacity"),
        ("Replay", "Replay"),
        ("FinalizedHeight", "InvalidHeight"),
        ("InvalidHeight", "InvalidHeight"),
        ("NoActivation", "NotActivated"),
        ("ActivatedInSameBlock", "NotActivated"),
        ("ClosedOracle", "OracleClosed"),
        ("InvalidPrice", "InvalidPrice"),
        ("ArithmeticOverflow", "Arithmetic"),
        ("Capacity", "Capacity"),
        ("Book(", "OrderRejected"),
    ];
    for (needle, code) in table {
        if text.contains(needle) {
            return OpError::new(code, text);
        }
    }
    if matches!(error, EconomicError::InvalidInput) {
        return OpError::new(
            "InvalidInput",
            "input rejected: a bad order field (zero or oversized quantity or price) or a duplicate id",
        );
    }
    OpError::from_debug(error)
}

// ---- pg_new ---------------------------------------------------------------------------------

fn joint(number: u64, actor: &AccountId, operation: JointOperation) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: tx(number),
        actor: actor.clone(),
        operation,
    })
}

fn new_session(req: &RpcValue) -> Res<Obj> {
    let anchor = opt_u64(req, "anchor_height")?.unwrap_or(3);
    if anchor < 3 {
        return Err(bad("`anchor_height` must be at least 3"));
    }
    let leverage = opt_u64(req, "leverage")?.unwrap_or(10);
    if !(1..=50).contains(&leverage) {
        return Err(bad("`leverage` must be 1..=50"));
    }
    let notional_text = opt_str(req, "notional")?.unwrap_or_else(|| "1".to_owned());
    let notional =
        Decimal::from_str(&notional_text).map_err(|_| bad("`notional` is not a decimal"))?;
    if notional.is_zero() || notional.is_negative() {
        return Err(bad("`notional` must be positive"));
    }
    let price_1e4 = u128::from(opt_u64(req, "price_1e4")?.unwrap_or(DEFAULT_PRICE_1E4));
    if price_1e4 == 0 {
        return Err(bad("`price_1e4` must be positive"));
    }
    let marks = opt_bool(req, "marks");
    let margin = match opt_str(req, "margin_per_contract")? {
        Some(text) => {
            Decimal::from_str(&text).map_err(|_| bad("`margin_per_contract` is not a decimal"))?
        }
        None => notional
            .checked_mul(&price_decimal(price_1e4))
            .and_then(|v| v.checked_div(&Decimal::from(u128::from(leverage)), 2, Rounding::HalfUp))
            .map_err(|_| arith("margin"))?,
    };
    if margin.is_zero() || margin.is_negative() {
        return Err(bad("`margin_per_contract` must be positive"));
    }
    let issuer = opt_account(req, "issuer")?.unwrap_or(account("issuer")?);
    let oracle = opt_account(req, "oracle")?.unwrap_or(account("oracle")?);
    let admin = opt_account(req, "admin")?.unwrap_or(account("admin")?);
    let supply = match opt_u64(req, "issuer_atoms")? {
        Some(n) if n > MAX_AMOUNT => return Err(OpError::new("AmountTooLarge", "issuer_atoms")),
        Some(n) => u128::from(n),
        None => 1_000_000 * ATOMS,
    };

    // Initial accounts: strings or {name, tusd_atoms}.
    let mut grants: Vec<(AccountId, u128)> = Vec::new();
    let mut accounts: BTreeSet<AccountId> = [issuer.clone(), oracle.clone(), admin.clone()].into();
    if accounts.len() != 3 {
        return Err(bad("issuer, oracle and admin must be distinct accounts"));
    }
    if let Some(list) = opt(req, "accounts") {
        let items = list
            .as_array()
            .map_err(|_| bad("`accounts` is not an array"))?;
        if items.len() > MAX_ACCOUNTS {
            return Err(OpError::new("Capacity", "too many initial accounts"));
        }
        for item in items {
            let (name, atoms) = if item.as_json().trim_start().starts_with('"') {
                (
                    item.as_string()
                        .map_err(|_| bad("account is not a string"))?,
                    0,
                )
            } else {
                (
                    str_field(&item, "name")?,
                    opt_u64(&item, "tusd_atoms")?.unwrap_or(0),
                )
            };
            if atoms > MAX_AMOUNT {
                return Err(OpError::new("AmountTooLarge", "tusd_atoms"));
            }
            let who = account(&name)?;
            if !accounts.insert(who.clone()) {
                return Err(bad(format!("duplicate account `{name}`")));
            }
            if atoms > 0 {
                grants.push((who, u128::from(atoms)));
            }
        }
    }

    let id = {
        let supplied = opt_str(req, "id")?;
        STORE.with(|store| {
            let mut store = store.borrow_mut();
            if store.map.len() >= MAX_SESSIONS {
                return Err(OpError::new("Capacity", "too many sessions"));
            }
            match supplied {
                Some(id) if store.map.contains_key(&id) => Err(OpError::new(
                    "SessionExists",
                    format!("session `{id}` exists"),
                )),
                Some(id) => Ok(id),
                None => loop {
                    store.seq += 1;
                    let id = format!("s{}", store.seq);
                    if !store.map.contains_key(&id) {
                        break Ok(id);
                    }
                },
            }
        })?
    };

    // tUSD is issued as a managed property by the issuer. The economic config needs the
    // collateral in the bootstrap ledger, so genesis applies the same ledger operation that a
    // type-1 `Issue` applies; every later grant, redeem and send is an ordinary input.
    let tusd = Session::tusd();
    let mut ledger = Ledger::new();
    ledger
        .apply(
            tx(1),
            Operation::CreateProperty {
                id: tusd,
                issuer: issuer.clone(),
                kind: PropertyKind::Managed,
                initial_atoms: supply,
            },
        )
        .map_err(OpError::from_debug)?;
    let bootstrap = JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Bitcoin,
        admin.clone(),
        ledger,
        BTreeSet::from([tusd]),
    )
    .map_err(OpError::from_debug)?;

    let contract = ContractId::try_from(1).map_err(OpError::from_debug)?;
    let config = EconomicConfig {
        native_entitlements: None,
        insurance: None,
        source_consensus: None,
        network: TradeLayerMainnet::Bitcoin,
        anchor: EconomicConfig::genesis_anchor(TradeLayerMainnet::Bitcoin),
        contract,
        collateral: tusd,
        inverse: false,
        notional: notional.clone(),
        initial_margin_per_contract: margin.clone(),
        fee_route: FeeRoute::Oracle(1),
        policy: SettlementPolicy::ConservingV1,
        funding: None,
        marking: marks.then_some(MarkConfig {
            oracle_id: 1,
            policy: MarkPolicy::ConservingMarkBasisV1,
        }),
        liquidation: None,
        amm: None,
        contract_binding: None,
    };
    let mut state =
        EconomicState::from_joint_unverified(config, bootstrap).map_err(|e| econ_error(&e))?;
    // Live checks and the block's own additions every block, the complete audit at every
    // 64th height.
    state.set_audit_policy(AuditPolicy::default());

    // Block 1: activate the opcodes. Block 2: oracle, first price, initial grants.
    state
        .apply_block_unverified(
            1,
            vec![joint(
                10,
                &admin,
                JointOperation::Registry(RegistryAction::Activate {
                    types: vec![1, 2, 6, 11, 12, 13, 14, 15, 18],
                    code_hash: [0x79; 32],
                }),
            )],
        )
        .map_err(|e| econ_error(&e))?;
    let mut setup = vec![
        joint(
            20,
            &oracle,
            JointOperation::Registry(RegistryAction::CreateOracle {
                ticker: "BTCUSD".into(),
                url: "https://mempool.space/api/v1/prices".into(),
                backup: account("oracle-backup")?,
                lag: 1,
            }),
        ),
        joint(
            21,
            &oracle,
            JointOperation::Registry(RegistryAction::PublishOracle {
                oracle_id: 1,
                price_1e4,
                high_1e4: None,
                low_1e4: None,
                close_1e4: None,
            }),
        ),
    ];
    for (i, (who, atoms)) in grants.into_iter().enumerate() {
        setup.push(joint(
            100 + i as u64,
            &issuer,
            JointOperation::Grant {
                id: tusd,
                recipient: who,
                amount_atoms: atoms,
            },
        ));
    }
    state
        .apply_block_unverified(2, setup)
        .map_err(|e| econ_error(&e))?;

    let session = Session {
        state,
        issuer,
        oracle,
        accounts,
        order_owner: BTreeMap::new(),
        leverage,
        margin: margin.clone(),
        marks,
        notional,
        next_id: FIRST_INPUT_ID,
        anchor,
        blocks: 0,
    };
    let next_height = session.ext(session.engine_next());
    STORE.with(|store| store.borrow_mut().map.insert(id.clone(), session));
    Ok(Obj::new()
        .str("session", &id)
        .num("next_height", next_height)
        .num("price_1e4", price_1e4)
        .num("leverage", leverage)
        .str("margin_per_contract", &margin.to_string())
        .bool("marks", marks)
        .str("notional", &notional_text)
        .num("tusd_property", TUSD))
}

// ---- pg_block -------------------------------------------------------------------------------

struct Built {
    inputs: Vec<EconomicInput>,
    results: Vec<String>,
    next_id: u64,
    touched: BTreeSet<AccountId>,
    owners: Vec<(u64, AccountId)>,
}

fn side_of(req: &RpcValue) -> Res<Side> {
    match str_field(req, "side")?.as_str() {
        "buy" => Ok(Side::Buy),
        "sell" => Ok(Side::Sell),
        _ => Err(bad("`side` must be \"buy\" or \"sell\"")),
    }
}

/// Turns the request's ops into engine inputs in the given order. Reads the session only, so a
/// failure here leaves it untouched.
fn build(s: &Session, engine: u64, ops: &[RpcValue]) -> Res<Built> {
    let ledger = s.state.ledger();
    let mut reference = s.last_price_1e4().unwrap_or(0);
    let mut counter = s.next_id;
    let mut next = || {
        let n = counter;
        counter += 1;
        n
    };
    let mut inputs = Vec::new();
    let mut results = Vec::new();
    let mut touched = BTreeSet::new();
    let mut owners = Vec::new();
    // The default actor of grant/redeem is the issuer of the named property.
    let issuer_of = |id: PropertyId| {
        ledger
            .property(id)
            .map_or_else(|| s.issuer.clone(), |p| p.issuer.clone())
    };
    for (index, op) in ops.iter().enumerate() {
        let kind = str_field(op, "t")?;
        let mut result = Obj::new().num("i", index).str("t", &kind);
        match kind.as_str() {
            "issue" => {
                let by = opt_account(op, "by")?.unwrap_or_else(|| s.issuer.clone());
                let id = u32::try_from(u64_field(op, "property")?)
                    .ok()
                    .filter(|n| u64::from(*n) >= FIRST_FREE_PROPERTY)
                    .and_then(|n| PropertyId::try_from(n).ok())
                    .ok_or_else(|| bad("`property` must be 9 or higher"))?;
                let property_kind = match opt_str(op, "kind")?.as_deref() {
                    None | Some("managed") => PropertyKind::Managed,
                    Some("fixed") => PropertyKind::Fixed,
                    Some(_) => return Err(bad("`kind` must be \"managed\" or \"fixed\"")),
                };
                touched.insert(by.clone());
                inputs.push(joint(
                    next(),
                    &by,
                    JointOperation::Issue {
                        id,
                        kind: property_kind,
                        initial_atoms: amount_field(op, "atoms")?,
                    },
                ));
            }
            "grant" => {
                let id = property_of(op)?;
                let by = opt_account(op, "by")?.unwrap_or_else(|| issuer_of(id));
                let to = account_field(op, "to")?;
                touched.insert(to.clone());
                inputs.push(joint(
                    next(),
                    &by,
                    JointOperation::Grant {
                        id,
                        recipient: to,
                        amount_atoms: amount_field(op, "atoms")?,
                    },
                ));
            }
            "redeem" => {
                // The issuer burns only its own available units; a holder first sends them back.
                let id = property_of(op)?;
                let by = opt_account(op, "by")?.unwrap_or_else(|| issuer_of(id));
                let atoms = amount_field(op, "atoms")?;
                if let Some(from) = opt_account(op, "from")?.filter(|f| *f != by) {
                    touched.insert(from.clone());
                    inputs.push(joint(
                        next(),
                        &from,
                        JointOperation::Send {
                            id,
                            recipient: by.clone(),
                            amount_atoms: atoms,
                        },
                    ));
                }
                touched.insert(by.clone());
                inputs.push(joint(
                    next(),
                    &by,
                    JointOperation::Redeem {
                        id,
                        amount_atoms: atoms,
                    },
                ));
            }
            "send" => {
                let from = account_field(op, "from")?;
                let to = account_field(op, "to")?;
                touched.insert(from.clone());
                touched.insert(to.clone());
                inputs.push(joint(
                    next(),
                    &from,
                    JointOperation::Send {
                        id: property_of(op)?,
                        recipient: to,
                        amount_atoms: amount_field(op, "atoms")?,
                    },
                ));
            }
            "oracle" => {
                let price_1e4 = amount_field(op, "price_1e4")?;
                reference = price_1e4;
                inputs.push(joint(
                    next(),
                    &s.oracle,
                    JointOperation::Registry(RegistryAction::PublishOracle {
                        oracle_id: 1,
                        price_1e4,
                        high_1e4: None,
                        low_1e4: None,
                        close_1e4: None,
                    }),
                ));
            }
            "order" => {
                let owner = account_field(op, "account")?;
                let market = opt_bool(op, "market");
                let price = if market && opt(op, "price").is_none() {
                    price_decimal(reference)
                } else {
                    decimal_field(op, "price")?
                };
                let id = next();
                inputs.push(EconomicInput::Order(Order {
                    txid: tx(id),
                    owner: owner.clone(),
                    contract: s.state.config().contract,
                    height: engine,
                    side: side_of(op)?,
                    quantity: decimal_field(op, "qty")?,
                    price,
                    post: opt_bool(op, "post"),
                    is_market: market,
                }));
                result = result.num("id", id);
                touched.insert(owner.clone());
                owners.push((id, owner));
            }
            "cancel" => {
                inputs.push(EconomicInput::Cancel {
                    txid: tx(next()),
                    owner: account_field(op, "account")?,
                    criteria: CancelCriteria::Txid(tx(u64_field(op, "order_id")?)),
                });
            }
            "cancel_all" => {
                inputs.push(EconomicInput::Cancel {
                    txid: tx(next()),
                    owner: account_field(op, "account")?,
                    criteria: CancelCriteria::All,
                });
            }
            other => return Err(bad(format!("unknown input type `{other}`"))),
        }
        results.push(result.build());
    }
    Ok(Built {
        inputs,
        results,
        next_id: counter,
        touched,
        owners,
    })
}

fn block(s: &mut Session, req: &RpcValue) -> Res<Obj> {
    let height = u64_field(req, "height")?;
    let engine = s.engine_next();
    let expected = s.ext(engine);
    if height != expected {
        return Err(OpError::new(
            "HeightMismatch",
            format!("expected height {expected}, got {height}"),
        ));
    }
    let ops = field(req, "ops")?
        .as_array()
        .map_err(|_| bad("`ops` is not an array"))?;
    if ops.len() > MAX_OPS {
        return Err(OpError::new("Capacity", "too many ops in one block"));
    }
    let built = build(s, engine, &ops)?;
    let receipts_before = s.state.receipt_count();
    // The engine applies the block on a private clone and publishes it only on success.
    s.state
        .apply_block_unverified(engine, built.inputs)
        .map_err(|e| econ_error(&e))?;
    s.next_id = built.next_id;
    s.blocks += 1;
    if s.accounts.len() + built.touched.len() <= MAX_ACCOUNTS * 4 {
        s.accounts.extend(built.touched);
    }
    s.order_owner.extend(built.owners);

    let fills = s.state.receipts_since(receipts_before);
    let shown: Vec<String> = fills
        .iter()
        .take(MAX_FILLS_IN_BLOCK)
        .map(|r| fill_json(s, r))
        .collect();
    Ok(Obj::new()
        .num("height", height)
        .num("engine_height", engine)
        .num("blocks", s.blocks)
        .num("price_1e4", s.last_price_1e4().unwrap_or(0))
        .num("fill_count", fills.len())
        .raw("fills", array(shown))
        .raw("ops", array(built.results)))
}

fn fill_json(s: &Session, r: &TradeReceipt) -> String {
    let name = |id: TxId| {
        s.order_owner
            .get(&tx_number(id))
            .map_or_else(String::new, |a| a.as_str().to_owned())
    };
    Obj::new()
        .num("height", s.ext(r.height))
        .str("price", &r.price.to_string())
        .str("qty", &r.quantity.to_string())
        .str("buyer", &name(r.buy_txid))
        .str("seller", &name(r.sell_txid))
        .num("buy_id", tx_number(r.buy_txid))
        .num("sell_id", tx_number(r.sell_txid))
        .num("fee_atoms", r.retained_fee_atoms)
        .build()
}

// ---- pg_state -------------------------------------------------------------------------------

fn arith(what: &str) -> OpError {
    OpError::new("Arithmetic", what)
}

fn state_view(id: &str, s: &mut Session, req: &RpcValue) -> Res<Obj> {
    let depth = opt_u64(req, "book_depth")?.unwrap_or(200) as usize;
    let fill_limit = opt_u64(req, "fills")?.unwrap_or(20) as usize;
    if opt_bool(req, "verify") {
        s.state.verify().map_err(|e| econ_error(&e))?;
        s.state
            .ledger()
            .verify_conservation()
            .map_err(OpError::from_debug)?;
    }
    let state = &s.state;
    let tusd = Session::tusd();
    let mark_1e4 = s.last_price_1e4();
    let mark = mark_1e4.map(price_decimal);
    let engine = state.finalized_height().unwrap_or(0);

    // Order book and per-owner reserves.
    let side_orders = |side: Side| -> Vec<&RestingOrder> {
        let mut orders: Vec<_> = state
            .book()
            .resting()
            .iter()
            .filter(|r| r.order.side == side)
            .collect();
        orders.sort_by(|a, b| {
            let by_price = a.order.price.cmp(&b.order.price);
            let by_price = if side == Side::Buy {
                by_price.reverse()
            } else {
                by_price
            };
            by_price.then(a.sequence.cmp(&b.sequence))
        });
        orders
    };
    let bids = side_orders(Side::Buy);
    let asks = side_orders(Side::Sell);
    let mut reserved: BTreeMap<&AccountId, u128> = BTreeMap::new();
    for r in state.book().resting() {
        *reserved.entry(&r.order.owner).or_default() += state.reserve_atoms(r.order.txid);
    }
    let order_json = |r: &RestingOrder| {
        Obj::new()
            .num("id", tx_number(r.order.txid))
            .str("account", r.order.owner.as_str())
            .str(
                "side",
                if r.order.side == Side::Buy {
                    "buy"
                } else {
                    "sell"
                },
            )
            .str("price", &r.order.price.to_string())
            .str("qty", &r.remaining.to_string())
            .bool("post", r.order.post)
            .bool("market", r.order.is_market)
            .num("height", s.ext(r.order.height))
            .num("reserve_atoms", state.reserve_atoms(r.order.txid))
            .build()
    };
    let (bid_count, ask_count) = (bids.len(), asks.len());

    // Accounts.
    let mut accounts = Vec::new();
    let mut available_total = 0u128;
    let mut margin_total = 0u128;
    let mut reserve_total = 0u128;
    for who in &s.accounts {
        let available = state.ledger().available_atoms(tusd, who);
        let margin = state.margin_atoms(who);
        let reserve = reserved.get(who).copied().unwrap_or(0);
        available_total += available;
        margin_total += margin;
        reserve_total += reserve;
        accounts.push(
            Obj::new()
                .str("name", who.as_str())
                .num("available_atoms", available)
                .num("margin_atoms", margin)
                .num("reserved_atoms", reserve)
                .num("equity_atoms", available + margin + reserve)
                .build(),
        );
    }

    // Positions: entry-based unrealized PnL at the last oracle price. The engine settles each
    // oracle publication into `realized_pnl` (settled basis), so the two overlap by design.
    let mut positions = Vec::new();
    let mut net = Decimal::zero();
    let mut open_interest = Decimal::zero();
    for p in state.positions().positions() {
        net = net.checked_add(&p.contracts).map_err(|_| arith("net"))?;
        if p.contracts.is_zero() {
            continue;
        }
        if !p.contracts.is_negative() {
            open_interest = open_interest
                .checked_add(&p.contracts)
                .map_err(|_| arith("open interest"))?;
        }
        let entry = p.avg_price.clone().filter(|e| !e.is_zero());
        let unrealized = match (&mark, &entry) {
            (Some(m), Some(e)) => m
                .checked_sub(e)
                .and_then(|d| d.checked_mul(&p.contracts))
                .and_then(|d| d.checked_mul(&s.notional))
                .and_then(|d| d.quantize(8, Rounding::HalfUp))
                .map_err(|_| arith("unrealized pnl"))?,
            _ => Decimal::zero(),
        };
        positions.push(
            Obj::new()
                .str("account", p.address.as_str())
                .str("contracts", &p.contracts.to_string())
                .raw(
                    "entry",
                    entry.map_or_else(|| "null".to_owned(), |e| quote(&e.to_string())),
                )
                .num("margin_atoms", state.margin_atoms(&p.address))
                .str("unrealized_pnl", &unrealized.to_string())
                .str("realized_pnl", &p.realized_pnl.to_string())
                .build(),
        );
    }
    let calls: Vec<String> = state
        .mark_state()
        .maintenance
        .iter()
        .map(|c| {
            Obj::new()
                .str("account", c.owner.as_str())
                .num("required_atoms", c.required_atoms)
                .num("margin_atoms", c.margin_atoms)
                .build()
        })
        .collect();

    // Custody and supply.
    let fee_key = state
        .config()
        .fee_route
        .key(tusd)
        .map_err(OpError::from_debug)?;
    let fees = state
        .fees()
        .expected_custody_atoms(fee_key)
        .map_err(OpError::from_debug)?;
    let loss = state.loss_pool_atoms();
    let insurance = state.insurance_custody_atoms();
    let supply = state
        .ledger()
        .property(tusd)
        .map_or(0, |p| p.total_supply_atoms);
    let named_total = available_total + margin_total + reserve_total + fees + loss + insurance;
    let properties: Vec<String> = state
        .ledger()
        .properties()
        .map(|(pid, p)| {
            Obj::new()
                .num("id", pid.get())
                .str("issuer", p.issuer.as_str())
                .str(
                    "kind",
                    if p.kind == PropertyKind::Managed {
                        "managed"
                    } else {
                        "fixed"
                    },
                )
                .num("supply_atoms", p.total_supply_atoms)
                .build()
        })
        .collect();

    let receipts = state.receipts();
    let fills: Vec<String> = receipts
        .iter()
        .rev()
        .take(fill_limit)
        .map(|r| fill_json(s, r))
        .collect();

    Ok(Obj::new()
        .str("session", id)
        .num("engine_height", engine)
        .num("height", s.ext(engine))
        .num("next_height", s.ext(s.engine_next()))
        .num("blocks", s.blocks)
        .raw(
            "oracle",
            Obj::new()
                .raw(
                    "price_1e4",
                    mark_1e4.map_or_else(|| "null".to_owned(), |p| p.to_string()),
                )
                .str("account", s.oracle.as_str())
                .build(),
        )
        .raw(
            "contract",
            Obj::new()
                .num("id", 1)
                .str("symbol", "BTC/tUSD perpetual")
                .bool("inverse", false)
                .str("mode", "fixed-margin")
                .str("notional", &s.notional.to_string())
                .num("leverage", s.leverage)
                .str("margin_per_contract", &s.margin.to_string())
                .bool("marks", s.marks)
                .build(),
        )
        .str("issuer", s.issuer.as_str())
        .raw("accounts", array(accounts))
        .raw(
            "book",
            Obj::new()
                .raw("bids", array(bids.into_iter().take(depth).map(order_json)))
                .raw("asks", array(asks.into_iter().take(depth).map(order_json)))
                .num("bid_count", bid_count)
                .num("ask_count", ask_count)
                .build(),
        )
        .raw("positions", array(positions))
        .str("net_contracts", &net.to_string())
        .str("open_interest", &open_interest.to_string())
        .raw("maintenance_calls", array(calls))
        .raw(
            "custody",
            Obj::new()
                .num("supply_atoms", supply)
                .num("available_atoms", available_total)
                .num("margin_atoms", margin_total)
                .num("reserved_atoms", reserve_total)
                .num("fees_atoms", fees)
                .num("loss_pool_atoms", loss)
                .num("insurance_atoms", insurance)
                .num("unfunded_loss_atoms", state.unfunded_loss_atoms())
                .bool("balanced", named_total == supply)
                .build(),
        )
        .raw("properties", array(properties))
        .num("fill_total", state.receipt_count())
        .num(
            "state_bytes",
            state.snapshot_bytes().map_or(0, |bytes| bytes.len()),
        )
        .num("ledger_markers", state.ledger().processed_count())
        .num("input_ids", state.joint().processed_count())
        .raw("fills", array(fills)))
}
