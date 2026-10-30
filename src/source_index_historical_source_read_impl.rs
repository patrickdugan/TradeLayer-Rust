impl MainIndexReadViewV1 {
    /// Bounded source reads from the actual independently checked historical
    /// Directory/page bytes. This grants no mutable index or write permit.
    fn source_database_read_from_view(&self, method: &str, args: &[V]) -> Result<DependencyReturn> {
        let one = match method {
            "findOneAsync" => true,
            "findAsync" => false,
            _ => return Err(CompactIndexError::ColdWriteRefused),
        };
        let query = args.first().cloned().unwrap_or(V::Undefined);
        let matcher = Query::parse(&query)?;
        if let Query::Id(id) = &matcher {
            valid_id(id)?;
            let page = self
                .directory()
                .pages
                .partition_point(|p| p.first.as_str() <= id);
            let row = if page == 0 {
                None
            } else {
                self.page(page - 1)?
                    .into_iter()
                    .find(|v| v.get("_id") == V::String(id.clone()))
            };
            return Ok(DependencyReturn::Pending(if one {
                row.unwrap_or(V::Null)
            } else {
                V::Array(row.into_iter().collect())
            }));
        }
        let mut cursor = self.cursor();
        let mut rows = vec![];
        while let Some(row) = cursor.next_row()? {
            if matcher.matches(&row_id(&row)?) {
                if rows.len() >= 1024 {
                    return Err(CompactIndexError::Limit);
                }
                rows.push(row);
                if one {
                    break;
                }
            }
        }
        let value = if one {
            rows.into_iter().next().unwrap_or(V::Null)
        } else {
            V::Array(rows)
        };
        value.validate()?;
        Ok(DependencyReturn::Pending(value))
    }
}
impl NativeHistoricalValidityPlaybackV1<'_> {
    pub(crate) fn source_actual_rows(&self) -> Result<RowStreamReceipt> {
        let mut cursor = self.current.cursor();
        while cursor.next_row()?.is_some() {}
        Ok(cursor.receipt())
    }
    pub(crate) fn source_read_view(&self) -> &MainIndexReadViewV1 {
        &self.current
    }
    pub(crate) fn source_history_pin(&self) -> &SourceIndexHistoryPinV1 {
        &self.boundaries[self.next]
    }
    pub(crate) fn scoped_cold_index(
        &mut self,
        capability: &NativeColdIndexExecutionV1,
    ) -> Result<&mut SourceCompactIndex> {
        if self.index.cold_execution != Some(capability.scope) {
            return Err(CompactIndexError::ColdWriteRefused);
        }
        Ok(self.index)
    }
}

impl MainIndexReadViewV1 {
    pub(crate) fn source_database_read(
        &self,
        method: &str,
        args: &[V],
    ) -> Result<DependencyReturn> {
        match &self.data {
            MainIndexReadDataV1::Original(data) => data.source_database_read(method, args),
            MainIndexReadDataV1::Verified(_) => self.source_database_read_from_view(method, args),
        }
    }
}
impl SourceIndexReadSnapshot {
    pub(crate) fn source_database_read(
        &self,
        method: &str,
        args: &[V],
    ) -> Result<DependencyReturn> {
        MainIndexReadViewV1::from_original(self.clone())
            .source_database_read_from_view(method, args)
    }
}
