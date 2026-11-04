//! Economic playground: in-memory sessions over the `ConservingV1` economic owner.

use crate::json::{Obj, OpError, Res};
use tradelayer_core::client::RpcValue;

pub fn handle(op: &str, req: &RpcValue) -> Res<Obj> {
    let _ = req;
    Err(OpError::new("UnknownOp", format!("unknown op `{op}`")))
}
