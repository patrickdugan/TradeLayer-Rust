//! Wire codec and raw-block operations (stateless).

use crate::json::{Obj, OpError, Res};
use tradelayer_core::client::RpcValue;

pub fn handle(op: &str, req: &RpcValue) -> Res<Obj> {
    let _ = req;
    match op {
        "version" => Ok(Obj::new().str("crate", env!("CARGO_PKG_VERSION"))),
        other => Err(OpError::new("UnknownOp", format!("unknown op `{other}`"))),
    }
}
