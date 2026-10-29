// Actual source put page/directory calculation; no IO until commit.
impl SourceCompactIndex {
    fn native_prepare_metadata_put(
        &self,
        expected: &MetadataSnapshot,
        row: V,
    ) -> Result<NativeMetadataPutPlanV1> {
        self.require_snapshot(expected)?;
        let (directory, mutations) =
            prepared_metadata_mutations(&self.directory, row, |_, page| self.page(page))?;
        let append = self.journal.prepare_native_append(mutations)?;
        Ok(NativeMetadataPutPlanV1 {
            expected: expected.clone(),
            directory,
            append,
        })
    }
}

fn prepared_metadata_mutations(
    source_directory: &Directory,
    row: V,
    read_page: impl FnOnce(usize, &Page) -> Result<Vec<V>>,
) -> Result<(Directory, Vec<Mutation>)> {
    let id = row_id(&row)?;
    let mut directory = source_directory.clone();
    let i = directory
        .pages
        .partition_point(|p| p.first <= id)
        .saturating_sub(1);
    let mut page = if directory.pages.is_empty() {
        vec![]
    } else {
        read_page(i, &directory.pages[i])?
    };
    let insertion = page.partition_point(|v| row_id(v).is_ok_and(|k| k < id));
    if page
        .get(insertion)
        .is_some_and(|v| v.get("_id") == V::String(id.clone()))
    {
        page[insertion] = row;
    } else {
        page.insert(insertion, row);
        directory.rows += 1;
    }
    let mut mutations = vec![];
    if directory.pages.is_empty() {
        let n = directory.next_page;
        directory.next_page = directory
            .next_page
            .checked_add(1)
            .ok_or(CompactIndexError::Limit)?;
        directory.pages.push(Page {
            number: n,
            first: id,
        });
        mutations.push(Mutation::Put {
            key: page_key(n),
            value: encode_page(&page)?,
        });
    } else if page.len() <= PAGE_ROWS {
        directory.pages[i].first = row_id(&page[0])?;
        mutations.push(Mutation::Put {
            key: page_key(directory.pages[i].number),
            value: encode_page(&page)?,
        });
    } else {
        if directory.pages.len() >= MAX_PAGES {
            return Err(CompactIndexError::Limit);
        }
        let right = page.split_off(page.len() / 2);
        let n = directory.next_page;
        directory.next_page = directory
            .next_page
            .checked_add(1)
            .ok_or(CompactIndexError::Limit)?;
        directory.pages[i].first = row_id(&page[0])?;
        mutations.push(Mutation::Put {
            key: page_key(directory.pages[i].number),
            value: encode_page(&page)?,
        });
        mutations.push(Mutation::Put {
            key: page_key(n),
            value: encode_page(&right)?,
        });
        directory.pages.insert(
            i + 1,
            Page {
                number: n,
                first: row_id(&right[0])?,
            },
        );
    }
    mutations.push(Mutation::Put {
        key: DIRECTORY.into(),
        value: directory.encode()?,
    });
    Ok((directory, mutations))
}
