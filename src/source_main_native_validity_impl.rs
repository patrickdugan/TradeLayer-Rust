// Derived source validity query/update with one actual IO-fact hook.
impl SourceCompactIndex {
    pub(crate) fn native_stream_upsert_validity(
        &mut self,
        expected: &MetadataSnapshot,
        txid: V,
        ty: V,
        valid: V,
        reason: V,
        pre_update: &mut impl FnMut(&[V]) -> Result<Option<DependencyReturn>>,
    ) -> Result<StreamedValidityEvaluation> {
        for v in [&txid, &ty, &valid, &reason] {
            v.validate()?;
        }
        self.require_snapshot(expected)?;
        let snapshot = self.read_generation(expected)?;
        let mut cursor = snapshot.cursor();
        let mut matched = 0u64;
        let mut chain = crate::hash256::double_sha256(VALIDITY_STREAM_PROFILE.as_bytes());
        while let Some(row) = cursor.next_row()? {
            if row_id(&row)?.starts_with("tx") {
                let body = row.diagnostic_json()?;
                let mut bytes = chain.to_vec();
                bytes.extend_from_slice(&matched.to_be_bytes());
                bytes.extend_from_slice(&(body.len() as u32).to_be_bytes());
                bytes.extend_from_slice(body.as_bytes());
                chain = crate::hash256::double_sha256(&bytes);
                matched += 1;
            }
        }
        let read = cursor.receipt();
        let query = PrefixQueryReceipt {
            profile: VALIDITY_STREAM_PROFILE.into(),
            predicate: "^tx".into(),
            snapshot: expected.clone(),
            visited_rows: read.rows,
            matched_rows: matched,
            chain_sha256d: chain,
            complete: read.complete,
        };
        coercion_domain(&txid)?;
        let suffix = match source_text(&txid) {
            Ok(t) => format!("-{t}"),
            Err(e) => {
                return Ok(StreamedValidityEvaluation {
                    profile: VALIDITY_STREAM_PROFILE.into(),
                    query,
                    outcome: crate::validity_source::SourceOutcome::Thrown(e),
                    update: None,
                    post_state: self.snapshot()?,
                })
            }
        };
        let mut find = snapshot.cursor();
        let mut selected = None;
        while let Some(row) = find.next_row()? {
            let id = row_id(&row)?;
            if id.starts_with("tx") && id.ends_with(&suffix) {
                selected = Some(id);
                break;
            }
        }
        let Some(id) = selected else {
            return Ok(StreamedValidityEvaluation {
                profile: VALIDITY_STREAM_PROFILE.into(),
                query,
                outcome: crate::validity_source::SourceOutcome::Returned(V::Undefined),
                update: None,
                post_state: self.snapshot()?,
            });
        };
        let arguments = vec![
            V::Object(vec![("_id".into(), V::String(id))]),
            V::Object(vec![(
                "$set".into(),
                V::Object(vec![
                    ("type".into(), ty),
                    ("valid".into(), valid),
                    ("reason".into(), reason),
                ]),
            )]),
            V::Object(vec![("upsert".into(), V::Bool(true))]),
        ];
        let returned = match pre_update(&arguments)? {
            Some(returned) => returned,
            None => self.database("updateAsync", &arguments)?,
        };
        let outcome = match &returned {
            DependencyReturn::Pending(_) | DependencyReturn::Immediate(_) => {
                crate::validity_source::SourceOutcome::Returned(V::Undefined)
            }
            DependencyReturn::PendingThrow(e) | DependencyReturn::Throw(e) => {
                crate::validity_source::SourceOutcome::Thrown(e.clone())
            }
        };
        let mut call_args = vec![V::Object(vec![(
            "$database".into(),
            V::String("txIndex".into()),
        )])];
        call_args.extend(arguments);
        Ok(StreamedValidityEvaluation {
            profile: VALIDITY_STREAM_PROFILE.into(),
            query,
            outcome,
            update: Some(crate::validity_source::DependencyObservation {
                call: DependencyCall {
                    target: "db.updateAsync".into(),
                    arguments: call_args,
                },
                returned,
            }),
            post_state: self.snapshot()?,
        })
    }
}
