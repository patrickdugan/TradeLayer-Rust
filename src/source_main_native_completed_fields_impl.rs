fn native_main_publication_fields(
    publication: &NativeMainCompletedPublicationV1,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    visitor(
        "publication/profile",
        &s(NATIVE_COMPLETED_PUBLICATION_PROFILE),
    )?;
    visitor("publication/installed", &V::Bool(publication.installed))?;
    visitor(
        "publication/startCommands",
        &s(&publication.start.head.commands.to_string()),
    )?;
    visitor(
        "publication/startHead",
        &s(&hex(&publication.start.head.commitment)),
    )?;
    visitor(
        "publication/startHistory",
        &s(&publication.start.history_len.to_string()),
    )?;
    visitor(
        "publication/sourceStartCommands",
        &s(&publication.start.source_head.commands.to_string()),
    )?;
    visitor(
        "publication/sourceStartHead",
        &s(&hex(&publication.start.source_head.commitment)),
    )?;
    crate::source_contract_runtime::native_main_row_visit_value(
        "publication/originalBefore",
        &physical_value(&publication.start.before),
        visitor,
    )?;
    visitor(
        "publication/startEffects",
        &s(&publication.start.effects.len().to_string()),
    )?;
    for (at, effect) in publication.start.effects.iter().enumerate() {
        crate::source_contract_runtime::native_main_row_visit_value(
            &format!("publication/startEffect/{at}"),
            effect,
            visitor,
        )?;
    }
    if let Some(prepared) = &publication.prepared {
        visitor(
            "publication/logicalBytes",
            &s(&prepared.logical.len().to_string()),
        )?;
        visitor(
            "publication/logicalSha256",
            &s(&hex(&sha256(&prepared.logical))),
        )?;
        visitor(
            "publication/receiptRetained",
            &V::Bool(prepared.receipt.is_some()),
        )?;
    }
    native_main_completed_barrier_fields(publication, visitor)?;
    Ok(())
}
