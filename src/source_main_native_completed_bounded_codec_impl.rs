fn native_main_completed_logical_bytes(
    receipt: &NativeMainSessionReceipt,
    maximum: usize,
) -> Result<Vec<u8>, NativeMainSessionError> {
    let mut bytes = NativeCompletedBytes::new(maximum);
    bytes.extend_from_slice(RECEIPT_PROFILE.as_bytes());
    bytes.extend_from_slice(&receipt.sequence.to_be_bytes());
    native_completed_frame(&mut bytes, &receipt.command.value())?;
    native_completed_frame(&mut bytes, &receipt.status.value())?;
    native_completed_frame(
        &mut bytes,
        &receipt.grouping.as_ref().map_or(V::Null, logical_read),
    )?;
    bytes.extend_from_slice(&(receipt.validity.len() as u64).to_be_bytes());
    for evaluation in &receipt.validity {
        let query = &evaluation.query;
        let update = evaluation.update.as_ref().map_or(V::Null, |observation| {
            obj(&[
                ("target", s(&observation.call.target)),
                ("arguments", V::Array(observation.call.arguments.clone())),
                (
                    "returned",
                    SourceContractCommandOutcome::Dependency(observation.returned.clone()).value(),
                ),
            ])
        });
        let outcome = match &evaluation.outcome {
            crate::validity_source::SourceOutcome::Returned(v) => obj(&[("returned", v.clone())]),
            crate::validity_source::SourceOutcome::Thrown(e) => {
                obj(&[("name", s(&e.name)), ("message", s(&e.message))])
            }
        };
        native_completed_frame(
            &mut bytes,
            &obj(&[
                ("profile", s(&evaluation.profile)),
                ("predicate", s(&query.predicate)),
                ("visited", s(&query.visited_rows.to_string())),
                ("matched", s(&query.matched_rows.to_string())),
                ("chain", s(&hex(&query.chain_sha256d))),
                ("complete", V::Bool(query.complete)),
                ("update", update),
                ("outcome", outcome),
            ]),
        )?;
    }
    bytes.extend_from_slice(&(receipt.effects.len() as u64).to_be_bytes());
    for effect in &receipt.effects {
        native_completed_frame(&mut bytes, effect)?;
    }
    bytes.extend_from_slice(&receipt.financial_commitment);
    native_completed_frame(
        &mut bytes,
        &receipt.after_rows.as_ref().map_or(V::Null, logical_read),
    )?;
    bytes.finish()
}
