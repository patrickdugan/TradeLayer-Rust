// Readonly live visitor shape, generated from the same actual historical
// source metadata. These scalars authorize no append and are not writer tokens.
#[derive(Clone)]
pub(crate) struct HistoricalInlinePhysicalProjectionV1 {
    pub(crate) target_log_bytes: u64,
    pub(crate) retained_ancestors: usize,
    pub(crate) after: MetadataSnapshot,
    pub(crate) intermediate: Option<MetadataSnapshot>,
}
impl NativeHistoricalInlineDatabaseV1 {
    pub(crate) fn visit_live_projection(
        &self,
        facts: Option<&HistoricalInlinePhysicalProjectionV1>,
        visitor: &mut dyn FnMut(&str, &V),
    ) -> Result<()> {
        visitor("profile", &prepared_text(INLINE_DATABASE_PROFILE));
        visitor(
            "context",
            &prepared_text(&self.original.storage.persistent_identity),
        );
        visitor(
            "canonicalFile",
            &prepared_text(self.original.storage.canonical_catalog.to_string_lossy()),
        );
        visitor(
            "expected",
            &prepared_snapshot_value(&self.original.metadata),
        );
        visitor("method", &prepared_text(&self.method));
        visitor("arguments", &V::Array(self.arguments.clone()));
        visitor("before", &prepared_state_value(&self.before_state));
        visitor("after", &prepared_state_value(&self.after_state));
        visitor("write", &V::Bool(self.requires_native_write()));
        if self.requires_native_write() {
            let facts = facts.ok_or(CompactIndexError::WrongContext)?;
            visitor(
                "directory",
                &prepared_text(prepared_hex(&self.directory.encode()?)),
            );
            visitor(
                "append/targetBytes",
                &prepared_text(facts.target_log_bytes.to_string()),
            );
            visitor(
                "append/retainedAncestors",
                &prepared_text(facts.retained_ancestors.to_string()),
            );
            for (name, pin) in [
                ("before", Some(self.original.metadata.clone())),
                ("after", Some(facts.after.clone())),
                ("intermediate", facts.intermediate.clone()),
            ] {
                visitor(&format!("append/{name}"), &V::Bool(pin.is_some()));
                if let Some(pin) = pin {
                    let snapshot = pin;
                    visitor(
                        &format!("append/{name}/pin"),
                        &prepared_snapshot_value(&snapshot),
                    );
                }
            }
            visitor(
                "mutations/length",
                &prepared_text(self.mutations.len().to_string()),
            );
            for (i, mutation) in self.mutations.iter().enumerate() {
                let (kind, key, value) = match mutation {
                    Mutation::Put { key, value } => ("Put", key, Some(value)),
                    Mutation::Delete { key } => ("Delete", key, None),
                };
                visitor(&format!("mutations/{i}/kind"), &prepared_text(kind));
                visitor(&format!("mutations/{i}/key"), &prepared_text(key));
                if let Some(value) = value {
                    visitor(
                        &format!("mutations/{i}/value"),
                        &prepared_text(prepared_hex(value)),
                    );
                }
            }
        }
        let (kind, value, error) = match &self.returned {
            DependencyReturn::Pending(v) => ("Pending", Some(v), None),
            DependencyReturn::Immediate(v) => ("Immediate", Some(v), None),
            DependencyReturn::Throw(e) => ("Throw", None, Some(e)),
            DependencyReturn::PendingThrow(e) => ("PendingThrow", None, Some(e)),
        };
        visitor("returned/kind", &prepared_text(kind));
        if let Some(v) = value {
            visitor("returned/value", v);
        }
        if let Some(e) = error {
            visitor("returned/name", &prepared_text(&e.name));
            visitor("returned/message", &prepared_text(&e.message));
        }
        Ok(())
    }
}
