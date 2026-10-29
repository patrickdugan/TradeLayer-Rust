// Actual source DB await preparation. This is an explicit new entry; the old
// database()/put() bodies, bytes and synchronous API remain unchanged.
pub const INLINE_DATABASE_PROFILE: &str = "TL-NATIVE-INLINE-DATABASE-AWAIT-13-1";
pub struct NativePreparedInlineDatabaseV1 {
    context: (String, std::path::PathBuf),
    expected: MetadataSnapshot,
    method: String,
    arguments: Vec<V>,
    before_state: NativeMetadataStateReceiptV1,
    after_state: NativeMetadataStateReceiptV1,
    put: Option<NativeMetadataPutPlanV1>,
    returned: DependencyReturn,
    before_directory: Directory,
    recovery_before: crate::compact_journal::VerifiedCompactPrefixV1,
}
impl NativePreparedInlineDatabaseV1 {
    pub fn method(&self) -> &str {
        &self.method
    }
    pub fn arguments(&self) -> &[V] {
        &self.arguments
    }
    pub fn expected(&self) -> &MetadataSnapshot {
        &self.expected
    }
    pub fn append_plan(&self) -> Option<&crate::compact_journal::NativeAppendPlan> {
        self.put.as_ref().map(|p| &p.append)
    }
    pub fn before_state(&self) -> &NativeMetadataStateReceiptV1 {
        &self.before_state
    }
    pub fn after_state(&self) -> &NativeMetadataStateReceiptV1 {
        &self.after_state
    }
    pub fn durable_read_context(&self) -> V {
        prepared_obj(vec![
            ("profile", prepared_text(INLINE_DATABASE_PROFILE)),
            ("context", prepared_text(&self.context.0)),
            (
                "canonicalFile",
                prepared_text(self.context.1.to_string_lossy()),
            ),
            ("expected", prepared_snapshot_value(&self.expected)),
            ("method", prepared_text(&self.method)),
            ("arguments", V::Array(self.arguments.clone())),
            ("before", prepared_state_value(&self.before_state)),
            ("after", prepared_state_value(&self.after_state)),
        ])
    }
    pub fn visit_canonical_fields(&self, visitor: &mut dyn FnMut(&str, &V)) -> Result<()> {
        visitor("profile", &prepared_text(INLINE_DATABASE_PROFILE));
        visitor("context", &prepared_text(&self.context.0));
        visitor(
            "canonicalFile",
            &prepared_text(self.context.1.to_string_lossy()),
        );
        visitor("expected", &prepared_snapshot_value(&self.expected));
        visitor("method", &prepared_text(&self.method));
        visitor("arguments", &V::Array(self.arguments.clone()));
        visitor("before", &prepared_state_value(&self.before_state));
        visitor("after", &prepared_state_value(&self.after_state));
        visitor("write", &V::Bool(self.put.is_some()));
        if let Some(put) = &self.put {
            visitor(
                "directory",
                &prepared_text(prepared_hex(&put.directory.encode()?)),
            );
            visitor(
                "append/targetBytes",
                &prepared_text(put.append.configuration().target_log_bytes.to_string()),
            );
            visitor(
                "append/retainedAncestors",
                &prepared_text(put.append.configuration().retained_ancestors.to_string()),
            );
            for (name, pin) in [
                ("before", Some(put.append.before().clone())),
                ("after", Some(put.append.after().clone())),
                ("intermediate", put.append.intermediate()),
            ] {
                visitor(&format!("append/{name}"), &V::Bool(pin.is_some()));
                if let Some(pin) = pin {
                    let mut snapshot = self.expected.clone();
                    snapshot.sequence = pin.logical_sequence;
                    snapshot.publication = pin.publication;
                    visitor(
                        &format!("append/{name}/pin"),
                        &prepared_snapshot_value(&snapshot),
                    );
                }
            }
            visitor(
                "mutations/length",
                &prepared_text(put.append.mutations().len().to_string()),
            );
            for (i, mutation) in put.append.mutations().iter().enumerate() {
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
impl SourceCompactIndex {
    pub fn prepare_inline_database(
        &self,
        expected: &MetadataSnapshot,
        method: &str,
        arguments: Vec<V>,
    ) -> Result<NativePreparedInlineDatabaseV1> {
        self.require_snapshot(expected)?;
        if arguments.len() > 3 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        for value in &arguments {
            value.validate()?;
        }
        let snapshot = self.read_generation(expected)?;
        let before_state = prepared_metadata_state(&snapshot.data, &[])?;
        let (row_to_put, returned) =
            inline_database_source_outcome(method, &arguments, |id| self.get(id))?;
        let put = row_to_put
            .map(|row| self.native_prepare_metadata_put(expected, row))
            .transpose()?;
        let after_state = if let Some(put) = &put {
            prepared_metadata_state(&snapshot.data, put.append.mutations())?
        } else {
            before_state.clone()
        };
        Ok(NativePreparedInlineDatabaseV1 {
            context: self.native_store_context()?,
            before_directory: self.directory.clone(),
            recovery_before: self.journal.verified_prefix_pin()?,
            expected: expected.clone(),
            method: method.into(),
            arguments,
            before_state,
            after_state,
            put,
            returned,
        })
    }
    /// Writes real native bytes but deliberately does not resume the source PC.
    /// Any lost native return leaves that same PC/plan pending for verified recovery.
    pub fn append_inline_database(
        &mut self,
        plan: &NativePreparedInlineDatabaseV1,
        permit: Option<&SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<()> {
        self.require_live_metadata_write()?;
        self.require_snapshot(&plan.expected)?;
        if self.native_store_context()? != plan.context
            || self.native_complete_metadata_state()? != plan.before_state
        {
            return Err(CompactIndexError::WrongContext);
        }
        if let Some(put) = &plan.put {
            self.journal.commit_prepared_native_append_with_escrow(
                &put.append,
                permit.map(|p| &p.storage),
            )?;
            self.directory = put.directory.clone();
        } else if permit.is_some() {
            return Err(CompactIndexError::WrongContext);
        }
        if self.native_complete_metadata_state()? != plan.after_state {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(())
    }
    /// Read-only completion proof; no second append and no source execution.
    pub fn resolve_inline_database_post(
        &self,
        plan: &NativePreparedInlineDatabaseV1,
    ) -> Result<DependencyReturn> {
        let current = self.snapshot()?;
        if self.native_store_context()? != plan.context
            || self.native_complete_metadata_state()? != plan.after_state
        {
            return Err(CompactIndexError::WrongContext);
        }
        if let Some(put) = &plan.put {
            if current.context != plan.expected.context
                || current.profile != plan.expected.profile
                || current.sequence != put.append.after().logical_sequence
                || current.publication != put.append.after().publication
                || self.directory.encode()? != put.directory.encode()?
            {
                return Err(CompactIndexError::WrongContext);
            }
        } else if current != plan.expected {
            return Err(CompactIndexError::WrongContext);
        }
        Ok(plan.returned.clone())
    }
}
