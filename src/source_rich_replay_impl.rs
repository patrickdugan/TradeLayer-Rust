// Binary rich frames are admitted only under outer TLCR0e. Legacy value
// decoders remain closed; diagnostic text is never used for execution.
fn rich_selected(policy: SourceContractRuntimePolicy) -> u8 {
    if policy == SourceContractRuntimePolicy::RichThrownV14 {
        14
    } else {
        0
    }
}
fn rich_protocol_error(
    error: crate::source_rich_value::protocol::ProtocolError,
) -> SourceContractReplayError {
    use crate::source_rich_value::{protocol::ProtocolError, Error};
    match error {
        ProtocolError::Capacity | ProtocolError::Graph(Error::Capacity) => {
            SourceContractReplayError::Capacity
        }
        _ => SourceContractReplayError::InvalidCheckpoint,
    }
}
fn rich_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn rich_graph_view(graph: &crate::source_rich_value::Graph) -> V {
    match crate::source_rich_value::wire::encode(graph) {
        Ok(bytes) => obj(&[
            ("graphVersion", s("TLRV1")),
            ("frameHex", s(&rich_hex(&bytes))),
        ]),
        Err(error) => obj(&[
            ("graphVersion", s("TLRV1")),
            ("invalidGraph", s(&format!("{error:?}"))),
        ]),
    }
}
fn rich_command_view(call: &crate::source_rich_value::protocol::InvokeRichV1) -> V {
    obj(&[
        ("kind", s("InvokeRichV1")),
        ("target", s(&call.target)),
        ("graph", rich_graph_view(&call.arguments)),
    ])
}
fn rich_outcome_view(value: &crate::source_rich_value::protocol::RichDependencyV1) -> V {
    obj(&[
        ("kind", s("RichDependencyV1")),
        (
            "transport",
            s(match value.transport {
                crate::source_rich_value::protocol::Transport::Immediate => "Immediate",
                crate::source_rich_value::protocol::Transport::Pending => "Pending",
                crate::source_rich_value::protocol::Transport::Throw => "Throw",
                crate::source_rich_value::protocol::Transport::PendingThrow => "PendingThrow",
            }),
        ),
        ("epoch", s(&value.epoch.to_string())),
        ("graph", rich_graph_view(&value.graph)),
    ])
}
fn rich_command_frame(
    policy: SourceContractRuntimePolicy,
    command: &SourceContractCommand,
) -> Result<Vec<u8>> {
    if let SourceContractCommand::InvokeRichV1(call) = command {
        return call
            .encode(
                rich_selected(policy),
                &crate::source_rich_value::intrinsics::capabilities(),
                crate::source_rich_value::Limits::default(),
            )
            .map_err(rich_protocol_error);
    }
    if policy == SourceContractRuntimePolicy::RichThrownV14
        && matches!(
            command,
            SourceContractCommand::Invoke(_)
                | SourceContractCommand::EvaluateQueue(_)
                | SourceContractCommand::ExecuteContract(_)
        )
    {
        return Err(SourceContractReplayError::InvalidCommand);
    }
    diagnostic(&command.value())
}
fn rich_outcome_frame(
    policy: SourceContractRuntimePolicy,
    outcome: &SourceContractCommandOutcome,
) -> Result<Vec<u8>> {
    if let SourceContractCommandOutcome::RichDependencyV1(value) = outcome {
        return value
            .encode(
                rich_selected(policy),
                &crate::source_rich_value::intrinsics::capabilities(),
                crate::source_rich_value::Limits::default(),
            )
            .map_err(rich_protocol_error);
    }
    if policy == SourceContractRuntimePolicy::RichThrownV14
        && !matches!(
            outcome,
            SourceContractCommandOutcome::Void | SourceContractCommandOutcome::LocalError(_)
        )
    {
        return Err(SourceContractReplayError::InvalidCheckpoint);
    }
    diagnostic(&outcome.value())
}
fn rich_legacy_frame(bytes: &[u8]) -> Result<V> {
    let text =
        core::str::from_utf8(bytes).map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
    let value = crate::property_catalog::parse_diagnostic_value(text)
        .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
    if diagnostic(&value)? != bytes {
        return Err(SourceContractReplayError::InvalidCheckpoint);
    }
    Ok(value)
}
fn rich_command_decode(
    policy: SourceContractRuntimePolicy,
    bytes: &[u8],
) -> Result<SourceContractCommand> {
    use crate::source_rich_value::{
        intrinsics,
        protocol::{InvokeRichV1, CALL_MAGIC},
        Limits,
    };
    let command = if bytes.starts_with(CALL_MAGIC) {
        SourceContractCommand::InvokeRichV1(
            InvokeRichV1::decode(
                rich_selected(policy),
                bytes,
                &intrinsics::capabilities(),
                Limits::default(),
            )
            .map_err(rich_protocol_error)?,
        )
    } else {
        SourceContractCommand::from_value(&rich_legacy_frame(bytes)?)?
    };
    if rich_command_frame(policy, &command)? != bytes {
        return Err(SourceContractReplayError::InvalidCheckpoint);
    }
    Ok(command)
}
fn rich_outcome_decode(
    policy: SourceContractRuntimePolicy,
    bytes: &[u8],
) -> Result<SourceContractCommandOutcome> {
    use crate::source_rich_value::{
        intrinsics,
        protocol::{RichDependencyV1, OUTCOME_MAGIC},
        Limits,
    };
    let outcome = if bytes.starts_with(OUTCOME_MAGIC) {
        SourceContractCommandOutcome::RichDependencyV1(
            RichDependencyV1::decode(
                rich_selected(policy),
                bytes,
                &intrinsics::capabilities(),
                Limits::default(),
            )
            .map_err(rich_protocol_error)?,
        )
    } else {
        SourceContractCommandOutcome::from_value(&rich_legacy_frame(bytes)?)?
    };
    if rich_outcome_frame(policy, &outcome)? != bytes {
        return Err(SourceContractReplayError::InvalidCheckpoint);
    }
    Ok(outcome)
}
fn rich_read_blob(c: &mut Cursor<'_>) -> Result<Vec<u8>> {
    let len = c
        .u32()
        .map_err(|_| SourceContractReplayError::InvalidCheckpoint)? as usize;
    if len > MAX_FRAME_BYTES {
        return Err(SourceContractReplayError::Capacity);
    }
    Ok(c.take(len)
        .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?
        .to_vec())
}
