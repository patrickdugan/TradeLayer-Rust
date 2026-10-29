// Literal admitted insert/update source outcome, shared by live and historical reads.
fn inline_database_source_outcome(
    method: &str,
    arguments: &[V],
    mut lookup: impl FnMut(&str) -> Result<Option<V>>,
) -> Result<(Option<V>, DependencyReturn)> {
    let query = arguments.first().cloned().unwrap_or(V::Undefined);
    let mut row_to_put = None;
    let returned = match method {
        "insertAsync" => {
            profile_value(&query)?;
            match memory_copy(&query) {
                Err(e) => DependencyReturn::PendingThrow(e),
                Ok(row) => {
                    let id = row_id(&row)?;
                    if lookup(&id)?.is_some() {
                        source_error(format!(
                            "Can't insert key {id}, it violates the unique constraint"
                        ))
                    } else {
                        row_to_put = Some(row.clone());
                        DependencyReturn::Pending(row)
                    }
                }
            }
        }
        "updateAsync" => {
            let Query::Id(id) = Query::parse(&query)? else {
                return Err(SourceRuleError::UnknownDependency.into());
            };
            let old = lookup(&id)?;
            let upsert = arguments
                .get(2)
                .is_some_and(|v| v.get("upsert") == V::Bool(true));
            if old.is_none() && !upsert {
                DependencyReturn::Pending(V::number(0.))
            } else {
                let update = arguments.get(1).ok_or(SourceRuleError::InvalidInput)?;
                let mut row =
                    old.unwrap_or_else(|| V::Object(vec![("_id".into(), V::String(id.clone()))]));
                if let V::Object(fields) = update {
                    if fields.len() == 1 && fields[0].0 == "$set" {
                        let V::Object(sets) = &fields[0].1 else {
                            return Err(SourceRuleError::UnknownDependency.into());
                        };
                        for (k, v) in sets {
                            if k == "_id" || k.starts_with('$') || k.contains('.') {
                                return Err(SourceRuleError::UnknownDependency.into());
                            }
                            row.insert(k, v.clone())?;
                        }
                    } else if fields.iter().all(|(k, _)| !k.starts_with('$')) {
                        row = update.clone();
                    } else {
                        return Err(SourceRuleError::UnknownDependency.into());
                    }
                } else {
                    return Err(SourceRuleError::InvalidInput.into());
                }
                if row.get("_id") == V::Undefined {
                    row.insert("_id", V::String(id.clone()))?;
                }
                if row_id(&row)? != id {
                    source_error("You cannot change a document's _id")
                } else {
                    profile_value(&row)?;
                    match memory_copy(&row) {
                        Err(e) => DependencyReturn::PendingThrow(e),
                        Ok(row) => {
                            row_to_put = Some(row);
                            DependencyReturn::Pending(V::number(1.))
                        }
                    }
                }
            }
        }
        _ => return Err(SourceRuleError::UnknownDependency.into()),
    };
    Ok((row_to_put, returned))
}
