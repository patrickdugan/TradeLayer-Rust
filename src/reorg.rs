//! Source-compatible RPC fork observations, not reorganization authority.
//!
//! The pinned JavaScript only filters tip statuses, fetches blocks, and logs.
//! A report neither chooses a best chain nor authenticates headers, transaction
//! data, checkpoints, or economic rollback. See `docs/REORG.md`.

use core::fmt;

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_LF_SHA256: &str =
    "3e4cabb0dc560fd8c601b72e05f0e698ce02adc646eaef241189fd192d906a82";
pub const MAX_REORG_TIPS: usize = 1024;
pub const MAX_REPORTED_HASH_BYTES: usize = 256;
pub const MAX_TIP_STATUS_BYTES: usize = 64;
pub const MAX_RPC_ERROR_BYTES: usize = 4096;
pub const MAX_REPORTED_BLOCK_HEIGHT: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserverInputError {
    HashTooLong,
    StatusTooLong,
    ErrorTooLong,
    HeightOutOfRange,
}
impl fmt::Display for ObserverInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ObserverInputError {}

/// Opaque RPC metadata. A string named hash is not a validated block ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainTip {
    reported_hash: String,
    status: String,
}
impl ChainTip {
    pub fn new(reported_hash: &str, status: &str) -> Result<Self, ObserverInputError> {
        check_hash(reported_hash)?;
        if status.len() > MAX_TIP_STATUS_BYTES {
            return Err(ObserverInputError::StatusTooLong);
        }
        Ok(Self {
            reported_hash: reported_hash.to_owned(),
            status: status.to_owned(),
        })
    }
    pub fn reported_hash(&self) -> &str {
        &self.reported_hash
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn is_source_candidate(&self) -> bool {
        matches!(self.status.as_str(), "valid-fork" | "headers-only")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedBlock {
    height: u64,
    reported_hash: String,
}
impl ReportedBlock {
    pub fn new(height: u64, reported_hash: &str) -> Result<Self, ObserverInputError> {
        check_hash(reported_hash)?;
        if height > MAX_REPORTED_BLOCK_HEIGHT {
            return Err(ObserverInputError::HeightOutOfRange);
        }
        Ok(Self {
            height,
            reported_hash: reported_hash.to_owned(),
        })
    }
    pub const fn height(&self) -> u64 {
        self.height
    }
    pub fn reported_hash(&self) -> &str {
        &self.reported_hash
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcFailure {
    message: String,
}
impl RpcFailure {
    pub fn new(message: &str) -> Result<Self, ObserverInputError> {
        if message.len() > MAX_RPC_ERROR_BYTES {
            return Err(ObserverInputError::ErrorTooLong);
        }
        Ok(Self {
            message: message.to_owned(),
        })
    }
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl fmt::Display for RpcFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for RpcFailure {}

/// An injected RPC transport. Constructing/configuring a real client, response
/// decoding, I/O limits, timeouts and chain authentication belong to the caller.
/// Returned metadata/error types have explicit local bounds.
pub trait ReorgRpc {
    fn get_chain_tips(&mut self) -> Result<Vec<ChainTip>, RpcFailure>;
    fn get_block(&mut self, reported_hash: &str) -> Result<ReportedBlock, RpcFailure>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObserverFailure {
    Rpc(RpcFailure),
    Input(ObserverInputError),
    TooManyTips { actual: usize, maximum: usize },
}
impl fmt::Display for ObserverFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rpc(error) => error.fmt(f),
            Self::Input(error) => error.fmt(f),
            Self::TooManyTips { actual, maximum } => {
                write!(f, "RPC returned {actual} tips; local limit is {maximum}")
            }
        }
    }
}
impl std::error::Error for ObserverFailure {}

/// Typed equivalents of actual source console calls. RPC error values expose
/// their bounded message, not runtime-specific Error stacks or object shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLog {
    BlockFetchError(RpcFailure),
    CheckError(ObserverFailure),
    PotentialReorg { height: u64, reported_hash: String },
}
impl SourceLog {
    pub fn message(&self) -> String {
        match self {
            Self::BlockFetchError(error) => format!("Error fetching block: {error}"),
            Self::CheckError(error) => format!("Error in checkReOrg: {error}"),
            Self::PotentialReorg {
                height,
                reported_hash,
            } => format!("Potential reorg detected at block {height}: {reported_hash}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockFetchReport {
    /// getBlock returns its block or rethrows its RPC error in the source.
    pub result: Result<ReportedBlock, ObserverFailure>,
    pub logs: Vec<SourceLog>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkObservation {
    pub tip: ChainTip,
    pub block: ReportedBlock,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReorgReport {
    pub total_tip_count: Option<usize>,
    pub selected_tip_count: Option<usize>,
    /// Successfully fetched prefix, in RPC tip order, including duplicates.
    pub observations: Vec<ForkObservation>,
    pub failed_tip: Option<ChainTip>,
    /// checkReOrg swallows the error. This report retains it for inspection;
    /// no error escapes as an economic state transition or rollback command.
    pub swallowed_failure: Option<ObserverFailure>,
    pub logs: Vec<SourceLog>,
}
impl ReorgReport {
    pub fn completed(&self) -> bool {
        self.swallowed_failure.is_none()
    }
}

/// Replaces `new litecoin.Client(config)` with explicit caller-owned transport.
/// There is no background loop, retry, mutable global client or hidden RPC call.
pub struct ReorgChecker<R> {
    rpc: R,
}
impl<R: ReorgRpc> ReorgChecker<R> {
    pub fn new(rpc: R) -> Self {
        Self { rpc }
    }
    pub fn rpc(&self) -> &R {
        &self.rpc
    }
    pub fn into_rpc(self) -> R {
        self.rpc
    }

    pub fn get_block(&mut self, reported_hash: &str) -> BlockFetchReport {
        if let Err(error) = check_hash(reported_hash) {
            return BlockFetchReport {
                result: Err(ObserverFailure::Input(error)),
                logs: Vec::new(),
            };
        }
        match self.rpc.get_block(reported_hash) {
            Ok(block) => BlockFetchReport {
                result: Ok(block),
                logs: Vec::new(),
            },
            Err(error) => BlockFetchReport {
                result: Err(ObserverFailure::Rpc(error.clone())),
                logs: vec![SourceLog::BlockFetchError(error)],
            },
        }
    }

    pub fn check_reorg(&mut self) -> ReorgReport {
        let mut report = ReorgReport {
            total_tip_count: None,
            selected_tip_count: None,
            observations: Vec::new(),
            failed_tip: None,
            swallowed_failure: None,
            logs: Vec::new(),
        };
        let tips = match self.rpc.get_chain_tips() {
            Ok(tips) => tips,
            Err(error) => return swallowed(report, ObserverFailure::Rpc(error)),
        };
        report.total_tip_count = Some(tips.len());
        if tips.len() > MAX_REORG_TIPS {
            return swallowed(
                report,
                ObserverFailure::TooManyTips {
                    actual: tips.len(),
                    maximum: MAX_REORG_TIPS,
                },
            );
        }
        // The source completes filter() before fetching the first block.
        let selected: Vec<_> = tips
            .into_iter()
            .filter(ChainTip::is_source_candidate)
            .collect();
        report.selected_tip_count = Some(selected.len());
        for tip in selected {
            let fetch = self.get_block(tip.reported_hash());
            report.logs.extend(fetch.logs);
            match fetch.result {
                Ok(block) => {
                    report.logs.push(SourceLog::PotentialReorg {
                        height: block.height(),
                        reported_hash: block.reported_hash().to_owned(),
                    });
                    report.observations.push(ForkObservation { tip, block });
                }
                Err(error) => {
                    report.failed_tip = Some(tip);
                    return swallowed(report, error);
                }
            }
        }
        report
    }
}
fn swallowed(mut report: ReorgReport, error: ObserverFailure) -> ReorgReport {
    report.logs.push(SourceLog::CheckError(error.clone()));
    report.swallowed_failure = Some(error);
    report
}
fn check_hash(hash: &str) -> Result<(), ObserverInputError> {
    if hash.len() > MAX_REPORTED_HASH_BYTES {
        Err(ObserverInputError::HashTooLong)
    } else {
        Ok(())
    }
}
